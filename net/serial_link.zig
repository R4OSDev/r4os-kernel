const serial = @import("../driver/com.zig");
const protocol_api = @import("../kernel/protocol_api.zig");
const r4p = @import("../program/r4p.zig");
const r4p_contract = @import("r4p_contract.zig");

pub const MAGIC: [4]u8 = .{ 'R', '4', 'S', 'L' };
pub const VERSION: u8 = 1;
pub const TYPE_DIAG: u8 = 1;
pub const TYPE_MESSAGE: u8 = 2;
pub const HEADER_SIZE: usize = 10;
pub const MAX_PAYLOAD: usize = 256;
pub const MAX_FRAME: usize = HEADER_SIZE + MAX_PAYLOAD;

const State = struct {
    present: bool = false,
    initialized: bool = false,
    port_base: u16 = serial.COM2,
    loopback_tests: u64 = 0,
    host_tests: u64 = 0,
    message_tx: u64 = 0,
    message_rx: u64 = 0,
    tx_skipped: u64 = 0,
    tx_frames: u64 = 0,
    tx_bytes: u64 = 0,
    rx_frames: u64 = 0,
    rx_bytes: u64 = 0,
    polls: u64 = 0,
    bad_magic: u64 = 0,
    bad_version: u64 = 0,
    bad_length: u64 = 0,
    checksum_errors: u64 = 0,
    overflows: u64 = 0,
    timeouts: u64 = 0,
    r4p_build: u64 = 0,
    r4p_parse: u64 = 0,
    r4p_self: u64 = 0,
    r4p_dispatch_failures: u64 = 0,
    last_type: u8 = 0,
    last_payload_len: u16 = 0,
    last_payload: [MAX_PAYLOAD]u8 = .{0} ** MAX_PAYLOAD,
    last_message_len: u16 = 0,
    last_message: [MAX_PAYLOAD]u8 = .{0} ** MAX_PAYLOAD,
    last_error: []const u8 = "none",
};

pub const Snapshot = struct {
    present: bool = false,
    initialized: bool = false,
    port_base: u16 = serial.COM2,
    loopback_tests: u64 = 0,
    host_tests: u64 = 0,
    message_tx: u64 = 0,
    message_rx: u64 = 0,
    tx_skipped: u64 = 0,
    tx_frames: u64 = 0,
    tx_bytes: u64 = 0,
    rx_frames: u64 = 0,
    rx_bytes: u64 = 0,
    polls: u64 = 0,
    bad_magic: u64 = 0,
    bad_version: u64 = 0,
    bad_length: u64 = 0,
    checksum_errors: u64 = 0,
    overflows: u64 = 0,
    timeouts: u64 = 0,
    r4p_build: u64 = 0,
    r4p_parse: u64 = 0,
    r4p_self: u64 = 0,
    r4p_dispatch_failures: u64 = 0,
    last_type: u8 = 0,
    last_payload_len: u16 = 0,
    last_message_len: u16 = 0,
    last_payload: [MAX_PAYLOAD]u8 = .{0} ** MAX_PAYLOAD,
    last_message: [MAX_PAYLOAD]u8 = .{0} ** MAX_PAYLOAD,
    last_error: [32]u8 = .{0} ** 32,
};

pub const Message = struct {
    len: u16 = 0,
    data: [MAX_PAYLOAD]u8 = .{0} ** MAX_PAYLOAD,
};

var state: State = .{};

pub fn init() void {
    const present = serial.com2.isPresent();
    state = .{
        .present = present,
        .initialized = present,
        .port_base = serial.COM2,
        .last_error = if (present) "none" else "com2-missing",
    };
    if (present) serial.com2.init();
}

pub fn poll() void {
    if (!state.present or !state.initialized) return;
    state.polls += 1;
    var guard: usize = 0;
    while (guard < 256) : (guard += 1) {
        const byte = serial.com2.getc() orelse return;
        parseByte(byte);
    }
    state.timeouts += 1;
    state.last_error = "poll-budget";
}

pub fn sendFrame(frame_type: u8, payload: []const u8) bool {
    if (!state.present) return fail("com2-missing");
    if (!state.initialized) return fail("not-initialized");
    var frame: [MAX_FRAME]u8 = undefined;
    const len = buildFramePath(frame_type, payload, frame[0..]) orelse {
        state.bad_length += 1;
        state.last_error = "tx-length";
        return false;
    };
    writeFrame(frame[0..len]);
    return true;
}

pub fn sendMessage(payload: []const u8) bool {
    if (sendFrame(TYPE_MESSAGE, payload)) {
        state.message_tx += 1;
        return true;
    }
    return false;
}

pub fn snapshot(out: *Snapshot) void {
    out.* = .{
        .present = state.present,
        .initialized = state.initialized,
        .port_base = state.port_base,
        .loopback_tests = state.loopback_tests,
        .host_tests = state.host_tests,
        .message_tx = state.message_tx,
        .message_rx = state.message_rx,
        .tx_skipped = state.tx_skipped,
        .tx_frames = state.tx_frames,
        .tx_bytes = state.tx_bytes,
        .rx_frames = state.rx_frames,
        .rx_bytes = state.rx_bytes,
        .polls = state.polls,
        .bad_magic = state.bad_magic,
        .bad_version = state.bad_version,
        .bad_length = state.bad_length,
        .checksum_errors = state.checksum_errors,
        .overflows = state.overflows,
        .timeouts = state.timeouts,
        .r4p_build = state.r4p_build,
        .r4p_parse = state.r4p_parse,
        .r4p_self = state.r4p_self,
        .r4p_dispatch_failures = state.r4p_dispatch_failures,
        .last_type = state.last_type,
        .last_payload_len = state.last_payload_len,
        .last_message_len = state.last_message_len,
    };
    const payload_len: usize = @intCast(state.last_payload_len);
    const message_len: usize = @intCast(state.last_message_len);
    if (payload_len != 0) @memcpy(out.last_payload[0..payload_len], state.last_payload[0..payload_len]);
    if (message_len != 0) @memcpy(out.last_message[0..message_len], state.last_message[0..message_len]);
    copyFixed(out.last_error[0..], state.last_error);
}

