// Temporary stateless forwarding to owners not migrated in this wave yet.
export fn r4os_zig_work_display_init() callconv(.c) bool {
    return @import("../display/queue.zig").init();
}
export fn r4os_zig_work_invoke_owned(owner: u32, handler: *const fn (usize) callconv(.c) i32, context: usize) callconv(.c) i32 {
    return @import("driver_api.zig").invokeOwnedWork(owner, handler, context);
}
