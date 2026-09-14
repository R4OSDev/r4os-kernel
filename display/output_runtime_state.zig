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
pub const Entry = struct {
    driver: lifetime.Owner = .{ .kind = .driver, .id = 0, .generation = 0 },
    target: abi.GfxOutputTarget = .{},
    binding: abi.GfxBackendBinding = .{},
    width: u32 = 0,
    height: u32 = 0,
    active: bool = false,
    mode_blocked: bool = false,
    mode_lost: bool = false,
    removing: bool = false,
    pending: ?queue.Fence = null,
    direct: bool = false,
    statistics: stats.Owner = .{},

    pub fn snapshot(self: *const Entry) backend.Snapshot {
        return .{ .state = .software_native, .owner = self.driver.id, .generation = self.target.display_generation,
            .adapter_id = self.binding.adapter_id };
    }
    pub fn info(self: *const Entry) Error!abi.DisplayPresentationInfo {
        var value = try self.statistics.readInfo(self.target.head_id, self.snapshot());
        value.width = self.width; value.height = self.height;
        if (!self.active) value.flags = (value.flags & ~abi.display_presentation_info_active) | abi.display_presentation_info_occluded;
        if (self.mode_lost) value.flags |= abi.display_presentation_info_lost;
        return value;
    }
    pub fn publishInfo(self: *Entry, driver: lifetime.Owner, value: abi.DisplayPresentationInfo) Error!void {
        if (!self.driver.eql(driver) or value.head_id != self.target.head_id or value.width != self.width or value.height != self.height) return error.Stale;
        try self.statistics.publishInfo(driver.id, driver.generation, value, self.snapshot());
    }
    pub fn publishStats(self: *Entry, driver: lifetime.Owner, value: abi.DisplayPresentationStats) Error!void {
        if (!self.driver.eql(driver) or value.head_id != self.target.head_id) return error.Stale;
        try self.statistics.publish(driver.id, driver.generation, value, self.snapshot());
    }
};
pub const Store = struct {
    entries: [capacity]Entry = @splat(.{}),
    // Separate from the boot/native-primary generation space. Never wraps.
    serial: u64 = @as(u64, 1) << 63,

    pub fn register(self: *Store, driver: lifetime.Owner, request: abi.GfxAdditionalOutput) Error!abi.GfxOutputTarget {
        const binding = request.backend;
        if (driver.kind != .driver or !driver.valid() or request.version != 1 or request.size != @sizeOf(abi.GfxAdditionalOutput) or
            request.flags != 0 or request.reserved0 != 0 or request.job_size != @sizeOf(abi.GfxDriverJob) or
            request.head_id >= capacity or request.width == 0 or request.width > 65536 or request.height == 0 or request.height > 65536 or
            request.format != abi.gfx_buffer_format_xrgb8888 or binding.adapter_id == 0 or
            request.output.adapter_id != binding.adapter_id or request.output.device_generation != binding.device_generation or
            request.output.connector_id == 0 or request.output.connection_generation == 0) return error.Invalid;
        var free: ?*Entry = null;
        for (&self.entries) |*entry| {
            if (entry.driver.id == 0) { if (free == null) free = entry; continue; }
            if (entry.target.adapter_id == binding.adapter_id and
                (entry.target.head_id == request.head_id or entry.target.connector_id == request.output.connector_id)) {
                // A driver may refresh its paused catalog identity. This
                // does not retire storage or change the physical geometry.
                if (!entry.driver.eql(driver) or entry.active or entry.mode_blocked or entry.removing or
                    !std.meta.eql(entry.binding, binding) or entry.target.head_id != request.head_id or
                    entry.target.connector_id != request.output.connector_id or entry.width != request.width or
                    entry.height != request.height) return error.Busy;
                entry.target = identity.fromOutput(request.output, request.head_id, entry.target.display_generation);
                return entry.target;
            }
        }
        const entry = free orelse return error.Capacity;
        const generation = std.math.add(u64, self.serial, 1) catch return error.Exhausted;
        // Validate the complete binding before publishing any new entry.
        errdefer entry.* = .{};
        try entry.statistics.bind(driver.id, driver.generation, binding, generation);
        try entry.statistics.publishInfo(driver.id, driver.generation, .{ .flags = abi.display_presentation_info_native | abi.display_presentation_info_active,
            .head_id = request.head_id, .backend = binding, .display_generation = generation, .sequence = 1,
            .width = request.width, .height = request.height, .format = request.format, .policies = 7, .buffer_count = 2, .plane_count = 1 },
            .{ .state = .software_native, .owner = driver.id, .generation = generation, .adapter_id = binding.adapter_id });
        entry.driver = driver;
        entry.target = identity.fromOutput(request.output, request.head_id, generation);
        entry.binding = binding;
        entry.width = request.width; entry.height = request.height;
        self.serial = generation;
        return entry.target;
    }
    pub fn find(self: *Store, target: abi.GfxOutputTarget) Error!*Entry {
        if (!identity.valid(target)) return error.Invalid;
        for (&self.entries) |*entry| if (entry.driver.id != 0 and identity.same(entry.target, target)) return entry;
        return error.Stale;
    }
    pub fn at(self: *Store, adapter: u32, head: u32) ?*Entry {
        for (&self.entries) |*entry| if (entry.driver.id != 0 and entry.active and entry.target.adapter_id == adapter and entry.target.head_id == head) return entry;
        return null;
    }
    pub fn stop(self: *Store, driver: u32) void {
        // The common queue, not this metadata catalog, owns submitted BOs.
        // Clearing a stopped driver's routes cannot retire a device lease.
        for (&self.entries) |*entry| if (entry.driver.id == driver) { entry.* = .{}; };
    }
};
