//! Sole native controller assembly. Native callers use fixed copied records
//! and stateless incident/console hooks; no legacy controller state remains.
pub(super) use super::xhci_async::*;
pub(super) use super::xhci_boot::*;
pub(super) use super::xhci_endpoints::*;
pub(super) use super::xhci_enumeration::*;
pub(super) use super::xhci_events::*;
pub(super) use super::xhci_handles::*;
pub(super) use super::xhci_io::*;
pub(super) use super::xhci_owner::{idle_device_wait, selected_allows_mutation};
pub(super) use super::xhci_ports::*;
pub(super) use super::xhci_recovery::*;
pub(super) use super::xhci_rings::*;
pub(super) use super::xhci_runtime::*;
pub(super) use super::xhci_transfers::*;
use super::{
    xhci_state::{current, state},
    xhci_types::{DeviceRuntime, IncidentToken, Status},
};
use crate::sched::sync_types::Text;
use crate::{boot::log, memory::phys, platform::pci_inventory as pci};
use core::ptr;
struct RegistryOutcome(usize);
impl Drop for RegistryOutcome {
    fn drop(&mut self) {
        unsafe {
            super::host_controller::r4os_rust_usb_host_set_state(
                self.0,
                if (*current()).controller_running {
                    1
                } else {
                    2
                },
            );
        }
    }
}
unsafe extern "C" fn registration_log(text: *const u8, len: usize) {
    unsafe { log::r4os_rust_bootlog_write(text, len) }
}
unsafe fn boot_text(text: &[u8]) {
    unsafe { log::r4os_rust_bootlog_write(text.as_ptr(), text.len()) }
}
pub(super) unsafe fn probe_owned(owner_id: u32, source: u32) -> bool {
    unsafe {
        super::xhci_host::install_callbacks();
        close_last_sync_transfer_incident();
        if !teardown_for_reprobe() {
            (*current()).failures += 1;
            (*current()).reason = Text::literal(b"xHCI reprobe teardown could not halt controller");
            boot_text(b"[XHCI][WARN] reprobe teardown retained active DMA\r\n");
            return false;
        }
        // Preserve the resident controller guard, descriptor identity, worker identity
        // and never-reset generation. Only the original probe fields are reinitialized.
        current().write(Status {
            probed: true,
            ..Status::EMPTY
        });
        super::wait::reset_metrics();
        let s = state();
        (*s).command_ring_virt = 0;
        (*s).event_ring_virt = 0;
        (*s).erst_virt = 0;
        (*s).dcbaa_virt = 0;
        (*s).scratchpad_array_virt = 0;
        ptr::addr_of_mut!((*s).scratchpad_frames).write([0; 32]);
        (*s).first_device_virt = 0;
        (*s).first_input_virt = 0;
        (*s).first_ep0_ring_virt = 0;
        (*s).first_interrupt_ring_virt = 0;
        (*s).first_interrupt_buffer_virt = 0;
        (*s).first_bulk_in_ring_virt = 0;
        (*s).first_bulk_out_ring_virt = 0;
        (*s).first_bulk_buffer_virt = 0;
        (*s).first_descriptor_virt = 0;
        ptr::addr_of_mut!((*s).runtimes).write([DeviceRuntime::EMPTY; 8]);
        (*s).active_runtime_index = None;
        ptr::addr_of_mut!((*s).deferred_events).write(super::event_router::Mailbox::EMPTY);
        ptr::addr_of_mut!((*s).pending_port_changes)
            .write(super::event_router::PortChanges::default());
        ptr::addr_of_mut!((*s).transfer_objects).write(super::transfer_pool::Pool::EMPTY);
        (*s).last_sync_transfer_incident = IncidentToken::EMPTY;
        (*s).active_command = None;
        super::xhci_async::reset_after_teardown();
        (*s).activated_owner_id = owner_id;
        super::xhci_async::resume_port_task();
        super::core::reset();
        (*s).canonical_backend.source = source;
        let index = super::host_controller::r4os_rust_usb_host_register(
            b"XHCI".as_ptr(),
            4,
            ptr::addr_of!((*s).canonical_backend),
            owner_id,
            source == 0,
            Some(registration_log),
        );
        if index == usize::MAX {
            (*current()).reason = Text::literal(b"xHCI host registry owner already exists");
            return false;
        }
        (*current()).host_controller_id = super::host_controller::r4os_rust_usb_host_id(index);
        (*current()).owner_id = owner_id;
        (*current()).host_source = source;
        let _registry = RegistryOutcome(index);
        let ps = pci::snapshot();
        if ps.xhci_count == 0 {
            (*current()).reason = Text::literal(b"no xHCI controller found");
            (*current()).bios_handoff = Text::literal(b"not-needed");
            boot_text(b"[XHCI] not found\r\n");
            return false;
        }
        (*current()).present = true;
        (*current()).device = ps.first_xhci;
        let route = pci::read_interrupt_route((*current()).device);
        (*current()).irq_line = route.line;
        (*current()).irq_pin = route.pin;
        (*current()).command_before = pci::read_command((*current()).device);
        (*current()).bar0_raw = pci::read_bar((*current()).device, 0);
        (*current()).bar1_raw = pci::read_bar((*current()).device, 1);
        (*current()).bar_is_io = (*current()).bar0_raw & 1 != 0;
        (*current()).bar_is_64 = ((*current()).bar0_raw >> 1) & 3 == 2;
        if (*current()).bar_is_io {
            (*current()).failures += 1;
            (*current()).reason = Text::literal(b"BAR0 is I/O space, expected MMIO");
            boot_text(b"[XHCI][WARN] BAR0 is I/O space\r\n");
            return false;
        }
        (*current()).mmio_phys = pci::read_bar64((*current()).device, 0) & 0xffff_ffff_ffff_fff0;
        if (*current()).mmio_phys == 0 {
            (*current()).failures += 1;
            (*current()).reason = Text::literal(b"BAR0 MMIO base is zero");
            boot_text(b"[XHCI][WARN] BAR0 MMIO base is zero\r\n");
            return false;
        }
        if !super::xhci_hardware::map_mmio((*current()).mmio_phys, super::xhci_constants::MAP_BYTES)
        {
            (*current()).failures += 1;
            (*current()).reason = Text::literal(b"failed to map xHCI MMIO");
            boot_text(b"[XHCI][WARN] MMIO map failed\r\n");
            return false;
        }
        (*current()).mapped = true;
        (*current()).mmio_virt = phys::r4os_rust_phys_to_virt((*current()).mmio_phys);
        enable_pci_memory_and_bus_master();
        read_capabilities();
        handle_bios_handoff();
        read_operational();
        read_runtime();
        read_ports();
        init_controller();
        read_operational();
        read_runtime();
        read_ports();
        (*current()).reason = Text::literal(if (*current()).get_config_ok {
            b"xHCI controller running; first USB configuration parsed"
        } else if (*current()).get_descriptor_ok {
            b"xHCI controller running; first USB device descriptor read"
        } else if (*current()).address_device_ok {
            b"xHCI controller running; first USB device addressed"
        } else if (*current()).controller_running {
            b"xHCI controller running; USB enumeration core active"
        } else if (*current()).dma_ready {
            b"xHCI DMA prepared; controller did not reach running state"
        } else {
            b"diagnostic MMIO mapped; xHCI init incomplete"
        });
        log_summary();
        true
    }
}
pub(super) unsafe fn log_summary() {
    unsafe {
        boot_text(b"[XHCI] ");
        log::r4os_rust_bootlog_dec(u64::from((*current()).device.bus));
        boot_text(b":");
        log::r4os_rust_bootlog_dec(u64::from((*current()).device.device));
        boot_text(b".");
        log::r4os_rust_bootlog_dec(u64::from((*current()).device.function));
        boot_text(b" mmio=0x");
        log::r4os_rust_bootlog_hex((*current()).mmio_phys, 16);
        boot_text(b" hci=0x");
        log::r4os_rust_bootlog_hex(u64::from((*current()).hciversion), 4);
        boot_text(b" slots=");
        log::r4os_rust_bootlog_dec(u64::from((*current()).max_slots));
        boot_text(b" ports=");
        log::r4os_rust_bootlog_dec(u64::from((*current()).max_ports));
        boot_text(b" sts=0x");
        log::r4os_rust_bootlog_hex(u64::from((*current()).usbsts), 8);
        boot_text(b" handoff=");
        let label = (*current()).bios_handoff;
        log::r4os_rust_bootlog_write(label.pointer, label.len);
        boot_text(b"\r\n");
    }
}

