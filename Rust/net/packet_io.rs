//! Original R4P packet I/O with native statistics and owned decoded payloads.
//! Wire/protocol work and caller copies precede metadata publication. Packet
//! views are formed only from a retained owned message in the calling frame.
use super::ipv4::PacketView as NativeIpView;
use super::{
    adapters, builders, config, core_bytes,
    packet_views::{self, OwnedIpv4Packet, OwnedUdpPacket},
    protocol_runtime as runtime, protocol_stats as stats, r4p_contract as wire, readers,
    tcp_client,
};
use crate::sched::sync_types::Text;
use core::mem::MaybeUninit;
fn input(packet: NativeIpView, checksum: bool) -> readers::WireReadInput {
    readers::WireReadInput {
        source_ip: packet.source_ip,
        dest_ip: packet.dest_ip,
        own_mac: [0; 6],
        protocol: packet.protocol,
        l4_checksum_valid: checksum,
    }
}
fn empty_input() -> readers::WireReadInput {
    readers::WireReadInput {
        source_ip: [0; 4],
        dest_ip: [0; 4],
        own_mac: [0; 6],
        protocol: 0,
        l4_checksum_valid: false,
    }
}
fn learn_arp(sender: [u8; 4], mac: [u8; 6], target: [u8; 4]) {
    if target != config::settings().local_ip
        || sender == [0; 4]
        || adapters::snapshot(0).is_some_and(|a| a.mac == mac)
    {
        return;
    }
    unsafe {
        stats::r4os_rust_net_arp_remember(&sender, &mac);
    }
}
pub(super) fn learn_ipv4(sender: [u8; 4], mac: [u8; 6]) {
    let cfg = config::settings();
    if sender == [0; 4]
        || sender == cfg.local_ip
        || !core_bytes::same_subnet(cfg.local_ip, sender, cfg.netmask)
        || adapters::snapshot(0).is_some_and(|a| a.mac == mac)
    {
        return;
    }
    unsafe {
        stats::r4os_rust_net_arp_remember(&sender, &mac);
    }
}
fn ethernet_report(report: readers::WireReadReport) {
    runtime::r4os_rust_net_note_ethernet_dispatch_failures(report.dispatch_failures);
    if report.reason.len != 0 {
        unsafe {
            stats::r4os_rust_net_ethernet_owned_error(report.reason);
        }
    }
}
fn ethernet_build_report(report: builders::WireBuildReport) {
    runtime::r4os_rust_net_note_ethernet_dispatch_failures(report.dispatch_failures);
    if report.reason.len != 0 {
        unsafe {
            stats::r4os_rust_net_ethernet_owned_error(report.reason);
        }
    }
}
fn arp_report(report: readers::WireReadReport) {
    runtime::r4os_rust_net_note_arp_dispatch_failures(report.dispatch_failures);
    if report.reason.len != 0 {
        unsafe {
            stats::r4os_rust_net_arp_owned_error(report.reason);
        }
    }
}
fn arp_build_report(report: builders::WireBuildReport) {
    runtime::r4os_rust_net_note_arp_dispatch_failures(report.dispatch_failures);
    if report.reason.len != 0 {
        unsafe {
            stats::r4os_rust_net_arp_owned_error(report.reason);
        }
    }
}
fn ipv4_report(report: readers::WireReadReport) {
    runtime::r4os_rust_net_note_ipv4_dispatch_failures(report.dispatch_failures);
    if report.reason.len != 0 {
        unsafe {
            stats::r4os_rust_net_ipv4_owned_error(report.reason);
        }
    }
}
fn ipv4_build_report(report: builders::WireBuildReport) {
    runtime::r4os_rust_net_note_ipv4_dispatch_failures(report.dispatch_failures);
    if report.reason.len != 0 {
        unsafe {
            stats::r4os_rust_net_ipv4_owned_error(report.reason);
        }
    }
}
fn icmp_report(report: readers::WireReadReport) {
    runtime::r4os_rust_net_note_icmp_dispatch_failures(report.dispatch_failures);
    if report.reason.len != 0 {
        unsafe {
            stats::r4os_rust_net_icmp_owned_error(report.reason);
        }
    }
}
fn icmp_build_report(report: builders::WireBuildReport) {
    runtime::r4os_rust_net_note_icmp_dispatch_failures(report.dispatch_failures);
    if report.reason.len != 0 {
        unsafe {
            stats::r4os_rust_net_icmp_owned_error(report.reason);
        }
    }
}
fn udp_report(report: readers::WireReadReport) {
    runtime::r4os_rust_net_note_udp_dispatch_failures(report.dispatch_failures);
    if report.reason.len != 0 {
        unsafe {
            stats::r4os_rust_net_udp_owned_error(report.reason);
        }
    }
}
fn udp_build_report(report: builders::WireBuildReport) {
    runtime::r4os_rust_net_note_udp_dispatch_failures(report.dispatch_failures);
    if report.reason.len != 0 {
        unsafe {
            stats::r4os_rust_net_udp_owned_error(report.reason);
        }
    }
}
pub(super) unsafe fn build_ethernet(
    params: builders::WireEthernetBuild,
    out: *mut u8,
    capacity: usize,
) -> Option<usize> {
    let mut report = MaybeUninit::<builders::WireBuildReport>::uninit();
    unsafe {
        builders::r4os_rust_wire_build_ethernet(&params, out, capacity, report.as_mut_ptr());
    }
    let report = unsafe { report.assume_init() };
    ethernet_build_report(report);
    if report.length == usize::MAX {
        return None;
    }
    runtime::r4os_rust_net_note_ethernet_r4p_build(1);
    Some(report.length)
}
/// # Safety
/// Exact readable resident parameters, retained input and writable output
/// through R4P activation/dispatch/copies. No CPU owner spans any such work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_packet_build_ethernet(
    params: *const builders::WireEthernetBuild,
    out: *mut u8,
    capacity: usize,
) -> usize {
    let params = unsafe { *params };
    unsafe { build_ethernet(params, out, capacity) }.unwrap_or(usize::MAX)
}
pub(super) unsafe fn build_arp(
    params: builders::WireArpBuild,
    out: *mut u8,
    capacity: usize,
) -> Option<usize> {
    let mut report = MaybeUninit::<builders::WireBuildReport>::uninit();
    unsafe {
        builders::r4os_rust_wire_build_arp(&params, out, capacity, report.as_mut_ptr());
    }
    let report = unsafe { report.assume_init() };
    arp_build_report(report);
    if report.length == usize::MAX {
        return None;
    }
    runtime::r4os_rust_net_note_arp_r4p_build(1);
    Some(report.length)
}
/// # Safety
/// Exact readable resident parameters, retained input and writable output
/// through R4P activation/dispatch/copies. No CPU owner spans any such work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_packet_build_arp(
    params: *const builders::WireArpBuild,
    out: *mut u8,
    capacity: usize,
) -> usize {
    let params = unsafe { *params };
    unsafe { build_arp(params, out, capacity) }.unwrap_or(usize::MAX)
}
pub(super) unsafe fn build_ipv4(
    params: builders::WireIpv4Build,
    data: *const u8,
    len: usize,
    out: *mut u8,
    capacity: usize,
) -> Option<usize> {
    let mut report = MaybeUninit::<builders::WireBuildReport>::uninit();
    unsafe {
        builders::r4os_rust_wire_build_ipv4(&params, data, len, out, capacity, report.as_mut_ptr());
    }
    let report = unsafe { report.assume_init() };
    ipv4_build_report(report);
    if report.length == usize::MAX {
        return None;
    }
    runtime::r4os_rust_net_note_ipv4_r4p_build(1);
    Some(report.length)
}
/// # Safety
/// Exact readable resident parameters, retained input and writable output
/// through R4P activation/dispatch/copies. No CPU owner spans any such work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_packet_build_ipv4(
    params: *const builders::WireIpv4Build,
    data: *const u8,
    len: usize,
    out: *mut u8,
    capacity: usize,
) -> usize {
    let params = unsafe { *params };
    unsafe { build_ipv4(params, data, len, out, capacity) }.unwrap_or(usize::MAX)
}
pub(super) unsafe fn build_icmp_request(
    params: builders::WireIcmpBuild,
    out: *mut u8,
    capacity: usize,
) -> Option<usize> {
    let mut report = MaybeUninit::<builders::WireBuildReport>::uninit();
    unsafe {
        builders::r4os_rust_wire_build_icmp_request(&params, out, capacity, report.as_mut_ptr());
    }
    let report = unsafe { report.assume_init() };
    icmp_build_report(report);
    if report.length == usize::MAX {
        return None;
    }
    runtime::r4os_rust_net_note_icmp_r4p_build(1);
    Some(report.length)
}
/// # Safety
/// Exact readable resident parameters, retained input and writable output
/// through R4P activation/dispatch/copies. No CPU owner spans any such work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_packet_build_icmp_request(
    params: *const builders::WireIcmpBuild,
    out: *mut u8,
    capacity: usize,
) -> usize {
    let params = unsafe { *params };
    unsafe { build_icmp_request(params, out, capacity) }.unwrap_or(usize::MAX)
}
pub(super) unsafe fn build_icmp_reply(
    data: *const u8,
    len: usize,
    out: *mut u8,
    capacity: usize,
) -> Option<usize> {
    let mut report = MaybeUninit::<builders::WireBuildReport>::uninit();
    unsafe {
        builders::r4os_rust_wire_build_icmp_reply(data, len, out, capacity, report.as_mut_ptr());
    }
    let report = unsafe { report.assume_init() };
    icmp_build_report(report);
    if report.length == usize::MAX {
        return None;
    }
    runtime::r4os_rust_net_note_icmp_r4p_build(1);
    Some(report.length)
}
/// # Safety
/// Exact readable resident parameters, retained input and writable output
/// through R4P activation/dispatch/copies. No CPU owner spans any such work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_packet_build_icmp_reply(
    data: *const u8,
    len: usize,
    out: *mut u8,
    capacity: usize,
) -> usize {
    unsafe { build_icmp_reply(data, len, out, capacity) }.unwrap_or(usize::MAX)
}
pub(super) unsafe fn build_udp(
    params: builders::WireUdpBuild,
    data: *const u8,
    len: usize,
    out: *mut u8,
    capacity: usize,
) -> Option<usize> {
    let mut report = MaybeUninit::<builders::WireBuildReport>::uninit();
    unsafe {
        builders::r4os_rust_wire_build_udp(&params, data, len, out, capacity, report.as_mut_ptr());
    }
    let report = unsafe { report.assume_init() };
    udp_build_report(report);
    if report.length == usize::MAX {
        return None;
    }
    runtime::r4os_rust_net_note_udp_r4p_build(1);
    Some(report.length)
}
/// # Safety
/// Exact readable resident parameters, retained input and writable output
/// through R4P activation/dispatch/copies. No CPU owner spans any such work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_packet_build_udp(
    params: *const builders::WireUdpBuild,
    data: *const u8,
    len: usize,
    out: *mut u8,
    capacity: usize,
) -> usize {
    let params = unsafe { *params };
    unsafe { build_udp(params, data, len, out, capacity) }.unwrap_or(usize::MAX)
}
/// # Safety
/// Exact readable parameters, retained input and writable output through R4P.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_packet_build_tcp(
    params: *const builders::WireTcpBuild,
    data: *const u8,
    len: usize,
    out: *mut u8,
    capacity: usize,
) -> usize {
    let params = unsafe { *params };
    unsafe { tcp_client::build(params, data, len, out, capacity) }.unwrap_or(usize::MAX)
}
pub(super) unsafe fn ethernet_rx(mac: [u8; 6], data: *const u8, len: usize) -> bool {
    let mut read_input = empty_input();
    read_input.own_mac = mac;
    let mut op = MaybeUninit::<wire::EthernetFrameOp>::uninit();
    let mut report = MaybeUninit::<readers::WireReadReport>::uninit();
    unsafe {
        readers::r4os_rust_wire_read_ethernet_rx(
            &read_input,
            data,
            len,
            op.as_mut_ptr(),
            report.as_mut_ptr(),
        );
    }
    let report = unsafe { report.assume_init() };
    ethernet_report(report);
    if !report.completed {
        return false;
    }
    let op = unsafe { op.assume_init() };
    unsafe {
        stats::r4os_rust_net_ethernet_owned_rx(&op);
    }
    if report.accepted {
        runtime::r4os_rust_net_note_ethernet_r4p_rx(1);
    }
    report.accepted
}
pub(super) unsafe fn ethernet_tx(data: *const u8, len: usize) -> bool {
    let read_input = empty_input();
    let mut op = MaybeUninit::<wire::EthernetFrameOp>::uninit();
    let mut report = MaybeUninit::<readers::WireReadReport>::uninit();
    unsafe {
        readers::r4os_rust_wire_read_ethernet_tx(
            &read_input,
            data,
            len,
            op.as_mut_ptr(),
            report.as_mut_ptr(),
        );
    }
    let report = unsafe { report.assume_init() };
    ethernet_report(report);
    if !report.completed {
        return false;
    }
    let op = unsafe { op.assume_init() };
    unsafe {
        stats::r4os_rust_net_ethernet_owned_tx(&op);
    }
    if report.accepted {
        runtime::r4os_rust_net_note_ethernet_r4p_tx(1);
    }
    report.accepted
}
pub(super) unsafe fn arp_rx(data: *const u8, len: usize) {
    let read_input = empty_input();
    let mut op = MaybeUninit::<wire::ArpOp>::uninit();
    let mut report = MaybeUninit::<readers::WireReadReport>::uninit();
    unsafe {
        readers::r4os_rust_wire_read_arp_rx(
            &read_input,
            data,
            len,
            op.as_mut_ptr(),
            report.as_mut_ptr(),
        );
    }
    let report = unsafe { report.assume_init() };
    arp_report(report);
    if !report.completed {
        return;
    }
    let op = unsafe { op.assume_init() };
    let request = unsafe { stats::r4os_rust_net_arp_owned_rx(&op) };
    if request {
        learn_arp(op.sender_ip, op.sender_mac, op.seen_target_ip);
        unsafe {
            stats::r4os_rust_net_arp_owned_error(Text::literal(b"rx-request"));
        }
    }
    runtime::r4os_rust_net_note_arp_r4p_rx(1);
}
pub(super) unsafe fn arp_tx(data: *const u8, len: usize) {
    let read_input = empty_input();
    let mut op = MaybeUninit::<wire::ArpOp>::uninit();
    let mut report = MaybeUninit::<readers::WireReadReport>::uninit();
    unsafe {
        readers::r4os_rust_wire_read_arp_tx(
            &read_input,
            data,
            len,
            op.as_mut_ptr(),
            report.as_mut_ptr(),
        );
    }
    let report = unsafe { report.assume_init() };
    arp_report(report);
    if !report.completed {
        return;
    }
    let op = unsafe { op.assume_init() };
    unsafe {
        stats::r4os_rust_net_arp_owned_tx(&op);
    }
    runtime::r4os_rust_net_note_arp_r4p_tx(1);
}
pub(super) unsafe fn ipv4_rx(data: *const u8, len: usize) -> Option<OwnedIpv4Packet> {
    let read_input = empty_input();
    let mut op = MaybeUninit::<wire::Ipv4Op>::uninit();
    let mut report = MaybeUninit::<readers::WireReadReport>::uninit();
    unsafe {
        readers::r4os_rust_wire_read_ipv4_rx(
            &read_input,
            data,
            len,
            op.as_mut_ptr(),
            report.as_mut_ptr(),
        );
    }
    let report = unsafe { report.assume_init() };
    ipv4_report(report);
    if !report.completed {
        return None;
    }
    let op = unsafe { op.assume_init() };
    unsafe {
        stats::r4os_rust_net_ipv4_owned_rx(&op);
    }
    if !report.accepted {
        return None;
    }
    runtime::r4os_rust_net_note_ipv4_r4p_rx(1);
    let mut value = MaybeUninit::uninit();
    unsafe {
        packet_views::r4os_rust_wire_ipv4_view(&op, value.as_mut_ptr());
    }
    Some(unsafe { value.assume_init() })
}
pub(super) unsafe fn ipv4_tx(data: *const u8, len: usize) -> Option<OwnedIpv4Packet> {
    let read_input = empty_input();
    let mut op = MaybeUninit::<wire::Ipv4Op>::uninit();
    let mut report = MaybeUninit::<readers::WireReadReport>::uninit();
    unsafe {
        readers::r4os_rust_wire_read_ipv4_tx(
            &read_input,
            data,
            len,
            op.as_mut_ptr(),
            report.as_mut_ptr(),
        );
    }
    let report = unsafe { report.assume_init() };
    ipv4_report(report);
    if !report.completed {
        return None;
    }
    let op = unsafe { op.assume_init() };
    unsafe {
        stats::r4os_rust_net_ipv4_owned_tx(&op);
    }
    if !report.accepted {
        return None;
    }
    runtime::r4os_rust_net_note_ipv4_r4p_tx(1);
    let mut value = MaybeUninit::uninit();
    unsafe {
        packet_views::r4os_rust_wire_ipv4_view(&op, value.as_mut_ptr());
    }
    Some(unsafe { value.assume_init() })
}
pub(super) unsafe fn icmp_rx(packet: NativeIpView) {
    let read_input = input(packet, false);
    let mut op = MaybeUninit::<wire::IcmpOp>::uninit();
    let mut report = MaybeUninit::<readers::WireReadReport>::uninit();
    unsafe {
        readers::r4os_rust_wire_read_icmp_rx(
            &read_input,
            packet.payload.pointer,
            packet.payload.len,
            op.as_mut_ptr(),
            report.as_mut_ptr(),
        );
    }
    let report = unsafe { report.assume_init() };
    icmp_report(report);
    if !report.completed {
        return;
    }
    let op = unsafe { op.assume_init() };
    unsafe {
        stats::r4os_rust_net_icmp_owned_rx(&op);
    }
    if report.accepted {
        runtime::r4os_rust_net_note_icmp_r4p_rx(1);
    }
}
pub(super) unsafe fn icmp_tx(packet: NativeIpView) {
    let read_input = input(packet, false);
    let mut op = MaybeUninit::<wire::IcmpOp>::uninit();
    let mut report = MaybeUninit::<readers::WireReadReport>::uninit();
    unsafe {
        readers::r4os_rust_wire_read_icmp_tx(
            &read_input,
            packet.payload.pointer,
            packet.payload.len,
            op.as_mut_ptr(),
            report.as_mut_ptr(),
        );
    }
    let report = unsafe { report.assume_init() };
    icmp_report(report);
    if !report.completed {
        return;
    }
    let op = unsafe { op.assume_init() };
    unsafe {
        stats::r4os_rust_net_icmp_owned_tx(&op);
    }
    if report.accepted {
        runtime::r4os_rust_net_note_icmp_r4p_tx(1);
    }
}
pub(super) unsafe fn udp_rx(packet: NativeIpView, checksum: bool) -> Option<OwnedUdpPacket> {
    let read_input = input(packet, checksum);
    let mut op = MaybeUninit::<wire::UdpOp>::uninit();
    let mut report = MaybeUninit::<readers::WireReadReport>::uninit();
    unsafe {
        readers::r4os_rust_wire_read_udp_rx(
            &read_input,
            packet.payload.pointer,
            packet.payload.len,
            op.as_mut_ptr(),
            report.as_mut_ptr(),
        );
    }
    let report = unsafe { report.assume_init() };
    udp_report(report);
    if !report.completed {
        return None;
    }
    let op = unsafe { op.assume_init() };
    unsafe {
        stats::r4os_rust_net_udp_owned_rx(&op);
    }
    if !report.accepted {
        return None;
    }
    runtime::r4os_rust_net_note_udp_r4p_rx(1);
    let mut value = MaybeUninit::uninit();
    unsafe {
        packet_views::r4os_rust_wire_udp_view(&op, value.as_mut_ptr());
    }
    Some(unsafe { value.assume_init() })
}
pub(super) unsafe fn udp_tx(packet: NativeIpView) -> Option<OwnedUdpPacket> {
    let read_input = input(packet, false);
    let mut op = MaybeUninit::<wire::UdpOp>::uninit();
    let mut report = MaybeUninit::<readers::WireReadReport>::uninit();
    unsafe {
        readers::r4os_rust_wire_read_udp_tx(
            &read_input,
            packet.payload.pointer,
            packet.payload.len,
            op.as_mut_ptr(),
            report.as_mut_ptr(),
        );
    }
    let report = unsafe { report.assume_init() };
    udp_report(report);
    if !report.completed {
        return None;
    }
    let op = unsafe { op.assume_init() };
    unsafe {
        stats::r4os_rust_net_udp_owned_tx(&op);
    }
    if !report.accepted {
        return None;
    }
    runtime::r4os_rust_net_note_udp_r4p_tx(1);
    let mut value = MaybeUninit::uninit();
    unsafe {
        packet_views::r4os_rust_wire_udp_view(&op, value.as_mut_ptr());
    }
    Some(unsafe { value.assume_init() })
}

