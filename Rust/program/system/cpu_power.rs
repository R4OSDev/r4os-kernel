//! Preterminal admission and cooperative return, without profile policy.
//! Keep this guard across real service/driver waits, never a Runtime owner.
use crate::{
    boot::foundation as log,
    contract as c,
    platform::{cpu_power::SystemTransition, timer},
    program::service_api,
};

pub(super) fn prepare(timeout: u64) -> Result<SystemTransition, i32> {
    let Some(transition) = SystemTransition::acquire() else {
        log::puts(b"[POWER] CPU return transition already active; request refused\r\n");
        return Err(c::power_transition_error_busy);
    };
    let ticks = timeout.min(u64::from(timer::frequency().max(1)).saturating_mul(5));
    if !service_api::prepare_cpu_power_stop(ticks) || !transition.returned() {
        // Refusal reopens new ownership, retaining every live provider/control.
        // Do not stop APs or turn a failed return into a reset acknowledgement.
        log::puts(b"[POWER] CPU requests not returned; system transition refused\r\n");
        return Err(c::power_transition_error_retained);
    }
    Ok(transition)
}

/// The old raw slots never acquire a new service/process wait contract.
/// Existing hardware ownership makes them refuse immediately; callers using
/// the new explicit operation can request cooperative return first.
pub(super) fn prepare_immediate() -> Option<SystemTransition> {
    let transition = SystemTransition::acquire()?;
    if !transition.returned() {
        log::puts(b"[POWER] CPU requests still owned; orderly transition required\r\n");
        return None;
    }
    Some(transition)
}
