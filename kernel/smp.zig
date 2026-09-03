// x86_64 SMP bring-up and lifecycle owner.
//
// APs are started sequentially through private low-memory INIT/SIPI
// trampolines.  They publish `parked` only after CPU-local GDT/TSS/GS, IDT,
// FPU and scheduler state are ready.  The BSP never waits without a deadline,
// and failed APs are excluded from the schedulable mask permanently.

const std = @import("std");
const acpi = @import("../platform/acpi.zig");
const blocks = @import("../memory/blocks.zig");
const boot_config = @import("boot_config.zig");
const config = @import("config");
const fpu = @import("../arch/x86_64/fpu.zig");
const gdt = @import("../arch/x86_64/gdt.zig");
const com = @import("../driver/com.zig");
const idt = @import("../arch/x86_64/idt.zig");
const interrupts = @import("../arch/x86_64/interrupts.zig");
const lapic = @import("../arch/x86_64/lapic.zig");
const msr = @import("../arch/x86_64/msr.zig");
const heap = @import("../memory/heap.zig");
const owner_locks = @import("../memory/owner_locks.zig");
const paging = @import("../memory/paging.zig");
const percpu = @import("../arch/x86_64/percpu.zig");
const phys = @import("../memory/phys.zig");
const tlb = @import("../arch/x86_64/tlb_shootdown.zig");
const platform_cpu = @import("../platform/cpu.zig");
const monotonic = @import("../platform/monotonic.zig");
const scheduler = @import("../sched/scheduler.zig");
const task = @import("../sched/task.zig");
const r4x = @import("../program/r4x.zig");
const timer = @import("timer.zig");
const virt = @import("../memory/virt.zig");
const policy = @import("smp_policy.zig");
const k = @import("log.zig");

const PAGE_SIZE: usize = 4096;
const CONFIG_OFFSET: usize = 0x800;
const GDT_OFFSET: usize = 0x900;
const AP_STACK_SIZE: usize = 64 * 1024;
const IA32_EFER: u32 = 0xC000_0080;
const EFER_LME: u64 = 1 << 8;
const EFER_LMA: u64 = 1 << 10;
const AP_START_TIMEOUT_NS: u64 = 250_000_000;
const AP_STOP_TIMEOUT_NS: u64 = 100_000_000;
const ACCEPTANCE_TIMEOUT_NS: u64 = 10_000_000_000;
const ACCEPTANCE_ITERATIONS: u64 = 4_000_000;
const ACCEPTANCE_MIN_SPEEDUP_MILLI: u64 = 1050;
const OWNER_STRESS_ITERATIONS: u64 = 64;
const TLB_PROBE_OLD: u64 = 0x544C_422D_4F4C_4421;
const TLB_PROBE_NEW: u64 = 0x544C_422D_4E45_5721;
const TRANSITION_CODE_SELECTOR: u16 = 0x08;
const TRANSITION_PROTECTED_SELECTOR: u16 = 0x18;

extern const r4os_ap_trampoline_start: u8;
extern const r4os_ap_trampoline_protected_mode: u8;
extern const r4os_ap_trampoline_long_mode: u8;
extern const r4os_ap_trampoline_end: u8;
extern fn r4os_read_cr0() callconv(.c) u64;
extern fn r4os_read_cr3() callconv(.c) u64;
extern fn r4os_read_cr4() callconv(.c) u64;

const TrampolineConfig = packed struct {
    magic: u64,
    cr3: u64,
    cr0: u64,
    cr4: u64,
    efer: u64,
    entry: u64,
    stack: u64,
    cpu_index: u64,
    far_offset: u32,
    far_selector: u16,
    far_padding: u16,
    gdt_limit: u16,
    gdt_base: u32,
    progress: u8,
    progress_padding: u8,
    protected_offset: u32,
    protected_selector: u16,
    protected_padding: u16,
};

comptime {
    std.debug.assert(@offsetOf(TrampolineConfig, "progress") == 0x4E);
    std.debug.assert(@offsetOf(TrampolineConfig, "protected_offset") == 0x50);
}

pub const Status = struct {
    initialized: bool = false,
    activated: bool = false,
    discovered: u32 = 1,
    started: u32 = 0,
    online: u32 = 1,
    failed: u32 = 0,
    duplicates: u32 = 0,
    disabled: u32 = 0,
    startup_timeouts: u32 = 0,
    stop_timeouts: u32 = 0,
};

var current: Status = .{};
var release_aps: bool = false;
var topology: policy.Plan = .{};
var trampoline_phys: [percpu.max_cpus]u64 = .{0} ** percpu.max_cpus;
var ap_boot_stacks: [percpu.max_cpus][AP_STACK_SIZE]u8 align(64) = undefined;
var acceptance_release: u8 = 0;
var acceptance_done: u32 = 0;
var acceptance_failures: u32 = 0;
var acceptance_cpu_mask: u64 = 0;
var acceptance_results: [percpu.max_cpus]u64 = .{0} ** percpu.max_cpus;
var acceptance_clock_starts: [percpu.max_cpus]u64 = .{0} ** percpu.max_cpus;
var acceptance_clock_ends: [percpu.max_cpus]u64 = .{0} ** percpu.max_cpus;
var acceptance_tick_starts: [percpu.max_cpus]u64 = .{0} ** percpu.max_cpus;
var acceptance_tick_ends: [percpu.max_cpus]u64 = .{0} ** percpu.max_cpus;
var acceptance_clock_regressions: u32 = 0;
var acceptance_tlb_address: u64 = 0;
var acceptance_tlb_phase: u8 = 0;
var acceptance_tlb_ready_mask: u64 = 0;
var acceptance_tlb_updated_mask: u64 = 0;
var acceptance_tlb_failures: u32 = 0;
var acceptance_owner_done_mask: u64 = 0;
var acceptance_owner_failures: u32 = 0;
var acceptance_retire_release: u8 = 0;

