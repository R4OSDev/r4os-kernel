const SocketText = @import("../driver/usb/core.zig").Text;
const NativeSocketSummary = extern struct {
    active: u32,
    queued: u32,
    delivered: u64,
    drops: u64,
};
extern fn r4os_rust_udp_sockets_reset() void;
extern fn r4os_rust_udp_socket_bind(u16, *SocketText) i32;
extern fn r4os_rust_udp_socket_close(u32, *SocketText) i32;
extern fn r4os_rust_udp_sockets_close_all() u32;
extern fn r4os_rust_udp_socket_summary(*NativeSocketSummary) void;
extern fn r4os_rust_udp_socket_bound(u16) bool;
extern fn r4os_rust_udp_socket_port(u32, *u16) bool;
extern fn r4os_rust_udp_socket_note_tx(u32) void;
extern fn r4os_rust_udp_socket_dispatch(*const UdpRecvInfo, [*]const u8, usize, *SocketText) bool;
extern fn r4os_rust_udp_socket_recv(u32, *UdpRecvInfo, [*]u8, usize, *SocketText) i32;
fn udpSocketSnapshot() NativeSocketSummary {
    var out: NativeSocketSummary = undefined;
    r4os_rust_udp_socket_summary(&out);
    return out;
}

const NativeBytesText = @import("../driver/usb/core.zig").Text;
// Native admission owns all transition/callback atomics; no mirrored gate.
extern fn r4os_rust_net_admission_reset() void;
extern fn r4os_rust_net_transition_begin() bool;
extern fn r4os_rust_net_mutation_begin() bool;
extern fn r4os_rust_net_mutation_end() void;
extern fn r4os_rust_net_mutation_quarantine() void;
extern fn r4os_rust_net_callback_enter() bool;
extern fn r4os_rust_net_callback_leave() void;
const native_tasks = @import("../sched/task.zig");
const r4x_api = @import("../program/r4x_api.zig");
const arp = @import("arp.zig");
const net_config = @import("config.zig");
const ethernet = @import("ethernet.zig");
const icmp = @import("icmp.zig");
const ipv4 = @import("ipv4.zig");
const udp = @import("udp.zig");
const dhcp = @import("dhcp.zig");
const dhcp_runtime = @import("dhcp_runtime.zig");
const dns = @import("dns.zig");
const tcp = @import("tcp.zig");
const tcp_runtime = @import("tcp_runtime.zig");
const serial_link = @import("serial_link.zig");
const timing = @import("timing.zig");
const backend_contract = @import("backend_contract.zig");
const rx_handoff = @import("rx_handoff.zig");
const boot_config = @import("../kernel/boot_config.zig");
const ipc = @import("../kernel/ipc.zig");
const bootlog = @import("../kernel/bootlog.zig");
const irq_router = @import("../kernel/irq_router.zig");
const protocol_api = @import("../kernel/protocol_api.zig");
const protocol_registry = @import("../protocol/registry.zig");
const r4p = @import("../program/r4p.zig");
const r4p_contract = @import("r4p_contract.zig");
const interrupts = @import("../arch/x86_64/interrupts.zig");
const owner_locks = @import("../memory/owner_locks.zig");
const pci_inventory = @import("../platform/pci_inventory.zig");
const time_core = @import("../platform/time.zig");
const kernel_config = @import("config");
const scheduler = @import("../sched/scheduler.zig");
const sync = @import("../sched/sync.zig");
const sched_task = @import("../sched/task.zig");
const k = @import("../kernel/log.zig");

pub const MAX_ADAPTERS: usize = 8;
pub const MAX_PACKET_SIZE: usize = 1536;
pub const RX_HANDOFF_QUEUE_SIZE: usize = rx_handoff.capacity;
pub const RX_HANDOFF_BATCH_BUDGET: usize = 32;
pub const PACKET_POOL_SIZE: usize = 16;
pub const APP_IPV4_QUEUE_SIZE: usize = 64;
pub const APP_IPV4_PAYLOAD_MAX: usize = MAX_PACKET_SIZE;
const APP_IPV4_BUCKET_COUNT: usize = 6;
const APP_IPV4_BUCKET_ICMP: usize = 0;
const APP_IPV4_BUCKET_UDP_DHCP: usize = 1;
const APP_IPV4_BUCKET_UDP_DNS: usize = 2;
const APP_IPV4_BUCKET_UDP_OTHER: usize = 3;
const APP_IPV4_BUCKET_TCP: usize = 4;
const APP_IPV4_BUCKET_OTHER: usize = 5;
const CLASS_NETWORK: u8 = 0x02;
const SUBCLASS_ETHERNET: u8 = 0x00;
const ARP_CACHE_TTL_TICKS: u64 = timing.DEFAULT_ARP_CACHE_TTL_TICKS;
const ARP_CACHE_ENTRIES: usize = 8;
const ARP_RESOLVE_TIMEOUT_TICKS: u64 = timing.DEFAULT_ARP_RESOLVE_TIMEOUT_TICKS;
const ARP_RESOLVE_POLL_ROUNDS: usize = 64;
const ARP_RESOLVE_MAX_LOOPS: usize = 200000;
const ARP_RESOLVE_ATTEMPTS: usize = 3;
const ARP_PENDING_QUEUE_LIMIT: u64 = 0;
const DHCP_TIMEOUT_TICKS: u64 = timing.DEFAULT_DHCP_TIMEOUT_TICKS;
const DHCP_POLL_ROUNDS: usize = 512;
const DHCP_MAX_LOOPS: usize = 200000;
const DHCP_MAX_ATTEMPTS: u8 = 3;
const DHCP_COORDINATOR_POLL_TICKS: u64 = timing.msToTicks(100);
const DHCP_RETRY_BASE_TICKS: u64 = timing.msToTicks(250);
const DHCP_RETRY_MAX_TICKS: u64 = timing.msToTicks(4000);
const DHCP_TASK_START_RETRY_TICKS: u64 = timing.msToTicks(1000);
const SYSTEM_TRANSITION_CALLBACK_DRAIN_TICKS: u64 = timing.msToTicks(1000);
const DNS_TIMEOUT_TICKS: u64 = timing.DEFAULT_DNS_TIMEOUT_TICKS;
const DNS_SOURCE_PORT_BASE: u16 = 49152;
const DNS_CACHE_ENTRIES: usize = 4;
const DNS_DEFAULT_CACHE_TTL_SECONDS: u32 = 60;
const DNS_NEGATIVE_CACHE_TTL_SECONDS: u32 = 30;
const UDP_SOCKET_MAX: usize = 8;
const UDP_SOCKET_QUEUE_SIZE: usize = 4;
const UDP_SOCKET_PAYLOAD_MAX: usize = 1472;
const TCP_LISTEN_TIMEOUT_TICKS: u64 = timing.DEFAULT_TCP_LISTEN_TIMEOUT_TICKS;
const TCP_RETRANSMIT_TIMEOUT_TICKS: u64 = timing.DEFAULT_TCP_RETRANSMIT_TIMEOUT_TICKS;
const TCP_TIME_WAIT_TICKS: u64 = timing.DEFAULT_TCP_TIME_WAIT_TICKS;
const SERVICE_OPERATION_TIMEOUT_TICKS: u64 = timing.DEFAULT_SERVICE_OPERATION_TIMEOUT_TICKS;
// 0.56.23: 2 -> 5 Retransmits mit RTO-Backoff (Befund 6.3).
const TCP_MAX_RETRANSMITS: u8 = 5;
// 0.56.23: adaptiver RTO - SRTT-Glaettung, geklemmt auf 50 ms..1,5 s.
const TCP_RTO_MIN_TICKS: u64 = timing.msToTicks(50);
const TCP_RTO_MAX_TICKS: u64 = timing.msToTicks(1_500);
pub const TCP_DELAYED_ACK_MS: u32 = 40;
const TCP_DELAYED_ACK_TICKS: u64 = @max(1, timing.msToTicks(TCP_DELAYED_ACK_MS));
// 0.56.23: 512 -> 32; seit net-rx-Task (0.56.2) und NIC-IRQ (0.56.21)
// ist das Poll hier nur noch Opportunismus, kein Antrieb.
const TCP_POLL_ROUNDS: usize = 32;
const DHCP_BROADCAST_IP: [4]u8 = .{ 255, 255, 255, 255 };
const DHCP_ZERO_IP: [4]u8 = .{ 0, 0, 0, 0 };
const DHCP_BROADCAST_MAC: [6]u8 = .{ 255, 255, 255, 255, 255, 255 };
pub const ADAPTER_FLAG_TRUSTED_BACKEND: u32 = 1 << 0;
pub const ADAPTER_FLAG_BROADCAST: u32 = 1 << 2;
pub const TcpSummary = r4x_api.TcpSummary;
pub const TcpConnectionInfo = r4x_api.TcpConnectionInfo;
pub const TCP_BUFFER_SIZE: usize = tcp.BUFFER_SIZE;
pub const TimingStatus = timing.Status;

pub const TcpPerformance = extern struct {
    local_mss: u32 = tcp.LOCAL_MSS,
    catalog_capacity: u32 = tcp.SENT_SEGMENT_CAPACITY,
    delayed_ack_ms: u32 = TCP_DELAYED_ACK_MS,
    local_window_scale: u32 = tcp.LOCAL_WINDOW_SCALE,
    outstanding_segments: u32 = 0,
    outstanding_bytes: u32 = 0,
    outstanding_segments_peak: u32 = 0,
    outstanding_bytes_peak: u32 = 0,
    write_calls: u64 = 0,
    write_requested_bytes: u64 = 0,
    write_completed_bytes: u64 = 0,
    write_segments: u64 = 0,
    write_partial: u64 = 0,
    remote_window_stalls: u64 = 0,
    catalog_stalls: u64 = 0,
    backend_busy_stalls: u64 = 0,
    pure_ack_tx: u64 = 0,
    delayed_ack_requests: u64 = 0,
    delayed_ack_tx: u64 = 0,
    immediate_ack_tx: u64 = 0,
    ack_coalesced: u64 = 0,
    ack_piggybacked: u64 = 0,
    window_update_tx: u64 = 0,
    adapter_poll_rounds: u64 = 0,
    service_poll_requests: u64 = 0,
    service_poll_skips: u64 = 0,
    retransmits: u64 = 0,
    mss_negotiated: u64 = 0,
    window_scale_negotiated: u64 = 0,
};

pub const RxAdmission = enum {
    accepted,
    accepted_fallback,
    invalid_adapter,
    invalid_frame,
    unavailable,
    queue_busy,
    irq_context,
};

pub const ProtocolKind = enum {
    ethernet,
    arp,
    ipv4,
    icmp,
    udp,
    dhcp,
    dns,
    tcp,
    serial_link,
};

pub const FALLBACK_POLICY_NONE: u8 = 0;
pub const FALLBACK_DECISION_NONE: u8 = 0;

pub const ProtocolRuntimeStats = extern struct {
    active_r4p: bool = false,
    r4p_state: u8 = r4p.ROLE_STATE_MISSING,
    builtin_fallback: bool = false,
    fallback_policy: u8 = 0,
    fallback_decision: u8 = 0,
    r4p_rx: u64 = 0,
    r4p_tx: u64 = 0,
    r4p_control: u64 = 0,
    r4p_build: u64 = 0,
    r4p_classify: u64 = 0,
    dispatch_failures: u64 = 0,
};

pub const R4pRuntimeStatus = extern struct {
    protocol_count: u32 = 0,
    active: u32 = 0,
    missing: u32 = 0,
    r4p_rx: u64 = 0,
    r4p_tx: u64 = 0,
    r4p_control: u64 = 0,
    r4p_build: u64 = 0,
    r4p_classify: u64 = 0,
    dispatch_failures: u64 = 0,
};

pub const DriverLifecycleStatus = struct {
    tests: u64 = 0,
    cases: u64 = 0,
};

pub const DiagnosticCountersStatus = struct {
    packet_corpus_tests: u64 = 0,
    packet_corpus_cases: u64 = 0,
    negative_path_tests: u64 = 0,
    negative_path_cases: u64 = 0,
    environment_contract_tests: u64 = 0,
    environment_contract_cases: u64 = 0,
    limit_contract_tests: u64 = 0,
    limit_contract_cases: u64 = 0,
    power_lifecycle_tests: u64 = 0,
    power_lifecycle_cases: u64 = 0,
};

const R4P_RUNTIME_PROTOCOLS = [_]ProtocolKind{
    .ethernet,
    .arp,
    .ipv4,
    .icmp,
    .udp,
    .dhcp,
    .dns,
    .tcp,
    .serial_link,
};

pub const DhcpLeaseTiming = extern struct {
    bound: bool = false,
    acquired_tick: u64 = 0,
    elapsed_seconds: u32 = 0,
    remaining_seconds: u32 = 0,
    renew_in_seconds: u32 = 0,
    rebind_in_seconds: u32 = 0,
};

pub const DhcpRuntimeStatus = extern struct {
    state: dhcp_runtime.State = .disabled,
    desired_dhcp: bool = false,
    link_up: bool = false,
    task_started: bool = false,
    task_id: u32 = 0,
    task_generation: u64 = 0,
    link_generation: u32 = 0,
    operation_generation: u32 = 0,
    transition_tick: u64 = 0,
    next_retry_tick: u64 = 0,
    last_timeout_tick: u64 = 0,
    retry_round: u8 = 0,
    operation_active: bool = false,
    recoveries: u64 = 0,
    starts: u64 = 0,
    cancels: u64 = 0,
};

pub const DnsCacheStatus = dns.CacheStatus;

const Ipv4RouteDecision = struct {
    result: TxResult = .ok,
    next_hop_ip: [4]u8 = .{0} ** 4,
    dest_mac: [6]u8 = .{0} ** 6,
    needs_arp: bool = true,
    last_error: []const u8 = "none",
};

pub const UdpRecvInfo = extern struct {
    source_ip: [4]u8 = .{0} ** 4,
    dest_ip: [4]u8 = .{0} ** 4,
    source_port: u16 = 0,
    dest_port: u16 = 0,
    length: u16 = 0,
};

pub const UdpStatus = struct {
    active_sockets: u32 = 0,
    max_sockets: u32 = UDP_SOCKET_MAX,
    queued_packets: u32 = 0,
    queue_limit: u32 = UDP_SOCKET_QUEUE_SIZE,
    payload_max: u32 = UDP_SOCKET_PAYLOAD_MAX,
    delivered: u64 = 0,
    drops: u64 = 0,
    last_error: []const u8 = "none",
};

pub const BackpressureStatus = struct {
    packet_pool_used: u32 = 0,
    packet_pool_limit: u32 = PACKET_POOL_SIZE,
    packet_drops: u64 = 0,
    app_ipv4_queued: u32 = 0,
    app_ipv4_queue_limit: u32 = APP_IPV4_QUEUE_SIZE * APP_IPV4_BUCKET_COUNT,
    app_ipv4_drops: u64 = 0,
    udp_active_sockets: u32 = 0,
    udp_socket_limit: u32 = UDP_SOCKET_MAX,
    udp_queued_packets: u32 = 0,
    udp_queue_limit_per_socket: u32 = UDP_SOCKET_QUEUE_SIZE,
    udp_queue_limit_total: u32 = UDP_SOCKET_MAX * UDP_SOCKET_QUEUE_SIZE,
    udp_drops: u64 = 0,
    udp_payload_max: u32 = UDP_SOCKET_PAYLOAD_MAX,
    tcp_active_connections: u32 = 0,
    tcp_connection_limit: u32 = tcp.MAX_CONNECTIONS,
    tcp_active_listeners: u32 = 0,
    tcp_listener_limit: u32 = tcp.MAX_LISTENERS,
    tcp_buffer_size: u32 = tcp.BUFFER_SIZE,
    tcp_rx_drops: u64 = 0,
    ipc_service_channels: u32 = 0,
    ipc_service_handlers: u32 = 0,
    ipc_service_queued: u32 = 0,
    ipc_service_queue_limit: u32 = 0,
    ipc_service_drops: u64 = 0,
    ipc_service_message_max: u32 = 0,
    ipc_service_queue_depth: u32 = 0,
    tx_failures: u64 = 0,
    tx_no_adapter: u64 = 0,
    tx_link_down: u64 = 0,
    tx_busy: u64 = 0,
    tx_too_large: u64 = 0,
    tx_unsupported: u64 = 0,
    tx_backend_error: u64 = 0,
    tx_last_result: []const u8 = "none",
    nonblocking_empty_status: []const u8 = "would-block",
    resource_queue_full: u64 = 0,
    resource_packet_drops: u64 = 0,
    resource_buffer_small: u64 = 0,
    resource_retries: u64 = 0,
    resource_timeouts: u64 = 0,
    resource_cancels: u64 = 0,
    resource_backend_busy: u64 = 0,
};

pub const CleanupStatus = struct {
    runs: u64 = 0,
    link_down_cleanups: u64 = 0,
    adapter_reset_cleanups: u64 = 0,
    adapter_unregister_cleanups: u64 = 0,
    service_restart_cleanups: u64 = 0,
    poweroff_cleanups: u64 = 0,
    reboot_cleanups: u64 = 0,
    udp_sockets_closed: u64 = 0,
    tcp_connections_aborted: u64 = 0,
    tcp_listeners_closed: u64 = 0,
    dhcp_operations_cancelled: u64 = 0,
    dns_operations_cancelled: u64 = 0,
    last_udp_closed: u32 = 0,
    last_tcp_connections: u32 = 0,
    last_tcp_listeners: u32 = 0,
    last_reason: []const u8 = "none",
};

// NETDIAG runs on a live system. Its probes may account normal cleanup
// reasons, but may only retire the exact resources they created themselves.
const DiagnosticCleanupScope = extern struct {
    udp_handle: u32 = 0,
    tcp_connection_id: u32 = 0,
    tcp_listener_port: u16 = 0,
};

pub const ErrorStatus = struct {
    total: u64 = 0,
    packet_errors: u64 = 0,
    service_errors: u64 = 0,
    adapter_errors: u64 = 0,
    tx_failures: u64 = 0,
    protocol_errors: u64 = 0,
    r4p_dispatch_failures: u64 = 0,
    last_adapter_error: []const u8 = "none",
    last_protocol_error: []const u8 = "none",
};

const TxBackpressureCounters = struct {
    failures: u64 = 0,
    no_adapter: u64 = 0,
    link_down: u64 = 0,
    busy: u64 = 0,
    too_large: u64 = 0,
    unsupported: u64 = 0,
    backend_error: u64 = 0,
    last_result: TxResult = .ok,
    has_result: bool = false,
};

pub const Bus = enum {
    unknown,
    pci,
    pcie,
    serial,
};

pub const Link = enum {
    unknown,
    down,
    up,
};

pub const Lifecycle = enum {
    unknown,
    registered,
    active,
    link_down,
    resetting,
    shutdown,
};

