//! Existing network diagnostic bodies, unchanged assertions and case counts.
//! Fixtures retire only their own handles/adapters; no metadata/TCP owner spans
//! packet dispatch, callbacks, waiting, console output or another owner.
use super::{
    adapter_runtime, adapters, app_ipv4, backend_contract as backend, builders, cleanup, config,
    config_writer::Addresses, core_bytes as bytes, dhcp_client, dhcp_messages, dns_client, frames,
    ipv4, packet_io as io, packet_pool, protocol_runtime, protocol_stats as stats, rx_runtime, tcp,
    tcp_client, tcp_wire, telemetry, timing, udp_sockets,
};
use crate::{boot::foundation, sched::sync_types::Text};
use core::{mem::MaybeUninit, ptr, slice};
fn capture<T>(f: unsafe extern "C" fn(*mut T)) -> T {
    let mut out = MaybeUninit::uninit();
    // SAFETY: every selected owner writes the complete paired resident record.
    unsafe {
        f(out.as_mut_ptr());
        out.assume_init()
    }
}
fn equal(text: Text, expected: &[u8]) -> bool {
    // SAFETY: these are copied native owner labels with static kernel lifetime.
    (if text.len == 0 {
        &[][..]
    } else {
        unsafe { slice::from_raw_parts(text.pointer, text.len) }
    }) == expected
}
fn cleanup_status() -> cleanup::NativeCleanupStatus {
    capture(cleanup::r4os_rust_net_cleanup_status)
}
fn udp_status() -> telemetry::NativeUdpStatus {
    capture(telemetry::r4os_rust_net_telemetry_udp)
}
fn summary() -> crate::contract::TcpSummary {
    capture(tcp::r4os_rust_net_tcp_summary)
}
fn tcp_stats() -> tcp::Stats {
    capture(tcp::r4os_rust_net_tcp_get_stats)
}
fn dns_stats() -> super::dns::Stats {
    let mut out = MaybeUninit::uninit();
    unsafe {
        dns_client::r4os_rust_dns_client_stats(out.as_mut_ptr(), false);
        out.assume_init()
    }
}
fn arp_stats() -> super::arp::Stats {
    let mut out = MaybeUninit::uninit();
    unsafe {
        stats::r4os_rust_net_arp_owned_stats(out.as_mut_ptr(), false);
        out.assume_init()
    }
}
fn ethernet_stats() -> super::ethernet::Stats {
    capture(stats::r4os_rust_net_ethernet_owned_stats)
}
fn ipv4_stats() -> ipv4::Stats {
    capture(stats::r4os_rust_net_ipv4_owned_stats)
}
fn icmp_stats() -> super::icmp::Stats {
    capture(stats::r4os_rust_net_icmp_owned_stats)
}
fn udp_stats() -> super::udp::Stats {
    capture(stats::r4os_rust_net_udp_owned_stats)
}
fn pending(reason: &'static [u8], dns_name: &'static [u8]) {
    unsafe {
        dhcp_client::r4os_rust_dhcp_client_set_pending(true, &Text::literal(reason));
        dns_client::r4os_rust_dns_client_set_pending(dns_name.as_ptr(), dns_name.len());
    }
}
fn scope(
    udp_handle: u32,
    tcp_connection_id: u32,
    tcp_listener_port: u16,
) -> cleanup::DiagnosticCleanupScope {
    cleanup::DiagnosticCleanupScope {
        udp_handle,
        tcp_connection_id,
        tcp_listener_port,
    }
}
fn abort(id: u32, reason: &'static [u8]) {
    unsafe {
        tcp::r4os_rust_net_tcp_abort(id, reason.as_ptr(), reason.len());
    }
}
fn tcp_error(reason: &'static [u8]) {
    unsafe {
        protocol_runtime::r4os_rust_net_tcp_wire_error_set(Text::literal(reason));
    }
}
fn listener(port: u16) -> bool {
    unsafe { tcp::r4os_rust_net_tcp_has_listener(port) }
}
fn listen(port: u16) -> bool {
    unsafe { tcp::r4os_rust_net_tcp_listen(port) }
}
fn adapter(index: usize) -> adapters::NativeAdapter {
    adapters::snapshot(index).expect("exact diagnostic adapter")
}
struct DiagnosticAdapter(usize);
impl Drop for DiagnosticAdapter {
    fn drop(&mut self) {
        // SAFETY: fixture alone owns this adapter, with no outstanding callback.
        unsafe {
            adapter_runtime::r4os_rust_net_adapter_runtime_discard(self.0);
        }
    }
}
fn register(name: &'static [u8], driver: &'static [u8], status: bool) -> Option<usize> {
    let mut a = adapters::NativeAdapter::EMPTY;
    a.name = Text::literal(name);
    a.driver = Text::literal(driver);
    a.link = 2;
    if status {
        a.ops.status = Some(lifecycle_status);
    }
    let index = unsafe { adapters::r4os_rust_net_adapter_register(&a) };
    (index != usize::MAX).then_some(index)
}
unsafe extern "C" fn lifecycle_status(
    _: usize,
    _: usize,
    out: *mut adapters::BackendStatus,
) -> i32 {
    // SAFETY: refresh lends its complete exclusive canonical integer record.
    let mut status = unsafe { core::mem::zeroed::<adapters::BackendStatus>() };
    status.irq_line = 255;
    status.link_up = u32::from(cleanup::r4os_rust_net_diagnostic_link());
    unsafe {
        out.write(status);
    }
    0
}
fn ip_view(protocol: u8, source_ip: [u8; 4], data: &[u8]) -> ipv4::PacketView {
    ipv4::PacketView {
        protocol,
        source_ip,
        dest_ip: config::settings().local_ip,
        payload: Text {
            pointer: data.as_ptr(),
            len: data.len(),
        },
    }
}
struct RestoreConfig(config::Settings);
impl Drop for RestoreConfig {
    fn drop(&mut self) {
        config::restore(self.0);
    }
}

