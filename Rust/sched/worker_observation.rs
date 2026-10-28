//! Explicit short diagnostic window at real owning execution edges. No task,
//! caller pointer, work lifetime or completion permission is retained here.
//! Default admission is a single false atomic read at each participating edge;
//! no background sampler, permanent timing instrumentation or per-byte hook.
use crate::{
    arch::{percpu, runtime::RuntimeGuard},
    boot::log::Line,
    sched::{kernel_check, scheduler, task::task_registry as registry},
};
use core::{
    cell::UnsafeCell,
    fmt::Write,
    sync::atomic::{AtomicBool, Ordering},
};

pub(crate) const DESKTOP: usize = 0;
pub(crate) const FILE: usize = 1;
pub(crate) const SERVICE: usize = 2;
pub(crate) const AUDIO: usize = 3;
const KINDS: usize = 4;
const RECEIPTS: usize = 16;
// The real screenshot owner can spend over 15s writing a 1280x720 BMP on
// supported ATA PIO. Observe its actual return; the default all/127 controller
// keeps its original deadline. No extra job is sampled by this expiry wait.
const WINDOW_TICKS: u64 = 30_000;
const DEFERRED_RECEIPTS: usize = 16;
#[derive(Clone, Copy)]
struct Deferred {
    task: u32,
    generation: u64,
    scheduled: u64,
    age_ticks: u64,
    rip: u64,
    cpu: u32,
    critical: bool,
    priority: u32,
    role: u32,
    name: [u8; 24],
}
#[derive(Clone, Copy)]
pub(crate) struct Stamp {
    sequence: u64,
    kind: usize,
    task: u32,
    generation: u64,
    owner: u32,
    owner_generation: u64,
    key: u64,
    start_cpu: u32,
}
#[derive(Clone, Copy)]
struct Receipt {
    stamp: Stamp,
    end_cpu: u32,
    result: i32,
}
#[derive(Clone, Copy)]
struct State {
    sequence: u64,
    entered: [u64; KINDS],
    returned: [u64; KINDS],
    successful: [u64; KINDS],
    pending: [u64; KINDS],
    begin_cpus: [u64; KINDS],
    end_cpus: [u64; KINDS],
    records: [[Option<Receipt>; RECEIPTS]; KINDS],
    invalid: u64,
    deferred: [Option<Deferred>; DEFERRED_RECEIPTS],
    deferred_tail: Option<Deferred>,
    deferred_last: Option<(u32, u64, u64, u64)>,
    deferred_overflow: u64,
}
impl State {
    const EMPTY: Self = Self {
        sequence: 0,
        entered: [0; KINDS],
        returned: [0; KINDS],
        successful: [0; KINDS],
        pending: [0; KINDS],
        begin_cpus: [0; KINDS],
        end_cpus: [0; KINDS],
        records: [[None; RECEIPTS]; KINDS],
        invalid: 0,
        deferred: [None; DEFERRED_RECEIPTS],
        deferred_tail: None,
        deferred_last: None,
        deferred_overflow: 0,
    };
}
struct Cell(UnsafeCell<State>);
// Short Runtime metadata admission only. The existing KernelCheck gate owns
// window reset/close; all old stamps reject its subsequent sequence.
unsafe impl Sync for Cell {}
static DATA: Cell = Cell(UnsafeCell::new(State::EMPTY));
static ACTIVE: AtomicBool = AtomicBool::new(false);

