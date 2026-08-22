const task = @import("task.zig");
const task_context = @import("task_context.zig");
const config = @import("config");
const fpu = @import("../arch/x86_64/fpu.zig");
const interrupts = @import("../arch/x86_64/interrupts.zig");
const timer = @import("../kernel/timer.zig");
const k = @import("../kernel/log.zig");

extern fn r4os_context_switch(old_rsp: *u64, new_rsp: u64) callconv(.c) void;

var current_task: ?*task.Task = null;
var initialized = false;
var yield_count: u64 = 0;
var sleep_count: u64 = 0;
var wake_count: u64 = 0;
var idle_wait_count: u64 = 0;
var object_wait_count: u64 = 0;
var object_wake_count: u64 = 0;
var object_timeout_count: u64 = 0;
var object_cancel_count: u64 = 0;
var boot_preempt_disable_depth: u32 = 0;
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
};

pub fn init() bool {
    task_context.clear();
    if (task.count() == 0) return false;
    current_task = task.first() orelse return false;
    task_context.bind(&current_task.?.unwind_guard_count);
    initialized = true;
    yield_count = 0;
    sleep_count = 0;
    wake_count = 0;
    idle_wait_count = 0;
    object_wait_count = 0;
    object_wake_count = 0;
    object_timeout_count = 0;
    object_cancel_count = 0;
    boot_preempt_disable_depth = 0;
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
    external_irq_fpu_guard_entries = 0;
    external_irq_fpu_guard_mismatches = 0;
    external_irq_fpu_guard_mismatch_reported = false;
    priority_rr_cursor_id = 0;
    priority_rr_cursor_generation = 0;
    // 0.56.15: Recycle-Wachhund (Befund 13.2.3) - task.zig darf den Slot des
    // aktuell laufenden Tasks nie recyceln (exitCurrent laeuft auf seinem
    // Stack weiter, bis der Scheduler weggeschaltet hat).
    task.setCurrentProvider(current);
    return true;
}

pub fn stats() Stats {
    const diagnostic_ordinal = if (current_task) |running| task.ordinalOf(running) orelse 0 else 0;
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
    };
}

pub fn current() ?*task.Task {
    if (!initialized) return null;
    return current_task;
}

