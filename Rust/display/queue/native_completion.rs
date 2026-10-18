//! Exact native_completion operations of the parent owner; no duplicated state.
use super::*;

pub fn take_native(id: u32, binding: model::Binding, output_bytes: u32) -> Result<Taken, Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Unavailable);
    }
    let instant = now();
    let _bo = BoLock::acquire();
    let backend = unsafe { &*backend_locked(binding)? };
    if backend.owner.id != u64::from(id) || id == 0 {
        return Err(Error::WrongOwner);
    }
    if output_bytes < native_job_capacity(backend) {
        return Err(Error::Invalid);
    }
    if backend.closing {
        return Err(Error::DeviceLost);
    }
    let fence = unsafe { state() }
        .take_ready_for(Some(binding), instant)
        .ok_or(Error::Busy)?;
    with_ingress(|ingress| ingress.arm(id, fence)).expect("native exact IRQ ingress");
    buffers::visibility();
    Ok(Taken {
        entry: unsafe { (*data()).resources[fence.slot as usize - 1] },
        producer: unsafe { state() }.producer(fence)?,
    })
}

pub fn native_segment(
    id: u32,
    fence: model::Fence,
    which: u32,
    offset: u64,
    mask: u64,
) -> Result<a::GfxDmaSegment, Error> {
    if irq_router::in_dispatch() || which > 1 {
        return Err(Error::Invalid);
    }
    let _bo = BoLock::acquire();
    let _ = unsafe { job_backend_locked(id, fence) }?;
    if !unsafe { state() }.query(fence)?.device_active {
        return Err(Error::AlreadyCompleted);
    }
    let held = unsafe { (*data()).resources[fence.slot as usize - 1].uses[which as usize] }
        .get()
        .ok_or(Error::Invalid)?;
    Ok(driver_memory::dma_segment(held, offset, mask)?)
}

pub(super) unsafe fn native_job_locked(
    id: u32,
    fence: model::Fence,
) -> Result<*mut queue_native::Job, Error> {
    let _ = unsafe { job_backend_locked(id, fence) }?;
    if !unsafe { state() }.query(fence)?.device_active {
        return Err(Error::AlreadyCompleted);
    }
    let entry = unsafe { (*data()).resources[fence.slot as usize - 1] };
    if entry.fence != fence || entry.operation != resource::NATIVE {
        return Err(Error::Invalid);
    }
    let job = unsafe { (*data()).native_jobs[fence.slot as usize - 1] };
    if job.is_null() {
        Err(Error::Stale)
    } else {
        Ok(job)
    }
}

pub fn native_info(id: u32, fence: model::Fence) -> Result<a::GfxNativeJobInfo, Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Unavailable);
    }
    let _bo = BoLock::acquire();
    Ok(unsafe { (*native_job_locked(id, fence)?).info })
}

pub fn native_binding(
    id: u32,
    fence: model::Fence,
    index: u32,
) -> Result<a::GfxNativeBinding, Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Unavailable);
    }
    let _bo = BoLock::acquire();
    unsafe { (&*native_job_locked(id, fence)?).binding(index) }
}

pub fn native_data(
    id: u32,
    fence: model::Fence,
    offset: u32,
    count: u32,
) -> Result<NativeData, Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Unavailable);
    }
    if count == 0 || count > a::gfx_native_read_capacity {
        return Err(Error::Invalid);
    }
    let _bo = BoLock::acquire();
    let job = unsafe { &*native_job_locked(id, fence)? };
    if count as usize > job.command_bytes || offset as usize > job.command_bytes - count as usize {
        return Err(Error::Invalid);
    }
    let mut result = NativeData {
        bytes: [MaybeUninit::uninit(); a::gfx_native_read_capacity as usize],
    };
    unsafe {
        ptr::copy_nonoverlapping(
            job.commands.add(offset as usize),
            result.bytes.as_mut_ptr().cast(),
            count as usize,
        );
    }
    Ok(result)
}

pub fn native_render_list(id: u32, fence: model::Fence) -> Result<a::GfxRenderList, Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Unavailable);
    }
    let _bo = BoLock::acquire();
    let _ = unsafe { job_backend_locked(id, fence) }?;
    with_resources(|state, resources, _| resources.render_list(state, fence))
}

