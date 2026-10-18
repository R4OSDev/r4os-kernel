//! Exact presentation operations of the parent owner; no duplicated state.
use super::*;

pub(super) fn begin_cpu_write(device: *const Device) -> Option<CpuWrite> {
    if unsafe { (*device).value.kind } != NATIVE {
        return Some(CpuWrite {
            callback: None,
            context: 0,
            active: true,
            damage: None,
        });
    }
    let native = unsafe { (*context()).native_backend }?;
    if let Some(begin) = native.begin_cpu {
        if native.end_cpu.is_none() || !unsafe { begin.call(native.context) } {
            return None;
        }
        return Some(CpuWrite {
            callback: native.end_cpu,
            context: native.context,
            active: true,
            damage: None,
        });
    }
    if native.end_cpu.is_none() {
        Some(CpuWrite {
            callback: None,
            context: 0,
            active: true,
            damage: None,
        })
    } else {
        None
    }
}

pub fn fill(rgb: u32) -> bool {
    let Ok(_execution) = execution() else {
        return false;
    };
    let device = unsafe { (*context()).primary };
    if device.is_null() {
        return false;
    }
    let Some(op) = (unsafe { (*device).ops.fill }) else {
        return false;
    };
    let Some(mut write) = begin_cpu_write(device) else {
        return false;
    };
    let start = timer::tick_count();
    let ok = op(device, rgb);
    if ok {
        write.damage = Some(unsafe { (*device).value.last_present_rect });
    }
    if !write.finish(ok) {
        return false;
    }
    if ok {
        record_timing(device, start);
        publish_stats();
    }
    ok
}

pub fn rect(x: i32, y: i32, w: u32, h: u32, rgb: u32) -> bool {
    let Ok(_execution) = execution() else {
        return false;
    };
    let device = unsafe { (*context()).primary };
    if device.is_null() {
        return false;
    }
    let Some(op) = (unsafe { (*device).ops.rect }) else {
        return false;
    };
    let Some(mut write) = begin_cpu_write(device) else {
        return false;
    };
    let start = timer::tick_count();
    let ok = op(device, x, y, w, h, rgb);
    if ok {
        write.damage = Some(unsafe { (*device).value.last_present_rect });
    }
    if !write.finish(ok) {
        return false;
    }
    if ok {
        record_timing(device, start);
        publish_stats();
    }
    ok
}

