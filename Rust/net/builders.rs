//! Original kernel-side R4P packet construction. Wire encoding stays with each
//! unchanged R4P. Reports contain only this call's effects, so the compatibility
//! facade applies them to current metadata after all callbacks/copies finish.
use super::{config, operations as ops, r4p_contract as wire};
use crate::sched::sync_types::Text;
use core::ptr;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct WireBuildReport {
    pub length: usize,
    pub dispatch_failures: u64,
    pub reason: Text,
    pub flags: u32,
    pub id: u16,
    pub message_type: u8,
    pub tcp_flags: u16,
}
impl WireBuildReport {
    const EMPTY: Self = Self {
        length: usize::MAX,
        dispatch_failures: 0,
        reason: Text::literal(b""),
        flags: 0,
        id: 0,
        message_type: 0,
        tcp_flags: 0,
    };
    fn failed(&mut self, reason: &'static [u8], count: bool) {
        self.reason = Text::literal(reason);
        if count {
            self.dispatch_failures += 1;
        }
    }
    fn start<T>(&mut self, op: Option<T>) -> Option<T> {
        if op.is_none() {
            self.failed(b"r4p-required", true);
        }
        op
    }
    fn dispatched(&mut self, ok: bool) -> bool {
        if !ok {
            self.failed(b"r4p-dispatch", true);
        }
        ok
    }
    // SAFETY: source is this call's private initialized operation; caller retains
    // writable output through any fault. No resident metadata reference is held.
    unsafe fn finish(
        &mut self,
        result: i32,
        source: &[u8],
        length: u32,
        out: *mut u8,
        capacity: usize,
    ) {
        let len = length as usize;
        if result != 0 || len > capacity {
            self.failed(b"r4p-build", true);
            return;
        }
        let source = &source[..len];
        if len != 0 {
            unsafe {
                ptr::copy_nonoverlapping(source.as_ptr(), out, len);
            }
        }
        self.length = len;
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct WireEthernetBuild {
    pub source_mac: [u8; 6],
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct WireArpBuild {
    pub source_mac: [u8; 6],
    pub target_ip: [u8; 4],
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct WireIpv4Build {
    pub source_mac: [u8; 6],
    pub dest_mac: [u8; 6],
    pub source_ip: [u8; 4],
    pub dest_ip: [u8; 4],
    pub protocol: u8,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct WireIcmpBuild {
    pub ident: u16,
    pub seq: u16,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct WireUdpBuild {
    pub source_ip: [u8; 4],
    pub dest_ip: [u8; 4],
    pub source_port: u16,
    pub dest_port: u16,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct WireDhcpBuild {
    pub xid: u32,
    pub mac: [u8; 6],
    pub requested_ip: [u8; 4],
    pub server_ip: [u8; 4],
    pub client_ip: [u8; 4],
    pub state: u8,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct WireTcpBuild {
    pub source_ip: [u8; 4],
    pub dest_ip: [u8; 4],
    pub source_port: u16,
    pub dest_port: u16,
    pub seq: u32,
    pub ack: u32,
    pub flags: u16,
    pub window: u16,
    pub options: u32,
}
// SAFETY: private initialized operation output and separate readable input.
unsafe fn stage(dst: &mut [u8], data: *const u8, len: usize) -> bool {
    if len > dst.len() {
        return false;
    }
    if len != 0 {
        unsafe {
            ptr::copy_nonoverlapping(data, dst.as_mut_ptr(), len);
        }
    }
    true
}
unsafe fn ethernet(params: WireEthernetBuild, out: *mut u8, capacity: usize) -> WireBuildReport {
    let mut r = WireBuildReport::EMPTY;
    let Some(mut op) = r.start(ops::new_ethernet_op()) else {
        return r;
    };
    op.source_mac = params.source_mac;
    if !r.dispatched(unsafe {
        ops::ethernet_dispatch(wire::ETHERNET_OP_BUILD_DIAG_FRAME, &mut op, 0)
    }) {
        return r;
    }
    unsafe {
        r.finish(op.result, &op.frame, op.frame_len, out, capacity);
    }
    r
}
unsafe fn arp(params: WireArpBuild, out: *mut u8, capacity: usize) -> WireBuildReport {
    let mut r = WireBuildReport::EMPTY;
    let Some(mut op) = r.start(ops::new_arp_op()) else {
        return r;
    };
    op.local_ip = config::settings().local_ip;
    op.source_mac = params.source_mac;
    op.target_ip = params.target_ip;
    if !r.dispatched(unsafe { ops::arp_dispatch(wire::ARP_OP_BUILD_REQUEST, &mut op, 0) }) {
        return r;
    }
    unsafe {
        r.finish(op.result, &op.frame, op.frame_len, out, capacity);
    }
    r
}
unsafe fn ipv4(
    params: WireIpv4Build,
    data: *const u8,
    len: usize,
    out: *mut u8,
    capacity: usize,
) -> WireBuildReport {
    let mut r = WireBuildReport::EMPTY;
    let Some(mut op) = r.start(ops::new_ipv4_op()) else {
        return r;
    };
    if !unsafe { stage(&mut op.payload, data, len) } {
        r.failed(b"payload-size", true);
        return r;
    }
    op.source_mac = params.source_mac;
    op.dest_mac = params.dest_mac;
    op.source_ip = params.source_ip;
    op.dest_ip = params.dest_ip;
    op.protocol = params.protocol;
    op.ttl = 64;
    op.payload_len = len as u32;
    if !r.dispatched(unsafe { ops::ipv4_dispatch(wire::IPV4_OP_BUILD_PACKET, &mut op, 0) }) {
        return r;
    }
    unsafe {
        r.finish(op.result, &op.frame, op.frame_len, out, capacity);
    }
    r
}
unsafe fn icmp_request(params: WireIcmpBuild, out: *mut u8, capacity: usize) -> WireBuildReport {
    let mut r = WireBuildReport::EMPTY;
    let Some(mut op) = r.start(ops::new_icmp_op()) else {
        return r;
    };
    op.ident = params.ident;
    op.seq = params.seq;
    if !r.dispatched(unsafe { ops::icmp_dispatch(wire::ICMP_OP_BUILD_ECHO_REQUEST, &mut op, 0) }) {
        return r;
    }
    unsafe {
        r.finish(op.result, &op.payload, op.payload_len, out, capacity);
    }
    r
}
unsafe fn icmp_reply(
    data: *const u8,
    len: usize,
    out: *mut u8,
    capacity: usize,
) -> WireBuildReport {
    let mut r = WireBuildReport::EMPTY;
    let Some(mut op) = r.start(ops::new_icmp_op()) else {
        return r;
    };
    if !unsafe { stage(&mut op.payload, data, len) } {
        r.failed(b"r4p-dispatch", false);
        return r;
    }
    op.payload_len = len as u32;
    if !r.dispatched(unsafe { ops::icmp_dispatch(wire::ICMP_OP_BUILD_ECHO_REPLY, &mut op, 0) }) {
        return r;
    }
    unsafe {
        r.finish(op.result, &op.payload, op.payload_len, out, capacity);
    }
    r
}
unsafe fn udp(
    params: WireUdpBuild,
    data: *const u8,
    len: usize,
    out: *mut u8,
    capacity: usize,
) -> WireBuildReport {
    let mut r = WireBuildReport::EMPTY;
    let Some(mut op) = r.start(ops::new_udp_op()) else {
        return r;
    };
    if !unsafe { stage(&mut op.payload, data, len) } {
        r.failed(b"payload-size", true);
        return r;
    }
    op.source_ip = params.source_ip;
    op.dest_ip = params.dest_ip;
    op.source_port = params.source_port;
    op.dest_port = params.dest_port;
    op.payload_len = len as u32;
    if !r.dispatched(unsafe { ops::udp_dispatch(wire::UDP_OP_BUILD_DATAGRAM, &mut op, 0) }) {
        return r;
    }
    unsafe {
        r.finish(op.result, &op.datagram, op.datagram_len, out, capacity);
    }
    r
}
#[derive(Clone, Copy)]
enum DhcpKind {
    Discover,
    Request,
    Release,
}
unsafe fn dhcp(
    kind: DhcpKind,
    params: WireDhcpBuild,
    out: *mut u8,
    capacity: usize,
) -> WireBuildReport {
    let mut r = WireBuildReport::EMPTY;
    let Some(mut op) = r.start(ops::new_dhcp_op()) else {
        return r;
    };
    op.xid = params.xid;
    op.mac = params.mac;
    let opcode = match kind {
        DhcpKind::Discover => wire::DHCP_OP_BUILD_DISCOVER,
        DhcpKind::Request => {
            assert!(params.state <= 2, "Original DHCP request state");
            op.requested_ip = params.requested_ip;
            op.server_ip = params.server_ip;
            op.client_ip = if params.state == 0 {
                [0; 4]
            } else {
                params.requested_ip
            };
            wire::DHCP_OP_BUILD_REQUEST
        }
        DhcpKind::Release => {
            op.client_ip = params.client_ip;
            op.server_ip = params.server_ip;
            wire::DHCP_OP_BUILD_RELEASE
        }
    };
    if !r.dispatched(unsafe { ops::dhcp_dispatch(opcode, &mut op, 0) }) {
        return r;
    }
    unsafe {
        r.finish(op.result, &op.payload, op.payload_len, out, capacity);
    }
    if r.length != usize::MAX {
        r.flags = op.flags;
        r.message_type = op.message_type;
    }
    r
}
unsafe fn dns(
    id: u16,
    data: *const u8,
    len: usize,
    out: *mut u8,
    capacity: usize,
) -> WireBuildReport {
    let mut r = WireBuildReport::EMPTY;
    let Some(mut op) = r.start(ops::new_dns_op()) else {
        return r;
    };
    op.id = id;
    if len == 0 || !unsafe { stage(&mut op.name, data, len) } {
        r.failed(b"r4p-dispatch", false);
        return r;
    }
    op.name_len = len as u16;
    if !r.dispatched(unsafe { ops::dns_dispatch(wire::DNS_OP_BUILD_A_QUERY, &mut op, 0) }) {
        return r;
    }
    unsafe {
        r.finish(op.result, &op.payload, op.payload_len, out, capacity);
    }
    if r.length != usize::MAX {
        r.id = op.id;
    }
    r
}
unsafe fn tcp(
    params: WireTcpBuild,
    data: *const u8,
    len: usize,
    out: *mut u8,
    capacity: usize,
) -> WireBuildReport {
    let mut r = WireBuildReport::EMPTY;
    let Some(mut op) = r.start(ops::new_tcp_op()) else {
        return r;
    };
    op.source_ip = params.source_ip;
    op.dest_ip = params.dest_ip;
    op.source_port = params.source_port;
    op.dest_port = params.dest_port;
    op.seq = params.seq;
    op.ack = params.ack;
    op.flags = params.flags;
    op.reserved = params.window;
    op.index = params.options;
    if !unsafe { stage(&mut op.payload, data, len) } {
        r.failed(b"r4p-dispatch", false);
        return r;
    }
    op.payload_len = len as u32;
    if !r.dispatched(unsafe { ops::tcp_dispatch(wire::TCP_OP_BUILD_SEGMENT, &mut op, 0) }) {
        return r;
    }
    unsafe {
        r.finish(op.result, &op.segment, op.segment_len, out, capacity);
    }
    if r.length != usize::MAX {
        r.tcp_flags = op.flags;
        r.reason = Text::literal(b"none");
    }
    r
}

// The paired C exports below are the temporary Zig compatibility boundary.

/// # Safety
/// Exact paired stack parameters/report and readable input/writable output
/// through R4P activation/execution and any caller-memory faults. No metadata
/// owner is held; the report applies only this operation's eventual effects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_build_ethernet(
    params: *const WireEthernetBuild,
    out: *mut u8,
    capacity: usize,
    report: *mut WireBuildReport,
) {
    let value = unsafe { ethernet(*params, out, capacity) };
    unsafe {
        report.write(value);
    }
}

/// # Safety
/// Exact paired stack parameters/report and readable input/writable output
/// through R4P activation/execution and any caller-memory faults. No metadata
/// owner is held; the report applies only this operation's eventual effects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_build_arp(
    params: *const WireArpBuild,
    out: *mut u8,
    capacity: usize,
    report: *mut WireBuildReport,
) {
    let value = unsafe { arp(*params, out, capacity) };
    unsafe {
        report.write(value);
    }
}

/// # Safety
/// Exact paired stack parameters/report and readable input/writable output
/// through R4P activation/execution and any caller-memory faults. No metadata
/// owner is held; the report applies only this operation's eventual effects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_build_ipv4(
    params: *const WireIpv4Build,
    data: *const u8,
    len: usize,
    out: *mut u8,
    capacity: usize,
    report: *mut WireBuildReport,
) {
    let value = unsafe { ipv4(*params, data, len, out, capacity) };
    unsafe {
        report.write(value);
    }
}

/// # Safety
/// Exact paired stack parameters/report and readable input/writable output
/// through R4P activation/execution and any caller-memory faults. No metadata
/// owner is held; the report applies only this operation's eventual effects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_build_icmp_request(
    params: *const WireIcmpBuild,
    out: *mut u8,
    capacity: usize,
    report: *mut WireBuildReport,
) {
    let value = unsafe { icmp_request(*params, out, capacity) };
    unsafe {
        report.write(value);
    }
}

/// # Safety
/// Exact paired stack parameters/report and readable input/writable output
/// through R4P activation/execution and any caller-memory faults. No metadata
/// owner is held; the report applies only this operation's eventual effects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_build_icmp_reply(
    data: *const u8,
    len: usize,
    out: *mut u8,
    capacity: usize,
    report: *mut WireBuildReport,
) {
    let value = unsafe { icmp_reply(data, len, out, capacity) };
    unsafe {
        report.write(value);
    }
}

/// # Safety
/// Exact paired stack parameters/report and readable input/writable output
/// through R4P activation/execution and any caller-memory faults. No metadata
/// owner is held; the report applies only this operation's eventual effects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_build_udp(
    params: *const WireUdpBuild,
    data: *const u8,
    len: usize,
    out: *mut u8,
    capacity: usize,
    report: *mut WireBuildReport,
) {
    let value = unsafe { udp(*params, data, len, out, capacity) };
    unsafe {
        report.write(value);
    }
}

/// # Safety
/// Exact paired stack parameters/report and readable input/writable output
/// through R4P activation/execution and any caller-memory faults. No metadata
/// owner is held; the report applies only this operation's eventual effects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_build_dhcp_discover(
    params: *const WireDhcpBuild,
    out: *mut u8,
    capacity: usize,
    report: *mut WireBuildReport,
) {
    let value = unsafe { dhcp(DhcpKind::Discover, *params, out, capacity) };
    unsafe {
        report.write(value);
    }
}

/// # Safety
/// Exact paired stack parameters/report and readable input/writable output
/// through R4P activation/execution and any caller-memory faults. No metadata
/// owner is held; the report applies only this operation's eventual effects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_build_dhcp_request(
    params: *const WireDhcpBuild,
    out: *mut u8,
    capacity: usize,
    report: *mut WireBuildReport,
) {
    let value = unsafe { dhcp(DhcpKind::Request, *params, out, capacity) };
    unsafe {
        report.write(value);
    }
}

