//! Original bounded boot status/progress renderer. Bootstrap/log callers retain
//! the existing serialized boot-output lifetime; handoff retires without repaint.
use super::framebuffer::{self as fb, Framebuffer};
use crate::{
    boot::{limine, r4b},
    font::builtin,
};
use core::{cell::UnsafeCell, ptr, slice};
pub const MAX_STATUS_BYTES: usize = 32;
const TRACK: u32 = 0x041318;
const TRACK_LIGHT: u32 = 0xc8c8c8;
const TRACK_DARK: u32 = 0x06242a;
const PROGRESS: u32 = 0xd9c75e;
const PROGRESS_HILITE: u32 = 0xfff2a0;
const BACKGROUND: u32 = 0x041318;
const FOREGROUND: u32 = 0xf0f0f0;
const DETAIL: u32 = 0xb8d5d8;
const ERROR: u32 = 0xff8a80;
static ASSET: &[u8] = include_bytes!("../../Assets/Bootscreen/Generated/BOOTSCREEN.R4B");
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Geometry {
    pub progress_x: u64,
    pub progress_y: u64,
    pub progress_w: u64,
    pub progress_h: u64,
    pub status_x: u64,
    pub status_y: u64,
    pub status_w: u64,
    pub status_scale: u64,
}
impl Geometry {
    pub const EMPTY: Self = Self {
        progress_x: 0,
        progress_y: 0,
        progress_w: 0,
        progress_h: 0,
        status_x: 0,
        status_y: 0,
        status_w: 0,
        status_scale: 1,
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct LineText {
    pub bytes: [u8; MAX_STATUS_BYTES],
    pub len: usize,
}
impl LineText {
    pub const EMPTY: Self = Self {
        bytes: [0; MAX_STATUS_BYTES],
        len: 0,
    };
    pub fn clear(&mut self) {
        self.bytes.fill(0);
        self.len = 0;
    }
    /// # Safety
    /// Caller retains readable text and original serialized boot-output admission; line outputs are exclusive and the active framebuffer writer remains resident until handoff.
    pub unsafe fn set_raw(&mut self, p: *const u8, len: usize) {
        self.clear();
        for i in 0..len.min(MAX_STATUS_BYTES) {
            let ch = unsafe { p.add(i).read() };
            self.bytes[self.len] = if (0x20..=0x7e).contains(&ch) {
                ch
            } else if matches!(ch, b'\r' | b'\n' | b'\t') {
                b' '
            } else {
                b'?'
            };
            self.len += 1;
        }
        while self.len > 0 && self.bytes[self.len - 1] == b' ' {
            self.len -= 1;
        }
    }
}
#[repr(transparent)]
pub struct PublicLine(UnsafeCell<LineText>);
// SAFETY: original boot-output sequencing owns these lines. Temporary C aliases
// expose the same resident bytes to the original serialized owner fixtures.
unsafe impl Sync for PublicLine {}
#[unsafe(no_mangle)]
pub static r4os_rust_boot_screen_current_status: PublicLine =
    PublicLine(UnsafeCell::new(LineText::EMPTY));
#[unsafe(no_mangle)]
pub static r4os_rust_boot_screen_current_detail: PublicLine =
    PublicLine(UnsafeCell::new(LineText::EMPTY));
#[unsafe(no_mangle)]
pub static r4os_rust_boot_screen_current_error: PublicLine =
    PublicLine(UnsafeCell::new(LineText::EMPTY));
struct State {
    frame: *mut Framebuffer,
    geometry: Geometry,
    last_phase: u8,
}
struct Storage(UnsafeCell<State>);
// SAFETY: same serialized boot-output admission as the original state/lines;
// no state reference spans another owner or externally callable callback.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State {
    frame: ptr::null_mut(),
    geometry: Geometry::EMPTY,
    last_phase: 4,
}));
fn status() -> *mut LineText {
    r4os_rust_boot_screen_current_status.0.get()
}
fn detail() -> *mut LineText {
    r4os_rust_boot_screen_current_detail.0.get()
}
fn error() -> *mut LineText {
    r4os_rust_boot_screen_current_error.0.get()
}
pub fn geometry(f: &Framebuffer) -> Geometry {
    let w = if f.width >= 960 {
        420
    } else if f.width >= 320 {
        f.width / 2
    } else {
        f.width
    };
    let h = if f.height >= 480 { 14 } else { 8 };
    let bottom = if f.height >= 480 { 96 } else { 24 };
    let x = if f.width > w { (f.width - w) / 2 } else { 0 };
    let default_y = if f.height > bottom + h {
        f.height - bottom
    } else if f.height > h + 4 {
        f.height - h - 4
    } else {
        0
    };
    let scale = if w >= 384 && f.height >= 360 { 2 } else { 1 };
    let status_h = 8 * scale * 2 + 3;
    let after = 6 + status_h + 4;
    let latest = if f.height > h + after {
        f.height - h - after
    } else {
        0
    };
    let y = default_y.min(latest);
    Geometry {
        progress_x: x,
        progress_y: y,
        progress_w: w,
        progress_h: h,
        status_x: x,
        status_y: y + h + 6,
        status_w: w,
        status_scale: scale,
    }
}
pub fn status_bottom(g: Geometry) -> u64 {
    g.status_y + 8 * g.status_scale.max(1) * 2 + 3
}
fn limine_frame(f: &Framebuffer) -> limine::Framebuffer {
    limine::Framebuffer {
        address: f.address,
        width: f.width,
        height: f.height,
        pitch: f.pitch,
        bpp: f.bpp,
        memory_model: f.memory_model,
        red_mask_size: f.red_mask_size,
        red_mask_shift: f.red_mask_shift,
        green_mask_size: f.green_mask_size,
        green_mask_shift: f.green_mask_shift,
        blue_mask_size: f.blue_mask_size,
        blue_mask_shift: f.blue_mask_shift,
        unused: f.unused,
        edid_size: f.edid_size,
        edid: f.edid,
    }
}
/// # Safety
/// Caller retains exact readable asset bytes and the complete framebuffer writer.
pub unsafe fn render_prepared(frame: &Framebuffer, bytes: *const u8, len: usize) -> bool {
    let canonical = limine_frame(frame);
    unsafe { r4b::r4os_rust_r4b_draw(&canonical, bytes, len) }
}
/// # Safety
/// Frame descriptor/mapping remains resident through boot status and handoff.
pub unsafe fn render(frame: *mut Framebuffer) -> u8 {
    let f = unsafe { &*frame };
    if !fb::supports_rgb32(f) || !unsafe { render_prepared(f, ASSET.as_ptr(), ASSET.len()) } {
        return 1;
    }
    let g = geometry(f);
    unsafe {
        let s = STATE.0.get();
        ptr::addr_of_mut!((*s).frame).write(frame);
        ptr::addr_of_mut!((*s).geometry).write(g);
        ptr::addr_of_mut!((*s).last_phase).write(4);
        (*status()).clear();
        (*detail()).clear();
        (*error()).clear();
        draw_progress(f, g, 4);
        redraw();
    }
    0
}
pub fn is_active() -> bool {
    unsafe { !ptr::addr_of!((*STATE.0.get()).frame).read().is_null() }
}
pub fn phase(phase: u8) {
    let s = STATE.0.get();
    let frame = unsafe { ptr::addr_of!((*s).frame).read() };
    if frame.is_null() {
        return;
    }
    let f = unsafe { &*frame };
    if !fb::supports_rgb32(f) || phase < unsafe { ptr::addr_of!((*s).last_phase).read() } {
        return;
    }
    unsafe {
        ptr::addr_of_mut!((*s).last_phase).write(phase);
        draw_progress(f, ptr::addr_of!((*s).geometry).read(), phase);
    }
}
unsafe fn redraw() {
    let s = STATE.0.get();
    let frame = unsafe { ptr::addr_of!((*s).frame).read() };
    if frame.is_null() {
        return;
    }
    let f = unsafe { &*frame };
    if !fb::supports_rgb32(f) {
        return;
    }
    let g = unsafe { ptr::addr_of!((*s).geometry).read() };
    let primary = unsafe { status().read() };
    let e = unsafe { error().read() };
    let d = unsafe { detail().read() };
    let (secondary, color) = if e.len != 0 { (e, ERROR) } else { (d, DETAIL) };
    unsafe {
        draw_status_rows(
            f,
            g,
            &primary.bytes[..primary.len],
            &secondary.bytes[..secondary.len],
            color,
        );
    }
}
/// # Safety
/// Caller retains readable text and original serialized boot-output admission; line outputs are exclusive and the active framebuffer writer remains resident until handoff.
pub unsafe fn set_status(p: *const u8, len: usize) {
    unsafe {
        (*status()).set_raw(p, len);
        redraw();
    }
}
/// # Safety
/// Caller retains readable text and original serialized boot-output admission; line outputs are exclusive and the active framebuffer writer remains resident until handoff.
pub unsafe fn set_error(p: *const u8, len: usize) {
    if unsafe { ptr::addr_of!((*error()).len).read() } != 0 {
        return;
    }
    unsafe {
        (*detail()).clear();
        (*error()).set_raw(p, len);
        redraw();
    }
}
/// # Safety
/// Caller retains readable text and original serialized boot-output admission; line outputs are exclusive and the active framebuffer writer remains resident until handoff.
pub unsafe fn set_detail(p: *const u8, len: usize) {
    if unsafe { ptr::addr_of!((*error()).len).read() } != 0 {
        return;
    }
    unsafe {
        (*detail()).set_raw(p, len);
        redraw();
    }
}
pub fn clear_detail() {
    unsafe {
        (*detail()).clear();
        redraw();
    }
}
pub fn complete() {
    unsafe {
        let s = STATE.0.get();
        ptr::addr_of_mut!((*s).last_phase).write(100);
        ptr::addr_of_mut!((*s).frame).write(ptr::null_mut());
        ptr::addr_of_mut!((*s).geometry).write(Geometry::EMPTY);
    }
}
pub fn copy_line_part(out: &mut [u8], start: usize, value: &[u8]) -> usize {
    let mut len = start.min(out.len());
    for &ch in value {
        if len >= out.len() {
            break;
        }
        out[len] = ch;
        len += 1;
    }
    len
}
fn prefixed(prefix: &[u8], p: *const u8, len: usize, kind: u8) {
    if kind != 0 && unsafe { ptr::addr_of!((*error()).len).read() } != 0 {
        return;
    }
    let mut line = [0u8; MAX_STATUS_BYTES];
    let n = copy_line_part(&mut line, 0, prefix);
    let value = if len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(p, len) }
    };
    let n = copy_line_part(&mut line, n, value);
    unsafe {
        match kind {
            0 => set_status(line.as_ptr(), n),
            1 => set_detail(line.as_ptr(), n),
            2 => set_error(line.as_ptr(), n),
            _ => unreachable!(),
        }
    }
}
/// # Safety
/// Caller retains readable name bytes and serialized boot-output admission.
pub unsafe fn driver(p: *const u8, len: usize) {
    prefixed(b"Treiber: ", p, len, 0)
}
/// # Safety
/// Caller retains readable name bytes and serialized boot-output admission.
pub unsafe fn driver_error(p: *const u8, len: usize) {
    prefixed(b"Fehler: ", p, len, 2)
}
/// # Safety
/// Caller retains readable name bytes and serialized boot-output admission.
pub unsafe fn service(p: *const u8, len: usize) {
    prefixed(b"Dienst: ", p, len, 1)
}
/// # Safety
/// Caller retains readable name bytes and serialized boot-output admission.
pub unsafe fn service_error(p: *const u8, len: usize) {
    prefixed(b"Fehler: ", p, len, 2)
}
/// # Safety
/// Caller retains readable text and original serialized boot-output admission; line outputs are exclusive and the active framebuffer writer remains resident until handoff.
pub unsafe fn service_stage(p: *const u8, len: usize, stage: *const u8, stage_len: usize) {
    if unsafe { ptr::addr_of!((*error()).len).read() } != 0 {
        return;
    }
    let name = if len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(p, len) }
    };
    let value = if stage_len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(stage, stage_len) }
    };
    let mut line = [0u8; MAX_STATUS_BYTES];
    let n = copy_line_part(&mut line, 0, name);
    let n = copy_line_part(&mut line, n, b": ");
    let n = copy_line_part(&mut line, n, value);
    unsafe { set_detail(line.as_ptr(), n) }
}
unsafe fn draw_glyph(f: &Framebuffer, x: u64, y: u64, ch: u8, scale: u64, color: u32) {
    let glyph = builtin::glyph(u32::from(if (0x20..=0x7e).contains(&ch) {
        ch
    } else {
        b'?'
    }));
    for row in 0..8u64 {
        let bits = glyph[row as usize];
        for col in 0..8u64 {
            if (bits >> (7 - col)) & 1 == 0 {
                continue;
            }
            unsafe {
                fb::rect(f, x + col * scale, y + row * scale, scale, scale, color);
            }
        }
    }
}
unsafe fn draw_status_line(f: &Framebuffer, g: Geometry, y: u64, text: &[u8], color: u32) {
    if g.status_w <= 8 {
        return;
    }
    let scale = g.status_scale.max(1);
    let cw = 8 * scale;
    let available = g.status_w - 8;
    let count = text.len().min((available / cw) as usize);
    for (index, &ch) in text[..count].iter().enumerate() {
        unsafe {
            draw_glyph(f, g.status_x + 4 + index as u64 * cw, y, ch, scale, color);
        }
    }
}
/// # Safety
/// Caller retains the complete framebuffer writer and immutable text spans.
pub unsafe fn draw_status_rows(
    f: &Framebuffer,
    g: Geometry,
    status: &[u8],
    secondary: &[u8],
    color: u32,
) {
    if g.status_w == 0 || g.status_x >= f.width || g.status_y >= f.height {
        return;
    }
    let row_h = 8 * g.status_scale.max(1);
    let second = g.status_y + row_h + 3;
    if row_h == 0 || second >= f.height {
        return;
    }
    unsafe {
        fb::rect(f, g.status_x, g.status_y, g.status_w, row_h, BACKGROUND);
        fb::rect(f, g.status_x, second, g.status_w, row_h, BACKGROUND);
        draw_status_line(f, g, g.status_y, status, FOREGROUND);
        draw_status_line(f, g, second, secondary, color);
    }
}
unsafe fn draw_track(f: &Framebuffer, g: Geometry) {
    if g.progress_w < 4 || g.progress_h < 3 {
        return;
    }
    unsafe {
        fb::rect(
            f,
            g.progress_x,
            g.progress_y,
            g.progress_w,
            g.progress_h,
            TRACK_DARK,
        );
        fb::rect(f, g.progress_x, g.progress_y, g.progress_w, 1, TRACK_LIGHT);
        fb::rect(f, g.progress_x, g.progress_y, 1, g.progress_h, TRACK_LIGHT);
        fb::rect(
            f,
            g.progress_x + 1,
            g.progress_y + 1,
            g.progress_w - 2,
            g.progress_h - 2,
            TRACK,
        );
    }
}
unsafe fn draw_progress(f: &Framebuffer, g: Geometry, percent: u8) {
    if g.progress_w < 4
        || g.progress_h < 3
        || g.progress_x >= f.width
        || g.progress_y >= f.height
        || g.progress_y + g.progress_h + 2 >= f.height
    {
        return;
    }
    unsafe { draw_track(f, g) }
    let x = g.progress_x + 2;
    let y = g.progress_y + 2;
    let w = g.progress_w.saturating_sub(4);
    let h = if g.progress_h > 4 {
        g.progress_h - 4
    } else {
        1
    };
    let fill = w * u64::from(percent.min(100)) / 100;
    if fill == 0 {
        return;
    }
    unsafe {
        fb::rect(f, x, y, fill, h, PROGRESS);
        if h > 1 {
            fb::rect(f, x, y, fill, 1, PROGRESS_HILITE);
        }
    }
}

