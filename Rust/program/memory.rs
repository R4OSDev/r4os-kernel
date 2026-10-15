//! Unchanged R4X memory metadata admission and per-class profile defaults.
//! No allocations or global state; values retain binary KB/MB/GB units.
use crate::{fs::vfs_types::Bytes, loader::r4m};
use core::{ptr, slice};

const KB: u64 = 1024;
const MB: u64 = 1024 * KB;
const GB: u64 = 1024 * MB;
const PAGE: u64 = 4096;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Limits {
    pub vm_reserve_limit: u64,
    pub vm_commit_limit: u64,
    pub resident_limit: u64,
    pub stack_reserve: u64,
    pub stack_initial_commit: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Contract {
    pub profile: u8,
    pub limits: Limits,
    pub tag: [u8; 16],
}
fn span(v: &[u8]) -> Bytes {
    Bytes {
        data: v.as_ptr(),
        len: v.len(),
    }
}
unsafe fn bytes<'a>(v: Bytes) -> &'a [u8] {
    if v.len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(v.data, v.len) }
    }
}
fn value<'a>(metadata: &'a [u8], prefix: &[u8]) -> Option<&'a [u8]> {
    let v = unsafe { r4m::r4os_rust_r4m_meta(1, span(metadata), span(prefix), ptr::null_mut()) };
    v.present.then(|| &metadata[v.offset..v.offset + v.len])
}
fn trim(v: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = v.len();
    while start < end && matches!(v[start], b' ' | b'\t') {
        start += 1;
    }
    while end > start && matches!(v[end - 1], b' ' | b'\t') {
        end -= 1;
    }
    &v[start..end]
}
fn profile(v: &[u8]) -> Option<u8> {
    let v = trim(v);
    for (names, id) in [
        (&[b"tiny".as_slice()][..], 1),
        (&[b"normal".as_slice()][..], 2),
        (&[b"desktop".as_slice()][..], 3),
        (&[b"service".as_slice()][..], 4),
        (
            &[b"large-service".as_slice(), b"large_service".as_slice()][..],
            5,
        ),
        (&[b"build-tool".as_slice(), b"build_tool".as_slice()][..], 6),
        (&[b"browser".as_slice()][..], 7),
        (&[b"workstation".as_slice()][..], 8),
    ] {
        if names.iter().any(|name| v.eq_ignore_ascii_case(name)) {
            return Some(id);
        }
    }
    None
}
pub fn page_align(v: u64) -> Option<u64> {
    if v == 0 {
        return None;
    }
    Some(v.checked_add(PAGE - 1)? & !(PAGE - 1))
}
fn size(v: &[u8]) -> Option<u64> {
    let v = trim(v);
    if v.is_empty() {
        return None;
    }
    let mut end = v.len();
    while end > 0 && v[end - 1].is_ascii_alphabetic() {
        end -= 1;
    }
    if end == 0 {
        return None;
    }
    let mut number = 0u64;
    for ch in &v[..end] {
        if *ch == b'_' {
            continue;
        }
        if !ch.is_ascii_digit() {
            return None;
        }
        number = number.checked_mul(10)?.checked_add(u64::from(ch - b'0'))?;
    }
    let suffix = trim(&v[end..]);
    let factor = if suffix.is_empty() || suffix.eq_ignore_ascii_case(b"b") {
        1
    } else if suffix.eq_ignore_ascii_case(b"k") || suffix.eq_ignore_ascii_case(b"kb") {
        KB
    } else if suffix.eq_ignore_ascii_case(b"m") || suffix.eq_ignore_ascii_case(b"mb") {
        MB
    } else if suffix.eq_ignore_ascii_case(b"g") || suffix.eq_ignore_ascii_case(b"gb") {
        GB
    } else {
        return None;
    };
    page_align(number.checked_mul(factor)?)
}
fn limits(profile: u8) -> Limits {
    let (reserve, commit, stack_reserve, stack_commit) = match profile {
        1 => (128 * MB, 32 * MB, 2 * MB, 64 * KB),
        0 | 2 => (1024 * MB, 256 * MB, 4 * MB, 64 * KB),
        3 => (2 * GB, 512 * MB, 4 * MB, 128 * KB),
        4 => (1024 * MB, 256 * MB, 4 * MB, 64 * KB),
        5 | 6 => (4 * GB, 1024 * MB, 8 * MB, 128 * KB),
        7 => (16 * GB, 4 * GB, 32 * MB, 256 * KB),
        8 => (8 * GB, 2 * GB, 32 * MB, 256 * KB),
        _ => unreachable!(),
    };
    Limits {
        vm_reserve_limit: reserve,
        vm_commit_limit: commit,
        resident_limit: commit,
        stack_reserve,
        stack_initial_commit: stack_commit,
    }
}
fn valid(l: Limits) -> Option<()> {
    let reserve = page_align(l.vm_reserve_limit)?;
    let commit = page_align(l.vm_commit_limit)?;
    let resident = page_align(l.resident_limit)?;
    let stack_reserve = page_align(l.stack_reserve)?;
    let stack_commit = page_align(l.stack_initial_commit)?;
    (commit <= reserve
        && resident <= commit
        && stack_reserve > PAGE
        && stack_commit <= stack_reserve - PAGE)
        .then_some(())
}
pub fn resolve(metadata: &[u8], class: u8) -> Option<Contract> {
    let profile = if let Some(v) = value(metadata, b"memory.profile=") {
        profile(v)?
    } else {
        match class {
            0 => 2,
            1 => 3,
            2 => 4,
            _ => unreachable!(),
        }
    };
    let mut out = Contract {
        profile,
        limits: limits(profile),
        tag: [0; 16],
    };
    if let Some(v) = value(metadata, b"memory.reserve=") {
        out.limits.vm_reserve_limit = size(v)?;
    }
    if let Some(v) = value(metadata, b"memory.commit=") {
        out.limits.vm_commit_limit = size(v)?;
    }
    if let Some(v) = value(metadata, b"memory.resident=") {
        out.limits.resident_limit = size(v)?;
    }
    if let Some(v) = value(metadata, b"memory.stack.reserve=") {
        out.limits.stack_reserve = size(v)?;
    }
    if let Some(v) = value(metadata, b"memory.stack.commit=") {
        out.limits.stack_initial_commit = size(v)?;
    }
    if let Some(v) = value(metadata, b"memory.tag=") {
        let n = v.len().min(15);
        out.tag[..n].copy_from_slice(&v[..n]);
    }
    valid(out.limits)?;
    Some(out)
}
/// # Safety
/// Metadata is immutable and resident, output is aligned exclusive C storage.
/// Failed admission leaves output unchanged; class is the original 0..=2 enum.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_memory_contract(
    metadata: Bytes,
    class: u8,
    out: *mut Contract,
) -> bool {
    let Some(contract) = resolve(unsafe { bytes(metadata) }, class) else {
        return false;
    };
    unsafe { out.write(contract) };
    true
}
const _: () = {
    assert!(core::mem::size_of::<Limits>() == 40);
    assert!(core::mem::size_of::<Contract>() == 64);
    assert!(core::mem::offset_of!(Contract, limits) == 8);
    assert!(core::mem::offset_of!(Contract, tag) == 48);
};
