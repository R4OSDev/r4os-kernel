//! Bounded boot-only domain construction; the scheduler never rescans CPUID
//! images. Contradictory qualified peer counts/vendors lose geometry trust.
#[path = "capacity.rs"]
mod capacity;
use super::{SLOTS, count};
use crate::{
    arch::smp_policy::MAX_CPUS,
    platform::cpu_topology_decode::{Cache, CacheState, Discovery, Geometry},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Domain {
    pub core: u64,
    pub cache: u64,
    pub capacity: u16,
    pub capacity_scope: u64,
    pub faster: u64,
    pub slower: u64,
}
impl Domain {
    pub const EMPTY: Self = Self {
        core: 0,
        cache: 0,
        capacity: 0,
        capacity_scope: 0,
        faster: 0,
        slower: 0,
    };
}
fn base(id: u32, bits: u32) -> u32 {
    id >> bits
}
fn image(index: usize) -> (u32, Discovery, Geometry) {
    let (id, _) = SLOTS[index].descriptor().unwrap();
    let data = SLOTS[index].read().unwrap();
    (id, data, data.geometry.unwrap())
}
fn same_package(a: u32, ga: Geometry, b: u32, gb: Geometry) -> bool {
    ga.package_shift == gb.package_shift && base(a, ga.package_shift) == base(b, gb.package_shift)
}
fn same_core(a: u32, ga: Geometry, b: u32, gb: Geometry) -> bool {
    ga.smt_shift == gb.smt_shift && base(a, ga.smt_shift) == base(b, gb.smt_shift)
}
fn last_cache(d: Discovery, g: Geometry) -> Option<Cache> {
    if d.cache_state != CacheState::Complete {
        return None;
    }
    d.caches[..d.cache_count]
        .iter()
        .copied()
        .filter(|c| matches!(c.kind, 1 | 3) && c.sharing > 1 && c.sharing_shift <= g.package_shift)
        .max_by_key(|c| (c.level, c.kind))
}
pub(super) fn derive(local_valid: u64) -> (u64, [Domain; MAX_CPUS]) {
    let mut trusted = local_valid;
    let n = count() as usize;
    for i in 0..n {
        if local_valid & (1 << i) == 0 {
            continue;
        }
        let (a, da, ga) = image(i);
        let (mut package, mut core) = (0u64, 0u64);
        let mut vendor_conflict = false;
        for j in 0..n {
            if local_valid & (1 << j) == 0 {
                continue;
            }
            let (b, db, gb) = image(j);
            if same_package(a, ga, b, gb) {
                package |= 1 << j;
                vendor_conflict |= da.vendor != db.vendor;
            }
            if same_core(a, ga, b, gb) {
                core |= 1 << j;
            }
        }
        if vendor_conflict || package.count_ones() > ga.logical_bound {
            trusted &= !package;
        }
        if core.count_ones() > ga.threads {
            trusted &= !core;
        }
    }
    let mut result = [Domain::EMPTY; MAX_CPUS];
    for (i, domain) in result.iter_mut().enumerate().take(n) {
        if trusted & (1 << i) == 0 {
            continue;
        }
        let (a, da, ga) = image(i);
        let ca = last_cache(da, ga);
        for j in 0..n {
            if trusted & (1 << j) == 0 {
                continue;
            }
            let (b, db, gb) = image(j);
            if same_core(a, ga, b, gb) {
                domain.core |= 1 << j;
            }
            if same_package(a, ga, b, gb)
                && da.cache_leaf == db.cache_leaf
                && let (Some(ca), Some(cb)) = (ca, last_cache(db, gb))
                && ca == cb
                && base(a, ca.sharing_shift) == base(b, cb.sharing_shift)
            {
                domain.cache |= 1 << j;
            }
        }
    }
    capacity::derive(trusted, &mut result);
    (trusted, result)
}
