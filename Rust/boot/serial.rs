//! The one COM1 stream keeps its fixed ring, drop accounting and bounded
//! synchronous shutdown policy. COM2 remains a caller-owned raw port.
use super::owner::Guard;
use super::serial_admission as admission;
use crate::arch::io::{inb, outb};
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
const COM1: u16 = 0x3f8;
const CAPACITY: usize = 8192;
const FIFO: usize = 16;
const TIMEOUT: usize = 1_000_000;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Stats {
    pub(crate) ring_size: u64,
    pub(crate) pending: u64,
    pub(crate) ring_bytes: u64,
    pub(crate) sync_bytes: u64,
    pub(crate) full_stalls: u64,
    pub(crate) dropped_bytes: u64,
    pub(crate) sync_failures: u64,
    pub(crate) sync_mode: bool,
    pub(crate) sync_uart_available: bool,
    pub(crate) write_calls: u64,
    pub(crate) bulk_calls: u64,
    pub(crate) lock_acquisitions: u64,
    pub(crate) uart_status_reads: u64,
    pub(crate) drain_calls: u64,
    pub(crate) max_span: u64,
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
    known_uart: bool,
    fifo: usize,
}
struct Storage(UnsafeCell<State>);
// SAFETY: the existing serial owner covers all state/ring accesses. Atomics
// live separately to allow the original lock-free empty check without aliasing
// an exclusive State borrow or racing non-atomic Rust storage.
unsafe impl Sync for Storage {}
static SERIAL: Storage = Storage(UnsafeCell::new(State {
    ring: [0; CAPACITY],
    stats: Stats::EMPTY,
    known_uart: false,
    fifo: 1,
}));
static HEAD: AtomicUsize = AtomicUsize::new(0);
static TAIL: AtomicUsize = AtomicUsize::new(0);
static EMERGENCY_UART: AtomicBool = AtomicBool::new(false);
fn pending() -> usize {
    HEAD.load(Ordering::Relaxed)
        .wrapping_sub(TAIL.load(Ordering::Relaxed))
}

/// Only the original early bootstrap may attach debug sinks. A later panic,
/// shutdown or old initialization callback cannot reacquire a released port.
pub(super) fn init_debug() -> bool {
    if !admission::begin_init() {
        return false;
    }
    r4os_rust_serial_init(COM1);
    let scratch = unsafe { inb(COM1 + 7) };
    let present = r4os_rust_serial_present(COM1);
    unsafe {
        outb(COM1 + 7, scratch);
    }
    let fifo = if present && unsafe { inb(COM1 + 2) } & 0xc0 == 0xc0 {
        FIFO
    } else {
        1
    };
    {
        let _owner = Guard::acquire(1);
        unsafe {
            (*SERIAL.0.get()).known_uart = present;
            (*SERIAL.0.get()).fifo = fifo;
        }
    }
    EMERGENCY_UART.store(present, Ordering::Release);
    admission::initialized();
    true
}
pub(crate) fn debug_active() -> bool {
    admission::active()
}
pub(crate) fn debug_claims_com1() -> bool {
    admission::claims_com1()
}

