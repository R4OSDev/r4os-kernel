pub const version: u32 = 2;
pub const negotiation_version: u32 = 1;
pub const packet_version: u32 = 1;
pub const max_packet_segments: usize = 8;

pub const capability_rx_l4_checksum_valid: u64 = 1 << 0;
pub const capability_tx_l4_checksum_partial: u64 = 1 << 1;
pub const capability_rx_scatter: u64 = 1 << 2;
pub const capability_tx_scatter: u64 = 1 << 3;
pub const capability_rx_vlan_strip: u64 = 1 << 4;
pub const capability_tx_vlan_insert: u64 = 1 << 5;
pub const capability_tx_segmentation: u64 = 1 << 6;
pub const capability_interrupt_moderation: u64 = 1 << 7;
pub const capability_tx_notification_suppression: u64 = 1 << 8;
pub const capability_multiqueue: u64 = 1 << 9;
pub const capability_async_tx_completion: u64 = 1 << 10;
pub const known_capabilities: u64 = (1 << 11) - 1;

// 0.69.50 deliberately admits one first consumer. All other defined bits
// remain visible as rejected so a driver can retain its byte-identical flat
// software path. SMP may raise the selected queue count in a later version.
pub const supported_capabilities: u64 = capability_rx_l4_checksum_valid;

pub const ownership_borrowed_until_return: u32 = 0;
pub const ownership_transferred_until_completion: u32 = 1;

pub const packet_flag_rx_l4_checksum_valid: u64 = 1 << 0;
pub const packet_flag_tx_l4_checksum_partial: u64 = 1 << 1;
pub const packet_flag_vlan_tag_valid: u64 = 1 << 2;
pub const packet_flag_segmentation: u64 = 1 << 3;
pub const packet_flag_scatter: u64 = 1 << 4;
pub const known_packet_flags: u64 = (1 << 5) - 1;
pub const protocol_buffer_flag_rx_l4_checksum_valid: u32 = 1 << 0;

pub const BufferSegment = extern struct {
    address: u64 = 0,
    bytes: u32 = 0,
    reserved: u32 = 0,
};

/// Common RX/TX packet description. `fallback_addr/fallback_bytes` is the
/// mandatory canonical byte stream. Optional segment and offload fields may
/// only replace work after their capability appears in `accepted`; otherwise
/// the receiver ignores them and consumes the flat fallback unchanged.
pub const Packet = extern struct {
    version: u32 = packet_version,
    size: u32 = @sizeOf(Packet),
    flags: u64 = 0,
    fallback_addr: u64 = 0,
    segments_addr: u64 = 0,
    completion_handle: u64 = 0,
    fallback_bytes: u32 = 0,
    ownership: u32 = ownership_borrowed_until_return,
    queue_index: u16 = 0,
    segment_count: u16 = 0,
    checksum_start: u16 = 0,
    checksum_offset: u16 = 0,
    vlan_tag: u16 = 0,
    gso_size: u16 = 0,
    reserved: u32 = 0,
};

pub const TxComplete = *const fn (u64, i32, u32) callconv(.c) void;

pub const TxRequest = extern struct {
    version: u32 = packet_version,
    size: u32 = @sizeOf(TxRequest),
    packet: Packet = .{},
    complete: ?TxComplete = null,
};

pub const TransmitPacketFn = *const fn (?*anyopaque, *const TxRequest) callconv(.c) i32;

pub const Negotiation = extern struct {
    version: u32 = negotiation_version,
    size: u32 = @sizeOf(Negotiation),
    offered: u64 = 0,
    accepted: u64 = 0,
    rejected: u64 = 0,
    rx_queue_count: u16 = 1,
    tx_queue_count: u16 = 1,
    max_rx_segments: u16 = 1,
    max_tx_segments: u16 = 1,
    rx_ownership: u32 = ownership_borrowed_until_return,
    tx_ownership: u32 = ownership_borrowed_until_return,
    interrupt_moderation_us: u32 = 0,
    reserved: u32 = 0,
};

pub const Offer = struct {
    offered: u64 = 0,
    required: u64 = 0,
    rx_queue_count: u16 = 1,
    tx_queue_count: u16 = 1,
    max_rx_segments: u16 = 1,
    max_tx_segments: u16 = 1,
    rx_ownership: u32 = ownership_borrowed_until_return,
    tx_ownership: u32 = ownership_borrowed_until_return,
    interrupt_moderation_us: u32 = 0,
};

pub const NegotiateError = error{
    InvalidOffer,
    RequiredNotOffered,
    RequiredUnsupported,
};

pub fn negotiate(offer: Offer) NegotiateError!Negotiation {
    if (offer.rx_queue_count == 0 or offer.tx_queue_count == 0 or
        offer.max_rx_segments == 0 or offer.max_tx_segments == 0 or
        offer.max_rx_segments > max_packet_segments or offer.max_tx_segments > max_packet_segments or
        offer.rx_ownership > ownership_transferred_until_completion or
        offer.tx_ownership > ownership_transferred_until_completion)
    {
        return error.InvalidOffer;
    }
    if ((offer.required & ~offer.offered) != 0) return error.RequiredNotOffered;

    const accepted = offer.offered & supported_capabilities;
    if ((offer.required & ~accepted) != 0) return error.RequiredUnsupported;

    return .{
        .offered = offer.offered,
        .accepted = accepted,
        .rejected = offer.offered & ~accepted,
        // The current BSP-only Netcore intentionally selects one queue and a
        // flat canonical buffer even when the device offers larger values.
        .rx_queue_count = 1,
        .tx_queue_count = 1,
        .max_rx_segments = 1,
        .max_tx_segments = 1,
        .rx_ownership = ownership_borrowed_until_return,
        .tx_ownership = ownership_borrowed_until_return,
        .interrupt_moderation_us = 0,
    };
}