/// # Safety
/// Input retains at least a readable terminator or4096bytes on the exact
/// calling task. Display execution retains the frame through CPU completion.
pub unsafe fn text_z(
    font_id: Option<u32>,
    x: i32,
    y: i32,
    value: *const u8,
    fg: u32,
    bg: u32,
) -> bool {
    if value.is_null() {
        return false;
    }
    let Ok(_execution) = execution() else {
        return false;
    };
    let device = unsafe { (*context()).primary };
    if device.is_null() {
        return false;
    }
    let frame = unsafe { (*device).frame };
    if frame.is_null() {
        return false;
    }
    let frame = unsafe { frame.read() };
    if !fb::supports_rgb32(&frame) {
        return false;
    }
    let Some(mut write) = begin_cpu_write(device) else {
        return false;
    };
    let mut length = 0;
    while length < 4096 && unsafe { value.add(length).read() } != 0 {
        length += 1;
    }
    let catalog = font::owner::acquire();
    let state = unsafe { &*catalog.state() };
    let selected = state.normalize_id(font_id.unwrap_or_else(|| state.current_font_id()));
    let line_height = state.glyph_height_for_font(selected);
    let packed_fg = fb::pack_rgb(&frame, fg);
    let packed_bg = fb::pack_rgb(&frame, bg);
    let mut pen_x = i64::from(x);
    let mut pen_y = i64::from(y);
    let mut offset = 0;
    let mut pixels = 0;
    let mut bounds = Rect::default();
    let start = timer::tick_count();
    while offset < length {
        let scalar = unsafe { font::decode::decode_raw(value, length, offset) };
        offset += scalar.consumed;
        if scalar.codepoint == u32::from(b'\r') {
            continue;
        }
        if scalar.codepoint == u32::from(b'\n') {
            pen_x = i64::from(x);
            pen_y += i64::from(line_height);
            continue;
        }
        let glyph = state.glyph_bitmap_for_font(selected, scalar.codepoint);
        if let Some(clipped) = clip_signed_rect(
            pen_x,
            pen_y,
            glyph.width.max(glyph.advance),
            glyph.line_height,
            unsafe { (*device).value.mode },
        ) {
            bounds = if pixels == 0 {
                clipped
            } else {
                merge_rect(bounds, clipped)
            };
            pixels += u64::from(clipped.w) * u64::from(clipped.h);
            for row in 0..clipped.h {
                let gy = usize::try_from(i64::from(clipped.y) + i64::from(row) - pen_y)
                    .expect("clipped glyph row");
                for column in 0..clipped.w {
                    let gx = usize::try_from(i64::from(clipped.x) + i64::from(column) - pen_x)
                        .expect("clipped glyph column");
                    let ink = gy < glyph.height as usize
                        && gy < glyph.rows.len()
                        && gx < glyph.width as usize
                        && gx < 64
                        && glyph.rows[gy] & (1u64 << gx) != 0;
                    unsafe {
                        fb::put_packed(
                            &frame,
                            u64::from(clipped.x + column),
                            u64::from(clipped.y + row),
                            if ink { packed_fg } else { packed_bg },
                        );
                    }
                }
            }
        }
        pen_x += i64::from(glyph.advance);
    }
    if pixels == 0 {
        return false;
    }
    write.damage = Some(bounds);
    if !write.finish(true) {
        return false;
    }
    record_present_aggregate(device, REASON_RECT, bounds, pixels, false);
    record_timing(device, start);
    publish_stats();
    true
}

pub fn put_packed32(x: u64, y: u64, color32: u32) -> bool {
    let Ok(_execution) = execution() else {
        return false;
    };
    let device = unsafe { (*context()).primary };
    if device.is_null() {
        return false;
    }
    let Some(op) = (unsafe { (*device).ops.put_packed32 }) else {
        return false;
    };
    let Some(mut write) = begin_cpu_write(device) else {
        return false;
    };
    let ok = op(device, x, y, color32);
    if ok {
        write.damage = Some(Rect {
            x: u32::try_from(x).expect("pixel x"),
            y: u32::try_from(y).expect("pixel y"),
            w: 1,
            h: 1,
        });
    }
    write.finish(ok) && ok
}

pub fn put_xrgb32(x: u64, y: u64, rgb: u32) -> bool {
    let Ok(_execution) = execution() else {
        return false;
    };
    let device = unsafe { (*context()).primary };
    if device.is_null() {
        return false;
    }
    let Some(op) = (unsafe { (*device).ops.put_xrgb32 }) else {
        return false;
    };
    let Some(mut write) = begin_cpu_write(device) else {
        return false;
    };
    let ok = op(device, x, y, rgb);
    if ok {
        write.damage = Some(Rect {
            x: u32::try_from(x).expect("pixel x"),
            y: u32::try_from(y).expect("pixel y"),
            w: 1,
            h: 1,
        });
    }
    write.finish(ok) && ok
}

/// # Safety
/// Original readable source span retained through the exact CPU write lease.
pub unsafe fn present_packed32_rect(
    x: u64,
    y: u64,
    w: u64,
    h: u64,
    src: &[u8],
    stride: u64,
) -> bool {
    let Ok(_execution) = execution() else {
        return false;
    };
    let device = unsafe { (*context()).primary };
    if device.is_null() {
        return false;
    }
    let Some(op) = (unsafe { (*device).ops.present_packed32_rect }) else {
        return false;
    };
    let Some(mut write) = begin_cpu_write(device) else {
        return false;
    };
    let start = timer::tick_count();
    let ok = unsafe { op(device, x, y, w, h, src, stride) };
    if ok {
        write.damage = Some(unsafe { (*device).value.last_present_rect });
    }
    if !write.finish(ok) {
        return false;
    }
    if ok {
        record_timing(device, start);
        publish_stats();
    }
    ok
}

