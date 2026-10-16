//! Original frame dispatch, IPv4/ARP/ICMP sends and UDP coordination.
//! Decoder messages belong to this activation; only raw retained caller bytes
//! span R4P/driver work. Network admission never spans callbacks or waits.
use super::{
    adapters, app_ipv4, builders, cleanup, config, core_bytes, dhcp_client, dns_client, ethernet,
    ipv4, packet_io as io, packet_views::OwnedIpv4Packet, protocol_stats as stats, rx_handoff,
    rx_runtime, tcp_wire, transmit, udp_sockets,
};
use crate::{build_bridge::config as build_config, memory::owner::Guard, sched::sync_types::Text};
use core::{cell::UnsafeCell, mem::MaybeUninit, ptr};
#[repr(C)]
pub struct Peers {
    pub rx: *const rx_runtime::RxPeers,
    pub arp: *const stats::ArpPeers,
}
struct PeerStorage(UnsafeCell<*const Peers>);
// SAFETY: singleton bootstrap installs one immutable kernel-lifetime table.
unsafe impl Sync for PeerStorage {}
static PEERS: PeerStorage = PeerStorage(UnsafeCell::new(ptr::null()));
pub(crate) fn peers() -> &'static Peers {
    let p = {
        let _guard = Guard::acquire(2);
        unsafe { *PEERS.0.get() }
    };
    assert!(!p.is_null(), "Frame peers installed before network work");
    unsafe { &*p }
}
fn count() -> usize {
    adapters::r4os_rust_net_adapter_count()
}
fn mac(index: usize) -> [u8; 6] {
    adapters::snapshot(index).map_or([0; 6], |a| a.mac)
}
fn ip_view(packet: &OwnedIpv4Packet) -> ipv4::PacketView {
    ipv4::PacketView {
        protocol: packet.protocol,
        source_ip: packet.source_ip,
        dest_ip: packet.dest_ip,
        payload: Text {
            pointer: packet.data.as_ptr(),
            len: packet.payload_len,
        },
    }
}
fn arp_stats() -> super::arp::Stats {
    let mut out = MaybeUninit::uninit();
    unsafe {
        stats::r4os_rust_net_arp_owned_stats(out.as_mut_ptr(), false);
        out.assume_init()
    }
}
fn ether_error() -> Text {
    let mut out = MaybeUninit::uninit();
    unsafe {
        stats::r4os_rust_net_ethernet_owned_stats(out.as_mut_ptr());
        out.assume_init()
    }
    .last_error
}
fn ip_error() -> Text {
    let mut out = MaybeUninit::uninit();
    unsafe {
        stats::r4os_rust_net_ipv4_owned_stats(out.as_mut_ptr());
        out.assume_init()
    }
    .last_error
}
fn udp_error(reason: &'static [u8]) {
    unsafe {
        stats::r4os_rust_net_udp_owned_error(Text::literal(reason));
    }
}
fn poll(rounds: usize, without_rx_task: bool) {
    if without_rx_task {
        unsafe {
            rx_runtime::r4os_rust_net_rx_runtime_poll_without_task(rounds, peers().rx);
        }
    } else {
        unsafe {
            rx_runtime::r4os_rust_net_rx_runtime_poll(rounds, 255, peers().rx);
        }
    }
}
unsafe extern "C" fn sent(data: *const u8, len: usize) {
    unsafe {
        io::ethernet_tx(data, len);
    }
    let kind = unsafe { io::ethernet_type(data, len) };
    if kind == ethernet::TYPE_ARP {
        unsafe {
            io::arp_tx(data, len);
        }
    }
    if kind == ethernet::TYPE_IPV4
        && let Some(packet) = (unsafe { io::ipv4_tx(data, len) })
    {
        let view = ip_view(&packet);
        unsafe {
            io::icmp_tx(view);
            io::udp_tx(view);
            tcp_wire::tx(view);
        }
    }
}
unsafe extern "C" fn cleanup_link(reason: *const u8, len: usize) {
    unsafe {
        cleanup::all(Text {
            pointer: reason,
            len,
        });
    }
}
unsafe fn transmit(index: usize, data: *const u8, len: usize) -> u8 {
    let peers = transmit::TransmitPeers {
        sent,
        cleanup: cleanup_link,
    };
    unsafe { transmit::r4os_rust_net_transmit(index, data, len, &peers) }
}
fn resolve(index: usize, target: [u8; 4], out: &mut [u8; 6]) -> u8 {
    unsafe { stats::r4os_rust_net_arp_resolve(index, &target, out, peers().arp) }
}
fn reject_large(protocol: u8, target: [u8; 4], reason: &'static [u8]) -> u8 {
    unsafe {
        stats::r4os_rust_net_ipv4_reject_large(protocol, &target, Text::literal(reason));
    }
    transmit::record(4)
}
pub(super) unsafe fn send_ipv4(target: [u8; 4], protocol: u8, data: *const u8, len: usize) -> u8 {
    if count() == 0 {
        return 1;
    }
    if len > 1536 {
        return reject_large(protocol, target, b"tx-size");
    }
    let mtu = adapters::snapshot(0).map_or(1500, |a| a.mtu);
    if !ipv4::payload_fits_mtu(mtu, len) {
        return reject_large(protocol, target, b"tx-mtu");
    }
    let mut dest_mac = [0; 6];
    let route = resolve(0, target, &mut dest_mac);
    if route != 0 {
        return route;
    }
    let mut frame = [0; 1536];
    let params = builders::WireIpv4Build {
        source_mac: mac(0),
        dest_mac,
        source_ip: config::settings().local_ip,
        dest_ip: target,
        protocol,
    };
    let Some(length) =
        (unsafe { io::build_ipv4(params, data, len, frame.as_mut_ptr(), frame.len()) })
    else {
        return reject_large(protocol, target, b"tx-build");
    };
    let result = unsafe { transmit(0, frame.as_ptr(), length) };
    if result == 0 {
        poll(32, true);
    }
    result
}
fn send_arp(index: usize, target: [u8; 4]) -> u8 {
    if index >= count() {
        return 1;
    }
    let mut frame = [0; 60];
    let params = builders::WireArpBuild {
        source_mac: mac(index),
        target_ip: target,
    };
    let Some(length) = (unsafe { io::build_arp(params, frame.as_mut_ptr(), frame.len()) }) else {
        return 4;
    };
    unsafe { transmit(index, frame.as_ptr(), length) }
}
unsafe fn arp_reply(index: usize, data: *const u8, len: usize) {
    if index >= count() || len < 42 || unsafe { io::ethernet_type(data, len) } != ethernet::TYPE_ARP
    {
        return;
    }
    let mut header = [0; 42];
    unsafe {
        ptr::copy_nonoverlapping(data, header.as_mut_ptr(), header.len());
    }
    if core_bytes::read_be16(&header, 14) != 1
        || core_bytes::read_be16(&header, 16) != ethernet::TYPE_IPV4
        || header[18] != 6
        || header[19] != 4
        || core_bytes::read_be16(&header, 20) != 1
    {
        return;
    }
    let target: [u8; 4] = header[38..42].try_into().expect("ARP target shape");
    let local = config::settings().local_ip;
    if target != local {
        return;
    }
    let requester_mac: [u8; 6] = header[22..28].try_into().expect("ARP sender shape");
    let requester_ip: [u8; 4] = header[28..32].try_into().expect("ARP sender shape");
    if requester_mac == mac(index) {
        return;
    }
    let mut reply = [0; 60];
    reply[..6].copy_from_slice(&requester_mac);
    reply[6..12].copy_from_slice(&mac(index));
    core_bytes::write_be16(&mut reply, 12, ethernet::TYPE_ARP);
    core_bytes::write_be16(&mut reply, 14, 1);
    core_bytes::write_be16(&mut reply, 16, ethernet::TYPE_IPV4);
    reply[18] = 6;
    reply[19] = 4;
    core_bytes::write_be16(&mut reply, 20, 2);
    reply[22..28].copy_from_slice(&mac(index));
    reply[28..32].copy_from_slice(&local);
    reply[32..38].copy_from_slice(&requester_mac);
    reply[38..42].copy_from_slice(&requester_ip);
    unsafe {
        transmit(index, reply.as_ptr(), reply.len());
    }
}
unsafe fn echo_reply(index: usize, data: *const u8, len: usize, packet: ipv4::PacketView) {
    if index >= count() || len < 14 {
        return;
    }
    let mut dest_mac = [0; 6];
    unsafe {
        ptr::copy_nonoverlapping(data.add(6), dest_mac.as_mut_ptr(), 6);
    }
    let mut payload = [0; 128];
    let Some(length) = (unsafe {
        io::build_icmp_reply(
            packet.payload.pointer,
            packet.payload.len,
            payload.as_mut_ptr(),
            payload.len(),
        )
    }) else {
        return;
    };
    let mut frame = [0; 256];
    let params = builders::WireIpv4Build {
        source_mac: mac(index),
        dest_mac,
        source_ip: config::settings().local_ip,
        dest_ip: packet.source_ip,
        protocol: 1,
    };
    let Some(length) = (unsafe {
        io::build_ipv4(
            params,
            payload.as_ptr(),
            length,
            frame.as_mut_ptr(),
            frame.len(),
        )
    }) else {
        return;
    };
    unsafe {
        transmit(index, frame.as_ptr(), length);
    }
}
unsafe fn received(
    index: usize,
    data: *const u8,
    len: usize,
    metadata: rx_handoff::Metadata,
) -> bool {
    if index >= count() {
        return false;
    }
    if build_config::ENABLE_NET_LOSS_TEST && rx_runtime::r4os_rust_net_rx_runtime_loss_drop() {
        return false;
    }
    if len == 0 || len > 1536 {
        unsafe {
            adapters::r4os_rust_net_adapter_drop(index, Text::literal(b"rx-size"));
        }
        return false;
    }
    if !unsafe { io::ethernet_rx(mac(index), data, len) } {
        unsafe {
            adapters::r4os_rust_net_adapter_drop(index, ether_error());
        }
        return false;
    }
    let kind = unsafe { io::ethernet_type(data, len) };
    if kind == ethernet::TYPE_ARP {
        let before = arp_stats().cache_updates;
        unsafe {
            io::arp_rx(data, len);
            arp_reply(index, data, len);
        }
        let after = arp_stats();
        if after.cache_updates != before && after.cache_valid {
            stats::r4os_rust_net_arp_remember_stats();
        }
        adapters::r4os_rust_net_adapter_rx(index, len);
        return true;
    }
    if kind == ethernet::TYPE_IPV4 {
        let Some(packet) = (unsafe { io::ipv4_rx(data, len) }) else {
            unsafe {
                adapters::r4os_rust_net_adapter_drop(index, ip_error());
            }
            return false;
        };
        let view = ip_view(&packet);
        assert!(len >= 12, "Accepted Ethernet header shape");
        let mut source_mac = [0; 6];
        unsafe {
            ptr::copy_nonoverlapping(data.add(6), source_mac.as_mut_ptr(), 6);
        }
        io::learn_ipv4(view.source_ip, source_mac);
        let info = app_ipv4::Info {
            source_ip: view.source_ip,
            dest_ip: view.dest_ip,
            protocol: view.protocol,
            source_port: 0,
            dest_port: 0,
            truncated: false,
            payload_len: 0,
        };
        unsafe {
            app_ipv4::r4os_rust_app_ipv4_enqueue(&info, view.payload.pointer, view.payload.len);
            io::icmp_rx(view);
        }
        let l4 = metadata.l4_checksum_valid && (view.protocol == 17 || view.protocol == 6);
        rx_runtime::r4os_rust_net_rx_runtime_l4(view.protocol, l4);
        if let Some(udp) = unsafe { io::udp_rx(view, l4) } {
            let info = udp_sockets::Info {
                source_ip: udp.source_ip,
                dest_ip: udp.dest_ip,
                source_port: udp.source_port,
                dest_port: udp.dest_port,
                length: 0,
            };
            let mut reason = MaybeUninit::uninit();
            if unsafe {
                udp_sockets::r4os_rust_udp_socket_dispatch(
                    &info,
                    udp.data.as_ptr(),
                    udp.payload_len,
                    reason.as_mut_ptr(),
                )
            } {
                unsafe {
                    stats::r4os_rust_net_udp_owned_error(reason.assume_init());
                }
            }
            if (udp.dest_port == 68 || udp.source_port == 67)
                && !udp_sockets::r4os_rust_udp_socket_bound(udp.dest_port)
            {
                unsafe {
                    dhcp_client::r4os_rust_dhcp_client_message(udp.data.as_ptr(), udp.payload_len);
                }
            }
            if (udp.dest_port == 53 || udp.source_port == 53)
                && !udp_sockets::r4os_rust_udp_socket_bound(udp.dest_port)
            {
                unsafe {
                    dns_client::r4os_rust_dns_client_response(udp.data.as_ptr(), udp.payload_len);
                }
            }
        }
        unsafe {
            tcp_wire::rx(view, l4);
        }
        if unsafe { io::icmp_echo(view.payload.pointer, view.payload.len) } {
            unsafe {
                echo_reply(index, data, len, view);
            }
        }
        adapters::r4os_rust_net_adapter_rx(index, len);
        return true;
    }
    // Unsupported EtherTypes have no raw consumer; retain no diagnostic slot.
    adapters::r4os_rust_net_adapter_rx(index, len);
    true
}
fn echo_target(target: [u8; 4]) -> u8 {
    if count() == 0 {
        return 1;
    }
    let mut dest_mac = [0; 6];
    let route = resolve(0, target, &mut dest_mac);
    if route != 0 {
        return route;
    }
    let mut payload = [0; 32];
    let params = builders::WireIcmpBuild {
        ident: 0x1234,
        seq: stats::r4os_rust_net_icmp_sequence(),
    };
    let Some(length) =
        (unsafe { io::build_icmp_request(params, payload.as_mut_ptr(), payload.len()) })
    else {
        return 4;
    };
    stats::r4os_rust_net_icmp_sequence_advance();
    let mut frame = [0; 60];
    let params = builders::WireIpv4Build {
        source_mac: mac(0),
        dest_mac,
        source_ip: config::settings().local_ip,
        dest_ip: target,
        protocol: 1,
    };
    let Some(length) = (unsafe {
        io::build_ipv4(
            params,
            payload.as_ptr(),
            length,
            frame.as_mut_ptr(),
            frame.len(),
        )
    }) else {
        return 4;
    };
    let result = unsafe { transmit(0, frame.as_ptr(), length) };
    if result == 0 {
        poll(32, true);
    }
    result
}
fn udp_port(handle: u32) -> Option<u16> {
    let mut port = 0;
    if unsafe { udp_sockets::r4os_rust_udp_socket_port(handle, &mut port) } {
        Some(port)
    } else {
        None
    }
}
unsafe fn udp_send(handle: u32, target: [u8; 4], port: u16, data: *const u8, len: usize) -> u8 {
    let Some(source_port) = udp_port(handle) else {
        udp_error(b"send-handle");
        return 6;
    };
    if port == 0 {
        udp_error(b"send-port");
        return 6;
    }
    let mut packet = [0; 1536];
    let params = builders::WireUdpBuild {
        source_ip: config::settings().local_ip,
        dest_ip: target,
        source_port,
        dest_port: port,
    };
    let Some(length) =
        (unsafe { io::build_udp(params, data, len, packet.as_mut_ptr(), packet.len()) })
    else {
        udp_error(b"send-build");
        return 4;
    };
    let result = unsafe { send_ipv4(target, 17, packet.as_ptr(), length) };
    if result == 0 {
        udp_sockets::r4os_rust_udp_socket_note_tx(handle);
    }
    result
}
unsafe fn udp_dhcp(
    handle: u32,
    index: usize,
    source: [u8; 4],
    target: [u8; 4],
    data: *const u8,
    len: usize,
) -> u8 {
    let Some(source_port) = udp_port(handle) else {
        udp_error(b"send-handle");
        return 6;
    };
    if index >= count() {
        udp_error(b"send-adapter");
        return 1;
    }
    let mut packet = [0; 1536];
    let params = builders::WireUdpBuild {
        source_ip: source,
        dest_ip: target,
        source_port,
        dest_port: 67,
    };
    let Some(length) =
        (unsafe { io::build_udp(params, data, len, packet.as_mut_ptr(), packet.len()) })
    else {
        udp_error(b"send-build");
        return 4;
    };
    let mut dest_mac = [255; 6];
    if target != [255; 4] {
        let result = resolve(index, target, &mut dest_mac);
        if result != 0 {
            return result;
        }
    }
    let mut frame = [0; 1536];
    let params = builders::WireIpv4Build {
        source_mac: mac(index),
        dest_mac,
        source_ip: source,
        dest_ip: target,
        protocol: 17,
    };
    let Some(length) = (unsafe {
        io::build_ipv4(
            params,
            packet.as_ptr(),
            length,
            frame.as_mut_ptr(),
            frame.len(),
        )
    }) else {
        udp_error(b"send-build");
        return 4;
    };
    let result = unsafe { transmit(index, frame.as_ptr(), length) };
    if result == 0 {
        udp_sockets::r4os_rust_udp_socket_note_tx(handle);
        poll(512, false);
    }
    result
}
/// # Safety
/// Singleton initialization; both peer records have immutable kernel lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_frames_reset(input: *const Peers) {
    let _guard = Guard::acquire(2);
    unsafe {
        PEERS.0.get().write(input);
    }
}
/// # Safety
/// Retained readable frame and resident metadata through dispatch/waits; caller
/// keeps backend/claim admission until processing acknowledges completion.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_frames_receive(
    index: usize,
    data: *const u8,
    len: usize,
    metadata: *const rx_handoff::Metadata,
) -> bool {
    let metadata = unsafe { *metadata };
    unsafe { received(index, data, len, metadata) }
}
/// # Safety
/// Retained readable frame throughout backend/protocol work and cleanup.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_frames_transmit(
    index: usize,
    data: *const u8,
    len: usize,
) -> u8 {
    unsafe { transmit(index, data, len) }
}
/// # Safety
/// Exact readable target and retained readable payload through transport/waits.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_frames_send_ipv4(
    target: *const [u8; 4],
    protocol: u8,
    data: *const u8,
    len: usize,
) -> u8 {
    let target = unsafe { *target };
    unsafe { send_ipv4(target, protocol, data, len) }
}
/// # Safety
/// Exact readable target through synchronous packet construction and send.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_frames_send_arp(index: usize, target: *const [u8; 4]) -> u8 {
    send_arp(index, unsafe { *target })
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_frames_arp_gateway() -> u8 {
    if count() == 0 {
        return 1;
    }
    let result = send_arp(0, config::settings().gateway_ip);
    if result == 0 {
        unsafe {
            stats::r4os_rust_net_arp_wait(&config::settings().gateway_ip, peers().arp);
        }
    }
    result
}
/// # Safety
/// Exact readable target through routing, packet construction and send/waits.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_frames_echo(target: *const [u8; 4]) -> u8 {
    echo_target(unsafe { *target })
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_frames_udp_bind(port: u16) -> i32 {
    let mut reason = MaybeUninit::uninit();
    let result = unsafe { udp_sockets::r4os_rust_udp_socket_bind(port, reason.as_mut_ptr()) };
    unsafe {
        stats::r4os_rust_net_udp_owned_error(reason.assume_init());
    }
    result
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_frames_udp_close(handle: u32) -> i32 {
    let mut reason = MaybeUninit::uninit();
    let result = unsafe { udp_sockets::r4os_rust_udp_socket_close(handle, reason.as_mut_ptr()) };
    unsafe {
        stats::r4os_rust_net_udp_owned_error(reason.assume_init());
    }
    result
}
/// # Safety
/// Readable target/payload retained through routing, R4P and backend work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_frames_udp_send(
    handle: u32,
    target: *const [u8; 4],
    port: u16,
    data: *const u8,
    len: usize,
) -> u8 {
    let target = unsafe { *target };
    unsafe { udp_send(handle, target, port, data, len) }
}
/// # Safety
/// Readable addresses/payload retained through routing, R4P and backend work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_frames_udp_dhcp(
    handle: u32,
    index: usize,
    source: *const [u8; 4],
    target: *const [u8; 4],
    data: *const u8,
    len: usize,
) -> u8 {
    let (source, target) = unsafe { (*source, *target) };
    unsafe { udp_dhcp(handle, index, source, target, data, len) }
}
/// # Safety
/// Separate exclusive writable caller info/payload retained through any fault.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_frames_udp_recv(
    handle: u32,
    info: *mut udp_sockets::Info,
    data: *mut u8,
    capacity: usize,
) -> i32 {
    let mut reason = MaybeUninit::uninit();
    let result = unsafe {
        udp_sockets::r4os_rust_udp_socket_recv(handle, info, data, capacity, reason.as_mut_ptr())
    };
    unsafe {
        stats::r4os_rust_net_udp_owned_error(reason.assume_init());
    }
    if result == -2 {
        stats::r4os_rust_net_buffer_small_note();
    }
    result
}

const _: () = {
    assert!(core::mem::size_of::<Peers>() == 16);
    assert!(core::mem::align_of::<Peers>() == 8);
    assert!(core::mem::offset_of!(Peers, rx) == 0);
    assert!(core::mem::offset_of!(Peers, arp) == 8);
};
