//! Original R4D network registry and registration-ID callback admission.
//! The existing Net core remains the sole topology/packet execution owner.
use crate::{
    boot::log,
    contract as a,
    driver::{api_admission as admission, api_validation as validation},
};
use core::{
    cell::UnsafeCell,
    mem::{MaybeUninit, size_of},
    ptr,
};
#[derive(Clone, Copy)]
struct Record {
    used: bool,
    owner: u32,
    registration: u32,
    name: [u8; 32],
    name_len: usize,
    descriptor: *const a::NetBackend,
    negotiation: a::NetBackendNegotiation,
}
impl Record {
    fn empty() -> Self {
        Self {
            used: false,
            owner: 0,
            registration: 0,
            name: [0; 32],
            name_len: 0,
            descriptor: ptr::null(),
            negotiation: a::NetBackendNegotiation::default(),
        }
    }
}
struct Storage(UnsafeCell<[MaybeUninit<Record>; 8]>);
// SAFETY: bootstrap initializes once; original Net topology admission protects
// mutations and callbacks. No mutable registry borrow spans foreign code.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new([const { MaybeUninit::uninit() }; 8]));
static INITIALIZED: Initialized = Initialized(UnsafeCell::new(false));
struct Initialized(UnsafeCell<bool>);
unsafe impl Sync for Initialized {}
fn raw() -> *mut Record {
    let p = STATE.0.get().cast::<Record>();
    if !unsafe { *INITIALIZED.0.get() } {
        for i in 0..8 {
            unsafe { ptr::write(p.add(i), Record::empty()) };
        }
        unsafe { *INITIALIZED.0.get() = true };
    }
    p
}
fn record(i: usize) -> Record {
    unsafe { ptr::read(raw().add(i)) }
}
fn find_registration(id: i32) -> Option<Record> {
    if id <= 0 {
        return None;
    }
    (0..8).find_map(|i| {
        let r = record(i);
        (r.used && r.registration == id as u32).then_some(r)
    })
}
fn find_adapter(index: usize) -> Option<Record> {
    let id = unsafe { r4os_zig_net_lookup(0, index) };
    if id < 0 {
        None
    } else {
        find_registration(i32::try_from(id).expect("Original non-reused Net registration"))
    }
}
#[repr(C)]
#[derive(Default)]
struct Offer {
    offered: u64,
    required: u64,
    rx_queues: u16,
    tx_queues: u16,
    rx_segments: u16,
    tx_segments: u16,
    rx_owner: u32,
    tx_owner: u32,
    moderation: u32,
}
unsafe extern "C" {
    fn r4os_zig_net_mutation(op: u8) -> bool;
    fn r4os_zig_net_callback(enter: bool) -> bool;
    fn r4os_zig_net_negotiate(offer: *const Offer, out: *mut a::NetBackendNegotiation) -> i32;
    fn r4os_zig_net_register(
        name: *const u8,
        len: usize,
        d: *const a::NetBackend,
        selected: *const a::NetBackendNegotiation,
    ) -> isize;
    fn r4os_zig_net_lookup(op: u8, value: usize) -> isize;
    fn r4os_zig_net_unregister(index: usize) -> bool;
    fn r4os_zig_net_receive(
        index: usize,
        packet: bool,
        queue: u16,
        flags: u64,
        frame: *const u8,
        len: u32,
    ) -> u8;
    fn r4os_zig_net_schedule(index: usize) -> bool;
    fn r4os_zig_net_packet_valid(
        packet: *const a::NetPacket,
        selected: *const a::NetBackendNegotiation,
    ) -> bool;
    fn r4os_zig_net_registered_log(
        name: *const u8,
        len: usize,
        index: usize,
        selected: *const a::NetBackendNegotiation,
    );
}
struct Mutation;
impl Drop for Mutation {
    fn drop(&mut self) {
        unsafe { r4os_zig_net_mutation(1) };
    }
}
struct Callback;
impl Callback {
    fn enter() -> Option<Self> {
        unsafe { r4os_zig_net_callback(true) }.then(|| Self)
    }
}
impl Drop for Callback {
    fn drop(&mut self) {
        unsafe { r4os_zig_net_callback(false) };
    }
}
fn puts(s: &[u8]) {
    unsafe { log::r4os_rust_bootlog_write(s.as_ptr(), s.len()) }
}
unsafe fn offer(d: *const a::NetBackend) -> Offer {
    if unsafe { (*d).version } < 2 || unsafe { (*d).size } < size_of::<a::NetBackend>() as u32 {
        return Offer {
            rx_queues: 1,
            tx_queues: 1,
            rx_segments: 1,
            tx_segments: 1,
            ..Offer::default()
        };
    }
    unsafe {
        Offer {
            offered: (*d).offered_capabilities,
            required: (*d).required_capabilities,
            rx_queues: (*d).rx_queue_count,
            tx_queues: (*d).tx_queue_count,
            rx_segments: (*d).max_rx_segments,
            tx_segments: (*d).max_tx_segments,
            rx_owner: (*d).rx_ownership,
            tx_owner: (*d).tx_ownership,
            moderation: (*d).interrupt_moderation_us,
        }
    }
}
/// # Safety
/// Readable resident descriptor keeps its original declared C prefix and all
/// callbacks through exact Net topology unregister or failed retained cleanup.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_net_register(
    name: *const u8,
    d: *const a::NetBackend,
) -> i32 {
    if !unsafe { validation::r4os_rust_driver_net_valid(d) } {
        return -1;
    }
    let mut selected = MaybeUninit::uninit();
    let code = unsafe { r4os_zig_net_negotiate(&offer(d), selected.as_mut_ptr()) };
    if code != 0 {
        puts(b"[R4D][ERROR] net backend capability negotiation failed: ");
        puts(match code {
            1 => b"invalid-offer",
            2 => b"required-not-offered",
            3 => b"required-unsupported",
            _ => unreachable!(),
        });
        puts(b"\r\n");
        return -4;
    }
    let selected = unsafe { selected.assume_init() };
    if !unsafe { r4os_zig_net_mutation(0) } {
        return -5;
    }
    let _mutation = Mutation;
    let Some(i) = (0..8).find(|&i| !record(i).used) else {
        puts(b"[R4D][ERROR] net backend table full\r\n");
        return -2;
    };
    let mut r = Record::empty();
    r.used = true;
    r.owner = admission::r4os_rust_driver_api_owner_scalar(0);
    r.descriptor = d;
    r.negotiation = selected;
    while r.name_len < 31 && unsafe { *name.add(r.name_len) } != 0 {
        r.name[r.name_len] = unsafe { *name.add(r.name_len) };
        r.name_len += 1;
    }
    let p = unsafe { raw().add(i) };
    unsafe { ptr::write(p, r) };
    let index =
        unsafe { r4os_zig_net_register(ptr::addr_of!((*p).name).cast(), r.name_len, d, &selected) };
    if index < 0 {
        unsafe { ptr::write(p, Record::empty()) };
        puts(b"[R4D][ERROR] net core adapter table full\r\n");
        return -3;
    }
    let registration = unsafe { r4os_zig_net_lookup(0, index as usize) };
    assert!(registration > 0);
    unsafe { (*p).registration = registration as u32 };
    unsafe { r4os_zig_net_registered_log(r.name.as_ptr(), r.name_len, index as usize, &selected) };
    i32::try_from(registration).expect("Original bounded Net registration")
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_net_has_owner(owner: u32) -> bool {
    (0..8).any(|i| {
        let r = record(i);
        r.used && r.owner == owner
    })
}
#[repr(C)]
pub struct Cleanup {
    pub removed: u32,
    pub failed: bool,
}
/// # Safety
/// Output is a resident private C record. The prepared original topology
/// mutation remains active until real backend shutdown and unregister finish.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_net_cleanup(owner: u32, out: *mut Cleanup) {
    let mut result = Cleanup {
        removed: 0,
        failed: false,
    };
    for i in 0..8 {
        let r = record(i);
        if !r.used || r.owner != owner {
            continue;
        }
        if let Some(shutdown) = unsafe { (*r.descriptor).shutdown }
            && unsafe { shutdown((*r.descriptor).context) } != 0
        {
            result.failed = true;
            continue;
        }
        let index = unsafe { r4os_zig_net_lookup(1, r.registration as usize) };
        if index < 0 || !unsafe { r4os_zig_net_unregister(index as usize) } {
            result.failed = true;
            continue;
        }
        unsafe { ptr::write(raw().add(i), Record::empty()) };
        result.removed += 1;
    }
    unsafe { ptr::write(out, result) };
}
/// # Safety
/// Frame and retained descriptor stay readable through the original admitted
/// Net callback. Result is the exact existing TxResult representation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_net_transmit(
    index: usize,
    frame: *const u8,
    len: u32,
) -> u8 {
    let Some(r) = find_adapter(index) else {
        return 6;
    };
    let Some(tx) = (unsafe { (*r.descriptor).transmit }) else {
        return 5;
    };
    match unsafe { tx((*r.descriptor).context, frame, len) } {
        0 => 0,
        1 => 3,
        2 => 4,
        3 => 2,
        4 => 5,
        _ => 6,
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_net_poll(index: usize) {
    if let Some(r) = find_adapter(index)
        && let Some(poll) = unsafe { (*r.descriptor).poll }
    {
        unsafe { poll((*r.descriptor).context) };
    }
}
/// # Safety
/// Resident output and retained descriptor keep their original callback layout.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_net_status(
    index: usize,
    out: *mut a::NetBackendStatus,
) -> i32 {
    let Some(r) = find_adapter(index) else {
        return -1;
    };
    let Some(status) = (unsafe { (*r.descriptor).status }) else {
        return -2;
    };
    unsafe { status((*r.descriptor).context, out) }
}
fn receive_result(result: u8, packet: bool) -> i32 {
    match result {
        0 => 0,
        1 => i32::from(packet),
        2 => -1,
        3 => -2,
        4 => -3,
        5 => -4,
        6 => -5,
        _ => unreachable!(),
    }
}
/// # Safety
/// The flat resident frame remains readable through the original Net callback;
/// the core owns receive admission and canonical byte preservation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_net_receive(id: i32, frame: *const u8, len: u32) -> i32 {
    let Some(_callback) = Callback::enter() else {
        return -3;
    };
    if id < 0 {
        return -1;
    }
    if len == 0 || len > 1536 {
        return -2;
    }
    let Some(r) = find_registration(id) else {
        return -1;
    };
    let index = unsafe { r4os_zig_net_lookup(1, r.registration as usize) };
    if index < 0 {
        return -1;
    }
    receive_result(
        unsafe { r4os_zig_net_receive(index as usize, false, 0, 0, frame, len) },
        false,
    )
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_net_schedule(id: i32) -> i32 {
    let Some(_callback) = Callback::enter() else {
        return -3;
    };
    if id < 0 {
        return -1;
    }
    let Some(r) = find_registration(id) else {
        return -1;
    };
    let index = unsafe { r4os_zig_net_lookup(1, r.registration as usize) };
    if index < 0 {
        return -1;
    }
    if index >= unsafe { r4os_zig_net_lookup(2, 0) } {
        return -1;
    }
    if unsafe { r4os_zig_net_schedule(index as usize) } {
        0
    } else {
        -3
    }
}
/// # Safety
/// The versioned resident output is exclusive and keeps its declared prefix.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_net_query(
    id: i32,
    out: *mut a::NetBackendNegotiation,
) -> i32 {
    let Some(_callback) = Callback::enter() else {
        return -3;
    };
    if id < 0 {
        return -1;
    }
    if unsafe { (*out).version } != a::net_backend_negotiation_version
        || unsafe { (*out).size } < size_of::<a::NetBackendNegotiation>() as u32
    {
        return -2;
    }
    let Some(r) = find_registration(id) else {
        return -1;
    };
    unsafe { ptr::write(out, r.negotiation) };
    0
}
/// # Safety
/// Resident packet and canonical fallback bytes survive Net receive admission;
/// optional scatter/offload fields retain their original negotiated semantics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_net_receive_packet(
    id: i32,
    packet: *const a::NetPacket,
) -> i32 {
    let Some(_callback) = Callback::enter() else {
        return -3;
    };
    if id < 0 {
        return -1;
    }
    let Some(r) = find_registration(id) else {
        return -1;
    };
    let index = unsafe { r4os_zig_net_lookup(1, r.registration as usize) };
    if index < 0 {
        return -1;
    }
    if !unsafe { r4os_zig_net_packet_valid(packet, &r.negotiation) } {
        return -2;
    }
    let value = unsafe { ptr::read(packet) };
    receive_result(
        unsafe {
            r4os_zig_net_receive(
                index as usize,
                true,
                value.queue_index,
                value.flags,
                value.fallback_addr as *const u8,
                value.fallback_bytes,
            )
        },
        true,
    )
}
