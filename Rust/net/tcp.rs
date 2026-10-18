//! Original bounded TCP state machine. Its sole resident tables are admitted
//! by its task-bound owner; payload/catalog arrays never exceed bounded plans.
use super::tcp_runtime::{self, ConnectionIdentity, SegmentKind as RetransmitKind};
use crate::{
    contract::{TcpConnectionInfo as ConnectionInfo, TcpSummary as Summary},
    sched::sync_types::Text,
};
use core::{cell::UnsafeCell, slice};
pub const MAX_CONNECTIONS: usize = tcp_runtime::CONNECTION_CAPACITY;
pub const MAX_LISTENERS: usize = 4;
pub const BUFFER_SIZE: usize = 256 * 1024;
pub const MAX_ADVERTISED_WINDOW: u16 = 0xffff;
pub const LOCAL_MSS: u16 = 1460;
pub const DEFAULT_PEER_MSS: u16 = 536;
pub const LOCAL_WINDOW_SCALE: u8 = 2;
pub const MAX_WINDOW_SCALE: u8 = 14;
pub const OUT_OF_ORDER_SLOT_COUNT: usize = 48;
pub const OUT_OF_ORDER_SLOT_SIZE: usize = 2048;
pub const SENT_SEGMENT_CAPACITY: usize = tcp_runtime::SENT_SEGMENT_CAPACITY;
pub const SENT_PAYLOAD_CAPACITY: usize = tcp_runtime::SENT_PAYLOAD_CAPACITY;
pub const TIME_WAIT_TICKS: u64 = super::timing::ms_to_ticks(3000);
pub const FLAG_FIN: u16 = tcp_runtime::FLAG_FIN;
pub const FLAG_SYN: u16 = tcp_runtime::FLAG_SYN;
pub const FLAG_RST: u16 = 4;
pub const FLAG_PSH: u16 = 8;
pub const FLAG_ACK: u16 = 16;
pub const OPTION_MSS_MASK: u32 = 0xffff;
pub const OPTION_WINDOW_SCALE_PRESENT: u32 = 1 << 16;
pub const OPTION_WINDOW_SCALE_SHIFT: u32 = 17;
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum State {
    Closed,
    SynSent,
    Established,
    FinWait,
    SynReceived,
    LastAck,
    TimeWait,
}
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AckRequest {
    None,
    Delayed,
    Immediate,
}
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WriteBlockReason {
    None,
    RemoteWindow,
    Catalog,
    State,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SegmentView {
    pub source_ip: [u8; 4],
    pub dest_ip: [u8; 4],
    pub source_port: u16,
    pub dest_port: u16,
    pub seq: u32,
    pub ack: u32,
    pub flags: u16,
    pub window: u16,
    pub mss: u16,
    pub window_scale: u8,
    pub window_scale_present: bool,
    pub payload: Text,
}
impl SegmentView {
    pub const EMPTY: Self = Self {
        source_ip: [0; 4],
        dest_ip: [0; 4],
        source_port: 0,
        dest_port: 0,
        seq: 0,
        ack: 0,
        flags: 0,
        window: 0,
        mss: 0,
        window_scale: 0,
        window_scale_present: false,
        payload: Text::literal(b""),
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CleanupResult {
    pub connections: u32,
    pub listeners: u32,
}
impl CleanupResult {
    pub const EMPTY: Self = Self {
        connections: 0,
        listeners: 0,
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Stats {
    pub rx_segments: u64,
    pub tx_segments: u64,
    pub syn_tx: u64,
    pub synack_rx: u64,
    pub synack_tx: u64,
    pub ack_tx: u64,
    pub data_tx: u64,
    pub data_rx: u64,
    pub fin_tx: u64,
    pub rst_rx: u64,
    pub listen_syn_rx: u64,
    pub accepts: u64,
    pub retransmits: u64,
    pub rx_drops: u64,
    pub malformed: u64,
    pub checksum_errors: u64,
    pub timeouts: u64,
    pub self_tests: u64,
    pub write_calls: u64,
    pub write_requested_bytes: u64,
    pub write_completed_bytes: u64,
    pub write_segments: u64,
    pub write_partial: u64,
    pub remote_window_stalls: u64,
    pub catalog_stalls: u64,
    pub backend_busy_stalls: u64,
    pub pure_ack_tx: u64,
    pub delayed_ack_requests: u64,
    pub delayed_ack_tx: u64,
    pub immediate_ack_tx: u64,
    pub ack_coalesced: u64,
    pub ack_piggybacked: u64,
    pub window_update_tx: u64,
    pub outstanding_segments_peak: u32,
    pub outstanding_bytes_peak: u32,
    pub outstanding_segments_current: u32,
    pub outstanding_bytes_current: u32,
    pub mss_negotiated: u64,
    pub window_scale_negotiated: u64,
    pub last_flags: u16,
    pub last_source_port: u16,
    pub last_dest_port: u16,
    pub last_seq: u32,
    pub last_ack: u32,
    pub last_payload_len: u32,
    pub last_error: Text,
}
impl Stats {
    pub const EMPTY: Self = Self {
        rx_segments: 0,
        tx_segments: 0,
        syn_tx: 0,
        synack_rx: 0,
        synack_tx: 0,
        ack_tx: 0,
        data_tx: 0,
        data_rx: 0,
        fin_tx: 0,
        rst_rx: 0,
        listen_syn_rx: 0,
        accepts: 0,
        retransmits: 0,
        rx_drops: 0,
        malformed: 0,
        checksum_errors: 0,
        timeouts: 0,
        self_tests: 0,
        write_calls: 0,
        write_requested_bytes: 0,
        write_completed_bytes: 0,
        write_segments: 0,
        write_partial: 0,
        remote_window_stalls: 0,
        catalog_stalls: 0,
        backend_busy_stalls: 0,
        pure_ack_tx: 0,
        delayed_ack_requests: 0,
        delayed_ack_tx: 0,
        immediate_ack_tx: 0,
        ack_coalesced: 0,
        ack_piggybacked: 0,
        window_update_tx: 0,
        outstanding_segments_peak: 0,
        outstanding_bytes_peak: 0,
        outstanding_segments_current: 0,
        outstanding_bytes_current: 0,
        mss_negotiated: 0,
        window_scale_negotiated: 0,
        last_flags: 0,
        last_source_port: 0,
        last_dest_port: 0,
        last_seq: 0,
        last_ack: 0,
        last_payload_len: 0,
        last_error: Text::literal(b"none"),
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SendPlan {
    pub local_port: u16,
    pub remote_port: u16,
    pub remote_ip: [u8; 4],
    pub seq: u32,
    pub ack: u32,
    pub rx_window: u16,
    pub options: u32,
}
impl SendPlan {
    pub const EMPTY: Self = Self {
        local_port: 0,
        remote_port: 0,
        remote_ip: [0; 4],
        seq: 0,
        ack: 0,
        rx_window: 0,
        options: 0,
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RetransmitPlan {
    pub local_port: u16,
    pub remote_port: u16,
    pub remote_ip: [u8; 4],
    pub seq: u32,
    pub ack: u32,
    pub flags: u16,
    pub rx_window: u16,
    pub options: u32,
    pub payload_storage: [u8; SENT_PAYLOAD_CAPACITY],
    pub payload_len: u16,
    pub token: u64,
    pub sent_tick: u64,
    pub retransmits: u8,
}
impl RetransmitPlan {
    pub const EMPTY: Self = Self {
        local_port: 0,
        remote_port: 0,
        remote_ip: [0; 4],
        seq: 0,
        ack: 0,
        flags: 0,
        rx_window: 0,
        options: 0,
        payload_storage: [0; SENT_PAYLOAD_CAPACITY],
        payload_len: 0,
        token: 0,
        sent_tick: 0,
        retransmits: 0,
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct OutstandingInfo {
    pub sent_tick: u64,
    pub retransmits: u8,
}
impl OutstandingInfo {
    pub const EMPTY: Self = Self {
        sent_tick: 0,
        retransmits: 0,
    };
}
#[repr(C)]
pub struct Connection {
    pub used: bool,
    pub id: u32,
    pub generation: u32,
    pub listener_generation: u64,
    pub state: State,
    pub local_port: u16,
    pub remote_port: u16,
    pub remote_ip: [u8; 4],
    pub seq: u32,
    pub ack: u32,
    pub tx_bytes: u64,
    pub rx_bytes: u64,
    pub retransmits: u32,
    pub tx_window: u32,
    pub tx_ack: u32,
    pub peer_mss: u16,
    pub peer_window_scale: u8,
    pub window_scale_offered: bool,
    pub window_scale_received: bool,
    pub window_scale_active: bool,
    pub negotiation_counted: bool,
    pub last_advertised_window: u16,
    pub delayed_ack_pending: bool,
    pub delayed_ack_immediate: bool,
    pub delayed_ack_segments: u8,
    pub delayed_ack_deadline: u64,
    pub window_update_pending: bool,
    pub rx_drops: u32,
    pub accepted_claimed: bool,
    pub close_requested: bool,
    pub time_wait_until: u64,
    pub rx_head: usize,
    pub rx_len: usize,
    pub rx: [u8; BUFFER_SIZE],
    pub last_seq: u32,
    pub last_ack: u32,
    pub last_flags: u16,
    pub last_len: usize,
    pub sent: tcp_runtime::SendCatalog,
    pub out_of_order: [OutOfOrderSegment; OUT_OF_ORDER_SLOT_COUNT],
}
impl Connection {
    pub const EMPTY: Self = Self {
        used: false,
        id: 0,
        generation: 0,
        listener_generation: 0,
        state: State::Closed,
        local_port: 0,
        remote_port: 0,
        remote_ip: [0; 4],
        seq: 0,
        ack: 0,
        tx_bytes: 0,
        rx_bytes: 0,
        retransmits: 0,
        tx_window: 0,
        tx_ack: 0,
        peer_mss: DEFAULT_PEER_MSS,
        peer_window_scale: 0,
        window_scale_offered: false,
        window_scale_received: false,
        window_scale_active: false,
        negotiation_counted: false,
        last_advertised_window: 0,
        delayed_ack_pending: false,
        delayed_ack_immediate: false,
        delayed_ack_segments: 0,
        delayed_ack_deadline: 0,
        window_update_pending: false,
        rx_drops: 0,
        accepted_claimed: false,
        close_requested: false,
        time_wait_until: 0,
        rx_head: 0,
        rx_len: 0,
        rx: [0; BUFFER_SIZE],
        last_seq: 0,
        last_ack: 0,
        last_flags: 0,
        last_len: 0,
        sent: tcp_runtime::SendCatalog::EMPTY,
        out_of_order: [OutOfOrderSegment::EMPTY; OUT_OF_ORDER_SLOT_COUNT],
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct OutOfOrderSegment {
    pub valid: bool,
    pub seq: u32,
    pub len: usize,
    pub payload: [u8; OUT_OF_ORDER_SLOT_SIZE],
}
impl OutOfOrderSegment {
    pub const EMPTY: Self = Self {
        valid: false,
        seq: 0,
        len: 0,
        payload: [0; OUT_OF_ORDER_SLOT_SIZE],
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Listener {
    pub used: bool,
    pub port: u16,
    pub generation: u64,
}
impl Listener {
    pub const EMPTY: Self = Self {
        used: false,
        port: 0,
        generation: 0,
    };
}
struct Storage<T>(UnsafeCell<T>);
// SAFETY: all table access requires exact common network admission. No
// exclusive reference to a whole table is formed. Local slot/statistic
// borrows cover resident storage only; foreign-byte staging ends them before
// a caller-memory fault. The task owner remains held through copy completion.
unsafe impl<T> Sync for Storage<T> {}
static STATS: Storage<Stats> = Storage(UnsafeCell::new(Stats::EMPTY));
static CONNECTIONS: Storage<[Connection; MAX_CONNECTIONS]> = Storage(UnsafeCell::new(
    [const { Connection::EMPTY }; MAX_CONNECTIONS],
));
static GENERATIONS: Storage<[u32; MAX_CONNECTIONS]> =
    Storage(UnsafeCell::new([0; MAX_CONNECTIONS]));
static LISTENERS: Storage<[Listener; MAX_LISTENERS]> =
    Storage(UnsafeCell::new([Listener::EMPTY; MAX_LISTENERS]));
static NEXT_ID: Storage<u32> = Storage(UnsafeCell::new(1));
static NEXT_PORT: Storage<u16> = Storage(UnsafeCell::new(49152));
// Never reset on close/abort or a later TCP reset: a port is not a lifetime.
static NEXT_LISTENER_GENERATION: Storage<u64> = Storage(UnsafeCell::new(1));
fn stats() -> *mut Stats {
    STATS.0.get()
}
fn connection(slot: usize) -> *mut Connection {
    assert!(slot < MAX_CONNECTIONS);
    unsafe { CONNECTIONS.0.get().cast::<Connection>().add(slot) }
}
fn listener(slot: usize) -> *mut Listener {
    assert!(slot < MAX_LISTENERS);
    unsafe { LISTENERS.0.get().cast::<Listener>().add(slot) }
}
unsafe fn payload<'a>(view: Text) -> &'a [u8] {
    if view.len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(view.pointer, view.len) }
    }
}
fn text(value: &'static [u8]) -> Text {
    Text::literal(value)
}

pub fn option_metadata(mss: u16, scale: Option<u8>) -> u32 {
    let mut value = u32::from(mss);
    if let Some(scale) = scale {
        value |= OPTION_WINDOW_SCALE_PRESENT;
        value |= u32::from(scale.min(MAX_WINDOW_SCALE)) << OPTION_WINDOW_SCALE_SHIFT;
    }
    value
}
pub fn option_mss(metadata: u32) -> u16 {
    (metadata & OPTION_MSS_MASK) as u16
}
pub fn option_window_scale(metadata: u32) -> Option<u8> {
    if metadata & OPTION_WINDOW_SCALE_PRESENT == 0 {
        None
    } else {
        Some(((metadata >> OPTION_WINDOW_SCALE_SHIFT) & 15) as u8)
    }
}
unsafe fn by_id(id: u32) -> Option<*mut Connection> {
    unsafe {
        for slot in 0..MAX_CONNECTIONS {
            let c = connection(slot);
            if (*c).used && (*c).id == id {
                return Some(c);
            }
        }
        None
    }
}
unsafe fn match_connection(view: SegmentView) -> Option<*mut Connection> {
    unsafe {
        for slot in 0..MAX_CONNECTIONS {
            let c = connection(slot);
            if !(*c).used || (*c).state == State::Closed {
                continue;
            }
            if (*c).local_port == view.dest_port
                && (*c).remote_port == view.source_port
                && (*c).remote_ip == view.source_ip
            {
                return Some(c);
            }
        }
        None
    }
}
unsafe fn next_slot_generation(slot: usize) -> u32 {
    unsafe {
        let value = &mut (*GENERATIONS.0.get())[slot];
        *value = value.wrapping_add(1);
        if *value == 0 {
            *value = 1;
        }
        *value
    }
}
unsafe fn note_catalog_added(len: usize) {
    unsafe {
        let s = &mut *stats();
        s.outstanding_segments_current = s.outstanding_segments_current.wrapping_add(1);
        s.outstanding_bytes_current = s.outstanding_bytes_current.wrapping_add(len as u32);
        s.outstanding_segments_peak = s
            .outstanding_segments_peak
            .max(s.outstanding_segments_current);
        s.outstanding_bytes_peak = s.outstanding_bytes_peak.max(s.outstanding_bytes_current);
    }
}
unsafe fn note_catalog_reduced(
    before_segments: usize,
    before_bytes: usize,
    after_segments: usize,
    after_bytes: usize,
) {
    unsafe {
        let s = &mut *stats();
        if before_segments > after_segments {
            s.outstanding_segments_current = s
                .outstanding_segments_current
                .saturating_sub((before_segments - after_segments) as u32);
        }
        if before_bytes > after_bytes {
            s.outstanding_bytes_current = s
                .outstanding_bytes_current
                .saturating_sub((before_bytes - after_bytes) as u32);
        }
    }
}
unsafe fn discard_catalog(c: &mut Connection) {
    unsafe { note_catalog_reduced(c.sent.count(), c.sent.outstanding_bytes(), 0, 0) };
    c.sent.reset();
}
unsafe fn reset_connection_slot(c: &mut Connection) {
    unsafe { discard_catalog(c) };
    c.used = false;
    c.id = 0;
    c.generation = 0;
    c.listener_generation = 0;
    c.state = State::Closed;
    c.local_port = 0;
    c.remote_port = 0;
    c.remote_ip = [0; 4];
    c.seq = 0;
    c.ack = 0;
    c.tx_bytes = 0;
    c.rx_bytes = 0;
    c.retransmits = 0;
    c.tx_window = 0;
    c.tx_ack = 0;
    c.peer_mss = DEFAULT_PEER_MSS;
    c.peer_window_scale = 0;
    c.window_scale_offered = false;
    c.window_scale_received = false;
    c.window_scale_active = false;
    c.negotiation_counted = false;
    c.last_advertised_window = 0;
    c.delayed_ack_pending = false;
    c.delayed_ack_immediate = false;
    c.delayed_ack_segments = 0;
    c.delayed_ack_deadline = 0;
    c.window_update_pending = false;
    c.rx_drops = 0;
    c.accepted_claimed = false;
    c.close_requested = false;
    c.time_wait_until = 0;
    c.rx_head = 0;
    c.rx_len = 0;
    c.last_seq = 0;
    c.last_ack = 0;
    c.last_flags = 0;
    c.last_len = 0;
    for slot in &mut c.out_of_order {
        slot.valid = false;
    }
}
pub(super) unsafe fn reset() {
    unsafe {
        stats().write(Stats::EMPTY);
        for slot in 0..MAX_CONNECTIONS {
            reset_connection_slot(&mut *connection(slot));
        }
        GENERATIONS.0.get().write([0; MAX_CONNECTIONS]);
        LISTENERS.0.get().write([Listener::EMPTY; MAX_LISTENERS]);
        NEXT_ID.0.get().write(1);
        NEXT_PORT.0.get().write(49152);
    }
}
unsafe fn reap_closed_connection_slots() -> u32 {
    unsafe {
        let mut count = 0;
        for slot in 0..MAX_CONNECTIONS {
            let c = &mut *connection(slot);
            if c.used && c.state == State::Closed {
                reset_connection_slot(c);
                count += 1;
            }
        }
        if count != 0 {
            (*stats()).last_error = text(b"closed-reap");
        }
        count
    }
}
unsafe fn alloc_connection() -> Option<usize> {
    unsafe {
        for slot in 0..MAX_CONNECTIONS {
            if !(*connection(slot)).used {
                return Some(slot);
            }
        }
        reap_closed_connection_slots();
        (0..MAX_CONNECTIONS).find(|&slot| !(*connection(slot)).used)
    }
}
unsafe fn active_count() -> u32 {
    unsafe {
        (0..MAX_CONNECTIONS)
            .filter(|&slot| (*connection(slot)).used && (*connection(slot)).state != State::Closed)
            .count() as u32
    }
}
unsafe fn listener_count() -> u32 {
    unsafe {
        (0..MAX_LISTENERS)
            .filter(|&slot| (*listener(slot)).used)
            .count() as u32
    }
}
pub(super) unsafe fn record_write_start(requested: usize) {
    unsafe {
        (*stats()).write_calls = (*stats()).write_calls.wrapping_add(1);
        (*stats()).write_requested_bytes = (*stats())
            .write_requested_bytes
            .wrapping_add(requested as u64);
    }
}
pub(super) unsafe fn record_write_finish(
    requested: usize,
    written: usize,
    segments: usize,
    block: WriteBlockReason,
) {
    unsafe {
        let s = &mut *stats();
        s.write_completed_bytes = s.write_completed_bytes.wrapping_add(written as u64);
        s.write_segments = s.write_segments.wrapping_add(segments as u64);
        if written < requested {
            s.write_partial = s.write_partial.wrapping_add(1);
        }
        match block {
            WriteBlockReason::RemoteWindow => {
                s.remote_window_stalls = s.remote_window_stalls.wrapping_add(1)
            }
            WriteBlockReason::Catalog => s.catalog_stalls = s.catalog_stalls.wrapping_add(1),
            _ => {}
        }
    }
}
pub(super) unsafe fn record_backend_busy() {
    unsafe {
        (*stats()).backend_busy_stalls = (*stats()).backend_busy_stalls.wrapping_add(1);
    }
}
pub(super) unsafe fn summary() -> Summary {
    unsafe {
        let s = &*stats();
        Summary {
            max_connections: MAX_CONNECTIONS as u32,
            active_connections: active_count(),
            buffer_size: BUFFER_SIZE as u32,
            active_listeners: listener_count(),
            syn_tx: s.syn_tx,
            synack_rx: s.synack_rx,
            synack_tx: s.synack_tx,
            ack_tx: s.ack_tx,
            data_tx: s.data_tx,
            data_rx: s.data_rx,
            fin_tx: s.fin_tx,
            rst_rx: s.rst_rx,
            listen_syn_rx: s.listen_syn_rx,
            accepts: s.accepts,
            retransmits: s.retransmits,
            rx_drops: s.rx_drops,
            last_source_port: s.last_source_port,
            last_dest_port: s.last_dest_port,
            last_seq: s.last_seq,
            last_ack: s.last_ack,
            last_payload_len: s.last_payload_len,
            checksum_errors: s.checksum_errors,
            timeouts: s.timeouts,
            self_tests: s.self_tests,
        }
    }
}
pub(super) unsafe fn connection_info(index: u32) -> Option<ConnectionInfo> {
    if index as usize >= MAX_CONNECTIONS {
        return None;
    }
    unsafe {
        let c = &*connection(index as usize);
        if !c.used {
            return None;
        }
        Some(ConnectionInfo {
            id: c.id,
            state: c.state as u8,
            local_port: c.local_port,
            remote_port: c.remote_port,
            remote_ip: c.remote_ip,
            tx_bytes: c.tx_bytes,
            rx_bytes: c.rx_bytes,
            pending_rx: c.rx_len as u32,
            retransmits: c.retransmits,
            rx_window: rx_free(c) as u32,
            tx_window: c.tx_window,
            tx_ack: c.tx_ack,
            rx_drops: c.rx_drops,
            seq: c.seq,
            ack: c.ack,
            last_seq: c.last_seq,
            last_ack: c.last_ack,
            last_flags: c.last_flags,
            last_payload_len: u16::try_from(c.last_len).expect("TCP last payload"),
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct ListenerIdentity {
    pub slot: usize,
    pub port: u16,
    pub generation: u64,
}

// Existing diagnostic probes, ported with the original values and checks.
// Keep raw slot identities across state-machine calls: a probe never retains
// a Rust reference while another call mutates the same connection.
struct ProbeEnd {
    id: u32,
    abort: bool,
}
impl Drop for ProbeEnd {
    fn drop(&mut self) {
        unsafe {
            if self.abort {
                abort(self.id, text(b"performance-probe"));
            } else {
                close(self.id);
            }
        }
    }
}

const _: () = {
    assert!(core::mem::size_of::<SegmentView>() == 48);
    assert!(core::mem::align_of::<SegmentView>() == 8);
    assert!(core::mem::offset_of!(SegmentView, source_ip) == 0);
    assert!(core::mem::offset_of!(SegmentView, dest_ip) == 4);
    assert!(core::mem::offset_of!(SegmentView, source_port) == 8);
    assert!(core::mem::offset_of!(SegmentView, dest_port) == 10);
    assert!(core::mem::offset_of!(SegmentView, seq) == 12);
    assert!(core::mem::offset_of!(SegmentView, ack) == 16);
    assert!(core::mem::offset_of!(SegmentView, flags) == 20);
    assert!(core::mem::offset_of!(SegmentView, window) == 22);
    assert!(core::mem::offset_of!(SegmentView, mss) == 24);
    assert!(core::mem::offset_of!(SegmentView, window_scale) == 26);
    assert!(core::mem::offset_of!(SegmentView, window_scale_present) == 27);
    assert!(core::mem::offset_of!(SegmentView, payload) == 32);
};
const _: () = {
    assert!(core::mem::size_of::<Stats>() == 336);
    assert!(core::mem::align_of::<Stats>() == 8);
    assert!(core::mem::offset_of!(Stats, rx_segments) == 0);
    assert!(core::mem::offset_of!(Stats, tx_segments) == 8);
    assert!(core::mem::offset_of!(Stats, syn_tx) == 16);
    assert!(core::mem::offset_of!(Stats, synack_rx) == 24);
    assert!(core::mem::offset_of!(Stats, synack_tx) == 32);
    assert!(core::mem::offset_of!(Stats, ack_tx) == 40);
    assert!(core::mem::offset_of!(Stats, data_tx) == 48);
    assert!(core::mem::offset_of!(Stats, data_rx) == 56);
    assert!(core::mem::offset_of!(Stats, fin_tx) == 64);
    assert!(core::mem::offset_of!(Stats, rst_rx) == 72);
    assert!(core::mem::offset_of!(Stats, listen_syn_rx) == 80);
    assert!(core::mem::offset_of!(Stats, accepts) == 88);
    assert!(core::mem::offset_of!(Stats, retransmits) == 96);
    assert!(core::mem::offset_of!(Stats, rx_drops) == 104);
    assert!(core::mem::offset_of!(Stats, malformed) == 112);
    assert!(core::mem::offset_of!(Stats, checksum_errors) == 120);
    assert!(core::mem::offset_of!(Stats, timeouts) == 128);
    assert!(core::mem::offset_of!(Stats, self_tests) == 136);
    assert!(core::mem::offset_of!(Stats, write_calls) == 144);
    assert!(core::mem::offset_of!(Stats, write_requested_bytes) == 152);
    assert!(core::mem::offset_of!(Stats, write_completed_bytes) == 160);
    assert!(core::mem::offset_of!(Stats, write_segments) == 168);
    assert!(core::mem::offset_of!(Stats, write_partial) == 176);
    assert!(core::mem::offset_of!(Stats, remote_window_stalls) == 184);
    assert!(core::mem::offset_of!(Stats, catalog_stalls) == 192);
    assert!(core::mem::offset_of!(Stats, backend_busy_stalls) == 200);
    assert!(core::mem::offset_of!(Stats, pure_ack_tx) == 208);
    assert!(core::mem::offset_of!(Stats, delayed_ack_requests) == 216);
    assert!(core::mem::offset_of!(Stats, delayed_ack_tx) == 224);
    assert!(core::mem::offset_of!(Stats, immediate_ack_tx) == 232);
    assert!(core::mem::offset_of!(Stats, ack_coalesced) == 240);
    assert!(core::mem::offset_of!(Stats, ack_piggybacked) == 248);
    assert!(core::mem::offset_of!(Stats, window_update_tx) == 256);
    assert!(core::mem::offset_of!(Stats, outstanding_segments_peak) == 264);
    assert!(core::mem::offset_of!(Stats, outstanding_bytes_peak) == 268);
    assert!(core::mem::offset_of!(Stats, outstanding_segments_current) == 272);
    assert!(core::mem::offset_of!(Stats, outstanding_bytes_current) == 276);
    assert!(core::mem::offset_of!(Stats, mss_negotiated) == 280);
    assert!(core::mem::offset_of!(Stats, window_scale_negotiated) == 288);
    assert!(core::mem::offset_of!(Stats, last_flags) == 296);
    assert!(core::mem::offset_of!(Stats, last_source_port) == 298);
    assert!(core::mem::offset_of!(Stats, last_dest_port) == 300);
    assert!(core::mem::offset_of!(Stats, last_seq) == 304);
    assert!(core::mem::offset_of!(Stats, last_ack) == 308);
    assert!(core::mem::offset_of!(Stats, last_payload_len) == 312);
    assert!(core::mem::offset_of!(Stats, last_error) == 320);
};
const _: () = {
    assert!(core::mem::size_of::<CleanupResult>() == 8);
    assert!(core::mem::align_of::<CleanupResult>() == 4);
    assert!(core::mem::offset_of!(CleanupResult, connections) == 0);
    assert!(core::mem::offset_of!(CleanupResult, listeners) == 4);
};
const _: () = {
    assert!(core::mem::size_of::<SendPlan>() == 24);
    assert!(core::mem::align_of::<SendPlan>() == 4);
    assert!(core::mem::offset_of!(SendPlan, local_port) == 0);
    assert!(core::mem::offset_of!(SendPlan, remote_port) == 2);
    assert!(core::mem::offset_of!(SendPlan, remote_ip) == 4);
    assert!(core::mem::offset_of!(SendPlan, seq) == 8);
    assert!(core::mem::offset_of!(SendPlan, ack) == 12);
    assert!(core::mem::offset_of!(SendPlan, rx_window) == 16);
    assert!(core::mem::offset_of!(SendPlan, options) == 20);
};
const _: () = {
    assert!(core::mem::size_of::<OutstandingInfo>() == 16);
    assert!(core::mem::align_of::<OutstandingInfo>() == 8);
    assert!(core::mem::offset_of!(OutstandingInfo, sent_tick) == 0);
    assert!(core::mem::offset_of!(OutstandingInfo, retransmits) == 8);
};

impl RetransmitPlan {
    pub fn payload_slice(&self) -> &[u8] {
        &self.payload_storage[..usize::from(self.payload_len)]
    }
}

const _: () = {
    assert!(core::mem::size_of::<RetransmitPlan>() == 1512);
    assert!(core::mem::align_of::<RetransmitPlan>() == 8);
    assert!(core::mem::offset_of!(RetransmitPlan, local_port) == 0);
    assert!(core::mem::offset_of!(RetransmitPlan, remote_port) == 2);
    assert!(core::mem::offset_of!(RetransmitPlan, remote_ip) == 4);
    assert!(core::mem::offset_of!(RetransmitPlan, seq) == 8);
    assert!(core::mem::offset_of!(RetransmitPlan, ack) == 12);
    assert!(core::mem::offset_of!(RetransmitPlan, flags) == 16);
    assert!(core::mem::offset_of!(RetransmitPlan, rx_window) == 18);
    assert!(core::mem::offset_of!(RetransmitPlan, options) == 20);
    assert!(core::mem::offset_of!(RetransmitPlan, payload_storage) == 24);
    assert!(core::mem::offset_of!(RetransmitPlan, payload_len) == 1484);
    assert!(core::mem::offset_of!(RetransmitPlan, token) == 1488);
    assert!(core::mem::offset_of!(RetransmitPlan, sent_tick) == 1496);
    assert!(core::mem::offset_of!(RetransmitPlan, retransmits) == 1504);
};

// Private operation modules share the sole state and original typed owners.
mod connections;
mod contract_checks;
mod foreign_api;
mod receive;
mod transmit_io;
pub(super) use connections::{
    abort, abort_all, abort_connections_on_port, accept_inbound, begin_live_connect,
    claim_accepted, close_listener, close_ready, connection_id_for_segment, connection_identity,
    connection_identity_at, connection_identity_for_segment, connection_on_port,
    connection_with_data_on_port, established, fin_waiting, finish_listener, has_listener,
    identity_active, listen, listener_identity, mark_timeout, query_listener,
    record_checksum_error, record_malformed, request_close, set_error,
};
pub(super) use contract_checks::{
    buffer_limit_probe, graceful_close_probe, out_of_order_probe, overlap_retransmit_probe,
    performance_contract_probe, self_test_connect, wraparound_probe,
};
use foreign_api::loopback_write_foreign;
pub(super) use foreign_api::wait_observations;
pub use foreign_api::{
    r4os_rust_net_tcp_abort, r4os_rust_net_tcp_abort_all,
    r4os_rust_net_tcp_abort_connections_on_port, r4os_rust_net_tcp_accept_inbound,
    r4os_rust_net_tcp_apply_rx_view, r4os_rust_net_tcp_apply_tx_view,
    r4os_rust_net_tcp_begin_live_connect, r4os_rust_net_tcp_buffer_limit_probe,
    r4os_rust_net_tcp_can_track_send, r4os_rust_net_tcp_claim_accepted, r4os_rust_net_tcp_close,
    r4os_rust_net_tcp_close_listener, r4os_rust_net_tcp_close_ready, r4os_rust_net_tcp_closed,
    r4os_rust_net_tcp_commit_sent, r4os_rust_net_tcp_connection_id_for_segment,
    r4os_rust_net_tcp_connection_identity, r4os_rust_net_tcp_connection_identity_at,
    r4os_rust_net_tcp_connection_identity_for_segment, r4os_rust_net_tcp_connection_info,
    r4os_rust_net_tcp_connection_on_port, r4os_rust_net_tcp_connection_slot_bytes,
    r4os_rust_net_tcp_connection_with_data_on_port, r4os_rust_net_tcp_delayed_ack_due,
    r4os_rust_net_tcp_established, r4os_rust_net_tcp_fin_waiting, r4os_rust_net_tcp_get_stats,
    r4os_rust_net_tcp_graceful_close_probe, r4os_rust_net_tcp_has_listener,
    r4os_rust_net_tcp_identity_active, r4os_rust_net_tcp_listen, r4os_rust_net_tcp_loopback_write,
    r4os_rust_net_tcp_mark_timeout, r4os_rust_net_tcp_note_ack_sent,
    r4os_rust_net_tcp_option_metadata, r4os_rust_net_tcp_option_mss,
    r4os_rust_net_tcp_option_window_scale, r4os_rust_net_tcp_out_of_order_probe,
    r4os_rust_net_tcp_outstanding_bytes, r4os_rust_net_tcp_outstanding_count,
    r4os_rust_net_tcp_outstanding_info, r4os_rust_net_tcp_outstanding_totals,
    r4os_rust_net_tcp_overlap_retransmit_probe, r4os_rust_net_tcp_performance_contract_probe,
    r4os_rust_net_tcp_plan_retransmit, r4os_rust_net_tcp_plan_send, r4os_rust_net_tcp_read,
    r4os_rust_net_tcp_readable, r4os_rust_net_tcp_reap_time_wait,
    r4os_rust_net_tcp_record_backend_busy, r4os_rust_net_tcp_record_checksum_error,
    r4os_rust_net_tcp_record_malformed, r4os_rust_net_tcp_record_retransmit,
    r4os_rust_net_tcp_record_write_finish, r4os_rust_net_tcp_record_write_start,
    r4os_rust_net_tcp_request_ack, r4os_rust_net_tcp_request_close, r4os_rust_net_tcp_reset,
    r4os_rust_net_tcp_rx_ack_of, r4os_rust_net_tcp_self_test_connect,
    r4os_rust_net_tcp_send_allowance, r4os_rust_net_tcp_set_error, r4os_rust_net_tcp_summary,
    r4os_rust_net_tcp_tx_ack_of, r4os_rust_net_tcp_window_update_required,
    r4os_rust_net_tcp_wraparound_probe, r4os_rust_net_tcp_write_block_reason,
};
use receive::{
    advertised_window, advertised_window_for_flags, flush_out_of_order_payload, options_for_flags,
    out_of_order_count, rx_free, sanitized_peer_mss, write_rx,
};
pub(super) use receive::{apply_rx_view, apply_tx_view, reap_time_wait};
pub(super) use transmit_io::{
    can_track_send, close, closed, commit_sent, delayed_ack_due, loopback_write, note_ack_sent,
    outstanding_bytes, outstanding_count, outstanding_info, plan_retransmit, plan_send, read,
    readable, record_retransmit, request_ack, rx_ack_of, send_allowance, tx_ack_of,
    window_update_required, write_block_reason,
};
