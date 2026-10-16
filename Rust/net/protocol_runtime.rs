//! Sole R4P accounting and TCP wire-result labels. Queries combine copied
//! counters with the real passive loader/SerialLink state after admission.
use super::{r4p_contract as wire, serial_link, tcp, tcp_owner};
use crate::{fs::vfs_types::Bytes, loader::r4p, memory::owner::Guard, sched::sync_types::Text};
use core::cell::UnsafeCell;
#[repr(u8)]
#[derive(Clone, Copy)]
enum Kind {
    Ethernet,
    Arp,
    Ipv4,
    Icmp,
    Udp,
    Dhcp,
    Dns,
    Tcp,
    SerialLink,
}
impl Kind {
    fn from_code(value: u8) -> Self {
        match value {
            0 => Self::Ethernet,
            1 => Self::Arp,
            2 => Self::Ipv4,
            3 => Self::Icmp,
            4 => Self::Udp,
            5 => Self::Dhcp,
            6 => Self::Dns,
            7 => Self::Tcp,
            8 => Self::SerialLink,
            _ => panic!("Exact network protocol kind"),
        }
    }
    fn role(self) -> &'static [u8] {
        match self {
            Self::Ethernet => b"net.ethernet",
            Self::Arp => b"net.arp",
            Self::Ipv4 => b"net.ipv4",
            Self::Icmp => b"net.icmp",
            Self::Udp => b"net.udp",
            Self::Dhcp => b"net.dhcp",
            Self::Dns => b"net.dns",
            Self::Tcp => b"net.tcp",
            Self::SerialLink => b"net.serial_link",
        }
    }
}
#[derive(Clone, Copy)]
enum Metric {
    Rx,
    Tx,
    Control,
    Build,
    Classify,
    DispatchFailures,
}
#[derive(Clone, Copy)]
struct Counters {
    rx: u64,
    tx: u64,
    control: u64,
    build: u64,
    classify: u64,
    failures: u64,
}
impl Counters {
    const EMPTY: Self = Self {
        rx: 0,
        tx: 0,
        control: 0,
        build: 0,
        classify: 0,
        failures: 0,
    };
}
struct State {
    protocols: [Counters; 8],
    tcp_flags: u16,
    tcp_error: Text,
}
impl State {
    const EMPTY: Self = Self {
        protocols: [Counters::EMPTY; 8],
        tcp_flags: 0,
        tcp_error: Text::literal(b"none"),
    };
}
struct Storage(UnsafeCell<State>);
// SAFETY: resident state uses short network metadata admission. Loader and
// SerialLink queries, task/TCP admission and caller output run after release.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State::EMPTY));
fn note(kind: Kind, metric: Metric, amount: u64) {
    let _guard = Guard::acquire(2);
    unsafe {
        let row = &mut (*STATE.0.get()).protocols[kind as usize];
        match metric {
            Metric::Rx => row.rx += amount,
            Metric::Tx => row.tx += amount,
            Metric::Control => row.control += amount,
            Metric::Build => row.build += amount,
            Metric::Classify => row.classify += amount,
            Metric::DispatchFailures => row.failures += amount,
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ProtocolRuntimeStats {
    pub active_r4p: bool,
    pub r4p_state: u8,
    pub builtin_fallback: bool,
    pub fallback_policy: u8,
    pub fallback_decision: u8,
    pub r4p_rx: u64,
    pub r4p_tx: u64,
    pub r4p_control: u64,
    pub r4p_build: u64,
    pub r4p_classify: u64,
    pub dispatch_failures: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct R4pRuntimeStatus {
    pub protocol_count: u32,
    pub active: u32,
    pub missing: u32,
    pub r4p_rx: u64,
    pub r4p_tx: u64,
    pub r4p_control: u64,
    pub r4p_build: u64,
    pub r4p_classify: u64,
    pub dispatch_failures: u64,
}
fn snapshot(kind: Kind) -> ProtocolRuntimeStats {
    let counts = if matches!(kind, Kind::SerialLink) {
        let s = serial_link::snapshot();
        Counters {
            rx: s.r4p_parse,
            tx: s.r4p_build,
            control: s.r4p_self,
            build: s.r4p_build,
            classify: 0,
            failures: s.r4p_dispatch_failures,
        }
    } else {
        let _guard = Guard::acquire(2);
        unsafe { (*STATE.0.get()).protocols[kind as usize] }
    };
    let role = kind.role();
    let active = unsafe {
        r4p::r4os_rust_r4p_status(Bytes {
            data: role.as_ptr(),
            len: role.len(),
        })
    };
    ProtocolRuntimeStats {
        active_r4p: active.active_r4p,
        r4p_state: active.state,
        builtin_fallback: active.builtin_fallback,
        fallback_policy: 0,
        fallback_decision: 0,
        r4p_rx: counts.rx,
        r4p_tx: counts.tx,
        r4p_control: counts.control,
        r4p_build: counts.build,
        r4p_classify: counts.classify,
        dispatch_failures: counts.failures,
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_protocol_runtime_reset() {
    let _guard = Guard::acquire(2);
    unsafe {
        STATE.0.get().write(State::EMPTY);
    }
}
/// # Safety
/// Writable paired output. Counter admission never spans external owner queries/output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_protocol_runtime(kind: u8, out: *mut ProtocolRuntimeStats) {
    let value = snapshot(Kind::from_code(kind));
    unsafe {
        out.write(value);
    }
}
/// # Safety
/// Writable paired output. Every role query remains passive as before.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_r4p_runtime(out: *mut R4pRuntimeStatus) {
    let mut s = R4pRuntimeStatus {
        protocol_count: 9,
        active: 0,
        missing: 0,
        r4p_rx: 0,
        r4p_tx: 0,
        r4p_control: 0,
        r4p_build: 0,
        r4p_classify: 0,
        dispatch_failures: 0,
    };
    for code in 0..9 {
        let row = snapshot(Kind::from_code(code));
        if row.active_r4p {
            s.active += 1;
        } else {
            s.missing += 1;
        }
        s.r4p_rx += row.r4p_rx;
        s.r4p_tx += row.r4p_tx;
        s.r4p_control += row.r4p_control;
        s.r4p_build += row.r4p_build;
        s.r4p_classify += row.r4p_classify;
        s.dispatch_failures += row.dispatch_failures;
    }
    unsafe {
        out.write(s);
    }
}
/// # Safety
/// Writable paired text; borrows immutable resident protocol-role metadata.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_protocol_role(kind: u8, out: *mut Text) {
    unsafe {
        out.write(Text::literal(Kind::from_code(kind).role()));
    }
}
/// # Safety
/// Retained immutable error text exactly as in the original metadata contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_wire_error_set(reason: Text) {
    let _guard = Guard::acquire(2);
    unsafe {
        (*STATE.0.get()).tcp_error = reason;
    }
}
/// # Safety
/// Writable paired text output; no output write spans metadata admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_wire_error(out: *mut Text) {
    let text = {
        let _guard = Guard::acquire(2);
        unsafe { (*STATE.0.get()).tcp_error }
    };
    unsafe {
        out.write(text);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_tcp_wire_flags_set(flags: u16) {
    let _guard = Guard::acquire(2);
    unsafe {
        (*STATE.0.get()).tcp_flags = flags;
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_tcp_wire_flags() -> u16 {
    let _guard = Guard::acquire(2);
    unsafe { (*STATE.0.get()).tcp_flags }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_tcp_wire_result(result: i32, flags: u16) {
    let label: &[u8] = match result {
        wire::TCP_RESULT_OK => b"none",
        wire::TCP_RESULT_NOT_TCP => b"not-tcp",
        wire::TCP_RESULT_NO_CONNECTION => b"no-conn",
        wire::TCP_RESULT_BAD_STATE => b"state",
        wire::TCP_RESULT_BUFFER_SMALL => b"buffer",
        wire::TCP_RESULT_SHORT => b"short",
        wire::TCP_RESULT_CHECKSUM => b"checksum",
        _ => b"error",
    };
    let _guard = Guard::acquire(2);
    unsafe {
        (*STATE.0.get()).tcp_flags = flags;
        (*STATE.0.get()).tcp_error = Text::literal(label);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_tcp_wire_reject(result: i32) {
    let Some(_guard) = tcp_owner::Guard::acquire() else {
        return;
    };
    unsafe {
        match result {
            wire::TCP_RESULT_SHORT => tcp::record_malformed(Text::literal(b"short")),
            wire::TCP_RESULT_CHECKSUM => tcp::record_checksum_error(),
            _ => {}
        }
    }
}

// Typed compatibility counter entries and paired record assertions follow.

#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_ethernet_r4p_rx(amount: u64) {
    note(Kind::Ethernet, Metric::Rx, amount);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_ethernet_r4p_tx(amount: u64) {
    note(Kind::Ethernet, Metric::Tx, amount);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_ethernet_r4p_build(amount: u64) {
    note(Kind::Ethernet, Metric::Build, amount);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_ethernet_dispatch_failures(amount: u64) {
    note(Kind::Ethernet, Metric::DispatchFailures, amount);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_arp_r4p_rx(amount: u64) {
    note(Kind::Arp, Metric::Rx, amount);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_arp_r4p_tx(amount: u64) {
    note(Kind::Arp, Metric::Tx, amount);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_arp_r4p_build(amount: u64) {
    note(Kind::Arp, Metric::Build, amount);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_arp_dispatch_failures(amount: u64) {
    note(Kind::Arp, Metric::DispatchFailures, amount);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_ipv4_r4p_rx(amount: u64) {
    note(Kind::Ipv4, Metric::Rx, amount);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_ipv4_r4p_tx(amount: u64) {
    note(Kind::Ipv4, Metric::Tx, amount);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_ipv4_r4p_build(amount: u64) {
    note(Kind::Ipv4, Metric::Build, amount);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_ipv4_dispatch_failures(amount: u64) {
    note(Kind::Ipv4, Metric::DispatchFailures, amount);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_icmp_r4p_rx(amount: u64) {
    note(Kind::Icmp, Metric::Rx, amount);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_icmp_r4p_tx(amount: u64) {
    note(Kind::Icmp, Metric::Tx, amount);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_icmp_r4p_build(amount: u64) {
    note(Kind::Icmp, Metric::Build, amount);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_icmp_r4p_classify(amount: u64) {
    note(Kind::Icmp, Metric::Classify, amount);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_icmp_dispatch_failures(amount: u64) {
    note(Kind::Icmp, Metric::DispatchFailures, amount);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_udp_r4p_rx(amount: u64) {
    note(Kind::Udp, Metric::Rx, amount);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_udp_r4p_tx(amount: u64) {
    note(Kind::Udp, Metric::Tx, amount);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_udp_r4p_build(amount: u64) {
    note(Kind::Udp, Metric::Build, amount);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_udp_dispatch_failures(amount: u64) {
    note(Kind::Udp, Metric::DispatchFailures, amount);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_dhcp_r4p_rx(amount: u64) {
    note(Kind::Dhcp, Metric::Rx, amount);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_dhcp_r4p_build(amount: u64) {
    note(Kind::Dhcp, Metric::Build, amount);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_dhcp_dispatch_failures(amount: u64) {
    note(Kind::Dhcp, Metric::DispatchFailures, amount);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_dns_r4p_rx(amount: u64) {
    note(Kind::Dns, Metric::Rx, amount);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_dns_r4p_build(amount: u64) {
    note(Kind::Dns, Metric::Build, amount);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_dns_dispatch_failures(amount: u64) {
    note(Kind::Dns, Metric::DispatchFailures, amount);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_tcp_r4p_rx(amount: u64) {
    note(Kind::Tcp, Metric::Rx, amount);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_tcp_r4p_tx(amount: u64) {
    note(Kind::Tcp, Metric::Tx, amount);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_tcp_r4p_control(amount: u64) {
    note(Kind::Tcp, Metric::Control, amount);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_tcp_r4p_build(amount: u64) {
    note(Kind::Tcp, Metric::Build, amount);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_note_tcp_dispatch_failures(amount: u64) {
    note(Kind::Tcp, Metric::DispatchFailures, amount);
}

const _: () = {
    assert!(core::mem::size_of::<ProtocolRuntimeStats>() == 56);
    assert!(core::mem::align_of::<ProtocolRuntimeStats>() == 8);
    assert!(core::mem::offset_of!(ProtocolRuntimeStats, active_r4p) == 0);
    assert!(core::mem::offset_of!(ProtocolRuntimeStats, r4p_state) == 1);
    assert!(core::mem::offset_of!(ProtocolRuntimeStats, builtin_fallback) == 2);
    assert!(core::mem::offset_of!(ProtocolRuntimeStats, fallback_policy) == 3);
    assert!(core::mem::offset_of!(ProtocolRuntimeStats, fallback_decision) == 4);
    assert!(core::mem::offset_of!(ProtocolRuntimeStats, r4p_rx) == 8);
    assert!(core::mem::offset_of!(ProtocolRuntimeStats, r4p_tx) == 16);
    assert!(core::mem::offset_of!(ProtocolRuntimeStats, r4p_control) == 24);
    assert!(core::mem::offset_of!(ProtocolRuntimeStats, r4p_build) == 32);
    assert!(core::mem::offset_of!(ProtocolRuntimeStats, r4p_classify) == 40);
    assert!(core::mem::offset_of!(ProtocolRuntimeStats, dispatch_failures) == 48);
};
const _: () = {
    assert!(core::mem::size_of::<R4pRuntimeStatus>() == 64);
    assert!(core::mem::align_of::<R4pRuntimeStatus>() == 8);
    assert!(core::mem::offset_of!(R4pRuntimeStatus, protocol_count) == 0);
    assert!(core::mem::offset_of!(R4pRuntimeStatus, active) == 4);
    assert!(core::mem::offset_of!(R4pRuntimeStatus, missing) == 8);
    assert!(core::mem::offset_of!(R4pRuntimeStatus, r4p_rx) == 16);
    assert!(core::mem::offset_of!(R4pRuntimeStatus, r4p_tx) == 24);
    assert!(core::mem::offset_of!(R4pRuntimeStatus, r4p_control) == 32);
    assert!(core::mem::offset_of!(R4pRuntimeStatus, r4p_build) == 40);
    assert!(core::mem::offset_of!(R4pRuntimeStatus, r4p_classify) == 48);
    assert!(core::mem::offset_of!(R4pRuntimeStatus, dispatch_failures) == 56);
};
