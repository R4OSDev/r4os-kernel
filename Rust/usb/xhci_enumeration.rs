//! Original USB slot addressing and descriptor discovery, including the
//! storage/HID preference and exact retained runtime selection. No new policy.
use super::xhci_state::Context;
use super::{
    timing,
    xhci_constants::*,
    xhci_descriptors,
    xhci_hardware::{dma_fence, read_le16, trb_type},
    xhci_ports::*,
    xhci_rings::*,
    xhci_runtime::*,
    xhci_state::{current, state},
    xhci_types::ProbeRecord,
};
use crate::sched::sync_types::Text;
use core::{ptr, slice};
#[derive(Clone, Copy)]
pub(super) enum SelectedKind {
    MassStorage,
    HidInput,
    Generic,
}
pub(super) unsafe fn begin_probe_record(ctx: Context, port_index: usize) -> usize {
    unsafe {
        let index = usize::from((*current(ctx)).probe_record_count);
        if index >= MAX_PORTS {
            return MAX_PORTS - 1;
        }
        let p = ptr::addr_of!((*current(ctx)).first_ports[port_index]).read();
        ptr::addr_of_mut!((*current(ctx)).probe_records[index]).write(ProbeRecord {
            active: true,
            port: p.index,
            speed: ((p.portsc >> 10) & 15) as u8,
            pls: port_link_state(ctx, p.portsc) as u8,
            stage: Text::literal(b"start"),
            ..ProbeRecord::EMPTY
        });
        (*current(ctx)).probe_record_count += 1;
        index
    }
}
pub(super) unsafe fn update_probe_record(ctx: Context, index: usize, stage: &'static [u8]) {
    unsafe {
        if index >= MAX_PORTS {
            return;
        }
        let c = current(ctx);
        let r = ptr::addr_of_mut!((*c).probe_records[index]);
        if !(*r).active {
            return;
        }
        (*r).stage = Text::literal(stage);
        (*r).slot = (*c).addressed_slot_id;
        (*r).command_cc = (*c).last_completion_code;
        (*r).transfer_cc = (*c).last_transfer_completion_code;
        (*r).class_code = (*c).first_interface_class;
        (*r).subclass = (*c).first_interface_subclass;
        (*r).protocol = (*c).first_interface_protocol;
        (*r).vid = (*c).device_vendor_id;
        (*r).pid = (*c).device_product_id;
    }
}
pub(super) unsafe fn is_boot_hid_input(ctx: Context) -> bool {
    unsafe {
        (*current(ctx)).first_interface_class == 3
            && (*current(ctx)).first_interface_subclass == 1
            && ((*current(ctx)).first_interface_protocol == 1
                || (*current(ctx)).first_interface_protocol == 2)
            && (*current(ctx)).first_endpoint_address & 0x80 != 0
            && (*current(ctx)).first_endpoint_attributes & 3 == 3
    }
}
pub(super) unsafe fn is_mass_storage_bulk(ctx: Context) -> bool {
    unsafe {
        (*current(ctx)).first_interface_class == 8
            && (*current(ctx)).first_interface_subclass == 6
            && (*current(ctx)).first_interface_protocol == 0x50
            && (*current(ctx)).bulk_in_endpoint_address != 0
            && (*current(ctx)).bulk_out_endpoint_address != 0
    }
}
pub(super) unsafe fn enable_slot_smoke(ctx: Context) -> bool {
    unsafe {
        (*current(ctx)).enable_slot_attempted = true;
        let Some(completion) =
            super::xhci::submit_command(ctx, 0, 0, trb_type(TRB_TYPE_ENABLE_SLOT))
        else {
            return false;
        };
        (*current(ctx)).enable_slot_ok = completion.code == COMPLETION_SUCCESS;
        (*current(ctx)).last_completion_code = completion.code;
        (*current(ctx)).last_slot_id = completion.slot_id;
        if !(*current(ctx)).enable_slot_ok {
            (*current(ctx)).failures += 1;
        }
        (*current(ctx)).enable_slot_ok
    }
}
pub(super) unsafe fn disable_slot(ctx: Context, slot: u8) -> bool {
    unsafe {
        if slot == 0 {
            return false;
        }
        (*current(ctx)).disable_slot_commands += 1;
        let completion = super::xhci::submit_command(
            ctx,
            0,
            0,
            trb_type(TRB_TYPE_DISABLE_SLOT) | (u32::from(slot) << 24),
        );
        let ok = completion.is_some_and(|event| event.code == COMPLETION_SUCCESS);
        if !ok {
            (*current(ctx)).disable_slot_failures += 1;
        }
        if ok && (*state(ctx)).dcbaa_virt != 0 {
            ptr::with_exposed_provenance_mut::<u64>((*state(ctx)).dcbaa_virt as usize)
                .add(usize::from(slot))
                .write(0);
            dma_fence();
        }
        ok
    }
}
pub(super) unsafe fn alloc_first_device_runtime(ctx: Context) -> bool {
    unsafe {
        if (*current(ctx)).addressed_slot_id == 0 {
            return false;
        }
        let Some(index) =
            runtime_index_for_slot(ctx, (*current(ctx)).addressed_slot_id).or_else(|| {
                allocate_runtime(
                    ctx,
                    (*current(ctx)).addressed_slot_id,
                    (*current(ctx)).addressed_port,
                    (*current(ctx)).addressed_speed,
                )
            })
        else {
            return false;
        };
        if !activate_runtime(ctx, index) {
            return false;
        }
        let s = state(ctx);
        if (*s).first_device_virt == 0
            || (*s).first_input_virt == 0
            || (*s).first_ep0_ring_virt == 0
            || (*s).first_descriptor_virt == 0
        {
            return false;
        }
        for (address, size) in [
            ((*s).first_device_virt, DEVICE_CONTEXT_BYTES),
            ((*s).first_input_virt, INPUT_CONTEXT_BYTES),
            ((*s).first_descriptor_virt, DESCRIPTOR_BYTES),
            ((*s).first_interrupt_buffer_virt, INTERRUPT_BUFFER_BYTES),
            ((*s).first_bulk_buffer_virt, BULK_BUFFER_BYTES),
        ] {
            ptr::write_bytes(
                ptr::with_exposed_provenance_mut::<u8>(address as usize),
                0,
                size,
            );
        }
        ptr::with_exposed_provenance_mut::<u64>((*s).dcbaa_virt as usize)
            .add(usize::from((*current(ctx)).addressed_slot_id))
            .write((*current(ctx)).device_context_phys);
        init_ep0_ring(ctx);
        init_interrupt_ring(ctx);
        init_bulk_rings(ctx);
        true
    }
}
pub(super) unsafe fn address_first_device(ctx: Context) -> bool {
    unsafe {
        (*current(ctx)).address_device_attempted = true;
        let control = trb_type(TRB_TYPE_ADDRESS_DEVICE)
            | (u32::from((*current(ctx)).addressed_slot_id) << 24);
        let Some(completion) =
            super::xhci::submit_command(ctx, (*current(ctx)).input_context_phys, 0, control)
        else {
            return false;
        };
        (*current(ctx)).last_completion_code = completion.code;
        (*current(ctx)).last_slot_id = completion.slot_id;
        (*current(ctx)).address_device_ok = completion.code == COMPLETION_SUCCESS;
        if !(*current(ctx)).address_device_ok {
            (*current(ctx)).failures += 1;
        }
        if (*current(ctx)).address_device_ok {
            super::wait::milliseconds(
                timing::SET_ADDRESS_SETTLE_MS,
                Text::literal(b"xhci-address-settle"),
                0,
            );
        }
        (*current(ctx)).address_device_ok
    }
}
pub(super) unsafe fn get_first_device_descriptor(ctx: Context) -> bool {
    unsafe {
        (*current(ctx)).get_descriptor_attempted = true;
        let descriptor =
            ptr::with_exposed_provenance_mut::<u8>((*state(ctx)).first_descriptor_virt as usize);
        ptr::write_bytes(descriptor, 0, DESCRIPTOR_BYTES);
        if !super::xhci::submit_control_in(
            ctx,
            USB_REQ_GET_DESCRIPTOR,
            u16::from(USB_DESC_DEVICE) << 8,
            0,
            DEVICE_DESCRIPTOR_LEN,
            (*current(ctx)).descriptor_phys,
        ) {
            return false;
        }
        xhci_descriptors::parse_device(
            ctx,
            &mut *current(ctx),
            slice::from_raw_parts(descriptor, DESCRIPTOR_BYTES),
        );
        (*current(ctx)).get_descriptor_ok = (*current(ctx)).descriptor_len
            == DEVICE_DESCRIPTOR_LEN as u8
            && (*current(ctx)).descriptor_type == USB_DESC_DEVICE;
        if !(*current(ctx)).get_descriptor_ok {
            (*current(ctx)).failures += 1;
        }
        (*current(ctx)).get_descriptor_ok
    }
}
pub(super) unsafe fn get_first_config_descriptor(ctx: Context) -> bool {
    unsafe {
        (*current(ctx)).get_config_attempted = true;
        let descriptor =
            ptr::with_exposed_provenance_mut::<u8>((*state(ctx)).first_descriptor_virt as usize);
        ptr::write_bytes(descriptor, 0, DESCRIPTOR_BYTES);
        if !super::xhci::submit_control_in(
            ctx,
            USB_REQ_GET_DESCRIPTOR,
            u16::from(USB_DESC_CONFIGURATION) << 8,
            0,
            CONFIG_DESCRIPTOR_HEADER_LEN,
            (*current(ctx)).descriptor_phys,
        ) {
            return false;
        }
        let total = read_le16(descriptor, 2);
        (*current(ctx)).config_total_length = total;
        if u16::from(descriptor.read()) < CONFIG_DESCRIPTOR_HEADER_LEN
            || descriptor.add(1).read() != USB_DESC_CONFIGURATION
            || total < CONFIG_DESCRIPTOR_HEADER_LEN
            || usize::from(total) > DESCRIPTOR_BYTES
        {
            (*current(ctx)).failures += 1;
            return false;
        }
        ptr::write_bytes(descriptor, 0, DESCRIPTOR_BYTES);
        if !super::xhci::submit_control_in(
            ctx,
            USB_REQ_GET_DESCRIPTOR,
            u16::from(USB_DESC_CONFIGURATION) << 8,
            0,
            total,
            (*current(ctx)).descriptor_phys,
        ) {
            return false;
        }
        xhci_descriptors::parse_config(
            ctx,
            &mut *current(ctx),
            slice::from_raw_parts(descriptor, DESCRIPTOR_BYTES),
            total,
        );
        (*current(ctx)).get_config_ok =
            (*current(ctx)).config_total_length == total && (*current(ctx)).interface_count > 0;
        if !(*current(ctx)).get_config_ok {
            (*current(ctx)).failures += 1;
        }
        (*current(ctx)).get_config_ok
    }
}
pub(super) unsafe fn read_first_string_language(ctx: Context) -> Option<u16> {
    unsafe {
        let descriptor =
            ptr::with_exposed_provenance_mut::<u8>((*state(ctx)).first_descriptor_virt as usize);
        ptr::write_bytes(descriptor, 0, DESCRIPTOR_BYTES);
        if !super::xhci::submit_control_in(
            ctx,
            USB_REQ_GET_DESCRIPTOR,
            u16::from(USB_DESC_STRING) << 8,
            0,
            4,
            (*current(ctx)).descriptor_phys,
        ) {
            return None;
        }
        if descriptor.read() < 4 || descriptor.add(1).read() != USB_DESC_STRING {
            return None;
        }
        Some(read_le16(descriptor, 2))
    }
}
pub(super) unsafe fn read_ascii_string_descriptor(
    ctx: Context,
    index: u8,
    language: u16,
    out: *mut [u8; 32],
    length: *mut u8,
) -> bool {
    unsafe {
        out.write([0; 32]);
        length.write(0);
        let descriptor =
            ptr::with_exposed_provenance_mut::<u8>((*state(ctx)).first_descriptor_virt as usize);
        ptr::write_bytes(descriptor, 0, DESCRIPTOR_BYTES);
        if !super::xhci::submit_control_in(
            ctx,
            USB_REQ_GET_DESCRIPTOR,
            (u16::from(USB_DESC_STRING) << 8) | u16::from(index),
            language,
            255,
            (*current(ctx)).descriptor_phys,
        ) {
            return false;
        }
        let len = usize::from(descriptor.read()).min(255);
        if len < 2 || descriptor.add(1).read() != USB_DESC_STRING {
            return false;
        }
        let (mut offset, mut count) = (2usize, 0usize);
        while offset + 1 < len && count < MAX_STRING_CHARS {
            let code = read_le16(descriptor, offset);
            (*out)[count] = if (0x20..=0x7e).contains(&code) {
                code as u8
            } else {
                b'?'
            };
            offset += 2;
            count += 1;
        }
        length.write(count as u8);
        true
    }
}
pub(super) unsafe fn get_first_string_descriptors(ctx: Context) -> bool {
    unsafe {
        let c = current(ctx);
        (*c).get_strings_attempted = true;
        (*c).manufacturer_len = 0;
        (*c).product_len = 0;
        (*c).serial_len = 0;
        ptr::addr_of_mut!((*c).manufacturer_string).write([0; 32]);
        ptr::addr_of_mut!((*c).product_string).write([0; 32]);
        ptr::addr_of_mut!((*c).serial_string).write([0; 32]);
        if (*c).manufacturer_index == 0 && (*c).product_index == 0 && (*c).serial_index == 0 {
            (*c).get_strings_ok = true;
            return true;
        }
        (*c).string_language_id = read_first_string_language(ctx).unwrap_or(0x409);
        let mut ok = true;
        if (*c).manufacturer_index != 0 {
            ok = read_ascii_string_descriptor(
                ctx,
                (*c).manufacturer_index,
                (*c).string_language_id,
                ptr::addr_of_mut!((*c).manufacturer_string),
                ptr::addr_of_mut!((*c).manufacturer_len),
            ) && ok;
        }
        if (*c).product_index != 0 {
            ok = read_ascii_string_descriptor(
                ctx,
                (*c).product_index,
                (*c).string_language_id,
                ptr::addr_of_mut!((*c).product_string),
                ptr::addr_of_mut!((*c).product_len),
            ) && ok;
        }
        if (*c).serial_index != 0 {
            ok = read_ascii_string_descriptor(
                ctx,
                (*c).serial_index,
                (*c).string_language_id,
                ptr::addr_of_mut!((*c).serial_string),
                ptr::addr_of_mut!((*c).serial_len),
            ) && ok;
        }
        (*c).get_strings_ok = ok;
        if !ok {
            (*c).failures += 1;
        }
        ok
    }
}
pub(super) unsafe fn probe_port_device(ctx: Context, index: usize) -> bool {
    unsafe {
        persist_active_runtime(ctx);
        (*state(ctx)).active_runtime_index = None;
        reset_first_enumeration_fields(ctx);
        (*current(ctx)).ports_probed += 1;
        let record = begin_probe_record(ctx, index);
        if !enable_slot_smoke(ctx) {
            update_probe_record(ctx, record, b"enable-slot-failed");
            return false;
        }
        (*current(ctx)).addressed_slot_id = (*current(ctx)).last_slot_id;
        (*current(ctx)).addressed_port = (*current(ctx)).first_ports[index].index;
        (*current(ctx)).addressed_speed =
            (((*current(ctx)).first_ports[index].portsc >> 10) & 15) as u8;
        update_probe_record(ctx, record, b"slot-enabled");
        if !alloc_first_device_runtime(ctx) {
            update_probe_record(ctx, record, b"alloc-failed");
            return false;
        }
        setup_first_input_context(ctx, index);
        if !address_first_device(ctx) {
            update_probe_record(ctx, record, b"address-failed");
            return false;
        }
        update_probe_record(ctx, record, b"addressed");
        if !get_first_device_descriptor(ctx) {
            update_probe_record(ctx, record, b"device-desc-failed");
            return false;
        }
        update_probe_record(ctx, record, b"device-desc");
        if !get_first_config_descriptor(ctx) {
            update_probe_record(ctx, record, b"config-desc-failed");
            return false;
        }
        get_first_string_descriptors(ctx);
        update_probe_record(ctx, record, b"configured");
        (*current(ctx)).get_config_ok
    }
}
pub(super) unsafe fn select_device_by_port(ctx: Context, port: u8) -> bool {
    unsafe {
        if port == 0 {
            return false;
        }
        if activate_runtime_by_port(ctx, port) {
            return true;
        }
        if (*current(ctx)).addressed_slot_id != 0
            && (*current(ctx)).addressed_port == port
            && (*current(ctx)).get_config_ok
        {
            return true;
        }
        for index in 0..usize::from((*current(ctx)).port_count_seen) {
            if (*current(ctx)).first_ports[index].index != port {
                continue;
            }
            disable_current_slot_for_scan(ctx);
            return select_port_device(ctx, index, SelectedKind::Generic);
        }
        false
    }
}
pub(super) unsafe fn select_port_device(ctx: Context, index: usize, kind: SelectedKind) -> bool {
    unsafe {
        if activate_runtime_by_port(ctx, (*current(ctx)).first_ports[index].index) {
            match kind {
                SelectedKind::MassStorage => (*current(ctx)).selected_mass_storage = true,
                SelectedKind::HidInput => (*current(ctx)).selected_hid_input = true,
                SelectedKind::Generic => {}
            }
            return true;
        }
        if !probe_port_device(ctx, index) || !(*current(ctx)).get_config_ok {
            disable_current_slot_for_scan(ctx);
            return false;
        }
        match kind {
            SelectedKind::MassStorage => (*current(ctx)).selected_mass_storage = true,
            SelectedKind::HidInput => (*current(ctx)).selected_hid_input = true,
            SelectedKind::Generic => {}
        }
        publish_current_core_device(ctx);
        true
    }
}
pub(super) unsafe fn enumerate_first_connected_device(ctx: Context) -> bool {
    unsafe {
        refresh_ports_and_reclaim(ctx);
        let (mut mass, mut hid, mut fallback) = (None, None, None);
        let mut configured = false;
        for index in 0..usize::from((*current(ctx)).port_count_seen) {
            if !prepare_port_for_probe(ctx, index) {
                continue;
            }
            if !probe_port_device(ctx, index) {
                disable_current_slot_for_scan(ctx);
                continue;
            }
            if (*current(ctx)).get_config_ok {
                configured = true;
                publish_current_core_device(ctx);
                if is_mass_storage_bulk(ctx) {
                    if mass.is_none() {
                        mass = Some(index);
                    }
                } else if is_boot_hid_input(ctx) {
                    if hid.is_none() {
                        hid = Some(index);
                    }
                } else {
                    if fallback.is_none() {
                        fallback = Some(index);
                    }
                    (*current(ctx)).non_hid_devices_skipped += 1;
                }
                persist_active_runtime(ctx);
                continue;
            }
            disable_current_slot_for_scan(ctx);
        }
        if let Some(index) = mass {
            return select_port_device(ctx, index, SelectedKind::MassStorage);
        }
        if let Some(index) = hid {
            return select_port_device(ctx, index, SelectedKind::HidInput);
        }
        if let Some(index) = fallback {
            return select_port_device(ctx, index, SelectedKind::Generic);
        }
        configured
    }
}

