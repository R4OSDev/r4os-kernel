//! Typed native calls shared by the original NIPC and R4NET coordinators.
//! IPC slices refer to request-owned resident scratch. Public foreign buffers
//! stay raw at R4NET and at the actual transport owners across faults/waits.
use super::{config, dhcp_client, dns_client, frames, protocol_stats, tcp, tcp_client, telemetry};
use crate::sched::sync_types::Text;
use core::mem::MaybeUninit;
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TxResult {
    Ok,
    NoAdapter,
    LinkDown,
    Busy,
    TooLarge,
    Unsupported,
    BackendError,
}
impl TxResult {
    pub(crate) fn from_code(code: u8) -> Self {
        match code {
            0 => Self::Ok,
            1 => Self::NoAdapter,
            2 => Self::LinkDown,
            3 => Self::Busy,
            4 => Self::TooLarge,
            5 => Self::Unsupported,
            6 => Self::BackendError,
            _ => panic!("Exact native transmit result"),
        }
    }
}
pub(crate) fn capture<T>(f: unsafe extern "C" fn(*mut T)) -> T {
    let mut out = MaybeUninit::uninit();
    unsafe {
        f(out.as_mut_ptr());
        out.assume_init()
    }
}
pub(crate) fn dhcp_stats() -> super::dhcp::Stats {
    dhcp_client::stats()
}
pub(crate) fn dhcp_lease_timing() -> dhcp_client::DhcpLeaseTiming {
    capture(dhcp_client::r4os_rust_dhcp_client_lease_timing)
}
pub(crate) fn dhcp_runtime_status() -> dhcp_client::DhcpRuntimeStatus {
    capture(dhcp_client::r4os_rust_dhcp_client_runtime)
}
pub(crate) fn dhcp_acquire_default_until(deadline: Option<u64>) -> TxResult {
    TxResult::from_code(dhcp_client::r4os_rust_dhcp_client_acquire_default(
        deadline.is_some(),
        deadline.unwrap_or(0),
    ))
}
pub(crate) fn dhcp_renew_default_until(deadline: Option<u64>) -> TxResult {
    TxResult::from_code(dhcp_client::r4os_rust_dhcp_client_renew_default(
        deadline.is_some(),
        deadline.unwrap_or(0),
    ))
}
pub(crate) fn dhcp_release_default() -> TxResult {
    TxResult::from_code(dhcp_client::r4os_rust_dhcp_client_release_default())
}
pub(crate) fn dns_stats() -> super::dns::Stats {
    let mut out = MaybeUninit::uninit();
    unsafe {
        dns_client::r4os_rust_dns_client_stats(out.as_mut_ptr(), true);
        out.assume_init()
    }
}
pub(crate) fn dns_cache_status() -> super::cache::DnsStatus {
    capture(dns_client::r4os_rust_dns_client_cache_status)
}
pub(crate) fn dns_cache_entry_count() -> u32 {
    dns_client::r4os_rust_dns_client_cache_count(false)
}
pub(crate) fn dns_cache_negative_count() -> u32 {
    dns_client::r4os_rust_dns_client_cache_count(true)
}
pub(crate) fn dns_resolve_a(name: &[u8], out: &mut [u8; 4]) -> i32 {
    unsafe { dns_client::r4os_rust_dns_client_resolve(name.as_ptr(), name.len(), out) }
}
pub(crate) fn dns_resolve_a_with_server(name: &[u8], server: [u8; 4], out: &mut [u8; 4]) -> i32 {
    unsafe {
        dns_client::r4os_rust_dns_client_resolve_server(name.as_ptr(), name.len(), &server, out)
    }
}
pub(crate) fn tcp_stats() -> tcp::Stats {
    capture(tcp::r4os_rust_net_tcp_get_stats)
}
pub(crate) fn tcp_summary(out: &mut crate::contract::TcpSummary) {
    unsafe {
        tcp::r4os_rust_net_tcp_summary(out);
    }
}
pub(crate) fn tcp_connection_info(index: u32, out: &mut crate::contract::TcpConnectionInfo) -> i32 {
    unsafe { tcp::r4os_rust_net_tcp_connection_info(index, out) }
}
pub(crate) fn tcp_connect(ip: [u8; 4], port: u16) -> i32 {
    tcp_connect_until(ip, port, None)
}
pub(crate) fn tcp_connect_until(ip: [u8; 4], port: u16, deadline: Option<u64>) -> i32 {
    unsafe {
        tcp_client::r4os_rust_tcp_client_connect(
            &ip,
            port,
            deadline.is_some(),
            deadline.unwrap_or(0),
        )
    }
}
pub(crate) fn tcp_write(id: u32, data: &[u8]) -> i32 {
    unsafe { tcp_client::r4os_rust_tcp_client_write(id, data.as_ptr(), data.len()) }
}
pub(crate) fn tcp_read(id: u32, data: &mut [u8]) -> i32 {
    unsafe { tcp_client::r4os_rust_tcp_client_read(id, data.as_mut_ptr(), data.len()) }
}
pub(crate) fn tcp_read_available(id: u32, data: &mut [u8]) -> i32 {
    unsafe { tcp_client::r4os_rust_tcp_client_read_available(id, data.as_mut_ptr(), data.len()) }
}
pub(crate) fn tcp_accept_read_once(port: u16, data: &mut [u8], id: &mut u32) -> i32 {
    unsafe {
        tcp_client::r4os_rust_tcp_client_accept_read_once(port, data.as_mut_ptr(), data.len(), id)
    }
}
pub(crate) fn tcp_accept_read_on_listener(port: u16, data: &mut [u8], id: &mut u32) -> i32 {
    unsafe { tcp_client::r4os_rust_tcp_client_accept_read(port, data.as_mut_ptr(), data.len(), id) }
}
pub(crate) fn tcp_accept_on_listener(port: u16, id: &mut u32) -> i32 {
    unsafe { tcp_client::r4os_rust_tcp_client_accept(port, id) }
}
pub(crate) fn tcp_accept_poll_on_listener(port: u16, id: &mut u32) -> i32 {
    unsafe { tcp_client::r4os_rust_tcp_client_accept_poll(port, id) }
}
pub(crate) fn tcp_close(id: u32) -> i32 {
    tcp_client::r4os_rust_tcp_client_close(id)
}
pub(crate) fn tcp_abort(id: u32) -> i32 {
    tcp_client::r4os_rust_tcp_client_abort(id)
}
pub(crate) fn tcp_retransmit_last(id: u32) -> i32 {
    tcp_client::r4os_rust_tcp_client_retransmit_last(id)
}
pub(crate) fn tcp_listen(port: u16) -> bool {
    unsafe { tcp::r4os_rust_net_tcp_listen(port) }
}
pub(crate) fn tcp_has_listener(port: u16) -> bool {
    unsafe { tcp::r4os_rust_net_tcp_has_listener(port) }
}
pub(crate) fn tcp_close_listener(port: u16) {
    unsafe {
        tcp::r4os_rust_net_tcp_close_listener(port);
    }
}
pub(crate) fn tcp_poll_service() {
    unsafe {
        super::rx_runtime::r4os_rust_net_rx_runtime_service_poll(32, frames::peers().rx);
    }
}
pub(crate) fn udp_bind(port: u16) -> i32 {
    frames::r4os_rust_frames_udp_bind(port)
}
pub(crate) fn udp_close(id: u32) -> i32 {
    frames::r4os_rust_frames_udp_close(id)
}
pub(crate) fn udp_send_to(id: u32, ip: [u8; 4], port: u16, data: &[u8]) -> TxResult {
    TxResult::from_code(unsafe {
        frames::r4os_rust_frames_udp_send(id, &ip, port, data.as_ptr(), data.len())
    })
}
pub(crate) fn udp_recv_from(id: u32, info: &mut super::udp_sockets::Info, data: &mut [u8]) -> i32 {
    unsafe { frames::r4os_rust_frames_udp_recv(id, info, data.as_mut_ptr(), data.len()) }
}
pub(crate) fn udp_status() -> telemetry::NativeUdpStatus {
    capture(telemetry::r4os_rust_net_telemetry_udp)
}
pub(crate) fn local_ip() -> [u8; 4] {
    config::settings().local_ip
}
pub(crate) fn tx_result_name(result: TxResult) -> &'static [u8] {
    match result {
        TxResult::Ok => b"ok",
        TxResult::NoAdapter => b"no-adapter",
        TxResult::LinkDown => b"link-down",
        TxResult::Busy => b"busy",
        TxResult::TooLarge => b"too-large",
        TxResult::Unsupported => b"unsupported",
        TxResult::BackendError => b"backend-error",
    }
}
pub(crate) fn dns_result_name(result: i32) -> &'static [u8] {
    match result {
        0 => b"ok",
        -1 => b"short",
        -2 => b"header",
        -3 => b"qname",
        -4 => b"question",
        -5 => b"aname",
        -6 => b"answer",
        -7 => b"atype",
        -8 => b"buffer-small",
        -9 => b"name",
        -10 => b"nxdomain",
        -11 => b"timeout",
        -12 => b"no-server",
        -13 => b"tx-error",
        _ => b"unknown",
    }
}
pub(crate) fn cleanup_status() -> super::cleanup::NativeCleanupStatus {
    capture(super::cleanup::r4os_rust_net_cleanup_status)
}
pub(crate) fn udp_error() -> Text {
    capture(protocol_stats::r4os_rust_net_udp_owned_stats).last_error
}
