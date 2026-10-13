//! Existing R4B0 splash contract and centered, pitch-aware framebuffer draw.
//! No heap, scheduler or public platform API participates in this early path.
use super::limine::Framebuffer;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Header {
    width: u32,
    height: u32,
    pixel_format: u32,
    pixel_offset: usize,
    payload_size: usize,
}
#[repr(C)]
pub struct Origin {
    x: u64,
    y: u64,
}
fn le16(bytes: &[u8]) -> u16 {
    u16::from_le_bytes([bytes[0], bytes[1]])
}
fn le32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}
fn read(bytes: &[u8], complete: bool) -> Option<Header> {
    if bytes.len() < 32
        || &bytes[..4] != b"R4B0"
        || le16(&bytes[4..6]) != 1
        || le16(&bytes[6..8]) != 32
    {
        return None;
    }
    let header = Header {
        width: le32(&bytes[8..12]),
        height: le32(&bytes[12..16]),
        pixel_format: le32(&bytes[16..20]),
        pixel_offset: le32(&bytes[20..24]) as usize,
        payload_size: le32(&bytes[24..28]) as usize,
    };
    if header.width != 1280 || header.height != 720 || header.pixel_format != 1 {
        return None;
    }
    if complete
        && (header.pixel_offset != 32
            || header.payload_size != 1280 * 720 * 4
            || bytes.len() < 32 + 1280 * 720 * 4)
    {
        return None;
    }
    Some(header)
}
fn origin(width: u64, height: u64, image_width: u32, image_height: u32) -> Option<Origin> {
    if width < image_width as u64 || height < image_height as u64 {
        return None;
    }
    Some(Origin {
        x: (width - image_width as u64) / 2,
        y: (height - image_height as u64) / 2,
    })
}
fn rgb32(frame: &Framebuffer) -> bool {
    frame.bpp == 32 && frame.width <= u64::MAX / 4 && frame.pitch >= frame.width * 4
}
fn channel(value: u8, size: u8, shift: u8) -> u32 {
    if size == 0 {
        return 0;
    }
    let scaled = if size >= 8 {
        (value as u32) << (size - 8)
    } else {
        (value as u32) >> (8 - size)
    };
    scaled << shift
}
fn pack(frame: &Framebuffer, rgb: u32) -> u32 {
    if frame.red_mask_size == 0 && frame.green_mask_size == 0 && frame.blue_mask_size == 0 {
        return rgb;
    }
    channel((rgb >> 16) as u8, frame.red_mask_size, frame.red_mask_shift)
        | channel(
            (rgb >> 8) as u8,
            frame.green_mask_size,
            frame.green_mask_shift,
        )
        | channel(rgb as u8, frame.blue_mask_size, frame.blue_mask_shift)
}
unsafe fn draw(frame: &Framebuffer, bytes: &[u8], header: Header) -> bool {
    if !rgb32(frame) {
        return false;
    }
    let Some(origin) = origin(frame.width, frame.height, header.width, header.height) else {
        return false;
    };
    let Some(pixel_bytes) = (header.width as usize)
        .checked_mul(header.height as usize)
        .and_then(|n| n.checked_mul(4))
    else {
        return false;
    };
    if header.payload_size != pixel_bytes
        || header.pixel_offset > bytes.len()
        || pixel_bytes > bytes.len() - header.pixel_offset
    {
        return false;
    }
    for y in 0..header.height {
        for x in 0..header.width {
            let source =
                header.pixel_offset + (y as usize * header.width as usize + x as usize) * 4;
            let color = pack(frame, le32(&bytes[source..source + 4]) & 0x00ff_ffff);
            let offset = (origin.y + y as u64) * frame.pitch + (origin.x + x as u64) * 4;
            // MMIO is not a Rust allocation. Keep raw device addresses and
            // volatile stores; unaligned firmware layouts use exact bytes.
            let destination = frame.address.wrapping_add(offset as usize);
            if destination.addr() & 3 == 0 {
                unsafe { destination.cast::<u32>().write_volatile(color) };
            } else {
                for (index, byte) in color.to_le_bytes().into_iter().enumerate() {
                    unsafe { destination.wrapping_add(index).write_volatile(byte) };
                }
            }
        }
    }
    true
}
#[unsafe(no_mangle)]
/// # Safety
/// bytes retains len readable bytes; output owns one writable C Header.
pub unsafe extern "C" fn r4os_rust_r4b_read(
    bytes: *const u8,
    len: usize,
    output: *mut Header,
    complete: bool,
) -> bool {
    let data = if len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(bytes, len) }
    };
    let Some(header) = read(data, complete) else {
        return false;
    };
    if !output.is_null() {
        unsafe { output.write(header) };
    }
    true
}
#[unsafe(no_mangle)]
/// # Safety
/// output owns a writable, aligned Origin until return.
pub unsafe extern "C" fn r4os_rust_r4b_origin(
    width: u64,
    height: u64,
    image_width: u32,
    image_height: u32,
    output: *mut Origin,
) -> bool {
    let Some(value) = origin(width, height, image_width, image_height) else {
        return false;
    };
    unsafe { output.write(value) };
    true
}
#[unsafe(no_mangle)]
/// # Safety
/// frame metadata and its full pitched MMIO mapping stay live and writable
/// under the boot-display owner. bytes/header are readable and non-aliasing.
pub unsafe extern "C" fn r4os_rust_r4b_draw_header(
    frame: *const Framebuffer,
    bytes: *const u8,
    len: usize,
    header: *const Header,
) -> bool {
    let data = if len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(bytes, len) }
    };
    unsafe { draw(&*frame, data, header.read()) }
}
#[unsafe(no_mangle)]
/// # Safety
/// Same mapping/borrow contract as draw_header; header is validated here.
pub unsafe extern "C" fn r4os_rust_r4b_draw(
    frame: *const Framebuffer,
    bytes: *const u8,
    len: usize,
) -> bool {
    let data = if len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(bytes, len) }
    };
    let Some(header) = read(data, true) else {
        return false;
    };
    unsafe { draw(&*frame, data, header) }
}
