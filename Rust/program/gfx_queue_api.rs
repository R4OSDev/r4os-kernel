//! Public timeline API. Copied request/dependency data stays outside metadata
//! owners; the original task pin protects explicit submissions and releases.
#![allow(clippy::too_many_arguments)]
use crate::{
    contract as a,
    display::{
        equal::same, error::Error, native_driver, output_target, ownership, queue as runtime,
        queue_resources as resource, queue_state as model, wire,
    },
    gfx::{
        buffer_api,
        types::{Handle, Owner},
    },
};
use core::{ffi::c_void, mem::size_of, ptr};
pub fn error_code(error: Error) -> i32 {
    match error {
        Error::Unavailable => a::gfx_queue_error_unavailable,
        Error::DeviceLost => a::gfx_queue_error_device_lost,
        Error::WaitTimeout => a::gfx_queue_error_wait_timeout,
        Error::WaitCancelled => a::gfx_queue_error_wait_cancelled,
        Error::AlreadyCompleted => a::gfx_queue_error_already_completed,
        Error::Invalid | Error::WrongOwner => a::gfx_buffer_error_invalid,
        Error::Overflow | Error::Exhausted => a::gfx_buffer_error_overflow,
        Error::Unsupported => a::gfx_buffer_error_unsupported,
        Error::OutOfMemory => a::gfx_buffer_error_oom,
        Error::Budget => a::gfx_buffer_error_budget,
        Error::Capacity => a::gfx_buffer_error_capacity,
        Error::Stale => a::gfx_buffer_error_stale,
        Error::Busy => a::gfx_buffer_error_busy,
        Error::Closed => a::gfx_buffer_error_closed,
        _ => panic!("non-queue public error"),
    }
}
pub fn fence(input: a::GfxFence) -> model::Fence {
    model::Fence {
        slot: input.slot,
        timeline: input.timeline,
        point: input.point,
        binding: model::Binding {
            adapter: input.adapter_id,
            device_generation: input.device_generation,
            reset_generation: input.reset_generation,
        },
    }
}
pub fn public_fence(value: model::Fence) -> a::GfxFence {
    a::GfxFence {
        slot: value.slot,
        timeline: value.timeline,
        point: value.point,
        adapter_id: value.binding.adapter,
        device_generation: value.binding.device_generation,
        reset_generation: value.binding.reset_generation,
    }
}
pub fn public_status(value: model::Status) -> a::GfxFenceStatus {
    a::GfxFenceStatus {
        fence: public_fence(value.fence),
        phase: value.phase,
        result: value.result,
        milestone: value.milestone,
        flags: if value.device_active {
            a::gfx_queue_flag_device_active
        } else {
            0
        } | if value.resources_held {
            a::gfx_queue_flag_resources_held
        } else {
            0
        },
        deadline_ns: value.deadline_ns,
        completed_ns: value.completed_ns,
        ..Default::default()
    }
}
fn header(version: u32, size: u32, needed: usize) -> bool {
    version == 1 && size as usize >= needed
}
unsafe fn output<T>(value: *const T) -> Result<(), i32> {
    if unsafe { buffer_api::valid_output(value) } {
        Ok(())
    } else {
        Err(a::gfx_queue_error_invalid)
    }
}
fn call() -> Result<ownership::Call, i32> {
    ownership::Call::acquire().map_err(|_| a::gfx_queue_error_busy)
}
fn finish(result: Result<(), i32>) -> i32 {
    result.map_or_else(|error| error, |()| a::gfx_queue_ok)
}
/// # Safety
/// Writable complete initialized backend output remains resident after query.
pub unsafe extern "C" fn backend(index: u32, out: *mut a::GfxBackendBinding) -> i32 {
    finish((|| {
        unsafe {
            output(out)?;
            if let Some(info) = runtime::backend_at(index) {
                out.write(a::GfxBackendBinding {
                    adapter_id: info.binding.adapter,
                    device_generation: info.binding.device_generation,
                    reset_generation: info.binding.reset_generation,
                    milestone: info.milestone,
                    ..Default::default()
                });
            } else {
                return Err(0);
            }
        }
        Ok(())
    })())
}
/// # Safety
/// Output retains its original eight-byte header and declared released prefix.
pub unsafe extern "C" fn backend_info(index: u32, out: *mut a::GfxBackendInfo) -> i32 {
    finish((|| {
        unsafe {
            let bytes = wire::capacity(out).ok_or(a::gfx_queue_error_invalid)?;
            if let Some(info) = runtime::backend_at(index) {
                let result = a::GfxBackendInfo {
                    size: bytes,
                    binding: a::GfxBackendBinding {
                        adapter_id: info.binding.adapter,
                        device_generation: info.binding.device_generation,
                        reset_generation: info.binding.reset_generation,
                        milestone: info.milestone,
                        ..Default::default()
                    },
                    profile: info.profile,
                    operations: info.operations,
                    memory_generation: info.memory_generation,
                    ..Default::default()
                };
                wire::write(out, bytes, result);
            } else {
                return Err(0);
            }
        }
        Ok(())
    })())
}
/// # Safety
/// Exact complete input binding and initialized properties output are retained.
pub unsafe extern "C" fn backend_properties(
    input: *const a::GfxBackendBinding,
    out: *mut a::GfxBackendProperties,
) -> i32 {
    finish((|| {
        unsafe {
            if input.is_null() {
                return Err(a::gfx_queue_error_invalid);
            }
            output(out)?;
            let value = input.read();
            if !header(value.version, value.size, size_of::<a::GfxBackendBinding>()) {
                return Err(a::gfx_queue_error_invalid);
            }
            if let Some(result) =
                runtime::backend_properties(native_driver::binding(value), value.milestone)
                    .map_err(error_code)?
            {
                out.write(result)
            } else {
                return Err(0);
            }
        }
        Ok(())
    })())
}
/// # Safety
/// Exact caller plus copied complete config and initialized output are resident.
pub unsafe fn open(
    owner: Owner,
    input: *const a::GfxQueueConfig,
    out: *mut a::GfxQueueHandle,
) -> i32 {
    finish((|| {
        unsafe {
            if input.is_null() {
                return Err(a::gfx_queue_error_invalid);
            }
            output(out)?;
            let value = input.read();
            if !header(value.version, value.size, size_of::<a::GfxQueueConfig>()) {
                return Err(a::gfx_queue_error_invalid);
            }
            if value.policy > 1 || value.milestone > 2 {
                return Err(a::gfx_queue_error_unsupported);
            }
            let config = model::Config {
                policy: value.policy,
                milestone: value.milestone,
                capacity: value.capacity,
                binding: model::Binding {
                    adapter: value.adapter_id,
                    device_generation: value.device_generation,
                    reset_generation: value.reset_generation,
                },
            };
            let _call = call()?;
            let timeline = runtime::open(owner, config).map_err(error_code)?;
            out.write(a::GfxQueueHandle {
                timeline,
                ..Default::default()
            });
        }
        Ok(())
    })())
}
/// # Safety
/// Exact caller and complete queue handle stay live through close admission.
pub unsafe fn close(owner: Owner, input: *const a::GfxQueueHandle) -> i32 {
    finish((|| {
        unsafe {
            if input.is_null() {
                return Err(a::gfx_queue_error_invalid);
            }
            let value = input.read();
            if !header(value.version, value.size, size_of::<a::GfxQueueHandle>()) {
                return Err(a::gfx_queue_error_invalid);
            }
            let _call = call()?;
            runtime::close(owner, value.timeline).map_err(error_code)?;
        }
        Ok(())
    })())
}
fn dependencies(value: &a::GfxSubmission) -> Result<[model::Fence; model::MAX_DEPENDENCIES], i32> {
    let mut dependencies = [model::Fence::default(); model::MAX_DEPENDENCIES];
    for (i, dependency) in value.dependencies.iter().enumerate() {
        if i < (value.dependency_count as usize) {
            dependencies[i] = fence(*dependency)
        } else if !same(dependency, &a::GfxFence::default()) {
            return Err(a::gfx_queue_error_invalid);
        }
    }
    Ok(dependencies)
}
/// # Safety
/// All complete command/resource ranges remain live through resident copying;
/// output and queue handle have initialized exact public layouts.
pub unsafe fn submit_native(
    owner: Owner,
    queue: *const a::GfxQueueHandle,
    input: *const a::GfxSubmission,
    native: *const a::GfxNativeSubmission,
    out: *mut a::GfxFenceStatus,
) -> i32 {
    finish((|| {
        unsafe {
            if queue.is_null() || input.is_null() || native.is_null() || !native.is_aligned() {
                return Err(a::gfx_queue_error_invalid);
            }
            output(out)?;
            let queue = queue.read();
            let value = wire::read(input).ok_or(a::gfx_queue_error_invalid)?;
            let native = native.read();
            if !header(queue.version, queue.size, size_of::<a::GfxQueueHandle>())
                || value.operation != a::gfx_queue_operation_native
                || (value.size as usize) < size_of::<a::GfxSubmission>()
                || value.dependency_count as usize > model::MAX_DEPENDENCIES
                || !same(&value.source, &a::GfxBufferHandle::default())
                || !same(&value.target, &a::GfxBufferHandle::default())
                || value.source_offset != 0
                || value.target_offset != 0
                || value.byte_length != 0
                || value.row_count != 0
                || value.reserved0 != 0
                || value.source_pitch != 0
                || value.target_pitch != 0
                || !same(&value.render, &a::GfxRenderCommand::default())
            {
                return Err(a::gfx_queue_error_invalid);
            }
            let dependencies = dependencies(&value)?;
            let _call = call()?;
            let snapshot = runtime::submit_native(
                owner,
                queue.timeline,
                &resource::Submission {
                    deadline_ns: value.deadline_ns,
                    frame_key: value.frame_key,
                    dependencies: &dependencies[..value.dependency_count as usize],
                },
                native,
            )
            .map_err(error_code)?;
            out.write(public_status(snapshot));
        }
        Ok(())
    })())
}
unsafe fn submit_common(
    owner: Owner,
    queue: *const a::GfxQueueHandle,
    input: *const a::GfxSubmission,
    list: *const a::GfxRenderList,
    grid: *const a::GfxRenderGridList,
    color: *const a::GfxRenderColorList,
    combined: *const a::GfxRenderColorGridList,
    target: Option<a::GfxOutputTarget>,
    out: *mut a::GfxFenceStatus,
) -> i32 {
    finish((|| {
        unsafe {
            if queue.is_null() || input.is_null() {
                return Err(a::gfx_queue_error_invalid);
            }
            output(out)?;
            let queue = queue.read();
            let value = wire::read(input).ok_or(a::gfx_queue_error_invalid)?;
            if !header(queue.version, queue.size, size_of::<a::GfxQueueHandle>())
                || value.dependency_count as usize > model::MAX_DEPENDENCIES
                || value.source.reserved0 != 0
                || value.target.reserved0 != 0
                || value.reserved0 != 0
            {
                return Err(a::gfx_queue_error_invalid);
            }
            let operation = value.operation;
            if operation > resource::RENDER_COLOR_GRID_LIST {
                return Err(a::gfx_queue_error_unsupported);
            }
            if target.is_some()
                && !matches!(operation, resource::PRESENT | resource::DIRECT_PRESENT)
            {
                return Err(a::gfx_queue_error_invalid);
            }
            if matches!(
                operation,
                resource::RENDER
                    | resource::RENDER_LIST
                    | resource::RENDER_GRID_LIST
                    | resource::RENDER_COLOR_LIST
                    | resource::RENDER_COLOR_GRID_LIST
            ) && (value.size as usize) < size_of::<a::GfxSubmission>()
            {
                return Err(a::gfx_queue_error_invalid);
            }
            if (operation == resource::RENDER_LIST) != !list.is_null()
                || (operation == resource::RENDER_GRID_LIST) != !grid.is_null()
                || (operation == resource::RENDER_COLOR_LIST) != !color.is_null()
                || (operation == resource::RENDER_COLOR_GRID_LIST) != !combined.is_null()
            {
                return Err(a::gfx_queue_error_invalid);
            }
            let dependencies = dependencies(&value)?;
            let _call = call()?;
            let submission = resource::Submission {
                deadline_ns: value.deadline_ns,
                frame_key: value.frame_key,
                dependencies: &dependencies[..value.dependency_count as usize],
            };
            let request = resource::Request {
                operation,
                source: Handle {
                    id: value.source.id,
                    generation: value.source.generation,
                },
                target: Handle {
                    id: value.target.id,
                    generation: value.target.generation,
                },
                source_offset: value.source_offset,
                target_offset: value.target_offset,
                bytes: value.byte_length,
                row_count: value.row_count,
                source_pitch: value.source_pitch,
                target_pitch: value.target_pitch,
                render: value.render,
                render_list: list,
                grid_list: grid,
                color_list: color,
                color_grid_list: combined,
                display_target: target.unwrap_or_default(),
                ..Default::default()
            };
            let snapshot = if matches!(operation, resource::PRESENT | resource::DIRECT_PRESENT) {
                native_driver::submit_image(owner, queue.timeline, &submission, &request)
            } else {
                runtime::submit(owner, queue.timeline, &submission, &request)
            }
            .map_err(error_code)?;
            out.write(public_status(snapshot));
        }
        Ok(())
    })())
}
/// # Safety
/// Exact retained queue/submission/output and corresponding list payload are
/// copied before any shared owner. Kind selects the original public entrypoint.
pub unsafe fn submit(
    owner: Owner,
    queue: *const a::GfxQueueHandle,
    input: *const a::GfxSubmission,
    kind: u32,
    extra: *const c_void,
    out: *mut a::GfxFenceStatus,
) -> i32 {
    unsafe {
        match kind {
            0 => submit_common(
                owner,
                queue,
                input,
                ptr::null(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
                None,
                out,
            ),
            1 => {
                let list = extra.cast::<a::GfxRenderList>();
                if list.is_null() || !list.is_aligned() {
                    return a::gfx_queue_error_invalid;
                }
                let list = list.read();
                submit_common(
                    owner,
                    queue,
                    input,
                    &list,
                    ptr::null(),
                    ptr::null(),
                    ptr::null(),
                    None,
                    out,
                )
            }
            2 => {
                let list = extra.cast::<a::GfxRenderGridList>();
                if list.is_null() || !list.is_aligned() {
                    return a::gfx_queue_error_invalid;
                }
                let list = list.read();
                submit_common(
                    owner,
                    queue,
                    input,
                    ptr::null(),
                    &list,
                    ptr::null(),
                    ptr::null(),
                    None,
                    out,
                )
            }
            3 => {
                let list = extra.cast::<a::GfxRenderColorList>();
                if list.is_null() || !list.is_aligned() {
                    return a::gfx_queue_error_invalid;
                }
                let list = list.read();
                submit_common(
                    owner,
                    queue,
                    input,
                    ptr::null(),
                    ptr::null(),
                    &list,
                    ptr::null(),
                    None,
                    out,
                )
            }
            4 => {
                let list = extra.cast::<a::GfxRenderColorGridList>();
                if list.is_null() || !list.is_aligned() {
                    return a::gfx_queue_error_invalid;
                }
                let list = list.read();
                submit_common(
                    owner,
                    queue,
                    input,
                    ptr::null(),
                    ptr::null(),
                    ptr::null(),
                    &list,
                    None,
                    out,
                )
            }
            5 => {
                let target = extra.cast::<a::GfxOutputTarget>();
                if target.is_null() || !target.is_aligned() {
                    return a::gfx_queue_error_invalid;
                }
                let target = target.read();
                if !output_target::valid(&target) {
                    return a::gfx_queue_error_invalid;
                }
                submit_common(
                    owner,
                    queue,
                    input,
                    ptr::null(),
                    ptr::null(),
                    ptr::null(),
                    ptr::null(),
                    Some(target),
                    out,
                )
            }
            _ => panic!("public submission kind"),
        }
    }
}
/// # Safety
/// Exact readable public fence and initialized status output remain resident.
pub unsafe extern "C" fn query(input: *const a::GfxFence, out: *mut a::GfxFenceStatus) -> i32 {
    finish((|| {
        unsafe {
            if input.is_null() {
                return Err(a::gfx_queue_error_invalid);
            }
            output(out)?;
            let snapshot = runtime::query(fence(input.read())).map_err(error_code)?;
            out.write(public_status(snapshot));
        }
        Ok(())
    })())
}
/// # Safety
/// Fence/output stay resident throughout the original scheduler wait.
pub unsafe extern "C" fn wait(
    input: *const a::GfxFence,
    timeout: u64,
    wait_for: u32,
    out: *mut a::GfxFenceStatus,
) -> i32 {
    finish((|| {
        unsafe {
            if input.is_null() {
                return Err(a::gfx_queue_error_invalid);
            }
            output(out)?;
            let identity = fence(input.read());
            if wait_for > 1 {
                return Err(a::gfx_queue_error_invalid);
            }
            let snapshot = runtime::wait(identity, timeout, wait_for).map_err(error_code)?;
            out.write(public_status(snapshot));
        }
        Ok(())
    })())
}
/// # Safety
/// Exact caller and complete fence remain valid through cancel/drop admission.
pub unsafe fn release(owner: Owner, input: *const a::GfxFence, cancel: bool) -> i32 {
    finish((|| {
        unsafe {
            if input.is_null() {
                return Err(a::gfx_queue_error_invalid);
            }
            let identity = fence(input.read());
            let _call = call()?;
            if cancel {
                runtime::cancel(owner, identity)
            } else {
                runtime::drop_reference(owner, identity)
            }
            .map_err(error_code)?;
        }
        Ok(())
    })())
}
#[repr(C)]
pub struct Args {
    pub owner: Owner,
    pub value: u64,
    pub which: u32,
    pub other: u32,
    pub input: *const c_void,
    pub queue: *const a::GfxQueueHandle,
    pub extra: *const c_void,
}
/// # Safety
/// Each original public API slot supplies the exact resident records above.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_queue_api(
    op: u32,
    args: *const Args,
    out: *mut c_void,
) -> i32 {
    let args = unsafe { &*args };
    unsafe {
        match op {
            0 => backend(args.which, out.cast()),
            1 => backend_info(args.which, out.cast()),
            2 => backend_properties(args.input.cast(), out.cast()),
            3 => open(args.owner, args.input.cast(), out.cast()),
            4 => close(args.owner, args.input.cast()),
            5 => submit(
                args.owner,
                args.queue,
                args.input.cast(),
                args.which,
                args.extra,
                out.cast(),
            ),
            6 => submit_native(
                args.owner,
                args.queue,
                args.input.cast(),
                args.extra.cast(),
                out.cast(),
            ),
            7 => query(args.input.cast(), out.cast()),
            8 => wait(args.input.cast(), args.value, args.which, out.cast()),
            9 => release(args.owner, args.input.cast(), true),
            10 => release(args.owner, args.input.cast(), false),
            11 => {
                out.cast::<model::Fence>()
                    .write(fence(args.input.cast::<a::GfxFence>().read()));
                0
            }
            12 => {
                out.cast::<a::GfxFence>()
                    .write(public_fence(args.input.cast::<model::Fence>().read()));
                0
            }
            13 => {
                out.cast::<a::GfxFenceStatus>()
                    .write(public_status(args.input.cast::<model::Status>().read()));
                0
            }
            14 => match crate::display::error::status(args.which as i32) {
                Err(e) => error_code(e),
                Ok(()) => panic!("queue error projection"),
            },
            _ => panic!("public queue API operation"),
        }
    }
}
const _: () = {
    assert!(size_of::<Args>() == 64);
    assert!(core::mem::offset_of!(Args, input) == 40);
};
