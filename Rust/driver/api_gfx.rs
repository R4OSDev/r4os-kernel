//! Original graphics entrypoint ownership. Cached tables carry no identity;
//! admission resolves the exact driver epoch again for each public callback.
use crate::{
    contract as a,
    driver::{api_admission as admission, heap, irq_router, threads},
    sched::scheduler,
};
use core::{mem::MaybeUninit, ptr};
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Identity {
    pub id: u64,
    pub generation: u64,
}

pub fn identity(mode: u8, admit: bool) -> Result<Identity, i32> {
    if mode == 1
        && (admission::r4os_rust_driver_api_owner_scalar(0) == 0
            || admission::r4os_rust_driver_api_owner_scalar(1) == 0)
    {
        return Err(a::gfx_buffer_error_invalid);
    }
    if irq_router::r4os_rust_irq_router_local(false) != 0
        || (mode != 3 && admission::r4os_rust_driver_api_storage_owner() != 0)
        || threads::r4os_rust_driver_thread_current(0, 0) != 0
        || (mode < 2 && scheduler::current().is_null())
    {
        return Err(a::gfx_buffer_error_invalid);
    }
    let owner = admission::r4os_rust_driver_api_runtime_owner(false);
    if owner == 0 {
        return Err(a::gfx_buffer_error_invalid);
    }
    if mode < 2 {
        let mut out = MaybeUninit::uninit();
        let code = unsafe { driver_buffer_identity(owner, admit, out.as_mut_ptr()) };
        return if code == 0 {
            Ok(unsafe { out.assume_init() })
        } else {
            Err(code)
        };
    }
    let mut snapshot = a::DriverHeapStats::default();
    if unsafe { heap::r4os_rust_driver_heap_stats(owner, &mut snapshot) } != a::driver_heap_ok
        || snapshot.owner_epoch == 0
    {
        return Err(a::gfx_buffer_error_stale);
    }
    if admit && snapshot.closing != 0 {
        return Err(a::gfx_buffer_error_closed);
    }
    Ok(Identity {
        id: u64::from(owner),
        generation: snapshot.owner_epoch,
    })
}
/// # Safety
/// Output is an aligned resident private identity record, exclusive for this
/// call. The backend retains its exact driver epoch until confirmed cleanup.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_identity(
    mode: u8,
    admit: bool,
    out: *mut Identity,
) -> i32 {
    match identity(mode, admit) {
        Ok(value) => {
            unsafe { ptr::write(out, value) };
            0
        }
        Err(code) => code,
    }
}
fn held() -> bool {
    admission::r4os_rust_driver_api_owner_scalar(0) != 0
        && admission::r4os_rust_driver_api_owner_scalar(1) != 0
}
fn invalid(irq: bool) -> bool {
    irq && irq_router::r4os_rust_irq_router_local(false) != 0
}
/// # Safety
/// Pointer is resident and readable for the schema header after nonzero and
/// alignment admission; size checks never form references beyond its prefix.
pub unsafe fn header<T>(p: *const T, aligned: bool) -> bool {
    if p.is_null() || aligned && !(p as usize).is_multiple_of(core::mem::align_of::<T>()) {
        return false;
    }
    let h = p.cast::<u32>();
    unsafe { *h == 1 && *h.add(1) >= core::mem::size_of::<T>() as u32 }
}
const _: () = {
    assert!(core::mem::size_of::<Identity>() == 16);
};

