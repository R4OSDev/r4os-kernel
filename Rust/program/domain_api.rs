//! Program generation admission for existing GPU/display/audio/capture owners.
//! Canonical pointers retain the original lower-owner validation and completion
//! lifetimes; these adapters hold no registry lock or Rust record borrow.
use super::context;
use crate::contract as c;
use core::ptr;

pub(super) unsafe fn audio_owner() -> Option<crate::audio::mixer::Owner> {
    let handle = unsafe { context::handle()? };
    Some(crate::audio::mixer::Owner {
        instance_id: handle.instance_id,
        generation: handle.generation,
    })
}

/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_audio_open_stream(
    rate: u32,
    channels: u16,
    format: u16,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::service_api_result_invalid;
        };
        program_domain_audio_open_stream(owner, rate, channels, format)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_audio_output_info(
    index: u32,
    out: *mut c::AudioOutputInfo,
) -> i32 {
    unsafe { program_domain_audio_output_info(index, out) }
}

/// # Safety
/// Writable disjoint canonical caller buffers remain resident until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_audio_output_catalog(
    outputs: *mut c::AudioOutputInfo,
    capacity: u32,
    out: *mut c::AudioOutputCatalog,
) -> i32 {
    unsafe { crate::audio::core::audio_output_catalog(outputs, capacity, out) }
}

/// # Safety
/// `out` is a writable canonical record for the admitted program invocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_audio_stream_progress(
    id: u32,
    generation: u64,
    out: *mut c::AudioStreamProgress,
) -> i32 {
    let Some(owner) = (unsafe { context::handle() }) else {
        return c::service_api_result_invalid;
    };
    unsafe {
        crate::audio::core::stream_progress_for_owner(
            crate::audio::mixer::Owner {
                instance_id: owner.instance_id,
                generation: owner.generation,
            },
            id,
            generation,
            out,
        )
    }
}

/// # Safety
/// `input` is a readable canonical record for the admitted program invocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_audio_set_stream_expectation(
    id: u32,
    input: *const c::AudioStreamExpectation,
) -> i32 {
    let Some(owner) = (unsafe { context::handle() }) else {
        return c::service_api_result_invalid;
    };
    unsafe {
        crate::audio::core::set_stream_expectation_for_owner(
            crate::audio::mixer::Owner {
                instance_id: owner.instance_id,
                generation: owner.generation,
            },
            id,
            input,
        )
    }
}

