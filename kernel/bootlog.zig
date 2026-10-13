// Thin compatibility view of the Rust-owned bounded bootlog.
pub const BUFFER_SIZE: usize = 65536;
pub const FLAG_WRAPPED: u32 = 1;
pub const Info = extern struct { length: usize, flags: u32, total_written: u64, dropped_bytes: u64 };
comptime {
    if (@sizeOf(Info) != 32 or @offsetOf(Info, "total_written") != 16) @compileError("Rust bootlog info differs");
}
extern fn r4os_rust_bootlog_info(*Info) callconv(.c) void;
extern fn r4os_rust_bootlog_write([*]const u8, usize) callconv(.c) void;
extern fn r4os_rust_bootlog_dec(u64) callconv(.c) void;
extern fn r4os_rust_bootlog_hex(u64, usize) callconv(.c) void;
extern fn r4os_rust_bootlog_read(usize, [*]u8, usize) callconv(.c) usize;
pub fn info() Info {
    var result: Info = undefined;
    r4os_rust_bootlog_info(&result);
    return result;
}
pub fn puts(text: []const u8) void {
    r4os_rust_bootlog_write(text.ptr, text.len);
}
pub fn putc(byte: u8) void {
    const text = [_]u8{byte};
    puts(&text);
}
pub fn putDec(value: u64) void {
    r4os_rust_bootlog_dec(value);
}
pub fn putHex(value: u64, width: usize) void {
    r4os_rust_bootlog_hex(value, width);
}
pub fn snapshot(out: []u8) usize {
    return read(0, out);
}
pub fn capacity() usize {
    return BUFFER_SIZE;
}
pub fn length() usize {
    return info().length;
}
pub fn flags() u32 {
    return info().flags;
}
pub fn totalWritten() u64 {
    return info().total_written;
}
pub fn droppedBytes() u64 {
    return info().dropped_bytes;
}
pub fn read(offset: usize, out: []u8) usize {
    return r4os_rust_bootlog_read(offset, out.ptr, out.len);
}
