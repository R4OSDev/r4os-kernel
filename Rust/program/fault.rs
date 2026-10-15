//! R4X fault routing and resident continuation placement. Interrupted frames
//! and SIMD state remain on the exact task stack before any waiting pager I/O.
//! The remaining pager and async-I/O owners retain their own admitted work.
use super::{
    registry_types::ProgramInstance,
    stack,
    threads::{self, Thread},
    vm,
};
use crate::{
    memory::{owner, virt_policy},
    sched::{scheduler, task::task_registry},
};
use core::{mem::MaybeUninit, ptr};
const MAIN: u32 = 1;
const GUARD: u64 = 4096;
const HEADROOM: u64 = 64 * 1024;
unsafe extern "C" {
    fn r4os_zig_program_fault_demand(address: u64, error: u64, owner: u32) -> bool;
}
unsafe fn grow_thread(thread: *mut Thread, address: u64) -> bool {
    unsafe {
        let before = (*thread).stack.committed_size;
        if !stack::grow(ptr::addr_of_mut!((*thread).stack), address) {
            return false;
        }
        if !(*thread).stack.serial_telemetry {
            return true;
        }
        stack::blog(b"[R4X] thread stack grow owner=");
        stack::bdec(u64::from((*thread).instance_id));
        stack::blog(b" thread=");
        stack::bdec(u64::from((*thread).id));
        stack::blog(b" committed=");
        stack::bdec(before);
        stack::blog(b" -> ");
        stack::bdec((*thread).stack.committed_size);
        stack::blog(b"\r\n");
        true
    }
}
unsafe fn prepare(address: u64, rsp: u64, bytes: u64) -> Option<u64> {
    unsafe {
        let mut window = MaybeUninit::<virt_policy::Window>::uninit();
        virt_policy::r4os_rust_vm_window(6, window.as_mut_ptr());
        let window = window.assume_init();
        if address < window.base || address - window.base >= window.len {
            return None;
        }
        if !owner::fault_allowed() {
            return None;
        }
        let current = scheduler::current();
        if current.is_null() || rsp < bytes {
            return None;
        }
        let frame = (rsp - bytes) & !15;
        let thread = threads::current();
        if !thread.is_null() {
            if (*thread).task_id != (*task_registry::core(current.cast())).id {
                return None;
            }
            if (*thread).flags & MAIN != 0 {
                let instance = (*thread).owner_instance;
                if instance.is_null() {
                    return None;
                }
                if stack::contains_instance(instance, rsp - 1) {
                    let floor = frame.saturating_sub(HEADROOM);
                    if floor < (*instance).program_stack_base + GUARD {
                        return None;
                    }
                    if floor < (*instance).program_stack_committed_base
                        && !stack::grow_instance(instance, floor)
                    {
                        return None;
                    }
                    return Some(frame);
                }
            } else if stack::contains(ptr::addr_of!((*thread).stack), rsp - 1) {
                let floor = frame.saturating_sub(HEADROOM);
                if floor < (*thread).stack.base + (*thread).stack.guard_size {
                    return None;
                }
                if floor < (*thread).stack.committed_base && !grow_thread(thread, floor) {
                    return None;
                }
                return Some(frame);
            }
        }
        let core = task_registry::core(current.cast());
        if rsp > (*core).stack_top
            || frame < (*core).stack_base
            || frame - (*core).stack_base < 32 * 1024
        {
            return None;
        }
        Some(frame)
    }
}
/// # Safety
/// Invoked on exception IST with interrupts disabled. Exact interrupted task
/// and resident output survive the call; no pageable operation is admitted.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_fault_prepare(
    address: u64,
    rsp: u64,
    bytes: u64,
    out: *mut u64,
) -> bool {
    let Some(frame) = (unsafe { prepare(address, rsp, bytes) }) else {
        return false;
    };
    unsafe { out.write(frame) };
    true
}
/// # Safety
/// Original fault/exception admission; waiting pageable faults already retain
/// their full frame/SIMD state on this task's own resident guarded stack.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_fault_handle(
    address: u64,
    error: u64,
    pageable: bool,
) -> bool {
    unsafe {
        if error & 1 != 0 || !owner::fault_allowed() {
            return false;
        }
        let task = scheduler::current();
        let current_id = if task.is_null() {
            None
        } else {
            Some((*task_registry::core(task.cast())).id)
        };
        if pageable {
            let instance = super::context::execution();
            if !instance.is_null()
                && vm::profile_limit(2, instance, 4096)
                && r4os_zig_program_fault_demand(address, error, (*instance).id)
            {
                return true;
            }
        }
        let thread = threads::current();
        if !thread.is_null() {
            if (*thread).flags & MAIN != 0 {
                let instance = (*thread).owner_instance;
                if !instance.is_null() && stack::contains_instance(instance, address) {
                    return stack::grow_instance(instance, address);
                }
            } else if stack::contains(ptr::addr_of!((*thread).stack), address) {
                return grow_thread(thread, address);
            }
        }
        let mut stack_owner: *mut ProgramInstance = ptr::null_mut();
        let mut thread_owner: *mut Thread = ptr::null_mut();
        let mut cursor = (*threads::state()).head;
        while !cursor.is_null() {
            let thread = cursor;
            cursor = (*thread).registry_next;
            if !(*thread).used {
                continue;
            }
            if (*thread).flags & MAIN != 0 {
                let instance = (*thread).owner_instance;
                if instance.is_null() || !stack::contains_instance(instance, address) {
                    continue;
                }
                if current_id.is_some_and(|id| (*thread).task_id == id) {
                    return stack::grow_instance(instance, address);
                }
                if stack_owner.is_null() {
                    stack_owner = instance;
                }
                continue;
            }
            if !stack::contains(ptr::addr_of!((*thread).stack), address) {
                continue;
            }
            if current_id.is_some_and(|id| (*thread).task_id == id) {
                return grow_thread(thread, address);
            }
            if thread_owner.is_null() {
                thread_owner = thread;
            }
        }
        if !stack_owner.is_null() {
            return stack::grow_instance(stack_owner, address);
        }
        if !thread_owner.is_null() {
            return grow_thread(thread_owner, address);
        }
        false
    }
}
