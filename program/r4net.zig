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
    return ipc.open(channel_id);
}

pub fn ipcSend(channel_id: u32, data: [*]const u8, len: u32) callconv(.c) i32 {
    if (len > ipc.MAX_MESSAGE_SIZE) return -1;
    return ipc.send(channel_id, data[0..len]);
}

pub fn ipcRecv(channel_id: u32, out: [*]u8, max_len: u32) callconv(.c) i32 {
    if (max_len > ipc.MAX_MESSAGE_SIZE) return -1;
    return ipc.recv(channel_id, out[0..max_len]);
}

pub fn ipcPoll(channel_id: u32) callconv(.c) i32 {
    return ipc.poll(channel_id);
}

pub fn ipcClose(channel_id: u32) callconv(.c) i32 {
    return ipc.close(channel_id);
}

pub fn ipcSummary(out: *IpcSummary) callconv(.c) i32 {
    var s: ipc.Summary = .{};
    const result = ipc.summary(&s);
    if (result < 0) return result;
    out.* = .{
        .max_channels = s.max_channels,
        .active_channels = s.active_channels,
        .max_message_size = s.max_message_size,
        .queue_depth = s.queue_depth,
        .sends = s.sends,
        .receives = s.receives,
        .drops = s.drops,
        .errors = s.errors,
        .echo_tests = s.echo_tests,
    };
    return result;
}

pub fn ipcChannel(channel_id: u32, out: *IpcChannelInfo) callconv(.c) i32 {
    var info: ipc.ChannelInfo = .{};
    const result = ipc.channelInfo(channel_id, &info);
    out.* = .{
        .id = info.id,
        .active = info.active,
        .queued = info.queued,
        .queue_depth = info.queue_depth,
        .max_message_size = info.max_message_size,
        .has_handler = info.has_handler,
        .opens = info.opens,
        .closes = info.closes,
        .sends = info.sends,
        .receives = info.receives,
        .drops = info.drops,
        .name = info.name,
    };
    return result;
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
    if (payload_len > ipc.MAX_MESSAGE_SIZE - net_ipc_services.HEADER_SIZE) return net_ipc_services.RESULT_BAD_REQUEST;
    if (out_capacity > ipc.MAX_MESSAGE_SIZE) return net_ipc_services.RESULT_BAD_REQUEST;
    const payload = payload_ptr[0..@as(usize, @intCast(payload_len))];
    const out = out_ptr[0..@as(usize, @intCast(out_capacity))];
    return net_ipc_services.sendRequestAsClient(channel_id, op, request_id, client_id, payload, out);
}

pub fn ipcPerformance(channel_id: u32, out: *IpcPerformanceSummary) callconv(.c) i32 {
    const caller_version = out.version;
    const caller_size: usize = out.size;
    const header_size: usize = @offsetOf(IpcPerformanceSummary, "worker_started");
    const required_size: usize = @sizeOf(IpcPerformanceSummary);
    if (caller_version == 0 or caller_size < header_size) return -1;
    if (caller_size < required_size) {
        out.version = 1;
        out.size = @intCast(required_size);
        return -1;
    }

    var s: ipc.PerformanceSummary = .{};
    const result = ipc.performanceSummaryFor(channel_id, &s);
    if (result < 0) return result;
    out.* = .{
        .version = 1,
        .size = @intCast(required_size),
        .worker_started = s.worker_started,
        .worker_task_id = s.worker_task_id,
        .active_channels = s.active_channels,
        .queue_used = s.queue_used,
        .queue_ready = s.queue_ready,
        .queue_running = s.queue_running,
        .queue_limit = s.queue_limit,
        .handler_queued = s.handler_queued,
        .handler_completed = s.handler_completed,
        .handler_failures = s.handler_failures,
        .handler_direct = s.handler_direct,
        .handler_waits = s.handler_waits,
        .handler_wait_timeouts = s.handler_wait_timeouts,
        .handler_queue_ns = s.handler_queue_ns,
        .handler_queue_max_ns = s.handler_queue_max_ns,
        .handler_run_ns = s.handler_run_ns,
        .handler_run_max_ns = s.handler_run_max_ns,
        .handler_e2e_ns = s.handler_e2e_ns,
        .handler_e2e_max_ns = s.handler_e2e_max_ns,
        .request_bytes = s.request_bytes,
        .response_bytes = s.response_bytes,
        .payload_copy_bytes = s.payload_copy_bytes,
        .payload_clear_bytes = s.payload_clear_bytes,
        .queue_full = s.queue_full,
        .queue_empty = s.queue_empty,
        .admission_waits = s.admission_waits,
        .admission_timeouts = s.admission_timeouts,
        .recv_buffer_small = s.recv_buffer_small,
        .response_search_slots = s.response_search_slots,
        .stale_drops = s.stale_drops,
        .lock_contentions = s.lock_contentions,
        .irq_denied = s.irq_denied,
    };
    return result;
}

pub fn tcpConnect(a: u8, b: u8, c: u8, d: u8, port: u16) callconv(.c) i32 {
    return net.tcpConnect(.{ a, b, c, d }, port);
}

pub fn tcpWrite(conn_id: u32, data: [*]const u8, len: u32) callconv(.c) i32 {
    if (len > net.TCP_BUFFER_SIZE) return -1;
    return net.tcpWrite(conn_id, data[0..len]);
}

pub fn tcpRead(conn_id: u32, out: [*]u8, max_len: u32) callconv(.c) i32 {
    if (max_len > net.TCP_BUFFER_SIZE) return -1;
    return net.tcpRead(conn_id, out[0..max_len]);
}

pub fn tcpEchoListenOnce(port: u16, out: [*]u8, max_len: u32) callconv(.c) i32 {
    if (max_len > net.TCP_BUFFER_SIZE) return -1;
    return net.tcpEchoListenOnce(port, out[0..max_len]);
}

