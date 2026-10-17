//! Resident identities outlive flags, waits and failed retirement. Only the
//! admitted controller mutates this ledger; its caller takes over after DONE.
use super::live;
use crate::{
    arch::runtime::RuntimeGuard,
    platform::timer,
    sched::{
        scheduler,
        task::{
            task_lifecycle as life, task_registry as registry, task_resources as resources,
            task_types::{Bytes, CreateFailure, Entry, ExecutionOwnerKind, Role, State, Task},
        },
    },
};
use core::{cell::UnsafeCell, ptr};
const MAX: usize = 96;
#[derive(Clone, Copy)]
struct Record {
    task: *mut Task,
    id: u32,
    generation: u64,
    retained: bool,
    done: bool,
}
impl Record {
    const EMPTY: Self = Self {
        task: ptr::null_mut(),
        id: 0,
        generation: 0,
        retained: false,
        done: false,
    };
}
struct Ledger {
    records: [Record; MAX],
    count: usize,
    reaped: u64,
}
struct Cell(UnsafeCell<Ledger>);
// Exclusive diagnostic gate, then BSP controller, then DONE acquire handover.
unsafe impl Sync for Cell {}
static LEDGER: Cell = Cell(UnsafeCell::new(Ledger {
    records: [Record::EMPTY; MAX],
    count: 0,
    reaped: 0,
}));
pub(super) unsafe fn reset() {
    unsafe {
        LEDGER.0.get().write(Ledger {
            records: [Record::EMPTY; MAX],
            count: 0,
            reaped: 0,
        });
    }
}
pub(crate) fn create(name: &'static [u8], entry: Entry, role: Role) -> *mut Task {
    assert!(live(), "diagnostic task outside admitted session");
    unsafe {
        let count = (*LEDGER.0.get()).count;
        if count == MAX {
            return ptr::null_mut();
        }
        let mut failure = CreateFailure::None;
        let t = resources::create(
            resources::CreateArgs {
                name: Bytes {
                    pointer: name.as_ptr(),
                    len: name.len(),
                },
                state: State::Blocked,
                needs_fpu: true,
                entry: Some(entry),
                role,
                smp_eligible: false,
                smp_r4x: false,
                owner_kind: ExecutionOwnerKind::None,
                owner_context: ptr::null_mut(),
            },
            &mut failure,
        );
        if t.is_null() {
            return t;
        }
        let _runtime = RuntimeGuard::acquire();
        let c = registry::core(t);
        // Blocked construction has no runnable path before this publication.
        let pinned = life::pin(t);
        assert!(pinned, "new blocked diagnostic task pin");
        (*LEDGER.0.get()).records[count] = Record {
            task: t,
            id: (*c).id,
            generation: (*c).generation,
            retained: false,
            done: false,
        };
        (*LEDGER.0.get()).count += 1;
        t
    }
}
pub(crate) fn start(t: *mut Task) -> bool {
    scheduler::publish_created(t.cast())
}
pub(crate) fn identity(t: *mut Task) -> (u32, u64) {
    let _runtime = RuntimeGuard::acquire();
    unsafe {
        let c = registry::core(t);
        ((*c).id, (*c).generation)
    }
}
pub(crate) fn sleep(ticks: u64, reason: &'static [u8]) {
    unsafe {
        scheduler::r4os_rust_scheduler_task(
            4,
            ptr::null_mut(),
            ticks,
            0,
            reason.as_ptr(),
            reason.len(),
        );
    }
}
pub(crate) fn finish_task(t: *mut Task, kill: bool) -> bool {
    if t.is_null() {
        return false;
    }
    let record = unsafe {
        (0..(*LEDGER.0.get()).count)
            .map(|i| (*LEDGER.0.get()).records[i])
            .find(|r| r.task == t)
    };
    record
        .and_then(|r| finish(r.id, r.generation, kill))
        .unwrap_or(false)
}
pub(crate) fn finish(id: u32, generation: u64, kill: bool) -> Option<bool> {
    let index = unsafe {
        (0..(*LEDGER.0.get()).count).find(|&i| {
            let r = (*LEDGER.0.get()).records[i];
            r.id == id && r.generation == generation
        })
    }?;
    let end = timer::tick_count().saturating_add(2000);
    loop {
        let r = unsafe { (*LEDGER.0.get()).records[index] };
        if r.done {
            return Some(true);
        }
        if r.retained {
            return Some(false);
        }
        if kill {
            unsafe {
                life::kill(id, Some(generation));
            }
        }
        // Pending keeps our last pin. Only this exact owner's synchronous
        // metadata/resource return can acknowledge the final retirement.
        match unsafe { life::retire_diagnostic_pin(r.task, id, generation) } {
            life::DiagnosticRetire::Released => {
                unsafe {
                    (*LEDGER.0.get()).records[index].done = true;
                    (*LEDGER.0.get()).records[index].task = ptr::null_mut();
                    (*LEDGER.0.get()).reaped += 1;
                }
                return Some(true);
            }
            life::DiagnosticRetire::Retained => {
                unsafe {
                    (*LEDGER.0.get()).records[index].retained = true;
                }
                return Some(false);
            }
            life::DiagnosticRetire::Pending => {}
        }
        if timer::tick_count() >= end {
            return Some(false);
        }
        sleep(1, b"spectra-task-retire");
    }
}
pub(super) fn cleanup_except(controller: *mut Task) -> bool {
    let mut ok = true;
    let count = unsafe { (*LEDGER.0.get()).count };
    for i in 0..count {
        let r = unsafe { (*LEDGER.0.get()).records[i] };
        if !r.done && (controller.is_null() || r.task != controller) {
            ok &= finish(r.id, r.generation, true) == Some(true);
        }
    }
    ok
}
pub(super) fn summary() -> (u64, u64, u32) {
    unsafe {
        let p = LEDGER.0.get();
        let count = (*p).count as u64;
        (count, (*p).reaped, (count - (*p).reaped) as u32)
    }
}
