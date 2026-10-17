//! Original derived network metrics and diagnostic log formatting. Each owner
//! supplies an owned snapshot. No CPU/registry admission spans another owner's
//! waitable operation, a backend status callback or console/bootlog output.
use super::{
    adapter_runtime, adapters, app_ipv4, arp, backend_admission::CallbackGuard, cleanup,
    dhcp_client, dns, dns_client, ethernet, icmp, ipv4, packet_pool, protocol_runtime,
    protocol_stats, rx_runtime, tcp, tcp_rto, tcp_wait, timing, transmit, udp, udp_sockets,
};
use crate::{
    boot::{foundation, log},
    build_config::config,
    platform::pci_inventory,
    sched::sync_types::Text,
    service::{ipc, ipc_types::ChannelInfo},
};
use core::{mem::MaybeUninit, slice};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NativeBackpressureStatus {
    pub packet_pool_used: u32,
    pub packet_pool_limit: u32,
    pub packet_drops: u64,
    pub app_ipv4_queued: u32,
    pub app_ipv4_queue_limit: u32,
    pub app_ipv4_drops: u64,
    pub udp_active_sockets: u32,
    pub udp_socket_limit: u32,
    pub udp_queued_packets: u32,
    pub udp_queue_limit_per_socket: u32,
    pub udp_queue_limit_total: u32,
    pub udp_drops: u64,
    pub udp_payload_max: u32,
    pub tcp_active_connections: u32,
    pub tcp_connection_limit: u32,
    pub tcp_active_listeners: u32,
    pub tcp_listener_limit: u32,
    pub tcp_buffer_size: u32,
    pub tcp_rx_drops: u64,
    pub ipc_service_channels: u32,
    pub ipc_service_handlers: u32,
    pub ipc_service_queued: u32,
    pub ipc_service_queue_limit: u32,
    pub ipc_service_drops: u64,
    pub ipc_service_message_max: u32,
    pub ipc_service_queue_depth: u32,
    pub tx_failures: u64,
    pub tx_no_adapter: u64,
    pub tx_link_down: u64,
    pub tx_busy: u64,
    pub tx_too_large: u64,
    pub tx_unsupported: u64,
    pub tx_backend_error: u64,
    pub tx_last_result: Text,
    pub nonblocking_empty_status: Text,
    pub resource_queue_full: u64,
    pub resource_packet_drops: u64,
    pub resource_buffer_small: u64,
    pub resource_retries: u64,
    pub resource_timeouts: u64,
    pub resource_cancels: u64,
    pub resource_backend_busy: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct NativeErrorStatus {
    pub total: u64,
    pub packet_errors: u64,
    pub service_errors: u64,
    pub adapter_errors: u64,
    pub tx_failures: u64,
    pub protocol_errors: u64,
    pub r4p_dispatch_failures: u64,
    pub last_adapter_error: Text,
    pub last_protocol_error: Text,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct NativeUdpStatus {
    pub active_sockets: u32,
    pub max_sockets: u32,
    pub queued_packets: u32,
    pub queue_limit: u32,
    pub payload_max: u32,
    pub delivered: u64,
    pub drops: u64,
    pub last_error: Text,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct TcpPerformance {
    pub local_mss: u32,
    pub catalog_capacity: u32,
    pub delayed_ack_ms: u32,
    pub local_window_scale: u32,
    pub outstanding_segments: u32,
    pub outstanding_bytes: u32,
    pub outstanding_segments_peak: u32,
    pub outstanding_bytes_peak: u32,
    pub write_calls: u64,
    pub write_requested_bytes: u64,
    pub write_completed_bytes: u64,
    pub write_segments: u64,
    pub write_partial: u64,
    pub remote_window_stalls: u64,
    pub catalog_stalls: u64,
    pub backend_busy_stalls: u64,
    pub pure_ack_tx: u64,
    pub delayed_ack_requests: u64,
    pub delayed_ack_tx: u64,
    pub immediate_ack_tx: u64,
    pub ack_coalesced: u64,
    pub ack_piggybacked: u64,
    pub window_update_tx: u64,
    pub adapter_poll_rounds: u64,
    pub service_poll_requests: u64,
    pub service_poll_skips: u64,
    pub retransmits: u64,
    pub mss_negotiated: u64,
    pub window_scale_negotiated: u64,
}

fn dec(value: impl Into<u64>) {
    foundation::dec(value.into());
}

fn none(value: Text) -> bool {
    value.len == 4 && unsafe { slice::from_raw_parts(value.pointer, value.len) } == b"none"
}
fn boot_text(value: Text) {
    if value.len != 0 {
        unsafe {
            log::r4os_rust_bootlog_write(value.pointer, value.len);
        }
    }
}
fn boot_puts(value: &[u8]) {
    unsafe {
        log::r4os_rust_bootlog_write(value.as_ptr(), value.len());
    }
}
fn count() -> usize {
    adapters::r4os_rust_net_adapter_count()
}
macro_rules! snapshot {
    ($name:ident,$ty:ty,$owner:path $(,$arg:expr)*) => {
        fn $name()->$ty { let mut out=MaybeUninit::uninit();unsafe{$owner(out.as_mut_ptr() $(,$arg)*);out.assume_init()} }
    };
}
snapshot!(
    ethernet_stats,
    ethernet::Stats,
    protocol_stats::r4os_rust_net_ethernet_owned_stats
);
snapshot!(
    arp_stats,
    arp::Stats,
    protocol_stats::r4os_rust_net_arp_owned_stats,
    false
);
snapshot!(
    ipv4_stats,
    ipv4::Stats,
    protocol_stats::r4os_rust_net_ipv4_owned_stats
);
snapshot!(
    icmp_stats,
    icmp::Stats,
    protocol_stats::r4os_rust_net_icmp_owned_stats
);
snapshot!(
    udp_stats,
    udp::Stats,
    protocol_stats::r4os_rust_net_udp_owned_stats
);
snapshot!(
    dns_stats,
    dns::Stats,
    dns_client::r4os_rust_dns_client_stats,
    false
);
snapshot!(tcp_stats, tcp::Stats, tcp::r4os_rust_net_tcp_get_stats);
snapshot!(
    tcp_summary,
    crate::contract::TcpSummary,
    tcp::r4os_rust_net_tcp_summary
);
snapshot!(
    udp_summary,
    udp_sockets::Summary,
    udp_sockets::r4os_rust_udp_socket_summary
);
snapshot!(
    tx_counters,
    transmit::NativeTxCounters,
    transmit::r4os_rust_net_tx_counters
);
snapshot!(
    cleanup_status,
    cleanup::NativeCleanupStatus,
    cleanup::r4os_rust_net_cleanup_status
);
snapshot!(
    r4p_status,
    protocol_runtime::R4pRuntimeStatus,
    protocol_runtime::r4os_rust_net_r4p_runtime
);
snapshot!(
    rx_summary,
    rx_runtime::RxTaskSummary,
    rx_runtime::r4os_rust_net_rx_runtime_summary
);
snapshot!(
    poll_status,
    rx_runtime::RxPollStatus,
    rx_runtime::r4os_rust_net_rx_runtime_poll_status
);
snapshot!(
    rto_status,
    tcp_rto::RtoStatus,
    tcp_rto::r4os_rust_tcp_rto_status
);
struct IpcServiceBackpressure {
    channels: u32,
    handlers: u32,
    queued: u32,
    queue_limit: u32,
    drops: u64,
}
fn ipc_backpressure() -> IpcServiceBackpressure {
    let mut status = IpcServiceBackpressure {
        channels: 0,
        handlers: 0,
        queued: 0,
        queue_limit: 0,
        drops: 0,
    };
    for id in [2, 3, 4, 5] {
        let mut info = ChannelInfo::EMPTY;
        unsafe {
            ipc::r4os_rust_ipc_channel(id, &mut info);
        }
        status.channels += 1;
        status.queue_limit += info.queue_depth;
        status.queued += info.queued;
        status.drops += info.drops;
        if info.has_handler != 0 {
            status.handlers += 1;
        }
    }
    status
}
pub(crate) fn backpressure() -> NativeBackpressureStatus {
    let tx = tx_counters();
    let summary = tcp_summary();
    let ipc = ipc_backpressure();
    let free = packet_pool::r4os_rust_net_packet_free_count();
    let app_drops = app_ipv4::r4os_rust_app_ipv4_drops();
    let udp = udp_summary();
    let packet_drops = packet_pool::r4os_rust_net_packet_drops();
    let arp = arp_stats();
    let dhcp = dhcp_client::stats();
    let dns = dns_stats();
    let cleanup = cleanup_status();
    NativeBackpressureStatus {
        packet_pool_used: u32::try_from(16 - free).expect("Original packet pool bound"),
        packet_pool_limit: 16,
        packet_drops,
        app_ipv4_queued: u32::try_from(app_ipv4::r4os_rust_app_ipv4_queued())
            .expect("Original IPv4 queue bound"),
        app_ipv4_queue_limit: 384,
        app_ipv4_drops: app_drops,
        udp_active_sockets: udp.active,
        udp_socket_limit: 8,
        udp_queued_packets: udp.queued,
        udp_queue_limit_per_socket: 4,
        udp_queue_limit_total: 32,
        udp_drops: udp.drops,
        udp_payload_max: 1472,
        tcp_active_connections: summary.active_connections,
        tcp_connection_limit: summary.max_connections,
        tcp_active_listeners: summary.active_listeners,
        tcp_listener_limit: 4,
        tcp_buffer_size: summary.buffer_size,
        tcp_rx_drops: summary.rx_drops,
        ipc_service_channels: ipc.channels,
        ipc_service_handlers: ipc.handlers,
        ipc_service_queued: ipc.queued,
        ipc_service_queue_limit: ipc.queue_limit,
        ipc_service_drops: ipc.drops,
        ipc_service_message_max: 4096,
        ipc_service_queue_depth: 8,
        tx_failures: tx.failures,
        tx_no_adapter: tx.no_adapter,
        tx_link_down: tx.link_down,
        tx_busy: tx.busy,
        tx_too_large: tx.too_large,
        tx_unsupported: tx.unsupported,
        tx_backend_error: tx.backend_error,
        tx_last_result: Text::literal(if tx.has_result {
            transmit::result_name(tx.last_result)
        } else {
            b"none"
        }),
        nonblocking_empty_status: timing::operation_status_name(6),
        resource_queue_full: app_drops + udp.drops + ipc.drops,
        resource_packet_drops: packet_drops + app_drops + udp.drops + summary.rx_drops,
        resource_buffer_small: protocol_stats::r4os_rust_net_buffer_small_count(),
        resource_retries: arp.resolve_retries + dhcp.retries + summary.retransmits,
        resource_timeouts: arp.resolve_timeouts
            + arp.pending_timeouts
            + dhcp.timeouts
            + dns.timeouts
            + summary.timeouts,
        resource_cancels: cleanup.dhcp_operations_cancelled + cleanup.dns_operations_cancelled,
        resource_backend_busy: tx.busy,
    }
}
fn adapter_errors() -> u64 {
    let Some(_callback) = CallbackGuard::acquire() else {
        return 0;
    };
    let mut total = 0;
    let mut index = 0;
    while index < count() {
        if let Some(adapter) = adapters::snapshot(index) {
            total += adapter.stats.drops + adapter.stats.errors;
            if let Some(callback) = adapter.ops.status {
                let mut status = unsafe { core::mem::zeroed::<adapters::BackendStatus>() };
                status.irq_line = 255;
                if unsafe { callback(adapter.ops.status_context, index, &mut status) } == 0 {
                    total += status.drops + status.errors;
                }
            }
        }
        index += 1;
    }
    total
}
fn last_adapter_error() -> Text {
    let mut index = 0;
    while index < count() {
        if let Some(adapter) = adapters::snapshot(index)
            && !none(adapter.stats.last_error)
        {
            return adapter.stats.last_error;
        }
        index += 1;
    }
    Text::literal(b"none")
}
fn last_protocol_error() -> Text {
    let mut wire = MaybeUninit::uninit();
    unsafe {
        protocol_runtime::r4os_rust_net_tcp_wire_error(wire.as_mut_ptr());
    }
    let wire = unsafe { wire.assume_init() };
    if !none(wire) {
        return wire;
    }
    let label = tcp_stats().last_error;
    if !none(label) {
        return label;
    }
    let label = dns_stats().last_error;
    if !none(label) {
        return label;
    }
    let label = dhcp_client::stats().last_error;
    if !none(label) {
        return label;
    }
    let label = udp_stats().last_error;
    if !none(label) {
        return label;
    }
    let label = icmp_stats().last_error;
    if !none(label) {
        return label;
    }
    let label = ipv4_stats().last_error;
    if !none(label) {
        return label;
    }
    let label = arp_stats().last_error;
    if !none(label) {
        return label;
    }
    let label = ethernet_stats().last_error;
    if !none(label) {
        return label;
    }
    Text::literal(b"none")
}
pub(crate) fn errors() -> NativeErrorStatus {
    let tx = tx_counters();
    let tcp = tcp_summary();
    let ipc = ipc_backpressure();
    let r4p = r4p_status();
    let adapter = adapter_errors();
    let eth = ethernet_stats();
    let arp = arp_stats();
    let ip = ipv4_stats();
    let icmp = icmp_stats();
    let udp = udp_stats();
    let dhcp = dhcp_client::stats();
    let dns = dns_stats();
    let packets = packet_pool::r4os_rust_net_packet_drops()
        + app_ipv4::r4os_rust_app_ipv4_drops()
        + udp_summary().drops
        + tcp.rx_drops
        + eth.dropped_short
        + eth.dropped_filter
        + eth.unknown_ethertype
        + arp.malformed
        + arp.pending_drops
        + ip.dropped_short
        + ip.dropped_version
        + ip.dropped_checksum
        + ip.dropped_fragment
        + ip.dropped_destination
        + ip.dropped_tx_too_large
        + ip.malformed
        + icmp.malformed
        + icmp.checksum_errors
        + udp.dropped_short
        + udp.dropped_length
        + udp.checksum_errors
        + udp.malformed
        + dhcp.malformed
        + dhcp.release_errors
        + dns.malformed
        + dns.tx_errors
        + tcp.checksum_errors;
    let protocols = arp.resolve_timeouts
        + arp.resolve_misses
        + dns.timeouts
        + dns.nxdomain
        + tcp.timeouts
        + tcp.rst_rx;
    NativeErrorStatus {
        total: packets + protocols + ipc.drops + adapter + tx.failures + r4p.dispatch_failures,
        packet_errors: packets,
        service_errors: ipc.drops,
        adapter_errors: adapter,
        tx_failures: tx.failures,
        protocol_errors: protocols,
        r4p_dispatch_failures: r4p.dispatch_failures,
        last_adapter_error: last_adapter_error(),
        last_protocol_error: last_protocol_error(),
    }
}
fn udp_status() -> NativeUdpStatus {
    let value = udp_summary();
    NativeUdpStatus {
        active_sockets: value.active,
        max_sockets: 8,
        queued_packets: value.queued,
        queue_limit: 4,
        payload_max: 1472,
        delivered: value.delivered,
        drops: value.drops,
        last_error: udp_stats().last_error,
    }
}
fn tcp_performance() -> TcpPerformance {
    let poll = poll_status();
    let s = tcp_stats();
    let mut outstanding_segments = 0;
    let mut outstanding_bytes = 0;
    unsafe {
        tcp::r4os_rust_net_tcp_outstanding_totals(
            &mut outstanding_segments,
            &mut outstanding_bytes,
        );
    }
    TcpPerformance {
        local_mss: 1460,
        catalog_capacity: 48,
        delayed_ack_ms: 40,
        local_window_scale: 2,
        outstanding_segments,
        outstanding_bytes,
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
        adapter_poll_rounds: poll.adapter_poll_rounds,
        service_poll_requests: poll.service_poll_requests,
        service_poll_skips: poll.service_poll_skips,
        retransmits: s.retransmits,
        mss_negotiated: s.mss_negotiated,
        window_scale_negotiated: s.window_scale_negotiated,
    }
}
fn log_errors(reason: Text) {
    let s = errors();
    boot_puts(b"[NET][ERR] ");
    boot_text(reason);
    boot_puts(b" total=");
    log::r4os_rust_bootlog_dec(s.total);
    boot_puts(b" packet=");
    log::r4os_rust_bootlog_dec(s.packet_errors);
    boot_puts(b" service=");
    log::r4os_rust_bootlog_dec(s.service_errors);
    boot_puts(b" adapter=");
    log::r4os_rust_bootlog_dec(s.adapter_errors);
    boot_puts(b" tx=");
    log::r4os_rust_bootlog_dec(s.tx_failures);
    boot_puts(b" proto=");
    log::r4os_rust_bootlog_dec(s.protocol_errors);
    boot_puts(b" r4p_dispatch=");
    log::r4os_rust_bootlog_dec(s.r4p_dispatch_failures);
    boot_puts(b" last_adapter=");
    boot_text(s.last_adapter_error);
    boot_puts(b" last_protocol=");
    boot_text(s.last_protocol_error);
    boot_puts(b"\r\n");
}
fn log_rx() {
    let rto_status = rto_status();

    let handoff = rx_summary();
    foundation::puts(b"NETRX handoff=");
    dec(handoff.processed);
    foundation::puts(b"/");
    dec(handoff.accepted);
    foundation::puts(b" queue=");
    dec(handoff.queue_ready);
    foundation::puts(b"/");
    dec(handoff.queue_occupied);
    foundation::puts(b"/");
    dec(handoff.queue_high_water);
    foundation::puts(b" busy=");
    dec(handoff.queue_busy);
    foundation::puts(b" batch=");
    dec(handoff.batch_max);
    foundation::puts(b"/");
    dec(handoff.budget_exhaustions);
    foundation::puts(b" wake=");
    dec(handoff.schedules_from_irq);
    foundation::puts(b"/");
    dec(handoff.event_wakeups);
    foundation::puts(b"/");
    dec(handoff.fallback_timeouts);
    foundation::puts(b" latency-ns=");
    dec(handoff.schedule_latency_last_ns);
    foundation::puts(b"/");
    dec(handoff.schedule_latency_max_ns);
    foundation::puts(b"+");
    dec(handoff.handoff_latency_last_ns);
    foundation::puts(b"/");
    dec(handoff.handoff_latency_max_ns);
    foundation::puts(b" irq-inline=");
    dec(handoff.irq_frame_rejects);
    foundation::puts(b" balanced=");
    foundation::puts(if handoff.ownership_balanced {
        b"yes"
    } else {
        b"no"
    });
    foundation::puts(b" offload=");
    dec(handoff.l4_offload_used);
    foundation::puts(b"/");
    dec(handoff.metadata_selected);
    foundation::puts(b"/");
    dec(handoff.metadata_fallbacks);
    foundation::puts(b" software=");
    dec(handoff.l4_software_checks);
    foundation::puts(b" polls=");
    dec(handoff.polls);
    foundation::puts(b" skips=");
    dec(handoff.guard_skips);
    let t = tcp_stats();
    foundation::puts(b" tcp_rx=");
    dec(t.rx_segments);
    foundation::puts(b" synack=");
    dec(t.synack_tx);
    foundation::puts(b" acc=");
    dec(t.accepts);
    foundation::puts(b" tcp_sig=");
    dec(tcp_wait::activity_count());
    foundation::puts(b" rto_srtt=");
    dec(rto_status.last_ticks);
    foundation::puts(b"/");
    dec(rto_status.samples);
    foundation::puts(b" pro_retx=");
    dec(rto_status.proactive_retransmits);
    if config::ENABLE_NET_LOSS_TEST {
        foundation::puts(b" loss_drops=");
        dec(rx_runtime::r4os_rust_net_rx_runtime_loss_drops());
    }
    if count() > 0
        && let Some(bs) = adapter_runtime::refresh(0, None)
    {
        foundation::puts(b" nic_rx=");
        dec(bs.rx_packets);

        foundation::puts(b" nic_rxe=");
        dec(bs.rx_errors);
        foundation::puts(b" nic_txe=");
        dec(bs.tx_errors);
        foundation::puts(b" nic_ovw=");
        dec(bs.rx_overflows);
        foundation::puts(b" nic_rec=");
        dec(bs.rx_recoveries);

        foundation::puts(b" nic_irq=");
        dec(bs.irq_count);
        foundation::puts(b"/");
        dec(bs.irq_handled);
        if bs.offered_capabilities != 0 {
            foundation::puts(b" nic_caps=");
            unsafe {
                net_console_hex(bs.accepted_capabilities, 16);
            };
            foundation::puts(b"/");
            unsafe {
                net_console_hex(bs.offered_capabilities, 16);
            };
            foundation::puts(b" nic_offload=");
            dec(bs.rx_offload_packets);
            foundation::puts(b"/");
            dec(bs.rx_software_fallbacks);
            foundation::puts(b"/");
            dec(bs.rx_metadata_errors);
        }
    }
    foundation::puts(b"\r\n");
}

#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_telemetry_candidates() -> usize {
    let mut total = 0;
    let mut index = 0;
    while let Some(device) = pci_inventory::device_at(index) {
        if device.class_code == 2 && device.subclass == 0 {
            total += 1;
        }
        index += 1;
    }
    total
}
/// # Safety
/// Exact exclusive paired output, copied after all owner snapshots complete.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_telemetry_backpressure(out: *mut NativeBackpressureStatus) {
    unsafe {
        out.write(backpressure());
    }
}
/// # Safety
/// Exact exclusive paired output, retained through bounded backend status work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_telemetry_errors(out: *mut NativeErrorStatus) {
    let value = errors();
    unsafe {
        out.write(value);
    }
}
/// # Safety
/// Exact exclusive paired output, copied after all owner snapshots complete.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_telemetry_udp(out: *mut NativeUdpStatus) {
    unsafe {
        out.write(udp_status());
    }
}
/// # Safety
/// Exact exclusive paired output, copied after all owner snapshots complete.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_telemetry_tcp_performance(out: *mut TcpPerformance) {
    let value = tcp_performance();
    unsafe {
        out.write(value);
    }
}
/// # Safety
/// Readable retained kernel reason label through synchronous bootlog output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_telemetry_log_errors(reason: Text) {
    log_errors(reason);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_telemetry_log_rx() {
    log_rx();
}

