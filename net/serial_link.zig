// Native SerialLink owner; wire protocols and public status remain stable.
const serial = @import("../driver/com.zig");
const runtime = @import("serial_link_runtime.zig");
pub const MAGIC: [4]u8 = .{ 'R', '4', 'S', 'L' };
pub const VERSION: u8 = 1;
pub const TYPE_DIAG: u8 = 1;
pub const TYPE_MESSAGE: u8 = 2;
pub const HEADER_SIZE: usize = 10;
pub const MAX_PAYLOAD: usize = runtime.max_payload;
pub const MAX_FRAME: usize = HEADER_SIZE + MAX_PAYLOAD;
pub const Snapshot = extern struct {
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

pub const Message = runtime.Message;

extern fn r4os_rust_serial_link_init() void;
extern fn r4os_rust_serial_link_poll() void;
extern fn r4os_rust_serial_link_send_frame(u8, [*]const u8, usize) bool;
extern fn r4os_rust_serial_link_send_message([*]const u8, usize) bool;
extern fn r4os_rust_serial_link_snapshot(*Snapshot) void;
extern fn r4os_rust_serial_link_message(*Message) bool;
pub fn init() void {
    r4os_rust_serial_link_init();
}
pub fn poll() void {
    r4os_rust_serial_link_poll();
}
pub fn sendFrame(frame_type: u8, payload: []const u8) bool {
    return r4os_rust_serial_link_send_frame(frame_type, payload.ptr, payload.len);
}
pub fn sendMessage(payload: []const u8) bool {
    return r4os_rust_serial_link_send_message(payload.ptr, payload.len);
}
pub fn snapshot(out: *Snapshot) void {
    r4os_rust_serial_link_snapshot(out);
}
pub fn takeMessage(out: *Message) bool {
    return r4os_rust_serial_link_message(out);
}

comptime {
    if (@sizeOf(Snapshot) != 720 or @alignOf(Snapshot) != 8 or @offsetOf(Snapshot, "present") != 0 or @offsetOf(Snapshot, "initialized") != 1 or @offsetOf(Snapshot, "port_base") != 2 or @offsetOf(Snapshot, "loopback_tests") != 8 or @offsetOf(Snapshot, "host_tests") != 16 or @offsetOf(Snapshot, "message_tx") != 24 or @offsetOf(Snapshot, "message_rx") != 32 or @offsetOf(Snapshot, "tx_skipped") != 40 or @offsetOf(Snapshot, "tx_frames") != 48 or @offsetOf(Snapshot, "tx_bytes") != 56 or @offsetOf(Snapshot, "rx_frames") != 64 or @offsetOf(Snapshot, "rx_bytes") != 72 or @offsetOf(Snapshot, "polls") != 80 or @offsetOf(Snapshot, "bad_magic") != 88 or @offsetOf(Snapshot, "bad_version") != 96 or @offsetOf(Snapshot, "bad_length") != 104 or @offsetOf(Snapshot, "checksum_errors") != 112 or @offsetOf(Snapshot, "overflows") != 120 or @offsetOf(Snapshot, "timeouts") != 128 or @offsetOf(Snapshot, "r4p_build") != 136 or @offsetOf(Snapshot, "r4p_parse") != 144 or @offsetOf(Snapshot, "r4p_self") != 152 or @offsetOf(Snapshot, "r4p_dispatch_failures") != 160 or @offsetOf(Snapshot, "last_type") != 168 or @offsetOf(Snapshot, "last_payload_len") != 170 or @offsetOf(Snapshot, "last_message_len") != 172 or @offsetOf(Snapshot, "last_payload") != 174 or @offsetOf(Snapshot, "last_message") != 430 or @offsetOf(Snapshot, "last_error") != 686) @compileError("Native network Snapshot layout");
}