pub const Stats = struct {
    rx_packets: u64 = 0,
    tx_packets: u64 = 0,
    rx_bytes: u64 = 0,
    tx_bytes: u64 = 0,
    drops: u64 = 0,
    errors: u64 = 0,
    resets: u64 = 0,
    last_error: []const u8 = "none",
};

pub const TxResult = enum {
    ok,
    no_adapter,
    link_down,
    busy,
    too_large,
    unsupported,
    backend_error,
};

pub const AdapterOps = struct {
    transmit: ?*const fn (adapter_index: usize, frame: []const u8) TxResult = null,
    poll: ?*const fn (adapter_index: usize) void = null,
    status: ?*const fn (adapter_index: usize, out: *BackendStatus) i32 = null,
};

pub const BackendStatus = extern struct {
    link_up: u32 = 0,
    rx_packets: u64 = 0,
    tx_packets: u64 = 0,
    drops: u64 = 0,
    errors: u64 = 0,
    irq_line: u8 = 0xFF,
    irq_pin: u8 = 0,
    irq_registered: u8 = 0,
    irq_mode: u8 = 0,
    irq_count: u64 = 0,
    irq_handled: u64 = 0,
    poll_count: u64 = 0,
    poll_fallbacks: u64 = 0,
    last_isr: u16 = 0,
    reserved: u16 = 0,
    // 0.56.7: Fehleraufschluesselung fuer die NETRX-Diagnose. Der
    // Sammelzaehler `errors` (rx+tx+ovw) hat die Fehlerklasse verdeckt;
    // Erweiterung am Struct-Ende, Defaults halten Treiber ohne
    // Aufschluesselung kompatibel. Spiegel: abi.NetBackendStatus.
    rx_errors: u64 = 0,
    tx_errors: u64 = 0,
    rx_overflows: u64 = 0,
    rx_recoveries: u64 = 0,
    offered_capabilities: u64 = 0,
    accepted_capabilities: u64 = 0,
    rx_offload_packets: u64 = 0,
    rx_software_fallbacks: u64 = 0,
    rx_metadata_errors: u64 = 0,
};

pub const Adapter = struct {
    // Stable during this boot, including when the enumeration array compacts.
    registration_id: u32 = 0,
    name: []const u8 = "",
    driver: []const u8 = "",
    bus: Bus = .unknown,
    bus_no: u8 = 0,
    device_no: u8 = 0,
    function_no: u8 = 0,
    vendor_id: u16 = 0,
    device_id: u16 = 0,
    mac: [6]u8 = .{0} ** 6,
    mtu: u16 = 1500,
    flags: u32 = ADAPTER_FLAG_TRUSTED_BACKEND | ADAPTER_FLAG_BROADCAST,
    link: Link = .unknown,
    lifecycle: Lifecycle = .registered,
    registered_tick: u64 = 0,
    state_changed_tick: u64 = 0,
    stats: Stats = .{},
    backend: backend_contract.Negotiation = .{},
    ops: AdapterOps = .{},
};

pub const Packet = extern struct {
    adapter_index: usize = 0,
    len: usize = 0,
    flags: u32 = 0,
    data: [MAX_PACKET_SIZE]u8 = .{0} ** MAX_PACKET_SIZE,
};

pub const AppIpv4Packet = extern struct {
    source_ip: [4]u8 = .{0} ** 4,
    dest_ip: [4]u8 = .{0} ** 4,
    protocol: u8 = 0,
    source_port: u16 = 0,
    dest_port: u16 = 0,
    truncated: bool = false,
    payload_len: u32 = 0,
};

const DhcpExpectedResponse = enum {
    none,
    offer,
    ack_or_nak,
};

pub fn init() void {
    r4os_rust_net_admission_reset();
    r4os_rust_net_packet_pool_reset();
    r4os_rust_net_cache_reset();
    dhcp.resetMessages();
    r4os_rust_app_ipv4_reset();
    r4os_rust_udp_sockets_reset();
    r4os_rust_net_adapters_reset();
    r4os_rust_net_protocol_runtime_reset();

    r4os_rust_net_rx_runtime_reset();
    r4os_rust_frames_reset(&frame_peers);
    r4os_rust_net_protocol_stats_reset();
    net_config.reset();

    r4os_rust_dhcp_client_reset(&dhcp_peers);
    r4os_rust_dns_client_reset(&dns_peers);
    tcp.reset();
    r4os_rust_tcp_client_reset(&tcp_client_peers);
    r4os_rust_tcp_rto_reset(true);

    r4os_rust_net_tx_reset();
    r4os_rust_net_cleanup_reset();
}

pub fn configureDhcpTestInjection(config: *const boot_config.Config) void {
    dhcp.configureTestInjection(boot_config.optionValue(config, "NET", "test_drop_offer"), boot_config.optionValue(config, "NET", "test_drop_ack"));
}

pub fn register(adapter: Adapter) ?usize {
    const value = toNativeAdapter(adapter);
    const result = r4os_rust_net_adapter_register(&value);
    return if (result == ~@as(usize, 0)) null else result;
}

/// Registration IDs are never recycled within a boot; zero is not a handle.
/// Callers resolving a driver reference hold a backend callback admission.
pub fn indexForRegistration(registration: u32) ?usize {
    const result = r4os_rust_net_adapter_index(registration);
    return if (result == ~@as(usize, 0)) null else result;
}

pub fn unregister(index: usize) bool {
    return unregisterInternal(index, null);
}

fn unregisterInternal(index: usize, diagnostic_scope: ?DiagnosticCleanupScope) bool {
    var scope = diagnostic_scope orelse DiagnosticCleanupScope{};
    return r4os_rust_net_adapter_runtime_unregister(index, if (diagnostic_scope != null) &scope else null);
}

pub fn resetAdapterLifecycle(index: usize, reason: []const u8) bool {
    return resetAdapterLifecycleInternal(index, reason, null);
}

fn resetAdapterLifecycleInternal(index: usize, reason: []const u8, diagnostic_scope: ?DiagnosticCleanupScope) bool {
    var scope = diagnostic_scope orelse DiagnosticCleanupScope{};
    return r4os_rust_net_adapter_runtime_reset(index, adapterText(reason), if (diagnostic_scope != null) &scope else null);
}

pub fn cleanupNetworkOperations(reason: []const u8) CleanupStatus {
    var value: NativeCleanupStatus = undefined;
    r4os_rust_net_cleanup_all(adapterText(reason), &value);
    return cleanupFromNative(value);
}

pub fn beginSystemTransition(reason: []const u8) bool {
    return r4os_rust_net_cleanup_transition(adapterText(reason));
}

pub fn beginBackendMutation() bool {
    if (!r4os_rust_net_mutation_begin()) return false;
    _ = cancelRxHandoffQueue();
    return true;
}

pub fn endBackendMutation() void {
    r4os_rust_net_mutation_end();
}

pub fn quarantineBackendMutation() void {
    r4os_rust_net_mutation_quarantine();
}

pub fn enterBackendCallback() bool {
    return r4os_rust_net_callback_enter();
}

pub fn leaveBackendCallback() void {
    r4os_rust_net_callback_leave();
}

pub fn cancelDhcpForStaticConfig() void {
    _ = cancelDhcpOperation("static-config");
}

pub fn cleanupStatus() CleanupStatus {
    var value: NativeCleanupStatus = undefined;
    r4os_rust_net_cleanup_status(&value);
    return cleanupFromNative(value);
}

pub fn runCleanupProbe() bool {
    return r4os_rust_net_diag_cleanup();
}

pub fn runPowerLifecycleProbe() bool {
    return r4os_rust_net_diag_power();
}

pub fn runLinkLifecycleProbe() bool {
    return r4os_rust_net_diag_link();
}

pub fn runAdapterResetProbe() bool {
    return r4os_rust_net_diag_reset();
}

pub fn runDriverLifecycleProbe() bool {
    return r4os_rust_net_diag_driver();
}

pub fn driverLifecycleStatus() DriverLifecycleStatus {
    const counters = nativeDiagnosticCounters();
    return .{
        .tests = counters.driver_lifecycle_tests,
        .cases = counters.driver_lifecycle_cases,
    };
}

pub fn runRxHandoffProbe() bool {
    return r4os_rust_net_diag_rx();
}

pub fn runBackendCapabilityProbe() bool {
    return r4os_rust_net_diag_capability();
}

pub fn diagnosticCountersStatus() DiagnosticCountersStatus {
    const counters = nativeDiagnosticCounters();
    return .{
        .packet_corpus_tests = counters.packet_corpus_tests,
        .packet_corpus_cases = counters.packet_corpus_cases,
        .negative_path_tests = counters.negative_path_tests,
        .negative_path_cases = counters.negative_path_cases,
        .environment_contract_tests = counters.environment_contract_tests,
        .environment_contract_cases = counters.environment_contract_cases,
        .limit_contract_tests = counters.limit_contract_tests,
        .limit_contract_cases = counters.limit_contract_cases,
        .power_lifecycle_tests = counters.power_lifecycle_tests,
        .power_lifecycle_cases = counters.power_lifecycle_cases,
    };
}

pub fn runEnvironmentContractProbe() bool {
    return r4os_rust_net_diag_environment();
}

fn cancelDhcpOperation(reason: []const u8) bool {
    return r4os_rust_dhcp_client_cancel(.{ .pointer = reason.ptr, .len = reason.len });
}

pub fn count() usize {
    return r4os_rust_net_adapter_count();
}

pub fn get(index: usize) ?Adapter {
    var value: NativeAdapter = undefined;
    return if (r4os_rust_net_adapter_get(index, &value)) fromNativeAdapter(value) else null;
}

pub fn refreshAdapterRuntime(index: usize) ?BackendStatus {
    return refreshAdapterRuntimeInternal(index, null);
}

fn refreshAdapterRuntimeInternal(index: usize, diagnostic_scope: ?DiagnosticCleanupScope) ?BackendStatus {
    var scope = diagnostic_scope orelse DiagnosticCleanupScope{};
    var out: BackendStatus = undefined;
    return if (r4os_rust_net_adapter_runtime_refresh(index, if (diagnostic_scope != null) &scope else null, &out)) out else null;
}

pub fn hasAdapterForDevice(vendor_id: u16, device_id: u16) bool {
    return r4os_rust_net_adapter_has_device(vendor_id, device_id);
}

pub fn allocPacket() ?usize {
    const handle = r4os_rust_net_packet_alloc();
    return if (handle == @import("std").math.maxInt(usize)) null else handle;
}

pub fn freePacket(handle: usize) void {
    r4os_rust_net_packet_free(handle);
}

pub fn packet(handle: usize) ?*Packet {
    return r4os_rust_net_packet(handle);
}

pub fn packetConst(handle: usize) ?*const Packet {
    return r4os_rust_net_packet(handle);
}

pub fn buildEthernetDiagFrame(out: []u8, src_mac: [6]u8) ?[]u8 {
    const params = WireEthernetBuild{ .source_mac = src_mac };
    const length = r4os_rust_packet_build_ethernet(&params, out.ptr, out.len);
    return if (length == ~@as(usize, 0)) null else out[0..length];
}

// 0.56.23: Verlust-Harness (NUR unter -Dnet-loss-test aktiv): jedes
// N-te empfangene Paket wird verworfen; TCP muss per Retransmit
// durchkommen. Nie im Default-Build wirksam.
// 0.56.23: jedes 64. RX-Frame (~1,5%) - der aktuelle Retransmit sendet
// nur das jeweils letzte Segment neu (kein Sliding-Window), das traegt
// gelegentlichen Einzelverlust; 6% ueberforderten den 64-KB-FTP-Transfer.
const NET_LOSS_EVERY_N: u64 = 64;

/// Announces device-owned RX work without touching a packet buffer. This is
/// the only network operation intended for an R4D IRQ handler: the handler
/// acknowledges its cause, records one adapter bit and wakes `net-rx`.
pub fn scheduleRxWork(adapter_index: usize) bool {
    return r4os_rust_net_rx_runtime_schedule(adapter_index);
}

/// Copies a driver-owned frame into the bounded common queue. Protocol work
/// never runs here. IRQ callers are rejected so the device descriptor can be
/// retained and retried by the task-side poll instead of entering R4P, socket
/// wakeups or reply transmission from an external interrupt.
pub fn receiveFrame(adapter_index: usize, frame: []const u8) RxAdmission {
    return receiveFrameInternal(adapter_index, 0, 0, frame, false);
}

/// Versioned packet intake. The canonical flat bytes always take the same
/// bounded copy path as `receiveFrame`; metadata can only remove the one
/// negotiated L4 checksum pass. Unknown or rejected metadata returns
/// `accepted_fallback` after copying the bytes unchanged.
pub fn receivePacket(adapter_index: usize, queue_index: u16, packet_flags: u64, frame: []const u8) RxAdmission {
    return receiveFrameInternal(adapter_index, queue_index, packet_flags, frame, true);
}

fn receiveFrameInternal(adapter_index: usize, queue_index: u16, packet_flags: u64, frame: []const u8, metadata_api: bool) RxAdmission {
    return @enumFromInt(r4os_rust_net_rx_runtime_receive(adapter_index, queue_index, packet_flags, frame.ptr, frame.len, metadata_api));
}

fn processReceivedFrame(adapter_index: usize, frame: []const u8, metadata: rx_handoff.Metadata) bool {
    return r4os_rust_frames_receive(adapter_index, frame.ptr, frame.len, &metadata);
}

fn cancelRxHandoffQueue() usize {
    return r4os_rust_net_rx_runtime_cancel();
}

pub fn transmit(adapter_index: usize, frame: []const u8) TxResult {
    return @enumFromInt(r4os_rust_frames_transmit(adapter_index, frame.ptr, frame.len));
}

pub fn candidateCount() usize {
    return r4os_rust_net_telemetry_candidates();
}

pub fn udpBind(port: u16) i32 {
    return r4os_rust_frames_udp_bind(port);
}

pub fn udpClose(handle: u32) i32 {
    return r4os_rust_frames_udp_close(handle);
}

pub fn udpSendTo(handle: u32, dest_ip: [4]u8, dest_port: u16, payload: []const u8) TxResult {
    return @enumFromInt(r4os_rust_frames_udp_send(handle, &dest_ip, dest_port, payload.ptr, payload.len));
}

fn udpSendDhcpFromAdapter(handle: u32, adapter_index: usize, source_ip: [4]u8, dest_ip: [4]u8, payload: []const u8) TxResult {
    return @enumFromInt(r4os_rust_frames_udp_dhcp(handle, adapter_index, &source_ip, &dest_ip, payload.ptr, payload.len));
}

pub fn udpRecvFrom(handle: u32, out_info: *UdpRecvInfo, out_payload: []u8) i32 {
    return r4os_rust_frames_udp_recv(handle, out_info, out_payload.ptr, out_payload.len);
}

pub fn udpRecvFromWait(handle: u32, out_info: *UdpRecvInfo, out_payload: []u8, timeout_ticks: u64) i32 {
    return r4os_rust_udp_recv_wait(handle, out_info, out_payload.ptr, out_payload.len, timeout_ticks, &udp_wait_peers);
}

pub fn udpStatus(out: *UdpStatus) void {
    var value: NativeUdpStatus = undefined;
    r4os_rust_net_telemetry_udp(&value);
    out.* = UdpFromNative(value);
}

pub fn udpSocketActiveCount() u32 {
    return udpSocketSnapshot().active;
}

pub fn udpSocketQueuedCount() u32 {
    return udpSocketSnapshot().queued;
}

pub fn udpSocketDropCount() u64 {
    return udpSocketSnapshot().drops;
}

pub fn udpSocketCapacity() u32 {
    return UDP_SOCKET_MAX;
}

pub fn packetDropCount() u64 {
    return r4os_rust_net_packet_drops();
}

pub fn packetFreeCount() usize {
    return r4os_rust_net_packet_free_count();
}

pub fn readAppIpv4(protocol: u8, out: *AppIpv4Packet, payload_out: []u8) i32 {
    return r4os_rust_app_ipv4_read(protocol, out, payload_out.ptr, payload_out.len);
}

pub fn busName(bus: Bus) []const u8 {
    return switch (bus) {
        .unknown => "unknown",
        .pci => "pci",
        .pcie => "pcie",
        .serial => "serial",
    };
}

pub fn linkName(link: Link) []const u8 {
    return switch (link) {
        .unknown => "unknown",
        .down => "down",
        .up => "up",
    };
}

pub fn lifecycleName(lifecycle: Lifecycle) []const u8 {
    return switch (lifecycle) {
        .unknown => "unknown",
        .registered => "registered",
        .active => "active",
        .link_down => "link-down",
        .resetting => "resetting",
        .shutdown => "shutdown",
    };
}

pub fn txResultName(result: TxResult) []const u8 {
    return switch (result) {
        .ok => "ok",
        .no_adapter => "no-adapter",
        .link_down => "link-down",
        .busy => "busy",
        .too_large => "too-large",
        .unsupported => "unsupported",
        .backend_error => "backend-error",
    };
}

pub fn ethernetStats() ethernet.Stats {
    return ethernet.ownedStats();
}

pub fn arpStats() arp.Stats {
    return arp.ownedStats(true);
}

pub fn ipv4Stats() ipv4.Stats {
    return ipv4.ownedStats();
}

pub fn icmpStats() icmp.Stats {
    return icmp.ownedStats();
}

pub fn udpStats() udp.Stats {
    return udp.ownedStats();
}

pub fn dhcpStats() dhcp.Stats {
    return dhcp.ownedStats();
}

pub fn dhcpRuntimeStatus() DhcpRuntimeStatus {
    var out: DhcpRuntimeStatus = undefined;
    r4os_rust_dhcp_client_runtime(&out);
    return out;
}

pub fn dhcpRuntimeStateName(state: dhcp_runtime.State) []const u8 {
    return dhcp_runtime.Coordinator.stateName(state);
}

pub fn dhcpLeaseTiming() DhcpLeaseTiming {
    var out: DhcpLeaseTiming = undefined;
    r4os_rust_dhcp_client_lease_timing(&out);
    return out;
}

pub fn dnsStats() dns.Stats {
    return dns.ownedStats(true);
}

pub fn dnsCacheStatus() DnsCacheStatus {
    return dns.ownedCacheStatus();
}

pub fn dnsCacheEntryCount() u32 {
    return r4os_rust_dns_client_cache_count(false);
}

pub fn dnsCacheNegativeCount() u32 {
    return r4os_rust_dns_client_cache_count(true);
}

pub fn backpressureStatus() BackpressureStatus {
    var out: NativeBackpressureStatus = undefined;
    r4os_rust_net_telemetry_backpressure(&out);
    return BackpressureFromNative(out);
}

const IpcServiceBackpressure = struct {
    channels: u32 = 0,
    handlers: u32 = 0,
    queued: u32 = 0,
    queue_limit: u32 = 0,
    drops: u64 = 0,
};

