comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
// Native USB owner: stateless compatibility facade.
extern fn r4os_rust_usb_bulk_fields(u8, bool, u16, u8, *BulkFields) void;
// Linux and xHCI treat Max ESIT Payload as zero for asynchronous bulk
// endpoints. Their average TRB length is not derivable from the endpoint
// descriptors either, so keep the advisory value zero instead of inventing a
// periodic-looking payload size.
pub const BULK_AVERAGE_TRB_LENGTH: u16 = 0;

pub const BulkFields = extern struct {
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
    var out: BulkFields = undefined;
    r4os_rust_usb_bulk_fields(speed, in_direction, descriptor_max_packet, descriptor_max_burst, &out);
    return out;
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

comptime {
    if (@sizeOf(BulkFields) != 16 or
        @alignOf(BulkFields) != 4 or
        @offsetOf(BulkFields, "dword0") != 0 or
        @offsetOf(BulkFields, "dword1") != 4 or
        @offsetOf(BulkFields, "dword4") != 8 or
        @offsetOf(BulkFields, "max_packet") != 12 or
        @offsetOf(BulkFields, "max_burst") != 14) @compileError("USB BulkFields private C layout");
}
