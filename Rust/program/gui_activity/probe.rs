//! Additional observations of the original two owned CALC processes. No new
//! suite, fake instance/IRQ, foreign queue mutation or periodic product work.
use super::*;
use crate::{
    platform::timer,
    sched::{
        scheduler,
        task::{
            task_lifecycle, task_registry,
            task_types::{ExecutionOwnerKind, State, Task},
        },
    },
};
use core::sync::atomic::{AtomicU32, Ordering};

pub(crate) fn empty() -> bool {
    let _runtime = RuntimeGuard::acquire();
    unsafe { (*SUBSCRIBERS.0.get()).is_null() }
}

pub(crate) unsafe fn shared_sequences(handles: &[c::ProgramProcessHandle]) -> Option<[u64; 2]> {
    let first = unsafe { Lease::handle(handles[0]) }?;
    let second = unsafe { Lease::handle(handles[1]) }?;
    let _runtime = RuntimeGuard::acquire();
    let mut sequences = [0; 2];
    for (i, lease) in [&first, &second].iter().enumerate() {
        let gui = unsafe { (*lease.0.instance).gui_payload.cast::<t::Gui>() };
        if gui.is_null()
            || unsafe {
                (*gui).activity.queue.core.closing
                    || (*gui).activity.subscriptions != c::gui_activity_shared_clipboard
            }
        {
            return None;
        }
        sequences[i] = unsafe { (*gui).activity.sequence };
    }
    Some(sequences)
}

pub(crate) unsafe fn shared_commits(handles: &[c::ProgramProcessHandle], before: [u64; 2]) -> bool {
    let Some(after) = (unsafe { shared_sequences(handles) }) else {
        return false;
    };
    let delta = [
        after[0].wrapping_sub(before[0]),
        after[1].wrapping_sub(before[1]),
    ];
    // Existing live /SELFTEST: clear, write, clear. Provider startup can have
    // happened on CALC's first clipboard query or after the earlier snapshot.
    let ok = delta[0] == delta[1] && (3..=4).contains(&delta[0]);
    crate::boot::log_route::puts(if ok {
        b"GUIACT08735 CLIPSVC PASS real-commits=3 receivers=2 publications="
    } else {
        b"GUIACT08735 CLIPSVC FAIL publications="
    });
    crate::boot::log_route::dec(delta[0]);
    crate::boot::log_route::puts(b",");
    crate::boot::log_route::dec(delta[1]);
    crate::boot::log_route::puts(b"\n");
    ok
}

