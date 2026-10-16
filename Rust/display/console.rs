//! Original early framebuffer console. The single admitted early-console sink
//! owns resident cells; callers retain its framebuffer/firmware writer lifetime.
use super::{console_scroll_buffer as scroll, framebuffer as fb};
use crate::font;
use core::{cell::UnsafeCell, mem::MaybeUninit, ptr, slice};
struct Cells(UnsafeCell<MaybeUninit<[scroll::Cell; scroll::MAX_CELLS]>>);
// SAFETY: one early framebuffer console is active. Its existing output admission
// serializes cell writes, and backing_valid admits reads only after full clear.
unsafe impl Sync for Cells {}
static CELLS: Cells = Cells(UnsafeCell::new(MaybeUninit::uninit()));
fn cell_ptr() -> *mut scroll::Cell {
    CELLS.0.get().cast()
}
#[repr(C)]
pub struct Console {
    pub framebuffer: *mut fb::Framebuffer,
    pub cols: u32,
    pub rows: u32,
    pub cur_x: u32,
    pub cur_y: u32,
    pub fg: u32,
    pub bg: u32,
    pub scale: u32,
    pub margin_left: u32,
    pub margin_top: u32,
    pub margin_right: u32,
    pub margin_bottom: u32,
    pub backing_cols: u32,
    pub backing_rows: u32,
    pub backing_valid: bool,
    pub pixel_backing: *mut u32,
    pub pixel_backing_len: usize,
    pub has_pixel_backing: bool,
    pub pixel_backing_valid: bool,
    pub pixel_backing_width: u64,
    pub pixel_backing_height: u64,
}
impl Console {
    /// # Safety
    /// Frame descriptor/mapping remains stable for this console's full lifetime.
    pub unsafe fn new(framebuffer: *mut fb::Framebuffer) -> Self {
        let frame = unsafe { &*framebuffer };
        let scale = 2;
        let gw = font::glyph_width() * scale;
        let gh = font::glyph_height() * scale;
        let supported = fb::supports_rgb32(frame);
        Self {
            framebuffer,
            cols: if supported {
                u32::try_from(frame.width / u64::from(gw)).expect("Original console columns")
            } else {
                0
            },
            rows: if supported {
                u32::try_from(frame.height / u64::from(gh)).expect("Original console rows")
            } else {
                0
            },
            cur_x: 0,
            cur_y: 0,
            fg: 0xe0e0e0,
            bg: 0x001830,
            scale,
            margin_left: 0,
            margin_top: 0,
            margin_right: 0,
            margin_bottom: 0,
            backing_cols: 0,
            backing_rows: 0,
            backing_valid: false,
            pixel_backing: ptr::NonNull::<u32>::dangling().as_ptr(),
            pixel_backing_len: 0,
            has_pixel_backing: false,
            pixel_backing_valid: false,
            pixel_backing_width: 0,
            pixel_backing_height: 0,
        }
    }
    fn frame(&self) -> &fb::Framebuffer {
        unsafe { &*self.framebuffer }
    }
    pub fn set_font_scale(&mut self, scale: u32) {
        self.scale = scale.max(1);
        self.recalculate_geometry();
        self.invalidate_cells();
    }
    /// # Safety
    /// Caller owns aligned resident pixel storage for the complete console lifetime.
    pub unsafe fn attach_pixels(&mut self, pixels: *mut u32, len: usize) -> bool {
        let Some(bytes) = scroll::required_pixel_bytes(self.frame().width, self.frame().height)
        else {
            return false;
        };
        if len < bytes / 4 {
            return false;
        }
        self.pixel_backing = pixels;
        self.pixel_backing_len = len;
        self.has_pixel_backing = true;
        self.pixel_backing_valid = false;
        true
    }
    pub fn set_colors(&mut self, fg: u32, bg: u32) {
        self.fg = fg;
        self.bg = bg;
    }
    pub fn set_margins(&mut self, left: u32, top: u32, right: u32, bottom: u32) {
        self.margin_left = if left < self.cols { left } else { 0 };
        self.margin_top = if top < self.rows { top } else { 0 };
        self.margin_right = if right < self.cols - self.margin_left {
            right
        } else {
            0
        };
        self.margin_bottom = if bottom < self.rows - self.margin_top {
            bottom
        } else {
            0
        };
        self.cur_x = self.margin_left;
        self.cur_y = self.margin_top;
    }
    pub fn text_cols(&self) -> u32 {
        if self.cols <= self.margin_left + self.margin_right {
            self.cols
        } else {
            self.cols - self.margin_left - self.margin_right
        }
    }
    pub fn text_rows(&self) -> u32 {
        if self.rows <= self.margin_top + self.margin_bottom {
            self.rows
        } else {
            self.rows - self.margin_top - self.margin_bottom
        }
    }
    pub fn set_cursor(&mut self, x: u32, y: u32) {
        let max_x = if self.text_cols() == 0 {
            0
        } else {
            self.margin_left + self.text_cols() - 1
        };
        let max_y = if self.text_rows() == 0 {
            0
        } else {
            self.margin_top + self.text_rows() - 1
        };
        self.cur_x = x.min(max_x);
        self.cur_y = y.min(max_y);
    }
    pub fn clear(&mut self) {
        if !self.usable() {
            return;
        }
        unsafe { fb::fill(self.frame(), self.bg) }
        self.cur_x = self.margin_left;
        self.cur_y = self.margin_top;
        self.reset_cells(self.bg, self.bg, false);
        self.reset_pixels(self.bg, self.bg, false);
    }
    pub fn clear_framed(&mut self, border: u32, inner: u32) {
        if !self.usable() {
            return;
        }
        unsafe { fb::fill(self.frame(), border) }
        self.bg = inner;
        let cw = font::glyph_width() * self.scale;
        let ch = font::glyph_height() * self.scale;
        let x = self.margin_left * cw;
        let y = self.margin_top * ch;
        let w = self.text_cols() * cw;
        let h = (self.rows - self.margin_top - self.margin_bottom) * ch;
        unsafe {
            fb::rect(
                self.frame(),
                u64::from(x),
                u64::from(y),
                u64::from(w),
                u64::from(h),
                inner,
            )
        }
        self.cur_x = self.margin_left;
        self.cur_y = self.margin_top;
        self.reset_cells(border, inner, true);
        self.reset_pixels(border, inner, true);
    }
    pub fn invalidate_external(&mut self) {
        self.invalidate_backing();
    }
    pub fn putc(&mut self, c: u8) {
        if !self.usable() {
            return;
        }
        match c {
            b'\n' => self.newline(),
            b'\r' => self.cur_x = self.margin_left,
            8 => self.backspace(),
            b'\t' => {
                let next = (self.cur_x + 4) & !3;
                while self.cur_x < next && self.cur_x < self.text_right() {
                    self.draw_glyph(b' ');
                    self.cur_x += 1;
                }
            }
            _ => {
                if self.cur_x >= self.text_right() {
                    self.newline()
                }
                self.draw_glyph(c);
                self.cur_x += 1;
            }
        }
    }
    /// # Safety
    /// Raw text retains its supplied readable lifetime to synchronous completion.
    pub unsafe fn puts_raw(&mut self, text: *const u8, len: usize) {
        for i in 0..len {
            self.putc(unsafe { text.add(i).read() });
        }
    }
    pub fn put_hex(&mut self, value: u64, digits: u8) {
        let hex = b"0123456789ABCDEF";
        let mut i = digits;
        while i > 0 {
            i -= 1;
            let shift = u32::from(i) * 4;
            let nibble = ((value >> shift) & 15) as usize;
            self.putc(hex[nibble]);
        }
    }
    pub fn put_dec(&mut self, value: u64) {
        if value == 0 {
            self.putc(b'0');
            return;
        }
        let mut bytes = [0; 20];
        let mut n = value;
        let mut pos = bytes.len();
        while n > 0 {
            pos -= 1;
            bytes[pos] = b'0' + (n % 10) as u8;
            n /= 10;
        }
        unsafe {
            self.puts_raw(bytes[pos..].as_ptr(), bytes.len() - pos);
        }
    }
    fn newline(&mut self) {
        if !self.usable() {
            return;
        }
        self.cur_x = self.margin_left;
        self.cur_y += 1;
        if self.cur_y >= self.text_bottom() {
            self.scroll();
            self.cur_y = self.text_bottom() - 1;
        }
    }
    fn backspace(&mut self) {
        if !self.usable() || self.cur_x <= self.margin_left {
            return;
        }
        self.cur_x -= 1;
        self.draw_glyph(b' ');
    }
    fn draw_glyph(&mut self, c: u8) {
        let gw = font::glyph_width();
        let gh = font::glyph_height();
        let px = self.cur_x * gw * self.scale;
        let py = self.cur_y * gh * self.scale;
        let fg = fb::pack_rgb(self.frame(), self.fg);
        let bg = fb::pack_rgb(self.frame(), self.bg);
        self.remember(self.cur_x, self.cur_y, c, fg, bg);
        let shadow = self.pixel_framebuffer();
        let target = shadow.as_ref().unwrap_or(self.frame());
        for row in 0..gh {
            let bits = font::glyph_row(c, row);
            for col in 0..gw {
                let shift = gw - 1 - col;
                assert!(shift < 8, "Original u3 console glyph shift");
                let color = if (bits >> shift) & 1 == 1 { fg } else { bg };
                for sy in 0..self.scale {
                    for sx in 0..self.scale {
                        unsafe {
                            fb::put_packed(
                                target,
                                u64::from(px + col * self.scale + sx),
                                u64::from(py + row * self.scale + sy),
                                color,
                            );
                        }
                    }
                }
            }
        }
        if shadow.is_some() {
            self.present_pixels(
                u64::from(px),
                u64::from(py),
                u64::from(gw * self.scale),
                u64::from(gh * self.scale),
            );
        }
    }
    fn scroll(&mut self) {
        let line_h = font::glyph_height() * self.scale;
        let cw = font::glyph_width() * self.scale;
        let visible_h = u64::from(self.text_bottom() - self.margin_top) * u64::from(line_h);
        let visible_w = u64::from(self.text_cols()) * u64::from(cw);
        if visible_h <= u64::from(line_h) || visible_w == 0 {
            return;
        }
        let x0 = u64::from(self.margin_left) * u64::from(cw);
        let y0 = u64::from(self.margin_top) * u64::from(line_h);
        let move_h = visible_h - u64::from(line_h);
        let mut moved = false;
        if let Some(count) = self.backing_count() {
            let blank = self.cell(b' ');
            let cells = unsafe { slice::from_raw_parts_mut(cell_ptr(), count) };
            if scroll::scroll_up(
                cells,
                self.cols,
                self.rows,
                self.margin_left,
                self.margin_top,
                self.text_right(),
                self.text_bottom(),
                blank,
            ) {
                moved = true
            } else {
                self.invalidate_backing();
            }
        }
        if self.pixel_framebuffer().is_some() {
            let width = self.frame().width as usize;
            let blank = fb::pack_rgb(self.frame(), self.bg);
            let pixels =
                unsafe { slice::from_raw_parts_mut(self.pixel_backing, self.pixel_backing_len) };
            scroll::scroll_pixels_up(
                pixels,
                width,
                x0 as usize,
                y0 as usize,
                visible_w as usize,
                visible_h as usize,
                line_h as usize,
                blank,
            );
            self.present_pixels(x0, y0, visible_w, visible_h);
            return;
        }
        if moved {
            self.redraw_cells();
            return;
        }
        unsafe {
            fb::move_rect_up(
                self.frame(),
                x0,
                y0,
                visible_w,
                visible_h,
                u64::from(line_h),
            );
            fb::rect(
                self.frame(),
                x0,
                y0 + move_h,
                visible_w,
                u64::from(line_h),
                self.bg,
            );
        }
    }
    fn reset_cells(&mut self, outer_rgb: u32, inner_rgb: u32, framed: bool) {
        let Some(count) = scroll::required_cell_count(self.cols, self.rows) else {
            self.invalidate_backing();
            return;
        };
        let outer = scroll::Cell {
            glyph: b' ',
            foreground: fb::pack_rgb(self.frame(), self.fg),
            background: fb::pack_rgb(self.frame(), outer_rgb),
        };
        let inner = scroll::Cell {
            glyph: b' ',
            foreground: outer.foreground,
            background: fb::pack_rgb(self.frame(), inner_rgb),
        };
        let cells = unsafe { slice::from_raw_parts_mut(cell_ptr(), count) };
        scroll::fill(cells, outer);
        if framed {
            for row in self.margin_top..self.text_bottom() {
                for col in self.margin_left..self.text_right() {
                    cells[self.backing_index(row, col)] = inner;
                }
            }
        }
        self.backing_cols = self.cols;
        self.backing_rows = self.rows;
        self.backing_valid = true;
    }
    fn invalidate_backing(&mut self) {
        self.invalidate_cells();
        self.pixel_backing_valid = false;
    }
    fn invalidate_cells(&mut self) {
        self.backing_valid = false;
        self.backing_cols = 0;
        self.backing_rows = 0;
    }
    fn pixel_framebuffer(&self) -> Option<fb::Framebuffer> {
        let frame = self.frame();
        if !self.pixel_backing_valid
            || self.pixel_backing_width != frame.width
            || self.pixel_backing_height != frame.height
            || !self.has_pixel_backing
        {
            return None;
        }
        let mut out = *frame;
        out.address = self.pixel_backing.cast();
        out.pitch = out.width * 4;
        Some(out)
    }
    fn reset_pixels(&mut self, outer: u32, inner: u32, framed: bool) {
        self.pixel_backing_valid = false;
        if !self.has_pixel_backing {
            return;
        }
        let Some(bytes) = scroll::required_pixel_bytes(self.frame().width, self.frame().height)
        else {
            return;
        };
        if self.pixel_backing_len < bytes / 4 {
            return;
        }
        self.pixel_backing_width = self.frame().width;
        self.pixel_backing_height = self.frame().height;
        let color = fb::pack_rgb(self.frame(), outer);
        unsafe { slice::from_raw_parts_mut(self.pixel_backing, bytes / 4) }.fill(color);
        self.pixel_backing_valid = true;
        if framed {
            let frame = self
                .pixel_framebuffer()
                .expect("Original initialized pixel shadow");
            let cw = font::glyph_width() * self.scale;
            let ch = font::glyph_height() * self.scale;
            unsafe {
                fb::rect(
                    &frame,
                    u64::from(self.margin_left * cw),
                    u64::from(self.margin_top * ch),
                    u64::from(self.text_cols() * cw),
                    u64::from(self.text_rows() * ch),
                    inner,
                );
            }
        }
    }
    fn present_pixels(&self, x: u64, y: u64, w: u64, h: u64) {
        let Some(frame) = self.pixel_framebuffer() else {
            return;
        };
        if x >= frame.width || y >= frame.height {
            return;
        }
        let offset = (y * frame.width + x) as usize;
        assert!(
            offset <= self.pixel_backing_len,
            "Original packed shadow slice"
        );
        unsafe {
            fb::blit_packed(
                self.frame(),
                x,
                y,
                w,
                h,
                self.pixel_backing.add(offset),
                self.pixel_backing_len - offset,
                frame.width as usize,
            );
        }
    }
    fn backing_count(&self) -> Option<usize> {
        if !self.backing_valid || self.backing_cols != self.cols || self.backing_rows != self.rows {
            return None;
        }
        scroll::required_cell_count(self.cols, self.rows)
    }
    fn backing_index(&self, row: u32, col: u32) -> usize {
        row as usize * self.cols as usize + col as usize
    }
    fn cell(&self, glyph: u8) -> scroll::Cell {
        scroll::Cell {
            glyph,
            foreground: fb::pack_rgb(self.frame(), self.fg),
            background: fb::pack_rgb(self.frame(), self.bg),
        }
    }
    fn remember(&self, col: u32, row: u32, glyph: u8, foreground: u32, background: u32) {
        let Some(count) = self.backing_count() else {
            return;
        };
        if col >= self.cols || row >= self.rows {
            return;
        }
        let index = self.backing_index(row, col);
        assert!(index < count);
        unsafe {
            cell_ptr().add(index).write(scroll::Cell {
                glyph,
                foreground,
                background,
            })
        }
    }
    fn redraw_cells(&mut self) {
        let Some(count) = self.backing_count() else {
            return;
        };
        let gw = font::glyph_width();
        let gh = font::glyph_height();
        let cw = gw * self.scale;
        let ch = gh * self.scale;
        for row in self.margin_top..self.text_bottom() {
            for glyph_row in 0..gh {
                for sy in 0..self.scale {
                    let py = row * ch + glyph_row * self.scale + sy;
                    for col in self.margin_left..self.text_right() {
                        let index = self.backing_index(row, col);
                        assert!(index < count);
                        let stored = unsafe { cell_ptr().add(index).read() };
                        let bits = font::glyph_row(stored.glyph, glyph_row);
                        for gc in 0..gw {
                            let shift = gw - 1 - gc;
                            assert!(shift < 8, "Original u3 console glyph shift");
                            let color = if (bits >> shift) & 1 == 1 {
                                stored.foreground
                            } else {
                                stored.background
                            };
                            for sx in 0..self.scale {
                                unsafe {
                                    fb::put_packed(
                                        self.frame(),
                                        u64::from(col * cw + gc * self.scale + sx),
                                        u64::from(py),
                                        color,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    fn usable(&self) -> bool {
        self.cols > 0 && self.rows > 0 && fb::supports_rgb32(self.frame())
    }
    fn recalculate_geometry(&mut self) {
        if !fb::supports_rgb32(self.frame()) {
            self.cols = 0;
            self.rows = 0;
            return;
        }
        let cw = font::glyph_width() * self.scale;
        let ch = font::glyph_height() * self.scale;
        self.cols = if cw == 0 {
            0
        } else {
            u32::try_from(self.frame().width / u64::from(cw)).expect("Original console width")
        };
        self.rows = if ch == 0 {
            0
        } else {
            u32::try_from(self.frame().height / u64::from(ch)).expect("Original console height")
        };
        self.margin_left = 0;
        self.margin_top = 0;
        self.margin_right = 0;
        self.margin_bottom = 0;
        self.cur_x = 0;
        self.cur_y = 0;
    }
    pub fn text_right(&self) -> u32 {
        self.cols - self.margin_right
    }
    pub fn text_bottom(&self) -> u32 {
        self.rows - self.margin_bottom
    }
}

/// # Safety
/// Caller owns the sole admitted console and its stable mapped framebuffer;
/// resident pixel backing/text spans and outputs keep their exact lifetimes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_console_init(
    framebuffer: *mut fb::Framebuffer,
    out: *mut Console,
) {
    let value = unsafe { Console::new(framebuffer) };
    unsafe { out.write(value) }
}

/// # Safety
/// Caller owns the sole admitted console and its stable mapped framebuffer;
/// resident pixel backing/text spans and outputs keep their exact lifetimes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_console_set_font_scale(this: *mut Console, scale: u32) {
    unsafe { (*this).set_font_scale(scale) };
}

/// # Safety
/// Caller owns the sole admitted console and its stable mapped framebuffer;
/// resident pixel backing/text spans and outputs keep their exact lifetimes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_console_attach_pixel_backing(
    this: *mut Console,
    pixels: *mut u32,
    pixels_len: usize,
) -> bool {
    unsafe { (*this).attach_pixels(pixels, pixels_len) }
}

/// # Safety
/// Caller owns the sole admitted console and its stable mapped framebuffer;
/// resident pixel backing/text spans and outputs keep their exact lifetimes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_console_set_colors(this: *mut Console, fg: u32, bg: u32) {
    unsafe { (*this).set_colors(fg, bg) };
}

/// # Safety
/// Caller owns the sole admitted console and its stable mapped framebuffer;
/// resident pixel backing/text spans and outputs keep their exact lifetimes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_console_set_margins(
    this: *mut Console,
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
) {
    unsafe { (*this).set_margins(left, top, right, bottom) };
}

/// # Safety
/// Caller owns the sole admitted console and its stable mapped framebuffer;
/// resident pixel backing/text spans and outputs keep their exact lifetimes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_console_text_cols(this: *const Console) -> u32 {
    unsafe { (*this).text_cols() }
}

/// # Safety
/// Caller owns the sole admitted console and its stable mapped framebuffer;
/// resident pixel backing/text spans and outputs keep their exact lifetimes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_console_text_rows(this: *const Console) -> u32 {
    unsafe { (*this).text_rows() }
}

/// # Safety
/// Caller owns the sole admitted console and its stable mapped framebuffer;
/// resident pixel backing/text spans and outputs keep their exact lifetimes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_console_set_cursor(this: *mut Console, x: u32, y: u32) {
    unsafe { (*this).set_cursor(x, y) };
}

/// # Safety
/// Caller owns the sole admitted console and its stable mapped framebuffer;
/// resident pixel backing/text spans and outputs keep their exact lifetimes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_console_clear(this: *mut Console) {
    unsafe { (*this).clear() };
}

