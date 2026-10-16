//! Exact USB-v2 backend callback table. Every callback retains controller
//! admission until completion; the registry itself holds no metadata lock over
//! these calls. Only admitted guards are constructed, including error returns.
use super::{
    types::{ControlRequest as HostControl, HostDevice, HostEndpoint, HostStatus},
    xhci_handles::*,
    xhci_owner as owner,
    xhci_state::{current, state},
    xhci_types::{ControlRequest, DeviceHandle},
};
use core::{ffi::c_void, ptr, slice};
struct Guard;
impl Guard {
    fn enter() -> Option<Self> {
        if owner::acquire() { Some(Self) } else { None }
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        owner::release();
    }
}
unsafe fn buffer<'a>(pointer: *mut u8, len: u32) -> &'a mut [u8] {
    if len == 0 {
        &mut []
    } else {
        unsafe { slice::from_raw_parts_mut(pointer, len as usize) }
    }
}
pub(super) unsafe extern "C" fn host_port_scan(_: *mut c_void) -> i32 {
    i32::from(unsafe { super::xhci::service_port_changes() })
}
pub(super) unsafe extern "C" fn host_address_device(
    _: *mut c_void,
    port: u8,
    out: *mut HostDevice,
) -> i32 {
    unsafe {
        let Some(_guard) = Guard::enter() else {
            return -1;
        };
        if !super::xhci::select_device_by_port(port) {
            return -2;
        }
        let handle = DeviceHandle {
            port: (*current()).addressed_port,
            slot_id: (*current()).addressed_slot_id,
            speed: (*current()).addressed_speed,
            config_value: (*current()).config_value,
            vendor_id: (*current()).device_vendor_id,
            product_id: (*current()).device_product_id,
            ..DeviceHandle::EMPTY
        };
        out.write(usb_host_device_handle(handle));
        0
    }
}
pub(super) unsafe extern "C" fn host_configure_device(
    _: *mut c_void,
    raw: *const HostDevice,
    configuration: u8,
) -> i32 {
    unsafe {
        let Some(_guard) = Guard::enter() else {
            return -1;
        };
        let mut handle = device_from_usb_host(*raw);
        if !select_device_handle(&mut handle) || !owner::selected_allows_mutation() {
            return -2;
        }
        if configuration != 0 {
            (*current()).config_value = configuration;
        }
        if super::xhci::set_first_configuration() {
            0
        } else {
            -3
        }
    }
}
pub(super) unsafe extern "C" fn host_control_transfer(
    _: *mut c_void,
    raw_device: *const HostDevice,
    request: *const HostControl,
    bytes: *mut u8,
    len: u32,
) -> i32 {
    unsafe {
        let Some(_guard) = Guard::enter() else {
            return -1;
        };
        let mut device = device_from_usb_host(*raw_device);
        if !select_device_handle(&mut device) || (*state()).first_descriptor_virt == 0 {
            return -2;
        }
        if !owner::selected_allows_mutation() {
            return -2;
        }
        if u32::from((*request).length) > len
            || usize::from((*request).length) > super::xhci_constants::DESCRIPTOR_BYTES
        {
            return -3;
        }
        let direction = match (*request).direction {
            0 => 0,
            1 => 1,
            2 => 2,
            _ => return -4,
        };
        let scratch =
            ptr::with_exposed_provenance_mut::<u8>((*state()).first_descriptor_virt as usize);
        let size = usize::from((*request).length);
        if direction == 2 && size != 0 {
            ptr::copy_nonoverlapping(bytes, scratch, size);
        }
        if direction == 1 && size != 0 {
            ptr::write_bytes(scratch, 0, size);
        }
        let ok = super::xhci::submit_control(ControlRequest {
            request_type: (*request).request_type,
            request: (*request).request,
            value: (*request).value,
            index_value: (*request).index_value,
            length: (*request).length,
            data_phys: if direction == 0 {
                0
            } else {
                (*current()).descriptor_phys
            },
            direction,
        });
        if !ok {
            return -5;
        }
        let actual = u32::from((*request).length) - (*current()).last_control_residue;
        if direction == 1 && actual != 0 {
            ptr::copy_nonoverlapping(scratch, bytes, actual as usize);
        }
        i32::try_from(actual).expect("USB control length")
    }
}
pub(super) unsafe extern "C" fn host_bulk_transfer(
    _: *mut c_void,
    raw: *const HostEndpoint,
    bytes: *mut u8,
    len: u32,
    direction: u32,
) -> i32 {
    unsafe {
        let Some(_guard) = Guard::enter() else {
            return -1;
        };
        let Some(mut endpoint) = endpoint_from_usb_host(*raw) else {
            return -2;
        };
        let bytes = buffer(bytes, len);
        let ok = match direction {
            1 => endpoint.kind == 2 && super::xhci::bulk_in_for_handle(&mut endpoint, bytes),
            2 => endpoint.kind == 3 && super::xhci::bulk_out_for_handle(&mut endpoint, bytes),
            _ => false,
        };
        if !ok {
            return -3;
        }
        i32::try_from((*current()).last_bulk_actual_len).expect("USB bulk length")
    }
}
pub(super) unsafe extern "C" fn host_interrupt_transfer(
    _: *mut c_void,
    raw: *const HostEndpoint,
    bytes: *mut u8,
    len: u32,
    actual: *mut u32,
) -> i32 {
    unsafe {
        let Some(_guard) = Guard::enter() else {
            return -1;
        };
        let Some(mut endpoint) = endpoint_from_usb_host(*raw) else {
            return -2;
        };
        let result =
            super::xhci::poll_interrupt_in_report_status(&mut endpoint, buffer(bytes, len));
        actual.write(
            u32::try_from(super::xhci::last_interrupt_actual_length())
                .expect("USB interrupt length"),
        );
        match result {
            super::xhci::InterruptPollResult::Report => 1,
            super::xhci::InterruptPollResult::NoReport => 0,
            super::xhci::InterruptPollResult::Failed => -3,
        }
    }
}
pub(super) unsafe extern "C" fn host_reset_port(_: *mut c_void, port: u8) -> i32 {
    let Some(_guard) = Guard::enter() else {
        return -1;
    };
    if unsafe { super::xhci_ports::reset_port(port) } {
        0
    } else {
        -2
    }
}
pub(super) unsafe extern "C" fn host_clear_halt(_: *mut c_void, raw: *const HostEndpoint) -> i32 {
    unsafe {
        let Some(_guard) = Guard::enter() else {
            return -1;
        };
        let Some(mut endpoint) = endpoint_from_usb_host(*raw) else {
            return -2;
        };
        if super::xhci::clear_endpoint_halt_for_handle(&mut endpoint.device, endpoint.address, None)
        {
            0
        } else {
            -3
        }
    }
}
pub(super) unsafe extern "C" fn host_reset_endpoint(
    _: *mut c_void,
    raw: *const HostEndpoint,
) -> i32 {
    unsafe {
        let Some(_guard) = Guard::enter() else {
            return -1;
        };
        let Some(mut endpoint) = endpoint_from_usb_host(*raw) else {
            return -2;
        };
        if super::xhci::reset_endpoint_state_for_handle(
            &mut endpoint.device,
            endpoint.address,
            None,
        ) {
            0
        } else {
            -3
        }
    }
}
pub(super) unsafe extern "C" fn host_poll(_: *mut c_void) -> i32 {
    i32::from(unsafe { super::xhci::service_port_changes() })
}
pub(super) unsafe extern "C" fn host_shutdown(_: *mut c_void) -> i32 {
    unsafe {
        let Some(_guard) = Guard::enter() else {
            return -1;
        };
        let ok = super::xhci_boot::teardown_for_reprobe();
        if ok {
            (*current()).probed = false;
            (*current()).controller_running = false;
            (*current()).present = false;
            (*current()).reason = crate::sched::sync_types::Text::literal(b"xHCI backend unloaded");
        } else {
            (*current()).reason = crate::sched::sync_types::Text::literal(
                b"xHCI unload deferred or halt failed; resources retained",
            );
        }
        if ok { 0 } else { -2 }
    }
}
pub(super) unsafe extern "C" fn host_status(_: *mut c_void, out: *mut HostStatus) -> i32 {
    unsafe {
        let Some(_guard) = Guard::enter() else {
            return -1;
        };
        let snapshot = super::xhci_status::status_locked();
        let transfers = (*state()).transfer_objects.snapshot();
        out.write(HostStatus {
            state: if snapshot.controller_running {
                1
            } else if snapshot.present {
                0
            } else {
                2
            },
            source: snapshot.host_source,
            ports: u32::from(snapshot.port_count_seen),
            devices: u32::try_from(super::core::count()).expect("USB device count"),
            transfers: snapshot.control_transfers
                + snapshot.bulk_transfers
                + snapshot.interrupt_polls,
            failures: snapshot.failures,
            flags: (*state()).canonical_backend.flags,
            queue_depth: super::transfer_pool::MAX_TRANSFERS as u16,
            max_transfer_bytes: super::trb_chain::MAX_TRANSFER_BYTES,
            active_transfers: transfers.active,
            completions: transfers.completed,
            interrupts: snapshot.irq_handled,
            polls: snapshot.poll_fallbacks,
            cancellations: transfers.cancelled,
            ..HostStatus::EMPTY
        });
        0
    }
}
pub(super) unsafe fn install_callbacks() {
    unsafe {
        let s = state();
        if (*s).callbacks_initialized {
            return;
        }
        let descriptor = ptr::addr_of_mut!((*s).canonical_backend);
        (*descriptor).port_scan = host_port_scan as *const () as usize;
        (*descriptor).address_device = host_address_device as *const () as usize;
        (*descriptor).configure_device = host_configure_device as *const () as usize;
        (*descriptor).control_transfer = host_control_transfer as *const () as usize;
        (*descriptor).bulk_transfer = host_bulk_transfer as *const () as usize;
        (*descriptor).interrupt_transfer = host_interrupt_transfer as *const () as usize;
        (*descriptor).reset_port = host_reset_port as *const () as usize;
        (*descriptor).clear_halt = host_clear_halt as *const () as usize;
        (*descriptor).reset_endpoint = host_reset_endpoint as *const () as usize;
        (*descriptor).poll = host_poll as *const () as usize;
        (*descriptor).shutdown = host_shutdown as *const () as usize;
        (*descriptor).status = host_status as *const () as usize;
        (*s).callbacks_initialized = true;
    }
}