pub(super) unsafe fn disable_current_slot_for_scan(ctx: Context) {
    unsafe {
        if (*current(ctx)).addressed_slot_id == 0 {
            return;
        }
        let slot = (*current(ctx)).addressed_slot_id;
        let port = (*current(ctx)).addressed_port;
        if let Some(index) = runtime_index_for_slot(ctx, slot)
            && defer_runtime_detach(ctx, ptr::addr_of_mut!((*state(ctx)).runtimes[index]))
        {
            return;
        }
        if !disable_slot(ctx, slot) {
            return;
        }
        if port != 0 {
            super::core::remove(ctx.name(), port);
        }
        release_runtime_by_slot(ctx, slot);
        (*current(ctx)).addressed_slot_id = 0;
        (*current(ctx)).addressed_port = 0;
        (*current(ctx)).control_endpoint_faulted = false;
        (*current(ctx)).interrupt_endpoint_configured = false;
        (*current(ctx)).interrupt_endpoint_faulted = false;
        (*current(ctx)).interrupt_pending = false;
        (*current(ctx)).interrupt_pending_trb_phys = 0;
        (*current(ctx)).interrupt_transfer_handle = 0;
        (*current(ctx)).interrupt_pending_streak = 0;
        (*current(ctx)).interrupt_endpoint_id = 0;
        (*current(ctx)).interrupt_endpoint_address = 0;
        (*current(ctx)).interrupt_endpoint_max_packet = 0;
        (*current(ctx)).interrupt_endpoint_interval_raw = 0;
        (*current(ctx)).interrupt_endpoint_interval_context = 0;
        (*current(ctx)).bulk_endpoints_configured = false;
        (*current(ctx)).bulk_endpoints_faulted = false;
    }
}
pub(super) unsafe fn reclaim_disconnected_runtimes(ctx: Context) {
    unsafe {
        persist_active_runtime(ctx);
        for index in 0..8 {
            if !(*state(ctx)).runtimes[index].active {
                continue;
            }
            let slot = (*state(ctx)).runtimes[index].slot_id;
            let port = (*state(ctx)).runtimes[index].port;
            if slot == 0 || port == 0 || port_is_connected(ctx, port) {
                continue;
            }
            if defer_runtime_detach(ctx, ptr::addr_of_mut!((*state(ctx)).runtimes[index])) {
                continue;
            }
            (*current(ctx)).port_disconnects += 1;
            if !disable_slot(ctx, slot) {
                continue;
            }
            super::core::remove(ctx.name(), port);
            release_runtime_by_slot(ctx, slot);
            (*current(ctx)).reclaimed_slots += 1;
            if (*current(ctx)).addressed_slot_id == slot {
                (*current(ctx)).addressed_slot_id = 0;
                (*current(ctx)).addressed_port = 0;
                (*current(ctx)).control_endpoint_faulted = false;
                (*current(ctx)).interrupt_endpoint_configured = false;
                (*current(ctx)).interrupt_endpoint_faulted = false;
                (*current(ctx)).interrupt_pending = false;
                (*current(ctx)).interrupt_pending_trb_phys = 0;
                (*current(ctx)).interrupt_transfer_handle = 0;
                (*current(ctx)).interrupt_pending_streak = 0;
                (*current(ctx)).interrupt_endpoint_id = 0;
                (*current(ctx)).interrupt_endpoint_address = 0;
                (*current(ctx)).interrupt_endpoint_max_packet = 0;
                (*current(ctx)).interrupt_endpoint_interval_raw = 0;
                (*current(ctx)).interrupt_endpoint_interval_context = 0;
                (*current(ctx)).bulk_endpoints_configured = false;
                (*current(ctx)).bulk_endpoints_faulted = false;
            }
        }
    }
}
pub(super) unsafe fn refresh_ports_and_reclaim(ctx: Context) {
    unsafe {
        if !(*current(ctx)).present
            || (*current(ctx)).mmio_virt == 0
            || !(*current(ctx)).controller_running
        {
            return;
        }
        super::xhci_io::read_operational(ctx);
        super::xhci_io::read_runtime(ctx);
        super::xhci_io::read_ports(ctx);
        reclaim_disconnected_runtimes(ctx);
    }
}
pub(super) unsafe fn publish_current_core_device(ctx: Context) {
    unsafe {
        let c = current(ctx);
        let device = super::types::Device {
            controller_id: ctx.id(),
            generation: (*state(ctx))
                .active_runtime_index
                .map_or(0, |i| (*state(ctx)).runtimes[i].generation),
            configured: (*c).get_config_ok,
            controller: ctx.name(),
            slot_id: (*c).addressed_slot_id,
            port: (*c).addressed_port,
            speed: (*c).addressed_speed,
            usb_version_bcd: (*c).usb_version_bcd,
            vendor_id: (*c).device_vendor_id,
            product_id: (*c).device_product_id,
            device_version_bcd: (*c).device_version_bcd,
            device_class: (*c).device_class,
            device_subclass: (*c).device_subclass,
            device_protocol: (*c).device_protocol,
            config_value: (*c).config_value,
            config_attributes: (*c).config_attributes,
            config_max_power_ma: (*c).config_max_power_ma,
            interface_count: (*c).interface_count,
            interface_record_count: (*c).interface_record_count,
            interfaces: (*c).interface_records,
            endpoint_count: (*c).endpoint_count,
            first_interface_number: (*c).first_interface_number,
            first_interface_class: (*c).first_interface_class,
            first_interface_subclass: (*c).first_interface_subclass,
            first_interface_protocol: (*c).first_interface_protocol,
            first_endpoint_address: (*c).first_endpoint_address,
            first_endpoint_attributes: (*c).first_endpoint_attributes,
            first_endpoint_max_packet: (*c).first_endpoint_max_packet,
            first_endpoint_interval: (*c).first_endpoint_interval,
            bulk_in_endpoint_address: (*c).bulk_in_endpoint_address,
            bulk_in_endpoint_max_packet: (*c).bulk_in_endpoint_max_packet,
            bulk_in_endpoint_max_burst: (*c).bulk_in_endpoint_max_burst,
            bulk_out_endpoint_address: (*c).bulk_out_endpoint_address,
            bulk_out_endpoint_max_packet: (*c).bulk_out_endpoint_max_packet,
            bulk_out_endpoint_max_burst: (*c).bulk_out_endpoint_max_burst,
            hid_descriptor_len: (*c).first_hid_descriptor_len,
            hid_report_descriptor_len: (*c).first_hid_report_descriptor_len,
            string_language_id: (*c).string_language_id,
            manufacturer_len: (*c).manufacturer_len,
            product_len: (*c).product_len,
            serial_len: (*c).serial_len,
            manufacturer_string: (*c).manufacturer_string,
            product_string: (*c).product_string,
            serial_string: (*c).serial_string,
            ..super::types::Device::EMPTY
        };
        super::core::publish_or_replace(device);
    }
}
