const fb = @import("framebuffer.zig");
const cpu = @import("../platform/cpu.zig");
const paging = @import("../memory/paging.zig");
const timer = @import("../kernel/timer.zig");

pub const DeviceKind = enum(u8) {
    none = 0,
    bootfb = 1,
};

pub const DeviceFlags = struct {
    pub const visible: u32 = 1 << 0;
    pub const fixed_mode: u32 = 1 << 1;
    pub const cpu_present: u32 = 1 << 2;
    pub const rgb32: u32 = 1 << 3;
    pub const fill: u32 = 1 << 4;
    pub const rect: u32 = 1 << 5;
    pub const packed32: u32 = 1 << 6;
    pub const xrgb32: u32 = 1 << 7;
};

pub const Mode = struct {
    width: u32 = 0,
    height: u32 = 0,
    pitch: u32 = 0,
    bpp: u16 = 0,
    memory_model: u8 = 0,
    red_mask_size: u8 = 0,
    red_mask_shift: u8 = 0,
    green_mask_size: u8 = 0,
    green_mask_shift: u8 = 0,
    blue_mask_size: u8 = 0,
    blue_mask_shift: u8 = 0,
};

pub const Rect = struct {
    x: u32 = 0,
    y: u32 = 0,
    w: u32 = 0,
    h: u32 = 0,
};

pub const MappingKind = enum(u8) {
    none = 0,
    bootloader_framebuffer = 1,
};

pub const CachePolicy = enum(u8) {
    unknown = 0,
    bootloader_default = 1,
    pat_write_combining = 2,
    write_combining_unsupported = 3,
    write_combining_failed = 4,
};

pub const Mapping = struct {
    kind: MappingKind = .none,
    cache_policy: CachePolicy = .unknown,
    virt_base: u64 = 0,
    byte_len: u64 = 0,
    volatile_cpu_writes: bool = false,
};

pub const PresentReason = enum(u8) {
    none = 0,
    fill = 1,
    rect = 2,
    packed32_present = 3,
    xrgb32_present = 4,
};

pub const DeviceOps = struct {
    fill: ?*const fn (device: *Device, rgb: u32) bool = null,
    rect: ?*const fn (device: *Device, x: i32, y: i32, w: u32, h: u32, rgb: u32) bool = null,
    present_packed32_rect: ?*const fn (device: *Device, x0: u64, y0: u64, w: u64, h: u64, src: []const u8, src_stride_pixels: u64) bool = null,
    present_xrgb32_rect: ?*const fn (device: *Device, x0: u64, y0: u64, w: u64, h: u64, src: []const u8, src_stride_pixels: u64) bool = null,
    put_packed32: ?*const fn (device: *Device, x: u64, y: u64, color32: u32) bool = null,
    put_xrgb32: ?*const fn (device: *Device, x: u64, y: u64, rgb: u32) bool = null,
};

pub const DisplayTarget = struct {
    name: []const u8 = "none",
    kind: DeviceKind = .none,
    flags: u32 = 0,
    mode: Mode = .{},
    mapping: Mapping = .{},
    // Low-level backend handle only. Normal display consumers must use the
    // DisplayManager boundary: mode, mapping, stats and present operations.
    framebuffer: ?*fb.Framebuffer = null,
};

pub const Device = struct {
    name: []const u8 = "none",
    kind: DeviceKind = .none,
    flags: u32 = 0,
    mode: Mode = .{},
    mapping: Mapping = .{},
    framebuffer: ?*fb.Framebuffer = null,
    ops: *const DeviceOps = &empty_ops,
    present_count: u64 = 0,
    present_pixels_total: u64 = 0,
    present_bytes_total: u64 = 0,
    last_present_pixels: u64 = 0,
    last_present_bytes: u64 = 0,
    last_present_rect: Rect = .{},
    last_present_reason: PresentReason = .none,
    last_present_converted: bool = false,
    full_present_count: u64 = 0,
    partial_present_count: u64 = 0,
    fill_present_count: u64 = 0,
    rect_present_count: u64 = 0,
    packed32_present_count: u64 = 0,
    xrgb32_present_count: u64 = 0,
    conversion_present_count: u64 = 0,
    present_total_ticks: u64 = 0,
    present_max_ticks: u64 = 0,
    present_last_ticks: u64 = 0,
    present_slow_count: u64 = 0,
};

