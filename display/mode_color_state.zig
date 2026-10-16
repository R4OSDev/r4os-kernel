//! Fixed private validation projection, with unchanged Contract and BO records.
const a = @import("r4os_kernel_contract");
const bo = @import("../memory/gfx_buffer_layout.zig");
pub const Error = error{ Invalid, Stale, Unsupported };
extern fn r4os_rust_display_mode_color_validate(*const a.GfxModeColorRequest, *const a.GfxOutputColorState, *const bo.Descriptor) i32;
pub fn validate(request: *const a.GfxModeColorRequest, source: a.GfxOutputColorState, image: bo.Descriptor) Error!void {
    return switch (r4os_rust_display_mode_color_validate(request, &source, &image)) {
        0 => {}, 1 => error.Invalid, 3 => error.Unsupported, 7 => error.Stale, else => unreachable,
    };
}
