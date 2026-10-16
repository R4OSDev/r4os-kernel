//! Existing icmp protocol metadata and caller-owned statistics. Wire
//! encoding/checksums stay with the unchanged R4P implementation.
use crate::sched::sync_types::Text;
pub const IPV4_PROTOCOL: u8 = 1;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Stats {
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub echo_requests_rx: u64,
    pub echo_replies_rx: u64,
    pub echo_requests_tx: u64,
    pub echo_replies_tx: u64,
    pub destination_unreachable_rx: u64,
    pub port_unreachable_rx: u64,
    pub time_exceeded_rx: u64,
    pub malformed: u64,
    pub checksum_errors: u64,
    pub last_type: u8,
    pub last_code: u8,
    pub last_id: u16,
    pub last_seq: u16,
    pub last_error: Text,
}
impl Stats {
    pub const EMPTY: Self = Self {
        rx_packets: 0,
        tx_packets: 0,
        echo_requests_rx: 0,
        echo_replies_rx: 0,
        echo_requests_tx: 0,
        echo_replies_tx: 0,
        destination_unreachable_rx: 0,
        port_unreachable_rx: 0,
        time_exceeded_rx: 0,
        malformed: 0,
        checksum_errors: 0,
        last_type: 0,
        last_code: 0,
        last_id: 0,
        last_seq: 0,
        last_error: Text::literal(b"none"),
    };
}
/// # Safety
/// Exact exclusive paired statistics output; payload references never escape.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_icmp_reset(out: *mut Stats) {
    unsafe { out.write(Stats::EMPTY) };
}

const _: () = {
    assert!(core::mem::size_of::<Stats>() == 112);
    assert!(core::mem::align_of::<Stats>() == 8);
    assert!(core::mem::offset_of!(Stats, rx_packets) == 0);
    assert!(core::mem::offset_of!(Stats, tx_packets) == 8);
    assert!(core::mem::offset_of!(Stats, echo_requests_rx) == 16);
    assert!(core::mem::offset_of!(Stats, echo_replies_rx) == 24);
    assert!(core::mem::offset_of!(Stats, echo_requests_tx) == 32);
    assert!(core::mem::offset_of!(Stats, echo_replies_tx) == 40);
    assert!(core::mem::offset_of!(Stats, destination_unreachable_rx) == 48);
    assert!(core::mem::offset_of!(Stats, port_unreachable_rx) == 56);
    assert!(core::mem::offset_of!(Stats, time_exceeded_rx) == 64);
    assert!(core::mem::offset_of!(Stats, malformed) == 72);
    assert!(core::mem::offset_of!(Stats, checksum_errors) == 80);
    assert!(core::mem::offset_of!(Stats, last_type) == 88);
    assert!(core::mem::offset_of!(Stats, last_code) == 89);
    assert!(core::mem::offset_of!(Stats, last_id) == 90);
    assert!(core::mem::offset_of!(Stats, last_seq) == 92);
    assert!(core::mem::offset_of!(Stats, last_error) == 96);
};

use super::r4p_contract as wire;
fn err(s: &mut Stats, value: &'static [u8]) {
    s.last_error = Text::literal(value);
}

fn classify_rx(s: &mut Stats, typ: u8, code: u8) {
    match (typ, code) {
        (0, 0) | (8, 0) => err(s, b"none"),
        (3, 3) => {
            s.destination_unreachable_rx += 1;
            s.port_unreachable_rx += 1;
            err(s, b"port-unreach");
        }
        (3, _) => {
            s.destination_unreachable_rx += 1;
            err(s, b"dest-unreach");
        }
        (11, _) => {
            s.time_exceeded_rx += 1;
            err(s, b"time-exceeded");
        }
        _ => err(s, b"icmp-other"),
    }
}
pub fn apply_rx_result(s: &mut Stats, op: &wire::IcmpOp) {
    s.last_type = op.typ;
    s.last_code = op.code;
    s.last_id = op.ident;
    s.last_seq = op.seq;
    match op.result {
        wire::ICMP_RESULT_OK => {
            s.rx_packets += 1;
            if op.flags & wire::ICMP_FLAG_ECHO_REPLY != 0 {
                s.echo_replies_rx += 1;
            }
            if op.flags & wire::ICMP_FLAG_ECHO_REQUEST != 0 {
                s.echo_requests_rx += 1;
            }
            classify_rx(s, op.typ, op.code);
        }
        wire::ICMP_RESULT_SHORT => {
            s.malformed += 1;
            err(s, b"short");
        }
        wire::ICMP_RESULT_CHECKSUM => {
            s.checksum_errors += 1;
            err(s, b"checksum");
        }
        _ => err(s, b"not-icmp"),
    }
}
pub fn apply_tx_result(s: &mut Stats, op: &wire::IcmpOp) {
    s.last_type = op.typ;
    s.last_code = op.code;
    s.last_id = op.ident;
    s.last_seq = op.seq;
    match op.result {
        wire::ICMP_RESULT_OK => {
            s.tx_packets += 1;
            if op.flags & wire::ICMP_FLAG_ECHO_REQUEST != 0 {
                s.echo_requests_tx += 1;
            }
            if op.flags & wire::ICMP_FLAG_ECHO_REPLY != 0 {
                s.echo_replies_tx += 1;
            }
            err(s, b"none");
        }
        wire::ICMP_RESULT_SHORT => err(s, b"short"),
        wire::ICMP_RESULT_CHECKSUM => err(s, b"checksum"),
        _ => err(s, b"not-icmp"),
    }
}

/// # Safety
/// Exact exclusive paired statistics and readable canonical wire operation;
/// caller retains protocol admission. This applies metadata only, with no I/O.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_icmp_apply_rx_result(
    stats: *mut Stats,
    op: *const wire::IcmpOp,
) {
    unsafe { apply_rx_result(&mut *stats, &*op) }
}

/// # Safety
/// Exact exclusive paired statistics and readable canonical wire operation;
/// caller retains protocol admission. This applies metadata only, with no I/O.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_icmp_apply_tx_result(
    stats: *mut Stats,
    op: *const wire::IcmpOp,
) {
    unsafe { apply_tx_result(&mut *stats, &*op) }
}
