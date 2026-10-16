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
}
impl Listener {
    pub const EMPTY: Self = Self {
        used: false,
        port: 0,
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
unsafe fn connection_info(index: u32) -> Option<ConnectionInfo> {
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
pub(super) unsafe fn self_test_connect(remote_ip: [u8; 4], remote_port: u16) -> i32 {
    unsafe {
        let Some(slot) = alloc_connection() else {
            return -1;
        };
        let c = &mut *connection(slot);
        reset_connection_slot(c);
        c.used = true;
        c.id = *NEXT_ID.0.get();
        c.generation = next_slot_generation(slot);
        c.state = State::SynSent;
        c.local_port = *NEXT_PORT.0.get();
        c.remote_port = remote_port;
        c.remote_ip = remote_ip;
        c.seq = 0x1000 + *NEXT_ID.0.get();
        *NEXT_ID.0.get() = (*NEXT_ID.0.get()).wrapping_add(1);
        *NEXT_PORT.0.get() = (*NEXT_PORT.0.get()).wrapping_add(1);
        (*stats()).syn_tx += 1;
        (*stats()).synack_rx += 1;
        (*stats()).ack_tx += 1;
        c.ack = 0x2001;
        c.state = State::Established;
        i32::try_from(c.id).expect("TCP self connection id")
    }
}
pub(super) unsafe fn begin_live_connect(
    remote_ip: [u8; 4],
    remote_port: u16,
    initial_seq: u32,
) -> i32 {
    unsafe {
        let Some(slot) = alloc_connection() else {
            (*stats()).last_error = text(b"no-conn");
            return -1;
        };
        let c = &mut *connection(slot);
        reset_connection_slot(c);
        c.used = true;
        c.id = *NEXT_ID.0.get();
        c.generation = next_slot_generation(slot);
        c.state = State::SynSent;
        c.local_port = *NEXT_PORT.0.get();
        c.remote_port = remote_port;
        c.remote_ip = remote_ip;
        c.seq = initial_seq;
        c.window_scale_offered = true;
        *NEXT_ID.0.get() = (*NEXT_ID.0.get()).wrapping_add(1);
        if *NEXT_ID.0.get() == 0 {
            *NEXT_ID.0.get() = 1;
        }
        *NEXT_PORT.0.get() = (*NEXT_PORT.0.get()).wrapping_add(1);
        if *NEXT_PORT.0.get() < 49152 {
            *NEXT_PORT.0.get() = 49152;
        }
        (*stats()).last_error = text(b"syn-sent");
        i32::try_from(c.id).expect("TCP live connection id")
    }
}
pub(super) unsafe fn has_listener(port: u16) -> bool {
    unsafe {
        (0..MAX_LISTENERS).any(|slot| (*listener(slot)).used && (*listener(slot)).port == port)
    }
}
pub(super) unsafe fn listen(port: u16) -> bool {
    unsafe {
        if port == 0 {
            return false;
        }
        if has_listener(port) {
            return true;
        }
        for slot in 0..MAX_LISTENERS {
            let l = &mut *listener(slot);
            if l.used {
                continue;
            }
            *l = Listener { used: true, port };
            (*stats()).last_error = text(b"listen");
            return true;
        }
        (*stats()).last_error = text(b"listen-full");
        false
    }
}
pub(super) unsafe fn close_listener(port: u16) {
    unsafe {
        for slot in 0..MAX_LISTENERS {
            let l = &mut *listener(slot);
            if l.used && l.port == port {
                *l = Listener::EMPTY;
                (*stats()).last_error = text(b"listen-close");
                return;
            }
        }
    }
}
pub(super) unsafe fn accept_inbound(view: SegmentView, initial_seq: u32) -> Option<u32> {
    unsafe {
        if !has_listener(view.dest_port) || view.flags & FLAG_SYN == 0 || view.flags & FLAG_ACK != 0
        {
            return None;
        }
        if let Some(c) = match_connection(view) {
            if (*c).state == State::SynReceived {
                return Some((*c).id);
            }
            reset_connection_slot(&mut *c);
            (*stats()).last_error = text(b"syn-reuse");
        }
        let Some(slot) = alloc_connection() else {
            (*stats()).last_error = text(b"accept-full");
            return None;
        };
        let c = &mut *connection(slot);
        reset_connection_slot(c);
        c.used = true;
        c.id = *NEXT_ID.0.get();
        c.generation = next_slot_generation(slot);
        c.state = State::SynReceived;
        c.local_port = view.dest_port;
        c.remote_port = view.source_port;
        c.remote_ip = view.source_ip;
        c.seq = initial_seq;
        c.ack = view.seq.wrapping_add(1);
        c.tx_window = u32::from(view.window);
        c.peer_mss = sanitized_peer_mss(view.mss);
        c.peer_window_scale = if view.window_scale_present {
            view.window_scale.min(MAX_WINDOW_SCALE)
        } else {
            0
        };
        c.window_scale_offered = view.window_scale_present;
        c.window_scale_received = view.window_scale_present;
        c.window_scale_active = view.window_scale_present;
        *NEXT_ID.0.get() = (*NEXT_ID.0.get()).wrapping_add(1);
        if *NEXT_ID.0.get() == 0 {
            *NEXT_ID.0.get() = 1;
        }
        (*stats()).listen_syn_rx += 1;
        (*stats()).last_error = text(b"syn-rx");
        Some(c.id)
    }
}
pub(super) unsafe fn abort(conn_id: u32, reason: Text) {
    unsafe {
        if let Some(c) = by_id(conn_id) {
            discard_catalog(&mut *c);
            (*c).state = State::Closed;
            (*c).used = false;
            (*stats()).last_error = reason;
        }
    }
}
pub(super) unsafe fn mark_timeout(reason: Text) {
    unsafe {
        (*stats()).timeouts += 1;
        (*stats()).last_error = reason;
    }
}
pub(super) unsafe fn record_malformed(reason: Text) {
    unsafe {
        (*stats()).malformed += 1;
        (*stats()).last_error = reason;
    }
}
pub(super) unsafe fn record_checksum_error() {
    unsafe {
        (*stats()).checksum_errors += 1;
        (*stats()).last_error = text(b"checksum");
    }
}
pub(super) unsafe fn set_error(reason: Text) {
    unsafe {
        (*stats()).last_error = reason;
    }
}
pub(super) unsafe fn established(id: u32) -> bool {
    unsafe { by_id(id).is_some_and(|c| (*c).state == State::Established) }
}
pub(super) unsafe fn request_close(id: u32) -> bool {
    unsafe {
        let Some(c) = by_id(id) else {
            return false;
        };
        match (*c).state {
            State::Established => {
                (*c).close_requested = true;
                (*stats()).last_error = text(b"close-pending");
                true
            }
            State::FinWait | State::LastAck | State::TimeWait | State::Closed => true,
            _ => false,
        }
    }
}
pub(super) unsafe fn close_ready(id: u32) -> bool {
    unsafe {
        by_id(id).is_some_and(|c| {
            (*c).state == State::Established && (*c).close_requested && (*c).sent.count() == 0
        })
    }
}
pub(super) unsafe fn fin_waiting(id: u32) -> bool {
    unsafe { by_id(id).is_some_and(|c| (*c).state == State::FinWait) }
}
pub(super) unsafe fn connection_identity(id: u32) -> Option<ConnectionIdentity> {
    unsafe {
        for slot in 0..MAX_CONNECTIONS {
            let c = &*connection(slot);
            if c.used && c.id == id && c.generation != 0 {
                return Some(ConnectionIdentity {
                    connection_id: c.id,
                    slot,
                    generation: c.generation,
                });
            }
        }
        None
    }
}
pub(super) unsafe fn connection_identity_at(slot: usize) -> Option<ConnectionIdentity> {
    if slot >= MAX_CONNECTIONS {
        return None;
    }
    unsafe {
        let c = &*connection(slot);
        if !c.used || c.id == 0 || c.generation == 0 {
            None
        } else {
            Some(ConnectionIdentity {
                connection_id: c.id,
                slot,
                generation: c.generation,
            })
        }
    }
}
pub(super) unsafe fn identity_active(identity: ConnectionIdentity) -> bool {
    unsafe { connection_identity_at(identity.slot).is_some_and(|current| current.eql(identity)) }
}
pub(super) unsafe fn connection_with_data_on_port(port: u16) -> Option<u32> {
    unsafe {
        for slot in 0..MAX_CONNECTIONS {
            let c = &*connection(slot);
            if c.used
                && c.state == State::Established
                && c.local_port == port
                && !c.accepted_claimed
                && c.rx_len != 0
            {
                return Some(c.id);
            }
        }
        None
    }
}
pub(super) unsafe fn connection_on_port(port: u16) -> Option<u32> {
    unsafe {
        for slot in 0..MAX_CONNECTIONS {
            let c = &*connection(slot);
            if c.used
                && c.state == State::Established
                && c.local_port == port
                && !c.accepted_claimed
            {
                return Some(c.id);
            }
        }
        None
    }
}
pub(super) unsafe fn claim_accepted(id: u32) -> bool {
    unsafe {
        let Some(c) = by_id(id) else {
            return false;
        };
        if (*c).accepted_claimed {
            return false;
        }
        (*c).accepted_claimed = true;
        true
    }
}
pub(super) unsafe fn connection_id_for_segment(view: SegmentView) -> Option<u32> {
    unsafe { match_connection(view).map(|c| (*c).id) }
}
pub(super) unsafe fn connection_identity_for_segment(
    view: SegmentView,
) -> Option<ConnectionIdentity> {
    unsafe { connection_identity(connection_id_for_segment(view)?) }
}
pub(super) unsafe fn abort_connections_on_port(port: u16, reason: Text) {
    unsafe {
        for slot in 0..MAX_CONNECTIONS {
            let c = &mut *connection(slot);
            if c.used && c.local_port == port {
                reset_connection_slot(c);
                (*stats()).last_error = reason;
            }
        }
    }
}
pub(super) unsafe fn abort_all(reason: Text) -> CleanupResult {
    unsafe {
        let mut result = CleanupResult::EMPTY;
        for slot in 0..MAX_CONNECTIONS {
            let c = &mut *connection(slot);
            if c.used {
                reset_connection_slot(c);
                result.connections += 1;
            }
        }
        for slot in 0..MAX_LISTENERS {
            let l = &mut *listener(slot);
            if l.used {
                *l = Listener::EMPTY;
                result.listeners += 1;
            }
        }
        if result.connections != 0 || result.listeners != 0 {
            (*stats()).last_error = reason;
        }
        result
    }
}
pub(super) unsafe fn plan_send(id: u32, flags: u16) -> Option<SendPlan> {
    unsafe {
        let c = &*by_id(id)?;
        Some(SendPlan {
            local_port: c.local_port,
            remote_port: c.remote_port,
            remote_ip: c.remote_ip,
            seq: c.seq,
            ack: c.ack,
            rx_window: advertised_window_for_flags(c, flags),
            options: options_for_flags(c, flags),
        })
    }
}
pub(super) unsafe fn send_allowance(id: u32, requested: usize) -> usize {
    unsafe {
        let Some(c) = by_id(id) else {
            return 0;
        };
        let c = &*c;
        if c.state != State::Established || c.close_requested {
            return 0;
        }
        tcp_runtime::send_allowance(
            c.tx_window,
            requested.min(usize::from(c.peer_mss)),
            c.sent.has_capacity(),
        )
    }
}
pub(super) unsafe fn write_block_reason(id: u32) -> WriteBlockReason {
    unsafe {
        let Some(c) = by_id(id) else {
            return WriteBlockReason::State;
        };
        let c = &*c;
        if c.state != State::Established || c.close_requested {
            WriteBlockReason::State
        } else if c.tx_window == 0 {
            WriteBlockReason::RemoteWindow
        } else if !c.sent.has_capacity() {
            WriteBlockReason::Catalog
        } else {
            WriteBlockReason::None
        }
    }
}
pub(super) unsafe fn can_track_send(id: u32, flags: u16, len: usize) -> bool {
    unsafe {
        let Some(c) = by_id(id) else {
            return false;
        };
        let c = &*c;
        let ack_only = flags == FLAG_ACK && len == 0;
        if c.state == State::Closed && !ack_only {
            return false;
        }
        !tcp_runtime::needs_tracking(flags, len)
            || (len <= SENT_PAYLOAD_CAPACITY && c.sent.has_capacity())
    }
}
pub(super) unsafe fn plan_retransmit(id: u32, kind: RetransmitKind) -> Option<RetransmitPlan> {
    unsafe {
        let c = &*by_id(id)?;
        if c.state == State::Closed {
            return None;
        }
        let segment = c.sent.oldest(kind)?;
        let bytes = segment.payload_slice();
        let mut plan = RetransmitPlan {
            local_port: c.local_port,
            remote_port: c.remote_port,
            remote_ip: c.remote_ip,
            seq: segment.seq,
            ack: segment.ack,
            flags: segment.flags,
            rx_window: advertised_window_for_flags(c, segment.flags),
            options: options_for_flags(c, segment.flags),
            payload_storage: [0; SENT_PAYLOAD_CAPACITY],
            payload_len: bytes.len() as u16,
            token: segment.token,
            sent_tick: segment.sent_tick,
            retransmits: segment.retransmits,
        };
        plan.payload_storage[..bytes.len()].copy_from_slice(bytes);
        Some(plan)
    }
}
pub(super) unsafe fn outstanding_info(id: u32, kind: RetransmitKind) -> Option<OutstandingInfo> {
    unsafe {
        let c = &*by_id(id)?;
        let s = c.sent.oldest(kind)?;
        Some(OutstandingInfo {
            sent_tick: s.sent_tick,
            retransmits: s.retransmits,
        })
    }
}
pub(super) unsafe fn outstanding_count(id: u32) -> usize {
    unsafe { by_id(id).map_or(0, |c| (*c).sent.count()) }
}
pub(super) unsafe fn outstanding_bytes(id: u32) -> usize {
    unsafe { by_id(id).map_or(0, |c| (*c).sent.outstanding_bytes()) }
}
unsafe fn remember_sent(c: &mut Connection, flags: u16, bytes: &[u8], tick: u64) -> bool {
    if !tcp_runtime::needs_tracking(flags, bytes.len()) {
        return true;
    }
    if c.sent.track(c.seq, c.ack, flags, bytes, tick).is_none() {
        return false;
    }
    unsafe { note_catalog_added(bytes.len()) };
    c.last_seq = c.seq;
    c.last_ack = c.ack;
    c.last_flags = flags;
    c.last_len = bytes.len();
    true
}
pub(super) unsafe fn commit_sent(id: u32, flags: u16, bytes: &[u8], tick: u64) -> bool {
    unsafe {
        let Some(c) = by_id(id) else {
            return false;
        };
        let c = &mut *c;
        if !remember_sent(c, flags, bytes, tick) {
            return false;
        }
        if flags & FLAG_SYN != 0 {
            if flags & FLAG_ACK != 0 {
                (*stats()).synack_tx += 1;
                (*stats()).last_error = text(b"synack-tx");
            } else {
                (*stats()).syn_tx += 1;
                (*stats()).last_error = text(b"syn");
            }
            return true;
        }
        if flags & FLAG_ACK != 0 {
            (*stats()).ack_tx += 1;
            if bytes.is_empty() && flags & (FLAG_SYN | FLAG_FIN | FLAG_RST) == 0 {
                (*stats()).pure_ack_tx += 1;
            }
        }
        if !bytes.is_empty() {
            c.seq = c
                .seq
                .wrapping_add(u32::try_from(bytes.len()).expect("TCP payload"));
            c.tx_window = c.tx_window.saturating_sub(bytes.len() as u32);
            c.tx_bytes += bytes.len() as u64;
            (*stats()).data_tx += 1;
            (*stats()).last_error = text(b"data");
        }
        if flags & FLAG_FIN != 0 {
            c.seq = c.seq.wrapping_add(1);
            c.close_requested = false;
            c.state = State::FinWait;
            (*stats()).fin_tx += 1;
            (*stats()).last_error = text(b"fin");
        }
        true
    }
}
pub(super) unsafe fn record_retransmit(id: u32, token: u64, tick: u64, reason: Text) -> bool {
    unsafe {
        let Some(c) = by_id(id) else {
            return false;
        };
        let c = &mut *c;
        if !c.sent.mark_retransmitted(token, tick) {
            return false;
        }
        c.retransmits += 1;
        (*stats()).retransmits += 1;
        (*stats()).last_error = reason;
        true
    }
}
pub(super) unsafe fn tx_ack_of(id: u32) -> u32 {
    unsafe { by_id(id).map_or(0, |c| (*c).tx_ack) }
}
pub(super) unsafe fn rx_ack_of(id: u32) -> u32 {
    unsafe { by_id(id).map_or(0, |c| (*c).ack) }
}
pub(super) unsafe fn request_ack(
    id: u32,
    previous: u32,
    view: SegmentView,
    now: u64,
    delay: u64,
) -> AckRequest {
    unsafe {
        let Some(c) = by_id(id) else {
            return AckRequest::None;
        };
        let c = &mut *c;
        if view.payload.len == 0 && view.flags & FLAG_FIN == 0 {
            return AckRequest::None;
        }
        let immediate = view.flags & FLAG_FIN != 0
            || c.ack == previous
            || view.seq != previous
            || rx_free(c) == 0;
        if c.delayed_ack_pending {
            c.delayed_ack_segments = c.delayed_ack_segments.saturating_add(1);
            c.delayed_ack_immediate = true;
            c.delayed_ack_deadline = now;
            (*stats()).ack_coalesced = (*stats()).ack_coalesced.wrapping_add(1);
            return AckRequest::Immediate;
        }
        c.delayed_ack_pending = true;
        c.delayed_ack_segments = 1;
        c.delayed_ack_immediate = immediate;
        c.delayed_ack_deadline = if immediate {
            now
        } else {
            now.saturating_add(delay.max(1))
        };
        if immediate {
            return AckRequest::Immediate;
        }
        (*stats()).delayed_ack_requests = (*stats()).delayed_ack_requests.wrapping_add(1);
        AckRequest::Delayed
    }
}
pub(super) unsafe fn delayed_ack_due(slot: usize, now: u64) -> Option<u32> {
    if slot >= MAX_CONNECTIONS {
        return None;
    }
    unsafe {
        let c = &*connection(slot);
        if !c.used
            || !c.delayed_ack_pending
            || (!c.delayed_ack_immediate
                && !tcp_runtime::deadline_reached(now, c.delayed_ack_deadline))
        {
            None
        } else {
            Some(c.id)
        }
    }
}
pub(super) unsafe fn note_ack_sent(id: u32, piggybacked: bool, window: u16) {
    unsafe {
        let Some(c) = by_id(id) else {
            return;
        };
        let c = &mut *c;
        c.last_advertised_window = window;
        if c.window_update_pending {
            c.window_update_pending = false;
            (*stats()).window_update_tx = (*stats()).window_update_tx.wrapping_add(1);
        }
        if !c.delayed_ack_pending {
            return;
        }
        if piggybacked {
            (*stats()).ack_piggybacked = (*stats()).ack_piggybacked.wrapping_add(1);
        } else if c.delayed_ack_immediate {
            (*stats()).immediate_ack_tx = (*stats()).immediate_ack_tx.wrapping_add(1);
        } else {
            (*stats()).delayed_ack_tx = (*stats()).delayed_ack_tx.wrapping_add(1);
        }
        c.delayed_ack_pending = false;
        c.delayed_ack_immediate = false;
        c.delayed_ack_segments = 0;
        c.delayed_ack_deadline = 0;
    }
}
pub(super) unsafe fn window_update_required(id: u32) -> bool {
    unsafe {
        let Some(c) = by_id(id) else {
            return false;
        };
        let c = &mut *c;
        if c.delayed_ack_pending {
            return true;
        }
        let current = advertised_window(c);
        if current <= c.last_advertised_window {
            return false;
        }
        let shift = if c.window_scale_active {
            LOCAL_WINDOW_SCALE
        } else {
            0
        };
        let wire_mss = 1.max(LOCAL_MSS >> shift);
        if c.last_advertised_window != 0 && current - c.last_advertised_window < wire_mss {
            return false;
        }
        c.window_update_pending = true;
        true
    }
}
pub(super) unsafe fn readable(id: u32) -> bool {
    unsafe { by_id(id).is_none_or(|c| (*c).rx_len != 0 || (*c).state == State::Closed) }
}
pub(super) unsafe fn closed(id: u32) -> bool {
    unsafe {
        by_id(id)
            .is_none_or(|c| matches!((*c).state, State::Closed | State::TimeWait | State::LastAck))
    }
}
pub(super) unsafe fn loopback_write(id: u32, bytes: *const u8, len: usize) -> i32 {
    unsafe { loopback_write_foreign(id, bytes, len) }
}
pub(super) unsafe fn read(id: u32, out: &mut [u8]) -> i32 {
    unsafe {
        let Some(c) = by_id(id) else {
            return -1;
        };
        let c = &mut *c;
        let len = out.len().min(c.rx_len);
        if len != 0 {
            let mut copied = 0;
            while copied < len {
                let run = (len - copied).min(c.rx.len() - c.rx_head);
                out[copied..copied + run].copy_from_slice(&c.rx[c.rx_head..c.rx_head + run]);
                c.rx_head = (c.rx_head + run) % c.rx.len();
                c.rx_len -= run;
                copied += run;
            }
            if c.rx_len == 0 {
                c.rx_head = 0;
            }
            flush_out_of_order_payload(c);
        } else if c.state == State::Closed {
            discard_catalog(c);
            c.used = false;
            return -1;
        }
        len as i32
    }
}
pub(super) unsafe fn close(id: u32) -> i32 {
    unsafe {
        let Some(c) = by_id(id) else {
            return -1;
        };
        let c = &mut *c;
        if c.state == State::Closed {
            discard_catalog(c);
            c.used = false;
            return 0;
        }
        if c.state != State::FinWait {
            c.state = State::FinWait;
            (*stats()).fin_tx += 1;
        }
        c.state = State::Closed;
        discard_catalog(c);
        c.used = false;
        0
    }
}

unsafe fn record_view(view: SegmentView) {
    unsafe {
        let s = &mut *stats();
        s.last_flags = view.flags;
        s.last_source_port = view.source_port;
        s.last_dest_port = view.dest_port;
        s.last_seq = view.seq;
        s.last_ack = view.ack;
        s.last_payload_len = u32::try_from(view.payload.len).expect("TCP receive payload");
        s.last_error = text(b"none");
    }
}
pub(super) unsafe fn apply_rx_view(view: SegmentView) -> Option<SegmentView> {
    unsafe {
        (*stats()).rx_segments += 1;
        record_view(view);
        if view.flags & FLAG_RST != 0 {
            (*stats()).rst_rx += 1;
            if let Some(c) = match_connection(view) {
                let c = &mut *c;
                if !reset_seq_acceptable(c, view) {
                    c.rx_drops += 1;
                    (*stats()).rx_drops += 1;
                    (*stats()).last_error = text(b"rst-stale");
                    return Some(view);
                }
                discard_catalog(c);
                c.state = State::Closed;
                c.used = false;
            }
            (*stats()).last_error = text(b"rst");
            return Some(view);
        }
        if let Some(c) = match_connection(view) {
            let c = &mut *c;
            if c.state == State::SynSent
                && view.flags & (FLAG_SYN | FLAG_ACK) == FLAG_SYN | FLAG_ACK
                && view.ack == c.seq.wrapping_add(1)
            {
                apply_peer_handshake_options(c, view);
                c.seq = c.seq.wrapping_add(1);
                c.ack = view.seq.wrapping_add(1);
                c.state = State::Established;
                update_tx_window(c, view);
                count_negotiation(c);
                (*stats()).synack_rx += 1;
                (*stats()).last_error = text(b"synack");
            } else if c.state == State::SynReceived
                && view.flags & FLAG_ACK != 0
                && view.ack == c.seq.wrapping_add(1)
            {
                c.seq = c.seq.wrapping_add(1);
                c.state = State::Established;
                update_tx_window(c, view);
                count_negotiation(c);
                (*stats()).accepts += 1;
                (*stats()).last_error = text(b"accepted");
            } else if c.state == State::Established {
                if (view.payload.len != 0 || view.flags & FLAG_FIN != 0)
                    && !segment_seq_acceptable(c, view)
                {
                    c.rx_drops += 1;
                    (*stats()).rx_drops += 1;
                    (*stats()).last_error = text(b"seq-stale");
                    return Some(view);
                }
                update_tx_window(c, view);
                if view.payload.len != 0 {
                    accept_payload(c, view.seq, payload(view.payload));
                }
                // FIN consumes only the next exact in-order sequence number. The
                // original drain-then-close userland contract is retained.
                if view.flags & FLAG_FIN != 0
                    && view.seq.wrapping_add(view.payload.len as u32) == c.ack
                {
                    c.ack = c.ack.wrapping_add(1);
                    c.state = State::Closed;
                    (*stats()).last_error = text(b"fin-rx");
                }
            } else if c.state == State::FinWait && view.flags & FLAG_ACK != 0 {
                update_tx_window(c, view);
                if view.flags & FLAG_FIN != 0 && view.seq == c.ack {
                    c.ack = c.ack.wrapping_add(1);
                }
                c.state = State::Closed;
                if view.flags & FLAG_FIN == 0 {
                    discard_catalog(c);
                    c.used = false;
                }
                (*stats()).last_error = text(b"closed");
            }
        }
        Some(view)
    }
}
pub(super) unsafe fn apply_tx_view(view: SegmentView) -> Option<SegmentView> {
    unsafe {
        record_view(view);
        (*stats()).tx_segments += 1;
        Some(view)
    }
}
unsafe fn accept_payload(c: &mut Connection, seq: u32, bytes: &[u8]) {
    unsafe {
        if bytes.is_empty() {
            return;
        }
        if seq == c.ack {
            accept_in_order_payload(c, seq, bytes);
            flush_out_of_order_payload(c);
            return;
        }
        if tcp_runtime::seq_after(seq, c.ack) {
            store_out_of_order_payload(c, seq, bytes);
            return;
        }
        let overlap = c.ack.wrapping_sub(seq) as usize;
        if overlap < bytes.len() {
            accept_in_order_payload(c, c.ack, &bytes[overlap..]);
            flush_out_of_order_payload(c);
        } else {
            (*stats()).last_error = text(b"seq-dup");
        }
    }
}
unsafe fn accept_in_order_payload(c: &mut Connection, seq: u32, bytes: &[u8]) {
    unsafe {
        let copied = write_rx(c, bytes);
        if copied < bytes.len() {
            let dropped = bytes.len() - copied;
            c.rx_drops += u32::try_from(dropped).expect("TCP drops");
            (*stats()).rx_drops += dropped as u64;
        }
        c.rx_bytes += copied as u64;
        c.ack = seq.wrapping_add(copied as u32);
        if copied != 0 {
            (*stats()).data_rx += 1;
            (*stats()).last_error = text(if copied == bytes.len() {
                b"data-rx"
            } else {
                b"rx-full"
            });
        } else {
            (*stats()).last_error = text(b"rx-full");
        }
    }
}
unsafe fn store_out_of_order_payload(c: &mut Connection, seq: u32, bytes: &[u8]) {
    unsafe {
        if bytes.len() > OUT_OF_ORDER_SLOT_SIZE {
            c.rx_drops += u32::try_from(bytes.len()).expect("TCP large OOO");
            (*stats()).rx_drops += bytes.len() as u64;
            (*stats()).last_error = text(b"ooo-large");
            return;
        }
        let end = seq.wrapping_add(bytes.len() as u32);
        if !tcp_runtime::seq_after(end, c.ack) {
            (*stats()).last_error = text(b"seq-dup");
            return;
        }
        if let Some(index) = find_out_of_order_covering(c, seq, bytes.len()) {
            let slot = &mut c.out_of_order[index];
            if bytes.len() > slot.len {
                slot.payload[..bytes.len()].copy_from_slice(bytes);
                slot.len = bytes.len();
            }
            (*stats()).last_error = text(b"ooo-dup");
            return;
        }
        let Some(index) = find_out_of_order_free(c)
            .or_else(|| select_out_of_order_replacement(c, seq, bytes.len()))
        else {
            c.rx_drops += bytes.len() as u32;
            (*stats()).rx_drops += bytes.len() as u64;
            (*stats()).last_error = text(b"ooo-full");
            return;
        };
        let slot = &mut c.out_of_order[index];
        if slot.valid {
            c.rx_drops += slot.len as u32;
            (*stats()).rx_drops += slot.len as u64;
        }
        slot.payload[..bytes.len()].copy_from_slice(bytes);
        slot.seq = seq;
        slot.len = bytes.len();
        slot.valid = true;
        (*stats()).last_error = text(b"ooo-store");
    }
}
unsafe fn flush_out_of_order_payload(c: &mut Connection) {
    unsafe {
        while let Some(index) = find_flushable_out_of_order(c) {
            let offset = c.ack.wrapping_sub(c.out_of_order[index].seq) as usize;
            if offset >= c.out_of_order[index].len {
                c.out_of_order[index] = OutOfOrderSegment::EMPTY;
                continue;
            }
            let remaining = c.out_of_order[index].len - offset;
            if rx_free(c) < remaining {
                (*stats()).last_error = text(b"ooo-wait");
                return;
            }
            let mut bytes = [0u8; OUT_OF_ORDER_SLOT_SIZE];
            bytes[..remaining]
                .copy_from_slice(&c.out_of_order[index].payload[offset..c.out_of_order[index].len]);
            let seq = c.ack;
            c.out_of_order[index] = OutOfOrderSegment::EMPTY;
            accept_in_order_payload(c, seq, &bytes[..remaining]);
            if (*stats()).last_error.len == 7 && payload((*stats()).last_error) == b"data-rx" {
                (*stats()).last_error = text(b"ooo-flush");
            }
        }
    }
}
fn find_out_of_order_free(c: &Connection) -> Option<usize> {
    c.out_of_order.iter().position(|slot| !slot.valid)
}
fn find_out_of_order_covering(c: &Connection, seq: u32, len: usize) -> Option<usize> {
    let end = seq.wrapping_add(len as u32);
    c.out_of_order.iter().position(|slot| {
        if !slot.valid {
            return false;
        }
        let slot_end = slot.seq.wrapping_add(slot.len as u32);
        let seq_covered = slot.seq == seq
            || (!tcp_runtime::seq_after(slot.seq, seq) && tcp_runtime::seq_after(slot_end, seq));
        let end_covered = slot_end == end || tcp_runtime::seq_after(slot_end, end);
        seq_covered && end_covered
    })
}
fn select_out_of_order_replacement(c: &Connection, seq: u32, _len: usize) -> Option<usize> {
    let distance = seq.wrapping_sub(c.ack);
    let mut farthest = None;
    let mut farthest_distance = 0;
    for (index, slot) in c.out_of_order.iter().enumerate() {
        if !slot.valid {
            continue;
        }
        let current = slot.seq.wrapping_sub(c.ack);
        if farthest.is_none() || current > farthest_distance {
            farthest = Some(index);
            farthest_distance = current;
        }
    }
    if distance < farthest_distance {
        farthest
    } else {
        None
    }
}
fn find_flushable_out_of_order(c: &Connection) -> Option<usize> {
    let mut best = None;
    let mut best_offset = 0;
    for (index, slot) in c.out_of_order.iter().enumerate() {
        if !slot.valid {
            continue;
        }
        let end = slot.seq.wrapping_add(slot.len as u32);
        let at_ack = slot.seq == c.ack;
        let overlaps =
            !tcp_runtime::seq_after(slot.seq, c.ack) && tcp_runtime::seq_after(end, c.ack);
        if !at_ack && !overlaps {
            continue;
        }
        let offset = c.ack.wrapping_sub(slot.seq);
        if best.is_none() || offset < best_offset {
            best = Some(index);
            best_offset = offset;
        }
    }
    best
}
fn out_of_order_count(c: &Connection) -> usize {
    c.out_of_order.iter().filter(|slot| slot.valid).count()
}
unsafe fn update_tx_window(c: &mut Connection, view: SegmentView) {
    unsafe {
        let shift = if c.window_scale_active && view.flags & FLAG_SYN == 0 {
            c.peer_window_scale.min(MAX_WINDOW_SCALE)
        } else {
            0
        };
        let advertised = u32::from(view.window) << shift;
        if view.flags & FLAG_ACK == 0 {
            c.tx_window = advertised;
            return;
        }
        if c.tx_ack == 0 || tcp_runtime::seq_after(view.ack, c.tx_ack) {
            c.tx_ack = view.ack;
        }
        let segments = c.sent.count();
        let bytes = c.sent.outstanding_bytes();
        c.sent.acknowledge(view.ack);
        note_catalog_reduced(segments, bytes, c.sent.count(), c.sent.outstanding_bytes());
        let right_edge = view.ack.wrapping_add(advertised);
        c.tx_window = if tcp_runtime::seq_after(right_edge, c.seq) {
            right_edge.wrapping_sub(c.seq)
        } else {
            0
        };
    }
}
fn write_rx(c: &mut Connection, bytes: &[u8]) -> usize {
    let len = bytes.len().min(rx_free(c));
    let mut copied = 0;
    while copied < len {
        let tail = (c.rx_head + c.rx_len) % c.rx.len();
        let run = (len - copied).min(c.rx.len() - tail);
        c.rx[tail..tail + run].copy_from_slice(&bytes[copied..copied + run]);
        c.rx_len += run;
        copied += run;
    }
    len
}
fn rx_free(c: &Connection) -> usize {
    c.rx.len() - c.rx_len
}
fn reset_seq_acceptable(c: &Connection, view: SegmentView) -> bool {
    match c.state {
        State::SynSent => view.flags & FLAG_ACK != 0 && view.ack == c.seq.wrapping_add(1),
        State::SynReceived
        | State::Established
        | State::FinWait
        | State::LastAck
        | State::TimeWait => segment_seq_acceptable(c, view),
        State::Closed => false,
    }
}
fn segment_seq_acceptable(c: &Connection, view: SegmentView) -> bool {
    if view.seq == c.ack {
        return true;
    }
    if view.payload.len != 0 {
        let end = view.seq.wrapping_add(view.payload.len as u32);
        if !tcp_runtime::seq_after(view.seq, c.ack) && tcp_runtime::seq_after(end, c.ack) {
            return true;
        }
    }
    view.seq.wrapping_sub(c.ack) < rx_free(c).max(1) as u32
}
pub(super) unsafe fn reap_time_wait(now: u64) -> u32 {
    unsafe {
        let mut freed = 0;
        for slot in 0..MAX_CONNECTIONS {
            let c = &mut *connection(slot);
            if !c.used {
                continue;
            }
            if c.state == State::Closed && c.rx_len == 0 {
                if c.time_wait_until == 0 {
                    c.time_wait_until = now.wrapping_add(TIME_WAIT_TICKS);
                } else if tcp_runtime::deadline_reached(now, c.time_wait_until) {
                    reset_connection_slot(c);
                    freed += 1;
                }
                continue;
            }
            if c.state != State::FinWait && !(c.state == State::Established && c.close_requested) {
                continue;
            }
            if c.time_wait_until == 0 {
                c.time_wait_until = now.wrapping_add(TIME_WAIT_TICKS);
                continue;
            }
            if tcp_runtime::deadline_reached(now, c.time_wait_until) {
                reset_connection_slot(c);
                freed += 1;
            }
        }
        freed
    }
}
fn advertised_window(c: &Connection) -> u16 {
    let free = rx_free(c);
    let scaled = if c.window_scale_active {
        free >> LOCAL_WINDOW_SCALE
    } else {
        free
    };
    scaled.min(usize::from(MAX_ADVERTISED_WINDOW)) as u16
}
fn advertised_window_for_flags(c: &Connection, flags: u16) -> u16 {
    if flags & FLAG_SYN != 0 {
        rx_free(c).min(usize::from(MAX_ADVERTISED_WINDOW)) as u16
    } else {
        advertised_window(c)
    }
}
fn sanitized_peer_mss(value: u16) -> u16 {
    if value == 0 {
        DEFAULT_PEER_MSS
    } else {
        LOCAL_MSS.min(value)
    }
}
fn apply_peer_handshake_options(c: &mut Connection, view: SegmentView) {
    c.peer_mss = sanitized_peer_mss(view.mss);
    c.window_scale_received = view.window_scale_present;
    c.peer_window_scale = if view.window_scale_present {
        view.window_scale.min(MAX_WINDOW_SCALE)
    } else {
        0
    };
    c.window_scale_active = c.window_scale_offered && c.window_scale_received;
}
unsafe fn count_negotiation(c: &mut Connection) {
    if c.negotiation_counted {
        return;
    }
    c.negotiation_counted = true;
    unsafe {
        if c.peer_mss != DEFAULT_PEER_MSS {
            (*stats()).mss_negotiated += 1;
        }
        if c.window_scale_active {
            (*stats()).window_scale_negotiated += 1;
        }
    }
}
fn options_for_flags(c: &Connection, flags: u16) -> u32 {
    if flags & FLAG_SYN == 0 {
        0
    } else {
        option_metadata(
            LOCAL_MSS,
            if c.window_scale_offered {
                Some(LOCAL_WINDOW_SCALE)
            } else {
                None
            },
        )
    }
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
unsafe fn probe_connection(remote: [u8; 4], port: u16) -> Option<(ProbeEnd, *mut Connection)> {
    unsafe {
        let result = self_test_connect(remote, port);
        if result <= 0 {
            return None;
        }
        let id = result as u32;
        let end = ProbeEnd { id, abort: false };
        let c = by_id(id)?;
        Some((end, c))
    }
}
unsafe fn probe_view(
    remote: [u8; 4],
    c: *const Connection,
    seq: u32,
    ack: u32,
    flags: u16,
    bytes: &[u8],
) -> SegmentView {
    unsafe {
        SegmentView {
            source_ip: remote,
            dest_ip: [192, 0, 2, 2],
            source_port: (*c).remote_port,
            dest_port: (*c).local_port,
            seq,
            ack,
            flags,
            window: MAX_ADVERTISED_WINDOW,
            payload: Text {
                pointer: bytes.as_ptr(),
                len: bytes.len(),
            },
            ..SegmentView::EMPTY
        }
    }
}
pub(super) unsafe fn buffer_limit_probe() -> bool {
    unsafe {
        let remote = [198, 51, 100, 90];
        let before = (*stats()).rx_drops;
        let Some((_end, c)) = probe_connection(remote, 65090) else {
            return false;
        };
        let chunk = [0x51; 512];
        let mut filled = 0;
        while filled < BUFFER_SIZE {
            let want = chunk.len().min(BUFFER_SIZE - filled);
            let copied = write_rx(&mut *c, &chunk[..want]);
            if copied != want {
                return false;
            }
            filled += copied;
        }
        if rx_free(&*c) != 0 {
            return false;
        }
        let view = probe_view(remote, c, (*c).ack, (*c).seq, FLAG_ACK | FLAG_PSH, b"X");
        if apply_rx_view(view).is_none() {
            return false;
        }
        (*stats()).rx_drops == before + 1
            && (*c).rx_drops == 1
            && payload((*stats()).last_error) == b"rx-full"
    }
}
unsafe fn small_out_of_order_probe() -> bool {
    unsafe {
        let remote = [198, 51, 100, 91];
        let before = (*stats()).rx_drops;
        let Some((end, c)) = probe_connection(remote, 65091) else {
            return false;
        };
        let base = (*c).ack;
        let third = probe_view(remote, c, base + 6, (*c).seq, FLAG_ACK | FLAG_PSH, b"GHI");
        if apply_rx_view(third).is_none() {
            return false;
        }
        if out_of_order_count(&*c) != 1 || (*c).rx_len != 0 || (*c).ack != base {
            return false;
        }
        let second = probe_view(remote, c, base + 3, (*c).seq, FLAG_ACK | FLAG_PSH, b"DEF");
        if apply_rx_view(second).is_none() {
            return false;
        }
        if out_of_order_count(&*c) != 2 || (*c).rx_len != 0 || (*c).ack != base {
            return false;
        }
        let first = probe_view(remote, c, base, (*c).seq, FLAG_ACK | FLAG_PSH, b"ABC");
        if apply_rx_view(first).is_none() {
            return false;
        }
        let mut out = [0; 9];
        let got = read(end.id, &mut out);
        (*stats()).self_tests += 1;
        got == 9
            && out == *b"ABCDEFGHI"
            && out_of_order_count(&*c) == 0
            && (*stats()).rx_drops == before
    }
}
fn fill_out_of_order_probe_payload(out: &mut [u8], segment: usize) {
    for (index, byte) in out.iter_mut().enumerate() {
        *byte = ((segment * 17 + index) & 255) as u8;
    }
}
fn verify_out_of_order_probe_payload(bytes: &[u8], segment: usize) -> bool {
    bytes
        .iter()
        .enumerate()
        .all(|(index, &byte)| byte == ((segment * 17 + index) & 255) as u8)
}
unsafe fn burst_out_of_order_probe() -> bool {
    unsafe {
        const COUNT: usize = 40;
        const SIZE: usize = 1024;
        let remote = [198, 51, 100, 92];
        let before = (*stats()).rx_drops;
        let Some((end, c)) = probe_connection(remote, 65092) else {
            return false;
        };
        let base = (*c).ack;
        let mut bytes = [0; SIZE];
        let mut index = COUNT;
        while index > 1 {
            index -= 1;
            fill_out_of_order_probe_payload(&mut bytes, index);
            let view = probe_view(
                remote,
                c,
                base + (index * SIZE) as u32,
                (*c).seq,
                FLAG_ACK | FLAG_PSH,
                &bytes,
            );
            if apply_rx_view(view).is_none() {
                return false;
            }
        }
        if out_of_order_count(&*c) != COUNT - 1 || (*c).rx_len != 0 || (*c).ack != base {
            return false;
        }
        fill_out_of_order_probe_payload(&mut bytes, 0);
        let view = probe_view(remote, c, base, (*c).seq, FLAG_ACK | FLAG_PSH, &bytes);
        if apply_rx_view(view).is_none() {
            return false;
        }
        if out_of_order_count(&*c) != 0 || (*c).rx_len != COUNT * SIZE {
            return false;
        }
        let mut out = [0; SIZE];
        for index in 0..COUNT {
            if read(end.id, &mut out) != SIZE as i32
                || !verify_out_of_order_probe_payload(&out, index)
            {
                return false;
            }
        }
        (*stats()).self_tests += 1;
        out_of_order_count(&*c) == 0 && (*c).rx_len == 0 && (*stats()).rx_drops == before
    }
}
pub(super) unsafe fn out_of_order_probe() -> bool {
    unsafe { small_out_of_order_probe() && burst_out_of_order_probe() }
}
pub(super) unsafe fn overlap_retransmit_probe() -> bool {
    unsafe {
        let remote = [198, 51, 100, 92];
        let Some((_end, c)) = probe_connection(remote, 65092) else {
            return false;
        };
        let base = (*c).ack;
        let first = probe_view(remote, c, base, (*c).seq, FLAG_ACK | FLAG_PSH, b"ABCDE");
        if apply_rx_view(first).is_none() || (*c).ack != base.wrapping_add(5) {
            return false;
        }
        let before = (*c).rx_bytes;
        let overlap = probe_view(remote, c, base, (*c).seq, FLAG_ACK | FLAG_PSH, b"ABCDEFGH");
        if apply_rx_view(overlap).is_none() {
            return false;
        }
        (*c).ack == base.wrapping_add(8) && (*c).rx_bytes == before + 3
    }
}
pub(super) unsafe fn wraparound_probe() -> bool {
    unsafe {
        let remote = [198, 51, 100, 93];
        let Some((_end, c)) = probe_connection(remote, 65093) else {
            return false;
        };
        (*c).ack = 0xffff_fffe;
        let before = (*c).rx_bytes;
        let first = probe_view(remote, c, 0xffff_fffe, (*c).seq, FLAG_ACK | FLAG_PSH, b"AB");
        if apply_rx_view(first).is_none() || (*c).ack != 0 {
            return false;
        }
        let second = probe_view(remote, c, 0, (*c).seq, FLAG_ACK | FLAG_PSH, b"CD");
        if apply_rx_view(second).is_none() || (*c).ack != 2 || (*c).rx_bytes != before + 4 {
            return false;
        }
        let fin = probe_view(remote, c, 2, (*c).seq, FLAG_ACK | FLAG_FIN, b"");
        if apply_rx_view(fin).is_none() {
            return false;
        }
        (*c).ack == 3 && (*c).state == State::Closed
    }
}
pub(super) unsafe fn graceful_close_probe() -> bool {
    unsafe {
        let remote = [198, 51, 100, 94];
        let Some((end, c)) = probe_connection(remote, 65094) else {
            return false;
        };
        let id = end.id;
        if !commit_sent(id, FLAG_ACK | FLAG_PSH, b"last", 1)
            || !request_close(id)
            || close_ready(id)
            || send_allowance(id, 1) != 0
        {
            return false;
        }
        let ack = probe_view(remote, c, (*c).ack, (*c).seq, FLAG_ACK, b"");
        if apply_rx_view(ack).is_none() || !close_ready(id) {
            return false;
        }
        if !commit_sent(id, FLAG_ACK | FLAG_FIN, b"", 2) || !fin_waiting(id) || close_ready(id) {
            return false;
        }
        let ack = probe_view(remote, c, (*c).ack, (*c).seq, FLAG_ACK, b"");
        if apply_rx_view(ack).is_none() {
            return false;
        }
        connection_identity(id).is_none()
    }
}
pub(super) unsafe fn performance_contract_probe() -> bool {
    unsafe {
        let remote = [198, 51, 100, 95];
        let initial = 0x6000u32;
        let remote_seq = 0x7000;
        let before_mss = (*stats()).mss_negotiated;
        let before_scale = (*stats()).window_scale_negotiated;
        let before_coalesced = (*stats()).ack_coalesced;
        let result = begin_live_connect(remote, 65095, initial);
        if result <= 0 {
            return false;
        }
        let id = result as u32;
        let _end = ProbeEnd { id, abort: true };
        let Some(syn) = plan_send(id, FLAG_SYN) else {
            return false;
        };
        if syn.rx_window != MAX_ADVERTISED_WINDOW
            || option_mss(syn.options) != LOCAL_MSS
            || option_window_scale(syn.options) != Some(LOCAL_WINDOW_SCALE)
        {
            return false;
        }
        if !commit_sent(id, FLAG_SYN, b"", 1) {
            return false;
        }
        let Some(c) = by_id(id) else {
            return false;
        };
        let mut syn_ack = probe_view(
            remote,
            c,
            remote_seq,
            initial.wrapping_add(1),
            FLAG_SYN | FLAG_ACK,
            b"",
        );
        syn_ack.mss = LOCAL_MSS;
        syn_ack.window_scale = 3;
        syn_ack.window_scale_present = true;
        if apply_rx_view(syn_ack).is_none()
            || !established(id)
            || (*c).peer_mss != LOCAL_MSS
            || !(*c).window_scale_active
        {
            return false;
        }
        if (*stats()).mss_negotiated != before_mss.wrapping_add(1)
            || (*stats()).window_scale_negotiated != before_scale.wrapping_add(1)
            || sanitized_peer_mss(0) != DEFAULT_PEER_MSS
            || sanitized_peer_mss(4096) != LOCAL_MSS
        {
            return false;
        }
        const REQUESTED: usize = 4068;
        let bytes = [0x5a; LOCAL_MSS as usize];
        let mut written = 0;
        let mut segments = 0;
        while written < REQUESTED {
            let allowed = send_allowance(id, REQUESTED - written);
            if allowed == 0 {
                return false;
            }
            let flags = FLAG_ACK
                | if written + allowed == REQUESTED {
                    FLAG_PSH
                } else {
                    0
                };
            if !commit_sent(id, flags, &bytes[..allowed], 2 + segments) {
                return false;
            }
            written += allowed;
            segments += 1;
        }
        if segments != 3 || outstanding_count(id) != 3 || outstanding_bytes(id) != REQUESTED {
            return false;
        }
        let partial = initial
            .wrapping_add(1)
            .wrapping_add(u32::from(LOCAL_MSS))
            .wrapping_add(100);
        let ack = probe_view(remote, c, (*c).ack, partial, FLAG_ACK, b"");
        if apply_rx_view(ack).is_none() {
            return false;
        }
        let Some(retransmit) = plan_retransmit(id, RetransmitKind::Data) else {
            return false;
        };
        if outstanding_count(id) != 2
            || outstanding_bytes(id) != REQUESTED - usize::from(LOCAL_MSS) - 100
            || retransmit.seq != partial
        {
            return false;
        }
        let mut zero = ack;
        zero.window = 0;
        if apply_rx_view(zero).is_none()
            || send_allowance(id, 1) != 0
            || write_block_reason(id) != WriteBlockReason::RemoteWindow
        {
            return false;
        }
        let previous = (*c).ack;
        let first = probe_view(remote, c, previous, partial, FLAG_ACK | FLAG_PSH, b"A");
        if apply_rx_view(first).is_none()
            || request_ack(id, previous, first, 100, 5) != AckRequest::Delayed
        {
            return false;
        }
        let Some(identity) = connection_identity(id) else {
            return false;
        };
        if delayed_ack_due(identity.slot, 104).is_some() {
            return false;
        }
        let previous = (*c).ack;
        let mut second = first;
        second.seq = previous;
        second.payload = Text::literal(b"B");
        if apply_rx_view(second).is_none()
            || request_ack(id, previous, second, 101, 5) != AckRequest::Immediate
        {
            return false;
        }
        if delayed_ack_due(identity.slot, 101) != Some(id)
            || (*stats()).ack_coalesced != before_coalesced.wrapping_add(1)
        {
            return false;
        }
        let Some(plan) = plan_send(id, FLAG_ACK) else {
            return false;
        };
        note_ack_sent(id, false, plan.rx_window);
        if delayed_ack_due(identity.slot, 200).is_some() {
            return false;
        }
        (*stats()).self_tests = (*stats()).self_tests.wrapping_add(1);
        true
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

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_connection_slot_bytes() -> usize {
    core::mem::size_of::<Connection>()
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_option_metadata(mss: u16, window_scale: u8) -> u32 {
    option_metadata(
        mss,
        if window_scale == 255 {
            None
        } else {
            Some(window_scale)
        },
    )
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_option_mss(metadata: u32) -> u16 {
    option_mss(metadata)
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_option_window_scale(
    metadata: u32,
    result: *mut u8,
) -> bool {
    unsafe {
        if let Some(value) = option_window_scale(metadata) {
            result.write(value);
            true
        } else {
            false
        }
    }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_reset() {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return;
    };
    unsafe { reset() }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_get_stats(result: *mut Stats) {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        unsafe {
            result.write(Stats {
                last_error: Text::literal(b"cancelled"),
                ..Stats::EMPTY
            })
        };
        return;
    };
    unsafe { result.write(stats().read()) }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_record_write_start(requested: usize) {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return;
    };
    unsafe { record_write_start(requested) }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_record_write_finish(
    requested: usize,
    written: usize,
    segments: usize,
    block: u8,
) {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return;
    };
    unsafe {
        record_write_finish(
            requested,
            written,
            segments,
            match block {
                0 => WriteBlockReason::None,
                1 => WriteBlockReason::RemoteWindow,
                2 => WriteBlockReason::Catalog,
                3 => WriteBlockReason::State,
                _ => panic!("TCP typed discriminant"),
            },
        )
    }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_record_backend_busy() {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return;
    };
    unsafe { record_backend_busy() }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_outstanding_totals(
    segment_count: *mut u32,
    byte_count: *mut u32,
) {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        unsafe {
            segment_count.write(0);
            byte_count.write(0)
        };
        return;
    };
    unsafe {
        segment_count.write((*stats()).outstanding_segments_current);
        byte_count.write((*stats()).outstanding_bytes_current)
    }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_summary(out: *mut Summary) {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        unsafe {
            out.write(Summary {
                max_connections: MAX_CONNECTIONS as u32,
                buffer_size: BUFFER_SIZE as u32,
                ..Summary::default()
            })
        };
        return;
    };
    unsafe { out.write(summary()) }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_connection_info(
    index: u32,
    out: *mut ConnectionInfo,
) -> i32 {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return 0;
    };
    unsafe {
        if let Some(value) = connection_info(index) {
            out.write(value);
            1
        } else {
            0
        }
    }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_self_test_connect(
    remote_ip: *const [u8; 4],
    remote_port: u16,
) -> i32 {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return -1;
    };
    unsafe { self_test_connect(*remote_ip, remote_port) }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_begin_live_connect(
    remote_ip: *const [u8; 4],
    remote_port: u16,
    initial_seq: u32,
) -> i32 {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return -1;
    };
    unsafe { begin_live_connect(*remote_ip, remote_port, initial_seq) }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_listen(port: u16) -> bool {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe { listen(port) }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_close_listener(port: u16) {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return;
    };
    unsafe { close_listener(port) }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_has_listener(port: u16) -> bool {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe { has_listener(port) }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_accept_inbound(
    view: *const SegmentView,
    initial_seq: u32,
    result: *mut u32,
) -> bool {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe {
        if let Some(value) = accept_inbound(*view, initial_seq) {
            result.write(value);
            true
        } else {
            false
        }
    }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_abort(
    conn_id: u32,
    reason: *const u8,
    reason_len: usize,
) {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return;
    };
    unsafe {
        abort(
            conn_id,
            Text {
                pointer: reason,
                len: reason_len,
            },
        )
    }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_mark_timeout(reason: *const u8, reason_len: usize) {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return;
    };
    unsafe {
        mark_timeout(Text {
            pointer: reason,
            len: reason_len,
        })
    }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_record_malformed(reason: *const u8, reason_len: usize) {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return;
    };
    unsafe {
        record_malformed(Text {
            pointer: reason,
            len: reason_len,
        })
    }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_record_checksum_error() {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return;
    };
    unsafe { record_checksum_error() }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_set_error(reason: *const u8, reason_len: usize) {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return;
    };
    unsafe {
        set_error(Text {
            pointer: reason,
            len: reason_len,
        })
    }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_established(conn_id: u32) -> bool {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe { established(conn_id) }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_request_close(conn_id: u32) -> bool {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe { request_close(conn_id) }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_close_ready(conn_id: u32) -> bool {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe { close_ready(conn_id) }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_fin_waiting(conn_id: u32) -> bool {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe { fin_waiting(conn_id) }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_connection_identity(
    conn_id: u32,
    result: *mut ConnectionIdentity,
) -> bool {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe {
        if let Some(value) = connection_identity(conn_id) {
            result.write(value);
            true
        } else {
            false
        }
    }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_connection_identity_at(
    slot: usize,
    result: *mut ConnectionIdentity,
) -> bool {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe {
        if let Some(value) = connection_identity_at(slot) {
            result.write(value);
            true
        } else {
            false
        }
    }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_identity_active(
    identity: *const ConnectionIdentity,
) -> bool {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe { identity_active(*identity) }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_connection_with_data_on_port(
    port: u16,
    result: *mut u32,
) -> bool {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe {
        if let Some(value) = connection_with_data_on_port(port) {
            result.write(value);
            true
        } else {
            false
        }
    }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_connection_on_port(port: u16, result: *mut u32) -> bool {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe {
        if let Some(value) = connection_on_port(port) {
            result.write(value);
            true
        } else {
            false
        }
    }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_claim_accepted(conn_id: u32) -> bool {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe { claim_accepted(conn_id) }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_connection_id_for_segment(
    view: *const SegmentView,
    result: *mut u32,
) -> bool {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe {
        if let Some(value) = connection_id_for_segment(*view) {
            result.write(value);
            true
        } else {
            false
        }
    }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_connection_identity_for_segment(
    view: *const SegmentView,
    result: *mut ConnectionIdentity,
) -> bool {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe {
        if let Some(value) = connection_identity_for_segment(*view) {
            result.write(value);
            true
        } else {
            false
        }
    }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_abort_connections_on_port(
    port: u16,
    reason: *const u8,
    reason_len: usize,
) {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return;
    };
    unsafe {
        abort_connections_on_port(
            port,
            Text {
                pointer: reason,
                len: reason_len,
            },
        )
    }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_abort_all(
    reason: *const u8,
    reason_len: usize,
    result: *mut CleanupResult,
) {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        unsafe { result.write(CleanupResult::EMPTY) };
        return;
    };
    unsafe {
        result.write(abort_all(Text {
            pointer: reason,
            len: reason_len,
        }))
    }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_plan_send(
    conn_id: u32,
    flags: u16,
    result: *mut SendPlan,
) -> bool {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe {
        if let Some(value) = plan_send(conn_id, flags) {
            result.write(value);
            true
        } else {
            false
        }
    }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_send_allowance(conn_id: u32, requested: usize) -> usize {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return 0;
    };
    unsafe { send_allowance(conn_id, requested) }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_write_block_reason(conn_id: u32) -> u8 {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return 3;
    };
    unsafe { write_block_reason(conn_id) as u8 }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_can_track_send(
    conn_id: u32,
    flags: u16,
    payload_len: usize,
) -> bool {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe { can_track_send(conn_id, flags, payload_len) }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_plan_retransmit(
    conn_id: u32,
    kind: u8,
    result: *mut RetransmitPlan,
) -> bool {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe {
        if let Some(value) = plan_retransmit(
            conn_id,
            match kind {
                0 => RetransmitKind::Any,
                1 => RetransmitKind::Data,
                2 => RetransmitKind::Syn,
                3 => RetransmitKind::Fin,
                _ => panic!("TCP typed discriminant"),
            },
        ) {
            result.write(value);
            true
        } else {
            false
        }
    }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_outstanding_info(
    conn_id: u32,
    kind: u8,
    result: *mut OutstandingInfo,
) -> bool {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe {
        if let Some(value) = outstanding_info(
            conn_id,
            match kind {
                0 => RetransmitKind::Any,
                1 => RetransmitKind::Data,
                2 => RetransmitKind::Syn,
                3 => RetransmitKind::Fin,
                _ => panic!("TCP typed discriminant"),
            },
        ) {
            result.write(value);
            true
        } else {
            false
        }
    }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_outstanding_count(conn_id: u32) -> usize {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return 0;
    };
    unsafe { outstanding_count(conn_id) }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_outstanding_bytes(conn_id: u32) -> usize {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return 0;
    };
    unsafe { outstanding_bytes(conn_id) }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_commit_sent(
    conn_id: u32,
    flags: u16,
    payload: *const u8,
    payload_len: usize,
    sent_tick: u64,
) -> bool {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return false;
    };
    if payload_len > SENT_PAYLOAD_CAPACITY {
        return false;
    }
    let mut staged = [0u8; SENT_PAYLOAD_CAPACITY];
    if payload_len != 0 {
        unsafe { core::ptr::copy_nonoverlapping(payload, staged.as_mut_ptr(), payload_len) };
    }
    unsafe { commit_sent(conn_id, flags, &staged[..payload_len], sent_tick) }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_record_retransmit(
    conn_id: u32,
    token: u64,
    sent_tick: u64,
    reason: *const u8,
    reason_len: usize,
) -> bool {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe {
        record_retransmit(
            conn_id,
            token,
            sent_tick,
            Text {
                pointer: reason,
                len: reason_len,
            },
        )
    }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_tx_ack_of(conn_id: u32) -> u32 {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return 0;
    };
    unsafe { tx_ack_of(conn_id) }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_rx_ack_of(conn_id: u32) -> u32 {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return 0;
    };
    unsafe { rx_ack_of(conn_id) }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_request_ack(
    conn_id: u32,
    previous_ack: u32,
    view: *const SegmentView,
    now: u64,
    delay_ticks: u64,
) -> u8 {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return 0;
    };
    unsafe { request_ack(conn_id, previous_ack, *view, now, delay_ticks) as u8 }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_delayed_ack_due(
    slot: usize,
    now: u64,
    result: *mut u32,
) -> bool {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe {
        if let Some(value) = delayed_ack_due(slot, now) {
            result.write(value);
            true
        } else {
            false
        }
    }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_note_ack_sent(
    conn_id: u32,
    piggybacked: bool,
    advertised_window: u16,
) {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return;
    };
    unsafe { note_ack_sent(conn_id, piggybacked, advertised_window) }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_window_update_required(conn_id: u32) -> bool {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe { window_update_required(conn_id) }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_readable(conn_id: u32) -> bool {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe { readable(conn_id) }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_closed(conn_id: u32) -> bool {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe { closed(conn_id) }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_loopback_write(
    conn_id: u32,
    data: *const u8,
    data_len: usize,
) -> i32 {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return -1;
    };
    unsafe { loopback_write(conn_id, data, data_len) }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_read(conn_id: u32, out: *mut u8, out_len: usize) -> i32 {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return -1;
    };
    unsafe { read_foreign(conn_id, out, out_len) }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_close(conn_id: u32) -> i32 {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return -1;
    };
    unsafe { close(conn_id) }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_buffer_limit_probe() -> bool {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe { buffer_limit_probe() }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_out_of_order_probe() -> bool {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe { out_of_order_probe() }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_apply_rx_view(
    view: *const SegmentView,
    result: *mut SegmentView,
) -> bool {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe {
        if let Some(value) = apply_rx_view(*view) {
            result.write(value);
            true
        } else {
            false
        }
    }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_apply_tx_view(
    view: *const SegmentView,
    result: *mut SegmentView,
) -> bool {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe {
        if let Some(value) = apply_tx_view(*view) {
            result.write(value);
            true
        } else {
            false
        }
    }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_reap_time_wait(now: u64) -> u32 {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return 0;
    };
    unsafe { reap_time_wait(now) }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_overlap_retransmit_probe() -> bool {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe { overlap_retransmit_probe() }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_wraparound_probe() -> bool {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe { wraparound_probe() }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_graceful_close_probe() -> bool {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe { graceful_close_probe() }
}
/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_performance_contract_probe() -> bool {
    let Some(_guard) = super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe { performance_contract_probe() }
}

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

/// Caller retains exact TCP task admission; output may fault. Every state
/// borrow ends before copying to it. The original initial read limit fixes
/// the prefix, including when intermediate reads flush out-of-order data.
unsafe fn read_foreign(id: u32, out: *mut u8, capacity: usize) -> i32 {
    unsafe {
        let Some(c) = by_id(id) else {
            return -1;
        };
        let total = capacity.min((*c).rx_len);
        if total == 0 {
            return read(id, &mut []);
        }
        let mut staged = [0u8; 4096];
        let mut copied = 0;
        while copied < total {
            let run = (total - copied).min(staged.len());
            let got = read(id, &mut staged[..run]);
            assert_eq!(got, run as i32, "Exact admitted TCP read prefix");
            core::ptr::copy_nonoverlapping(staged.as_ptr(), out.add(copied), run);
            copied += run;
        }
        total as i32
    }
}
/// Caller retains exact TCP task admission. No resident-table reference is
/// live during a potentially faulting read from foreign caller bytes.
unsafe fn loopback_write_foreign(id: u32, data: *const u8, len: usize) -> i32 {
    unsafe {
        let Some(c) = by_id(id) else {
            return -1;
        };
        if (*c).state != State::Established || len > BUFFER_SIZE {
            return -1;
        }
        (*c).tx_bytes += len as u64;
        (*stats()).data_tx += 1;
        let total = len.min(rx_free(&*c));
        let mut copied = 0;
        let mut staged = [0u8; 4096];
        while copied < total {
            let run = (total - copied).min(staged.len());
            core::ptr::copy_nonoverlapping(data.add(copied), staged.as_mut_ptr(), run);
            let wrote = write_rx(&mut *c, &staged[..run]);
            assert_eq!(wrote, run, "Exact admitted TCP loopback prefix");
            copied += run;
        }
        (*c).rx_bytes += copied as u64;
        (*stats()).data_rx += 1;
        len as i32
    }
}
/// # Safety
/// Native TCP task owner is held and slot is in the fixed connection table.
/// This copied readiness projection is published before scheduler enrollment
/// or wake. A scheduler predicate must never acquire the waitable TCP owner.
pub(super) unsafe fn wait_observation_word(slot: usize) -> u64 {
    unsafe {
        let c = &*connection(slot);
        if !c.used {
            return 0;
        }
        let mut flags = 0u64;
        // The same owner-local state rules as established/readable/closed.
        if c.state == State::Established {
            flags |= 1;
        }
        if c.rx_len != 0 || c.state == State::Closed {
            flags |= 2;
        }
        if matches!(c.state, State::Closed | State::TimeWait | State::LastAck) {
            flags |= 4;
        }
        if c.state == State::Established && !c.accepted_claimed {
            flags |= 8;
            if c.rx_len != 0 {
                flags |= 16;
            }
        }
        u64::from(c.id) | (u64::from(c.local_port) << 32) | (flags << 48)
    }
}
