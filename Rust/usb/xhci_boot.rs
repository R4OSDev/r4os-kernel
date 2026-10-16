//! Original firmware handoff, controller stop/reset/run and DMA bootstrap.
//! All allocations retain their exact pointer until hardware quiescence; a
//! failed halt keeps DMA resident. Only the controller owner invokes this code.
use super::{
    timing,
    wait_types::Wait,
    xhci_constants::*,
    xhci_hardware::{alloc_frame_zero, dma_fence, scratchpad_count},
    xhci_io::*,
    xhci_runtime::{free_all_runtimes, free_dma_frame, persist_active_runtime},
    xhci_state::{current, state},
    xhci_types::{ErstEntry, Trb},
};
use crate::{memory::phys, platform::pci_inventory as pci, sched::sync_types::Text};
use core::ptr;
pub(super) unsafe fn enable_pci_memory_and_bus_master() {
    unsafe {
        let wanted = (*current()).command_before | PCI_COMMAND_MEMORY | PCI_COMMAND_BUS_MASTER;
        pci::write_command((*current()).device, wanted);
        (*current()).command_after = pci::read_command((*current()).device);
    }
}
pub(super) unsafe fn read_capabilities() {
    unsafe {
        let c = current();
        (*c).caplength = read8(CAP_CAPLENGTH);
        (*c).hciversion = read16(CAP_CAPLENGTH + 2);
        (*c).hcsparams1 = read32(CAP_HCSPARAMS1);
        (*c).hcsparams2 = read32(CAP_HCSPARAMS2);
        (*c).hcsparams3 = read32(CAP_HCSPARAMS3);
        (*c).hccparams1 = read32(CAP_HCCPARAMS1);
        (*c).dboff = read32(CAP_DBOFF);
        (*c).rtsoff = read32(CAP_RTSOFF);
        (*c).hccparams2 = read32(CAP_HCCPARAMS2);
        (*c).max_slots = ((*c).hcsparams1 & 255) as u8;
        (*c).max_interrupters = (((*c).hcsparams1 >> 8) & 0x7ff) as u16;
        (*c).max_ports = (((*c).hcsparams1 >> 24) & 255) as u8;
        (*c).op_virt = (*c).mmio_virt + u64::from((*c).caplength);
        (*c).runtime_virt = (*c).mmio_virt + u64::from((*c).rtsoff & 0xffff_ffe0);
    }
}
pub(super) unsafe fn handle_bios_handoff() {
    unsafe {
        let c = current();
        let xecp = ((*c).hccparams1 >> 16) & 0xffff;
        if xecp == 0 {
            (*c).bios_handoff = Text::literal(b"no-extended-caps");
            return;
        }
        let (mut offset, mut guard) = (xecp * 4, 0u32);
        while offset >= 0x20 && u64::from(offset + 4) <= MAP_BYTES && guard < 32 {
            let cap = read32(u64::from(offset));
            let id = (cap & 255) as u8;
            let next = ((cap >> 8) & 255) as u8;
            if id == XCAP_ID_LEGACY {
                (*c).legacy_cap_offset = offset;
                (*c).legacy_usblegsup_before = read32(u64::from(offset));
                if (*c).legacy_usblegsup_before & USBLEGSUP_BIOS_OWNED == 0 {
                    (*c).legacy_usblegsup_after = (*c).legacy_usblegsup_before;
                    (*c).bios_handoff = Text::literal(b"not-needed");
                    return;
                }
                write32(
                    u64::from(offset),
                    (*c).legacy_usblegsup_before | USBLEGSUP_OS_OWNED,
                );
                let mut wait = Wait::begin(
                    timing::XHCI_BIOS_HANDOFF_TIMEOUT_MS,
                    Text::literal(b"xhci-bios-handoff"),
                    0,
                );
                while read32(u64::from(offset)) & USBLEGSUP_BIOS_OWNED != 0 && !wait.expired() {
                    wait.idle();
                }
                (*c).legacy_usblegsup_after = read32(u64::from(offset));
                let released = (*c).legacy_usblegsup_after & USBLEGSUP_BIOS_OWNED == 0;
                wait.finish(released);
                if released {
                    (*c).bios_handoff = Text::literal(b"ok");
                } else {
                    (*c).timeouts += 1;
                    (*c).bios_handoff = Text::literal(b"timeout");
                }
                return;
            }
            if next == 0 {
                break;
            }
            offset += u32::from(next) * 4;
            guard += 1;
        }
        (*c).bios_handoff = Text::literal(b"no-legacy-cap");
    }
}
pub(super) unsafe fn stop_controller() -> bool {
    unsafe {
        let command = read_op32(OP_USBCMD) & !USBCMD_RUN;
        write_op32(OP_USBCMD, command);
        if !wait_op_set(
            OP_USBSTS,
            USBSTS_HCH,
            timing::XHCI_CONTROLLER_HALT_TIMEOUT_MS,
            Text::literal(b"xhci-controller-halt"),
        ) {
            (*current()).timeouts += 1;
            (*current()).reason = Text::literal(b"timeout waiting for xHCI halt");
            return false;
        }
        (*current()).controller_stopped = true;
        (*current()).controller_running = false;
        true
    }
}
pub(super) unsafe fn reset_controller() -> bool {
    unsafe {
        write_op32(OP_USBCMD, read_op32(OP_USBCMD) | USBCMD_HCRST);
        if !wait_op_clear(
            OP_USBCMD,
            USBCMD_HCRST,
            timing::XHCI_CONTROLLER_RESET_TIMEOUT_MS,
            Text::literal(b"xhci-controller-reset"),
        ) {
            (*current()).timeouts += 1;
            (*current()).reason = Text::literal(b"timeout waiting for xHCI reset bit clear");
            return false;
        }
        if !wait_op_clear(
            OP_USBSTS,
            USBSTS_CNR,
            timing::XHCI_CONTROLLER_READY_TIMEOUT_MS,
            Text::literal(b"xhci-controller-ready"),
        ) {
            (*current()).timeouts += 1;
            (*current()).reason = Text::literal(b"timeout waiting for controller not ready");
            return false;
        }
        write_op32(OP_USBSTS, USBSTS_EINT | USBSTS_PCD | USBSTS_HCE);
        (*current()).controller_reset_ok = true;
        true
    }
}
pub(super) unsafe fn fail_alloc(_name: &[u8]) -> bool {
    unsafe {
        (*current()).failures += 1;
        (*current()).reason = Text::literal(b"failed to allocate xHCI DMA frame");
        false
    }
}
pub(super) unsafe fn alloc_controller_memory() -> bool {
    unsafe {
        let c = current();
        (*c).dcbaa_phys = match alloc_frame_zero() {
            Some(v) => v,
            None => return fail_alloc(b"dcbaa"),
        };
        (*c).command_ring_phys = match alloc_frame_zero() {
            Some(v) => v,
            None => return fail_alloc(b"command ring"),
        };
        (*c).event_ring_phys = match alloc_frame_zero() {
            Some(v) => v,
            None => return fail_alloc(b"event ring"),
        };
        (*c).erst_phys = match alloc_frame_zero() {
            Some(v) => v,
            None => return fail_alloc(b"erst"),
        };
        let s = state();
        (*s).dcbaa_virt = phys::r4os_rust_phys_to_virt((*c).dcbaa_phys);
        (*s).command_ring_virt = phys::r4os_rust_phys_to_virt((*c).command_ring_phys);
        (*s).event_ring_virt = phys::r4os_rust_phys_to_virt((*c).event_ring_phys);
        (*s).erst_virt = phys::r4os_rust_phys_to_virt((*c).erst_phys);
        (*c).context_size = if (*c).hccparams1 & (1 << 2) != 0 {
            64
        } else {
            32
        };
        if !alloc_scratchpads() {
            return false;
        }
        (*c).dma_ready = true;
        true
    }
}
pub(super) unsafe fn alloc_scratchpads() -> bool {
    unsafe {
        let c = current();
        let count = scratchpad_count(c);
        (*c).scratchpad_count = count;
        if count == 0 {
            return true;
        }
        if usize::from(count) > MAX_SCRATCHPADS {
            (*c).failures += 1;
            (*c).reason = Text::literal(b"too many xHCI scratchpad buffers");
            return false;
        }
        (*c).scratchpad_array_phys = match alloc_frame_zero() {
            Some(v) => v,
            None => return fail_alloc(b"scratchpad array"),
        };
        let s = state();
        (*s).scratchpad_array_virt = phys::r4os_rust_phys_to_virt((*c).scratchpad_array_phys);
        let entries = ptr::with_exposed_provenance_mut::<u64>((*s).scratchpad_array_virt as usize);
        for index in 0..usize::from(count) {
            let frame = match alloc_frame_zero() {
                Some(v) => v,
                None => return fail_alloc(b"scratchpad"),
            };
            (*s).scratchpad_frames[index] = frame;
            entries.add(index).write(frame);
        }
        ptr::with_exposed_provenance_mut::<u64>((*s).dcbaa_virt as usize)
            .write((*c).scratchpad_array_phys);
        true
    }
}
pub(super) unsafe fn free_controller_memory() {
    unsafe {
        let c = current();
        let s = state();
        if (*c).mapped && (*c).op_virt != 0 && (*c).runtime_virt != 0 {
            write_rt32(RT_IR0_IMAN, 0);
            write_rt32(RT_IR0_ERSTSZ, 0);
            write_rt64(RT_IR0_ERSTBA, 0);
            write_rt64(RT_IR0_ERDP, 0);
            write_op32(OP_CONFIG, 0);
            write_op64(OP_CRCR, 0);
            write_op64(OP_DCBAAP, 0);
            dma_fence();
        }
        for index in 0..MAX_SCRATCHPADS {
            free_dma_frame(ptr::addr_of_mut!((*s).scratchpad_frames[index]));
        }
        free_dma_frame(ptr::addr_of_mut!((*c).scratchpad_array_phys));
        free_dma_frame(ptr::addr_of_mut!((*c).erst_phys));
        free_dma_frame(ptr::addr_of_mut!((*c).event_ring_phys));
        free_dma_frame(ptr::addr_of_mut!((*c).command_ring_phys));
        free_dma_frame(ptr::addr_of_mut!((*c).dcbaa_phys));
        (*s).scratchpad_array_virt = 0;
        ptr::addr_of_mut!((*s).scratchpad_frames).write([0; 32]);
        (*s).erst_virt = 0;
        (*s).event_ring_virt = 0;
        (*s).command_ring_virt = 0;
        (*s).dcbaa_virt = 0;
        (*c).scratchpad_count = 0;
        (*c).dma_ready = false;
    }
}
pub(super) unsafe fn init_command_ring() {
    unsafe {
        let s = state();
        let ring = ptr::with_exposed_provenance_mut::<Trb>((*s).command_ring_virt as usize);
        for index in 0..COMMAND_TRB_COUNT {
            ring.add(index).write(Trb::EMPTY);
        }
        ring.add(COMMAND_TRB_COUNT - 1).write(Trb {
            parameter: (*current()).command_ring_phys,
            status: 0,
            control: super::xhci_hardware::trb_type(TRB_TYPE_LINK) | TRB_LINK_TOGGLE_CYCLE,
        });
        (*current()).command_enqueue = 0;
        (*current()).command_cycle = 1;
        (*current()).command_ring_wraps = 0;
    }
}
pub(super) unsafe fn init_event_ring() {
    unsafe {
        let s = state();
        (*s).deferred_events = super::event_router::Mailbox::EMPTY;
        (*s).transfer_objects = super::transfer_pool::Pool::EMPTY;
        (*s).active_command = None;
        let ring = ptr::with_exposed_provenance_mut::<Trb>((*s).event_ring_virt as usize);
        for index in 0..EVENT_TRB_COUNT {
            ring.add(index).write(Trb::EMPTY);
        }
        ptr::with_exposed_provenance_mut::<ErstEntry>((*s).erst_virt as usize).write(ErstEntry {
            base: (*current()).event_ring_phys,
            size: EVENT_TRB_COUNT as u32,
            reserved: 0,
        });
        (*current()).event_dequeue = 0;
        (*current()).event_cycle = 1;
        (*current()).event_ring_wraps = 0;
        (*current()).event_erdp_commits = 0;
        (*current()).event_max_batch = 0;
    }
}
pub(super) unsafe fn setup_rings_and_contexts() {
    unsafe {
        init_command_ring();
        init_event_ring();
        dma_fence();
        let c = current();
        (*c).max_slots_enabled = (*c).max_slots;
        write_op64(OP_DCBAAP, (*c).dcbaa_phys);
        write_op64(OP_CRCR, (*c).command_ring_phys | CRCR_RCS);
        write_op32(OP_CONFIG, u32::from((*c).max_slots_enabled));
        write_rt32(RT_IR0_IMOD, 0);
        write_rt32(RT_IR0_ERSTSZ, 1);
        write_rt64(RT_IR0_ERDP, (*c).event_ring_phys);
        // ERSTBA is the last publication that starts the hardware event-ring state.
        write_rt64(RT_IR0_ERSTBA, (*c).erst_phys);
        write_rt32(RT_IR0_IMAN, IMAN_IP);
        read_rt32(RT_IR0_IMAN);
        (*c).crcr = read_op64(OP_CRCR);
        (*c).dcbaap = read_op64(OP_DCBAAP);
        (*c).config = read_op32(OP_CONFIG);
        (*c).erstsz0 = read_rt32(RT_IR0_ERSTSZ);
        (*c).erstba0 = read_rt64(RT_IR0_ERSTBA);
        (*c).erdp0 = read_rt64(RT_IR0_ERDP);
        (*c).iman0 = read_rt32(RT_IR0_IMAN);
        (*c).imod0 = read_rt32(RT_IR0_IMOD);
    }
}
pub(super) unsafe fn run_controller() -> bool {
    unsafe {
        write_op32(OP_USBSTS, USBSTS_EINT | USBSTS_PCD | USBSTS_HCE);
        write_op32(OP_USBCMD, read_op32(OP_USBCMD) | USBCMD_RUN);
        if !wait_op_clear(
            OP_USBSTS,
            USBSTS_HCH,
            timing::XHCI_CONTROLLER_RUN_TIMEOUT_MS,
            Text::literal(b"xhci-controller-run"),
        ) {
            (*current()).timeouts += 1;
            (*current()).reason = Text::literal(b"timeout waiting for xHCI run");
            return false;
        }
        (*current()).controller_running = true;
        true
    }
}
pub(super) unsafe fn init_controller() -> bool {
    unsafe {
        (*current()).controller_halted_before = read_op32(OP_USBSTS) & USBSTS_HCH != 0;
        if !stop_controller() || !reset_controller() {
            return false;
        }
        read_capabilities();
        if !alloc_controller_memory() {
            free_controller_memory();
            return false;
        }
        setup_rings_and_contexts();
        if !run_controller() {
            if stop_controller() {
                free_controller_memory();
            }
            return false;
        }
        read_ports();
        super::xhci::reset_connected_ports();
        super::xhci::enumerate_first_connected_device();
        true
    }
}
pub(super) unsafe fn teardown_for_reprobe() -> bool {
    unsafe {
        if !(*current()).probed {
            return true;
        }
        for index in 0..8 {
            if (*state()).runtimes[index].transaction_active {
                return false;
            }
        }
        super::xhci_async::request_port_task_stop();
        super::xhci::stop_event_irq();
        persist_active_runtime();
        if (*current()).mapped && (*current()).mmio_virt != 0 && !stop_controller() {
            return false;
        }
        free_all_runtimes();
        free_controller_memory();
        if (*current()).present {
            pci::write_command((*current()).device, (*current()).command_before);
            (*current()).command_after = pci::read_command((*current()).device);
        }
        super::core::reset();
        true
    }
}
