pub const TARGET_RATE: u32 = 48_000;
pub const TARGET_CHANNELS: u16 = 2;
pub const FORMAT_S16LE: u16 = 1;
pub const FORMAT_U8: u16 = 2;
pub const TARGET_FRAME_BYTES: usize = 4;
const PHASE_ONE: u64 = 65_536;
const count_work = @import("builtin").is_test or
    (@hasDecl(@import("root"), "audio_work_counters") and @import("root").audio_work_counters);
pub const Work = struct { sample_reads: u64 = 0, sample_writes: u64 = 0, interpolations: u64 = 0, direct_bytes: u64 = 0 };
pub var work: Work = .{};

pub const ResamplerState = struct {
    rate: u32 = 0,
    channels: u16 = 0,
    format: u16 = 0,
    phase_q16: u64 = 0,
    phase_remainder: u32 = 0,
    previous: StereoFrame = .{ .left = 0, .right = 0 },
    previous_valid: bool = false,
    chunk_done: bool = false,

    pub fn reset(self: *ResamplerState) void {
        self.* = .{};
    }

    pub fn resetIfFormatChanged(self: *ResamplerState, rate: u32, channels: u16, format: u16) void {
        if (self.rate == rate and self.channels == channels and self.format == format) return;
        self.* = .{ .rate = rate, .channels = channels, .format = format };
    }

    pub fn beginChunk(self: *ResamplerState, rate: u32, channels: u16, format: u16) void {
        self.resetIfFormatChanged(rate, channels, format);
        self.chunk_done = false;
    }
};

pub fn sourceFrameBytes(channels: u16, format: u16) ?usize {
    if (channels == 0 or channels > 2) return null;
    return switch (format) {
        FORMAT_S16LE => @as(usize, channels) * 2,
        FORMAT_U8 => @as(usize, channels),
        else => null,
    };
}

pub fn outputFrameCount(input_len: usize, rate: u32, channels: u16, format: u16) usize {
    if (rate == 0) return 0;
    const frame_bytes = sourceFrameBytes(channels, format) orelse return 0;
    const input_frames = input_len / frame_bytes;
    if (input_frames == 0) return 0;
    const frames = (@as(u64, input_frames) * TARGET_RATE + rate - 1) / rate;
    return @intCast(frames);
}

/// Borrow a bounded native-format span until the caller copies it into its
/// own ring/DMA storage. Null requires conversion; an empty span makes no
/// progress. Fractional phase must retain interpolation, even at target rate.
pub fn takeDirectChunk(state: *ResamplerState, input: []const u8, rate: u32, channels: u16, format: u16, max_bytes: usize) ?[]const u8 {
    if (rate != TARGET_RATE or channels != TARGET_CHANNELS or format != FORMAT_S16LE or input.len < TARGET_FRAME_BYTES) return null;
    state.resetIfFormatChanged(rate, channels, format);
    if (state.previous_valid or state.phase_q16 & (PHASE_ONE - 1) != 0) return null;
    if (state.chunk_done) return input[0..0];
    const frames = input.len / TARGET_FRAME_BYTES;
    const limit = @as(u64, frames) * PHASE_ONE;
    if (state.phase_q16 >= limit) {
        state.phase_q16 -= limit;
        state.chunk_done = true;
        return input[0..0];
    }
    const first: usize = @intCast(state.phase_q16 / PHASE_ONE);
    const count = @min(frames - first, max_bytes / TARGET_FRAME_BYTES);
    state.phase_q16 += @as(u64, count) * PHASE_ONE;
    if (state.phase_q16 >= limit) {
        state.phase_q16 -= limit;
        state.chunk_done = true;
    }
    if (count_work) work.direct_bytes += count * TARGET_FRAME_BYTES;
    return input[first * TARGET_FRAME_BYTES ..][0 .. count * TARGET_FRAME_BYTES];
}

