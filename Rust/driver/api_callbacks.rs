//! Canonical DriverApi C callbacks. No device or admission state.
use crate::contract::*;

pub(super) unsafe extern "C" fn driver_monotonic_clock(out: *mut MonotonicClockInfo) -> i32 {
    unsafe { crate::platform::monotonic::r4os_rust_monotonic_clock_api(out) }
}

pub(super) unsafe extern "C" fn driver_dma_sync_range_for_device(
    mapping: *const DmaMapping,
    offset: u32,
    bytes: u32,
) -> i32 {
    unsafe {
        crate::driver::api_dma::r4os_rust_driver_dma_sync(mapping, false, true, offset, bytes)
    }
}

pub(super) unsafe extern "C" fn driver_dma_sync_range_for_cpu(
    mapping: *const DmaMapping,
    offset: u32,
    bytes: u32,
) -> i32 {
    unsafe { crate::driver::api_dma::r4os_rust_driver_dma_sync(mapping, true, true, offset, bytes) }
}

pub(super) unsafe extern "C" fn driver_driver_work_submit_owned(
    handler: unsafe extern "C" fn(usize) -> i32,
    context: usize,
    out: *mut u32,
) -> i32 {
    unsafe {
        crate::driver::api_admission::r4os_rust_driver_api_work_owned(handler, context, out.cast())
    }
}

pub(super) unsafe extern "C" fn driver_log_info(text: *const u8) {
    unsafe { crate::driver::api_entry::r4os_rust_driver_log(0, text.cast()) };
}

pub(super) unsafe extern "C" fn driver_log_warn(text: *const u8) {
    unsafe { crate::driver::api_entry::r4os_rust_driver_log(1, text.cast()) };
}

pub(super) unsafe extern "C" fn driver_log_error(text: *const u8) {
    unsafe { crate::driver::api_entry::r4os_rust_driver_log(2, text.cast()) };
}

pub(super) unsafe extern "C" fn driver_free_dma_buffer(physical: u64, bytes: u32) {
    let _ = bytes;
    crate::driver::api_dma::r4os_rust_driver_dma_free_physical(physical);
}

pub(super) unsafe extern "C" fn driver_request_irq(
    irq: u8,
    handler: *const core::ffi::c_void,
) -> i32 {
    let _ = (irq, handler);
    crate::driver::api_entry::r4os_rust_driver_irq_legacy(false)
}

pub(super) unsafe extern "C" fn driver_release_irq(irq: u8) -> i32 {
    let _ = irq;
    crate::driver::api_entry::r4os_rust_driver_irq_legacy(true)
}

pub(super) unsafe extern "C" fn driver_register_audio_backend(
    name: *const u8,
    backend: *const core::ffi::c_void,
) -> i32 {
    unsafe { crate::audio::core::register_audio_backend_z(name, backend) }
}

pub(super) unsafe extern "C" fn driver_register_storage_backend(
    name: *const u8,
    backend: *const core::ffi::c_void,
) -> i32 {
    unsafe { crate::driver::api_storage::r4os_rust_driver_storage_register(name, backend.cast()) }
}

pub(super) unsafe extern "C" fn driver_register_input_backend(
    name: *const u8,
    backend: *const core::ffi::c_void,
) -> i32 {
    let _ = backend;
    unsafe { crate::driver::api_entry::r4os_rust_driver_input(name) }
}

pub(super) unsafe extern "C" fn driver_register_synth_engine(
    name: *const u8,
    engine: *const core::ffi::c_void,
) -> i32 {
    unsafe { crate::audio::core::register_synth_engine_z(name, engine) }
}

pub(super) unsafe extern "C" fn driver_register_mixer_backend(
    name: *const u8,
    backend: *const core::ffi::c_void,
) -> i32 {
    unsafe { crate::audio::core::register_mixer_backend_z(name, backend) }
}

pub(super) unsafe extern "C" fn driver_register_net_backend(
    name: *const u8,
    descriptor: *const core::ffi::c_void,
) -> i32 {
    unsafe { crate::driver::api_net::r4os_rust_driver_net_register(name, descriptor.cast()) }
}