pub fn errorStatus() ErrorStatus {
    var out: NativeErrorStatus = undefined;
    r4os_rust_net_telemetry_errors(&out);
    return ErrorFromNative(out);
}

pub fn logErrorStatus(reason: []const u8) void {
    r4os_rust_net_telemetry_log_errors(adapterText(reason));
}

pub fn runBackpressureProbe() bool {
    return r4os_rust_net_diag_backpressure();
}

pub fn runLimitContractProbe() bool {
    return r4os_rust_net_diag_limit();
}

pub fn runTcpPerformanceContractProbe() bool {
    return tcp.performanceContractProbe();
}

pub fn timingStatus() TimingStatus {
    return timing.status();
}

pub fn runTimingProbe() bool {
    return r4os_rust_net_diag_timing();
}

pub fn runPacketCorpusProbe() bool {
    return r4os_rust_net_diag_corpus();
}

pub fn runNegativePathProbe() bool {
    return r4os_rust_net_diag_negative();
}

pub fn tcpStats() tcp.Stats {
    return tcp.getStats();
}

pub fn tcpPerformance(out: *TcpPerformance) void {
    r4os_rust_net_telemetry_tcp_performance(out);
}

pub fn arpCacheAgeTicks() u64 {
    return r4os_rust_net_arp_cache_age_owned();
}

pub fn arpCacheTtlTicks() u64 {
    return ARP_CACHE_TTL_TICKS;
}

pub fn protocolRuntimeStats(kind: ProtocolKind) ProtocolRuntimeStats {
    var value: ProtocolRuntimeStats = undefined;
    r4os_rust_net_protocol_runtime(@intFromEnum(kind), &value);
    return value;
}

pub fn r4pRuntimeStatus() R4pRuntimeStatus {
    var value: R4pRuntimeStatus = undefined;
    r4os_rust_net_r4p_runtime(&value);
    return value;
}

pub fn runR4pRuntimeProbe() bool {
    return r4os_rust_net_diag_r4p();
}

// Exactly one task may move a device RX cursor. Drivers now only copy into the
// common queue; R4P and socket wakeups are run later by net-rx. A protocol
// handler may still poll recursively while resolving ARP. In that case one
// queued frame is processed from another ownership slot, preserving the old
// forward-progress guarantee without reusing the outer frame buffer.
const RX_NESTED_PROCESS_LIMIT: u8 = 4;
const ALL_ADAPTER_MASK: u16 = (@as(u16, 1) << MAX_ADAPTERS) - 1;

pub fn pollAdapters(rounds: usize) void {
    pollAdapterMask(rounds, ALL_ADAPTER_MASK);
}

fn pollAdaptersWithoutRxTask(rounds: usize) void {
    r4os_rust_net_rx_runtime_poll_without_task(rounds, &rx_peers);
}

fn pollAdapterMask(rounds: usize, adapter_mask: u16) void {
    r4os_rust_net_rx_runtime_poll(rounds, adapter_mask, &rx_peers);
}

// 0.56.2: Autonomer Hintergrund-RX-Task. Der heutige Netzstack nimmt
// eingehende Frames nur bei pollAdapters-Gelegenheiten an (NIC-IRQ wird
// nicht zugestellt, Befund 6.1/0.56.1-Baseline) - ohne diesen Task ist
// eingehender Verkehr im Idle nichtdeterministisch. Start aus main.zig
// NACH initTaskRuntime (task.init() wischt fruehere Threads).
const RX_TASK_POLL_ROUNDS: usize = 1;
const RX_TASK_FALLBACK_TICKS: u64 = @max(1, timing.msToTicks(10));

pub const RxTaskSummary = extern struct {
    started: bool = false,
    task_id: u32 = 0,
    task_generation: u64 = 0,
    polls: u64 = 0,
    iterations: u64 = 0,
    guard_skips: u64 = 0,
    nested_polls: u64 = 0,
    queue_capacity: u32 = RX_HANDOFF_QUEUE_SIZE,
    queue_ready: u32 = 0,
    queue_occupied: u32 = 0,
    queue_high_water: u32 = 0,
    accepted: u64 = 0,
    processed: u64 = 0,
    cancelled: u64 = 0,
    queue_busy: u64 = 0,
    protocol_success: u64 = 0,
    protocol_failures: u64 = 0,
    schedules: u64 = 0,
    schedules_from_irq: u64 = 0,
    schedule_coalesced: u64 = 0,
    event_wakeups: u64 = 0,
    fallback_timeouts: u64 = 0,
    batches: u64 = 0,
    batch_max: u32 = 0,
    budget_exhaustions: u64 = 0,
    handoff_latency_samples: u64 = 0,
    handoff_latency_last_ns: u64 = 0,
    handoff_latency_max_ns: u64 = 0,
    schedule_latency_samples: u64 = 0,
    schedule_latency_last_ns: u64 = 0,
    schedule_latency_max_ns: u64 = 0,
    irq_frame_rejects: u64 = 0,
    release_failures: u64 = 0,
    metadata_submissions: u64 = 0,
    metadata_selected: u64 = 0,
    metadata_fallbacks: u64 = 0,
    l4_offload_used: u64 = 0,
    l4_software_checks: u64 = 0,
    ownership_balanced: bool = false,
};

pub fn rxTaskSummary() RxTaskSummary {
    var value: RxTaskSummary = undefined;
    r4os_rust_net_rx_runtime_summary(&value);
    return value;
}

pub fn startRxTask() bool {
    return r4os_rust_net_rx_runtime_start(&rx_peers);
}

pub fn startDhcpTask() bool {
    return r4os_rust_dhcp_client_start();
}

// Diagnose-Zeile alle ~20 s auf COM1 (0.56.2): macht die komplette
// RX-Kette im Zeitverlauf sichtbar (Task -> Guard -> NIC -> Kernel-TCP),
// ohne Gast-Programme oder ABI-Aenderungen zu brauchen.
const RX_TASK_LOG_INTERVAL: u64 = 2000;

fn logRxTaskStatus() void {
    r4os_rust_net_telemetry_log_rx();
}

pub fn sendArpRequest(adapter_index: usize, target_ip: [4]u8) TxResult {
    return @enumFromInt(r4os_rust_frames_send_arp(adapter_index, &target_ip));
}

pub fn arpTestGateway() TxResult {
    return @enumFromInt(r4os_rust_frames_arp_gateway());
}

pub fn arpFlush() void {
    r4os_rust_net_arp_cache_flush_owned();
}

pub fn dhcpAcquireDefault() TxResult {
    return dhcpAcquireDefaultUntil(null);
}

pub fn dhcpAcquireDefaultUntil(deadline_tick: ?u64) TxResult {
    return @enumFromInt(r4os_rust_dhcp_client_acquire_default(deadline_tick != null, deadline_tick orelse 0));
}

pub fn dhcpRenewDefault() TxResult {
    return dhcpRenewDefaultUntil(null);
}

pub fn dhcpRenewDefaultUntil(deadline_tick: ?u64) TxResult {
    return @enumFromInt(r4os_rust_dhcp_client_renew_default(deadline_tick != null, deadline_tick orelse 0));
}

pub fn dhcpReleaseDefault() TxResult {
    return @enumFromInt(r4os_rust_dhcp_client_release_default());
}

pub fn dhcpAcquire(adapter_index: usize) TxResult {
    return dhcpAcquireUntil(adapter_index, null);
}

pub fn dhcpAcquireUntil(adapter_index: usize, deadline_tick: ?u64) TxResult {
    return @enumFromInt(r4os_rust_dhcp_client_acquire(adapter_index, deadline_tick != null, deadline_tick orelse 0));
}

pub fn dhcpRenew(adapter_index: usize) TxResult {
    return dhcpRenewUntil(adapter_index, null);
}

pub fn dhcpRenewUntil(adapter_index: usize, deadline_tick: ?u64) TxResult {
    return @enumFromInt(r4os_rust_dhcp_client_renew(adapter_index, deadline_tick != null, deadline_tick orelse 0));
}

pub fn dhcpRelease(adapter_index: usize) TxResult {
    return @enumFromInt(r4os_rust_dhcp_client_release(adapter_index));
}

pub fn sendIpv4Payload(target_ip: [4]u8, protocol: u8, payload: []const u8) TxResult {
    return @enumFromInt(r4os_rust_frames_send_ipv4(&target_ip, protocol, payload.ptr, payload.len));
}

pub fn dnsResolveA(name: []const u8, out: *[4]u8) i32 {
    return r4os_rust_dns_client_resolve(name.ptr, name.len, out);
}

pub fn dnsResolveAWithServer(name: []const u8, server: [4]u8, out: *[4]u8) i32 {
    return r4os_rust_dns_client_resolve_server(name.ptr, name.len, &server, out);
}

pub fn dnsResultName(result: i32) []const u8 {
    return switch (result) {
        dns.RESULT_OK => "ok",
        dns.RESULT_SHORT => "short",
        dns.RESULT_HEADER => "header",
        dns.RESULT_QNAME => "qname",
        dns.RESULT_QUESTION => "question",
        dns.RESULT_ANAME => "aname",
        dns.RESULT_ANSWER => "answer",
        dns.RESULT_ATYPE => "atype",
        dns.RESULT_BUFFER_SMALL => "buffer-small",
        dns.RESULT_NAME => "name",
        dns.RESULT_NXDOMAIN => "nxdomain",
        dns.RESULT_TIMEOUT => "timeout",
        dns.RESULT_NO_SERVER => "no-server",
        dns.RESULT_TX => "tx-error",
        else => "unknown",
    };
}

pub fn icmpEchoGateway() TxResult {
    return icmpEchoTarget(net_config.gatewayIp());
}

pub fn icmpEchoTarget(target_ip: [4]u8) TxResult {
    return @enumFromInt(r4os_rust_frames_echo(&target_ip));
}

pub fn tcpConnect(remote_ip: [4]u8, port: u16) i32 {
    return tcpConnectUntil(remote_ip, port, null);
}

pub fn tcpConnectUntil(remote_ip: [4]u8, port: u16, request_deadline_tick: ?u64) i32 {
    return r4os_rust_tcp_client_connect(&remote_ip, port, request_deadline_tick != null, request_deadline_tick orelse 0);
}

pub fn tcpWrite(conn_id: u32, data: []const u8) i32 {
    return r4os_rust_tcp_client_write(conn_id, data.ptr, data.len);
}

pub fn tcpRead(conn_id: u32, out: []u8) i32 {
    return r4os_rust_tcp_client_read(conn_id, out.ptr, out.len);
}

pub fn tcpReadAvailable(conn_id: u32, out: []u8) i32 {
    return r4os_rust_tcp_client_read_available(conn_id, out.ptr, out.len);
}

pub fn tcpListen(port: u16) bool {
    return tcp.listen(port);
}

pub fn tcpHasListener(port: u16) bool {
    return tcp.hasListener(port);
}

pub fn tcpPollService() void {
    r4os_rust_net_rx_runtime_service_poll(TCP_POLL_ROUNDS, &rx_peers);
}

pub fn tcpRetransmitLast(conn_id: u32) i32 {
    return r4os_rust_tcp_client_retransmit_last(conn_id);
}

pub fn tcpCloseListener(port: u16) void {
    tcp.closeListener(port);
}

pub fn tcpEchoListenOnce(port: u16, out: []u8) i32 {
    return r4os_rust_tcp_client_echo_once(port, out.ptr, out.len);
}

pub fn tcpAcceptReadOnce(port: u16, out: []u8, conn_id_out: *u32) i32 {
    return r4os_rust_tcp_client_accept_read_once(port, out.ptr, out.len, conn_id_out);
}

pub fn tcpAcceptReadOnListener(port: u16, out: []u8, conn_id_out: *u32) i32 {
    return r4os_rust_tcp_client_accept_read(port, out.ptr, out.len, conn_id_out);
}

pub fn tcpAcceptOnListener(port: u16, conn_id_out: *u32) i32 {
    return r4os_rust_tcp_client_accept(port, conn_id_out);
}

pub fn tcpAcceptPollOnListener(port: u16, conn_id_out: *u32) i32 {
    return r4os_rust_tcp_client_accept_poll(port, conn_id_out);
}

pub fn tcpClose(conn_id: u32) i32 {
    return r4os_rust_tcp_client_close(conn_id);
}

pub fn tcpAbort(conn_id: u32) i32 {
    return r4os_rust_tcp_client_abort(conn_id);
}

pub fn tcpSummary(out: *tcp.Summary) void {
    tcp.summary(out);
}

pub fn tcpConnectionInfo(index: u32, out: *tcp.ConnectionInfo) i32 {
    return tcp.connectionInfo(index, out);
}

pub fn localIp() [4]u8 {
    return net_config.localIp();
}

fn retransmitTcpConnection(conn_id: u32, kind: tcp.RetransmitKind, reason: []const u8) TxResult {
    return @enumFromInt(r4os_rust_tcp_client_retransmit(conn_id, @intCast(@intFromEnum(kind)), adapterText(reason)));
}

fn retransmitTcpData(conn_id: u32) TxResult {
    return retransmitTcpConnection(conn_id, .data, "data-retry");
}

fn retransmitTcpFin(conn_id: u32) TxResult {
    return retransmitTcpConnection(conn_id, .fin, "fin-retry");
}

// --- 0.56.22: Eventgetriebene TCP-Waits (Befund 6.2) ---
// EINE globale Activity-Queue statt Queue-pro-Slot: der RX-Dispatch weckt
// nach jedem verarbeiteten TCP-Segment ALLE Wartenden (wakeAll ist bei den
// wenigen gleichzeitigen Warte-Tasks billig); die waitUnless-Praedikate
// filtern und schliessen das Lost-Wakeup-Fenster (Muster 0.56.13/0.56.19).
// Der Antrieb kommt vom net-rx-Task (0.56.2) und der NIC-IRQ-Zustellung
// (0.56.21) - die Warte-Schleifen pollen selbst nur noch initial einmal.
// wakeAll aus dem IRQ-Kontext ist sicher: wakeOneWith laeuft unter
// enterCritical ohne Schlaf (Praezedenz: onTick ruft wakeTask im IRQ).
// 0.56.40: hz-neutral (250-ms-Slice; bei 100 Hz wie zuvor 25 Ticks).
const TCP_WAIT_SLICE_TICKS: u64 = timing.msToTicks(250);

// Adaptiver RTO: Der physische Slot ist nur zusammen mit Connection-ID und
// Slotgeneration gueltig. Der Sendekatalog besitzt die Segmentzeitstempel;
// diese Tabelle besitzt ausschliesslich die connection-lokale SRTT-Messung.

// Backoff pro Retransmit-Runde (rto * 2^n, gedeckelt).

// Der net-rx-Task retransmittiert das aelteste unbestaetigte Daten- oder
// FIN-Segment. SYN bleibt Eigentum der synchronen Connect-Schleife; FIN
// gehoert nach dem nicht blockierenden tcpClose dem Kernelzustand.
const PROACTIVE_MAX_RESEND: u8 = 5;

const NativeRouteDecision = extern struct {
    result: u8,
    next_hop_ip: [4]u8,
    dest_mac: [6]u8,
    needs_arp: bool,
    last_error: NativeBytesText,
};
extern fn r4os_rust_net_bytes_route_ipv4_target(*const [4]u8, *NativeRouteDecision) void;
extern fn r4os_rust_net_bytes_same_subnet(*const [4]u8, *const [4]u8, *const [4]u8) bool;
extern fn r4os_rust_net_bytes_is_limited_broadcast_ip(*const [4]u8) bool;
extern fn r4os_rust_net_bytes_is_subnet_broadcast_ip(*const [4]u8) bool;
extern fn r4os_rust_net_bytes_is_multicast_ip(*const [4]u8) bool;
extern fn r4os_rust_net_bytes_multicast_mac_for_ip(*const [4]u8, *[6]u8) void;
extern fn r4os_rust_net_bytes_copy_fixed_bytes([*]u8, usize, [*]const u8, usize) void;
extern fn r4os_rust_net_bytes_fixed_text_equals_ignore_case([*]const u8, usize, [*]const u8, usize) bool;
extern fn r4os_rust_net_bytes_upper_ascii(u8) u8;
extern fn r4os_rust_net_bytes_mem_eql([*]const u8, usize, [*]const u8, usize) bool;
extern fn r4os_rust_net_bytes_mem_contains([*]const u8, usize, [*]const u8, usize) bool;
extern fn r4os_rust_net_bytes_copy_ip_bytes([*]u8, usize, *const [4]u8) void;
extern fn r4os_rust_net_bytes_copy_mac_bytes([*]u8, usize, *const [6]u8) void;
extern fn r4os_rust_net_bytes_copy_ip_from_bytes(*[4]u8, [*]const u8, usize) void;
extern fn r4os_rust_net_bytes_copy_mac_from_bytes(*[6]u8, [*]const u8, usize) void;
extern fn r4os_rust_net_bytes_mac_equals(*const [6]u8, *const [6]u8) bool;
extern fn r4os_rust_net_bytes_is_zero_ip(*const [4]u8) bool;
extern fn r4os_rust_net_bytes_is_zero_mac(*const [6]u8) bool;
extern fn r4os_rust_net_bytes_read_be16([*]const u8, usize, usize) u16;
extern fn r4os_rust_net_bytes_write_be16([*]u8, usize, usize, u16) void;
extern fn r4os_rust_net_bytes_internet_checksum([*]const u8, usize) u16;
extern fn r4os_rust_net_bytes_write_be32([*]u8, usize, usize, u32) void;

comptime {
    if (@sizeOf(NativeRouteDecision) != 32 or @alignOf(NativeRouteDecision) != 8 or @offsetOf(NativeRouteDecision, "result") != 0 or @offsetOf(NativeRouteDecision, "next_hop_ip") != 1 or @offsetOf(NativeRouteDecision, "dest_mac") != 5 or @offsetOf(NativeRouteDecision, "needs_arp") != 11 or @offsetOf(NativeRouteDecision, "last_error") != 16) @compileError("Native network NativeRouteDecision layout");
}
comptime {
    if (@intFromEnum(TxResult.ok) != 0 or @intFromEnum(TxResult.backend_error) != 6) @compileError("Native route result values");
}

comptime {
    if (@sizeOf(UdpRecvInfo) != 14 or @alignOf(UdpRecvInfo) != 2 or @offsetOf(UdpRecvInfo, "source_ip") != 0 or @offsetOf(UdpRecvInfo, "dest_ip") != 4 or @offsetOf(UdpRecvInfo, "source_port") != 8 or @offsetOf(UdpRecvInfo, "dest_port") != 10 or @offsetOf(UdpRecvInfo, "length") != 12) @compileError("Native network UdpRecvInfo layout");
}

