// Temporary private C facade; boot renderer state and pixels belong to Rust.
const fb = @import("../display/framebuffer.zig");
const font = @import("font.zig");
const MAX_STATUS_BYTES: usize = 32;
const STATUS_BACKGROUND: u32 = 0x041318;
const DETAIL_FOREGROUND: u32 = 0xB8D5D8;
const ERROR_FOREGROUND: u32 = 0xFF8A80;
const STATUS_TEXT_PADDING: u64 = 4;
const STATUS_ROW_GAP: u64 = 3;
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

const Geometry = extern struct {
    progress_x: u64 = 0,
    progress_y: u64 = 0,
    progress_w: u64 = 0,
    progress_h: u64 = 0,
    status_x: u64 = 0,
    status_y: u64 = 0,
    status_w: u64 = 0,
    status_scale: u64 = 1,
};

const LineText = extern struct {
    bytes: [MAX_STATUS_BYTES]u8 = .{0} ** MAX_STATUS_BYTES,
    len: usize = 0,
    fn clear(self: *LineText) void {
        r4os_rust_boot_screen_line(self, "".ptr, 0);
    }
    fn set(self: *LineText, text: []const u8) void {
        r4os_rust_boot_screen_line(self, text.ptr, text.len);
    }
    fn slice(self: *const LineText) []const u8 {
        return self.bytes[0..self.len];
    }
};
extern var r4os_rust_boot_screen_current_status: LineText;
extern var r4os_rust_boot_screen_current_detail: LineText;
extern var r4os_rust_boot_screen_current_error: LineText;
const current_status = &r4os_rust_boot_screen_current_status;
const current_detail = &r4os_rust_boot_screen_current_detail;
const current_error = &r4os_rust_boot_screen_current_error;
extern fn r4os_rust_boot_screen_line(*LineText, [*]const u8, usize) void;
extern fn r4os_rust_boot_screen_text(u32, [*]const u8, usize, [*]const u8, usize) void;
extern fn r4os_rust_boot_screen_render(*fb.Framebuffer) u8;
extern fn r4os_rust_boot_screen_prepared(*const fb.Framebuffer, [*]const u8, usize) bool;
extern fn r4os_rust_boot_screen_phase(u8) void;
extern fn r4os_rust_boot_screen_active() bool;
extern fn r4os_rust_boot_screen_clear_detail() void;
extern fn r4os_rust_boot_screen_complete() void;
extern fn r4os_rust_boot_screen_geometry(*const fb.Framebuffer, *Geometry) void;
extern fn r4os_rust_boot_screen_rows(*const fb.Framebuffer, *const Geometry, [*]const u8, usize, [*]const u8, usize, u32) void;
extern fn r4os_rust_boot_screen_bottom(*const Geometry) u64;
pub fn renderToFramebuffer(f: *fb.Framebuffer) RenderResult {
    return @enumFromInt(r4os_rust_boot_screen_render(f));
}
pub fn renderPreparedR4B(f: *fb.Framebuffer, bytes: []const u8) bool {
    return r4os_rust_boot_screen_prepared(f, bytes.ptr, bytes.len);
}
pub fn setPhase(phase: Phase) void {
    r4os_rust_boot_screen_phase(@intFromEnum(phase));
}
pub fn isActive() bool {
    return r4os_rust_boot_screen_active();
}
pub fn clearDetail() void {
    r4os_rust_boot_screen_clear_detail();
}
pub fn completeForHandoff() void {
    r4os_rust_boot_screen_complete();
}
pub fn setServiceStage(name: []const u8, stage: []const u8) void {
    r4os_rust_boot_screen_text(7, name.ptr, name.len, stage.ptr, stage.len);
}
fn progressGeometry(f: *const fb.Framebuffer) Geometry {
    var out: Geometry = undefined;
    r4os_rust_boot_screen_geometry(f, &out);
    return out;
}
fn drawStatusRows(f: *fb.Framebuffer, g: Geometry, status: []const u8, secondary: []const u8, color: u32) void {
    r4os_rust_boot_screen_rows(f, &g, status.ptr, status.len, secondary.ptr, secondary.len, color);
}
pub fn setStatus(text: []const u8) void {
    r4os_rust_boot_screen_text(0, text.ptr, text.len, "".ptr, 0);
}
pub fn setError(text: []const u8) void {
    r4os_rust_boot_screen_text(1, text.ptr, text.len, "".ptr, 0);
}
pub fn setDetail(text: []const u8) void {
    r4os_rust_boot_screen_text(2, text.ptr, text.len, "".ptr, 0);
}
pub fn setDriver(text: []const u8) void {
    r4os_rust_boot_screen_text(3, text.ptr, text.len, "".ptr, 0);
}
pub fn setDriverError(text: []const u8) void {
    r4os_rust_boot_screen_text(4, text.ptr, text.len, "".ptr, 0);
}
pub fn setService(text: []const u8) void {
    r4os_rust_boot_screen_text(5, text.ptr, text.len, "".ptr, 0);
}
pub fn setServiceError(text: []const u8) void {
    r4os_rust_boot_screen_text(6, text.ptr, text.len, "".ptr, 0);
}
comptime {
    if (@sizeOf(Geometry) != 64 or @alignOf(Geometry) != 8 or @sizeOf(LineText) != 40 or @alignOf(LineText) != 8 or @offsetOf(LineText, "bytes") != 0 or @offsetOf(LineText, "len") != 32) @compileError("Native boot-screen C layout");
    if (@offsetOf(Geometry, "progress_x") != 0) @compileError("Native boot geometry offset");
    if (@offsetOf(Geometry, "progress_y") != 8) @compileError("Native boot geometry offset");
    if (@offsetOf(Geometry, "progress_w") != 16) @compileError("Native boot geometry offset");
    if (@offsetOf(Geometry, "progress_h") != 24) @compileError("Native boot geometry offset");
    if (@offsetOf(Geometry, "status_x") != 32) @compileError("Native boot geometry offset");
    if (@offsetOf(Geometry, "status_y") != 40) @compileError("Native boot geometry offset");
    if (@offsetOf(Geometry, "status_w") != 48) @compileError("Native boot geometry offset");
    if (@offsetOf(Geometry, "status_scale") != 56) @compileError("Native boot geometry offset");
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
    return r4os_rust_boot_screen_bottom(&geometry);
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
