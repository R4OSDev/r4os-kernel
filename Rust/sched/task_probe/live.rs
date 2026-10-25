//! Same role/FPU assertions as boot, with exact live-session retirement.
//! Timer manipulation and global reserve injections deliberately stay boot-only.
use super::{deadlines, helpers::*, resources, roles, state::*};
use crate::{
    arch::{fpu, runtime::RuntimeGuard},
    sched::{
        kernel_check::{self, tasks},
        scheduler,
        task::{
            self, task_registry,
            task_types::{State, Task, WaitResult},
        },
    },
};
use core::{cell::UnsafeCell, ptr};
struct Deadlines {
    tasks: [*mut Task; STORM],
    ticks: [u64; STORM],
    completed: u64,
    failed: bool,
}
struct Cell(UnsafeCell<Deadlines>);
// Only the admitted BSP controller and its BSP-only tasks access these fields.
unsafe impl Sync for Cell {}
static DEADLINES: Cell = Cell(UnsafeCell::new(Deadlines {
    tasks: [ptr::null_mut(); STORM],
    ticks: [0; STORM],
    completed: 0,
    failed: false,
}));
pub(crate) unsafe fn begin() {
    unsafe {
        reset();
    }
}
pub(crate) unsafe fn case(bit: u32) -> Option<bool> {
    unsafe {
        Some(match bit {
            2 => {
                let result = roles::directed();
                if result {
                    kernel_check::update(|r| r.role_mask = 15);
                }
                result
            }
            3 => {
                roles::fairness()
                    && scheduler::slice_probe::run()
                    && super::balance::run()
                    && kernel_check::park::run()
            }
            4 => {
                roles::inversion()
                    && scheduler::rank_probe::run()
                    && crate::sched::mutex_pi::probe::run()
            }
            5 => real_deadlines(),
            6 => {
                let mut status = core::mem::MaybeUninit::uninit();
                fpu::r4os_rust_fpu_status(status.as_mut_ptr());
                if !status.assume_init().avx_enabled {
                    return None;
                }
                resources::work_fpu()
            }
            _ => return None,
        })
    }
}
unsafe extern "C" fn deadline_main() {
    unsafe {
        let p = DEADLINES.0.get();
        let t = scheduler::current().cast::<Task>();
        let Some(index) = (*p).tasks.iter().position(|&v| v == t) else {
            (*p).failed = true;
            return;
        };
        let _runtime = RuntimeGuard::acquire();
        let c = task_registry::core(t);
        if now() < (*p).ticks[index] || (*c).wait_result != WaitResult::Timeout {
            (*p).failed = true;
        }
        (*p).completed += 1;
    }
}
unsafe fn real_deadlines() -> bool {
    unsafe {
        let p = DEADLINES.0.get();
        p.write(Deadlines {
            tasks: [ptr::null_mut(); STORM],
            ticks: [0; STORM],
            completed: 0,
            failed: false,
        });
        let mut probes = [ptr::null_mut(); STORM];
        if !deadlines::allocate(&mut probes, deadline_main) {
            return false;
        }
        (*p).tasks = probes;
        let start = now();
        let enrolled = {
            let _runtime = RuntimeGuard::acquire();
            for (i, t) in probes.into_iter().enumerate() {
                let deadline = start.saturating_add(if i == 0 { EARLY_TICKS } else { LONG_TICKS });
                (*p).ticks[i] = deadline;
                task::diagnostic_timeout(t, deadline, b"spectra-real-deadline");
            }
            probes.iter().all(|&t| view(t).state == State::Blocked)
        };
        let end = start.saturating_add(2000);
        while (*p).completed < STORM as u64 && now() < end && !kernel_check::cancelled() {
            tasks::sleep(1, b"spectra-deadline-join");
        }
        let passed = enrolled && (*p).completed == STORM as u64 && !(*p).failed && queue_valid();
        let mut cleaned = true;
        for t in probes {
            let v = view(t);
            if !unpin(t) || !release(v.id, v.generation) {
                cleaned = false;
            }
        }
        if passed && cleaned {
            kernel_check::update(|r| r.deadline_tasks = STORM as u64);
        }
        passed && cleaned
    }
}
