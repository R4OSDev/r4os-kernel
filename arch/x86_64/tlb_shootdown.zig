const owner_locks = @import("../../memory/owner_locks.zig");
const percpu = @import("percpu.zig");
const policy = @import("tlb_policy.zig");

pub const VECTOR: u8 = 0xF2;
const PAGE_SIZE: u64 = 4096;
const FULL_RELOAD_THRESHOLD_PAGES: u64 = 64;
const NORMAL_SPIN_LIMIT: u64 = 25_000_000;
const DIAGNOSTIC_SPIN_LIMIT: u64 = 250_000;

extern fn r4os_read_cr3() callconv(.c) u64;
extern fn r4os_write_cr3(value: u64) callconv(.c) void;
extern fn r4os_invlpg(addr: usize) callconv(.c) void;

pub const SendFn = *const fn (cpu_index: u32, vector: u8) bool;

pub const Result = struct {
    ok: bool = false,
    generation: u64 = 0,
    target_mask: u64 = 0,
    acknowledgement_mask: u64 = 0,
    offline_mask: u64 = 0,
    spins: u64 = 0,
};

pub const Stats = struct {
    requests: u64 = 0,
    successes: u64 = 0,
    timeouts: u64 = 0,
    expected_timeouts: u64 = 0,
    ipis_sent: u64 = 0,
    acknowledgements: u64 = 0,
    offline_exclusions: u64 = 0,
    local_page_flushes: u64 = 0,
    local_full_reloads: u64 = 0,
    max_wait_spins: u64 = 0,
    generation: u64 = 0,
};

var send_ipi: ?SendFn = null;
var generation: u64 = 0;
var request_generation: u64 = 0;
var request_base: u64 = 0;
var request_page_count: u64 = 0;
var request_target_mask: u64 = 0;
var acknowledgement_generation: [percpu.max_cpus]u64 = .{0} ** percpu.max_cpus;
var diagnostic_drop_ack_mask: u64 = 0;
var diagnostic_request_armed: bool = false;

var requests: u64 = 0;
var successes: u64 = 0;
var timeouts: u64 = 0;
var expected_timeouts: u64 = 0;
var ipis_sent: u64 = 0;
var acknowledgements: u64 = 0;
var offline_exclusions: u64 = 0;
var local_page_flushes: u64 = 0;
var local_full_reloads: u64 = 0;
var max_wait_spins: u64 = 0;

pub fn registerSender(sender: SendFn) void {
    send_ipi = sender;
}

pub fn invalidateRange(base: u64, page_count: u64) Result {
    if (page_count == 0 or (base & (PAGE_SIZE - 1)) != 0) return .{};
    if (!owner_locks.page_tables.heldByCurrent()) return .{};

    _ = @atomicRmw(u64, &requests, .Add, 1, .monotonic);
    flushLocal(base, page_count);

    const current_cpu = percpu.currentIndex();
    const target_mask = policy.targetMask(percpu.onlineMask(), current_cpu);
    generation +%= 1;
    if (generation == 0) generation = 1;
    const current_generation = generation;
    @atomicStore(u64, &request_base, base, .release);
    @atomicStore(u64, &request_page_count, page_count, .release);
    @atomicStore(u64, &request_target_mask, target_mask, .release);
    @atomicStore(u64, &request_generation, current_generation, .release);

    if (target_mask == 0) {
        _ = @atomicRmw(u64, &successes, .Add, 1, .monotonic);
        return .{ .ok = true, .generation = current_generation };
    }

    const sender = send_ipi orelse {
        _ = @atomicRmw(u64, &timeouts, .Add, 1, .monotonic);
        return .{ .generation = current_generation, .target_mask = target_mask };
    };
    var cpu_index: u32 = 0;
    while (cpu_index < percpu.max_cpus) : (cpu_index += 1) {
        const bit = @as(u64, 1) << @intCast(cpu_index);
        if ((target_mask & bit) == 0) continue;
        if (sender(cpu_index, VECTOR)) {
            _ = @atomicRmw(u64, &ipis_sent, .Add, 1, .monotonic);
        } else if (percpu.state(cpu_index) != .online) {
            _ = @atomicRmw(u64, &offline_exclusions, .Add, 1, .monotonic);
        }
    }

    const diagnostic = @atomicLoad(bool, &diagnostic_request_armed, .acquire);
    const spin_limit = if (diagnostic) DIAGNOSTIC_SPIN_LIMIT else NORMAL_SPIN_LIMIT;
    var spins: u64 = 0;
    var observed_offline: u64 = 0;
    while (spins < spin_limit) : (spins += 1) {
        const acknowledged = acknowledgementMaskForGeneration(target_mask, current_generation);
        const offline = policy.offlineSinceSnapshot(target_mask, percpu.onlineMask());
        if (offline != 0) {
            const newly_offline = offline & ~observed_offline;
            if (newly_offline != 0) {
                _ = @atomicRmw(u64, &offline_exclusions, .Add, @as(u64, @intCast(@popCount(newly_offline))), .monotonic);
                observed_offline |= newly_offline;
            }
        }
        if (policy.completed(target_mask, acknowledged | offline)) {
            _ = @atomicRmw(u64, &successes, .Add, 1, .monotonic);
            _ = @atomicRmw(u64, &max_wait_spins, .Max, spins, .monotonic);
            return .{
                .ok = true,
                .generation = current_generation,
                .target_mask = target_mask,
                .acknowledgement_mask = acknowledged | offline,
                .offline_mask = offline,
                .spins = spins,
            };
        }
        asm volatile ("pause");
    }

    const acknowledged = acknowledgementMaskForGeneration(target_mask, current_generation);
    const offline = policy.offlineSinceSnapshot(target_mask, percpu.onlineMask());
    _ = @atomicRmw(u64, &timeouts, .Add, 1, .monotonic);
    if (diagnostic) _ = @atomicRmw(u64, &expected_timeouts, .Add, 1, .monotonic);
    _ = @atomicRmw(u64, &max_wait_spins, .Max, spins, .monotonic);
    return .{
        .generation = current_generation,
        .target_mask = target_mask,
        .acknowledgement_mask = acknowledged | offline,
        .offline_mask = offline,
        .spins = spins,
    };
}

