//! Private C compatibility entries. Device/endpoint records are fixed layouts;
//! the sole controller guard owns every read/mutation, including diagnostics.
use super::{
    types::{Device, HostDevice, HostEndpoint},
    wait_types::Deadline,
    xhci::*,
    xhci_owner as owner,
    xhci_state::TopologyHook,
    xhci_types::{DeviceHandle, DeviceTransaction, EndpointHandle, Status},
};
use core::slice;
pub(super) struct Guard;
impl Guard {
    pub(super) fn enter() -> Option<Self> {
        if owner::acquire() { Some(Self) } else { None }
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        owner::release();
    }
}
unsafe fn output<'a>(p: *mut u8, n: usize) -> &'a mut [u8] {
    if n == 0 {
        &mut []
    } else {
        unsafe { slice::from_raw_parts_mut(p, n) }
    }
}
unsafe fn input<'a>(p: *const u8, n: usize) -> &'a [u8] {
    if n == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(p, n) }
    }
}
/// # Safety
/// Caller balances admission on the same task; resident guard outlives nesting.
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_usb_xhci_acquire() -> bool {
    owner::acquire()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_usb_xhci_release() {
    owner::release();
}
/// # Safety
/// Exact caller-retained device and output; independent class guard stays held.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_begin_transaction(
    h: *mut DeviceHandle,
    out: *mut DeviceTransaction,
) -> bool {
    unsafe {
        if let Some(t) = owner::begin_transaction(h) {
            out.write(t);
            true
        } else {
            false
        }
    }
}
/// # Safety
/// Exact admitted generation and caller's class guard remain resident.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_end_transaction(t: *const DeviceTransaction) {
    unsafe {
        owner::end_transaction(*t);
    }
}
/// # Safety
/// Exclusive full diagnostic output; no controller pointer escapes the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_status(out: *mut Status) {
    unsafe {
        out.write(status());
    }
}
/// # Safety
/// Exact borrowed handle and exclusive output; false leaves output untouched.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_status_for_device(
    h: *const DeviceHandle,
    out: *mut Status,
) -> bool {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return false;
        };
        let Some(i) = runtime_index_for_identity((*h).slot_id, (*h).port) else {
            return false;
        };
        if (*super::xhci_state::state()).runtimes[i].generation != (*h).generation
            || !activate_runtime(i)
        {
            return false;
        }
        out.write(super::xhci_status::status_locked());
        true
    }
}
/// # Safety
/// Exact borrowed device; incident generation transfers to the calling owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_take_device_incident(h: *const DeviceHandle) -> u64 {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return 0;
        };
        let Some(i) = runtime_index_for_identity((*h).slot_id, (*h).port) else {
            return 0;
        };
        if (*super::xhci_state::state()).runtimes[i].generation != (*h).generation
            || !activate_runtime(i)
        {
            return 0;
        }
        take_last_sync_transfer_incident_token().generation
    }
}
/// # Safety
/// Kernel-resident topology callback retains lifetime until explicitly replaced.
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_usb_xhci_set_topology_hook(h: Option<TopologyHook>) {
    set_topology_hook(h);
}
/// # Safety
/// Callback follows the original controller admission and out-of-owner notification.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_service_port_changes() -> bool {
    unsafe { service_port_changes() }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_probe() -> bool {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return false;
        };
        probe_owned(0, 0)
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_activate(owner_id: u32, source: u32) -> i32 {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return -1;
        };
        activate(owner_id, source)
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_start_port_task() -> bool {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return false;
        };
        start_port_task()
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_usb_host_device_handle(
    h: *const DeviceHandle,
    out: *mut HostDevice,
) {
    unsafe {
        out.write(HostDevice::EMPTY);
        let Some(_g) = Guard::enter() else {
            return;
        };
        out.write(usb_host_device_handle(*h));
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_usb_host_endpoint_handle(
    h: *const EndpointHandle,
    out: *mut HostEndpoint,
) {
    unsafe {
        out.write(HostEndpoint::EMPTY);
        let Some(_g) = Guard::enter() else {
            return;
        };
        out.write(usb_host_endpoint_handle(*h));
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_device_handle_from_core(
    h: *const Device,
    out: *mut DeviceHandle,
) {
    unsafe {
        out.write(DeviceHandle::EMPTY);
        let Some(_g) = Guard::enter() else {
            return;
        };
        out.write(device_handle_from_core(*h));
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_interrupt_in_handle_from_core(
    h: *const Device,
    out: *mut EndpointHandle,
) {
    unsafe {
        out.write(EndpointHandle::EMPTY);
        let Some(_g) = Guard::enter() else {
            return;
        };
        out.write(interrupt_in_handle_from_core(*h));
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_bulk_in_handle_from_core(
    h: *const Device,
    out: *mut EndpointHandle,
) {
    unsafe {
        out.write(EndpointHandle::EMPTY);
        let Some(_g) = Guard::enter() else {
            return;
        };
        out.write(bulk_in_handle_from_core(*h));
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_bulk_out_handle_from_core(
    h: *const Device,
    out: *mut EndpointHandle,
) {
    unsafe {
        out.write(EndpointHandle::EMPTY);
        let Some(_g) = Guard::enter() else {
            return;
        };
        out.write(bulk_out_handle_from_core(*h));
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_select_device_handle(h: *mut DeviceHandle) -> bool {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return false;
        };
        select_device_handle(h)
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_device_handle_current(h: *const DeviceHandle) -> bool {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return false;
        };
        device_handle_current(*h)
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_configure_interrupt_in_endpoint_handle(
    h: *mut EndpointHandle,
) -> bool {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return false;
        };
        configure_interrupt_in_endpoint_handle(h)
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_select_device_by_port(port: u8) -> bool {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return false;
        };
        select_device_by_port(port)
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_configure_first_interrupt_in_endpoint(
    address: u8,
    packet: u16,
    interval: u8,
) -> bool {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return false;
        };
        configure_first_interrupt_in_endpoint(address, packet, interval)
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_configure_first_bulk_endpoints(
    input_address: u8,
    input_packet: u16,
    input_burst: u8,
    output_address: u8,
    output_packet: u16,
    output_burst: u8,
) -> bool {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return false;
        };
        configure_first_bulk_endpoints(
            input_address,
            input_packet,
            input_burst,
            output_address,
            output_packet,
            output_burst,
        )
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_configure_bulk_endpoint_handles(
    input: *mut EndpointHandle,
    output: *mut EndpointHandle,
) -> bool {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return false;
        };
        configure_bulk_endpoint_handles(input, output)
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_set_first_configuration() -> bool {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return false;
        };
        set_first_configuration()
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_set_first_configuration_for_hid() -> bool {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return false;
        };
        set_first_configuration()
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_set_configuration_for_handle(
    h: *mut DeviceHandle,
) -> bool {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return false;
        };
        set_configuration_for_handle(h)
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_set_first_hid_boot_protocol(interface: u8) -> bool {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return false;
        };
        set_first_hid_boot_protocol(interface)
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_set_hid_boot_protocol_for_handle(
    h: *mut DeviceHandle,
    interface: u8,
) -> bool {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return false;
        };
        set_hid_boot_protocol_for_handle(h, interface)
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_set_first_hid_idle(interface: u8) -> bool {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return false;
        };
        set_first_hid_idle(interface)
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_set_hid_idle_for_handle(
    h: *mut DeviceHandle,
    interface: u8,
) -> bool {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return false;
        };
        set_hid_idle_for_handle(h, interface)
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_reset_mass_storage_interface_for_handle(
    h: *mut DeviceHandle,
    interface: u8,
) -> bool {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return false;
        };
        reset_mass_storage_interface_for_handle(h, interface, None)
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_reset_mass_storage_interface_for_handle_within(
    h: *mut DeviceHandle,
    interface: u8,
    budget: *const Deadline,
) -> bool {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return false;
        };
        reset_mass_storage_interface_for_handle(h, interface, Some(&*budget))
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_reset_first_mass_storage_interface(
    interface: u8,
) -> bool {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return false;
        };
        reset_first_mass_storage_interface(interface, None)
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_clear_endpoint_halt_for_handle(
    h: *mut DeviceHandle,
    address: u8,
) -> bool {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return false;
        };
        clear_endpoint_halt_for_handle(h, address, None)
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_clear_endpoint_halt_for_handle_within(
    h: *mut DeviceHandle,
    address: u8,
    budget: *const Deadline,
) -> bool {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return false;
        };
        clear_endpoint_halt_for_handle(h, address, Some(&*budget))
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_clear_first_endpoint_halt(address: u8) -> bool {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return false;
        };
        clear_first_endpoint_halt(address, None)
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_reset_endpoint_state_for_handle(
    h: *mut DeviceHandle,
    address: u8,
) -> bool {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return false;
        };
        reset_endpoint_state_for_handle(h, address, None)
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_reset_endpoint_state_for_handle_within(
    h: *mut DeviceHandle,
    address: u8,
    budget: *const Deadline,
) -> bool {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return false;
        };
        reset_endpoint_state_for_handle(h, address, Some(&*budget))
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_reset_first_endpoint_state(address: u8) -> bool {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return false;
        };
        reset_first_endpoint_state(address, None)
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_reconfigure_mass_storage_bulk_endpoints_for_handle_within(
    h: *mut DeviceHandle,
    input_address: u8,
    output_address: u8,
    budget: *const Deadline,
) -> bool {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return false;
        };
        reconfigure_mass_storage_bulk_endpoints_for_handle_within(
            h,
            input_address,
            output_address,
            &*budget,
        )
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_get_hid_report_descriptor_for_handle(
    h: *mut DeviceHandle,
    interface: u8,
    expected: u16,
    out: *mut u8,
    len: usize,
) -> usize {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return usize::MAX;
        };
        get_hid_report_descriptor_for_handle(h, interface, expected, output(out, len))
            .unwrap_or(usize::MAX)
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_last_interrupt_actual_length() -> usize {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return 0;
        };
        last_interrupt_actual_length()
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_last_interrupt_requested_length() -> usize {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return 0;
        };
        last_interrupt_requested_length()
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_last_interrupt_residue() -> u32 {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return 0;
        };
        last_interrupt_residue()
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_take_last_sync_transfer_incident_token() -> u64 {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return 0;
        };
        take_last_sync_transfer_incident_token().generation
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_poll_first_interrupt_in_report_status(
    out: *mut u8,
    len: usize,
) -> u8 {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return 2;
        };
        poll_first_interrupt_in_report_status(output(out, len)) as u8
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_poll_interrupt_in_report_status(
    h: *mut EndpointHandle,
    out: *mut u8,
    len: usize,
) -> u8 {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return 2;
        };
        poll_interrupt_in_report_status(h, output(out, len)) as u8
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_bulk_out(bytes: *const u8, len: usize) -> bool {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return false;
        };
        bulk_out(input(bytes, len))
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_bulk_out_for_handle(
    h: *mut EndpointHandle,
    bytes: *const u8,
    len: usize,
) -> bool {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return false;
        };
        bulk_out_for_handle(h, input(bytes, len))
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_bulk_in(bytes: *mut u8, len: usize) -> bool {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return false;
        };
        bulk_in(output(bytes, len))
    }
}

/// # Safety
/// Paired records and buffers remain resident for the admitted call; outputs
/// are exclusive, and caller retention spans callbacks and blocking work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_xhci_bulk_in_for_handle(
    h: *mut EndpointHandle,
    bytes: *mut u8,
    len: usize,
) -> bool {
    unsafe {
        let Some(_g) = Guard::enter() else {
            return false;
        };
        bulk_in_for_handle(h, output(bytes, len))
    }
}
