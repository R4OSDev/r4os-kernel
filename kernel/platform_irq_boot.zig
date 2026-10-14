// Native controller setup and validated timer handoff; presentation cuts only.
const boot_status = @import("boot_status.zig");
const fatal = @import("fatal.zig");
const hpet = @import("../arch/x86_64/hpet.zig");
const ioapic = @import("../arch/x86_64/ioapic.zig");
const irq_status = @import("../platform/irq.zig");
const lapic = @import("../arch/x86_64/lapic.zig");
const time_core = @import("../platform/time.zig");
const timer = @import("timer.zig");
pub const TimerHandoffStatus = struct {
    attempted: bool = false,
    active_backend: timer.Backend = .pit,
    requested_hz: u32 = timer.DEFAULT_HZ,
    validated_ticks: u64 = 0,
    pit_irq0_route_masked: bool = false,
    fallback_reason: []const u8 = "not initialized",
};

pub const Status = struct {
    initialized: bool = false,
    lapic_status: lapic.Status = .{},
    ioapic_status: ioapic.Status = .{},
    hpet_status: hpet.Status = .{},
    ioapic_routing_active: bool = false,
    hpet_visible: bool = false,
    hpet_initialized: bool = false,
    pic_masked: bool = false,
    pic_master_mask: u8 = 0xFF,
    pic_slave_mask: u8 = 0xFF,
    pic_fallback_reason: []const u8 = "not initialized",
    timer_handoff: TimerHandoffStatus = .{},
    final_irq_status: irq_status.Status = .{},
    status_finalized: bool = false,
    time_status_logged: bool = false,
};

const RawText = extern struct { pointer: ?[*]const u8, bytes: u64 };
const RawHandoff = extern struct {
    attempted: bool,
    active_backend: u8,
    requested_hz: u32,
    validated_ticks: u64,
    pit_irq0_route_masked: bool,
    fallback_reason: RawText,
};
const RawStatus = extern struct {
    initialized: bool,
    lapic_status: lapic.RawStatus,
    ioapic_status: ioapic.RawStatus,
    hpet_status: hpet.RawStatus,
    ioapic_routing_active: bool,
    hpet_visible: bool,
    hpet_initialized: bool,
    pic_masked: bool,
    pic_master_mask: u8,
    pic_slave_mask: u8,
    pic_fallback_reason: RawText,
    timer_handoff: RawHandoff,
    final_irq_status: irq_status.RawStatus,
    status_finalized: bool,
    time_status_logged: bool,
};
extern fn r4os_rust_irq_boot_control(operation: u8) bool;
extern fn r4os_rust_irq_boot_status(output: *RawStatus) void;
export fn r4os_zig_irq_boot_step(operation: u8) callconv(.c) bool {
    if (@import("builtin").is_test) @panic("IRQ boot work requested by host fixture");
    switch (operation) {
        0 => return fatal.fail(.irq, "Platform IRQ boot before platform state"),
        1 => boot_status.statusLine("  Platform IRQ [IOAPIC]\r\n"),
        2 => boot_status.statusLine("  IOAPIC legacy routing [PIC fallback]\r\n"),
        3 => boot_status.statusLine("  Timer [HPET]\r\n"),
        4 => boot_status.statusLine("  Timer [LAPIC]\r\n"),
        5 => boot_status.statusLine("  Modern timer [PIT fallback]\r\n"),
        6 => time_core.logStatus(),
        else => return false,
    }
    return true;
}
fn text(raw: RawText) []const u8 {
    return if (raw.pointer) |pointer| pointer[0..@intCast(raw.bytes)] else "";
}
pub fn init() bool {
    return r4os_rust_irq_boot_control(0);
}
pub fn isInitialized() bool {
    return r4os_rust_irq_boot_control(1);
}
pub fn status() Status {
    var raw: RawStatus = undefined;
    r4os_rust_irq_boot_status(&raw);
    return .{
        .initialized = raw.initialized,
        .lapic_status = lapic.project(raw.lapic_status),
        .ioapic_status = ioapic.project(raw.ioapic_status),
        .hpet_status = hpet.project(raw.hpet_status),
        .ioapic_routing_active = raw.ioapic_routing_active,
        .hpet_visible = raw.hpet_visible,
        .hpet_initialized = raw.hpet_initialized,
        .pic_masked = raw.pic_masked,
        .pic_master_mask = raw.pic_master_mask,
        .pic_slave_mask = raw.pic_slave_mask,
        .pic_fallback_reason = text(raw.pic_fallback_reason),
        .timer_handoff = .{ .attempted = raw.timer_handoff.attempted, .active_backend = @enumFromInt(raw.timer_handoff.active_backend), .requested_hz = raw.timer_handoff.requested_hz, .validated_ticks = raw.timer_handoff.validated_ticks, .pit_irq0_route_masked = raw.timer_handoff.pit_irq0_route_masked, .fallback_reason = text(raw.timer_handoff.fallback_reason) },
        .final_irq_status = irq_status.project(raw.final_irq_status),
        .status_finalized = raw.status_finalized,
        .time_status_logged = raw.time_status_logged,
    };
}
comptime {
    if (@sizeOf(RawHandoff) != 40 or @sizeOf(RawStatus) != 504 or @offsetOf(RawStatus, "timer_handoff") != 352 or @offsetOf(RawStatus, "final_irq_status") != 392)
        @compileError("Rust IRQ boot status layout differs");
}
