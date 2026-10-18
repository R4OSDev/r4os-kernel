//! Exact jobs operations of the parent owner; no duplicated state.
use super::*;

pub fn submit(
    owner: Owner,
    timeline: u64,
    request: &resource::Submission<'_>,
    transport: &resource::Request,
) -> Result<model::Status, Error> {
    if matches!(
        transport.operation,
        resource::NATIVE | resource::PRESENT | resource::DIRECT_PRESENT
    ) {
        return Err(Error::Unsupported);
    }
    submit_impl(owner, timeline, request, transport, None)
}

/// # Safety
/// The exact R4X unwind pin retains all command/resource input ranges until
/// the native job copied them; later execution owns only resident metadata.
pub unsafe fn submit_native(
    owner: Owner,
    timeline: u64,
    request: &resource::Submission<'_>,
    input: a::GfxNativeSubmission,
) -> Result<model::Status, Error> {
    if !available() || irq_router::in_dispatch() {
        return Err(Error::Unavailable);
    }
    let job = unsafe { queue_native::Job::create(input) }?;
    let mut constructed = ConstructedJob {
        pointer: job,
        destroy: true,
    };
    let _wake = WakeOnDrop;
    let instant = now();
    buffers::lock_prepared(buffers::Need {
        objects: 0,
        references: 0,
        leases: unsafe { (*job).resource_count },
    })?;
    let _bo = BoLock;
    let config = unsafe { state() }.configuration(timeline, owner)?;
    if config.binding.adapter == 0 {
        return Err(Error::Unsupported);
    }
    let backend = unsafe { &*backend_locked(config.binding)? };
    if backend.closing {
        return Err(Error::DeviceLost);
    }
    let info = unsafe { (*job).info };
    if backend.operations & (1 << a::gfx_queue_operation_native) == 0
        || info.interface_id_lo != backend.profile.interface_id_lo
        || info.interface_id_hi != backend.profile.interface_id_hi
        || info.revision != backend.profile.revision
    {
        return Err(Error::Unsupported);
    }
    let binding = crate::gfx::layout::Binding {
        adapter: backend.binding.adapter,
        driver_owner: u32::try_from(backend.owner.id).expect("driver id"),
        device_generation: backend.memory_generation,
    };
    let accepted = (|| {
        // The job's acquisition calls the canonical BO/VA owners itself. No
        // second mutable BO-table view spans those native metadata operations.
        {
            let mut seen = [false; FENCE_CAPACITY];
            let mut pending = [MaybeUninit::<u16>::uninit(); FENCE_CAPACITY];
            let state = unsafe { state() };
            let mut resources = unsafe { resources(&mut seen, &mut pending) };
            unsafe {
                (&mut *job).acquire_locked(
                    owner,
                    binding,
                    &state,
                    &mut resources,
                    timeline,
                    request.dependencies,
                )
            }?;
        }
        let transport = resource::Request {
            operation: resource::NATIVE,
            ..resource::Request::default()
        };
        unsafe { submit_locked(owner, timeline, request, &transport, None, instant) }
    })();
    let accepted = match accepted {
        Ok(value) => value,
        Err(error) => {
            unsafe { (&mut *job).rollback_locked() };
            return Err(error);
        }
    };
    let index = accepted.fence.slot as usize - 1;
    let p = unsafe { data() };
    assert!(unsafe { (*p).native_jobs[index].is_null() });
    unsafe {
        (*p).resources[index].native_uses = (*job).uses;
        (*p).resources[index].native_uses_len = (*job).resource_count;
        (*p).native_jobs[index] = job;
    }
    constructed.destroy = false;
    Ok(accepted)
}

pub fn submit_display_image(
    owner: Owner,
    timeline: u64,
    request: &resource::Submission<'_>,
    transport: &resource::Request,
    binding: model::Binding,
) -> Result<model::Status, Error> {
    if transport.operation != resource::PRESENT && transport.operation != resource::DIRECT_PRESENT {
        return Err(Error::Invalid);
    }
    submit_impl(owner, timeline, request, transport, Some(binding))
}

