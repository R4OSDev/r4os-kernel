pub const COMPLETION_SUCCESS: u8 = 1;
pub const COMPLETION_SHORT_PACKET: u8 = 13;

pub const Evaluation = struct {
    accepted: bool,
    actual_len: u32,
};

// A successful xHCI Transfer Event reports zero residue. Short Packet is a
// successful terminal condition for IN endpoints; the Event TRB length is the
// untransferred residue. OUT transfers must complete without a short packet.
pub fn evaluate(in_direction: bool, requested: u32, completion_code: u8, residue: u32) Evaluation {
    if (residue > requested) return .{ .accepted = false, .actual_len = 0 };
    const accepted = (completion_code == COMPLETION_SUCCESS and residue == 0) or
        (in_direction and completion_code == COMPLETION_SHORT_PACKET);
    return .{
        .accepted = accepted,
        .actual_len = if (accepted) requested - residue else 0,
    };
}

test "bulk IN accepts exact and short completions" {
    const testing = @import("std").testing;

    const exact = evaluate(true, 36, COMPLETION_SUCCESS, 0);
    try testing.expect(exact.accepted);
    try testing.expectEqual(@as(u32, 36), exact.actual_len);

    const short = evaluate(true, 36, COMPLETION_SHORT_PACKET, 5);
    try testing.expect(short.accepted);
    try testing.expectEqual(@as(u32, 31), short.actual_len);
}

test "bulk OUT and impossible residue fail closed" {
    const testing = @import("std").testing;

    try testing.expect(!evaluate(false, 31, COMPLETION_SHORT_PACKET, 1).accepted);
    try testing.expect(!evaluate(false, 31, COMPLETION_SHORT_PACKET, 0).accepted);
    try testing.expect(!evaluate(false, 31, COMPLETION_SUCCESS, 1).accepted);
    try testing.expect(!evaluate(true, 31, COMPLETION_SUCCESS, 1).accepted);
    try testing.expect(!evaluate(true, 13, COMPLETION_SUCCESS, 14).accepted);
    try testing.expect(!evaluate(true, 13, 4, 0).accepted);
}

test "only IN completion code 13 represents a short transfer" {
    const testing = @import("std").testing;

    const zero_residue_short = evaluate(true, 64, COMPLETION_SHORT_PACKET, 0);
    try testing.expect(zero_residue_short.accepted);
    try testing.expectEqual(@as(u32, 64), zero_residue_short.actual_len);

    const real_short = evaluate(true, 64, COMPLETION_SHORT_PACKET, 17);
    try testing.expect(real_short.accepted);
    try testing.expectEqual(@as(u32, 47), real_short.actual_len);

    try testing.expect(!evaluate(true, 64, COMPLETION_SUCCESS, 17).accepted);
    try testing.expect(!evaluate(true, 64, 12, 17).accepted);
    try testing.expect(!evaluate(false, 64, COMPLETION_SHORT_PACKET, 17).accepted);
}
