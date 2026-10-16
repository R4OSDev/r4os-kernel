//! Original USB slot addressing and descriptor discovery, including the
//! storage/HID preference and exact retained runtime selection. No new policy.
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
pub(super) unsafe fn begin_probe_record(port_index: usize) -> usize {
    unsafe {
        let index = usize::from((*current()).probe_record_count);
        if index >= MAX_PORTS {
            return MAX_PORTS - 1;
        }
        let p = ptr::addr_of!((*current()).first_ports[port_index]).read();
        ptr::addr_of_mut!((*current()).probe_records[index]).write(ProbeRecord {
            active: true,
            port: p.index,
            speed: ((p.portsc >> 10) & 15) as u8,
            pls: port_link_state(p.portsc) as u8,
            stage: Text::literal(b"start"),
            ..ProbeRecord::EMPTY
        });
        (*current()).probe_record_count += 1;
        index
    }
}
pub(super) unsafe fn update_probe_record(index: usize, stage: &'static [u8]) {
    unsafe {
        if index >= MAX_PORTS {
            return;
        }
        let c = current();
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
pub(super) unsafe fn is_boot_hid_input() -> bool {
    unsafe {
        (*current()).first_interface_class == 3
            && (*current()).first_interface_subclass == 1
            && ((*current()).first_interface_protocol == 1
                || (*current()).first_interface_protocol == 2)
            && (*current()).first_endpoint_address & 0x80 != 0
            && (*current()).first_endpoint_attributes & 3 == 3
    }
}
pub(super) unsafe fn is_mass_storage_bulk() -> bool {
    unsafe {
        (*current()).first_interface_class == 8
            && (*current()).first_interface_subclass == 6
            && (*current()).first_interface_protocol == 0x50
            && (*current()).bulk_in_endpoint_address != 0
            && (*current()).bulk_out_endpoint_address != 0
    }
}
pub(super) unsafe fn enable_slot_smoke() -> bool {
    unsafe {
        (*current()).enable_slot_attempted = true;
        let Some(completion) = super::xhci::submit_command(0, 0, trb_type(TRB_TYPE_ENABLE_SLOT))
        else {
            return false;
        };
        (*current()).enable_slot_ok = completion.code == COMPLETION_SUCCESS;
        (*current()).last_completion_code = completion.code;
        (*current()).last_slot_id = completion.slot_id;
        if !(*current()).enable_slot_ok {
            (*current()).failures += 1;
        }
        (*current()).enable_slot_ok
    }
}
pub(super) unsafe fn disable_slot(slot: u8) -> bool {
    unsafe {
        if slot == 0 {
            return false;
        }
        (*current()).disable_slot_commands += 1;
        let completion = super::xhci::submit_command(
            0,
            0,
            trb_type(TRB_TYPE_DISABLE_SLOT) | (u32::from(slot) << 24),
        );
        let ok = completion.is_some_and(|event| event.code == COMPLETION_SUCCESS);
        if !ok {
            (*current()).disable_slot_failures += 1;
        }
        if ok && (*state()).dcbaa_virt != 0 {
            ptr::with_exposed_provenance_mut::<u64>((*state()).dcbaa_virt as usize)
                .add(usize::from(slot))
                .write(0);
            dma_fence();
        }
        ok
    }
}
pub(super) unsafe fn alloc_first_device_runtime() -> bool {
    unsafe {
        if (*current()).addressed_slot_id == 0 {
            return false;
        }
        let Some(index) = runtime_index_for_slot((*current()).addressed_slot_id).or_else(|| {
            allocate_runtime(
                (*current()).addressed_slot_id,
                (*current()).addressed_port,
                (*current()).addressed_speed,
            )
        }) else {
            return false;
        };
        if !activate_runtime(index) {
            return false;
        }
        let s = state();
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
            .add(usize::from((*current()).addressed_slot_id))
            .write((*current()).device_context_phys);
        init_ep0_ring();
        init_interrupt_ring();
        init_bulk_rings();
        true
    }
}
pub(super) unsafe fn address_first_device() -> bool {
    unsafe {
        (*current()).address_device_attempted = true;
        let control =
            trb_type(TRB_TYPE_ADDRESS_DEVICE) | (u32::from((*current()).addressed_slot_id) << 24);
        let Some(completion) =
            super::xhci::submit_command((*current()).input_context_phys, 0, control)
        else {
            return false;
        };
        (*current()).last_completion_code = completion.code;
        (*current()).last_slot_id = completion.slot_id;
        (*current()).address_device_ok = completion.code == COMPLETION_SUCCESS;
        if !(*current()).address_device_ok {
            (*current()).failures += 1;
        }
        if (*current()).address_device_ok {
            super::wait::milliseconds(
                timing::SET_ADDRESS_SETTLE_MS,
                Text::literal(b"xhci-address-settle"),
                0,
            );
        }
        (*current()).address_device_ok
    }
}
pub(super) unsafe fn get_first_device_descriptor() -> bool {
    unsafe {
        (*current()).get_descriptor_attempted = true;
        let descriptor =
            ptr::with_exposed_provenance_mut::<u8>((*state()).first_descriptor_virt as usize);
        ptr::write_bytes(descriptor, 0, DESCRIPTOR_BYTES);
        if !super::xhci::submit_control_in(
            USB_REQ_GET_DESCRIPTOR,
            u16::from(USB_DESC_DEVICE) << 8,
            0,
            DEVICE_DESCRIPTOR_LEN,
            (*current()).descriptor_phys,
        ) {
            return false;
        }
        xhci_descriptors::parse_device(
            &mut *current(),
            slice::from_raw_parts(descriptor, DESCRIPTOR_BYTES),
        );
        (*current()).get_descriptor_ok = (*current()).descriptor_len == DEVICE_DESCRIPTOR_LEN as u8
            && (*current()).descriptor_type == USB_DESC_DEVICE;
        if !(*current()).get_descriptor_ok {
            (*current()).failures += 1;
        }
        (*current()).get_descriptor_ok
    }
}
pub(super) unsafe fn get_first_config_descriptor() -> bool {
    unsafe {
        (*current()).get_config_attempted = true;
        let descriptor =
            ptr::with_exposed_provenance_mut::<u8>((*state()).first_descriptor_virt as usize);
        ptr::write_bytes(descriptor, 0, DESCRIPTOR_BYTES);
        if !super::xhci::submit_control_in(
            USB_REQ_GET_DESCRIPTOR,
            u16::from(USB_DESC_CONFIGURATION) << 8,
            0,
            CONFIG_DESCRIPTOR_HEADER_LEN,
            (*current()).descriptor_phys,
        ) {
            return false;
        }
        let total = read_le16(descriptor, 2);
        (*current()).config_total_length = total;
        if u16::from(descriptor.read()) < CONFIG_DESCRIPTOR_HEADER_LEN
            || descriptor.add(1).read() != USB_DESC_CONFIGURATION
            || total < CONFIG_DESCRIPTOR_HEADER_LEN
            || usize::from(total) > DESCRIPTOR_BYTES
        {
            (*current()).failures += 1;
            return false;
        }
        ptr::write_bytes(descriptor, 0, DESCRIPTOR_BYTES);
        if !super::xhci::submit_control_in(
            USB_REQ_GET_DESCRIPTOR,
            u16::from(USB_DESC_CONFIGURATION) << 8,
            0,
            total,
            (*current()).descriptor_phys,
        ) {
            return false;
        }
        xhci_descriptors::parse_config(
            &mut *current(),
            slice::from_raw_parts(descriptor, DESCRIPTOR_BYTES),
            total,
        );
        (*current()).get_config_ok =
            (*current()).config_total_length == total && (*current()).interface_count > 0;
        if !(*current()).get_config_ok {
            (*current()).failures += 1;
        }
        (*current()).get_config_ok
    }
}
pub(super) unsafe fn read_first_string_language() -> Option<u16> {
    unsafe {
        let descriptor =
            ptr::with_exposed_provenance_mut::<u8>((*state()).first_descriptor_virt as usize);
        ptr::write_bytes(descriptor, 0, DESCRIPTOR_BYTES);
        if !super::xhci::submit_control_in(
            USB_REQ_GET_DESCRIPTOR,
            u16::from(USB_DESC_STRING) << 8,
            0,
            4,
            (*current()).descriptor_phys,
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
    index: u8,
    language: u16,
    out: *mut [u8; 32],
    length: *mut u8,
) -> bool {
    unsafe {
        out.write([0; 32]);
        length.write(0);
        let descriptor =
            ptr::with_exposed_provenance_mut::<u8>((*state()).first_descriptor_virt as usize);
        ptr::write_bytes(descriptor, 0, DESCRIPTOR_BYTES);
        if !super::xhci::submit_control_in(
            USB_REQ_GET_DESCRIPTOR,
            (u16::from(USB_DESC_STRING) << 8) | u16::from(index),
            language,
            255,
            (*current()).descriptor_phys,
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
pub(super) unsafe fn get_first_string_descriptors() -> bool {
    unsafe {
        let c = current();
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
        (*c).string_language_id = read_first_string_language().unwrap_or(0x409);
        let mut ok = true;
        if (*c).manufacturer_index != 0 {
            ok = read_ascii_string_descriptor(
                (*c).manufacturer_index,
                (*c).string_language_id,
                ptr::addr_of_mut!((*c).manufacturer_string),
                ptr::addr_of_mut!((*c).manufacturer_len),
            ) && ok;
        }
        if (*c).product_index != 0 {
            ok = read_ascii_string_descriptor(
                (*c).product_index,
                (*c).string_language_id,
                ptr::addr_of_mut!((*c).product_string),
                ptr::addr_of_mut!((*c).product_len),
            ) && ok;
        }
        if (*c).serial_index != 0 {
            ok = read_ascii_string_descriptor(
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
pub(super) unsafe fn probe_port_device(index: usize) -> bool {
    unsafe {
        persist_active_runtime();
        (*state()).active_runtime_index = None;
        reset_first_enumeration_fields();
        (*current()).ports_probed += 1;
        let record = begin_probe_record(index);
        if !enable_slot_smoke() {
            update_probe_record(record, b"enable-slot-failed");
            return false;
        }
        (*current()).addressed_slot_id = (*current()).last_slot_id;
        (*current()).addressed_port = (*current()).first_ports[index].index;
        (*current()).addressed_speed = (((*current()).first_ports[index].portsc >> 10) & 15) as u8;
        update_probe_record(record, b"slot-enabled");
        if !alloc_first_device_runtime() {
            update_probe_record(record, b"alloc-failed");
            return false;
        }
        setup_first_input_context(index);
        if !address_first_device() {
            update_probe_record(record, b"address-failed");
            return false;
        }
        update_probe_record(record, b"addressed");
        if !get_first_device_descriptor() {
            update_probe_record(record, b"device-desc-failed");
            return false;
        }
        update_probe_record(record, b"device-desc");
        if !get_first_config_descriptor() {
            update_probe_record(record, b"config-desc-failed");
            return false;
        }
        get_first_string_descriptors();
        update_probe_record(record, b"configured");
        (*current()).get_config_ok
    }
}
pub(super) unsafe fn select_device_by_port(port: u8) -> bool {
    unsafe {
        if port == 0 {
            return false;
        }
        if activate_runtime_by_port(port) {
            return true;
        }
        if (*current()).addressed_slot_id != 0
            && (*current()).addressed_port == port
            && (*current()).get_config_ok
        {
            return true;
        }
        for index in 0..usize::from((*current()).port_count_seen) {
            if (*current()).first_ports[index].index != port {
                continue;
            }
            disable_current_slot_for_scan();
            return select_port_device(index, SelectedKind::Generic);
        }
        false
    }
}
pub(super) unsafe fn select_port_device(index: usize, kind: SelectedKind) -> bool {
    unsafe {
        if activate_runtime_by_port((*current()).first_ports[index].index) {
            match kind {
                SelectedKind::MassStorage => (*current()).selected_mass_storage = true,
                SelectedKind::HidInput => (*current()).selected_hid_input = true,
                SelectedKind::Generic => {}
            }
            return true;
        }
        if !probe_port_device(index) || !(*current()).get_config_ok {
            disable_current_slot_for_scan();
            return false;
        }
        match kind {
            SelectedKind::MassStorage => (*current()).selected_mass_storage = true,
            SelectedKind::HidInput => (*current()).selected_hid_input = true,
            SelectedKind::Generic => {}
        }
        publish_current_core_device();
        true
    }
}
pub(super) unsafe fn enumerate_first_connected_device() -> bool {
    unsafe {
        refresh_ports_and_reclaim();
        let (mut mass, mut hid, mut fallback) = (None, None, None);
        let mut configured = false;
        for index in 0..usize::from((*current()).port_count_seen) {
            if !prepare_port_for_probe(index) {
                continue;
            }
            if !probe_port_device(index) {
                disable_current_slot_for_scan();
                continue;
            }
            if (*current()).get_config_ok {
                configured = true;
                publish_current_core_device();
                if is_mass_storage_bulk() {
                    if mass.is_none() {
                        mass = Some(index);
                    }
                } else if is_boot_hid_input() {
                    if hid.is_none() {
                        hid = Some(index);
                    }
                } else {
                    if fallback.is_none() {
                        fallback = Some(index);
                    }
                    (*current()).non_hid_devices_skipped += 1;
                }
                persist_active_runtime();
                continue;
            }
            disable_current_slot_for_scan();
        }
        if let Some(index) = mass {
            return select_port_device(index, SelectedKind::MassStorage);
        }
        if let Some(index) = hid {
            return select_port_device(index, SelectedKind::HidInput);
        }
        if let Some(index) = fallback {
            return select_port_device(index, SelectedKind::Generic);
        }
        configured
    }
}

pub(super) unsafe fn disable_current_slot_for_scan() {
    unsafe {
        if (*current()).addressed_slot_id == 0 {
            return;
        }
        let slot = (*current()).addressed_slot_id;
        let port = (*current()).addressed_port;
        if let Some(index) = runtime_index_for_slot(slot)
            && defer_runtime_detach(ptr::addr_of_mut!((*state()).runtimes[index]))
        {
            return;
        }
        if !disable_slot(slot) {
            return;
        }
        if port != 0 {
            super::core::remove(Text::literal(b"xhci"), port);
        }
        release_runtime_by_slot(slot);
        (*current()).addressed_slot_id = 0;
        (*current()).addressed_port = 0;
        (*current()).control_endpoint_faulted = false;
        (*current()).interrupt_endpoint_configured = false;
        (*current()).interrupt_endpoint_faulted = false;
        (*current()).interrupt_pending = false;
        (*current()).interrupt_pending_trb_phys = 0;
        (*current()).interrupt_transfer_handle = 0;
        (*current()).interrupt_pending_streak = 0;
        (*current()).interrupt_endpoint_id = 0;
        (*current()).interrupt_endpoint_address = 0;
        (*current()).interrupt_endpoint_max_packet = 0;
        (*current()).interrupt_endpoint_interval_raw = 0;
        (*current()).interrupt_endpoint_interval_context = 0;
        (*current()).bulk_endpoints_configured = false;
        (*current()).bulk_endpoints_faulted = false;
    }
}
pub(super) unsafe fn reclaim_disconnected_runtimes() {
    unsafe {
        persist_active_runtime();
        for index in 0..8 {
            if !(*state()).runtimes[index].active {
                continue;
            }
            let slot = (*state()).runtimes[index].slot_id;
            let port = (*state()).runtimes[index].port;
            if slot == 0 || port == 0 || port_is_connected(port) {
                continue;
            }
            if defer_runtime_detach(ptr::addr_of_mut!((*state()).runtimes[index])) {
                continue;
            }
            (*current()).port_disconnects += 1;
            if !disable_slot(slot) {
                continue;
            }
            super::core::remove(Text::literal(b"xhci"), port);
            release_runtime_by_slot(slot);
            (*current()).reclaimed_slots += 1;
            if (*current()).addressed_slot_id == slot {
                (*current()).addressed_slot_id = 0;
                (*current()).addressed_port = 0;
                (*current()).control_endpoint_faulted = false;
                (*current()).interrupt_endpoint_configured = false;
                (*current()).interrupt_endpoint_faulted = false;
                (*current()).interrupt_pending = false;
                (*current()).interrupt_pending_trb_phys = 0;
                (*current()).interrupt_transfer_handle = 0;
                (*current()).interrupt_pending_streak = 0;
                (*current()).interrupt_endpoint_id = 0;
                (*current()).interrupt_endpoint_address = 0;
                (*current()).interrupt_endpoint_max_packet = 0;
                (*current()).interrupt_endpoint_interval_raw = 0;
                (*current()).interrupt_endpoint_interval_context = 0;
                (*current()).bulk_endpoints_configured = false;
                (*current()).bulk_endpoints_faulted = false;
            }
        }
    }
}
pub(super) unsafe fn refresh_ports_and_reclaim() {
    unsafe {
        if !(*current()).present || (*current()).mmio_virt == 0 || !(*current()).controller_running
        {
            return;
        }
        super::xhci_io::read_operational();
        super::xhci_io::read_runtime();
        super::xhci_io::read_ports();
        reclaim_disconnected_runtimes();
    }
}
pub(super) unsafe fn publish_current_core_device() {
    unsafe {
        let c = current();
        let device = super::types::Device {
            configured: (*c).get_config_ok,
            controller: Text::literal(b"xhci"),
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
