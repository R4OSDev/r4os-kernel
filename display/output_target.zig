//! Copied identity facade; native comparisons never inspect padding.
const abi = @import("r4os_kernel_contract");
extern fn r4os_rust_display_target_valid(*const abi.GfxOutputTarget) bool;
extern fn r4os_rust_display_target_same(*const abi.GfxOutputTarget, *const abi.GfxOutputTarget) bool;
extern fn r4os_rust_display_target_from(*const abi.GfxOutputId, u32, u64, *abi.GfxOutputTarget) void;
pub fn valid(value: abi.GfxOutputTarget) bool { return r4os_rust_display_target_valid(&value); }
pub fn same(left: abi.GfxOutputTarget, right: abi.GfxOutputTarget) bool { return r4os_rust_display_target_same(&left, &right); }
pub fn fromOutput(value: abi.GfxOutputId, head: u32, generation: u64) abi.GfxOutputTarget {
    var out: abi.GfxOutputTarget = undefined;
    r4os_rust_display_target_from(&value, head, generation, &out); return out;
}
