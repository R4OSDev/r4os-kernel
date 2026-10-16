//! The shared controller status AUX bit routes every byte, independently of
//! which IRQ woke the drain. Original drain bound and foreign IRQ counters.
use super::{keyboard, mouse};
use crate::arch::io;
use core::{
    ptr,
    sync::atomic::{AtomicU64, Ordering},
};
static COUNTS: [AtomicU64; 8] = [const { AtomicU64::new(0) }; 8];
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Stats {
    pub irq1_count: u64,
    pub irq12_count: u64,
    pub byte_count: u64,
    pub keyboard_byte_count: u64,
    pub mouse_byte_count: u64,
    pub keyboard_bytes_on_irq12: u64,
    pub mouse_bytes_on_irq1: u64,
    pub drain_limit_hits: u64,
}
fn inc(index: usize) {
    COUNTS[index].fetch_add(1, Ordering::Relaxed);
}
pub fn on_irq(irq: u8) {
    if irq == keyboard::IRQ {
        inc(0);
        keyboard::note_irq();
    } else if irq == mouse::IRQ {
        inc(1);
        mouse::note_irq();
    }
    for _ in 0..64 {
        let status = unsafe { io::inb(0x64) };
        if status & 1 == 0 {
            return;
        }
        route(irq, status, unsafe { io::inb(0x60) });
    }
    if unsafe { io::inb(0x64) } & 1 != 0 {
        inc(7);
    }
}
pub fn route(irq: u8, status: u8, byte: u8) {
    inc(2);
    if status & 0x20 != 0 {
        inc(4);
        if irq == keyboard::IRQ {
            inc(6);
        }
        mouse::controller_byte(byte);
        return;
    }
    inc(3);
    if irq == mouse::IRQ {
        inc(5);
    }
    keyboard::controller_byte(byte);
}
pub fn stats() -> Stats {
    let c = core::array::from_fn::<_, 8, _>(|i| COUNTS[i].load(Ordering::Relaxed));
    Stats {
        irq1_count: c[0],
        irq12_count: c[1],
        byte_count: c[2],
        keyboard_byte_count: c[3],
        mouse_byte_count: c[4],
        keyboard_bytes_on_irq12: c[5],
        mouse_bytes_on_irq1: c[6],
        drain_limit_hits: c[7],
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_i8042_irq(irq: u8) {
    on_irq(irq);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_i8042_route(irq: u8, status: u8, byte: u8) {
    route(irq, status, byte);
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller owns an aligned exclusive statistics output for the copied controller snapshot.
pub unsafe extern "C" fn r4os_rust_input_i8042_stats(out: *mut Stats) {
    unsafe { ptr::write(out, stats()) }
}

const _: () = {
    assert!(core::mem::size_of::<Stats>() == 64);
    assert!(core::mem::align_of::<Stats>() == 8);
    assert!(core::mem::offset_of!(Stats, irq1_count) == 0);
    assert!(core::mem::offset_of!(Stats, irq12_count) == 8);
    assert!(core::mem::offset_of!(Stats, byte_count) == 16);
    assert!(core::mem::offset_of!(Stats, keyboard_byte_count) == 24);
    assert!(core::mem::offset_of!(Stats, mouse_byte_count) == 32);
    assert!(core::mem::offset_of!(Stats, keyboard_bytes_on_irq12) == 40);
    assert!(core::mem::offset_of!(Stats, mouse_bytes_on_irq1) == 48);
    assert!(core::mem::offset_of!(Stats, drain_limit_hits) == 56);
};
