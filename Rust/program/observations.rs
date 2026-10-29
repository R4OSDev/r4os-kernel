//! Generation-bound references to detached terminal evidence. Bindings never
//! pin a ProgramInstance, Task, image, VM, GUI frame or active output writer.
use super::{
    completions, console, context, payload_types as t, registry,
    registry_state::state,
    registry_types::{ProgramCompletionNode, ProgramObservationNode},
};
use crate::{
    contract as c,
    memory::{heap, owner},
    platform::timer,
    sched::{
        sync_types::{Text, WaitQueue, WaitRequest},
        sync_wait, task_context,
    },
};
use core::{
    cell::UnsafeCell,
    ffi::c_void,
    mem, ptr,
    sync::atomic::{AtomicU64, Ordering},
};

// This resident queue is only for observation waiters, including cancellation
// when one binding closes. Its revision is functional enrollment state, not
// telemetry. Producer wake and predicate/enrollment share scheduler runtime.
struct Queue(UnsafeCell<WaitQueue>);
unsafe impl Sync for Queue {}
static WAITERS: Queue = Queue(UnsafeCell::new(WaitQueue::EMPTY));
static REVISION: AtomicU64 = AtomicU64::new(1);
pub(super) fn signal() {
    REVISION.fetch_add(1, Ordering::Release);
    unsafe {
        sync_wait::r4os_rust_sync_queue(WAITERS.0.get(), 1, ptr::null_mut(), 0, ptr::null_mut());
    }
}
unsafe extern "C" fn unchanged(raw: *mut c_void) -> bool {
    unsafe { REVISION.load(Ordering::Acquire) == *raw.cast::<u64>() }
}
fn valid(key: c::ProgramObservationHandle) -> bool {
    key.id != 0 && registry::valid(key.process)
}
unsafe fn find(key: c::ProgramObservationHandle) -> *mut ProgramObservationNode {
    unsafe {
        let mut item = (*state()).observation_head;
        while !item.is_null() {
            if (*item).key.id == key.id && registry::equal((*item).key.process, key.process) {
                return item;
            }
            item = (*item).next;
        }
        ptr::null_mut()
    }
}
unsafe fn target(handle: c::ProgramProcessHandle) -> *mut ProgramCompletionNode {
    unsafe {
        let mut node = (*state()).completion_head;
        while !node.is_null() {
            if registry::equal((*node).handle, handle) {
                return node;
            }
            node = (*node).next;
        }
        ptr::null_mut()
    }
}

/// Admission and refcount publication are one registry transaction. Candidate
/// allocation and all caller writes occur outside that owner.
/// # Safety
/// `process` is a readable aligned handle and `out` writable aligned handle
/// storage. Pointed-to spans must remain accessible for this call.
pub unsafe extern "C" fn retain(
    process: *const c::ProgramProcessHandle,
    flags: u32,
    out: *mut c::ProgramObservationHandle,
) -> i32 {
    if process.is_null() || out.is_null() || flags & !c::program_observation_flag_portable != 0 {
        return -1;
    }
    let handle = unsafe { process.read() };
    if !registry::valid(handle) {
        return -1;
    }
    let Some(_task) = task_context::Admission::acquire(true) else {
        return -5;
    };
    let Some(creator) = (unsafe { context::handle() }) else {
        return -5;
    };
    owner::prepare_stack();
    let memory = heap::r4os_rust_heap_alloc(
        mem::size_of::<ProgramObservationNode>(),
        mem::align_of::<ProgramObservationNode>(),
    );
    if memory.is_null() {
        return -8;
    }
    if !registry::lock() {
        unsafe {
            heap::r4os_rust_heap_free(memory, mem::size_of::<ProgramObservationNode>());
        }
        return -5;
    }
    let mut key = c::ProgramObservationHandle::default();
    let result = unsafe {
        let node = target(handle);
        if node.is_null() {
            registry::missing(handle)
        } else if (*node).observation_refs == u32::MAX || (*state()).next_observation_id == u64::MAX
        {
            -8
        } else {
            key = c::ProgramObservationHandle {
                process: handle,
                id: (*state()).next_observation_id,
            };
            (*state()).next_observation_id += 1;
            let item = memory.cast::<ProgramObservationNode>();
            item.write(ProgramObservationNode {
                next: (*state()).observation_head,
                key,
                owner: if flags & c::program_observation_flag_portable != 0 {
                    c::ProgramProcessHandle::default()
                } else {
                    creator
                },
                completion: node,
            });
            (*node).observation_refs += 1;
            (*state()).observation_head = item;
            0
        }
    };
    registry::unlock();
    if result != 0 {
        unsafe {
            heap::r4os_rust_heap_free(memory, mem::size_of::<ProgramObservationNode>());
        }
    } else {
        unsafe {
            out.write(key);
        }
    }
    result
}

