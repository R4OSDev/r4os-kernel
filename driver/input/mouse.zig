// Sole mouse owner is native; fixed C projections and passive display adapter.
extern fn r4os_rust_input_mouse_motion() u64;
extern fn r4os_rust_input_mouse_init(*const Bounds) bool;
extern fn r4os_rust_input_mouse_disable() void;
extern fn r4os_rust_input_mouse_irq() void;
extern fn r4os_rust_input_mouse_byte(u8) void;
extern fn r4os_rust_input_mouse_snapshot(*State, bool) void;
extern fn r4os_rust_input_mouse_stats(*Stats) void;
extern fn r4os_rust_input_mouse_poll_hook(?PollHook) void;
extern fn r4os_rust_input_mouse_inject(i32, i32, u8, i32) void;
extern fn r4os_rust_input_mouse_enable_cursor() void;
extern fn r4os_rust_input_mouse_disable_cursor() void;
extern fn r4os_rust_input_mouse_refresh_cursor() void;
extern fn r4os_rust_input_mouse_cursor(*CursorOverlay) void;
extern fn r4os_rust_input_mouse_mask(usize, usize) u8;
export fn r4os_zig_input_mouse_bounds(out: *Bounds) callconv(.c) void {
    const b = @import("../../display/surface_pipeline.zig").pixelBounds();
    out.* = .{ .width = b.width, .height = b.height };
}
const io = @import("../../arch/x86_64/io.zig");
const desktop_events = @import("../../kernel/desktop_events.zig");
const pic = @import("../../arch/x86_64/pic.zig");

pub const IRQ: u8 = 12;

const DATA_PORT: u16 = 0x60;
const STATUS_PORT: u16 = 0x64;
const COMMAND_PORT: u16 = 0x64;

const STATUS_OUTPUT_FULL: u8 = 0x01;
const STATUS_INPUT_FULL: u8 = 0x02;

const CMD_READ_CONFIG: u8 = 0x20;
const CMD_WRITE_CONFIG: u8 = 0x60;
const CMD_ENABLE_AUX: u8 = 0xA8;
const CMD_WRITE_AUX: u8 = 0xD4;

const MOUSE_SET_DEFAULTS: u8 = 0xF6;
const MOUSE_ENABLE_STREAMING: u8 = 0xF4;
const MOUSE_ACK: u8 = 0xFA;

const PACKET_LEN: usize = 3;
pub const CURSOR_W: usize = 12;
pub const CURSOR_H: usize = 16;

pub const PollHook = *const fn () callconv(.c) void;

pub const State = extern struct {
    x: i32,
    y: i32,
    dx: i32,
    dy: i32,
    wheel: i32,
    buttons: u8,
    packets: u64,
    present: bool,
};

// One atomic word contains two independent modulo2^32 sums. Unlike the
// legacy clamped coordinates and last-packet dx/dy, these preserve coalesced
// motion across display edges. No task lock is acquired in an IRQ callback.

pub fn motionTotals() u64 {
    return r4os_rust_input_mouse_motion();
}

pub const CursorOverlay = extern struct {
    visible: bool,
    x: i32,
    y: i32,
    max_x: i32,
    max_y: i32,
};

pub const Bounds = extern struct {
    width: u32,
    height: u32,
};

pub const Stats = extern struct {
    irq_count: u64,
    bytes_received: u64,
    packets: u64,
    usb_packets: u64,
    x: i32,
    y: i32,
    dx: i32,
    dy: i32,
    wheel: i32,
    buttons: u8,
    present: bool,
};

pub fn init(bounds: Bounds) bool {
    return r4os_rust_input_mouse_init(&bounds);
}

pub fn disable() void {
    r4os_rust_input_mouse_disable();
}

pub fn noteIrq() void {
    r4os_rust_input_mouse_irq();
}

pub fn onControllerByte(data: u8) void {
    r4os_rust_input_mouse_byte(data);
}

pub fn snapshot() State {
    var out: State = undefined;
    r4os_rust_input_mouse_snapshot(&out, false);
    return out;
}

pub fn stats() Stats {
    var out: Stats = undefined;
    r4os_rust_input_mouse_stats(&out);
    return out;
}

pub fn snapshotForApi() State {
    var out: State = undefined;
    r4os_rust_input_mouse_snapshot(&out, true);
    return out;
}

pub fn setPollHook(hook: ?PollHook) void {
    r4os_rust_input_mouse_poll_hook(hook);
}

pub fn injectRelativePacket(dx: i32, dy: i32, buttons: u8) void {
    r4os_rust_input_mouse_inject(dx, dy, buttons, 0);
}

pub fn injectRelativePacketWheel(dx: i32, dy: i32, buttons: u8, wheel: i32) void {
    r4os_rust_input_mouse_inject(dx, dy, buttons, wheel);
}

pub fn enableCursor() void {
    r4os_rust_input_mouse_enable_cursor();
}

pub fn disableCursor() void {
    r4os_rust_input_mouse_disable_cursor();
}

pub fn refreshCursor() void {
    r4os_rust_input_mouse_refresh_cursor();
}

pub fn cursorOverlay() CursorOverlay {
    var out: CursorOverlay = undefined;
    r4os_rust_input_mouse_cursor(&out);
    return out;
}

pub fn cursorMask(x: usize, y: usize) u8 {
    return r4os_rust_input_mouse_mask(x, y);
}

comptime {
    if (@sizeOf(State) != 40 or @alignOf(State) != 8 or @offsetOf(State, "x") != 0 or @offsetOf(State, "y") != 4 or @offsetOf(State, "dx") != 8 or @offsetOf(State, "dy") != 12 or @offsetOf(State, "wheel") != 16 or @offsetOf(State, "buttons") != 20 or @offsetOf(State, "packets") != 24 or @offsetOf(State, "present") != 32) @compileError("Native input State layout");
}

comptime {
    if (@sizeOf(CursorOverlay) != 20 or @alignOf(CursorOverlay) != 4 or @offsetOf(CursorOverlay, "visible") != 0 or @offsetOf(CursorOverlay, "x") != 4 or @offsetOf(CursorOverlay, "y") != 8 or @offsetOf(CursorOverlay, "max_x") != 12 or @offsetOf(CursorOverlay, "max_y") != 16) @compileError("Native input CursorOverlay layout");
}

comptime {
    if (@sizeOf(Bounds) != 8 or @alignOf(Bounds) != 4 or @offsetOf(Bounds, "width") != 0 or @offsetOf(Bounds, "height") != 4) @compileError("Native input Bounds layout");
}

comptime {
    if (@sizeOf(Stats) != 56 or @alignOf(Stats) != 8 or @offsetOf(Stats, "irq_count") != 0 or @offsetOf(Stats, "bytes_received") != 8 or @offsetOf(Stats, "packets") != 16 or @offsetOf(Stats, "usb_packets") != 24 or @offsetOf(Stats, "x") != 32 or @offsetOf(Stats, "y") != 36 or @offsetOf(Stats, "dx") != 40 or @offsetOf(Stats, "dy") != 44 or @offsetOf(Stats, "wheel") != 48 or @offsetOf(Stats, "buttons") != 52 or @offsetOf(Stats, "present") != 53) @compileError("Native input Stats layout");
}
