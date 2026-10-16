//! Stateless R4NET C facade; all platform behavior belongs to Rust.
const r4x_api = @import("r4x_api.zig");
const r4sys = @import("r4sys.zig");
const ipc = @import("../kernel/ipc.zig");
const net = @import("../net/core.zig");
const net_config = @import("../net/config.zig");
const net_config_writer = @import("../net/config_writer.zig");
const net_ipc_services = @import("../net/ipc_services.zig");
const serial_link = @import("../net/serial_link.zig");

pub const name = "R4NET";

pub const IpcSummary = r4x_api.IpcSummary;

pub const IpcChannelInfo = r4x_api.IpcChannelInfo;

pub const IpcPerformanceSummary = r4x_api.IpcPerformanceSummary;

pub const TcpSummary = r4x_api.TcpSummary;

pub const TcpConnectionInfo = r4x_api.TcpConnectionInfo;

pub const TcpPerformanceInfo = r4x_api.TcpPerformanceInfo;

pub const TcpAcceptResult = r4x_api.TcpAcceptResult;

pub const NetIpv4Packet = r4x_api.NetIpv4Packet;

pub const UdpRecvInfo = extern struct {
    source_ip: [4]u8 = .{0} ** 4,
    dest_ip: [4]u8 = .{0} ** 4,
    source_port: u16 = 0,
    dest_port: u16 = 0,
    length: u16 = 0,
    reserved: u16 = 0,
};

pub const UdpStatus = extern struct {
    active_sockets: u32 = 0,
    max_sockets: u32 = 0,
    queued_packets: u32 = 0,
    queue_limit: u32 = 0,
    payload_max: u32 = 0,
    reserved: u32 = 0,
    delivered: u64 = 0,
    drops: u64 = 0,
    last_error: [32]u8 = .{0} ** 32,
};

pub const SERIAL_LINK_PAYLOAD_MAX: usize = 256;
pub const SERIAL_LINK_RESULT_OK: i32 = 0;
pub const SERIAL_LINK_RESULT_NO_PORT: i32 = -1;
pub const SERIAL_LINK_RESULT_NOT_INITIALIZED: i32 = -2;
pub const SERIAL_LINK_RESULT_TOO_LARGE: i32 = -3;
pub const SERIAL_LINK_RESULT_FAILED: i32 = -4;

pub const SerialLinkStatus = r4x_api.SerialLinkStatus;

pub const SerialLinkMessage = r4x_api.SerialLinkMessage;

pub const NET_CONFIG_FLAG_CONFIGURED: u32 = 1 << 0;
pub const NET_CONFIG_FLAG_DNS_CONFIGURED: u32 = 1 << 1;
pub const NET_CONFIG_FLAG_ADAPTER_PRESENT: u32 = 1 << 2;
pub const NET_CONFIG_FLAG_LINK_UP: u32 = 1 << 3;
pub const NET_CONFIG_FLAG_WRITE_PERSISTENT: u32 = 1 << 8;
pub const NET_CONFIG_FLAG_APPLY_LIVE: u32 = 1 << 9;

pub const NET_CONFIG_OK: i32 = 0;
pub const NET_CONFIG_NO_ADAPTER: i32 = 1;
pub const NET_CONFIG_INVALID_IP: i32 = -1;
pub const NET_CONFIG_WRITE_FAILED: i32 = -2;
pub const NET_CONFIG_UNSUPPORTED: i32 = -4;
pub const NET_CONFIG_BUFFER_SMALL: i32 = -5;

pub const NetConfigSnapshot = r4x_api.NetConfigSnapshot;

pub const NetConfigRequest = r4x_api.NetConfigRequest;

pub const DHCP_STATUS_FLAG_BOUND: u32 = 1 << 0;
pub const DHCP_STATUS_FLAG_DNS_CONFIGURED: u32 = 1 << 1;
pub const DHCP_STATUS_FLAG_PENDING: u32 = 1 << 2;
pub const DHCP_STATUS_FLAG_DESIRED: u32 = 1 << 3;
pub const DHCP_STATUS_FLAG_TASK_STARTED: u32 = 1 << 4;
pub const DHCP_STATUS_FLAG_LINK_UP: u32 = 1 << 5;
pub const DHCP_STATUS_FLAG_RETRY_WAIT: u32 = 1 << 6;

