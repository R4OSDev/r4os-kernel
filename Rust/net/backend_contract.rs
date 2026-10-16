//! Existing first-consumer NIC negotiation and byte-preserving RX fallback.
//! Canonical packet/query ABI comes from the unchanged platform contract.
pub use crate::contract::{NetBackendNegotiation as Negotiation, NetPacket as Packet};
pub const MAX_PACKET_SEGMENTS: usize = 8;
pub const SUPPORTED_CAPABILITIES: u64 = 1;
pub const KNOWN_PACKET_FLAGS: u64 = 31;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Offer {
    pub offered: u64,
    pub required: u64,
    pub rx_queue_count: u16,
    pub tx_queue_count: u16,
    pub max_rx_segments: u16,
    pub max_tx_segments: u16,
    pub rx_ownership: u32,
    pub tx_ownership: u32,
    pub interrupt_moderation_us: u32,
}
pub fn negotiate(offer: Offer) -> Result<Negotiation, u8> {
    if offer.rx_queue_count == 0
        || offer.tx_queue_count == 0
        || offer.max_rx_segments == 0
        || offer.max_tx_segments == 0
        || offer.max_rx_segments as usize > MAX_PACKET_SEGMENTS
        || offer.max_tx_segments as usize > MAX_PACKET_SEGMENTS
        || offer.rx_ownership > 1
        || offer.tx_ownership > 1
    {
        return Err(1);
    }
    if offer.required & !offer.offered != 0 {
        return Err(2);
    }
    let accepted = offer.offered & SUPPORTED_CAPABILITIES;
    if offer.required & !accepted != 0 {
        return Err(3);
    }
    Ok(Negotiation {
        version: 1,
        size: core::mem::size_of::<Negotiation>() as u32,
        offered: offer.offered,
        accepted,
        rejected: offer.offered & !accepted,
        rx_queue_count: 1,
        tx_queue_count: 1,
        max_rx_segments: 1,
        max_tx_segments: 1,
        rx_ownership: 0,
        tx_ownership: 0,
        interrupt_moderation_us: 0,
        reserved: 0,
    })
}
pub fn valid_negotiation_query(query: Negotiation) -> bool {
    query.version == 1 && query.size as usize >= core::mem::size_of::<Negotiation>()
}
pub fn valid_rx_packet(packet: Packet, selected: Negotiation, max_frame: usize) -> bool {
    packet.version == 1
        && packet.size as usize >= core::mem::size_of::<Packet>()
        && packet.ownership == 0
        && packet.fallback_addr != 0
        && packet.fallback_bytes != 0
        && packet.fallback_bytes as usize <= max_frame
        && packet.queue_index < selected.rx_queue_count
        && packet.segment_count as usize <= MAX_PACKET_SEGMENTS
        && (packet.segment_count == 0 || packet.segments_addr != 0)
}
#[repr(u8)]
#[derive(Clone, Copy)]
pub enum RxMetadataDecision {
    Software,
    SoftwareFallback,
    L4ChecksumValid,
}
pub fn select_rx_metadata(accepted: u64, flags: u64, frame: &[u8]) -> RxMetadataDecision {
    if flags == 0 {
        return RxMetadataDecision::Software;
    }
    if flags & !KNOWN_PACKET_FLAGS != 0
        || flags != 1
        || accepted & 1 == 0
        || !flat_ipv4_l4_frame(frame)
    {
        return RxMetadataDecision::SoftwareFallback;
    }
    RxMetadataDecision::L4ChecksumValid
}
fn be16(bytes: &[u8], at: usize) -> u16 {
    (u16::from(bytes[at]) << 8) | u16::from(bytes[at + 1])
}
fn flat_ipv4_l4_frame(frame: &[u8]) -> bool {
    if frame.len() < 34 || be16(frame, 12) != 0x0800 {
        return false;
    }
    let ip = &frame[14..];
    if ip[0] >> 4 != 4 {
        return false;
    }
    let header = usize::from(ip[0] & 15) * 4;
    if header < 20 || ip.len() < header {
        return false;
    }
    let total = usize::from(be16(ip, 2));
    if total < header || total > ip.len() || be16(ip, 6) & 0x3fff != 0 {
        return false;
    }
    let l4 = total - header;
    match ip[9] {
        17 => l4 >= 8,
        6 => l4 >= 20 && ip[header + 12] >> 4 >= 5 && usize::from(ip[header + 12] >> 4) * 4 <= l4,
        _ => false,
    }
}
/// # Safety
/// Exact borrowed offer and exclusive paired negotiation output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_negotiate(offer: *const Offer, out: *mut Negotiation) -> u8 {
    match negotiate(unsafe { *offer }) {
        Ok(value) => {
            unsafe {
                out.write(value);
            }
            0
        }
        Err(code) => code,
    }
}
/// # Safety
/// Borrowed canonical query remains resident for this pure validation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_valid_query(query: *const Negotiation) -> bool {
    valid_negotiation_query(unsafe { *query })
}
/// # Safety
/// Exact borrowed packet/query records; no buffer address is dereferenced.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_valid_rx(
    packet: *const Packet,
    selected: *const Negotiation,
    max_frame: usize,
) -> bool {
    valid_rx_packet(unsafe { *packet }, unsafe { *selected }, max_frame)
}
/// # Safety
/// Readable frame is retained throughout this pure metadata admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_rx_metadata(
    accepted: u64,
    flags: u64,
    bytes: *const u8,
    len: usize,
) -> u8 {
    let frame = if len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(bytes, len) }
    };
    select_rx_metadata(accepted, flags, frame) as u8
}

