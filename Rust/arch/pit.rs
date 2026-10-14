//! Original PIT divisor/rate and wrapping epoch. IRQ0 writes one counter;
//! remote clock readers use the same resident atomic storage.
use super::{io, pic};
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
pub const BASE_HZ: u32 = 1_193_182;
pub const DEFAULT_HZ: u32 = 1000;
pub const IRQ: u8 = 0;
static TICKS: AtomicU64 = AtomicU64::new(0);
static HZ: AtomicU32 = AtomicU32::new(DEFAULT_HZ);
static DIVISOR: AtomicU32 = AtomicU32::new(BASE_HZ / DEFAULT_HZ);
pub fn init(requested: u32) {
    let hz = if requested == 0 {
        DEFAULT_HZ
    } else {
        requested
    };
    HZ.store(hz, Ordering::Relaxed);
    let divisor = (BASE_HZ / hz).clamp(1, 0xffff);
    DIVISOR.store(divisor, Ordering::Relaxed);
    unsafe {
        io::outb(0x43, 0x36);
        io::outb(0x40, divisor as u8);
        io::outb(0x40, (divisor >> 8) as u8);
    }
    pic::unmask(IRQ);
}
pub fn on_tick() {
    TICKS.fetch_add(1, Ordering::Relaxed);
}
pub fn tick_count() -> u64 {
    TICKS.load(Ordering::Relaxed)
}
pub fn frequency() -> u32 {
    HZ.load(Ordering::Relaxed)
}
pub fn divisor() -> u32 {
    DIVISOR.load(Ordering::Relaxed)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_pit_control(operation: u8, argument: u32) -> u64 {
    match operation {
        0 => init(argument),
        1 => on_tick(),
        2 => return tick_count(),
        3 => return u64::from(frequency()),
        4 => return u64::from(divisor()),
        _ => panic!("Invalid private PIT operation"),
    }
    0
}
