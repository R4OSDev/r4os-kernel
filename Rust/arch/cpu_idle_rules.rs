//! Pure qualification of the architectural Intel C2 MWAIT entry contract.
use crate::{contract as c, platform::cpu_capabilities_decode::Raw};

pub(crate) fn hardware(raw: &Raw, hint: u32) -> Result<(), u32> {
    let one = raw.get(1, 0).ok_or(c::cpu_idle_reason_monitor)?;
    if one.ecx & (1 << 31) != 0 {
        return Err(c::cpu_idle_reason_hypervisor);
    }
    if raw.vendor() != *b"GenuineIntel" || raw.signature().is_none_or(|s| s.0 != 6) {
        return Err(c::cpu_idle_reason_vendor);
    }
    if one.ecx & (1 << 3) == 0 || one.edx & (1 << 5) == 0 {
        return Err(c::cpu_idle_reason_monitor);
    }
    let five = raw.get(5, 0).ok_or(c::cpu_idle_reason_monitor)?;
    if five.ecx & 3 != 3 || five.eax == 0 || five.eax > five.ebx || five.ebx > 4096 {
        return Err(c::cpu_idle_reason_monitor);
    }
    if raw.get(6, 0).is_none_or(|v| v.eax & (1 << 2) == 0) {
        return Err(c::cpu_idle_reason_arat);
    }
    if raw
        .get(0x8000_0007, 0)
        .is_none_or(|v| v.edx & (1 << 8) == 0)
    {
        return Err(c::cpu_idle_reason_invariant_tsc);
    }
    // EAX[7:4] is the target hardware C-state index (C1=0). ACPI's C2
    // semantics are independently validated by the external firmware owner.
    // Permit advertised deeper hints, never the reserved index15 or high bits.
    let state = (hint >> 4) & 15;
    let substate = hint & 15;
    if hint > 0xff || state == 0 || state >= 7 || (five.edx >> ((state + 1) * 4)) & 15 <= substate {
        return Err(c::cpu_idle_reason_substate);
    }
    Ok(())
}

pub(crate) fn target(latency: u32, minimum: u32, factor: u32) -> u64 {
    u64::from(minimum).max(u64::from(latency.max(1)) * u64::from(factor))
}
