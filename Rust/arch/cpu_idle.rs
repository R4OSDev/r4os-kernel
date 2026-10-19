//! Resident copied idle admission. Firmware interpretation and bounded intent
//! are external; only exact CPU-local entry/wake mechanics execute here.
#[path = "cpu_idle_rules.rs"]
mod cpu_idle_rules;
use crate::{
    arch::{lapic, percpu, runtime, safe_msr, smp},
    contract as c,
    platform::{cpu_capabilities as catalog, cpu_power, cpu_topology, monotonic},
};
use core::{
    cell::UnsafeCell,
    mem,
    sync::atomic::{AtomicU64, Ordering::SeqCst},
};
use runtime::RuntimeGuard;

#[derive(Clone, Copy)]
struct Declaration {
    owner: u32,
    owner_epoch: u64,
    value: c::CpuIdleDescriptor,
    revision: u64,
}
struct State {
    declarations: [Option<Declaration>; 64],
    policy: Option<(u32, u64, c::CpuIdlePolicy)>,
    next_revision: u64,
}
struct Storage(UnsafeCell<State>);
// Only short existing Runtime admission accesses non-atomic copied metadata.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State {
    declarations: [None; 64],
    policy: None,
    next_revision: 1,
}));
struct Counters {
    gate: AtomicU64,
    active: AtomicU64,
    attempts: AtomicU64,
    fallback: AtomicU64,
    cancelled: AtomicU64,
    elapsed: AtomicU64,
    last_elapsed: AtomicU64,
    reason: AtomicU64,
}
impl Counters {
    const fn new() -> Self {
        Self {
            gate: AtomicU64::new(0),
            active: AtomicU64::new(0),
            attempts: AtomicU64::new(0),
            fallback: AtomicU64::new(0),
            cancelled: AtomicU64::new(0),
            elapsed: AtomicU64::new(0),
            last_elapsed: AtomicU64::new(0),
            reason: AtomicU64::new(1),
        }
    }
}
static COUNTERS: [Counters; 64] = [const { Counters::new() }; 64];
// Permanent ordinary kernel BSS pages (normal WB mapping), one stable address
// per CPU. No address churn, user pointer, freed memory or module storage.
#[repr(C, align(4096))]
struct MonitorPage {
    word: AtomicU64,
    padding: [u8; 4088],
}
static MONITORS: [MonitorPage; 64] = [const {
    MonitorPage {
        word: AtomicU64::new(0),
        padding: [0; 4088],
    }
}; 64];