pub fn initBsp() void {
    percpu.initBsp(platform_cpu.bootApicId());
}

pub fn reserveLowMemory() void {
    var reserved: u32 = 0;
    var index: u32 = 1;
    while (index < percpu.max_cpus) : (index += 1) {
        const low_page = phys.allocContiguousFramesBelow(1, 0x000F_FFFF) orelse break;
        _ = blocks.claimPhysicalRange(low_page, PAGE_SIZE, .kernel, .kernel, index, "smp-trampoline") catch {
            phys.freeFrame(low_page);
            break;
        };
        trampoline_phys[index] = low_page;
        reserved += 1;
    }
    k.puts("[SMP] low-memory trampolines reserved=");
    k.putDec(reserved);
    k.puts("\r\n");
}

pub fn startApplicationProcessors(info: acpi.Info) bool {
    if (current.initialized) return true;
    if (!lapic.isEnabled()) {
        current = .{ .initialized = true };
        logSummary("lapic-unavailable");
        return true;
    }

    var candidates: [acpi.MAX_MADT_CPUS]policy.Candidate = .{policy.Candidate{}} ** acpi.MAX_MADT_CPUS;
    const candidate_count: usize = @intCast(@min(info.madt_lapic_count, acpi.MAX_MADT_CPUS));
    var candidate_index: usize = 0;
    while (candidate_index < candidate_count) : (candidate_index += 1) {
        const cpu = info.madt_cpu_entries[candidate_index];
        candidates[candidate_index] = .{
            .apic_id = cpu.apic_id,
            .enabled = cpu.enabled(),
            .online_capable = cpu.onlineCapable(),
        };
    }
    topology = policy.buildPlan(lapic.localApicId(), candidates[0..candidate_count]);
    current = .{
        .initialized = true,
        .discovered = topology.count,
        .duplicates = topology.duplicates,
        .disabled = topology.disabled,
    };
    @atomicStore(bool, &release_aps, false, .release);
    if (topology.count > 1) interrupts.enableRuntimeSerialization();

    var index: u32 = 1;
    while (index < topology.count) : (index += 1) {
        const apic_id = topology.apic_ids[index];
        _ = percpu.configure(index, apic_id, .detected);
        if (comptime config.smp_fail_ap_index != 0xFFFF_FFFF) {
            if (index == config.smp_fail_ap_index) {
                failAp(index, false, "diagnostic-injection");
                continue;
            }
        }
        const low_page = prepareTrampoline(index) orelse {
            failAp(index, false, "trampoline");
            continue;
        };
        if (!scheduler.prepareSecondary(index)) {
            failAp(index, false, "idle-task");
            continue;
        }
        _ = percpu.setState(index, .starting);
        const vector: u8 = @intCast(low_page >> 12);
        if (!lapic.sendInitSipi(apic_id, vector)) {
            failAp(index, false, "init-sipi");
            continue;
        }
        if (!waitForState(index, .parked, AP_START_TIMEOUT_NS)) {
            failAp(index, true, "startup-timeout");
            continue;
        }
        // A clock-qualification failure does not discard a healthy AP. The
        // shared clocksource instead demotes every CPU to HPET atomically.
        _ = monotonic.finalizeCpuRegistration(index);
        current.started += 1;
    }
    logSummary("parked");
    return true;
}

pub fn activate() void {
    if (!current.initialized or current.activated) return;
    tlb.registerSender(sendTlbIpi);
    @atomicStore(bool, &release_aps, true, .release);
    var index: u32 = 1;
    while (index < topology.count) : (index += 1) {
        const before = percpu.state(index);
        if (before == .online) {
            current.online += 1;
        } else if (before == .parked and waitForState(index, .online, AP_START_TIMEOUT_NS)) {
            current.online += 1;
        } else if (before == .parked) {
            failAp(index, true, "activation-timeout");
        }
    }
    current.activated = true;
    logSummary("active");
}

const TlbAcceptanceProbe = struct {
    range_id: u32,
    address: u64,
    old_frame: u64,
    new_frame: u64,
    mapped_frame: u64,
};

fn prepareTlbAcceptanceProbe() ?TlbAcceptanceProbe {
    const range_id = virt.reserve(.{
        .window = .temp_kernel,
        .len = PAGE_SIZE,
        .kind = .virtual_range,
        .owner = .kernel,
        .owner_id = 0,
        .name = "tlb-acceptance",
        .flags = paging.WRITABLE | paging.NO_EXECUTE,
    }) catch return null;
    const range = virt.rangeInfo(range_id) orelse {
        virt.release(range_id) catch {};
        return null;
    };
    const old_frame = claimTlbProbeFrame("tlb-probe-old") orelse {
        virt.release(range_id) catch {};
        return null;
    };
    const new_frame = claimTlbProbeFrame("tlb-probe-new") orelse {
        _ = releaseTlbProbeFrame(old_frame);
        virt.release(range_id) catch {};
        return null;
    };
    const old_word: *volatile u64 = @ptrFromInt(phys.physToVirt(old_frame));
    const new_word: *volatile u64 = @ptrFromInt(phys.physToVirt(new_frame));
    old_word.* = TLB_PROBE_OLD;
    new_word.* = TLB_PROBE_NEW;
    if (!paging.mapPage(range.base, old_frame, paging.WRITABLE | paging.NO_EXECUTE)) {
        _ = releaseTlbProbeFrame(old_frame);
        _ = releaseTlbProbeFrame(new_frame);
        virt.release(range_id) catch {};
        return null;
    }
    return .{
        .range_id = range_id,
        .address = range.base,
        .old_frame = old_frame,
        .new_frame = new_frame,
        .mapped_frame = old_frame,
    };
}

