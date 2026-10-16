const count_work = @import("builtin").is_test or (@hasDecl(@import("root"), "audio_work_counters") and @import("root").audio_work_counters);
pub const TARGET_RATE: u32 = 48000;
pub const TARGET_CHANNELS: u16 = 2;
pub const FORMAT_S16LE: u16 = 1;
pub const FORMAT_U8: u16 = 2;
pub const TARGET_FRAME_BYTES: usize = 4;
pub const Work = extern struct { sample_reads: u64 = 0, sample_writes: u64 = 0, interpolations: u64 = 0, direct_bytes: u64 = 0 };
extern var r4os_rust_audio_pcm_work: Work;
pub const work = &r4os_rust_audio_pcm_work;
const StereoFrame = extern struct { left: i16 = 0, right: i16 = 0 };
pub const ResamplerState = extern struct {
    rate: u32 = 0,
    channels: u16 = 0,
    format: u16 = 0,
    phase_q16: u64 = 0,
    phase_remainder: u32 = 0,
    previous: StereoFrame = .{},
    previous_valid: bool = false,
    chunk_done: bool = false,
    pub fn reset(self: *@This()) void {
        _ = call(0, .{ .state = self });
    }
    pub fn resetIfFormatChanged(self: *@This(), rate: u32, channels: u16, format: u16) void {
        _ = call(1, .{ .state = self, .rate = rate, .channels = channels, .format = format });
    }
    pub fn beginChunk(self: *@This(), rate: u32, channels: u16, format: u16) void {
        _ = call(2, .{ .state = self, .rate = rate, .channels = channels, .format = format });
    }
};
const Span = extern struct { pointer: ?[*]const u8 = null, len: usize = 0 };
const Args = extern struct { state: ?*ResamplerState = null, input: ?[*]const u8 = null, input_len: usize = 0, output: ?[*]u8 = null, output_len: usize = 0, rate: u32 = 0, channels: u16 = 0, format: u16 = 0, start: usize = 0, work: ?*Work = if (count_work) work else null, span: ?*Span = null };
extern fn r4os_rust_audio_pcm(op: u32, args: *const Args) usize;
fn call(op: u32, args: Args) usize {
    return r4os_rust_audio_pcm(op, &args);
}
pub fn sourceFrameBytes(channels: u16, format: u16) ?usize {
    const n = call(3, .{ .channels = channels, .format = format });
    return if (n == 0) null else n;
}
pub fn outputFrameCount(input_len: usize, rate: u32, channels: u16, format: u16) usize {
    return call(4, .{ .input_len = input_len, .rate = rate, .channels = channels, .format = format });
}
pub fn takeDirectChunk(state: *ResamplerState, input: []const u8, rate: u32, channels: u16, format: u16, max_bytes: usize) ?[]const u8 {
    var span: Span = .{};
    if (call(5, .{ .state = state, .input = input.ptr, .input_len = input.len, .rate = rate, .channels = channels, .format = format, .output_len = max_bytes, .span = &span }) == 0) return null;
    return if (span.len == 0) input[0..0] else span.pointer.?[0..span.len];
}
pub fn convertToStereoS16(input: []const u8, rate: u32, channels: u16, format: u16, output: []u8, start_output_frame: usize) usize {
    return call(6, .{ .input = input.ptr, .input_len = input.len, .rate = rate, .channels = channels, .format = format, .output = output.ptr, .output_len = output.len, .start = start_output_frame });
}
pub fn convertStreamingToStereoS16(state: *ResamplerState, input: []const u8, rate: u32, channels: u16, format: u16, output: []u8) usize {
    return call(7, .{ .state = state, .input = input.ptr, .input_len = input.len, .rate = rate, .channels = channels, .format = format, .output = output.ptr, .output_len = output.len });
}
pub fn finishStreamingToStereoS16(state: *ResamplerState, output: []u8) usize {
    return call(8, .{ .state = state, .output = output.ptr, .output_len = output.len });
}
pub fn pendingTailBytes(state: *const ResamplerState) usize {
    return call(9, .{ .state = @constCast(state) });
}
fn readS16(input: []const u8, pos: usize) i16 {
    return @bitCast(@as(u16, @truncate(call(10, .{ .input = input.ptr, .input_len = input.len, .start = pos }))));
}
fn writeS16(output: []u8, pos: usize, value: i16) void {
    _ = call(11, .{ .output = output.ptr, .output_len = output.len, .start = pos, .rate = @as(u16, @bitCast(value)) });
}
comptime {
    if (@sizeOf(ResamplerState) != 32 or @alignOf(ResamplerState) != 8 or @offsetOf(ResamplerState, "previous") != 20 or @sizeOf(Args) != 72) @compileError("private PCM layout");
}
test "native PCM is bit exact with bounded partial outputs and borrowed input ownership" {
    const std = @import("std");
    const input = [_]u8{ 0, 128, 255, 127, 255, 255, 0, 0, 52, 18, 204, 237, 0xaa };
    var state = ResamplerState{};
    state.beginChunk(TARGET_RATE, 2, FORMAT_S16LE);
    work.* = .{};
    var out: [5]u8 = .{0xcc} ** 5;
    try std.testing.expectEqual(@as(usize, 0), convertStreamingToStereoS16(&state, &input, TARGET_RATE, 2, FORMAT_S16LE, out[0..3]));
    try std.testing.expectEqual(@as(u64, 0), state.phase_q16);
    for (0..3) |i| {
        try std.testing.expectEqual(@as(usize, 4), convertStreamingToStereoS16(&state, &input, TARGET_RATE, 2, FORMAT_S16LE, &out));
        try std.testing.expectEqualSlices(u8, input[i * 4 ..][0..4], out[0..4]);
        try std.testing.expectEqual(@as(u8, 0xcc), out[4]);
        try std.testing.expectEqual(i == 2, state.chunk_done);
    }
    try std.testing.expectEqual(@as(u64, 0), work.sample_reads + work.sample_writes + work.interpolations);
    try std.testing.expectEqual(@as(u64, 12), work.direct_bytes);
    state.beginChunk(TARGET_RATE, 2, FORMAT_S16LE);
    const direct = takeDirectChunk(&state, &input, TARGET_RATE, 2, FORMAT_S16LE, 8).?;
    try std.testing.expectEqual(@intFromPtr(&input), @intFromPtr(direct.ptr));
    try std.testing.expectEqual(@as(usize, 8), direct.len);
    try std.testing.expect(!state.chunk_done);
    try std.testing.expectEqual(@as(usize, 4), takeDirectChunk(&state, &input, TARGET_RATE, 2, FORMAT_S16LE, 8).?.len);
    try std.testing.expect(state.chunk_done);
}

