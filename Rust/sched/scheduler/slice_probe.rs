//! Fixed work in the existing explicit scheduler/fairness check. Two exact
//! AP actors expose cooperative first dispatch and actual switch costs. Native
//! kernel instructions retain their normal IRQ deferral; real module Timer/
//! IPI preemption stays covered by the original boot acceptance. No hot
//! path override, synthetic clock, foreign CPU state or benchmark suite.
use super::*;
use crate::sched::{
    kernel_check::{self, tasks},
    task::{
        task_registry as registry,
        task_types::{Role, State as TaskState, Task as Descriptor},
    },
    task_context::Admission,
};
use core::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32};

const BLOCK: u64 = 8192;
const BLOCKS: u64 = 8192;
static ACTORS: [AtomicPtr<Descriptor>; 2] = [const { AtomicPtr::new(ptr::null_mut()) }; 2];
static ARMED: AtomicBool = AtomicBool::new(false);
static GO: AtomicBool = AtomicBool::new(false);
static STOP: AtomicBool = AtomicBool::new(false);
static END: AtomicU64 = AtomicU64::new(0);
static DONE: AtomicU32 = AtomicU32::new(0);
static FAILED: AtomicBool = AtomicBool::new(false);
static WAKE_TICK: AtomicU64 = AtomicU64::new(0);
static LATENCY: AtomicU64 = AtomicU64::new(0);
static CHECKSUMS: [AtomicU64; 2] = [const { AtomicU64::new(0) }; 2];
static FINISHED: [AtomicU64; 2] = [const { AtomicU64::new(0) }; 2];

fn active() -> bool {
    !STOP.load(Ordering::Acquire) && timer::tick_count() < END.load(Ordering::Relaxed)
}
fn clock() -> u64 {
    crate::platform::clocksource::now_nanoseconds().unwrap_or(0)
}
unsafe extern "C" fn actor() {
    let Some(_admission) = Admission::acquire(true) else {
        FAILED.store(true, Ordering::Release);
        return;
    };
    let me = current().cast::<Descriptor>();
    let Some(index) = ACTORS.iter().position(|p| p.load(Ordering::Acquire) == me) else {
        FAILED.store(true, Ordering::Release);
        return;
    };
    if index == 0 {
        ARMED.store(true, Ordering::Release);
    }
    while !GO.load(Ordering::Acquire) && active() {
        core::hint::spin_loop();
    }
    if index == 1 {
        LATENCY.store(
            timer::tick_count().saturating_sub(WAKE_TICK.load(Ordering::Acquire)),
            Ordering::Release,
        );
    }
    let mut value = 0x8713_2871_3287_1328u64;
    for _ in 0..BLOCKS {
        if !active() {
            FAILED.store(true, Ordering::Release);
            break;
        }
        for _ in 0..BLOCK {
            value =
                value.wrapping_mul(1_664_525).wrapping_add(1_013_904_223) ^ value.rotate_left(13);
        }
        value = core::hint::black_box(value);
        safe_reschedule();
    }
    CHECKSUMS[index].store(value, Ordering::Relaxed);
    FINISHED[index].store(clock(), Ordering::Relaxed);
    DONE.fetch_or(1 << index, Ordering::Release);
}

