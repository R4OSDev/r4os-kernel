// Compatibility facade. Rust owns phase, pending failure and terminal
// decisions; clock instrumentation and framebuffer primitives retain their
// existing owners until their respective migration waves.
const boot_status = @import("boot_status.zig");
const bootscreen = @import("bootscreen.zig");
const crash = @import("crash.zig");
const crash_screen = @import("crash_screen.zig");
const timer = @import("timer.zig");
extern fn r4os_rust_fatal_init() callconv(.c) void;
extern fn r4os_rust_fatal_phase(crash.BootPhase) callconv(.c) void;
extern fn r4os_rust_fatal_record(crash.BootPhase, [*]const u8, usize) callconv(.c) void;
extern fn r4os_rust_fatal_pending(crash.BootPhase, [*]const u8, usize) callconv(.c) noreturn;
extern fn r4os_rust_kernel_fatal(crash.BootPhase, [*]const u8, usize) callconv(.c) noreturn;
extern fn r4os_rust_zig_panic([*]const u8, usize) callconv(.c) noreturn;
extern fn r4os_rust_panic_fatal([*]const u8, usize) callconv(.c) noreturn;
pub fn init() void {
    r4os_rust_fatal_init();
}
pub fn setBootPhase(phase: crash.BootPhase) void {
    r4os_rust_fatal_phase(phase);
}
pub fn fail(phase: crash.BootPhase, message: []const u8) bool {
    recordFailure(phase, message);
    return false;
}
pub fn recordFailure(phase: crash.BootPhase, message: []const u8) void {
    r4os_rust_fatal_record(phase, message.ptr, message.len);
}
pub fn haltPendingOrMessage(phase: crash.BootPhase, message: []const u8) noreturn {
    r4os_rust_fatal_pending(phase, message.ptr, message.len);
}
pub fn kernelFatal(phase: crash.BootPhase, message: []const u8) noreturn {
    r4os_rust_kernel_fatal(phase, message.ptr, message.len);
}
pub fn zigPanic(message: []const u8) noreturn {
    r4os_rust_zig_panic(message.ptr, message.len);
}
pub fn rustPanic(message: []const u8) noreturn {
    r4os_rust_panic_fatal(message.ptr, message.len);
}


export fn r4os_zig_fatal_record_display(message: [*]const u8, len: usize) callconv(.c) void {
    bootscreen.setError(message[0..len]);
    boot_status.disableForCrash();
}
export fn r4os_zig_fatal_display(message: [*]const u8, len: usize) callconv(.c) void {
    bootscreen.setError(message[0..len]);
}
export fn r4os_zig_fatal_ticks() callconv(.c) u64 {
    return timer.tickCount();
}
export fn r4os_zig_crash_output(report: *const crash.CrashReport, reentrant: bool) callconv(.c) void {
    boot_status.disableForCrash();
    if (reentrant) crash_screen.serialMirror(report) else {
        _ = crash_screen.render(report);
    }
}