pub fn lastMessage(out: *Message) bool {
    out.* = .{};
    if (state.last_message_len == 0) return false;
    const message_len: usize = @intCast(state.last_message_len);
    out.len = state.last_message_len;
    @memcpy(out.data[0..message_len], state.last_message[0..message_len]);
    return true;
}

fn writeFrame(frame: []const u8) void {
    for (frame) |byte| serial.com2.putc(byte);
    state.tx_frames += 1;
    state.tx_bytes += frame.len;
    state.last_error = "none";
}

fn parseByte(byte: u8) void {
    if (!r4p.hasActiveR4p("net.serial_link")) {
        state.r4p_dispatch_failures += 1;
        state.last_error = "r4p-required";
        return;
    }
    if (!parseByteR4p(byte)) state.last_error = "r4p-dispatch";
}

fn buildFramePath(frame_type: u8, payload: []const u8, out: []u8) ?usize {
    var op = newR4slOp() orelse {
        state.r4p_dispatch_failures += 1;
        state.last_error = "r4p-required";
        return null;
    };
    if (payload.len > op.payload.len) {
        state.bad_length += 1;
        state.last_error = "payload-size";
        return null;
    }
    op.frame_type = frame_type;
    op.payload_len = @intCast(payload.len);
    if (payload.len != 0) @memcpy(op.payload[0..payload.len], payload);
    if (!dispatchR4sl(r4p_contract.R4SL_OP_BUILD_FRAME, &op) or op.result != r4p_contract.R4SL_RESULT_OK or op.frame_len > out.len) {
        state.r4p_dispatch_failures += 1;
        state.last_error = "r4p-build";
        return null;
    }
    const len: usize = @intCast(op.frame_len);
    @memcpy(out[0..len], op.frame[0..len]);
    state.r4p_build += 1;
    state.last_error = "none";
    return out[0..len].len;
}

fn parseByteR4p(byte: u8) bool {
    var op = newR4slOp() orelse return false;
    op.input[0] = byte;
    op.input_len = 1;
    if (!dispatchR4sl(r4p_contract.R4SL_OP_PARSE_BYTES, &op)) return false;
    switch (op.result) {
        r4p_contract.R4SL_RESULT_NEED_MORE => return true,
        r4p_contract.R4SL_RESULT_OK => {
            if (op.completed != 0) {
                applyR4slFrame(op);
                state.r4p_parse += 1;
            }
            return true;
        },
        r4p_contract.R4SL_RESULT_BAD_MAGIC => {
            state.bad_magic += 1;
            state.last_error = "bad-magic";
            return true;
        },
        r4p_contract.R4SL_RESULT_BAD_VERSION => {
            state.bad_version += 1;
            state.last_error = "bad-version";
            return true;
        },
        r4p_contract.R4SL_RESULT_BAD_LENGTH => {
            state.bad_length += 1;
            state.last_error = "bad-length";
            return true;
        },
        r4p_contract.R4SL_RESULT_CHECKSUM => {
            state.checksum_errors += 1;
            state.last_error = "checksum";
            return true;
        },
        r4p_contract.R4SL_RESULT_OVERFLOW => {
            state.overflows += 1;
            state.last_error = "overflow";
            return true;
        },
        else => {
            state.r4p_dispatch_failures += 1;
            return false;
        },
    }
}

fn applyR4slFrame(op: r4p_contract.R4slOp) void {
    const payload_len: usize = @intCast(op.payload_len);
    state.rx_frames += 1;
    state.rx_bytes += payload_len;
    state.last_type = op.frame_type;
    state.last_payload_len = @intCast(payload_len);
    if (payload_len != 0) @memcpy(state.last_payload[0..payload_len], op.payload[0..payload_len]);
    if (op.frame_type == TYPE_MESSAGE) {
        state.message_rx += 1;
        state.last_message_len = @intCast(payload_len);
        if (payload_len != 0) @memcpy(state.last_message[0..payload_len], op.payload[0..payload_len]);
    }
    state.last_error = "none";
}

fn newR4slOp() ?r4p_contract.R4slOp {
    if (!r4p.hasActiveR4p("net.serial_link")) return null;
    return .{};
}

fn dispatchR4sl(opcode: u32, op: *r4p_contract.R4slOp) bool {
    var buffer = protocol_api.ProtocolBuffer{
        .data = @ptrCast(op),
        .len = @sizeOf(r4p_contract.R4slOp),
        .capacity = @sizeOf(r4p_contract.R4slOp),
        .flags = 0,
        .reserved = 0,
    };
    const result = r4p.dispatch("net.serial_link", opcode, &buffer, &buffer);
    if (result == -5 or (result == -4 and op.result == 0)) {
        state.r4p_dispatch_failures += 1;
        return false;
    }
    return true;
}

fn fail(reason: []const u8) bool {
    state.last_error = reason;
    return false;
}

fn copyFixed(out: []u8, value: []const u8) void {
    @memset(out, 0);
    if (out.len == 0) return;
    const len = @min(out.len - 1, value.len);
    if (len != 0) @memcpy(out[0..len], value[0..len]);
}
