//! Original kernel-side R4P RX/TX parsing and classification. Each call owns
//! its wire record; it returns effects after module execution without retaining
//! a protocol-statistics reference, network token or shared payload scratch.
use super::{config, operations as ops, r4p_contract as wire};
use crate::sched::sync_types::Text;
use core::ptr;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct WireReadInput {
    pub source_ip: [u8; 4],
    pub dest_ip: [u8; 4],
    pub own_mac: [u8; 6],
    pub protocol: u8,
    pub l4_checksum_valid: bool,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct WireReadReport {
    pub dispatch_failures: u64,
    pub reason: Text,
    pub completed: bool,
    pub accepted: bool,
    pub value: u32,
}
impl WireReadReport {
    const EMPTY: Self = Self {
        dispatch_failures: 0,
        reason: Text::literal(b""),
        completed: false,
        accepted: false,
        value: 0,
    };
    fn fail(&mut self, reason: &'static [u8], count: bool) {
        self.reason = Text::literal(reason);
        if count {
            self.dispatch_failures += 1;
        }
    }
    fn start<T>(&mut self, op: Option<T>) -> Option<T> {
        if op.is_none() {
            self.fail(b"r4p-required", true);
        }
        op
    }
    fn dispatched(&mut self, ok: bool) -> bool {
        if !ok {
            self.fail(b"r4p-dispatch", true);
        }
        ok
    }
}
// SAFETY: this call's private initialized operation and separate readable input.
unsafe fn stage(dst: &mut [u8], data: *const u8, len: usize) -> bool {
    if len > dst.len() {
        return false;
    }
    if len != 0 {
        unsafe {
            ptr::copy_nonoverlapping(data, dst.as_mut_ptr(), len);
        }
    }
    true
}
#[derive(Clone, Copy)]
enum EthernetKind {
    Rx,
    Tx,
    FrameType,
}
unsafe fn ethernet(
    kind: EthernetKind,
    input: WireReadInput,
    data: *const u8,
    len: usize,
) -> (WireReadReport, Option<wire::EthernetFrameOp>) {
    let mut r = WireReadReport::EMPTY;
    let Some(mut op) = r.start(ops::new_ethernet_op()) else {
        return (r, None);
    };
    let opcode = match kind {
        EthernetKind::Rx => {
            op.own_mac = input.own_mac;
            wire::ETHERNET_OP_HANDLE_RX
        }
        EthernetKind::Tx => wire::ETHERNET_OP_HANDLE_TX,
        EthernetKind::FrameType => wire::ETHERNET_OP_FRAME_TYPE,
    };
    if !unsafe { stage(&mut op.frame, data, len) } {
        r.fail(
            if matches!(kind, EthernetKind::Tx) {
                b"tx-size"
            } else {
                b"rx-size"
            },
            true,
        );
        return (r, None);
    }
    op.frame_len = len as u32;
    if !r.dispatched(unsafe { ops::ethernet_dispatch(opcode, &mut op, 0) }) {
        return (r, None);
    }
    r.completed = true;
    r.accepted = op.result == wire::ETHERNET_RESULT_OK;
    r.value = u32::from(op.ethertype);
    (r, Some(op))
}
unsafe fn arp(tx: bool, data: *const u8, len: usize) -> (WireReadReport, Option<wire::ArpOp>) {
    let mut r = WireReadReport::EMPTY;
    let Some(mut op) = r.start(ops::new_arp_op()) else {
        return (r, None);
    };
    if !unsafe { stage(&mut op.frame, data, len) } {
        r.fail(b"r4p-dispatch", false);
        return (r, None);
    }
    op.frame_len = len as u32;
    if !r.dispatched(unsafe {
        ops::arp_dispatch(
            if tx {
                wire::ARP_OP_HANDLE_TX
            } else {
                wire::ARP_OP_HANDLE_RX
            },
            &mut op,
            0,
        )
    }) {
        return (r, None);
    }
    r.completed = true;
    r.accepted = true;
    (r, Some(op))
}
unsafe fn ipv4(tx: bool, data: *const u8, len: usize) -> (WireReadReport, Option<wire::Ipv4Op>) {
    let mut r = WireReadReport::EMPTY;
    let Some(mut op) = r.start(ops::new_ipv4_op()) else {
        return (r, None);
    };
    if !tx {
        op.local_ip = config::settings().local_ip;
    }
    if !unsafe { stage(&mut op.frame, data, len) } {
        r.fail(b"r4p-dispatch", false);
        return (r, None);
    }
    op.frame_len = len as u32;
    if !r.dispatched(unsafe {
        ops::ipv4_dispatch(
            if tx {
                wire::IPV4_OP_HANDLE_TX
            } else {
                wire::IPV4_OP_HANDLE_RX
            },
            &mut op,
            0,
        )
    }) {
        return (r, None);
    }
    r.completed = true;
    r.accepted = op.result == wire::IPV4_RESULT_OK;
    (r, Some(op))
}
#[derive(Clone, Copy)]
enum IcmpKind {
    Rx,
    Tx,
    EchoRequest,
}
unsafe fn icmp(
    kind: IcmpKind,
    input: WireReadInput,
    data: *const u8,
    len: usize,
) -> (WireReadReport, Option<wire::IcmpOp>) {
    let mut r = WireReadReport::EMPTY;
    if !matches!(kind, IcmpKind::EchoRequest) && input.protocol != 1 {
        return (r, None);
    }
    let Some(mut op) = r.start(ops::new_icmp_op()) else {
        return (r, None);
    };
    if !unsafe { stage(&mut op.payload, data, len) } {
        r.fail(b"r4p-dispatch", false);
        return (r, None);
    }
    op.payload_len = len as u32;
    let opcode = match kind {
        IcmpKind::Rx => wire::ICMP_OP_HANDLE_RX,
        IcmpKind::Tx => wire::ICMP_OP_HANDLE_TX,
        IcmpKind::EchoRequest => wire::ICMP_OP_IS_ECHO_REQUEST,
    };
    if !r.dispatched(unsafe { ops::icmp_dispatch(opcode, &mut op, 0) }) {
        return (r, None);
    }
    r.completed = true;
    r.accepted = op.result == wire::ICMP_RESULT_OK;
    r.value = u32::from(op.flags & wire::ICMP_FLAG_ECHO_REQUEST != 0);
    (r, Some(op))
}
unsafe fn udp(
    tx: bool,
    input: WireReadInput,
    data: *const u8,
    len: usize,
) -> (WireReadReport, Option<wire::UdpOp>) {
    let mut r = WireReadReport::EMPTY;
    if input.protocol != 17 {
        return (r, None);
    }
    let Some(mut op) = r.start(ops::new_udp_op()) else {
        return (r, None);
    };
    op.source_ip = input.source_ip;
    op.dest_ip = input.dest_ip;
    if !unsafe { stage(&mut op.datagram, data, len) } {
        r.fail(b"r4p-dispatch", false);
        return (r, None);
    }
    op.datagram_len = len as u32;
    let flags = u32::from(!tx && input.l4_checksum_valid);
    if !r.dispatched(unsafe {
        ops::udp_dispatch(
            if tx {
                wire::UDP_OP_HANDLE_TX
            } else {
                wire::UDP_OP_HANDLE_RX
            },
            &mut op,
            flags,
        )
    }) {
        return (r, None);
    }
    r.completed = true;
    r.accepted = op.result == wire::UDP_RESULT_OK;
    (r, Some(op))
}
pub(super) unsafe fn dhcp(data: *const u8, len: usize) -> (WireReadReport, Option<wire::DhcpOp>) {
    let mut r = WireReadReport::EMPTY;
    let Some(mut op) = r.start(ops::new_dhcp_op()) else {
        return (r, None);
    };
    if !unsafe { stage(&mut op.payload, data, len) } {
        r.fail(b"r4p-dispatch", false);
        return (r, None);
    }
    op.payload_len = len as u32;
    if !r.dispatched(unsafe { ops::dhcp_dispatch(wire::DHCP_OP_HANDLE_MESSAGE, &mut op, 0) }) {
        return (r, None);
    }
    r.completed = true;
    r.accepted = op.result == wire::DHCP_RESULT_OK;
    (r, Some(op))
}
unsafe fn dns(data: *const u8, len: usize) -> (WireReadReport, Option<wire::DnsOp>) {
    let mut r = WireReadReport::EMPTY;
    let Some(mut op) = r.start(ops::new_dns_op()) else {
        return (r, None);
    };
    if !unsafe { stage(&mut op.payload, data, len) } {
        r.fail(b"r4p-dispatch", false);
        return (r, None);
    }
    op.payload_len = len as u32;
    if !r.dispatched(unsafe { ops::dns_dispatch(wire::DNS_OP_HANDLE_RESPONSE, &mut op, 0) }) {
        return (r, None);
    }
    r.completed = true;
    r.accepted = op.result == wire::DNS_RESULT_OK;
    (r, Some(op))
}
unsafe fn tcp(
    tx: bool,
    input: WireReadInput,
    data: *const u8,
    len: usize,
) -> (WireReadReport, Option<wire::TcpOp>) {
    let mut r = WireReadReport::EMPTY;
    if input.protocol != 6 {
        return (r, None);
    }
    let Some(mut op) = r.start(ops::new_tcp_op()) else {
        return (r, None);
    };
    op.source_ip = input.source_ip;
    op.dest_ip = input.dest_ip;
    if !unsafe { stage(&mut op.segment, data, len) } {
        r.fail(b"r4p-dispatch", false);
        return (r, None);
    }
    op.segment_len = len as u32;
    let flags = u32::from(!tx && input.l4_checksum_valid);
    if !r.dispatched(unsafe {
        ops::tcp_dispatch(
            if tx {
                wire::TCP_OP_HANDLE_TX
            } else {
                wire::TCP_OP_HANDLE_RX
            },
            &mut op,
            flags,
        )
    }) {
        return (r, None);
    }
    r.completed = true;
    r.accepted = op.result == wire::TCP_RESULT_OK;
    (r, Some(op))
}

// Paired native packet entries retain copied operation reports.

/// # Safety
/// Exact paired resident/stack inputs and disjoint readable packet. Outputs
/// remain writable through activation/execution; the operation output is valid
/// exactly when completed=true. No metadata reference spans any callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_read_ethernet_rx(
    input: *const WireReadInput,
    data: *const u8,
    len: usize,
    operation: *mut wire::EthernetFrameOp,
    report: *mut WireReadReport,
) {
    let input = unsafe { *input };
    let (value, op) = unsafe { ethernet(EthernetKind::Rx, input, data, len) };
    if let Some(op) = op {
        unsafe {
            operation.write(op);
        }
    }
    unsafe {
        report.write(value);
    }
}

