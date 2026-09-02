const ipc = @import("../kernel/ipc.zig");
const bootlog = @import("../kernel/bootlog.zig");
const r4x_api = @import("../program/r4x_api.zig");
const dns = @import("dns.zig");
const net = @import("core.zig");
const net_timing = @import("timing.zig");

pub const MAGIC: u32 = 0x4350494E; // "NIPC"
pub const VERSION: u16 = 1;
pub const OP_STATUS: u16 = 1;
pub const OP_SERVICE_RESTART: u16 = 0x00F1;
pub const OP_DHCP_STATUS_RESULT: u16 = 0x0210;
pub const OP_DHCP_ACQUIRE: u16 = 0x0201;
pub const OP_DHCP_RENEW: u16 = 0x0202;
pub const OP_DHCP_RELEASE: u16 = 0x0203;
pub const OP_DHCP_ACQUIRE_RESULT: u16 = 0x0211;
pub const OP_DHCP_RENEW_RESULT: u16 = 0x0212;
pub const OP_DHCP_RELEASE_RESULT: u16 = 0x0213;
pub const OP_DNS_RESOLVE_A: u16 = 0x0101;
pub const OP_DNS_RESOLVE_A_SERVER: u16 = 0x0102;
pub const OP_DNS_STATUS_RESULT: u16 = 0x0110;
pub const OP_DNS_RESOLVE_A_RESULT: u16 = 0x0111;
pub const OP_DNS_RESOLVE_A_SERVER_RESULT: u16 = 0x0112;
pub const OP_TCP_CONNECT: u16 = 0x0301;
pub const OP_TCP_WRITE: u16 = 0x0302;
pub const OP_TCP_READ: u16 = 0x0303;
pub const OP_TCP_CLOSE: u16 = 0x0304;
pub const OP_TCP_LISTEN: u16 = 0x0305;
pub const OP_TCP_ACCEPT_READ: u16 = 0x0306;
pub const OP_TCP_CLOSE_LISTEN: u16 = 0x0307;
pub const OP_TCP_CONNECTIONS: u16 = 0x0308;
pub const OP_TCP_POLL: u16 = 0x0309;
pub const OP_TCP_ACCEPT: u16 = 0x030A;
pub const OP_TCP_STATUS_RESULT: u16 = 0x0310;
pub const OP_TCP_CONNECT_RESULT: u16 = 0x0311;
pub const OP_TCP_WRITE_RESULT: u16 = 0x0312;
pub const OP_TCP_READ_RESULT: u16 = 0x0313;
pub const OP_TCP_CLOSE_RESULT: u16 = 0x0314;
pub const OP_TCP_LISTEN_RESULT: u16 = 0x0315;
pub const OP_TCP_ACCEPT_READ_RESULT: u16 = 0x0316;
pub const OP_TCP_CLOSE_LISTEN_RESULT: u16 = 0x0317;
pub const OP_TCP_POLL_RESULT: u16 = 0x0318;
pub const OP_TCP_ACCEPT_RESULT: u16 = 0x0319;
pub const OP_TCP_ABORT_RESULT: u16 = 0x031A;
pub const OP_TCP_ACCEPT_POLL_RESULT: u16 = 0x031B;
pub const OP_TCP_RETRANSMIT_RESULT: u16 = 0x031C;
pub const OP_UDP_BIND: u16 = 0x0401;
pub const OP_UDP_SENDTO: u16 = 0x0402;
pub const OP_UDP_RECV: u16 = 0x0403;
pub const OP_UDP_CLOSE: u16 = 0x0404;
pub const OP_UDP_STATUS_RESULT: u16 = 0x0410;
pub const OP_UDP_BIND_RESULT: u16 = 0x0411;
pub const OP_UDP_SENDTO_RESULT: u16 = 0x0412;
pub const OP_UDP_RECV_RESULT: u16 = 0x0413;
pub const OP_UDP_CLOSE_RESULT: u16 = 0x0414;
pub const RESULT_OK: i32 = 0;
pub const RESULT_BAD_REQUEST: i32 = -1;
pub const RESULT_BAD_SERVICE: i32 = -2;
pub const RESULT_BAD_OP: i32 = -3;
pub const HEADER_SIZE: usize = 24;

const TCP_SERVICE_HANDLES: usize = 16;
const TCP_STALE_HANDLE_TOMBSTONES: usize = 16;
const TCP_MESSAGE_PAYLOAD_MAX: usize = ipc.MAX_MESSAGE_SIZE - HEADER_SIZE;
const TCP_WRITE_PAYLOAD_MAX: usize = TCP_MESSAGE_PAYLOAD_MAX - 4;
// 0.56.37: Draht-sichere Segmentgroesse fuer die kernelseitige
// Zerlegung grosser IPC-Writes (MTU 1500 - 20 IP - 20 TCP = 1460;
// tcpWrite baut genau EIN Segment pro Aufruf). Der erste Verdacht
// "1514-Byte-Frames brechen" war falsch - der echte Bruch war der
// unguarded RTL8139-transmit unter dem erstmals feuernden
// net-rx-Retransmit (TX-Lock im Treiber, siehe RTL8139.R4D 0.56.37).
const TCP_WIRE_SEGMENT_MAX: usize = 1460;
const DHCP_STATUS_MAGIC: u32 = 0x53504844;
const DHCP_STATUS_VERSION: u16 = 1;
const DHCP_RESULT_MAGIC: u32 = 0x50514844;
const DHCP_RESULT_VERSION: u16 = 1;
const DNS_STATUS_MAGIC: u32 = 0x53534844;
const DNS_STATUS_VERSION: u16 = 1;
const DNS_RESULT_MAGIC: u32 = 0x53524444;
const DNS_RESULT_VERSION: u16 = 1;
const TCP_STATUS_MAGIC: u32 = 0x53544354;
const TCP_STATUS_VERSION: u16 = 2;
const TCP_RESULT_MAGIC: u32 = 0x53524354;
const TCP_RESULT_VERSION: u16 = 2;
const UDP_STATUS_MAGIC: u32 = 0x53504455;
const UDP_STATUS_VERSION: u16 = 2;
const UDP_RESULT_MAGIC: u32 = 0x52504455;
const UDP_RESULT_VERSION: u16 = 2;
const DHCP_ACTION_ACQUIRE: u16 = 1;
const DHCP_ACTION_RENEW: u16 = 2;
const DHCP_ACTION_RELEASE: u16 = 3;
const DNS_ACTION_RESOLVE_A: u16 = 1;
const DNS_ACTION_RESOLVE_A_SERVER: u16 = 2;
const TCP_ACTION_CONNECT: u16 = 1;
const TCP_ACTION_WRITE: u16 = 2;
const TCP_ACTION_READ: u16 = 3;
const TCP_ACTION_CLOSE: u16 = 4;
const TCP_ACTION_LISTEN: u16 = 5;
const TCP_ACTION_ACCEPT_READ: u16 = 6;
const TCP_ACTION_CLOSE_LISTEN: u16 = 7;
const TCP_ACTION_POLL: u16 = 8;
const TCP_ACTION_ACCEPT: u16 = 9;
const TCP_ACTION_ABORT: u16 = 10;
const TCP_ACTION_ACCEPT_POLL: u16 = 11;
const TCP_ACTION_RETRANSMIT: u16 = 12;
const UDP_ACTION_BIND: u16 = 1;
const UDP_ACTION_SENDTO: u16 = 2;
const UDP_ACTION_RECV: u16 = 3;
const UDP_ACTION_CLOSE: u16 = 4;
const DHCP_FLAG_BOUND: u32 = 1 << 0;
const DHCP_FLAG_PENDING: u32 = 1 << 1;
const DHCP_FLAG_DNS_CONFIGURED: u32 = 1 << 2;
const DHCP_FLAG_DESIRED: u32 = 1 << 3;
const DHCP_FLAG_TASK_STARTED: u32 = 1 << 4;
const DHCP_FLAG_LINK_UP: u32 = 1 << 5;
const DHCP_FLAG_RETRY_WAIT: u32 = 1 << 6;
const DNS_FLAG_OK: u32 = 1 << 0;
const DNS_FLAG_PENDING: u32 = 1 << 1;
const DNS_FLAG_CACHE_VALID: u32 = 1 << 2;
const DNS_FLAG_CACHE_HIT: u32 = 1 << 3;
const DNS_FLAG_EXPLICIT_SERVER: u32 = 1 << 4;
pub const DNS_FLAG_CACHE_VALID_PUBLIC: u32 = DNS_FLAG_CACHE_VALID;
pub const DNS_FLAG_CACHE_HIT_PUBLIC: u32 = DNS_FLAG_CACHE_HIT;
const TCP_FLAG_OK: u32 = 1 << 0;
const TCP_FLAG_TIMEOUT: u32 = 1 << 1;
const TCP_FLAG_DATA: u32 = 1 << 2;
const TCP_FLAG_HANDLE_VALID: u32 = 1 << 3;
const TCP_FLAG_CONN_VALID: u32 = 1 << 4;
const TCP_FLAG_REMOTE_VALID: u32 = 1 << 5;
const TCP_FLAG_LISTENER: u32 = 1 << 6;
const TCP_FLAG_LIFECYCLE_VALID: u32 = 1 << 7;
const TCP_STATUS_FLAG_LISTENER_ACTIVE: u32 = 1 << 0;
const TCP_STATUS_FLAG_LAST_SEGMENT: u32 = 1 << 1;
const TCP_STATUS_FLAG_LIFECYCLE_VALID: u32 = 1 << 2;
const UDP_FLAG_OK: u32 = 1 << 0;
const UDP_FLAG_DATA: u32 = 1 << 1;
const UDP_FLAG_HANDLE_VALID: u32 = 1 << 2;
const UDP_FLAG_REMOTE_VALID: u32 = 1 << 3;
const UDP_FLAG_LIFECYCLE_VALID: u32 = 1 << 4;
const UDP_STATUS_FLAG_LIFECYCLE_VALID: u32 = UDP_FLAG_LIFECYCLE_VALID;
const SERVICE_STATUS_SHIFT: u5 = 24;
const SERVICE_STATUS_MASK: u32 = 0xFF000000;
pub const SOCKET_LIFECYCLE_UNKNOWN_PUBLIC: u32 = 0;
pub const SOCKET_LIFECYCLE_ACTIVE_PUBLIC: u32 = 1;
pub const SOCKET_LIFECYCLE_CLOSED_PUBLIC: u32 = 2;
pub const SOCKET_LIFECYCLE_RESET_PUBLIC: u32 = 3;
pub const SOCKET_LIFECYCLE_TIMEOUT_PUBLIC: u32 = 4;
pub const SOCKET_LIFECYCLE_PEER_GONE_PUBLIC: u32 = 5;
pub const SOCKET_LIFECYCLE_LOCAL_ABORT_PUBLIC: u32 = 6;
pub const SOCKET_LIFECYCLE_LOCAL_CLOSE_PUBLIC: u32 = 7;
pub const SOCKET_LIFECYCLE_PENDING_CLOSE_PUBLIC: u32 = 8;
pub const SOCKET_LIFECYCLE_WOULD_BLOCK_PUBLIC: u32 = 9;
pub const SOCKET_LIFECYCLE_BAD_HANDLE_PUBLIC: u32 = 10;
pub const SOCKET_LIFECYCLE_OWNER_MISMATCH_PUBLIC: u32 = 11;
pub const SOCKET_LIFECYCLE_LISTENER_PUBLIC: u32 = 12;
pub const SOCKET_LIFECYCLE_DROPPED_PUBLIC: u32 = 13;
pub const TCP_FLAG_TIMEOUT_PUBLIC: u32 = TCP_FLAG_TIMEOUT;
pub const TCP_FLAG_DATA_PUBLIC: u32 = TCP_FLAG_DATA;
pub const TCP_FLAG_HANDLE_VALID_PUBLIC: u32 = TCP_FLAG_HANDLE_VALID;
pub const TCP_FLAG_CONN_VALID_PUBLIC: u32 = TCP_FLAG_CONN_VALID;
pub const TCP_FLAG_LIFECYCLE_VALID_PUBLIC: u32 = TCP_FLAG_LIFECYCLE_VALID;
pub const TCP_STATUS_FLAG_LISTENER_ACTIVE_PUBLIC: u32 = TCP_STATUS_FLAG_LISTENER_ACTIVE;
pub const TCP_STATUS_FLAG_LAST_SEGMENT_PUBLIC: u32 = TCP_STATUS_FLAG_LAST_SEGMENT;
pub const TCP_STATUS_FLAG_LIFECYCLE_VALID_PUBLIC: u32 = TCP_STATUS_FLAG_LIFECYCLE_VALID;
pub const UDP_FLAG_DATA_PUBLIC: u32 = UDP_FLAG_DATA;
pub const UDP_FLAG_HANDLE_VALID_PUBLIC: u32 = UDP_FLAG_HANDLE_VALID;
pub const UDP_FLAG_REMOTE_VALID_PUBLIC: u32 = UDP_FLAG_REMOTE_VALID;
pub const UDP_FLAG_LIFECYCLE_VALID_PUBLIC: u32 = UDP_FLAG_LIFECYCLE_VALID;
pub const UDP_STATUS_FLAG_LIFECYCLE_VALID_PUBLIC: u32 = UDP_STATUS_FLAG_LIFECYCLE_VALID;
pub const SERVICE_STATUS_MASK_PUBLIC: u32 = SERVICE_STATUS_MASK;

const TcpServiceHandle = struct {
    used: bool = false,
    handle: u32 = 0,
    conn_id: u32 = 0,
    owner_id: u16 = 0,
};

const TcpStaleHandle = struct {
    used: bool = false,
    handle: u32 = 0,
    conn_id: u32 = 0,
    owner_id: u16 = 0,
    lifecycle_cause: u32 = SOCKET_LIFECYCLE_CLOSED_PUBLIC,
};

pub const DhcpServiceStatus = extern struct {
    magic: u32 = DHCP_STATUS_MAGIC,
    version: u16 = DHCP_STATUS_VERSION,
    runtime_state: u16 = 0,
    flags: u32 = 0,
    xid: u32 = 0,
    offered_ip: [4]u8 = .{0} ** 4,
    server_ip: [4]u8 = .{0} ** 4,
    netmask: [4]u8 = .{0} ** 4,
    gateway_ip: [4]u8 = .{0} ** 4,
    dns_ip: [4]u8 = .{0} ** 4,
    lease_seconds: u32 = 0,
    renew_seconds: u32 = 0,
    rebind_seconds: u32 = 0,
    elapsed_seconds: u32 = 0,
    remaining_seconds: u32 = 0,
    renew_in_seconds: u32 = 0,
    rebind_in_seconds: u32 = 0,
    last_attempt: u32 = 0,
    last_type: u32 = 0,
    discover_tx: u64 = 0,
    offer_rx: u64 = 0,
    request_tx: u64 = 0,
    ack_rx: u64 = 0,
    nak_rx: u64 = 0,
    release_tx: u64 = 0,
    retries: u64 = 0,
    timeouts: u64 = 0,
    release_errors: u64 = 0,
    malformed: u64 = 0,
    self_tests: u64 = 0,
    pending_label: [16]u8 = .{0} ** 16,
    last_error: [32]u8 = .{0} ** 32,
};

pub const DhcpServiceResult = extern struct {
    magic: u32 = DHCP_RESULT_MAGIC,
    version: u16 = DHCP_RESULT_VERSION,
    action: u16 = 0,
    result: i32 = 6,
    flags: u32 = 0,
    offered_ip: [4]u8 = .{0} ** 4,
    server_ip: [4]u8 = .{0} ** 4,
    netmask: [4]u8 = .{0} ** 4,
    gateway_ip: [4]u8 = .{0} ** 4,
    dns_ip: [4]u8 = .{0} ** 4,
    lease_seconds: u32 = 0,
    elapsed_seconds: u32 = 0,
    remaining_seconds: u32 = 0,
    renew_in_seconds: u32 = 0,
    rebind_in_seconds: u32 = 0,
    last_attempt: u32 = 0,
    last_type: u32 = 0,
    discover_tx: u64 = 0,
    offer_rx: u64 = 0,
    request_tx: u64 = 0,
    ack_rx: u64 = 0,
    nak_rx: u64 = 0,
    release_tx: u64 = 0,
    retries: u64 = 0,
    timeouts: u64 = 0,
    release_errors: u64 = 0,
    malformed: u64 = 0,
};

pub const DnsServiceStatus = extern struct {
    magic: u32 = DNS_STATUS_MAGIC,
    version: u16 = DNS_STATUS_VERSION,
    reserved: u16 = 0,
    flags: u32 = 0,
    last_result: i32 = 0,
    last_id: u16 = 0,
    name_len: u16 = 0,
    pending_name_len: u16 = 0,
    cache_name_len: u16 = 0,
    last_server: [4]u8 = .{0} ** 4,
    last_answer: [4]u8 = .{0} ** 4,
    cache_server: [4]u8 = .{0} ** 4,
    cache_answer: [4]u8 = .{0} ** 4,
    cache_age_seconds: u32 = 0,
    cache_ttl_seconds: u32 = 0,
    cache_remaining_seconds: u32 = 0,
    queries_tx: u64 = 0,
    resolve_requests: u64 = 0,
    responses_rx: u64 = 0,
    a_records: u64 = 0,
    timeouts: u64 = 0,
    nxdomain: u64 = 0,
    tx_errors: u64 = 0,
    malformed: u64 = 0,
    self_tests: u64 = 0,
    cache_hits: u64 = 0,
    cache_stores: u64 = 0,
    name: [64]u8 = .{0} ** 64,
    pending_name: [64]u8 = .{0} ** 64,
    cache_name: [64]u8 = .{0} ** 64,
    last_error: [32]u8 = .{0} ** 32,
};

pub const DnsServiceResult = extern struct {
    magic: u32 = DNS_RESULT_MAGIC,
    version: u16 = DNS_RESULT_VERSION,
    action: u16 = 0,
    result: i32 = -13,
    flags: u32 = 0,
    answer: [4]u8 = .{0} ** 4,
    server: [4]u8 = .{0} ** 4,
    cache_answer: [4]u8 = .{0} ** 4,
    cache_age_seconds: u32 = 0,
    cache_ttl_seconds: u32 = 0,
    cache_remaining_seconds: u32 = 0,
    queries_tx: u64 = 0,
    resolve_requests: u64 = 0,
    responses_rx: u64 = 0,
    a_records: u64 = 0,
    timeouts: u64 = 0,
    nxdomain: u64 = 0,
    tx_errors: u64 = 0,
    malformed: u64 = 0,
    cache_hits: u64 = 0,
    cache_stores: u64 = 0,
    last_id: u16 = 0,
    name_len: u16 = 0,
    name: [96]u8 = .{0} ** 96,
    last_error: [32]u8 = .{0} ** 32,
};