pub(super) fn submit_impl(
    owner: Owner,
    timeline: u64,
    request: &resource::Submission<'_>,
    transport: &resource::Request,
    output_binding: Option<model::Binding>,
) -> Result<model::Status, Error> {
    if !available() || irq_router::in_dispatch() {
        return Err(Error::Unavailable);
    }
    let instant = now();
    let snapshot = {
        buffers::lock_prepared(buffers::Need {
            objects: 0,
            references: 0,
            leases: 2,
        })?;
        let _bo = BoLock;
        unsafe { submit_locked(owner, timeline, request, transport, output_binding, instant) }?
    };
    wake();
    Ok(snapshot)
}

/// # Safety
/// Shared BO owner already admits additional target validation and submission;
/// input and prepared lease metadata stay retained without callbacks or waits.
pub unsafe fn submit_output_locked(
    owner: Owner,
    timeline: u64,
    request: &resource::Submission<'_>,
    transport: &resource::Request,
    binding: model::Binding,
) -> Result<model::Status, Error> {
    if !available() || irq_router::in_dispatch() {
        return Err(Error::Unavailable);
    }
    if (transport.operation != resource::PRESENT && transport.operation != resource::DIRECT_PRESENT)
        || transport.display_target.connector_id == 0
    {
        return Err(Error::Invalid);
    }
    unsafe { submit_locked(owner, timeline, request, transport, Some(binding), now()) }
}

/// # Safety
/// Exact query admitted by the shared BO owner.
pub unsafe fn query_locked(fence: model::Fence) -> Result<model::Status, Error> {
    unsafe { state() }.query(fence)
}

pub(super) unsafe fn submit_locked(
    owner: Owner,
    timeline: u64,
    request: &resource::Submission<'_>,
    transport: &resource::Request,
    output_binding: Option<model::Binding>,
    instant: u64,
) -> Result<model::Status, Error> {
    let _output = if transport.display_target.connector_id != 0 {
        Some(ownership::enter_state())
    } else {
        None
    };
    let config = unsafe { state() }.configuration(timeline, owner)?;
    if output_binding.is_some_and(|binding| binding != config.binding) {
        return Err(Error::Stale);
    }
    let backend = if config.binding.adapter == 0 {
        None
    } else {
        Some(unsafe { backend_locked(config.binding)?.read() })
    };
    if transport.display_target.connector_id != 0 {
        let native = backend.ok_or(Error::Unsupported)?;
        outputs::validate_target_locked(
            u32::try_from(native.owner.id).expect("driver id"),
            transport.display_target,
        )?;
    }
    let operations = backend.map_or(11, |native| native.operations);
    assert!(transport.operation < 64, "queue operation shift");
    if operations & (1u64 << transport.operation) == 0 {
        return Err(Error::Unsupported);
    }
    if transport.operation == resource::UPLOAD {
        if owner != resource::DISPLAY_OWNER {
            return Err(Error::Unsupported);
        }
        let native = backend.ok_or(Error::Unsupported)?;
        if native.closing || native.display_timeline != timeline {
            return Err(Error::Unsupported);
        }
    }
    let mut retained = *transport;
    retained.has_memory = backend.is_some();
    retained.memory = backend.map_or(crate::gfx::layout::Binding::default(), |native| {
        crate::gfx::layout::Binding {
            adapter: native.binding.adapter,
            driver_owner: u32::try_from(native.owner.id).expect("driver id"),
            device_generation: native.memory_generation,
        }
    });
    let accepted = with_resources(|state, resources, bo| {
        resources.submit(state, bo, timeline, owner, request, &retained, instant)
    })?;
    let p = unsafe { data() };
    let index = accepted.slot as usize - 1;
    unsafe {
        ptr::addr_of_mut!((*p).completions[index]).write(event(false));
        ptr::addr_of_mut!((*p).releases[index]).write(event(false));
    }
    buffers::visibility();
    Ok(unsafe { state() }
        .query(accepted)
        .expect("exact accepted queue fence"))
}

pub fn query(fence: model::Fence) -> Result<model::Status, Error> {
    let _bo = BoLock::acquire();
    unsafe { state() }.query(fence)
}

