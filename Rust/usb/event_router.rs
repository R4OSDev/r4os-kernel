//! Bounded event routing. Storage is embedded in its sole xHCI owner; no
//! callback, wait, allocation or hardware access occurs in these transitions.
pub const CAPACITY: usize = 256;
pub const MAX_TRANSFER_TRB_POINTERS: usize = 32;
pub const TRANSFER_EVENT_TYPE: u8 = 32;
pub const COMMAND_COMPLETION_EVENT_TYPE: u8 = 33;
pub const PORT_STATUS_CHANGE_EVENT_TYPE: u8 = 34;
pub const EVENT_DATA_BIT: u32 = 1 << 2;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Event {
    pub event_type: u8,
    pub code: u8,
    pub slot_id: u8,
    pub endpoint_id: u8,
    pub parameter: u64,
    pub length: u32,
    pub control: u32,
}
impl Event {
    pub const EMPTY: Self = Self {
        event_type: 0,
        code: 0,
        slot_id: 0,
        endpoint_id: 0,
        parameter: 0,
        length: 0,
        control: 0,
    };
    pub fn has_event_data(self) -> bool {
        self.event_type == TRANSFER_EVENT_TYPE && self.control & EVENT_DATA_BIT != 0
    }
    pub fn trb_pointer(self) -> Option<u64> {
        if self.has_event_data()
            || !matches!(
                self.event_type,
                TRANSFER_EVENT_TYPE | COMMAND_COMPLETION_EVENT_TYPE
            )
        {
            return None;
        }
        Some(normalize_trb_pointer(self.parameter))
    }
    pub fn port_id(self) -> Option<u8> {
        (self.event_type == PORT_STATUS_CHANGE_EVENT_TYPE).then_some((self.parameter >> 24) as u8)
    }
}
#[repr(u8)]
#[derive(Clone, Copy)]
pub enum PortAction {
    Acknowledge,
    Remove,
    Enumerate,
    Replace,
}
pub fn decide_port_action(runtime: bool, connected: bool, changed: bool) -> PortAction {
    if changed && runtime {
        return if connected {
            PortAction::Replace
        } else {
            PortAction::Remove
        };
    }
    if !connected {
        return if runtime {
            PortAction::Remove
        } else {
            PortAction::Acknowledge
        };
    }
    if !runtime {
        return PortAction::Enumerate;
    }
    PortAction::Acknowledge
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct PortChangeSnapshot {
    pub pending_mask: u16,
    pub pending: u8,
    pub events: u64,
    pub queued: u64,
    pub coalesced: u64,
    pub invalid: u64,
    pub taken: u64,
    pub retries: u64,
    pub high_water: u8,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct PortChanges {
    pub pending_mask: u16,
    pub events: u64,
    pub queued: u64,
    pub coalesced: u64,
    pub invalid: u64,
    pub taken: u64,
    pub retries: u64,
    pub high_water: u8,
}
impl PortChanges {
    pub fn route(&mut self, event: Event, max_ports: u8) -> bool {
        let Some(port) = event.port_id() else {
            return false;
        };
        self.events = self.events.wrapping_add(1);
        if port == 0 || port > max_ports || port > 16 {
            self.invalid = self.invalid.wrapping_add(1);
            return true;
        }
        let bit = 1u16 << (port - 1);
        if self.pending_mask & bit != 0 {
            self.coalesced = self.coalesced.wrapping_add(1);
            return true;
        }
        self.pending_mask |= bit;
        self.queued = self.queued.wrapping_add(1);
        self.high_water = self.high_water.max(self.pending_mask.count_ones() as u8);
        true
    }
    pub fn take_next(&mut self) -> Option<u8> {
        for index in 0..16 {
            let bit = 1u16 << index;
            if self.pending_mask & bit == 0 {
                continue;
            }
            self.pending_mask &= !bit;
            self.taken = self.taken.wrapping_add(1);
            return Some(index + 1);
        }
        None
    }
    pub fn retry(&mut self, port: u8) {
        if port == 0 || port > 16 {
            return;
        }
        self.pending_mask |= 1u16 << (port - 1);
        self.retries = self.retries.wrapping_add(1);
    }
    pub fn snapshot(&self) -> PortChangeSnapshot {
        PortChangeSnapshot {
            pending_mask: self.pending_mask,
            pending: self.pending_mask.count_ones() as u8,
            events: self.events,
            queued: self.queued,
            coalesced: self.coalesced,
            invalid: self.invalid,
            taken: self.taken,
            retries: self.retries,
            high_water: self.high_water,
        }
    }
}
/// Fixed private projection of the former Zig tagged union. Command uses
/// pointer zero; transfer preserves the exact bounded normalized TD pointers.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Match {
    pub tag: u8,
    pub slot_id: u8,
    pub endpoint_id: u8,
    pub trb_count: u8,
    pub trb_phys: [u64; MAX_TRANSFER_TRB_POINTERS],
}
impl Match {
    pub const EMPTY: Self = Self {
        tag: 0,
        slot_id: 0,
        endpoint_id: 0,
        trb_count: 1,
        trb_phys: [0; MAX_TRANSFER_TRB_POINTERS],
    };
    pub fn transfer(slot: u8, endpoint: u8, pointer: u64) -> Self {
        let mut out = Self {
            slot_id: slot,
            endpoint_id: endpoint,
            ..Self::EMPTY
        };
        out.trb_phys[0] = normalize_trb_pointer(pointer);
        out
    }
    pub fn transfer_td(
        slot: u8,
        endpoint: u8,
        pointers: &[u64; MAX_TRANSFER_TRB_POINTERS],
        count: u8,
    ) -> Self {
        let count = usize::from(count).min(MAX_TRANSFER_TRB_POINTERS);
        let mut out = Self {
            slot_id: slot,
            endpoint_id: endpoint,
            trb_count: count as u8,
            ..Self::EMPTY
        };
        for (index, value) in pointers[..count].iter().enumerate() {
            out.trb_phys[index] = normalize_trb_pointer(*value);
        }
        out
    }
    pub fn command(pointer: u64) -> Self {
        let mut out = Self {
            tag: 1,
            trb_count: 0,
            ..Self::EMPTY
        };
        out.trb_phys[0] = normalize_trb_pointer(pointer);
        out
    }
}
pub fn normalize_trb_pointer(value: u64) -> u64 {
    value & !0x0f
}
pub fn matches(event: Event, expected: Match) -> bool {
    match expected.tag {
        1 => {
            event.event_type == COMMAND_COMPLETION_EVENT_TYPE
                && normalize_trb_pointer(event.parameter)
                    == normalize_trb_pointer(expected.trb_phys[0])
        }
        0 => {
            event.event_type == TRANSFER_EVENT_TYPE
                && !event.has_event_data()
                && event.slot_id == expected.slot_id
                && event.endpoint_id == expected.endpoint_id
                && expected.trb_phys
                    [..usize::from(expected.trb_count).min(MAX_TRANSFER_TRB_POINTERS)]
                    .contains(&normalize_trb_pointer(event.parameter))
        }
        _ => panic!("Invalid USB match projection"),
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Snapshot {
    pub pending: usize,
    pub queued: u64,
    pub delivered: u64,
    pub overflows: u64,
    pub purged: u64,
    pub high_water: usize,
}
#[repr(C)]
pub struct Mailbox {
    pub events: [Event; CAPACITY],
    pub count: usize,
    pub queued: u64,
    pub delivered: u64,
    pub overflows: u64,
    pub purged: u64,
    pub high_water: usize,
}
impl Mailbox {
    pub const EMPTY: Self = Self {
        events: [Event::EMPTY; CAPACITY],
        count: 0,
        queued: 0,
        delivered: 0,
        overflows: 0,
        purged: 0,
        high_water: 0,
    };
    pub fn enqueue(&mut self, event: Event) -> bool {
        if self.count >= CAPACITY {
            self.overflows += 1;
            return false;
        }
        self.events[self.count] = event;
        self.count += 1;
        self.queued += 1;
        self.high_water = self.high_water.max(self.count);
        true
    }
    fn remove_at(&mut self, index: usize) -> Event {
        let removed = self.events[index];
        self.events.copy_within(index + 1..self.count, index);
        self.count -= 1;
        self.events[self.count] = Event::EMPTY;
        removed
    }
    pub fn take(&mut self, expected: Match) -> Option<Event> {
        let index = self.events[..self.count]
            .iter()
            .position(|event| matches(*event, expected))?;
        let event = self.remove_at(index);
        self.delivered += 1;
        Some(event)
    }
    pub fn purge(&mut self, expected: Match) -> usize {
        self.purge_where(|event| matches(event, expected))
    }
    pub fn purge_slot(&mut self, slot: u8) -> usize {
        self.purge_where(|event| event.slot_id == slot)
    }
    fn purge_where(&mut self, predicate: impl Fn(Event) -> bool) -> usize {
        let (mut removed, mut index) = (0, 0);
        while index < self.count {
            if !predicate(self.events[index]) {
                index += 1;
                continue;
            }
            self.remove_at(index);
            removed += 1;
        }
        self.purged += removed as u64;
        removed
    }
    pub fn clear(&mut self) -> usize {
        let removed = self.count;
        self.events[..self.count].fill(Event::EMPTY);
        self.count = 0;
        self.purged += removed as u64;
        removed
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            pending: self.count,
            queued: self.queued,
            delivered: self.delivered,
            overflows: self.overflows,
            purged: self.purged,
            high_water: self.high_water,
        }
    }
}
pub fn self_test() -> bool {
    let mut mailbox = Mailbox::EMPTY;
    let hid = Event {
        event_type: 32,
        code: 1,
        slot_id: 2,
        endpoint_id: 3,
        parameter: 0x2000,
        control: 1,
        ..Event::EMPTY
    };
    let storage = Event {
        slot_id: 4,
        endpoint_id: 5,
        parameter: 0x4000,
        ..hid
    };
    let command = Event {
        event_type: 33,
        code: 1,
        slot_id: 4,
        parameter: 0x6000,
        control: 1,
        ..Event::EMPTY
    };
    if !mailbox.enqueue(hid) || !mailbox.enqueue(storage) || !mailbox.enqueue(command) {
        return false;
    }
    if mailbox
        .take(Match::transfer(4, 5, 0x4000))
        .is_none_or(|event| event.parameter != storage.parameter)
    {
        return false;
    }
    if mailbox
        .take(Match::command(0x6000))
        .is_none_or(|event| event.parameter != command.parameter)
    {
        return false;
    }
    mailbox
        .take(Match::transfer(2, 3, 0x2000))
        .is_some_and(|event| event.parameter == hid.parameter)
        && mailbox.count == 0
}
// C entry points borrow only aligned caller-owned fixed records for this
// bounded operation. All outputs are exclusive; no reference escapes.

/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_event_data(event: *const Event) -> bool {
    unsafe { (*event).has_event_data() }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_event_pointer(event: *const Event, out: *mut u64) -> bool {
    if let Some(value) = unsafe { (*event).trb_pointer() } {
        unsafe { out.write(value) };
        true
    } else {
        false
    }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_event_port(event: *const Event) -> u16 {
    unsafe { (*event).port_id() }.map_or(256, u16::from)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_usb_port_action(runtime: bool, connected: bool, changed: bool) -> u8 {
    decide_port_action(runtime, connected, changed) as u8
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_ports_reset(ports: *mut PortChanges) {
    unsafe { ports.write(PortChanges::default()) }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_ports_route(
    ports: *mut PortChanges,
    event: *const Event,
    max: u8,
) -> bool {
    unsafe { (&mut *ports).route(*event, max) }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_ports_take(ports: *mut PortChanges) -> u8 {
    unsafe { (&mut *ports).take_next() }.unwrap_or(0)
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_ports_retry(ports: *mut PortChanges, port: u8) {
    unsafe { (&mut *ports).retry(port) }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_ports_snapshot(
    ports: *const PortChanges,
    out: *mut PortChangeSnapshot,
) {
    unsafe { out.write((*ports).snapshot()) }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_match_transfer(
    slot: u8,
    endpoint: u8,
    pointer: u64,
    out: *mut Match,
) {
    unsafe { out.write(Match::transfer(slot, endpoint, pointer)) }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_match_td(
    slot: u8,
    endpoint: u8,
    pointers: *const [u64; 32],
    count: u8,
    out: *mut Match,
) {
    unsafe { out.write(Match::transfer_td(slot, endpoint, &*pointers, count)) }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_match_command(pointer: u64, out: *mut Match) {
    unsafe { out.write(Match::command(pointer)) }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_event_matches(
    event: *const Event,
    expected: *const Match,
) -> bool {
    unsafe { matches(*event, *expected) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_usb_normalize_pointer(value: u64) -> u64 {
    normalize_trb_pointer(value)
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_mailbox_reset(mailbox: *mut Mailbox) {
    unsafe { mailbox.write(Mailbox::EMPTY) }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_mailbox_enqueue(
    mailbox: *mut Mailbox,
    event: *const Event,
) -> bool {
    unsafe { (&mut *mailbox).enqueue(*event) }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_mailbox_take(
    mailbox: *mut Mailbox,
    expected: *const Match,
    out: *mut Event,
) -> bool {
    if let Some(event) = unsafe { (&mut *mailbox).take(*expected) } {
        unsafe { out.write(event) };
        true
    } else {
        false
    }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_mailbox_purge(
    mailbox: *mut Mailbox,
    expected: *const Match,
) -> usize {
    unsafe { (&mut *mailbox).purge(*expected) }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_mailbox_purge_slot(
    mailbox: *mut Mailbox,
    slot: u8,
) -> usize {
    unsafe { (&mut *mailbox).purge_slot(slot) }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_mailbox_clear(mailbox: *mut Mailbox) -> usize {
    unsafe { (&mut *mailbox).clear() }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_mailbox_count(mailbox: *const Mailbox) -> usize {
    unsafe { (*mailbox).count }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_mailbox_snapshot(
    mailbox: *const Mailbox,
    out: *mut Snapshot,
) {
    unsafe { out.write((*mailbox).snapshot()) }
}
const _: () = {
    assert!(core::mem::size_of::<Event>() == 24);
    assert!(core::mem::offset_of!(Event, parameter) == 8);
    assert!(core::mem::size_of::<Match>() == 264);
    assert!(core::mem::offset_of!(Match, trb_phys) == 8);
    assert!(core::mem::size_of::<PortChanges>() == 64);
    assert!(core::mem::offset_of!(PortChanges, high_water) == 56);
    assert!(core::mem::size_of::<PortChangeSnapshot>() == 64);
    assert!(core::mem::offset_of!(PortChangeSnapshot, pending) == 2);
    assert!(core::mem::size_of::<Mailbox>() == 6192);
    assert!(core::mem::offset_of!(Mailbox, count) == 6144);
    assert!(core::mem::size_of::<Snapshot>() == 48);
};
