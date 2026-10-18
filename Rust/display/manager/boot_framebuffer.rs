//! Exact boot_framebuffer operations of the parent owner; no duplicated state.
use super::*;

pub(super) fn bootfb_fill(device: *mut Device, rgb: u32) -> bool {
    let frame = unsafe { (*device).frame };
    if frame.is_null() {
        return false;
    }
    let frame = unsafe { frame.read() };
    unsafe { fb::fill(&frame, rgb) };
    record_present(
        device,
        REASON_FILL,
        0,
        0,
        u32::try_from(frame.width).expect("fill width"),
        u32::try_from(frame.height).expect("fill height"),
        false,
    );
    true
}

pub(super) fn clip_signed_rect(x: i64, y: i64, w: u32, h: u32, mode: Mode) -> Option<Rect> {
    let left = x.max(0);
    let top = y.max(0);
    let right = i64::from(mode.width).min(x + i64::from(w));
    let bottom = i64::from(mode.height).min(y + i64::from(h));
    if left >= right || top >= bottom {
        None
    } else {
        Some(Rect {
            x: u32::try_from(left).expect("clip x"),
            y: u32::try_from(top).expect("clip y"),
            w: u32::try_from(right - left).expect("clip width"),
            h: u32::try_from(bottom - top).expect("clip height"),
        })
    }
}

pub(super) fn bootfb_rect(device: *mut Device, x: i32, y: i32, w: u32, h: u32, rgb: u32) -> bool {
    let frame = unsafe { (*device).frame };
    if frame.is_null() {
        return false;
    }
    let Some(rect) = clip_signed_rect(i64::from(x), i64::from(y), w, h, unsafe {
        (*device).value.mode
    }) else {
        return false;
    };
    unsafe {
        fb::rect(
            &frame.read(),
            u64::from(rect.x),
            u64::from(rect.y),
            u64::from(rect.w),
            u64::from(rect.h),
            rgb,
        );
    }
    record_present(device, REASON_RECT, rect.x, rect.y, rect.w, rect.h, false);
    true
}

pub(super) unsafe fn bootfb_copy_packed32_rect(
    device: *mut Device,
    reason: u8,
    x: u64,
    y: u64,
    w: u64,
    h: u64,
    src: &[u8],
    stride: u64,
) -> bool {
    let frame = unsafe { (*device).frame };
    if frame.is_null() {
        return false;
    }
    let frame = unsafe { frame.read() };
    if !fb::supports_rgb32(&frame) || w == 0 || h == 0 || x >= frame.width || y >= frame.height {
        return false;
    }
    let clipped_w = w.min(frame.width - x);
    let clipped_h = h.min(frame.height - y);
    let stride_bytes = stride * 4;
    let row_bytes = clipped_w * 4;
    if (src.len() as u64) < (clipped_h - 1) * stride_bytes + row_bytes {
        return false;
    }
    for row in 0..clipped_h {
        let offset = usize::try_from(row * stride_bytes).expect("packed row offset");
        let bytes = usize::try_from(row_bytes).expect("packed row bytes");
        let dst = unsafe {
            frame
                .address
                .add(((y + row) * frame.pitch + x * 4) as usize)
        };
        unsafe { copy_to_visible(dst, &src[offset..offset + bytes]) };
    }
    record_present(
        device,
        reason,
        u32::try_from(x).expect("packed x"),
        u32::try_from(y).expect("packed y"),
        u32::try_from(clipped_w).expect("packed width"),
        u32::try_from(clipped_h).expect("packed height"),
        false,
    );
    true
}

pub(super) unsafe fn bootfb_copy_xrgb32_region(
    device: *mut Device,
    region: blit_backend::Region,
    src: &[u8],
    stride: u32,
) -> bool {
    let frame = unsafe { (*device).frame };
    if frame.is_null() {
        return false;
    }
    let frame = unsafe { frame.read() };
    let stride_bytes = u64::from(stride) * 4;
    let row_bytes = u64::from(region.w) * 4;
    let first = (u64::from(region.src_y) * u64::from(stride) + u64::from(region.src_x)) * 4;
    let last_end = first + (u64::from(region.h) - 1) * stride_bytes + row_bytes;
    if last_end > src.len() as u64 {
        return false;
    }
    for y in 0..region.h {
        let offset = usize::try_from(first + u64::from(y) * stride_bytes).expect("XRGB row offset");
        if fb::is_native_xrgb32(&frame) {
            let dst = unsafe {
                frame.address.add(
                    ((u64::from(region.dst_y) + u64::from(y)) * frame.pitch
                        + u64::from(region.dst_x) * 4) as usize,
                )
            };
            unsafe { copy_to_visible(dst, &src[offset..offset + row_bytes as usize]) };
            continue;
        }
        for x in 0..region.w {
            let pixel_offset = offset + x as usize * 4;
            unsafe {
                fb::put_packed(
                    &frame,
                    u64::from(region.dst_x) + u64::from(x),
                    u64::from(region.dst_y) + u64::from(y),
                    fb::pack_rgb(&frame, read_xrgb32(src, pixel_offset)),
                );
            }
        }
    }
    true
}

