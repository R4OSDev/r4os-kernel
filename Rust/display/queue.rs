//! Canonical graphics queue runtime. One BO mutex owns queue/resource/backend
//! metadata; runtime-only ingress admits IRQ receipts. Worker callbacks, waits,
//! allocation and physical destruction retain their original unlocked edges.
#![allow(clippy::too_many_arguments)]
use super::{
    error::Error,
    output_target, outputs,
    ownership::{self, Resident},
    queue_ingress as ingress, queue_native, queue_resources as resource, queue_state as model,
};
use crate::{
    arch::runtime,
    contract as a,
    driver::{api_admission, irq_router, work},
    gfx::{
        allocations, buffer_owner, buffers, driver_memory,
        types::{Handle, Owner},
        virtual_runtime,
    },
    platform::{monotonic, timer},
    sched::{
        scheduler, sync_objects,
        sync_types::{Event, WaitQueue},
        task::{self, task_api},
    },
    service::desktop_events,
};
use core::{
    cell::UnsafeCell,
    ffi::c_void,
    mem::{MaybeUninit, size_of},
    ptr, slice,
    sync::atomic::{AtomicBool, Ordering},
};
pub const QUEUE_CAPACITY: usize = 32;
pub const FENCE_CAPACITY: usize = 128;
const BACKEND_CAPACITY: usize = 16;
const COPY_SLICE_BYTES: u64 = 64 * 1024;
const SOFTWARE: model::Binding = model::Binding {
    adapter: 0,
    device_generation: 1,
    reset_generation: 1,
};
const EMPTY_FENCE: model::Fence = model::Fence {
    slot: 0,
    timeline: 0,
    point: 0,
    binding: SOFTWARE,
};
const EMPTY_INGRESS: ingress::Entry = ingress::Entry {
    fence: EMPTY_FENCE,
    owner: 0,
    active: false,
    pending: false,
    result: 0,
    instant: 0,
};
const EMPTY_WAKEUP: ingress::WakeEntry = ingress::WakeEntry {
    owner: 0,
    binding: SOFTWARE,
    pending: false,
};
static INGRESS: Resident<[ingress::Entry; FENCE_CAPACITY]> =
    Resident(UnsafeCell::new([EMPTY_INGRESS; FENCE_CAPACITY]));
static WAKEUPS: Resident<[ingress::WakeEntry; BACKEND_CAPACITY]> =
    Resident(UnsafeCell::new([EMPTY_WAKEUP; BACKEND_CAPACITY]));
