//! Fixed projection of native complete color payload prefixes.
const std = @import("std");
const Color = @import("r4os_kernel_contract").GfxOutputColorState;
extern fn r4os_rust_display_wire(u32, u32, ?*const anyopaque, ?*anyopaque, u32, ?*u32) bool;
pub fn capacity(pointer: *const Color) ?u32 {
    var out: u32 = undefined; return if (r4os_rust_display_wire(2, 4, pointer, null, 0, &out)) out else null;
}
pub fn read(pointer: *const Color) ?Color {
    var out: Color = undefined; return if (r4os_rust_display_wire(1, 4, pointer, &out, 0, null)) out else null;
}
pub fn write(pointer: *Color, count: u32, value: Color) void { std.debug.assert(r4os_rust_display_wire(3, 4, &value, pointer, count, null)); }