pub fn native_render_grid_list(
    id: u32,
    fence: model::Fence,
) -> Result<a::GfxRenderGridList, Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Unavailable);
    }
    let _bo = BoLock::acquire();
    let _ = unsafe { job_backend_locked(id, fence) }?;
    with_resources(|state, resources, _| resources.render_grid_list(state, fence))
}

pub fn native_render_color_list(
    id: u32,
    fence: model::Fence,
) -> Result<a::GfxRenderColorList, Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Unavailable);
    }
    let _bo = BoLock::acquire();
    let _ = unsafe { job_backend_locked(id, fence) }?;
    with_resources(|state, resources, _| resources.render_color_list(state, fence))
}

pub fn retain_native(
    identity: Owner,
    fence: model::Fence,
    which: u32,
) -> Result<RetainedResource, Error> {
    if irq_router::in_dispatch() || which > 1 {
        return Err(Error::Invalid);
    }
    buffers::lock_prepared(buffers::Need {
        objects: 0,
        references: 1,
        leases: 0,
    })?;
    let _bo = BoLock;
    unsafe { driver_memory::admit_locked(identity) }?;
    let backend = unsafe { &*backend_locked(fence.binding)? };
    if backend.owner != identity {
        return Err(Error::WrongOwner);
    }
    if backend.closing {
        return Err(Error::DeviceLost);
    }
    let reference = with_resources(|state, resources, buffers| {
        resources.retain(state, buffers, fence, which, identity)
    })?;
    Ok(RetainedResource {
        reference,
        buffer: unsafe { buffers::read() }
            .buffer_for(reference, identity)
            .expect("exact retained native reference"),
    })
}

pub fn complete_native(
    id: u32,
    fence: model::Fence,
    result: u32,
    quiesced: bool,
) -> Result<(), Error> {
    let instant = now();
    buffers::visibility();
    with_ingress(|ingress| ingress.acknowledge(id, fence, result, quiesced, instant))?;
    wake();
    Ok(())
}

pub fn retain_native_scanout(
    identity: Owner,
    fence: model::Fence,
) -> Result<RetainedResource, Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Invalid);
    }
    buffers::lock_prepared(buffers::Need {
        objects: 0,
        references: 1,
        leases: 0,
    })?;
    let _bo = BoLock;
    unsafe { driver_memory::admit_locked(identity) }?;
    let backend =
        unsafe { &*job_backend_locked(u32::try_from(identity.id).expect("driver id"), fence)? };
    if backend.owner != identity {
        return Err(Error::WrongOwner);
    }
    if backend.closing {
        return Err(Error::DeviceLost);
    }
    let reference = with_resources(|state, resources, buffers| {
        resources.retain_scanout(state, buffers, fence, identity)
    })?;
    Ok(RetainedResource {
        reference,
        buffer: unsafe { buffers::read() }
            .buffer_for(reference, identity)
            .expect("exact retained scanout"),
    })
}

pub fn begin_native_scanout(id: u32, fence: model::Fence) -> Result<(), Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Invalid);
    }
    let instant = now();
    {
        let _bo = BoLock::acquire();
        let _ = unsafe { job_backend_locked(id, fence) }?;
        let entry = unsafe { (*data()).resources[fence.slot as usize - 1] };
        if entry.fence != fence || entry.operation != resource::DIRECT_PRESENT {
            return Err(Error::Invalid);
        }
        unsafe { state() }.begin_scanout(fence, instant)?;
    }
    wake();
    Ok(())
}

pub fn native_scanout_retire_requested(id: u32, fence: model::Fence) -> Result<bool, Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Invalid);
    }
    let _bo = BoLock::acquire();
    let _ = unsafe { job_backend_locked(id, fence) }?;
    let entry = unsafe { (*data()).resources[fence.slot as usize - 1] };
    if entry.fence != fence || entry.operation != resource::DIRECT_PRESENT {
        return Err(Error::Invalid);
    }
    unsafe { state() }.scanout_retire_requested(fence)
}