fn close(cpu: usize) {
    COUNTERS[cpu].gate.store(0, SeqCst);
    MONITORS[cpu].word.fetch_add(1, SeqCst);
    if COUNTERS[cpu].active.load(SeqCst) != 0 {
        smp::send_reschedule(cpu as u32);
    }
}
fn reopen(s: &State, cpu: usize) {
    if s.policy.is_some_and(|p| p.2.enabled == 1)
        && let Some(d) =
            s.declarations[cpu].filter(|d| d.value.reason == 0 && d.value.entry_method == 1)
    {
        COUNTERS[cpu].gate.store(d.revision, SeqCst);
    }
}
pub(crate) fn active_locked(_: &RuntimeGuard) -> bool {
    COUNTERS.iter().any(|c| c.active.load(SeqCst) != 0)
}
/// Closes admission first, then requires real local exits. Safe to retry this
/// *withdrawal*, never an instruction or arbitrary hardware mutation.
pub(crate) fn withdraw_owner_locked(_: &RuntimeGuard, owner: u32) -> bool {
    let s = unsafe { &mut *STATE.0.get() };
    let policy = s.policy.is_some_and(|p| p.0 == owner);
    let mut busy = false;
    for (cpu, counter) in COUNTERS.iter().enumerate() {
        if policy || s.declarations[cpu].is_some_and(|d| d.owner == owner) {
            close(cpu);
            busy |= counter.active.load(SeqCst) != 0;
        }
    }
    if busy {
        return false;
    }
    if policy {
        s.policy = None;
    }
    for d in &mut s.declarations {
        if d.is_some_and(|v| v.owner == owner) {
            *d = None;
        }
    }
    true
}
pub(crate) fn publish_locked(_: &RuntimeGuard, id: (u32, u64), v: c::CpuIdleDescriptor) -> i32 {
    if v.cpu_index >= 64
        || v.version != 1
        || v.size < mem::size_of::<c::CpuIdleDescriptor>() as u32
        || v.flags != 0
        || v.reserved0 != 0
        || v.reason > c::cpu_idle_reason_withdrawn
        || v.source > 2
        || v.state_count > 32
        || v.lpi_state_count > 32
        || v.reason == 0
            && (v.source != 1
                || v.entry_method != 1
                || v.cstate_type != 2
                || v.coordination != 1
                || v.context_loss != 0
                || v.entry_hint > 255
                || v.descriptor_identity == 0
                || v.state_count == 0
                || v.selected_index >= v.state_count
                || v.latency_us > 65535
                || v.min_residency_us != 0)
    {
        return c::cpu_capabilities_error_invalid;
    }
    let cpu = v.cpu_index as usize;
    let s = unsafe { &mut *STATE.0.get() };
    if s.declarations[cpu].is_some_and(|d| (d.owner, d.owner_epoch) != id) {
        return c::cpu_capabilities_error_busy;
    }
    if v.reason != c::cpu_idle_reason_withdrawn
        && (v.epoch == 0
            || cpu_topology::epoch(v.cpu_index) != v.epoch
            || !percpu::is_schedulable(v.cpu_index))
    {
        return c::cpu_capabilities_error_epoch;
    }
    close(cpu);
    if COUNTERS[cpu].active.load(SeqCst) != 0 {
        return c::cpu_capabilities_error_busy;
    }
    let Some(next) = s.next_revision.checked_add(1) else {
        return c::cpu_capabilities_error_busy;
    };
    let revision = s.next_revision;
    s.next_revision = next;
    s.declarations[cpu] = Some(Declaration {
        owner: id.0,
        owner_epoch: id.1,
        value: v,
        revision,
    });
    COUNTERS[cpu].reason.store(
        u64::from(if v.reason == 0 {
            c::cpu_idle_reason_monitor_fsm
        } else {
            v.reason
        }),
        SeqCst,
    );
    reopen(s, cpu);
    0
}
pub(crate) fn policy_locked(_: &RuntimeGuard, id: (u32, u64), v: c::CpuIdlePolicy) -> i32 {
    if v.version != 1
        || v.size < mem::size_of::<c::CpuIdlePolicy>() as u32
        || v.reserved0 != 0
        || v.enabled > 1
        || v.enabled == 1
            && (!(1..=1000).contains(&v.maximum_latency_us)
                || !(1..=16).contains(&v.residency_factor))
    {
        return c::cpu_capabilities_error_invalid;
    }
    let s = unsafe { &mut *STATE.0.get() };
    if s.policy.is_some_and(|p| (p.0, p.1) != id) {
        return c::cpu_capabilities_error_busy;
    }
    for cpu in 0..64 {
        close(cpu);
    }
    if COUNTERS.iter().any(|c| c.active.load(SeqCst) != 0) {
        return c::cpu_capabilities_error_busy;
    }
    s.policy = Some((id.0, id.1, v));
    for cpu in 0..64 {
        reopen(s, cpu);
    }
    0
}
pub(crate) struct Ticket {
    cpu: usize,
    epoch: u64,
    revision: u64,
    hint: u32,
    target: u64,
}
/// Scheduler owns Runtime and has just rechecked Ready and due task deadlines.
pub(crate) fn prepare_locked(_: &RuntimeGuard) -> Option<Ticket> {
    let cpu = percpu::current_index() as usize;
    let counters = COUNTERS.get(cpu)?;
    if !crate::sched::scheduler::idle_switch_complete(cpu as u32) {
        return None;
    }
    let s = unsafe { &*STATE.0.get() };
    let d = s.declarations[cpu]?;
    let reason = (|| {
        if d.value.reason != 0 {
            return Err(d.value.reason);
        }
        if cpu_power::transitioning()
            || !percpu::is_schedulable(cpu as u32)
            || cpu_topology::epoch(cpu as u32) != d.value.epoch
        {
            return Err(c::cpu_idle_reason_epoch);
        }
        let p = s
            .policy
            .filter(|p| p.2.enabled == 1)
            .ok_or(c::cpu_idle_reason_policy)?
            .2;
        if d.value.latency_us > p.maximum_latency_us {
            return Err(c::cpu_idle_reason_policy);
        }
        let cap =
            catalog::at_epoch(cpu as u32, d.value.epoch).map_err(|_| c::cpu_idle_reason_epoch)?;
        cpu_idle_rules::hardware(&cap.raw, d.value.entry_hint)?;
        if !crate::memory::paging::resident_writeback_page(
            core::ptr::addr_of!(MONITORS[cpu].word) as u64
        ) {
            return Err(c::cpu_idle_reason_monitor);
        }
        // Read only the architecturally feature-qualified Intel register.
        // A protected #GP refuses entry; never enable/reset a firmware setting.
        if !matches!(unsafe { safe_msr::read_exact(cpu as u32,d.value.epoch,0x1a0) }, Ok(v) if v & (1 << 18) != 0)
        {
            return Err(c::cpu_idle_reason_monitor_fsm);
        }
        let target = cpu_idle_rules::target(
            d.value.latency_us,
            d.value.min_residency_us,
            p.residency_factor,
        );
        let budget = lapic::idle_budget_us().ok_or(c::cpu_idle_reason_timer)?;
        if budget <= target {
            return Err(c::cpu_idle_reason_short_budget);
        }
        Ok(target)
    })();
    match reason {
        Err(reason) => {
            counters.reason.store(u64::from(reason), SeqCst);
            None
        }
        Ok(target) => {
            if counters.gate.load(SeqCst) != d.revision {
                return None;
            }
            counters.reason.store(0, SeqCst);
            counters.active.fetch_add(1, SeqCst);
            Some(Ticket {
                cpu,
                epoch: d.value.epoch,
                revision: d.revision,
                hint: d.value.entry_hint,
                target,
            })
        }
    }
}
/// Runtime was released for switch, IF remains0. Pending/new unmasked APIC
/// interrupts break MWAIT even before STI, via the qualified ECX[0] extension.
pub(crate) fn enter(ticket: Ticket) {
    let cpu = ticket.cpu;
    enter_with(
        ticket,
        || unsafe {
            core::arch::asm!("monitor",in("rax") core::ptr::addr_of!(MONITORS[cpu].word),in("ecx")0u32,in("edx")0u32,options(nostack));
        },
        |hint| unsafe {
            core::arch::asm!("mwait",in("eax")hint,in("ecx")1u32,options(nostack));
        },
    );
    // Unlike STI;HLT, masked-break MWAIT returns with IF0 and its wake IRQ
    // still pending. Deliver that IRQ after the exact entry pin is released
    // and before scheduler leave_idle can replace its armed timer. The NOP
    // consumes STI's interrupt shadow; back-to-back STI;CLI would lose this
    // delivery window. No Runtime owner or module reference spans the IRQ.
    unsafe {
        core::arch::asm!("sti", "nop", "cli", options(nostack));
    }
}
// Statically bound instruction peers permit exercising this actual ordering
// with synthetic interrupts in the existing host source gate. Production
// always supplies the two exact local instructions above, never module code.
fn enter_with(ticket: Ticket, monitor: impl FnOnce(), wait: impl FnOnce(u32)) {
    let t = &COUNTERS[ticket.cpu];
    let start = monotonic::now_nanoseconds();
    monitor();
    if t.gate.load(SeqCst) == ticket.revision
        && cpu_topology::epoch(ticket.cpu as u32) == ticket.epoch
        && percpu::is_schedulable(ticket.cpu as u32)
        && !cpu_power::transitioning()
        && crate::sched::scheduler::idle_entry_still_empty()
        && lapic::idle_budget_us().is_some_and(|v| v > ticket.target)
    {
        t.attempts.fetch_add(1, SeqCst);
        wait(ticket.hint);
    } else {
        t.cancelled.fetch_add(1, SeqCst);
    }
    let elapsed = start
        .zip(monotonic::now_nanoseconds())
        .map_or(0, |(a, b)| b.saturating_sub(a));
    t.last_elapsed.store(elapsed, SeqCst);
    t.elapsed.fetch_add(elapsed, SeqCst);
    t.active.fetch_sub(1, SeqCst);
}
pub(crate) fn hlt() {
    if let Some(c) = COUNTERS.get(percpu::current_index() as usize) {
        c.fallback.fetch_add(1, SeqCst);
    }
    runtime::r4os_rust_runtime_control(13, 0);
}
pub(crate) fn info(cpu: u32) -> Result<c::CpuIdleInfo, i32> {
    let _runtime = RuntimeGuard::acquire();
    let cap = catalog::info(cpu)?;
    let s = unsafe { &*STATE.0.get() };
    let t = &COUNTERS[cpu as usize];
    let mut v = c::CpuIdleInfo {
        cpu_index: cpu,
        epoch: cap.epoch,
        reason: c::cpu_idle_reason_absent,
        policy_generation: s.policy.map_or(0, |p| p.2.provider_generation),
        entry_attempts: t.attempts.load(SeqCst),
        hlt_fallbacks: t.fallback.load(SeqCst),
        cancelled_entries: t.cancelled.load(SeqCst),
        elapsed_ns: t.elapsed.load(SeqCst),
        last_elapsed_ns: t.last_elapsed.load(SeqCst),
        active_entries: t.active.load(SeqCst),
        ..Default::default()
    };
    if let Some(d) = s.declarations[cpu as usize] {
        let f = d.value;
        v.firmware_generation = f.firmware_generation;
        v.descriptor_identity = f.descriptor_identity;
        v.source = f.source;
        v.state_count = f.state_count;
        v.selected_index = f.selected_index;
        v.cstate_type = f.cstate_type;
        v.entry_method = f.entry_method;
        v.entry_hint = f.entry_hint;
        v.latency_us = f.latency_us;
        v.min_residency_us = f.min_residency_us;
        v.coordination = f.coordination;
        v.context_loss = f.context_loss;
        v.declared_power_mw = f.declared_power_mw;
        v.lpi_state_count = f.lpi_state_count;
        v.descriptor_revision = d.revision;
        v.reason = t.reason.load(SeqCst) as u32;
        if f.reason == 0 && t.gate.load(SeqCst) != d.revision {
            v.reason = c::cpu_idle_reason_withdrawn;
        }
        if cpu_power::transitioning() && f.reason == 0 {
            v.reason = c::cpu_idle_reason_policy;
        }
        if f.reason == 0
            && s.policy
                .is_none_or(|p| p.2.enabled == 0 || f.latency_us > p.2.maximum_latency_us)
        {
            v.reason = c::cpu_idle_reason_policy;
        }
        if d.value.epoch != cap.epoch || cap.flags & c::cpu_capabilities_flag_schedulable == 0 {
            v.reason = c::cpu_idle_reason_epoch;
        }
        v.state = if v.reason == c::cpu_idle_reason_withdrawn {
            3
        } else if f.source == 0 {
            0
        } else if v.reason == 0 {
            2
        } else {
            1
        };
    }
    Ok(v)
}