fn claimTlbProbeFrame(name: []const u8) ?u64 {
    const frame = phys.allocFrame() orelse return null;
    _ = blocks.claimPhysicalRange(frame, PAGE_SIZE, .kernel, .kernel, 0, name) catch {
        phys.freeFrame(frame);
        return null;
    };
    return frame;
}

fn releaseTlbProbeFrame(frame: u64) bool {
    var plan: blocks.PhysicalReleasePlan = undefined;
    blocks.preparePhysicalRangeRelease(frame, PAGE_SIZE, &plan) catch return false;
    defer blocks.cancelPhysicalRangeRelease(&plan);
    phys.freeFrame(frame);
    blocks.commitPhysicalRangeRelease(&plan);
    return true;
}

fn cleanupTlbAcceptanceProbe(probe: *TlbAcceptanceProbe) bool {
    var ok = true;
    if (probe.mapped_frame != 0) {
        if (paging.mappedFrame(probe.address) == probe.mapped_frame and paging.unmapPage(probe.address)) {
            probe.mapped_frame = 0;
        } else {
            ok = false;
        }
    }
    if (probe.mapped_frame != probe.old_frame) ok = releaseTlbProbeFrame(probe.old_frame) and ok;
    if (probe.mapped_frame != probe.new_frame) ok = releaseTlbProbeFrame(probe.new_frame) and ok;
    if (probe.mapped_frame == 0) {
        virt.release(probe.range_id) catch {
            ok = false;
        };
    }
    return ok;
}

fn firstRemoteCpu(mask: u64) ?u32 {
    const current_cpu = percpu.currentIndex();
    var index: u32 = 0;
    while (index < percpu.max_cpus) : (index += 1) {
        if (index == current_cpu) continue;
        if ((mask & (@as(u64, 1) << @intCast(index))) != 0) return index;
    }
    return null;
}

// Test-profile-only proof for the new execution boundary.  It runs identical
// fixed integer work once on the BSP and once as one permanently placed
// internal worker per online CPU.  This is an acceptance probe, not the
// release benchmark and not a public affinity interface.
pub fn acceptanceProbeEnabled() bool {
    const value = boot_config.optionValue(boot_config.get(), "SMP", "selftest") orelse return false;
    return std.ascii.eqlIgnoreCase(value, "yes");
}