fn cleanup_probe() -> bool {
    let before = cleanup_status();
    let before_udp = udp_status();
    let before_tcp = summary();
    if listener(65012) {
        return false;
    }
    let handle = frames::r4os_rust_frames_udp_bind(65011);
    if handle <= 0 {
        return false;
    }
    if !listen(65012) {
        frames::r4os_rust_frames_udp_close(handle as u32);
        return false;
    }
    pending(b"diag", b"cleanup.test");
    let after = unsafe {
        cleanup::scoped(
            Text::literal(b"diag-cleanup"),
            scope(handle as u32, 0, 65012),
        )
    };
    let after_udp = udp_status();
    let after_tcp = summary();
    after.runs == before.runs + 1
        && after.last_udp_closed == 1
        && after.last_tcp_listeners == 1
        && after.udp_sockets_closed == before.udp_sockets_closed + 1
        && after.tcp_listeners_closed == before.tcp_listeners_closed + 1
        && after.dhcp_operations_cancelled == before.dhcp_operations_cancelled + 1
        && after.dns_operations_cancelled == before.dns_operations_cancelled + 1
        && after_udp.active_sockets == before_udp.active_sockets
        && after_tcp.active_connections == before_tcp.active_connections
        && after_tcp.active_listeners == before_tcp.active_listeners
        && !listener(65012)
        && !dhcp_client::stats().operation_pending
        && !dns_stats().operation_pending
        && equal(dhcp_client::stats().pending_label, b"idle")
}
fn power_case(
    reason: &'static [u8],
    udp_port: u16,
    listen_port: u16,
    remote_ip: [u8; 4],
    remote_port: u16,
) -> bool {
    let before = cleanup_status();
    let before_udp = udp_status();
    let before_tcp = summary();
    if listener(listen_port) {
        return false;
    }
    let handle = frames::r4os_rust_frames_udp_bind(udp_port);
    if handle <= 0 {
        return false;
    }
    let conn = unsafe { tcp::r4os_rust_net_tcp_self_test_connect(&remote_ip, remote_port) };
    if conn <= 0 {
        frames::r4os_rust_frames_udp_close(handle as u32);
        return false;
    }
    if !listen(listen_port) {
        frames::r4os_rust_frames_udp_close(handle as u32);
        abort(conn as u32, b"diag-rollback");
        return false;
    }
    pending(reason, b"power.cleanup");
    let after = unsafe {
        cleanup::scoped(
            Text::literal(reason),
            scope(handle as u32, conn as u32, listen_port),
        )
    };
    let after_udp = udp_status();
    let after_tcp = summary();
    let reason_ok = match reason {
        b"poweroff" => after.poweroff_cleanups == before.poweroff_cleanups + 1,
        b"reboot" => after.reboot_cleanups == before.reboot_cleanups + 1,
        _ => false,
    };
    reason_ok
        && after.runs == before.runs + 1
        && after.last_udp_closed == 1
        && after.last_tcp_connections == 1
        && after.last_tcp_listeners == 1
        && after.udp_sockets_closed == before.udp_sockets_closed + 1
        && after.tcp_connections_aborted == before.tcp_connections_aborted + 1
        && after.tcp_listeners_closed == before.tcp_listeners_closed + 1
        && after.dhcp_operations_cancelled == before.dhcp_operations_cancelled + 1
        && after.dns_operations_cancelled == before.dns_operations_cancelled + 1
        && after_udp.active_sockets == before_udp.active_sockets
        && after_tcp.active_connections == before_tcp.active_connections
        && after_tcp.active_listeners == before_tcp.active_listeners
        && !listener(listen_port)
        && !dhcp_client::stats().operation_pending
        && !dns_stats().operation_pending
        && equal(dhcp_client::stats().pending_label, b"idle")
}
fn power_probe() -> bool {
    if !power_case(b"poweroff", 65031, 65032, [198, 51, 100, 41], 80) {
        return false;
    }
    if !power_case(b"reboot", 65033, 65034, [198, 51, 100, 42], 443) {
        return false;
    }
    cleanup::r4os_rust_net_note_power_lifecycle(2);
    true
}
fn link_probe() -> bool {
    let before = cleanup_status();
    let before_udp = udp_status();
    let before_tcp = summary();
    if listener(65016) {
        return false;
    }
    cleanup::r4os_rust_net_diagnostic_link_set(true);
    let Some(index) = register(b"diag-net", b"diag", true) else {
        return false;
    };
    let _fixture = DiagnosticAdapter(index);
    let handle = frames::r4os_rust_frames_udp_bind(65015);
    if handle <= 0 {
        return false;
    }
    if !listen(65016) {
        frames::r4os_rust_frames_udp_close(handle as u32);
        return false;
    }
    pending(b"lifecycle", b"lifecycle.test");
    cleanup::r4os_rust_net_diagnostic_link_set(false);
    adapter_runtime::refresh(index, Some(scope(handle as u32, 0, 65016)));
    let after = cleanup_status();
    let after_udp = udp_status();
    let after_tcp = summary();
    let down = adapter(index).link == 1
        && adapter(index).lifecycle == 3
        && after.runs == before.runs + 1
        && after.link_down_cleanups == before.link_down_cleanups + 1
        && after.last_udp_closed == 1
        && after.last_tcp_listeners == 1
        && after.udp_sockets_closed == before.udp_sockets_closed + 1
        && after.tcp_listeners_closed == before.tcp_listeners_closed + 1
        && after_udp.active_sockets == before_udp.active_sockets
        && after_tcp.active_connections == before_tcp.active_connections
        && after_tcp.active_listeners == before_tcp.active_listeners
        && !listener(65016)
        && !dhcp_client::stats().operation_pending
        && !dns_stats().operation_pending;
    cleanup::r4os_rust_net_diagnostic_link_set(true);
    adapter_runtime::refresh(index, Some(scope(0, 0, 0)));
    let up = adapter(index).link == 2 && adapter(index).lifecycle == 2;
    down && up
}
fn reset_probe() -> bool {
    let before = cleanup_status();
    let before_udp = udp_status();
    let before_tcp = summary();
    if listener(65018) {
        return false;
    }
    cleanup::r4os_rust_net_diagnostic_link_set(true);
    let Some(index) = register(b"diag-reset", b"diag", true) else {
        return false;
    };
    let _fixture = DiagnosticAdapter(index);
    let handle = frames::r4os_rust_frames_udp_bind(65017);
    if handle <= 0 {
        return false;
    }
    if !listen(65018) {
        frames::r4os_rust_frames_udp_close(handle as u32);
        return false;
    }
    pending(b"adapter-reset", b"reset.test");
    if !unsafe {
        adapter_runtime::r4os_rust_net_adapter_runtime_reset(
            index,
            Text::literal(b"diag-reset"),
            &scope(handle as u32, 0, 65018),
        )
    } {
        return false;
    }
    let after = cleanup_status();
    let after_udp = udp_status();
    let after_tcp = summary();
    adapter(index).stats.resets == 1
        && adapter(index).lifecycle == 2
        && after.runs == before.runs + 1
        && after.adapter_reset_cleanups == before.adapter_reset_cleanups + 1
        && after.last_udp_closed == 1
        && after.last_tcp_listeners == 1
        && after.udp_sockets_closed == before.udp_sockets_closed + 1
        && after.tcp_listeners_closed == before.tcp_listeners_closed + 1
        && after_udp.active_sockets == before_udp.active_sockets
        && after_tcp.active_connections == before_tcp.active_connections
        && after_tcp.active_listeners == before_tcp.active_listeners
        && !listener(65018)
        && !dhcp_client::stats().operation_pending
        && !dns_stats().operation_pending
}
fn driver_probe() -> bool {
    let before_count = adapters::r4os_rust_net_adapter_count();
    if before_count + 1 >= adapters::CAPACITY {
        return false;
    }
    let before = cleanup_status();
    let before_udp = udp_status();
    let before_tcp = summary();
    if listener(65022) {
        return false;
    }
    cleanup::r4os_rust_net_diagnostic_link_set(true);
    let Some(index) = register(b"diag-r4d-unload", b"diag-r4d", true) else {
        return false;
    };
    if adapters::r4os_rust_net_adapter_count() != before_count + 1 {
        drop(DiagnosticAdapter(index));
        return false;
    }
    let packet_index = packet_pool::r4os_rust_net_packet_alloc();
    if packet_index == usize::MAX {
        drop(DiagnosticAdapter(index));
        return false;
    }
    let packet = unsafe { packet_pool::r4os_rust_net_packet(packet_index) };
    if packet.is_null() {
        return false;
    }
    unsafe {
        (*packet).adapter_index = index;
        (*packet).len = 8;
    }
    let handle = frames::r4os_rust_frames_udp_bind(65021);
    if handle <= 0 {
        packet_pool::r4os_rust_net_packet_free(packet_index);
        drop(DiagnosticAdapter(index));
        return false;
    }
    if !listen(65022) {
        packet_pool::r4os_rust_net_packet_free(packet_index);
        frames::r4os_rust_frames_udp_close(handle as u32);
        drop(DiagnosticAdapter(index));
        return false;
    }
    pending(b"driver-unload", b"driver-unload.test");
    if !unsafe {
        adapter_runtime::r4os_rust_net_adapter_runtime_unregister(
            index,
            &scope(handle as u32, 0, 65022),
        )
    } {
        return false;
    }
    let after = cleanup_status();
    let after_udp = udp_status();
    let after_tcp = summary();
    let unload = adapters::r4os_rust_net_adapter_count() == before_count
        && unsafe { packet_pool::r4os_rust_net_packet(packet_index) }.is_null()
        && after.runs == before.runs + 1
        && after.adapter_unregister_cleanups == before.adapter_unregister_cleanups + 1
        && after.last_udp_closed == 1
        && after.last_tcp_listeners == 1
        && after.udp_sockets_closed == before.udp_sockets_closed + 1
        && after.tcp_listeners_closed == before.tcp_listeners_closed + 1
        && after.dhcp_operations_cancelled == before.dhcp_operations_cancelled + 1
        && after.dns_operations_cancelled == before.dns_operations_cancelled + 1
        && after_udp.active_sockets == before_udp.active_sockets
        && after_tcp.active_connections == before_tcp.active_connections
        && after_tcp.active_listeners == before_tcp.active_listeners
        && !listener(65022)
        && !dhcp_client::stats().operation_pending
        && !dns_stats().operation_pending;
    if !unload {
        return false;
    }
    let Some(reload_index) = register(b"diag-r4d-reload", b"diag-r4d", true) else {
        return false;
    };
    let reload = adapters::r4os_rust_net_adapter_count() == before_count + 1
        && adapter(reload_index).lifecycle == 2
        && adapter(reload_index).link == 2;
    drop(DiagnosticAdapter(reload_index));
    if !reload || adapters::r4os_rust_net_adapter_count() != before_count {
        return false;
    }
    cleanup::r4os_rust_net_note_driver_lifecycle();
    true
}

