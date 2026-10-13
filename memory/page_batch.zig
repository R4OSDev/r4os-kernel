const std = @import("std");
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}

pub const max_extent_pages: u64 = 64;
pub const max_translation_bytes: u64 = 256 * 1024 * 1024;

const RawTranslation = extern struct {
    virtual_base: u64,
    bytes: u64,
    consumed: u64,
    physical: u64,
    physical_present: bool,
};
extern fn r4os_rust_translation_init(virtual_base: u64, bytes: u64, out: *RawTranslation) bool;
extern fn r4os_rust_translation_accept(value: *RawTranslation, address: u64, remaining: u64) bool;
extern fn r4os_rust_translation_result(value: *const RawTranslation, out: *u64) bool;
extern fn r4os_rust_page_batch_bound(remaining: u64) u64;
extern fn r4os_rust_page_batch_prefix(frames: [*]const u64, count: usize, page: u64) usize;

/// Accumulate fresh translations while the caller retains the virtual range.
/// A leaf covers every byte to its boundary, including huge leaves. This
/// stores only values; no page-table pointer survives an owner handoff.
pub const ContiguousTranslation = struct {
    virtual: u64,
    bytes: u64,
    consumed: u64 = 0,
    physical: ?u64 = null,

    pub fn init(virtual: u64, bytes: u64) ?ContiguousTranslation {
        var raw: RawTranslation = undefined;
        return if (r4os_rust_translation_init(virtual, bytes, &raw)) fromRaw(raw) else null;
    }
    pub fn accept(self: *ContiguousTranslation, address: u64, leaf_remaining: u64) bool {
        var raw = self.toRaw();
        const accepted = r4os_rust_translation_accept(&raw, address, leaf_remaining);
        self.* = fromRaw(raw);
        return accepted;
    }
    pub fn result(self: *const ContiguousTranslation) ?u64 {
        const raw = self.toRaw();
        var physical: u64 = undefined;
        return if (r4os_rust_translation_result(&raw, &physical)) physical else null;
    }
    fn toRaw(self: *const ContiguousTranslation) RawTranslation {
        return .{ .virtual_base = self.virtual, .bytes = self.bytes, .consumed = self.consumed, .physical = self.physical orelse 0, .physical_present = self.physical != null };
    }
    fn fromRaw(raw: RawTranslation) ContiguousTranslation {
        return .{ .virtual = raw.virtual_base, .bytes = raw.bytes, .consumed = raw.consumed, .physical = if (raw.physical_present) raw.physical else null };
    }
};

pub fn boundedPageCount(remaining_pages: u64) u64 {
    return r4os_rust_page_batch_bound(remaining_pages);
}

/// Counts the naturally contiguous prefix of already acquired frames. It is
/// deliberately not a physical-memory search and therefore cannot turn an
/// ordinary VM commit into an unbounded contiguous-allocation scan.
pub fn contiguousPrefix(frames: []const u64, page_size: u64) usize {
    return r4os_rust_page_batch_prefix(frames.ptr, frames.len, page_size);
}
comptime {
    if (@sizeOf(RawTranslation) != 40) @compileError("Rust translation C layout mismatch");
}

test "batch size remains stack and latency bounded" {
    try std.testing.expectEqual(@as(u64, 0), boundedPageCount(0));
    try std.testing.expectEqual(@as(u64, 7), boundedPageCount(7));
    try std.testing.expectEqual(max_extent_pages, boundedPageCount(1000));
}

test "natural extent grouping stops at the first discontinuity" {
    const frames = [_]u64{ 0x1000, 0x2000, 0x3000, 0x9000, 0xA000 };
    try std.testing.expectEqual(@as(usize, 3), contiguousPrefix(frames[0..], 4096));
    try std.testing.expectEqual(@as(usize, 2), contiguousPrefix(frames[3..], 4096));
    try std.testing.expectEqual(@as(usize, 0), contiguousPrefix(frames[0..0], 4096));
}

test "fresh contiguous translation covers partial huge leaves and rejects late holes and overflow" {
    const t = std.testing;
    const start: u64 = 0x1ff123;
    var mixed = ContiguousTranslation.init(start, 0x200000 + 0x1000).?;
    try t.expect(mixed.accept(start + 0x40000000, 0x200000 - start));
    try t.expect(mixed.result() == null);
    try t.expect(mixed.accept(0x40200000, 0x200000));
    try t.expect(mixed.result() == null);
    try t.expect(mixed.accept(0x40400000, 0x1000));
    try t.expectEqual(@as(?u64, start + 0x40000000), mixed.result());
    try t.expectEqual(mixed.bytes, mixed.consumed);
    // A bad final 4K leaf cannot be hidden by the previous owner chunk or
    // by checking only the first and last addresses of a larger range.
    for ([_]u64{ 0, 0x80000000 }) |bad_span| {
        var late = ContiguousTranslation.init(0x1000, 65 * 4096 + 1).?;
        for (0..64) |i| try t.expect(late.accept(0x100000 + i * 4096, 4096));
        const consumed = late.consumed;
        try t.expect(!late.accept(bad_span, if (bad_span == 0) 0 else 4096));
        try t.expectEqual(consumed, late.consumed);
        try t.expect(late.result() == null);
        try t.expect(late.accept(0x140000, 4096) and late.result() == null);
        try t.expect(late.accept(0x141000, 4096));
        try t.expectEqual(@as(?u64, 0x100000), late.result());
        try t.expectEqual(@as(u64, 65 * 4096 + 1), late.consumed);
    }
    try t.expect(ContiguousTranslation.init(0, 0) == null);
    try t.expect(ContiguousTranslation.init(0, max_translation_bytes + 1) == null);
    try t.expect(ContiguousTranslation.init(std.math.maxInt(u64), 2) == null);
    var overflow = ContiguousTranslation.init(0, 4096).?;
    try t.expect(!overflow.accept(std.math.maxInt(u64) - 4094, 4096));
    try t.expect(overflow.result() == null and overflow.consumed == 0);
    try t.expect(overflow.accept(std.math.maxInt(u64) - 4095, 4096));
    try t.expectEqual(@as(?u64, std.math.maxInt(u64) - 4095), overflow.result());
}
