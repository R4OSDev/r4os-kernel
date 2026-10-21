//! Exact per-device persist/load and enumeration reset records. Configuration
//! acknowledgements travel with the device runtime, including identical values.
use super::xhci_state::Context;
use super::{types::Interface, xhci_state::state, xhci_types::DeviceRuntime};
use crate::sched::sync_types::Text;
use core::ptr;
pub(super) unsafe fn persist_active_runtime(ctx: Context) {
    unsafe {
        let s = state(ctx);
        let Some(index) = (*s).active_runtime_index else {
            return;
        };
        if index >= 8 || !(*s).runtimes[index].active {
            return;
        }
        let rt = ptr::addr_of_mut!((*s).runtimes[index]);
        (*rt).slot_id = (*s).current.addressed_slot_id;
        (*rt).port = (*s).current.addressed_port;
        (*rt).speed = (*s).current.addressed_speed;
        (*rt).config_value = (*s).current.config_value;
        (*rt).set_configuration_attempted = (*s).current.set_configuration_attempted;
        (*rt).set_configuration_ok = (*s).current.set_configuration_ok;
        (*rt).vendor_id = (*s).current.device_vendor_id;
        (*rt).product_id = (*s).current.device_product_id;
        (*rt).device_context_phys = (*s).current.device_context_phys;
        (*rt).input_context_phys = (*s).current.input_context_phys;
        (*rt).ep0_ring_phys = (*s).current.ep0_ring_phys;
        (*rt).interrupt_ring_phys = (*s).current.interrupt_ring_phys;
        (*rt).interrupt_buffer_phys = (*s).current.interrupt_buffer_phys;
        (*rt).bulk_in_ring_phys = (*s).current.bulk_in_ring_phys;
        (*rt).bulk_out_ring_phys = (*s).current.bulk_out_ring_phys;
        (*rt).bulk_buffer_phys = (*s).current.bulk_buffer_phys;
        (*rt).bulk_buffer_frames = (*s).current.bulk_buffer_frames;
        (*rt).descriptor_phys = (*s).current.descriptor_phys;
        (*rt).device_virt = (*s).first_device_virt;
        (*rt).input_virt = (*s).first_input_virt;
        (*rt).ep0_ring_virt = (*s).first_ep0_ring_virt;
        (*rt).interrupt_ring_virt = (*s).first_interrupt_ring_virt;
        (*rt).interrupt_buffer_virt = (*s).first_interrupt_buffer_virt;
        (*rt).bulk_in_ring_virt = (*s).first_bulk_in_ring_virt;
        (*rt).bulk_out_ring_virt = (*s).first_bulk_out_ring_virt;
        (*rt).bulk_buffer_virt = (*s).first_bulk_buffer_virt;
        (*rt).descriptor_virt = (*s).first_descriptor_virt;
        (*rt).ep0_enqueue = (*s).current.ep0_enqueue;
        (*rt).ep0_cycle = (*s).current.ep0_cycle;
        (*rt).control_endpoint_faulted = (*s).current.control_endpoint_faulted;
        (*rt).interrupt_enqueue = (*s).current.interrupt_enqueue;
        (*rt).interrupt_cycle = (*s).current.interrupt_cycle;
        (*rt).interrupt_endpoint_id = (*s).current.interrupt_endpoint_id;
        (*rt).interrupt_endpoint_address = (*s).current.interrupt_endpoint_address;
        (*rt).interrupt_endpoint_max_packet = (*s).current.interrupt_endpoint_max_packet;
        (*rt).interrupt_endpoint_interval_raw = (*s).current.interrupt_endpoint_interval_raw;
        (*rt).interrupt_endpoint_interval_context =
            (*s).current.interrupt_endpoint_interval_context;
        (*rt).interrupt_endpoint_configured = (*s).current.interrupt_endpoint_configured;
        (*rt).interrupt_endpoint_faulted = (*s).current.interrupt_endpoint_faulted;
        (*rt).interrupt_pending = (*s).current.interrupt_pending;
        (*rt).interrupt_pending_trb_phys = (*s).current.interrupt_pending_trb_phys;
        (*rt).interrupt_transfer_handle = (*s).current.interrupt_transfer_handle;
        (*rt).interrupt_pending_streak = (*s).current.interrupt_pending_streak;
        (*rt).last_interrupt_ep_state = (*s).current.last_interrupt_ep_state;
        (*rt).last_interrupt_request_len = (*s).current.last_interrupt_request_len;
        (*rt).last_interrupt_residue = (*s).current.last_interrupt_residue;
        (*rt).last_interrupt_actual_len = (*s).current.last_interrupt_actual_len;
        (*rt).last_interrupt_completion_code = (*s).current.last_interrupt_completion_code;
        (*rt).bulk_endpoints_configured = (*s).current.bulk_endpoints_configured;
        (*rt).bulk_endpoints_faulted = (*s).current.bulk_endpoints_faulted;
        (*rt).bulk_in_endpoint_id = (*s).current.bulk_in_endpoint_id;
        (*rt).bulk_in_endpoint_address = (*s).current.bulk_in_endpoint_address;
        (*rt).bulk_in_endpoint_max_packet = (*s).current.bulk_in_endpoint_max_packet;
        (*rt).bulk_in_endpoint_max_burst = (*s).current.bulk_in_endpoint_max_burst;
        (*rt).bulk_out_endpoint_id = (*s).current.bulk_out_endpoint_id;
        (*rt).bulk_out_endpoint_address = (*s).current.bulk_out_endpoint_address;
        (*rt).bulk_out_endpoint_max_packet = (*s).current.bulk_out_endpoint_max_packet;
        (*rt).bulk_out_endpoint_max_burst = (*s).current.bulk_out_endpoint_max_burst;
        (*rt).bulk_in_enqueue = (*s).current.bulk_in_enqueue;
        (*rt).bulk_in_cycle = (*s).current.bulk_in_cycle;
        (*rt).bulk_in_link_update_pending = (*s).current.bulk_in_link_update_pending;
        (*rt).bulk_out_enqueue = (*s).current.bulk_out_enqueue;
        (*rt).bulk_out_cycle = (*s).current.bulk_out_cycle;
        (*rt).bulk_out_link_update_pending = (*s).current.bulk_out_link_update_pending;
        (*rt).last_control_request_type = (*s).current.last_control_request_type;
        (*rt).last_control_request = (*s).current.last_control_request;
        (*rt).last_control_value = (*s).current.last_control_value;
        (*rt).last_control_index = (*s).current.last_control_index;
        (*rt).last_control_length = (*s).current.last_control_length;
        (*rt).last_control_direction = (*s).current.last_control_direction;
        (*rt).last_control_completion_code = (*s).current.last_control_completion_code;
        (*rt).last_control_residue = (*s).current.last_control_residue;
        (*rt).last_control_ok = (*s).current.last_control_ok;
        (*rt).last_bulk_direction = (*s).current.last_bulk_direction;
        (*rt).last_bulk_result = (*s).current.last_bulk_result;
        (*rt).last_bulk_request_len = (*s).current.last_bulk_request_len;
        (*rt).last_bulk_residue = (*s).current.last_bulk_residue;
        (*rt).last_bulk_actual_len = (*s).current.last_bulk_actual_len;
        (*rt).last_bulk_completion_code = (*s).current.last_bulk_completion_code;
        (*rt).last_wait_cancelled = (*s).current.last_wait_cancelled;
        (*rt).sync_transfer_incident = (*s).last_sync_transfer_incident;
    }
}
pub(super) unsafe fn load_runtime(ctx: Context, index: usize) {
    unsafe {
        let s = state(ctx);
        let rt = ptr::addr_of!((*s).runtimes[index]);

        (*s).current.addressed_slot_id = (*rt).slot_id;
        (*s).current.addressed_port = (*rt).port;
        (*s).current.addressed_speed = (*rt).speed;
        (*s).current.config_value = (*rt).config_value;
        (*s).current.set_configuration_attempted = (*rt).set_configuration_attempted;
        (*s).current.set_configuration_ok = (*rt).set_configuration_ok;
        (*s).current.device_vendor_id = (*rt).vendor_id;
        (*s).current.device_product_id = (*rt).product_id;
        (*s).current.get_config_ok = (*rt).config_value != 0;
        (*s).current.device_context_phys = (*rt).device_context_phys;
        (*s).current.input_context_phys = (*rt).input_context_phys;
        (*s).current.ep0_ring_phys = (*rt).ep0_ring_phys;
        (*s).current.interrupt_ring_phys = (*rt).interrupt_ring_phys;
        (*s).current.interrupt_buffer_phys = (*rt).interrupt_buffer_phys;
        (*s).current.bulk_in_ring_phys = (*rt).bulk_in_ring_phys;
        (*s).current.bulk_out_ring_phys = (*rt).bulk_out_ring_phys;
        (*s).current.bulk_buffer_phys = (*rt).bulk_buffer_phys;
        (*s).current.bulk_buffer_frames = (*rt).bulk_buffer_frames;
        (*s).current.descriptor_phys = (*rt).descriptor_phys;
        (*s).first_device_virt = (*rt).device_virt;
        (*s).first_input_virt = (*rt).input_virt;
        (*s).first_ep0_ring_virt = (*rt).ep0_ring_virt;
        (*s).first_interrupt_ring_virt = (*rt).interrupt_ring_virt;
        (*s).first_interrupt_buffer_virt = (*rt).interrupt_buffer_virt;
        (*s).first_bulk_in_ring_virt = (*rt).bulk_in_ring_virt;
        (*s).first_bulk_out_ring_virt = (*rt).bulk_out_ring_virt;
        (*s).first_bulk_buffer_virt = (*rt).bulk_buffer_virt;
        (*s).first_descriptor_virt = (*rt).descriptor_virt;
        (*s).current.ep0_enqueue = (*rt).ep0_enqueue;
        (*s).current.ep0_cycle = (*rt).ep0_cycle;
        (*s).current.control_endpoint_faulted = (*rt).control_endpoint_faulted;
        (*s).current.interrupt_enqueue = (*rt).interrupt_enqueue;
        (*s).current.interrupt_cycle = (*rt).interrupt_cycle;
        (*s).current.interrupt_endpoint_id = (*rt).interrupt_endpoint_id;
        (*s).current.interrupt_endpoint_address = (*rt).interrupt_endpoint_address;
        (*s).current.interrupt_endpoint_max_packet = (*rt).interrupt_endpoint_max_packet;
        (*s).current.interrupt_endpoint_interval_raw = (*rt).interrupt_endpoint_interval_raw;
        (*s).current.interrupt_endpoint_interval_context =
            (*rt).interrupt_endpoint_interval_context;
        (*s).current.interrupt_endpoint_configured = (*rt).interrupt_endpoint_configured;
        (*s).current.interrupt_endpoint_faulted = (*rt).interrupt_endpoint_faulted;
        (*s).current.interrupt_pending = (*rt).interrupt_pending;
        (*s).current.interrupt_transfer_handle = (*rt).interrupt_transfer_handle;
        (*s).current.interrupt_pending_trb_phys = (*rt).interrupt_pending_trb_phys;
        (*s).current.interrupt_pending_streak = (*rt).interrupt_pending_streak;
        (*s).current.last_interrupt_ep_state = (*rt).last_interrupt_ep_state;
        (*s).current.last_interrupt_request_len = (*rt).last_interrupt_request_len;
        (*s).current.last_interrupt_residue = (*rt).last_interrupt_residue;
        (*s).current.last_interrupt_actual_len = (*rt).last_interrupt_actual_len;
        (*s).current.last_interrupt_completion_code = (*rt).last_interrupt_completion_code;
        (*s).current.bulk_endpoints_configured = (*rt).bulk_endpoints_configured;
        (*s).current.bulk_endpoints_faulted = (*rt).bulk_endpoints_faulted;
        (*s).current.bulk_in_endpoint_id = (*rt).bulk_in_endpoint_id;
        (*s).current.bulk_in_endpoint_address = (*rt).bulk_in_endpoint_address;
        (*s).current.bulk_in_endpoint_max_packet = (*rt).bulk_in_endpoint_max_packet;
        (*s).current.bulk_in_endpoint_max_burst = (*rt).bulk_in_endpoint_max_burst;
        (*s).current.bulk_out_endpoint_id = (*rt).bulk_out_endpoint_id;
        (*s).current.bulk_out_endpoint_address = (*rt).bulk_out_endpoint_address;
        (*s).current.bulk_out_endpoint_max_packet = (*rt).bulk_out_endpoint_max_packet;
        (*s).current.bulk_out_endpoint_max_burst = (*rt).bulk_out_endpoint_max_burst;
        (*s).current.bulk_in_enqueue = (*rt).bulk_in_enqueue;
        (*s).current.bulk_in_cycle = (*rt).bulk_in_cycle;
        (*s).current.bulk_in_link_update_pending = (*rt).bulk_in_link_update_pending;
        (*s).current.bulk_out_enqueue = (*rt).bulk_out_enqueue;
        (*s).current.bulk_out_cycle = (*rt).bulk_out_cycle;
        (*s).current.bulk_out_link_update_pending = (*rt).bulk_out_link_update_pending;
        (*s).current.last_control_request_type = (*rt).last_control_request_type;
        (*s).current.last_control_request = (*rt).last_control_request;
        (*s).current.last_control_value = (*rt).last_control_value;
        (*s).current.last_control_index = (*rt).last_control_index;
        (*s).current.last_control_length = (*rt).last_control_length;
        (*s).current.last_control_direction = (*rt).last_control_direction;
        (*s).current.last_control_completion_code = (*rt).last_control_completion_code;
        (*s).current.last_control_residue = (*rt).last_control_residue;
        (*s).current.last_control_ok = (*rt).last_control_ok;
        (*s).current.last_bulk_direction = (*rt).last_bulk_direction;
        (*s).current.last_bulk_result = (*rt).last_bulk_result;
        (*s).current.last_bulk_request_len = (*rt).last_bulk_request_len;
        (*s).current.last_bulk_residue = (*rt).last_bulk_residue;
        (*s).current.last_bulk_actual_len = (*rt).last_bulk_actual_len;
        (*s).current.last_bulk_completion_code = (*rt).last_bulk_completion_code;
        (*s).current.last_wait_cancelled = (*rt).last_wait_cancelled;
        (*s).last_sync_transfer_incident = (*rt).sync_transfer_incident;
    }
}
pub(super) unsafe fn reset_first_enumeration_fields(ctx: Context) {
    unsafe {
        let s = state(ctx);

        (*s).current.address_device_attempted = false;
        (*s).current.address_device_ok = false;
        (*s).current.get_descriptor_attempted = false;
        (*s).current.get_descriptor_ok = false;
        (*s).current.get_config_attempted = false;
        (*s).current.get_config_ok = false;
        (*s).current.get_strings_attempted = false;
        (*s).current.get_strings_ok = false;
        (*s).current.set_configuration_attempted = false;
        (*s).current.set_configuration_ok = false;
        (*s).current.hid_set_protocol_attempted = false;
        (*s).current.hid_set_protocol_ok = false;
        (*s).current.hid_set_idle_attempted = false;
        (*s).current.hid_set_idle_ok = false;
        (*s).current.interrupt_endpoint_configured = false;
        (*s).current.interrupt_endpoint_faulted = false;
        (*s).current.interrupt_pending = false;
        (*s).current.interrupt_pending_trb_phys = 0;
        (*s).current.interrupt_transfer_handle = 0;
        (*s).current.interrupt_pending_streak = 0;
        (*s).current.last_interrupt_ep_state = 0;
        (*s).current.interrupt_endpoint_id = 0;
        (*s).current.interrupt_endpoint_address = 0;
        (*s).current.interrupt_endpoint_max_packet = 0;
        (*s).current.interrupt_endpoint_interval_raw = 0;
        (*s).current.interrupt_endpoint_interval_context = 0;
        (*s).current.interrupt_enqueue = 0;
        (*s).current.interrupt_cycle = 1;
        (*s).current.last_interrupt_request_len = 0;
        (*s).current.last_interrupt_residue = 0;
        (*s).current.last_interrupt_actual_len = 0;
        (*s).current.last_interrupt_completion_code = 0;
        (*s).current.bulk_endpoints_configured = false;
        (*s).current.bulk_endpoints_faulted = false;
        (*s).current.bulk_in_endpoint_id = 0;
        (*s).current.bulk_out_endpoint_id = 0;
        (*s).current.bulk_in_enqueue = 0;
        (*s).current.bulk_in_cycle = 1;
        (*s).current.bulk_in_link_update_pending = false;
        (*s).current.bulk_out_enqueue = 0;
        (*s).current.bulk_out_cycle = 1;
        (*s).current.bulk_out_link_update_pending = false;
        (*s).current.selected_mass_storage = false;
        (*s).current.selected_hid_input = false;
        (*s).current.last_control_request_type = 0;
        (*s).current.last_control_request = 0;
        (*s).current.last_control_value = 0;
        (*s).current.last_control_index = 0;
        (*s).current.last_control_length = 0;
        (*s).current.last_control_direction = Text::literal(b"none");
        (*s).current.last_control_completion_code = 0;
        (*s).current.last_control_residue = 0;
        (*s).current.last_control_ok = false;
        (*s).current.control_endpoint_faulted = false;
        (*s).current.descriptor_len = 0;
        (*s).current.descriptor_type = 0;
        (*s).current.usb_version_bcd = 0;
        (*s).current.device_class = 0;
        (*s).current.device_subclass = 0;
        (*s).current.device_protocol = 0;
        (*s).current.device_max_packet0 = 0;
        (*s).current.device_vendor_id = 0;
        (*s).current.device_product_id = 0;
        (*s).current.device_version_bcd = 0;
        (*s).current.manufacturer_index = 0;
        (*s).current.product_index = 0;
        (*s).current.serial_index = 0;
        (*s).current.config_total_length = 0;
        (*s).current.config_value = 0;
        (*s).current.config_attributes = 0;
        (*s).current.config_max_power_ma = 0;
        (*s).current.interface_count = 0;
        (*s).current.interface_record_count = 0;
        (*s).current.interface_records = [Interface::EMPTY; 8];
        (*s).current.endpoint_count = 0;
        (*s).current.descriptor_records = 0;
        (*s).current.descriptor_unknown = 0;
        (*s).current.descriptor_malformed = 0;
        (*s).current.hid_descriptor_count = 0;
        (*s).current.ss_endpoint_companion_count = 0;
        (*s).current.selected_interface_reason = Text::literal(b"none");
        (*s).current.first_interface_number = 0;
        (*s).current.first_interface_class = 0;
        (*s).current.first_interface_subclass = 0;
        (*s).current.first_interface_protocol = 0;
        (*s).current.first_endpoint_address = 0;
        (*s).current.first_endpoint_attributes = 0;
        (*s).current.first_endpoint_max_packet = 0;
        (*s).current.first_endpoint_interval = 0;
        (*s).current.first_hid_descriptor_len = 0;
        (*s).current.first_hid_report_descriptor_len = 0;
        (*s).current.bulk_in_endpoint_address = 0;
        (*s).current.bulk_in_endpoint_max_packet = 0;
        (*s).current.bulk_in_endpoint_max_burst = 0;
        (*s).current.bulk_out_endpoint_address = 0;
        (*s).current.bulk_out_endpoint_max_packet = 0;
        (*s).current.bulk_out_endpoint_max_burst = 0;
        (*s).current.string_language_id = 0;
        (*s).current.manufacturer_len = 0;
        (*s).current.product_len = 0;
        (*s).current.serial_len = 0;
        (*s).current.manufacturer_string = [0; 32];
        (*s).current.product_string = [0; 32];
        (*s).current.serial_string = [0; 32];
    }
}
pub(super) unsafe fn clear_current_runtime_selection(ctx: Context, slot_id: u8) {
    unsafe {
        let s = state(ctx);
        if slot_id != 0 && (*s).current.addressed_slot_id != slot_id {
            return;
        }

        (*s).current.addressed_slot_id = 0;
        (*s).current.addressed_port = 0;
        (*s).current.addressed_speed = 0;
        (*s).current.config_value = 0;
        (*s).current.device_vendor_id = 0;
        (*s).current.device_product_id = 0;
        (*s).current.get_config_ok = false;
        (*s).current.device_context_phys = 0;
        (*s).current.input_context_phys = 0;
        (*s).current.ep0_ring_phys = 0;
        (*s).current.interrupt_ring_phys = 0;
        (*s).current.interrupt_buffer_phys = 0;
        (*s).current.bulk_in_ring_phys = 0;
        (*s).current.bulk_out_ring_phys = 0;
        (*s).current.bulk_buffer_phys = 0;
        (*s).current.bulk_buffer_frames = 0;
        (*s).current.descriptor_phys = 0;
        (*s).first_device_virt = 0;
        (*s).first_input_virt = 0;
        (*s).first_ep0_ring_virt = 0;
        (*s).first_interrupt_ring_virt = 0;
        (*s).first_interrupt_buffer_virt = 0;
        (*s).first_bulk_in_ring_virt = 0;
        (*s).first_bulk_out_ring_virt = 0;
        (*s).first_bulk_buffer_virt = 0;
        (*s).first_descriptor_virt = 0;
        (*s).current.control_endpoint_faulted = false;
        (*s).current.interrupt_endpoint_configured = false;
        (*s).current.interrupt_endpoint_faulted = false;
        (*s).current.interrupt_pending = false;
        (*s).current.interrupt_pending_trb_phys = 0;
        (*s).current.interrupt_transfer_handle = 0;
        (*s).current.interrupt_pending_streak = 0;
        (*s).current.interrupt_endpoint_id = 0;
        (*s).current.interrupt_endpoint_address = 0;
        (*s).current.interrupt_endpoint_max_packet = 0;
        (*s).current.interrupt_endpoint_interval_raw = 0;
        (*s).current.interrupt_endpoint_interval_context = 0;
        (*s).current.bulk_endpoints_configured = false;
        (*s).current.bulk_endpoints_faulted = false;
        (*s).current.bulk_in_endpoint_id = 0;
        (*s).current.bulk_in_endpoint_address = 0;
        (*s).current.bulk_in_endpoint_max_packet = 0;
        (*s).current.bulk_in_endpoint_max_burst = 0;
        (*s).current.bulk_out_endpoint_id = 0;
        (*s).current.bulk_out_endpoint_address = 0;
        (*s).current.bulk_out_endpoint_max_packet = 0;
        (*s).current.bulk_out_endpoint_max_burst = 0;
    }
}
pub(super) unsafe fn runtime_index_for_slot(ctx: Context, slot: u8) -> Option<usize> {
    for index in 0..8 {
        let rt = unsafe { ptr::addr_of!((*state(ctx)).runtimes[index]).read() };
        if rt.active && rt.slot_id == slot {
            return Some(index);
        }
    }
    None
}
pub(super) unsafe fn runtime_index_for_identity(ctx: Context, slot: u8, port: u8) -> Option<usize> {
    if slot == 0 || port == 0 {
        return None;
    }
    for index in 0..8 {
        let rt = unsafe { ptr::addr_of!((*state(ctx)).runtimes[index]).read() };
        if rt.active && rt.slot_id == slot && rt.port == port {
            return Some(index);
        }
    }
    None
}
pub(super) unsafe fn activate_runtime(ctx: Context, index: usize) -> bool {
    unsafe {
        let s = state(ctx);
        if index >= 8 || !(*s).runtimes[index].active {
            return false;
        }
        if let Some(active) = (*s).active_runtime_index {
            if active == index {
                persist_active_runtime(ctx);
                return true;
            }
            persist_active_runtime(ctx);
        }
        (*s).active_runtime_index = Some(index);
        (*s).current.runtime_switches += 1;
        load_runtime(ctx, index);
        true
    }
}
pub(super) unsafe fn activate_runtime_by_port(ctx: Context, port: u8) -> bool {
    for index in 0..8 {
        let rt = unsafe { ptr::addr_of!((*state(ctx)).runtimes[index]).read() };
        if rt.active && rt.port == port {
            return unsafe { activate_runtime(ctx, index) };
        }
    }
    false
}

