//! Bounded local CPUID topology, independent of boot masks and the BSP model.
//! Counts describe advertised domains; they never grant schedulable capacity.
mod cache;
use core::arch::x86_64::CpuidResult;
pub(crate) const MAX_CACHES: usize = 16;
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Problem {
    Unavailable = 0,
    Malformed = 1,
    Identity = 2,
    LegacyModel = 3,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Geometry {
    pub leaf: u32,
    pub package: u32,
    pub core: u32,
    pub thread: u32,
    pub smt_shift: u32,
    pub package_shift: u32,
    pub threads: u32,
    pub logical_bound: u32,
}
impl Geometry {
    fn new(leaf: u32, apic: u32, smt: u32, package: u32, threads: u32, logical: u32) -> Self {
        Self {
            leaf,
            package: apic >> package,
            core: (apic & mask(package)) >> smt,
            thread: apic & mask(smt),
            smt_shift: smt,
            package_shift: package,
            threads,
            logical_bound: logical,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Class {
    pub leaf: u32,
    pub kind: u32,
    pub model: u32,
    pub signature: u32,
    pub efficiency: Option<u32>,
    pub heterogeneous: bool,
    pub asymmetric: bool,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Cache {
    pub kind: u32,
    pub level: u32,
    pub line_bytes: u32,
    pub sharing: u32,
    pub sharing_shift: u32,
    pub bytes: u64,
    pub properties: u32,
}
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CacheState {
    Unavailable = 0,
    Complete = 1,
    Invalid = 2,
    Truncated = 3,
}
#[derive(Clone, Copy)]
pub(crate) struct Discovery {
    pub vendor: [u8; 12],
    pub geometry: Result<Geometry, Problem>,
    pub class: Option<Class>,
    pub cache_leaf: u32,
    pub cache_state: CacheState,
    pub cache_count: usize,
    pub caches: [Cache; MAX_CACHES],
}
fn mask(bits: u32) -> u32 {
    ((1u64 << bits) - 1) as u32
}
pub(crate) fn width(count: u32) -> u32 {
    u32::BITS - count.saturating_sub(1).leading_zeros()
}
type Topology = Result<(Geometry, Option<Class>), Problem>;

fn hierarchy(leaf: u32, apic: u32, query: &mut impl FnMut(u32, u32) -> CpuidResult) -> Topology {
    let amd = leaf == 0x8000_0026;
    let mut seen = [0u8; 16];
    let (mut last_shift, mut last_count, mut last_kind) = (0, 0, 0);
    let mut smt = None;
    let mut core = false;
    let mut class = None;
    let (mut heterogeneous, mut asymmetric) = (false, false);
    let mut terminated = false;
    for level in 0..seen.len() {
        let r = query(leaf, level as u32);
        let count = r.ebx & 0xffff;
        let kind = ((r.ecx >> 8) & 0xff) as u8;
        if count == 0 || kind == 0 {
            if level == 0 {
                return Err(Problem::Unavailable);
            }
            terminated = true;
            break;
        }
        if r.edx != apic {
            return Err(Problem::Identity);
        }
        let shift = r.eax & 0x1f;
        // Holes and asymmetric domains are valid; counts need not be powers
        // of two or integer multiples. Shifts still bound addressable IDs.
        if r.ecx & 0xff != level as u32
            || seen[..level].contains(&kind)
            || shift < last_shift
            || count < last_count
            || u64::from(count) > 1u64 << shift
            || (level == 0 && kind != 1)
            || (amd && (kind > 4 || kind <= last_kind))
            || (leaf == 0x0b && kind > 2)
        {
            return Err(Problem::Malformed);
        }
        seen[level] = kind;
        if kind == 1 {
            smt = Some((shift, count));
            if amd {
                class = Some(Class {
                    leaf,
                    kind: r.ebx >> 28,
                    model: (r.ebx >> 24) & 15,
                    signature: query(1, 0).eax,
                    efficiency: (r.eax & (1 << 29) != 0).then_some((r.ebx >> 16) & 0xff),
                    heterogeneous: false,
                    asymmetric: false,
                });
            }
        }
        if !amd && kind == 2 {
            core = true;
        }
        if amd {
            heterogeneous |= r.eax & (1 << 30) != 0;
            asymmetric |= r.eax & (1 << 31) != 0;
        }
        last_shift = shift;
        last_count = count;
        last_kind = kind;
    }
    let Some((smt, threads)) = smt else {
        return Err(Problem::Malformed);
    };
    // Without the enclosing Intel Core or AMD Socket level the package
    // boundary is not established. Do not manufacture it from a CPU count.
    if !terminated || (!amd && !core) || (amd && last_kind != 4) {
        return Err(Problem::Malformed);
    }
    if let Some(c) = &mut class {
        c.heterogeneous = heterogeneous;
        c.asymmetric = asymmetric;
    }
    Ok((
        Geometry::new(leaf, apic, smt, last_shift, threads, last_count),
        class,
    ))
}

fn amd_legacy(
    apic: u32,
    maximum: u32,
    query: &mut impl FnMut(u32, u32) -> CpuidResult,
) -> Topology {
    if maximum < 0x8000_001e || query(0x8000_0001, 0).ecx & (1 << 22) == 0 {
        return Err(Problem::Unavailable);
    }
    let signature = query(1, 0).eax;
    let family = (signature >> 8) & 15;
    let family = family
        + if family == 15 {
            (signature >> 20) & 255
        } else {
            0
        };
    // Older AMD CoreId describes CMT/compute units on some families; that is
    // not evidence for a physical SMT core. Keep the functional flat fallback.
    if family < 0x17 {
        return Err(Problem::LegacyModel);
    }
    let r = query(0x8000_001e, 0);
    if r.eax != apic {
        return Err(Problem::Identity);
    }
    let threads = ((r.ebx >> 8) & 0xff) + 1;
    let size = query(0x8000_0008, 0).ecx;
    let logical = (size & 0xff) + 1;
    let smt = width(threads);
    let package = match (size >> 12) & 15 {
        0 => width(logical),
        value => value,
    };
    if threads > logical || package < smt || u64::from(logical) > 1u64 << package {
        return Err(Problem::Malformed);
    }
    Ok((
        Geometry::new(0x8000_001e, apic, smt, package, threads, logical),
        None,
    ))
}

pub(crate) fn discover(apic: u32, mut query: impl FnMut(u32, u32) -> CpuidResult) -> Discovery {
    let base = query(0, 0);
    let mut vendor = [0; 12];
    vendor[..4].copy_from_slice(&base.ebx.to_le_bytes());
    vendor[4..8].copy_from_slice(&base.edx.to_le_bytes());
    vendor[8..].copy_from_slice(&base.ecx.to_le_bytes());
    let intel = vendor == *b"GenuineIntel";
    let amd = vendor == *b"AuthenticAMD";
    let extended = query(0x8000_0000, 0).eax;
    let mut topology = Err(Problem::Unavailable);
    let candidates = if amd {
        [0x8000_0026, 0x0b]
    } else {
        [0x1f, 0x0b]
    };
    for leaf in candidates {
        if (leaf < 0x8000_0000 && base.eax < leaf) || (leaf >= 0x8000_0000 && extended < leaf) {
            continue;
        }
        topology = hierarchy(leaf, apic, &mut query);
        if topology != Err(Problem::Unavailable) {
            break;
        }
    }
    if topology == Err(Problem::Unavailable) && amd {
        topology = amd_legacy(apic, extended, &mut query);
    }
    let (geometry, mut class) = match topology {
        Ok((g, c)) => (Ok(g), c),
        Err(e) => (Err(e), None),
    };
    // 1A can also exist on homogeneous Intel parts. Its nonzero native ID
    // enumerates the leaf; the independent hybrid feature remains a fact.
    if intel && base.eax >= 0x1a {
        let raw = query(0x1a, 0).eax;
        if raw != 0 {
            class = Some(Class {
                leaf: 0x1a,
                kind: raw >> 24,
                model: raw & 0x00ff_ffff,
                signature: query(1, 0).eax,
                efficiency: None,
                heterogeneous: query(7, 0).edx & (1 << 15) != 0,
                asymmetric: false,
            });
        }
    }
    let cache_leaf = if amd && extended >= 0x8000_001d && query(0x8000_0001, 0).ecx & (1 << 22) != 0
    {
        0x8000_001d
    } else if intel && base.eax >= 4 {
        4
    } else {
        0
    };
    let (caches, cache_count, cache_state) = cache::read(cache_leaf, &mut query);
    Discovery {
        vendor,
        geometry,
        class,
        cache_leaf,
        cache_state,
        cache_count,
        caches,
    }
}
