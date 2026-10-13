// Rust selects the optional COM1 hooks; the general log facade stays with
// its own owner until 0.83.26. No ConsoleSink or native slice crosses C.
const log = @import("log.zig");
extern fn r4os_rust_com_debug_boot() callconv(.c) void;
pub fn init() void {
    r4os_rust_com_debug_boot();
}
export fn r4os_zig_kernel_serial_sinks(putc: ?log.SerialSink, write: ?log.SerialBulkSink, flush: ?log.SerialControlHook, sync: ?log.SerialControlHook) callconv(.c) void {
    log.setSerialSink(putc);
    log.setSerialBulkSink(write);
    log.setSerialFlushHook(flush);
    log.setSerialSyncHook(sync);
}