comptime {
    if (@sizeOf(NativeSocketSummary) != 24 or @alignOf(NativeSocketSummary) != 8 or @offsetOf(NativeSocketSummary, "active") != 0 or @offsetOf(NativeSocketSummary, "queued") != 4 or @offsetOf(NativeSocketSummary, "delivered") != 8 or @offsetOf(NativeSocketSummary, "drops") != 16) @compileError("Native network NativeSocketSummary layout");
}
comptime {
    if (UDP_SOCKET_MAX != 8 or UDP_SOCKET_QUEUE_SIZE != 4 or UDP_SOCKET_PAYLOAD_MAX != 1472) @compileError("Native UDP socket capacities");
}

const UdpWaitPeers = extern struct {
    recv: *const fn (u32, *UdpRecvInfo, [*]u8, usize) callconv(.c) i32,
    poll: *const fn (usize) callconv(.c) void,
    idle: *const fn () callconv(.c) void,
    err: *const fn ([*]const u8, usize) callconv(.c) void,
};
const udp_wait_peers = UdpWaitPeers{ .recv = udpWaitRecv, .poll = udpWaitPoll, .idle = udpWaitIdle, .err = udpWaitError };
extern fn r4os_rust_udp_recv_wait(u32, *UdpRecvInfo, [*]u8, usize, u64, *const UdpWaitPeers) i32;
fn udpWaitRecv(handle: u32, info: *UdpRecvInfo, data: [*]u8, len: usize) callconv(.c) i32 {
    return udpRecvFrom(handle, info, data[0..len]);
}
fn udpWaitPoll(rounds: usize) callconv(.c) void {
    pollAdapters(rounds);
}
fn udpWaitIdle() callconv(.c) void {
    scheduler.yield();
    interrupts.enable();
    interrupts.waitForInterrupt();
}
fn udpWaitError(text: [*]const u8, len: usize) callconv(.c) void {
    udp.setOwnedError(text[0..len]);
}
comptime {
    if (DHCP_MAX_LOOPS != 200000 or DHCP_POLL_ROUNDS != 512 or @sizeOf(UdpWaitPeers) != 32 or @alignOf(UdpWaitPeers) != 8 or @offsetOf(UdpWaitPeers, "recv") != 0 or @offsetOf(UdpWaitPeers, "poll") != 8 or @offsetOf(UdpWaitPeers, "idle") != 16 or @offsetOf(UdpWaitPeers, "err") != 24) @compileError("Native UDP wait peer/layout/policy");
}

extern fn r4os_rust_net_packet_pool_reset() void;
extern fn r4os_rust_net_packet_alloc() usize;
extern fn r4os_rust_net_packet_free(usize) void;
extern fn r4os_rust_net_packet(usize) ?*Packet;
extern fn r4os_rust_net_packet_free_count() usize;
extern fn r4os_rust_net_packet_drops() u64;
extern fn r4os_rust_net_packet_remove_adapter(usize) void;

comptime {
    if (@sizeOf(Packet) != 1560 or @alignOf(Packet) != 8 or @offsetOf(Packet, "adapter_index") != 0 or @offsetOf(Packet, "len") != 8 or @offsetOf(Packet, "flags") != 16 or @offsetOf(Packet, "data") != 20) @compileError("Native network Packet layout");
}
comptime {
    if (PACKET_POOL_SIZE != 16 or MAX_PACKET_SIZE != 1536) @compileError("Native packet pool capacity");
}

extern fn r4os_rust_app_ipv4_reset() void;
extern fn r4os_rust_app_ipv4_enqueue(*const AppIpv4Packet, [*]const u8, usize) void;
extern fn r4os_rust_app_ipv4_queued() usize;
extern fn r4os_rust_app_ipv4_drops() u64;
extern fn r4os_rust_app_ipv4_read(u8, *AppIpv4Packet, [*]u8, usize) i32;
comptime {
    if (APP_IPV4_QUEUE_SIZE != 64 or APP_IPV4_PAYLOAD_MAX != 1536 or APP_IPV4_BUCKET_COUNT != 6 or APP_IPV4_BUCKET_ICMP != 0 or APP_IPV4_BUCKET_UDP_DHCP != 1 or APP_IPV4_BUCKET_UDP_DNS != 2 or APP_IPV4_BUCKET_UDP_OTHER != 3 or APP_IPV4_BUCKET_TCP != 4 or APP_IPV4_BUCKET_OTHER != 5) @compileError("Native application IPv4 queue policy");
}

comptime {
    if (@sizeOf(AppIpv4Packet) != 20 or @alignOf(AppIpv4Packet) != 4 or @offsetOf(AppIpv4Packet, "source_ip") != 0 or @offsetOf(AppIpv4Packet, "dest_ip") != 4 or @offsetOf(AppIpv4Packet, "protocol") != 8 or @offsetOf(AppIpv4Packet, "source_port") != 10 or @offsetOf(AppIpv4Packet, "dest_port") != 12 or @offsetOf(AppIpv4Packet, "truncated") != 14 or @offsetOf(AppIpv4Packet, "payload_len") != 16) @compileError("Native network AppIpv4Packet layout");
}

extern fn r4os_rust_net_cache_reset() void;
comptime {
    if (ARP_CACHE_ENTRIES != 8 or DNS_CACHE_ENTRIES != 4 or DNS_DEFAULT_CACHE_TTL_SECONDS != 60 or DNS_NEGATIVE_CACHE_TTL_SECONDS != 30) @compileError("Native network cache policy");
}

extern fn r4os_rust_wire_ethernet_new(*r4p_contract.EthernetFrameOp) bool;
extern fn r4os_rust_wire_ethernet_set_frame(*r4p_contract.EthernetFrameOp, [*]const u8, usize) bool;
extern fn r4os_rust_wire_ethernet_dispatch(u32, *r4p_contract.EthernetFrameOp, u32) bool;
extern fn r4os_rust_wire_arp_new(*r4p_contract.ArpOp) bool;
extern fn r4os_rust_wire_arp_set_frame(*r4p_contract.ArpOp, [*]const u8, usize) bool;
extern fn r4os_rust_wire_arp_dispatch(u32, *r4p_contract.ArpOp, u32) bool;
extern fn r4os_rust_wire_ipv4_new(*r4p_contract.Ipv4Op) bool;
extern fn r4os_rust_wire_ipv4_set_frame(*r4p_contract.Ipv4Op, [*]const u8, usize) bool;
extern fn r4os_rust_wire_ipv4_dispatch(u32, *r4p_contract.Ipv4Op, u32) bool;
extern fn r4os_rust_wire_icmp_new(*r4p_contract.IcmpOp) bool;
extern fn r4os_rust_wire_icmp_set_payload(*r4p_contract.IcmpOp, [*]const u8, usize) bool;
extern fn r4os_rust_wire_icmp_dispatch(u32, *r4p_contract.IcmpOp, u32) bool;
extern fn r4os_rust_wire_udp_new(*r4p_contract.UdpOp) bool;
extern fn r4os_rust_wire_udp_set_datagram(*r4p_contract.UdpOp, [*]const u8, usize) bool;
extern fn r4os_rust_wire_udp_dispatch(u32, *r4p_contract.UdpOp, u32) bool;
extern fn r4os_rust_wire_dhcp_new(*r4p_contract.DhcpOp) bool;
extern fn r4os_rust_wire_dhcp_set_payload(*r4p_contract.DhcpOp, [*]const u8, usize) bool;
extern fn r4os_rust_wire_dhcp_dispatch(u32, *r4p_contract.DhcpOp, u32) bool;
extern fn r4os_rust_wire_dns_new(*r4p_contract.DnsOp) bool;
extern fn r4os_rust_wire_dns_set_name(*r4p_contract.DnsOp, [*]const u8, usize) bool;
extern fn r4os_rust_wire_dns_set_payload(*r4p_contract.DnsOp, [*]const u8, usize) bool;
extern fn r4os_rust_wire_dns_dispatch(u32, *r4p_contract.DnsOp, u32) bool;
extern fn r4os_rust_wire_tcp_new(*r4p_contract.TcpOp) bool;
extern fn r4os_rust_wire_tcp_set_payload(*r4p_contract.TcpOp, [*]const u8, usize) bool;
extern fn r4os_rust_wire_tcp_set_segment(*r4p_contract.TcpOp, [*]const u8, usize) bool;
extern fn r4os_rust_wire_tcp_dispatch(u32, *r4p_contract.TcpOp, u32) bool;

const WireBuildReport = extern struct {
    length: usize = 0,
    dispatch_failures: u64 = 0,
    reason: NativeBytesText = .{ .pointer = "".ptr, .len = 0 },
    flags: u32 = 0,
    id: u16 = 0,
    message_type: u8 = 0,
    tcp_flags: u16 = 0,
};

const WireEthernetBuild = extern struct {
    source_mac: [6]u8 = .{0} ** 6,
};

const WireArpBuild = extern struct {
    source_mac: [6]u8 = .{0} ** 6,
    target_ip: [4]u8 = .{0} ** 4,
};

const WireIpv4Build = extern struct {
    source_mac: [6]u8 = .{0} ** 6,
    dest_mac: [6]u8 = .{0} ** 6,
    source_ip: [4]u8 = .{0} ** 4,
    dest_ip: [4]u8 = .{0} ** 4,
    protocol: u8 = 0,
};

const WireIcmpBuild = extern struct {
    ident: u16 = 0,
    seq: u16 = 0,
};

const WireUdpBuild = extern struct {
    source_ip: [4]u8 = .{0} ** 4,
    dest_ip: [4]u8 = .{0} ** 4,
    source_port: u16 = 0,
    dest_port: u16 = 0,
};

const WireDhcpBuild = extern struct {
    xid: u32 = 0,
    mac: [6]u8 = .{0} ** 6,
    requested_ip: [4]u8 = .{0} ** 4,
    server_ip: [4]u8 = .{0} ** 4,
    client_ip: [4]u8 = .{0} ** 4,
    state: u8 = 0,
};

const WireTcpBuild = extern struct {
    source_ip: [4]u8 = .{0} ** 4,
    dest_ip: [4]u8 = .{0} ** 4,
    source_port: u16 = 0,
    dest_port: u16 = 0,
    seq: u32 = 0,
    ack: u32 = 0,
    flags: u16 = 0,
    window: u16 = 0,
    options: u32 = 0,
};

extern fn r4os_rust_wire_build_ethernet(*const WireEthernetBuild, [*]u8, usize, *WireBuildReport) void;
extern fn r4os_rust_wire_build_arp(*const WireArpBuild, [*]u8, usize, *WireBuildReport) void;
extern fn r4os_rust_wire_build_ipv4(*const WireIpv4Build, [*]const u8, usize, [*]u8, usize, *WireBuildReport) void;
extern fn r4os_rust_wire_build_icmp_request(*const WireIcmpBuild, [*]u8, usize, *WireBuildReport) void;
extern fn r4os_rust_wire_build_icmp_reply([*]const u8, usize, [*]u8, usize, *WireBuildReport) void;
extern fn r4os_rust_wire_build_udp(*const WireUdpBuild, [*]const u8, usize, [*]u8, usize, *WireBuildReport) void;
extern fn r4os_rust_wire_build_dhcp_discover(*const WireDhcpBuild, [*]u8, usize, *WireBuildReport) void;
extern fn r4os_rust_wire_build_dhcp_request(*const WireDhcpBuild, [*]u8, usize, *WireBuildReport) void;
extern fn r4os_rust_wire_build_dhcp_release(*const WireDhcpBuild, [*]u8, usize, *WireBuildReport) void;
extern fn r4os_rust_wire_build_dns(u16, [*]const u8, usize, [*]u8, usize, *WireBuildReport) void;
extern fn r4os_rust_wire_build_tcp(*const WireTcpBuild, [*]const u8, usize, [*]u8, usize, *WireBuildReport) void;

comptime {
    if (@sizeOf(WireBuildReport) != 48 or @alignOf(WireBuildReport) != 8 or @offsetOf(WireBuildReport, "length") != 0 or @offsetOf(WireBuildReport, "dispatch_failures") != 8 or @offsetOf(WireBuildReport, "reason") != 16 or @offsetOf(WireBuildReport, "flags") != 32 or @offsetOf(WireBuildReport, "id") != 36 or @offsetOf(WireBuildReport, "message_type") != 38 or @offsetOf(WireBuildReport, "tcp_flags") != 40) @compileError("Native network WireBuildReport layout");
}

comptime {
    if (@sizeOf(WireEthernetBuild) != 6 or @alignOf(WireEthernetBuild) != 1 or @offsetOf(WireEthernetBuild, "source_mac") != 0) @compileError("Native network WireEthernetBuild layout");
}

comptime {
    if (@sizeOf(WireArpBuild) != 10 or @alignOf(WireArpBuild) != 1 or @offsetOf(WireArpBuild, "source_mac") != 0 or @offsetOf(WireArpBuild, "target_ip") != 6) @compileError("Native network WireArpBuild layout");
}

comptime {
    if (@sizeOf(WireIpv4Build) != 21 or @alignOf(WireIpv4Build) != 1 or @offsetOf(WireIpv4Build, "source_mac") != 0 or @offsetOf(WireIpv4Build, "dest_mac") != 6 or @offsetOf(WireIpv4Build, "source_ip") != 12 or @offsetOf(WireIpv4Build, "dest_ip") != 16 or @offsetOf(WireIpv4Build, "protocol") != 20) @compileError("Native network WireIpv4Build layout");
}

comptime {
    if (@sizeOf(WireIcmpBuild) != 4 or @alignOf(WireIcmpBuild) != 2 or @offsetOf(WireIcmpBuild, "ident") != 0 or @offsetOf(WireIcmpBuild, "seq") != 2) @compileError("Native network WireIcmpBuild layout");
}

comptime {
    if (@sizeOf(WireUdpBuild) != 12 or @alignOf(WireUdpBuild) != 2 or @offsetOf(WireUdpBuild, "source_ip") != 0 or @offsetOf(WireUdpBuild, "dest_ip") != 4 or @offsetOf(WireUdpBuild, "source_port") != 8 or @offsetOf(WireUdpBuild, "dest_port") != 10) @compileError("Native network WireUdpBuild layout");
}

comptime {
    if (@sizeOf(WireDhcpBuild) != 24 or @alignOf(WireDhcpBuild) != 4 or @offsetOf(WireDhcpBuild, "xid") != 0 or @offsetOf(WireDhcpBuild, "mac") != 4 or @offsetOf(WireDhcpBuild, "requested_ip") != 10 or @offsetOf(WireDhcpBuild, "server_ip") != 14 or @offsetOf(WireDhcpBuild, "client_ip") != 18 or @offsetOf(WireDhcpBuild, "state") != 22) @compileError("Native network WireDhcpBuild layout");
}

comptime {
    if (@sizeOf(WireTcpBuild) != 28 or @alignOf(WireTcpBuild) != 4 or @offsetOf(WireTcpBuild, "source_ip") != 0 or @offsetOf(WireTcpBuild, "dest_ip") != 4 or @offsetOf(WireTcpBuild, "source_port") != 8 or @offsetOf(WireTcpBuild, "dest_port") != 10 or @offsetOf(WireTcpBuild, "seq") != 12 or @offsetOf(WireTcpBuild, "ack") != 16 or @offsetOf(WireTcpBuild, "flags") != 20 or @offsetOf(WireTcpBuild, "window") != 22 or @offsetOf(WireTcpBuild, "options") != 24) @compileError("Native network WireTcpBuild layout");
}

const WireReadInput = extern struct {
    source_ip: [4]u8 = .{0} ** 4,
    dest_ip: [4]u8 = .{0} ** 4,
    own_mac: [6]u8 = .{0} ** 6,
    protocol: u8 = 0,
    l4_checksum_valid: bool = false,
};
const WireReadReport = extern struct {
    dispatch_failures: u64,
    reason: NativeBytesText,
    completed: bool,
    accepted: bool,
    value: u32,
};
comptime {
    if (backend_contract.protocol_buffer_flag_rx_l4_checksum_valid != 1 or icmp.IPV4_PROTOCOL != 1 or udp.IPV4_PROTOCOL != 17 or tcp.IPV4_PROTOCOL != 6) @compileError("Native wire reader protocol/checksum metadata");
}
extern fn r4os_rust_wire_read_ethernet_rx(*const WireReadInput, [*]const u8, usize, *r4p_contract.EthernetFrameOp, *WireReadReport) void;
extern fn r4os_rust_wire_read_ethernet_tx(*const WireReadInput, [*]const u8, usize, *r4p_contract.EthernetFrameOp, *WireReadReport) void;
extern fn r4os_rust_wire_read_ethernet_type(*const WireReadInput, [*]const u8, usize, *r4p_contract.EthernetFrameOp, *WireReadReport) void;
extern fn r4os_rust_wire_read_arp_rx(*const WireReadInput, [*]const u8, usize, *r4p_contract.ArpOp, *WireReadReport) void;
extern fn r4os_rust_wire_read_arp_tx(*const WireReadInput, [*]const u8, usize, *r4p_contract.ArpOp, *WireReadReport) void;
extern fn r4os_rust_wire_read_ipv4_rx(*const WireReadInput, [*]const u8, usize, *r4p_contract.Ipv4Op, *WireReadReport) void;
extern fn r4os_rust_wire_read_ipv4_tx(*const WireReadInput, [*]const u8, usize, *r4p_contract.Ipv4Op, *WireReadReport) void;
extern fn r4os_rust_wire_read_icmp_rx(*const WireReadInput, [*]const u8, usize, *r4p_contract.IcmpOp, *WireReadReport) void;
extern fn r4os_rust_wire_read_icmp_tx(*const WireReadInput, [*]const u8, usize, *r4p_contract.IcmpOp, *WireReadReport) void;
extern fn r4os_rust_wire_read_icmp_echo(*const WireReadInput, [*]const u8, usize, *r4p_contract.IcmpOp, *WireReadReport) void;
extern fn r4os_rust_wire_read_udp_rx(*const WireReadInput, [*]const u8, usize, *r4p_contract.UdpOp, *WireReadReport) void;
extern fn r4os_rust_wire_read_udp_tx(*const WireReadInput, [*]const u8, usize, *r4p_contract.UdpOp, *WireReadReport) void;
extern fn r4os_rust_wire_read_dhcp(*const WireReadInput, [*]const u8, usize, *r4p_contract.DhcpOp, *WireReadReport) void;
extern fn r4os_rust_wire_read_dns(*const WireReadInput, [*]const u8, usize, *r4p_contract.DnsOp, *WireReadReport) void;
extern fn r4os_rust_wire_read_tcp_rx(*const WireReadInput, [*]const u8, usize, *r4p_contract.TcpOp, *WireReadReport) void;
extern fn r4os_rust_wire_read_tcp_tx(*const WireReadInput, [*]const u8, usize, *r4p_contract.TcpOp, *WireReadReport) void;

comptime {
    if (@sizeOf(WireReadInput) != 16 or @alignOf(WireReadInput) != 1 or @offsetOf(WireReadInput, "source_ip") != 0 or @offsetOf(WireReadInput, "dest_ip") != 4 or @offsetOf(WireReadInput, "own_mac") != 8 or @offsetOf(WireReadInput, "protocol") != 14 or @offsetOf(WireReadInput, "l4_checksum_valid") != 15) @compileError("Native network WireReadInput layout");
}