fn dec(value: impl Into<u64>) {
    foundation::dec(value.into());
}
fn rx_probe() -> bool {
    let status = capture(rx_runtime::r4os_rust_net_rx_runtime_summary);
    let ok = status.started
        && status.queue_capacity == super::rx_handoff::CAPACITY as u32
        && status.queue_high_water <= status.queue_capacity
        && status.batch_max <= 32
        && status.processed == status.protocol_success + status.protocol_failures
        && status.schedules_from_irq <= status.schedules
        && status.release_failures == 0
        && status.irq_frame_rejects == 0
        && status.metadata_selected <= status.metadata_submissions
        && status.metadata_fallbacks <= status.metadata_submissions
        && status.l4_offload_used <= status.metadata_selected
        && status.ownership_balanced;

    foundation::puts(b"NETRX handoff-probe result=");
    foundation::puts(if ok { b"OK" } else { b"FAILED" });
    foundation::puts(b" accepted=");
    dec(status.accepted);
    foundation::puts(b" processed=");
    dec(status.processed);
    foundation::puts(b" cancelled=");
    dec(status.cancelled);
    foundation::puts(b" queue=");
    dec(status.queue_ready);
    foundation::puts(b"/");
    dec(status.queue_occupied);
    foundation::puts(b"/");
    dec(status.queue_high_water);
    foundation::puts(b" busy=");
    dec(status.queue_busy);
    foundation::puts(b" irq-wake=");
    dec(status.schedules_from_irq);
    foundation::puts(b" event=");
    dec(status.event_wakeups);
    foundation::puts(b" fallback=");
    dec(status.fallback_timeouts);
    foundation::puts(b" batch-max=");
    dec(status.batch_max);
    foundation::puts(b" budget-end=");
    dec(status.budget_exhaustions);
    foundation::puts(b" schedule-tail-ns=");
    dec(status.schedule_latency_max_ns);
    foundation::puts(b" handoff-tail-ns=");
    dec(status.handoff_latency_max_ns);
    foundation::puts(b" irq-inline=");
    dec(status.irq_frame_rejects);
    foundation::puts(b" offload=");
    dec(status.l4_offload_used);
    foundation::puts(b"/");
    dec(status.metadata_selected);
    foundation::puts(b" fallback=");
    dec(status.metadata_fallbacks);
    foundation::puts(b" software=");
    dec(status.l4_software_checks);
    foundation::puts(b"\r\n");

    if ok {
        cleanup::r4os_rust_net_note_driver_environment();
    }
    ok
}
fn capability_probe() -> bool {
    let offer = backend::Offer {
        offered: 1 | (1 << 9),
        required: 0,
        rx_queue_count: 4,
        tx_queue_count: 4,
        max_rx_segments: 1,
        max_tx_segments: 1,
        rx_ownership: 0,
        tx_ownership: 0,
        interrupt_moderation_us: 0,
    };
    let Ok(capable) = backend::negotiate(offer) else {
        return false;
    };
    let flat_offer = backend::Offer {
        offered: 0,
        rx_queue_count: 1,
        tx_queue_count: 1,
        ..offer
    };
    let Ok(flat) = backend::negotiate(flat_offer) else {
        return false;
    };
    if !matches!(
        backend::negotiate(backend::Offer {
            offered: 1 << 6,
            required: 1 << 6,
            ..flat_offer
        }),
        Err(3)
    ) {
        return false;
    }
    let mut frame = [0u8; 42];
    frame[12] = 8;
    frame[13] = 0;
    frame[14] = 0x45;
    frame[16] = 0;
    frame[17] = 28;
    frame[23] = 17;
    frame[38] = 0;
    frame[39] = 8;
    let original = frame;
    let capable_metadata = backend::select_rx_metadata(capable.accepted, 1, &frame);
    let flat_metadata = backend::select_rx_metadata(flat.accepted, 1, &frame);
    let unknown_metadata = backend::select_rx_metadata(capable.accepted, 1 | (1u64 << 63), &frame);
    // Canonical descriptor is integers only; preserve Zig's version/size defaults.
    let mut packet = unsafe { core::mem::zeroed::<backend::Packet>() };
    packet.version = 1;
    packet.size = core::mem::size_of::<backend::Packet>() as u32;
    packet.fallback_addr = 1;
    packet.fallback_bytes = frame.len() as u32;
    let mut wrong_queue = packet;
    wrong_queue.queue_index = 1;
    let ok = capable.accepted == 1
        && capable.rejected == 1 << 9
        && capable.rx_queue_count == 1
        && capable.tx_queue_count == 1
        && flat.accepted == 0
        && flat.rejected == 0
        && matches!(
            capable_metadata,
            backend::RxMetadataDecision::L4ChecksumValid
        )
        && matches!(flat_metadata, backend::RxMetadataDecision::SoftwareFallback)
        && matches!(
            unknown_metadata,
            backend::RxMetadataDecision::SoftwareFallback
        )
        && backend::valid_rx_packet(packet, capable, 1536)
        && !backend::valid_rx_packet(wrong_queue, capable, 1536)
        && frame == original;
    foundation::puts(b"NETCAP negotiation-probe result=");
    foundation::puts(if ok { b"OK" } else { b"FAILED" });
    foundation::puts(b" accepted=");
    hex(capable.accepted, 16);
    foundation::puts(b" rejected=");
    hex(capable.rejected, 16);
    foundation::puts(b" queues=");
    dec(capable.rx_queue_count);
    foundation::puts(b"/");
    dec(capable.tx_queue_count);
    foundation::puts(b" fallback=byte-identical\r\n");
    if ok {
        cleanup::r4os_rust_net_note_driver_limit();
    }
    ok
}

