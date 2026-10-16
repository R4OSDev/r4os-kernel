//! Existing private generation-bound handles and stable public USB-v2 handle
//! projections. A zero private generation retains the original compatibility
//! selection semantics; published core handles always capture the live epoch.
use super::{
    types::{Device, HostDevice, HostEndpoint},
    xhci_ports::*,
    xhci_runtime::*,
    xhci_state::{current, state},
    xhci_types::{DeviceHandle, EndpointHandle},
};
use crate::sched::sync_types::Text;
use core::ptr;
pub(super) unsafe fn usb_host_device_handle(handle: DeviceHandle) -> HostDevice {
    unsafe {
        HostDevice {
            controller_id: (*current()).host_controller_id,
            port: handle.port,
            slot_id: handle.slot_id,
            speed: handle.speed,
            config_value: handle.config_value,
            vendor_id: handle.vendor_id,
            product_id: handle.product_id,
        }
    }
}
pub(super) unsafe fn usb_host_endpoint_handle(handle: EndpointHandle) -> HostEndpoint {
    assert!(handle.kind <= 3);
    HostEndpoint {
        device: unsafe { usb_host_device_handle(handle.device) },
        kind: u32::from(handle.kind),
        address: handle.address,
        endpoint_id: handle.endpoint_id,
        max_packet: handle.max_packet,
        interval: handle.interval,
        max_burst: handle.max_burst,
    }
}
pub(super) fn device_from_usb_host(handle: HostDevice) -> DeviceHandle {
    DeviceHandle {
        port: handle.port,
        slot_id: handle.slot_id,
        speed: handle.speed,
        config_value: handle.config_value,
        vendor_id: handle.vendor_id,
        product_id: handle.product_id,
        ..DeviceHandle::EMPTY
    }
}
pub(super) fn endpoint_from_usb_host(handle: HostEndpoint) -> Option<EndpointHandle> {
    if handle.kind > 3 {
        return None;
    }
    Some(EndpointHandle {
        device: device_from_usb_host(handle.device),
        kind: handle.kind as u8,
        address: handle.address,
        endpoint_id: handle.endpoint_id,
        max_packet: handle.max_packet,
        interval: handle.interval,
        max_burst: handle.max_burst,
    })
}
pub(super) unsafe fn device_handle_from_core(device: Device) -> DeviceHandle {
    unsafe {
        DeviceHandle {
            generation: runtime_index_for_identity(device.slot_id, device.port)
                .map_or(0, |index| (*state()).runtimes[index].generation),
            controller: device.controller,
            port: device.port,
            slot_id: device.slot_id,
            speed: device.speed,
            config_value: device.config_value,
            vendor_id: device.vendor_id,
            product_id: device.product_id,
        }
    }
}
pub(super) unsafe fn interrupt_in_handle_from_core(device: Device) -> EndpointHandle {
    EndpointHandle {
        device: unsafe { device_handle_from_core(device) },
        kind: 1,
        address: device.first_endpoint_address,
        endpoint_id: super::xhci::endpoint_dci(device.first_endpoint_address),
        max_packet: device.first_endpoint_max_packet,
        interval: device.first_endpoint_interval,
        ..EndpointHandle::EMPTY
    }
}
pub(super) unsafe fn bulk_in_handle_from_core(device: Device) -> EndpointHandle {
    EndpointHandle {
        device: unsafe { device_handle_from_core(device) },
        kind: 2,
        address: device.bulk_in_endpoint_address,
        endpoint_id: super::xhci::endpoint_dci(device.bulk_in_endpoint_address),
        max_packet: device.bulk_in_endpoint_max_packet,
        max_burst: device.bulk_in_endpoint_max_burst,
        ..EndpointHandle::EMPTY
    }
}
pub(super) unsafe fn bulk_out_handle_from_core(device: Device) -> EndpointHandle {
    EndpointHandle {
        device: unsafe { device_handle_from_core(device) },
        kind: 3,
        address: device.bulk_out_endpoint_address,
        endpoint_id: super::xhci::endpoint_dci(device.bulk_out_endpoint_address),
        max_packet: device.bulk_out_endpoint_max_packet,
        max_burst: device.bulk_out_endpoint_max_burst,
        ..EndpointHandle::EMPTY
    }
}
pub(super) unsafe fn refresh_handle_from_current(handle: *mut DeviceHandle) {
    unsafe {
        (*handle).generation =
            runtime_index_for_identity((*current()).addressed_slot_id, (*current()).addressed_port)
                .map_or(0, |index| (*state()).runtimes[index].generation);
        (*handle).controller = Text::literal(b"xhci");
        (*handle).slot_id = (*current()).addressed_slot_id;
        (*handle).port = (*current()).addressed_port;
        (*handle).speed = (*current()).addressed_speed;
        (*handle).config_value = (*current()).config_value;
        (*handle).vendor_id = (*current()).device_vendor_id;
        (*handle).product_id = (*current()).device_product_id;
    }
}
pub(super) fn same_device_target(a: DeviceHandle, b: DeviceHandle) -> bool {
    a.port != 0
        && b.port != 0
        && a.slot_id != 0
        && b.slot_id != 0
        && a.port == b.port
        && a.slot_id == b.slot_id
        && (a.generation == 0 || b.generation == 0 || a.generation == b.generation)
}
pub(super) unsafe fn device_handle_current(handle: DeviceHandle) -> bool {
    unsafe {
        if handle.generation == 0 || handle.port == 0 || handle.port > (*current()).port_count_seen
        {
            return false;
        }
        refresh_port_snapshot_by_number(handle.port);
        let p = ptr::addr_of!((*current()).first_ports[usize::from(handle.port - 1)]).read();
        if !p.connected || p.change_bits & super::xhci_constants::PORTSC_CSC != 0 {
            return false;
        }
        let Some(index) = runtime_index_for_identity(handle.slot_id, handle.port) else {
            return false;
        };
        (*state()).runtimes[index].generation == handle.generation
            && !(*state()).runtimes[index].detach_pending
    }
}
pub(super) unsafe fn select_device_handle(handle: *mut DeviceHandle) -> bool {
    unsafe {
        if (*handle).port == 0 {
            return false;
        }
        if super::xhci::port_task_started() {
            refresh_port_snapshot_by_number((*handle).port);
        } else {
            super::xhci::refresh_ports_and_reclaim();
        }
        if !port_is_connected((*handle).port) {
            return false;
        }
        if (*handle).slot_id != 0 {
            let Some(index) = runtime_index_for_identity((*handle).slot_id, (*handle).port) else {
                return false;
            };
            if (*state()).runtimes[index].detach_pending
                || ((*handle).generation != 0
                    && (*state()).runtimes[index].generation != (*handle).generation)
            {
                return false;
            }
            if !activate_runtime_by_identity((*handle).slot_id, (*handle).port) {
                return false;
            }
            if (*current()).addressed_slot_id != (*handle).slot_id
                || (*current()).addressed_port != (*handle).port
            {
                return false;
            }
            refresh_handle_from_current(handle);
            return true;
        }
        if !super::xhci::select_device_by_port((*handle).port) {
            return false;
        }
        refresh_handle_from_current(handle);
        true
    }
}
