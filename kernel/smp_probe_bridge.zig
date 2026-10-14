const native_tasks = @import("../sched/task.zig");
// Exact C report transport and remaining Task/VM/Mutex owner cuts.
const std = @import("std");
const boot_config = @import("boot_config.zig");
const blocks = @import("../memory/blocks.zig");
const com = @import("../driver/com.zig");
const heap = @import("../memory/heap.zig");
const interrupts = @import("../arch/x86_64/interrupts.zig");
const owner_locks = @import("../memory/owner_locks.zig");
const paging = @import("../memory/paging.zig");
const phys = @import("../memory/phys.zig");
const monotonic = @import("../platform/monotonic.zig");
const clocksource = @import("../platform/clocksource.zig");
const scheduler = @import("../sched/scheduler.zig");
const sync = @import("../sched/sync.zig");
const task = @import("../sched/task.zig");
const r4x = @import("../program/r4x.zig");
const timer = @import("timer.zig");
const virt = @import("../memory/virt.zig");
const k = @import("log.zig");
const PAGE_SIZE: usize = 4096;
const OWNER_STRESS_ITERATIONS: u64 = 64;
const TLB_PROBE_OLD: u64 = 0x544C_422D_4F4C_4421;
const TLB_PROBE_NEW: u64 = 0x544C_422D_4E45_5721;
const RawClock = extern struct {
    source: u32,
    generation: u32,
    frequency_hz: u64,
    resolution_ns: u64,
    flags: u32,
    fallback_reason: u32,
    registered_cpu_mask: u64,
    max_cpu_skew_ns: u64,
    calibration_error_ppm: u64,
};
pub const HeapReport = extern struct {
    ok: bool = false,
    iterations: u64 = 0,
    commit_calls: u64 = 0,
    commit_pages: u64 = 0,
    uncommit_calls: u64 = 0,
    release_suppressed: u64 = 0,
    poison_bytes: u64 = 0,
    retained_pages: u64 = 0,
    block_claims: u64 = 0,
    extent_allocations: u64 = 0,
    map_batches: u64 = 0,
    unmap_batches: u64 = 0,
    under_pressure: bool = false,
};
pub const Report = extern struct {
    ok: bool,
    clock_ok: bool,
    tlb_ok: bool,
    lock_ok: bool,
    serial_ok: bool,
    timeout_rejected: bool,
    frame_retained: bool,
    tlb_cleanup_ok: bool,
    online_count: u32,
    failures: u32,
    clock_regressions: u32,
    tlb_failures: u32,
    owner_failures: u32,
    sequential_ns: u64,
    parallel_ns: u64,
    speedup_milli: u64,
    online_mask: u64,
    observed_mask: u64,
    tlb_ready_mask: u64,
    tlb_updated_mask: u64,
    owner_done_mask: u64,
    irq_delta: u64,
    heap_probe: HeapReport,
    clock_status: RawClock,
    tlb_stats: @import("../arch/x86_64/tlb_shootdown.zig").Stats,
    runtime_stats: interrupts.RuntimeStats,
    owner_stats: owner_locks.Stats,
    serial_stats: com.LogTxStats,
};
comptime {
    if (@sizeOf(Report) != 600 or @offsetOf(Report, "heap_probe") != 104 or @offsetOf(Report, "serial_stats") != 488 or
        @sizeOf(HeapReport) != 104 or @sizeOf(RawClock) != 56 or @sizeOf(TlbAcceptanceProbe) != 40)
        @compileError("Native SMP probe report differs");
}
// The synchronization primitive stays with the single Zig sync owner until
// 0.83.11. Probe counters/state themselves have only the native Rust owner.
var probe_mutex: sync.Mutex = sync.Mutex.initClass("smp-owner-probe", sync.LockRank.local, .sleepable);
export fn r4os_zig_smp_probe_mutex(operation: u8) callconv(.c) bool {
    return switch (operation) {
        0 => blk: {
            probe_mutex = sync.Mutex.initClass("smp-owner-probe", sync.LockRank.local, .sleepable);
            break :blk true;
        },
        1 => probe_mutex.lock(timer.DEFAULT_HZ),
        2 => probe_mutex.unlock(),
        else => unreachable,
    };
}
export fn r4os_zig_smp_probe_setup(output: *TlbAcceptanceProbe) callconv(.c) bool {
    output.* = prepareTlbAcceptanceProbe() orelse return false;
    return true;
}
export fn r4os_zig_smp_probe_cleanup(probe: *TlbAcceptanceProbe) callconv(.c) bool {
    return cleanupTlbAcceptanceProbe(probe);
}
export fn r4os_zig_smp_probe_task(operation: u8, target: ?*task.Task, value: u64, entry: ?task.Entry) callconv(.c) u64 {
    return switch (operation) {
        0 => blk: {
            const created = task.createParallelWorkerBlocked("smp-kwork", entry.?) orelse break :blk 0;
            break :blk @intFromPtr(created);
        },
        1 => @intFromBool(task.bindBlockedHomeCpu(target.?, @intCast(value))),
        2 => blk: {
            task.markReady(target.?, value);
            break :blk native_tasks.get(target.?, .home_cpu);
        },
        else => unreachable,
    };
}
export fn r4os_zig_smp_probe_finish(operation: u8, cpu: u32, usable: u64) callconv(.c) bool {
    return switch (operation) {
        0 => r4x.runR4lPreemptionAcceptance(cpu, usable),
        1 => r4x.runParallelOwnersAcceptance(),
        else => unreachable,
    };
}
export fn r4os_zig_smp_probe_heap(output: *HeapReport) callconv(.c) void {
    const result = heap.acceptanceProbe();
    inline for (@typeInfo(HeapReport).@"struct".fields) |field| @field(output.*, field.name) = @field(result, field.name);
}
export fn r4os_zig_smp_probe_counters(runtime_output: *interrupts.RuntimeStats, owner_output: *owner_locks.Stats, serial_output: *com.LogTxStats) callconv(.c) bool {
    runtime_output.* = interrupts.runtimeStats();
    owner_output.* = owner_locks.combinedStats();
    serial_output.* = com.logTxStats();
    const serial = serial_output.*;
    return serial.bulk_calls != 0 and serial.max_span > 1 and serial.lock_acquisitions == serial.write_calls and
        serial.uart_status_reads < serial.ring_bytes and serial.dropped_bytes == 0;
}
export fn r4os_zig_smp_probe_text(event: u8, text: [*]const u8, len: usize) callconv(.c) void {
    switch (event) {
        0 => {
            probePuts("[SMPPROBE] result=FAILED reason=");
            probePuts(text[0..len]);
            probePuts("\r\n");
        },
        1 => probePuts("[SMPPROBE] result=SKIPPED cpus=1 reason=single-cpu\r\n"),
        else => unreachable,
    }
}
export fn r4os_zig_smp_probe_report(report: *const Report) callconv(.c) void {
    const raw = report.clock_status;
    const clock_status = clocksource.Status{ .source = @enumFromInt(raw.source), .generation = raw.generation, .frequency_hz = raw.frequency_hz, .resolution_ns = raw.resolution_ns, .registered_cpu_mask = raw.registered_cpu_mask, .max_cpu_skew_ns = raw.max_cpu_skew_ns, .calibration_error_ppm = raw.calibration_error_ppm, .fallback_reason = @enumFromInt(raw.fallback_reason) };
    probePuts("[SMPPROBE] result=");
    probePuts(if (report.ok) "OK" else "FAILED");
    probePuts(" cpus=");
    probePutDec(report.online_count);
    probePuts(" sequential_ns=");
    probePutDec(report.sequential_ns);
    probePuts(" parallel_ns=");
    probePutDec(report.parallel_ns);
    probePuts(" speedup_milli=");
    probePutDec(report.speedup_milli);
    probePuts(" expected_mask=0x");
    probePutHex(report.online_mask, 8);
    probePuts(" observed_mask=0x");
    probePutHex(report.observed_mask, 8);
    probePuts(" failures=");
    probePutDec(report.failures);
    probePuts("\r\n");
    probePuts("[HEAPPROBE] result=");
    probePuts(if (report.heap_probe.ok) "OK" else "FAILED");
    probePuts(" iterations=");
    probePutDec(report.heap_probe.iterations);
    probePuts(" commit_calls=");
    probePutDec(report.heap_probe.commit_calls);
    probePuts(" commit_pages=");
    probePutDec(report.heap_probe.commit_pages);
    probePuts(" uncommit_calls=");
    probePutDec(report.heap_probe.uncommit_calls);
    probePuts(" release_suppressed=");
    probePutDec(report.heap_probe.release_suppressed);
    probePuts(" poison_bytes=");
    probePutDec(report.heap_probe.poison_bytes);
    probePuts(" retained_pages=");
    probePutDec(report.heap_probe.retained_pages);
    probePuts(" block_claims=");
    probePutDec(report.heap_probe.block_claims);
    probePuts(" extent_allocations=");
    probePutDec(report.heap_probe.extent_allocations);
    probePuts(" map_batches=");
    probePutDec(report.heap_probe.map_batches);
    probePuts(" unmap_batches=");
    probePutDec(report.heap_probe.unmap_batches);
    probePuts(" pressure=");
    probePuts(if (report.heap_probe.under_pressure) "yes" else "no");
    probePuts("\r\n");
    probePuts("[TLBPROBE] result=");
    probePuts(if (report.tlb_ok) "OK" else "FAILED");
    probePuts(" cpus=");
    probePutDec(report.online_count);
    probePuts(" ready_mask=0x");
    probePutHex(report.tlb_ready_mask, 8);
    probePuts(" updated_mask=0x");
    probePutHex(report.tlb_updated_mask, 8);
    probePuts(" stale_failures=");
    probePutDec(report.tlb_failures);
    probePuts(" timeout_rejected=");
    probePutDec(@intFromBool(report.timeout_rejected));
    probePuts(" frame_retained=");
    probePutDec(@intFromBool(report.frame_retained));
    probePuts(" requests=");
    probePutDec(report.tlb_stats.requests);
    probePuts(" ipis=");
    probePutDec(report.tlb_stats.ipis_sent);
    probePuts(" acks=");
    probePutDec(report.tlb_stats.acknowledgements);
    probePuts(" timeouts=");
    probePutDec(report.tlb_stats.timeouts);
    probePuts(" expected_timeouts=");
    probePutDec(report.tlb_stats.expected_timeouts);
    probePuts(" generation=");
    probePutDec(report.tlb_stats.generation);
    probePuts(" cleanup=");
    probePuts(if (report.tlb_cleanup_ok) "yes" else "no");
    probePuts("\r\n");
    probePuts("[LOCKPROBE] result=");
    probePuts(if (report.lock_ok) "OK" else "FAILED");
    probePuts(" runtime_acquisitions=");
    probePutDec(report.runtime_stats.acquisitions);
    probePuts(" nested=");
    probePutDec(report.runtime_stats.nested_acquisitions);
    probePuts(" collisions=");
    probePutDec(report.runtime_stats.collisions);
    probePuts(" cpu_collisions=");
    probePutDec(report.runtime_stats.cpu_collisions);
    probePuts(" wait_spins=");
    probePutDec(report.runtime_stats.wait_spins);
    probePuts(" max_wait=");
    probePutDec(report.runtime_stats.max_wait_spins);
    probePuts(" max_hold_cycles=");
    probePutDec(report.runtime_stats.max_hold_cycles);
    probePuts(" legacy_global=");
    probePutDec(report.runtime_stats.legacy_global_acquisitions);
    probePuts(" runtime_owner_classes=1 owner_classes=");
    probePutDec(owner_locks.class_count);
    probePuts(" owner_acquisitions=");
    probePutDec(report.owner_stats.acquisitions);
    probePuts(" owner_collisions=");
    probePutDec(report.owner_stats.collisions);
    probePuts(" order_violations=");
    probePutDec(report.owner_stats.order_violations);
    probePuts(" stress_iterations=");
    probePutDec(OWNER_STRESS_ITERATIONS * @as(u64, report.online_count));
    probePuts(" stress_mask=0x");
    probePutHex(report.owner_done_mask, 8);
    probePuts(" stress_failures=");
    probePutDec(report.owner_failures);
    probePuts("\r\n");
    probePuts("[SERIALPROBE] result=");
    probePuts(if (report.serial_ok) "OK" else "FAILED");
    probePuts(" write_calls=");
    probePutDec(report.serial_stats.write_calls);
    probePuts(" bulk_calls=");
    probePutDec(report.serial_stats.bulk_calls);
    probePuts(" lock_acquisitions=");
    probePutDec(report.serial_stats.lock_acquisitions);
    probePuts(" status_reads=");
    probePutDec(report.serial_stats.uart_status_reads);
    probePuts(" ring_bytes=");
    probePutDec(report.serial_stats.ring_bytes);
    probePuts(" max_span=");
    probePutDec(report.serial_stats.max_span);
    probePuts(" dropped=");
    probePutDec(report.serial_stats.dropped_bytes);
    probePuts("\r\n");
    probePuts("[CLOCKPROBE] result=");
    probePuts(if (report.clock_ok) "OK" else "FAILED");
    probePuts(" source=");
    probePuts(switch (clock_status.source) {
        .unavailable => "unavailable",
        .tsc => "TSC",
        .hpet => "HPET",
    });
    probePuts(" cpus=");
    probePutDec(report.online_count);
    probePuts(" registered_mask=0x");
    probePutHex(clock_status.registered_cpu_mask, 8);
    probePuts(" regressions=");
    probePutDec(report.clock_regressions);
    probePuts(" irq_delta=");
    probePutDec(report.irq_delta);
    probePuts(" generation=");
    probePutDec(clock_status.generation);
    probePuts(" max_skew_ns=");
    probePutDec(clock_status.max_cpu_skew_ns);
    probePuts(" calibration_ppm=");
    probePutDec(clock_status.calibration_error_ppm);
    probePuts(" fallback=");
    probePuts(monotonic.hardwareFallbackReasonName(clock_status.fallback_reason));
    probePuts("\r\n");
}
const TlbAcceptanceProbe = extern struct {
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
