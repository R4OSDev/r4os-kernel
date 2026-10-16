comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
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
    xrgb2101010 = 0x30335258,
    argb2101010 = 0x30335241,
    abgr16161616f = 0x48344241,
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
pub const Binding = extern struct {
    adapter: u32 = 0,
    driver_owner: u32 = 0,
    device_generation: u64 = 0,

    pub fn valid(self: Binding) bool {
        return r4os_rust_gfx_layout_binding(&self, false);
    }

    pub fn portable(self: Binding) bool {
        return r4os_rust_gfx_layout_binding(&self, true);
    }
};
pub const Plane = extern struct { offset: u64 = 0, pitch: u64 = 0 };
pub const Descriptor = extern struct {
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
pub const Range = extern struct { offset: u64, bytes: u64 };
pub const Layout = extern struct { allocation_bytes: u64, planes: [max_planes]Range, plane_count: u32 };

extern fn r4os_rust_gfx_layout_span(u64, u64, u64) bool;
extern fn r4os_rust_gfx_layout_binding(*const Binding, bool) bool;
extern fn r4os_rust_gfx_layout_validate(*const Descriptor, bool, *Layout) i32;
pub fn spanFits(total: u64, offset: u64, bytes: u64) bool {
    return r4os_rust_gfx_layout_span(total, offset, bytes);
}
pub fn validate(descriptor: Descriptor) Error!Layout {
    return nativeValidate(descriptor, false);
}
pub fn validateDriverOwned(descriptor: Descriptor) Error!Layout {
    return nativeValidate(descriptor, true);
}
fn nativeValidate(descriptor: Descriptor, owned: bool) Error!Layout {
    var out: Layout = undefined;
    return switch (r4os_rust_gfx_layout_validate(&descriptor, owned, &out)) {
        0 => out,
        1 => error.Invalid,
        2 => error.Overflow,
        3 => error.Unsupported,
        else => unreachable,
    };
}

test "buffer layout keeps 64-bit sizes and rejects wrapping pitch, maps and padding" {
    const t = std.testing;
    try t.expectError(error.Invalid, validate(.{ .bytes = 4096, .alignment = 0 }));
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
    image = .{ .bytes = 96, .width = 3, .height = 4, .format = .abgr16161616f, .plane_count = 1 };
    image.planes[0].pitch = 24;
    try t.expectEqual(@as(u64, 96), (try validate(image)).planes[0].bytes);
    image.planes[0].pitch = 20;
    try t.expectError(error.Invalid, validate(image));
    image.format = .xrgb2101010;
    image.planes[0].pitch = 12;
    image.bytes = 48;
    try t.expectEqual(@as(u64, 48), (try validate(image)).planes[0].bytes);
    image.format = .argb2101010;
    image.planes[0].offset = 2;
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
    try t.expectError(error.Unsupported, validateDriverOwned(image)); // CPU usage is never admitted.
    image.usage = Usage.transfer_source | Usage.transfer_target;
    image.bytes = 128;
    image.planes[1].offset = 64;
    const native_layout = try validateDriverOwned(image);
    try t.expectEqual(@as(u64, 64), native_layout.planes[0].bytes);
    try t.expectEqual(@as(u64, 64), native_layout.planes[1].bytes);
    image.planes[1].offset = 20;
    try t.expectError(error.Invalid, validateDriverOwned(image));
    image.planes[1].offset = 136;
    try t.expectError(error.Invalid, validateDriverOwned(image));
}

comptime {
    if (@sizeOf(Binding) != 16 or @alignOf(Binding) != 8) @compileError("Native GFX Binding C layout");
    if (@offsetOf(Binding, "adapter") != 0) @compileError("Native GFX Binding.adapter C offset");
    if (@offsetOf(Binding, "driver_owner") != 4) @compileError("Native GFX Binding.driver_owner C offset");
    if (@offsetOf(Binding, "device_generation") != 8) @compileError("Native GFX Binding.device_generation C offset");
    if (@sizeOf(Plane) != 16 or @alignOf(Plane) != 8) @compileError("Native GFX Plane C layout");
    if (@offsetOf(Plane, "offset") != 0) @compileError("Native GFX Plane.offset C offset");
    if (@offsetOf(Plane, "pitch") != 8) @compileError("Native GFX Plane.pitch C offset");
    if (@sizeOf(Descriptor) != 128 or @alignOf(Descriptor) != 8) @compileError("Native GFX Descriptor C layout");
    if (@offsetOf(Descriptor, "bytes") != 0) @compileError("Native GFX Descriptor.bytes C offset");
    if (@offsetOf(Descriptor, "alignment") != 8) @compileError("Native GFX Descriptor.alignment C offset");
    if (@offsetOf(Descriptor, "modifier") != 16) @compileError("Native GFX Descriptor.modifier C offset");
    if (@offsetOf(Descriptor, "width") != 24) @compileError("Native GFX Descriptor.width C offset");
    if (@offsetOf(Descriptor, "height") != 28) @compileError("Native GFX Descriptor.height C offset");
    if (@offsetOf(Descriptor, "format") != 32) @compileError("Native GFX Descriptor.format C offset");
    if (@offsetOf(Descriptor, "plane_count") != 36) @compileError("Native GFX Descriptor.plane_count C offset");
    if (@offsetOf(Descriptor, "planes") != 40) @compileError("Native GFX Descriptor.planes C offset");
    if (@offsetOf(Descriptor, "usage") != 104) @compileError("Native GFX Descriptor.usage C offset");
    if (@offsetOf(Descriptor, "location") != 108) @compileError("Native GFX Descriptor.location C offset");
    if (@offsetOf(Descriptor, "binding") != 112) @compileError("Native GFX Descriptor.binding C offset");
    if (@sizeOf(Range) != 16 or @alignOf(Range) != 8) @compileError("Native GFX Range C layout");
    if (@offsetOf(Range, "offset") != 0) @compileError("Native GFX Range.offset C offset");
    if (@offsetOf(Range, "bytes") != 8) @compileError("Native GFX Range.bytes C offset");
    if (@sizeOf(Layout) != 80 or @alignOf(Layout) != 8) @compileError("Native GFX Layout C layout");
    if (@offsetOf(Layout, "allocation_bytes") != 0) @compileError("Native GFX Layout.allocation_bytes C offset");
    if (@offsetOf(Layout, "planes") != 8) @compileError("Native GFX Layout.planes C offset");
    if (@offsetOf(Layout, "plane_count") != 72) @compileError("Native GFX Layout.plane_count C offset");
}
