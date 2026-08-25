const fb = @import("framebuffer.zig");
const blit_backend = @import("blit_backend.zig");
const cpu = @import("../platform/cpu.zig");
const paging = @import("../memory/paging.zig");
const timer = @import("../kernel/timer.zig");

pub const DeviceKind = enum(u8) {
    none = 0,
    bootfb = 1,
};

pub const DeviceFlags = struct {
    pub const visible: u32 = 1 << 0;
    pub const fixed_mode: u32 = 1 << 1;
    pub const cpu_present: u32 = 1 << 2;
    pub const rgb32: u32 = 1 << 3;
    pub const fill: u32 = 1 << 4;
    pub const rect: u32 = 1 << 5;
    pub const packed32: u32 = 1 << 6;
    pub const xrgb32: u32 = 1 << 7;
};

pub const Mode = struct {
    width: u32 = 0,
    height: u32 = 0,
    pitch: u32 = 0,
    bpp: u16 = 0,
    memory_model: u8 = 0,
    red_mask_size: u8 = 0,
    red_mask_shift: u8 = 0,
    green_mask_size: u8 = 0,
    green_mask_shift: u8 = 0,
    blue_mask_size: u8 = 0,
    blue_mask_shift: u8 = 0,
};

pub const Rect = struct {
    x: u32 = 0,
    y: u32 = 0,
    w: u32 = 0,
    h: u32 = 0,
};

pub const MAX_PRESENT_REGIONS: usize = 8;
pub const PRESENT_FORMAT_XRGB32: u32 = 1;

pub const PresentRegion = blit_backend.Region;

pub const PresentOutcome = struct {
    success: bool = false,
    accelerated: bool = false,
    fallback: bool = false,
    source_generation: u64 = 0,
    present_generation: u64 = 0,
    fence: u64 = 0,
    completed_fence: u64 = 0,
    region_count: u32 = 0,
    pixel_count: u32 = 0,
    fallback_regions: u32 = 0,
    backend_error: i32 = 0,
    present_tick: u64 = 0,
    elapsed_ticks: u64 = 0,
    backend_name: [blit_backend.NAME_BYTES]u8 = .{0} ** blit_backend.NAME_BYTES,
};

pub const PresentCapabilities = struct {
    flags: u32 = 0,
    formats: u32 = PRESENT_FORMAT_XRGB32,
    max_regions: u32 = MAX_PRESENT_REGIONS,
    backend_kind: u32 = 1,
    backend_name: [blit_backend.NAME_BYTES]u8 = .{0} ** blit_backend.NAME_BYTES,
    fallback_name: [blit_backend.NAME_BYTES]u8 = .{0} ** blit_backend.NAME_BYTES,
};

pub const MappingKind = enum(u8) {
    none = 0,
    bootloader_framebuffer = 1,
};

pub const CachePolicy = enum(u8) {
    unknown = 0,
    bootloader_default = 1,
    pat_write_combining = 2,
    write_combining_unsupported = 3,
    write_combining_failed = 4,
};

pub const Mapping = struct {
    kind: MappingKind = .none,
    cache_policy: CachePolicy = .unknown,
    virt_base: u64 = 0,
    byte_len: u64 = 0,
    volatile_cpu_writes: bool = false,
};

pub const PresentReason = enum(u8) {
    none = 0,
    fill = 1,
    rect = 2,
    packed32_present = 3,
    xrgb32_present = 4,
};

pub const DeviceOps = struct {
    fill: ?*const fn (device: *Device, rgb: u32) bool = null,
    rect: ?*const fn (device: *Device, x: i32, y: i32, w: u32, h: u32, rgb: u32) bool = null,
    present_packed32_rect: ?*const fn (device: *Device, x0: u64, y0: u64, w: u64, h: u64, src: []const u8, src_stride_pixels: u64) bool = null,
    present_xrgb32_rect: ?*const fn (device: *Device, x0: u64, y0: u64, w: u64, h: u64, src: []const u8, src_stride_pixels: u64) bool = null,
    put_packed32: ?*const fn (device: *Device, x: u64, y: u64, color32: u32) bool = null,
    put_xrgb32: ?*const fn (device: *Device, x: u64, y: u64, rgb: u32) bool = null,
};

