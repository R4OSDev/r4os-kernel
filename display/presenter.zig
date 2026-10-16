//! Projection of native presentation normalization; DisplayManager owns results.
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

pub const Region = extern struct {
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
const View = extern struct { pixels: [*]const u32, pixel_count: usize, width: usize, height: usize, pitch_pixels: usize, format: u32, revision: u64 };
const Request = extern struct { source: View, regions: [*]const Region, regions_len: usize, reason: u32, source_generation: u64, input_tick: u64, input_tick_valid: bool };
const NativeSurface = extern struct { pixels: [*]u32, pixel_count: usize, width: usize, height: usize, pitch_pixels: usize, format: u32, kind: u32, revision: u64 };
extern fn r4os_rust_display_presenter(u32, *const anyopaque, ?*const surface.Rect, u32, *PresentResult) void;
pub fn presentXrgb32(request: PresentRequest) PresentResult {
    const input: Request = .{ .source = .{ .pixels = request.source.pixels.ptr, .pixel_count = request.source.pixels.len,
        .width = request.source.width, .height = request.source.height, .pitch_pixels = request.source.pitch_pixels,
        .format = @intFromEnum(request.source.format), .revision = request.source.revision },
        .regions = request.regions.ptr, .regions_len = request.regions.len, .reason = @intFromEnum(request.reason),
        .source_generation = request.source_generation, .input_tick = request.input_tick, .input_tick_valid = request.input_tick_valid };
    var out: PresentResult = undefined; r4os_rust_display_presenter(0, &input, null, 0, &out); return out;
}
pub fn presentRect(source: *surface.Surface, rect: surface.Rect, reason: PresentReason) PresentResult {
    const input: NativeSurface = .{ .pixels = source.pixels.ptr, .pixel_count = source.pixels.len, .width = source.width, .height = source.height,
        .pitch_pixels = source.pitch_pixels, .format = @intFromEnum(source.format), .kind = @intFromEnum(source.kind), .revision = source.revision };
    var out: PresentResult = undefined; r4os_rust_display_presenter(1, &input, &rect, @intFromEnum(reason), &out); return out;
}
comptime { if (@sizeOf(View) != 56 or @sizeOf(Request) != 104 or @sizeOf(Region) != 24) @compileError("Native presenter C layout"); }
