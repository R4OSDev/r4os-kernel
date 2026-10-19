//! Named local operations for external hardware owners. There is
//! no caller-supplied MSR address and no speculative model-register scan.
use super::{
    cpu_capabilities::{self as catalog, LocalIrq, Snapshot, bit},
    cpu_topology,
};
use crate::{
    arch::{cpu_lifecycle as life, percpu, runtime, safe_msr},
    contract as c,
    sched::task_context::Admission,
};

pub(crate) struct Session {
    index: u32,
    epoch: u64,
    _lease: life::ProbeLease,
    _admission: Admission,
}
impl Session {
    /// The admission guards retain the exact task and stable CPU membership;
    /// neither is a runtime/ranked owner held across an instruction or wait.
    pub(crate) fn acquire(index: u32, epoch: u64) -> Result<Self, i32> {
        if catalog::flags() & (1 << 9) == 0 || !crate::memory::owner::fault_allowed() {
            return Err(c::cpu_capabilities_error_context);
        }
        let admission = Admission::acquire(true).ok_or(c::cpu_capabilities_error_context)?;
        let lease = life::probe_lease().ok_or(c::cpu_capabilities_error_busy)?;
        let value = Self {
            index,
            epoch,
            _lease: lease,
            _admission: admission,
        };
        {
            let _irq = LocalIrq::acquire();
            value.current()?;
        }
        Ok(value)
    }
    fn current(&self) -> Result<Snapshot, i32> {
        if self.index >= cpu_topology::count() {
            return Err(c::cpu_capabilities_error_index);
        }
        if self.epoch == 0 || cpu_topology::epoch(self.index) != self.epoch {
            return Err(c::cpu_capabilities_error_epoch);
        }
        if !percpu::is_online(self.index)
            || !percpu::is_schedulable(self.index)
            || life::is_terminal()
        {
            return Err(c::cpu_capabilities_error_offline);
        }
        if percpu::current_index() != self.index {
            return Err(c::cpu_capabilities_error_context);
        }
        let s = catalog::at_epoch(self.index, self.epoch)?;
        if bit(&s.raw, 1, 0, 3, 5) != Some(true) {
            return Err(c::cpu_capabilities_error_unsupported);
        }
        Ok(s)
    }
    pub(crate) fn read(&self, id: u32) -> Result<c::CpuRegisterInfo, i32> {
        let _irq = LocalIrq::acquire();
        let s = self.current()?;
        let (msr, name) = select(&s, id).ok_or(c::cpu_capabilities_error_unsupported)?;
        if matches!(
            id,
            c::cpu_register_id_intel_hwp_capabilities
                | c::cpu_register_id_intel_hwp_request
                | c::cpu_register_id_intel_hwp_package
        ) && unsafe { safe_msr::read_exact(self.index, self.epoch, 0x770) }
            .map_err(register_error)?
            != 1
        {
            return Err(c::cpu_capabilities_error_unsupported);
        }
        // Local IDT and exact epoch come from real boot qualification. Select
        // established the architectural feature or exact verified model.
        // Intel's signature latch is read through its exact architectural
        // zero-preload/CPUID1 procedure. This is neither a microcode update
        // nor a caller-controlled write. AMD PATCH_LEVEL is strictly read-only.
        if id == c::cpu_register_id_microcode && s.raw.vendor() == *b"GenuineIntel" {
            unsafe { safe_msr::write_exact(self.index, self.epoch, msr, 0) }
                .map_err(register_error)?;
            let _ = core::arch::x86_64::__cpuid(1);
        }
        let value =
            unsafe { safe_msr::read_exact(self.index, self.epoch, msr) }.map_err(register_error)?;
        let mut out = c::CpuRegisterInfo {
            cpu_index: self.index,
            register_id: id,
            flags: c::cpu_register_flag_raw,
            epoch: self.epoch,
            value,
            capture_sequence: s.sequence,
            ..Default::default()
        };
        out.name[..name.len()].copy_from_slice(name);
        Ok(out)
    }
    pub(crate) fn compare_write(
        &self,
        token: u64,
        id: u32,
        expected: u64,
        requested: u64,
    ) -> Result<c::CpuRegisterWriteInfo, i32> {
        let _irq = LocalIrq::acquire();
        let s = self.current()?;
        let (msr, _) = select(&s, id).ok_or(c::cpu_capabilities_error_unsupported)?;
        if !writable(&s, id, expected, requested) {
            return Err(c::cpu_capabilities_error_unsupported);
        }
        if id == c::cpu_register_id_intel_hwp_request
            && unsafe { safe_msr::read_exact(self.index, self.epoch, 0x770) }
                .map_err(register_error)?
                != 1
        {
            return Err(c::cpu_capabilities_error_unsupported);
        }
        let before =
            unsafe { safe_msr::read_exact(self.index, self.epoch, msr) }.map_err(register_error)?;
        let mut out = c::CpuRegisterWriteInfo {
            cpu_index: self.index,
            register_id: id,
            epoch: self.epoch,
            lease: token,
            expected,
            requested,
            observed_before: before,
            flags: c::cpu_register_write_flag_before,
            ..Default::default()
        };
        if before != expected {
            out.last_error = c::cpu_capabilities_error_busy;
            return Ok(out);
        }
        out.flags |= c::cpu_register_write_flag_attempted;
        match unsafe { safe_msr::write_exact(self.index, self.epoch, msr, requested) } {
            Ok(()) => out.flags |= c::cpu_register_write_flag_written,
            Err(reason) => out.last_error = register_error(reason),
        }
        match unsafe { safe_msr::read_exact(self.index, self.epoch, msr) } {
            Ok(value) => {
                out.observed_after = value;
                out.flags |= c::cpu_register_write_flag_after;
                if value != requested && out.last_error == 0 {
                    out.last_error = c::cpu_capabilities_error_busy;
                }
            }
            Err(reason) => out.last_error = register_error(reason),
        }
        Ok(out)
    }
    pub(crate) fn check(&self) -> Result<c::ProgramKernelCpuRegisterCheck, i32> {
        let s = {
            let _irq = LocalIrq::acquire();
            self.current()?
        };
        let saved = catalog::flags();
        let mut out = c::ProgramKernelCpuRegisterCheck {
            cpu_index: self.index,
            epoch: self.epoch,
            capture_sequence: s.sequence,
            flags_before: saved,
            faults_before: safe_msr::recovered_faults(self.index),
            ..Default::default()
        };
        // FFFFFFFF is reserved in the architectural MSR spaces. Zero writes
        // can only be refused or silently ignored by a filtering hypervisor;
        // the latter is a failed check, never a passing recovery receipt.
        const RESERVED: u32 = u32::MAX;
        for (write, off, case) in [
            (false, false, c::cpu_register_check_case_read_on),
            (true, false, c::cpu_register_check_case_write_on),
            (false, true, c::cpu_register_check_case_read_off),
            (true, true, c::cpu_register_check_case_write_off),
        ] {
            let irq = if off { Some(LocalIrq::acquire()) } else { None };
            let before = catalog::flags();
            let faults = safe_msr::recovered_faults(self.index);
            // The exact primitive rechecks physical CPU and epoch after CLI,
            // even if an IF-on caller migrated immediately before the call.
            let result = if write {
                unsafe { safe_msr::write_exact(self.index, self.epoch, RESERVED, 0) }
            } else {
                unsafe { safe_msr::read_exact(self.index, self.epoch, RESERVED) }.map(|_| ())
            };
            let after = catalog::flags();
            let delta = safe_msr::recovered_faults(self.index).checked_sub(faults);
            let preserved = (before ^ (after)) & (1 << 9) == 0;
            if preserved {
                out.if_checks += 1;
            }
            if result != Err(safe_msr::Error::Busy) {
                if write {
                    out.writes += 1;
                } else {
                    out.reads += 1;
                }
            }
            if result == Err(safe_msr::Error::Fault)
                && delta == Some(1)
                && preserved
                && percpu::current_index() == self.index
                && cpu_topology::epoch(self.index) == self.epoch
            {
                out.passed |= case;
            } else {
                out.failed |= case;
            }
            drop(irq);
        }
        runtime::restore_local(saved);
        out.flags_after = catalog::flags();
        out.faults_after = safe_msr::recovered_faults(self.index);
        out.faults = out
            .faults_after
            .checked_sub(out.faults_before)
            .and_then(|v| u32::try_from(v).ok())
            .unwrap_or(0);
        if (out.flags_before ^ out.flags_after) & (1 << 9) != 0 {
            out.cleanup_errors += 1;
        }
        out.status = if out.passed == c::cpu_register_check_case_all
            && out.failed == 0
            && out.cleanup_errors == 0
            && out.faults == 4
            && out.if_checks == 4
        {
            c::cpu_register_check_status_passed
        } else {
            c::cpu_register_check_status_failed
        };
        Ok(out)
    }
}
fn register_error(reason: safe_msr::Error) -> i32 {
    match reason {
        safe_msr::Error::Busy => c::cpu_capabilities_error_busy,
        safe_msr::Error::Fault => c::cpu_capabilities_error_fault,
    }
}
fn native_haswell(s: &Snapshot) -> bool {
    s.raw.vendor() == *b"GenuineIntel"
        && bit(&s.raw, 1, 0, 2, 31) == Some(false)
        && s.raw
            .signature()
            .is_some_and(|(family, model, _)| family == 6 && matches!(model, 0x3c | 0x45 | 0x46))
}
fn native_milan_b1(s: &Snapshot) -> bool {
    s.raw.vendor() == *b"AuthenticAMD"
        && bit(&s.raw, 1, 0, 2, 31) == Some(false)
        && s.raw.signature() == Some((0x19, 1, 1))
}
fn native_skylake_hwp(s: &Snapshot) -> bool {
    s.raw.vendor() == *b"GenuineIntel"
        && bit(&s.raw, 1, 0, 2, 31) == Some(false)
        && s.raw.signature().is_some_and(|(family, model, _)| family == 6 && matches!(model, 0x4e | 0x5e))
        && bit(&s.raw, 6, 0, 0, 7) == Some(true)
        // PECI overrides, posted fast access and later package polarity are
        // separate models/contracts. Never guess effective ownership there.
        && [16, 18, 22].iter().all(|&b| bit(&s.raw, 6, 0, 0, b) == Some(false))
}
fn writable(s: &Snapshot, id: u32, expected: u64, requested: u64) -> bool {
    if !native_skylake_hwp(s) {
        return false;
    }
    match id {
        c::cpu_register_id_intel_hwp_enable => expected == 0 && requested == 1,
        c::cpu_register_id_intel_hwp_request => {
            let raw = &s.raw;
            let epp = if bit(raw, 6, 0, 0, 10) == Some(true) {
                0xff00_0000
            } else {
                0
            };
            let fields = 0x00ff_ffff | epp;
            let mut known = fields;
            if bit(raw, 6, 0, 0, 9) == Some(true) {
                known |= 0x3ffu64 << 32;
            }
            if bit(raw, 6, 0, 0, 11) == Some(true) {
                known |= 1u64 << 42;
            }
            if bit(raw, 6, 0, 0, 17) == Some(true) {
                known |= 0x1fu64 << 59;
            }
            expected & !known == 0
                && expected & (1u64 << 42) == 0
                && (expected ^ requested) & !fields == 0
        }
        _ => false,
    }
}
/// Pure whitelist. Optional feature absence, other models and unknown vendors
/// cannot be promoted merely because RDMSR exists or the CPU brand looks alike.
fn select(s: &Snapshot, id: u32) -> Option<(u32, &'static [u8])> {
    let raw = &s.raw;
    let intel = raw.vendor() == *b"GenuineIntel";
    let amd = raw.vendor() == *b"AuthenticAMD";
    if bit(raw, 1, 0, 3, 5) != Some(true) {
        return None;
    }
    match id {
        c::cpu_register_id_efer if bit(raw, 0x8000_0001, 0, 3, 29) == Some(true) => {
            Some((0xc000_0080, b"efer"))
        }
        c::cpu_register_id_apic_base if bit(raw, 1, 0, 3, 9) == Some(true) => {
            Some((0x1b, b"apic-base"))
        }
        c::cpu_register_id_pat if bit(raw, 1, 0, 3, 16) == Some(true) => Some((0x277, b"pat")),
        c::cpu_register_id_mtrr_cap if bit(raw, 1, 0, 3, 12) == Some(true) => {
            Some((0xfe, b"mtrr-cap"))
        }
        c::cpu_register_id_mperf if (intel || amd) && bit(raw, 6, 0, 2, 0) == Some(true) => {
            Some((0xe7, b"mperf"))
        }
        c::cpu_register_id_aperf if (intel || amd) && bit(raw, 6, 0, 2, 0) == Some(true) => {
            Some((0xe8, b"aperf"))
        }
        // Intel SDM093 Vol4 table2-29, family6 models3C/45/46 only. Other
        // verified tables can be appended deliberately in later versions.
        c::cpu_register_id_intel_platform_info
            if intel
                && raw.signature().is_some_and(|(family, model, _)| {
                    family == 6 && matches!(model, 0x3c | 0x45 | 0x46)
                }) =>
        {
            Some((0xce, b"intel-platform-info"))
        }
        c::cpu_register_id_microcode if native_haswell(s) || native_milan_b1(s) => {
            Some((0x8b, b"microcode"))
        }
        c::cpu_register_id_intel_temperature_target if native_haswell(s) => {
            Some((0x1a2, b"intel-temp-target"))
        }
        c::cpu_register_id_intel_thermal_status
            if native_haswell(s)
                && bit(raw, 1, 0, 3, 22) == Some(true)
                && bit(raw, 6, 0, 0, 0) == Some(true) =>
        {
            Some((0x19c, b"intel-thermal-status"))
        }
        c::cpu_register_id_intel_energy_unit if native_haswell(s) => {
            Some((0x606, b"intel-energy-unit"))
        }
        c::cpu_register_id_intel_package_energy if native_haswell(s) => {
            Some((0x611, b"intel-pkg-energy"))
        }
        c::cpu_register_id_intel_core_energy if native_haswell(s) => {
            Some((0x639, b"intel-core-energy"))
        }
        c::cpu_register_id_amd_pstate0 if native_milan_b1(s) => Some((0xc001_0064, b"amd-pstate0")),
        c::cpu_register_id_amd_energy_unit if native_milan_b1(s) => {
            Some((0xc001_0299, b"amd-energy-unit"))
        }
        c::cpu_register_id_amd_package_energy if native_milan_b1(s) => {
            Some((0xc001_029b, b"amd-pkg-energy"))
        }
        c::cpu_register_id_amd_core_energy if native_milan_b1(s) => {
            Some((0xc001_029a, b"amd-core-energy"))
        }
        c::cpu_register_id_intel_hwp_enable if native_skylake_hwp(s) => {
            Some((0x770, b"intel-hwp-enable"))
        }
        c::cpu_register_id_intel_hwp_capabilities if native_skylake_hwp(s) => {
            Some((0x771, b"intel-hwp-caps"))
        }
        c::cpu_register_id_intel_hwp_request if native_skylake_hwp(s) => {
            Some((0x774, b"intel-hwp-request"))
        }
        c::cpu_register_id_intel_hwp_package
            if native_skylake_hwp(s) && bit(raw, 6, 0, 0, 11) == Some(true) =>
        {
            Some((0x772, b"intel-hwp-package"))
        }
        _ => None,
    }
}
