//! One scheduler state owner; runtime admission is mandatory after SMP start.
//! Counters, pending decisions and CPU slots have no foreign/live Rust borrow.
pub use super::scheduler_types::{CpuPreemptionStats, Stats, StructureStats};
use super::task_context;
use crate::{
    arch::{fpu, lapic, percpu, pit, runtime, smp_policy::MAX_CPUS},
    build_bridge::config,
    memory::owner,
    platform::timer,
};
use core::{cell::UnsafeCell, ffi::c_void, ptr};
const ROLE_COUNT: usize = 4;
const BUCKETS: usize = 5;
const WAIT_FOREVER: u64 = u64::MAX;
const TIMER_WAIT_OBJECT: u64 = 0x5449_4d45_5741_4954;
const QUANTUM: u64 = {
    let ticks = (30 * pit::DEFAULT_HZ) / 1000;
    if ticks == 0 { 1 } else { ticks as u64 }
};
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
}
impl CpuState {
    const EMPTY: Self = Self {
        current: ptr::null_mut(),
        idle: ptr::null_mut(),
        initialized: false,
        boot_depth: 0,
        pending: false,
    };
}
struct State {
    cpu: [CpuState; MAX_CPUS],
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
    safe_reschedule_point_count: u64,
    safe_reschedule_switch_count: u64,
    safe_reschedule_deferred_irq_count: u64,
    safe_reschedule_deferred_owner_count: u64,
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
        safe_reschedule_point_count: 0,
        safe_reschedule_switch_count: 0,
        safe_reschedule_deferred_irq_count: 0,
        safe_reschedule_deferred_owner_count: 0,
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
    #[cfg(not(r4os_owner_fixture))]
    fn r4os_context_switch(old_rsp: *mut u64, new_rsp: u64);
    fn r4os_zig_scheduler_log(event: u8, task: Task, a: u64, b: u64);
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
fn text_mutate(operation: u8, task: Task, a: u64, b: u64, text: *const u8, len: usize) -> u64 {
    unsafe { super::task::scheduler_mutate(operation, task, a, b, text, len) }
}
fn task_query(operation: u8, a: u64) -> Task {
    ptr::with_exposed_provenance_mut(query(operation, ptr::null_mut(), a) as usize)
}
fn log(event: u8, task: Task, a: u64, b: u64) {
    unsafe { r4os_zig_scheduler_log(event, task, a, b) }
}
unsafe fn switch_context(outgoing: *mut u64, incoming: u64) {
    #[cfg(r4os_owner_fixture)]
    {
        let _ = (outgoing, incoming);
        panic!("Physical context switch requires the kernel guest");
    }
    #[cfg(not(r4os_owner_fixture))]
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
fn preempt_depth() -> u32 {
    let task = current();
    if task.is_null() {
        local().boot_depth
    } else {
        view(task).preempt_depth
    }
}
fn send_reschedule(index: u32) {
    if index == percpu::current_index() || percpu::r4os_rust_percpu_control(13, index, 0, 0) == 0 {
        return;
    }
    let apic = percpu::r4os_rust_percpu_control(8, index, 0, 0);
    if apic != u64::MAX {
        lapic::r4os_rust_lapic_control(5, apic, 0xf0);
    }
}
fn init() -> bool {
    unsafe { task_context::bind(ptr::null_mut()) };
    if query(0, ptr::null_mut(), 0) == 0 {
        return false;
    }
    change(|s| *s = State::EMPTY);
    let first = task_query(1, 0);
    if first.is_null() {
        return false;
    }
    local_change(|s| {
        s.current = first;
        s.initialized = true
    });
    unsafe { task_context::bind(view(first).unwind_slot) };
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
        s.boot_depth = 0;
        s.pending = false;
        s.initialized = true
    });
    mutate(0, idle, 0, 0);
    unsafe { task_context::bind(view(idle).unwind_slot) };
    percpu::r4os_rust_percpu_control(9, index, 1, 0);
    true
}
pub fn secondary_loop() -> ! {
    runtime::r4os_rust_runtime_control(1, 0);
    loop {
        yield_task();
        wait_until_deadline();
    }
}
fn preempt_disable() {
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
fn preempt_enable() {
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
fn refresh_pending(running: Task) {
    if local().pending {
        set_pending(urgent(running));
    }
}
fn next_ready(force_priority: bool) -> Task {
    let ordinal = change(|s| {
        s.priority_selects = s.priority_selects.wrapping_add(1);
        s.priority_selects
    });
    if !force_priority && ordinal % 8 == 0 {
        change(|s| s.ready_candidate_visit_count = s.ready_candidate_visit_count.wrapping_add(1));
        let task = task_query(4, u64::from(percpu::current_index()));
        if !task.is_null() {
            change(|s| s.priority_rr_picks = s.priority_rr_picks.wrapping_add(1));
        }
        return task;
    }
    let mut best = ptr::null_mut();
    for rank in 0..ROLE_COUNT {
        change(|s| s.ready_candidate_visit_count = s.ready_candidate_visit_count.wrapping_add(1));
        let task = task_query(
            5,
            u64::from(percpu::current_index()) | ((rank as u64) << 32),
        );
        if !task.is_null() {
            best = task;
            break;
        }
    }
    if !best.is_null() {
        let item = view(best);
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
    best
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
    if !runtime::release_for_switch() {
        runtime::halt_forever();
    }
}
pub fn yield_task() {
    if !enabled() {
        return;
    }
    let flags = runtime::save_runtime();
    change(|s| s.yield_count = s.yield_count.wrapping_add(1));
    let now = tick();
    preempt_disable();
    let old = local().current;
    if old.is_null() {
        preempt_enable();
        runtime::restore_runtime(flags);
        return;
    }
    if view(old).state == 2 {
        restore_productive(old);
    }
    mutate(1, old, now, 0);
    let priority_wakeup = local().pending && urgent(old);
    let selected = next_ready(priority_wakeup);
    let next = if selected.is_null() {
        set_pending(false);
        if view(old).state == 2 {
            preempt_enable();
            runtime::restore_runtime(flags);
            return;
        }
        let idle = local().idle;
        if idle.is_null() {
            preempt_enable();
            runtime::restore_runtime(flags);
            return;
        }
        idle
    } else {
        selected
    };
    if next == old {
        mutate(0, old, 0, 0);
        preempt_enable();
        set_pending(false);
        runtime::restore_runtime(flags);
        return;
    }
    let old_view = view(old);
    if old_view.state == 2 {
        mutate(if old_view.cpu_idle { 3 } else { 2 }, old, now, 0);
    } else {
        mutate(4, old, 0, 0);
    }
    let latency = mutate(5, next, now, 0);
    record_ready(next, latency);
    mutate(0, next, 0, 0);
    mutate(6, old, 0, 0);
    mutate(7, next, 0, 0);
    note_productive(next);
    preempt_enable();
    local_change(|s| s.current = next);
    unsafe { task_context::bind(view(next).unwind_slot) };
    refresh_pending(next);
    require_switch(old);
    let outgoing = view(old).rsp_slot;
    let incoming = view(next).rsp;
    // No state reference or RAII owner spans this physical switch. Assembly
    // publishes outgoing RSP before releasing the unique runtime owner.
    unsafe { switch_context(outgoing, incoming) };
    runtime::restore_runtime(flags);
    mutate(17, ptr::null_mut(), 0, 0);
}
pub fn preempt_from_irq() {
    if !enabled() {
        return;
    }
    let now = tick();
    let old = local().current;
    if old.is_null() || view(old).state != 2 {
        return;
    }
    let wakeup = local().pending && urgent(old);
    let next = next_ready(wakeup);
    if next.is_null() {
        set_pending(false);
        return;
    }
    if next == old {
        mutate(0, old, 0, 0);
        set_pending(false);
        return;
    }
    mutate(2, old, now, 0);
    let latency = mutate(5, next, now, 0);
    record_ready(next, latency);
    mutate(0, next, 0, 0);
    mutate(6, old, 0, 0);
    mutate(7, next, 0, 0);
    note_productive(next);
    local_change(|s| s.current = next);
    unsafe { task_context::bind(view(next).unwind_slot) };
    if wakeup {
        change(|s| {
            s.wakeup_preemption_switch_count = s.wakeup_preemption_switch_count.wrapping_add(1)
        });
    }
    refresh_pending(next);
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
    preempt_disable();
    let running = local().current;
    if running.is_null() {
        preempt_enable();
        runtime::restore_runtime(flags);
        return ptr::null_mut();
    }
    let now = tick();
    let wake = if timeout == WAIT_FOREVER {
        0
    } else {
        timer::r4os_rust_timer_control(6, now, timeout)
    };
    let previous = query(8, ptr::null_mut(), 0);
    text_mutate(14, running, wake, object, reason, len);
    if percpu::current_index() != 0 && wake != 0 && wake < previous {
        send_reschedule(0);
    }
    change(|s| {
        s.sleep_count = s.sleep_count.wrapping_add(1);
        s.object_wait_count = s.object_wait_count.wrapping_add(1);
    });
    preempt_enable();
    runtime::restore_runtime(flags);
    running
}
fn park_blocked(task: Task) {
    let flags = runtime::save_runtime();
    runtime::restore_runtime(flags);
    while view(task).state == 3 {
        yield_task();
        if view(task).state == 3 {
            change(|s| s.idle_wait_count = s.idle_wait_count.wrapping_add(1));
            wait_until_deadline();
            runtime::restore_runtime(flags);
        }
    }
    if local().current == task && view(task).state == 1 {
        let latency = mutate(5, task, tick(), 0);
        record_ready(task, latency);
        mutate(0, task, 0, 0);
    }
    runtime::restore_runtime(flags);
}
fn notify_ready(task: Task) {
    let item = view(task);
    let target = item.home_cpu;
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
fn publish_created(task: Task) -> bool {
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
    if !urgent(running) {
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
fn safe_reschedule() -> bool {
    change(|s| s.safe_reschedule_point_count = s.safe_reschedule_point_count.wrapping_add(1));
    if !enabled() {
        return false;
    }
    if !owner::fault_allowed() {
        change(|s| {
            s.safe_reschedule_deferred_owner_count =
                s.safe_reschedule_deferred_owner_count.wrapping_add(1)
        });
        return false;
    }
    let flags = runtime::save_runtime();
    if flags & (1 << 9) == 0 {
        change(|s| {
            s.safe_reschedule_deferred_irq_count =
                s.safe_reschedule_deferred_irq_count.wrapping_add(1)
        });
        runtime::restore_runtime(flags);
        return false;
    }
    let running = local().current;
    if running.is_null() {
        runtime::restore_runtime(flags);
        return false;
    }
    let item = view(running);
    if item.state != 2 || ready_count() == 0 {
        set_pending(false);
        runtime::restore_runtime(flags);
        return false;
    }
    let priority = local().pending && urgent(running);
    if !priority {
        set_pending(false);
    }
    if !priority && since(tick(), item.last_scheduled_tick) < QUANTUM {
        runtime::restore_runtime(flags);
        return false;
    }
    if preempt_depth() != 0 || item.held_locks != 0 || item.handoff_pending {
        change(|s| {
            s.safe_reschedule_deferred_owner_count =
                s.safe_reschedule_deferred_owner_count.wrapping_add(1)
        });
        runtime::restore_runtime(flags);
        return false;
    }
    runtime::restore_runtime(flags);
    change(|s| s.safe_reschedule_switch_count = s.safe_reschedule_switch_count.wrapping_add(1));
    yield_task();
    true
}
fn has_other_ready() -> bool {
    let flags = runtime::save_runtime();
    let ready = enabled() && ready_count() != 0;
    runtime::restore_runtime(flags);
    ready
}
fn exit_current(retire: bool) -> ! {
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
    if due || ready_count() != 0 {
        runtime::restore_runtime(flags);
        return;
    }
    if bsp {
        timer::r4os_rust_timer_control(12, query(8, ptr::null_mut(), 0), 0);
    }
    if !runtime::release_for_switch() {
        runtime::halt_forever();
    }
    runtime::r4os_rust_runtime_control(13, 0);
    runtime::r4os_rust_runtime_control(0, 0);
    if bsp {
        let leave = runtime::save_runtime();
        timer::r4os_rust_timer_control(13, 0, 0);
        runtime::restore_runtime(leave);
    }
    runtime::restore_runtime(flags);
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
    if ready_count() == 0 {
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
    if !priority && ticks < QUANTUM {
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
    let result = timer_preemption(now, preemptible);
    warnings(now);
    result
}
pub fn on_reschedule(preemptible: bool) {
    let flags = runtime::save_runtime();
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
    if !fpu::restore_initial() {
        if restore {
            mutate(7, running, 0, 0);
        }
        return ExternalIrqFpuGuard::EMPTY;
    }
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
    }
    fpu::restore_initial();
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
        safe_reschedule_points: s.safe_reschedule_point_count,
        safe_reschedule_switches: s.safe_reschedule_switch_count,
        safe_reschedule_deferred_irq: s.safe_reschedule_deferred_irq_count,
        safe_reschedule_deferred_owner: s.safe_reschedule_deferred_owner_count,
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
        safe_reschedule_points: s.safe_reschedule_point_count,
        safe_reschedule_switches: s.safe_reschedule_switch_count,
        safe_reschedule_deferred_irq: s.safe_reschedule_deferred_irq_count,
        safe_reschedule_deferred_owner: s.safe_reschedule_deferred_owner_count,
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
