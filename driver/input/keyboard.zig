// Native keyboard owner; only typed C projection remains here.
extern fn r4os_rust_input_keyboard_init() void;
extern fn r4os_rust_input_keyboard_disable() void;
extern fn r4os_rust_input_keyboard_irq() void;
extern fn r4os_rust_input_keyboard_byte(u8) void;
extern fn r4os_rust_input_keyboard_read(*u32) bool;
extern fn r4os_rust_input_keyboard_read_char(*u8) bool;
extern fn r4os_rust_input_keyboard_physical(*PhysicalEvent) bool;
extern fn r4os_rust_input_keyboard_physical_stats(*PhysicalStats) void;
extern fn r4os_rust_input_keyboard_pending() bool;
extern fn r4os_rust_input_keyboard_generation() u64;
extern fn r4os_rust_input_keyboard_signal() void;
extern fn r4os_rust_input_keyboard_wait(u64, u64, *u64) u8;
extern fn r4os_rust_input_keyboard_free() u32;
extern fn r4os_rust_input_keyboard_accept(u32) bool;
extern fn r4os_rust_input_keyboard_stats(*NativeStats) void;
extern fn r4os_rust_input_keyboard_layout(u32) void;
extern fn r4os_rust_input_keyboard_layout_name([*]const u8, usize, bool) bool;
extern fn r4os_rust_input_keyboard_layout_id() u32;
extern fn r4os_rust_input_keyboard_input_hook(?InputHook) void;
extern fn r4os_rust_input_keyboard_poll_hook(?PollHook) void;
const io = @import("../../arch/x86_64/io.zig");
const desktop_events = @import("../../kernel/desktop_events.zig");
const key_layout = @import("key_layout.zig");
const codepoint_queue = @import("codepoint_queue.zig");
const physical_key = @import("physical_key.zig");
const pic = @import("../../arch/x86_64/pic.zig");
const time_core = @import("../../platform/time.zig");
const sync = @import("../../sched/sync.zig");

pub const IRQ: u8 = 1;

const DATA_PORT: u16 = 0x60;
const STATUS_PORT: u16 = 0x64;
const STATUS_OUTPUT_FULL: u8 = 0x01;
const MODIFIER_STALE_MS: u64 = 750;

pub const InputHook = *const fn () callconv(.c) u8;
pub const PollHook = *const fn () callconv(.c) void;

pub const KEY_UP: u8 = 0x80;
pub const KEY_DOWN: u8 = 0x81;
pub const KEY_F3: u8 = 0x82;
pub const KEY_CTRL_ESC: u8 = 0x83;
pub const KEY_SHIFT_TAB: u8 = 0x84;
pub const KEY_ALT_TAB: u8 = 0x85;
pub const KEY_ALT_F4: u8 = 0x86;
pub const KEY_ALT_SHIFT_TAB: u8 = 0x87;
pub const KEY_LEFT: u8 = 0x88;
pub const KEY_RIGHT: u8 = 0x89;
pub const KEY_HOME: u8 = 0x8A;
pub const KEY_END: u8 = 0x8B;
pub const KEY_START_MENU: u8 = 0x8C;
pub const KEY_PAGE_UP: u8 = 0x8D;
pub const KEY_PAGE_DOWN: u8 = 0x8E;
pub const KEY_DELETE: u8 = 0x7F;

pub const Layout = key_layout.Layout;

pub const Stats = struct {
    irq_count: u64,
    scancode_count: u64,
    decoded_count: u64,
    unmapped_character_count: u64,
    layout_fallback_count: u64,
    push_attempt_count: u64,
    dropped_count: u64,
    queue_capacity: u32,
    queue_pending: u32,
    queue_high_water: u32,
    modifiers: u8,
    modifier_stale_resets: u64,
    pending: bool,
    layout: Layout,
    layout_name: []const u8,
    layout_display: []const u8,
};

pub fn init() void {
    r4os_rust_input_keyboard_init();
}

pub fn disable() void {
    r4os_rust_input_keyboard_disable();
}

pub fn noteIrq() void {
    r4os_rust_input_keyboard_irq();
}

pub fn onControllerByte(data: u8) void {
    r4os_rust_input_keyboard_byte(data);
}

pub fn readChar() ?u8 {
    var out: u8 = 0;
    return if (r4os_rust_input_keyboard_read_char(&out)) out else null;
}

pub fn readCodepoint() ?u32 {
    var out: u32 = 0;
    return if (r4os_rust_input_keyboard_read(&out)) out else null;
}

pub const PhysicalEvent = physical_key.Event;
pub const PhysicalStats = physical_key.Snapshot;

