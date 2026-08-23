const io = @import("../../arch/x86_64/io.zig");
const desktop_events = @import("../../kernel/desktop_events.zig");
const key_layout = @import("key_layout.zig");
const pic = @import("../../arch/x86_64/pic.zig");
const time_core = @import("../../platform/time.zig");

pub const IRQ: u8 = 1;

const DATA_PORT: u16 = 0x60;
const STATUS_PORT: u16 = 0x64;
const STATUS_OUTPUT_FULL: u8 = 0x01;
const BUFFER_LEN: usize = 256;
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

var buf: [BUFFER_LEN]u32 = .{0} ** BUFFER_LEN;
var head: usize = 0;
var tail: usize = 0;
var shift_down = false;
var ctrl_down = false;
var alt_down = false;
var altgr_down = false;
var modifier_tick: u64 = 0;
var modifier_key_seen = false;
var modifier_stale_resets: u64 = 0;
var extended = false;
var layout: Layout = key_layout.default_layout;
var input_hook: ?InputHook = null;
var poll_hook: ?PollHook = null;
var irq_count: u64 = 0;
var scancode_count: u64 = 0;
var decoded_count: u64 = 0;
var unmapped_character_count: u64 = 0;
var layout_fallback_count: u64 = 0;
var push_attempt_count: u64 = 0;
var dropped_count: u64 = 0;
var queue_high_water: u32 = 0;

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
    drainOutput();
    pic.unmask(IRQ);
}

pub fn disable() void {
    pic.mask(IRQ);
    head = 0;
    tail = 0;
    shift_down = false;
    ctrl_down = false;
    alt_down = false;
    altgr_down = false;
    modifier_tick = 0;
    modifier_key_seen = false;
    extended = false;
    drainOutput();
}

pub fn noteIrq() void {
    irq_count +%= 1;
}

pub fn onControllerByte(data: u8) void {
    scancode_count +%= 1;
    handleScancode(data);
}

pub fn readChar() ?u8 {
    const codepoint = readCodepoint() orelse return null;
    if (codepoint > 0xff) return null;
    return @intCast(codepoint);
}

pub fn readCodepoint() ?u32 {
    if (poll_hook) |hook| hook();
    clearStaleModifiers();
    if (input_hook) |hook| {
        const hooked = hook();
        if (hooked != 0) return hooked;
    }
    if (head == tail) return null;
    const codepoint = buf[tail];
    tail = (tail + 1) % BUFFER_LEN;
    return codepoint;
}

pub fn pending() bool {
    return head != tail;
}

pub fn stats() Stats {
    return .{
        .irq_count = irq_count,
        .scancode_count = scancode_count,
        .decoded_count = decoded_count,
        .unmapped_character_count = unmapped_character_count,
        .layout_fallback_count = layout_fallback_count,
        .push_attempt_count = push_attempt_count,
        .dropped_count = dropped_count,
        .queue_capacity = BUFFER_LEN - 1,
        .queue_pending = queuePendingCount(),
        .queue_high_water = queue_high_water,
        .modifiers = modifierBits(),
        .modifier_stale_resets = modifier_stale_resets,
        .pending = pending(),
        .layout = layout,
        .layout_name = key_layout.name(layout),
        .layout_display = key_layout.display(layout),
    };
}

pub fn setLayout(new_layout: Layout) void {
    layout = new_layout;
}

pub fn setLayoutByName(name: []const u8) bool {
    if (key_layout.parseName(name)) |parsed| {
        layout = parsed;
        return true;
    }
    layout = key_layout.default_layout;
    layout_fallback_count +%= 1;
    return false;
}

pub fn trySetLayoutByName(name: []const u8) bool {
    const parsed = key_layout.parseName(name) orelse return false;
    layout = parsed;
    return true;
}

pub fn activeLayoutName() []const u8 {
    return key_layout.name(layout);
}

pub fn activeLayoutDisplay() []const u8 {
    return key_layout.display(layout);
}

pub fn activeLayoutIndex() usize {
    return key_layout.indexOf(layout) orelse 0;
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
    input_hook = hook;
}

pub fn setPollHook(hook: ?PollHook) void {
    poll_hook = hook;
}

pub fn injectScancode(scancode: u8) void {
    scancode_count +%= 1;
    handleScancode(scancode);
}

