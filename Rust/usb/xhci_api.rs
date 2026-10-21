//! Native guarded xHCI access. Exact handles and the existing controller owner
//! protect snapshots, incidents, bootstrap activation and endpoint projection.
use super::xhci_state::{self, Context};
use super::{
    types::HostEndpoint,
    xhci, xhci_owner as owner,
    xhci_types::{DeviceHandle, EndpointHandle, Status},
};
use core::mem::MaybeUninit;
pub(super) struct Guard {
    pub(super) ctx: Context,
}
impl Guard {
    pub(super) fn enter(ctx: Context) -> Option<Self> {
        if owner::acquire(ctx) {
            Some(Self { ctx })
        } else {
            None
        }
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        let ctx = self.ctx;
        owner::release(ctx);
    }
}
/// Caller retains the exact device/class lease; false leaves output untouched.
pub(super) unsafe fn status_for_device(
    handle: DeviceHandle,
    out: &mut MaybeUninit<Status>,
) -> bool {
    unsafe {
        let Some(_guard) = Guard::for_device(handle) else {
            return false;
        };
        let ctx = _guard.ctx;
        let Some(index) = xhci::runtime_index_for_identity(ctx, handle.slot_id, handle.port) else {
            return false;
        };
        if (*super::xhci_state::state(ctx)).runtimes[index].generation != handle.generation
            || !xhci::activate_runtime(ctx, index)
        {
            return false;
        }
        out.write(super::xhci_status::status_locked(ctx));
        true
    }
}
/// Incident ownership transfers only for this exact admitted device generation.
pub(super) unsafe fn take_device_incident(handle: DeviceHandle) -> u64 {
    unsafe {
        let Some(_guard) = Guard::for_device(handle) else {
            return 0;
        };
        let ctx = _guard.ctx;
        let Some(index) = xhci::runtime_index_for_identity(ctx, handle.slot_id, handle.port) else {
            return 0;
        };
        if (*super::xhci_state::state(ctx)).runtimes[index].generation != handle.generation
            || !xhci::activate_runtime(ctx, index)
        {
            return 0;
        }
        xhci::take_last_sync_transfer_incident_token(ctx).generation
    }
}
/// Boot caller retains bootstrap and exact task/module admission.
pub(crate) unsafe fn probe() -> bool {
    let ps = crate::platform::pci_inventory::snapshot();
    // Preserve the unavailable-backend registration when no controller exists.
    // Activation success describes the driver owner, not present hardware.
    let mut running = false;
    for device in controller_devices(ps.xhci_count == 0) {
        let Some(ctx) = xhci_state::ensure(device) else {
            continue;
        };
        let Some(_guard) = Guard::enter(ctx) else {
            continue;
        };
        running |= unsafe { xhci::probe_owned(ctx, 0, 0) };
    }
    running
}
/// Driver caller retains exact host/module identity through activation.
pub(crate) unsafe fn activate(owner_id: u32, source: u32) -> i32 {
    let ps = crate::platform::pci_inventory::snapshot();
    let mut first = -1;
    for device in controller_devices(ps.xhci_count == 0) {
        let Some(ctx) = xhci_state::ensure(device) else {
            continue;
        };
        let Some(_guard) = Guard::enter(ctx) else {
            continue;
        };
        let result = unsafe { xhci::activate(ctx, owner_id, source) };
        if result >= 0 && first < 0 {
            first = result;
        }
    }
    first
}
/// Cached PCI inventory only. Configuration-space access stays in driver init.
fn controller_devices(absent: bool) -> impl Iterator<Item = crate::platform::pci_scan::Device> {
    (0..crate::platform::pci_inventory::count())
        .filter_map(crate::platform::pci_inventory::device_at)
        .filter(|d| d.class_code == 12 && d.subclass == 3 && d.prog_if == 0x30)
        .chain(absent.then_some(crate::platform::pci_scan::Device::EMPTY))
}
pub(super) unsafe fn start_port_task() -> bool {
    let mut all = true;
    for ctx in xhci_state::instances() {
        let Some(_guard) = Guard::enter(ctx) else {
            all = false;
            continue;
        };
        all &= unsafe { xhci::start_port_task(ctx) };
    }
    all
}
pub(super) fn controller_name(id: u32) -> Option<crate::sched::sync_types::Text> {
    xhci_state::for_id(id).map(Context::name)
}
pub(super) unsafe fn host_endpoint_handle(handle: EndpointHandle) -> HostEndpoint {
    let Some(_guard) = Guard::for_device(handle.device) else {
        return HostEndpoint::EMPTY;
    };
    let ctx = _guard.ctx;
    unsafe { xhci::usb_host_endpoint_handle(ctx, handle) }
}