pub fn runAcceptanceProbeIfEnabled(usable_bytes: u64) bool {
    if (!acceptanceProbeEnabled()) return true;

    const online_mask = percpu.schedulableMask();
    const online_count: u32 = @intCast(@popCount(online_mask));
    if (online_count <= 1) {
        probePuts("[SMPPROBE] result=SKIPPED cpus=1 reason=single-cpu\r\n");
        return true;
    }

    var tlb_probe = prepareTlbAcceptanceProbe() orelse return acceptanceFail("tlb-setup");
    var tlb_cleanup_pending = true;
    defer if (tlb_cleanup_pending) {
        _ = cleanupTlbAcceptanceProbe(&tlb_probe);
    };

    var expected_checksum: u64 = 0;
    const sequential_start = monotonic.capture();
    var cpu_index: u32 = 0;
    while (cpu_index < percpu.max_cpus) : (cpu_index += 1) {
        const bit = @as(u64, 1) << @intCast(cpu_index);
        if ((online_mask & bit) == 0) continue;
        expected_checksum ^= acceptanceWork(cpu_index);
    }
    const sequential_ns = monotonic.elapsedNanoseconds(sequential_start, monotonic.capture()) orelse {
        return acceptanceFail("clock-sequential");
    };

    acceptance_results = .{0} ** percpu.max_cpus;
    acceptance_clock_starts = .{0} ** percpu.max_cpus;
    acceptance_clock_ends = .{0} ** percpu.max_cpus;
    acceptance_tick_starts = .{0} ** percpu.max_cpus;
    acceptance_tick_ends = .{0} ** percpu.max_cpus;
    @atomicStore(u8, &acceptance_release, 0, .release);
    @atomicStore(u32, &acceptance_done, 0, .release);
    @atomicStore(u32, &acceptance_failures, 0, .release);
    @atomicStore(u32, &acceptance_clock_regressions, 0, .release);
    @atomicStore(u64, &acceptance_cpu_mask, 0, .release);
    @atomicStore(u64, &acceptance_tlb_address, tlb_probe.address, .release);
    @atomicStore(u8, &acceptance_tlb_phase, 1, .release);
    @atomicStore(u64, &acceptance_tlb_ready_mask, 0, .release);
    @atomicStore(u64, &acceptance_tlb_updated_mask, 0, .release);
    @atomicStore(u32, &acceptance_tlb_failures, 0, .release);
    @atomicStore(u64, &acceptance_owner_done_mask, 0, .release);
    @atomicStore(u32, &acceptance_owner_failures, 0, .release);
    @atomicStore(u8, &acceptance_retire_release, 0, .release);
    defer @atomicStore(u8, &acceptance_retire_release, 1, .release);

    var workers: [percpu.max_cpus]?*task.Task = .{null} ** percpu.max_cpus;
    var worker_count: u32 = 0;
    while (worker_count < online_count) : (worker_count += 1) {
        workers[worker_count] = task.createParallelWorkerBlocked("smp-kwork", acceptanceWorkerMain) orelse {
            return acceptanceFail("worker-create");
        };
    }

    const ready_tick = timer.tickCount();
    worker_count = 0;
    var placement_mask: u64 = 0;
    while (worker_count < online_count) : (worker_count += 1) {
        const worker = workers[worker_count] orelse return acceptanceFail("worker-missing");
        task.markReady(worker, ready_tick);
        const target: u32 = worker.home_cpu;
        const bit = @as(u64, 1) << @intCast(target);
        if ((online_mask & bit) == 0 or (placement_mask & bit) != 0) {
            _ = @atomicRmw(u32, &acceptance_failures, .Add, 1, .acq_rel);
        }
        placement_mask |= bit;
    }

    const tlb_probe_start = monotonic.capture();
    @atomicStore(u8, &acceptance_release, 1, .release);
    cpu_index = 1;
    while (cpu_index < percpu.max_cpus) : (cpu_index += 1) {
        if ((online_mask & (@as(u64, 1) << @intCast(cpu_index))) != 0) sendReschedule(cpu_index);
    }

    var tlb_runtime_ok = true;
    while ((@atomicLoad(u64, &acceptance_tlb_ready_mask, .acquire) & online_mask) != online_mask) {
        scheduler.yield();
        const elapsed = monotonic.elapsedNanoseconds(tlb_probe_start, monotonic.capture()) orelse 0;
        if (elapsed >= ACCEPTANCE_TIMEOUT_NS) {
            tlb_runtime_ok = false;
            break;
        }
    }

    if (paging.unmapPage(tlb_probe.address)) {
        tlb_probe.mapped_frame = 0;
        if (paging.mapPage(tlb_probe.address, tlb_probe.new_frame, paging.WRITABLE | paging.NO_EXECUTE)) {
            tlb_probe.mapped_frame = tlb_probe.new_frame;
        } else {
            tlb_runtime_ok = false;
            if (paging.mapPage(tlb_probe.address, tlb_probe.old_frame, paging.WRITABLE | paging.NO_EXECUTE)) {
                tlb_probe.mapped_frame = tlb_probe.old_frame;
            }
        }
    } else {
        tlb_runtime_ok = false;
    }
    @atomicStore(u8, &acceptance_tlb_phase, 2, .release);

    while ((@atomicLoad(u64, &acceptance_tlb_updated_mask, .acquire) & online_mask) != online_mask) {
        scheduler.yield();
        const elapsed = monotonic.elapsedNanoseconds(tlb_probe_start, monotonic.capture()) orelse 0;
        if (elapsed >= ACCEPTANCE_TIMEOUT_NS) {
            tlb_runtime_ok = false;
            break;
        }
    }

    const timeout_before = tlb.stats();
    var timeout_rejected = false;
    var frame_retained = false;
    if (tlb_probe.mapped_frame == tlb_probe.new_frame) {
        if (firstRemoteCpu(online_mask)) |target_cpu| {
            if (tlb.armDiagnosticMissingAck(target_cpu)) {
                timeout_rejected = !paging.unmapPage(tlb_probe.address);
                tlb.disarmDiagnosticMissingAck();
                frame_retained = paging.mappedFrame(tlb_probe.address) == tlb_probe.new_frame and
                    blocks.claimedPhysicalPrefix(tlb_probe.new_frame, PAGE_SIZE) == PAGE_SIZE;
            }
        }
    }
    const timeout_after = tlb.stats();
    tlb_runtime_ok = tlb_runtime_ok and timeout_rejected and frame_retained and
        timeout_after.timeouts == timeout_before.timeouts + 1 and
        timeout_after.expected_timeouts == timeout_before.expected_timeouts + 1;

    const deadline_before = timer.deadlineStats();
    const parallel_start = monotonic.capture();
    @atomicStore(u8, &acceptance_tlb_phase, 3, .release);

    while (@atomicLoad(u32, &acceptance_done, .acquire) < online_count) {
        scheduler.yield();
        const elapsed = monotonic.elapsedNanoseconds(parallel_start, monotonic.capture()) orelse {
            return acceptanceFail("clock-parallel");
        };
        if (elapsed >= ACCEPTANCE_TIMEOUT_NS) return acceptanceFail("timeout");
    }
    const parallel_ns = monotonic.elapsedNanoseconds(parallel_start, monotonic.capture()) orelse {
        return acceptanceFail("clock-parallel");
    };

    var actual_checksum: u64 = 0;
    cpu_index = 0;
    while (cpu_index < percpu.max_cpus) : (cpu_index += 1) {
        if ((online_mask & (@as(u64, 1) << @intCast(cpu_index))) == 0) continue;
        actual_checksum ^= acceptance_results[cpu_index];
    }
    const observed_mask = @atomicLoad(u64, &acceptance_cpu_mask, .acquire);
    const failures = @atomicLoad(u32, &acceptance_failures, .acquire);
    const clock_regressions = @atomicLoad(u32, &acceptance_clock_regressions, .acquire);
    const deadline_after = timer.deadlineStats();
    const clock_status = monotonic.hardwareStatus();
    var clock_samples_ok = true;
    cpu_index = 0;
    while (cpu_index < percpu.max_cpus) : (cpu_index += 1) {
        if ((online_mask & (@as(u64, 1) << @intCast(cpu_index))) == 0) continue;
        clock_samples_ok = clock_samples_ok and
            acceptance_clock_starts[cpu_index] != 0 and
            acceptance_clock_ends[cpu_index] > acceptance_clock_starts[cpu_index] and
            acceptance_tick_ends[cpu_index] >= acceptance_tick_starts[cpu_index];
    }
    const clock_registration_ok =
        (clock_status.registered_cpu_mask & online_mask) == online_mask;
    const irq_delta = deadline_after.timer_irqs -% deadline_before.timer_irqs;
    const clock_ok = clock_status.source != .unavailable and clock_samples_ok and
        clock_registration_ok and clock_regressions == 0 and irq_delta != 0;
    const speedup_milli: u64 = if (parallel_ns == 0)
        0
    else
        @intCast((@as(u128, sequential_ns) * 1000) / parallel_ns);
    const heap_probe = heap.acceptanceProbe();
    const tlb_worker_failures = @atomicLoad(u32, &acceptance_tlb_failures, .acquire);
    const tlb_ready_mask = @atomicLoad(u64, &acceptance_tlb_ready_mask, .acquire);
    const tlb_updated_mask = @atomicLoad(u64, &acceptance_tlb_updated_mask, .acquire);
    const tlb_cleanup_ok = cleanupTlbAcceptanceProbe(&tlb_probe);
    tlb_cleanup_pending = false;
    @atomicStore(u8, &acceptance_retire_release, 1, .release);
    const tlb_stats = tlb.stats();
    const runtime_stats = interrupts.runtimeStats();
    const owner_stats = owner_locks.combinedStats();
    const owner_done_mask = @atomicLoad(u64, &acceptance_owner_done_mask, .acquire);
    const owner_failures = @atomicLoad(u32, &acceptance_owner_failures, .acquire);
    const lock_ok = runtime_stats.legacy_global_acquisitions == 0 and owner_stats.order_violations == 0 and
        owner_failures == 0 and (owner_done_mask & online_mask) == online_mask;
    const serial_stats = com.logTxStats();
    const serial_ok = serial_stats.bulk_calls != 0 and serial_stats.max_span > 1 and
        serial_stats.lock_acquisitions == serial_stats.write_calls and
        serial_stats.uart_status_reads < serial_stats.ring_bytes and serial_stats.dropped_bytes == 0;
    const r4l_preemption_ok = if (firstRemoteCpu(online_mask)) |target_cpu|
        r4x.runR4lPreemptionAcceptance(target_cpu, usable_bytes)
    else
        false;
    const tlb_ok = tlb_runtime_ok and tlb_cleanup_ok and tlb_worker_failures == 0 and
        (tlb_ready_mask & online_mask) == online_mask and (tlb_updated_mask & online_mask) == online_mask and
        tlb_stats.timeouts == tlb_stats.expected_timeouts and tlb_stats.successes != 0;
    const ok = failures == 0 and clock_ok and placement_mask == online_mask and observed_mask == online_mask and
        actual_checksum == expected_checksum and speedup_milli >= ACCEPTANCE_MIN_SPEEDUP_MILLI and heap_probe.ok and
        tlb_ok and lock_ok and serial_ok and r4l_preemption_ok;

    probePuts("[SMPPROBE] result=");
    probePuts(if (ok) "OK" else "FAILED");
    probePuts(" cpus=");
    probePutDec(online_count);
    probePuts(" sequential_ns=");
    probePutDec(sequential_ns);
    probePuts(" parallel_ns=");
    probePutDec(parallel_ns);
    probePuts(" speedup_milli=");
    probePutDec(speedup_milli);
    probePuts(" expected_mask=0x");
    probePutHex(online_mask, 8);
    probePuts(" observed_mask=0x");
    probePutHex(observed_mask, 8);
    probePuts(" failures=");
    probePutDec(failures);
    probePuts("\r\n");
    probePuts("[HEAPPROBE] result=");
    probePuts(if (heap_probe.ok) "OK" else "FAILED");
    probePuts(" iterations=");
    probePutDec(heap_probe.iterations);
    probePuts(" commit_calls=");
    probePutDec(heap_probe.commit_calls);
    probePuts(" commit_pages=");
    probePutDec(heap_probe.commit_pages);
    probePuts(" uncommit_calls=");
    probePutDec(heap_probe.uncommit_calls);
    probePuts(" release_suppressed=");
    probePutDec(heap_probe.release_suppressed);
    probePuts(" poison_bytes=");
    probePutDec(heap_probe.poison_bytes);
    probePuts(" retained_pages=");
    probePutDec(heap_probe.retained_pages);
    probePuts(" block_claims=");
    probePutDec(heap_probe.block_claims);
    probePuts(" extent_allocations=");
    probePutDec(heap_probe.extent_allocations);
    probePuts(" map_batches=");
    probePutDec(heap_probe.map_batches);
    probePuts(" unmap_batches=");
    probePutDec(heap_probe.unmap_batches);
    probePuts(" pressure=");
    probePuts(if (heap_probe.under_pressure) "yes" else "no");
    probePuts("\r\n");
    probePuts("[TLBPROBE] result=");
    probePuts(if (tlb_ok) "OK" else "FAILED");
    probePuts(" cpus=");
    probePutDec(online_count);
    probePuts(" ready_mask=0x");
    probePutHex(tlb_ready_mask, 8);
    probePuts(" updated_mask=0x");
    probePutHex(tlb_updated_mask, 8);
    probePuts(" stale_failures=");
    probePutDec(tlb_worker_failures);
    probePuts(" timeout_rejected=");
    probePutDec(@intFromBool(timeout_rejected));
    probePuts(" frame_retained=");
    probePutDec(@intFromBool(frame_retained));
    probePuts(" requests=");
    probePutDec(tlb_stats.requests);
    probePuts(" ipis=");
    probePutDec(tlb_stats.ipis_sent);
    probePuts(" acks=");
    probePutDec(tlb_stats.acknowledgements);
    probePuts(" timeouts=");
    probePutDec(tlb_stats.timeouts);
    probePuts(" expected_timeouts=");
    probePutDec(tlb_stats.expected_timeouts);
    probePuts(" generation=");
    probePutDec(tlb_stats.generation);
    probePuts(" cleanup=");
    probePuts(if (tlb_cleanup_ok) "yes" else "no");
    probePuts("\r\n");
    probePuts("[LOCKPROBE] result=");
    probePuts(if (lock_ok) "OK" else "FAILED");
    probePuts(" runtime_acquisitions=");
    probePutDec(runtime_stats.acquisitions);
    probePuts(" nested=");
    probePutDec(runtime_stats.nested_acquisitions);
    probePuts(" collisions=");
    probePutDec(runtime_stats.collisions);
    probePuts(" cpu_collisions=");
    probePutDec(runtime_stats.cpu_collisions);
    probePuts(" wait_spins=");
    probePutDec(runtime_stats.wait_spins);
    probePuts(" max_wait=");
    probePutDec(runtime_stats.max_wait_spins);
    probePuts(" max_hold_cycles=");
    probePutDec(runtime_stats.max_hold_cycles);
    probePuts(" legacy_global=");
    probePutDec(runtime_stats.legacy_global_acquisitions);
    probePuts(" runtime_owner_classes=1 owner_classes=");
    probePutDec(owner_locks.class_count);
    probePuts(" owner_acquisitions=");
    probePutDec(owner_stats.acquisitions);
    probePuts(" owner_collisions=");
    probePutDec(owner_stats.collisions);
    probePuts(" order_violations=");
    probePutDec(owner_stats.order_violations);
    probePuts(" stress_iterations=");
    probePutDec(OWNER_STRESS_ITERATIONS * @as(u64, online_count));
    probePuts(" stress_mask=0x");
    probePutHex(owner_done_mask, 8);
    probePuts(" stress_failures=");
    probePutDec(owner_failures);
    probePuts("\r\n");
    probePuts("[SERIALPROBE] result=");
    probePuts(if (serial_ok) "OK" else "FAILED");
    probePuts(" write_calls=");
    probePutDec(serial_stats.write_calls);
    probePuts(" bulk_calls=");
    probePutDec(serial_stats.bulk_calls);
    probePuts(" lock_acquisitions=");
    probePutDec(serial_stats.lock_acquisitions);
    probePuts(" status_reads=");
    probePutDec(serial_stats.uart_status_reads);
    probePuts(" ring_bytes=");
    probePutDec(serial_stats.ring_bytes);
    probePuts(" max_span=");
    probePutDec(serial_stats.max_span);
    probePuts(" dropped=");
    probePutDec(serial_stats.dropped_bytes);
    probePuts("\r\n");
    probePuts("[CLOCKPROBE] result=");
    probePuts(if (clock_ok) "OK" else "FAILED");
    probePuts(" source=");
    probePuts(switch (clock_status.source) {
        .unavailable => "unavailable",
        .tsc => "TSC",
        .hpet => "HPET",
    });
    probePuts(" cpus=");
    probePutDec(online_count);
    probePuts(" registered_mask=0x");
    probePutHex(clock_status.registered_cpu_mask, 8);
    probePuts(" regressions=");
    probePutDec(clock_regressions);
    probePuts(" irq_delta=");
    probePutDec(irq_delta);
    probePuts(" generation=");
    probePutDec(clock_status.generation);
    probePuts(" max_skew_ns=");
    probePutDec(clock_status.max_cpu_skew_ns);
    probePuts(" calibration_ppm=");
    probePutDec(clock_status.calibration_error_ppm);
    probePuts(" fallback=");
    probePuts(monotonic.hardwareFallbackReasonName(clock_status.fallback_reason));
    probePuts("\r\n");
    return ok;
}

