// Early PS/2 input foundation for kernel startup.
//
// This layer wraps boot orchestration for the current PS/2 path. The actual
// keyboard and mouse drivers live under driver/input. This file stays boot
// orchestration and is not a general input subsystem.

const driver_registry = @import("../driver/registry.zig");
const keyboard = @import("../driver/input/keyboard.zig");
const mouse = @import("../driver/input/mouse.zig");
const display = @import("../display/display.zig");

pub const MouseStatus = enum(u8) {
    not_started,
    ready,
    not_found,
    no_display,
};

pub fn initKeyboard() void {
    r4os_rust_input_boot_keyboard();
}

pub fn isKeyboardInitialized() bool {
    return r4os_rust_input_boot_keyboard_ready();
}

pub fn initMouse() void {
    r4os_rust_input_boot_mouse();
}

pub fn mouseStatus() MouseStatus {
    return @enumFromInt(r4os_rust_input_boot_mouse_status());
}

pub fn mouseStatusLine() []const u8 {
    return switch (mouseStatus()) {
        .not_started => "[not ready]\r\n",
        .ready => "[OK]\r\n",
        .not_found => "[not found]\r\n",
        .no_display => "[no display]\r\n",
    };
}

pub fn completePs2() void {
    r4os_rust_input_boot_complete();
}

extern fn r4os_rust_input_boot_keyboard() void;
extern fn r4os_rust_input_boot_keyboard_ready() bool;
extern fn r4os_rust_input_boot_mouse() void;
extern fn r4os_rust_input_boot_mouse_status() u8;
extern fn r4os_rust_input_boot_complete() void;
export fn r4os_zig_input_boot_registry_begin() callconv(.c) usize {
    return driver_registry.beginLoad("PS2", 3, 1) orelse ~@as(usize, 0);
}
export fn r4os_zig_input_boot_registry_state(slot: usize, state: u8) callconv(.c) void {
    driver_registry.setState(slot, @enumFromInt(state));
}
export fn r4os_zig_input_boot_mode(out: *mouse.Bounds) callconv(.c) bool {
    const mode = display.activeMode() orelse return false;
    out.* = .{ .width = mode.width, .height = mode.height };
    return true;
}
