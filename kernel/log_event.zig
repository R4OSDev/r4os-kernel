const bootlog = @import("bootlog.zig");

pub const Severity = enum {
    info,
    warn,
    err,
};

pub fn driver(severity: Severity, owner: u32, text: [*:0]const u8) void {
    writeHeader("Driver", severity);
    bootlog.puts(" owner=");
    bootlog.putDec(owner);
    writeText(text);
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
