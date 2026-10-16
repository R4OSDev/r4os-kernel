//! Original direct diagnostic band and retained first-incident text. The
//! diagnostic/fatal caller owns sequencing, just as before the migration; no
//! lock, allocation, storage I/O or new owner is introduced on this path.
use super::{
    firmware,
    framebuffer::{self as fb, Framebuffer},
};
use crate::font::builtin;
use core::{cell::UnsafeCell, mem::MaybeUninit, ptr, slice};
const BG: u32 = 0x101830;
const FG: u32 = 0xf0f0f0;
const CAPACITY: usize = 4096;
struct State {
    started: bool,
    row: u32,
    col: u32,
    cols: u32,
    rows: u32,
    frame: MaybeUninit<Framebuffer>,
    fb_ready: bool,
    active: bool,
    visible: bool,
    saturated: bool,
    next: u64,
    generation: u64,
    resolvable: u64,
    evidence: [u8; CAPACITY],
    len: usize,
}
struct Storage(UnsafeCell<State>);
// SAFETY: original diagnostic/deadman/fatal output admission owns sequencing.
// No mutable Rust state reference spans a firmware-owner call.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State {
    started: false,
    row: 0,
    col: 0,
    cols: 0,
    rows: 0,
    frame: MaybeUninit::uninit(),
    fb_ready: false,
    active: false,
    visible: false,
    saturated: false,
    next: 0,
    generation: 0,
    resolvable: 0,
    evidence: [0; CAPACITY],
    len: 0,
}));
fn state() -> *mut State {
    STATE.0.get()
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TextView {
    pub pointer: *const u8,
    pub len: usize,
}
pub fn incident_view() -> TextView {
    unsafe {
        let s = state();
        TextView {
            pointer: ptr::addr_of!((*s).evidence).cast(),
            len: ptr::addr_of!((*s).len).read(),
        }
    }
}
fn capture_byte(s: &mut State, ch: u8) {
    if s.len >= CAPACITY {
        return;
    }
    s.evidence[s.len] = if ch == b'\n' || ch == b'\r' || (0x20..=0x7e).contains(&ch) {
        ch
    } else {
        b'?'
    };
    s.len += 1;
}
fn start(resolvable: bool) -> u64 {
    unsafe {
        let s = &mut *state();
        if s.active {
            return 0;
        }
        s.next = s.next.wrapping_add(1);
        if s.next == 0 {
            s.next = 1
        }
        s.active = true;
        s.generation = s.next;
        s.resolvable = if resolvable { s.generation } else { 0 };
        s.len = 0;
        s.visible = false;
        s.saturated = false;
        s.row = 0;
        s.col = 0;
        if resolvable { s.generation } else { 0 }
    }
}
fn ensure_incident() {
    if !unsafe { ptr::addr_of!((*state()).active).read() } {
        let _ = start(false);
    }
}
fn framebuffer() -> Option<Framebuffer> {
    unsafe {
        let s = state();
        if !ptr::addr_of!((*s).fb_ready).read() {
            let source = fb::boot_framebuffer()?;
            ptr::addr_of_mut!((*s).frame)
                .cast::<Framebuffer>()
                .write(source);
            if !fb::supports_rgb32(&source) {
                return None;
            }
            ptr::addr_of_mut!((*s).fb_ready).write(true);
        }
        Some(ptr::addr_of!((*s).frame).cast::<Framebuffer>().read())
    }
}
fn prepare_write(f: &Framebuffer) -> bool {
    unsafe {
        let s = &mut *state();
        if !s.started {
            s.started = true;
            s.cols = u32::try_from(f.width / 8).expect("Original diagnostic columns");
            s.rows = u32::try_from(f.height / 8)
                .expect("Original diagnostic rows")
                .min(16);
            s.row = 0;
            s.col = 0;
            s.saturated = false;
        }
        if s.cols == 0 || s.rows == 0 {
            return false;
        }
        if !s.visible {
            fb::rect(f, 0, 0, f.width, u64::from(s.rows) * 8, BG);
            s.visible = true;
            s.row = 0;
            s.col = 0;
            s.saturated = false;
        }
        !s.saturated
    }
}
fn prepare_overlay() {
    let Some(_lease) = firmware::Lease::acquire() else {
        return;
    };
    let Some(f) = framebuffer() else { return };
    let _ = prepare_write(&f);
}
pub fn begin() {
    ensure_incident();
    unsafe { ptr::addr_of_mut!((*state()).resolvable).write(0) }
    prepare_overlay();
}
pub fn begin_resolvable() -> u64 {
    let token = start(true);
    prepare_overlay();
    token
}
pub fn resolve(token: u64) -> bool {
    unsafe {
        let s = &mut *state();
        if token == 0 || !s.active || token != s.generation || token != s.resolvable {
            return false;
        }
        s.active = false;
        s.generation = 0;
        s.resolvable = 0;
        s.visible = false;
        s.saturated = false;
        s.row = 0;
        s.col = 0;
        true
    }
}
pub fn reset_external() {
    unsafe {
        let s = &mut *state();
        s.visible = false;
        s.saturated = false;
        s.row = 0;
        s.col = 0;
    }
}
fn newline(s: &mut State) -> bool {
    s.col = 0;
    if s.rows == 0 || s.saturated {
        return false;
    }
    if s.row + 1 >= s.rows {
        s.saturated = true;
        return false;
    }
    s.row += 1;
    true
}
unsafe fn draw_glyph(f: &Framebuffer, x: u32, y: u32, ch: u8) {
    let glyph = builtin::glyph(u32::from(ch));
    let fg = fb::pack_rgb(f, FG);
    let bg = fb::pack_rgb(f, BG);
    for (gy, &bits) in glyph.iter().enumerate() {
        for gx in 0..8 {
            unsafe {
                fb::put_packed(
                    f,
                    u64::from(x) * 8 + gx,
                    u64::from(y) * 8 + gy as u64,
                    if bits >> (7 - gx) & 1 != 0 { fg } else { bg },
                );
            }
        }
    }
}
/// # Safety
/// Caller retains readable text and original serialized diagnostic admission.
pub unsafe fn write(p: *const u8, len: usize) {
    ensure_incident();
    {
        let s = unsafe { &mut *state() };
        for i in 0..len {
            let ch = unsafe { p.add(i).read() };
            if ch != b'\r' {
                capture_byte(s, ch);
            }
        }
    }
    let Some(_lease) = firmware::Lease::acquire() else {
        return;
    };
    let Some(f) = framebuffer() else { return };
    if !prepare_write(&f) {
        return;
    }
    let s = unsafe { &mut *state() };
    for i in 0..len {
        let ch = unsafe { p.add(i).read() };
        if matches!(ch, b'\r' | b'\n') {
            continue;
        }
        if s.col >= s.cols && !newline(s) {
            return;
        }
        unsafe {
            draw_glyph(
                &f,
                s.col,
                s.row,
                if (0x20..=0x7e).contains(&ch) {
                    ch
                } else {
                    b'?'
                },
            );
        }
        s.col += 1;
    }
}
pub fn end_line() {
    ensure_incident();
    unsafe { capture_byte(&mut *state(), b'\n') }
    let Some(_lease) = firmware::Lease::acquire() else {
        return;
    };
    let Some(f) = framebuffer() else { return };
    if !prepare_write(&f) {
        return;
    }
    let _ = newline(unsafe { &mut *state() });
}
pub fn number(mut value: u64, hex: bool) {
    let mut buf = [0u8; 20];
    let mut i = buf.len();
    let radix = if hex {
        unsafe { write(b"0x".as_ptr(), 2) }
        16
    } else {
        10
    };
    loop {
        i -= 1;
        buf[i] = b"0123456789ABCDEF"[(value % radix) as usize];
        value /= radix;
        if value == 0 {
            break;
        }
    }
    unsafe { write(buf.as_ptr().add(i), buf.len() - i) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_diag_begin(resolvable: bool) -> u64 {
    if resolvable {
        begin_resolvable()
    } else {
        begin();
        0
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_diag_resolve(token: u64) -> bool {
    resolve(token)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_diag_reset() {
    reset_external()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_diag_end_line() {
    end_line()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_diag_number(value: u64, hex: bool) {
    number(value, hex)
}
/// # Safety
/// Caller retains readable text and original serialized diagnostic admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_diag_write(p: *const u8, len: usize) {
    unsafe { write(p, len) }
}
/// # Safety
/// Caller owns the view output; resident evidence keeps its original lifetime
/// until a new incident replaces it. No new ownership is conveyed by this view.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_diag_text(out: *mut TextView) {
    unsafe { out.write(incident_view()) }
}
/// # Safety
/// Original crash caller freezes diagnostic evidence throughout replay.
pub unsafe fn incident_text<'a>() -> &'a [u8] {
    let view = incident_view();
    if view.len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(view.pointer, view.len) }
    }
}
const _: () = {
    assert!(core::mem::size_of::<TextView>() == 16);
    assert!(core::mem::align_of::<TextView>() == 8);
    assert!(core::mem::offset_of!(TextView, pointer) == 0);
    assert!(core::mem::offset_of!(TextView, len) == 8);
};

/// Synchronous text span; the slice owns the complete readable lifetime.
pub fn text(bytes: &[u8]) {
    unsafe { write(bytes.as_ptr(), bytes.len()) }
}
