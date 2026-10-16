//! Additional native output lifetimes; metadata only. Hardware setup and
//! scanout storage belong to R4D, topology policy to the desktop/R4GFX.
const std = @import("std");
pub const abi = @import("r4os_kernel_contract");
const identity = @import("output_target.zig");
const lifetime = @import("../memory/gfx_buffer_owner.zig");
const stats = @import("presentation_stats.zig");
const backend = @import("backend_state.zig");
const queue = @import("queue_state.zig");
pub const capacity = abi.gfx_output_max_assignments;
pub const Error = stats.Error || error{ Busy, Exhausted };
pub const Entry = extern struct {
    driver: lifetime.Owner = .{ .kind = .driver, .id = 0, .generation = 0 }, target: abi.GfxOutputTarget = .{}, binding: abi.GfxBackendBinding = .{},
    width: u32 = 0, height: u32 = 0, active: bool = false, mode_blocked: bool = false, mode_lost: bool = false, removing: bool = false,
    has_pending: bool = false, pending_data: queue.Fence = .{}, direct: bool = false, statistics: stats.Owner = .{},
    pub fn pendingValue(self: *const Entry) ?queue.Fence { return if (self.has_pending) self.pending_data else null; }
    pub fn setPending(self: *Entry, value: ?queue.Fence) void { self.has_pending = value != null; if (value) |fence| self.pending_data = fence; }
    pub fn snapshot(self: *const Entry) backend.Snapshot { var out: backend.Snapshot = undefined; status(r4os_rust_display_additional_entry(0, self, &self.driver, null, &out)) catch unreachable; return out; }
    pub fn info(self: *const Entry) Error!abi.DisplayPresentationInfo { var out: abi.DisplayPresentationInfo = undefined; try status(r4os_rust_display_additional_entry(1, self, &self.driver, null, &out)); return out; }
    pub fn publishInfo(self: *Entry, driver: lifetime.Owner, value: abi.DisplayPresentationInfo) Error!void { try status(r4os_rust_display_additional_entry(2, self, &driver, &value, null)); }
    pub fn publishStats(self: *Entry, driver: lifetime.Owner, value: abi.DisplayPresentationStats) Error!void { try status(r4os_rust_display_additional_entry(3, self, &driver, &value, null)); }
};
pub const Store = extern struct {
    entries: [capacity]Entry = @splat(.{}), serial: u64 = @as(u64, 1) << 63,
    pub fn register(self: *Store, driver: lifetime.Owner, request: abi.GfxAdditionalOutput) Error!abi.GfxOutputTarget {
        var out: abi.GfxOutputTarget = undefined; try status(r4os_rust_display_additional_register(self, &driver, &request, &out)); return out;
    }
    pub fn find(self: *Store, target: abi.GfxOutputTarget) Error!*Entry { var index: usize = undefined; try status(r4os_rust_display_additional_find(self, &target, &index)); return &self.entries[index]; }
    pub fn at(self: *Store, adapter: u32, head: u32) ?*Entry { var index: usize = undefined; return if (r4os_rust_display_additional_at(self, adapter, head, &index)) &self.entries[index] else null; }
    pub fn stop(self: *Store, driver: u32) void { r4os_rust_display_additional_stop(self, driver); }
};
fn status(code: i32) Error!void { return switch (code) { 0 => {}, 1 => error.Invalid, 3 => error.Unsupported, 4 => error.Exhausted, 6 => error.Capacity, 7 => error.Stale, 8 => error.Busy, else => unreachable }; }
extern fn r4os_rust_display_additional_entry(u32, *const Entry, *const lifetime.Owner, ?*const anyopaque, ?*anyopaque) i32;
extern fn r4os_rust_display_additional_register(*Store, *const lifetime.Owner, *const abi.GfxAdditionalOutput, *abi.GfxOutputTarget) i32;
extern fn r4os_rust_display_additional_find(*const Store, *const abi.GfxOutputTarget, *usize) i32;
extern fn r4os_rust_display_additional_at(*const Store, u32, u32, *usize) bool;
extern fn r4os_rust_display_additional_stop(*Store, u32) void;
comptime {
    if (@sizeOf(Entry) != 9904 or @alignOf(Entry) != 8 or @offsetOf(Entry, "pending_data") != 120 or @offsetOf(Entry, "direct") != 168 or @offsetOf(Entry, "statistics") != 176 or @sizeOf(Store) != 79240 or @offsetOf(Store, "serial") != 79232) @compileError("Native additional output C layout");
}
