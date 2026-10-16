// Native process-owned notification namespace and scheduler wait transport.
// Userland libraries retain their own mutex/condition-variable policy.
const NativePrng = extern struct { xorshift: usize = 0 };
const NativeTree = extern struct { root: ?*anyopaque = null, prng: NativePrng = .{} };
pub const Owner = extern struct { tree: NativeTree = .{}, closing: bool = false };
extern fn r4os_rust_notifications_call(u8, *Owner, u64, u64, u64, ?*u64) callconv(.c) i32;
extern fn r4os_rust_notifications_cleanup(*Owner) callconv(.c) bool;
pub fn create(owner: *Owner, output: *u64) i32 {
    return r4os_rust_notifications_call(0, owner, 0, 0, 0, output);
}
pub fn query(owner: *Owner, handle: u64, output: *u64) i32 {
    return r4os_rust_notifications_call(1, owner, handle, 0, 0, output);
}
pub fn notify(owner: *Owner, handle: u64, count: u32) i32 {
    return r4os_rust_notifications_call(2, owner, handle, count, 0, null);
}
pub fn wait(owner: *Owner, handle: u64, observed: u64, timeout_ticks: u64) i32 {
    return r4os_rust_notifications_call(3, owner, handle, observed, timeout_ticks, null);
}
pub fn close(owner: *Owner, handle: u64) i32 {
    return r4os_rust_notifications_call(4, owner, handle, 0, 0, null);
}
pub fn cleanup(owner: *Owner) bool {
    return r4os_rust_notifications_cleanup(owner);
}
comptime {
    if (@sizeOf(NativeTree) != 16 or @alignOf(NativeTree) != 8 or @offsetOf(NativeTree, "root") != 0 or
        @offsetOf(NativeTree, "prng") != 8 or @sizeOf(Owner) != 24 or @alignOf(Owner) != 8 or
        @offsetOf(Owner, "tree") != 0 or @offsetOf(Owner, "closing") != 16)
        @compileError("Native notification owner layout drift");
}
