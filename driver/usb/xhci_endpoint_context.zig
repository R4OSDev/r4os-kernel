// Linux and xHCI treat Max ESIT Payload as zero for asynchronous bulk
// endpoints. Their average TRB length is not derivable from the endpoint
// descriptors either, so keep the advisory value zero instead of inventing a
// periodic-looking payload size.
pub const BULK_AVERAGE_TRB_LENGTH: u16 = 0;

pub const BulkFields = struct {
    dword0: u32,
    dword1: u32,
    dword4: u32,
    max_packet: u16,
    max_burst: u8,
};

// Builds the software-owned words of an xHCI Bulk Endpoint Context.
// Max ESIT Payload is reserved for asynchronous endpoints and therefore stays
// zero; only the low 16 bits of dword4 contain Average TRB Length.
pub fn bulkFields(speed: u8, in_direction: bool, descriptor_max_packet: u16, descriptor_max_burst: u8) BulkFields {
    const superspeed = speed >= 4;
    const fallback_packet: u16 = if (superspeed) 1024 else 512;
    const masked_packet = descriptor_max_packet & 0x07ff;
    const max_packet = if (masked_packet == 0) fallback_packet else masked_packet;
    const max_burst = if (superspeed) @min(descriptor_max_burst, 15) else 0;
    const endpoint_type: u32 = if (in_direction) 6 else 2;

    return .{
        .dword0 = 0,
        .dword1 = (3 << 1) |
            (endpoint_type << 3) |
            (@as(u32, max_burst) << 8) |
            (@as(u32, max_packet) << 16),
        .dword4 = BULK_AVERAGE_TRB_LENGTH,
        .max_packet = max_packet,
        .max_burst = max_burst,
    };
}

test "Samsung FIT SuperSpeed BOT fields preserve bMaxBurst" {
    const testing = @import("std").testing;

    const out = bulkFields(4, false, 1024, 8);
    try testing.expectEqual(@as(u32, 0x0400_0816), out.dword1);
    try testing.expectEqual(@as(u32, 0), out.dword4);
    try testing.expectEqual(@as(u16, 1024), out.max_packet);
    try testing.expectEqual(@as(u8, 8), out.max_burst);

    const in = bulkFields(4, true, 1024, 8);
    try testing.expectEqual(@as(u32, 0x0400_0836), in.dword1);
    try testing.expectEqual(@as(u32, 0), in.dword4 >> 16);
}

test "non-SuperSpeed bulk clears companion-only burst" {
    const testing = @import("std").testing;

    const fields = bulkFields(3, true, 512, 8);
    try testing.expectEqual(@as(u8, 0), fields.max_burst);
    try testing.expectEqual(@as(u32, 0x0200_0036), fields.dword1);
}

test "bulk descriptor fields are bounded" {
    const testing = @import("std").testing;

    const fields = bulkFields(4, false, 0xffff, 0xff);
    try testing.expectEqual(@as(u16, 0x07ff), fields.max_packet);
    try testing.expectEqual(@as(u8, 15), fields.max_burst);
    try testing.expectEqual(@as(u32, BULK_AVERAGE_TRB_LENGTH), fields.dword4);
}
