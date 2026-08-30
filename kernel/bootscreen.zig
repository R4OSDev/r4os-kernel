const fb = @import("../display/framebuffer.zig");
const font = @import("font.zig");
const r4b = @import("bootscreen_r4b.zig");

const TRACK: u32 = 0x041318;
const TRACK_BORDER_LIGHT: u32 = 0xC8C8C8;
const TRACK_BORDER_DARK: u32 = 0x06242A;
const PROGRESS: u32 = 0xD9C75E;
const PROGRESS_HILITE: u32 = 0xFFF2A0;
const STATUS_BACKGROUND: u32 = 0x041318;
const STATUS_FOREGROUND: u32 = 0xF0F0F0;
const DETAIL_FOREGROUND: u32 = 0xB8D5D8;
const ERROR_FOREGROUND: u32 = 0xFF8A80;
const STATUS_TEXT_PADDING: u64 = 4;
const STATUS_TOP_GAP: u64 = 6;
const STATUS_ROW_GAP: u64 = 3;
const STATUS_BOTTOM_GAP: u64 = 4;
const MAX_STATUS_BYTES: usize = 32;

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
    status_x: u64 = 0,
    status_y: u64 = 0,
    status_w: u64 = 0,
    status_scale: u64 = 1,
};

const LineText = struct {
    bytes: [MAX_STATUS_BYTES]u8 = .{0} ** MAX_STATUS_BYTES,
    len: usize = 0,

    fn clear(self: *LineText) void {
        self.bytes = .{0} ** MAX_STATUS_BYTES;
        self.len = 0;
    }

    fn set(self: *LineText, text: []const u8) void {
        self.clear();
        for (text) |ch| {
            if (self.len >= self.bytes.len) break;
            self.bytes[self.len] = if (ch >= 0x20 and ch <= 0x7E)
                ch
            else if (ch == '\r' or ch == '\n' or ch == '\t')
                ' '
            else
                '?';
            self.len += 1;
        }
        while (self.len > 0 and self.bytes[self.len - 1] == ' ') self.len -= 1;
    }

    fn slice(self: *const LineText) []const u8 {
        return self.bytes[0..self.len];
    }
};

var active_framebuffer: ?*fb.Framebuffer = null;
var active_geometry: Geometry = .{};
var last_phase: Phase = .framebuffer;
var current_status: LineText = .{};
var current_detail: LineText = .{};
var current_error: LineText = .{};

pub fn renderToFramebuffer(framebuf: *fb.Framebuffer) RenderResult {
    if (!fb.supportsRgb32(framebuf)) return .unsupported;
    const geometry = draw(framebuf) orelse return .unsupported;
    active_framebuffer = framebuf;
    active_geometry = geometry;
    last_phase = .framebuffer;
    current_status.clear();
    current_detail.clear();
    current_error.clear();
    drawProgress(framebuf, active_geometry, @intFromEnum(last_phase));
    redrawStatusRows();
    return .framebuffer;
}

pub fn setPhase(phase: Phase) void {
    const framebuf = active_framebuffer orelse return;
    if (!fb.supportsRgb32(framebuf)) return;
    if (@intFromEnum(phase) < @intFromEnum(last_phase)) return;
    last_phase = phase;
    drawProgress(framebuf, active_geometry, @intFromEnum(phase));
}

pub fn isActive() bool {
    return active_framebuffer != null;
}

pub fn setStatus(text: []const u8) void {
    current_status.set(text);
    redrawStatusRows();
}

pub fn setError(text: []const u8) void {
    if (current_error.len != 0) return;
    current_detail.clear();
    current_error.set(text);
    redrawStatusRows();
}

pub fn setDetail(text: []const u8) void {
    if (current_error.len != 0) return;
    current_detail.set(text);
    redrawStatusRows();
}

pub fn clearDetail() void {
    current_detail.clear();
    redrawStatusRows();
}

pub fn setDriver(name: []const u8) void {
    setPrefixedStatus("Treiber: ", name);
}

