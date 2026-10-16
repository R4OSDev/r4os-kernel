//! DriverApi passes its authenticated init/work/IRQ owner. All pageable
//! inputs/outputs stay outside the BO owner; caller ids create no authority.
use super::{
    error::Error, ownership, queue, queue_resources as resource, queue_state as model, wire,
};
use crate::{
    contract as a,
    driver::irq_router,
    gfx::{buffer_api, types::Owner},
    program::gfx_queue_api as api,
};
use core::{ffi::c_void, mem::size_of, ptr};
fn binding(id: u32, input: a::GfxBackendBinding) -> Result<model::Binding, Error> {
    if input.version != 1 || (input.size as usize) < size_of::<a::GfxBackendBinding>() {
        return Err(Error::Invalid);
    }
    let value = super::native_driver::binding(input);
    let milestone = queue::native_milestone(id, value)?;
    if input.milestone != milestone {
        return Err(Error::Invalid);
    }
    Ok(value)
}
fn public_binding(value: model::Binding, milestone: u32) -> a::GfxBackendBinding {
    a::GfxBackendBinding {
        adapter_id: value.adapter,
        device_generation: value.device_generation,
        reset_generation: value.reset_generation,
        milestone,
        ..Default::default()
    }
}
fn finish(result: Result<(), i32>) -> i32 {
    result.map_or_else(|error| error, |()| a::gfx_queue_ok)
}
unsafe fn output<T>(out: *const T) -> Result<(), i32> {
    if unsafe { buffer_api::valid_output(out) } {
        Ok(())
    } else {
        Err(a::gfx_queue_error_invalid)
    }
}
/// # Safety
/// Exact driver admission and original complete profile/request/output live
/// through registration; queue registration may read only a released prefix.
pub unsafe fn register_profile(
    identity: Owner,
    input: *const a::GfxBackendRegistration,
    profile: *const a::GfxBackendProfile,
    out: *mut a::GfxBackendBinding,
) -> i32 {
    finish((|| {
        unsafe {
            if input.is_null() || profile.is_null() {
                return Err(a::gfx_queue_error_invalid);
            }
            output(out)?;
            let request = wire::read(input).ok_or(a::gfx_queue_error_invalid)?;
            if request.notify_callback < 0xffff800000000000 || irq_router::in_dispatch() {
                return Err(a::gfx_queue_error_invalid);
            }
            if request.milestone > 2 {
                return Err(a::gfx_queue_error_unsupported);
            }
            let value = queue::register_native(
                identity,
                queue::NativeConfig {
                    adapter: request.adapter_id,
                    milestone: request.milestone,
                    notify: core::mem::transmute::<usize, crate::driver::work::Handler>(
                        request.notify_callback as usize,
                    ),
                    context: request.context as usize,
                    profile: profile.read(),
                    operations: request.operations,
                    memory_generation: request.memory_generation,
                },
            )
            .map_err(api::error_code)?;
            out.write(public_binding(value, request.milestone));
        }
        Ok(())
    })())
}
/// # Safety
/// Original complete backend record is retained through authenticated removal.
pub unsafe fn unregister(id: u32, input: *const a::GfxBackendBinding, quiesced: u32) -> i32 {
    finish((|| {
        unsafe {
            if input.is_null() || quiesced > 1 || irq_router::in_dispatch() {
                return Err(a::gfx_queue_error_invalid);
            }
            let value = binding(id, input.read()).map_err(api::error_code)?;
            queue::unregister_native(id, value, quiesced == 1).map_err(api::error_code)?;
        }
        Ok(())
    })())
}
/// # Safety
/// Exact admitted owner retains complete input and properties for this call.
pub unsafe fn publish_properties(
    identity: Owner,
    input: *const a::GfxBackendBinding,
    properties: *const a::GfxBackendProperties,
) -> i32 {
    finish((|| {
        unsafe {
            if input.is_null() || properties.is_null() || irq_router::in_dispatch() {
                return Err(a::gfx_queue_error_invalid);
            }
            let value = input.read();
            if value.version != 1 || (value.size as usize) < size_of::<a::GfxBackendBinding>() {
                return Err(a::gfx_queue_error_invalid);
            }
            queue::publish_native_properties(
                identity,
                super::native_driver::binding(value),
                value.milestone,
                properties.read(),
            )
            .map_err(api::error_code)?;
        }
        Ok(())
    })())
}
/// # Safety
/// Complete backend is retained through its exact authenticated operation edit.
pub unsafe fn update_operations(
    id: u32,
    input: *const a::GfxBackendBinding,
    operations: u64,
) -> i32 {
    finish((|| {
        unsafe {
            if input.is_null() || irq_router::in_dispatch() {
                return Err(a::gfx_queue_error_invalid);
            }
            let value = binding(id, input.read()).map_err(api::error_code)?;
            queue::update_native_operations(id, value, operations).map_err(api::error_code)?;
        }
        Ok(())
    })())
}
/// # Safety
/// Original binding and initialized released job output prefix stay resident.
pub unsafe fn take(id: u32, input: *const a::GfxBackendBinding, out: *mut a::GfxDriverJob) -> i32 {
    finish((|| {
        unsafe {
            if input.is_null() || irq_router::in_dispatch() {
                return Err(a::gfx_queue_error_invalid);
            }
            let bytes = wire::capacity(out).ok_or(a::gfx_queue_error_invalid)?;
            let value = binding(id, input.read()).map_err(api::error_code)?;
            let taken = queue::take_native(id, value, bytes).map_err(api::error_code)?;
            let job = taken.entry;
            let result = a::GfxDriverJob {
                size: bytes,
                fence: api::public_fence(job.fence),
                operation: job.operation,
                source_buffer: job.uses[0]
                    .get()
                    .map_or_else(a::GfxBufferHandle::default, |use_| {
                        buffer_api::public_handle(use_.buffer)
                    }),
                target_buffer: job.uses[1]
                    .get()
                    .map_or_else(a::GfxBufferHandle::default, |use_| {
                        buffer_api::public_handle(use_.buffer)
                    }),
                byte_length: if matches!(
                    job.operation,
                    resource::COPY_ROWS | resource::PRESENT | resource::DIRECT_PRESENT
                ) {
                    job.row_bytes
                } else {
                    job.bytes
                },
                source_offset: job.source_offset,
                target_offset: job.target_offset,
                row_count: job.row_count,
                source_pitch: job.source_pitch,
                target_pitch: job.target_pitch,
                render: job.render,
                deadline_ns: job.deadline_ns,
                display_target: job.display_target,
                producer_kind: taken.producer.kind + 1,
                producer_id: taken.producer.id,
                producer_generation: taken.producer.generation,
                ..Default::default()
            };
            wire::write(out, bytes, result);
        }
        Ok(())
    })())
}
/// # Safety
/// Exact authenticated driver retains a complete original public fence.
pub unsafe fn complete(id: u32, input: *const a::GfxFence, result: u32, quiesced: u32) -> i32 {
    finish((|| {
        unsafe {
            if input.is_null() || quiesced > 1 {
                return Err(a::gfx_queue_error_invalid);
            }
            let value = api::fence(input.read());
            if result > model::DEPENDENCY_FAILED {
                return Err(a::gfx_queue_error_invalid);
            }
            queue::complete_native(id, value, result, quiesced == 1).map_err(api::error_code)?;
        }
        Ok(())
    })())
}
/// # Safety
/// Both complete input and initialized output remain live after backend reset.
pub unsafe fn reset(
    id: u32,
    input: *const a::GfxBackendBinding,
    quiesced: u32,
    out: *mut a::GfxBackendBinding,
) -> i32 {
    finish((|| {
        unsafe {
            if input.is_null()
                || quiesced > 1
                || !buffer_api::valid_output(out)
                || irq_router::in_dispatch()
            {
                return Err(a::gfx_queue_error_invalid);
            }
            let request = input.read();
            let value = binding(id, request).map_err(api::error_code)?;
            let changed = queue::reset_native(id, value, quiesced == 1).map_err(api::error_code)?;
            out.write(public_binding(changed, request.milestone));
        }
        Ok(())
    })())
}
/// # Safety
/// Exact complete fence and initialized DMA segment output remain resident.
pub unsafe fn segment(
    id: u32,
    input: *const a::GfxFence,
    which: u32,
    offset: u64,
    mask: u64,
    out: *mut a::GfxDmaSegment,
) -> i32 {
    finish((|| {
        unsafe {
            if input.is_null() {
                return Err(a::gfx_queue_error_invalid);
            }
            output(out)?;
            let value = queue::native_segment(id, api::fence(input.read()), which, offset, mask)
                .map_err(api::error_code)?;
            out.write(value);
        }
        Ok(())
    })())
}
/// # Safety
/// Exact admitted driver pins a complete source fence and initialized output
/// through mapping-only or scanout reference retention.
pub unsafe fn retain(
    identity: Owner,
    input: *const a::GfxFence,
    which: u32,
    scanout: bool,
    out: *mut a::GfxBufferReference,
) -> i32 {
    finish((|| {
        unsafe {
            if input.is_null() {
                return Err(a::gfx_queue_error_invalid);
            }
            output(out)?;
            let value = api::fence(input.read());
            let _call = ownership::Call::acquire().map_err(|_| a::gfx_queue_error_busy)?;
            let retained = if scanout {
                queue::retain_native_scanout(identity, value)
            } else {
                queue::retain_native(identity, value, which)
            }
            .map_err(api::error_code)?;
            out.write(a::GfxBufferReference {
                reference: buffer_api::public_handle(retained.reference),
                buffer: buffer_api::public_handle(retained.buffer),
                flags: if scanout {
                    a::gfx_buffer_reference_immutable
                } else {
                    a::gfx_buffer_reference_mapping_only
                },
                ..Default::default()
            });
        }
        Ok(())
    })())
}
unsafe fn read<T>(
    id: u32,
    input: *const a::GfxFence,
    out: *mut T,
    query: impl FnOnce(u32, model::Fence) -> Result<T, Error>,
) -> i32 {
    finish((|| {
        unsafe {
            if input.is_null() {
                return Err(a::gfx_queue_error_invalid);
            }
            output(out)?;
            let value = query(id, api::fence(input.read())).map_err(api::error_code)?;
            out.write(value);
        }
        Ok(())
    })())
}
/// # Safety
/// Complete binding and initialized owner-info output stay live after lookup.
pub unsafe fn queue_owner_info(
    id: u32,
    input: *const a::GfxBackendBinding,
    timeline: u64,
    out: *mut a::GfxQueueOwnerInfo,
) -> i32 {
    finish((|| {
        unsafe {
            if irq_router::in_dispatch() || input.is_null() {
                return Err(a::gfx_queue_error_invalid);
            }
            output(out)?;
            let value = binding(id, input.read()).map_err(api::error_code)?;
            if let Some(info) =
                queue::native_queue_owner_info(id, value, timeline).map_err(api::error_code)?
            {
                out.write(info)
            } else {
                return Err(0);
            }
        }
        Ok(())
    })())
}
/// # Safety
/// Original complete fence and count writable disjoint output bytes are live.
pub unsafe fn read_native_data(
    id: u32,
    input: *const a::GfxFence,
    offset: u32,
    out: *mut u8,
    count: u32,
) -> i32 {
    finish((|| {
        unsafe {
            if input.is_null() || out.is_null() || out.addr() > usize::MAX - count as usize {
                return Err(a::gfx_queue_error_invalid);
            }
            let result = queue::native_data(id, api::fence(input.read()), offset, count)
                .map_err(api::error_code)?;
            ptr::copy_nonoverlapping(result.bytes.as_ptr().cast::<u8>(), out, count as usize);
        }
        Ok(())
    })())
}
/// # Safety
/// Complete fence retains the exact authenticated scanout job for this call.
pub unsafe fn begin_scanout(id: u32, input: *const a::GfxFence) -> i32 {
    finish((|| {
        unsafe {
            if input.is_null() {
                return Err(a::gfx_queue_error_invalid);
            }
            queue::begin_native_scanout(id, api::fence(input.read())).map_err(api::error_code)?;
        }
        Ok(())
    })())
}
/// # Safety
/// Exact driver retains the complete fence through the retirement query.
pub unsafe fn scanout_retire_requested(id: u32, input: *const a::GfxFence) -> i32 {
    if input.is_null() {
        return a::gfx_queue_error_invalid;
    }
    queue::native_scanout_retire_requested(id, api::fence(unsafe { input.read() }))
        .map_or_else(api::error_code, i32::from)
}
#[repr(C)]
#[derive(Default)]
pub struct Args {
    pub owner: Owner,
    pub value: u64,
    pub extra_value: u64,
    pub id: u32,
    pub which: u32,
    pub other: u32,
    pub input: *const c_void,
    pub second: *const c_void,
}
/// # Safety
/// DriverApi retains authenticated identity and the exact original slot data.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_driver_queue(
    op: u32,
    args: *const Args,
    out: *mut c_void,
) -> i32 {
    let args = unsafe { &*args };
    unsafe {
        match op {
            0 => register_profile(
                args.owner,
                args.input.cast(),
                args.second.cast(),
                out.cast(),
            ),
            1 => unregister(args.id, args.input.cast(), args.other),
            2 => publish_properties(args.owner, args.input.cast(), args.second.cast()),
            3 => update_operations(args.id, args.input.cast(), args.value),
            4 => take(args.id, args.input.cast(), out.cast()),
            5 => complete(args.id, args.input.cast(), args.which, args.other),
            6 => read(
                args.id,
                args.input.cast(),
                out.cast(),
                queue::native_render_list,
            ),
            7 => read(
                args.id,
                args.input.cast(),
                out.cast(),
                queue::native_render_grid_list,
            ),
            8 => reset(args.id, args.input.cast(), args.other, out.cast()),
            9 => read(
                args.id,
                args.input.cast(),
                out.cast(),
                queue::native_render_color_list,
            ),
            10 => segment(
                args.id,
                args.input.cast(),
                args.which,
                args.value,
                args.extra_value,
                out.cast(),
            ),
            11 => retain(args.owner, args.input.cast(), args.which, false, out.cast()),
            12 => retain(args.owner, args.input.cast(), 0, true, out.cast()),
            13 => read(args.id, args.input.cast(), out.cast(), queue::native_info),
            14 => queue_owner_info(args.id, args.input.cast(), args.value, out.cast()),
            15 => read_native_data(
                args.id,
                args.input.cast(),
                args.which,
                out.cast(),
                args.other,
            ),
            16 => read(args.id, args.input.cast(), out.cast(), |id, fence| {
                queue::native_binding(id, fence, args.which)
            }),
            17 => begin_scanout(args.id, args.input.cast()),
            18 => scanout_retire_requested(args.id, args.input.cast()),
            _ => panic!("driver queue API operation"),
        }
    }
}
const _: () = {
    assert!(size_of::<Args>() == 72);
    assert!(core::mem::offset_of!(Args, input) == 56);
};
