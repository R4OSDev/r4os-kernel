//! Owned synthetic blockqueue checks shared by boot and explicit Spectra work.
mod backend;
mod cases;
mod live;
mod session;
use super::{
    block_accounting as accounting, block_device as device, block_execution as execution,
    block_io as io, block_workers as workers,
};
use crate::{
    boot::log_route::{dec, puts},
    platform::timer,
    sched::{
        sync_objects,
        sync_types::Event,
        task::{
            diagnostic,
            task_types::{Role, Task},
        },
        task_context::Admission,
    },
};
use core::{
    cell::UnsafeCell,
    ffi::c_void,
    mem::MaybeUninit,
    ptr,
    sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering},
};
pub(crate) use live::check;
use session::{
    active, case, create, finish_workers, kill_denied, queue, reference, register, summary,
    unregister,
};
const WAIT: u64 = 2 * crate::arch::pit::DEFAULT_HZ as u64;
struct Events([UnsafeCell<Event>; 13]);
// Reset only with the exclusive gate after all previous waiters retired.
unsafe impl Sync for Events {}
static EVENTS: Events = Events([const { UnsafeCell::new(Event::AUTO) }; 13]);
struct Buffers([UnsafeCell<[u8; 512]>; 8]);
// Resident backing is reused only after actual caller/backend retirement.
unsafe impl Sync for Buffers {}
static BUFFERS: Buffers = Buffers([const { UnsafeCell::new([0; 512]) }; 8]);
struct Requests([UnsafeCell<MaybeUninit<device::AsyncRequest>>; 12]);
// One exact own backend publishes an initialized prefix with release ordering.
unsafe impl Sync for Requests {}
static REQUESTS: Requests = Requests([const { UnsafeCell::new(MaybeUninit::uninit()) }; 12]);
static SUBMISSIONS: AtomicU32 = AtomicU32::new(0);
static PENDING: AtomicU32 = AtomicU32::new(0);
static CANCELS: AtomicU32 = AtomicU32::new(0);
static TIMEOUT_CANCELS: AtomicU32 = AtomicU32::new(0);
static RESET_CANCELS: AtomicU32 = AtomicU32::new(0);
static RESETS: AtomicU32 = AtomicU32::new(0);
static ACCEPTED: AtomicU32 = AtomicU32::new(0);
static DUPLICATE_REJECTED: AtomicU32 = AtomicU32::new(0);
static DUPLICATE_KIND: AtomicU32 = AtomicU32::new(0);
static LATE_REJECTED: AtomicU32 = AtomicU32::new(0);
static REQUESTER_ACTIVE: AtomicU32 = AtomicU32::new(0);
static SYNC_ACTIVE: AtomicU32 = AtomicU32::new(0);
static SYNC_MAX: AtomicU32 = AtomicU32::new(0);
static FLAGS: [AtomicBool; 10] = [const { AtomicBool::new(false) }; 10];
static SLOW: AtomicUsize = AtomicUsize::new(usize::MAX);
static FAST: AtomicUsize = AtomicUsize::new(usize::MAX);
static ASYNC: AtomicUsize = AtomicUsize::new(usize::MAX);
static ABORT: AtomicBool = AtomicBool::new(false);
static CANCEL: AtomicBool = AtomicBool::new(false);
fn signal(index: usize) {
    unsafe {
        sync_objects::r4os_rust_sync_event(EVENTS.0[index].get(), 0, 0);
    }
}
fn wait(index: usize, ticks: u64) -> u32 {
    unsafe { sync_objects::r4os_rust_sync_event(EVENTS.0[index].get(), 2, ticks) }
}
fn done(index: usize) -> bool {
    !cancelled() && wait(index, WAIT) == 1
}
fn flag(index: usize) -> bool {
    FLAGS[index].load(Ordering::Acquire)
}
fn set(index: usize, value: bool) {
    FLAGS[index].store(value, Ordering::Release);
}
fn buffer(index: usize) -> *mut u8 {
    BUFFERS.0[index].get().cast()
}
fn context_pointer() -> *mut c_void {
    ptr::addr_of!(SUBMISSIONS).cast_mut().cast()
}
fn cancelled() -> bool {
    CANCEL.load(Ordering::Acquire) || ABORT.load(Ordering::Acquire)
}
fn read(index: usize, lba: u64, b: usize) -> bool {
    reference(index).is_some_and(|v| unsafe { io::read_exact(v, lba, 1, buffer(b), 512, true) })
}
fn config(name: &'static [u8], controller: &'static [u8], read: device::Read) -> device::Config {
    let mut c = device::Config::new(device::text(name), read);
    c.controller = device::text(controller);
    c.bus = 5;
    c.sector_size = 512;
    c.sector_count = 1;
    c.context = context_pointer();
    c
}
struct Requester;
impl Requester {
    fn enter() -> Option<Self> {
        if ABORT.load(Ordering::Acquire) {
            return None;
        }
        REQUESTER_ACTIVE.fetch_add(1, Ordering::AcqRel);
        let guard = Self;
        if ABORT.load(Ordering::Acquire) {
            drop(guard);
            None
        } else {
            Some(guard)
        }
    }
}
impl Drop for Requester {
    fn drop(&mut self) {
        REQUESTER_ACTIVE.fetch_sub(1, Ordering::Release);
    }
}
struct Activity;
impl Activity {
    fn enter() -> Self {
        let active = SYNC_ACTIVE.fetch_add(1, Ordering::AcqRel) + 1;
        SYNC_MAX.fetch_max(active, Ordering::Relaxed);
        Self
    }
}
impl Drop for Activity {
    fn drop(&mut self) {
        SYNC_ACTIVE.fetch_sub(1, Ordering::Release);
    }
}
pub(crate) fn parallel_dispatch() -> bool {
    let Some(mut gate) = session::gate() else {
        puts(b"BLOCKDISPATCHCHECK FAIL busy-or-retained\r\n");
        return false;
    };
    unsafe {
        session::reset();
    }
    let ok = cases::run();
    let clean = session::cleanup();
    if !clean {
        gate.0 = false;
    }
    ok && clean
}
