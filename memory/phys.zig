// Compatibility facade for the one Rust-owned PMM bitmap and counters.
const k = @import("../kernel/log.zig");
pub const FRAME_SIZE: u64 = 4096;
pub const Stats = extern struct {
    total_frames: u64 = 0,
    free_frames: u64 = 0,
    used_frames: u64 = 0,
    bitmap_bytes: u64 = 0,
    bitmap_base: u64 = 0,
    double_free_errors: u64 = 0,
    bad_free_errors: u64 = 0,
    extent_allocations: u64 = 0,
    extent_allocated_frames: u64 = 0,
    extent_max_frames: u64 = 0,
    extent_frees: u64 = 0,
    extent_freed_frames: u64 = 0,
};

pub const FrameExtent = extern struct {
    base: u64,
    count: u64,
};

extern fn r4os_rust_phys_init() callconv(.c) bool;
extern fn r4os_rust_phys_alloc(*u64) callconv(.c) bool;
extern fn r4os_rust_phys_extent(u64, *FrameExtent) callconv(.c) bool;
extern fn r4os_rust_phys_contiguous(u64, u64, bool, *u64) callconv(.c) bool;
extern fn r4os_rust_phys_free_contiguous(u64, u64) callconv(.c) void;
extern fn r4os_rust_phys_free_extent(u64, u64) callconv(.c) void;
extern fn r4os_rust_phys_free(u64) callconv(.c) void;
extern fn r4os_rust_phys_stats(*Stats) callconv(.c) void;
extern fn r4os_rust_phys_to_virt(u64) callconv(.c) u64;
extern fn r4os_rust_virt_to_phys(u64) callconv(.c) u64;
comptime {
    if (@sizeOf(Stats) != 96 or @sizeOf(FrameExtent) != 16) @compileError("Rust PMM C layout differs");
}
pub fn init() bool {
    return r4os_rust_phys_init();
}
pub fn allocFrame() ?u64 {
    var result: u64 = undefined;
    return if (r4os_rust_phys_alloc(&result)) result else null;
}
pub fn allocFrameExtent(max_count: u64) ?FrameExtent {
    var result: FrameExtent = undefined;
    return if (r4os_rust_phys_extent(max_count, &result)) result else null;
}
pub fn allocContiguousFrames(count: u64) ?u64 {
    var result: u64 = undefined;
    return if (r4os_rust_phys_contiguous(count, ~@as(u64, 0), true, &result)) result else null;
}
pub fn allocContiguousFramesBelow(count: u64, max_phys_addr: u64) ?u64 {
    var result: u64 = undefined;
    return if (r4os_rust_phys_contiguous(count, max_phys_addr, false, &result)) result else null;
}
pub fn freeContiguousFrames(addr: u64, count: u64) void {
    r4os_rust_phys_free_contiguous(addr, count);
}
pub fn freeFrameExtent(extent: FrameExtent) void {
    r4os_rust_phys_free_extent(extent.base, extent.count);
}
pub fn freeFrame(addr: u64) void {
    r4os_rust_phys_free(addr);
}
pub fn stats() Stats {
    var result: Stats = undefined;
    r4os_rust_phys_stats(&result);
    return result;
}
pub fn physToVirt(addr: u64) u64 {
    return r4os_rust_phys_to_virt(addr);
}
pub fn virtToPhys(addr: u64) u64 {
    return r4os_rust_virt_to_phys(addr);
}

// Logging remains at its not-yet-migrated owner; no PMM borrow spans it.
export fn r4os_zig_pmm_fault(kind: u8, addr: u64) callconv(.c) void {
    k.puts("PMM FAULT ");
    k.puts(switch (kind) {
        1 => "BAD-FREE-ALIGN",
        2 => "BAD-FREE-RANGE",
        3 => "DOUBLE-FREE",
        else => "UNKNOWN",
    });
    k.puts(" frame=0x");
    k.putHex(addr, 16);
    k.puts("\r\n");
}

pub fn dumpStats() void {
    const s = stats();
    k.puts("  Physical frames: total=");
    k.putDec(s.total_frames);
    k.puts(" free=");
    k.putDec(s.free_frames);
    k.puts(" used=");
    k.putDec(s.used_frames);
    k.puts("\r\n");
    k.puts("  Physical extents: alloc=");
    k.putDec(s.extent_allocations);
    k.puts(" frames=");
    k.putDec(s.extent_allocated_frames);
    k.puts(" max=");
    k.putDec(s.extent_max_frames);
    k.puts(" free=");
    k.putDec(s.extent_frees);
    k.puts(" returned=");
    k.putDec(s.extent_freed_frames);
    k.puts("\r\n");
    k.puts("  PMM bitmap: base=0x");
    k.putHex(s.bitmap_base, 16);
    k.puts(" bytes=");
    k.putDec(s.bitmap_bytes);
    k.puts("\r\n");
}
