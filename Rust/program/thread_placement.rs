//! Exact-generation public thread placement. Caller memory is accessed outside
//! both registries; a short Thread pin spans only the typed scheduler operation.
use super::threads;
use crate::{
    contract as c,
    memory::owner::Guard,
    sched::{task::thread_placement as policy, task_context},
};
use core::ptr;

struct Pin {
    thread: *mut threads::Thread,
    identity: policy::Identity,
}
impl Pin {
    unsafe fn acquire(handle: c::ProgramJoinHandle) -> Result<Self, i32> {
        if handle.thread_id == 0
            || handle.instance_id == 0
            || handle.thread_generation == 0
            || handle.instance_generation == 0
            || handle.reserved != 0
        {
            return Err(c::thread_error_invalid);
        }
        if unsafe { threads::current() }.is_null() {
            return Err(c::thread_error_no_instance);
        }
        let thread = unsafe { threads::pin(handle.thread_id, Some(handle), false) };
        if thread.is_null() {
            return Err(c::thread_error_not_found);
        }
        let identity = {
            let _guard = Guard::acquire(0);
            unsafe {
                let instance = (*thread).owner_instance;
                if !threads::contains(thread)
                    || !(*thread).used
                    || !matches!((*thread).state, 1 | 2)
                    || (*thread).task_detached
                    || instance.is_null()
                    || !(*instance).used
                    || (*instance).done
                    || (*instance).close_requested
                {
                    None
                } else {
                    Some(policy::Identity {
                        id: (*thread).task_id,
                        generation: (*thread).task_generation,
                        thread: thread.cast(),
                    })
                }
            }
        };
        match identity {
            Some(identity) => Ok(Self { thread, identity }),
            None => {
                assert!(unsafe { threads::unpin(thread) });
                Err(c::thread_error_not_found)
            }
        }
    }
}
impl Drop for Pin {
    fn drop(&mut self) {
        assert!(
            unsafe { threads::unpin(self.thread) },
            "placement exact Thread pin"
        );
    }
}
fn admit_caller() -> Option<task_context::Admission> {
    // Copies can fault and the Thread registry is a lower-ranked owner. Do
    // not enter it from an existing CPU-bound owner or an IF-off caller.
    let flags: u64;
    unsafe {
        core::arch::asm!("pushfq; pop {}", out(reg) flags, options(preserves_flags));
    }
    if flags & (1 << 9) == 0 || !crate::memory::owner::fault_allowed() {
        return None;
    }
    task_context::Admission::acquire(true)
}

fn overlap(a: usize, a_len: usize, b: usize, b_len: usize) -> bool {
    match (a.checked_add(a_len), b.checked_add(b_len)) {
        (Some(a_end), Some(b_end)) => a < b_end && b < a_end,
        _ => true,
    }
}

/// # Safety
/// Live caller-owned ABI buffers and optional CPU vector remain valid for the
/// call. Trusted caller memory can fault, so no registry owner spans a copy.
pub(crate) unsafe extern "C" fn get(
    handle: *const c::ProgramJoinHandle,
    info: *mut c::ThreadPlacementInfo,
    cpus: *mut u64,
    capacity: u32,
) -> i32 {
    let Some(_caller) = admit_caller() else {
        return c::thread_error_busy;
    };
    if handle.is_null() || info.is_null() {
        return c::thread_error_invalid;
    }
    let version = unsafe { ptr::addr_of!((*info).version).read_unaligned() };
    let size = unsafe { ptr::addr_of!((*info).size).read_unaligned() };
    if version != c::thread_placement_version || size < 64 {
        return c::thread_error_invalid;
    }
    let words = policy::word_count();
    if capacity != 0 && capacity < words {
        return c::thread_placement_error_capacity;
    }
    if capacity != 0
        && (cpus.is_null() || overlap(info.addr(), 64, cpus.addr(), words as usize * 8))
    {
        return c::thread_error_invalid;
    }
    let pinned = match unsafe { Pin::acquire(handle.read_unaligned()) } {
        Ok(pin) => pin,
        Err(error) => return error,
    };
    let snapshot = policy::get(pinned.identity);
    drop(pinned);
    match snapshot {
        Ok((snapshot, bits)) => {
            if capacity != 0 {
                unsafe { cpus.write_unaligned(bits) };
            }
            unsafe { info.write_unaligned(snapshot) };
            c::thread_ok
        }
        Err(error) => error,
    }
}

/// # Safety
/// Borrowed input is copied before registry admission. No allocation, wait or
/// caller pointer is retained by an accepted pending placement request.
pub(crate) unsafe extern "C" fn set(
    handle: *const c::ProgramJoinHandle,
    request: *const c::ThreadPlacementRequest,
    cpus: *const u64,
    count: u32,
) -> i32 {
    let Some(_caller) = admit_caller() else {
        return c::thread_error_busy;
    };
    if handle.is_null() || request.is_null() {
        return c::thread_error_invalid;
    }
    let request = unsafe { request.read_unaligned() };
    if request.version != c::thread_placement_version
        || request.size != 32
        || request.flags & !c::thread_placement_flag_default_cpus != 0
        || request.work_class > c::thread_placement_class_preferred
        || request.reserved0 != 0
        || request.reserved1 != 0
    {
        return c::thread_error_invalid;
    }
    let default_cpus = request.flags & c::thread_placement_flag_default_cpus != 0;
    if (default_cpus && count != 0) || (!default_cpus && count != policy::word_count()) {
        return c::thread_placement_error_capacity;
    }
    if !default_cpus && cpus.is_null() {
        return c::thread_error_invalid;
    }
    // Private scheduler currently fits in one word; the public vector contract
    // is extensible and its length is discovered rather than fixed in callers.
    let bits = if default_cpus {
        0
    } else {
        unsafe { cpus.read_unaligned() }
    };
    if !default_cpus && bits == 0 {
        return c::thread_error_invalid;
    }
    let pinned = match unsafe { Pin::acquire(handle.read_unaligned()) } {
        Ok(pin) => pin,
        Err(error) => return error,
    };
    let result = policy::set(
        pinned.identity,
        policy::Policy {
            cpus: bits,
            preferred: (request.preferred_cpu != c::thread_placement_cpu_none)
                .then_some(request.preferred_cpu),
            class: request.work_class,
        },
    );
    drop(pinned);
    // Acceptance is asynchronous for physical currents. The reschedule IPI,
    // timer or an explicit caller yield reaches the existing saved boundary.
    result
}
