//! Private bounded owned payload messages. A returned view borrows its exact
//! caller-owned message after transfer, rather than shared global scratch or
//! the stack frame of a decoder that has already returned.
use super::r4p_contract as wire;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct OwnedIpv4Packet {
    pub protocol: u8,
    pub source_ip: [u8; 4],
    pub dest_ip: [u8; 4],
    pub payload_len: usize,
    pub data: [u8; 1536],
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct OwnedUdpPacket {
    pub source_ip: [u8; 4],
    pub dest_ip: [u8; 4],
    pub source_port: u16,
    pub dest_port: u16,
    pub length: u16,
    pub payload_len: usize,
    pub data: [u8; 1536],
}
/// # Safety
/// Exact initialized readable wire record and separate writable paired message.
/// Both remain retained through this bounded copy; no pointer escapes into op.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_ipv4_view(
    op: *const wire::Ipv4Op,
    out: *mut OwnedIpv4Packet,
) {
    let op = unsafe { &*op };
    let len = (op.payload_len as usize).min(1536);
    let mut value = OwnedIpv4Packet {
        protocol: op.protocol,
        source_ip: op.source_ip,
        dest_ip: op.dest_ip,
        payload_len: len,
        data: [0; 1536],
    };
    value.data[..len].copy_from_slice(&op.payload[..len]);
    unsafe {
        out.write(value);
    }
}
/// # Safety
/// Exact initialized readable wire record and separate writable paired message.
/// Both remain retained through this bounded copy; no pointer escapes into op.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wire_udp_view(op: *const wire::UdpOp, out: *mut OwnedUdpPacket) {
    let op = unsafe { &*op };
    let len = (op.payload_len as usize).min(1536);
    let mut value = OwnedUdpPacket {
        source_ip: op.source_ip,
        dest_ip: op.dest_ip,
        source_port: op.source_port,
        dest_port: op.dest_port,
        length: op.length,
        payload_len: len,
        data: [0; 1536],
    };
    value.data[..len].copy_from_slice(&op.payload[..len]);
    unsafe {
        out.write(value);
    }
}

const _: () = {
    assert!(core::mem::size_of::<OwnedIpv4Packet>() == 1560);
    assert!(core::mem::align_of::<OwnedIpv4Packet>() == 8);
    assert!(core::mem::offset_of!(OwnedIpv4Packet, protocol) == 0);
    assert!(core::mem::offset_of!(OwnedIpv4Packet, source_ip) == 1);
    assert!(core::mem::offset_of!(OwnedIpv4Packet, dest_ip) == 5);
    assert!(core::mem::offset_of!(OwnedIpv4Packet, payload_len) == 16);
    assert!(core::mem::offset_of!(OwnedIpv4Packet, data) == 24);
};
const _: () = {
    assert!(core::mem::size_of::<OwnedUdpPacket>() == 1560);
    assert!(core::mem::align_of::<OwnedUdpPacket>() == 8);
    assert!(core::mem::offset_of!(OwnedUdpPacket, source_ip) == 0);
    assert!(core::mem::offset_of!(OwnedUdpPacket, dest_ip) == 4);
    assert!(core::mem::offset_of!(OwnedUdpPacket, source_port) == 8);
    assert!(core::mem::offset_of!(OwnedUdpPacket, dest_port) == 10);
    assert!(core::mem::offset_of!(OwnedUdpPacket, length) == 12);
    assert!(core::mem::offset_of!(OwnedUdpPacket, payload_len) == 16);
    assert!(core::mem::offset_of!(OwnedUdpPacket, data) == 24);
};
