//! One scheduler state owner; runtime admission is mandatory after SMP start.
//! Early safe-point counters use separate atomics; no State borrow spans a switch.
pub(crate) mod rank_probe;
pub(crate) mod selection;
pub use super::scheduler_types::{CpuPreemptionStats, Stats, StructureStats};
use super::{task::thread_placement, task_context};
use crate::{
    arch::{fpu, percpu, pit, runtime, smp_policy::MAX_CPUS},
    build_config::config,
    memory::owner,
    platform::timer,
};
use core::{
    cell::UnsafeCell,
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicU64, Ordering},
};
use selection::{Fairness, Order, ROLE_COUNT};
const BUCKETS: usize = 5;
const WAIT_FOREVER: u64 = u64::MAX;
const TIMER_WAIT_OBJECT: u64 = 0x5449_4d45_5741_4954;
const QUANTUM: u64 = {
    let ticks = (30 * pit::DEFAULT_HZ) / 1000;
    if ticks == 0 { 1 } else { ticks as u64 }
};
pub(crate) const PLACEMENT_COOLDOWN: u64 = QUANTUM;
const LONG_WARN: u64 = pit::DEFAULT_HZ as u64;
const STARVE_WARN: u64 = 2 * pit::DEFAULT_HZ as u64;
type Task = *mut c_void;
#[derive(Clone, Copy)]
struct CpuState {
    current: Task,
    idle: Task,
    initialized: bool,
    boot_depth: u32,
    pending: bool,
    current_work: bool,
    balance_next_cpu: u32,
    fairness: Fairness,
}
impl CpuState {
    const EMPTY: Self = Self {
        current: ptr::null_mut(),
        idle: ptr::null_mut(),
        initialized: false,
        boot_depth: 0,
        pending: false,
        current_work: false,
        balance_next_cpu: 0,
        fairness: Fairness::EMPTY,
    };
}
struct State {
    cpu: [CpuState; MAX_CPUS],
    balance_at: [u64; MAX_CPUS],
    placement_outgoing: [Task; MAX_CPUS],
    yield_count: u64,
    sleep_count: u64,
    wake_count: u64,
    idle_wait_count: u64,
    object_wait_count: u64,
    object_wake_count: u64,
    object_timeout_count: u64,
    object_cancel_count: u64,
    preempt_disable_call_count: u64,
    preempt_enable_call_count: u64,
    preempt_disable_underflow_count: u64,
    preempt_disable_max_depth: u32,
    preemption_simulation_tick_count: u64,
    preemption_eligible_tick_count: u64,
    preemption_deferred_disabled_count: u64,
    preemption_deferred_critical_count: u64,
    preemption_deferred_no_task_count: u64,
    preemption_deferred_no_ready_count: u64,
    preemption_deferred_quantum_count: u64,
    preemption_deferred_kernel_ip_count: u64,
    preemption_switch_tick_count: u64,
    preemption_quantum_expired_count: u64,
    preemption_app_code_tick_count: u64,
    timer_preemption_switches_by_cpu: [u64; MAX_CPUS],
    reschedule_ipi_switches_by_cpu: [u64; MAX_CPUS],
    long_running_warning_count: u64,
    starvation_warning_count: u64,
    ready_latency_sample_count: u64,
    ready_latency_total_ticks: u64,
    ready_latency_max_ticks: u64,
    ready_latency_last_ticks: u64,
    ready_waiting_max_ticks: u64,
    wait_object_total_ticks: u64,
    wait_object_max_ticks: u64,
    wait_object_last_ticks: u64,
    run_without_switch_max_ticks: u64,
    quantum_overrun_count: u64,
    quantum_overrun_max_ticks: u64,
    preemption_deferred_max_ticks: u64,
    wakeup_reschedule_request_count: u64,
    wakeup_preemption_switch_count: u64,
    ready_candidate_visit_count: u64,
    timeout_candidate_visit_count: u64,
    warning_candidate_visit_count: u64,
    timeout_storm_deferral_count: u64,
    ready_latency_role_samples: [u64; ROLE_COUNT],
    external_irq_fpu_guard_entries: u64,
    external_irq_fpu_guard_mismatches: u64,
    priority_selects: u64,
    priority_picks_high: u64,
    priority_picks_normal: u64,
    priority_picks_low: u64,
    priority_rr_picks: u64,
    role_picks: [u64; ROLE_COUNT],
    st_prio_busy_stop: bool,
    initialized: bool,
    external_irq_fpu_guard_mismatch_reported: bool,
    ready_latency_role_buckets: [[u64; BUCKETS]; ROLE_COUNT],
}
impl State {
    const EMPTY: Self = Self {
        cpu: [CpuState::EMPTY; MAX_CPUS],
        balance_at: [0; MAX_CPUS],
        placement_outgoing: [ptr::null_mut(); MAX_CPUS],
        yield_count: 0,
        sleep_count: 0,
        wake_count: 0,
        idle_wait_count: 0,
        object_wait_count: 0,
        object_wake_count: 0,
        object_timeout_count: 0,
        object_cancel_count: 0,
        preempt_disable_call_count: 0,
        preempt_enable_call_count: 0,
        preempt_disable_underflow_count: 0,
        preempt_disable_max_depth: 0,
        preemption_simulation_tick_count: 0,
        preemption_eligible_tick_count: 0,
        preemption_deferred_disabled_count: 0,
        preemption_deferred_critical_count: 0,
        preemption_deferred_no_task_count: 0,
        preemption_deferred_no_ready_count: 0,
        preemption_deferred_quantum_count: 0,
        preemption_deferred_kernel_ip_count: 0,
        preemption_switch_tick_count: 0,
        preemption_quantum_expired_count: 0,
        preemption_app_code_tick_count: 0,
        timer_preemption_switches_by_cpu: [0; MAX_CPUS],
        reschedule_ipi_switches_by_cpu: [0; MAX_CPUS],
        long_running_warning_count: 0,
        starvation_warning_count: 0,
        ready_latency_sample_count: 0,
        ready_latency_total_ticks: 0,
        ready_latency_max_ticks: 0,
        ready_latency_last_ticks: 0,
        ready_waiting_max_ticks: 0,
        wait_object_total_ticks: 0,
        wait_object_max_ticks: 0,
        wait_object_last_ticks: 0,
        run_without_switch_max_ticks: 0,
        quantum_overrun_count: 0,
        quantum_overrun_max_ticks: 0,
        preemption_deferred_max_ticks: 0,
        wakeup_reschedule_request_count: 0,
        wakeup_preemption_switch_count: 0,
        ready_candidate_visit_count: 0,
        timeout_candidate_visit_count: 0,
        warning_candidate_visit_count: 0,
        timeout_storm_deferral_count: 0,
        ready_latency_role_samples: [0; ROLE_COUNT],
        external_irq_fpu_guard_entries: 0,
        external_irq_fpu_guard_mismatches: 0,
        priority_selects: 0,
        priority_picks_high: 0,
        priority_picks_normal: 0,
        priority_picks_low: 0,
        priority_rr_picks: 0,
        role_picks: [0; ROLE_COUNT],
        st_prio_busy_stop: false,
        initialized: false,
        external_irq_fpu_guard_mismatch_reported: false,
        ready_latency_role_buckets: [[0; BUCKETS]; ROLE_COUNT],
    };
}
// Safe points reject outer owners before taking Runtime. Their counters must
// therefore live outside State: an atomic field access through a concurrent
// whole-State mutable reference would still violate that owner's borrowing.
struct CooperationCounters {
    points: AtomicU64,
    switches: AtomicU64,
    deferred_irq: AtomicU64,
    deferred_owner: AtomicU64,
}
impl CooperationCounters {
    const fn new() -> Self {
        Self {
            points: AtomicU64::new(0),
            switches: AtomicU64::new(0),
            deferred_irq: AtomicU64::new(0),
            deferred_owner: AtomicU64::new(0),
        }
    }
    // The original singleton scheduler reset precedes AP publication.
    fn reset(&self) {
        self.points.store(0, Ordering::Relaxed);
        self.switches.store(0, Ordering::Relaxed);
        self.deferred_irq.store(0, Ordering::Relaxed);
        self.deferred_owner.store(0, Ordering::Relaxed);
    }
}
static COOPERATION: CooperationCounters = CooperationCounters::new();
struct Storage(UnsafeCell<State>);
// SAFETY: singleton initialization precedes APs. Runtime owner serializes every
// later scheduling mutation. Current-CPU reads retain the old local IRQ/task
// admission. Short closures cannot call foreign code, wait or switch stacks.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State::EMPTY));
fn read<T>(f: impl FnOnce(&State) -> T) -> T {
    f(unsafe { &*STATE.0.get() })
}
fn change<T>(f: impl FnOnce(&mut State) -> T) -> T {
    f(unsafe { &mut *STATE.0.get() })
}
fn slot(index: u32) -> usize {
    if index as usize >= MAX_CPUS {
        0
    } else {
        index as usize
    }
}
fn cpu_state(index: u32) -> CpuState {
    read(|s| s.cpu[slot(index)])
}
fn local() -> CpuState {
    cpu_state(percpu::current_index())
}
fn cpu_change<T>(index: u32, f: impl FnOnce(&mut CpuState) -> T) -> T {
    change(|s| f(&mut s.cpu[slot(index)]))
}
fn local_change<T>(f: impl FnOnce(&mut CpuState) -> T) -> T {
    cpu_change(percpu::current_index(), f)
}
fn enabled() -> bool {
    read(|s| s.initialized) && local().initialized
}
pub fn current() -> Task {
    if enabled() {
        local().current
    } else {
        ptr::null_mut()
    }
}
/// # Safety
/// Caller holds runtime admission; the copied exact task pointer remains
/// resident under that owner and must not escape into a blocking operation.
pub(crate) unsafe fn current_on_cpu(index: u32) -> Task {
    if index as usize >= MAX_CPUS || !cpu_state(index).initialized {
        return ptr::null_mut();
    }
    cpu_state(index).current
}
// Runtime admission serializes the pointer and its derived work floor. Ready
// exact currents still contribute at least one unit, whether linked or not.
pub(crate) fn current_has_work(index: u32) -> bool {
    index < MAX_CPUS as u32 && cpu_state(index).initialized && cpu_state(index).current_work
}
pub(crate) fn all_current_have_work(mut mask: u64) -> bool {
    if mask == 0 || mask & crate::arch::smp_policy::CPU_MASK != mask {
        return false;
    }
    while mask != 0 {
        let cpu = mask.trailing_zeros();
        mask &= mask - 1;
        if !current_has_work(cpu) {
            return false;
        }
    }
    true
}
pub(crate) fn note_current_state(cpu: u32, task: Task, work: bool) {
    if cpu < MAX_CPUS as u32 && cpu_state(cpu).current == task {
        cpu_change(cpu, |s| s.current_work = work);
    }
}
fn publish_current(task: Task) {
    let item = view(task);
    let work = !item.cpu_idle && matches!(item.state, 1 | 2);
    local_change(|s| {
        s.current = task;
        s.current_work = work;
    });
    crate::arch::fpu_fault::publish_task(percpu::current_index(), item.id, item.generation);
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TaskView {
    pub(crate) rsp_slot: *mut u64,
    pub(crate) unwind_slot: *mut u32,
    pub(crate) entry: Option<unsafe extern "C" fn()>,
    pub(crate) generation: u64,
    pub(crate) rsp: u64,
    pub(crate) wake_tick: u64,
    pub(crate) wait_object: u64,
    pub(crate) last_scheduled_tick: u64,
    pub(crate) ready_since_tick: u64,
    pub(crate) created_tick: u64,
    pub(crate) last_long_warning: u64,
    pub(crate) last_starvation_warning: u64,
    pub(crate) max_ready_latency: u64,
    pub(crate) id: u32,
    pub(crate) home_cpu: u32,
    pub(crate) preempt_depth: u32,
    pub(crate) held_locks: u32,
    pub(crate) unwind_count: u32,
    pub(crate) state: u8,
    pub(crate) role: u8,
    pub(crate) priority: u8,
    pub(crate) dispatch_rank: u8,
    pub(crate) cpu_idle: bool,
    pub(crate) smp_eligible: bool,
    pub(crate) smp_r4x: bool,
    pub(crate) uses_fpu: bool,
    pub(crate) fpu_valid: bool,
    pub(crate) fpu_present: bool,
    pub(crate) handoff_pending: bool,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ExternalIrqFpuGuard {
    interrupted_task: Task,
    interrupted_generation: u64,
    restore_task_state: bool,
    armed: bool,
}
impl ExternalIrqFpuGuard {
    const EMPTY: Self = Self {
        interrupted_task: ptr::null_mut(),
        interrupted_generation: 0,
        restore_task_state: false,
        armed: false,
    };
}
unsafe extern "C" {

    fn r4os_context_switch(old_rsp: *mut u64, new_rsp: u64);

}
// Keep scalar native projections visible to the caller; a single field
// access must not materialize the complete temporary TaskView.
#[inline(always)]
fn view(task: Task) -> TaskView {
    unsafe { super::task::scheduler_view(task) }
}
fn query(operation: u8, task: Task, a: u64) -> u64 {
    unsafe { super::task::scheduler_query(operation, task, a) }
}
fn mutate(operation: u8, task: Task, a: u64, b: u64) -> u64 {
    unsafe { super::task::scheduler_mutate(operation, task, a, b, ptr::null(), 0) }
}
fn task_query(operation: u8, a: u64) -> Task {
    ptr::with_exposed_provenance_mut(query(operation, ptr::null_mut(), a) as usize)
}
fn log(event: u8, task: Task, a: u64, b: u64) {
    unsafe { scheduler_log(event, task, a, b) }
}
unsafe fn switch_context(outgoing: *mut u64, incoming: u64) {
    unsafe {
        r4os_context_switch(outgoing, incoming);
    }
}
fn tick() -> u64 {
    timer::tick_count()
}
fn since(now: u64, then: u64) -> u64 {
    if then == 0 || now < then {
        0
    } else {
        now - then
    }
}
fn urgent(running: Task) -> bool {
    query(7, running, u64::from(percpu::current_index())) != 0
}
fn ready_count() -> u64 {
    query(3, ptr::null_mut(), u64::from(percpu::current_index()))
}
fn set_pending(value: bool) {
    local_change(|s| s.pending = value)
}
pub(crate) fn preempt_depth() -> u32 {
    let task = current();
    if task.is_null() {
        local().boot_depth
    } else {
        view(task).preempt_depth
    }
}
fn send_reschedule(index: u32) {
    crate::arch::smp::send_reschedule(index);
}
fn init() -> bool {
    unsafe { task_context::bind(ptr::null_mut()) };
    if query(0, ptr::null_mut(), 0) == 0 {
        return false;
    }
    change(|s| *s = State::EMPTY);
    COOPERATION.reset();
    let first = task_query(1, 0);
    if first.is_null() {
        return false;
    }
    publish_current(first);
    local_change(|s| s.initialized = true);
    unsafe { task_context::bind(view(first).unwind_slot) };
    let idle = unsafe { super::task::task_resources::create_bsp_idle(bsp_idle_entry) };
    if idle.is_null() {
        return false;
    }
    local_change(|s| s.idle = idle.cast());
    change(|s| s.initialized = true);
    let deadline = timer::r4os_rust_timer_control(11, 0, 0);
    log(0, ptr::null_mut(), deadline, 0);
    true
}
pub fn prepare_secondary(index: u32) -> bool {
    if !read(|s| s.initialized) || index == 0 || index as usize >= MAX_CPUS {
        return false;
    }
    if !cpu_state(index).idle.is_null() {
        return true;
    }
    let idle = task_query(2, u64::from(index));
    if idle.is_null() {
        return false;
    }
    cpu_change(index, |s| s.idle = idle);
    true
}
pub fn init_secondary(index: u32) -> bool {
    if !read(|s| s.initialized) || index == 0 || index as usize >= MAX_CPUS {
        return false;
    }
    let idle = cpu_state(index).idle;
    if idle.is_null() {
        return false;
    }
    unsafe { task_context::bind(ptr::null_mut()) };
    cpu_change(index, |s| {
        s.current = idle;
        s.current_work = false;
        s.boot_depth = 0;
        s.pending = false;
        s.fairness = Fairness::EMPTY;
        s.initialized = true
    });
    let item = view(idle);
    crate::arch::fpu_fault::publish_task(index, item.id, item.generation);
    mutate(0, idle, 0, 0);
    unsafe { task_context::bind(view(idle).unwind_slot) };
    percpu::r4os_rust_percpu_control(9, index, 1, 0);
    true
}
unsafe extern "C" fn bsp_idle_entry() {
    // BSP timeout/timer work remains in the existing owner. This continuation
    // owns a boot-initialized FPU buffer as those paths can reach callbacks.
    secondary_loop()
}

pub fn secondary_loop() -> ! {
    runtime::r4os_rust_runtime_control(1, 0);
    loop {
        crate::arch::cpu_lifecycle::idle_checkpoint();
        yield_task();
        crate::arch::cpu_lifecycle::idle_checkpoint();
        wait_until_deadline();
    }
}
/// Caller holds runtime admission; execution has reached the permanent idle
/// continuation after cpu.S saved the outgoing frame and released its owner.
pub(crate) fn idle_switch_complete(index: u32) -> bool {
    let cpu = cpu_state(index);
    index == percpu::current_index()
        && cpu.initialized
        && cpu.current == cpu.idle
        && !cpu.current.is_null()
        && !cpu.current_work
        && read(|s| s.placement_outgoing[index as usize].is_null())
}
pub(crate) fn preempt_disable() {
    change(|s| s.preempt_disable_call_count = s.preempt_disable_call_count.wrapping_add(1));
    let task = current();
    let depth = if !task.is_null() {
        mutate(8, task, 0, 0) as u32
    } else {
        local_change(|s| {
            s.boot_depth = s.boot_depth.saturating_add(1);
            s.boot_depth
        })
    };
    change(|s| s.preempt_disable_max_depth = s.preempt_disable_max_depth.max(depth));
}
pub(crate) fn preempt_enable() {
    change(|s| s.preempt_enable_call_count = s.preempt_enable_call_count.wrapping_add(1));
    let task = current();
    let ok = if !task.is_null() {
        mutate(9, task, 0, 0) != 0
    } else {
        local_change(|s| {
            if s.boot_depth == 0 {
                false
            } else {
                s.boot_depth -= 1;
                true
            }
        })
    };
    if !ok {
        change(|s| {
            s.preempt_disable_underflow_count = s.preempt_disable_underflow_count.wrapping_add(1)
        });
    }
}
fn bucket(latency: u64) -> usize {
    match latency {
        0 => 0,
        1 => 1,
        2..=3 => 2,
        4..=7 => 3,
        _ => 4,
    }
}
fn record_ready(task: Task, latency: u64) {
    let role = view(task).role as usize;
    change(|s| {
        s.ready_latency_sample_count = s.ready_latency_sample_count.wrapping_add(1);
        s.ready_latency_total_ticks = s.ready_latency_total_ticks.wrapping_add(latency);
        s.ready_latency_last_ticks = latency;
        s.ready_latency_max_ticks = s.ready_latency_max_ticks.max(latency);
        s.ready_latency_role_samples[role] = s.ready_latency_role_samples[role].wrapping_add(1);
        s.ready_latency_role_buckets[role][bucket(latency)] =
            s.ready_latency_role_buckets[role][bucket(latency)].wrapping_add(1);
    });
}
fn record_wait(latency: u64) {
    change(|s| {
        s.wait_object_total_ticks = s.wait_object_total_ticks.wrapping_add(latency);
        s.wait_object_last_ticks = latency;
        s.wait_object_max_ticks = s.wait_object_max_ticks.max(latency);
    });
}
fn record_run(ticks: u64) {
    change(|s| s.run_without_switch_max_ticks = s.run_without_switch_max_ticks.max(ticks));
}
fn record_overrun(ticks: u64) {
    if ticks <= QUANTUM {
        return;
    }
    let extra = ticks - QUANTUM;
    change(|s| {
        s.quantum_overrun_count = s.quantum_overrun_count.wrapping_add(1);
        s.quantum_overrun_max_ticks = s.quantum_overrun_max_ticks.max(extra);
    });
}
fn record_deferred(ticks: u64) {
    change(|s| s.preemption_deferred_max_ticks = s.preemption_deferred_max_ticks.max(ticks));
}
fn refresh_pending(running: Task, rank_before_dispatch: u8) {
    // Dispatch accounting can exhaust the selected task's last boost before
    // it becomes current. Publish only that actual demotion; an unchanged
    // lower-rank FIFO pick retains its existing fairness policy.
    unsafe {
        rank_changed(
            running.cast(),
            rank_before_dispatch,
            view(running).dispatch_rank,
        );
    }
    if local().pending {
        set_pending(urgent(running));
    }
}
fn next_ready(force_priority: bool) -> Task {
    if !percpu::is_schedulable(percpu::current_index()) {
        return ptr::null_mut();
    }
    unsafe {
        super::mutex_pi::pump();
    }
    refill_ready();
    // Attempt statistics retain their old meaning, but never drive policy.
    change(|s| s.priority_selects = s.priority_selects.wrapping_add(1));
    let cpu = u64::from(percpu::current_index());
    // Copy the local policy; no mutable Scheduler borrow spans Task queries.
    // Runtime retains the exact destination and Ready heads throughout.
    let pick = local().fairness.choose(force_priority, |order| {
        change(|s| s.ready_candidate_visit_count = s.ready_candidate_visit_count.wrapping_add(1));
        let task = match order {
            Order::Fifo => task_query(4, cpu),
            Order::Rank(rank) => task_query(5, cpu | (u64::from(rank) << 32)),
        };
        (!task.is_null()).then_some(task)
    });
    let Some(next) = pick.task else {
        return ptr::null_mut();
    };
    if !force_priority {
        local_change(|s| s.fairness = pick.fairness);
    }
    if pick.fifo {
        change(|s| s.priority_rr_picks = s.priority_rr_picks.wrapping_add(1));
    } else {
        let item = view(next);
        change(|s| {
            match item.priority {
                0 => s.priority_picks_high = s.priority_picks_high.wrapping_add(1),
                1 => s.priority_picks_normal = s.priority_picks_normal.wrapping_add(1),
                2 => s.priority_picks_low = s.priority_picks_low.wrapping_add(1),
                _ => panic!("Invalid task priority"),
            };
            s.role_picks[item.role as usize] = s.role_picks[item.role as usize].wrapping_add(1);
        });
    }
    next
}
fn restore_productive(task: Task) {
    if percpu::current_index() == 0 && !view(task).cpu_idle {
        timer::r4os_rust_timer_control(13, 0, 0);
    }
}
fn note_productive(task: Task) {
    restore_productive(task);
    let item = view(task);
    if !item.smp_eligible || item.cpu_idle {
        return;
    }
    let index = percpu::current_index();
    if percpu::r4os_rust_percpu_control(17, index, u64::from(item.smp_r4x), 0) == 0 || !item.smp_r4x
    {
        return;
    }
    log(1, task, u64::from(index), 0);
}
fn require_switch(outgoing: Task) {
    if !runtime::enabled() {
        return;
    }
    let depth = unsafe { percpu::r4os_rust_percpu_depth().read() };
    if depth == 1 {
        return;
    }
    log(
        2,
        outgoing,
        u64::from(percpu::current_index()),
        u64::from(depth),
    );
    runtime::halt_forever();
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_finish_context_switch() {
    let cpu = percpu::current_index() as usize;
    let outgoing = change(|s| core::mem::replace(&mut s.placement_outgoing[cpu], ptr::null_mut()));
    // cpu.S has published outgoing RSP and installed the incoming stack.
    // Runtime admission still excludes every foreign scheduler mutation.
    unsafe { thread_placement::finish_switch(outgoing.cast()) };
    if !runtime::release_for_switch() {
        runtime::halt_forever();
    }
}
pub(crate) fn request_placement_switch(cpu: u32) {
    cpu_change(cpu, |s| s.pending = true);
    send_reschedule(cpu);
}
pub(crate) fn request_cpu_drain(cpu: u32, epoch: u64) -> bool {
    cpu_change(cpu, |s| s.pending = true);
    crate::arch::smp::request_drain_reschedule(cpu, epoch)
}
fn stage_placement(old: Task, depth: u32) -> bool {
    let draining =
        percpu::state(percpu::current_index()) == percpu::DRAINING && !view(old).cpu_idle;
    let staged = unsafe {
        if draining {
            thread_placement::stage_cpu_drain(old.cast(), depth)
        } else {
            thread_placement::stage(old.cast(), depth)
        }
    };
    if draining && !staged {
        crate::arch::cpu_lifecycle::note_busy(
            percpu::current_index(),
            crate::arch::cpu_lifecycle::Reason::Resources,
        );
    }
    if staged {
        let cpu = percpu::current_index() as usize;
        change(|s| {
            assert!(s.placement_outgoing[cpu].is_null());
            s.placement_outgoing[cpu] = old;
        });
    }
    staged
}

pub fn yield_task() {
    if !enabled() {
        return;
    }
    let flags = runtime::save_runtime();
    let switched = yield_owned();
    runtime::restore_runtime(flags);
    if switched {
        mutate(17, ptr::null_mut(), 0, 0);
    }
}
// Retain admission on a no-switch return. A true return is this exact saved
// stack resuming as Running after the existing physical RSP/FPU handoff; the
// finish-switch hook has already released Runtime. No Rust borrow spans it.
fn yield_owned() -> bool {
    change(|s| s.yield_count = s.yield_count.wrapping_add(1));
    let now = tick();
    preempt_disable();
    let old = local().current;
    if old.is_null() {
        preempt_enable();
        return false;
    }
    if view(old).state == 2 {
        restore_productive(old);
    }
    mutate(1, old, now, 0);
    let departing = stage_placement(old, 1);
    let priority_wakeup = local().pending && urgent(old);
    let selected = next_ready(priority_wakeup);
    let next = if selected.is_null() {
        set_pending(false);
        if view(old).state == 2 && !departing {
            preempt_enable();
            return false;
        }
        let idle = local().idle;
        if idle.is_null() {
            assert!(!departing, "placement requires initialized idle");
            preempt_enable();
            return false;
        }
        idle
    } else {
        selected
    };
    if next == old {
        mutate(0, old, 0, 0);
        preempt_enable();
        set_pending(false);
        return false;
    }
    let old_view = view(old);
    if old_view.state == 2 {
        mutate(if old_view.cpu_idle { 3 } else { 2 }, old, now, 0);
    } else {
        mutate(4, old, 0, 0);
    }
    let rank_before_dispatch = view(next).dispatch_rank;
    let latency = mutate(5, next, now, 0);
    record_ready(next, latency);
    mutate(0, next, 0, 0);
    mutate(6, old, 0, 0);
    mutate(7, next, 0, 0);
    note_productive(next);
    preempt_enable();
    publish_current(next);
    unsafe { task_context::bind(view(next).unwind_slot) };
    refresh_pending(next, rank_before_dispatch);
    require_switch(old);
    let outgoing = view(old).rsp_slot;
    let incoming = view(next).rsp;
    // No state reference or RAII owner spans this physical switch. Assembly
    // publishes outgoing RSP before releasing the unique runtime owner.
    unsafe { switch_context(outgoing, incoming) };
    true
}
pub fn preempt_from_irq() {
    preempt_irq(0, false);
}
pub(crate) fn preempt_timer_from_irq(rip: u64) {
    preempt_irq(rip, true);
}
fn preempt_irq(rip: u64, timer: bool) {
    if !enabled() {
        return;
    }
    let now = tick();
    let old = local().current;
    if old.is_null() || view(old).state != 2 {
        return;
    }
    let departing = stage_placement(old, 0);
    let wakeup = local().pending && urgent(old);
    let selected = next_ready(wakeup);
    let next = if selected.is_null() && departing {
        local().idle
    } else {
        selected
    };
    assert!(!departing || !next.is_null(), "placement requires idle");
    if next.is_null() {
        set_pending(false);
        return;
    }
    if next == old {
        mutate(0, old, 0, 0);
        set_pending(false);
        return;
    }
    if timer {
        crate::sched::fpu_observation::timer_switch(rip);
    }
    mutate(if view(old).cpu_idle { 3 } else { 2 }, old, now, 0);
    let rank_before_dispatch = view(next).dispatch_rank;
    let latency = mutate(5, next, now, 0);
    record_ready(next, latency);
    mutate(0, next, 0, 0);
    mutate(6, old, 0, 0);
    mutate(7, next, 0, 0);
    note_productive(next);
    publish_current(next);
    unsafe { task_context::bind(view(next).unwind_slot) };
    if wakeup {
        change(|s| {
            s.wakeup_preemption_switch_count = s.wakeup_preemption_switch_count.wrapping_add(1)
        });
    }
    refresh_pending(next, rank_before_dispatch);
    require_switch(old);
    let outgoing = view(old).rsp_slot;
    let incoming = view(next).rsp;
    unsafe { switch_context(outgoing, incoming) };
}
unsafe fn block_current(object: u64, timeout: u64, reason: *const u8, len: usize) -> Task {
    if !enabled() {
        return ptr::null_mut();
    }
    let flags = runtime::save_runtime();
    let task = unsafe { block_current_owned(object, timeout, reason, len) };
    runtime::restore_runtime(flags);
    task
}
/// # Safety
/// Caller retains canonical runtime admission and resident reason storage.
/// This bounded enrollment never switches or releases the caller's owner.
pub(crate) unsafe fn block_current_owned(
    object: u64,
    timeout: u64,
    reason: *const u8,
    len: usize,
) -> Task {
    if !enabled() {
        return ptr::null_mut();
    }
    preempt_disable();
    let running = local().current;
    if running.is_null() {
        preempt_enable();
        return ptr::null_mut();
    }
    let now = tick();
    let wake = if timeout == WAIT_FOREVER {
        0
    } else {
        timer::r4os_rust_timer_control(6, now, timeout)
    };
    let previous = query(8, ptr::null_mut(), 0);
    unsafe {
        super::task::begin_wait_at(
            running.cast(),
            wake,
            now,
            super::task::task_types::Bytes {
                pointer: reason,
                len,
            },
            object,
        );
    }
    if percpu::current_index() != 0 && wake != 0 && wake < previous {
        send_reschedule(0);
    }
    change(|s| {
        s.sleep_count = s.sleep_count.wrapping_add(1);
        s.object_wait_count = s.object_wait_count.wrapping_add(1);
    });
    preempt_enable();
    running
}
fn park_blocked(task: Task) {
    let flags = runtime::save_runtime();
    unsafe { park_blocked_owned(task, flags) };
}
/// # Safety
/// Consumes exactly the current task's outer Runtime admission and restores
/// the caller's IF. Enrollment/queue references have ended; this exact task
/// remains physically current until the existing RSP/FPU save releases it.
pub(crate) unsafe fn park_blocked_owned(task: Task, flags: u64) {
    loop {
        // Every mutable Task/State observation has Runtime admission. In the
        // standalone path, a producer may already have woken this current.
        if view(task).state != 3 {
            if local().current == task && view(task).state == 1 {
                let rank_before_dispatch = view(task).dispatch_rank;
                let latency = mutate(5, task, tick(), 0);
                record_ready(task, latency);
                mutate(0, task, 0, 0);
                refresh_pending(task, rank_before_dispatch);
            }
            runtime::restore_runtime(flags);
            return;
        }
        if yield_owned() {
            // Only dispatch of this exact Running task can resume its stack.
            // No queue/record or unowned mutable TaskView is inspected here.
            runtime::restore_runtime(flags);
            mutate(17, ptr::null_mut(), 0, 0);
            return;
        }
        if view(task).state == 3 {
            change(|s| s.idle_wait_count = s.idle_wait_count.wrapping_add(1));
            runtime::restore_runtime(flags);
            wait_until_deadline();
            let _ = runtime::save_runtime();
        }
    }
}
// Runtime admission covers queue transfer, exact target publication and IPI.
fn balance_ready(transfer: super::task::balance::Transfer, now: impl FnMut() -> u64) -> Task {
    let target = transfer.target();
    if !cpu_state(target).initialized {
        return ptr::null_mut();
    }
    let moved = unsafe { super::task::balance::pull_prepared(transfer, now, QUANTUM) };
    if moved.is_null() {
        return ptr::null_mut();
    }
    cpu_change(target, |s| s.pending = true);
    change(|s| {
        s.wakeup_reschedule_request_count = s.wakeup_reschedule_request_count.wrapping_add(1)
    });
    send_reschedule(target);
    moved.cast()
}
fn refill_ready() {
    let target = percpu::current_index();
    if ready_count() != 0 || !enabled() {
        return;
    }
    refill_from_sources(target, tick);
}
fn balance_checkpoint(now: u64) {
    let index = slot(percpu::current_index());
    if now < read(|s| s.balance_at[index]) {
        return;
    }
    change(|s| s.balance_at[index] = now.saturating_add(QUANTUM));
    let moved = refill_from_sources(percpu::current_index(), || now);
    if !moved.is_null() {
        unsafe { super::task::balance::mark_periodic(moved.cast()) };
    }
}
fn refill_from_sources(target: u32, now: impl FnMut() -> u64) -> Task {
    let online = percpu::r4os_rust_percpu_control(14, 0, 0, 0);
    if online.count_ones() < 2 {
        return ptr::null_mut();
    }
    let start = local().balance_next_cpu;
    for source in crate::arch::smp_policy::online_from(online & !(1 << target), start) {
        if !cpu_state(source).initialized {
            continue;
        }
        let Some(transfer) = (unsafe {
            super::task::balance::prepare(
                source,
                target,
                cpu_state(source).current.cast(),
                cpu_state(target).current.cast(),
            )
        }) else {
            continue;
        };
        // One donor and at most eight candidates per attempt. Rotate donors
        // even when their bounded prefix contains only bound tasks.
        local_change(|s| s.balance_next_cpu = (source + 1) % MAX_CPUS as u32);
        return balance_ready(transfer, now);
    }
    ptr::null_mut()
}
fn balance_wake(source: u32, ready_time: u64) -> Task {
    let online = percpu::r4os_rust_percpu_control(14, 0, 0, 0);
    let current = cpu_state(source).current.cast();
    let source_load = unsafe { super::task::balance::workload(source, current) };
    if source_load <= 1
        || (source_load <= 2 && all_current_have_work(online & !(1 << source)))
        || unsafe {
            super::task::balance::singleton_blocked(source, current, || ready_time, QUANTUM)
        }
    {
        return ptr::null_mut();
    }
    let mut targets = 0;
    let mut loads = [0usize; MAX_CPUS];
    for cpu in crate::arch::smp_policy::online_from(online, 0) {
        if !cpu_state(cpu).initialized {
            continue;
        }
        let load = &mut loads[cpu as usize];
        let running = cpu_state(cpu).current;
        *load = unsafe { super::task::balance::workload(cpu, running.cast()) };
        if cpu != source && !running.is_null() && source_load > load.saturating_add(1) {
            targets |= 1 << cpu;
        }
    }
    super::placement::choose(
        targets,
        &loads,
        crate::platform::cpu_topology::domains(),
        Some(source),
    )
    .map_or(ptr::null_mut(), |target| {
        let Some(transfer) = (unsafe {
            super::task::balance::prepare(source, target, current, cpu_state(target).current.cast())
        }) else {
            return ptr::null_mut();
        };
        balance_ready(transfer, || ready_time)
    })
}
/// # Safety
/// Runtime retains this exact live descriptor and completed rank-index update.
/// This requests a decision only; no owner, IF, stack or preemption guard is
/// released here, and a rank change never re-enqueues or migrates the task.
pub(crate) unsafe fn rank_changed(
    task: *mut super::task::task_types::Task,
    old: u8,
    new: u8,
) -> bool {
    use super::task::{task_registry as registry, task_types::State};
    if task.is_null() || old == new || !enabled() {
        return false;
    }
    unsafe {
        let c = registry::core(task);
        let (target, running_changed) = match (*c).state {
            State::Running if new > old => (u32::from((*c).running_cpu), true),
            State::Ready if new < old && (*c).ready_linked && (*c).running_cpu == 0xff => {
                (u32::from((*c).home_cpu), false)
            }
            _ => return false,
        };
        if target as usize >= MAX_CPUS
            || !percpu::is_schedulable(target)
            || !cpu_state(target).initialized
        {
            return false;
        }
        let running = cpu_state(target).current;
        if running.is_null()
            || view(running).state != 2
            || running_changed && running != task.cast()
            || !running_changed && new >= view(running).dispatch_rank
            || query(7, running, u64::from(target)) == 0
        {
            return false;
        }
        cpu_change(target, |s| s.pending = true);
        if target != percpu::current_index() {
            send_reschedule(target);
        }
        true
    }
}
// Runtime admission retains this exact published Ready task. Also used by the
// existing live diagnostic after releasing its own temporary affinity pin.
// Both production callers have just published ready_since_tick; the live
// caller re-notifies within that same runtime admission. Reuse that functional
// time snapshot for wake cooldowns rather than publishing the global clock again.
pub(crate) fn notify_ready(task: Task) {
    let item = view(task);
    let target = item.home_cpu;
    let moved = balance_wake(target, item.ready_since_tick);
    // The exact sole Ready task has already been published and notified on
    // its real destination. An empty former queue needs no source IPI.
    // Retain the original source notification whenever other Ready work stays.
    if moved == task && query(3, ptr::null_mut(), u64::from(target)) == 0 {
        return;
    }
    let local_cpu = percpu::current_index();
    if target != local_cpu && percpu::r4os_rust_percpu_control(13, target, 0, 0) != 0 {
        cpu_change(target, |s| s.pending = true);
        change(|s| {
            s.wakeup_reschedule_request_count = s.wakeup_reschedule_request_count.wrapping_add(1)
        });
        send_reschedule(target);
    } else {
        let running = cpu_state(target).current;
        if !running.is_null() {
            let current = view(running);
            if current.state == 2 && item.dispatch_rank < current.dispatch_rank {
                cpu_change(target, |s| s.pending = true);
                change(|s| {
                    s.wakeup_reschedule_request_count =
                        s.wakeup_reschedule_request_count.wrapping_add(1)
                });
            }
        }
    }
}
fn wake_task(task: Task, result: u32) -> bool {
    let flags = runtime::save_runtime();
    preempt_disable();
    if view(task).state != 3 {
        preempt_enable();
        runtime::restore_runtime(flags);
        return false;
    }
    let waited = mutate(15, task, u64::from(result), 0);
    record_wait(waited);
    change(|s| {
        s.wake_count = s.wake_count.wrapping_add(1);
        match result {
            1 => s.object_wake_count = s.object_wake_count.wrapping_add(1),
            2 => s.object_timeout_count = s.object_timeout_count.wrapping_add(1),
            3 => s.object_cancel_count = s.object_cancel_count.wrapping_add(1),
            _ => {}
        }
    });
    notify_ready(task);
    preempt_enable();
    runtime::restore_runtime(flags);
    true
}
pub(crate) fn publish_created(task: Task) -> bool {
    let flags = runtime::save_runtime();
    preempt_disable();
    let item = view(task);
    if item.state != 3 || item.wait_object != 0 || item.wake_tick != 0 {
        preempt_enable();
        runtime::restore_runtime(flags);
        return false;
    }
    mutate(2, task, tick(), 0);
    notify_ready(task);
    preempt_enable();
    runtime::restore_runtime(flags);
    true
}
fn defer_critical(task: Task, ticks: u64, preemptible: bool) -> bool {
    if preempt_depth() != 0 {
        change(|s| {
            s.preemption_deferred_critical_count =
                s.preemption_deferred_critical_count.wrapping_add(1)
        });
        mutate(11, task, ticks, 0);
        record_deferred(ticks);
        return true;
    }
    if !preemptible {
        change(|s| {
            s.preemption_deferred_kernel_ip_count =
                s.preemption_deferred_kernel_ip_count.wrapping_add(1)
        });
        mutate(11, task, ticks, 0);
        record_deferred(ticks);
        return true;
    }
    false
}
pub fn preempt_pending(preemptible: bool) -> bool {
    if !local().pending || !enabled() {
        return false;
    }
    let running = local().current;
    if running.is_null() || view(running).state != 2 {
        return false;
    }
    if !urgent(running)
        && !unsafe { thread_placement::pending(running.cast()) }
        && !(percpu::state(percpu::current_index()) == percpu::DRAINING && !view(running).cpu_idle)
    {
        set_pending(false);
        return false;
    }
    let ticks = since(tick(), view(running).last_scheduled_tick);
    change(|s| s.preemption_eligible_tick_count = s.preemption_eligible_tick_count.wrapping_add(1));
    mutate(10, running, 0, 0);
    if defer_critical(running, ticks, preemptible) {
        return false;
    }
    change(|s| {
        s.preemption_app_code_tick_count = s.preemption_app_code_tick_count.wrapping_add(1);
        s.preemption_switch_tick_count = s.preemption_switch_tick_count.wrapping_add(1);
    });
    true
}
pub(crate) fn safe_reschedule() -> bool {
    COOPERATION.points.fetch_add(1, Ordering::Relaxed);
    if !owner::fault_allowed() {
        COOPERATION.deferred_owner.fetch_add(1, Ordering::Relaxed);
        return false;
    }
    let flags = runtime::save_runtime();
    if !enabled() {
        runtime::restore_runtime(flags);
        return false;
    }
    if flags & (1 << 9) == 0 {
        COOPERATION.deferred_irq.fetch_add(1, Ordering::Relaxed);
        runtime::restore_runtime(flags);
        return false;
    }
    let running = local().current;
    if running.is_null() {
        runtime::restore_runtime(flags);
        return false;
    }
    let item = view(running);
    // An admitted placement change can need the real local idle continuation,
    // even without another Ready task or an expired quantum. Apply it only at
    // the existing full-save boundary, with the same owner/IRQ guards below.
    let placement = unsafe { thread_placement::pending(running.cast()) }
        || (percpu::state(percpu::current_index()) == percpu::DRAINING && !item.cpu_idle);
    if item.state != 2 || ready_count() == 0 && !placement {
        set_pending(false);
        runtime::restore_runtime(flags);
        return false;
    }
    let priority = placement || local().pending && urgent(running);
    if !priority {
        set_pending(false);
    }
    if !priority && since(tick(), item.last_scheduled_tick) < QUANTUM {
        runtime::restore_runtime(flags);
        return false;
    }
    if preempt_depth() != 0 || item.held_locks != 0 || item.handoff_pending {
        COOPERATION.deferred_owner.fetch_add(1, Ordering::Relaxed);
        runtime::restore_runtime(flags);
        return false;
    }
    runtime::restore_runtime(flags);
    COOPERATION.switches.fetch_add(1, Ordering::Relaxed);
    yield_task();
    true
}
fn has_other_ready() -> bool {
    let flags = runtime::save_runtime();
    let ready = enabled() && ready_count() != 0;
    runtime::restore_runtime(flags);
    ready
}
pub(crate) fn exit_current(retire: bool) -> ! {
    let flags = runtime::save_runtime();
    preempt_disable();
    let task = current();
    if !task.is_null() {
        let item = view(task);
        if item.held_locks != 0 || item.unwind_count != 0 {
            log(3, task, 0, 0);
            runtime::halt_forever();
        }
        if mutate(16, task, u64::from(retire), 0) == 0 {
            log(4, task, 0, 0);
            runtime::halt_forever();
        }
    }
    preempt_enable();
    runtime::restore_runtime(flags);
    yield_task();
    loop {
        if has_other_ready() {
            yield_task();
        } else {
            wait_until_deadline();
        }
    }
}
fn wake_expired(now: u64) -> bool {
    for _ in 0..64 {
        let candidate = task_query(9, 0);
        if candidate.is_null() {
            break;
        }
        change(|s| {
            s.timeout_candidate_visit_count = s.timeout_candidate_visit_count.wrapping_add(1)
        });
        if view(candidate).wake_tick > now {
            break;
        }
        wake_task(candidate, 2);
    }
    let candidate = task_query(9, 0);
    if !candidate.is_null() && view(candidate).wake_tick <= now {
        change(|s| s.timeout_storm_deferral_count = s.timeout_storm_deferral_count.wrapping_add(1));
        return true;
    }
    false
}
fn poll_timed() -> bool {
    let flags = runtime::save_runtime();
    let due = percpu::current_index() == 0 && wake_expired(tick());
    runtime::restore_runtime(flags);
    due
}
fn wait_until_deadline() {
    let flags = runtime::save_runtime();
    let bsp = percpu::current_index() == 0;
    let due = bsp && poll_timed();
    let pi_pending = unsafe { super::mutex_pi::pump() };
    refill_ready();
    if due || pi_pending || ready_count() != 0 {
        runtime::restore_runtime(flags);
        return;
    }
    if bsp {
        timer::r4os_rust_timer_control(12, query(8, ptr::null_mut(), 0), 0);
    }
    let idle_entry = {
        let owner = runtime::RuntimeGuard::acquire();
        crate::arch::cpu_idle::prepare_locked(&owner)
    };
    if !runtime::release_for_switch() {
        runtime::halt_forever();
    }
    if let Some(entry) = idle_entry {
        crate::arch::cpu_idle::enter(entry);
    } else {
        crate::arch::cpu_idle::hlt();
    }
    runtime::r4os_rust_runtime_control(0, 0);
    if bsp {
        let leave = runtime::save_runtime();
        timer::r4os_rust_timer_control(13, 0, 0);
        runtime::restore_runtime(leave);
    }
    runtime::restore_runtime(flags);
}
/// Exact final MONITOR/MWAIT boundary: recheck absolute due work and Ready
/// under the same Runtime owner as HLT admission, then release it before the
/// instruction. Later remote enqueues retain their existing reschedule IPI.
pub(crate) fn idle_entry_still_empty() -> bool {
    let _owner = runtime::RuntimeGuard::acquire();
    let index = percpu::current_index();
    let due = index == 0 && wake_expired(tick());
    let pi_pending = unsafe { super::mutex_pi::pump() };
    refill_ready();
    !due && !pi_pending && ready_count() == 0 && idle_switch_complete(index)
}
fn timer_preemption(now: u64, preemptible: bool) -> bool {
    change(|s| {
        s.preemption_simulation_tick_count = s.preemption_simulation_tick_count.wrapping_add(1)
    });
    let running = local().current;
    if !read(|s| s.initialized) || running.is_null() || view(running).state != 2 {
        change(|s| {
            s.preemption_deferred_no_task_count =
                s.preemption_deferred_no_task_count.wrapping_add(1)
        });
        return false;
    }
    if view(running).cpu_idle {
        refill_ready();
    } else {
        // Use the already published functional IRQ time. Nonempty destinations
        // receive at most one bounded donor attempt per CPU and quantum; urgent
        // empty/idle refill remains independent of this periodic deadline.
        balance_checkpoint(now);
    }
    let placement = unsafe { thread_placement::pending(running.cast()) }
        || (percpu::state(percpu::current_index()) == percpu::DRAINING && !view(running).cpu_idle);
    if ready_count() == 0 && !placement {
        set_pending(false);
        change(|s| {
            s.preemption_deferred_no_ready_count =
                s.preemption_deferred_no_ready_count.wrapping_add(1)
        });
        return false;
    }
    let priority = local().pending && urgent(running);
    if local().pending && !priority {
        set_pending(false);
    }
    let ticks = since(now, view(running).last_scheduled_tick);
    if !priority && !placement && ticks < QUANTUM {
        change(|s| {
            s.preemption_deferred_quantum_count =
                s.preemption_deferred_quantum_count.wrapping_add(1)
        });
        mutate(11, running, ticks, 0);
        record_deferred(ticks);
        return false;
    }
    change(|s| {
        if !priority {
            s.preemption_quantum_expired_count = s.preemption_quantum_expired_count.wrapping_add(1);
        }
        s.preemption_eligible_tick_count = s.preemption_eligible_tick_count.wrapping_add(1);
    });
    mutate(10, running, 0, 0);
    if defer_critical(running, ticks, preemptible) {
        return false;
    }
    let index = percpu::current_index() as usize;
    change(|s| {
        s.preemption_app_code_tick_count = s.preemption_app_code_tick_count.wrapping_add(1);
        s.preemption_switch_tick_count = s.preemption_switch_tick_count.wrapping_add(1);
        s.timer_preemption_switches_by_cpu[index] =
            s.timer_preemption_switches_by_cpu[index].wrapping_add(1);
    });
    true
}
fn warnings(now: u64) {
    if !read(|s| s.initialized) {
        return;
    }
    let running = local().current;
    if !running.is_null() {
        let item = view(running);
        if item.state == 2 && !item.cpu_idle {
            let ticks = since(now, item.last_scheduled_tick);
            if ticks >= LONG_WARN && since(now, item.last_long_warning) >= LONG_WARN {
                mutate(12, running, now, 0);
                change(|s| {
                    s.long_running_warning_count = s.long_running_warning_count.wrapping_add(1)
                });
            }
        }
    }
    if now & 0xf != 0 || !config::ENABLE_METRICS {
        return;
    }
    for _ in 0..ready_count().min(8) {
        let task = task_query(6, u64::from(percpu::current_index()));
        if task.is_null() {
            break;
        }
        change(|s| {
            s.warning_candidate_visit_count = s.warning_candidate_visit_count.wrapping_add(1)
        });
        let item = view(task);
        let base = if item.ready_since_tick != 0 {
            item.ready_since_tick
        } else {
            item.created_tick
        };
        let waiting = since(now, base);
        change(|s| s.ready_waiting_max_ticks = s.ready_waiting_max_ticks.max(waiting));
        if waiting < STARVE_WARN || since(now, item.last_starvation_warning) < STARVE_WARN {
            continue;
        }
        mutate(13, task, now, 0);
        change(|s| s.starvation_warning_count = s.starvation_warning_count.wrapping_add(1));
    }
}
pub fn on_tick(now: u64, preemptible: bool, secondary: bool) -> bool {
    if secondary && !enabled() {
        return false;
    }
    if enabled() {
        let running = local().current;
        if !running.is_null() {
            let item = view(running);
            if item.state == 2 && (!secondary || !item.cpu_idle) {
                if !secondary {
                    restore_productive(running);
                }
                let ticks = mutate(21, running, now, 0);
                record_run(ticks);
                record_overrun(ticks);
            }
        }
    }
    if !secondary {
        wake_expired(now);
    }
    unsafe {
        super::mutex_pi::pump();
    }
    let result = timer_preemption(now, preemptible);
    warnings(now);
    result
}
pub fn on_reschedule(preemptible: bool) {
    let flags = runtime::save_runtime();
    let running = local().current;
    if percpu::state(percpu::current_index()) == percpu::DRAINING
        && !preemptible
        && !running.is_null()
        && !view(running).cpu_idle
    {
        crate::arch::cpu_lifecycle::note_busy(
            percpu::current_index(),
            crate::arch::cpu_lifecycle::Reason::KernelContext,
        );
    }
    if !running.is_null() && view(running).cpu_idle {
        refill_ready();
    }
    if preempt_pending(preemptible) {
        let index = percpu::current_index() as usize;
        change(|s| {
            s.reschedule_ipi_switches_by_cpu[index] =
                s.reschedule_ipi_switches_by_cpu[index].wrapping_add(1)
        });
        preempt_from_irq();
    }
    runtime::restore_runtime(flags);
}
fn irq_enter() -> ExternalIrqFpuGuard {
    if fpu::active_bytes() == 0 {
        return ExternalIrqFpuGuard::EMPTY;
    }
    let running = local().current;
    let mut restore = false;
    if !running.is_null() {
        let item = view(running);
        if item.uses_fpu && item.fpu_valid && item.fpu_present {
            mutate(6, running, 0, 0);
            restore = true;
        }
    }
    assert!(fpu::restore_initial(), "IRQ canonical FP restore rejected");
    change(|s| s.external_irq_fpu_guard_entries = s.external_irq_fpu_guard_entries.wrapping_add(1));
    ExternalIrqFpuGuard {
        interrupted_task: running,
        interrupted_generation: if running.is_null() {
            0
        } else {
            view(running).generation
        },
        restore_task_state: restore,
        armed: true,
    }
}
fn irq_leave(guard: ExternalIrqFpuGuard) {
    if !guard.armed {
        return;
    }
    if guard.restore_task_state {
        if !guard.interrupted_task.is_null() && local().current == guard.interrupted_task {
            let item = view(guard.interrupted_task);
            if item.generation == guard.interrupted_generation
                && item.uses_fpu
                && item.fpu_valid
                && item.fpu_present
            {
                mutate(7, guard.interrupted_task, 0, 0);
                return;
            }
        }
        let report = change(|s| {
            s.external_irq_fpu_guard_mismatches =
                s.external_irq_fpu_guard_mismatches.wrapping_add(1);
            if s.external_irq_fpu_guard_mismatch_reported {
                false
            } else {
                s.external_irq_fpu_guard_mismatch_reported = true;
                true
            }
        });
        if report {
            log(5, ptr::null_mut(), 0, 0);
        }
        panic!("IRQ FP owner identity changed");
    }
    assert!(
        fpu::restore_initial(),
        "IRQ exit canonical FP restore rejected"
    );
}
pub(crate) unsafe fn sleep(ticks: u64, reason: *const u8, len: usize) {
    if ticks == 0 {
        yield_task();
        return;
    }
    let task = unsafe { block_current(TIMER_WAIT_OBJECT, ticks, reason, len) };
    if !task.is_null() {
        park_blocked(task);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn taskEntryTrampoline() -> ! {
    runtime::r4os_rust_runtime_control(1, 0);
    let task = current();
    if !task.is_null()
        && let Some(entry) = view(task).entry
    {
        unsafe { entry() };
    }
    exit_current(false)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_scheduler_priority_worker() {
    while !read(|s| s.st_prio_busy_stop) {
        let start = tick();
        while tick() == start && !read(|s| s.st_prio_busy_stop) {}
        yield_task();
    }
}
fn priority_self_test() -> bool {
    if !read(|s| s.initialized) {
        return false;
    }
    let task = current();
    if task.is_null() {
        return false;
    }
    change(|s| s.st_prio_busy_stop = false);
    let mut spawned = 0;
    for _ in 0..3 {
        let created = task_query(11, r4os_scheduler_priority_worker as *const () as u64);
        if !created.is_null() {
            spawned += 1;
        }
    }
    if spawned == 0 {
        log(6, ptr::null_mut(), 0, 0);
        return false;
    }
    let old_role = view(task).role;
    if mutate(18, task, 0, 0) == 0 {
        return false;
    }
    mutate(19, task, 0, 0);
    for _ in 0..50 {
        unsafe { sleep(1, b"schedprio".as_ptr(), 9) };
    }
    let latency = view(task).max_ready_latency;
    mutate(18, task, u64::from(old_role), 0);
    change(|s| s.st_prio_busy_stop = true);
    let ok = latency <= 3;
    log(7, ptr::null_mut(), u64::from(ok), latency);
    ok
}
/// # Safety
/// Task arguments retain the exact stable task/generation and existing runtime
/// admission. Operations that can switch do not carry a Rust borrow or guard.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_scheduler_task(
    operation: u8,
    task: Task,
    a: u64,
    b: u64,
    text: *const u8,
    len: usize,
) -> u64 {
    match operation {
        0 => unsafe { block_current(a, b, text, len) }.addr() as u64,
        1 => {
            park_blocked(task);
            0
        }
        2 => u64::from(wake_task(task, a as u32)),
        3 => u64::from(publish_created(task)),
        4 => {
            unsafe { sleep(a, text, len) };
            0
        }
        _ => panic!("Invalid private scheduler task operation"),
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_scheduler_control(operation: u8, index: u32, value: u64) -> u64 {
    match operation {
        0 => u64::from(init()),
        1 => u64::from(prepare_secondary(index)),
        2 => u64::from(init_secondary(index)),
        3 => secondary_loop(),
        4 => current().addr() as u64,
        5 => u64::from(has_other_ready()),
        6 => {
            preempt_disable();
            0
        }
        7 => {
            preempt_enable();
            0
        }
        8 => {
            yield_task();
            0
        }
        9 => {
            preempt_from_irq();
            0
        }
        10 => u64::from(preempt_pending(value != 0)),
        11 => u64::from(safe_reschedule()),
        12 => exit_current(false),
        13 => exit_current(true),
        14 => u64::from(poll_timed()),
        15 => {
            on_reschedule(value != 0);
            0
        }
        16 => u64::from(priority_self_test()),
        17 => u64::from(preempt_depth()),
        _ => panic!("Invalid private scheduler control"),
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_scheduler_tick(now: u64, preemptible: bool, secondary: bool) -> bool {
    on_tick(now, preemptible, secondary)
}
/// # Safety
/// Caller holds local IRQ/owner admission and exclusive aligned C guard output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_scheduler_irq_enter(output: *mut ExternalIrqFpuGuard) {
    unsafe { output.write(irq_enter()) }
}
/// # Safety
/// Input is the unchanged exact IRQ/task/generation token, consumed once under
/// the existing IRQ contract. Handlers cannot yield or switch tasks.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_scheduler_irq_leave(input: *const ExternalIrqFpuGuard) {
    irq_leave(unsafe { input.read() })
}
/// # Safety
/// Caller owns exclusive fixed C output. Runtime serializes the diagnostic
/// copy; no State borrow crosses the task ordinal/depth provider callbacks.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_scheduler_stats(output: *mut Stats) {
    let flags = runtime::save_runtime();
    let task = local().current;
    let ordinal = if task.is_null() {
        0
    } else {
        query(10, task, 0).min(u64::from(u32::MAX)) as u32
    };
    let depth = preempt_depth();
    let now = tick();
    let value = read(|s| Stats {
        initialized: s.initialized,
        current_index: ordinal,
        yields: s.yield_count,
        sleeps: s.sleep_count,
        wakes: s.wake_count,
        idle_waits: s.idle_wait_count,
        object_waits: s.object_wait_count,
        object_wakes: s.object_wake_count,
        object_timeouts: s.object_timeout_count,
        object_cancels: s.object_cancel_count,
        ticks: now,
        preemption_supported: 1,
        preemption_enabled: 1,
        preemption_test_mode: 0,
        preempt_disable_depth: depth,
        preempt_disable_max_depth: s.preempt_disable_max_depth,
        preempt_disable_calls: s.preempt_disable_call_count,
        preempt_enable_calls: s.preempt_enable_call_count,
        preempt_disable_underflows: s.preempt_disable_underflow_count,
        preemption_simulation_ticks: s.preemption_simulation_tick_count,
        preemption_eligible_ticks: s.preemption_eligible_tick_count,
        preemption_deferred_disabled: s.preemption_deferred_disabled_count,
        preemption_deferred_critical: s.preemption_deferred_critical_count,
        preemption_deferred_no_task: s.preemption_deferred_no_task_count,
        preemption_deferred_no_ready: s.preemption_deferred_no_ready_count,
        preemption_deferred_quantum: s.preemption_deferred_quantum_count,
        preemption_deferred_kernel_ip: s.preemption_deferred_kernel_ip_count,
        preemption_switch_ticks: s.preemption_switch_tick_count,
        preemption_quantum_ticks: QUANTUM as u32,
        preemption_quantum_expired: s.preemption_quantum_expired_count,
        preemption_app_code_ticks: s.preemption_app_code_tick_count,
        long_running_task_warnings: s.long_running_warning_count,
        starvation_warnings: s.starvation_warning_count,
        ready_latency_samples: s.ready_latency_sample_count,
        ready_latency_total_ticks: s.ready_latency_total_ticks,
        ready_latency_max_ticks: s.ready_latency_max_ticks,
        ready_latency_last_ticks: s.ready_latency_last_ticks,
        ready_waiting_max_ticks: s.ready_waiting_max_ticks,
        wait_object_total_ticks: s.wait_object_total_ticks,
        wait_object_max_ticks: s.wait_object_max_ticks,
        wait_object_last_ticks: s.wait_object_last_ticks,
        run_without_switch_max_ticks: s.run_without_switch_max_ticks,
        quantum_overrun_count: s.quantum_overrun_count,
        quantum_overrun_max_ticks: s.quantum_overrun_max_ticks,
        preemption_deferred_max_ticks: s.preemption_deferred_max_ticks,
        long_running_warn_threshold_ticks: LONG_WARN,
        starvation_warn_threshold_ticks: STARVE_WARN,
        priority_selects: s.priority_selects,
        priority_picks_high: s.priority_picks_high,
        priority_picks_normal: s.priority_picks_normal,
        priority_picks_low: s.priority_picks_low,
        priority_rr_picks: s.priority_rr_picks,
        role_picks: s.role_picks,
        ready_latency_role_samples: s.ready_latency_role_samples,
        ready_latency_role_buckets: s.ready_latency_role_buckets,
        safe_reschedule_points: COOPERATION.points.load(Ordering::Relaxed),
        safe_reschedule_switches: COOPERATION.switches.load(Ordering::Relaxed),
        safe_reschedule_deferred_irq: COOPERATION.deferred_irq.load(Ordering::Relaxed),
        safe_reschedule_deferred_owner: COOPERATION.deferred_owner.load(Ordering::Relaxed),
    });
    unsafe { output.write(value) };
    runtime::restore_runtime(flags);
}
/// # Safety
/// Caller owns exclusive aligned fixed C structure output until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_scheduler_structure(output: *mut StructureStats) {
    let flags = runtime::save_runtime();
    let pending = local().pending;
    let value = read(|s| StructureStats {
        ready_candidate_visits: s.ready_candidate_visit_count,
        timeout_candidate_visits: s.timeout_candidate_visit_count,
        warning_candidate_visits: s.warning_candidate_visit_count,
        timeout_storm_deferrals: s.timeout_storm_deferral_count,
        wakeup_reschedule_requests: s.wakeup_reschedule_request_count,
        wakeup_preemption_switches: s.wakeup_preemption_switch_count,
        safe_reschedule_points: COOPERATION.points.load(Ordering::Relaxed),
        safe_reschedule_switches: COOPERATION.switches.load(Ordering::Relaxed),
        safe_reschedule_deferred_irq: COOPERATION.deferred_irq.load(Ordering::Relaxed),
        safe_reschedule_deferred_owner: COOPERATION.deferred_owner.load(Ordering::Relaxed),
        reschedule_pending: pending,
    });
    unsafe { output.write(value) };
    runtime::restore_runtime(flags);
}
/// # Safety
/// Caller retains original passive CPU-counter admission and fixed C output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_scheduler_cpu_stats(
    index: u32,
    output: *mut CpuPreemptionStats,
) {
    let value = if index as usize >= MAX_CPUS {
        CpuPreemptionStats::EMPTY
    } else {
        read(|s| CpuPreemptionStats {
            timer_switches: s.timer_preemption_switches_by_cpu[index as usize],
            reschedule_ipi_switches: s.reschedule_ipi_switches_by_cpu[index as usize],
        })
    };
    unsafe { output.write(value) };
}

const _: () = {
    assert!(core::mem::size_of::<TaskView>() == 136);
    assert!(core::mem::offset_of!(TaskView, id) == 104);
    assert!(core::mem::offset_of!(TaskView, state) == 124);
    assert!(core::mem::offset_of!(TaskView, handoff_pending) == 134);
    assert!(core::mem::size_of::<ExternalIrqFpuGuard>() == 24);
    assert!(core::mem::offset_of!(ExternalIrqFpuGuard, restore_task_state) == 16);
};

pub fn wakeup_checkpoint() -> u64 {
    // Preserve the original nested diagnostic admission used by IRQ dispatch.
    let flags = runtime::save_runtime();
    let value = read(|s| s.wakeup_reschedule_request_count);
    runtime::restore_runtime(flags);
    value
}

unsafe fn scheduler_log(event: u8, task: Task, a: u64, b: u64) {
    use crate::boot::log_route as l;
    match event {
        0 => {
            l::puts(b"[TIMER] deadline_queue=ordered idle=");
            l::puts(if a != 0 {
                b"one-shot"
            } else {
                b"periodic-fallback"
            });
            l::puts(b" backend=");
            l::puts(crate::platform::timer::backend_name());
            l::puts(b"\r\n");
        }
        1 => {
            l::puts(b"[SMP] productive cpu=");
            l::dec(a);
            l::puts(b" task=");
            let t = unsafe {
                (*task.cast::<crate::sched::task::task_types::Task>())
                    .core
                    .get()
            };
            let name = unsafe { core::ptr::addr_of!((*t).name).read() };
            l::puts(unsafe { core::slice::from_raw_parts(name.pointer, name.len) });
            l::puts(b" class=r4x\r\n");
        }
        2 => {
            let t = unsafe {
                (*task.cast::<crate::sched::task::task_types::Task>())
                    .core
                    .get()
            };
            l::puts(b"[SMP] switch boundary violation cpu=");
            l::dec(a);
            l::puts(b" task=");
            l::dec(u64::from(unsafe { core::ptr::addr_of!((*t).id).read() }));
            l::puts(b"/");
            let name = unsafe { core::ptr::addr_of!((*t).name).read() };
            l::puts(unsafe { core::slice::from_raw_parts(name.pointer, name.len) });
            l::puts(b" depth=");
            l::dec(b);
            l::puts(b" state=");
            l::puts(
                match unsafe { core::ptr::addr_of!((*t).state).read() } as u8 {
                    0 => b"unused",
                    1 => b"ready",
                    2 => b"running",
                    3 => b"blocked",
                    4 => b"dead",
                    _ => unreachable!(),
                },
            );
            l::puts(b"\r\n");
        }
        3 => {
            let t = unsafe {
                (*task.cast::<crate::sched::task::task_types::Task>())
                    .core
                    .get()
            };
            l::puts(b"TASK EXIT INVARIANT: owned synchronization remains id=");
            l::dec(u64::from(unsafe { core::ptr::addr_of!((*t).id).read() }));
            l::puts(b" locks=");
            l::dec(u64::from(unsafe {
                core::ptr::addr_of!((*t).held_lock_count).read()
            }));
            l::puts(b" unwind=");
            l::dec(u64::from(unsafe {
                core::ptr::addr_of!((*t).unwind_guard_count).read()
            }));
            l::puts(b" objects=");
            let held = unsafe { core::ptr::addr_of!((*t).held_locks).read() };
            let mut wrote = false;
            for h in held {
                if !h.active {
                    continue;
                }
                if wrote {
                    l::puts(b",");
                }
                l::puts(unsafe { core::slice::from_raw_parts(h.name.pointer, h.name.len) });
                l::puts(b"=");
                l::hex(h.object_id, 16);
                l::puts(b"@");
                l::dec(u64::from(h.rank));
                wrote = true;
            }
            if !wrote {
                l::puts(b"none");
            }
            l::puts(b"\r\n");
        }
        4 => {
            l::puts(b"TASK EXIT INVARIANT: transition rejected id=");
            let t = unsafe {
                (*task.cast::<crate::sched::task::task_types::Task>())
                    .core
                    .get()
            };
            l::dec(u64::from(unsafe { core::ptr::addr_of!((*t).id).read() }));
            l::puts(b"\r\n");
        }
        5 => l::puts(b"IRQ FPU GUARD task identity changed inside handler\r\n"),
        6 => l::puts(b"SCHEDPRIO FAIL busy-spawn\r\n"),
        7 => {
            let mut s = core::mem::MaybeUninit::uninit();
            unsafe {
                r4os_rust_scheduler_stats(s.as_mut_ptr());
            }
            let s = unsafe { s.assume_init() };
            l::puts(if a != 0 {
                b"SCHEDPRIO OK"
            } else {
                b"SCHEDPRIO FAIL"
            });
            l::puts(b" high_latency_max=");
            l::dec(b);
            l::puts(b" bar=3 picks_high=");
            l::dec(s.priority_picks_high);
            l::puts(b" picks_normal=");
            l::dec(s.priority_picks_normal);
            l::puts(b" rr_picks=");
            l::dec(s.priority_rr_picks);
            l::puts(b" selects=");
            l::dec(s.priority_selects);
            l::puts(b"\r\n");
        }
        _ => panic!("Invalid scheduler diagnostic"),
    }
}
