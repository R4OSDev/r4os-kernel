const task = @import("task.zig");
const task_context = @import("task_context.zig");
const config = @import("config");
const fpu = @import("../arch/x86_64/fpu.zig");
const interrupts = @import("../arch/x86_64/interrupts.zig");
const lapic = @import("../arch/x86_64/lapic.zig");
const percpu = @import("../arch/x86_64/percpu.zig");
const timer = @import("../kernel/timer.zig");
const k = @import("../kernel/log.zig");

extern fn r4os_context_switch(old_rsp: *u64, new_rsp: u64) callconv(.c) void;

var initialized = false;
var yield_count: u64 = 0;
var sleep_count: u64 = 0;
var wake_count: u64 = 0;
var idle_wait_count: u64 = 0;
var object_wait_count: u64 = 0;
var object_wake_count: u64 = 0;
var object_timeout_count: u64 = 0;
var object_cancel_count: u64 = 0;
var preempt_disable_call_count: u64 = 0;
var preempt_enable_call_count: u64 = 0;
var preempt_disable_underflow_count: u64 = 0;
var preempt_disable_max_depth: u32 = 0;
var preemption_simulation_tick_count: u64 = 0;
var preemption_eligible_tick_count: u64 = 0;
var preemption_deferred_disabled_count: u64 = 0;
var preemption_deferred_critical_count: u64 = 0;
var preemption_deferred_no_task_count: u64 = 0;
var preemption_deferred_no_ready_count: u64 = 0;
var preemption_deferred_quantum_count: u64 = 0;
var preemption_deferred_kernel_ip_count: u64 = 0;
var preemption_switch_tick_count: u64 = 0;
var preemption_quantum_expired_count: u64 = 0;
var preemption_app_code_tick_count: u64 = 0;
var timer_preemption_switches_by_cpu: [percpu.max_cpus]u64 = .{0} ** percpu.max_cpus;
var reschedule_ipi_switches_by_cpu: [percpu.max_cpus]u64 = .{0} ** percpu.max_cpus;
var long_running_warning_count: u64 = 0;
var starvation_warning_count: u64 = 0;
var ready_latency_sample_count: u64 = 0;
var ready_latency_total_ticks: u64 = 0;
var ready_latency_max_ticks: u64 = 0;
var ready_latency_last_ticks: u64 = 0;
var ready_waiting_max_ticks: u64 = 0;
var wait_object_total_ticks: u64 = 0;
var wait_object_max_ticks: u64 = 0;
var wait_object_last_ticks: u64 = 0;
var run_without_switch_max_ticks: u64 = 0;
var quantum_overrun_count: u64 = 0;
var quantum_overrun_max_ticks: u64 = 0;
var preemption_deferred_max_ticks: u64 = 0;
const RESCHEDULE_VECTOR: u8 = 0xF0;

const CpuSchedulerState = struct {
    current_task: ?*task.Task = null,
    idle_task: ?*task.Task = null,
    initialized: bool = false,
    boot_preempt_disable_depth: u32 = 0,
    reschedule_requested: bool = false,
};

var cpu_states: [percpu.max_cpus]CpuSchedulerState = .{CpuSchedulerState{}} ** percpu.max_cpus;

fn cpuState(index: u32) *CpuSchedulerState {
    const slot: usize = @intCast(if (index < percpu.max_cpus) index else 0);
    return &cpu_states[slot];
}

fn localState() *CpuSchedulerState {
    return cpuState(percpu.currentIndex());
}
var wakeup_reschedule_request_count: u64 = 0;
var wakeup_preemption_switch_count: u64 = 0;
var safe_reschedule_point_count: u64 = 0;
var safe_reschedule_switch_count: u64 = 0;
var safe_reschedule_deferred_irq_count: u64 = 0;
var safe_reschedule_deferred_owner_count: u64 = 0;
var ready_candidate_visit_count: u64 = 0;
var timeout_candidate_visit_count: u64 = 0;
var warning_candidate_visit_count: u64 = 0;
var timeout_storm_deferral_count: u64 = 0;

pub const WAIT_FOREVER: u64 = 0xFFFF_FFFF_FFFF_FFFF;
pub const timer_wait_object: u64 = 0x5449_4D45_5741_4954; // "TIMEWAIT"
pub const preemption_supported: u32 = 1;
pub const preemption_enabled: u32 = 1;
pub const preemption_test_mode: u32 = 0;
// 0.56.40: hz-neutral in ms definiert (bei 100 Hz identische
// Tick-Werte wie zuvor: 30 ms Quantum, 1 s/2 s Warnschwellen).
pub const preemption_quantum_ticks: u32 = @intCast(@max(1, (30 * timer.DEFAULT_HZ) / 1000));
pub const long_running_warn_ticks: u64 = @max(1, (1000 * @as(u64, timer.DEFAULT_HZ)) / 1000);
pub const starvation_warn_ticks: u64 = @max(1, (2000 * @as(u64, timer.DEFAULT_HZ)) / 1000);
const MAX_TIMEOUT_WAKEUPS_PER_IRQ: u32 = 64;
pub const ready_latency_bucket_count: usize = 5;
var ready_latency_role_samples: [task.role_count]u64 = .{0} ** task.role_count;
var ready_latency_role_buckets: [task.role_count][ready_latency_bucket_count]u64 =
    .{.{0} ** ready_latency_bucket_count} ** task.role_count;

pub const Stats = struct {
    initialized: bool = false,
    current_index: u32 = 0,
    yields: u64 = 0,
    sleeps: u64 = 0,
    wakes: u64 = 0,
    idle_waits: u64 = 0,
    object_waits: u64 = 0,
    object_wakes: u64 = 0,
    object_timeouts: u64 = 0,
    object_cancels: u64 = 0,
    ticks: u64 = 0,
    preemption_supported: u32 = preemption_supported,
    preemption_enabled: u32 = preemption_enabled,
    preemption_test_mode: u32 = preemption_test_mode,
    preempt_disable_depth: u32 = 0,
    preempt_disable_max_depth: u32 = 0,
    preempt_disable_calls: u64 = 0,
    preempt_enable_calls: u64 = 0,
    preempt_disable_underflows: u64 = 0,
    preemption_simulation_ticks: u64 = 0,
    preemption_eligible_ticks: u64 = 0,
    preemption_deferred_disabled: u64 = 0,
    preemption_deferred_critical: u64 = 0,
    preemption_deferred_no_task: u64 = 0,
    preemption_deferred_no_ready: u64 = 0,
    preemption_deferred_quantum: u64 = 0,
    preemption_deferred_kernel_ip: u64 = 0,
    preemption_switch_ticks: u64 = 0,
    preemption_quantum_ticks: u32 = preemption_quantum_ticks,
    preemption_quantum_expired: u64 = 0,
    preemption_app_code_ticks: u64 = 0,
    long_running_task_warnings: u64 = 0,
    starvation_warnings: u64 = 0,
    ready_latency_samples: u64 = 0,
    ready_latency_total_ticks: u64 = 0,
    ready_latency_max_ticks: u64 = 0,
    ready_latency_last_ticks: u64 = 0,
    ready_waiting_max_ticks: u64 = 0,
    wait_object_total_ticks: u64 = 0,
    wait_object_max_ticks: u64 = 0,
    wait_object_last_ticks: u64 = 0,
    run_without_switch_max_ticks: u64 = 0,
    quantum_overrun_count: u64 = 0,
    quantum_overrun_max_ticks: u64 = 0,
    preemption_deferred_max_ticks: u64 = 0,
    long_running_warn_threshold_ticks: u64 = long_running_warn_ticks,
    starvation_warn_threshold_ticks: u64 = starvation_warn_ticks,
    // 0.56.18: Prioritaets-Auswahlzaehler (Befund 4.1).
    priority_selects: u64 = 0,
    priority_picks_high: u64 = 0,
    priority_picks_normal: u64 = 0,
    priority_picks_low: u64 = 0,
    priority_rr_picks: u64 = 0,
    role_picks: [task.role_count]u64 = .{0} ** task.role_count,
    ready_latency_role_samples: [task.role_count]u64 = .{0} ** task.role_count,
    ready_latency_role_buckets: [task.role_count][ready_latency_bucket_count]u64 =
        .{.{0} ** ready_latency_bucket_count} ** task.role_count,
    safe_reschedule_points: u64 = 0,
    safe_reschedule_switches: u64 = 0,
    safe_reschedule_deferred_irq: u64 = 0,
    safe_reschedule_deferred_owner: u64 = 0,
};

