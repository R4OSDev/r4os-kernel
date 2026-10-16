//! Permanent native connections between the existing network owners.
//! Bootstrap installs these immutable kernel-lifetime tables in the original
//! order, before admitting workers or external packet callbacks.
use super::{
    adapters, app_ipv4, backend_admission, cache, cleanup, config, dhcp_client, dhcp_messages,
    dns_client, frames, packet_pool, protocol_runtime, protocol_stats, rx_runtime, tcp, tcp_client,
    tcp_rto, telemetry, transmit, udp_sockets,
};
use crate::{
    boot::config as boot_config,
    sched::{scheduler, sync_types::Text},
};
use core::{ffi::c_void, ptr};

pub(crate) static RX: rx_runtime::RxPeers = rx_runtime::RxPeers {
    process: frames::r4os_rust_frames_receive,
    maintenance: tcp_client::r4os_rust_tcp_client_maintenance,
    log: telemetry::r4os_rust_net_telemetry_log_rx,
};
static ARP: protocol_stats::ArpPeers = protocol_stats::ArpPeers {
    request: frames::r4os_rust_frames_send_arp,
    poll,
    idle,
};
static FRAME: frames::Peers = frames::Peers { rx: &RX, arp: &ARP };
static DHCP: dhcp_client::Peers = dhcp_client::Peers {
    refresh,
    poll,
    idle,
    bind: udp_bind,
    close: udp_close,
    recv: frames::r4os_rust_frames_udp_recv,
    send: dhcp_send,
    arp_flush,
};
static DNS: dns_client::Peers = dns_client::Peers {
    bind: udp_bind,
    close: udp_close,
    recv: frames::r4os_rust_frames_udp_recv,
    send: frames::r4os_rust_frames_udp_send,
    poll,
};
static TCP: tcp_client::Peers = tcp_client::Peers {
    send_ipv4: frames::r4os_rust_frames_send_ipv4,
    poll: poll_without_task,
};
unsafe extern "C" fn udp_bind(port: u16) -> i32 {
    frames::r4os_rust_frames_udp_bind(port)
}
unsafe extern "C" fn udp_close(handle: u32) -> i32 {
    frames::r4os_rust_frames_udp_close(handle)
}
unsafe extern "C" fn poll(rounds: usize) {
    unsafe { rx_runtime::r4os_rust_net_rx_runtime_poll(rounds, 255, &RX) }
}
unsafe extern "C" fn poll_without_task(rounds: usize) {
    unsafe { rx_runtime::r4os_rust_net_rx_runtime_poll_without_task(rounds, &RX) }
}
unsafe extern "C" fn idle() {
    scheduler::yield_task();
    unsafe {
        core::arch::asm!("sti", "hlt", options(nomem, nostack));
    }
}
unsafe extern "C" fn refresh(index: usize) {
    let mut out = adapters::BackendStatus::default();
    unsafe {
        super::adapter_runtime::r4os_rust_net_adapter_runtime_refresh(index, ptr::null(), &mut out);
    }
}
unsafe extern "C" fn dhcp_send(
    index: usize,
    socket: u32,
    source: *const [u8; 4],
    target: *const [u8; 4],
    data: *const u8,
    len: usize,
) -> u8 {
    unsafe { frames::r4os_rust_frames_udp_dhcp(socket, index, source, target, data, len) }
}
unsafe extern "C" fn arp_flush() {
    protocol_stats::r4os_rust_net_arp_cache_flush_owned();
}

/// # Safety
/// Singleton BSP network bootstrap, before runtime RX/DHCP work is admitted.
pub(crate) unsafe fn init() {
    backend_admission::r4os_rust_net_admission_reset();
    packet_pool::r4os_rust_net_packet_pool_reset();
    cache::r4os_rust_net_cache_reset();
    dhcp_messages::r4os_rust_dhcp_messages_reset();
    unsafe {
        app_ipv4::r4os_rust_app_ipv4_reset();
        udp_sockets::r4os_rust_udp_sockets_reset();
    }
    adapters::r4os_rust_net_adapters_reset();
    protocol_runtime::r4os_rust_net_protocol_runtime_reset();
    unsafe {
        rx_runtime::r4os_rust_net_rx_runtime_reset();
        frames::r4os_rust_frames_reset(&FRAME);
    }
    protocol_stats::r4os_rust_net_protocol_stats_reset();
    config::reset();
    unsafe {
        dhcp_client::r4os_rust_dhcp_client_reset(&DHCP);
        dns_client::r4os_rust_dns_client_reset(&DNS);
        tcp::r4os_rust_net_tcp_reset();
        tcp_client::r4os_rust_tcp_client_reset(&TCP);
    }
    tcp_rto::r4os_rust_tcp_rto_reset(true);
    transmit::r4os_rust_net_tx_reset();
    cleanup::r4os_rust_net_cleanup_reset();
}
/// # Safety
/// Config is the immutable resident boot configuration for this generation.
pub(crate) unsafe fn configure_injection(config: *const c_void) {
    let config = config.cast();
    let offers =
        unsafe { boot_config::option_value(config, b"NET", b"test_drop_offer") }.unwrap_or(b"");
    let acks =
        unsafe { boot_config::option_value(config, b"NET", b"test_drop_ack") }.unwrap_or(b"");
    unsafe {
        dhcp_messages::r4os_rust_dhcp_test_injection(
            Text {
                pointer: offers.as_ptr(),
                len: offers.len(),
            },
            Text {
                pointer: acks.as_ptr(),
                len: acks.len(),
            },
        );
    }
}