/// # Safety
/// `out` is writable until the exact stream's drain and retirement return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_audio_close_stream_progress(
    id: u32,
    generation: u64,
    out: *mut c::AudioStreamProgress,
) -> i32 {
    let Some(owner) = (unsafe { context::handle() }) else {
        return c::service_api_result_invalid;
    };
    unsafe {
        crate::audio::core::close_stream_progress_for_owner(
            crate::audio::mixer::Owner {
                instance_id: owner.instance_id,
                generation: owner.generation,
            },
            id,
            generation,
            out,
        )
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_audio_select_output(id: *const [u8; 64]) -> i32 {
    unsafe {
        let mut len = 0;
        while len < 64 && id.cast::<u8>().add(len).read() != 0 {
            len += 1;
        }
        if len == 64 {
            return c::service_api_result_invalid;
        }
        program_domain_audio_select_output(id.cast(), len)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_audio_write(
    stream_id: u32,
    data_ptr: *const u8,
    byte_count: u32,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::service_api_result_invalid;
        };
        program_domain_audio_write(owner, stream_id, data_ptr, byte_count)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_audio_close(stream_id: u32) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::service_api_result_invalid;
        };
        program_domain_audio_close(owner, stream_id)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_audio_set_volume(
    stream_id: u32,
    fixed_volume: u32,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::service_api_result_invalid;
        };
        program_domain_audio_set_volume(owner, stream_id, fixed_volume)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_remote_frame_acquire() -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::remote_frame_error_unavailable;
        };
        program_domain_remote_frame_acquire(owner)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_remote_frame_release() -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::remote_frame_error_unavailable;
        };
        program_domain_remote_frame_release(owner)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_remote_frame_publish(
    info: *const c::RemoteFrameInfo,
    pixels: *const u32,
    count: u32,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::remote_frame_error_unavailable;
        };
        if !program_domain_remote_publisher(owner) {
            return c::remote_frame_error_unavailable;
        }
        program_domain_remote_frame_publish(owner, info, pixels, count)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_remote_frame_publish_regions(
    info: *const c::RemoteFrameInfo,
    pixels: *const u32,
    count: u32,
    regions: *const c::DisplayDamageRect,
    region_count: u32,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::remote_frame_error_unavailable;
        };
        if !program_domain_remote_publisher(owner) {
            return c::remote_frame_error_unavailable;
        }
        program_domain_remote_frame_publish_regions(
            owner,
            info,
            pixels,
            count,
            regions,
            region_count,
        )
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_remote_frame_snapshot_acquire(
    expected: u32,
    info: *mut c::RemoteFrameInfo,
    lease: *mut c::RemoteFrameLease,
) -> i32 {
    unsafe {
        if info.is_null() || lease.is_null() {
            return c::remote_frame_error_invalid;
        }
        let Some(owner) = context::handle() else {
            return c::remote_frame_error_unavailable;
        };
        program_domain_remote_frame_snapshot_acquire(owner, expected, info, lease)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_remote_frame_snapshot_release(
    lease: *const c::RemoteFrameLease,
) -> i32 {
    unsafe {
        if lease.is_null() {
            return c::remote_frame_error_invalid;
        }
        let Some(owner) = context::handle() else {
            return c::remote_frame_error_unavailable;
        };
        program_domain_remote_frame_snapshot_release(owner, lease)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_remote_frame_source_reset() -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::remote_frame_error_unavailable;
        };
        program_domain_remote_frame_source_reset(owner)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_gfx_buffer_create(
    input: *const c::GfxBufferDescriptor,
    output: *mut c::GfxBufferReference,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_buffer_error_unavailable;
        };
        program_domain_gfx_buffer_create(owner, input, output)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_gfx_virtual_start(
    input: *const c::GfxVirtualRequest,
    output: *mut c::GfxVirtualStatus,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_buffer_error_unavailable;
        };
        let instance = context::execution();
        if instance.is_null() {
            return c::gfx_buffer_error_unavailable;
        }
        program_domain_gfx_virtual_start(
            owner,
            ptr::addr_of_mut!((*instance).gfx_virtual_closed),
            input,
            output,
        )
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_gfx_virtual_query(
    input: *const c::GfxBufferHandle,
    output: *mut c::GfxVirtualStatus,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_buffer_error_unavailable;
        };
        program_domain_gfx_virtual_query(owner, input, output)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_gfx_virtual_close(
    input: *const c::GfxBufferHandle,
    mode: u32,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_buffer_error_unavailable;
        };
        program_domain_gfx_virtual_close(owner, input, mode)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_gfx_virtual_wait(
    input: *const c::GfxBufferHandle,
    until: u32,
    timeout: u64,
    output: *mut c::GfxVirtualStatus,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_buffer_error_unavailable;
        };
        program_domain_gfx_virtual_wait(owner, input, until, timeout, output)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_gfx_native_start(
    input: *const c::GfxNativeAllocation,
    output: *mut c::GfxNativeStatus,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_buffer_error_unavailable;
        };
        program_domain_gfx_native_start(owner, input, output)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_gfx_native_query(
    input: *const c::GfxBufferHandle,
    output: *mut c::GfxNativeStatus,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_buffer_error_unavailable;
        };
        program_domain_gfx_native_query(owner, input, output)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_gfx_native_receive(
    input: *const c::GfxBufferHandle,
    output: *mut c::GfxBufferReference,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_buffer_error_unavailable;
        };
        program_domain_gfx_native_receive(owner, input, output)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_gfx_native_close(
    input: *const c::GfxBufferHandle,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_buffer_error_unavailable;
        };
        program_domain_gfx_native_close(owner, input)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_gfx_native_wait(
    input: *const c::GfxBufferHandle,
    timeout_ticks: u64,
    output: *mut c::GfxNativeStatus,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_buffer_error_unavailable;
        };
        program_domain_gfx_native_wait(owner, input, timeout_ticks, output)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_gfx_queue_open(
    input: *const c::GfxQueueConfig,
    output: *mut c::GfxQueueHandle,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_queue_error_unavailable;
        };
        program_domain_gfx_queue_open(owner, input, output)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_gfx_atomic_test(
    input: *const c::GfxAtomicState,
    output: *mut c::GfxAtomicResult,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_output_error_unavailable;
        };
        program_domain_gfx_atomic_test(owner, input, output)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_gfx_atomic_commit(
    input: *const c::GfxAtomicState,
    output: *mut c::GfxAtomicResult,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_output_error_unavailable;
        };
        program_domain_gfx_atomic_commit(owner, input, output)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_gfx_atomic_submit(
    input: *const c::GfxAtomicState,
    confirmation_ms: u32,
    output: *mut c::GfxModeStatus,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_output_error_unavailable;
        };
        program_domain_gfx_atomic_submit(owner, input, confirmation_ms, output)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_gfx_atomic_test_color(
    input: *const c::GfxModeColorRequest,
    output: *mut c::GfxAtomicResult,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_output_error_unavailable;
        };
        program_domain_gfx_atomic_test_color(owner, input, output)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_gfx_atomic_submit_color(
    input: *const c::GfxModeColorRequest,
    confirmation_ms: u32,
    output: *mut c::GfxModeStatus,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_output_error_unavailable;
        };
        program_domain_gfx_atomic_submit_color(owner, input, confirmation_ms, output)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_gfx_refresh_request(
    input: *const c::GfxRefreshRequest,
    output: *mut c::GfxRefreshRequest,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_output_error_unavailable;
        };
        program_domain_gfx_refresh_request(owner, input, output)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_gfx_power_request(
    input: *const c::GfxPowerRequest,
    output: *mut c::GfxPowerRequest,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_output_error_unavailable;
        };
        program_domain_gfx_power_request(owner, input, output)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_gfx_atomic_resolve(
    ticket: u64,
    action: u32,
    output: *mut c::GfxModeStatus,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_output_error_unavailable;
        };
        program_domain_gfx_atomic_resolve(owner, ticket, action, output)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_gfx_queue_close(
    input: *const c::GfxQueueHandle,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_queue_error_unavailable;
        };
        program_domain_gfx_queue_close(owner, input)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_gfx_queue_submit(
    queue: *const c::GfxQueueHandle,
    input: *const c::GfxSubmission,
    output: *mut c::GfxFenceStatus,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_queue_error_unavailable;
        };
        program_domain_gfx_queue_submit(owner, queue, input, output)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_gfx_queue_submit_render_list(
    queue: *const c::GfxQueueHandle,
    input: *const c::GfxSubmission,
    list: *const c::GfxRenderList,
    output: *mut c::GfxFenceStatus,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_queue_error_unavailable;
        };
        program_domain_gfx_queue_submit_render_list(owner, queue, input, list, output)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_gfx_queue_submit_render_grid_list(
    queue: *const c::GfxQueueHandle,
    input: *const c::GfxSubmission,
    list: *const c::GfxRenderGridList,
    output: *mut c::GfxFenceStatus,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_queue_error_unavailable;
        };
        program_domain_gfx_queue_submit_render_grid_list(owner, queue, input, list, output)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_gfx_queue_submit_output(
    queue: *const c::GfxQueueHandle,
    input: *const c::GfxSubmission,
    target: *const c::GfxOutputTarget,
    output: *mut c::GfxFenceStatus,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_queue_error_unavailable;
        };
        program_domain_gfx_queue_submit_output(owner, queue, input, target, output)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_gfx_queue_submit_render_color_list(
    queue: *const c::GfxQueueHandle,
    input: *const c::GfxSubmission,
    list: *const c::GfxRenderColorList,
    output: *mut c::GfxFenceStatus,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_queue_error_unavailable;
        };
        program_domain_gfx_queue_submit_render_color_list(owner, queue, input, list, output)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_gfx_queue_submit_native(
    queue: *const c::GfxQueueHandle,
    input: *const c::GfxSubmission,
    native: *const c::GfxNativeSubmission,
    output: *mut c::GfxFenceStatus,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_queue_error_unavailable;
        };
        program_domain_gfx_queue_submit_native(owner, queue, input, native, output)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_gfx_queue_submit_render_color_grid_list(
    queue: *const c::GfxQueueHandle,
    input: *const c::GfxSubmission,
    list: *const c::GfxRenderColorGridList,
    output: *mut c::GfxFenceStatus,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_queue_error_unavailable;
        };
        program_domain_gfx_queue_submit_render_color_grid_list(owner, queue, input, list, output)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_gfx_fence_cancel(
    input: *const c::GfxFence,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_queue_error_unavailable;
        };
        program_domain_gfx_fence_cancel(owner, input)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_gfx_fence_release(
    input: *const c::GfxFence,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_queue_error_unavailable;
        };
        program_domain_gfx_fence_release(owner, input)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_gfx_buffer_describe(
    input: *const c::GfxBufferHandle,
    output: *mut c::GfxBufferDescriptor,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_buffer_error_unavailable;
        };
        program_domain_gfx_buffer_describe(owner, input, output)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_gfx_buffer_import(
    input: *const c::GfxBufferHandle,
    output: *mut c::GfxBufferReference,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_buffer_error_unavailable;
        };
        program_domain_gfx_buffer_import(owner, input, output)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_gfx_buffer_release(
    input: *const c::GfxBufferHandle,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_buffer_error_unavailable;
        };
        program_domain_gfx_buffer_release(owner, input)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_gfx_buffer_map(
    input: *const c::GfxBufferHandle,
    access: u32,
    offset: u64,
    bytes: u64,
    output: *mut c::GfxBufferMap,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_buffer_error_unavailable;
        };
        program_domain_gfx_buffer_map(owner, input, access, offset, bytes, output)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_gfx_buffer_unmap(
    input: *const c::GfxBufferHandle,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_buffer_error_unavailable;
        };
        program_domain_gfx_buffer_unmap(owner, input)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_gfx_buffer_map_persistent(
    input: *const c::GfxBufferHandle,
    access: u32,
    offset: u64,
    bytes: u64,
    output: *mut c::GfxBufferMap,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_buffer_error_unavailable;
        };
        program_domain_gfx_buffer_map_persistent(owner, input, access, offset, bytes, output)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_gfx_brightness_request(
    input: *const c::GfxBrightnessRequest,
    output: *mut c::GfxBrightnessRequest,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_output_error_unavailable;
        };
        program_domain_gfx_brightness_request(owner, input, output)
    }
}
/// # Safety
/// Original resident canonical input/output buffers and exact program execution
/// lifetime; the existing lower owner retains its admission/ACK contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_domain_display_cursor_submit(
    input: *const c::DisplayCursorRequest,
    output: *mut c::DisplayCursorStatus,
) -> i32 {
    unsafe {
        let Some(owner) = context::handle() else {
            return c::gfx_output_error_unavailable;
        };
        program_domain_display_cursor_submit(owner, input, output)
    }
}

unsafe fn program_domain_gfx_buffer_create(
    owner: c::ProgramProcessHandle,
    input: *const c::GfxBufferDescriptor,
    output: *mut c::GfxBufferReference,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: crate::gfx::types::Owner {
            kind: 1,
            id: u64::from(owner.instance_id),
            generation: owner.generation,
        },
        input: input.cast(),
        ..Default::default()
    };
    unsafe { crate::gfx::buffer_api::r4os_rust_gfx_buffer_api(0, &args, output.cast()) }
}

