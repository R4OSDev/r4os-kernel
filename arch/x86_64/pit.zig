const io = @import("io.zig");
const pic = @import("pic.zig");

const PIT_CHANNEL0: u16 = 0x40;
const PIT_COMMAND: u16 = 0x43;
const PIT_BASE_HZ: u32 = 1_193_182;

pub const IRQ: u8 = 0;
// 0.56.40: Tickrate 1000 Hz (Wiederanlauf des 0.56.29-Versuchs).
// Voraussetzungen diesmal erfuellt: alle Tick-Konstanten hz-neutral
// in ms definiert (scheduler/page_cache/net/desktop_events/r4desk,
// RTL8139-Dead-Slot auf Echtzeit, AC97/HDA/MemorySuite/Diagnostics
// via msTicks), und der IF=0-Ring der Endlos-yield-Schleifen ist
// gefixt (exitCurrent/waitForShellAfterBoot hlt'en im Idle-Fall).
pub const DEFAULT_HZ: u32 = 1000;

var ticks: u64 = 0;
var hz: u32 = DEFAULT_HZ;

pub fn init(requested_hz: u32) void {
    hz = if (requested_hz == 0) DEFAULT_HZ else requested_hz;
    var divisor = PIT_BASE_HZ / hz;
    if (divisor == 0) divisor = 1;
    if (divisor > 0xFFFF) divisor = 0xFFFF;

    io.outb(PIT_COMMAND, 0x36); // channel 0, lo/hi, mode 3
    io.outb(PIT_CHANNEL0, @truncate(divisor));
    io.outb(PIT_CHANNEL0, @truncate(divisor >> 8));
    pic.unmask(IRQ);
}

pub fn onTick() void {
    const p: *volatile u64 = &ticks;
    p.* +%= 1;
}

pub fn tickCount() u64 {
    const p: *volatile u64 = &ticks;
    return p.*;
}

pub fn frequency() u32 {
    return hz;
}
