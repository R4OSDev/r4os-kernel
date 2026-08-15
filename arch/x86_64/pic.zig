const io = @import("io.zig");

const PIC1_COMMAND: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_COMMAND: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;

const ICW1_INIT: u8 = 0x10;
const ICW1_ICW4: u8 = 0x01;
const ICW4_8086: u8 = 0x01;
const EOI: u8 = 0x20;

pub const MASTER_OFFSET: u8 = 0x20;
pub const SLAVE_OFFSET: u8 = 0x28;

var master_mask: u8 = 0xFF;
var slave_mask: u8 = 0xFF;

pub fn init() void {
    master_mask = 0xFF;
    slave_mask = 0xFF;

    io.outb(PIC1_COMMAND, ICW1_INIT | ICW1_ICW4);
    io.wait();
    io.outb(PIC2_COMMAND, ICW1_INIT | ICW1_ICW4);
    io.wait();

    io.outb(PIC1_DATA, MASTER_OFFSET);
    io.wait();
    io.outb(PIC2_DATA, SLAVE_OFFSET);
    io.wait();

    io.outb(PIC1_DATA, 0x04);
    io.wait();
    io.outb(PIC2_DATA, 0x02);
    io.wait();

    io.outb(PIC1_DATA, ICW4_8086);
    io.wait();
    io.outb(PIC2_DATA, ICW4_8086);
    io.wait();

    setMasks(master_mask, slave_mask);
}

pub fn unmask(irq: u8) void {
    if (irq < 8) {
        master_mask &= ~(@as(u8, 1) << @intCast(irq));
    } else {
        slave_mask &= ~(@as(u8, 1) << @intCast(irq - 8));
        master_mask &= ~@as(u8, 0x04);
    }
    setMasks(master_mask, slave_mask);
}

pub fn mask(irq: u8) void {
    if (irq < 8) {
        master_mask |= @as(u8, 1) << @intCast(irq);
    } else {
        slave_mask |= @as(u8, 1) << @intCast(irq - 8);
    }
    setMasks(master_mask, slave_mask);
}

pub fn maskAll() void {
    master_mask = 0xFF;
    slave_mask = 0xFF;
    setMasks(master_mask, slave_mask);
}

pub fn endOfInterrupt(irq: u8) void {
    if (irq >= 8) io.outb(PIC2_COMMAND, EOI);
    io.outb(PIC1_COMMAND, EOI);
}

pub fn masterMask() u8 {
    return master_mask;
}

pub fn slaveMask() u8 {
    return slave_mask;
}

fn setMasks(master: u8, slave: u8) void {
    io.outb(PIC1_DATA, master);
    io.outb(PIC2_DATA, slave);
}
