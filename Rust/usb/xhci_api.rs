//! Native guarded xHCI access. Exact handles and the existing controller owner
//! protect snapshots, incidents, bootstrap activation and endpoint projection.
use super::{
    types::HostEndpoint,
    xhci, xhci_owner as owner,
    xhci_types::{DeviceHandle, EndpointHandle, Status},
};
use core::mem::MaybeUninit;
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
/// Caller retains the exact device/class lease; false leaves output untouched.
pub(super) unsafe fn status_for_device(
    handle: DeviceHandle,
    out: &mut MaybeUninit<Status>,
) -> bool {
    unsafe {
        let Some(_guard) = Guard::enter() else {
            return false;
        };
        let Some(index) = xhci::runtime_index_for_identity(handle.slot_id, handle.port) else {
            return false;
        };
        if (*super::xhci_state::state()).runtimes[index].generation != handle.generation
            || !xhci::activate_runtime(index)
        {
            return false;
        }
        out.write(super::xhci_status::status_locked());
        true
    }
}
/// Incident ownership transfers only for this exact admitted device generation.
pub(super) unsafe fn take_device_incident(handle: DeviceHandle) -> u64 {
    unsafe {
        let Some(_guard) = Guard::enter() else {
            return 0;
        };
        let Some(index) = xhci::runtime_index_for_identity(handle.slot_id, handle.port) else {
            return 0;
        };
        if (*super::xhci_state::state()).runtimes[index].generation != handle.generation
            || !xhci::activate_runtime(index)
        {
            return 0;
        }
        xhci::take_last_sync_transfer_incident_token().generation
    }
}
/// Boot caller retains bootstrap and exact task/module admission.
pub(crate) unsafe fn probe() -> bool {
    let Some(_guard) = Guard::enter() else {
        return false;
    };
    unsafe { xhci::probe_owned(0, 0) }
}
/// Driver caller retains exact host/module identity through activation.
pub(crate) unsafe fn activate(owner_id: u32, source: u32) -> i32 {
    let Some(_guard) = Guard::enter() else {
        return -1;
    };
    unsafe { xhci::activate(owner_id, source) }
}
pub(super) unsafe fn start_port_task() -> bool {
    let Some(_guard) = Guard::enter() else {
        return false;
    };
    unsafe { xhci::start_port_task() }
}
pub(super) unsafe fn host_endpoint_handle(handle: EndpointHandle) -> HostEndpoint {
    let Some(_guard) = Guard::enter() else {
        return HostEndpoint::EMPTY;
    };
    unsafe { xhci::usb_host_endpoint_handle(handle) }
}