comptime {
    if (@sizeOf(WireReadReport) != 32 or @alignOf(WireReadReport) != 8 or @offsetOf(WireReadReport, "dispatch_failures") != 0 or @offsetOf(WireReadReport, "reason") != 8 or @offsetOf(WireReadReport, "completed") != 24 or @offsetOf(WireReadReport, "accepted") != 25 or @offsetOf(WireReadReport, "value") != 28) @compileError("Native network WireReadReport layout");
}

const RtoStatus = extern struct {
    proactive_retransmits: u64,
    samples: u64,
    last_ticks: u64,
};
const RtoPeerFn = *const fn (u32) callconv(.c) u8;
const RtoPeers = extern struct {
    data: RtoPeerFn,
    fin: RtoPeerFn,
};
const rto_peers = RtoPeers{ .data = rtoSendData, .fin = rtoSendFin };
fn rtoSendData(id: u32) callconv(.c) u8 {
    return @intFromEnum(retransmitTcpData(id));
}
fn rtoSendFin(id: u32) callconv(.c) u8 {
    return @intFromEnum(retransmitTcpFin(id));
}
extern fn r4os_rust_tcp_rto_reset(bool) void;
extern fn r4os_rust_tcp_rto_release(*const tcp.ConnectionIdentity) void;
extern fn r4os_rust_tcp_rto_for_conn(u32) u64;
extern fn r4os_rust_tcp_rto_backoff(u32, u8) u64;
extern fn r4os_rust_tcp_rto_stamp_send(u32) void;
extern fn r4os_rust_tcp_rto_refresh(u32) void;
extern fn r4os_rust_tcp_rto_on_ack(*const tcp.ConnectionIdentity) void;
extern fn r4os_rust_tcp_rto_status(*RtoStatus) void;
extern fn r4os_rust_tcp_rto_sweep(u64, *const RtoPeers) void;

comptime {
    if (TCP_RTO_MIN_TICKS != timing.msToTicks(50) or TCP_RTO_MAX_TICKS != timing.msToTicks(1500) or PROACTIVE_MAX_RESEND != 5 or tcp.MAX_CONNECTIONS != 16 or @intFromEnum(TxResult.ok) != 0) @compileError("Native TCP RTO policy");
}

comptime {
    if (@sizeOf(RtoStatus) != 24 or @alignOf(RtoStatus) != 8 or @offsetOf(RtoStatus, "proactive_retransmits") != 0 or @offsetOf(RtoStatus, "samples") != 8 or @offsetOf(RtoStatus, "last_ticks") != 16) @compileError("Native network RtoStatus layout");
}

comptime {
    if (@sizeOf(RtoPeers) != 16 or @alignOf(RtoPeers) != 8 or @offsetOf(RtoPeers, "data") != 0 or @offsetOf(RtoPeers, "fin") != 8) @compileError("Native network RtoPeers layout");
}

const NativeAdapterTransmitFn = *const fn (usize, usize, [*]const u8, usize) callconv(.c) u8;
const NativeAdapterPollFn = *const fn (usize, usize) callconv(.c) void;
const NativeAdapterStatusFn = *const fn (usize, usize, *BackendStatus) callconv(.c) i32;
const NativeAdapterOps = extern struct {
    transmit: ?NativeAdapterTransmitFn,
    transmit_context: usize,
    poll: ?NativeAdapterPollFn,
    poll_context: usize,
    status: ?NativeAdapterStatusFn,
    status_context: usize,
};
const NativeAdapterStats = extern struct {
    rx_packets: u64,
    tx_packets: u64,
    rx_bytes: u64,
    tx_bytes: u64,
    drops: u64,
    errors: u64,
    resets: u64,
    last_error: NativeBytesText,
};
const NativeAdapter = extern struct {
    registration_id: u32,
    name: NativeBytesText,
    driver: NativeBytesText,
    bus: u8,
    bus_no: u8,
    device_no: u8,
    function_no: u8,
    vendor_id: u16,
    device_id: u16,
    mac: [6]u8,
    mtu: u16,
    flags: u32,
    link: u8,
    lifecycle: u8,
    registered_tick: u64,
    state_changed_tick: u64,
    stats: NativeAdapterStats,
    backend: backend_contract.Negotiation,
    ops: NativeAdapterOps,
};

extern fn r4os_rust_net_adapters_reset() void;
extern fn r4os_rust_net_adapter_count() usize;
extern fn r4os_rust_net_adapter_register(*const NativeAdapter) usize;
extern fn r4os_rust_net_adapter_get(usize, *NativeAdapter) bool;
extern fn r4os_rust_net_adapter_index(u32) usize;
extern fn r4os_rust_net_adapter_has_device(u16, u16) bool;
extern fn r4os_rust_net_adapter_remove(usize) bool;
extern fn r4os_rust_net_adapter_lifecycle(usize, u8) void;
extern fn r4os_rust_net_adapter_link(usize, u8) void;
extern fn r4os_rust_net_adapter_error(usize, NativeBytesText) void;
extern fn r4os_rust_net_adapter_drop(usize, NativeBytesText) void;
extern fn r4os_rust_net_adapter_reset_note(usize, NativeBytesText) void;
extern fn r4os_rust_net_adapter_rx(usize, usize) void;
extern fn r4os_rust_net_adapter_tx_ok(usize, usize) void;
extern fn r4os_rust_net_adapter_tx_error(usize, NativeBytesText) void;
extern fn r4os_rust_net_adapter_transmit(usize, [*]const u8, usize) u8;
extern fn r4os_rust_net_adapter_poll(usize) void;
extern fn r4os_rust_net_adapter_status(usize, *BackendStatus) i32;

fn adapterText(value: []const u8) NativeBytesText {
    return .{ .pointer = value.ptr, .len = value.len };
}
// Original Zig function pointers are only interpreted by these typed Zig
// invokers. Rust calls the paired C function, retaining an opaque context.
fn adapterZigTransmit(context: usize, index: usize, data: [*]const u8, len: usize) callconv(.c) u8 {
    const f: *const fn (usize, []const u8) TxResult = @ptrFromInt(context);
    return @intFromEnum(f(index, data[0..len]));
}
fn adapterZigPoll(context: usize, index: usize) callconv(.c) void {
    const f: *const fn (usize) void = @ptrFromInt(context);
    f(index);
}
fn adapterZigStatus(context: usize, index: usize, out: *BackendStatus) callconv(.c) i32 {
    const f: *const fn (usize, *BackendStatus) i32 = @ptrFromInt(context);
    return f(index, out);
}
fn adapterNativeTransmit(index: usize, data: []const u8) TxResult {
    return @enumFromInt(r4os_rust_net_adapter_transmit(index, data.ptr, data.len));
}
fn adapterNativePoll(index: usize) void {
    r4os_rust_net_adapter_poll(index);
}
fn adapterNativeStatus(index: usize, out: *BackendStatus) i32 {
    return r4os_rust_net_adapter_status(index, out);
}
fn toNativeAdapterStats(value: Stats) NativeAdapterStats {
    return .{ .rx_packets = value.rx_packets, .tx_packets = value.tx_packets, .rx_bytes = value.rx_bytes, .tx_bytes = value.tx_bytes, .drops = value.drops, .errors = value.errors, .resets = value.resets, .last_error = adapterText(value.last_error) };
}
fn fromNativeAdapterStats(value: NativeAdapterStats) Stats {
    return .{ .rx_packets = value.rx_packets, .tx_packets = value.tx_packets, .rx_bytes = value.rx_bytes, .tx_bytes = value.tx_bytes, .drops = value.drops, .errors = value.errors, .resets = value.resets, .last_error = value.last_error.slice() };
}
fn toNativeAdapter(value: Adapter) NativeAdapter {
    return .{ .registration_id = value.registration_id, .name = adapterText(value.name), .driver = adapterText(value.driver), .bus = @intFromEnum(value.bus), .bus_no = value.bus_no, .device_no = value.device_no, .function_no = value.function_no, .vendor_id = value.vendor_id, .device_id = value.device_id, .mac = value.mac, .mtu = value.mtu, .flags = value.flags, .link = @intFromEnum(value.link), .lifecycle = @intFromEnum(value.lifecycle), .registered_tick = value.registered_tick, .state_changed_tick = value.state_changed_tick, .stats = toNativeAdapterStats(value.stats), .backend = value.backend, .ops = toNativeAdapterOps(value.ops) };
}
fn fromNativeAdapter(value: NativeAdapter) Adapter {
    return .{ .registration_id = value.registration_id, .name = value.name.slice(), .driver = value.driver.slice(), .bus = @enumFromInt(value.bus), .bus_no = value.bus_no, .device_no = value.device_no, .function_no = value.function_no, .vendor_id = value.vendor_id, .device_id = value.device_id, .mac = value.mac, .mtu = value.mtu, .flags = value.flags, .link = @enumFromInt(value.link), .lifecycle = @enumFromInt(value.lifecycle), .registered_tick = value.registered_tick, .state_changed_tick = value.state_changed_tick, .stats = fromNativeAdapterStats(value.stats), .backend = value.backend, .ops = fromNativeAdapterOps(value.ops) };
}

fn toNativeAdapterOps(value: AdapterOps) NativeAdapterOps {
    return .{ .transmit = if (value.transmit != null) adapterZigTransmit else null, .transmit_context = if (value.transmit) |f| @intFromPtr(f) else 0, .poll = if (value.poll != null) adapterZigPoll else null, .poll_context = if (value.poll) |f| @intFromPtr(f) else 0, .status = if (value.status != null) adapterZigStatus else null, .status_context = if (value.status) |f| @intFromPtr(f) else 0 };
}
fn fromNativeAdapterOps(value: NativeAdapterOps) AdapterOps {
    return .{ .transmit = if (value.transmit != null) adapterNativeTransmit else null, .poll = if (value.poll != null) adapterNativePoll else null, .status = if (value.status != null) adapterNativeStatus else null };
}
comptime {
    if (MAX_ADAPTERS != 8 or (ADAPTER_FLAG_TRUSTED_BACKEND | ADAPTER_FLAG_BROADCAST) != 5 or @intFromEnum(Bus.serial) != 3 or @intFromEnum(Link.up) != 2 or @intFromEnum(Lifecycle.shutdown) != 5) @compileError("Native adapter registry constants");
}

comptime {
    if (@sizeOf(NativeAdapterOps) != 48 or @alignOf(NativeAdapterOps) != 8 or @offsetOf(NativeAdapterOps, "transmit") != 0 or @offsetOf(NativeAdapterOps, "transmit_context") != 8 or @offsetOf(NativeAdapterOps, "poll") != 16 or @offsetOf(NativeAdapterOps, "poll_context") != 24 or @offsetOf(NativeAdapterOps, "status") != 32 or @offsetOf(NativeAdapterOps, "status_context") != 40) @compileError("Native network NativeAdapterOps layout");
}

comptime {
    if (@sizeOf(NativeAdapterStats) != 72 or @alignOf(NativeAdapterStats) != 8 or @offsetOf(NativeAdapterStats, "rx_packets") != 0 or @offsetOf(NativeAdapterStats, "tx_packets") != 8 or @offsetOf(NativeAdapterStats, "rx_bytes") != 16 or @offsetOf(NativeAdapterStats, "tx_bytes") != 24 or @offsetOf(NativeAdapterStats, "drops") != 32 or @offsetOf(NativeAdapterStats, "errors") != 40 or @offsetOf(NativeAdapterStats, "resets") != 48 or @offsetOf(NativeAdapterStats, "last_error") != 56) @compileError("Native network NativeAdapterStats layout");
}

comptime {
    if (@sizeOf(NativeAdapter) != 256 or @alignOf(NativeAdapter) != 8 or @offsetOf(NativeAdapter, "registration_id") != 0 or @offsetOf(NativeAdapter, "name") != 8 or @offsetOf(NativeAdapter, "driver") != 24 or @offsetOf(NativeAdapter, "bus") != 40 or @offsetOf(NativeAdapter, "bus_no") != 41 or @offsetOf(NativeAdapter, "device_no") != 42 or @offsetOf(NativeAdapter, "function_no") != 43 or @offsetOf(NativeAdapter, "vendor_id") != 44 or @offsetOf(NativeAdapter, "device_id") != 46 or @offsetOf(NativeAdapter, "mac") != 48 or @offsetOf(NativeAdapter, "mtu") != 54 or @offsetOf(NativeAdapter, "flags") != 56 or @offsetOf(NativeAdapter, "link") != 60 or @offsetOf(NativeAdapter, "lifecycle") != 61 or @offsetOf(NativeAdapter, "registered_tick") != 64 or @offsetOf(NativeAdapter, "state_changed_tick") != 72 or @offsetOf(NativeAdapter, "stats") != 80 or @offsetOf(NativeAdapter, "backend") != 152 or @offsetOf(NativeAdapter, "ops") != 208) @compileError("Native network NativeAdapter layout");
}

comptime {
    if (@sizeOf(BackendStatus) != 160 or @alignOf(BackendStatus) != 8 or @offsetOf(BackendStatus, "link_up") != 0 or @offsetOf(BackendStatus, "rx_packets") != 8 or @offsetOf(BackendStatus, "tx_packets") != 16 or @offsetOf(BackendStatus, "drops") != 24 or @offsetOf(BackendStatus, "errors") != 32 or @offsetOf(BackendStatus, "irq_line") != 40 or @offsetOf(BackendStatus, "irq_pin") != 41 or @offsetOf(BackendStatus, "irq_registered") != 42 or @offsetOf(BackendStatus, "irq_mode") != 43 or @offsetOf(BackendStatus, "irq_count") != 48 or @offsetOf(BackendStatus, "irq_handled") != 56 or @offsetOf(BackendStatus, "poll_count") != 64 or @offsetOf(BackendStatus, "poll_fallbacks") != 72 or @offsetOf(BackendStatus, "last_isr") != 80 or @offsetOf(BackendStatus, "reserved") != 82 or @offsetOf(BackendStatus, "rx_errors") != 88 or @offsetOf(BackendStatus, "tx_errors") != 96 or @offsetOf(BackendStatus, "rx_overflows") != 104 or @offsetOf(BackendStatus, "rx_recoveries") != 112 or @offsetOf(BackendStatus, "offered_capabilities") != 120 or @offsetOf(BackendStatus, "accepted_capabilities") != 128 or @offsetOf(BackendStatus, "rx_offload_packets") != 136 or @offsetOf(BackendStatus, "rx_software_fallbacks") != 144 or @offsetOf(BackendStatus, "rx_metadata_errors") != 152) @compileError("Native network BackendStatus layout");
}

const NativeTxCounters = extern struct {
    failures: u64,
    no_adapter: u64,
    link_down: u64,
    busy: u64,
    too_large: u64,
    unsupported: u64,
    backend_error: u64,
    last_result: u8,
    has_result: bool,
};
const TransmitSentFn = *const fn ([*]const u8, usize) callconv(.c) void;
const TransmitCleanupFn = *const fn ([*]const u8, usize) callconv(.c) void;
const TransmitPeers = extern struct {
    sent: TransmitSentFn,
    cleanup: TransmitCleanupFn,
};
extern fn r4os_rust_net_tx_reset() void;
extern fn r4os_rust_net_tx_record(u8) void;
extern fn r4os_rust_net_tx_counters(*NativeTxCounters) void;
extern fn r4os_rust_net_transmit(usize, [*]const u8, usize, *const TransmitPeers) u8;

comptime {
    if (MAX_PACKET_SIZE != 1536 or @intFromEnum(TxResult.ok) != 0 or @intFromEnum(TxResult.no_adapter) != 1 or @intFromEnum(TxResult.link_down) != 2 or @intFromEnum(TxResult.busy) != 3 or @intFromEnum(TxResult.too_large) != 4 or @intFromEnum(TxResult.unsupported) != 5 or @intFromEnum(TxResult.backend_error) != 6) @compileError("Native TX result/frame policy");
}

comptime {
    if (@sizeOf(NativeTxCounters) != 64 or @alignOf(NativeTxCounters) != 8 or @offsetOf(NativeTxCounters, "failures") != 0 or @offsetOf(NativeTxCounters, "no_adapter") != 8 or @offsetOf(NativeTxCounters, "link_down") != 16 or @offsetOf(NativeTxCounters, "busy") != 24 or @offsetOf(NativeTxCounters, "too_large") != 32 or @offsetOf(NativeTxCounters, "unsupported") != 40 or @offsetOf(NativeTxCounters, "backend_error") != 48 or @offsetOf(NativeTxCounters, "last_result") != 56 or @offsetOf(NativeTxCounters, "has_result") != 57) @compileError("Native network NativeTxCounters layout");
}

comptime {
    if (@sizeOf(TransmitPeers) != 16 or @alignOf(TransmitPeers) != 8 or @offsetOf(TransmitPeers, "sent") != 0 or @offsetOf(TransmitPeers, "cleanup") != 8) @compileError("Native network TransmitPeers layout");
}

const OwnedIpv4Packet = extern struct {
    protocol: u8,
    source_ip: [4]u8,
    dest_ip: [4]u8,
    payload_len: usize,
    data: [1536]u8,
};
const OwnedUdpPacket = extern struct {
    source_ip: [4]u8,
    dest_ip: [4]u8,
    source_port: u16,
    dest_port: u16,
    length: u16,
    payload_len: usize,
    data: [1536]u8,
};
extern fn r4os_rust_wire_ipv4_view(*const r4p_contract.Ipv4Op, *OwnedIpv4Packet) void;
extern fn r4os_rust_wire_udp_view(*const r4p_contract.UdpOp, *OwnedUdpPacket) void;

comptime {
    if (@sizeOf(OwnedIpv4Packet) != 1560 or @alignOf(OwnedIpv4Packet) != 8 or @offsetOf(OwnedIpv4Packet, "protocol") != 0 or @offsetOf(OwnedIpv4Packet, "source_ip") != 1 or @offsetOf(OwnedIpv4Packet, "dest_ip") != 5 or @offsetOf(OwnedIpv4Packet, "payload_len") != 16 or @offsetOf(OwnedIpv4Packet, "data") != 24) @compileError("Native network OwnedIpv4Packet layout");
}

comptime {
    if (@sizeOf(OwnedUdpPacket) != 1560 or @alignOf(OwnedUdpPacket) != 8 or @offsetOf(OwnedUdpPacket, "source_ip") != 0 or @offsetOf(OwnedUdpPacket, "dest_ip") != 4 or @offsetOf(OwnedUdpPacket, "source_port") != 8 or @offsetOf(OwnedUdpPacket, "dest_port") != 10 or @offsetOf(OwnedUdpPacket, "length") != 12 or @offsetOf(OwnedUdpPacket, "payload_len") != 16 or @offsetOf(OwnedUdpPacket, "data") != 24) @compileError("Native network OwnedUdpPacket layout");
}

