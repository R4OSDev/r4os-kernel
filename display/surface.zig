// R4OS generic software surface model.
//
// Introduced in 0.26.3 as the neutral replacement vocabulary for the old surface_pipeline
// backbuffer concept. This file is intentionally standalone for the first
// refactor step; call sites are migrated in later 0.26.X versions.

pub const PixelFormat = enum {
    xrgb32,
};

pub const SurfaceKind = enum {
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

pub const Rect = struct {
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

    pub fn right(self: Rect) i32 {
        return self.x + @as(i32, @intCast(self.w));
    }

    pub fn bottom(self: Rect) i32 {
        return self.y + @as(i32, @intCast(self.h));
    }

    pub fn clipTo(self: Rect, width: usize, height: usize) ?Rect {
        if (self.isEmpty()) return null;

        var x0 = self.x;
        var y0 = self.y;
        var x1 = self.right();
        var y1 = self.bottom();

        if (x0 < 0) x0 = 0;
        if (y0 < 0) y0 = 0;

        const max_x = @as(i32, @intCast(width));
        const max_y = @as(i32, @intCast(height));
        if (x1 > max_x) x1 = max_x;
        if (y1 > max_y) y1 = max_y;
        if (x1 <= x0 or y1 <= y0) return null;

        return .{
            .x = x0,
            .y = y0,
            .w = @as(usize, @intCast(x1 - x0)),
            .h = @as(usize, @intCast(y1 - y0)),
        };
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

    pub fn line(self: *Surface, y: usize) []u32 {
        const start = y * self.pitch_pixels;
        return self.pixels[start .. start + self.width];
    }

    pub fn fill(self: *Surface, color: u32) void {
        var y: usize = 0;
        while (y < self.height) : (y += 1) {
            @memset(self.line(y), color);
        }
        self.revision +%= 1;
    }

    pub fn fillRect(self: *Surface, rect: Rect, color: u32) void {
        const clipped = rect.clipTo(self.width, self.height) orelse return;
        const x0 = @as(usize, @intCast(clipped.x));
        const y0 = @as(usize, @intCast(clipped.y));
        var y: usize = 0;
        while (y < clipped.h) : (y += 1) {
            const start = (y0 + y) * self.pitch_pixels + x0;
            @memset(self.pixels[start .. start + clipped.w], color);
        }
        self.revision +%= 1;
    }
};
