const std = @import("std");
const a = @import("r4os_kernel_contract");
const modules = @import("modules.zig");
const module_file = @import("module_file.zig");
const catalog = @import("module_resources.zig");
const clock = @import("../platform/monotonic.zig");
pub var state: @import("driver_resource_state.zig").State = .{};

// Every entry except the read-only clock runs under DriverApi's owner guard,
// including complete synchronous filesystem reads and owner shutdown.
pub fn stat(owner: u32, name: []const u8, out: *a.DriverResourceInfo) i32 {
    if (out.version != 1 or out.size < @sizeOf(a.DriverResourceInfo) or !catalog.validName(name)) return a.driver_resource_error_invalid;
    out.* = .{};
    const binding = state.current(owner) catch return a.driver_resource_error_stale;
    const view = modules.driverResourceView(binding.slot, binding.module_generation) orelse return a.driver_resource_error_source;
    const index = view.catalog.find(name) orelse return a.driver_resource_error_not_found;
    out.* = .{
        .handle = state.handle(owner, index) catch return a.driver_resource_error_stale,
        .byte_length = view.catalog.records[index].bytes,
        .module_generation = binding.module_generation,
    };
    return a.driver_resource_ok;
}

pub fn readAt(owner: u32, id: u64, offset: u64, output: []u8, deadline_ns: u64) i32 {
    if (output.len == 0 or output.len > a.driver_resource_max_read_bytes or deadline_ns == 0 or deadline_ns == std.math.maxInt(u64)) return a.driver_resource_error_invalid;
    const binding = state.current(owner) catch return a.driver_resource_error_stale;
    const index = state.resolve(owner, id) catch return a.driver_resource_error_stale;
    const view = modules.driverResourceView(binding.slot, binding.module_generation) orelse return a.driver_resource_error_source;
    if (index >= view.catalog.count or view.catalog.records[index].kind != 3) return a.driver_resource_error_stale;
    const record = view.catalog.records[index];
    if (offset > record.bytes or output.len > record.bytes - offset) return a.driver_resource_error_invalid;
    const started = nowNs();
    if (started >= deadline_ns) return a.driver_resource_error_deadline;
    // The captured Volume includes MountRef generation. Each range obtains a
    // real filesystem request lease, rejecting stale/remounted sources. NTFS
    // additionally checks the captured MFT sequence; no pathname is reopened.
    // The synchronous storage core retains buffers until hardware use ends.
    const ok = module_file.readExact(.{
        .source = view.source,
        .offset = view.file_offset + record.offset + @as(usize, @intCast(offset)),
        .out = output,
        .name = "driver-resource",
        .verbose = false,
    });
    const finished = nowNs();
    if (finished < started or finished >= deadline_ns) return a.driver_resource_error_deadline;
    if (!ok) return a.driver_resource_error_io;
    return @intCast(output.len);
}

pub fn nowNs() callconv(.c) u64 {
    return clock.nowNanoseconds() orelse std.math.maxInt(u64);
}