extern fn r4os_rust_net_protocol_runtime_reset() void;
extern fn r4os_rust_net_protocol_runtime(u8, *ProtocolRuntimeStats) void;
extern fn r4os_rust_net_r4p_runtime(*R4pRuntimeStatus) void;
extern fn r4os_rust_net_protocol_role(u8, *NativeBytesText) void;
extern fn r4os_rust_net_tcp_wire_error_set(NativeBytesText) void;
extern fn r4os_rust_net_tcp_wire_error(*NativeBytesText) void;
extern fn r4os_rust_net_tcp_wire_flags_set(u16) void;
extern fn r4os_rust_net_tcp_wire_result(i32, u16) void;
extern fn r4os_rust_net_tcp_wire_reject(i32) void;

extern fn r4os_rust_net_note_ethernet_r4p_rx(u64) void;
extern fn r4os_rust_net_note_ethernet_r4p_tx(u64) void;
extern fn r4os_rust_net_note_ethernet_r4p_build(u64) void;
extern fn r4os_rust_net_note_ethernet_dispatch_failures(u64) void;
extern fn r4os_rust_net_note_arp_r4p_rx(u64) void;
extern fn r4os_rust_net_note_arp_r4p_tx(u64) void;
extern fn r4os_rust_net_note_arp_r4p_build(u64) void;
extern fn r4os_rust_net_note_arp_dispatch_failures(u64) void;
extern fn r4os_rust_net_note_ipv4_r4p_rx(u64) void;
extern fn r4os_rust_net_note_ipv4_r4p_tx(u64) void;
extern fn r4os_rust_net_note_ipv4_r4p_build(u64) void;
extern fn r4os_rust_net_note_ipv4_dispatch_failures(u64) void;
extern fn r4os_rust_net_note_icmp_r4p_rx(u64) void;
extern fn r4os_rust_net_note_icmp_r4p_tx(u64) void;
extern fn r4os_rust_net_note_icmp_r4p_build(u64) void;
extern fn r4os_rust_net_note_icmp_r4p_classify(u64) void;
extern fn r4os_rust_net_note_icmp_dispatch_failures(u64) void;
extern fn r4os_rust_net_note_udp_r4p_rx(u64) void;
extern fn r4os_rust_net_note_udp_r4p_tx(u64) void;
extern fn r4os_rust_net_note_udp_r4p_build(u64) void;
extern fn r4os_rust_net_note_udp_dispatch_failures(u64) void;
extern fn r4os_rust_net_note_dhcp_r4p_rx(u64) void;
extern fn r4os_rust_net_note_dhcp_r4p_build(u64) void;
extern fn r4os_rust_net_note_dhcp_dispatch_failures(u64) void;
extern fn r4os_rust_net_note_dns_r4p_rx(u64) void;
extern fn r4os_rust_net_note_dns_r4p_build(u64) void;
extern fn r4os_rust_net_note_dns_dispatch_failures(u64) void;
extern fn r4os_rust_net_note_tcp_r4p_rx(u64) void;
extern fn r4os_rust_net_note_tcp_r4p_tx(u64) void;
extern fn r4os_rust_net_note_tcp_r4p_control(u64) void;
extern fn r4os_rust_net_note_tcp_r4p_build(u64) void;
extern fn r4os_rust_net_note_tcp_dispatch_failures(u64) void;
comptime {
    if (@intFromEnum(ProtocolKind.ethernet) != 0 or @intFromEnum(ProtocolKind.arp) != 1 or @intFromEnum(ProtocolKind.ipv4) != 2 or @intFromEnum(ProtocolKind.icmp) != 3 or @intFromEnum(ProtocolKind.udp) != 4 or @intFromEnum(ProtocolKind.dhcp) != 5 or @intFromEnum(ProtocolKind.dns) != 6 or @intFromEnum(ProtocolKind.tcp) != 7 or @intFromEnum(ProtocolKind.serial_link) != 8 or FALLBACK_POLICY_NONE != 0 or FALLBACK_DECISION_NONE != 0 or R4P_RUNTIME_PROTOCOLS.len != 9) @compileError("Native protocol runtime kinds/policy");
}

comptime {
    if (@sizeOf(ProtocolRuntimeStats) != 56 or @alignOf(ProtocolRuntimeStats) != 8 or @offsetOf(ProtocolRuntimeStats, "active_r4p") != 0 or @offsetOf(ProtocolRuntimeStats, "r4p_state") != 1 or @offsetOf(ProtocolRuntimeStats, "builtin_fallback") != 2 or @offsetOf(ProtocolRuntimeStats, "fallback_policy") != 3 or @offsetOf(ProtocolRuntimeStats, "fallback_decision") != 4 or @offsetOf(ProtocolRuntimeStats, "r4p_rx") != 8 or @offsetOf(ProtocolRuntimeStats, "r4p_tx") != 16 or @offsetOf(ProtocolRuntimeStats, "r4p_control") != 24 or @offsetOf(ProtocolRuntimeStats, "r4p_build") != 32 or @offsetOf(ProtocolRuntimeStats, "r4p_classify") != 40 or @offsetOf(ProtocolRuntimeStats, "dispatch_failures") != 48) @compileError("Native network ProtocolRuntimeStats layout");
}

comptime {
    if (@sizeOf(R4pRuntimeStatus) != 64 or @alignOf(R4pRuntimeStatus) != 8 or @offsetOf(R4pRuntimeStatus, "protocol_count") != 0 or @offsetOf(R4pRuntimeStatus, "active") != 4 or @offsetOf(R4pRuntimeStatus, "missing") != 8 or @offsetOf(R4pRuntimeStatus, "r4p_rx") != 16 or @offsetOf(R4pRuntimeStatus, "r4p_tx") != 24 or @offsetOf(R4pRuntimeStatus, "r4p_control") != 32 or @offsetOf(R4pRuntimeStatus, "r4p_build") != 40 or @offsetOf(R4pRuntimeStatus, "r4p_classify") != 48 or @offsetOf(R4pRuntimeStatus, "dispatch_failures") != 56) @compileError("Native network R4pRuntimeStatus layout");
}

const RxProcessFn = *const fn (usize, [*]const u8, usize, *const rx_handoff.Metadata) callconv(.c) bool;
const RxMaintenanceFn = *const fn (u64) callconv(.c) void;
const RxLogFn = *const fn () callconv(.c) void;
const RxPeers = extern struct {
    process: RxProcessFn,
    maintenance: RxMaintenanceFn,
    log: RxLogFn,
};
const rx_peers = RxPeers{ .process = rxProcess, .maintenance = rxMaintenance, .log = rxLog };
fn rxProcess(index: usize, data: [*]const u8, len: usize, metadata: *const rx_handoff.Metadata) callconv(.c) bool {
    return processReceivedFrame(index, data[0..len], metadata.*);
}
fn rxMaintenance(now: u64) callconv(.c) void {
    r4os_rust_tcp_client_maintenance(now);
}
fn rxLog() callconv(.c) void {
    logRxTaskStatus();
}
const RxPollStatus = extern struct {
    adapter_poll_rounds: u64,
    service_poll_requests: u64,
    service_poll_skips: u64,
};
extern fn r4os_rust_net_rx_runtime_reset() void;
extern fn r4os_rust_net_rx_runtime_schedule(usize) bool;
extern fn r4os_rust_net_rx_runtime_receive(usize, u16, u64, [*]const u8, usize, bool) u8;
extern fn r4os_rust_net_rx_runtime_process_one(*const RxPeers) bool;
extern fn r4os_rust_net_rx_runtime_process_batch(usize, *const RxPeers) usize;
extern fn r4os_rust_net_rx_runtime_has_ready() bool;
extern fn r4os_rust_net_rx_runtime_take_scheduled() u16;
extern fn r4os_rust_net_rx_runtime_cancel() usize;
extern fn r4os_rust_net_rx_runtime_poll(usize, u16, *const RxPeers) void;
extern fn r4os_rust_net_rx_runtime_poll_without_task(usize, *const RxPeers) void;
extern fn r4os_rust_net_rx_runtime_service_poll(usize, *const RxPeers) void;
extern fn r4os_rust_net_rx_runtime_is_current() bool;
extern fn r4os_rust_net_rx_runtime_summary(*RxTaskSummary) void;
extern fn r4os_rust_net_rx_runtime_start(*const RxPeers) bool;
extern fn r4os_rust_net_rx_runtime_l4(u8, bool) void;
extern fn r4os_rust_net_rx_runtime_loss_drop() bool;
extern fn r4os_rust_net_rx_runtime_loss_drops() u64;
extern fn r4os_rust_net_rx_runtime_poll_status(*RxPollStatus) void;

comptime {
    if (MAX_ADAPTERS != 8 or MAX_PACKET_SIZE != 1536 or RX_HANDOFF_QUEUE_SIZE != 64 or RX_HANDOFF_BATCH_BUDGET != 32 or RX_NESTED_PROCESS_LIMIT != 4 or ALL_ADAPTER_MASK != 255 or RX_TASK_POLL_ROUNDS != 1 or RX_TASK_FALLBACK_TICKS != timing.msToTicks(10) or RX_TASK_LOG_INTERVAL != 2000 or NET_LOSS_EVERY_N != 64 or @intFromEnum(RxAdmission.accepted) != 0 or @intFromEnum(RxAdmission.accepted_fallback) != 1 or @intFromEnum(RxAdmission.invalid_adapter) != 2 or @intFromEnum(RxAdmission.invalid_frame) != 3 or @intFromEnum(RxAdmission.unavailable) != 4 or @intFromEnum(RxAdmission.queue_busy) != 5 or @intFromEnum(RxAdmission.irq_context) != 6) @compileError("Native RX policy/return codes");
}

comptime {
    if (@sizeOf(RxTaskSummary) != 288 or @alignOf(RxTaskSummary) != 8 or @offsetOf(RxTaskSummary, "started") != 0 or @offsetOf(RxTaskSummary, "task_id") != 4 or @offsetOf(RxTaskSummary, "task_generation") != 8 or @offsetOf(RxTaskSummary, "polls") != 16 or @offsetOf(RxTaskSummary, "iterations") != 24 or @offsetOf(RxTaskSummary, "guard_skips") != 32 or @offsetOf(RxTaskSummary, "nested_polls") != 40 or @offsetOf(RxTaskSummary, "queue_capacity") != 48 or @offsetOf(RxTaskSummary, "queue_ready") != 52 or @offsetOf(RxTaskSummary, "queue_occupied") != 56 or @offsetOf(RxTaskSummary, "queue_high_water") != 60 or @offsetOf(RxTaskSummary, "accepted") != 64 or @offsetOf(RxTaskSummary, "processed") != 72 or @offsetOf(RxTaskSummary, "cancelled") != 80 or @offsetOf(RxTaskSummary, "queue_busy") != 88 or @offsetOf(RxTaskSummary, "protocol_success") != 96 or @offsetOf(RxTaskSummary, "protocol_failures") != 104 or @offsetOf(RxTaskSummary, "schedules") != 112 or @offsetOf(RxTaskSummary, "schedules_from_irq") != 120 or @offsetOf(RxTaskSummary, "schedule_coalesced") != 128 or @offsetOf(RxTaskSummary, "event_wakeups") != 136 or @offsetOf(RxTaskSummary, "fallback_timeouts") != 144 or @offsetOf(RxTaskSummary, "batches") != 152 or @offsetOf(RxTaskSummary, "batch_max") != 160 or @offsetOf(RxTaskSummary, "budget_exhaustions") != 168 or @offsetOf(RxTaskSummary, "handoff_latency_samples") != 176 or @offsetOf(RxTaskSummary, "handoff_latency_last_ns") != 184 or @offsetOf(RxTaskSummary, "handoff_latency_max_ns") != 192 or @offsetOf(RxTaskSummary, "schedule_latency_samples") != 200 or @offsetOf(RxTaskSummary, "schedule_latency_last_ns") != 208 or @offsetOf(RxTaskSummary, "schedule_latency_max_ns") != 216 or @offsetOf(RxTaskSummary, "irq_frame_rejects") != 224 or @offsetOf(RxTaskSummary, "release_failures") != 232 or @offsetOf(RxTaskSummary, "metadata_submissions") != 240 or @offsetOf(RxTaskSummary, "metadata_selected") != 248 or @offsetOf(RxTaskSummary, "metadata_fallbacks") != 256 or @offsetOf(RxTaskSummary, "l4_offload_used") != 264 or @offsetOf(RxTaskSummary, "l4_software_checks") != 272 or @offsetOf(RxTaskSummary, "ownership_balanced") != 280) @compileError("Native network RxTaskSummary layout");
}

comptime {
    if (@sizeOf(RxPollStatus) != 24 or @alignOf(RxPollStatus) != 8 or @offsetOf(RxPollStatus, "adapter_poll_rounds") != 0 or @offsetOf(RxPollStatus, "service_poll_requests") != 8 or @offsetOf(RxPollStatus, "service_poll_skips") != 16) @compileError("Native network RxPollStatus layout");
}

comptime {
    if (@sizeOf(RxPeers) != 24 or @alignOf(RxPeers) != 8 or @offsetOf(RxPeers, "process") != 0 or @offsetOf(RxPeers, "maintenance") != 8 or @offsetOf(RxPeers, "log") != 16) @compileError("Native network RxPeers layout");
}

extern fn r4os_rust_net_tcp_activity_signal() void;
extern fn r4os_rust_net_tcp_activity_count() u64;
extern fn r4os_rust_net_tcp_wait_established(u32, u64) void;
extern fn r4os_rust_net_tcp_wait_readable(u32, u64) void;
extern fn r4os_rust_net_tcp_wait_closed(u32, u64) void;
extern fn r4os_rust_net_tcp_wait_port_data(u16, u64) void;
extern fn r4os_rust_net_tcp_wait_port_connection(u16, u64) void;

const DhcpPeers = extern struct {
    refresh: *const fn (usize) callconv(.c) void,
    poll: *const fn (usize) callconv(.c) void,
    idle: *const fn () callconv(.c) void,
    bind: *const fn (u16) callconv(.c) i32,
    close: *const fn (u32) callconv(.c) i32,
    recv: *const fn (u32, *UdpRecvInfo, [*]u8, usize) callconv(.c) i32,
    send: *const fn (usize, u32, *const [4]u8, *const [4]u8, [*]const u8, usize) callconv(.c) u8,
    arp_flush: *const fn () callconv(.c) void,
};
const dhcp_peers = DhcpPeers{ .refresh = dhcpPeerRefresh, .poll = dhcpPeerPoll, .idle = udpWaitIdle, .bind = dhcpPeerBind, .close = dhcpPeerClose, .recv = udpWaitRecv, .send = dhcpPeerSend, .arp_flush = dhcpPeerArpFlush };
fn dhcpPeerRefresh(index: usize) callconv(.c) void {
    _ = refreshAdapterRuntime(index);
}
fn dhcpPeerPoll(rounds: usize) callconv(.c) void {
    pollAdapters(rounds);
}
fn dhcpPeerBind(port: u16) callconv(.c) i32 {
    return udpBind(port);
}
fn dhcpPeerClose(handle: u32) callconv(.c) i32 {
    return udpClose(handle);
}
fn dhcpPeerSend(index: usize, socket: u32, source: *const [4]u8, dest: *const [4]u8, data: [*]const u8, len: usize) callconv(.c) u8 {
    return @intCast(@intFromEnum(udpSendDhcpFromAdapter(socket, index, source.*, dest.*, data[0..len])));
}
fn dhcpPeerArpFlush() callconv(.c) void {
    arpFlush();
}
extern fn r4os_rust_dhcp_client_reset(*const DhcpPeers) void;
extern fn r4os_rust_dhcp_client_start() bool;
extern fn r4os_rust_dhcp_client_retry_task() void;
extern fn r4os_rust_dhcp_client_cancel(SocketText) bool;
extern fn r4os_rust_dhcp_client_invalidate_lease() void;
extern fn r4os_rust_dhcp_client_runtime(*DhcpRuntimeStatus) void;
extern fn r4os_rust_dhcp_client_lease_timing(*DhcpLeaseTiming) void;
extern fn r4os_rust_dhcp_client_message([*]const u8, usize) bool;
extern fn r4os_rust_dhcp_client_acquire(usize, bool, u64) u8;
extern fn r4os_rust_dhcp_client_renew(usize, bool, u64) u8;
extern fn r4os_rust_dhcp_client_release(usize) u8;
extern fn r4os_rust_dhcp_client_acquire_default(bool, u64) u8;
extern fn r4os_rust_dhcp_client_renew_default(bool, u64) u8;
extern fn r4os_rust_dhcp_client_release_default() u8;

comptime {
    if (@sizeOf(DhcpLeaseTiming) != 32 or @alignOf(DhcpLeaseTiming) != 8 or @offsetOf(DhcpLeaseTiming, "bound") != 0 or @offsetOf(DhcpLeaseTiming, "acquired_tick") != 8 or @offsetOf(DhcpLeaseTiming, "elapsed_seconds") != 16 or @offsetOf(DhcpLeaseTiming, "remaining_seconds") != 20 or @offsetOf(DhcpLeaseTiming, "renew_in_seconds") != 24 or @offsetOf(DhcpLeaseTiming, "rebind_in_seconds") != 28) @compileError("Native network DhcpLeaseTiming layout");
}

comptime {
    if (@sizeOf(DhcpRuntimeStatus) != 88 or @alignOf(DhcpRuntimeStatus) != 8 or @offsetOf(DhcpRuntimeStatus, "state") != 0 or @offsetOf(DhcpRuntimeStatus, "desired_dhcp") != 2 or @offsetOf(DhcpRuntimeStatus, "link_up") != 3 or @offsetOf(DhcpRuntimeStatus, "task_started") != 4 or @offsetOf(DhcpRuntimeStatus, "task_id") != 8 or @offsetOf(DhcpRuntimeStatus, "task_generation") != 16 or @offsetOf(DhcpRuntimeStatus, "link_generation") != 24 or @offsetOf(DhcpRuntimeStatus, "operation_generation") != 28 or @offsetOf(DhcpRuntimeStatus, "transition_tick") != 32 or @offsetOf(DhcpRuntimeStatus, "next_retry_tick") != 40 or @offsetOf(DhcpRuntimeStatus, "last_timeout_tick") != 48 or @offsetOf(DhcpRuntimeStatus, "retry_round") != 56 or @offsetOf(DhcpRuntimeStatus, "operation_active") != 57 or @offsetOf(DhcpRuntimeStatus, "recoveries") != 64 or @offsetOf(DhcpRuntimeStatus, "starts") != 72 or @offsetOf(DhcpRuntimeStatus, "cancels") != 80) @compileError("Native network DhcpRuntimeStatus layout");
}

