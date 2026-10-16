//! Fixed projection of native complete released graphics payload prefixes.
const std = @import("std");
const abi = @import("r4os_kernel_contract");
extern fn r4os_rust_display_wire(u32, u32, ?*const anyopaque, ?*anyopaque, u32, ?*u32) bool;
fn kind(comptime T: type) u32 {
    return if (T == abi.GfxBackendRegistration) 0 else if (T == abi.GfxBackendInfo) 1 else if (T == abi.GfxSubmission) 2
        else if (T == abi.GfxDriverJob) 3 else @compileError("Unsupported graphics queue payload");
}
pub fn prefix(comptime T: type, available: u32) ?u32 {
    var out: u32 = undefined; return if (r4os_rust_display_wire(0, kind(T), null, null, available, &out)) out else null;
}
pub fn read(comptime T: type, input: *const T) ?T {
    var out: T = undefined; return if (r4os_rust_display_wire(1, kind(T), input, &out, 0, null)) out else null;
}
pub fn capacity(comptime T: type, output: *T) ?u32 {
    var out: u32 = undefined; return if (r4os_rust_display_wire(2, kind(T), output, null, 0, &out)) out else null;
}
pub fn write(comptime T: type, output: *T, size: u32, value: T) void {
    std.debug.assert(r4os_rust_display_wire(3, kind(T), &value, output, size, null));
}
