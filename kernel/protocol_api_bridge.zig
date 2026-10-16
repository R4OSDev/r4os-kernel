// Fixed private file-read call; logging binds directly to the native owner.
const r4sys = @import("../program/r4sys.zig");
export fn r4os_zig_protocol_file_read(path: [*:0]const u8, output: [*]u8, len: u32) callconv(.c) i32 {
    return r4sys.fileRead(path, output, len);
}
