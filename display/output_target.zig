// Copied routing identity. An explicit target never treats adapter zero or a
// missing connector as a wildcard, and no caller pointer survives admission.
const std = @import("std");
const abi = @import("r4os_kernel_contract");

pub fn valid(value: abi.GfxOutputTarget) bool {
    return value.version == 1 and value.size == @sizeOf(abi.GfxOutputTarget) and value.reserved0 == 0 and
        value.connector_id != 0 and value.device_generation != 0 and value.connection_generation != 0 and
        value.display_generation != 0 and value.head_id < abi.gfx_output_max_assignments;
}
pub fn same(left: abi.GfxOutputTarget, right: abi.GfxOutputTarget) bool {
    return valid(left) and valid(right) and std.meta.eql(left, right);
}
pub fn fromOutput(identity: abi.GfxOutputId, head: u32, generation: u64) abi.GfxOutputTarget {
    return .{ .adapter_id = identity.adapter_id, .connector_id = identity.connector_id,
        .device_generation = identity.device_generation, .connection_generation = identity.connection_generation,
        .head_id = head, .display_generation = generation };
}
