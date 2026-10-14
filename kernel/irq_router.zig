// Stateless private C facade to the one native IRQ route/dispatch owner.
comptime {
    _ = @import("irq_router_bridge.zig");
}
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

pub const IrqTimingStats = extern struct {
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

extern fn r4os_rust_irq_router_register(irq: u8, handler: IrqHandler, context: usize, flags: u32, owner: u32) i32;
extern fn r4os_rust_irq_router_cleanup(owner: u32) u32;
extern fn r4os_rust_irq_router_unregister(irq: u8, handler: IrqHandler, context: usize) i32;
extern fn r4os_rust_irq_router_stats(irq: u8, detail: bool, output: *anyopaque) i32;
extern fn r4os_rust_irq_router_local(owner: bool) u32;
extern fn r4os_rust_irq_router_dispatch(irq: u8) void;
comptime {
    if (@sizeOf(IrqStats) != 64 or @sizeOf(IrqTimingStats) != 88 or @offsetOf(IrqTimingStats, "clock_generation") != 72)
        @compileError("Native IRQ stats C layout drift");
}
pub fn inDispatch() bool {
    return r4os_rust_irq_router_local(false) != 0;
}
pub fn currentOwner() u32 {
    return r4os_rust_irq_router_local(true);
}
pub fn register(irq: u8, handler: IrqHandler, context: usize, flags: u32, owner: u32) i32 {
    return r4os_rust_irq_router_register(irq, handler, context, flags, owner);
}
pub fn cleanupOwner(owner: u32) u32 {
    return r4os_rust_irq_router_cleanup(owner);
}
pub fn unregister(irq: u8, handler: IrqHandler, context: usize) i32 {
    return r4os_rust_irq_router_unregister(irq, handler, context);
}
pub fn stats(irq: u8, output: *IrqStats) i32 {
    return r4os_rust_irq_router_stats(irq, false, output);
}
pub fn timingStats(irq: u8, output: *IrqTimingStats) i32 {
    return r4os_rust_irq_router_stats(irq, true, output);
}
pub fn dispatch(irq: u8) void {
    r4os_rust_irq_router_dispatch(irq);
}
