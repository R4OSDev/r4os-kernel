const boot_info = @import("../bootloader/boot_info.zig");
const log = @import("../kernel/log.zig");
const bootscreen = @import("../kernel/bootscreen.zig");
const fb = @import("../display/framebuffer.zig");
const Console = @import("console.zig").Console;
const display = @import("../display/display.zig");
const surface_pipeline = @import("../display/surface_pipeline.zig");

pub const ConsoleMetrics = struct {
    cols: u32 = 0,
    rows: u32 = 0,
    scale: u32 = 0,
};

pub const State = struct {
    console_metrics: ConsoleMetrics,
};

var framebuffer_storage: fb.Framebuffer = undefined;
var console_storage: Console = undefined;
var state_storage: ?State = null;

pub fn init() ?State {
    if (state_storage) |state| return state;

    const boot_framebuf = boot_info.framebuffer() orelse return null;
    framebuffer_storage = framebufferFromBootInfo(boot_framebuf);
    const framebuf = &framebuffer_storage;
    console_storage = Console.init(framebuf);
    const boot_target = displayTargetFromBootFramebuffer(framebuf);
    display.registerBootBackend(boot_target);
    surface_pipeline.initFromDisplayManager();
    console_storage.clear();
    _ = bootscreen.renderToFramebuffer(framebuf);
    console_storage.invalidateBackingForExternalDisplay();
    log.setConsoleSink(consoleSink(&console_storage));

    state_storage = .{ .console_metrics = consoleMetrics(&console_storage) };
    return state_storage;
}

pub fn get() ?State {
    return state_storage;
}

pub fn invalidateConsoleBackingForExternalDisplay() void {
    if (state_storage == null) return;
    console_storage.invalidateBackingForExternalDisplay();
}

fn framebufferFromBootInfo(src: *const boot_info.Framebuffer) fb.Framebuffer {
    return .{
        .address = src.address,
        .width = src.width,
        .height = src.height,
        .pitch = src.pitch,
        .bpp = src.bpp,
        .memory_model = src.memory_model,
        .red_mask_size = src.red_mask_size,
        .red_mask_shift = src.red_mask_shift,
        .green_mask_size = src.green_mask_size,
        .green_mask_shift = src.green_mask_shift,
        .blue_mask_size = src.blue_mask_size,
        .blue_mask_shift = src.blue_mask_shift,
        .unused = src.unused,
        .edid_size = src.edid_size,
        .edid = src.edid,
    };
}

fn displayTargetFromBootFramebuffer(framebuf: *fb.Framebuffer) display.DisplayTarget {
    return .{
        .name = "bootfb",
        .kind = .bootfb,
        .flags = displayFlagsFromFramebuffer(framebuf),
        .mode = displayModeFromFramebuffer(framebuf),
        .mapping = displayMappingFromFramebuffer(framebuf),
        .framebuffer = framebuf,
    };
}

fn displayFlagsFromFramebuffer(framebuf: *const fb.Framebuffer) u32 {
    var flags: u32 = display.DeviceFlags.visible |
        display.DeviceFlags.fixed_mode |
        display.DeviceFlags.cpu_present |
        display.DeviceFlags.fill |
        display.DeviceFlags.rect;
    if (fb.supportsRgb32(framebuf)) {
        flags |= display.DeviceFlags.rgb32 |
            display.DeviceFlags.packed32 |
            display.DeviceFlags.xrgb32;
    }
    return flags;
}

fn displayModeFromFramebuffer(framebuf: *const fb.Framebuffer) display.Mode {
    return .{
        .width = @intCast(framebuf.width),
        .height = @intCast(framebuf.height),
        .pitch = @intCast(framebuf.pitch),
        .bpp = framebuf.bpp,
        .memory_model = framebuf.memory_model,
        .red_mask_size = framebuf.red_mask_size,
        .red_mask_shift = framebuf.red_mask_shift,
        .green_mask_size = framebuf.green_mask_size,
        .green_mask_shift = framebuf.green_mask_shift,
        .blue_mask_size = framebuf.blue_mask_size,
        .blue_mask_shift = framebuf.blue_mask_shift,
    };
}

fn displayMappingFromFramebuffer(framebuf: *const fb.Framebuffer) display.Mapping {
    return .{
        .kind = .bootloader_framebuffer,
        .cache_policy = .bootloader_default,
        .virt_base = @intFromPtr(framebuf.address),
        .byte_len = framebuf.pitch * framebuf.height,
        .volatile_cpu_writes = true,
    };
}

fn consoleMetrics(console: *const Console) ConsoleMetrics {
    return .{
        .cols = console.cols,
        .rows = console.rows,
        .scale = console.scale,
    };
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
    sinkConsole(context).putc(ch);
}

fn consoleSinkPuts(context: *anyopaque, s: []const u8) void {
    sinkConsole(context).puts(s);
}

fn consoleSinkClear(context: *anyopaque) void {
    sinkConsole(context).clear();
}

fn consoleSinkClearFramed(context: *anyopaque, border: u32, inner: u32) void {
    sinkConsole(context).clearFramed(border, inner);
}

fn consoleSinkSetMargins(context: *anyopaque, left: u32, top: u32, right: u32, bottom: u32) void {
    sinkConsole(context).setMargins(left, top, right, bottom);
}

fn consoleSinkSetColors(context: *anyopaque, fg: u32, bg: u32) void {
    sinkConsole(context).setColors(fg, bg);
}

fn consoleSinkSetFontScale(context: *anyopaque, scale: u32) void {
    sinkConsole(context).setFontScale(scale);
}

fn consoleSinkSetCursor(context: *anyopaque, x: u32, y: u32) void {
    sinkConsole(context).setCursor(x, y);
}

fn consoleSinkCols(context: *anyopaque) u32 {
    return sinkConsole(context).textCols();
}

fn consoleSinkRows(context: *anyopaque) u32 {
    return sinkConsole(context).textRows();
}