pub fn readPhysicalEvent() ?PhysicalEvent {
    var out: PhysicalEvent = undefined;
    return if (r4os_rust_input_keyboard_physical(&out)) out else null;
}

pub fn physicalStats() PhysicalStats {
    var out: PhysicalStats = undefined;
    r4os_rust_input_keyboard_physical_stats(&out);
    return out;
}

pub fn pending() bool {
    return r4os_rust_input_keyboard_pending();
}

pub fn inputGeneration() u64 {
    return r4os_rust_input_keyboard_generation();
}

pub fn signalInputActivity() void {
    r4os_rust_input_keyboard_signal();
}

pub fn waitInput(last_generation: u64, timeout_ticks: u64, out_generation: *u64) sync.WaitResult {
    return @enumFromInt(r4os_rust_input_keyboard_wait(last_generation, timeout_ticks, out_generation));
}

pub fn queueFreeCount() u32 {
    return r4os_rust_input_keyboard_free();
}

pub fn canAccept(count: u32) bool {
    return r4os_rust_input_keyboard_accept(count);
}

pub fn stats() Stats {
    var out: NativeStats = undefined;
    r4os_rust_input_keyboard_stats(&out);
    return .{ .irq_count = out.irq_count, .scancode_count = out.scancode_count, .decoded_count = out.decoded_count, .unmapped_character_count = out.unmapped_character_count, .layout_fallback_count = out.layout_fallback_count, .push_attempt_count = out.push_attempt_count, .dropped_count = out.dropped_count, .queue_capacity = out.queue_capacity, .queue_pending = out.queue_pending, .queue_high_water = out.queue_high_water, .modifiers = out.modifiers, .modifier_stale_resets = out.modifier_stale_resets, .pending = out.pending, .layout = @enumFromInt(out.layout), .layout_name = out.layout_name.slice(), .layout_display = out.layout_display.slice() };
}

pub fn setLayout(new_layout: Layout) void {
    r4os_rust_input_keyboard_layout(@intFromEnum(new_layout));
}

pub fn setLayoutByName(name: []const u8) bool {
    return r4os_rust_input_keyboard_layout_name(name.ptr, name.len, true);
}

pub fn trySetLayoutByName(name: []const u8) bool {
    return r4os_rust_input_keyboard_layout_name(name.ptr, name.len, false);
}

pub fn activeLayoutName() []const u8 {
    return key_layout.name(@enumFromInt(r4os_rust_input_keyboard_layout_id()));
}

pub fn activeLayoutDisplay() []const u8 {
    return key_layout.display(@enumFromInt(r4os_rust_input_keyboard_layout_id()));
}

pub fn activeLayoutIndex() usize {
    return r4os_rust_input_keyboard_layout_id();
}

pub fn layoutCount() usize {
    return key_layout.count();
}

pub fn layoutAt(index: usize) ?Layout {
    return key_layout.at(index);
}

pub fn layoutNameAt(index: usize) ?[]const u8 {
    const item = layoutAt(index) orelse return null;
    return key_layout.name(item);
}

pub fn layoutDisplayAt(index: usize) ?[]const u8 {
    const item = layoutAt(index) orelse return null;
    return key_layout.display(item);
}

pub fn setInputHook(hook: ?InputHook) void {
    r4os_rust_input_keyboard_input_hook(hook);
}

pub fn setPollHook(hook: ?PollHook) void {
    r4os_rust_input_keyboard_poll_hook(hook);
}

pub fn injectScancode(scancode: u8) void {
    r4os_rust_input_keyboard_byte(scancode);
}

const Text = @import("key_layout_types.zig").Text;
const NativeStats = extern struct {
    irq_count: u64,
    scancode_count: u64,
    decoded_count: u64,
    unmapped_character_count: u64,
    layout_fallback_count: u64,
    push_attempt_count: u64,
    dropped_count: u64,
    queue_capacity: u32,
    queue_pending: u32,
    queue_high_water: u32,
    modifiers: u8,
    modifier_stale_resets: u64,
    pending: bool,
    layout: u32,
    layout_name: Text,
    layout_display: Text,
};

comptime {
    if (@sizeOf(NativeStats) != 120 or @alignOf(NativeStats) != 8 or @offsetOf(NativeStats, "modifier_stale_resets") != 72 or @offsetOf(NativeStats, "layout") != 84 or @offsetOf(NativeStats, "layout_name") != 88 or @offsetOf(NativeStats, "layout_display") != 104) @compileError("Native keyboard snapshot C layout");
}
