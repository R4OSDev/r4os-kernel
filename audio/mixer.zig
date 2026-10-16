const std = @import("std");
const count_work = @import("builtin").is_test or (@hasDecl(@import("root"), "audio_work_counters") and @import("root").audio_work_counters);
pub const Work = extern struct { accumulations: u64 = 0, sample_writes: u64 = 0, direct_bytes: u64 = 0 };
extern var r4os_rust_audio_mixer_work: Work;
pub const work = &r4os_rust_audio_mixer_work;
pub const Source = struct { ring: []const u8, read_pos: usize, volume: u32 };
const WireSource = extern struct { ring: [*]const u8, len: usize, read_pos: usize, volume: u32 };
pub const Owner = extern struct {
    instance_id: u32 = 0,
    generation: u64 = 0,
    pub fn valid(self: @This()) bool {
        return call(7, .{ .volume = self.instance_id, .total = @bitCast(self.generation) }) != 0;
    }
};
const Reader = *const fn (?*const anyopaque, usize, *WireSource) callconv(.c) void;
const Args = extern struct { ring: ?[*]u8 = null, ring_len: usize = 0, data: ?[*]const u8 = null, data_len: usize = 0, position: ?*usize = null, used: ?*usize = null, offset: usize = 0, volume: u32 = 0, total: i64 = 0, sample: i16 = 0, sources: ?*const anyopaque = null, count: usize = 0, work: ?*Work = if (count_work) work else null, reader: ?Reader = null };
extern fn r4os_rust_audio_mixer(op: u32, args: *const Args) i64;
fn call(op: u32, args: Args) i64 {
    return r4os_rust_audio_mixer(op, &args);
}
fn readSource(raw: ?*const anyopaque, index: usize, out: *WireSource) callconv(.c) void {
    const sources: [*]const Source = @ptrCast(@alignCast(raw.?));
    const source = sources[index];
    out.* = .{ .ring = source.ring.ptr, .len = source.ring.len, .read_pos = source.read_pos, .volume = source.volume };
}
pub fn mix(sources: []const Source, output: []u8) void {
    _ = call(0, .{ .ring = output.ptr, .ring_len = output.len, .sources = sources.ptr, .count = sources.len, .reader = readSource });
}
pub fn sameOwner(a: Owner, b: Owner) bool {
    return call(8, .{ .ring = @ptrCast(@constCast(&a)), .data = @ptrCast(&b) }) != 0;
}
pub fn ringWrite(ring: []u8, write_pos: *usize, used: *usize, data: []const u8) usize {
    return @intCast(call(1, .{ .ring = ring.ptr, .ring_len = ring.len, .position = write_pos, .used = used, .data = data.ptr, .data_len = data.len }));
}
pub fn ringReadS16(ring: []const u8, read_pos: usize, byte_offset: usize) i16 {
    return @intCast(call(2, .{ .ring = @constCast(ring.ptr), .ring_len = ring.len, .offset = read_pos, .data_len = byte_offset }));
}
pub fn ringConsume(ring_len: usize, read_pos: *usize, used: *usize, byte_count: usize) void {
    _ = call(3, .{ .ring_len = ring_len, .position = read_pos, .used = used, .offset = byte_count });
}
pub fn accumulateSample(total: i64, sample: i16, volume_fixed: u32) i64 {
    return call(4, .{ .total = total, .sample = sample, .volume = volume_fixed });
}
pub fn clampSample(total: i64) i16 {
    return @intCast(call(5, .{ .total = total }));
}
pub fn writeS16(out: []u8, offset: usize, sample: i16) void {
    _ = call(6, .{ .ring = out.ptr, .ring_len = out.len, .offset = offset, .sample = sample });
}
comptime {
    if (@sizeOf(Owner) != 16 or @sizeOf(WireSource) != 32 or @sizeOf(Args) != 112) @compileError("private mixer layout");
}
test "stream owners include the generation" {
    const first = Owner{ .instance_id = 7, .generation = 11 };
    try std.testing.expect(first.valid());
    try std.testing.expect(sameOwner(first, .{ .instance_id = 7, .generation = 11 }));
    try std.testing.expect(!sameOwner(first, .{ .instance_id = 7, .generation = 12 }));
    try std.testing.expect(!(Owner{ .instance_id = 7 }).valid());
}

test "ring write and sample read preserve wrapped PCM" {
    var ring: [8]u8 = .{0} ** 8;
    var write_pos: usize = 6;
    var used: usize = 0;
    const pcm = [_]u8{ 0x34, 0x12, 0xCC, 0xED };
    try std.testing.expectEqual(@as(usize, pcm.len), ringWrite(ring[0..], &write_pos, &used, pcm[0..]));
    try std.testing.expectEqual(@as(i16, 0x1234), ringReadS16(ring[0..], 6, 0));
    try std.testing.expectEqual(@as(i16, -0x1234), ringReadS16(ring[0..], 6, 2));
    var read_pos: usize = 6;
    ringConsume(ring.len, &read_pos, &used, 4);
    try std.testing.expectEqual(@as(usize, 2), read_pos);
    try std.testing.expectEqual(@as(usize, 0), used);
}

test "fixed point volume participates in saturating mix" {
    var total: i64 = 0;
    total = accumulateSample(total, 20_000, 0x0001_0000);
    total = accumulateSample(total, 20_000, 0x0000_8000);
    try std.testing.expectEqual(@as(i16, 30_000), clampSample(total));
    total = accumulateSample(total, 20_000, 0x0001_0000);
    try std.testing.expectEqual(@as(i16, 32_767), clampSample(total));

    var out: [2]u8 = undefined;
    writeS16(out[0..], 0, -12_345);
    try std.testing.expectEqual(@as(i16, -12_345), ringReadS16(out[0..], 0, 0));
}

test "mix keeps unity PCM bit exact across wrap, and still scales and saturates multiple streams" {
    var ring: [16]u8 = undefined;
    for (&ring, 0..) |*b, i| b.* = @truncate(i * 31 + 17);
    const source = Source{ .ring = &ring, .read_pos = 12, .volume = 65536 };
    var out: [12]u8 = undefined;
    work.* = .{};
    mix(&.{source}, &out);
    for (0..6) |i| try std.testing.expectEqual(ringReadS16(&ring, 12, i * 2), ringReadS16(&out, 0, i * 2));
    try std.testing.expectEqual(@as(u64, 0), work.accumulations);
    try std.testing.expectEqual(@as(u64, 12), work.direct_bytes);
    for ([_]u32{ 0, 32768, 65536, 131072 }) |volume| {
        var scaled = source;
        scaled.volume = volume;
        mix(&.{scaled}, &out);
        for (0..6) |i| {
            const expected = clampSample(@divTrunc(@as(i64, ringReadS16(&ring, 12, i * 2)) * volume, 65536));
            try std.testing.expectEqual(expected, ringReadS16(&out, 0, i * 2));
        }
        mix(&.{ source, scaled }, &out);
        for (0..6) |i| {
            const a: i64 = ringReadS16(&ring, 12, i * 2);
            try std.testing.expectEqual(clampSample(a + @divTrunc(a * volume, 65536)), ringReadS16(&out, 0, i * 2));
        }
    }
}
