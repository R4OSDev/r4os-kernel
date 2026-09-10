const bootlog = @import("bootlog.zig");
const std = @import("std");

pub const Severity = enum {
    info,
    warn,
    err,
};

pub fn driver(severity: Severity, owner: u32, text: [*:0]const u8) void {
    // Parallel driver callbacks must publish one bounded log record, without
    // interleaving headers, owner IDs and individual payload bytes.
    var length: usize = 0;
    while (length < 512 and text[length] != 0) : (length += 1) {}
    const suffix: []const u8 = if (length == 512 and text[length] != 0) " [truncated]" else "";
    var buffer: [640]u8 = undefined;
    const line = std.fmt.bufPrint(&buffer, "[LOG1] source=Driver severity={s} owner={d} text={s}{s}\r\n", .{ severityName(severity), owner, text[0..length], suffix }) catch unreachable;
    bootlog.puts(line);
}

pub fn protocol(severity: Severity, slot: u32, text: [*:0]const u8) void {
    writeHeader("Protocol", severity);
    bootlog.puts(" slot=");
    bootlog.putDec(slot);
    writeText(text);
}

fn writeHeader(source: []const u8, severity: Severity) void {
    bootlog.puts("[LOG1] source=");
    bootlog.puts(source);
    bootlog.puts(" severity=");
    bootlog.puts(severityName(severity));
}

fn writeText(text: [*:0]const u8) void {
    bootlog.puts(" text=");
    var i: usize = 0;
    while (i < 512 and text[i] != 0) : (i += 1) {
        bootlog.putc(text[i]);
    }
    if (i == 512 and text[i] != 0) bootlog.puts(" [truncated]");
    bootlog.puts("\r\n");
}

fn severityName(severity: Severity) []const u8 {
    return switch (severity) {
        .info => "Info",
        .warn => "Warn",
        .err => "Error",
    };
}
