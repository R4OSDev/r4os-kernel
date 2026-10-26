//! Exact endpoint generations, bounded request slots and wait handoff.
use super::{
    registry::{self as reg, Identity},
    registry_types::*,
};
use crate::{
    arch::runtime::RuntimeGuard,
    contract as c,
    fs::vfs_types::Bytes,
    platform::{clock_math, timer},
    sched::{sync_objects, sync_types::*, sync_wait, task_context},
};
use core::{
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicU32, Ordering},
};
pub(super) unsafe fn slot(ep: *mut Endpoint, index: usize) -> *mut RequestSlot {
    assert!(index < DEPTH);
    unsafe {
        ptr::addr_of_mut!((*ep).queue)
            .cast::<RequestSlot>()
            .add(index)
    }
}
unsafe fn queue_op(queue: *mut WaitQueue, operation: u8, result: u32) -> u64 {
    unsafe {
        sync_wait::r4os_rust_sync_queue(
            queue,
            operation,
            ptr::null_mut(),
            u64::from(result),
            ptr::null_mut(),
        )
    }
}
unsafe fn release_permit(ep: *mut Endpoint) {
    crate::program::async_io::service_changed(unsafe { (*ep).handle });
    unsafe {
        sync_objects::r4os_rust_sync_semaphore(
            ptr::addr_of_mut!((*ep).slots_available),
            4,
            1,
            ptr::null(),
            ptr::null_mut(),
        )
    };
}
unsafe fn try_permit(ep: *mut Endpoint) -> bool {
    unsafe {
        sync_objects::r4os_rust_sync_semaphore(
            ptr::addr_of_mut!((*ep).slots_available),
            0,
            0,
            ptr::null(),
            ptr::null_mut(),
        ) != 0
    }
}
pub(super) unsafe fn reset_slot(p: *mut RequestSlot) {
    unsafe {
        (*p).state = REQUEST_FREE;
        (*p).request_id = 0;
        (*p).client_id = 0;
        (*p).op = 0;
        (*p).flags = 0;
        (*p).request_len = 0;
        (*p).response_len = 0;
        (*p).response_status = c::service_api_result_ok;
        (*p).response_available = WaitQueue::EMPTY;
    }
}
pub(super) unsafe fn reset_metadata(ep: *mut Endpoint, count_reset: bool) {
    unsafe {
        // Stable mutex/generation/request-ID block/lifetime/timing and every
        // payload byte remain untouched, including during immediate reuse.
        (*ep).used = false;
        (*ep).service_slot = 0;
        (*ep).handle = 0;
        (*ep).flags = 0;
        (*ep).queue_high_water = 0;
        (*ep).max_active_workers = 0;
        (*ep).open_handles = 0;
        (*ep).requests = 0;
        (*ep).responses = 0;
        (*ep).drops = 0;
        (*ep).busy_rejections = 0;
        (*ep).timeouts = 0;
        (*ep).cancellations = 0;
        (*ep).completion_waits = 0;
        (*ep).completion_timeouts = 0;
        (*ep).completion_wait_rounds = 0;
        (*ep).targeted_response_wakes = 0;
        (*ep).targeted_response_wake_misses = 0;
        (*ep).admission_waits = 0;
        (*ep).admission_timeouts = 0;
        (*ep).queued_count = 0;
        (*ep).requests_available = WaitQueue::EMPTY;
        (*ep).slots_available = Semaphore {
            count: DEPTH as u32,
            max_count: DEPTH as u32,
            queue: WaitQueue::EMPTY,
        };
        for i in 0..DEPTH {
            reset_slot(slot(ep, i));
        }
        if count_reset {
            (*ep).lifetime.endpoint_metadata_resets =
                (*ep).lifetime.endpoint_metadata_resets.wrapping_add(1);
        }
    }
}
pub(super) unsafe fn wake_waiters(ep: *mut Endpoint, result: u32) {
    crate::program::async_io::service_changed(unsafe { (*ep).handle });
    unsafe {
        for i in 0..DEPTH {
            let s = slot(ep, i);
            if (*s).state != REQUEST_FREE {
                queue_op(ptr::addr_of_mut!((*s).response_available), 4, result);
            }
        }
        queue_op(ptr::addr_of_mut!((*ep).requests_available), 4, result);
        queue_op(ptr::addr_of_mut!((*ep).slots_available.queue), 4, result);
    }
}
pub(super) unsafe fn retire(ep: *mut Endpoint, result: u32) {
    unsafe {
        wake_waiters(ep, result);
        reset_metadata(ep, true);
    }
}
pub(super) unsafe fn advance(ep: *mut Endpoint) {
    unsafe {
        (*ep).generation = (*ep).generation.wrapping_add(1);
        if (*ep).generation == 0 {
            (*ep).generation = 1;
        }
    }
}
pub(super) unsafe fn allocate_request(ep: *mut Endpoint, counter: *mut u32) -> u32 {
    unsafe {
        if (*ep).request_id_next != 0 && (*ep).request_id_next <= (*ep).request_id_limit {
            let id = (*ep).request_id_next;
            (*ep).request_id_next += 1;
            return id;
        }
        let counter = AtomicU32::from_ptr(counter);
        let mut observed = counter.load(Ordering::Relaxed);
        loop {
            let first = if observed == 0 || observed > REQUEST_MAX {
                1
            } else {
                observed
            };
            let count = REQUEST_BLOCK.min(REQUEST_MAX - first + 1);
            let after = first + count;
            let next = if after > REQUEST_MAX { 1 } else { after };
            match counter.compare_exchange_weak(
                observed,
                next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    (*ep).request_id_next = first + 1;
                    (*ep).request_id_limit = first + count - 1;
                    return first;
                }
                Err(actual) => observed = actual,
            }
        }
    }
}
unsafe fn scan(ep: *mut Endpoint, n: usize) {
    unsafe {
        (*ep).lifetime.queue_scan_passes = (*ep).lifetime.queue_scan_passes.wrapping_add(1);
        (*ep).lifetime.queue_scan_slots = (*ep).lifetime.queue_scan_slots.wrapping_add(n as u64);
    }
}
unsafe fn free_slot(ep: *mut Endpoint) -> Option<usize> {
    unsafe {
        for i in 0..DEPTH {
            if (*slot(ep, i)).state == REQUEST_FREE {
                scan(ep, i + 1);
                return Some(i);
            }
        }
        scan(ep, DEPTH);
        None
    }
}
unsafe fn queued_slot(ep: *mut Endpoint) -> Option<usize> {
    unsafe {
        if (*ep).queued_count == 0 {
            return None;
        }
        scan(ep, 1);
        Some((*ep).queued_slots[0])
    }
}
unsafe fn remove_queued(ep: *mut Endpoint, s: *const RequestSlot) {
    unsafe {
        for i in 0..(*ep).queued_count {
            if !ptr::eq(slot(ep, (*ep).queued_slots[i]), s) {
                continue;
            }
            scan(ep, i + 1);
            for n in i..(*ep).queued_count - 1 {
                (*ep).queued_slots[n] = (*ep).queued_slots[n + 1];
            }
            (*ep).queued_count -= 1;
            return;
        }
        panic!("Queued service slot missing from admitted FIFO");
    }
}
pub(super) unsafe fn find_request(ep: *mut Endpoint, id: u32, state: Option<u8>) -> Option<usize> {
    unsafe {
        for i in 0..DEPTH {
            let s = slot(ep, i);
            if (if let Some(state) = state {
                (*s).state == state
            } else {
                (*s).state != REQUEST_FREE
            }) && (*s).request_id == id
            {
                scan(ep, i + 1);
                return Some(i);
            }
        }
        scan(ep, DEPTH);
        None
    }
}
pub(super) unsafe fn counts(ep: *mut Endpoint) -> QueueCounts {
    unsafe {
        let mut out = QueueCounts::EMPTY;
        for i in 0..DEPTH {
            match (*slot(ep, i)).state {
                REQUEST_FREE => {}
                REQUEST_QUEUED => out.queued += 1,
                REQUEST_DELIVERED => out.delivered += 1,
                REQUEST_RESPONDED => out.responded += 1,
                _ => panic!("Invalid service request state"),
            }
        }
        out.used = out.queued + out.delivered + out.responded;
        scan(ep, DEPTH);
        out
    }
}
unsafe fn clear_slot(ep: *mut Endpoint, s: *mut RequestSlot) {
    unsafe {
        if (*s).state == REQUEST_QUEUED {
            remove_queued(ep, s);
        }
        queue_op(ptr::addr_of_mut!((*s).response_available), 4, 3);
        reset_slot(s);
        (*ep).lifetime.slot_metadata_resets = (*ep).lifetime.slot_metadata_resets.wrapping_add(1);
    }
}
unsafe fn fill_header(
    s: *const RequestSlot,
    out: *mut c::ServiceMessageHeader,
    len: u16,
    status: i32,
) {
    unsafe {
        out.write(c::ServiceMessageHeader {
            op: (*s).op,
            request_id: (*s).request_id,
            client_id: (*s).client_id,
            flags: (*s).flags,
            payload_len: len,
            status,
            ..c::ServiceMessageHeader::default()
        });
    }
}
unsafe fn clear_header(out: *mut c::ServiceMessageHeader) {
    unsafe {
        out.write(c::ServiceMessageHeader {
            magic: 0,
            version: 0,
            ..c::ServiceMessageHeader::default()
        });
    }
}
unsafe fn publish(
    ep: *mut Endpoint,
    client: u32,
    op: u16,
    data: Bytes,
    deadline: Option<u64>,
) -> i32 {
    unsafe {
        let Some(i) = free_slot(ep) else {
            release_permit(ep);
            (*ep).busy_rejections = (*ep).busy_rejections.wrapping_add(1);
            return c::service_api_result_busy;
        };
        let s = slot(ep, i);
        let id = allocate_request(ep, reg::NEXT_REQUEST.as_ptr());
        reset_slot(s);
        (*s).request_id = id;
        (*s).client_id = client;
        (*s).op = op;
        (*s).flags = (*ep).flags;
        (*s).request_len = data.len as u16;
        if data.len != 0 {
            ptr::copy_nonoverlapping(
                data.data,
                ptr::addr_of_mut!((*s).request_payload).cast(),
                data.len,
            );
            (*ep).lifetime.payload_copy_bytes = (*ep)
                .lifetime
                .payload_copy_bytes
                .wrapping_add(data.len as u64);
        }
        // Copying pageable client bytes can itself wait. Async admission is
        // published only while its ORIGINAL submission deadline still holds.
        if deadline.is_some_and(|end| timer::tick_count() >= end) {
            reset_slot(s);
            release_permit(ep);
            (*ep).timeouts = (*ep).timeouts.wrapping_add(1);
            (*ep).admission_timeouts = (*ep).admission_timeouts.wrapping_add(1);
            return c::service_api_result_timeout;
        }
        assert!((*ep).queued_count < DEPTH);
        (*ep).queued_slots[(*ep).queued_count] = i;
        (*ep).queued_count += 1;
        (*s).state = REQUEST_QUEUED;
        (*ep).requests = (*ep).requests.wrapping_add(1);
        (*ep).queue_high_water = (*ep).queue_high_water.max(counts(ep).used);
        queue_op(ptr::addr_of_mut!((*ep).requests_available), 0, 0);
        id as i32
    }
}
pub(super) unsafe fn submit(handle: u32, client: u32, op: u16, data: Bytes) -> i32 {
    unsafe {
        if op == 0 {
            return c::service_api_result_invalid;
        }
        if data.len > PAYLOAD {
            return c::service_api_result_payload_too_large;
        }
        let Some(lease) = reg::lock_handle(handle, 4) else {
            return c::service_api_result_bad_handle;
        };
        let ep = lease.endpoint;
        if !try_permit(ep) {
            (*ep).busy_rejections = (*ep).busy_rejections.wrapping_add(1);
            return c::service_api_result_busy;
        }
        publish(ep, client, op, data, None)
    }
}
pub(super) unsafe fn leave_unwind(unwind: *mut task_context::Unwind) {
    unsafe {
        if (*unwind).active {
            let _guard = RuntimeGuard::acquire();
            task_context::leave(*unwind);
        }
        unwind.write(task_context::Unwind::EMPTY);
    }
}
unsafe fn publish_guarded(
    ep: *mut Endpoint,
    client: u32,
    op: u16,
    data: Bytes,
    output: *mut task_context::Unwind,
    acquired: task_context::Unwind,
) -> i32 {
    unsafe { publish_guarded_at(ep, client, op, data, output, acquired, None) }
}
unsafe fn publish_guarded_at(
    ep: *mut Endpoint,
    client: u32,
    op: u16,
    data: Bytes,
    output: *mut task_context::Unwind,
    acquired: task_context::Unwind,
    deadline: Option<u64>,
) -> i32 {
    unsafe {
        let mut owned = acquired;
        if !output.is_null() {
            if !owned.active {
                {
                    let _guard = RuntimeGuard::acquire();
                    owned = task_context::enter();
                }
                if !owned.admitted() {
                    release_permit(ep);
                    return c::service_api_result_busy;
                }
            }
            output.write(owned);
        }
        let result = publish(ep, client, op, data, deadline);
        if result <= 0 {
            leave_unwind(&mut owned);
            if !output.is_null() {
                output.write(task_context::Unwind::EMPTY);
            }
        } else if output.is_null() {
            leave_unwind(&mut owned);
        }
        result
    }
}
/// Productive Async admission never sleeps for a permit. Its absolute finite
/// deadline is checked under the exact endpoint owner, again after payload copy.
pub(crate) unsafe fn submit_async(
    handle: u32,
    client: u32,
    op: u16,
    data: Bytes,
    deadline: Option<u64>,
    output: *mut task_context::Unwind,
) -> i32 {
    unsafe {
        if op == 0 {
            return c::service_api_result_invalid;
        }
        if data.len > PAYLOAD {
            return c::service_api_result_payload_too_large;
        }
        let Some(lease) = reg::lock_handle(handle, 5) else {
            return c::service_api_result_bad_handle;
        };
        let ep = lease.endpoint;
        if deadline.is_some_and(|end| timer::tick_count() >= end) {
            (*ep).timeouts = (*ep).timeouts.wrapping_add(1);
            (*ep).admission_timeouts = (*ep).admission_timeouts.wrapping_add(1);
            return c::service_api_result_timeout;
        }
        if !try_permit(ep) {
            (*ep).busy_rejections = (*ep).busy_rejections.wrapping_add(1);
            return c::service_api_result_busy;
        }
        publish_guarded_at(
            ep,
            client,
            op,
            data,
            output,
            task_context::Unwind::EMPTY,
            deadline,
        )
    }
}
pub(super) unsafe fn submit_wait(
    handle: u32,
    client: u32,
    op: u16,
    data: Bytes,
    timeout: u64,
    output: *mut task_context::Unwind,
) -> i32 {
    unsafe {
        if !output.is_null() {
            output.write(task_context::Unwind::EMPTY);
        }
        if op == 0 {
            return c::service_api_result_invalid;
        }
        if data.len > PAYLOAD {
            return c::service_api_result_payload_too_large;
        }
        let forever = timeout == FOREVER;
        let deadline = if forever {
            0
        } else {
            clock_math::finite_deadline(timer::tick_count(), timeout)
        };
        let Some(identity) = reg::lookup_identity(handle) else {
            return c::service_api_result_bad_handle;
        };
        let Some(mut lease) = reg::lock_identity(identity, 5) else {
            return c::service_api_result_bad_handle;
        };
        let ep = lease.endpoint;
        if timeout != 0 && !forever && timer::tick_count() >= deadline {
            (*ep).timeouts = (*ep).timeouts.wrapping_add(1);
            (*ep).admission_timeouts = (*ep).admission_timeouts.wrapping_add(1);
            return c::service_api_result_timeout;
        }
        if try_permit(ep) {
            return publish_guarded(ep, client, op, data, output, task_context::Unwind::EMPTY);
        }
        if timeout == 0 {
            (*ep).busy_rejections = (*ep).busy_rejections.wrapping_add(1);
            return c::service_api_result_busy;
        }
        let now = timer::tick_count();
        if !forever && now >= deadline {
            (*ep).timeouts = (*ep).timeouts.wrapping_add(1);
            (*ep).admission_timeouts = (*ep).admission_timeouts.wrapping_add(1);
            return c::service_api_result_timeout;
        }
        (*ep).admission_waits = (*ep).admission_waits.wrapping_add(1);
        let request = WaitRequest {
            timeout: if forever { FOREVER } else { deadline - now },
            reason: Text::literal(b"semaphore"),
            predicate: None,
            context: ptr::null_mut(),
            release: Some(reg::release_for_wait),
            release_context: ptr::from_mut(&mut lease.guard).cast(),
            flags: 0,
            consume_runtime: false,
        };
        let mut unwind = task_context::Unwind::EMPTY;
        let result = sync_objects::r4os_rust_sync_semaphore(
            ptr::addr_of_mut!((*ep).slots_available),
            3,
            0,
            &request,
            &mut unwind,
        );
        let Some(result_lease) = reg::lock_identity(identity, 5) else {
            leave_unwind(&mut unwind);
            return if matches!(result, 3 | 4) {
                c::service_api_result_not_running
            } else {
                c::service_api_result_bad_handle
            };
        };
        let ep = result_lease.endpoint;
        match result {
            1 => {}
            2 => {
                (*ep).timeouts = (*ep).timeouts.wrapping_add(1);
                (*ep).admission_timeouts = (*ep).admission_timeouts.wrapping_add(1);
                return c::service_api_result_timeout;
            }
            3 | 4 => {
                leave_unwind(&mut unwind);
                return c::service_api_result_not_running;
            }
            0 | 5 => {
                leave_unwind(&mut unwind);
                return c::service_api_result_busy;
            }
            _ => panic!("Invalid service admission wait result"),
        }
        if !forever && timer::tick_count() >= deadline {
            release_permit(ep);
            leave_unwind(&mut unwind);
            (*ep).timeouts = (*ep).timeouts.wrapping_add(1);
            (*ep).admission_timeouts = (*ep).admission_timeouts.wrapping_add(1);
            return c::service_api_result_timeout;
        }
        publish_guarded(ep, client, op, data, output, unwind)
    }
}
pub(super) unsafe fn recv(
    handle: u32,
    header: *mut c::ServiceMessageHeader,
    out: *mut u8,
    capacity: usize,
) -> i32 {
    unsafe {
        clear_header(header);
        let Some(lease) = reg::lock_handle(handle, 4) else {
            return c::service_api_result_bad_handle;
        };
        let ep = lease.endpoint;
        let Some(i) = queued_slot(ep) else {
            return 0;
        };
        let s = slot(ep, i);
        let len = usize::from((*s).request_len);
        if capacity < len {
            fill_header(s, header, (*s).request_len, c::service_api_result_ok);
            return c::service_api_result_buffer_too_small;
        }
        if len != 0 {
            ptr::copy_nonoverlapping(ptr::addr_of!((*s).request_payload).cast(), out, len);
            (*ep).lifetime.payload_copy_bytes =
                (*ep).lifetime.payload_copy_bytes.wrapping_add(len as u64);
        }
        fill_header(s, header, (*s).request_len, c::service_api_result_ok);
        remove_queued(ep, s);
        (*s).state = REQUEST_DELIVERED;
        (*ep).max_active_workers = (*ep).max_active_workers.max(counts(ep).delivered);
        len as i32
    }
}
pub(super) unsafe fn reply(handle: u32, id: u32, status: i32, data: Bytes) -> i32 {
    unsafe {
        if data.len > PAYLOAD {
            return c::service_api_result_payload_too_large;
        }
        let Some(lease) = reg::lock_handle(handle, 4) else {
            return c::service_api_result_bad_handle;
        };
        let ep = lease.endpoint;
        let Some(i) = find_request(ep, id, Some(REQUEST_DELIVERED)) else {
            return c::service_api_result_not_found;
        };
        let s = slot(ep, i);
        if data.len != 0 {
            ptr::copy_nonoverlapping(
                data.data,
                ptr::addr_of_mut!((*s).response_payload).cast(),
                data.len,
            );
            (*ep).lifetime.payload_copy_bytes = (*ep)
                .lifetime
                .payload_copy_bytes
                .wrapping_add(data.len as u64);
        }
        (*s).response_len = data.len as u16;
        (*s).response_status = status;
        (*s).state = REQUEST_RESPONDED;
        crate::program::async_io::service_changed((*ep).handle);
        (*ep).responses = (*ep).responses.wrapping_add(1);
        if queue_op(ptr::addr_of_mut!((*s).response_available), 0, 0) != 0 {
            (*ep).targeted_response_wakes = (*ep).targeted_response_wakes.wrapping_add(1);
        } else {
            (*ep).targeted_response_wake_misses =
                (*ep).targeted_response_wake_misses.wrapping_add(1);
        }
        c::service_api_result_ok
    }
}
pub(super) unsafe fn take_response(
    handle: u32,
    id: u32,
    header: *mut c::ServiceMessageHeader,
    out: *mut u8,
    capacity: usize,
) -> i32 {
    unsafe { take_response_inner(handle, id, header, out, capacity, false, None) }
}
/// Native Async continuation poll distinguishes a disappeared ID from a pending
/// reply. The public historical nonblocking take keeps its existing result0.
pub(crate) unsafe fn take_async_response(
    handle: u32,
    id: u32,
    header: *mut c::ServiceMessageHeader,
    out: *mut u8,
    capacity: usize,
    deadline: Option<u64>,
) -> i32 {
    unsafe { take_response_inner(handle, id, header, out, capacity, true, deadline) }
}
unsafe fn take_response_inner(
    handle: u32,
    id: u32,
    header: *mut c::ServiceMessageHeader,
    out: *mut u8,
    capacity: usize,
    require_id: bool,
    deadline: Option<u64>,
) -> i32 {
    unsafe {
        clear_header(header);
        let Some(lease) = reg::lock_handle(handle, 4) else {
            return c::service_api_result_bad_handle;
        };
        let ep = lease.endpoint;
        if deadline.is_some_and(|end| timer::tick_count() >= end) {
            return c::service_api_result_timeout;
        }
        let Some(i) = find_request(ep, id, None) else {
            return if require_id {
                c::service_api_result_not_found
            } else {
                0
            };
        };
        if (*slot(ep, i)).state != REQUEST_RESPONDED {
            return 0;
        }
        let s = slot(ep, i);
        let len = usize::from((*s).response_len);
        fill_header(s, header, (*s).response_len, (*s).response_status);
        if capacity < len {
            clear_slot(ep, s);
            release_permit(ep);
            return c::service_api_result_buffer_too_small;
        }
        if len != 0 {
            ptr::copy_nonoverlapping(ptr::addr_of!((*s).response_payload).cast(), out, len);
            (*ep).lifetime.payload_copy_bytes =
                (*ep).lifetime.payload_copy_bytes.wrapping_add(len as u64);
        }
        clear_slot(ep, s);
        release_permit(ep);
        len as i32
    }
}
struct EndpointWait {
    identity: Identity,
}
unsafe extern "C" fn endpoint_needed(raw: *mut c_void) -> bool {
    unsafe {
        let id = (*raw.cast::<EndpointWait>()).identity;
        reg::identity_matches(id.endpoint, id) && (*id.endpoint).queued_count == 0
    }
}
pub(super) unsafe fn poll(handle: u32) -> i32 {
    unsafe {
        let Some(lease) = reg::lock_handle(handle, 4) else {
            return c::service_api_result_bad_handle;
        };
        counts(lease.endpoint).queued as i32
    }
}
pub(super) unsafe fn wait_endpoint(handle: u32, timeout: u64) -> i32 {
    unsafe {
        let Some(identity) = reg::lookup_identity(handle) else {
            return c::service_api_result_bad_handle;
        };
        let Some(mut lease) = reg::lock_identity(identity, 5) else {
            return c::service_api_result_bad_handle;
        };
        let queued = counts(lease.endpoint).queued;
        if queued != 0 {
            return queued as i32;
        }
        let mut context = EndpointWait { identity };
        let mut request = WaitRequest::predicate(
            timeout,
            Text::literal(b"svc-endpoint"),
            endpoint_needed,
            ptr::from_mut(&mut context).cast(),
        );
        request.release = Some(reg::release_for_wait);
        request.release_context = ptr::from_mut(&mut lease.guard).cast();
        // The original sync transport releases the exact mutex and evaluates
        // this predicate under one runtime admission. No new producer can
        // acquire that mutex between release, predicate and enrollment.
        let result = sync_wait::r4os_rust_sync_wait(
            ptr::addr_of_mut!((*lease.endpoint).requests_available),
            &request,
        );
        let Some(result_lease) = reg::lock_identity(identity, 5) else {
            return if matches!(result, 3 | 4) {
                c::service_api_result_not_running
            } else {
                c::service_api_result_bad_handle
            };
        };
        counts(result_lease.endpoint).queued as i32
    }
}
struct ResponseWait {
    identity: Identity,
    slot: *mut RequestSlot,
    id: u32,
}
unsafe extern "C" fn response_needed(raw: *mut c_void) -> bool {
    unsafe {
        let p = raw.cast::<ResponseWait>();
        reg::identity_matches((*p).identity.endpoint, (*p).identity)
            && (*(*p).slot).request_id == (*p).id
            && (*(*p).slot).state != REQUEST_FREE
            && (*(*p).slot).state != REQUEST_RESPONDED
    }
}
pub(super) unsafe fn wait_response(handle: u32, id: u32, timeout: u64) -> i32 {
    unsafe {
        if id == 0 {
            return c::service_api_result_invalid;
        }
        let Some(identity) = reg::lookup_identity(handle) else {
            return c::service_api_result_bad_handle;
        };
        let Some(mut lease) = reg::lock_identity(identity, 5) else {
            return c::service_api_result_bad_handle;
        };
        let ep = lease.endpoint;
        let Some(i) = find_request(ep, id, None) else {
            return c::service_api_result_not_found;
        };
        if (*slot(ep, i)).state == REQUEST_RESPONDED {
            return c::service_api_result_ok;
        }
        (*ep).completion_waits = (*ep).completion_waits.wrapping_add(1);
        (*ep).completion_wait_rounds = (*ep).completion_wait_rounds.wrapping_add(1);
        let mut context = ResponseWait {
            identity,
            slot: slot(ep, i),
            id,
        };
        let mut request = WaitRequest::predicate(
            timeout,
            Text::literal(b"service-response"),
            response_needed,
            ptr::from_mut(&mut context).cast(),
        );
        request.release = Some(reg::release_for_wait);
        request.release_context = ptr::from_mut(&mut lease.guard).cast();
        let result = sync_wait::r4os_rust_sync_wait(
            ptr::addr_of_mut!((*context.slot).response_available),
            &request,
        );
        let Some(result_lease) = reg::lock_identity(identity, 5) else {
            return if matches!(result, 3 | 4) {
                c::service_api_result_not_running
            } else {
                c::service_api_result_bad_handle
            };
        };
        let ep = result_lease.endpoint;
        match result {
            2 => {
                (*ep).timeouts = (*ep).timeouts.wrapping_add(1);
                (*ep).completion_timeouts = (*ep).completion_timeouts.wrapping_add(1);
                return c::service_api_result_timeout;
            }
            3 | 4 => return c::service_api_result_not_running,
            0 | 5 => return c::service_api_result_busy,
            1 => {}
            _ => panic!("Invalid service response wait result"),
        }
        let Some(i) = find_request(ep, id, None) else {
            return c::service_api_result_not_found;
        };
        if (*slot(ep, i)).state == REQUEST_RESPONDED {
            c::service_api_result_ok
        } else {
            c::service_api_result_busy
        }
    }
}
pub(super) unsafe fn cancel(handle: u32, id: u32) -> i32 {
    unsafe {
        let Some(lease) = reg::lock_handle(handle, 4) else {
            return c::service_api_result_bad_handle;
        };
        let ep = lease.endpoint;
        let Some(i) = find_request(ep, id, None) else {
            return c::service_api_result_not_found;
        };
        (*ep).drops = (*ep).drops.wrapping_add(1);
        (*ep).cancellations = (*ep).cancellations.wrapping_add(1);
        clear_slot(ep, slot(ep, i));
        release_permit(ep);
        c::service_api_result_ok
    }
}