pub const DisplayTarget = struct {
    name: []const u8 = "none",
    kind: DeviceKind = .none,
    flags: u32 = 0,
    mode: Mode = .{},
    mapping: Mapping = .{},
    // Low-level backend handle only. Normal display consumers must use the
    // DisplayManager boundary: mode, mapping, stats and present operations.
    framebuffer: ?*fb.Framebuffer = null,
};

pub const Device = struct {
    name: []const u8 = "none",
    kind: DeviceKind = .none,
    flags: u32 = 0,
    mode: Mode = .{},
    mapping: Mapping = .{},
    framebuffer: ?*fb.Framebuffer = null,
    ops: *const DeviceOps = &empty_ops,
    present_count: u64 = 0,
    present_pixels_total: u64 = 0,
    present_bytes_total: u64 = 0,
    last_present_pixels: u64 = 0,
    last_present_bytes: u64 = 0,
    last_present_rect: Rect = .{},
    last_present_reason: PresentReason = .none,
    last_present_converted: bool = false,
    full_present_count: u64 = 0,
    partial_present_count: u64 = 0,
    fill_present_count: u64 = 0,
    rect_present_count: u64 = 0,
    packed32_present_count: u64 = 0,
    xrgb32_present_count: u64 = 0,
    conversion_present_count: u64 = 0,
    present_total_ticks: u64 = 0,
    present_max_ticks: u64 = 0,
    present_last_ticks: u64 = 0,
    present_slow_count: u64 = 0,
};

pub const Stats = struct {
    registered: bool = false,
    name: []const u8 = "none",
    kind: DeviceKind = .none,
    flags: u32 = 0,
    mode: Mode = .{},
    mapping: Mapping = .{},
    present_count: u64 = 0,
    present_pixels_total: u64 = 0,
    present_bytes_total: u64 = 0,
    last_present_pixels: u64 = 0,
    last_present_bytes: u64 = 0,
    last_present_rect: Rect = .{},
    last_present_reason: PresentReason = .none,
    last_present_converted: bool = false,
    full_present_count: u64 = 0,
    partial_present_count: u64 = 0,
    fill_present_count: u64 = 0,
    rect_present_count: u64 = 0,
    packed32_present_count: u64 = 0,
    xrgb32_present_count: u64 = 0,
    conversion_present_count: u64 = 0,
    present_total_ticks: u64 = 0,
    present_max_ticks: u64 = 0,
    present_last_ticks: u64 = 0,
    present_slow_count: u64 = 0,
};

const empty_ops: DeviceOps = .{};
const bootfb_ops: DeviceOps = .{
    .fill = bootfbFill,
    .rect = bootfbRect,
    .present_packed32_rect = bootfbPresentPacked32Rect,
    .present_xrgb32_rect = bootfbPresentXrgb32Rect,
    .put_packed32 = bootfbPutPacked32,
    .put_xrgb32 = bootfbPutXrgb32,
};

var bootfb_device: Device = .{ .ops = &bootfb_ops };
var primary_device: ?*Device = null;
var present_generation: u64 = 0;
var completed_fence: u64 = 0;

pub fn registerBootBackend(target: DisplayTarget) void {
    bootfb_device = .{
        .name = target.name,
        .kind = target.kind,
        .flags = target.flags,
        .mode = target.mode,
        .mapping = target.mapping,
        .framebuffer = target.framebuffer,
        .ops = &bootfb_ops,
    };
    primary_device = &bootfb_device;
    present_generation = 0;
    completed_fence = 0;
}

pub fn activeBackendRegistered() bool {
    return primary_device != null;
}

pub fn activeBackendName() []const u8 {
    const device = primary_device orelse return "none";
    return device.name;
}

pub fn activeBackendKind() DeviceKind {
    const device = primary_device orelse return .none;
    return device.kind;
}

pub fn activeMode() ?Mode {
    const device = primary_device orelse return null;
    return device.mode;
}

pub fn activeMapping() ?Mapping {
    const device = primary_device orelse return null;
    return device.mapping;
}

pub fn activeFramebufferForLegacy() ?*fb.Framebuffer {
    const device = primary_device orelse return null;
    return device.framebuffer;
}

