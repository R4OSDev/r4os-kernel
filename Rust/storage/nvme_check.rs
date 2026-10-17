//! One explicit read-only request, with native and hardware completion identity.
mod case;
mod live;
mod observer;
use crate::{
    contract as c,
    platform::timer,
    sched::{
        task::{
            diagnostic,
            task_types::{Role, Task},
        },
        task_context::Admission,
    },
    storage::{
        block_observation::{ReadReceipt, Subscription},
        block_registry,
    },
};
use core::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    ptr,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};
pub(crate) use live::check;
#[repr(align(4096))]
struct Sector(UnsafeCell<[u8; 4096]>);
// The exclusive run gate holds this resident backing through real DMA release.
unsafe impl Sync for Sector {}
static SECTOR: Sector = Sector(UnsafeCell::new([0; 4096]));
struct Session {
    report: c::ProgramKernelNvmeCheck,
    tasks: diagnostic::Ledger<1>,
    controller: *mut Task,
    pin: MaybeUninit<block_registry::Pin>,
    pin_active: bool,
    buffer_active: bool,
    subscription: Subscription,
    receipt: ReadReceipt,
}
struct Cell(UnsafeCell<MaybeUninit<Session>>);
// Gate -> one BSP controller -> DONE acquire -> exact caller. Observer has its
// separate single-writer release/acquire cell, never mutable caller output.
unsafe impl Sync for Cell {}
static SESSION: Cell = Cell(UnsafeCell::new(MaybeUninit::uninit()));
static BUSY: AtomicBool = AtomicBool::new(false);
static CANCEL: AtomicBool = AtomicBool::new(false);
fn session() -> *mut Session {
    SESSION.0.get().cast()
}
struct Gate(bool);
impl Drop for Gate {
    fn drop(&mut self) {
        if self.0 {
            BUSY.store(false, Ordering::Release);
        }
    }
}
fn gate() -> Option<Gate> {
    BUSY.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .ok()
        .map(|_| Gate(true))
}
unsafe fn reset(run_id: u64) {
    unsafe {
        session().write(Session {
            report: c::ProgramKernelNvmeCheck {
                run_id,
                ..Default::default()
            },
            tasks: diagnostic::Ledger::EMPTY,
            controller: ptr::null_mut(),
            pin: MaybeUninit::uninit(),
            pin_active: false,
            buffer_active: false,
            subscription: Subscription {
                cookie: run_id,
                observe: observer::receive,
            },
            receipt: ReadReceipt::default(),
        });
    }
    CANCEL.store(false, Ordering::Release);
    observer::reset(run_id);
}
fn buffer() -> *mut u8 {
    SECTOR.0.get().cast()
}
fn any_device() -> bool {
    (0..crate::storage::access_state::MAX_DEVICES)
        .any(|index| crate::storage::block_device::config(index).is_some_and(|v| v.bus == 3))
}