/// The terminal #MC path cannot acquire diagnostic/serial owners or call an
/// unloadable UART sink. This lease exists only for retained early COM1 with
/// zero admitted writers. Released/busy/absent hardware leaves RAM evidence.
pub(crate) struct Emergency {
    budget: usize,
}
pub(crate) fn emergency() -> Option<Emergency> {
    if EMERGENCY_UART.load(Ordering::Acquire) && admission::emergency() {
        Some(Emergency { budget: 2_000_000 })
    } else {
        None
    }
}
impl Emergency {
    pub(crate) fn write(&mut self, text: &[u8]) {
        for &byte in text {
            let mut ready = false;
            for _ in 0..256 {
                if self.budget == 0 {
                    return;
                }
                self.budget -= 1;
                if unsafe { inb(COM1 + 5) } & 0x20 != 0 {
                    ready = true;
                    break;
                }
            }
            if !ready {
                self.budget = 0;
                return;
            }
            unsafe {
                outb(COM1, byte);
            }
        }
    }
    pub(crate) fn hex(&mut self, value: u64) {
        let mut text = [b'0'; 16];
        for (i, byte) in text.iter_mut().enumerate() {
            *byte = b"0123456789ABCDEF"[((value >> ((15 - i) * 4)) & 15) as usize];
        }
        self.write(&text);
    }
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
    if len == 0 || !admission::active() {
        return;
    }
    let _guard = Guard::acquire(1);
    let Some(_writer) = admission::admit() else {
        return;
    };
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
    if !admission::active() || pending() == 0 {
        return;
    }
    let _guard = Guard::acquire(1);
    let Some(_writer) = admission::admit() else {
        return;
    };
    unsafe { &mut *SERIAL.0.get() }.drain();
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_serial_flush() {
    if !admission::active() {
        return;
    }
    let _guard = Guard::acquire(1);
    let Some(_writer) = admission::admit() else {
        return;
    };
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
    if !admission::active() {
        return;
    }
    let _guard = Guard::acquire(1);
    let Some(_writer) = admission::admit() else {
        return;
    };
    // Do not hold a Rust &mut across the intentional nested owner acquisition.
    unsafe {
        (*SERIAL.0.get()).stats.sync_mode = true;
    }
    r4os_rust_serial_flush();
}

const CLOSE_MESSAGE: &[u8] =
    b"[COM1] Debugging wird nach dem Boot abgeschaltet; Anschluss wird freigegeben.\r\n";
struct CloseBudget {
    start: u64,
    ticks: u64,
    polls: usize,
}
impl CloseBudget {
    fn new() -> Self {
        Self {
            start: crate::platform::timer::tick_count(),
            ticks: (u64::from(crate::platform::timer::frequency()) * 3).max(1),
            polls: 0,
        }
    }
    fn live(&self) -> bool {
        self.polls < TIMEOUT
            && crate::platform::timer::tick_count().wrapping_sub(self.start) < self.ticks
    }
    fn wait(&mut self) -> bool {
        self.polls += 1;
        if !self.live() {
            return false;
        }
        if self.polls.is_multiple_of(64) {
            let reason = b"com1-debug-close";
            unsafe {
                crate::sched::scheduler::sleep(1, reason.as_ptr(), reason.len());
            }
        } else {
            core::hint::spin_loop();
        }
        true
    }
    fn ready(&mut self, mask: u8) -> bool {
        loop {
            if !self.live() {
                return false;
            }
            let status = unsafe { inb(COM1 + 5) };
            if status == 0xff {
                return false;
            }
            if status & mask == mask {
                return true;
            }
            if !self.wait() {
                return false;
            }
        }
    }
}
/// Valid task context only. The atomic claim closes admission first, then
/// waits outside all no-sleep owners. Admitted writers finish under the old
/// serial owner; their counters drop before that owner can permit preemption.
pub(super) fn close_after_boot() -> bool {
    if !admission::begin_close() {
        return false;
    }
    unsafe {
        super::log_route::set_serial(None, None, None, None);
    }
    let mut budget = CloseBudget::new();
    while admission::writers() != 0 {
        if !budget.wait() {
            return close_result(false);
        }
    }
    let (known, fifo) = {
        let _owner = Guard::acquire(1);
        unsafe { ((*SERIAL.0.get()).known_uart, (*SERIAL.0.get()).fifo) }
    };
    if !known {
        return close_result(false);
    }
    while pending() != 0 {
        if !budget.ready(0x20) {
            return close_result(false);
        }
        let mut bytes = [0u8; FIFO];
        let (tail, count) = {
            let _owner = Guard::acquire(1);
            let tail = TAIL.load(Ordering::Relaxed);
            let count = pending().min(fifo);
            for (index, byte) in bytes[..count].iter_mut().enumerate() {
                *byte =
                    unsafe { (*SERIAL.0.get()).ring[tail.wrapping_add(index) & (CAPACITY - 1)] };
            }
            (tail, count)
        };
        for byte in &bytes[..count] {
            unsafe {
                outb(COM1, *byte);
            }
        }
        let _owner = Guard::acquire(1);
        TAIL.store(tail.wrapping_add(count), Ordering::Relaxed);
    }
    {
        let _owner = Guard::acquire(1);
        let stats = unsafe { &mut (*SERIAL.0.get()).stats };
        stats.write_calls = stats.write_calls.wrapping_add(1);
        stats.bulk_calls = stats.bulk_calls.wrapping_add(1);
        stats.lock_acquisitions = stats.lock_acquisitions.wrapping_add(1);
        stats.max_span = stats.max_span.max(CLOSE_MESSAGE.len() as u64);
    }
    let mut offset = 0;
    while offset < CLOSE_MESSAGE.len() {
        if !budget.ready(0x20) {
            return close_result(false);
        }
        let count = fifo.min(CLOSE_MESSAGE.len() - offset);
        for byte in &CLOSE_MESSAGE[offset..offset + count] {
            unsafe {
                outb(COM1, *byte);
            }
        }
        offset += count;
        let _owner = Guard::acquire(1);
        let stats = unsafe { &mut (*SERIAL.0.get()).stats };
        stats.sync_bytes = stats.sync_bytes.wrapping_add(count as u64);
    }
    // THRE alone acknowledges no shift-register completion. Both transmitter
    // states must be genuinely empty before a normal owner may claim COM1.
    close_result(budget.ready(0x60))
}
fn close_result(released: bool) -> bool {
    // A stuck admitted writer may still own the metadata lock. Failure must
    // retain the port without waiting on that same owner beyond the budget.
    if !released && admission::writers() != 0 {
        admission::finish(false);
        return false;
    }
    {
        let _owner = Guard::acquire(1);
        let stats = unsafe { &mut (*SERIAL.0.get()).stats };
        stats.sync_uart_available = false;
        if !released {
            stats.sync_failures = stats.sync_failures.wrapping_add(1);
        }
    }
    admission::finish(released);
    !debug_claims_com1()
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
