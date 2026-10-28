//! Explicit AP idle proof. Reads real IRQ/loan edges and creates one exact
//! pinned diagnostic worker; it never parks foreign work or caps capacity.
use super::{cancelled, tasks};
use crate::{
    arch::{lapic, percpu},
    platform::{cpu_topology, monotonic, timer},
    sched::{secondary_idle, task::task_types::Role},
};
use core::{
    fmt::Write,
    sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
};
static DONE: AtomicBool = AtomicBool::new(false);
static FAILED: AtomicBool = AtomicBool::new(false);
static TARGET: AtomicU32 = AtomicU32::new(0);
static ROUNDS: AtomicU32 = AtomicU32::new(0);
static ACTIVE_IRQS: AtomicU64 = AtomicU64::new(0);
static MAX_SLEEP: AtomicU64 = AtomicU64::new(0);
static EPOCH: AtomicU64 = AtomicU64::new(0);
#[derive(Clone, Copy)]
struct Reading {
    idle: secondary_idle::Sample,
    timer: (u64, u64, u64, u32, u32),
    coherent: bool,
    began_tick: u64,
    ended_tick: u64,
}
fn reading(cpu: u32) -> Option<Reading> {
    let began_tick = timer::tick_count();
    let idle = secondary_idle::sample(cpu)?;
    let timer = lapic::secondary_timer_sample(cpu)?;
    // A dispatch/re-arm can occur between these independent copied counters.
    // Never attribute a later active timer to an earlier idle receipt.
    let coherent = secondary_idle::sample(cpu).is_some_and(|after| after == idle);
    Some(Reading {
        idle,
        timer,
        coherent,
        began_tick,
        ended_tick: timer::tick_count(),
    })
}
impl Reading {
    fn quiet(self) -> bool {
        self.coherent
            && self.idle.paused
            && !self.idle.retrying
            && self.timer.0 != 0
            && self.idle.epoch == self.timer.0
    }
    fn same_quiet_loan(self, first: Self) -> bool {
        self.quiet()
            && first.quiet()
            && self.idle.entries == first.idle.entries
            && self.idle.returns == first.idle.returns
            && self.idle.retry_arms == first.idle.retry_arms
            && self.timer.0 == first.timer.0
            && self.timer.3 == first.timer.3
    }
}
#[derive(Clone, Copy, Default)]
struct Quiet {
    first: Option<Reading>,
    anchor: Option<Reading>,
    last: Option<Reading>,
    ticks: u64,
    longest: u64,
    confirmed: u64,
    spans: u64,
    maximum_irqs: u64,
    excess: bool,
}
impl Quiet {
    fn observe(&mut self, next: Reading, minimum: u64) {
        self.first.get_or_insert(next);
        self.last = Some(next);
        if let Some(anchor) = self.anchor
            && next.same_quiet_loan(anchor)
        {
            // Lower bound: no preemption during either copied snapshot can
            // extend the interval being claimed as actual observed idle.
            let span = next.began_tick.saturating_sub(anchor.ended_tick);
            if span >= minimum {
                let delta = next
                    .timer
                    .1
                    .wrapping_sub(anchor.timer.1)
                    .wrapping_add(next.timer.2.wrapping_sub(anchor.timer.2));
                self.maximum_irqs = self.maximum_irqs.max(delta);
                self.excess |= delta > 1;
                self.longest = self.longest.max(span);
                self.ticks = self.ticks.saturating_add(span - self.confirmed);
                if self.confirmed == 0 {
                    self.spans += 1;
                }
                self.confirmed = span;
            }
        } else {
            self.anchor = next.quiet().then_some(next);
            self.confirmed = 0;
        }
    }
}
fn fail() {
    FAILED.store(true, Ordering::Release);
    DONE.store(true, Ordering::Release);
}
unsafe extern "C" fn worker() {
    let cpu = percpu::current_index();
    let epoch = cpu_topology::epoch(cpu);
    if cpu != TARGET.load(Ordering::Acquire)
        || epoch != EPOCH.load(Ordering::Acquire)
        || secondary_idle::sample(cpu).is_none_or(|r| r.paused)
    {
        fail();
        return;
    }
    let Some((e, before, _, _, _)) = lapic::secondary_timer_sample(cpu) else {
        fail();
        return;
    };
    let Some(start) = monotonic::now_nanoseconds() else {
        fail();
        return;
    };
    loop {
        let Some(now) = monotonic::now_nanoseconds() else {
            fail();
            return;
        };
        if now.saturating_sub(start) >= 20_000_000 {
            break;
        }
        if cancelled() {
            fail();
            return;
        }
        core::hint::spin_loop();
    }
    let Some((e2, after, _, _, _)) = lapic::secondary_timer_sample(cpu) else {
        fail();
        return;
    };
    if e != epoch || e2 != epoch || after <= before {
        fail();
        return;
    }
    ACTIVE_IRQS.store(after - before, Ordering::Release);
    for round in 1..=16 {
        let before = timer::tick_count();
        tasks::sleep(2, b"ap-idle-earlier-deadline");
        let elapsed = timer::tick_count().saturating_sub(before);
        if cancelled()
            || percpu::current_index() != cpu
            || cpu_topology::epoch(cpu) != epoch
            || elapsed < 2
            || secondary_idle::sample(cpu).is_none_or(|r| r.paused)
        {
            fail();
            return;
        }
        MAX_SLEEP.fetch_max(elapsed, Ordering::Relaxed);
        ROUNDS.store(round, Ordering::Release);
    }
    DONE.store(true, Ordering::Release);
}
pub(super) fn run() -> Option<bool> {
    let Some(_observation) = secondary_idle::Observation::begin() else {
        return Some(false);
    };
    tasks::sleep(100, b"ap-idle-settle");
    let online = (1..64u32)
        .filter(|&cpu| percpu::is_schedulable(cpu))
        .fold(0u64, |mask, cpu| mask | (1u64 << cpu));
    if online == 0 {
        return None;
    }
    let hz = u64::from(timer::frequency()).max(1);
    let begin_tick = timer::tick_count();
    let end_tick = begin_tick.saturating_add((hz / 2).max(1));
    let minimum = (hz / 20).max(2); // At least50ms: a 1kHz storm cannot pass.
    let step = (hz / 100).max(1);
    let mut measured = [Quiet::default(); 64];
    loop {
        for cpu in 1..64u32 {
            if online & (1u64 << cpu) != 0
                && let Some(next) = reading(cpu)
            {
                measured[cpu as usize].observe(next, minimum);
            }
        }
        let now = timer::tick_count();
        if now >= end_tick || cancelled() {
            break;
        }
        tasks::sleep(step.min(end_tick - now), b"ap-idle-observed-window");
    }
    let elapsed = timer::tick_count().saturating_sub(begin_tick);
    let mut candidate = None;
    let mut quiet = 0u32;
    let mut retained = 0u32;
    let mut excess_irqs = false;
    for cpu in 1..64u32 {
        let m = measured[cpu as usize];
        let (Some(first), Some(last)) = (m.first, m.last) else {
            continue;
        };
        let b = first.idle;
        let a = last.idle;
        let (epoch, irqs, stale, _, _) = first.timer;
        let (_, end_irqs, end_stale, generation, mode) = last.timer;
        let qualified = m.longest >= minimum;
        let delta = end_irqs
            .wrapping_sub(irqs)
            .wrapping_add(end_stale.wrapping_sub(stale));
        let mut line = crate::boot::log::Line::<768>::new();
        write!(&mut line,"[AP-IDLE58] cpu={} epoch={} generation={} mode={} nominal_hz={} observed_ticks={} irq_before={} irq_after={} stale_before={} stale_after={} actual_delta={} quiet_ticks={} longest_quiet={} quiet_spans={} max_quiet_irq_delta={} qualified={} coherent={}/{} retry_arms={}/{} entries={}/{} returns={}/{}\r\n",cpu,epoch,generation,mode,timer::frequency(),elapsed,irqs,end_irqs,stale,end_stale,delta,m.ticks,m.longest,m.spans,m.maximum_irqs,u8::from(qualified),u8::from(first.coherent),u8::from(last.coherent),b.retry_arms,a.retry_arms,b.entries,a.entries,b.returns,a.returns).expect("bounded idle receipt");
        crate::boot::log::text(line.as_bytes());
        let mut line = crate::boot::log::Line::<256>::new();
        write!(
            &mut line,
            "[AP-IDLE65] cpu={} epoch={} retained_empty_prepares={}/{} qualified={}\r\n",
            cpu,
            epoch,
            b.retained_empty,
            a.retained_empty,
            u8::from(qualified)
        )
        .expect("bounded retained idle receipt");
        crate::boot::log::text(line.as_bytes());
        excess_irqs |= m.excess;
        if qualified {
            quiet += 1;
            if a.retained_empty != 0 {
                retained += 1;
            }
            if candidate.is_none() && last.quiet() {
                candidate = Some((cpu, last.timer.0));
            }
        }
    }
    // One queued old vector is allowed per unchanged quiet loan, NOT per
    // copied sample. The anchor survives all samples until a real dispatch,
    // retry-arm or clock/CPU epoch transition; a slow periodic storm also fails.
    if excess_irqs {
        crate::boot::log::text(b"[AP-IDLE58] result=FAIL reason=quiet-irq-bound\r\n");
        return Some(false);
    }
    if retained == 0 {
        crate::boot::log::text(b"[AP-IDLE65] result=FAIL reason=no-retained-empty-loan\r\n");
        return Some(false);
    }
    let Some((cpu, epoch)) = candidate else {
        crate::boot::log::text(b"[AP-IDLE58] result=FAIL reason=no-observed-quiet-target\r\n");
        return Some(false);
    };
    DONE.store(false, Ordering::Release);
    FAILED.store(false, Ordering::Release);
    TARGET.store(cpu, Ordering::Release);
    EPOCH.store(epoch, Ordering::Release);
    ROUNDS.store(0, Ordering::Release);
    ACTIVE_IRQS.store(0, Ordering::Release);
    MAX_SLEEP.store(0, Ordering::Release);
    let t = tasks::create_on_cpu(b"spectra-ap-idle", worker, Role::Interactive, cpu);
    if t.is_null() {
        return Some(false);
    }
    if !tasks::start(t) {
        let _ = tasks::finish_task(t, true);
        return Some(false);
    }
    let end = timer::tick_count().saturating_add(u64::from(timer::frequency()).max(1) * 5);
    while !DONE.load(Ordering::Acquire) && !cancelled() && timer::tick_count() < end {
        tasks::sleep(1, b"ap-idle-owned-worker");
    }
    let done = DONE.load(Ordering::Acquire);
    let reaped = tasks::finish_task(t, !done);
    let result = done
        && !FAILED.load(Ordering::Acquire)
        && reaped
        && ROUNDS.load(Ordering::Acquire) == 16
        && ACTIVE_IRQS.load(Ordering::Acquire) > 0;
    let mut line = crate::boot::log::Line::<256>::new();
    write!(&mut line,"[AP-IDLE58] result={} quiet_cpus={} target={} capacity={} active_irq_delta={} timed_wakes={} max_sleep_ticks={} reaped={}\r\n",if result {"PASS"} else {"FAIL"},quiet,cpu,online.count_ones()+1,ACTIVE_IRQS.load(Ordering::Acquire),ROUNDS.load(Ordering::Acquire),MAX_SLEEP.load(Ordering::Acquire),u8::from(reaped)).expect("bounded idle result");
    crate::boot::log::text(line.as_bytes());
    Some(result)
}