unsafe fn close(key: c::ProgramObservationHandle) -> i32 {
    if !valid(key) {
        return -1;
    }
    if !registry::lock() {
        return -5;
    }
    let mut removed = ptr::null_mut();
    let mut finished = ptr::null_mut();
    unsafe {
        let mut link = ptr::addr_of_mut!((*state()).observation_head);
        while !(*link).is_null() {
            let item = *link;
            if (*item).key.id == key.id && registry::equal((*item).key.process, key.process) {
                *link = (*item).next;
                removed = item;
                let node = (*item).completion;
                assert!((*node).observation_refs != 0);
                (*node).observation_refs -= 1;
                if (*node).observation_refs == 0
                    && ((*node).state == 3 || ((*node).state == 2 && !(*node).owner))
                {
                    if (*node).state == 2 {
                        assert!((*(*state()).stats).completion_ready != 0);
                        (*(*state()).stats).completion_ready -= 1;
                    }
                    registry::unlink_completion(node);
                    (*node).state = 3;
                    finished = node;
                }
                break;
            }
            link = ptr::addr_of_mut!((*item).next);
        }
    }
    registry::unlock();
    if removed.is_null() {
        return -2;
    }
    unsafe {
        heap::r4os_rust_heap_free(removed.cast(), mem::size_of::<ProgramObservationNode>());
        if !finished.is_null() {
            completions::release(finished);
        }
    }
    signal();
    0
}
/// # Safety
/// `input` points to an accessible aligned observation handle. Copies of
/// this token name the same binding and become closed on success.
pub unsafe extern "C" fn release(input: *const c::ProgramObservationHandle) -> i32 {
    if input.is_null() {
        return -1;
    }
    let Some(_task) = task_context::Admission::acquire(true) else {
        return -5;
    };
    unsafe { close(input.read()) }
}
pub(super) unsafe fn orphan(creator: c::ProgramProcessHandle) {
    if !registry::valid(creator) {
        return;
    }
    loop {
        if !registry::lock() {
            return;
        }
        let key = unsafe {
            let mut item = (*state()).observation_head;
            let mut key = None;
            while !item.is_null() {
                if registry::equal((*item).owner, creator) {
                    key = Some((*item).key);
                    break;
                }
                item = (*item).next;
            }
            key
        };
        registry::unlock();
        let Some(key) = key else {
            return;
        };
        unsafe {
            close(key);
        }
    }
}

/// Failure-only metadata; normal writes mutate their console-owned flags.
/// No caller or output owner is retained beyond this registry transaction.
pub(super) unsafe fn output_incomplete(instance: *const super::registry_types::ProgramInstance) {
    if instance.is_null() || !registry::lock() {
        return;
    }
    unsafe {
        let slot = registry::lookup_id((*instance).id, true);
        if !slot.is_null()
            && ptr::addr_of!((*slot).instance) == instance
            && !(*slot).completion.is_null()
        {
            (*(*slot).completion).flags |= c::program_completion_flag_output_incomplete;
        }
    }
    registry::unlock();
}

