// Canonical surface-to-display presenter.
//
// This module normalizes source/destination regions. DisplayManager owns the
// only productive present counters, backend selection, fences and fallback.

const surface = @import("surface.zig");
const display = @import("display.zig");

pub const PresentReason = enum {
    unknown,
    boot,
    redraw,
    cursor,
    diagnostic,
    legacy_bridge,
};

pub const Region = struct {
    source_x: u32 = 0,
    source_y: u32 = 0,
    destination_x: u32 = 0,
    destination_y: u32 = 0,
    w: u32 = 0,
    h: u32 = 0,
};

pub const PresentRequest = struct {
    source: surface.View,
    regions: []const Region,
    reason: PresentReason = .redraw,
    source_generation: u64 = 0,
    input_tick: u64 = 0,
    input_tick_valid: bool = false,
};

pub const PresentResult = display.PresentOutcome;

pub fn presentXrgb32(request: PresentRequest) PresentResult {
    if (request.source.format != .xrgb32 or request.regions.len == 0 or
        request.regions.len > display.MAX_PRESENT_REGIONS)
    {
        return .{ .source_generation = request.source_generation };
    }

    var normalized: [display.MAX_PRESENT_REGIONS]display.PresentRegion =
        .{display.PresentRegion{}} ** display.MAX_PRESENT_REGIONS;
    for (request.regions, 0..) |region, index| {
        if (region.w == 0 or region.h == 0 or
            @as(u64, region.source_x) + region.w > request.source.width or
            @as(u64, region.source_y) + region.h > request.source.height)
        {
            return .{ .source_generation = request.source_generation };
        }
        normalized[index] = .{
            .dst_x = region.destination_x,
            .dst_y = region.destination_y,
            .src_x = region.source_x,
            .src_y = region.source_y,
            .w = region.w,
            .h = region.h,
        };
    }

    const pixel_count = request.source.pixels.len;
    if (pixel_count > ~@as(u32, 0) or request.source.pitch_pixels > ~@as(u32, 0)) {
        return .{ .source_generation = request.source_generation };
    }
    return display.presentXrgb32Regions(
        request.source.pixels.ptr,
        @intCast(pixel_count),
        @intCast(request.source.pitch_pixels),
        normalized[0..request.regions.len],
        request.source_generation,
        request.input_tick,
        request.input_tick_valid,
    );
}

pub fn presentRect(source: *surface.Surface, rect: surface.Rect, reason: PresentReason) PresentResult {
    const clipped = rect.clipTo(source.width, source.height) orelse return .{};
    const x: u32 = @intCast(clipped.x);
    const y: u32 = @intCast(clipped.y);
    const region = Region{
        .source_x = x,
        .source_y = y,
        .destination_x = x,
        .destination_y = y,
        .w = @intCast(clipped.w),
        .h = @intCast(clipped.h),
    };
    return presentXrgb32(.{
        .source = source.view(),
        .regions = (&region)[0..1],
        .reason = reason,
        .source_generation = source.revision,
    });
}