fn one(mode: u32) -> bool {
    let Some(cpu) = rank_probe::quiet_cpu() else {
        return false;
    };
    for flag in [&ARMED, &GO, &STOP, &FAILED] {
        flag.store(false, Ordering::Relaxed);
    }
    DONE.store(0, Ordering::Relaxed);
    LATENCY.store(u64::MAX, Ordering::Relaxed);
    END.store(timer::tick_count().saturating_add(5000), Ordering::Relaxed);
    for i in 0..2 {
        CHECKSUMS[i].store(0, Ordering::Relaxed);
        FINISHED[i].store(0, Ordering::Relaxed);
    }
    let role = match mode {
        1 => Role::ShortCompletion,
        2 => Role::Batch,
        _ => Role::Interactive,
    };
    let rank = if mode == 2 { 3 } else { 2 };
    let quantum = dispatch_quantum(rank);
    let a = tasks::create_on_cpu(b"spectra-slice-running", actor, role, cpu);
    let b = tasks::create_on_cpu(b"spectra-slice-ready", actor, role, cpu);
    ACTORS[0].store(a, Ordering::Release);
    ACTORS[1].store(b, Ordering::Release);
    let created = !a.is_null() && !b.is_null();
    let started = created && tasks::start(a);
    while started && !ARMED.load(Ordering::Acquire) && active() && !kernel_check::cancelled() {
        tasks::sleep(1, b"spectra-slice-start");
    }
    let fp_before = fpu::snapshot();
    let counts_before = {
        let _runtime = runtime::RuntimeGuard::acquire();
        (
            read(|s| s.timer_preemption_switches_by_cpu[cpu as usize]),
            COOPERATION.switches.load(Ordering::Relaxed),
        )
    };
    let start_ns = clock();
    let admitted = if started && ARMED.load(Ordering::Acquire) {
        let _runtime = runtime::RuntimeGuard::acquire();
        unsafe {
            let exact = cpu_state(cpu).current == a.cast()
                && (*registry::core(a)).state == TaskState::Running
                && (*registry::core(a)).role == role
                && query(3, ptr::null_mut(), u64::from(cpu)) == 0;
            if exact && tasks::start(b) {
                WAKE_TICK.store(timer::tick_count(), Ordering::Relaxed);
                GO.store(true, Ordering::Release);
                true
            } else {
                false
            }
        }
    } else {
        false
    };
    while admitted && DONE.load(Ordering::Acquire) != 3 && active() && !kernel_check::cancelled() {
        tasks::sleep(1, b"spectra-slice-join");
    }
    STOP.store(true, Ordering::Release);
    let fp_after = fpu::snapshot();
    let (timer_switches, safe_switches, switches) = {
        let _runtime = runtime::RuntimeGuard::acquire();
        (
            read(|s| s.timer_preemption_switches_by_cpu[cpu as usize]) - counts_before.0,
            COOPERATION.switches.load(Ordering::Relaxed) - counts_before.1,
            if created {
                unsafe { (*registry::core(a)).switches_in + (*registry::core(b)).switches_in }
            } else {
                0
            },
        )
    };
    let latency = LATENCY.load(Ordering::Acquire);
    let checksums = CHECKSUMS.each_ref().map(|v| v.load(Ordering::Relaxed));
    let elapsed = FINISHED
        .iter()
        .map(|v| v.load(Ordering::Relaxed))
        .max()
        .unwrap_or(0)
        .saturating_sub(start_ns);
    let passed = admitted
        && DONE.load(Ordering::Acquire) == 3
        && !FAILED.load(Ordering::Acquire)
        && checksums[0] != 0
        && checksums[0] == checksums[1]
        && start_ns != 0
        && elapsed != 0
        // An observed IRQ/host delay remains visible. This isolated check's
        // tolerance is not a physical response-time guarantee.
        && latency <= quantum + 4
        && switches > 2
        && safe_switches != 0;
    let mut clean = true;
    for t in [a, b] {
        if !t.is_null() {
            clean &= tasks::finish_task(t, !passed);
        }
    }
    crate::boot::log_route::puts(b"SLICE08732 ");
    crate::boot::log_route::puts(if passed && clean { b"PASS" } else { b"FAIL" });
    for (label, value) in [
        (b" mode=".as_slice(), u64::from(mode)),
        (b" quantum=", quantum),
        (b" ready_ticks=", latency),
        (b" elapsed_ns=", elapsed),
        (b" work=", 2 * BLOCK * BLOCKS),
        (b" switches=", switches),
        (b" timer=", timer_switches),
        (b" safe=", safe_switches),
        (
            b" global_fpu_saves=",
            fp_after.save_count - fp_before.save_count,
        ),
        (
            b" global_fpu_restores=",
            fp_after.restore_count - fp_before.restore_count,
        ),
        (b" checksum=", checksums[0]),
    ] {
        crate::boot::log_route::puts(label);
        crate::boot::log_route::dec(value);
    }
    crate::boot::log_route::puts(if clean {
        b" own=reaped\n"
    } else {
        b" own=retained\n"
    });
    passed && clean
}
pub(crate) fn run() -> bool {
    if percpu::r4os_rust_percpu_control(14, 0, 0, 0) & !1 == 0 {
        return true;
    }
    (0..3).all(one)
}
