//! Existing dns protocol metadata and caller-owned statistics. Wire
//! encoding/checksums stay with the unchanged R4P implementation.
use crate::sched::sync_types::Text;
pub const PORT: u16 = 53;
pub const RESULT_OK: i32 = 0;
pub const RESULT_SHORT: i32 = -1;
pub const RESULT_HEADER: i32 = -2;
pub const RESULT_QNAME: i32 = -3;
pub const RESULT_QUESTION: i32 = -4;
pub const RESULT_ANAME: i32 = -5;
pub const RESULT_ANSWER: i32 = -6;
pub const RESULT_ATYPE: i32 = -7;
pub const RESULT_BUFFER_SMALL: i32 = -8;
pub const RESULT_NAME: i32 = -9;
pub const RESULT_NXDOMAIN: i32 = -10;
pub const RESULT_TIMEOUT: i32 = -11;
pub const RESULT_NO_SERVER: i32 = -12;
pub const RESULT_TX: i32 = -13;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Stats {
    pub queries_tx: u64,
    pub responses_rx: u64,
    pub a_records: u64,
    pub resolve_requests: u64,
    pub timeouts: u64,
    pub nxdomain: u64,
    pub tx_errors: u64,
    pub malformed: u64,
    pub self_tests: u64,
    pub last_id: u16,
    pub last_result: i32,
    pub last_server: [u8; 4],
    pub last_answer: [u8; 4],
    pub last_name: [u8; 64],
    pub operation_pending: bool,
    pub pending_name: [u8; 64],
    pub cache_valid: bool,
    pub cache_name: [u8; 64],
    pub cache_server: [u8; 4],
    pub cache_answer: [u8; 4],
    pub cache_hits: u64,
    pub cache_stores: u64,
    pub cache_updated_tick: u64,
    pub cache_ttl_seconds: u32,
    pub last_error: Text,
}
impl Stats {
    pub const EMPTY: Self = Self {
        queries_tx: 0,
        responses_rx: 0,
        a_records: 0,
        resolve_requests: 0,
        timeouts: 0,
        nxdomain: 0,
        tx_errors: 0,
        malformed: 0,
        self_tests: 0,
        last_id: 0,
        last_result: 0,
        last_server: [0; 4],
        last_answer: [0; 4],
        last_name: [0; 64],
        operation_pending: false,
        pending_name: [0; 64],
        cache_valid: false,
        cache_name: [0; 64],
        cache_server: [0; 4],
        cache_answer: [0; 4],
        cache_hits: 0,
        cache_stores: 0,
        cache_updated_tick: 0,
        cache_ttl_seconds: 60,
        last_error: Text::literal(b"none"),
    };
}
/// # Safety
/// Exact exclusive paired statistics output; payload references never escape.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_dns_reset(out: *mut Stats) {
    unsafe { out.write(Stats::EMPTY) };
}

const _: () = {
    assert!(core::mem::size_of::<Stats>() == 344);
    assert!(core::mem::align_of::<Stats>() == 8);
    assert!(core::mem::offset_of!(Stats, queries_tx) == 0);
    assert!(core::mem::offset_of!(Stats, responses_rx) == 8);
    assert!(core::mem::offset_of!(Stats, a_records) == 16);
    assert!(core::mem::offset_of!(Stats, resolve_requests) == 24);
    assert!(core::mem::offset_of!(Stats, timeouts) == 32);
    assert!(core::mem::offset_of!(Stats, nxdomain) == 40);
    assert!(core::mem::offset_of!(Stats, tx_errors) == 48);
    assert!(core::mem::offset_of!(Stats, malformed) == 56);
    assert!(core::mem::offset_of!(Stats, self_tests) == 64);
    assert!(core::mem::offset_of!(Stats, last_id) == 72);
    assert!(core::mem::offset_of!(Stats, last_result) == 76);
    assert!(core::mem::offset_of!(Stats, last_server) == 80);
    assert!(core::mem::offset_of!(Stats, last_answer) == 84);
    assert!(core::mem::offset_of!(Stats, last_name) == 88);
    assert!(core::mem::offset_of!(Stats, operation_pending) == 152);
    assert!(core::mem::offset_of!(Stats, pending_name) == 153);
    assert!(core::mem::offset_of!(Stats, cache_valid) == 217);
    assert!(core::mem::offset_of!(Stats, cache_name) == 218);
    assert!(core::mem::offset_of!(Stats, cache_server) == 282);
    assert!(core::mem::offset_of!(Stats, cache_answer) == 286);
    assert!(core::mem::offset_of!(Stats, cache_hits) == 296);
    assert!(core::mem::offset_of!(Stats, cache_stores) == 304);
    assert!(core::mem::offset_of!(Stats, cache_updated_tick) == 312);
    assert!(core::mem::offset_of!(Stats, cache_ttl_seconds) == 320);
    assert!(core::mem::offset_of!(Stats, last_error) == 328);
};

use super::r4p_contract as wire;
fn err(s: &mut Stats, value: &'static [u8]) {
    s.last_error = Text::literal(value);
}

pub fn apply_build_result(s: &mut Stats, op: &wire::DnsOp) {
    apply_build_metadata(s, op.id);
}
pub(super) fn apply_build_metadata(s: &mut Stats, id: u16) {
    s.queries_tx += 1;
    s.last_id = id;
    s.last_result = 1;
    err(s, b"query");
}
pub fn apply_rx_result(s: &mut Stats, op: &wire::DnsOp) {
    s.last_id = op.id;
    s.last_result = op.result;
    match op.result {
        wire::DNS_RESULT_OK => {
            s.responses_rx += 1;
            if op.flags & wire::DNS_FLAG_A_RECORD != 0 {
                s.a_records += 1;
            }
            s.last_answer = op.answer;
            err(s, b"answer");
        }
        wire::DNS_RESULT_SHORT => {
            s.malformed += 1;
            err(s, b"short");
        }
        wire::DNS_RESULT_HEADER => {
            s.malformed += 1;
            err(s, b"header");
        }
        wire::DNS_RESULT_QNAME => {
            s.malformed += 1;
            err(s, b"qname");
        }
        wire::DNS_RESULT_QUESTION => {
            s.malformed += 1;
            err(s, b"question");
        }
        wire::DNS_RESULT_ANAME => {
            s.malformed += 1;
            err(s, b"aname");
        }
        wire::DNS_RESULT_ANSWER => {
            s.malformed += 1;
            err(s, b"answer");
        }
        wire::DNS_RESULT_ATYPE => {
            s.malformed += 1;
            err(s, b"atype");
        }
        wire::DNS_RESULT_NXDOMAIN => {
            s.nxdomain += 1;
            err(s, b"nxdomain");
        }
        wire::DNS_RESULT_TIMEOUT => {
            s.timeouts += 1;
            err(s, b"timeout");
        }
        _ => {
            s.malformed += 1;
            err(s, b"name");
        }
    }
}

/// # Safety
/// Exact exclusive paired statistics and readable canonical wire operation;
/// caller retains protocol admission. This applies metadata only, with no I/O.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_dns_apply_rx_result(
    stats: *mut Stats,
    op: *const wire::DnsOp,
) {
    unsafe { apply_rx_result(&mut *stats, &*op) }
}

/// # Safety
/// Exact exclusive paired statistics and readable canonical wire operation;
/// caller retains protocol admission. This applies metadata only, with no I/O.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_dns_apply_build_result(
    stats: *mut Stats,
    op: *const wire::DnsOp,
) {
    unsafe { apply_build_result(&mut *stats, &*op) }
}
