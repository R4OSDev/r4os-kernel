//! Exact backends operations of the parent owner; no duplicated state.
use super::*;

pub fn validated_profile(mut input: a::GfxBackendProfile) -> Result<a::GfxBackendProfile, Error> {
    if input.version != 1
        || input.size < size_of::<a::GfxBackendProfile>() as u32
        || input.data_bytes as usize > input.data.len()
    {
        return Err(Error::Invalid);
    }
    let empty = input.interface_id_lo == 0 && input.interface_id_hi == 0;
    if (empty && (input.revision != 0 || input.data_bytes != 0)) || (!empty && input.revision == 0)
    {
        return Err(Error::Invalid);
    }
    if input.data[input.data_bytes as usize..]
        .iter()
        .any(|&byte| byte != 0)
    {
        return Err(Error::Invalid);
    }
    input.size = size_of::<a::GfxBackendProfile>() as u32;
    Ok(input)
}

pub fn validated_properties(
    mut input: a::GfxBackendProperties,
) -> Result<a::GfxBackendProperties, Error> {
    if input.version != 1
        || input.size < size_of::<a::GfxBackendProperties>() as u32
        || (input.interface_id_lo == 0 && input.interface_id_hi == 0)
        || input.revision == 0
        || input.data_bytes == 0
        || input.data_bytes as usize > input.data.len()
    {
        return Err(Error::Invalid);
    }
    if input.data[input.data_bytes as usize..]
        .iter()
        .any(|&byte| byte != 0)
    {
        return Err(Error::Invalid);
    }
    input.size = size_of::<a::GfxBackendProperties>() as u32;
    Ok(input)
}

pub fn publish_native_properties(
    identity: Owner,
    binding: model::Binding,
    milestone: u32,
    input: a::GfxBackendProperties,
) -> Result<(), Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Invalid);
    }
    let value = validated_properties(input)?;
    let _bo = BoLock::acquire();
    let backend = unsafe { &mut *backend_locked(binding)? };
    if backend.owner != identity {
        return Err(Error::WrongOwner);
    }
    if backend.closing {
        return Err(Error::DeviceLost);
    }
    if backend.milestone != milestone {
        return Err(Error::Invalid);
    }
    if backend.properties.is_some() {
        return Err(Error::Busy);
    }
    backend.properties = Some(value);
    Ok(())
}

pub fn backend_properties(
    binding: model::Binding,
    milestone: u32,
) -> Result<Option<a::GfxBackendProperties>, Error> {
    let _bo = BoLock::acquire();
    let backend = unsafe { &*backend_locked(binding)? };
    if backend.closing {
        return Err(Error::DeviceLost);
    }
    if backend.milestone != milestone {
        return Err(Error::Invalid);
    }
    Ok(backend.properties)
}

pub fn register_native(identity: Owner, config: NativeConfig) -> Result<model::Binding, Error> {
    if !available() || irq_router::in_dispatch() {
        return Err(Error::Unavailable);
    }
    assert!(config.milestone <= 2, "queue milestone enum");
    if identity.kind != 2
        || !identity.valid()
        || config.adapter == 0
        || config.milestone == 0
        || config.operations == 0
        || config.operations & !4095 != 0
    {
        return Err(Error::Invalid);
    }
    let profile = validated_profile(config.profile)?;
    if config.operations & (1 << a::gfx_queue_operation_native) != 0
        && profile.interface_id_lo == 0
        && profile.interface_id_hi == 0
    {
        return Err(Error::Unsupported);
    }
    let _bo = BoLock::acquire();
    let p = unsafe { data() };
    if unsafe { (*p).backend_serial } == u64::MAX {
        return Err(Error::Exhausted);
    }
    if unsafe { &*ptr::addr_of!((*p).backends) }
        .iter()
        .any(|backend| backend.owner.id != 0 && backend.binding.adapter == config.adapter)
    {
        return Err(Error::Busy);
    }
    for i in 0..BACKEND_CAPACITY {
        if unsafe { (*p).backends[i].owner.id } == 0 {
            unsafe {
                (*p).backend_serial += 1;
            }
            let serial = unsafe { (*p).backend_serial };
            let binding = model::Binding {
                adapter: config.adapter,
                device_generation: serial,
                reset_generation: 1,
            };
            unsafe {
                ptr::addr_of_mut!((*p).backends[i]).write(Backend {
                    owner: identity,
                    binding,
                    milestone: config.milestone,
                    notify: Some(config.notify),
                    context: config.context,
                    profile,
                    operations: config.operations,
                    job_operations: config.operations,
                    memory_generation: if config.memory_generation != 0 {
                        config.memory_generation
                    } else {
                        serial
                    },
                    ..Backend::default()
                });
            }
            with_wakeups(|wakeups| {
                wakeups.bind(i, u32::try_from(identity.id).expect("driver id"), binding)
            });
            return Ok(binding);
        }
    }
    Err(Error::Capacity)
}

