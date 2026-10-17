//! R4X fault routing and resident continuation placement. Interrupted frames
//! and SIMD state remain on the exact task stack before any waiting pager I/O.
//! The remaining pager and async-I/O owners retain their own admitted work.
use super::{
    stack,
    stack_fault::{self, Failure},
    threads::{self, Thread},
    vm,
};
use crate::{
    contract as c,
    memory::{owner, virt_policy},
    sched::{scheduler, task::task_registry},
};
use core::{mem::MaybeUninit, ptr};
const MAIN: u32 = 1;
const GUARD: u64 = 4096;
const HEADROOM: u64 = 64 * 1024;

unsafe fn grow_thread(thread: *mut Thread, address: u64) -> Result<(), Failure> {
    unsafe {
        let before = (*thread).stack.committed_size;
        stack::grow(ptr::addr_of_mut!((*thread).stack), address)?;
        if !(*thread).stack.serial_telemetry {
            return Ok(());
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
        Ok(())
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
                        stack_fault::preparation_failed(
                            address,
                            rsp,
                            Failure::new(c::stack_fault_reason_continuation_headroom),
                        );
                        return None;
                    }
                    if floor < (*instance).program_stack_committed_base
                        && let Err(failure) = stack::grow_instance(instance, floor)
                    {
                        stack_fault::preparation_failed(address, rsp, failure);
                        return None;
                    }
                    return Some(frame);
                }
            } else if stack::contains(ptr::addr_of!((*thread).stack), rsp - 1) {
                let floor = frame.saturating_sub(HEADROOM);
                if floor < (*thread).stack.base + (*thread).stack.guard_size {
                    stack_fault::preparation_failed(
                        address,
                        rsp,
                        Failure::new(c::stack_fault_reason_continuation_headroom),
                    );
                    return None;
                }
                if floor < (*thread).stack.committed_base
                    && let Err(failure) = grow_thread(thread, floor)
                {
                    stack_fault::preparation_failed(address, rsp, failure);
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
            if rsp > (*core).stack_base && rsp <= (*core).stack_top {
                stack_fault::preparation_failed(
                    address,
                    rsp,
                    Failure::new(c::stack_fault_reason_continuation_headroom),
                );
            }
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
    rsp: u64,
) -> bool {
    unsafe {
        if error & 1 != 0 || !owner::fault_allowed() {
            return false;
        }
        if pageable {
            let instance = super::context::execution();
            if !instance.is_null()
                && vm::profile_limit(2, instance, 4096)
                && program_fault_demand(address, error, (*instance).id)
            {
                return true;
            }
        }
        let thread = threads::current();
        if !thread.is_null() {
            if (*thread).flags & MAIN != 0 {
                let instance = (*thread).owner_instance;
                if !instance.is_null() && stack::contains_instance(instance, address) {
                    return growth_result(
                        stack::grow_instance(instance, address),
                        address,
                        rsp,
                        error,
                    );
                }
            } else if stack::contains(ptr::addr_of!((*thread).stack), address) {
                return growth_result(grow_thread(thread, address), address, rsp, error);
            }
        }
        // Only the retained current execution binding admits stack growth.
        // An address in another task's reservation is not ownership proof.
        false
    }
}

unsafe fn growth_result(result: Result<(), Failure>, address: u64, rsp: u64, error: u64) -> bool {
    match result {
        Ok(()) => true,
        Err(failure) => {
            unsafe { stack_fault::growth_failed(address, rsp, error, failure) };
            false
        }
    }
}

unsafe fn program_fault_demand(address: u64, error: u64, owner: u32) -> bool {
    unsafe {
        crate::memory::vm_pager_api::r4os_rust_vm_demand_fault(address, error, 3, u64::from(owner))
    }
}
