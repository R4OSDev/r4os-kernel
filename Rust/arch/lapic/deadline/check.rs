//! Isolated existing TASKREGISTRY boot selector. No public/live diagnostic
//! changes the global timer or injects a source fallback.
use super::*;
use crate::{arch::lapic, platform::monotonic, sched::scheduler};
use core::sync::atomic::AtomicBool;
static STAGE: AtomicU32 = AtomicU32::new(0);
// Active AP witnesses keep the original peer-IRQ and clock-demotion oracles
// meaningful after legitimate idle timers stop. Exact diagnostic placement
// affects these owned Tasks only; no CPU is parked or removed from capacity.
static PEER_STOP: AtomicBool = AtomicBool::new(false);
static PEER_STARTED: AtomicU64 = AtomicU64::new(0);
struct Peers {
    ledger: crate::sched::task::diagnostic::Ledger<MAX_CPUS>,
    tasks: [*mut crate::sched::task::task_types::Task; MAX_CPUS],
}
unsafe extern "C" fn peer() {
    PEER_STARTED.fetch_or(1u64 << percpu::current_index(), Ordering::Release);
    while !PEER_STOP.load(Ordering::Acquire) {
        core::hint::spin_loop();
    }
}
impl Peers {
    fn start() -> Option<Self> {
        use crate::sched::task::{diagnostic, task_types::Role};
        PEER_STOP.store(false, Ordering::Release);
        PEER_STARTED.store(0, Ordering::Release);
        let mut out = Self {
            ledger: diagnostic::Ledger::EMPTY,
            tasks: [core::ptr::null_mut(); MAX_CPUS],
        };
        let mut mask = 0u64;
        for cpu in 1..cpu_topology::count() {
            if !percpu::is_schedulable(cpu) {
                continue;
            }
            let task = out
                .ledger
                .create(b"deadline-ap-witness", peer, Role::Batch, Some(cpu));
            out.tasks[cpu as usize] = task;
            if task.is_null() || !diagnostic::start(task) {
                return None;
            }
            mask |= 1u64 << cpu;
        }
        until(|| PEER_STARTED.load(Ordering::Acquire) == mask).then_some(out)
    }
}
impl Drop for Peers {
    fn drop(&mut self) {
        PEER_STOP.store(true, Ordering::Release);
        for (cpu, task) in self.tasks.into_iter().enumerate() {
            if !task.is_null() {
                let kill = PEER_STARTED.load(Ordering::Acquire) & (1u64 << cpu) == 0;
                assert!(
                    self.ledger.finish_task(task, kill),
                    "exact AP timer witness retirement"
                );
            }
        }
    }
}

