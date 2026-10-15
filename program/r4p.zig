//! Typed R4P facade. Native Rust owns preload admission, the lazy catalog,
//! activation, dependency resolution and protocol callback ordering.
const protocol_api = @import("../kernel/protocol_api.zig");
const registry = @import("../protocol/registry.zig");
const loader_perf = @import("../kernel/loader_perf.zig");

pub const ROLE_STATE_MISSING: u8 = 0;
pub const ROLE_STATE_LOADED: u8 = 1;
pub const ROLE_STATE_ACTIVE: u8 = 2;
pub const ROLE_STATE_BLOCKED: u8 = 3;
pub const ROLE_STATE_ERROR: u8 = 4;
pub const ROLE_STATE_DISABLED: u8 = 5;
pub const RoleRuntimeStatus = struct {
    active_r4p: bool = false,
    builtin_fallback: bool = false,
    state: u8 = ROLE_STATE_MISSING,
};
const Status = extern struct { active_r4p: bool, builtin_fallback: bool, state: u8 };
const Bytes = extern struct { data: [*]const u8, len: usize };
fn bytes(value: []const u8) Bytes { return .{ .data = value.ptr, .len = value.len }; }
extern fn r4os_rust_r4p_load_all() void;
extern fn r4os_rust_r4p_preload(Bytes, Bytes) bool;
extern fn r4os_rust_r4p_finish_preload() void;
extern fn r4os_rust_r4p_dump() void;
extern fn r4os_rust_r4p_dispatch(Bytes, u32, *const protocol_api.ProtocolBuffer, *protocol_api.ProtocolBuffer) i32;
extern fn r4os_rust_r4p_has_active(Bytes) bool;
extern fn r4os_rust_r4p_source(Bytes, bool) Bytes;
extern fn r4os_rust_r4p_status(Bytes) Status;

pub fn loadAll() void { r4os_rust_r4p_load_all(); }
pub fn loadPreloadModule(name: []const u8, data: []const u8) bool { return r4os_rust_r4p_preload(bytes(name), bytes(data)); }
pub fn finishPreload() void { r4os_rust_r4p_finish_preload(); }
pub fn dumpStatus() void { r4os_rust_r4p_dump(); }
pub fn dispatch(role: []const u8, op: u32, input: *const protocol_api.ProtocolBuffer, output: *protocol_api.ProtocolBuffer) i32 {
    return r4os_rust_r4p_dispatch(bytes(role), op, input, output);
}
pub fn hasActiveR4p(role: []const u8) bool { return r4os_rust_r4p_has_active(bytes(role)); }
pub fn activeSourceName(role: []const u8) []const u8 {
    const value = r4os_rust_r4p_source(bytes(role), false);
    return value.data[0..value.len];
}
pub fn requiredSourceName(role: []const u8) []const u8 {
    const value = r4os_rust_r4p_source(bytes(role), true);
    return value.data[0..value.len];
}
pub fn roleRuntimeStatus(role: []const u8) RoleRuntimeStatus {
    const value = r4os_rust_r4p_status(bytes(role));
    return .{ .active_r4p = value.active_r4p, .builtin_fallback = value.builtin_fallback, .state = value.state };
}

// Stateless projections of the existing instrumentation and registry formatter.
export fn r4os_zig_r4p_dump() callconv(.c) void { registry.dumpStatus(); }
export fn r4os_zig_r4p_perf(operation: u8, a: u64, b: u64, c: u64, d: u64) callconv(.c) u64 {
    switch (operation) {
        0 => return loader_perf.now(),
        1 => loader_perf.addR4pScanTicks(a),
        2 => loader_perf.addR4pReadTicks(a),
        3 => loader_perf.addR4pResolveTicks(a),
        5 => loader_perf.addR4pInitTicks(a),
        6 => loader_perf.recordR4pResults(@intCast(a), @intCast(b), @intCast(c), @intCast(d)),
        7 => loader_perf.recordR4pScanEntry(),
        8 => loader_perf.recordR4pCandidate(),
        else => unreachable,
    }
    return 0;
}
comptime {
    if (@sizeOf(Bytes) != 16 or @sizeOf(Status) != 3 or @alignOf(Status) != 1)
        @compileError("R4P facade C layout drift");
}
