// Stateless PPI1/Contract facade; Rust owns table callbacks and execution slot.
comptime {
    _ = @import("protocol_api_bridge.zig");
}
const r4x_api = @import("../program/r4x_api.zig");
pub const MAGIC: u32 = 0x31495050;
pub const VERSION: u32 = 1;
pub const ProtocolStatus = r4x_api.ProtocolStatus;
pub const ProtocolBuffer = r4x_api.ProtocolBuffer;
// Temporary fixed C view of the unchanged PPI1 wire prefix; native table
// uses the canonical Contract ProtocolApi, which the Kernel Zig facade lacks.
pub const Table = extern struct {
    magic: u32,
    version: u32,
    size: u32,
    reserved: u32,
    log_info: *const fn ([*:0]const u8) callconv(.c) void,
    log_warn: *const fn ([*:0]const u8) callconv(.c) void,
    log_error: *const fn ([*:0]const u8) callconv(.c) void,
    register_role: *const fn ([*:0]const u8, u32, u32) callconv(.c) i32,
    set_status: *const fn (u32, [*:0]const u8) callconv(.c) i32,
    dependency_status: *const fn ([*:0]const u8) callconv(.c) i32,
    alloc: *const fn (u32, u32) callconv(.c) ?*anyopaque,
    free: *const fn (?*anyopaque, u32) callconv(.c) void,
    file_read: *const fn ([*:0]const u8, [*]u8, u32) callconv(.c) i32,
};
extern fn r4os_rust_protocol_table() *Table;
extern fn r4os_rust_protocol_slot(operation: u8, slot: usize) void;
pub fn table() *Table {
    return r4os_rust_protocol_table();
}
pub fn enter(slot: usize) void {
    r4os_rust_protocol_slot(0, slot);
}
pub fn leave() void {
    r4os_rust_protocol_slot(1, 0);
}
comptime {
    if (@sizeOf(Table) != 88 or @offsetOf(Table, "file_read") != 80)
        @compileError("Canonical PPI1 Contract layout drift");
}
