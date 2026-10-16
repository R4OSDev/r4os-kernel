//! Native canonical target/frame state and atomic IRQ bounds projection.
const display = @import("display.zig");
pub const Target = extern struct { width: u64 = 0, height: u64 = 0, pitch: u64 = 0, bpp: u16 = 0 };
pub const PixelBounds = struct { width: u32, height: u32 };
extern fn r4os_rust_surface_pipeline(u32, *const Target, u32, u32) u64;
pub fn pixelBounds() PixelBounds { const value = r4os_rust_surface_pipeline(7, &.{}, 0, 0); return .{ .width = @truncate(value), .height = @truncate(value >> 32) }; }
pub fn initFromDisplayManager() void {
    const mode = display.activeMode() orelse { initTarget(.{}); return; };
    initTarget(.{ .width = mode.width, .height = mode.height, .pitch = mode.pitch, .bpp = mode.bpp });
}
pub fn initTarget(target: Target) void { _ = r4os_rust_surface_pipeline(0, &target, 0, 0); }
pub fn publishTargetLocked(target: Target) void { _ = r4os_rust_surface_pipeline(1, &target, 0, 0); }
pub fn width() u32 { return @intCast(r4os_rust_surface_pipeline(2, &.{}, 0, 0)); }
pub fn height() u32 { return @intCast(r4os_rust_surface_pipeline(3, &.{}, 0, 0)); }
pub fn beginFrame() i32 { return @intCast(r4os_rust_surface_pipeline(4, &.{}, 0, 0)); }
pub fn beginFrameRect(x: i32, y: i32, w: u32, h: u32) i32 { _ = x; _ = y; return @intCast(r4os_rust_surface_pipeline(5, &.{}, w, h)); }
pub fn present() i32 { return @intCast(r4os_rust_surface_pipeline(6, &.{}, 0, 0)); }
comptime { if (@sizeOf(Target) != 32 or @offsetOf(Target, "bpp") != 24) @compileError("Native frame target C layout"); }
