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
// Text replies reserve room for fixed labels, decimal IDs and bounded errors.
// Request-owned scratch stays below 4 KB, including while the TCP call waits.
const TCP_TEXT_METADATA_MAX: usize = 256;
const TCP_TEXT_DATA_MAX: usize = TCP_MESSAGE_PAYLOAD_MAX - TCP_TEXT_METADATA_MAX;
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

pub const RestartStatus = extern struct {
    restarts: u64 = 0,
    last_queued: u32 = 0,
    last_handles: u32 = 0,
    last_tombstones: u32 = 0,
};

pub const ServiceBackpressureStatus = extern struct {
    payload_too_large: u64 = 0,
    response_buffer_small: u64 = 0,
    send_failures: u64 = 0,
    stale_skips: u64 = 0,
    request_payload_limit: u32 = @intCast(TCP_MESSAGE_PAYLOAD_MAX),
    response_buffer_limit: u32 = @intCast(ipc.MAX_MESSAGE_SIZE),
};

pub const ServiceLifecycleStatus = extern struct {
    close_ok: u64 = 0,
    close_failed: u64 = 0,
    close_cancelled: u64 = 0,
    cleanup_aborts: u64 = 0,
    cleanup_poweroff: u64 = 0,
    cleanup_reboot: u64 = 0,
};