/// # Safety
/// Exact paired resident/stack inputs and disjoint readable packet. Outputs
/// remain writable through activation/execution; the operation output is valid
/// exactly when completed=true. No metadata reference spans any callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_read_ethernet_tx(
    input: *const WireReadInput,
    data: *const u8,
    len: usize,
    operation: *mut wire::EthernetFrameOp,
    report: *mut WireReadReport,
) {
    let input = unsafe { *input };
    let (value, op) = unsafe { ethernet(EthernetKind::Tx, input, data, len) };
    if let Some(op) = op {
        unsafe {
            operation.write(op);
        }
    }
    unsafe {
        report.write(value);
    }
}

/// # Safety
/// Exact paired resident/stack inputs and disjoint readable packet. Outputs
/// remain writable through activation/execution; the operation output is valid
/// exactly when completed=true. No metadata reference spans any callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_read_ethernet_type(
    input: *const WireReadInput,
    data: *const u8,
    len: usize,
    operation: *mut wire::EthernetFrameOp,
    report: *mut WireReadReport,
) {
    let input = unsafe { *input };
    let (value, op) = unsafe { ethernet(EthernetKind::FrameType, input, data, len) };
    if let Some(op) = op {
        unsafe {
            operation.write(op);
        }
    }
    unsafe {
        report.write(value);
    }
}