pub fn tcpAcceptReadOnce(port: u16, out: [*]u8, max_len: u32, result: *TcpAcceptResult) callconv(.c) i32 {
    result.* = .{};
    if (max_len > net.TCP_BUFFER_SIZE) return -1;
    var conn_id: u32 = 0;
    const got = net.tcpAcceptReadOnce(port, out[0..max_len], &conn_id);
    if (got > 0) {
        result.* = .{
            .conn_id = conn_id,
            .bytes = @intCast(got),
        };
    }
    return got;
}

pub fn tcpClose(conn_id: u32) callconv(.c) i32 {
    return net.tcpClose(conn_id);
}

pub fn tcpSummary(out: *TcpSummary) callconv(.c) i32 {
    var s: net.TcpSummary = .{};
    net.tcpSummary(&s);
    out.* = .{
        .max_connections = s.max_connections,
        .active_connections = s.active_connections,
        .buffer_size = s.buffer_size,
        .syn_tx = s.syn_tx,
        .synack_rx = s.synack_rx,
        .ack_tx = s.ack_tx,
        .data_tx = s.data_tx,
        .data_rx = s.data_rx,
        .fin_tx = s.fin_tx,
        .rst_rx = s.rst_rx,
        .checksum_errors = s.checksum_errors,
        .timeouts = s.timeouts,
        .self_tests = s.self_tests,
        .active_listeners = s.active_listeners,
        .synack_tx = s.synack_tx,
        .listen_syn_rx = s.listen_syn_rx,
        .accepts = s.accepts,
        .retransmits = s.retransmits,
        .rx_drops = s.rx_drops,
        .last_source_port = s.last_source_port,
        .last_dest_port = s.last_dest_port,
        .last_seq = s.last_seq,
        .last_ack = s.last_ack,
        .last_payload_len = s.last_payload_len,
    };
    return 1;
}

pub fn tcpConnection(index: u32, out: *TcpConnectionInfo) callconv(.c) i32 {
    var info: net.TcpConnectionInfo = .{};
    const result = net.tcpConnectionInfo(index, &info);
    if (result <= 0) return result;
    fillTcpConnectionInfo(out, info);
    return result;
}

pub fn netIpv4Send(a: u8, b: u8, c: u8, d: u8, protocol: u8, payload: [*]const u8, len: u32) callconv(.c) i32 {
    if (len > net.MAX_PACKET_SIZE) return txResultCode(.too_large);
    return txResultCode(net.sendIpv4Payload(.{ a, b, c, d }, protocol, payload[0..len]));
}

pub fn netIpv4Recv(protocol: u8, out: *NetIpv4Packet, payload: [*]u8, capacity: u32) callconv(.c) i32 {
    if (capacity > net.APP_IPV4_PAYLOAD_MAX) return -1;
    net.pollAdapters(4096);
    var info: net.AppIpv4Packet = .{};
    const result = net.readAppIpv4(protocol, &info, payload[0..capacity]);
    out.* = .{
        .source_ip = info.source_ip,
        .dest_ip = info.dest_ip,
        .protocol = info.protocol,
        .truncated = if (info.truncated) 1 else 0,
        .reserved = 0,
        .payload_len = info.payload_len,
    };
    return result;
}

pub fn udpBind(port: u16) callconv(.c) i32 {
    return net.udpBind(port);
}

pub fn udpSendTo(handle: u32, a: u8, b: u8, c: u8, d: u8, port: u16, payload: [*]const u8, len: u32) callconv(.c) i32 {
    if (len > net.MAX_PACKET_SIZE) return txResultCode(.too_large);
    return txResultCode(net.udpSendTo(handle, .{ a, b, c, d }, port, payload[0..len]));
}

pub fn udpRecvFrom(handle: u32, out: *UdpRecvInfo, payload: [*]u8, capacity: u32) callconv(.c) i32 {
    if (capacity > net.MAX_PACKET_SIZE) return -1;
    net.pollAdapters(4096);
    var info: net.UdpRecvInfo = .{};
    const result = net.udpRecvFrom(handle, &info, payload[0..capacity]);
    out.* = .{
        .source_ip = info.source_ip,
        .dest_ip = info.dest_ip,
        .source_port = info.source_port,
        .dest_port = info.dest_port,
        .length = info.length,
        .reserved = 0,
    };
    return result;
}

pub fn udpRecvFromWait(handle: u32, out: *UdpRecvInfo, payload: [*]u8, capacity: u32, timeout_ticks: u64) callconv(.c) i32 {
    if (capacity > net.MAX_PACKET_SIZE) return -1;
    var info: net.UdpRecvInfo = .{};
    const result = net.udpRecvFromWait(handle, &info, payload[0..capacity], timeout_ticks);
    out.* = .{
        .source_ip = info.source_ip,
        .dest_ip = info.dest_ip,
        .source_port = info.source_port,
        .dest_port = info.dest_port,
        .length = info.length,
        .reserved = 0,
    };
    return result;
}

pub fn udpClose(handle: u32) callconv(.c) i32 {
    return net.udpClose(handle);
}

pub fn udpStatus(out: *UdpStatus) callconv(.c) i32 {
    var status: net.UdpStatus = .{};
    net.udpStatus(&status);
    out.* = .{
        .active_sockets = status.active_sockets,
        .max_sockets = status.max_sockets,
        .queued_packets = status.queued_packets,
        .queue_limit = status.queue_limit,
        .payload_max = status.payload_max,
        .reserved = 0,
        .delivered = status.delivered,
        .drops = status.drops,
    };
    copyFixedZ(out.last_error[0..], status.last_error);
    return 1;
}

pub fn serialLinkStatus(out: *SerialLinkStatus) callconv(.c) i32 {
    serial_link.poll();
    fillSerialLinkStatus(out);
    return 1;
}

pub fn serialLinkPoll() callconv(.c) i32 {
    serial_link.poll();
    return 1;
}

