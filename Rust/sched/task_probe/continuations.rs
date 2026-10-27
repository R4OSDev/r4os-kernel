//! Explicit physical normal-Work turns, exact pending tickets, timer waits,
//! queued cancellation and old terminal-result semantics. No diagnostic task,
//! queue, synthetic completion, clock or permanent scheduling policy.
use super::{fpu, helpers::*};
use crate::{contract as a, driver::work, sched::kernel_check};
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

static HANDLES: [AtomicU32; 4] = [const { AtomicU32::new(0) }; 4];
static TURNS: AtomicU32 = AtomicU32::new(0);
static CANCEL_TURNS: AtomicU32 = AtomicU32::new(0);
static OBSERVED: AtomicBool = AtomicBool::new(false);
static BAD: AtomicBool = AtomicBool::new(false);
static SUBMITTED: AtomicU64 = AtomicU64::new(0);
static STARTED: AtomicU64 = AtomicU64::new(0);
static WAIT_TICK: AtomicU64 = AtomicU64::new(0);

pub(super) fn retained() -> bool {
    HANDLES.iter().any(|v| v.load(Ordering::Acquire) != 0)
}

unsafe extern "C" fn callback(kind: usize) -> i32 {
    match kind {
        0 => {
            let turn = TURNS.fetch_add(1, Ordering::AcqRel);
            let handle = HANDLES[0].load(Ordering::Acquire);
            let mut status = a::DriverCompletionStatus::default();
            let valid = unsafe { work::r4os_rust_driver_work_status(handle, &mut status) } == 0
                && status.handle == handle
                && status.state == a::driver_work_state_running
                && status.flags == a::driver_work_flag_continuation
                && status.completed_tick == 0
                && work::r4os_rust_driver_work_release(handle) == -2;
            if turn == 0 {
                SUBMITTED.store(status.submitted_tick, Ordering::Release);
                STARTED.store(status.started_tick, Ordering::Release);
            }
            if !valid
                || status.submitted_tick != SUBMITTED.load(Ordering::Acquire)
                || status.started_tick != STARTED.load(Ordering::Acquire)
            {
                BAD.store(true, Ordering::Release);
            }
            // Reuse the real nested IRQ/full-YMM guard witness in every turn;
            // no volatile register is expected to survive a C callback return.
            if unsafe { fpu::callback(0) } != 0 {
                BAD.store(true, Ordering::Release);
            }
            match turn {
                0 => a::driver_work_result_continue,
                1 => {
                    if !OBSERVED.load(Ordering::Acquire) {
                        BAD.store(true, Ordering::Release);
                    }
                    WAIT_TICK.store(now(), Ordering::Release);
                    a::driver_work_result_wait
                }
                2 => {
                    if now() <= WAIT_TICK.load(Ordering::Acquire) {
                        BAD.store(true, Ordering::Release);
                    }
                    0
                }
                _ => {
                    BAD.store(true, Ordering::Release);
                    -1
                }
            }
        }
        1 => {
            if TURNS.load(Ordering::Acquire) != 1 {
                BAD.store(true, Ordering::Release);
            }
            OBSERVED.store(true, Ordering::Release);
            0
        }
        2 => {
            CANCEL_TURNS.fetch_add(1, Ordering::AcqRel);
            a::driver_work_result_wait
        }
        3 => a::driver_work_result_continue,
        _ => -1,
    }
}

fn submit(kind: usize, flags: u32) -> bool {
    let mut handle = 0;
    if unsafe { work::r4os_rust_driver_work_submit(0, callback, kind, flags, false, &mut handle) }
        != 0
        || handle == 0
    {
        return false;
    }
    HANDLES[kind].store(handle, Ordering::Release);
    if kernel_check::live() {
        kernel_check::update(|r| {
            r.work_submitted += 1;
            r.live_work += 1;
        });
    }
    true
}

fn join(kind: usize, cancelled: bool, expected: i32) -> bool {
    let handle = HANDLES[kind].load(Ordering::Acquire);
    let mut result = i32::MIN;
    let waited = unsafe { work::r4os_rust_driver_work_wait(handle, 2000, &mut result) };
    if waited
        != (if cancelled {
            a::driver_work_result_cancelled
        } else {
            0
        })
        || result != expected
    {
        return false;
    }
    if work::r4os_rust_driver_work_release(handle) != 0 {
        return false;
    }
    HANDLES[kind].store(0, Ordering::Release);
    if kernel_check::live() {
        kernel_check::update(|r| {
            r.work_released += 1;
            r.live_work -= 1;
        });
    }
    let mut stale = a::DriverCompletionStatus::default();
    (unsafe { work::r4os_rust_driver_work_status(handle, &mut stale) }) == -1
}

pub(super) fn run() -> bool {
    if retained() {
        return fail(b"continuation-retained");
    }
    TURNS.store(0, Ordering::Release);
    CANCEL_TURNS.store(0, Ordering::Release);
    OBSERVED.store(false, Ordering::Release);
    BAD.store(false, Ordering::Release);
    let submitted = {
        // Enqueue the real FIFO observer before making the first turn runnable.
        // Both slots/completions are preallocated; no callback, pager or wait
        // runs while this tiny resident metadata enrollment holds IF.
        let _guard = crate::arch::runtime::RuntimeGuard::acquire();
        submit(0, a::driver_work_flag_continuation) && submit(1, 0)
    };
    if !submitted
        || !join(0, false, 0)
        || !join(1, false, 0)
        || TURNS.load(Ordering::Acquire) != 3
        || !OBSERVED.load(Ordering::Acquire)
    {
        return fail(b"continuation-turns-fifo-join");
    }
    if !submit(2, a::driver_work_flag_continuation) {
        return fail(b"continuation-cancel-submit");
    }
    let end = now().saturating_add(1000);
    while CANCEL_TURNS.load(Ordering::Acquire) == 0 && now() < end {
        sleep(1, b"continuation-first-turn");
    }
    let handle = HANDLES[2].load(Ordering::Acquire);
    let turns = CANCEL_TURNS.load(Ordering::Acquire);
    if turns == 0
        || work::r4os_rust_driver_work_cancel(handle) != 0
        || !join(2, true, a::driver_work_result_cancelled)
    {
        return fail(b"continuation-cancel-join");
    }
    sleep(2, b"continuation-cancel-retired");
    if CANCEL_TURNS.load(Ordering::Acquire) != turns
        || !submit(3, 0)
        || !join(3, false, a::driver_work_result_continue)
    {
        return fail(b"continuation-stale-or-legacy");
    }
    let mut rejected = u32::MAX;
    if unsafe {
        work::r4os_rust_driver_work_submit(
            0,
            callback,
            0,
            a::driver_work_flag_continuation | a::driver_work_flag_from_irq,
            false,
            &mut rejected,
        )
    } != -3
        || rejected != 0
        || BAD.load(Ordering::Acquire)
    {
        return fail(b"continuation-owner-or-clock");
    }
    puts(b"WORK08755 PASS turns=3 fifo=1 original-clock=1 pending-release-denied=3 tick-wait=1 cancel=1 stale=4 legacy=1 ymm=3 tickets=4 released=4\r\n");
    true
}
