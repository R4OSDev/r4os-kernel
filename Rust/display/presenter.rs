//! Existing source/destination normalization. Only DisplayManager selects the
//! backend and owns productive presentation counters, fences and fallback.
use super::{blit_backend, core_types::PresentOutcome, manager, surface};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct View {
    pub pixels: *const u32,
    pub pixel_count: usize,
    pub width: usize,
    pub height: usize,
    pub pitch_pixels: usize,
    pub format: u32,
    pub revision: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Region {
    pub source_x: u32,
    pub source_y: u32,
    pub destination_x: u32,
    pub destination_y: u32,
    pub w: u32,
    pub h: u32,
}
#[repr(C)]
pub struct Request {
    pub source: View,
    pub regions: *const Region,
    pub regions_len: usize,
    pub reason: u32,
    pub source_generation: u64,
    pub input_tick: u64,
    pub input_tick_valid: bool,
}
/// # Safety
/// Exact readable source and region ranges stay retained through the single
/// synchronous display submission, under the caller's existing task lifetime.
pub unsafe fn present_xrgb32(request: &Request) -> PresentOutcome {
    let failed = PresentOutcome {
        source_generation: request.source_generation,
        ..PresentOutcome::default()
    };
    if request.source.format != 0 || request.regions_len == 0 || request.regions_len > 8 {
        return failed;
    }
    let regions = unsafe { core::slice::from_raw_parts(request.regions, request.regions_len) };
    let mut normalized = [blit_backend::Region::default(); 8];
    for (index, region) in regions.iter().enumerate() {
        if region.w == 0
            || region.h == 0
            || u64::from(region.source_x) + u64::from(region.w) > request.source.width as u64
            || u64::from(region.source_y) + u64::from(region.h) > request.source.height as u64
        {
            return failed;
        }
        normalized[index] = blit_backend::Region {
            dst_x: region.destination_x,
            dst_y: region.destination_y,
            src_x: region.source_x,
            src_y: region.source_y,
            w: region.w,
            h: region.h,
        };
    }
    if request.source.pixel_count > u32::MAX as usize
        || request.source.pitch_pixels > u32::MAX as usize
    {
        return failed;
    }
    unsafe {
        manager::present_xrgb32_regions(
            request.source.pixels,
            request.source.pixel_count as u32,
            request.source.pitch_pixels as u32,
            &normalized[..regions.len()],
            request.source_generation,
            request.input_tick,
            request.input_tick_valid,
        )
    }
}
/// # Safety
/// Exact surface read lifetime covers its clipped view and CPU presentation.
pub unsafe fn present_rect(
    source: &surface::Surface,
    rect: surface::Rect,
    reason: u32,
) -> PresentOutcome {
    let Some(clipped) = rect.clip_to(source.width, source.height) else {
        return PresentOutcome::default();
    };
    let x = u32::try_from(clipped.x).expect("present clipped x");
    let y = u32::try_from(clipped.y).expect("present clipped y");
    let region = Region {
        source_x: x,
        source_y: y,
        destination_x: x,
        destination_y: y,
        w: u32::try_from(clipped.w).expect("present clipped width"),
        h: u32::try_from(clipped.h).expect("present clipped height"),
    };
    unsafe {
        present_xrgb32(&Request {
            source: View {
                pixels: source.pixels,
                pixel_count: source.pixel_count,
                width: source.width,
                height: source.height,
                pitch_pixels: source.pitch_pixels,
                format: source.format,
                revision: source.revision,
            },
            regions: &region,
            regions_len: 1,
            reason,
            source_generation: source.revision,
            input_tick: 0,
            input_tick_valid: false,
        })
    }
}
/// # Safety
/// Selected exact copied request/surface/rectangle and writable complete result
/// remain retained through their original synchronous presentation lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_presenter(
    op: u32,
    input: *const core::ffi::c_void,
    rect: *const surface::Rect,
    reason: u32,
    out: *mut PresentOutcome,
) {
    let value = match op {
        0 => unsafe { present_xrgb32(&*input.cast::<Request>()) },
        1 => unsafe { present_rect(&*input.cast::<surface::Surface>(), rect.read(), reason) },
        _ => panic!("presenter operation"),
    };
    unsafe { out.write(value) };
}
const _: () = {
    assert!(
        core::mem::size_of::<View>() == 56
            && core::mem::size_of::<Request>() == 104
            && core::mem::size_of::<Region>() == 24
    );
};