/// # Safety
/// Resident Contract buffers retain their declared C prefixes through the
/// original operation. Admission and output publication preserve their order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_display_query(
    output: *mut a::GfxDriverDisplayApi,
) -> i32 {
    if let Err(code) = identity(1, true) {
        return code;
    }
    if output.is_null()
        || unsafe { *output.cast::<u32>() } != 1
        || unsafe { *output.cast::<u32>().add(1) } < 40
    {
        return a::gfx_output_error_invalid;
    }
    let bytes = (unsafe { *output.cast::<u32>().add(1) } as usize & !7)
        .min(core::mem::size_of::<a::GfxDriverDisplayApi>());
    let value = a::GfxDriverDisplayApi {
        size: bytes as u32,
        boot_info: r4os_rust_driver_gfx_display_boot_info as *const () as u64,
        prepare: r4os_rust_driver_gfx_display_prepare as *const () as u64,
        transition: r4os_rust_driver_gfx_display_transition as *const () as u64,
        schedule: r4os_rust_driver_gfx_display_schedule as *const () as u64,
        boot_hold: r4os_rust_driver_gfx_boot_hold as *const () as u64,
        boot_finish: r4os_rust_driver_gfx_boot_finish as *const () as u64,
        prepare_held: r4os_rust_driver_gfx_display_prepare_held as *const () as u64,
        presentation_stats: r4os_rust_driver_gfx_display_presentation_stats as *const () as u64,
        cursor_configure: r4os_rust_driver_gfx_cursor_configure as *const () as u64,
        cursor_take: r4os_rust_driver_gfx_cursor_take as *const () as u64,
        cursor_complete: r4os_rust_driver_gfx_cursor_complete as *const () as u64,
        presentation_info: r4os_rust_driver_gfx_display_presentation_info as *const () as u64,
        output_register: r4os_rust_driver_gfx_display_output_register as *const () as u64,
        output_transition: r4os_rust_driver_gfx_display_output_transition as *const () as u64,
        device_reset: r4os_rust_driver_gfx_display_device_reset as *const () as u64,
        prepare_reset: r4os_rust_driver_gfx_display_prepare_reset as *const () as u64,
        terminal_release: r4os_rust_driver_gfx_display_terminal_release as *const () as u64,
        resume_headless: r4os_rust_driver_gfx_display_resume_headless as *const () as u64,
        ..a::GfxDriverDisplayApi::default()
    };
    unsafe {
        ptr::copy_nonoverlapping(
            ptr::addr_of!(value).cast::<u8>(),
            output.cast::<u8>(),
            bytes,
        )
    };
    a::gfx_output_ok
}

/// # Safety
/// Resident Contract buffers retain their declared C prefixes through the
/// original operation. Admission and output publication preserve their order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_queue_query(
    output: *mut a::GfxDriverQueueApi,
) -> i32 {
    if let Err(code) = identity(0, true) {
        return code;
    }
    if output.is_null()
        || unsafe { *output.cast::<u32>() } != 1
        || unsafe { *output.cast::<u32>().add(1) } < 56
    {
        return a::gfx_queue_error_invalid;
    }
    let bytes = (unsafe { *output.cast::<u32>().add(1) } as usize & !7)
        .min(core::mem::size_of::<a::GfxDriverQueueApi>());
    let value = a::GfxDriverQueueApi {
        size: bytes as u32,
        register_backend: r4os_rust_driver_gfx_queue_register as *const () as u64,
        unregister_backend: r4os_rust_driver_gfx_queue_unregister as *const () as u64,
        take: r4os_rust_driver_gfx_queue_take as *const () as u64,
        complete: r4os_rust_driver_gfx_queue_complete as *const () as u64,
        reset: r4os_rust_driver_gfx_queue_reset as *const () as u64,
        segment: r4os_rust_driver_gfx_queue_segment as *const () as u64,
        retain_resource: r4os_rust_driver_gfx_queue_retain as *const () as u64,
        register_profile: r4os_rust_driver_gfx_queue_register_profile as *const () as u64,
        update_operations: r4os_rust_driver_gfx_queue_update_operations as *const () as u64,
        read_render_list: r4os_rust_driver_gfx_queue_read_render_list as *const () as u64,
        read_render_grid_list: r4os_rust_driver_gfx_queue_read_render_grid_list as *const () as u64,
        read_render_color_list: r4os_rust_driver_gfx_queue_read_render_color_list as *const ()
            as u64,
        publish_properties: r4os_rust_driver_gfx_queue_publish_properties as *const () as u64,
        read_native_info: r4os_rust_driver_gfx_queue_read_native_info as *const () as u64,
        read_native_data: r4os_rust_driver_gfx_queue_read_native_data as *const () as u64,
        read_native_binding: r4os_rust_driver_gfx_queue_read_native_binding as *const () as u64,
        queue_owner_info: r4os_rust_driver_gfx_queue_owner_info as *const () as u64,
        retain_scanout: r4os_rust_driver_gfx_queue_retain_scanout as *const () as u64,
        begin_scanout: r4os_rust_driver_gfx_queue_begin_scanout as *const () as u64,
        scanout_retire_requested: r4os_rust_driver_gfx_queue_scanout_retire_requested as *const ()
            as u64,
        ..a::GfxDriverQueueApi::default()
    };
    unsafe {
        ptr::copy_nonoverlapping(
            ptr::addr_of!(value).cast::<u8>(),
            output.cast::<u8>(),
            bytes,
        )
    };
    a::gfx_queue_ok
}

