const fb = @import("../display/framebuffer.zig");
const r4b = @import("bootscreen_r4b.zig");

const TRACK: u32 = 0x041318;
const TRACK_BORDER_LIGHT: u32 = 0xC8C8C8;
const TRACK_BORDER_DARK: u32 = 0x06242A;
const PROGRESS: u32 = 0xD9C75E;
const PROGRESS_HILITE: u32 = 0xFFF2A0;

const bootscreen_asset = @embedFile("generated/BOOTSCREEN.R4B");

pub const Phase = enum(u8) {
    framebuffer = 4,
    cpu = 9,
    timer = 13,
    driver = 18,
    input = 23,
    intro = 28,
    memory = 39,
    storage_foundation = 49,
    module = 54,
    platform = 61,
    usb_preload = 65,
    service = 69,
    storage_controllers = 76,
    loader = 83,
    irq = 87,
    audio = 90,
    network = 93,
    usb_hid = 96,
    task_runtime = 97,
    driver_policy = 98,
    runtime = 99,
    handoff = 100,
};

pub const RenderResult = enum(u8) {
    framebuffer,
    unsupported,
};

const Geometry = struct {
    progress_x: u64 = 0,
    progress_y: u64 = 0,
    progress_w: u64 = 0,
    progress_h: u64 = 0,
};

var active_framebuffer: ?*fb.Framebuffer = null;
var active_geometry: Geometry = .{};
var last_phase: Phase = .framebuffer;

pub fn renderToFramebuffer(framebuf: *fb.Framebuffer) RenderResult {
    if (!fb.supportsRgb32(framebuf)) return .unsupported;
    const geometry = draw(framebuf) orelse return .unsupported;
    active_framebuffer = framebuf;
    active_geometry = geometry;
    last_phase = .framebuffer;
    drawProgress(framebuf, active_geometry, @intFromEnum(last_phase));
    return .framebuffer;
}

pub fn setPhase(phase: Phase) void {
    const framebuf = active_framebuffer orelse return;
    if (!fb.supportsRgb32(framebuf)) return;
    if (@intFromEnum(phase) < @intFromEnum(last_phase)) return;
    last_phase = phase;
    drawProgress(framebuf, active_geometry, @intFromEnum(phase));
}

pub fn completeForHandoff() void {
    // The shell reports readiness after its first committed frame/prompt.
    // Retire the boot renderer without painting over that ready surface.
    last_phase = .handoff;
    active_framebuffer = null;
    active_geometry = .{};
}

pub fn renderPreparedR4B(framebuf: *fb.Framebuffer, bytes: []const u8) bool {
    return r4b.draw(framebuf, bytes);
}

fn draw(framebuf: *fb.Framebuffer) ?Geometry {
    if (!renderPreparedR4B(framebuf, bootscreen_asset[0..])) return null;
    return progressGeometry(framebuf);
}

fn progressGeometry(framebuf: *const fb.Framebuffer) Geometry {
    const progress_w: u64 = if (framebuf.width >= 960)
        420
    else if (framebuf.width >= 320)
        framebuf.width / 2
    else
        framebuf.width;
    const progress_h: u64 = if (framebuf.height >= 480) 14 else 8;
    const bottom_margin: u64 = if (framebuf.height >= 480) 96 else 24;
    const progress_x = if (framebuf.width > progress_w) (framebuf.width - progress_w) / 2 else 0;
    const progress_y = if (framebuf.height > bottom_margin + progress_h)
        framebuf.height - bottom_margin
    else if (framebuf.height > progress_h + 4)
        framebuf.height - progress_h - 4
    else
        0;
    return .{
        .progress_x = progress_x,
        .progress_y = progress_y,
        .progress_w = progress_w,
        .progress_h = progress_h,
    };
}

fn drawProgress(framebuf: *fb.Framebuffer, geometry: Geometry, percent: u8) void {
    if (geometry.progress_w < 4 or geometry.progress_h < 3) return;
    if (geometry.progress_x >= framebuf.width or geometry.progress_y >= framebuf.height) return;
    if (geometry.progress_y + geometry.progress_h + 2 >= framebuf.height) return;

    drawProgressTrack(framebuf, geometry);

    const inner_x = geometry.progress_x + 2;
    const inner_y = geometry.progress_y + 2;
    const inner_w = if (geometry.progress_w > 4) geometry.progress_w - 4 else 0;
    const inner_h = if (geometry.progress_h > 4) geometry.progress_h - 4 else 1;
    const clamped = if (percent > 100) 100 else percent;
    const fill_w = (inner_w * clamped) / 100;
    if (fill_w == 0) return;

    fb.rect(framebuf, inner_x, inner_y, fill_w, inner_h, PROGRESS);
    if (inner_h > 1) fb.rect(framebuf, inner_x, inner_y, fill_w, 1, PROGRESS_HILITE);
}

fn drawProgressTrack(framebuf: *fb.Framebuffer, geometry: Geometry) void {
    if (geometry.progress_w < 4 or geometry.progress_h < 3) return;
    fb.rect(framebuf, geometry.progress_x, geometry.progress_y, geometry.progress_w, geometry.progress_h, TRACK_BORDER_DARK);
    fb.rect(framebuf, geometry.progress_x, geometry.progress_y, geometry.progress_w, 1, TRACK_BORDER_LIGHT);
    fb.rect(framebuf, geometry.progress_x, geometry.progress_y, 1, geometry.progress_h, TRACK_BORDER_LIGHT);
    fb.rect(framebuf, geometry.progress_x + 1, geometry.progress_y + 1, geometry.progress_w - 2, geometry.progress_h - 2, TRACK);
}
