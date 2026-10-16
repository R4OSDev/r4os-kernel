//! Released statistics prefixes; no allocation, budget or residency policy.
const a = @import("r4os_kernel_contract");
extern fn r4os_rust_gfx_stats_capacity(*const a.GfxBufferStats) u32;
extern fn r4os_rust_gfx_stats_write(*a.GfxBufferStats, u32, *const a.GfxBufferStats) void;
pub fn capacity(pointer: *const a.GfxBufferStats) ?u32 {
    const count = r4os_rust_gfx_stats_capacity(pointer);
    return if (count == 0) null else count;
}
pub fn write(pointer: *a.GfxBufferStats, count: u32, value: a.GfxBufferStats) void {
    r4os_rust_gfx_stats_write(pointer, count, &value);
}
comptime {
    if (@sizeOf(a.GfxBufferStats) != 136 or @alignOf(a.GfxBufferStats) != 8 or @offsetOf(a.GfxBufferStats, "system_bytes") != 56) @compileError("Native graphics statistics prefix layout");
}