pub const CpuPreemptionStats = struct {
    timer_switches: u64 = 0,
    reschedule_ipi_switches: u64 = 0,
};

pub fn cpuPreemptionStats(cpu_index: u32) CpuPreemptionStats {
    if (cpu_index >= percpu.max_cpus) return .{};
    const index: usize = @intCast(cpu_index);
    return .{
        .timer_switches = timer_preemption_switches_by_cpu[index],
        .reschedule_ipi_switches = reschedule_ipi_switches_by_cpu[index],
    };
}

pub fn init() bool {
    task_context.clear();
    if (task.count() == 0) return false;
    cpu_states = .{CpuSchedulerState{}} ** percpu.max_cpus;
    const state = localState();
    state.current_task = task.first() orelse return false;
    state.initialized = true;
    task_context.bind(&state.current_task.?.unwind_guard_count);
    initialized = true;
    yield_count = 0;
    sleep_count = 0;
    wake_count = 0;
    idle_wait_count = 0;
    object_wait_count = 0;
    object_wake_count = 0;
    object_timeout_count = 0;
    object_cancel_count = 0;
    state.boot_preempt_disable_depth = 0;
    preempt_disable_call_count = 0;
    preempt_enable_call_count = 0;
    preempt_disable_underflow_count = 0;
    preempt_disable_max_depth = 0;
    preemption_simulation_tick_count = 0;
    preemption_eligible_tick_count = 0;
    preemption_deferred_disabled_count = 0;
    preemption_deferred_critical_count = 0;
    preemption_deferred_no_task_count = 0;
    preemption_deferred_no_ready_count = 0;
    preemption_deferred_quantum_count = 0;
    preemption_deferred_kernel_ip_count = 0;
    preemption_switch_tick_count = 0;
    preemption_quantum_expired_count = 0;
    preemption_app_code_tick_count = 0;
    timer_preemption_switches_by_cpu = .{0} ** percpu.max_cpus;
    reschedule_ipi_switches_by_cpu = .{0} ** percpu.max_cpus;
    long_running_warning_count = 0;
    starvation_warning_count = 0;
    ready_latency_sample_count = 0;
    ready_latency_total_ticks = 0;
    ready_latency_max_ticks = 0;
    ready_latency_last_ticks = 0;
    ready_waiting_max_ticks = 0;
    wait_object_total_ticks = 0;
    wait_object_max_ticks = 0;
    wait_object_last_ticks = 0;
    run_without_switch_max_ticks = 0;
    quantum_overrun_count = 0;
    quantum_overrun_max_ticks = 0;
    preemption_deferred_max_ticks = 0;
    state.reschedule_requested = false;
    wakeup_reschedule_request_count = 0;
    wakeup_preemption_switch_count = 0;
    safe_reschedule_point_count = 0;
    safe_reschedule_switch_count = 0;
    safe_reschedule_deferred_irq_count = 0;
    safe_reschedule_deferred_owner_count = 0;
    ready_candidate_visit_count = 0;
    timeout_candidate_visit_count = 0;
    warning_candidate_visit_count = 0;
    timeout_storm_deferral_count = 0;
    ready_latency_role_samples = .{0} ** task.role_count;
    ready_latency_role_buckets = .{.{0} ** ready_latency_bucket_count} ** task.role_count;
    priority_selects = 0;
    priority_picks_high = 0;
    priority_picks_normal = 0;
    priority_picks_low = 0;
    priority_rr_picks = 0;
    role_picks = .{0} ** task.role_count;
    external_irq_fpu_guard_entries = 0;
    external_irq_fpu_guard_mismatches = 0;
    external_irq_fpu_guard_mismatch_reported = false;
    // 0.56.15: Recycle-Wachhund (Befund 13.2.3) - task.zig darf den Slot des
    // aktuell laufenden Tasks nie recyceln (exitCurrent laeuft auf seinem
    // Stack weiter, bis der Scheduler weggeschaltet hat).
    task.setCurrentProvider(current);
    const deadline_mode = timer.enableDeadlineScheduling();
    k.puts("[TIMER] deadline_queue=ordered idle=");
    k.puts(if (deadline_mode) "one-shot" else "periodic-fallback");
    k.puts(" backend=");
    k.puts(timer.backendName());
    k.puts("\r\n");
    return true;
}

pub fn prepareSecondary(index: u32) bool {
    if (!initialized or index == 0 or index >= percpu.max_cpus) return false;
    const state = cpuState(index);
    if (state.idle_task != null) return true;
    state.idle_task = task.createCpuIdleTask(index) orelse return false;
    return true;
}

pub fn initSecondary(index: u32) bool {
    if (!initialized or index == 0 or index >= percpu.max_cpus) return false;
    const state = cpuState(index);
    const idle = state.idle_task orelse return false;
    task_context.clear();
    state.current_task = idle;
    state.boot_preempt_disable_depth = 0;
    state.reschedule_requested = false;
    state.initialized = true;
    task.markRunning(idle);
    task_context.bind(&idle.unwind_guard_count);
    percpu.setSchedulerReady(index, true);
    return true;
}

pub fn secondaryLoop() noreturn {
    interrupts.enable();
    while (true) {
        // yield() performs the queue observation inside the owner boundary;
        // the wait helper then performs the final atomic no-work recheck.
        yield();
        waitForInterruptUntilNextDeadline();
    }
}