pub(super) unsafe fn lose_locked(backend: *mut Backend, quiesced: bool, instant: u64) {
    unsafe {
        (*backend).closing = true;
    }
    let base = unsafe { ptr::addr_of_mut!((*data()).backends).cast::<Backend>() };
    let index = unsafe { backend.offset_from(base) } as usize;
    with_wakeups(|wakeups| wakeups.close(index));
    unsafe { state() }.device_lost(unsafe { (*backend).binding }, instant);
    if quiesced {
        for i in 0..FENCE_CAPACITY {
            let job = unsafe { (*data()).jobs[i] };
            let binding = unsafe { (*backend).binding };
            if job.fence.slot != 0
                && (job.active || job.scanout)
                && job.fence.binding.adapter == binding.adapter
                && job.fence.binding.device_generation == binding.device_generation
            {
                with_ingress(|ingress| ingress.quiesce(job.fence));
                unsafe { state() }
                    .complete(job.fence, model::FAILED, true, instant)
                    .expect("native exact stopped backend");
            }
        }
    }
}

pub fn reset_native(
    id: u32,
    binding: model::Binding,
    quiesced: bool,
) -> Result<model::Binding, Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Unavailable);
    }
    let instant = now();
    let next = {
        let _bo = BoLock::acquire();
        let backend = unsafe { backend_locked(binding)? };
        if id == 0 || unsafe { (*backend).owner.id } != u64::from(id) {
            return Err(Error::WrongOwner);
        }
        if unsafe { (*backend).binding.reset_generation } == u64::MAX {
            return Err(Error::Exhausted);
        }
        unsafe { lose_locked(backend, quiesced, instant) };
        if !quiesced {
            None
        } else {
            unsafe {
                (*backend).binding.reset_generation += 1;
                (*backend).properties = None;
                (*backend).display_timeline = 0;
                (*backend).closing = false;
            }
            let index = unsafe { backend.offset_from(ptr::addr_of_mut!((*data()).backends).cast()) }
                as usize;
            let binding = unsafe { (*backend).binding };
            with_wakeups(|wakeups| wakeups.bind(index, id, binding));
            Some(binding)
        }
    };
    wake();
    outputs::stopped_driver(id);
    next.ok_or(Error::Busy)
}

pub(super) unsafe fn retained_locked(backend: Backend) -> bool {
    if backend.notifying || backend.work_handle != 0 {
        return true;
    }
    unsafe { state() }.jobs.iter().any(|job| {
        job.fence.slot != 0
            && job.fence.binding.adapter == backend.binding.adapter
            && job.fence.binding.device_generation == backend.binding.device_generation
            && (job.active || job.resources_held)
    })
}

pub fn unregister_native(id: u32, binding: model::Binding, quiesced: bool) -> Result<(), Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Unavailable);
    }
    if id == 0 {
        return Err(Error::WrongOwner);
    }
    let instant = now();
    let busy = {
        let _bo = BoLock::acquire();
        let backend = unsafe { backend_locked(binding) }.map_err(|e| {
            if e == Error::Unsupported {
                Error::Stale
            } else {
                e
            }
        })?;
        if id == 0 || unsafe { (*backend).owner.id } != u64::from(id) {
            return Err(Error::WrongOwner);
        }
        unsafe { lose_locked(backend, quiesced, instant) };
        let busy = unsafe { retained_locked(backend.read()) };
        if !busy {
            unsafe { backend.write(Backend::default()) };
        }
        busy
    };
    wake();
    outputs::stopped_driver(id);
    if busy { Err(Error::Busy) } else { Ok(()) }
}

pub fn closing_driver(id: u32) {
    let instant = now();
    {
        let _bo = BoLock::acquire();
        let p = unsafe { data() };
        for i in 0..BACKEND_CAPACITY {
            if id != 0 && unsafe { (*p).backends[i].owner.id } == u64::from(id) {
                unsafe { lose_locked(ptr::addr_of_mut!((*p).backends[i]), false, instant) };
            }
        }
    }
    outputs::stopped_driver(id);
    wake()
}

pub fn retains_driver(id: u32) -> bool {
    let _bo = BoLock::acquire();
    unsafe { &*ptr::addr_of!((*data()).backends) }
        .iter()
        .any(|backend| id != 0 && backend.owner.id == u64::from(id))
}

pub fn wake_native(id: u32, binding: model::Binding) -> Result<(), Error> {
    with_wakeups(|wakeups| wakeups.request(id, binding))?;
    wake();
    Ok(())
}

