//! Bounded feature meanings, independent of hardware access and lifecycle.
//! Source: Intel SDM093 Vol1 ch21; AMD feature namespaces stay vendor-scoped.
use super::cpu_capabilities::{Snapshot, bit, register};
use crate::contract as c;
#[derive(Clone, Copy)]
enum Vendor {
    Any,
    Intel,
    Amd,
    Known,
}
#[derive(Clone, Copy)]
enum Control {
    None,
    Unknown,
    Cr0,
    Cr4(u8),
    Sse,
    Xsave,
    Xcr0(u64),
    Efer(u8),
    Mtrr,
}
#[derive(Clone, Copy)]
struct Feature {
    id: u32,
    name: &'static [u8],
    leaf: u32,
    sub: u32,
    reg: u32,
    bit: u32,
    vendor: Vendor,
    control: Control,
}
use Control::*;
use Vendor::*;
const fn f(
    id: u32,
    name: &'static [u8],
    leaf: u32,
    reg: u32,
    bit: u32,
    vendor: Vendor,
    control: Control,
) -> Feature {
    Feature {
        id,
        name,
        leaf,
        sub: 0,
        reg,
        bit,
        vendor,
        control,
    }
}
const FEATURES: &[Feature] = &[
    f(c::cpu_feature_id_x87, b"x87", 1, 3, 0, Any, Cr0),
    f(c::cpu_feature_id_msr, b"msr", 1, 3, 5, Any, None),
    f(c::cpu_feature_id_fxsr, b"fxsr", 1, 3, 24, Any, Cr4(9)),
    f(c::cpu_feature_id_sse, b"sse", 1, 3, 25, Any, Sse),
    f(c::cpu_feature_id_sse2, b"sse2", 1, 3, 26, Any, Sse),
    f(c::cpu_feature_id_sse3, b"sse3", 1, 2, 0, Any, Sse),
    f(c::cpu_feature_id_ssse3, b"ssse3", 1, 2, 9, Any, Sse),
    f(c::cpu_feature_id_sse41, b"sse4.1", 1, 2, 19, Any, Sse),
    f(c::cpu_feature_id_sse42, b"sse4.2", 1, 2, 20, Any, Sse),
    f(c::cpu_feature_id_xsave, b"xsave", 1, 2, 26, Any, Xsave),
    f(c::cpu_feature_id_avx, b"avx", 1, 2, 28, Any, Xcr0(7)),
    f(c::cpu_feature_id_avx2, b"avx2", 7, 1, 5, Any, Xcr0(7)),
    f(
        c::cpu_feature_id_avx512f,
        b"avx512f",
        7,
        1,
        16,
        Any,
        Xcr0(0xe7),
    ),
    f(
        c::cpu_feature_id_amx_tile,
        b"amx-tile",
        7,
        3,
        24,
        Any,
        Xcr0((1 << 17) | (1 << 18)),
    ),
    f(c::cpu_feature_id_aes, b"aes", 1, 2, 25, Any, Sse),
    f(c::cpu_feature_id_pclmulqdq, b"pclmulqdq", 1, 2, 1, Any, Sse),
    f(c::cpu_feature_id_vaes, b"vaes", 7, 2, 9, Any, Xcr0(7)),
    f(
        c::cpu_feature_id_vpclmulqdq,
        b"vpclmulqdq",
        7,
        2,
        10,
        Any,
        Xcr0(7),
    ),
    f(c::cpu_feature_id_rdrand, b"rdrand", 1, 2, 30, Any, None),
    f(c::cpu_feature_id_rdseed, b"rdseed", 7, 1, 18, Any, None),
    f(c::cpu_feature_id_tsc, b"tsc", 1, 3, 4, Any, None),
    f(
        c::cpu_feature_id_invariant_tsc,
        b"invariant-tsc",
        0x8000_0007,
        3,
        8,
        Known,
        None,
    ),
    f(c::cpu_feature_id_pat, b"pat", 1, 3, 16, Any, None),
    f(c::cpu_feature_id_mtrr, b"mtrr", 1, 3, 12, Any, Mtrr),
    f(
        c::cpu_feature_id_nx,
        b"nx",
        0x8000_0001,
        3,
        20,
        Any,
        Efer(11),
    ),
    f(
        c::cpu_feature_id_page1gb,
        b"page1gb",
        0x8000_0001,
        3,
        26,
        Any,
        Unknown,
    ),
    f(c::cpu_feature_id_pcid, b"pcid", 1, 2, 17, Any, Cr4(17)),
    f(c::cpu_feature_id_invpcid, b"invpcid", 7, 1, 10, Any, None),
    f(
        c::cpu_feature_id_fsgsbase,
        b"fsgsbase",
        7,
        1,
        0,
        Any,
        Cr4(16),
    ),
    f(c::cpu_feature_id_smep, b"smep", 7, 1, 7, Any, Cr4(20)),
    f(c::cpu_feature_id_smap, b"smap", 7, 1, 20, Any, Cr4(21)),
    f(c::cpu_feature_id_la57, b"la57", 7, 2, 16, Any, Cr4(12)),
    f(c::cpu_feature_id_clflush, b"clflush", 1, 3, 19, Any, None),
    f(
        c::cpu_feature_id_clflushopt,
        b"clflushopt",
        7,
        1,
        23,
        Any,
        None,
    ),
    f(c::cpu_feature_id_clwb, b"clwb", 7, 1, 24, Any, None),
    f(
        c::cpu_feature_id_monitor_mwait,
        b"monitor-mwait",
        1,
        2,
        3,
        Any,
        Unknown,
    ),
    f(c::cpu_feature_id_arat, b"arat", 6, 0, 2, Intel, None),
    f(
        c::cpu_feature_id_aperf_mperf,
        b"aperf-mperf",
        6,
        2,
        0,
        Known,
        Unknown,
    ),
    f(
        c::cpu_feature_id_intel_dts,
        b"intel-dts",
        6,
        0,
        0,
        Intel,
        Unknown,
    ),
    f(
        c::cpu_feature_id_intel_turbo,
        b"intel-turbo",
        6,
        0,
        1,
        Intel,
        Unknown,
    ),
    f(
        c::cpu_feature_id_intel_hwp,
        b"intel-hwp",
        6,
        0,
        7,
        Intel,
        Unknown,
    ),
    f(
        c::cpu_feature_id_intel_hwp_epp,
        b"intel-hwp-epp",
        6,
        0,
        10,
        Intel,
        Unknown,
    ),
    f(
        c::cpu_feature_id_intel_hwp_package,
        b"intel-hwp-package",
        6,
        0,
        11,
        Intel,
        Unknown,
    ),
    f(
        c::cpu_feature_id_intel_hw_feedback,
        b"intel-hw-feedback",
        6,
        0,
        19,
        Intel,
        Unknown,
    ),
    f(
        c::cpu_feature_id_intel_pmu,
        b"intel-pmu",
        0x0a,
        0,
        32,
        Intel,
        Unknown,
    ),
    f(
        c::cpu_feature_id_amd_cpb,
        b"amd-cpb",
        0x8000_0007,
        3,
        9,
        Amd,
        Unknown,
    ),
    f(
        c::cpu_feature_id_amd_cppc,
        b"amd-cppc",
        0x8000_0008,
        1,
        27,
        Amd,
        Unknown,
    ),
    f(
        c::cpu_feature_id_amd_effective_frequency_ro,
        b"amd-effective-freq-ro",
        0x8000_0007,
        3,
        10,
        Amd,
        Unknown,
    ),
    f(
        c::cpu_feature_id_amd_pmu,
        b"amd-pmu-v2",
        0x8000_0022,
        0,
        0,
        Amd,
        Unknown,
    ),
    f(c::cpu_feature_id_x2apic, b"x2apic", 1, 2, 21, Any, Unknown),
    f(
        c::cpu_feature_id_tsc_deadline,
        b"tsc-deadline",
        1,
        2,
        24,
        Any,
        Unknown,
    ),
    f(c::cpu_feature_id_cx8, b"cx8", 1, 3, 8, Any, None),
    f(c::cpu_feature_id_cmov, b"cmov", 1, 3, 15, Any, None),
    f(c::cpu_feature_id_mmx, b"mmx", 1, 3, 23, Any, Cr0),
    f(
        c::cpu_feature_id_long_mode,
        b"long-mode",
        0x8000_0001,
        3,
        29,
        Any,
        Efer(10),
    ),
];
pub(crate) fn count() -> u32 {
    FEATURES.len() as u32
}
fn applicable(v: Vendor, raw: &super::cpu_capabilities_decode::Raw) -> bool {
    let name = raw.vendor();
    match v {
        Any => true,
        Intel => name == *b"GenuineIntel",
        Amd => name == *b"AuthenticAMD",
        Known => name == *b"GenuineIntel" || name == *b"AuthenticAMD",
    }
}
fn activation(s: &Snapshot, control: Control) -> Option<(u64, bool)> {
    let os = &s.os;
    let cr0 = os.flags & c::cpu_capabilities_os_cr0 != 0;
    let cr4 = os.flags & c::cpu_capabilities_os_cr4 != 0;
    let sse = cr0
        && cr4
        && os.cr0 & (1 << 2) == 0
        && os.cr4 & ((1 << 9) | (1 << 10)) == ((1 << 9) | (1 << 10));
    match control {
        None | Unknown => Option::None,
        Cr0 => cr0.then_some((os.cr0, os.cr0 & (1 << 2) == 0)),
        Cr4(bit) => cr4.then_some((os.cr4, os.cr4 & (1u64 << bit) != 0)),
        Sse => (cr0 && cr4).then_some((os.cr4, sse)),
        Xsave => cr4.then_some((os.cr4, os.cr4 & (1 << 18) != 0)),
        Xcr0(mask) => {
            if !cr4 {
                return Option::None;
            }
            if os.cr4 & (1 << 18) == 0 {
                return Some((os.cr4, false));
            }
            if os.flags & c::cpu_capabilities_os_xcr0 == 0 {
                return Option::None;
            }
            // AVX-family prerequisites stay independent of its raw feature bit.
            let dependency = if mask == 7 || mask == 0xe7 {
                bit(&s.raw, 1, 0, 2, 28) == Some(true)
            } else {
                true
            };
            Some((os.xcr0, sse && dependency && os.xcr0 & mask == mask))
        }
        Efer(bit) => (os.flags & c::cpu_capabilities_os_efer != 0)
            .then_some((os.efer, os.efer & (1u64 << bit) != 0)),
        Mtrr => (os.flags & c::cpu_capabilities_os_mtrr_default != 0)
            .then_some((os.mtrr_default, os.mtrr_default & (1 << 11) != 0)),
    }
}
pub(crate) fn feature(s: &Snapshot, cpu: u32, index: u32) -> Result<c::CpuFeatureInfo, i32> {
    let Some(f) = FEATURES.get(index as usize) else {
        return Err(c::cpu_capabilities_end);
    };
    let mut out = c::CpuFeatureInfo {
        cpu_index: cpu,
        feature_index: index,
        feature_id: f.id,
        epoch: s.epoch,
        source_leaf: f.leaf,
        source_subleaf: f.sub,
        source_register: f.reg,
        source_bit: f.bit,
        use_state: c::cpu_feature_use_unknown,
        reason: c::cpu_feature_reason_no_owner,
        ..Default::default()
    };
    out.name[..f.name.len()].copy_from_slice(f.name);
    if f.bit < 32
        && crate::build_config::config::MODULE_CPU_REQUIREMENTS
            .iter()
            .any(|&(_, leaf, sub, reg, bit)| {
                leaf == f.leaf && sub == f.sub && u32::from(reg) == f.reg && u32::from(bit) == f.bit
            })
    {
        out.flags |= c::cpu_feature_flag_module_required;
    }
    if !applicable(f.vendor, &s.raw) {
        out.reason = c::cpu_feature_reason_vendor;
        return Ok(out);
    }
    let Some(raw) = s.raw.get(f.leaf, f.sub) else {
        out.reason = c::cpu_feature_reason_missing_leaf;
        return Ok(out);
    };
    out.raw_value = register(raw, f.reg);
    out.flags |= c::cpu_feature_flag_raw;
    let present = if f.bit == 32 {
        out.raw_value & 255 != 0
    } else {
        out.raw_value & (1u32 << f.bit) != 0
    };
    if !present {
        out.hardware_state = c::cpu_feature_hardware_absent;
        out.os_state = c::cpu_feature_os_disabled;
        out.reason = c::cpu_feature_reason_absent;
        return Ok(out);
    }
    out.hardware_state = c::cpu_feature_hardware_present;
    if matches!(f.control, None) {
        out.os_state = c::cpu_feature_os_not_applicable;
        out.reason = c::cpu_feature_reason_no_os_control;
    } else if let Some((value, enabled)) = activation(s, f.control) {
        out.control_value = value;
        out.flags |= c::cpu_feature_flag_control;
        out.os_state = if enabled {
            c::cpu_feature_os_enabled
        } else {
            c::cpu_feature_os_disabled
        };
        if !enabled {
            out.reason = c::cpu_feature_reason_disabled;
        }
    } else {
        out.reason = c::cpu_feature_reason_control_unknown;
    }
    Ok(out)
}
const _: () = {
    assert!(FEATURES.len() < 256);
    let mut i = 0;
    while i < FEATURES.len() {
        assert!(FEATURES[i].name.len() < 24);
        i += 1;
    }
};
