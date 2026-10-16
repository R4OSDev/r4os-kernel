//! Original DMA-ring publication and producer/link cycle transitions. No
//! second ring cursor exists: all positions belong to the current device owner.
use super::{
    ring_cycle, trb_chain,
    xhci_constants::*,
    xhci_hardware::{dma_fence, max_packet_for_speed, trb_type},
    xhci_state::state,
    xhci_types::{BulkSubmission, BulkTrbSubmission, InterruptSubmission, Trb},
};
use core::ptr;
fn ring(address: u64) -> *mut Trb {
    ptr::with_exposed_provenance_mut(address as usize)
}
pub(super) unsafe fn update_link_trb(address: u64, count: usize, cycle: u8, continues: bool) {
    unsafe {
        if address == 0 || count == 0 {
            return;
        }
        let p = ring(address).add(count - 1);
        dma_fence();
        let old = ptr::addr_of!((*p).control).read();
        ptr::addr_of_mut!((*p).control)
            .write(ring_cycle::published_link_control(old, cycle, continues));
        dma_fence();
    }
}
pub(super) unsafe fn init_ring(address: u64, physical: u64) {
    unsafe {
        if address == 0 || physical == 0 {
            return;
        }
        let p = ring(address);
        for index in 0..TRANSFER_TRB_COUNT {
            p.add(index).write(Trb::EMPTY);
        }
        p.add(TRANSFER_TRB_COUNT - 1).write(Trb {
            parameter: physical,
            status: 0,
            control: trb_type(TRB_TYPE_LINK) | TRB_LINK_TOGGLE_CYCLE | TRB_CYCLE,
        });
    }
}
pub(super) unsafe fn init_ep0_ring() {
    unsafe {
        let s = state();
        let p = ring((*s).first_ep0_ring_virt);
        for index in 0..TRANSFER_TRB_COUNT {
            p.add(index).write(Trb::EMPTY);
        }
        p.add(TRANSFER_TRB_COUNT - 1).write(Trb {
            parameter: (*s).current.ep0_ring_phys,
            status: 0,
            control: trb_type(TRB_TYPE_LINK) | TRB_LINK_TOGGLE_CYCLE,
        });
        (*s).current.ep0_enqueue = 0;
        (*s).current.ep0_cycle = 1;
        (*s).current.control_endpoint_faulted = false;
    }
}
pub(super) unsafe fn init_interrupt_ring() {
    unsafe {
        let s = state();
        if let Some(owner) = super::xhci::current_interrupt_owner_match() {
            (*s).deferred_events.purge(owner);
        }
        super::xhci::release_interrupt_transfer(true);
        let p = ring((*s).first_interrupt_ring_virt);
        for index in 0..TRANSFER_TRB_COUNT {
            p.add(index).write(Trb::EMPTY);
        }
        p.add(TRANSFER_TRB_COUNT - 1).write(Trb {
            parameter: (*s).current.interrupt_ring_phys,
            status: 0,
            control: trb_type(TRB_TYPE_LINK) | TRB_LINK_TOGGLE_CYCLE,
        });
        (*s).current.interrupt_enqueue = 0;
        (*s).current.interrupt_cycle = 1;
        (*s).current.interrupt_pending = false;
        (*s).current.interrupt_pending_trb_phys = 0;
        (*s).current.interrupt_transfer_handle = 0;
        (*s).current.interrupt_pending_streak = 0;
    }
}
pub(super) unsafe fn init_bulk_rings() {
    unsafe {
        let s = state();
        init_ring((*s).first_bulk_in_ring_virt, (*s).current.bulk_in_ring_phys);
        init_ring(
            (*s).first_bulk_out_ring_virt,
            (*s).current.bulk_out_ring_phys,
        );
        (*s).current.bulk_in_enqueue = 0;
        (*s).current.bulk_in_cycle = 1;
        (*s).current.bulk_out_enqueue = 0;
        (*s).current.bulk_out_cycle = 1;
    }
}
pub(super) unsafe fn setup_first_input_context(port_index: usize) {
    unsafe {
        let s = state();
        let input = ptr::with_exposed_provenance_mut::<u32>((*s).first_input_virt as usize);
        let count = usize::from((*s).current.context_size) / 4;
        input.write(0);
        input.add(1).write(3);
        let slot = input.add(count);
        slot.write((u32::from((*s).current.addressed_speed) << 20) | (1 << 27));
        slot.add(1)
            .write(u32::from((*s).current.first_ports[port_index].index) << 16);
        slot.add(2).write(0);
        slot.add(3).write(0);
        let ep0 = input.add(count * 2);
        ep0.write(0);
        ep0.add(1).write(
            (3 << 1)
                | (4 << 3)
                | (u32::from(max_packet_for_speed((*s).current.addressed_speed)) << 16),
        );
        ep0.add(2).write(((*s).current.ep0_ring_phys | 1) as u32);
        ep0.add(3)
            .write((((*s).current.ep0_ring_phys | 1) >> 32) as u32);
        ep0.add(4).write(8);
    }
}
pub(super) unsafe fn write_bulk_trb(
    input: bool,
    parameter: u64,
    status: u32,
    control: u32,
) -> BulkTrbSubmission {
    unsafe {
        let s = state();
        let (address, physical, enqueue, producer) = if input {
            (
                (*s).first_bulk_in_ring_virt,
                (*s).current.bulk_in_ring_phys,
                (*s).current.bulk_in_enqueue,
                (*s).current.bulk_in_cycle,
            )
        } else {
            (
                (*s).first_bulk_out_ring_virt,
                (*s).current.bulk_out_ring_phys,
                (*s).current.bulk_out_enqueue,
                (*s).current.bulk_out_cycle,
            )
        };
        let trb_phys = physical + u64::from(enqueue) * core::mem::size_of::<Trb>() as u64;
        let cycle = if producer != 0 { TRB_CYCLE } else { 0 };
        ring(address).add(usize::from(enqueue)).write(Trb {
            parameter,
            status,
            control: control | cycle,
        });
        let advance = ring_cycle::after_submission(enqueue, producer, TRANSFER_TRB_COUNT);
        if advance.wrapped {
            update_link_trb(
                address,
                TRANSFER_TRB_COUNT,
                advance.link_cycle,
                control & TRB_CHAIN != 0,
            );
        }
        if input {
            (*s).current.bulk_in_enqueue = advance.next_enqueue;
            (*s).current.bulk_in_cycle = advance.next_producer_cycle;
            (*s).current.bulk_in_link_update_pending = false;
            if advance.wrapped {
                (*s).current.bulk_in_ring_wraps += 1;
            }
        } else {
            (*s).current.bulk_out_enqueue = advance.next_enqueue;
            (*s).current.bulk_out_cycle = advance.next_producer_cycle;
            (*s).current.bulk_out_link_update_pending = false;
            if advance.wrapped {
                (*s).current.bulk_out_ring_wraps += 1;
            }
        }
        BulkTrbSubmission { trb_phys }
    }
}
pub(super) unsafe fn write_bulk_trb_chain(
    input: bool,
    parameter: u64,
    bytes: u32,
) -> Option<BulkSubmission> {
    let plan = trb_chain::build(parameter, bytes)?;
    let mut out = BulkSubmission::EMPTY;
    let count = usize::from(plan.count);
    for index in 0..count {
        let segment = plan.segments[index];
        let remaining = u32::try_from(count - index - 1).expect("USB TRB count");
        let status = segment.len | (remaining.min(31) << 17);
        let last = index + 1 == count;
        let control = trb_type(TRB_TYPE_NORMAL) | if last { TRB_IOC } else { TRB_CHAIN };
        let submission = unsafe { write_bulk_trb(input, segment.phys, status, control) };
        out.trb_phys[usize::from(out.trb_count)] = submission.trb_phys;
        out.trb_count += 1;
    }
    Some(out)
}
pub(super) unsafe fn write_interrupt_trb(
    parameter: u64,
    status: u32,
    control: u32,
) -> InterruptSubmission {
    unsafe {
        let s = state();
        let enqueue = (*s).current.interrupt_enqueue;
        let producer = (*s).current.interrupt_cycle;
        let physical = (*s).current.interrupt_ring_phys
            + u64::from(enqueue) * core::mem::size_of::<Trb>() as u64;
        let cycle = if producer != 0 { TRB_CYCLE } else { 0 };
        ring((*s).first_interrupt_ring_virt)
            .add(usize::from(enqueue))
            .write(Trb {
                parameter,
                status,
                control: control | cycle,
            });
        let advance = ring_cycle::after_submission(enqueue, producer, TRANSFER_TRB_COUNT);
        if advance.wrapped {
            update_link_trb(
                (*s).first_interrupt_ring_virt,
                TRANSFER_TRB_COUNT,
                advance.link_cycle,
                false,
            );
        }
        (*s).current.interrupt_enqueue = advance.next_enqueue;
        (*s).current.interrupt_cycle = advance.next_producer_cycle;
        InterruptSubmission {
            trb_phys: physical,
            enqueue,
            producer_cycle: producer,
            advance,
        }
    }
}
pub(super) unsafe fn write_transfer_trb(parameter: u64, status: u32, control: u32) -> u64 {
    unsafe {
        let s = state();
        let enqueue = (*s).current.ep0_enqueue;
        let producer = (*s).current.ep0_cycle;
        let physical =
            (*s).current.ep0_ring_phys + u64::from(enqueue) * core::mem::size_of::<Trb>() as u64;
        let cycle = if producer != 0 { TRB_CYCLE } else { 0 };
        ring((*s).first_ep0_ring_virt)
            .add(usize::from(enqueue))
            .write(Trb {
                parameter,
                status,
                control: control | cycle,
            });
        let advance = ring_cycle::after_submission(enqueue, producer, TRANSFER_TRB_COUNT);
        if advance.wrapped {
            update_link_trb(
                (*s).first_ep0_ring_virt,
                TRANSFER_TRB_COUNT,
                advance.link_cycle,
                control & TRB_CHAIN != 0,
            );
            (*s).current.ep0_ring_wraps += 1;
        }
        (*s).current.ep0_enqueue = advance.next_enqueue;
        (*s).current.ep0_cycle = advance.next_producer_cycle;
        physical
    }
}