pub const Stats = struct {
    registered: bool = false,
    name: []const u8 = "none",
    kind: DeviceKind = .none,
    flags: u32 = 0,
    mode: Mode = .{},
    mapping: Mapping = .{},
    present_count: u64 = 0,
    present_pixels_total: u64 = 0,
    present_bytes_total: u64 = 0,
    last_present_pixels: u64 = 0,
    last_present_bytes: u64 = 0,
    last_present_rect: Rect = .{},
    last_present_reason: PresentReason = .none,
    last_present_converted: bool = false,
    full_present_count: u64 = 0,
    partial_present_count: u64 = 0,
    fill_present_count: u64 = 0,
    rect_present_count: u64 = 0,
    packed32_present_count: u64 = 0,
    xrgb32_present_count: u64 = 0,
    conversion_present_count: u64 = 0,
    present_total_ticks: u64 = 0,
    present_max_ticks: u64 = 0,
    present_last_ticks: u64 = 0,
    present_slow_count: u64 = 0,
};

const empty_ops: DeviceOps = .{};
const bootfb_ops: DeviceOps = .{
    .fill = bootfbFill,
    .rect = bootfbRect,
    .present_packed32_rect = bootfbPresentPacked32Rect,
    .present_xrgb32_rect = bootfbPresentXrgb32Rect,
    .put_packed32 = bootfbPutPacked32,
    .put_xrgb32 = bootfbPutXrgb32,
};

var bootfb_device: Device = .{ .ops = &bootfb_ops };
var primary_device: ?*Device = null;

pub fn registerBootBackend(target: DisplayTarget) void {
    bootfb_device = .{
        .name = target.name,
        .kind = target.kind,
        .flags = target.flags,
        .mode = target.mode,
        .mapping = target.mapping,
        .framebuffer = target.framebuffer,
        .ops = &bootfb_ops,
    };
    primary_device = &bootfb_device;
}

pub fn activeBackendRegistered() bool {
    return primary_device != null;
}

pub fn activeBackendName() []const u8 {
    const device = primary_device orelse return "none";
    return device.name;
}

pub fn activeBackendKind() DeviceKind {
    const device = primary_device orelse return .none;
    return device.kind;
}

pub fn activeMode() ?Mode {
    const device = primary_device orelse return null;
    return device.mode;
}

pub fn activeMapping() ?Mapping {
    const device = primary_device orelse return null;
    return device.mapping;
}

pub fn activeFramebufferForLegacy() ?*fb.Framebuffer {
    const device = primary_device orelse return null;
    return device.framebuffer;
}

pub fn enableFramebufferWriteCombining() bool {
    const device = primary_device orelse return false;
    if (!cpu.writeCombiningBasisAvailable()) {
        device.mapping.cache_policy = .write_combining_unsupported;
        return false;
    }
    if (device.mapping.virt_base == 0 or device.mapping.byte_len == 0) {
        device.mapping.cache_policy = .write_combining_failed;
        return false;
    }
    if (!paging.setWriteCombiningRange(device.mapping.virt_base, device.mapping.byte_len)) {
        device.mapping.cache_policy = .write_combining_failed;
        return false;
    }
    device.mapping.cache_policy = .pat_write_combining;
    return true;
}

pub fn framebuffer() ?*fb.Framebuffer {
    return activeFramebufferForLegacy();
}

