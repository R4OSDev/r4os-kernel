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

// Explicit-session evidence only. The Ready owner retains every descriptor
// while Runtime is held; retain values, never foreign task pointers, afterwards.
#[derive(Clone, Copy)]
struct ReadyEntry {
    id: u32,
    generation: u64,
    owner: u8,
    context: usize,
    state: u8,
    role: u8,
    home: u8,
    running: u8,
    linked: bool,
    idle: bool,
    own: u8,
}
impl ReadyEntry {
    const EMPTY: Self = Self {
        id: 0,
        generation: 0,
        owner: 0,
        context: 0,
        state: 0,
        role: 0,
        home: 0,
        running: 0,
        linked: false,
        idle: false,
        own: 0,
    };
}
#[derive(Clone, Copy)]
struct ReadyEvidence {
    count: u64,
    observed: usize,
    complete: bool,
    projection_valid: bool,
    entries: [ReadyEntry; 8],
}
impl ReadyEvidence {
    const EMPTY: Self = Self {
        count: 0,
        observed: 0,
        complete: true,
        projection_valid: true,
        entries: [ReadyEntry::EMPTY; 8],
    };
}
// Caller holds Runtime for the entire traversal, including the real count.
unsafe fn ready_evidence(cpu: u32, a: *mut Descriptor, b: *mut Descriptor) -> ReadyEvidence {
    unsafe {
        let mut out = ReadyEvidence::EMPTY;
        out.count = query(3, ptr::null_mut(), u64::from(cpu));
        let mut next = task_query(4, u64::from(cpu)).cast::<Descriptor>();
        let mut previous = ptr::null_mut();
        while !next.is_null() && out.observed < out.entries.len() {
            let c = registry::core(next);
            let entry = ReadyEntry {
                id: (*c).id,
                generation: (*c).generation,
                owner: (*c).execution_owner_kind as u8,
                context: (*c).execution_owner_context.addr(),
                state: (*c).state as u8,
                role: (*c).role as u8,
                home: (*c).home_cpu,
                running: (*c).running_cpu,
                linked: (*c).ready_linked,
                idle: (*c).cpu_idle,
                own: if next == a {
                    1
                } else if next == b {
                    2
                } else {
                    0
                },
            };
            out.projection_valid &= (*c).ready_prev == previous
                && entry.id != 0
                && entry.generation != 0
                && entry.linked
                && !entry.idle
                && entry.state == TaskState::Ready as u8
                && u32::from(entry.home) == cpu
                && entry.running == 0xff;
            out.entries[out.observed] = entry;
            out.observed += 1;
            previous = next;
            next = (*c).ready_next;
        }
        out.complete = next.is_null();
        out.projection_valid &= out.complete && out.count == out.observed as u64;
        out
    }
}
fn log_ready_evidence(mode: u32, cpu: u32, evidence: ReadyEvidence) {
    crate::boot::log_route::puts(b"SLICE08894 admission");
    for (label, value) in [
        (b" mode=".as_slice(), u64::from(mode)),
        (b" cpu=", u64::from(cpu)),
        (b" count=", evidence.count),
        (b" observed=", evidence.observed as u64),
        (b" complete=", u64::from(evidence.complete)),
        (b" projection=", u64::from(evidence.projection_valid)),
    ] {
        crate::boot::log_route::puts(label);
        crate::boot::log_route::dec(value);
    }
    crate::boot::log_route::puts(b"\n");
    for entry in &evidence.entries[..evidence.observed] {
        // Split lines retain complete 64-bit identities through LogService's
        // existing record bound. Context is opaque evidence, not owner truth.
        crate::boot::log_route::puts(b"SLICE08894 ready");
        for (label, value) in [
            (b" id=".as_slice(), u64::from(entry.id)),
            (b" gen=", entry.generation),
            (b" owner=", u64::from(entry.owner)),
            (b" context=", entry.context as u64),
        ] {
            crate::boot::log_route::puts(label);
            crate::boot::log_route::dec(value);
        }
        crate::boot::log_route::puts(b"\nSLICE08894 state");
        for (label, value) in [
            (b" id=".as_slice(), u64::from(entry.id)),
            (b" gen=", entry.generation),
            (b" state=", u64::from(entry.state)),
            (b" home=", u64::from(entry.home)),
            (b" role=", u64::from(entry.role)),
            (b" running=", u64::from(entry.running)),
            (b" linked=", u64::from(entry.linked)),
            (b" idle=", u64::from(entry.idle)),
            (b" own=", u64::from(entry.own)),
        ] {
            crate::boot::log_route::puts(label);
            crate::boot::log_route::dec(value);
        }
        crate::boot::log_route::puts(b"\n");
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Attempt {
    Passed,
    Unavailable,
    Failed,
}
unsafe extern "C" fn arrival() {}

fn one(mode: u32, controlled_arrival: bool) -> Attempt {
    let Some(cpu) = rank_probe::quiet_cpu() else {
        crate::boot::log_route::puts(b"SLICE08894 UNAVAILABLE no-quiet-cpu\n");
        return Attempt::Unavailable;
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
    let extra = if controlled_arrival {
        tasks::create_on_cpu(b"spectra-slice-arrival", arrival, Role::Batch, cpu)
    } else {
        ptr::null_mut()
    };
    ACTORS[0].store(a, Ordering::Release);
    ACTORS[1].store(b, Ordering::Release);
    let created = !a.is_null() && !b.is_null() && (!controlled_arrival || !extra.is_null());
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
    let mut admission_mask = 0;
    let mut queued_before = 0;
    let mut queue_evidence = ReadyEvidence::EMPTY;
    let mut extra_identity = (0, 0);
    let mut extra_published = false;
    let admitted = if started && ARMED.load(Ordering::Acquire) {
        let _runtime = runtime::RuntimeGuard::acquire();
        unsafe {
            if controlled_arrival {
                // The real owner publishes this session's third task after
                // the quiet snapshot. It is never a foreign queue reset.
                extra_identity = (
                    (*registry::core(extra)).id,
                    (*registry::core(extra)).generation,
                );
                extra_published = tasks::start(extra);
            }
            queued_before = query(3, ptr::null_mut(), u64::from(cpu));
            if queued_before != 0 {
                queue_evidence = ready_evidence(cpu, a, b);
            }
            admission_mask = u32::from(cpu_state(cpu).current == a.cast())
                | (u32::from((*registry::core(a)).state == TaskState::Running) << 1)
                | (u32::from((*registry::core(a)).role == role) << 2)
                | (u32::from(queued_before == 0) << 3);
            let exact = admission_mask == 15;
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
    if queued_before != 0 {
        log_ready_evidence(mode, cpu, queue_evidence);
    }
    if controlled_arrival {
        crate::boot::log_route::puts(b"SLICE08894 injection id=");
        crate::boot::log_route::dec(u64::from(extra_identity.0));
        crate::boot::log_route::puts(b" gen=");
        crate::boot::log_route::dec(extra_identity.1);
        crate::boot::log_route::puts(b" published=");
        crate::boot::log_route::dec(u64::from(extra_published));
        crate::boot::log_route::puts(b"\n");
    }
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
    for t in [a, b, extra] {
        if !t.is_null() {
            clean &= tasks::finish_task(t, !passed);
        }
    }
    // A valid later Ready publication explains only non-admission. It never
    // relaxes an admitted workload's checksum/role/quantum/FPU requirements.
    let contention = !admitted
        && admission_mask == 7
        && queue_evidence.projection_valid
        && queue_evidence.complete
        && queue_evidence.count != 0
        && queue_evidence.entries[..queue_evidence.observed]
            .iter()
            .all(|e| e.own == 0);
    let injected = extra_published
        && extra_identity.0 != 0
        && extra_identity.1 != 0
        && queue_evidence.entries[..queue_evidence.observed]
            .iter()
            .filter(|e| (e.id, e.generation) == extra_identity)
            .count()
            == 1;
    let result = if controlled_arrival {
        if contention && injected && clean {
            Attempt::Passed
        } else {
            Attempt::Failed
        }
    } else if contention && clean {
        Attempt::Unavailable
    } else if passed && clean {
        Attempt::Passed
    } else {
        Attempt::Failed
    };
    ACTORS[0].store(ptr::null_mut(), Ordering::Release);
    ACTORS[1].store(ptr::null_mut(), Ordering::Release);
    if controlled_arrival {
        crate::boot::log_route::puts(b"SLICE08894 CONTROL ");
    } else {
        crate::boot::log_route::puts(b"SLICE08732 ");
    }
    crate::boot::log_route::puts(match result {
        Attempt::Passed => b"PASS",
        Attempt::Unavailable => b"UNAVAILABLE",
        Attempt::Failed => b"FAIL",
    });
    for (label, value) in [
        (b" mode=".as_slice(), u64::from(mode)),
        (b" admission-mask=", u64::from(admission_mask)),
        (b" queued-before=", queued_before),
        (b" quantum=", quantum),
        (b" ready_ticks=", latency),
        (b" elapsed_ns=", elapsed),
        (b" work=", if admitted { 2 * BLOCK * BLOCKS } else { 0 }),
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
    result
}
pub(crate) fn run() -> bool {
    if percpu::r4os_rust_percpu_control(14, 0, 0, 0) & !1 == 0 {
        return true;
    }
    if one(0, true) != Attempt::Passed {
        return false;
    }
    // Keep complete session history within the existing 208-record ledger:
    // three control actors and at most two additional actor pairs. No task
    // capacity change or loss of earlier retirement receipts is needed.
    let mut retries = 2;
    for mode in 0..3 {
        let mut qualified = false;
        for _ in 0..3 {
            match one(mode, false) {
                Attempt::Passed => {
                    qualified = true;
                    break;
                }
                Attempt::Unavailable => {
                    if retries == 0 {
                        break;
                    }
                    retries -= 1;
                }
                Attempt::Failed => return false,
            }
        }
        if !qualified {
            crate::boot::log_route::puts(b"SLICE08894 UNAVAILABLE admission-budget-exhausted\n");
            return false;
        }
    }
    true
}