/// # Safety
/// Exact paired resident/stack inputs and disjoint readable packet. Outputs
/// remain writable through activation/execution; the operation output is valid
/// exactly when completed=true. No metadata reference spans any callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_read_arp_rx(
    _input: *const WireReadInput,
    data: *const u8,
    len: usize,
    operation: *mut wire::ArpOp,
    report: *mut WireReadReport,
) {
    let (value, op) = unsafe { arp(false, data, len) };
    if let Some(op) = op {
        unsafe {
            operation.write(op);
        }
    }
    unsafe {
        report.write(value);
    }
}

/// # Safety
/// Exact paired resident/stack inputs and disjoint readable packet. Outputs
/// remain writable through activation/execution; the operation output is valid
/// exactly when completed=true. No metadata reference spans any callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_read_arp_tx(
    _input: *const WireReadInput,
    data: *const u8,
    len: usize,
    operation: *mut wire::ArpOp,
    report: *mut WireReadReport,
) {
    let (value, op) = unsafe { arp(true, data, len) };
    if let Some(op) = op {
        unsafe {
            operation.write(op);
        }
    }
    unsafe {
        report.write(value);
    }
}

/// # Safety
/// Exact paired resident/stack inputs and disjoint readable packet. Outputs
/// remain writable through activation/execution; the operation output is valid
/// exactly when completed=true. No metadata reference spans any callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_read_ipv4_rx(
    _input: *const WireReadInput,
    data: *const u8,
    len: usize,
    operation: *mut wire::Ipv4Op,
    report: *mut WireReadReport,
) {
    let (value, op) = unsafe { ipv4(false, data, len) };
    if let Some(op) = op {
        unsafe {
            operation.write(op);
        }
    }
    unsafe {
        report.write(value);
    }
}

