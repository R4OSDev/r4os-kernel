//! Typed preparation and dispatch of the existing canonical network R4P
//! operations. All wire objects are caller-owned; no network metadata owner
//! or resident reference spans activation, protocol execution or caller copies.
use super::r4p_contract as wire;
use crate::{contract, fs::vfs_types::Bytes, loader::r4p};
use core::{mem, ptr};
fn role(name: &'static [u8]) -> Bytes {
    Bytes {
        data: name.as_ptr(),
        len: name.len(),
    }
}
fn dispatch<T>(name: &'static [u8], opcode: u32, op: *mut T, flags: u32) -> i32 {
    let mut buffer = contract::ProtocolBuffer {
        data: op.cast(),
        len: mem::size_of::<T>() as u32,
        capacity: mem::size_of::<T>() as u32,
        flags,
        reserved: 0,
    };
    unsafe { r4p::r4os_rust_r4p_dispatch(role(name), opcode, &raw const buffer, &raw mut buffer) }
}
pub(super) fn new_ethernet_op() -> Option<wire::EthernetFrameOp> {
    if unsafe { r4p::r4os_rust_r4p_has_active(role(b"net.ethernet")) } {
        Some(wire::EthernetFrameOp::default())
    } else {
        None
    }
}
/// # Safety
/// Exact writable canonical wire record; activation may perform module I/O.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_ethernet_new(out: *mut wire::EthernetFrameOp) -> bool {
    if let Some(op) = new_ethernet_op() {
        unsafe { out.write(op) };
        true
    } else {
        false
    }
}
/// # Safety
/// Exact writable stack/resident operation and readable, disjoint input. No
/// mutable operation reference spans a possible fault in caller input.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_ethernet_set_frame(
    op: *mut wire::EthernetFrameOp,
    data: *const u8,
    len: usize,
) -> bool {
    unsafe {
        if len > (*op).frame.len() {
            return false;
        }
        (*op).frame_len = len as u32;
        if len != 0 {
            ptr::copy_nonoverlapping(data, ptr::addr_of_mut!((*op).frame).cast(), len);
        }
        true
    }
}
/// # Safety
/// Exact canonical stack/resident operation retained through protocol execution.
pub(super) unsafe fn ethernet_dispatch(
    opcode: u32,
    op: *mut wire::EthernetFrameOp,
    flags: u32,
) -> bool {
    let result = dispatch(b"net.ethernet", opcode, op, flags);
    result >= 0 || result == wire::ETHERNET_RESULT_SHORT || result == wire::ETHERNET_RESULT_FILTERED
}
/// # Safety
/// Exact canonical stack/resident operation retained through protocol execution.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_ethernet_dispatch(
    opcode: u32,
    op: *mut wire::EthernetFrameOp,
    flags: u32,
) -> bool {
    unsafe { ethernet_dispatch(opcode, op, flags) }
}
pub(super) fn new_arp_op() -> Option<wire::ArpOp> {
    if unsafe { r4p::r4os_rust_r4p_has_active(role(b"net.arp")) } {
        Some(wire::ArpOp::default())
    } else {
        None
    }
}
/// # Safety
/// Exact writable canonical wire record; activation may perform module I/O.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_arp_new(out: *mut wire::ArpOp) -> bool {
    if let Some(op) = new_arp_op() {
        unsafe { out.write(op) };
        true
    } else {
        false
    }
}
/// # Safety
/// Exact writable stack/resident operation and readable, disjoint input. No
/// mutable operation reference spans a possible fault in caller input.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_arp_set_frame(
    op: *mut wire::ArpOp,
    data: *const u8,
    len: usize,
) -> bool {
    unsafe {
        if len > (*op).frame.len() {
            return false;
        }
        (*op).frame_len = len as u32;
        if len != 0 {
            ptr::copy_nonoverlapping(data, ptr::addr_of_mut!((*op).frame).cast(), len);
        }
        true
    }
}
/// # Safety
/// Exact canonical stack/resident operation retained through protocol execution.
pub(super) unsafe fn arp_dispatch(opcode: u32, op: *mut wire::ArpOp, flags: u32) -> bool {
    let result = dispatch(b"net.arp", opcode, op, flags);
    !((result == -5 || result == -4) && unsafe { (*op).result } == 0)
}
/// # Safety
/// Exact canonical stack/resident operation retained through protocol execution.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_arp_dispatch(
    opcode: u32,
    op: *mut wire::ArpOp,
    flags: u32,
) -> bool {
    unsafe { arp_dispatch(opcode, op, flags) }
}
pub(super) fn new_ipv4_op() -> Option<wire::Ipv4Op> {
    if unsafe { r4p::r4os_rust_r4p_has_active(role(b"net.ipv4")) } {
        Some(wire::Ipv4Op::default())
    } else {
        None
    }
}
/// # Safety
/// Exact writable canonical wire record; activation may perform module I/O.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_ipv4_new(out: *mut wire::Ipv4Op) -> bool {
    if let Some(op) = new_ipv4_op() {
        unsafe { out.write(op) };
        true
    } else {
        false
    }
}
/// # Safety
/// Exact writable stack/resident operation and readable, disjoint input. No
/// mutable operation reference spans a possible fault in caller input.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_ipv4_set_frame(
    op: *mut wire::Ipv4Op,
    data: *const u8,
    len: usize,
) -> bool {
    unsafe {
        if len > (*op).frame.len() {
            return false;
        }
        (*op).frame_len = len as u32;
        if len != 0 {
            ptr::copy_nonoverlapping(data, ptr::addr_of_mut!((*op).frame).cast(), len);
        }
        true
    }
}
/// # Safety
/// Exact canonical stack/resident operation retained through protocol execution.
pub(super) unsafe fn ipv4_dispatch(opcode: u32, op: *mut wire::Ipv4Op, flags: u32) -> bool {
    let result = dispatch(b"net.ipv4", opcode, op, flags);
    !((result == -5 || result == -4) && unsafe { (*op).result } == 0)
}
/// # Safety
/// Exact canonical stack/resident operation retained through protocol execution.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_ipv4_dispatch(
    opcode: u32,
    op: *mut wire::Ipv4Op,
    flags: u32,
) -> bool {
    unsafe { ipv4_dispatch(opcode, op, flags) }
}
pub(super) fn new_icmp_op() -> Option<wire::IcmpOp> {
    if unsafe { r4p::r4os_rust_r4p_has_active(role(b"net.icmp")) } {
        Some(wire::IcmpOp::default())
    } else {
        None
    }
}
/// # Safety
/// Exact writable canonical wire record; activation may perform module I/O.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_icmp_new(out: *mut wire::IcmpOp) -> bool {
    if let Some(op) = new_icmp_op() {
        unsafe { out.write(op) };
        true
    } else {
        false
    }
}
/// # Safety
/// Exact writable stack/resident operation and readable, disjoint input. No
/// mutable operation reference spans a possible fault in caller input.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_icmp_set_payload(
    op: *mut wire::IcmpOp,
    data: *const u8,
    len: usize,
) -> bool {
    unsafe {
        if len > (*op).payload.len() {
            return false;
        }
        (*op).payload_len = len as u32;
        if len != 0 {
            ptr::copy_nonoverlapping(data, ptr::addr_of_mut!((*op).payload).cast(), len);
        }
        true
    }
}
/// # Safety
/// Exact canonical stack/resident operation retained through protocol execution.
pub(super) unsafe fn icmp_dispatch(opcode: u32, op: *mut wire::IcmpOp, flags: u32) -> bool {
    let result = dispatch(b"net.icmp", opcode, op, flags);
    !((result == -5 || result == -4) && unsafe { (*op).result } == 0)
}
/// # Safety
/// Exact canonical stack/resident operation retained through protocol execution.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_icmp_dispatch(
    opcode: u32,
    op: *mut wire::IcmpOp,
    flags: u32,
) -> bool {
    unsafe { icmp_dispatch(opcode, op, flags) }
}
pub(super) fn new_udp_op() -> Option<wire::UdpOp> {
    if unsafe { r4p::r4os_rust_r4p_has_active(role(b"net.udp")) } {
        Some(wire::UdpOp::default())
    } else {
        None
    }
}
/// # Safety
/// Exact writable canonical wire record; activation may perform module I/O.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_udp_new(out: *mut wire::UdpOp) -> bool {
    if let Some(op) = new_udp_op() {
        unsafe { out.write(op) };
        true
    } else {
        false
    }
}
/// # Safety
/// Exact writable stack/resident operation and readable, disjoint input. No
/// mutable operation reference spans a possible fault in caller input.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_udp_set_datagram(
    op: *mut wire::UdpOp,
    data: *const u8,
    len: usize,
) -> bool {
    unsafe {
        if len > (*op).datagram.len() {
            return false;
        }
        (*op).datagram_len = len as u32;
        if len != 0 {
            ptr::copy_nonoverlapping(data, ptr::addr_of_mut!((*op).datagram).cast(), len);
        }
        true
    }
}
/// # Safety
/// Exact canonical stack/resident operation retained through protocol execution.
pub(super) unsafe fn udp_dispatch(opcode: u32, op: *mut wire::UdpOp, flags: u32) -> bool {
    let result = dispatch(b"net.udp", opcode, op, flags);
    !((result == -5 || result == -4) && unsafe { (*op).result } == 0)
}
/// # Safety
/// Exact canonical stack/resident operation retained through protocol execution.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_udp_dispatch(
    opcode: u32,
    op: *mut wire::UdpOp,
    flags: u32,
) -> bool {
    unsafe { udp_dispatch(opcode, op, flags) }
}
pub(super) fn new_dhcp_op() -> Option<wire::DhcpOp> {
    if unsafe { r4p::r4os_rust_r4p_has_active(role(b"net.dhcp")) } {
        Some(wire::DhcpOp::default())
    } else {
        None
    }
}
/// # Safety
/// Exact writable canonical wire record; activation may perform module I/O.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_dhcp_new(out: *mut wire::DhcpOp) -> bool {
    if let Some(op) = new_dhcp_op() {
        unsafe { out.write(op) };
        true
    } else {
        false
    }
}
/// # Safety
/// Exact writable stack/resident operation and readable, disjoint input. No
/// mutable operation reference spans a possible fault in caller input.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_dhcp_set_payload(
    op: *mut wire::DhcpOp,
    data: *const u8,
    len: usize,
) -> bool {
    unsafe {
        if len > (*op).payload.len() {
            return false;
        }
        (*op).payload_len = len as u32;
        if len != 0 {
            ptr::copy_nonoverlapping(data, ptr::addr_of_mut!((*op).payload).cast(), len);
        }
        true
    }
}
/// # Safety
/// Exact canonical stack/resident operation retained through protocol execution.
pub(super) unsafe fn dhcp_dispatch(opcode: u32, op: *mut wire::DhcpOp, flags: u32) -> bool {
    let result = dispatch(b"net.dhcp", opcode, op, flags);
    !((result == -5 || result == -4) && unsafe { (*op).result } == 0)
}
/// # Safety
/// Exact canonical stack/resident operation retained through protocol execution.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_dhcp_dispatch(
    opcode: u32,
    op: *mut wire::DhcpOp,
    flags: u32,
) -> bool {
    unsafe { dhcp_dispatch(opcode, op, flags) }
}
pub(super) fn new_dns_op() -> Option<wire::DnsOp> {
    if unsafe { r4p::r4os_rust_r4p_has_active(role(b"net.dns")) } {
        Some(wire::DnsOp::default())
    } else {
        None
    }
}
/// # Safety
/// Exact writable canonical wire record; activation may perform module I/O.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_dns_new(out: *mut wire::DnsOp) -> bool {
    if let Some(op) = new_dns_op() {
        unsafe { out.write(op) };
        true
    } else {
        false
    }
}
/// # Safety
/// Exact writable stack/resident operation and readable, disjoint input. No
/// mutable operation reference spans a possible fault in caller input.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_dns_set_name(
    op: *mut wire::DnsOp,
    data: *const u8,
    len: usize,
) -> bool {
    unsafe {
        if len > (*op).name.len() || len == 0 {
            return false;
        }
        (*op).name_len = len as u16;
        if len != 0 {
            ptr::copy_nonoverlapping(data, ptr::addr_of_mut!((*op).name).cast(), len);
        }
        true
    }
}
/// # Safety
/// Exact writable stack/resident operation and readable, disjoint input. No
/// mutable operation reference spans a possible fault in caller input.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_dns_set_payload(
    op: *mut wire::DnsOp,
    data: *const u8,
    len: usize,
) -> bool {
    unsafe {
        if len > (*op).payload.len() {
            return false;
        }
        (*op).payload_len = len as u32;
        if len != 0 {
            ptr::copy_nonoverlapping(data, ptr::addr_of_mut!((*op).payload).cast(), len);
        }
        true
    }
}
/// # Safety
/// Exact canonical stack/resident operation retained through protocol execution.
pub(super) unsafe fn dns_dispatch(opcode: u32, op: *mut wire::DnsOp, flags: u32) -> bool {
    let result = dispatch(b"net.dns", opcode, op, flags);
    !((result == -5 || result == -4) && unsafe { (*op).result } == 0)
}
/// # Safety
/// Exact canonical stack/resident operation retained through protocol execution.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_dns_dispatch(
    opcode: u32,
    op: *mut wire::DnsOp,
    flags: u32,
) -> bool {
    unsafe { dns_dispatch(opcode, op, flags) }
}
pub(super) fn new_tcp_op() -> Option<wire::TcpOp> {
    if unsafe { r4p::r4os_rust_r4p_has_active(role(b"net.tcp")) } {
        Some(wire::TcpOp::default())
    } else {
        None
    }
}
/// # Safety
/// Exact writable canonical wire record; activation may perform module I/O.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_tcp_new(out: *mut wire::TcpOp) -> bool {
    if let Some(op) = new_tcp_op() {
        unsafe { out.write(op) };
        true
    } else {
        false
    }
}
/// # Safety
/// Exact writable stack/resident operation and readable, disjoint input. No
/// mutable operation reference spans a possible fault in caller input.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_tcp_set_payload(
    op: *mut wire::TcpOp,
    data: *const u8,
    len: usize,
) -> bool {
    unsafe {
        if len > (*op).payload.len() {
            return false;
        }
        (*op).payload_len = len as u32;
        if len != 0 {
            ptr::copy_nonoverlapping(data, ptr::addr_of_mut!((*op).payload).cast(), len);
        }
        true
    }
}
/// # Safety
/// Exact writable stack/resident operation and readable, disjoint input. No
/// mutable operation reference spans a possible fault in caller input.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_tcp_set_segment(
    op: *mut wire::TcpOp,
    data: *const u8,
    len: usize,
) -> bool {
    unsafe {
        if len > (*op).segment.len() {
            return false;
        }
        (*op).segment_len = len as u32;
        if len != 0 {
            ptr::copy_nonoverlapping(data, ptr::addr_of_mut!((*op).segment).cast(), len);
        }
        true
    }
}
/// # Safety
/// Exact canonical stack/resident operation retained through protocol execution.
pub(super) unsafe fn tcp_dispatch(opcode: u32, op: *mut wire::TcpOp, flags: u32) -> bool {
    let result = dispatch(b"net.tcp", opcode, op, flags);
    !((result == -5 || result == -4) && unsafe { (*op).result } == 0)
}
/// # Safety
/// Exact canonical stack/resident operation retained through protocol execution.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_tcp_dispatch(
    opcode: u32,
    op: *mut wire::TcpOp,
    flags: u32,
) -> bool {
    unsafe { tcp_dispatch(opcode, op, flags) }
}
