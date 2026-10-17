//! Sole R4NET platform implementation. Canonical tables, slots and layouts do
//! not change. Foreign buffers stay raw through transport/IPC waits; queries
//! build resident copied records and publish only to the caller's exact output.
use super::{network_projection as project, system::io as system_io};
use crate::{
    contract::*,
    fs::vfs_types::Bytes,
    net::{
        adapter_runtime, adapters, app_ipv4, cleanup, config, config_writer, dhcp_client,
        diagnostics, dns_client, frames, ipc_net as shared, ipc_services, packet_pool,
        protocol_runtime, protocol_stats, rx_runtime, tcp, tcp_client, telemetry, timing,
        udp_runtime, udp_sockets,
    },
    sched::sync_types::Text,
    service::{ipc, ipc_types as channel},
};
use core::{
    mem::{self, MaybeUninit},
    ptr, slice,
};
use project::*;
pub(super) trait TextView {
    fn view(&self) -> &[u8];
}
impl TextView for Text {
    fn view(&self) -> &[u8] {
        if self.len == 0 {
            &[]
        } else {
            unsafe { slice::from_raw_parts(self.pointer, self.len) }
        }
    }
}
impl TextView for &[u8] {
    fn view(&self) -> &[u8] {
        self
    }
}
impl<const N: usize> TextView for [u8; N] {
    fn view(&self) -> &[u8] {
        self
    }
}
impl<const N: usize> TextView for &[u8; N] {
    fn view(&self) -> &[u8] {
        &self[..]
    }
}
pub(super) fn copy_fixed(out: &mut [u8], value: impl TextView) {
    out.fill(0);
    if out.is_empty() {
        return;
    }
    let value = value.view();
    let count = value.len().min(out.len() - 1);
    out[..count].copy_from_slice(&value[..count]);
}
pub(super) fn link_name(value: u8) -> &'static [u8] {
    match value {
        0 => b"unknown",
        1 => b"down",
        2 => b"up",
        _ => panic!("Canonical link value"),
    }
}
fn lifecycle_name(value: u8) -> &'static [u8] {
    match value {
        0 => b"unknown",
        1 => b"registered",
        2 => b"active",
        3 => b"link-down",
        4 => b"resetting",
        5 => b"shutdown",
        _ => panic!("Canonical lifecycle value"),
    }
}
fn poll(rounds: usize) {
    unsafe {
        rx_runtime::r4os_rust_net_rx_runtime_poll(rounds, 255, frames::peers().rx);
    }
}

