const std = @import("std");
const count_work = @import("builtin").is_test or
    (@hasDecl(@import("root"), "audio_work_counters") and @import("root").audio_work_counters);
pub const Work = struct { accumulations: u64 = 0, sample_writes: u64 = 0, direct_bytes: u64 = 0 };
pub var work: Work = .{};

pub const Source = struct { ring: []const u8, read_pos: usize, volume: u32 };

/// Sources already own at least output.len bytes; the stream owner retains
/// them until the backend acknowledges the complete mixed block.
pub fn mix(sources: []const Source, output: []u8) void {
    if (sources.len == 1 and sources[0].volume == 65_536) {
        const source = sources[0];
        const first = @min(output.len, source.ring.len - source.read_pos);
        @memcpy(output[0..first], source.ring[source.read_pos..][0..first]);
        @memcpy(output[first..], source.ring[0 .. output.len - first]);
        if (count_work) work.direct_bytes += output.len;
        return;
    }
    var byte_offset: usize = 0;
    while (byte_offset < output.len) : (byte_offset += 2) {
        var total: i64 = 0;
        for (sources) |source| {
            total = accumulateSample(total, ringReadS16(source.ring, source.read_pos, byte_offset), source.volume);
        }
        writeS16(output, byte_offset, clampSample(total));
    }
}

pub const Owner = struct {
    instance_id: u32 = 0,
    generation: u64 = 0,

    pub fn valid(self: Owner) bool {
        return (self.instance_id == 0 and self.generation == 0) or
            (self.instance_id != 0 and self.generation != 0);
    }
};

pub fn sameOwner(a: Owner, b: Owner) bool {
    return a.instance_id == b.instance_id and a.generation == b.generation;
}

pub fn ringWrite(ring: []u8, write_pos: *usize, used: *usize, data: []const u8) usize {
    if (ring.len == 0 or used.* >= ring.len or data.len == 0) return 0;
    const count = @min(data.len, ring.len - used.*);
    const first = @min(count, ring.len - write_pos.*);
    @memcpy(ring[write_pos.* .. write_pos.* + first], data[0..first]);
    if (first < count) @memcpy(ring[0 .. count - first], data[first..count]);
    write_pos.* = (write_pos.* + count) % ring.len;
    used.* += count;
    return count;
}

pub fn ringReadS16(ring: []const u8, read_pos: usize, byte_offset: usize) i16 {
    if (ring.len < 2) return 0;
    const low_index = (read_pos + byte_offset) % ring.len;
    const high_index = (low_index + 1) % ring.len;
    const word = @as(u16, ring[low_index]) | (@as(u16, ring[high_index]) << 8);
    return @bitCast(word);
}

pub fn ringConsume(ring_len: usize, read_pos: *usize, used: *usize, byte_count: usize) void {
    if (ring_len == 0 or used.* == 0) return;
    const count = @min(byte_count, used.*);
    read_pos.* = (read_pos.* + count) % ring_len;
    used.* -= count;
}

pub fn accumulateSample(total: i64, sample: i16, volume_fixed: u32) i64 {
    if (count_work) work.accumulations += 1;
    return total + @divTrunc(@as(i64, sample) * @as(i64, volume_fixed), 65_536);
}

pub fn clampSample(total: i64) i16 {
    if (total > 32_767) return 32_767;
    if (total < -32_768) return -32_768;
    return @intCast(total);
}

pub fn writeS16(out: []u8, offset: usize, sample: i16) void {
    if (count_work) work.sample_writes += 1;
    const word: u16 = @bitCast(sample);
    out[offset] = @truncate(word);
    out[offset + 1] = @truncate(word >> 8);
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
    work = .{};
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