pub fn enableFramebufferWriteCombining() bool {
    const device = primary_device orelse return false;
    if (!cpu.writeCombiningBasisAvailable()) {
        device.mapping.cache_policy = .write_combining_unsupported;
        return false;
    }
    if (device.mapping.virt_base == 0 or device.mapping.byte_len == 0) {
        device.mapping.cache_policy = .write_combining_failed;
        return false;
    }
    if (!paging.setWriteCombiningRange(device.mapping.virt_base, device.mapping.byte_len)) {
        device.mapping.cache_policy = .write_combining_failed;
        return false;
    }
    device.mapping.cache_policy = .pat_write_combining;
    return true;
}

pub fn framebuffer() ?*fb.Framebuffer {
    return activeFramebufferForLegacy();
}

pub fn stats() Stats {
    const device = primary_device orelse return .{};
    return .{
        .registered = true,
        .name = device.name,
        .kind = device.kind,
        .flags = device.flags,
        .mode = device.mode,
        .mapping = device.mapping,
        .present_count = device.present_count,
        .present_pixels_total = device.present_pixels_total,
        .present_bytes_total = device.present_bytes_total,
        .last_present_pixels = device.last_present_pixels,
        .last_present_bytes = device.last_present_bytes,
        .last_present_rect = device.last_present_rect,
        .last_present_reason = device.last_present_reason,
        .last_present_converted = device.last_present_converted,
        .full_present_count = device.full_present_count,
        .partial_present_count = device.partial_present_count,
        .fill_present_count = device.fill_present_count,
        .rect_present_count = device.rect_present_count,
        .packed32_present_count = device.packed32_present_count,
        .xrgb32_present_count = device.xrgb32_present_count,
        .conversion_present_count = device.conversion_present_count,
        .present_total_ticks = device.present_total_ticks,
        .present_max_ticks = device.present_max_ticks,
        .present_last_ticks = device.present_last_ticks,
        .present_slow_count = device.present_slow_count,
    };
}

pub fn fill(rgb: u32) bool {
    const device = primary_device orelse return false;
    const op = device.ops.fill orelse return false;
    const start = timer.tickCount();
    const ok = op(device, rgb);
    if (ok) recordPresentTiming(device, start);
    return ok;
}

pub fn rect(x: i32, y: i32, w: u32, h: u32, rgb: u32) bool {
    const device = primary_device orelse return false;
    const op = device.ops.rect orelse return false;
    const start = timer.tickCount();
    const ok = op(device, x, y, w, h, rgb);
    if (ok) recordPresentTiming(device, start);
    return ok;
}

pub fn putPacked32(x: u64, y: u64, color32: u32) bool {
    const device = primary_device orelse return false;
    const op = device.ops.put_packed32 orelse return false;
    return op(device, x, y, color32);
}

pub fn putXrgb32(x: u64, y: u64, rgb: u32) bool {
    const device = primary_device orelse return false;
    const op = device.ops.put_xrgb32 orelse return false;
    return op(device, x, y, rgb);
}

pub fn presentPacked32Rect(x0: u64, y0: u64, w: u64, h: u64, src: []const u8, src_stride_pixels: u64) bool {
    const device = primary_device orelse return false;
    const op = device.ops.present_packed32_rect orelse return false;
    const start = timer.tickCount();
    const ok = op(device, x0, y0, w, h, src, src_stride_pixels);
    if (ok) recordPresentTiming(device, start);
    return ok;
}

pub fn presentXrgb32Rect(x0: u64, y0: u64, w: u64, h: u64, src: []const u8, src_stride_pixels: u64) bool {
    if (x0 > ~@as(u32, 0) or y0 > ~@as(u32, 0) or w > ~@as(u32, 0) or h > ~@as(u32, 0) or
        src_stride_pixels > ~@as(u32, 0) or (src.len & 3) != 0 or (@intFromPtr(src.ptr) & 3) != 0)
    {
        return false;
    }
    const pixels: [*]const u32 = @ptrCast(@alignCast(src.ptr));
    const region = PresentRegion{
        .dst_x = @intCast(x0),
        .dst_y = @intCast(y0),
        .src_x = 0,
        .src_y = 0,
        .w = @intCast(w),
        .h = @intCast(h),
    };
    return presentXrgb32Regions(
        pixels,
        @intCast(src.len / @sizeOf(u32)),
        @intCast(src_stride_pixels),
        (&region)[0..1],
        0,
        0,
        false,
    ).success;
}

