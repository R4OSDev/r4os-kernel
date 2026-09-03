// Early COM1 debug hook for kernel startup.

const config = @import("config");
const log = @import("log.zig");

pub fn init() void {
    if (comptime config.enable_com1_debug) {
        const com = @import("../driver/com.zig");
        com.init();
        log.setSerialSink(com1Putc);
        log.setSerialBulkSink(com1Write);
        // COM1-TX-Ring: Crash-/Poweroff-Pfade schalten auf synchron, bleiben
        // bei einem nicht antwortenden UART aber strikt begrenzt.
        log.setSerialFlushHook(com1Flush);
        log.setSerialSyncHook(com1EnterSync);
    } else {
        log.setSerialSink(null);
        log.setSerialBulkSink(null);
        log.setSerialFlushHook(null);
        log.setSerialSyncHook(null);
    }
}

fn com1Putc(ch: u8) callconv(.c) void {
    const com = @import("../driver/com.zig");
    com.logPutc(ch);
}

fn com1Write(ptr: [*]const u8, len: usize) callconv(.c) void {
    const com = @import("../driver/com.zig");
    com.logWrite(ptr[0..len]);
}

fn com1Flush() callconv(.c) void {
    const com = @import("../driver/com.zig");
    com.logFlushSync();
}

fn com1EnterSync() callconv(.c) void {
    const com = @import("../driver/com.zig");
    com.logEnterSyncMode();
}
