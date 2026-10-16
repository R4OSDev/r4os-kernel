//! Opaque projection of the sole native resident command/VA job owner.
const abi = @import("r4os_kernel_contract");
const buffers = @import("../memory/gfx_buffers.zig");
const lifetime = buffers.lifetime;
const queue = @import("queue_state.zig");
const resources = @import("queue_resources.zig");
pub const Error = resources.Error || buffers.Error || @import("../kernel/gfx_virtual.zig").Error;
const Projection = extern struct { info: abi.GfxNativeJobInfo, resource_count: usize, uses: [*]resources.UseSlot, commands: [*]u8, command_bytes: usize };
const Acquire = extern struct { producer: lifetime.Owner, binding: lifetime.layout.Binding, queue: queue.NativeView, resources: resources.NativeView,
    timeline: u64, dependencies: [*]const queue.Fence, dependencies_len: usize, seen: [*]bool, pending: [*]u16 };
extern fn r4os_rust_display_queue_native(u32, ?*Job, ?*const anyopaque, ?*anyopaque, u32) i32;
extern fn r4os_rust_display_queue_native_acquire(*Job, *const Acquire) i32;
fn checked(code: i32) Error!void { return switch (code) {
    0 => {}, 1 => error.Invalid, 2 => error.Overflow, 3 => error.Unsupported, 4 => error.Exhausted, 5 => error.Budget, 6 => error.Capacity,
    7 => error.Stale, 8 => error.Busy, 9 => error.Closed, 10 => error.WrongOwner, 11 => error.OutOfMemory, 12 => error.Unavailable,
    13 => error.WaitTimeout, 14 => error.WaitCancelled, 16 => error.AlreadyCompleted, else => unreachable,
}; }
pub const Job = opaque {
    pub fn create(input: abi.GfxNativeSubmission) buffers.Error!*Job {
        var out: *Job = undefined; const result = r4os_rust_display_queue_native(0, null, &input, @ptrCast(&out), 0);
        switch (result) { 0 => return out, 1 => return error.Invalid, 11 => return error.OutOfMemory, else => unreachable }
    }
    fn projection(self: *Job) Projection { var out: Projection = undefined; checked(r4os_rust_display_queue_native(4, self, null, &out, 0)) catch unreachable; return out; }
    pub fn info(self: *Job) abi.GfxNativeJobInfo { return self.projection().info; }
    pub fn resourceCount(self: *Job) usize { return self.projection().resource_count; }
    pub fn useSlots(self: *Job) []resources.UseSlot { const value = self.projection(); return value.uses[0..value.resource_count]; }
    pub fn commandData(self: *Job) []const u8 { const value = self.projection(); return value.commands[0..value.command_bytes]; }
    pub fn binding(self: *Job, index: u32) Error!abi.GfxNativeBinding { var out: abi.GfxNativeBinding = undefined; try checked(r4os_rust_display_queue_native(5, self, null, &out, index)); return out; }
    pub fn acquireLocked(self: *Job, producer: lifetime.Owner, binding_value: lifetime.layout.Binding, state: anytype, held_resources: anytype, timeline: u64, dependencies: []const queue.Fence) Error!void {
        var seen: [state.jobs.len]bool = @splat(false); var pending: [state.jobs.len]u16 = undefined;
        const args: Acquire = .{ .producer = producer, .binding = binding_value, .queue = state.nativeView(), .resources = held_resources.nativeView(),
            .timeline = timeline, .dependencies = dependencies.ptr, .dependencies_len = dependencies.len, .seen = &seen, .pending = &pending };
        try checked(r4os_rust_display_queue_native_acquire(self, &args));
    }
    pub fn rollbackLocked(self: *Job) void { checked(r4os_rust_display_queue_native(1, self, null, null, 0)) catch unreachable; }
    pub fn releaseBindingsLocked(self: *Job) void { checked(r4os_rust_display_queue_native(2, self, null, null, 0)) catch unreachable; }
    pub fn destroy(self: *Job) void { checked(r4os_rust_display_queue_native(3, self, null, null, 0)) catch unreachable; }
};
