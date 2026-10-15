//! Original mapping-pair codec, including signed eight-byte delta handling.
use super::ntfs_format_types::*;
pub fn next(s: &[u8], state: &mut IteratorState) -> Option<Run> {
    if state.failed || state.offset >= s.len() {
        return None;
    }
    let base = state.offset;
    let h = s[base];
    if h == 0 {
        return None;
    }
    let ln = usize::from(h & 15);
    let dn = usize::from((h >> 4) & 15);
    if ln == 0 || ln > 8 || dn > 8 {
        state.failed = true;
        return None;
    }
    let encoded = 1 + ln + dn;
    if encoded > s.len() - base {
        state.failed = true;
        return None;
    }
    let mut length = 0u64;
    for i in 0..ln {
        length |= u64::from(s[base + 1 + i]) << (8 * i)
    }
    if length == 0 {
        state.failed = true;
        return None;
    }
    let mut run = Run {
        length_clusters: length,
        ..Run::default()
    };
    if dn > 0 {
        let mut delta = 0u64;
        for i in 0..dn {
            delta |= u64::from(s[base + 1 + ln + i]) << (8 * i)
        }
        if dn < 8 {
            let bits = 8 * dn;
            if delta & (1u64 << (bits - 1)) != 0 {
                delta |= u64::MAX << bits
            }
        }
        let Some(current) = state.current_lcn.checked_add(delta as i64) else {
            state.failed = true;
            return None;
        };
        state.current_lcn = current;
        if current < 0 {
            state.failed = true;
            return None;
        }
        run.has_lcn = true;
        run.lcn = current as u64
    }
    state.offset += encoded;
    Some(run)
}
pub fn encode(out: &mut [u8], length: u64, delta: Option<i64>) -> Option<usize> {
    if length == 0 {
        return None;
    }
    let mut n = 0;
    let mut remaining = length;
    while remaining != 0 {
        n += 1;
        remaining >>= 8
    }
    if n < 8 && (length >> (8 * n - 1)) & 1 == 1 {
        n += 1
    }
    let mut d = 0;
    if let Some(delta) = delta {
        d = 1;
        while d < 8 {
            let bits = 8 * d;
            let min = -(1i64 << (bits - 1));
            let max = (1i64 << (bits - 1)) - 1;
            if delta >= min && delta <= max {
                break;
            }
            d += 1
        }
    }
    let total = 1 + n + d;
    if out.len() < total {
        return None;
    }
    out[0] = ((d << 4) | n) as u8;
    for i in 0..n {
        out[1 + i] = (length >> (8 * i)) as u8
    }
    if let Some(delta) = delta {
        for i in 0..d {
            out[1 + n + i] = ((delta as u64) >> (8 * i)) as u8
        }
    }
    Some(total)
}
