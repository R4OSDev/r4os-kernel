//! Bounded program text/path/wire helpers and unchanged role/class labels.
//! These algorithms allocate no storage and retain their original byte rules.
use crate::{boot::foundation, fs::vfs_types::Bytes};
use core::{ptr, slice};
pub(super) fn span(value: &[u8]) -> Bytes {
    Bytes {
        data: value.as_ptr(),
        len: value.len(),
    }
}
pub(super) unsafe fn bytes<'a>(value: Bytes) -> &'a [u8] {
    if value.len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(value.data, value.len) }
    }
}
fn separator(value: u8) -> bool {
    matches!(value, b'/' | b'\\')
}
pub(super) fn basename(value: &[u8]) -> &[u8] {
    let mut i = value.len();
    while i > 0 {
        if separator(value[i - 1]) {
            return &value[i..];
        }
        i -= 1;
    }
    value
}
fn fixed(value: &[u8]) -> &[u8] {
    let len = value.iter().position(|&v| v == 0).unwrap_or(value.len());
    &value[..len]
}
pub(super) fn class(policy: u32, flags: u32) -> u8 {
    match policy {
        1 => 0,
        2 => 1,
        0 => {
            if flags & 1 != 0 {
                0
            } else if flags & 2 != 0 {
                1
            } else if flags & 4 != 0 {
                2
            } else {
                1
            }
        }
        _ => unreachable!("Private launch policy"),
    }
}
/// # Safety
/// For arithmetic operations the output is exclusive resident storage; absence
/// leaves it unchanged. Alignment zero retains the original checked failure.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_util_number(
    op: u8,
    a: usize,
    b: usize,
    out: *mut usize,
) -> bool {
    unsafe {
        let value = match op {
            0 => a.checked_add(b),
            1 => {
                let add = b.checked_sub(1).expect("Program alignment underflow");
                a.checked_add(add).map(|sum| sum & !add)
            }
            2 => return a.is_power_of_two(),
            _ => unreachable!(),
        };
        if let Some(value) = value {
            out.write(value);
            true
        } else {
            false
        }
    }
}
/// # Safety
/// Both borrowed byte spans remain resident for this bounded comparison.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_util_compare(op: u8, a: Bytes, b: Bytes) -> bool {
    unsafe {
        let a = bytes(a);
        let b = bytes(b);
        match op {
            0 => a == b,
            1 => a.starts_with(b),
            2 => a.len() >= b.len() && a[..b.len()].eq_ignore_ascii_case(b),
            3 => {
                let a = fixed(a);
                b.is_empty()
                    || (a.len() >= b.len() && a.windows(b.len()).any(|v| v.eq_ignore_ascii_case(b)))
            }
            4 => a.is_empty() || (a.len() == 1 && separator(a[0])),
            _ => unreachable!(),
        }
    }
}
/// # Safety
/// Returned span borrows the original retained input or a permanent root label.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_util_slice(op: u8, input: Bytes) -> Bytes {
    unsafe {
        let data = bytes(input);
        let (start, end) = match op {
            0 => {
                let mut start = 0;
                let mut end = data.len();
                while start < end && matches!(data[start], b' ' | b'\t') {
                    start += 1;
                }
                while end > start && matches!(data[end - 1], b' ' | b'\t') {
                    end -= 1;
                }
                (start, end)
            }
            1 => (0, fixed(data).len()),
            2 => {
                if data.len() >= 2 && data[1] == b':' {
                    (2, data.len())
                } else {
                    (0, data.len())
                }
            }
            3 => {
                let mut i = data.len();
                while i > 1 && !separator(data[i - 1]) {
                    i -= 1;
                }
                if i <= 1 {
                    return span(b"\\");
                }
                (0, i - 1)
            }
            4 => (data.len() - basename(data).len(), data.len()),
            _ => unreachable!(),
        };
        Bytes {
            data: input.data.wrapping_add(start),
            len: end - start,
        }
    }
}
/// # Safety
/// Borrowed bytes and exclusive optional index output retain their lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_util_index(
    input: Bytes,
    value: u8,
    out: *mut usize,
) -> bool {
    unsafe {
        if let Some(index) = bytes(input).iter().position(|&v| v == value) {
            out.write(index);
            true
        } else {
            false
        }
    }
}
unsafe fn copy_path(input: &[u8], out: *mut u8, capacity: usize) -> bool {
    if input.len() + 1 > capacity {
        return false;
    }
    unsafe {
        ptr::copy_nonoverlapping(input.as_ptr(), out, input.len());
        out.add(input.len()).write(0);
    }
    true
}
/// # Safety
/// Inputs remain resident; destination is exclusive, sufficiently aligned bytes
/// with the original nonoverlap contract. Failure retains existing prefixes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_util_path(
    op: u8,
    a: Bytes,
    b: Bytes,
    out: *mut u8,
    capacity: usize,
) -> bool {
    unsafe {
        let a = bytes(a);
        let b = bytes(b);
        match op {
            0 => copy_path(a, out, capacity),
            1 => {
                let sep = !a.is_empty() && !separator(a[a.len() - 1]);
                let n = a.len() + usize::from(sep) + b.len();
                if n + 1 > capacity {
                    return false;
                }
                ptr::copy_nonoverlapping(a.as_ptr(), out, a.len());
                let mut pos = a.len();
                if sep {
                    out.add(pos).write(b'\\');
                    pos += 1;
                }
                ptr::copy_nonoverlapping(b.as_ptr(), out.add(pos), b.len());
                out.add(n).write(0);
                true
            }
            2 => {
                if a.len() <= 3 && a.len() >= 2 && a[1] == b':' {
                    return copy_path(a, out, capacity);
                }
                let mut end = a.len();
                while end > 0 && separator(a[end - 1]) {
                    end -= 1;
                }
                let mut i = end;
                while i > 0 {
                    if separator(a[i - 1]) {
                        let keep = if i == 3 && a.len() >= 2 && a[1] == b':' {
                            3
                        } else {
                            i - 1
                        };
                        return copy_path(&a[..keep], out, capacity);
                    }
                    i -= 1;
                }
                copy_path(a, out, capacity)
            }
            _ => unreachable!(),
        }
    }
}
/// # Safety
/// At least width original readable bytes; width is exactly2,4,8.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_util_read(input: Bytes, width: u8) -> u64 {
    unsafe {
        assert!(matches!(width, 2 | 4 | 8));
        let data = &bytes(input)[..usize::from(width)];
        data.iter()
            .enumerate()
            .fold(0, |value, (i, byte)| value | (u64::from(*byte) << (i * 8)))
    }
}
/// # Safety
/// Exclusive original writable span; width is exactly2,4,8. A short destination
/// retains original bytewise write-before-bounds-failure behavior.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_util_write(
    out: *mut u8,
    len: usize,
    width: u8,
    value: u64,
) {
    unsafe {
        assert!(matches!(width, 2 | 4 | 8));
        let data = slice::from_raw_parts_mut(out, len);
        let mut i = 0;
        while i < usize::from(width) {
            data[i] = (value >> (i * 8)) as u8;
            i += 1;
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_util_enum(op: u8, value: u32, flags: u32) -> u32 {
    match op {
        0 => u32::from(class(value, flags)),
        1 => (flags & 7).count_ones(),
        2 | 3 => {
            if value <= 2 {
                value
            } else {
                u32::MAX
            }
        }
        _ => unreachable!(),
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_util_label(group: u8, value: u8) -> Bytes {
    let text: &[u8] = match group {
        0 => match value {
            0 => b"foreground",
            1 => b"shell",
            2 => b"background",
            _ => unreachable!(),
        },
        1 => match value {
            0 => b"console",
            1 => b"gui",
            2 => b"service",
            _ => unreachable!(),
        },
        2 => match value {
            0 => b"unknown",
            1 => b"tiny",
            2 => b"normal",
            3 => b"desktop",
            4 => b"service",
            5 => b"large-service",
            6 => b"build-tool",
            7 => b"browser",
            8 => b"workstation",
            _ => unreachable!(),
        },
        3 => match value {
            0 => b"running",
            1 => b"closing",
            2 => b"done",
            _ => unreachable!(),
        },
        4 => match value {
            0 => b"none",
            1 => b"terminal-window",
            2 => b"terminal-mode",
            _ => unreachable!(),
        },
        _ => unreachable!(),
    };
    span(text)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_util_signed(value: i32) {
    if value < 0 {
        foundation::puts(b"-");
        foundation::dec(
            value
                .checked_neg()
                .expect("Program signed decimal overflow") as u64,
        );
    } else {
        foundation::dec(value as u64);
    }
}
