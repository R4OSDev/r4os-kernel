//! Completion read/consume/orphan ownership and bounded exit history.
//! Registry admission protects exact nodes; output and heap release occur
//! only after unlinking and unlocking, through the existing payload owner.
use super::{registry, registry_state::state, registry_types::*};
use crate::contract::{ProgramProcessCompletion as Completion, ProgramProcessHandle as Handle};
use core::ptr;

unsafe fn record(node: *const ProgramCompletionNode) -> Completion {
    unsafe {
        Completion {
            handle: (*node).handle,
            sequence: (*node).sequence,
            start_tick: (*node).start_tick,
            finish_tick: (*node).finish_tick,
            exit_code: (*node).exit_code,
            task_id: (*node).task_id,
            output_revision: (*node).output_revision,
            output_length: (*node).output_length,
            flags: (*node).flags,
            app_class: (*node).app_class,
            role: (*node).role,
            exit_reason: (*node).exit_reason,
            reserved0: 0,
            console_state: (*node).console_state,
        }
    }
}
pub(super) unsafe fn release(node: *mut ProgramCompletionNode) {
    unsafe {
        super::instance_storage::free_output(node);
        registry::free_completion(node);
    }
}
unsafe fn ready(handle: Handle) -> bool {
    if !registry::lock() {
        return false;
    }
    let node = unsafe { registry::completion_handle(handle) };
    let result = !node.is_null() && unsafe { (*node).state == 2 };
    registry::unlock();
    result
}
unsafe fn read(handle: Handle, out: *mut Completion) -> i32 {
    if !registry::lock() {
        return -5;
    }
    let result = unsafe {
        let node = registry::completion_handle(handle);
        if node.is_null() {
            registry::missing(handle)
        } else if (*node).state != 2 {
            -5
        } else {
            out.write(record(node));
            0
        }
    };
    registry::unlock();
    result
}
unsafe fn consume(handle: Handle, out: *mut Completion) -> i32 {
    if !registry::lock() {
        return -5;
    }
    unsafe {
        let node = registry::completion_handle(handle);
        if node.is_null() {
            let result = registry::missing(handle);
            registry::unlock();
            return result;
        }
        if (*node).state != 2 {
            registry::unlock();
            return -5;
        }
        if !out.is_null() {
            out.write(record(node));
        }
        registry::unlink_completion(node);
        (*node).state = 3;
        if (*(*state()).stats).completion_ready != 0 {
            (*(*state()).stats).completion_ready -= 1;
        }
        registry::unlock();
        release(node);
    }
    0
}
pub(super) unsafe fn orphan(handle: Handle) {
    loop {
        if !registry::lock() {
            return;
        }
        let mut reap = ptr::null_mut();
        let mut changed = false;
        unsafe {
            let mut node = (*state()).completion_head;
            while !node.is_null() {
                if !(*node).owner || !registry::equal((*node).owner_handle, handle) {
                    node = (*node).next;
                    continue;
                }
                (*node).owner = false;
                (*node).owner_handle = Handle::default();
                (*node).flags &= !8;
                changed = true;
                if (*node).state == 2 {
                    registry::unlink_completion(node);
                    (*node).state = 3;
                    if (*(*state()).stats).completion_ready != 0 {
                        (*(*state()).stats).completion_ready -= 1;
                    }
                    reap = node;
                }
                break;
            }
        }
        registry::unlock();
        if !reap.is_null() {
            unsafe { release(reap) };
        }
        if !changed {
            return;
        }
    }
}
unsafe fn abandon(handle: Handle) {
    if !registry::lock() {
        return;
    }
    let mut consumed = ptr::null_mut();
    unsafe {
        let node = registry::completion_handle(handle);
        if !node.is_null() {
            (*node).owner = false;
            (*node).owner_handle = Handle::default();
            (*node).flags &= !8;
            if (*node).state == 2 {
                registry::unlink_completion(node);
                (*node).state = 3;
                if (*(*state()).stats).completion_ready != 0 {
                    (*(*state()).stats).completion_ready -= 1;
                }
                consumed = node;
            }
        }
    }
    registry::unlock();
    if !consumed.is_null() {
        unsafe { release(consumed) };
    }
}

/// # Safety
/// Caller holds the registry mutex and the exact instance lifetime. History
/// stays in the single native ring; the original sequence saturates at MAX.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_history_append(
    handle: Handle,
    instance: *const ProgramInstance,
    exit_code: i32,
    reason: u8,
    start_tick: u64,
    finish_tick: u64,
) -> u64 {
    unsafe {
        let s = state();
        let sequence = (*s).next_exit_sequence;
        (*s).last_exit_sequence = sequence;
        (*s).next_exit_sequence = (*s).next_exit_sequence.saturating_add(1);
        (*(*s).history)[(*s).history_head] = ExitRecord {
            used: true,
            handle,
            sequence,
            start_tick,
            finish_tick,
            app_class: (*instance).app_class,
            role: (*instance).role,
            exit_reason: reason,
            exit_code,
        };
        (*s).history_head = ((*s).history_head + 1) % 16;
        if (*s).history_count < 16 {
            (*s).history_count += 1;
        }
        sequence
    }
}
/// # Safety
/// Exact resident borrowed node under registry admission; output is an
/// exclusive aligned public completion record, disjoint from live nodes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_completion_copy(
    node: *const ProgramCompletionNode,
    out: *mut Completion,
) {
    unsafe {
        out.write(record(node));
    }
}
/// # Safety
/// Op1 needs exclusive aligned output; op2 accepts null or such an output.
/// Caller supplies only a value handle and holds no registry mutex. Detached
/// output cleanup retains its original payload/console lifetime contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_completion_access(
    op: u8,
    handle: Handle,
    out: *mut Completion,
) -> i32 {
    unsafe {
        match op {
            0 => i32::from(ready(handle)),
            1 => read(handle, out),
            2 => consume(handle, out),
            3 => {
                orphan(handle);
                0
            }
            4 => {
                abandon(handle);
                0
            }
            _ => unreachable!(),
        }
    }
}

const _: () = {
    assert!(core::mem::size_of::<Completion>() == 128);
    assert!(core::mem::offset_of!(Completion, console_state) == 64);
};