/// # Safety
/// Exact paired stack parameters/report and readable input/writable output
/// through R4P activation/execution and any caller-memory faults. No metadata
/// owner is held; the report applies only this operation's eventual effects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_build_dhcp_release(
    params: *const WireDhcpBuild,
    out: *mut u8,
    capacity: usize,
    report: *mut WireBuildReport,
) {
    let value = unsafe { dhcp(DhcpKind::Release, *params, out, capacity) };
    unsafe {
        report.write(value);
    }
}

/// # Safety
/// Exact paired stack parameters/report and readable input/writable output
/// through R4P activation/execution and any caller-memory faults. No metadata
/// owner is held; the report applies only this operation's eventual effects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_build_dns(
    id: u16,
    data: *const u8,
    len: usize,
    out: *mut u8,
    capacity: usize,
    report: *mut WireBuildReport,
) {
    let value = unsafe { dns(id, data, len, out, capacity) };
    unsafe {
        report.write(value);
    }
}

/// # Safety
/// Exact paired stack parameters/report and readable input/writable output
/// through R4P activation/execution and any caller-memory faults. No metadata
/// owner is held; the report applies only this operation's eventual effects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_build_tcp(
    params: *const WireTcpBuild,
    data: *const u8,
    len: usize,
    out: *mut u8,
    capacity: usize,
    report: *mut WireBuildReport,
) {
    let value = unsafe { tcp(*params, data, len, out, capacity) };
    unsafe {
        report.write(value);
    }
}

