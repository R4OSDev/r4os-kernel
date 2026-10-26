//! Bounded Service continuations. Endpoint payloads are copied into native
//! slots; caller pointers stay owned by the exact Async request until its
//! terminal completion. No worker, Task-bound token or mutex spans a reply wait.
use super::*;
use core::sync::atomic::{AtomicU32, AtomicU64};

// Notification coalescing only, never endpoint identity/ownership. Colliding
// handles may cause an extra bounded retry; the native endpoint revalidates ID.
struct Bucket {
    interest: AtomicU32,
    epoch: AtomicU64,
}
static BUCKETS: [Bucket; 64] = [const {
    Bucket {
        interest: AtomicU32::new(0),
        epoch: AtomicU64::new(1),
    }
}; 64];
fn bucket(handle: u32) -> &'static Bucket {
    &BUCKETS[handle as usize % BUCKETS.len()]
}
pub(super) enum Step {
    Pending,
    Done(i32),
}
pub(super) fn epoch(handle: u32) -> u64 {
    bucket(handle).epoch.load(Ordering::Acquire)
}
pub(super) unsafe fn interested(req: *mut Request) {
    unsafe {
        assert!(!(*req).service_interested);
        (*req).service_interested = true;
        assert!(
            bucket((*req).service_handle)
                .interest
                .fetch_add(1, Ordering::AcqRel)
                < CAPACITY as u32
        );
    }
}
/// Async mutex held; no backend may retain the service ID at terminal publish.
pub(super) unsafe fn finished(req: *mut Request) {
    unsafe {
        if (*req).service_interested {
            assert_eq!(
                (*req).service_request_id,
                0,
                "service completion before endpoint release"
            );
            (*req).service_interested = false;
            assert!(
                bucket((*req).service_handle)
                    .interest
                    .fetch_sub(1, Ordering::AcqRel)
                    != 0
            );
        }
    }
}
/// Called after a real response/permit/lifetime change under the endpoint
/// owner. No Async lock, request pointer, registration or foreign lifetime.
/// Event state is resident and synchronized by its existing Runtime owner.
pub(super) fn changed(handle: u32) {
    let bucket = bucket(handle);
    if bucket.interest.load(Ordering::Acquire) == 0 {
        return;
    }
    bucket.epoch.fetch_add(1, Ordering::AcqRel);
    unsafe {
        for i in dispatch::WIDTH..WORKERS {
            sync_objects::r4os_rust_sync_event(
                ptr::addr_of_mut!((*state()).workers[i].event),
                0,
                0,
            );
        }
    }
}
pub(super) unsafe fn expired(req: *const Request, now: u64) -> bool {
    unsafe {
        (*req).service_timeout != 0
            && (*req).service_timeout != u64::MAX
            && now >= (*req).service_deadline
    }
}
/// Async mutex held. Only detached pending continuations contribute a deadline;
/// an active worker owns its own bounded attempt and cancellation lifetime.
pub(super) unsafe fn wait_ticks() -> u64 {
    unsafe {
        let mut earliest = u64::MAX;
        for i in 0..CAPACITY {
            let req = ptr::addr_of!((*state()).requests[i]);
            if (*req).used
                && (*req).kind == Kind::Service
                && (*req).state == 2
                && (*req).task_id == 0
                && !(*req).cancel_requested
                && (*req).service_timeout != 0
                && (*req).service_timeout != u64::MAX
            {
                earliest = earliest.min((*req).service_deadline);
            }
        }
        if earliest == u64::MAX {
            u64::MAX
        } else {
            earliest.saturating_sub(timer::tick_count())
        }
    }
}
/// The exact worker still owns req. Cancellation retains the ID until the
/// endpoint acknowledges removal; caller retirement cannot free these buffers
/// until the actual worker release/terminal completion, as before.
pub(super) unsafe fn abort(req: *mut Request) {
    unsafe {
        let id = (*req).service_request_id;
        if id != 0 {
            program_async_service_control(false, (*req).service_handle, id, 0);
            clear_service(req, id);
        }
    }
}
pub(super) unsafe fn step(req: *mut Request) -> Step {
    unsafe {
        if expired(req, timer::tick_count()) {
            abort(req);
            return Step::Done(c::service_api_result_timeout);
        }
        let handle = (*req).service_handle;
        if (*req).service_request_id == 0 {
            let mut token = task_context::Unwind::EMPTY;
            let data = if (*req).service_request_len == 0 {
                b"".as_ptr()
            } else {
                ptr::with_exposed_provenance((*req).service_request)
            };
            // Nonblocking admission. An unsuccessful admission retains no
            // permit or Task-bound token across this continuation's release.
            let raw = program_async_service_submit(
                handle,
                (*req).owner_id,
                (*req).service_op,
                data,
                (*req).service_request_len,
                if (*req).service_timeout != 0 && (*req).service_timeout != u64::MAX {
                    Some((*req).service_deadline)
                } else {
                    None
                },
                &mut token,
            );
            if raw == c::service_api_result_busy && (*req).service_timeout != 0 {
                return Step::Pending;
            }
            if raw <= 0 {
                return Step::Done(raw);
            }
            let id = raw as u32;
            if !publish_service(req, id) {
                program_async_service_control(false, handle, id, 0);
                task_context::leave(token);
                return Step::Done(CANCELLED);
            }
            if !task_context::leave(token) {
                abort(req);
                return Step::Done(BUSY);
            }
        }
        // Admission and queue delay consume the SAME submission deadline.
        if expired(req, timer::tick_count()) {
            abort(req);
            return Step::Done(c::service_api_result_timeout);
        }
        let id = (*req).service_request_id;
        let header = ptr::with_exposed_provenance_mut::<Header>((*req).service_response_header);
        let mut empty = 0u8;
        let output = if (*req).service_response_capacity == 0 {
            ptr::addr_of_mut!(empty)
        } else {
            ptr::with_exposed_provenance_mut((*req).service_response)
        };
        let result = program_async_service_take(
            handle,
            id,
            header,
            output,
            (*req).service_response_capacity,
            if (*req).service_timeout != 0 && (*req).service_timeout != u64::MAX {
                Some((*req).service_deadline)
            } else {
                None
            },
        );
        if result < 0 {
            abort(req);
            return Step::Done(result);
        }
        if result > 0 || (*header).magic == c::service_api_magic {
            // take_response has already removed the exact endpoint slot.
            clear_service(req, id);
            return Step::Done(result);
        }
        if (*req).service_timeout == 0 {
            abort(req);
            Step::Done(c::service_api_result_timeout)
        } else {
            Step::Pending
        }
    }
}