pub fn status() Status {
    return current;
}

pub fn stopOthers() void {
    if (!current.initialized) return;
    var index: u32 = 1;
    while (index < topology.count) : (index += 1) {
        if (percpu.state(index) != .online) continue;
        _ = percpu.setState(index, .stopping);
        percpu.setSchedulable(index, false);
        const apic_id = percpu.apicId(index) orelse continue;
        _ = lapic.sendStop(apic_id, idt.STOP_VECTOR);
    }
    index = 1;
    while (index < topology.count) : (index += 1) {
        if (percpu.state(index) != .stopping) continue;
        if (!waitForState(index, .offline, AP_STOP_TIMEOUT_NS)) current.stop_timeouts += 1;
    }
}

pub fn sendReschedule(cpu_index: u32) void {
    if (cpu_index == percpu.currentIndex() or !percpu.isSchedulable(cpu_index)) return;
    const apic_id = percpu.apicId(cpu_index) orelse return;
    _ = lapic.sendReschedule(apic_id, idt.RESCHEDULE_VECTOR);
}

fn sendTlbIpi(cpu_index: u32, vector: u8) bool {
    if (cpu_index == percpu.currentIndex() or percpu.state(cpu_index) != .online) return false;
    const apic_id = percpu.apicId(cpu_index) orelse return false;
    return lapic.sendIpi(apic_id, vector);
}