pub const DhcpStatus = r4x_api.DhcpStatus;

pub const NET_DETAIL_FLAG_ADAPTER_PRESENT: u32 = 1 << 0;
pub const NET_DETAIL_FLAG_LINK_UP: u32 = 1 << 1;
pub const NET_DETAIL_FLAG_DNS_CONFIGURED: u32 = 1 << 2;
pub const NET_DETAIL_FLAG_ARP_CACHE_VALID: u32 = 1 << 3;
pub const NET_DETAIL_FLAG_DHCP_BOUND: u32 = 1 << 4;
pub const NET_DETAIL_FLAG_DHCP_DNS_CONFIGURED: u32 = 1 << 5;
pub const NET_DETAIL_FLAG_BACKEND_STATUS: u32 = 1 << 6;
pub const NET_DETAIL_FLAG_IRQ_REGISTERED: u32 = 1 << 7;
pub const NET_DETAIL_PROTOCOL_COUNT: usize = 9;
pub const NET_DETAIL_PROTOCOL_ETHERNET: usize = 0;
pub const NET_DETAIL_PROTOCOL_ARP: usize = 1;
pub const NET_DETAIL_PROTOCOL_IPV4: usize = 2;
pub const NET_DETAIL_PROTOCOL_ICMP: usize = 3;
pub const NET_DETAIL_PROTOCOL_UDP: usize = 4;
pub const NET_DETAIL_PROTOCOL_DHCP: usize = 5;
pub const NET_DETAIL_PROTOCOL_DNS: usize = 6;
pub const NET_DETAIL_PROTOCOL_TCP: usize = 7;
pub const NET_DETAIL_PROTOCOL_SERIAL_LINK: usize = 8;
pub const NET_DETAIL_MAX_TCP_CONNECTIONS: usize = 8;

pub const NetDetailProtocolRuntime = r4x_api.NetDetailProtocolRuntime;

pub const NetDetailAdapter = r4x_api.NetDetailAdapter;

pub const NetDetailEthernet = r4x_api.NetDetailEthernet;

pub const NetDetailArp = r4x_api.NetDetailArp;

pub const NetDetailIpv4 = r4x_api.NetDetailIpv4;

pub const NetDetailIcmp = r4x_api.NetDetailIcmp;

pub const NetDetailUdp = r4x_api.NetDetailUdp;

pub const NetDetailDns = r4x_api.NetDetailDns;

pub const NetDetailSnapshot = r4x_api.NetDetailSnapshot;

pub const NET_DIAG_OP_TIMING: u32 = 1;
pub const NET_DIAG_OP_BACKPRESSURE: u32 = 2;
pub const NET_DIAG_OP_CLEANUP: u32 = 3;
pub const NET_DIAG_OP_POWER: u32 = 4;
pub const NET_DIAG_OP_LIFECYCLE: u32 = 5;
pub const NET_DIAG_OP_RESET: u32 = 6;
pub const NET_DIAG_OP_DRIVER: u32 = 7;
pub const NET_DIAG_OP_ENVIRONMENT: u32 = 8;
pub const NET_DIAG_OP_LIMIT: u32 = 9;
pub const NET_DIAG_OP_CORPUS: u32 = 10;
pub const NET_DIAG_OP_NEGATIVE: u32 = 11;
pub const NET_DIAG_OP_R4P: u32 = 12;
pub const NET_DIAG_OP_ERRORS: u32 = 13;

pub const NET_DIAG_OK: i32 = 0;
pub const NET_DIAG_FAILED: i32 = -1;
pub const NET_DIAG_BAD_OP: i32 = -2;

pub const NetDiagTiming = r4x_api.NetDiagTiming;

pub const NetDiagBackpressure = r4x_api.NetDiagBackpressure;

pub const NetDiagCleanup = r4x_api.NetDiagCleanup;

pub const NetDiagDriver = r4x_api.NetDiagDriver;

pub const NetDiagErrors = r4x_api.NetDiagErrors;

pub const NetDiagR4p = r4x_api.NetDiagR4p;

pub const NetDiagResult = r4x_api.NetDiagResult;

pub fn ipcOpen(channel_id: u32) callconv(.c) i32 {
    return r4os_rust_r4net_ipc_open(channel_id);
}

pub fn ipcSend(channel_id: u32, data: [*]const u8, len: u32) callconv(.c) i32 {
    return r4os_rust_r4net_ipc_send(channel_id, data, len);
}