/// Only an actually eligible, deferred timer switch supplies a nonzero RIP.
/// Caller already holds the IRQ runtime owner. The original WORKERS window
/// is the sole opt-in; no clock, allocation, formatter or callback runs here.
pub(crate) fn deferred(rip: u64, ticks: u64, critical: bool) {
    if ticks < 250 || rip == 0 || !ACTIVE.load(Ordering::Acquire) {
        return;
    }
    let task = scheduler::current();
    if task.is_null() || percpu::current_index() != 0 {
        return;
    }
    unsafe {
        let core = registry::core(task.cast());
        if (*core).cpu_idle || (*core).generation == 0 {
            return;
        }
        let state = &mut *DATA.0.get();
        if let Some((id, generation, scheduled, age)) = state.deferred_last
            && id == (*core).id
            && generation == (*core).generation
            && scheduled == (*core).last_scheduled_tick
            && ticks < age.saturating_add(50)
        {
            return;
        }
        let mut row = Deferred {
            task: (*core).id,
            generation: (*core).generation,
            scheduled: (*core).last_scheduled_tick,
            age_ticks: ticks,
            rip,
            cpu: percpu::current_index(),
            critical,
            priority: (*core).priority as u32,
            role: (*core).role as u32,
            name: [0; 24],
        };
        let name = (*core).name;
        let n = name.len.min(row.name.len());
        if n != 0 {
            core::ptr::copy_nonoverlapping(name.pointer, row.name.as_mut_ptr(), n);
        }
        state.deferred_last = Some((row.task, row.generation, row.scheduled, row.age_ticks));
        if let Some(slot) = state.deferred.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(row);
        } else {
            state.deferred_overflow = state.deferred_overflow.saturating_add(1);
            if state
                .deferred_tail
                .is_none_or(|old| row.age_ticks > old.age_ticks)
            {
                state.deferred_tail = Some(row);
            }
        }
    }
}

pub(crate) fn begin(kind: usize, owner: u32, owner_generation: u64, key: u64) -> Option<Stamp> {
    if !ACTIVE.load(Ordering::Acquire) {
        return None;
    }
    let _runtime = RuntimeGuard::acquire();
    if !ACTIVE.load(Ordering::Relaxed) {
        return None;
    }
    let task = scheduler::current();
    unsafe {
        let state = &mut *DATA.0.get();
        if task.is_null() || kind >= KINDS || owner == 0 || owner_generation == 0 {
            state.invalid += 1;
            return None;
        }
        let core = registry::core(task.cast());
        let cpu = percpu::current_index();
        if !(*core).smp_eligible || (*core).running_cpu != cpu as u8 {
            state.invalid += 1;
            return None;
        }
        state.entered[kind] += 1;
        state.begin_cpus[kind] |= 1u64 << cpu;
        Some(Stamp {
            sequence: state.sequence,
            kind,
            task: (*core).id,
            generation: (*core).generation,
            owner,
            owner_generation,
            key,
            start_cpu: cpu,
        })
    }
}

pub(crate) fn finish(stamp: Option<Stamp>, result: i32, terminal: bool) {
    let Some(stamp) = stamp else {
        return;
    };
    if !ACTIVE.load(Ordering::Acquire) {
        return;
    }
    let _runtime = RuntimeGuard::acquire();
    if !ACTIVE.load(Ordering::Relaxed) {
        return;
    }
    unsafe {
        let state = &mut *DATA.0.get();
        if stamp.sequence != state.sequence {
            return;
        }
        let task = scheduler::current();
        if task.is_null() {
            state.invalid += 1;
            return;
        }
        let core = registry::core(task.cast());
        if (*core).id != stamp.task || (*core).generation != stamp.generation {
            state.invalid += 1;
            return;
        }
        if !terminal {
            state.pending[stamp.kind] += 1;
            return;
        }
        state.returned[stamp.kind] += 1;
        state.successful[stamp.kind] += u64::from(result >= 0);
        state.end_cpus[stamp.kind] |= 1u64 << percpu::current_index();
        if let Some(slot) = state.records[stamp.kind]
            .iter_mut()
            .find(|slot| slot.is_none())
        {
            *slot = Some(Receipt {
                stamp,
                end_cpu: percpu::current_index(),
                result,
            });
        }
    }
}