pub fn stats() Stats {
    const irq_flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(irq_flags);
    const state = localState();
    const diagnostic_ordinal = if (state.current_task) |running| task.ordinalOf(running) orelse 0 else 0;
    return .{
        .initialized = initialized,
        // Legacy ABI field only. Scheduler identity is the stable Task object;
        // 0.59.11 replaces this transient ordinal with the cursor snapshot API.
        .current_index = @intCast(@min(diagnostic_ordinal, @as(usize, 0xFFFF_FFFF))),
        .yields = yield_count,
        .sleeps = sleep_count,
        .wakes = wake_count,
        .idle_waits = idle_wait_count,
        .object_waits = object_wait_count,
        .object_wakes = object_wake_count,
        .object_timeouts = object_timeout_count,
        .object_cancels = object_cancel_count,
        .ticks = timer.tickCount(),
        .preempt_disable_depth = currentPreemptDepth(),
        .preempt_disable_max_depth = preempt_disable_max_depth,
        .preempt_disable_calls = preempt_disable_call_count,
        .preempt_enable_calls = preempt_enable_call_count,
        .preempt_disable_underflows = preempt_disable_underflow_count,
        .preemption_simulation_ticks = preemption_simulation_tick_count,
        .preemption_eligible_ticks = preemption_eligible_tick_count,
        .preemption_deferred_disabled = preemption_deferred_disabled_count,
        .preemption_deferred_critical = preemption_deferred_critical_count,
        .preemption_deferred_no_task = preemption_deferred_no_task_count,
        .preemption_deferred_no_ready = preemption_deferred_no_ready_count,
        .preemption_deferred_quantum = preemption_deferred_quantum_count,
        .preemption_deferred_kernel_ip = preemption_deferred_kernel_ip_count,
        .preemption_switch_ticks = preemption_switch_tick_count,
        .preemption_quantum_expired = preemption_quantum_expired_count,
        .preemption_app_code_ticks = preemption_app_code_tick_count,
        .long_running_task_warnings = long_running_warning_count,
        .starvation_warnings = starvation_warning_count,
        .ready_latency_samples = ready_latency_sample_count,
        .ready_latency_total_ticks = ready_latency_total_ticks,
        .ready_latency_max_ticks = ready_latency_max_ticks,
        .ready_latency_last_ticks = ready_latency_last_ticks,
        .ready_waiting_max_ticks = ready_waiting_max_ticks,
        .wait_object_total_ticks = wait_object_total_ticks,
        .wait_object_max_ticks = wait_object_max_ticks,
        .wait_object_last_ticks = wait_object_last_ticks,
        .run_without_switch_max_ticks = run_without_switch_max_ticks,
        .quantum_overrun_count = quantum_overrun_count,
        .quantum_overrun_max_ticks = quantum_overrun_max_ticks,
        .preemption_deferred_max_ticks = preemption_deferred_max_ticks,
        .long_running_warn_threshold_ticks = long_running_warn_ticks,
        .starvation_warn_threshold_ticks = starvation_warn_ticks,
        .priority_selects = priority_selects,
        .priority_picks_high = priority_picks_high,
        .priority_picks_normal = priority_picks_normal,
        .priority_picks_low = priority_picks_low,
        .priority_rr_picks = priority_rr_picks,
        .role_picks = role_picks,
        .ready_latency_role_samples = ready_latency_role_samples,
        .ready_latency_role_buckets = ready_latency_role_buckets,
        .safe_reschedule_points = safe_reschedule_point_count,
        .safe_reschedule_switches = safe_reschedule_switch_count,
        .safe_reschedule_deferred_irq = safe_reschedule_deferred_irq_count,
        .safe_reschedule_deferred_owner = safe_reschedule_deferred_owner_count,
    };
}

pub fn current() ?*task.Task {
    const state = localState();
    if (!initialized or !state.initialized) return null;
    return state.current_task;
}

// 0.56.40: Idle-Erkennung fuer die Exit-/Boot-Warteschleifen. true,
// sobald irgendein ANDERER Task ready ist - dann muss die Schleife
// sofort yielden statt zu hlt'en, sonst bremst jeder Rotationsbesuch
// das System um bis zu einen Tick.
pub fn hasOtherReadyTask() bool {
    const irq_flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(irq_flags);
    return initialized and localState().initialized and task.readyCountForCpu(percpu.currentIndex()) != 0;
}

pub const StructureStats = struct {
    ready_candidate_visits: u64 = 0,
    timeout_candidate_visits: u64 = 0,
    warning_candidate_visits: u64 = 0,
    timeout_storm_deferrals: u64 = 0,
    wakeup_reschedule_requests: u64 = 0,
    wakeup_preemption_switches: u64 = 0,
    safe_reschedule_points: u64 = 0,
    safe_reschedule_switches: u64 = 0,
    safe_reschedule_deferred_irq: u64 = 0,
    safe_reschedule_deferred_owner: u64 = 0,
    reschedule_pending: bool = false,
};

pub fn structureStats() StructureStats {
    const irq_flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(irq_flags);
    const state = localState();
    return .{
        .ready_candidate_visits = ready_candidate_visit_count,
        .timeout_candidate_visits = timeout_candidate_visit_count,
        .warning_candidate_visits = warning_candidate_visit_count,
        .timeout_storm_deferrals = timeout_storm_deferral_count,
        .wakeup_reschedule_requests = wakeup_reschedule_request_count,
        .wakeup_preemption_switches = wakeup_preemption_switch_count,
        .safe_reschedule_points = safe_reschedule_point_count,
        .safe_reschedule_switches = safe_reschedule_switch_count,
        .safe_reschedule_deferred_irq = safe_reschedule_deferred_irq_count,
        .safe_reschedule_deferred_owner = safe_reschedule_deferred_owner_count,
        .reschedule_pending = state.reschedule_requested,
    };
}

pub fn currentId() ?u32 {
    const running_task = current() orelse return null;
    return running_task.id;
}

pub fn currentName() ?[]const u8 {
    const running_task = current() orelse return null;
    return running_task.name;
}

pub const ExternalIrqFpuGuard = struct {
    interrupted_task: ?*task.Task = null,
    interrupted_generation: u64 = 0,
    restore_task_state: bool = false,
    armed: bool = false,
};

var external_irq_fpu_guard_entries: u64 = 0;
var external_irq_fpu_guard_mismatches: u64 = 0;
var external_irq_fpu_guard_mismatch_reported = false;

// An external R4D IRQ handler is not a task and therefore has no scheduler
// FPU slot of its own.  Save the interrupted R4X state before entering module
// code and give the handler a clean MXCSR/XMM/YMM baseline.  irq_router calls
// this immediately around the handler while interrupt delivery is disabled;
// handlers must not yield or switch tasks.
pub fn enterExternalIrqFpuGuard() ExternalIrqFpuGuard {
    if (fpu.activeStateBytes() == 0) return .{};

    const running = localState().current_task;
    var restore_task_state = false;
    if (running) |interrupted| {
        if (interrupted.uses_fpu and
            interrupted.fpu_state_valid and
            interrupted.fpu_state != null)
        {
            task.saveFpuState(interrupted);
            restore_task_state = true;
        }
    }

    if (!fpu.restoreInitialState()) {
        if (restore_task_state) task.restoreFpuState(running.?);
        return .{};
    }
    external_irq_fpu_guard_entries +%= 1;
    return .{
        .interrupted_task = running,
        .interrupted_generation = if (running) |interrupted| interrupted.generation else 0,
        .restore_task_state = restore_task_state,
        .armed = true,
    };
}