/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_ipc_open(channel: u32) -> i32 {
    ipc::open(channel)
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_ipc_send(channel: u32, data: *const u8, len: u32) -> i32 {
    if len as usize > ipc::MAX_MESSAGE_SIZE {
        return -1;
    }
    unsafe {
        ipc::send(
            channel,
            Bytes {
                data,
                len: len as usize,
            },
        )
    }
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_ipc_recv(channel: u32, out: *mut u8, len: u32) -> i32 {
    if len as usize > ipc::MAX_MESSAGE_SIZE {
        return -1;
    }
    unsafe { ipc::recv(channel, out, len as usize) }
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_ipc_poll(channel: u32) -> i32 {
    ipc::poll_channel(channel)
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_ipc_close(channel: u32) -> i32 {
    ipc::close_channel(channel)
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_ipc_summary(out: *mut IpcSummary) -> i32 {
    let mut s = channel::Summary::EMPTY;
    let result = unsafe { ipc::r4os_rust_ipc_summary(&mut s) };
    if result < 0 {
        return result;
    }
    unsafe {
        out.write(IpcSummary {
            max_channels: s.max_channels,
            active_channels: s.active_channels,
            max_message_size: s.max_message_size,
            queue_depth: s.queue_depth,
            sends: s.sends,
            receives: s.receives,
            drops: s.drops,
            errors: s.errors,
            echo_tests: s.echo_tests,
        });
    }
    result
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_ipc_channel(
    channel: u32,
    out: *mut IpcChannelInfo,
) -> i32 {
    let mut s = channel::ChannelInfo::EMPTY;
    let result = unsafe { ipc::r4os_rust_ipc_channel(channel, &mut s) };
    unsafe {
        out.write(IpcChannelInfo {
            id: s.id,
            active: s.active,
            queued: s.queued,
            queue_depth: s.queue_depth,
            max_message_size: s.max_message_size,
            has_handler: s.has_handler,
            opens: s.opens,
            closes: s.closes,
            sends: s.sends,
            receives: s.receives,
            drops: s.drops,
            name: s.name,
            ..IpcChannelInfo::default()
        });
    }
    result
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_net_service_request(
    channel: u32,
    op: u16,
    id: u32,
    client: u16,
    data: *const u8,
    len: u32,
    out: *mut u8,
    capacity: u32,
) -> i32 {
    if len as usize > ipc::MAX_MESSAGE_SIZE - super::super::net::ipc_types::HEADER_SIZE {
        return -1;
    }
    if capacity as usize > ipc::MAX_MESSAGE_SIZE {
        return -1;
    }
    unsafe {
        ipc_services::r4os_rust_net_ipc_request(
            channel,
            op,
            id,
            client,
            data,
            len as usize,
            out,
            capacity as usize,
        )
    }
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_ipc_performance(
    channel: u32,
    out: *mut IpcPerformanceSummary,
) -> i32 {
    let (version, size) = unsafe {
        (
            ptr::addr_of!((*out).version).read(),
            ptr::addr_of!((*out).size).read() as usize,
        )
    };
    let header = mem::offset_of!(IpcPerformanceSummary, worker_started);
    let required = mem::size_of::<IpcPerformanceSummary>();
    if version == 0 || size < header {
        return -1;
    }
    if size < required {
        unsafe {
            ptr::addr_of_mut!((*out).version).write(1);
            ptr::addr_of_mut!((*out).size).write(required as u32);
        }
        return -1;
    }
    let mut s = channel::PerformanceSummary::EMPTY;
    let result = unsafe { ipc::r4os_rust_ipc_performance(channel, true, &mut s) };
    if result < 0 {
        return result;
    }
    let value = project::project_ipc_performance(s);
    unsafe {
        out.write(value);
    }
    result
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_tcp_connect(a: u8, b: u8, c: u8, d: u8, port: u16) -> i32 {
    unsafe { tcp_client::r4os_rust_tcp_client_connect(&[a, b, c, d], port, false, 0) }
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_tcp_write(id: u32, data: *const u8, len: u32) -> i32 {
    if len as usize > tcp::BUFFER_SIZE {
        return -1;
    }
    unsafe { tcp_client::r4os_rust_tcp_client_write(id, data, len as usize) }
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_tcp_read(id: u32, out: *mut u8, len: u32) -> i32 {
    if len as usize > tcp::BUFFER_SIZE {
        return -1;
    }
    unsafe { tcp_client::r4os_rust_tcp_client_read(id, out, len as usize) }
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_tcp_echo_listen_once(
    port: u16,
    out: *mut u8,
    len: u32,
) -> i32 {
    if len as usize > tcp::BUFFER_SIZE {
        return -1;
    }
    unsafe { tcp_client::r4os_rust_tcp_client_echo_once(port, out, len as usize) }
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_tcp_accept_read_once(
    port: u16,
    out: *mut u8,
    len: u32,
    result: *mut TcpAcceptResult,
) -> i32 {
    unsafe {
        result.write(TcpAcceptResult::default());
    }
    if len as usize > tcp::BUFFER_SIZE {
        return -1;
    }
    let mut id = 0;
    let got = unsafe {
        tcp_client::r4os_rust_tcp_client_accept_read_once(port, out, len as usize, &mut id)
    };
    if got > 0 {
        unsafe {
            result.write(TcpAcceptResult {
                conn_id: id,
                bytes: got as u32,
            });
        }
    }
    got
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_tcp_close(id: u32) -> i32 {
    tcp_client::r4os_rust_tcp_client_close(id)
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_tcp_summary(out: *mut TcpSummary) -> i32 {
    let value = shared::capture(tcp::r4os_rust_net_tcp_summary);
    unsafe {
        out.write(value);
    }
    1
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_tcp_connection(
    index: u32,
    out: *mut TcpConnectionInfo,
) -> i32 {
    let mut info = TcpConnectionInfo::default();
    let result = unsafe { tcp::r4os_rust_net_tcp_connection_info(index, &mut info) };
    if result <= 0 {
        return result;
    }
    let mut value = TcpConnectionInfo::default();
    project::fill_tcp_connection_info(&mut value, info);
    unsafe {
        out.write(value);
    }
    result
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_tcp_performance(out: *mut TcpPerformanceInfo) -> i32 {
    let (version, size) = unsafe {
        (
            ptr::addr_of!((*out).version).read(),
            ptr::addr_of!((*out).size).read() as usize,
        )
    };
    let header = mem::offset_of!(TcpPerformanceInfo, local_mss);
    let required = mem::size_of::<TcpPerformanceInfo>();
    if version == 0 || size < header {
        return -1;
    }
    if size < required {
        unsafe {
            ptr::addr_of_mut!((*out).version).write(1);
            ptr::addr_of_mut!((*out).size).write(required as u32);
        }
        return -1;
    }
    let s = shared::capture(telemetry::r4os_rust_net_telemetry_tcp_performance);
    let value = project::project_tcp_performance(s);
    unsafe {
        out.write(value);
    }
    1
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_net_ipv4_send(
    a: u8,
    b: u8,
    c: u8,
    d: u8,
    protocol: u8,
    data: *const u8,
    len: u32,
) -> i32 {
    if len as usize > packet_pool::MAX_FRAME {
        return 4;
    }
    i32::from(unsafe {
        frames::r4os_rust_frames_send_ipv4(&[a, b, c, d], protocol, data, len as usize)
    })
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_net_ipv4_recv(
    protocol: u8,
    out: *mut NetIpv4Packet,
    data: *mut u8,
    len: u32,
) -> i32 {
    if len as usize > app_ipv4::PAYLOAD_MAX {
        return -1;
    }
    poll(4096);
    let mut info = app_ipv4::Info::EMPTY;
    let result =
        unsafe { app_ipv4::r4os_rust_app_ipv4_read(protocol, &mut info, data, len as usize) };
    unsafe {
        out.write(NetIpv4Packet {
            source_ip: info.source_ip,
            dest_ip: info.dest_ip,
            protocol: info.protocol,
            truncated: u8::from(info.truncated),
            reserved: 0,
            payload_len: info.payload_len,
        });
    }
    result
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_udp_bind(port: u16) -> i32 {
    frames::r4os_rust_frames_udp_bind(port)
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_udp_send_to(
    id: u32,
    a: u8,
    b: u8,
    c: u8,
    d: u8,
    port: u16,
    data: *const u8,
    len: u32,
) -> i32 {
    if len as usize > packet_pool::MAX_FRAME {
        return 4;
    }
    i32::from(unsafe {
        frames::r4os_rust_frames_udp_send(id, &[a, b, c, d], port, data, len as usize)
    })
}
fn udp_projection(info: udp_sockets::Info) -> UdpRecvInfo {
    UdpRecvInfo {
        source_ip: info.source_ip,
        dest_ip: info.dest_ip,
        source_port: info.source_port,
        dest_port: info.dest_port,
        length: info.length,
        reserved: 0,
    }
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_udp_recv_from(
    id: u32,
    out: *mut UdpRecvInfo,
    data: *mut u8,
    len: u32,
) -> i32 {
    if len as usize > packet_pool::MAX_FRAME {
        return -1;
    }
    poll(4096);
    let mut info = unsafe { mem::zeroed::<udp_sockets::Info>() };
    let result = unsafe { frames::r4os_rust_frames_udp_recv(id, &mut info, data, len as usize) };
    unsafe {
        out.write(udp_projection(info));
    }
    result
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_udp_recv_from_wait(
    id: u32,
    out: *mut UdpRecvInfo,
    data: *mut u8,
    len: u32,
    timeout: u64,
) -> i32 {
    if len as usize > packet_pool::MAX_FRAME {
        return -1;
    }
    let mut info = unsafe { mem::zeroed::<udp_sockets::Info>() };
    let result = unsafe {
        udp_runtime::r4os_rust_udp_recv_wait(
            id,
            &mut info,
            data,
            len as usize,
            timeout,
            &udp_runtime::STANDARD_PEERS,
        )
    };
    unsafe {
        out.write(udp_projection(info));
    }
    result
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_udp_close(id: u32) -> i32 {
    frames::r4os_rust_frames_udp_close(id)
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_udp_status(out: *mut UdpStatus) -> i32 {
    let status = shared::udp_status();
    let mut value = UdpStatus {
        active_sockets: status.active_sockets,
        max_sockets: status.max_sockets,
        queued_packets: status.queued_packets,
        queue_limit: status.queue_limit,
        payload_max: status.payload_max,
        reserved: 0,
        delivered: status.delivered,
        drops: status.drops,
        ..UdpStatus::default()
    };
    copy_fixed(&mut value.last_error, status.last_error);
    unsafe {
        out.write(value);
    }
    1
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_serial_link_status(_out: *mut SerialLinkStatus) -> i32 {
    // Reserved published ABI slot. No buffer access, UART, loader or work.
    crate::contract::err_no_fn
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_serial_link_poll() -> i32 {
    // Reserved published ABI slot. No buffer access, UART, loader or work.
    crate::contract::err_no_fn
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_serial_link_send_message(
    _data: *const u8,
    _len: u32,
) -> i32 {
    // Reserved published ABI slot. No buffer access, UART, loader or work.
    crate::contract::err_no_fn
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_serial_link_host_test() -> i32 {
    // Reserved published ABI slot. No buffer access, UART, loader or work.
    crate::contract::err_no_fn
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_serial_link_inbox(_out: *mut SerialLinkMessage) -> i32 {
    // Reserved published ABI slot. No buffer access, UART, loader or work.
    crate::contract::err_no_fn
}
fn trim(mut value: &[u8]) -> &[u8] {
    while matches!(value.first(), Some(b' ' | b'\t')) {
        value = &value[1..];
    }
    while matches!(value.last(), Some(b' ' | b'\t')) {
        value = &value[..value.len() - 1];
    }
    value
}
fn fixed(value: &[u8]) -> &[u8] {
    trim(&value[..value.iter().position(|&b| b == 0).unwrap_or(value.len())])
}
fn parse_ip(value: &[u8]) -> Option<[u8; 4]> {
    let value = fixed(value);
    if value.is_empty() {
        None
    } else {
        config::parse_ipv4(value)
    }
}
fn valid_mask(mask: [u8; 4]) -> bool {
    let value = u32::from_be_bytes(mask);
    if value == 0 {
        return false;
    }
    let mut seen_zero = false;
    for bit in 0..32 {
        if value & (0x8000_0000u32 >> bit) == 0 {
            seen_zero = true;
        } else if seen_zero {
            return false;
        }
    }
    true
}
fn persist(addresses: config_writer::Addresses) -> i32 {
    let mut input = [0; 4096];
    let mut output = [0; 4096];
    // Original R4SYS request/lane, invalidation and file transaction; only the
    // rewrite callback is now a typed Rust call using the same native codec.
    let result = unsafe {
        system_io::rewrite_with(
            super::registry_format_types::Bytes {
                data: b"C:\\CONFIG.R4S".as_ptr(),
                len: 13,
            },
            input.as_mut_ptr(),
            input.len(),
            output.as_mut_ptr(),
            output.len(),
            |source, dest, capacity| {
                let source = source.slice();
                let output = slice::from_raw_parts_mut(dest, capacity);
                config_writer::rewrite_config(source, output, addresses)
            },
        )
    };
    if result == -5 {
        NET_CONFIG_BUFFER_SMALL
    } else if result < 0 {
        NET_CONFIG_WRITE_FAILED
    } else {
        NET_CONFIG_OK
    }
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_net_config_set(request: *const NetConfigRequest) -> i32 {
    let request = unsafe { request.read() };
    if request.flags & (NET_CONFIG_FLAG_WRITE_PERSISTENT | NET_CONFIG_FLAG_APPLY_LIVE) == 0 {
        return NET_CONFIG_UNSUPPORTED;
    }
    let Some(local_ip) = parse_ip(&request.local_ip) else {
        return NET_CONFIG_INVALID_IP;
    };
    let Some(netmask) = parse_ip(&request.netmask) else {
        return NET_CONFIG_INVALID_IP;
    };
    if !valid_mask(netmask) {
        return NET_CONFIG_INVALID_IP;
    }
    let Some(gateway_ip) = parse_ip(&request.gateway_ip) else {
        return NET_CONFIG_INVALID_IP;
    };
    let dns_text = fixed(&request.dns_ip);
    let dns_configured = !dns_text.is_empty();
    let dns_ip = if dns_configured {
        let Some(ip) = config::parse_ipv4(dns_text) else {
            return NET_CONFIG_INVALID_IP;
        };
        ip
    } else {
        [0; 4]
    };
    if request.flags & NET_CONFIG_FLAG_WRITE_PERSISTENT != 0 {
        let result = persist(config_writer::Addresses {
            local_ip,
            netmask,
            gateway_ip,
            dns_ip,
            dns_configured,
        });
        if result != NET_CONFIG_OK {
            return result;
        }
    }
    if request.flags & NET_CONFIG_FLAG_APPLY_LIVE != 0 {
        config::apply_addresses(
            config_writer::Addresses {
                local_ip,
                netmask,
                gateway_ip,
                dns_ip,
                dns_configured,
            },
            false,
        );
        unsafe {
            dhcp_client::r4os_rust_dhcp_client_cancel(Text::literal(b"static-config"));
        }
        protocol_stats::r4os_rust_net_arp_cache_flush_owned();
        if adapters::r4os_rust_net_adapter_count() == 0 {
            return NET_CONFIG_NO_ADAPTER;
        }
    }
    NET_CONFIG_OK
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_net_config_get(out: *mut NetConfigSnapshot) -> i32 {
    let value = config_snapshot();
    unsafe {
        out.write(value);
    }
    NET_CONFIG_OK
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_net_dns_resolve(
    data: *const u8,
    len: u32,
    out: *mut [u8; 4],
) -> i32 {
    if len > 255 {
        return -9;
    }
    unsafe { dns_client::r4os_rust_dns_client_resolve(data, len as usize, out) }
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_net_dns_resolve_server(
    a: u8,
    b: u8,
    c: u8,
    d: u8,
    data: *const u8,
    len: u32,
    out: *mut [u8; 4],
) -> i32 {
    if len > 255 {
        return -9;
    }
    unsafe {
        dns_client::r4os_rust_dns_client_resolve_server(data, len as usize, &[a, b, c, d], out)
    }
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_net_dhcp_acquire() -> i32 {
    i32::from(dhcp_client::r4os_rust_dhcp_client_acquire_default(false, 0))
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_net_dhcp_renew() -> i32 {
    i32::from(dhcp_client::r4os_rust_dhcp_client_renew_default(false, 0))
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_net_dhcp_release() -> i32 {
    i32::from(dhcp_client::r4os_rust_dhcp_client_release_default())
}
fn config_snapshot() -> NetConfigSnapshot {
    let cfg = config::settings();
    let mut value = NetConfigSnapshot {
        local_ip: cfg.local_ip,
        netmask: cfg.netmask,
        gateway_ip: cfg.gateway_ip,
        dns_ip: cfg.dns_ip,
        flags: flag(cfg.configured, NET_CONFIG_FLAG_CONFIGURED)
            | flag(cfg.dns_configured, NET_CONFIG_FLAG_DNS_CONFIGURED),
        adapter_count: adapters::r4os_rust_net_adapter_count() as u32,
        invalid_options: cfg.invalid_options.min(u64::from(u32::MAX)) as u32,
        ..NetConfigSnapshot::default()
    };
    copy_fixed(&mut value.source, config::source_name());
    copy_fixed(&mut value.link, b"none");
    copy_fixed(&mut value.last_error, cfg.last_error);
    // Retain the exact backend/name lifetime until copied text is resident.
    let _callback = crate::net::backend_admission::CallbackGuard::acquire();
    if _callback.is_some()
        && let Some(a) = adapters::snapshot(0)
    {
        value.flags |= NET_CONFIG_FLAG_ADAPTER_PRESENT;
        if a.link == 2 {
            value.flags |= NET_CONFIG_FLAG_LINK_UP;
        }
        value.mac = a.mac;
        value.mtu = a.mtu;
        copy_fixed(&mut value.adapter_name, a.name);
        copy_fixed(&mut value.link, link_name(a.link));
        copy_fixed(&mut value.last_error, a.stats.last_error);
    }
    value
}
fn flag(condition: bool, bit: u32) -> u32 {
    if condition { bit } else { 0 }
}
fn dhcp_status() -> DhcpStatus {
    let stats = shared::dhcp_stats();
    let runtime = shared::dhcp_runtime_status();
    let link_up = adapters::snapshot(0).is_some_and(|a| a.link == 2);
    let mut value = DhcpStatus {
        discover_tx: stats.discover_tx,
        offer_rx: stats.offer_rx,
        request_tx: stats.request_tx,
        ack_rx: stats.ack_rx,
        nak_rx: stats.nak_rx,
        release_tx: stats.release_tx,
        retries: stats.retries,
        timeouts: stats.timeouts,
        release_errors: stats.release_errors,
        malformed: stats.malformed,
        self_tests: stats.self_tests,
        xid: stats.lease.xid,
        offered_ip: stats.lease.offered_ip,
        server_ip: stats.lease.server_ip,
        netmask: stats.lease.netmask,
        gateway_ip: stats.lease.gateway_ip,
        dns_ip: stats.lease.dns_ip,
        lease_seconds: stats.lease.lease_seconds,
        renew_seconds: stats.lease.renew_seconds,
        rebind_seconds: stats.lease.rebind_seconds,
        flags: flag(stats.lease.bound, DHCP_STATUS_FLAG_BOUND)
            | flag(stats.lease.dns_configured, DHCP_STATUS_FLAG_DNS_CONFIGURED)
            | flag(
                stats.operation_pending || runtime.operation_active,
                DHCP_STATUS_FLAG_PENDING,
            )
            | flag(config::settings().dhcp_enabled, DHCP_STATUS_FLAG_DESIRED)
            | flag(runtime.task_started, DHCP_STATUS_FLAG_TASK_STARTED)
            | flag(link_up, DHCP_STATUS_FLAG_LINK_UP)
            | flag(
                runtime.state == crate::net::dhcp_runtime::State::RetryWait,
                DHCP_STATUS_FLAG_RETRY_WAIT,
            ),
        last_attempt: stats.last_attempt,
        last_type: stats.last_type,
        runtime_state: runtime.state as u16,
        ..DhcpStatus::default()
    };
    copy_fixed(&mut value.last_error, stats.last_error);
    value
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_net_dhcp_status(out: *mut DhcpStatus) -> i32 {
    let value = dhcp_status();
    unsafe {
        out.write(value);
    }
    1
}
fn detail(index: u32) -> (i32, NetDetailSnapshot) {
    let mut out = NetDetailSnapshot {
        adapter_index: index,
        ..NetDetailSnapshot::default()
    };
    out.config = config_snapshot();
    out.dhcp = dhcp_status();
    let dhcp_runtime = shared::dhcp_runtime_status();
    out.link_generation = dhcp_runtime.link_generation;
    out.tcp = shared::capture(tcp::r4os_rust_net_tcp_summary);
    out.flags = flag(
        out.config.flags & NET_CONFIG_FLAG_DNS_CONFIGURED != 0,
        NET_DETAIL_FLAG_DNS_CONFIGURED,
    ) | flag(
        out.dhcp.flags & DHCP_STATUS_FLAG_BOUND != 0,
        NET_DETAIL_FLAG_DHCP_BOUND,
    ) | flag(
        out.dhcp.flags & DHCP_STATUS_FLAG_DNS_CONFIGURED != 0,
        NET_DETAIL_FLAG_DHCP_DNS_CONFIGURED,
    );
    let eth = shared::capture(protocol_stats::r4os_rust_net_ethernet_owned_stats);
    out.ethernet = NetDetailEthernet {
        rx: eth.rx,
        tx: eth.tx,
        broadcast: eth.broadcast,
        own_unicast: eth.own_unicast,
        dropped_short: eth.dropped_short,
        dropped_filter: eth.dropped_filter,
        unknown_ethertype: eth.unknown_ethertype,
        test_frames: eth.test_frames,
        last_ethertype: eth.last_ethertype,
        ..NetDetailEthernet::default()
    };
    copy_fixed(&mut out.ethernet.last_error, eth.last_error);

    let arp = {
        let mut s = MaybeUninit::uninit();
        unsafe {
            protocol_stats::r4os_rust_net_arp_owned_stats(s.as_mut_ptr(), true);
            s.assume_init()
        }
    };
    if arp.cache_valid {
        out.flags |= NET_DETAIL_FLAG_ARP_CACHE_VALID;
    }
    out.arp = NetDetailArp {
        requests_tx: arp.requests_tx,
        replies_tx: arp.replies_tx,
        replies_rx: arp.replies_rx,
        requests_rx: arp.requests_rx,
        malformed: arp.malformed,
        cache_updates: arp.cache_updates,
        cache_hits: arp.cache_hits,
        resolve_attempts: arp.resolve_attempts,
        resolve_retries: arp.resolve_retries,
        resolve_timeouts: arp.resolve_timeouts,
        resolve_misses: arp.resolve_misses,
        pending_packets: arp.pending_packets,
        pending_timeouts: arp.pending_timeouts,
        pending_drops: arp.pending_drops,
        pending_queue_limit: arp.pending_queue_limit,
        last_opcode: arp.last_opcode,
        cache_ip: arp.cache_ip,
        cache_mac: arp.cache_mac,
        cache_age_ticks: protocol_stats::r4os_rust_net_arp_cache_age_owned(),
        cache_ttl_ticks: timing::DEFAULT_ARP_CACHE_TTL_TICKS,
        ..NetDetailArp::default()
    };
    copy_fixed(&mut out.arp.last_error, arp.last_error);

    let ip = shared::capture(protocol_stats::r4os_rust_net_ipv4_owned_stats);
    out.ipv4 = NetDetailIpv4 {
        rx_packets: ip.rx_packets,
        tx_packets: ip.tx_packets,
        dropped_short: ip.dropped_short,
        dropped_version: ip.dropped_version,
        dropped_checksum: ip.dropped_checksum,
        dropped_fragment: ip.dropped_fragment,
        dropped_destination: ip.dropped_destination,
        dropped_tx_too_large: ip.dropped_tx_too_large,
        malformed: ip.malformed,
        last_protocol: ip.last_protocol,
        last_source: ip.last_source,
        last_dest: ip.last_dest,
        ..NetDetailIpv4::default()
    };
    copy_fixed(&mut out.ipv4.last_error, ip.last_error);

    let ic = shared::capture(protocol_stats::r4os_rust_net_icmp_owned_stats);
    out.icmp = NetDetailIcmp {
        rx_packets: ic.rx_packets,
        tx_packets: ic.tx_packets,
        echo_requests_rx: ic.echo_requests_rx,
        echo_replies_rx: ic.echo_replies_rx,
        echo_requests_tx: ic.echo_requests_tx,
        echo_replies_tx: ic.echo_replies_tx,
        destination_unreachable_rx: ic.destination_unreachable_rx,
        port_unreachable_rx: ic.port_unreachable_rx,
        time_exceeded_rx: ic.time_exceeded_rx,
        malformed: ic.malformed,
        checksum_errors: ic.checksum_errors,
        last_type: ic.last_type,
        last_code: ic.last_code,
        last_id: ic.last_id,
        last_seq: ic.last_seq,
        ..NetDetailIcmp::default()
    };
    copy_fixed(&mut out.icmp.last_error, ic.last_error);

    let udp = shared::capture(protocol_stats::r4os_rust_net_udp_owned_stats);
    out.udp = NetDetailUdp {
        rx_packets: udp.rx_packets,
        tx_packets: udp.tx_packets,
        dropped_short: udp.dropped_short,
        dropped_length: udp.dropped_length,
        checksum_errors: udp.checksum_errors,
        malformed: udp.malformed,
        dhcp_rx: udp.dhcp_rx,
        dns_rx: udp.dns_rx,
        self_tests: udp.self_tests,
        last_source_port: udp.last_source_port,
        last_dest_port: udp.last_dest_port,
        ..NetDetailUdp::default()
    };
    copy_fixed(&mut out.udp.last_error, udp.last_error);

    let dns = shared::dns_stats();
    out.dns = NetDetailDns {
        queries_tx: dns.queries_tx,
        responses_rx: dns.responses_rx,
        a_records: dns.a_records,
        resolve_requests: dns.resolve_requests,
        timeouts: dns.timeouts,
        nxdomain: dns.nxdomain,
        tx_errors: dns.tx_errors,
        malformed: dns.malformed,
        self_tests: dns.self_tests,
        last_id: dns.last_id,
        last_result: dns.last_result,
        last_server: dns.last_server,
        last_answer: dns.last_answer,
        ..NetDetailDns::default()
    };
    copy_fixed(&mut out.dns.last_error, dns.last_error);

    let tcp_stats = shared::tcp_stats();
    copy_fixed(&mut out.tcp_last_error, tcp_stats.last_error);
    for kind in 0..8 {
        let mut stats = MaybeUninit::uninit();
        unsafe {
            protocol_runtime::r4os_rust_net_protocol_runtime(kind, stats.as_mut_ptr());
        }
        project::fill_net_detail_runtime(&mut out.protocols[usize::from(kind)], unsafe {
            stats.assume_init()
        });
    }
    for conn_index in 0..NET_DETAIL_MAX_TCP_CONNECTIONS {
        let mut info = TcpConnectionInfo::default();
        if unsafe { tcp::r4os_rust_net_tcp_connection_info(conn_index as u32, &mut info) } <= 0 {
            continue;
        }
        let at = out.tcp_connection_count as usize;
        if at >= NET_DETAIL_MAX_TCP_CONNECTIONS {
            break;
        }
        project::fill_tcp_connection_info(&mut out.tcp_connections[at], info);
        out.tcp_connection_count += 1;
    }
    let index_usize = index as usize;
    let Some(_callback) = crate::net::backend_admission::CallbackGuard::acquire() else {
        return (0, out);
    };
    let mut backend = MaybeUninit::uninit();
    let backend_ok = unsafe {
        adapter_runtime::r4os_rust_net_adapter_runtime_refresh(
            index_usize,
            ptr::null(),
            backend.as_mut_ptr(),
        )
    };
    let Some(adapter) = adapters::snapshot(index_usize) else {
        return (0, out);
    };
    out.flags |= NET_DETAIL_FLAG_ADAPTER_PRESENT;
    if adapter.link == 2 {
        out.flags |= NET_DETAIL_FLAG_LINK_UP;
    }
    out.adapter = NetDetailAdapter {
        index,
        count: adapters::r4os_rust_net_adapter_count() as u32,
        flags: adapter.flags,
        lifecycle: u32::from(adapter.lifecycle),
        bus_no: adapter.bus_no,
        device_no: adapter.device_no,
        function_no: adapter.function_no,
        class_code: 0x02,
        subclass: 0x00,
        prog_if: 0,
        vendor_id: adapter.vendor_id,
        device_id: adapter.device_id,
        mac: adapter.mac,
        mtu: adapter.mtu,
        rx_packets: adapter.stats.rx_packets,
        tx_packets: adapter.stats.tx_packets,
        rx_bytes: adapter.stats.rx_bytes,
        tx_bytes: adapter.stats.tx_bytes,
        drops: adapter.stats.drops,
        errors: adapter.stats.errors,
        resets: adapter.stats.resets,
        registered_tick: adapter.registered_tick,
        state_changed_tick: adapter.state_changed_tick,
        dhcp_retry_round: u16::from(dhcp_runtime.retry_round),
        ..NetDetailAdapter::default()
    };
    copy_fixed(&mut out.adapter.name, adapter.name);
    copy_fixed(&mut out.adapter.driver, adapter.driver);
    copy_fixed(&mut out.adapter.link, link_name(adapter.link));
    copy_fixed(&mut out.adapter.state, lifecycle_name(adapter.lifecycle));
    copy_fixed(&mut out.adapter.last_error, adapter.stats.last_error);
    if backend_ok {
        let backend = unsafe { backend.assume_init() };
        out.flags |= NET_DETAIL_FLAG_BACKEND_STATUS;
        if backend.irq_registered != 0 {
            out.flags |= NET_DETAIL_FLAG_IRQ_REGISTERED;
        }
        out.adapter.backend_rx_packets = backend.rx_packets;
        out.adapter.backend_tx_packets = backend.tx_packets;
        out.adapter.backend_drops = backend.drops;
        out.adapter.backend_errors = backend.errors;
        out.adapter.irq_line = backend.irq_line;
        out.adapter.irq_pin = backend.irq_pin;
        out.adapter.irq_mode = backend.irq_mode;
        out.adapter.irq_registered = backend.irq_registered;
        out.adapter.irq_count = backend.irq_count;
        out.adapter.irq_handled = backend.irq_handled;
        out.adapter.poll_count = backend.poll_count;
        out.adapter.poll_fallbacks = backend.poll_fallbacks;
        out.adapter.last_isr = backend.last_isr;
    }
    (1, out)
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_net_detail_get(
    index: u32,
    out: *mut NetDetailSnapshot,
) -> i32 {
    let (result, value) = detail(index);
    unsafe {
        out.write(value);
    }
    result
}
fn error_probe() -> bool {
    let before = telemetry::errors();
    diagnostics::r4os_rust_net_diag_backpressure();
    unsafe {
        telemetry::r4os_rust_net_telemetry_log_errors(Text::literal(b"netdiag-errors"));
    }
    let after = telemetry::errors();
    after.total > before.total && after.tx_failures > before.tx_failures
}
fn run_diag(op: u32) -> i32 {
    let ok = match op {
        NET_DIAG_OP_TIMING => diagnostics::r4os_rust_net_diag_timing(),
        NET_DIAG_OP_BACKPRESSURE => diagnostics::r4os_rust_net_diag_backpressure(),
        NET_DIAG_OP_CLEANUP => diagnostics::r4os_rust_net_diag_cleanup(),
        NET_DIAG_OP_POWER => diagnostics::r4os_rust_net_diag_power(),
        NET_DIAG_OP_LIFECYCLE => diagnostics::r4os_rust_net_diag_link(),
        NET_DIAG_OP_RESET => diagnostics::r4os_rust_net_diag_reset(),
        NET_DIAG_OP_DRIVER => {
            diagnostics::r4os_rust_net_diag_driver()
                && diagnostics::r4os_rust_net_diag_rx()
                && diagnostics::r4os_rust_net_diag_capability()
        }
        NET_DIAG_OP_ENVIRONMENT => diagnostics::r4os_rust_net_diag_environment(),
        NET_DIAG_OP_LIMIT => diagnostics::r4os_rust_net_diag_limit(),
        NET_DIAG_OP_CORPUS => diagnostics::r4os_rust_net_diag_corpus(),
        NET_DIAG_OP_NEGATIVE => diagnostics::r4os_rust_net_diag_negative(),
        NET_DIAG_OP_R4P => diagnostics::r4os_rust_net_diag_r4p(),
        NET_DIAG_OP_ERRORS => error_probe(),
        _ => return NET_DIAG_BAD_OP,
    };
    if ok { NET_DIAG_OK } else { NET_DIAG_FAILED }
}
fn diag_result(op: u32) -> NetDiagResult {
    let mut out = NetDiagResult {
        op,
        ..NetDiagResult::default()
    };
    let status = run_diag(op);
    project::fill_net_diag_timing(&mut out.timing, timing::status());
    project::fill_net_diag_backpressure(&mut out.backpressure, telemetry::backpressure());
    project::fill_net_diag_cleanup(&mut out.cleanup, shared::cleanup_status());
    let counters = shared::capture(cleanup::r4os_rust_net_diagnostic_counters);
    out.driver = NetDiagDriver {
        tests: counters.driver_lifecycle_tests,
        cases: counters.driver_lifecycle_cases,
    };
    project::fill_net_diag_errors(&mut out.errors, telemetry::errors());
    project::fill_net_diag_r4p(
        &mut out.r4p,
        shared::capture(protocol_runtime::r4os_rust_net_r4p_runtime),
    );
    // Preserve the original late counters query after the other owner snapshots.
    let counters = shared::capture(cleanup::r4os_rust_net_diagnostic_counters);
    match op {
        NET_DIAG_OP_TIMING => {
            out.tests = 1;
            out.cases = 8;
        }
        NET_DIAG_OP_BACKPRESSURE => {
            out.tests = 1;
            out.cases = 2;
        }
        NET_DIAG_OP_CLEANUP => {
            out.tests = out.cleanup.runs;
            out.cases = u64::from(out.cleanup.last_udp_closed)
                + u64::from(out.cleanup.last_tcp_connections)
                + u64::from(out.cleanup.last_tcp_listeners);
        }
        NET_DIAG_OP_POWER => {
            out.tests = counters.power_lifecycle_tests;
            out.cases = counters.power_lifecycle_cases;
        }
        NET_DIAG_OP_LIFECYCLE => {
            out.tests = out.cleanup.runs;
            out.cases = 2;
        }
        NET_DIAG_OP_RESET => {
            out.tests = out.cleanup.runs;
            out.cases = 1;
        }
        NET_DIAG_OP_DRIVER => {
            out.tests = out.driver.tests;
            out.cases = out.driver.cases;
        }
        NET_DIAG_OP_ENVIRONMENT => {
            out.tests = counters.environment_contract_tests;
            out.cases = counters.environment_contract_cases;
        }
        NET_DIAG_OP_LIMIT => {
            out.tests = counters.limit_contract_tests;
            out.cases = counters.limit_contract_cases;
        }
        NET_DIAG_OP_CORPUS => {
            out.tests = counters.packet_corpus_tests;
            out.cases = counters.packet_corpus_cases;
        }
        NET_DIAG_OP_NEGATIVE => {
            out.tests = counters.negative_path_tests;
            out.cases = counters.negative_path_cases;
        }
        NET_DIAG_OP_R4P => {
            out.tests = 1;
            out.cases = u64::from(out.r4p.active);
        }
        NET_DIAG_OP_ERRORS => {
            out.tests = 1;
            out.cases = out.errors.total;
        }
        _ => {}
    }
    out.status = status;
    out
}
/// # Safety
/// Canonical caller retains readable inputs and exclusive writable outputs,
/// with their declared alignment/extent, through the complete operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4net_net_diag_run(op: u32, out: *mut NetDiagResult) -> i32 {
    let value = diag_result(op);
    let status = value.status;
    unsafe {
        out.write(value);
    }
    status
}

// Paired original C signatures and local facade records.
const _: unsafe extern "C" fn(u32) -> i32 = r4os_rust_r4net_ipc_open;
const _: unsafe extern "C" fn(u32, *const u8, u32) -> i32 = r4os_rust_r4net_ipc_send;
const _: unsafe extern "C" fn(u32, *mut u8, u32) -> i32 = r4os_rust_r4net_ipc_recv;
const _: unsafe extern "C" fn(u32) -> i32 = r4os_rust_r4net_ipc_poll;
const _: unsafe extern "C" fn(u32) -> i32 = r4os_rust_r4net_ipc_close;
const _: unsafe extern "C" fn(*mut IpcSummary) -> i32 = r4os_rust_r4net_ipc_summary;
const _: unsafe extern "C" fn(u32, *mut IpcChannelInfo) -> i32 = r4os_rust_r4net_ipc_channel;
const _: unsafe extern "C" fn(u32, u16, u32, u16, *const u8, u32, *mut u8, u32) -> i32 =
    r4os_rust_r4net_net_service_request;
const _: unsafe extern "C" fn(u32, *mut IpcPerformanceSummary) -> i32 =
    r4os_rust_r4net_ipc_performance;
const _: unsafe extern "C" fn(u8, u8, u8, u8, u16) -> i32 = r4os_rust_r4net_tcp_connect;
const _: unsafe extern "C" fn(u32, *const u8, u32) -> i32 = r4os_rust_r4net_tcp_write;
const _: unsafe extern "C" fn(u32, *mut u8, u32) -> i32 = r4os_rust_r4net_tcp_read;
const _: unsafe extern "C" fn(u16, *mut u8, u32) -> i32 = r4os_rust_r4net_tcp_echo_listen_once;
const _: unsafe extern "C" fn(u16, *mut u8, u32, *mut TcpAcceptResult) -> i32 =
    r4os_rust_r4net_tcp_accept_read_once;
const _: unsafe extern "C" fn(u32) -> i32 = r4os_rust_r4net_tcp_close;
const _: unsafe extern "C" fn(*mut TcpSummary) -> i32 = r4os_rust_r4net_tcp_summary;
const _: unsafe extern "C" fn(u32, *mut TcpConnectionInfo) -> i32 = r4os_rust_r4net_tcp_connection;
const _: unsafe extern "C" fn(*mut TcpPerformanceInfo) -> i32 = r4os_rust_r4net_tcp_performance;
const _: unsafe extern "C" fn(u8, u8, u8, u8, u8, *const u8, u32) -> i32 =
    r4os_rust_r4net_net_ipv4_send;
const _: unsafe extern "C" fn(u8, *mut NetIpv4Packet, *mut u8, u32) -> i32 =
    r4os_rust_r4net_net_ipv4_recv;
const _: unsafe extern "C" fn(u16) -> i32 = r4os_rust_r4net_udp_bind;
const _: unsafe extern "C" fn(u32, u8, u8, u8, u8, u16, *const u8, u32) -> i32 =
    r4os_rust_r4net_udp_send_to;
const _: unsafe extern "C" fn(u32, *mut UdpRecvInfo, *mut u8, u32) -> i32 =
    r4os_rust_r4net_udp_recv_from;
const _: unsafe extern "C" fn(u32, *mut UdpRecvInfo, *mut u8, u32, u64) -> i32 =
    r4os_rust_r4net_udp_recv_from_wait;
const _: unsafe extern "C" fn(u32) -> i32 = r4os_rust_r4net_udp_close;
const _: unsafe extern "C" fn(*mut UdpStatus) -> i32 = r4os_rust_r4net_udp_status;
const _: unsafe extern "C" fn(*mut SerialLinkStatus) -> i32 = r4os_rust_r4net_serial_link_status;
const _: unsafe extern "C" fn() -> i32 = r4os_rust_r4net_serial_link_poll;
const _: unsafe extern "C" fn(*const u8, u32) -> i32 = r4os_rust_r4net_serial_link_send_message;
const _: unsafe extern "C" fn() -> i32 = r4os_rust_r4net_serial_link_host_test;
const _: unsafe extern "C" fn(*mut SerialLinkMessage) -> i32 = r4os_rust_r4net_serial_link_inbox;
const _: unsafe extern "C" fn(*mut NetConfigSnapshot) -> i32 = r4os_rust_r4net_net_config_get;
const _: unsafe extern "C" fn(*const NetConfigRequest) -> i32 = r4os_rust_r4net_net_config_set;
const _: unsafe extern "C" fn(*const u8, u32, *mut [u8; 4]) -> i32 =
    r4os_rust_r4net_net_dns_resolve;
const _: unsafe extern "C" fn(u8, u8, u8, u8, *const u8, u32, *mut [u8; 4]) -> i32 =
    r4os_rust_r4net_net_dns_resolve_server;
const _: unsafe extern "C" fn() -> i32 = r4os_rust_r4net_net_dhcp_acquire;
const _: unsafe extern "C" fn() -> i32 = r4os_rust_r4net_net_dhcp_renew;
const _: unsafe extern "C" fn() -> i32 = r4os_rust_r4net_net_dhcp_release;
const _: unsafe extern "C" fn(*mut DhcpStatus) -> i32 = r4os_rust_r4net_net_dhcp_status;
const _: unsafe extern "C" fn(u32, *mut NetDetailSnapshot) -> i32 = r4os_rust_r4net_net_detail_get;
const _: unsafe extern "C" fn(u32, *mut NetDiagResult) -> i32 = r4os_rust_r4net_net_diag_run;
const _: () = {
    assert!(core::mem::size_of::<UdpRecvInfo>() == 16);
    assert!(core::mem::align_of::<UdpRecvInfo>() == 2);
    assert!(core::mem::offset_of!(UdpRecvInfo, source_ip) == 0);
    assert!(core::mem::offset_of!(UdpRecvInfo, dest_ip) == 4);
    assert!(core::mem::offset_of!(UdpRecvInfo, source_port) == 8);
    assert!(core::mem::offset_of!(UdpRecvInfo, dest_port) == 10);
    assert!(core::mem::offset_of!(UdpRecvInfo, length) == 12);
    assert!(core::mem::offset_of!(UdpRecvInfo, reserved) == 14);
};
const _: () = {
    assert!(core::mem::size_of::<UdpStatus>() == 72);
    assert!(core::mem::align_of::<UdpStatus>() == 8);
    assert!(core::mem::offset_of!(UdpStatus, active_sockets) == 0);
    assert!(core::mem::offset_of!(UdpStatus, max_sockets) == 4);
    assert!(core::mem::offset_of!(UdpStatus, queued_packets) == 8);
    assert!(core::mem::offset_of!(UdpStatus, queue_limit) == 12);
    assert!(core::mem::offset_of!(UdpStatus, payload_max) == 16);
    assert!(core::mem::offset_of!(UdpStatus, reserved) == 20);
    assert!(core::mem::offset_of!(UdpStatus, delivered) == 24);
    assert!(core::mem::offset_of!(UdpStatus, drops) == 32);
    assert!(core::mem::offset_of!(UdpStatus, last_error) == 40);
};
