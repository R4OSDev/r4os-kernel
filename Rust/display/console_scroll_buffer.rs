//! Original bounded cell and ordinary-RAM pixel scrolling, preserving margins.
use core::slice;
pub const MAX_CELLS: usize = 65_536;
pub const MAX_PIXEL_BYTES: usize = 64 * 1024 * 1024;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Cell {
    pub glyph: u32,
    pub foreground: u32,
    pub background: u32,
}
impl Default for Cell {
    fn default() -> Self {
        Self {
            glyph: u32::from(b' '),
            foreground: 0,
            background: 0,
        }
    }
}
pub fn required_cell_count(cols: u32, rows: u32) -> Option<usize> {
    let count = u64::from(cols) * u64::from(rows);
    if count > MAX_CELLS as u64 || count > usize::MAX as u64 {
        None
    } else {
        Some(count as usize)
    }
}
pub fn required_pixel_bytes(width: u64, height: u64) -> Option<usize> {
    if width == 0
        || height == 0
        || width > MAX_PIXEL_BYTES as u64 / 4
        || height > MAX_PIXEL_BYTES as u64 / 4 / width
    {
        None
    } else {
        Some((width * height * 4) as usize)
    }
}
pub fn fill(cells: &mut [Cell], value: Cell) {
    cells.fill(value);
}
#[expect(
    clippy::too_many_arguments,
    reason = "Preserve the framebuffer/scroll geometry contract"
)]
pub fn scroll_pixels_up(
    pixels: &mut [u32],
    stride: usize,
    left: usize,
    top: usize,
    width: usize,
    height: usize,
    rows_up: usize,
    blank: u32,
) {
    let mut row = 0;
    while row + rows_up < height {
        let dst = (top + row) * stride + left;
        let src = (top + row + rows_up) * stride + left;
        assert!(
            dst <= pixels.len()
                && width <= pixels.len() - dst
                && src <= pixels.len()
                && width <= pixels.len() - src,
            "Original shadow row bounds"
        );
        pixels.copy_within(src..src + width, dst);
        row += 1;
    }
    while row < height {
        let dst = (top + row) * stride + left;
        pixels[dst..dst + width].fill(blank);
        row += 1;
    }
}
#[allow(clippy::too_many_arguments)]
pub fn scroll_up(
    cells: &mut [Cell],
    cols: u32,
    rows: u32,
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
    blank: Cell,
) -> bool {
    let Some(required) = required_cell_count(cols, rows) else {
        return false;
    };
    if required > cells.len()
        || left >= right
        || top >= bottom
        || right > cols
        || bottom > rows
        || bottom - top <= 1
    {
        return false;
    }
    for row in top..bottom - 1 {
        for col in left..right {
            cells[row as usize * cols as usize + col as usize] =
                cells[(row as usize + 1) * cols as usize + col as usize];
        }
    }
    for col in left..right {
        cells[(bottom as usize - 1) * cols as usize + col as usize] = blank;
    }
    true
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_scroll_cells(cols: u32, rows: u32) -> usize {
    required_cell_count(cols, rows).unwrap_or(usize::MAX)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_scroll_pixel_bytes(width: u64, height: u64) -> usize {
    required_pixel_bytes(width, height).unwrap_or(usize::MAX)
}
/// # Safety
/// Caller retains exclusive aligned resident cell storage and initialized value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_scroll_fill(p: *mut Cell, len: usize, value: *const Cell) {
    let cells = if len == 0 {
        &mut []
    } else {
        unsafe { slice::from_raw_parts_mut(p, len) }
    };
    fill(cells, unsafe { value.read() });
}
/// # Safety
/// Exact exclusive ordinary-RAM pixels retain the original row/stride bounds.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_scroll_pixels(
    p: *mut u32,
    len: usize,
    stride: usize,
    left: usize,
    top: usize,
    width: usize,
    height: usize,
    rows: usize,
    blank: u32,
) {
    let pixels = if len == 0 {
        &mut []
    } else {
        unsafe { slice::from_raw_parts_mut(p, len) }
    };
    scroll_pixels_up(pixels, stride, left, top, width, height, rows, blank);
}
/// # Safety
/// Caller owns the original exclusive cell slice and stable initialized blank.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_scroll_up(
    p: *mut Cell,
    len: usize,
    cols: u32,
    rows: u32,
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
    blank: *const Cell,
) -> bool {
    let cells = if len == 0 {
        &mut []
    } else {
        unsafe { slice::from_raw_parts_mut(p, len) }
    };
    scroll_up(cells, cols, rows, left, top, right, bottom, unsafe {
        blank.read()
    })
}

const _: () = {
    assert!(core::mem::size_of::<Cell>() == 12);
    assert!(core::mem::align_of::<Cell>() == 4);
    assert!(core::mem::offset_of!(Cell, glyph) == 0);
    assert!(core::mem::offset_of!(Cell, foreground) == 4);
    assert!(core::mem::offset_of!(Cell, background) == 8);
};
