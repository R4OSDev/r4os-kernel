//! Original R4NET projections into the unchanged platform contract.
use super::network_api::copy_fixed;
use crate::{
    contract::*,
    net::{cleanup, protocol_runtime, telemetry, timing},
};
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
pub const DHCP_STATUS_FLAG_BOUND: u32 = 1 << 0;
pub const DHCP_STATUS_FLAG_DNS_CONFIGURED: u32 = 1 << 1;
pub const DHCP_STATUS_FLAG_PENDING: u32 = 1 << 2;
pub const DHCP_STATUS_FLAG_DESIRED: u32 = 1 << 3;
pub const DHCP_STATUS_FLAG_TASK_STARTED: u32 = 1 << 4;
pub const DHCP_STATUS_FLAG_LINK_UP: u32 = 1 << 5;
pub const DHCP_STATUS_FLAG_RETRY_WAIT: u32 = 1 << 6;
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
pub(super) fn fill_net_diag_timing(out: &mut NetDiagTiming, status: timing::Status) {
    *out = NetDiagTiming {
        ticks: status.ticks,
        hz: status.hz,
        arp_cache_ttl_ticks: status.arp_cache_ttl_ticks,
        arp_resolve_timeout_ticks: status.arp_resolve_timeout_ticks,
        dhcp_timeout_ticks: status.dhcp_timeout_ticks,
        dns_timeout_ticks: status.dns_timeout_ticks,
        tcp_listen_timeout_ticks: status.tcp_listen_timeout_ticks,
        tcp_retransmit_timeout_ticks: status.tcp_retransmit_timeout_ticks,
        tcp_time_wait_ticks: status.tcp_time_wait_ticks,
        service_operation_timeout_ticks: status.service_operation_timeout_ticks,
        operation_status_count: status.operation_status_count,
        ..NetDiagTiming::default()
    };
}
pub(super) fn fill_net_diag_backpressure(
    out: &mut NetDiagBackpressure,
    status: telemetry::NativeBackpressureStatus,
) {
    *out = NetDiagBackpressure {
        packet_pool_used: status.packet_pool_used,
        packet_pool_limit: status.packet_pool_limit,
        app_ipv4_queued: status.app_ipv4_queued,
        app_ipv4_queue_limit: status.app_ipv4_queue_limit,
        udp_active_sockets: status.udp_active_sockets,
        udp_socket_limit: status.udp_socket_limit,
        udp_queued_packets: status.udp_queued_packets,
        udp_queue_limit_total: status.udp_queue_limit_total,
        tcp_active_connections: status.tcp_active_connections,
        tcp_connection_limit: status.tcp_connection_limit,
        tcp_active_listeners: status.tcp_active_listeners,
        tcp_listener_limit: status.tcp_listener_limit,
        tcp_buffer_size: status.tcp_buffer_size,
        ipc_service_channels: status.ipc_service_channels,
        ipc_service_handlers: status.ipc_service_handlers,
        ipc_service_queued: status.ipc_service_queued,
        ipc_service_queue_limit: status.ipc_service_queue_limit,
        ipc_service_message_max: status.ipc_service_message_max,
        ipc_service_queue_depth: status.ipc_service_queue_depth,
        packet_drops: status.packet_drops,
        app_ipv4_drops: status.app_ipv4_drops,
        udp_drops: status.udp_drops,
        tcp_rx_drops: status.tcp_rx_drops,
        ipc_service_drops: status.ipc_service_drops,
        tx_failures: status.tx_failures,
        tx_no_adapter: status.tx_no_adapter,
        tx_link_down: status.tx_link_down,
        tx_busy: status.tx_busy,
        tx_too_large: status.tx_too_large,
        tx_unsupported: status.tx_unsupported,
        tx_backend_error: status.tx_backend_error,
        resource_queue_full: status.resource_queue_full,
        resource_packet_drops: status.resource_packet_drops,
        resource_buffer_small: status.resource_buffer_small,
        resource_retries: status.resource_retries,
        resource_timeouts: status.resource_timeouts,
        resource_cancels: status.resource_cancels,
        resource_backend_busy: status.resource_backend_busy,
        ..NetDiagBackpressure::default()
    };
    copy_fixed(&mut out.tx_last_result, status.tx_last_result);
    copy_fixed(
        &mut out.nonblocking_empty_status,
        status.nonblocking_empty_status,
    );
}
pub(super) fn fill_net_diag_cleanup(
    out: &mut NetDiagCleanup,
    status: cleanup::NativeCleanupStatus,
) {
    *out = NetDiagCleanup {
        runs: status.runs,
        link_down_cleanups: status.link_down_cleanups,
        adapter_reset_cleanups: status.adapter_reset_cleanups,
        adapter_unregister_cleanups: status.adapter_unregister_cleanups,
        service_restart_cleanups: status.service_restart_cleanups,
        poweroff_cleanups: status.poweroff_cleanups,
        reboot_cleanups: status.reboot_cleanups,
        udp_sockets_closed: status.udp_sockets_closed,
        tcp_connections_aborted: status.tcp_connections_aborted,
        tcp_listeners_closed: status.tcp_listeners_closed,
        dhcp_operations_cancelled: status.dhcp_operations_cancelled,
        dns_operations_cancelled: status.dns_operations_cancelled,
        last_udp_closed: status.last_udp_closed,
        last_tcp_connections: status.last_tcp_connections,
        last_tcp_listeners: status.last_tcp_listeners,
        ..NetDiagCleanup::default()
    };
    copy_fixed(&mut out.last_reason, status.last_reason);
}
pub(super) fn fill_net_diag_errors(out: &mut NetDiagErrors, status: telemetry::NativeErrorStatus) {
    *out = NetDiagErrors {
        total: status.total,
        packet_errors: status.packet_errors,
        service_errors: status.service_errors,
        adapter_errors: status.adapter_errors,
        tx_failures: status.tx_failures,
        protocol_errors: status.protocol_errors,
        r4p_dispatch_failures: status.r4p_dispatch_failures,
        ..NetDiagErrors::default()
    };
    copy_fixed(&mut out.last_adapter_error, status.last_adapter_error);
    copy_fixed(&mut out.last_protocol_error, status.last_protocol_error);
}
pub(super) fn fill_net_diag_r4p(out: &mut NetDiagR4p, status: protocol_runtime::R4pRuntimeStatus) {
    *out = NetDiagR4p {
        protocol_count: status.protocol_count,
        active: status.active,
        missing: status.missing,
        r4p_rx: status.r4p_rx,
        r4p_tx: status.r4p_tx,
        r4p_control: status.r4p_control,
        r4p_build: status.r4p_build,
        r4p_classify: status.r4p_classify,
        dispatch_failures: status.dispatch_failures,
        ..NetDiagR4p::default()
    };
}
pub(super) fn fill_net_detail_runtime(
    out: &mut NetDetailProtocolRuntime,
    stats: protocol_runtime::ProtocolRuntimeStats,
) {
    *out = NetDetailProtocolRuntime {
        active_r4p: u8::from(stats.active_r4p),
        r4p_state: stats.r4p_state,
        builtin_fallback: u8::from(stats.builtin_fallback),
        fallback_policy: stats.fallback_policy,
        fallback_decision: stats.fallback_decision,
        r4p_rx: stats.r4p_rx,
        r4p_tx: stats.r4p_tx,
        r4p_control: stats.r4p_control,
        r4p_build: stats.r4p_build,
        r4p_classify: stats.r4p_classify,
        fallbacks: 0,
        dispatch_failures: stats.dispatch_failures,
        ..NetDetailProtocolRuntime::default()
    };
}
pub(super) fn fill_tcp_connection_info(out: &mut TcpConnectionInfo, info: TcpConnectionInfo) {
    *out = TcpConnectionInfo {
        id: info.id,
        state: info.state,
        local_port: info.local_port,
        remote_port: info.remote_port,
        remote_ip: info.remote_ip,
        tx_bytes: info.tx_bytes,
        rx_bytes: info.rx_bytes,
        pending_rx: info.pending_rx,
        retransmits: info.retransmits,
        rx_window: info.rx_window,
        tx_window: info.tx_window,
        tx_ack: info.tx_ack,
        rx_drops: info.rx_drops,
        seq: info.seq,
        ack: info.ack,
        last_seq: info.last_seq,
        last_ack: info.last_ack,
        last_flags: info.last_flags,
        last_payload_len: info.last_payload_len,
    };
}

