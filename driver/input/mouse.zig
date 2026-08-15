const io = @import("../../arch/x86_64/io.zig");
const desktop_events = @import("../../kernel/desktop_events.zig");
const pic = @import("../../arch/x86_64/pic.zig");

pub const IRQ: u8 = 12;

const DATA_PORT: u16 = 0x60;
const STATUS_PORT: u16 = 0x64;
const COMMAND_PORT: u16 = 0x64;

const STATUS_OUTPUT_FULL: u8 = 0x01;
const STATUS_INPUT_FULL: u8 = 0x02;
const STATUS_AUX_DATA: u8 = 0x20;

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

var state: State = .{
    .x = 0,
    .y = 0,
    .dx = 0,
    .dy = 0,
    .wheel = 0,
    .buttons = 0,
    .packets = 0,
    .present = false,
};

var max_x: i32 = 0;
var max_y: i32 = 0;
var packet: [PACKET_LEN]u8 = .{0} ** PACKET_LEN;
var packet_index: usize = 0;
var button_press_latch: u8 = 0;
var irq_count: u64 = 0;
var bytes_received: u64 = 0;
var usb_packets: u64 = 0;
var poll_hook: ?PollHook = null;

var cursor_visible = false;
var cursor_x: i32 = 0;
var cursor_y: i32 = 0;

pub const CursorOverlay = struct {
    visible: bool,
    x: i32,
    y: i32,
    max_x: i32,
    max_y: i32,
};

pub const Bounds = struct {
    width: u32,
    height: u32,
};

pub const Stats = struct {
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
    max_x = if (bounds.width == 0) 0 else @intCast(bounds.width - 1);
    max_y = if (bounds.height == 0) 0 else @intCast(bounds.height - 1);
    state.x = @divTrunc(max_x, 2);
    state.y = @divTrunc(max_y, 2);
    cursor_x = state.x;
    cursor_y = state.y;

    drainOutput();
    writeCommand(CMD_ENABLE_AUX);

    writeCommand(CMD_READ_CONFIG);
    var config = readDataWait() orelse return false;
    config |= 0x02; // IRQ12 enable
    config &= ~@as(u8, 0x20); // aux clock enable
    writeCommand(CMD_WRITE_CONFIG);
    writeData(config);

    if (!sendMouseCommand(MOUSE_SET_DEFAULTS)) return false;
    if (!sendMouseCommand(MOUSE_ENABLE_STREAMING)) return false;

    state.present = true;
    pic.unmask(IRQ);
    return true;
}

pub fn disable() void {
    disableCursor();
    pic.mask(IRQ);
    state.present = false;
    packet_index = 0;
    button_press_latch = 0;
}

pub fn onIrq() void {
    irq_count +%= 1;
    var guard: u32 = 0;
    while (guard < 16) : (guard += 1) {
        const status = io.inb(STATUS_PORT);
        if ((status & STATUS_OUTPUT_FULL) == 0) break;
        const data = io.inb(DATA_PORT);
        if ((status & STATUS_AUX_DATA) == 0) continue;
        bytes_received +%= 1;
        handleByte(data);
    }
}

pub fn snapshot() State {
    pollUsb();
    return state;
}

pub fn stats() Stats {
    pollUsb();
    return .{
        .irq_count = irq_count,
        .bytes_received = bytes_received,
        .packets = state.packets,
        .usb_packets = usb_packets,
        .x = state.x,
        .y = state.y,
        .dx = state.dx,
        .dy = state.dy,
        .wheel = state.wheel,
        .buttons = state.buttons,
        .present = state.present,
    };
}

pub fn snapshotForApi() State {
    pollUsb();
    var s = state;
    const latched = button_press_latch;
    if (latched != 0) {
        s.buttons |= latched;
        button_press_latch = 0;
    }
    state.wheel = 0;
    return s;
}

pub fn setPollHook(hook: ?PollHook) void {
    poll_hook = hook;
}

pub fn injectRelativePacket(dx: i32, dy: i32, buttons: u8) void {
    injectRelativePacketWheel(dx, dy, buttons, 0);
}

