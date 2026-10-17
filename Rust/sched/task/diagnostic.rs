//! Exact task receipts shared by bounded diagnostic owners. One controller
//! owns a ledger; release/acquire completion hands it to its waiting caller.
use super::{
    task_lifecycle as life, task_registry as registry, task_resources as resources,
    task_types::{Bytes, CreateFailure, Entry, ExecutionOwnerKind, Role, State, Task},
};
use crate::{arch::runtime::RuntimeGuard, platform::timer, sched::scheduler};
use core::ptr;
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
/// Diagnostic load bound, never a system task capacity. This type is not Sync;
/// each owner supplies exclusive resident storage and its completion handoff.
pub(crate) struct Ledger<const MAX: usize> {
    records: [Record; MAX],
    count: usize,
    reaped: u64,
}
impl<const MAX: usize> Ledger<MAX> {
    pub(crate) const EMPTY: Self = Self {
        records: [Record::EMPTY; MAX],
        count: 0,
        reaped: 0,
    };
    pub(crate) fn create(
        &mut self,
        name: &'static [u8],
        entry: Entry,
        role: Role,
        cpu: Option<u32>,
    ) -> *mut Task {
        if self.count == MAX {
            return ptr::null_mut();
        }
        let mut failure = CreateFailure::None;
        let t = unsafe {
            resources::create(
                resources::CreateArgs {
                    name: Bytes {
                        pointer: name.as_ptr(),
                        len: name.len(),
                    },
                    state: State::Blocked,
                    needs_fpu: true,
                    entry: Some(entry),
                    role,
                    smp_eligible: cpu.is_some(),
                    smp_r4x: false,
                    owner_kind: ExecutionOwnerKind::None,
                    owner_context: ptr::null_mut(),
                },
                &mut failure,
            )
        };
        if t.is_null() {
            return t;
        }
        let (id, generation) = {
            let _runtime = RuntimeGuard::acquire();
            unsafe {
                let c = registry::core(t);
                assert!(life::pin(t), "new blocked diagnostic task pin");
                let record = Record {
                    task: t,
                    id: (*c).id,
                    generation: (*c).generation,
                    retained: false,
                    done: false,
                };
                self.records[self.count] = record;
                self.count += 1;
                (record.id, record.generation)
            }
        };
        if let Some(cpu) = cpu
            && !unsafe { life::bind_diagnostic_cpu(t, cpu) }
        {
            let _ = self.finish(id, generation, true);
            return ptr::null_mut();
        }
        t
    }
    pub(crate) fn finish_task(&mut self, t: *mut Task, kill: bool) -> bool {
        if t.is_null() {
            return false;
        }
        let record = self.records[..self.count]
            .iter()
            .copied()
            .find(|r| r.task == t);
        record
            .and_then(|r| self.finish(r.id, r.generation, kill))
            .unwrap_or(false)
    }
    pub(crate) fn finish(&mut self, id: u32, generation: u64, kill: bool) -> Option<bool> {
        let index = self.records[..self.count]
            .iter()
            .position(|r| r.id == id && r.generation == generation)?;
        let end = timer::tick_count().saturating_add(2000);
        loop {
            let r = self.records[index];
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
            match unsafe { life::retire_diagnostic_pin(r.task, id, generation) } {
                life::DiagnosticRetire::Released => {
                    self.records[index].done = true;
                    self.records[index].task = ptr::null_mut();
                    self.reaped += 1;
                    return Some(true);
                }
                life::DiagnosticRetire::Retained => {
                    self.records[index].retained = true;
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
    pub(crate) fn cleanup_except(&mut self, controller: *mut Task) -> bool {
        self.finish_except(controller, true)
    }
    pub(crate) fn finish_except(&mut self, controller: *mut Task, kill: bool) -> bool {
        let mut ok = true;
        for i in 0..self.count {
            let r = self.records[i];
            if !r.done && (controller.is_null() || r.task != controller) {
                ok &= self.finish(r.id, r.generation, kill) == Some(true);
            }
        }
        ok
    }
    pub(crate) fn summary(&self) -> (u64, u64, u32) {
        (
            self.count as u64,
            self.reaped,
            (self.count as u64 - self.reaped) as u32,
        )
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