pub fn setDriverError(name: []const u8) void {
    setPrefixedError("Fehler: ", name);
}

pub fn setService(name: []const u8) void {
    setPrefixedDetail("Dienst: ", name);
}

pub fn setServiceStage(name: []const u8, stage: []const u8) void {
    if (current_error.len != 0) return;
    var line: [MAX_STATUS_BYTES]u8 = .{0} ** MAX_STATUS_BYTES;
    var len = copyLinePart(line[0..], 0, name);
    len = copyLinePart(line[0..], len, ": ");
    len = copyLinePart(line[0..], len, stage);
    setDetail(line[0..len]);
}

pub fn setServiceError(name: []const u8) void {
    setPrefixedError("Fehler: ", name);
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
    const default_progress_y = if (framebuf.height > bottom_margin + progress_h)
        framebuf.height - bottom_margin
    else if (framebuf.height > progress_h + 4)
        framebuf.height - progress_h - 4
    else
        0;
    const status_scale: u64 = if (progress_w >= 384 and framebuf.height >= 360) 2 else 1;
    const status_h = @as(u64, font.GLYPH_H) * status_scale * 2 + STATUS_ROW_GAP;
    const required_after_progress = STATUS_TOP_GAP + status_h + STATUS_BOTTOM_GAP;
    const latest_progress_y = if (framebuf.height > progress_h + required_after_progress)
        framebuf.height - progress_h - required_after_progress
    else
        0;
    const progress_y = @min(default_progress_y, latest_progress_y);
    return .{
        .progress_x = progress_x,
        .progress_y = progress_y,
        .progress_w = progress_w,
        .progress_h = progress_h,
        .status_x = progress_x,
        .status_y = progress_y + progress_h + STATUS_TOP_GAP,
        .status_w = progress_w,
        .status_scale = status_scale,
    };
}

fn redrawStatusRows() void {
    const framebuf = active_framebuffer orelse return;
    if (!fb.supportsRgb32(framebuf)) return;
    const secondary = if (current_error.len != 0) current_error.slice() else current_detail.slice();
    const secondary_color = if (current_error.len != 0) ERROR_FOREGROUND else DETAIL_FOREGROUND;
    drawStatusRows(framebuf, active_geometry, current_status.slice(), secondary, secondary_color);
}

fn setPrefixedStatus(prefix: []const u8, value: []const u8) void {
    var line: [MAX_STATUS_BYTES]u8 = .{0} ** MAX_STATUS_BYTES;
    const len = copyPrefixedLine(line[0..], prefix, value);
    setStatus(line[0..len]);
}

fn setPrefixedError(prefix: []const u8, value: []const u8) void {
    if (current_error.len != 0) return;
    var line: [MAX_STATUS_BYTES]u8 = .{0} ** MAX_STATUS_BYTES;
    const len = copyPrefixedLine(line[0..], prefix, value);
    setError(line[0..len]);
}

fn setPrefixedDetail(prefix: []const u8, value: []const u8) void {
    if (current_error.len != 0) return;
    var line: [MAX_STATUS_BYTES]u8 = .{0} ** MAX_STATUS_BYTES;
    const len = copyPrefixedLine(line[0..], prefix, value);
    setDetail(line[0..len]);
}

fn copyPrefixedLine(line: []u8, prefix: []const u8, value: []const u8) usize {
    var len = copyLinePart(line, 0, prefix);
    len = copyLinePart(line, len, value);
    return len;
}

fn copyLinePart(line: []u8, start: usize, value: []const u8) usize {
    var len = @min(start, line.len);
    for (value) |ch| {
        if (len >= line.len) break;
        line[len] = ch;
        len += 1;
    }
    return len;
}