/// # Safety
/// Caller owns an aligned line record and readable text for this synchronous call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_boot_screen_line(line: *mut LineText, p: *const u8, len: usize) {
    unsafe { (*line).set_raw(p, len) }
}
/// # Safety
/// Caller retains serialized boot-output admission and readable text; this call
/// does not retain the text. Operations keep the original first-error boundary.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_boot_screen_text(
    op: u32,
    p: *const u8,
    len: usize,
    q: *const u8,
    qlen: usize,
) {
    unsafe {
        match op {
            0 => set_status(p, len),
            1 => set_error(p, len),
            2 => set_detail(p, len),
            3 => driver(p, len),
            4 => driver_error(p, len),
            5 => service(p, len),
            6 => service_error(p, len),
            7 => service_stage(p, len, q, qlen),
            _ => unreachable!(),
        }
    }
}
/// # Safety
/// Caller retains a resident framebuffer writer until handoff retires it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_boot_screen_render(frame: *mut Framebuffer) -> u8 {
    unsafe { render(frame) }
}
/// # Safety
/// Caller owns the framebuffer writer and readable asset for this synchronous draw.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_boot_screen_prepared(
    frame: *const Framebuffer,
    p: *const u8,
    len: usize,
) -> bool {
    unsafe { render_prepared(&*frame, p, len) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_boot_screen_phase(value: u8) {
    phase(value)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_boot_screen_active() -> bool {
    is_active()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_boot_screen_clear_detail() {
    clear_detail()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_boot_screen_complete() {
    complete()
}
/// # Safety
/// Caller retains a readable descriptor and owns the aligned geometry output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_boot_screen_geometry(f: *const Framebuffer, out: *mut Geometry) {
    unsafe { out.write(geometry(&*f)) }
}
/// # Safety
/// Caller retains the exact framebuffer writer and immutable text/geometry inputs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_boot_screen_rows(
    f: *const Framebuffer,
    g: *const Geometry,
    p: *const u8,
    len: usize,
    q: *const u8,
    qlen: usize,
    color: u32,
) {
    let a = if len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(p, len) }
    };
    let b = if qlen == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(q, qlen) }
    };
    unsafe { draw_status_rows(&*f, g.read(), a, b, color) }
}
/// # Safety
/// Caller provides one initialized geometry record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_boot_screen_bottom(g: *const Geometry) -> u64 {
    status_bottom(unsafe { g.read() })
}
const _: () = {
    assert!(core::mem::size_of::<Geometry>() == 64);
    assert!(core::mem::align_of::<Geometry>() == 8);
    assert!(core::mem::size_of::<LineText>() == 40);
    assert!(core::mem::align_of::<LineText>() == 8);
    assert!(core::mem::offset_of!(LineText, bytes) == 0);
    assert!(core::mem::offset_of!(LineText, len) == 32);
    assert!(core::mem::offset_of!(Geometry, progress_x) == 0);
    assert!(core::mem::offset_of!(Geometry, progress_y) == 8);
    assert!(core::mem::offset_of!(Geometry, progress_w) == 16);
    assert!(core::mem::offset_of!(Geometry, progress_h) == 24);
    assert!(core::mem::offset_of!(Geometry, status_x) == 32);
    assert!(core::mem::offset_of!(Geometry, status_y) == 40);
    assert!(core::mem::offset_of!(Geometry, status_w) == 48);
    assert!(core::mem::offset_of!(Geometry, status_scale) == 56);
};
