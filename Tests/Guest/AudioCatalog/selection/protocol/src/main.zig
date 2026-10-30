const r4os = @import("r4os");
const p = @import("shared.zig");
const a = r4os.abi;
var resident: ?*p.State = null;
comptime {
    asm (r4os.r4dev.protocolEntriesAsm("p56p_init", "p56p_shutdown", "p56p_query", "p56p_dispatch"));
}
export fn p56p_init(api: *const r4os.r4dev.ProtocolApi) callconv(.c) i32 {
    const ctx = r4os.r4dev.ProtocolContext.init(api);
    if (ctx.registerRole(p.role, .audio, 0) != 0) return -1;
    return ctx.setStatus(.active, "Private actual output-query observer");
}
export fn p56p_shutdown() callconv(.c) i32 {
    resident = null;
    return 0;
}
export fn p56p_query(out: *a.ProtocolStatus) callconv(.c) i32 {
    out.* = .{ .state = @intFromEnum(a.ProtocolState.active) };
    return 0;
}
fn input(comptime T: type, buffer: *const a.ProtocolBuffer) ?*const T {
    if (buffer.data == null or buffer.len != @sizeOf(T) or @intFromPtr(buffer.data.?) % @alignOf(T) != 0) return null;
    return @ptrCast(@alignCast(buffer.data.?));
}
export fn p56p_dispatch(op: u32, data: *const a.ProtocolBuffer, out: *a.ProtocolBuffer) callconv(.c) i32 {
    if (op == p.ping) return 0;
    if (op == p.unregister) {
        resident = null;
        return 0;
    }
    if (op == p.register) {
        const value = input(p.Registration, data) orelse return -1;
        if (value.address == 0 or value.address % @alignOf(p.State) != 0 or value.bytes != @sizeOf(p.State)) return -2;
        const state: *p.State = @ptrFromInt(value.address);
        if (p.load(&state.magic) != p.magic or p.load(&state.size) != @sizeOf(p.State)) return -3;
        if (resident != null and @intFromPtr(resident.?) != value.address) return -4;
        resident = state;
        return 0;
    }
    const state = resident orelse return -5;
    if (op == p.release_select) {
        @atomicStore(u32, &state.select_released, 1, .release);
        return 0;
    }
    if (op == p.rebind) {
        _ = @atomicRmw(u32, &state.rebind_request, .Add, 1, .acq_rel);
        return 0;
    }
    if (op == p.configure) {
        const value = input(p.Controls, data) orelse return -6;
        if (value.output_count > 32 or value.secondary_count > 32) return -7;
        if (!p.set(state, value.*)) return a.service_api_result_busy;
        return 0;
    }
    if (op == p.snapshot) {
        if (out.data == null or out.capacity < @sizeOf(p.State) or @intFromPtr(out.data.?) % @alignOf(p.State) != 0) return -8;
        const result: *p.State = @ptrCast(@alignCast(out.data.?));
        result.* = p.copy(state);
        out.len = @sizeOf(p.State);
        return 0;
    }
    return -9;
}