pub const TcpServiceStatus = extern struct {
    magic: u32 = TCP_STATUS_MAGIC,
    version: u16 = TCP_STATUS_VERSION,
    reserved: u16 = 0,
    flags: u32 = 0,
    request_owner_id: u16 = 0,
    owned_handles: u16 = 0,
    legacy_handles: u16 = 0,
    reserved3: u16 = 0,
    active_connections: u32 = 0,
    max_connections: u32 = 0,
    active_listeners: u32 = 0,
    handle_count: u32 = 0,
    max_handles: u32 = TCP_SERVICE_HANDLES,
    tcp_buffer_size: u32 = @intCast(net.TCP_BUFFER_SIZE),
    message_payload_max: u32 = TCP_MESSAGE_PAYLOAD_MAX,
    write_max: u32 = TCP_WRITE_PAYLOAD_MAX,
    read_max: u32 = 0,
    rx_segments: u64 = 0,
    tx_segments: u64 = 0,
    syn_tx: u64 = 0,
    synack_rx: u64 = 0,
    ack_tx: u64 = 0,
    data_tx: u64 = 0,
    data_rx: u64 = 0,
    fin_tx: u64 = 0,
    rst_rx: u64 = 0,
    checksum_errors: u64 = 0,
    timeouts: u64 = 0,
    self_tests: u64 = 0,
    synack_tx: u64 = 0,
    listen_syn_rx: u64 = 0,
    accepts: u64 = 0,
    retransmits: u64 = 0,
    rx_drops: u64 = 0,
    last_source_port: u16 = 0,
    last_dest_port: u16 = 0,
    last_flags: u16 = 0,
    reserved2: u16 = 0,
    last_seq: u32 = 0,
    last_ack: u32 = 0,
    last_payload_len: u32 = 0,
    owner_mismatches: u64 = 0,
    last_error: [32]u8 = .{0} ** 32,
    stale_handles_reaped: u64 = 0,
    stale_tombstones: u32 = 0,
    last_lifecycle_cause: u32 = SOCKET_LIFECYCLE_UNKNOWN_PUBLIC,
    lifecycle_closed: u64 = 0,
    lifecycle_reset: u64 = 0,
    lifecycle_timeout: u64 = 0,
    lifecycle_peer_gone: u64 = 0,
    lifecycle_local_abort: u64 = 0,
    lifecycle_local_close: u64 = 0,
    lifecycle_pending_close: u64 = 0,
    lifecycle_would_block: u64 = 0,
    lifecycle_bad_handle: u64 = 0,
    lifecycle_owner_mismatch: u64 = 0,
    read_wait_timeouts: u64 = 0,
    accept_wait_timeouts: u64 = 0,
    write_would_block: u64 = 0,
    close_cancelled: u64 = 0,
};

pub const TcpServiceResult = extern struct {
    magic: u32 = TCP_RESULT_MAGIC,
    version: u16 = TCP_RESULT_VERSION,
    action: u16 = 0,
    result: i32 = -1,
    flags: u32 = 0,
    handle: u32 = 0,
    conn_id: u32 = 0,
    bytes: u32 = 0,
    requested_bytes: u32 = 0,
    port: u16 = 0,
    remote_port: u16 = 0,
    remote_ip: [4]u8 = .{0} ** 4,
    active_connections: u32 = 0,
    max_connections: u32 = 0,
    active_listeners: u32 = 0,
    handle_count: u32 = 0,
    max_handles: u32 = TCP_SERVICE_HANDLES,
    tcp_buffer_size: u32 = @intCast(net.TCP_BUFFER_SIZE),
    message_payload_max: u32 = TCP_MESSAGE_PAYLOAD_MAX,
    write_max: u32 = TCP_WRITE_PAYLOAD_MAX,
    read_max: u32 = 0,
    pending_rx: u32 = 0,
    rx_window: u32 = 0,
    tx_window: u32 = 0,
    tx_seq: u32 = 0,
    tx_ack: u32 = 0,
    retransmits: u32 = 0,
    rx_drops: u32 = 0,
    local_ip: [4]u8 = .{0} ** 4,
    local_port: u16 = 0,
    reserved3: u16 = 0,
    last_error: [32]u8 = .{0} ** 32,
    lifecycle_cause: u32 = SOCKET_LIFECYCLE_UNKNOWN_PUBLIC,
    service_status: u32 = 0,
    owner_id: u16 = 0,
    reserved4: u16 = 0,
};

const TCP_RESULT_DATA_MAX: usize = TCP_MESSAGE_PAYLOAD_MAX - @sizeOf(TcpServiceResult);

pub const UdpServiceStatus = extern struct {
    magic: u32 = UDP_STATUS_MAGIC,
    version: u16 = UDP_STATUS_VERSION,
    reserved: u16 = 0,
    flags: u32 = 0,
    active_sockets: u32 = 0,
    max_sockets: u32 = 0,
    queued_packets: u32 = 0,
    queue_limit: u32 = 0,
    payload_max: u32 = 0,
    message_payload_max: u32 = TCP_MESSAGE_PAYLOAD_MAX,
    send_max: u32 = 0,
    recv_max: u32 = 0,
    delivered: u64 = 0,
    drops: u64 = 0,
    last_error: [32]u8 = .{0} ** 32,
    last_lifecycle_cause: u32 = SOCKET_LIFECYCLE_UNKNOWN_PUBLIC,
    reserved2: u32 = 0,
    lifecycle_closed: u64 = 0,
    lifecycle_timeout: u64 = 0,
    lifecycle_local_close: u64 = 0,
    lifecycle_would_block: u64 = 0,
    lifecycle_bad_handle: u64 = 0,
    lifecycle_dropped: u64 = 0,
};

pub const UdpServiceResult = extern struct {
    magic: u32 = UDP_RESULT_MAGIC,
    version: u16 = UDP_RESULT_VERSION,
    action: u16 = 0,
    result: i32 = -1,
    flags: u32 = 0,
    handle: u32 = 0,
    bytes: u32 = 0,
    requested_bytes: u32 = 0,
    source_ip: [4]u8 = .{0} ** 4,
    dest_ip: [4]u8 = .{0} ** 4,
    source_port: u16 = 0,
    dest_port: u16 = 0,
    active_sockets: u32 = 0,
    max_sockets: u32 = 0,
    queued_packets: u32 = 0,
    queue_limit: u32 = 0,
    payload_max: u32 = 0,
    message_payload_max: u32 = TCP_MESSAGE_PAYLOAD_MAX,
    send_max: u32 = 0,
    recv_max: u32 = 0,
    delivered: u64 = 0,
    drops: u64 = 0,
    last_error: [32]u8 = .{0} ** 32,
    lifecycle_cause: u32 = SOCKET_LIFECYCLE_UNKNOWN_PUBLIC,
    service_status: u32 = 0,
};

const UDP_SEND_PAYLOAD_MAX: usize = TCP_MESSAGE_PAYLOAD_MAX - 10;
const UDP_RESULT_DATA_MAX: usize = TCP_MESSAGE_PAYLOAD_MAX - @sizeOf(UdpServiceResult);

pub const RestartStatus = struct {
    restarts: u64 = 0,
    last_queued: u32 = 0,
    last_handles: u32 = 0,
    last_tombstones: u32 = 0,
};

pub const ServiceBackpressureStatus = struct {
    payload_too_large: u64 = 0,
    response_buffer_small: u64 = 0,
    send_failures: u64 = 0,
    stale_skips: u64 = 0,
    request_payload_limit: u32 = @intCast(TCP_MESSAGE_PAYLOAD_MAX),
    response_buffer_limit: u32 = @intCast(ipc.MAX_MESSAGE_SIZE),
};

pub const ServiceLifecycleStatus = struct {
    close_ok: u64 = 0,
    close_failed: u64 = 0,
    close_cancelled: u64 = 0,
    cleanup_aborts: u64 = 0,
    cleanup_poweroff: u64 = 0,
    cleanup_reboot: u64 = 0,
};

const SocketLifecycleCounters = struct {
    last_cause: u32 = SOCKET_LIFECYCLE_UNKNOWN_PUBLIC,
    closed: u64 = 0,
    reset: u64 = 0,
    timeout: u64 = 0,
    peer_gone: u64 = 0,
    local_abort: u64 = 0,
    local_close: u64 = 0,
    pending_close: u64 = 0,
    would_block: u64 = 0,
    bad_handle: u64 = 0,
    owner_mismatch: u64 = 0,
    dropped: u64 = 0,
};

var tcp_handles: [TCP_SERVICE_HANDLES]TcpServiceHandle = .{TcpServiceHandle{}} ** TCP_SERVICE_HANDLES;
var tcp_stale_tombstones: [TCP_STALE_HANDLE_TOMBSTONES]TcpStaleHandle = .{TcpStaleHandle{}} ** TCP_STALE_HANDLE_TOMBSTONES;
var tcp_stale_tombstone_next: usize = 0;
var tcp_next_handle: u32 = 1;
var tcp_owner_mismatches: u64 = 0;
var tcp_stale_handles_reaped: u64 = 0;
var tcp_read_wait_timeouts: u64 = 0;
var tcp_accept_wait_timeouts: u64 = 0;
var tcp_write_would_block: u64 = 0;
var stale_responses_skipped: u64 = 0;
var tcp_lifecycle: SocketLifecycleCounters = .{};
var udp_lifecycle: SocketLifecycleCounters = .{};
var restart_status: RestartStatus = .{};
var service_backpressure_status: ServiceBackpressureStatus = .{};
var service_lifecycle_status: ServiceLifecycleStatus = .{};

pub fn init() void {
    _ = ipc.registerService(ipc.CHANNEL_NET_DHCP, "net.dhcp", handle);
    _ = ipc.registerService(ipc.CHANNEL_NET_DNS, "net.dns", handle);
    _ = ipc.registerService(ipc.CHANNEL_NET_TCP, "net.tcp", handle);
    _ = ipc.registerService(ipc.CHANNEL_NET_UDP, "net.udp", handle);
}

pub fn restartServices(reason: []const u8) RestartStatus {
    _ = reason;
    const before_queues = queueStatus();
    restart_status.restarts += 1;
    restart_status.last_queued = before_queues.queued;
    restart_status.last_handles = tcpHandleCountRaw();
    restart_status.last_tombstones = tcpTombstoneCountRaw();
    const cleanup = net.cleanupNetworkOperations("service-restart");
    noteCleanupLifecycle("service-restart", cleanup);
    clearTcpServiceState();
    drainServiceQueues();
    init();
    return restart_status;
}

pub fn restartStatus() RestartStatus {
    return restart_status;
}

fn writeRestartStatus(w: *Writer, status: RestartStatus) void {
    w.text("ok restarts=");
    w.num(status.restarts);
    w.text(" last_queue=");
    w.num(status.last_queued);
    w.text(" last_handles=");
    w.num(status.last_handles);
    w.text(" last_tomb=");
    w.num(status.last_tombstones);
}

fn writeServiceStatusLine(w: *Writer) void {
    const status = restartStatus();
    w.text("  response_match=channel/op/request stale_skips=");
    w.num(stale_responses_skipped);
    w.text(" restarts=");
    w.num(status.restarts);
    w.text(" last_queue=");
    w.num(status.last_queued);
    w.text(" last_handles=");
    w.num(status.last_handles);
    w.text(" last_tomb=");
    w.num(status.last_tombstones);
}

fn writeCleanupStatusLine(w: *Writer) void {
    const s = net.cleanupStatus();
    w.text("  Cleanup: runs=");
    w.num(s.runs);
    w.text(" link_down=");
    w.num(s.link_down_cleanups);
    w.text(" reset=");
    w.num(s.adapter_reset_cleanups);
    w.text(" unreg=");
    w.num(s.adapter_unregister_cleanups);
    w.text(" svc_restart=");
    w.num(s.service_restart_cleanups);
    w.text(" poweroff=");
    w.num(s.poweroff_cleanups);
    w.text(" reboot=");
    w.num(s.reboot_cleanups);
    w.text(" udp_closed=");
    w.num(s.udp_sockets_closed);
    w.text(" tcp_abort=");
    w.num(s.tcp_connections_aborted);
    w.text(" tcp_listen=");
    w.num(s.tcp_listeners_closed);
    w.text(" dhcp_cancel=");
    w.num(s.dhcp_operations_cancelled);
    w.text(" dns_cancel=");
    w.num(s.dns_operations_cancelled);
    w.text(" last_udp=");
    w.num(s.last_udp_closed);
    w.text(" last_tcp=");
    w.num(s.last_tcp_connections);
    w.text("/");
    w.num(s.last_tcp_listeners);
    w.text(" last=");
    w.text(s.last_reason);
}

fn writeRestartDiagnostics(w: *Writer) void {
    w.text("\r\nIPC network services\r\n");
    writeServiceStatusLine(w);
    w.text("\r\n");
    writeCleanupStatusLine(w);
}

pub fn serviceName(channel_id: u32) []const u8 {
    return switch (channel_id) {
        ipc.CHANNEL_NET_DHCP => "net.dhcp",
        ipc.CHANNEL_NET_DNS => "net.dns",
        ipc.CHANNEL_NET_TCP => "net.tcp",
        ipc.CHANNEL_NET_UDP => "net.udp",
        else => "unknown",
    };
}

const QueueStatus = struct {
    channels: u32 = 0,
    handlers: u32 = 0,
    queued: u32 = 0,
    limit: u32 = 0,
    drops: u64 = 0,
};

fn queueStatus() QueueStatus {
    var status: QueueStatus = .{};
    const channels = [_]u32{
        ipc.CHANNEL_NET_DHCP,
        ipc.CHANNEL_NET_DNS,
        ipc.CHANNEL_NET_TCP,
        ipc.CHANNEL_NET_UDP,
    };
    for (channels) |channel_id| {
        var info: ipc.ChannelInfo = .{};
        _ = ipc.channelInfo(channel_id, &info);
        status.channels += 1;
        status.limit += info.queue_depth;
        status.queued += info.queued;
        status.drops += info.drops;
        if (info.has_handler != 0) status.handlers += 1;
    }
    return status;
}

fn drainServiceQueues() void {
    var scratch: [ipc.MAX_MESSAGE_SIZE]u8 = undefined;
    const channels = [_]u32{
        ipc.CHANNEL_NET_DHCP,
        ipc.CHANNEL_NET_DNS,
        ipc.CHANNEL_NET_TCP,
        ipc.CHANNEL_NET_UDP,
    };
    for (channels) |channel_id| {
        while (ipc.recv(channel_id, scratch[0..]) > 0) {}
    }
}

pub fn statusRequest(channel_id: u32, request_id: u32, out: []u8) i32 {
    return sendRequest(channel_id, OP_STATUS, request_id, "", out);
}

pub fn sendRequest(channel_id: u32, op: u16, request_id: u32, payload: []const u8, out: []u8) i32 {
    return sendRequestAsClient(channel_id, op, request_id, 0, payload, out);
}

pub fn sendRequestAsClient(channel_id: u32, op: u16, request_id: u32, client_id: u16, payload: []const u8, out: []u8) i32 {
    if (payload.len > TCP_MESSAGE_PAYLOAD_MAX) {
        service_backpressure_status.payload_too_large += 1;
        return RESULT_BAD_REQUEST;
    }
    if (out.len < HEADER_SIZE) {
        service_backpressure_status.response_buffer_small += 1;
        return RESULT_BAD_REQUEST;
    }
    var message: [ipc.MAX_MESSAGE_SIZE]u8 = undefined;
    writeHeader(message[0..], channel_id, op, request_id, RESULT_OK, @intCast(payload.len)) orelse return RESULT_BAD_REQUEST;
    writeU16(message[0..], 16, client_id);
    if (payload.len != 0) @memcpy(message[HEADER_SIZE .. HEADER_SIZE + payload.len], payload);
    const got = ipc.request(channel_id, message[0 .. HEADER_SIZE + payload.len], out, ipc.WAIT_FOREVER);
    if (got < 0) {
        if (out.len < ipc.MAX_MESSAGE_SIZE) service_backpressure_status.response_buffer_small += 1;
        service_backpressure_status.send_failures += 1;
        return RESULT_BAD_SERVICE;
    }
    return got;
}

pub fn staleResponseSkips() u64 {
    return stale_responses_skipped;
}

pub fn serviceBackpressureStatus() ServiceBackpressureStatus {
    service_backpressure_status.stale_skips = stale_responses_skipped;
    return service_backpressure_status;
}

pub fn serviceErrorCount() u64 {
    const status = queueStatus();
    const bp = serviceBackpressureStatus();
    const lifecycle = serviceLifecycleStatus();
    return status.drops +
        bp.payload_too_large +
        bp.response_buffer_small +
        bp.send_failures +
        lifecycle.close_failed +
        lifecycle.close_cancelled +
        lifecycle.cleanup_aborts;
}

pub fn logServiceErrorStatus(reason: []const u8) void {
    const status = queueStatus();
    const bp = serviceBackpressureStatus();
    const lifecycle = serviceLifecycleStatus();
    bootlog.puts("[NETSVC][ERR] ");
    bootlog.puts(reason);
    bootlog.puts(" total=");
    bootlog.putDec(serviceErrorCount());
    bootlog.puts(" drops=");
    bootlog.putDec(status.drops);
    bootlog.puts(" req_large=");
    bootlog.putDec(bp.payload_too_large);
    bootlog.puts(" resp_small=");
    bootlog.putDec(bp.response_buffer_small);
    bootlog.puts(" send_fail=");
    bootlog.putDec(bp.send_failures);
    bootlog.puts(" close_fail=");
    bootlog.putDec(lifecycle.close_failed);
    bootlog.puts(" close_cancel=");
    bootlog.putDec(lifecycle.close_cancelled);
    bootlog.puts(" cleanup_abort=");
    bootlog.putDec(lifecycle.cleanup_aborts);
    bootlog.puts("\r\n");
}

pub fn noteCleanupLifecycle(reason: []const u8, cleanup: net.CleanupStatus) void {
    if (cleanup.last_tcp_connections != 0 or cleanup.last_tcp_listeners != 0) {
        service_lifecycle_status.cleanup_aborts += 1;
    }
    if (textEquals(reason, "poweroff")) {
        service_lifecycle_status.cleanup_poweroff += 1;
    } else if (textEquals(reason, "reboot")) {
        service_lifecycle_status.cleanup_reboot += 1;
    }
}

pub fn serviceLifecycleStatus() ServiceLifecycleStatus {
    return service_lifecycle_status;
}