pub fn stats() Stats {
    const device = primary_device orelse return .{};
    return .{
        .registered = true,
        .name = device.name,
        .kind = device.kind,
        .flags = device.flags,
        .mode = device.mode,
        .mapping = device.mapping,
        .present_count = device.present_count,
        .present_pixels_total = device.present_pixels_total,
        .present_bytes_total = device.present_bytes_total,
        .last_present_pixels = device.last_present_pixels,
        .last_present_bytes = device.last_present_bytes,
        .last_present_rect = device.last_present_rect,
        .last_present_reason = device.last_present_reason,
        .last_present_converted = device.last_present_converted,
        .full_present_count = device.full_present_count,
        .partial_present_count = device.partial_present_count,
        .fill_present_count = device.fill_present_count,
        .rect_present_count = device.rect_present_count,
        .packed32_present_count = device.packed32_present_count,
        .xrgb32_present_count = device.xrgb32_present_count,
        .conversion_present_count = device.conversion_present_count,
        .present_total_ticks = device.present_total_ticks,
        .present_max_ticks = device.present_max_ticks,
        .present_last_ticks = device.present_last_ticks,
        .present_slow_count = device.present_slow_count,
    };
}

pub fn fill(rgb: u32) bool {
    const device = primary_device orelse return false;
    const op = device.ops.fill orelse return false;
    const start = timer.tickCount();
    const ok = op(device, rgb);
    if (ok) recordPresentTiming(device, start);
    return ok;
}

pub fn rect(x: i32, y: i32, w: u32, h: u32, rgb: u32) bool {
    const device = primary_device orelse return false;
    const op = device.ops.rect orelse return false;
    const start = timer.tickCount();
    const ok = op(device, x, y, w, h, rgb);
    if (ok) recordPresentTiming(device, start);
    return ok;
}

pub fn putPacked32(x: u64, y: u64, color32: u32) bool {
    const device = primary_device orelse return false;
    const op = device.ops.put_packed32 orelse return false;
    return op(device, x, y, color32);
}

pub fn putXrgb32(x: u64, y: u64, rgb: u32) bool {
    const device = primary_device orelse return false;
    const op = device.ops.put_xrgb32 orelse return false;
    return op(device, x, y, rgb);
}

pub fn presentPacked32Rect(x0: u64, y0: u64, w: u64, h: u64, src: []const u8, src_stride_pixels: u64) bool {
    const device = primary_device orelse return false;
    const op = device.ops.present_packed32_rect orelse return false;
    const start = timer.tickCount();
    const ok = op(device, x0, y0, w, h, src, src_stride_pixels);
    if (ok) recordPresentTiming(device, start);
    return ok;
}

pub fn presentXrgb32Rect(x0: u64, y0: u64, w: u64, h: u64, src: []const u8, src_stride_pixels: u64) bool {
    const device = primary_device orelse return false;
    const op = device.ops.present_xrgb32_rect orelse return false;
    const start = timer.tickCount();
    const ok = op(device, x0, y0, w, h, src, src_stride_pixels);
    if (ok) recordPresentTiming(device, start);
    return ok;
}

pub fn operationNames(flags: u32) []const u8 {
    if ((flags & DeviceFlags.xrgb32) != 0) return "present/blit/fill/rect/xrgb32";
    if ((flags & DeviceFlags.packed32) != 0) return "present/blit/fill/rect/packed32";
    if ((flags & DeviceFlags.rect) != 0) return "fill/rect";
    return "none";
}

pub fn presentReasonName(reason: PresentReason) []const u8 {
    return switch (reason) {
        .none => "none",
        .fill => "fill",
        .rect => "rect",
        .packed32_present => "packed32-present",
        .xrgb32_present => "xrgb32-present",
    };
}

pub fn mappingKindName(kind: MappingKind) []const u8 {
    return switch (kind) {
        .none => "none",
        .bootloader_framebuffer => "bootloader-framebuffer",
    };
}

pub fn cachePolicyName(policy: CachePolicy) []const u8 {
    return switch (policy) {
        .unknown => "unknown",
        .bootloader_default => "bootloader-default",
        .pat_write_combining => "pat-write-combining",
        .write_combining_unsupported => "write-combining-unsupported",
        .write_combining_failed => "write-combining-failed",
    };
}

fn bootfbFill(device: *Device, rgb: u32) bool {
    const f = device.framebuffer orelse return false;
    fb.fill(f, rgb);
    recordPresent(device, .fill, 0, 0, @intCast(f.width), @intCast(f.height), false);
    return true;
}

