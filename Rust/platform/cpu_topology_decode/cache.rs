//! Deterministic cache leaves share geometry, not an online CPU count.
use super::{Cache, CacheState, MAX_CACHES, width};
use core::arch::x86_64::CpuidResult;
pub(super) fn read(
    leaf: u32,
    query: &mut impl FnMut(u32, u32) -> CpuidResult,
) -> ([Cache; MAX_CACHES], usize, CacheState) {
    let mut caches = [Cache::default(); MAX_CACHES];
    if leaf == 0 {
        return (caches, 0, CacheState::Unavailable);
    }
    for (index, item) in caches.iter_mut().enumerate() {
        let r = query(leaf, index as u32);
        let kind = r.eax & 31;
        if kind == 0 {
            return (caches, index, CacheState::Complete);
        }
        let level = (r.eax >> 5) & 7;
        let line = (r.ebx & 0xfff) + 1;
        let partitions = ((r.ebx >> 12) & 0x3ff) + 1;
        let ways = ((r.ebx >> 22) & 0x3ff) + 1;
        let bytes = u64::from(line)
            .checked_mul(partitions.into())
            .and_then(|v| v.checked_mul(ways.into()))
            .and_then(|v| v.checked_mul(u64::from(r.ecx) + 1));
        let Some(bytes) = bytes.filter(|_| kind <= 3 && level != 0) else {
            return ([Cache::default(); MAX_CACHES], 0, CacheState::Invalid);
        };
        let sharing = ((r.eax >> 14) & 0xfff) + 1;
        *item = Cache {
            kind,
            level,
            line_bytes: line,
            sharing,
            sharing_shift: width(sharing),
            bytes,
            properties: ((r.eax >> 8) & 3) | ((r.edx & 7) << 2),
        };
    }
    // Preserve known individual descriptors with explicit bounded truncation.
    (caches, MAX_CACHES, CacheState::Truncated)
}
