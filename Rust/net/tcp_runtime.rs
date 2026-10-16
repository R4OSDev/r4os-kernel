//! Existing retained send catalog and exact connection-generation RTO state.
//! Catalogs are always mutated in place; a full catalog exceeds a task stack.
pub const CONNECTION_CAPACITY: usize = 16;
pub const SENT_SEGMENT_CAPACITY: usize = 48;
pub const SENT_PAYLOAD_CAPACITY: usize = 1460;
pub const FLAG_FIN: u16 = 1;
pub const FLAG_SYN: u16 = 2;
#[repr(u8)]
#[derive(Clone, Copy)]
pub enum SegmentKind {
    Any,
    Data,
    Syn,
    Fin,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ConnectionIdentity {
    pub connection_id: u32,
    pub slot: usize,
    pub generation: u32,
}
impl ConnectionIdentity {
    pub fn eql(self, other: Self) -> bool {
        self.connection_id == other.connection_id
            && self.slot == other.slot
            && self.generation == other.generation
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SentSegment {
    pub valid: bool,
    pub token: u64,
    pub seq: u32,
    pub ack: u32,
    pub flags: u16,
    pub payload_offset: u16,
    pub payload_len: u16,
    pub sent_tick: u64,
    pub retransmits: u8,
    pub payload: [u8; SENT_PAYLOAD_CAPACITY],
}
impl SentSegment {
    pub const EMPTY: Self = Self {
        valid: false,
        token: 0,
        seq: 0,
        ack: 0,
        flags: 0,
        payload_offset: 0,
        payload_len: 0,
        sent_tick: 0,
        retransmits: 0,
        payload: [0; SENT_PAYLOAD_CAPACITY],
    };
    pub fn payload_slice(&self) -> &[u8] {
        let start = usize::from(self.payload_offset);
        &self.payload[start..start + usize::from(self.payload_len)]
    }
    pub fn sequence_length(&self) -> u32 {
        sequence_length_for(self.flags, usize::from(self.payload_len))
    }
    fn matches(&self, kind: SegmentKind) -> bool {
        self.valid
            && match kind {
                SegmentKind::Any => true,
                SegmentKind::Data => self.payload_len != 0,
                SegmentKind::Syn => self.flags & FLAG_SYN != 0,
                SegmentKind::Fin => self.flags & FLAG_FIN != 0,
            }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct AckResult {
    pub progressed: bool,
    pub fully_acked: u8,
    pub remaining: u8,
}
#[repr(C)]
pub struct SendCatalog {
    pub segments: [SentSegment; SENT_SEGMENT_CAPACITY],
    pub next_token: u64,
}
impl SendCatalog {
    pub const EMPTY: Self = Self {
        segments: [SentSegment::EMPTY; SENT_SEGMENT_CAPACITY],
        next_token: 1,
    };
    pub fn reset(&mut self) {
        for segment in &mut self.segments {
            segment.valid = false;
        }
        self.next_token = 1;
    }
    pub fn has_capacity(&self) -> bool {
        self.segments.iter().any(|s| !s.valid)
    }
    pub fn count(&self) -> usize {
        self.segments.iter().filter(|s| s.valid).count()
    }
    pub fn free_count(&self) -> usize {
        SENT_SEGMENT_CAPACITY - self.count()
    }
    pub fn outstanding_bytes(&self) -> usize {
        self.segments
            .iter()
            .filter(|s| s.valid)
            .map(|s| usize::from(s.payload_len))
            .sum()
    }
    pub fn track(
        &mut self,
        seq: u32,
        ack: u32,
        flags: u16,
        payload: &[u8],
        tick: u64,
    ) -> Option<u64> {
        if !needs_tracking(flags, payload.len()) || payload.len() > SENT_PAYLOAD_CAPACITY {
            return None;
        }
        for segment in &mut self.segments {
            if segment.valid {
                continue;
            }
            let mut token = self.next_token;
            self.next_token = self.next_token.wrapping_add(1);
            if self.next_token == 0 {
                self.next_token = 1;
            }
            if token == 0 {
                token = 1;
            }
            // Keep prior tail bytes unobservable behind exact length/validity.
            segment.valid = true;
            segment.token = token;
            segment.seq = seq;
            segment.ack = ack;
            segment.flags = flags;
            segment.payload_offset = 0;
            segment.payload_len = payload.len() as u16;
            segment.sent_tick = tick;
            segment.retransmits = 0;
            segment.payload[..payload.len()].copy_from_slice(payload);
            return Some(token);
        }
        None
    }
    pub fn acknowledge(&mut self, ack: u32) -> AckResult {
        let mut result = AckResult::default();
        for segment in &mut self.segments {
            if !segment.valid || !seq_after(ack, segment.seq) {
                continue;
            }
            let distance = ack.wrapping_sub(segment.seq);
            let total = segment.sequence_length();
            if distance >= total {
                segment.valid = false;
                result.progressed = true;
                result.fully_acked = result.fully_acked.saturating_add(1);
                continue;
            }
            let mut acknowledged = distance;
            if segment.flags & FLAG_SYN != 0 && acknowledged != 0 {
                segment.flags &= !FLAG_SYN;
                segment.seq = segment.seq.wrapping_add(1);
                acknowledged -= 1;
                result.progressed = true;
            }
            if acknowledged != 0 && segment.payload_len != 0 {
                let trim = acknowledged.min(u32::from(segment.payload_len)) as u16;
                segment.payload_offset += trim;
                segment.payload_len -= trim;
                segment.seq = segment.seq.wrapping_add(u32::from(trim));
                acknowledged -= u32::from(trim);
                result.progressed = true;
            }
            if segment.flags & FLAG_FIN != 0 && acknowledged != 0 {
                segment.valid = false;
                result.progressed = true;
                result.fully_acked = result.fully_acked.saturating_add(1);
            }
        }
        result.remaining = u8::try_from(self.count()).expect("TCP retained catalog count");
        result
    }
    pub fn oldest(&self, kind: SegmentKind) -> Option<&SentSegment> {
        let mut result: Option<&SentSegment> = None;
        for segment in &self.segments {
            if !segment.matches(kind) {
                continue;
            }
            if result.is_none_or(|old| segment.token < old.token) {
                result = Some(segment);
            }
        }
        result
    }
    pub fn mark_retransmitted(&mut self, token: u64, tick: u64) -> bool {
        for segment in &mut self.segments {
            if !segment.valid || segment.token != token {
                continue;
            }
            segment.sent_tick = tick;
            segment.retransmits = segment.retransmits.saturating_add(1);
            return true;
        }
        false
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RtoState {
    pub bound: bool,
    pub connection_id: u32,
    pub generation: u32,
    pub srtt_ticks: u64,
    pub sent_tick: u64,
    pub sent_tx_ack: u32,
}
impl RtoState {
    pub const EMPTY: Self = Self {
        bound: false,
        connection_id: 0,
        generation: 0,
        srtt_ticks: 0,
        sent_tick: 0,
        sent_tx_ack: 0,
    };
}
#[repr(C)]
pub struct RtoTable {
    pub slots: [RtoState; CONNECTION_CAPACITY],
}
impl RtoTable {
    pub const EMPTY: Self = Self {
        slots: [RtoState::EMPTY; CONNECTION_CAPACITY],
    };
    pub fn reset(&mut self) {
        *self = Self::EMPTY;
    }
    pub fn bind(&mut self, identity: ConnectionIdentity) -> Option<&mut RtoState> {
        if identity.connection_id == 0
            || identity.generation == 0
            || identity.slot >= self.slots.len()
        {
            return None;
        }
        let state = &mut self.slots[identity.slot];
        if !state.bound
            || state.connection_id != identity.connection_id
            || state.generation != identity.generation
        {
            *state = RtoState {
                bound: true,
                connection_id: identity.connection_id,
                generation: identity.generation,
                ..RtoState::EMPTY
            };
        }
        Some(state)
    }
    pub fn get(&mut self, identity: ConnectionIdentity) -> Option<&mut RtoState> {
        if identity.slot >= self.slots.len() {
            return None;
        }
        let state = &mut self.slots[identity.slot];
        if !state.bound
            || state.connection_id != identity.connection_id
            || state.generation != identity.generation
        {
            return None;
        }
        Some(state)
    }
    pub fn release(&mut self, identity: ConnectionIdentity) {
        if let Some(state) = self.get(identity) {
            *state = RtoState::EMPTY;
        }
    }
    pub fn release_slot(&mut self, slot: usize) {
        if slot < self.slots.len() {
            self.slots[slot] = RtoState::EMPTY;
        }
    }
}
pub fn needs_tracking(flags: u16, len: usize) -> bool {
    len != 0 || flags & (FLAG_SYN | FLAG_FIN) != 0
}
pub fn send_allowance(window: u32, requested: usize, capacity: bool) -> usize {
    if !capacity || window == 0 || requested == 0 {
        0
    } else {
        requested.min(window as usize)
    }
}
pub fn sequence_length_for(flags: u16, len: usize) -> u32 {
    let mut result = u32::try_from(len).expect("TCP payload sequence length");
    if flags & FLAG_SYN != 0 {
        result = result.wrapping_add(1);
    }
    if flags & FLAG_FIN != 0 {
        result = result.wrapping_add(1);
    }
    result
}
pub fn seq_after(a: u32, b: u32) -> bool {
    a.wrapping_sub(b) as i32 > 0
}
pub fn deadline_reached(now: u64, deadline: u64) -> bool {
    now.wrapping_sub(deadline) as i64 >= 0
}
fn kind(raw: u8) -> SegmentKind {
    match raw {
        0 => SegmentKind::Any,
        1 => SegmentKind::Data,
        2 => SegmentKind::Syn,
        3 => SegmentKind::Fin,
        _ => panic!("Typed TCP segment kind"),
    }
}
/// # Safety
/// Borrowed fixed identity records remain resident for this comparison.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_identity_eq(
    a: *const ConnectionIdentity,
    b: *const ConnectionIdentity,
) -> bool {
    unsafe { (*a).eql(*b) }
}
/// # Safety
/// Exact retained segment and exclusive output length. Caller retains catalog
/// admission/lifetime while using the returned payload address.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_segment_payload(
    s: *const SentSegment,
    len: *mut usize,
) -> *const u8 {
    let bytes = unsafe { (&*s).payload_slice() };
    unsafe {
        len.write(bytes.len());
    }
    bytes.as_ptr()
}
/// # Safety
/// Exact retained resident segment.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_segment_sequence(s: *const SentSegment) -> u32 {
    unsafe { (&*s).sequence_length() }
}
/// # Safety
/// Exclusive catalog admission; reset preserves payload storage in place.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_catalog_reset(c: *mut SendCatalog) {
    unsafe {
        (&mut *c).reset();
    }
}
/// # Safety
/// Borrowed resident catalog under its existing owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_catalog_capacity(c: *const SendCatalog) -> bool {
    unsafe { (&*c).has_capacity() }
}
/// # Safety
/// Borrowed resident catalog under its existing owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_catalog_count(c: *const SendCatalog) -> usize {
    unsafe { (&*c).count() }
}
/// # Safety
/// Borrowed resident catalog under its existing owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_catalog_free(c: *const SendCatalog) -> usize {
    unsafe { (&*c).free_count() }
}
/// # Safety
/// Borrowed resident catalog under its existing owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_catalog_bytes(c: *const SendCatalog) -> usize {
    unsafe { (&*c).outstanding_bytes() }
}
/// # Safety
/// Exclusive catalog admission and readable retained input payload.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_catalog_track(
    c: *mut SendCatalog,
    seq: u32,
    ack: u32,
    flags: u16,
    bytes: *const u8,
    len: usize,
    tick: u64,
) -> u64 {
    if len > SENT_PAYLOAD_CAPACITY {
        return 0;
    }
    let data = if len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(bytes, len) }
    };
    unsafe { (&mut *c).track(seq, ack, flags, data, tick).unwrap_or(0) }
}
/// # Safety
/// Exclusive catalog admission and nonaliasing fixed result output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_catalog_ack(
    c: *mut SendCatalog,
    ack: u32,
    out: *mut AckResult,
) {
    unsafe {
        out.write((&mut *c).acknowledge(ack));
    }
}
/// # Safety
/// Caller retains catalog ownership while using the exact returned segment.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_catalog_oldest(
    c: *const SendCatalog,
    k: u8,
) -> *const SentSegment {
    unsafe { (&*c).oldest(kind(k)).map_or(core::ptr::null(), |s| s) }
}
/// # Safety
/// Exclusive catalog admission and exact retained segment token.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_catalog_retransmit(
    c: *mut SendCatalog,
    token: u64,
    tick: u64,
) -> bool {
    unsafe { (&mut *c).mark_retransmitted(token, tick) }
}
/// # Safety
/// Exclusive stable RTO table.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_rto_reset(t: *mut RtoTable) {
    unsafe {
        (&mut *t).reset();
    }
}
/// # Safety
/// Exclusive table and exact identity. Caller keeps admission for the returned
/// slot; its pointer is not a separate owner or generation-independent handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_rto_bind(
    t: *mut RtoTable,
    id: *const ConnectionIdentity,
) -> *mut RtoState {
    unsafe { (&mut *t).bind(*id).map_or(core::ptr::null_mut(), |s| s) }
}
/// # Safety
/// Exclusive table and exact identity; returned slot retains caller admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_rto_get(
    t: *mut RtoTable,
    id: *const ConnectionIdentity,
) -> *mut RtoState {
    unsafe { (&mut *t).get(*id).map_or(core::ptr::null_mut(), |s| s) }
}
/// # Safety
/// Exclusive table and exact retained identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_rto_release(
    t: *mut RtoTable,
    id: *const ConnectionIdentity,
) {
    unsafe {
        (&mut *t).release(*id);
    }
}
/// # Safety
/// Exclusive table and original slot boundary.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_rto_release_slot(t: *mut RtoTable, slot: usize) {
    unsafe {
        (&mut *t).release_slot(slot);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_tcp_tracking(flags: u16, len: usize) -> bool {
    needs_tracking(flags, len)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_tcp_allowance(
    window: u32,
    requested: usize,
    capacity: bool,
) -> usize {
    send_allowance(window, requested, capacity)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_tcp_sequence(flags: u16, len: usize) -> u32 {
    sequence_length_for(flags, len)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_tcp_seq_after(a: u32, b: u32) -> bool {
    seq_after(a, b)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_tcp_deadline(now: u64, deadline: u64) -> bool {
    deadline_reached(now, deadline)
}

const _: () = {
    assert!(core::mem::size_of::<ConnectionIdentity>() == 24);
    assert!(core::mem::align_of::<ConnectionIdentity>() == 8);
    assert!(core::mem::offset_of!(ConnectionIdentity, connection_id) == 0);
    assert!(core::mem::offset_of!(ConnectionIdentity, slot) == 8);
    assert!(core::mem::offset_of!(ConnectionIdentity, generation) == 16);
};
const _: () = {
    assert!(core::mem::size_of::<SentSegment>() == 1504);
    assert!(core::mem::align_of::<SentSegment>() == 8);
    assert!(core::mem::offset_of!(SentSegment, valid) == 0);
    assert!(core::mem::offset_of!(SentSegment, token) == 8);
    assert!(core::mem::offset_of!(SentSegment, seq) == 16);
    assert!(core::mem::offset_of!(SentSegment, ack) == 20);
    assert!(core::mem::offset_of!(SentSegment, flags) == 24);
    assert!(core::mem::offset_of!(SentSegment, payload_offset) == 26);
    assert!(core::mem::offset_of!(SentSegment, payload_len) == 28);
    assert!(core::mem::offset_of!(SentSegment, sent_tick) == 32);
    assert!(core::mem::offset_of!(SentSegment, retransmits) == 40);
    assert!(core::mem::offset_of!(SentSegment, payload) == 41);
};
const _: () = {
    assert!(core::mem::size_of::<AckResult>() == 3);
    assert!(core::mem::align_of::<AckResult>() == 1);
    assert!(core::mem::offset_of!(AckResult, progressed) == 0);
    assert!(core::mem::offset_of!(AckResult, fully_acked) == 1);
    assert!(core::mem::offset_of!(AckResult, remaining) == 2);
};
const _: () = {
    assert!(core::mem::size_of::<SendCatalog>() == 72200);
    assert!(core::mem::align_of::<SendCatalog>() == 8);
    assert!(core::mem::offset_of!(SendCatalog, segments) == 0);
    assert!(core::mem::offset_of!(SendCatalog, next_token) == 72192);
};
const _: () = {
    assert!(core::mem::size_of::<RtoState>() == 40);
    assert!(core::mem::align_of::<RtoState>() == 8);
    assert!(core::mem::offset_of!(RtoState, bound) == 0);
    assert!(core::mem::offset_of!(RtoState, connection_id) == 4);
    assert!(core::mem::offset_of!(RtoState, generation) == 8);
    assert!(core::mem::offset_of!(RtoState, srtt_ticks) == 16);
    assert!(core::mem::offset_of!(RtoState, sent_tick) == 24);
    assert!(core::mem::offset_of!(RtoState, sent_tx_ack) == 32);
};
const _: () = {
    assert!(core::mem::size_of::<RtoTable>() == 640);
    assert!(core::mem::align_of::<RtoTable>() == 8);
    assert!(core::mem::offset_of!(RtoTable, slots) == 0);
};