pub fn parsePayload(response: []const u8, status: *i32) ?[]const u8 {
    if (response.len < HEADER_SIZE) return null;
    if (readU32(response, 0) != MAGIC or readU16(response, 4) != VERSION) return null;
    const payload_len = readU16(response, 18);
    if (HEADER_SIZE + payload_len > response.len) return null;
    status.* = readI32(response, 20);
    return response[HEADER_SIZE .. HEADER_SIZE + payload_len];
}

fn handle(channel_id: u32, request: []const u8, response: []u8) i32 {
    if (request.len < HEADER_SIZE) return writeError(response, channel_id, 0, 0, RESULT_BAD_REQUEST);
    if (readU32(request, 0) != MAGIC or readU16(request, 4) != VERSION) return writeError(response, channel_id, 0, 0, RESULT_BAD_REQUEST);
    const service = readU16(request, 6);
    const op = readU16(request, 8);
    const request_id = readU32(request, 12);
    const client_id = readU16(request, 16);
    const payload_len = readU16(request, 18);
    if (HEADER_SIZE + payload_len > request.len) return writeError(response, channel_id, op, request_id, RESULT_BAD_REQUEST);
    const request_payload = request[HEADER_SIZE .. HEADER_SIZE + payload_len];
    if (service != channel_id) return writeError(response, channel_id, op, request_id, RESULT_BAD_SERVICE);

    var response_payload: [ipc.MAX_MESSAGE_SIZE - HEADER_SIZE]u8 = undefined;
    var w = Writer{ .out = response_payload[0..] };
    if (op == OP_SERVICE_RESTART) {
        const status = restartServices("manual");
        writeRestartStatus(&w, status);
        writeRestartDiagnostics(&w);
        return writeResponse(response, channel_id, op, request_id, RESULT_OK, w.slice());
    }
    if (channel_id == ipc.CHANNEL_NET_DHCP and op == OP_DHCP_STATUS_RESULT) {
        const status_result = makeDhcpStatusResult();
        const bytes: [*]const u8 = @ptrCast(&status_result);
        return writeResponse(response, channel_id, op, request_id, RESULT_OK, bytes[0..@sizeOf(DhcpServiceStatus)]);
    }
    if (channel_id == ipc.CHANNEL_NET_DHCP and (op == OP_DHCP_ACQUIRE or op == OP_DHCP_RENEW or op == OP_DHCP_RELEASE)) {
        writeDhcpOperation(&w, op, request_payload);
        return writeResponse(response, channel_id, op, request_id, RESULT_OK, w.slice());
    }
    if (channel_id == ipc.CHANNEL_NET_DHCP and (op == OP_DHCP_ACQUIRE_RESULT or op == OP_DHCP_RENEW_RESULT or op == OP_DHCP_RELEASE_RESULT)) {
        const result = makeDhcpOperationResult(op, request_payload);
        const bytes: [*]const u8 = @ptrCast(&result);
        return writeResponse(response, channel_id, op, request_id, RESULT_OK, bytes[0..@sizeOf(DhcpServiceResult)]);
    }
    if (channel_id == ipc.CHANNEL_NET_DNS and (op == OP_DNS_RESOLVE_A or op == OP_DNS_RESOLVE_A_SERVER)) {
        writeDnsResolve(&w, op, request_payload);
        return writeResponse(response, channel_id, op, request_id, RESULT_OK, w.slice());
    }
    if (channel_id == ipc.CHANNEL_NET_DNS and op == OP_DNS_STATUS_RESULT) {
        const status_result = makeDnsStatusResult();
        const bytes: [*]const u8 = @ptrCast(&status_result);
        return writeResponse(response, channel_id, op, request_id, RESULT_OK, bytes[0..@sizeOf(DnsServiceStatus)]);
    }
    if (channel_id == ipc.CHANNEL_NET_DNS and (op == OP_DNS_RESOLVE_A_RESULT or op == OP_DNS_RESOLVE_A_SERVER_RESULT)) {
        const result = makeDnsResolveResult(op, request_payload);
        const bytes: [*]const u8 = @ptrCast(&result);
        return writeResponse(response, channel_id, op, request_id, RESULT_OK, bytes[0..@sizeOf(DnsServiceResult)]);
    }
    if (channel_id == ipc.CHANNEL_NET_TCP and op == OP_TCP_STATUS_RESULT) {
        const status_result = makeTcpStatusResult(client_id);
        const bytes: [*]const u8 = @ptrCast(&status_result);
        return writeResponse(response, channel_id, op, request_id, RESULT_OK, bytes[0..@sizeOf(TcpServiceStatus)]);
    }
    if (channel_id == ipc.CHANNEL_NET_TCP and isTcpOperation(op)) {
        writeTcpOperation(&w, op, request_payload, client_id);
        return writeResponse(response, channel_id, op, request_id, RESULT_OK, w.slice());
    }
    if (channel_id == ipc.CHANNEL_NET_TCP and isTcpResultOperation(op)) {
        return handleTcpResultOperation(response, channel_id, op, request_id, request_payload, client_id);
    }
    if (channel_id == ipc.CHANNEL_NET_UDP and op == OP_UDP_STATUS_RESULT) {
        const status_result = makeUdpStatusResult();
        const bytes: [*]const u8 = @ptrCast(&status_result);
        return writeResponse(response, channel_id, op, request_id, RESULT_OK, bytes[0..@sizeOf(UdpServiceStatus)]);
    }
    if (channel_id == ipc.CHANNEL_NET_UDP and isUdpOperation(op)) {
        writeUdpOperation(&w, op, request_payload);
        return writeResponse(response, channel_id, op, request_id, RESULT_OK, w.slice());
    }
    if (channel_id == ipc.CHANNEL_NET_UDP and isUdpResultOperation(op)) {
        return handleUdpResultOperation(response, channel_id, op, request_id, request_payload);
    }
    if (op != OP_STATUS) return writeError(response, channel_id, op, request_id, RESULT_BAD_OP);
    switch (channel_id) {
        ipc.CHANNEL_NET_DHCP => writeDhcpStatus(&w),
        ipc.CHANNEL_NET_DNS => writeDnsStatus(&w),
        ipc.CHANNEL_NET_TCP => writeTcpStatus(&w),
        ipc.CHANNEL_NET_UDP => writeUdpStatus(&w),
        else => return writeError(response, channel_id, op, request_id, RESULT_BAD_SERVICE),
    }
    return writeResponse(response, channel_id, op, request_id, RESULT_OK, w.slice());
}

// TCP- und UDP-Ergebnisantworten besitzen jeweils zwei fast 4 KB grosse
// Arbeitsbereiche. Bleiben beide Zweige im gemeinsamen Handler, reserviert
// ReleaseSafe sie zusammen in dessen Frame. Der einzelne service-ipc-Worker
// lief dadurch beim ersten SSH-Accept in den Guard seines 64-KB-Stacks.
// Getrennte, nicht inlinebare Frames halten nur den tatsaechlich ausgefuehrten
// Protokollzweig gleichzeitig aktiv und bewahren die Handler-Reentranz.
noinline fn handleTcpResultOperation(
    response: []u8,
    channel_id: u32,
    op: u16,
    request_id: u32,
    request_payload: []const u8,
    client_id: u16,
) i32 {
    var data_buf: [TCP_RESULT_DATA_MAX]u8 = undefined;
    const tcp_result = makeTcpResult(op, request_payload, data_buf[0..], client_id);
    const result_bytes: [*]const u8 = @ptrCast(&tcp_result.result);
    var payload_buf: [@sizeOf(TcpServiceResult) + TCP_RESULT_DATA_MAX]u8 = undefined;
    @memcpy(payload_buf[0..@sizeOf(TcpServiceResult)], result_bytes[0..@sizeOf(TcpServiceResult)]);
    if (tcp_result.data.len != 0) {
        @memcpy(payload_buf[@sizeOf(TcpServiceResult) .. @sizeOf(TcpServiceResult) + tcp_result.data.len], tcp_result.data);
    }
    return writeResponse(response, channel_id, op, request_id, RESULT_OK, payload_buf[0 .. @sizeOf(TcpServiceResult) + tcp_result.data.len]);
}

noinline fn handleUdpResultOperation(
    response: []u8,
    channel_id: u32,
    op: u16,
    request_id: u32,
    request_payload: []const u8,
) i32 {
    var data_buf: [UDP_RESULT_DATA_MAX]u8 = undefined;
    const udp_result = makeUdpResult(op, request_payload, data_buf[0..]);
    const result_bytes: [*]const u8 = @ptrCast(&udp_result.result);
    var payload_buf: [@sizeOf(UdpServiceResult) + UDP_RESULT_DATA_MAX]u8 = undefined;
    @memcpy(payload_buf[0..@sizeOf(UdpServiceResult)], result_bytes[0..@sizeOf(UdpServiceResult)]);
    if (udp_result.data.len != 0) {
        @memcpy(payload_buf[@sizeOf(UdpServiceResult) .. @sizeOf(UdpServiceResult) + udp_result.data.len], udp_result.data);
    }
    return writeResponse(response, channel_id, op, request_id, RESULT_OK, payload_buf[0 .. @sizeOf(UdpServiceResult) + udp_result.data.len]);
}

fn writeDhcpStatus(w: *Writer) void {
    const s = net.dhcpStats();
    const timing = net.dhcpLeaseTiming();
    w.text("state=");
    w.text(if (s.lease.bound) "bound" else "released");
    w.text(" pending=");
    w.text(if (s.operation_pending) "yes" else "no");
    w.text(" op=");
    w.text(s.pending_label);
    w.text(" ip=");
    w.ip(s.lease.offered_ip);
    w.text(" server=");
    w.ip(s.lease.server_ip);
    w.text(" lease=");
    w.num(s.lease.lease_seconds);
    w.text(" elapsed=");
    w.num(timing.elapsed_seconds);
    w.text(" remaining=");
    w.num(timing.remaining_seconds);
    w.text(" renew_in=");
    w.num(timing.renew_in_seconds);
    w.text(" rebind_in=");
    w.num(timing.rebind_in_seconds);
    w.text(" discover=");
    w.num(s.discover_tx);
    w.text(" offer=");
    w.num(s.offer_rx);
    w.text(" request=");
    w.num(s.request_tx);
    w.text(" ack=");
    w.num(s.ack_rx);
    w.text(" nak=");
    w.num(s.nak_rx);
    w.text(" release=");
    w.num(s.release_tx);
    w.text(" retry=");
    w.num(s.retries);
    w.text(" timeout=");
    w.num(s.timeouts);
    w.text(" relerr=");
    w.num(s.release_errors);
    w.text(" malformed=");
    w.num(s.malformed);
    w.text(" inactive=");
    w.num(s.inactive_rx);
    w.text(" foreign=");
    w.num(s.foreign_rx);
    w.text(" phase=");
    w.num(s.out_of_phase_rx);
    w.text(" attempt=");
    w.num(s.last_attempt);
    w.text(" last=");
    w.text(s.last_error);
}

fn makeDhcpStatusResult() DhcpServiceStatus {
    const s = net.dhcpStats();
    const timing = net.dhcpLeaseTiming();
    const runtime = net.dhcpRuntimeStatus();
    var flags: u32 = 0;
    if (s.lease.bound) flags |= DHCP_FLAG_BOUND;
    if (s.operation_pending) flags |= DHCP_FLAG_PENDING;
    if (s.lease.dns_configured) flags |= DHCP_FLAG_DNS_CONFIGURED;
    if (runtime.desired_dhcp) flags |= DHCP_FLAG_DESIRED;
    if (runtime.task_started) flags |= DHCP_FLAG_TASK_STARTED;
    if (runtime.link_up) flags |= DHCP_FLAG_LINK_UP;
    if (runtime.state == .retry_wait) flags |= DHCP_FLAG_RETRY_WAIT;
    var out = DhcpServiceStatus{
        .runtime_state = @intFromEnum(runtime.state),
        .flags = flags,
        .xid = s.lease.xid,
        .offered_ip = s.lease.offered_ip,
        .server_ip = s.lease.server_ip,
        .netmask = s.lease.netmask,
        .gateway_ip = s.lease.gateway_ip,
        .dns_ip = s.lease.dns_ip,
        .lease_seconds = s.lease.lease_seconds,
        .renew_seconds = s.lease.renew_seconds,
        .rebind_seconds = s.lease.rebind_seconds,
        .elapsed_seconds = timing.elapsed_seconds,
        .remaining_seconds = timing.remaining_seconds,
        .renew_in_seconds = timing.renew_in_seconds,
        .rebind_in_seconds = timing.rebind_in_seconds,
        .last_attempt = s.last_attempt,
        .last_type = s.last_type,
        .discover_tx = s.discover_tx,
        .offer_rx = s.offer_rx,
        .request_tx = s.request_tx,
        .ack_rx = s.ack_rx,
        .nak_rx = s.nak_rx,
        .release_tx = s.release_tx,
        .retries = s.retries,
        .timeouts = s.timeouts,
        .release_errors = s.release_errors,
        .malformed = s.malformed,
        .self_tests = s.self_tests,
    };
    copyBounded(out.pending_label[0..], s.pending_label);
    copyBounded(out.last_error[0..], s.last_error);
    return out;
}

fn writeDhcpOperation(w: *Writer, op: u16, payload: []const u8) void {
    const request = splitServiceDeadline(payload);
    const result = switch (op) {
        OP_DHCP_ACQUIRE => net.dhcpAcquireDefaultUntil(request.deadline_tick),
        OP_DHCP_RENEW => net.dhcpRenewDefaultUntil(request.deadline_tick),
        OP_DHCP_RELEASE => net.dhcpReleaseDefault(),
        else => unreachable,
    };
    const s = net.dhcpStats();
    const timing = net.dhcpLeaseTiming();
    w.text("action=");
    w.text(switch (op) {
        OP_DHCP_ACQUIRE => "acquire",
        OP_DHCP_RENEW => "renew",
        OP_DHCP_RELEASE => "release",
        else => "unknown",
    });
    w.text(" result=");
    w.text(net.txResultName(result));
    w.text(" code=");
    w.signed(netTxResultCode(result));
    w.text(" state=");
    w.text(if (s.lease.bound) "bound" else "released");
    w.text(" pending=");
    w.text(if (s.operation_pending) "yes" else "no");
    w.text(" ip=");
    w.ip(s.lease.offered_ip);
    w.text(" server=");
    w.ip(s.lease.server_ip);
    w.text(" remaining=");
    w.num(timing.remaining_seconds);
    w.text(" renew_in=");
    w.num(timing.renew_in_seconds);
    w.text(" rebind_in=");
    w.num(timing.rebind_in_seconds);
    w.text(" timeout=");
    w.num(s.timeouts);
    w.text(" retry=");
    w.num(s.retries);
    w.text(" relerr=");
    w.num(s.release_errors);
    w.text(" last=");
    w.text(s.last_error);
}

fn makeDhcpOperationResult(op: u16, payload: []const u8) DhcpServiceResult {
    const request = splitServiceDeadline(payload);
    const tx = switch (op) {
        OP_DHCP_ACQUIRE_RESULT => net.dhcpAcquireDefaultUntil(request.deadline_tick),
        OP_DHCP_RENEW_RESULT => net.dhcpRenewDefaultUntil(request.deadline_tick),
        OP_DHCP_RELEASE_RESULT => net.dhcpReleaseDefault(),
        else => unreachable,
    };
    const s = net.dhcpStats();
    const timing = net.dhcpLeaseTiming();
    const runtime = net.dhcpRuntimeStatus();
    var flags: u32 = 0;
    if (s.lease.bound) flags |= DHCP_FLAG_BOUND;
    if (s.operation_pending) flags |= DHCP_FLAG_PENDING;
    if (s.lease.dns_configured) flags |= DHCP_FLAG_DNS_CONFIGURED;
    if (runtime.desired_dhcp) flags |= DHCP_FLAG_DESIRED;
    if (runtime.task_started) flags |= DHCP_FLAG_TASK_STARTED;
    if (runtime.link_up) flags |= DHCP_FLAG_LINK_UP;
    if (runtime.state == .retry_wait) flags |= DHCP_FLAG_RETRY_WAIT;
    flags = withServiceStatus(flags, serviceStatusFromTxResult(tx, s.last_error));
    return .{
        .action = switch (op) {
            OP_DHCP_ACQUIRE_RESULT => DHCP_ACTION_ACQUIRE,
            OP_DHCP_RENEW_RESULT => DHCP_ACTION_RENEW,
            OP_DHCP_RELEASE_RESULT => DHCP_ACTION_RELEASE,
            else => 0,
        },
        .result = netTxResultCode(tx),
        .flags = flags,
        .offered_ip = s.lease.offered_ip,
        .server_ip = s.lease.server_ip,
        .netmask = s.lease.netmask,
        .gateway_ip = s.lease.gateway_ip,
        .dns_ip = s.lease.dns_ip,
        .lease_seconds = s.lease.lease_seconds,
        .elapsed_seconds = timing.elapsed_seconds,
        .remaining_seconds = timing.remaining_seconds,
        .renew_in_seconds = timing.renew_in_seconds,
        .rebind_in_seconds = timing.rebind_in_seconds,
        .last_attempt = s.last_attempt,
        .last_type = s.last_type,
        .discover_tx = s.discover_tx,
        .offer_rx = s.offer_rx,
        .request_tx = s.request_tx,
        .ack_rx = s.ack_rx,
        .nak_rx = s.nak_rx,
        .release_tx = s.release_tx,
        .retries = s.retries,
        .timeouts = s.timeouts,
        .release_errors = s.release_errors,
        .malformed = s.malformed,
    };
}

fn writeDnsStatus(w: *Writer) void {
    const s = net.dnsStats();
    const cache = net.dnsCacheStatus();
    w.text("pending=");
    w.text(if (s.operation_pending) "yes" else "no");
    w.text(" name=");
    w.text(if (s.operation_pending) s.pending_name[0..] else s.last_name[0..]);
    w.text(" queries=");
    w.num(s.queries_tx);
    w.text(" resolve=");
    w.num(s.resolve_requests);
    w.text(" responses=");
    w.num(s.responses_rx);
    w.text(" a=");
    w.num(s.a_records);
    w.text(" timeout=");
    w.num(s.timeouts);
    w.text(" nxdomain=");
    w.num(s.nxdomain);
    w.text(" txerr=");
    w.num(s.tx_errors);
    w.text(" malformed=");
    w.num(s.malformed);
    w.text(" server=");
    w.ip(s.last_server);
    w.text(" id=");
    w.num(s.last_id);
    w.text(" answer=");
    w.ip(s.last_answer);
    w.text(" result=");
    w.signed(s.last_result);
    w.text(" cache=");
    w.text(if (cache.valid) "yes" else "no");
    w.text(" cache_name=");
    w.text(s.cache_name[0..]);
    w.text(" cache_answer=");
    w.ip(s.cache_answer);
    w.text(" cache_hits=");
    w.num(s.cache_hits);
    w.text(" cache_stores=");
    w.num(s.cache_stores);
    w.text(" cache_age=");
    w.num(cache.age_seconds);
    w.text("/");
    w.num(cache.ttl_seconds);
    w.text(" cache_remaining=");
    w.num(cache.remaining_seconds);
    w.text(" cache_entries=");
    w.num(net.dnsCacheEntryCount());
    w.text(" cache_negative=");
    w.num(net.dnsCacheNegativeCount());
    w.text(" last=");
    w.text(s.last_error);
}

