//! Retains the separately encoded image for the common mode transaction.
//! The console shadow remains owned by native_driver/native_additional_mode.
const std = @import("std");
const a = @import("r4os_kernel_contract");
const buffers = @import("../memory/gfx_buffers.zig");
const api = @import("../program/gfx_buffer_api.zig");
const outputs = @import("outputs.zig");
const owner = @import("queue_resources.zig").display_owner;
pub const Error = outputs.Error;

fn validateLocked(caller: buffers.Owner, request: *const a.GfxModeColorRequest, source: a.GfxOutputColorState) Error!buffers.Handle {
    const handle = try api.handle(request.image);
    const descriptor = try buffers.store.describe(handle, caller);
    try @import("mode_color_state.zig").validate(request, source, descriptor);
    const shadow = try api.handle(request.state.assignments[0].buffer);
    if ((try buffers.store.bufferFor(handle, caller)).eql(try buffers.store.bufferFor(shadow, caller))) return error.Invalid;
    return handle;
}
pub fn validate(caller: buffers.Owner, request: *const a.GfxModeColorRequest) Error!void {
    if (request.state.count != 1) return error.Invalid;
    const source = try outputs.colorAt(request.state.assignments[0].output);
    buffers.lock(); defer buffers.unlock();
    _ = try validateLocked(caller, request, source);
}
pub const Owner = struct {
    reference: buffers.Handle = .{},
    driver_reference: buffers.Handle = .{},
    read: buffers.Handle = .{},
    driver: buffers.Owner = .{ .kind = .driver, .id = 0, .generation = 0 },

    pub fn empty(self: *const Owner) bool { return self.reference.id == 0 and self.driver_reference.id == 0 and self.read.id == 0; }
    pub fn prepare(self: *Owner, caller: buffers.Owner, driver: buffers.Owner, request: *const a.GfxModeColorRequest) Error!a.GfxDriverModeColor {
        if (!self.empty()) return error.Busy;
        if (request.state.count != 1) return error.Invalid;
        const source = try outputs.colorAt(request.state.assignments[0].output);
        try buffers.lockPrepared(.{ .references = 2, .leases = 1 }); defer buffers.unlock();
        const handle = try validateLocked(caller, request, source);
        const descriptor = try buffers.store.describe(handle, caller);
        const reference = try buffers.store.share(handle, owner);
        errdefer buffers.store.drop(reference, owner) catch unreachable;
        const driver_reference = try buffers.store.share(handle, driver);
        errdefer buffers.store.drop(driver_reference, driver) catch unreachable;
        const read = try buffers.store.use(reference, owner, .device_read, 0, descriptor.bytes);
        errdefer buffers.store.endUse(read.lease, owner, true) catch unreachable;
        const wire = try api.referenceLocked(driver_reference, driver);
        self.* = .{ .reference = reference, .driver_reference = driver_reference, .read = read.lease, .driver = driver };
        return .{ .signal = request.signal, .reference = wire };
    }
    pub fn release(self: *Owner) Error!void {
        buffers.lock(); defer buffers.unlock();
        if (self.read.id != 0) { try buffers.store.endUse(self.read, owner, true); self.read = .{}; }
        if (self.driver_reference.id != 0) { try buffers.store.drop(self.driver_reference, self.driver); self.driver_reference = .{}; }
        if (self.reference.id != 0) { try buffers.store.drop(self.reference, owner); self.reference = .{}; }
    }
};