pub fn serialLinkSendMessage(data: [*]const u8, len: u32) callconv(.c) i32 {
    if (len > SERIAL_LINK_PAYLOAD_MAX) return SERIAL_LINK_RESULT_TOO_LARGE;
    if (serial_link.sendMessage(data[0..len])) return @intCast(len);
    var status: serial_link.Snapshot = .{};
    serial_link.snapshot(&status);
    if (!status.present) return SERIAL_LINK_RESULT_NO_PORT;
    if (!status.initialized) return SERIAL_LINK_RESULT_NOT_INITIALIZED;
    return SERIAL_LINK_RESULT_FAILED;
}

pub fn serialLinkHostTest() callconv(.c) i32 {
    serial_link.poll();
    var status: serial_link.Snapshot = .{};
    serial_link.snapshot(&status);
    if (!status.present) return SERIAL_LINK_RESULT_NO_PORT;
    if (!status.initialized) return SERIAL_LINK_RESULT_NOT_INITIALIZED;
    return SERIAL_LINK_RESULT_FAILED;
}

pub fn serialLinkInbox(out: *SerialLinkMessage) callconv(.c) i32 {
    serial_link.poll();
    var message: serial_link.Message = .{};
    if (!serial_link.lastMessage(&message)) {
        out.* = .{};
        return 0;
    }
    out.* = .{ .len = message.len };
    const len: usize = @intCast(message.len);
    if (len != 0) @memcpy(out.data[0..len], message.data[0..len]);
    return @intCast(message.len);
}

pub fn netConfigGet(out: *NetConfigSnapshot) callconv(.c) i32 {
    fillNetConfigSnapshot(out);
    return NET_CONFIG_OK;
}

pub fn netConfigSet(request: *const NetConfigRequest) callconv(.c) i32 {
    if ((request.flags & (NET_CONFIG_FLAG_WRITE_PERSISTENT | NET_CONFIG_FLAG_APPLY_LIVE)) == 0) return NET_CONFIG_UNSUPPORTED;
    const local_ip = parseFixedIpv4(request.local_ip[0..]) orelse return NET_CONFIG_INVALID_IP;
    const netmask = parseFixedIpv4(request.netmask[0..]) orelse return NET_CONFIG_INVALID_IP;
    if (!validNetmask(netmask)) return NET_CONFIG_INVALID_IP;
    const gateway_ip = parseFixedIpv4(request.gateway_ip[0..]) orelse return NET_CONFIG_INVALID_IP;
    const dns_text = fixedText(request.dns_ip[0..]);
    const dns_configured = dns_text.len > 0;
    const dns_ip = if (dns_configured) (net_config.parseIpv4(dns_text) orelse return NET_CONFIG_INVALID_IP) else [_]u8{ 0, 0, 0, 0 };

    if ((request.flags & NET_CONFIG_FLAG_WRITE_PERSISTENT) != 0) {
        const persist = writePersistentNetConfig(local_ip, netmask, gateway_ip, dns_ip, dns_configured);
        if (persist != NET_CONFIG_OK) return persist;
    }

    if ((request.flags & NET_CONFIG_FLAG_APPLY_LIVE) != 0) {
        net_config.applyRuntime(local_ip, netmask, gateway_ip, dns_ip, dns_configured);
        net.cancelDhcpForStaticConfig();
        net.arpFlush();
        if (net.count() == 0) return NET_CONFIG_NO_ADAPTER;
    }
    return NET_CONFIG_OK;
}

pub fn netDnsResolve(name_ptr: [*]const u8, len: u32, out: *[4]u8) callconv(.c) i32 {
    if (len > 255) return -9;
    return net.dnsResolveA(name_ptr[0..len], out);
}

pub fn netDnsResolveServer(a: u8, b: u8, c: u8, d: u8, name_ptr: [*]const u8, len: u32, out: *[4]u8) callconv(.c) i32 {
    if (len > 255) return -9;
    return net.dnsResolveAWithServer(name_ptr[0..len], .{ a, b, c, d }, out);
}

pub fn netDhcpAcquire() callconv(.c) i32 {
    return txResultCode(net.dhcpAcquireDefault());
}

pub fn netDhcpRenew() callconv(.c) i32 {
    return txResultCode(net.dhcpRenewDefault());
}

pub fn netDhcpRelease() callconv(.c) i32 {
    return txResultCode(net.dhcpReleaseDefault());
}

pub fn netDhcpStatus(out: *DhcpStatus) callconv(.c) i32 {
    const stats = net.dhcpStats();
    const runtime = net.dhcpRuntimeStatus();
    const link_up = if (net.get(0)) |adapter| adapter.link == .up else false;
    out.* = .{
        .discover_tx = stats.discover_tx,
        .offer_rx = stats.offer_rx,
        .request_tx = stats.request_tx,
        .ack_rx = stats.ack_rx,
        .nak_rx = stats.nak_rx,
        .release_tx = stats.release_tx,
        .retries = stats.retries,
        .timeouts = stats.timeouts,
        .release_errors = stats.release_errors,
        .malformed = stats.malformed,
        .self_tests = stats.self_tests,
        .xid = stats.lease.xid,
        .offered_ip = stats.lease.offered_ip,
        .server_ip = stats.lease.server_ip,
        .netmask = stats.lease.netmask,
        .gateway_ip = stats.lease.gateway_ip,
        .dns_ip = stats.lease.dns_ip,
        .lease_seconds = stats.lease.lease_seconds,
        .renew_seconds = stats.lease.renew_seconds,
        .rebind_seconds = stats.lease.rebind_seconds,
        .flags = (if (stats.lease.bound) DHCP_STATUS_FLAG_BOUND else 0) |
            (if (stats.lease.dns_configured) DHCP_STATUS_FLAG_DNS_CONFIGURED else 0) |
            (if (stats.operation_pending or runtime.operation_active) DHCP_STATUS_FLAG_PENDING else 0) |
            (if (net_config.dhcpEnabled()) DHCP_STATUS_FLAG_DESIRED else 0) |
            (if (runtime.task_started) DHCP_STATUS_FLAG_TASK_STARTED else 0) |
            (if (link_up) DHCP_STATUS_FLAG_LINK_UP else 0) |
            (if (runtime.state == .retry_wait) DHCP_STATUS_FLAG_RETRY_WAIT else 0),
        .last_attempt = stats.last_attempt,
        .last_type = stats.last_type,
        .runtime_state = @intFromEnum(runtime.state),
    };
    copyFixedZ(out.last_error[0..], stats.last_error);
    return 1;
}

