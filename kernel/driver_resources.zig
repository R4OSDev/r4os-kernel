// Stateless facade. Exact DriverApi execution admission spans every resource
// operation; canonical bindings, validation, clocks and results belong to Rust.
const a = @import("r4os_kernel_contract");
const resource_state = @import("driver_resource_state.zig");
const bridge = @import("driver_resources_bridge.zig");
extern fn r4os_rust_driver_resource_state() *resource_state.State;
extern fn r4os_rust_driver_resource_stat(owner: u32, name: [*]const u8, len: usize, output: *a.DriverResourceInfo) i32;
extern fn r4os_rust_driver_resource_read(owner: u32, id: u64, offset: u64, output: [*]u8, len: usize, deadline: u64, plan: *anyopaque) i32;
extern fn r4os_rust_driver_resource_now() u64;
extern fn r4os_rust_driver_resource_acpi_stat(owner: u32, signature: u32, index: u32, output: *a.DriverFirmwareTableInfo) i32;
extern fn r4os_rust_driver_resource_acpi_read(owner: u32, handle: u64, offset: u64, output: [*]u8, len: usize, deadline: u64) i32;
const StateFacade = struct {
    pub fn bind(_: @This(), owner: u32, slot: usize, generation: u32) resource_state.Error!void {
        return r4os_rust_driver_resource_state().bind(owner, slot, generation);
    }
    pub fn current(_: @This(), owner: u32) resource_state.Error!*const resource_state.Binding {
        return r4os_rust_driver_resource_state().current(owner);
    }
    pub fn handle(_: @This(), owner: u32, index: usize) resource_state.Error!u64 {
        return r4os_rust_driver_resource_state().handle(owner, index);
    }
    pub fn resolve(_: @This(), owner: u32, id: u64) resource_state.Error!usize {
        return r4os_rust_driver_resource_state().resolve(owner, id);
    }
    pub fn close(_: @This(), owner: u32) void {
        r4os_rust_driver_resource_state().close(owner);
    }
    pub fn finish(_: @This(), owner: u32) void {
        r4os_rust_driver_resource_state().finish(owner);
    }
};
pub const state = StateFacade{};
pub fn stat(owner: u32, name: []const u8, output: *a.DriverResourceInfo) i32 {
    return r4os_rust_driver_resource_stat(owner, name.ptr, name.len, output);
}
pub fn readAt(owner: u32, id: u64, offset: u64, output: []u8, deadline_ns: u64) i32 {
    var plan: bridge.ReadPlan = undefined;
    return r4os_rust_driver_resource_read(owner, id, offset, output.ptr, output.len, deadline_ns, &plan);
}
pub fn nowNs() callconv(.c) u64 {
    return r4os_rust_driver_resource_now();
}
pub fn acpiStat(owner: u32, signature: u32, index: u32, output: *a.DriverFirmwareTableInfo) i32 {
    return r4os_rust_driver_resource_acpi_stat(owner, signature, index, output);
}
pub fn acpiReadAt(owner: u32, handle: u64, offset: u64, output: []u8, deadline_ns: u64) i32 {
    return r4os_rust_driver_resource_acpi_read(owner, handle, offset, output.ptr, output.len, deadline_ns);
}