impl Guard {
    pub(super) fn for_id(id: u32) -> Option<Self> {
        let ctx = xhci_state::for_id(id)?;
        let guard = Self::enter(ctx)?;
        if ctx.id() == id { Some(guard) } else { None }
    }
    pub(super) fn for_device(handle: DeviceHandle) -> Option<Self> {
        Self::for_id(handle.controller_id)
    }
}
pub(super) unsafe fn device_status(handle: DeviceHandle) -> Status {
    let mut out = MaybeUninit::uninit();
    if unsafe { status_for_device(handle, &mut out) } {
        unsafe { out.assume_init() }
    } else {
        Status::EMPTY
    }
}
/// Representative legacy log; PCI diagnosis selects its exact instance.
pub(super) unsafe fn status() -> Status {
    xhci_state::first().map_or(Status::EMPTY, |ctx| unsafe { xhci::status(ctx) })
}
pub(super) fn set_topology_hook(hook: Option<super::xhci_state::TopologyHook>) {
    for ctx in xhci_state::instances() {
        xhci::set_topology_hook(ctx, hook);
    }
}
pub(super) unsafe fn begin_transaction(
    handle: *mut DeviceHandle,
) -> Option<super::xhci_types::DeviceTransaction> {
    let ctx = xhci_state::for_id(unsafe { (*handle).controller_id })?;
    unsafe { owner::begin_transaction(ctx, handle) }
}
pub(super) unsafe fn end_transaction(transaction: super::xhci_types::DeviceTransaction) {
    let ctx = xhci_state::for_id(transaction.controller_id).expect("retained USB controller epoch");
    unsafe { owner::end_transaction(ctx, transaction) }
}
pub(super) unsafe fn usb_host_endpoint_handle(handle: EndpointHandle) -> HostEndpoint {
    unsafe { host_endpoint_handle(handle) }
}

pub(super) unsafe fn device_handle_from_core(device: super::types::Device) -> DeviceHandle {
    let Some(g) = Guard::for_id(device.controller_id) else {
        return DeviceHandle::EMPTY;
    };
    unsafe { xhci::device_handle_from_core(g.ctx, device) }
}

pub(super) unsafe fn bulk_in_handle_from_core(device: super::types::Device) -> EndpointHandle {
    let Some(g) = Guard::for_id(device.controller_id) else {
        return EndpointHandle::EMPTY;
    };
    unsafe { xhci::bulk_in_handle_from_core(g.ctx, device) }
}

pub(super) unsafe fn bulk_out_handle_from_core(device: super::types::Device) -> EndpointHandle {
    let Some(g) = Guard::for_id(device.controller_id) else {
        return EndpointHandle::EMPTY;
    };
    unsafe { xhci::bulk_out_handle_from_core(g.ctx, device) }
}

pub(super) unsafe fn device_handle_current(handle: DeviceHandle) -> bool {
    let Some(g) = Guard::for_device(handle) else {
        return false;
    };
    unsafe { xhci::device_handle_current(g.ctx, handle) }
}
pub(super) unsafe fn last_interrupt_actual_length(handle: DeviceHandle) -> usize {
    unsafe { device_status(handle) }.last_interrupt_actual_len as usize
}
pub(super) unsafe fn last_interrupt_requested_length(handle: DeviceHandle) -> usize {
    unsafe { device_status(handle) }.last_interrupt_request_len as usize
}
pub(super) unsafe fn last_interrupt_residue(handle: DeviceHandle) -> u32 {
    unsafe { device_status(handle) }.last_interrupt_residue
}
use super::wait_types::Deadline;

pub(super) unsafe fn set_configuration_for_handle(handle: *mut DeviceHandle) -> bool {
    let Some(g) = Guard::for_device(unsafe { *handle }) else {
        return false;
    };
    unsafe { xhci::set_configuration_for_handle(g.ctx, handle) }
}

pub(super) unsafe fn reset_mass_storage_interface_for_handle(
    handle: *mut DeviceHandle,
    interface: u8,
    budget: Option<&Deadline>,
) -> bool {
    let Some(g) = Guard::for_device(unsafe { *handle }) else {
        return false;
    };
    unsafe { xhci::reset_mass_storage_interface_for_handle(g.ctx, handle, interface, budget) }
}

