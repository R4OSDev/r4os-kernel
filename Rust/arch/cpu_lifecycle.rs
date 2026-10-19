//! Controlled logical AP reuse. Only this owner constructs quiescent/stop
//! receipts; other domains consume the exact CPU identity and execution epoch.

/// The AP is in its permanent idle continuation with IF=0, without runtime,
/// ranked or module owners. It can only poll lock-free TLB requests, cancel
/// its own park, or acknowledge an authenticated stop.
pub(crate) struct Parked {
    index: u32,
    epoch: u64,
    switch_serial: u64,
}
impl Parked {
    pub(crate) fn index(&self) -> u32 {
        self.index
    }
    pub(crate) fn epoch(&self) -> u64 {
        self.epoch
    }
}

/// This receipt is produced only after the real park, last TLB withdrawal and
/// terminal stop acknowledgements agree. Retained idle/boot/IST stacks remain
/// owned until the next exact INIT/start or terminal system shutdown.
pub(crate) struct Stopped {
    index: u32,
    apic_id: u32,
    epoch: u64,
    switch_serial: u64,
    tlb_generation: u64,
}
impl Stopped {
    pub(crate) fn index(&self) -> u32 {
        self.index
    }
    pub(crate) fn apic_id(&self) -> u32 {
        self.apic_id
    }
    pub(crate) fn epoch(&self) -> u64 {
        self.epoch
    }
    pub(crate) fn current(&self) -> bool {
        self.index != 0
            && super::percpu::state(self.index) == super::percpu::OFFLINE
            && crate::platform::cpu_topology::epoch(self.index) == self.epoch
    }
}

use super::smp_policy::MAX_CPUS;
use crate::contract as c;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub(crate) enum Reason {
    None = 0,
    Fixed,
    KernelContext,
    Resources,
    TaskBudget,
    PageOwner,
    Tlb,
    Irq,
    Timer,
    Timeout,
    Qualification,
    Busy,
    Caller,
    Index,
    Epoch,
    State,
    Terminal,
}
impl Reason {
    fn from_raw(value: u32) -> Self {
        match value {
            0 => Self::None,
            1 => Self::Fixed,
            2 => Self::KernelContext,
            3 => Self::Resources,
            4 => Self::TaskBudget,
            5 => Self::PageOwner,
            6 => Self::Tlb,
            7 => Self::Irq,
            8 => Self::Timer,
            9 => Self::Timeout,
            10 => Self::Qualification,
            11 => Self::Busy,
            12 => Self::Caller,
            13 => Self::Index,
            14 => Self::Epoch,
            15 => Self::State,
            16 => Self::Terminal,
            _ => Self::Resources,
        }
    }
}
struct Record {
    installed: AtomicU64,
    drain: AtomicU64,
    cancel: AtomicU64,
    parked: AtomicU64,
    switch_serial: AtomicU64,
    withdrawn: AtomicU64,
    tlb_generation: AtomicU64,
    stop_request: AtomicU64,
    stopped: AtomicU64,
    retired: AtomicU64,
    busy_epoch: AtomicU64,
    reason: AtomicU32,
    migrations: AtomicU32,
    irq_retargets: AtomicU32,
    irq_epoch: AtomicU64,
    cycles: AtomicU64,
}
impl Record {
    const fn new() -> Self {
        Self {
            installed: AtomicU64::new(0),
            drain: AtomicU64::new(0),
            cancel: AtomicU64::new(0),
            parked: AtomicU64::new(0),
            switch_serial: AtomicU64::new(0),
            withdrawn: AtomicU64::new(0),
            tlb_generation: AtomicU64::new(0),
            stop_request: AtomicU64::new(0),
            stopped: AtomicU64::new(0),
            retired: AtomicU64::new(0),
            busy_epoch: AtomicU64::new(0),
            reason: AtomicU32::new(0),
            migrations: AtomicU32::new(0),
            irq_retargets: AtomicU32::new(0),
            irq_epoch: AtomicU64::new(0),
            cycles: AtomicU64::new(0),
        }
    }
}
static RECORDS: [Record; MAX_CPUS] = [const { Record::new() }; MAX_CPUS];
const WRITER: u64 = 1u64 << 63;
static ACTIVITY: AtomicU64 = AtomicU64::new(0);
static CAPACITY_GENERATION: AtomicU64 = AtomicU64::new(1);
static TERMINAL: AtomicBool = AtomicBool::new(false);

