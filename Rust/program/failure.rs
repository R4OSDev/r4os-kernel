//! Original one-shot lifecycle test admission and progress acknowledgments.
//! This native owner is the only mutable failure state; immutable boot option
//! bytes are borrowed briefly from the existing configuration owner.
use super::registry_state::state;
use crate::{
    fs::vfs_types::Bytes,
    sched::{
        sync_locks, sync_objects,
        sync_types::{Mutex, Text},
    },
};
use core::{cell::UnsafeCell, slice};
struct Cell<T>(UnsafeCell<T>);
// Access follows the original rank100 failure mutex; no reference spans peers.
unsafe impl<T> Sync for Cell<T> {}
impl<T> Cell<T> {
    const fn new(v: T) -> Self {
        Self(UnsafeCell::new(v))
    }
    const fn get(&self) -> *mut T {
        self.0.get()
    }
}
#[derive(Default)]
struct Data {
    phase: u8,
    last_phase: u32,
    consumed: bool,
    signalled: bool,
    retried: bool,
    recovered: bool,
}
static DATA: Cell<Data> = Cell::new(Data {
    phase: 0,
    last_phase: 0,
    consumed: false,
    signalled: false,
    retried: false,
    recovered: false,
});
static LOCK: Cell<Mutex> = Cell::new(Mutex {
    rank: 100,
    mode: 0,
    name: Text::literal(b"r4x-lifecycle-failure"),
    ..Mutex::EMPTY
});
unsafe extern "C" {
    fn r4os_zig_program_selftest_option(group: u8) -> Bytes;
}
fn lock() -> bool {
    unsafe { sync_locks::r4os_rust_sync_mutex(LOCK.get(), 1, u64::MAX) }
}
fn unlock() {
    unsafe { sync_locks::r4os_rust_sync_mutex(LOCK.get(), 2, 0) };
}
pub(super) fn allowed(group: u8) -> bool {
    let v = unsafe { r4os_zig_program_selftest_option(group) };
    let text = if v.len == 0 {
        &[][..]
    } else {
        unsafe { slice::from_raw_parts(v.data, v.len) }
    };
    text.eq_ignore_ascii_case(b"yes")
}
fn configure(phase: u8) {
    if phase != 0 && !allowed(2) {
        return;
    }
    if !lock() {
        return;
    }
    unsafe {
        DATA.get().write(Data {
            phase,
            last_phase: u32::from(phase),
            consumed: false,
            signalled: false,
            retried: false,
            recovered: false,
        });
    }
    unlock();
}
fn arm(phase: u8) -> bool {
    if phase == 0 || !allowed(2) {
        return false;
    }
    if !lock() {
        return false;
    }
    let success = unsafe { (*DATA.get()).phase == 0 };
    if success {
        unsafe {
            DATA.get().write(Data {
                phase,
                last_phase: u32::from(phase),
                consumed: false,
                signalled: false,
                retried: false,
                recovered: false,
            });
        }
    }
    unlock();
    success
}
fn signal() {
    if lock() {
        unsafe {
            (*DATA.get()).signalled = true;
        }
        unlock();
    }
    unsafe { sync_objects::r4os_rust_sync_event((*state()).event, 0, 0) };
}
pub(super) fn consume(phase: u8) -> bool {
    if !lock() {
        return false;
    }
    let success = unsafe { (*DATA.get()).phase != 0 && (*DATA.get()).phase == phase };
    if success {
        unsafe {
            (*DATA.get()).phase = 0;
            (*DATA.get()).consumed = true;
        }
    }
    unlock();
    success
}
pub(super) fn note(recovered: bool) {
    if !lock() {
        return;
    }
    unsafe {
        if recovered {
            if (*DATA.get()).consumed {
                (*DATA.get()).recovered = true;
            }
        } else {
            (*DATA.get()).retried = true;
        }
    }
    unlock();
}
#[repr(C)]
#[derive(Default)]
pub struct Snapshot {
    pub phase: u32,
    pub armed: bool,
    pub consumed: bool,
    pub reaper_signalled: bool,
    pub retried: bool,
    pub recovered: bool,
    padding: [u8; 3],
}
/// # Safety
/// Phase is an original validated0..13 ordinal; only op2 signals the reaper.
/// Every operation preserves original failure/registry owner separation.
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_failure_control(op: u8, phase: u8) -> bool {
    match op {
        0 => {
            configure(phase);
            true
        }
        1 => arm(phase),
        2 => {
            signal();
            true
        }
        3 => consume(phase),
        4 => {
            note(false);
            true
        }
        5 => {
            note(true);
            true
        }
        _ => unreachable!(),
    }
}
/// # Safety
/// Output is an exclusive aligned resident private C snapshot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_failure_snapshot(out: *mut Snapshot) {
    if !lock() {
        unsafe { out.write(Snapshot::default()) };
        return;
    }
    unsafe {
        let p = DATA.get();
        out.write(Snapshot {
            phase: if (*p).phase != 0 {
                u32::from((*p).phase)
            } else {
                (*p).last_phase
            },
            armed: (*p).phase != 0,
            consumed: (*p).consumed,
            reaper_signalled: (*p).signalled,
            retried: (*p).retried,
            recovered: (*p).recovered,
            padding: [0; 3],
        });
    }
    unlock();
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_selftest_allowed(group: u8) -> bool {
    allowed(group)
}
const _: () = {
    assert!(core::mem::size_of::<Snapshot>() == 12);
    assert!(core::mem::offset_of!(Snapshot, recovered) == 8);
};