fn makeDnsStatusResult() DnsServiceStatus {
    const s = net.dnsStats();
    const cache = net.dnsCacheStatus();
    var flags: u32 = 0;
    if (s.last_result == 0 and (s.responses_rx != 0 or s.a_records != 0)) flags |= DNS_FLAG_OK;
    if (s.operation_pending) flags |= DNS_FLAG_PENDING;
    if (cache.valid) flags |= DNS_FLAG_CACHE_VALID;
    var out = DnsServiceStatus{
        .flags = flags,
        .last_result = s.last_result,
        .last_id = s.last_id,
        .last_server = s.last_server,
        .last_answer = s.last_answer,
        .cache_server = s.cache_server,
        .cache_answer = s.cache_answer,
        .cache_age_seconds = cache.age_seconds,
        .cache_ttl_seconds = cache.ttl_seconds,
        .cache_remaining_seconds = cache.remaining_seconds,
        .queries_tx = s.queries_tx,
        .resolve_requests = s.resolve_requests,
        .responses_rx = s.responses_rx,
        .a_records = s.a_records,
        .timeouts = s.timeouts,
        .nxdomain = s.nxdomain,
        .tx_errors = s.tx_errors,
        .malformed = s.malformed,
        .self_tests = s.self_tests,
        .cache_hits = s.cache_hits,
        .cache_stores = s.cache_stores,
    };
    copyBounded(out.name[0..], s.last_name[0..]);
    out.name_len = @intCast(stringLenZ(out.name[0..]));
    copyBounded(out.pending_name[0..], s.pending_name[0..]);
    out.pending_name_len = @intCast(stringLenZ(out.pending_name[0..]));
    copyBounded(out.cache_name[0..], s.cache_name[0..]);
    out.cache_name_len = @intCast(stringLenZ(out.cache_name[0..]));
    copyBounded(out.last_error[0..], s.last_error);
    return out;
}

fn writeDnsResolve(w: *Writer, op: u16, payload: []const u8) void {
    var server: [4]u8 = .{0} ** 4;
    var name = payload;
    const explicit_server = op == OP_DNS_RESOLVE_A_SERVER;
    if (explicit_server) {
        if (payload.len < 5) {
            w.text("name= result=name code=-9");
            w.text(" ip=0.0.0.0 server=0.0.0.0 pending=no cache=no cache_hits=0 cache_stores=0 timeout=0 last=bad-request");
            return;
        }
        server = .{ payload[0], payload[1], payload[2], payload[3] };
        name = payload[4..];
    }
    var answer: [4]u8 = .{0} ** 4;
    const result = if (explicit_server) net.dnsResolveAWithServer(name, server, &answer) else net.dnsResolveA(name, &answer);
    w.text("name=");
    w.text(name);
    w.text(" result=");
    w.text(net.dnsResultName(result));
    w.text(" code=");
    w.signed(result);
    w.text(" ip=");
    w.ip(answer);
    const s = net.dnsStats();
    const cache = net.dnsCacheStatus();
    w.text(" server=");
    w.ip(if (explicit_server) server else s.last_server);
    w.text(" pending=");
    w.text(if (s.operation_pending) "yes" else "no");
    w.text(" cache=");
    w.text(if (cache.valid) "yes" else "no");
    w.text(" cache_hits=");
    w.num(s.cache_hits);
    w.text(" cache_stores=");
    w.num(s.cache_stores);
    w.text(" timeout=");
    w.num(s.timeouts);
    w.text(" cache_entries=");
    w.num(net.dnsCacheEntryCount());
    w.text(" cache_negative=");
    w.num(net.dnsCacheNegativeCount());
    w.text(" last=");
    w.text(s.last_error);
}

fn makeDnsResolveResult(op: u16, payload: []const u8) DnsServiceResult {
    var server: [4]u8 = .{0} ** 4;
    var name = payload;
    const explicit_server = op == OP_DNS_RESOLVE_A_SERVER_RESULT;
    if (explicit_server) {
        if (payload.len < 5) return dnsBadRequestResult(op, "");
        server = .{ payload[0], payload[1], payload[2], payload[3] };
        name = payload[4..];
    }

    var answer: [4]u8 = .{0} ** 4;
    const before = net.dnsStats();
    const result = if (explicit_server) net.dnsResolveAWithServer(name, server, &answer) else net.dnsResolveA(name, &answer);
    const after = net.dnsStats();
    const cache = net.dnsCacheStatus();

    var flags: u32 = 0;
    if (result == 0) flags |= DNS_FLAG_OK;
    if (after.operation_pending) flags |= DNS_FLAG_PENDING;
    if (cache.valid) flags |= DNS_FLAG_CACHE_VALID;
    if (after.cache_hits > before.cache_hits) flags |= DNS_FLAG_CACHE_HIT;
    if (explicit_server) flags |= DNS_FLAG_EXPLICIT_SERVER;
    flags = withServiceStatus(flags, serviceStatusFromDnsResult(result));

    var out = DnsServiceResult{
        .action = if (explicit_server) DNS_ACTION_RESOLVE_A_SERVER else DNS_ACTION_RESOLVE_A,
        .result = result,
        .flags = flags,
        .answer = answer,
        .server = if (explicit_server) server else after.last_server,
        .cache_answer = after.cache_answer,
        .cache_age_seconds = cache.age_seconds,
        .cache_ttl_seconds = cache.ttl_seconds,
        .cache_remaining_seconds = cache.remaining_seconds,
        .queries_tx = after.queries_tx,
        .resolve_requests = after.resolve_requests,
        .responses_rx = after.responses_rx,
        .a_records = after.a_records,
        .timeouts = after.timeouts,
        .nxdomain = after.nxdomain,
        .tx_errors = after.tx_errors,
        .malformed = after.malformed,
        .cache_hits = after.cache_hits,
        .cache_stores = after.cache_stores,
        .last_id = after.last_id,
    };
    copyBounded(out.name[0..], name);
    out.name_len = @intCast(@min(name.len, out.name.len));
    copyBounded(out.last_error[0..], after.last_error);
    return out;
}

fn dnsBadRequestResult(op: u16, name: []const u8) DnsServiceResult {
    var out = DnsServiceResult{
        .action = if (op == OP_DNS_RESOLVE_A_SERVER_RESULT) DNS_ACTION_RESOLVE_A_SERVER else DNS_ACTION_RESOLVE_A,
        .result = -9,
        .flags = withServiceStatus(if (op == OP_DNS_RESOLVE_A_SERVER_RESULT) DNS_FLAG_EXPLICIT_SERVER else 0, .failed),
    };
    copyBounded(out.name[0..], name);
    out.name_len = @intCast(@min(name.len, out.name.len));
    copyBounded(out.last_error[0..], "bad-request");
    return out;
}

fn writeUdpStatus(w: *Writer) void {
    var s: net.UdpStatus = .{};
    net.udpStatus(&s);
    w.text("active=");
    w.num(s.active_sockets);
    w.text("/");
    w.num(s.max_sockets);
    w.text(" queue=");
    w.num(s.queued_packets);
    w.text("/");
    w.num(s.queue_limit);
    w.text(" payload_max=");
    w.num(s.payload_max);
    w.text(" message_payload=");
    w.num(TCP_MESSAGE_PAYLOAD_MAX);
    w.text(" send_max=");
    w.num(UDP_SEND_PAYLOAD_MAX);
    w.text(" recv_max=");
    w.num(UDP_RESULT_DATA_MAX);
    w.text(" delivered=");
    w.num(s.delivered);
    w.text(" drops=");
    w.num(s.drops);
    w.text(" lifecycle_last=");
    w.text(socketLifecycleName(udp_lifecycle.last_cause));
    w.text(" would_block=");
    w.num(udp_lifecycle.would_block);
    w.text(" last=");
    w.text(s.last_error);
}

fn makeUdpStatusResult() UdpServiceStatus {
    var s: net.UdpStatus = .{};
    net.udpStatus(&s);
    var out = UdpServiceStatus{
        .active_sockets = s.active_sockets,
        .max_sockets = s.max_sockets,
        .queued_packets = s.queued_packets,
        .queue_limit = s.queue_limit,
        .payload_max = s.payload_max,
        .message_payload_max = TCP_MESSAGE_PAYLOAD_MAX,
        .send_max = UDP_SEND_PAYLOAD_MAX,
        .recv_max = UDP_RESULT_DATA_MAX,
        .delivered = s.delivered,
        .drops = s.drops,
        .last_lifecycle_cause = udp_lifecycle.last_cause,
        .lifecycle_closed = udp_lifecycle.closed,
        .lifecycle_timeout = udp_lifecycle.timeout,
        .lifecycle_local_close = udp_lifecycle.local_close,
        .lifecycle_would_block = udp_lifecycle.would_block,
        .lifecycle_bad_handle = udp_lifecycle.bad_handle,
        .lifecycle_dropped = udp_lifecycle.dropped,
    };
    if (udp_lifecycle.last_cause != SOCKET_LIFECYCLE_UNKNOWN_PUBLIC) out.flags |= UDP_STATUS_FLAG_LIFECYCLE_VALID;
    copyBounded(out.last_error[0..], s.last_error);
    return out;
}

fn writeUdpOperation(w: *Writer, op: u16, payload: []const u8) void {
    switch (op) {
        OP_UDP_BIND => writeUdpBind(w, payload),
        OP_UDP_SENDTO => writeUdpSendTo(w, payload),
        OP_UDP_RECV => writeUdpRecv(w, payload),
        OP_UDP_CLOSE => writeUdpClose(w, payload),
        else => unreachable,
    }
}

fn isUdpOperation(op: u16) bool {
    return op == OP_UDP_BIND or
        op == OP_UDP_SENDTO or
        op == OP_UDP_RECV or
        op == OP_UDP_CLOSE;
}

fn isUdpResultOperation(op: u16) bool {
    return op == OP_UDP_BIND_RESULT or
        op == OP_UDP_SENDTO_RESULT or
        op == OP_UDP_RECV_RESULT or
        op == OP_UDP_CLOSE_RESULT;
}

const UdpResultPayload = struct {
    result: UdpServiceResult,
    data: []const u8,
};

fn makeUdpResult(op: u16, payload: []const u8, data_out: []u8) UdpResultPayload {
    var out = UdpServiceResult{ .action = udpResultAction(op) };
    const data = switch (op) {
        OP_UDP_BIND_RESULT => udpBindResult(&out, payload),
        OP_UDP_SENDTO_RESULT => udpSendToResult(&out, payload),
        OP_UDP_RECV_RESULT => udpRecvResult(&out, payload, data_out),
        OP_UDP_CLOSE_RESULT => udpCloseResult(&out, payload),
        else => "",
    };
    fillUdpResultStatus(&out);
    setUdpOperationStatus(&out);
    return .{ .result = out, .data = data };
}

fn udpResultAction(op: u16) u16 {
    return switch (op) {
        OP_UDP_BIND_RESULT => UDP_ACTION_BIND,
        OP_UDP_SENDTO_RESULT => UDP_ACTION_SENDTO,
        OP_UDP_RECV_RESULT => UDP_ACTION_RECV,
        OP_UDP_CLOSE_RESULT => UDP_ACTION_CLOSE,
        else => 0,
    };
}

fn udpBindResult(out: *UdpServiceResult, payload: []const u8) []const u8 {
    if (payload.len < 2) return udpBadRequestResult(out);
    out.dest_port = readU16(payload, 0);
    if (out.dest_port == 0) return udpBadRequestResult(out);
    const svc_handle = net.udpBind(out.dest_port);
    out.result = svc_handle;
    if (svc_handle > 0) {
        out.result = 0;
        out.handle = @intCast(svc_handle);
        out.flags |= UDP_FLAG_OK | UDP_FLAG_HANDLE_VALID;
        setUdpLifecycle(out, SOCKET_LIFECYCLE_ACTIVE_PUBLIC);
    }
    return "";
}

fn udpSendToResult(out: *UdpServiceResult, payload: []const u8) []const u8 {
    if (payload.len < 10) return udpBadRequestResult(out);
    out.handle = readU32(payload, 0);
    out.dest_ip = .{ payload[4], payload[5], payload[6], payload[7] };
    out.dest_port = readU16(payload, 8);
    out.requested_bytes = @intCast(payload.len - 10);
    out.flags |= UDP_FLAG_HANDLE_VALID | UDP_FLAG_REMOTE_VALID;
    const tx = net.udpSendTo(out.handle, out.dest_ip, out.dest_port, payload[10..]);
    out.result = netTxResultCode(tx);
    if (tx == .ok) {
        out.flags |= UDP_FLAG_OK;
        out.bytes = out.requested_bytes;
        setUdpLifecycle(out, SOCKET_LIFECYCLE_ACTIVE_PUBLIC);
    } else if (tx == .busy) {
        setUdpServiceStatus(out, .would_block);
        setUdpLifecycle(out, SOCKET_LIFECYCLE_WOULD_BLOCK_PUBLIC);
    } else {
        setUdpLifecycle(out, SOCKET_LIFECYCLE_BAD_HANDLE_PUBLIC);
    }
    return "";
}

fn udpRecvResult(out: *UdpServiceResult, payload: []const u8, data_out: []u8) []const u8 {
    if (payload.len < 6) return udpBadRequestResult(out);
    out.handle = readU32(payload, 0);
    const max_len = @min(@as(usize, readU16(payload, 4)), data_out.len);
    if (max_len == 0) return udpBadRequestResult(out);
    out.requested_bytes = @intCast(max_len);
    out.flags |= UDP_FLAG_HANDLE_VALID;
    var info: net.UdpRecvInfo = .{};
    const got = net.udpRecvFrom(out.handle, &info, data_out[0..max_len]);
    out.result = got;
    if (got >= 0) {
        out.result = 0;
        out.bytes = @intCast(got);
        out.source_ip = info.source_ip;
        out.dest_ip = info.dest_ip;
        out.source_port = info.source_port;
        out.dest_port = info.dest_port;
        out.flags |= UDP_FLAG_OK;
        if (got > 0) {
            out.flags |= UDP_FLAG_DATA | UDP_FLAG_REMOTE_VALID;
            setUdpLifecycle(out, SOCKET_LIFECYCLE_ACTIVE_PUBLIC);
            return data_out[0..@as(usize, @intCast(got))];
        }
        setUdpServiceStatus(out, .would_block);
        setUdpLifecycle(out, SOCKET_LIFECYCLE_WOULD_BLOCK_PUBLIC);
    }
    return "";
}

fn udpCloseResult(out: *UdpServiceResult, payload: []const u8) []const u8 {
    if (payload.len < 4) return udpBadRequestResult(out);
    out.handle = readU32(payload, 0);
    out.flags |= UDP_FLAG_HANDLE_VALID;
    const result = net.udpClose(out.handle);
    out.result = result;
    if (result == 0) {
        out.flags |= UDP_FLAG_OK;
        setUdpLifecycle(out, SOCKET_LIFECYCLE_LOCAL_CLOSE_PUBLIC);
        recordServiceClose(.ok);
    } else {
        setUdpLifecycle(out, SOCKET_LIFECYCLE_BAD_HANDLE_PUBLIC);
        recordServiceClose(.failed);
    }
    return "";
}

fn udpBadRequestResult(out: *UdpServiceResult) []const u8 {
    out.result = -1;
    setUdpServiceStatus(out, .failed);
    copyBounded(out.last_error[0..], "bad-request");
    return "";
}

fn fillUdpResultStatus(out: *UdpServiceResult) void {
    var s: net.UdpStatus = .{};
    net.udpStatus(&s);
    out.active_sockets = s.active_sockets;
    out.max_sockets = s.max_sockets;
    out.queued_packets = s.queued_packets;
    out.queue_limit = s.queue_limit;
    out.payload_max = s.payload_max;
    out.message_payload_max = TCP_MESSAGE_PAYLOAD_MAX;
    out.send_max = UDP_SEND_PAYLOAD_MAX;
    out.recv_max = UDP_RESULT_DATA_MAX;
    out.delivered = s.delivered;
    out.drops = s.drops;
    if (out.last_error[0] == 0) copyBounded(out.last_error[0..], s.last_error);
}

fn setUdpOperationStatus(out: *UdpServiceResult) void {
    if (serviceStatusEncoded(out.flags)) {
        out.service_status = serviceOperationStatusCode(out.flags);
        return;
    }
    const last_error = fixedText(out.last_error[0..]);
    if (out.result == 0) {
        if (textEquals(last_error, net_timing.operationStatusName(.would_block))) {
            setUdpServiceStatus(out, .would_block);
            setUdpLifecycle(out, SOCKET_LIFECYCLE_WOULD_BLOCK_PUBLIC);
        } else {
            setUdpServiceStatus(out, .ok);
        }
        return;
    }
    if (textContains(last_error, "timeout")) {
        setUdpServiceStatus(out, .timeout);
        setUdpLifecycle(out, SOCKET_LIFECYCLE_TIMEOUT_PUBLIC);
    } else {
        setUdpServiceStatus(out, .failed);
        if (out.lifecycle_cause == SOCKET_LIFECYCLE_UNKNOWN_PUBLIC) setUdpLifecycle(out, SOCKET_LIFECYCLE_BAD_HANDLE_PUBLIC);
    }
}

fn writeUdpBind(w: *Writer, payload: []const u8) void {
    if (payload.len < 2) {
        w.text("op=bind result=bad-request code=-1");
        return;
    }
    const port = readU16(payload, 0);
    const svc_handle = net.udpBind(port);
    w.text("op=bind result=");
    if (svc_handle > 0) {
        w.text("ok code=0 handle=");
        w.num(@as(u32, @intCast(svc_handle)));
    } else {
        w.text("failed code=");
        w.signed(svc_handle);
        w.text(" handle=0");
    }
    w.text(" port=");
    w.num(port);
    w.text(" last=");
    w.text(udpLastError());
}

