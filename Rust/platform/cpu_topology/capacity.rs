//! Static, scoped placement hints from the one immutable boot catalog. Values
//! are policy ordering, never measured capacity/frequency or an ISA admission.
use super::{Domain, count, image, same_core, same_package};
use crate::{
    arch::smp_policy::MAX_CPUS,
    platform::cpu_topology_decode::{Class, Discovery},
};

fn score(d: &Discovery) -> Option<u16> {
    let c = d.class?;
    match (&d.vendor, c.leaf) {
        // Explicit Intel class lookup, not numerical ordering of core IDs.
        (b"GenuineIntel", 0x1a) => match c.kind {
            0x40 => Some(2),
            0x20 => Some(1),
            _ => None,
        },
        // AMD defines this static ordering only within a specific core type.
        // CoreType itself is model-specific; do not invent a P/E mapping.
        (b"AuthenticAMD", 0x8000_0026) if c.signature != 0 => c
            .efficiency
            .filter(|&rank| rank <= u8::MAX as u32)
            .map(|rank| rank as u16),
        _ => None,
    }
}
fn same_group(a: &Discovery, ca: Class, b: &Discovery, cb: Class) -> bool {
    a.vendor == b.vendor
        && ca.leaf == cb.leaf
        && (a.vendor == *b"GenuineIntel"
            || (ca.kind == cb.kind && ca.model == cb.model && ca.signature == cb.signature))
}

pub(super) fn derive(trusted: u64, result: &mut [Domain; MAX_CPUS]) {
    let n = count() as usize;
    // A physical core's SMT peers must agree on the exact class/rank facts.
    // This does not revoke otherwise valid geometry or CPU admission.
    let mut coherent = trusted;
    for i in 0..n {
        if trusted & (1 << i) == 0 {
            continue;
        }
        let (a, da, ga) = image(i);
        for j in 0..n {
            if trusted & (1 << j) == 0 {
                continue;
            }
            let (b, db, gb) = image(j);
            if same_core(a, ga, b, gb) && da.class != db.class {
                coherent &= !(1 << i);
            }
        }
    }
    for (i, domain) in result.iter_mut().enumerate().take(n) {
        if coherent & (1 << i) == 0 {
            continue;
        }
        let (a, da, ga) = image(i);
        let (Some(ca), Some(value)) = (da.class, score(&da)) else {
            continue;
        };
        let (mut scope, mut faster, mut slower) = (0, 0, 0);
        let mut valid = true;
        for j in 0..n {
            if trusted & (1 << j) == 0 {
                continue;
            }
            let (b, db, gb) = image(j);
            if !same_package(a, ga, b, gb) {
                continue;
            }
            let Some(cb) = db.class else {
                valid = false;
                break;
            };
            if !same_group(&da, ca, &db, cb) {
                // Distinct AMD core types/native models have no common scale.
                continue;
            }
            let Some(other) = score(&db) else {
                valid = false;
                break;
            };
            if coherent & (1 << j) == 0 {
                valid = false;
                break;
            }
            scope |= 1 << j;
            if other > value {
                faster |= 1 << j;
            } else if other < value {
                slower |= 1 << j;
            }
        }
        // Homogeneous groups retain the old flat hot path. Missing/incoherent
        // peer data cannot manufacture a low-capacity CPU or a priority tier.
        if valid && (faster | slower) != 0 {
            domain.capacity = value;
            domain.capacity_scope = scope;
            domain.faster = faster;
            domain.slower = slower;
        }
    }
}