pub fn irqTarget(ordinal: u32) u32 {
    const mask = percpu.schedulableMask();
    if (@popCount(mask) <= 1) return topology.apic_ids[0];
    var remaining = ordinal % @as(u32, @intCast(@popCount(mask)));
    var index: u32 = 0;
    while (index < percpu.max_cpus) : (index += 1) {
        if ((mask & (@as(u64, 1) << @intCast(index))) == 0) continue;
        if (remaining == 0) return percpu.apicId(index) orelse topology.apic_ids[0];
        remaining -= 1;
    }
    return topology.apic_ids[0];
}

pub export fn r4os_ap_entry(index_raw: u64) callconv(.c) noreturn {
    const index: u32 = @intCast(index_raw);
    gdt.initCurrent(index);
    percpu.install(index);
    idt.loadCurrent();
    if (!fpu.initCurrentCpu() or !lapic.initCurrentCpu() or !scheduler.initSecondary(index)) {
        _ = percpu.setState(index, .failed);
        interrupts.haltForever();
    }
    _ = monotonic.registerCurrentCpu(index);
    _ = percpu.setState(index, .parked);
    while (!@atomicLoad(bool, &release_aps, .acquire)) {
        if (percpu.state(index) == .stopping) {
            _ = percpu.setState(index, .offline);
            interrupts.haltForever();
        }
        asm volatile ("pause");
    }
    if (!lapic.startSecondaryPeriodicTimer(timer.DEFAULT_HZ)) {
        _ = percpu.setState(index, .failed);
        interrupts.haltForever();
    }
    percpu.setSchedulable(index, true);
    _ = percpu.setState(index, .online);
    scheduler.secondaryLoop();
}