pub fn leaveExternalIrqFpuGuard(guard: ExternalIrqFpuGuard) void {
    if (!guard.armed) return;

    if (guard.restore_task_state) {
        if (guard.interrupted_task) |interrupted| {
            if (localState().current_task == interrupted and
                interrupted.generation == guard.interrupted_generation and
                interrupted.uses_fpu and
                interrupted.fpu_state_valid and
                interrupted.fpu_state != null)
            {
                task.restoreFpuState(interrupted);
                return;
            }
        }
        external_irq_fpu_guard_mismatches +%= 1;
        if (!external_irq_fpu_guard_mismatch_reported) {
            external_irq_fpu_guard_mismatch_reported = true;
            k.puts("IRQ FPU GUARD task identity changed inside handler\r\n");
        }
    }

    // Pure soft-float kernel tasks have no task state to restore.  Reset the
    // hardware nevertheless so a module handler cannot leak MXCSR/SIMD state
    // into the next unguarded kernel path.
    _ = fpu.restoreInitialState();
}

pub fn preemptDisable() void {
    const state = localState();
    preempt_disable_call_count +%= 1;
    if (current()) |running_task| {
        const depth = task.recordPreemptDisable(running_task);
        if (depth > preempt_disable_max_depth) preempt_disable_max_depth = depth;
        return;
    }
    state.boot_preempt_disable_depth +|= 1;
    if (state.boot_preempt_disable_depth > preempt_disable_max_depth) {
        preempt_disable_max_depth = state.boot_preempt_disable_depth;
    }
}

pub fn preemptEnable() void {
    const state = localState();
    preempt_enable_call_count +%= 1;
    if (current()) |running_task| {
        if (!task.recordPreemptEnable(running_task)) {
            preempt_disable_underflow_count +%= 1;
        }
        return;
    }
    if (state.boot_preempt_disable_depth == 0) {
        preempt_disable_underflow_count +%= 1;
        return;
    }
    state.boot_preempt_disable_depth -= 1;
}

pub fn yield() void {
    const state = localState();
    if (!initialized or !state.initialized) return;
    const irq_flags = interrupts.saveAndDisableRuntime();
    yield_count +%= 1;
    const now = timer.tickCount();

    preemptDisable();
    const old = state.current_task orelse {
        preemptEnable();
        interrupts.restore(irq_flags);
        return;
    };
    // A productive BSP can observe the scheduler without switching tasks.
    // Such a no-op yield must still close a delayed idle one-shot handoff;
    // otherwise the single event expires and later timed waits never get a
    // periodic IRQ.
    if (old.state == .running) restoreProductiveTimer(old);
    task.recordYield(old, now);
    const priority_wakeup = state.reschedule_requested and task.hasMoreUrgentReady(percpu.currentIndex(), old);
    const selected = nextReadyTask(priority_wakeup);
    const next_task = selected orelse blk: {
        state.reschedule_requested = false;
        if (old.state == .running) {
            preemptEnable();
            interrupts.restore(irq_flags);
            return;
        }
        break :blk state.idle_task orelse {
            preemptEnable();
            interrupts.restore(irq_flags);
            return;
        };
    };
    // A task whose remote wake raced its physical block boundary used to be
    // selectable from its own queue. Never restore an older context of the
    // stack that is executing this scheduler invocation.
    if (next_task == old) {
        task.markRunning(old);
        preemptEnable();
        state.reschedule_requested = false;
        interrupts.restore(irq_flags);
        return;
    }
    if (old.state == .running) {
        if (old.cpu_idle) task.parkCpuIdle(old) else task.markReady(old, now);
    } else {
        task.noteSwitchedOut(old);
    }
    recordReadyLatency(next_task, task.recordScheduled(next_task, now));
    task.markRunning(next_task);
    task.saveFpuState(old);
    task.restoreFpuState(next_task);
    noteProductiveTask(next_task);
    preemptEnable();
    state.current_task = next_task;
    task_context.bind(&next_task.unwind_guard_count);
    refreshRescheduleRequest(next_task);
    requireSwitchBoundary(old);
    r4os_context_switch(&old.rsp, next_task.rsp);
    interrupts.restore(irq_flags);
    _ = task.reapDeferred();
}

pub fn preemptFromIrq() void {
    const state = localState();
    if (!initialized or !state.initialized) return;
    const now = timer.tickCount();
    const old = state.current_task orelse return;
    if (old.state != .running) return;

    const wakeup_switch = state.reschedule_requested and task.hasMoreUrgentReady(percpu.currentIndex(), old);
    const next_task = nextReadyTask(wakeup_switch) orelse {
        state.reschedule_requested = false;
        return;
    };
    if (next_task == old) {
        task.markRunning(old);
        state.reschedule_requested = false;
        return;
    }

    task.markReady(old, now);
    recordReadyLatency(next_task, task.recordScheduled(next_task, now));
    task.markRunning(next_task);
    task.saveFpuState(old);
    task.restoreFpuState(next_task);
    noteProductiveTask(next_task);
    state.current_task = next_task;
    task_context.bind(&next_task.unwind_guard_count);
    if (wakeup_switch) wakeup_preemption_switch_count +%= 1;
    refreshRescheduleRequest(next_task);
    requireSwitchBoundary(old);
    r4os_context_switch(&old.rsp, next_task.rsp);
}

fn requireSwitchBoundary(outgoing: *const task.Task) void {
    if (!interrupts.runtimeSerializationEnabled()) return;
    const depth = percpu.runtimeCriticalDepth().*;
    if (depth == 1) return;
    k.puts("[SMP] switch boundary violation cpu=");
    k.putDec(percpu.currentIndex());
    k.puts(" task=");
    k.putDec(outgoing.id);
    k.puts("/");
    k.puts(outgoing.name);
    k.puts(" depth=");
    k.putDec(depth);
    k.puts(" state=");
    k.puts(task.stateName(outgoing.state));
    k.puts("\r\n");
    interrupts.haltForever();
}

// Called by r4os_context_switch after the outgoing RSP is durable and while
// running below the incoming saved context.  This is the SMP publication
// boundary: ready tasks cannot be selected remotely before their stack image
// is complete.
export fn r4os_finish_context_switch() callconv(.c) void {
    if (!interrupts.releaseRuntimeForContextSwitch()) interrupts.haltForever();
}

pub fn sleepTicks(ticks: u64) void {
    sleepTicksWithReason(ticks, "sleep");
}

pub fn sleepTicksWithReason(ticks: u64, reason: []const u8) void {
    if (ticks == 0) {
        yield();
        return;
    }

    const blocked_task = blockCurrent(timer_wait_object, ticks, reason) orelse return;
    parkBlocked(blocked_task);
}

pub fn blockCurrent(object: u64, timeout_ticks: u64, reason: []const u8) ?*task.Task {
    const state = localState();
    if (!initialized or !state.initialized) return null;
    const irq_flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(irq_flags);
    preemptDisable();
    defer preemptEnable();
    const running = state.current_task orelse return null;
    const now = timer.tickCount();
    const wake_tick = if (timeout_ticks == WAIT_FOREVER) 0 else timer.deadlineAfter(now, timeout_ticks);
    task.beginWait(running, wake_tick, reason, object);
    sleep_count +%= 1;
    object_wait_count +%= 1;
    return running;
}

