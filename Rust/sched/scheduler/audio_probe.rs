//! Explicit CHECK THREAD live case. Production queues, actual CPU selection,
//! bounded callbacks and exact terminal ticket/Task retirement; no hot-path hook.
use super::*;
use crate::{
    arch::runtime::RuntimeGuard,
    contract as a,
    driver::{work, work_queue},
    sched::{
        kernel_check::{self, tasks},
        task::task_types::Role,
    },
};
use core::sync::atomic::{AtomicBool, AtomicU32};
static HANDLES: [AtomicU32; 3] = [const { AtomicU32::new(0) }; 3];
static TRACE: AtomicU32 = AtomicU32::new(0);
static INPUT: AtomicU32 = AtomicU32::new(0);
static AUDIO: AtomicU32 = AtomicU32::new(0);
static NORMAL: AtomicU32 = AtomicU32::new(0);
static TAIL: AtomicU32 = AtomicU32::new(0);
static CANCELLED_CALLED: AtomicBool = AtomicBool::new(false);
static AUDIO_DONE: AtomicBool = AtomicBool::new(false);
static NORMAL_DURING: AtomicBool = AtomicBool::new(false);
pub(crate) fn retained_work() -> bool {
    HANDLES.iter().any(|h| h.load(Ordering::Acquire) != 0)
}
fn sequence() -> u32 {
    TRACE.fetch_add(1, Ordering::AcqRel) + 1
}
unsafe extern "C" fn input() {
    INPUT.store(sequence(), Ordering::Release);
}
unsafe extern "C" fn normal() {
    NORMAL.store(sequence(), Ordering::Release);
    NORMAL_DURING.store(
        AUDIO.load(Ordering::Acquire) != 0 && !AUDIO_DONE.load(Ordering::Acquire),
        Ordering::Release,
    );
}
unsafe extern "C" fn callback(context: usize) -> i32 {
    match context {
        0 => {
            CANCELLED_CALLED.store(true, Ordering::Release);
        }
        1 => {
            AUDIO.store(sequence(), Ordering::Release);
            let end = tick().saturating_add(6);
            while tick() < end {
                safe_reschedule();
                core::hint::spin_loop();
            }
            AUDIO_DONE.store(true, Ordering::Release);
        }
        2 => {
            TAIL.store(sequence(), Ordering::Release);
        }
        _ => return -1,
    }
    0
}
fn submit(index: usize, deadline: u64) -> bool {
    let request = a::DriverWorkRequest {
        version: a::driver_work_request_version,
        size: core::mem::size_of::<a::DriverWorkRequest>() as u32,
        work_class: a::driver_work_class_audio_refill,
        flags: 0,
        handler: callback,
        context: index,
        serial_key: 0x8751 + index as u64,
        deadline_tick: deadline,
        budget_ticks: 2,
    };
    let mut handle = 0;
    let result = unsafe { work::r4os_rust_driver_work_request(0, &request, &mut handle) };
    if result != 0 {
        return false;
    }
    HANDLES[index].store(handle, Ordering::Release);
    kernel_check::update(|r| {
        r.work_submitted += 1;
        r.live_work += 1;
    });
    true
}
fn retire(index: usize, expected: i32) -> bool {
    let handle = HANDLES[index].load(Ordering::Acquire);
    if handle == 0 {
        return true;
    }
    let mut result = -1;
    let status = unsafe { work::r4os_rust_driver_work_wait(handle, 1000, &mut result) };
    // Wait returns -7 AND copies -7 for a cancelled, terminal ticket.
    // Preserve that receipt and proceed to its actual Release.
    let completed = status == if expected == -7 { -7 } else { 0 };
    if !matches!(status, 0 | -7) {
        work::r4os_rust_driver_work_cancel(handle);
        if !matches!(
            unsafe { work::r4os_rust_driver_work_wait(handle, 1000, &mut result) },
            0 | -7
        ) {
            return false;
        }
    }
    if work::r4os_rust_driver_work_release(handle) != 0 {
        return false;
    }
    HANDLES[index].store(0, Ordering::Release);
    kernel_check::update(|r| {
        r.work_released += 1;
        r.live_work -= 1;
    });
    completed && result == expected
}
pub(crate) fn run() -> bool {
    if retained_work() {
        return false;
    }
    for value in [&TRACE, &INPUT, &AUDIO, &NORMAL, &TAIL] {
        value.store(0, Ordering::Release);
    }
    CANCELLED_CALLED.store(false, Ordering::Release);
    AUDIO_DONE.store(false, Ordering::Release);
    NORMAL_DURING.store(false, Ordering::Release);
    let input = tasks::create(b"spectra-audio-input", input, Role::Input);
    let normal = tasks::create(b"spectra-audio-completion", normal, Role::ShortCompletion);
    let passed = (|| {
        if input.is_null() || normal.is_null() {
            return false;
        }
        let (old, worker) = {
            let _runtime = RuntimeGuard::acquire();
            if !submit(0, tick().saturating_add(1000)) {
                return false;
            }
            audio_candidate(tick());
            let hint = read(|s| s.audio.hint);
            let old = HANDLES[0].load(Ordering::Acquire);
            if hint.ticket != old
                || hint.serial_key != 0x8751
                || hint.worker.generation == 0
                || !unsafe {
                    super::super::task::audio_worker(hint.worker.id, hint.worker.generation + 1)
                }
                .is_null()
                || work::r4os_rust_driver_work_cancel(old) != 0
                || !retire(0, -7)
            {
                return false;
            }
            if audio_candidate(tick()).eligible || read(|s| s.audio.hint.ticket) != 0 {
                return false;
            }
            (old, hint.worker)
        };
        // Consume the cancelled event, so its earlier wake cannot explain
        // the later Audio-before-completion CPU order.
        let end = tick().saturating_add(1000);
        loop {
            let parked = {
                let _runtime = RuntimeGuard::acquire();
                unsafe { super::super::task::audio_worker(worker.id, worker.generation) }.is_null()
            };
            if parked {
                break;
            }
            if tick() >= end {
                return false;
            }
            tasks::sleep(1, b"spectra-audio-cancel-drain");
        }
        {
            let _runtime = RuntimeGuard::acquire();
            if !tasks::start(normal)
                || !tasks::start(input)
                || !submit(2, tick().saturating_add(1000))
                || !submit(1, tick().saturating_sub(1))
            {
                return false;
            }
            let replacement = HANDLES[2].load(Ordering::Acquire);
            let old_slot = work_queue::r4os_rust_work_handle(1, old as usize, 0);
            let new_slot = work_queue::r4os_rust_work_handle(1, replacement as usize, 0);
            let candidate = audio_candidate(tick());
            if old == replacement
                || old_slot != new_slot
                || !candidate.eligible
                || read(|s| s.audio.hint.ticket) != HANDLES[1].load(Ordering::Acquire)
                || task_query(5, 1 << 32) != normal.cast()
            {
                return false;
            }
        }
        // Real blocking and full-save switching choose production r4d-audio.
        let audio_clean = retire(1, 0);
        let tail_clean = retire(2, 0);
        audio_clean
            && tail_clean
            && INPUT.load(Ordering::Acquire) == 1
            && AUDIO.load(Ordering::Acquire) == 2
            && NORMAL.load(Ordering::Acquire) == 3
            && TAIL.load(Ordering::Acquire) == 4
            && NORMAL_DURING.load(Ordering::Acquire)
            && !CANCELLED_CALLED.load(Ordering::Acquire)
    })();
    let mut clean = true;
    for index in 0..3 {
        clean &= retire(index, if index == 0 { -7 } else { 0 });
    }
    for task in [input, normal] {
        if !task.is_null() {
            clean &= tasks::finish_task(task, !passed);
        }
    }
    crate::boot::log_route::puts(if passed && clean {
        b"AUDIO08751 PASS input=1 audio=2 completion=3 tail=4 normal-during-audio=1 cancelled=1 reused-generation=1 tasks=2 work=3 own=reaped\n"
    } else {
        b"AUDIO08751 FAIL order-or-lifetime\n"
    });
    passed && clean
}
