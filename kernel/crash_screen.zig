// Private C facade. Native fatal renderer uses the original report and sinks.
const crash = @import("crash.zig");
const fb = @import("../display/framebuffer.zig");
const log = @import("log.zig");
pub const RenderResult = enum(u8) { framebuffer = 0, serial_only = 1 };
extern fn r4os_rust_crash_screen_render(*const crash.CrashReport) u8;
extern fn r4os_rust_crash_screen_frame(*const fb.Framebuffer, *const crash.CrashReport) bool;
extern fn r4os_rust_crash_screen_serial(*const crash.CrashReport) void;
pub fn render(report: *const crash.CrashReport) RenderResult {
    return @enumFromInt(r4os_rust_crash_screen_render(report));
}
pub fn renderToFramebuffer(frame: *fb.Framebuffer, report: *const crash.CrashReport) bool {
    return r4os_rust_crash_screen_frame(frame, report);
}
pub fn serialMirror(report: *const crash.CrashReport) void {
    r4os_rust_crash_screen_serial(report);
}
export fn r4os_zig_crash_screen_sync() callconv(.c) void {
    log.serialEnterSync();
}
export fn r4os_zig_crash_screen_putc(ch: u8) callconv(.c) void {
    log.serialPutcRaw(ch);
}