const _: () = {
    assert!(core::mem::size_of::<NativeBackpressureStatus>() == 272);
    assert!(core::mem::align_of::<NativeBackpressureStatus>() == 8);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, packet_pool_used) == 0);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, packet_pool_limit) == 4);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, packet_drops) == 8);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, app_ipv4_queued) == 16);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, app_ipv4_queue_limit) == 20);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, app_ipv4_drops) == 24);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, udp_active_sockets) == 32);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, udp_socket_limit) == 36);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, udp_queued_packets) == 40);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, udp_queue_limit_per_socket) == 44);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, udp_queue_limit_total) == 48);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, udp_drops) == 56);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, udp_payload_max) == 64);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, tcp_active_connections) == 68);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, tcp_connection_limit) == 72);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, tcp_active_listeners) == 76);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, tcp_listener_limit) == 80);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, tcp_buffer_size) == 84);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, tcp_rx_drops) == 88);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, ipc_service_channels) == 96);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, ipc_service_handlers) == 100);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, ipc_service_queued) == 104);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, ipc_service_queue_limit) == 108);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, ipc_service_drops) == 112);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, ipc_service_message_max) == 120);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, ipc_service_queue_depth) == 124);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, tx_failures) == 128);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, tx_no_adapter) == 136);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, tx_link_down) == 144);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, tx_busy) == 152);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, tx_too_large) == 160);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, tx_unsupported) == 168);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, tx_backend_error) == 176);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, tx_last_result) == 184);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, nonblocking_empty_status) == 200);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, resource_queue_full) == 216);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, resource_packet_drops) == 224);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, resource_buffer_small) == 232);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, resource_retries) == 240);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, resource_timeouts) == 248);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, resource_cancels) == 256);
    assert!(core::mem::offset_of!(NativeBackpressureStatus, resource_backend_busy) == 264);
};
const _: () = {
    assert!(core::mem::size_of::<NativeErrorStatus>() == 88);
    assert!(core::mem::align_of::<NativeErrorStatus>() == 8);
    assert!(core::mem::offset_of!(NativeErrorStatus, total) == 0);
    assert!(core::mem::offset_of!(NativeErrorStatus, packet_errors) == 8);
    assert!(core::mem::offset_of!(NativeErrorStatus, service_errors) == 16);
    assert!(core::mem::offset_of!(NativeErrorStatus, adapter_errors) == 24);
    assert!(core::mem::offset_of!(NativeErrorStatus, tx_failures) == 32);
    assert!(core::mem::offset_of!(NativeErrorStatus, protocol_errors) == 40);
    assert!(core::mem::offset_of!(NativeErrorStatus, r4p_dispatch_failures) == 48);
    assert!(core::mem::offset_of!(NativeErrorStatus, last_adapter_error) == 56);
    assert!(core::mem::offset_of!(NativeErrorStatus, last_protocol_error) == 72);
};
const _: () = {
    assert!(core::mem::size_of::<NativeUdpStatus>() == 56);
    assert!(core::mem::align_of::<NativeUdpStatus>() == 8);
    assert!(core::mem::offset_of!(NativeUdpStatus, active_sockets) == 0);
    assert!(core::mem::offset_of!(NativeUdpStatus, max_sockets) == 4);
    assert!(core::mem::offset_of!(NativeUdpStatus, queued_packets) == 8);
    assert!(core::mem::offset_of!(NativeUdpStatus, queue_limit) == 12);
    assert!(core::mem::offset_of!(NativeUdpStatus, payload_max) == 16);
    assert!(core::mem::offset_of!(NativeUdpStatus, delivered) == 24);
    assert!(core::mem::offset_of!(NativeUdpStatus, drops) == 32);
    assert!(core::mem::offset_of!(NativeUdpStatus, last_error) == 40);
};
const _: () = {
    assert!(core::mem::size_of::<TcpPerformance>() == 200);
    assert!(core::mem::align_of::<TcpPerformance>() == 8);
    assert!(core::mem::offset_of!(TcpPerformance, local_mss) == 0);
    assert!(core::mem::offset_of!(TcpPerformance, catalog_capacity) == 4);
    assert!(core::mem::offset_of!(TcpPerformance, delayed_ack_ms) == 8);
    assert!(core::mem::offset_of!(TcpPerformance, local_window_scale) == 12);
    assert!(core::mem::offset_of!(TcpPerformance, outstanding_segments) == 16);
    assert!(core::mem::offset_of!(TcpPerformance, outstanding_bytes) == 20);
    assert!(core::mem::offset_of!(TcpPerformance, outstanding_segments_peak) == 24);
    assert!(core::mem::offset_of!(TcpPerformance, outstanding_bytes_peak) == 28);
    assert!(core::mem::offset_of!(TcpPerformance, write_calls) == 32);
    assert!(core::mem::offset_of!(TcpPerformance, write_requested_bytes) == 40);
    assert!(core::mem::offset_of!(TcpPerformance, write_completed_bytes) == 48);
    assert!(core::mem::offset_of!(TcpPerformance, write_segments) == 56);
    assert!(core::mem::offset_of!(TcpPerformance, write_partial) == 64);
    assert!(core::mem::offset_of!(TcpPerformance, remote_window_stalls) == 72);
    assert!(core::mem::offset_of!(TcpPerformance, catalog_stalls) == 80);
    assert!(core::mem::offset_of!(TcpPerformance, backend_busy_stalls) == 88);
    assert!(core::mem::offset_of!(TcpPerformance, pure_ack_tx) == 96);
    assert!(core::mem::offset_of!(TcpPerformance, delayed_ack_requests) == 104);
    assert!(core::mem::offset_of!(TcpPerformance, delayed_ack_tx) == 112);
    assert!(core::mem::offset_of!(TcpPerformance, immediate_ack_tx) == 120);
    assert!(core::mem::offset_of!(TcpPerformance, ack_coalesced) == 128);
    assert!(core::mem::offset_of!(TcpPerformance, ack_piggybacked) == 136);
    assert!(core::mem::offset_of!(TcpPerformance, window_update_tx) == 144);
    assert!(core::mem::offset_of!(TcpPerformance, adapter_poll_rounds) == 152);
    assert!(core::mem::offset_of!(TcpPerformance, service_poll_requests) == 160);
    assert!(core::mem::offset_of!(TcpPerformance, service_poll_skips) == 168);
    assert!(core::mem::offset_of!(TcpPerformance, retransmits) == 176);
    assert!(core::mem::offset_of!(TcpPerformance, mss_negotiated) == 184);
    assert!(core::mem::offset_of!(TcpPerformance, window_scale_negotiated) == 192);
};

unsafe fn net_console_hex(value: u64, digits: usize) {
    crate::boot::log_route::hex(value, u8::try_from(digits).expect("hex width"));
}