pub fn injectRelativePacketWheel(dx: i32, dy: i32, buttons: u8, wheel: i32) void {
    state.dx = dx;
    state.dy = dy;
    state.wheel += wheel;
    state.x = clamp(state.x + dx, 0, max_x);
    state.y = clamp(state.y + dy, 0, max_y);
    const new_buttons = buttons & 0x07;
    button_press_latch |= new_buttons & ~state.buttons;
    state.buttons = new_buttons;
    state.present = true;
    state.packets += 1;
    desktop_events.signal();
    usb_packets += 1;
    refreshCursor();
}

pub fn enableCursor() void {
    cursor_visible = true;
    cursor_x = state.x;
    cursor_y = state.y;
}

pub fn disableCursor() void {
    cursor_visible = false;
}

pub fn refreshCursor() void {
    if (!cursor_visible) return;
    cursor_x = state.x;
    cursor_y = state.y;
}

pub fn cursorOverlay() CursorOverlay {
    return .{
        .visible = cursor_visible,
        .x = cursor_x,
        .y = cursor_y,
        .max_x = max_x,
        .max_y = max_y,
    };
}

fn handleByte(byte: u8) void {
    if (packet_index == 0 and (byte & 0x08) == 0) return;
    packet[packet_index] = byte;
    packet_index += 1;
    if (packet_index < PACKET_LEN) return;
    packet_index = 0;
    handlePacket(packet);
}

fn handlePacket(p: [PACKET_LEN]u8) void {
    if ((p[0] & 0xC0) != 0) return;
    const dx = signExtend(p[1], (p[0] & 0x10) != 0);
    const dy = signExtend(p[2], (p[0] & 0x20) != 0);

    state.dx = dx;
    state.dy = -dy;
    state.x = clamp(state.x + dx, 0, max_x);
    state.y = clamp(state.y - dy, 0, max_y);
    const new_buttons = p[0] & 0x07;
    button_press_latch |= new_buttons & ~state.buttons;
    state.buttons = new_buttons;
    state.packets += 1;
    desktop_events.signal();
    refreshCursor();
}

pub fn cursorMask(x: usize, y: usize) u8 {
    if (x == 0 and y < 14) return 2;
    if (x == 1 and y < 12) return 1;
    if (x == y and y < 11) return 2;
    if (x + 1 == y and y < 12) return 1;
    if (y == 11 and x < 8) return 2;
    if (y == 12 and x < 7) return 1;
    if (x == 4 and y >= 10 and y < 16) return 2;
    if (x == 5 and y >= 11 and y < 16) return 1;
    if (x == 6 and y >= 13 and y < 16) return 2;
    return 0;
}

fn pollUsb() void {
    if (poll_hook) |hook| hook();
}

fn sendMouseCommand(command: u8) bool {
    writeCommand(CMD_WRITE_AUX);
    writeData(command);
    return (readDataWait() orelse 0) == MOUSE_ACK;
}

fn writeCommand(command: u8) void {
    waitInputClear();
    io.outb(COMMAND_PORT, command);
}

fn writeData(data: u8) void {
    waitInputClear();
    io.outb(DATA_PORT, data);
}

fn readDataWait() ?u8 {
    var guard: u32 = 0;
    while (guard < 100_000) : (guard += 1) {
        if ((io.inb(STATUS_PORT) & STATUS_OUTPUT_FULL) != 0) return io.inb(DATA_PORT);
    }
    return null;
}

fn waitInputClear() void {
    var guard: u32 = 0;
    while ((io.inb(STATUS_PORT) & STATUS_INPUT_FULL) != 0 and guard < 100_000) : (guard += 1) {
        io.wait();
    }
}

fn drainOutput() void {
    var guard: u32 = 0;
    while ((io.inb(STATUS_PORT) & STATUS_OUTPUT_FULL) != 0 and guard < 256) : (guard += 1) {
        _ = io.inb(DATA_PORT);
    }
}

fn signExtend(value: u8, negative: bool) i32 {
    var result: i32 = value;
    if (negative) result -= 256;
    return result;
}

fn clamp(value: i32, min: i32, max: i32) i32 {
    if (value < min) return min;
    if (value > max) return max;
    return value;
}
