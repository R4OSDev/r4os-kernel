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

extern fn r4os_rust_boot_redirect() callconv(.c) void;
extern fn r4os_rust_boot_release() callconv(.c) void;
extern fn r4os_rust_boot_unroute() callconv(.c) void;
extern fn r4os_rust_boot_redirect_active() callconv(.c) bool;
export fn r4os_zig_boot_foundation_summary() callconv(.c) void {
    printFoundationSummary();
}
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
    r4os_rust_boot_redirect();
}
pub fn releaseForUserSession() void {
    r4os_rust_boot_release();
}
pub fn disableForCrash() void {
    r4os_rust_boot_unroute();
}
pub fn isBootLogRedirectActive() bool {
    return r4os_rust_boot_redirect_active();
}
pub fn statusLine(text: []const u8) void {
    log.puts(text);
}