unsafe fn program_domain_gfx_buffer_describe(
    owner: c::ProgramProcessHandle,
    input: *const c::GfxBufferHandle,
    output: *mut c::GfxBufferDescriptor,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: crate::gfx::types::Owner {
            kind: 1,
            id: u64::from(owner.instance_id),
            generation: owner.generation,
        },
        input: input.cast(),
        ..Default::default()
    };
    unsafe { crate::gfx::buffer_api::r4os_rust_gfx_buffer_api(1, &args, output.cast()) }
}

unsafe fn program_domain_gfx_buffer_import(
    owner: c::ProgramProcessHandle,
    input: *const c::GfxBufferHandle,
    output: *mut c::GfxBufferReference,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: crate::gfx::types::Owner {
            kind: 1,
            id: u64::from(owner.instance_id),
            generation: owner.generation,
        },
        input: input.cast(),
        ..Default::default()
    };
    unsafe { crate::gfx::buffer_api::r4os_rust_gfx_buffer_api(2, &args, output.cast()) }
}

pub(super) unsafe fn program_domain_gfx_buffer_release(
    owner: c::ProgramProcessHandle,
    input: *const c::GfxBufferHandle,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: crate::gfx::types::Owner {
            kind: 1,
            id: u64::from(owner.instance_id),
            generation: owner.generation,
        },
        input: input.cast(),
        ..Default::default()
    };
    unsafe { crate::gfx::buffer_api::r4os_rust_gfx_buffer_api(3, &args, core::ptr::null_mut()) }
}