pub(super) unsafe fn activate_runtime_by_identity(ctx: Context, slot: u8, port: u8) -> bool {
    unsafe { runtime_index_for_identity(ctx, slot, port) }
        .is_some_and(|index| unsafe { activate_runtime(ctx, index) })
}
pub(super) unsafe fn defer_runtime_detach(ctx: Context, rt: *mut DeviceRuntime) -> bool {
    unsafe {
        if !(*rt).transaction_active {
            return false;
        }
        (*rt).detach_pending = true;
        (*state(ctx)).pending_port_changes.retry((*rt).port);
        true
    }
}
pub(super) unsafe fn free_dma_frame(_ctx: Context, frame: *mut u64) {
    unsafe {
        if *frame == 0 {
            return;
        }
        crate::memory::phys::r4os_rust_phys_free(*frame);
        frame.write(0);
    }
}
pub(super) unsafe fn free_dma_frames(_ctx: Context, base: *mut u64, count: u16) {
    unsafe {
        if *base == 0 {
            return;
        }
        if count <= 1 {
            crate::memory::phys::r4os_rust_phys_free(*base);
        } else {
            crate::memory::phys::r4os_rust_phys_free_contiguous(*base, u64::from(count));
        }
        base.write(0);
    }
}
pub(super) unsafe fn free_runtime_frames(ctx: Context, rt: *mut DeviceRuntime) {
    unsafe {
        free_dma_frame(ctx, ptr::addr_of_mut!((*rt).descriptor_phys));
        free_dma_frames(
            ctx,
            ptr::addr_of_mut!((*rt).bulk_buffer_phys),
            (*rt).bulk_buffer_frames,
        );
        (*rt).bulk_buffer_frames = 0;
        free_dma_frame(ctx, ptr::addr_of_mut!((*rt).bulk_out_ring_phys));
        free_dma_frame(ctx, ptr::addr_of_mut!((*rt).bulk_in_ring_phys));
        free_dma_frame(ctx, ptr::addr_of_mut!((*rt).interrupt_buffer_phys));
        free_dma_frame(ctx, ptr::addr_of_mut!((*rt).interrupt_ring_phys));
        free_dma_frame(ctx, ptr::addr_of_mut!((*rt).ep0_ring_phys));
        free_dma_frame(ctx, ptr::addr_of_mut!((*rt).input_context_phys));
        free_dma_frame(ctx, ptr::addr_of_mut!((*rt).device_context_phys));
        rt.write(DeviceRuntime::EMPTY);
    }
}
pub(super) unsafe fn free_all_runtimes(ctx: Context) {
    unsafe {
        for index in 0..8 {
            free_runtime_frames(ctx, ptr::addr_of_mut!((*state(ctx)).runtimes[index]));
        }
        ptr::addr_of_mut!((*state(ctx)).runtimes).write([DeviceRuntime::EMPTY; 8]);
        (*state(ctx)).active_runtime_index = None;
        (*state(ctx)).current.retained_slots = 0;
        clear_current_runtime_selection(ctx, (*state(ctx)).current.addressed_slot_id);
    }
}
pub(super) unsafe fn release_runtime_by_slot(ctx: Context, slot: u8) {
    unsafe {
        if let Some(index) = runtime_index_for_slot(ctx, slot)
            && defer_runtime_detach(ctx, ptr::addr_of_mut!((*state(ctx)).runtimes[index]))
        {
            return;
        }
        (*state(ctx)).deferred_events.purge_slot(slot);
        (*state(ctx)).transfer_objects.purge_slot(slot);
        for index in 0..8 {
            let rt = ptr::addr_of_mut!((*state(ctx)).runtimes[index]);
            if !(*rt).active || (*rt).slot_id != slot {
                continue;
            }
            let active = (*state(ctx)).active_runtime_index == Some(index);
            if active {
                super::xhci::close_last_sync_transfer_incident(ctx);
            } else {
                super::xhci::resolve_incident(ctx, (*rt).sync_transfer_incident);
            }
            free_runtime_frames(ctx, rt);
            if (*state(ctx)).current.retained_slots > 0 {
                (*state(ctx)).current.retained_slots -= 1;
            }
            if active {
                (*state(ctx)).active_runtime_index = None;
                clear_current_runtime_selection(ctx, slot);
            }
            return;
        }
    }
}
pub(super) unsafe fn runtime_index_for_port(ctx: Context, port: u8) -> Option<usize> {
    for index in 0..8 {
        let rt = unsafe { ptr::addr_of!((*state(ctx)).runtimes[index]).read() };
        if rt.active && rt.port == port {
            return Some(index);
        }
    }
    None
}
unsafe fn fail_alloc_runtime(ctx: Context, index: usize) -> Option<usize> {
    unsafe {
        free_runtime_frames(ctx, ptr::addr_of_mut!((*state(ctx)).runtimes[index]));
        (*state(ctx)).current.failures += 1;
        (*state(ctx)).current.reason = Text::literal(b"failed to allocate xHCI DMA frame");
        None
    }
}
pub(super) unsafe fn allocate_runtime(
    ctx: Context,
    slot_id: u8,
    port: u8,
    speed: u8,
) -> Option<usize> {
    unsafe {
        let s = state(ctx);
        (*s).deferred_events.purge_slot(slot_id);
        for index in 0..8 {
            let rt = ptr::addr_of_mut!((*s).runtimes[index]);
            if (*rt).active {
                continue;
            }
            if (*s).runtime_generation == u64::MAX {
                return None;
            }
            (*s).runtime_generation += 1;
            rt.write(DeviceRuntime {
                active: true,
                generation: (*s).runtime_generation,
                slot_id,
                port,
                speed,
                ..DeviceRuntime::EMPTY
            });
            (*rt).device_context_phys = match super::xhci_hardware::alloc_frame_zero() {
                Some(frame) => frame,
                None => return fail_alloc_runtime(ctx, index),
            };
            (*rt).input_context_phys = match super::xhci_hardware::alloc_frame_zero() {
                Some(frame) => frame,
                None => return fail_alloc_runtime(ctx, index),
            };
            (*rt).ep0_ring_phys = match super::xhci_hardware::alloc_frame_zero() {
                Some(frame) => frame,
                None => return fail_alloc_runtime(ctx, index),
            };
            (*rt).descriptor_phys = match super::xhci_hardware::alloc_frame_zero() {
                Some(frame) => frame,
                None => return fail_alloc_runtime(ctx, index),
            };
            (*rt).device_virt =
                crate::memory::phys::r4os_rust_phys_to_virt((*rt).device_context_phys);
            (*rt).input_virt =
                crate::memory::phys::r4os_rust_phys_to_virt((*rt).input_context_phys);
            (*rt).ep0_ring_virt = crate::memory::phys::r4os_rust_phys_to_virt((*rt).ep0_ring_phys);
            (*rt).descriptor_virt =
                crate::memory::phys::r4os_rust_phys_to_virt((*rt).descriptor_phys);
            (*s).current.retained_slots += 1;
            return Some(index);
        }
        (*s).current.failures += 1;
        (*s).current.reason = Text::literal(b"xHCI runtime table full");
        None
    }
}

