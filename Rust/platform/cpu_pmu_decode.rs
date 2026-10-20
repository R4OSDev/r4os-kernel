//! Finite PMU/model/event policy. No hardware access or guessed vendor events.
use super::cpu_capabilities_decode::Raw;
pub(crate) const INTEL: u32 = 1;
pub(crate) const MILAN_B1: u32 = 2;
pub(crate) const CYCLES: u32 = 0;
pub(crate) const INSTRUCTIONS: u32 = 1;
pub(crate) const BRANCHES: u32 = 2;
pub(crate) const BRANCH_MISSES: u32 = 3;
pub(crate) const CACHE_REFERENCES: u32 = 4;
pub(crate) const CACHE_MISSES: u32 = 5;
pub(crate) const NONE: u32 = 0;
pub(crate) const NO_PMU: u32 = 1;
pub(crate) const VIRTUAL: u32 = 2;
pub(crate) const LAYOUT: u32 = 3;
pub(crate) const MODEL: u32 = 4;
pub(crate) const MALFORMED: u32 = 5;
pub(crate) const NO_MSR: u32 = 6;
pub(crate) const ENABLE: u64 = 1 << 22;
pub(crate) const INTERRUPT: u64 = 1 << 20;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Discovery {
    pub profile: u32,
    pub reason: u32,
    pub signature: u32,
    pub version: u32,
    pub counters: u32,
    pub width: u32,
    pub fixed_counters: u32,
    pub fixed_width: u32,
    pub architectural_events: u32,
    pub events: u32,
    pub virtual_cpu: bool,
}
pub(crate) fn discover(raw: &Raw) -> Discovery {
    let basic = raw.get(1, 0);
    let mut out = Discovery {
        signature: basic.map_or(0, |r| r.eax),
        virtual_cpu: basic.is_some_and(|r| r.ecx & (1 << 31) != 0),
        reason: NO_PMU,
        ..Default::default()
    };
    match &raw.vendor() {
        b"GenuineIntel" => {
            if let Some(r) = raw.get(0x0a, 0) {
                out.version = r.eax & 255;
                if out.version != 0 {
                    out.counters = (r.eax >> 8) & 255;
                    out.width = (r.eax >> 16) & 255;
                    let length = (r.eax >> 24) & 255;
                    if out.version >= 2 {
                        out.fixed_counters = r.edx & 31;
                        out.fixed_width = (r.edx >> 5) & 255;
                    }
                    for (id, bit) in [0, 1, 5, 6, 3, 4].into_iter().enumerate() {
                        if bit < length && r.ebx & (1 << bit) == 0 {
                            out.architectural_events |= 1 << bit;
                            out.events |= 1 << id;
                        }
                    }
                    out.reason = if out.counters == 0
                        || out.counters > 32
                        || !(32..=64).contains(&out.width)
                        || length > 32
                    {
                        MALFORMED
                    } else if !(2..=5).contains(&out.version)
                        || raw.signature().is_none_or(|s| s.0 != 6)
                        // Initial scopes also qualify PEBS/debug side effects
                        // before a write, requiring documented DS/PDCM reads.
                        || basic.is_none_or(|v| v.ecx & (1 << 15) == 0 || v.edx & (1 << 21) == 0)
                    {
                        LAYOUT
                    } else {
                        out.profile = INTEL;
                        NONE
                    };
                }
            }
        }
        b"AuthenticAMD" => {
            let extended_core = raw
                .get(0x8000_0001, 0)
                .is_some_and(|r| r.ecx & (1 << 23) != 0);
            if let Some(r) = raw.get(0x8000_0022, 0)
                && r.eax & 1 != 0
            {
                out.version = 2;
                out.counters = r.ebx & 15;
                // This leaf enumerates count, not a model-qualified width.
                // The initial implementation grants no V2 event profile.
                out.width = 0;
                // V2 exposure is a fact, not an event-catalogue/profile grant.
                out.reason = if out.counters == 0 { MALFORMED } else { MODEL };
            } else if extended_core {
                out.version = 1;
                out.reason = MODEL;
                if raw.signature() == Some((0x19, 1, 1)) {
                    // Exact PPR55898 model: six48-bit private thread counters.
                    out.profile = MILAN_B1;
                    out.counters = 6;
                    out.width = 48;
                    out.events = 63;
                    out.reason = NONE;
                }
            }
        }
        _ => {}
    }
    if basic.is_none_or(|r| r.edx & (1 << 5) == 0) {
        out.reason = NO_MSR;
    }
    if out.virtual_cpu {
        out.reason = VIRTUAL;
    }
    out
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Plan {
    pub profile: u32,
    pub version: u32,
    pub width: u32,
    pub counter_index: u32,
    pub control: u32,
    pub counter: u32,
    pub select: u64,
}
pub(crate) fn plan(d: &Discovery, event: u32) -> Option<Plan> {
    if d.reason != NONE || d.virtual_cpu || event >= 6 || d.events & (1 << event) == 0 {
        return None;
    }
    let (index, control, counter, select) = match d.profile {
        INTEL => (
            0,
            0x186,
            0xc1,
            [0x003c, 0x00c0, 0x00c4, 0x00c5, 0x4f2e, 0x412e][event as usize],
        ),
        MILAN_B1 => {
            let retired = matches!(event, INSTRUCTIONS | BRANCHES | BRANCH_MISSES);
            let index = if retired { 2 } else { 0 };
            // Revision Guide56683 erratum1292 overrides PPR's reserved bit43:
            // affected non-FP retire events use exactly PERF_CTL2,43=1,INT=0.
            let workaround = if retired { 1u64 << 43 } else { 0 };
            (
                index,
                0xc001_0200 + 2 * index,
                0xc001_0201 + 2 * index,
                [0x0076, 0x00c0, 0x00c2, 0x00c3, 0xf860, 0x0864][event as usize] | workaround,
            )
        }
        _ => return None,
    };
    Some(Plan {
        profile: d.profile,
        version: d.version,
        width: d.width,
        counter_index: index,
        control,
        counter,
        select: select | (3 << 16),
    })
}
pub(crate) fn mask(width: u32) -> Option<u64> {
    if !(1..=64).contains(&width) {
        None
    } else {
        Some(u64::MAX >> (64 - width))
    }
}
pub(crate) fn legacy_intel_write(value: u64, width: u32) -> Option<u64> {
    let mask = mask(width)?;
    let extended = (value as u32 as i32 as i64 as u64) & mask;
    (value & !mask == 0 && extended == value).then_some(value & u64::from(u32::MAX))
}
pub(crate) fn delta(before: u64, after: u64, width: u32) -> Option<u64> {
    let mask = mask(width)?;
    (before & !mask == 0 && after & !mask == 0).then_some(after.wrapping_sub(before) & mask)
}
#[cfg(test)]
mod tests {
    use super::*;
    use core::arch::x86_64::CpuidResult as C;
    impl Raw {
        fn pmu_test_put(&mut self, leaf: u32, sub: u32, value: C) {
            let i = super::super::cpu_capabilities_decode::LEAVES
                .iter()
                .position(|v| *v == (leaf, sub))
                .unwrap();
            self.rows[i] = value;
            self.known |= 1u64 << i;
        }
    }
    fn raw(amd: bool) -> Raw {
        let name = if amd {
            b"AuthenticAMD"
        } else {
            b"GenuineIntel"
        };
        let mut raw = Raw::EMPTY;
        raw.pmu_test_put(
            0,
            0,
            C {
                eax: 0x16,
                ebx: u32::from_le_bytes(name[..4].try_into().unwrap()),
                edx: u32::from_le_bytes(name[4..8].try_into().unwrap()),
                ecx: u32::from_le_bytes(name[8..].try_into().unwrap()),
            },
        );
        raw.pmu_test_put(
            1,
            0,
            C {
                eax: if amd { 0xa00f11 } else { 0x506e3 },
                ebx: 0,
                ecx: 1 << 15,
                edx: (1 << 5) | (1 << 21),
            },
        );
        raw.pmu_test_put(
            0x0a,
            0,
            C {
                eax: 4 | (4 << 8) | (48 << 16) | (7 << 24),
                ebx: 0,
                ecx: 0,
                edx: 3 | (48 << 5),
            },
        );
        raw.pmu_test_put(
            0x8000_0001,
            0,
            C {
                eax: 0,
                ebx: 0,
                ecx: 1 << 23,
                edx: 0,
            },
        );
        raw
    }
    #[test]
    fn intel_architectural_events_follow_availability_bits_and_length() {
        let mut r = raw(false);
        let d = discover(&r);
        assert_eq!((d.profile, d.events, d.width), (INTEL, 63, 48));
        assert_eq!((d.fixed_counters, d.fixed_width), (3, 48));
        let mut a = r.get(0x0a, 0).unwrap();
        a.ebx = 1 << 5;
        r.pmu_test_put(0x0a, 0, a);
        assert!(plan(&discover(&r), BRANCHES).is_none());
        a.eax = (a.eax & 0x00ff_ffff) | (2 << 24);
        r.pmu_test_put(0x0a, 0, a);
        assert_eq!(discover(&r).events, 3);
    }
    #[test]
    fn absent_malformed_future_and_unqualified_interfaces_never_plan() {
        let mut r = raw(false);
        let mut a = r.get(0x0a, 0).unwrap();
        for (version, count, width, length) in [
            (0, 4, 48, 7),
            (1, 4, 48, 7),
            (6, 4, 48, 7),
            (4, 0, 48, 7),
            (4, 33, 48, 7),
            (4, 4, 31, 7),
            (4, 4, 65, 7),
            (4, 4, 48, 33),
        ] {
            a.eax = version | (count << 8) | (width << 16) | (length << 24);
            r.pmu_test_put(0x0a, 0, a);
            assert!(plan(&discover(&r), CYCLES).is_none());
        }
        assert!(plan(&discover(&Raw::EMPTY), 0).is_none());
    }
    #[test]
    fn virtual_is_never_a_native_event_grant() {
        for amd in [false, true] {
            let mut r = raw(amd);
            let mut a = r.get(1, 0).unwrap();
            a.ecx |= 1 << 31;
            r.pmu_test_put(1, 0, a);
            let d = discover(&r);
            assert_eq!(d.reason, VIRTUAL);
            for e in 0..6 {
                assert!(plan(&d, e).is_none());
            }
        }
    }
    #[test]
    fn milan_retire_erratum_uses_counter2_bit43_without_interrupts() {
        let d = discover(&raw(true));
        assert_eq!((d.profile, d.counters, d.width), (MILAN_B1, 6, 48));
        for e in 0..6 {
            let p = plan(&d, e).unwrap();
            let retired = matches!(e, INSTRUCTIONS | BRANCHES | BRANCH_MISSES);
            assert_eq!(p.counter_index, if retired { 2 } else { 0 });
            assert_eq!(p.select & (1 << 43) != 0, retired);
            assert_eq!(p.select & (INTERRUPT | ENABLE), 0);
            assert_eq!(p.control, 0xc001_0200 + 2 * p.counter_index);
        }
    }
    #[test]
    fn amd_v2_and_unknown_models_are_exposure_only() {
        let mut r = raw(true);
        let mut a = r.get(1, 0).unwrap();
        a.eax = 0xa00f12;
        r.pmu_test_put(1, 0, a);
        assert_eq!(discover(&r).reason, MODEL);
        r.pmu_test_put(
            0x8000_0022,
            0,
            C {
                eax: 1,
                ebx: 6,
                ecx: 0,
                edx: 0,
            },
        );
        let d = discover(&r);
        assert_eq!((d.version, d.counters, d.width, d.profile), (2, 6, 0, 0));
        assert!(plan(&d, 0).is_none());
    }
    #[test]
    fn missing_msr_ds_pdcm_or_core_feature_never_plans() {
        for bit in [5, 21] {
            let mut r = raw(false);
            let mut a = r.get(1, 0).unwrap();
            a.edx &= !(1 << bit);
            r.pmu_test_put(1, 0, a);
            assert!(plan(&discover(&r), 0).is_none());
        }
        let mut r = raw(false);
        let mut a = r.get(1, 0).unwrap();
        a.ecx &= !(1 << 15);
        r.pmu_test_put(1, 0, a);
        assert_eq!(discover(&r).reason, LAYOUT);
        let mut r = raw(true);
        r.pmu_test_put(
            0x8000_0001,
            0,
            C {
                eax: 0,
                ebx: 0,
                ecx: 0,
                edx: 0,
            },
        );
        assert!(plan(&discover(&r), 0).is_none());
    }
    #[test]
    fn classic_intel_restore_never_truncates_an_arbitrary_saved_counter() {
        assert_eq!(legacy_intel_write(0x1234, 48), Some(0x1234));
        assert_eq!(legacy_intel_write(0xffff_8000_0001, 48), Some(0x8000_0001));
        assert_eq!(legacy_intel_write(0x1234_5678_9abc, 48), None);
        assert_eq!(legacy_intel_write(1 << 48, 48), None);
    }
    #[test]
    fn modulo_arithmetic_is_explicit_and_never_proves_absent_multiple_wraps() {
        assert_eq!(delta(250, 5, 8), Some(11));
        assert_eq!(delta(u64::MAX - 1, 2, 64), Some(4));
        assert_eq!(delta(256, 0, 8), None);
        assert_eq!(mask(0), None);
        assert_eq!(mask(65), None);
    }
}