/// # Safety
/// Resident Contract buffers retain their declared C prefixes through the
/// original operation. Admission and output publication preserve their order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_memory_query(
    output: *mut a::GfxDriverMemoryApi,
) -> i32 {
    if output.is_null()
        || unsafe { *output.cast::<u32>() } != 1
        || unsafe { *output.cast::<u32>().add(1) } < 112
    {
        return a::gfx_buffer_error_invalid;
    }
    if let Err(code) = identity(0, true) {
        return code;
    }
    let bytes = (unsafe { *output.cast::<u32>().add(1) } as usize & !7)
        .min(core::mem::size_of::<a::GfxDriverMemoryApi>());
    let value = a::GfxDriverMemoryApi {
        size: bytes as u32,
        buffer_create: r4os_rust_driver_gfx_buffer_create as *const () as u64,
        buffer_describe: r4os_rust_driver_gfx_buffer_describe as *const () as u64,
        buffer_import: r4os_rust_driver_gfx_buffer_import as *const () as u64,
        buffer_release: r4os_rust_driver_gfx_buffer_release as *const () as u64,
        buffer_map: r4os_rust_driver_gfx_buffer_map as *const () as u64,
        buffer_unmap: r4os_rust_driver_gfx_buffer_unmap as *const () as u64,
        device_acquire: r4os_rust_driver_gfx_device_acquire as *const () as u64,
        device_segment: r4os_rust_driver_gfx_device_segment as *const () as u64,
        device_release: r4os_rust_driver_gfx_device_release as *const () as u64,
        mmio_map: if held() {
            r4os_rust_driver_gfx_mmio_map as *const () as u64
        } else {
            0
        },
        mmio_unmap: if held() {
            r4os_rust_driver_gfx_mmio_unmap as *const () as u64
        } else {
            0
        },
        collect: r4os_rust_driver_gfx_collect as *const () as u64,
        buffer_stats: r4os_rust_driver_gfx_memory_stats as *const () as u64,
        buffer_reserve: r4os_rust_driver_gfx_owned_reserve as *const () as u64,
        buffer_commit: r4os_rust_driver_gfx_owned_commit as *const () as u64,
        buffer_abort: r4os_rust_driver_gfx_owned_abort as *const () as u64,
        buffer_take_release: r4os_rust_driver_gfx_owned_take_release as *const () as u64,
        buffer_finish_release: r4os_rust_driver_gfx_owned_finish_release as *const () as u64,
        native_register: r4os_rust_driver_gfx_native_register as *const () as u64,
        native_unregister: r4os_rust_driver_gfx_native_unregister as *const () as u64,
        native_take: r4os_rust_driver_gfx_native_take as *const () as u64,
        native_complete: r4os_rust_driver_gfx_native_complete as *const () as u64,
        virtual_register: r4os_rust_driver_gfx_virtual_register as *const () as u64,
        virtual_unregister: r4os_rust_driver_gfx_virtual_unregister as *const () as u64,
        virtual_take: r4os_rust_driver_gfx_virtual_take as *const () as u64,
        virtual_complete: r4os_rust_driver_gfx_virtual_complete as *const () as u64,
        memory_budget: r4os_rust_driver_gfx_memory_budget as *const () as u64,
        telemetry_exchange: r4os_rust_driver_gfx_telemetry_exchange as *const () as u64,
        device_lost: r4os_rust_driver_gfx_device_lost as *const () as u64,
        reserved_span: r4os_rust_driver_gfx_reserved_span as *const () as u64,
        unmanaged_span: r4os_rust_driver_gfx_unmanaged_span as *const () as u64,
        ..a::GfxDriverMemoryApi::default()
    };
    unsafe {
        ptr::copy_nonoverlapping(
            ptr::addr_of!(value).cast::<u8>(),
            output.cast::<u8>(),
            bytes,
        )
    };
    a::gfx_buffer_result_ok
}

unsafe fn driver_buffer_identity(owner: u32, admit: bool, out: *mut Identity) -> i32 {
    match crate::gfx::driver_memory::owner(owner, admit) {
        Ok(id) => {
            unsafe {
                out.write(Identity {
                    id: id.id,
                    generation: id.generation,
                })
            };
            0
        }
        Err(e) => crate::gfx::buffer_api::status(e),
    }
}