pub(super) fn project_ipc_performance(
    s: crate::service::ipc_types::PerformanceSummary,
) -> IpcPerformanceSummary {
    IpcPerformanceSummary {
        version: 1,
        size: core::mem::size_of::<IpcPerformanceSummary>() as u32,
        worker_started: s.worker_started,
        worker_task_id: s.worker_task_id,
        active_channels: s.active_channels,
        queue_used: s.queue_used,
        queue_ready: s.queue_ready,
        queue_running: s.queue_running,
        queue_limit: s.queue_limit,
        handler_queued: s.handler_queued,
        handler_completed: s.handler_completed,
        handler_failures: s.handler_failures,
        handler_direct: s.handler_direct,
        handler_waits: s.handler_waits,
        handler_wait_timeouts: s.handler_wait_timeouts,
        handler_queue_ns: s.handler_queue_ns,
        handler_queue_max_ns: s.handler_queue_max_ns,
        handler_run_ns: s.handler_run_ns,
        handler_run_max_ns: s.handler_run_max_ns,
        handler_e2e_ns: s.handler_e2e_ns,
        handler_e2e_max_ns: s.handler_e2e_max_ns,
        request_bytes: s.request_bytes,
        response_bytes: s.response_bytes,
        payload_copy_bytes: s.payload_copy_bytes,
        payload_clear_bytes: s.payload_clear_bytes,
        queue_full: s.queue_full,
        queue_empty: s.queue_empty,
        admission_waits: s.admission_waits,
        admission_timeouts: s.admission_timeouts,
        recv_buffer_small: s.recv_buffer_small,
        response_search_slots: s.response_search_slots,
        stale_drops: s.stale_drops,
        lock_contentions: s.lock_contentions,
        irq_denied: s.irq_denied,
        ..IpcPerformanceSummary::default()
    }
}
pub(super) fn project_tcp_performance(s: telemetry::TcpPerformance) -> TcpPerformanceInfo {
    TcpPerformanceInfo {
        version: 1,
        size: core::mem::size_of::<TcpPerformanceInfo>() as u32,
        local_mss: s.local_mss,
        catalog_capacity: s.catalog_capacity,
        delayed_ack_ms: s.delayed_ack_ms,
        local_window_scale: s.local_window_scale,
        outstanding_segments: s.outstanding_segments,
        outstanding_bytes: s.outstanding_bytes,
        outstanding_segments_peak: s.outstanding_segments_peak,
        outstanding_bytes_peak: s.outstanding_bytes_peak,
        write_calls: s.write_calls,
        write_requested_bytes: s.write_requested_bytes,
        write_completed_bytes: s.write_completed_bytes,
        write_segments: s.write_segments,
        write_partial: s.write_partial,
        remote_window_stalls: s.remote_window_stalls,
        catalog_stalls: s.catalog_stalls,
        backend_busy_stalls: s.backend_busy_stalls,
        pure_ack_tx: s.pure_ack_tx,
        delayed_ack_requests: s.delayed_ack_requests,
        delayed_ack_tx: s.delayed_ack_tx,
        immediate_ack_tx: s.immediate_ack_tx,
        ack_coalesced: s.ack_coalesced,
        ack_piggybacked: s.ack_piggybacked,
        window_update_tx: s.window_update_tx,
        adapter_poll_rounds: s.adapter_poll_rounds,
        service_poll_requests: s.service_poll_requests,
        service_poll_skips: s.service_poll_skips,
        retransmits: s.retransmits,
        mss_negotiated: s.mss_negotiated,
        window_scale_negotiated: s.window_scale_negotiated,
    }
}