/// The sole productive XRGB32 present/statistics path. All regions are
/// validated before the first visible write. The external backend is one
/// synchronous optimization attempt; every absence, incompatibility or
/// callback error falls back to the boot framebuffer copy for the complete
/// generation.
pub fn presentXrgb32Regions(
    source: [*]const u32,
    source_pixel_count: u32,
    source_stride_pixels: u32,
    regions: []const PresentRegion,
    source_generation: u64,
    input_tick: u64,
    input_tick_valid: bool,
) PresentOutcome {
    var outcome = PresentOutcome{ .source_generation = source_generation };
    const device = primary_device orelse return outcome;
    const f = device.framebuffer orelse return outcome;
    if (!fb.supportsRgb32(f) or source_pixel_count == 0 or source_stride_pixels == 0 or
        regions.len == 0 or regions.len > MAX_PRESENT_REGIONS)
    {
        return outcome;
    }

    var pixels_total: u64 = 0;
    var bounds = Rect{};
    for (regions, 0..) |region, index| {
        if (!validPresentRegion(region, device.mode, source_pixel_count, source_stride_pixels)) return outcome;
        const region_pixels = @as(u64, region.w) * region.h;
        if (pixels_total > ~@as(u32, 0) - region_pixels) return outcome;
        pixels_total += region_pixels;
        if (index == 0) {
            bounds = .{ .x = region.dst_x, .y = region.dst_y, .w = region.w, .h = region.h };
        } else {
            bounds = mergeRect(bounds, .{ .x = region.dst_x, .y = region.dst_y, .w = region.w, .h = region.h });
        }
    }

    const start = timer.tickCount();
    var external = blit_backend.InvokeResult{};
    if (fb.isNativeXrgb32(f) and (f.pitch & 3) == 0) {
        const job = blit_backend.Job{
            .target_address = @intFromPtr(f.address),
            .target_width = @intCast(f.width),
            .target_height = @intCast(f.height),
            .target_pitch_pixels = @intCast(f.pitch / @sizeOf(u32)),
            .source_pixel_count = source_pixel_count,
            .source_address = @intFromPtr(source),
            .source_stride_pixels = source_stride_pixels,
            .region_count = @intCast(regions.len),
            .regions_address = @intFromPtr(regions.ptr),
        };
        external = blit_backend.invoke(&job);
    }

    if (external.attempted and external.result == 0) {
        outcome.accelerated = true;
        outcome.backend_name = external.name;
    } else {
        const source_bytes = @as([*]const u8, @ptrCast(source))[0 .. @as(usize, source_pixel_count) * @sizeOf(u32)];
        for (regions) |region| {
            if (!bootfbCopyXrgb32Region(device, region, source_bytes, source_stride_pixels)) return outcome;
        }
        outcome.fallback = true;
        outcome.fallback_regions = @intCast(regions.len);
        outcome.backend_error = if (external.attempted) external.result else 0;
        copyName(outcome.backend_name[0..], "bootfb-cpu");
    }

    present_generation +%= 1;
    if (present_generation == 0) present_generation = 1;
    completed_fence = present_generation;
    const completed_tick = timer.tickCount();
    recordPresentAggregate(device, .xrgb32_present, bounds, pixels_total, false);
    recordPresentTimingAt(device, start, completed_tick);
    outcome.success = true;
    outcome.present_generation = present_generation;
    outcome.fence = present_generation;
    outcome.completed_fence = completed_fence;
    outcome.region_count = @intCast(regions.len);
    outcome.pixel_count = @intCast(pixels_total);
    outcome.present_tick = completed_tick;
    outcome.elapsed_ticks = if (input_tick_valid and completed_tick >= input_tick) completed_tick - input_tick else 0;
    return outcome;
}

pub fn presentCapabilities() PresentCapabilities {
    var result = PresentCapabilities{
        .flags = 1 | 2 | 4,
    };
    copyName(result.backend_name[0..], "bootfb-cpu");
    copyName(result.fallback_name[0..], "bootfb-cpu");
    const external = blit_backend.snapshot();
    const target_compatible = if (primary_device) |device|
        if (device.framebuffer) |frame| fb.isNativeXrgb32(frame) and (frame.pitch & 3) == 0 else false
    else
        false;
    if (external.active and target_compatible) {
        result.flags |= 8 | 16;
        result.backend_kind = 2;
        result.backend_name = external.name;
        result.max_regions = @intCast(@min(@as(usize, external.max_regions), MAX_PRESENT_REGIONS));
    }
    return result;
}

