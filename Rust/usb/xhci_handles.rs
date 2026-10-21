//! Existing private generation-bound handles and stable public USB-v2 handle
//! projections. A zero private generation retains the original compatibility
//! selection semantics; published core handles always capture the live epoch.
use super::xhci_state::Context;
use super::{
    types::{Device, HostDevice, HostEndpoint},
    xhci_ports::*,
    xhci_runtime::*,
    xhci_state::{current, state},
    xhci_types::{DeviceHandle, EndpointHandle},
};
use core::ptr;
pub(super) unsafe fn usb_host_device_handle(_ctx: Context, handle: DeviceHandle) -> HostDevice {
    HostDevice {
        controller_id: handle.controller_id,
        port: handle.port,
        slot_id: handle.slot_id,
        speed: handle.speed,
        config_value: handle.config_value,
        vendor_id: handle.vendor_id,
        product_id: handle.product_id,
    }
}
pub(super) unsafe fn usb_host_endpoint_handle(
    ctx: Context,
    handle: EndpointHandle,
) -> HostEndpoint {
    assert!(handle.kind <= 3);
    HostEndpoint {
        device: unsafe { usb_host_device_handle(ctx, handle.device) },
        kind: u32::from(handle.kind),
        address: handle.address,
        endpoint_id: handle.endpoint_id,
        max_packet: handle.max_packet,
        interval: handle.interval,
        max_burst: handle.max_burst,
    }
}
pub(super) fn device_from_usb_host(_ctx: Context, handle: HostDevice) -> DeviceHandle {
    DeviceHandle {
        controller_id: handle.controller_id,
        port: handle.port,
        slot_id: handle.slot_id,
        speed: handle.speed,
        config_value: handle.config_value,
        vendor_id: handle.vendor_id,
        product_id: handle.product_id,
        ..DeviceHandle::EMPTY
    }
}
pub(super) fn endpoint_from_usb_host(ctx: Context, handle: HostEndpoint) -> Option<EndpointHandle> {
    if handle.kind > 3 {
        return None;
    }
    Some(EndpointHandle {
        device: device_from_usb_host(ctx, handle.device),
        kind: handle.kind as u8,
        address: handle.address,
        endpoint_id: handle.endpoint_id,
        max_packet: handle.max_packet,
        interval: handle.interval,
        max_burst: handle.max_burst,
    })
}
pub(super) unsafe fn device_handle_from_core(ctx: Context, device: Device) -> DeviceHandle {
    DeviceHandle {
        controller_id: ctx.id(),
        generation: device.generation,
        controller: device.controller,
        port: device.port,
        slot_id: device.slot_id,
        speed: device.speed,
        config_value: device.config_value,
        vendor_id: device.vendor_id,
        product_id: device.product_id,
    }
}
pub(super) unsafe fn bulk_in_handle_from_core(ctx: Context, device: Device) -> EndpointHandle {
    EndpointHandle {
        device: unsafe { device_handle_from_core(ctx, device) },
        kind: 2,
        address: device.bulk_in_endpoint_address,
        endpoint_id: super::xhci::endpoint_dci(ctx, device.bulk_in_endpoint_address),
        max_packet: device.bulk_in_endpoint_max_packet,
        max_burst: device.bulk_in_endpoint_max_burst,
        ..EndpointHandle::EMPTY
    }
}
pub(super) unsafe fn bulk_out_handle_from_core(ctx: Context, device: Device) -> EndpointHandle {
    EndpointHandle {
        device: unsafe { device_handle_from_core(ctx, device) },
        kind: 3,
        address: device.bulk_out_endpoint_address,
        endpoint_id: super::xhci::endpoint_dci(ctx, device.bulk_out_endpoint_address),
        max_packet: device.bulk_out_endpoint_max_packet,
        max_burst: device.bulk_out_endpoint_max_burst,
        ..EndpointHandle::EMPTY
    }
}
pub(super) unsafe fn refresh_handle_from_current(ctx: Context, handle: *mut DeviceHandle) {
    unsafe {
        (*handle).generation = runtime_index_for_identity(
            ctx,
            (*current(ctx)).addressed_slot_id,
            (*current(ctx)).addressed_port,
        )
        .map_or(0, |index| (*state(ctx)).runtimes[index].generation);
        (*handle).controller_id = ctx.id();
        (*handle).controller = ctx.name();
        (*handle).slot_id = (*current(ctx)).addressed_slot_id;
        (*handle).port = (*current(ctx)).addressed_port;
        (*handle).speed = (*current(ctx)).addressed_speed;
        (*handle).config_value = (*current(ctx)).config_value;
        (*handle).vendor_id = (*current(ctx)).device_vendor_id;
        (*handle).product_id = (*current(ctx)).device_product_id;
    }
}
pub(super) fn same_device_target(_ctx: Context, a: DeviceHandle, b: DeviceHandle) -> bool {
    a.controller_id == b.controller_id
        && a.controller_id != 0
        && a.port != 0
        && b.port != 0
        && a.slot_id != 0
        && b.slot_id != 0
        && a.port == b.port
        && a.slot_id == b.slot_id
        && (a.generation == 0 || b.generation == 0 || a.generation == b.generation)
}
pub(super) unsafe fn device_handle_current(ctx: Context, handle: DeviceHandle) -> bool {
    unsafe {
        if handle.controller_id != ctx.id()
            || handle.generation == 0
            || handle.port == 0
            || handle.port > (*current(ctx)).port_count_seen
        {
            return false;
        }
        refresh_port_snapshot_by_number(ctx, handle.port);
        let p = ptr::addr_of!((*current(ctx)).first_ports[usize::from(handle.port - 1)]).read();
        if !p.connected || p.change_bits & super::xhci_constants::PORTSC_CSC != 0 {
            return false;
        }
        let Some(index) = runtime_index_for_identity(ctx, handle.slot_id, handle.port) else {
            return false;
        };
        (*state(ctx)).runtimes[index].generation == handle.generation
            && !(*state(ctx)).runtimes[index].detach_pending
    }
}
pub(super) unsafe fn select_device_handle(ctx: Context, handle: *mut DeviceHandle) -> bool {
    unsafe {
        if (*handle).controller_id != ctx.id() || (*handle).port == 0 {
            return false;
        }
        if super::xhci::port_task_started(ctx) {
            refresh_port_snapshot_by_number(ctx, (*handle).port);
        } else {
            super::xhci::refresh_ports_and_reclaim(ctx);
        }
        if !port_is_connected(ctx, (*handle).port) {
            return false;
        }
        if (*handle).slot_id != 0 {
            let Some(index) = runtime_index_for_identity(ctx, (*handle).slot_id, (*handle).port)
            else {
                return false;
            };
            if (*state(ctx)).runtimes[index].detach_pending
                || ((*handle).generation != 0
                    && (*state(ctx)).runtimes[index].generation != (*handle).generation)
            {
                return false;
            }
            if !activate_runtime_by_identity(ctx, (*handle).slot_id, (*handle).port) {
                return false;
            }
            if (*current(ctx)).addressed_slot_id != (*handle).slot_id
                || (*current(ctx)).addressed_port != (*handle).port
            {
                return false;
            }
            refresh_handle_from_current(ctx, handle);
            return true;
        }
        if !super::xhci::select_device_by_port(ctx, (*handle).port) {
            return false;
        }
        refresh_handle_from_current(ctx, handle);
        true
    }
}
