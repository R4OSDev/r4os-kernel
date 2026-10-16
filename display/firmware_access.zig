//! Native atomic gate, exact existing task token. No second writer counter.
comptime { if (@import("builtin").is_test) _ = @import("rust_owner_test"); }
const ownership = @import("ownership.zig");
const Unwind = @import("../sched/task_context.zig").UnwindToken;
pub const Gate = extern struct {
    state: u32 = 0,
    pub fn tryAcquire(self: *Gate) bool { return r4os_rust_display_firmware_gate_op(0, self); }
    pub fn release(self: *Gate) void { _ = r4os_rust_display_firmware_gate_op(1, self); }
    pub fn tryRevoke(self: *Gate) bool { return r4os_rust_display_firmware_gate_op(2, self); }
    pub fn restore(self: *Gate) void { _ = r4os_rust_display_firmware_gate_op(3, self); }
    pub fn isRevoked(self: *const Gate) bool { return r4os_rust_display_firmware_gate_op(4, self); }
};
extern var r4os_rust_display_firmware_gate: Gate;
pub const gate = &r4os_rust_display_firmware_gate;
extern fn r4os_rust_display_firmware_gate_op(u32, *const Gate) bool;
extern fn r4os_rust_display_firmware_acquire(*Unwind) bool;
extern fn r4os_rust_display_firmware_release(*const Unwind) void;
pub const Lease = struct {
    call: Unwind,
    pub fn release(self: Lease) void { r4os_rust_display_firmware_release(&self.call); }
};
pub fn acquire() ?Lease {
    var call: Unwind = undefined;
    return if (r4os_rust_display_firmware_acquire(&call)) .{ .call = call } else null;
}

test "firmware revoke waits for every admitted writer and rejects stale access" {
    const testing = @import("std").testing;
    var local: Gate = .{};
    try testing.expect(local.tryAcquire());
    try testing.expect(local.tryAcquire());
    try testing.expect(!local.tryRevoke());
    local.release();
    try testing.expect(!local.tryRevoke());
    local.release();
    try testing.expect(local.tryRevoke());
    try testing.expect(!local.tryAcquire());
    try testing.expect(!local.tryRevoke());
    local.restore();
    try testing.expect(local.tryAcquire());
    local.release();
}
