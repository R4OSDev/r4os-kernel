//! Sole original adapter registry and bounded statistics. Callback records use
//! typed C invokers with an opaque original context, never an assumed Zig ABI.
//! Registry snapshots release metadata admission before callbacks or output.
use super::{backend_contract, timing};
pub use crate::contract::NetBackendStatus as BackendStatus;
use crate::{memory::owner::Guard, sched::sync_types::Text};
use core::cell::UnsafeCell;
pub const CAPACITY: usize = 8;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NativeAdapterStats {
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub drops: u64,
    pub errors: u64,
    pub resets: u64,
    pub last_error: Text,
}
impl NativeAdapterStats {
    const EMPTY: Self = Self {
        rx_packets: 0,
        tx_packets: 0,
        rx_bytes: 0,
        tx_bytes: 0,
        drops: 0,
        errors: 0,
        resets: 0,
        last_error: Text::literal(b"none"),
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NativeAdapterOps {
    pub transmit: Option<unsafe extern "C" fn(usize, usize, *const u8, usize) -> u8>,
    pub transmit_context: usize,
    pub poll: Option<unsafe extern "C" fn(usize, usize)>,
    pub poll_context: usize,
    pub status: Option<unsafe extern "C" fn(usize, usize, *mut BackendStatus) -> i32>,
    pub status_context: usize,
}
impl NativeAdapterOps {
    const EMPTY: Self = Self {
        transmit: None,
        transmit_context: 0,
        poll: None,
        poll_context: 0,
        status: None,
        status_context: 0,
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NativeAdapter {
    pub registration_id: u32,
    pub name: Text,
    pub driver: Text,
    pub bus: u8,
    pub bus_no: u8,
    pub device_no: u8,
    pub function_no: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub mac: [u8; 6],
    pub mtu: u16,
    pub flags: u32,
    pub link: u8,
    pub lifecycle: u8,
    pub registered_tick: u64,
    pub state_changed_tick: u64,
    pub stats: NativeAdapterStats,
    pub backend: backend_contract::Negotiation,
    pub ops: NativeAdapterOps,
}
impl NativeAdapter {
    pub(crate) const EMPTY: Self = Self {
        registration_id: 0,
        name: Text::literal(b""),
        driver: Text::literal(b""),
        bus: 0,
        bus_no: 0,
        device_no: 0,
        function_no: 0,
        vendor_id: 0,
        device_id: 0,
        mac: [0; 6],
        mtu: 1500,
        flags: 5,
        link: 0,
        lifecycle: 1,
        registered_tick: 0,
        state_changed_tick: 0,
        stats: NativeAdapterStats::EMPTY,
        backend: backend_contract::Negotiation {
            version: 1,
            size: 56,
            offered: 0,
            accepted: 0,
            rejected: 0,
            rx_queue_count: 1,
            tx_queue_count: 1,
            max_rx_segments: 1,
            max_tx_segments: 1,
            rx_ownership: 0,
            tx_ownership: 0,
            interrupt_moderation_us: 0,
            reserved: 0,
        },
        ops: NativeAdapterOps::EMPTY,
    };
}
struct State {
    entries: [NativeAdapter; CAPACITY],
    count: usize,
    next_registration: u32,
}
struct Storage(UnsafeCell<State>);
// SAFETY: all table access holds short common network metadata admission.
// Callback drain owns removal/compaction and the lifetime of borrowed names,
// invokers and contexts. No mutable reference escapes an operation.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State {
    entries: [NativeAdapter::EMPTY; CAPACITY],
    count: 0,
    next_registration: 1,
}));
fn state() -> *mut State {
    STATE.0.get()
}
fn lifecycle(link: u8) -> u8 {
    match link {
        0 => 1,
        1 => 3,
        2 => 2,
        _ => panic!("Exact adapter link"),
    }
}
fn set_lifecycle(value: &mut NativeAdapter, phase: u8) {
    assert!(phase <= 5, "Exact adapter lifecycle");
    if value.lifecycle != phase {
        value.lifecycle = phase;
        value.state_changed_tick = timing::now_ticks();
    }
}
// SAFETY: caller holds network admission and uses the reference only until it ends.
unsafe fn entry(index: usize) -> Option<&'static mut NativeAdapter> {
    unsafe {
        if index >= (*state()).count {
            None
        } else {
            Some(&mut (*state()).entries[index])
        }
    }
}
pub(crate) fn snapshot(index: usize) -> Option<NativeAdapter> {
    let _guard = Guard::acquire(2);
    unsafe { entry(index).copied() }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_adapters_reset() {
    let _guard = Guard::acquire(2);
    unsafe {
        let s = &mut *state();
        s.entries = [NativeAdapter::EMPTY; CAPACITY];
        s.count = 0; /* IDs never recycle in the same boot, including repeated init. */
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_adapter_count() -> usize {
    let _guard = Guard::acquire(2);
    unsafe { (*state()).count }
}
/// # Safety
/// Exact paired readable adapter with retained immutable text and callback
/// context/code until removal after callback drain. The caller owns registration.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_adapter_register(input: *const NativeAdapter) -> usize {
    let mut value = unsafe { *input };
    assert!(
        value.bus <= 3 && value.link <= 2 && value.lifecycle <= 5,
        "Exact adapter enum values"
    );
    let _guard = Guard::acquire(2);
    unsafe {
        let s = &mut *state();
        if s.count >= CAPACITY || s.next_registration > 0x7fff_ffff {
            return usize::MAX;
        }
        value.registration_id = s.next_registration;
        s.next_registration += 1;
        let now = timing::now_ticks();
        value.registered_tick = now;
        value.state_changed_tick = now;
        value.lifecycle = lifecycle(value.link);
        let slot = s.count;
        s.entries[slot] = value;
        s.count += 1;
        slot
    }
}
/// # Safety
/// Writable paired output. No caller write spans metadata admission; pointers
/// in the result borrow the registration's retained immutable owner resources.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_adapter_get(index: usize, out: *mut NativeAdapter) -> bool {
    if let Some(value) = snapshot(index) {
        unsafe {
            out.write(value);
        }
        true
    } else {
        false
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_adapter_index(registration: u32) -> usize {
    if registration == 0 {
        return usize::MAX;
    }
    let _guard = Guard::acquire(2);
    unsafe {
        let s = &*state();
        s.entries[..s.count]
            .iter()
            .position(|e| e.registration_id == registration)
            .unwrap_or(usize::MAX)
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_adapter_has_device(vendor: u16, device: u16) -> bool {
    let _guard = Guard::acquire(2);
    unsafe {
        let s = &*state();
        s.entries[..s.count]
            .iter()
            .any(|e| e.vendor_id == vendor && e.device_id == device)
    }
}
/// # Safety
/// The caller owns completed callback/packet admission for compaction. The
/// diagnostic packet pool has already discarded/reindexed its exact leases.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_adapter_remove(index: usize) -> bool {
    let _guard = Guard::acquire(2);
    unsafe {
        let s = &mut *state();
        if index >= s.count {
            return false;
        }
        s.entries.copy_within(index + 1..s.count, index);
        s.count -= 1;
        s.entries[s.count] = NativeAdapter::EMPTY;
        true
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_adapter_lifecycle(index: usize, phase: u8) {
    let _guard = Guard::acquire(2);
    unsafe {
        if let Some(e) = entry(index) {
            set_lifecycle(e, phase);
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_adapter_link(index: usize, link: u8) {
    let _guard = Guard::acquire(2);
    unsafe {
        if let Some(e) = entry(index) {
            e.link = link;
            set_lifecycle(e, lifecycle(link));
        }
    }
}
/// # Safety
/// Retained immutable error text as in the original adapter statistics contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_adapter_error(index: usize, reason: Text) {
    let _guard = Guard::acquire(2);
    unsafe {
        if let Some(e) = entry(index) {
            e.stats.last_error = reason;
        }
    }
}
/// # Safety
/// Retained immutable error text as in the original adapter statistics contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_adapter_drop(index: usize, reason: Text) {
    let _guard = Guard::acquire(2);
    unsafe {
        if let Some(e) = entry(index) {
            e.stats.drops += 1;
            e.stats.last_error = reason;
        }
    }
}
/// # Safety
/// Retained immutable error text as in the original adapter statistics contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_adapter_reset_note(index: usize, reason: Text) {
    let _guard = Guard::acquire(2);
    unsafe {
        if let Some(e) = entry(index) {
            e.stats.resets += 1;
            e.stats.last_error = reason;
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_adapter_rx(index: usize, len: usize) {
    let _guard = Guard::acquire(2);
    unsafe {
        if let Some(e) = entry(index) {
            e.stats.rx_packets += 1;
            e.stats.rx_bytes += len as u64;
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_adapter_tx_ok(index: usize, len: usize) {
    let _guard = Guard::acquire(2);
    unsafe {
        if let Some(e) = entry(index) {
            if e.link == 0 {
                e.link = 2;
            }
            set_lifecycle(e, 2);
            e.stats.tx_packets += 1;
            e.stats.tx_bytes += len as u64;
            e.stats.last_error = Text::literal(b"none");
        }
    }
}
/// # Safety
/// Retained immutable result label, applied after the original link/error cleanup.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_adapter_tx_error(index: usize, reason: Text) {
    let _guard = Guard::acquire(2);
    unsafe {
        if let Some(e) = entry(index) {
            e.stats.errors += 1;
            e.stats.last_error = reason;
        }
    }
}
/// # Safety
/// Caller holds callback admission for this enumeration index; frame remains
/// readable and driver-owned execution stays retained through callback return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_adapter_transmit(
    index: usize,
    data: *const u8,
    len: usize,
) -> u8 {
    let Some(value) = snapshot(index) else {
        return 1;
    };
    let Some(f) = value.ops.transmit else {
        return 5;
    };
    unsafe { f(value.ops.transmit_context, index, data, len) }
}
/// # Safety
/// Caller holds callback admission for this enumeration index through return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_adapter_poll(index: usize) {
    if let Some(value) = snapshot(index)
        && let Some(f) = value.ops.poll
    {
        unsafe {
            f(value.ops.poll_context, index);
        }
    }
}
/// # Safety
/// Caller holds callback admission and a writable canonical backend status.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_adapter_status(
    index: usize,
    out: *mut BackendStatus,
) -> i32 {
    let Some(value) = snapshot(index) else {
        return -1;
    };
    let Some(f) = value.ops.status else {
        return -1;
    };
    unsafe { f(value.ops.status_context, index, out) }
}

const _: () = {
    assert!(core::mem::size_of::<NativeAdapterOps>() == 48);
    assert!(core::mem::align_of::<NativeAdapterOps>() == 8);
    assert!(core::mem::offset_of!(NativeAdapterOps, transmit) == 0);
    assert!(core::mem::offset_of!(NativeAdapterOps, transmit_context) == 8);
    assert!(core::mem::offset_of!(NativeAdapterOps, poll) == 16);
    assert!(core::mem::offset_of!(NativeAdapterOps, poll_context) == 24);
    assert!(core::mem::offset_of!(NativeAdapterOps, status) == 32);
    assert!(core::mem::offset_of!(NativeAdapterOps, status_context) == 40);
};
const _: () = {
    assert!(core::mem::size_of::<NativeAdapterStats>() == 72);
    assert!(core::mem::align_of::<NativeAdapterStats>() == 8);
    assert!(core::mem::offset_of!(NativeAdapterStats, rx_packets) == 0);
    assert!(core::mem::offset_of!(NativeAdapterStats, tx_packets) == 8);
    assert!(core::mem::offset_of!(NativeAdapterStats, rx_bytes) == 16);
    assert!(core::mem::offset_of!(NativeAdapterStats, tx_bytes) == 24);
    assert!(core::mem::offset_of!(NativeAdapterStats, drops) == 32);
    assert!(core::mem::offset_of!(NativeAdapterStats, errors) == 40);
    assert!(core::mem::offset_of!(NativeAdapterStats, resets) == 48);
    assert!(core::mem::offset_of!(NativeAdapterStats, last_error) == 56);
};
const _: () = {
    assert!(core::mem::size_of::<NativeAdapter>() == 256);
    assert!(core::mem::align_of::<NativeAdapter>() == 8);
    assert!(core::mem::offset_of!(NativeAdapter, registration_id) == 0);
    assert!(core::mem::offset_of!(NativeAdapter, name) == 8);
    assert!(core::mem::offset_of!(NativeAdapter, driver) == 24);
    assert!(core::mem::offset_of!(NativeAdapter, bus) == 40);
    assert!(core::mem::offset_of!(NativeAdapter, bus_no) == 41);
    assert!(core::mem::offset_of!(NativeAdapter, device_no) == 42);
    assert!(core::mem::offset_of!(NativeAdapter, function_no) == 43);
    assert!(core::mem::offset_of!(NativeAdapter, vendor_id) == 44);
    assert!(core::mem::offset_of!(NativeAdapter, device_id) == 46);
    assert!(core::mem::offset_of!(NativeAdapter, mac) == 48);
    assert!(core::mem::offset_of!(NativeAdapter, mtu) == 54);
    assert!(core::mem::offset_of!(NativeAdapter, flags) == 56);
    assert!(core::mem::offset_of!(NativeAdapter, link) == 60);
    assert!(core::mem::offset_of!(NativeAdapter, lifecycle) == 61);
    assert!(core::mem::offset_of!(NativeAdapter, registered_tick) == 64);
    assert!(core::mem::offset_of!(NativeAdapter, state_changed_tick) == 72);
    assert!(core::mem::offset_of!(NativeAdapter, stats) == 80);
    assert!(core::mem::offset_of!(NativeAdapter, backend) == 152);
    assert!(core::mem::offset_of!(NativeAdapter, ops) == 208);
};
const _: () = {
    assert!(core::mem::size_of::<BackendStatus>() == 160);
    assert!(core::mem::align_of::<BackendStatus>() == 8);
    assert!(core::mem::offset_of!(BackendStatus, link_up) == 0);
    assert!(core::mem::offset_of!(BackendStatus, rx_packets) == 8);
    assert!(core::mem::offset_of!(BackendStatus, tx_packets) == 16);
    assert!(core::mem::offset_of!(BackendStatus, drops) == 24);
    assert!(core::mem::offset_of!(BackendStatus, errors) == 32);
    assert!(core::mem::offset_of!(BackendStatus, irq_line) == 40);
    assert!(core::mem::offset_of!(BackendStatus, irq_pin) == 41);
    assert!(core::mem::offset_of!(BackendStatus, irq_registered) == 42);
    assert!(core::mem::offset_of!(BackendStatus, irq_mode) == 43);
    assert!(core::mem::offset_of!(BackendStatus, irq_count) == 48);
    assert!(core::mem::offset_of!(BackendStatus, irq_handled) == 56);
    assert!(core::mem::offset_of!(BackendStatus, poll_count) == 64);
    assert!(core::mem::offset_of!(BackendStatus, poll_fallbacks) == 72);
    assert!(core::mem::offset_of!(BackendStatus, last_isr) == 80);
    assert!(core::mem::offset_of!(BackendStatus, reserved) == 82);
    assert!(core::mem::offset_of!(BackendStatus, rx_errors) == 88);
    assert!(core::mem::offset_of!(BackendStatus, tx_errors) == 96);
    assert!(core::mem::offset_of!(BackendStatus, rx_overflows) == 104);
    assert!(core::mem::offset_of!(BackendStatus, rx_recoveries) == 112);
    assert!(core::mem::offset_of!(BackendStatus, offered_capabilities) == 120);
    assert!(core::mem::offset_of!(BackendStatus, accepted_capabilities) == 128);
    assert!(core::mem::offset_of!(BackendStatus, rx_offload_packets) == 136);
    assert!(core::mem::offset_of!(BackendStatus, rx_software_fallbacks) == 144);
    assert!(core::mem::offset_of!(BackendStatus, rx_metadata_errors) == 152);
};
