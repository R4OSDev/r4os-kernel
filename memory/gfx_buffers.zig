// Typed C projection of the single native BO runtime and metadata owner.
const sync = @import("../sched/sync.zig");
pub const lifetime = @import("gfx_buffer_owner.zig");
pub const layout = lifetime.layout;
pub const Owner = lifetime.Owner;
pub const Handle = lifetime.Handle;
pub const Error = lifetime.Error || error{OutOfMemory};
pub const Created = extern struct { buffer: Handle, reference: Handle, address: u64, bytes: u64 };
pub const Need = extern struct { objects: usize = 0, references: usize = 0, leases: usize = 0 };
pub const total_budget: u64 = 4 * 1024 * 1024 * 1024;
pub const producer_budget: u64 = 2 * 1024 * 1024 * 1024;
pub const system_reserve: u64 = 16 * 1024 * 1024;
pub const Store = lifetime.GlobalTable(total_budget, producer_budget);
pub const store: Store = .{};
pub const raster_owner = Owner{ .kind = .kernel, .id = 1, .generation = 1 };
extern fn r4os_rust_gfx_buffers_mutex() *sync.Mutex;
extern fn r4os_rust_gfx_buffers_lock(bool) void;
extern fn r4os_rust_gfx_buffers_visibility() void;
extern fn r4os_rust_gfx_buffers_run(u32, *const lifetime.NativeArgs, ?*anyopaque) i32;
fn call(op: u32, args: lifetime.NativeArgs, out: ?*anyopaque) Error!void {
    return switch (r4os_rust_gfx_buffers_run(op, &args, out)) {
        0 => {},
        1 => error.Invalid,
        2 => error.Overflow,
        3 => error.Unsupported,
        4 => error.Exhausted,
        5 => error.Budget,
        6 => error.Capacity,
        7 => error.Stale,
        8 => error.Busy,
        9 => error.Closed,
        10 => error.WrongOwner,
        11 => error.OutOfMemory,
        else => unreachable,
    };
}
pub fn metadataLock() *sync.Mutex {
    return r4os_rust_gfx_buffers_mutex();
}
pub fn lock() void {
    r4os_rust_gfx_buffers_lock(true);
}
pub fn unlock() void {
    r4os_rust_gfx_buffers_lock(false);
}
pub fn visibility() void {
    r4os_rust_gfx_buffers_visibility();
}
pub fn lockPrepared(need: Need) Error!void {
    return call(6, .{ .input = &need }, null);
}
pub fn create(owner: Owner, descriptor: layout.Descriptor) Error!Created {
    var out: Created = undefined;
    try call(0, .{ .owner = owner, .input = &descriptor }, &out);
    return out;
}
pub fn mapLocked(reference: Handle, owner: Owner, access: lifetime.Access, offset: u64, bytes: u64) Error!lifetime.Use {
    var out: lifetime.NativeUse = undefined;
    try call(7, .{ .handle = reference, .owner = owner, .access = @intFromEnum(access), .offset = offset, .bytes = bytes }, &out);
    return out.value();
}
pub fn unmapCpuLocked(lease: Handle, owner: Owner) Error!void {
    return call(8, .{ .handle = lease, .owner = owner }, null);
}
pub fn drop(reference: Handle, owner: Owner) Error!void {
    return call(1, .{ .handle = reference, .owner = owner }, null);
}
pub fn stopped(owner: Owner) void {
    call(2, .{ .owner = owner }, null) catch unreachable;
}
pub fn collect() void {
    call(3, .{}, null) catch unreachable;
}
pub fn retainsDriver(owner: u32) bool {
    var out: bool = undefined;
    call(4, .{ .value = owner }, &out) catch unreachable;
    return out;
}
pub fn pendingReleaseForOwner(owner: Owner) bool {
    var out: bool = undefined;
    call(5, .{ .owner = owner }, &out) catch unreachable;
    return out;
}
comptime {
    if (@sizeOf(Need) != 24 or @sizeOf(Created) != 48) @compileError("native BO runtime records");
    if (@offsetOf(Created, "buffer") != 0 or @offsetOf(Created, "reference") != 16 or @offsetOf(Created, "address") != 32 or @offsetOf(Created, "bytes") != 40) @compileError("native BO creation offsets");
}
