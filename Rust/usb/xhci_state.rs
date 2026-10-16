//! Sole native controller storage, activated together with xhci.rs. Never
//! install this beside a second active Zig controller owner. All callbacks,
//! waits and DMA cleanup retain raw exact state/generation identities.
use super::{
    event_router::{Mailbox, Match, PortChanges},
    transfer_pool::Pool,
    types::HostDescriptor,
    xhci_types::{DeviceRuntime, IncidentToken, Status},
};
use crate::sched::sync_types::{Event, Text, UnwindGuard, WaitQueue};
use core::cell::UnsafeCell;
pub(super) type TopologyHook = unsafe extern "C" fn();
pub(super) struct State {
    pub current: Status,
    pub command_ring_virt: u64,
    pub event_ring_virt: u64,
    pub erst_virt: u64,
    pub dcbaa_virt: u64,
    pub scratchpad_array_virt: u64,
    pub scratchpad_frames: [u64; 32],
    pub first_device_virt: u64,
    pub first_input_virt: u64,
    pub first_ep0_ring_virt: u64,
    pub first_interrupt_ring_virt: u64,
    pub first_interrupt_buffer_virt: u64,
    pub first_bulk_in_ring_virt: u64,
    pub first_bulk_out_ring_virt: u64,
    pub first_bulk_buffer_virt: u64,
    pub first_descriptor_virt: u64,
    pub runtimes: [DeviceRuntime; 8],
    // Never reset this generation during teardown/reprobe.
    pub runtime_generation: u64,
    pub active_runtime_index: Option<usize>,
    pub deferred_events: Mailbox,
    pub pending_port_changes: PortChanges,
    pub transfer_objects: Pool,
    pub last_sync_transfer_incident: IncidentToken,
    pub active_command: Option<Match>,
    pub event_irq: Event,
    pub activated_owner_id: u32,
    pub controller_ownership: UnwindGuard,
    pub canonical_backend: HostDescriptor,
    pub callbacks_initialized: bool,
}
impl State {
    const EMPTY: Self = Self {
        current: Status::EMPTY,
        command_ring_virt: 0,
        event_ring_virt: 0,
        erst_virt: 0,
        dcbaa_virt: 0,
        scratchpad_array_virt: 0,
        scratchpad_frames: [0; 32],
        first_device_virt: 0,
        first_input_virt: 0,
        first_ep0_ring_virt: 0,
        first_interrupt_ring_virt: 0,
        first_interrupt_buffer_virt: 0,
        first_bulk_in_ring_virt: 0,
        first_bulk_out_ring_virt: 0,
        first_bulk_buffer_virt: 0,
        first_descriptor_virt: 0,
        runtimes: [DeviceRuntime::EMPTY; 8],
        runtime_generation: 0,
        active_runtime_index: None,
        deferred_events: Mailbox::EMPTY,
        pending_port_changes: PortChanges {
            pending_mask: 0,
            events: 0,
            queued: 0,
            coalesced: 0,
            invalid: 0,
            taken: 0,
            retries: 0,
            high_water: 0,
        },
        transfer_objects: Pool::EMPTY,
        last_sync_transfer_incident: IncidentToken::EMPTY,
        active_command: None,
        event_irq: Event::AUTO,
        activated_owner_id: 0,
        controller_ownership: UnwindGuard {
            owner: 0,
            owner_generation: 0,
            boot_owner: false,
            depth: 0,
            name: Text::literal(b"xhci-controller"),
            queue: WaitQueue::EMPTY,
        },
        canonical_backend: HostDescriptor {
            flags: 255,
            ..HostDescriptor::EMPTY
        },
        callbacks_initialized: false,
    };
}
struct Storage(UnsafeCell<State>);
// SAFETY: the controller UnwindGuard and exact generation-bound transactions
// serialize mutations. IRQ interactions retain their original runtime boundary;
// no mutable Rust state reference may span a callback, release or scheduler wait.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State::EMPTY));
pub(super) fn state() -> *mut State {
    STATE.0.get()
}
pub(super) fn current() -> *mut Status {
    unsafe { core::ptr::addr_of_mut!((*state()).current) }
}