pub(super) use super::xhci_hardware::{
    dma_fence, trb_type, volatile_read32, volatile_read64, volatile_write32,
};
pub(super) unsafe fn status() -> Status {
    // Silent admission also protects operational reads by callers already
    // holding this reentrant guard. An unavailable read is explicit.
    let guard = unsafe { core::ptr::addr_of_mut!((*state()).controller_ownership) };
    if !unsafe {
        crate::sched::sync_locks::guard_enter(
            guard,
            super::xhci_constants::CONTROLLER_OWNERSHIP_TIMEOUT_TICKS,
        )
    } {
        return Status {
            reason: Text::literal(b"xHCI controller busy; snapshot unavailable"),
            ..Status::EMPTY
        };
    }
    let result = unsafe { super::xhci_status::status_locked() };
    unsafe {
        crate::sched::sync_locks::guard_leave(guard);
    }
    result
}
pub(super) unsafe fn activate(owner_id: u32, source: u32) -> i32 {
    unsafe {
        if super::host_controller::r4os_rust_usb_host_find(b"XHCI".as_ptr(), 4) != usize::MAX {
            return -2;
        }
        probe_owned(owner_id, source);
        let i = super::host_controller::r4os_rust_usb_host_find(b"XHCI".as_ptr(), 4);
        if i == usize::MAX {
            -3
        } else {
            i32::try_from(i).expect("USB host index")
        }
    }
}
pub(super) unsafe fn last_interrupt_actual_length() -> usize {
    unsafe { (*current()).last_interrupt_actual_len as usize }
}
pub(super) unsafe fn last_interrupt_requested_length() -> usize {
    unsafe { (*current()).last_interrupt_request_len as usize }
}
pub(super) unsafe fn last_interrupt_residue() -> u32 {
    unsafe { (*current()).last_interrupt_residue }
}

pub(super) fn resolve_incident(token: IncidentToken) {
    super::xhci_diag::resolve(token);
}
pub(super) unsafe fn submit_control(request: super::xhci_types::ControlRequest) -> bool {
    unsafe { submit_control_within(request, None) }
}