// 0.56.40: Idle-Erkennung fuer die Exit-/Boot-Warteschleifen. true,
// sobald irgendein ANDERER Task ready ist - dann muss die Schleife
// sofort yielden statt zu hlt'en, sonst bremst jeder Rotationsbesuch
// das System um bis zu einen Tick.
pub fn hasOtherReadyTask() bool {
    if (!initialized) return false;
    const running = current_task;
    var cursor = task.first();
    while (cursor) |candidate| : (cursor = task.next(candidate)) {
        if (candidate == running) continue;
        if (candidate.state == .ready) return true;
    }
    return false;
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

    const running = current_task;
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
            if (current_task == interrupted and
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
    preempt_disable_call_count +%= 1;
    if (current()) |running_task| {
        const depth = task.recordPreemptDisable(running_task);
        if (depth > preempt_disable_max_depth) preempt_disable_max_depth = depth;
        return;
    }
    boot_preempt_disable_depth +|= 1;
    if (boot_preempt_disable_depth > preempt_disable_max_depth) {
        preempt_disable_max_depth = boot_preempt_disable_depth;
    }
}

pub fn preemptEnable() void {
    preempt_enable_call_count +%= 1;
    if (current()) |running_task| {
        if (!task.recordPreemptEnable(running_task)) {
            preempt_disable_underflow_count +%= 1;
        }
        return;
    }
    if (boot_preempt_disable_depth == 0) {
        preempt_disable_underflow_count +%= 1;
        return;
    }
    boot_preempt_disable_depth -= 1;
}

pub fn yield() void {
    if (!initialized or task.count() <= 1) return;
    const irq_flags = interrupts.saveAndDisable();
    yield_count +%= 1;
    const now = timer.tickCount();

    preemptDisable();
    const old = current_task orelse {
        preemptEnable();
        interrupts.restore(irq_flags);
        return;
    };
    task.recordYield(old, now);
    const next_task = nextReadyTask(old) orelse {
        preemptEnable();
        interrupts.restore(irq_flags);
        return;
    };
    if (old.state == .running) task.markReady(old, now);
    recordReadyLatency(task.recordScheduled(next_task, now));
    task.markRunning(next_task);
    task.saveFpuState(old);
    task.restoreFpuState(next_task);
    preemptEnable();
    current_task = next_task;
    task_context.bind(&next_task.unwind_guard_count);
    r4os_context_switch(&old.rsp, next_task.rsp);
    interrupts.restore(irq_flags);
    _ = task.reapDeferred();
}

pub fn preemptFromIrq() void {
    if (!initialized or task.count() <= 1) return;
    const now = timer.tickCount();
    const old = current_task orelse return;
    if (old.state != .running) return;

    const next_task = nextReadyTask(old) orelse return;

    task.markReady(old, now);
    recordReadyLatency(task.recordScheduled(next_task, now));
    task.markRunning(next_task);
    task.saveFpuState(old);
    task.restoreFpuState(next_task);
    current_task = next_task;
    task_context.bind(&next_task.unwind_guard_count);
    r4os_context_switch(&old.rsp, next_task.rsp);
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
    if (!initialized) return null;
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    preemptDisable();
    defer preemptEnable();
    const running = current_task orelse return null;
    const now = timer.tickCount();
    const wake_tick = if (timeout_ticks == WAIT_FOREVER) 0 else now + timeout_ticks;
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
    const park_irq_flags = interrupts.saveAndDisable();
    interrupts.restore(park_irq_flags);
    defer interrupts.restore(park_irq_flags);
    while (blocked_task.state == .blocked) {
        yield();
        if (blocked_task.state == .blocked) {
            idle_wait_count +%= 1;
            interrupts.enable();
            interrupts.waitForInterrupt();
            interrupts.restore(park_irq_flags);
        }
    }
    if (current_task == blocked_task and blocked_task.state == .ready) {
        recordReadyLatency(task.recordScheduled(blocked_task, timer.tickCount()));
        task.markRunning(blocked_task);
    }
}

pub fn wakeTask(target: *task.Task, result: task.WaitResult) bool {
    const irq_flags = interrupts.saveAndDisable();
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
    return true;
}

pub fn exitCurrent() noreturn {
    exitCurrentImpl(false);
}

// The boot owner has no external lifecycle reaper. Mark it for deferred
// release before the terminal context switch so it cannot remain in the
// runtime registry after its successful handoff.
pub fn exitCurrentAndRetire() noreturn {
    exitCurrentImpl(true);
}

fn exitCurrentImpl(retire: bool) noreturn {
    const irq_flags = interrupts.saveAndDisable();
    preemptDisable();
    if (current()) |t| {
        if (t.held_lock_count != 0 or t.unwind_guard_count != 0) {
            k.puts("TASK EXIT INVARIANT: owned synchronization remains id=");
            k.putDec(t.id);
            k.puts("\r\n");
            interrupts.haltForever();
        }
        _ = task.detachWait(t);
        t.state = .dead;
        t.wake_tick = 0;
        t.blocked_since_tick = 0;
        t.wait_reason = "";
        t.wait_object = 0;
        t.wait_result = .killed;
        t.retire_pending = retire;
    }
    preemptEnable();
    interrupts.restore(irq_flags);
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
            interrupts.enable();
            interrupts.waitForInterrupt();
        }
    }
}

