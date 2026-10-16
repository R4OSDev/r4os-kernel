// Typed C projection into the single native metadata owner.
const abi = @import("r4os_kernel_contract");
const buffers = @import("../memory/gfx_buffers.zig");
const Args = buffers.lifetime.NativeArgs;
extern fn r4os_rust_gfx_telemetry_api(u32, *const Args, ?*anyopaque) i32;
pub fn query(input: *const abi.GfxTelemetryRequest, output: *abi.GfxTelemetryState) callconv(.c) i32 {
    const args = Args{ .input = input };
    return r4os_rust_gfx_telemetry_api(0, &args, output);
}
pub fn publish(owner: buffers.Owner, input: *const abi.GfxTelemetryState, output: *abi.GfxTelemetryDemand) i32 {
    const args = Args{ .owner = owner, .input = input };
    return r4os_rust_gfx_telemetry_api(1, &args, output);
}
pub fn closeDriver(id: u32) void {
    const args = Args{ .value = id };
    _ = r4os_rust_gfx_telemetry_api(2, &args, null);
}
