// Other owner waves retain log formatting and R4SYS file access. These exact
// private C cuts retain no protocol table, execution slot or registry state.
const log_event = @import("log_event.zig");
const r4sys = @import("../program/r4sys.zig");
export fn r4os_zig_protocol_log(severity: u8, slot: u32, text: [*:0]const u8) callconv(.c) void {
    log_event.protocol(switch (severity) {
        0 => .info,
        1 => .warn,
        2 => .err,
        else => unreachable,
    }, slot, text);
}
export fn r4os_zig_protocol_file_read(path: [*:0]const u8, output: [*]u8, len: u32) callconv(.c) i32 {
    return r4sys.fileRead(path, output, len);
}
