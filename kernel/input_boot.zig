// Early PS/2 input foundation for kernel startup.
//
// This layer wraps boot orchestration for the current PS/2 path. The actual
// keyboard and mouse drivers live under driver/input. This file stays boot
// orchestration and is not a general input subsystem.

const driver_registry = @import("../driver/registry.zig");
const keyboard = @import("../driver/input/keyboard.zig");
const mouse = @import("../driver/input/mouse.zig");
const display = @import("../display/display.zig");

var ps2_slot: ?usize = null;
var keyboard_initialized = false;
var ps2_completed = false;
var mouse_status: MouseStatus = .not_started;

pub const MouseStatus = enum {
    not_started,
    ready,
    not_found,
    no_display,
};

pub fn initKeyboard() void {
    if (keyboard_initialized) return;

    ensurePs2Slot();
    keyboard.init();
    keyboard_initialized = true;
}

pub fn isKeyboardInitialized() bool {
    return keyboard_initialized;
}

pub fn initMouse() void {
    if (mouse_status != .not_started) return;

    const mode = display.activeMode() orelse {
        mouse_status = .no_display;
        return;
    };
    const bounds = mouse.Bounds{ .width = mode.width, .height = mode.height };
    mouse_status = if (mouse.init(bounds)) .ready else .not_found;
}

pub fn mouseStatus() MouseStatus {
    return mouse_status;
}

pub fn mouseStatusLine() []const u8 {
    return switch (mouse_status) {
        .not_started => "[not ready]\r\n",
        .ready => "[OK]\r\n",
        .not_found => "[not found]\r\n",
        .no_display => "[no display]\r\n",
    };
}

pub fn completePs2() void {
    if (ps2_completed) return;

    if (ps2_slot) |slot| {
        if (keyboard_initialized) {
            driver_registry.setState(slot, .initialized);
            driver_registry.setState(slot, .active);
        } else {
            driver_registry.setState(slot, .failed);
        }
    }

    ps2_completed = true;
}

fn ensurePs2Slot() void {
    if (ps2_slot != null) return;
    ps2_slot = driver_registry.beginLoad("PS2", 3, 1);
}