pub fn presentFenceCompleted(fence: u64) bool {
    return fence != 0 and fence <= completed_fence;
}

pub fn highestCompletedFence() u64 {
    return completed_fence;
}

test "external blit error falls back once and preserves exact damage" {
    const testing = @import("std").testing;
    const FailBackend = struct {
        fn present(_: usize, _: *const blit_backend.Job) callconv(.c) i32 {
            return -77;
        }
    };

    var target: [64]u32 align(32) = .{0xA5A5_A5A5} ** 64;
    var frame = fb.Framebuffer{
        .address = @ptrCast(target[0..].ptr),
        .width = 8,
        .height = 8,
        .pitch = 8 * @sizeOf(u32),
        .bpp = 32,
        .memory_model = 1,
        .red_mask_size = 8,
        .red_mask_shift = 16,
        .green_mask_size = 8,
        .green_mask_shift = 8,
        .blue_mask_size = 8,
        .blue_mask_shift = 0,
        .unused = .{0} ** 5,
        .edid_size = 0,
        .edid = null,
    };
    registerBootBackend(.{
        .name = "test-bootfb",
        .kind = .bootfb,
        .flags = DeviceFlags.visible | DeviceFlags.cpu_present | DeviceFlags.rgb32 | DeviceFlags.xrgb32,
        .mode = .{ .width = 8, .height = 8, .pitch = 32, .bpp = 32 },
        .framebuffer = &frame,
    });
    defer {
        primary_device = null;
        bootfb_device = .{ .ops = &bootfb_ops };
        present_generation = 0;
        completed_fence = 0;
    }

    const descriptor = blit_backend.Descriptor{
        .flags = blit_backend.REQUIRED_FLAGS | blit_backend.FLAG_CPU_FAST_COPY,
        .max_regions = MAX_PRESENT_REGIONS,
        .present = FailBackend.present,
    };
    try testing.expectEqual(@as(i32, 0), blit_backend.register(91, "FAILBLIT", &descriptor));
    defer _ = blit_backend.unregister(91, "FAILBLIT");

    var source: [64]u32 align(32) = undefined;
    for (&source, 0..) |*pixel, index| pixel.* = 0x0010_0000 | @as(u32, @intCast(index));
    const regions = [_]PresentRegion{
        .{ .dst_x = 1, .dst_y = 1, .src_x = 1, .src_y = 1, .w = 2, .h = 2 },
        .{ .dst_x = 5, .dst_y = 5, .src_x = 5, .src_y = 5, .w = 2, .h = 2 },
    };
    const outcome = presentXrgb32Regions(source[0..].ptr, source.len, 8, regions[0..], 44, 0, false);
    try testing.expect(outcome.success);
    try testing.expect(outcome.fallback);
    try testing.expect(!outcome.accelerated);
    try testing.expectEqual(@as(i32, -77), outcome.backend_error);
    try testing.expectEqual(@as(u32, 2), outcome.region_count);
    try testing.expectEqual(@as(u32, 8), outcome.pixel_count);
    try testing.expectEqual(outcome.fence, outcome.completed_fence);
    try testing.expect(presentFenceCompleted(outcome.fence));

    var y: usize = 0;
    while (y < 8) : (y += 1) {
        var x: usize = 0;
        while (x < 8) : (x += 1) {
            const damaged = (x >= 1 and x < 3 and y >= 1 and y < 3) or
                (x >= 5 and x < 7 and y >= 5 and y < 7);
            try testing.expectEqual(if (damaged) source[y * 8 + x] else 0xA5A5_A5A5, target[y * 8 + x]);
        }
    }
}

pub fn operationNames(flags: u32) []const u8 {
    if ((flags & DeviceFlags.xrgb32) != 0) return "present/blit/fill/rect/xrgb32";
    if ((flags & DeviceFlags.packed32) != 0) return "present/blit/fill/rect/packed32";
    if ((flags & DeviceFlags.rect) != 0) return "fill/rect";
    return "none";
}

