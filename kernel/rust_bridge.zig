const config = @import("config");
const fatal = @import("fatal.zig");
const version = @import("version.zig");
const crash = @import("crash.zig");

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

// The existing fatal owner remains active until its Rust migration wave.
// Only bounded borrowed bytes cross this private C ABI; no native language
// reference, allocation, registry or unwind state is duplicated.
export fn r4os_zig_rust_fatal(message: [*]const u8, len: usize) callconv(.c) noreturn {
    const count: usize = @min(len, crash.MAX_MESSAGE_BYTES);
    fatal.rustPanic(message[0..count]);
}