pub fn handleIpi() void {
    const current_generation = @atomicLoad(u64, &request_generation, .acquire);
    if (current_generation == 0) return;
    const cpu_index = percpu.currentIndex();
    const bit = @as(u64, 1) << @intCast(cpu_index);
    if ((@atomicLoad(u64, &request_target_mask, .acquire) & bit) == 0) return;
    const base = @atomicLoad(u64, &request_base, .acquire);
    const page_count = @atomicLoad(u64, &request_page_count, .acquire);
    flushLocal(base, page_count);

    const drop_mask = @atomicLoad(u64, &diagnostic_drop_ack_mask, .acquire);
    if ((drop_mask & bit) != 0) {
        _ = @atomicRmw(u64, &diagnostic_drop_ack_mask, .And, ~bit, .acq_rel);
        return;
    }
    @atomicStore(u64, &acknowledgement_generation[@intCast(cpu_index)], current_generation, .release);
    _ = @atomicRmw(u64, &acknowledgements, .Add, 1, .monotonic);
}

pub fn armDiagnosticMissingAck(cpu_index: u32) bool {
    if (cpu_index >= percpu.max_cpus or cpu_index == percpu.currentIndex() or percpu.state(cpu_index) != .online) {
        return false;
    }
    const bit = @as(u64, 1) << @intCast(cpu_index);
    @atomicStore(u64, &diagnostic_drop_ack_mask, bit, .release);
    @atomicStore(bool, &diagnostic_request_armed, true, .release);
    return true;
}

pub fn disarmDiagnosticMissingAck() void {
    @atomicStore(bool, &diagnostic_request_armed, false, .release);
    @atomicStore(u64, &diagnostic_drop_ack_mask, 0, .release);
}

pub fn stats() Stats {
    return .{
        .requests = @atomicLoad(u64, &requests, .monotonic),
        .successes = @atomicLoad(u64, &successes, .monotonic),
        .timeouts = @atomicLoad(u64, &timeouts, .monotonic),
        .expected_timeouts = @atomicLoad(u64, &expected_timeouts, .monotonic),
        .ipis_sent = @atomicLoad(u64, &ipis_sent, .monotonic),
        .acknowledgements = @atomicLoad(u64, &acknowledgements, .monotonic),
        .offline_exclusions = @atomicLoad(u64, &offline_exclusions, .monotonic),
        .local_page_flushes = @atomicLoad(u64, &local_page_flushes, .monotonic),
        .local_full_reloads = @atomicLoad(u64, &local_full_reloads, .monotonic),
        .max_wait_spins = @atomicLoad(u64, &max_wait_spins, .monotonic),
        .generation = @atomicLoad(u64, &request_generation, .acquire),
    };
}

fn flushLocal(base: u64, page_count: u64) void {
    if (page_count >= FULL_RELOAD_THRESHOLD_PAGES) {
        r4os_write_cr3(r4os_read_cr3());
        _ = @atomicRmw(u64, &local_full_reloads, .Add, 1, .monotonic);
        return;
    }
    var page: u64 = 0;
    while (page < page_count) : (page += 1) {
        r4os_invlpg(@intCast(base + page * PAGE_SIZE));
    }
    _ = @atomicRmw(u64, &local_page_flushes, .Add, page_count, .monotonic);
}

fn acknowledgementMaskForGeneration(target_mask: u64, wanted_generation: u64) u64 {
    var mask: u64 = 0;
    var cpu_index: u32 = 0;
    while (cpu_index < percpu.max_cpus) : (cpu_index += 1) {
        const bit = @as(u64, 1) << @intCast(cpu_index);
        if ((target_mask & bit) == 0) continue;
        if (@atomicLoad(u64, &acknowledgement_generation[@intCast(cpu_index)], .acquire) == wanted_generation) {
            mask |= bit;
        }
    }
    return mask;
}