pub fn presentReasonName(reason: PresentReason) []const u8 {
    return switch (reason) {
        .none => "none",
        .fill => "fill",
        .rect => "rect",
        .packed32_present => "packed32-present",
        .xrgb32_present => "xrgb32-present",
    };
}

pub fn mappingKindName(kind: MappingKind) []const u8 {
    return switch (kind) {
        .none => "none",
        .bootloader_framebuffer => "bootloader-framebuffer",
    };
}

pub fn cachePolicyName(policy: CachePolicy) []const u8 {
    return switch (policy) {
        .unknown => "unknown",
        .bootloader_default => "bootloader-default",
        .pat_write_combining => "pat-write-combining",
        .write_combining_unsupported => "write-combining-unsupported",
        .write_combining_failed => "write-combining-failed",
    };
}

fn bootfbFill(device: *Device, rgb: u32) bool {
    const f = device.framebuffer orelse return false;
    fb.fill(f, rgb);
    recordPresent(device, .fill, 0, 0, @intCast(f.width), @intCast(f.height), false);
    return true;
}

fn bootfbRect(device: *Device, x: i32, y: i32, w: u32, h: u32, rgb: u32) bool {
    const f = device.framebuffer orelse return false;
    if (x < 0 or y < 0) return false;
    const ux: u64 = @intCast(x);
    const uy: u64 = @intCast(y);
    fb.rect(f, ux, uy, w, h, rgb);
    const clipped_w = if (ux >= f.width) 0 else @min(@as(u64, w), f.width - ux);
    const clipped_h = if (uy >= f.height) 0 else @min(@as(u64, h), f.height - uy);
    recordPresent(device, .rect, @intCast(ux), @intCast(uy), @intCast(clipped_w), @intCast(clipped_h), false);
    return true;
}

fn bootfbPutPacked32(device: *Device, x: u64, y: u64, color32: u32) bool {
    const f = device.framebuffer orelse return false;
    fb.putPacked32(f, x, y, color32);
    return true;
}

fn bootfbPutXrgb32(device: *Device, x: u64, y: u64, rgb: u32) bool {
    const f = device.framebuffer orelse return false;
    fb.putPacked32(f, x, y, fb.packRgb(f, rgb));
    return true;
}

fn bootfbPresentPacked32Rect(device: *Device, x0: u64, y0: u64, w: u64, h: u64, src: []const u8, src_stride_pixels: u64) bool {
    return bootfbCopyPacked32Rect(device, .packed32_present, x0, y0, w, h, src, src_stride_pixels);
}

fn bootfbCopyPacked32Rect(device: *Device, reason: PresentReason, x0: u64, y0: u64, w: u64, h: u64, src: []const u8, src_stride_pixels: u64) bool {
    const f = device.framebuffer orelse return false;
    if (!fb.supportsRgb32(f) or w == 0 or h == 0) return false;
    if (x0 >= f.width or y0 >= f.height) return false;

    const clipped_w = @min(w, f.width - x0);
    const clipped_h = @min(h, f.height - y0);
    const src_stride_bytes = src_stride_pixels * 4;
    const row_bytes = clipped_w * 4;
    if (src.len < (clipped_h - 1) * src_stride_bytes + row_bytes) return false;

    var y: u64 = 0;
    while (y < clipped_h) : (y += 1) {
        const src_offset: usize = @intCast(y * src_stride_bytes);
        const dst = f.address + (y0 + y) * f.pitch + x0 * 4;
        copyToVisible(dst, src[src_offset .. src_offset + @as(usize, @intCast(row_bytes))]);
    }
    recordPresent(device, reason, @intCast(x0), @intCast(y0), @intCast(clipped_w), @intCast(clipped_h), false);
    return true;
}

