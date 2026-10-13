const config = @import("config");
const fatal = @import("fatal.zig");
const version = @import("version.zig");
const crash = @import("crash.zig");
const owner_locks = @import("../memory/owner_locks.zig");

const BuildInfo = extern struct {
    magic: u32,
    size: u32,
    flags: u64,
    smp_fail_ap_index: u32,
    kernel_major: u32,
    kernel_minor: u32,
    kernel_patch: u32,
};
comptime {
    if (@sizeOf(BuildInfo) != 32 or @alignOf(BuildInfo) != 8 or @offsetOf(BuildInfo, "flags") != 8 or @offsetOf(BuildInfo, "smp_fail_ap_index") != 16) @compileError("Private Rust build bridge layout");
}
extern fn r4os_rust_build_info(*BuildInfo) callconv(.c) void;
extern fn r4os_rust_boot_probe() callconv(.c) void;

pub fn init() void {
    var actual: BuildInfo = undefined;
    r4os_rust_build_info(&actual);
    if (actual.magic != 0x52344231 or actual.size != @sizeOf(BuildInfo) or actual.flags != config.rust_build_flags or
        actual.smp_fail_ap_index != config.smp_fail_ap_index or actual.kernel_major != version.current.major or
        actual.kernel_minor != version.current.minor or actual.kernel_patch != version.current.patch)
    {
        fatal.kernelFatal(.entry, "Rust/Zig build configuration mismatch");
    }
    r4os_rust_boot_probe();
}

// Bounded output helpers used by migrated Rust boot owners. General logging
// still owns the active sink graph until its instrumentation migration wave.
const log = @import("log.zig");
export fn r4os_zig_kernel_write(text: [*]const u8, len: usize) callconv(.c) void {
    log.puts(text[0..len]);
}
export fn r4os_zig_kernel_dec(value: u64) callconv(.c) void {
    log.putDec(value);
}
export fn r4os_zig_kernel_hex(value: u64, width: u8) callconv(.c) void {
    log.putHex(value, width);
}
export fn r4os_zig_kernel_putc(byte: u8) callconv(.c) void {
    log.putc(byte);
}

const BootLockToken = extern struct { flags: u64, cpu_index: u32, previous_rank: u8, outermost: bool };
comptime {
    if (@sizeOf(BootLockToken) != 16 or @offsetOf(BootLockToken, "cpu_index") != 8)
        @compileError("Rust boot owner token layout differs");
}
fn bootLock(class: u8) *owner_locks.Lock {
    return if (class == 0) &owner_locks.boot_log else &owner_locks.serial_output;
}
export fn r4os_zig_boot_lock(class: u8, output: *BootLockToken) callconv(.c) void {
    const token = bootLock(class).acquire();
    output.* = .{ .flags = token.flags, .cpu_index = token.cpu_index, .previous_rank = token.previous_rank, .outermost = token.outermost };
}
export fn r4os_zig_boot_unlock(class: u8, input: *const BootLockToken) callconv(.c) void {
    bootLock(class).release(.{ .flags = input.flags, .cpu_index = input.cpu_index, .previous_rank = input.previous_rank, .outermost = input.outermost });
}
