//! Resident reaper doorbell and retry rounds. Notifications are progress hints;
//! only existing generation/phase/native resource owners may acknowledge free.
use super::{async_io, registry, registry_state, thread_retire};
use crate::sched::sync_objects;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

static PROGRESS: AtomicBool = AtomicBool::new(false);
static ROUND: AtomicU64 = AtomicU64::new(1);

pub(crate) fn round() -> u64 {
    ROUND.load(Ordering::Acquire)
}
/// New queue admission always rings, including while an external reap attempt
/// temporarily owns the last dequeued record and the main reaper is idle.
/// A queue handback alone never invalidates an unchanged dependency wait.
pub(crate) fn queued() {
    unsafe {
        sync_objects::r4os_rust_sync_event((*registry_state::state()).event, 0, 0);
    }
}
/// Only call after an enrolled ownership change or explicit diagnostic resume,
/// at an existing Runtime-safe edge. Never enter Runtime under the VM owner.
pub(crate) fn changed() {
    PROGRESS.store(true, Ordering::Release);
    queued();
}
/// Main reaper only, after consuming an actual Signaled Event. A retry timeout
/// must not invalidate unrelated progress waits. Invalidate cached rounds under
/// both owners before recycling the round, including unlocked in-flight work.
pub(super) fn received() -> bool {
    if !PROGRESS.swap(false, Ordering::AcqRel) {
        return true;
    }
    let current = round();
    let next = if let Some(next) = current.checked_add(1) {
        next
    } else {
        if !registry::reset_retire_rounds()
            || !async_io::reset_retire_rounds()
            || !thread_retire::reset_retire_rounds()
        {
            // The consumed credit still needs its round. Lock admission
            // failure may not turn the next retry into a credit-free success.
            PROGRESS.store(true, Ordering::Release);
            return false;
        }
        1
    };
    ROUND.store(next, Ordering::Release);
    true
}