fn bootfbPresentXrgb32Rect(device: *Device, x0: u64, y0: u64, w: u64, h: u64, src: []const u8, src_stride_pixels: u64) bool {
    const f = device.framebuffer orelse return false;
    if (!fb.supportsRgb32(f) or w == 0 or h == 0) return false;
    if (x0 >= f.width or y0 >= f.height) return false;

    const clipped_w = @min(w, f.width - x0);
    const clipped_h = @min(h, f.height - y0);
    const src_stride_bytes = src_stride_pixels * 4;
    const row_bytes = clipped_w * 4;
    if (src.len < (clipped_h - 1) * src_stride_bytes + row_bytes) return false;

    if (fb.isNativeXrgb32(f)) {
        return bootfbCopyPacked32Rect(device, .xrgb32_present, x0, y0, clipped_w, clipped_h, src, src_stride_pixels);
    }

    var y: u64 = 0;
    while (y < clipped_h) : (y += 1) {
        const src_offset: usize = @intCast(y * src_stride_bytes);
        var x: u64 = 0;
        while (x < clipped_w) : (x += 1) {
            const pixel_offset = src_offset + @as(usize, @intCast(x * 4));
            fb.putPacked32(f, x0 + x, y0 + y, fb.packRgb(f, readXrgb32(src, pixel_offset)));
        }
    }
    recordPresent(device, .xrgb32_present, @intCast(x0), @intCast(y0), @intCast(clipped_w), @intCast(clipped_h), true);
    return true;
}

fn bootfbCopyXrgb32Region(device: *Device, region: PresentRegion, src: []const u8, src_stride_pixels: u32) bool {
    const f = device.framebuffer orelse return false;
    const src_stride_bytes = @as(u64, src_stride_pixels) * @sizeOf(u32);
    const row_bytes = @as(u64, region.w) * @sizeOf(u32);
    const first_offset = (@as(u64, region.src_y) * src_stride_pixels + region.src_x) * @sizeOf(u32);
    const last_end = first_offset + (@as(u64, region.h) - 1) * src_stride_bytes + row_bytes;
    if (last_end > src.len) return false;

    var y: u32 = 0;
    while (y < region.h) : (y += 1) {
        const src_offset: usize = @intCast(first_offset + @as(u64, y) * src_stride_bytes);
        if (fb.isNativeXrgb32(f)) {
            const dst = f.address + (@as(u64, region.dst_y) + y) * f.pitch + @as(u64, region.dst_x) * @sizeOf(u32);
            copyToVisible(dst, src[src_offset .. src_offset + @as(usize, @intCast(row_bytes))]);
            continue;
        }
        var x: u32 = 0;
        while (x < region.w) : (x += 1) {
            const pixel_offset = src_offset + @as(usize, x) * @sizeOf(u32);
            fb.putPacked32(
                f,
                @as(u64, region.dst_x) + x,
                @as(u64, region.dst_y) + y,
                fb.packRgb(f, readXrgb32(src, pixel_offset)),
            );
        }
    }
    return true;
}

fn validPresentRegion(region: PresentRegion, mode: Mode, source_pixel_count: u32, source_stride_pixels: u32) bool {
    if (region.w == 0 or region.h == 0 or region.src_x >= source_stride_pixels) return false;
    if (region.w > source_stride_pixels - region.src_x) return false;
    if (region.dst_x >= mode.width or region.dst_y >= mode.height) return false;
    if (region.w > mode.width - region.dst_x or region.h > mode.height - region.dst_y) return false;
    const last_row = @as(u64, region.src_y) + region.h - 1;
    const last_end = last_row * source_stride_pixels + region.src_x + region.w;
    return last_end <= source_pixel_count;
}

fn mergeRect(a: Rect, b: Rect) Rect {
    const left = @min(a.x, b.x);
    const top = @min(a.y, b.y);
    const right = @max(@as(u64, a.x) + a.w, @as(u64, b.x) + b.w);
    const bottom = @max(@as(u64, a.y) + a.h, @as(u64, b.y) + b.h);
    return .{
        .x = left,
        .y = top,
        .w = @intCast(right - left),
        .h = @intCast(bottom - top),
    };
}

fn copyName(out: []u8, name: []const u8) void {
    @memset(out, 0);
    if (out.len == 0) return;
    const count = @min(name.len, out.len - 1);
    if (count != 0) @memcpy(out[0..count], name[0..count]);
}

fn recordPresent(device: *Device, reason: PresentReason, x: u32, y: u32, w: u32, h: u32, converted: bool) void {
    const pixels = @as(u64, w) * h;
    recordPresentAggregate(device, reason, .{ .x = x, .y = y, .w = w, .h = h }, pixels, converted);
}

