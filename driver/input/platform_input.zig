// Short copied-input owner; never executes firmware or user policy under it.
const a = @import("r4os_kernel_contract");
const locks = @import("../../memory/owner_locks.zig");
const model = @import("platform_input_state.zig");
const events = @import("../../kernel/desktop_events.zig");
var lock: locks.Lock = .{ .class = .driver_work, .rank = 2 };
var state: model.State = .{};
fn now() u64 { return @import("../../platform/monotonic.zig").nowNanoseconds() orelse 0; }
pub fn snapshot(output: *a.PlatformInputSnapshot) callconv(.c) i32 {
    if (@intFromPtr(output) == 0 or @intFromPtr(output) % @alignOf(a.PlatformInputSnapshot) != 0 or
        output.version != 1 or output.size < @sizeOf(a.PlatformInputSnapshot)) return -1;
    const token = lock.acquire(); defer lock.release(token);
    output.* = state.value; return 1;
}
pub fn submit(owner: u32, epoch: u64, kind: u32, value: u32) i32 {
    const stamp = now();
    const changed = blk: {
        const token = lock.acquire(); defer lock.release(token);
        break :blk state.submit(owner, epoch, kind, value, stamp) catch |err| return switch (err) {
            error.Busy => a.driver_resource_error_busy,
            error.Stale => a.driver_resource_error_stale,
            else => a.driver_resource_error_invalid,
        };
    };
    if (changed) events.signal();
    return a.driver_resource_ok;
}
pub fn remove(owner: u32) void {
    const stamp = now();
    const changed = blk: {
        const token = lock.acquire(); defer lock.release(token);
        break :blk state.remove(owner, stamp) catch false;
    };
    if (changed) events.signal();
}
pub fn usb(present: bool) void {
    const stamp = now();
    const changed = blk: {
        const token = lock.acquire(); defer lock.release(token);
        break :blk state.usb(present, stamp) catch false;
    };
    if (changed) events.signal();
}
pub fn consumer(usage: u32) void {
    const stamp = now();
    const changed = blk: {
        const token = lock.acquire(); defer lock.release(token);
        break :blk state.consumer(usage, stamp) catch false;
    };
    if (changed) events.signal();
}
