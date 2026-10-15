//! Exact phase admission and deferred retirement queue ownership.
use super::{failure, registry, registry_state::state, registry_types::*};
use crate::contract::ProgramProcessHandle as Handle;

/// # Safety
/// Value handle only, no caller-held registry mutex. Failure progress uses its
/// rank100 owner strictly after releasing the rank110 registry mutex.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_retire_defer(handle: Handle) -> u8 {
    if !registry::lock() {
        return 2;
    }
    unsafe {
        let slot = registry::lookup_handle(handle, true);
        if !slot.is_null() && (*slot).state == 6 {
            (*slot).retire_in_progress = false;
            (*slot).retire_attempts = (*slot).retire_attempts.wrapping_add(1);
            let stats = (*state()).stats;
            (*stats).retire_deferred = (*stats).retire_deferred.wrapping_add(1);
            (*stats).retire_retries = (*stats).retire_retries.wrapping_add(1);
            registry::enqueue(slot);
        }
    }
    registry::unlock();
    failure::note(false);
    2
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