fn recordPresentAggregate(device: *Device, reason: PresentReason, bounds: Rect, pixels: u64, converted: bool) void {
    const bytes = pixels * 4;
    device.present_count += 1;
    device.present_pixels_total += pixels;
    device.present_bytes_total += bytes;
    device.last_present_pixels = pixels;
    device.last_present_bytes = bytes;
    device.last_present_rect = bounds;
    device.last_present_reason = reason;
    device.last_present_converted = converted;
    if (converted) device.conversion_present_count += 1;
    if (bounds.x == 0 and bounds.y == 0 and bounds.w == device.mode.width and bounds.h == device.mode.height and pixels == @as(u64, device.mode.width) * device.mode.height) {
        device.full_present_count += 1;
    } else {
        device.partial_present_count += 1;
    }
    switch (reason) {
        .none => {},
        .fill => device.fill_present_count += 1,
        .rect => device.rect_present_count += 1,
        .packed32_present => device.packed32_present_count += 1,
        .xrgb32_present => device.xrgb32_present_count += 1,
    }
}

fn recordPresentTiming(device: *Device, start: u64) void {
    recordPresentTimingAt(device, start, timer.tickCount());
}

fn recordPresentTimingAt(device: *Device, start: u64, end: u64) void {
    const elapsed = if (end >= start) end - start else 0;
    device.present_total_ticks +%= elapsed;
    device.present_last_ticks = elapsed;
    if (elapsed > device.present_max_ticks) device.present_max_ticks = elapsed;
    if (elapsed > 1) device.present_slow_count +%= 1;
}

fn copyToVisible(dst: [*]volatile u8, src: []const u8) void {
    // 0.56.12: Vorab-validierter, subcall-freier u32-Zeilenpfad. Der alte
    // Loop rief readXrgb32 PRO PIXEL (Bounds-Check + Alignment-Check je
    // Wort) - bei Millionen Pixeln pro Frame reiner Overhead. Ist sowohl
    // Ziel ALS AUCH Quelle 4-Byte-ausgerichtet (Normalfall: Framebuffer
    // und Present-Puffer sind seiten-/wortausgerichtet), laeuft eine
    // reine Wort-fuer-Wort-Kopie ganz ohne Pro-Pixel-Check.
    const word_len = src.len & ~@as(usize, 3);
    if ((@intFromPtr(dst) & 3) == 0 and (@intFromPtr(src.ptr) & 3) == 0) {
        const dst_words: [*]volatile u32 = @ptrCast(@alignCast(dst));
        const src_words: [*]const u32 = @ptrCast(@alignCast(src.ptr));
        const count = word_len / 4;
        var wi: usize = 0;
        while (wi < count) : (wi += 1) dst_words[wi] = src_words[wi];
        var i: usize = word_len;
        while (i < src.len) : (i += 1) dst[i] = src[i];
        return;
    }
    // Ziel wortausgerichtet, Quelle nicht: Woerter aus Bytes assemblieren,
    // aber ohne den Pro-Wort-Bounds-Check des alten readXrgb32.
    if ((@intFromPtr(dst) & 3) == 0 and word_len == src.len) {
        const dst_words: [*]volatile u32 = @ptrCast(@alignCast(dst));
        const count = word_len / 4;
        var wi: usize = 0;
        while (wi < count) : (wi += 1) {
            const o = wi * 4;
            dst_words[wi] = @as(u32, src[o]) |
                (@as(u32, src[o + 1]) << 8) |
                (@as(u32, src[o + 2]) << 16) |
                (@as(u32, src[o + 3]) << 24);
        }
        return;
    }
    var i: usize = 0;
    while (i < src.len) : (i += 1) dst[i] = src[i];
}

fn readXrgb32(src: []const u8, offset: usize) u32 {
    if (offset + 3 >= src.len) return 0;
    const ptr = &src[offset];
    if ((@intFromPtr(ptr) & 3) == 0) {
        const word: *const u32 = @ptrCast(@alignCast(ptr));
        return word.*;
    }
    return @as(u32, src[offset + 0]) |
        (@as(u32, src[offset + 1]) << 8) |
        (@as(u32, src[offset + 2]) << 16) |
        (@as(u32, src[offset + 3]) << 24);
}

pub const DisplayManager = struct {
    // Marker type for the current singleton manager. In this transition step the
    // module still owns the active backend internally; callers should treat the
    // exported fill/rect/put/present functions as the DisplayManager boundary.
};
