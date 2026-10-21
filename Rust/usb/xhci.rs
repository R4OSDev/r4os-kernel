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
use super::xhci_state::Context;
pub(super) use super::xhci_transfers::*;
use super::{
    xhci_state::{current, state},
    xhci_types::{DeviceRuntime, IncidentToken, Status},
};
use crate::sched::sync_types::Text;
use crate::{boot::log, memory::phys, platform::pci_inventory as pci};
use core::ptr;
struct RegistryOutcome(usize, Context);
impl Drop for RegistryOutcome {
    fn drop(&mut self) {
        let ctx = self.1;
        unsafe {
            super::host_controller::r4os_rust_usb_host_set_state(
                self.0,
                if (*current(ctx)).controller_running {
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
pub(super) unsafe fn probe_owned(ctx: Context, owner_id: u32, source: u32) -> bool {
    unsafe {
        super::xhci_host::install_callbacks(ctx);
        close_last_sync_transfer_incident(ctx);
        if !teardown_for_reprobe(ctx) {
            (*current(ctx)).failures += 1;
            (*current(ctx)).reason =
                Text::literal(b"xHCI reprobe teardown could not halt controller");
            boot_text(b"[XHCI][WARN] reprobe teardown retained active DMA\r\n");
            return false;
        }
        // Preserve the resident controller guard, descriptor identity, worker identity
        // and never-reset generation. Only the original probe fields are reinitialized.
        current(ctx).write(Status {
            probed: true,
            ..Status::EMPTY
        });
        super::wait::reset_metrics();
        let s = state(ctx);
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
        super::xhci_async::reset_after_teardown(ctx);
        (*s).activated_owner_id = owner_id;
        super::xhci_async::resume_port_task(ctx);
        super::core::remove_controller(ctx.id());
        (*s).canonical_backend.source = source;
        let index = super::host_controller::r4os_rust_usb_host_register(
            ctx.name().pointer,
            ctx.name().len,
            ptr::addr_of!((*s).canonical_backend),
            owner_id,
            source == 0,
            Some(registration_log),
        );
        if index == usize::MAX {
            (*current(ctx)).reason = Text::literal(b"xHCI host registry owner already exists");
            return false;
        }
        (*current(ctx)).host_controller_id = super::host_controller::r4os_rust_usb_host_id(index);
        ctx.publish_id((*current(ctx)).host_controller_id);
        (*current(ctx)).owner_id = owner_id;
        (*current(ctx)).host_source = source;
        let _registry = RegistryOutcome(index, ctx);
        let ps = pci::snapshot();
        if ps.xhci_count == 0 {
            (*current(ctx)).reason = Text::literal(b"no xHCI controller found");
            (*current(ctx)).bios_handoff = Text::literal(b"not-needed");
            boot_text(b"[XHCI] not found\r\n");
            return false;
        }
        (*current(ctx)).present = true;
        (*current(ctx)).device = (*state(ctx)).pci_device;
        let route = pci::read_interrupt_route((*current(ctx)).device);
        (*current(ctx)).irq_line = route.line;
        (*current(ctx)).irq_pin = route.pin;
        (*current(ctx)).command_before = pci::read_command((*current(ctx)).device);
        (*current(ctx)).bar0_raw = pci::read_bar((*current(ctx)).device, 0);
        (*current(ctx)).bar1_raw = pci::read_bar((*current(ctx)).device, 1);
        (*current(ctx)).bar_is_io = (*current(ctx)).bar0_raw & 1 != 0;
        (*current(ctx)).bar_is_64 = ((*current(ctx)).bar0_raw >> 1) & 3 == 2;
        if (*current(ctx)).bar_is_io {
            (*current(ctx)).failures += 1;
            (*current(ctx)).reason = Text::literal(b"BAR0 is I/O space, expected MMIO");
            boot_text(b"[XHCI][WARN] BAR0 is I/O space\r\n");
            return false;
        }
        (*current(ctx)).mmio_phys =
            pci::read_bar64((*current(ctx)).device, 0) & 0xffff_ffff_ffff_fff0;
        if (*current(ctx)).mmio_phys == 0 {
            (*current(ctx)).failures += 1;
            (*current(ctx)).reason = Text::literal(b"BAR0 MMIO base is zero");
            boot_text(b"[XHCI][WARN] BAR0 MMIO base is zero\r\n");
            return false;
        }
        if !super::xhci_hardware::map_mmio(
            (*current(ctx)).mmio_phys,
            super::xhci_constants::MAP_BYTES,
        ) {
            (*current(ctx)).failures += 1;
            (*current(ctx)).reason = Text::literal(b"failed to map xHCI MMIO");
            boot_text(b"[XHCI][WARN] MMIO map failed\r\n");
            return false;
        }
        (*current(ctx)).mapped = true;
        (*current(ctx)).mmio_virt = phys::r4os_rust_phys_to_virt((*current(ctx)).mmio_phys);
        enable_pci_memory_and_bus_master(ctx);
        read_capabilities(ctx);
        handle_bios_handoff(ctx);
        read_operational(ctx);
        read_runtime(ctx);
        read_ports(ctx);
        init_controller(ctx);
        read_operational(ctx);
        read_runtime(ctx);
        read_ports(ctx);
        (*current(ctx)).reason = Text::literal(if (*current(ctx)).get_config_ok {
            b"xHCI controller running; first USB configuration parsed"
        } else if (*current(ctx)).get_descriptor_ok {
            b"xHCI controller running; first USB device descriptor read"
        } else if (*current(ctx)).address_device_ok {
            b"xHCI controller running; first USB device addressed"
        } else if (*current(ctx)).controller_running {
            b"xHCI controller running; USB enumeration core active"
        } else if (*current(ctx)).dma_ready {
            b"xHCI DMA prepared; controller did not reach running state"
        } else {
            b"diagnostic MMIO mapped; xHCI init incomplete"
        });
        log_summary(ctx);
        true
    }
}
pub(super) unsafe fn log_summary(ctx: Context) {
    unsafe {
        boot_text(b"[XHCI] ");
        log::r4os_rust_bootlog_dec(u64::from((*current(ctx)).device.bus));
        boot_text(b":");
        log::r4os_rust_bootlog_dec(u64::from((*current(ctx)).device.device));
        boot_text(b".");
        log::r4os_rust_bootlog_dec(u64::from((*current(ctx)).device.function));
        boot_text(b" mmio=0x");
        log::r4os_rust_bootlog_hex((*current(ctx)).mmio_phys, 16);
        boot_text(b" hci=0x");
        log::r4os_rust_bootlog_hex(u64::from((*current(ctx)).hciversion), 4);
        boot_text(b" slots=");
        log::r4os_rust_bootlog_dec(u64::from((*current(ctx)).max_slots));
        boot_text(b" ports=");
        log::r4os_rust_bootlog_dec(u64::from((*current(ctx)).max_ports));
        boot_text(b" sts=0x");
        log::r4os_rust_bootlog_hex(u64::from((*current(ctx)).usbsts), 8);
        boot_text(b" handoff=");
        let label = (*current(ctx)).bios_handoff;
        log::r4os_rust_bootlog_write(label.pointer, label.len);
        boot_text(b"\r\n");
    }
}

pub(super) use super::xhci_hardware::{
    dma_fence, trb_type, volatile_read32, volatile_read64, volatile_write32,
};
pub(super) unsafe fn status(ctx: Context) -> Status {
    // Silent admission also protects operational reads by callers already
    // holding this reentrant guard. An unavailable read is explicit.
    let guard = unsafe { core::ptr::addr_of_mut!((*state(ctx)).controller_ownership) };
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
    let result = unsafe { super::xhci_status::status_locked(ctx) };
    unsafe {
        crate::sched::sync_locks::guard_leave(guard);
    }
    result
}
pub(super) unsafe fn activate(ctx: Context, owner_id: u32, source: u32) -> i32 {
    unsafe {
        if super::host_controller::r4os_rust_usb_host_find(b"XHCI".as_ptr(), 4) != usize::MAX {
            return -2;
        }
        probe_owned(ctx, owner_id, source);
        let i = super::host_controller::r4os_rust_usb_host_find(b"XHCI".as_ptr(), 4);
        if i == usize::MAX {
            -3
        } else {
            i32::try_from(i).expect("USB host index")
        }
    }
}
pub(super) unsafe fn last_interrupt_actual_length(ctx: Context) -> usize {
    unsafe { (*current(ctx)).last_interrupt_actual_len as usize }
}

pub(super) fn resolve_incident(_ctx: Context, token: IncidentToken) {
    super::xhci_diag::resolve(token);
}
pub(super) unsafe fn submit_control(
    ctx: Context,
    request: super::xhci_types::ControlRequest,
) -> bool {
    unsafe { submit_control_within(ctx, request, None) }
}