fn prepareTrampoline(index: u32) ?u64 {
    const start = @intFromPtr(&r4os_ap_trampoline_start);
    const protected_mode = @intFromPtr(&r4os_ap_trampoline_protected_mode);
    const long_mode = @intFromPtr(&r4os_ap_trampoline_long_mode);
    const end = @intFromPtr(&r4os_ap_trampoline_end);
    if (end <= start or end - start >= CONFIG_OFFSET) {
        k.puts("[SMP] trampoline invalid size=");
        k.putDec(if (end > start) end - start else 0);
        k.puts("\r\n");
        return null;
    }
    const low_page = trampoline_phys[index];
    if (low_page == 0) {
        k.puts("[SMP] trampoline low-memory reservation missing\r\n");
        return null;
    }
    if (!paging.isMapped(low_page) and !paging.mapPage(low_page, low_page, paging.WRITABLE)) {
        k.puts("[SMP] trampoline identity mapping failed\r\n");
        return null;
    }
    const page: [*]u8 = @ptrFromInt(phys.physToVirt(low_page));
    @memset(page[0..PAGE_SIZE], 0);
    const source: [*]const u8 = @ptrFromInt(start);
    @memcpy(page[0 .. end - start], source[0 .. end - start]);

    const transition_gdt = [4]u64{
        0,
        0x00AF_9A00_0000_FFFF,
        0x00CF_9200_0000_FFFF,
        0x00CF_9A00_0000_FFFF,
    };
    const gdt_dest: *align(1) [4]u64 = @ptrFromInt(phys.physToVirt(low_page) + GDT_OFFSET);
    gdt_dest.* = transition_gdt;
    const slot: usize = @intCast(index);
    const stack_top = @intFromPtr(&ap_boot_stacks[slot]) + AP_STACK_SIZE;
    const cfg: *align(1) TrampolineConfig = @ptrFromInt(phys.physToVirt(low_page) + CONFIG_OFFSET);
    cfg.* = .{
        .magic = 0x5234_4F53_4150_3031,
        .cr3 = r4os_read_cr3(),
        .cr0 = r4os_read_cr0(),
        .cr4 = r4os_read_cr4(),
        // LMA reports the current CPU's active long-mode state and is
        // read-only.  Copying the BSP's set bit into the AP's real-mode
        // WRMSR faults on hardware/KVM even though TCG accepts it.
        .efer = (msr.read(IA32_EFER) & ~EFER_LMA) | EFER_LME,
        .entry = @intFromPtr(&r4os_ap_entry),
        .stack = stack_top,
        .cpu_index = index,
        .far_offset = @intCast(low_page + (long_mode - start)),
        .far_selector = TRANSITION_CODE_SELECTOR,
        .far_padding = 0,
        .gdt_limit = @sizeOf(@TypeOf(transition_gdt)) - 1,
        .gdt_base = @intCast(low_page + GDT_OFFSET),
        .progress = 0,
        .progress_padding = 0,
        .protected_offset = @intCast(low_page + (protected_mode - start)),
        .protected_selector = TRANSITION_PROTECTED_SELECTOR,
        .protected_padding = 0,
    };
    return low_page;
}

fn waitForState(index: u32, wanted: percpu.State, timeout_ns: u64) bool {
    const started = monotonic.nowNanoseconds();
    var spins: u64 = 0;
    while (percpu.state(index) != wanted) : (spins += 1) {
        const state_now = percpu.state(index);
        if (state_now == .failed or state_now == .offline) return false;
        if (started) |start_ns| {
            const now = monotonic.nowNanoseconds() orelse start_ns;
            if (now -% start_ns >= timeout_ns) return false;
        } else if (spins >= 100_000_000) {
            return false;
        }
        asm volatile ("pause");
    }
    return true;
}

fn failAp(index: u32, timeout: bool, reason: []const u8) void {
    percpu.setSchedulable(index, false);
    _ = percpu.setState(index, .failed);
    current.failed += 1;
    if (timeout) current.startup_timeouts += 1;
    k.puts("[SMP] ap=");
    k.putDec(index);
    k.puts(" apic=");
    k.putDec(percpu.apicId(index) orelse 0);
    k.puts(" failed=");
    k.puts(reason);
    if (timeout) {
        k.puts(" progress=");
        k.putDec(trampolineProgress(index));
    }
    k.puts("\r\n");
}

fn trampolineProgress(index: u32) u8 {
    if (index >= trampoline_phys.len) return 0;
    const low_page = trampoline_phys[index];
    if (low_page == 0) return 0;
    const progress_ptr: *volatile u8 = @ptrFromInt(
        phys.physToVirt(low_page) + CONFIG_OFFSET + @offsetOf(TrampolineConfig, "progress"),
    );
    return progress_ptr.*;
}

fn logSummary(stage: []const u8) void {
    k.puts("[SMP] stage=");
    k.puts(stage);
    k.puts(" discovered=");
    k.putDec(current.discovered);
    k.puts(" started=");
    k.putDec(current.started);
    k.puts(" online=");
    k.putDec(current.online);
    k.puts(" failed=");
    k.putDec(current.failed);
    k.puts(" fallback=");
    k.puts(if (current.online == 1) "1cpu" else "no");
    k.puts("\r\n");
}

