//! Existing udp protocol metadata and caller-owned statistics. Wire
//! encoding/checksums stay with the unchanged R4P implementation.
use crate::sched::sync_types::Text;
pub const IPV4_PROTOCOL: u8 = 17;
pub const HEADER_SIZE: usize = 8;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DatagramView {
    pub source_ip: [u8; 4],
    pub dest_ip: [u8; 4],
    pub source_port: u16,
    pub dest_port: u16,
    pub length: u16,
    pub payload: Text,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Stats {
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub dropped_short: u64,
    pub dropped_length: u64,
    pub checksum_errors: u64,
    pub malformed: u64,
    pub dhcp_rx: u64,
    pub dns_rx: u64,
    pub self_tests: u64,
    pub last_source_port: u16,
    pub last_dest_port: u16,
    pub last_error: Text,
}
impl Stats {
    pub const EMPTY: Self = Self {
        rx_packets: 0,
        tx_packets: 0,
        dropped_short: 0,
        dropped_length: 0,
        checksum_errors: 0,
        malformed: 0,
        dhcp_rx: 0,
        dns_rx: 0,
        self_tests: 0,
        last_source_port: 0,
        last_dest_port: 0,
        last_error: Text::literal(b"none"),
    };
}
/// # Safety
/// Exact exclusive paired statistics output; payload references never escape.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_udp_reset(out: *mut Stats) {
    unsafe { out.write(Stats::EMPTY) };
}

const _: () = {
    assert!(core::mem::size_of::<Stats>() == 96);
    assert!(core::mem::align_of::<Stats>() == 8);
    assert!(core::mem::offset_of!(Stats, rx_packets) == 0);
    assert!(core::mem::offset_of!(Stats, tx_packets) == 8);
    assert!(core::mem::offset_of!(Stats, dropped_short) == 16);
    assert!(core::mem::offset_of!(Stats, dropped_length) == 24);
    assert!(core::mem::offset_of!(Stats, checksum_errors) == 32);
    assert!(core::mem::offset_of!(Stats, malformed) == 40);
    assert!(core::mem::offset_of!(Stats, dhcp_rx) == 48);
    assert!(core::mem::offset_of!(Stats, dns_rx) == 56);
    assert!(core::mem::offset_of!(Stats, self_tests) == 64);
    assert!(core::mem::offset_of!(Stats, last_source_port) == 72);
    assert!(core::mem::offset_of!(Stats, last_dest_port) == 74);
    assert!(core::mem::offset_of!(Stats, last_error) == 80);
};

use super::r4p_contract as wire;
fn err(s: &mut Stats, value: &'static [u8]) {
    s.last_error = Text::literal(value);
}

pub fn apply_rx_result(s: &mut Stats, op: &wire::UdpOp) {
    s.last_source_port = op.source_port;
    s.last_dest_port = op.dest_port;
    match op.result {
        wire::UDP_RESULT_OK => {
            s.rx_packets += 1;
            err(s, b"none");
            if op.dest_port == super::dhcp::CLIENT_PORT
                || op.source_port == super::dhcp::SERVER_PORT
            {
                s.dhcp_rx += 1;
            }
            if op.dest_port == super::dns::PORT || op.source_port == super::dns::PORT {
                s.dns_rx += 1;
            }
        }
        wire::UDP_RESULT_SHORT => {
            s.dropped_short += 1;
            err(s, b"short");
        }
        wire::UDP_RESULT_LENGTH => {
            s.dropped_length += 1;
            err(s, b"length");
        }
        wire::UDP_RESULT_CHECKSUM => {
            s.checksum_errors += 1;
            err(s, b"checksum");
        }
        _ => {
            s.malformed += 1;
            err(s, b"not-udp");
        }
    }
}
pub fn apply_tx_result(s: &mut Stats, op: &wire::UdpOp) {
    s.last_source_port = op.source_port;
    s.last_dest_port = op.dest_port;
    match op.result {
        wire::UDP_RESULT_OK => {
            s.tx_packets += 1;
            err(s, b"none");
        }
        wire::UDP_RESULT_SHORT => err(s, b"short"),
        wire::UDP_RESULT_LENGTH => err(s, b"length"),
        wire::UDP_RESULT_CHECKSUM => err(s, b"checksum"),
        _ => err(s, b"not-udp"),
    }
}

/// # Safety
/// Exact exclusive paired statistics and readable canonical wire operation;
/// caller retains protocol admission. This applies metadata only, with no I/O.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_udp_apply_rx_result(
    stats: *mut Stats,
    op: *const wire::UdpOp,
) {
    unsafe { apply_rx_result(&mut *stats, &*op) }
}

/// # Safety
/// Exact exclusive paired statistics and readable canonical wire operation;
/// caller retains protocol admission. This applies metadata only, with no I/O.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_udp_apply_tx_result(
    stats: *mut Stats,
    op: *const wire::UdpOp,
) {
    unsafe { apply_tx_result(&mut *stats, &*op) }
}
