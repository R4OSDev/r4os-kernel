//! Privileged BO lifetime for additional native heads. No framebuffer, GPU
//! commands or long primary DisplayExecution ownership belongs to this path.
const std = @import("std");
const abi = @import("r4os_kernel_contract");
const runtime = @import("output_runtime.zig");
const buffers = @import("../memory/gfx_buffers.zig");
const api = @import("../program/gfx_buffer_api.zig");
const queue = @import("queue.zig");
const owner = @import("queue_resources.zig").display_owner;
pub const Binding = runtime.ModeBinding;
pub const Error = @import("outputs.zig").Error || @import("output_runtime_state.zig").Error;
const Surface = struct {
    reference: buffers.Handle = .{},
    driver_reference: buffers.Handle = .{},
    read_lease: buffers.Handle = .{},
    bytes: u64 = 0,
};
const Resident = struct { binding: Binding, surface: Surface };
const Change = struct {
    index: usize,
    binding: Binding,
    old: Surface,
    new: Surface,
    width: u32,
    height: u32,
    using_new: bool = false,
};
var residents: [abi.gfx_output_max_assignments]?Resident = @splat(null);
var change: ?Change = null;
var execution = @import("../sched/sync.zig").UnwindGuard.init("additional-native-mode");

fn sameConsumer(left: Binding, driver: buffers.Owner, target: abi.GfxOutputTarget) bool {
    // Catalog refresh can replace connection_generation without retiring
    // the same physical head. display_generation identifies its lifetime.
    return left.driver.eql(driver) and left.target.display_generation == target.display_generation and
        left.target.adapter_id == target.adapter_id and left.target.device_generation == target.device_generation and
        left.target.connector_id == target.connector_id and left.target.head_id == target.head_id;
}
pub fn prepare(caller: buffers.Owner, assignment: abi.GfxScanoutState, mode: abi.GfxOutputMode, expected: Binding) Error!abi.GfxBufferReference {
    if (!execution.enter(0)) return error.Busy;
    defer _ = execution.leave();
    if (change != null) return error.Busy;
    if (assignment.source_x != 0 or assignment.source_y != 0 or assignment.destination_x != 0 or assignment.destination_y != 0 or
        assignment.source_width != mode.width or assignment.source_height != mode.height or
        assignment.destination_width != mode.width or assignment.destination_height != mode.height or
        assignment.rotation != 0 or assignment.color != 0 or assignment.bits_per_color != 8) return error.Unsupported;
    const source = try api.handle(assignment.buffer);
    const current = try runtime.modeBinding(assignment);
    if (!std.meta.eql(current, expected)) return error.Stale;
    try queue.validateOutputBinding(@intCast(expected.driver.id), expected.backend);
    var free: ?usize = null;
    const index = blk: {
        for (&residents, 0..) |*slot, index| {
            if (slot.*) |value| {
                if (sameConsumer(value.binding, expected.driver, expected.target)) break :blk index;
            } else if (free == null) free = index;
        }
        break :blk free orelse return error.Capacity;
    };
    const old: Surface = if (residents[index]) |value| value.surface else .{};
    try runtime.beginMode(expected);
    errdefer runtime.finishMode(expected, expected.width, expected.height, expected.active, false) catch {};
    const prepared = blk: {
        buffers.lock(); defer buffers.unlock();
        const descriptor = try buffers.store.describe(source, caller);
        const usage = buffers.layout.Usage.cpu_write | buffers.layout.Usage.transfer_source | buffers.layout.Usage.scanout;
        if (descriptor.format != .xrgb8888 or descriptor.location != .system or !descriptor.binding.portable() or
            descriptor.modifier != 0 or descriptor.plane_count != 1 or descriptor.planes[0].offset != 0 or
            descriptor.usage & usage != usage or descriptor.width != mode.width or descriptor.height != mode.height or
            descriptor.planes[0].pitch != @as(u64, mode.width) * 4 or descriptor.bytes != descriptor.planes[0].pitch * mode.height)
            return error.Unsupported;
        if (old.reference.id != 0 and (try buffers.store.bufferFor(source, caller)).eql(try buffers.store.bufferFor(old.reference, owner)))
            return error.Invalid;
        const reference = try buffers.store.share(source, owner);
        errdefer buffers.store.drop(reference, owner) catch unreachable;
        const driver_reference = try buffers.store.share(source, expected.driver);
        errdefer buffers.store.drop(driver_reference, expected.driver) catch unreachable;
        const read = try buffers.store.use(reference, owner, .device_read, 0, descriptor.bytes);
        errdefer buffers.store.endUse(read.lease, owner, true) catch unreachable;
        break :blk .{ .surface = Surface{ .reference = reference, .driver_reference = driver_reference,
            .read_lease = read.lease, .bytes = descriptor.bytes }, .wire = try api.referenceLocked(driver_reference, expected.driver) };
    };
    residents[index] = .{ .binding = expected, .surface = old };
    change = .{ .index = index, .binding = expected, .old = old, .new = prepared.surface, .width = mode.width, .height = mode.height };
    return prepared.wire;
}
pub fn start(operation: u32, expected: Binding) Error!void {
    if (!execution.enter(0)) return error.Busy;
    defer _ = execution.leave();
    const value = if (change) |*slot| slot else return error.Stale;
    if (!std.meta.eql(value.binding, expected)) return error.Stale;
    try queue.validateOutputBinding(@intCast(expected.driver.id), expected.backend);
    try runtime.beginMode(expected);
    const surface = if (operation == abi.gfx_mode_operation_apply) &value.old else
        if (operation == abi.gfx_mode_operation_rollback) &value.new else
        if (operation == abi.gfx_mode_operation_confirm) return else return error.Invalid;
    // The initial private shadow belongs entirely to R4D. It needs no
    // invented common reference when the first additional mode is applied.
    if (surface.reference.id != 0 and surface.read_lease.id == 0) {
        buffers.lock(); defer buffers.unlock();
        surface.read_lease = (try buffers.store.use(surface.reference, owner, .device_read, 0, surface.bytes)).lease;
    }
}
fn releaseRead(surface: *Surface) Error!void {
    if (surface.read_lease.id == 0) return;
    buffers.lock(); defer buffers.unlock();
    try buffers.store.endUse(surface.read_lease, owner, true);
    surface.read_lease = .{};
}
fn releaseSurface(driver: buffers.Owner, surface: *Surface) Error!void {
    try releaseRead(surface);
    if (surface.driver_reference.id != 0) {
        try buffers.drop(surface.driver_reference, driver);
        surface.driver_reference = .{};
    }
    if (surface.reference.id != 0) {
        try buffers.drop(surface.reference, owner);
        surface.reference = .{};
    }
}
pub fn settle(operation: u32, outcome: u32) Error!void {
    if (!execution.enter(0)) return error.Busy;
    defer _ = execution.leave();
    const value = if (change) |*slot| slot else return error.Stale;
    const expected = value.binding;
    if (outcome == abi.gfx_output_outcome_lost) {
        try runtime.finishMode(expected, if (value.using_new) value.width else expected.width,
            if (value.using_new) value.height else expected.height, false, true);
        return; // A missing receipt cannot release either consumer.
    }
    if (outcome == abi.gfx_output_outcome_applied) {
        if (operation == abi.gfx_mode_operation_apply) {
            try releaseRead(&value.new);
            try runtime.finishMode(expected, value.width, value.height, expected.active, false);
            residents[value.index].?.surface = value.new;
            value.using_new = true;
            return;
        }
        if (operation != abi.gfx_mode_operation_confirm or !value.using_new) return error.Invalid;
        try runtime.finishMode(expected, value.width, value.height, expected.active, false);
        try releaseSurface(expected.driver, &value.old);
        residents[value.index].?.surface = value.new;
    } else if (outcome == abi.gfx_output_outcome_old_preserved) {
        try releaseRead(&value.old);
        try runtime.finishMode(expected, expected.width, expected.height, expected.active, false);
        try releaseSurface(expected.driver, &value.new);
        residents[value.index].?.surface = value.old;
    } else return error.Invalid;
    change = null;
}
pub fn retire(driver: buffers.Owner, target: abi.GfxOutputTarget) Error!void {
    if (!execution.enter(0)) return error.Busy;
    defer _ = execution.leave();
    for (&residents, 0..) |*slot, index| if (slot.*) |*value| {
        if (!sameConsumer(value.binding, driver, target)) continue;
        if (change) |pending| if (pending.index == index) return error.Busy;
        try releaseSurface(driver, &value.surface);
        slot.* = null;
        return;
    };
}