pub(super) unsafe fn ethernet_type(data: *const u8, len: usize) -> u16 {
    let input = empty_input();
    let mut op = MaybeUninit::<wire::EthernetFrameOp>::uninit();
    let mut report = MaybeUninit::<readers::WireReadReport>::uninit();
    unsafe {
        readers::r4os_rust_wire_read_ethernet_type(
            &input,
            data,
            len,
            op.as_mut_ptr(),
            report.as_mut_ptr(),
        );
    }
    let report = unsafe { report.assume_init() };
    ethernet_report(report);
    if report.completed {
        u16::try_from(report.value).expect("Original EtherType bounds")
    } else {
        0
    }
}
pub(super) unsafe fn icmp_echo(data: *const u8, len: usize) -> bool {
    let input = empty_input();
    let mut op = MaybeUninit::<wire::IcmpOp>::uninit();
    let mut report = MaybeUninit::<readers::WireReadReport>::uninit();
    unsafe {
        readers::r4os_rust_wire_read_icmp_echo(
            &input,
            data,
            len,
            op.as_mut_ptr(),
            report.as_mut_ptr(),
        );
    }
    let report = unsafe { report.assume_init() };
    icmp_report(report);
    if !report.accepted {
        return false;
    }
    runtime::r4os_rust_net_note_icmp_r4p_classify(1);
    report.value != 0
}
/// # Safety
/// Retained readable exact inputs throughout R4P dispatch/copies, outside
/// metadata admission. No record reference escapes into a later activation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_packet_ethernet_rx(
    mac: *const [u8; 6],
    data: *const u8,
    len: usize,
) -> bool {
    let mac = unsafe { *mac };
    unsafe { ethernet_rx(mac, data, len) }
}
/// # Safety
/// Retained readable exact inputs throughout R4P dispatch/copies, outside
/// metadata admission. No record reference escapes into a later activation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_packet_ethernet_tx(data: *const u8, len: usize) -> bool {
    unsafe { ethernet_tx(data, len) }
}
/// # Safety
/// Retained readable exact inputs throughout R4P dispatch/copies, outside
/// metadata admission. No record reference escapes into a later activation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_packet_ethernet_type(data: *const u8, len: usize) -> u16 {
    unsafe { ethernet_type(data, len) }
}
/// # Safety
/// Retained readable exact inputs throughout R4P dispatch/copies, outside
/// metadata admission. No record reference escapes into a later activation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_packet_arp_rx(data: *const u8, len: usize) {
    unsafe { arp_rx(data, len) }
}
/// # Safety
/// Retained readable exact inputs throughout R4P dispatch/copies, outside
/// metadata admission. No record reference escapes into a later activation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_packet_arp_tx(data: *const u8, len: usize) {
    unsafe { arp_tx(data, len) }
}
/// # Safety
/// Retained readable exact inputs throughout R4P dispatch/copies, outside
/// metadata admission. No record reference escapes into a later activation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_packet_icmp_echo(data: *const u8, len: usize) -> bool {
    unsafe { icmp_echo(data, len) }
}
/// # Safety
/// Retained readable exact input and separate writable paired owned message;
/// failed decodes leave the output unused. No admission spans dispatch/copies.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_packet_ipv4_rx(
    data: *const u8,
    len: usize,
    out: *mut OwnedIpv4Packet,
) -> bool {
    if let Some(value) = unsafe { ipv4_rx(data, len) } {
        unsafe {
            out.write(value);
        }
        true
    } else {
        false
    }
}
/// # Safety
/// Retained readable exact input and separate writable paired owned message;
/// failed decodes leave the output unused. No admission spans dispatch/copies.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_packet_ipv4_tx(
    data: *const u8,
    len: usize,
    out: *mut OwnedIpv4Packet,
) -> bool {
    if let Some(value) = unsafe { ipv4_tx(data, len) } {
        unsafe {
            out.write(value);
        }
        true
    } else {
        false
    }
}
/// # Safety
/// Retained readable exact input and separate writable paired owned message;
/// failed decodes leave the output unused. No admission spans dispatch/copies.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_packet_udp_rx(
    input: *const NativeIpView,
    checksum: bool,
    out: *mut OwnedUdpPacket,
) -> bool {
    let input = unsafe { *input };
    if let Some(value) = unsafe { udp_rx(input, checksum) } {
        unsafe {
            out.write(value);
        }
        true
    } else {
        false
    }
}
/// # Safety
/// Retained readable exact input and separate writable paired owned message;
/// failed decodes leave the output unused. No admission spans dispatch/copies.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_packet_udp_tx(
    input: *const NativeIpView,
    out: *mut OwnedUdpPacket,
) -> bool {
    let input = unsafe { *input };
    if let Some(value) = unsafe { udp_tx(input) } {
        unsafe {
            out.write(value);
        }
        true
    } else {
        false
    }
}
/// # Safety
/// Exact paired view and retained readable packet through protocol dispatch.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_packet_icmp_rx(input: *const NativeIpView) {
    let input = unsafe { *input };
    unsafe {
        icmp_rx(input);
    }
}
/// # Safety
/// Exact paired view and retained readable packet through protocol dispatch.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_packet_icmp_tx(input: *const NativeIpView) {
    let input = unsafe { *input };
    unsafe {
        icmp_tx(input);
    }
}

/// # Safety
/// Exact readable source addresses, copied before bounded metadata admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_packet_learn_ipv4(ip: *const [u8; 4], mac: *const [u8; 6]) {
    let (ip, mac) = unsafe { (*ip, *mac) };
    learn_ipv4(ip, mac);
}