fn handleScancode(scancode: u8) void {
    if (scancode == 0xE0) {
        extended = true;
        return;
    }
    if (extended) {
        extended = false;
        switch (scancode) {
            0x38 => {
                altgr_down = true;
                markModifierEvent();
                return;
            },
            0xB8 => {
                altgr_down = false;
                markModifierEvent();
                return;
            },
            0x1D => {
                ctrl_down = true;
                markModifierEvent();
                return;
            },
            0x9D => {
                ctrl_down = false;
                markModifierEvent();
                return;
            },
            else => {},
        }
        if ((scancode & 0x80) != 0) return;
        markNonModifierKey();
        switch (scancode) {
            0x48 => push(KEY_UP),
            0x50 => push(KEY_DOWN),
            0x4B => push(KEY_LEFT),
            0x4D => push(KEY_RIGHT),
            0x47 => push(KEY_HOME),
            0x4F => push(KEY_END),
            0x49 => push(KEY_PAGE_UP),
            0x51 => push(KEY_PAGE_DOWN),
            0x53 => push(KEY_DELETE),
            0x5B, 0x5C => push(KEY_START_MENU),
            else => {},
        }
        return;
    }

    switch (scancode) {
        0x2A, 0x36 => {
            shift_down = true;
            markModifierEvent();
            return;
        },
        0xAA, 0xB6 => {
            shift_down = false;
            markModifierEvent();
            return;
        },
        0x1D => {
            ctrl_down = true;
            markModifierEvent();
            return;
        },
        0x9D => {
            ctrl_down = false;
            markModifierEvent();
            return;
        },
        0x38 => {
            alt_down = true;
            markModifierEvent();
            return;
        },
        0xB8 => {
            alt_down = false;
            markModifierEvent();
            return;
        },
        else => {},
    }

    if ((scancode & 0x80) != 0) return;
    markNonModifierKey();
    if (scancode == 0x01 and ctrl_down) {
        push(KEY_CTRL_ESC);
        return;
    }
    if (scancode == 0x3D) {
        push(KEY_F3);
        return;
    }
    if (scancode == 0x0F and alt_down) {
        push(if (shift_down) KEY_ALT_SHIFT_TAB else KEY_ALT_TAB);
        return;
    }
    if (scancode == 0x3E and alt_down) {
        push(KEY_ALT_F4);
        return;
    }
    if (scancode == 0x0F and shift_down) {
        push(KEY_SHIFT_TAB);
        return;
    }
    if (scancodeToCodepoint(scancode, shift_down, ctrl_down, altgr_down)) |codepoint| {
        push(codepoint);
    }
}

fn push(codepoint: u32) void {
    push_attempt_count +%= 1;
    const next = (head + 1) % BUFFER_LEN;
    if (next == tail) {
        dropped_count +%= 1;
        return;
    }
    buf[head] = codepoint;
    head = next;
    decoded_count +%= 1;
    const queued = queuePendingCount();
    if (queued > queue_high_water) queue_high_water = queued;
    // 0.56.28: Desktop-Aktivitaets-Event (weckt desktopActivityWait).
    desktop_events.signal();
}

fn queuePendingCount() u32 {
    return @intCast(if (head >= tail) head - tail else BUFFER_LEN - tail + head);
}

fn drainOutput() void {
    var guard: u32 = 0;
    while ((io.inb(STATUS_PORT) & STATUS_OUTPUT_FULL) != 0 and guard < 256) : (guard += 1) {
        _ = io.inb(DATA_PORT);
    }
}

fn markModifierEvent() void {
    modifier_tick = time_core.monotonicTicks();
    modifier_key_seen = false;
}

fn markNonModifierKey() void {
    if (ctrl_down or alt_down or altgr_down) modifier_key_seen = true;
}

fn clearStaleModifiers() void {
    if (!modifier_key_seen) return;
    if (!ctrl_down and !alt_down and !altgr_down) return;
    const hz = time_core.monotonicFrequency();
    if (hz == 0 or modifier_tick == 0) return;
    const now = time_core.monotonicTicks();
    if (now < modifier_tick) {
        modifier_tick = now;
        return;
    }
    const timeout = @max(@as(u64, 1), (@as(u64, hz) * MODIFIER_STALE_MS) / 1000);
    if (now - modifier_tick < timeout) return;
    ctrl_down = false;
    alt_down = false;
    altgr_down = false;
    extended = false;
    modifier_key_seen = false;
    modifier_stale_resets +%= 1;
}

fn modifierBits() u8 {
    return (if (shift_down) @as(u8, 0x01) else 0) |
        (if (ctrl_down) @as(u8, 0x02) else 0) |
        (if (alt_down) @as(u8, 0x04) else 0) |
        (if (altgr_down) @as(u8, 0x08) else 0);
}

fn scancodeToCodepoint(scancode: u8, shifted: bool, ctrl: bool, altgr: bool) ?u32 {
    if (ctrl) {
        if (key_layout.ctrlChar(layout, scancode)) |c| return c;
    }
    const mapped = key_layout.translate(layout, scancode, .{ .shift = shifted, .altgr = altgr });
    if (mapped.hasCharacter()) return mapped.codepoint;
    if (mapped.ascii != 0) return mapped.ascii;
    return null;
}
