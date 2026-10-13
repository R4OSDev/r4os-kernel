//! The one COM1 stream keeps its fixed ring, drop accounting and bounded
//! synchronous shutdown policy. COM2 remains a caller-owned raw port.
use super::owner::Guard;
use crate::arch::io::{inb, outb};
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicUsize, Ordering};
const COM1: u16 = 0x3f8;
const CAPACITY: usize = 8192;
const FIFO: usize = 16;
const TIMEOUT: usize = 1_000_000;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Stats {
    ring_size: u64,
    pending: u64,
    ring_bytes: u64,
    sync_bytes: u64,
    full_stalls: u64,
    dropped_bytes: u64,
    sync_failures: u64,
    sync_mode: bool,
    sync_uart_available: bool,
    write_calls: u64,
    bulk_calls: u64,
    lock_acquisitions: u64,
    uart_status_reads: u64,
    drain_calls: u64,
    max_span: u64,
}
impl Stats {
    const EMPTY: Self = Self {
        ring_size: CAPACITY as u64,
        pending: 0,
        ring_bytes: 0,
        sync_bytes: 0,
        full_stalls: 0,
        dropped_bytes: 0,
        sync_failures: 0,
        sync_mode: false,
        sync_uart_available: true,
        write_calls: 0,
        bulk_calls: 0,
        lock_acquisitions: 0,
        uart_status_reads: 0,
        drain_calls: 0,
        max_span: 0,
    };
}
struct State {
    ring: [u8; CAPACITY],
    stats: Stats,
}
struct Storage(UnsafeCell<State>);
// SAFETY: the existing serial owner covers all state/ring accesses. Atomics
// live separately to allow the original lock-free empty check without aliasing
// an exclusive State borrow or racing non-atomic Rust storage.
unsafe impl Sync for Storage {}
static SERIAL: Storage = Storage(UnsafeCell::new(State {
    ring: [0; CAPACITY],
    stats: Stats::EMPTY,
}));
static HEAD: AtomicUsize = AtomicUsize::new(0);
static TAIL: AtomicUsize = AtomicUsize::new(0);
fn pending() -> usize {
    HEAD.load(Ordering::Relaxed)
        .wrapping_sub(TAIL.load(Ordering::Relaxed))
}