/// # Safety
/// Caller owns the sole admitted console and its stable mapped framebuffer;
/// resident pixel backing/text spans and outputs keep their exact lifetimes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_console_clear_framed(
    this: *mut Console,
    border: u32,
    inner: u32,
) {
    unsafe { (*this).clear_framed(border, inner) };
}

/// # Safety
/// Caller owns the sole admitted console and its stable mapped framebuffer;
/// resident pixel backing/text spans and outputs keep their exact lifetimes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_console_invalidate_backing_for_external_display(
    this: *mut Console,
) {
    unsafe { (*this).invalidate_external() };
}

/// # Safety
/// Caller owns the sole admitted console and its stable mapped framebuffer;
/// resident pixel backing/text spans and outputs keep their exact lifetimes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_console_putc(this: *mut Console, c: u8) {
    unsafe { (*this).putc(c) };
}

/// # Safety
/// Caller owns the sole admitted console and its stable mapped framebuffer;
/// resident pixel backing/text spans and outputs keep their exact lifetimes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_console_puts(this: *mut Console, s: *const u8, s_len: usize) {
    unsafe { (*this).puts_raw(s, s_len) };
}

/// # Safety
/// Caller owns the sole admitted console and its stable mapped framebuffer;
/// resident pixel backing/text spans and outputs keep their exact lifetimes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_console_put_hex(this: *mut Console, value: u64, digits: u8) {
    unsafe { (*this).put_hex(value, digits) };
}

