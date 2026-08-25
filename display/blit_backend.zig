// Owner-bound external display blit backend contract.
//
// The kernel owns the display target and the fallback. An R4D may provide one
// synchronous, self-tested copy implementation, but never owns or retains the
// framebuffer, source buffer, region list or a fence after the callback.

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

pub const Snapshot = struct {
    active: bool = false,
    name: [NAME_BYTES]u8 = .{0} ** NAME_BYTES,
    flags: u32 = 0,
    max_regions: u32 = 0,
};

pub const InvokeResult = struct {
    attempted: bool = false,
    result: i32 = -1,
    name: [NAME_BYTES]u8 = .{0} ** NAME_BYTES,
};

const Registration = struct {
    active: bool = false,
    admissions_open: bool = false,
    busy: u32 = 0,
    owner: u32 = 0,
    name: [NAME_BYTES]u8 = .{0} ** NAME_BYTES,
    descriptor: ?*const Descriptor = null,
};

var registration: Registration = .{};

pub fn register(owner: u32, name: []const u8, descriptor: *const Descriptor) i32 {
    if (owner == 0 or name.len == 0 or name.len >= NAME_BYTES) return -1;
    if (descriptor.version != VERSION or descriptor.size < @sizeOf(Descriptor)) return -2;
    if ((descriptor.flags & REQUIRED_FLAGS) != REQUIRED_FLAGS or descriptor.max_regions == 0) return -2;
    if (registration.active) return -3;

    registration = .{
        .active = true,
        .admissions_open = true,
        .owner = owner,
        .descriptor = descriptor,
    };
    @memcpy(registration.name[0..name.len], name);
    return 0;
}

pub fn unregister(owner: u32, name: []const u8) i32 {
    if (!registration.active or registration.owner != owner or !nameEqual(registration.name[0..], name)) return -1;
    if (registration.busy != 0) return -2;
    registration = .{};
    return 0;
}

pub fn snapshot() Snapshot {
    const descriptor = registration.descriptor orelse return .{};
    if (!registration.active or !registration.admissions_open) return .{};
    return .{
        .active = true,
        .name = registration.name,
        .flags = descriptor.flags,
        .max_regions = descriptor.max_regions,
    };
}

pub fn invoke(job: *const Job) InvokeResult {
    const descriptor = registration.descriptor orelse return .{};
    if (!registration.active or !registration.admissions_open or job.region_count > descriptor.max_regions) return .{};
    registration.busy +%= 1;
    defer registration.busy -%= 1;
    return .{
        .attempted = true,
        .result = descriptor.present(descriptor.context, job),
        .name = registration.name,
    };
}

pub fn prepareOwnerCleanup(owner: u32) bool {
    if (!registration.active or registration.owner != owner) return true;
    if (registration.busy != 0) return false;
    registration.admissions_open = false;
    return true;
}

pub fn cancelOwnerCleanup(owner: u32) void {
    if (registration.active and registration.owner == owner) registration.admissions_open = true;
}

pub fn cleanupOwner(owner: u32) u32 {
    if (!registration.active or registration.owner != owner) return 0;
    if (registration.busy != 0) return 0;
    registration = .{};
    return 1;
}

fn nameEqual(stored: []const u8, expected: []const u8) bool {
    var len: usize = 0;
    while (len < stored.len and stored[len] != 0) : (len += 1) {}
    if (len != expected.len) return false;
    return len == 0 or equal(stored[0..len], expected);
}

fn equal(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var i: usize = 0;
    while (i < a.len) : (i += 1) if (a[i] != b[i]) return false;
    return true;
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