pub fn convertToStereoS16(
    input: []const u8,
    rate: u32,
    channels: u16,
    format: u16,
    output: []u8,
    start_output_frame: usize,
) usize {
    if (rate == 0) return 0;
    const frame_bytes = sourceFrameBytes(channels, format) orelse return 0;
    const input_frames = input.len / frame_bytes;
    if (input_frames == 0) return 0;

    const total_output_frames = outputFrameCount(input.len, rate, channels, format);
    if (start_output_frame >= total_output_frames) return 0;

    var out_pos: usize = 0;
    var out_frame = start_output_frame;
    while (out_pos + TARGET_FRAME_BYTES <= output.len and out_frame < total_output_frames) : (out_frame += 1) {
        const scaled = (@as(u64, out_frame) * rate * 65_536) / TARGET_RATE;
        var src_index: usize = @intCast(scaled >> 16);
        if (src_index >= input_frames) src_index = input_frames - 1;
        const next_index = if (src_index + 1 < input_frames) src_index + 1 else src_index;
        const frac: u32 = @truncate(scaled & 0xFFFF);

        const a = readFrame(input, src_index, channels, format);
        const b = readFrame(input, next_index, channels, format);
        const left = lerpS16(a.left, b.left, frac);
        const right = lerpS16(a.right, b.right, frac);
        writeS16(output, out_pos, left);
        writeS16(output, out_pos + 2, right);
        out_pos += TARGET_FRAME_BYTES;
    }
    return out_pos;
}

/// Consume one caller-owned chunk in bounded output slices. A fractional
/// position after its last sample waits for the next chunk instead of inventing
/// a repeated boundary sample. Only that last decoded frame is retained.
pub fn convertStreamingToStereoS16(
    state: *ResamplerState,
    input: []const u8,
    rate: u32,
    channels: u16,
    format: u16,
    output: []u8,
) usize {
    if (state.chunk_done or rate == 0) return 0;
    const frame_bytes = sourceFrameBytes(channels, format) orelse return 0;
    const input_frames = input.len / frame_bytes;
    if (input_frames == 0) return 0;

    if (takeDirectChunk(state, input, rate, channels, format, output.len)) |direct| {
        @memcpy(output[0..direct.len], direct);
        return direct.len;
    }

    state.resetIfFormatChanged(rate, channels, format);
    const prefix: usize = @intFromBool(state.previous_valid);
    const last_position = @as(u64, input_frames + prefix - 1) * PHASE_ONE;
    var out_pos: usize = 0;
    while (out_pos + TARGET_FRAME_BYTES <= output.len and state.phase_q16 <= last_position) {
        const src_index: usize = @intCast(state.phase_q16 / PHASE_ONE);
        const frac: u32 = @truncate(state.phase_q16 & (PHASE_ONE - 1));
        const a = streamingFrame(state, input, src_index, prefix, channels, format);
        const b = if (frac == 0) a else streamingFrame(state, input, src_index + 1, prefix, channels, format);
        writeS16(output, out_pos, lerpS16(a.left, b.left, frac));
        writeS16(output, out_pos + 2, lerpS16(a.right, b.right, frac));
        out_pos += TARGET_FRAME_BYTES;
        advancePhase(state);
    }
    if (state.phase_q16 > last_position) {
        state.phase_q16 -= last_position;
        state.previous = readFrame(input, input_frames - 1, channels, format);
        state.previous_valid = true;
        state.chunk_done = true;
    }
    return out_pos;
}

fn streamingFrame(state: *const ResamplerState, input: []const u8, index: usize, prefix: usize, channels: u16, format: u16) StereoFrame {
    if (prefix != 0 and index == 0) return state.previous;
    return readFrame(input, index - prefix, channels, format);
}

