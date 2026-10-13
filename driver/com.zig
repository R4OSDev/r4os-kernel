// Compatibility facade: the raw UART implementation and single COM1 TX
// ring live in Rust/boot/serial.rs; all original admission/timeout rules remain.
pub const COM1: u16 = 0x3F8;
pub const COM2: u16 = 0x2F8;
extern fn r4os_rust_serial_init(u16) callconv(.c) void;
extern fn r4os_rust_serial_present(u16) callconv(.c) bool;
extern fn r4os_rust_serial_putc(u16, u8) callconv(.c) void;
extern fn r4os_rust_serial_available(u16, [*]const u8, usize) callconv(.c) usize;
extern fn r4os_rust_serial_getc(u16) callconv(.c) i32;
extern fn r4os_rust_serial_has_data(u16) callconv(.c) bool;
extern fn r4os_rust_serial_write([*]const u8, usize) callconv(.c) void;
extern fn r4os_rust_serial_drain() callconv(.c) void;
extern fn r4os_rust_serial_flush() callconv(.c) void;
extern fn r4os_rust_serial_sync() callconv(.c) void;
extern fn r4os_rust_serial_stats(*LogTxStats) callconv(.c) void;
const TX_RING_SIZE: usize = 8192;
pub const Port = struct {
    base: u16,
    pub fn init(self: Port) void {
        r4os_rust_serial_init(self.base);
    }
    pub fn isPresent(self: Port) bool {
        return r4os_rust_serial_present(self.base);
    }
    pub fn putc(self: Port, byte: u8) void {
        r4os_rust_serial_putc(self.base, byte);
    }
    pub fn writeAvailable(self: Port, bytes: []const u8) usize {
        return r4os_rust_serial_available(self.base, bytes.ptr, bytes.len);
    }
    pub fn hasData(self: Port) bool {
        return r4os_rust_serial_has_data(self.base);
    }
    pub fn getc(self: Port) ?u8 {
        const value = r4os_rust_serial_getc(self.base);
        return if (value < 0) null else @intCast(value);
    }
    pub fn puts(self: Port, bytes: []const u8) void {
        for (bytes) |byte| self.putc(byte);
    }
};
pub var com1: Port = .{ .base = COM1 };
pub var com2: Port = .{ .base = COM2 };
pub fn init() void {
    com1.init();
}
pub fn puts(bytes: []const u8) void {
    logWrite(bytes);
}
pub const LogTxStats = extern struct {
    ring_size: u64 = TX_RING_SIZE,
    pending: u64 = 0,
    ring_bytes: u64 = 0,
    sync_bytes: u64 = 0,
    full_stalls: u64 = 0,
    dropped_bytes: u64 = 0,
    sync_failures: u64 = 0,
    sync_mode: bool = false,
    sync_uart_available: bool = true,
    write_calls: u64 = 0,
    bulk_calls: u64 = 0,
    lock_acquisitions: u64 = 0,
    uart_status_reads: u64 = 0,
    drain_calls: u64 = 0,
    max_span: u64 = 0,
};

comptime {
    if (@sizeOf(LogTxStats) != 112 or @offsetOf(LogTxStats, "write_calls") != 64) @compileError("Rust serial stats differ");
}
pub fn logTxStats() LogTxStats {
    var result: LogTxStats = undefined;
    r4os_rust_serial_stats(&result);
    return result;
}
pub fn logPutc(byte: u8) void {
    const data = [_]u8{byte};
    logWrite(&data);
}
pub fn logWrite(bytes: []const u8) void {
    r4os_rust_serial_write(bytes.ptr, bytes.len);
}
pub fn logDrain() void {
    r4os_rust_serial_drain();
}
pub fn logFlushSync() void {
    r4os_rust_serial_flush();
}
pub fn logEnterSyncMode() void {
    r4os_rust_serial_sync();
}
