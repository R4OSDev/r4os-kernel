//! Qualified local optional CPU facts. Atomic copies may change with an exact
//! execution epoch; the topology owner's borrowed immutable hardware stays put.
use super::{cpu_capabilities_decode as decode, cpu_capabilities_store::Store, cpu_topology};
use crate::{
    arch::{percpu, runtime, safe_msr, smp_policy::MAX_CPUS},
    contract as c,
};

pub(crate) const OS_WORDS: usize = 7;
static SLOTS: [Store<{ decode::WORDS + OS_WORDS }>; MAX_CPUS] = [const { Store::new() }; MAX_CPUS];
#[derive(Clone, Copy)]
pub(crate) struct Os {
    pub cr0: u64,
    pub cr4: u64,
    pub xcr0: u64,
    pub efer: u64,
    pub pat: u64,
    pub mtrr_default: u64,
    pub flags: u32,
}
#[derive(Clone, Copy)]
pub(crate) struct Snapshot {
    pub raw: decode::Raw,
    pub os: Os,
    pub epoch: u64,
    pub sequence: u64,
}
pub(crate) struct LocalIrq(u64);
impl LocalIrq {
    pub(crate) fn acquire() -> Self {
        Self(runtime::save_local())
    }
}
impl Drop for LocalIrq {
    fn drop(&mut self) {
        runtime::restore_local(self.0);
    }
}
pub(crate) fn flags() -> u64 {
    let value;
    unsafe {
        core::arch::asm!("pushfq","pop {}",out(reg)value,options(preserves_flags));
    }
    value
}
pub(crate) fn bit(raw: &decode::Raw, leaf: u32, sub: u32, reg: u32, bit: u32) -> Option<bool> {
    raw.get(leaf, sub)
        .map(|v| register(v, reg) & (1u32 << bit) != 0)
}
pub(crate) fn register(v: core::arch::x86_64::CpuidResult, reg: u32) -> u32 {
    match reg {
        0 => v.eax,
        1 => v.ebx,
        2 => v.ecx,
        3 => v.edx,
        _ => unreachable!(),
    }
}
/// Only the exact post-baseline BSP/AP boot owner calls this, with own IDT,
/// PAT/FPU/LAPIC and topology identity already established. Optional MSR
/// refusal is an observed missing control, never failed CPU admission.
pub(crate) fn capture(index: u32, epoch: u64) -> bool {
    let _irq = LocalIrq::acquire();
    if index != percpu::current_index() || epoch == 0 || cpu_topology::epoch(index) != epoch {
        return false;
    }
    let Some(slot) = SLOTS.get(index as usize) else {
        return false;
    };
    let raw = decode::capture(core::arch::x86_64::__cpuid_count);
    let cr0;
    let cr4;
    unsafe {
        core::arch::asm!("mov {}, cr0",out(reg)cr0,options(nomem,nostack,preserves_flags));
        core::arch::asm!("mov {}, cr4",out(reg)cr4,options(nomem,nostack,preserves_flags));
    }
    let mut os = Os {
        cr0,
        cr4,
        xcr0: 0,
        efer: 0,
        pat: 0,
        mtrr_default: 0,
        flags: c::cpu_capabilities_os_cr0 | c::cpu_capabilities_os_cr4,
    };
    if bit(&raw, 1, 0, 2, 26) == Some(true) && cr4 & (1 << 18) != 0 {
        let low: u32;
        let high: u32;
        unsafe {
            core::arch::asm!("xgetbv",in("ecx")0u32,out("eax")low,out("edx")high,options(nomem,nostack,preserves_flags));
        }
        os.xcr0 = u64::from(low) | (u64::from(high) << 32);
        os.flags |= c::cpu_capabilities_os_xcr0;
    }
    if bit(&raw, 1, 0, 3, 5) == Some(true) {
        // EFER is architectural only with advertised long mode. No speculative
        // vendor power/PMU MSR read occurs during ordinary boot capture.
        for (supported, id, flag, destination) in [
            (
                bit(&raw, 0x8000_0001, 0, 3, 29) == Some(true),
                0xc000_0080,
                c::cpu_capabilities_os_efer,
                &mut os.efer,
            ),
            (
                bit(&raw, 1, 0, 3, 16) == Some(true),
                0x277,
                c::cpu_capabilities_os_pat,
                &mut os.pat,
            ),
            (
                bit(&raw, 1, 0, 3, 12) == Some(true),
                0x2ff,
                c::cpu_capabilities_os_mtrr_default,
                &mut os.mtrr_default,
            ),
        ] {
            if supported {
                match unsafe { safe_msr::read(id) } {
                    Ok(value) => {
                        *destination = value;
                        os.flags |= flag;
                    }
                    Err(_) => os.flags |= c::cpu_capabilities_os_msr_fault,
                }
            }
        }
    }
    // The local MCA owner may set documented CR4.MCE only after its resident
    // error capture is ready. Publish the actual resulting control state so
    // passive capability data remains coherent with native readback.
    crate::arch::machine_check::prepare(index, epoch, &raw);
    unsafe {
        core::arch::asm!("mov {}, cr4",out(reg)os.cr4,options(nomem,nostack,preserves_flags));
    }
    let mut words = [0; decode::WORDS + OS_WORDS];
    words[..decode::WORDS].copy_from_slice(&raw.encode());
    words[decode::WORDS..].copy_from_slice(&[
        os.cr0,
        os.cr4,
        os.xcr0,
        os.efer,
        os.pat,
        os.mtrr_default,
        u64::from(os.flags),
    ]);
    cpu_topology::epoch(index) == epoch && slot.publish(epoch, words).is_ok()
}
pub(crate) fn snapshot(index: u32) -> Result<Snapshot, i32> {
    if index >= cpu_topology::count() {
        return Err(c::cpu_capabilities_error_index);
    }
    let s = SLOTS[index as usize]
        .snapshot()
        .ok_or(c::cpu_capabilities_error_busy)?;
    if s.epoch != cpu_topology::epoch(index) {
        return Err(c::cpu_capabilities_error_busy);
    }
    let raw = decode::Raw::decode(
        s.words[..decode::WORDS]
            .try_into()
            .expect("Fixed capture words"),
    )
    .ok_or(c::cpu_capabilities_error_busy)?;
    let w = &s.words[decode::WORDS..];
    Ok(Snapshot {
        raw,
        os: Os {
            cr0: w[0],
            cr4: w[1],
            xcr0: w[2],
            efer: w[3],
            pat: w[4],
            mtrr_default: w[5],
            flags: w[6] as u32,
        },
        epoch: s.epoch,
        sequence: s.sequence,
    })
}
pub(crate) fn at_epoch(index: u32, epoch: u64) -> Result<Snapshot, i32> {
    if index >= cpu_topology::count() {
        return Err(c::cpu_capabilities_error_index);
    }
    if epoch == 0 || cpu_topology::epoch(index) != epoch {
        return Err(c::cpu_capabilities_error_epoch);
    }
    let s = snapshot(index)?;
    if s.epoch != epoch {
        return Err(c::cpu_capabilities_error_epoch);
    }
    Ok(s)
}
pub(crate) fn info(index: u32) -> Result<c::CpuCapabilitiesInfo, i32> {
    let s = snapshot(index)?;
    let raw = &s.raw;
    let identity = cpu_topology::snapshot(index).ok_or(c::cpu_capabilities_error_busy)?;
    if identity.start_epoch != s.epoch {
        return Err(c::cpu_capabilities_error_busy);
    }
    let mut out = c::CpuCapabilitiesInfo {
        cpu_index: index,
        state: u32::from(percpu::state(index)),
        flags: c::cpu_capabilities_flag_captured,
        feature_count: super::cpu_capabilities_features::count(),
        epoch: s.epoch,
        capture_sequence: s.sequence,
        apic_id: identity.apic_id,
        configured_cpus: cpu_topology::count(),
        vendor_name: raw.vendor(),
        hypervisor_name: raw.hypervisor(),
        cr0: s.os.cr0,
        cr4: s.os.cr4,
        xcr0: s.os.xcr0,
        efer: s.os.efer,
        pat: s.os.pat,
        os_read_flags: s.os.flags,
        recovered_msr_faults: safe_msr::recovered_faults(index),
        legacy_inventory_msr_faults: if index == 0 {
            super::cpu::inventory_msr_failures()
        } else {
            0
        },
        ..Default::default()
    };
    if percpu::is_online(index) {
        out.flags |= c::cpu_capabilities_flag_online;
    }
    if percpu::is_schedulable(index) {
        out.flags |= c::cpu_capabilities_flag_schedulable;
    }
    if let Some(r) = raw.get(0, 0) {
        out.max_basic_leaf = r.eax;
    }
    if let Some(r) = raw.get(0x8000_0000, 0) {
        out.max_extended_leaf = r.eax;
    }
    if let Some(r) = raw.get(7, 0) {
        out.max_leaf7_subleaf = r.eax;
    }
    if let Some((family, model, stepping)) = raw.signature() {
        out.family = family;
        out.model = model;
        out.stepping = stepping;
        out.flags |= c::cpu_capabilities_flag_signature;
    }
    if let Some(brand) = raw.brand() {
        out.brand_name = brand;
        out.flags |= c::cpu_capabilities_flag_brand;
    }
    if bit(raw, 1, 0, 2, 31) == Some(true) {
        out.flags |= c::cpu_capabilities_flag_hypervisor_present;
    }
    if let Some(max) = raw.hypervisor_maximum() {
        out.max_hypervisor_leaf = max;
        out.flags |= c::cpu_capabilities_flag_hypervisor_valid;
    }
    if let Some(r) = raw.get(0x4000_0000, 0) {
        out.raw_hypervisor_maximum = r.eax;
    }
    for (issue, flag) in [
        (
            decode::INVALID_BASIC,
            c::cpu_capabilities_flag_invalid_basic,
        ),
        (
            decode::INVALID_EXTENDED,
            c::cpu_capabilities_flag_invalid_extended,
        ),
        (
            decode::INVALID_HYPERVISOR,
            c::cpu_capabilities_flag_invalid_hypervisor,
        ),
        (
            decode::LEGACY_KVM_MAXIMUM,
            c::cpu_capabilities_flag_legacy_kvm_maximum,
        ),
    ] {
        if raw.issues & issue != 0 {
            out.flags |= flag;
        }
    }
    // Intel's nominal frequency/ratio formats are not inferred for unknown
    // vendors or AMD. No brand parsing and no guessed crystal oscillator.
    if raw.vendor() == *b"GenuineIntel" {
        if let Some(r) = raw.get(0x16, 0) {
            out.base_mhz = r.eax & 0xffff;
            out.maximum_mhz = r.ebx & 0xffff;
            out.bus_mhz = r.ecx & 0xffff;
            if out.base_mhz != 0 || out.maximum_mhz != 0 || out.bus_mhz != 0 {
                out.flags |= c::cpu_capabilities_flag_nominal_frequency;
            }
        }
        if let Some(r) = raw.get(0x15, 0) {
            out.tsc_denominator = r.eax;
            out.tsc_numerator = r.ebx;
            out.crystal_hz = r.ecx;
            if r.eax != 0 && r.ebx != 0 && r.ecx != 0 {
                out.flags |= c::cpu_capabilities_flag_tsc_ratio;
            }
        }
        if let Some(r) = raw.get(0x0a, 0) {
            out.pmu_version = r.eax & 255;
            if out.pmu_version != 0 {
                out.pmu_counters = (r.eax >> 8) & 255;
                out.pmu_counter_width = (r.eax >> 16) & 255;
                out.flags |= c::cpu_capabilities_flag_pmu;
            }
        }
    } else if raw.vendor() == *b"AuthenticAMD"
        && let Some(r) = raw.get(0x8000_0022, 0)
        && r.eax & 1 != 0
    {
        out.pmu_version = 2;
        out.pmu_counters = r.ebx & 15;
        out.flags |= c::cpu_capabilities_flag_pmu;
    }
    if let Some(r) = raw.get(1, 0)
        && r.edx & (1 << 19) != 0
    {
        out.clflush_line_bytes = ((r.ebx >> 8) & 255) * 8;
    }
    if let Some(r) = raw.get(0x8000_0008, 0) {
        out.physical_address_bits = r.eax & 255;
        out.virtual_address_bits = (r.eax >> 8) & 255;
    }
    if let Some(r) = raw.get(0x0d, 0) {
        out.supported_xcr0 = u64::from(r.eax) | (u64::from(r.edx) << 32);
        out.enabled_xsave_bytes = r.ebx;
        out.maximum_xsave_bytes = r.ecx;
    }
    if cpu_topology::epoch(index) != s.epoch {
        return Err(c::cpu_capabilities_error_busy);
    }
    Ok(out)
}