pub(super) unsafe fn program_domain_gfx_buffer_map(
    owner: c::ProgramProcessHandle,
    input: *const c::GfxBufferHandle,
    access: u32,
    offset: u64,
    bytes: u64,
    output: *mut c::GfxBufferMap,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: crate::gfx::types::Owner {
            kind: 1,
            id: u64::from(owner.instance_id),
            generation: owner.generation,
        },
        input: input.cast(),
        access,
        offset,
        bytes,
        ..Default::default()
    };
    unsafe { crate::gfx::buffer_api::r4os_rust_gfx_buffer_api(4, &args, output.cast()) }
}

pub(super) unsafe fn program_domain_gfx_buffer_unmap(
    owner: c::ProgramProcessHandle,
    input: *const c::GfxBufferHandle,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: crate::gfx::types::Owner {
            kind: 1,
            id: u64::from(owner.instance_id),
            generation: owner.generation,
        },
        input: input.cast(),
        ..Default::default()
    };
    unsafe { crate::gfx::buffer_api::r4os_rust_gfx_buffer_api(6, &args, core::ptr::null_mut()) }
}

unsafe fn program_domain_gfx_buffer_map_persistent(
    owner: c::ProgramProcessHandle,
    input: *const c::GfxBufferHandle,
    access: u32,
    offset: u64,
    bytes: u64,
    output: *mut c::GfxBufferMap,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: crate::gfx::types::Owner {
            kind: 1,
            id: u64::from(owner.instance_id),
            generation: owner.generation,
        },
        input: input.cast(),
        access,
        offset,
        bytes,
        ..Default::default()
    };
    unsafe { crate::gfx::buffer_api::r4os_rust_gfx_buffer_api(5, &args, output.cast()) }
}

unsafe fn program_domain_gfx_native_start(
    owner: c::ProgramProcessHandle,
    input: *const c::GfxNativeAllocation,
    output: *mut c::GfxNativeStatus,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: crate::gfx::types::Owner {
            kind: 1,
            id: u64::from(owner.instance_id),
            generation: owner.generation,
        },
        input: input.cast(),
        ..Default::default()
    };
    unsafe {
        crate::gfx::allocation_api::r4os_rust_gfx_allocation_api(false, 0, &args, output.cast())
    }
}

unsafe fn program_domain_gfx_native_query(
    owner: c::ProgramProcessHandle,
    input: *const c::GfxBufferHandle,
    output: *mut c::GfxNativeStatus,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: crate::gfx::types::Owner {
            kind: 1,
            id: u64::from(owner.instance_id),
            generation: owner.generation,
        },
        input: input.cast(),
        ..Default::default()
    };
    unsafe {
        crate::gfx::allocation_api::r4os_rust_gfx_allocation_api(false, 1, &args, output.cast())
    }
}