fn bootfbRect(device: *Device, x: i32, y: i32, w: u32, h: u32, rgb: u32) bool {
    const f = device.framebuffer orelse return false;
    if (x < 0 or y < 0) return false;
    const ux: u64 = @intCast(x);
    const uy: u64 = @intCast(y);
    fb.rect(f, ux, uy, w, h, rgb);
    const clipped_w = if (ux >= f.width) 0 else @min(@as(u64, w), f.width - ux);
    const clipped_h = if (uy >= f.height) 0 else @min(@as(u64, h), f.height - uy);
    recordPresent(device, .rect, @intCast(ux), @intCast(uy), @intCast(clipped_w), @intCast(clipped_h), false);
    return true;
}

fn bootfbPutPacked32(device: *Device, x: u64, y: u64, color32: u32) bool {
    const f = device.framebuffer orelse return false;
    fb.putPacked32(f, x, y, color32);
    return true;
}

fn bootfbPutXrgb32(device: *Device, x: u64, y: u64, rgb: u32) bool {
    const f = device.framebuffer orelse return false;
    fb.putPacked32(f, x, y, fb.packRgb(f, rgb));
    return true;
}

fn bootfbPresentPacked32Rect(device: *Device, x0: u64, y0: u64, w: u64, h: u64, src: []const u8, src_stride_pixels: u64) bool {
    return bootfbCopyPacked32Rect(device, .packed32_present, x0, y0, w, h, src, src_stride_pixels);
}

fn bootfbCopyPacked32Rect(device: *Device, reason: PresentReason, x0: u64, y0: u64, w: u64, h: u64, src: []const u8, src_stride_pixels: u64) bool {
    const f = device.framebuffer orelse return false;
    if (!fb.supportsRgb32(f) or w == 0 or h == 0) return false;
    if (x0 >= f.width or y0 >= f.height) return false;

    const clipped_w = @min(w, f.width - x0);
    const clipped_h = @min(h, f.height - y0);
    const src_stride_bytes = src_stride_pixels * 4;
    const row_bytes = clipped_w * 4;
    if (src.len < (clipped_h - 1) * src_stride_bytes + row_bytes) return false;

    var y: u64 = 0;
    while (y < clipped_h) : (y += 1) {
        const src_offset: usize = @intCast(y * src_stride_bytes);
        const dst = f.address + (y0 + y) * f.pitch + x0 * 4;
        copyToVisible(dst, src[src_offset .. src_offset + @as(usize, @intCast(row_bytes))]);
    }
    recordPresent(device, reason, @intCast(x0), @intCast(y0), @intCast(clipped_w), @intCast(clipped_h), false);
    return true;
}

fn bootfbPresentXrgb32Rect(device: *Device, x0: u64, y0: u64, w: u64, h: u64, src: []const u8, src_stride_pixels: u64) bool {
    const f = device.framebuffer orelse return false;
    if (!fb.supportsRgb32(f) or w == 0 or h == 0) return false;
    if (x0 >= f.width or y0 >= f.height) return false;

    const clipped_w = @min(w, f.width - x0);
    const clipped_h = @min(h, f.height - y0);
    const src_stride_bytes = src_stride_pixels * 4;
    const row_bytes = clipped_w * 4;
    if (src.len < (clipped_h - 1) * src_stride_bytes + row_bytes) return false;

    if (fb.isNativeXrgb32(f)) {
        return bootfbCopyPacked32Rect(device, .xrgb32_present, x0, y0, clipped_w, clipped_h, src, src_stride_pixels);
    }

    var y: u64 = 0;
    while (y < clipped_h) : (y += 1) {
        const src_offset: usize = @intCast(y * src_stride_bytes);
        var x: u64 = 0;
        while (x < clipped_w) : (x += 1) {
            const pixel_offset = src_offset + @as(usize, @intCast(x * 4));
            fb.putPacked32(f, x0 + x, y0 + y, fb.packRgb(f, readXrgb32(src, pixel_offset)));
        }
    }
    recordPresent(device, .xrgb32_present, @intCast(x0), @intCast(y0), @intCast(clipped_w), @intCast(clipped_h), true);
    return true;
}

