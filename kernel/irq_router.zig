const interrupts = @import("../arch/x86_64/interrupts.zig");
const percpu = @import("../arch/x86_64/percpu.zig");
const ioapic = @import("../arch/x86_64/ioapic.zig");
const pic = @import("../arch/x86_64/pic.zig");
const scheduler = @import("../sched/scheduler.zig");
const timer = @import("timer.zig");
const time_core = @import("../platform/time.zig");
const k = @import("log.zig");

pub const MAX_IRQS: usize = 32;
pub const MAX_HANDLERS_PER_IRQ: usize = 4;

pub const IRQ_FLAG_SHARED: u32 = 1 << 0;
pub const IRQ_FLAG_LEVEL_LOW: u32 = 1 << 1;
// 0.59.19: MSI-Vektoren werden direkt vom LAPIC zugestellt; die Route hat
// keine IOAPIC-/PIC-Leitung. Der IDT-Vektor 32+irq ist fest verdrahtet und
// die Dispatch-Schleife sendet immer ein LAPIC-EOI.
pub const IRQ_FLAG_MSI: u32 = 1 << 2;
pub const IRQ_RESULT_HANDLED: u32 = 1;

pub const IrqHandler = *const fn (u8, usize) callconv(.c) u32;

pub const IrqStats = extern struct {
    irq: u8 = 0,
    registered: u8 = 0,
    shared: u8 = 0,
    masked: u8 = 1,
    dispatch_count: u64 = 0,
    handled_count: u64 = 0,
    last_result: u32 = 0,
    reserved: u32 = 0,
    handler_total_ticks: u64 = 0,
    handler_max_ticks: u64 = 0,
    handler_last_ticks: u64 = 0,
    last_owner: u32 = 0,
};

pub const IrqTimingStats = struct {
    dispatch_samples: u64 = 0,
    handler_samples: u64 = 0,
    observer_reads: u64 = 0,
    dispatch_total_ns: u64 = 0,
    dispatch_max_ns: u64 = 0,
    dispatch_last_ns: u64 = 0,
    handler_total_ns: u64 = 0,
    handler_max_ns: u64 = 0,
    handler_last_ns: u64 = 0,
    clock_generation: u32 = 0,
    unavailable_samples: u32 = 0,
    mixed_generation: bool = false,
};

const Entry = struct {
    active: bool = false,
    shared: bool = false,
    handler: ?IrqHandler = null,
    context: usize = 0,
    owner: u32 = 0,
};

var entries: [MAX_IRQS][MAX_HANDLERS_PER_IRQ]Entry = .{.{Entry{}} ** MAX_HANDLERS_PER_IRQ} ** MAX_IRQS;
var stats_table: [MAX_IRQS]IrqStats = initStats();
var timing_table: [MAX_IRQS]IrqTimingStats = .{IrqTimingStats{}} ** MAX_IRQS;
var dispatch_depth: [percpu.max_cpus]u32 = .{0} ** percpu.max_cpus;
var active_owner: [percpu.max_cpus]u32 = .{0} ** percpu.max_cpus;

pub fn inDispatch() bool {
    return dispatch_depth[percpu.currentIndex()] != 0;
}

pub fn currentOwner() u32 {
    return active_owner[percpu.currentIndex()];
}

// 0.56.9: Kernel-Space-Schranke fuer Handler-Zeiger. Die rip=0- bzw.
// rip=0x4000011CC-Crashes (Modul-LINK-Base!) passen auf einen Call durch
// einen ungueltigen Handler-/Funktionszeiger; die Wachhunde machen den
// Fall laut und benennen Vektor+Owner, statt ins Leere zu springen.
const KERNEL_SPACE_BOUND: u64 = 0xFFFF_8000_0000_0000;
var bad_handler_reported: bool = false;
var bad_handler_count: u64 = 0;

fn handlerPlausible(handler: IrqHandler) bool {
    return @intFromPtr(handler) >= KERNEL_SPACE_BOUND;
}

fn reportBadHandler(where: []const u8, irq: u8, ptr: u64, owner: u32) void {
    bad_handler_count += 1;
    if (bad_handler_reported) return;
    bad_handler_reported = true;
    k.puts("IRQROUTER BAD HANDLER ");
    k.puts(where);
    k.puts(" irq=");
    k.putDec(irq);
    k.puts(" ptr=0x");
    k.putHex(ptr, 16);
    k.puts(" owner=");
    k.putDec(owner);
    k.puts("\r\n");
}