unsafe fn program_domain_gfx_native_receive(
    owner: c::ProgramProcessHandle,
    input: *const c::GfxBufferHandle,
    output: *mut c::GfxBufferReference,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: crate::gfx::types::Owner {
            kind: 1,
            id: u64::from(owner.instance_id),
            generation: owner.generation,
        },
        input: input.cast(),
        ..Default::default()
    };
    unsafe {
        crate::gfx::allocation_api::r4os_rust_gfx_allocation_api(false, 2, &args, output.cast())
    }
}

unsafe fn program_domain_gfx_native_close(
    owner: c::ProgramProcessHandle,
    input: *const c::GfxBufferHandle,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: crate::gfx::types::Owner {
            kind: 1,
            id: u64::from(owner.instance_id),
            generation: owner.generation,
        },
        input: input.cast(),
        ..Default::default()
    };
    unsafe {
        crate::gfx::allocation_api::r4os_rust_gfx_allocation_api(
            false,
            3,
            &args,
            core::ptr::null_mut(),
        )
    }
}

unsafe fn program_domain_gfx_native_wait(
    owner: c::ProgramProcessHandle,
    input: *const c::GfxBufferHandle,
    timeout_ticks: u64,
    output: *mut c::GfxNativeStatus,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: crate::gfx::types::Owner {
            kind: 1,
            id: u64::from(owner.instance_id),
            generation: owner.generation,
        },
        input: input.cast(),
        value: timeout_ticks,
        ..Default::default()
    };
    unsafe {
        crate::gfx::allocation_api::r4os_rust_gfx_allocation_api(false, 4, &args, output.cast())
    }
}

unsafe fn program_domain_gfx_virtual_start(
    owner: c::ProgramProcessHandle,
    closed: *mut bool,
    input: *const c::GfxVirtualRequest,
    output: *mut c::GfxVirtualStatus,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: crate::gfx::types::Owner {
            kind: 1,
            id: u64::from(owner.instance_id),
            generation: owner.generation,
        },
        input: input.cast(),
        other_input: closed.cast(),
        ..Default::default()
    };
    unsafe { crate::gfx::virtual_api::r4os_rust_gfx_virtual_api(false, 0, &args, output.cast()) }
}

unsafe fn program_domain_gfx_virtual_query(
    owner: c::ProgramProcessHandle,
    input: *const c::GfxBufferHandle,
    output: *mut c::GfxVirtualStatus,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: crate::gfx::types::Owner {
            kind: 1,
            id: u64::from(owner.instance_id),
            generation: owner.generation,
        },
        input: input.cast(),
        ..Default::default()
    };
    unsafe { crate::gfx::virtual_api::r4os_rust_gfx_virtual_api(false, 1, &args, output.cast()) }
}

unsafe fn program_domain_gfx_virtual_close(
    owner: c::ProgramProcessHandle,
    input: *const c::GfxBufferHandle,
    mode: u32,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: crate::gfx::types::Owner {
            kind: 1,
            id: u64::from(owner.instance_id),
            generation: owner.generation,
        },
        input: input.cast(),
        access: mode,
        ..Default::default()
    };
    unsafe {
        crate::gfx::virtual_api::r4os_rust_gfx_virtual_api(false, 2, &args, core::ptr::null_mut())
    }
}

unsafe fn program_domain_gfx_virtual_wait(
    owner: c::ProgramProcessHandle,
    input: *const c::GfxBufferHandle,
    until: u32,
    timeout: u64,
    output: *mut c::GfxVirtualStatus,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: crate::gfx::types::Owner {
            kind: 1,
            id: u64::from(owner.instance_id),
            generation: owner.generation,
        },
        input: input.cast(),
        access: until,
        value: timeout,
        ..Default::default()
    };
    unsafe { crate::gfx::virtual_api::r4os_rust_gfx_virtual_api(false, 3, &args, output.cast()) }
}

unsafe fn program_domain_gfx_queue_open(
    owner: c::ProgramProcessHandle,
    input: *const c::GfxQueueConfig,
    output: *mut c::GfxQueueHandle,
) -> i32 {
    let args = crate::program::gfx_queue_api::Args {
        owner: crate::gfx::types::Owner {
            kind: 1,
            id: u64::from(owner.instance_id),
            generation: owner.generation,
        },
        input: input.cast(),
        which: 0,
        ..Default::default()
    };
    unsafe { crate::program::gfx_queue_api::r4os_rust_gfx_queue_api(3, &args, output.cast()) }
}

unsafe fn program_domain_gfx_queue_close(
    owner: c::ProgramProcessHandle,
    input: *const c::GfxQueueHandle,
) -> i32 {
    let args = crate::program::gfx_queue_api::Args {
        owner: crate::gfx::types::Owner {
            kind: 1,
            id: u64::from(owner.instance_id),
            generation: owner.generation,
        },
        input: input.cast(),
        which: 0,
        ..Default::default()
    };
    unsafe {
        crate::program::gfx_queue_api::r4os_rust_gfx_queue_api(4, &args, core::ptr::null_mut())
    }
}

