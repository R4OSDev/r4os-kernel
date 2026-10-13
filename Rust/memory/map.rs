//! Immutable boot-map accounting and device extent proofs. These functions
//! neither admit physical frames nor change cache mappings or driver policy.
use crate::boot::info::MemoryEntry;
#[repr(C)]
#[derive(Default)]
pub struct Summary {
    pub entries: u64,
    pub invalid_entries: u64,
    pub usable_bytes: u64,
    pub usable_page_bytes: u64,
    pub largest_usable_base: u64,
    pub largest_usable_len: u64,
    pub overflow: bool,
}
pub fn summarize(entries: &[MemoryEntry]) -> Summary {
    let mut result = Summary {
        entries: entries.len() as u64,
        ..Summary::default()
    };
    for entry in entries {
        if !entry.valid {
            result.invalid_entries += 1;
            continue;
        }
        if entry.kind == 0 {
            for (total, value) in [
                (&mut result.usable_bytes, entry.length),
                (&mut result.usable_page_bytes, entry.usable_len),
            ] {
                if let Some(sum) = total.checked_add(value) {
                    *total = sum;
                } else {
                    *total = u64::MAX;
                    result.overflow = true;
                }
            }
            if entry.usable_len > result.largest_usable_len {
                result.largest_usable_base = entry.usable_base;
                result.largest_usable_len = entry.usable_len;
            }
        }
    }
    result
}
pub fn covers(base: u64, bytes: u64, entries: &[MemoryEntry], exclude_system: bool) -> bool {
    if (exclude_system && entries.is_empty())
        || base == 0
        || bytes == 0
        || (base | bytes) & 4095 != 0
    {
        return false;
    }
    let Some(end) = base.checked_add(bytes) else {
        return false;
    };
    if end > 1 << 52 {
        return false;
    }
    for entry in entries {
        if entry.length == 0 {
            continue;
        }
        let Some(last) = entry.base.checked_add(entry.length) else {
            return false;
        };
        if exclude_system && (!entry.valid || entry.end != last) {
            return false;
        }
        if entry.base >= end || last <= base {
            continue;
        }
        if !entry.valid || entry.end != last || (entry.kind != 1 && entry.kind != 7) {
            return false;
        }
    }
    if exclude_system {
        return true;
    }
    let mut cursor = base;
    while cursor < end {
        let mut next = cursor;
        for entry in entries {
            if entry.valid && entry.base <= cursor && entry.end > cursor {
                next = next.max(end.min(entry.end));
            }
        }
        if next == cursor {
            return false;
        }
        cursor = next;
    }
    true
}
#[unsafe(no_mangle)]
/// # Safety
/// entries retains count immutable C MemoryEntry values; output is exclusive.
pub unsafe extern "C" fn r4os_rust_memory_summary(
    entries: *const MemoryEntry,
    count: usize,
    output: *mut Summary,
) {
    let entries = if count == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(entries, count) }
    };
    unsafe { output.write(summarize(entries)) };
}
#[unsafe(no_mangle)]
/// # Safety
/// entries retains count immutable C MemoryEntry values until return.
pub unsafe extern "C" fn r4os_rust_device_extent(
    base: u64,
    bytes: u64,
    entries: *const MemoryEntry,
    count: usize,
    exclude_system: bool,
) -> bool {
    let entries = if count == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(entries, count) }
    };
    covers(base, bytes, entries, exclude_system)
}
