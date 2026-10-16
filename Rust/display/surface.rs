//! Original generic XRGB32 software surface. Pixel accesses remain raw stores;
//! the caller's exact CPU ownership does not manufacture a DMA exclusivity claim.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: usize,
    pub h: usize,
}
impl Rect {
    pub fn is_empty(self) -> bool {
        self.w == 0 || self.h == 0
    }
    pub fn right(self) -> i32 {
        self.x + i32::try_from(self.w).expect("surface rectangle width")
    }
    pub fn bottom(self) -> i32 {
        self.y + i32::try_from(self.h).expect("surface rectangle height")
    }
    pub fn clip_to(self, width: usize, height: usize) -> Option<Self> {
        if self.is_empty() {
            return None;
        }
        let mut x0 = self.x;
        let mut y0 = self.y;
        let mut x1 = self.right();
        let mut y1 = self.bottom();
        if x0 < 0 {
            x0 = 0
        }
        if y0 < 0 {
            y0 = 0
        }
        let max_x = i32::try_from(width).expect("surface clip width");
        let max_y = i32::try_from(height).expect("surface clip height");
        if x1 > max_x {
            x1 = max_x
        }
        if y1 > max_y {
            y1 = max_y
        }
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        Some(Self {
            x: x0,
            y: y0,
            w: (x1 - x0) as usize,
            h: (y1 - y0) as usize,
        })
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Surface {
    pub pixels: *mut u32,
    pub pixel_count: usize,
    pub width: usize,
    pub height: usize,
    pub pitch_pixels: usize,
    pub format: u32,
    pub kind: u32,
    pub revision: u64,
}
impl Surface {
    pub fn bounds(&self) -> Rect {
        Rect {
            x: 0,
            y: 0,
            w: self.width,
            h: self.height,
        }
    }
    /// # Safety
    /// Caller retains the software pixel span; no write or reference is lent.
    pub unsafe fn line(&self, y: usize) -> (*mut u32, usize) {
        let start = y * self.pitch_pixels;
        let end = start + self.width;
        assert!(end <= self.pixel_count, "surface row bounds");
        (unsafe { self.pixels.add(start) }, self.width)
    }
    /// # Safety
    /// Caller retains CPU write admission for the exact software pixel span.
    pub unsafe fn fill(&mut self, color: u32) {
        for y in 0..self.height {
            let (row, width) = unsafe { self.line(y) };
            for x in 0..width {
                unsafe { row.add(x).write(color) }
            }
        }
        self.revision = self.revision.wrapping_add(1);
    }
    /// # Safety
    /// Caller retains CPU write admission for the exact software pixel span.
    pub unsafe fn fill_rect(&mut self, rect: Rect, color: u32) {
        let Some(clipped) = rect.clip_to(self.width, self.height) else {
            return;
        };
        let x0 = clipped.x as usize;
        let y0 = clipped.y as usize;
        for y in 0..clipped.h {
            let start = (y0 + y) * self.pitch_pixels + x0;
            let end = start + clipped.w;
            assert!(end <= self.pixel_count, "surface rectangle bounds");
            for x in 0..clipped.w {
                unsafe { self.pixels.add(start + x).write(color) }
            }
        }
        self.revision = self.revision.wrapping_add(1);
    }
}
/// # Safety
/// Exact initialized input rectangle and exclusive output C rectangle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_surface_rect(
    op: u32,
    input: *const Rect,
    width: usize,
    height: usize,
    out: *mut Rect,
) -> bool {
    let input = unsafe { input.read() };
    let value = match op {
        0 => input.clip_to(width, height),
        1 => Some(Rect {
            x: input.right(),
            ..Rect::default()
        }),
        2 => Some(Rect {
            y: input.bottom(),
            ..Rect::default()
        }),
        _ => panic!("surface rectangle operation"),
    };
    if let Some(value) = value {
        unsafe { out.write(value) };
        true
    } else {
        false
    }
}
/// # Safety
/// Exact initialized surface retains its pixel allocation; outputs are exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_surface_line(
    input: *const Surface,
    y: usize,
    pixels: *mut *mut u32,
    count: *mut usize,
) {
    let (row, len) = unsafe { (&*input).line(y) };
    unsafe {
        pixels.write(row);
        count.write(len)
    }
}
/// # Safety
/// Caller retains software pixel write admission and exclusive C metadata.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_surface_fill(input: *mut Surface, color: u32) {
    unsafe { (&mut *input).fill(color) }
}
/// # Safety
/// Exact initialized surface/rectangle; caller retains software write admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_surface_fill_rect(
    input: *mut Surface,
    rect: *const Rect,
    color: u32,
) {
    unsafe { (&mut *input).fill_rect(rect.read(), color) }
}
const _: () = {
    use core::mem::{align_of, offset_of, size_of};
    assert!(size_of::<Rect>() == 24 && align_of::<Rect>() == 8 && offset_of!(Rect, w) == 8);
    assert!(
        size_of::<Surface>() == 56
            && align_of::<Surface>() == 8
            && offset_of!(Surface, revision) == 48
    );
};