unsafe fn query(key: c::ProgramObservationHandle, out: *mut c::ProgramProcessCompletion) -> i32 {
    if !registry::lock() {
        return -5;
    }
    let result = unsafe {
        let item = find(key);
        if item.is_null() {
            -2
        } else {
            let node = (*item).completion;
            if (*node).state == 1 {
                -5
            } else {
                assert!(matches!((*node).state, 2 | 3));
                out.write(completions::record(node));
                0
            }
        }
    };
    registry::unlock();
    result
}
/// # Safety
/// `input` is a readable aligned observation and `out` writable aligned
/// completion storage, valid through this possibly blocking call.
pub unsafe extern "C" fn wait(
    input: *const c::ProgramObservationHandle,
    timeout: u64,
    out: *mut c::ProgramProcessCompletion,
) -> i32 {
    if input.is_null() || out.is_null() {
        return -1;
    }
    let key = unsafe { input.read() };
    if !valid(key) {
        return -1;
    }
    let Some(_task) = task_context::Admission::acquire(true) else {
        return -5;
    };
    owner::prepare_stack();
    let start = if timeout == u64::MAX {
        0
    } else {
        timer::tick_count()
    };
    loop {
        let revision = REVISION.load(Ordering::Acquire);
        let mut result = c::ProgramProcessCompletion::default();
        let status = unsafe { query(key, ptr::addr_of_mut!(result)) };
        if status != -5 {
            if status == 0 {
                unsafe {
                    out.write(result);
                }
            }
            return status;
        }
        if (unsafe { context::handle() }).is_some_and(|own| registry::equal(own, key.process)) {
            return -7;
        }
        let remaining = if timeout == u64::MAX {
            u64::MAX
        } else {
            timeout.saturating_sub(timer::tick_count().wrapping_sub(start))
        };
        if remaining == 0 {
            return -6;
        }
        let request = WaitRequest {
            timeout: remaining,
            reason: Text::literal(b"program_observation"),
            predicate: Some(unchanged),
            context: ptr::addr_of!(revision).cast_mut().cast(),
            release: None,
            release_context: ptr::null_mut(),
            flags: 0,
            consume_runtime: false,
        };
        let wake = unsafe { sync_wait::r4os_rust_sync_wait(WAITERS.0.get(), &request) };
        if !matches!(wake, 1 | 2) {
            return -5;
        }
    }
}

/// # Safety
/// `input` is readable and aligned; `out` covers `capacity` writable bytes
/// and `out_read` one aligned writable u32, all valid for this call.
pub unsafe extern "C" fn read(
    input: *const c::ProgramObservationHandle,
    offset: u32,
    out: *mut u8,
    capacity: u32,
    out_read: *mut u32,
) -> i32 {
    if input.is_null() || out.is_null() || out_read.is_null() {
        return -1;
    }
    let key = unsafe { input.read() };
    if !valid(key) {
        return -1;
    }
    let Some(_task) = task_context::Admission::acquire(true) else {
        return -5;
    };
    owner::prepare_stack();
    unsafe { copy_output(key, offset, out, capacity, out_read) }
}
#[inline(never)]
unsafe fn copy_output(
    key: c::ProgramObservationHandle,
    offset: u32,
    out: *mut u8,
    capacity: u32,
    out_read: *mut u32,
) -> i32 {
    // Touch staging before Registry. No caller access or output allocation
    // occurs while holding that owner; detached output is immutable/resident.
    let mut resident = [0u8; console::CAPACITY];
    if !registry::lock() {
        return -5;
    }
    let length = unsafe {
        let item = find(key);
        if item.is_null() {
            Err(-2)
        } else {
            let node = (*item).completion;
            if !matches!((*node).state, 2 | 3) {
                Err(-5)
            } else if (*node).flags & c::program_completion_flag_output_available == 0 {
                Err(-12)
            } else if offset > (*node).output_length
                || (*node).output_length as usize > resident.len()
            {
                Err(-13)
            } else {
                let count =
                    (capacity as usize).min((*node).output_length as usize - offset as usize);
                if count != 0
                    && ((*node).output_payload.is_null()
                        || console::r4os_rust_program_console_read(
                            (*node).output_payload.cast::<t::ConsoleTranscript>(),
                            offset as usize,
                            resident.as_mut_ptr(),
                            count,
                            false,
                        ) != count)
                {
                    Err(-13)
                } else {
                    Ok(count)
                }
            }
        }
    };
    registry::unlock();
    match length {
        Err(code) => code,
        Ok(count) => unsafe {
            ptr::copy_nonoverlapping(resident.as_ptr(), out, count);
            out_read.write(count as u32);
            0
        },
    }
}