pub(crate) struct ProbeLease;
impl Drop for ProbeLease {
    fn drop(&mut self) {
        ACTIVITY.fetch_sub(1, Ordering::Release);
    }
}
/// CPU-sensitive explicit probes retain stable membership through real joins.
/// This is distinct from the existing exact caller/task unwind admission.
pub(crate) fn probe_lease() -> Option<ProbeLease> {
    ACTIVITY
        .try_update(Ordering::AcqRel, Ordering::Acquire, |old| {
            if old & WRITER == 0 && old < WRITER - 1 {
                Some(old + 1)
            } else {
                None
            }
        })
        .ok()
        .map(|_| ProbeLease)
}
pub(crate) struct Claim;
impl Drop for Claim {
    fn drop(&mut self) {
        ACTIVITY.store(0, Ordering::Release);
    }
}
pub(crate) fn caller_context() -> bool {
    let flags: u64;
    unsafe {
        core::arch::asm!("pushfq", "pop {}", out(reg) flags, options(preserves_flags));
    }
    super::percpu::current_index() == 0
        && flags & (1 << 9) != 0
        && crate::memory::owner::fault_allowed()
}
pub(crate) fn claim() -> Result<Claim, Reason> {
    if !caller_context() {
        return Err(Reason::Caller);
    }
    if TERMINAL.load(Ordering::Acquire) {
        return Err(Reason::Terminal);
    }
    if CAPACITY_GENERATION.load(Ordering::Acquire) > u64::MAX - 8 {
        return Err(Reason::Busy);
    }
    let claim = ACTIVITY
        .compare_exchange(0, WRITER, Ordering::AcqRel, Ordering::Acquire)
        .map(|_| Claim)
        .map_err(|_| Reason::Busy)?;
    if !crate::sched::task::cpu_drain::controller_stays_bsp() {
        return Err(Reason::Caller);
    }
    Ok(claim)
}
pub(crate) fn capacity_generation() -> u64 {
    CAPACITY_GENERATION.load(Ordering::Acquire)
}
pub(crate) fn capacity_changed() {
    let _ = CAPACITY_GENERATION.try_update(Ordering::AcqRel, Ordering::Acquire, |old| {
        old.checked_add(1)
    });
}
pub(crate) fn install(index: u32, epoch: u64) {
    if let Some(record) = RECORDS.get(index as usize) {
        record.installed.store(epoch, Ordering::Release);
    }
}
pub(crate) fn note_busy(index: u32, reason: Reason) {
    if let Some(record) = RECORDS.get(index as usize) {
        record.reason.store(reason as u32, Ordering::Relaxed);
        record
            .busy_epoch
            .store(record.installed.load(Ordering::Acquire), Ordering::Release);
    }
}
pub(crate) fn terminal() {
    TERMINAL.store(true, Ordering::Release);
}
pub(crate) fn is_terminal() -> bool {
    TERMINAL.load(Ordering::Acquire)
}
pub(crate) fn note_migration(source: u32) {
    if let Some(record) = RECORDS.get(source as usize)
        && super::percpu::state(source) == super::percpu::DRAINING
    {
        record.migrations.fetch_add(1, Ordering::Relaxed);
    }
}
fn bounded_wait(mut done: impl FnMut() -> bool) -> bool {
    let start = crate::platform::clocksource::now_nanoseconds();
    for _ in 0..20_000_000 {
        if done() {
            return true;
        }
        super::tlb::service_pending();
        if start.is_some_and(|t| {
            crate::platform::clocksource::now_nanoseconds()
                .is_some_and(|now| now.saturating_sub(t) >= 250_000_000)
        }) {
            break;
        }
        core::hint::spin_loop();
    }
    done()
}
fn validate(index: u32, expected: u64) -> Result<&'static Record, Reason> {
    if index == 0
        || index as usize >= MAX_CPUS
        || index >= super::smp::r4os_rust_smp_control(6, 0) as u32
    {
        return Err(Reason::Index);
    }
    if expected == 0 || crate::platform::cpu_topology::epoch(index) != expected {
        return Err(Reason::Epoch);
    }
    if is_terminal() {
        return Err(Reason::Terminal);
    }
    Ok(&RECORDS[index as usize])
}
fn cancel(index: u32, epoch: u64, reason: Reason) -> Result<(), Reason> {
    use super::{percpu, runtime::RuntimeGuard};
    let record = &RECORDS[index as usize];
    note_busy(index, reason);
    {
        let _runtime = RuntimeGuard::acquire();
        // Once removed from TLB membership, no old continuation is reusable.
        if record.withdrawn.load(Ordering::Acquire) == epoch {
            return Err(reason);
        }
        if percpu::state(index) == percpu::DRAINING && percpu::cancel_unparked(index) {
            record.drain.store(0, Ordering::Release);
            return Err(reason);
        }
        if percpu::state(index) == percpu::PARKED {
            record.cancel.store(epoch, Ordering::Release);
        }
    }
    let _ = bounded_wait(|| percpu::state(index) == 4);
    Err(reason)
}
fn stopped_receipt(index: u32, epoch: u64) -> Option<Stopped> {
    use super::percpu;
    let r = RECORDS.get(index as usize)?;
    if index == 0
        || percpu::state(index) != percpu::OFFLINE
        || percpu::tlb_mask() & (1u64 << index) != 0
        || crate::platform::cpu_topology::epoch(index) != epoch
        || [
            r.installed.load(Ordering::Acquire),
            r.drain.load(Ordering::Acquire),
            r.irq_epoch.load(Ordering::Acquire),
            r.parked.load(Ordering::Acquire),
            r.withdrawn.load(Ordering::Acquire),
            r.stop_request.load(Ordering::Acquire),
            r.stopped.load(Ordering::Acquire),
        ]
        .iter()
        .any(|value| *value != epoch)
    {
        return None;
    }
    Some(Stopped {
        index,
        apic_id: u32::try_from(percpu::r4os_rust_percpu_control(8, index, 0, 0)).ok()?,
        epoch,
        switch_serial: r.switch_serial.load(Ordering::Acquire),
        tlb_generation: r.tlb_generation.load(Ordering::Acquire),
    })
}
fn reusable_receipt(index: u32, epoch: u64) -> Option<Stopped> {
    let stopped = stopped_receipt(index, epoch)?;
    (RECORDS[index as usize].retired.load(Ordering::Acquire) == epoch).then_some(stopped)
}
/// The caller owns the single lifecycle writer and exact task admission. No
/// owner survives a wait for physical park/stop, or the shared INIT/start path.
pub(crate) fn offline(_claim: &Claim, index: u32, expected: u64) -> Result<(), Reason> {
    use super::{percpu, runtime::RuntimeGuard, tlb};
    let record = validate(index, expected)?;
    if reusable_receipt(index, expected).is_some() {
        return Ok(());
    }
    {
        let _runtime = RuntimeGuard::acquire();
        if percpu::state(index) != 4 || record.installed.load(Ordering::Acquire) != expected {
            return Err(Reason::State);
        }
        let remaining = percpu::r4os_rust_percpu_control(14, 0, 0, 0) & !(1u64 << index);
        if let Err(reason) = unsafe { crate::sched::task::cpu_drain::preflight(index, remaining) } {
            note_busy(index, reason);
            return Err(reason);
        }
        record.reason.store(0, Ordering::Relaxed);
        record.busy_epoch.store(0, Ordering::Relaxed);
        record.cancel.store(0, Ordering::Relaxed);
        record.parked.store(0, Ordering::Relaxed);
        record.withdrawn.store(0, Ordering::Relaxed);
        record.stop_request.store(0, Ordering::Relaxed);
        record.stopped.store(0, Ordering::Relaxed);
        record.retired.store(0, Ordering::Relaxed);
        record.irq_epoch.store(0, Ordering::Relaxed);
        record.migrations.store(0, Ordering::Relaxed);
        record.irq_retargets.store(0, Ordering::Relaxed);
        record.drain.store(expected, Ordering::Release);
        if !percpu::begin_drain(index) {
            return Err(Reason::State);
        }
    }
    let irq = match crate::driver::api_msi::migrate_cpu(index, expected) {
        Ok(n) => n,
        Err(reason) => return cancel(index, expected, reason),
    };
    let io = match super::ioapic::migrate_cpu(index, expected) {
        Ok(n) => n,
        Err(reason) => return cancel(index, expected, reason),
    };
    record.irq_retargets.store(irq + io, Ordering::Relaxed);
    record.irq_epoch.store(expected, Ordering::Release);
    {
        let _runtime = RuntimeGuard::acquire();
        if !crate::sched::scheduler::request_cpu_drain(index, expected) {
            drop(_runtime);
            return cancel(index, expected, Reason::Irq);
        }
    }
    if !bounded_wait(|| {
        record.parked.load(Ordering::Acquire) == expected
            || record.busy_epoch.load(Ordering::Acquire) == expected
    }) {
        return cancel(index, expected, Reason::Timeout);
    }
    if record.parked.load(Ordering::Acquire) != expected {
        return cancel(
            index,
            expected,
            Reason::from_raw(record.reason.load(Ordering::Acquire)),
        );
    }
    {
        let _runtime = RuntimeGuard::acquire();
        let remaining = percpu::r4os_rust_percpu_control(14, 0, 0, 0);
        let moved = unsafe {
            crate::sched::task::cpu_drain::preflight(index, remaining)
                .and_then(|_| crate::sched::task::cpu_drain::move_saved(index))
        };
        match moved {
            Ok(n) => {
                record.migrations.fetch_add(n, Ordering::Relaxed);
            }
            Err(reason) => {
                drop(_runtime);
                return cancel(index, expected, reason);
            }
        }
    }
    let parked = Parked {
        index,
        epoch: expected,
        switch_serial: record.switch_serial.load(Ordering::Acquire),
    };
    let generation;
    {
        let Some(_tables) = crate::memory::owner::Guard::try_acquire(6) else {
            return cancel(index, expected, Reason::PageOwner);
        };
        let mut ack = None;
        let matched = bounded_wait(|| {
            ack = tlb::withdraw_cpu(&parked);
            ack.is_some()
        });
        if !matched {
            drop(_tables);
            return cancel(index, expected, Reason::Tlb);
        }
        generation = ack.unwrap();
        record.tlb_generation.store(generation, Ordering::Relaxed);
        record.withdrawn.store(expected, Ordering::Release);
    }
    // The page-table owner is already returned: an uncertain stop can never
    // block invalidations or unrelated CPU progress.
    record.stop_request.store(expected, Ordering::Release);
    if !percpu::arm_stop(index) || !bounded_wait(|| stopped_receipt(index, expected).is_some()) {
        note_busy(index, Reason::Timeout);
        return Err(Reason::Timeout);
    }
    let stopped = stopped_receipt(index, expected).ok_or(Reason::Resources)?;
    if stopped.switch_serial != parked.switch_serial
        || stopped.tlb_generation != generation
        || !crate::platform::cpu_topology::withdraw(&stopped)
        || !super::pat::withdraw(&stopped)
        || !crate::platform::clocksource::withdraw_cpu(&stopped)
    {
        note_busy(index, Reason::Qualification);
        return Err(Reason::Qualification);
    }
    record.cycles.fetch_add(1, Ordering::Relaxed);
    record.retired.store(expected, Ordering::Release);
    Ok(())
}
pub(crate) fn online(_claim: &Claim, index: u32, expected: u64) -> Result<(), Reason> {
    let record = validate(index, expected)?;
    let stopped = reusable_receipt(index, expected).ok_or(Reason::State)?;
    if !super::smp::restart(&stopped) {
        note_busy(index, Reason::Qualification);
        return Err(Reason::Qualification);
    }
    record.reason.store(0, Ordering::Release);
    Ok(())
}
pub(crate) fn result(reason: Reason) -> i32 {
    match reason {
        Reason::None => c::cpu_lifecycle_ok,
        Reason::Caller => c::cpu_lifecycle_error_context,
        Reason::Index => c::cpu_lifecycle_error_index,
        Reason::Epoch => c::cpu_lifecycle_error_epoch,
        Reason::Qualification => c::cpu_lifecycle_error_qualification,
        Reason::State | Reason::Terminal => c::cpu_lifecycle_error_state,
        _ => c::cpu_lifecycle_error_busy,
    }
}
pub(crate) fn reason_value(reason: Reason) -> u32 {
    match reason {
        Reason::None => c::cpu_lifecycle_reason_none,
        Reason::Fixed => c::cpu_lifecycle_reason_fixed,
        Reason::KernelContext => c::cpu_lifecycle_reason_kernel_context,
        Reason::Resources => c::cpu_lifecycle_reason_resources,
        Reason::TaskBudget => c::cpu_lifecycle_reason_task_budget,
        Reason::PageOwner => c::cpu_lifecycle_reason_page_owner,
        Reason::Tlb => c::cpu_lifecycle_reason_tlb,
        Reason::Irq => c::cpu_lifecycle_reason_irq,
        Reason::Timer => c::cpu_lifecycle_reason_timer,
        Reason::Timeout => c::cpu_lifecycle_reason_timeout,
        Reason::Qualification => c::cpu_lifecycle_reason_qualification,
        Reason::Busy => c::cpu_lifecycle_reason_busy,
        Reason::Caller => c::cpu_lifecycle_reason_caller,
        Reason::Index => c::cpu_lifecycle_reason_index,
        Reason::Epoch => c::cpu_lifecycle_reason_epoch,
        Reason::State => c::cpu_lifecycle_reason_state,
        Reason::Terminal => c::cpu_lifecycle_reason_terminal,
    }
}
fn state_value(state: u8) -> u32 {
    match state {
        0 => c::cpu_lifecycle_state_absent,
        1 => c::cpu_lifecycle_state_detected,
        2 => c::cpu_lifecycle_state_starting,
        3 => c::cpu_lifecycle_state_entered,
        4 => c::cpu_lifecycle_state_online,
        5 => c::cpu_lifecycle_state_terminal_pending,
        6 => c::cpu_lifecycle_state_terminal,
        7 => c::cpu_lifecycle_state_failed,
        8 => c::cpu_lifecycle_state_qualified,
        super::percpu::DRAINING => c::cpu_lifecycle_state_draining,
        super::percpu::PARKED => c::cpu_lifecycle_state_parked,
        super::percpu::OFFLINE => c::cpu_lifecycle_state_offline,
        super::percpu::STOP_PENDING => c::cpu_lifecycle_state_stop_pending,
        _ => c::cpu_lifecycle_state_failed,
    }
}
pub(crate) fn snapshot(index: u32) -> Option<c::CpuLifecycleInfo> {
    use super::{percpu, smp};
    let configured = smp::r4os_rust_smp_control(6, 0) as u32;
    if index >= configured {
        return None;
    }
    let r = RECORDS.get(index as usize)?;
    let epoch = crate::platform::cpu_topology::epoch(index);
    let state = percpu::state(index);
    let mut flags = 0;
    if percpu::is_online(index) {
        flags |= c::cpu_lifecycle_flag_online;
    }
    if percpu::is_schedulable(index) {
        flags |= c::cpu_lifecycle_flag_schedulable;
    }
    if state == percpu::PARKED && r.parked.load(Ordering::Acquire) == epoch {
        flags |= c::cpu_lifecycle_flag_parked;
    }
    if percpu::tlb_mask() & (1u64 << index) != 0 {
        flags |= c::cpu_lifecycle_flag_tlb_enrolled;
    }
    if stopped_receipt(index, epoch).is_some() {
        flags |= c::cpu_lifecycle_flag_current_receipt;
    }
    if reusable_receipt(index, epoch).is_some() {
        flags |= c::cpu_lifecycle_flag_reusable;
    }
    if is_terminal() {
        flags |= c::cpu_lifecycle_flag_terminal;
        flags &= !c::cpu_lifecycle_flag_reusable;
    }
    Some(c::CpuLifecycleInfo {
        cpu_index: index,
        state: state_value(state),
        flags,
        reason: reason_value(Reason::from_raw(r.reason.load(Ordering::Acquire))),
        configured_cpus: configured,
        available_cpus: percpu::r4os_rust_percpu_control(14, 0, 0, 0).count_ones(),
        epoch,
        installed_epoch: r.installed.load(Ordering::Acquire),
        capacity_generation: capacity_generation(),
        parked_epoch: r.parked.load(Ordering::Acquire),
        withdrawn_epoch: r.withdrawn.load(Ordering::Acquire),
        stopped_epoch: r.stopped.load(Ordering::Acquire),
        tlb_generation: r.tlb_generation.load(Ordering::Acquire),
        switch_serial: r.switch_serial.load(Ordering::Acquire),
        offline_cycles: r.cycles.load(Ordering::Acquire),
        tasks_migrated: r.migrations.load(Ordering::Acquire),
        irq_retargets: r.irq_retargets.load(Ordering::Acquire),
        irq_epoch: r.irq_epoch.load(Ordering::Acquire),
        ..Default::default()
    })
}