pub fn parkBlocked(blocked_task: *task.Task) void {
    // A blocking caller enters with its own exact interrupt state. The idle
    // wait must temporarily open IRQs so timer/object wakeups can arrive, but
    // it must not leak the trailing CLI to the resumed task. In particular,
    // the block worker services runtime USB I/O after Event.waitResult(); an
    // IF=0 leak there turns xHCI's tick deadline into its short CPU guard.
    const park_irq_flags = interrupts.saveAndDisableRuntime();
    interrupts.restore(park_irq_flags);
    defer interrupts.restore(park_irq_flags);
    while (blocked_task.state == .blocked) {
        yield();
        if (blocked_task.state == .blocked) {
            idle_wait_count +%= 1;
            waitForInterruptUntilNextDeadline();
            interrupts.restore(park_irq_flags);
        }
    }
    if (localState().current_task == blocked_task and blocked_task.state == .ready) {
        recordReadyLatency(blocked_task, task.recordScheduled(blocked_task, timer.tickCount()));
        task.markRunning(blocked_task);
    }
}

pub fn wakeTask(target: *task.Task, result: task.WaitResult) bool {
    const irq_flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(irq_flags);
    preemptDisable();
    defer preemptEnable();
    if (target.state != .blocked) return false;
    const wait_ticks = task.finishWait(target, result);
    recordObjectWaitLatency(wait_ticks);
    wake_count +%= 1;
    switch (result) {
        .signaled => object_wake_count +%= 1,
        .timeout => object_timeout_count +%= 1,
        .cancelled => object_cancel_count +%= 1,
        else => {},
    }
    const target_cpu: u32 = target.home_cpu;
    const target_state = cpuState(target_cpu);
    if (target_cpu != percpu.currentIndex() and percpu.isSchedulable(target_cpu)) {
        target_state.reschedule_requested = true;
        wakeup_reschedule_request_count +%= 1;
        sendRescheduleToCpu(target_cpu);
    } else if (target_state.current_task) |running| {
        if (running.state == .running and task.dispatchRank(target) < task.dispatchRank(running)) {
            target_state.reschedule_requested = true;
            wakeup_reschedule_request_count +%= 1;
        }
    }
    return true;
}

// Publish a fully constructed blocked task and notify its selected CPU. This
// is distinct from wakeTask(): the task has never entered a WaitQueue and
// therefore has no wait result to complete. Remote notification is required
// because an AP may already be sleeping with an idle one-shot timer.
pub fn publishCreatedTask(target: *task.Task) bool {
    const irq_flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(irq_flags);
    preemptDisable();
    defer preemptEnable();
    if (target.state != .blocked or target.wait_object != 0 or target.wake_tick != 0) return false;
    task.markReady(target, timer.tickCount());
    const target_cpu: u32 = target.home_cpu;
    const target_state = cpuState(target_cpu);
    if (target_cpu != percpu.currentIndex() and percpu.isSchedulable(target_cpu)) {
        target_state.reschedule_requested = true;
        wakeup_reschedule_request_count +%= 1;
        sendRescheduleToCpu(target_cpu);
    } else if (target_state.current_task) |running| {
        if (running.state == .running and task.dispatchRank(target) < task.dispatchRank(running)) {
            target_state.reschedule_requested = true;
            wakeup_reschedule_request_count +%= 1;
        }
    }
    return true;
}

// A wake only publishes ready state while the wait queue still owns its
// critical section. IRQ dispatch calls this after all handlers and EOIs, so a
// higher-priority task can take over without switching inside a queue lock.
pub fn preemptPendingWake(preemptible_instruction_pointer: bool) bool {
    const state = localState();
    if (!state.reschedule_requested or !initialized or !state.initialized) return false;
    const running = state.current_task orelse return false;
    if (running.state != .running) return false;
    if (!task.hasMoreUrgentReady(percpu.currentIndex(), running)) {
        state.reschedule_requested = false;
        return false;
    }
    const scheduled_ticks = ticksSince(timer.tickCount(), running.last_scheduled_tick);
    preemption_eligible_tick_count +%= 1;
    task.recordPreemptionProbe(running);
    if (currentPreemptDepth() != 0) {
        preemption_deferred_critical_count +%= 1;
        task.recordPreemptionDeferred(running, scheduled_ticks);
        recordPreemptionDeferredWindow(scheduled_ticks);
        return false;
    }
    if (!preemptible_instruction_pointer) {
        preemption_deferred_kernel_ip_count +%= 1;
        task.recordPreemptionDeferred(running, scheduled_ticks);
        recordPreemptionDeferredWindow(scheduled_ticks);
        return false;
    }
    if (preemption_enabled == 0) {
        preemption_deferred_disabled_count +%= 1;
        task.recordPreemptionDeferred(running, scheduled_ticks);
        recordPreemptionDeferredWindow(scheduled_ticks);
        return false;
    }
    preemption_app_code_tick_count +%= 1;
    preemption_switch_tick_count +%= 1;
    return true;
}

// Explicit owner-safe return boundary for synchronous kernel producers. It is
// deliberately not called by generic WaitQueue code: that code may still be
// nested inside an endpoint, registry, or device owner. Callers place this
// only after releasing their owner and lock state. IRQ producers use the
// separate post-handler/EOI decision in idt.zig.
pub fn safeReschedulePoint() bool {
    const state = localState();
    safe_reschedule_point_count +%= 1;
    if (!state.reschedule_requested or !initialized or !state.initialized or preemption_enabled == 0) return false;

    const irq_flags = interrupts.saveAndDisableRuntime();
    if (!interrupts.wereEnabled(irq_flags)) {
        safe_reschedule_deferred_irq_count +%= 1;
        interrupts.restore(irq_flags);
        return false;
    }
    const running = state.current_task orelse {
        interrupts.restore(irq_flags);
        return false;
    };
    if (running.state != .running or !task.hasMoreUrgentReady(percpu.currentIndex(), running)) {
        state.reschedule_requested = false;
        interrupts.restore(irq_flags);
        return false;
    }
    if (currentPreemptDepth() != 0 or running.held_lock_count != 0 or running.wait_handoff_guard_pending) {
        safe_reschedule_deferred_owner_count +%= 1;
        interrupts.restore(irq_flags);
        return false;
    }
    interrupts.restore(irq_flags);

    safe_reschedule_switch_count +%= 1;
    yield();
    return true;
}

pub fn exitCurrent() noreturn {
    exitCurrentImpl(false);
}

// A task that completes its own epilogue may transfer Task storage to the
// scheduler reaper before the terminal switch. Any external execution-owner
// record remains stable until the exact Task generation has disappeared.
pub fn exitCurrentAndRetire() noreturn {
    exitCurrentImpl(true);
}