fn writeUdpSendTo(w: *Writer, payload: []const u8) void {
    if (payload.len < 10) {
        w.text("op=sendto result=bad-request code=-1");
        return;
    }
    const svc_handle = readU32(payload, 0);
    const dest_ip: [4]u8 = .{ payload[4], payload[5], payload[6], payload[7] };
    const dest_port = readU16(payload, 8);
    const data = payload[10..];
    const tx = net.udpSendTo(svc_handle, dest_ip, dest_port, data);
    w.text("op=sendto result=");
    w.text(net.txResultName(tx));
    w.text(" code=");
    w.signed(netTxResultCode(tx));
    w.text(" handle=");
    w.num(svc_handle);
    w.text(" remote=");
    w.ip(dest_ip);
    w.text(":");
    w.num(dest_port);
    w.text(" bytes=");
    w.num(data.len);
    w.text(" last=");
    w.text(udpLastError());
}

fn writeUdpRecv(w: *Writer, payload: []const u8) void {
    if (payload.len < 6) {
        w.text("op=recv result=bad-request code=-1");
        return;
    }
    const svc_handle = readU32(payload, 0);
    const max_len = @min(@as(usize, readU16(payload, 4)), UDP_RESULT_DATA_MAX);
    if (max_len == 0) {
        w.text("op=recv result=bad-request code=-1");
        return;
    }
    var buf: [UDP_RESULT_DATA_MAX]u8 = undefined;
    var info: net.UdpRecvInfo = .{};
    const got = net.udpRecvFrom(svc_handle, &info, buf[0..max_len]);
    w.text("op=recv result=");
    if (got >= 0) {
        w.text("ok code=0 bytes=");
        w.num(@as(u32, @intCast(got)));
    } else {
        w.text("failed code=");
        w.signed(got);
        w.text(" bytes=0");
    }
    w.text(" handle=");
    w.num(svc_handle);
    if (got > 0) {
        w.text(" source=");
        w.ip(info.source_ip);
        w.text(":");
        w.num(info.source_port);
        w.text(" dest=");
        w.ip(info.dest_ip);
        w.text(":");
        w.num(info.dest_port);
        w.text(" data=");
        w.text(buf[0..@as(usize, @intCast(got))]);
    }
    w.text(" last=");
    w.text(udpLastError());
}

fn writeUdpClose(w: *Writer, payload: []const u8) void {
    if (payload.len < 4) {
        w.text("op=close result=bad-request code=-1");
        recordServiceClose(.failed);
        return;
    }
    const svc_handle = readU32(payload, 0);
    const result = net.udpClose(svc_handle);
    const status: net_timing.OperationStatus = if (result == 0) .ok else .failed;
    recordServiceClose(status);
    w.text("op=close result=");
    w.text(if (result == 0) "ok" else "failed");
    w.text(" code=");
    w.signed(result);
    w.text(" handle=");
    w.num(svc_handle);
    w.text(" last=");
    w.text(udpLastError());
    w.text(" status=");
    w.text(net_timing.operationStatusName(status));
}

fn udpLastError() []const u8 {
    var s: net.UdpStatus = .{};
    net.udpStatus(&s);
    return s.last_error;
}

fn writeTcpStatus(w: *Writer) void {
    reapTcpStaleHandles();
    var summary: net.TcpSummary = .{};
    net.tcpSummary(&summary);
    const stats = net.tcpStats();
    w.text("active=");
    w.num(summary.active_connections);
    w.text("/");
    w.num(summary.max_connections);
    w.text(" handles=");
    w.num(tcpHandleCount());
    w.text("/");
    w.num(TCP_SERVICE_HANDLES);
    w.text(" listen=");
    w.num(summary.active_listeners);
    w.text(" tcp_buffer=");
    w.num(summary.buffer_size);
    w.text(" message_payload=");
    w.num(TCP_MESSAGE_PAYLOAD_MAX);
    w.text(" write_max=");
    w.num(TCP_WRITE_PAYLOAD_MAX);
    w.text(" tx=");
    w.num(summary.data_tx);
    w.text(" rx=");
    w.num(summary.data_rx);
    w.text(" retrans=");
    w.num(summary.retransmits);
    w.text(" drops=");
    w.num(summary.rx_drops);
    w.text(" stale_handles=");
    w.num(tcp_stale_handles_reaped);
    w.text(" tombstones=");
    w.num(tcpTombstoneCountRaw());
    w.text(" lifecycle_last=");
    w.text(socketLifecycleName(tcp_lifecycle.last_cause));
    w.text(" read_timeout=");
    w.num(tcp_read_wait_timeouts);
    w.text(" accept_timeout=");
    w.num(tcp_accept_wait_timeouts);
    w.text(" last=");
    w.text(stats.last_error);
}

fn makeTcpStatusResult(client_id: u16) TcpServiceStatus {
    reapTcpStaleHandles();
    var summary: net.TcpSummary = .{};
    net.tcpSummary(&summary);
    const stats = net.tcpStats();
    var out = TcpServiceStatus{
        .request_owner_id = client_id,
        .owned_handles = tcpOwnedHandleCount(client_id),
        .legacy_handles = tcpLegacyHandleCount(),
        .active_connections = summary.active_connections,
        .max_connections = summary.max_connections,
        .active_listeners = summary.active_listeners,
        .handle_count = tcpHandleCount(),
        .max_handles = TCP_SERVICE_HANDLES,
        .tcp_buffer_size = summary.buffer_size,
        .message_payload_max = TCP_MESSAGE_PAYLOAD_MAX,
        .write_max = TCP_WRITE_PAYLOAD_MAX,
        .read_max = @intCast(TCP_RESULT_DATA_MAX),
        .rx_segments = stats.rx_segments,
        .tx_segments = stats.tx_segments,
        .syn_tx = summary.syn_tx,
        .synack_rx = summary.synack_rx,
        .ack_tx = summary.ack_tx,
        .data_tx = summary.data_tx,
        .data_rx = summary.data_rx,
        .fin_tx = summary.fin_tx,
        .rst_rx = summary.rst_rx,
        .checksum_errors = summary.checksum_errors,
        .timeouts = summary.timeouts,
        .self_tests = summary.self_tests,
        .synack_tx = summary.synack_tx,
        .listen_syn_rx = summary.listen_syn_rx,
        .accepts = summary.accepts,
        .retransmits = summary.retransmits,
        .rx_drops = summary.rx_drops,
        .last_source_port = stats.last_source_port,
        .last_dest_port = stats.last_dest_port,
        .last_flags = stats.last_flags,
        .last_seq = stats.last_seq,
        .last_ack = stats.last_ack,
        .last_payload_len = stats.last_payload_len,
        .owner_mismatches = tcp_owner_mismatches,
        .stale_handles_reaped = tcp_stale_handles_reaped,
        .stale_tombstones = tcpTombstoneCountRaw(),
        .last_lifecycle_cause = tcp_lifecycle.last_cause,
        .lifecycle_closed = tcp_lifecycle.closed,
        .lifecycle_reset = tcp_lifecycle.reset,
        .lifecycle_timeout = tcp_lifecycle.timeout,
        .lifecycle_peer_gone = tcp_lifecycle.peer_gone,
        .lifecycle_local_abort = tcp_lifecycle.local_abort,
        .lifecycle_local_close = tcp_lifecycle.local_close,
        .lifecycle_pending_close = tcp_lifecycle.pending_close,
        .lifecycle_would_block = tcp_lifecycle.would_block,
        .lifecycle_bad_handle = tcp_lifecycle.bad_handle,
        .lifecycle_owner_mismatch = tcp_lifecycle.owner_mismatch,
        .read_wait_timeouts = tcp_read_wait_timeouts,
        .accept_wait_timeouts = tcp_accept_wait_timeouts,
        .write_would_block = tcp_write_would_block,
        .close_cancelled = service_lifecycle_status.close_cancelled,
    };
    if (summary.active_listeners != 0) out.flags |= TCP_STATUS_FLAG_LISTENER_ACTIVE;
    if (stats.last_flags != 0 or stats.last_source_port != 0 or stats.last_dest_port != 0 or stats.last_payload_len != 0) {
        out.flags |= TCP_STATUS_FLAG_LAST_SEGMENT;
    }
    if (tcp_lifecycle.last_cause != SOCKET_LIFECYCLE_UNKNOWN_PUBLIC) out.flags |= TCP_STATUS_FLAG_LIFECYCLE_VALID;
    copyBounded(out.last_error[0..], stats.last_error);
    return out;
}

fn writeTcpOperation(w: *Writer, op: u16, payload: []const u8, client_id: u16) void {
    switch (op) {
        OP_TCP_CONNECT => writeTcpConnect(w, payload, client_id),
        OP_TCP_WRITE => writeTcpWrite(w, payload, client_id),
        OP_TCP_READ => writeTcpRead(w, payload, client_id),
        OP_TCP_CLOSE => writeTcpClose(w, payload, client_id),
        OP_TCP_LISTEN => writeTcpListen(w, payload),
        OP_TCP_ACCEPT_READ => writeTcpAcceptRead(w, payload, client_id),
        OP_TCP_CLOSE_LISTEN => writeTcpCloseListen(w, payload),
        OP_TCP_CONNECTIONS => writeTcpConnections(w),
        OP_TCP_POLL => writeTcpPoll(w, payload, client_id),
        OP_TCP_ACCEPT => writeTcpAccept(w, payload, client_id),
        else => unreachable,
    }
}

fn isTcpOperation(op: u16) bool {
    return op == OP_TCP_CONNECT or
        op == OP_TCP_WRITE or
        op == OP_TCP_READ or
        op == OP_TCP_CLOSE or
        op == OP_TCP_LISTEN or
        op == OP_TCP_ACCEPT_READ or
        op == OP_TCP_CLOSE_LISTEN or
        op == OP_TCP_CONNECTIONS or
        op == OP_TCP_POLL or
        op == OP_TCP_ACCEPT;
}

fn isTcpResultOperation(op: u16) bool {
    return op == OP_TCP_CONNECT_RESULT or
        op == OP_TCP_WRITE_RESULT or
        op == OP_TCP_READ_RESULT or
        op == OP_TCP_CLOSE_RESULT or
        op == OP_TCP_LISTEN_RESULT or
        op == OP_TCP_ACCEPT_READ_RESULT or
        op == OP_TCP_CLOSE_LISTEN_RESULT or
        op == OP_TCP_POLL_RESULT or
        op == OP_TCP_ACCEPT_RESULT or
        op == OP_TCP_ABORT_RESULT or
        op == OP_TCP_ACCEPT_POLL_RESULT or
        op == OP_TCP_RETRANSMIT_RESULT;
}

const TcpResultPayload = struct {
    result: TcpServiceResult,
    data: []const u8,
};

fn makeTcpResult(op: u16, payload: []const u8, data_out: []u8, client_id: u16) TcpResultPayload {
    const request = if (op == OP_TCP_CONNECT_RESULT) splitServiceDeadline(payload) else ServiceDeadlineRequest{ .payload = payload };
    var out = TcpServiceResult{ .action = tcpResultAction(op), .read_max = @intCast(TCP_RESULT_DATA_MAX), .owner_id = client_id };
    const data = switch (op) {
        OP_TCP_CONNECT_RESULT => tcpConnectResult(&out, request.payload, request.deadline_tick, client_id),
        OP_TCP_WRITE_RESULT => tcpWriteResult(&out, request.payload, client_id),
        OP_TCP_READ_RESULT => tcpReadResult(&out, request.payload, data_out, client_id),
        OP_TCP_CLOSE_RESULT => tcpCloseResult(&out, request.payload, client_id),
        OP_TCP_LISTEN_RESULT => tcpListenResult(&out, request.payload),
        OP_TCP_ACCEPT_READ_RESULT => tcpAcceptReadResult(&out, request.payload, data_out, client_id),
        OP_TCP_CLOSE_LISTEN_RESULT => tcpCloseListenResult(&out, request.payload),
        OP_TCP_POLL_RESULT => tcpPollResult(&out, request.payload, client_id),
        OP_TCP_ACCEPT_RESULT => tcpAcceptResult(&out, request.payload, client_id),
        OP_TCP_ABORT_RESULT => tcpAbortResult(&out, request.payload, client_id),
        OP_TCP_ACCEPT_POLL_RESULT => tcpAcceptPollResult(&out, request.payload, client_id),
        OP_TCP_RETRANSMIT_RESULT => tcpRetransmitResult(&out, request.payload, client_id),
        else => "",
    };
    fillTcpResultStatus(&out);
    setTcpOperationStatus(&out);
    return .{ .result = out, .data = data };
}

fn tcpResultAction(op: u16) u16 {
    return switch (op) {
        OP_TCP_CONNECT_RESULT => TCP_ACTION_CONNECT,
        OP_TCP_WRITE_RESULT => TCP_ACTION_WRITE,
        OP_TCP_READ_RESULT => TCP_ACTION_READ,
        OP_TCP_CLOSE_RESULT => TCP_ACTION_CLOSE,
        OP_TCP_LISTEN_RESULT => TCP_ACTION_LISTEN,
        OP_TCP_ACCEPT_READ_RESULT => TCP_ACTION_ACCEPT_READ,
        OP_TCP_CLOSE_LISTEN_RESULT => TCP_ACTION_CLOSE_LISTEN,
        OP_TCP_POLL_RESULT => TCP_ACTION_POLL,
        OP_TCP_ACCEPT_RESULT => TCP_ACTION_ACCEPT,
        OP_TCP_ABORT_RESULT => TCP_ACTION_ABORT,
        OP_TCP_ACCEPT_POLL_RESULT => TCP_ACTION_ACCEPT_POLL,
        OP_TCP_RETRANSMIT_RESULT => TCP_ACTION_RETRANSMIT,
        else => 0,
    };
}

fn tcpConnectResult(out: *TcpServiceResult, payload: []const u8, deadline_tick: ?u64, client_id: u16) []const u8 {
    if (payload.len < 6) return tcpBadRequestResult(out);
    out.remote_ip = .{ payload[0], payload[1], payload[2], payload[3] };
    out.port = readU16(payload, 4);
    out.remote_port = out.port;
    out.flags |= TCP_FLAG_REMOTE_VALID;
    const conn = net.tcpConnectUntil(out.remote_ip, out.port, deadline_tick);
    if (conn <= 0) {
        out.result = conn;
        setTcpLifecycle(out, tcpLifecycleCauseFromError(fixedText(net.tcpStats().last_error)));
        return "";
    }
    out.conn_id = @intCast(conn);
    out.flags |= TCP_FLAG_CONN_VALID;
    const svc_handle = allocateTcpHandle(out.conn_id, client_id) orelse {
        _ = net.tcpClose(out.conn_id);
        out.result = -2;
        copyBounded(out.last_error[0..], "handle-full");
        return "";
    };
    out.handle = svc_handle;
    out.result = 0;
    out.flags |= TCP_FLAG_OK | TCP_FLAG_HANDLE_VALID;
    setTcpLifecycle(out, SOCKET_LIFECYCLE_ACTIVE_PUBLIC);
    return "";
}

fn tcpWriteResult(out: *TcpServiceResult, payload: []const u8, client_id: u16) []const u8 {
    if (payload.len < 4) return tcpBadRequestResult(out);
    out.handle = readU32(payload, 0);
    const lookup = resolveTcpHandle(out.handle, client_id);
    if (lookup.result != 0) {
        out.result = lookup.result;
        copyBounded(out.last_error[0..], lookup.last_error);
        setTcpLifecycle(out, lookup.lifecycle_cause);
        return "";
    }
    const conn_id = lookup.conn_id;
    out.conn_id = conn_id;
    out.flags |= TCP_FLAG_HANDLE_VALID | TCP_FLAG_CONN_VALID;
    const data = payload[4..];
    out.requested_bytes = @intCast(data.len);
    // 0.56.37: Seit ipc_max_message_size=4096 tragen Write-Requests bis
    // ~4 KB Payload; net.tcpWrite baut aber genau EIN Draht-Segment pro
    // Aufruf, und Chunks von 1461..1516 Bytes rutschen durch dessen
    // groben Groessencheck (MAX_PACKET_SIZE-HEADER=1516) und sterben
    // erst am MTU-Check in sendIpv4Payload (tx-mtu-Reject OHNE
    // tx_large-Zaehler - Root-Cause der 0.56.37-Gate-FAILs, der
    // Aufrufer retryte denselben Chunk endlos). Deshalb HIER in
    // draht-sichere 1460er-Segmente zerlegen; bei Teilerfolg meldet
    // bytes die real geschriebene Zahl (Partial-Write-Semantik, die
    // SDK-Paced-Loops setzen den Rest fort).
    var total: usize = 0;
    var write_error: i32 = 0;
    while (total < data.len) {
        const chunk_len = @min(data.len - total, TCP_WIRE_SEGMENT_MAX);
        const written = net.tcpWrite(conn_id, data[total .. total + chunk_len]);
        if (written < 0) {
            write_error = written;
            break;
        }
        total += @intCast(written);
        if (written != @as(i32, @intCast(chunk_len))) break;
    }
    if (total > 0 or data.len == 0) {
        out.result = 0;
        out.bytes = @intCast(total);
        out.flags |= TCP_FLAG_OK;
        setTcpLifecycle(out, if (total == 0) SOCKET_LIFECYCLE_WOULD_BLOCK_PUBLIC else SOCKET_LIFECYCLE_ACTIVE_PUBLIC);
        if (total == 0) tcp_write_would_block += 1;
    } else if (write_error == 0) {
        // Erster Chunk lieferte written==0 (would-block ohne Fehlercode).
        out.result = 0;
        out.bytes = 0;
        out.flags |= TCP_FLAG_OK;
        setTcpLifecycle(out, SOCKET_LIFECYCLE_WOULD_BLOCK_PUBLIC);
        tcp_write_would_block += 1;
    } else {
        out.result = write_error;
        setTcpLifecycle(out, tcpLifecycleCauseFromError(fixedText(net.tcpStats().last_error)));
    }
    return "";
}

