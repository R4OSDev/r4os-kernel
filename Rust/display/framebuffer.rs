//! Original packed32 software framebuffer operations. Volatile device reads
//! and writes retain alignment paths, clipping and forward overlap semantics.
use core::ffi::c_void;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Framebuffer {
    pub address: *mut u8,
    pub width: u64,
    pub height: u64,
    pub pitch: u64,
    pub bpp: u16,
    pub memory_model: u8,
    pub red_mask_size: u8,
    pub red_mask_shift: u8,
    pub green_mask_size: u8,
    pub green_mask_shift: u8,
    pub blue_mask_size: u8,
    pub blue_mask_shift: u8,
    pub unused: [u8; 5],
    pub edid_size: u64,
    pub edid: *const c_void,
}
pub fn supports_rgb32(f: &Framebuffer) -> bool {
    f.bpp == 32 && f.width <= u64::MAX / 4 && f.pitch >= f.width * 4
}
pub fn is_native_xrgb32(f: &Framebuffer) -> bool {
    if !supports_rgb32(f) {
        return false;
    }
    if f.red_mask_size == 0 && f.green_mask_size == 0 && f.blue_mask_size == 0 {
        return true;
    }
    f.red_mask_size == 8
        && f.red_mask_shift == 16
        && f.green_mask_size == 8
        && f.green_mask_shift == 8
        && f.blue_mask_size == 8
        && f.blue_mask_shift == 0
}
fn pack_channel(value: u8, size: u8, shift: u8) -> u32 {
    if size == 0 {
        return 0;
    }
    let scaled = if size >= 8 {
        u32::from(value) << (size - 8)
    } else {
        u32::from(value) >> (8 - size)
    };
    scaled << shift
}
fn unpack_channel(value: u32, size: u8, shift: u8) -> u8 {
    if size == 0 {
        return 0;
    }
    let max = (1u32 << size) - 1;
    let raw = (value >> shift) & max;
    u8::try_from((raw * 255 + max / 2) / max).expect("Original framebuffer channel range")
}
pub fn pack_rgb(f: &Framebuffer, rgb: u32) -> u32 {
    if f.red_mask_size == 0 && f.green_mask_size == 0 && f.blue_mask_size == 0 {
        return rgb;
    }
    pack_channel((rgb >> 16) as u8, f.red_mask_size, f.red_mask_shift)
        | pack_channel((rgb >> 8) as u8, f.green_mask_size, f.green_mask_shift)
        | pack_channel(rgb as u8, f.blue_mask_size, f.blue_mask_shift)
}
pub fn unpack_rgb(f: &Framebuffer, value: u32) -> u32 {
    if f.red_mask_size == 0 && f.green_mask_size == 0 && f.blue_mask_size == 0 {
        return value & 0x00ffffff;
    }
    (u32::from(unpack_channel(value, f.red_mask_size, f.red_mask_shift)) << 16)
        | (u32::from(unpack_channel(value, f.green_mask_size, f.green_mask_shift)) << 8)
        | u32::from(unpack_channel(value, f.blue_mask_size, f.blue_mask_shift))
}
unsafe fn write_pixel32(p: *mut u8, value: u32) {
    if p.addr() & 3 == 0 {
        unsafe { p.cast::<u32>().write_volatile(value) }
        return;
    }
    for (i, b) in value.to_le_bytes().into_iter().enumerate() {
        unsafe { p.add(i).write_volatile(b) }
    }
}
unsafe fn read_pixel32(p: *const u8) -> u32 {
    if p.addr() & 3 == 0 {
        return unsafe { p.cast::<u32>().read_volatile() };
    }
    let bytes = core::array::from_fn(|i| unsafe { p.add(i).read_volatile() });
    u32::from_le_bytes(bytes)
}
/// # Safety
/// Exact caller-held framebuffer mapping/writer lease covers every clipped pixel.
pub unsafe fn pixel(f: &Framebuffer, x: u64, y: u64, rgb: u32) {
    if !supports_rgb32(f) || x >= f.width || y >= f.height {
        return;
    }
    unsafe {
        write_pixel32(
            f.address.add((y * f.pitch + x * 4) as usize),
            pack_rgb(f, rgb),
        );
    }
}
/// # Safety
/// Exact caller-held readable framebuffer mapping covers every clipped pixel.
pub unsafe fn read_packed(f: &Framebuffer, x: u64, y: u64) -> u32 {
    if !supports_rgb32(f) || x >= f.width || y >= f.height {
        return 0;
    }
    unsafe { read_pixel32(f.address.add((y * f.pitch + x * 4) as usize)) }
}
/// # Safety
/// Exact caller-held framebuffer mapping/writer lease covers every clipped pixel.
pub unsafe fn put_packed(f: &Framebuffer, x: u64, y: u64, color: u32) {
    if !supports_rgb32(f) || x >= f.width || y >= f.height {
        return;
    }
    unsafe {
        write_pixel32(f.address.add((y * f.pitch + x * 4) as usize), color);
    }
}
/// # Safety
/// Exact caller-held framebuffer mapping/writer lease covers the clipped rectangle.
pub unsafe fn rect(f: &Framebuffer, x: u64, y: u64, w: u64, h: u64, rgb: u32) {
    if !supports_rgb32(f) || x >= f.width || y >= f.height {
        return;
    }
    let x1 = x + w.min(f.width - x);
    let y1 = y + h.min(f.height - y);
    let color = pack_rgb(f, rgb);
    for row in y..y1 {
        let p = unsafe { f.address.add((row * f.pitch) as usize) };
        for col in x..x1 {
            unsafe {
                write_pixel32(p.add((col * 4) as usize), color);
            }
        }
    }
}
/// # Safety
/// Caller retains the exact framebuffer mapping and complete CPU writer lease.
pub unsafe fn fill(f: &Framebuffer, rgb: u32) {
    unsafe { rect(f, 0, 0, f.width, f.height, rgb) }
}
/// # Safety
/// Caller retains nonaliasing initialized RAM pixels and exact framebuffer mapping
/// through the synchronous present; stride/length are original element bounds.
#[expect(
    clippy::too_many_arguments,
    reason = "Preserve the framebuffer/scroll geometry contract"
)]
pub unsafe fn blit_packed(
    f: &Framebuffer,
    x: u64,
    y: u64,
    w: u64,
    h: u64,
    source: *const u32,
    len: usize,
    stride: usize,
) -> bool {
    if !supports_rgb32(f) || w == 0 || h == 0 || x >= f.width || y >= f.height {
        return false;
    }
    let width = w.min(f.width - x) as usize;
    let height = h.min(f.height - y) as usize;
    if stride < width || len < width || height - 1 > (len - width) / stride {
        return false;
    }
    for row in 0..height {
        let dst = unsafe { f.address.add(((y + row as u64) * f.pitch + x * 4) as usize) };
        let start = row * stride;
        assert!(
            start <= len && width <= len - start,
            "Original packed RAM row bounds"
        );
        for col in 0..width {
            let value = unsafe { source.add(start + col).read() };
            if dst.addr() & 3 == 0 {
                unsafe { dst.cast::<u32>().add(col).write_volatile(value) }
            } else {
                unsafe { write_pixel32(dst.add(col * 4), value) }
            }
        }
    }
    true
}
unsafe fn copy_forward(dst: *mut u8, src: *const u8, len: u64) {
    let mut offset = 0usize;
    let len = len as usize;
    if (dst.addr() | src.addr()) & 7 == 0 {
        let count = len / 8;
        for i in 0..count {
            let v = unsafe { src.cast::<u64>().add(i).read_volatile() };
            unsafe { dst.cast::<u64>().add(i).write_volatile(v) }
        }
        offset = count * 8;
    }
    if ((dst.addr() + offset) | (src.addr() + offset)) & 3 == 0 {
        let count = (len - offset) / 4;
        for i in 0..count {
            let v = unsafe { src.add(offset).cast::<u32>().add(i).read_volatile() };
            unsafe { dst.add(offset).cast::<u32>().add(i).write_volatile(v) }
        }
        offset += count * 4;
    }
    for i in offset..len {
        let value = unsafe { src.add(i).read_volatile() };
        unsafe { dst.add(i).write_volatile(value) }
    }
}
/// # Safety
/// Exact mapping/CPU writer lease covers source and destination device rows.
pub unsafe fn move_rect_up(f: &Framebuffer, x: u64, y: u64, w: u64, h: u64, rows: u64) -> bool {
    if !supports_rgb32(f) || w == 0 || h == 0 || rows == 0 || x >= f.width || y >= f.height {
        return false;
    }
    let width = w.min(f.width - x);
    let height = h.min(f.height - y);
    if rows >= height {
        return false;
    }
    let move_h = height - rows;
    let bytes = width * 4;
    for row in 0..move_h {
        let dst = unsafe { f.address.add(((y + row) * f.pitch + x * 4) as usize) };
        let src = unsafe { f.address.add(((y + rows + row) * f.pitch + x * 4) as usize) };
        unsafe {
            copy_forward(dst, src, bytes);
        }
    }
    true
}
// Every C entry keeps the caller's original mapping/lease, without acquiring
// a second display owner or retaining a framebuffer pointer after return.
/// # Safety
/// f is an aligned initialized framebuffer descriptor with its original lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fb_supports(f: *const Framebuffer) -> bool {
    supports_rgb32(unsafe { &*f })
}
/// # Safety
/// f is an aligned initialized framebuffer descriptor with its original lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fb_native(f: *const Framebuffer) -> bool {
    is_native_xrgb32(unsafe { &*f })
}
/// # Safety
/// Descriptor and exact mapped pixel writer lease remain live through the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fb_pixel(f: *const Framebuffer, x: u64, y: u64, c: u32) {
    unsafe { pixel(&*f, x, y, c) }
}
/// # Safety
/// Descriptor and exact readable device mapping remain live through the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fb_read(f: *const Framebuffer, x: u64, y: u64) -> u32 {
    unsafe { read_packed(&*f, x, y) }
}
/// # Safety
/// Descriptor and exact mapped pixel writer lease remain live through the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fb_put(f: *const Framebuffer, x: u64, y: u64, c: u32) {
    unsafe { put_packed(&*f, x, y, c) }
}
/// # Safety
/// Descriptor and exact mapped pixel writer lease remain live through the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fb_rect(
    f: *const Framebuffer,
    x: u64,
    y: u64,
    w: u64,
    h: u64,
    c: u32,
) {
    unsafe { rect(&*f, x, y, w, h, c) }
}
/// # Safety
/// Descriptor and exact mapped pixel writer lease remain live through the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fb_fill(f: *const Framebuffer, c: u32) {
    unsafe { fill(&*f, c) }
}
/// # Safety
/// Caller retains initialized RAM bounds and the exact framebuffer writer lease.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fb_blit(
    f: *const Framebuffer,
    x: u64,
    y: u64,
    w: u64,
    h: u64,
    p: *const u32,
    len: usize,
    stride: usize,
) -> bool {
    unsafe { blit_packed(&*f, x, y, w, h, p, len, stride) }
}
/// # Safety
/// Caller retains the exact readable/writable device mapping for the clipped move.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fb_move(
    f: *const Framebuffer,
    x: u64,
    y: u64,
    w: u64,
    h: u64,
    rows: u64,
) -> bool {
    unsafe { move_rect_up(&*f, x, y, w, h, rows) }
}
/// # Safety
/// Descriptor is initialized and retained for this pure format conversion.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fb_pack(f: *const Framebuffer, c: u32) -> u32 {
    pack_rgb(unsafe { &*f }, c)
}
/// # Safety
/// Descriptor is initialized and retained for this pure format conversion.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fb_unpack(f: *const Framebuffer, c: u32) -> u32 {
    unpack_rgb(unsafe { &*f }, c)
}