/// # Safety
/// Exact original readable four-byte pixels and source byte extent through
/// presentation; alignment/geometry retain the original wrapper validation.
pub unsafe fn present_xrgb32_rect(x: u64, y: u64, w: u64, h: u64, src: &[u8], stride: u64) -> bool {
    if x > u64::from(u32::MAX)
        || y > u64::from(u32::MAX)
        || w > u64::from(u32::MAX)
        || h > u64::from(u32::MAX)
        || stride > u64::from(u32::MAX)
        || src.len() & 3 != 0
        || src.as_ptr().addr() & 3 != 0
    {
        return false;
    }
    let region = blit_backend::Region {
        dst_x: x as u32,
        dst_y: y as u32,
        src_x: 0,
        src_y: 0,
        w: w as u32,
        h: h as u32,
    };
    unsafe {
        present_xrgb32_regions(
            src.as_ptr().cast(),
            u32::try_from(src.len() / 4).expect("present pixel count"),
            stride as u32,
            &[region],
            0,
            0,
            false,
        )
    }
    .success
}

/// # Safety
/// Source and each admitted pixel region remain readable, with the exact
/// retained output/CPU lease until this synchronous generation completes.
pub unsafe fn present_xrgb32_regions(
    source: *const u32,
    source_pixels: u32,
    source_stride: u32,
    regions: &[blit_backend::Region],
    source_generation: u64,
    input_tick: u64,
    input_tick_valid: bool,
) -> PresentOutcome {
    let mut outcome = PresentOutcome {
        source_generation,
        ..PresentOutcome::default()
    };
    let Ok(_execution) = execution() else {
        return outcome;
    };
    let device = unsafe { (*context()).primary };
    if device.is_null() {
        return outcome;
    }
    let frame = unsafe { (*device).frame };
    if frame.is_null() {
        return outcome;
    }
    let frame = unsafe { frame.read() };
    if !fb::supports_rgb32(&frame)
        || source_pixels == 0
        || source_stride == 0
        || regions.is_empty()
        || regions.len() > 8
    {
        return outcome;
    }
    let mut pixels_total = 0u64;
    let mut bounds = Rect::default();
    for (index, &region) in regions.iter().enumerate() {
        if !valid_present_region(
            region,
            unsafe { (*device).value.mode },
            source_pixels,
            source_stride,
        ) {
            return outcome;
        }
        let pixels = u64::from(region.w) * u64::from(region.h);
        if pixels_total > u64::from(u32::MAX) - pixels {
            return outcome;
        }
        pixels_total += pixels;
        let rect = Rect {
            x: region.dst_x,
            y: region.dst_y,
            w: region.w,
            h: region.h,
        };
        bounds = if index == 0 {
            rect
        } else {
            merge_rect(bounds, rect)
        };
    }
    let Some(mut write) = begin_cpu_write(device) else {
        return outcome;
    };
    let start = timer::tick_count();
    let mut external = blit_backend::InvokeResult::default();
    if fb::is_native_xrgb32(&frame) && frame.pitch & 3 == 0 {
        let job = blit_backend::Job {
            version: 1,
            size: core::mem::size_of::<blit_backend::Job>() as u32,
            format: 1,
            flags: 0,
            target_address: frame.address.addr() as u64,
            target_width: u32::try_from(frame.width).expect("frame width"),
            target_height: u32::try_from(frame.height).expect("frame height"),
            target_pitch_pixels: u32::try_from(frame.pitch / 4).expect("frame pitch"),
            source_pixel_count: source_pixels,
            source_address: source.addr() as u64,
            source_stride_pixels: source_stride,
            region_count: u32::try_from(regions.len()).expect("region count"),
            regions_address: regions.as_ptr().addr() as u64,
        };
        external = unsafe { blit_backend::invoke(&job) };
    }
    if external.attempted && external.result == 0 {
        outcome.accelerated = true;
        outcome.backend_name = external.name;
    } else {
        let source_bytes =
            unsafe { slice::from_raw_parts(source.cast::<u8>(), source_pixels as usize * 4) };
        for &region in regions {
            if !unsafe { bootfb_copy_xrgb32_region(device, region, source_bytes, source_stride) } {
                return outcome;
            }
        }
        outcome.fallback = true;
        outcome.fallback_regions = regions.len() as u32;
        outcome.backend_error = if external.attempted {
            external.result
        } else {
            0
        };
        copy_name(
            &mut outcome.backend_name,
            if unsafe { (*device).value.kind } == BOOTFB {
                b"bootfb-cpu"
            } else {
                b"native-cpu"
            },
        );
    }
    write.damage = Some(bounds);
    if !write.finish(true) {
        return outcome;
    }
    let completed_tick = timer::tick_count();
    record_present_aggregate(device, REASON_XRGB32, bounds, pixels_total, false);
    record_timing_at(device, start, completed_tick);
    unsafe { core::arch::asm!("sfence", options(nostack, preserves_flags)) };
    let (generation, completed) = {
        let _state = ownership::enter_state();
        let p = context();
        unsafe {
            (*p).present_generation = (*p).present_generation.wrapping_add(1);
            if (*p).present_generation == 0 {
                (*p).present_generation = 1;
            }
            (*p).completed_fence = (*p).present_generation;
            (*p).completed_stats = capture_stats();
            ((*p).present_generation, (*p).completed_fence)
        }
    };
    outcome.success = true;
    outcome.present_generation = generation;
    outcome.fence = generation;
    outcome.completed_fence = completed;
    outcome.region_count = regions.len() as u32;
    outcome.pixel_count = u32::try_from(pixels_total).expect("present pixels");
    outcome.present_tick = completed_tick;
    outcome.elapsed_ticks = if input_tick_valid && completed_tick >= input_tick {
        completed_tick - input_tick
    } else {
        0
    };
    outcome
}

