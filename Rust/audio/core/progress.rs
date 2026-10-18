//! Exact progress operations of the parent owner; no duplicated state.
use super::*;

pub(crate) unsafe fn start_progress_worker() -> bool {
    unsafe {
        if g!(progress_worker_started) {
            return true;
        }
        let name = b"audio-progress";
        let request = task_api::CreateRequest {
            name: name.as_ptr(),
            name_len: name.len(),
            state: 1,
            role: 1,
            needs_fpu: true,
            smp_eligible: false,
            smp_r4x: false,
            owner_kind: 0,
            owner_context: ptr::null_mut(),
            entry: Some(progress_worker),
        };
        let mut failure = 0;
        if task_api::r4os_rust_task_create(&request, false, &mut failure).is_null() {
            return false;
        }
        g!(progress_worker_started) = true;
        true
    }
}

// Retain original checked multiply/add overflow and deadline rounding behavior.
#[allow(clippy::manual_div_ceil)]
pub(super) fn schedule_progress(pending: &AtomicU64, milliseconds: u64) {
    let ticks = 1u64.max((milliseconds * timer::frequency() as u64 + 999) / 1000);
    let due = timer::r4os_rust_timer_control(7, ticks, 0);
    let mut previous = pending.load(Ordering::Acquire);
    while due < previous {
        match pending.compare_exchange_weak(previous, due, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => {
                unsafe {
                    sync_objects::r4os_rust_sync_event(PROGRESS.0.get(), 0, 0);
                }
                return;
            }
            Err(actual) => previous = actual,
        }
    }
}

pub(super) unsafe extern "C" fn progress_worker() {
    loop {
        let now = timer::tick_count();
        if unsafe { run_progress_if_due(now) } {
            continue;
        }
        let due = next_progress_tick();
        unsafe {
            sync_objects::r4os_rust_sync_event(
                PROGRESS.0.get(),
                2,
                if due == u64::MAX {
                    u64::MAX
                } else {
                    due.saturating_sub(now)
                },
            );
        }
    }
}

pub(super) fn next_progress_tick() -> u64 {
    MIX_DUE
        .load(Ordering::Acquire)
        .min(BACKEND_DUE.load(Ordering::Acquire))
}

pub(super) fn claim_progress(pending: &AtomicU64, now: u64) -> bool {
    let due = pending.load(Ordering::Acquire);
    due != u64::MAX
        && now >= due
        && pending
            .compare_exchange(due, u64::MAX, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
}

pub(super) unsafe fn run_progress_if_due(now: u64) -> bool {
    unsafe {
        if now < next_progress_tick() {
            return false;
        }
        if claim_progress(&MIX_DUE, now) {
            relock();
            let result = pump_available_locked(true);
            unlock();
            if result == BUSY {
                schedule_progress(&MIX_DUE, 2);
            }
        }
        if claim_progress(&BACKEND_DUE, now) {
            service_backend_progress();
        }
        true
    }
}

pub(super) unsafe fn service_backend_progress() {
    unsafe {
        let Some(_output) = OutputGuard::acquire(false) else {
            schedule_progress(&BACKEND_DUE, 2);
            return;
        };
        let Some(slot) = g!(active_audio_slot) else {
            return;
        };
        let p = backend(slot);
        if (*p).closing {
            return;
        }
        let config = (*p).config;
        if let Some(status) = config.status {
            let mut value = BackendStatus::default();
            if status(config.context, &mut value) == BUSY {
                schedule_progress(&BACKEND_DUE, 2);
            }
        }
    }
}
