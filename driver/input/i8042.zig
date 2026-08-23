const io = @import("../../arch/x86_64/io.zig");
const keyboard = @import("keyboard.zig");
const mouse = @import("mouse.zig");

const data_port: u16 = 0x60;
const status_port: u16 = 0x64;
const status_output_full: u8 = 0x01;
const status_aux_data: u8 = 0x20;
const drain_limit: u32 = 64;

var irq1_count: u64 = 0;
var irq12_count: u64 = 0;
var byte_count: u64 = 0;
var keyboard_byte_count: u64 = 0;
var mouse_byte_count: u64 = 0;
var keyboard_bytes_on_irq12: u64 = 0;
var mouse_bytes_on_irq1: u64 = 0;
var drain_limit_hits: u64 = 0;

pub const Stats = struct {
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
    if (irq == keyboard.IRQ) {
        irq1_count +%= 1;
        keyboard.noteIrq();
    } else if (irq == mouse.IRQ) {
        irq12_count +%= 1;
        mouse.noteIrq();
    }

    var drained: u32 = 0;
    while (drained < drain_limit) : (drained += 1) {
        const status = io.inb(status_port);
        if ((status & status_output_full) == 0) return;
        routeControllerByte(irq, status, io.inb(data_port));
    }
    if ((io.inb(status_port) & status_output_full) != 0) drain_limit_hits +%= 1;
}

fn routeControllerByte(irq: u8, status: u8, byte: u8) void {
    byte_count +%= 1;
    if ((status & status_aux_data) != 0) {
        mouse_byte_count +%= 1;
        if (irq == keyboard.IRQ) mouse_bytes_on_irq1 +%= 1;
        mouse.onControllerByte(byte);
        return;
    }
    keyboard_byte_count +%= 1;
    if (irq == mouse.IRQ) keyboard_bytes_on_irq12 +%= 1;
    keyboard.onControllerByte(byte);
}

pub fn stats() Stats {
    return .{
        .irq1_count = irq1_count,
        .irq12_count = irq12_count,
        .byte_count = byte_count,
        .keyboard_byte_count = keyboard_byte_count,
        .mouse_byte_count = mouse_byte_count,
        .keyboard_bytes_on_irq12 = keyboard_bytes_on_irq12,
        .mouse_bytes_on_irq1 = mouse_bytes_on_irq1,
        .drain_limit_hits = drain_limit_hits,
    };
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