/// # Safety
/// Exact paired resident/stack inputs and disjoint readable packet. Outputs
/// remain writable through activation/execution; the operation output is valid
/// exactly when completed=true. No metadata reference spans any callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_read_ipv4_tx(
    _input: *const WireReadInput,
    data: *const u8,
    len: usize,
    operation: *mut wire::Ipv4Op,
    report: *mut WireReadReport,
) {
    let (value, op) = unsafe { ipv4(true, data, len) };
    if let Some(op) = op {
        unsafe {
            operation.write(op);
        }
    }
    unsafe {
        report.write(value);
    }
}

/// # Safety
/// Exact paired resident/stack inputs and disjoint readable packet. Outputs
/// remain writable through activation/execution; the operation output is valid
/// exactly when completed=true. No metadata reference spans any callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_read_icmp_rx(
    input: *const WireReadInput,
    data: *const u8,
    len: usize,
    operation: *mut wire::IcmpOp,
    report: *mut WireReadReport,
) {
    let input = unsafe { *input };
    let (value, op) = unsafe { icmp(IcmpKind::Rx, input, data, len) };
    if let Some(op) = op {
        unsafe {
            operation.write(op);
        }
    }
    unsafe {
        report.write(value);
    }
}

/// # Safety
/// Exact paired resident/stack inputs and disjoint readable packet. Outputs
/// remain writable through activation/execution; the operation output is valid
/// exactly when completed=true. No metadata reference spans any callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_read_icmp_tx(
    input: *const WireReadInput,
    data: *const u8,
    len: usize,
    operation: *mut wire::IcmpOp,
    report: *mut WireReadReport,
) {
    let input = unsafe { *input };
    let (value, op) = unsafe { icmp(IcmpKind::Tx, input, data, len) };
    if let Some(op) = op {
        unsafe {
            operation.write(op);
        }
    }
    unsafe {
        report.write(value);
    }
}

/// # Safety
/// Exact paired resident/stack inputs and disjoint readable packet. Outputs
/// remain writable through activation/execution; the operation output is valid
/// exactly when completed=true. No metadata reference spans any callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_read_icmp_echo(
    input: *const WireReadInput,
    data: *const u8,
    len: usize,
    operation: *mut wire::IcmpOp,
    report: *mut WireReadReport,
) {
    let input = unsafe { *input };
    let (value, op) = unsafe { icmp(IcmpKind::EchoRequest, input, data, len) };
    if let Some(op) = op {
        unsafe {
            operation.write(op);
        }
    }
    unsafe {
        report.write(value);
    }
}

/// # Safety
/// Exact paired resident/stack inputs and disjoint readable packet. Outputs
/// remain writable through activation/execution; the operation output is valid
/// exactly when completed=true. No metadata reference spans any callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_read_udp_rx(
    input: *const WireReadInput,
    data: *const u8,
    len: usize,
    operation: *mut wire::UdpOp,
    report: *mut WireReadReport,
) {
    let input = unsafe { *input };
    let (value, op) = unsafe { udp(false, input, data, len) };
    if let Some(op) = op {
        unsafe {
            operation.write(op);
        }
    }
    unsafe {
        report.write(value);
    }
}

