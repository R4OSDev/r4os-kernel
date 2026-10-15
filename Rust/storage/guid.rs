//! Original strict lowercase UUID syntax and GPT mixed-field byte order.
use core::{ptr, slice};
const ORDER: [usize; 16] = [3, 2, 1, 0, 5, 4, 7, 6, 8, 9, 10, 11, 12, 13, 14, 15];
fn nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    }
}
/// # Safety
/// Input is readable for len bytes; successful output is an exclusive16-byte
/// C array. A rejected original syntax leaves the output unchanged.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_storage_guid_parse(
    text: *const u8,
    len: usize,
    out: *mut u8,
) -> bool {
    if len != 36 {
        return false;
    }
    let text = unsafe { slice::from_raw_parts(text, len) };
    let mut result = [0u8; 16];
    let mut pos = 0;
    for (i, &destination) in ORDER.iter().enumerate() {
        if matches!(i, 4 | 6 | 8 | 10) {
            if text[pos] != b'-' {
                return false;
            }
            pos += 1;
        }
        let Some(hi) = nibble(text[pos]) else {
            return false;
        };
        let Some(lo) = nibble(text[pos + 1]) else {
            return false;
        };
        result[destination] = hi * 16 + lo;
        pos += 2;
    }
    unsafe { ptr::copy_nonoverlapping(result.as_ptr(), out, 16) };
    true
}
/// # Safety
/// Input/output are resident C arrays of16/36 bytes and remain valid for this
/// synchronous representation copy, without overlapping mutable storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_storage_guid_format(input: *const u8, out: *mut u8) {
    let input = unsafe { ptr::read(input.cast::<[u8; 16]>()) };
    let hex = b"0123456789abcdef";
    let mut result = [0u8; 36];
    let mut pos = 0;
    for (i, &source) in ORDER.iter().enumerate() {
        if matches!(i, 4 | 6 | 8 | 10) {
            result[pos] = b'-';
            pos += 1;
        }
        result[pos] = hex[usize::from(input[source] >> 4)];
        result[pos + 1] = hex[usize::from(input[source] & 15)];
        pos += 2;
    }
    unsafe { ptr::copy_nonoverlapping(result.as_ptr(), out, 36) };
}
/// # Safety
/// Both pointers refer to readable16-byte C UUID values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_storage_guid_equal(left: *const u8, right: *const u8) -> bool {
    unsafe { slice::from_raw_parts(left, 16) == slice::from_raw_parts(right, 16) }
}
