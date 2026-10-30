const r4os = @import("r4os");
const p = @import("shared.zig");
const a = r4os.abi;
// The fixture driver explicitly lends its resident State until unregister.
// Only the scoped test guest loads these modules; no product API is added.
var resident: ?*p.State = null;
comptime { asm(r4os.r4dev.protocolEntriesAsm("p52_init", "p52_shutdown", "p52_query", "p52_dispatch")); }
export fn p52_init(api: *const r4os.r4dev.ProtocolApi) callconv(.c) i32 {
    const ctx = r4os.r4dev.ProtocolContext.init(api);
    if (ctx.registerRole(p.role, .audio, 0) != 0) return -1;
    return ctx.setStatus(.active, "Temporary real audio callback observer");
}
export fn p52_shutdown() callconv(.c) i32 { resident = null; return 0; }
export fn p52_query(out: *a.ProtocolStatus) callconv(.c) i32 {
    out.* = .{ .state = @intFromEnum(a.ProtocolState.active) }; return 0;
}
fn input(comptime T: type, buffer: *const a.ProtocolBuffer) ?*const T {
    if (buffer.data == null or buffer.len != @sizeOf(T)) return null;
    if (@intFromPtr(buffer.data.?) % @alignOf(T) != 0) return null;
    return @ptrCast(@alignCast(buffer.data.?));
}
export fn p52_dispatch(op: u32, in_buffer: *const a.ProtocolBuffer, out: *a.ProtocolBuffer) callconv(.c) i32 {
    if (op == p.ping) return 0;
    if (op == p.register) {
        const value = input(p.Registration, in_buffer) orelse return -1;
        if (value.address == 0 or value.address % @alignOf(p.State) != 0 or value.bytes != @sizeOf(p.State)) return -2;
        if (resident != null and @intFromPtr(resident.?) != value.address) return -2;
        const state: *p.State = @ptrFromInt(value.address);
        if (p.load(&state.magic) != p.magic or p.load(&state.size) != @sizeOf(p.State)) return -3;
        resident = state; return 0;
    }
    if (op == p.unregister) { resident = null; return 0; }
    const state = resident orelse return -4;
    if (op == p.configure) {
        const controls = input(p.Controls, in_buffer) orelse return -5;
        p.set(state, controls.*); return 0;
    }
    if (op == p.snapshot) {
        if (out.data == null or out.capacity < @sizeOf(p.State) or @intFromPtr(out.data.?) % @alignOf(p.State) != 0) return -6;
        const copy: *p.State = @ptrCast(@alignCast(out.data.?));
        copy.* = p.copy(state); out.len = @sizeOf(p.State); return 0;
    }
    return -7;
}
