pub const TARGET_RATE: u32 = 48_000;
pub const TARGET_CHANNELS: u16 = 2;
pub const FORMAT_S16LE: u16 = 1;
pub const FORMAT_U8: u16 = 2;
pub const TARGET_FRAME_BYTES: usize = 4;
const PHASE_ONE: u64 = 65_536;

pub const ResamplerState = struct {
    rate: u32 = 0,
    channels: u16 = 0,
    format: u16 = 0,
    phase_q16: u64 = 0,
    chunk_done: bool = false,

    pub fn reset(self: *ResamplerState) void {
        self.* = .{};
    }

    pub fn resetIfFormatChanged(self: *ResamplerState, rate: u32, channels: u16, format: u16) void {
        if (self.rate == rate and self.channels == channels and self.format == format) return;
        self.rate = rate;
        self.channels = channels;
        self.format = format;
        self.phase_q16 = 0;
        self.chunk_done = false;
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

pub fn convertStreamingToStereoS16(
    state: *ResamplerState,
    input: []const u8,
    rate: u32,
    channels: u16,
    format: u16,
    output: []u8,
) usize {
    if (state.chunk_done) return 0;
    if (rate == 0) return 0;
    const frame_bytes = sourceFrameBytes(channels, format) orelse return 0;
    const input_frames = input.len / frame_bytes;
    if (input_frames == 0) return 0;

    state.resetIfFormatChanged(rate, channels, format);
    const step_q16 = (@as(u64, rate) * PHASE_ONE) / TARGET_RATE;
    if (step_q16 == 0) return 0;
    const input_frames_q16 = @as(u64, input_frames) * PHASE_ONE;

    var out_pos: usize = 0;
    while (out_pos + TARGET_FRAME_BYTES <= output.len and state.phase_q16 < input_frames_q16) {
        var src_index: usize = @intCast(state.phase_q16 >> 16);
        if (src_index >= input_frames) src_index = input_frames - 1;
        const next_index = if (src_index + 1 < input_frames) src_index + 1 else src_index;
        const frac: u32 = @truncate(state.phase_q16 & 0xFFFF);

        const a = readFrame(input, src_index, channels, format);
        const b = readFrame(input, next_index, channels, format);
        const left = lerpS16(a.left, b.left, frac);
        const right = lerpS16(a.right, b.right, frac);
        writeS16(output, out_pos, left);
        writeS16(output, out_pos + 2, right);
        out_pos += TARGET_FRAME_BYTES;
        state.phase_q16 += step_q16;
    }

    if (state.phase_q16 >= input_frames_q16) {
        state.phase_q16 -= input_frames_q16;
        state.chunk_done = true;
    }
    return out_pos;
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
    const word = @as(u16, data[pos]) | (@as(u16, data[pos + 1]) << 8);
    return @bitCast(word);
}

fn writeS16(data: []u8, pos: usize, value: i16) void {
    const word: u16 = @bitCast(value);
    data[pos] = @truncate(word);
    data[pos + 1] = @truncate(word >> 8);
}

fn u8ToS16(value: u8) i16 {
    return (@as(i16, @intCast(value)) - 128) << 8;
}

fn lerpS16(a: i16, b: i16, frac: u32) i16 {
    const av = @as(i32, a);
    const bv = @as(i32, b);
    const mixed = av + @divTrunc((bv - av) * @as(i32, @intCast(frac)), 65_536);
    return clampI16(mixed);
}

fn clampI16(value: i32) i16 {
    if (value > 32_767) return 32_767;
    if (value < -32_768) return -32_768;
    return @intCast(value);
}
