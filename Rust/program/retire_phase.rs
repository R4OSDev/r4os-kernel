//! Exact phase admission and deferred retirement queue ownership.
use super::{failure, registry, registry_state::state, registry_types::*, retire_wait::Wait};
use crate::contract::ProgramProcessHandle as Handle;

/// # Safety
/// Value handle only, no caller-held registry mutex. Failure progress uses its
/// rank100 owner strictly after releasing the rank110 registry mutex.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_retire_defer(handle: Handle) -> u8 {
    unsafe { defer(handle, Wait::Retry, true) }
}
pub(super) unsafe fn defer(handle: Handle, wait: Wait, notify: bool) -> u8 {
    if !registry::lock() {
        return 2;
    }
    unsafe {
        let slot = registry::lookup_handle(handle, true);
        if !slot.is_null() && (*slot).state == 6 {
            (*slot).retire_in_progress = false;
            (*slot).retire_attempts = (*slot).retire_attempts.wrapping_add(1);
            (*slot)
                .retire_retry
                .defer(crate::platform::timer::tick_count(), wait);
            let stats = (*state()).stats;
            (*stats).retire_deferred = (*stats).retire_deferred.wrapping_add(1);
            (*stats).retire_retries = (*stats).retire_retries.wrapping_add(1);
            registry::enqueue(slot);
        }
    }
    registry::unlock();
    if notify {
        super::retire_progress::queued();
    }
    failure::note(false);
    2
}
pub(super) unsafe fn pin_wait(handle: Handle) -> Wait {
    if !registry::lock() {
        return Wait::Retry;
    }
    let waiting = unsafe {
        let slot = registry::lookup_handle(handle, true);
        !slot.is_null() && (*slot).state == 6 && (*slot).pin_count != 0
    };
    registry::unlock();
    if waiting { Wait::Progress } else { Wait::Retry }
}
/// # Safety
/// Slot/completion retain the exact original worker lifetime. This function
/// reacquires registry admission and validates pointer, identity, phase, worker
/// ownership and pins before advancing. Only cancel->detach tolerates pins.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_retire_advance(
    slot: *mut ProgramRegistrySlot,
    handle: Handle,
    completion: *mut ProgramCompletionNode,
    expected: u8,
    next: u8,
) -> bool {
    if !registry::lock() {
        return false;
    }
    let success = unsafe {
        let current = registry::lookup_handle(handle, true);
        !current.is_null()
            && current == slot
            && (*current).state == 6
            && (*current).retire_in_progress
            && (expected == 0 || (*current).pin_count == 0)
            && (*current).completion == completion
            && (*current).retire_phase == expected
    };
    if success {
        unsafe {
            (*slot).retire_phase = next;
        }
    }
    registry::unlock();
    success
}
