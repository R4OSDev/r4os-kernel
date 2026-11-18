//! Canonical native orchestration of the existing explicit TaskRegistry cases.
//! Native owner operations retain their exact task and resource identities.
mod balance;
mod continuations;
mod deadlines;
mod fpu;
mod fpu_contract;
mod fpu_environment;
mod fpu_opt;
mod helpers;
mod irq_retirement;
pub(crate) mod live;
mod owner_snapshots;
mod resources;
mod roles;
mod run;
mod state;
mod workers;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Case {
    TscDeadline,
    IdleDeadline,
    Xsaveopt,
    XstateContract,
    FpuEnvironment,
    FpuGuards,
    FpuWork,
    All,
}

fn select(value: &[u8]) -> Option<Case> {
    const CASES: &[(&[u8], Case)] = &[
        (b"tsc-deadline", Case::TscDeadline),
        (b"idle-deadline", Case::IdleDeadline),
        (b"xsaveopt", Case::Xsaveopt),
        (b"xstate-contract", Case::XstateContract),
        (b"fpu-environment", Case::FpuEnvironment),
        (b"fpu-guards", Case::FpuGuards),
        (b"fpu-work", Case::FpuWork),
        (b"yes", Case::All),
    ];
    CASES
        .iter()
        .find(|(name, _)| value.eq_ignore_ascii_case(name))
        .map(|(_, case)| *case)
}

pub(crate) fn selected() -> Option<Case> {
    let value = unsafe {
        crate::boot::config::option_value(crate::boot::config::get(), b"TASKREGISTRY", b"selftest")
    }?;
    select(value)
}

#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_task_registry_selftest() -> bool {
    selected().is_none_or(run_selected)
}
pub(crate) fn run_selected(choice: Case) -> bool {
    let Some(mut gate) = crate::sched::kernel_check::gate() else {
        return false;
    };
    let result = boot_cases(choice);
    if !result || resources::retained_work() {
        gate.retain();
    }
    result
}
fn boot_cases(choice: Case) -> bool {
    unsafe {
        match choice {
            Case::TscDeadline => {
                if !run::prepare() {
                    return false;
                }
                // Reuse the original strict checkpoint/earlier-IRQ and bounded
                // idle-timeout oracles before the isolated hardware/source cases.
                let idle = deadlines::probe() && deadlines::storm();
                let line = if idle {
                    b"[TSCDEADLINE-IDLE] result=PASS checkpoint=finite earlier=real-irq due=65 bounded=64+1 future=held\r\n".as_slice()
                } else {
                    b"[TSCDEADLINE-CHECK] result=FAILED stage=legacy-idle\r\n".as_slice()
                };
                crate::boot::log::text(line);
                crate::boot::log_route::serial_write_raw(line);
                if !idle {
                    crate::arch::lapic::log_timer(0);
                    return false;
                }
                // Keep the original real source demotion/AP wake receipt. The
                // same BSP oracle then proves retention on the calibrated count
                // fallback, with a new clock generation and fresh physical arm.
                crate::arch::lapic::deadline_boot_check() && deadlines::retained_idle()
            }
            Case::IdleDeadline => {
                if !deadlines::retained_idle() || !deadlines::storm() {
                    return helpers::fail(b"idle-deadline");
                }
                helpers::puts(b"TASKREG08244 idle-timeout PASS due=65 bounded=64+1 future=held timer-irq=unchanged\r\n");
                true
            }
            Case::Xsaveopt => fpu_opt::run(),
            Case::XstateContract => fpu_contract::run(),
            Case::FpuEnvironment => fpu_environment::run(),
            Case::FpuGuards => resources::work_fpu(),
            Case::FpuWork => {
                if !resources::work_fpu() {
                    return false;
                }
                if !resources::reserve_probe() {
                    return helpers::fail(b"fpu-reserve");
                }
                helpers::puts(b"TASKREG08201 fpu-reserve PASS admission=oom reuse=2 initial=clean guard-roundtrip=preserved\r\n");
                true
            }
            Case::All => run::run(),
        }
    }
}