pub fn ipcRecv(channel_id: u32, out: [*]u8, max_len: u32) callconv(.c) i32 {
    return r4os_rust_r4net_ipc_recv(channel_id, out, max_len);
}

pub fn ipcPoll(channel_id: u32) callconv(.c) i32 {
    return r4os_rust_r4net_ipc_poll(channel_id);
}

pub fn ipcClose(channel_id: u32) callconv(.c) i32 {
    return r4os_rust_r4net_ipc_close(channel_id);
}

pub fn ipcSummary(out: *IpcSummary) callconv(.c) i32 {
    return r4os_rust_r4net_ipc_summary(out);
}

pub fn ipcChannel(channel_id: u32, out: *IpcChannelInfo) callconv(.c) i32 {
    return r4os_rust_r4net_ipc_channel(channel_id, out);
}

pub fn netServiceRequest(
    channel_id: u32,
    op: u16,
    request_id: u32,
    client_id: u16,
    payload_ptr: [*]const u8,
    payload_len: u32,
    out_ptr: [*]u8,
    out_capacity: u32,
) callconv(.c) i32 {
    return r4os_rust_r4net_net_service_request(channel_id, op, request_id, client_id, payload_ptr, payload_len, out_ptr, out_capacity);
}

pub fn ipcPerformance(channel_id: u32, out: *IpcPerformanceSummary) callconv(.c) i32 {
    return r4os_rust_r4net_ipc_performance(channel_id, out);
}

pub fn tcpConnect(a: u8, b: u8, c: u8, d: u8, port: u16) callconv(.c) i32 {
    return r4os_rust_r4net_tcp_connect(a, b, c, d, port);
}

pub fn tcpWrite(conn_id: u32, data: [*]const u8, len: u32) callconv(.c) i32 {
    return r4os_rust_r4net_tcp_write(conn_id, data, len);
}

pub fn tcpRead(conn_id: u32, out: [*]u8, max_len: u32) callconv(.c) i32 {
    return r4os_rust_r4net_tcp_read(conn_id, out, max_len);
}

pub fn tcpEchoListenOnce(port: u16, out: [*]u8, max_len: u32) callconv(.c) i32 {
    return r4os_rust_r4net_tcp_echo_listen_once(port, out, max_len);
}

pub fn tcpAcceptReadOnce(port: u16, out: [*]u8, max_len: u32, result: *TcpAcceptResult) callconv(.c) i32 {
    return r4os_rust_r4net_tcp_accept_read_once(port, out, max_len, result);
}

pub fn tcpClose(conn_id: u32) callconv(.c) i32 {
    return r4os_rust_r4net_tcp_close(conn_id);
}

pub fn tcpSummary(out: *TcpSummary) callconv(.c) i32 {
    return r4os_rust_r4net_tcp_summary(out);
}

pub fn tcpConnection(index: u32, out: *TcpConnectionInfo) callconv(.c) i32 {
    return r4os_rust_r4net_tcp_connection(index, out);
}

pub fn tcpPerformance(out: *TcpPerformanceInfo) callconv(.c) i32 {
    return r4os_rust_r4net_tcp_performance(out);
}

pub fn netIpv4Send(a: u8, b: u8, c: u8, d: u8, protocol: u8, payload: [*]const u8, len: u32) callconv(.c) i32 {
    return r4os_rust_r4net_net_ipv4_send(a, b, c, d, protocol, payload, len);
}

pub fn netIpv4Recv(protocol: u8, out: *NetIpv4Packet, payload: [*]u8, capacity: u32) callconv(.c) i32 {
    return r4os_rust_r4net_net_ipv4_recv(protocol, out, payload, capacity);
}

pub fn udpBind(port: u16) callconv(.c) i32 {
    return r4os_rust_r4net_udp_bind(port);
}

pub fn udpSendTo(handle: u32, a: u8, b: u8, c: u8, d: u8, port: u16, payload: [*]const u8, len: u32) callconv(.c) i32 {
    return r4os_rust_r4net_udp_send_to(handle, a, b, c, d, port, payload, len);
}

pub fn udpRecvFrom(handle: u32, out: *UdpRecvInfo, payload: [*]u8, capacity: u32) callconv(.c) i32 {
    return r4os_rust_r4net_udp_recv_from(handle, out, payload, capacity);
}