fn recordPresent(device: *Device, reason: PresentReason, x: u32, y: u32, w: u32, h: u32, converted: bool) void {
    const pixels = @as(u64, w) * h;
    const bytes = pixels * 4;
    device.present_count += 1;
    device.present_pixels_total += pixels;
    device.present_bytes_total += bytes;
    device.last_present_pixels = pixels;
    device.last_present_bytes = bytes;
    device.last_present_rect = .{ .x = x, .y = y, .w = w, .h = h };
    device.last_present_reason = reason;
    device.last_present_converted = converted;
    if (converted) device.conversion_present_count += 1;
    if (x == 0 and y == 0 and w == device.mode.width and h == device.mode.height) {
        device.full_present_count += 1;
    } else {
        device.partial_present_count += 1;
    }
    switch (reason) {
        .none => {},
        .fill => device.fill_present_count += 1,
        .rect => device.rect_present_count += 1,
        .packed32_present => device.packed32_present_count += 1,
        .xrgb32_present => device.xrgb32_present_count += 1,
    }
}

fn recordPresentTiming(device: *Device, start: u64) void {
    const end = timer.tickCount();
    const elapsed = if (end >= start) end - start else 0;
    device.present_total_ticks +%= elapsed;
    device.present_last_ticks = elapsed;
    if (elapsed > device.present_max_ticks) device.present_max_ticks = elapsed;
    if (elapsed > 1) device.present_slow_count +%= 1;
}

fn copyToVisible(dst: [*]volatile u8, src: []const u8) void {
    // 0.56.12: Vorab-validierter, subcall-freier u32-Zeilenpfad. Der alte
    // Loop rief readXrgb32 PRO PIXEL (Bounds-Check + Alignment-Check je
    // Wort) - bei Millionen Pixeln pro Frame reiner Overhead. Ist sowohl
    // Ziel ALS AUCH Quelle 4-Byte-ausgerichtet (Normalfall: Framebuffer
    // und Present-Puffer sind seiten-/wortausgerichtet), laeuft eine
    // reine Wort-fuer-Wort-Kopie ganz ohne Pro-Pixel-Check.
    const word_len = src.len & ~@as(usize, 3);
    if ((@intFromPtr(dst) & 3) == 0 and (@intFromPtr(src.ptr) & 3) == 0) {
        const dst_words: [*]volatile u32 = @ptrCast(@alignCast(dst));
        const src_words: [*]const u32 = @ptrCast(@alignCast(src.ptr));
        const count = word_len / 4;
        var wi: usize = 0;
        while (wi < count) : (wi += 1) dst_words[wi] = src_words[wi];
        var i: usize = word_len;
        while (i < src.len) : (i += 1) dst[i] = src[i];
        return;
    }
    // Ziel wortausgerichtet, Quelle nicht: Woerter aus Bytes assemblieren,
    // aber ohne den Pro-Wort-Bounds-Check des alten readXrgb32.
    if ((@intFromPtr(dst) & 3) == 0 and word_len == src.len) {
        const dst_words: [*]volatile u32 = @ptrCast(@alignCast(dst));
        const count = word_len / 4;
        var wi: usize = 0;
        while (wi < count) : (wi += 1) {
            const o = wi * 4;
            dst_words[wi] = @as(u32, src[o]) |
                (@as(u32, src[o + 1]) << 8) |
                (@as(u32, src[o + 2]) << 16) |
                (@as(u32, src[o + 3]) << 24);
        }
        return;
    }
    var i: usize = 0;
    while (i < src.len) : (i += 1) dst[i] = src[i];
}

fn readXrgb32(src: []const u8, offset: usize) u32 {
    if (offset + 3 >= src.len) return 0;
    const ptr = &src[offset];
    if ((@intFromPtr(ptr) & 3) == 0) {
        const word: *const u32 = @ptrCast(@alignCast(ptr));
        return word.*;
    }
    return @as(u32, src[offset + 0]) |
        (@as(u32, src[offset + 1]) << 8) |
        (@as(u32, src[offset + 2]) << 16) |
        (@as(u32, src[offset + 3]) << 24);
}

pub const DisplayManager = struct {
    // Marker type for the current singleton manager. In this transition step the
    // module still owns the active backend internally; callers should treat the
    // exported fill/rect/put/present functions as the DisplayManager boundary.
};
