//! Explicit diagnostic metadata owned by the exact current native task.
//! All access holds runtime admission. No pointer/borrow/owner spans userland
//! work, a physical switch or R4D callback. Only real IRQ entry paths record.
use crate::{
    arch::{fpu, percpu},
    contract as c,
    sched::{scheduler, task::task_registry},
};

#[derive(Clone, Copy, Default)]
pub(crate) struct Observation {
    sequence: u64,
    active: bool,
    info: c::FpuObservationInfo,
}

/// Caller holds runtime admission and has validated the complete request and
/// own retained module interval. Output is a copy; no task reference escapes.
pub(crate) fn control(request: c::FpuObservationRequest) -> Result<c::FpuObservationInfo, i32> {
    if request.version != 1 || request.size != 48 || request.flags != 0 {
        return Err(c::fpu_observation_error_invalid);
    }
    let task = scheduler::current();
    if task.is_null() || fpu::active_bytes() == 0 {
        return Err(c::fpu_observation_error_unavailable);
    }
    let core = unsafe { task_registry::core(task.cast()) };
    if unsafe { !(*core).uses_fpu || !(*core).fpu_state_valid || (*core).generation == 0 } {
        return Err(c::fpu_observation_error_unavailable);
    }
    let base = c::FpuObservationInfo {
        flags: c::fpu_observation_flag_available,
        task_id: unsafe { (*core).id },
        task_generation: unsafe { (*core).generation },
        ..Default::default()
    };
    let slot = unsafe { core::ptr::addr_of_mut!((*core).fpu_observation) };
    match request.operation {
        c::fpu_observation_capabilities => {
            if request.token != 0
                || request.task_generation != 0
                || request.rip_begin != 0
                || request.rip_end != 0
            {
                return Err(c::fpu_observation_error_invalid);
            }
            Ok(base)
        }
        c::fpu_observation_begin => {
            if request.token != 0
                || request.task_generation != 0
                || request.rip_begin == 0
                || request.rip_end <= request.rip_begin
                || request.rip_end - request.rip_begin > c::fpu_observation_max_window_bytes
            {
                return Err(c::fpu_observation_error_invalid);
            }
            if unsafe { (*slot).active } {
                return Err(c::fpu_observation_error_busy);
            }
            let sequence = unsafe { (*slot).sequence }
                .checked_add(1)
                .ok_or(c::fpu_observation_error_exhausted)?;
            let info = c::FpuObservationInfo {
                flags: base.flags | c::fpu_observation_flag_active,
                token: sequence,
                rip_begin: request.rip_begin,
                rip_end: request.rip_end,
                ..base
            };
            unsafe {
                slot.write(Observation {
                    sequence,
                    active: true,
                    info,
                });
            }
            Ok(info)
        }
        c::fpu_observation_end => {
            let old = unsafe { slot.read() };
            if !old.active
                || request.token == 0
                || request.token != old.info.token
                || request.task_generation != base.task_generation
                || old.info.task_generation != base.task_generation
                || request.rip_begin != old.info.rip_begin
                || request.rip_end != old.info.rip_end
            {
                return Err(c::fpu_observation_error_stale);
            }
            let info = c::FpuObservationInfo {
                flags: base.flags | c::fpu_observation_flag_closed,
                ..old.info
            };
            unsafe {
                slot.write(Observation {
                    active: false,
                    info,
                    ..old
                });
            }
            Ok(info)
        }
        _ => Err(c::fpu_observation_error_invalid),
    }
}

// The task is the actual physical current under the existing IRQ runtime
// owner. Direct scheduler_irq_enter/leave calls cannot reach these hooks.
fn record(rip: u64, kind: u8, irq: u32, owner: u32, handler: u64) {
    let task = scheduler::current();
    if task.is_null() {
        return;
    }
    let core = unsafe { task_registry::core(task.cast()) };
    let slot = unsafe { core::ptr::addr_of_mut!((*core).fpu_observation) };
    if !unsafe { (*slot).active } {
        return;
    }
    let mut observation = unsafe { slot.read() };
    if observation.info.task_generation != unsafe { (*core).generation }
        || observation.info.task_id != unsafe { (*core).id }
        || rip < observation.info.rip_begin
        || rip >= observation.info.rip_end
    {
        return;
    }
    let cpu = percpu::current_index();
    if cpu >= 64 {
        return;
    }
    observation.info.observed_cpus |= 1u64 << cpu;
    observation.info.last_cpu = cpu;
    observation.info.last_rip = rip;
    match kind {
        0 => observation.info.timer_irqs = observation.info.timer_irqs.saturating_add(1),
        1 => {
            observation.info.timer_preemptions =
                observation.info.timer_preemptions.saturating_add(1)
        }
        2 => {
            if owner == 0 || handler == 0 {
                return;
            }
            observation.info.external_handlers =
                observation.info.external_handlers.saturating_add(1);
            observation.info.last_irq = irq;
            observation.info.last_owner = owner;
            observation.info.last_handler = handler;
        }
        _ => return,
    }
    unsafe {
        slot.write(observation);
    }
}
pub(crate) fn timer(rip: u64) {
    record(rip, 0, u32::MAX, 0, 0);
}
pub(crate) fn timer_switch(rip: u64) {
    record(rip, 1, u32::MAX, 0, 0);
}
pub(crate) fn external(rip: u64, irq: u8, owner: u32, handler: u64) {
    // IRQ0 remains a local timer even when an R4D (for example EXAMPLE)
    // subscribes to its ticks. It cannot prove an external device interrupt.
    if irq != 0 {
        record(rip, 2, u32::from(irq), owner, handler);
    }
}