pub fn validNegotiationQuery(query: *const Negotiation) bool {
    return query.version == negotiation_version and query.size >= @sizeOf(Negotiation);
}

pub fn validRxPacket(packet: *const Packet, selected: Negotiation, max_frame_bytes: usize) bool {
    if (packet.version != packet_version or packet.size < @sizeOf(Packet)) return false;
    if (packet.ownership != ownership_borrowed_until_return) return false;
    if (packet.fallback_addr == 0 or packet.fallback_bytes == 0 or packet.fallback_bytes > max_frame_bytes) return false;
    if (packet.queue_index >= selected.rx_queue_count) return false;
    if (packet.segment_count > max_packet_segments) return false;
    if (packet.segment_count != 0 and packet.segments_addr == 0) return false;
    return true;
}

pub const RxMetadataDecision = enum(u8) {
    software,
    software_fallback,
    l4_checksum_valid,
};

/// Converts untrusted/optional packet metadata into the one currently
/// admitted protocol flag. Unknown, unaccepted, fragmented, VLAN-wrapped or
/// malformed claims never reject or mutate the canonical frame; they select
/// the ordinary software verifier instead.
pub fn selectRxMetadata(accepted_capabilities: u64, packet_flags: u64, frame: []const u8) RxMetadataDecision {
    if (packet_flags == 0) return .software;
    if ((packet_flags & ~known_packet_flags) != 0) return .software_fallback;
    if (packet_flags != packet_flag_rx_l4_checksum_valid) return .software_fallback;
    if ((accepted_capabilities & capability_rx_l4_checksum_valid) == 0) return .software_fallback;
    if (!flatIpv4L4Frame(frame)) return .software_fallback;
    return .l4_checksum_valid;
}

fn flatIpv4L4Frame(frame: []const u8) bool {
    const ethernet_bytes: usize = 14;
    const ipv4_min_bytes: usize = 20;
    if (frame.len < ethernet_bytes + ipv4_min_bytes) return false;
    if (readBe16(frame, 12) != 0x0800) return false;

    const ip = frame[ethernet_bytes..];
    if ((ip[0] >> 4) != 4) return false;
    const header_bytes = @as(usize, ip[0] & 0x0f) * 4;
    if (header_bytes < ipv4_min_bytes or ip.len < header_bytes) return false;
    const total_bytes: usize = readBe16(ip, 2);
    if (total_bytes < header_bytes or total_bytes > ip.len) return false;
    if ((readBe16(ip, 6) & 0x3fff) != 0) return false;

    const l4_bytes = total_bytes - header_bytes;
    return switch (ip[9]) {
        17 => l4_bytes >= 8,
        6 => l4_bytes >= 20 and (ip[header_bytes + 12] >> 4) >= 5 and
            @as(usize, ip[header_bytes + 12] >> 4) * 4 <= l4_bytes,
        else => false,
    };
}

fn readBe16(bytes: []const u8, offset: usize) u16 {
    return (@as(u16, bytes[offset]) << 8) | bytes[offset + 1];
}

test "negotiation admits only checksum and keeps BSP single-queue fallback" {
    const std = @import("std");
    const offered = capability_rx_l4_checksum_valid |
        capability_tx_segmentation |
        capability_multiqueue |
        (@as(u64, 1) << 63);
    const selected = try negotiate(.{
        .offered = offered,
        .rx_queue_count = 4,
        .tx_queue_count = 4,
        .max_rx_segments = 8,
        .max_tx_segments = 8,
        .interrupt_moderation_us = 50,
    });
    try std.testing.expectEqual(capability_rx_l4_checksum_valid, selected.accepted);
    try std.testing.expectEqual(offered & ~capability_rx_l4_checksum_valid, selected.rejected);
    try std.testing.expectEqual(@as(u16, 1), selected.rx_queue_count);
    try std.testing.expectEqual(@as(u16, 1), selected.max_rx_segments);
    try std.testing.expectEqual(@as(u32, 0), selected.interrupt_moderation_us);
}

test "required rejected capability fails instead of silently changing semantics" {
    const std = @import("std");
    try std.testing.expectError(error.RequiredNotOffered, negotiate(.{
        .offered = capability_rx_l4_checksum_valid,
        .required = capability_tx_segmentation,
    }));
    try std.testing.expectError(error.RequiredUnsupported, negotiate(.{
        .offered = capability_tx_segmentation,
        .required = capability_tx_segmentation,
    }));
}

test "unknown and incapable RX metadata select byte-preserving software fallback" {
    const std = @import("std");
    var frame: [42]u8 = .{0} ** 42;
    frame[12] = 0x08;
    frame[13] = 0x00;
    frame[14] = 0x45;
    frame[16] = 0;
    frame[17] = 28;
    frame[23] = 17;
    frame[38] = 0;
    frame[39] = 8;
    const original = frame;

    try std.testing.expectEqual(RxMetadataDecision.l4_checksum_valid, selectRxMetadata(
        capability_rx_l4_checksum_valid,
        packet_flag_rx_l4_checksum_valid,
        &frame,
    ));
    try std.testing.expectEqual(RxMetadataDecision.software_fallback, selectRxMetadata(
        0,
        packet_flag_rx_l4_checksum_valid,
        &frame,
    ));
    try std.testing.expectEqual(RxMetadataDecision.software_fallback, selectRxMetadata(
        capability_rx_l4_checksum_valid,
        packet_flag_rx_l4_checksum_valid | (@as(u64, 1) << 63),
        &frame,
    ));
    try std.testing.expectEqualSlices(u8, &original, &frame);
}
