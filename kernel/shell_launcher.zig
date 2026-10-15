//! External-shell startup facade; native Rust owns ordering and fallback decisions.
const boot_config = @import("boot_config.zig");
const boot_perf = @import("boot_perf.zig");
const bootlog = @import("bootlog.zig");
const bootscreen = @import("bootscreen.zig");
const drive = @import("../fs/drive.zig");
const fatal = @import("fatal.zig");
const k = @import("log.zig");
const loader_perf = @import("loader_perf.zig");
const r4x = @import("../program/r4x.zig");
const usb_msc = @import("../driver/usb/msc.zig");

const Bytes = extern struct { data: [*]const u8, len: usize };
const MscStatus = extern struct {
    present: bool,
    block_registered: bool,
    read_failures: u64,
    transport_failures: u64,
    reason: Bytes,
};
const Config = extern struct { path: Bytes, args: Bytes };
fn bytes(value: []const u8) Bytes { return .{ .data = value.ptr, .len = value.len }; }
fn slice(value: Bytes) []const u8 { return value.data[0..value.len]; }
extern fn r4os_rust_program_shell_start(*const boot_config.Config, u64) noreturn;
pub fn start(config: *const boot_config.Config, usable_bytes: u64) noreturn {
    r4os_rust_program_shell_start(config, usable_bytes);
}
export fn r4os_zig_program_shell_init(usable_bytes: u64) callconv(.c) void { r4x.initializeRuntime(usable_bytes); }
export fn r4os_zig_program_shell_config(config: *const boot_config.Config, output: *Config) callconv(.c) void {
    output.* = .{ .path = bytes(boot_config.shellPath(config)), .args = bytes(boot_config.shellArgs(config)) };
}
export fn r4os_zig_program_shell_text(channel: u8, value: Bytes) callconv(.c) void {
    switch (channel) {
        0 => k.puts(slice(value)),
        1 => bootlog.puts(slice(value)),
        2 => bootscreen.setStatus(slice(value)),
        3 => bootscreen.setDetail(slice(value)),
        4 => bootscreen.clearDetail(),
        else => unreachable,
    }
}
export fn r4os_zig_program_shell_perf(operation: u8, a: u64, b: u64) callconv(.c) u64 {
    switch (operation) {
        0 => return loader_perf.beginServiceBoot(),
        1 => loader_perf.finishServiceBoot(a, @intCast(b)),
        2 => boot_perf.beginShellAttempt(switch (a) { 0 => .configured, 1 => .terminal_fallback, 2 => .recovery_fallback, else => unreachable }),
        3 => boot_perf.noteShellLaunched(r4x.activeShellInstanceId()),
        4 => boot_perf.noteShellLaunchFailure(),
        5 => boot_perf.failNoShell(),
        else => unreachable,
    }
    return 0;
}
export fn r4os_zig_program_shell_run(boot: *drive.Drive, working: *drive.Drive, path: Bytes, args: Bytes, shell: bool, host: u32) callconv(.c) u8 {
    const result = if (shell) r4x.runShellPathWithHost(boot, slice(path), slice(args), working, @enumFromInt(host)) else r4x.runPath(boot, slice(path), slice(args), working);
    return @intFromEnum(result);
}
export fn r4os_zig_program_shell_msc(output: *MscStatus) callconv(.c) void {
    const status = usb_msc.status();
    output.* = .{ .present = status.present, .block_registered = status.block_registered, .read_failures = status.read_failures, .transport_failures = status.transport_failures, .reason = bytes(status.reason) };
}
export fn r4os_zig_program_shell_fatal(reason: Bytes) callconv(.c) noreturn { fatal.kernelFatal(.shell, slice(reason)); }
comptime {
    if (@sizeOf(Bytes) != 16 or @sizeOf(Config) != 32 or @sizeOf(MscStatus) != 40 or @offsetOf(MscStatus, "reason") != 24)
        @compileError("Native shell startup C projection layout drift");
}