fn advancePhase(state: *ResamplerState) void {
    const numerator = @as(u64, state.rate) * PHASE_ONE;
    state.phase_q16 += numerator / TARGET_RATE;
    state.phase_remainder += @intCast(numerator % TARGET_RATE);
    if (state.phase_remainder >= TARGET_RATE) {
        state.phase_q16 += 1;
        state.phase_remainder -= TARGET_RATE;
    }
}

/// End-of-stream alone extends the last sample to its full source-frame
/// duration. May be called repeatedly with bounded output; no input is borrowed.
/// A new chunk must not begin until this final tail is fully drained.
pub fn finishStreamingToStereoS16(state: *ResamplerState, output: []u8) usize {
    if (!state.chunk_done or !state.previous_valid) return 0;
    var out_pos: usize = 0;
    while (out_pos + TARGET_FRAME_BYTES <= output.len and state.phase_q16 < PHASE_ONE) {
        writeS16(output, out_pos, state.previous.left);
        writeS16(output, out_pos + 2, state.previous.right);
        out_pos += TARGET_FRAME_BYTES;
        advancePhase(state);
    }
    if (state.phase_q16 >= PHASE_ONE) {
        state.previous_valid = false;
        state.phase_q16 = 0;
        state.phase_remainder = 0;
    }
    return out_pos;
}

/// Output retained only to complete the final source frame's duration.
pub fn pendingTailBytes(state: *const ResamplerState) usize {
    if (!state.chunk_done or !state.previous_valid or state.rate == 0 or state.phase_q16 >= PHASE_ONE) return 0;
    const remaining = (PHASE_ONE - state.phase_q16) * TARGET_RATE - state.phase_remainder;
    const step = @as(u64, state.rate) * PHASE_ONE;
    return @intCast(((remaining + step - 1) / step) * TARGET_FRAME_BYTES);
}

const StereoFrame = struct {
    left: i16,
    right: i16,
};

fn readFrame(input: []const u8, frame_index: usize, channels: u16, format: u16) StereoFrame {
    const frame_bytes = sourceFrameBytes(channels, format) orelse return .{ .left = 0, .right = 0 };
    const pos = frame_index * frame_bytes;
    if (format == FORMAT_S16LE) {
        const left = readS16(input, pos);
        const right = if (channels == 2) readS16(input, pos + 2) else left;
        return .{ .left = left, .right = right };
    }

    const left = u8ToS16(input[pos]);
    const right = if (channels == 2) u8ToS16(input[pos + 1]) else left;
    return .{ .left = left, .right = right };
}

fn readS16(data: []const u8, pos: usize) i16 {
    if (count_work) work.sample_reads += 1;
    const word = @as(u16, data[pos]) | (@as(u16, data[pos + 1]) << 8);
    return @bitCast(word);
}

fn writeS16(data: []u8, pos: usize, value: i16) void {
    if (count_work) work.sample_writes += 1;
    const word: u16 = @bitCast(value);
    data[pos] = @truncate(word);
    data[pos + 1] = @truncate(word >> 8);
}

fn u8ToS16(value: u8) i16 {
    return (@as(i16, @intCast(value)) - 128) << 8;
}

fn lerpS16(a: i16, b: i16, frac: u32) i16 {
    if (count_work) work.interpolations += 1;
    const av = @as(i64, a);
    const bv = @as(i64, b);
    const mixed = av + @divTrunc((bv - av) * @as(i64, frac), 65_536);
    return clampI16(mixed);
}

fn clampI16(value: i64) i16 {
    if (value > 32_767) return 32_767;
    if (value < -32_768) return -32_768;
    return @intCast(value);
}

test "native PCM is bit exact with bounded partial outputs and borrowed input ownership" {
    const std = @import("std");
    const input = [_]u8{ 0, 128, 255, 127, 255, 255, 0, 0, 52, 18, 204, 237, 0xaa };
    var state = ResamplerState{};
    state.beginChunk(TARGET_RATE, 2, FORMAT_S16LE);
    work = .{};
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
    work = .{};
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