fn tcpReadResult(out: *TcpServiceResult, payload: []const u8, data_out: []u8, client_id: u16) []const u8 {
    if (payload.len < 6) return tcpBadRequestResult(out);
    out.handle = readU32(payload, 0);
    const lookup = resolveTcpHandle(out.handle, client_id);
    if (lookup.result != 0) {
        out.result = lookup.result;
        copyBounded(out.last_error[0..], lookup.last_error);
        setTcpLifecycle(out, lookup.lifecycle_cause);
        return "";
    }
    const conn_id = lookup.conn_id;
    out.conn_id = conn_id;
    out.flags |= TCP_FLAG_HANDLE_VALID | TCP_FLAG_CONN_VALID;
    const max_len = @min(@as(usize, readU16(payload, 4)), data_out.len);
    out.requested_bytes = @intCast(max_len);
    const got = net.tcpReadAvailable(conn_id, data_out[0..max_len]);
    out.result = got;
    if (got >= 0) {
        out.result = 0;
        out.bytes = @intCast(got);
        out.flags |= TCP_FLAG_OK;
        if (got > 0) {
            out.flags |= TCP_FLAG_DATA;
            setTcpLifecycle(out, SOCKET_LIFECYCLE_ACTIVE_PUBLIC);
        } else {
            copyBounded(out.last_error[0..], net_timing.operationStatusName(.would_block));
            setTcpServiceStatus(out, .would_block);
            setTcpLifecycle(out, SOCKET_LIFECYCLE_WOULD_BLOCK_PUBLIC);
        }
        return data_out[0..@as(usize, @intCast(got))];
    }
    setTcpLifecycle(out, tcpLifecycleCauseFromError(fixedText(net.tcpStats().last_error)));
    return "";
}

fn tcpCloseResult(out: *TcpServiceResult, payload: []const u8, client_id: u16) []const u8 {
    if (payload.len < 4) return tcpBadRequestResult(out);
    out.handle = readU32(payload, 0);
    const lookup = resolveTcpHandleForClose(out.handle, client_id);
    if (lookup.result != 0) {
        out.result = lookup.result;
        copyBounded(out.last_error[0..], lookup.last_error);
        setTcpLifecycle(out, lookup.lifecycle_cause);
        recordServiceClose(.failed);
        return "";
    }
    const conn_id = lookup.conn_id;
    out.conn_id = conn_id;
    out.flags |= TCP_FLAG_HANDLE_VALID | TCP_FLAG_CONN_VALID;
    const result = net.tcpClose(conn_id);
    out.result = result;
    if (result >= 0) {
        releaseTcpHandle(out.handle);
        out.result = 0;
        out.flags |= TCP_FLAG_OK;
        setTcpServiceStatus(out, .ok);
        setTcpLifecycle(out, SOCKET_LIFECYCLE_PENDING_CLOSE_PUBLIC);
        recordServiceClose(.ok);
    } else if (!tcpConnectionUsableForHandle(conn_id)) {
        releaseTcpHandle(out.handle);
        out.result = 0;
        out.flags |= TCP_FLAG_OK;
        setTcpServiceStatus(out, .cancelled);
        setTcpLifecycle(out, if (lookup.lifecycle_cause != SOCKET_LIFECYCLE_UNKNOWN_PUBLIC) lookup.lifecycle_cause else tcpLifecycleCauseForConnection(conn_id));
        recordServiceClose(.cancelled);
    } else {
        setTcpLifecycle(out, tcpLifecycleCauseFromError(fixedText(net.tcpStats().last_error)));
        recordServiceClose(.failed);
    }
    return "";
}

fn tcpAbortResult(out: *TcpServiceResult, payload: []const u8, client_id: u16) []const u8 {
    if (payload.len < 4) return tcpBadRequestResult(out);
    out.handle = readU32(payload, 0);
    const lookup = resolveTcpHandleForClose(out.handle, client_id);
    if (lookup.result != 0) {
        out.result = lookup.result;
        copyBounded(out.last_error[0..], lookup.last_error);
        setTcpLifecycle(out, lookup.lifecycle_cause);
        recordServiceClose(.failed);
        return "";
    }
    out.conn_id = lookup.conn_id;
    out.flags |= TCP_FLAG_HANDLE_VALID | TCP_FLAG_CONN_VALID;
    fillTcpConnectionFields(out);
    const result = net.tcpAbort(out.conn_id);
    releaseTcpHandle(out.handle);
    out.result = result;
    if (result == 0) {
        out.flags |= TCP_FLAG_OK;
        setTcpServiceStatus(out, .cancelled);
        setTcpLifecycle(out, SOCKET_LIFECYCLE_LOCAL_ABORT_PUBLIC);
        recordServiceClose(.cancelled);
    } else if (!tcpConnectionUsableForHandle(out.conn_id)) {
        out.result = 0;
        out.flags |= TCP_FLAG_OK;
        setTcpServiceStatus(out, .cancelled);
        setTcpLifecycle(out, if (lookup.lifecycle_cause != SOCKET_LIFECYCLE_UNKNOWN_PUBLIC) lookup.lifecycle_cause else tcpLifecycleCauseForConnection(out.conn_id));
        recordServiceClose(.cancelled);
    } else {
        setTcpLifecycle(out, tcpLifecycleCauseFromError(fixedText(net.tcpStats().last_error)));
        recordServiceClose(.failed);
    }
    return "";
}

fn tcpRetransmitResult(out: *TcpServiceResult, payload: []const u8, client_id: u16) []const u8 {
    if (payload.len < 4) return tcpBadRequestResult(out);
    out.handle = readU32(payload, 0);
    const lookup = resolveTcpHandle(out.handle, client_id);
    if (lookup.result != 0) {
        out.result = lookup.result;
        copyBounded(out.last_error[0..], lookup.last_error);
        setTcpLifecycle(out, lookup.lifecycle_cause);
        return "";
    }
    out.conn_id = lookup.conn_id;
    out.flags |= TCP_FLAG_HANDLE_VALID | TCP_FLAG_CONN_VALID;
    const result = net.tcpRetransmitLast(out.conn_id);
    out.result = result;
    if (result == 0) {
        out.flags |= TCP_FLAG_OK;
        setTcpLifecycle(out, SOCKET_LIFECYCLE_ACTIVE_PUBLIC);
    } else {
        setTcpLifecycle(out, tcpLifecycleCauseFromError(fixedText(net.tcpStats().last_error)));
    }
    fillTcpConnectionFields(out);
    return "";
}

fn tcpListenResult(out: *TcpServiceResult, payload: []const u8) []const u8 {
    if (payload.len < 2) return tcpBadRequestResult(out);
    out.port = readU16(payload, 0);
    out.local_port = out.port;
    out.local_ip = net.localIp();
    if (out.port == 0) return tcpBadRequestResult(out);
    if (net.tcpListen(out.port)) {
        out.result = 0;
        out.flags |= TCP_FLAG_OK | TCP_FLAG_LISTENER;
        setTcpLifecycle(out, SOCKET_LIFECYCLE_LISTENER_PUBLIC);
    } else {
        out.result = -1;
        setTcpLifecycle(out, tcpLifecycleCauseFromError(fixedText(net.tcpStats().last_error)));
    }
    return "";
}

fn tcpAcceptReadResult(out: *TcpServiceResult, payload: []const u8, data_out: []u8, client_id: u16) []const u8 {
    if (payload.len < 4) return tcpBadRequestResult(out);
    out.port = readU16(payload, 0);
    const max_len = @min(@as(usize, readU16(payload, 2)), data_out.len);
    out.requested_bytes = @intCast(max_len);
    if (out.port == 0 or max_len == 0) return tcpBadRequestResult(out);
    var conn_id: u32 = 0;
    const persistent_listener = net.tcpHasListener(out.port);
    const got = if (persistent_listener)
        net.tcpAcceptReadOnListener(out.port, data_out[0..max_len], &conn_id)
    else
        net.tcpAcceptReadOnce(out.port, data_out[0..max_len], &conn_id);
    out.conn_id = conn_id;
    if (got > 0) {
        const svc_handle = allocateTcpHandle(conn_id, client_id) orelse {
            _ = net.tcpClose(conn_id);
            out.result = -2;
            copyBounded(out.last_error[0..], "handle-full");
            return "";
        };
        out.handle = svc_handle;
        out.result = 0;
        out.bytes = @intCast(got);
        out.flags |= TCP_FLAG_OK | TCP_FLAG_DATA | TCP_FLAG_HANDLE_VALID | TCP_FLAG_CONN_VALID;
        setTcpLifecycle(out, SOCKET_LIFECYCLE_ACTIVE_PUBLIC);
        return data_out[0..@as(usize, @intCast(got))];
    }
    if (got == 0) {
        out.result = 0;
        out.flags |= TCP_FLAG_TIMEOUT;
        tcp_accept_wait_timeouts += 1;
        setTcpLifecycle(out, SOCKET_LIFECYCLE_TIMEOUT_PUBLIC);
        return "";
    }
    out.result = got;
    if (conn_id != 0) out.flags |= TCP_FLAG_CONN_VALID;
    setTcpLifecycle(out, tcpLifecycleCauseFromError(fixedText(net.tcpStats().last_error)));
    return "";
}

fn tcpAcceptResult(out: *TcpServiceResult, payload: []const u8, client_id: u16) []const u8 {
    if (payload.len < 2) return tcpBadRequestResult(out);
    out.port = readU16(payload, 0);
    if (out.port == 0) return tcpBadRequestResult(out);
    var conn_id: u32 = 0;
    const got = net.tcpAcceptOnListener(out.port, &conn_id);
    out.conn_id = conn_id;
    if (got > 0) {
        const svc_handle = allocateTcpHandle(conn_id, client_id) orelse {
            _ = net.tcpClose(conn_id);
            out.result = -2;
            copyBounded(out.last_error[0..], "handle-full");
            return "";
        };
        out.handle = svc_handle;
        out.result = 0;
        out.flags |= TCP_FLAG_OK | TCP_FLAG_HANDLE_VALID | TCP_FLAG_CONN_VALID;
        setTcpLifecycle(out, SOCKET_LIFECYCLE_ACTIVE_PUBLIC);
        return "";
    }
    if (got == 0) {
        out.result = 0;
        out.flags |= TCP_FLAG_TIMEOUT;
        tcp_accept_wait_timeouts += 1;
        setTcpLifecycle(out, SOCKET_LIFECYCLE_TIMEOUT_PUBLIC);
        return "";
    }
    out.result = got;
    if (conn_id != 0) out.flags |= TCP_FLAG_CONN_VALID;
    setTcpLifecycle(out, tcpLifecycleCauseFromError(fixedText(net.tcpStats().last_error)));
    return "";
}

fn tcpAcceptPollResult(out: *TcpServiceResult, payload: []const u8, client_id: u16) []const u8 {
    if (payload.len < 2) return tcpBadRequestResult(out);
    out.port = readU16(payload, 0);
    if (out.port == 0) return tcpBadRequestResult(out);
    var conn_id: u32 = 0;
    const got = net.tcpAcceptPollOnListener(out.port, &conn_id);
    out.conn_id = conn_id;
    if (got > 0) {
        const svc_handle = allocateTcpHandle(conn_id, client_id) orelse {
            _ = net.tcpClose(conn_id);
            out.result = -2;
            copyBounded(out.last_error[0..], "handle-full");
            return "";
        };
        out.handle = svc_handle;
        out.result = 0;
        out.flags |= TCP_FLAG_OK | TCP_FLAG_HANDLE_VALID | TCP_FLAG_CONN_VALID;
        setTcpLifecycle(out, SOCKET_LIFECYCLE_ACTIVE_PUBLIC);
        return "";
    }
    if (got == 0) {
        out.result = 0;
        out.flags |= TCP_FLAG_TIMEOUT;
        copyBounded(out.last_error[0..], "accept-poll-empty");
        setTcpServiceStatus(out, .would_block);
        setTcpLifecycle(out, SOCKET_LIFECYCLE_WOULD_BLOCK_PUBLIC);
        return "";
    }
    out.result = got;
    if (conn_id != 0) out.flags |= TCP_FLAG_CONN_VALID;
    setTcpLifecycle(out, tcpLifecycleCauseFromError(fixedText(net.tcpStats().last_error)));
    return "";
}

fn tcpCloseListenResult(out: *TcpServiceResult, payload: []const u8) []const u8 {
    if (payload.len < 2) return tcpBadRequestResult(out);
    out.port = readU16(payload, 0);
    if (out.port == 0) return tcpBadRequestResult(out);
    net.tcpCloseListener(out.port);
    out.result = 0;
    out.flags |= TCP_FLAG_OK | TCP_FLAG_LISTENER;
    setTcpLifecycle(out, SOCKET_LIFECYCLE_LOCAL_CLOSE_PUBLIC);
    return "";
}

fn tcpPollResult(out: *TcpServiceResult, payload: []const u8, client_id: u16) []const u8 {
    if (payload.len < 4) return tcpBadRequestResult(out);
    out.handle = readU32(payload, 0);
    const lookup = resolveTcpHandle(out.handle, client_id);
    if (lookup.result != 0) {
        out.result = lookup.result;
        copyBounded(out.last_error[0..], lookup.last_error);
        setTcpLifecycle(out, lookup.lifecycle_cause);
        return "";
    }
    out.conn_id = lookup.conn_id;
    net.tcpPollService();
    out.result = 0;
    out.flags |= TCP_FLAG_OK | TCP_FLAG_HANDLE_VALID | TCP_FLAG_CONN_VALID;
    fillTcpConnectionFields(out);
    if (out.lifecycle_cause == SOCKET_LIFECYCLE_UNKNOWN_PUBLIC) setTcpLifecycle(out, tcpLifecycleCauseForConnection(out.conn_id));
    return "";
}

fn tcpBadRequestResult(out: *TcpServiceResult) []const u8 {
    out.result = -1;
    setTcpServiceStatus(out, .failed);
    copyBounded(out.last_error[0..], "bad-request");
    return "";
}

fn fillTcpResultStatus(out: *TcpServiceResult) void {
    reapTcpStaleHandles();
    var summary: net.TcpSummary = .{};
    net.tcpSummary(&summary);
    out.active_connections = summary.active_connections;
    out.max_connections = summary.max_connections;
    out.active_listeners = summary.active_listeners;
    out.handle_count = tcpHandleCount();
    out.max_handles = TCP_SERVICE_HANDLES;
    out.tcp_buffer_size = summary.buffer_size;
    out.message_payload_max = TCP_MESSAGE_PAYLOAD_MAX;
    out.write_max = TCP_WRITE_PAYLOAD_MAX;
    out.read_max = @intCast(TCP_RESULT_DATA_MAX);
    if (out.local_ip[0] == 0 and out.local_ip[1] == 0 and out.local_ip[2] == 0 and out.local_ip[3] == 0) out.local_ip = net.localIp();
    if (out.conn_id != 0) fillTcpConnectionFields(out);
    if (out.last_error[0] == 0) copyBounded(out.last_error[0..], net.tcpStats().last_error);
    if (out.lifecycle_cause == SOCKET_LIFECYCLE_UNKNOWN_PUBLIC) setTcpLifecycle(out, tcpLifecycleCauseFromError(fixedText(out.last_error[0..])));
}

fn setTcpOperationStatus(out: *TcpServiceResult) void {
    if (serviceStatusEncoded(out.flags)) {
        out.service_status = serviceOperationStatusCode(out.flags);
        return;
    }
    const last_error = fixedText(out.last_error[0..]);
    if ((out.flags & TCP_FLAG_TIMEOUT) != 0 or textContains(last_error, "timeout")) {
        setTcpServiceStatus(out, .timeout);
        if (out.lifecycle_cause == SOCKET_LIFECYCLE_UNKNOWN_PUBLIC) setTcpLifecycle(out, SOCKET_LIFECYCLE_TIMEOUT_PUBLIC);
        return;
    }
    if (out.result == 0 and (out.flags & TCP_FLAG_OK) != 0) {
        setTcpServiceStatus(out, .ok);
        return;
    }
    if (textEquals(last_error, net_timing.operationStatusName(.would_block))) {
        setTcpServiceStatus(out, .would_block);
        if (out.lifecycle_cause == SOCKET_LIFECYCLE_UNKNOWN_PUBLIC) setTcpLifecycle(out, SOCKET_LIFECYCLE_WOULD_BLOCK_PUBLIC);
    } else {
        setTcpServiceStatus(out, .failed);
        if (out.lifecycle_cause == SOCKET_LIFECYCLE_UNKNOWN_PUBLIC) setTcpLifecycle(out, SOCKET_LIFECYCLE_BAD_HANDLE_PUBLIC);
    }
}

fn fillTcpConnectionFields(out: *TcpServiceResult) void {
    var summary: net.TcpSummary = .{};
    net.tcpSummary(&summary);
    var index: u32 = 0;
    while (index < summary.max_connections) : (index += 1) {
        var info: net.TcpConnectionInfo = .{};
        if (net.tcpConnectionInfo(index, &info) <= 0) continue;
        if (info.id != out.conn_id) continue;
        out.pending_rx = info.pending_rx;
        out.rx_window = info.rx_window;
        out.tx_window = info.tx_window;
        out.tx_seq = info.seq;
        out.tx_ack = info.tx_ack;
        out.retransmits = info.retransmits;
        out.rx_drops = info.rx_drops;
        out.local_ip = net.localIp();
        out.local_port = info.local_port;
        if (out.remote_ip[0] == 0 and out.remote_ip[1] == 0 and out.remote_ip[2] == 0 and out.remote_ip[3] == 0) out.remote_ip = info.remote_ip;
        if (out.remote_port == 0) out.remote_port = info.remote_port;
        out.flags |= TCP_FLAG_CONN_VALID;
        return;
    }
}

fn writeTcpConnect(w: *Writer, payload: []const u8, client_id: u16) void {
    if (payload.len < 6) {
        w.text("op=connect result=bad-request code=-1");
        return;
    }
    const remote_ip: [4]u8 = .{ payload[0], payload[1], payload[2], payload[3] };
    const port = readU16(payload, 4);
    const conn = net.tcpConnect(remote_ip, port);
    w.text("op=connect result=");
    if (conn > 0) {
        const conn_id: u32 = @intCast(conn);
        const svc_handle = allocateTcpHandle(conn_id, client_id) orelse {
            _ = net.tcpClose(conn_id);
            w.text("failed code=-2");
            w.text(" conn=");
            w.num(conn_id);
            w.text(" handle=0");
            w.text(" remote=");
            w.ip(remote_ip);
            w.text(":");
            w.num(port);
            w.text(" last=handle-full");
            return;
        };
        w.text("ok code=0 handle=");
        w.num(svc_handle);
        w.text(" conn=");
        w.num(conn_id);
    } else {
        w.text("failed code=");
        w.signed(conn);
        w.text(" handle=0 conn=0");
    }
    w.text(" remote=");
    w.ip(remote_ip);
    w.text(":");
    w.num(port);
    w.text(" last=");
    w.text(net.tcpStats().last_error);
}