comptime {
    if (@sizeOf(DhcpPeers) != 64 or @alignOf(DhcpPeers) != 8 or @offsetOf(DhcpPeers, "refresh") != 0 or @offsetOf(DhcpPeers, "poll") != 8 or @offsetOf(DhcpPeers, "idle") != 16 or @offsetOf(DhcpPeers, "bind") != 24 or @offsetOf(DhcpPeers, "close") != 32 or @offsetOf(DhcpPeers, "recv") != 40 or @offsetOf(DhcpPeers, "send") != 48 or @offsetOf(DhcpPeers, "arp_flush") != 56) @compileError("Native DHCP peers layout");
}

const DnsPeers = extern struct {
    bind: *const fn (u16) callconv(.c) i32,
    close: *const fn (u32) callconv(.c) i32,
    recv: *const fn (u32, *UdpRecvInfo, [*]u8, usize) callconv(.c) i32,
    send: *const fn (u32, *const [4]u8, u16, [*]const u8, usize) callconv(.c) u8,
    poll: *const fn (usize) callconv(.c) void,
};
const dns_peers = DnsPeers{ .bind = dhcpPeerBind, .close = dhcpPeerClose, .recv = udpWaitRecv, .send = dnsPeerSend, .poll = dhcpPeerPoll };
fn dnsPeerSend(handle: u32, server: *const [4]u8, port: u16, data: [*]const u8, len: usize) callconv(.c) u8 {
    return @intCast(@intFromEnum(udpSendTo(handle, server.*, port, data[0..len])));
}
extern fn r4os_rust_dns_client_reset(*const DnsPeers) void;
extern fn r4os_rust_dns_client_resolve([*]const u8, usize, *[4]u8) i32;
extern fn r4os_rust_dns_client_resolve_server([*]const u8, usize, *const [4]u8, *[4]u8) i32;
extern fn r4os_rust_dns_client_response([*]const u8, usize) bool;
extern fn r4os_rust_dns_client_timeout([*]const u8, usize, *const [4]u8) i32;
extern fn r4os_rust_dns_client_cancel(SocketText) bool;
extern fn r4os_rust_dns_client_cache_count(bool) u32;
comptime {
    if (DNS_SOURCE_PORT_BASE != 49152 or DNS_TIMEOUT_TICKS != timing.msToTicks(4000) or DHCP_MAX_LOOPS != 200000 or DHCP_POLL_ROUNDS != 512 or DHCP_MAX_ATTEMPTS != 3 or DHCP_TIMEOUT_TICKS != timing.msToTicks(2000) or DHCP_COORDINATOR_POLL_TICKS != timing.msToTicks(100) or DHCP_RETRY_BASE_TICKS != timing.msToTicks(250) or DHCP_RETRY_MAX_TICKS != timing.msToTicks(4000) or DHCP_TASK_START_RETRY_TICKS != timing.msToTicks(1000) or dhcp.CLIENT_PORT != 68 or dhcp.SERVER_PORT != 67 or dns.PORT != 53 or @intFromEnum(Link.down) != 1 or @intFromEnum(Link.up) != 2 or sync.LockRank.local != 10) @compileError("Native DHCP/DNS policy");
}

comptime {
    if (@sizeOf(DnsPeers) != 40 or @alignOf(DnsPeers) != 8 or @offsetOf(DnsPeers, "bind") != 0 or @offsetOf(DnsPeers, "close") != 8 or @offsetOf(DnsPeers, "recv") != 16 or @offsetOf(DnsPeers, "send") != 24 or @offsetOf(DnsPeers, "poll") != 32) @compileError("Native DNS peers layout");
}

const ArpPeers = extern struct { request: *const fn (usize, *const [4]u8) callconv(.c) u8, poll: *const fn (usize) callconv(.c) void, idle: *const fn () callconv(.c) void };
const arp_peers = ArpPeers{ .request = arpPeerRequest, .poll = dhcpPeerPoll, .idle = udpWaitIdle };
fn arpPeerRequest(index: usize, target: *const [4]u8) callconv(.c) u8 {
    return @intCast(@intFromEnum(sendArpRequest(index, target.*)));
}
extern fn r4os_rust_net_arp_resolve(usize, *const [4]u8, *[6]u8, *const ArpPeers) u8;
extern fn r4os_rust_net_arp_wait(*const [4]u8, *const ArpPeers) bool;
extern fn r4os_rust_net_arp_negative_timeout() bool;
extern fn r4os_rust_net_arp_remember(*const [4]u8, *const [6]u8) void;
extern fn r4os_rust_net_arp_remember_stats() void;
extern fn r4os_rust_net_arp_lookup(*const [4]u8, *[6]u8) bool;
extern fn r4os_rust_net_arp_cache_count_owned() u32;
extern fn r4os_rust_net_arp_cache_expire_owned() void;
extern fn r4os_rust_net_arp_cache_flush_owned() void;
extern fn r4os_rust_net_arp_cache_age_owned() u64;
extern fn r4os_rust_net_ipv4_reject_large(u8, *const [4]u8, SocketText) void;
extern fn r4os_rust_net_protocol_stats_reset() void;
extern fn r4os_rust_net_buffer_small_note() void;
extern fn r4os_rust_net_buffer_small_count() u64;
extern fn r4os_rust_net_icmp_sequence() u16;
extern fn r4os_rust_net_icmp_sequence_advance() void;
comptime {
    if (ARP_RESOLVE_POLL_ROUNDS != 64 or ARP_RESOLVE_MAX_LOOPS != 200000 or ARP_RESOLVE_ATTEMPTS != 3 or ARP_PENDING_QUEUE_LIMIT != 0 or ARP_RESOLVE_TIMEOUT_TICKS != timing.msToTicks(500)) @compileError("Native ARP policy");
}
comptime {
    if (@sizeOf(ArpPeers) != 24 or @alignOf(ArpPeers) != 8 or @offsetOf(ArpPeers, "request") != 0 or @offsetOf(ArpPeers, "poll") != 8 or @offsetOf(ArpPeers, "idle") != 16) @compileError("Native ARP peers layout");
}

const NativeCleanupStatus = extern struct {
    runs: u64,
    link_down_cleanups: u64,
    adapter_reset_cleanups: u64,
    adapter_unregister_cleanups: u64,
    service_restart_cleanups: u64,
    poweroff_cleanups: u64,
    reboot_cleanups: u64,
    udp_sockets_closed: u64,
    tcp_connections_aborted: u64,
    tcp_listeners_closed: u64,
    dhcp_operations_cancelled: u64,
    dns_operations_cancelled: u64,
    last_udp_closed: u32,
    last_tcp_connections: u32,
    last_tcp_listeners: u32,
    last_reason: SocketText,
};
const NativeDiagnosticCounters = extern struct {
    packet_corpus_tests: u64,
    packet_corpus_cases: u64,
    negative_path_tests: u64,
    negative_path_cases: u64,
    driver_lifecycle_tests: u64,
    driver_lifecycle_cases: u64,
    environment_contract_tests: u64,
    environment_contract_cases: u64,
    limit_contract_tests: u64,
    limit_contract_cases: u64,
    power_lifecycle_tests: u64,
    power_lifecycle_cases: u64,
    link_up: bool,
};

fn cleanupFromNative(value: NativeCleanupStatus) CleanupStatus {
    return .{
        .runs = value.runs,
        .link_down_cleanups = value.link_down_cleanups,
        .adapter_reset_cleanups = value.adapter_reset_cleanups,
        .adapter_unregister_cleanups = value.adapter_unregister_cleanups,
        .service_restart_cleanups = value.service_restart_cleanups,
        .poweroff_cleanups = value.poweroff_cleanups,
        .reboot_cleanups = value.reboot_cleanups,
        .udp_sockets_closed = value.udp_sockets_closed,
        .tcp_connections_aborted = value.tcp_connections_aborted,
        .tcp_listeners_closed = value.tcp_listeners_closed,
        .dhcp_operations_cancelled = value.dhcp_operations_cancelled,
        .dns_operations_cancelled = value.dns_operations_cancelled,
        .last_udp_closed = value.last_udp_closed,
        .last_tcp_connections = value.last_tcp_connections,
        .last_tcp_listeners = value.last_tcp_listeners,
        .last_reason = value.last_reason.slice(),
    };
}

fn nativeDiagnosticCounters() NativeDiagnosticCounters {
    var out: NativeDiagnosticCounters = undefined;
    r4os_rust_net_diagnostic_counters(&out);
    return out;
}
extern fn r4os_rust_net_cleanup_reset() void;
extern fn r4os_rust_net_cleanup_all(SocketText, *NativeCleanupStatus) void;
extern fn r4os_rust_net_cleanup_scoped(SocketText, *const DiagnosticCleanupScope, *NativeCleanupStatus) void;
extern fn r4os_rust_net_cleanup_status(*NativeCleanupStatus) void;
extern fn r4os_rust_net_cleanup_transition(SocketText) bool;
extern fn r4os_rust_net_diagnostic_counters(*NativeDiagnosticCounters) void;
extern fn r4os_rust_net_diagnostic_link_set(bool) void;
extern fn r4os_rust_net_diagnostic_link() bool;
extern fn r4os_rust_net_note_packet_corpus(u64) void;
extern fn r4os_rust_net_note_negative_path(u64) void;
extern fn r4os_rust_net_note_environment_contract(u64) void;
extern fn r4os_rust_net_note_limit_contract(u64) void;
extern fn r4os_rust_net_note_power_lifecycle(u64) void;
extern fn r4os_rust_net_note_driver_lifecycle() void;
extern fn r4os_rust_net_note_driver_environment() void;
extern fn r4os_rust_net_note_driver_limit() void;

comptime {
    if (@intFromEnum(Lifecycle.shutdown) != 5) @compileError("Native terminal lifecycle");
}

comptime {
    if (@sizeOf(NativeCleanupStatus) != 128 or @alignOf(NativeCleanupStatus) != 8 or @offsetOf(NativeCleanupStatus, "runs") != 0 or @offsetOf(NativeCleanupStatus, "link_down_cleanups") != 8 or @offsetOf(NativeCleanupStatus, "adapter_reset_cleanups") != 16 or @offsetOf(NativeCleanupStatus, "adapter_unregister_cleanups") != 24 or @offsetOf(NativeCleanupStatus, "service_restart_cleanups") != 32 or @offsetOf(NativeCleanupStatus, "poweroff_cleanups") != 40 or @offsetOf(NativeCleanupStatus, "reboot_cleanups") != 48 or @offsetOf(NativeCleanupStatus, "udp_sockets_closed") != 56 or @offsetOf(NativeCleanupStatus, "tcp_connections_aborted") != 64 or @offsetOf(NativeCleanupStatus, "tcp_listeners_closed") != 72 or @offsetOf(NativeCleanupStatus, "dhcp_operations_cancelled") != 80 or @offsetOf(NativeCleanupStatus, "dns_operations_cancelled") != 88 or @offsetOf(NativeCleanupStatus, "last_udp_closed") != 96 or @offsetOf(NativeCleanupStatus, "last_tcp_connections") != 100 or @offsetOf(NativeCleanupStatus, "last_tcp_listeners") != 104 or @offsetOf(NativeCleanupStatus, "last_reason") != 112) @compileError("Native network NativeCleanupStatus layout");
}

comptime {
    if (@sizeOf(NativeDiagnosticCounters) != 104 or @alignOf(NativeDiagnosticCounters) != 8 or @offsetOf(NativeDiagnosticCounters, "packet_corpus_tests") != 0 or @offsetOf(NativeDiagnosticCounters, "packet_corpus_cases") != 8 or @offsetOf(NativeDiagnosticCounters, "negative_path_tests") != 16 or @offsetOf(NativeDiagnosticCounters, "negative_path_cases") != 24 or @offsetOf(NativeDiagnosticCounters, "driver_lifecycle_tests") != 32 or @offsetOf(NativeDiagnosticCounters, "driver_lifecycle_cases") != 40 or @offsetOf(NativeDiagnosticCounters, "environment_contract_tests") != 48 or @offsetOf(NativeDiagnosticCounters, "environment_contract_cases") != 56 or @offsetOf(NativeDiagnosticCounters, "limit_contract_tests") != 64 or @offsetOf(NativeDiagnosticCounters, "limit_contract_cases") != 72 or @offsetOf(NativeDiagnosticCounters, "power_lifecycle_tests") != 80 or @offsetOf(NativeDiagnosticCounters, "power_lifecycle_cases") != 88 or @offsetOf(NativeDiagnosticCounters, "link_up") != 96) @compileError("Native network NativeDiagnosticCounters layout");
}

comptime {
    if (@sizeOf(DiagnosticCleanupScope) != 12 or @alignOf(DiagnosticCleanupScope) != 4 or @offsetOf(DiagnosticCleanupScope, "udp_handle") != 0 or @offsetOf(DiagnosticCleanupScope, "tcp_connection_id") != 4 or @offsetOf(DiagnosticCleanupScope, "tcp_listener_port") != 8) @compileError("Native network DiagnosticCleanupScope layout");
}

const TcpClientPeers = extern struct {
    send_ipv4: *const fn (*const [4]u8, u8, [*]const u8, usize) callconv(.c) u8,
    poll: *const fn (usize) callconv(.c) void,
};
const TcpSendResult = extern struct { available: bool, result: u8 };
const tcp_client_peers = TcpClientPeers{ .send_ipv4 = tcpClientSendIpv4, .poll = tcpClientPoll };
fn tcpClientSendIpv4(target: *const [4]u8, protocol: u8, data: [*]const u8, len: usize) callconv(.c) u8 {
    return @intCast(@intFromEnum(sendIpv4Payload(target.*, protocol, data[0..len])));
}
fn tcpClientPoll(rounds: usize) callconv(.c) void {
    pollAdaptersWithoutRxTask(rounds);
}
extern fn r4os_rust_tcp_client_reset(*const TcpClientPeers) void;
extern fn r4os_rust_tcp_client_connect(*const [4]u8, u16, bool, u64) i32;
extern fn r4os_rust_tcp_client_write(u32, [*]const u8, usize) i32;
extern fn r4os_rust_tcp_client_read(u32, [*]u8, usize) i32;
extern fn r4os_rust_tcp_client_read_available(u32, [*]u8, usize) i32;
extern fn r4os_rust_tcp_client_echo_once(u16, [*]u8, usize) i32;
extern fn r4os_rust_tcp_client_accept_read_once(u16, [*]u8, usize, *u32) i32;
extern fn r4os_rust_tcp_client_accept_read(u16, [*]u8, usize, *u32) i32;
extern fn r4os_rust_tcp_client_accept(u16, *u32) i32;
extern fn r4os_rust_tcp_client_accept_poll(u16, *u32) i32;
extern fn r4os_rust_tcp_client_close(u32) i32;
extern fn r4os_rust_tcp_client_abort(u32) i32;
extern fn r4os_rust_tcp_client_retransmit_last(u32) i32;
extern fn r4os_rust_tcp_client_maintenance(u64) void;
extern fn r4os_rust_tcp_client_send(u32, u16, [*]const u8, usize, *TcpSendResult) void;
extern fn r4os_rust_tcp_client_retransmit(u32, u8, SocketText) u8;
extern fn r4os_rust_tcp_client_next_seq(*const [4]u8, u16) u32;
comptime {
    if (TCP_POLL_ROUNDS != 32 or TCP_MAX_RETRANSMITS != 5 or TCP_WAIT_SLICE_TICKS != timing.msToTicks(250) or TCP_LISTEN_TIMEOUT_TICKS != timing.msToTicks(10000) or SERVICE_OPERATION_TIMEOUT_TICKS != timing.msToTicks(4000) or @intFromEnum(tcp.RetransmitKind.any) != 0 or @intFromEnum(tcp.RetransmitKind.data) != 1 or @intFromEnum(tcp.RetransmitKind.syn) != 2 or @intFromEnum(tcp.RetransmitKind.fin) != 3) @compileError("Native TCP transport policy");
}

comptime {
    if (@sizeOf(TcpClientPeers) != 16 or @alignOf(TcpClientPeers) != 8 or @offsetOf(TcpClientPeers, "send_ipv4") != 0 or @offsetOf(TcpClientPeers, "poll") != 8) @compileError("Native TCP client paired layout");
}

comptime {
    if (@sizeOf(TcpSendResult) != 2 or @alignOf(TcpSendResult) != 1 or @offsetOf(TcpSendResult, "available") != 0 or @offsetOf(TcpSendResult, "result") != 1) @compileError("Native TCP client paired layout");
}

const NativeIpView = extern struct {
    protocol: u8,
    source_ip: [4]u8,
    dest_ip: [4]u8,
    payload: SocketText,
};

extern fn r4os_rust_net_tcp_wire_receive(*const NativeIpView, bool) void;
extern fn r4os_rust_net_tcp_wire_transmitted(*const NativeIpView) void;
comptime {
    if (TCP_DELAYED_ACK_MS != 40 or TCP_DELAYED_ACK_TICKS != timing.msToTicks(40)) @compileError("Native TCP delayed ACK policy");
}

comptime {
    if (@sizeOf(NativeIpView) != 32 or @alignOf(NativeIpView) != 8 or @offsetOf(NativeIpView, "protocol") != 0 or @offsetOf(NativeIpView, "source_ip") != 1 or @offsetOf(NativeIpView, "dest_ip") != 5 or @offsetOf(NativeIpView, "payload") != 16) @compileError("Native network NativeIpView layout");
}

extern fn r4os_rust_packet_build_ethernet(*const WireEthernetBuild, [*]u8, usize) usize;
extern fn r4os_rust_packet_build_arp(*const WireArpBuild, [*]u8, usize) usize;
extern fn r4os_rust_packet_build_ipv4(*const WireIpv4Build, [*]const u8, usize, [*]u8, usize) usize;
extern fn r4os_rust_packet_build_icmp_request(*const WireIcmpBuild, [*]u8, usize) usize;
extern fn r4os_rust_packet_build_icmp_reply([*]const u8, usize, [*]u8, usize) usize;
extern fn r4os_rust_packet_build_udp(*const WireUdpBuild, [*]const u8, usize, [*]u8, usize) usize;
extern fn r4os_rust_packet_build_tcp(*const WireTcpBuild, [*]const u8, usize, [*]u8, usize) usize;
extern fn r4os_rust_packet_ethernet_rx(*const [6]u8, [*]const u8, usize) bool;
extern fn r4os_rust_packet_ethernet_tx([*]const u8, usize) bool;
extern fn r4os_rust_packet_ethernet_type([*]const u8, usize) u16;
extern fn r4os_rust_packet_arp_rx([*]const u8, usize) void;
extern fn r4os_rust_packet_arp_tx([*]const u8, usize) void;
extern fn r4os_rust_packet_icmp_echo([*]const u8, usize) bool;
extern fn r4os_rust_packet_ipv4_rx([*]const u8, usize, *OwnedIpv4Packet) bool;
extern fn r4os_rust_packet_ipv4_tx([*]const u8, usize, *OwnedIpv4Packet) bool;
extern fn r4os_rust_packet_udp_rx(*const NativeIpView, bool, *OwnedUdpPacket) bool;
extern fn r4os_rust_packet_udp_tx(*const NativeIpView, *OwnedUdpPacket) bool;
extern fn r4os_rust_packet_icmp_rx(*const NativeIpView) void;
extern fn r4os_rust_packet_icmp_tx(*const NativeIpView) void;

