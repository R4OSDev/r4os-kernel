//! Existing ethernet protocol metadata and caller-owned statistics. Wire
//! encoding/checksums stay with the unchanged R4P implementation.
use crate::sched::sync_types::Text;
pub const TYPE_IPV4: u16 = 0x0800;
pub const TYPE_ARP: u16 = 0x0806;
pub const TYPE_R4OS_DIAG: u16 = 0x88B5;
pub const MIN_FRAME_SIZE: usize = 60;
pub const HEADER_SIZE: usize = 14;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Stats {
    pub rx: u64,
    pub tx: u64,
    pub broadcast: u64,
    pub own_unicast: u64,
    pub dropped_short: u64,
    pub dropped_filter: u64,
    pub unknown_ethertype: u64,
    pub test_frames: u64,
    pub last_ethertype: u16,
    pub last_error: Text,
}
impl Stats {
    pub const EMPTY: Self = Self {
        rx: 0,
        tx: 0,
        broadcast: 0,
        own_unicast: 0,
        dropped_short: 0,
        dropped_filter: 0,
        unknown_ethertype: 0,
        test_frames: 0,
        last_ethertype: 0,
        last_error: Text::literal(b"none"),
    };
}
/// # Safety
/// Exact exclusive paired statistics output; payload references never escape.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_ethernet_reset(out: *mut Stats) {
    unsafe { out.write(Stats::EMPTY) };
}

const _: () = {
    assert!(core::mem::size_of::<Stats>() == 88);
    assert!(core::mem::align_of::<Stats>() == 8);
    assert!(core::mem::offset_of!(Stats, rx) == 0);
    assert!(core::mem::offset_of!(Stats, tx) == 8);
    assert!(core::mem::offset_of!(Stats, broadcast) == 16);
    assert!(core::mem::offset_of!(Stats, own_unicast) == 24);
    assert!(core::mem::offset_of!(Stats, dropped_short) == 32);
    assert!(core::mem::offset_of!(Stats, dropped_filter) == 40);
    assert!(core::mem::offset_of!(Stats, unknown_ethertype) == 48);
    assert!(core::mem::offset_of!(Stats, test_frames) == 56);
    assert!(core::mem::offset_of!(Stats, last_ethertype) == 64);
    assert!(core::mem::offset_of!(Stats, last_error) == 72);
};

use super::r4p_contract as wire;
fn err(s: &mut Stats, value: &'static [u8]) {
    s.last_error = Text::literal(value);
}

fn classify(s: &mut Stats, kind: u16) {
    match kind {
        TYPE_IPV4 | TYPE_ARP => {}
        TYPE_R4OS_DIAG => s.test_frames += 1,
        _ => s.unknown_ethertype += 1,
    }
}
pub fn apply_rx_result(s: &mut Stats, op: &wire::EthernetFrameOp) {
    s.last_ethertype = op.ethertype;
    match op.result {
        wire::ETHERNET_RESULT_OK => {
            if op.flags & wire::ETHERNET_FLAG_BROADCAST != 0 {
                s.broadcast += 1;
            }
            if op.flags & wire::ETHERNET_FLAG_OWN_UNICAST != 0 {
                s.own_unicast += 1;
            }
            s.rx += 1;
            classify(s, op.ethertype);
            err(s, b"none");
        }
        wire::ETHERNET_RESULT_SHORT => {
            s.dropped_short += 1;
            err(s, b"rx-short");
        }
        wire::ETHERNET_RESULT_FILTERED => {
            s.dropped_filter += 1;
            err(s, b"rx-filter");
        }
        _ => err(s, b"r4p-rx-error"),
    }
}
pub fn apply_tx_result(s: &mut Stats, op: &wire::EthernetFrameOp) {
    s.last_ethertype = op.ethertype;
    match op.result {
        wire::ETHERNET_RESULT_OK => {
            if op.flags & wire::ETHERNET_FLAG_BROADCAST != 0 {
                s.broadcast += 1;
            }
            s.tx += 1;
            classify(s, op.ethertype);
            err(s, b"none");
        }
        wire::ETHERNET_RESULT_SHORT => {
            s.dropped_short += 1;
            err(s, b"tx-short");
        }
        _ => err(s, b"r4p-tx-error"),
    }
}

/// # Safety
/// Exact exclusive paired statistics and readable canonical wire operation;
/// caller retains protocol admission. This applies metadata only, with no I/O.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_ethernet_apply_rx_result(
    stats: *mut Stats,
    op: *const wire::EthernetFrameOp,
) {
    unsafe { apply_rx_result(&mut *stats, &*op) }
}

/// # Safety
/// Exact exclusive paired statistics and readable canonical wire operation;
/// caller retains protocol admission. This applies metadata only, with no I/O.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_ethernet_apply_tx_result(
    stats: *mut Stats,
    op: *const wire::EthernetFrameOp,
) {
    unsafe { apply_tx_result(&mut *stats, &*op) }
}