fn writeTcpWrite(w: *Writer, payload: []const u8, client_id: u16) void {
    if (payload.len < 4) {
        w.text("op=write result=bad-request code=-1");
        return;
    }
    const svc_handle = readU32(payload, 0);
    const lookup = resolveTcpHandle(svc_handle, client_id);
    if (lookup.result != 0) {
        w.text("op=write result=failed code=");
        w.signed(lookup.result);
        w.text(" handle=");
        w.num(svc_handle);
        w.text(" last=");
        w.text(lookup.last_error);
        w.text(" handles=");
        writeTcpHandleUsage(w);
        return;
    }
    const conn_id = lookup.conn_id;
    const data = payload[4..];
    // 0.56.37: IPC-Messages tragen jetzt bis ~4 KB Payload, tcpWrite
    // sendet aber genau EIN Draht-Segment pro Aufruf (MTU 1500 =>
    // 1460 Datenbytes). Grosse Service-Writes werden deshalb HIER in
    // draht-sichere Segmente zerlegt; bei Teilerfolg wird die real
    // geschriebene Byte-Zahl gemeldet und der Aufrufer setzt den Rest
    // fort (gleiche Partial-Write-Semantik wie zuvor).
    var total: usize = 0;
    var write_error: i32 = 0;
    while (total < data.len) {
        const chunk_len = @min(data.len - total, TCP_WIRE_SEGMENT_MAX);
        const written = net.tcpWrite(conn_id, data[total .. total + chunk_len]);
        if (written < 0) {
            write_error = written;
            break;
        }
        total += @intCast(written);
        if (written != @as(i32, @intCast(chunk_len))) break;
    }
    w.text("op=write result=");
    if (total > 0 or data.len == 0) {
        w.text("ok code=0 bytes=");
        w.num(@as(u32, @intCast(total)));
    } else {
        w.text("failed code=");
        w.signed(write_error);
    }
    w.text(" handle=");
    w.num(svc_handle);
    w.text(" conn=");
    w.num(conn_id);
    w.text(" last=");
    w.text(net.tcpStats().last_error);
}

fn writeTcpRead(w: *Writer, payload: []const u8, client_id: u16) void {
    if (payload.len < 6) {
        w.text("op=read result=bad-request code=-1");
        return;
    }
    const svc_handle = readU32(payload, 0);
    const lookup = resolveTcpHandle(svc_handle, client_id);
    if (lookup.result != 0) {
        w.text("op=read result=failed code=");
        w.signed(lookup.result);
        w.text(" handle=");
        w.num(svc_handle);
        w.text(" last=");
        w.text(lookup.last_error);
        w.text(" handles=");
        writeTcpHandleUsage(w);
        return;
    }
    const conn_id = lookup.conn_id;
    const max_len = @min(@as(usize, readU16(payload, 4)), net.TCP_BUFFER_SIZE);
    var buf: [net.TCP_BUFFER_SIZE]u8 = undefined;
    const got = net.tcpRead(conn_id, buf[0..max_len]);
    w.text("op=read result=");
    if (got >= 0) {
        w.text("ok code=0 bytes=");
        w.num(@as(u32, @intCast(got)));
        if (got > 0) {
            w.text(" data=");
            w.text(buf[0..@as(usize, @intCast(got))]);
        }
    } else {
        w.text("failed code=");
        w.signed(got);
    }
    w.text(" handle=");
    w.num(svc_handle);
    w.text(" conn=");
    w.num(conn_id);
    w.text(" last=");
    w.text(net.tcpStats().last_error);
}

fn writeTcpClose(w: *Writer, payload: []const u8, client_id: u16) void {
    if (payload.len < 4) {
        w.text("op=close result=bad-request code=-1");
        recordServiceClose(.failed);
        return;
    }
    const svc_handle = readU32(payload, 0);
    const lookup = resolveTcpHandleForClose(svc_handle, client_id);
    if (lookup.result != 0) {
        recordServiceClose(.failed);
        w.text("op=close result=failed code=");
        w.signed(lookup.result);
        w.text(" handle=");
        w.num(svc_handle);
        w.text(" last=");
        w.text(lookup.last_error);
        w.text(" handles=");
        writeTcpHandleUsage(w);
        return;
    }
    const conn_id = lookup.conn_id;
    const result = net.tcpClose(conn_id);
    releaseTcpHandle(svc_handle);
    var status: net_timing.OperationStatus = .failed;
    w.text("op=close result=");
    if (result >= 0) {
        status = .ok;
        recordServiceClose(status);
        w.text("ok code=");
        w.num(0);
    } else if (!tcpConnectionUsableForHandle(conn_id)) {
        status = .cancelled;
        recordServiceClose(status);
        w.text("ok code=");
        w.num(0);
    } else {
        recordServiceClose(status);
        w.text("failed code=");
        w.signed(result);
    }
    w.text(" handle=");
    w.num(svc_handle);
    w.text(" conn=");
    w.num(conn_id);
    w.text(" last=");
    w.text(net.tcpStats().last_error);
    w.text(" status=");
    w.text(net_timing.operationStatusName(status));
}

fn writeTcpListen(w: *Writer, payload: []const u8) void {
    if (payload.len < 2) {
        w.text("op=listen result=bad-request code=-1");
        return;
    }
    const port = readU16(payload, 0);
    if (port == 0) {
        w.text("op=listen result=bad-request code=-1 port=0");
        return;
    }
    const ok = net.tcpListen(port);
    w.text("op=listen result=");
    w.text(if (ok) "ok" else "failed");
    w.text(" port=");
    w.num(port);
    w.text(" code=");
    w.signed(if (ok) 0 else -1);
    w.text(" last=");
    w.text(net.tcpStats().last_error);
}

fn writeTcpAcceptRead(w: *Writer, payload: []const u8, client_id: u16) void {
    if (payload.len < 4) {
        w.text("op=acceptread result=bad-request code=-1");
        return;
    }
    const port = readU16(payload, 0);
    const max_len = @min(@as(usize, readU16(payload, 2)), net.TCP_BUFFER_SIZE);
    if (port == 0 or max_len == 0) {
        w.text("op=acceptread result=bad-request code=-1");
        return;
    }
    var conn_id: u32 = 0;
    var buf: [net.TCP_BUFFER_SIZE]u8 = undefined;
    const persistent_listener = net.tcpHasListener(port);
    const got = if (persistent_listener)
        net.tcpAcceptReadOnListener(port, buf[0..max_len], &conn_id)
    else
        net.tcpAcceptReadOnce(port, buf[0..max_len], &conn_id);
    w.text("op=acceptread result=");
    if (got > 0) {
        const svc_handle = allocateTcpHandle(conn_id, client_id) orelse {
            _ = net.tcpClose(conn_id);
            w.text("failed code=-2 handle=0 conn=");
            w.num(conn_id);
            w.text(" port=");
            w.num(port);
            w.text(" last=handle-full");
            return;
        };
        w.text("ok code=0 bytes=");
        w.num(@as(u32, @intCast(got)));
        w.text(" handle=");
        w.num(svc_handle);
        w.text(" conn=");
        w.num(conn_id);
        w.text(" data=");
        w.text(buf[0..@as(usize, @intCast(got))]);
    } else if (got == 0) {
        w.text("timeout code=0 bytes=0 conn=0");
    } else {
        w.text("failed code=");
        w.signed(got);
        w.text(" conn=");
        w.num(conn_id);
    }
    w.text(" port=");
    w.num(port);
    w.text(" last=");
    w.text(net.tcpStats().last_error);
}

fn writeTcpAccept(w: *Writer, payload: []const u8, client_id: u16) void {
    if (payload.len < 2) {
        w.text("op=accept result=bad-request code=-1");
        return;
    }
    const port = readU16(payload, 0);
    if (port == 0) {
        w.text("op=accept result=bad-request code=-1");
        return;
    }
    var conn_id: u32 = 0;
    const got = net.tcpAcceptOnListener(port, &conn_id);
    w.text("op=accept result=");
    if (got > 0) {
        const svc_handle = allocateTcpHandle(conn_id, client_id) orelse {
            _ = net.tcpClose(conn_id);
            w.text("failed port=");
            w.num(port);
            w.text(" code=-2 handle=0 conn=");
            w.num(conn_id);
            w.text(" last=handle-full");
            return;
        };
        w.text("ok port=");
        w.num(port);
        w.text(" code=0 handle=");
        w.num(svc_handle);
        w.text(" conn=");
        w.num(conn_id);
    } else if (got == 0) {
        w.text("timeout port=");
        w.num(port);
        w.text(" code=0 handle=0 conn=0");
    } else {
        w.text("failed port=");
        w.num(port);
        w.text(" code=");
        w.signed(got);
        w.text(" handle=0 conn=");
        w.num(conn_id);
    }
    w.text(" last=");
    w.text(net.tcpStats().last_error);
}

fn writeTcpCloseListen(w: *Writer, payload: []const u8) void {
    if (payload.len < 2) {
        w.text("op=closelisten result=bad-request code=-1");
        return;
    }
    const port = readU16(payload, 0);
    if (port == 0) {
        w.text("op=closelisten result=bad-request code=-1 port=0");
        return;
    }
    net.tcpCloseListener(port);
    w.text("op=closelisten result=ok port=");
    w.num(port);
    w.text(" code=0");
    w.text(" last=");
    w.text(net.tcpStats().last_error);
}

fn writeTcpPoll(w: *Writer, payload: []const u8, client_id: u16) void {
    if (payload.len < 4) {
        w.text("op=poll result=bad-request code=-1");
        return;
    }
    const svc_handle = readU32(payload, 0);
    const lookup = resolveTcpHandle(svc_handle, client_id);
    if (lookup.result != 0) {
        w.text("op=poll result=failed code=");
        w.signed(lookup.result);
        w.text(" handle=");
        w.num(svc_handle);
        w.text(" last=");
        w.text(lookup.last_error);
        w.text(" handles=");
        writeTcpHandleUsage(w);
        return;
    }
    var result = TcpServiceResult{ .handle = svc_handle, .conn_id = lookup.conn_id };
    fillTcpConnectionFields(&result);
    w.text("op=poll result=ok code=0 handle=");
    w.num(svc_handle);
    w.text(" conn=");
    w.num(lookup.conn_id);
    w.text(" pending=");
    w.num(result.pending_rx);
    w.text(" win=");
    w.num(result.rx_window);
    w.text(" txwin=");
    w.num(result.tx_window);
    w.text(" txseq=");
    w.num(result.tx_seq);
    w.text(" txack=");
    w.num(result.tx_ack);
    w.text(" retrans=");
    w.num(result.retransmits);
    w.text(" drops=");
    w.num(result.rx_drops);
    w.text(" last=");
    w.text(net.tcpStats().last_error);
}

fn writeTcpConnections(w: *Writer) void {
    reapTcpStaleHandles();
    var summary: net.TcpSummary = .{};
    net.tcpSummary(&summary);
    w.text("connections=");
    w.num(summary.active_connections);
    w.text("/");
    w.num(summary.max_connections);
    w.text(" handles=");
    w.num(tcpHandleCount());
    w.text("/");
    w.num(TCP_SERVICE_HANDLES);
    var index: u32 = 0;
    var printed: u32 = 0;
    while (index < summary.max_connections) : (index += 1) {
        var info: net.TcpConnectionInfo = .{};
        if (net.tcpConnectionInfo(index, &info) <= 0) continue;
        printed += 1;
        w.text(" #");
        w.num(index);
        w.text(" id=");
        w.num(info.id);
        w.text(" handle=");
        w.num(handleForConnectionAny(info.id) orelse 0);
        w.text(" owner=");
        w.num(ownerForConnection(info.id));
        w.text(" ");
        w.text(tcpStateName(info.state));
        w.text(" l=");
        w.num(info.local_port);
        w.text(" r=");
        w.ip(info.remote_ip);
        w.text(":");
        w.num(info.remote_port);
        w.text(" rx=");
        w.num(info.rx_bytes);
        w.text(" tx=");
        w.num(info.tx_bytes);
        w.text(" pending=");
        w.num(info.pending_rx);
        w.text(" win=");
        w.num(info.rx_window);
        w.text(" txwin=");
        w.num(info.tx_window);
        w.text(" txack=");
        w.num(info.tx_ack);
        w.text(" retrans=");
        w.num(info.retransmits);
        w.text(" drops=");
        w.num(info.rx_drops);
        w.text(" seq=");
        w.num(info.seq);
        w.text(" ack=");
        w.num(info.ack);
        w.text(" last_flags=");
        w.num(info.last_flags);
        w.text(" last_payload=");
        w.num(info.last_payload_len);
    }
    if (printed == 0) w.text(" none");
}

fn tcpStateName(state: u8) []const u8 {
    return switch (state) {
        0 => "closed",
        1 => "syn-sent",
        2 => "established",
        3 => "fin-wait",
        4 => "syn-received",
        else => "unknown",
    };
}