pub fn register(irq: u8, handler: IrqHandler, context: usize, flags: u32, owner: u32) i32 {
    if (irq >= MAX_IRQS) return -1;
    if (!handlerPlausible(handler)) {
        reportBadHandler("register", irq, @intFromPtr(handler), owner);
        return -4;
    }

    const irq_flags = interrupts.saveAndDisableFor(.interrupt);
    defer interrupts.restore(irq_flags);

    const shared = (flags & IRQ_FLAG_SHARED) != 0;
    const irq_index: usize = @intCast(irq);
    var free_slot: ?usize = null;
    var active_count: u8 = 0;
    var all_shared = true;

    for (entries[irq_index], 0..) |entry, index| {
        if (!entry.active) {
            if (free_slot == null) free_slot = index;
            continue;
        }
        if (entry.handler != null and entry.handler.? == handler and entry.context == context) return 0;
        active_count += 1;
        all_shared = all_shared and entry.shared;
    }

    if (active_count != 0 and (!shared or !all_shared)) return -2;
    const slot = free_slot orelse return -3;

    entries[irq_index][slot] = .{
        .active = true,
        .shared = shared,
        .handler = handler,
        .context = context,
        .owner = owner,
    };
    refreshStats(irq);
    enableLine(irq, flags);
    return 0;
}

pub fn cleanupOwner(owner: u32) u32 {
    if (owner == 0) return 0;

    const irq_flags = interrupts.saveAndDisableFor(.interrupt);
    defer interrupts.restore(irq_flags);

    var removed: u32 = 0;
    var irq: u8 = 0;
    while (irq < MAX_IRQS) : (irq += 1) {
        const irq_index: usize = @intCast(irq);
        var changed = false;
        for (&entries[irq_index]) |*entry| {
            if (!entry.active or entry.owner != owner) continue;
            entry.* = .{};
            removed += 1;
            changed = true;
        }
        if (!changed) continue;
        refreshStats(irq);
        if (countHandlers(irq) == 0 and !isKernelOwnedIrq(irq)) disableLine(irq);
    }
    return removed;
}

pub fn unregister(irq: u8, handler: IrqHandler, context: usize) i32 {
    if (irq >= MAX_IRQS) return -1;

    const irq_flags = interrupts.saveAndDisableFor(.interrupt);
    defer interrupts.restore(irq_flags);

    const irq_index: usize = @intCast(irq);
    for (&entries[irq_index]) |*entry| {
        if (!entry.active or entry.handler == null or entry.handler.? != handler or entry.context != context) continue;
        entry.* = .{};
        refreshStats(irq);
        if (countHandlers(irq) == 0 and !isKernelOwnedIrq(irq)) disableLine(irq);
        return 0;
    }
    return -2;
}

pub fn stats(irq: u8, out: *IrqStats) i32 {
    if (irq >= MAX_IRQS) return -1;
    const irq_flags = interrupts.saveAndDisableFor(.interrupt);
    defer interrupts.restore(irq_flags);
    out.* = stats_table[@intCast(irq)];
    return 0;
}

pub fn timingStats(irq: u8, out: *IrqTimingStats) i32 {
    if (irq >= MAX_IRQS) return -1;
    const irq_flags = interrupts.saveAndDisableFor(.interrupt);
    defer interrupts.restore(irq_flags);
    out.* = timing_table[@intCast(irq)];
    return 0;
}

pub fn dispatch(irq: u8) void {
    if (irq >= MAX_IRQS) return;
    const irq_index: usize = @intCast(irq);
    var invoked = false;
    var last_result: u32 = 0;
    const measure_dispatch = stats_table[irq_index].registered != 0;
    const dispatch_start = if (measure_dispatch) time_core.monotonicCapture() else time_core.MonotonicStamp{};
    if (measure_dispatch) timing_table[irq_index].observer_reads +|= 1;

    const cpu_index = percpu.currentIndex();
    dispatch_depth[cpu_index] +|= 1;
    defer dispatch_depth[cpu_index] -= 1;

    for (entries[irq_index]) |entry| {
        if (!entry.active) continue;
        if (entry.handler) |handler| {
            if (!handlerPlausible(handler)) {
                // Tabelle korrumpiert oder unrelozierter Zeiger: laut
                // ueberspringen statt call-ins-Leere (rip=0-Crashklasse).
                reportBadHandler("dispatch", irq, @intFromPtr(handler), entry.owner);
                continue;
            }
            invoked = true;
            active_owner[cpu_index] = entry.owner;
            const start = timer.tickCount();
            const timing_start = time_core.monotonicCapture();
            timing_table[irq_index].observer_reads +|= 1;
            // R4D modules are built with the normal SIMD-capable module
            // target.  Unlike a task, an asynchronous IRQ handler has no FPU
            // context of its own, so preserve the interrupted R4X state and
            // execute the handler from the architectural initial state.
            const fpu_guard = scheduler.enterExternalIrqFpuGuard();
            last_result = handler(irq, entry.context);
            scheduler.leaveExternalIrqFpuGuard(fpu_guard);
            const timing_end = time_core.monotonicCapture();
            timing_table[irq_index].observer_reads +|= 1;
            const elapsed = elapsedTicks(start, timer.tickCount());
            stats_table[irq_index].handler_total_ticks +%= elapsed;
            stats_table[irq_index].handler_last_ticks = elapsed;
            if (elapsed > stats_table[irq_index].handler_max_ticks) stats_table[irq_index].handler_max_ticks = elapsed;
            stats_table[irq_index].last_owner = entry.owner;
            active_owner[cpu_index] = 0;
            if ((last_result & IRQ_RESULT_HANDLED) != 0) {
                stats_table[irq_index].handled_count +%= 1;
            }
            recordHandlerTiming(irq_index, timing_start, timing_end);
        }
    }

    if (invoked) {
        stats_table[irq_index].dispatch_count +%= 1;
        stats_table[irq_index].last_result = last_result;
    }
    if (measure_dispatch) {
        const dispatch_end = time_core.monotonicCapture();
        timing_table[irq_index].observer_reads +|= 1;
        recordDispatchTiming(irq_index, dispatch_start, dispatch_end);
    }
}