const fn event(auto: bool) -> Event {
    Event {
        signaled: false,
        mode: auto as u8,
        queue: WaitQueue::EMPTY,
    }
}
static WORKER_EVENT: Resident<Event> = Resident(UnsafeCell::new(event(true)));
static STARTED: AtomicBool = AtomicBool::new(false);
#[derive(Clone, Copy, Default)]
struct Waiter {
    task_id: u32,
    generation: u64,
    fence: model::Fence,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NativeConfig {
    pub adapter: u32,
    pub milestone: u32,
    pub notify: work::Handler,
    pub context: usize,
    pub profile: a::GfxBackendProfile,
    pub operations: u64,
    pub memory_generation: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BackendInfo {
    pub binding: model::Binding,
    pub milestone: u32,
    pub profile: a::GfxBackendProfile,
    pub operations: u64,
    pub memory_generation: u64,
}
#[derive(Clone, Copy)]
struct Backend {
    owner: Owner,
    binding: model::Binding,
    milestone: u32,
    notify: Option<work::Handler>,
    context: usize,
    closing: bool,
    notifying: bool,
    lifecycle_dirty: bool,
    work_handle: u32,
    display_timeline: u64,
    profile: a::GfxBackendProfile,
    properties: Option<a::GfxBackendProperties>,
    operations: u64,
    job_operations: u64,
    target_jobs: bool,
    memory_generation: u64,
}
impl Default for Backend {
    fn default() -> Self {
        Self {
            owner: Owner {
                kind: 2,
                id: 0,
                generation: 0,
            },
            binding: SOFTWARE,
            milestone: 1,
            notify: None,
            context: 0,
            closing: false,
            notifying: false,
            lifecycle_dirty: false,
            work_handle: 0,
            display_timeline: 0,
            profile: a::GfxBackendProfile::default(),
            properties: None,
            operations: 7,
            job_operations: 7,
            target_jobs: false,
            memory_generation: 0,
        }
    }
}
struct Runtime {
    queues: [model::Queue; QUEUE_CAPACITY],
    jobs: [model::Job; FENCE_CAPACITY],
    scalars: model::Scalars,
    resources: [resource::Entry; FENCE_CAPACITY],
    lists: [a::GfxRenderList; FENCE_CAPACITY],
    grids: [[a::GfxSampleGrid; a::gfx_render_list_capacity as usize]; FENCE_CAPACITY],
    colors: [a::GfxRenderColorProgram; FENCE_CAPACITY],
    native_jobs: [*mut queue_native::Job; FENCE_CAPACITY],
    completions: [Event; FENCE_CAPACITY],
    releases: [Event; FENCE_CAPACITY],
    next_copy: usize,
    waiters: [Waiter; 256],
    backends: [Backend; BACKEND_CAPACITY],
    backend_serial: u64,
}
static STORAGE: Resident<MaybeUninit<Runtime>> = Resident(UnsafeCell::new(MaybeUninit::uninit()));
static READY: Resident<bool> = Resident(UnsafeCell::new(false));
// Shared BO owner is mandatory even for the first fieldwise initialization.
// No native reference to the complete large runtime is ever materialized.
unsafe fn data() -> *mut Runtime {
    let p = STORAGE.0.get().cast::<Runtime>();
    if !unsafe { READY.0.get().read() } {
        unsafe {
            for i in 0..QUEUE_CAPACITY {
                ptr::addr_of_mut!((*p).queues)
                    .cast::<model::Queue>()
                    .add(i)
                    .write(model::Queue::default());
            }
            for i in 0..FENCE_CAPACITY {
                ptr::addr_of_mut!((*p).jobs)
                    .cast::<model::Job>()
                    .add(i)
                    .write(model::Job::default());
                ptr::addr_of_mut!((*p).resources)
                    .cast::<resource::Entry>()
                    .add(i)
                    .write(resource::Entry::default());
                ptr::addr_of_mut!((*p).lists)
                    .cast::<a::GfxRenderList>()
                    .add(i)
                    .write(a::GfxRenderList::default());
                let grids = ptr::addr_of_mut!((*p).grids)
                    .cast::<a::GfxSampleGrid>()
                    .add(i * a::gfx_render_list_capacity as usize);
                for j in 0..a::gfx_render_list_capacity as usize {
                    grids.add(j).write(a::GfxSampleGrid::default());
                }
                ptr::addr_of_mut!((*p).colors)
                    .cast::<a::GfxRenderColorProgram>()
                    .add(i)
                    .write(a::GfxRenderColorProgram::default());
                ptr::addr_of_mut!((*p).native_jobs)
                    .cast::<*mut queue_native::Job>()
                    .add(i)
                    .write(ptr::null_mut());
                ptr::addr_of_mut!((*p).completions)
                    .cast::<Event>()
                    .add(i)
                    .write(event(false));
                ptr::addr_of_mut!((*p).releases)
                    .cast::<Event>()
                    .add(i)
                    .write(event(false));
            }
            ptr::addr_of_mut!((*p).scalars).write(model::Scalars::default());
            ptr::addr_of_mut!((*p).next_copy).write(0);
            for i in 0..256 {
                ptr::addr_of_mut!((*p).waiters)
                    .cast::<Waiter>()
                    .add(i)
                    .write(Waiter::default());
            }
            for i in 0..BACKEND_CAPACITY {
                ptr::addr_of_mut!((*p).backends)
                    .cast::<Backend>()
                    .add(i)
                    .write(Backend::default());
            }
            ptr::addr_of_mut!((*p).backend_serial).write(1);
            READY.0.get().write(true);
        }
    }
    p
}
unsafe fn state<'a>() -> model::Table<'a> {
    let p = unsafe { data() };
    unsafe {
        model::RawView {
            queues: ptr::addr_of_mut!((*p).queues).cast(),
            queues_len: QUEUE_CAPACITY,
            jobs: ptr::addr_of_mut!((*p).jobs).cast(),
            jobs_len: FENCE_CAPACITY,
            scalars: ptr::addr_of_mut!((*p).scalars),
        }
        .borrow()
    }
}
unsafe fn resources<'a>(
    seen: &'a mut [bool],
    pending: &'a mut [MaybeUninit<u16>],
) -> resource::Table<'a> {
    let p = unsafe { data() };
    unsafe {
        resource::RawView {
            entries: ptr::addr_of_mut!((*p).resources).cast(),
            len: FENCE_CAPACITY,
            lists: ptr::addr_of_mut!((*p).lists).cast(),
            grids: ptr::addr_of_mut!((*p).grids).cast(),
            colors: ptr::addr_of_mut!((*p).colors).cast(),
        }
        .borrow(seen, pending)
    }
}
fn with_resources<R>(
    f: impl FnOnce(&mut model::Table<'_>, &mut resource::Table<'_>, &mut buffer_owner::Table<'_>) -> R,
) -> R {
    let mut seen = [false; FENCE_CAPACITY];
    let mut pending = [MaybeUninit::<u16>::uninit(); FENCE_CAPACITY];
    let mut state = unsafe { state() };
    let mut resources = unsafe { resources(&mut seen, &mut pending) };
    let mut bo = unsafe { buffers::table() };
    f(&mut state, &mut resources, &mut bo)
}
struct BoLock;
impl BoLock {
    fn acquire() -> Self {
        buffers::lock();
        Self
    }
}
impl Drop for BoLock {
    fn drop(&mut self) {
        buffers::unlock()
    }
}
struct RuntimeLock(u64);
impl RuntimeLock {
    fn acquire() -> Self {
        Self(runtime::save_runtime())
    }
}
impl Drop for RuntimeLock {
    fn drop(&mut self) {
        runtime::restore_runtime(self.0)
    }
}
fn with_ingress<R>(f: impl FnOnce(&mut ingress::Ingress<'_>) -> R) -> R {
    let _runtime = RuntimeLock::acquire();
    let mut value = ingress::Ingress {
        entries: unsafe { &mut *INGRESS.0.get() },
    };
    f(&mut value)
}
fn with_wakeups<R>(f: impl FnOnce(&mut ingress::Wakeups<'_>) -> R) -> R {
    let _runtime = RuntimeLock::acquire();
    let mut value = ingress::Wakeups {
        entries: unsafe { &mut *WAKEUPS.0.get() },
    };
    f(&mut value)
}
pub fn init() -> bool {
    if STARTED.load(Ordering::Acquire) {
        return true;
    }
    let request = task_api::CreateRequest {
        name: b"gfx-work".as_ptr(),
        name_len: 8,
        state: 1,
        role: 1,
        needs_fpu: true,
        smp_eligible: false,
        smp_r4x: false,
        owner_kind: 0,
        owner_context: ptr::null_mut(),
        entry: Some(worker_main),
    };
    let mut failure = 0;
    if unsafe { task_api::r4os_rust_task_create(&request, false, &mut failure) }.is_null() {
        return false;
    }
    STARTED.store(true, Ordering::Release);
    true
}
pub fn available() -> bool {
    STARTED.load(Ordering::Acquire)
}
pub fn wake() {
    unsafe { sync_objects::event_signal(WORKER_EVENT.0.get()) }
}
fn now() -> u64 {
    monotonic::now_nanoseconds().unwrap_or(0)
}
pub fn backend_at(index: u32) -> Option<BackendInfo> {
    if !available() || index as usize > BACKEND_CAPACITY {
        return None;
    }
    if index == 0 {
        return Some(BackendInfo {
            binding: SOFTWARE,
            milestone: 0,
            profile: a::GfxBackendProfile::default(),
            operations: 11,
            memory_generation: 0,
        });
    }
    let _bo = BoLock::acquire();
    let backend = unsafe { (*data()).backends[index as usize - 1] };
    if backend.owner.id == 0 || backend.closing {
        None
    } else {
        Some(BackendInfo {
            binding: backend.binding,
            milestone: backend.milestone,
            profile: backend.profile,
            operations: backend.operations,
            memory_generation: backend.memory_generation,
        })
    }
}
unsafe fn backend_locked(binding: model::Binding) -> Result<*mut Backend, Error> {
    let p = unsafe { data() };
    for i in 0..BACKEND_CAPACITY {
        let backend = unsafe { ptr::addr_of_mut!((*p).backends[i]) };
        if unsafe { (*backend).owner.id } == 0
            || unsafe { (*backend).binding.adapter } != binding.adapter
        {
            continue;
        }
        if unsafe { (*backend).binding } != binding {
            return Err(Error::Stale);
        }
        return Ok(backend);
    }
    Err(Error::Unsupported)
}
unsafe fn job_backend_locked(id: u32, fence: model::Fence) -> Result<*mut Backend, Error> {
    if id == 0 || fence.binding.adapter == 0 {
        return Err(Error::WrongOwner);
    }
    let _ = unsafe { state() }.query(fence)?;
    let p = unsafe { data() };
    for i in 0..BACKEND_CAPACITY {
        let backend = unsafe { ptr::addr_of_mut!((*p).backends[i]) };
        if unsafe { (*backend).owner.id } == u64::from(id)
            && unsafe { (*backend).binding.adapter } == fence.binding.adapter
            && unsafe { (*backend).binding.device_generation } == fence.binding.device_generation
        {
            return Ok(backend);
        }
    }
    Err(Error::WrongOwner)
}
pub fn open(owner: Owner, config: model::Config) -> Result<u64, Error> {
    if !available() {
        return Err(Error::Unavailable);
    }
    let _bo = BoLock::acquire();
    if config.binding.adapter == 0 {
        if config.binding != SOFTWARE || config.milestone != 0 {
            return Err(Error::Unsupported);
        }
    } else {
        let backend = unsafe { &*backend_locked(config.binding)? };
        if backend.closing {
            return Err(Error::DeviceLost);
        }
        if backend.milestone != config.milestone {
            return Err(Error::Unsupported);
        }
    }
    unsafe { state() }.open(owner, config)
}
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
unsafe fn queue_closing_locked(binding: model::Binding) {
    let p = unsafe { data() };
    for backend in unsafe { &mut *ptr::addr_of_mut!((*p).backends) } {
        if backend.owner.id != 0 && !backend.closing && backend.binding == binding {
            backend.lifecycle_dirty = true;
            return;
        }
    }
}
fn native_job_capacity(backend: &Backend) -> u32 {
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
#[repr(C)]
pub struct Taken {
    pub entry: resource::Entry,
    pub producer: Owner,
}
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
unsafe fn native_job_locked(id: u32, fence: model::Fence) -> Result<*mut queue_native::Job, Error> {
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
#[repr(C)]
pub struct NativeData {
    pub bytes: [MaybeUninit<u8>; a::gfx_native_read_capacity as usize],
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
#[repr(C)]
pub struct RetainedResource {
    pub reference: Handle,
    pub buffer: Handle,
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
unsafe fn lose_locked(backend: *mut Backend, quiesced: bool, instant: u64) {
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
unsafe fn retained_locked(backend: Backend) -> bool {
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
fn notify_native() -> bool {
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
struct DriverOwner;
impl Drop for DriverOwner {
    fn drop(&mut self) {
        let _ = api_admission::r4os_rust_driver_api_leave();
    }
}
unsafe extern "C" fn notify_driver(index: usize) -> i32 {
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
struct ConstructedJob {
    pointer: *mut queue_native::Job,
    destroy: bool,
}
impl Drop for ConstructedJob {
    fn drop(&mut self) {
        if self.destroy {
            unsafe { queue_native::Job::destroy(self.pointer) };
        }
    }
}
struct WakeOnDrop;
impl Drop for WakeOnDrop {
    fn drop(&mut self) {
        wake()
    }
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
fn submit_impl(
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
unsafe fn submit_locked(
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
fn wait_satisfied(status: model::Status, wait_for: u32) -> bool {
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
fn publish_and_release() {
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
fn copy_slice() -> bool {
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
unsafe extern "C" fn worker_main() {
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
#[repr(C)]
pub struct Args {
    pub owner: Owner,
    pub config: model::Config,
    pub binding: model::Binding,
    pub fence: model::Fence,
    pub timeline: u64,
    pub deadline: u64,
    pub frame_key: u64,
    pub value: u64,
    pub other: u64,
    pub id: u32,
    pub which: u32,
    pub bytes: u32,
    pub result: u32,
    pub flag: bool,
    pub dependencies: *const model::Fence,
    pub dependencies_len: usize,
    pub input: *const c_void,
    pub request: *const resource::Request,
}
#[repr(C)]
pub struct Optional<T> {
    pub has: bool,
    pub value: MaybeUninit<T>,
}
fn optional<T>(value: Option<T>) -> Optional<T> {
    Optional {
        has: value.is_some(),
        value: value.map_or(MaybeUninit::uninit(), MaybeUninit::new),
    }
}
/// # Safety
/// Exact fixed private args and operation-specific records retain their
/// original BO/runtime/task/driver admission. Pageable input is already copied
/// or pinned by its API owner. Locked operations never acquire that BO twice.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_queue(
    op: u32,
    args: *const Args,
    out: *mut c_void,
) -> i32 {
    let args = unsafe { &*args };
    let dependencies = if args.dependencies_len == 0 {
        &[][..]
    } else {
        unsafe { slice::from_raw_parts(args.dependencies, args.dependencies_len) }
    };
    let submission = resource::Submission {
        deadline_ns: args.deadline,
        frame_key: args.frame_key,
        dependencies,
    };
    let result = match op {
        0 => {
            unsafe { out.cast::<bool>().write(init()) };
            Ok(())
        }
        1 => {
            unsafe { out.cast::<bool>().write(available()) };
            Ok(())
        }
        2 => {
            wake();
            Ok(())
        }
        3 => {
            unsafe {
                out.cast::<Optional<BackendInfo>>()
                    .write(optional(backend_at(args.id)))
            };
            Ok(())
        }
        4 => open(args.owner, args.config).map(|v| unsafe { out.cast::<u64>().write(v) }),
        5 => validated_profile(unsafe { args.input.cast::<a::GfxBackendProfile>().read() })
            .map(|v| unsafe { out.cast::<a::GfxBackendProfile>().write(v) }),
        6 => validated_properties(unsafe { args.input.cast::<a::GfxBackendProperties>().read() })
            .map(|v| unsafe { out.cast::<a::GfxBackendProperties>().write(v) }),
        7 => publish_native_properties(args.owner, args.binding, args.which, unsafe {
            args.input.cast::<a::GfxBackendProperties>().read()
        }),
        8 => backend_properties(args.binding, args.which).map(|v| unsafe {
            out.cast::<Optional<a::GfxBackendProperties>>()
                .write(optional(v))
        }),
        9 => register_native(args.owner, unsafe {
            args.input.cast::<NativeConfig>().read()
        })
        .map(|v| unsafe { out.cast::<model::Binding>().write(v) }),
        10 => {
            native_milestone(args.id, args.binding).map(|v| unsafe { out.cast::<u32>().write(v) })
        }
        11 => {
            native_operations(args.id, args.binding).map(|v| unsafe { out.cast::<u64>().write(v) })
        }
        12 => native_queue_owner_info(args.id, args.binding, args.timeline).map(|v| unsafe {
            out.cast::<Optional<a::GfxQueueOwnerInfo>>()
                .write(optional(v))
        }),
        13 => unsafe { require_output_targets_locked(args.owner, args.binding, args.bytes) },
        14 => {
            unsafe {
                out.cast::<bool>().write(output_busy_locked(
                    args.input.cast::<a::GfxOutputTarget>().read(),
                ))
            };
            Ok(())
        }
        15 => update_native_operations(args.id, args.binding, args.value),
        16 => bind_display_queue(args.id, args.binding, args.timeline),
        17 => {
            unbind_display_queue(args.id, args.binding, args.timeline);
            Ok(())
        }
        18 => validate_output_binding(args.id, unsafe {
            args.input.cast::<a::GfxBackendBinding>().read()
        }),
        19 => take_native(args.id, args.binding, args.bytes)
            .map(|v| unsafe { out.cast::<Taken>().write(v) }),
        20 => native_segment(args.id, args.fence, args.which, args.value, args.other)
            .map(|v| unsafe { out.cast::<a::GfxDmaSegment>().write(v) }),
        21 => native_info(args.id, args.fence)
            .map(|v| unsafe { out.cast::<a::GfxNativeJobInfo>().write(v) }),
        22 => native_binding(args.id, args.fence, args.which)
            .map(|v| unsafe { out.cast::<a::GfxNativeBinding>().write(v) }),
        23 => native_data(
            args.id,
            args.fence,
            u32::try_from(args.value).expect("native command offset"),
            args.bytes,
        )
        .map(|v| unsafe { out.cast::<NativeData>().write(v) }),
        24 => native_render_list(args.id, args.fence)
            .map(|v| unsafe { out.cast::<a::GfxRenderList>().write(v) }),
        25 => native_render_grid_list(args.id, args.fence)
            .map(|v| unsafe { out.cast::<a::GfxRenderGridList>().write(v) }),
        26 => native_render_color_list(args.id, args.fence)
            .map(|v| unsafe { out.cast::<a::GfxRenderColorList>().write(v) }),
        27 => retain_native(args.owner, args.fence, args.which)
            .map(|v| unsafe { out.cast::<RetainedResource>().write(v) }),
        28 => complete_native(args.id, args.fence, args.result, args.flag),
        29 => retain_native_scanout(args.owner, args.fence)
            .map(|v| unsafe { out.cast::<RetainedResource>().write(v) }),
        30 => begin_native_scanout(args.id, args.fence),
        31 => native_scanout_retire_requested(args.id, args.fence)
            .map(|v| unsafe { out.cast::<bool>().write(v) }),
        32 => reset_native(args.id, args.binding, args.flag)
            .map(|v| unsafe { out.cast::<model::Binding>().write(v) }),
        33 => unregister_native(args.id, args.binding, args.flag),
        34 => {
            closing_driver(args.id);
            Ok(())
        }
        35 => {
            unsafe { out.cast::<bool>().write(retains_driver(args.id)) };
            Ok(())
        }
        36 => wake_native(args.id, args.binding),
        37 => wake_output(args.id, unsafe {
            args.input.cast::<a::GfxOutputTarget>().read()
        }),
        38 => submit(args.owner, args.timeline, &submission, unsafe {
            &*args.request
        })
        .map(|v| unsafe { out.cast::<model::Status>().write(v) }),
        39 => unsafe {
            submit_native(
                args.owner,
                args.timeline,
                &submission,
                args.input.cast::<a::GfxNativeSubmission>().read(),
            )
        }
        .map(|v| unsafe { out.cast::<model::Status>().write(v) }),
        40 => submit_display_image(
            args.owner,
            args.timeline,
            &submission,
            unsafe { &*args.request },
            args.binding,
        )
        .map(|v| unsafe { out.cast::<model::Status>().write(v) }),
        41 => unsafe {
            submit_output_locked(
                args.owner,
                args.timeline,
                &submission,
                &*args.request,
                args.binding,
            )
        }
        .map(|v| unsafe { out.cast::<model::Status>().write(v) }),
        42 => unsafe { query_locked(args.fence) }
            .map(|v| unsafe { out.cast::<model::Status>().write(v) }),
        43 => query(args.fence).map(|v| unsafe { out.cast::<model::Status>().write(v) }),
        44 => cancel(args.owner, args.fence),
        45 => close(args.owner, args.timeline),
        46 => drop_reference(args.owner, args.fence),
        47 => {
            stopped(args.owner);
            Ok(())
        }
        48 => {
            retired_task(args.id, args.value);
            Ok(())
        }
        49 => wait(args.fence, args.value, args.which)
            .map(|v| unsafe { out.cast::<model::Status>().write(v) }),
        _ => panic!("graphics queue runtime operation"),
    };
    match result {
        Ok(()) => 0,
        Err(error) => error as i32,
    }
}