pub fn cancel(owner: Owner, fence: model::Fence) -> Result<(), Error> {
    let instant = now();
    {
        let _bo = BoLock::acquire();
        unsafe { state() }.cancel(fence, owner, instant)?;
    }
    wake();
    Ok(())
}

pub fn close(owner: Owner, timeline: u64) -> Result<(), Error> {
    let instant = now();
    {
        let _bo = BoLock::acquire();
        let binding = unsafe { state() }
            .queues
            .iter()
            .find(|queue| queue.timeline == timeline && queue.owner == owner)
            .map(|queue| queue.config.binding);
        if let Some(binding) = binding {
            unsafe { queue_closing_locked(binding) };
        }
        unsafe { state() }.close(timeline, owner, instant)?;
    }
    wake();
    Ok(())
}

pub fn drop_reference(owner: Owner, fence: model::Fence) -> Result<(), Error> {
    {
        let _bo = BoLock::acquire();
        unsafe { state() }.drop_reference(fence, owner)?;
    }
    wake();
    Ok(())
}

pub fn stopped(owner: Owner) {
    let instant = now();
    {
        let _bo = BoLock::acquire();
        for i in 0..QUEUE_CAPACITY {
            let queue = unsafe { (*data()).queues[i] };
            if queue.timeline != 0 && queue.owner == owner {
                unsafe { queue_closing_locked(queue.config.binding) };
            }
        }
        unsafe { state() }.stopped(owner, instant);
    }
    wake()
}

pub fn retired_task(id: u32, generation: u64) {
    {
        let _bo = BoLock::acquire();
        for i in 0..256 {
            let waiter = unsafe { (*data()).waiters[i] };
            if waiter.task_id == id && waiter.generation == generation {
                unsafe { state() }
                    .release_waiter(waiter.fence)
                    .expect("retired task exact fence wait");
                unsafe {
                    (*data()).waiters[i] = Waiter::default();
                }
            }
        }
    }
    wake()
}

pub(super) fn wait_satisfied(status: model::Status, wait_for: u32) -> bool {
    status.phase == model::TERMINAL && (wait_for == 0 || !status.resources_held)
}

pub fn wait(
    fence: model::Fence,
    timeout_ticks: u64,
    wait_for: u32,
) -> Result<model::Status, Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Unavailable);
    }
    assert!(wait_for <= 1, "queue wait enum");
    let current = scheduler::current();
    if current.is_null() {
        return Err(Error::Unavailable);
    }
    let (slot, event) = {
        let _bo = BoLock::acquire();
        let status = unsafe { state() }.query(fence)?;
        if wait_satisfied(status, wait_for) {
            return Ok(status);
        }
        let p = unsafe { data() };
        let index = unsafe { &*ptr::addr_of!((*p).waiters) }
            .iter()
            .position(|waiter| waiter.task_id == 0)
            .ok_or(Error::Capacity)?;
        unsafe { state() }.retain_waiter(fence)?;
        let (task_id, generation) = unsafe { task::immutable_identity(current) };
        unsafe {
            (*p).waiters[index] = Waiter {
                task_id,
                generation,
                fence,
            };
        }
        let event = if wait_for == 0 {
            unsafe { ptr::addr_of_mut!((*p).completions[fence.slot as usize - 1]) }
        } else {
            unsafe { ptr::addr_of_mut!((*p).releases[fence.slot as usize - 1]) }
        };
        (index, event)
    };
    let result = unsafe { sync_objects::r4os_rust_sync_event(event, 2, timeout_ticks) };
    let status = {
        let _bo = BoLock::acquire();
        let snapshot = unsafe { state() }
            .query(fence)
            .expect("exact live queued wait");
        unsafe { state() }
            .release_waiter(fence)
            .expect("exact queued waiter return");
        unsafe {
            (*data()).waiters[slot] = Waiter::default();
        }
        snapshot
    };
    wake();
    if wait_satisfied(status, wait_for) {
        Ok(status)
    } else {
        Err(if result == 2 {
            Error::WaitTimeout
        } else {
            Error::WaitCancelled
        })
    }
}