pub(super) unsafe extern "C" fn driver_alloc_dma_region(
    bytes: u32,
    alignment: u32,
    out: *mut DmaBuffer,
) -> i32 {
    unsafe {
        crate::driver::api_dma::r4os_rust_driver_dma_allocate(bytes, alignment, u64::MAX, out)
    }
}

pub(super) unsafe extern "C" fn driver_pci_read_bar(
    kind: u8,
    bus: u8,
    device: u8,
    function: u8,
    index: u8,
) -> u32 {
    {
        crate::driver::api_platform::r4os_rust_driver_pci_bar(
            kind, bus, device, function, index, false,
        )
    }
    .try_into()
    .expect("checked callback result")
}

pub(super) unsafe extern "C" fn driver_irq_register(
    irq: u8,
    handler: unsafe extern "C" fn(u8, usize) -> u32,
    context: usize,
    flags: u32,
) -> i32 {
    unsafe { crate::driver::api_entry::r4os_rust_driver_irq_register(irq, handler, context, flags) }
}

pub(super) unsafe extern "C" fn driver_irq_unregister(
    irq: u8,
    handler: unsafe extern "C" fn(u8, usize) -> u32,
    context: usize,
) -> i32 {
    unsafe { crate::driver::irq_router::r4os_rust_irq_router_unregister(irq, handler, context) }
}

pub(super) unsafe extern "C" fn driver_irq_stats(irq: u8, out: *mut IrqStats) -> i32 {
    unsafe { crate::driver::irq_router::r4os_rust_irq_router_stats(irq, false, out.cast()) }
}

pub(super) unsafe extern "C" fn driver_register_audio_output_backend(
    name: *const u8,
    backend: *const core::ffi::c_void,
) -> i32 {
    unsafe { crate::driver::api_audio::r4os_rust_driver_audio_register(name, backend.cast()) }
}

pub(super) unsafe extern "C" fn driver_tick_count() -> u64 {
    crate::platform::timer::tick_count()
}

pub(super) unsafe extern "C" fn driver_timer_frequency() -> u32 {
    crate::platform::timer::frequency()
}

pub(super) unsafe extern "C" fn driver_storage_backend_recovery_begin(name: *const u8) -> i32 {
    unsafe { crate::driver::api_storage::r4os_rust_driver_storage_recovery(name.cast(), false, 0) }
}

pub(super) unsafe extern "C" fn driver_storage_backend_recovery_finish(
    name: *const u8,
    ok: i32,
) -> i32 {
    unsafe { crate::driver::api_storage::r4os_rust_driver_storage_recovery(name.cast(), true, ok) }
}