const _: () = {
    assert!(core::mem::size_of::<Packet>() == 64);
    assert!(core::mem::align_of::<Packet>() == 8);
    assert!(core::mem::offset_of!(Packet, version) == 0);
    assert!(core::mem::offset_of!(Packet, size) == 4);
    assert!(core::mem::offset_of!(Packet, flags) == 8);
    assert!(core::mem::offset_of!(Packet, fallback_addr) == 16);
    assert!(core::mem::offset_of!(Packet, segments_addr) == 24);
    assert!(core::mem::offset_of!(Packet, completion_handle) == 32);
    assert!(core::mem::offset_of!(Packet, fallback_bytes) == 40);
    assert!(core::mem::offset_of!(Packet, ownership) == 44);
    assert!(core::mem::offset_of!(Packet, queue_index) == 48);
    assert!(core::mem::offset_of!(Packet, segment_count) == 50);
    assert!(core::mem::offset_of!(Packet, checksum_start) == 52);
    assert!(core::mem::offset_of!(Packet, checksum_offset) == 54);
    assert!(core::mem::offset_of!(Packet, vlan_tag) == 56);
    assert!(core::mem::offset_of!(Packet, gso_size) == 58);
    assert!(core::mem::offset_of!(Packet, reserved) == 60);
};
const _: () = {
    assert!(core::mem::size_of::<Negotiation>() == 56);
    assert!(core::mem::align_of::<Negotiation>() == 8);
    assert!(core::mem::offset_of!(Negotiation, version) == 0);
    assert!(core::mem::offset_of!(Negotiation, size) == 4);
    assert!(core::mem::offset_of!(Negotiation, offered) == 8);
    assert!(core::mem::offset_of!(Negotiation, accepted) == 16);
    assert!(core::mem::offset_of!(Negotiation, rejected) == 24);
    assert!(core::mem::offset_of!(Negotiation, rx_queue_count) == 32);
    assert!(core::mem::offset_of!(Negotiation, tx_queue_count) == 34);
    assert!(core::mem::offset_of!(Negotiation, max_rx_segments) == 36);
    assert!(core::mem::offset_of!(Negotiation, max_tx_segments) == 38);
    assert!(core::mem::offset_of!(Negotiation, rx_ownership) == 40);
    assert!(core::mem::offset_of!(Negotiation, tx_ownership) == 44);
    assert!(core::mem::offset_of!(Negotiation, interrupt_moderation_us) == 48);
    assert!(core::mem::offset_of!(Negotiation, reserved) == 52);
};
const _: () = {
    assert!(core::mem::size_of::<Offer>() == 40);
    assert!(core::mem::align_of::<Offer>() == 8);
    assert!(core::mem::offset_of!(Offer, offered) == 0);
    assert!(core::mem::offset_of!(Offer, required) == 8);
    assert!(core::mem::offset_of!(Offer, rx_queue_count) == 16);
    assert!(core::mem::offset_of!(Offer, tx_queue_count) == 18);
    assert!(core::mem::offset_of!(Offer, max_rx_segments) == 20);
    assert!(core::mem::offset_of!(Offer, max_tx_segments) == 22);
    assert!(core::mem::offset_of!(Offer, rx_ownership) == 24);
    assert!(core::mem::offset_of!(Offer, tx_ownership) == 28);
    assert!(core::mem::offset_of!(Offer, interrupt_moderation_us) == 32);
};