pub(super) fn valid_present_region(
    region: blit_backend::Region,
    mode: Mode,
    pixels: u32,
    stride: u32,
) -> bool {
    if region.w == 0
        || region.h == 0
        || region.src_x >= stride
        || region.w > stride - region.src_x
        || region.dst_x >= mode.width
        || region.dst_y >= mode.height
        || region.w > mode.width - region.dst_x
        || region.h > mode.height - region.dst_y
    {
        return false;
    }
    let last_row = u64::from(region.src_y) + u64::from(region.h) - 1;
    let last_end = last_row * u64::from(stride) + u64::from(region.src_x) + u64::from(region.w);
    last_end <= u64::from(pixels)
}

pub(super) fn merge_rect(a: Rect, b: Rect) -> Rect {
    let left = a.x.min(b.x);
    let top = a.y.min(b.y);
    let right = (u64::from(a.x) + u64::from(a.w)).max(u64::from(b.x) + u64::from(b.w));
    let bottom = (u64::from(a.y) + u64::from(a.h)).max(u64::from(b.y) + u64::from(b.h));
    Rect {
        x: left,
        y: top,
        w: u32::try_from(right - u64::from(left)).expect("merged rectangle width"),
        h: u32::try_from(bottom - u64::from(top)).expect("merged rectangle height"),
    }
}

pub(super) fn copy_name(out: &mut [u8], name: &[u8]) {
    out.fill(0);
    if out.is_empty() {
        return;
    }
    let count = name.len().min(out.len() - 1);
    if count != 0 {
        out[..count].copy_from_slice(&name[..count]);
    }
}

pub(super) unsafe fn copy_to_visible(dst: *mut u8, src: &[u8]) {
    let word_len = src.len() & !3;
    if dst.addr() & 3 == 0 && src.as_ptr().addr() & 3 == 0 {
        let dst_words = dst.cast::<u32>();
        let src_words = src.as_ptr().cast::<u32>();
        for index in 0..word_len / 4 {
            unsafe {
                dst_words
                    .add(index)
                    .write_volatile(src_words.add(index).read())
            };
        }
        for (index, &byte) in src.iter().enumerate().skip(word_len) {
            unsafe { dst.add(index).write_volatile(byte) };
        }
        return;
    }
    if dst.addr() & 3 == 0 && word_len == src.len() {
        let dst_words = dst.cast::<u32>();
        for index in 0..word_len / 4 {
            let offset = index * 4;
            let value = u32::from(src[offset])
                | (u32::from(src[offset + 1]) << 8)
                | (u32::from(src[offset + 2]) << 16)
                | (u32::from(src[offset + 3]) << 24);
            unsafe { dst_words.add(index).write_volatile(value) };
        }
        return;
    }
    for (index, &byte) in src.iter().enumerate() {
        unsafe { dst.add(index).write_volatile(byte) };
    }
}

pub(super) fn read_xrgb32(src: &[u8], offset: usize) -> u32 {
    if offset + 3 >= src.len() {
        return 0;
    }
    let p = unsafe { src.as_ptr().add(offset) };
    if p.addr() & 3 == 0 {
        unsafe { p.cast::<u32>().read() }
    } else {
        u32::from(src[offset])
            | (u32::from(src[offset + 1]) << 8)
            | (u32::from(src[offset + 2]) << 16)
            | (u32::from(src[offset + 3]) << 24)
    }
}

pub(super) fn bootfb_put_packed32(device: *mut Device, x: u64, y: u64, color: u32) -> bool {
    let frame = unsafe { (*device).frame };
    if frame.is_null() {
        return false;
    }
    unsafe {
        fb::put_packed(&frame.read(), x, y, color);
    }
    true
}

pub(super) fn bootfb_put_xrgb32(device: *mut Device, x: u64, y: u64, rgb: u32) -> bool {
    let frame = unsafe { (*device).frame };
    if frame.is_null() {
        return false;
    }
    let frame = unsafe { frame.read() };
    unsafe {
        fb::put_packed(&frame, x, y, fb::pack_rgb(&frame, rgb));
    }
    true
}

pub(super) unsafe fn bootfb_present_packed32_rect(
    device: *mut Device,
    x: u64,
    y: u64,
    w: u64,
    h: u64,
    src: &[u8],
    stride: u64,
) -> bool {
    unsafe { bootfb_copy_packed32_rect(device, REASON_PACKED32, x, y, w, h, src, stride) }
}
