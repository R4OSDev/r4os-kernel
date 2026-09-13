//! Common cursor input lifetime. Call under the existing BO metadata owner;
//! no CPU pixels, allocations, callbacks or GPU commands are processed here.
const std = @import("std");
const lifetime = @import("../memory/gfx_buffer_owner.zig");
const a = @import("r4os_kernel_contract");
pub const owner: lifetime.Owner = .{ .kind = .kernel, .id = 4, .generation = 1 };
pub const Source = struct {
    reference: lifetime.Handle = .{},
    borrowed: lifetime.Handle = .{},
    lease: lifetime.Handle = .{},
    driver: lifetime.Owner = .{ .kind = .driver, .id = 0, .generation = 0 },
    pub fn open(self: *Source, store: anytype, caller: lifetime.Owner, driver: lifetime.Owner, request: a.DisplayCursorRequest) !void {
        if (self.reference.id != 0 or self.borrowed.id != 0 or self.lease.id != 0) return error.Busy;
        const input: lifetime.Handle = .{ .id = request.reference.id, .generation = request.reference.generation };
        const d = try store.describe(input, caller);
        if (d.location != .system or !d.binding.portable() or d.modifier != 0 or d.format != .argb8888 or d.plane_count != 1 or
            d.planes[0].offset != 0 or d.width != request.width or d.height != request.height or d.planes[0].pitch != request.pitch or
            d.bytes != request.byte_length or d.usage & lifetime.layout.Usage.cpu_read == 0) return error.Invalid;
        self.driver = driver;
        errdefer self.close(store) catch {};
        self.reference = try store.share(input, owner);
        const held = try store.use(self.reference, owner, .cpu_read, 0, d.bytes);
        self.lease = held.lease;
        if (held.backing.cache != .write_back) return error.Unsupported;
        self.borrowed = try store.share(self.reference, driver);
    }
    pub fn close(self: *Source, store: anytype) !void {
        if (self.borrowed.id != 0) { try store.drop(self.borrowed, self.driver); self.borrowed = .{}; }
        if (self.lease.id != 0) { try store.endUse(self.lease, owner, false); self.lease = .{}; }
        if (self.reference.id != 0) { try store.drop(self.reference, owner); self.reference = .{}; }
    }
    pub fn wire(self: *const Source) a.GfxBufferHandle { return .{ .id = self.borrowed.id, .generation = self.borrowed.generation }; }
};
