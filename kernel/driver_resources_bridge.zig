// Still-Zig module-catalog and filesystem operations only, until their owner
// waves. A plan keeps the exact captured Volume/MountRef/MFT identity through
// synchronous I/O; it is caller-local and never reopens a pathname.
const a = @import("r4os_kernel_contract");
const modules = @import("modules.zig");
const module_file = @import("module_file.zig");
const catalog = @import("module_resources.zig");
pub const ReadPlan = struct { source: module_file.FileSource, file_offset: usize, record_offset: usize };
const Lookup = extern struct { index: usize, bytes: u64 };
export fn r4os_zig_driver_resource_name_valid(name: [*]const u8, len: usize) callconv(.c) bool {
    return catalog.validName(name[0..len]);
}
export fn r4os_zig_driver_resource_lookup(slot: usize, generation: u32, name: [*]const u8, len: usize, output: *Lookup) callconv(.c) i32 {
    const view = modules.driverResourceView(slot, generation) orelse return a.driver_resource_error_source;
    const index = view.catalog.find(name[0..len]) orelse return a.driver_resource_error_not_found;
    output.* = .{ .index = index, .bytes = view.catalog.records[index].bytes };
    return a.driver_resource_ok;
}
export fn r4os_zig_driver_resource_prepare(slot: usize, generation: u32, index: usize, opaque_plan: *anyopaque, bytes: *u64) callconv(.c) i32 {
    const view = modules.driverResourceView(slot, generation) orelse return a.driver_resource_error_source;
    if (index >= view.catalog.count or view.catalog.records[index].kind != 3) return a.driver_resource_error_stale;
    const record = view.catalog.records[index];
    const plan: *ReadPlan = @ptrCast(@alignCast(opaque_plan));
    plan.* = .{ .source = view.source, .file_offset = view.file_offset, .record_offset = record.offset };
    bytes.* = record.bytes;
    return a.driver_resource_ok;
}
export fn r4os_zig_driver_resource_read(opaque_plan: *const anyopaque, offset: u64, output: [*]u8, len: usize) callconv(.c) bool {
    const plan: *const ReadPlan = @ptrCast(@alignCast(opaque_plan));
    return module_file.readExact(.{ .source = plan.source, .offset = plan.file_offset + plan.record_offset + @as(usize, @intCast(offset)), .out = output[0..len], .name = "driver-resource", .verbose = false });
}
