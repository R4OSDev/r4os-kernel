// Central kernel logging: optionally writes to serial (COM1) and to an early
// ConsoleSink.
//
// Before `setConsoleSink()`, output only goes to a configured serial sink.

pub const SerialSink = *const fn (u8) callconv(.c) void;
pub const OutputHook = *const fn (u8) callconv(.c) bool;

pub const ConsoleSink = struct {
    context: *anyopaque,
    putc: *const fn (*anyopaque, u8) void,
    puts: *const fn (*anyopaque, []const u8) void,
    clear: *const fn (*anyopaque) void,
    clearFramed: *const fn (*anyopaque, u32, u32) void,
    setMargins: *const fn (*anyopaque, u32, u32, u32, u32) void,
    setColors: *const fn (*anyopaque, u32, u32) void,
    setFontScale: *const fn (*anyopaque, u32) void,
    setCursor: *const fn (*anyopaque, u32, u32) void,
    cols: *const fn (*anyopaque) u32,
    rows: *const fn (*anyopaque) u32,
};

pub const SerialControlHook = *const fn () callconv(.c) void;

var serial_sink: ?SerialSink = null;
var console_sink: ?ConsoleSink = null;
var output_hook: ?OutputHook = null;
var output_hook_intercepts_serial: bool = false;
var serial_flush_hook: ?SerialControlHook = null;
var serial_sync_hook: ?SerialControlHook = null;

pub fn setSerialSink(sink: ?SerialSink) void {
    serial_sink = sink;
}

// 0.56.15: COM1-TX-Ring-Steuerung ohne direkten com.zig-Import.
pub fn setSerialFlushHook(hook: ?SerialControlHook) void {
    serial_flush_hook = hook;
}

pub fn setSerialSyncHook(hook: ?SerialControlHook) void {
    serial_sync_hook = hook;
}

// Poweroff-/Abschluss-Pfad: gepufferte Serial-Ausgabe synchron leeren; ein
// nicht fortschreitender UART bleibt durch den COM-Treiber begrenzt.
pub fn serialFlush() void {
    if (serial_flush_hook) |hook| hook();
}

// Crash-Pfad: ab jetzt synchron auf COM1 schreiben, mit latched Timeout bei
// einem nicht antwortenden UART.
pub fn serialEnterSync() void {
    if (serial_sync_hook) |hook| hook();
}

pub fn setConsoleSink(sink: ?ConsoleSink) void {
    console_sink = sink;
}

pub fn serialPutcRaw(ch: u8) void {
    if (serial_sink) |sink| sink(ch);
}

pub fn consolePutcRaw(ch: u8) void {
    if (console_sink) |sink| sink.putc(sink.context, ch);
}

pub fn setOutputHook(hook: ?OutputHook) void {
    output_hook = hook;
    output_hook_intercepts_serial = false;
}

pub fn setOutputHookIntercept(hook: ?OutputHook) void {
    output_hook = hook;
    output_hook_intercepts_serial = hook != null;
}

pub fn clearConsole() void {
    if (console_sink) |sink| sink.clear(sink.context);
}

pub fn setConsoleMargins(left: u32, top: u32, right: u32, bottom: u32) void {
    if (console_sink) |sink| sink.setMargins(sink.context, left, top, right, bottom);
}

pub fn setConsoleColors(fg: u32, bg: u32) void {
    if (console_sink) |sink| sink.setColors(sink.context, fg, bg);
}

pub fn setConsoleFontScale(scale: u32) void {
    if (console_sink) |sink| sink.setFontScale(sink.context, scale);
}

pub fn setConsoleCursor(x: u32, y: u32) void {
    if (console_sink) |sink| sink.setCursor(sink.context, x, y);
}

pub fn clearConsoleFramed(border: u32, inner: u32) void {
    if (console_sink) |sink| sink.clearFramed(sink.context, border, inner);
}

pub fn consoleCols() u32 {
    if (console_sink) |sink| return sink.cols(sink.context);
    return 0;
}

pub fn consoleRows() u32 {
    if (console_sink) |sink| return sink.rows(sink.context);
    return 0;
}

pub fn putc(ch: u8) void {
    if (output_hook_intercepts_serial) {
        if (output_hook) |hook| {
            if (hook(ch)) return;
        }
    }
    if (serial_sink) |sink| {
        sink(ch);
    }
    if (!output_hook_intercepts_serial) {
        if (output_hook) |hook| {
            if (hook(ch)) return;
        }
    }
    if (console_sink) |sink| sink.putc(sink.context, ch);
}

pub fn puts(s: []const u8) void {
    if (output_hook != null) {
        for (s) |ch| putc(ch);
        return;
    }
    if (serial_sink) |sink| {
        for (s) |ch| sink(ch);
    }
    if (console_sink) |sink| sink.puts(sink.context, s);
}

pub fn putHex(value: u64, digits: u8) void {
    const hex = "0123456789ABCDEF";
    var i: u8 = digits;
    while (i > 0) {
        i -= 1;
        const shift: u6 = @intCast(@as(u32, i) * 4);
        const nibble: u4 = @truncate(value >> shift);
        putc(hex[nibble]);
    }
}

pub fn putDec(value: u64) void {
    if (value == 0) {
        putc('0');
        return;
    }
    var buf: [20]u8 = undefined;
    var n = value;
    var i: usize = buf.len;
    while (n > 0) {
        i -= 1;
        buf[i] = @intCast('0' + (n % 10));
        n /= 10;
    }
    puts(buf[i..]);
}

pub fn newline() void {
    puts("\r\n");
}
