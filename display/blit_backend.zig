// Owner-bound external display blit backend contract.
//
// The kernel owns the display target and the fallback. An R4D may provide one
// synchronous, self-tested copy implementation, but never owns or retains the
// framebuffer, source buffer, region list or a fence after the callback.

comptime { if (@import("builtin").is_test) _ = @import("rust_owner_test"); }

pub const VERSION: u32 = 1;
pub const FLAG_XRGB32: u32 = 1 << 0;
pub const FLAG_SYNCHRONOUS: u32 = 1 << 1;
pub const FLAG_SELF_TESTED: u32 = 1 << 2;
pub const FLAG_CPU_FAST_COPY: u32 = 1 << 3;
pub const REQUIRED_FLAGS: u32 = FLAG_XRGB32 | FLAG_SYNCHRONOUS | FLAG_SELF_TESTED;
pub const NAME_BYTES: usize = 24;

pub const Region = extern struct {
    dst_x: u32 = 0,
    dst_y: u32 = 0,
    src_x: u32 = 0,
    src_y: u32 = 0,
    w: u32 = 0,
    h: u32 = 0,
};

pub const Job = extern struct {
    version: u32 = VERSION,
    size: u32 = @sizeOf(Job),
    format: u32 = 1,
    flags: u32 = 0,
    target_address: u64 = 0,
    target_width: u32 = 0,
    target_height: u32 = 0,
    target_pitch_pixels: u32 = 0,
    source_pixel_count: u32 = 0,
    source_address: u64 = 0,
    source_stride_pixels: u32 = 0,
    region_count: u32 = 0,
    regions_address: u64 = 0,
};

pub const PresentFn = *const fn (usize, *const Job) callconv(.c) i32;

pub const Descriptor = extern struct {
    version: u32 = VERSION,
    size: u32 = @sizeOf(Descriptor),
    flags: u32 = 0,
    max_regions: u32 = 0,
    context: usize = 0,
    present: PresentFn,
};

pub const Snapshot = extern struct {
    active: bool = false,
    name: [NAME_BYTES]u8 = .{0} ** NAME_BYTES,
    flags: u32 = 0,
    max_regions: u32 = 0,
};

pub const InvokeResult = extern struct {
    attempted: bool = false,
    result: i32 = -1,
    name: [NAME_BYTES]u8 = .{0} ** NAME_BYTES,
};

extern fn r4os_rust_display_blit_register(u32, [*]const u8, usize, *const Descriptor) i32;
extern fn r4os_rust_display_blit_unregister(u32, [*]const u8, usize) i32;
extern fn r4os_rust_display_blit_snapshot(*Snapshot) void;
extern fn r4os_rust_display_blit_invoke(*const Job, *InvokeResult) void;
extern fn r4os_rust_display_blit_cleanup(u32, u32) u32;
pub fn register(owner: u32, name: []const u8, descriptor: *const Descriptor) i32 { return r4os_rust_display_blit_register(owner, name.ptr, name.len, descriptor); }
pub fn unregister(owner: u32, name: []const u8) i32 { return r4os_rust_display_blit_unregister(owner, name.ptr, name.len); }
pub fn snapshot() Snapshot { var out: Snapshot = undefined; r4os_rust_display_blit_snapshot(&out); return out; }
pub fn invoke(job: *const Job) InvokeResult { var out: InvokeResult = undefined; r4os_rust_display_blit_invoke(job, &out); return out; }
pub fn prepareOwnerCleanup(owner: u32) bool { return r4os_rust_display_blit_cleanup(0, owner) != 0; }
pub fn cancelOwnerCleanup(owner: u32) void { _ = r4os_rust_display_blit_cleanup(1, owner); }
pub fn cleanupOwner(owner: u32) u32 { return r4os_rust_display_blit_cleanup(2, owner); }
comptime {
    if (@sizeOf(Region) != 24 or @sizeOf(Job) != 64 or @sizeOf(Descriptor) != 32 or @alignOf(Descriptor) != 8 or @offsetOf(Job, "regions_address") != 56 or @offsetOf(Descriptor, "present") != 24) @compileError("Display blit descriptor C layout");
    if (@sizeOf(Snapshot) != 36 or @offsetOf(Snapshot, "flags") != 28 or @sizeOf(InvokeResult) != 32 or @offsetOf(InvokeResult, "name") != 8) @compileError("Display blit result C layout");
}

test "owner cleanup closes callback admission and removes the backend" {
    const Tester = struct {
        fn present(_: usize, _: *const Job) callconv(.c) i32 {
            return 0;
        }
    };
    const descriptor = Descriptor{
        .flags = REQUIRED_FLAGS | FLAG_CPU_FAST_COPY,
        .max_regions = 8,
        .present = Tester.present,
    };
    try @import("std").testing.expectEqual(@as(i32, 0), register(7, "TEST", &descriptor));
    try @import("std").testing.expect(snapshot().active);
    try @import("std").testing.expect(prepareOwnerCleanup(7));
    try @import("std").testing.expect(!snapshot().active);
    cancelOwnerCleanup(7);
    try @import("std").testing.expect(snapshot().active);
    try @import("std").testing.expectEqual(@as(u32, 1), cleanupOwner(7));
    try @import("std").testing.expect(!snapshot().active);
}
