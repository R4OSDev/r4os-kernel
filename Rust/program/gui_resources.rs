//! Canonical GUI batch resource validation. Byte reads retain the wire bounds,
//! exact integer viewport mapping, palette/alpha requirements and finite shape
//! coordinates used by the existing producer and renderer contracts.
use crate::contract::{self as c, GuiFrameCommand as Command};
use core::{mem, ptr};
pub(super) fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    if end > bytes.len() {
        return None;
    }
    Some(u32::from_le_bytes(bytes[offset..end].try_into().ok()?))
}
macro_rules! field {
    ($bytes:expr,$ty:ty,$field:ident) => {
        read_u32($bytes, mem::offset_of!($ty, $field))?
    };
}
pub(super) fn kind(command: &Command) -> Option<u16> {
    Some(match command.kind {
        c::gui_frame_command_kind_clear | c::gui_frame_command_kind_rect => 0,
        c::gui_frame_command_kind_text => {
            if command.resource_bytes == 0 {
                0
            } else {
                1
            }
        }
        c::gui_frame_command_kind_raster => 2,
        c::gui_frame_command_kind_alpha8 => 3,
        c::gui_frame_command_kind_path_fill
        | c::gui_frame_command_kind_path_stroke
        | c::gui_frame_command_kind_rounded_rect
        | c::gui_frame_command_kind_shadow
        | c::gui_frame_command_kind_argb32 => 4,
        c::gui_frame_command_kind_indexed8 => 5,
        c::gui_frame_command_kind_xrgb32_nearest => 6,
        c::gui_frame_command_kind_shared_raster => 7,
        _ => return None,
    })
}
pub(super) fn resource<'a>(command: &Command, resources: &'a [u8]) -> Option<&'a [u8]> {
    if command.resource_bytes == 0 {
        return Some(&[]);
    }
    let end = command
        .resource_offset
        .checked_add(command.resource_bytes)?;
    if end > resources.len() as u64 {
        return None;
    }
    Some(&resources[usize::try_from(command.resource_offset).ok()?..usize::try_from(end).ok()?])
}
pub(super) fn shared_descriptor(resource: &[u8]) -> Option<c::GuiSharedRasterResource> {
    if resource.len() != mem::size_of::<c::GuiSharedRasterResource>() {
        return None;
    }
    // Canonical integer-only C record; the target and wire are little endian.
    Some(unsafe { ptr::read_unaligned(resource.as_ptr().cast()) })
}
fn plain(command: &Command, tint: bool) -> bool {
    (tint || command.rgb == 0)
        && command.fg == 0
        && command.bg == 0
        && command.font_id == 0
        && command.text_w == 0
        && command.text_h == 0
        && command.baseline == 0
        && command.line_height == 0
        && command.parameter0 == 0
        && command.parameter1 == 0
}
struct Viewport {
    source_x: u32,
    source_y: u32,
    source_w: u32,
    source_h: u32,
    guest_w: u32,
    guest_h: u32,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
}
fn source_bounds(view: &Viewport) -> Option<(u64, u64)> {
    let right = u64::from(view.source_x).checked_add(u64::from(view.source_w))?;
    let bottom = u64::from(view.source_y).checked_add(u64::from(view.source_h))?;
    if right > u64::from(view.guest_w) || bottom > u64::from(view.guest_h) {
        return None;
    }
    Some((right, bottom))
}
fn command_bounds(command: &Command, view: &Viewport) -> Option<(i64, i64)> {
    let right = i64::from(command.x).checked_add(i64::from(command.w))?;
    let bottom = i64::from(command.y).checked_add(i64::from(command.h))?;
    let viewport_right = i64::from(view.x).checked_add(i64::from(view.w))?;
    let viewport_bottom = i64::from(view.y).checked_add(i64::from(view.h))?;
    if command.x < view.x
        || command.y < view.y
        || right > viewport_right
        || bottom > viewport_bottom
    {
        return None;
    }
    Some((right, bottom))
}
fn mapped(
    command: &Command,
    view: &Viewport,
    right: i64,
    bottom: i64,
    source_right: u64,
    source_bottom: u64,
) -> Option<()> {
    let left = u64::try_from(i64::from(command.x) - i64::from(view.x)).expect("Viewport left");
    let top = u64::try_from(i64::from(command.y) - i64::from(view.y)).expect("Viewport top");
    let right = u64::try_from(right - i64::from(view.x)).expect("Viewport right");
    let bottom = u64::try_from(bottom - i64::from(view.y)).expect("Viewport bottom");
    let mapped_left = (left * u64::from(view.guest_w)) / u64::from(view.w);
    let mapped_top = (top * u64::from(view.guest_h)) / u64::from(view.h);
    let mapped_right = ((right - 1) * u64::from(view.guest_w)) / u64::from(view.w);
    let mapped_bottom = ((bottom - 1) * u64::from(view.guest_h)) / u64::from(view.h);
    if mapped_left < u64::from(view.source_x)
        || mapped_top < u64::from(view.source_y)
        || mapped_right >= source_right
        || mapped_bottom >= source_bottom
    {
        None
    } else {
        Some(())
    }
}
pub(super) fn shared(command: &Command, resource: &[u8]) -> Option<()> {
    if command.w == 0
        || command.h == 0
        || command.w > 128
        || command.h > 128
        || !plain(command, true)
        || command.resource_bytes != u64::from(c::gui_shared_raster_resource_size)
    {
        return None;
    }
    let descriptor = shared_descriptor(resource)?;
    if descriptor.version != c::gui_shared_raster_resource_version
        || descriptor.size != c::gui_shared_raster_resource_size
        || descriptor.handle.id == 0
        || descriptor.handle.generation == 0
        || descriptor.raster_generation == 0
        || descriptor.flags != 0
        || descriptor.source_w == 0
        || descriptor.source_h == 0
        || descriptor.guest_w == 0
        || descriptor.guest_h == 0
        || descriptor.viewport_w == 0
        || descriptor.viewport_h == 0
    {
        return None;
    }
    if !matches!(
        descriptor.format,
        c::gui_shared_raster_format_xrgb32
            | c::gui_shared_raster_format_indexed8
            | c::gui_shared_raster_format_alpha8
    ) {
        return None;
    }
    if descriptor.format != c::gui_shared_raster_format_alpha8 && command.rgb != 0 {
        return None;
    }
    let view = Viewport {
        source_x: descriptor.source_x,
        source_y: descriptor.source_y,
        source_w: descriptor.source_w,
        source_h: descriptor.source_h,
        guest_w: descriptor.guest_w,
        guest_h: descriptor.guest_h,
        x: descriptor.viewport_x,
        y: descriptor.viewport_y,
        w: descriptor.viewport_w,
        h: descriptor.viewport_h,
    };
    let (source_right, source_bottom) = source_bounds(&view)?;
    let (right, bottom) = command_bounds(command, &view)?;
    if descriptor.format == c::gui_shared_raster_format_alpha8 {
        return (view.w == view.guest_w
            && view.h == view.guest_h
            && i64::from(command.x) == i64::from(view.x) + i64::from(view.source_x)
            && i64::from(command.y) == i64::from(view.y) + i64::from(view.source_y)
            && command.w == view.source_w
            && command.h == view.source_h)
            .then_some(());
    }
    mapped(command, &view, right, bottom, source_right, source_bottom)
}
pub(super) fn indexed(command: &Command, resource: &[u8]) -> Option<()> {
    if command.w == 0
        || command.h == 0
        || command.w > 128
        || command.h > 128
        || !plain(command, false)
        || resource.len() < c::gui_indexed8_pixels_offset as usize
    {
        return None;
    }
    type T = c::GuiIndexed8Resource;
    if field!(resource, T, version) != c::gui_indexed8_resource_version
        || field!(resource, T, size) != c::gui_indexed8_resource_size
    {
        return None;
    }
    let view = Viewport {
        source_x: field!(resource, T, source_x),
        source_y: field!(resource, T, source_y),
        source_w: field!(resource, T, source_w),
        source_h: field!(resource, T, source_h),
        guest_w: field!(resource, T, guest_w),
        guest_h: field!(resource, T, guest_h),
        x: field!(resource, T, viewport_x) as i32,
        y: field!(resource, T, viewport_y) as i32,
        w: field!(resource, T, viewport_w),
        h: field!(resource, T, viewport_h),
    };
    let palette_entries = field!(resource, T, palette_entries);
    let palette_offset = field!(resource, T, palette_offset);
    let pixels_offset = field!(resource, T, pixels_offset);
    let stride = field!(resource, T, pixel_stride);
    if view.source_w == 0
        || view.source_h == 0
        || view.source_w > 128
        || view.source_h > 128
        || view.guest_w == 0
        || view.guest_h == 0
        || view.w == 0
        || view.h == 0
        || palette_entries != c::gui_indexed8_palette_entries
        || palette_offset != c::gui_indexed8_palette_offset
        || pixels_offset != c::gui_indexed8_pixels_offset
        || stride != view.source_w
    {
        return None;
    }
    let (source_right, source_bottom) = source_bounds(&view)?;
    let pixels = u64::from(view.source_w).checked_mul(u64::from(view.source_h))?;
    let required = u64::from(pixels_offset).checked_add(pixels)?;
    if required != resource.len() as u64 || command.resource_bytes != required {
        return None;
    }
    let (right, bottom) = command_bounds(command, &view)?;
    mapped(command, &view, right, bottom, source_right, source_bottom)?;
    let mut palette_byte = (palette_offset + 3) as usize;
    while palette_byte < pixels_offset as usize {
        if resource[palette_byte] != 0 {
            return None;
        }
        palette_byte += 4;
    }
    Some(())
}
pub(super) fn xrgb(command: &Command, resource: &[u8]) -> Option<()> {
    if command.w == 0
        || command.h == 0
        || command.w > 128
        || command.h > 128
        || !plain(command, false)
        || resource.len() < c::gui_xrgb32_pixels_offset as usize
    {
        return None;
    }
    type T = c::GuiXrgb32Resource;
    if field!(resource, T, version) != c::gui_xrgb32_resource_version
        || field!(resource, T, size) != c::gui_xrgb32_resource_size
    {
        return None;
    }
    let view = Viewport {
        source_x: field!(resource, T, source_x),
        source_y: field!(resource, T, source_y),
        source_w: field!(resource, T, source_w),
        source_h: field!(resource, T, source_h),
        guest_w: field!(resource, T, guest_w),
        guest_h: field!(resource, T, guest_h),
        x: field!(resource, T, viewport_x) as i32,
        y: field!(resource, T, viewport_y) as i32,
        w: field!(resource, T, viewport_w),
        h: field!(resource, T, viewport_h),
    };
    let pixels_offset = field!(resource, T, pixels_offset);
    let stride = field!(resource, T, pixel_stride);
    let flags = field!(resource, T, flags);
    let reserved = field!(resource, T, reserved0);
    if view.source_w == 0
        || view.source_h == 0
        || view.source_w > 128
        || view.source_h > 128
        || view.guest_w == 0
        || view.guest_h == 0
        || view.w == 0
        || view.h == 0
        || pixels_offset != c::gui_xrgb32_pixels_offset
        || stride < view.source_w
        || stride > 128
        || flags != 0
        || reserved != 0
    {
        return None;
    }
    let (source_right, source_bottom) = source_bounds(&view)?;
    let rows = u64::from(view.source_h - 1).checked_mul(u64::from(stride))?;
    let pixels = rows.checked_add(u64::from(view.source_w))?;
    let bytes = pixels.checked_mul(4)?;
    let required = u64::from(pixels_offset).checked_add(bytes)?;
    if required != resource.len() as u64 || command.resource_bytes != required {
        return None;
    }
    let (right, bottom) = command_bounds(command, &view)?;
    mapped(command, &view, right, bottom, source_right, source_bottom)?;
    let mut alpha_byte = (pixels_offset + 3) as usize;
    while alpha_byte < resource.len() {
        if resource[alpha_byte] != 0 {
            return None;
        }
        alpha_byte += 4;
    }
    Some(())
}
pub(super) fn float(bits: u32) -> Option<f32> {
    let value = f32::from_bits(bits);
    value.is_finite().then_some(value)
}
pub(super) fn coordinate(bits: u32) -> bool {
    float(bits).is_some_and(|value| value.abs() <= c::gui_shape_max_coordinate as f32)
}
pub(super) fn non_negative(bits: u32, maximum: f32) -> bool {
    float(bits).is_some_and(|value| value >= 0.0 && value <= maximum)
}
pub(super) fn path(resource: &[u8], count: u32) -> Option<()> {
    let mut active = false;
    for index in 0..count {
        let relative = (index as usize).checked_mul(mem::size_of::<c::GuiPathSegment>())?;
        let offset = mem::size_of::<c::GuiShapeResource>().checked_add(relative)?;
        macro_rules! part {
            ($name:ident) => {
                read_u32(resource, offset + mem::offset_of!(c::GuiPathSegment, $name))?
            };
        }
        let kind = part!(kind);
        let flags = part!(flags);
        if flags != 0 {
            return None;
        }
        let x1 = part!(x1_bits);
        let y1 = part!(y1_bits);
        let x2 = part!(x2_bits);
        let y2 = part!(y2_bits);
        let x3 = part!(x3_bits);
        let y3 = part!(y3_bits);
        match kind {
            c::gui_path_segment_kind_move => {
                if !coordinate(x1) || !coordinate(y1) || x2 != 0 || y2 != 0 || x3 != 0 || y3 != 0 {
                    return None;
                }
                active = true;
            }
            c::gui_path_segment_kind_line => {
                if !active
                    || !coordinate(x1)
                    || !coordinate(y1)
                    || x2 != 0
                    || y2 != 0
                    || x3 != 0
                    || y3 != 0
                {
                    return None;
                }
            }
            c::gui_path_segment_kind_quadratic => {
                if !active
                    || !coordinate(x1)
                    || !coordinate(y1)
                    || !coordinate(x2)
                    || !coordinate(y2)
                    || x3 != 0
                    || y3 != 0
                {
                    return None;
                }
            }
            c::gui_path_segment_kind_cubic => {
                if !active
                    || !coordinate(x1)
                    || !coordinate(y1)
                    || !coordinate(x2)
                    || !coordinate(y2)
                    || !coordinate(x3)
                    || !coordinate(y3)
                {
                    return None;
                }
            }
            c::gui_path_segment_kind_close => {
                if !active || x1 != 0 || y1 != 0 || x2 != 0 || y2 != 0 || x3 != 0 || y3 != 0 {
                    return None;
                }
                active = false;
            }
            _ => return None,
        }
    }
    Some(())
}
pub(super) fn shape(command: &Command, resource: &[u8]) -> Option<()> {
    type T = c::GuiShapeResource;
    if command.w == 0
        || command.h == 0
        || command.w > c::gui_shape_max_dimension
        || command.h > c::gui_shape_max_dimension
    {
        return None;
    }
    let pixels = u64::from(command.w).checked_mul(u64::from(command.h))?;
    if pixels > u64::from(c::gui_shape_max_pixels)
        || command.parameter0 != 0
        || command.parameter1 != 0
        || !plain(command, false)
        || resource.len() < mem::size_of::<T>()
        || command.resource_bytes != resource.len() as u64
    {
        return None;
    }
    if field!(resource, T, version) != c::gui_shape_resource_version
        || field!(resource, T, size) != c::gui_shape_resource_size
    {
        return None;
    }
    let geometry = field!(resource, T, geometry_kind);
    let flags = field!(resource, T, flags);
    if flags & !c::gui_shape_flag_shadow_inset != 0 {
        return None;
    }
    let count = field!(resource, T, segment_count);
    if count > c::gui_shape_max_segments {
        return None;
    }
    let segment_bytes = u64::from(count).checked_mul(u64::from(c::gui_path_segment_size))?;
    let expected = u64::from(c::gui_shape_resource_size).checked_add(segment_bytes)?;
    if expected != resource.len() as u64 {
        return None;
    }
    let fill = field!(resource, T, fill_rule);
    if fill != c::gui_shape_fill_rule_nonzero && fill != c::gui_shape_fill_rule_evenodd {
        return None;
    }
    let join = field!(resource, T, line_join);
    let cap = field!(resource, T, line_cap);
    if join > c::gui_shape_line_join_bevel || cap > c::gui_shape_line_cap_square {
        return None;
    }
    let clip_w = field!(resource, T, clip_w);
    let clip_h = field!(resource, T, clip_h);
    if (clip_w == 0) != (clip_h == 0)
        || clip_w > c::gui_shape_max_dimension
        || clip_h > c::gui_shape_max_dimension
    {
        return None;
    }
    if field!(resource, T, reserved0) != 0
        || field!(resource, T, reserved1) != 0
        || field!(resource, T, reserved2) != 0
    {
        return None;
    }
    let max = c::gui_shape_max_coordinate as f32;
    let stroke = field!(resource, T, stroke_width_bits);
    let miter = field!(resource, T, miter_limit_bits);
    if !non_negative(stroke, max) || !non_negative(miter, max) {
        return None;
    }
    if join == c::gui_shape_line_join_miter && float(miter)? < 1.0 {
        return None;
    }
    if !coordinate(field!(resource, T, shadow_offset_x_bits))
        || !coordinate(field!(resource, T, shadow_offset_y_bits))
        || !coordinate(field!(resource, T, shadow_spread_bits))
        || !non_negative(
            field!(resource, T, shadow_blur_bits),
            c::gui_shape_max_blur_radius as f32,
        )
    {
        return None;
    }
    match geometry {
        c::gui_shape_geometry_kind_path => {
            if count == 0 {
                return None;
            }
            let mut offset = mem::offset_of!(T, geometry_x_bits);
            while offset <= mem::offset_of!(T, border_left_bits) {
                if read_u32(resource, offset)? != 0 {
                    return None;
                }
                offset += 4;
            }
            path(resource, count)?;
        }
        c::gui_shape_geometry_kind_rounded_rect => {
            if count != 0 || join != 0 || cap != 0 || stroke != 0 || miter != 0 {
                return None;
            }
            if !coordinate(field!(resource, T, geometry_x_bits))
                || !coordinate(field!(resource, T, geometry_y_bits))
            {
                return None;
            }
            let w = float(field!(resource, T, geometry_w_bits))?;
            let h = float(field!(resource, T, geometry_h_bits))?;
            if w <= 0.0 || h <= 0.0 || w > max || h > max {
                return None;
            }
            let mut offset = mem::offset_of!(T, radius_top_left_x_bits);
            while offset <= mem::offset_of!(T, border_left_bits) {
                if !non_negative(read_u32(resource, offset)?, max) {
                    return None;
                }
                offset += 4;
            }
        }
        _ => return None,
    }
    match command.kind {
        c::gui_frame_command_kind_path_fill => {
            (geometry == c::gui_shape_geometry_kind_path).then_some(())
        }
        c::gui_frame_command_kind_path_stroke => (geometry == c::gui_shape_geometry_kind_path
            && float(stroke).unwrap_or(0.0) > 0.0
            && join != 0
            && cap != 0)
            .then_some(()),
        c::gui_frame_command_kind_rounded_rect => {
            (geometry == c::gui_shape_geometry_kind_rounded_rect).then_some(())
        }
        c::gui_frame_command_kind_shadow => Some(()),
        _ => None,
    }
}
pub(super) fn batch(command: &Command, resources: &[u8]) -> bool {
    if command.version != c::gui_frame_command_version
        || command.size != c::gui_frame_command_size
        || command.flags != 0
    {
        return false;
    }
    let Some(kind) = kind(command) else {
        return false;
    };
    let Some(resource) = resource(command, resources) else {
        return false;
    };
    match command.kind {
        c::gui_frame_command_kind_clear | c::gui_frame_command_kind_rect => {
            kind == 0
                && command.resource_offset == 0
                && command.resource_bytes == 0
                && command.parameter0 == 0
                && command.parameter1 == 0
        }
        c::gui_frame_command_kind_text => {
            if command.parameter0 != 0 || command.parameter1 != 0 {
                return false;
            }
            if resource.is_empty() {
                return command.resource_offset == 0;
            }
            core::str::from_utf8(resource).is_ok()
        }
        c::gui_frame_command_kind_raster => {
            if command.w == 0
                || command.h == 0
                || command.w > 128
                || command.h > 128
                || command.parameter0 < 1
                || command.parameter0 > 16
                || command.parameter1 != 0
            {
                return false;
            }
            let Some(pixels) = u64::from(command.w).checked_mul(u64::from(command.h)) else {
                return false;
            };
            if pixels > 16384 {
                return false;
            }
            let Some(bytes) = pixels.checked_mul(4) else {
                return false;
            };
            if command.resource_bytes != bytes {
                return false;
            }
            let mut offset = 3;
            while offset < resource.len() {
                if resource[offset] != 0 {
                    return false;
                }
                offset += 4;
            }
            true
        }
        c::gui_frame_command_kind_alpha8 => {
            if command.w == 0
                || command.h == 0
                || command.w > c::gui_alpha8_max_width
                || command.h > c::gui_alpha8_max_height
                || command.parameter0 != 0
                || command.parameter1 != 0
            {
                return false;
            }
            let Some(bytes) = u64::from(command.w).checked_mul(u64::from(command.h)) else {
                return false;
            };
            if bytes > u64::from(c::gui_alpha8_max_pixels) {
                return false;
            }
            command.resource_bytes == bytes
        }
        c::gui_frame_command_kind_argb32 => {
            if kind != 4
                || command.w == 0
                || command.h == 0
                || command.w > c::gui_argb32_max_width
                || command.h > c::gui_argb32_max_height
                || command.parameter0 < 1
                || command.parameter0 > 16
                || command.parameter1 != 0
            {
                return false;
            }
            if command.rgb != 0
                || command.fg != 0
                || command.bg != 0
                || command.font_id != 0
                || command.text_w != 0
                || command.text_h != 0
                || command.baseline != 0
                || command.line_height != 0
            {
                return false;
            }
            let Some(pixels) = u64::from(command.w).checked_mul(u64::from(command.h)) else {
                return false;
            };
            if pixels > u64::from(c::gui_argb32_max_pixels) {
                return false;
            }
            let Some(bytes) = pixels.checked_mul(4) else {
                return false;
            };
            command.resource_bytes == bytes
        }
        c::gui_frame_command_kind_indexed8 => kind == 5 && indexed(command, resource).is_some(),
        c::gui_frame_command_kind_xrgb32_nearest => kind == 6 && xrgb(command, resource).is_some(),
        c::gui_frame_command_kind_shared_raster => kind == 7 && shared(command, resource).is_some(),
        c::gui_frame_command_kind_path_fill
        | c::gui_frame_command_kind_path_stroke
        | c::gui_frame_command_kind_rounded_rect
        | c::gui_frame_command_kind_shadow => kind == 4 && shape(command, resource).is_some(),
        _ => false,
    }
}
