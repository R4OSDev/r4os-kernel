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
    platform::monotonic,
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
        state: 3,
        role: 1,
        needs_fpu: true,
        smp_eligible: false,
        smp_r4x: false,
        owner_kind: 0,
        owner_context: ptr::null_mut(),
        entry: Some(worker_main),
    };
    let mut failure = 0;
    let task = unsafe { task_api::r4os_rust_task_create(&request, false, &mut failure) };
    if task.is_null() {
        return false;
    }
    STARTED.store(true, Ordering::Release);
    assert!(
        crate::sched::scheduler::publish_created(task),
        "graphics work first publication"
    );
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

#[repr(C)]
pub struct Taken {
    pub entry: resource::Entry,
    pub producer: Owner,
}

#[repr(C)]
pub struct NativeData {
    pub bytes: [MaybeUninit<u8>; a::gfx_native_read_capacity as usize],
}

#[repr(C)]
pub struct RetainedResource {
    pub reference: Handle,
    pub buffer: Handle,
}

struct DriverOwner;
impl Drop for DriverOwner {
    fn drop(&mut self) {
        let _ = api_admission::r4os_rust_driver_api_leave();
    }
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

// Private operation modules share the sole state and original typed owners.
mod backends;
mod jobs;
mod native_completion;
mod worker;
pub use backends::{
    backend_properties, bind_display_queue, native_milestone, native_operations,
    native_queue_owner_info, output_busy_locked, publish_native_properties, register_native,
    require_output_targets_locked, unbind_display_queue, update_native_operations,
    validate_output_binding, validated_profile, validated_properties,
};
use backends::{native_job_capacity, queue_closing_locked};
pub use jobs::{
    cancel, close, drop_reference, query, query_locked, retired_task, stopped, submit,
    submit_display_image, submit_native, submit_output_locked, wait,
};
use native_completion::notify_native;
pub use native_completion::{
    begin_native_scanout, closing_driver, complete_native, native_binding, native_data,
    native_info, native_render_color_list, native_render_grid_list, native_render_list,
    native_scanout_retire_requested, native_segment, reset_native, retain_native,
    retain_native_scanout, retains_driver, take_native, unregister_native, wake_native,
    wake_output,
};
use worker::worker_main;