fn exitCurrentImpl(retire: bool) noreturn {
    const irq_flags = interrupts.saveAndDisableRuntime();
    preemptDisable();
    if (current()) |t| {
        if (t.held_lock_count != 0 or t.unwind_guard_count != 0) {
            k.puts("TASK EXIT INVARIANT: owned synchronization remains id=");
            k.putDec(t.id);
            k.puts(" locks=");
            k.putDec(t.held_lock_count);
            k.puts(" unwind=");
            k.putDec(t.unwind_guard_count);
            k.puts(" objects=");
            var lock_index: usize = 0;
            var wrote_lock = false;
            while (lock_index < t.held_locks.len) : (lock_index += 1) {
                const held = t.held_locks[lock_index];
                if (!held.active) continue;
                if (wrote_lock) k.puts(",");
                k.puts(held.name);
                k.puts("=");
                k.putHex(held.object_id, 16);
                k.puts("@");
                k.putDec(held.rank);
                wrote_lock = true;
            }
            if (!wrote_lock) k.puts("none");
            k.puts("\r\n");
            interrupts.haltForever();
        }
        if (!task.finishCurrentForExit(t, retire)) {
            k.puts("TASK EXIT INVARIANT: transition rejected id=");
            k.putDec(t.id);
            k.puts("\r\n");
            interrupts.haltForever();
        }
    }
    preemptEnable();
    interrupts.restore(irq_flags);
    // A terminal task must cross the physical switch boundary even when its
    // local runqueue is empty. In that case yield() selects this CPU's idle
    // task, clears running_cpu and makes deferred teardown possible.
    yield();
    // 0.56.40: NIE heiss spinnen. yield() restauriert die IF-Flags des
    // jeweiligen Task-Eintritts - ein Zombie, der den Exit-Pfad mit
    // IF=0 erreicht, reichte das im yield-Ring endlos weiter: sobald
    // ALLE anderen Tasks auf den Timer warteten, stand das System mit
    // 100% CPU und toten Timer-IRQs (SLEEP-Haenger-Befund, RIP-Beweis
    // yield/nextReadyIndex mit RFL.IF=0). Deshalb bei leerer
    // Ready-Menge Interrupts explizit oeffnen und hlt'en. hlt NUR im
    // Idle-Fall: unkonditional nach jedem yield kostete jeder
    // Rotationsbesuch bis zu einen Tick und wuergte I/O-lastige
    // Phasen ab (FSDIAG-Smoke-Watchdog-Befund).
    while (true) {
        if (hasOtherReadyTask()) {
            yield();
        } else {
            waitForInterruptUntilNextDeadline();
        }
    }
}

fn waitForInterruptUntilNextDeadline() void {
    const irq_flags = interrupts.saveAndDisableRuntime();
    // Recheck after joining the cross-CPU owner boundary.  A remote producer
    // cannot publish work between this check and the lock release below; its
    // IPI is then consumed by the adjacent STI/HLT sequence.
    if (task.readyCountForCpu(percpu.currentIndex()) != 0) {
        interrupts.restore(irq_flags);
        return;
    }
    const bsp = percpu.currentIndex() == 0;
    if (bsp) _ = timer.enterIdleDeadline(task.minWakeTick());
    if (!interrupts.releaseRuntimeForContextSwitch()) interrupts.haltForever();
    interrupts.enableAndWaitForInterrupt();
    interrupts.disable();
    if (bsp) {
        const leave_flags = interrupts.saveAndDisableRuntime();
        _ = timer.leaveIdleDeadline();
        interrupts.restore(leave_flags);
    }
    interrupts.restore(irq_flags);
}

pub fn onTick(now: u64, preemptible_instruction_pointer: bool) bool {
    const state = localState();
    if (initialized and state.initialized) {
        if (state.current_task) |running| {
            if (running.state == .running) {
                // A one-shot may fire after IRQ dispatch has already handed
                // the BSP to productive work. Restore the periodic source in
                // the IRQ that consumes that final one-shot event.
                restoreProductiveTimer(running);
                const run_ticks = task.recordRunTick(running, now);
                recordRunWindow(run_ticks);
                recordQuantumOverrun(run_ticks);
            }
        }
    }
    // The timed projection is ordered, so IRQ work stops at the first future
    // deadline. A simultaneous-deadline storm is drained in bounded batches;
    // the periodic active mode or the next one-shot checkpoint continues it.
    var timeout_wakeups: u32 = 0;
    while (timeout_wakeups < MAX_TIMEOUT_WAKEUPS_PER_IRQ) : (timeout_wakeups += 1) {
        const candidate = task.firstTimedWait() orelse break;
        timeout_candidate_visit_count +%= 1;
        if (candidate.wake_tick > now) break;
        _ = wakeTask(candidate, .timeout);
    }
    if (task.firstTimedWait()) |candidate| {
        if (candidate.wake_tick <= now) timeout_storm_deferral_count +%= 1;
    }
    const should_preempt = runTimerPreemption(now, preemptible_instruction_pointer);
    recordRuntimeWarnings(now);
    return should_preempt;
}

pub fn onSecondaryTick(now: u64, preemptible_instruction_pointer: bool) bool {
    const state = localState();
    if (!initialized or !state.initialized) return false;
    if (state.current_task) |running| {
        if (running.state == .running and !running.cpu_idle) {
            const run_ticks = task.recordRunTick(running, now);
            recordRunWindow(run_ticks);
            recordQuantumOverrun(run_ticks);
        }
    }
    const should_preempt = runTimerPreemption(now, preemptible_instruction_pointer);
    recordRuntimeWarnings(now);
    return should_preempt;
}

pub fn onRescheduleIpi(preemptible_instruction_pointer: bool) void {
    const irq_flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(irq_flags);
    if (preemptPendingWake(preemptible_instruction_pointer)) {
        const cpu_index: usize = @intCast(percpu.currentIndex());
        reschedule_ipi_switches_by_cpu[cpu_index] +%= 1;
        preemptFromIrq();
    }
}

pub fn dumpCurrent() void {
    k.puts("  Scheduler current: ");
    if (current()) |t| {
        k.puts("#");
        k.putDec(t.id);
        k.puts(" ");
        k.puts(t.name);
    } else {
        k.puts("none");
    }
    k.puts("\r\n");
}

pub fn dumpStatus() void {
    const state = localState();
    k.puts("Scheduler status\r\n");
    k.puts("  Initialized: ");
    k.puts(if (initialized) "yes" else "no");
    k.puts(" current_index=");
    if (state.current_task) |running| {
        k.putDec(task.ordinalOf(running) orelse 0);
    } else {
        k.putDec(0);
    }
    k.puts(" ticks=");
    k.putDec(timer.tickCount());
    k.puts("\r\n");
    k.puts("  Counters: yields=");
    k.putDec(yield_count);
    k.puts(" sleeps=");
    k.putDec(sleep_count);
    k.puts(" wakes=");
    k.putDec(wake_count);
    k.puts(" idle_waits=");
    k.putDec(idle_wait_count);
    k.puts(" object_waits=");
    k.putDec(object_wait_count);
    k.puts(" object_wakes=");
    k.putDec(object_wake_count);
    k.puts(" object_timeouts=");
    k.putDec(object_timeout_count);
    k.puts(" object_cancels=");
    k.putDec(object_cancel_count);
    k.puts("\r\n");
    k.puts("  Preemption: supported=");
    k.putDec(preemption_supported);
    k.puts(" enabled=");
    k.putDec(preemption_enabled);
    k.puts(" test=");
    k.putDec(preemption_test_mode);
    k.puts(" depth=");
    k.putDec(currentPreemptDepth());
    k.puts(" max=");
    k.putDec(preempt_disable_max_depth);
    k.puts(" sim=");
    k.putDec(preemption_simulation_tick_count);
    k.puts(" eligible=");
    k.putDec(preemption_eligible_tick_count);
    k.puts(" disabled=");
    k.putDec(preemption_deferred_disabled_count);
    k.puts(" critical=");
    k.putDec(preemption_deferred_critical_count);
    k.puts(" no_ready=");
    k.putDec(preemption_deferred_no_ready_count);
    k.puts(" quantum=");
    k.putDec(preemption_deferred_quantum_count);
    k.puts(" kernel_ip=");
    k.putDec(preemption_deferred_kernel_ip_count);
    k.puts(" long=");
    k.putDec(long_running_warning_count);
    k.puts(" starve=");
    k.putDec(starvation_warning_count);
    k.puts(" ready_max=");
    k.putDec(ready_latency_max_ticks);
    k.puts(" wait_max=");
    k.putDec(wait_object_max_ticks);
    k.puts(" run_max=");
    k.putDec(run_without_switch_max_ticks);
    k.puts("\r\n");
    const deadline = timer.deadlineStats();
    k.puts("  Timer deadlines: mode=");
    k.puts(timer.deadlineModeName());
    k.puts(" irqs=");
    k.putDec(deadline.timer_irqs);
    k.puts(" idle_entries=");
    k.putDec(deadline.idle_entries);
    k.puts(" arms=");
    k.putDec(deadline.one_shot_arms);
    k.puts(" cancels=");
    k.putDec(deadline.cancels);
    k.puts(" storm_deferrals=");
    k.putDec(timeout_storm_deferral_count);
    k.puts("\r\n");
    dumpCurrent();
    task.dump();
}

