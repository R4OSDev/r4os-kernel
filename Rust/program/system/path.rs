//! Original bounded C-string/path primitives, with caller-owned buffers.
use core::mem::MaybeUninit;
// Original undefined caller scratch: only a written prefix becomes a byte
// slice. No initialized Rust byte reference covers uninitialized storage.
pub(super) struct Buffer<const N: usize>(MaybeUninit<[u8; N]>);
impl<const N: usize> Buffer<N> {
    pub const fn new() -> Self {
        Self(MaybeUninit::uninit())
    }
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.0.as_mut_ptr().cast()
    }
    pub unsafe fn initialized(&self, len: usize) -> &[u8] {
        assert!(len <= N);
        unsafe { core::slice::from_raw_parts(self.0.as_ptr().cast(), len) }
    }
}
pub(super) unsafe fn copy_z<const N: usize>(
    input: *const u8,
    out: &mut Buffer<N>,
) -> Option<&[u8]> {
    unsafe {
        let mut len = 0;
        let output = out.as_mut_ptr();
        while len < N && input.add(len).read() != 0 {
            output.add(len).write(input.add(len).read());
            len += 1;
        }
        if len == N {
            None
        } else {
            Some(out.initialized(len))
        }
    }
}
pub fn fixed_z(out: &mut [u8], value: &[u8]) {
    out.fill(0);
    if out.is_empty() {
        return;
    }
    let count = value.len().min(out.len() - 1);
    out[..count].copy_from_slice(&value[..count]);
}
pub fn write_z(out: &mut [u8], value: &[u8]) -> bool {
    if value.len() + 1 > out.len() {
        return false;
    }
    out[..value.len()].copy_from_slice(value);
    out[value.len()] = 0;
    true
}
pub fn equal_path(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(&a, &b)| {
            let a = if a == b'/' {
                b'\\'
            } else {
                a.to_ascii_uppercase()
            };
            let b = if b == b'/' {
                b'\\'
            } else {
                b.to_ascii_uppercase()
            };
            a == b
        })
}
pub(super) fn separator(ch: u8) -> bool {
    matches!(ch, b'\\' | b'/')
}
pub(super) fn parent(value: &[u8]) -> &[u8] {
    for i in (2..=value.len()).rev() {
        if separator(value[i - 1]) {
            return &value[..i - 1];
        }
    }
    b"\\"
}
pub(super) fn root(value: &[u8]) -> bool {
    value.is_empty() || value.len() == 1 && separator(value[0])
}
pub(super) fn base(value: &[u8]) -> &[u8] {
    for i in (1..=value.len()).rev() {
        if separator(value[i - 1]) {
            return &value[i..];
        }
    }
    value
}
pub(super) fn join<'a>(parent: &[u8], name: &[u8], out: &'a mut [u8]) -> Option<&'a [u8]> {
    let sep = !parent.is_empty() && !separator(parent[parent.len() - 1]);
    let mut pos = parent.len();
    if pos + usize::from(sep) + name.len() > out.len() {
        return None;
    }
    out[..pos].copy_from_slice(parent);
    if sep {
        out[pos] = b'\\';
        pos += 1;
    }
    out[pos..pos + name.len()].copy_from_slice(name);
    pos += name.len();
    Some(&out[..pos])
}
pub(super) fn drive_z(letter: u8, value: &[u8], out: &mut [u8]) -> bool {
    if value.len() >= 2 && value[1] == b':' {
        return write_z(out, value);
    }
    let sep = value.is_empty() || !separator(value[0]);
    if 2 + usize::from(sep) + value.len() + 1 > out.len() {
        return false;
    }
    out[0] = letter.to_ascii_uppercase();
    out[1] = b':';
    let mut pos = 2;
    if sep {
        out[pos] = b'\\';
        pos += 1;
    }
    out[pos..pos + value.len()].copy_from_slice(value);
    out[pos + value.len()] = 0;
    true
}
pub(super) fn parent_buffer<'a>(value: &[u8], out: &'a mut [u8]) -> Option<&'a [u8]> {
    if value.len() > 1 {
        let mut end = value.len();
        while end > 0 && separator(value[end - 1]) {
            end -= 1;
        }
        for i in (1..=end).rev() {
            if separator(value[i - 1]) {
                let keep = if i <= 1 { 1 } else { i - 1 };
                if keep > out.len() {
                    return None;
                }
                out[..keep].copy_from_slice(&value[..keep]);
                return Some(&out[..keep]);
            }
        }
    }
    if out.is_empty() {
        None
    } else {
        out[0] = b'\\';
        Some(&out[..1])
    }
}
pub(super) fn descendant(candidate: &[u8], directory: &[u8]) -> bool {
    let mut len = directory.len();
    while len > 1 && separator(directory[len - 1]) {
        len -= 1;
    }
    if candidate.len() < len {
        return false;
    }
    for i in 0..len {
        if separator(candidate[i]) && separator(directory[i]) {
            continue;
        }
        if !candidate[i].eq_ignore_ascii_case(&directory[i]) {
            return false;
        }
    }
    candidate.len() == len || separator(candidate[len]) || len == 1 && separator(directory[0])
}
pub(super) fn could_alias(left: &[u8], right: &[u8]) -> bool {
    left.eq_ignore_ascii_case(right) || left.iter().chain(right).any(|&ch| ch >= 0x80)
}