fn drawStatusRows(framebuf: *fb.Framebuffer, geometry: Geometry, status: []const u8, secondary: []const u8, secondary_color: u32) void {
    if (geometry.status_w == 0 or geometry.status_x >= framebuf.width or geometry.status_y >= framebuf.height) return;
    const scale = if (geometry.status_scale == 0) 1 else geometry.status_scale;
    const row_h = @as(u64, font.GLYPH_H) * scale;
    const second_y = geometry.status_y + row_h + STATUS_ROW_GAP;
    if (row_h == 0 or second_y >= framebuf.height) return;

    fb.rect(framebuf, geometry.status_x, geometry.status_y, geometry.status_w, row_h, STATUS_BACKGROUND);
    fb.rect(framebuf, geometry.status_x, second_y, geometry.status_w, row_h, STATUS_BACKGROUND);
    drawStatusLine(framebuf, geometry, geometry.status_y, status, STATUS_FOREGROUND);
    drawStatusLine(framebuf, geometry, second_y, secondary, secondary_color);
}

fn drawStatusLine(framebuf: *fb.Framebuffer, geometry: Geometry, y: u64, text: []const u8, color: u32) void {
    if (geometry.status_w <= STATUS_TEXT_PADDING * 2) return;
    const scale = if (geometry.status_scale == 0) 1 else geometry.status_scale;
    const cell_w = @as(u64, font.GLYPH_W) * scale;
    const available_w = geometry.status_w - STATUS_TEXT_PADDING * 2;
    const max_chars: usize = @intCast(available_w / cell_w);
    const count = @min(text.len, max_chars);
    var index: usize = 0;
    while (index < count) : (index += 1) {
        drawGlyph(
            framebuf,
            geometry.status_x + STATUS_TEXT_PADDING + @as(u64, @intCast(index)) * cell_w,
            y,
            text[index],
            scale,
            color,
        );
    }
}