pub fn wake_output(id: u32, target: a::GfxOutputTarget) -> Result<(), Error> {
    let binding = {
        let _bo = BoLock::acquire();
        unsafe { &*ptr::addr_of!((*data()).backends) }
            .iter()
            .find(|backend| {
                id != 0
                    && backend.owner.id == u64::from(id)
                    && !backend.closing
                    && backend.binding.adapter == target.adapter_id
                    && backend.binding.device_generation == target.device_generation
            })
            .map(|backend| backend.binding)
            .ok_or(Error::Stale)?
    };
    wake_native(id, binding)
}

pub(super) fn notify_native() -> bool {
    let mut pending = false;
    for i in 0..BACKEND_CAPACITY {
        let snapshot = {
            let _bo = BoLock::acquire();
            unsafe { (*data()).backends[i] }
        };
        if snapshot.owner.id == 0 {
            continue;
        }
        if snapshot.work_handle != 0 {
            let mut status = work::CompletionStatus::default();
            let rc =
                unsafe { work::r4os_rust_driver_work_status(snapshot.work_handle, &mut status) };
            if (rc != 0 || status.state == 3 || status.state == 4)
                && (rc != 0 || work::r4os_rust_driver_work_release(snapshot.work_handle) == 0)
            {
                let _bo = BoLock::acquire();
                let backend = unsafe { &mut (*data()).backends[i] };
                if backend.binding.device_generation == snapshot.binding.device_generation
                    && backend.work_handle == snapshot.work_handle
                {
                    backend.work_handle = 0;
                }
            }
            pending = true;
            continue;
        }
        let (ready, selected) = {
            let _bo = BoLock::acquire();
            let backend = unsafe { &mut (*data()).backends[i] };
            let mut ready = false;
            if backend.owner.id != 0
                && !backend.closing
                && !backend.notifying
                && backend.work_handle == 0
            {
                {
                    let _runtime = RuntimeLock::acquire();
                    let mut wakeups = ingress::Wakeups {
                        entries: unsafe { &mut *WAKEUPS.0.get() },
                    };
                    ready = wakeups.take(i) || backend.lifecycle_dirty;
                    backend.lifecycle_dirty = false;
                }
                let model = unsafe { state() };
                for job in model.jobs.iter() {
                    if job.fence.slot != 0
                        && (job.phase == model::QUEUED
                            || (job.scanout
                                && (job.retire_requested
                                    || !job.client_reference
                                    || model.queues[job.queue].closing)))
                        && job.fence.binding == backend.binding
                    {
                        ready = true;
                        break;
                    }
                }
            }
            if ready {
                backend.notifying = true;
            }
            (ready, *backend)
        };
        if !ready {
            continue;
        }
        let mut handle = 0;
        let rc = unsafe {
            work::r4os_rust_driver_work_submit(
                u32::try_from(selected.owner.id).expect("driver id"),
                notify_driver,
                i,
                0,
                false,
                &mut handle,
            )
        };
        {
            let _bo = BoLock::acquire();
            let backend = unsafe { &mut (*data()).backends[i] };
            assert_eq!(
                backend.binding.device_generation,
                selected.binding.device_generation
            );
            backend.work_handle = if rc == 0 { handle } else { 0 };
            backend.notifying = false;
            if rc != 0 && !backend.closing {
                let _ = with_wakeups(|wakeups| {
                    wakeups.request(
                        u32::try_from(selected.owner.id).expect("driver id"),
                        selected.binding,
                    )
                });
            }
        }
        pending = true;
    }
    pending
}

pub(super) unsafe extern "C" fn notify_driver(index: usize) -> i32 {
    if index >= BACKEND_CAPACITY {
        return -1;
    }
    let id = work::r4os_rust_driver_work_current();
    if id == 0 {
        return -1;
    }
    if !api_admission::r4os_rust_driver_api_enter(id, u64::from(timer::frequency().max(1)) * 3) {
        let retry = {
            let _bo = BoLock::acquire();
            unsafe { (*data()).backends[index] }
        };
        if retry.owner.id == u64::from(id) && !retry.closing {
            let _ = wake_native(id, retry.binding);
        }
        return -1;
    }
    let _owner = DriverOwner;
    let selected = {
        let _bo = BoLock::acquire();
        unsafe { (*data()).backends[index] }
    };
    if selected.owner.id != u64::from(id) || selected.closing || selected.notify.is_none() {
        return 0;
    }
    unsafe { selected.notify.expect("exact queue driver callback")(selected.context) }
}