// Private operation modules share the sole state and original typed owners.
mod display;
mod memory;
mod outputs;
mod queue;
pub use display::{
    r4os_rust_driver_gfx_boot_finish, r4os_rust_driver_gfx_boot_hold,
    r4os_rust_driver_gfx_cursor_complete, r4os_rust_driver_gfx_cursor_configure,
    r4os_rust_driver_gfx_cursor_take, r4os_rust_driver_gfx_display_boot_info,
    r4os_rust_driver_gfx_display_device_reset, r4os_rust_driver_gfx_display_output_register,
    r4os_rust_driver_gfx_display_output_transition, r4os_rust_driver_gfx_display_prepare,
    r4os_rust_driver_gfx_display_prepare_held, r4os_rust_driver_gfx_display_prepare_reset,
    r4os_rust_driver_gfx_display_presentation_info,
    r4os_rust_driver_gfx_display_presentation_stats, r4os_rust_driver_gfx_display_resume_headless,
    r4os_rust_driver_gfx_display_schedule, r4os_rust_driver_gfx_display_terminal_release,
    r4os_rust_driver_gfx_display_transition,
};
pub use memory::{
    r4os_rust_driver_gfx_buffer_create, r4os_rust_driver_gfx_buffer_describe,
    r4os_rust_driver_gfx_buffer_import, r4os_rust_driver_gfx_buffer_map,
    r4os_rust_driver_gfx_buffer_release, r4os_rust_driver_gfx_buffer_unmap,
    r4os_rust_driver_gfx_collect, r4os_rust_driver_gfx_device_acquire,
    r4os_rust_driver_gfx_device_lost, r4os_rust_driver_gfx_device_release,
    r4os_rust_driver_gfx_device_segment, r4os_rust_driver_gfx_memory_budget,
    r4os_rust_driver_gfx_memory_stats, r4os_rust_driver_gfx_mmio_map,
    r4os_rust_driver_gfx_mmio_unmap, r4os_rust_driver_gfx_native_complete,
    r4os_rust_driver_gfx_native_register, r4os_rust_driver_gfx_native_take,
    r4os_rust_driver_gfx_native_unregister, r4os_rust_driver_gfx_owned_abort,
    r4os_rust_driver_gfx_owned_commit, r4os_rust_driver_gfx_owned_finish_release,
    r4os_rust_driver_gfx_owned_reserve, r4os_rust_driver_gfx_owned_take_release,
    r4os_rust_driver_gfx_reserved_span, r4os_rust_driver_gfx_telemetry_exchange,
    r4os_rust_driver_gfx_unmanaged_span, r4os_rust_driver_gfx_virtual_complete,
    r4os_rust_driver_gfx_virtual_register, r4os_rust_driver_gfx_virtual_take,
    r4os_rust_driver_gfx_virtual_unregister,
};
pub use outputs::{
    r4os_rust_driver_gfx_audio_publish, r4os_rust_driver_gfx_audio_query,
    r4os_rust_driver_gfx_brightness_read, r4os_rust_driver_gfx_mode_complete,
    r4os_rust_driver_gfx_mode_enable, r4os_rust_driver_gfx_mode_read_color,
    r4os_rust_driver_gfx_mode_restore, r4os_rust_driver_gfx_mode_status,
    r4os_rust_driver_gfx_mode_take, r4os_rust_driver_gfx_output_brightness_publish,
    r4os_rust_driver_gfx_output_color_publish, r4os_rust_driver_gfx_output_pause,
    r4os_rust_driver_gfx_output_power_publish, r4os_rust_driver_gfx_output_publish,
    r4os_rust_driver_gfx_output_query, r4os_rust_driver_gfx_output_refresh_publish,
    r4os_rust_driver_gfx_output_withdraw, r4os_rust_driver_gfx_power_read,
    r4os_rust_driver_gfx_receiver_close, r4os_rust_driver_gfx_receiver_register,
    r4os_rust_driver_gfx_receiver_replace, r4os_rust_driver_gfx_refresh_read,
};
pub use queue::{
    r4os_rust_driver_gfx_queue_begin_scanout, r4os_rust_driver_gfx_queue_complete,
    r4os_rust_driver_gfx_queue_owner_info, r4os_rust_driver_gfx_queue_publish_properties,
    r4os_rust_driver_gfx_queue_read_native_binding, r4os_rust_driver_gfx_queue_read_native_data,
    r4os_rust_driver_gfx_queue_read_native_info, r4os_rust_driver_gfx_queue_read_render_color_list,
    r4os_rust_driver_gfx_queue_read_render_grid_list, r4os_rust_driver_gfx_queue_read_render_list,
    r4os_rust_driver_gfx_queue_register, r4os_rust_driver_gfx_queue_register_profile,
    r4os_rust_driver_gfx_queue_reset, r4os_rust_driver_gfx_queue_retain,
    r4os_rust_driver_gfx_queue_retain_scanout, r4os_rust_driver_gfx_queue_scanout_retire_requested,
    r4os_rust_driver_gfx_queue_segment, r4os_rust_driver_gfx_queue_take,
    r4os_rust_driver_gfx_queue_unregister, r4os_rust_driver_gfx_queue_update_operations,
};
