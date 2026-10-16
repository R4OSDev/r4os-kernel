//! Existing arp protocol metadata and caller-owned statistics. Wire
//! encoding/checksums stay with the unchanged R4P implementation.
use crate::sched::sync_types::Text;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Stats {
    pub requests_tx: u64,
    pub replies_tx: u64,
    pub replies_rx: u64,
    pub requests_rx: u64,
    pub malformed: u64,
    pub cache_updates: u64,
    pub cache_hits: u64,
    pub resolve_attempts: u64,
    pub resolve_retries: u64,
    pub resolve_timeouts: u64,
    pub resolve_misses: u64,
    pub pending_packets: u64,
    pub pending_timeouts: u64,
    pub pending_drops: u64,
    pub pending_queue_limit: u64,
    pub last_opcode: u16,
    pub last_error: Text,
    pub cache_valid: bool,
    pub cache_ip: [u8; 4],
    pub cache_mac: [u8; 6],
}
impl Stats {
    pub const EMPTY: Self = Self {
        requests_tx: 0,
        replies_tx: 0,
        replies_rx: 0,
        requests_rx: 0,
        malformed: 0,
        cache_updates: 0,
        cache_hits: 0,
        resolve_attempts: 0,
        resolve_retries: 0,
        resolve_timeouts: 0,
        resolve_misses: 0,
        pending_packets: 0,
        pending_timeouts: 0,
        pending_drops: 0,
        pending_queue_limit: 0,
        last_opcode: 0,
        last_error: Text::literal(b"none"),
        cache_valid: false,
        cache_ip: [0; 4],
        cache_mac: [0; 6],
    };
}
/// # Safety
/// Exact exclusive paired statistics output; payload references never escape.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_arp_reset(out: *mut Stats) {
    unsafe { out.write(Stats::EMPTY) };
}

const _: () = {
    assert!(core::mem::size_of::<Stats>() == 160);
    assert!(core::mem::align_of::<Stats>() == 8);
    assert!(core::mem::offset_of!(Stats, requests_tx) == 0);
    assert!(core::mem::offset_of!(Stats, replies_tx) == 8);
    assert!(core::mem::offset_of!(Stats, replies_rx) == 16);
    assert!(core::mem::offset_of!(Stats, requests_rx) == 24);
    assert!(core::mem::offset_of!(Stats, malformed) == 32);
    assert!(core::mem::offset_of!(Stats, cache_updates) == 40);
    assert!(core::mem::offset_of!(Stats, cache_hits) == 48);
    assert!(core::mem::offset_of!(Stats, resolve_attempts) == 56);
    assert!(core::mem::offset_of!(Stats, resolve_retries) == 64);
    assert!(core::mem::offset_of!(Stats, resolve_timeouts) == 72);
    assert!(core::mem::offset_of!(Stats, resolve_misses) == 80);
    assert!(core::mem::offset_of!(Stats, pending_packets) == 88);
    assert!(core::mem::offset_of!(Stats, pending_timeouts) == 96);
    assert!(core::mem::offset_of!(Stats, pending_drops) == 104);
    assert!(core::mem::offset_of!(Stats, pending_queue_limit) == 112);
    assert!(core::mem::offset_of!(Stats, last_opcode) == 120);
    assert!(core::mem::offset_of!(Stats, last_error) == 128);
    assert!(core::mem::offset_of!(Stats, cache_valid) == 144);
    assert!(core::mem::offset_of!(Stats, cache_ip) == 145);
    assert!(core::mem::offset_of!(Stats, cache_mac) == 149);
};

use super::r4p_contract as wire;
fn err(s: &mut Stats, value: &'static [u8]) {
    s.last_error = Text::literal(value);
}

/// Returns the original request-learning action to the separate cache owner.
pub fn apply_rx_result(s: &mut Stats, op: &wire::ArpOp) -> bool {
    s.last_opcode = op.opcode;
    match op.result {
        wire::ARP_RESULT_OK => {
            if op.flags & wire::ARP_FLAG_REPLY != 0 {
                s.replies_rx += 1;
                s.cache_mac = op.sender_mac;
                s.cache_ip = op.sender_ip;
                s.cache_valid = true;
                s.cache_updates += 1;
                err(s, b"none");
            } else if op.flags & wire::ARP_FLAG_REQUEST != 0 {
                s.requests_rx += 1;
                err(s, b"rx-request");
                return true;
            } else {
                err(s, b"none");
            }
        }
        wire::ARP_RESULT_SHORT => {
            s.malformed += 1;
            err(s, b"rx-short");
        }
        wire::ARP_RESULT_SHAPE => {
            s.malformed += 1;
            err(s, b"rx-shape");
        }
        wire::ARP_RESULT_OPCODE => {
            s.malformed += 1;
            err(s, b"rx-opcode");
        }
        _ => err(s, b"rx-not-arp"),
    };
    false
}
pub fn apply_tx_result(s: &mut Stats, op: &wire::ArpOp) {
    s.last_opcode = op.opcode;
    match op.result {
        wire::ARP_RESULT_OK => {
            if op.flags & wire::ARP_FLAG_REQUEST != 0 {
                s.requests_tx += 1;
                err(s, b"none");
            }
        }
        wire::ARP_RESULT_SHORT => {
            s.malformed += 1;
            err(s, b"tx-short");
        }
        wire::ARP_RESULT_OPCODE => {
            s.malformed += 1;
            err(s, b"tx-opcode");
        }
        _ => err(s, b"tx-not-arp"),
    }
}

/// # Safety
/// Exact exclusive paired statistics and readable canonical wire operation;
/// caller retains protocol admission. This applies metadata only, with no I/O.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_arp_apply_rx_result(
    stats: *mut Stats,
    op: *const wire::ArpOp,
) -> bool {
    unsafe { apply_rx_result(&mut *stats, &*op) }
}

/// # Safety
/// Exact exclusive paired statistics and readable canonical wire operation;
/// caller retains protocol admission. This applies metadata only, with no I/O.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_arp_apply_tx_result(
    stats: *mut Stats,
    op: *const wire::ArpOp,
) {
    unsafe { apply_tx_result(&mut *stats, &*op) }
}
