// Native network helper; caller retains its original owner/storage.
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
extern fn r4os_rust_net_negotiate(*const Offer, *Negotiation) u8;
extern fn r4os_rust_net_valid_query(*const Negotiation) bool;
extern fn r4os_rust_net_valid_rx(*const Packet, *const Negotiation, usize) bool;
extern fn r4os_rust_net_rx_metadata(u64, u64, [*]const u8, usize) u8;
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

pub const Offer = extern struct {
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
    var out: Negotiation = undefined;
    return switch (r4os_rust_net_negotiate(&offer, &out)) {
        0 => out,
        1 => error.InvalidOffer,
        2 => error.RequiredNotOffered,
        3 => error.RequiredUnsupported,
        else => unreachable,
    };
}

pub fn validNegotiationQuery(query: *const Negotiation) bool {
    return r4os_rust_net_valid_query(query);
}

pub fn validRxPacket(packet: *const Packet, selected: Negotiation, max_frame_bytes: usize) bool {
    return r4os_rust_net_valid_rx(packet, &selected, max_frame_bytes);
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
    return @enumFromInt(r4os_rust_net_rx_metadata(accepted_capabilities, packet_flags, frame.ptr, frame.len));
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

comptime {
    if (@sizeOf(Packet) != 64 or @alignOf(Packet) != 8 or @offsetOf(Packet, "version") != 0 or @offsetOf(Packet, "size") != 4 or @offsetOf(Packet, "flags") != 8 or @offsetOf(Packet, "fallback_addr") != 16 or @offsetOf(Packet, "segments_addr") != 24 or @offsetOf(Packet, "completion_handle") != 32 or @offsetOf(Packet, "fallback_bytes") != 40 or @offsetOf(Packet, "ownership") != 44 or @offsetOf(Packet, "queue_index") != 48 or @offsetOf(Packet, "segment_count") != 50 or @offsetOf(Packet, "checksum_start") != 52 or @offsetOf(Packet, "checksum_offset") != 54 or @offsetOf(Packet, "vlan_tag") != 56 or @offsetOf(Packet, "gso_size") != 58 or @offsetOf(Packet, "reserved") != 60) @compileError("Native network Packet layout");
}

comptime {
    if (@sizeOf(Negotiation) != 56 or @alignOf(Negotiation) != 8 or @offsetOf(Negotiation, "version") != 0 or @offsetOf(Negotiation, "size") != 4 or @offsetOf(Negotiation, "offered") != 8 or @offsetOf(Negotiation, "accepted") != 16 or @offsetOf(Negotiation, "rejected") != 24 or @offsetOf(Negotiation, "rx_queue_count") != 32 or @offsetOf(Negotiation, "tx_queue_count") != 34 or @offsetOf(Negotiation, "max_rx_segments") != 36 or @offsetOf(Negotiation, "max_tx_segments") != 38 or @offsetOf(Negotiation, "rx_ownership") != 40 or @offsetOf(Negotiation, "tx_ownership") != 44 or @offsetOf(Negotiation, "interrupt_moderation_us") != 48 or @offsetOf(Negotiation, "reserved") != 52) @compileError("Native network Negotiation layout");
}

comptime {
    if (@sizeOf(Offer) != 40 or @alignOf(Offer) != 8 or @offsetOf(Offer, "offered") != 0 or @offsetOf(Offer, "required") != 8 or @offsetOf(Offer, "rx_queue_count") != 16 or @offsetOf(Offer, "tx_queue_count") != 18 or @offsetOf(Offer, "max_rx_segments") != 20 or @offsetOf(Offer, "max_tx_segments") != 22 or @offsetOf(Offer, "rx_ownership") != 24 or @offsetOf(Offer, "tx_ownership") != 28 or @offsetOf(Offer, "interrupt_moderation_us") != 32) @compileError("Native network Offer layout");
}