fn currentPreemptDepth() u32 {
    if (current()) |running_task| return running_task.preempt_disable_depth;
    return localState().boot_preempt_disable_depth;
}

fn runTimerPreemption(now: u64, preemptible_instruction_pointer: bool) bool {
    const state = localState();
    preemption_simulation_tick_count +%= 1;
    if (!initialized) {
        preemption_deferred_no_task_count +%= 1;
        return false;
    }
    const running = state.current_task orelse {
        preemption_deferred_no_task_count +%= 1;
        return false;
    };
    if (running.state != .running) {
        preemption_deferred_no_task_count +%= 1;
        return false;
    }
    // This is only an eligibility probe. It must not consume a priority
    // selection or an anti-starvation turn; the IRQ switch performs the one
    // authoritative ready-task selection afterwards.
    if (task.readyCountForCpu(percpu.currentIndex()) == 0) {
        state.reschedule_requested = false;
        preemption_deferred_no_ready_count +%= 1;
        return false;
    }
    const priority_wakeup = state.reschedule_requested and task.hasMoreUrgentReady(percpu.currentIndex(), running);
    if (state.reschedule_requested and !priority_wakeup) state.reschedule_requested = false;
    const scheduled_ticks = ticksSince(now, running.last_scheduled_tick);
    if (!priority_wakeup and scheduled_ticks < @as(u64, preemption_quantum_ticks)) {
        preemption_deferred_quantum_count +%= 1;
        task.recordPreemptionDeferred(running, scheduled_ticks);
        recordPreemptionDeferredWindow(scheduled_ticks);
        return false;
    }

    if (!priority_wakeup) preemption_quantum_expired_count +%= 1;
    preemption_eligible_tick_count +%= 1;
    task.recordPreemptionProbe(running);
    if (currentPreemptDepth() != 0) {
        preemption_deferred_critical_count +%= 1;
        task.recordPreemptionDeferred(running, scheduled_ticks);
        recordPreemptionDeferredWindow(scheduled_ticks);
        return false;
    }
    if (!preemptible_instruction_pointer) {
        preemption_deferred_kernel_ip_count +%= 1;
        task.recordPreemptionDeferred(running, scheduled_ticks);
        recordPreemptionDeferredWindow(scheduled_ticks);
        return false;
    }
    preemption_app_code_tick_count +%= 1;
    if (preemption_enabled == 0) {
        preemption_deferred_disabled_count +%= 1;
        task.recordPreemptionDeferred(running, scheduled_ticks);
        recordPreemptionDeferredWindow(scheduled_ticks);
        return false;
    }

    preemption_switch_tick_count +%= 1;
    timer_preemption_switches_by_cpu[@intCast(percpu.currentIndex())] +%= 1;
    return true;
}

fn recordRuntimeWarnings(now: u64) void {
    const state = localState();
    if (!initialized) return;
    if (state.current_task) |running| {
        if (running.state == .running and !running.cpu_idle) {
            const since = ticksSince(now, running.last_scheduled_tick);
            if (since >= long_running_warn_ticks and
                ticksSince(now, running.last_long_run_warning_tick) >= long_running_warn_ticks)
            {
                task.recordLongRunWarning(running, now);
                long_running_warning_count +%= 1;
            }
        }
    }

    if ((now & 0xF) != 0) return;
    // 0.56.13 (Befund 4.4): Starvation-Scan nur mit Metrics (-Dmetrics).
    if (comptime !config.enable_metrics) return;
    var cursor = task.firstReady(percpu.currentIndex());
    while (cursor) |candidate| : (cursor = task.nextReady(candidate)) {
        warning_candidate_visit_count +%= 1;
        const base_tick = if (candidate.ready_since_tick != 0)
            candidate.ready_since_tick
        else
            candidate.created_tick;
        const waiting_ticks = ticksSince(now, base_tick);
        if (waiting_ticks > ready_waiting_max_ticks) ready_waiting_max_ticks = waiting_ticks;
        if (waiting_ticks < starvation_warn_ticks) continue;
        if (ticksSince(now, candidate.last_starvation_warning_tick) < starvation_warn_ticks) continue;
        task.recordStarvationWarning(candidate, now);
        starvation_warning_count +%= 1;
    }
}

fn readyLatencyBucket(latency: u64) usize {
    return if (latency == 0)
        0
    else if (latency == 1)
        1
    else if (latency <= 3)
        2
    else if (latency <= 7)
        3
    else
        4;
}

fn recordReadyLatency(selected: *const task.Task, latency: u64) void {
    ready_latency_sample_count +%= 1;
    ready_latency_total_ticks +%= latency;
    ready_latency_last_ticks = latency;
    if (latency > ready_latency_max_ticks) ready_latency_max_ticks = latency;
    const role_index: usize = @intFromEnum(selected.role);
    ready_latency_role_samples[role_index] +%= 1;
    ready_latency_role_buckets[role_index][readyLatencyBucket(latency)] +%= 1;
}

fn recordObjectWaitLatency(wait_ticks: u64) void {
    wait_object_total_ticks +%= wait_ticks;
    wait_object_last_ticks = wait_ticks;
    if (wait_ticks > wait_object_max_ticks) wait_object_max_ticks = wait_ticks;
}

fn recordRunWindow(run_ticks: u64) void {
    if (run_ticks > run_without_switch_max_ticks) run_without_switch_max_ticks = run_ticks;
}

fn recordQuantumOverrun(run_ticks: u64) void {
    const quantum = @as(u64, preemption_quantum_ticks);
    if (quantum == 0 or run_ticks <= quantum) return;
    const overrun = run_ticks - quantum;
    quantum_overrun_count +%= 1;
    if (overrun > quantum_overrun_max_ticks) quantum_overrun_max_ticks = overrun;
}

fn recordPreemptionDeferredWindow(scheduled_ticks: u64) void {
    if (scheduled_ticks > preemption_deferred_max_ticks) {
        preemption_deferred_max_ticks = scheduled_ticks;
    }
}