fn hex(value: u64, digits: usize) {
    unsafe {
        net_console_hex(value, digits);
    }
}
fn environment_absence() -> bool {
    if adapters::r4os_rust_net_adapter_count() == 0 {
        if unsafe { frames::send_ipv4([10, 0, 2, 1], 1, b"ENV".as_ptr(), 3) } != 1 {
            return false;
        }
        if frames::r4os_rust_frames_arp_gateway() != 1 {
            return false;
        }
        if dhcp_client::r4os_rust_dhcp_client_acquire_default(false, 0) != 1 {
            return false;
        }
        if unsafe { tcp_client::r4os_rust_tcp_client_connect(&[10, 0, 2, 2], 80, false, 0) }
            != super::r4p_contract::TCP_RESULT_NO_CONNECTION
        {
            return false;
        }
        return equal(dhcp_client::stats().last_error, b"no-adapter")
            && equal(tcp_stats().last_error, b"no-adapter");
    }
    let frame = [0; 60];
    if unsafe { frames::r4os_rust_frames_transmit(adapters::CAPACITY, frame.as_ptr(), frame.len()) }
        != 1
    {
        return false;
    }
    if dhcp_client::r4os_rust_dhcp_client_acquire(adapters::CAPACITY, false, 0) != 1 {
        return false;
    }
    !unsafe {
        adapter_runtime::r4os_rust_net_adapter_runtime_reset(
            adapters::CAPACITY,
            Text::literal(b"env-invalid-adapter"),
            ptr::null(),
        )
    }
}
fn environment_defaults() -> bool {
    config::reset();
    let cfg = config::settings();
    cfg.local_ip == config::DEFAULT_LOCAL_IP
        && cfg.netmask == config::DEFAULT_NETMASK
        && cfg.gateway_ip == config::DEFAULT_GATEWAY_IP
        && cfg.dns_ip == config::DEFAULT_DNS_IP
        && !cfg.configured
        && !cfg.dhcp_enabled
        && !cfg.dns_configured
        && cfg.invalid_options == 0
        && equal(config::source_name(), b"default")
}
fn environment_bad_config() -> bool {
    config::apply_options(|key| {
        if key == b"ip" {
            Some(b"999.1.1.1")
        } else {
            None
        }
    });
    let current = config::settings();
    current.invalid_options == 1
        && equal(current.last_error, b"bad-ipv4")
        && equal(config::source_name(), b"bad-ipv4")
}
fn r4p_required() -> bool {
    let status = capture(protocol_runtime::r4os_rust_net_r4p_runtime);
    if status.protocol_count != 9 || status.active + status.missing != status.protocol_count {
        return false;
    }
    for kind in 0..9 {
        let mut role = MaybeUninit::uninit();
        let mut stats = MaybeUninit::uninit();
        unsafe {
            protocol_runtime::r4os_rust_net_protocol_role(kind, role.as_mut_ptr());
            let role: Text = role.assume_init();
            if crate::driver::protocol_registry::r4os_rust_protocol_registry_query(
                0,
                0,
                role.pointer,
                role.len,
            ) == 0
            {
                return false;
            }
            protocol_runtime::r4os_rust_net_protocol_runtime(kind, stats.as_mut_ptr());
            let stats: protocol_runtime::ProtocolRuntimeStats = stats.assume_init();
            if stats.builtin_fallback || stats.fallback_policy != 0 || stats.fallback_decision != 0
            {
                return false;
            }
        }
    }
    true
}
fn environment_probe() -> bool {
    let _restore = RestoreConfig(config::settings());
    if !environment_absence() {
        return false;
    }
    if !environment_defaults() {
        return false;
    }
    if !environment_bad_config() {
        return false;
    }
    if !r4p_required() {
        return false;
    }
    cleanup::r4os_rust_net_note_environment_contract(4);
    true
}

