const display = @import("display.zig");

const Target = struct {
    width: u64 = 0,
    height: u64 = 0,
    pitch: u64 = 0,
    bpp: u16 = 0,

    fn valid(self: Target) bool {
        return self.width != 0 and self.height != 0;
    }
};

const FrameMode = enum(u8) {
    none = 0,
    explicit_full_frame = 1,
    explicit_rect_frame = 2,
};

var target: Target = .{};
var frame_active = false;
var frame_mode: FrameMode = .none;
var last_frame_mode: FrameMode = .none;

pub fn initFromDisplayManager() void {
    const mode = display.activeMode() orelse {
        initTarget(.{});
        return;
    };
    initTarget(.{
        .width = mode.width,
        .height = mode.height,
        .pitch = mode.pitch,
        .bpp = mode.bpp,
    });
}

pub fn initTarget(new_target: Target) void {
    target = new_target;
    frame_active = false;
    frame_mode = .none;
    last_frame_mode = .none;
}

pub fn width() u32 {
    return @intCast(target.width);
}

pub fn height() u32 {
    return @intCast(target.height);
}

pub fn beginFrame() i32 {
    if (!target.valid()) return 0;
    frame_active = true;
    frame_mode = .explicit_full_frame;
    return 1;
}

pub fn beginFrameRect(x: i32, y: i32, w: u32, h: u32) i32 {
    _ = x;
    _ = y;
    if (!target.valid() or w == 0 or h == 0) return 0;
    frame_active = true;
    frame_mode = .explicit_rect_frame;
    return 1;
}

pub fn present() i32 {
    if (!target.valid()) return 0;
    if (frame_mode != .none) last_frame_mode = frame_mode;
    frame_active = false;
    frame_mode = .none;
    return 1;
}
