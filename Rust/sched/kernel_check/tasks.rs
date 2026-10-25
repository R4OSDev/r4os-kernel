//! The scheduler check's resident ledger. Only its admitted BSP controller
//! mutates it until DONE transfers ownership to the waiting caller.
use super::live;
pub(crate) use crate::sched::task::diagnostic::{identity, sleep, start};
use crate::sched::task::{
    diagnostic::Ledger,
    task_types::{Entry, Role, Task},
};
use core::cell::UnsafeCell;
// The complete selected profile keeps historical receipts for earlier cases
// before admitting the 65-task deadline case. 96 entries exhausted that
// session despite every earlier task being reaped. This bounds diagnostic
// history only; it is not the system Task capacity or a cleanup shortcut.
// 0.87.30 adds 31 exact Mutex-chain receipts to the 104-task scheduler
// profile; the combined profile also retains Sync/FPU/DriverWork receipts.
const SESSION_RECORDS: usize = 192;
struct Cell(UnsafeCell<Ledger<SESSION_RECORDS>>);
// Exclusive gate -> BSP controller -> DONE acquire -> original caller.
unsafe impl Sync for Cell {}
static LEDGER: Cell = Cell(UnsafeCell::new(Ledger::EMPTY));
pub(super) unsafe fn reset() {
    unsafe {
        LEDGER.0.get().write(Ledger::EMPTY);
    }
}
pub(crate) fn create(name: &'static [u8], entry: Entry, role: Role) -> *mut Task {
    assert!(live(), "diagnostic task outside admitted session");
    unsafe { (&mut *LEDGER.0.get()).create(name, entry, role, None) }
}
pub(crate) fn create_on_cpu(name: &'static [u8], entry: Entry, role: Role, cpu: u32) -> *mut Task {
    assert!(live(), "diagnostic task outside admitted session");
    unsafe { (&mut *LEDGER.0.get()).create(name, entry, role, Some(cpu)) }
}
pub(crate) fn finish_task(t: *mut Task, kill: bool) -> bool {
    unsafe { (&mut *LEDGER.0.get()).finish_task(t, kill) }
}
pub(crate) fn finish(id: u32, generation: u64, kill: bool) -> Option<bool> {
    unsafe { (&mut *LEDGER.0.get()).finish(id, generation, kill) }
}
pub(super) fn cleanup_except(controller: *mut Task) -> bool {
    unsafe { (&mut *LEDGER.0.get()).cleanup_except(controller) }
}
pub(super) fn summary() -> (u64, u64, u32) {
    unsafe { (&*LEDGER.0.get()).summary() }
}