const _: () = {
    assert!(core::mem::size_of::<WireBuildReport>() == 48);
    assert!(core::mem::align_of::<WireBuildReport>() == 8);
    assert!(core::mem::offset_of!(WireBuildReport, length) == 0);
    assert!(core::mem::offset_of!(WireBuildReport, dispatch_failures) == 8);
    assert!(core::mem::offset_of!(WireBuildReport, reason) == 16);
    assert!(core::mem::offset_of!(WireBuildReport, flags) == 32);
    assert!(core::mem::offset_of!(WireBuildReport, id) == 36);
    assert!(core::mem::offset_of!(WireBuildReport, message_type) == 38);
    assert!(core::mem::offset_of!(WireBuildReport, tcp_flags) == 40);
};
const _: () = {
    assert!(core::mem::size_of::<WireEthernetBuild>() == 6);
    assert!(core::mem::align_of::<WireEthernetBuild>() == 1);
    assert!(core::mem::offset_of!(WireEthernetBuild, source_mac) == 0);
};
const _: () = {
    assert!(core::mem::size_of::<WireArpBuild>() == 10);
    assert!(core::mem::align_of::<WireArpBuild>() == 1);
    assert!(core::mem::offset_of!(WireArpBuild, source_mac) == 0);
    assert!(core::mem::offset_of!(WireArpBuild, target_ip) == 6);
};
const _: () = {
    assert!(core::mem::size_of::<WireIpv4Build>() == 21);
    assert!(core::mem::align_of::<WireIpv4Build>() == 1);
    assert!(core::mem::offset_of!(WireIpv4Build, source_mac) == 0);
    assert!(core::mem::offset_of!(WireIpv4Build, dest_mac) == 6);
    assert!(core::mem::offset_of!(WireIpv4Build, source_ip) == 12);
    assert!(core::mem::offset_of!(WireIpv4Build, dest_ip) == 16);
    assert!(core::mem::offset_of!(WireIpv4Build, protocol) == 20);
};
const _: () = {
    assert!(core::mem::size_of::<WireIcmpBuild>() == 4);
    assert!(core::mem::align_of::<WireIcmpBuild>() == 2);
    assert!(core::mem::offset_of!(WireIcmpBuild, ident) == 0);
    assert!(core::mem::offset_of!(WireIcmpBuild, seq) == 2);
};
const _: () = {
    assert!(core::mem::size_of::<WireUdpBuild>() == 12);
    assert!(core::mem::align_of::<WireUdpBuild>() == 2);
    assert!(core::mem::offset_of!(WireUdpBuild, source_ip) == 0);
    assert!(core::mem::offset_of!(WireUdpBuild, dest_ip) == 4);
    assert!(core::mem::offset_of!(WireUdpBuild, source_port) == 8);
    assert!(core::mem::offset_of!(WireUdpBuild, dest_port) == 10);
};
const _: () = {
    assert!(core::mem::size_of::<WireDhcpBuild>() == 24);
    assert!(core::mem::align_of::<WireDhcpBuild>() == 4);
    assert!(core::mem::offset_of!(WireDhcpBuild, xid) == 0);
    assert!(core::mem::offset_of!(WireDhcpBuild, mac) == 4);
    assert!(core::mem::offset_of!(WireDhcpBuild, requested_ip) == 10);
    assert!(core::mem::offset_of!(WireDhcpBuild, server_ip) == 14);
    assert!(core::mem::offset_of!(WireDhcpBuild, client_ip) == 18);
    assert!(core::mem::offset_of!(WireDhcpBuild, state) == 22);
};
const _: () = {
    assert!(core::mem::size_of::<WireTcpBuild>() == 28);
    assert!(core::mem::align_of::<WireTcpBuild>() == 4);
    assert!(core::mem::offset_of!(WireTcpBuild, source_ip) == 0);
    assert!(core::mem::offset_of!(WireTcpBuild, dest_ip) == 4);
    assert!(core::mem::offset_of!(WireTcpBuild, source_port) == 8);
    assert!(core::mem::offset_of!(WireTcpBuild, dest_port) == 10);
    assert!(core::mem::offset_of!(WireTcpBuild, seq) == 12);
    assert!(core::mem::offset_of!(WireTcpBuild, ack) == 16);
    assert!(core::mem::offset_of!(WireTcpBuild, flags) == 20);
    assert!(core::mem::offset_of!(WireTcpBuild, window) == 22);
    assert!(core::mem::offset_of!(WireTcpBuild, options) == 24);
};