fn netTxResultCode(result: net.TxResult) i32 {
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

const TcpHandleLookup = struct {
    conn_id: u32 = 0,
    result: i32 = -1,
    last_error: []const u8 = "bad-handle",
    lifecycle_cause: u32 = SOCKET_LIFECYCLE_BAD_HANDLE_PUBLIC,
};

fn allocateTcpHandle(conn_id: u32, owner_id: u16) ?u32 {
    reapTcpStaleHandles();
    if (handleForConnection(conn_id, owner_id)) |existing| return existing;
    var i: usize = 0;
    while (i < tcp_handles.len) : (i += 1) {
        if (tcp_handles[i].used) continue;
        const svc_handle = nextTcpHandle();
        tcp_handles[i] = .{ .used = true, .handle = svc_handle, .conn_id = conn_id, .owner_id = owner_id };
        return svc_handle;
    }
    return null;
}

fn nextTcpHandle() u32 {
    const svc_handle = tcp_next_handle;
    tcp_next_handle +%= 1;
    if (tcp_next_handle == 0) tcp_next_handle = 1;
    return svc_handle;
}

fn resolveTcpHandle(svc_handle: u32, owner_id: u16) TcpHandleLookup {
    var i: usize = 0;
    while (i < tcp_handles.len) : (i += 1) {
        if (!tcp_handles[i].used or tcp_handles[i].handle != svc_handle) continue;
        if (!tcpConnectionUsableForHandle(tcp_handles[i].conn_id)) {
            const cause = tcpLifecycleCauseForConnection(tcp_handles[i].conn_id);
            reapTcpHandleAt(i, cause);
            return .{ .result = -5, .last_error = "dead-handle", .lifecycle_cause = cause };
        }
        if (!tcpHandleOwnerMatches(tcp_handles[i].owner_id, owner_id)) {
            tcp_owner_mismatches += 1;
            recordTcpLifecycle(SOCKET_LIFECYCLE_OWNER_MISMATCH_PUBLIC);
            return .{ .result = -4, .last_error = "owner-mismatch", .lifecycle_cause = SOCKET_LIFECYCLE_OWNER_MISMATCH_PUBLIC };
        }
        return .{ .conn_id = tcp_handles[i].conn_id, .result = 0, .last_error = "", .lifecycle_cause = tcpLifecycleCauseForConnection(tcp_handles[i].conn_id) };
    }
    return .{};
}

fn resolveTcpHandleForClose(svc_handle: u32, owner_id: u16) TcpHandleLookup {
    var i: usize = 0;
    while (i < tcp_handles.len) : (i += 1) {
        if (!tcp_handles[i].used or tcp_handles[i].handle != svc_handle) continue;
        if (!tcpHandleOwnerMatches(tcp_handles[i].owner_id, owner_id)) {
            tcp_owner_mismatches += 1;
            recordTcpLifecycle(SOCKET_LIFECYCLE_OWNER_MISMATCH_PUBLIC);
            return .{ .result = -4, .last_error = "owner-mismatch", .lifecycle_cause = SOCKET_LIFECYCLE_OWNER_MISMATCH_PUBLIC };
        }
        return .{ .conn_id = tcp_handles[i].conn_id, .result = 0, .last_error = "", .lifecycle_cause = tcpLifecycleCauseForConnection(tcp_handles[i].conn_id) };
    }
    i = 0;
    while (i < tcp_stale_tombstones.len) : (i += 1) {
        if (!tcp_stale_tombstones[i].used or tcp_stale_tombstones[i].handle != svc_handle) continue;
        if (!tcpHandleOwnerMatches(tcp_stale_tombstones[i].owner_id, owner_id)) {
            tcp_owner_mismatches += 1;
            recordTcpLifecycle(SOCKET_LIFECYCLE_OWNER_MISMATCH_PUBLIC);
            return .{ .result = -4, .last_error = "owner-mismatch", .lifecycle_cause = SOCKET_LIFECYCLE_OWNER_MISMATCH_PUBLIC };
        }
        const conn_id = tcp_stale_tombstones[i].conn_id;
        const cause = tcp_stale_tombstones[i].lifecycle_cause;
        tcp_stale_tombstones[i] = .{};
        return .{ .conn_id = conn_id, .result = 0, .last_error = "", .lifecycle_cause = cause };
    }
    return .{};
}

fn releaseTcpHandle(svc_handle: u32) void {
    var i: usize = 0;
    while (i < tcp_handles.len) : (i += 1) {
        if (tcp_handles[i].used and tcp_handles[i].handle == svc_handle) {
            tcp_handles[i] = .{};
            return;
        }
    }
    i = 0;
    while (i < tcp_stale_tombstones.len) : (i += 1) {
        if (tcp_stale_tombstones[i].used and tcp_stale_tombstones[i].handle == svc_handle) {
            tcp_stale_tombstones[i] = .{};
            return;
        }
    }
}

fn clearTcpServiceState() void {
    tcp_handles = .{TcpServiceHandle{}} ** TCP_SERVICE_HANDLES;
    tcp_stale_tombstones = .{TcpStaleHandle{}} ** TCP_STALE_HANDLE_TOMBSTONES;
    tcp_stale_tombstone_next = 0;
    tcp_next_handle = 1;
}

fn tcpHandleCountRaw() u32 {
    var count: u32 = 0;
    var i: usize = 0;
    while (i < tcp_handles.len) : (i += 1) {
        if (tcp_handles[i].used) count += 1;
    }
    return count;
}

fn tcpTombstoneCountRaw() u32 {
    var count: u32 = 0;
    var i: usize = 0;
    while (i < tcp_stale_tombstones.len) : (i += 1) {
        if (tcp_stale_tombstones[i].used) count += 1;
    }
    return count;
}

fn tcpHandleCount() u32 {
    reapTcpStaleHandles();
    var count: u32 = 0;
    var i: usize = 0;
    while (i < tcp_handles.len) : (i += 1) {
        if (!tcp_handles[i].used) continue;
        count += 1;
    }
    return count;
}

fn tcpOwnedHandleCount(owner_id: u16) u16 {
    if (owner_id == 0) return 0;
    reapTcpStaleHandles();
    var count: u16 = 0;
    var i: usize = 0;
    while (i < tcp_handles.len) : (i += 1) {
        if (!tcp_handles[i].used) continue;
        if (tcp_handles[i].owner_id == owner_id) count += 1;
    }
    return count;
}

fn tcpLegacyHandleCount() u16 {
    reapTcpStaleHandles();
    var count: u16 = 0;
    var i: usize = 0;
    while (i < tcp_handles.len) : (i += 1) {
        if (!tcp_handles[i].used) continue;
        if (tcp_handles[i].owner_id == 0) count += 1;
    }
    return count;
}

fn writeTcpHandleUsage(w: *Writer) void {
    w.num(tcpHandleCount());
    w.text("/");
    w.num(TCP_SERVICE_HANDLES);
}

fn handleForConnection(conn_id: u32, owner_id: u16) ?u32 {
    reapTcpStaleHandles();
    var i: usize = 0;
    while (i < tcp_handles.len) : (i += 1) {
        if (tcp_handles[i].used and tcp_handles[i].conn_id == conn_id and tcp_handles[i].owner_id == owner_id) return tcp_handles[i].handle;
    }
    return null;
}

fn handleForConnectionAny(conn_id: u32) ?u32 {
    reapTcpStaleHandles();
    var i: usize = 0;
    while (i < tcp_handles.len) : (i += 1) {
        if (tcp_handles[i].used and tcp_handles[i].conn_id == conn_id) return tcp_handles[i].handle;
    }
    return null;
}

fn ownerForConnection(conn_id: u32) u16 {
    reapTcpStaleHandles();
    var i: usize = 0;
    while (i < tcp_handles.len) : (i += 1) {
        if (tcp_handles[i].used and tcp_handles[i].conn_id == conn_id) return tcp_handles[i].owner_id;
    }
    return 0;
}

fn tcpHandleOwnerMatches(handle_owner: u16, request_owner: u16) bool {
    return handle_owner == 0 or request_owner == 0 or handle_owner == request_owner;
}

fn reapTcpStaleHandles() void {
    var i: usize = 0;
    while (i < tcp_handles.len) : (i += 1) {
        if (!tcp_handles[i].used) continue;
        if (tcpConnectionUsableForHandle(tcp_handles[i].conn_id)) continue;
        reapTcpHandleAt(i, tcpLifecycleCauseForConnection(tcp_handles[i].conn_id));
    }
}

fn reapTcpHandleAt(index: usize, cause: u32) void {
    const entry = tcp_handles[index];
    const conn_id = entry.conn_id;
    rememberTcpStaleHandle(entry, cause);
    tcp_handles[index] = .{};
    tcp_stale_handles_reaped += 1;
    recordTcpLifecycle(cause);
    _ = net.tcpClose(conn_id);
}

fn rememberTcpStaleHandle(entry: TcpServiceHandle, cause: u32) void {
    if (!entry.used or entry.handle == 0) return;
    var i: usize = 0;
    while (i < tcp_stale_tombstones.len) : (i += 1) {
        if (tcp_stale_tombstones[i].used and tcp_stale_tombstones[i].handle == entry.handle) {
            tcp_stale_tombstones[i] = .{ .used = true, .handle = entry.handle, .conn_id = entry.conn_id, .owner_id = entry.owner_id, .lifecycle_cause = cause };
            return;
        }
    }
    i = 0;
    while (i < tcp_stale_tombstones.len) : (i += 1) {
        if (tcp_stale_tombstones[i].used) continue;
        tcp_stale_tombstones[i] = .{ .used = true, .handle = entry.handle, .conn_id = entry.conn_id, .owner_id = entry.owner_id, .lifecycle_cause = cause };
        return;
    }
    tcp_stale_tombstones[tcp_stale_tombstone_next] = .{ .used = true, .handle = entry.handle, .conn_id = entry.conn_id, .owner_id = entry.owner_id, .lifecycle_cause = cause };
    tcp_stale_tombstone_next = (tcp_stale_tombstone_next + 1) % tcp_stale_tombstones.len;
}

fn tcpConnectionUsableForHandle(conn_id: u32) bool {
    var summary: net.TcpSummary = .{};
    net.tcpSummary(&summary);
    var index: u32 = 0;
    while (index < summary.max_connections) : (index += 1) {
        var info: net.TcpConnectionInfo = .{};
        if (net.tcpConnectionInfo(index, &info) <= 0) continue;
        if (info.id != conn_id) continue;
        return info.state != 0 or info.pending_rx != 0;
    }
    return false;
}

fn writeError(out: []u8, channel_id: u32, op: u16, request_id: u32, status: i32) i32 {
    return writeResponse(out, channel_id, op, request_id, status, "");
}

fn writeResponse(out: []u8, channel_id: u32, op: u16, request_id: u32, status: i32, payload: []const u8) i32 {
    if (out.len < HEADER_SIZE) return RESULT_BAD_REQUEST;
    const len = if (payload.len > out.len - HEADER_SIZE) out.len - HEADER_SIZE else payload.len;
    writeHeader(out, channel_id, op, request_id, status, @intCast(len)) orelse return RESULT_BAD_REQUEST;
    if (len != 0) @memcpy(out[HEADER_SIZE .. HEADER_SIZE + len], payload[0..len]);
    return @intCast(HEADER_SIZE + len);
}

pub fn serviceSemanticFlags(flags: u32) u32 {
    return flags & ~SERVICE_STATUS_MASK;
}

pub fn serviceOperationStatusName(flags: u32) []const u8 {
    return net_timing.operationStatusName(serviceOperationStatus(flags));
}

fn recordServiceClose(status: net_timing.OperationStatus) void {
    switch (status) {
        .ok => service_lifecycle_status.close_ok += 1,
        .cancelled => service_lifecycle_status.close_cancelled += 1,
        else => service_lifecycle_status.close_failed += 1,
    }
}

fn setTcpServiceStatus(out: *TcpServiceResult, status: net_timing.OperationStatus) void {
    out.flags = withServiceStatus(out.flags, status);
    out.service_status = @intFromEnum(status);
}

fn setUdpServiceStatus(out: *UdpServiceResult, status: net_timing.OperationStatus) void {
    out.flags = withServiceStatus(out.flags, status);
    out.service_status = @intFromEnum(status);
}

fn setTcpLifecycle(out: *TcpServiceResult, cause: u32) void {
    if (cause == SOCKET_LIFECYCLE_UNKNOWN_PUBLIC or out.lifecycle_cause == cause) return;
    out.lifecycle_cause = cause;
    out.flags |= TCP_FLAG_LIFECYCLE_VALID;
    recordTcpLifecycle(cause);
}

fn setUdpLifecycle(out: *UdpServiceResult, cause: u32) void {
    if (cause == SOCKET_LIFECYCLE_UNKNOWN_PUBLIC or out.lifecycle_cause == cause) return;
    out.lifecycle_cause = cause;
    out.flags |= UDP_FLAG_LIFECYCLE_VALID;
    recordUdpLifecycle(cause);
}

fn recordTcpLifecycle(cause: u32) void {
    recordSocketLifecycle(&tcp_lifecycle, cause);
}

fn recordUdpLifecycle(cause: u32) void {
    recordSocketLifecycle(&udp_lifecycle, cause);
}

fn recordSocketLifecycle(counters: *SocketLifecycleCounters, cause: u32) void {
    if (cause == SOCKET_LIFECYCLE_UNKNOWN_PUBLIC) return;
    counters.last_cause = cause;
    switch (cause) {
        SOCKET_LIFECYCLE_CLOSED_PUBLIC => counters.closed += 1,
        SOCKET_LIFECYCLE_RESET_PUBLIC => counters.reset += 1,
        SOCKET_LIFECYCLE_TIMEOUT_PUBLIC => counters.timeout += 1,
        SOCKET_LIFECYCLE_PEER_GONE_PUBLIC => counters.peer_gone += 1,
        SOCKET_LIFECYCLE_LOCAL_ABORT_PUBLIC => counters.local_abort += 1,
        SOCKET_LIFECYCLE_LOCAL_CLOSE_PUBLIC => counters.local_close += 1,
        SOCKET_LIFECYCLE_PENDING_CLOSE_PUBLIC => counters.pending_close += 1,
        SOCKET_LIFECYCLE_WOULD_BLOCK_PUBLIC => counters.would_block += 1,
        SOCKET_LIFECYCLE_BAD_HANDLE_PUBLIC => counters.bad_handle += 1,
        SOCKET_LIFECYCLE_OWNER_MISMATCH_PUBLIC => counters.owner_mismatch += 1,
        SOCKET_LIFECYCLE_DROPPED_PUBLIC => counters.dropped += 1,
        else => {},
    }
}

fn tcpLifecycleCauseForConnection(conn_id: u32) u32 {
    var summary: net.TcpSummary = .{};
    net.tcpSummary(&summary);
    var index: u32 = 0;
    while (index < summary.max_connections) : (index += 1) {
        var info: net.TcpConnectionInfo = .{};
        if (net.tcpConnectionInfo(index, &info) <= 0) continue;
        if (info.id != conn_id) continue;
        return switch (info.state) {
            0 => if (info.pending_rx != 0) SOCKET_LIFECYCLE_PEER_GONE_PUBLIC else SOCKET_LIFECYCLE_CLOSED_PUBLIC,
            3 => SOCKET_LIFECYCLE_PENDING_CLOSE_PUBLIC,
            else => SOCKET_LIFECYCLE_ACTIVE_PUBLIC,
        };
    }
    const from_error = tcpLifecycleCauseFromError(fixedText(net.tcpStats().last_error));
    return if (from_error == SOCKET_LIFECYCLE_UNKNOWN_PUBLIC) SOCKET_LIFECYCLE_PEER_GONE_PUBLIC else from_error;
}

fn tcpLifecycleCauseFromError(last_error: []const u8) u32 {
    if (last_error.len == 0) return SOCKET_LIFECYCLE_UNKNOWN_PUBLIC;
    if (textContains(last_error, "owner-mismatch")) return SOCKET_LIFECYCLE_OWNER_MISMATCH_PUBLIC;
    if (textContains(last_error, "bad-handle") or textContains(last_error, "handle-full") or textContains(last_error, "dead-handle")) return SOCKET_LIFECYCLE_BAD_HANDLE_PUBLIC;
    if (textContains(last_error, "would-block") or textContains(last_error, "poll-empty")) return SOCKET_LIFECYCLE_WOULD_BLOCK_PUBLIC;
    if (textContains(last_error, "timeout")) return SOCKET_LIFECYCLE_TIMEOUT_PUBLIC;
    if (textContains(last_error, "rst") or textContains(last_error, "reset")) return SOCKET_LIFECYCLE_RESET_PUBLIC;
    if (textContains(last_error, "fin-rx") or textContains(last_error, "peer")) return SOCKET_LIFECYCLE_PEER_GONE_PUBLIC;
    if (textContains(last_error, "abort")) return SOCKET_LIFECYCLE_LOCAL_ABORT_PUBLIC;
    if (textContains(last_error, "close")) return SOCKET_LIFECYCLE_LOCAL_CLOSE_PUBLIC;
    if (textContains(last_error, "bad-state")) return SOCKET_LIFECYCLE_CLOSED_PUBLIC;
    return SOCKET_LIFECYCLE_UNKNOWN_PUBLIC;
}

fn socketLifecycleName(cause: u32) []const u8 {
    return switch (cause) {
        SOCKET_LIFECYCLE_ACTIVE_PUBLIC => "active",
        SOCKET_LIFECYCLE_CLOSED_PUBLIC => "closed",
        SOCKET_LIFECYCLE_RESET_PUBLIC => "reset",
        SOCKET_LIFECYCLE_TIMEOUT_PUBLIC => "timeout",
        SOCKET_LIFECYCLE_PEER_GONE_PUBLIC => "peer-gone",
        SOCKET_LIFECYCLE_LOCAL_ABORT_PUBLIC => "local-abort",
        SOCKET_LIFECYCLE_LOCAL_CLOSE_PUBLIC => "local-close",
        SOCKET_LIFECYCLE_PENDING_CLOSE_PUBLIC => "pending-close",
        SOCKET_LIFECYCLE_WOULD_BLOCK_PUBLIC => "would-block",
        SOCKET_LIFECYCLE_BAD_HANDLE_PUBLIC => "bad-handle",
        SOCKET_LIFECYCLE_OWNER_MISMATCH_PUBLIC => "owner-mismatch",
        SOCKET_LIFECYCLE_LISTENER_PUBLIC => "listener",
        SOCKET_LIFECYCLE_DROPPED_PUBLIC => "dropped",
        else => "unknown",
    };
}

fn withServiceStatus(flags: u32, status: net_timing.OperationStatus) u32 {
    return serviceSemanticFlags(flags) | (@as(u32, @intFromEnum(status)) << SERVICE_STATUS_SHIFT);
}

fn serviceStatusEncoded(flags: u32) bool {
    return (flags & SERVICE_STATUS_MASK) != 0;
}

fn serviceOperationStatus(flags: u32) net_timing.OperationStatus {
    return switch ((flags & SERVICE_STATUS_MASK) >> SERVICE_STATUS_SHIFT) {
        0 => .idle,
        1 => .pending,
        2 => .ok,
        3 => .timeout,
        4 => .failed,
        5 => .cancelled,
        6 => .would_block,
        else => .failed,
    };
}

fn serviceOperationStatusCode(flags: u32) u32 {
    return @intFromEnum(serviceOperationStatus(flags));
}

fn serviceStatusFromTxResult(result: net.TxResult, last_error: []const u8) net_timing.OperationStatus {
    return switch (result) {
        .ok => .ok,
        .busy => .would_block,
        .backend_error => if (textContains(last_error, "timeout")) .timeout else .failed,
        else => .failed,
    };
}

fn serviceStatusFromDnsResult(result: i32) net_timing.OperationStatus {
    return switch (result) {
        dns.RESULT_OK => .ok,
        dns.RESULT_TIMEOUT => .timeout,
        else => .failed,
    };
}

fn fixedText(value: []const u8) []const u8 {
    return value[0..stringLenZ(value)];
}

fn textEquals(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var index: usize = 0;
    while (index < a.len) : (index += 1) {
        if (a[index] != b[index]) return false;
    }
    return true;
}

fn textContains(haystack: []const u8, needle: []const u8) bool {
    if (needle.len == 0) return true;
    if (needle.len > haystack.len) return false;
    var index: usize = 0;
    while (index + needle.len <= haystack.len) : (index += 1) {
        if (textEquals(haystack[index .. index + needle.len], needle)) return true;
    }
    return false;
}

fn copyBounded(out: []u8, value: []const u8) void {
    @memset(out, 0);
    if (out.len == 0) return;
    const len = @min(value.len, out.len - 1);
    if (len != 0) @memcpy(out[0..len], value[0..len]);
}

const ServiceDeadlineRequest = struct {
    payload: []const u8,
    deadline_tick: ?u64 = null,
};

fn splitServiceDeadline(payload: []const u8) ServiceDeadlineRequest {
    const footer_size = @sizeOf(r4x_api.ServiceDeadlineFooter);
    if (payload.len < footer_size) return .{ .payload = payload };
    const footer = payload[payload.len - footer_size ..];
    const expected = r4x_api.ServiceDeadlineFooter{};
    if (readU32(footer, 0) != expected.magic or
        readU16(footer, 4) != expected.version or
        readU16(footer, 6) != footer_size or
        readU32(footer, 8) != @as(u32, @intCast(payload.len - footer_size)) or
        readU32(footer, 12) != 0)
    {
        return .{ .payload = payload };
    }
    return .{
        .payload = payload[0 .. payload.len - footer_size],
        .deadline_tick = readU64(footer, 16),
    };
}

fn stringLenZ(value: []const u8) usize {
    var len: usize = 0;
    while (len < value.len and value[len] != 0) : (len += 1) {}
    return len;
}

fn writeHeader(out: []u8, channel_id: u32, op: u16, request_id: u32, status: i32, payload_len: u16) ?void {
    if (out.len < HEADER_SIZE) return null;
    writeU32(out, 0, MAGIC);
    writeU16(out, 4, VERSION);
    writeU16(out, 6, @intCast(channel_id));
    writeU16(out, 8, op);
    writeU16(out, 10, 0);
    writeU32(out, 12, request_id);
    writeU16(out, 16, 0);
    writeU16(out, 18, payload_len);
    writeI32(out, 20, status);
}

const Writer = struct {
    out: []u8,
    pos: usize = 0,

    fn put(self: *Writer, ch: u8) void {
        if (self.pos >= self.out.len) return;
        self.out[self.pos] = ch;
        self.pos += 1;
    }

    fn text(self: *Writer, value: []const u8) void {
        for (value) |ch| if (ch != 0) self.put(ch);
    }

    fn slice(self: *const Writer) []const u8 {
        return self.out[0..self.pos];
    }

    fn num(self: *Writer, value: anytype) void {
        var buf: [20]u8 = undefined;
        var pos: usize = buf.len;
        var n: u64 = @intCast(value);
        if (n == 0) {
            self.put('0');
            return;
        }
        while (n > 0) {
            pos -= 1;
            buf[pos] = '0' + @as(u8, @intCast(n % 10));
            n /= 10;
        }
        self.text(buf[pos..]);
    }

    fn ip(self: *Writer, value: [4]u8) void {
        self.num(value[0]);
        self.put('.');
        self.num(value[1]);
        self.put('.');
        self.num(value[2]);
        self.put('.');
        self.num(value[3]);
    }

    fn signed(self: *Writer, value: i32) void {
        if (value < 0) {
            self.put('-');
            self.num(@as(u32, @intCast(-value)));
        } else {
            self.num(@as(u32, @intCast(value)));
        }
    }
};

fn writeU16(out: []u8, offset: usize, value: u16) void {
    out[offset] = @intCast(value & 0xFF);
    out[offset + 1] = @intCast(value >> 8);
}

fn writeU32(out: []u8, offset: usize, value: u32) void {
    writeU16(out, offset, @intCast(value & 0xFFFF));
    writeU16(out, offset + 2, @intCast(value >> 16));
}

fn writeI32(out: []u8, offset: usize, value: i32) void {
    writeU32(out, offset, @bitCast(value));
}

fn readU16(in_bytes: []const u8, offset: usize) u16 {
    return @as(u16, in_bytes[offset]) | (@as(u16, in_bytes[offset + 1]) << 8);
}

fn readU32(in_bytes: []const u8, offset: usize) u32 {
    return @as(u32, readU16(in_bytes, offset)) | (@as(u32, readU16(in_bytes, offset + 2)) << 16);
}

fn readU64(in_bytes: []const u8, offset: usize) u64 {
    return @as(u64, readU32(in_bytes, offset)) | (@as(u64, readU32(in_bytes, offset + 4)) << 32);
}

fn readI32(in_bytes: []const u8, offset: usize) i32 {
    return @bitCast(readU32(in_bytes, offset));
}
