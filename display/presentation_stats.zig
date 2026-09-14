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
pub const Owner = struct {
    driver: usize = 0,
    driver_generation: u64 = 0,
    display_generation: u64 = 0,
    backend: a.GfxBackendBinding = .{},
    heads: [a.gfx_output_max_assignments]?a.DisplayPresentationStats = @splat(null),
    infos: [a.gfx_output_max_assignments]?a.DisplayPresentationInfo = @splat(null),
    history: [32]?a.DisplayPresentationStats = @splat(null),
    history_cursor: usize = 0,

    pub fn bind(self: *Owner, driver: usize, driver_generation: u64, binding: a.GfxBackendBinding, generation: u64) Error!void {
        if (driver == 0 or driver_generation == 0 or generation == 0 or binding.version != 1 or binding.size < @sizeOf(a.GfxBackendBinding) or
            binding.adapter_id == 0 or binding.device_generation == 0 or binding.reset_generation == 0 or
            binding.milestone != a.gfx_queue_milestone_device_execution) return error.Invalid;
        self.* = .{ .driver = driver, .driver_generation = driver_generation, .display_generation = generation, .backend = binding };
    }
    fn current(self: *const Owner, state: backend_state.Snapshot) bool {
        return self.driver != 0 and state.owner == self.driver and state.generation == self.display_generation and
            state.adapter_id == self.backend.adapter_id and (state.state == .software_native or state.state == .native or state.state == .recovering);
    }
    pub fn publishInfo(self: *Owner, driver: usize, generation: u64, supplied: a.DisplayPresentationInfo, state: backend_state.Snapshot) Error!void {
        if (supplied.version != 1 or supplied.size < @sizeOf(a.DisplayPresentationInfo)) return error.Invalid;
        var input = supplied; input.size = @sizeOf(a.DisplayPresentationInfo);
        if (driver != self.driver or generation != self.driver_generation or !self.current(state) or
            input.display_generation != self.display_generation or !std.meta.eql(input.backend, self.backend)) return error.Stale;
        const native = a.display_presentation_info_native;
        const sync = a.display_presentation_info_synchronized;
        const visible = a.display_presentation_info_visibility;
        if (input.sequence == 0 or input.head_id >= 8 or input.width == 0 or input.height == 0 or
            input.format != a.gfx_buffer_format_xrgb8888 or input.flags & ~@as(u32, 511) != 0 or input.flags & native == 0 or
            input.buffer_count < 2 or input.buffer_count > 3 or input.plane_count == 0 or input.plane_count > 8 or
            input.policies & ~@as(u32, 7) != 0 or input.policies & 1 == 0 or input.reserved0 != 0 or input.path > 3 or
            input.interval_ns > std.time.ns_per_s or (input.observed_ns == 0) != (input.observed_sequence == 0) or
            (input.flags & sync != 0 and input.flags & visible == 0) or
            (input.flags & (a.display_presentation_info_direct | a.display_presentation_info_overlay) != 0 and input.flags & visible == 0) or
            (input.path == 2 and input.flags & a.display_presentation_info_direct == 0) or
            (input.path == 3 and input.flags & a.display_presentation_info_overlay == 0)) return error.Invalid;
        var free: ?usize = null;
        for (&self.infos, 0..) |*slot, index| {
            const previous = slot.* orelse { if (free == null) free = index; continue; };
            if (previous.head_id != input.head_id) continue;
            if (std.meta.eql(previous, input)) return;
            if (input.sequence <= previous.sequence or input.observed_sequence < previous.observed_sequence or input.observed_ns < previous.observed_ns or
                (previous.flags & a.display_presentation_info_lost != 0 and input.flags & a.display_presentation_info_lost == 0)) return error.Stale;
            slot.* = input; return;
        }
        self.infos[free orelse return error.Capacity] = input;
    }
    pub fn readInfo(self: *const Owner, head: u32, state: backend_state.Snapshot) Error!a.DisplayPresentationInfo {
        if (!self.current(state)) return error.Unsupported;
        for (&self.infos) |*slot| if (slot.*) |sample| if (sample.head_id == head) {
            var value = sample;
            if (state.state == .recovering) value.flags |= a.display_presentation_info_lost;
            return value;
        };
        return error.Unsupported;
    }
    pub fn publish(self: *Owner, driver: usize, generation: u64, supplied: a.DisplayPresentationStats, state: backend_state.Snapshot) Error!void {
        if (supplied.version != 1 or supplied.size < @sizeOf(a.DisplayPresentationStats)) return error.Invalid;
        var input = supplied;
        input.size = @sizeOf(a.DisplayPresentationStats);
        if (driver != self.driver or generation != self.driver_generation or !self.current(state) or
            input.display_generation != self.display_generation or !std.meta.eql(input.backend, self.backend)) return error.Stale;
        if (input.version != 1 or input.size < @sizeOf(a.DisplayPresentationStats) or input.sequence == 0 or
            input.flags & a.display_presentation_flag_available == 0 or input.flags & ~@as(u32, 7) != 0 or input.pending & ~@as(u32, 15) != 0 or
            input.buffer_count == 0 or input.rendered_count > input.acquired_count or input.visible_count > input.submitted_count or
            input.released_count > input.visible_count or input.visible_count - input.released_count > 1 or
            input.visible_sequence > input.submitted_count or (input.source_timeline == 0) != (input.source_point == 0)) return error.Invalid;
        if (input.visible_count == 0) {
            if (input.visible_sequence != 0 or input.source_timeline != 0 or input.source_point != 0 or input.render_point != 0 or input.window_point != 0 or
                input.submitted_ns != 0 or input.visible_ns != 0 or input.gpu_timestamp != 0 or input.irq_sequence != 0 or
                input.irq_observed_ns != 0 or input.released_ns != 0) return error.Invalid;
        } else {
            if (input.visible_sequence == 0 or (input.render_point == 0 and input.flags & a.display_presentation_flag_direct == 0) or
                (input.flags & a.display_presentation_flag_direct != 0 and input.source_timeline == 0) or input.window_point == 0 or input.submitted_ns == 0 or
                input.visible_ns < input.submitted_ns or input.irq_sequence == 0 or input.irq_observed_ns < input.submitted_ns or
                input.irq_observed_ns > input.visible_ns or (input.released_ns != 0 and input.released_ns < input.visible_ns) or
                (input.released_ns == 0) != (input.released_count < input.visible_count)) return error.Invalid;
        }
        var free: ?usize = null;
        for (&self.heads, 0..) |*slot, index| {
            const previous = slot.* orelse { if (free == null) free = index; continue; };
            if (previous.head_id != input.head_id) continue;
            if (input.sequence == previous.sequence and std.meta.eql(input, previous)) return;
            if (input.sequence <= previous.sequence or input.acquired_count < previous.acquired_count or input.rendered_count < previous.rendered_count or
                input.submitted_count < previous.submitted_count or input.visible_count < previous.visible_count or input.released_count < previous.released_count or
                input.rejected_count < previous.rejected_count or input.visible_sequence < previous.visible_sequence or input.visible_ns < previous.visible_ns or
                input.irq_sequence < previous.irq_sequence or (previous.flags & a.display_presentation_flag_lost != 0 and input.flags & a.display_presentation_flag_lost == 0)) return error.Stale;
            slot.* = input; self.remember(input); return;
        }
        self.heads[free orelse return error.Capacity] = input;
        self.remember(input);
    }
    fn remember(self: *Owner, value: a.DisplayPresentationStats) void {
        if (value.visible_ns == 0 or value.source_timeline == 0) return;
        for (&self.history) |*slot| if (slot.*) |prior| {
            if (prior.head_id == value.head_id and prior.source_timeline == value.source_timeline and prior.source_point == value.source_point) {
                slot.* = value; return;
            }
        };
        self.history[self.history_cursor] = value;
        self.history_cursor = (self.history_cursor + 1) % self.history.len;
    }
    pub fn feedback(self: *const Owner, head: u32, source: a.GfxFence, state: backend_state.Snapshot) Error!a.DisplayPresentationStats {
        if (source.timeline == 0 or source.point == 0) return error.Invalid;
        if (!self.current(state) or source.adapter_id != self.backend.adapter_id or source.device_generation != self.backend.device_generation or
            source.reset_generation != self.backend.reset_generation) return error.Stale;
        for (&self.history) |*slot| if (slot.*) |value| if (value.head_id == head and value.source_timeline == source.timeline and value.source_point == source.point) {
            var result = value;
            if (state.state == .recovering) result.flags |= a.display_presentation_flag_lost;
            return result;
        };
        return error.Unsupported;
    }
    pub fn read(self: *const Owner, head: u32, state: backend_state.Snapshot) Error!a.DisplayPresentationStats {
        if (!self.current(state)) return error.Unsupported;
        for (&self.heads) |*slot| if (slot.*) |sample| if (sample.head_id == head) {
            var value = sample;
            if (state.state == .recovering) value.flags |= a.display_presentation_flag_lost;
            return value;
        };
        return error.Unsupported;
    }
};