pub(super) unsafe fn clear_endpoint_halt_for_handle(
    handle: *mut DeviceHandle,
    address: u8,
    budget: Option<&Deadline>,
) -> bool {
    let Some(g) = Guard::for_device(unsafe { *handle }) else {
        return false;
    };
    unsafe { xhci::clear_endpoint_halt_for_handle(g.ctx, handle, address, budget) }
}

pub(super) unsafe fn reconfigure_mass_storage_bulk_endpoints_for_handle_within(
    handle: *mut DeviceHandle,
    input: u8,
    output: u8,
    budget: &Deadline,
) -> bool {
    let Some(g) = Guard::for_device(unsafe { *handle }) else {
        return false;
    };
    unsafe {
        xhci::reconfigure_mass_storage_bulk_endpoints_for_handle_within(
            g.ctx, handle, input, output, budget,
        )
    }
}

pub(super) unsafe fn reset_endpoint_state_for_handle(
    handle: *mut DeviceHandle,
    address: u8,
    budget: Option<&Deadline>,
) -> bool {
    let Some(g) = Guard::for_device(unsafe { *handle }) else {
        return false;
    };
    unsafe { xhci::reset_endpoint_state_for_handle(g.ctx, handle, address, budget) }
}

pub(super) unsafe fn set_hid_boot_protocol_for_handle(
    handle: *mut DeviceHandle,
    interface: u8,
) -> bool {
    let Some(g) = Guard::for_device(unsafe { *handle }) else {
        return false;
    };
    unsafe { xhci::set_hid_boot_protocol_for_handle(g.ctx, handle, interface) }
}

pub(super) unsafe fn set_hid_idle_for_handle(handle: *mut DeviceHandle, interface: u8) -> bool {
    let Some(g) = Guard::for_device(unsafe { *handle }) else {
        return false;
    };
    unsafe { xhci::set_hid_idle_for_handle(g.ctx, handle, interface) }
}

pub(super) unsafe fn get_hid_report_descriptor_for_handle(
    handle: *mut DeviceHandle,
    interface: u8,
    expected: u16,
    out: &mut [u8],
) -> Option<usize> {
    let g = Guard::for_device(unsafe { *handle })?;
    unsafe { xhci::get_hid_report_descriptor_for_handle(g.ctx, handle, interface, expected, out) }
}

pub(super) unsafe fn configure_interrupt_in_endpoint_handle(handle: *mut EndpointHandle) -> bool {
    let Some(g) = Guard::for_device(unsafe { (*handle).device }) else {
        return false;
    };
    unsafe { xhci::configure_interrupt_in_endpoint_handle(g.ctx, handle) }
}

pub(super) unsafe fn configure_bulk_endpoint_handles(
    input: *mut EndpointHandle,
    output: *mut EndpointHandle,
) -> bool {
    let Some(g) = Guard::for_device(unsafe { (*input).device }) else {
        return false;
    };
    unsafe { xhci::configure_bulk_endpoint_handles(g.ctx, input, output) }
}

pub(super) unsafe fn select_device_handle(handle: *mut DeviceHandle) -> bool {
    let Some(g) = Guard::for_device(unsafe { *handle }) else {
        return false;
    };
    unsafe { xhci::select_device_handle(g.ctx, handle) }
}

// Only the resident HID owner supplies this wake-only callback. It cannot
// consume a report or borrow class admission while controller state is owned.
pub(super) unsafe fn input_interest(
    endpoint: EndpointHandle,
    hook: Option<super::xhci_state::TopologyHook>,
) -> bool {
    let Some(g) = Guard::for_id(endpoint.device.controller_id) else {
        return false;
    };
    unsafe {
        let s = xhci_state::state(g.ctx);
        let Some(i) =
            xhci::runtime_index_for_identity(g.ctx, endpoint.device.slot_id, endpoint.device.port)
        else {
            return hook.is_none();
        };
        if (*s).runtimes[i].generation != endpoint.device.generation {
            return hook.is_none();
        }
        if let Some(callback) = hook {
            let current_endpoint = if (*s).active_runtime_index == Some(i) {
                (*xhci_state::current(g.ctx)).interrupt_endpoint_id
            } else {
                (*s).runtimes[i].interrupt_endpoint_id
            };
            if endpoint.kind != 1
                || current_endpoint != endpoint.endpoint_id
                || !xhci::device_handle_current(g.ctx, endpoint.device)
            {
                return false;
            }
            (*s).input_hook = Some(callback);
            (*s).input_generations[i] = endpoint.device.generation;
            (*s).input_endpoints[i] = endpoint.endpoint_id;
        } else {
            (*s).input_generations[i] = 0;
            (*s).input_endpoints[i] = 0;
        }
    }
    true
}
