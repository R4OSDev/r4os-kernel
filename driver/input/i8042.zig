comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
extern fn r4os_rust_input_i8042_irq(u8) void;
extern fn r4os_rust_input_i8042_route(u8, u8, u8) void;
extern fn r4os_rust_input_i8042_stats(*Stats) void;
const io = @import("../../arch/x86_64/io.zig");
const keyboard = @import("keyboard.zig");
const mouse = @import("mouse.zig");

const data_port: u16 = 0x60;
const status_port: u16 = 0x64;
const status_output_full: u8 = 0x01;
const status_aux_data: u8 = 0x20;
const drain_limit: u32 = 64;

pub const Stats = extern struct {
    irq1_count: u64,
    irq12_count: u64,
    byte_count: u64,
    keyboard_byte_count: u64,
    mouse_byte_count: u64,
    keyboard_bytes_on_irq12: u64,
    mouse_bytes_on_irq1: u64,
    drain_limit_hits: u64,
};

/// The i8042 owns the shared output port. IRQ1 and IRQ12 are only wake
/// reasons; the AUX status bit on each byte decides which decoder owns it.
pub fn onIrq(irq: u8) void {
    r4os_rust_input_i8042_irq(irq);
}

fn routeControllerByte(irq: u8, status: u8, byte: u8) void {
    r4os_rust_input_i8042_route(irq, status, byte);
}

pub fn stats() Stats {
    var out: Stats = undefined;
    r4os_rust_input_i8042_stats(&out);
    return out;
}

test "i8042 routes foreign IRQ bytes by controller status" {
    const std = @import("std");
    const before = stats();
    const keyboard_before = keyboard.stats();
    const mouse_before = mouse.stats();

    routeControllerByte(mouse.IRQ, 0, 0x9e);
    routeControllerByte(keyboard.IRQ, status_aux_data, 0x08);

    const after = stats();
    try std.testing.expectEqual(before.byte_count + 2, after.byte_count);
    try std.testing.expectEqual(before.keyboard_bytes_on_irq12 + 1, after.keyboard_bytes_on_irq12);
    try std.testing.expectEqual(before.mouse_bytes_on_irq1 + 1, after.mouse_bytes_on_irq1);
    try std.testing.expectEqual(keyboard_before.scancode_count + 1, keyboard.stats().scancode_count);
    try std.testing.expectEqual(mouse_before.bytes_received + 1, mouse.stats().bytes_received);
}

comptime {
    if (@sizeOf(Stats) != 64 or @alignOf(Stats) != 8 or @offsetOf(Stats, "irq1_count") != 0 or @offsetOf(Stats, "irq12_count") != 8 or @offsetOf(Stats, "byte_count") != 16 or @offsetOf(Stats, "keyboard_byte_count") != 24 or @offsetOf(Stats, "mouse_byte_count") != 32 or @offsetOf(Stats, "keyboard_bytes_on_irq12") != 40 or @offsetOf(Stats, "mouse_bytes_on_irq1") != 48 or @offsetOf(Stats, "drain_limit_hits") != 56) @compileError("Native input Stats layout");
}