pub(crate) fn run() -> Option<bool> {
    let online = percpu::r4os_rust_percpu_control(14, 0, 0, 0);
    if online.count_ones() < 2 {
        return None;
    }
    let sequence = {
        let _runtime = RuntimeGuard::acquire();
        unsafe {
            let sequence = (*DATA.0.get()).sequence.checked_add(1)?;
            DATA.0.get().write(State {
                sequence,
                ..State::EMPTY
            });
            ACTIVE.store(true, Ordering::Release);
            sequence
        }
    };
    let mut line = Line::<128>::new();
    let _ = write!(
        line,
        "WORKER57 BEGIN window={} ticks={} online={:X}\r\n",
        sequence, WINDOW_TICKS, online
    );
    line.emit();
    let start = crate::platform::timer::tick_count();
    while !kernel_check::cancelled()
        && crate::platform::timer::tick_count().wrapping_sub(start) < WINDOW_TICKS
    {
        kernel_check::tasks::sleep(100, b"worker-observation");
    }
    let state = {
        let _runtime = RuntimeGuard::acquire();
        ACTIVE.store(false, Ordering::Release);
        unsafe { DATA.0.get().read() }
    };
    let names = [
        "desktop-compute",
        "async-file",
        "async-service",
        "audio-progress",
    ];
    let mut union = 0;
    let mut ok = !kernel_check::cancelled() && state.invalid == 0;
    for (kind, name) in names.iter().enumerate() {
        union |= state.begin_cpus[kind];
        ok &= state.begin_cpus[kind] & !1 != 0
            && state.successful[kind] != 0
            && (state.begin_cpus[kind] | state.end_cpus[kind]) & !online == 0;
        let mut line = Line::<384>::new();
        let _ = write!(
            line,
            "WORKER57 window={} kind={} begin={} returned={} ok={} pending={} begin-cpus={:X} end-cpus={:X} retained={}\r\n",
            sequence,
            name,
            state.entered[kind],
            state.returned[kind],
            state.successful[kind],
            state.pending[kind],
            state.begin_cpus[kind],
            state.end_cpus[kind],
            state.records[kind].iter().flatten().count()
        );
        line.emit();
        for r in state.records[kind].iter().flatten() {
            let s = r.stamp;
            let mut line = Line::<384>::new();
            let _ = write!(
                line,
                "WORKER57 receipt window={} kind={} task={}/{} owner={}/{} key={} cpus={}/{} result={}\r\n",
                sequence,
                name,
                s.task,
                s.generation,
                s.owner,
                s.owner_generation,
                s.key,
                s.start_cpu,
                r.end_cpu,
                r.result
            );
            line.emit();
        }
    }
    ok &= union.count_ones() >= 2;
    for (index, row) in state
        .deferred
        .into_iter()
        .chain([state.deferred_tail])
        .enumerate()
    {
        let Some(row) = row else { continue };
        let n = row
            .name
            .iter()
            .position(|b| *b == 0)
            .unwrap_or(row.name.len());
        let mut line = Line::<384>::new();
        let _ = write!(
            line,
            "DEFER59 window={} index={} task={}/{} name={} cpu={} scheduled={} age-ticks={} rip={:016X} critical={} priority={} role={} overflow={} scope=eligible-timer-defer-not-isolated-cpu-time\r\n",
            sequence,
            index,
            row.task,
            row.generation,
            core::str::from_utf8(&row.name[..n]).unwrap_or("?"),
            row.cpu,
            row.scheduled,
            row.age_ticks,
            row.rip,
            row.critical,
            row.priority,
            row.role,
            state.deferred_overflow
        );
        line.emit();
    }
    let mut line = Line::<192>::new();
    let _ = write!(
        line,
        "WORKER57 {} window={} invalid={} cpus={:X} timing=off receipts=bounded-original-returns\r\n",
        if ok { "PASS" } else { "FAILED" },
        sequence,
        state.invalid,
        union
    );
    line.emit();
    Some(ok)
}
