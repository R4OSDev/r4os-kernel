//! Bounded prefix observation under one canonical immutable font-bank lease.
use crate::{contract as a, font};

/// # Safety
/// Immutable complete mapped source, whole aligned exclusive disjoint outputs
/// and provider code remain live through real return. No caller pointer escapes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_draw_font_prefix_metrics2(
    font_id: u32,
    text: *const u8,
    text_bytes: u32,
    rows: *mut a::FontAdvance2,
    capacity: u32,
    out_info: *mut a::FontAdvanceInfo2,
) -> i32 {
    if text_bytes > a::font_metrics2_max_bytes
        || capacity > a::font_metrics2_max_bytes
        || text_bytes != 0 && text.is_null()
        || capacity != 0 && (rows.is_null() || !rows.is_aligned())
        || out_info.is_null()
        || !out_info.is_aligned()
    {
        return a::font_metrics2_invalid;
    }
    let view = font::owner::acquire();
    if view.bank == 255 {
        return a::font_metrics2_unavailable;
    }
    let Some(revision) = font::owner::metrics_revision(&view) else {
        return a::font_metrics2_exhausted;
    };
    if revision == 0 {
        return a::font_metrics2_unavailable;
    }
    let state = unsafe { &*view.state() };
    if !state.is_renderable(font_id) {
        return a::font_metrics2_font;
    }
    let length = text_bytes as usize;
    let mut cursor = 0;
    let mut required = 0u32;
    while cursor < length {
        let first = unsafe { text.add(cursor).read() };
        let decoded = unsafe { font::decode::decode_raw(text, length, cursor) };
        if first == 0 || first >= 128 && decoded.consumed == 1 || decoded.consumed == 0 {
            return a::font_metrics2_encoding;
        }
        cursor += decoded.consumed;
        required += 1;
    }
    let mut receipt = a::FontAdvanceInfo2 {
        font_revision: revision,
        font_id,
        text_bytes,
        required_count: required,
        ..Default::default()
    };
    if capacity < required {
        unsafe { out_info.write(receipt) };
        return a::font_metrics2_capacity;
    }
    cursor = 0;
    let mut count = 0u32;
    let mut line_width = 0u32;
    let mut maximum = 0u32;
    while cursor < length {
        let first = unsafe { text.add(cursor).read() };
        let decoded = unsafe { font::decode::decode_raw(text, length, cursor) };
        if first == 0 || first >= 128 && decoded.consumed == 1 || decoded.consumed == 0 {
            return a::font_metrics2_encoding;
        }
        cursor += decoded.consumed;
        match decoded.codepoint {
            13 => {}
            10 => {
                maximum = maximum.max(line_width);
                line_width = 0;
            }
            scalar => {
                let Some(next) =
                    line_width.checked_add(state.glyph_advance_for_font(font_id, scalar))
                else {
                    return a::font_metrics2_overflow;
                };
                line_width = next;
            }
        }
        if count >= capacity {
            return a::font_metrics2_invalid;
        }
        unsafe {
            rows.add(count as usize).write(a::FontAdvance2 {
                byte_end: cursor as u32,
                prefix_width: maximum.max(line_width),
            })
        };
        count += 1;
    }
    if count != required {
        return a::font_metrics2_invalid;
    }
    receipt.count = count;
    receipt.flags = a::font_metrics2_flag_complete;
    unsafe { out_info.write(receipt) };
    a::font_metrics2_ok
}
