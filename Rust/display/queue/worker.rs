//! Exact worker operations of the parent owner; no duplicated state.
use super::*;

pub(super) fn publish_and_release() {
    let mut desktop_progress = false;
    for _ in 0..FENCE_CAPACITY {
        let notification = {
            let _bo = BoLock::acquire();
            unsafe { state() }.take_notification().map(|fence| {
                (fence, unsafe {
                    ptr::addr_of_mut!((*data()).completions[fence.slot as usize - 1])
                })
            })
        };
        let Some((fence, event)) = notification else {
            break;
        };
        // The exact publication pin prevents event reuse across this wake.
        unsafe { sync_objects::event_signal(event) };
        desktop_progress = true;
        let _bo = BoLock::acquire();
        unsafe { state() }
            .published(fence)
            .expect("exact terminal publication");
    }
    for _ in 0..FENCE_CAPACITY {
        let taken = {
            let _bo = BoLock::acquire();
            let Some(ticket) = (unsafe { state() }).take_release() else {
                break;
            };
            let result = with_resources(|_, resources, bo| resources.release(bo, ticket));
            if result.is_err() {
                unsafe { state() }
                    .released(ticket, false)
                    .expect("exact failed resource release");
                break;
            }
            let p = unsafe { data() };
            let index = ticket.fence.slot as usize - 1;
            let native = unsafe { (*p).native_jobs[index] };
            if !native.is_null() {
                unsafe { (&mut *native).release_bindings_locked() };
            }
            unsafe {
                (*p).native_jobs[index] = ptr::null_mut();
            }
            unsafe { state() }
                .released(ticket, true)
                .expect("exact resource release");
            unsafe { state() }
                .retain_waiter(ticket.fence)
                .expect("exact resource publication pin");
            (ticket, native, unsafe {
                ptr::addr_of_mut!((*p).releases[index])
            })
        };
        let (ticket, native, event) = taken;
        if !native.is_null() {
            unsafe { queue_native::Job::destroy(native) };
        }
        unsafe { sync_objects::event_signal(event) };
        desktop_progress = true;
        let _bo = BoLock::acquire();
        unsafe { state() }
            .release_waiter(ticket.fence)
            .expect("exact resource publication return");
    }
    {
        let _bo = BoLock::acquire();
        while unsafe { state() }.reap_one().is_some() {}
    }
    buffers::collect();
    if desktop_progress {
        desktop_events::r4os_rust_desktop_signal();
    }
}

pub(super) fn copy_slice() -> bool {
    let instant = now();
    let selected = {
        let _bo = BoLock::acquire();
        unsafe { state() }.expire(instant);
        let _ = unsafe { state() }.take_ready_for(Some(SOFTWARE), instant);
        let p = unsafe { data() };
        let mut selected = None;
        for offset in 0..FENCE_CAPACITY {
            let slot = (unsafe { (*p).next_copy } + offset) % FENCE_CAPACITY;
            let entry = unsafe { (*p).resources[slot] };
            if entry.fence.slot == 0 || entry.fence.binding != SOFTWARE {
                continue;
            }
            if !unsafe { state() }
                .query(entry.fence)
                .expect("software queued entry")
                .device_active
            {
                continue;
            }
            selected = Some(entry);
            unsafe {
                (*p).next_copy = (slot + 1) % FENCE_CAPACITY;
            }
            break;
        }
        selected
    };
    let Some(entry) = selected else {
        return false;
    };
    let status = query(entry.fence).expect("retained software copy");
    let count = if status.phase == model::TERMINAL {
        0
    } else {
        unsafe { resource::copy_chunk(&entry, COPY_SLICE_BYTES, 256) }
    };
    buffers::visibility();
    let finished_at = now();
    let _bo = BoLock::acquire();
    unsafe {
        (*data()).resources[entry.fence.slot as usize - 1].copied += count;
    }
    if status.phase == model::TERMINAL || entry.copied + count == entry.bytes {
        unsafe { state() }
            .complete(entry.fence, model::COMPLETE, true, finished_at)
            .expect("exact software copy completion");
    }
    true
}

pub(super) unsafe extern "C" fn worker_main() {
    loop {
        for slot in 0..FENCE_CAPACITY {
            if let Some(ack) = with_ingress(|ingress| ingress.take(slot)) {
                let _bo = BoLock::acquire();
                match unsafe { state() }.complete(ack.fence, ack.result, true, ack.instant) {
                    Ok(()) | Err(Error::Stale | Error::AlreadyCompleted) => {}
                    Err(_) => panic!("native exact IRQ completion"),
                }
            }
        }
        publish_and_release();
        let allocation_pending = allocations::service();
        let virtual_pending = virtual_runtime::service();
        let native_pending = notify_native();
        if copy_slice() {
            scheduler::yield_task();
            continue;
        }
        let pending = {
            let _bo = BoLock::acquire();
            unsafe { state() }.jobs.iter().any(|job| {
                job.fence.slot != 0 && (job.phase != model::TERMINAL || job.notification_pending)
            })
        };
        let _ = unsafe {
            sync_objects::event_wait(
                WORKER_EVENT.0.get(),
                if pending || native_pending || allocation_pending || virtual_pending {
                    1
                } else {
                    u64::MAX
                },
            )
        };
    }
}