fn drawGlyph(framebuf: *fb.Framebuffer, x: u64, y: u64, ch: u8, scale: u64, color: u32) void {
    const glyph = font.glyph(if (ch >= 0x20 and ch <= 0x7E) ch else '?');
    var row: u64 = 0;
    while (row < font.GLYPH_H) : (row += 1) {
        const bits = glyph[@intCast(row)];
        var col: u64 = 0;
        while (col < font.GLYPH_W) : (col += 1) {
            const shift: u3 = @intCast(font.GLYPH_W - 1 - col);
            if (((bits >> shift) & 1) == 0) continue;
            fb.rect(framebuf, x + col * scale, y + row * scale, scale, scale, color);
        }
    }
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

test "boot status geometry keeps both rows below progress" {
    const testing = @import("std").testing;
    var storage: [4]u8 align(4) = .{0} ** 4;

    var large = testFramebuffer(storage[0..].ptr, 1280, 720, 1280 * 4);
    const large_geometry = progressGeometry(&large);
    try testing.expectEqual(@as(u64, 2), large_geometry.status_scale);
    try testing.expect(large_geometry.status_y >= large_geometry.progress_y + large_geometry.progress_h);
    try testing.expect(statusBottom(large_geometry) <= large.height);

    var small = testFramebuffer(storage[0..].ptr, 320, 200, 320 * 4);
    const small_geometry = progressGeometry(&small);
    try testing.expectEqual(@as(u64, 1), small_geometry.status_scale);
    try testing.expect(small_geometry.status_y >= small_geometry.progress_y + small_geometry.progress_h);
    try testing.expect(statusBottom(small_geometry) <= small.height);
}

test "boot status text is bounded and single line" {
    const testing = @import("std").testing;
    var line: LineText = .{};
    line.set("Treiber:\tRTL8168\n");
    try testing.expectEqualStrings("Treiber: RTL8168", line.slice());

    line.set("abcdefghijklmnopqrstuvwxyz-0123456789-more");
    try testing.expectEqual(@as(usize, MAX_STATUS_BYTES), line.len);
    try testing.expectEqualStrings("abcdefghijklmnopqrstuvwxyz-01234", line.slice());
}

test "boot status redraw clears stale text and empty error row" {
    const testing = @import("std").testing;
    const width: u64 = 320;
    const height: u64 = 200;
    const pitch: u64 = width * 4;
    const storage = try testing.allocator.alloc(u8, @intCast(pitch * height));
    defer testing.allocator.free(storage);
    @memset(storage, 0xA5);

    var frame = testFramebuffer(storage.ptr, width, height, pitch);
    const geometry = progressGeometry(&frame);
    drawStatusRows(&frame, geometry, "Treiber: sehr lang", "Fehler: alt", ERROR_FOREGROUND);
    drawStatusRows(&frame, geometry, "OK", "", DETAIL_FOREGROUND);

    const row_h = @as(u64, font.GLYPH_H) * geometry.status_scale;
    const second_y = geometry.status_y + row_h + STATUS_ROW_GAP;
    var y = second_y;
    while (y < second_y + row_h) : (y += 1) {
        var x = geometry.status_x;
        while (x < geometry.status_x + geometry.status_w) : (x += 1) {
            try testing.expectEqual(STATUS_BACKGROUND, fb.readPacked32(&frame, x, y));
        }
    }

    const cell_w = @as(u64, font.GLYPH_W) * geometry.status_scale;
    y = geometry.status_y;
    while (y < geometry.status_y + row_h) : (y += 1) {
        var x = geometry.status_x + STATUS_TEXT_PADDING + 2 * cell_w;
        while (x < geometry.status_x + geometry.status_w) : (x += 1) {
            try testing.expectEqual(STATUS_BACKGROUND, fb.readPacked32(&frame, x, y));
        }
    }
    try testing.expectEqual(@as(u32, 0xA5A5A5A5), fb.readPacked32(&frame, 0, 0));
}

test "boot status keeps current step and first actionable error" {
    const testing = @import("std").testing;
    current_status.clear();
    current_detail.clear();
    current_error.clear();
    defer current_status.clear();
    defer current_detail.clear();
    defer current_error.clear();

    setDriver("RTL8168");
    setDriverError("RTL8168");
    setError("later generic error");
    try testing.expectEqualStrings("Treiber: RTL8168", current_status.slice());
    try testing.expectEqualStrings("Fehler: RTL8168", current_error.slice());
}

test "boot status service detail is replaced by first service error" {
    const testing = @import("std").testing;
    current_status.clear();
    current_detail.clear();
    current_error.clear();
    defer current_status.clear();
    defer current_detail.clear();
    defer current_error.clear();

    setStatus("Dienste starten");
    setService("AUDSVC");
    try testing.expectEqualStrings("Dienste starten", current_status.slice());
    try testing.expectEqualStrings("Dienst: AUDSVC", current_detail.slice());
    try testing.expectEqual(@as(usize, 0), current_error.len);

    setServiceStage("UPDSVC", "laden");
    try testing.expectEqualStrings("UPDSVC: laden", current_detail.slice());

    setServiceError("AUDSVC");
    setService("SSHD");
    clearDetail();
    try testing.expectEqual(@as(usize, 0), current_detail.len);
    try testing.expectEqualStrings("Fehler: AUDSVC", current_error.slice());
}

fn statusBottom(geometry: Geometry) u64 {
    const scale = if (geometry.status_scale == 0) 1 else geometry.status_scale;
    return geometry.status_y + @as(u64, font.GLYPH_H) * scale * 2 + STATUS_ROW_GAP;
}

fn testFramebuffer(address: [*]u8, width: u64, height: u64, pitch: u64) fb.Framebuffer {
    return .{
        .address = address,
        .width = width,
        .height = height,
        .pitch = pitch,
        .bpp = 32,
        .memory_model = 1,
        .red_mask_size = 0,
        .red_mask_shift = 0,
        .green_mask_size = 0,
        .green_mask_shift = 0,
        .blue_mask_size = 0,
        .blue_mask_shift = 0,
        .unused = .{0} ** 5,
        .edid_size = 0,
        .edid = null,
    };
}