unsafe fn program_domain_gfx_queue_submit(
    owner: c::ProgramProcessHandle,
    queue: *const c::GfxQueueHandle,
    input: *const c::GfxSubmission,
    output: *mut c::GfxFenceStatus,
) -> i32 {
    let args = crate::program::gfx_queue_api::Args {
        owner: crate::gfx::types::Owner {
            kind: 1,
            id: u64::from(owner.instance_id),
            generation: owner.generation,
        },
        input: input.cast(),
        which: 0,
        queue,
        ..Default::default()
    };
    unsafe { crate::program::gfx_queue_api::r4os_rust_gfx_queue_api(5, &args, output.cast()) }
}

unsafe fn program_domain_gfx_queue_submit_native(
    owner: c::ProgramProcessHandle,
    queue: *const c::GfxQueueHandle,
    input: *const c::GfxSubmission,
    native: *const c::GfxNativeSubmission,
    output: *mut c::GfxFenceStatus,
) -> i32 {
    let args = crate::program::gfx_queue_api::Args {
        owner: crate::gfx::types::Owner {
            kind: 1,
            id: u64::from(owner.instance_id),
            generation: owner.generation,
        },
        input: input.cast(),
        which: 0,
        queue,
        extra: native.cast(),
        ..Default::default()
    };
    unsafe { crate::program::gfx_queue_api::r4os_rust_gfx_queue_api(6, &args, output.cast()) }
}

unsafe fn program_domain_gfx_queue_submit_render_list(
    owner: c::ProgramProcessHandle,
    queue: *const c::GfxQueueHandle,
    input: *const c::GfxSubmission,
    list: *const c::GfxRenderList,
    output: *mut c::GfxFenceStatus,
) -> i32 {
    let args = crate::program::gfx_queue_api::Args {
        owner: crate::gfx::types::Owner {
            kind: 1,
            id: u64::from(owner.instance_id),
            generation: owner.generation,
        },
        input: input.cast(),
        which: 1,
        queue,
        extra: list.cast(),
        ..Default::default()
    };
    unsafe { crate::program::gfx_queue_api::r4os_rust_gfx_queue_api(5, &args, output.cast()) }
}

unsafe fn program_domain_gfx_queue_submit_render_grid_list(
    owner: c::ProgramProcessHandle,
    queue: *const c::GfxQueueHandle,
    input: *const c::GfxSubmission,
    list: *const c::GfxRenderGridList,
    output: *mut c::GfxFenceStatus,
) -> i32 {
    let args = crate::program::gfx_queue_api::Args {
        owner: crate::gfx::types::Owner {
            kind: 1,
            id: u64::from(owner.instance_id),
            generation: owner.generation,
        },
        input: input.cast(),
        which: 2,
        queue,
        extra: list.cast(),
        ..Default::default()
    };
    unsafe { crate::program::gfx_queue_api::r4os_rust_gfx_queue_api(5, &args, output.cast()) }
}

unsafe fn program_domain_gfx_queue_submit_render_color_list(
    owner: c::ProgramProcessHandle,
    queue: *const c::GfxQueueHandle,
    input: *const c::GfxSubmission,
    list: *const c::GfxRenderColorList,
    output: *mut c::GfxFenceStatus,
) -> i32 {
    let args = crate::program::gfx_queue_api::Args {
        owner: crate::gfx::types::Owner {
            kind: 1,
            id: u64::from(owner.instance_id),
            generation: owner.generation,
        },
        input: input.cast(),
        which: 3,
        queue,
        extra: list.cast(),
        ..Default::default()
    };
    unsafe { crate::program::gfx_queue_api::r4os_rust_gfx_queue_api(5, &args, output.cast()) }
}

unsafe fn program_domain_gfx_queue_submit_render_color_grid_list(
    owner: c::ProgramProcessHandle,
    queue: *const c::GfxQueueHandle,
    input: *const c::GfxSubmission,
    list: *const c::GfxRenderColorGridList,
    output: *mut c::GfxFenceStatus,
) -> i32 {
    let args = crate::program::gfx_queue_api::Args {
        owner: crate::gfx::types::Owner {
            kind: 1,
            id: u64::from(owner.instance_id),
            generation: owner.generation,
        },
        input: input.cast(),
        which: 4,
        queue,
        extra: list.cast(),
        ..Default::default()
    };
    unsafe { crate::program::gfx_queue_api::r4os_rust_gfx_queue_api(5, &args, output.cast()) }
}

unsafe fn program_domain_gfx_queue_submit_output(
    owner: c::ProgramProcessHandle,
    queue: *const c::GfxQueueHandle,
    input: *const c::GfxSubmission,
    target: *const c::GfxOutputTarget,
    output: *mut c::GfxFenceStatus,
) -> i32 {
    let args = crate::program::gfx_queue_api::Args {
        owner: crate::gfx::types::Owner {
            kind: 1,
            id: u64::from(owner.instance_id),
            generation: owner.generation,
        },
        input: input.cast(),
        which: 5,
        queue,
        extra: target.cast(),
        ..Default::default()
    };
    unsafe { crate::program::gfx_queue_api::r4os_rust_gfx_queue_api(5, &args, output.cast()) }
}

