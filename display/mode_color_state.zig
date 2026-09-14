//! Copied color-mode admission. Only integer layout and published source
//! facts live here; transfer functions, EDID and link policy stay in userland.
const std = @import("std");
const a = @import("r4os_kernel_contract");
const bo = @import("../memory/gfx_buffer_layout.zig");
pub const Error = error{ Invalid, Stale, Unsupported };

pub fn validate(request: *const a.GfxModeColorRequest, source: a.GfxOutputColorState, image: bo.Descriptor) Error!void {
    if (request.version != 1 or request.size != @sizeOf(a.GfxModeColorRequest) or request.state.count != 1 or
        request.image.id == 0 or request.image.generation == 0 or request.image.reserved0 != 0) return error.Invalid;
    const assignment = request.state.assignments[0];
    if (!std.meta.eql(source.identity, assignment.output) or source.revision != request.state.topology_revision) return error.Stale;
    const signal = request.signal;
    if (signal.version != 1 or signal.size != @sizeOf(a.GfxColorSignal) or signal.reserved0 != 0 or signal.metadata_valid > 1 or
        signal.reference_white == 0 or signal.peak < signal.reference_white or signal.peak > 100_000_000 or signal.black >= signal.reference_white)
        return error.Invalid;
    if (source.version != 1 or source.size < @sizeOf(a.GfxOutputColorState) or source.flags & 7 != 7 or signal.pipeline != 7) return error.Unsupported;
    const format: u32 = switch (signal.format) { a.gfx_buffer_format_xrgb8888 => 1, a.gfx_buffer_format_xrgb2101010 => 2, else => return error.Unsupported };
    const depth: u32 = switch (signal.bpc) { 8 => 1, 10 => 2, else => return error.Unsupported };
    const primaries: u32 = switch (signal.primaries) { 1 => 1, 3 => 2, else => return error.Unsupported };
    const transfer: u32 = switch (signal.transfer) { 1 => 1, 3 => 4, 4 => 8, else => return error.Unsupported };
    const range: u32 = switch (signal.range) { 1 => 1, 2 => 2, else => return error.Unsupported };
    if (format != depth or source.formats & format == 0 or source.depths & depth == 0 or source.color_spaces & primaries == 0 or
        source.transfers & transfer == 0 or source.ranges & range == 0) return error.Unsupported;
    if (transfer == 1) {
        if (signal.metadata_valid != 0 or !std.meta.eql(signal.metadata, a.GfxHdrMetadata{})) return error.Invalid;
    } else if (format != 2 or primaries != 2 or signal.metadata_valid != 1 or
        source.flags & (a.gfx_output_color_hdmi_metadata | a.gfx_output_color_dp_metadata) == 0) return error.Unsupported;
    if (@intFromEnum(image.format) != signal.format or image.location != .system or !image.binding.portable() or image.modifier != 0 or
        image.plane_count != 1 or image.planes[0].offset != 0 or image.width != assignment.source_width or image.height != assignment.source_height or
        image.planes[0].pitch != @as(u64, image.width) * 4 or image.bytes != @as(u128, image.planes[0].pitch) * image.height or
        image.usage & (bo.Usage.cpu_write | bo.Usage.transfer_source) != bo.Usage.cpu_write | bo.Usage.transfer_source) return error.Unsupported;
}
