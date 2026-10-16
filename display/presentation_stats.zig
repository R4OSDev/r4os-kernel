//! Bounded metadata only. The display state owner serializes bind, publish
//! and read; this store performs no callbacks, waits or buffer operations.
const std = @import("std");
const a = @import("r4os_kernel_contract");
const backend_state = @import("backend_state.zig");
pub const Error = error{ Invalid, Stale, Unsupported, Capacity };
pub fn code(err: Error) i32 {
    return switch (err) {
        error.Invalid => a.gfx_output_error_invalid,
        error.Stale => a.gfx_output_error_stale,
        error.Unsupported => a.gfx_output_error_unsupported,
        error.Capacity => a.gfx_output_error_capacity,
    };
}
const StatsSlot = extern struct { has: bool = false, value: a.DisplayPresentationStats = undefined };
const InfoSlot = extern struct { has: bool = false, value: a.DisplayPresentationInfo = undefined };
pub const Owner = extern struct {
    driver: usize = 0, driver_generation: u64 = 0, display_generation: u64 = 0,
    backend: a.GfxBackendBinding = .{},
    heads: [a.gfx_output_max_assignments]StatsSlot = @splat(.{}),
    infos: [a.gfx_output_max_assignments]InfoSlot = @splat(.{}),
    history: [32]StatsSlot = @splat(.{}), history_cursor: usize = 0,
    pub fn bind(self: *Owner, driver: usize, driver_generation: u64, binding: a.GfxBackendBinding, generation: u64) Error!void {
        try status(r4os_rust_display_stats_bind(self, driver, driver_generation, &binding, generation));
    }
    pub fn publishInfo(self: *Owner, driver: usize, generation: u64, supplied: a.DisplayPresentationInfo, state: backend_state.Snapshot) Error!void {
        try status(r4os_rust_display_stats_publish_info(self, driver, generation, &supplied, &state));
    }
    pub fn readInfo(self: *const Owner, head: u32, state: backend_state.Snapshot) Error!a.DisplayPresentationInfo {
        var out: a.DisplayPresentationInfo = undefined; try status(r4os_rust_display_stats_read_info(self, head, &state, &out)); return out;
    }
    pub fn publish(self: *Owner, driver: usize, generation: u64, supplied: a.DisplayPresentationStats, state: backend_state.Snapshot) Error!void {
        try status(r4os_rust_display_stats_publish(self, driver, generation, &supplied, &state));
    }
    pub fn read(self: *const Owner, head: u32, state: backend_state.Snapshot) Error!a.DisplayPresentationStats {
        var out: a.DisplayPresentationStats = undefined; try status(r4os_rust_display_stats_read(self, head, &state, &out)); return out;
    }
    pub fn feedback(self: *const Owner, head: u32, source: a.GfxFence, state: backend_state.Snapshot) Error!a.DisplayPresentationStats {
        var out: a.DisplayPresentationStats = undefined; try status(r4os_rust_display_stats_feedback(self, head, &source, &state, &out)); return out;
    }
};
fn status(value: i32) Error!void { return switch (value) {
    0 => {}, 1 => error.Invalid, 3 => error.Unsupported, 6 => error.Capacity, 7 => error.Stale, else => unreachable,
}; }
extern fn r4os_rust_display_stats_bind(*Owner, usize, u64, *const a.GfxBackendBinding, u64) i32;
extern fn r4os_rust_display_stats_publish_info(*Owner, usize, u64, *const a.DisplayPresentationInfo, *const backend_state.Snapshot) i32;
extern fn r4os_rust_display_stats_read_info(*const Owner, u32, *const backend_state.Snapshot, *a.DisplayPresentationInfo) i32;
extern fn r4os_rust_display_stats_publish(*Owner, usize, u64, *const a.DisplayPresentationStats, *const backend_state.Snapshot) i32;
extern fn r4os_rust_display_stats_read(*const Owner, u32, *const backend_state.Snapshot, *a.DisplayPresentationStats) i32;
extern fn r4os_rust_display_stats_feedback(*const Owner, u32, *const a.GfxFence, *const backend_state.Snapshot, *a.DisplayPresentationStats) i32;
comptime {
    if (@sizeOf(StatsSlot) != 216 or @sizeOf(InfoSlot) != 128 or @sizeOf(Owner) != 9728 or @alignOf(Owner) != 8 or @offsetOf(Owner, "heads") != 56 or @offsetOf(Owner, "infos") != 1784 or @offsetOf(Owner, "history") != 2808 or @offsetOf(Owner, "history_cursor") != 9720) @compileError("Native presentation metadata C layout");
}
