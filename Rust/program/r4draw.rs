//! R4DRAW presentation/font facade and its display revision. Rendering,
//! catalog data, output generations and real completions retain their owners.
#![allow(clippy::too_many_arguments)]
use crate::{
    contract as a,
    display::{
        core_types::PresentOutcome, manager as display, native_driver, output_runtime,
        output_target, presenter, surface_pipeline,
    },
    font,
    input::mouse,
};
use core::{
    ffi::c_void,
    mem::size_of,
    ptr,
    sync::atomic::{AtomicU32, AtomicUsize, Ordering},
};
// Set once during the original linear API initialization. Raw legacy code
// addresses are invoked only by the exact Zig-signature thunk, never cast C.
static DISPLAY_USED: AtomicUsize = AtomicUsize::new(0);
static FONT_CHANGED: AtomicUsize = AtomicUsize::new(0);
static REVISION: AtomicU32 = AtomicU32::new(0);
unsafe extern "C" {
    fn r4os_zig_r4draw_hook(code: usize);
}
pub fn set_display_used_hook(code: usize) {
    DISPLAY_USED.store(code, Ordering::Relaxed)
}
pub fn set_font_changed_hook(code: usize) {
    FONT_CHANGED.store(code, Ordering::Relaxed)
}
pub fn mark_display_used() {
    let mut previous = REVISION.load(Ordering::Acquire);
    loop {
        let next = if previous == u32::MAX {
            1
        } else {
            previous + 1
        };
        match REVISION.compare_exchange_weak(previous, next, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => break,
            Err(value) => previous = value,
        }
    }
    let hook = DISPLAY_USED.load(Ordering::Relaxed);
    if hook != 0 {
        unsafe { r4os_zig_r4draw_hook(hook) }
    }
}
pub fn screen_width() -> u32 {
    surface_pipeline::width()
}
pub fn screen_height() -> u32 {
    surface_pipeline::height()
}
pub fn clear(rgb: u32) {
    if display::fill(rgb) {
        mark_display_used()
    }
}
pub fn rect(x: i32, y: i32, w: u32, h: u32, rgb: u32) {
    if display::rect(x, y, w, h, rgb) {
        mark_display_used()
    }
}
/// # Safety
/// Exact NUL-terminated caller text remains live through synchronous drawing.
pub unsafe fn text(font: Option<u32>, x: i32, y: i32, value: *const u8, fg: u32, bg: u32) {
    if unsafe { display::text_z(font, x, y, value, fg, bg) } {
        mark_display_used()
    }
}
pub fn display_revision() -> u32 {
    REVISION.load(Ordering::Acquire)
}
pub fn begin_frame() -> i32 {
    mark_display_used();
    mouse::r4os_rust_input_mouse_disable_cursor();
    surface_pipeline::begin_frame()
}
pub fn begin_frame_rect(_x: i32, _y: i32, w: u32, h: u32) -> i32 {
    mark_display_used();
    mouse::r4os_rust_input_mouse_disable_cursor();
    surface_pipeline::begin_frame_rect(w, h)
}
pub fn present() -> i32 {
    mark_display_used();
    surface_pipeline::present()
}
/// # Safety
/// Pixel buffer retains the declared count through its single synchronous blit.
pub unsafe fn blit(
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    pixels: *const u32,
    pixel_count: u32,
    stride: u32,
) -> i32 {
    if x < 0 || y < 0 || w == 0 || h == 0 || pixels.is_null() || stride < w {
        return -1;
    }
    let needed = (u64::from(h) - 1) * u64::from(stride) + u64::from(w);
    if needed > u64::from(pixel_count) || needed * 4 > usize::MAX as u64 {
        return -2;
    }
    let region = presenter::Region {
        destination_x: x as u32,
        destination_y: y as u32,
        w,
        h,
        ..Default::default()
    };
    let result = unsafe {
        presenter::present_xrgb32(&presenter::Request {
            source: presenter::View {
                pixels,
                pixel_count: needed as usize,
                width: w as usize,
                height: h as usize,
                pitch_pixels: stride as usize,
                format: 0,
                revision: 0,
            },
            regions: &region,
            regions_len: 1,
            reason: 5,
            source_generation: 0,
            input_tick: 0,
            input_tick_valid: false,
        })
    };
    if !result.success {
        return -3;
    }
    mark_display_used();
    0
}
/// # Safety
/// Full request, source count, region count and result remain resident through
/// the same public synchronous presentation operation.
pub unsafe fn present_regions(
    request: *const a::DisplayPresentRequest,
    pixels: *const u32,
    pixel_count: u32,
    regions: *const a::DisplayDamageRect,
    region_count: u32,
    out: *mut a::DisplayPresentResult,
) -> i32 {
    if out.is_null() {
        return a::display_present_error_invalid;
    }
    unsafe { out.write(a::DisplayPresentResult::default()) };
    if request.is_null() || pixels.is_null() || regions.is_null() {
        return a::display_present_error_invalid;
    }
    unsafe {
        let request = request.read();
        if request.magic != a::display_present_magic
            || request.version != a::display_present_version
            || (request.size as usize) < size_of::<a::DisplayPresentRequest>()
            || request.format != a::display_present_format_xrgb32
            || request.source_width == 0
            || request.source_height == 0
            || request.source_stride_pixels < request.source_width
            || region_count == 0
            || region_count > a::display_damage_max_regions
        {
            return a::display_present_error_invalid;
        }
        let needed = (u64::from(request.source_height) - 1)
            * u64::from(request.source_stride_pixels)
            + u64::from(request.source_width);
        if needed > u64::from(pixel_count) {
            return a::display_present_error_out_of_range;
        }
        let mut normalized = [presenter::Region::default(); a::display_damage_max_regions as usize];
        for (index, slot) in normalized
            .iter_mut()
            .enumerate()
            .take(region_count as usize)
        {
            let region = regions.add(index).read();
            if region.x < 0 || region.y < 0 || region.w == 0 || region.h == 0 {
                return a::display_present_error_out_of_range;
            }
            let x = region.x as u32;
            let y = region.y as u32;
            if x >= request.source_width
                || y >= request.source_height
                || region.w > request.source_width - x
                || region.h > request.source_height - y
            {
                return a::display_present_error_out_of_range;
            }
            *slot = presenter::Region {
                source_x: x,
                source_y: y,
                destination_x: x,
                destination_y: y,
                w: region.w,
                h: region.h,
            };
        }
        let result = presenter::present_xrgb32(&presenter::Request {
            source: presenter::View {
                pixels,
                pixel_count: pixel_count as usize,
                width: request.source_width as usize,
                height: request.source_height as usize,
                pitch_pixels: request.source_stride_pixels as usize,
                format: 0,
                revision: request.source_generation,
            },
            regions: normalized.as_ptr(),
            regions_len: region_count as usize,
            reason: 2,
            source_generation: request.source_generation,
            input_tick: request.input_tick,
            input_tick_valid: request.flags & a::display_present_request_flag_input_tick_valid != 0,
        });
        out.write(public_result(result));
        if !result.success {
            return a::display_present_error_unavailable;
        }
        let _ = surface_pipeline::present();
        mark_display_used();
        0
    }
}
fn public_result(result: PresentOutcome) -> a::DisplayPresentResult {
    a::DisplayPresentResult {
        flags: if result.success {
            a::display_present_result_success
        } else {
            0
        } | if result.success && result.fence == result.completed_fence {
            a::display_present_result_completed
        } else {
            0
        } | if result.accelerated {
            a::display_present_result_accelerated
        } else {
            0
        } | if result.fallback {
            a::display_present_result_fallback
        } else {
            0
        },
        source_generation: result.source_generation,
        present_generation: result.present_generation,
        fence: result.fence,
        completed_fence: result.completed_fence,
        region_count: result.region_count,
        pixel_count: result.pixel_count,
        fallback_regions: result.fallback_regions,
        backend_error: result.backend_error,
        present_tick: result.present_tick,
        elapsed_ticks: result.elapsed_ticks,
        backend_name: result.backend_name,
        ..Default::default()
    }
}
/// # Safety
/// Complete capability output remains writable for its copied result.
pub unsafe fn present_capabilities(out: *mut a::DisplayPresentCapabilities) -> i32 {
    if out.is_null() {
        return a::display_present_error_invalid;
    }
    let c = display::present_capabilities();
    unsafe {
        out.write(a::DisplayPresentCapabilities {
            flags: c.flags,
            formats: c.formats,
            max_regions: c.max_regions,
            backend_kind: c.backend_kind,
            backend_name: c.backend_name,
            fallback_name: c.fallback_name,
            ..Default::default()
        })
    };
    0
}
/// # Safety
/// Complete completion output remains writable through this passive query.
pub unsafe fn present_completion(fence: u64, out: *mut a::DisplayPresentCompletion) -> i32 {
    if out.is_null() || fence == 0 {
        return a::display_present_error_invalid;
    }
    let completed = display::highest_completed_fence();
    let complete = fence <= completed;
    let result = if complete {
        0
    } else {
        a::display_present_error_unavailable
    };
    unsafe {
        out.write(a::DisplayPresentCompletion {
            flags: if complete {
                a::display_present_completion_complete | a::display_present_completion_cpu_stores
            } else {
                0
            },
            fence,
            completed_fence: completed,
            result,
            ..Default::default()
        })
    };
    result
}
fn presentation_code(error: crate::display::error::Error) -> i32 {
    use crate::display::error::Error;
    match error {
        Error::Invalid => a::gfx_output_error_invalid,
        Error::Stale => a::gfx_output_error_stale,
        Error::Unsupported => a::gfx_output_error_unsupported,
        Error::Capacity => a::gfx_output_error_capacity,
        _ => panic!("presentation facts error"),
    }
}
/// # Safety
/// Exact aligned current-size target output is initialized and retained.
pub unsafe fn output_target(adapter: u32, head: u32, out: *mut a::GfxOutputTarget) -> i32 {
    if out.is_null()
        || !out.is_aligned()
        || unsafe { (*out).version != 1 || (*out).size as usize != size_of::<a::GfxOutputTarget>() }
    {
        return a::gfx_output_error_invalid;
    }
    match native_driver::output_target(adapter, head) {
        Ok(target) => {
            unsafe { out.write(target) };
            a::gfx_output_ok
        }
        Err(e) => native_driver::code(e),
    }
}
/// # Safety
/// Complete aligned target and initialized full facts output remain resident.
pub unsafe fn output_presentation_info(
    input: *const a::GfxOutputTarget,
    out: *mut a::DisplayPresentationInfo,
) -> i32 {
    let result = (|| {
        unsafe {
            if input.is_null() || !input.is_aligned() || !crate::gfx::buffer_api::valid_output(out)
            {
                return Err(a::gfx_output_error_invalid);
            }
            let target = input.read();
            let value = if output_runtime::contains(target) {
                output_runtime::info(target).map_err(super::gfx_output_api::code)?
            } else {
                let current = native_driver::output_target(target.adapter_id, target.head_id)
                    .map_err(native_driver::code)?;
                if !output_target::same(&current, &target) {
                    return Err(a::gfx_output_error_stale);
                }
                let value =
                    display::presentation_info(target.head_id).map_err(presentation_code)?;
                let after = native_driver::output_target(target.adapter_id, target.head_id)
                    .map_err(native_driver::code)?;
                if !output_target::same(&after, &target)
                    || value.display_generation != target.display_generation
                {
                    return Err(a::gfx_output_error_stale);
                }
                value
            };
            out.write(value);
        }
        Ok(())
    })();
    result.map_or_else(|e| e, |()| a::gfx_output_ok)
}
/// # Safety
/// Complete aligned target/fence and initialized full facts output stay live.
pub unsafe fn output_presentation_feedback(
    input: *const a::GfxOutputTarget,
    source: *const a::GfxFence,
    out: *mut a::DisplayPresentationStats,
) -> i32 {
    let result = (|| {
        unsafe {
            if input.is_null()
                || !input.is_aligned()
                || source.is_null()
                || !crate::gfx::buffer_api::valid_output(out)
            {
                return Err(a::gfx_output_error_invalid);
            }
            let target = input.read();
            let fence = source.read();
            let value = if output_runtime::contains(target) {
                output_runtime::feedback(target, fence).map_err(super::gfx_output_api::code)?
            } else {
                let current = native_driver::output_target(target.adapter_id, target.head_id)
                    .map_err(native_driver::code)?;
                if !output_target::same(&current, &target) {
                    return Err(a::gfx_output_error_stale);
                }
                let sample = display::presentation_feedback(target.head_id, fence)
                    .map_err(presentation_code)?;
                if sample.display_generation != target.display_generation {
                    return Err(a::gfx_output_error_stale);
                }
                sample
            };
            out.write(value);
        }
        Ok(())
    })();
    result.map_or_else(|e| e, |()| a::gfx_output_ok)
}
/// # Safety
/// Complete initialized stats/info output and optional full fence are resident.
pub unsafe fn presentation_facts(
    head: u32,
    kind: u32,
    source: *const a::GfxFence,
    out: *mut c_void,
) -> i32 {
    unsafe {
        match kind {
            0 => {
                let out = out.cast::<a::DisplayPresentationStats>();
                if !crate::gfx::buffer_api::valid_output(out) {
                    return a::gfx_output_error_invalid;
                }
                match display::presentation_stats(head) {
                    Ok(s) => {
                        out.write(s);
                        a::gfx_output_ok
                    }
                    Err(e) => presentation_code(e),
                }
            }
            1 => {
                let out = out.cast::<a::DisplayPresentationInfo>();
                if !crate::gfx::buffer_api::valid_output(out) {
                    return a::gfx_output_error_invalid;
                }
                match display::presentation_info(head) {
                    Ok(s) => {
                        out.write(s);
                        a::gfx_output_ok
                    }
                    Err(e) => presentation_code(e),
                }
            }
            2 => {
                let out = out.cast::<a::DisplayPresentationStats>();
                if source.is_null() || !crate::gfx::buffer_api::valid_output(out) {
                    return a::gfx_output_error_invalid;
                }
                match display::presentation_feedback(head, source.read()) {
                    Ok(s) => {
                        out.write(s);
                        a::gfx_output_ok
                    }
                    Err(e) => presentation_code(e),
                }
            }
            _ => panic!("presentation facts selector"),
        }
    }
}
pub fn font_count() -> u32 {
    font::owner::with_catalog(|state| u32::try_from(state.font_count()).expect("font count"))
}
fn copy_fixed(out: &mut [u8], value: &[u8]) {
    out.fill(0);
    if out.is_empty() {
        return;
    }
    let count = value.len().min(out.len() - 1);
    if count > 0 {
        out[..count].copy_from_slice(&value[..count])
    }
}
/// # Safety
/// Output is a complete writable public record; native catalog lease owns data.
pub unsafe fn font_info(font_id: u32, force_selected: bool, out: *mut a::GuiFontInfo) -> i32 {
    if out.is_null() {
        return -1;
    }
    let view = font::owner::acquire();
    let state = unsafe { &*view.state() };
    unsafe { out.write(a::GuiFontInfo::default()) };
    if font_id == a::gui_font_builtin_id {
        let mut info = a::GuiFontInfo {
            id: a::gui_font_builtin_id,
            kind: 0,
            flags: a::gui_font_flag_renderable
                | a::gui_font_flag_builtin
                | if force_selected || state.current_font_id() == a::gui_font_builtin_id {
                    a::gui_font_flag_selected
                } else {
                    0
                },
            weight: 400,
            style_flags: 0,
            charset_flags: 0,
            width: 8,
            height: 8,
            max_advance: 8,
            line_height: 8,
            baseline: 7,
            glyph_count: 95,
            strike_count: 1,
            ..Default::default()
        };
        copy_fixed(&mut info.family, b"R4OS");
        copy_fixed(&mut info.face, b"Builtin 8x8");
        copy_fixed(&mut info.style, b"Regular");
        copy_fixed(&mut info.status, b"builtin fallback");
        unsafe { out.write(info) };
        return 1;
    }
    let Some(entry) = state.catalog_entry_for_id(font_id) else {
        return 0;
    };
    let mut info = a::GuiFontInfo {
        id: font_id,
        kind: u32::from(entry.kind as u16),
        flags: if entry.renderable {
            a::gui_font_flag_renderable
        } else {
            0
        } | if force_selected || entry.selected {
            a::gui_font_flag_selected
        } else {
            0
        },
        weight: u32::from(entry.weight),
        style_flags: entry.style_flags,
        charset_flags: entry.charset_flags,
        width: u32::from(entry.width),
        height: u32::from(entry.height),
        max_advance: u32::from(entry.max_advance),
        line_height: u32::from(entry.line_height),
        baseline: i32::from(entry.baseline),
        glyph_count: entry.glyph_count,
        strike_count: u32::from(entry.strike_count),
        ..Default::default()
    };
    copy_fixed(&mut info.path, &entry.path[..entry.path_len]);
    copy_fixed(&mut info.family, &entry.family[..entry.family_len]);
    copy_fixed(&mut info.face, &entry.face[..entry.face_len]);
    copy_fixed(&mut info.style, &entry.style[..entry.style_len]);
    copy_fixed(&mut info.status, &entry.status[..entry.status_len]);
    unsafe { out.write(info) };
    1
}
/// # Safety
/// Text is readable through the first NUL or4096 bytes; output is complete.
pub unsafe fn font_measure(font_id: u32, value: *const u8, out: *mut a::GuiTextMetrics) -> i32 {
    if value.is_null() || out.is_null() {
        return -1;
    }
    let view = font::owner::acquire();
    let state = unsafe { &*view.state() };
    if !state.is_renderable(font_id) {
        return -2;
    }
    let mut len = 0;
    while len < 4096 && unsafe { value.add(len).read() } != 0 {
        len += 1
    }
    let metrics = unsafe { state.measure_raw(font_id, value, len) };
    unsafe {
        out.write(a::GuiTextMetrics {
            width: metrics.width,
            height: metrics.height,
            line_height: metrics.line_height,
            baseline: i32::from(metrics.baseline),
            visible_bytes: metrics.visible_bytes.min(u32::MAX as usize) as u32,
            flags: u32::from(metrics.clipped),
        })
    };
    0
}
pub fn font_glyph_row(font_id: u32, codepoint: u32, row: u32) -> u64 {
    if codepoint > 0x10ffff || row as usize >= font::types::MAX_GLYPH_H {
        return 0;
    }
    font::owner::with_catalog(|state| {
        if state.is_renderable(font_id) {
            state.glyph_row_mask_for_font(font_id, codepoint, row)
        } else {
            0
        }
    })
}
/// # Safety
/// Output is a complete writable glyph bitmap owned by the caller.
pub unsafe fn font_glyph_bitmap(font_id: u32, codepoint: u32, out: *mut a::GuiGlyphBitmap) -> i32 {
    if out.is_null() || codepoint > 0x10ffff {
        return -1;
    }
    let view = font::owner::acquire();
    let state = unsafe { &*view.state() };
    if !state.is_renderable(font_id) {
        return -2;
    }
    let b = state.glyph_bitmap_for_font(font_id, codepoint);
    unsafe {
        out.write(a::GuiGlyphBitmap {
            width: b.width,
            height: b.height,
            advance: b.advance,
            line_height: b.line_height,
            baseline: i32::from(b.baseline),
            reserved0: 0,
            rows: b.rows,
        })
    };
    0
}
pub fn font_reload() -> i32 {
    let result = font::catalog_io::reload_installed();
    if result.unavailable {
        return -1;
    }
    let hook = FONT_CHANGED.load(Ordering::Relaxed);
    if hook != 0 {
        unsafe { r4os_zig_r4draw_hook(hook) }
    }
    i32::try_from(result.registered).expect("font registered count")
}
pub fn font_revision() -> u32 {
    font::owner::revision()
}
#[repr(C)]
pub struct Args {
    pub a: u64,
    pub b: u64,
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub color: u32,
    pub bg: u32,
    pub input: *const c_void,
    pub second: *const c_void,
    pub third: *const c_void,
}
/// # Safety
/// Exact original R4DRAW slot inputs/outputs survive synchronous execution.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4draw(op: u32, args: *const Args, out: *mut c_void) -> u64 {
    let args = unsafe { &*args };
    let signed = |v: i32| v as i64 as u64;
    unsafe {
        match op {
            0 => {
                set_display_used_hook(args.a as usize);
                0
            }
            1 => {
                set_font_changed_hook(args.a as usize);
                0
            }
            2 => {
                mark_display_used();
                0
            }
            3 => u64::from(screen_width()),
            4 => u64::from(screen_height()),
            5 => {
                clear(args.color);
                0
            }
            6 => {
                rect(args.x, args.y, args.w, args.h, args.color);
                0
            }
            7 => {
                text(None, args.x, args.y, args.input.cast(), args.color, args.bg);
                0
            }
            8 => u64::from(display_revision()),
            9 => signed(begin_frame()),
            10 => signed(begin_frame_rect(args.x, args.y, args.w, args.h)),
            11 => signed(present()),
            12 => signed(blit(
                args.x,
                args.y,
                args.w,
                args.h,
                args.input.cast(),
                args.b as u32,
                args.a as u32,
            )),
            13 => signed(present_regions(
                args.input.cast(),
                args.second.cast(),
                args.b as u32,
                args.third.cast(),
                args.a as u32,
                out.cast(),
            )),
            14 => signed(present_capabilities(out.cast())),
            15 => signed(present_completion(args.a, out.cast())),
            16 => signed(output_target(args.w, args.h, out.cast())),
            17 => signed(output_presentation_info(args.input.cast(), out.cast())),
            18 => signed(output_presentation_feedback(
                args.input.cast(),
                args.second.cast(),
                out.cast(),
            )),
            19 => signed(presentation_facts(args.w, 0, ptr::null(), out)),
            20 => signed(presentation_facts(args.w, 1, ptr::null(), out)),
            21 => signed(presentation_facts(args.w, 2, args.input.cast(), out)),
            22 => u64::from(font_count()),
            23 => signed(font_info(args.w, false, out.cast())),
            24 => signed(font_measure(args.w, args.input.cast(), out.cast())),
            25 => font_glyph_row(args.w, args.h, args.a as u32),
            26 => signed(font_glyph_bitmap(args.w, args.h, out.cast())),
            27 => signed(font_reload()),
            28 => u64::from(font_revision()),
            29 => {
                text(
                    Some(args.w),
                    args.x,
                    args.y,
                    args.input.cast(),
                    args.color,
                    args.bg,
                );
                0
            }
            _ => panic!("R4DRAW slot"),
        }
    }
}
const _: () = {
    assert!(size_of::<Args>() == 64);
    assert!(core::mem::offset_of!(Args, input) == 40);
};