fn backpressure_probe() -> bool {
    let before = telemetry::backpressure();
    if before.packet_pool_used > before.packet_pool_limit
        || before.app_ipv4_queued > before.app_ipv4_queue_limit
        || before.udp_queued_packets > before.udp_queue_limit_total
        || before.udp_active_sockets > before.udp_socket_limit
        || before.tcp_active_connections > before.tcp_connection_limit
        || before.tcp_active_listeners > before.tcp_listener_limit
        || before.ipc_service_queued > before.ipc_service_queue_limit
        || before.ipc_service_channels != 4
        || before.ipc_service_queue_depth != crate::service::ipc::QUEUE_DEPTH as u32
        || before.ipc_service_message_max != crate::service::ipc::MAX_MESSAGE_SIZE as u32
        || !equal(before.nonblocking_empty_status, b"would-block")
        || before.resource_backend_busy != before.tx_busy
        || before.resource_packet_drops < before.packet_drops
    {
        return false;
    }
    let test_frame = [0; 64];
    if unsafe {
        frames::r4os_rust_frames_transmit(adapters::CAPACITY, test_frame.as_ptr(), test_frame.len())
    } != 1
    {
        return false;
    }
    let invalid = telemetry::backpressure();
    if invalid.tx_no_adapter != before.tx_no_adapter + 1
        || invalid.tx_failures != before.tx_failures + 1
        || !equal(invalid.tx_last_result, b"no-adapter")
    {
        return false;
    }
    let handle = frames::r4os_rust_frames_udp_bind(65001);
    if handle <= 0 {
        return false;
    }
    let mut info = MaybeUninit::uninit();
    let mut payload = [0; 8];
    let got = unsafe {
        super::udp_runtime::r4os_rust_udp_recv_wait(
            handle as u32,
            info.as_mut_ptr(),
            payload.as_mut_ptr(),
            payload.len(),
            0,
            &super::udp_runtime::STANDARD_PEERS,
        )
    };
    let would_block = got == 0 && equal(udp_stats().last_error, b"would-block");
    frames::r4os_rust_frames_udp_close(handle as u32);
    if !would_block {
        return false;
    }
    let after = telemetry::backpressure();
    after.udp_active_sockets == before.udp_active_sockets
        && after.udp_queued_packets == before.udp_queued_packets
        && after.packet_pool_limit == before.packet_pool_limit
        && after.app_ipv4_queue_limit == before.app_ipv4_queue_limit
        && after.ipc_service_queue_limit == before.ipc_service_queue_limit
        && after.ipc_service_message_max == before.ipc_service_message_max
        && after.tx_no_adapter == invalid.tx_no_adapter
        && after.tx_failures == invalid.tx_failures
        && after.resource_packet_drops >= after.packet_drops
        && after.resource_backend_busy == after.tx_busy
}
fn limit_tx() -> bool {
    let before = telemetry::backpressure();
    if adapters::r4os_rust_net_adapter_count() >= adapters::CAPACITY {
        return false;
    }
    let Some(index) = register(b"diag-limit-tx", b"diag", false) else {
        return false;
    };
    let _fixture = DiagnosticAdapter(index);
    let frame = [0; packet_pool::MAX_FRAME + 1];
    let result = unsafe { frames::r4os_rust_frames_transmit(index, frame.as_ptr(), frame.len()) };
    let after = telemetry::backpressure();
    result == 4
        && after.tx_too_large == before.tx_too_large + 1
        && after.tx_failures == before.tx_failures + 1
        && equal(after.tx_last_result, b"too-large")
}
fn limit_app() -> bool {
    let payload = [0x41; app_ipv4::PAYLOAD_MAX + 1];
    let input = app_ipv4::Info {
        source_ip: [203, 0, 113, 11],
        dest_ip: config::settings().local_ip,
        protocol: 1,
        ..app_ipv4::Info::EMPTY
    };
    let mut info = app_ipv4::Info::EMPTY;
    let mut out = [0; 8];
    let got = unsafe {
        app_ipv4::r4os_rust_app_ipv4_enqueue(&input, payload.as_ptr(), payload.len());
        app_ipv4::r4os_rust_app_ipv4_read(1, &mut info, out.as_mut_ptr(), out.len())
    };
    got == out.len() as i32
        && info.truncated
        && info.payload_len == (app_ipv4::PAYLOAD_MAX + 1) as u32
        && info.protocol == 1
}
struct UdpSocket(u32);
impl Drop for UdpSocket {
    fn drop(&mut self) {
        frames::r4os_rust_frames_udp_close(self.0);
    }
}
fn dispatch_udp(source: [u8; 4], source_port: u16, port: u16, data: &[u8]) {
    let info = udp_sockets::Info {
        source_ip: source,
        dest_ip: config::settings().local_ip,
        source_port,
        dest_port: port,
        length: 0,
    };
    let mut reason = MaybeUninit::uninit();
    unsafe {
        if udp_sockets::r4os_rust_udp_socket_dispatch(
            &info,
            data.as_ptr(),
            data.len(),
            reason.as_mut_ptr(),
        ) {
            stats::r4os_rust_net_udp_owned_error(reason.assume_init());
        }
    }
}
fn limit_udp_receive() -> bool {
    let before = telemetry::backpressure();
    let handle = frames::r4os_rust_frames_udp_bind(65091);
    if handle <= 0 {
        return false;
    }
    let _socket = UdpSocket(handle as u32);
    dispatch_udp([203, 0, 113, 12], 50000, 65091, b"LIMIT");
    let mut info = MaybeUninit::uninit();
    let mut out = [0; 2];
    let got = unsafe {
        frames::r4os_rust_frames_udp_recv(
            handle as u32,
            info.as_mut_ptr(),
            out.as_mut_ptr(),
            out.len(),
        )
    };
    let after = telemetry::backpressure();
    got == -2
        && after.resource_buffer_small == before.resource_buffer_small + 1
        && equal(udp_stats().last_error, b"recv-small")
}
fn limit_udp_payload() -> bool {
    let before = telemetry::backpressure();
    let handle = frames::r4os_rust_frames_udp_bind(65092);
    if handle <= 0 {
        return false;
    }
    let _socket = UdpSocket(handle as u32);
    let payload = [0x55; udp_sockets::PAYLOAD_MAX + 1];
    dispatch_udp([203, 0, 113, 13], 50001, 65092, &payload);
    let after = telemetry::backpressure();
    after.udp_drops == before.udp_drops + 1
        && after.resource_queue_full == before.resource_queue_full + 1
        && equal(udp_stats().last_error, b"socket-payload")
}
fn limit_probe() -> bool {
    let _restore = RestoreConfig(config::settings());
    if !limit_tx() {
        return false;
    }
    if !limit_app() {
        return false;
    }
    if !limit_udp_receive() {
        return false;
    }
    if !limit_udp_payload() {
        return false;
    }
    unsafe {
        if !tcp::r4os_rust_net_tcp_buffer_limit_probe() {
            return false;
        }
        if !tcp::r4os_rust_net_tcp_out_of_order_probe() {
            return false;
        }
        if !tcp::r4os_rust_net_tcp_overlap_retransmit_probe() {
            return false;
        }
        if !tcp::r4os_rust_net_tcp_wraparound_probe() {
            return false;
        }
        if !tcp::r4os_rust_net_tcp_graceful_close_probe() {
            return false;
        }
        if !tcp::r4os_rust_net_tcp_performance_contract_probe() {
            return false;
        }
    }
    cleanup::r4os_rust_net_note_limit_contract(10);
    true
}
fn timing_probe() -> bool {
    let s = timing::status();
    timing::contract_check()
        && s.arp_cache_ttl_ticks == timing::DEFAULT_ARP_CACHE_TTL_TICKS
        && s.arp_resolve_timeout_ticks == timing::DEFAULT_ARP_RESOLVE_TIMEOUT_TICKS
        && s.dhcp_timeout_ticks == timing::DEFAULT_DHCP_TIMEOUT_TICKS
        && s.dns_timeout_ticks == timing::DEFAULT_DNS_TIMEOUT_TICKS
        && s.tcp_listen_timeout_ticks == timing::DEFAULT_TCP_LISTEN_TIMEOUT_TICKS
        && s.tcp_retransmit_timeout_ticks == timing::DEFAULT_TCP_RETRANSMIT_TIMEOUT_TICKS
        && s.tcp_time_wait_ticks == timing::DEFAULT_TCP_TIME_WAIT_TICKS
        && s.service_operation_timeout_ticks == timing::DEFAULT_SERVICE_OPERATION_TIMEOUT_TICKS
}
fn connection(id: u32) -> Option<crate::contract::TcpConnectionInfo> {
    for index in 0..tcp::MAX_CONNECTIONS {
        let mut info = MaybeUninit::uninit();
        if unsafe { tcp::r4os_rust_net_tcp_connection_info(index as u32, info.as_mut_ptr()) } <= 0 {
            continue;
        }
        let info = unsafe { info.assume_init() };
        if info.id == id {
            return Some(info);
        }
    }
    None
}
fn negative_rst() -> bool {
    let before = summary();
    let before_rst = tcp_stats().rst_rx;
    let remote = [198, 51, 100, 10];
    let raw = unsafe { tcp::r4os_rust_net_tcp_begin_live_connect(&remote, 65080, 0x1111_0000) };
    if raw <= 0 {
        return false;
    }
    let id = raw as u32;
    let Some(info) = connection(id) else {
        abort(id, b"negative-missing");
        return false;
    };
    let mut segment = [0; 64];
    let params = builders::WireTcpBuild {
        source_ip: remote,
        dest_ip: config::settings().local_ip,
        source_port: info.remote_port,
        dest_port: info.local_port,
        seq: 1,
        ack: info.seq.wrapping_add(1),
        flags: tcp::FLAG_ACK | tcp::FLAG_RST,
        window: tcp::MAX_ADVERTISED_WINDOW,
        options: 0,
    };
    let Some(len) = (unsafe {
        tcp_client::build(params, b"".as_ptr(), 0, segment.as_mut_ptr(), segment.len())
    }) else {
        abort(id, b"negative-rst-build");
        return false;
    };
    let packet = ip_view(6, remote, &segment[..len]);
    unsafe {
        tcp_wire::r4os_rust_net_tcp_wire_receive(&packet, false);
    }
    tcp_error(b"rst");
    let after = summary();
    tcp_stats().rst_rx == before_rst + 1
        && after.active_connections == before.active_connections
        && connection(id).is_none()
        && equal(tcp_stats().last_error, b"rst")
}
fn negative_tcp_timeout() -> bool {
    let before = summary();
    let before_timeouts = tcp_stats().timeouts;
    let raw = unsafe {
        tcp::r4os_rust_net_tcp_begin_live_connect(&[198, 51, 100, 11], 65081, 0x2222_0000)
    };
    if raw <= 0 {
        return false;
    }
    let id = raw as u32;
    unsafe {
        tcp::r4os_rust_net_tcp_mark_timeout(b"negative-timeout".as_ptr(), 16);
    }
    abort(id, b"negative-timeout");
    tcp_error(b"negative-timeout");
    let after = summary();
    tcp_stats().timeouts == before_timeouts + 1
        && after.active_connections == before.active_connections
        && connection(id).is_none()
        && equal(tcp_stats().last_error, b"negative-timeout")
}
fn negative_probe() -> bool {
    let _restore = RestoreConfig(config::settings());
    config::apply_addresses(
        Addresses {
            local_ip: [192, 0, 2, 2],
            netmask: [255, 255, 255, 0],
            gateway_ip: [0; 4],
            dns_ip: [0; 4],
            dns_configured: false,
        },
        false,
    );
    let route = bytes::route_ipv4_target([203, 0, 113, 9]);
    if route.result != 6 || !equal(route.last_error, b"route-no-gateway") {
        return false;
    }
    if !stats::r4os_rust_net_arp_negative_timeout() {
        return false;
    }
    let mut out = [0; 4];
    let name = b"negative-noserver.r4os";
    let result =
        unsafe { dns_client::r4os_rust_dns_client_resolve(name.as_ptr(), name.len(), &mut out) };
    if result != super::dns::RESULT_NO_SERVER
        || out != [0; 4]
        || !equal(dns_stats().last_error, b"no-server")
    {
        return false;
    }
    let before = dns_stats().timeouts;
    let name = b"negative-timeout.r4os";
    let result = unsafe {
        dns_client::r4os_rust_dns_client_timeout(name.as_ptr(), name.len(), &[203, 0, 113, 53])
    };
    if result != super::dns::RESULT_TIMEOUT
        || dns_stats().timeouts != before + 1
        || dns_stats().last_result != super::dns::RESULT_TIMEOUT
        || !equal(dns_stats().last_error, b"timeout")
    {
        return false;
    }
    if !negative_rst() {
        return false;
    }
    if !negative_tcp_timeout() {
        return false;
    }
    cleanup::r4os_rust_net_note_negative_path(6);
    true
}
fn corpus_ethernet() -> bool {
    let mac = [0x52, 0x54, 0, 0x12, 0x34, 0x56];
    let remote = [0x52, 0x55, 0x0a, 0, 2, 2];
    let before = ethernet_stats().dropped_short;
    let short = [0; 4];
    if unsafe { io::ethernet_rx(mac, short.as_ptr(), short.len()) } {
        return false;
    }
    if ethernet_stats().dropped_short <= before {
        return false;
    }
    let before = ethernet_stats().unknown_ethertype;
    let mut unknown = [0; 60];
    unknown[..6].copy_from_slice(&mac);
    unknown[6..12].copy_from_slice(&remote);
    bytes::write_be16(&mut unknown, 12, 0x1234);
    if !unsafe { io::ethernet_rx(mac, unknown.as_ptr(), unknown.len()) } {
        return false;
    }
    ethernet_stats().unknown_ethertype > before
}
fn corpus_arp() -> bool {
    let mac = [0x52, 0x54, 0, 0x12, 0x34, 0x56];
    let before = arp_stats().malformed;
    let mut short = [0; 20];
    bytes::write_be16(&mut short, 12, 0x0806);
    unsafe {
        io::arp_rx(short.as_ptr(), short.len());
    }
    let mut frame = [0; 60];
    let Some(len) = (unsafe {
        io::build_arp(
            builders::WireArpBuild {
                source_mac: mac,
                target_ip: [10, 0, 2, 2],
            },
            frame.as_mut_ptr(),
            frame.len(),
        )
    }) else {
        return false;
    };
    frame[18] = 5;
    unsafe {
        io::arp_rx(frame.as_ptr(), len);
    }
    arp_stats().malformed >= before + 2
}
fn build_ip(out: &mut [u8]) -> Option<usize> {
    let params = builders::WireIpv4Build {
        source_mac: [0x52, 0x55, 0x0a, 0, 2, 2],
        dest_mac: [0x52, 0x54, 0, 0x12, 0x34, 0x56],
        source_ip: [10, 0, 2, 2],
        dest_ip: config::settings().local_ip,
        protocol: 1,
    };
    unsafe { io::build_ipv4(params, b"BADIP".as_ptr(), 5, out.as_mut_ptr(), out.len()) }
}
fn refresh_ip_checksum(frame: &mut [u8]) {
    bytes::write_be16(frame, 24, 0);
    let ihl = usize::from(frame[14] & 15) * 4;
    let checksum = bytes::internet_checksum(&frame[14..14 + ihl]);
    bytes::write_be16(frame, 24, checksum);
}
fn corpus_ipv4() -> bool {
    let before = ipv4_stats().dropped_short;
    let mut short = [0; 18];
    bytes::write_be16(&mut short, 12, 0x0800);
    if unsafe { io::ipv4_rx(short.as_ptr(), short.len()) }.is_some() {
        return false;
    }
    if ipv4_stats().dropped_short <= before {
        return false;
    }
    let before = ipv4_stats().dropped_version;
    let mut frame = [0; 96];
    let Some(len) = build_ip(&mut frame) else {
        return false;
    };
    frame[14] = 0x65;
    if unsafe { io::ipv4_rx(frame.as_ptr(), len) }.is_some() {
        return false;
    }
    if ipv4_stats().dropped_version <= before {
        return false;
    }
    let before = ipv4_stats().dropped_fragment;
    let mut frame = [0; 96];
    let Some(len) = build_ip(&mut frame) else {
        return false;
    };
    bytes::write_be16(&mut frame, 20, 0x2000);
    refresh_ip_checksum(&mut frame);
    if unsafe { io::ipv4_rx(frame.as_ptr(), len) }.is_some() {
        return false;
    }
    if ipv4_stats().dropped_fragment <= before {
        return false;
    }
    let before = ipv4_stats().dropped_checksum;
    let mut frame = [0; 96];
    let Some(len) = build_ip(&mut frame) else {
        return false;
    };
    frame[24] ^= 1;
    if unsafe { io::ipv4_rx(frame.as_ptr(), len) }.is_some() {
        return false;
    }
    ipv4_stats().dropped_checksum > before
}
fn corpus_icmp() -> bool {
    let before = icmp_stats().malformed;
    let packet = ip_view(1, [10, 0, 2, 2], &[8, 0, 0, 0]);
    unsafe {
        io::icmp_rx(packet);
    }
    if icmp_stats().malformed <= before {
        return false;
    }
    let before = icmp_stats().checksum_errors;
    let mut data = [0; 32];
    let Some(len) = (unsafe {
        io::build_icmp_request(
            builders::WireIcmpBuild {
                ident: 0x1111,
                seq: 1,
            },
            data.as_mut_ptr(),
            data.len(),
        )
    }) else {
        return false;
    };
    data[8] ^= 1;
    let packet = ip_view(1, [10, 0, 2, 2], &data[..len]);
    unsafe {
        io::icmp_rx(packet);
    }
    icmp_stats().checksum_errors > before
}
fn corpus_udp() -> bool {
    let source = [10, 0, 2, 2];
    let before = udp_stats().dropped_short;
    let packet = ip_view(17, source, &[1, 2, 3, 4]);
    if unsafe { io::udp_rx(packet, false) }.is_some() {
        return false;
    }
    if udp_stats().dropped_short <= before {
        return false;
    }
    let before = udp_stats().dropped_length;
    let mut data = [0; 8];
    bytes::write_be16(&mut data, 0, 1234);
    bytes::write_be16(&mut data, 2, 4321);
    bytes::write_be16(&mut data, 4, 12);
    if unsafe { io::udp_rx(ip_view(17, source, &data), false) }.is_some() {
        return false;
    }
    if udp_stats().dropped_length <= before {
        return false;
    }
    let before = udp_stats().checksum_errors;
    let mut data = [0; 32];
    let Some(len) = (unsafe {
        io::build_udp(
            builders::WireUdpBuild {
                source_ip: source,
                dest_ip: config::settings().local_ip,
                source_port: 1234,
                dest_port: 4321,
            },
            b"BAD".as_ptr(),
            3,
            data.as_mut_ptr(),
            data.len(),
        )
    }) else {
        return false;
    };
    data[8] ^= 1;
    if unsafe { io::udp_rx(ip_view(17, source, &data[..len]), false) }.is_some() {
        return false;
    }
    udp_stats().checksum_errors > before
}
struct RestoreDhcp {
    pending: bool,
    expectation: dhcp_messages::DhcpExpectation,
}
impl Drop for RestoreDhcp {
    fn drop(&mut self) {
        unsafe {
            dhcp_client::r4os_rust_dhcp_client_set_pending(self.pending, ptr::null());
            dhcp_messages::r4os_rust_dhcp_expectation_set(&self.expectation);
        }
    }
}
fn corpus_dhcp() -> bool {
    let before = dhcp_client::stats().malformed;
    let before_pending = dhcp_client::stats().operation_pending;
    let expectation = capture(dhcp_messages::r4os_rust_dhcp_expectation);
    let _restore = RestoreDhcp {
        pending: before_pending,
        expectation,
    };
    // Same synthetic response window as the original corpus. The productive
    // pending/phase/XID/MAC gate remains in force for normal traffic.
    unsafe {
        dhcp_client::r4os_rust_dhcp_client_set_pending(true, ptr::null());
        dhcp_messages::r4os_rust_dhcp_expectation_set(&dhcp_messages::DhcpExpectation {
            expected: 1,
            ..expectation
        });
        let short = [0; 16];
        if dhcp_client::r4os_rust_dhcp_client_message(short.as_ptr(), short.len()) {
            return false;
        }
        let bad = [0; 241];
        if dhcp_client::r4os_rust_dhcp_client_message(bad.as_ptr(), bad.len()) {
            return false;
        }
        let mut no_type = [0; 241];
        no_type[236..].copy_from_slice(&[0x63, 0x82, 0x53, 0x63, 255]);
        if dhcp_client::r4os_rust_dhcp_client_message(no_type.as_ptr(), no_type.len()) {
            return false;
        }
    }
    dhcp_client::stats().malformed >= before + 3
}
fn corpus_dns() -> bool {
    let before = dns_stats().malformed;
    unsafe {
        if dns_client::r4os_rust_dns_client_response([0, 1, 2, 3].as_ptr(), 4) {
            return false;
        }
        let mut bad = [0; 12];
        bytes::write_be16(&mut bad, 2, 0x0100);
        if dns_client::r4os_rust_dns_client_response(bad.as_ptr(), bad.len()) {
            return false;
        }
        let mut bad = [0; 13];
        bytes::write_be16(&mut bad, 2, 0x8180);
        bytes::write_be16(&mut bad, 4, 1);
        bytes::write_be16(&mut bad, 6, 1);
        bad[12] = 63;
        if dns_client::r4os_rust_dns_client_response(bad.as_ptr(), bad.len()) {
            return false;
        }
    }
    dns_stats().malformed >= before + 3
}
fn corpus_tcp() -> bool {
    let source = [10, 0, 2, 2];
    let before = tcp_stats().malformed;
    let packet = ip_view(6, source, &[0, 80, 0x1f, 0x90]);
    unsafe {
        tcp_wire::r4os_rust_net_tcp_wire_receive(&packet, false);
    }
    if tcp_stats().malformed <= before {
        return false;
    }
    let before = summary();
    let mut data = [0; 64];
    let Some(len) = (unsafe {
        tcp_client::build(
            builders::WireTcpBuild {
                source_ip: source,
                dest_ip: config::settings().local_ip,
                source_port: 80,
                dest_port: 49152,
                seq: 1,
                ack: 0,
                flags: tcp::FLAG_ACK,
                window: tcp::MAX_ADVERTISED_WINDOW,
                options: 0,
            },
            b"BAD".as_ptr(),
            3,
            data.as_mut_ptr(),
            data.len(),
        )
    }) else {
        return false;
    };
    data[20] ^= 1;
    let packet = ip_view(6, source, &data[..len]);
    unsafe {
        tcp_wire::r4os_rust_net_tcp_wire_receive(&packet, false);
    }
    summary().checksum_errors > before.checksum_errors
}
fn corpus_probe() -> bool {
    if !corpus_ethernet() {
        return false;
    }
    if !corpus_arp() {
        return false;
    }
    if !corpus_ipv4() {
        return false;
    }
    if !corpus_icmp() {
        return false;
    }
    if !corpus_udp() {
        return false;
    }
    if !corpus_dhcp() {
        return false;
    }
    if !corpus_dns() {
        return false;
    }
    if !corpus_tcp() {
        return false;
    }
    cleanup::r4os_rust_net_note_packet_corpus(21);
    true
}
fn r4p_probe() -> bool {
    let status = capture(protocol_runtime::r4os_rust_net_r4p_runtime);
    if status.protocol_count != 9
        || status.active + status.missing != status.protocol_count
        || status.active != status.protocol_count
    {
        return false;
    }
    let mut failures = 0u64;
    for kind in 0..9 {
        let mut stats = MaybeUninit::uninit();
        let stats = unsafe {
            protocol_runtime::r4os_rust_net_protocol_runtime(kind, stats.as_mut_ptr());
            stats.assume_init()
        };
        if stats.builtin_fallback || stats.fallback_policy != 0 || stats.fallback_decision != 0 {
            return false;
        }
        failures += stats.dispatch_failures;
    }
    status.dispatch_failures == failures
}
// Each entry is the same explicitly invoked existing diagnostic, not boot work.
macro_rules! entry {
    ($name:ident, $body:ident) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $name() -> bool {
            $body()
        }
    };
}
entry!(r4os_rust_net_diag_cleanup, cleanup_probe);
entry!(r4os_rust_net_diag_power, power_probe);
entry!(r4os_rust_net_diag_link, link_probe);
entry!(r4os_rust_net_diag_reset, reset_probe);
entry!(r4os_rust_net_diag_driver, driver_probe);
entry!(r4os_rust_net_diag_rx, rx_probe);
entry!(r4os_rust_net_diag_capability, capability_probe);
entry!(r4os_rust_net_diag_environment, environment_probe);
entry!(r4os_rust_net_diag_backpressure, backpressure_probe);
entry!(r4os_rust_net_diag_limit, limit_probe);
entry!(r4os_rust_net_diag_timing, timing_probe);
entry!(r4os_rust_net_diag_corpus, corpus_probe);
entry!(r4os_rust_net_diag_negative, negative_probe);
entry!(r4os_rust_net_diag_r4p, r4p_probe);

unsafe fn net_console_hex(value: u64, digits: usize) {
    crate::boot::log_route::hex(value, u8::try_from(digits).expect("hex width"));
}