extern fn r4os_rust_packet_learn_ipv4(*const [4]u8, *const [6]u8) void;

const FramePeers = extern struct { rx: *const RxPeers, arp: *const ArpPeers };
const frame_peers = FramePeers{ .rx = &rx_peers, .arp = &arp_peers };
extern fn r4os_rust_frames_reset(*const FramePeers) void;
extern fn r4os_rust_frames_receive(usize, [*]const u8, usize, *const rx_handoff.Metadata) bool;
extern fn r4os_rust_frames_transmit(usize, [*]const u8, usize) u8;
extern fn r4os_rust_frames_send_ipv4(*const [4]u8, u8, [*]const u8, usize) u8;
extern fn r4os_rust_frames_send_arp(usize, *const [4]u8) u8;
extern fn r4os_rust_frames_arp_gateway() u8;
extern fn r4os_rust_frames_echo(*const [4]u8) u8;
extern fn r4os_rust_frames_udp_bind(u16) i32;
extern fn r4os_rust_frames_udp_close(u32) i32;
extern fn r4os_rust_frames_udp_send(u32, *const [4]u8, u16, [*]const u8, usize) u8;
extern fn r4os_rust_frames_udp_dhcp(u32, usize, *const [4]u8, *const [4]u8, [*]const u8, usize) u8;
extern fn r4os_rust_frames_udp_recv(u32, *UdpRecvInfo, [*]u8, usize) i32;
comptime {
    if (@sizeOf(FramePeers) != 16 or @alignOf(FramePeers) != 8 or @offsetOf(FramePeers, "rx") != 0 or @offsetOf(FramePeers, "arp") != 8 or ALL_ADAPTER_MASK != 255 or ethernet.HEADER_SIZE != 14 or ethernet.MIN_FRAME_SIZE != 60 or ipv4.HEADER_SIZE != 20 or UDP_SOCKET_PAYLOAD_MAX != 1472 or (Adapter{}).mtu != 1500) @compileError("Native frame/peer policy");
}

extern fn r4os_rust_net_adapter_runtime_refresh(usize, ?*const DiagnosticCleanupScope, *BackendStatus) bool;
extern fn r4os_rust_net_adapter_runtime_unregister(usize, ?*const DiagnosticCleanupScope) bool;
extern fn r4os_rust_net_adapter_runtime_discard(usize) bool;
extern fn r4os_rust_net_adapter_runtime_remove(usize) bool;
extern fn r4os_rust_net_adapter_runtime_reset(usize, SocketText, ?*const DiagnosticCleanupScope) bool;
comptime {
    if (@intFromEnum(Link.unknown) != 0 or @intFromEnum(Link.down) != 1 or @intFromEnum(Link.up) != 2 or @intFromEnum(Lifecycle.registered) != 1 or @intFromEnum(Lifecycle.active) != 2 or @intFromEnum(Lifecycle.link_down) != 3 or @intFromEnum(Lifecycle.resetting) != 4 or @intFromEnum(Lifecycle.shutdown) != 5 or (BackendStatus{}).irq_line != 255) @compileError("Native runtime adapter lifecycle/defaults");
}

const NativeBackpressureStatus = extern struct {
    packet_pool_used: u32,
    packet_pool_limit: u32,
    packet_drops: u64,
    app_ipv4_queued: u32,
    app_ipv4_queue_limit: u32,
    app_ipv4_drops: u64,
    udp_active_sockets: u32,
    udp_socket_limit: u32,
    udp_queued_packets: u32,
    udp_queue_limit_per_socket: u32,
    udp_queue_limit_total: u32,
    udp_drops: u64,
    udp_payload_max: u32,
    tcp_active_connections: u32,
    tcp_connection_limit: u32,
    tcp_active_listeners: u32,
    tcp_listener_limit: u32,
    tcp_buffer_size: u32,
    tcp_rx_drops: u64,
    ipc_service_channels: u32,
    ipc_service_handlers: u32,
    ipc_service_queued: u32,
    ipc_service_queue_limit: u32,
    ipc_service_drops: u64,
    ipc_service_message_max: u32,
    ipc_service_queue_depth: u32,
    tx_failures: u64,
    tx_no_adapter: u64,
    tx_link_down: u64,
    tx_busy: u64,
    tx_too_large: u64,
    tx_unsupported: u64,
    tx_backend_error: u64,
    tx_last_result: SocketText,
    nonblocking_empty_status: SocketText,
    resource_queue_full: u64,
    resource_packet_drops: u64,
    resource_buffer_small: u64,
    resource_retries: u64,
    resource_timeouts: u64,
    resource_cancels: u64,
    resource_backend_busy: u64,
};

const NativeErrorStatus = extern struct {
    total: u64,
    packet_errors: u64,
    service_errors: u64,
    adapter_errors: u64,
    tx_failures: u64,
    protocol_errors: u64,
    r4p_dispatch_failures: u64,
    last_adapter_error: SocketText,
    last_protocol_error: SocketText,
};

const NativeUdpStatus = extern struct {
    active_sockets: u32,
    max_sockets: u32,
    queued_packets: u32,
    queue_limit: u32,
    payload_max: u32,
    delivered: u64,
    drops: u64,
    last_error: SocketText,
};

fn BackpressureFromNative(value: NativeBackpressureStatus) BackpressureStatus {
    return .{
        .packet_pool_used = value.packet_pool_used,
        .packet_pool_limit = value.packet_pool_limit,
        .packet_drops = value.packet_drops,
        .app_ipv4_queued = value.app_ipv4_queued,
        .app_ipv4_queue_limit = value.app_ipv4_queue_limit,
        .app_ipv4_drops = value.app_ipv4_drops,
        .udp_active_sockets = value.udp_active_sockets,
        .udp_socket_limit = value.udp_socket_limit,
        .udp_queued_packets = value.udp_queued_packets,
        .udp_queue_limit_per_socket = value.udp_queue_limit_per_socket,
        .udp_queue_limit_total = value.udp_queue_limit_total,
        .udp_drops = value.udp_drops,
        .udp_payload_max = value.udp_payload_max,
        .tcp_active_connections = value.tcp_active_connections,
        .tcp_connection_limit = value.tcp_connection_limit,
        .tcp_active_listeners = value.tcp_active_listeners,
        .tcp_listener_limit = value.tcp_listener_limit,
        .tcp_buffer_size = value.tcp_buffer_size,
        .tcp_rx_drops = value.tcp_rx_drops,
        .ipc_service_channels = value.ipc_service_channels,
        .ipc_service_handlers = value.ipc_service_handlers,
        .ipc_service_queued = value.ipc_service_queued,
        .ipc_service_queue_limit = value.ipc_service_queue_limit,
        .ipc_service_drops = value.ipc_service_drops,
        .ipc_service_message_max = value.ipc_service_message_max,
        .ipc_service_queue_depth = value.ipc_service_queue_depth,
        .tx_failures = value.tx_failures,
        .tx_no_adapter = value.tx_no_adapter,
        .tx_link_down = value.tx_link_down,
        .tx_busy = value.tx_busy,
        .tx_too_large = value.tx_too_large,
        .tx_unsupported = value.tx_unsupported,
        .tx_backend_error = value.tx_backend_error,
        .tx_last_result = value.tx_last_result.slice(),
        .nonblocking_empty_status = value.nonblocking_empty_status.slice(),
        .resource_queue_full = value.resource_queue_full,
        .resource_packet_drops = value.resource_packet_drops,
        .resource_buffer_small = value.resource_buffer_small,
        .resource_retries = value.resource_retries,
        .resource_timeouts = value.resource_timeouts,
        .resource_cancels = value.resource_cancels,
        .resource_backend_busy = value.resource_backend_busy,
    };
}

fn ErrorFromNative(value: NativeErrorStatus) ErrorStatus {
    return .{
        .total = value.total,
        .packet_errors = value.packet_errors,
        .service_errors = value.service_errors,
        .adapter_errors = value.adapter_errors,
        .tx_failures = value.tx_failures,
        .protocol_errors = value.protocol_errors,
        .r4p_dispatch_failures = value.r4p_dispatch_failures,
        .last_adapter_error = value.last_adapter_error.slice(),
        .last_protocol_error = value.last_protocol_error.slice(),
    };
}

fn UdpFromNative(value: NativeUdpStatus) UdpStatus {
    return .{
        .active_sockets = value.active_sockets,
        .max_sockets = value.max_sockets,
        .queued_packets = value.queued_packets,
        .queue_limit = value.queue_limit,
        .payload_max = value.payload_max,
        .delivered = value.delivered,
        .drops = value.drops,
        .last_error = value.last_error.slice(),
    };
}

extern fn r4os_rust_net_telemetry_candidates() usize;
extern fn r4os_rust_net_telemetry_backpressure(*NativeBackpressureStatus) void;
extern fn r4os_rust_net_telemetry_errors(*NativeErrorStatus) void;
extern fn r4os_rust_net_telemetry_udp(*NativeUdpStatus) void;
extern fn r4os_rust_net_telemetry_tcp_performance(*TcpPerformance) void;
extern fn r4os_rust_net_telemetry_log_errors(SocketText) void;
extern fn r4os_rust_net_telemetry_log_rx() void;
export fn r4os_zig_net_console_hex(value: u64, digits: usize) callconv(.c) void {
    k.putHex(value, @intCast(digits));
}
comptime {
    if (PACKET_POOL_SIZE != 16 or APP_IPV4_QUEUE_SIZE * APP_IPV4_BUCKET_COUNT != 384 or UDP_SOCKET_MAX != 8 or UDP_SOCKET_QUEUE_SIZE != 4 or UDP_SOCKET_PAYLOAD_MAX != 1472 or tcp.MAX_LISTENERS != 4 or tcp.LOCAL_MSS != 1460 or tcp.SENT_SEGMENT_CAPACITY != 48 or TCP_DELAYED_ACK_MS != 40 or tcp.LOCAL_WINDOW_SCALE != 2 or ipc.CHANNEL_NET_DHCP != 2 or ipc.CHANNEL_NET_DNS != 3 or ipc.CHANNEL_NET_TCP != 4 or ipc.CHANNEL_NET_UDP != 5 or ipc.MAX_MESSAGE_SIZE != 4096 or ipc.QUEUE_DEPTH != 8 or @intFromEnum(timing.OperationStatus.would_block) != 6) @compileError("Native network telemetry capacities/statuses");
}

comptime {
    if (@sizeOf(NativeBackpressureStatus) != 272 or @alignOf(NativeBackpressureStatus) != 8 or @offsetOf(NativeBackpressureStatus, "packet_pool_used") != 0 or @offsetOf(NativeBackpressureStatus, "packet_pool_limit") != 4 or @offsetOf(NativeBackpressureStatus, "packet_drops") != 8 or @offsetOf(NativeBackpressureStatus, "app_ipv4_queued") != 16 or @offsetOf(NativeBackpressureStatus, "app_ipv4_queue_limit") != 20 or @offsetOf(NativeBackpressureStatus, "app_ipv4_drops") != 24 or @offsetOf(NativeBackpressureStatus, "udp_active_sockets") != 32 or @offsetOf(NativeBackpressureStatus, "udp_socket_limit") != 36 or @offsetOf(NativeBackpressureStatus, "udp_queued_packets") != 40 or @offsetOf(NativeBackpressureStatus, "udp_queue_limit_per_socket") != 44 or @offsetOf(NativeBackpressureStatus, "udp_queue_limit_total") != 48 or @offsetOf(NativeBackpressureStatus, "udp_drops") != 56 or @offsetOf(NativeBackpressureStatus, "udp_payload_max") != 64 or @offsetOf(NativeBackpressureStatus, "tcp_active_connections") != 68 or @offsetOf(NativeBackpressureStatus, "tcp_connection_limit") != 72 or @offsetOf(NativeBackpressureStatus, "tcp_active_listeners") != 76 or @offsetOf(NativeBackpressureStatus, "tcp_listener_limit") != 80 or @offsetOf(NativeBackpressureStatus, "tcp_buffer_size") != 84 or @offsetOf(NativeBackpressureStatus, "tcp_rx_drops") != 88 or @offsetOf(NativeBackpressureStatus, "ipc_service_channels") != 96 or @offsetOf(NativeBackpressureStatus, "ipc_service_handlers") != 100 or @offsetOf(NativeBackpressureStatus, "ipc_service_queued") != 104 or @offsetOf(NativeBackpressureStatus, "ipc_service_queue_limit") != 108 or @offsetOf(NativeBackpressureStatus, "ipc_service_drops") != 112 or @offsetOf(NativeBackpressureStatus, "ipc_service_message_max") != 120 or @offsetOf(NativeBackpressureStatus, "ipc_service_queue_depth") != 124 or @offsetOf(NativeBackpressureStatus, "tx_failures") != 128 or @offsetOf(NativeBackpressureStatus, "tx_no_adapter") != 136 or @offsetOf(NativeBackpressureStatus, "tx_link_down") != 144 or @offsetOf(NativeBackpressureStatus, "tx_busy") != 152 or @offsetOf(NativeBackpressureStatus, "tx_too_large") != 160 or @offsetOf(NativeBackpressureStatus, "tx_unsupported") != 168 or @offsetOf(NativeBackpressureStatus, "tx_backend_error") != 176 or @offsetOf(NativeBackpressureStatus, "tx_last_result") != 184 or @offsetOf(NativeBackpressureStatus, "nonblocking_empty_status") != 200 or @offsetOf(NativeBackpressureStatus, "resource_queue_full") != 216 or @offsetOf(NativeBackpressureStatus, "resource_packet_drops") != 224 or @offsetOf(NativeBackpressureStatus, "resource_buffer_small") != 232 or @offsetOf(NativeBackpressureStatus, "resource_retries") != 240 or @offsetOf(NativeBackpressureStatus, "resource_timeouts") != 248 or @offsetOf(NativeBackpressureStatus, "resource_cancels") != 256 or @offsetOf(NativeBackpressureStatus, "resource_backend_busy") != 264) @compileError("Native network NativeBackpressureStatus layout");
}

comptime {
    if (@sizeOf(NativeErrorStatus) != 88 or @alignOf(NativeErrorStatus) != 8 or @offsetOf(NativeErrorStatus, "total") != 0 or @offsetOf(NativeErrorStatus, "packet_errors") != 8 or @offsetOf(NativeErrorStatus, "service_errors") != 16 or @offsetOf(NativeErrorStatus, "adapter_errors") != 24 or @offsetOf(NativeErrorStatus, "tx_failures") != 32 or @offsetOf(NativeErrorStatus, "protocol_errors") != 40 or @offsetOf(NativeErrorStatus, "r4p_dispatch_failures") != 48 or @offsetOf(NativeErrorStatus, "last_adapter_error") != 56 or @offsetOf(NativeErrorStatus, "last_protocol_error") != 72) @compileError("Native network NativeErrorStatus layout");
}

comptime {
    if (@sizeOf(NativeUdpStatus) != 56 or @alignOf(NativeUdpStatus) != 8 or @offsetOf(NativeUdpStatus, "active_sockets") != 0 or @offsetOf(NativeUdpStatus, "max_sockets") != 4 or @offsetOf(NativeUdpStatus, "queued_packets") != 8 or @offsetOf(NativeUdpStatus, "queue_limit") != 12 or @offsetOf(NativeUdpStatus, "payload_max") != 16 or @offsetOf(NativeUdpStatus, "delivered") != 24 or @offsetOf(NativeUdpStatus, "drops") != 32 or @offsetOf(NativeUdpStatus, "last_error") != 40) @compileError("Native network NativeUdpStatus layout");
}

comptime {
    if (@sizeOf(TcpPerformance) != 200 or @alignOf(TcpPerformance) != 8 or @offsetOf(TcpPerformance, "local_mss") != 0 or @offsetOf(TcpPerformance, "catalog_capacity") != 4 or @offsetOf(TcpPerformance, "delayed_ack_ms") != 8 or @offsetOf(TcpPerformance, "local_window_scale") != 12 or @offsetOf(TcpPerformance, "outstanding_segments") != 16 or @offsetOf(TcpPerformance, "outstanding_bytes") != 20 or @offsetOf(TcpPerformance, "outstanding_segments_peak") != 24 or @offsetOf(TcpPerformance, "outstanding_bytes_peak") != 28 or @offsetOf(TcpPerformance, "write_calls") != 32 or @offsetOf(TcpPerformance, "write_requested_bytes") != 40 or @offsetOf(TcpPerformance, "write_completed_bytes") != 48 or @offsetOf(TcpPerformance, "write_segments") != 56 or @offsetOf(TcpPerformance, "write_partial") != 64 or @offsetOf(TcpPerformance, "remote_window_stalls") != 72 or @offsetOf(TcpPerformance, "catalog_stalls") != 80 or @offsetOf(TcpPerformance, "backend_busy_stalls") != 88 or @offsetOf(TcpPerformance, "pure_ack_tx") != 96 or @offsetOf(TcpPerformance, "delayed_ack_requests") != 104 or @offsetOf(TcpPerformance, "delayed_ack_tx") != 112 or @offsetOf(TcpPerformance, "immediate_ack_tx") != 120 or @offsetOf(TcpPerformance, "ack_coalesced") != 128 or @offsetOf(TcpPerformance, "ack_piggybacked") != 136 or @offsetOf(TcpPerformance, "window_update_tx") != 144 or @offsetOf(TcpPerformance, "adapter_poll_rounds") != 152 or @offsetOf(TcpPerformance, "service_poll_requests") != 160 or @offsetOf(TcpPerformance, "service_poll_skips") != 168 or @offsetOf(TcpPerformance, "retransmits") != 176 or @offsetOf(TcpPerformance, "mss_negotiated") != 184 or @offsetOf(TcpPerformance, "window_scale_negotiated") != 192) @compileError("Native network TcpPerformance layout");
}

extern fn r4os_rust_net_diag_cleanup() bool;

extern fn r4os_rust_net_diag_power() bool;

extern fn r4os_rust_net_diag_link() bool;

extern fn r4os_rust_net_diag_reset() bool;

extern fn r4os_rust_net_diag_driver() bool;

extern fn r4os_rust_net_diag_rx() bool;

extern fn r4os_rust_net_diag_capability() bool;

extern fn r4os_rust_net_diag_environment() bool;

extern fn r4os_rust_net_diag_backpressure() bool;

extern fn r4os_rust_net_diag_limit() bool;

extern fn r4os_rust_net_diag_timing() bool;

extern fn r4os_rust_net_diag_corpus() bool;

extern fn r4os_rust_net_diag_negative() bool;

extern fn r4os_rust_net_diag_r4p() bool;
