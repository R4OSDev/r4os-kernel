// R4OS generic software surface model.
//
// Introduced in 0.26.3 as the neutral replacement vocabulary for the old surface_pipeline
// backbuffer concept. This file is intentionally standalone for the first
// refactor step; call sites are migrated in later 0.26.X versions.

pub const PixelFormat = enum(u32) {
    xrgb32,
};

pub const SurfaceKind = enum(u32) {
    generic,
    boot_framebuffer_shadow,
    desktop,
    window_frame,
    window_title,
    window_client,
    popup,
    dialog,
    cursor,
    legacy_bridge,
};

pub const Rect = extern struct {
    x: i32,
    y: i32,
    w: usize,
    h: usize,

    pub fn empty() Rect {
        return .{ .x = 0, .y = 0, .w = 0, .h = 0 };
    }

    pub fn isEmpty(self: Rect) bool {
        return self.w == 0 or self.h == 0;
    }

    pub fn right(self: Rect) i32 { var out: Rect = undefined; _ = r4os_rust_surface_rect(1, &self, 0, 0, &out); return out.x; }

    pub fn bottom(self: Rect) i32 { var out: Rect = undefined; _ = r4os_rust_surface_rect(2, &self, 0, 0, &out); return out.y; }

    pub fn clipTo(self: Rect, width: usize, height: usize) ?Rect {
        var out: Rect = undefined; return if (r4os_rust_surface_rect(0, &self, width, height, &out)) out else null;
    }
};

pub const Surface = struct {
    pixels: []u32,
    width: usize,
    height: usize,
    pitch_pixels: usize,
    format: PixelFormat = .xrgb32,
    kind: SurfaceKind = .generic,
    revision: u64 = 0,

    pub fn init(
        pixels: []u32,
        width: usize,
        height: usize,
        pitch_pixels: usize,
        kind: SurfaceKind,
    ) Surface {
        return .{
            .pixels = pixels,
            .width = width,
            .height = height,
            .pitch_pixels = pitch_pixels,
            .kind = kind,
        };
    }

    pub fn bounds(self: Surface) Rect {
        return .{ .x = 0, .y = 0, .w = self.width, .h = self.height };
    }

    pub fn view(self: *const Surface) View {
        return .{
            .pixels = self.pixels,
            .width = self.width,
            .height = self.height,
            .pitch_pixels = self.pitch_pixels,
            .format = self.format,
            .revision = self.revision,
        };
    }

    fn native(self: *const Surface) NativeSurface {
        return .{ .pixels = self.pixels.ptr, .pixel_count = self.pixels.len, .width = self.width,
            .height = self.height, .pitch_pixels = self.pitch_pixels, .format = @intFromEnum(self.format),
            .kind = @intFromEnum(self.kind), .revision = self.revision };
    }
    pub fn line(self: *Surface, y: usize) []u32 {
        const value = self.native(); var pixels: [*]u32 = undefined; var count: usize = undefined;
        r4os_rust_surface_line(&value, y, &pixels, &count); return pixels[0..count];
    }

    pub fn fill(self: *Surface, color: u32) void {
        var value = self.native(); r4os_rust_surface_fill(&value, color); self.revision = value.revision;
    }

    pub fn fillRect(self: *Surface, rect: Rect, color: u32) void {
        var value = self.native(); r4os_rust_surface_fill_rect(&value, &rect, color); self.revision = value.revision;
    }
};

pub const View = struct {
    pixels: []const u32,
    width: usize,
    height: usize,
    pitch_pixels: usize,
    format: PixelFormat = .xrgb32,
    revision: u64 = 0,
};

// Slice headers remain a Zig caller projection, never a cross-language layout.
const NativeSurface = extern struct {
    pixels: [*]u32, pixel_count: usize, width: usize, height: usize, pitch_pixels: usize,
    format: u32, kind: u32, revision: u64,
};
extern fn r4os_rust_surface_rect(u32, *const Rect, usize, usize, *Rect) bool;
extern fn r4os_rust_surface_line(*const NativeSurface, usize, *[*]u32, *usize) void;
extern fn r4os_rust_surface_fill(*NativeSurface, u32) void;
extern fn r4os_rust_surface_fill_rect(*NativeSurface, *const Rect, u32) void;
comptime {
    if (@sizeOf(Rect) != 24 or @alignOf(Rect) != 8 or @offsetOf(Rect, "w") != 8 or @sizeOf(NativeSurface) != 56 or @offsetOf(NativeSurface, "revision") != 48) @compileError("Native software surface C layout");
}
