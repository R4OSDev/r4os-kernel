// R4OS display presenter contract.
//
// Introduced in 0.26.4 as the neutral boundary between software surfaces and
// the DisplayManager/DisplayBackend path. The legacy surface_pipeline present path remains
// active until later 0.26.X steps migrate call sites onto this contract.

const surface = @import("surface.zig");
const display = @import("../display/display.zig");

var stats_storage: PresentStats = .{};

pub const PresentReason = enum {
    unknown,
    boot,
    redraw,
    cursor,
    diagnostic,
    legacy_bridge,
};

pub const PresentMode = enum {
    partial,
    full,
};

pub const PresentRequest = struct {
    source: *surface.Surface,
    rect: surface.Rect,
    reason: PresentReason = .redraw,
};

pub const PresentResult = struct {
    rect: surface.Rect,
    mode: PresentMode,
    pixels: usize,
};

pub const PresentStats = struct {
    presents: u64 = 0,
    full_presents: u64 = 0,
    partial_presents: u64 = 0,
    skipped_empty: u64 = 0,
    pixels_presented: u64 = 0,
    last_rect: surface.Rect = surface.Rect.empty(),
    last_reason: PresentReason = .unknown,

    pub fn record(self: *PresentStats, result: PresentResult, reason: PresentReason) void {
        self.presents +%= 1;
        switch (result.mode) {
            .full => self.full_presents +%= 1,
            .partial => self.partial_presents +%= 1,
        }
        self.pixels_presented +%= @as(u64, @intCast(result.pixels));
        self.last_rect = result.rect;
        self.last_reason = reason;
    }

    pub fn recordSkippedEmpty(self: *PresentStats) void {
        self.skipped_empty +%= 1;
    }
};

pub fn normalize(request: PresentRequest) ?PresentResult {
    const clipped = request.rect.clipTo(request.source.width, request.source.height) orelse return null;
    const full = clipped.x == 0 and clipped.y == 0 and
        clipped.w == request.source.width and clipped.h == request.source.height;

    return .{
        .rect = clipped,
        .mode = if (full) .full else .partial,
        .pixels = clipped.w * clipped.h,
    };
}

pub fn presentXrgb32(request: PresentRequest) ?PresentResult {
    if (request.source.format != .xrgb32) return null;
    const result = normalize(request) orelse {
        stats_storage.recordSkippedEmpty();
        return null;
    };

    const rect = result.rect;
    const x0 = @as(usize, @intCast(rect.x));
    const y0 = @as(usize, @intCast(rect.y));
    const source = request.source;
    const offset = (y0 * source.pitch_pixels + x0) * @sizeOf(u32);
    const byte_len = source.pixels.len * @sizeOf(u32);
    if (offset >= byte_len) return null;
    const bytes = @as([*]const u8, @ptrCast(source.pixels.ptr))[0..byte_len];

    if (!display.presentXrgb32Rect(
        @intCast(x0),
        @intCast(y0),
        @intCast(rect.w),
        @intCast(rect.h),
        bytes[offset..],
        @intCast(source.pitch_pixels),
    )) return null;

    stats_storage.record(result, request.reason);
    return result;
}

pub fn stats() PresentStats {
    return stats_storage;
}

pub fn resetStats() void {
    stats_storage = .{};
}
