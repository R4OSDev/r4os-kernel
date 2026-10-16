const boot_info = @import("../bootloader/boot_info.zig");
const log = @import("../kernel/log.zig");
const bootscreen = @import("../kernel/bootscreen.zig");
const fb = @import("../display/framebuffer.zig");
const Console = @import("console.zig").Console;
const scroll_buffer = @import("console_scroll_buffer.zig");
const heap = @import("../memory/heap.zig");
const display = @import("../display/display.zig");
const surface_pipeline = @import("../display/surface_pipeline.zig");
const firmware_access = @import("firmware_access.zig");

pub const ConsoleMetrics = extern struct {
    cols: u32 = 0,
    rows: u32 = 0,
    scale: u32 = 0,
};

pub const State = extern struct {
    console_metrics: ConsoleMetrics,
};

pub fn init() ?State {
    var out: State = undefined;
    return if (r4os_rust_boot_display_init(&out)) out else null;
}

pub fn get() ?State {
    var out: State = undefined;
    return if (r4os_rust_boot_display_get(&out)) out else null;
}

// Called once after heap initialization, outside the boot-log owner. Storage
// stays resident for later console/fatal use; sink callbacks never allocate.
pub fn prepareConsolePixelBacking(memory_budget: u64) bool {
    return r4os_rust_boot_display_prepare(memory_budget);
}

pub fn invalidateConsoleBackingForExternalDisplay() void {
    r4os_rust_boot_display_invalidate();
}

fn consoleSink(console: *Console) log.ConsoleSink {
    return .{
        .context = console,
        .putc = consoleSinkPutc,
        .puts = consoleSinkPuts,
        .clear = consoleSinkClear,
        .clearFramed = consoleSinkClearFramed,
        .setMargins = consoleSinkSetMargins,
        .setColors = consoleSinkSetColors,
        .setFontScale = consoleSinkSetFontScale,
        .setCursor = consoleSinkSetCursor,
        .cols = consoleSinkCols,
        .rows = consoleSinkRows,
    };
}

fn sinkConsole(context: *anyopaque) *Console {
    return @ptrCast(@alignCast(context));
}

fn consoleSinkPutc(context: *anyopaque, ch: u8) void {
    const lease = firmware_access.acquire() orelse return;
    defer lease.release();
    sinkConsole(context).putc(ch);
}

fn consoleSinkPuts(context: *anyopaque, s: []const u8) void {
    const lease = firmware_access.acquire() orelse return;
    defer lease.release();
    sinkConsole(context).puts(s);
}

fn consoleSinkClear(context: *anyopaque) void {
    const lease = firmware_access.acquire() orelse return;
    defer lease.release();
    sinkConsole(context).clear();
}

fn consoleSinkClearFramed(context: *anyopaque, border: u32, inner: u32) void {
    const lease = firmware_access.acquire() orelse return;
    defer lease.release();
    sinkConsole(context).clearFramed(border, inner);
}

fn consoleSinkSetMargins(context: *anyopaque, left: u32, top: u32, right: u32, bottom: u32) void {
    const lease = firmware_access.acquire() orelse return;
    defer lease.release();
    sinkConsole(context).setMargins(left, top, right, bottom);
}

fn consoleSinkSetColors(context: *anyopaque, fg: u32, bg: u32) void {
    const lease = firmware_access.acquire() orelse return;
    defer lease.release();
    sinkConsole(context).setColors(fg, bg);
}

fn consoleSinkSetFontScale(context: *anyopaque, scale: u32) void {
    const lease = firmware_access.acquire() orelse return;
    defer lease.release();
    sinkConsole(context).setFontScale(scale);
}

fn consoleSinkSetCursor(context: *anyopaque, x: u32, y: u32) void {
    const lease = firmware_access.acquire() orelse return;
    defer lease.release();
    sinkConsole(context).setCursor(x, y);
}

fn consoleSinkCols(context: *anyopaque) u32 {
    return sinkConsole(context).textCols();
}

fn consoleSinkRows(context: *anyopaque) u32 {
    return sinkConsole(context).textRows();
}

extern fn r4os_rust_boot_display_init(*State) bool;
extern fn r4os_rust_boot_display_get(*State) bool;
extern fn r4os_rust_boot_display_prepare(u64) bool;
extern fn r4os_rust_boot_display_invalidate() void;

export fn r4os_zig_boot_display_sink(console: *Console) callconv(.c) void {
    log.setConsoleSink(consoleSink(console));
}
comptime {
    if (@sizeOf(ConsoleMetrics) != 12 or @alignOf(ConsoleMetrics) != 4 or @offsetOf(ConsoleMetrics, "rows") != 4 or @offsetOf(ConsoleMetrics, "scale") != 8 or @sizeOf(State) != 12 or @alignOf(State) != 4 or @offsetOf(State, "console_metrics") != 0) @compileError("Native boot-display snapshot C layout");
}