unsafe fn program_domain_display_cursor_submit(
    owner: c::ProgramProcessHandle,
    input: *const c::DisplayCursorRequest,
    output: *mut c::DisplayCursorStatus,
) -> i32 {
    let owner = crate::gfx::types::Owner {
        kind: 1,
        id: u64::from(owner.instance_id),
        generation: owner.generation,
    };
    unsafe {
        crate::display::cursor_work::r4os_rust_display_cursor_work(
            3,
            &owner,
            input.cast(),
            output.cast(),
        )
    }
}

unsafe fn program_domain_audio_close(owner: c::ProgramProcessHandle, stream_id: u32) -> i32 {
    unsafe {
        crate::audio::core::close_stream_for_owner(
            crate::audio::mixer::Owner {
                instance_id: owner.instance_id,
                generation: owner.generation,
            },
            stream_id,
        )
    }
}

unsafe fn program_domain_audio_open_stream(
    owner: c::ProgramProcessHandle,
    rate: u32,
    channels: u16,
    format: u16,
) -> i32 {
    unsafe {
        crate::audio::core::open_stream_for_owner(
            crate::audio::mixer::Owner {
                instance_id: owner.instance_id,
                generation: owner.generation,
            },
            rate,
            channels,
            format,
        )
    }
}

unsafe fn program_domain_audio_write(
    owner: c::ProgramProcessHandle,
    stream_id: u32,
    data_ptr: *const u8,
    byte_count: u32,
) -> i32 {
    unsafe {
        crate::audio::core::write_stream_for_owner(
            crate::audio::mixer::Owner {
                instance_id: owner.instance_id,
                generation: owner.generation,
            },
            stream_id,
            data_ptr,
            byte_count,
        )
    }
}

unsafe fn program_domain_audio_set_volume(
    owner: c::ProgramProcessHandle,
    stream_id: u32,
    fixed_volume: u32,
) -> i32 {
    unsafe {
        crate::audio::core::set_volume_for_owner(
            crate::audio::mixer::Owner {
                instance_id: owner.instance_id,
                generation: owner.generation,
            },
            stream_id,
            fixed_volume,
        )
    }
}

unsafe fn program_domain_audio_output_info(index: u32, out: *mut c::AudioOutputInfo) -> i32 {
    unsafe { crate::audio::core::audio_output_info(index, out) }
}

unsafe fn program_domain_audio_select_output(id: *const u8, len: usize) -> i32 {
    unsafe {
        crate::audio::core::audio_select_output(if len == 0 {
            &[]
        } else {
            core::slice::from_raw_parts(id, len)
        })
    }
}

unsafe fn program_domain_gfx_atomic_test(
    owner: c::ProgramProcessHandle,
    input: *const c::GfxAtomicState,
    output: *mut c::GfxAtomicResult,
) -> i32 {
    unsafe {
        crate::program::gfx_output_api::atomic(
            crate::gfx::types::Owner {
                kind: 1,
                id: u64::from(owner.instance_id),
                generation: owner.generation,
            },
            input,
            output,
            false,
        )
    }
}

unsafe fn program_domain_gfx_atomic_commit(
    owner: c::ProgramProcessHandle,
    input: *const c::GfxAtomicState,
    output: *mut c::GfxAtomicResult,
) -> i32 {
    unsafe {
        crate::program::gfx_output_api::atomic(
            crate::gfx::types::Owner {
                kind: 1,
                id: u64::from(owner.instance_id),
                generation: owner.generation,
            },
            input,
            output,
            true,
        )
    }
}

unsafe fn program_domain_gfx_atomic_submit(
    owner: c::ProgramProcessHandle,
    input: *const c::GfxAtomicState,
    confirmation_ms: u32,
    output: *mut c::GfxModeStatus,
) -> i32 {
    unsafe {
        crate::program::gfx_output_api::submit(
            crate::gfx::types::Owner {
                kind: 1,
                id: u64::from(owner.instance_id),
                generation: owner.generation,
            },
            input,
            confirmation_ms,
            output,
        )
    }
}

unsafe fn program_domain_gfx_atomic_submit_color(
    owner: c::ProgramProcessHandle,
    input: *const c::GfxModeColorRequest,
    confirmation_ms: u32,
    output: *mut c::GfxModeStatus,
) -> i32 {
    unsafe {
        crate::program::gfx_output_api::submit_color(
            crate::gfx::types::Owner {
                kind: 1,
                id: u64::from(owner.instance_id),
                generation: owner.generation,
            },
            input,
            confirmation_ms,
            output,
        )
    }
}

unsafe fn program_domain_gfx_atomic_test_color(
    owner: c::ProgramProcessHandle,
    input: *const c::GfxModeColorRequest,
    output: *mut c::GfxAtomicResult,
) -> i32 {
    unsafe {
        crate::program::gfx_output_api::test_color(
            crate::gfx::types::Owner {
                kind: 1,
                id: u64::from(owner.instance_id),
                generation: owner.generation,
            },
            input,
            output,
        )
    }
}

unsafe fn program_domain_gfx_atomic_resolve(
    owner: c::ProgramProcessHandle,
    ticket: u64,
    action: u32,
    output: *mut c::GfxModeStatus,
) -> i32 {
    unsafe {
        crate::program::gfx_output_api::resolve(
            crate::gfx::types::Owner {
                kind: 1,
                id: u64::from(owner.instance_id),
                generation: owner.generation,
            },
            ticket,
            action,
            output,
        )
    }
}

