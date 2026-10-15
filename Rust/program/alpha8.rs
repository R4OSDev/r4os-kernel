//! Pure Alpha8 transport geometry and compact little-endian packing. Original
//! validation/error order keeps padded source rows out of the visible stream.
use core::{mem, ptr};
#[repr(u8)]
#[derive(Clone, Copy)]
pub(super) enum Error {
    Dimensions = 1,
    Stride = 2,
    Source = 3,
    Destination = 4,
    Overflow = 5,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Geometry {
    pub pixel_count: usize,
    pub source_bytes: usize,
    pub packed_words: usize,
}
pub(super) fn validate(
    width: u32,
    height: u32,
    stride: u32,
    source_len: usize,
    max_width: u32,
    max_height: u32,
    max_pixels: usize,
) -> Result<Geometry, Error> {
    if width == 0 || height == 0 || width > max_width || height > max_height {
        return Err(Error::Dimensions);
    }
    if stride < width {
        return Err(Error::Stride);
    }
    let pixels = (width as usize)
        .checked_mul(height as usize)
        .ok_or(Error::Overflow)?;
    if pixels > max_pixels {
        return Err(Error::Dimensions);
    }
    let preceding = (height as usize - 1)
        .checked_mul(stride as usize)
        .ok_or(Error::Overflow)?;
    let source = preceding
        .checked_add(width as usize)
        .ok_or(Error::Overflow)?;
    if source_len < source {
        return Err(Error::Source);
    }
    let rounded = pixels.checked_add(3).ok_or(Error::Overflow)?;
    Ok(Geometry {
        pixel_count: pixels,
        source_bytes: source,
        packed_words: rounded / 4,
    })
}
pub(super) unsafe fn pack(
    destination: *mut u32,
    destination_len: usize,
    source: *const u8,
    source_len: usize,
    width: u32,
    height: u32,
    stride: u32,
) -> Result<usize, Error> {
    let geometry = validate(width, height, stride, source_len, width, height, usize::MAX)?;
    if destination_len < geometry.packed_words {
        return Err(Error::Destination);
    }
    unsafe {
        ptr::write_bytes(destination, 0, geometry.packed_words);
        let mut output = 0;
        for row in 0..height as usize {
            let source_row = row * stride as usize;
            for column in 0..width as usize {
                let shift = ((output & 3) * 8) as u32;
                let word = destination.add(output / 4);
                word.write(
                    word.read() | (u32::from(source.add(source_row + column).read()) << shift),
                );
                output += 1;
            }
        }
    }
    Ok(geometry.packed_words)
}
/// # Safety
/// Exclusive complete Geometry output. Success initializes it; errors leave
/// it unchanged and preserve the original dimensions/stride/size precedence.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_alpha8_validate(
    width: u32,
    height: u32,
    stride: u32,
    source_len: usize,
    max_width: u32,
    max_height: u32,
    max_pixels: usize,
    out: *mut Geometry,
) -> u8 {
    match validate(
        width, height, stride, source_len, max_width, max_height, max_pixels,
    ) {
        Ok(value) => {
            unsafe {
                out.write(value);
            }
            0
        }
        Err(error) => error as u8,
    }
}
/// # Safety
/// Valid original independent source/destination spans and exclusive successful
/// count output. All error checks complete before any destination byte changes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_alpha8_pack(
    destination: *mut u32,
    destination_len: usize,
    source: *const u8,
    source_len: usize,
    width: u32,
    height: u32,
    stride: u32,
    out: *mut usize,
) -> u8 {
    match unsafe {
        pack(
            destination,
            destination_len,
            source,
            source_len,
            width,
            height,
            stride,
        )
    } {
        Ok(value) => {
            unsafe {
                out.write(value);
            }
            0
        }
        Err(error) => error as u8,
    }
}
/// # Safety
/// Readable original packed-word span and exclusive successful byte output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_alpha8_byte(
    words: *const u32,
    len: usize,
    index: usize,
    out: *mut u8,
) -> bool {
    if index / 4 >= len {
        return false;
    }
    let shift = ((index & 3) * 8) as u32;
    unsafe {
        out.write((words.add(index / 4).read() >> shift) as u8);
    }
    true
}
const _: () = {
    assert!(mem::size_of::<Geometry>() == 24);
    assert!(mem::align_of::<Geometry>() == 8);
    assert!(mem::offset_of!(Geometry, pixel_count) == 0);
    assert!(mem::offset_of!(Geometry, source_bytes) == 8);
    assert!(mem::offset_of!(Geometry, packed_words) == 16);
};
