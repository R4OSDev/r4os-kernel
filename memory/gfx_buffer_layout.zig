// Hardware-independent buffer layout validation. Addresses are deliberately
// absent: CPU mappings, DMA page lists and GPU virtual mappings have distinct
// leases and are never inferred from each other.
const std = @import("std");

pub const max_planes = 4;
pub const linear_modifier: u64 = 0;
pub const Format = enum(u32) {
    bytes = 0,
    xrgb8888 = 0x34325258,
    argb8888 = 0x34325241,
    r8 = 0x20203852,
    nv12 = 0x3231564e,
    p010 = 0x30313050,
};
pub const Usage = struct {
    pub const cpu_read: u32 = 1;
    pub const cpu_write: u32 = 2;
    pub const transfer_source: u32 = 4;
    pub const transfer_target: u32 = 8;
    pub const render: u32 = 16;
    pub const scanout: u32 = 32;
    pub const valid: u32 = 63;
};
pub const Location = enum(u32) { system = 0, device_local = 1 };
pub const Binding = struct {
    adapter: u32 = 0,
    driver_owner: u32 = 0,
    device_generation: u64 = 0,

    pub fn valid(self: Binding) bool {
        return (self.adapter == 0 and self.driver_owner == 0 and self.device_generation == 0) or
            (self.adapter != 0 and self.driver_owner != 0 and self.device_generation != 0);
    }

    pub fn portable(self: Binding) bool {
        return self.adapter == 0 and self.driver_owner == 0 and self.device_generation == 0;
    }
};
pub const Plane = struct { offset: u64 = 0, pitch: u64 = 0 };
pub const Descriptor = struct {
    bytes: u64,
    alignment: u64 = 4096,
    modifier: u64 = linear_modifier,
    width: u32 = 0,
    height: u32 = 0,
    format: Format = .bytes,
    plane_count: u32 = 0,
    planes: [max_planes]Plane = .{Plane{}} ** max_planes,
    usage: u32 = Usage.cpu_read | Usage.cpu_write,
    location: Location = .system,
    binding: Binding = .{},
};
pub const Error = error{ Invalid, Overflow, Unsupported };
pub const Range = struct { offset: u64, bytes: u64 };
pub const Layout = struct { allocation_bytes: u64, planes: [max_planes]Range, plane_count: u32 };

pub fn spanFits(total: u64, offset: u64, bytes: u64) bool {
    return bytes != 0 and offset < total and bytes <= total - offset;
}

pub fn validate(descriptor: Descriptor) Error!Layout {
    if (descriptor.bytes == 0 or !std.math.isPowerOfTwo(descriptor.alignment) or
        descriptor.alignment > (@as(u64, 1) << 30) or descriptor.usage == 0 or
        (descriptor.usage & ~Usage.valid) != 0 or !descriptor.binding.valid()) return error.Invalid;
    if (descriptor.modifier != linear_modifier) return error.Unsupported;
    if (descriptor.location == .device_local and descriptor.binding.portable()) return error.Invalid;
    const allocation_alignment = @max(@as(u64, 4096), descriptor.alignment);
    const padded = std.math.add(u64, descriptor.bytes, allocation_alignment - 1) catch return error.Overflow;
    var result = Layout{ .allocation_bytes = padded & ~(allocation_alignment - 1), .planes = .{Range{ .offset = 0, .bytes = 0 }} ** max_planes, .plane_count = descriptor.plane_count };
    if (descriptor.format == .bytes) {
        if (descriptor.width != 0 or descriptor.height != 0 or descriptor.plane_count != 0 or
            (descriptor.usage & (Usage.render | Usage.scanout)) != 0) return error.Invalid;
    } else {
        const multi = descriptor.format == .nv12 or descriptor.format == .p010;
        const count: u32 = if (multi) 2 else 1;
        if (descriptor.width == 0 or descriptor.height == 0 or descriptor.plane_count != count) return error.Invalid;
        for (0..count) |index| {
            const plane = descriptor.planes[index];
            const sample_bytes: u64 = switch (descriptor.format) {
                .xrgb8888, .argb8888 => 4,
                .p010 => 2,
                else => 1,
            };
            const columns: u64 = if (multi and index == 1) ((@as(u64, descriptor.width) + 1) / 2) * 2 else descriptor.width;
            const rows: u64 = if (multi and index == 1) (@as(u64, descriptor.height) + 1) / 2 else descriptor.height;
            const row_bytes = std.math.mul(u64, columns, sample_bytes) catch return error.Overflow;
            if (plane.pitch < row_bytes or plane.pitch % sample_bytes != 0 or plane.offset % sample_bytes != 0) return error.Invalid;
            // Complete padded rows are owned, including the final row's tail.
            const bytes = std.math.mul(u64, plane.pitch, rows) catch return error.Overflow;
            if (!spanFits(descriptor.bytes, plane.offset, bytes)) return error.Invalid;
            for (result.planes[0..index]) |previous| {
                if (plane.offset < previous.offset + previous.bytes and previous.offset < plane.offset + bytes) return error.Invalid;
            }
            result.planes[index] = .{ .offset = plane.offset, .bytes = bytes };
        }
    }
    for (descriptor.planes[descriptor.plane_count..]) |plane| {
        if (plane.offset != 0 or plane.pitch != 0) return error.Invalid;
    }
    return result;
}

test "buffer layout keeps 64-bit sizes and rejects wrapping pitch, maps and padding" {
    const t = std.testing;
    const huge: u64 = @as(u64, 8) * 1024 * 1024 * 1024;
    try t.expectEqual(huge, (try validate(.{ .bytes = huge })).allocation_bytes);
    try t.expectError(error.Overflow, validate(.{ .bytes = std.math.maxInt(u64) }));
    try t.expect(!spanFits(huge, huge - 1, 2));
    try t.expect(!spanFits(huge, std.math.maxInt(u64), 4));
    var image = Descriptor{ .bytes = 64, .width = 3, .height = 4, .format = .xrgb8888, .plane_count = 1 };
    image.planes[0].pitch = 16;
    try t.expectEqual(@as(u64, 64), (try validate(image)).planes[0].bytes);
    image.bytes = 60;
    try t.expectError(error.Invalid, validate(image));
    image.bytes = huge;
    image.planes[0].pitch = std.math.maxInt(u64) - 3;
    try t.expectError(error.Overflow, validate(image));
    image.planes[0].pitch = 8;
    try t.expectError(error.Invalid, validate(image));
}

test "multi-plane layout validates chroma extents, overlap, bindings and unsupported modifiers" {
    const t = std.testing;
    var image = Descriptor{ .bytes = 48, .width = 5, .height = 3, .format = .nv12, .plane_count = 2 };
    image.planes[0] = .{ .offset = 0, .pitch = 8 };
    image.planes[1] = .{ .offset = 24, .pitch = 8 };
    try t.expectEqual(@as(u64, 16), (try validate(image)).planes[1].bytes);
    image.planes[1].offset = 16;
    try t.expectError(error.Invalid, validate(image));
    image.planes[1].offset = 24;
    image.planes[1].pitch = 5;
    try t.expectError(error.Invalid, validate(image));
    image.planes[1].pitch = 8;
    image.location = .device_local;
    try t.expectError(error.Invalid, validate(image));
    image.binding = .{ .adapter = 4, .driver_owner = 9, .device_generation = 7 };
    _ = try validate(image);
    image.modifier = 1;
    try t.expectError(error.Unsupported, validate(image));
}
