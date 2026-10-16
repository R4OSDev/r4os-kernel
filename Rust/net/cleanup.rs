//! Sole network cleanup publication and existing diagnostic metadata owner.
//! Resource retirement is performed by its exact native socket/TCP/DHCP/DNS
//! owners, outside short metadata admission. Scoped diagnostics retire only
//! their own handles; productive cleanup remains system-wide.
use super::{
    adapters, backend_admission, dhcp_client, dns_client, protocol_stats, rx_runtime, tcp, tcp_rto,
    udp_sockets,
};
use crate::{memory::owner::Guard, sched::sync_types::Text};
use core::{cell::UnsafeCell, mem::MaybeUninit, slice};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NativeCleanupStatus {
    pub runs: u64,
    pub link_down_cleanups: u64,
    pub adapter_reset_cleanups: u64,
    pub adapter_unregister_cleanups: u64,
    pub service_restart_cleanups: u64,
    pub poweroff_cleanups: u64,
    pub reboot_cleanups: u64,
    pub udp_sockets_closed: u64,
    pub tcp_connections_aborted: u64,
    pub tcp_listeners_closed: u64,
    pub dhcp_operations_cancelled: u64,
    pub dns_operations_cancelled: u64,
    pub last_udp_closed: u32,
    pub last_tcp_connections: u32,
    pub last_tcp_listeners: u32,
    pub last_reason: Text,
}
impl NativeCleanupStatus {
    const EMPTY: Self = Self {
        runs: 0,
        link_down_cleanups: 0,
        adapter_reset_cleanups: 0,
        adapter_unregister_cleanups: 0,
        service_restart_cleanups: 0,
        poweroff_cleanups: 0,
        reboot_cleanups: 0,
        udp_sockets_closed: 0,
        tcp_connections_aborted: 0,
        tcp_listeners_closed: 0,
        dhcp_operations_cancelled: 0,
        dns_operations_cancelled: 0,
        last_udp_closed: 0,
        last_tcp_connections: 0,
        last_tcp_listeners: 0,
        last_reason: Text::literal(b"none"),
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NativeDiagnosticCounters {
    pub packet_corpus_tests: u64,
    pub packet_corpus_cases: u64,
    pub negative_path_tests: u64,
    pub negative_path_cases: u64,
    pub driver_lifecycle_tests: u64,
    pub driver_lifecycle_cases: u64,
    pub environment_contract_tests: u64,
    pub environment_contract_cases: u64,
    pub limit_contract_tests: u64,
    pub limit_contract_cases: u64,
    pub power_lifecycle_tests: u64,
    pub power_lifecycle_cases: u64,
    pub link_up: bool,
}
impl NativeDiagnosticCounters {
    const EMPTY: Self = Self {
        packet_corpus_tests: 0,
        packet_corpus_cases: 0,
        negative_path_tests: 0,
        negative_path_cases: 0,
        driver_lifecycle_tests: 0,
        driver_lifecycle_cases: 0,
        environment_contract_tests: 0,
        environment_contract_cases: 0,
        limit_contract_tests: 0,
        limit_contract_cases: 0,
        power_lifecycle_tests: 0,
        power_lifecycle_cases: 0,
        link_up: true,
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DiagnosticCleanupScope {
    pub udp_handle: u32,
    pub tcp_connection_id: u32,
    pub tcp_listener_port: u16,
}

struct Metadata {
    cleanup: NativeCleanupStatus,
    diagnostics: NativeDiagnosticCounters,
}
impl Metadata {
    const EMPTY: Self = Self {
        cleanup: NativeCleanupStatus::EMPTY,
        diagnostics: NativeDiagnosticCounters::EMPTY,
    };
}
struct Storage(UnsafeCell<Metadata>);
// SAFETY: all publication/counter accesses hold the short network metadata owner.
unsafe impl Sync for Storage {}
static STORAGE: Storage = Storage(UnsafeCell::new(Metadata::EMPTY));
fn metadata<T>(f: impl FnOnce(&mut Metadata) -> T) -> T {
    let _guard = Guard::acquire(2);
    unsafe { f(&mut *STORAGE.0.get()) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_cleanup_reset() {
    metadata(|s| {
        let link = s.diagnostics.link_up;
        *s = Metadata::EMPTY;
        s.diagnostics.link_up = link;
    });
}
fn record(
    reason: Text,
    udp: u32,
    tcp: tcp::CleanupResult,
    dhcp: bool,
    dns: bool,
) -> NativeCleanupStatus {
    // Label comparison occurs before admission. The retained pointer keeps the
    // original stable kernel-string lifetime, never a temporary copied label.
    let bytes = if reason.len == 0 {
        &[][..]
    } else {
        unsafe { slice::from_raw_parts(reason.pointer, reason.len) }
    };
    let kind = match bytes {
        b"link-down" => 1,
        b"adapter-reset" => 2,
        b"adapter-unregister" => 3,
        b"service-restart" => 4,
        b"poweroff" => 5,
        b"reboot" => 6,
        _ => 0,
    };
    metadata(|s| {
        let s = &mut s.cleanup;
        s.runs += 1;
        s.last_reason = reason;
        match kind {
            1 => s.link_down_cleanups += 1,
            2 => s.adapter_reset_cleanups += 1,
            3 => s.adapter_unregister_cleanups += 1,
            4 => s.service_restart_cleanups += 1,
            5 => s.poweroff_cleanups += 1,
            6 => s.reboot_cleanups += 1,
            _ => {}
        }
        s.udp_sockets_closed += u64::from(udp);
        s.tcp_connections_aborted += u64::from(tcp.connections);
        s.tcp_listeners_closed += u64::from(tcp.listeners);
        if dhcp {
            s.dhcp_operations_cancelled += 1;
        }
        if dns {
            s.dns_operations_cancelled += 1;
        }
        s.last_udp_closed = udp;
        s.last_tcp_connections = tcp.connections;
        s.last_tcp_listeners = tcp.listeners;
        *s
    })
}
pub(super) unsafe fn all(reason: Text) -> NativeCleanupStatus {
    let closed = udp_sockets::r4os_rust_udp_sockets_close_all();
    if closed != 0 {
        unsafe {
            protocol_stats::r4os_rust_net_udp_owned_error(reason);
        }
    }
    let mut tcp = MaybeUninit::uninit();
    unsafe {
        tcp::r4os_rust_net_tcp_abort_all(reason.pointer, reason.len, tcp.as_mut_ptr());
    }
    let tcp = unsafe { tcp.assume_init() };
    tcp_rto::r4os_rust_tcp_rto_reset(false);
    let dhcp = unsafe { dhcp_client::r4os_rust_dhcp_client_cancel(reason) };
    let dns = unsafe { dns_client::r4os_rust_dns_client_cancel(reason) };
    record(reason, closed, tcp, dhcp, dns)
}
pub(super) unsafe fn scoped(reason: Text, scope: DiagnosticCleanupScope) -> NativeCleanupStatus {
    let mut closed = 0;
    if scope.udp_handle != 0 {
        let mut close_reason = MaybeUninit::uninit();
        let result = unsafe {
            udp_sockets::r4os_rust_udp_socket_close(scope.udp_handle, close_reason.as_mut_ptr())
        };
        unsafe {
            protocol_stats::r4os_rust_net_udp_owned_error(close_reason.assume_init());
        }
        if result == 0 {
            closed = 1;
            unsafe {
                protocol_stats::r4os_rust_net_udp_owned_error(reason);
            }
        }
    }
    let mut tcp = tcp::CleanupResult {
        connections: 0,
        listeners: 0,
    };
    if scope.tcp_connection_id != 0
        && !unsafe { tcp::r4os_rust_net_tcp_closed(scope.tcp_connection_id) }
    {
        unsafe {
            tcp::r4os_rust_net_tcp_abort(scope.tcp_connection_id, reason.pointer, reason.len);
        }
        tcp.connections = 1;
    }
    if scope.tcp_listener_port != 0
        && unsafe { tcp::r4os_rust_net_tcp_has_listener(scope.tcp_listener_port) }
    {
        unsafe {
            tcp::r4os_rust_net_tcp_close_listener(scope.tcp_listener_port);
        }
        tcp.listeners = 1;
    }
    if tcp.connections != 0 || tcp.listeners != 0 {
        unsafe {
            tcp::r4os_rust_net_tcp_set_error(reason.pointer, reason.len);
        }
    }
    let dhcp = unsafe { dhcp_client::r4os_rust_dhcp_client_cancel(reason) };
    let dns = unsafe { dns_client::r4os_rust_dns_client_cancel(reason) };
    record(reason, closed, tcp, dhcp, dns)
}
/// # Safety
/// Stable original reason label and exclusive paired output through retirement.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_cleanup_all(reason: Text, out: *mut NativeCleanupStatus) {
    let value = unsafe { all(reason) };
    unsafe {
        out.write(value);
    }
}
/// # Safety
/// Resident scope, stable reason label and writable output through retirement.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_cleanup_scoped(
    reason: Text,
    scope: *const DiagnosticCleanupScope,
    out: *mut NativeCleanupStatus,
) {
    let scope = unsafe { *scope };
    let value = unsafe { scoped(reason, scope) };
    unsafe {
        out.write(value);
    }
}
/// # Safety
/// Exact writable paired output, copied after metadata admission ends.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_cleanup_status(out: *mut NativeCleanupStatus) {
    let value = metadata(|s| s.cleanup);
    unsafe {
        out.write(value);
    }
}
/// # Safety
/// Exact writable paired output, copied after metadata admission ends.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_diagnostic_counters(out: *mut NativeDiagnosticCounters) {
    let value = metadata(|s| s.diagnostics);
    unsafe {
        out.write(value);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_diagnostic_link_set(value: bool) {
    metadata(|s| s.diagnostics.link_up = value);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_diagnostic_link() -> bool {
    metadata(|s| s.diagnostics.link_up)
}
/// # Safety
/// The terminal reason has the existing stable immutable kernel-string lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_cleanup_transition(reason: Text) -> bool {
    if !backend_admission::r4os_rust_net_transition_begin() {
        return false;
    }
    rx_runtime::r4os_rust_net_rx_runtime_cancel();
    unsafe {
        all(reason);
    }
    for index in 0..adapters::r4os_rust_net_adapter_count() {
        adapters::r4os_rust_net_adapter_lifecycle(index, 5);
    }
    true
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_packet_corpus(cases: u64) {
    metadata(|s| {
        s.diagnostics.packet_corpus_tests += 1;
        s.diagnostics.packet_corpus_cases += cases;
    });
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_negative_path(cases: u64) {
    metadata(|s| {
        s.diagnostics.negative_path_tests += 1;
        s.diagnostics.negative_path_cases += cases;
    });
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_environment_contract(cases: u64) {
    metadata(|s| {
        s.diagnostics.environment_contract_tests += 1;
        s.diagnostics.environment_contract_cases += cases;
    });
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_limit_contract(cases: u64) {
    metadata(|s| {
        s.diagnostics.limit_contract_tests += 1;
        s.diagnostics.limit_contract_cases += cases;
    });
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_power_lifecycle(cases: u64) {
    metadata(|s| {
        s.diagnostics.power_lifecycle_tests += 1;
        s.diagnostics.power_lifecycle_cases += cases;
    });
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_driver_lifecycle() {
    metadata(|s| {
        let s = &mut s.diagnostics;
        s.driver_lifecycle_tests += 1;
        s.driver_lifecycle_cases += 4;
    });
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_driver_environment() {
    metadata(|s| {
        let s = &mut s.diagnostics;
        s.driver_lifecycle_tests = s.driver_lifecycle_tests.wrapping_add(1);
        s.driver_lifecycle_cases = s.driver_lifecycle_cases.wrapping_add(8);
    });
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_driver_limit() {
    metadata(|s| {
        let s = &mut s.diagnostics;
        s.driver_lifecycle_tests = s.driver_lifecycle_tests.wrapping_add(1);
        s.driver_lifecycle_cases = s.driver_lifecycle_cases.wrapping_add(7);
    });
}

const _: () = {
    assert!(core::mem::size_of::<NativeCleanupStatus>() == 128);
    assert!(core::mem::align_of::<NativeCleanupStatus>() == 8);
    assert!(core::mem::offset_of!(NativeCleanupStatus, runs) == 0);
    assert!(core::mem::offset_of!(NativeCleanupStatus, link_down_cleanups) == 8);
    assert!(core::mem::offset_of!(NativeCleanupStatus, adapter_reset_cleanups) == 16);
    assert!(core::mem::offset_of!(NativeCleanupStatus, adapter_unregister_cleanups) == 24);
    assert!(core::mem::offset_of!(NativeCleanupStatus, service_restart_cleanups) == 32);
    assert!(core::mem::offset_of!(NativeCleanupStatus, poweroff_cleanups) == 40);
    assert!(core::mem::offset_of!(NativeCleanupStatus, reboot_cleanups) == 48);
    assert!(core::mem::offset_of!(NativeCleanupStatus, udp_sockets_closed) == 56);
    assert!(core::mem::offset_of!(NativeCleanupStatus, tcp_connections_aborted) == 64);
    assert!(core::mem::offset_of!(NativeCleanupStatus, tcp_listeners_closed) == 72);
    assert!(core::mem::offset_of!(NativeCleanupStatus, dhcp_operations_cancelled) == 80);
    assert!(core::mem::offset_of!(NativeCleanupStatus, dns_operations_cancelled) == 88);
    assert!(core::mem::offset_of!(NativeCleanupStatus, last_udp_closed) == 96);
    assert!(core::mem::offset_of!(NativeCleanupStatus, last_tcp_connections) == 100);
    assert!(core::mem::offset_of!(NativeCleanupStatus, last_tcp_listeners) == 104);
    assert!(core::mem::offset_of!(NativeCleanupStatus, last_reason) == 112);
};
const _: () = {
    assert!(core::mem::size_of::<NativeDiagnosticCounters>() == 104);
    assert!(core::mem::align_of::<NativeDiagnosticCounters>() == 8);
    assert!(core::mem::offset_of!(NativeDiagnosticCounters, packet_corpus_tests) == 0);
    assert!(core::mem::offset_of!(NativeDiagnosticCounters, packet_corpus_cases) == 8);
    assert!(core::mem::offset_of!(NativeDiagnosticCounters, negative_path_tests) == 16);
    assert!(core::mem::offset_of!(NativeDiagnosticCounters, negative_path_cases) == 24);
    assert!(core::mem::offset_of!(NativeDiagnosticCounters, driver_lifecycle_tests) == 32);
    assert!(core::mem::offset_of!(NativeDiagnosticCounters, driver_lifecycle_cases) == 40);
    assert!(core::mem::offset_of!(NativeDiagnosticCounters, environment_contract_tests) == 48);
    assert!(core::mem::offset_of!(NativeDiagnosticCounters, environment_contract_cases) == 56);
    assert!(core::mem::offset_of!(NativeDiagnosticCounters, limit_contract_tests) == 64);
    assert!(core::mem::offset_of!(NativeDiagnosticCounters, limit_contract_cases) == 72);
    assert!(core::mem::offset_of!(NativeDiagnosticCounters, power_lifecycle_tests) == 80);
    assert!(core::mem::offset_of!(NativeDiagnosticCounters, power_lifecycle_cases) == 88);
    assert!(core::mem::offset_of!(NativeDiagnosticCounters, link_up) == 96);
};
const _: () = {
    assert!(core::mem::size_of::<DiagnosticCleanupScope>() == 12);
    assert!(core::mem::align_of::<DiagnosticCleanupScope>() == 4);
    assert!(core::mem::offset_of!(DiagnosticCleanupScope, udp_handle) == 0);
    assert!(core::mem::offset_of!(DiagnosticCleanupScope, tcp_connection_id) == 4);
    assert!(core::mem::offset_of!(DiagnosticCleanupScope, tcp_listener_port) == 8);
};
