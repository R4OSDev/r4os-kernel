//! Existing dhcp protocol metadata and caller-owned statistics. Wire
//! encoding/checksums stay with the unchanged R4P implementation.
use crate::sched::sync_types::Text;
pub const INFINITE_SECONDS: u32 = 0xffff_ffff;
pub const CLIENT_PORT: u16 = 68;
pub const SERVER_PORT: u16 = 67;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Lease {
    pub bound: bool,
    pub xid: u32,
    pub offered_ip: [u8; 4],
    pub server_ip: [u8; 4],
    pub netmask: [u8; 4],
    pub gateway_ip: [u8; 4],
    pub dns_ip: [u8; 4],
    pub dns_configured: bool,
    pub lease_seconds: u32,
    pub renew_seconds: u32,
    pub rebind_seconds: u32,
}
impl Lease {
    pub const EMPTY: Self = Self {
        bound: false,
        xid: 0,
        offered_ip: [0; 4],
        server_ip: [0; 4],
        netmask: [255, 255, 255, 0],
        gateway_ip: [0; 4],
        dns_ip: [0; 4],
        dns_configured: false,
        lease_seconds: 0,
        renew_seconds: 0,
        rebind_seconds: 0,
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Stats {
    pub discover_tx: u64,
    pub offer_rx: u64,
    pub request_tx: u64,
    pub ack_rx: u64,
    pub nak_rx: u64,
    pub release_tx: u64,
    pub retries: u64,
    pub timeouts: u64,
    pub release_errors: u64,
    pub malformed: u64,
    pub inactive_rx: u64,
    pub foreign_rx: u64,
    pub out_of_phase_rx: u64,
    pub self_tests: u64,
    pub lease: Lease,
    pub lease_acquired_tick: u64,
    pub last_attempt: u8,
    pub last_type: u8,
    pub operation_pending: bool,
    pub pending_label: Text,
    pub last_error: Text,
}
impl Stats {
    pub const EMPTY: Self = Self {
        discover_tx: 0,
        offer_rx: 0,
        request_tx: 0,
        ack_rx: 0,
        nak_rx: 0,
        release_tx: 0,
        retries: 0,
        timeouts: 0,
        release_errors: 0,
        malformed: 0,
        inactive_rx: 0,
        foreign_rx: 0,
        out_of_phase_rx: 0,
        self_tests: 0,
        lease: Lease::EMPTY,
        lease_acquired_tick: 0,
        last_attempt: 0,
        last_type: 0,
        operation_pending: false,
        pending_label: Text::literal(b"idle"),
        last_error: Text::literal(b"none"),
    };
}
/// # Safety
/// Exact exclusive paired statistics output; payload references never escape.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_dhcp_reset(out: *mut Stats) {
    unsafe { out.write(Stats::EMPTY) };
}

const _: () = {
    assert!(core::mem::size_of::<Lease>() == 44);
    assert!(core::mem::align_of::<Lease>() == 4);
    assert!(core::mem::offset_of!(Lease, bound) == 0);
    assert!(core::mem::offset_of!(Lease, xid) == 4);
    assert!(core::mem::offset_of!(Lease, offered_ip) == 8);
    assert!(core::mem::offset_of!(Lease, server_ip) == 12);
    assert!(core::mem::offset_of!(Lease, netmask) == 16);
    assert!(core::mem::offset_of!(Lease, gateway_ip) == 20);
    assert!(core::mem::offset_of!(Lease, dns_ip) == 24);
    assert!(core::mem::offset_of!(Lease, dns_configured) == 28);
    assert!(core::mem::offset_of!(Lease, lease_seconds) == 32);
    assert!(core::mem::offset_of!(Lease, renew_seconds) == 36);
    assert!(core::mem::offset_of!(Lease, rebind_seconds) == 40);
};
const _: () = {
    assert!(core::mem::size_of::<Stats>() == 208);
    assert!(core::mem::align_of::<Stats>() == 8);
    assert!(core::mem::offset_of!(Stats, discover_tx) == 0);
    assert!(core::mem::offset_of!(Stats, offer_rx) == 8);
    assert!(core::mem::offset_of!(Stats, request_tx) == 16);
    assert!(core::mem::offset_of!(Stats, ack_rx) == 24);
    assert!(core::mem::offset_of!(Stats, nak_rx) == 32);
    assert!(core::mem::offset_of!(Stats, release_tx) == 40);
    assert!(core::mem::offset_of!(Stats, retries) == 48);
    assert!(core::mem::offset_of!(Stats, timeouts) == 56);
    assert!(core::mem::offset_of!(Stats, release_errors) == 64);
    assert!(core::mem::offset_of!(Stats, malformed) == 72);
    assert!(core::mem::offset_of!(Stats, inactive_rx) == 80);
    assert!(core::mem::offset_of!(Stats, foreign_rx) == 88);
    assert!(core::mem::offset_of!(Stats, out_of_phase_rx) == 96);
    assert!(core::mem::offset_of!(Stats, self_tests) == 104);
    assert!(core::mem::offset_of!(Stats, lease) == 112);
    assert!(core::mem::offset_of!(Stats, lease_acquired_tick) == 160);
    assert!(core::mem::offset_of!(Stats, last_attempt) == 168);
    assert!(core::mem::offset_of!(Stats, last_type) == 169);
    assert!(core::mem::offset_of!(Stats, operation_pending) == 170);
    assert!(core::mem::offset_of!(Stats, pending_label) == 176);
    assert!(core::mem::offset_of!(Stats, last_error) == 192);
};

use super::r4p_contract as wire;
fn err(s: &mut Stats, value: &'static [u8]) {
    s.last_error = Text::literal(value);
}

fn lease_fields(lease: &mut Lease, op: &wire::DhcpOp) {
    lease.xid = op.xid;
    lease.offered_ip = op.offered_ip;
    lease.server_ip = op.server_ip;
    lease.netmask = op.netmask;
    lease.gateway_ip = op.gateway_ip;
    lease.dns_ip = op.dns_ip;
    lease.dns_configured = op.dns_configured != 0;
    lease.lease_seconds = op.lease_seconds;
    lease.renew_seconds = op.renew_seconds;
    lease.rebind_seconds = op.rebind_seconds;
}
pub fn apply_build_result(s: &mut Stats, op: &wire::DhcpOp) {
    apply_build_metadata(s, op.flags, op.message_type);
}
pub(super) fn apply_build_metadata(s: &mut Stats, flags: u32, message_type: u8) {
    s.last_type = message_type;
    err(s, b"none");
    if flags & wire::DHCP_FLAG_DISCOVER != 0 {
        s.discover_tx += 1;
    }
    if flags & wire::DHCP_FLAG_REQUEST != 0 {
        s.request_tx += 1;
    }
    if flags & wire::DHCP_FLAG_RELEASE != 0 {
        s.release_tx += 1;
        err(s, b"release");
    }
}
pub fn apply_rx_result(s: &mut Stats, op: &wire::DhcpOp) {
    s.last_type = op.message_type;
    match op.result {
        wire::DHCP_RESULT_OK => {
            if op.flags & wire::DHCP_FLAG_OFFER != 0 {
                s.offer_rx += 1;
                lease_fields(&mut s.lease, op);
                err(s, b"offer");
            } else if op.flags & wire::DHCP_FLAG_ACK != 0 {
                if op.lease_seconds == 0 || op.offered_ip == [0; 4] {
                    s.malformed += 1;
                    err(s, b"invalid-lease");
                    return;
                }
                s.ack_rx += 1;
                s.lease.bound = op.flags & wire::DHCP_FLAG_BOUND != 0;
                lease_fields(&mut s.lease, op);
                if s.lease.bound {
                    s.lease_acquired_tick = super::timing::now_ticks();
                }
                err(s, b"ack");
            } else if op.flags & wire::DHCP_FLAG_NAK != 0 {
                s.nak_rx += 1;
                s.lease.bound = false;
                s.lease_acquired_tick = 0;
                err(s, b"nak");
            } else {
                err(s, b"ignored");
            }
        }
        wire::DHCP_RESULT_IGNORED => err(s, b"ignored"),
        wire::DHCP_RESULT_NO_TYPE => {
            s.malformed += 1;
            err(s, b"no-type");
        }
        _ => {
            s.malformed += 1;
            err(s, b"shape");
        }
    }
}

/// # Safety
/// Exact exclusive paired statistics and readable canonical wire operation;
/// caller retains protocol admission. This applies metadata only, with no I/O.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_dhcp_apply_rx_result(
    stats: *mut Stats,
    op: *const wire::DhcpOp,
) {
    unsafe { apply_rx_result(&mut *stats, &*op) }
}

/// # Safety
/// Exact exclusive paired statistics and readable canonical wire operation;
/// caller retains protocol admission. This applies metadata only, with no I/O.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_dhcp_apply_build_result(
    stats: *mut Stats,
    op: *const wire::DhcpOp,
) {
    unsafe { apply_build_result(&mut *stats, &*op) }
}
