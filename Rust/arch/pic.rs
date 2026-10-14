//! One PIC mask state. Programming is admitted by singleton boot or the
//! existing IRQ/runtime owner; atomics permit passive remote readers.
use super::io;
use core::sync::atomic::{AtomicU8, Ordering};
pub const MASTER_OFFSET: u8 = 0x20;
pub const SLAVE_OFFSET: u8 = 0x28;
static MASTER_MASK: AtomicU8 = AtomicU8::new(0xff);
static SLAVE_MASK: AtomicU8 = AtomicU8::new(0xff);
fn out(port: u16, value: u8) {
    unsafe { io::outb(port, value) };
}
fn wait() {
    out(0x80, 0);
}
fn set_masks() {
    out(0x21, MASTER_MASK.load(Ordering::Relaxed));
    out(0xa1, SLAVE_MASK.load(Ordering::Relaxed));
}
pub fn init() {
    MASTER_MASK.store(0xff, Ordering::Relaxed);
    SLAVE_MASK.store(0xff, Ordering::Relaxed);
    for (port, value) in [
        (0x20, 0x11),
        (0xa0, 0x11),
        (0x21, MASTER_OFFSET),
        (0xa1, SLAVE_OFFSET),
        (0x21, 0x04),
        (0xa1, 0x02),
        (0x21, 0x01),
        (0xa1, 0x01),
    ] {
        out(port, value);
        wait();
    }
    set_masks();
}
pub fn unmask(irq: u8) {
    if irq < 8 {
        MASTER_MASK.store(
            MASTER_MASK.load(Ordering::Relaxed) & !(1u8 << irq),
            Ordering::Relaxed,
        );
    } else {
        SLAVE_MASK.store(
            SLAVE_MASK.load(Ordering::Relaxed) & !(1u8 << (irq - 8)),
            Ordering::Relaxed,
        );
        MASTER_MASK.store(
            MASTER_MASK.load(Ordering::Relaxed) & !0x04,
            Ordering::Relaxed,
        );
    }
    set_masks();
}
pub fn mask(irq: u8) {
    if irq < 8 {
        MASTER_MASK.store(
            MASTER_MASK.load(Ordering::Relaxed) | (1u8 << irq),
            Ordering::Relaxed,
        );
    } else {
        SLAVE_MASK.store(
            SLAVE_MASK.load(Ordering::Relaxed) | (1u8 << (irq - 8)),
            Ordering::Relaxed,
        );
    }
    set_masks();
}
pub fn mask_all() {
    MASTER_MASK.store(0xff, Ordering::Relaxed);
    SLAVE_MASK.store(0xff, Ordering::Relaxed);
    set_masks();
}
pub fn end_of_interrupt(irq: u8) {
    if irq >= 8 {
        out(0xa0, 0x20);
    }
    out(0x20, 0x20);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_pic_control(operation: u8, irq: u8) -> u8 {
    match operation {
        0 => init(),
        1 => unmask(irq),
        2 => mask(irq),
        3 => mask_all(),
        4 => end_of_interrupt(irq),
        5 => return MASTER_MASK.load(Ordering::Relaxed),
        6 => return SLAVE_MASK.load(Ordering::Relaxed),
        _ => panic!("Invalid private PIC operation"),
    }
    0
}