test "format changes reset phase while fractional native phase and other formats keep conversion" {
    const std = @import("std");
    var state = ResamplerState{};
    var out: [16]u8 = undefined;
    const native = [_]u8{ 0, 128, 255, 127, 255, 255, 0, 0 };
    state.beginChunk(TARGET_RATE, 2, FORMAT_S16LE);
    state.phase_q16 = 1;
    work.* = .{};
    try std.testing.expectEqual(@as(usize, 4), convertStreamingToStereoS16(&state, &native, TARGET_RATE, 2, FORMAT_S16LE, &out));
    try std.testing.expectEqual(@as(u64, 1), state.phase_q16);
    try std.testing.expectEqual(@as(usize, 4), finishStreamingToStereoS16(&state, out[4..]));
    try std.testing.expectEqualSlices(u8, &native, out[0..8]);
    try std.testing.expectEqual(@as(u64, 2), work.interpolations);
    try std.testing.expectEqual(@as(u64, 0), work.direct_bytes);
    state.beginChunk(24_000, 1, FORMAT_S16LE);
    const mono = [_]u8{ 0, 0, 255, 127 };
    try std.testing.expectEqual(@as(usize, 12), convertStreamingToStereoS16(&state, &mono, 24_000, 1, FORMAT_S16LE, &out));
    try std.testing.expectEqual(@as(usize, 4), finishStreamingToStereoS16(&state, out[12..]));
    for ([_]i16{ 0, 16383, 32767, 32767 }, 0..) |sample, i| {
        try std.testing.expectEqual(sample, readS16(&out, i * 4));
        try std.testing.expectEqual(sample, readS16(&out, i * 4 + 2));
    }
    state.beginChunk(TARGET_RATE, 1, FORMAT_U8);
    try std.testing.expectEqual(@as(usize, 12), convertStreamingToStereoS16(&state, &.{ 0, 128, 255 }, TARGET_RATE, 1, FORMAT_U8, &out));
    for ([_]i16{ -32768, 0, 32512 }, 0..) |sample, i| try std.testing.expectEqual(sample, readS16(&out, i * 4));
    state.beginChunk(TARGET_RATE, 2, FORMAT_S16LE);
    try std.testing.expectEqual(@as(u64, 0), state.phase_q16);
    try std.testing.expectEqual(@as(usize, 8), convertStreamingToStereoS16(&state, &native, TARGET_RATE, 2, FORMAT_S16LE, &out));
    try std.testing.expectEqualSlices(u8, &native, out[0..8]);
}