#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_serial_init(base: u16) {
    unsafe {
        outb(base + 1, 0);
        outb(base + 3, 0x80);
        outb(base, 3);
        outb(base + 1, 0);
        outb(base + 3, 3);
        outb(base + 2, 0xc7);
        outb(base + 4, 0x0b);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_serial_present(base: u16) -> bool {
    unsafe {
        outb(base + 7, 0x5a);
        if inb(base + 7) != 0x5a {
            return false;
        }
        outb(base + 7, 0xa5);
        inb(base + 7) == 0xa5
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_serial_putc(base: u16, byte: u8) {
    let mut spins = 0;
    unsafe {
        while inb(base + 5) & 0x20 == 0 {
            if spins >= TIMEOUT {
                return;
            }
            spins += 1;
        }
        outb(base, byte);
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller owns this port and keeps len readable resident bytes until return.
pub unsafe extern "C" fn r4os_rust_serial_available(
    base: u16,
    data: *const u8,
    len: usize,
) -> usize {
    if len == 0 || unsafe { inb(base + 5) } & 0x20 == 0 {
        return 0;
    }
    let count = len.min(FIFO);
    for index in 0..count {
        unsafe { outb(base, data.add(index).read()) };
    }
    count
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_serial_getc(base: u16) -> i32 {
    unsafe {
        if inb(base + 5) & 1 == 0 {
            -1
        } else {
            i32::from(inb(base))
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_serial_has_data(base: u16) -> bool {
    unsafe { inb(base + 5) & 1 != 0 }
}

impl State {
    fn drain(&mut self) {
        self.stats.drain_calls = self.stats.drain_calls.wrapping_add(1);
        if pending() == 0 {
            return;
        }
        self.stats.uart_status_reads = self.stats.uart_status_reads.wrapping_add(1);
        if unsafe { inb(COM1 + 5) } & 0x20 == 0 {
            return;
        }
        self.burst(pending().min(FIFO));
    }
    fn burst(&mut self, count: usize) {
        let mut tail = TAIL.load(Ordering::Relaxed);
        for _ in 0..count {
            unsafe { outb(COM1, self.ring[tail & (CAPACITY - 1)]) };
            tail = tail.wrapping_add(1);
        }
        TAIL.store(tail, Ordering::Relaxed);
    }
    fn ready_sync(&mut self) -> bool {
        if !self.stats.sync_uart_available {
            return false;
        }
        self.stats.uart_status_reads = self.stats.uart_status_reads.wrapping_add(1);
        let mut spins = 0;
        while unsafe { inb(COM1 + 5) } & 0x20 == 0 {
            if spins >= TIMEOUT {
                self.stats.sync_uart_available = false;
                self.stats.sync_failures = self.stats.sync_failures.wrapping_add(1);
                return false;
            }
            spins += 1;
        }
        true
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller keeps len readable bytes resident until return, outside the TX ring.
pub unsafe extern "C" fn r4os_rust_serial_write(data: *const u8, len: usize) {
    if len == 0 {
        return;
    }
    let _guard = Guard::acquire(1);
    let state = unsafe { &mut *SERIAL.0.get() };
    state.stats.write_calls = state.stats.write_calls.wrapping_add(1);
    if len > 1 {
        state.stats.bulk_calls = state.stats.bulk_calls.wrapping_add(1);
    }
    state.stats.lock_acquisitions = state.stats.lock_acquisitions.wrapping_add(1);
    state.stats.max_span = state.stats.max_span.max(len as u64);
    if state.stats.sync_mode {
        for index in 0..len {
            if state.ready_sync() {
                unsafe { outb(COM1, data.add(index).read()) };
                state.stats.sync_bytes = state.stats.sync_bytes.wrapping_add(1);
            } else {
                state.stats.dropped_bytes = state.stats.dropped_bytes.wrapping_add(1);
            }
        }
        return;
    }
    let mut drained = false;
    if len > CAPACITY.saturating_sub(pending()) {
        state.drain();
        drained = true;
    }
    let retained = pending().min(CAPACITY);
    let overflow = retained.saturating_add(len).saturating_sub(CAPACITY);
    let drop_existing = retained.min(overflow);
    let skip_input = overflow - drop_existing;
    if overflow != 0 {
        state.stats.full_stalls = state.stats.full_stalls.wrapping_add(overflow as u64);
        state.stats.dropped_bytes = state.stats.dropped_bytes.wrapping_add(overflow as u64);
        TAIL.store(
            TAIL.load(Ordering::Relaxed).wrapping_add(drop_existing),
            Ordering::Relaxed,
        );
    }
    let mut head = HEAD.load(Ordering::Relaxed);
    for index in skip_input..len {
        state.ring[head & (CAPACITY - 1)] = unsafe { data.add(index).read() };
        head = head.wrapping_add(1);
        state.stats.ring_bytes = state.stats.ring_bytes.wrapping_add(1);
    }
    HEAD.store(head, Ordering::Relaxed);
    if !drained {
        state.drain();
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_serial_drain() {
    if pending() == 0 {
        return;
    }
    let _guard = Guard::acquire(1);
    unsafe { &mut *SERIAL.0.get() }.drain();
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_serial_flush() {
    let _guard = Guard::acquire(1);
    let state = unsafe { &mut *SERIAL.0.get() };
    while pending() != 0 {
        if !state.ready_sync() {
            state.stats.dropped_bytes = state.stats.dropped_bytes.wrapping_add(pending() as u64);
            TAIL.store(HEAD.load(Ordering::Relaxed), Ordering::Relaxed);
            return;
        }
        state.burst(pending().min(FIFO));
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_serial_sync() {
    let _guard = Guard::acquire(1);
    // Do not hold a Rust &mut across the intentional nested owner acquisition.
    unsafe {
        (*SERIAL.0.get()).stats.sync_mode = true;
    }
    r4os_rust_serial_flush();
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller owns one writable, aligned C-layout Stats for this call.
pub unsafe extern "C" fn r4os_rust_serial_stats(output: *mut Stats) {
    let _guard = Guard::acquire(1);
    let mut result = unsafe { (*SERIAL.0.get()).stats };
    result.pending = pending() as u64;
    unsafe { output.write(result) };
}
const _: () = {
    assert!(
        core::mem::size_of::<Stats>() == 112 && core::mem::offset_of!(Stats, write_calls) == 64
    );
};