fn recordHandlerTiming(index: usize, start: time_core.MonotonicStamp, end: time_core.MonotonicStamp) void {
    const elapsed = measuredElapsed(index, start, end) orelse return;
    var timing = &timing_table[index];
    timing.handler_samples +|= 1;
    timing.handler_total_ns +|= elapsed;
    timing.handler_last_ns = elapsed;
    if (elapsed > timing.handler_max_ns) timing.handler_max_ns = elapsed;
}

fn recordDispatchTiming(index: usize, start: time_core.MonotonicStamp, end: time_core.MonotonicStamp) void {
    const elapsed = measuredElapsed(index, start, end) orelse return;
    var timing = &timing_table[index];
    timing.dispatch_samples +|= 1;
    timing.dispatch_total_ns +|= elapsed;
    timing.dispatch_last_ns = elapsed;
    if (elapsed > timing.dispatch_max_ns) timing.dispatch_max_ns = elapsed;
}

fn measuredElapsed(index: usize, start: time_core.MonotonicStamp, end: time_core.MonotonicStamp) ?u64 {
    var timing = &timing_table[index];
    if (start.generation != end.generation) {
        timing.mixed_generation = true;
        timing.unavailable_samples +|= 1;
        return null;
    }
    if (timing.clock_generation == 0) {
        timing.clock_generation = start.generation;
    } else if (timing.clock_generation != start.generation) {
        timing.mixed_generation = true;
    }
    return time_core.monotonicElapsed(start, end) orelse {
        timing.unavailable_samples +|= 1;
        return null;
    };
}

fn elapsedTicks(start: u64, end: u64) u64 {
    if (end < start) return 0;
    return end - start;
}

fn enableLine(irq: u8, flags: u32) void {
    const irq_index: usize = @intCast(irq);
    if ((flags & IRQ_FLAG_MSI) != 0) {
        stats_table[irq_index].masked = 0;
        return;
    }
    if (ioapic.isRoutingActive()) {
        if ((flags & IRQ_FLAG_LEVEL_LOW) != 0 and ioapic.activatePciIntxIrq(irq)) {
            if (irq < 16) pic.unmask(irq);
            stats_table[irq_index].masked = 0;
            return;
        }
        if (ioapic.activateLegacyIrq(irq)) {
            if (irq < 16) pic.unmask(irq);
            stats_table[irq_index].masked = 0;
            return;
        }
    }
    if (irq < 16) {
        pic.unmask(irq);
        stats_table[irq_index].masked = 0;
    }
}

fn disableLine(irq: u8) void {
    const irq_index: usize = @intCast(irq);
    if (ioapic.isRoutingActive()) {
        _ = ioapic.setLegacyIrqMasked(irq, true);
    } else if (irq < 16) {
        pic.mask(irq);
    }
    stats_table[irq_index].masked = 1;
}

fn refreshStats(irq: u8) void {
    const irq_index: usize = @intCast(irq);
    var registered: u8 = 0;
    var shared: u8 = 0;
    for (entries[irq_index]) |entry| {
        if (!entry.active) continue;
        registered += 1;
        if (entry.shared) shared += 1;
    }
    stats_table[irq_index].irq = irq;
    stats_table[irq_index].registered = registered;
    stats_table[irq_index].shared = shared;
}

fn countHandlers(irq: u8) u8 {
    const irq_index: usize = @intCast(irq);
    var count: u8 = 0;
    for (entries[irq_index]) |entry| {
        if (entry.active) count += 1;
    }
    return count;
}

fn isKernelOwnedIrq(irq: u8) bool {
    return irq == 0 or irq == 1 or irq == 12;
}

fn initStats() [MAX_IRQS]IrqStats {
    var result: [MAX_IRQS]IrqStats = .{IrqStats{}} ** MAX_IRQS;
    for (&result, 0..) |*entry, index| {
        entry.irq = @intCast(index);
        entry.masked = if (isKernelOwnedIrq(@intCast(index))) 0 else 1;
    }
    return result;
}