pub fn udpRecvFromWait(handle: u32, out: *UdpRecvInfo, payload: [*]u8, capacity: u32, timeout_ticks: u64) callconv(.c) i32 {
    return r4os_rust_r4net_udp_recv_from_wait(handle, out, payload, capacity, timeout_ticks);
}

pub fn udpClose(handle: u32) callconv(.c) i32 {
    return r4os_rust_r4net_udp_close(handle);
}

pub fn udpStatus(out: *UdpStatus) callconv(.c) i32 {
    return r4os_rust_r4net_udp_status(out);
}

pub fn serialLinkStatus(out: *SerialLinkStatus) callconv(.c) i32 {
    return r4os_rust_r4net_serial_link_status(out);
}

pub fn serialLinkPoll() callconv(.c) i32 {
    return r4os_rust_r4net_serial_link_poll();
}

pub fn serialLinkSendMessage(data: [*]const u8, len: u32) callconv(.c) i32 {
    return r4os_rust_r4net_serial_link_send_message(data, len);
}

pub fn serialLinkHostTest() callconv(.c) i32 {
    return r4os_rust_r4net_serial_link_host_test();
}

pub fn serialLinkInbox(out: *SerialLinkMessage) callconv(.c) i32 {
    return r4os_rust_r4net_serial_link_inbox(out);
}

pub fn netConfigGet(out: *NetConfigSnapshot) callconv(.c) i32 {
    return r4os_rust_r4net_net_config_get(out);
}

pub fn netConfigSet(request: *const NetConfigRequest) callconv(.c) i32 {
    return r4os_rust_r4net_net_config_set(request);
}

pub fn netDnsResolve(name_ptr: [*]const u8, len: u32, out: *[4]u8) callconv(.c) i32 {
    return r4os_rust_r4net_net_dns_resolve(name_ptr, len, out);
}

pub fn netDnsResolveServer(a: u8, b: u8, c: u8, d: u8, name_ptr: [*]const u8, len: u32, out: *[4]u8) callconv(.c) i32 {
    return r4os_rust_r4net_net_dns_resolve_server(a, b, c, d, name_ptr, len, out);
}

pub fn netDhcpAcquire() callconv(.c) i32 {
    return r4os_rust_r4net_net_dhcp_acquire();
}

pub fn netDhcpRenew() callconv(.c) i32 {
    return r4os_rust_r4net_net_dhcp_renew();
}

pub fn netDhcpRelease() callconv(.c) i32 {
    return r4os_rust_r4net_net_dhcp_release();
}

pub fn netDhcpStatus(out: *DhcpStatus) callconv(.c) i32 {
    return r4os_rust_r4net_net_dhcp_status(out);
}

pub fn netDetailGet(adapter_index: u32, out: *NetDetailSnapshot) callconv(.c) i32 {
    return r4os_rust_r4net_net_detail_get(adapter_index, out);
}

pub fn netDiagRun(op: u32, out: *NetDiagResult) callconv(.c) i32 {
    return r4os_rust_r4net_net_diag_run(op, out);
}

const PersistentNetConfigRewrite = struct {
    local_ip: [4]u8,
    netmask: [4]u8,
    gateway_ip: [4]u8,
    dns_ip: [4]u8,
    dns_configured: bool,
};

