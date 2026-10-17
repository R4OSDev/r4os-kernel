//! Missing native contracts: real guarded semaphore handoff and recursive
//! UnwindGuard ownership, including a contending task and kill deferral.
use super::{tasks, update};
use crate::{
    arch::runtime::RuntimeGuard,
    sched::{
        scheduler, sync_locks as locks, sync_objects as objects,
        sync_types::{Semaphore, Text, UnwindGuard, WaitQueue, WaitRequest},
        task::{
            task_lifecycle, task_registry,
            task_types::{Role, Task},
        },
        task_context,
    },
};
use core::{
    cell::UnsafeCell,
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};
struct Objects {
    semaphore: Semaphore,
    guard: UnwindGuard,
}
struct Cell(UnsafeCell<Objects>);
unsafe impl Sync for Cell {}
static OBJECTS: Cell = Cell(UnsafeCell::new(Objects {
    semaphore: Semaphore {
        count: 0,
        max_count: 2,
        queue: WaitQueue::EMPTY,
    },
    guard: UnwindGuard {
        owner: 0,
        owner_generation: 0,
        boot_owner: false,
        depth: 0,
        name: Text::literal(b"spectra-unwind"),
        queue: WaitQueue::EMPTY,
    },
}));
static CONTENDED: AtomicBool = AtomicBool::new(false);
static RELEASED: AtomicBool = AtomicBool::new(false);
static PASSED: AtomicBool = AtomicBool::new(false);
static OWNED: AtomicBool = AtomicBool::new(false);
static ALLOW_RELEASE: AtomicBool = AtomicBool::new(false);
static QUEUED: AtomicBool = AtomicBool::new(false);
fn semaphore() -> *mut Semaphore {
    unsafe { ptr::addr_of_mut!((*OBJECTS.0.get()).semaphore) }
}
fn guard() -> *mut UnwindGuard {
    unsafe { ptr::addr_of_mut!((*OBJECTS.0.get()).guard) }
}
fn depth() -> u32 {
    let _runtime = RuntimeGuard::acquire();
    unsafe { (*task_registry::core(scheduler::current().cast::<Task>())).unwind_guard_count }
}
unsafe extern "C" fn release_permit() {
    for _ in 0..2000 {
        let queued = {
            let _runtime = RuntimeGuard::acquire();
            unsafe { (*semaphore()).queue.core.count == 1 }
        };
        if queued {
            QUEUED.store(true, Ordering::Release);
            break;
        }
        tasks::sleep(1, b"spectra-permit-enrollment");
    }
    unsafe {
        objects::r4os_rust_sync_semaphore(semaphore(), 4, 1, ptr::null(), ptr::null_mut());
    }
}
unsafe extern "C" fn contend() {
    let timed_out = unsafe { !locks::guard_enter(guard(), 2) };
    if !timed_out {
        unsafe {
            locks::guard_leave(guard());
        }
    }
    CONTENDED.store(timed_out, Ordering::Release);
    for _ in 0..2000 {
        if RELEASED.load(Ordering::Acquire) {
            break;
        }
        tasks::sleep(1, b"spectra-guard-release");
    }
    let acquired = RELEASED.load(Ordering::Acquire) && unsafe { locks::guard_enter(guard(), 50) };
    OWNED.store(acquired, Ordering::Release);
    for _ in 0..2000 {
        if ALLOW_RELEASE.load(Ordering::Acquire) {
            break;
        }
        tasks::sleep(1, b"spectra-guard-held");
    }
    let released = acquired && unsafe { locks::guard_leave(guard()) };
    PASSED.store(timed_out && acquired && released, Ordering::Release);
}
fn spawn(name: &'static [u8], entry: unsafe extern "C" fn()) -> *mut Task {
    let t = tasks::create(name, entry, Role::Interactive);
    if !t.is_null() && !tasks::start(t) {
        tasks::finish_task(t, true);
        return ptr::null_mut();
    }
    t
}
pub(super) fn run() -> bool {
    unsafe {
        // Previous sessions can only release the gate after all own tasks
        // and queue references have actually retired.
        (*OBJECTS.0.get()).semaphore = Semaphore {
            count: 0,
            max_count: 2,
            queue: WaitQueue::EMPTY,
        };
        QUEUED.store(false, Ordering::Release);
        let start = depth();
        let child = spawn(b"spectra-guarded-permit", release_permit);
        if child.is_null() {
            return false;
        }
        let req = WaitRequest {
            timeout: 200,
            reason: Text::literal(b"spectra-guarded-permit"),
            predicate: None,
            context: ptr::null_mut(),
            release: None,
            release_context: ptr::null_mut(),
            flags: 0,
            consume_runtime: false,
        };
        let mut token = task_context::Unwind::EMPTY;
        let result = objects::r4os_rust_sync_semaphore(semaphore(), 3, 0, &req, &mut token);
        let retained =
            result == 1 && token.active && depth() == start + 1 && QUEUED.load(Ordering::Acquire);
        let returned = if token.active {
            let _runtime = RuntimeGuard::acquire();
            task_context::leave(token)
        } else {
            false
        };
        let joined = tasks::finish_task(child, false);
        let timeout_start = crate::platform::timer::tick_count();
        let timeout =
            objects::r4os_rust_sync_semaphore(semaphore(), 1, 1, ptr::null(), ptr::null_mut()) == 2
                && crate::platform::timer::tick_count().saturating_sub(timeout_start) >= 1;
        let cap =
            objects::r4os_rust_sync_semaphore(semaphore(), 4, 3, ptr::null(), ptr::null_mut()) == 2;
        let first =
            objects::r4os_rust_sync_semaphore(semaphore(), 0, 0, ptr::null(), ptr::null_mut()) == 1;
        let second =
            objects::r4os_rust_sync_semaphore(semaphore(), 0, 0, ptr::null(), ptr::null_mut()) == 1;
        let empty =
            objects::r4os_rust_sync_semaphore(semaphore(), 0, 0, ptr::null(), ptr::null_mut()) == 0;
        if !(retained
            && returned
            && joined
            && timeout
            && cap
            && first
            && second
            && empty
            && depth() == start)
        {
            return false;
        }
        update(|r| r.unwind_checks += 1);
        CONTENDED.store(false, Ordering::Release);
        RELEASED.store(false, Ordering::Release);
        PASSED.store(false, Ordering::Release);
        OWNED.store(false, Ordering::Release);
        ALLOW_RELEASE.store(false, Ordering::Release);
        if !locks::guard_enter(guard(), 0) {
            return false;
        }
        let recursive = locks::guard_enter(guard(), 0) && depth() == start + 1;
        let contender = spawn(b"spectra-unwind-contender", contend);
        let mut observed = false;
        for _ in 0..2000 {
            if CONTENDED.load(Ordering::Acquire) {
                observed = true;
                break;
            }
            tasks::sleep(1, b"spectra-guard-contended");
        }
        let first_leave = locks::guard_leave(guard());
        let still_owned = locks::guard_owned(guard()) && depth() == start + 1;
        let last_leave = locks::guard_leave(guard());
        RELEASED.store(true, Ordering::Release);
        let mut deferred = false;
        for _ in 0..2000 {
            if !contender.is_null() && OWNED.load(Ordering::Acquire) {
                let blocked = {
                    let _runtime = RuntimeGuard::acquire();
                    (*task_registry::core(contender)).state
                        == crate::sched::task::task_types::State::Blocked
                };
                if blocked {
                    let (id, generation) = tasks::identity(contender);
                    let metadata = locks::guard_metadata(guard());
                    deferred = metadata.owner == id
                        && metadata.generation == generation
                        && !task_lifecycle::kill(id, Some(generation));
                    break;
                }
            }
            tasks::sleep(1, b"spectra-guard-owner");
        }
        ALLOW_RELEASE.store(true, Ordering::Release);
        let joined = !contender.is_null() && tasks::finish_task(contender, false);
        let metadata = locks::guard_metadata(guard());
        let ok = recursive
            && deferred
            && observed
            && first_leave
            && still_owned
            && last_leave
            && joined
            && PASSED.load(Ordering::Acquire)
            && depth() == start
            && metadata.owner == 0
            && metadata.generation == 0
            && metadata.depth == 0
            && !metadata.boot_owner;
        if ok {
            update(|r| r.unwind_checks += 1);
        }
        ok
    }
}