const _: () = {
    assert!(core::mem::size_of::<Framebuffer>() == 64);
    assert!(core::mem::align_of::<Framebuffer>() == 8);
    assert!(core::mem::offset_of!(Framebuffer, address) == 0);
    assert!(core::mem::offset_of!(Framebuffer, width) == 8);
    assert!(core::mem::offset_of!(Framebuffer, height) == 16);
    assert!(core::mem::offset_of!(Framebuffer, pitch) == 24);
    assert!(core::mem::offset_of!(Framebuffer, bpp) == 32);
    assert!(core::mem::offset_of!(Framebuffer, memory_model) == 34);
    assert!(core::mem::offset_of!(Framebuffer, red_mask_size) == 35);
    assert!(core::mem::offset_of!(Framebuffer, red_mask_shift) == 36);
    assert!(core::mem::offset_of!(Framebuffer, green_mask_size) == 37);
    assert!(core::mem::offset_of!(Framebuffer, green_mask_shift) == 38);
    assert!(core::mem::offset_of!(Framebuffer, blue_mask_size) == 39);
    assert!(core::mem::offset_of!(Framebuffer, blue_mask_shift) == 40);
    assert!(core::mem::offset_of!(Framebuffer, unused) == 41);
    assert!(core::mem::offset_of!(Framebuffer, edid_size) == 48);
    assert!(core::mem::offset_of!(Framebuffer, edid) == 56);
};

// Boot info owns an immutable normalized copy; this is a field projection,
// never a second framebuffer mapping or display admission owner.
pub fn boot_framebuffer() -> Option<Framebuffer> {
    let info = crate::boot::info::r4os_rust_boot_info();
    let source = unsafe { (*info).framebuffer };
    if source.is_null() {
        return None;
    }
    let s = unsafe { &*source };
    Some(Framebuffer {
        address: s.address,
        width: s.width,
        height: s.height,
        pitch: s.pitch,
        bpp: s.bpp,
        memory_model: s.memory_model,
        red_mask_size: s.red_mask_size,
        red_mask_shift: s.red_mask_shift,
        green_mask_size: s.green_mask_size,
        green_mask_shift: s.green_mask_shift,
        blue_mask_size: s.blue_mask_size,
        blue_mask_shift: s.blue_mask_shift,
        unused: s.unused,
        edid_size: s.edid_size,
        edid: s.edid,
    })
}