struct Probe(u64);
impl Probe {
    fn acquire() -> Option<Self> {
        let flags = runtime::save_local();
        if flags & (1 << 9) == 0 || percpu::current_index() != 0 {
            runtime::restore_local(flags);
            return None;
        }
        scheduler::r4os_rust_scheduler_control(6, 0, 0);
        runtime::restore_local(flags);
        Some(Self(flags))
    }
}
impl Drop for Probe {
    fn drop(&mut self) {
        let _flags = runtime::save_local();
        lapic::r4os_rust_lapic_control(16, 0, 0);
        scheduler::r4os_rust_scheduler_control(7, 0, 0);
        runtime::restore_local(self.0);
    }
}
// Release BSP preemption admission before joining or creating witnesses.
// Every early return restores that admission before any resident ledger drops.
struct Session {
    probe: Option<Probe>,
    peers: Option<Peers>,
}
impl Session {
    fn start() -> Option<Self> {
        let mut out = Self {
            probe: None,
            peers: Peers::start(),
        };
        out.peers.as_ref()?;
        out.probe = Probe::acquire();
        out.probe.as_ref()?;
        Some(out)
    }
    fn idle_peers(&mut self) -> bool {
        drop(self.probe.take());
        drop(self.peers.take());
        self.probe = Probe::acquire();
        self.probe.is_some()
    }
    fn wake_peers(&mut self) -> bool {
        drop(self.probe.take());
        self.peers = Peers::start();
        self.probe = Probe::acquire();
        self.peers.is_some() && self.probe.is_some()
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        drop(self.probe.take());
        drop(self.peers.take());
    }
}
fn until(mut done: impl FnMut() -> bool) -> bool {
    let Some(start) = monotonic::now_nanoseconds() else {
        return false;
    };
    for _ in 0..5_000_000 {
        if done() {
            return true;
        }
        if monotonic::now_nanoseconds().is_none_or(|now| now.saturating_sub(start) > 200_000_000) {
            return false;
        }
        core::hint::spin_loop();
    }
    false
}
fn run_cases() -> bool {
    STAGE.store(1, Ordering::Release);
    let Some(mut session) = Session::start() else {
        return false;
    };
    let t = local();
    let mut peers = [0u64; MAX_CPUS];
    let count = cpu_topology::count();
    for cpu in 1..count {
        if percpu::is_schedulable(cpu) {
            if !active(cpu) {
                return false;
            }
            peers[cpu as usize] = TIMERS[cpu as usize].irqs.load(Ordering::Acquire);
        }
    }
    STAGE.store(2, Ordering::Release);
    if !one_shot() || arm(u64::MAX) != Some(u64::from(t.hz.load(Ordering::Acquire)) * 60) {
        return false;
    }
    let far = t.next.load(Ordering::Acquire);
    let readback = {
        let _local = Local::acquire();
        unsafe { safe_msr::read_exact(0, t.epoch.load(Ordering::Acquire), MSR) }
    };
    if readback != Ok(far) || read(LVT) != (VECTOR | DEADLINE) {
        return false;
    }
    let stale = t.stale.load(Ordering::Acquire);
    STAGE.store(3, Ordering::Release);
    let irqs = t.irqs.load(Ordering::Acquire);
    let sent = {
        let _local = Local::acquire();
        lapic::bsp_apic_id()
            .is_some_and(|id| lapic::r4os_rust_lapic_control(5, u64::from(id), VECTOR) != 0)
    };
    if !sent
        || !until(|| t.stale.load(Ordering::Acquire) > stale)
        || t.next.load(Ordering::Acquire) != far
        || t.irqs.load(Ordering::Acquire) != irqs
    {
        return false;
    }
    STAGE.store(4, Ordering::Release);
    if arm(0) != Some(1)
        || t.next.load(Ordering::Acquire) >= far
        || !until(|| t.irqs.load(Ordering::Acquire) > irqs)
    {
        return false;
    }
    let irqs = t.irqs.load(Ordering::Acquire);
    STAGE.store(5, Ordering::Release);
    // Deliberately expired target on our own timer: the actual hardware must
    // fire once; never write TSC itself or impersonate an IRQ completion.
    {
        let _local = Local::acquire();
        let past = super::super::read_tsc().saturating_sub(1).max(1);
        if !program(t, past) {
            return false;
        }
    }
    if !until(|| t.irqs.load(Ordering::Acquire) > irqs) {
        return false;
    }
    if !disarm() || t.next.load(Ordering::Acquire) != 0 || !resume_periodic() {
        return false;
    }
    STAGE.store(6, Ordering::Release);
    if !until(|| {
        (1..count).all(|cpu| {
            !percpu::is_schedulable(cpu)
                || TIMERS[cpu as usize].irqs.load(Ordering::Acquire) > peers[cpu as usize]
        })
    }) {
        return false;
    }
    // Prove the source generation can change while every AP is physically
    // idle/disarmed. The subsequent real Ready publication must wake each AP
    // and restore its calibrated count timer without periodic AP polling.
    if !session.idle_peers()
        || !until(|| {
            (1..count).all(|cpu| {
                !percpu::is_schedulable(cpu)
                    || TIMERS[cpu as usize].mode.load(Ordering::Acquire) == PAUSED
            })
        })
    {
        return false;
    }
    let generation = clocksource::generation();
    STAGE.store(7, Ordering::Release);
    if !clocksource::deadline_boot_fallback() || clocksource::generation() == generation {
        return false;
    }
    revalidate();
    if !session.wake_peers() {
        return false;
    }
    if !until(|| {
        (0..count).all(|cpu| {
            !percpu::is_schedulable(cpu)
                || TIMERS[cpu as usize].mode.load(Ordering::Acquire) == COUNT
        })
    }) {
        return false;
    }
    if read(LVT) != (VECTOR | PERIODIC) || read(INITIAL) != t.initial.load(Ordering::Acquire) {
        return false;
    }
    STAGE.store(8, Ordering::Release);
    true
}
pub(crate) fn run() -> bool {
    use core::fmt::Write;
    let ok = active(0) && run_cases();
    let mut line = crate::boot::log::Line::<256>::new();
    write!(&mut line,
        "[TSCDEADLINE-CHECK] result={} stage={} far=60s stale=self-IPI min=1tick expired=hardware ap-progress=required idle-demotion=real-wake fallback=HPET/count\r\n",
        if ok { "PASS" } else { "FAILED" }, STAGE.load(Ordering::Acquire)
    ).expect("Bounded isolated timer diagnostic");
    crate::boot::log::text(line.as_bytes());
    crate::boot::log_route::serial_write_raw(line.as_bytes());
    for cpu in 0..cpu_topology::count() {
        log(cpu);
    }
    ok
}