fn renderTestChunks(input: []const u8, rate: u32, chunk_frames: usize, output_limit: usize, output: []u8) !usize {
    const std = @import("std");
    var state = ResamplerState{};
    var position: usize = 0;
    var used: usize = 0;
    while (position < input.len) {
        const chunk = input[position..@min(input.len, position + chunk_frames * 2)];
        state.beginChunk(rate, 1, FORMAT_S16LE);
        while (!state.chunk_done) {
            const count = convertStreamingToStereoS16(&state, chunk, rate, 1, FORMAT_S16LE, output[used..@min(output.len, used + output_limit)]);
            try std.testing.expect(count != 0 or state.chunk_done);
            used += count;
        }
        position += chunk.len;
    }
    while (state.previous_valid) {
        const count = finishStreamingToStereoS16(&state, output[used..@min(output.len, used + output_limit)]);
        try std.testing.expect(count != 0 or !state.previous_valid);
        used += count;
    }
    try std.testing.expectEqual(@as(usize, 0), finishStreamingToStereoS16(&state, output[used..]));
    return used;
}

test "streaming PCM preserves extreme samples duration and chunk boundaries" {
    const std = @import("std");
    var input: [32]u8 = undefined;
    const values = [_]i16{ -32768, 32767, 0, 3000, 4000, -3000, 1200, -1600, 0, 2000, 4000, 6000, 8000, 10000, -10000, 0 };
    for (values, 0..) |value, index| writeS16(&input, index * 2, value);
    var whole: [512]u8 = undefined;
    var split: [512]u8 = undefined;
    for ([_]u32{ 8000, 11025, 24000, 36000, 44100, 48000, 96000, 192000 }) |rate| {
        const expected = try renderTestChunks(&input, rate, values.len, whole.len, &whole);
        try std.testing.expectEqual(outputFrameCount(input.len, rate, 1, FORMAT_S16LE) * TARGET_FRAME_BYTES, expected);
        for ([_]usize{ 1, 4 }) |chunk| {
            const got = try renderTestChunks(&input, rate, chunk, TARGET_FRAME_BYTES, &split);
            try std.testing.expectEqualSlices(u8, whole[0..expected], split[0..got]);
        }
        if (rate == 36000) try std.testing.expectEqual(@as(i16, 16383), readS16(&whole, TARGET_FRAME_BYTES));
    }
}