unsafe fn program_domain_gfx_brightness_request(
    owner: c::ProgramProcessHandle,
    input: *const c::GfxBrightnessRequest,
    output: *mut c::GfxBrightnessRequest,
) -> i32 {
    unsafe {
        crate::program::gfx_output_api::request_brightness(
            crate::gfx::types::Owner {
                kind: 1,
                id: u64::from(owner.instance_id),
                generation: owner.generation,
            },
            input,
            output,
        )
    }
}

unsafe fn program_domain_gfx_refresh_request(
    owner: c::ProgramProcessHandle,
    input: *const c::GfxRefreshRequest,
    output: *mut c::GfxRefreshRequest,
) -> i32 {
    unsafe {
        crate::program::gfx_output_api::request_refresh(
            crate::gfx::types::Owner {
                kind: 1,
                id: u64::from(owner.instance_id),
                generation: owner.generation,
            },
            input,
            output,
        )
    }
}

unsafe fn program_domain_gfx_power_request(
    owner: c::ProgramProcessHandle,
    input: *const c::GfxPowerRequest,
    output: *mut c::GfxPowerRequest,
) -> i32 {
    unsafe {
        crate::program::gfx_output_api::request_power(
            crate::gfx::types::Owner {
                kind: 1,
                id: u64::from(owner.instance_id),
                generation: owner.generation,
            },
            input,
            output,
        )
    }
}

unsafe fn program_domain_gfx_fence_cancel(
    owner: c::ProgramProcessHandle,
    input: *const c::GfxFence,
) -> i32 {
    unsafe {
        crate::program::gfx_queue_api::release(
            crate::gfx::types::Owner {
                kind: 1,
                id: u64::from(owner.instance_id),
                generation: owner.generation,
            },
            input,
            true,
        )
    }
}

unsafe fn program_domain_gfx_fence_release(
    owner: c::ProgramProcessHandle,
    input: *const c::GfxFence,
) -> i32 {
    unsafe {
        crate::program::gfx_queue_api::release(
            crate::gfx::types::Owner {
                kind: 1,
                id: u64::from(owner.instance_id),
                generation: owner.generation,
            },
            input,
            false,
        )
    }
}

unsafe fn program_domain_remote_frame_acquire(owner: c::ProgramProcessHandle) -> i32 {
    {
        crate::program::r4desk_frame::acquire(crate::program::remote_frame_state::Owner {
            id: owner.instance_id,
            generation: owner.generation,
        })
    }
}

unsafe fn program_domain_remote_frame_release(owner: c::ProgramProcessHandle) -> i32 {
    {
        crate::program::r4desk_frame::release(crate::program::remote_frame_state::Owner {
            id: owner.instance_id,
            generation: owner.generation,
        })
    }
}

unsafe fn program_domain_remote_frame_source_reset(owner: c::ProgramProcessHandle) -> i32 {
    {
        crate::program::r4desk_frame::source_reset(crate::program::remote_frame_state::Owner {
            id: owner.instance_id,
            generation: owner.generation,
        })
    }
}

unsafe fn program_domain_remote_publisher(owner: c::ProgramProcessHandle) -> bool {
    {
        crate::program::r4desk_frame::publisher(crate::program::remote_frame_state::Owner {
            id: owner.instance_id,
            generation: owner.generation,
        })
    }
}

unsafe fn program_domain_remote_frame_publish(
    owner: c::ProgramProcessHandle,
    info: *const c::RemoteFrameInfo,
    pixels: *const u32,
    count: u32,
) -> i32 {
    let _ = owner;
    unsafe { crate::program::r4desk_frame::publish(info, pixels, count) }
}

unsafe fn program_domain_remote_frame_publish_regions(
    owner: c::ProgramProcessHandle,
    info: *const c::RemoteFrameInfo,
    pixels: *const u32,
    count: u32,
    regions: *const c::DisplayDamageRect,
    region_count: u32,
) -> i32 {
    let _ = owner;
    unsafe {
        crate::program::r4desk_frame::publish_regions(info, pixels, count, regions, region_count)
    }
}

unsafe fn program_domain_remote_frame_snapshot_acquire(
    owner: c::ProgramProcessHandle,
    expected: u32,
    info: *mut c::RemoteFrameInfo,
    lease: *mut c::RemoteFrameLease,
) -> i32 {
    unsafe {
        crate::program::r4desk_frame::snapshot_acquire(
            crate::program::remote_frame_state::Owner {
                id: owner.instance_id,
                generation: owner.generation,
            },
            expected,
            info,
            lease,
        )
    }
}

unsafe fn program_domain_remote_frame_snapshot_release(
    owner: c::ProgramProcessHandle,
    lease: *const c::RemoteFrameLease,
) -> i32 {
    unsafe {
        crate::program::r4desk_frame::snapshot_release(
            crate::program::remote_frame_state::Owner {
                id: owner.instance_id,
                generation: owner.generation,
            },
            lease,
        )
    }
}
