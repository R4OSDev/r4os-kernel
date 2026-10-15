//! Original bytewise DOS-style glob rules and recursive wildcard order.

fn upper(value: u8) -> u8 {
    if value.is_ascii_lowercase() {
        value - (b'a' - b'A')
    } else {
        value
    }
}

// SAFETY: Caller retains the two immutable byte ranges through every recursive
// step. Each raw byte read is bounded by its original range; empty/null ranges
// are never turned into Rust references or read.
unsafe fn match_at(
    pattern: *const u8,
    pattern_len: usize,
    pi: usize,
    name: *const u8,
    name_len: usize,
    ni: usize,
) -> bool {
    if pi >= pattern_len {
        return ni >= name_len;
    }
    let pc = unsafe { pattern.add(pi).read() };
    if pc == b'*' {
        let mut cursor = ni;
        while cursor <= name_len {
            if unsafe { match_at(pattern, pattern_len, pi + 1, name, name_len, cursor) } {
                return true;
            }
            cursor += 1;
        }
        return false;
    }
    if ni >= name_len {
        return false;
    }
    if pc == b'?' || upper(pc) == upper(unsafe { name.add(ni).read() }) {
        return unsafe { match_at(pattern, pattern_len, pi + 1, name, name_len, ni + 1) };
    }
    false
}

/// # Safety
/// Nonempty ranges are readable bytes for the complete synchronous call;
/// the caller retains their storage and admits no concurrent mutation.
/// Empty patterns retain the original '*.*' match-all rule without a read.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fs_glob_matches(
    pattern: *const u8,
    pattern_len: usize,
    name: *const u8,
    name_len: usize,
) -> bool {
    if pattern_len == 0
        || (pattern_len == 1 && unsafe { pattern.read() } == b'*')
        || (pattern_len == 3
            && unsafe { pattern.read() } == b'*'
            && unsafe { pattern.add(1).read() } == b'.'
            && unsafe { pattern.add(2).read() } == b'*')
    {
        return true;
    }
    unsafe { match_at(pattern, pattern_len, 0, name, name_len, 0) }
}