mod check;
pub(crate) use check::run as check;

/// Runs only from the actual permanent AP idle continuation. Once parked,
/// IF stays zero and the loop never enters a device, filesystem or module.
pub(crate) fn idle_checkpoint() {
    use super::{fpu, lapic, percpu, runtime, smp, tlb};
    let index = percpu::current_index();
    if index == 0 || percpu::state(index) != percpu::DRAINING {
        return;
    }
    let record = &RECORDS[index as usize];
    let epoch = record.installed.load(Ordering::Acquire);
    if epoch == 0
        || record.drain.load(Ordering::Acquire) != epoch
        || record.irq_epoch.load(Ordering::Acquire) != epoch
        || crate::platform::cpu_topology::epoch(index) != epoch
    {
        return;
    }
    let rsp: u64;
    unsafe {
        core::arch::asm!("mov {}, rsp", out(reg) rsp, options(nomem, nostack, preserves_flags));
    }
    if !smp::idle_stack_contains(index, rsp) {
        note_busy(index, Reason::Resources);
        return;
    }
    let serial;
    {
        let _runtime = runtime::RuntimeGuard::acquire();
        if percpu::state(index) != percpu::DRAINING {
            return;
        }
        if record.irq_epoch.load(Ordering::Acquire) != epoch {
            return;
        }
        let Some(receipt) = (unsafe { crate::sched::task::cpu_drain::idle_receipt(index) }) else {
            note_busy(index, Reason::Resources);
            return;
        };
        if !crate::sched::scheduler::idle_switch_complete(index) || !percpu::park_current() {
            note_busy(index, Reason::Resources);
            return;
        }
        serial = receipt;
    }
    let saved = runtime::save_local();
    let mut timer = lapic::pause_secondary();
    let clean = fpu::restore_initial();
    if timer.is_some() && clean && crate::memory::owner::fault_allowed() {
        tlb::flush_all_local();
        record.switch_serial.store(serial, Ordering::Relaxed);
        record.parked.store(epoch, Ordering::Release);
    } else {
        note_busy(
            index,
            if timer.is_some() {
                Reason::Resources
            } else {
                Reason::Timer
            },
        );
    }
    loop {
        tlb::service_pending();
        if TERMINAL.load(Ordering::Acquire) {
            tlb::flush_all_local();
            lapic::r4os_rust_lapic_control(8, 0, 0);
            if percpu::terminal_parked_current() {
                runtime::halt_forever();
            }
        }
        if percpu::state(index) == percpu::STOP_PENDING
            && record.stop_request.load(Ordering::Acquire) == epoch
            && record.withdrawn.load(Ordering::Acquire) == epoch
            && record.parked.load(Ordering::Acquire) == epoch
            && record.installed.load(Ordering::Acquire) == epoch
            && crate::platform::cpu_topology::epoch(index) == epoch
            && percpu::tlb_mask() & (1u64 << index) == 0
        {
            tlb::flush_all_local();
            lapic::r4os_rust_lapic_control(8, 0, 0);
            if percpu::confirm_stopped_current() {
                record.stopped.store(epoch, Ordering::Release);
                runtime::halt_forever();
            }
        }
        if record.cancel.load(Ordering::Acquire) == epoch
            && record.withdrawn.load(Ordering::Acquire) != epoch
        {
            // Nonblocking: an IF=0 waiter must not obstruct a TLB requester
            // behind the runtime owner. Refusal continues the lock-free poll.
            if let Some(_runtime) = runtime::RuntimeGuard::try_acquire() {
                if let Some(token) = timer.take() {
                    tlb::flush_all_local();
                    if lapic::resume_secondary(token) && percpu::resume_current() {
                        record.cancel.store(0, Ordering::Release);
                        record.drain.store(0, Ordering::Release);
                        drop(_runtime);
                        runtime::restore_local(saved);
                        return;
                    }
                }
                note_busy(index, Reason::Timer);
            }
        }
        core::hint::spin_loop();
    }
}
