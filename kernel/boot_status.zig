// Early boot status routing.
//
// Normal kernel output is kept off the framebuffer. It still goes to COM1
// through log.zig first and is mirrored into bootlog for LOGSVC/diagnostics.
// The routing remains active after the user-session handoff so asynchronous
// kernel workers cannot paint over the desktop. Only the crash path releases
// the ConsoleSink deliberately.

const log = @import("log.zig");
const bootlog = @import("bootlog.zig");
const diag_screen = @import("diag_screen.zig");
const timer = @import("timer.zig");
const input_boot = @import("input_boot.zig");
const boot_display = @import("../display/boot_display.zig");

var boot_redirect_active: bool = false;

pub fn printFoundationSummary() void {
    log.puts("  Booted via Limine ");
    log.puts("[OK]\r\n");

    log.puts("  CPU tables / PIC ");
    log.puts("[OK]\r\n");

    log.puts("  Timer core ");
    log.puts(timer.backendName());
    log.puts(" ");
    log.putDec(timer.frequency());
    log.puts(" Hz [OK]\r\n");

    log.puts("  Keyboard IRQ ");
    log.puts(if (input_boot.isKeyboardInitialized()) "[OK]\r\n" else "[not ready]\r\n");

    log.puts("  Mouse IRQ ");
    log.puts(input_boot.mouseStatusLine());
}

pub fn beginBootLogRedirect() void {
    boot_redirect_active = true;
    log.setOutputHook(bootOutputHook);
}

pub fn releaseForUserSession() void {
    boot_redirect_active = false;
    boot_display.invalidateConsoleBackingForExternalDisplay();
    diag_screen.resetForExternalDisplay();
    // Keep the non-visible sink installed for runtime kernel messages. This is
    // intentionally the non-intercepting hook: log.zig writes COM1 first, the
    // hook mirrors into bootlog and suppresses only the framebuffer sink.
    log.setOutputHook(bootOutputHook);
}

pub fn disableForCrash() void {
    boot_redirect_active = false;
    log.setOutputHook(null);
}

pub fn isBootLogRedirectActive() bool {
    return boot_redirect_active;
}

pub fn statusLine(text: []const u8) void {
    log.puts(text);
}

fn bootOutputHook(ch: u8) callconv(.c) bool {
    bootlog.putc(ch);
    return true;
}