// Allocation groups remain local until complete. Only these unpublished
// frames may roll back without a hardware acknowledgement. Once committed,
// existing exact endpoint/controller cleanup owns every frame, including
// command failure/timeout; no speculative free follows a possible submission.
pub(super) unsafe fn ensure_interrupt_storage(ctx: Context) -> bool {
    unsafe {
        let s = state(ctx);
        let c = super::xhci_state::current(ctx);
        if (*s).active_runtime_index.is_none() {
            return false;
        }
        if (*c).interrupt_ring_phys != 0 || (*c).interrupt_buffer_phys != 0 {
            return (*c).interrupt_ring_phys != 0
                && (*c).interrupt_buffer_phys != 0
                && (*s).first_interrupt_ring_virt != 0
                && (*s).first_interrupt_buffer_virt != 0;
        }
        let Some(mut ring) = super::xhci_hardware::alloc_frame_zero() else {
            return false;
        };
        let Some(buffer) = super::xhci_hardware::alloc_frame_zero() else {
            free_dma_frame(ctx, &mut ring);
            return false;
        };
        (*c).interrupt_ring_phys = ring;
        (*c).interrupt_buffer_phys = buffer;
        (*s).first_interrupt_ring_virt = crate::memory::phys::r4os_rust_phys_to_virt(ring);
        (*s).first_interrupt_buffer_virt = crate::memory::phys::r4os_rust_phys_to_virt(buffer);
        super::xhci_rings::init_interrupt_ring(ctx);
        persist_active_runtime(ctx);
        true
    }
}
pub(super) unsafe fn ensure_bulk_storage(ctx: Context) -> bool {
    unsafe {
        let s = state(ctx);
        let c = super::xhci_state::current(ctx);
        if (*s).active_runtime_index.is_none() {
            return false;
        }
        if (*c).bulk_in_ring_phys != 0 || (*c).bulk_out_ring_phys != 0 || (*c).bulk_buffer_phys != 0
        {
            return (*c).bulk_in_ring_phys != 0
                && (*c).bulk_out_ring_phys != 0
                && (*c).bulk_buffer_phys != 0
                && (*c).bulk_buffer_frames == super::xhci_constants::BULK_BUFFER_FRAMES
                && (*s).first_bulk_in_ring_virt != 0
                && (*s).first_bulk_out_ring_virt != 0
                && (*s).first_bulk_buffer_virt != 0;
        }
        let Some(mut input) = super::xhci_hardware::alloc_frame_zero() else {
            return false;
        };
        let Some(mut output) = super::xhci_hardware::alloc_frame_zero() else {
            free_dma_frame(ctx, &mut input);
            return false;
        };
        let Some(buffer) =
            super::xhci_hardware::alloc_contiguous_zero(super::xhci_constants::BULK_BUFFER_FRAMES)
        else {
            free_dma_frame(ctx, &mut output);
            free_dma_frame(ctx, &mut input);
            return false;
        };
        (*c).bulk_in_ring_phys = input;
        (*c).bulk_out_ring_phys = output;
        (*c).bulk_buffer_phys = buffer;
        (*c).bulk_buffer_frames = super::xhci_constants::BULK_BUFFER_FRAMES;
        (*s).first_bulk_in_ring_virt = crate::memory::phys::r4os_rust_phys_to_virt(input);
        (*s).first_bulk_out_ring_virt = crate::memory::phys::r4os_rust_phys_to_virt(output);
        (*s).first_bulk_buffer_virt = crate::memory::phys::r4os_rust_phys_to_virt(buffer);
        super::xhci_rings::init_bulk_rings(ctx);
        persist_active_runtime(ctx);
        true
    }
}