pub fn onTick(now: u64, preemptible_instruction_pointer: bool) bool {
    if (initialized) {
        if (current_task) |running| {
            if (running.state == .running) {
                const run_ticks = task.recordRunTick(running, now);
                recordRunWindow(run_ticks);
                recordQuantumOverrun(run_ticks);
            }
        }
    }
    // 0.56.18: Timeout-Scan nur, wenn ueberhaupt ein Timeout faellig sein
    // kann (Befund 4.3). min_wake_tick ist eine untere Schranke; der Scan
    // berechnet sie hier als einziger neu (nach oben).
    if (now >= task.minWakeTick()) {
        var next_min: u64 = task.NO_WAKE_TICK;
        var cursor = task.first();
        while (cursor) |candidate| : (cursor = task.next(candidate)) {
            if (candidate.state != .blocked or candidate.wake_tick == 0) continue;
            if (now >= candidate.wake_tick) {
                _ = wakeTask(candidate, .timeout);
            } else if (candidate.wake_tick < next_min) {
                next_min = candidate.wake_tick;
            }
        }
        task.setMinWakeTick(next_min);
    }
    const should_preempt = runTimerPreemption(now, preemptible_instruction_pointer);
    recordRuntimeWarnings(now);
    return should_preempt;
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
    k.puts("Scheduler status\r\n");
    k.puts("  Initialized: ");
    k.puts(if (initialized) "yes" else "no");
    k.puts(" current_index=");
    if (current_task) |running| {
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
    dumpCurrent();
    task.dump();
}

fn currentPreemptDepth() u32 {
    if (current()) |running_task| return running_task.preempt_disable_depth;
    return boot_preempt_disable_depth;
}

fn runTimerPreemption(now: u64, preemptible_instruction_pointer: bool) bool {
    preemption_simulation_tick_count +%= 1;
    if (!initialized) {
        preemption_deferred_no_task_count +%= 1;
        return false;
    }
    const running = current_task orelse {
        preemption_deferred_no_task_count +%= 1;
        return false;
    };
    if (running.state != .running) {
        preemption_deferred_no_task_count +%= 1;
        return false;
    }
    // This is only an eligibility probe. It must not consume a priority
    // selection or advance the independent fairness cursor; the IRQ switch
    // performs the one authoritative ready-task selection afterwards.
    if (!hasReadyTaskExcluding(running)) {
        preemption_deferred_no_ready_count +%= 1;
        return false;
    }
    const scheduled_ticks = ticksSince(now, running.last_scheduled_tick);
    if (scheduled_ticks < @as(u64, preemption_quantum_ticks)) {
        preemption_deferred_quantum_count +%= 1;
        task.recordPreemptionDeferred(running, scheduled_ticks);
        recordPreemptionDeferredWindow(scheduled_ticks);
        return false;
    }

    preemption_quantum_expired_count +%= 1;
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
    return true;
}

fn recordRuntimeWarnings(now: u64) void {
    if (!initialized) return;
    if (current_task) |running| {
        if (running.state == .running) {
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
    var cursor = task.first();
    while (cursor) |candidate| : (cursor = task.next(candidate)) {
        if (candidate.state != .ready) continue;
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

fn recordReadyLatency(latency: u64) void {
    ready_latency_sample_count +%= 1;
    ready_latency_total_ticks +%= latency;
    ready_latency_last_ticks = latency;
    if (latency > ready_latency_max_ticks) ready_latency_max_ticks = latency;
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
// innerhalb der Klasse bleibt die Rotation ab start+1 erhalten (Round-
// Robin). Anti-Starvation: jede 8. Auswahl ist reines Round-Robin, damit
// NORMAL/LOW unter HIGH-Dauerlast garantiert drankommen.
const PRIORITY_RR_INTERVAL: u64 = 8;

var priority_selects: u64 = 0;
var priority_picks_high: u64 = 0;
var priority_picks_normal: u64 = 0;
var priority_picks_low: u64 = 0;
var priority_rr_picks: u64 = 0;
var priority_rr_cursor_id: u32 = 0;
var priority_rr_cursor_generation: u64 = 0;

fn hasReadyTaskExcluding(excluded: *task.Task) bool {
    var cursor = task.first();
    while (cursor) |candidate| : (cursor = task.next(candidate)) {
        if (candidate != excluded and candidate.state == .ready) return true;
    }
    return false;
}

fn plainNextReadyTask(start: *task.Task) ?*task.Task {
    var anchor = start;
    if (priority_rr_cursor_id != 0) {
        var saved_cursor = task.first();
        while (saved_cursor) |saved| : (saved_cursor = task.next(saved)) {
            if (saved.id == priority_rr_cursor_id and saved.generation == priority_rr_cursor_generation) {
                anchor = saved;
                break;
            }
        }
    }

    var cursor = task.nextCircular(anchor) orelse return null;
    while (cursor != anchor) : (cursor = task.nextCircular(cursor) orelse return null) {
        if (cursor == start) continue;
        if (cursor.state == .ready) {
            priority_rr_cursor_id = cursor.id;
            priority_rr_cursor_generation = cursor.generation;
            return cursor;
        }
    }

    // The saved cursor may be the only ready task besides the current one.
    // Fall back to one complete scan from current without retaining a raw
    // pointer; the next fairness pass resumes after the selected identity.
    cursor = task.nextCircular(start) orelse return null;
    while (cursor != start) : (cursor = task.nextCircular(cursor) orelse return null) {
        if (cursor.state == .ready) {
            priority_rr_cursor_id = cursor.id;
            priority_rr_cursor_generation = cursor.generation;
            return cursor;
        }
    }
    return null;
}

fn nextReadyTask(start: *task.Task) ?*task.Task {
    priority_selects +%= 1;
    if (priority_selects % PRIORITY_RR_INTERVAL == 0) {
        const selected = plainNextReadyTask(start);
        if (selected != null) priority_rr_picks +%= 1;
        return selected;
    }

    var best: ?*task.Task = null;
    var best_prio: u8 = 255;
    var cursor = task.nextCircular(start) orelse return null;
    while (cursor != start) : (cursor = task.nextCircular(cursor) orelse return null) {
        const candidate = cursor;
        if (candidate.state != .ready) continue;
        const p = @intFromEnum(candidate.priority);
        if (p < best_prio) {
            best_prio = p;
            best = candidate;
            if (p == @intFromEnum(task.Priority.high)) break;
        }
    }
    if (best != null) {
        switch (@as(task.Priority, @enumFromInt(best_prio))) {
            .high => priority_picks_high +%= 1,
            .normal => priority_picks_normal +%= 1,
            .low => priority_picks_low +%= 1,
        }
    }
    return best;
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

    const old_priority = me.priority;
    me.priority = .high;
    me.max_ready_latency_ticks = 0;
    var round: u32 = 0;
    while (round < ST_PRIO_ROUNDS) : (round += 1) {
        sleepTicksWithReason(1, "schedprio");
    }
    const high_latency_max = me.max_ready_latency_ticks;
    me.priority = old_priority;
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