struct TaskPin(*mut Task);
impl Drop for TaskPin {
    fn drop(&mut self) {
        assert!(unsafe { task_lifecycle::unpin(self.0) });
    }
}
// Keep a separate exact Task receipt alongside the program payload lease.
unsafe fn pin(lease: &Lease, handle: c::ProgramProcessHandle) -> Option<TaskPin> {
    let instance = lease.0.instance;
    let pin = {
        let _runtime = RuntimeGuard::acquire();
        let task = unsafe { task_lifecycle::pin_identity((*instance).task_id, None) };
        if task.is_null() {
            None
        } else {
            Some(TaskPin(task))
        }
    }?;
    let valid = {
        let _runtime = RuntimeGuard::acquire();
        unsafe {
            let core = task_registry::core(pin.0);
            if (*core).execution_owner_kind != ExecutionOwnerKind::ProgramThread
                || matches!((*core).state, State::Unused | State::Dead)
            {
                false
            } else {
                let thread = (*core).execution_owner_context.cast::<threads::Thread>();
                !thread.is_null()
                    && (*thread).owner_instance == instance
                    && (*thread).instance_id == handle.instance_id
                    && (*thread).instance_generation == handle.generation
                    && (*thread).task_generation == (*core).generation
                    && (*thread).task_id == (*core).id
                    && (*thread).flags & threads::MAIN != 0
            }
        }
    };
    // Unpin can complete terminal metadata release; never inherit our Runtime.
    if valid { Some(pin) } else { None }
}
unsafe fn blocked(task: *mut Task, gui: *mut t::Gui) -> bool {
    if task.is_null() || gui.is_null() {
        return false;
    }
    unsafe {
        let core = task_registry::core(task);
        let node = ptr::addr_of!((*core).wait_node);
        let queue = ptr::addr_of!((*gui).activity.queue.core);
        (*core).state == State::Blocked
            && (*core).running_cpu == 0xff
            && !(*core).timeout_linked
            && (*core).wake_tick == 0
            && (*node).owner == task.cast()
            && (*node).owner_generation == (*core).generation
            && (*node).queue == queue.cast_mut()
            && (*queue).head == node.cast_mut()
            && (*queue).tail == node.cast_mut()
            && (*queue).count == 1
            && !(*queue).closing
    }
}
fn wait(mut test: impl FnMut() -> bool) -> bool {
    let end = timer::tick_count().saturating_add(2000);
    while timer::tick_count() < end {
        if test() {
            return true;
        }
        scheduler::yield_task();
    }
    test()
}
pub(crate) unsafe fn run(handles: &[c::ProgramProcessHandle]) -> bool {
    let Some(first) = (unsafe { Lease::handle(handles[0]) }) else {
        return false;
    };
    let Some(second) = (unsafe { Lease::handle(handles[1]) }) else {
        return false;
    };
    let leases = [&first, &second];
    let guis = unsafe { leases.map(|lease| (*lease.0.instance).gui_payload.cast::<t::Gui>()) };
    let Some(first_task) = (unsafe { pin(&first, handles[0]) }) else {
        return false;
    };
    let Some(second_task) = (unsafe { pin(&second, handles[1]) }) else {
        return false;
    };
    let tasks = [first_task.0, second_task.0];
    let pair = move || {
        let _runtime = RuntimeGuard::acquire();
        unsafe { (0..2).all(|i| blocked(tasks[i], guis[i])) }
    };
    let mut stage = 1;
    let ok = (|| {
        if !wait(pair) {
            return false;
        }
        stage = 2;
        let revision = unsafe {
            AtomicU32::from_ptr(ptr::addr_of_mut!((*guis[0]).revision)).load(Ordering::Acquire)
        };
        {
            let _runtime = RuntimeGuard::acquire();
            if !unsafe { blocked(tasks[0], guis[0]) && blocked(tasks[1], guis[1]) } {
                return false;
            }
            let sequences = unsafe { guis.map(|gui| (*gui).activity.sequence) };
            for _ in 0..32 {
                desktop_events::r4os_rust_desktop_signal();
            }
            if notify(c::gui_activity_shared_directory) != 0
                || !unsafe { blocked(tasks[0], guis[0]) && blocked(tasks[1], guis[1]) }
                || unsafe { guis.map(|gui| (*gui).activity.sequence) } != sequences
            {
                return false;
            }
            // Real input publication: only our first exact GUI becomes Ready.
            let event = c::GuiEvent {
                kind: 6,
                key: b'8'.into(),
                ..Default::default()
            };
            if unsafe { gui_events::enqueue(guis[0], &event) } != 0
                || unsafe { (*task_registry::core(tasks[0])).state } != State::Ready
                || !unsafe { blocked(tasks[1], guis[1]) }
                || unsafe { (*guis[1]).activity.sequence } != sequences[1]
            {
                return false;
            }
        }
        stage = 3;
        if !wait(|| {
            pair()
                && unsafe {
                    AtomicU32::from_ptr(ptr::addr_of_mut!((*guis[0]).revision))
                        .load(Ordering::Acquire)
                        != revision
                }
        }) {
            return false;
        }
        stage = 4;
        {
            let _runtime = RuntimeGuard::acquire();
            if !unsafe { blocked(tasks[0], guis[0]) && blocked(tasks[1], guis[1]) } {
                return false;
            }
            let original = unsafe { (*guis[0]).window_info };
            // The real metadata publisher must ignore a repeated snapshot.
            if unsafe {
                gui_events::r4os_rust_program_gui_window_set(handles[0].instance_id, &original)
            } != 0
                || !unsafe { blocked(tasks[0], guis[0]) && blocked(tasks[1], guis[1]) }
            {
                return false;
            }
            let mut changed = original;
            changed.frame_x = changed.frame_x.wrapping_add(1);
            if unsafe {
                gui_events::r4os_rust_program_gui_window_set(handles[0].instance_id, &changed)
            } != 0
                || unsafe { (*task_registry::core(tasks[0])).state } != State::Ready
                || !unsafe { blocked(tasks[1], guis[1]) }
                || unsafe {
                    gui_events::r4os_rust_program_gui_window_set(handles[0].instance_id, &original)
                } != 0
            {
                return false;
            }
        }
        stage = 5;
        if !wait(pair) {
            return false;
        }
        stage = 6;
        {
            let _runtime = RuntimeGuard::acquire();
            if !unsafe { blocked(tasks[0], guis[0]) && blocked(tasks[1], guis[1]) }
                || unsafe {
                    guis.iter().any(|gui| {
                        (**gui).activity.subscriptions != c::gui_activity_shared_clipboard
                    })
                }
            {
                return false;
            }
            if notify(c::gui_activity_shared_clipboard) != 0
                || unsafe {
                    tasks
                        .iter()
                        .any(|task| (*task_registry::core(*task)).state != State::Ready)
                }
            {
                return false;
            }
        }
        stage = 7;
        wait(pair)
    })();
    crate::boot::log_route::puts(if ok {
        b"GUIACT08735 PASS desktop=32 isolated=2 directory-unsubscribed=2 targeted=1 window-receiver=1 unchanged-snapshot=2 clipboard=2 forever-timeouts=0\n"
    } else {
        b"GUIACT08735 FAIL stage="
    });
    if !ok {
        crate::boot::log_route::dec(stage);
        crate::boot::log_route::puts(b"\n");
    }
    // The original caller then performs real Close/Kill, joins, reap and
    // storage-baseline receipts. This observation is not a retirement ACK.
    ok
}