extern fn r4os_rust_r4net_ipc_open(u32) i32;
extern fn r4os_rust_r4net_ipc_send(u32, [*]const u8, u32) i32;
extern fn r4os_rust_r4net_ipc_recv(u32, [*]u8, u32) i32;
extern fn r4os_rust_r4net_ipc_poll(u32) i32;
extern fn r4os_rust_r4net_ipc_close(u32) i32;
extern fn r4os_rust_r4net_ipc_summary(*IpcSummary) i32;
extern fn r4os_rust_r4net_ipc_channel(u32, *IpcChannelInfo) i32;
extern fn r4os_rust_r4net_net_service_request(u32, u16, u32, u16, [*]const u8, u32, [*]u8, u32) i32;
extern fn r4os_rust_r4net_ipc_performance(u32, *IpcPerformanceSummary) i32;
extern fn r4os_rust_r4net_tcp_connect(u8, u8, u8, u8, u16) i32;
extern fn r4os_rust_r4net_tcp_write(u32, [*]const u8, u32) i32;
extern fn r4os_rust_r4net_tcp_read(u32, [*]u8, u32) i32;
extern fn r4os_rust_r4net_tcp_echo_listen_once(u16, [*]u8, u32) i32;
extern fn r4os_rust_r4net_tcp_accept_read_once(u16, [*]u8, u32, *TcpAcceptResult) i32;
extern fn r4os_rust_r4net_tcp_close(u32) i32;
extern fn r4os_rust_r4net_tcp_summary(*TcpSummary) i32;
extern fn r4os_rust_r4net_tcp_connection(u32, *TcpConnectionInfo) i32;
extern fn r4os_rust_r4net_tcp_performance(*TcpPerformanceInfo) i32;
extern fn r4os_rust_r4net_net_ipv4_send(u8, u8, u8, u8, u8, [*]const u8, u32) i32;
extern fn r4os_rust_r4net_net_ipv4_recv(u8, *NetIpv4Packet, [*]u8, u32) i32;
extern fn r4os_rust_r4net_udp_bind(u16) i32;
extern fn r4os_rust_r4net_udp_send_to(u32, u8, u8, u8, u8, u16, [*]const u8, u32) i32;
extern fn r4os_rust_r4net_udp_recv_from(u32, *UdpRecvInfo, [*]u8, u32) i32;
extern fn r4os_rust_r4net_udp_recv_from_wait(u32, *UdpRecvInfo, [*]u8, u32, u64) i32;
extern fn r4os_rust_r4net_udp_close(u32) i32;
extern fn r4os_rust_r4net_udp_status(*UdpStatus) i32;
extern fn r4os_rust_r4net_serial_link_status(*SerialLinkStatus) i32;
extern fn r4os_rust_r4net_serial_link_poll() i32;
extern fn r4os_rust_r4net_serial_link_send_message([*]const u8, u32) i32;
extern fn r4os_rust_r4net_serial_link_host_test() i32;
extern fn r4os_rust_r4net_serial_link_inbox(*SerialLinkMessage) i32;
extern fn r4os_rust_r4net_net_config_get(*NetConfigSnapshot) i32;
extern fn r4os_rust_r4net_net_config_set(*const NetConfigRequest) i32;
extern fn r4os_rust_r4net_net_dns_resolve([*]const u8, u32, *[4]u8) i32;
extern fn r4os_rust_r4net_net_dns_resolve_server(u8, u8, u8, u8, [*]const u8, u32, *[4]u8) i32;
extern fn r4os_rust_r4net_net_dhcp_acquire() i32;
extern fn r4os_rust_r4net_net_dhcp_renew() i32;
extern fn r4os_rust_r4net_net_dhcp_release() i32;
extern fn r4os_rust_r4net_net_dhcp_status(*DhcpStatus) i32;
extern fn r4os_rust_r4net_net_detail_get(u32, *NetDetailSnapshot) i32;
extern fn r4os_rust_r4net_net_diag_run(u32, *NetDiagResult) i32;

comptime {
    if (@sizeOf(UdpRecvInfo) != 16 or @alignOf(UdpRecvInfo) != 2 or @offsetOf(UdpRecvInfo, "source_ip") != 0 or @offsetOf(UdpRecvInfo, "dest_ip") != 4 or @offsetOf(UdpRecvInfo, "source_port") != 8 or @offsetOf(UdpRecvInfo, "dest_port") != 10 or @offsetOf(UdpRecvInfo, "length") != 12 or @offsetOf(UdpRecvInfo, "reserved") != 14) @compileError("Canonical native R4NET UdpRecvInfo layout");
}

comptime {
    if (@sizeOf(UdpStatus) != 72 or @alignOf(UdpStatus) != 8 or @offsetOf(UdpStatus, "active_sockets") != 0 or @offsetOf(UdpStatus, "max_sockets") != 4 or @offsetOf(UdpStatus, "queued_packets") != 8 or @offsetOf(UdpStatus, "queue_limit") != 12 or @offsetOf(UdpStatus, "payload_max") != 16 or @offsetOf(UdpStatus, "reserved") != 20 or @offsetOf(UdpStatus, "delivered") != 24 or @offsetOf(UdpStatus, "drops") != 32 or @offsetOf(UdpStatus, "last_error") != 40) @compileError("Canonical native R4NET UdpStatus layout");
}