pub fn present_capabilities() -> PresentCapabilities {
    let current = stats();
    if !current.registered {
        return PresentCapabilities {
            flags: 0,
            formats: 0,
            max_regions: 0,
            backend_kind: 0,
            ..PresentCapabilities::default()
        };
    }
    let mut result = PresentCapabilities {
        flags: 1 | 2 | 4 | 32,
        backend_kind: if current.kind == BOOTFB { 1 } else { 3 },
        ..PresentCapabilities::default()
    };
    let cpu_name = if current.kind == BOOTFB {
        &b"bootfb-cpu"[..]
    } else {
        &b"native-cpu"[..]
    };
    copy_name(&mut result.backend_name, cpu_name);
    copy_name(&mut result.fallback_name, cpu_name);
    let external = blit_backend::snapshot();
    let mode = current.mode;
    let compatible = current.registered
        && mode.bpp == 32
        && mode.red_mask_size == 8
        && mode.red_mask_shift == 16
        && mode.green_mask_size == 8
        && mode.green_mask_shift == 8
        && mode.blue_mask_size == 8
        && mode.blue_mask_shift == 0
        && mode.pitch & 3 == 0;
    if external.active && compatible {
        result.flags |= 8 | 16;
        result.backend_kind = 2;
        result.backend_name = external.name;
        result.max_regions = external.max_regions.min(8);
    }
    result
}

pub fn highest_completed_fence() -> u64 {
    let _state = ownership::enter_state();
    unsafe { (*context()).completed_fence }
}

pub fn present_fence_completed(fence: u64) -> bool {
    fence != 0 && fence <= highest_completed_fence()
}
