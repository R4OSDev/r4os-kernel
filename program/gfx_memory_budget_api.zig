// Typed C projection into the single native metadata owner.
const abi = @import("r4os_kernel_contract");
const buffers = @import("../memory/gfx_buffers.zig");
const Args = buffers.lifetime.NativeArgs;
extern fn r4os_rust_gfx_budget_api(*const Args, bool, *abi.GfxDeviceBudgetState) i32;
pub fn query(input: *const abi.GfxDeviceBudgetRequest, output: *abi.GfxDeviceBudgetState) callconv(.c) i32 {
    const args = Args{ .input = input };
    return r4os_rust_gfx_budget_api(&args, false, output);
}
pub fn provider(identity: buffers.Owner, input: *const abi.GfxDeviceBudgetRequest, output: *abi.GfxDeviceBudgetState) i32 {
    const args = Args{ .owner = identity, .input = input };
    return r4os_rust_gfx_budget_api(&args, true, output);
}
