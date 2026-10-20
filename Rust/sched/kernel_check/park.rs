//! Bounded real AP wake/close at both sides of the physical park boundary.
//! Session gate retains CPU membership; exact joins precede queue reuse/reset.
use super::tasks;
use crate::{
    arch::{
        percpu,
        runtime::{self, RuntimeGuard},
    },
    platform::clocksource,
    sched::{
        scheduler,
        sync_types::{Text, WaitQueue, WaitRequest},
        sync_wait,
        task::{
            task_registry as registry,
            task_types::{Role, State, Task, WaitResult},
        },
        wait_node,
    },
};
use core::{
    cell::UnsafeCell,
    ptr,
    sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU64, Ordering},
};
struct Cell(UnsafeCell<WaitQueue>);
// Only this admitted session; all queue access retains Runtime.
unsafe impl Sync for Cell {}
static QUEUE: Cell = Cell(UnsafeCell::new(WaitQueue::EMPTY));
static TARGET: AtomicPtr<Task> = AtomicPtr::new(ptr::null_mut());
static GENERATION: AtomicU64 = AtomicU64::new(0);
static MODE: AtomicU32 = AtomicU32::new(0);
static PEER_CPU: AtomicU32 = AtomicU32::new(0);
static DONE: AtomicBool = AtomicBool::new(false);
static PASSED: AtomicBool = AtomicBool::new(false);
static STOP: AtomicBool = AtomicBool::new(false);
const REUSED: u64 = 0x0866_c105_ed00_0000;
fn deadline() -> u64 {
    clocksource::now_nanoseconds()
        .unwrap_or(u64::MAX)
        .saturating_add(2_000_000_000)
}
fn before(end: u64) -> bool {
    clocksource::now_nanoseconds().unwrap_or(u64::MAX) < end
}
fn queue() -> *mut WaitQueue {
    QUEUE.0.get()
}
unsafe extern "C" fn producer() {
    let Some(_admission) = crate::sched::task_context::Admission::acquire(true) else {
        DONE.store(true, Ordering::Release);
        return;
    };
    let end = deadline();
    while before(end) && !STOP.load(Ordering::Acquire) {
        let completed = {
            let _owner = RuntimeGuard::acquire();
            unsafe {
                let t = TARGET.load(Ordering::Acquire);
                let c = registry::core(t);
                let mode = MODE.load(Ordering::Relaxed);
                let node = ptr::addr_of_mut!((*c).wait_node);
                let physical = scheduler::current_on_cpu(0) == t.cast();
                if (*queue()).core.count == 1
                    && (*c).state == State::Blocked
                    && physical == (mode < 2)
                {
                    let exact = (*c).generation == GENERATION.load(Ordering::Relaxed)
                        && (*node).owner == t.cast()
                        && (*node).owner_generation == (*c).generation
                        && (*node).queue == ptr::addr_of_mut!((*queue()).core)
                        && (*queue()).core.head == node
                        && runtime::current_cpu() == PEER_CPU.load(Ordering::Relaxed);
                    let closed = mode & 1 != 0;
                    let expected = if closed {
                        WaitResult::Cancelled
                    } else {
                        WaitResult::Signaled
                    };
                    let result = sync_wait::r4os_rust_sync_queue(
                        queue(),
                        if closed { 4 } else { 0 },
                        ptr::null_mut(),
                        if closed { 3 } else { 0 },
                        ptr::null_mut(),
                    );
                    let ok = exact
                        && result == if closed { 1 } else { u64::from((*c).id) }
                        && (*queue()).core.count == 0
                        && (*node).queue.is_null()
                        && (*c).state == State::Ready
                        && (*c).wait_result == expected;
                    // The old queue lifetime ends after exact detach. Reuse its
                    // storage before the waiter continues; never retain a borrow.
                    queue().write(WaitQueue::EMPTY);
                    (*queue()).core.closing = true;
                    (*queue()).core.generation = REUSED | u64::from(mode);
                    PASSED.store(ok, Ordering::Release);
                    true
                } else {
                    false
                }
            }
        };
        if completed {
            DONE.store(true, Ordering::Release);
            return;
        }
        // Early cases keep BSP IF=0; a sleeping AP would depend on its
        // timer owner. Poll independently, bounded by the hardware clock.
        for _ in 0..128 {
            core::hint::spin_loop();
        }
    }
    DONE.store(true, Ordering::Release);
}
fn one(mode: u32, cpu: u32) -> bool {
    let t = scheduler::current().cast::<Task>();
    {
        let _owner = RuntimeGuard::acquire();
        unsafe {
            queue().write(WaitQueue::EMPTY);
            GENERATION.store((*registry::core(t)).generation, Ordering::Relaxed);
        }
        TARGET.store(t, Ordering::Release);
        MODE.store(mode, Ordering::Relaxed);
        PEER_CPU.store(cpu, Ordering::Relaxed);
        DONE.store(false, Ordering::Relaxed);
        PASSED.store(false, Ordering::Relaxed);
        STOP.store(false, Ordering::Relaxed);
    }
    let peer = tasks::create_on_cpu(b"spectra-park-producer", producer, Role::Interactive, cpu);
    if peer.is_null() {
        return false;
    }
    if !tasks::start(peer) {
        tasks::finish_task(peer, true);
        return false;
    }
    // IF=0 excludes timer admissions during the bounded early-wake window.
    // The real AP producer remains independently runnable.
    let original = runtime::save_local();
    let result = unsafe {
        if mode < 2 {
            let flags = runtime::save_runtime();
            let c = registry::core(t);
            let linked = wait_node::link(
                ptr::addr_of_mut!((*queue()).core),
                ptr::addr_of_mut!((*c).wait_node),
                t.cast(),
                (*c).generation,
            );
            let blocked = if linked {
                scheduler::block_current_owned(
                    queue().addr() as u64,
                    u64::MAX,
                    b"spectra-before-park".as_ptr(),
                    19,
                )
            } else {
                ptr::null_mut()
            };
            if blocked.is_null() {
                wait_node::detach(ptr::addr_of_mut!((*c).wait_node));
            }
            runtime::restore_runtime(flags);
            if blocked.is_null() {
                WaitResult::Failed
            } else {
                let end = deadline();
                while !DONE.load(Ordering::Acquire) && before(end) {
                    core::hint::spin_loop();
                }
                if !DONE.load(Ordering::Acquire) {
                    STOP.store(true, Ordering::Release);
                    sync_wait::r4os_rust_sync_queue(
                        queue(),
                        4,
                        ptr::null_mut(),
                        3,
                        ptr::null_mut(),
                    );
                }
                scheduler::r4os_rust_scheduler_task(1, blocked, 0, 0, ptr::null(), 0);
                let _owner = RuntimeGuard::acquire();
                (*registry::core(t)).wait_result
            }
        } else {
            let request = WaitRequest {
                timeout: 2000,
                reason: Text::literal(b"spectra-after-save"),
                predicate: None,
                context: ptr::null_mut(),
                release: None,
                release_context: ptr::null_mut(),
                flags: 0,
                consume_runtime: false,
            };
            sync_wait::wait(queue(), &request)
        }
    };
    let if_off = runtime::save_local() & (1 << 9) == 0;
    runtime::restore_local(original);
    let resumed = {
        let _owner = RuntimeGuard::acquire();
        unsafe {
            let c = registry::core(t);
            (*c).generation == GENERATION.load(Ordering::Relaxed)
                && scheduler::current() == t.cast()
                && (*c).state == State::Running
                && (*c).preempt_disable_depth == 0
                && (*c).wait_node.queue.is_null()
                && (*c).wait_result == result
        }
    };
    let end = deadline();
    while !DONE.load(Ordering::Acquire) && before(end) {
        tasks::sleep(1, b"spectra-park-producer-return");
    }
    STOP.store(true, Ordering::Release);
    let joined = tasks::finish_task(peer, !DONE.load(Ordering::Acquire));
    let reused = {
        let _owner = RuntimeGuard::acquire();
        unsafe {
            (*queue()).core.closing
                && (*queue()).core.count == 0
                && (*queue()).core.head.is_null()
                && (*queue()).core.tail.is_null()
                && (*queue()).core.generation == REUSED | u64::from(mode)
        }
    };
    let expected = if mode & 1 == 0 {
        WaitResult::Signaled
    } else {
        WaitResult::Cancelled
    };
    joined && resumed && if_off && reused && result == expected && PASSED.load(Ordering::Acquire)
}
pub(crate) fn run() -> bool {
    let online = percpu::r4os_rust_percpu_control(14, 0, 0, 0) & !1;
    if online == 0 || clocksource::now_nanoseconds().is_none() {
        crate::boot::log_route::puts(
            b"PARKHANDOFF0866 unavailable: AP and hardware clock required\n",
        );
        return true;
    }
    let cpu = online.trailing_zeros();
    for mode in 0..4 {
        if !one(mode, cpu) {
            return false;
        }
    }
    crate::boot::log_route::puts(b"PARKHANDOFF0866 PASS early-wake=1 early-close=1 after-save-wake=1 after-save-close=1 joins=4 queue-reuse=4 IF-off=4 generations=exact\n");
    true
}
