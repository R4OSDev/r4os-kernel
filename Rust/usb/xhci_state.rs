//! Stable native controller instances. A copied Context names one resident
//! PCI owner; no task-local or global selection spans a callback or wait.
use super::{
    event_router::{Mailbox, Match, PortChanges},
    transfer_pool::Pool,
    types::HostDescriptor,
    xhci_types::{DeviceRuntime, IncidentToken, Status},
};
use crate::sched::sync_types::{Event, Text, UnwindGuard, WaitQueue};
use crate::{arch::runtime::RuntimeGuard, platform::pci_scan::Device as PciDevice};
use core::cell::UnsafeCell;
use core::{alloc::Layout, ptr};
pub(super) type TopologyHook = unsafe extern "C" fn();
pub(super) struct State {
    pub next: *mut State,
    pub pci_device: PciDevice,
    pub name: [u8; 32],
    pub name_len: usize,
    pub registration_id: core::sync::atomic::AtomicU32,
    pub asynchronous: super::xhci_async::AsyncState,
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
    pub(super) const fn empty() -> Self {
        Self {
            next: ptr::null_mut(),
            pci_device: PciDevice::EMPTY,
            name: [0; 32],
            name_len: 0,
            registration_id: core::sync::atomic::AtomicU32::new(0),
            asynchronous: super::xhci_async::AsyncState::empty(),
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
        }
    }
}
struct Catalogue(UnsafeCell<*mut State>);
// SAFETY: runtime admission covers links only. Instances remain resident, and
// their own UnwindGuard owns hardware/runtime mutation, including after waits.
unsafe impl Sync for Catalogue {}
static CATALOGUE: Catalogue = Catalogue(UnsafeCell::new(ptr::null_mut()));

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Context(*mut State);
impl Context {
    /// The host descriptor/IRQ/task retains this exact resident native context.
    pub(super) unsafe fn from_raw(pointer: *mut core::ffi::c_void) -> Self {
        assert!(!pointer.is_null(), "missing xHCI instance context");
        Self(pointer.cast())
    }
    pub(super) fn raw(self) -> *mut core::ffi::c_void {
        self.0.cast()
    }
    pub(super) fn asynchronous(self) -> &'static super::xhci_async::AsyncState {
        // Instances, including their atomic IRQ/worker records, remain resident.
        unsafe { &(*self.0).asynchronous }
    }
    pub(super) fn name(self) -> Text {
        unsafe {
            Text {
                pointer: (*self.0).name.as_ptr(),
                len: (*self.0).name_len,
            }
        }
    }
    pub(super) fn id(self) -> u32 {
        unsafe {
            (*self.0)
                .registration_id
                .load(core::sync::atomic::Ordering::Acquire)
        }
    }
    pub(super) fn publish_id(self, id: u32) {
        unsafe {
            (*self.0)
                .registration_id
                .store(id, core::sync::atomic::Ordering::Release);
        }
    }
}
pub(super) fn state(ctx: Context) -> *mut State {
    ctx.0
}
pub(super) fn current(ctx: Context) -> *mut Status {
    unsafe { ptr::addr_of_mut!((*ctx.0).current) }
}
pub(super) fn same_pci(a: PciDevice, b: PciDevice) -> bool {
    a.bus == b.bus
        && a.device == b.device
        && a.function == b.function
        && a.vendor_id == b.vendor_id
        && a.device_id == b.device_id
}
pub(super) fn first() -> Option<Context> {
    let _metadata = RuntimeGuard::acquire();
    let p = unsafe { *CATALOGUE.0.get() };
    (!p.is_null()).then_some(Context(p))
}
pub(super) struct Iter(*mut State);
impl Iterator for Iter {
    type Item = Context;
    fn next(&mut self) -> Option<Context> {
        let _metadata = RuntimeGuard::acquire();
        if self.0.is_null() {
            return None;
        }
        let p = self.0;
        self.0 = unsafe { (*p).next };
        Some(Context(p))
    }
}
pub(super) fn instances() -> Iter {
    let _metadata = RuntimeGuard::acquire();
    Iter(unsafe { *CATALOGUE.0.get() })
}
pub(super) fn for_pci(device: PciDevice) -> Option<Context> {
    instances().find(|ctx| unsafe { same_pci((*ctx.0).pci_device, device) })
}
/// Copied registration identity for passive PCI inventory; no hardware access.
pub(crate) fn host_id_for_pci(device: PciDevice) -> u32 {
    for_pci(device).map_or(0, Context::id)
}
pub(super) fn for_id(id: u32) -> Option<Context> {
    if id == 0 {
        return None;
    }
    instances().find(|ctx| ctx.id() == id)
}
pub(super) fn ensure(device: PciDevice) -> Option<Context> {
    if let Some(ctx) = for_pci(device) {
        return Some(ctx);
    }
    let layout = Layout::new::<State>();
    // No runtime/catalogue owner spans allocation or cleanup.
    let p = unsafe { alloc::alloc::alloc(layout).cast::<State>() };
    if p.is_null() {
        return None;
    }
    unsafe {
        p.write(State {
            pci_device: device,
            ..State::empty()
        });
        let hex = b"0123456789ABCDEF";
        let name = [
            b'X',
            b'H',
            b'C',
            b'I',
            b'@',
            hex[(device.bus >> 4) as usize],
            hex[(device.bus & 15) as usize],
            b':',
            hex[(device.device >> 4) as usize],
            hex[(device.device & 15) as usize],
            b'.',
            hex[device.function as usize],
        ];
        ptr::copy_nonoverlapping(
            name.as_ptr(),
            ptr::addr_of_mut!((*p).name).cast::<u8>(),
            name.len(),
        );
        // EMPTY represents the existing unavailable backend and must not bind
        // the real PCI function at 00:00.0 in passive hardware diagnosis.
        (*p).name_len = if device.vendor_id == 0 { 4 } else { name.len() };
        (*p).canonical_backend.context = p.cast();
    }
    let existing = {
        let _metadata = RuntimeGuard::acquire();
        let found = for_pci(device);
        if found.is_none() {
            let mut link = CATALOGUE.0.get();
            unsafe {
                while !(*link).is_null() {
                    link = ptr::addr_of_mut!((**link).next);
                }
                *link = p;
            }
        }
        found
    };
    if let Some(ctx) = existing {
        unsafe {
            alloc::alloc::dealloc(p.cast(), layout);
        }
        Some(ctx)
    } else {
        Some(Context(p))
    }
}
