//! Exact-cookie callback capture. An IRQ worker lends only its fixed record
//! for this synchronous copy; the resident cell never retains that pointer.
use super::*;
static COOKIE: AtomicU64 = AtomicU64::new(0);
static COUNT: AtomicU64 = AtomicU64::new(0);
static CLAIMED: AtomicBool = AtomicBool::new(false);
static READY: AtomicBool = AtomicBool::new(false);
struct Cell(UnsafeCell<MaybeUninit<c::StorageRequestObservation>>);
// One CAS winner writes; READY releases the immutable copy to the controller.
unsafe impl Sync for Cell {}
static DATA: Cell = Cell(UnsafeCell::new(MaybeUninit::uninit()));
pub(super) fn reset(cookie: u64) {
    READY.store(false, Ordering::Relaxed);
    CLAIMED.store(false, Ordering::Relaxed);
    COUNT.store(0, Ordering::Relaxed);
    COOKIE.store(cookie, Ordering::Release);
}
pub(super) unsafe extern "C" fn receive(cookie: u64, input: *const c::StorageRequestObservation) {
    if cookie == 0 || COOKIE.load(Ordering::Acquire) != cookie {
        return;
    }
    COUNT.fetch_add(1, Ordering::AcqRel);
    if input.is_null()
        || unsafe { (*input).version } != c::storage_observation_version
        || unsafe { (*input).size } < core::mem::size_of::<c::StorageRequestObservation>() as u32
    {
        return;
    }
    if CLAIMED
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        return;
    }
    unsafe {
        DATA.0.get().write(MaybeUninit::new(input.read()));
    }
    READY.store(true, Ordering::Release);
}
pub(super) fn snapshot() -> (u64, Option<c::StorageRequestObservation>) {
    let count = COUNT.load(Ordering::Acquire);
    let data = if READY.load(Ordering::Acquire) {
        Some(unsafe { (*DATA.0.get()).assume_init() })
    } else {
        None
    };
    (count, data)
}
