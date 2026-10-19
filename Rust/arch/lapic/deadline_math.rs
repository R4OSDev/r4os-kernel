//! Finite hardware deadlines. No CPU, clock origin or timer state lives here.
pub(super) const MIN_CYCLES: u64 = 16;
const MAX_SECONDS: u64 = 60;

pub(super) fn hardware_allowed(
    ecx: u32,
    edx: u32,
    signature: u32,
    intel: bool,
    invariant: bool,
) -> bool {
    let hypervisor = ecx & (1 << 31) != 0;
    edx & ((1 << 4) | (1 << 5) | (1 << 9)) == (1 << 4) | (1 << 5) | (1 << 9)
        && ecx & (1 << 24) != 0
        // Native idle must not stop the only timer's timebase. A VM exposing
        // deadline support owns virtual TSC/deadline coordination (SDM13.5.4.1).
        && (invariant || hypervisor)
        && native_model_allowed(intel, hypervisor, signature)
}

pub(super) fn interval(ticks: u64, tsc_hz: u64, hz: u32) -> Option<(u64, u64)> {
    if tsc_hz == 0 || hz == 0 {
        return None;
    }
    let ticks = ticks.max(1).min(u64::from(hz).checked_mul(MAX_SECONDS)?);
    let cycles = (u128::from(ticks) * u128::from(tsc_hz)).div_ceil(u128::from(hz));
    Some((ticks, u64::try_from(cycles).ok()?.max(MIN_CYCLES)))
}

pub(super) fn target(now: u64, cycles: u64) -> Option<u64> {
    now.checked_add(cycles.max(MIN_CYCLES))
        .filter(|&v| v != u64::MAX)
}

/// Skip missed periods in one arithmetic step; never loop to catch up IRQs.
pub(super) fn periodic_next(previous: u64, now: u64, period: u64) -> Option<u64> {
    if period == 0 || previous == 0 || now < previous {
        return None;
    }
    let periods = (now - previous).checked_div(period)?.checked_add(1)?;
    let next = previous.checked_add(periods.checked_mul(period)?)?;
    if next.saturating_sub(now) < MIN_CYCLES {
        target(now, MIN_CYCLES)
    } else {
        (next != u64::MAX).then_some(next)
    }
}

pub(super) fn remaining_ticks(now: u64, deadline: u64, tsc_hz: u64, hz: u32) -> u64 {
    if deadline <= now || tsc_hz == 0 {
        return 1;
    }
    let ticks = (u128::from(deadline - now) * u128::from(hz)).div_ceil(u128::from(tsc_hz));
    u64::try_from(ticks).unwrap_or(u64::MAX).max(1)
}

/// Some native Intel models require a verified microcode revision for this
/// timer (Linux v6.12 apic_validate_deadline_timer). Until the microcode owner
/// can attest it, retain the calibrated count timer on these exact models.
pub(super) fn native_model_allowed(intel: bool, hypervisor: bool, signature: u32) -> bool {
    if hypervisor {
        return true;
    }
    if !intel {
        return false;
    }
    let base_family = (signature >> 8) & 15;
    let family = base_family
        + if base_family == 15 {
            (signature >> 20) & 255
        } else {
            0
        };
    let model = ((signature >> 4) & 15)
        | if matches!(base_family, 6 | 15) {
            ((signature >> 16) & 15) << 4
        } else {
            0
        };
    family != 6
        || !matches!(
            model,
            0x3f | 0x4f
                | 0x56
                | 0x55
                | 0x3c
                | 0x45
                | 0x46
                | 0x3d
                | 0x47
                | 0x4e
                | 0x5e
                | 0x8e
                | 0x9e
        )
}
