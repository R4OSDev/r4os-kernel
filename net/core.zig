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

pub const ProtocolRuntimeStats = struct {
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

pub const R4pRuntimeStatus = struct {
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

pub const DhcpLeaseTiming = struct {
    bound: bool = false,
    acquired_tick: u64 = 0,
    elapsed_seconds: u32 = 0,
    remaining_seconds: u32 = 0,
    renew_in_seconds: u32 = 0,
    rebind_in_seconds: u32 = 0,
};

pub const DhcpRuntimeStatus = struct {
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

pub const DnsCacheStatus = struct {
    valid: bool = false,
    negative: bool = false,
    age_seconds: u32 = 0,
    ttl_seconds: u32 = 0,
    remaining_seconds: u32 = 0,
};

const DnsCacheEntry = struct {
    valid: bool = false,
    negative: bool = false,
    name: [64]u8 = .{0} ** 64,
    server: [4]u8 = .{0} ** 4,
    answer: [4]u8 = .{0} ** 4,
    result: i32 = dns.RESULT_OK,
    updated_tick: u64 = 0,
    ttl_seconds: u32 = DNS_DEFAULT_CACHE_TTL_SECONDS,
    hits: u64 = 0,
};

const ArpCacheEntry = struct {
    valid: bool = false,
    ip: [4]u8 = .{0} ** 4,
    mac: [6]u8 = .{0} ** 6,
    updated_tick: u64 = 0,
    hits: u64 = 0,
};

const Ipv4RouteDecision = struct {
    result: TxResult = .ok,
    next_hop_ip: [4]u8 = .{0} ** 4,
    dest_mac: [6]u8 = .{0} ** 6,
    needs_arp: bool = true,
    last_error: []const u8 = "none",
};

pub const UdpRecvInfo = struct {
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
const DiagnosticCleanupScope = struct {
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

const UdpSocketPacket = struct {
    source_ip: [4]u8 = .{0} ** 4,
    dest_ip: [4]u8 = .{0} ** 4,
    source_port: u16 = 0,
    dest_port: u16 = 0,
    len: u16 = 0,
    payload: [UDP_SOCKET_PAYLOAD_MAX]u8 = .{0} ** UDP_SOCKET_PAYLOAD_MAX,
};

const UdpSocket = struct {
    active: bool = false,
    handle: u32 = 0,
    port: u16 = 0,
    queue: [UDP_SOCKET_QUEUE_SIZE]UdpSocketPacket = .{UdpSocketPacket{}} ** UDP_SOCKET_QUEUE_SIZE,
    head: usize = 0,
    tail: usize = 0,
    count: usize = 0,
    rx_packets: u64 = 0,
    tx_packets: u64 = 0,
    drops: u64 = 0,
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

pub const Packet = struct {
    adapter_index: usize = 0,
    len: usize = 0,
    flags: u32 = 0,
    data: [MAX_PACKET_SIZE]u8 = .{0} ** MAX_PACKET_SIZE,
};

pub const AppIpv4Packet = struct {
    source_ip: [4]u8 = .{0} ** 4,
    dest_ip: [4]u8 = .{0} ** 4,
    protocol: u8 = 0,
    source_port: u16 = 0,
    dest_port: u16 = 0,
    truncated: bool = false,
    payload_len: u32 = 0,
};

const AppIpv4QueueEntry = struct {
    info: AppIpv4Packet = .{},
    seq: u64 = 0,
    payload: [APP_IPV4_PAYLOAD_MAX]u8 = .{0} ** APP_IPV4_PAYLOAD_MAX,
};

const AppIpv4Queue = struct {
    entries: [APP_IPV4_QUEUE_SIZE]AppIpv4QueueEntry = .{AppIpv4QueueEntry{}} ** APP_IPV4_QUEUE_SIZE,
    head: usize = 0,
    tail: usize = 0,
    count: usize = 0,
    drops: u64 = 0,
};

var adapters: [MAX_ADAPTERS]Adapter = undefined;
var adapter_count: usize = 0;
var system_transition_active: bool = false;
var backend_admission_closed: bool = false;
var backend_mutation_active: bool = false;
var backend_transition_unsafe: bool = false;
var backend_callback_count: u32 = 0;
var packet_pool: [PACKET_POOL_SIZE]Packet = .{Packet{}} ** PACKET_POOL_SIZE;
var packet_used: [PACKET_POOL_SIZE]bool = .{false} ** PACKET_POOL_SIZE;
var packet_drops: u64 = 0;
var buffer_small_events: u64 = 0;
var rx_handoff_queue: rx_handoff.Queue = .{};
var rx_work_event = sync.EventV2.initMode(false, .auto_reset);
var rx_schedule_mask: u16 = 0;
var rx_schedule_stamps: [MAX_ADAPTERS]time_core.MonotonicStamp =
    .{time_core.MonotonicStamp{}} ** MAX_ADAPTERS;
var rx_schedule_calls: u64 = 0;
var rx_schedule_from_irq: u64 = 0;
var rx_schedule_coalesced: u64 = 0;
var rx_schedule_invalid: u64 = 0;
var rx_event_wakeups: u64 = 0;
var rx_event_timeouts: u64 = 0;
var rx_protocol_success: u64 = 0;
var rx_protocol_failures: u64 = 0;
var rx_processing_depth: u8 = 0;
var rx_release_failures: u64 = 0;
var rx_batches: u64 = 0;
var rx_batch_max: u32 = 0;
var rx_budget_exhaustions: u64 = 0;
var rx_handoff_latency_samples: u64 = 0;
var rx_handoff_latency_total_ns: u64 = 0;
var rx_handoff_latency_last_ns: u64 = 0;
var rx_handoff_latency_max_ns: u64 = 0;
var rx_schedule_latency_samples: u64 = 0;
var rx_schedule_latency_total_ns: u64 = 0;
var rx_schedule_latency_last_ns: u64 = 0;
var rx_schedule_latency_max_ns: u64 = 0;
var rx_latency_unavailable: u64 = 0;
var rx_irq_frame_rejects: u64 = 0;
var rx_metadata_submissions: u64 = 0;
var rx_metadata_selected: u64 = 0;
var rx_metadata_fallbacks: u64 = 0;
var rx_l4_offload_used: u64 = 0;
var rx_l4_software_checks: u64 = 0;
var diag_lifecycle_link_up: bool = true;
var app_ipv4_queues: [APP_IPV4_BUCKET_COUNT]AppIpv4Queue = .{AppIpv4Queue{}} ** APP_IPV4_BUCKET_COUNT;
var app_ipv4_next_seq: u64 = 1;
var eth_stats: ethernet.Stats = .{};
var arp_stats: arp.Stats = .{};
var ipv4_stats: ipv4.Stats = .{};
var icmp_stats: icmp.Stats = .{};
var udp_stats: udp.Stats = .{};
var dhcp_stats: dhcp.Stats = .{};
var dhcp_coordinator: dhcp_runtime.Coordinator = .{};
var dhcp_operation_lock = sync.Mutex.initClass("dhcp-operation", sync.LockRank.local, .sleepable);
var dhcp_task_started = false;
var dhcp_task_id: u32 = 0;
var dhcp_task_generation: u64 = 0;
var dhcp_task_retry_requested = false;
var dhcp_task_next_retry_tick: u64 = 0;
var dhcp_last_logged_state: dhcp_runtime.State = .disabled;
var dhcp_last_logged_link_generation: u32 = 0;
var dhcp_test_drop_offers: u8 = 0;
var dhcp_test_drop_acks: u8 = 0;
const DhcpExpectedResponse = enum {
    none,
    offer,
    ack_or_nak,
};
var dhcp_expected_response: DhcpExpectedResponse = .none;
var dhcp_expected_xid: u32 = 0;
var dhcp_expected_mac: [6]u8 = .{0} ** 6;
var dns_stats: dns.Stats = .{};
var dns_cache_entries: [DNS_CACHE_ENTRIES]DnsCacheEntry = .{DnsCacheEntry{}} ** DNS_CACHE_ENTRIES;
var dns_cache_next_slot: usize = 0;
var arp_cache_entries: [ARP_CACHE_ENTRIES]ArpCacheEntry = .{ArpCacheEntry{}} ** ARP_CACHE_ENTRIES;
var arp_cache_next_slot: usize = 0;
var icmp_sequence: u16 = 1;
var arp_cache_updated_tick: u64 = 0;
var ethernet_r4p_rx: u64 = 0;
var ethernet_r4p_tx: u64 = 0;
var ethernet_r4p_build: u64 = 0;
var ethernet_dispatch_failures: u64 = 0;
var arp_r4p_rx: u64 = 0;
var arp_r4p_tx: u64 = 0;
var arp_r4p_build: u64 = 0;
var arp_dispatch_failures: u64 = 0;
var ipv4_r4p_rx: u64 = 0;
var ipv4_r4p_tx: u64 = 0;
var ipv4_r4p_build: u64 = 0;
var ipv4_dispatch_failures: u64 = 0;
var ipv4_payload_scratch: [MAX_PACKET_SIZE]u8 = .{0} ** MAX_PACKET_SIZE;
var icmp_r4p_rx: u64 = 0;
var icmp_r4p_tx: u64 = 0;
var icmp_r4p_build: u64 = 0;
var icmp_r4p_classify: u64 = 0;
var icmp_dispatch_failures: u64 = 0;
var udp_r4p_rx: u64 = 0;
var udp_r4p_tx: u64 = 0;
var udp_r4p_build: u64 = 0;
var udp_dispatch_failures: u64 = 0;
var udp_payload_scratch: [MAX_PACKET_SIZE]u8 = .{0} ** MAX_PACKET_SIZE;
var udp_sockets: [UDP_SOCKET_MAX]UdpSocket = .{UdpSocket{}} ** UDP_SOCKET_MAX;
var udp_next_handle: u32 = 1;
var udp_socket_delivered: u64 = 0;
var udp_socket_drops: u64 = 0;
var tx_backpressure: TxBackpressureCounters = .{};
var cleanup_status: CleanupStatus = .{};
var dhcp_r4p_rx: u64 = 0;
var dhcp_r4p_build: u64 = 0;
var dhcp_dispatch_failures: u64 = 0;
var dns_r4p_rx: u64 = 0;
var dns_r4p_build: u64 = 0;
var dns_dispatch_failures: u64 = 0;
var tcp_r4p_rx: u64 = 0;
var tcp_r4p_tx: u64 = 0;
var tcp_r4p_control: u64 = 0;
var tcp_r4p_build: u64 = 0;
var tcp_dispatch_failures: u64 = 0;
var tcp_last_flags: u16 = 0;
var tcp_last_error: []const u8 = "none";
var packet_corpus_tests: u64 = 0;
var packet_corpus_cases: u64 = 0;
var negative_path_tests: u64 = 0;
var negative_path_cases: u64 = 0;
var driver_lifecycle_tests: u64 = 0;
var driver_lifecycle_cases: u64 = 0;
var environment_contract_tests: u64 = 0;
var environment_contract_cases: u64 = 0;
var limit_contract_tests: u64 = 0;
var limit_contract_cases: u64 = 0;
var power_lifecycle_tests: u64 = 0;
var power_lifecycle_cases: u64 = 0;

pub fn init() void {
    adapter_count = 0;
    @atomicStore(bool, &system_transition_active, false, .release);
    @atomicStore(bool, &backend_admission_closed, false, .release);
    @atomicStore(bool, &backend_mutation_active, false, .release);
    @atomicStore(bool, &backend_transition_unsafe, false, .release);
    @atomicStore(u32, &backend_callback_count, 0, .release);
    packet_pool = .{Packet{}} ** PACKET_POOL_SIZE;
    packet_used = .{false} ** PACKET_POOL_SIZE;
    packet_drops = 0;
    buffer_small_events = 0;
    rx_handoff_queue.reset();
    rx_work_event = sync.EventV2.initMode(false, .auto_reset);
    rx_schedule_mask = 0;
    rx_schedule_stamps = .{time_core.MonotonicStamp{}} ** MAX_ADAPTERS;
    rx_schedule_calls = 0;
    rx_schedule_from_irq = 0;
    rx_schedule_coalesced = 0;
    rx_schedule_invalid = 0;
    rx_event_wakeups = 0;
    rx_event_timeouts = 0;
    rx_protocol_success = 0;
    rx_protocol_failures = 0;
    rx_processing_depth = 0;
    rx_release_failures = 0;
    rx_batches = 0;
    rx_batch_max = 0;
    rx_budget_exhaustions = 0;
    rx_handoff_latency_samples = 0;
    rx_handoff_latency_total_ns = 0;
    rx_handoff_latency_last_ns = 0;
    rx_handoff_latency_max_ns = 0;
    rx_schedule_latency_samples = 0;
    rx_schedule_latency_total_ns = 0;
    rx_schedule_latency_last_ns = 0;
    rx_schedule_latency_max_ns = 0;
    rx_latency_unavailable = 0;
    rx_irq_frame_rejects = 0;
    rx_metadata_submissions = 0;
    rx_metadata_selected = 0;
    rx_metadata_fallbacks = 0;
    rx_l4_offload_used = 0;
    rx_l4_software_checks = 0;
    app_ipv4_queues = .{AppIpv4Queue{}} ** APP_IPV4_BUCKET_COUNT;
    app_ipv4_next_seq = 1;
    ethernet.reset(&eth_stats);
    net_config.reset();
    arp.reset(&arp_stats);
    ipv4.reset(&ipv4_stats);
    icmp.reset(&icmp_stats);
    udp.reset(&udp_stats);
    dhcp.reset(&dhcp_stats);
    dhcp_coordinator.reset();
    dhcp_operation_lock = sync.Mutex.initClass("dhcp-operation", sync.LockRank.local, .sleepable);
    dhcp_task_started = false;
    dhcp_task_id = 0;
    dhcp_task_generation = 0;
    dhcp_task_retry_requested = false;
    dhcp_task_next_retry_tick = 0;
    dhcp_last_logged_state = .disabled;
    dhcp_last_logged_link_generation = 0;
    dhcp_test_drop_offers = 0;
    dhcp_test_drop_acks = 0;
    clearDhcpResponseExpectation();
    dns.reset(&dns_stats);
    dns_cache_entries = .{DnsCacheEntry{}} ** DNS_CACHE_ENTRIES;
    dns_cache_next_slot = 0;
    arp_cache_entries = .{ArpCacheEntry{}} ** ARP_CACHE_ENTRIES;
    arp_cache_next_slot = 0;
    tcp.reset();
    tcp_rto.reset();
    tcp_proactive_retransmits = 0;
    tcp_rto_samples = 0;
    tcp_rto_last_ticks = 0;
    icmp_sequence = 1;
    arp_cache_updated_tick = 0;
    ethernet_r4p_rx = 0;
    ethernet_r4p_tx = 0;
    ethernet_r4p_build = 0;
    ethernet_dispatch_failures = 0;
    arp_r4p_rx = 0;
    arp_r4p_tx = 0;
    arp_r4p_build = 0;
    arp_dispatch_failures = 0;
    ipv4_r4p_rx = 0;
    ipv4_r4p_tx = 0;
    ipv4_r4p_build = 0;
    ipv4_dispatch_failures = 0;
    ipv4_payload_scratch = .{0} ** MAX_PACKET_SIZE;
    icmp_r4p_rx = 0;
    icmp_r4p_tx = 0;
    icmp_r4p_build = 0;
    icmp_r4p_classify = 0;
    icmp_dispatch_failures = 0;
    udp_r4p_rx = 0;
    udp_r4p_tx = 0;
    udp_r4p_build = 0;
    udp_dispatch_failures = 0;
    udp_payload_scratch = .{0} ** MAX_PACKET_SIZE;
    udp_sockets = .{UdpSocket{}} ** UDP_SOCKET_MAX;
    udp_next_handle = 1;
    udp_socket_delivered = 0;
    udp_socket_drops = 0;
    tx_backpressure = .{};
    cleanup_status = .{};
    net_poll_gate = sync.UnwindGuard.init("net-poll");
    net_poll_skips = 0;
    net_poll_nested = 0;
    dhcp_r4p_rx = 0;
    dhcp_r4p_build = 0;
    dhcp_dispatch_failures = 0;
    dns_r4p_rx = 0;
    dns_r4p_build = 0;
    dns_dispatch_failures = 0;
    tcp_r4p_rx = 0;
    tcp_r4p_tx = 0;
    tcp_r4p_control = 0;
    tcp_r4p_build = 0;
    tcp_dispatch_failures = 0;
    tcp_last_flags = 0;
    tcp_last_error = "none";
    packet_corpus_tests = 0;
    packet_corpus_cases = 0;
    negative_path_tests = 0;
    negative_path_cases = 0;
    driver_lifecycle_tests = 0;
    driver_lifecycle_cases = 0;
    environment_contract_tests = 0;
    environment_contract_cases = 0;
    limit_contract_tests = 0;
    limit_contract_cases = 0;
    power_lifecycle_tests = 0;
    power_lifecycle_cases = 0;
}

pub fn configureDhcpTestInjection(config: *const boot_config.Config) void {
    dhcp_test_drop_offers = parseBoundedTestCount(boot_config.optionValue(config, "NET", "test_drop_offer"));
    dhcp_test_drop_acks = parseBoundedTestCount(boot_config.optionValue(config, "NET", "test_drop_ack"));
    if (dhcp_test_drop_offers != 0 or dhcp_test_drop_acks != 0) {
        bootlog.puts("[DHCP05913] test injection offer=");
        bootlog.putDec(dhcp_test_drop_offers);
        bootlog.puts(" ack=");
        bootlog.putDec(dhcp_test_drop_acks);
        bootlog.puts("\r\n");
    }
}

fn parseBoundedTestCount(value: ?[]const u8) u8 {
    const text = value orelse return 0;
    var parsed: u16 = 0;
    if (text.len == 0) return 0;
    for (text) |ch| {
        if (ch < '0' or ch > '9') return 0;
        parsed = parsed * 10 + @as(u16, ch - '0');
        if (parsed > 9) return 9;
    }
    return @intCast(parsed);
}

pub fn register(adapter: Adapter) ?usize {
    if (adapter_count >= MAX_ADAPTERS) return null;
    var normalized = adapter;
    const now = time_core.monotonicTicks();
    normalized.registered_tick = now;
    normalized.state_changed_tick = now;
    normalized.lifecycle = lifecycleFromLink(normalized.link);
    adapters[adapter_count] = normalized;
    adapter_count += 1;
    return adapter_count - 1;
}

pub fn unregister(index: usize) bool {
    return unregisterInternal(index, null);
}

fn unregisterInternal(index: usize, diagnostic_scope: ?DiagnosticCleanupScope) bool {
    if (index >= adapter_count) return false;
    setAdapterLifecycle(index, .shutdown);
    if (diagnostic_scope) |scope| {
        _ = cleanupDiagnosticNetworkOperations("adapter-unregister", scope);
    } else {
        _ = cleanupNetworkOperations("adapter-unregister");
    }

    return removeAdapterEntry(index);
}

fn discardDiagnosticAdapter(index: usize) bool {
    if (index >= adapter_count) return false;
    setAdapterLifecycle(index, .shutdown);
    return removeAdapterEntry(index);
}

fn removeAdapterEntry(index: usize) bool {
    if (index >= adapter_count) return false;

    var packet_index: usize = 0;
    while (packet_index < PACKET_POOL_SIZE) : (packet_index += 1) {
        if (!packet_used[packet_index]) continue;
        if (packet_pool[packet_index].adapter_index == index) {
            packet_used[packet_index] = false;
            packet_pool[packet_index] = .{};
        } else if (packet_pool[packet_index].adapter_index > index) {
            packet_pool[packet_index].adapter_index -= 1;
        }
    }

    var i = index;
    while (i + 1 < adapter_count) : (i += 1) {
        adapters[i] = adapters[i + 1];
    }
    adapter_count -= 1;
    adapters[adapter_count] = .{};
    return true;
}

pub fn resetAdapterLifecycle(index: usize, reason: []const u8) bool {
    return resetAdapterLifecycleInternal(index, reason, null);
}

fn resetAdapterLifecycleInternal(index: usize, reason: []const u8, diagnostic_scope: ?DiagnosticCleanupScope) bool {
    if (index >= adapter_count) return false;
    setAdapterLifecycle(index, .resetting);
    adapters[index].stats.resets += 1;
    adapters[index].stats.last_error = reason;
    if (diagnostic_scope) |scope| {
        _ = cleanupDiagnosticNetworkOperations("adapter-reset", scope);
    } else {
        _ = cleanupNetworkOperations("adapter-reset");
    }
    if (index >= adapter_count) return false;
    if (refreshAdapterRuntime(index) == null) {
        setAdapterLifecycle(index, lifecycleFromLink(adapters[index].link));
    }
    return true;
}

pub fn cleanupNetworkOperations(reason: []const u8) CleanupStatus {
    const closed_udp = closeAllUdpSockets(reason);
    const tcp_cleanup = tcp.abortAll(reason);
    tcp_rto.reset();
    const dhcp_cancelled = cancelDhcpOperation(reason);
    const dns_cancelled = cancelDnsOperation(reason);

    return recordCleanupResult(reason, closed_udp, tcp_cleanup, dhcp_cancelled, dns_cancelled);
}

pub fn beginSystemTransition(reason: []const u8) bool {
    // Close callback admission before a runtime R4D starts dismantling IRQs
    // and DMA. Every path that can be inside a backend or accept a frame owns
    // one reference, including nested poll -> RX -> TX paths. The second
    // transition check in enterBackendCallback closes the increment race.
    if (@atomicRmw(bool, &system_transition_active, .Xchg, true, .acq_rel)) return false;
    _ = @atomicRmw(bool, &backend_admission_closed, .Xchg, true, .acq_rel);
    if (!drainBackendMutation()) return false;
    if (@atomicLoad(bool, &backend_transition_unsafe, .acquire)) {
        k.puts("[NET] quarantined backend blocks warm reset\r\n");
        return false;
    }
    // A temporary mutation that observed the transition may have opened its
    // gate immediately before publishing mutation_active=false. Reclose after
    // that publication, then drain every callback admitted in the tiny gap.
    _ = @atomicRmw(bool, &backend_admission_closed, .Xchg, true, .acq_rel);
    if (!drainBackendCallbacks("system-transition")) return false;
    _ = cancelRxHandoffQueue();
    _ = cleanupNetworkOperations(reason);
    var index: usize = 0;
    while (index < adapter_count) : (index += 1) setAdapterLifecycle(index, .shutdown);
    return true;
}

pub fn beginBackendMutation() bool {
    if (@atomicLoad(bool, &system_transition_active, .acquire)) return false;
    if (@atomicRmw(bool, &backend_mutation_active, .Xchg, true, .acq_rel)) return false;
    if (@atomicLoad(bool, &system_transition_active, .acquire)) {
        @atomicStore(bool, &backend_mutation_active, false, .release);
        return false;
    }
    if (@atomicRmw(bool, &backend_admission_closed, .Xchg, true, .acq_rel)) {
        @atomicStore(bool, &backend_mutation_active, false, .release);
        return false;
    }
    // A concurrent permanent transition owns the closed admission gate. Do
    // not reopen it from the temporary-mutation path.
    if (@atomicLoad(bool, &system_transition_active, .acquire)) {
        endBackendMutation();
        return false;
    }
    if (drainBackendCallbacks("backend-mutation")) {
        _ = cancelRxHandoffQueue();
        return true;
    }
    endBackendMutation();
    return false;
}

pub fn endBackendMutation() void {
    if (!@atomicLoad(bool, &system_transition_active, .acquire) and
        !@atomicLoad(bool, &backend_transition_unsafe, .acquire))
    {
        @atomicStore(bool, &backend_admission_closed, false, .release);
    }
    @atomicStore(bool, &backend_mutation_active, false, .release);
}

pub fn quarantineBackendMutation() void {
    // A driver shutdown/finalizer has already touched hardware but could not
    // prove a safe teardown. Keep every backend callback sealed for the rest
    // of this boot while releasing the temporary-mutation ownership so a
    // later system transition can observe the quarantine and power off.
    _ = @atomicRmw(bool, &backend_transition_unsafe, .Xchg, true, .acq_rel);
    _ = @atomicRmw(bool, &backend_admission_closed, .Xchg, true, .acq_rel);
    @atomicStore(bool, &backend_mutation_active, false, .release);
}

fn drainBackendMutation() bool {
    const start = time_core.monotonicTicks();
    while (@atomicLoad(bool, &backend_mutation_active, .acquire)) {
        if (time_core.monotonicTicks() -% start >= SYSTEM_TRANSITION_CALLBACK_DRAIN_TICKS) {
            k.puts("[NET] system-transition backend mutation timeout\r\n");
            return false;
        }
        if (scheduler.current() != null) {
            scheduler.sleepTicksWithReason(1, "net-transition-mutation");
        } else {
            asm volatile ("pause");
        }
    }
    return true;
}

fn drainBackendCallbacks(reason: []const u8) bool {
    const start = time_core.monotonicTicks();
    while (@atomicLoad(u32, &backend_callback_count, .acquire) != 0) {
        if (time_core.monotonicTicks() -% start >= SYSTEM_TRANSITION_CALLBACK_DRAIN_TICKS) {
            k.puts("[NET] ");
            k.puts(reason);
            k.puts(" callback drain timeout active=");
            k.putDec(@atomicLoad(u32, &backend_callback_count, .acquire));
            k.puts("\r\n");
            return false;
        }
        if (scheduler.current() != null) {
            scheduler.sleepTicksWithReason(1, "net-transition-drain");
        } else {
            asm volatile ("pause");
        }
    }
    return true;
}

fn enterBackendCallback() bool {
    if (@atomicLoad(bool, &backend_admission_closed, .acquire)) return false;
    _ = @atomicRmw(u32, &backend_callback_count, .Add, 1, .acq_rel);
    if (!@atomicLoad(bool, &backend_admission_closed, .acquire)) return true;
    _ = @atomicRmw(u32, &backend_callback_count, .Sub, 1, .release);
    return false;
}

fn leaveBackendCallback() void {
    _ = @atomicRmw(u32, &backend_callback_count, .Sub, 1, .release);
}

pub fn cancelDhcpForStaticConfig() void {
    _ = cancelDhcpOperation("static-config");
}

fn cleanupDiagnosticNetworkOperations(reason: []const u8, scope: DiagnosticCleanupScope) CleanupStatus {
    var closed_udp: u32 = 0;
    if (scope.udp_handle != 0 and udpClose(scope.udp_handle) == 0) {
        closed_udp = 1;
        udp_stats.last_error = reason;
    }

    var tcp_cleanup: tcp.CleanupResult = .{};
    if (scope.tcp_connection_id != 0 and !tcp.closed(scope.tcp_connection_id)) {
        tcp.abort(scope.tcp_connection_id, reason);
        tcp_cleanup.connections = 1;
    }
    if (scope.tcp_listener_port != 0 and tcp.hasListener(scope.tcp_listener_port)) {
        tcp.closeListener(scope.tcp_listener_port);
        tcp_cleanup.listeners = 1;
    }
    if (tcp_cleanup.connections != 0 or tcp_cleanup.listeners != 0) tcp.setError(reason);

    const dhcp_cancelled = cancelDhcpOperation(reason);
    const dns_cancelled = cancelDnsOperation(reason);
    return recordCleanupResult(reason, closed_udp, tcp_cleanup, dhcp_cancelled, dns_cancelled);
}

fn recordCleanupResult(reason: []const u8, closed_udp: u32, tcp_cleanup: tcp.CleanupResult, dhcp_cancelled: bool, dns_cancelled: bool) CleanupStatus {
    cleanup_status.runs += 1;
    cleanup_status.last_reason = reason;
    recordCleanupReason(reason);

    cleanup_status.udp_sockets_closed += closed_udp;
    cleanup_status.tcp_connections_aborted += tcp_cleanup.connections;
    cleanup_status.tcp_listeners_closed += tcp_cleanup.listeners;
    if (dhcp_cancelled) cleanup_status.dhcp_operations_cancelled += 1;
    if (dns_cancelled) cleanup_status.dns_operations_cancelled += 1;
    cleanup_status.last_udp_closed = closed_udp;
    cleanup_status.last_tcp_connections = tcp_cleanup.connections;
    cleanup_status.last_tcp_listeners = tcp_cleanup.listeners;
    return cleanup_status;
}

fn recordCleanupReason(reason: []const u8) void {
    if (memEql(reason, "link-down")) {
        cleanup_status.link_down_cleanups += 1;
    } else if (memEql(reason, "adapter-reset")) {
        cleanup_status.adapter_reset_cleanups += 1;
    } else if (memEql(reason, "adapter-unregister")) {
        cleanup_status.adapter_unregister_cleanups += 1;
    } else if (memEql(reason, "service-restart")) {
        cleanup_status.service_restart_cleanups += 1;
    } else if (memEql(reason, "poweroff")) {
        cleanup_status.poweroff_cleanups += 1;
    } else if (memEql(reason, "reboot")) {
        cleanup_status.reboot_cleanups += 1;
    }
}

pub fn cleanupStatus() CleanupStatus {
    return cleanup_status;
}

pub fn runCleanupProbe() bool {
    const before = cleanupStatus();
    var before_udp: UdpStatus = .{};
    udpStatus(&before_udp);
    var before_tcp: tcp.Summary = .{};
    tcp.summary(&before_tcp);
    if (tcp.hasListener(65012)) return false;

    const udp_handle = udpBind(65011);
    if (udp_handle <= 0) return false;
    if (!tcpListen(65012)) {
        _ = udpClose(@intCast(udp_handle));
        return false;
    }
    dhcp_stats.operation_pending = true;
    dhcp_stats.pending_label = "diag";
    dns_stats.operation_pending = true;
    copyFixedBytes(dns_stats.pending_name[0..], "cleanup.test");

    const after_cleanup = cleanupDiagnosticNetworkOperations("diag-cleanup", .{
        .udp_handle = @intCast(udp_handle),
        .tcp_listener_port = 65012,
    });
    var after_udp: UdpStatus = .{};
    udpStatus(&after_udp);
    var after_tcp: tcp.Summary = .{};
    tcp.summary(&after_tcp);

    return after_cleanup.runs == before.runs + 1 and
        after_cleanup.last_udp_closed == 1 and
        after_cleanup.last_tcp_listeners == 1 and
        after_cleanup.udp_sockets_closed == before.udp_sockets_closed + 1 and
        after_cleanup.tcp_listeners_closed == before.tcp_listeners_closed + 1 and
        after_cleanup.dhcp_operations_cancelled == before.dhcp_operations_cancelled + 1 and
        after_cleanup.dns_operations_cancelled == before.dns_operations_cancelled + 1 and
        after_udp.active_sockets == before_udp.active_sockets and
        after_tcp.active_connections == before_tcp.active_connections and
        after_tcp.active_listeners == before_tcp.active_listeners and
        !tcp.hasListener(65012) and
        !dhcp_stats.operation_pending and
        !dns_stats.operation_pending and
        memEql(dhcp_stats.pending_label, "idle");
}

pub fn runPowerLifecycleProbe() bool {
    var cases: u64 = 0;
    if (!powerLifecycleCase("poweroff", 65031, 65032, .{ 198, 51, 100, 41 }, 80)) return false;
    cases += 1;
    if (!powerLifecycleCase("reboot", 65033, 65034, .{ 198, 51, 100, 42 }, 443)) return false;
    cases += 1;
    power_lifecycle_tests += 1;
    power_lifecycle_cases += cases;
    return true;
}

fn powerLifecycleCase(reason: []const u8, udp_port: u16, listen_port: u16, remote_ip: [4]u8, remote_port: u16) bool {
    const before = cleanupStatus();
    var before_udp: UdpStatus = .{};
    udpStatus(&before_udp);
    var before_tcp: tcp.Summary = .{};
    tcp.summary(&before_tcp);
    if (tcp.hasListener(listen_port)) return false;

    const udp_handle = udpBind(udp_port);
    if (udp_handle <= 0) return false;
    const conn_id = tcp.selfTestConnect(remote_ip, remote_port);
    if (conn_id <= 0) {
        _ = udpClose(@intCast(udp_handle));
        return false;
    }
    if (!tcpListen(listen_port)) {
        _ = udpClose(@intCast(udp_handle));
        tcp.abort(@intCast(conn_id), "diag-rollback");
        return false;
    }

    dhcp_stats.operation_pending = true;
    dhcp_stats.pending_label = reason;
    dns_stats.operation_pending = true;
    copyFixedBytes(dns_stats.pending_name[0..], "power.cleanup");

    const after_cleanup = cleanupDiagnosticNetworkOperations(reason, .{
        .udp_handle = @intCast(udp_handle),
        .tcp_connection_id = @intCast(conn_id),
        .tcp_listener_port = listen_port,
    });
    var after_udp: UdpStatus = .{};
    udpStatus(&after_udp);
    var after_tcp: tcp.Summary = .{};
    tcp.summary(&after_tcp);

    const reason_ok = if (memEql(reason, "poweroff"))
        after_cleanup.poweroff_cleanups == before.poweroff_cleanups + 1
    else if (memEql(reason, "reboot"))
        after_cleanup.reboot_cleanups == before.reboot_cleanups + 1
    else
        false;

    return reason_ok and
        after_cleanup.runs == before.runs + 1 and
        after_cleanup.last_udp_closed == 1 and
        after_cleanup.last_tcp_connections == 1 and
        after_cleanup.last_tcp_listeners == 1 and
        after_cleanup.udp_sockets_closed == before.udp_sockets_closed + 1 and
        after_cleanup.tcp_connections_aborted == before.tcp_connections_aborted + 1 and
        after_cleanup.tcp_listeners_closed == before.tcp_listeners_closed + 1 and
        after_cleanup.dhcp_operations_cancelled == before.dhcp_operations_cancelled + 1 and
        after_cleanup.dns_operations_cancelled == before.dns_operations_cancelled + 1 and
        after_udp.active_sockets == before_udp.active_sockets and
        after_tcp.active_connections == before_tcp.active_connections and
        after_tcp.active_listeners == before_tcp.active_listeners and
        !tcp.hasListener(listen_port) and
        !dhcp_stats.operation_pending and
        !dns_stats.operation_pending and
        memEql(dhcp_stats.pending_label, "idle");
}

pub fn runLinkLifecycleProbe() bool {
    const before = cleanupStatus();
    var before_udp: UdpStatus = .{};
    udpStatus(&before_udp);
    var before_tcp: tcp.Summary = .{};
    tcp.summary(&before_tcp);
    if (tcp.hasListener(65016)) return false;

    diag_lifecycle_link_up = true;
    const adapter_index = register(.{
        .name = "diag-net",
        .driver = "diag",
        .link = .up,
        .ops = .{
            .status = diagLifecycleStatus,
        },
    }) orelse return false;
    defer _ = discardDiagnosticAdapter(adapter_index);

    const udp_handle = udpBind(65015);
    if (udp_handle <= 0) return false;
    if (!tcpListen(65016)) {
        _ = udpClose(@intCast(udp_handle));
        return false;
    }
    dhcp_stats.operation_pending = true;
    dhcp_stats.pending_label = "lifecycle";
    dns_stats.operation_pending = true;
    copyFixedBytes(dns_stats.pending_name[0..], "lifecycle.test");

    diag_lifecycle_link_up = false;
    _ = refreshAdapterRuntimeInternal(adapter_index, .{ .udp_handle = @intCast(udp_handle), .tcp_listener_port = 65016 });
    const after_down = cleanupStatus();
    var after_udp: UdpStatus = .{};
    udpStatus(&after_udp);
    var after_tcp: tcp.Summary = .{};
    tcp.summary(&after_tcp);
    const down_ok = adapters[adapter_index].link == .down and
        adapters[adapter_index].lifecycle == .link_down and
        after_down.runs == before.runs + 1 and
        after_down.link_down_cleanups == before.link_down_cleanups + 1 and
        after_down.last_udp_closed == 1 and
        after_down.last_tcp_listeners == 1 and
        after_down.udp_sockets_closed == before.udp_sockets_closed + 1 and
        after_down.tcp_listeners_closed == before.tcp_listeners_closed + 1 and
        after_udp.active_sockets == before_udp.active_sockets and
        after_tcp.active_connections == before_tcp.active_connections and
        after_tcp.active_listeners == before_tcp.active_listeners and
        !tcp.hasListener(65016) and
        !dhcp_stats.operation_pending and
        !dns_stats.operation_pending;

    diag_lifecycle_link_up = true;
    _ = refreshAdapterRuntimeInternal(adapter_index, .{});
    const up_ok = adapters[adapter_index].link == .up and adapters[adapter_index].lifecycle == .active;
    return down_ok and up_ok;
}

pub fn runAdapterResetProbe() bool {
    const before = cleanupStatus();
    var before_udp: UdpStatus = .{};
    udpStatus(&before_udp);
    var before_tcp: tcp.Summary = .{};
    tcp.summary(&before_tcp);
    if (tcp.hasListener(65018)) return false;

    diag_lifecycle_link_up = true;
    const adapter_index = register(.{
        .name = "diag-reset",
        .driver = "diag",
        .link = .up,
        .ops = .{
            .status = diagLifecycleStatus,
        },
    }) orelse return false;
    defer _ = discardDiagnosticAdapter(adapter_index);

    const udp_handle = udpBind(65017);
    if (udp_handle <= 0) return false;
    if (!tcpListen(65018)) {
        _ = udpClose(@intCast(udp_handle));
        return false;
    }
    dhcp_stats.operation_pending = true;
    dhcp_stats.pending_label = "adapter-reset";
    dns_stats.operation_pending = true;
    copyFixedBytes(dns_stats.pending_name[0..], "reset.test");

    if (!resetAdapterLifecycleInternal(adapter_index, "diag-reset", .{
        .udp_handle = @intCast(udp_handle),
        .tcp_listener_port = 65018,
    })) return false;
    const after_reset = cleanupStatus();
    var after_udp: UdpStatus = .{};
    udpStatus(&after_udp);
    var after_tcp: tcp.Summary = .{};
    tcp.summary(&after_tcp);

    return adapters[adapter_index].stats.resets == 1 and
        adapters[adapter_index].lifecycle == .active and
        after_reset.runs == before.runs + 1 and
        after_reset.adapter_reset_cleanups == before.adapter_reset_cleanups + 1 and
        after_reset.last_udp_closed == 1 and
        after_reset.last_tcp_listeners == 1 and
        after_reset.udp_sockets_closed == before.udp_sockets_closed + 1 and
        after_reset.tcp_listeners_closed == before.tcp_listeners_closed + 1 and
        after_udp.active_sockets == before_udp.active_sockets and
        after_tcp.active_connections == before_tcp.active_connections and
        after_tcp.active_listeners == before_tcp.active_listeners and
        !tcp.hasListener(65018) and
        !dhcp_stats.operation_pending and
        !dns_stats.operation_pending;
}

pub fn runDriverLifecycleProbe() bool {
    const before_count = adapter_count;
    if (before_count + 1 >= MAX_ADAPTERS) return false;
    const before_cleanup = cleanupStatus();
    var before_udp: UdpStatus = .{};
    udpStatus(&before_udp);
    var before_tcp: tcp.Summary = .{};
    tcp.summary(&before_tcp);
    if (tcp.hasListener(65022)) return false;

    diag_lifecycle_link_up = true;

    const adapter_index = register(.{
        .name = "diag-r4d-unload",
        .driver = "diag-r4d",
        .link = .up,
        .ops = .{
            .status = diagLifecycleStatus,
        },
    }) orelse return false;
    if (adapter_count != before_count + 1) {
        _ = discardDiagnosticAdapter(adapter_index);
        return false;
    }

    const packet_index = allocPacket() orelse {
        _ = discardDiagnosticAdapter(adapter_index);
        return false;
    };
    packet_pool[packet_index].adapter_index = adapter_index;
    packet_pool[packet_index].len = 8;

    const udp_handle = udpBind(65021);
    if (udp_handle <= 0) {
        freePacket(packet_index);
        _ = discardDiagnosticAdapter(adapter_index);
        return false;
    }
    if (!tcpListen(65022)) {
        freePacket(packet_index);
        _ = udpClose(@intCast(udp_handle));
        _ = discardDiagnosticAdapter(adapter_index);
        return false;
    }
    dhcp_stats.operation_pending = true;
    dhcp_stats.pending_label = "driver-unload";
    dns_stats.operation_pending = true;
    copyFixedBytes(dns_stats.pending_name[0..], "driver-unload.test");

    if (!unregisterInternal(adapter_index, .{
        .udp_handle = @intCast(udp_handle),
        .tcp_listener_port = 65022,
    })) return false;
    const after_unload = cleanupStatus();
    var after_udp: UdpStatus = .{};
    udpStatus(&after_udp);
    var after_tcp: tcp.Summary = .{};
    tcp.summary(&after_tcp);
    const unload_ok = adapter_count == before_count and
        !packet_used[packet_index] and
        after_unload.runs == before_cleanup.runs + 1 and
        after_unload.adapter_unregister_cleanups == before_cleanup.adapter_unregister_cleanups + 1 and
        after_unload.last_udp_closed == 1 and
        after_unload.last_tcp_listeners == 1 and
        after_unload.udp_sockets_closed == before_cleanup.udp_sockets_closed + 1 and
        after_unload.tcp_listeners_closed == before_cleanup.tcp_listeners_closed + 1 and
        after_unload.dhcp_operations_cancelled == before_cleanup.dhcp_operations_cancelled + 1 and
        after_unload.dns_operations_cancelled == before_cleanup.dns_operations_cancelled + 1 and
        after_udp.active_sockets == before_udp.active_sockets and
        after_tcp.active_connections == before_tcp.active_connections and
        after_tcp.active_listeners == before_tcp.active_listeners and
        !tcp.hasListener(65022) and
        !dhcp_stats.operation_pending and
        !dns_stats.operation_pending;
    if (!unload_ok) return false;

    const reload_index = register(.{
        .name = "diag-r4d-reload",
        .driver = "diag-r4d",
        .link = .up,
        .ops = .{
            .status = diagLifecycleStatus,
        },
    }) orelse return false;
    const reload_ok = adapter_count == before_count + 1 and
        adapters[reload_index].lifecycle == .active and
        adapters[reload_index].link == .up;
    _ = discardDiagnosticAdapter(reload_index);
    if (!reload_ok or adapter_count != before_count) return false;

    driver_lifecycle_tests += 1;
    driver_lifecycle_cases += 4;
    return true;
}

pub fn driverLifecycleStatus() DriverLifecycleStatus {
    return .{
        .tests = driver_lifecycle_tests,
        .cases = driver_lifecycle_cases,
    };
}

pub fn runRxHandoffProbe() bool {
    const status = rxTaskSummary();
    const ok = status.started and
        status.queue_capacity == RX_HANDOFF_QUEUE_SIZE and
        status.queue_high_water <= status.queue_capacity and
        status.batch_max <= RX_HANDOFF_BATCH_BUDGET and
        status.processed == status.protocol_success + status.protocol_failures and
        status.schedules_from_irq <= status.schedules and
        status.release_failures == 0 and
        status.irq_frame_rejects == 0 and
        status.metadata_selected <= status.metadata_submissions and
        status.metadata_fallbacks <= status.metadata_submissions and
        status.l4_offload_used <= status.metadata_selected and
        status.ownership_balanced;

    k.puts("NETRX handoff-probe result=");
    k.puts(if (ok) "OK" else "FAILED");
    k.puts(" accepted=");
    k.putDec(status.accepted);
    k.puts(" processed=");
    k.putDec(status.processed);
    k.puts(" cancelled=");
    k.putDec(status.cancelled);
    k.puts(" queue=");
    k.putDec(status.queue_ready);
    k.puts("/");
    k.putDec(status.queue_occupied);
    k.puts("/");
    k.putDec(status.queue_high_water);
    k.puts(" busy=");
    k.putDec(status.queue_busy);
    k.puts(" irq-wake=");
    k.putDec(status.schedules_from_irq);
    k.puts(" event=");
    k.putDec(status.event_wakeups);
    k.puts(" fallback=");
    k.putDec(status.fallback_timeouts);
    k.puts(" batch-max=");
    k.putDec(status.batch_max);
    k.puts(" budget-end=");
    k.putDec(status.budget_exhaustions);
    k.puts(" schedule-tail-ns=");
    k.putDec(status.schedule_latency_max_ns);
    k.puts(" handoff-tail-ns=");
    k.putDec(status.handoff_latency_max_ns);
    k.puts(" irq-inline=");
    k.putDec(status.irq_frame_rejects);
    k.puts(" offload=");
    k.putDec(status.l4_offload_used);
    k.puts("/");
    k.putDec(status.metadata_selected);
    k.puts(" fallback=");
    k.putDec(status.metadata_fallbacks);
    k.puts(" software=");
    k.putDec(status.l4_software_checks);
    k.puts("\r\n");

    if (ok) {
        driver_lifecycle_tests +%= 1;
        driver_lifecycle_cases +%= 8;
    }
    return ok;
}

pub fn runBackendCapabilityProbe() bool {
    const capable = backend_contract.negotiate(.{
        .offered = backend_contract.capability_rx_l4_checksum_valid |
            backend_contract.capability_multiqueue,
        .rx_queue_count = 4,
        .tx_queue_count = 4,
    }) catch return false;
    const flat = backend_contract.negotiate(.{}) catch return false;
    if (backend_contract.negotiate(.{
        .offered = backend_contract.capability_tx_segmentation,
        .required = backend_contract.capability_tx_segmentation,
    })) |_| {
        return false;
    } else |err| {
        if (err != error.RequiredUnsupported) return false;
    }

    var frame: [42]u8 = .{0} ** 42;
    frame[12] = 0x08;
    frame[13] = 0x00;
    frame[14] = 0x45;
    frame[16] = 0;
    frame[17] = 28;
    frame[23] = udp.IPV4_PROTOCOL;
    frame[38] = 0;
    frame[39] = 8;
    const original = frame;

    const capable_metadata = backend_contract.selectRxMetadata(
        capable.accepted,
        backend_contract.packet_flag_rx_l4_checksum_valid,
        &frame,
    );
    const flat_metadata = backend_contract.selectRxMetadata(
        flat.accepted,
        backend_contract.packet_flag_rx_l4_checksum_valid,
        &frame,
    );
    const unknown_metadata = backend_contract.selectRxMetadata(
        capable.accepted,
        backend_contract.packet_flag_rx_l4_checksum_valid | (@as(u64, 1) << 63),
        &frame,
    );
    const packet_descriptor = backend_contract.Packet{
        .fallback_addr = 1,
        .fallback_bytes = frame.len,
    };
    var wrong_queue = packet_descriptor;
    wrong_queue.queue_index = 1;
    const ok = capable.accepted == backend_contract.capability_rx_l4_checksum_valid and
        capable.rejected == backend_contract.capability_multiqueue and
        capable.rx_queue_count == 1 and capable.tx_queue_count == 1 and
        flat.accepted == 0 and flat.rejected == 0 and
        capable_metadata == .l4_checksum_valid and
        flat_metadata == .software_fallback and
        unknown_metadata == .software_fallback and
        backend_contract.validRxPacket(&packet_descriptor, capable, MAX_PACKET_SIZE) and
        !backend_contract.validRxPacket(&wrong_queue, capable, MAX_PACKET_SIZE) and
        memEql(&frame, &original);

    k.puts("NETCAP negotiation-probe result=");
    k.puts(if (ok) "OK" else "FAILED");
    k.puts(" accepted=");
    k.putHex(capable.accepted, 16);
    k.puts(" rejected=");
    k.putHex(capable.rejected, 16);
    k.puts(" queues=");
    k.putDec(capable.rx_queue_count);
    k.puts("/");
    k.putDec(capable.tx_queue_count);
    k.puts(" fallback=byte-identical\r\n");

    if (ok) {
        driver_lifecycle_tests +%= 1;
        driver_lifecycle_cases +%= 7;
    }
    return ok;
}

pub fn diagnosticCountersStatus() DiagnosticCountersStatus {
    return .{
        .packet_corpus_tests = packet_corpus_tests,
        .packet_corpus_cases = packet_corpus_cases,
        .negative_path_tests = negative_path_tests,
        .negative_path_cases = negative_path_cases,
        .environment_contract_tests = environment_contract_tests,
        .environment_contract_cases = environment_contract_cases,
        .limit_contract_tests = limit_contract_tests,
        .limit_contract_cases = limit_contract_cases,
        .power_lifecycle_tests = power_lifecycle_tests,
        .power_lifecycle_cases = power_lifecycle_cases,
    };
}

pub fn runEnvironmentContractProbe() bool {
    const saved_config = net_config.settings();
    defer net_config.restore(saved_config);

    var passed: u64 = 0;
    if (!environmentAdapterAbsence()) return false;
    passed += 1;
    if (!environmentMissingConfigDefaults()) return false;
    passed += 1;
    if (!environmentBadConfig()) return false;
    passed += 1;
    if (!environmentR4pRequiredContract()) return false;
    passed += 1;

    environment_contract_tests += 1;
    environment_contract_cases += passed;
    return true;
}

fn environmentAdapterAbsence() bool {
    if (adapter_count == 0) {
        const payload = "ENV";
        if (sendIpv4Payload(.{ 10, 0, 2, 1 }, icmp.IPV4_PROTOCOL, payload) != .no_adapter) return false;
        if (arpTestGateway() != .no_adapter) return false;
        if (dhcpAcquireDefault() != .no_adapter) return false;
        if (tcpConnect(.{ 10, 0, 2, 2 }, 80) != r4p_contract.TCP_RESULT_NO_CONNECTION) return false;
        return memEql(dhcp_stats.last_error, "no-adapter") and memEql(tcp.getStats().last_error, "no-adapter");
    }

    var frame: [ethernet.MIN_FRAME_SIZE]u8 = .{0} ** ethernet.MIN_FRAME_SIZE;
    if (transmit(MAX_ADAPTERS, frame[0..]) != .no_adapter) return false;
    if (dhcpAcquire(MAX_ADAPTERS) != .no_adapter) return false;
    return !resetAdapterLifecycle(MAX_ADAPTERS, "env-invalid-adapter");
}

fn environmentMissingConfigDefaults() bool {
    net_config.reset();
    const cfg = net_config.settings();
    return ipv4.sameIp(cfg.local_ip, net_config.DEFAULT_LOCAL_IP) and
        ipv4.sameIp(cfg.netmask, net_config.DEFAULT_NETMASK) and
        ipv4.sameIp(cfg.gateway_ip, net_config.DEFAULT_GATEWAY_IP) and
        ipv4.sameIp(cfg.dns_ip, net_config.DEFAULT_DNS_IP) and
        !cfg.configured and
        !cfg.dhcp_enabled and
        !cfg.dns_configured and
        cfg.invalid_options == 0 and
        memEql(net_config.sourceName(), "default");
}

fn environmentBadConfig() bool {
    var cfg: boot_config.Config = .{};
    addEnvConfigOption(&cfg, "NET", "ip", "999.1.1.1") orelse return false;
    net_config.applyBootConfig(&cfg);
    const current = net_config.settings();
    return current.invalid_options == 1 and
        memEql(current.last_error, "bad-ipv4") and
        memEql(net_config.sourceName(), "bad-ipv4");
}

fn environmentR4pRequiredContract() bool {
    const status = r4pRuntimeStatus();
    if (status.protocol_count != R4P_RUNTIME_PROTOCOLS.len) return false;
    if (status.active + status.missing != status.protocol_count) return false;
    for (R4P_RUNTIME_PROTOCOLS) |kind| {
        if (!protocol_registry.activeRole(protocolRole(kind))) return false;
        const stats = protocolRuntimeStats(kind);
        if (stats.builtin_fallback or stats.fallback_policy != FALLBACK_POLICY_NONE or stats.fallback_decision != FALLBACK_DECISION_NONE) return false;
    }
    return true;
}

fn addEnvConfigOption(config: *boot_config.Config, driver: []const u8, key: []const u8, value: []const u8) ?void {
    if (config.option_count >= config.options.len) return null;
    var opt: boot_config.Option = .{};
    opt.driver_len = copyEnvText(driver, opt.driver[0..]);
    opt.key_len = copyEnvText(key, opt.key[0..]);
    opt.value_len = copyEnvText(value, opt.value[0..]);
    config.options[config.option_count] = opt;
    config.option_count += 1;
}

fn copyEnvText(src: []const u8, dst: []u8) usize {
    const n = if (src.len < dst.len) src.len else dst.len - 1;
    if (n > 0) @memcpy(dst[0..n], src[0..n]);
    if (n < dst.len) dst[n] = 0;
    return n;
}

fn protocolRole(kind: ProtocolKind) []const u8 {
    return switch (kind) {
        .ethernet => "net.ethernet",
        .arp => "net.arp",
        .ipv4 => "net.ipv4",
        .icmp => "net.icmp",
        .udp => "net.udp",
        .dhcp => "net.dhcp",
        .dns => "net.dns",
        .tcp => "net.tcp",
        .serial_link => "net.serial_link",
    };
}

fn diagLifecycleStatus(adapter_index: usize, out: *BackendStatus) i32 {
    _ = adapter_index;
    out.* = .{
        .link_up = if (diag_lifecycle_link_up) 1 else 0,
    };
    return 0;
}

fn closeAllUdpSockets(reason: []const u8) u32 {
    var closed: u32 = 0;
    var index: usize = 0;
    while (index < udp_sockets.len) : (index += 1) {
        if (!udp_sockets[index].active) continue;
        udp_sockets[index] = .{};
        closed += 1;
    }
    if (closed != 0) udp_stats.last_error = reason;
    return closed;
}

fn cancelDhcpOperation(reason: []const u8) bool {
    const was_pending = dhcp_stats.operation_pending or dhcp_coordinator.operation_active;
    clearDhcpResponseExpectation();
    dhcp_stats.operation_pending = false;
    dhcp_stats.pending_label = "idle";
    if (was_pending) dhcp_stats.last_error = reason;
    const before_state = dhcp_coordinator.state;
    _ = dhcp_coordinator.cancel(time_core.monotonicTicks(), net_config.dhcpEnabled());
    logDhcpRuntimeIfChanged(before_state, reason);
    return was_pending;
}

fn cancelDnsOperation(reason: []const u8) bool {
    const was_pending = dns_stats.operation_pending;
    dns_stats.operation_pending = false;
    @memset(dns_stats.pending_name[0..], 0);
    if (was_pending) {
        dns_stats.last_result = dns.RESULT_TIMEOUT;
        dns_stats.last_error = reason;
    }
    return was_pending;
}

pub fn count() usize {
    return adapter_count;
}

pub fn get(index: usize) ?*const Adapter {
    if (index >= adapter_count) return null;
    return &adapters[index];
}

pub fn getMutable(index: usize) ?*Adapter {
    if (index >= adapter_count) return null;
    return &adapters[index];
}

pub fn refreshAdapterRuntime(index: usize) ?BackendStatus {
    return refreshAdapterRuntimeInternal(index, null);
}

fn refreshAdapterRuntimeInternal(index: usize, diagnostic_scope: ?DiagnosticCleanupScope) ?BackendStatus {
    if (!enterBackendCallback()) return null;
    defer leaveBackendCallback();
    if (index >= adapter_count) return null;
    if (adapters[index].ops.status) |status_fn| {
        var backend_status: BackendStatus = .{};
        if (status_fn(index, &backend_status) == 0) {
            const new_link: Link = if (backend_status.link_up != 0) .up else .down;
            const old_link = adapters[index].link;
            if (old_link != new_link) {
                setAdapterLink(index, new_link);
                if (new_link == .down and old_link == .up) {
                    if (index == 0 and net_config.dhcpEnabled()) {
                        dhcp_stats.lease.bound = false;
                        dhcp_stats.lease_acquired_tick = 0;
                        net_config.clearDhcpLeasePreservingMode("link-down");
                        arpFlush();
                    }
                    if (diagnostic_scope) |scope| {
                        _ = cleanupDiagnosticNetworkOperations("link-down", scope);
                    } else {
                        _ = cleanupNetworkOperations("link-down");
                    }
                }
            } else if (adapters[index].lifecycle == .registered or adapters[index].lifecycle == .resetting) {
                setAdapterLifecycle(index, lifecycleFromLink(new_link));
            }
            return backend_status;
        }
    }
    setAdapterLifecycle(index, lifecycleFromLink(adapters[index].link));
    return null;
}

pub fn hasAdapterForDevice(vendor_id: u16, device_id: u16) bool {
    var index: usize = 0;
    while (index < adapter_count) : (index += 1) {
        if (adapters[index].vendor_id == vendor_id and adapters[index].device_id == device_id) return true;
    }
    return false;
}

pub fn allocPacket() ?usize {
    var index: usize = 0;
    while (index < PACKET_POOL_SIZE) : (index += 1) {
        if (packet_used[index]) continue;
        packet_used[index] = true;
        packet_pool[index] = .{};
        return index;
    }
    packet_drops += 1;
    return null;
}

pub fn freePacket(handle: usize) void {
    if (handle >= PACKET_POOL_SIZE) return;
    packet_used[handle] = false;
    packet_pool[handle].len = 0;
}

pub fn packet(handle: usize) ?*Packet {
    if (handle >= PACKET_POOL_SIZE or !packet_used[handle]) return null;
    return &packet_pool[handle];
}

pub fn packetConst(handle: usize) ?*const Packet {
    if (handle >= PACKET_POOL_SIZE or !packet_used[handle]) return null;
    return &packet_pool[handle];
}

pub fn buildEthernetDiagFrame(out: []u8, src_mac: [6]u8) ?[]u8 {
    var op = newEthernetOp() orelse {
        ethernet_dispatch_failures += 1;
        eth_stats.last_error = "r4p-required";
        return null;
    };
    op.source_mac = src_mac;
    if (!ethernetDispatch(r4p_contract.ETHERNET_OP_BUILD_DIAG_FRAME, &op)) {
        eth_stats.last_error = "r4p-dispatch";
        return null;
    }
    if (op.result != r4p_contract.ETHERNET_RESULT_OK or op.frame_len > out.len) {
        ethernet_dispatch_failures += 1;
        eth_stats.last_error = "r4p-build";
        return null;
    }
    const len: usize = @intCast(op.frame_len);
    @memcpy(out[0..len], op.frame[0..len]);
    ethernet_r4p_build += 1;
    return out[0..len];
}

// 0.56.23: Verlust-Harness (NUR unter -Dnet-loss-test aktiv): jedes
// N-te empfangene Paket wird verworfen; TCP muss per Retransmit
// durchkommen. Nie im Default-Build wirksam.
// 0.56.23: jedes 64. RX-Frame (~1,5%) - der aktuelle Retransmit sendet
// nur das jeweils letzte Segment neu (kein Sliding-Window), das traegt
// gelegentlichen Einzelverlust; 6% ueberforderten den 64-KB-FTP-Transfer.
const NET_LOSS_EVERY_N: u64 = 64;
var net_loss_counter: u64 = 0;
var net_loss_drops: u64 = 0;

fn rxEnterCritical() u64 {
    const irq_flags = interrupts.saveAndDisable();
    scheduler.preemptDisable();
    return irq_flags;
}

fn rxLeaveCritical(irq_flags: u64) void {
    scheduler.preemptEnable();
    interrupts.restore(irq_flags);
}

/// Announces device-owned RX work without touching a packet buffer. This is
/// the only network operation intended for an R4D IRQ handler: the handler
/// acknowledges its cause, records one adapter bit and wakes `net-rx`.
pub fn scheduleRxWork(adapter_index: usize) bool {
    if (!enterBackendCallback()) return false;
    defer leaveBackendCallback();
    if (adapter_index >= adapter_count) {
        const irq_flags = rxEnterCritical();
        rx_schedule_invalid +%= 1;
        rxLeaveCritical(irq_flags);
        return false;
    }

    const stamp = time_core.monotonicCapture();
    const bit = @as(u16, 1) << @intCast(adapter_index);
    var should_signal = false;
    const irq_flags = rxEnterCritical();
    rx_schedule_calls +%= 1;
    if (irq_router.inDispatch()) rx_schedule_from_irq +%= 1;
    if ((rx_schedule_mask & bit) != 0) {
        rx_schedule_coalesced +%= 1;
    } else {
        rx_schedule_mask |= bit;
        rx_schedule_stamps[adapter_index] = stamp;
        should_signal = true;
    }
    rxLeaveCritical(irq_flags);
    if (should_signal) rx_work_event.signal();
    return true;
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
    if (irq_router.inDispatch()) {
        const irq_flags = rxEnterCritical();
        rx_irq_frame_rejects +%= 1;
        rxLeaveCritical(irq_flags);
        return .irq_context;
    }
    if (frame.len == 0 or frame.len > MAX_PACKET_SIZE) return .invalid_frame;
    if (!enterBackendCallback()) return .unavailable;
    defer leaveBackendCallback();
    if (adapter_index >= adapter_count) return .invalid_adapter;
    if (queue_index >= adapters[adapter_index].backend.rx_queue_count) return .invalid_frame;

    const decision = if (metadata_api)
        backend_contract.selectRxMetadata(adapters[adapter_index].backend.accepted, packet_flags, frame)
    else
        backend_contract.RxMetadataDecision.software;
    const metadata = rx_handoff.Metadata{
        .l4_checksum_valid = decision == .l4_checksum_valid,
        .software_fallback = decision == .software_fallback,
    };

    const enqueued_ns = time_core.monotonicNanoseconds() orelse 0;
    var should_signal = false;
    const irq_flags = rxEnterCritical();
    const was_empty = rx_handoff_queue.queuedCount() == 0;
    const result = rx_handoff_queue.enqueueWithMetadata(adapter_index, frame, enqueued_ns, metadata);
    switch (result) {
        .accepted => {
            should_signal = was_empty;
            if (metadata_api and packet_flags != 0) rx_metadata_submissions +%= 1;
            if (decision == .l4_checksum_valid) rx_metadata_selected +%= 1;
            if (decision == .software_fallback) rx_metadata_fallbacks +%= 1;
        },
        .busy => should_signal = true,
        .invalid_frame => {},
    }
    rxLeaveCritical(irq_flags);
    if (should_signal) rx_work_event.signal();
    return switch (result) {
        .accepted => if (decision == .software_fallback) .accepted_fallback else .accepted,
        .invalid_frame => .invalid_frame,
        .busy => .queue_busy,
    };
}

fn processReceivedFrame(adapter_index: usize, frame: []const u8, metadata: rx_handoff.Metadata) bool {
    if (adapter_index >= adapter_count) return false;
    if (comptime kernel_config.enable_net_loss_test) {
        net_loss_counter +%= 1;
        if (net_loss_counter % NET_LOSS_EVERY_N == 0) {
            net_loss_drops +%= 1;
            return false;
        }
    }
    if (frame.len == 0 or frame.len > MAX_PACKET_SIZE) {
        adapters[adapter_index].stats.drops += 1;
        adapters[adapter_index].stats.last_error = "rx-size";
        return false;
    }
    if (!ethernetHandleRx(adapters[adapter_index].mac, frame)) {
        adapters[adapter_index].stats.drops += 1;
        adapters[adapter_index].stats.last_error = eth_stats.last_error;
        return false;
    }
    const frame_type = ethernetFrameType(frame);
    if (frame_type == ethernet.TYPE_ARP) {
        const before_updates = arp_stats.cache_updates;
        arpHandleRx(frame);
        sendArpReplyIfForUs(adapter_index, frame);
        if (arp_stats.cache_updates != before_updates and arp_stats.cache_valid) {
            rememberArpPeerFromStats();
        }
        adapters[adapter_index].stats.rx_packets += 1;
        adapters[adapter_index].stats.rx_bytes += frame.len;
        return true;
    }
    if (frame_type == ethernet.TYPE_IPV4) {
        const ip_view = ipv4HandleRx(frame) orelse {
            adapters[adapter_index].stats.drops += 1;
            adapters[adapter_index].stats.last_error = ipv4_stats.last_error;
            return false;
        };
        var source_mac: [6]u8 = .{0} ** 6;
        copyMacFromBytes(&source_mac, frame[6..12]);
        learnIpv4Sender(ip_view.source_ip, source_mac);
        enqueueAppIpv4(ip_view);
        icmpHandleRx(ip_view);
        const l4_checksum_valid = metadata.l4_checksum_valid and
            (ip_view.protocol == udp.IPV4_PROTOCOL or ip_view.protocol == tcp.IPV4_PROTOCOL);
        if (l4_checksum_valid) {
            rx_l4_offload_used +%= 1;
        } else if (ip_view.protocol == udp.IPV4_PROTOCOL or ip_view.protocol == tcp.IPV4_PROTOCOL) {
            rx_l4_software_checks +%= 1;
        }
        if (udpHandleRxMetadata(ip_view, l4_checksum_valid)) |udp_view| {
            dispatchUdpSocketDatagram(udp_view);
            if ((udp_view.dest_port == dhcp.CLIENT_PORT or udp_view.source_port == dhcp.SERVER_PORT) and !udpSocketPortBound(udp_view.dest_port)) _ = dhcpHandleMessage(udp_view.payload);
            if ((udp_view.dest_port == dns.PORT or udp_view.source_port == dns.PORT) and !udpSocketPortBound(udp_view.dest_port)) _ = dnsHandleResponse(udp_view.payload);
        }
        tcpHandleRxMetadata(ip_view, l4_checksum_valid);
        if (icmpIsEchoRequest(ip_view.payload)) sendIcmpEchoReply(adapter_index, frame, ip_view);
        adapters[adapter_index].stats.rx_packets += 1;
        adapters[adapter_index].stats.rx_bytes += frame.len;
        return true;
    }
    // Unsupported EtherTypes have already passed the Ethernet filter and are
    // accounted by ethernetHandleRx(). There is no raw-frame consumer, so
    // retaining them in the small diagnostic packet pool would leak one slot
    // per IPv6/LLDP/etc. frame until normal LAN traffic exhausts the pool.
    adapters[adapter_index].stats.rx_packets += 1;
    adapters[adapter_index].stats.rx_bytes += frame.len;
    return true;
}

fn processOneRxHandoff() bool {
    // The callback reference prevents an adapter removal from reindexing the
    // claimed frame while protocol code runs or yields.
    if (!enterBackendCallback()) return false;
    defer leaveBackendCallback();

    const irq_flags = rxEnterCritical();
    const claim = rx_handoff_queue.claim() orelse {
        rxLeaveCritical(irq_flags);
        return false;
    };
    const frame = rx_handoff_queue.frame(claim) orelse {
        rx_release_failures +%= 1;
        _ = rx_handoff_queue.release(claim);
        rxLeaveCritical(irq_flags);
        return false;
    };
    rxLeaveCritical(irq_flags);

    const process_start_ns = time_core.monotonicNanoseconds() orelse 0;
    rx_processing_depth +%= 1;
    const protocol_ok = processReceivedFrame(claim.adapter_index, frame, claim.metadata);
    rx_processing_depth -= 1;

    const finish_flags = rxEnterCritical();
    if (protocol_ok) {
        rx_protocol_success +%= 1;
    } else {
        rx_protocol_failures +%= 1;
    }
    if (!rx_handoff_queue.release(claim)) rx_release_failures +%= 1;
    if (claim.enqueued_ns != 0 and process_start_ns >= claim.enqueued_ns) {
        const latency = process_start_ns - claim.enqueued_ns;
        rx_handoff_latency_samples +%= 1;
        rx_handoff_latency_total_ns +%= latency;
        rx_handoff_latency_last_ns = latency;
        if (latency > rx_handoff_latency_max_ns) rx_handoff_latency_max_ns = latency;
    } else {
        rx_latency_unavailable +%= 1;
    }
    rxLeaveCritical(finish_flags);
    return true;
}

fn processRxHandoffBatch(budget: usize) usize {
    if (budget == 0) return 0;
    var processed: usize = 0;
    while (processed < budget and processOneRxHandoff()) : (processed += 1) {}
    if (processed == 0) return 0;

    const irq_flags = rxEnterCritical();
    rx_batches +%= 1;
    if (processed > rx_batch_max) rx_batch_max = @intCast(processed);
    if (processed == budget and rx_handoff_queue.queuedCount() != 0) rx_budget_exhaustions +%= 1;
    rxLeaveCritical(irq_flags);
    return processed;
}

fn rxHandoffHasReadyWork() bool {
    const irq_flags = rxEnterCritical();
    const ready = rx_handoff_queue.queuedCount() != 0;
    rxLeaveCritical(irq_flags);
    return ready;
}

fn takeScheduledAdapters() u16 {
    var stamps: [MAX_ADAPTERS]time_core.MonotonicStamp =
        .{time_core.MonotonicStamp{}} ** MAX_ADAPTERS;
    const irq_flags = rxEnterCritical();
    const mask = rx_schedule_mask;
    rx_schedule_mask = 0;
    var index: usize = 0;
    while (index < MAX_ADAPTERS) : (index += 1) {
        const bit = @as(u16, 1) << @intCast(index);
        if ((mask & bit) == 0) continue;
        stamps[index] = rx_schedule_stamps[index];
        rx_schedule_stamps[index] = .{};
    }
    rxLeaveCritical(irq_flags);

    var samples: u64 = 0;
    var total_ns: u64 = 0;
    var last_ns: u64 = 0;
    var max_ns: u64 = 0;
    var unavailable: u64 = 0;
    index = 0;
    while (index < MAX_ADAPTERS) : (index += 1) {
        const bit = @as(u16, 1) << @intCast(index);
        if ((mask & bit) == 0) continue;
        const latency = time_core.monotonicElapsedSince(stamps[index]) orelse {
            unavailable += 1;
            continue;
        };
        samples += 1;
        total_ns +%= latency;
        last_ns = latency;
        if (latency > max_ns) max_ns = latency;
    }
    if (samples != 0 or unavailable != 0) {
        const metric_flags = rxEnterCritical();
        rx_schedule_latency_samples +%= samples;
        rx_schedule_latency_total_ns +%= total_ns;
        rx_schedule_latency_last_ns = last_ns;
        if (max_ns > rx_schedule_latency_max_ns) rx_schedule_latency_max_ns = max_ns;
        rx_latency_unavailable +%= unavailable;
        rxLeaveCritical(metric_flags);
    }
    return mask;
}

fn cancelRxHandoffQueue() usize {
    const irq_flags = rxEnterCritical();
    const cancelled = rx_handoff_queue.cancelAll();
    rx_schedule_mask = 0;
    rx_schedule_stamps = .{time_core.MonotonicStamp{}} ** MAX_ADAPTERS;
    rxLeaveCritical(irq_flags);
    return cancelled;
}

pub fn transmit(adapter_index: usize, frame: []const u8) TxResult {
    if (!enterBackendCallback()) return recordTxResult(.link_down);
    defer leaveBackendCallback();
    if (adapter_index >= adapter_count) return recordTxResult(.no_adapter);
    if (frame.len == 0 or frame.len > MAX_PACKET_SIZE or frame.len > adapters[adapter_index].mtu + 18) {
        adapters[adapter_index].stats.drops += 1;
        adapters[adapter_index].stats.last_error = "tx-size";
        return recordTxResult(.too_large);
    }
    if (adapters[adapter_index].link == .down) {
        adapters[adapter_index].stats.last_error = "tx-link-down";
        setAdapterLifecycle(adapter_index, .link_down);
        return recordTxResult(.link_down);
    }
    const tx = adapters[adapter_index].ops.transmit orelse {
        adapters[adapter_index].stats.last_error = "tx-unsupported";
        return recordTxResult(.unsupported);
    };
    const result = tx(adapter_index, frame);
    if (result == .ok) {
        _ = recordTxResult(.ok);
        if (adapters[adapter_index].link == .unknown) adapters[adapter_index].link = .up;
        setAdapterLifecycle(adapter_index, .active);
        adapters[adapter_index].stats.tx_packets += 1;
        adapters[adapter_index].stats.tx_bytes += frame.len;
        adapters[adapter_index].stats.last_error = "none";
        _ = ethernetHandleTx(frame);
        const frame_type = ethernetFrameType(frame);
        if (frame_type == ethernet.TYPE_ARP) arpHandleTx(frame);
        if (frame_type == ethernet.TYPE_IPV4) {
            if (ipv4HandleTx(frame)) |ip_view| {
                icmpHandleTx(ip_view);
                _ = udpHandleTx(ip_view);
                tcpHandleTx(ip_view);
            }
        }
    } else {
        if (result == .link_down) {
            if (adapters[adapter_index].link == .up) {
                setAdapterLink(adapter_index, .down);
                _ = cleanupNetworkOperations("link-down");
            } else {
                adapters[adapter_index].link = .down;
                setAdapterLifecycle(adapter_index, .link_down);
            }
        }
        adapters[adapter_index].stats.errors += 1;
        adapters[adapter_index].stats.last_error = txResultName(result);
        _ = recordTxResult(result);
    }
    return result;
}

fn recordTxResult(result: TxResult) TxResult {
    tx_backpressure.last_result = result;
    tx_backpressure.has_result = true;
    switch (result) {
        .ok => {},
        .no_adapter => {
            tx_backpressure.failures += 1;
            tx_backpressure.no_adapter += 1;
        },
        .link_down => {
            tx_backpressure.failures += 1;
            tx_backpressure.link_down += 1;
        },
        .busy => {
            tx_backpressure.failures += 1;
            tx_backpressure.busy += 1;
        },
        .too_large => {
            tx_backpressure.failures += 1;
            tx_backpressure.too_large += 1;
        },
        .unsupported => {
            tx_backpressure.failures += 1;
            tx_backpressure.unsupported += 1;
        },
        .backend_error => {
            tx_backpressure.failures += 1;
            tx_backpressure.backend_error += 1;
        },
    }
    return result;
}

pub fn candidateCount() usize {
    var total: usize = 0;
    var index: usize = 0;
    while (pci_inventory.deviceAt(index)) |d| : (index += 1) {
        if (isNetwork(d.class_code, d.subclass)) total += 1;
    }
    return total;
}

fn ethernetHandleRx(own_mac: [6]u8, frame: []const u8) bool {
    var op = newEthernetOp() orelse {
        ethernet_dispatch_failures += 1;
        eth_stats.last_error = "r4p-required";
        return false;
    };
    op.own_mac = own_mac;
    if (!setEthernetFrame(&op, frame)) {
        ethernet_dispatch_failures += 1;
        eth_stats.last_error = "rx-size";
        return false;
    }
    if (!ethernetDispatch(r4p_contract.ETHERNET_OP_HANDLE_RX, &op)) {
        eth_stats.last_error = "r4p-dispatch";
        return false;
    }
    applyEthernetRxResult(op);
    if (op.result == r4p_contract.ETHERNET_RESULT_OK) {
        ethernet_r4p_rx += 1;
        return true;
    }
    return false;
}

fn ethernetHandleTx(frame: []const u8) bool {
    var op = newEthernetOp() orelse {
        ethernet_dispatch_failures += 1;
        eth_stats.last_error = "r4p-required";
        return false;
    };
    if (!setEthernetFrame(&op, frame)) {
        ethernet_dispatch_failures += 1;
        eth_stats.last_error = "tx-size";
        return false;
    }
    if (!ethernetDispatch(r4p_contract.ETHERNET_OP_HANDLE_TX, &op)) {
        eth_stats.last_error = "r4p-dispatch";
        return false;
    }
    applyEthernetTxResult(op);
    if (op.result == r4p_contract.ETHERNET_RESULT_OK) {
        ethernet_r4p_tx += 1;
        return true;
    }
    return false;
}

fn ethernetFrameType(frame: []const u8) u16 {
    var op = newEthernetOp() orelse {
        ethernet_dispatch_failures += 1;
        eth_stats.last_error = "r4p-required";
        return 0;
    };
    if (!setEthernetFrame(&op, frame)) {
        ethernet_dispatch_failures += 1;
        eth_stats.last_error = "rx-size";
        return 0;
    }
    if (!ethernetDispatch(r4p_contract.ETHERNET_OP_FRAME_TYPE, &op)) {
        eth_stats.last_error = "r4p-dispatch";
        return 0;
    }
    return op.ethertype;
}

fn newEthernetOp() ?r4p_contract.EthernetFrameOp {
    if (!r4p.hasActiveR4p("net.ethernet")) return null;
    return .{};
}

fn setEthernetFrame(op: *r4p_contract.EthernetFrameOp, frame: []const u8) bool {
    if (frame.len > op.frame.len) return false;
    op.frame_len = @intCast(frame.len);
    if (frame.len > 0) @memcpy(op.frame[0..frame.len], frame);
    return true;
}

fn ethernetDispatch(opcode: u32, op: *r4p_contract.EthernetFrameOp) bool {
    var buffer = protocol_api.ProtocolBuffer{
        .data = @ptrCast(op),
        .len = @sizeOf(r4p_contract.EthernetFrameOp),
        .capacity = @sizeOf(r4p_contract.EthernetFrameOp),
        .flags = 0,
        .reserved = 0,
    };
    const result = r4p.dispatch("net.ethernet", opcode, &buffer, &buffer);
    if (result < 0 and result != r4p_contract.ETHERNET_RESULT_SHORT and result != r4p_contract.ETHERNET_RESULT_FILTERED) {
        ethernet_dispatch_failures += 1;
        return false;
    }
    return true;
}

fn applyEthernetRxResult(op: r4p_contract.EthernetFrameOp) void {
    eth_stats.last_ethertype = op.ethertype;
    switch (op.result) {
        r4p_contract.ETHERNET_RESULT_OK => {
            if ((op.flags & r4p_contract.ETHERNET_FLAG_BROADCAST) != 0) eth_stats.broadcast += 1;
            if ((op.flags & r4p_contract.ETHERNET_FLAG_OWN_UNICAST) != 0) eth_stats.own_unicast += 1;
            eth_stats.rx += 1;
            classifyEthernetStats(op.ethertype);
            eth_stats.last_error = "none";
        },
        r4p_contract.ETHERNET_RESULT_SHORT => {
            eth_stats.dropped_short += 1;
            eth_stats.last_error = "rx-short";
        },
        r4p_contract.ETHERNET_RESULT_FILTERED => {
            eth_stats.dropped_filter += 1;
            eth_stats.last_error = "rx-filter";
        },
        else => {
            eth_stats.last_error = "r4p-rx-error";
        },
    }
}

fn applyEthernetTxResult(op: r4p_contract.EthernetFrameOp) void {
    eth_stats.last_ethertype = op.ethertype;
    switch (op.result) {
        r4p_contract.ETHERNET_RESULT_OK => {
            if ((op.flags & r4p_contract.ETHERNET_FLAG_BROADCAST) != 0) eth_stats.broadcast += 1;
            eth_stats.tx += 1;
            classifyEthernetStats(op.ethertype);
            eth_stats.last_error = "none";
        },
        r4p_contract.ETHERNET_RESULT_SHORT => {
            eth_stats.dropped_short += 1;
            eth_stats.last_error = "tx-short";
        },
        else => {
            eth_stats.last_error = "r4p-tx-error";
        },
    }
}

fn classifyEthernetStats(ethertype: u16) void {
    switch (ethertype) {
        ethernet.TYPE_IPV4, ethernet.TYPE_ARP => {},
        ethernet.TYPE_R4OS_DIAG => eth_stats.test_frames += 1,
        else => eth_stats.unknown_ethertype += 1,
    }
}

fn arpBuildRequest(out: []u8, source_mac: [6]u8, target_ip: [4]u8) ?[]u8 {
    var op = newArpOp() orelse {
        arp_dispatch_failures += 1;
        arp_stats.last_error = "r4p-required";
        return null;
    };
    op.local_ip = net_config.localIp();
    op.source_mac = source_mac;
    op.target_ip = target_ip;
    if (!arpDispatch(r4p_contract.ARP_OP_BUILD_REQUEST, &op)) {
        arp_stats.last_error = "r4p-dispatch";
        return null;
    }
    if (op.result != r4p_contract.ARP_RESULT_OK or op.frame_len > out.len) {
        arp_dispatch_failures += 1;
        arp_stats.last_error = "r4p-build";
        return null;
    }
    const len: usize = @intCast(op.frame_len);
    @memcpy(out[0..len], op.frame[0..len]);
    arp_r4p_build += 1;
    return out[0..len];
}

fn arpHandleRx(frame: []const u8) void {
    var op = newArpOp() orelse {
        arp_dispatch_failures += 1;
        arp_stats.last_error = "r4p-required";
        return;
    };
    if (!setArpFrame(&op, frame) or !arpDispatch(r4p_contract.ARP_OP_HANDLE_RX, &op)) {
        arp_stats.last_error = "r4p-dispatch";
        return;
    }
    applyArpRxResult(op);
    arp_r4p_rx += 1;
}

fn arpHandleTx(frame: []const u8) void {
    var op = newArpOp() orelse {
        arp_dispatch_failures += 1;
        arp_stats.last_error = "r4p-required";
        return;
    };
    if (!setArpFrame(&op, frame) or !arpDispatch(r4p_contract.ARP_OP_HANDLE_TX, &op)) {
        arp_stats.last_error = "r4p-dispatch";
        return;
    }
    applyArpTxResult(op);
    arp_r4p_tx += 1;
}

fn newArpOp() ?r4p_contract.ArpOp {
    if (!r4p.hasActiveR4p("net.arp")) return null;
    return .{};
}

fn setArpFrame(op: *r4p_contract.ArpOp, frame: []const u8) bool {
    if (frame.len > op.frame.len) return false;
    op.frame_len = @intCast(frame.len);
    if (frame.len > 0) @memcpy(op.frame[0..frame.len], frame);
    return true;
}

fn arpDispatch(opcode: u32, op: *r4p_contract.ArpOp) bool {
    var buffer = protocol_api.ProtocolBuffer{
        .data = @ptrCast(op),
        .len = @sizeOf(r4p_contract.ArpOp),
        .capacity = @sizeOf(r4p_contract.ArpOp),
        .flags = 0,
        .reserved = 0,
    };
    const result = r4p.dispatch("net.arp", opcode, &buffer, &buffer);
    if ((result == -5 or result == -4) and op.result == 0) {
        arp_dispatch_failures += 1;
        return false;
    }
    return true;
}

fn applyArpRxResult(op: r4p_contract.ArpOp) void {
    arp_stats.last_opcode = op.opcode;
    switch (op.result) {
        r4p_contract.ARP_RESULT_OK => {
            if ((op.flags & r4p_contract.ARP_FLAG_REPLY) != 0) {
                arp_stats.replies_rx += 1;
                arp_stats.cache_mac = op.sender_mac;
                arp_stats.cache_ip = op.sender_ip;
                arp_stats.cache_valid = true;
                arp_stats.cache_updates += 1;
                arp_stats.last_error = "none";
            } else if ((op.flags & r4p_contract.ARP_FLAG_REQUEST) != 0) {
                arp_stats.requests_rx += 1;
                learnArpRequestSender(op.sender_ip, op.sender_mac, op.seen_target_ip);
                arp_stats.last_error = "rx-request";
            } else {
                arp_stats.last_error = "none";
            }
        },
        r4p_contract.ARP_RESULT_SHORT => {
            arp_stats.malformed += 1;
            arp_stats.last_error = "rx-short";
        },
        r4p_contract.ARP_RESULT_SHAPE => {
            arp_stats.malformed += 1;
            arp_stats.last_error = "rx-shape";
        },
        r4p_contract.ARP_RESULT_OPCODE => {
            arp_stats.malformed += 1;
            arp_stats.last_error = "rx-opcode";
        },
        else => {
            arp_stats.last_error = "rx-not-arp";
        },
    }
}

fn applyArpTxResult(op: r4p_contract.ArpOp) void {
    arp_stats.last_opcode = op.opcode;
    switch (op.result) {
        r4p_contract.ARP_RESULT_OK => {
            if ((op.flags & r4p_contract.ARP_FLAG_REQUEST) != 0) {
                arp_stats.requests_tx += 1;
                arp_stats.last_error = "none";
            }
        },
        r4p_contract.ARP_RESULT_SHORT => {
            arp_stats.malformed += 1;
            arp_stats.last_error = "tx-short";
        },
        r4p_contract.ARP_RESULT_OPCODE => {
            arp_stats.malformed += 1;
            arp_stats.last_error = "tx-opcode";
        },
        else => {
            arp_stats.last_error = "tx-not-arp";
        },
    }
}

fn ipv4BuildPacket(out: []u8, source_mac: [6]u8, dest_mac: [6]u8, dest_ip: [4]u8, protocol: u8, payload: []const u8) ?[]u8 {
    return ipv4BuildPacketFrom(out, source_mac, dest_mac, net_config.localIp(), dest_ip, protocol, payload);
}

fn ipv4BuildPacketFrom(out: []u8, source_mac: [6]u8, dest_mac: [6]u8, source_ip: [4]u8, dest_ip: [4]u8, protocol: u8, payload: []const u8) ?[]u8 {
    var op = newIpv4Op() orelse {
        ipv4_dispatch_failures += 1;
        ipv4_stats.last_error = "r4p-required";
        return null;
    };
    if (payload.len > op.payload.len) {
        ipv4_dispatch_failures += 1;
        ipv4_stats.last_error = "payload-size";
        return null;
    }
    op.source_mac = source_mac;
    op.dest_mac = dest_mac;
    op.source_ip = source_ip;
    op.dest_ip = dest_ip;
    op.protocol = protocol;
    op.ttl = 64;
    op.payload_len = @intCast(payload.len);
    if (payload.len > 0) @memcpy(op.payload[0..payload.len], payload);
    if (!ipv4Dispatch(r4p_contract.IPV4_OP_BUILD_PACKET, &op)) {
        ipv4_stats.last_error = "r4p-dispatch";
        return null;
    }
    if (op.result != r4p_contract.IPV4_RESULT_OK or op.frame_len > out.len) {
        ipv4_dispatch_failures += 1;
        ipv4_stats.last_error = "r4p-build";
        return null;
    }
    const len: usize = @intCast(op.frame_len);
    @memcpy(out[0..len], op.frame[0..len]);
    ipv4_r4p_build += 1;
    return out[0..len];
}

fn ipv4HandleRx(frame: []const u8) ?ipv4.PacketView {
    var op = newIpv4Op() orelse {
        ipv4_dispatch_failures += 1;
        ipv4_stats.last_error = "r4p-required";
        return null;
    };
    op.local_ip = net_config.localIp();
    if (!setIpv4Frame(&op, frame) or !ipv4Dispatch(r4p_contract.IPV4_OP_HANDLE_RX, &op)) {
        ipv4_stats.last_error = "r4p-dispatch";
        return null;
    }
    applyIpv4RxResult(op);
    if (op.result != r4p_contract.IPV4_RESULT_OK) return null;
    ipv4_r4p_rx += 1;
    return ipv4ViewFromOp(op);
}

fn ipv4HandleTx(frame: []const u8) ?ipv4.PacketView {
    var op = newIpv4Op() orelse {
        ipv4_dispatch_failures += 1;
        ipv4_stats.last_error = "r4p-required";
        return null;
    };
    if (!setIpv4Frame(&op, frame) or !ipv4Dispatch(r4p_contract.IPV4_OP_HANDLE_TX, &op)) {
        ipv4_stats.last_error = "r4p-dispatch";
        return null;
    }
    applyIpv4TxResult(op);
    if (op.result != r4p_contract.IPV4_RESULT_OK) return null;
    ipv4_r4p_tx += 1;
    return ipv4ViewFromOp(op);
}

fn newIpv4Op() ?r4p_contract.Ipv4Op {
    if (!r4p.hasActiveR4p("net.ipv4")) return null;
    return .{};
}

fn setIpv4Frame(op: *r4p_contract.Ipv4Op, frame: []const u8) bool {
    if (frame.len > op.frame.len) return false;
    op.frame_len = @intCast(frame.len);
    if (frame.len > 0) @memcpy(op.frame[0..frame.len], frame);
    return true;
}

fn ipv4Dispatch(opcode: u32, op: *r4p_contract.Ipv4Op) bool {
    var buffer = protocol_api.ProtocolBuffer{
        .data = @ptrCast(op),
        .len = @sizeOf(r4p_contract.Ipv4Op),
        .capacity = @sizeOf(r4p_contract.Ipv4Op),
        .flags = 0,
        .reserved = 0,
    };
    const result = r4p.dispatch("net.ipv4", opcode, &buffer, &buffer);
    if ((result == -5 or result == -4) and op.result == 0) {
        ipv4_dispatch_failures += 1;
        return false;
    }
    return true;
}

fn ipv4ViewFromOp(op: r4p_contract.Ipv4Op) ipv4.PacketView {
    const len: usize = @min(@as(usize, @intCast(op.payload_len)), ipv4_payload_scratch.len);
    if (len > 0) @memcpy(ipv4_payload_scratch[0..len], op.payload[0..len]);
    return .{
        .protocol = op.protocol,
        .source_ip = op.source_ip,
        .dest_ip = op.dest_ip,
        .payload = ipv4_payload_scratch[0..len],
    };
}

fn applyIpv4RxResult(op: r4p_contract.Ipv4Op) void {
    ipv4_stats.last_protocol = op.protocol;
    ipv4_stats.last_source = op.source_ip;
    ipv4_stats.last_dest = op.dest_ip;
    switch (op.result) {
        r4p_contract.IPV4_RESULT_OK => {
            ipv4_stats.rx_packets += 1;
            ipv4_stats.last_error = "none";
        },
        r4p_contract.IPV4_RESULT_SHORT => {
            ipv4_stats.dropped_short += 1;
            ipv4_stats.last_error = "short";
        },
        r4p_contract.IPV4_RESULT_VERSION => {
            ipv4_stats.dropped_version += 1;
            ipv4_stats.last_error = "version-ihl";
        },
        r4p_contract.IPV4_RESULT_LENGTH => {
            ipv4_stats.malformed += 1;
            ipv4_stats.last_error = "length";
        },
        r4p_contract.IPV4_RESULT_FRAGMENT => {
            ipv4_stats.dropped_fragment += 1;
            ipv4_stats.last_error = "fragment";
        },
        r4p_contract.IPV4_RESULT_CHECKSUM => {
            ipv4_stats.dropped_checksum += 1;
            ipv4_stats.last_error = "checksum";
        },
        r4p_contract.IPV4_RESULT_DESTINATION => {
            ipv4_stats.dropped_destination += 1;
            ipv4_stats.last_error = "not-local";
        },
        else => {
            ipv4_stats.malformed += 1;
            ipv4_stats.last_error = "not-ipv4";
        },
    }
}

fn applyIpv4TxResult(op: r4p_contract.Ipv4Op) void {
    ipv4_stats.last_protocol = op.protocol;
    ipv4_stats.last_source = op.source_ip;
    ipv4_stats.last_dest = op.dest_ip;
    switch (op.result) {
        r4p_contract.IPV4_RESULT_OK => {
            ipv4_stats.tx_packets += 1;
            ipv4_stats.last_error = "none";
        },
        r4p_contract.IPV4_RESULT_SHORT => ipv4_stats.last_error = "short",
        r4p_contract.IPV4_RESULT_VERSION => ipv4_stats.last_error = "version-ihl",
        r4p_contract.IPV4_RESULT_LENGTH => ipv4_stats.last_error = "length",
        r4p_contract.IPV4_RESULT_FRAGMENT => ipv4_stats.last_error = "fragment",
        r4p_contract.IPV4_RESULT_CHECKSUM => ipv4_stats.last_error = "checksum",
        else => ipv4_stats.last_error = "not-ipv4",
    }
}

fn rejectIpv4TxTooLarge(protocol: u8, target_ip: [4]u8, reason: []const u8) TxResult {
    ipv4_stats.dropped_tx_too_large += 1;
    ipv4_stats.last_protocol = protocol;
    ipv4_stats.last_source = net_config.localIp();
    ipv4_stats.last_dest = target_ip;
    ipv4_stats.last_error = reason;
    // 0.56.37: auch in der Backpressure-Sicht zaehlen - der tx-mtu-
    // Reject war in NETDIAG BACKPRESSURE unsichtbar (tx_large=0),
    // was die Diagnose der 4-KB-Write-Regression massiv verzoegert hat.
    return recordTxResult(.too_large);
}

fn icmpBuildEchoRequest(out: []u8, ident: u16, seq: u16) ?[]u8 {
    var op = newIcmpOp() orelse {
        icmp_dispatch_failures += 1;
        icmp_stats.last_error = "r4p-required";
        return null;
    };
    op.ident = ident;
    op.seq = seq;
    if (!icmpDispatch(r4p_contract.ICMP_OP_BUILD_ECHO_REQUEST, &op)) {
        icmp_stats.last_error = "r4p-dispatch";
        return null;
    }
    if (op.result != r4p_contract.ICMP_RESULT_OK or op.payload_len > out.len) {
        icmp_dispatch_failures += 1;
        icmp_stats.last_error = "r4p-build";
        return null;
    }
    const len: usize = @intCast(op.payload_len);
    @memcpy(out[0..len], op.payload[0..len]);
    icmp_r4p_build += 1;
    return out[0..len];
}

fn icmpBuildEchoReply(out: []u8, request: []const u8) ?[]u8 {
    var op = newIcmpOp() orelse {
        icmp_dispatch_failures += 1;
        icmp_stats.last_error = "r4p-required";
        return null;
    };
    if (!setIcmpPayload(&op, request) or !icmpDispatch(r4p_contract.ICMP_OP_BUILD_ECHO_REPLY, &op)) {
        icmp_stats.last_error = "r4p-dispatch";
        return null;
    }
    if (op.result != r4p_contract.ICMP_RESULT_OK or op.payload_len > out.len) {
        icmp_dispatch_failures += 1;
        icmp_stats.last_error = "r4p-build";
        return null;
    }
    const len: usize = @intCast(op.payload_len);
    @memcpy(out[0..len], op.payload[0..len]);
    icmp_r4p_build += 1;
    return out[0..len];
}

fn icmpHandleRx(ip_view: ipv4.PacketView) void {
    if (ip_view.protocol != icmp.IPV4_PROTOCOL) return;
    var op = newIcmpOp() orelse {
        icmp_dispatch_failures += 1;
        icmp_stats.last_error = "r4p-required";
        return;
    };
    if (!setIcmpPayload(&op, ip_view.payload) or !icmpDispatch(r4p_contract.ICMP_OP_HANDLE_RX, &op)) {
        icmp_stats.last_error = "r4p-dispatch";
        return;
    }
    applyIcmpRxResult(op);
    if (op.result == r4p_contract.ICMP_RESULT_OK) icmp_r4p_rx += 1;
}

fn icmpHandleTx(ip_view: ipv4.PacketView) void {
    if (ip_view.protocol != icmp.IPV4_PROTOCOL) return;
    var op = newIcmpOp() orelse {
        icmp_dispatch_failures += 1;
        icmp_stats.last_error = "r4p-required";
        return;
    };
    if (!setIcmpPayload(&op, ip_view.payload) or !icmpDispatch(r4p_contract.ICMP_OP_HANDLE_TX, &op)) {
        icmp_stats.last_error = "r4p-dispatch";
        return;
    }
    applyIcmpTxResult(op);
    if (op.result == r4p_contract.ICMP_RESULT_OK) icmp_r4p_tx += 1;
}

fn icmpIsEchoRequest(payload: []const u8) bool {
    var op = newIcmpOp() orelse {
        icmp_dispatch_failures += 1;
        icmp_stats.last_error = "r4p-required";
        return false;
    };
    if (!setIcmpPayload(&op, payload) or !icmpDispatch(r4p_contract.ICMP_OP_IS_ECHO_REQUEST, &op)) {
        icmp_stats.last_error = "r4p-dispatch";
        return false;
    }
    if (op.result != r4p_contract.ICMP_RESULT_OK) return false;
    icmp_r4p_classify += 1;
    return (op.flags & r4p_contract.ICMP_FLAG_ECHO_REQUEST) != 0;
}

fn newIcmpOp() ?r4p_contract.IcmpOp {
    if (!r4p.hasActiveR4p("net.icmp")) return null;
    return .{};
}

fn setIcmpPayload(op: *r4p_contract.IcmpOp, payload: []const u8) bool {
    if (payload.len > op.payload.len) return false;
    op.payload_len = @intCast(payload.len);
    if (payload.len > 0) @memcpy(op.payload[0..payload.len], payload);
    return true;
}

fn icmpDispatch(opcode: u32, op: *r4p_contract.IcmpOp) bool {
    var buffer = protocol_api.ProtocolBuffer{
        .data = @ptrCast(op),
        .len = @sizeOf(r4p_contract.IcmpOp),
        .capacity = @sizeOf(r4p_contract.IcmpOp),
        .flags = 0,
        .reserved = 0,
    };
    const result = r4p.dispatch("net.icmp", opcode, &buffer, &buffer);
    if ((result == -5 or result == -4) and op.result == 0) {
        icmp_dispatch_failures += 1;
        return false;
    }
    return true;
}

fn applyIcmpRxResult(op: r4p_contract.IcmpOp) void {
    icmp_stats.last_type = op.typ;
    icmp_stats.last_code = op.code;
    icmp_stats.last_id = op.ident;
    icmp_stats.last_seq = op.seq;
    switch (op.result) {
        r4p_contract.ICMP_RESULT_OK => {
            icmp_stats.rx_packets += 1;
            if ((op.flags & r4p_contract.ICMP_FLAG_ECHO_REPLY) != 0) icmp_stats.echo_replies_rx += 1;
            if ((op.flags & r4p_contract.ICMP_FLAG_ECHO_REQUEST) != 0) icmp_stats.echo_requests_rx += 1;
            classifyIcmpRx(op.typ, op.code);
        },
        r4p_contract.ICMP_RESULT_SHORT => {
            icmp_stats.malformed += 1;
            icmp_stats.last_error = "short";
        },
        r4p_contract.ICMP_RESULT_CHECKSUM => {
            icmp_stats.checksum_errors += 1;
            icmp_stats.last_error = "checksum";
        },
        else => icmp_stats.last_error = "not-icmp",
    }
}

fn classifyIcmpRx(typ: u8, code: u8) void {
    if (typ == 0 and code == 0) {
        icmp_stats.last_error = "none";
    } else if (typ == 8 and code == 0) {
        icmp_stats.last_error = "none";
    } else if (typ == 3 and code == 3) {
        icmp_stats.destination_unreachable_rx += 1;
        icmp_stats.port_unreachable_rx += 1;
        icmp_stats.last_error = "port-unreach";
    } else if (typ == 3) {
        icmp_stats.destination_unreachable_rx += 1;
        icmp_stats.last_error = "dest-unreach";
    } else if (typ == 11) {
        icmp_stats.time_exceeded_rx += 1;
        icmp_stats.last_error = "time-exceeded";
    } else {
        icmp_stats.last_error = "icmp-other";
    }
}

fn applyIcmpTxResult(op: r4p_contract.IcmpOp) void {
    icmp_stats.last_type = op.typ;
    icmp_stats.last_code = op.code;
    icmp_stats.last_id = op.ident;
    icmp_stats.last_seq = op.seq;
    switch (op.result) {
        r4p_contract.ICMP_RESULT_OK => {
            icmp_stats.tx_packets += 1;
            if ((op.flags & r4p_contract.ICMP_FLAG_ECHO_REQUEST) != 0) icmp_stats.echo_requests_tx += 1;
            if ((op.flags & r4p_contract.ICMP_FLAG_ECHO_REPLY) != 0) icmp_stats.echo_replies_tx += 1;
            icmp_stats.last_error = "none";
        },
        r4p_contract.ICMP_RESULT_SHORT => icmp_stats.last_error = "short",
        r4p_contract.ICMP_RESULT_CHECKSUM => icmp_stats.last_error = "checksum",
        else => icmp_stats.last_error = "not-icmp",
    }
}

fn udpBuildDatagram(out: []u8, source_ip: [4]u8, dest_ip: [4]u8, source_port: u16, dest_port: u16, payload: []const u8) ?[]u8 {
    var op = newUdpOp() orelse {
        udp_dispatch_failures += 1;
        udp_stats.last_error = "r4p-required";
        return null;
    };
    if (payload.len > op.payload.len) {
        udp_dispatch_failures += 1;
        udp_stats.last_error = "payload-size";
        return null;
    }
    op.source_ip = source_ip;
    op.dest_ip = dest_ip;
    op.source_port = source_port;
    op.dest_port = dest_port;
    op.payload_len = @intCast(payload.len);
    if (payload.len > 0) @memcpy(op.payload[0..payload.len], payload);
    if (!udpDispatch(r4p_contract.UDP_OP_BUILD_DATAGRAM, &op)) {
        udp_stats.last_error = "r4p-dispatch";
        return null;
    }
    if (op.result != r4p_contract.UDP_RESULT_OK or op.datagram_len > out.len) {
        udp_dispatch_failures += 1;
        udp_stats.last_error = "r4p-build";
        return null;
    }
    const len: usize = @intCast(op.datagram_len);
    @memcpy(out[0..len], op.datagram[0..len]);
    udp_r4p_build += 1;
    return out[0..len];
}

fn udpHandleRx(ip_view: ipv4.PacketView) ?udp.DatagramView {
    return udpHandleRxMetadata(ip_view, false);
}

fn udpHandleRxMetadata(ip_view: ipv4.PacketView, l4_checksum_valid: bool) ?udp.DatagramView {
    if (ip_view.protocol != udp.IPV4_PROTOCOL) return null;
    var op = newUdpOp() orelse {
        udp_dispatch_failures += 1;
        udp_stats.last_error = "r4p-required";
        return null;
    };
    op.source_ip = ip_view.source_ip;
    op.dest_ip = ip_view.dest_ip;
    const buffer_flags: u32 = if (l4_checksum_valid) backend_contract.protocol_buffer_flag_rx_l4_checksum_valid else 0;
    if (!setUdpDatagram(&op, ip_view.payload) or !udpDispatchFlags(r4p_contract.UDP_OP_HANDLE_RX, &op, buffer_flags)) {
        udp_stats.last_error = "r4p-dispatch";
        return null;
    }
    applyUdpRxResult(op);
    if (op.result != r4p_contract.UDP_RESULT_OK) return null;
    udp_r4p_rx += 1;
    return udpViewFromOp(op);
}

fn udpHandleTx(ip_view: ipv4.PacketView) ?udp.DatagramView {
    if (ip_view.protocol != udp.IPV4_PROTOCOL) return null;
    var op = newUdpOp() orelse {
        udp_dispatch_failures += 1;
        udp_stats.last_error = "r4p-required";
        return null;
    };
    op.source_ip = ip_view.source_ip;
    op.dest_ip = ip_view.dest_ip;
    if (!setUdpDatagram(&op, ip_view.payload) or !udpDispatch(r4p_contract.UDP_OP_HANDLE_TX, &op)) {
        udp_stats.last_error = "r4p-dispatch";
        return null;
    }
    applyUdpTxResult(op);
    if (op.result != r4p_contract.UDP_RESULT_OK) return null;
    udp_r4p_tx += 1;
    return udpViewFromOp(op);
}

fn newUdpOp() ?r4p_contract.UdpOp {
    if (!r4p.hasActiveR4p("net.udp")) return null;
    return .{};
}

fn setUdpDatagram(op: *r4p_contract.UdpOp, datagram: []const u8) bool {
    if (datagram.len > op.datagram.len) return false;
    op.datagram_len = @intCast(datagram.len);
    if (datagram.len > 0) @memcpy(op.datagram[0..datagram.len], datagram);
    return true;
}

fn udpDispatch(opcode: u32, op: *r4p_contract.UdpOp) bool {
    return udpDispatchFlags(opcode, op, 0);
}

fn udpDispatchFlags(opcode: u32, op: *r4p_contract.UdpOp, flags: u32) bool {
    var buffer = protocol_api.ProtocolBuffer{
        .data = @ptrCast(op),
        .len = @sizeOf(r4p_contract.UdpOp),
        .capacity = @sizeOf(r4p_contract.UdpOp),
        .flags = flags,
        .reserved = 0,
    };
    const result = r4p.dispatch("net.udp", opcode, &buffer, &buffer);
    if ((result == -5 or result == -4) and op.result == 0) {
        udp_dispatch_failures += 1;
        return false;
    }
    return true;
}

fn udpViewFromOp(op: r4p_contract.UdpOp) udp.DatagramView {
    const len: usize = @min(@as(usize, @intCast(op.payload_len)), udp_payload_scratch.len);
    if (len > 0) @memcpy(udp_payload_scratch[0..len], op.payload[0..len]);
    return .{
        .source_ip = op.source_ip,
        .dest_ip = op.dest_ip,
        .source_port = op.source_port,
        .dest_port = op.dest_port,
        .length = op.length,
        .payload = udp_payload_scratch[0..len],
    };
}

fn applyUdpRxResult(op: r4p_contract.UdpOp) void {
    udp_stats.last_source_port = op.source_port;
    udp_stats.last_dest_port = op.dest_port;
    switch (op.result) {
        r4p_contract.UDP_RESULT_OK => {
            udp_stats.rx_packets += 1;
            udp_stats.last_error = "none";
            if (op.dest_port == dhcp.CLIENT_PORT or op.source_port == dhcp.SERVER_PORT) udp_stats.dhcp_rx += 1;
            if (op.dest_port == dns.PORT or op.source_port == dns.PORT) udp_stats.dns_rx += 1;
        },
        r4p_contract.UDP_RESULT_SHORT => {
            udp_stats.dropped_short += 1;
            udp_stats.last_error = "short";
        },
        r4p_contract.UDP_RESULT_LENGTH => {
            udp_stats.dropped_length += 1;
            udp_stats.last_error = "length";
        },
        r4p_contract.UDP_RESULT_CHECKSUM => {
            udp_stats.checksum_errors += 1;
            udp_stats.last_error = "checksum";
        },
        else => {
            udp_stats.malformed += 1;
            udp_stats.last_error = "not-udp";
        },
    }
}

fn applyUdpTxResult(op: r4p_contract.UdpOp) void {
    udp_stats.last_source_port = op.source_port;
    udp_stats.last_dest_port = op.dest_port;
    switch (op.result) {
        r4p_contract.UDP_RESULT_OK => {
            udp_stats.tx_packets += 1;
            udp_stats.last_error = "none";
        },
        r4p_contract.UDP_RESULT_SHORT => udp_stats.last_error = "short",
        r4p_contract.UDP_RESULT_LENGTH => udp_stats.last_error = "length",
        r4p_contract.UDP_RESULT_CHECKSUM => udp_stats.last_error = "checksum",
        else => udp_stats.last_error = "not-udp",
    }
}

pub fn udpBind(port: u16) i32 {
    if (port == 0) {
        udp_stats.last_error = "bind-port";
        return -1;
    }
    if (udpSocketIndexByPort(port) != null) {
        udp_stats.last_error = "bind-in-use";
        return -2;
    }
    var index: usize = 0;
    while (index < UDP_SOCKET_MAX) : (index += 1) {
        if (udp_sockets[index].active) continue;
        const handle = nextUdpSocketHandle();
        udp_sockets[index] = .{
            .active = true,
            .handle = handle,
            .port = port,
        };
        udp_stats.last_error = "bind-ok";
        return @intCast(handle);
    }
    udp_stats.last_error = "bind-full";
    return -3;
}

pub fn udpClose(handle: u32) i32 {
    const index = udpSocketIndexByHandle(handle) orelse {
        udp_stats.last_error = "close-handle";
        return -1;
    };
    udp_sockets[index] = .{};
    udp_stats.last_error = "close-ok";
    return 0;
}

pub fn udpSendTo(handle: u32, dest_ip: [4]u8, dest_port: u16, payload: []const u8) TxResult {
    const index = udpSocketIndexByHandle(handle) orelse {
        udp_stats.last_error = "send-handle";
        return .backend_error;
    };
    if (dest_port == 0) {
        udp_stats.last_error = "send-port";
        return .backend_error;
    }
    var datagram_buf: [MAX_PACKET_SIZE]u8 = .{0} ** MAX_PACKET_SIZE;
    const datagram = udpBuildDatagram(datagram_buf[0..], net_config.localIp(), dest_ip, udp_sockets[index].port, dest_port, payload) orelse {
        udp_stats.last_error = "send-build";
        return .too_large;
    };
    const result = sendIpv4Payload(dest_ip, udp.IPV4_PROTOCOL, datagram);
    if (result == .ok) udp_sockets[index].tx_packets += 1;
    return result;
}

fn udpSendDhcpFromAdapter(handle: u32, adapter_index: usize, source_ip: [4]u8, dest_ip: [4]u8, payload: []const u8) TxResult {
    const index = udpSocketIndexByHandle(handle) orelse {
        udp_stats.last_error = "send-handle";
        return .backend_error;
    };
    if (adapter_index >= adapter_count) {
        udp_stats.last_error = "send-adapter";
        return .no_adapter;
    }
    var datagram_buf: [MAX_PACKET_SIZE]u8 = .{0} ** MAX_PACKET_SIZE;
    const datagram = udpBuildDatagram(datagram_buf[0..], source_ip, dest_ip, udp_sockets[index].port, dhcp.SERVER_PORT, payload) orelse {
        udp_stats.last_error = "send-build";
        return .too_large;
    };
    var frame: [MAX_PACKET_SIZE]u8 = .{0} ** MAX_PACKET_SIZE;
    const ip_frame = ipv4BuildPacketFrom(frame[0..], adapters[adapter_index].mac, DHCP_BROADCAST_MAC, source_ip, dest_ip, udp.IPV4_PROTOCOL, datagram) orelse {
        udp_stats.last_error = "send-build";
        return .too_large;
    };
    const result = transmit(adapter_index, ip_frame);
    if (result == .ok) {
        udp_sockets[index].tx_packets += 1;
        pollAdapters(DHCP_POLL_ROUNDS);
    }
    return result;
}

pub fn udpRecvFrom(handle: u32, out_info: *UdpRecvInfo, out_payload: []u8) i32 {
    const index = udpSocketIndexByHandle(handle) orelse {
        udp_stats.last_error = "recv-handle";
        return -1;
    };
    if (udp_sockets[index].count == 0) {
        udp_stats.last_error = timing.operationStatusName(.would_block);
        return 0;
    }
    const queued = udp_sockets[index].queue[udp_sockets[index].head];
    const packet_len: usize = queued.len;
    if (out_payload.len < packet_len) {
        udp_stats.last_error = "recv-small";
        buffer_small_events += 1;
        return -2;
    }
    out_info.* = .{
        .source_ip = queued.source_ip,
        .dest_ip = queued.dest_ip,
        .source_port = queued.source_port,
        .dest_port = queued.dest_port,
        .length = queued.len,
    };
    if (packet_len > 0) @memcpy(out_payload[0..packet_len], queued.payload[0..packet_len]);
    udp_sockets[index].head = (udp_sockets[index].head + 1) % UDP_SOCKET_QUEUE_SIZE;
    udp_sockets[index].count -= 1;
    udp_stats.last_error = "recv-ok";
    return @intCast(packet_len);
}

pub fn udpRecvFromWait(handle: u32, out_info: *UdpRecvInfo, out_payload: []u8, timeout_ticks: u64) i32 {
    const deadline = timing.Deadline.start(timeout_ticks, DHCP_MAX_LOOPS);
    var loops: usize = 0;
    while (!deadline.loopLimitReached(loops)) : (loops += 1) {
        const got = udpRecvFrom(handle, out_info, out_payload);
        if (got != 0) return got;
        if (timeout_ticks == 0) {
            udp_stats.last_error = timing.operationStatusName(.would_block);
            return 0;
        }
        pollAdapters(DHCP_POLL_ROUNDS);
        if (deadline.expiredNow()) {
            udp_stats.last_error = "recv-timeout";
            return 0;
        }
        scheduler.yield();
        interrupts.enable();
        interrupts.waitForInterrupt();
    }
    udp_stats.last_error = "recv-timeout";
    return 0;
}

pub fn udpStatus(out: *UdpStatus) void {
    out.* = .{
        .active_sockets = udpSocketActiveCount(),
        .max_sockets = UDP_SOCKET_MAX,
        .queued_packets = udpSocketQueuedCount(),
        .queue_limit = UDP_SOCKET_QUEUE_SIZE,
        .payload_max = UDP_SOCKET_PAYLOAD_MAX,
        .delivered = udp_socket_delivered,
        .drops = udp_socket_drops,
        .last_error = udp_stats.last_error,
    };
}

pub fn udpSocketActiveCount() u32 {
    var active: u32 = 0;
    for (udp_sockets) |socket| {
        if (socket.active) active += 1;
    }
    return active;
}

pub fn udpSocketQueuedCount() u32 {
    var queued: u32 = 0;
    for (udp_sockets) |socket| {
        if (socket.active) queued += @intCast(socket.count);
    }
    return queued;
}

pub fn udpSocketDropCount() u64 {
    return udp_socket_drops;
}

pub fn udpSocketCapacity() u32 {
    return UDP_SOCKET_MAX;
}

fn dispatchUdpSocketDatagram(view: udp.DatagramView) void {
    const index = udpSocketIndexByPort(view.dest_port) orelse return;
    if (view.payload.len > UDP_SOCKET_PAYLOAD_MAX) {
        udp_sockets[index].drops += 1;
        udp_socket_drops += 1;
        udp_stats.last_error = "socket-payload";
        return;
    }
    if (udp_sockets[index].count >= UDP_SOCKET_QUEUE_SIZE) {
        udp_sockets[index].drops += 1;
        udp_socket_drops += 1;
        udp_stats.last_error = "socket-full";
        return;
    }
    const tail = udp_sockets[index].tail;
    udp_sockets[index].queue[tail] = .{
        .source_ip = view.source_ip,
        .dest_ip = view.dest_ip,
        .source_port = view.source_port,
        .dest_port = view.dest_port,
        .len = @intCast(view.payload.len),
    };
    if (view.payload.len > 0) {
        @memcpy(udp_sockets[index].queue[tail].payload[0..view.payload.len], view.payload);
    }
    udp_sockets[index].tail = (udp_sockets[index].tail + 1) % UDP_SOCKET_QUEUE_SIZE;
    udp_sockets[index].count += 1;
    udp_sockets[index].rx_packets += 1;
    udp_socket_delivered += 1;
}

fn udpSocketIndexByHandle(handle: u32) ?usize {
    if (handle == 0) return null;
    var index: usize = 0;
    while (index < UDP_SOCKET_MAX) : (index += 1) {
        if (udp_sockets[index].active and udp_sockets[index].handle == handle) return index;
    }
    return null;
}

fn udpSocketIndexByPort(port: u16) ?usize {
    var index: usize = 0;
    while (index < UDP_SOCKET_MAX) : (index += 1) {
        if (udp_sockets[index].active and udp_sockets[index].port == port) return index;
    }
    return null;
}

fn udpSocketPortBound(port: u16) bool {
    return udpSocketIndexByPort(port) != null;
}

fn nextUdpSocketHandle() u32 {
    const handle = udp_next_handle;
    udp_next_handle +%= 1;
    if (udp_next_handle == 0) udp_next_handle = 1;
    return handle;
}

fn dhcpBuildDiscover(out: []u8, xid: u32, mac: [6]u8) ?[]u8 {
    var op = newDhcpOp() orelse {
        dhcp_dispatch_failures += 1;
        dhcp_stats.last_error = "r4p-required";
        return null;
    };
    op.xid = xid;
    op.mac = mac;
    if (!dhcpDispatch(r4p_contract.DHCP_OP_BUILD_DISCOVER, &op)) {
        dhcp_stats.last_error = "r4p-dispatch";
        return null;
    }
    if (op.result != r4p_contract.DHCP_RESULT_OK or op.payload_len > out.len) {
        dhcp_dispatch_failures += 1;
        dhcp_stats.last_error = "r4p-build";
        return null;
    }
    const len: usize = @intCast(op.payload_len);
    @memcpy(out[0..len], op.payload[0..len]);
    applyDhcpBuildResult(op);
    dhcp_r4p_build += 1;
    return out[0..len];
}

fn dhcpBuildRequest(out: []u8, xid: u32, mac: [6]u8, requested_ip: [4]u8, server_ip: [4]u8) ?[]u8 {
    var op = newDhcpOp() orelse {
        dhcp_dispatch_failures += 1;
        dhcp_stats.last_error = "r4p-required";
        return null;
    };
    op.xid = xid;
    op.mac = mac;
    op.requested_ip = requested_ip;
    op.server_ip = server_ip;
    if (!dhcpDispatch(r4p_contract.DHCP_OP_BUILD_REQUEST, &op)) {
        dhcp_stats.last_error = "r4p-dispatch";
        return null;
    }
    if (op.result != r4p_contract.DHCP_RESULT_OK or op.payload_len > out.len) {
        dhcp_dispatch_failures += 1;
        dhcp_stats.last_error = "r4p-build";
        return null;
    }
    const len: usize = @intCast(op.payload_len);
    @memcpy(out[0..len], op.payload[0..len]);
    applyDhcpBuildResult(op);
    dhcp_r4p_build += 1;
    return out[0..len];
}

fn dhcpBuildRelease(out: []u8, xid: u32, mac: [6]u8, client_ip: [4]u8, server_ip: [4]u8) ?[]u8 {
    var op = newDhcpOp() orelse {
        dhcp_dispatch_failures += 1;
        dhcp_stats.last_error = "r4p-required";
        return null;
    };
    op.xid = xid;
    op.mac = mac;
    op.client_ip = client_ip;
    op.server_ip = server_ip;
    if (!dhcpDispatch(r4p_contract.DHCP_OP_BUILD_RELEASE, &op)) {
        dhcp_stats.last_error = "r4p-dispatch";
        return null;
    }
    if (op.result != r4p_contract.DHCP_RESULT_OK or op.payload_len > out.len) {
        dhcp_dispatch_failures += 1;
        dhcp_stats.last_error = "r4p-build";
        return null;
    }
    const len: usize = @intCast(op.payload_len);
    @memcpy(out[0..len], op.payload[0..len]);
    applyDhcpBuildResult(op);
    dhcp_r4p_build += 1;
    return out[0..len];
}

fn dhcpHandleMessage(payload: []const u8) bool {
    // Port 68 receives broadcasts for every DHCP client on the segment.  A
    // reply is allowed to affect state only while our own serialized DHCP
    // operation is waiting for it.
    if (!dhcp_stats.operation_pending or dhcp_expected_response == .none) {
        dhcp_stats.inactive_rx +%= 1;
        return false;
    }
    var op = newDhcpOp() orelse {
        dhcp_dispatch_failures += 1;
        dhcp_stats.last_error = "r4p-required";
        return false;
    };
    if (!setDhcpPayload(&op, payload) or !dhcpDispatch(r4p_contract.DHCP_OP_HANDLE_MESSAGE, &op)) {
        dhcp_stats.last_error = "r4p-dispatch";
        return false;
    }
    if (op.result != r4p_contract.DHCP_RESULT_OK) {
        applyDhcpRxResult(op);
        return false;
    }
    if (op.xid != dhcp_expected_xid or !macEqual(op.mac, dhcp_expected_mac)) {
        dhcp_stats.foreign_rx +%= 1;
        return false;
    }
    if (!dhcpResponseIsExpected(op.flags)) {
        dhcp_stats.out_of_phase_rx +%= 1;
        return false;
    }
    if ((op.flags & r4p_contract.DHCP_FLAG_OFFER) != 0 and dhcp_test_drop_offers != 0) {
        dhcp_test_drop_offers -= 1;
        dhcp_stats.last_error = "test-drop-offer";
        k.puts("DHCP05913 inject=drop-offer remaining=");
        k.putDec(dhcp_test_drop_offers);
        k.puts("\r\n");
        return false;
    }
    if ((op.flags & r4p_contract.DHCP_FLAG_ACK) != 0 and dhcp_test_drop_acks != 0) {
        dhcp_test_drop_acks -= 1;
        dhcp_stats.last_error = "test-drop-ack";
        k.puts("DHCP05913 inject=drop-ack remaining=");
        k.putDec(dhcp_test_drop_acks);
        k.puts("\r\n");
        return false;
    }
    applyDhcpRxResult(op);
    if (op.result == r4p_contract.DHCP_RESULT_OK) {
        dhcp_r4p_rx += 1;
        return true;
    }
    return false;
}

fn setDhcpResponseExpectation(expected: DhcpExpectedResponse, xid: u32, mac: [6]u8) void {
    dhcp_expected_response = expected;
    dhcp_expected_xid = xid;
    dhcp_expected_mac = mac;
}

fn clearDhcpResponseExpectation() void {
    dhcp_expected_response = .none;
    dhcp_expected_xid = 0;
    dhcp_expected_mac = .{0} ** 6;
}

fn dhcpResponseIsExpected(flags: u32) bool {
    return switch (dhcp_expected_response) {
        .none => false,
        .offer => (flags & r4p_contract.DHCP_FLAG_OFFER) != 0,
        .ack_or_nak => (flags & (r4p_contract.DHCP_FLAG_ACK | r4p_contract.DHCP_FLAG_NAK)) != 0,
    };
}

fn macEqual(left: [6]u8, right: [6]u8) bool {
    var index: usize = 0;
    while (index < left.len) : (index += 1) {
        if (left[index] != right[index]) return false;
    }
    return true;
}

fn newDhcpOp() ?r4p_contract.DhcpOp {
    if (!r4p.hasActiveR4p("net.dhcp")) return null;
    return .{};
}

fn setDhcpPayload(op: *r4p_contract.DhcpOp, payload: []const u8) bool {
    if (payload.len > op.payload.len) return false;
    op.payload_len = @intCast(payload.len);
    if (payload.len > 0) @memcpy(op.payload[0..payload.len], payload);
    return true;
}

fn dhcpDispatch(opcode: u32, op: *r4p_contract.DhcpOp) bool {
    var buffer = protocol_api.ProtocolBuffer{
        .data = @ptrCast(op),
        .len = @sizeOf(r4p_contract.DhcpOp),
        .capacity = @sizeOf(r4p_contract.DhcpOp),
        .flags = 0,
        .reserved = 0,
    };
    const result = r4p.dispatch("net.dhcp", opcode, &buffer, &buffer);
    if ((result == -5 or result == -4) and op.result == 0) {
        dhcp_dispatch_failures += 1;
        return false;
    }
    return true;
}

fn applyDhcpBuildResult(op: r4p_contract.DhcpOp) void {
    dhcp_stats.last_type = op.message_type;
    dhcp_stats.last_error = "none";
    if ((op.flags & r4p_contract.DHCP_FLAG_DISCOVER) != 0) dhcp_stats.discover_tx += 1;
    if ((op.flags & r4p_contract.DHCP_FLAG_REQUEST) != 0) dhcp_stats.request_tx += 1;
    if ((op.flags & r4p_contract.DHCP_FLAG_RELEASE) != 0) {
        dhcp_stats.release_tx += 1;
        dhcp_stats.last_error = "release";
    }
}

fn applyDhcpRxResult(op: r4p_contract.DhcpOp) void {
    dhcp_stats.last_type = op.message_type;
    switch (op.result) {
        r4p_contract.DHCP_RESULT_OK => {
            if ((op.flags & r4p_contract.DHCP_FLAG_OFFER) != 0) {
                dhcp_stats.offer_rx += 1;
                dhcp_stats.lease.xid = op.xid;
                dhcp_stats.lease.offered_ip = op.offered_ip;
                dhcp_stats.lease.server_ip = op.server_ip;
                dhcp_stats.lease.netmask = op.netmask;
                dhcp_stats.lease.gateway_ip = op.gateway_ip;
                dhcp_stats.lease.dns_ip = op.dns_ip;
                dhcp_stats.lease.dns_configured = op.dns_configured != 0;
                dhcp_stats.lease.lease_seconds = op.lease_seconds;
                dhcp_stats.lease.renew_seconds = op.renew_seconds;
                dhcp_stats.lease.rebind_seconds = op.rebind_seconds;
                dhcp_stats.last_error = "offer";
            } else if ((op.flags & r4p_contract.DHCP_FLAG_ACK) != 0) {
                dhcp_stats.ack_rx += 1;
                dhcp_stats.lease.bound = (op.flags & r4p_contract.DHCP_FLAG_BOUND) != 0;
                dhcp_stats.lease.xid = op.xid;
                dhcp_stats.lease.offered_ip = op.offered_ip;
                dhcp_stats.lease.server_ip = op.server_ip;
                dhcp_stats.lease.netmask = op.netmask;
                dhcp_stats.lease.gateway_ip = op.gateway_ip;
                dhcp_stats.lease.dns_ip = op.dns_ip;
                dhcp_stats.lease.dns_configured = op.dns_configured != 0;
                dhcp_stats.lease.lease_seconds = op.lease_seconds;
                dhcp_stats.lease.renew_seconds = op.renew_seconds;
                dhcp_stats.lease.rebind_seconds = op.rebind_seconds;
                if (dhcp_stats.lease.bound) markDhcpLeaseBound();
                dhcp_stats.last_error = "ack";
            } else if ((op.flags & r4p_contract.DHCP_FLAG_NAK) != 0) {
                dhcp_stats.nak_rx += 1;
                dhcp_stats.lease.bound = false;
                dhcp_stats.lease_acquired_tick = 0;
                dhcp_stats.last_error = "nak";
            } else {
                dhcp_stats.last_error = "ignored";
            }
        },
        r4p_contract.DHCP_RESULT_IGNORED => dhcp_stats.last_error = "ignored",
        r4p_contract.DHCP_RESULT_NO_TYPE => {
            dhcp_stats.malformed += 1;
            dhcp_stats.last_error = "no-type";
        },
        else => {
            dhcp_stats.malformed += 1;
            dhcp_stats.last_error = "shape";
        },
    }
}

fn dnsBuildAQuery(out: []u8, id: u16, name: []const u8) ?[]u8 {
    var op = newDnsOp() orelse {
        dns_dispatch_failures += 1;
        dns_stats.last_error = "r4p-required";
        return null;
    };
    op.id = id;
    if (!setDnsName(&op, name) or !dnsDispatch(r4p_contract.DNS_OP_BUILD_A_QUERY, &op)) {
        dns_stats.last_error = "r4p-dispatch";
        return null;
    }
    if (op.result != r4p_contract.DNS_RESULT_OK or op.payload_len > out.len) {
        dns_dispatch_failures += 1;
        dns_stats.last_error = "r4p-build";
        return null;
    }
    const len: usize = @intCast(op.payload_len);
    @memcpy(out[0..len], op.payload[0..len]);
    applyDnsBuildResult(op);
    dns_r4p_build += 1;
    return out[0..len];
}

fn dnsHandleResponse(payload: []const u8) bool {
    var op = newDnsOp() orelse {
        dns_dispatch_failures += 1;
        dns_stats.last_error = "r4p-required";
        return false;
    };
    if (!setDnsPayload(&op, payload) or !dnsDispatch(r4p_contract.DNS_OP_HANDLE_RESPONSE, &op)) {
        dns_stats.last_error = "r4p-dispatch";
        return false;
    }
    applyDnsRxResult(op);
    if (op.result == r4p_contract.DNS_RESULT_OK) {
        dns_r4p_rx += 1;
        return true;
    }
    return false;
}

fn newDnsOp() ?r4p_contract.DnsOp {
    if (!r4p.hasActiveR4p("net.dns")) return null;
    return .{};
}

fn setDnsName(op: *r4p_contract.DnsOp, name: []const u8) bool {
    if (name.len == 0 or name.len > op.name.len) return false;
    op.name_len = @intCast(name.len);
    @memcpy(op.name[0..name.len], name);
    return true;
}

fn setDnsPayload(op: *r4p_contract.DnsOp, payload: []const u8) bool {
    if (payload.len > op.payload.len) return false;
    op.payload_len = @intCast(payload.len);
    if (payload.len > 0) @memcpy(op.payload[0..payload.len], payload);
    return true;
}

fn dnsDispatch(opcode: u32, op: *r4p_contract.DnsOp) bool {
    var buffer = protocol_api.ProtocolBuffer{
        .data = @ptrCast(op),
        .len = @sizeOf(r4p_contract.DnsOp),
        .capacity = @sizeOf(r4p_contract.DnsOp),
        .flags = 0,
        .reserved = 0,
    };
    const result = r4p.dispatch("net.dns", opcode, &buffer, &buffer);
    if ((result == -5 or result == -4) and op.result == 0) {
        dns_dispatch_failures += 1;
        return false;
    }
    return true;
}

fn applyDnsBuildResult(op: r4p_contract.DnsOp) void {
    dns_stats.queries_tx += 1;
    dns_stats.last_id = op.id;
    dns_stats.last_result = 1;
    dns_stats.last_error = "query";
}

fn applyDnsRxResult(op: r4p_contract.DnsOp) void {
    dns_stats.last_id = op.id;
    dns_stats.last_result = op.result;
    switch (op.result) {
        r4p_contract.DNS_RESULT_OK => {
            dns_stats.responses_rx += 1;
            if ((op.flags & r4p_contract.DNS_FLAG_A_RECORD) != 0) dns_stats.a_records += 1;
            dns_stats.last_answer = op.answer;
            dns_stats.last_error = "answer";
        },
        r4p_contract.DNS_RESULT_SHORT => {
            dns_stats.malformed += 1;
            dns_stats.last_error = "short";
        },
        r4p_contract.DNS_RESULT_HEADER => {
            dns_stats.malformed += 1;
            dns_stats.last_error = "header";
        },
        r4p_contract.DNS_RESULT_QNAME => {
            dns_stats.malformed += 1;
            dns_stats.last_error = "qname";
        },
        r4p_contract.DNS_RESULT_QUESTION => {
            dns_stats.malformed += 1;
            dns_stats.last_error = "question";
        },
        r4p_contract.DNS_RESULT_ANAME => {
            dns_stats.malformed += 1;
            dns_stats.last_error = "aname";
        },
        r4p_contract.DNS_RESULT_ANSWER => {
            dns_stats.malformed += 1;
            dns_stats.last_error = "answer";
        },
        r4p_contract.DNS_RESULT_ATYPE => {
            dns_stats.malformed += 1;
            dns_stats.last_error = "atype";
        },
        r4p_contract.DNS_RESULT_NXDOMAIN => {
            dns_stats.nxdomain += 1;
            dns_stats.last_error = "nxdomain";
        },
        r4p_contract.DNS_RESULT_TIMEOUT => {
            dns_stats.timeouts += 1;
            dns_stats.last_error = "timeout";
        },
        else => {
            dns_stats.malformed += 1;
            dns_stats.last_error = "name";
        },
    }
}

fn tcpHandleRx(ip_view: ipv4.PacketView) void {
    tcpHandleRxMetadata(ip_view, false);
}

fn tcpHandleRxMetadata(ip_view: ipv4.PacketView, l4_checksum_valid: bool) void {
    if (ip_view.protocol != tcp.IPV4_PROTOCOL) return;
    var op = newTcpOp() orelse {
        tcp_dispatch_failures += 1;
        tcp_last_error = "r4p-required";
        return;
    };
    op.source_ip = ip_view.source_ip;
    op.dest_ip = ip_view.dest_ip;
    const buffer_flags: u32 = if (l4_checksum_valid) backend_contract.protocol_buffer_flag_rx_l4_checksum_valid else 0;
    if (!setTcpSegment(&op, ip_view.payload) or !tcpDispatchFlags(r4p_contract.TCP_OP_HANDLE_RX, &op, buffer_flags)) {
        tcp_last_error = "r4p-dispatch";
        return;
    }
    applyTcpResult(op);
    recordTcpRxReject(op.result);
    if (op.result != r4p_contract.TCP_RESULT_OK) return;
    tcp_r4p_rx += 1;
    const view = tcpViewFromOp(op, ip_view) orelse {
        tcp_dispatch_failures += 1;
        tcp_last_error = "r4p-view";
        return;
    };
    // Vor applyRxView sichern: RST/FIN duerfen den Slot im selben Aufruf
    // schliessen, der RTO-Zustand gehoert trotzdem genau dieser Generation.
    const rx_identity = tcp.connectionIdentityForSegment(view);
    if (tcp.applyRxView(view)) |parsed| {
        if ((parsed.flags & tcp.FLAG_SYN) != 0 and (parsed.flags & tcp.FLAG_ACK) == 0) {
            if (tcp.acceptInbound(parsed, nextTcpSeq(parsed.source_ip, parsed.source_port))) |conn_id| {
                _ = sendTcpForConnection(conn_id, tcp.FLAG_SYN | tcp.FLAG_ACK, "");
            }
        } else if (parsed.payload.len != 0 and (parsed.flags & tcp.FLAG_RST) == 0) {
            if (tcp.connectionIdForSegment(parsed)) |conn_id| {
                _ = sendTcpForConnection(conn_id, tcp.FLAG_ACK, "");
            }
        }
        // ACK-Fortschritt wird gegen die vor dem Zustandswechsel gebundene
        // Connection ausgewertet. Geschlossene Generationen werden geloest.
        if ((parsed.flags & tcp.FLAG_ACK) != 0) {
            if (rx_identity) |identity| rtoOnAck(identity);
        }
        if (rx_identity) |identity| {
            if (!tcp.identityActive(identity)) tcp_rto.release(identity);
        }
        // 0.56.22: Wartende Tasks wecken - jedes verarbeitete Segment
        // kann established/readable/closed-Zustaende geaendert haben.
        signalTcpActivity();
    }
}

fn tcpHandleTx(ip_view: ipv4.PacketView) void {
    if (ip_view.protocol != tcp.IPV4_PROTOCOL) return;
    var op = newTcpOp() orelse {
        tcp_dispatch_failures += 1;
        tcp_last_error = "r4p-required";
        return;
    };
    op.source_ip = ip_view.source_ip;
    op.dest_ip = ip_view.dest_ip;
    if (!setTcpSegment(&op, ip_view.payload) or !tcpDispatch(r4p_contract.TCP_OP_HANDLE_TX, &op)) {
        tcp_last_error = "r4p-dispatch";
        return;
    }
    applyTcpResult(op);
    if (op.result == r4p_contract.TCP_RESULT_OK) {
        const view = tcpViewFromOp(op, ip_view) orelse {
            tcp_dispatch_failures += 1;
            tcp_last_error = "r4p-view";
            return;
        };
        _ = tcp.applyTxView(view);
        tcp_r4p_tx += 1;
    }
}

fn tcpViewFromOp(op: r4p_contract.TcpOp, ip_view: ipv4.PacketView) ?tcp.SegmentView {
    const payload_len: usize = @intCast(op.payload_len);
    if (payload_len > op.payload.len) return null;
    return .{
        .source_ip = ip_view.source_ip,
        .dest_ip = ip_view.dest_ip,
        .source_port = op.source_port,
        .dest_port = op.dest_port,
        .seq = op.seq,
        .ack = op.ack,
        .flags = op.flags,
        .window = op.reserved,
        .payload = op.payload[0..payload_len],
    };
}

fn tcpBuildSegment(out: []u8, source_ip: [4]u8, dest_ip: [4]u8, source_port: u16, dest_port: u16, seq: u32, ack: u32, flags: u16, payload: []const u8, window: u16) ?[]u8 {
    var op = newTcpOp() orelse {
        tcp_dispatch_failures += 1;
        tcp_last_error = "r4p-required";
        return null;
    };
    op.source_ip = source_ip;
    op.dest_ip = dest_ip;
    op.source_port = source_port;
    op.dest_port = dest_port;
    op.seq = seq;
    op.ack = ack;
    op.flags = flags;
    op.reserved = window;
    if (!setTcpPayload(&op, payload) or !tcpDispatch(r4p_contract.TCP_OP_BUILD_SEGMENT, &op)) {
        tcp_last_error = "r4p-dispatch";
        return null;
    }
    if (op.result != r4p_contract.TCP_RESULT_OK or op.segment_len > out.len) {
        tcp_dispatch_failures += 1;
        tcp_last_error = "r4p-build";
        return null;
    }
    const len: usize = @intCast(op.segment_len);
    @memcpy(out[0..len], op.segment[0..len]);
    tcp_r4p_build += 1;
    tcp_last_flags = op.flags;
    tcp_last_error = "none";
    return out[0..len];
}

fn newTcpOp() ?r4p_contract.TcpOp {
    if (!r4p.hasActiveR4p("net.tcp")) return null;
    return .{};
}

fn setTcpPayload(op: *r4p_contract.TcpOp, payload: []const u8) bool {
    if (payload.len > op.payload.len) return false;
    op.payload_len = @intCast(payload.len);
    if (payload.len > 0) @memcpy(op.payload[0..payload.len], payload);
    return true;
}

fn setTcpSegment(op: *r4p_contract.TcpOp, segment: []const u8) bool {
    if (segment.len > op.segment.len) return false;
    op.segment_len = @intCast(segment.len);
    if (segment.len > 0) @memcpy(op.segment[0..segment.len], segment);
    return true;
}

fn tcpDispatch(opcode: u32, op: *r4p_contract.TcpOp) bool {
    return tcpDispatchFlags(opcode, op, 0);
}

fn tcpDispatchFlags(opcode: u32, op: *r4p_contract.TcpOp, flags: u32) bool {
    var buffer = protocol_api.ProtocolBuffer{
        .data = @ptrCast(op),
        .len = @sizeOf(r4p_contract.TcpOp),
        .capacity = @sizeOf(r4p_contract.TcpOp),
        .flags = flags,
        .reserved = 0,
    };
    const result = r4p.dispatch("net.tcp", opcode, &buffer, &buffer);
    if ((result == -5 or result == -4) and op.result == 0) {
        tcp_dispatch_failures += 1;
        return false;
    }
    return true;
}

fn applyTcpResult(op: r4p_contract.TcpOp) void {
    tcp_last_flags = op.flags;
    tcp_last_error = switch (op.result) {
        r4p_contract.TCP_RESULT_OK => "none",
        r4p_contract.TCP_RESULT_NOT_TCP => "not-tcp",
        r4p_contract.TCP_RESULT_NO_CONNECTION => "no-conn",
        r4p_contract.TCP_RESULT_BAD_STATE => "state",
        r4p_contract.TCP_RESULT_BUFFER_SMALL => "buffer",
        r4p_contract.TCP_RESULT_SHORT => "short",
        r4p_contract.TCP_RESULT_CHECKSUM => "checksum",
        else => "error",
    };
}

fn recordTcpRxReject(result: i32) void {
    switch (result) {
        r4p_contract.TCP_RESULT_SHORT => tcp.recordMalformed("short"),
        r4p_contract.TCP_RESULT_CHECKSUM => tcp.recordChecksumError(),
        else => {},
    }
}

fn mapTcpSummary(in_value: r4p_contract.TcpR4pSummary) tcp.Summary {
    return .{
        .max_connections = in_value.max_connections,
        .active_connections = in_value.active_connections,
        .buffer_size = in_value.buffer_size,
        .syn_tx = in_value.syn_tx,
        .synack_rx = in_value.synack_rx,
        .ack_tx = in_value.ack_tx,
        .data_tx = in_value.data_tx,
        .data_rx = in_value.data_rx,
        .fin_tx = in_value.fin_tx,
        .rst_rx = in_value.rst_rx,
        .checksum_errors = in_value.checksum_errors,
        .timeouts = in_value.timeouts,
        .self_tests = in_value.self_tests,
        .retransmits = 0,
        .rx_drops = 0,
    };
}

fn mapTcpInfo(in_value: r4p_contract.TcpR4pConnectionInfo) tcp.ConnectionInfo {
    return .{
        .id = in_value.id,
        .state = in_value.state,
        .local_port = in_value.local_port,
        .remote_port = in_value.remote_port,
        .remote_ip = in_value.remote_ip,
        .tx_bytes = in_value.tx_bytes,
        .rx_bytes = in_value.rx_bytes,
        .pending_rx = in_value.pending_rx,
        .rx_window = 0,
        .rx_drops = 0,
        .seq = 0,
        .ack = 0,
        .last_seq = 0,
        .last_ack = 0,
        .last_flags = 0,
        .last_payload_len = 0,
    };
}

fn setAdapterLifecycle(index: usize, lifecycle: Lifecycle) void {
    if (index >= adapter_count or adapters[index].lifecycle == lifecycle) return;
    adapters[index].lifecycle = lifecycle;
    adapters[index].state_changed_tick = time_core.monotonicTicks();
}

fn setAdapterLink(index: usize, link: Link) void {
    if (index >= adapter_count) return;
    adapters[index].link = link;
    setAdapterLifecycle(index, lifecycleFromLink(link));
}

fn lifecycleFromLink(link: Link) Lifecycle {
    return switch (link) {
        .up => .active,
        .down => .link_down,
        .unknown => .registered,
    };
}

fn isNetwork(class_code: u8, subclass: u8) bool {
    return class_code == CLASS_NETWORK and subclass == SUBCLASS_ETHERNET;
}

pub fn packetDropCount() u64 {
    return packet_drops;
}

pub fn packetFreeCount() usize {
    var free: usize = 0;
    var index: usize = 0;
    while (index < PACKET_POOL_SIZE) : (index += 1) {
        if (!packet_used[index]) free += 1;
    }
    return free;
}

pub fn readAppIpv4(protocol: u8, out: *AppIpv4Packet, payload_out: []u8) i32 {
    var best_bucket: ?usize = null;
    var best_offset: usize = 0;
    var best_seq: u64 = 0xFFFF_FFFF_FFFF_FFFF;
    var bucket: usize = 0;
    while (bucket < APP_IPV4_BUCKET_COUNT) : (bucket += 1) {
        var offset: usize = 0;
        while (offset < app_ipv4_queues[bucket].count) : (offset += 1) {
            const index = appIpv4Index(bucket, offset);
            const entry = app_ipv4_queues[bucket].entries[index];
            if (!appIpv4ProtocolMatches(protocol, entry.info.protocol)) continue;
            if (entry.seq < best_seq) {
                best_bucket = bucket;
                best_offset = offset;
                best_seq = entry.seq;
            }
        }
    }
    if (best_bucket) |selected_bucket| {
        const index = appIpv4Index(selected_bucket, best_offset);
        var info = app_ipv4_queues[selected_bucket].entries[index].info;
        const available_len = minUsize(@as(usize, @intCast(info.payload_len)), APP_IPV4_PAYLOAD_MAX);
        const copy_len = minUsize(available_len, payload_out.len);
        if (copy_len > 0) @memcpy(payload_out[0..copy_len], app_ipv4_queues[selected_bucket].entries[index].payload[0..copy_len]);
        if (copy_len < available_len) info.truncated = true;
        out.* = info;
        removeAppIpv4At(selected_bucket, best_offset);
        return @intCast(copy_len);
    }
    out.* = .{};
    return 0;
}

fn enqueueAppIpv4(view: ipv4.PacketView) void {
    const bucket = appIpv4BucketFor(view);
    if (app_ipv4_queues[bucket].count == APP_IPV4_QUEUE_SIZE) {
        app_ipv4_queues[bucket].entries[app_ipv4_queues[bucket].head] = .{};
        app_ipv4_queues[bucket].head = (app_ipv4_queues[bucket].head + 1) % APP_IPV4_QUEUE_SIZE;
        app_ipv4_queues[bucket].count -= 1;
        app_ipv4_queues[bucket].drops += 1;
    }

    const index = app_ipv4_queues[bucket].tail;
    app_ipv4_queues[bucket].entries[index] = .{};
    const copy_len = minUsize(view.payload.len, APP_IPV4_PAYLOAD_MAX);
    if (copy_len > 0) @memcpy(app_ipv4_queues[bucket].entries[index].payload[0..copy_len], view.payload[0..copy_len]);
    const ports = appIpv4Ports(view);
    app_ipv4_queues[bucket].entries[index].info = .{
        .source_ip = view.source_ip,
        .dest_ip = view.dest_ip,
        .protocol = view.protocol,
        .source_port = ports.source,
        .dest_port = ports.dest,
        .truncated = view.payload.len > APP_IPV4_PAYLOAD_MAX,
        .payload_len = @intCast(view.payload.len),
    };
    app_ipv4_queues[bucket].entries[index].seq = app_ipv4_next_seq;
    app_ipv4_next_seq +%= 1;
    if (app_ipv4_next_seq == 0) app_ipv4_next_seq = 1;
    app_ipv4_queues[bucket].tail = (app_ipv4_queues[bucket].tail + 1) % APP_IPV4_QUEUE_SIZE;
    app_ipv4_queues[bucket].count += 1;
}

fn removeAppIpv4At(bucket: usize, offset: usize) void {
    if (bucket >= APP_IPV4_BUCKET_COUNT or offset >= app_ipv4_queues[bucket].count) return;
    var i = offset;
    while (i + 1 < app_ipv4_queues[bucket].count) : (i += 1) {
        const dst = appIpv4Index(bucket, i);
        const src = appIpv4Index(bucket, i + 1);
        app_ipv4_queues[bucket].entries[dst] = app_ipv4_queues[bucket].entries[src];
    }
    app_ipv4_queues[bucket].count -= 1;
    app_ipv4_queues[bucket].tail = (app_ipv4_queues[bucket].head + app_ipv4_queues[bucket].count) % APP_IPV4_QUEUE_SIZE;
    app_ipv4_queues[bucket].entries[app_ipv4_queues[bucket].tail] = .{};
}

fn appIpv4Index(bucket: usize, offset: usize) usize {
    return (app_ipv4_queues[bucket].head + offset) % APP_IPV4_QUEUE_SIZE;
}

fn appIpv4ProtocolMatches(requested: u8, actual: u8) bool {
    return requested == 0 or requested == actual;
}

fn appIpv4BucketFor(view: ipv4.PacketView) usize {
    if (view.protocol == icmp.IPV4_PROTOCOL) return APP_IPV4_BUCKET_ICMP;
    if (view.protocol == udp.IPV4_PROTOCOL) {
        const ports = appIpv4Ports(view);
        if (ports.source == dhcp.SERVER_PORT or ports.dest == dhcp.CLIENT_PORT) return APP_IPV4_BUCKET_UDP_DHCP;
        if (ports.source == dns.PORT or ports.dest == dns.PORT) return APP_IPV4_BUCKET_UDP_DNS;
        return APP_IPV4_BUCKET_UDP_OTHER;
    }
    if (view.protocol == tcp.IPV4_PROTOCOL) return APP_IPV4_BUCKET_TCP;
    return APP_IPV4_BUCKET_OTHER;
}

fn appIpv4Ports(view: ipv4.PacketView) struct { source: u16, dest: u16 } {
    if ((view.protocol == udp.IPV4_PROTOCOL or view.protocol == tcp.IPV4_PROTOCOL) and view.payload.len >= 4) {
        return .{
            .source = readBe16(view.payload, 0),
            .dest = readBe16(view.payload, 2),
        };
    }
    return .{ .source = 0, .dest = 0 };
}

fn appIpv4QueuedCount() usize {
    var total: usize = 0;
    for (app_ipv4_queues) |queue| total += queue.count;
    return total;
}

fn appIpv4DropCount() u64 {
    var total: u64 = 0;
    for (app_ipv4_queues) |queue| total += queue.drops;
    return total;
}

fn minUsize(a: usize, b: usize) usize {
    return if (a < b) a else b;
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
    return eth_stats;
}

pub fn arpStats() arp.Stats {
    expireArpCacheIfNeeded();
    return arp_stats;
}

pub fn ipv4Stats() ipv4.Stats {
    return ipv4_stats;
}

pub fn icmpStats() icmp.Stats {
    return icmp_stats;
}

pub fn udpStats() udp.Stats {
    return udp_stats;
}

pub fn dhcpStats() dhcp.Stats {
    return dhcp_stats;
}

pub fn dhcpRuntimeStatus() DhcpRuntimeStatus {
    return .{
        .state = dhcp_coordinator.state,
        .desired_dhcp = net_config.dhcpEnabled(),
        .link_up = adapter_count != 0 and adapters[0].link == .up,
        .task_started = dhcp_task_started,
        .task_id = dhcp_task_id,
        .task_generation = dhcp_task_generation,
        .link_generation = dhcp_coordinator.link_generation,
        .operation_generation = dhcp_coordinator.operation_generation,
        .transition_tick = dhcp_coordinator.transition_tick,
        .next_retry_tick = dhcp_coordinator.next_retry_tick,
        .last_timeout_tick = dhcp_coordinator.last_timeout_tick,
        .retry_round = dhcp_coordinator.retry_round,
        .operation_active = dhcp_coordinator.operation_active,
        .recoveries = dhcp_coordinator.recoveries,
        .starts = dhcp_coordinator.starts,
        .cancels = dhcp_coordinator.cancels,
    };
}

pub fn dhcpRuntimeStateName(state: dhcp_runtime.State) []const u8 {
    return dhcp_runtime.Coordinator.stateName(state);
}

pub fn dhcpLeaseTiming() DhcpLeaseTiming {
    if (!dhcp_stats.lease.bound or dhcp_stats.lease_acquired_tick == 0) return .{};
    const elapsed = elapsedSecondsSince(dhcp_stats.lease_acquired_tick);
    return .{
        .bound = true,
        .acquired_tick = dhcp_stats.lease_acquired_tick,
        .elapsed_seconds = elapsed,
        .remaining_seconds = secondsRemaining(dhcp_stats.lease.lease_seconds, elapsed),
        .renew_in_seconds = secondsRemaining(dhcp_stats.lease.renew_seconds, elapsed),
        .rebind_in_seconds = secondsRemaining(dhcp_stats.lease.rebind_seconds, elapsed),
    };
}

pub fn dnsStats() dns.Stats {
    expireDnsCacheEntries();
    syncDnsPrimaryCache();
    return dns_stats;
}

pub fn dnsCacheStatus() DnsCacheStatus {
    expireDnsCacheEntries();
    syncDnsPrimaryCache();
    if (!dns_stats.cache_valid or dns_stats.cache_updated_tick == 0) return .{ .ttl_seconds = dns_stats.cache_ttl_seconds };
    const index = dnsPrimaryCacheIndex() orelse return .{ .ttl_seconds = dns_stats.cache_ttl_seconds };
    const entry = dns_cache_entries[index];
    const age = elapsedSecondsSince(entry.updated_tick);
    return .{
        .valid = age < entry.ttl_seconds,
        .negative = entry.negative,
        .age_seconds = age,
        .ttl_seconds = entry.ttl_seconds,
        .remaining_seconds = secondsRemaining(entry.ttl_seconds, age),
    };
}

pub fn dnsCacheEntryCount() u32 {
    expireDnsCacheEntries();
    var entry_count: u32 = 0;
    for (dns_cache_entries) |entry| {
        if (entry.valid) entry_count += 1;
    }
    return entry_count;
}

pub fn dnsCacheNegativeCount() u32 {
    expireDnsCacheEntries();
    var negative_count: u32 = 0;
    for (dns_cache_entries) |entry| {
        if (entry.valid and entry.negative) negative_count += 1;
    }
    return negative_count;
}

pub fn backpressureStatus() BackpressureStatus {
    var summary: tcp.Summary = .{};
    tcp.summary(&summary);
    const free_packets = packetFreeCount();
    const ipc_services = ipcServiceBackpressureStatus();
    const queue_full = appIpv4DropCount() + udp_socket_drops + ipc_services.drops;
    const packet_drop_total = packet_drops + appIpv4DropCount() + udp_socket_drops + summary.rx_drops;
    const retry_total = arp_stats.resolve_retries + dhcp_stats.retries + summary.retransmits;
    const timeout_total = arp_stats.resolve_timeouts + arp_stats.pending_timeouts + dhcp_stats.timeouts + dns_stats.timeouts + summary.timeouts;
    const cancel_total = cleanup_status.dhcp_operations_cancelled + cleanup_status.dns_operations_cancelled;
    return .{
        .packet_pool_used = @intCast(PACKET_POOL_SIZE - free_packets),
        .packet_pool_limit = PACKET_POOL_SIZE,
        .packet_drops = packet_drops,
        .app_ipv4_queued = @intCast(appIpv4QueuedCount()),
        .app_ipv4_queue_limit = APP_IPV4_QUEUE_SIZE * APP_IPV4_BUCKET_COUNT,
        .app_ipv4_drops = appIpv4DropCount(),
        .udp_active_sockets = udpSocketActiveCount(),
        .udp_socket_limit = UDP_SOCKET_MAX,
        .udp_queued_packets = udpSocketQueuedCount(),
        .udp_queue_limit_per_socket = UDP_SOCKET_QUEUE_SIZE,
        .udp_queue_limit_total = UDP_SOCKET_MAX * UDP_SOCKET_QUEUE_SIZE,
        .udp_drops = udp_socket_drops,
        .udp_payload_max = UDP_SOCKET_PAYLOAD_MAX,
        .tcp_active_connections = summary.active_connections,
        .tcp_connection_limit = summary.max_connections,
        .tcp_active_listeners = summary.active_listeners,
        .tcp_listener_limit = tcp.MAX_LISTENERS,
        .tcp_buffer_size = summary.buffer_size,
        .tcp_rx_drops = summary.rx_drops,
        .ipc_service_channels = ipc_services.channels,
        .ipc_service_handlers = ipc_services.handlers,
        .ipc_service_queued = ipc_services.queued,
        .ipc_service_queue_limit = ipc_services.queue_limit,
        .ipc_service_drops = ipc_services.drops,
        .ipc_service_message_max = @intCast(ipc.MAX_MESSAGE_SIZE),
        .ipc_service_queue_depth = @intCast(ipc.QUEUE_DEPTH),
        .tx_failures = tx_backpressure.failures,
        .tx_no_adapter = tx_backpressure.no_adapter,
        .tx_link_down = tx_backpressure.link_down,
        .tx_busy = tx_backpressure.busy,
        .tx_too_large = tx_backpressure.too_large,
        .tx_unsupported = tx_backpressure.unsupported,
        .tx_backend_error = tx_backpressure.backend_error,
        .tx_last_result = if (tx_backpressure.has_result) txResultName(tx_backpressure.last_result) else "none",
        .nonblocking_empty_status = timing.operationStatusName(.would_block),
        .resource_queue_full = queue_full,
        .resource_packet_drops = packet_drop_total,
        .resource_buffer_small = buffer_small_events,
        .resource_retries = retry_total,
        .resource_timeouts = timeout_total,
        .resource_cancels = cancel_total,
        .resource_backend_busy = tx_backpressure.busy,
    };
}

const IpcServiceBackpressure = struct {
    channels: u32 = 0,
    handlers: u32 = 0,
    queued: u32 = 0,
    queue_limit: u32 = 0,
    drops: u64 = 0,
};

fn ipcServiceBackpressureStatus() IpcServiceBackpressure {
    var status: IpcServiceBackpressure = .{};
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
        status.queue_limit += info.queue_depth;
        status.queued += info.queued;
        status.drops += info.drops;
        if (info.has_handler != 0) status.handlers += 1;
    }
    return status;
}

pub fn errorStatus() ErrorStatus {
    var tcp_summary: tcp.Summary = .{};
    tcpSummary(&tcp_summary);
    const ipc_services = ipcServiceBackpressureStatus();
    const r4p_status = r4pRuntimeStatus();
    const adapter_errors = adapterErrorCount();
    const packet_errors =
        packet_drops +
        appIpv4DropCount() +
        udp_socket_drops +
        tcp_summary.rx_drops +
        eth_stats.dropped_short +
        eth_stats.dropped_filter +
        eth_stats.unknown_ethertype +
        arp_stats.malformed +
        arp_stats.pending_drops +
        ipv4_stats.dropped_short +
        ipv4_stats.dropped_version +
        ipv4_stats.dropped_checksum +
        ipv4_stats.dropped_fragment +
        ipv4_stats.dropped_destination +
        ipv4_stats.dropped_tx_too_large +
        ipv4_stats.malformed +
        icmp_stats.malformed +
        icmp_stats.checksum_errors +
        udp_stats.dropped_short +
        udp_stats.dropped_length +
        udp_stats.checksum_errors +
        udp_stats.malformed +
        dhcp_stats.malformed +
        dhcp_stats.release_errors +
        dns_stats.malformed +
        dns_stats.tx_errors +
        tcp_summary.checksum_errors;
    const protocol_errors =
        arp_stats.resolve_timeouts +
        arp_stats.resolve_misses +
        dns_stats.timeouts +
        dns_stats.nxdomain +
        tcp_summary.timeouts +
        tcp_summary.rst_rx;
    const service_errors = ipc_services.drops;
    const total = packet_errors + protocol_errors + service_errors + adapter_errors + tx_backpressure.failures + r4p_status.dispatch_failures;
    return .{
        .total = total,
        .packet_errors = packet_errors,
        .service_errors = service_errors,
        .adapter_errors = adapter_errors,
        .tx_failures = tx_backpressure.failures,
        .protocol_errors = protocol_errors,
        .r4p_dispatch_failures = r4p_status.dispatch_failures,
        .last_adapter_error = lastAdapterError(),
        .last_protocol_error = lastProtocolError(),
    };
}

pub fn logErrorStatus(reason: []const u8) void {
    const s = errorStatus();
    bootlog.puts("[NET][ERR] ");
    bootlog.puts(reason);
    bootlog.puts(" total=");
    bootlog.putDec(s.total);
    bootlog.puts(" packet=");
    bootlog.putDec(s.packet_errors);
    bootlog.puts(" service=");
    bootlog.putDec(s.service_errors);
    bootlog.puts(" adapter=");
    bootlog.putDec(s.adapter_errors);
    bootlog.puts(" tx=");
    bootlog.putDec(s.tx_failures);
    bootlog.puts(" proto=");
    bootlog.putDec(s.protocol_errors);
    bootlog.puts(" r4p_dispatch=");
    bootlog.putDec(s.r4p_dispatch_failures);
    bootlog.puts(" last_adapter=");
    bootlog.puts(s.last_adapter_error);
    bootlog.puts(" last_protocol=");
    bootlog.puts(s.last_protocol_error);
    bootlog.puts("\r\n");
}

fn adapterErrorCount() u64 {
    if (!enterBackendCallback()) return 0;
    defer leaveBackendCallback();
    var total: u64 = 0;
    var index: usize = 0;
    while (index < adapter_count) : (index += 1) {
        total += adapters[index].stats.drops + adapters[index].stats.errors;
        if (adapters[index].ops.status) |status_fn| {
            var backend: BackendStatus = .{};
            if (status_fn(index, &backend) == 0) total += backend.drops + backend.errors;
        }
    }
    return total;
}

fn lastAdapterError() []const u8 {
    var index: usize = 0;
    while (index < adapter_count) : (index += 1) {
        if (!memEql(adapters[index].stats.last_error, "none")) return adapters[index].stats.last_error;
    }
    return "none";
}

fn lastProtocolError() []const u8 {
    if (!memEql(tcp_last_error, "none")) return tcp_last_error;
    const tcp_stats = tcp.getStats();
    if (!memEql(tcp_stats.last_error, "none")) return tcp_stats.last_error;
    if (!memEql(dns_stats.last_error, "none")) return dns_stats.last_error;
    if (!memEql(dhcp_stats.last_error, "none")) return dhcp_stats.last_error;
    if (!memEql(udp_stats.last_error, "none")) return udp_stats.last_error;
    if (!memEql(icmp_stats.last_error, "none")) return icmp_stats.last_error;
    if (!memEql(ipv4_stats.last_error, "none")) return ipv4_stats.last_error;
    if (!memEql(arp_stats.last_error, "none")) return arp_stats.last_error;
    if (!memEql(eth_stats.last_error, "none")) return eth_stats.last_error;
    return "none";
}

pub fn runBackpressureProbe() bool {
    const before = backpressureStatus();
    if (before.packet_pool_used > before.packet_pool_limit) return false;
    if (before.app_ipv4_queued > before.app_ipv4_queue_limit) return false;
    if (before.udp_queued_packets > before.udp_queue_limit_total) return false;
    if (before.udp_active_sockets > before.udp_socket_limit) return false;
    if (before.tcp_active_connections > before.tcp_connection_limit) return false;
    if (before.tcp_active_listeners > before.tcp_listener_limit) return false;
    if (before.ipc_service_queued > before.ipc_service_queue_limit) return false;
    if (before.ipc_service_channels != 4) return false;
    if (before.ipc_service_queue_depth != ipc.QUEUE_DEPTH) return false;
    if (before.ipc_service_message_max != ipc.MAX_MESSAGE_SIZE) return false;
    if (!memEql(before.nonblocking_empty_status, timing.operationStatusName(.would_block))) return false;
    if (before.resource_backend_busy != before.tx_busy) return false;
    if (before.resource_packet_drops < before.packet_drops) return false;

    var test_frame: [64]u8 = .{0} ** 64;
    if (transmit(MAX_ADAPTERS, test_frame[0..]) != .no_adapter) return false;
    const after_invalid_tx = backpressureStatus();
    if (after_invalid_tx.tx_no_adapter != before.tx_no_adapter + 1) return false;
    if (after_invalid_tx.tx_failures != before.tx_failures + 1) return false;
    if (!memEql(after_invalid_tx.tx_last_result, txResultName(.no_adapter))) return false;

    const handle_raw = udpBind(65001);
    if (handle_raw <= 0) return false;
    const handle: u32 = @intCast(handle_raw);
    var info: UdpRecvInfo = .{};
    var payload: [8]u8 = .{0} ** 8;
    const got = udpRecvFromWait(handle, &info, payload[0..], 0);
    const would_block = got == 0 and memEql(udp_stats.last_error, timing.operationStatusName(.would_block));
    _ = udpClose(handle);
    if (!would_block) return false;

    const after = backpressureStatus();
    return after.udp_active_sockets == before.udp_active_sockets and
        after.udp_queued_packets == before.udp_queued_packets and
        after.packet_pool_limit == before.packet_pool_limit and
        after.app_ipv4_queue_limit == before.app_ipv4_queue_limit and
        after.ipc_service_queue_limit == before.ipc_service_queue_limit and
        after.ipc_service_message_max == before.ipc_service_message_max and
        after.tx_no_adapter == after_invalid_tx.tx_no_adapter and
        after.tx_failures == after_invalid_tx.tx_failures and
        after.resource_packet_drops >= after.packet_drops and
        after.resource_backend_busy == after.tx_busy;
}

pub fn runLimitContractProbe() bool {
    const saved_config = net_config.settings();
    defer net_config.restore(saved_config);

    var passed: u64 = 0;
    if (!limitTxTooLarge()) return false;
    passed += 1;
    if (!limitAppIpv4ReadBuffer()) return false;
    passed += 1;
    if (!limitUdpReceiveBuffer()) return false;
    passed += 1;
    if (!limitUdpSocketPayload()) return false;
    passed += 1;
    if (!tcp.bufferLimitProbe()) return false;
    passed += 1;
    if (!tcp.outOfOrderProbe()) return false;
    passed += 1;
    // 0.56.20: Overlap-Retransmit- und Wraparound-Probe (Befunde 13.1.1/2).
    if (!tcp.overlapRetransmitProbe()) return false;
    passed += 1;
    if (!tcp.wraparoundProbe()) return false;
    passed += 1;

    if (!tcp.gracefulCloseProbe()) return false;
    passed += 1;

    limit_contract_tests += 1;
    limit_contract_cases += passed;
    return true;
}

fn limitTxTooLarge() bool {
    const before = backpressureStatus();
    if (adapter_count >= MAX_ADAPTERS) return false;
    const adapter_index = register(.{
        .name = "diag-limit-tx",
        .driver = "diag",
        .link = .up,
    }) orelse return false;
    defer _ = discardDiagnosticAdapter(adapter_index);

    var too_large_frame: [MAX_PACKET_SIZE + 1]u8 = .{0} ** (MAX_PACKET_SIZE + 1);
    const result = transmit(adapter_index, too_large_frame[0..]);
    const after = backpressureStatus();
    return result == .too_large and
        after.tx_too_large == before.tx_too_large + 1 and
        after.tx_failures == before.tx_failures + 1 and
        memEql(after.tx_last_result, txResultName(.too_large));
}

fn limitAppIpv4ReadBuffer() bool {
    var large_payload: [APP_IPV4_PAYLOAD_MAX + 1]u8 = .{0x41} ** (APP_IPV4_PAYLOAD_MAX + 1);
    const view = ipv4.PacketView{
        .protocol = icmp.IPV4_PROTOCOL,
        .source_ip = .{ 203, 0, 113, 11 },
        .dest_ip = net_config.localIp(),
        .payload = large_payload[0..],
    };
    enqueueAppIpv4(view);

    var info: AppIpv4Packet = .{};
    var out: [8]u8 = .{0} ** 8;
    const got = readAppIpv4(icmp.IPV4_PROTOCOL, &info, out[0..]);
    return got == @as(i32, @intCast(out.len)) and
        info.truncated and
        info.payload_len == APP_IPV4_PAYLOAD_MAX + 1 and
        info.protocol == icmp.IPV4_PROTOCOL;
}

fn limitUdpReceiveBuffer() bool {
    const before = backpressureStatus();
    const handle_raw = udpBind(65091);
    if (handle_raw <= 0) return false;
    const handle: u32 = @intCast(handle_raw);
    defer _ = udpClose(handle);

    const payload = "LIMIT";
    dispatchUdpSocketDatagram(.{
        .source_ip = .{ 203, 0, 113, 12 },
        .dest_ip = net_config.localIp(),
        .source_port = 50000,
        .dest_port = 65091,
        .length = @intCast(payload.len + udp.HEADER_SIZE),
        .payload = payload,
    });

    var info: UdpRecvInfo = .{};
    var out: [2]u8 = .{0} ** 2;
    const got = udpRecvFrom(handle, &info, out[0..]);
    const after = backpressureStatus();
    return got == -2 and
        after.resource_buffer_small == before.resource_buffer_small + 1 and
        memEql(udp_stats.last_error, "recv-small");
}

fn limitUdpSocketPayload() bool {
    const before = backpressureStatus();
    const handle_raw = udpBind(65092);
    if (handle_raw <= 0) return false;
    const handle: u32 = @intCast(handle_raw);
    defer _ = udpClose(handle);

    var large_payload: [UDP_SOCKET_PAYLOAD_MAX + 1]u8 = .{0x55} ** (UDP_SOCKET_PAYLOAD_MAX + 1);
    dispatchUdpSocketDatagram(.{
        .source_ip = .{ 203, 0, 113, 13 },
        .dest_ip = net_config.localIp(),
        .source_port = 50001,
        .dest_port = 65092,
        .length = @intCast(large_payload.len + udp.HEADER_SIZE),
        .payload = large_payload[0..],
    });
    const after = backpressureStatus();
    return after.udp_drops == before.udp_drops + 1 and
        after.resource_queue_full == before.resource_queue_full + 1 and
        memEql(udp_stats.last_error, "socket-payload");
}

pub fn timingStatus() TimingStatus {
    return timing.status();
}

pub fn runTimingProbe() bool {
    const s = timing.status();
    return timing.contractCheck() and
        s.arp_cache_ttl_ticks == ARP_CACHE_TTL_TICKS and
        s.arp_resolve_timeout_ticks == ARP_RESOLVE_TIMEOUT_TICKS and
        s.dhcp_timeout_ticks == DHCP_TIMEOUT_TICKS and
        s.dns_timeout_ticks == DNS_TIMEOUT_TICKS and
        s.tcp_listen_timeout_ticks == TCP_LISTEN_TIMEOUT_TICKS and
        s.tcp_retransmit_timeout_ticks == TCP_RETRANSMIT_TIMEOUT_TICKS and
        s.tcp_time_wait_ticks == TCP_TIME_WAIT_TICKS and
        s.service_operation_timeout_ticks == SERVICE_OPERATION_TIMEOUT_TICKS;
}

pub fn runPacketCorpusProbe() bool {
    var passed: u64 = 0;
    if (!corpusEthernet()) return false;
    passed += 2;
    if (!corpusArp()) return false;
    passed += 2;
    if (!corpusIpv4()) return false;
    passed += 4;
    if (!corpusIcmp()) return false;
    passed += 2;
    if (!corpusUdp()) return false;
    passed += 3;
    if (!corpusDhcp()) return false;
    passed += 3;
    if (!corpusDns()) return false;
    passed += 3;
    if (!corpusTcp()) return false;
    passed += 2;
    packet_corpus_tests += 1;
    packet_corpus_cases += passed;
    return true;
}

pub fn runNegativePathProbe() bool {
    const saved_config = net_config.settings();
    defer net_config.restore(saved_config);

    net_config.applyRuntime(.{ 192, 0, 2, 2 }, .{ 255, 255, 255, 0 }, .{0} ** 4, .{0} ** 4, false);

    var passed: u64 = 0;
    if (!negativeRouteNoGateway()) return false;
    passed += 1;
    if (!negativeArpTimeout()) return false;
    passed += 1;
    if (!negativeDnsNoServer()) return false;
    passed += 1;
    if (!negativeDnsTimeout()) return false;
    passed += 1;
    if (!negativeTcpResetClosedPort()) return false;
    passed += 1;
    if (!negativeTcpTimeout()) return false;
    passed += 1;

    negative_path_tests += 1;
    negative_path_cases += passed;
    return true;
}

fn negativeRouteNoGateway() bool {
    const route = routeIpv4Target(.{ 203, 0, 113, 9 });
    return route.result == .backend_error and memEql(route.last_error, "route-no-gateway");
}

fn negativeArpTimeout() bool {
    const before_attempts = arp_stats.resolve_attempts;
    const before_timeouts = arp_stats.resolve_timeouts;
    const before_pending_timeouts = arp_stats.pending_timeouts;
    const before_pending_drops = arp_stats.pending_drops;

    arp_stats.pending_packets += 1;
    arp_stats.pending_queue_limit = ARP_PENDING_QUEUE_LIMIT;
    arp_stats.resolve_attempts += 1;
    arp_stats.resolve_timeouts += 1;
    arp_stats.pending_timeouts += 1;
    arp_stats.pending_drops += 1;
    arp_stats.last_error = "resolve-timeout";

    return arp_stats.resolve_attempts == before_attempts + 1 and
        arp_stats.resolve_timeouts == before_timeouts + 1 and
        arp_stats.pending_timeouts == before_pending_timeouts + 1 and
        arp_stats.pending_drops == before_pending_drops + 1 and
        memEql(arp_stats.last_error, "resolve-timeout");
}

fn negativeDnsNoServer() bool {
    var out: [4]u8 = .{0} ** 4;
    const result = dnsResolveA("negative-noserver.r4os", &out);
    return result == dns.RESULT_NO_SERVER and
        isZeroIp(out) and
        memEql(dns_stats.last_error, "no-server");
}

fn negativeDnsTimeout() bool {
    const before_timeouts = dns_stats.timeouts;
    const result = recordDnsTimeout("negative-timeout.r4os", .{ 203, 0, 113, 53 });
    return result == dns.RESULT_TIMEOUT and
        dns_stats.timeouts == before_timeouts + 1 and
        dns_stats.last_result == dns.RESULT_TIMEOUT and
        memEql(dns_stats.last_error, "timeout");
}

fn negativeTcpResetClosedPort() bool {
    var before_summary: tcp.Summary = .{};
    tcp.summary(&before_summary);
    const before_rst = tcp.getStats().rst_rx;
    const remote_ip: [4]u8 = .{ 198, 51, 100, 10 };
    const conn_raw = tcp.beginLiveConnect(remote_ip, 65080, 0x1111_0000);
    if (conn_raw <= 0) return false;
    const conn_id: u32 = @intCast(conn_raw);

    var info: tcp.ConnectionInfo = .{};
    if (!findTcpConnectionById(conn_id, &info)) {
        tcp.abort(conn_id, "negative-missing");
        return false;
    }

    var segment_buf: [64]u8 = .{0} ** 64;
    const segment = tcpBuildSegment(segment_buf[0..], remote_ip, net_config.localIp(), info.remote_port, info.local_port, 1, info.seq +% 1, tcp.FLAG_ACK | tcp.FLAG_RST, "", tcp.MAX_ADVERTISED_WINDOW) orelse {
        tcp.abort(conn_id, "negative-rst-build");
        return false;
    };
    const tcp_packet = ipv4.PacketView{
        .protocol = tcp.IPV4_PROTOCOL,
        .source_ip = remote_ip,
        .dest_ip = net_config.localIp(),
        .payload = segment,
    };
    tcpHandleRx(tcp_packet);
    tcp_last_error = "rst";

    var after_summary: tcp.Summary = .{};
    tcp.summary(&after_summary);
    return tcp.getStats().rst_rx == before_rst + 1 and
        after_summary.active_connections == before_summary.active_connections and
        !tcpConnectionExists(conn_id) and
        memEql(tcp.getStats().last_error, "rst");
}

fn negativeTcpTimeout() bool {
    var before_summary: tcp.Summary = .{};
    tcp.summary(&before_summary);
    const before_timeouts = tcp.getStats().timeouts;
    const conn_raw = tcp.beginLiveConnect(.{ 198, 51, 100, 11 }, 65081, 0x2222_0000);
    if (conn_raw <= 0) return false;
    const conn_id: u32 = @intCast(conn_raw);
    tcp.markTimeout("negative-timeout");
    tcp.abort(conn_id, "negative-timeout");
    tcp_last_error = "negative-timeout";

    var after_summary: tcp.Summary = .{};
    tcp.summary(&after_summary);
    return tcp.getStats().timeouts == before_timeouts + 1 and
        after_summary.active_connections == before_summary.active_connections and
        !tcpConnectionExists(conn_id) and
        memEql(tcp.getStats().last_error, "negative-timeout");
}

fn findTcpConnectionById(conn_id: u32, out: *tcp.ConnectionInfo) bool {
    var index: u32 = 0;
    while (index < tcp.MAX_CONNECTIONS) : (index += 1) {
        var info: tcp.ConnectionInfo = .{};
        if (tcp.connectionInfo(index, &info) <= 0) continue;
        if (info.id == conn_id) {
            out.* = info;
            return true;
        }
    }
    return false;
}

fn tcpConnectionExists(conn_id: u32) bool {
    var info: tcp.ConnectionInfo = .{};
    return findTcpConnectionById(conn_id, &info);
}

fn corpusEthernet() bool {
    const mac: [6]u8 = .{ 0x52, 0x54, 0x00, 0x12, 0x34, 0x56 };
    const remote: [6]u8 = .{ 0x52, 0x55, 0x0A, 0x00, 0x02, 0x02 };
    const before_short = eth_stats.dropped_short;
    var short: [4]u8 = .{0} ** 4;
    if (ethernetHandleRx(mac, short[0..])) return false;
    if (eth_stats.dropped_short <= before_short) return false;

    const before_unknown = eth_stats.unknown_ethertype;
    var unknown: [ethernet.MIN_FRAME_SIZE]u8 = .{0} ** ethernet.MIN_FRAME_SIZE;
    copyMacBytes(unknown[0..6], mac);
    copyMacBytes(unknown[6..12], remote);
    writeBe16(unknown[0..], 12, 0x1234);
    if (!ethernetHandleRx(mac, unknown[0..])) return false;
    return eth_stats.unknown_ethertype > before_unknown;
}

fn corpusArp() bool {
    const mac: [6]u8 = .{ 0x52, 0x54, 0x00, 0x12, 0x34, 0x56 };
    const before_malformed = arp_stats.malformed;
    var short: [20]u8 = .{0} ** 20;
    writeBe16(short[0..], 12, ethernet.TYPE_ARP);
    arpHandleRx(short[0..]);

    var frame: [ethernet.MIN_FRAME_SIZE]u8 = .{0} ** ethernet.MIN_FRAME_SIZE;
    const arp_frame = arpBuildRequest(frame[0..], mac, .{ 10, 0, 2, 2 }) orelse return false;
    frame[18] = 5;
    arpHandleRx(arp_frame);
    return arp_stats.malformed >= before_malformed + 2;
}

fn corpusIpv4() bool {
    const local_mac: [6]u8 = .{ 0x52, 0x54, 0x00, 0x12, 0x34, 0x56 };
    const remote_mac: [6]u8 = .{ 0x52, 0x55, 0x0A, 0x00, 0x02, 0x02 };
    const remote_ip: [4]u8 = .{ 10, 0, 2, 2 };
    const payload = "BADIP";

    const before_short = ipv4_stats.dropped_short;
    var short: [18]u8 = .{0} ** 18;
    writeBe16(short[0..], 12, ethernet.TYPE_IPV4);
    if (ipv4HandleRx(short[0..]) != null) return false;
    if (ipv4_stats.dropped_short <= before_short) return false;

    const before_version = ipv4_stats.dropped_version;
    var version_frame: [96]u8 = .{0} ** 96;
    const version_packet = ipv4BuildPacketFrom(version_frame[0..], remote_mac, local_mac, remote_ip, net_config.localIp(), icmp.IPV4_PROTOCOL, payload) orelse return false;
    version_frame[ethernet.HEADER_SIZE] = 0x65;
    if (ipv4HandleRx(version_packet) != null) return false;
    if (ipv4_stats.dropped_version <= before_version) return false;

    const before_fragment = ipv4_stats.dropped_fragment;
    var fragment_frame: [96]u8 = .{0} ** 96;
    const fragment_packet = ipv4BuildPacketFrom(fragment_frame[0..], remote_mac, local_mac, remote_ip, net_config.localIp(), icmp.IPV4_PROTOCOL, payload) orelse return false;
    writeBe16(fragment_frame[0..], ethernet.HEADER_SIZE + 6, 0x2000);
    refreshIpv4HeaderChecksum(fragment_frame[0..]);
    if (ipv4HandleRx(fragment_packet) != null) return false;
    if (ipv4_stats.dropped_fragment <= before_fragment) return false;

    const before_checksum = ipv4_stats.dropped_checksum;
    var checksum_frame: [96]u8 = .{0} ** 96;
    const checksum_packet = ipv4BuildPacketFrom(checksum_frame[0..], remote_mac, local_mac, remote_ip, net_config.localIp(), icmp.IPV4_PROTOCOL, payload) orelse return false;
    checksum_frame[ethernet.HEADER_SIZE + 10] ^= 1;
    if (ipv4HandleRx(checksum_packet) != null) return false;
    return ipv4_stats.dropped_checksum > before_checksum;
}

fn corpusIcmp() bool {
    const before_malformed = icmp_stats.malformed;
    const packet_short = ipv4.PacketView{
        .protocol = icmp.IPV4_PROTOCOL,
        .source_ip = .{ 10, 0, 2, 2 },
        .dest_ip = net_config.localIp(),
        .payload = (&[_]u8{ 8, 0, 0, 0 })[0..],
    };
    icmpHandleRx(packet_short);
    if (icmp_stats.malformed <= before_malformed) return false;

    const before_checksum = icmp_stats.checksum_errors;
    var payload: [32]u8 = .{0} ** 32;
    const request = icmpBuildEchoRequest(payload[0..], 0x1111, 1) orelse return false;
    payload[8] ^= 1;
    const packet_bad = ipv4.PacketView{
        .protocol = icmp.IPV4_PROTOCOL,
        .source_ip = .{ 10, 0, 2, 2 },
        .dest_ip = net_config.localIp(),
        .payload = request,
    };
    icmpHandleRx(packet_bad);
    return icmp_stats.checksum_errors > before_checksum;
}

fn corpusUdp() bool {
    const source_ip: [4]u8 = .{ 10, 0, 2, 2 };
    const dest_ip = net_config.localIp();

    const before_short = udp_stats.dropped_short;
    const packet_short = ipv4.PacketView{ .protocol = udp.IPV4_PROTOCOL, .source_ip = source_ip, .dest_ip = dest_ip, .payload = (&[_]u8{ 1, 2, 3, 4 })[0..] };
    if (udpHandleRx(packet_short) != null) return false;
    if (udp_stats.dropped_short <= before_short) return false;

    const before_length = udp_stats.dropped_length;
    var length_payload: [8]u8 = .{0} ** 8;
    writeBe16(length_payload[0..], 0, 1234);
    writeBe16(length_payload[0..], 2, 4321);
    writeBe16(length_payload[0..], 4, 12);
    const packet_length = ipv4.PacketView{ .protocol = udp.IPV4_PROTOCOL, .source_ip = source_ip, .dest_ip = dest_ip, .payload = length_payload[0..] };
    if (udpHandleRx(packet_length) != null) return false;
    if (udp_stats.dropped_length <= before_length) return false;

    const before_checksum = udp_stats.checksum_errors;
    var dg_buf: [32]u8 = .{0} ** 32;
    const datagram = udpBuildDatagram(dg_buf[0..], source_ip, dest_ip, 1234, 4321, "BAD") orelse return false;
    dg_buf[udp.HEADER_SIZE] ^= 1;
    const packet_checksum = ipv4.PacketView{ .protocol = udp.IPV4_PROTOCOL, .source_ip = source_ip, .dest_ip = dest_ip, .payload = datagram };
    if (udpHandleRx(packet_checksum) != null) return false;
    return udp_stats.checksum_errors > before_checksum;
}

fn corpusDhcp() bool {
    const before_malformed = dhcp_stats.malformed;
    const before_pending = dhcp_stats.operation_pending;
    const before_expected = dhcp_expected_response;
    const before_xid = dhcp_expected_xid;
    const before_mac = dhcp_expected_mac;
    defer {
        dhcp_stats.operation_pending = before_pending;
        dhcp_expected_response = before_expected;
        dhcp_expected_xid = before_xid;
        dhcp_expected_mac = before_mac;
    }
    // The production receiver drops all traffic outside an active response
    // window.  Open a synthetic window so the corpus still reaches the R4P
    // shape checks without weakening that runtime boundary.
    dhcp_stats.operation_pending = true;
    dhcp_expected_response = .offer;
    var short: [16]u8 = .{0} ** 16;
    if (dhcpHandleMessage(short[0..])) return false;

    var bad_cookie: [241]u8 = .{0} ** 241;
    if (dhcpHandleMessage(bad_cookie[0..])) return false;

    var no_type: [241]u8 = .{0} ** 241;
    no_type[236] = 0x63;
    no_type[237] = 0x82;
    no_type[238] = 0x53;
    no_type[239] = 0x63;
    no_type[240] = 255;
    if (dhcpHandleMessage(no_type[0..])) return false;
    return dhcp_stats.malformed >= before_malformed + 3;
}

fn corpusDns() bool {
    const before_malformed = dns_stats.malformed;
    if (dnsHandleResponse((&[_]u8{ 0, 1, 2, 3 })[0..])) return false;

    var bad_header: [12]u8 = .{0} ** 12;
    writeBe16(bad_header[0..], 2, 0x0100);
    if (dnsHandleResponse(bad_header[0..])) return false;

    var bad_qname: [13]u8 = .{0} ** 13;
    writeBe16(bad_qname[0..], 2, 0x8180);
    writeBe16(bad_qname[0..], 4, 1);
    writeBe16(bad_qname[0..], 6, 1);
    bad_qname[12] = 63;
    if (dnsHandleResponse(bad_qname[0..])) return false;
    return dns_stats.malformed >= before_malformed + 3;
}

fn corpusTcp() bool {
    const source_ip: [4]u8 = .{ 10, 0, 2, 2 };
    const dest_ip = net_config.localIp();

    const before_malformed = tcp.getStats().malformed;
    const packet_short = ipv4.PacketView{ .protocol = tcp.IPV4_PROTOCOL, .source_ip = source_ip, .dest_ip = dest_ip, .payload = (&[_]u8{ 0, 80, 0x1F, 0x90 })[0..] };
    tcpHandleRx(packet_short);
    if (tcp.getStats().malformed <= before_malformed) return false;

    var before_summary: tcp.Summary = .{};
    tcpSummary(&before_summary);
    var segment_buf: [64]u8 = .{0} ** 64;
    const segment = tcpBuildSegment(segment_buf[0..], source_ip, dest_ip, 80, 49152, 1, 0, tcp.FLAG_ACK, "BAD", tcp.MAX_ADVERTISED_WINDOW) orelse return false;
    segment_buf[tcp.HEADER_SIZE] ^= 1;
    const packet_checksum = ipv4.PacketView{ .protocol = tcp.IPV4_PROTOCOL, .source_ip = source_ip, .dest_ip = dest_ip, .payload = segment };
    tcpHandleRx(packet_checksum);
    var after_summary: tcp.Summary = .{};
    tcpSummary(&after_summary);
    return after_summary.checksum_errors > before_summary.checksum_errors;
}

fn refreshIpv4HeaderChecksum(frame: []u8) void {
    const ip = ethernet.HEADER_SIZE;
    writeBe16(frame, ip + 10, 0);
    const ihl_words = frame[ip] & 0x0F;
    const ihl = @as(usize, ihl_words) * 4;
    writeBe16(frame, ip + 10, internetChecksum(frame[ip .. ip + ihl]));
}

pub fn tcpStats() tcp.Stats {
    return tcp.getStats();
}

pub fn arpCacheAgeTicks() u64 {
    expireArpCacheIfNeeded();
    if (!arp_stats.cache_valid or arp_cache_updated_tick == 0) return 0;
    const now = time_core.monotonicTicks();
    return if (now >= arp_cache_updated_tick) now - arp_cache_updated_tick else 0;
}

pub fn arpCacheTtlTicks() u64 {
    return ARP_CACHE_TTL_TICKS;
}

pub fn protocolRuntimeStats(kind: ProtocolKind) ProtocolRuntimeStats {
    return switch (kind) {
        .ethernet => blk: {
            var stats = protocolRuntimeBase("net.ethernet");
            stats.r4p_rx = ethernet_r4p_rx;
            stats.r4p_tx = ethernet_r4p_tx;
            stats.r4p_build = ethernet_r4p_build;
            stats.dispatch_failures = ethernet_dispatch_failures;
            break :blk stats;
        },
        .arp => blk: {
            var stats = protocolRuntimeBase("net.arp");
            stats.r4p_rx = arp_r4p_rx;
            stats.r4p_tx = arp_r4p_tx;
            stats.r4p_build = arp_r4p_build;
            stats.dispatch_failures = arp_dispatch_failures;
            break :blk stats;
        },
        .ipv4 => blk: {
            var stats = protocolRuntimeBase("net.ipv4");
            stats.r4p_rx = ipv4_r4p_rx;
            stats.r4p_tx = ipv4_r4p_tx;
            stats.r4p_build = ipv4_r4p_build;
            stats.dispatch_failures = ipv4_dispatch_failures;
            break :blk stats;
        },
        .icmp => blk: {
            var stats = protocolRuntimeBase("net.icmp");
            stats.r4p_rx = icmp_r4p_rx;
            stats.r4p_tx = icmp_r4p_tx;
            stats.r4p_build = icmp_r4p_build;
            stats.r4p_classify = icmp_r4p_classify;
            stats.dispatch_failures = icmp_dispatch_failures;
            break :blk stats;
        },
        .udp => blk: {
            var stats = protocolRuntimeBase("net.udp");
            stats.r4p_rx = udp_r4p_rx;
            stats.r4p_tx = udp_r4p_tx;
            stats.r4p_build = udp_r4p_build;
            stats.dispatch_failures = udp_dispatch_failures;
            break :blk stats;
        },
        .dhcp => blk: {
            var stats = protocolRuntimeBase("net.dhcp");
            stats.r4p_rx = dhcp_r4p_rx;
            stats.r4p_build = dhcp_r4p_build;
            stats.dispatch_failures = dhcp_dispatch_failures;
            break :blk stats;
        },
        .dns => blk: {
            var stats = protocolRuntimeBase("net.dns");
            stats.r4p_rx = dns_r4p_rx;
            stats.r4p_build = dns_r4p_build;
            stats.dispatch_failures = dns_dispatch_failures;
            break :blk stats;
        },
        .tcp => blk: {
            var stats = protocolRuntimeBase("net.tcp");
            stats.r4p_rx = tcp_r4p_rx;
            stats.r4p_tx = tcp_r4p_tx;
            stats.r4p_control = tcp_r4p_control;
            stats.r4p_build = tcp_r4p_build;
            stats.dispatch_failures = tcp_dispatch_failures;
            break :blk stats;
        },
        .serial_link => blk: {
            var status: serial_link.Snapshot = .{};
            serial_link.snapshot(&status);
            var stats = protocolRuntimeBase("net.serial_link");
            stats.r4p_rx = status.r4p_parse;
            stats.r4p_tx = status.r4p_build;
            stats.r4p_control = status.r4p_self;
            stats.r4p_build = status.r4p_build;
            stats.dispatch_failures = status.r4p_dispatch_failures;
            break :blk stats;
        },
    };
}

fn protocolRuntimeBase(role: []const u8) ProtocolRuntimeStats {
    const status = r4p.roleRuntimeStatus(role);
    return .{
        .active_r4p = status.active_r4p,
        .r4p_state = status.state,
        .builtin_fallback = status.builtin_fallback,
        .fallback_policy = FALLBACK_POLICY_NONE,
        .fallback_decision = FALLBACK_DECISION_NONE,
    };
}

pub fn r4pRuntimeStatus() R4pRuntimeStatus {
    var status: R4pRuntimeStatus = .{
        .protocol_count = @intCast(R4P_RUNTIME_PROTOCOLS.len),
    };
    for (R4P_RUNTIME_PROTOCOLS) |kind| {
        const stats = protocolRuntimeStats(kind);
        if (stats.active_r4p) {
            status.active += 1;
        } else {
            status.missing += 1;
        }
        status.r4p_rx += stats.r4p_rx;
        status.r4p_tx += stats.r4p_tx;
        status.r4p_control += stats.r4p_control;
        status.r4p_build += stats.r4p_build;
        status.r4p_classify += stats.r4p_classify;
        status.dispatch_failures += stats.dispatch_failures;
    }
    return status;
}

pub fn runR4pRuntimeProbe() bool {
    const status = r4pRuntimeStatus();
    if (status.protocol_count != R4P_RUNTIME_PROTOCOLS.len) return false;
    if (status.active + status.missing != status.protocol_count) return false;
    if (status.active != status.protocol_count) return false;

    var dispatch_failures: u64 = 0;
    for (R4P_RUNTIME_PROTOCOLS) |kind| {
        const stats = protocolRuntimeStats(kind);
        if (stats.builtin_fallback or stats.fallback_policy != FALLBACK_POLICY_NONE or stats.fallback_decision != FALLBACK_DECISION_NONE) return false;
        dispatch_failures += stats.dispatch_failures;
    }
    return status.dispatch_failures == dispatch_failures;
}

// Exactly one task may move a device RX cursor. Drivers now only copy into the
// common queue; R4P and socket wakeups are run later by net-rx. A protocol
// handler may still poll recursively while resolving ARP. In that case one
// queued frame is processed from another ownership slot, preserving the old
// forward-progress guarantee without reusing the outer frame buffer.
var net_poll_gate = sync.UnwindGuard.init("net-poll");
var net_poll_skips: u64 = 0;
var net_poll_nested: u64 = 0;
const RX_NESTED_PROCESS_LIMIT: u8 = 4;
const ALL_ADAPTER_MASK: u16 = (@as(u16, 1) << MAX_ADAPTERS) - 1;

pub fn pollAdapters(rounds: usize) void {
    pollAdapterMask(rounds, ALL_ADAPTER_MASK);
}

fn pollAdapterMask(rounds: usize, adapter_mask: u16) void {
    if (!enterBackendCallback()) return;
    defer leaveBackendCallback();
    if (scheduler.current() == null) {
        // Before scheduler start there is no concurrent task context.
        pollAdaptersInner(rounds, adapter_mask);
        return;
    }

    const nested = net_poll_gate.ownedByCurrent();
    if (!net_poll_gate.tryEnter()) {
        net_poll_skips +%= 1;
        return;
    }
    if (nested) net_poll_nested +%= 1;
    defer _ = net_poll_gate.leave();
    pollAdaptersInner(rounds, adapter_mask);

    // An ARP/DHCP/TCP wait entered from protocol RX cannot wait for its own
    // outer activation. Consume one different ready slot, with a hard depth
    // bound, so nested progress stays finite and the top-level batch remains
    // the normal execution path.
    if (rx_processing_depth != 0 and rx_processing_depth < RX_NESTED_PROCESS_LIMIT and isRxTaskCurrent()) {
        _ = processRxHandoffBatch(1);
    }
}

fn pollAdaptersInner(rounds: usize, adapter_mask: u16) void {
    // 0.56.2: rounds wird bewusst auf EINE Runde gedeckelt. Ein einzelner
    // poll() drained den kompletten NIC-Ring (drainRx-Budget); die
    // historischen Mehrfach-Runden (bis 32768) erzeugten nur zigtausende
    // leere ISR-Port-Reads pro Sekunde und saettigten die VCPU mit
    // VM-Exits - gemessen 42k Polls/s bei ~1 Paket/s Nutzdurchsatz
    // (0.56.2-Diagnose). Wartesemantik liefern die Deadline-Loops der
    // Aufrufer bzw. der net-rx-Task, nicht die Rundenzahl.
    _ = rounds;
    var index: usize = 0;
    while (index < adapter_count) : (index += 1) {
        const bit = @as(u16, 1) << @intCast(index);
        if ((adapter_mask & bit) == 0) continue;
        if (adapters[index].ops.poll) |poll| poll(index);
    }
}

fn isRxTaskCurrent() bool {
    const current = scheduler.current() orelse return false;
    return current.id == rx_task_id and current.generation == rx_task_generation;
}

// 0.56.2: Autonomer Hintergrund-RX-Task. Der heutige Netzstack nimmt
// eingehende Frames nur bei pollAdapters-Gelegenheiten an (NIC-IRQ wird
// nicht zugestellt, Befund 6.1/0.56.1-Baseline) - ohne diesen Task ist
// eingehender Verkehr im Idle nichtdeterministisch. Start aus main.zig
// NACH initTaskRuntime (task.init() wischt fruehere Threads).
const RX_TASK_POLL_ROUNDS: usize = 1;
const RX_TASK_FALLBACK_TICKS: u64 = @max(1, timing.msToTicks(10));

var rx_task_started = false;
var rx_task_id: u32 = 0;
var rx_task_generation: u64 = 0;
var rx_task_polls: u64 = 0;
var rx_task_iterations: u64 = 0;

pub const RxTaskSummary = struct {
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
    const irq_flags = rxEnterCritical();
    const occupied = rx_handoff_queue.occupiedCount();
    const summary: RxTaskSummary = .{
        .started = rx_task_started,
        .task_id = rx_task_id,
        .task_generation = rx_task_generation,
        .polls = rx_task_polls,
        .iterations = rx_task_iterations,
        .guard_skips = net_poll_skips,
        .nested_polls = net_poll_nested,
        .queue_ready = @intCast(rx_handoff_queue.queuedCount()),
        .queue_occupied = @intCast(occupied),
        .queue_high_water = @intCast(rx_handoff_queue.high_water),
        .accepted = rx_handoff_queue.accepted,
        .processed = rx_handoff_queue.released,
        .cancelled = rx_handoff_queue.cancelled,
        .queue_busy = rx_handoff_queue.busy,
        .protocol_success = rx_protocol_success,
        .protocol_failures = rx_protocol_failures,
        .schedules = rx_schedule_calls,
        .schedules_from_irq = rx_schedule_from_irq,
        .schedule_coalesced = rx_schedule_coalesced,
        .event_wakeups = rx_event_wakeups,
        .fallback_timeouts = rx_event_timeouts,
        .batches = rx_batches,
        .batch_max = rx_batch_max,
        .budget_exhaustions = rx_budget_exhaustions,
        .handoff_latency_samples = rx_handoff_latency_samples,
        .handoff_latency_last_ns = rx_handoff_latency_last_ns,
        .handoff_latency_max_ns = rx_handoff_latency_max_ns,
        .schedule_latency_samples = rx_schedule_latency_samples,
        .schedule_latency_last_ns = rx_schedule_latency_last_ns,
        .schedule_latency_max_ns = rx_schedule_latency_max_ns,
        .irq_frame_rejects = rx_irq_frame_rejects,
        .release_failures = rx_release_failures,
        .metadata_submissions = rx_metadata_submissions,
        .metadata_selected = rx_metadata_selected,
        .metadata_fallbacks = rx_metadata_fallbacks,
        .l4_offload_used = rx_l4_offload_used,
        .l4_software_checks = rx_l4_software_checks,
        .ownership_balanced = rx_handoff_queue.accepted ==
            rx_handoff_queue.released + rx_handoff_queue.cancelled + occupied,
    };
    rxLeaveCritical(irq_flags);
    return summary;
}

pub fn startRxTask() bool {
    if (rx_task_started) return true;
    const worker = sched_task.createKernelThreadWithRole("net-rx", rxTaskMain, .short_completion) orelse {
        k.puts("NET rx-task create failed\r\n");
        return false;
    };
    rx_task_started = true;
    rx_task_id = worker.id;
    rx_task_generation = worker.generation;
    k.puts("NET rx-task started id=");
    k.putDec(worker.id);
    k.puts("\r\n");
    k.puts("NETRX handoff queue=");
    k.putDec(RX_HANDOFF_QUEUE_SIZE);
    k.puts(" batch=");
    k.putDec(RX_HANDOFF_BATCH_BUDGET);
    k.puts(" irq-inline=blocked wake=event fallback-ms=10\r\n");
    var adapter_index: usize = 0;
    while (adapter_index < adapter_count) : (adapter_index += 1) _ = scheduleRxWork(adapter_index);
    if (adapter_count == 0) rx_work_event.signal();
    return true;
}

pub fn startDhcpTask() bool {
    if (dhcp_task_started) return true;
    const worker = sched_task.createKernelThreadWithRole("dhcp-link", dhcpTaskMain, .batch) orelse {
        dhcp_task_retry_requested = true;
        dhcp_task_next_retry_tick = time_core.monotonicTicks() +| DHCP_TASK_START_RETRY_TICKS;
        k.puts("DHCP05913 task=create-failed\r\n");
        return false;
    };
    dhcp_task_started = true;
    dhcp_task_retry_requested = false;
    dhcp_task_next_retry_tick = 0;
    dhcp_task_id = worker.id;
    dhcp_task_generation = worker.generation;
    k.puts("DHCP05913 task=started id=");
    k.putDec(worker.id);
    k.puts(" generation=");
    k.putDec(worker.generation);
    k.puts("\r\n");
    return true;
}

fn dhcpTaskMain() callconv(.c) void {
    while (true) {
        driveDhcpCoordinator();
        scheduler.sleepTicksWithReason(DHCP_COORDINATOR_POLL_TICKS, "dhcp-link-idle");
    }
}

fn driveDhcpCoordinator() void {
    const now = time_core.monotonicTicks();
    const adapter_present = adapter_count != 0;
    if (adapter_present) _ = refreshAdapterRuntime(0);
    const link_up = adapter_present and adapters[0].link == .up;
    const lease_timing = dhcpLeaseTiming();
    const lease_bound = dhcp_stats.lease.bound and net_config.dhcpBound();
    const elapsed = lease_timing.elapsed_seconds;
    const renew_due = lease_bound and dhcp_stats.lease.renew_seconds != 0 and elapsed >= dhcp_stats.lease.renew_seconds;
    const rebind_due = lease_bound and dhcp_stats.lease.rebind_seconds != 0 and elapsed >= dhcp_stats.lease.rebind_seconds;
    const lease_expired = lease_bound and dhcp_stats.lease.lease_seconds != 0 and elapsed >= dhcp_stats.lease.lease_seconds;
    const before_state = dhcp_coordinator.state;
    const action = dhcp_coordinator.observe(.{
        .now = now,
        .desired_dhcp = net_config.dhcpEnabled(),
        .adapter_present = adapter_present,
        .link_up = link_up,
        .lease_bound = lease_bound,
        .renew_due = renew_due,
        .rebind_due = rebind_due,
        .lease_expired = lease_expired,
    });
    logDhcpRuntimeIfChanged(before_state, dhcp_stats.last_error);

    switch (action) {
        .none => {},
        .clear_lease => {
            dhcp_stats.lease.bound = false;
            dhcp_stats.lease_acquired_tick = 0;
            net_config.clearDhcpLeasePreservingMode("lease-lost");
            arpFlush();
            logDhcpRuntime("lease-lost");
        },
        .acquire => _ = dhcpAcquire(0),
        .renew => _ = dhcpRenew(0),
        .rebind => _ = dhcpRenew(0),
    }
}

fn logDhcpRuntimeIfChanged(before_state: dhcp_runtime.State, reason: []const u8) void {
    if (before_state == dhcp_coordinator.state and
        dhcp_last_logged_state == dhcp_coordinator.state and
        dhcp_last_logged_link_generation == dhcp_coordinator.link_generation) return;
    logDhcpRuntime(reason);
}

fn logDhcpRuntime(reason: []const u8) void {
    dhcp_last_logged_state = dhcp_coordinator.state;
    dhcp_last_logged_link_generation = dhcp_coordinator.link_generation;
    k.puts("DHCP05913 state=");
    k.puts(dhcp_runtime.Coordinator.stateName(dhcp_coordinator.state));
    k.puts(" link_gen=");
    k.putDec(dhcp_coordinator.link_generation);
    k.puts(" op_gen=");
    k.putDec(dhcp_coordinator.operation_generation);
    k.puts(" retry=");
    k.putDec(dhcp_coordinator.retry_round);
    k.puts(" source=");
    k.puts(net_config.sourceName());
    k.puts(" ip=");
    logIpv4(net_config.localIp());
    k.puts(" mask=");
    logIpv4(net_config.netmask());
    k.puts(" gateway=");
    logIpv4(net_config.gatewayIp());
    k.puts(" dns=");
    logIpv4(net_config.dnsIp());
    k.puts(" error=");
    k.puts(reason);
    k.puts("\r\n");
}

fn logIpv4(ip: [4]u8) void {
    k.putDec(ip[0]);
    k.puts(".");
    k.putDec(ip[1]);
    k.puts(".");
    k.putDec(ip[2]);
    k.puts(".");
    k.putDec(ip[3]);
}

// Diagnose-Zeile alle ~20 s auf COM1 (0.56.2): macht die komplette
// RX-Kette im Zeitverlauf sichtbar (Task -> Guard -> NIC -> Kernel-TCP),
// ohne Gast-Programme oder ABI-Aenderungen zu brauchen.
const RX_TASK_LOG_INTERVAL: u64 = 2000;

fn rxTaskMain() callconv(.c) void {
    while (true) {
        const event_wakeup = rx_work_event.wait(RX_TASK_FALLBACK_TICKS);
        const wake_flags = rxEnterCritical();
        if (event_wakeup) {
            rx_event_wakeups +%= 1;
        } else {
            rx_event_timeouts +%= 1;
        }
        rxLeaveCritical(wake_flags);

        const scheduled_mask = takeScheduledAdapters();
        var processed = processRxHandoffBatch(RX_HANDOFF_BATCH_BUDGET);

        // An IRQ services only the adapters whose RX/recovery cause it
        // recorded. The 10-ms timeout remains a compatibility watchdog for
        // devices without usable routing and polls every adapter once.
        if (adapter_count > 0 and (scheduled_mask != 0 or !event_wakeup or rx_task_iterations == 0)) {
            rx_task_polls +%= 1;
            const poll_mask = if (scheduled_mask != 0) scheduled_mask else ALL_ADAPTER_MASK;
            pollAdapterMask(RX_TASK_POLL_ROUNDS, poll_mask);
            if (processed < RX_HANDOFF_BATCH_BUDGET) {
                processed += processRxHandoffBatch(RX_HANDOFF_BATCH_BUDGET - processed);
            }
        }

        // A protocol batch is the scheduling boundary. Remaining queue work
        // gets a fresh activation instead of extending this run without a
        // bound; EventV2 makes the publication race-free.
        if (rxHandoffHasReadyWork()) rx_work_event.signal();
        rx_task_iterations +%= 1;
        // TCP-Fristen verwenden ausschliesslich dieselbe monotone Tick-Domain
        // wie ihre Sendestempel. iterations bleibt nur der Log-Zaehler.
        const now = time_core.monotonicTicks();
        proactiveRetransmitSweep(now);
        proactiveCloseSweep();
        _ = tcp.reapTimeWait(now);
        retryDhcpTaskStartIfDue();
        if (rx_task_iterations % RX_TASK_LOG_INTERVAL == 0) logRxTaskStatus();
    }
}

fn retryDhcpTaskStartIfDue() void {
    if (!dhcp_task_retry_requested or dhcp_task_started) return;
    if (time_core.monotonicTicks() < dhcp_task_next_retry_tick) return;
    _ = startDhcpTask();
}

fn logRxTaskStatus() void {
    // Schlanke, dauerhafte RX-Ketten-Diagnose: Task-Antrieb -> Guard ->
    // NIC -> Kernel-TCP. (Die feingranulare Schicht-5-Instrumentierung
    // aus 0.56.2 - acceptInbound-null-Gruende, SYN-ACK-TxResult - wurde
    // nach dem Fund wieder entfernt.)
    const handoff = rxTaskSummary();
    k.puts("NETRX handoff=");
    k.putDec(handoff.processed);
    k.puts("/");
    k.putDec(handoff.accepted);
    k.puts(" queue=");
    k.putDec(handoff.queue_ready);
    k.puts("/");
    k.putDec(handoff.queue_occupied);
    k.puts("/");
    k.putDec(handoff.queue_high_water);
    k.puts(" busy=");
    k.putDec(handoff.queue_busy);
    k.puts(" batch=");
    k.putDec(handoff.batch_max);
    k.puts("/");
    k.putDec(handoff.budget_exhaustions);
    k.puts(" wake=");
    k.putDec(handoff.schedules_from_irq);
    k.puts("/");
    k.putDec(handoff.event_wakeups);
    k.puts("/");
    k.putDec(handoff.fallback_timeouts);
    k.puts(" latency-ns=");
    k.putDec(handoff.schedule_latency_last_ns);
    k.puts("/");
    k.putDec(handoff.schedule_latency_max_ns);
    k.puts("+");
    k.putDec(handoff.handoff_latency_last_ns);
    k.puts("/");
    k.putDec(handoff.handoff_latency_max_ns);
    k.puts(" irq-inline=");
    k.putDec(handoff.irq_frame_rejects);
    k.puts(" balanced=");
    k.puts(if (handoff.ownership_balanced) "yes" else "no");
    k.puts(" offload=");
    k.putDec(handoff.l4_offload_used);
    k.puts("/");
    k.putDec(handoff.metadata_selected);
    k.puts("/");
    k.putDec(handoff.metadata_fallbacks);
    k.puts(" software=");
    k.putDec(handoff.l4_software_checks);
    k.puts(" polls=");
    k.putDec(handoff.polls);
    k.puts(" skips=");
    k.putDec(handoff.guard_skips);
    const t = tcp.getStats();
    k.puts(" tcp_rx=");
    k.putDec(t.rx_segments);
    k.puts(" synack=");
    k.putDec(t.synack_tx);
    k.puts(" acc=");
    k.putDec(t.accepts);
    k.puts(" tcp_sig=");
    k.putDec(tcp_activity_signals);
    k.puts(" rto_srtt=");
    k.putDec(tcp_rto_last_ticks);
    k.puts("/");
    k.putDec(tcp_rto_samples);
    k.puts(" pro_retx=");
    k.putDec(tcp_proactive_retransmits);
    if (comptime kernel_config.enable_net_loss_test) {
        k.puts(" loss_drops=");
        k.putDec(net_loss_drops);
    }
    if (adapter_count > 0) {
        if (refreshAdapterRuntime(0)) |bs| {
            k.puts(" nic_rx=");
            k.putDec(bs.rx_packets);
            // 0.56.7: Fehlerklasse aufgeschluesselt statt Sammelzaehler
            // (rxe=RER/PUN, txe=TUN/TER/Timeout, ovw=RXOVW, rec=Recoveries).
            k.puts(" nic_rxe=");
            k.putDec(bs.rx_errors);
            k.puts(" nic_txe=");
            k.putDec(bs.tx_errors);
            k.puts(" nic_ovw=");
            k.putDec(bs.rx_overflows);
            k.puts(" nic_rec=");
            k.putDec(bs.rx_recoveries);
            // 0.56.21: IRQ-Zustellung sichtbar machen (Befund 6.1) -
            // nic_irq>0 beweist, dass die IOAPIC-Route traegt.
            k.puts(" nic_irq=");
            k.putDec(bs.irq_count);
            k.puts("/");
            k.putDec(bs.irq_handled);
            if (bs.offered_capabilities != 0) {
                k.puts(" nic_caps=");
                k.putHex(bs.accepted_capabilities, 16);
                k.puts("/");
                k.putHex(bs.offered_capabilities, 16);
                k.puts(" nic_offload=");
                k.putDec(bs.rx_offload_packets);
                k.puts("/");
                k.putDec(bs.rx_software_fallbacks);
                k.puts("/");
                k.putDec(bs.rx_metadata_errors);
            }
        }
    }
    k.puts("\r\n");
}

fn rememberArpPeer(ip: [4]u8, mac: [6]u8) void {
    if (isZeroIp(ip) or isZeroMac(mac)) return;
    const now = time_core.monotonicTicks();
    storeArpPeer(ip, mac, now);
    arp_stats.cache_ip = ip;
    arp_stats.cache_mac = mac;
    arp_stats.cache_valid = true;
    arp_stats.cache_updates += 1;
    arp_cache_updated_tick = now;
}

fn rememberArpPeerFromStats() void {
    if (!arp_stats.cache_valid or isZeroIp(arp_stats.cache_ip) or isZeroMac(arp_stats.cache_mac)) return;
    const now = time_core.monotonicTicks();
    storeArpPeer(arp_stats.cache_ip, arp_stats.cache_mac, now);
    arp_cache_updated_tick = now;
}

fn storeArpPeer(ip: [4]u8, mac: [6]u8, updated_tick: u64) void {
    var free_index: ?usize = null;
    var oldest_index: usize = 0;
    var oldest_tick: u64 = ~@as(u64, 0);
    var index: usize = 0;
    while (index < ARP_CACHE_ENTRIES) : (index += 1) {
        if (arp_cache_entries[index].valid and ipv4.sameIp(arp_cache_entries[index].ip, ip)) {
            arp_cache_entries[index].mac = mac;
            arp_cache_entries[index].updated_tick = updated_tick;
            return;
        }
        if (!arp_cache_entries[index].valid and free_index == null) free_index = index;
        if (arp_cache_entries[index].updated_tick < oldest_tick) {
            oldest_tick = arp_cache_entries[index].updated_tick;
            oldest_index = index;
        }
    }

    const slot = free_index orelse blk: {
        const chosen = arp_cache_next_slot;
        arp_cache_next_slot = (arp_cache_next_slot + 1) % ARP_CACHE_ENTRIES;
        if (oldest_tick != 0) break :blk oldest_index;
        break :blk chosen;
    };
    arp_cache_entries[slot] = .{
        .valid = true,
        .ip = ip,
        .mac = mac,
        .updated_tick = updated_tick,
        .hits = 0,
    };
}

fn lookupArpPeer(ip: [4]u8, out_mac: *[6]u8) bool {
    expireArpCacheIfNeeded();
    var index: usize = 0;
    while (index < ARP_CACHE_ENTRIES) : (index += 1) {
        if (!arp_cache_entries[index].valid) continue;
        if (!ipv4.sameIp(arp_cache_entries[index].ip, ip)) continue;
        arp_cache_entries[index].hits += 1;
        out_mac.* = arp_cache_entries[index].mac;
        syncArpPrimaryFromEntry(arp_cache_entries[index]);
        return true;
    }
    return false;
}

fn syncArpPrimaryFromEntry(entry: ArpCacheEntry) void {
    arp_stats.cache_valid = entry.valid;
    arp_stats.cache_ip = entry.ip;
    arp_stats.cache_mac = entry.mac;
    arp_cache_updated_tick = entry.updated_tick;
}

fn syncArpPrimaryFromCache() void {
    var newest_index: ?usize = null;
    var newest_tick: u64 = 0;
    var index: usize = 0;
    while (index < ARP_CACHE_ENTRIES) : (index += 1) {
        if (!arp_cache_entries[index].valid) continue;
        if (newest_index == null or arp_cache_entries[index].updated_tick >= newest_tick) {
            newest_index = index;
            newest_tick = arp_cache_entries[index].updated_tick;
        }
    }
    if (newest_index) |entry_index| {
        syncArpPrimaryFromEntry(arp_cache_entries[entry_index]);
    } else {
        arp_stats.cache_valid = false;
        arp_stats.cache_ip = .{0} ** 4;
        arp_stats.cache_mac = .{0} ** 6;
        arp_cache_updated_tick = 0;
    }
}

fn arpCacheEntryCount() u32 {
    expireArpCacheIfNeeded();
    var entry_count: u32 = 0;
    var index: usize = 0;
    while (index < ARP_CACHE_ENTRIES) : (index += 1) {
        if (arp_cache_entries[index].valid) entry_count += 1;
    }
    return entry_count;
}

fn waitForArpResolution(next_hop_ip: [4]u8) bool {
    const deadline = timing.Deadline.start(ARP_RESOLVE_TIMEOUT_TICKS, ARP_RESOLVE_MAX_LOOPS);
    var loops: usize = 0;
    while (!deadline.loopLimitReached(loops)) : (loops += 1) {
        pollAdapters(ARP_RESOLVE_POLL_ROUNDS);
        var mac: [6]u8 = .{0} ** 6;
        if (lookupArpPeer(next_hop_ip, &mac)) return true;

        if (deadline.expiredNow()) return false;
        scheduler.yield();
        interrupts.enable();
        interrupts.waitForInterrupt();
    }
    var mac: [6]u8 = .{0} ** 6;
    return lookupArpPeer(next_hop_ip, &mac);
}

pub fn sendArpRequest(adapter_index: usize, target_ip: [4]u8) TxResult {
    if (adapter_index >= adapter_count) return .no_adapter;
    var frame: [ethernet.MIN_FRAME_SIZE]u8 = .{0} ** ethernet.MIN_FRAME_SIZE;
    const arp_frame = arpBuildRequest(frame[0..], adapters[adapter_index].mac, target_ip) orelse return .too_large;
    return transmit(adapter_index, arp_frame);
}

fn sendArpReplyIfForUs(adapter_index: usize, request_frame: []const u8) void {
    if (adapter_index >= adapter_count) return;
    if (request_frame.len < ethernet.HEADER_SIZE + 28) return;
    if (ethernetFrameType(request_frame) != ethernet.TYPE_ARP) return;

    const arp_pos = ethernet.HEADER_SIZE;
    if (readBe16(request_frame, arp_pos) != 1) return;
    if (readBe16(request_frame, arp_pos + 2) != ethernet.TYPE_IPV4) return;
    if (request_frame[arp_pos + 4] != 6 or request_frame[arp_pos + 5] != 4) return;
    if (readBe16(request_frame, arp_pos + 6) != 1) return;

    var target_ip: [4]u8 = .{0} ** 4;
    copyIpFromBytes(&target_ip, request_frame[arp_pos + 24 .. arp_pos + 28]);
    const local_ip = net_config.localIp();
    if (!ipv4.sameIp(target_ip, local_ip)) return;

    var requester_mac: [6]u8 = .{0} ** 6;
    var requester_ip: [4]u8 = .{0} ** 4;
    copyMacFromBytes(&requester_mac, request_frame[arp_pos + 8 .. arp_pos + 14]);
    copyIpFromBytes(&requester_ip, request_frame[arp_pos + 14 .. arp_pos + 18]);
    if (macEquals(requester_mac, adapters[adapter_index].mac)) return;

    var reply: [ethernet.MIN_FRAME_SIZE]u8 = .{0} ** ethernet.MIN_FRAME_SIZE;
    copyMacBytes(reply[0..6], requester_mac);
    copyMacBytes(reply[6..12], adapters[adapter_index].mac);
    writeBe16(reply[0..], 12, ethernet.TYPE_ARP);

    writeBe16(reply[0..], arp_pos, 1);
    writeBe16(reply[0..], arp_pos + 2, ethernet.TYPE_IPV4);
    reply[arp_pos + 4] = 6;
    reply[arp_pos + 5] = 4;
    writeBe16(reply[0..], arp_pos + 6, 2);
    copyMacBytes(reply[arp_pos + 8 .. arp_pos + 14], adapters[adapter_index].mac);
    copyIpBytes(reply[arp_pos + 14 .. arp_pos + 18], local_ip);
    copyMacBytes(reply[arp_pos + 18 .. arp_pos + 24], requester_mac);
    copyIpBytes(reply[arp_pos + 24 .. arp_pos + 28], requester_ip);

    _ = transmit(adapter_index, reply[0..]);
}

fn learnArpRequestSender(sender_ip: [4]u8, sender_mac: [6]u8, target_ip: [4]u8) void {
    if (!ipv4.sameIp(target_ip, net_config.localIp())) return;
    if (isZeroIp(sender_ip)) return;
    if (adapter_count > 0 and macEquals(sender_mac, adapters[0].mac)) return;
    rememberArpPeer(sender_ip, sender_mac);
}

fn learnIpv4Sender(sender_ip: [4]u8, sender_mac: [6]u8) void {
    if (isZeroIp(sender_ip) or ipv4.sameIp(sender_ip, net_config.localIp())) return;
    if (!sameSubnet(net_config.localIp(), sender_ip, net_config.netmask())) return;
    if (adapter_count > 0 and macEquals(sender_mac, adapters[0].mac)) return;
    rememberArpPeer(sender_ip, sender_mac);
}

pub fn arpTestGateway() TxResult {
    if (adapter_count == 0) return .no_adapter;
    const result = sendArpRequest(0, net_config.gatewayIp());
    if (result == .ok) _ = waitForArpResolution(net_config.gatewayIp());
    return result;
}

pub fn arpFlush() void {
    arp_stats.cache_valid = false;
    arp_stats.cache_ip = .{0} ** 4;
    arp_stats.cache_mac = .{0} ** 6;
    arp_stats.last_error = "flushed";
    arp_cache_updated_tick = 0;
    arp_cache_entries = .{ArpCacheEntry{}} ** ARP_CACHE_ENTRIES;
    arp_cache_next_slot = 0;
}

pub fn dhcpAcquireDefault() TxResult {
    return dhcpAcquireDefaultUntil(null);
}

pub fn dhcpAcquireDefaultUntil(deadline_tick: ?u64) TxResult {
    net_config.enableDhcp();
    if (adapter_count == 0) {
        dhcp_stats.last_error = "no-adapter";
        return .no_adapter;
    }
    return dhcpAcquireUntil(0, deadline_tick);
}

pub fn dhcpRenewDefault() TxResult {
    return dhcpRenewDefaultUntil(null);
}

pub fn dhcpRenewDefaultUntil(deadline_tick: ?u64) TxResult {
    if (adapter_count == 0) {
        dhcp_stats.last_error = "no-adapter";
        return .no_adapter;
    }
    return dhcpRenewUntil(0, deadline_tick);
}

pub fn dhcpReleaseDefault() TxResult {
    if (adapter_count == 0) {
        dhcp_stats.last_error = "no-adapter";
        dhcp_stats.lease.bound = false;
        dhcp_stats.lease_acquired_tick = 0;
        net_config.clearDhcpLease();
        const before_state = dhcp_coordinator.state;
        _ = dhcp_coordinator.cancel(time_core.monotonicTicks(), false);
        logDhcpRuntimeIfChanged(before_state, "released");
        return .no_adapter;
    }
    return dhcpRelease(0);
}

pub fn dhcpAcquire(adapter_index: usize) TxResult {
    return dhcpAcquireUntil(adapter_index, null);
}

pub fn dhcpAcquireUntil(adapter_index: usize, deadline_tick: ?u64) TxResult {
    if (adapter_index >= adapter_count) {
        dhcp_stats.last_error = "no-adapter";
        return .no_adapter;
    }
    if (adapters[adapter_index].link == .down) {
        dhcp_stats.last_error = "link-down";
        return .link_down;
    }
    if (dhcpDeadlineExpired(deadline_tick)) return dhcpDeadlineResult("acquire-timeout");
    if (!beginDhcpOperation("acquire", .acquire)) return .busy;
    const result = dhcpAcquireOperation(adapter_index, deadline_tick);
    finishDhcpOperation(result, true);
    return result;
}

fn dhcpAcquireOperation(adapter_index: usize, deadline_tick: ?u64) TxResult {
    const socket_raw = bindDhcpClientSocket() orelse return .backend_error;
    const socket: u32 = @intCast(socket_raw);
    defer _ = udpClose(socket);

    const xid = nextDhcpXid(adapters[adapter_index].mac);
    dhcp_stats.lease = .{ .xid = xid };
    dhcp_stats.lease_acquired_tick = 0;
    dhcp_stats.last_error = "discover";

    var dhcp_payload: [MAX_PACKET_SIZE]u8 = .{0} ** MAX_PACKET_SIZE;
    const discover = dhcpBuildDiscover(dhcp_payload[0..], xid, adapters[adapter_index].mac) orelse {
        dhcp_stats.last_error = "discover-build";
        return .too_large;
    };
    setDhcpResponseExpectation(.offer, xid, adapters[adapter_index].mac);
    var result = sendDhcpWithOfferRetry(adapter_index, socket, discover, "offer-timeout", deadline_tick);
    if (result != .ok) return result;
    if (dhcp_stats.lease.xid != 0 and dhcp_stats.lease.xid != xid) {
        dhcp_stats.last_error = "xid-mismatch";
        return .backend_error;
    }
    if (isZeroIp(dhcp_stats.lease.offered_ip)) {
        dhcp_stats.last_error = "no-offer-ip";
        return .backend_error;
    }

    dhcp_stats.last_error = "request";
    const request = dhcpBuildRequest(dhcp_payload[0..], xid, adapters[adapter_index].mac, dhcp_stats.lease.offered_ip, dhcp_stats.lease.server_ip) orelse {
        dhcp_stats.last_error = "request-build";
        return .too_large;
    };
    setDhcpResponseExpectation(.ack_or_nak, xid, adapters[adapter_index].mac);
    result = sendDhcpWithAckRetry(adapter_index, socket, request, "ack-timeout", deadline_tick);
    if (result != .ok) return result;
    if (!dhcp_stats.lease.bound or isZeroIp(dhcp_stats.lease.offered_ip)) {
        dhcp_stats.last_error = "not-bound";
        return .backend_error;
    }

    const lease = normalizeDhcpLease(dhcp_stats.lease);
    dhcp_stats.lease = lease;
    markDhcpLeaseBound();
    net_config.applyDhcpLease(lease.offered_ip, lease.netmask, lease.gateway_ip, lease.dns_ip, lease.dns_configured);
    arpFlush();
    dhcp_stats.last_error = "bound";
    return .ok;
}

pub fn dhcpRenew(adapter_index: usize) TxResult {
    return dhcpRenewUntil(adapter_index, null);
}

pub fn dhcpRenewUntil(adapter_index: usize, deadline_tick: ?u64) TxResult {
    if (adapter_index >= adapter_count) {
        dhcp_stats.last_error = "no-adapter";
        return .no_adapter;
    }
    if (!dhcp_stats.lease.bound or isZeroIp(dhcp_stats.lease.offered_ip)) {
        dhcp_stats.last_error = "no-lease";
        return .backend_error;
    }
    if (dhcpDeadlineExpired(deadline_tick)) return dhcpDeadlineResult("renew-timeout");
    if (!beginDhcpOperation("renew", .renew)) return .busy;
    const result = dhcpRenewOperation(adapter_index, deadline_tick);
    finishDhcpOperation(result, true);
    return result;
}

fn dhcpRenewOperation(adapter_index: usize, deadline_tick: ?u64) TxResult {
    const socket_raw = bindDhcpClientSocket() orelse return .backend_error;
    const socket: u32 = @intCast(socket_raw);
    defer _ = udpClose(socket);

    const previous_lease = dhcp_stats.lease;
    const previous_tick = dhcp_stats.lease_acquired_tick;
    const xid = nextDhcpXid(adapters[adapter_index].mac);
    dhcp_stats.lease.xid = xid;
    dhcp_stats.last_error = "renew";

    var dhcp_payload: [MAX_PACKET_SIZE]u8 = .{0} ** MAX_PACKET_SIZE;
    const request = dhcpBuildRequest(dhcp_payload[0..], xid, adapters[adapter_index].mac, previous_lease.offered_ip, previous_lease.server_ip) orelse {
        dhcp_stats.last_error = "renew-build";
        return .too_large;
    };
    setDhcpResponseExpectation(.ack_or_nak, xid, adapters[adapter_index].mac);
    const nak_before = dhcp_stats.nak_rx;
    const result = sendDhcpWithAckRetryFrom(adapter_index, socket, DHCP_ZERO_IP, DHCP_BROADCAST_IP, request, "renew-timeout", deadline_tick);
    if (result != .ok) {
        if (dhcp_stats.nak_rx <= nak_before) {
            dhcp_stats.lease = previous_lease;
            dhcp_stats.lease_acquired_tick = previous_tick;
        }
        return result;
    }
    if (!dhcp_stats.lease.bound) {
        dhcp_stats.lease = previous_lease;
        dhcp_stats.lease_acquired_tick = previous_tick;
        dhcp_stats.last_error = "not-bound";
        return .backend_error;
    }

    const lease = normalizeDhcpLease(dhcp_stats.lease);
    dhcp_stats.lease = lease;
    markDhcpLeaseBound();
    net_config.applyDhcpLease(lease.offered_ip, lease.netmask, lease.gateway_ip, lease.dns_ip, lease.dns_configured);
    dhcp_stats.last_error = "renewed";
    return .ok;
}

pub fn dhcpRelease(adapter_index: usize) TxResult {
    if (adapter_index >= adapter_count) {
        dhcp_stats.last_error = "no-adapter";
        return .no_adapter;
    }
    if (!dhcp_stats.lease.bound or isZeroIp(dhcp_stats.lease.offered_ip)) {
        dhcp_stats.last_error = "no-lease";
        dhcp_stats.lease_acquired_tick = 0;
        net_config.clearDhcpLease();
        const before_state = dhcp_coordinator.state;
        _ = dhcp_coordinator.cancel(time_core.monotonicTicks(), false);
        logDhcpRuntimeIfChanged(before_state, "released");
        return .ok;
    }
    if (!beginDhcpOperation("release", null)) return .busy;
    const result = dhcpReleaseOperation(adapter_index);
    finishDhcpOperation(result, false);
    const before_state = dhcp_coordinator.state;
    _ = dhcp_coordinator.cancel(time_core.monotonicTicks(), false);
    logDhcpRuntimeIfChanged(before_state, "released");
    return result;
}

fn dhcpReleaseOperation(adapter_index: usize) TxResult {
    const socket_raw = bindDhcpClientSocket() orelse return .backend_error;
    const socket: u32 = @intCast(socket_raw);
    defer _ = udpClose(socket);

    const lease = dhcp_stats.lease;
    const xid = nextDhcpXid(adapters[adapter_index].mac);
    var dhcp_payload: [MAX_PACKET_SIZE]u8 = .{0} ** MAX_PACKET_SIZE;
    const release = dhcpBuildRelease(dhcp_payload[0..], xid, adapters[adapter_index].mac, lease.offered_ip, lease.server_ip) orelse {
        dhcp_stats.last_error = "release-build";
        return .too_large;
    };
    const dest_ip = if (isZeroIp(lease.server_ip)) DHCP_BROADCAST_IP else lease.server_ip;
    const result = sendDhcpUdpFrom(adapter_index, socket, lease.offered_ip, dest_ip, release);
    dhcp_stats.lease.bound = false;
    dhcp_stats.lease_acquired_tick = 0;
    net_config.clearDhcpLease();
    arpFlush();
    if (result != .ok) {
        dhcp_stats.release_errors += 1;
        dhcp_stats.last_error = txResultName(result);
        return result;
    }
    dhcp_stats.last_error = "released";
    return .ok;
}

fn bindDhcpClientSocket() ?i32 {
    const socket = udpBind(dhcp.CLIENT_PORT);
    if (socket <= 0) {
        dhcp_stats.last_error = "socket-bind";
        return null;
    }
    return socket;
}

fn sendDhcpUdp(adapter_index: usize, socket: u32, dhcp_payload: []const u8) TxResult {
    return sendDhcpUdpFrom(adapter_index, socket, DHCP_ZERO_IP, DHCP_BROADCAST_IP, dhcp_payload);
}

fn sendDhcpWithOfferRetry(adapter_index: usize, socket: u32, dhcp_payload: []const u8, timeout_label: []const u8, deadline_tick: ?u64) TxResult {
    var attempt: u8 = 1;
    while (attempt <= DHCP_MAX_ATTEMPTS) : (attempt += 1) {
        const wait_ticks = dhcpWaitBudget(deadline_tick, dhcpAttemptTimeout(attempt));
        if (wait_ticks == 0) return dhcpDeadlineResult(timeout_label);
        if (!dhcpOperationCanContinue(adapter_index)) return dhcpCancelledResult(adapter_index);
        dhcp_stats.last_attempt = attempt;
        const offer_target = dhcp_stats.offer_rx + 1;
        const result = sendDhcpUdp(adapter_index, socket, dhcp_payload);
        if (result != .ok) {
            dhcp_stats.last_error = txResultName(result);
            return result;
        }
        if (waitForDhcpProgress(adapter_index, socket, offer_target, dhcp_stats.ack_rx, wait_ticks)) return .ok;
        if (!dhcpOperationCanContinue(adapter_index)) return dhcpCancelledResult(adapter_index);
        recordDhcpRetry(timeout_label, attempt);
    }
    dhcp_stats.last_error = timeout_label;
    return .backend_error;
}

fn sendDhcpWithAckRetry(adapter_index: usize, socket: u32, dhcp_payload: []const u8, timeout_label: []const u8, deadline_tick: ?u64) TxResult {
    return sendDhcpWithAckRetryFrom(adapter_index, socket, DHCP_ZERO_IP, DHCP_BROADCAST_IP, dhcp_payload, timeout_label, deadline_tick);
}

fn sendDhcpWithAckRetryFrom(adapter_index: usize, socket: u32, source_ip: [4]u8, dest_ip: [4]u8, dhcp_payload: []const u8, timeout_label: []const u8, deadline_tick: ?u64) TxResult {
    var attempt: u8 = 1;
    while (attempt <= DHCP_MAX_ATTEMPTS) : (attempt += 1) {
        const wait_ticks = dhcpWaitBudget(deadline_tick, dhcpAttemptTimeout(attempt));
        if (wait_ticks == 0) return dhcpDeadlineResult(timeout_label);
        if (!dhcpOperationCanContinue(adapter_index)) return dhcpCancelledResult(adapter_index);
        dhcp_stats.last_attempt = attempt;
        const ack_target = dhcp_stats.ack_rx + 1;
        const nak_before = dhcp_stats.nak_rx;
        const result = sendDhcpUdpFrom(adapter_index, socket, source_ip, dest_ip, dhcp_payload);
        if (result != .ok) {
            dhcp_stats.last_error = txResultName(result);
            return result;
        }
        if (waitForDhcpAckOrNak(adapter_index, socket, ack_target, nak_before + 1, wait_ticks)) {
            if (dhcp_stats.nak_rx > nak_before) {
                dhcp_stats.lease.bound = false;
                net_config.clearDhcpLease();
                arpFlush();
                dhcp_stats.last_error = "nak";
                return .backend_error;
            }
            return .ok;
        }
        if (!dhcpOperationCanContinue(adapter_index)) return dhcpCancelledResult(adapter_index);
        recordDhcpRetry(timeout_label, attempt);
    }
    dhcp_stats.last_error = timeout_label;
    return .backend_error;
}

fn sendDhcpUdpFrom(adapter_index: usize, socket: u32, source_ip: [4]u8, dest_ip: [4]u8, dhcp_payload: []const u8) TxResult {
    return udpSendDhcpFromAdapter(socket, adapter_index, source_ip, dest_ip, dhcp_payload);
}

fn dhcpAttemptTimeout(attempt: u8) u64 {
    return DHCP_TIMEOUT_TICKS * @as(u64, attempt);
}

fn dhcpDeadlineExpired(deadline_tick: ?u64) bool {
    const deadline = deadline_tick orelse return false;
    return time_core.monotonicTicks() >= deadline;
}

fn dhcpWaitBudget(deadline_tick: ?u64, attempt_ticks: u64) u64 {
    const deadline = deadline_tick orelse return attempt_ticks;
    const now = time_core.monotonicTicks();
    if (now >= deadline) return 0;
    return @min(attempt_ticks, deadline - now);
}

fn dhcpDeadlineResult(label: []const u8) TxResult {
    dhcp_stats.timeouts += 1;
    dhcp_stats.last_error = label;
    return .backend_error;
}

fn recordDhcpRetry(timeout_label: []const u8, attempt: u8) void {
    dhcp_stats.timeouts += 1;
    if (attempt < DHCP_MAX_ATTEMPTS) dhcp_stats.retries += 1;
    dhcp_stats.last_error = timeout_label;
}

fn dhcpOperationCanContinue(adapter_index: usize) bool {
    if (!dhcp_stats.operation_pending or adapter_index >= adapter_count) return false;
    _ = refreshAdapterRuntime(adapter_index);
    return dhcp_stats.operation_pending and adapters[adapter_index].link != .down;
}

fn dhcpCancelledResult(adapter_index: usize) TxResult {
    if (adapter_index >= adapter_count) {
        dhcp_stats.last_error = "adapter-gone";
        return .no_adapter;
    }
    if (adapters[adapter_index].link == .down) {
        dhcp_stats.last_error = "link-down";
        return .link_down;
    }
    dhcp_stats.last_error = "cancelled";
    return .backend_error;
}

fn waitForDhcpProgress(adapter_index: usize, socket: u32, offer_target: u64, ack_target: u64, timeout_ticks: u64) bool {
    const deadline = timing.Deadline.start(timeout_ticks, DHCP_MAX_LOOPS);
    var loops: usize = 0;
    while (!deadline.loopLimitReached(loops)) : (loops += 1) {
        if (!dhcpOperationCanContinue(adapter_index)) return false;
        pollAdapters(DHCP_POLL_ROUNDS);
        drainDhcpSocket(socket);
        if (dhcp_stats.offer_rx >= offer_target and dhcp_stats.ack_rx >= ack_target) return true;

        if (deadline.expiredNow()) return false;
        scheduler.yield();
        interrupts.enable();
        interrupts.waitForInterrupt();
    }
    return dhcp_stats.offer_rx >= offer_target and dhcp_stats.ack_rx >= ack_target;
}

fn waitForDhcpAckOrNak(adapter_index: usize, socket: u32, ack_target: u64, nak_target: u64, timeout_ticks: u64) bool {
    const deadline = timing.Deadline.start(timeout_ticks, DHCP_MAX_LOOPS);
    var loops: usize = 0;
    while (!deadline.loopLimitReached(loops)) : (loops += 1) {
        if (!dhcpOperationCanContinue(adapter_index)) return false;
        pollAdapters(DHCP_POLL_ROUNDS);
        drainDhcpSocket(socket);
        if (dhcp_stats.ack_rx >= ack_target or dhcp_stats.nak_rx >= nak_target) return true;

        if (deadline.expiredNow()) return false;
        scheduler.yield();
        interrupts.enable();
        interrupts.waitForInterrupt();
    }
    return dhcp_stats.ack_rx >= ack_target or dhcp_stats.nak_rx >= nak_target;
}

fn drainDhcpSocket(socket: u32) void {
    var info: UdpRecvInfo = .{};
    var payload: [UDP_SOCKET_PAYLOAD_MAX]u8 = .{0} ** UDP_SOCKET_PAYLOAD_MAX;
    while (true) {
        const got = udpRecvFrom(socket, &info, payload[0..]);
        if (got < 0) {
            dhcp_stats.last_error = "socket-recv";
            return;
        }
        if (got == 0) return;
        if (info.source_port == dhcp.SERVER_PORT and info.dest_port == dhcp.CLIENT_PORT) {
            _ = dhcpHandleMessage(payload[0..@intCast(got)]);
        }
    }
}

fn normalizeDhcpLease(lease_in: dhcp.Lease) dhcp.Lease {
    var lease = lease_in;
    if (isZeroIp(lease.netmask)) lease.netmask = .{ 255, 255, 255, 0 };
    if (isZeroIp(lease.gateway_ip)) lease.gateway_ip = lease.server_ip;
    if (!lease.dns_configured and !isZeroIp(lease.server_ip)) {
        lease.dns_ip = lease.server_ip;
        lease.dns_configured = true;
    }
    if (lease.lease_seconds > 0) {
        if (lease.renew_seconds == 0) lease.renew_seconds = lease.lease_seconds / 2;
        if (lease.rebind_seconds == 0) lease.rebind_seconds = (lease.lease_seconds * 7) / 8;
    }
    return lease;
}

fn beginDhcpOperation(label: []const u8, operation: ?dhcp_runtime.Operation) bool {
    if (!dhcp_operation_lock.lock(sync.WAIT_FOREVER)) {
        dhcp_stats.last_error = "operation-lock";
        return false;
    }
    const before_state = dhcp_coordinator.state;
    if (operation) |kind| {
        if (!dhcp_coordinator.startOperation(time_core.monotonicTicks(), kind)) {
            _ = dhcp_operation_lock.unlock();
            dhcp_stats.last_error = "operation-busy";
            return false;
        }
    }
    dhcp_stats.operation_pending = true;
    clearDhcpResponseExpectation();
    dhcp_stats.pending_label = label;
    logDhcpRuntimeIfChanged(before_state, label);
    return true;
}

fn finishDhcpOperation(result: TxResult, coordinator_operation: bool) void {
    clearDhcpResponseExpectation();
    dhcp_stats.operation_pending = false;
    dhcp_stats.pending_label = "idle";
    if (coordinator_operation) {
        const before_state = dhcp_coordinator.state;
        dhcp_coordinator.finishOperation(
            time_core.monotonicTicks(),
            result == .ok,
            memContains(dhcp_stats.last_error, "timeout"),
            DHCP_RETRY_BASE_TICKS,
            DHCP_RETRY_MAX_TICKS,
        );
        logDhcpRuntimeIfChanged(before_state, dhcp_stats.last_error);
    }
    _ = dhcp_operation_lock.unlock();
}

fn markDhcpLeaseBound() void {
    dhcp_stats.lease_acquired_tick = time_core.monotonicTicks();
}

fn elapsedSecondsSince(start_tick: u64) u32 {
    return timing.elapsedSecondsSince(start_tick);
}

fn secondsRemaining(total: u32, elapsed: u32) u32 {
    return timing.secondsRemaining(total, elapsed);
}

fn nextDhcpXid(mac: [6]u8) u32 {
    var xid: u32 = @truncate(time_core.monotonicTicks());
    xid ^= (@as(u32, mac[0]) << 24) | (@as(u32, mac[1]) << 16) | (@as(u32, mac[2]) << 8) | @as(u32, mac[3]);
    xid ^= (@as(u32, mac[4]) << 8) | @as(u32, mac[5]);
    if (xid == 0) return 0x52444843;
    return xid;
}

pub fn sendIpv4Payload(target_ip: [4]u8, protocol: u8, payload: []const u8) TxResult {
    if (adapter_count == 0) return .no_adapter;
    if (payload.len > MAX_PACKET_SIZE) return rejectIpv4TxTooLarge(protocol, target_ip, "tx-size");
    if (!ipv4.payloadFitsMtu(adapters[0].mtu, payload.len)) return rejectIpv4TxTooLarge(protocol, target_ip, "tx-mtu");
    var dest_mac: [6]u8 = .{0} ** 6;
    const route_result = resolveIpv4DestMac(target_ip, &dest_mac);
    if (route_result != .ok) return route_result;

    var frame: [MAX_PACKET_SIZE]u8 = .{0} ** MAX_PACKET_SIZE;
    const ip_frame = ipv4BuildPacket(frame[0..], adapters[0].mac, dest_mac, target_ip, protocol, payload) orelse return rejectIpv4TxTooLarge(protocol, target_ip, "tx-build");
    const result = transmit(0, ip_frame);
    if (result == .ok) pollAdapters(TCP_POLL_ROUNDS); // 0.56.23: 32768-Exzess entfernt
    return result;
}

pub fn dnsResolveA(name: []const u8, out: *[4]u8) i32 {
    dns_stats.resolve_requests += 1;
    out.* = .{0} ** 4;
    if (name.len == 0 or name.len > 255) {
        dns_stats.last_result = dns.RESULT_NAME;
        dns_stats.last_error = "name";
        return dns.RESULT_NAME;
    }
    if (!net_config.dnsConfigured() or isZeroIp(net_config.dnsIp())) {
        dns_stats.last_result = dns.RESULT_NO_SERVER;
        dns_stats.last_error = "no-server";
        return dns.RESULT_NO_SERVER;
    }

    return dnsResolveAWithServerChecked(name, net_config.dnsIp(), out);
}

pub fn dnsResolveAWithServer(name: []const u8, server: [4]u8, out: *[4]u8) i32 {
    dns_stats.resolve_requests += 1;
    out.* = .{0} ** 4;
    if (name.len == 0 or name.len > 255) {
        dns_stats.last_result = dns.RESULT_NAME;
        dns_stats.last_error = "name";
        return dns.RESULT_NAME;
    }
    if (isZeroIp(server)) {
        dns_stats.last_result = dns.RESULT_NO_SERVER;
        dns_stats.last_error = "no-server";
        return dns.RESULT_NO_SERVER;
    }
    return dnsResolveAWithServerChecked(name, server, out);
}

fn dnsResolveAWithServerChecked(name: []const u8, server: [4]u8, out: *[4]u8) i32 {
    const id = nextDnsId(name);
    dns_stats.last_server = server;
    copyFixedBytes(dns_stats.last_name[0..], name);
    if (dnsCacheLookup(name, server, out)) |cached_result| {
        dns_stats.last_result = cached_result;
        dns_stats.last_error = if (cached_result == dns.RESULT_OK) "cache" else "cache-nxdomain";
        dns_stats.last_answer = if (cached_result == dns.RESULT_OK) out.* else .{0} ** 4;
        return cached_result;
    }

    beginDnsOperation(name);
    defer endDnsOperation();

    var query_buf: [512]u8 = .{0} ** 512;
    const query = dnsBuildAQuery(query_buf[0..], id, name) orelse {
        dns_stats.last_result = dns.RESULT_NAME;
        dns_stats.last_error = "query-build";
        return dns.RESULT_NAME;
    };

    const source_port = dnsSourcePort(id);
    const handle_raw = udpBind(source_port);
    if (handle_raw <= 0) {
        dns_stats.tx_errors += 1;
        dns_stats.last_result = dns.RESULT_TX;
        dns_stats.last_error = "udp-bind";
        return dns.RESULT_TX;
    }
    const handle: u32 = @intCast(handle_raw);
    defer _ = udpClose(handle);

    const tx = udpSendTo(handle, server, dns.PORT, query);
    if (tx != .ok) {
        dns_stats.tx_errors += 1;
        dns_stats.last_result = dns.RESULT_TX;
        dns_stats.last_error = txResultName(tx);
        return dns.RESULT_TX;
    }

    var info: UdpRecvInfo = .{};
    var response: [512]u8 = .{0} ** 512;
    const deadline = timing.Deadline.start(DNS_TIMEOUT_TICKS, DHCP_MAX_LOOPS);
    var loops: usize = 0;
    while (!deadline.loopLimitReached(loops) and !deadline.expiredNow()) : (loops += 1) {
        pollAdapters(DHCP_POLL_ROUNDS);
        const got = udpRecvFrom(handle, &info, response[0..]);
        if (got < 0) {
            dns_stats.last_result = dns.RESULT_TX;
            dns_stats.last_error = "udp-recv";
            return dns.RESULT_TX;
        }
        if (got > 0 and info.source_port == dns.PORT and ipv4.sameIp(info.source_ip, server)) {
            _ = dnsHandleResponse(response[0..@intCast(got)]);
            if (dns_stats.last_id == id and dns_stats.last_result != 1) {
                if (dns_stats.last_result == dns.RESULT_OK) {
                    out.* = dns_stats.last_answer;
                    dnsCacheStore(name, server, out.*, dns.RESULT_OK);
                    return dns.RESULT_OK;
                }
                if (dns_stats.last_result == dns.RESULT_NXDOMAIN) {
                    dnsCacheStore(name, server, .{0} ** 4, dns.RESULT_NXDOMAIN);
                    return dns.RESULT_NXDOMAIN;
                }
                return dns_stats.last_result;
            }
        }
        scheduler.yield();
    }

    return recordDnsTimeout(name, server);
}

fn recordDnsTimeout(name: []const u8, server: [4]u8) i32 {
    dns_stats.timeouts += 1;
    dns_stats.last_server = server;
    copyFixedBytes(dns_stats.last_name[0..], name);
    dns_stats.last_result = dns.RESULT_TIMEOUT;
    dns_stats.last_error = "timeout";
    return dns.RESULT_TIMEOUT;
}

fn beginDnsOperation(name: []const u8) void {
    dns_stats.operation_pending = true;
    copyFixedBytes(dns_stats.pending_name[0..], name);
}

fn endDnsOperation() void {
    dns_stats.operation_pending = false;
    @memset(dns_stats.pending_name[0..], 0);
}

fn dnsCacheLookup(name: []const u8, server: [4]u8, out: *[4]u8) ?i32 {
    expireDnsCacheEntries();
    const index = dnsCacheFind(name, server) orelse return null;
    var entry = &dns_cache_entries[index];
    out.* = entry.answer;
    entry.hits += 1;
    dns_stats.cache_hits += 1;
    syncDnsPrimaryCacheFromEntry(entry.*);
    return entry.result;
}

fn dnsCacheStore(name: []const u8, server: [4]u8, answer: [4]u8, result: i32) void {
    const index = dnsCacheFind(name, server) orelse dnsCacheReplacementSlot();
    const ttl = if (result == dns.RESULT_NXDOMAIN) DNS_NEGATIVE_CACHE_TTL_SECONDS else DNS_DEFAULT_CACHE_TTL_SECONDS;
    dns_cache_entries[index] = .{
        .valid = true,
        .negative = result == dns.RESULT_NXDOMAIN,
        .server = server,
        .answer = answer,
        .result = result,
        .updated_tick = time_core.monotonicTicks(),
        .ttl_seconds = ttl,
    };
    copyFixedBytes(dns_cache_entries[index].name[0..], name);
    dns_stats.cache_stores += 1;
    syncDnsPrimaryCacheFromEntry(dns_cache_entries[index]);
}

fn dnsCacheFind(name: []const u8, server: [4]u8) ?usize {
    var index: usize = 0;
    while (index < dns_cache_entries.len) : (index += 1) {
        const entry = &dns_cache_entries[index];
        if (!entry.valid) continue;
        if (!dnsCacheEntryFresh(entry.*)) {
            entry.* = .{};
            continue;
        }
        if (!ipv4.sameIp(entry.server, server)) continue;
        if (!fixedTextEqualsIgnoreCase(entry.name[0..], name)) continue;
        return index;
    }
    return null;
}

fn dnsCacheReplacementSlot() usize {
    var index: usize = 0;
    while (index < dns_cache_entries.len) : (index += 1) {
        if (!dns_cache_entries[index].valid or !dnsCacheEntryFresh(dns_cache_entries[index])) {
            dns_cache_entries[index] = .{};
            dns_cache_next_slot = (index + 1) % dns_cache_entries.len;
            return index;
        }
    }
    const slot = dns_cache_next_slot;
    dns_cache_next_slot = (dns_cache_next_slot + 1) % dns_cache_entries.len;
    return slot;
}

fn expireDnsCacheEntries() void {
    var changed = false;
    var index: usize = 0;
    while (index < dns_cache_entries.len) : (index += 1) {
        if (dns_cache_entries[index].valid and !dnsCacheEntryFresh(dns_cache_entries[index])) {
            dns_cache_entries[index] = .{};
            changed = true;
        }
    }
    if (changed) syncDnsPrimaryCache();
}

fn dnsCacheEntryFresh(entry: DnsCacheEntry) bool {
    if (!entry.valid or entry.updated_tick == 0 or entry.ttl_seconds == 0) return false;
    return elapsedSecondsSince(entry.updated_tick) < entry.ttl_seconds;
}

fn dnsPrimaryCacheIndex() ?usize {
    var best_index: ?usize = null;
    var best_tick: u64 = 0;
    var index: usize = 0;
    while (index < dns_cache_entries.len) : (index += 1) {
        const entry = dns_cache_entries[index];
        if (!entry.valid or !dnsCacheEntryFresh(entry)) continue;
        if (best_index == null or entry.updated_tick >= best_tick) {
            best_index = index;
            best_tick = entry.updated_tick;
        }
    }
    return best_index;
}

fn syncDnsPrimaryCache() void {
    if (dnsPrimaryCacheIndex()) |index| {
        syncDnsPrimaryCacheFromEntry(dns_cache_entries[index]);
        return;
    }
    dns_stats.cache_valid = false;
    @memset(dns_stats.cache_name[0..], 0);
    dns_stats.cache_server = .{0} ** 4;
    dns_stats.cache_answer = .{0} ** 4;
    dns_stats.cache_updated_tick = 0;
    dns_stats.cache_ttl_seconds = DNS_DEFAULT_CACHE_TTL_SECONDS;
}

fn syncDnsPrimaryCacheFromEntry(entry: DnsCacheEntry) void {
    dns_stats.cache_valid = entry.valid;
    dns_stats.cache_name = entry.name;
    dns_stats.cache_server = entry.server;
    dns_stats.cache_answer = entry.answer;
    dns_stats.cache_updated_tick = entry.updated_tick;
    dns_stats.cache_ttl_seconds = entry.ttl_seconds;
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

fn nextDnsId(name: []const u8) u16 {
    var value: u32 = @truncate(time_core.monotonicTicks());
    for (name) |ch| value = (value *% 33) ^ upperAscii(ch);
    value ^= value >> 16;
    const id: u16 = @truncate(value);
    return if (id == 0) 0x5244 else id;
}

fn dnsSourcePort(id: u16) u16 {
    return DNS_SOURCE_PORT_BASE + (id & 0x3FFF);
}

pub fn icmpEchoGateway() TxResult {
    return icmpEchoTarget(net_config.gatewayIp());
}

pub fn icmpEchoTarget(target_ip: [4]u8) TxResult {
    if (adapter_count == 0) return .no_adapter;
    var dest_mac: [6]u8 = .{0} ** 6;
    const route_result = resolveIpv4DestMac(target_ip, &dest_mac);
    if (route_result != .ok) return route_result;

    var payload: [32]u8 = .{0} ** 32;
    const echo = icmpBuildEchoRequest(payload[0..], 0x1234, icmp_sequence) orelse return .too_large;
    icmp_sequence +%= 1;

    var frame: [ethernet.MIN_FRAME_SIZE]u8 = .{0} ** ethernet.MIN_FRAME_SIZE;
    const ip_frame = ipv4BuildPacket(frame[0..], adapters[0].mac, dest_mac, target_ip, icmp.IPV4_PROTOCOL, echo) orelse return .too_large;
    const result = transmit(0, ip_frame);
    if (result == .ok) pollAdapters(TCP_POLL_ROUNDS); // 0.56.23: 32768-Exzess entfernt
    return result;
}

pub fn tcpConnect(remote_ip: [4]u8, port: u16) i32 {
    return tcpConnectUntil(remote_ip, port, null);
}

pub fn tcpConnectUntil(remote_ip: [4]u8, port: u16, request_deadline_tick: ?u64) i32 {
    if (adapter_count == 0) {
        tcp.setError("no-adapter");
        return r4p_contract.TCP_RESULT_NO_CONNECTION;
    }
    if (request_deadline_tick) |deadline| {
        if (time_core.monotonicTicks() >= deadline) {
            tcp.markTimeout("connect-timeout");
            return r4p_contract.TCP_RESULT_BAD_STATE;
        }
    }
    const conn = tcp.beginLiveConnect(remote_ip, port, nextTcpSeq(remote_ip, port));
    if (conn <= 0) return conn;
    const conn_id: u32 = @intCast(conn);
    const syn = sendTcpForConnection(conn_id, tcp.FLAG_SYN, "") orelse {
        abortTcpConnection(conn_id, "syn-tx");
        return r4p_contract.TCP_RESULT_BAD_STATE;
    };
    if (syn != .ok) {
        abortTcpConnection(conn_id, txResultName(syn));
        return r4p_contract.TCP_RESULT_BAD_STATE;
    }
    var retransmits: u8 = 0;
    while (true) {
        const now = time_core.monotonicTicks();
        if (request_deadline_tick) |deadline| if (now >= deadline) break;
        const attempt_deadline = now +| rtoBackoff(conn_id, retransmits);
        const wait_deadline = if (request_deadline_tick) |deadline| @min(deadline, attempt_deadline) else attempt_deadline;
        if (waitForTcpEstablishedUntil(conn_id, wait_deadline)) break;
        if (request_deadline_tick) |deadline| if (time_core.monotonicTicks() >= deadline) break;
        if (retransmits >= TCP_MAX_RETRANSMITS) break;
        const retry = retransmitTcpConnection(conn_id, .syn, "syn-retry");
        if (retry != .ok) break;
        retransmits += 1;
    }
    if (!tcp.established(conn_id)) {
        tcp.markTimeout("connect-timeout");
        abortTcpConnection(conn_id, "connect-timeout");
        return r4p_contract.TCP_RESULT_BAD_STATE;
    }
    _ = sendTcpForConnection(conn_id, tcp.FLAG_ACK, "");
    return conn;
}

pub fn tcpWrite(conn_id: u32, data: []const u8) i32 {
    if (!tcp.established(conn_id) or data.len > MAX_PACKET_SIZE - tcp.HEADER_SIZE) return r4p_contract.TCP_RESULT_BAD_STATE;
    const allowed = tcp.sendAllowance(conn_id, data.len);
    if (allowed == 0) return 0;
    const result = sendTcpForConnection(conn_id, tcp.FLAG_ACK | tcp.FLAG_PSH, data[0..allowed]) orelse return r4p_contract.TCP_RESULT_NO_CONNECTION;
    if (result == .busy) return 0;
    if (result != .ok) return r4p_contract.TCP_RESULT_BAD_STATE;
    return @intCast(allowed);
}

pub fn tcpRead(conn_id: u32, out: []u8) i32 {
    pollAdapters(TCP_POLL_ROUNDS);
    const immediate = tcp.read(conn_id, out);
    if (immediate > 0) {
        _ = sendTcpForConnection(conn_id, tcp.FLAG_ACK, "");
        return immediate;
    }
    if (immediate != 0) return immediate;
    // Ein leerer Read wartet nur auf Gegenrichtungsdaten. Retransmits sind
    // ausschliesslich Eigentum des unbestaetigten Sendekatalogs/RTO-Sweeps.
    if (waitForTcpRead(conn_id, out, SERVICE_OPERATION_TIMEOUT_TICKS)) |got| return got;
    tcp.markTimeout("read-timeout");
    return 0;
}

pub fn tcpReadAvailable(conn_id: u32, out: []u8) i32 {
    if (out.len == 0) return 0;
    pollAdapters(TCP_POLL_ROUNDS);
    var immediate = tcp.read(conn_id, out);
    if (immediate == 0) {
        pollAdapters(TCP_POLL_ROUNDS);
        immediate = tcp.read(conn_id, out);
    }
    if (immediate > 0) {
        _ = sendTcpForConnection(conn_id, tcp.FLAG_ACK, "");
    }
    return immediate;
}

pub fn tcpListen(port: u16) bool {
    return tcp.listen(port);
}

pub fn tcpHasListener(port: u16) bool {
    return tcp.hasListener(port);
}

pub fn tcpPollService() void {
    pollAdapters(TCP_POLL_ROUNDS);
}

pub fn tcpRetransmitLast(conn_id: u32) i32 {
    const result = retransmitTcpData(conn_id);
    return if (result == .ok) 0 else r4p_contract.TCP_RESULT_BAD_STATE;
}

pub fn tcpCloseListener(port: u16) void {
    tcp.closeListener(port);
}

pub fn tcpEchoListenOnce(port: u16, out: []u8) i32 {
    if (port == 0 or out.len == 0) return r4p_contract.TCP_RESULT_BAD_STATE;
    if (count() == 0) {
        tcp.markTimeout("listen-no-adapter");
        return r4p_contract.TCP_RESULT_BAD_STATE;
    }
    if (!tcp.listen(port)) return r4p_contract.TCP_RESULT_BAD_STATE;

    const deadline = timing.Deadline.start(TCP_LISTEN_TIMEOUT_TICKS, 0);
    var loops: usize = 0;
    while (!deadline.loopLimitReached(loops)) : (loops += 1) {
        pollAdapters(TCP_POLL_ROUNDS);
        if (tcp.connectionWithDataOnPort(port)) |conn_id| {
            const got = tcp.read(conn_id, out);
            if (got > 0) {
                const len: usize = @intCast(got);
                _ = sendTcpForConnection(conn_id, tcp.FLAG_ACK, "");
                _ = sendTcpForConnection(conn_id, tcp.FLAG_ACK | tcp.FLAG_PSH, out[0..len]);
                _ = tcpClose(conn_id);
                tcp.closeListener(port);
                return got;
            }
        }
        if (deadline.expiredNow()) break;
        var wait_ctx = TcpWaitCtx{ .port = port };
        _ = tcp_activity.waitUnless(TCP_WAIT_SLICE_TICKS, "tcp-accept", predStillWaitPortData, &wait_ctx);
    }

    tcp.closeListener(port);
    tcp.abortConnectionsOnPort(port, "listen-timeout");
    tcp.markTimeout("listen-timeout");
    return 0;
}

pub fn tcpAcceptReadOnce(port: u16, out: []u8, conn_id_out: *u32) i32 {
    conn_id_out.* = 0;
    if (port == 0 or out.len == 0) return r4p_contract.TCP_RESULT_BAD_STATE;
    if (count() == 0) {
        tcp.markTimeout("listen-no-adapter");
        return r4p_contract.TCP_RESULT_BAD_STATE;
    }
    if (!tcp.listen(port)) return r4p_contract.TCP_RESULT_BAD_STATE;

    const deadline = timing.Deadline.start(TCP_LISTEN_TIMEOUT_TICKS, 0);
    var loops: usize = 0;
    while (!deadline.loopLimitReached(loops)) : (loops += 1) {
        pollAdapters(TCP_POLL_ROUNDS);
        if (tcp.connectionWithDataOnPort(port)) |conn_id| {
            const got = tcp.read(conn_id, out);
            if (got > 0) {
                conn_id_out.* = conn_id;
                _ = tcp.claimAccepted(conn_id);
                _ = sendTcpForConnection(conn_id, tcp.FLAG_ACK, "");
                tcp.closeListener(port);
                return got;
            }
        }
        if (deadline.expiredNow()) break;
        var wait_ctx = TcpWaitCtx{ .port = port };
        _ = tcp_activity.waitUnless(TCP_WAIT_SLICE_TICKS, "tcp-accept", predStillWaitPortData, &wait_ctx);
    }

    tcp.closeListener(port);
    tcp.abortConnectionsOnPort(port, "listen-timeout");
    tcp.markTimeout("listen-timeout");
    return 0;
}

pub fn tcpAcceptReadOnListener(port: u16, out: []u8, conn_id_out: *u32) i32 {
    conn_id_out.* = 0;
    if (port == 0 or out.len == 0) return r4p_contract.TCP_RESULT_BAD_STATE;
    if (count() == 0) {
        tcp.markTimeout("listen-no-adapter");
        return r4p_contract.TCP_RESULT_BAD_STATE;
    }
    if (!tcp.hasListener(port)) {
        tcp.markTimeout("listen-missing");
        return r4p_contract.TCP_RESULT_BAD_STATE;
    }

    const deadline = timing.Deadline.start(TCP_LISTEN_TIMEOUT_TICKS, 0);
    var loops: usize = 0;
    while (!deadline.loopLimitReached(loops)) : (loops += 1) {
        pollAdapters(TCP_POLL_ROUNDS);
        if (tcp.connectionWithDataOnPort(port)) |conn_id| {
            const got = tcp.read(conn_id, out);
            if (got > 0) {
                conn_id_out.* = conn_id;
                _ = tcp.claimAccepted(conn_id);
                _ = sendTcpForConnection(conn_id, tcp.FLAG_ACK, "");
                return got;
            }
        }
        if (deadline.expiredNow()) break;
        var wait_ctx = TcpWaitCtx{ .port = port };
        _ = tcp_activity.waitUnless(TCP_WAIT_SLICE_TICKS, "tcp-accept", predStillWaitPortData, &wait_ctx);
    }

    tcp.markTimeout("accept-timeout");
    return 0;
}

pub fn tcpAcceptOnListener(port: u16, conn_id_out: *u32) i32 {
    conn_id_out.* = 0;
    if (port == 0) return r4p_contract.TCP_RESULT_BAD_STATE;
    if (count() == 0) {
        tcp.markTimeout("listen-no-adapter");
        return r4p_contract.TCP_RESULT_BAD_STATE;
    }
    if (!tcp.hasListener(port)) {
        tcp.markTimeout("listen-missing");
        return r4p_contract.TCP_RESULT_BAD_STATE;
    }

    const deadline = timing.Deadline.start(TCP_LISTEN_TIMEOUT_TICKS, 0);
    var loops: usize = 0;
    while (!deadline.loopLimitReached(loops)) : (loops += 1) {
        pollAdapters(TCP_POLL_ROUNDS);
        if (tcp.connectionOnPort(port)) |conn_id| {
            conn_id_out.* = conn_id;
            _ = tcp.claimAccepted(conn_id);
            _ = sendTcpForConnection(conn_id, tcp.FLAG_ACK, "");
            return 1;
        }
        if (deadline.expiredNow()) break;
        var wait_ctx = TcpWaitCtx{ .port = port };
        _ = tcp_activity.waitUnless(TCP_WAIT_SLICE_TICKS, "tcp-accept", predStillWaitPortConn, &wait_ctx);
    }

    tcp.markTimeout("accept-timeout");
    return 0;
}

pub fn tcpAcceptPollOnListener(port: u16, conn_id_out: *u32) i32 {
    conn_id_out.* = 0;
    if (port == 0) return r4p_contract.TCP_RESULT_BAD_STATE;
    if (!tcp.hasListener(port)) {
        tcp.setError("listen-missing");
        return r4p_contract.TCP_RESULT_BAD_STATE;
    }
    if (count() == 0) {
        tcp.setError("accept-poll-empty");
        return 0;
    }

    pollAdapters(TCP_POLL_ROUNDS);
    if (tcp.connectionOnPort(port)) |conn_id| {
        conn_id_out.* = conn_id;
        _ = tcp.claimAccepted(conn_id);
        _ = sendTcpForConnection(conn_id, tcp.FLAG_ACK, "");
        return 1;
    }

    tcp.setError("accept-poll-empty");
    return 0;
}

pub fn tcpClose(conn_id: u32) i32 {
    const closing_identity = tcp.connectionIdentity(conn_id);
    // Nach Peer-FIN ist die Gegenseite bereits fertig. Den verbliebenen
    // Slot samt RTO-Besitz koennen wir unmittelbar freigeben.
    if (tcp.closed(conn_id)) {
        _ = sendTcpForConnection(conn_id, tcp.FLAG_ACK, "");
        const result = tcp.close(conn_id);
        if (closing_identity) |identity| tcp_rto.release(identity);
        return result;
    }

    // R4NET.tcp_close ist laut Plattformvertrag nicht blockierend. Der
    // Verbindungszustand behaelt deshalb ausstehende Nutzdaten und FIN im
    // Kernelbesitz, waehrend der Aufruferhandle sofort ungueltig werden
    // darf. Der net-rx-Task sendet FIN erst nach dem letzten Daten-ACK und
    // uebernimmt Retransmit sowie begrenztes Reaping.
    if (!tcp.requestClose(conn_id)) {
        abortTcpConnection(conn_id, "close-state");
        return r4p_contract.TCP_RESULT_BAD_STATE;
    }
    _ = drivePendingTcpClose(conn_id);
    return r4p_contract.TCP_RESULT_OK;
}

pub fn tcpAbort(conn_id: u32) i32 {
    var info: tcp.ConnectionInfo = .{};
    var found = false;
    var index: u32 = 0;
    while (index < tcp.MAX_CONNECTIONS) : (index += 1) {
        if (tcp.connectionInfo(index, &info) <= 0) continue;
        if (info.id != conn_id) continue;
        found = true;
        break;
    }
    if (!found) return r4p_contract.TCP_RESULT_NO_CONNECTION;
    if (sendTcpForConnection(conn_id, tcp.FLAG_ACK | tcp.FLAG_RST, "")) |_| {}
    abortTcpConnection(conn_id, "abort");
    return r4p_contract.TCP_RESULT_OK;
}

fn abortTcpConnection(conn_id: u32, reason: []const u8) void {
    const identity = tcp.connectionIdentity(conn_id);
    tcp.abort(conn_id, reason);
    if (identity) |bound| tcp_rto.release(bound);
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

fn sendTcpForConnection(conn_id: u32, flags: u16, payload: []const u8) ?TxResult {
    // Der Katalogplatz wird vor dem Drahtzugriff reservierbar geprueft. So
    // kann kein erfolgreich gesendetes Segment ohne Retransmitbesitz enden.
    if (!tcp.canTrackSend(conn_id, flags, payload.len)) return .busy;
    const plan = tcp.planSend(conn_id) orelse return null;
    var result = sendTcpFromPlan(plan, flags, payload);
    // 0.56.7: .busy = TX-Ring voll (alle 4 Slots in Flight, z.B. kurzer
    // SLIRP-Stau). NUR Payload-Segmente kurz mit yield nachsetzen:
    // commitSent ist noch nicht gelaufen, die Wiederholung sendet exakt
    // denselben Plan. Ohne dieses Nachsetzen reichte tcpWrite BAD_STATE
    // nach oben und FTPSVC verlor seine 220-Begruessung. BEWUSST NICHT
    // fuer SYN/SYN-ACK/FIN: die laufen u.a. im net-rx-Task (RX-Handler
    // sendet SYN-ACK) - eine Yield-Schleife dort stoppt den RX-Drain
    // genau unter Last (Gate-Befund: busy-Retry inkl. SYN machte SSH
    // wieder schlechter). SYN hat tcpConnect-Retry, FIN hat
    // retransmitTcpFin, verlorene SYN-ACKs retransmittiert der Client.
    if (result == .busy and payload.len != 0) {
        var attempts: usize = 0;
        while (result == .busy and attempts < 64) : (attempts += 1) {
            scheduler.yield();
            result = sendTcpFromPlan(plan, flags, payload);
        }
    }
    if (result == .ok) {
        const sent_tick = time_core.monotonicTicks();
        if (!tcp.commitSent(conn_id, flags, payload, sent_tick)) return .backend_error;
        if (tcp_runtime.needsTracking(flags, payload.len)) rtoStampSend(conn_id);
    }
    return result;
}

fn sendTcpFromPlan(plan: tcp.SendPlan, flags: u16, payload: []const u8) TxResult {
    var segment_buf: [MAX_PACKET_SIZE]u8 = .{0} ** MAX_PACKET_SIZE;
    const segment = tcpBuildSegment(segment_buf[0..], net_config.localIp(), plan.remote_ip, plan.local_port, plan.remote_port, plan.seq, plan.ack, flags, payload, plan.rx_window) orelse return .too_large;
    return sendIpv4Payload(plan.remote_ip, tcp.IPV4_PROTOCOL, segment);
}

fn retransmitTcpConnection(conn_id: u32, kind: tcp.RetransmitKind, reason: []const u8) TxResult {
    const plan = tcp.planRetransmit(conn_id, kind) orelse return .backend_error;
    var segment_buf: [MAX_PACKET_SIZE]u8 = .{0} ** MAX_PACKET_SIZE;
    const segment = tcpBuildSegment(segment_buf[0..], net_config.localIp(), plan.remote_ip, plan.local_port, plan.remote_port, plan.seq, plan.ack, plan.flags, plan.payload, plan.rx_window) orelse return .too_large;
    const result = sendIpv4Payload(plan.remote_ip, tcp.IPV4_PROTOCOL, segment);
    if (result == .ok) {
        const sent_tick = time_core.monotonicTicks();
        if (!tcp.recordRetransmit(conn_id, plan.token, sent_tick, reason)) return .backend_error;
        rtoRefreshFromCatalog(conn_id);
    }
    return result;
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
var tcp_proactive_retransmits: u64 = 0;
var tcp_rto: tcp_runtime.RtoTable = .{};
var tcp_rto_samples: u64 = 0;
var tcp_rto_last_ticks: u64 = 0;

fn rtoForConn(conn_id: u32) u64 {
    const identity = tcp.connectionIdentity(conn_id) orelse return TCP_RTO_MAX_TICKS;
    const st = tcp_rto.bind(identity) orelse return TCP_RTO_MAX_TICKS;
    if (st.srtt_ticks == 0) return TCP_RTO_MAX_TICKS;
    const rto = st.srtt_ticks * 2;
    if (rto < TCP_RTO_MIN_TICKS) return TCP_RTO_MIN_TICKS;
    if (rto > TCP_RTO_MAX_TICKS) return TCP_RTO_MAX_TICKS;
    return rto;
}

// Backoff pro Retransmit-Runde (rto * 2^n, gedeckelt).
fn rtoBackoff(conn_id: u32, retransmits: u8) u64 {
    var t = rtoForConn(conn_id);
    var i: u8 = 0;
    while (i < retransmits) : (i += 1) {
        t *= 2;
        if (t >= TCP_RTO_MAX_TICKS) return TCP_RTO_MAX_TICKS;
    }
    return t;
}

// Der net-rx-Task retransmittiert das aelteste unbestaetigte Daten- oder
// FIN-Segment. SYN bleibt Eigentum der synchronen Connect-Schleife; FIN
// gehoert nach dem nicht blockierenden tcpClose dem Kernelzustand.
const PROACTIVE_MAX_RESEND: u8 = 5;

fn proactiveRetransmitSweep(now: u64) void {
    var slot: usize = 0;
    while (slot < tcp.MAX_CONNECTIONS) : (slot += 1) {
        const identity = tcp.connectionIdentityAt(slot) orelse {
            tcp_rto.releaseSlot(slot);
            continue;
        };
        const kind: tcp.RetransmitKind = if (tcp.finWaiting(identity.connection_id)) .fin else if (tcp.established(identity.connection_id)) .data else continue;
        const outstanding = tcp.outstandingInfo(identity.connection_id, kind) orelse continue;
        if (outstanding.retransmits >= PROACTIVE_MAX_RESEND) continue;
        const st = tcp_rto.bind(identity) orelse continue;
        if (st.sent_tick == 0 or st.sent_tick != outstanding.sent_tick) {
            st.sent_tick = outstanding.sent_tick;
            st.sent_tx_ack = tcp.txAckOf(identity.connection_id);
        }
        const rto = rtoBackoff(identity.connection_id, outstanding.retransmits);
        if (!tcp_runtime.deadlineReached(now, st.sent_tick +% rto)) continue;
        if (tcp.txAckOf(identity.connection_id) != st.sent_tx_ack) continue;
        const retransmit = switch (kind) {
            .data => retransmitTcpData(identity.connection_id),
            .fin => retransmitTcpFin(identity.connection_id),
            else => .backend_error,
        };
        if (retransmit != .ok) continue;
        tcp_proactive_retransmits +%= 1;
    }
}

fn drivePendingTcpClose(conn_id: u32) bool {
    if (!tcp.closeReady(conn_id)) return false;
    const result = sendTcpForConnection(conn_id, tcp.FLAG_ACK | tcp.FLAG_FIN, "") orelse return false;
    return result == .ok;
}

fn proactiveCloseSweep() void {
    var slot: usize = 0;
    while (slot < tcp.MAX_CONNECTIONS) : (slot += 1) {
        const identity = tcp.connectionIdentityAt(slot) orelse continue;
        _ = drivePendingTcpClose(identity.connection_id);
    }
}

fn rtoStampSend(conn_id: u32) void {
    const identity = tcp.connectionIdentity(conn_id) orelse return;
    const st = tcp_rto.bind(identity) orelse return;
    if (st.sent_tick != 0) return; // Messung laeuft schon
    const outstanding = tcp.outstandingInfo(conn_id, .any) orelse return;
    st.sent_tick = outstanding.sent_tick;
    st.sent_tx_ack = tcp.txAckOf(conn_id);
}

fn rtoRefreshFromCatalog(conn_id: u32) void {
    const identity = tcp.connectionIdentity(conn_id) orelse return;
    const st = tcp_rto.bind(identity) orelse return;
    if (tcp.outstandingInfo(conn_id, .any)) |outstanding| {
        st.sent_tick = outstanding.sent_tick;
        st.sent_tx_ack = tcp.txAckOf(conn_id);
    } else {
        st.sent_tick = 0;
        st.sent_tx_ack = tcp.txAckOf(conn_id);
    }
}

fn rtoOnAck(identity: tcp.ConnectionIdentity) void {
    if (!tcp.identityActive(identity)) {
        tcp_rto.release(identity);
        return;
    }
    const st = tcp_rto.bind(identity) orelse return;
    if (st.sent_tick == 0) return;
    const now_ack = tcp.txAckOf(identity.connection_id);
    if (now_ack == st.sent_tx_ack) return; // kein Fortschritt
    const now = time_core.monotonicTicks();
    const sample = if (now > st.sent_tick) now - st.sent_tick else 1;
    if (st.srtt_ticks == 0) {
        st.srtt_ticks = sample;
    } else {
        st.srtt_ticks = (st.srtt_ticks * 7 + sample) / 8;
    }
    tcp_rto_samples +%= 1;
    tcp_rto_last_ticks = st.srtt_ticks;
    if (tcp.outstandingInfo(identity.connection_id, .any)) |outstanding| {
        st.sent_tick = outstanding.sent_tick;
        st.sent_tx_ack = now_ack;
    } else {
        st.sent_tick = 0;
        st.sent_tx_ack = now_ack;
    }
}

var tcp_activity: sync.WaitQueue = sync.WaitQueue.init();
var tcp_activity_signals: u64 = 0;

fn signalTcpActivity() void {
    tcp_activity_signals +%= 1;
    _ = tcp_activity.wakeAll();
}

const TcpWaitCtx = struct {
    conn_id: u32 = 0,
    port: u16 = 0,
};

fn predStillWaitEstablished(raw: *anyopaque) bool {
    const c: *TcpWaitCtx = @ptrCast(@alignCast(raw));
    return !tcp.established(c.conn_id) and !tcp.closed(c.conn_id);
}

fn predStillWaitReadable(raw: *anyopaque) bool {
    const c: *TcpWaitCtx = @ptrCast(@alignCast(raw));
    return !tcp.readable(c.conn_id);
}

fn predStillWaitClosed(raw: *anyopaque) bool {
    const c: *TcpWaitCtx = @ptrCast(@alignCast(raw));
    return !tcp.closed(c.conn_id);
}

fn predStillWaitPortData(raw: *anyopaque) bool {
    const c: *TcpWaitCtx = @ptrCast(@alignCast(raw));
    return tcp.connectionWithDataOnPort(c.port) == null;
}

fn predStillWaitPortConn(raw: *anyopaque) bool {
    const c: *TcpWaitCtx = @ptrCast(@alignCast(raw));
    return tcp.connectionOnPort(c.port) == null;
}

fn waitForTcpEstablishedUntil(conn_id: u32, deadline_tick: u64) bool {
    pollAdapters(TCP_POLL_ROUNDS);
    var ctx = TcpWaitCtx{ .conn_id = conn_id };
    while (true) {
        if (tcp.established(conn_id)) return true;
        if (tcp.closed(conn_id)) return false;
        const now = time_core.monotonicTicks();
        if (now >= deadline_tick) return tcp.established(conn_id);
        _ = tcp_activity.waitUnless(@min(TCP_WAIT_SLICE_TICKS, deadline_tick - now), "tcp-est", predStillWaitEstablished, &ctx);
    }
}

fn waitForTcpRead(conn_id: u32, out: []u8, timeout_ticks: u64) ?i32 {
    pollAdapters(TCP_POLL_ROUNDS);
    const deadline = timing.Deadline.start(timeout_ticks, 1);
    var ctx = TcpWaitCtx{ .conn_id = conn_id };
    while (true) {
        const got = tcp.read(conn_id, out);
        if (got > 0) {
            _ = sendTcpForConnection(conn_id, tcp.FLAG_ACK, "");
            return got;
        }
        if (got != 0) return got;
        if (deadline.expiredNow()) return null;
        _ = tcp_activity.waitUnless(TCP_WAIT_SLICE_TICKS, "tcp-read", predStillWaitReadable, &ctx);
    }
}

fn waitForTcpClosed(conn_id: u32, timeout_ticks: u64) bool {
    pollAdapters(TCP_POLL_ROUNDS);
    const deadline = timing.Deadline.start(timeout_ticks, 1);
    var ctx = TcpWaitCtx{ .conn_id = conn_id };
    while (true) {
        if (tcp.closed(conn_id)) return true;
        if (deadline.expiredNow()) return false;
        _ = tcp_activity.waitUnless(TCP_WAIT_SLICE_TICKS, "tcp-close", predStillWaitClosed, &ctx);
    }
}

fn nextTcpSeq(remote_ip: [4]u8, port: u16) u32 {
    var value: u32 = @truncate(time_core.monotonicTicks());
    value ^= (@as(u32, remote_ip[0]) << 24) | (@as(u32, remote_ip[1]) << 16) | (@as(u32, remote_ip[2]) << 8) | remote_ip[3];
    value ^= @as(u32, port) << 8;
    if (value == 0) return 0x52444350;
    return value;
}

fn resolveIpv4DestMac(target_ip: [4]u8, out_mac: *[6]u8) TxResult {
    const route = routeIpv4Target(target_ip);
    if (route.result != .ok) {
        arp_stats.last_error = route.last_error;
        return route.result;
    }
    if (!route.needs_arp) {
        out_mac.* = route.dest_mac;
        arp_stats.last_error = route.last_error;
        return .ok;
    }
    if (lookupArpPeer(route.next_hop_ip, out_mac)) {
        arp_stats.cache_hits += 1;
        arp_stats.last_error = "cache-hit";
        return .ok;
    }

    arp_stats.pending_packets += 1;
    arp_stats.pending_queue_limit = ARP_PENDING_QUEUE_LIMIT;
    var attempt: usize = 0;
    while (attempt < ARP_RESOLVE_ATTEMPTS) : (attempt += 1) {
        arp_stats.resolve_attempts += 1;
        const arp_result = sendArpRequest(0, route.next_hop_ip);
        if (arp_result != .ok) {
            arp_stats.pending_drops += 1;
            return arp_result;
        }
        if (waitForArpResolution(route.next_hop_ip)) {
            if (lookupArpPeer(route.next_hop_ip, out_mac)) return .ok;
            arp_stats.resolve_misses += 1;
            arp_stats.pending_drops += 1;
            arp_stats.last_error = "resolve-miss";
            return .backend_error;
        }
        arp_stats.resolve_timeouts += 1;
        if (attempt + 1 < ARP_RESOLVE_ATTEMPTS) {
            arp_stats.resolve_retries += 1;
            arp_stats.last_error = "resolve-retry";
        }
    }
    arp_stats.last_error = "resolve-timeout";
    arp_stats.pending_timeouts += 1;
    arp_stats.pending_drops += 1;
    return .backend_error;
}

fn routeIpv4Target(target_ip: [4]u8) Ipv4RouteDecision {
    if (isZeroIp(target_ip)) {
        return .{ .result = .backend_error, .last_error = "route-zero" };
    }
    if (isLimitedBroadcastIp(target_ip) or isSubnetBroadcastIp(target_ip)) {
        return .{
            .next_hop_ip = target_ip,
            .dest_mac = DHCP_BROADCAST_MAC,
            .needs_arp = false,
            .last_error = "route-broadcast",
        };
    }
    if (isMulticastIp(target_ip)) {
        return .{
            .next_hop_ip = target_ip,
            .dest_mac = multicastMacForIp(target_ip),
            .needs_arp = false,
            .last_error = "route-multicast",
        };
    }
    if (sameSubnet(net_config.localIp(), target_ip, net_config.netmask())) {
        return .{ .next_hop_ip = target_ip, .last_error = "route-local" };
    }
    const gateway = net_config.gatewayIp();
    if (isZeroIp(gateway)) {
        return .{ .result = .backend_error, .last_error = "route-no-gateway" };
    }
    return .{ .next_hop_ip = gateway, .last_error = "route-gateway" };
}

fn sameSubnet(a: [4]u8, b: [4]u8, mask: [4]u8) bool {
    var index: usize = 0;
    while (index < 4) : (index += 1) {
        if ((a[index] & mask[index]) != (b[index] & mask[index])) return false;
    }
    return true;
}

fn isLimitedBroadcastIp(ip: [4]u8) bool {
    return ip[0] == 255 and ip[1] == 255 and ip[2] == 255 and ip[3] == 255;
}

fn isSubnetBroadcastIp(ip: [4]u8) bool {
    const local = net_config.localIp();
    const mask = net_config.netmask();
    var index: usize = 0;
    while (index < 4) : (index += 1) {
        const expected = (local[index] & mask[index]) | ~mask[index];
        if (ip[index] != expected) return false;
    }
    return true;
}

fn isMulticastIp(ip: [4]u8) bool {
    return ip[0] >= 224 and ip[0] <= 239;
}

fn multicastMacForIp(ip: [4]u8) [6]u8 {
    return .{ 0x01, 0x00, 0x5E, ip[1] & 0x7F, ip[2], ip[3] };
}

fn copyFixedBytes(out: []u8, value: []const u8) void {
    @memset(out, 0);
    const len = if (value.len < out.len - 1) value.len else out.len - 1;
    if (len != 0) @memcpy(out[0..len], value[0..len]);
}

fn fixedTextEqualsIgnoreCase(fixed: []const u8, value: []const u8) bool {
    var len: usize = 0;
    while (len < fixed.len and fixed[len] != 0) : (len += 1) {}
    if (len != value.len) return false;
    var i: usize = 0;
    while (i < len) : (i += 1) {
        if (upperAscii(fixed[i]) != upperAscii(value[i])) return false;
    }
    return true;
}

fn upperAscii(ch: u8) u8 {
    if (ch >= 'a' and ch <= 'z') return ch - ('a' - 'A');
    return ch;
}

fn sendIcmpEchoReply(adapter_index: usize, request_frame: []const u8, request: ipv4.PacketView) void {
    if (adapter_index >= adapter_count or request_frame.len < ethernet.HEADER_SIZE) return;
    var dest_mac: [6]u8 = .{0} ** 6;
    var i: usize = 0;
    while (i < 6) : (i += 1) dest_mac[i] = request_frame[6 + i];

    var payload: [128]u8 = .{0} ** 128;
    const reply_payload = icmpBuildEchoReply(payload[0..], request.payload) orelse return;
    var frame: [256]u8 = .{0} ** 256;
    const reply = ipv4BuildPacket(frame[0..], adapters[adapter_index].mac, dest_mac, request.source_ip, icmp.IPV4_PROTOCOL, reply_payload) orelse return;
    _ = transmit(adapter_index, reply);
}

fn expireArpCacheIfNeeded() void {
    const now = time_core.monotonicTicks();
    var expired_any = false;
    var index: usize = 0;
    while (index < ARP_CACHE_ENTRIES) : (index += 1) {
        if (!arp_cache_entries[index].valid or arp_cache_entries[index].updated_tick == 0) continue;
        if (now >= arp_cache_entries[index].updated_tick and now - arp_cache_entries[index].updated_tick > ARP_CACHE_TTL_TICKS) {
            arp_cache_entries[index].valid = false;
            expired_any = true;
        }
    }
    if (arp_stats.cache_valid and arp_cache_updated_tick != 0 and now >= arp_cache_updated_tick and now - arp_cache_updated_tick > ARP_CACHE_TTL_TICKS) {
        expired_any = true;
    }
    if (expired_any) {
        syncArpPrimaryFromCache();
        if (!arp_stats.cache_valid) arp_stats.last_error = "expired";
    }
}

fn memEql(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var index: usize = 0;
    while (index < a.len) : (index += 1) {
        if (a[index] != b[index]) return false;
    }
    return true;
}

fn memContains(haystack: []const u8, needle: []const u8) bool {
    if (needle.len == 0) return true;
    if (needle.len > haystack.len) return false;
    var start: usize = 0;
    while (start + needle.len <= haystack.len) : (start += 1) {
        if (memEql(haystack[start .. start + needle.len], needle)) return true;
    }
    return false;
}

fn copyIpBytes(dst: []u8, src: [4]u8) void {
    var i: usize = 0;
    while (i < 4) : (i += 1) dst[i] = src[i];
}

fn copyMacBytes(dst: []u8, src: [6]u8) void {
    var i: usize = 0;
    while (i < 6) : (i += 1) dst[i] = src[i];
}

fn copyIpFromBytes(dst: *[4]u8, src: []const u8) void {
    var i: usize = 0;
    while (i < 4) : (i += 1) dst[i] = src[i];
}

fn copyMacFromBytes(dst: *[6]u8, src: []const u8) void {
    var i: usize = 0;
    while (i < 6) : (i += 1) dst[i] = src[i];
}

fn macEquals(a: [6]u8, b: [6]u8) bool {
    var i: usize = 0;
    while (i < 6) : (i += 1) {
        if (a[i] != b[i]) return false;
    }
    return true;
}

fn isZeroIp(ip: [4]u8) bool {
    var i: usize = 0;
    while (i < 4) : (i += 1) {
        if (ip[i] != 0) return false;
    }
    return true;
}

fn isZeroMac(mac: [6]u8) bool {
    var i: usize = 0;
    while (i < 6) : (i += 1) {
        if (mac[i] != 0) return false;
    }
    return true;
}

fn readBe16(buf: []const u8, offset: usize) u16 {
    return (@as(u16, buf[offset]) << 8) | @as(u16, buf[offset + 1]);
}

fn writeBe16(buf: []u8, offset: usize, value: u16) void {
    buf[offset] = @intCast(value >> 8);
    buf[offset + 1] = @intCast(value & 0xFF);
}

fn internetChecksum(data: []const u8) u16 {
    var sum: u32 = 0;
    var i: usize = 0;
    while (i + 1 < data.len) : (i += 2) {
        sum += (@as(u32, data[i]) << 8) | data[i + 1];
    }
    if (i < data.len) sum += @as(u32, data[i]) << 8;
    while ((sum >> 16) != 0) sum = (sum & 0xFFFF) + (sum >> 16);
    return @intCast(~sum & 0xFFFF);
}

fn writeBe32(buf: []u8, offset: usize, value: u32) void {
    buf[offset] = @intCast(value >> 24);
    buf[offset + 1] = @intCast((value >> 16) & 0xFF);
    buf[offset + 2] = @intCast((value >> 8) & 0xFF);
    buf[offset + 3] = @intCast(value & 0xFF);
}