pub fn native_milestone(id: u32, binding: model::Binding) -> Result<u32, Error> {
    let _bo = BoLock::acquire();
    let backend = unsafe { &*backend_locked(binding)? };
    if id == 0 || backend.owner.id != u64::from(id) {
        return Err(Error::WrongOwner);
    }
    Ok(backend.milestone)
}

pub fn native_operations(id: u32, binding: model::Binding) -> Result<u64, Error> {
    let _bo = BoLock::acquire();
    let backend = unsafe { &*backend_locked(binding)? };
    if id == 0 || backend.owner.id != u64::from(id) {
        return Err(Error::WrongOwner);
    }
    Ok(backend.operations)
}

pub fn native_queue_owner_info(
    id: u32,
    binding: model::Binding,
    timeline: u64,
) -> Result<Option<a::GfxQueueOwnerInfo>, Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Unavailable);
    }
    if timeline == 0 {
        return Err(Error::Invalid);
    }
    let _bo = BoLock::acquire();
    let backend = unsafe { &*backend_locked(binding)? };
    if id == 0 || backend.owner.id != u64::from(id) {
        return Err(Error::WrongOwner);
    }
    if backend.closing {
        return Err(Error::DeviceLost);
    }
    for queue in unsafe { state() }.queues.iter() {
        if queue.timeline == timeline {
            if queue.config.binding != binding {
                return Err(Error::Stale);
            }
            return Ok(Some(a::GfxQueueOwnerInfo {
                timeline,
                producer_kind: queue.owner.kind + 1,
                closing: u32::from(queue.closing),
                producer_id: queue.owner.id,
                producer_generation: queue.owner.generation,
                inflight_jobs: queue.inflight,
                retained_jobs: queue.jobs,
                ..a::GfxQueueOwnerInfo::default()
            }));
        }
    }
    Ok(None)
}

pub(super) unsafe fn queue_closing_locked(binding: model::Binding) {
    let p = unsafe { data() };
    for backend in unsafe { &mut *ptr::addr_of_mut!((*p).backends) } {
        if backend.owner.id != 0 && !backend.closing && backend.binding == binding {
            backend.lifecycle_dirty = true;
            return;
        }
    }
}

pub(super) fn native_job_capacity(backend: &Backend) -> u32 {
    if backend.job_operations & (1 << a::gfx_queue_operation_native) != 0 {
        return size_of::<a::GfxDriverJob>() as u32;
    }
    if backend.target_jobs {
        272
    } else if backend.job_operations & 976 != 0 {
        224
    } else if backend.job_operations & 40 != 0 {
        136
    } else {
        112
    }
}