const SocketLifecycleCounters = extern struct {
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

pub fn init() void {
    r4os_rust_net_ipc_init();
}

pub fn restartServices(reason: []const u8) RestartStatus {
    _ = reason;
    var out: RestartStatus = undefined;
    r4os_rust_net_ipc_restart(&out);
    return out;
}

pub fn restartStatus() RestartStatus {
    var out: RestartStatus = undefined;
    r4os_rust_net_ipc_restart_status(&out);
    return out;
}

pub fn serviceName(channel_id: u32) []const u8 {
    var out: NativeText = undefined;
    r4os_rust_net_ipc_service_name(channel_id, &out);
    return out.ptr[0..out.len];
}

const QueueStatus = struct {
    channels: u32 = 0,
    handlers: u32 = 0,
    queued: u32 = 0,
    limit: u32 = 0,
    drops: u64 = 0,
};

pub fn statusRequest(channel_id: u32, request_id: u32, out: []u8) i32 {
    return sendRequest(channel_id, OP_STATUS, request_id, "", out);
}

pub fn sendRequest(channel_id: u32, op: u16, request_id: u32, payload: []const u8, out: []u8) i32 {
    return sendRequestAsClient(channel_id, op, request_id, 0, payload, out);
}

pub fn sendRequestAsClient(channel_id: u32, op: u16, request_id: u32, client_id: u16, payload: []const u8, out: []u8) i32 {
    return r4os_rust_net_ipc_request(channel_id, op, request_id, client_id, payload.ptr, payload.len, out.ptr, out.len);
}

pub fn staleResponseSkips() u64 {
    return r4os_rust_net_ipc_stale_skips();
}

pub fn serviceBackpressureStatus() ServiceBackpressureStatus {
    var out: ServiceBackpressureStatus = undefined;
    r4os_rust_net_ipc_backpressure(&out);
    return out;
}

pub fn serviceErrorCount() u64 {
    return r4os_rust_net_ipc_error_count();
}

pub fn logServiceErrorStatus(reason: []const u8) void {
    r4os_rust_net_ipc_log_errors(.{ .ptr = reason.ptr, .len = reason.len });
}

pub fn noteCleanupLifecycle(reason: []const u8, cleanup: net.CleanupStatus) void {
    r4os_rust_net_ipc_note_cleanup(.{ .ptr = reason.ptr, .len = reason.len }, cleanup.last_tcp_connections, cleanup.last_tcp_listeners);
}

pub fn serviceLifecycleStatus() ServiceLifecycleStatus {
    var out: ServiceLifecycleStatus = undefined;
    r4os_rust_net_ipc_lifecycle(&out);
    return out;
}

pub fn parsePayload(response: []const u8, status: *i32) ?[]const u8 {
    var len: usize = 0;
    const data = r4os_rust_net_ipc_parse(response.ptr, response.len, status, &len) orelse return null;
    return data[0..len];
}

// TCP- und UDP-Ergebnisantworten besitzen jeweils zwei fast 4 KB grosse
// Arbeitsbereiche. Bleiben beide Zweige im gemeinsamen Handler, reserviert
// ReleaseSafe sie zusammen in dessen Frame. Der einzelne service-ipc-Worker
// lief dadurch beim ersten SSH-Accept in den Guard seines 64-KB-Stacks.
// Getrennte, nicht inlinebare Frames halten nur den tatsaechlich ausgefuehrten
// Protokollzweig gleichzeitig aktiv und bewahren die Handler-Reentranz.

const UdpResultPayload = struct {
    result: UdpServiceResult,
    data: []const u8,
};

const TcpResultPayload = struct {
    result: TcpServiceResult,
    data: []const u8,
};

const TcpHandleLookup = struct {
    conn_id: u32 = 0,
    result: i32 = -1,
    last_error: []const u8 = "bad-handle",
    lifecycle_cause: u32 = SOCKET_LIFECYCLE_BAD_HANDLE_PUBLIC,
};

pub fn serviceSemanticFlags(flags: u32) u32 {
    return r4os_rust_net_ipc_semantic_flags(flags);
}

pub fn serviceOperationStatusName(flags: u32) []const u8 {
    var out: NativeText = undefined;
    r4os_rust_net_ipc_status_name(flags, &out);
    return out.ptr[0..out.len];
}

const ServiceDeadlineRequest = struct {
    payload: []const u8,
    deadline_tick: ?u64 = null,
};

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

    fn bytes(self: *Writer, value: []const u8) void {
        for (value) |ch| self.put(ch);
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

const NativeText = extern struct { ptr: [*]const u8, len: usize };
extern fn r4os_rust_net_ipc_init() void;
extern fn r4os_rust_net_ipc_handle(u32, [*]const u8, usize, [*]u8, usize) i32;
extern fn r4os_rust_net_ipc_request(u32, u16, u32, u16, [*]const u8, usize, [*]u8, usize) i32;
extern fn r4os_rust_net_ipc_restart(*RestartStatus) void;
extern fn r4os_rust_net_ipc_restart_status(*RestartStatus) void;
extern fn r4os_rust_net_ipc_backpressure(*ServiceBackpressureStatus) void;
extern fn r4os_rust_net_ipc_lifecycle(*ServiceLifecycleStatus) void;
extern fn r4os_rust_net_ipc_stale_skips() u64;
extern fn r4os_rust_net_ipc_error_count() u64;
extern fn r4os_rust_net_ipc_note_cleanup(NativeText, u32, u32) void;
extern fn r4os_rust_net_ipc_service_name(u32, *NativeText) void;
extern fn r4os_rust_net_ipc_parse([*]const u8, usize, *i32, *usize) ?[*]const u8;
extern fn r4os_rust_net_ipc_semantic_flags(u32) u32;
extern fn r4os_rust_net_ipc_status_name(u32, *NativeText) void;
extern fn r4os_rust_net_ipc_log_errors(NativeText) void;
export fn r4os_zig_net_ipc_register(id: u32) callconv(.c) void {
    _ = ipc.registerService(id, serviceName(id), handle);
}
fn handle(id: u32, request: []const u8, response: []u8) i32 {
    return r4os_rust_net_ipc_handle(id, request.ptr, request.len, response.ptr, response.len);
}
comptime {
    if (@sizeOf(NativeText) != 16 or @alignOf(NativeText) != 8 or @offsetOf(NativeText, "ptr") != 0 or @offsetOf(NativeText, "len") != 8) @compileError("Native IPC text layout");
}

comptime {
    if (@sizeOf(DhcpServiceStatus) != 208 or @alignOf(DhcpServiceStatus) != 8 or @offsetOf(DhcpServiceStatus, "magic") != 0 or @offsetOf(DhcpServiceStatus, "version") != 4 or @offsetOf(DhcpServiceStatus, "runtime_state") != 6 or @offsetOf(DhcpServiceStatus, "flags") != 8 or @offsetOf(DhcpServiceStatus, "xid") != 12 or @offsetOf(DhcpServiceStatus, "offered_ip") != 16 or @offsetOf(DhcpServiceStatus, "server_ip") != 20 or @offsetOf(DhcpServiceStatus, "netmask") != 24 or @offsetOf(DhcpServiceStatus, "gateway_ip") != 28 or @offsetOf(DhcpServiceStatus, "dns_ip") != 32 or @offsetOf(DhcpServiceStatus, "lease_seconds") != 36 or @offsetOf(DhcpServiceStatus, "renew_seconds") != 40 or @offsetOf(DhcpServiceStatus, "rebind_seconds") != 44 or @offsetOf(DhcpServiceStatus, "elapsed_seconds") != 48 or @offsetOf(DhcpServiceStatus, "remaining_seconds") != 52 or @offsetOf(DhcpServiceStatus, "renew_in_seconds") != 56 or @offsetOf(DhcpServiceStatus, "rebind_in_seconds") != 60 or @offsetOf(DhcpServiceStatus, "last_attempt") != 64 or @offsetOf(DhcpServiceStatus, "last_type") != 68 or @offsetOf(DhcpServiceStatus, "discover_tx") != 72 or @offsetOf(DhcpServiceStatus, "offer_rx") != 80 or @offsetOf(DhcpServiceStatus, "request_tx") != 88 or @offsetOf(DhcpServiceStatus, "ack_rx") != 96 or @offsetOf(DhcpServiceStatus, "nak_rx") != 104 or @offsetOf(DhcpServiceStatus, "release_tx") != 112 or @offsetOf(DhcpServiceStatus, "retries") != 120 or @offsetOf(DhcpServiceStatus, "timeouts") != 128 or @offsetOf(DhcpServiceStatus, "release_errors") != 136 or @offsetOf(DhcpServiceStatus, "malformed") != 144 or @offsetOf(DhcpServiceStatus, "self_tests") != 152 or @offsetOf(DhcpServiceStatus, "pending_label") != 160 or @offsetOf(DhcpServiceStatus, "last_error") != 176) @compileError("Native NIPC DhcpServiceStatus layout");
}

comptime {
    if (@sizeOf(DhcpServiceResult) != 144 or @alignOf(DhcpServiceResult) != 8 or @offsetOf(DhcpServiceResult, "magic") != 0 or @offsetOf(DhcpServiceResult, "version") != 4 or @offsetOf(DhcpServiceResult, "action") != 6 or @offsetOf(DhcpServiceResult, "result") != 8 or @offsetOf(DhcpServiceResult, "flags") != 12 or @offsetOf(DhcpServiceResult, "offered_ip") != 16 or @offsetOf(DhcpServiceResult, "server_ip") != 20 or @offsetOf(DhcpServiceResult, "netmask") != 24 or @offsetOf(DhcpServiceResult, "gateway_ip") != 28 or @offsetOf(DhcpServiceResult, "dns_ip") != 32 or @offsetOf(DhcpServiceResult, "lease_seconds") != 36 or @offsetOf(DhcpServiceResult, "elapsed_seconds") != 40 or @offsetOf(DhcpServiceResult, "remaining_seconds") != 44 or @offsetOf(DhcpServiceResult, "renew_in_seconds") != 48 or @offsetOf(DhcpServiceResult, "rebind_in_seconds") != 52 or @offsetOf(DhcpServiceResult, "last_attempt") != 56 or @offsetOf(DhcpServiceResult, "last_type") != 60 or @offsetOf(DhcpServiceResult, "discover_tx") != 64 or @offsetOf(DhcpServiceResult, "offer_rx") != 72 or @offsetOf(DhcpServiceResult, "request_tx") != 80 or @offsetOf(DhcpServiceResult, "ack_rx") != 88 or @offsetOf(DhcpServiceResult, "nak_rx") != 96 or @offsetOf(DhcpServiceResult, "release_tx") != 104 or @offsetOf(DhcpServiceResult, "retries") != 112 or @offsetOf(DhcpServiceResult, "timeouts") != 120 or @offsetOf(DhcpServiceResult, "release_errors") != 128 or @offsetOf(DhcpServiceResult, "malformed") != 136) @compileError("Native NIPC DhcpServiceResult layout");
}

comptime {
    if (@sizeOf(DnsServiceStatus) != 368 or @alignOf(DnsServiceStatus) != 8 or @offsetOf(DnsServiceStatus, "magic") != 0 or @offsetOf(DnsServiceStatus, "version") != 4 or @offsetOf(DnsServiceStatus, "reserved") != 6 or @offsetOf(DnsServiceStatus, "flags") != 8 or @offsetOf(DnsServiceStatus, "last_result") != 12 or @offsetOf(DnsServiceStatus, "last_id") != 16 or @offsetOf(DnsServiceStatus, "name_len") != 18 or @offsetOf(DnsServiceStatus, "pending_name_len") != 20 or @offsetOf(DnsServiceStatus, "cache_name_len") != 22 or @offsetOf(DnsServiceStatus, "last_server") != 24 or @offsetOf(DnsServiceStatus, "last_answer") != 28 or @offsetOf(DnsServiceStatus, "cache_server") != 32 or @offsetOf(DnsServiceStatus, "cache_answer") != 36 or @offsetOf(DnsServiceStatus, "cache_age_seconds") != 40 or @offsetOf(DnsServiceStatus, "cache_ttl_seconds") != 44 or @offsetOf(DnsServiceStatus, "cache_remaining_seconds") != 48 or @offsetOf(DnsServiceStatus, "queries_tx") != 56 or @offsetOf(DnsServiceStatus, "resolve_requests") != 64 or @offsetOf(DnsServiceStatus, "responses_rx") != 72 or @offsetOf(DnsServiceStatus, "a_records") != 80 or @offsetOf(DnsServiceStatus, "timeouts") != 88 or @offsetOf(DnsServiceStatus, "nxdomain") != 96 or @offsetOf(DnsServiceStatus, "tx_errors") != 104 or @offsetOf(DnsServiceStatus, "malformed") != 112 or @offsetOf(DnsServiceStatus, "self_tests") != 120 or @offsetOf(DnsServiceStatus, "cache_hits") != 128 or @offsetOf(DnsServiceStatus, "cache_stores") != 136 or @offsetOf(DnsServiceStatus, "name") != 144 or @offsetOf(DnsServiceStatus, "pending_name") != 208 or @offsetOf(DnsServiceStatus, "cache_name") != 272 or @offsetOf(DnsServiceStatus, "last_error") != 336) @compileError("Native NIPC DnsServiceStatus layout");
}

comptime {
    if (@sizeOf(DnsServiceResult) != 256 or @alignOf(DnsServiceResult) != 8 or @offsetOf(DnsServiceResult, "magic") != 0 or @offsetOf(DnsServiceResult, "version") != 4 or @offsetOf(DnsServiceResult, "action") != 6 or @offsetOf(DnsServiceResult, "result") != 8 or @offsetOf(DnsServiceResult, "flags") != 12 or @offsetOf(DnsServiceResult, "answer") != 16 or @offsetOf(DnsServiceResult, "server") != 20 or @offsetOf(DnsServiceResult, "cache_answer") != 24 or @offsetOf(DnsServiceResult, "cache_age_seconds") != 28 or @offsetOf(DnsServiceResult, "cache_ttl_seconds") != 32 or @offsetOf(DnsServiceResult, "cache_remaining_seconds") != 36 or @offsetOf(DnsServiceResult, "queries_tx") != 40 or @offsetOf(DnsServiceResult, "resolve_requests") != 48 or @offsetOf(DnsServiceResult, "responses_rx") != 56 or @offsetOf(DnsServiceResult, "a_records") != 64 or @offsetOf(DnsServiceResult, "timeouts") != 72 or @offsetOf(DnsServiceResult, "nxdomain") != 80 or @offsetOf(DnsServiceResult, "tx_errors") != 88 or @offsetOf(DnsServiceResult, "malformed") != 96 or @offsetOf(DnsServiceResult, "cache_hits") != 104 or @offsetOf(DnsServiceResult, "cache_stores") != 112 or @offsetOf(DnsServiceResult, "last_id") != 120 or @offsetOf(DnsServiceResult, "name_len") != 122 or @offsetOf(DnsServiceResult, "name") != 124 or @offsetOf(DnsServiceResult, "last_error") != 220) @compileError("Native NIPC DnsServiceResult layout");
}

comptime {
    if (@sizeOf(TcpServiceStatus) != 384 or @alignOf(TcpServiceStatus) != 8 or @offsetOf(TcpServiceStatus, "magic") != 0 or @offsetOf(TcpServiceStatus, "version") != 4 or @offsetOf(TcpServiceStatus, "reserved") != 6 or @offsetOf(TcpServiceStatus, "flags") != 8 or @offsetOf(TcpServiceStatus, "request_owner_id") != 12 or @offsetOf(TcpServiceStatus, "owned_handles") != 14 or @offsetOf(TcpServiceStatus, "legacy_handles") != 16 or @offsetOf(TcpServiceStatus, "reserved3") != 18 or @offsetOf(TcpServiceStatus, "active_connections") != 20 or @offsetOf(TcpServiceStatus, "max_connections") != 24 or @offsetOf(TcpServiceStatus, "active_listeners") != 28 or @offsetOf(TcpServiceStatus, "handle_count") != 32 or @offsetOf(TcpServiceStatus, "max_handles") != 36 or @offsetOf(TcpServiceStatus, "tcp_buffer_size") != 40 or @offsetOf(TcpServiceStatus, "message_payload_max") != 44 or @offsetOf(TcpServiceStatus, "write_max") != 48 or @offsetOf(TcpServiceStatus, "read_max") != 52 or @offsetOf(TcpServiceStatus, "rx_segments") != 56 or @offsetOf(TcpServiceStatus, "tx_segments") != 64 or @offsetOf(TcpServiceStatus, "syn_tx") != 72 or @offsetOf(TcpServiceStatus, "synack_rx") != 80 or @offsetOf(TcpServiceStatus, "ack_tx") != 88 or @offsetOf(TcpServiceStatus, "data_tx") != 96 or @offsetOf(TcpServiceStatus, "data_rx") != 104 or @offsetOf(TcpServiceStatus, "fin_tx") != 112 or @offsetOf(TcpServiceStatus, "rst_rx") != 120 or @offsetOf(TcpServiceStatus, "checksum_errors") != 128 or @offsetOf(TcpServiceStatus, "timeouts") != 136 or @offsetOf(TcpServiceStatus, "self_tests") != 144 or @offsetOf(TcpServiceStatus, "synack_tx") != 152 or @offsetOf(TcpServiceStatus, "listen_syn_rx") != 160 or @offsetOf(TcpServiceStatus, "accepts") != 168 or @offsetOf(TcpServiceStatus, "retransmits") != 176 or @offsetOf(TcpServiceStatus, "rx_drops") != 184 or @offsetOf(TcpServiceStatus, "last_source_port") != 192 or @offsetOf(TcpServiceStatus, "last_dest_port") != 194 or @offsetOf(TcpServiceStatus, "last_flags") != 196 or @offsetOf(TcpServiceStatus, "reserved2") != 198 or @offsetOf(TcpServiceStatus, "last_seq") != 200 or @offsetOf(TcpServiceStatus, "last_ack") != 204 or @offsetOf(TcpServiceStatus, "last_payload_len") != 208 or @offsetOf(TcpServiceStatus, "owner_mismatches") != 216 or @offsetOf(TcpServiceStatus, "last_error") != 224 or @offsetOf(TcpServiceStatus, "stale_handles_reaped") != 256 or @offsetOf(TcpServiceStatus, "stale_tombstones") != 264 or @offsetOf(TcpServiceStatus, "last_lifecycle_cause") != 268 or @offsetOf(TcpServiceStatus, "lifecycle_closed") != 272 or @offsetOf(TcpServiceStatus, "lifecycle_reset") != 280 or @offsetOf(TcpServiceStatus, "lifecycle_timeout") != 288 or @offsetOf(TcpServiceStatus, "lifecycle_peer_gone") != 296 or @offsetOf(TcpServiceStatus, "lifecycle_local_abort") != 304 or @offsetOf(TcpServiceStatus, "lifecycle_local_close") != 312 or @offsetOf(TcpServiceStatus, "lifecycle_pending_close") != 320 or @offsetOf(TcpServiceStatus, "lifecycle_would_block") != 328 or @offsetOf(TcpServiceStatus, "lifecycle_bad_handle") != 336 or @offsetOf(TcpServiceStatus, "lifecycle_owner_mismatch") != 344 or @offsetOf(TcpServiceStatus, "read_wait_timeouts") != 352 or @offsetOf(TcpServiceStatus, "accept_wait_timeouts") != 360 or @offsetOf(TcpServiceStatus, "write_would_block") != 368 or @offsetOf(TcpServiceStatus, "close_cancelled") != 376) @compileError("Native NIPC TcpServiceStatus layout");
}

comptime {
    if (@sizeOf(TcpServiceResult) != 156 or @alignOf(TcpServiceResult) != 4 or @offsetOf(TcpServiceResult, "magic") != 0 or @offsetOf(TcpServiceResult, "version") != 4 or @offsetOf(TcpServiceResult, "action") != 6 or @offsetOf(TcpServiceResult, "result") != 8 or @offsetOf(TcpServiceResult, "flags") != 12 or @offsetOf(TcpServiceResult, "handle") != 16 or @offsetOf(TcpServiceResult, "conn_id") != 20 or @offsetOf(TcpServiceResult, "bytes") != 24 or @offsetOf(TcpServiceResult, "requested_bytes") != 28 or @offsetOf(TcpServiceResult, "port") != 32 or @offsetOf(TcpServiceResult, "remote_port") != 34 or @offsetOf(TcpServiceResult, "remote_ip") != 36 or @offsetOf(TcpServiceResult, "active_connections") != 40 or @offsetOf(TcpServiceResult, "max_connections") != 44 or @offsetOf(TcpServiceResult, "active_listeners") != 48 or @offsetOf(TcpServiceResult, "handle_count") != 52 or @offsetOf(TcpServiceResult, "max_handles") != 56 or @offsetOf(TcpServiceResult, "tcp_buffer_size") != 60 or @offsetOf(TcpServiceResult, "message_payload_max") != 64 or @offsetOf(TcpServiceResult, "write_max") != 68 or @offsetOf(TcpServiceResult, "read_max") != 72 or @offsetOf(TcpServiceResult, "pending_rx") != 76 or @offsetOf(TcpServiceResult, "rx_window") != 80 or @offsetOf(TcpServiceResult, "tx_window") != 84 or @offsetOf(TcpServiceResult, "tx_seq") != 88 or @offsetOf(TcpServiceResult, "tx_ack") != 92 or @offsetOf(TcpServiceResult, "retransmits") != 96 or @offsetOf(TcpServiceResult, "rx_drops") != 100 or @offsetOf(TcpServiceResult, "local_ip") != 104 or @offsetOf(TcpServiceResult, "local_port") != 108 or @offsetOf(TcpServiceResult, "reserved3") != 110 or @offsetOf(TcpServiceResult, "last_error") != 112 or @offsetOf(TcpServiceResult, "lifecycle_cause") != 144 or @offsetOf(TcpServiceResult, "service_status") != 148 or @offsetOf(TcpServiceResult, "owner_id") != 152 or @offsetOf(TcpServiceResult, "reserved4") != 154) @compileError("Native NIPC TcpServiceResult layout");
}

comptime {
    if (@sizeOf(UdpServiceStatus) != 152 or @alignOf(UdpServiceStatus) != 8 or @offsetOf(UdpServiceStatus, "magic") != 0 or @offsetOf(UdpServiceStatus, "version") != 4 or @offsetOf(UdpServiceStatus, "reserved") != 6 or @offsetOf(UdpServiceStatus, "flags") != 8 or @offsetOf(UdpServiceStatus, "active_sockets") != 12 or @offsetOf(UdpServiceStatus, "max_sockets") != 16 or @offsetOf(UdpServiceStatus, "queued_packets") != 20 or @offsetOf(UdpServiceStatus, "queue_limit") != 24 or @offsetOf(UdpServiceStatus, "payload_max") != 28 or @offsetOf(UdpServiceStatus, "message_payload_max") != 32 or @offsetOf(UdpServiceStatus, "send_max") != 36 or @offsetOf(UdpServiceStatus, "recv_max") != 40 or @offsetOf(UdpServiceStatus, "delivered") != 48 or @offsetOf(UdpServiceStatus, "drops") != 56 or @offsetOf(UdpServiceStatus, "last_error") != 64 or @offsetOf(UdpServiceStatus, "last_lifecycle_cause") != 96 or @offsetOf(UdpServiceStatus, "reserved2") != 100 or @offsetOf(UdpServiceStatus, "lifecycle_closed") != 104 or @offsetOf(UdpServiceStatus, "lifecycle_timeout") != 112 or @offsetOf(UdpServiceStatus, "lifecycle_local_close") != 120 or @offsetOf(UdpServiceStatus, "lifecycle_would_block") != 128 or @offsetOf(UdpServiceStatus, "lifecycle_bad_handle") != 136 or @offsetOf(UdpServiceStatus, "lifecycle_dropped") != 144) @compileError("Native NIPC UdpServiceStatus layout");
}

comptime {
    if (@sizeOf(UdpServiceResult) != 128 or @alignOf(UdpServiceResult) != 8 or @offsetOf(UdpServiceResult, "magic") != 0 or @offsetOf(UdpServiceResult, "version") != 4 or @offsetOf(UdpServiceResult, "action") != 6 or @offsetOf(UdpServiceResult, "result") != 8 or @offsetOf(UdpServiceResult, "flags") != 12 or @offsetOf(UdpServiceResult, "handle") != 16 or @offsetOf(UdpServiceResult, "bytes") != 20 or @offsetOf(UdpServiceResult, "requested_bytes") != 24 or @offsetOf(UdpServiceResult, "source_ip") != 28 or @offsetOf(UdpServiceResult, "dest_ip") != 32 or @offsetOf(UdpServiceResult, "source_port") != 36 or @offsetOf(UdpServiceResult, "dest_port") != 38 or @offsetOf(UdpServiceResult, "active_sockets") != 40 or @offsetOf(UdpServiceResult, "max_sockets") != 44 or @offsetOf(UdpServiceResult, "queued_packets") != 48 or @offsetOf(UdpServiceResult, "queue_limit") != 52 or @offsetOf(UdpServiceResult, "payload_max") != 56 or @offsetOf(UdpServiceResult, "message_payload_max") != 60 or @offsetOf(UdpServiceResult, "send_max") != 64 or @offsetOf(UdpServiceResult, "recv_max") != 68 or @offsetOf(UdpServiceResult, "delivered") != 72 or @offsetOf(UdpServiceResult, "drops") != 80 or @offsetOf(UdpServiceResult, "last_error") != 88 or @offsetOf(UdpServiceResult, "lifecycle_cause") != 120 or @offsetOf(UdpServiceResult, "service_status") != 124) @compileError("Native NIPC UdpServiceResult layout");
}

comptime {
    if (@sizeOf(RestartStatus) != 24 or @alignOf(RestartStatus) != 8 or @offsetOf(RestartStatus, "restarts") != 0 or @offsetOf(RestartStatus, "last_queued") != 8 or @offsetOf(RestartStatus, "last_handles") != 12 or @offsetOf(RestartStatus, "last_tombstones") != 16) @compileError("Native NIPC RestartStatus layout");
}

comptime {
    if (@sizeOf(ServiceBackpressureStatus) != 40 or @alignOf(ServiceBackpressureStatus) != 8 or @offsetOf(ServiceBackpressureStatus, "payload_too_large") != 0 or @offsetOf(ServiceBackpressureStatus, "response_buffer_small") != 8 or @offsetOf(ServiceBackpressureStatus, "send_failures") != 16 or @offsetOf(ServiceBackpressureStatus, "stale_skips") != 24 or @offsetOf(ServiceBackpressureStatus, "request_payload_limit") != 32 or @offsetOf(ServiceBackpressureStatus, "response_buffer_limit") != 36) @compileError("Native NIPC ServiceBackpressureStatus layout");
}

comptime {
    if (@sizeOf(ServiceLifecycleStatus) != 48 or @alignOf(ServiceLifecycleStatus) != 8 or @offsetOf(ServiceLifecycleStatus, "close_ok") != 0 or @offsetOf(ServiceLifecycleStatus, "close_failed") != 8 or @offsetOf(ServiceLifecycleStatus, "close_cancelled") != 16 or @offsetOf(ServiceLifecycleStatus, "cleanup_aborts") != 24 or @offsetOf(ServiceLifecycleStatus, "cleanup_poweroff") != 32 or @offsetOf(ServiceLifecycleStatus, "cleanup_reboot") != 40) @compileError("Native NIPC ServiceLifecycleStatus layout");
}

comptime {
    if (@sizeOf(SocketLifecycleCounters) != 96 or @alignOf(SocketLifecycleCounters) != 8 or @offsetOf(SocketLifecycleCounters, "last_cause") != 0 or @offsetOf(SocketLifecycleCounters, "closed") != 8 or @offsetOf(SocketLifecycleCounters, "reset") != 16 or @offsetOf(SocketLifecycleCounters, "timeout") != 24 or @offsetOf(SocketLifecycleCounters, "peer_gone") != 32 or @offsetOf(SocketLifecycleCounters, "local_abort") != 40 or @offsetOf(SocketLifecycleCounters, "local_close") != 48 or @offsetOf(SocketLifecycleCounters, "pending_close") != 56 or @offsetOf(SocketLifecycleCounters, "would_block") != 64 or @offsetOf(SocketLifecycleCounters, "bad_handle") != 72 or @offsetOf(SocketLifecycleCounters, "owner_mismatch") != 80 or @offsetOf(SocketLifecycleCounters, "dropped") != 88) @compileError("Native NIPC SocketLifecycleCounters layout");
}

comptime {
    if (ipc.CHANNEL_NET_DHCP != 2 or ipc.CHANNEL_NET_DNS != 3 or ipc.CHANNEL_NET_TCP != 4 or ipc.CHANNEL_NET_UDP != 5) @compileError("Native NIPC channel identities");
}
