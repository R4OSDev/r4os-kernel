//! One resident double-buffered hive/cache and scratch owner. Transactions
//! release the ranked registry mutex before filesystem I/O and retain only
//! stable pointers protected by the original transaction gate.
use super::super::registry_format_types::{self as t, BuildKey, BuildValue, Bytes, View};
use crate::sched::{
    scheduler, sync_locks,
    sync_types::{Mutex, Text},
};
use core::{
    cell::UnsafeCell,
    ptr,
    sync::atomic::{AtomicU8, Ordering},
};
pub const HIVE_BYTES: usize = 32 * 1024;
pub const VALUES: usize = 512;
pub const KEYS: usize = 256;
pub const PATH_POOL: usize = 16 * 1024;
pub const NONE: u8 = 255;
pub const HIVE_PATH: &[u8] = b"C:\\R4OS\\REGISTRY\\SYSTEM.R4R";
pub const TMP_PATH: &[u8] = b"C:\\R4OS\\REGISTRY\\SYSTEM.TMP\0";
pub const BAK_PATH: &[u8] = b"C:\\R4OS\\REGISTRY\\SYSTEM.BAK\0";
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Performance {
    pub read_calls: u64,
    pub cache_hits: u64,
    pub file_loads: u64,
    pub validation_passes: u64,
    pub publications: u64,
    pub commits: u64,
    pub commit_failures: u64,
    pub atomic_retries: u64,
}
pub struct Slot {
    pub valid: bool,
    pub dirty: bool,
    pub kind: u16,
    pub len: usize,
    pub view: Option<View>,
    pub bytes: [u8; HIVE_BYTES],
}
impl Slot {
    const EMPTY: Self = Self {
        valid: false,
        dirty: false,
        kind: 1,
        len: 0,
        view: None,
        bytes: [0; HIVE_BYTES],
    };
    pub fn invalidate(&mut self, clear_dirty: bool) {
        self.valid = false;
        self.view = None;
        self.len = 0;
        if clear_dirty {
            self.dirty = false;
        }
    }
}
pub struct State {
    pub active: u8,
    pub pending: u8,
    pub slots: [Slot; 2],
    pub verify: [u8; HIVE_BYTES],
    pub performance: Performance,
    pub values: [BuildValue; VALUES],
    pub keys: [BuildKey; KEYS],
    pub order: [u32; VALUES],
    pub owners: [u32; VALUES],
    pub flat: [u32; KEYS],
    pub paths: [u8; PATH_POOL],
}
struct Owner(UnsafeCell<State>);
// SAFETY: Registry mutex serializes state, read views and pure candidate builds;
// the original transaction gate retains inactive/verify storage over I/O. No
// state reference survives a wait or external file callback. Buffers are stable.
unsafe impl Sync for Owner {}
static STATE: Owner = Owner(UnsafeCell::new(State {
    active: NONE,
    pending: NONE,
    slots: [Slot::EMPTY, Slot::EMPTY],
    verify: [0; HIVE_BYTES],
    performance: Performance {
        read_calls: 0,
        cache_hits: 0,
        file_loads: 0,
        validation_passes: 0,
        publications: 0,
        commits: 0,
        commit_failures: 0,
        atomic_retries: 0,
    },
    values: [BuildValue {
        key_path: Bytes::EMPTY,
        name: Bytes::EMPTY,
        value_type: 1,
        data: Bytes::EMPTY,
    }; VALUES],
    keys: [BuildKey::EMPTY; KEYS],
    order: [0; VALUES],
    owners: [t::INVALID; VALUES],
    flat: [t::INVALID; KEYS],
    paths: [0; PATH_POOL],
}));
struct Lock(UnsafeCell<Mutex>);
// SAFETY: Native sync owns lock-field mutation under Runtime; stable storage.
unsafe impl Sync for Lock {}
static LOCK: Lock = Lock(UnsafeCell::new(Mutex {
    rank: 115,
    name: Text::literal(b"r4r-hive-state"),
    ..Mutex::EMPTY
}));
static TRANSACTION: AtomicU8 = AtomicU8::new(0);
pub struct StateGuard;
impl StateGuard {
    pub(super) unsafe fn acquire() -> Option<Self> {
        if unsafe { sync_locks::r4os_rust_sync_mutex(LOCK.0.get(), 1, u64::MAX) } {
            Some(Self)
        } else {
            None
        }
    }
}
impl Drop for StateGuard {
    fn drop(&mut self) {
        unsafe { sync_locks::r4os_rust_sync_mutex(LOCK.0.get(), 2, 0) };
    }
}
pub struct Transaction;
impl Transaction {
    pub(super) unsafe fn acquire() -> Self {
        while TRANSACTION
            .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            scheduler::yield_task();
        }
        Self
    }
}
impl Drop for Transaction {
    fn drop(&mut self) {
        TRANSACTION.store(0, Ordering::Release);
    }
}
pub fn state() -> *mut State {
    STATE.0.get()
}
pub(super) unsafe fn slot(index: u8) -> *mut Slot {
    assert!(index < 2);
    unsafe {
        ptr::addr_of_mut!((*state()).slots)
            .cast::<Slot>()
            .add(usize::from(index))
    }
}
pub(super) unsafe fn cached(kind: u16) -> Option<View> {
    unsafe {
        let active = (*state()).active;
        if active == NONE {
            return None;
        }
        let slot = slot(active);
        if !(*slot).valid || (*slot).kind != kind {
            return None;
        }
        (*slot).view
    }
}
pub(super) unsafe fn inactive() -> u8 {
    if unsafe { (*state()).active } == 0 {
        1
    } else {
        0
    }
}
pub fn next_generation(value: u64) -> u64 {
    let next = value.wrapping_add(1);
    if next == 0 { 1 } else { next }
}
pub(super) unsafe fn publish_locked(index: u8, dirty: bool) {
    unsafe {
        let state = state();
        let previous = (*state).active;
        (*slot(index)).dirty = dirty;
        (*state).active = index;
        (*state).pending = NONE;
        (*state).performance.publications = (*state).performance.publications.wrapping_add(1);
        if previous != NONE && previous != index {
            (*slot(previous)).invalidate(true);
        }
    }
}
pub(super) unsafe fn note(index: u8) {
    unsafe {
        let Some(_guard) = StateGuard::acquire() else {
            return;
        };
        let perf = &mut (*state()).performance;
        let field = match index {
            0 => &mut perf.file_loads,
            1 => &mut perf.commits,
            2 => &mut perf.atomic_retries,
            _ => panic!("Registry note"),
        };
        *field = field.wrapping_add(1);
    }
}
pub(super) unsafe fn note_read() {
    unsafe {
        let count = &mut (*state()).performance.read_calls;
        *count = count.wrapping_add(1);
    }
}
pub(super) unsafe fn failure(index: u8, uncertain: bool) {
    unsafe {
        let Some(_guard) = StateGuard::acquire() else {
            return;
        };
        let state = state();
        (*state).performance.commit_failures = (*state).performance.commit_failures.wrapping_add(1);
        if uncertain && index < 2 && (*slot(index)).valid {
            (*state).pending = index;
        } else if index < 2 && (*state).active != index {
            (*slot(index)).invalidate(false);
        }
    }
}
pub(super) unsafe fn publish(index: u8, dirty: bool) -> i32 {
    unsafe {
        let Some(_guard) = StateGuard::acquire() else {
            return crate::contract::registry_api_result_io;
        };
        if index >= 2 || !(*slot(index)).valid {
            return crate::contract::registry_api_result_io;
        }
        publish_locked(index, dirty);
        crate::contract::registry_api_result_ok
    }
}
/// # Safety
/// Immutable original raw DOS path remains readable. Existing external writers
/// call after mutation; only the ranked cache owner may revoke published views.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_registry_invalidate(path: Bytes) {
    unsafe {
        if !super::path::equal_path(path.slice(), HIVE_PATH) {
            return;
        }
        let Some(_guard) = StateGuard::acquire() else {
            return;
        };
        let state = state();
        (*state).active = NONE;
        (*state).pending = NONE;
        for index in 0..2 {
            (*slot(index)).invalidate(true);
        }
    }
}
/// # Safety
/// Exclusive resident snapshot outputs; no caller copy occurs under this owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_registry_performance(
    performance: *mut Performance,
    generation: *mut u64,
    dirty: *mut bool,
    pending: *mut bool,
) -> bool {
    unsafe {
        let Some(_guard) = StateGuard::acquire() else {
            return false;
        };
        let state = state();
        let active = (*state).active;
        performance.write((*state).performance);
        generation.write(if active != NONE {
            (*slot(active))
                .view
                .map_or(0, |view| view.header.generation)
        } else {
            0
        });
        dirty.write(active != NONE && (*slot(active)).dirty);
        pending.write((*state).pending != NONE);
        true
    }
}
