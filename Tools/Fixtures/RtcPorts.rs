//! Native fixture: scheduled port interleaving, RTC freeze and injected faults.
//! The real x86 IRQ/ranked admission is covered by the separate KVM4 check.
use std::{
    cell::Cell,
    sync::{
        Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
static OWNER: Mutex<()> = Mutex::new(());
thread_local! { static HELD: Cell<bool> = const { Cell::new(false) }; }
pub struct Guard {
    _owner: MutexGuard<'static, ()>,
}
pub fn rtc_guard() -> Guard {
    let owner = OWNER.lock().unwrap();
    HELD.with(|v| assert!(!v.replace(true)));
    Guard { _owner: owner }
}
impl Drop for Guard {
    fn drop(&mut self) {
        HELD.with(|v| assert!(v.replace(false)));
    }
}
pub static CLOCK: AtomicU64 = AtomicU64::new(0);
pub static CLOCK_VALID: AtomicBool = AtomicBool::new(true);
pub fn now_nanoseconds() -> Option<u64> {
    CLOCK_VALID
        .load(Ordering::Relaxed)
        .then(|| CLOCK.load(Ordering::Relaxed))
}
pub struct Model {
    pub registers: [u8; 128],
    index: u8,
    pub reads: usize,
    pub calendar_writes: usize,
    pub change_century: bool,
    pub ignore_write: Option<u8>,
}
impl Model {
    const EMPTY: Self = Self {
        registers: [0; 128],
        index: 0,
        reads: 0,
        calendar_writes: 0,
        change_century: false,
        ignore_write: None,
    };
}
pub static PORTS: Mutex<Model> = Mutex::new(Model::EMPTY);
pub fn reset() {
    let mut ports = PORTS.lock().unwrap();
    *ports = Model::EMPTY;
    for (r, v) in [
        (0, 56),
        (2, 34),
        (4, 12),
        (6, 1),
        (7, 25),
        (8, 10),
        (9, 26),
        (0x32, 20),
        (0xb, 6),
        (0xd, 0x80),
    ] {
        ports.registers[r] = v;
    }
    CLOCK.store(0, Ordering::Relaxed);
    CLOCK_VALID.store(true, Ordering::Relaxed);
}
pub unsafe fn outb(port: u16, value: u8) {
    HELD.with(|v| assert!(v.get(), "CMOS instruction escaped canonical owner"));
    {
        let mut ports = PORTS.lock().unwrap();
        match port {
            0x70 => {
                assert_ne!(value & 0x80, 0, "NMI policy changed");
                ports.index = value & 0x7f;
            }
            0x71 => {
                let index = ports.index as usize;
                if index != 0xb {
                    assert_ne!(ports.registers[0xb] & 0x80, 0, "calendar write without SET");
                    ports.calendar_writes += 1;
                }
                if ports.ignore_write != Some(index as u8) {
                    ports.registers[index] = if index == 0xb && value & 0x80 != 0 {
                        value & !0x10
                    } else {
                        value
                    };
                }
            }
            0x80 => {}
            _ => panic!("Unexpected RTC port"),
        }
    }
    // A competitor is given a real scheduling opportunity after the index.
    if port == 0x70 {
        std::thread::yield_now();
    }
}
pub unsafe fn inb(port: u16) -> u8 {
    HELD.with(|v| assert!(v.get(), "CMOS read escaped canonical owner"));
    assert_eq!(port, 0x71);
    let mut ports = PORTS.lock().unwrap();
    ports.reads += 1;
    let index = ports.index as usize;
    let value = ports.registers[index];
    if index == 0x32 && ports.change_century {
        ports.registers[index] = 19;
        ports.change_century = false;
    }
    value
}
