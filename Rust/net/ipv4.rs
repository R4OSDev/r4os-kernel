//! Existing ipv4 protocol metadata and caller-owned statistics. Wire
//! encoding/checksums stay with the unchanged R4P implementation.
use crate::sched::sync_types::Text;
pub const HEADER_SIZE: usize = 20;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PacketView {
    pub protocol: u8,
    pub source_ip: [u8; 4],
    pub dest_ip: [u8; 4],
    pub payload: Text,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Stats {
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub dropped_short: u64,
    pub dropped_version: u64,
    pub dropped_checksum: u64,
    pub dropped_fragment: u64,
    pub dropped_destination: u64,
    pub dropped_tx_too_large: u64,
    pub malformed: u64,
    pub last_protocol: u8,
    pub last_error: Text,
    pub last_source: [u8; 4],
    pub last_dest: [u8; 4],
}
impl Stats {
    pub const EMPTY: Self = Self {
        rx_packets: 0,
        tx_packets: 0,
        dropped_short: 0,
        dropped_version: 0,
        dropped_checksum: 0,
        dropped_fragment: 0,
        dropped_destination: 0,
        dropped_tx_too_large: 0,
        malformed: 0,
        last_protocol: 0,
        last_error: Text::literal(b"none"),
        last_source: [0; 4],
        last_dest: [0; 4],
    };
}
/// # Safety
/// Exact exclusive paired statistics output; payload references never escape.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_ipv4_reset(out: *mut Stats) {
    unsafe { out.write(Stats::EMPTY) };
}
pub fn payload_fits_mtu(mtu: u16, len: usize) -> bool {
    HEADER_SIZE + len <= usize::from(mtu)
}
pub fn same_ip(a: [u8; 4], b: [u8; 4]) -> bool {
    a == b
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_ipv4_payload_fits(mtu: u16, len: usize) -> bool {
    payload_fits_mtu(mtu, len)
}
/// # Safety
/// Readable two exact IPv4 arrays.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_ipv4_same(a: *const [u8; 4], b: *const [u8; 4]) -> bool {
    same_ip(unsafe { *a }, unsafe { *b })
}

const _: () = {
    assert!(core::mem::size_of::<Stats>() == 104);
    assert!(core::mem::align_of::<Stats>() == 8);
    assert!(core::mem::offset_of!(Stats, rx_packets) == 0);
    assert!(core::mem::offset_of!(Stats, tx_packets) == 8);
    assert!(core::mem::offset_of!(Stats, dropped_short) == 16);
    assert!(core::mem::offset_of!(Stats, dropped_version) == 24);
    assert!(core::mem::offset_of!(Stats, dropped_checksum) == 32);
    assert!(core::mem::offset_of!(Stats, dropped_fragment) == 40);
    assert!(core::mem::offset_of!(Stats, dropped_destination) == 48);
    assert!(core::mem::offset_of!(Stats, dropped_tx_too_large) == 56);
    assert!(core::mem::offset_of!(Stats, malformed) == 64);
    assert!(core::mem::offset_of!(Stats, last_protocol) == 72);
    assert!(core::mem::offset_of!(Stats, last_error) == 80);
    assert!(core::mem::offset_of!(Stats, last_source) == 96);
    assert!(core::mem::offset_of!(Stats, last_dest) == 100);
};

use super::r4p_contract as wire;
fn err(s: &mut Stats, value: &'static [u8]) {
    s.last_error = Text::literal(value);
}

pub fn apply_rx_result(s: &mut Stats, op: &wire::Ipv4Op) {
    s.last_protocol = op.protocol;
    s.last_source = op.source_ip;
    s.last_dest = op.dest_ip;
    match op.result {
        wire::IPV4_RESULT_OK => {
            s.rx_packets += 1;
            err(s, b"none");
        }
        wire::IPV4_RESULT_SHORT => {
            s.dropped_short += 1;
            err(s, b"short");
        }
        wire::IPV4_RESULT_VERSION => {
            s.dropped_version += 1;
            err(s, b"version-ihl");
        }
        wire::IPV4_RESULT_LENGTH => {
            s.malformed += 1;
            err(s, b"length");
        }
        wire::IPV4_RESULT_FRAGMENT => {
            s.dropped_fragment += 1;
            err(s, b"fragment");
        }
        wire::IPV4_RESULT_CHECKSUM => {
            s.dropped_checksum += 1;
            err(s, b"checksum");
        }
        wire::IPV4_RESULT_DESTINATION => {
            s.dropped_destination += 1;
            err(s, b"not-local");
        }
        _ => {
            s.malformed += 1;
            err(s, b"not-ipv4");
        }
    }
}
pub fn apply_tx_result(s: &mut Stats, op: &wire::Ipv4Op) {
    s.last_protocol = op.protocol;
    s.last_source = op.source_ip;
    s.last_dest = op.dest_ip;
    match op.result {
        wire::IPV4_RESULT_OK => {
            s.tx_packets += 1;
            err(s, b"none");
        }
        wire::IPV4_RESULT_SHORT => err(s, b"short"),
        wire::IPV4_RESULT_VERSION => err(s, b"version-ihl"),
        wire::IPV4_RESULT_LENGTH => err(s, b"length"),
        wire::IPV4_RESULT_FRAGMENT => err(s, b"fragment"),
        wire::IPV4_RESULT_CHECKSUM => err(s, b"checksum"),
        _ => err(s, b"not-ipv4"),
    }
}

/// # Safety
/// Exact exclusive paired statistics and readable canonical wire operation;
/// caller retains protocol admission. This applies metadata only, with no I/O.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_ipv4_apply_rx_result(
    stats: *mut Stats,
    op: *const wire::Ipv4Op,
) {
    unsafe { apply_rx_result(&mut *stats, &*op) }
}

/// # Safety
/// Exact exclusive paired statistics and readable canonical wire operation;
/// caller retains protocol admission. This applies metadata only, with no I/O.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_ipv4_apply_tx_result(
    stats: *mut Stats,
    op: *const wire::Ipv4Op,
) {
    unsafe { apply_tx_result(&mut *stats, &*op) }
}
