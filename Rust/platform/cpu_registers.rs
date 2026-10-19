//! Named, read-only local operations for external hardware owners. There is
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
        // Local IDT and exact epoch come from real boot qualification. Select
        // established the architectural feature or exact verified model.
        let value = unsafe { safe_msr::read(msr) }.map_err(|reason| match reason {
            safe_msr::Error::Busy => c::cpu_capabilities_error_busy,
            safe_msr::Error::Fault => c::cpu_capabilities_error_fault,
        })?;
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
        _ => None,
    }
}