/// # Safety
/// Shared BO owner retained through additional-output admission; no work/wait.
pub unsafe fn require_output_targets_locked(
    identity: Owner,
    binding: model::Binding,
    job_size: u32,
) -> Result<(), Error> {
    if job_size != 272 && job_size != size_of::<a::GfxDriverJob>() as u32 {
        return Err(Error::Invalid);
    }
    let backend = unsafe { &mut *backend_locked(binding)? };
    if backend.owner != identity {
        return Err(Error::WrongOwner);
    }
    if backend.closing {
        return Err(Error::DeviceLost);
    }
    if backend.operations & (1 << a::gfx_queue_operation_present) == 0 {
        return Err(Error::Unsupported);
    }
    backend.target_jobs = true;
    Ok(())
}

/// # Safety
/// Shared BO owner admits this exact nonblocking resource/state query.
pub unsafe fn output_busy_locked(target: a::GfxOutputTarget) -> bool {
    let p = unsafe { data() };
    for entry in unsafe { &*ptr::addr_of!((*p).resources) } {
        if entry.fence.slot != 0 && output_target::same(&entry.display_target, &target) {
            let Ok(status) = (unsafe { state() }).query(entry.fence) else {
                continue;
            };
            if status.phase != model::TERMINAL || status.device_active || status.resources_held {
                return true;
            }
        }
    }
    false
}

pub fn update_native_operations(
    id: u32,
    binding: model::Binding,
    operations: u64,
) -> Result<(), Error> {
    if irq_router::in_dispatch() || operations == 0 || operations & !4095 != 0 {
        return Err(Error::Invalid);
    }
    let _bo = BoLock::acquire();
    let backend = unsafe { &mut *backend_locked(binding)? };
    if id == 0 || backend.owner.id != u64::from(id) {
        return Err(Error::WrongOwner);
    }
    if backend.closing {
        return Err(Error::DeviceLost);
    }
    if operations & (1 << a::gfx_queue_operation_native) != 0
        && backend.profile.interface_id_lo == 0
        && backend.profile.interface_id_hi == 0
    {
        return Err(Error::Unsupported);
    }
    backend.operations = operations;
    backend.job_operations |= operations;
    Ok(())
}

pub fn bind_display_queue(id: u32, binding: model::Binding, timeline: u64) -> Result<(), Error> {
    let _bo = BoLock::acquire();
    let backend = unsafe { &mut *backend_locked(binding)? };
    if id == 0 || backend.owner.id != u64::from(id) {
        return Err(Error::WrongOwner);
    }
    if backend.closing || backend.display_timeline != 0 {
        return Err(Error::Busy);
    }
    let config = unsafe { state() }.configuration(timeline, resource::DISPLAY_OWNER)?;
    if config.binding != binding || config.milestone != 1 {
        return Err(Error::Invalid);
    }
    backend.display_timeline = timeline;
    Ok(())
}

pub fn unbind_display_queue(id: u32, binding: model::Binding, timeline: u64) {
    let _bo = BoLock::acquire();
    let Ok(backend) = (unsafe { backend_locked(binding) }) else {
        return;
    };
    let backend = unsafe { &mut *backend };
    if id != 0 && backend.owner.id == u64::from(id) && backend.display_timeline == timeline {
        backend.display_timeline = 0;
    }
}

pub fn validate_output_binding(id: u32, input: a::GfxBackendBinding) -> Result<(), Error> {
    if input.version != 1
        || input.size < size_of::<a::GfxBackendBinding>() as u32
        || irq_router::in_dispatch()
    {
        return Err(Error::Invalid);
    }
    let _bo = BoLock::acquire();
    let backend = unsafe {
        &*backend_locked(model::Binding {
            adapter: input.adapter_id,
            device_generation: input.device_generation,
            reset_generation: input.reset_generation,
        })?
    };
    if id == 0 || backend.owner.id != u64::from(id) {
        return Err(Error::WrongOwner);
    }
    if backend.closing {
        return Err(Error::DeviceLost);
    }
    if backend.milestone != input.milestone {
        return Err(Error::Invalid);
    }
    Ok(())
}