pub fn netDetailGet(adapter_index: u32, out: *NetDetailSnapshot) callconv(.c) i32 {
    out.* = .{ .adapter_index = adapter_index };
    fillNetConfigSnapshot(&out.config);
    _ = netDhcpStatus(&out.dhcp);
    const dhcp_runtime_status = net.dhcpRuntimeStatus();
    out.link_generation = dhcp_runtime_status.link_generation;
    _ = tcpSummary(&out.tcp);

    out.flags = 0;
    if ((out.config.flags & NET_CONFIG_FLAG_DNS_CONFIGURED) != 0) out.flags |= NET_DETAIL_FLAG_DNS_CONFIGURED;
    if ((out.dhcp.flags & DHCP_STATUS_FLAG_BOUND) != 0) out.flags |= NET_DETAIL_FLAG_DHCP_BOUND;
    if ((out.dhcp.flags & DHCP_STATUS_FLAG_DNS_CONFIGURED) != 0) out.flags |= NET_DETAIL_FLAG_DHCP_DNS_CONFIGURED;

    const eth = net.ethernetStats();
    out.ethernet = .{
        .rx = eth.rx,
        .tx = eth.tx,
        .broadcast = eth.broadcast,
        .own_unicast = eth.own_unicast,
        .dropped_short = eth.dropped_short,
        .dropped_filter = eth.dropped_filter,
        .unknown_ethertype = eth.unknown_ethertype,
        .test_frames = eth.test_frames,
        .last_ethertype = eth.last_ethertype,
    };
    copyFixedZ(out.ethernet.last_error[0..], eth.last_error);

    const arp = net.arpStats();
    if (arp.cache_valid) out.flags |= NET_DETAIL_FLAG_ARP_CACHE_VALID;
    out.arp = .{
        .requests_tx = arp.requests_tx,
        .replies_tx = arp.replies_tx,
        .replies_rx = arp.replies_rx,
        .requests_rx = arp.requests_rx,
        .malformed = arp.malformed,
        .cache_updates = arp.cache_updates,
        .cache_hits = arp.cache_hits,
        .resolve_attempts = arp.resolve_attempts,
        .resolve_retries = arp.resolve_retries,
        .resolve_timeouts = arp.resolve_timeouts,
        .resolve_misses = arp.resolve_misses,
        .pending_packets = arp.pending_packets,
        .pending_timeouts = arp.pending_timeouts,
        .pending_drops = arp.pending_drops,
        .pending_queue_limit = arp.pending_queue_limit,
        .last_opcode = arp.last_opcode,
        .cache_ip = arp.cache_ip,
        .cache_mac = arp.cache_mac,
        .cache_age_ticks = net.arpCacheAgeTicks(),
        .cache_ttl_ticks = net.arpCacheTtlTicks(),
    };
    copyFixedZ(out.arp.last_error[0..], arp.last_error);

    const ip = net.ipv4Stats();
    out.ipv4 = .{
        .rx_packets = ip.rx_packets,
        .tx_packets = ip.tx_packets,
        .dropped_short = ip.dropped_short,
        .dropped_version = ip.dropped_version,
        .dropped_checksum = ip.dropped_checksum,
        .dropped_fragment = ip.dropped_fragment,
        .dropped_destination = ip.dropped_destination,
        .dropped_tx_too_large = ip.dropped_tx_too_large,
        .malformed = ip.malformed,
        .last_protocol = ip.last_protocol,
        .last_source = ip.last_source,
        .last_dest = ip.last_dest,
    };
    copyFixedZ(out.ipv4.last_error[0..], ip.last_error);

    const ic = net.icmpStats();
    out.icmp = .{
        .rx_packets = ic.rx_packets,
        .tx_packets = ic.tx_packets,
        .echo_requests_rx = ic.echo_requests_rx,
        .echo_replies_rx = ic.echo_replies_rx,
        .echo_requests_tx = ic.echo_requests_tx,
        .echo_replies_tx = ic.echo_replies_tx,
        .destination_unreachable_rx = ic.destination_unreachable_rx,
        .port_unreachable_rx = ic.port_unreachable_rx,
        .time_exceeded_rx = ic.time_exceeded_rx,
        .malformed = ic.malformed,
        .checksum_errors = ic.checksum_errors,
        .last_type = ic.last_type,
        .last_code = ic.last_code,
        .last_id = ic.last_id,
        .last_seq = ic.last_seq,
    };
    copyFixedZ(out.icmp.last_error[0..], ic.last_error);

    const udp = net.udpStats();
    out.udp = .{
        .rx_packets = udp.rx_packets,
        .tx_packets = udp.tx_packets,
        .dropped_short = udp.dropped_short,
        .dropped_length = udp.dropped_length,
        .checksum_errors = udp.checksum_errors,
        .malformed = udp.malformed,
        .dhcp_rx = udp.dhcp_rx,
        .dns_rx = udp.dns_rx,
        .self_tests = udp.self_tests,
        .last_source_port = udp.last_source_port,
        .last_dest_port = udp.last_dest_port,
    };
    copyFixedZ(out.udp.last_error[0..], udp.last_error);

    const dns = net.dnsStats();
    out.dns = .{
        .queries_tx = dns.queries_tx,
        .responses_rx = dns.responses_rx,
        .a_records = dns.a_records,
        .resolve_requests = dns.resolve_requests,
        .timeouts = dns.timeouts,
        .nxdomain = dns.nxdomain,
        .tx_errors = dns.tx_errors,
        .malformed = dns.malformed,
        .self_tests = dns.self_tests,
        .last_id = dns.last_id,
        .last_result = dns.last_result,
        .last_server = dns.last_server,
        .last_answer = dns.last_answer,
    };
    copyFixedZ(out.dns.last_error[0..], dns.last_error);

    const tcp_stats = net.tcpStats();
    copyFixedZ(out.tcp_last_error[0..], tcp_stats.last_error);
    fillNetDetailRuntime(&out.protocols[NET_DETAIL_PROTOCOL_ETHERNET], net.protocolRuntimeStats(.ethernet));
    fillNetDetailRuntime(&out.protocols[NET_DETAIL_PROTOCOL_ARP], net.protocolRuntimeStats(.arp));
    fillNetDetailRuntime(&out.protocols[NET_DETAIL_PROTOCOL_IPV4], net.protocolRuntimeStats(.ipv4));
    fillNetDetailRuntime(&out.protocols[NET_DETAIL_PROTOCOL_ICMP], net.protocolRuntimeStats(.icmp));
    fillNetDetailRuntime(&out.protocols[NET_DETAIL_PROTOCOL_UDP], net.protocolRuntimeStats(.udp));
    fillNetDetailRuntime(&out.protocols[NET_DETAIL_PROTOCOL_DHCP], net.protocolRuntimeStats(.dhcp));
    fillNetDetailRuntime(&out.protocols[NET_DETAIL_PROTOCOL_DNS], net.protocolRuntimeStats(.dns));
    fillNetDetailRuntime(&out.protocols[NET_DETAIL_PROTOCOL_TCP], net.protocolRuntimeStats(.tcp));
    fillNetDetailRuntime(&out.protocols[NET_DETAIL_PROTOCOL_SERIAL_LINK], net.protocolRuntimeStats(.serial_link));

    var conn_index: u32 = 0;
    while (conn_index < NET_DETAIL_MAX_TCP_CONNECTIONS) : (conn_index += 1) {
        var info: net.TcpConnectionInfo = .{};
        if (net.tcpConnectionInfo(conn_index, &info) <= 0) continue;
        const out_index: usize = @intCast(out.tcp_connection_count);
        if (out_index >= NET_DETAIL_MAX_TCP_CONNECTIONS) break;
        fillTcpConnectionInfo(&out.tcp_connections[out_index], info);
        out.tcp_connection_count += 1;
    }

    const adapter_usize: usize = @intCast(adapter_index);
    const backend_status = net.refreshAdapterRuntime(adapter_usize);
    const adapter = net.get(adapter_usize) orelse return 0;
    out.flags |= NET_DETAIL_FLAG_ADAPTER_PRESENT;
    if (adapter.link == .up) out.flags |= NET_DETAIL_FLAG_LINK_UP;
    out.adapter = .{
        .index = adapter_index,
        .count = @intCast(net.count()),
        .flags = adapter.flags,
        .lifecycle = netLifecycleCode(adapter.lifecycle),
        .bus_no = adapter.bus_no,
        .device_no = adapter.device_no,
        .function_no = adapter.function_no,
        .class_code = 0x02,
        .subclass = 0x00,
        .prog_if = 0,
        .vendor_id = adapter.vendor_id,
        .device_id = adapter.device_id,
        .mac = adapter.mac,
        .mtu = adapter.mtu,
        .rx_packets = adapter.stats.rx_packets,
        .tx_packets = adapter.stats.tx_packets,
        .rx_bytes = adapter.stats.rx_bytes,
        .tx_bytes = adapter.stats.tx_bytes,
        .drops = adapter.stats.drops,
        .errors = adapter.stats.errors,
        .resets = adapter.stats.resets,
        .registered_tick = adapter.registered_tick,
        .state_changed_tick = adapter.state_changed_tick,
        .dhcp_retry_round = dhcp_runtime_status.retry_round,
    };
    copyFixedZ(out.adapter.name[0..], adapter.name);
    copyFixedZ(out.adapter.driver[0..], adapter.driver);
    copyFixedZ(out.adapter.link[0..], linkName(adapter.link));
    copyFixedZ(out.adapter.state[0..], net.lifecycleName(adapter.lifecycle));
    copyFixedZ(out.adapter.last_error[0..], adapter.stats.last_error);

    if (backend_status) |backend| {
        out.flags |= NET_DETAIL_FLAG_BACKEND_STATUS;
        if (backend.irq_registered != 0) out.flags |= NET_DETAIL_FLAG_IRQ_REGISTERED;
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
    return 1;
}

pub fn netDiagRun(op: u32, out: *NetDiagResult) callconv(.c) i32 {
    out.* = .{ .op = op };
    const status = runNetDiagOp(op);
    fillNetDiagResult(op, out);
    out.status = status;
    return status;
}

fn runNetDiagOp(op: u32) i32 {
    const ok = switch (op) {
        NET_DIAG_OP_TIMING => net.runTimingProbe(),
        NET_DIAG_OP_BACKPRESSURE => net.runBackpressureProbe(),
        NET_DIAG_OP_CLEANUP => net.runCleanupProbe(),
        NET_DIAG_OP_POWER => net.runPowerLifecycleProbe(),
        NET_DIAG_OP_LIFECYCLE => net.runLinkLifecycleProbe(),
        NET_DIAG_OP_RESET => net.runAdapterResetProbe(),
        NET_DIAG_OP_DRIVER => net.runDriverLifecycleProbe(),
        NET_DIAG_OP_ENVIRONMENT => net.runEnvironmentContractProbe(),
        NET_DIAG_OP_LIMIT => net.runLimitContractProbe(),
        NET_DIAG_OP_CORPUS => net.runPacketCorpusProbe(),
        NET_DIAG_OP_NEGATIVE => net.runNegativePathProbe(),
        NET_DIAG_OP_R4P => net.runR4pRuntimeProbe(),
        NET_DIAG_OP_ERRORS => runErrorVisibilityProbe(),
        else => return NET_DIAG_BAD_OP,
    };
    return if (ok) NET_DIAG_OK else NET_DIAG_FAILED;
}

fn runErrorVisibilityProbe() bool {
    const before = net.errorStatus();
    _ = net.runBackpressureProbe();
    net.logErrorStatus("netdiag-errors");
    const after = net.errorStatus();
    return after.total > before.total and after.tx_failures > before.tx_failures;
}

fn fillNetDiagResult(op: u32, out: *NetDiagResult) void {
    fillNetDiagTiming(&out.timing, net.timingStatus());
    fillNetDiagBackpressure(&out.backpressure, net.backpressureStatus());
    fillNetDiagCleanup(&out.cleanup, net.cleanupStatus());
    fillNetDiagDriver(&out.driver, net.driverLifecycleStatus());
    fillNetDiagErrors(&out.errors, net.errorStatus());
    fillNetDiagR4p(&out.r4p, net.r4pRuntimeStatus());
    fillNetDiagCounts(op, out);
}

fn fillNetDiagCounts(op: u32, out: *NetDiagResult) void {
    const counters = net.diagnosticCountersStatus();
    switch (op) {
        NET_DIAG_OP_TIMING => {
            out.tests = 1;
            out.cases = 8;
        },
        NET_DIAG_OP_BACKPRESSURE => {
            out.tests = 1;
            out.cases = 2;
        },
        NET_DIAG_OP_CLEANUP => {
            out.tests = out.cleanup.runs;
            out.cases = @as(u64, out.cleanup.last_udp_closed) +
                @as(u64, out.cleanup.last_tcp_connections) +
                @as(u64, out.cleanup.last_tcp_listeners);
        },
        NET_DIAG_OP_POWER => {
            out.tests = counters.power_lifecycle_tests;
            out.cases = counters.power_lifecycle_cases;
        },
        NET_DIAG_OP_LIFECYCLE => {
            out.tests = out.cleanup.runs;
            out.cases = 2;
        },
        NET_DIAG_OP_RESET => {
            out.tests = out.cleanup.runs;
            out.cases = 1;
        },
        NET_DIAG_OP_DRIVER => {
            out.tests = out.driver.tests;
            out.cases = out.driver.cases;
        },
        NET_DIAG_OP_ENVIRONMENT => {
            out.tests = counters.environment_contract_tests;
            out.cases = counters.environment_contract_cases;
        },
        NET_DIAG_OP_LIMIT => {
            out.tests = counters.limit_contract_tests;
            out.cases = counters.limit_contract_cases;
        },
        NET_DIAG_OP_CORPUS => {
            out.tests = counters.packet_corpus_tests;
            out.cases = counters.packet_corpus_cases;
        },
        NET_DIAG_OP_NEGATIVE => {
            out.tests = counters.negative_path_tests;
            out.cases = counters.negative_path_cases;
        },
        NET_DIAG_OP_R4P => {
            out.tests = 1;
            out.cases = @as(u64, out.r4p.active);
        },
        NET_DIAG_OP_ERRORS => {
            out.tests = 1;
            out.cases = out.errors.total;
        },
        else => {},
    }
}

fn fillNetDiagTiming(out: *NetDiagTiming, status: net.TimingStatus) void {
    out.* = .{
        .ticks = status.ticks,
        .hz = status.hz,
        .arp_cache_ttl_ticks = status.arp_cache_ttl_ticks,
        .arp_resolve_timeout_ticks = status.arp_resolve_timeout_ticks,
        .dhcp_timeout_ticks = status.dhcp_timeout_ticks,
        .dns_timeout_ticks = status.dns_timeout_ticks,
        .tcp_listen_timeout_ticks = status.tcp_listen_timeout_ticks,
        .tcp_retransmit_timeout_ticks = status.tcp_retransmit_timeout_ticks,
        .tcp_time_wait_ticks = status.tcp_time_wait_ticks,
        .service_operation_timeout_ticks = status.service_operation_timeout_ticks,
        .operation_status_count = status.operation_status_count,
    };
}

fn fillNetDiagBackpressure(out: *NetDiagBackpressure, status: net.BackpressureStatus) void {
    out.* = .{
        .packet_pool_used = status.packet_pool_used,
        .packet_pool_limit = status.packet_pool_limit,
        .app_ipv4_queued = status.app_ipv4_queued,
        .app_ipv4_queue_limit = status.app_ipv4_queue_limit,
        .udp_active_sockets = status.udp_active_sockets,
        .udp_socket_limit = status.udp_socket_limit,
        .udp_queued_packets = status.udp_queued_packets,
        .udp_queue_limit_total = status.udp_queue_limit_total,
        .tcp_active_connections = status.tcp_active_connections,
        .tcp_connection_limit = status.tcp_connection_limit,
        .tcp_active_listeners = status.tcp_active_listeners,
        .tcp_listener_limit = status.tcp_listener_limit,
        .tcp_buffer_size = status.tcp_buffer_size,
        .ipc_service_channels = status.ipc_service_channels,
        .ipc_service_handlers = status.ipc_service_handlers,
        .ipc_service_queued = status.ipc_service_queued,
        .ipc_service_queue_limit = status.ipc_service_queue_limit,
        .ipc_service_message_max = status.ipc_service_message_max,
        .ipc_service_queue_depth = status.ipc_service_queue_depth,
        .packet_drops = status.packet_drops,
        .app_ipv4_drops = status.app_ipv4_drops,
        .udp_drops = status.udp_drops,
        .tcp_rx_drops = status.tcp_rx_drops,
        .ipc_service_drops = status.ipc_service_drops,
        .tx_failures = status.tx_failures,
        .tx_no_adapter = status.tx_no_adapter,
        .tx_link_down = status.tx_link_down,
        .tx_busy = status.tx_busy,
        .tx_too_large = status.tx_too_large,
        .tx_unsupported = status.tx_unsupported,
        .tx_backend_error = status.tx_backend_error,
        .resource_queue_full = status.resource_queue_full,
        .resource_packet_drops = status.resource_packet_drops,
        .resource_buffer_small = status.resource_buffer_small,
        .resource_retries = status.resource_retries,
        .resource_timeouts = status.resource_timeouts,
        .resource_cancels = status.resource_cancels,
        .resource_backend_busy = status.resource_backend_busy,
    };
    copyFixedZ(out.tx_last_result[0..], status.tx_last_result);
    copyFixedZ(out.nonblocking_empty_status[0..], status.nonblocking_empty_status);
}

fn fillNetDiagCleanup(out: *NetDiagCleanup, status: net.CleanupStatus) void {
    out.* = .{
        .runs = status.runs,
        .link_down_cleanups = status.link_down_cleanups,
        .adapter_reset_cleanups = status.adapter_reset_cleanups,
        .adapter_unregister_cleanups = status.adapter_unregister_cleanups,
        .service_restart_cleanups = status.service_restart_cleanups,
        .poweroff_cleanups = status.poweroff_cleanups,
        .reboot_cleanups = status.reboot_cleanups,
        .udp_sockets_closed = status.udp_sockets_closed,
        .tcp_connections_aborted = status.tcp_connections_aborted,
        .tcp_listeners_closed = status.tcp_listeners_closed,
        .dhcp_operations_cancelled = status.dhcp_operations_cancelled,
        .dns_operations_cancelled = status.dns_operations_cancelled,
        .last_udp_closed = status.last_udp_closed,
        .last_tcp_connections = status.last_tcp_connections,
        .last_tcp_listeners = status.last_tcp_listeners,
    };
    copyFixedZ(out.last_reason[0..], status.last_reason);
}

fn fillNetDiagDriver(out: *NetDiagDriver, status: net.DriverLifecycleStatus) void {
    out.* = .{
        .tests = status.tests,
        .cases = status.cases,
    };
}

fn fillNetDiagErrors(out: *NetDiagErrors, status: net.ErrorStatus) void {
    out.* = .{
        .total = status.total,
        .packet_errors = status.packet_errors,
        .service_errors = status.service_errors,
        .adapter_errors = status.adapter_errors,
        .tx_failures = status.tx_failures,
        .protocol_errors = status.protocol_errors,
        .r4p_dispatch_failures = status.r4p_dispatch_failures,
    };
    copyFixedZ(out.last_adapter_error[0..], status.last_adapter_error);
    copyFixedZ(out.last_protocol_error[0..], status.last_protocol_error);
}

fn fillNetDiagR4p(out: *NetDiagR4p, status: net.R4pRuntimeStatus) void {
    out.* = .{
        .protocol_count = status.protocol_count,
        .active = status.active,
        .missing = status.missing,
        .r4p_rx = status.r4p_rx,
        .r4p_tx = status.r4p_tx,
        .r4p_control = status.r4p_control,
        .r4p_build = status.r4p_build,
        .r4p_classify = status.r4p_classify,
        .dispatch_failures = status.dispatch_failures,
    };
}

fn fillSerialLinkStatus(out: *SerialLinkStatus) void {
    var status: serial_link.Snapshot = .{};
    serial_link.snapshot(&status);
    out.* = .{
        .present = if (status.present) 1 else 0,
        .initialized = if (status.initialized) 1 else 0,
        .port_base = status.port_base,
        .version = serial_link.VERSION,
        .max_payload = serial_link.MAX_PAYLOAD,
        .last_type = status.last_type,
        .last_payload_len = status.last_payload_len,
        .last_message_len = status.last_message_len,
        .loopback_tests = status.loopback_tests,
        .host_tests = status.host_tests,
        .message_tx = status.message_tx,
        .message_rx = status.message_rx,
        .tx_skipped = status.tx_skipped,
        .tx_frames = status.tx_frames,
        .tx_bytes = status.tx_bytes,
        .rx_frames = status.rx_frames,
        .rx_bytes = status.rx_bytes,
        .polls = status.polls,
        .bad_magic = status.bad_magic,
        .bad_version = status.bad_version,
        .bad_length = status.bad_length,
        .checksum_errors = status.checksum_errors,
        .overflows = status.overflows,
        .timeouts = status.timeouts,
        .r4p_build = status.r4p_build,
        .r4p_parse = status.r4p_parse,
        .r4p_self = status.r4p_self,
        .r4p_fallbacks = 0,
        .r4p_dispatch_failures = status.r4p_dispatch_failures,
    };
    const payload_len: usize = @intCast(status.last_payload_len);
    const message_len: usize = @intCast(status.last_message_len);
    if (payload_len != 0) @memcpy(out.last_payload[0..payload_len], status.last_payload[0..payload_len]);
    if (message_len != 0) @memcpy(out.last_message[0..message_len], status.last_message[0..message_len]);
    copyFixedZ(out.last_error[0..], status.last_error[0..]);
}

fn fillNetConfigSnapshot(out: *NetConfigSnapshot) void {
    const cfg = net_config.settings();
    out.* = .{
        .local_ip = cfg.local_ip,
        .netmask = cfg.netmask,
        .gateway_ip = cfg.gateway_ip,
        .dns_ip = cfg.dns_ip,
        .flags = (if (cfg.configured) NET_CONFIG_FLAG_CONFIGURED else 0) |
            (if (cfg.dns_configured) NET_CONFIG_FLAG_DNS_CONFIGURED else 0),
        .adapter_count = @intCast(net.count()),
        .invalid_options = @intCast(@min(cfg.invalid_options, 0xFFFF_FFFF)),
    };
    copyFixedZ(out.source[0..], net_config.sourceName());
    copyFixedZ(out.link[0..], "none");
    copyFixedZ(out.last_error[0..], cfg.last_error);
    if (net.get(0)) |adapter| {
        out.flags |= NET_CONFIG_FLAG_ADAPTER_PRESENT;
        if (adapter.link == .up) out.flags |= NET_CONFIG_FLAG_LINK_UP;
        out.mac = adapter.mac;
        out.mtu = adapter.mtu;
        copyFixedZ(out.adapter_name[0..], adapter.name);
        copyFixedZ(out.link[0..], linkName(adapter.link));
        copyFixedZ(out.last_error[0..], adapter.stats.last_error);
    }
}

fn fillNetDetailRuntime(out: *NetDetailProtocolRuntime, stats: net.ProtocolRuntimeStats) void {
    out.* = .{
        .active_r4p = if (stats.active_r4p) 1 else 0,
        .r4p_state = stats.r4p_state,
        .builtin_fallback = if (stats.builtin_fallback) 1 else 0,
        .fallback_policy = stats.fallback_policy,
        .fallback_decision = stats.fallback_decision,
        .r4p_rx = stats.r4p_rx,
        .r4p_tx = stats.r4p_tx,
        .r4p_control = stats.r4p_control,
        .r4p_build = stats.r4p_build,
        .r4p_classify = stats.r4p_classify,
        .fallbacks = 0,
        .dispatch_failures = stats.dispatch_failures,
    };
}

fn fillTcpConnectionInfo(out: *TcpConnectionInfo, info: net.TcpConnectionInfo) void {
    out.* = .{
        .id = info.id,
        .state = info.state,
        .local_port = info.local_port,
        .remote_port = info.remote_port,
        .remote_ip = info.remote_ip,
        .tx_bytes = info.tx_bytes,
        .rx_bytes = info.rx_bytes,
        .pending_rx = info.pending_rx,
        .retransmits = info.retransmits,
        .rx_window = info.rx_window,
        .tx_window = info.tx_window,
        .tx_ack = info.tx_ack,
        .rx_drops = info.rx_drops,
        .seq = info.seq,
        .ack = info.ack,
        .last_seq = info.last_seq,
        .last_ack = info.last_ack,
        .last_flags = info.last_flags,
        .last_payload_len = info.last_payload_len,
    };
}

const PersistentNetConfigRewrite = struct {
    local_ip: [4]u8,
    netmask: [4]u8,
    gateway_ip: [4]u8,
    dns_ip: [4]u8,
    dns_configured: bool,
};

fn rewritePersistentNetConfigBytes(
    input: []const u8,
    output: []u8,
    opaque_context: *const anyopaque,
) ?usize {
    const settings: *const PersistentNetConfigRewrite = @ptrCast(@alignCast(opaque_context));
    return net_config_writer.rewriteConfig(input, output, .{
        .local_ip = settings.local_ip,
        .netmask = settings.netmask,
        .gateway_ip = settings.gateway_ip,
        .dns_ip = settings.dns_ip,
        .dns_configured = settings.dns_configured,
    });
}

fn writePersistentNetConfig(local_ip: [4]u8, netmask: [4]u8, gateway_ip: [4]u8, dns_ip: [4]u8, dns_configured: bool) i32 {
    var input: [4096]u8 = .{0} ** 4096;
    var output: [4096]u8 = .{0} ** 4096;
    const settings = PersistentNetConfigRewrite{
        .local_ip = local_ip,
        .netmask = netmask,
        .gateway_ip = gateway_ip,
        .dns_ip = dns_ip,
        .dns_configured = dns_configured,
    };
    const rc = r4sys.rewriteFileUnderGate(
        "C:\\CONFIG.R4S",
        input[0..],
        output[0..],
        &settings,
        rewritePersistentNetConfigBytes,
    );
    if (rc == -5) return NET_CONFIG_BUFFER_SMALL;
    if (rc < 0) return NET_CONFIG_WRITE_FAILED;
    return NET_CONFIG_OK;
}

fn parseFixedIpv4(bytes: []const u8) ?[4]u8 {
    const text = fixedText(bytes);
    if (text.len == 0) return null;
    return net_config.parseIpv4(text);
}

fn fixedText(bytes: []const u8) []const u8 {
    var end: usize = 0;
    while (end < bytes.len and bytes[end] != 0) : (end += 1) {}
    return trimAscii(bytes[0..end]);
}

fn validNetmask(mask: [4]u8) bool {
    const value: u32 = (@as(u32, mask[0]) << 24) | (@as(u32, mask[1]) << 16) | (@as(u32, mask[2]) << 8) | mask[3];
    if (value == 0) return false;
    var seen_zero = false;
    var bit: usize = 0;
    while (bit < 32) : (bit += 1) {
        const set = (value & (@as(u32, 0x8000_0000) >> @as(u5, @intCast(bit)))) != 0;
        if (!set) {
            seen_zero = true;
        } else if (seen_zero) {
            return false;
        }
    }
    return true;
}

fn linkName(link: net.Link) []const u8 {
    return switch (link) {
        .unknown => "unknown",
        .down => "down",
        .up => "up",
    };
}

fn netLifecycleCode(lifecycle: net.Lifecycle) u32 {
    return switch (lifecycle) {
        .unknown => 0,
        .registered => 1,
        .active => 2,
        .link_down => 3,
        .resetting => 4,
        .shutdown => 5,
    };
}

fn txResultCode(result: net.TxResult) i32 {
    return switch (result) {
        .ok => 0,
        .no_adapter => 1,
        .link_down => 2,
        .busy => 3,
        .too_large => 4,
        .unsupported => 5,
        .backend_error => 6,
    };
}

fn trimAscii(s: []const u8) []const u8 {
    var start: usize = 0;
    var end = s.len;
    while (start < end and (s[start] == ' ' or s[start] == '\t')) : (start += 1) {}
    while (end > start and (s[end - 1] == ' ' or s[end - 1] == '\t')) : (end -= 1) {}
    return s[start..end];
}

fn copyFixedZ(out: []u8, value: []const u8) void {
    @memset(out, 0);
    if (out.len == 0) return;
    const count = @min(value.len, out.len - 1);
    if (count > 0) @memcpy(out[0..count], value[0..count]);
}