/// # Safety
/// Exact paired resident/stack inputs and disjoint readable packet. Outputs
/// remain writable through activation/execution; the operation output is valid
/// exactly when completed=true. No metadata reference spans any callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_read_udp_tx(
    input: *const WireReadInput,
    data: *const u8,
    len: usize,
    operation: *mut wire::UdpOp,
    report: *mut WireReadReport,
) {
    let input = unsafe { *input };
    let (value, op) = unsafe { udp(true, input, data, len) };
    if let Some(op) = op {
        unsafe {
            operation.write(op);
        }
    }
    unsafe {
        report.write(value);
    }
}

/// # Safety
/// Exact paired resident/stack inputs and disjoint readable packet. Outputs
/// remain writable through activation/execution; the operation output is valid
/// exactly when completed=true. No metadata reference spans any callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_read_dhcp(
    _input: *const WireReadInput,
    data: *const u8,
    len: usize,
    operation: *mut wire::DhcpOp,
    report: *mut WireReadReport,
) {
    let (value, op) = unsafe { dhcp(data, len) };
    if let Some(op) = op {
        unsafe {
            operation.write(op);
        }
    }
    unsafe {
        report.write(value);
    }
}

/// # Safety
/// Exact paired resident/stack inputs and disjoint readable packet. Outputs
/// remain writable through activation/execution; the operation output is valid
/// exactly when completed=true. No metadata reference spans any callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_read_dns(
    _input: *const WireReadInput,
    data: *const u8,
    len: usize,
    operation: *mut wire::DnsOp,
    report: *mut WireReadReport,
) {
    let (value, op) = unsafe { dns(data, len) };
    if let Some(op) = op {
        unsafe {
            operation.write(op);
        }
    }
    unsafe {
        report.write(value);
    }
}

/// # Safety
/// Exact paired resident/stack inputs and disjoint readable packet. Outputs
/// remain writable through activation/execution; the operation output is valid
/// exactly when completed=true. No metadata reference spans any callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_read_tcp_rx(
    input: *const WireReadInput,
    data: *const u8,
    len: usize,
    operation: *mut wire::TcpOp,
    report: *mut WireReadReport,
) {
    let input = unsafe { *input };
    let (value, op) = unsafe { tcp(false, input, data, len) };
    if let Some(op) = op {
        unsafe {
            operation.write(op);
        }
    }
    unsafe {
        report.write(value);
    }
}

/// # Safety
/// Exact paired resident/stack inputs and disjoint readable packet. Outputs
/// remain writable through activation/execution; the operation output is valid
/// exactly when completed=true. No metadata reference spans any callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_read_tcp_tx(
    input: *const WireReadInput,
    data: *const u8,
    len: usize,
    operation: *mut wire::TcpOp,
    report: *mut WireReadReport,
) {
    let input = unsafe { *input };
    let (value, op) = unsafe { tcp(true, input, data, len) };
    if let Some(op) = op {
        unsafe {
            operation.write(op);
        }
    }
    unsafe {
        report.write(value);
    }
}

const _: () = {
    assert!(core::mem::size_of::<WireReadInput>() == 16);
    assert!(core::mem::align_of::<WireReadInput>() == 1);
    assert!(core::mem::offset_of!(WireReadInput, source_ip) == 0);
    assert!(core::mem::offset_of!(WireReadInput, dest_ip) == 4);
    assert!(core::mem::offset_of!(WireReadInput, own_mac) == 8);
    assert!(core::mem::offset_of!(WireReadInput, protocol) == 14);
    assert!(core::mem::offset_of!(WireReadInput, l4_checksum_valid) == 15);
};
const _: () = {
    assert!(core::mem::size_of::<WireReadReport>() == 32);
    assert!(core::mem::align_of::<WireReadReport>() == 8);
    assert!(core::mem::offset_of!(WireReadReport, dispatch_failures) == 0);
    assert!(core::mem::offset_of!(WireReadReport, reason) == 8);
    assert!(core::mem::offset_of!(WireReadReport, completed) == 24);
    assert!(core::mem::offset_of!(WireReadReport, accepted) == 25);
    assert!(core::mem::offset_of!(WireReadReport, value) == 28);
};