fn ticksSince(now: u64, then: u64) u64 {
    if (then == 0 or now < then) return 0;
    return now - then;
}

// 0.56.18: Prioritaetsbewusste Auswahl (Befund 4.1). Beste Klasse gewinnt;
// innerhalb der Klasse liefert die Ready-FIFO Round-Robin. Anti-Starvation:
// jede 8. Auswahl nimmt den FIFO-Kopf unabhaengig von der Prioritaet, damit
// NORMAL/LOW unter HIGH-Dauerlast garantiert drankommen.
const PRIORITY_RR_INTERVAL: u64 = 8;

var priority_selects: u64 = 0;
var priority_picks_high: u64 = 0;
var priority_picks_normal: u64 = 0;
var priority_picks_low: u64 = 0;
var priority_rr_picks: u64 = 0;
var role_picks: [task.role_count]u64 = .{0} ** task.role_count;
fn refreshRescheduleRequest(running: *task.Task) void {
    const state = localState();
    if (state.reschedule_requested) {
        state.reschedule_requested = task.hasMoreUrgentReady(percpu.currentIndex(), running);
    }
}

fn plainNextReadyTask() ?*task.Task {
    ready_candidate_visit_count +%= 1;
    return task.firstReady(percpu.currentIndex());
}

fn nextReadyTask(force_priority: bool) ?*task.Task {
    priority_selects +%= 1;
    if (!force_priority and priority_selects % PRIORITY_RR_INTERVAL == 0) {
        const selected = plainNextReadyTask();
        if (selected != null) priority_rr_picks +%= 1;
        return selected;
    }

    var best: ?*task.Task = null;
    var best_rank: u8 = task.no_dispatch_rank;
    var cursor = task.firstReady(percpu.currentIndex());
    while (cursor) |candidate| : (cursor = task.nextReady(candidate)) {
        ready_candidate_visit_count +%= 1;
        const rank = task.dispatchRank(candidate);
        if (rank < best_rank) {
            best_rank = rank;
            best = candidate;
            if (rank == 0) break;
        }
    }
    if (best) |selected| {
        switch (task.effectivePriority(selected)) {
            .high => priority_picks_high +%= 1,
            .normal => priority_picks_normal +%= 1,
            .low => priority_picks_low +%= 1,
        }
        role_picks[@intFromEnum(selected.role)] +%= 1;
    }
    return best;
}

fn sendRescheduleToCpu(cpu_index: u32) void {
    if (cpu_index == percpu.currentIndex() or !percpu.isSchedulable(cpu_index)) return;
    const apic_id = percpu.apicId(cpu_index) orelse return;
    _ = lapic.sendReschedule(apic_id, RESCHEDULE_VECTOR);
}

fn noteProductiveTask(selected: *const task.Task) void {
    restoreProductiveTimer(selected);
    if (!selected.smp_eligible or selected.cpu_idle) return;
    const cpu_index = percpu.currentIndex();
    if (!percpu.noteProductive(cpu_index, selected.smp_r4x_work)) return;
    // The test-only kernel scaling probe reports its complete CPU mask in one
    // compact serial record. Keep the persistent 64-KiB bootlog for product
    // diagnostics; only the first audited R4X execution needs this marker.
    if (!selected.smp_r4x_work) return;
    k.puts("[SMP] productive cpu=");
    k.putDec(cpu_index);
    k.puts(" task=");
    k.puts(selected.name);
    k.puts(" class=");
    k.puts("r4x");
    k.puts("\r\n");
}

fn restoreProductiveTimer(selected: *const task.Task) void {
    // A BSP idle wait temporarily changes the shared HPET/LAPIC backend to a
    // one-shot. Every path that hands or retains the BSP for real work must
    // restore periodic delivery before that work can rely on scheduler
    // timeouts. leaveIdleDeadline() has a cheap idempotent periodic fast path.
    if (percpu.currentIndex() == 0 and !selected.cpu_idle) {
        _ = timer.leaveIdleDeadline();
    }
}

export fn taskEntryTrampoline() callconv(.c) noreturn {
    interrupts.enable();
    if (current()) |t| {
        if (t.entry) |entry| entry();
    }
    exitCurrent();
}

// --- 0.56.18: Prioritaets-Selbsttest (SCHEDPRIO) ---
// Drei NORMAL-Busy-Threads spinnen je ~1 Tick pro Turn (yield-kooperativ);
// der Testfaden stellt sich selbst auf HIGH und misst ueber 50 1-Tick-
// Sleeps seine max. Ready-Latenz. Mit Prioritaetsauswahl springt er der
// Busy-Rotation vorbei (Erwartung <= 3 Ticks: Spin-Rest + jede 8. Auswahl
// reines RR); reines Round-Robin laege bei bis zu 3 Busy-Turns a ~1 Tick
// darueber. Marker auf COM1 fuer die Gate-/Abnahme-Greps.
const ST_PRIO_BUSY_COUNT: usize = 3;
const ST_PRIO_ROUNDS: u32 = 50;
const ST_PRIO_LATENCY_BAR: u64 = 3;

var st_prio_busy_stop: bool = false;

fn stPrioBusyMain() callconv(.c) void {
    while (!st_prio_busy_stop) {
        const t0 = timer.tickCount();
        while (timer.tickCount() == t0 and !st_prio_busy_stop) {}
        yield();
    }
}

pub fn prioritySelfTest() bool {
    if (!initialized) return false;
    const me = current() orelse return false;

    st_prio_busy_stop = false;
    var spawned: usize = 0;
    var busy_index: usize = 0;
    while (busy_index < ST_PRIO_BUSY_COUNT) : (busy_index += 1) {
        if (task.createKernelThread("st-prio-busy", stPrioBusyMain) != null) spawned += 1;
    }
    if (spawned == 0) {
        k.puts("SCHEDPRIO FAIL busy-spawn\r\n");
        return false;
    }

    const old_role = me.role;
    if (!task.assignRole(me, .input)) return false;
    me.max_ready_latency_ticks = 0;
    var round: u32 = 0;
    while (round < ST_PRIO_ROUNDS) : (round += 1) {
        sleepTicksWithReason(1, "schedprio");
    }
    const high_latency_max = me.max_ready_latency_ticks;
    _ = task.assignRole(me, old_role);
    st_prio_busy_stop = true;
    // Busy-Threads sehen das Stop-Flag beim naechsten Spin-Check und
    // beenden sich selbst (exitCurrent via Trampolin).

    const ok = high_latency_max <= ST_PRIO_LATENCY_BAR;
    k.puts(if (ok) "SCHEDPRIO OK" else "SCHEDPRIO FAIL");
    k.puts(" high_latency_max=");
    k.putDec(high_latency_max);
    k.puts(" bar=");
    k.putDec(ST_PRIO_LATENCY_BAR);
    k.puts(" picks_high=");
    k.putDec(priority_picks_high);
    k.puts(" picks_normal=");
    k.putDec(priority_picks_normal);
    k.puts(" rr_picks=");
    k.putDec(priority_rr_picks);
    k.puts(" selects=");
    k.putDec(priority_selects);
    k.puts("\r\n");
    return ok;
}
