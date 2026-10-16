// Exact managed path classification is native; constants retain the on-disk contract.
pub const CompanionKind = enum { license, source, preload };
pub const companion_recovery_kernel = "0.1.199";
pub const preload_recovery_kernel = "0.1.208";
extern fn r4os_rust_update_paths(op: u32, data: [*]const u8, len: usize) i32;
pub fn companionKind(path: []const u8) ?CompanionKind {
    const value = r4os_rust_update_paths(0, path.ptr, path.len);
    return if (value < 0) null else @enumFromInt(@as(u32, @intCast(value)));
}
pub fn preloadTarget(path: []const u8) bool {
    return r4os_rust_update_paths(1, path.ptr, path.len) != 0;
}