/// # Safety
/// Caller owns the sole admitted console and its stable mapped framebuffer;
/// resident pixel backing/text spans and outputs keep their exact lifetimes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_console_put_dec(this: *mut Console, value: u64) {
    unsafe { (*this).put_dec(value) };
}

const _: () = {
    assert!(core::mem::size_of::<Console>() == 104);
    assert!(core::mem::align_of::<Console>() == 8);
    assert!(core::mem::offset_of!(Console, framebuffer) == 0);
    assert!(core::mem::offset_of!(Console, cols) == 8);
    assert!(core::mem::offset_of!(Console, rows) == 12);
    assert!(core::mem::offset_of!(Console, cur_x) == 16);
    assert!(core::mem::offset_of!(Console, cur_y) == 20);
    assert!(core::mem::offset_of!(Console, fg) == 24);
    assert!(core::mem::offset_of!(Console, bg) == 28);
    assert!(core::mem::offset_of!(Console, scale) == 32);
    assert!(core::mem::offset_of!(Console, margin_left) == 36);
    assert!(core::mem::offset_of!(Console, margin_top) == 40);
    assert!(core::mem::offset_of!(Console, margin_right) == 44);
    assert!(core::mem::offset_of!(Console, margin_bottom) == 48);
    assert!(core::mem::offset_of!(Console, backing_cols) == 52);
    assert!(core::mem::offset_of!(Console, backing_rows) == 56);
    assert!(core::mem::offset_of!(Console, backing_valid) == 60);
    assert!(core::mem::offset_of!(Console, pixel_backing) == 64);
    assert!(core::mem::offset_of!(Console, pixel_backing_len) == 72);
    assert!(core::mem::offset_of!(Console, has_pixel_backing) == 80);
    assert!(core::mem::offset_of!(Console, pixel_backing_valid) == 81);
    assert!(core::mem::offset_of!(Console, pixel_backing_width) == 88);
    assert!(core::mem::offset_of!(Console, pixel_backing_height) == 96);
};