fn acceptanceWorkerMain() callconv(.c) void {
    const cpu_index = percpu.currentIndex();
    while (@atomicLoad(u8, &acceptance_release, .acquire) == 0) asm volatile ("pause");
    if (cpu_index >= percpu.max_cpus) {
        _ = @atomicRmw(u32, &acceptance_failures, .Add, 1, .acq_rel);
    } else {
        const bit = @as(u64, 1) << @intCast(cpu_index);
        const previous = @atomicRmw(u64, &acceptance_cpu_mask, .Or, bit, .acq_rel);
        if ((previous & bit) != 0) {
            _ = @atomicRmw(u32, &acceptance_failures, .Add, 1, .acq_rel);
        } else {
            const probe_address = @atomicLoad(u64, &acceptance_tlb_address, .acquire);
            const probe_word: *const volatile u64 = @ptrFromInt(probe_address);
            if (@atomicLoad(u8, &acceptance_tlb_phase, .acquire) != 1 or probe_word.* != TLB_PROBE_OLD) {
                _ = @atomicRmw(u32, &acceptance_tlb_failures, .Add, 1, .acq_rel);
            }
            _ = @atomicRmw(u64, &acceptance_tlb_ready_mask, .Or, bit, .acq_rel);
            while (@atomicLoad(u8, &acceptance_tlb_phase, .acquire) < 2) scheduler.yield();
            if (probe_word.* != TLB_PROBE_NEW) {
                _ = @atomicRmw(u32, &acceptance_tlb_failures, .Add, 1, .acq_rel);
            }
            _ = @atomicRmw(u64, &acceptance_tlb_updated_mask, .Or, bit, .acq_rel);
            while (@atomicLoad(u8, &acceptance_tlb_phase, .acquire) < 3) scheduler.yield();
            runOwnerStress(cpu_index, bit);
            const start_ns = monotonic.nowNanoseconds() orelse 0;
            const start_tick = timer.tickCount();
            var prior_ns = start_ns;
            var clock_reads: u32 = 0;
            while (clock_reads < 256) : (clock_reads += 1) {
                const current_ns = monotonic.nowNanoseconds() orelse 0;
                if (current_ns < prior_ns) {
                    _ = @atomicRmw(u32, &acceptance_clock_regressions, .Add, 1, .acq_rel);
                }
                prior_ns = current_ns;
            }
            acceptance_results[cpu_index] = acceptanceWork(cpu_index);
            const end_tick = timer.tickCount();
            const end_ns = monotonic.nowNanoseconds() orelse 0;
            acceptance_clock_starts[cpu_index] = start_ns;
            acceptance_clock_ends[cpu_index] = end_ns;
            acceptance_tick_starts[cpu_index] = start_tick;
            acceptance_tick_ends[cpu_index] = end_tick;
            const deadline = timer.deadlineAfter(start_tick, 2);
            if (start_ns == 0 or end_ns <= start_ns or end_tick < start_tick or
                deadline <= start_tick or timer.remainingUntil(start_tick, deadline) != 2)
            {
                _ = @atomicRmw(u32, &acceptance_failures, .Add, 1, .acq_rel);
            }
        }
    }
    _ = @atomicRmw(u32, &acceptance_done, .Add, 1, .release);
    while (@atomicLoad(u8, &acceptance_retire_release, .acquire) == 0) scheduler.yield();
}

fn runOwnerStress(cpu_index: u32, bit: u64) void {
    var iteration: u64 = 0;
    while (iteration < OWNER_STRESS_ITERATIONS) : (iteration += 1) {
        const size: usize = @intCast(64 + ((iteration + cpu_index) & 7) * 32);
        const memory = heap.alloc(size, 16) orelse {
            _ = @atomicRmw(u32, &acceptance_owner_failures, .Add, 1, .acq_rel);
            continue;
        };
        const witness: u8 = @truncate(iteration ^ cpu_index);
        memory[0] = witness;
        memory[memory.len - 1] = ~witness;
        const content_ok = memory[0] == witness and memory[memory.len - 1] == ~witness;
        const free_result = heap.free(memory);
        if (!content_ok or free_result != .ok) {
            _ = @atomicRmw(u32, &acceptance_owner_failures, .Add, 1, .acq_rel);
        }
    }
    _ = @atomicRmw(u64, &acceptance_owner_done_mask, .Or, bit, .acq_rel);
}

fn acceptanceWork(cpu_index: u32) u64 {
    var value = @as(u64, cpu_index) *% 0x9E37_79B9_7F4A_7C15 +% 0xD1B5_4A32_D192_ED03;
    var iteration: u64 = 0;
    while (iteration < ACCEPTANCE_ITERATIONS) : (iteration += 1) {
        value ^= value >> 12;
        value ^= value << 25;
        value ^= value >> 27;
        value *%= 0x2545_F491_4F6C_DD1D;
        value +%= iteration ^ (@as(u64, cpu_index) << 32);
    }
    std.mem.doNotOptimizeAway(&value);
    return value;
}

fn acceptanceFail(reason: []const u8) bool {
    probePuts("[SMPPROBE] result=FAILED reason=");
    probePuts(reason);
    probePuts("\r\n");
    return false;
}

// Acceptance-only records belong in the captured serial test log, not in the
// fixed public 64-KiB bootlog ring whose early loader diagnostics are queried
// later by LOADERD.
fn probePuts(text: []const u8) void {
    k.serialWriteRaw(text);
}

fn probePutDec(value: u64) void {
    var buffer: [20]u8 = undefined;
    var index = buffer.len;
    var remaining = value;
    if (remaining == 0) return k.serialPutcRaw('0');
    while (remaining != 0) {
        index -= 1;
        buffer[index] = @intCast('0' + (remaining % 10));
        remaining /= 10;
    }
    probePuts(buffer[index..]);
}

fn probePutHex(value: u64, width: u8) void {
    const digits = "0123456789ABCDEF";
    var remaining = width;
    while (remaining != 0) {
        remaining -= 1;
        const shift: u6 = @intCast(@as(u32, remaining) * 4);
        k.serialPutcRaw(digits[@as(u4, @truncate(value >> shift))]);
    }
}
