//! Typed R4D compatibility facade. Native Rust owns discovery, descriptors,
//! runtime generations, driver admission and the complete terminal lifecycle.
const api = @import("r4x_api.zig");
const driver_api = @import("../kernel/driver_api.zig");

pub const RuntimeLoadResult = enum(u8) {
    loaded,
    already_active,
    name_invalid,
    file_missing,
    invalid_file,
    init_failed,
    load_failed,
};
pub const ModuleInfoResult = union(enum) { absent, busy, found: api.DriverModuleInfo };
const Bytes = extern struct { data: [*]const u8, len: usize };
fn bytes(value: []const u8) Bytes { return .{ .data = value.ptr, .len = value.len }; }
extern fn r4os_rust_r4d_discover() void;
extern fn r4os_rust_r4d_load_path(Bytes) bool;
extern fn r4os_rust_r4d_load(Bytes) RuntimeLoadResult;
extern fn r4os_rust_r4d_preload(Bytes, Bytes) RuntimeLoadResult;
extern fn r4os_rust_r4d_unload(Bytes) bool;
extern fn r4os_rust_r4d_transition() bool;
extern fn r4os_rust_r4d_succeeded(RuntimeLoadResult) bool;
extern fn r4os_rust_r4d_result_name(RuntimeLoadResult) Bytes;
extern fn r4os_rust_r4d_info(u32, *api.DriverModuleInfo) u8;

pub fn discoverAll() void { r4os_rust_r4d_discover(); }
pub fn loadPath(path: []const u8) bool { return r4os_rust_r4d_load_path(bytes(path)); }
pub fn loadRuntimeName(name: []const u8) bool { return runtimeLoadSucceeded(loadRuntimeNameResult(name)); }
pub fn loadRuntimeNameResult(name: []const u8) RuntimeLoadResult { return r4os_rust_r4d_load(bytes(name)); }
pub fn loadPreloadModule(name: []const u8, data: []const u8) RuntimeLoadResult { return r4os_rust_r4d_preload(bytes(name), bytes(data)); }
pub fn unloadRuntimeName(name: []const u8) bool { return r4os_rust_r4d_unload(bytes(name)); }
pub fn shutdownForSystemTransition() bool { return r4os_rust_r4d_transition(); }
pub fn runtimeLoadSucceeded(result: RuntimeLoadResult) bool { return r4os_rust_r4d_succeeded(result); }
pub fn runtimeLoadResultName(result: RuntimeLoadResult) []const u8 {
    const value = r4os_rust_r4d_result_name(result);
    return value.data[0..value.len];
}
pub fn moduleInfo(owner: u32) ModuleInfoResult {
    var result: api.DriverModuleInfo = undefined;
    return switch (r4os_rust_r4d_info(owner, &result)) {
        0 => .absent,
        1 => .busy,
        2 => .{ .found = result },
        else => unreachable,
    };
}
// Stateless projection of the canonical generated callback template. The
// sole initialized DriverApi table already belongs to the native owner.
export fn r4os_zig_r4d_table() callconv(.c) *const driver_api.Table { return driver_api.table(); }

comptime {
    if (@sizeOf(Bytes) != 16 or @sizeOf(api.DriverModuleInfo) != 128) @compileError("R4D facade C layout drift");
}