pub(super) unsafe extern "C" fn driver_driver_work_submit(
    handler: unsafe extern "C" fn(usize) -> i32,
    context: usize,
    flags: u32,
    out: *mut u32,
) -> i32 {
    unsafe {
        crate::driver::api_admission::r4os_rust_driver_api_work_submit(
            handler,
            context,
            flags,
            out.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn driver_driver_work_cancel(handle: u32) -> i32 {
    crate::driver::work::r4os_rust_driver_work_cancel(handle)
}

pub(super) unsafe extern "C" fn driver_driver_completion_wait(
    handle: u32,
    timeout_ticks: u64,
    out_result: *mut i32,
) -> i32 {
    unsafe { crate::driver::work::r4os_rust_driver_work_wait(handle, timeout_ticks, out_result) }
}

pub(super) unsafe extern "C" fn driver_driver_completion_status(
    handle: u32,
    out: *mut DriverCompletionStatus,
) -> i32 {
    unsafe { crate::driver::work::r4os_rust_driver_work_status(handle, out.cast()) }
}

pub(super) unsafe extern "C" fn driver_driver_completion_release(handle: u32) -> i32 {
    crate::driver::work::r4os_rust_driver_work_release(handle)
}

pub(super) unsafe extern "C" fn driver_driver_work_summary(out: *mut DriverWorkSummary) -> i32 {
    unsafe {
        crate::driver::work::r4os_rust_driver_work_summary(out.cast());
    }
    0
}

pub(super) unsafe extern "C" fn driver_protocol_dispatch(
    role: *const u8,
    op: u32,
    in_buffer: *const ProtocolBuffer,
    out_buffer: *mut ProtocolBuffer,
) -> i32 {
    unsafe {
        let mut len = 0;
        while role.add(len).read() != 0 {
            len += 1;
        }
        crate::loader::r4p::r4os_rust_r4p_dispatch(
            crate::fs::vfs_types::Bytes { data: role, len },
            op,
            in_buffer,
            out_buffer,
        )
    }
}

pub(super) unsafe extern "C" fn driver_dma_sync_for_device(mapping: *const DmaMapping) -> i32 {
    unsafe { crate::driver::api_dma::r4os_rust_driver_dma_sync(mapping.cast(), false, false, 0, 0) }
}

pub(super) unsafe extern "C" fn driver_dma_sync_for_cpu(mapping: *const DmaMapping) -> i32 {
    unsafe { crate::driver::api_dma::r4os_rust_driver_dma_sync(mapping.cast(), true, false, 0, 0) }
}

pub(super) unsafe extern "C" fn driver_register_display_blit_backend(
    name: *const u8,
    d: *const DisplayBlitBackend,
) -> i32 {
    unsafe { crate::driver::api_entry::r4os_rust_driver_blit(true, name.cast(), d.cast()) }
}

pub(super) unsafe extern "C" fn driver_unregister_display_blit_backend(name: *const u8) -> i32 {
    unsafe {
        crate::driver::api_entry::r4os_rust_driver_blit(false, name.cast(), core::ptr::null())
    }
}

pub(super) fn template() -> DriverApi {
    DriverApi {
        magic: driver_magic,
        version: driver_api_version,
        size: core::mem::size_of::<DriverApi>() as u32,
        reserved: 0,
        gfx_memory_query: Some(crate::driver::api_gfx::r4os_rust_driver_gfx_memory_query),
        gfx_queue_query: Some(crate::driver::api_gfx::r4os_rust_driver_gfx_queue_query),
        gfx_output_query: Some(crate::driver::api_gfx::r4os_rust_driver_gfx_output_query),
        gfx_display_query: Some(crate::driver::api_gfx::r4os_rust_driver_gfx_display_query),
        resource_query: Some(crate::driver::api_resource::r4os_rust_driver_resource_api),
        heap_query: Some(crate::driver::api_runtime::r4os_rust_driver_heap_api),
        monotonic_clock: Some(super::api_callbacks::driver_monotonic_clock),
        thread_query: Some(crate::driver::api_runtime::r4os_rust_driver_thread_api),
        semaphore_query: Some(crate::driver::api_runtime::r4os_rust_driver_semaphore_api),
        dma_sync_range_for_device: Some(super::api_callbacks::driver_dma_sync_range_for_device),
        dma_sync_range_for_cpu: Some(super::api_callbacks::driver_dma_sync_range_for_cpu),
        driver_work_submit_owned: Some(super::api_callbacks::driver_driver_work_submit_owned),
        log_info: super::api_callbacks::driver_log_info,
        log_warn: super::api_callbacks::driver_log_warn,
        log_error: super::api_callbacks::driver_log_error,
        port_inb: crate::driver::api_platform::r4os_rust_driver_port_inb,
        port_outb: crate::driver::api_platform::r4os_rust_driver_port_outb,
        alloc_dma_buffer: crate::driver::api_dma::r4os_rust_driver_dma_allocate_legacy,
        free_dma_buffer: super::api_callbacks::driver_free_dma_buffer,
        request_irq: super::api_callbacks::driver_request_irq,
        release_irq: super::api_callbacks::driver_release_irq,
        get_option: crate::driver::api_options::r4os_rust_driver_option,
        register_audio_backend: super::api_callbacks::driver_register_audio_backend,
        register_storage_backend: super::api_callbacks::driver_register_storage_backend,
        register_input_backend: super::api_callbacks::driver_register_input_backend,
        register_synth_engine: super::api_callbacks::driver_register_synth_engine,
        register_mixer_backend: super::api_callbacks::driver_register_mixer_backend,
        register_net_backend: super::api_callbacks::driver_register_net_backend,
        alloc_dma_region: super::api_callbacks::driver_alloc_dma_region,
        free_dma_region: crate::driver::api_dma::r4os_rust_driver_dma_free,
        pci_device_count: crate::driver::api_platform::r4os_rust_driver_pci_count,
        pci_device_at: crate::driver::api_platform::r4os_rust_driver_pci_at,
        pci_find_by_class: crate::driver::api_platform::r4os_rust_driver_pci_find,
        pci_read_config32: crate::driver::api_platform::r4os_rust_driver_pci_read,
        pci_write_config32: crate::driver::api_platform::r4os_rust_driver_pci_write,
        pci_read_bar: super::api_callbacks::driver_pci_read_bar,
        pci_enable_bus_master: crate::driver::api_platform::r4os_rust_driver_pci_bus_master,
        irq_register: super::api_callbacks::driver_irq_register,
        irq_unregister: super::api_callbacks::driver_irq_unregister,
        irq_stats: super::api_callbacks::driver_irq_stats,
        pci_map_bar: crate::driver::api_platform::r4os_rust_driver_pci_map,
        port_inw: crate::driver::api_platform::r4os_rust_driver_port_inw,
        port_outw: crate::driver::api_platform::r4os_rust_driver_port_outw,
        port_inl: crate::driver::api_platform::r4os_rust_driver_port_inl,
        port_outl: crate::driver::api_platform::r4os_rust_driver_port_outl,
        net_receive_frame: crate::driver::api_net::r4os_rust_driver_net_receive,
        register_audio_output_backend: super::api_callbacks::driver_register_audio_output_backend,
        unregister_audio_backend: crate::driver::api_audio::r4os_rust_driver_audio_unregister,
        tick_count: super::api_callbacks::driver_tick_count,
        timer_frequency: super::api_callbacks::driver_timer_frequency,
        wait_ticks: crate::driver::api_platform::r4os_rust_driver_wait_ticks,
        register_synth_engine_v2: crate::driver::api_entry::r4os_rust_driver_synth,
        unregister_storage_backend: crate::driver::api_storage::r4os_rust_driver_storage_unregister,
        storage_backend_recovery_begin: super::api_callbacks::driver_storage_backend_recovery_begin,
        storage_backend_recovery_finish:
            super::api_callbacks::driver_storage_backend_recovery_finish,
        register_usb_host_controller: crate::driver::api_entry::r4os_rust_driver_usb_register,
        unregister_usb_host_controller: crate::driver::api_entry::r4os_rust_driver_usb_unregister,
        driver_work_submit: super::api_callbacks::driver_driver_work_submit,
        driver_work_cancel: super::api_callbacks::driver_driver_work_cancel,
        driver_completion_wait: super::api_callbacks::driver_driver_completion_wait,
        driver_completion_status: super::api_callbacks::driver_driver_completion_status,
        driver_completion_release: super::api_callbacks::driver_driver_completion_release,
        driver_work_summary: super::api_callbacks::driver_driver_work_summary,
        pci_enable_msi: crate::driver::api_msi::r4os_rust_driver_msi_enable,
        alloc_dma_region_constrained: crate::driver::api_dma::r4os_rust_driver_dma_allocate,
        pci_disable_msi: crate::driver::api_msi::r4os_rust_driver_msi_disable,
        protocol_dispatch: Some(super::api_callbacks::driver_protocol_dispatch),
        dma_pin_buffer: crate::driver::api_dma::r4os_rust_driver_dma_pin,
        dma_map_pinned: crate::driver::api_dma::r4os_rust_driver_dma_map,
        dma_sync_for_device: super::api_callbacks::driver_dma_sync_for_device,
        dma_sync_for_cpu: super::api_callbacks::driver_dma_sync_for_cpu,
        dma_unmap: crate::driver::api_dma::r4os_rust_driver_dma_unmap,
        dma_unpin_buffer: crate::driver::api_dma::r4os_rust_driver_dma_unpin,
        driver_work_submit_request: crate::driver::api_entry::r4os_rust_driver_api_work_request,
        activate_usb_host_controller: crate::driver::api_entry::r4os_rust_driver_usb_activate,
        register_display_blit_backend: super::api_callbacks::driver_register_display_blit_backend,
        unregister_display_blit_backend:
            super::api_callbacks::driver_unregister_display_blit_backend,
        net_schedule_rx: crate::driver::api_net::r4os_rust_driver_net_schedule,
        net_backend_query: crate::driver::api_net::r4os_rust_driver_net_query,
        net_receive_packet: crate::driver::api_net::r4os_rust_driver_net_receive_packet,
    }
}
