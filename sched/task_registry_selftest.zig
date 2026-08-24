const std = @import("std");
const fpu = @import("../arch/x86_64/fpu.zig");
const interrupts = @import("../arch/x86_64/interrupts.zig");
const boot_config = @import("../kernel/boot_config.zig");
const k = @import("../kernel/log.zig");
const heap = @import("../memory/heap.zig");
const phys = @import("../memory/phys.zig");
const virt = @import("../memory/virt.zig");
const net_core = @import("../net/core.zig");
const timer = @import("../kernel/timer.zig");
const scheduler = @import("scheduler.zig");
const sync = @import("sync.zig");
const task = @import("task.zig");

const WORKER_COUNT: usize = 160;
const RUNNABLE_END: usize = 24;
const SLEEP_END: usize = 48;
const SIGNAL_END: usize = 80;
const CANCEL_END: usize = 112;
const TIMEOUT_END: usize = 144;
const KILL_END: usize = WORKER_COUNT;
const CHURN_COUNT: usize = 10_000;
const WAIT_BUDGET: usize = 20_000;
const NETWORK_PROGRESS_TICK_BUDGET: usize = 64;
const WAKE_LATENCY_BAR: u64 = 3;
const DEADLINE_EARLY_TICKS: u64 = 4;
const DEADLINE_LONG_TICKS: u64 = 20;
const DEADLINE_IRQ_BAR: u64 = 3;
const DEADLINE_LATENESS_BAR: u64 = 3;
const DEADLINE_STORM_COUNT: usize = 65;

const WorkerKind = enum {
    runnable,
    sleeping,
    signal_wait,
    cancel_wait,
    timeout_wait,
    kill_wait,
};

const Baseline = struct {
    task_count: usize,
    heap_used: usize,
    heap_blocks: usize,
    pmm_free: u64,
    vm_ranges: u64,
    vm_reserved: u64,
    vm_committed: u64,
    vm_resident: u64,
    fpu_live: u32,
};

const WorkerMix = struct {
    live: u64 = 0,
    runnable: u64 = 0,
    blocked: u64 = 0,
    priorities: u8 = 0,
    stable: bool = true,
};

var worker_tasks: [WORKER_COUNT]?*task.Task = .{null} ** WORKER_COUNT;
var worker_ids: [WORKER_COUNT]u32 = .{0} ** WORKER_COUNT;
var worker_generations: [WORKER_COUNT]u64 = .{0} ** WORKER_COUNT;
var worker_started: u32 = 0;
var worker_failures: u32 = 0;
var signal_results: u32 = 0;
var cancel_results: u32 = 0;
var timeout_results: u32 = 0;
var release_workers = false;
var churn_completed: u32 = 0;
var short_completed: u32 = 0;
var wake_probe_entered = false;
var wake_probe_completed = false;
var wake_probe_latency: u64 = 0;
var deadline_probe_elapsed: u64 = 0;
var deadline_probe_irqs: u64 = 0;
var deadline_probe_lateness: u64 = 0;
var deadline_probe_checkpointed = false;
var deadline_storm_batches: u32 = 0;

var signal_event = sync.EventV2.initMode(false, .manual_reset);
var cancel_queue = sync.WaitQueue.init();
var timeout_queue = sync.WaitQueue.init();
var kill_queue = sync.WaitQueue.init();
var finish_queue = sync.WaitQueue.init();
var wake_probe_event = sync.EventV2.initMode(false, .manual_reset);

pub fn runIfEnabled() bool {
    const value = boot_config.optionValue(boot_config.get(), "TASKREGISTRY", "selftest") orelse return true;
    if (!std.ascii.eqlIgnoreCase(value, "yes")) return true;
    return run();
}

fn run() bool {
    resetState();
    _ = task.reapDeferred();
    _ = task.reclaimStackCache(0);

    // Page-table pages are intentionally retained by paging after unmap. Warm
    // the exact high-water stack address span once, then take the leak
    // baseline. Subsequent create/reap cycles must return to this steady state.
    phase("warm-begin");
    if (!createWorkers()) {
        cleanupWorkers();
        return fail("warm-create");
    }
    cleanupWorkers();
    resetState();
    if (!queueProjectionValid("warm")) return fail("queue-warm");
    if (!runDeadlineProbe()) return fail("deadline-probe");
    if (!runDeadlineStormProbe()) return fail("deadline-storm");
    if (!runWakeProbe()) return fail("wakeup-probe");
    _ = task.reapDeferred();
    _ = task.reclaimStackCache(0);
    if (!queueProjectionValid("wake-reaped")) return fail("queue-wake-reaped");
    phase("begin");

    const baseline = takeBaseline();

    if (!createWorkers()) {
        cleanupWorkers();
        return fail("concurrency-create");
    }
    phase("created");
    if (task.count() != baseline.task_count + WORKER_COUNT or !workersStable()) {
        cleanupWorkers();
        return fail("concurrency-publish");
    }
    if (!queueProjectionValid("created")) {
        cleanupWorkers();
        return fail("queue-created");
    }
    const reap_before = task.reapStats();
    _ = task.reapDeferred();
    const reap_after = task.reapStats();
    if (reap_after.candidate_visits != reap_before.candidate_visits) {
        cleanupWorkers();
        return fail("reaper-empty-scan");
    }
    if (!waitForStarted()) {
        cleanupWorkers();
        return fail("concurrency-start");
    }
    phase("started");
    if (!waitForKindQueue(.cancel_wait, "taskreg-cancel") or
        !waitForKindQueue(.kill_wait, "taskreg-kill"))
    {
        cleanupWorkers();
        return fail("wait-enrollment");
    }
    phase("enrolled");
    const enrolled_queues = task.queueSnapshot();
    if (!enrolled_queues.valid or enrolled_queues.registry < WORKER_COUNT or enrolled_queues.ready >= enrolled_queues.registry) {
        cleanupWorkers();
        return fail("queue-enrolled");
    }

    signal_event.signal();
    const cancelled = cancel_queue.cancelAll();
    if (cancelled != CANCEL_END - SIGNAL_END) {
        cleanupWorkers();
        return fail("cancel-count");
    }
    var kill_index: usize = TIMEOUT_END;
    while (kill_index < KILL_END) : (kill_index += 1) {
        if (!task.kill(worker_ids[kill_index])) {
            cleanupWorkers();
            return fail("kill-detach");
        }
    }
    if (!waitForWaitResults()) {
        cleanupWorkers();
        return fail("wait-results");
    }
    phase("wait-results");
    if (!queueProjectionValid("wait-results")) {
        cleanupWorkers();
        return fail("queue-wait-results");
    }

    const mix = countWorkerStates();
    if (!mix.stable or mix.live < 128 or mix.runnable == 0 or mix.blocked == 0 or mix.priorities != 0b111 or worker_failures != 0) {
        cleanupWorkers();
        return fail("concurrency-state");
    }
    k.puts("TASKREG05910 concurrency live=");
    k.putDec(mix.live);
    k.puts(" runnable=");
    k.putDec(mix.runnable);
    k.puts(" blocked=");
    k.putDec(mix.blocked);
    k.puts(" priorities=3 wait=OK stable=OK\r\n");

    release_workers = true;
    _ = finish_queue.wakeAll();
    if (!waitForAllWorkersDead() or !releaseAllWorkers()) {
        cleanupWorkers();
        return fail("concurrency-reap");
    }
    _ = task.reapDeferred();
    _ = task.reclaimStackCache(0);
    if (!matchesBaseline(baseline)) {
        logBaselineDelta(baseline);
        return fail("concurrency-baseline");
    }
    phase("reaped");
    if (!queueProjectionValid("reaped")) return fail("queue-reaped");

    phase("churn-begin");
    if (!runChurn(baseline)) return fail("churn");
    _ = task.reapDeferred();
    _ = task.reclaimStackCache(0);
    if (churn_completed != CHURN_COUNT or !matchesBaseline(baseline)) {
        logBaselineDelta(baseline);
        return fail("churn-baseline");
    }
    phase("churn-complete");
    if (!queueProjectionValid("churn")) return fail("queue-churn");

    k.puts("TASKREG05910 churn=10000 task=baseline heap=baseline pmm=baseline vm=baseline fpu=baseline\r\n");

    // Take the progress baseline immediately before the deterministic normal
    // admission failure. This keeps the marker scoped to the reserve/recovery
    // phase instead of accepting work performed during concurrency or churn.
    const caller = scheduler.current() orelse return fail("caller-missing");
    const caller_id = caller.id;
    const caller_generation = caller.generation;
    const rx_summary = net_core.rxTaskSummary();
    if (!rx_summary.started or rx_summary.task_id == 0) return fail("net-rx-missing");
    const rx_task = task.pinByIdentity(rx_summary.task_id, rx_summary.task_generation) orelse return fail("net-rx-identity");
    defer _ = task.unpin(rx_task);
    const rx_generation = rx_task.generation;
    const rx_ticks_before = rx_task.run_ticks;
    const rx_switches_before = rx_task.switches_in;
    const rx_polls_before = rx_summary.polls;
    const rx_iterations_before = rx_summary.iterations;

    if (!runOomReserveTest()) return fail("oom-reserve");
    _ = task.reapDeferred();
    _ = task.reclaimStackCache(0);
    if (!matchesBaseline(baseline)) {
        logBaselineDelta(baseline);
        return fail("oom-baseline");
    }

    var rx_summary_after = net_core.rxTaskSummary();
    // A short net-rx dispatch normally finishes inside one timer interval, so
    // run_ticks can stay unchanged. The loop counter is the direct progress
    // proof even in the headless no-NIC environment where adapter polls are
    // intentionally zero.
    var network_progress = rx_task.generation == rx_generation and
        (rx_summary_after.iterations > rx_iterations_before or
            rx_task.run_ticks > rx_ticks_before or
            (rx_task.switches_in > rx_switches_before and rx_summary_after.polls > rx_polls_before));
    var progress_spins: usize = 0;
    while (!network_progress and progress_spins < NETWORK_PROGRESS_TICK_BUDGET) : (progress_spins += 1) {
        // NETRX normally sleeps between polls. A tight yield loop can finish
        // inside the same 1-kHz tick and falsely claim that the critical
        // reserve blocked progress. Advance one bounded tick per probe.
        scheduler.sleepTicksWithReason(1, "taskreg-net-progress");
        rx_summary_after = net_core.rxTaskSummary();
        network_progress = rx_task.generation == rx_generation and
            (rx_summary_after.iterations > rx_iterations_before or
                rx_task.run_ticks > rx_ticks_before or
                (rx_task.switches_in > rx_switches_before and rx_summary_after.polls > rx_polls_before));
    }
    k.puts("TASKREG05910 network run_ticks=");
    k.putDec(rx_ticks_before);
    k.puts("->");
    k.putDec(rx_task.run_ticks);
    k.puts(" switches=");
    k.putDec(rx_switches_before);
    k.puts("->");
    k.putDec(rx_task.switches_in);
    k.puts(" polls=");
    k.putDec(rx_polls_before);
    k.puts("->");
    k.putDec(rx_summary_after.polls);
    k.puts(" iterations=");
    k.putDec(rx_iterations_before);
    k.puts("->");
    k.putDec(rx_summary_after.iterations);
    k.puts("\r\n");
    const current_caller = scheduler.current() orelse return fail("caller-lost");
    const caller_alive = current_caller == caller and current_caller.id == caller_id and current_caller.generation == caller_generation and current_caller.state == .running;
    if (!network_progress or !caller_alive) return fail("critical-progress");

    k.puts("TASKREG05910 admission fault=task_metadata normal=REJECTED critical=OK reserve=returned netrx=progress caller=alive recovery=OK\r\n");
    k.puts("TASKREG06931 queues=OK registry=");
    k.putDec(enrolled_queues.registry);
    k.puts(" ready=");
    k.putDec(enrolled_queues.ready);
    k.puts(" timed=");
    k.putDec(enrolled_queues.timed);
    k.puts(" reaper_empty_visits=0 wake_latency=");
    k.putDec(wake_probe_latency);
    k.puts(" wake_bar=");
    k.putDec(WAKE_LATENCY_BAR);
    k.puts(" wake_request=OK lock_handoff=OK\r\n");
    k.puts("TASKREG06932 deadline_queue=ordered backend=");
    k.puts(timer.backendName());
    k.puts(" idle_irqs=");
    k.putDec(deadline_probe_irqs);
    k.puts(" irq_bar=");
    k.putDec(DEADLINE_IRQ_BAR);
    k.puts(" elapsed=");
    k.putDec(deadline_probe_elapsed);
    k.puts(" requested=");
    k.putDec(DEADLINE_EARLY_TICKS);
    k.puts(" lateness=");
    k.putDec(deadline_probe_lateness);
    k.puts(" earlier=OK cancel=OK long=");
    k.puts(if (deadline_probe_checkpointed) "checkpointed" else "FAILED");
    k.puts(" storm=");
    k.putDec(DEADLINE_STORM_COUNT);
    k.puts(" batches=");
    k.putDec(deadline_storm_batches);
    k.puts(" periodic=restored fallback=0\r\n");
    k.puts("TASKREG05910 result: OK\r\n");
    return true;
}

fn resetState() void {
    worker_tasks = .{null} ** WORKER_COUNT;
    worker_ids = .{0} ** WORKER_COUNT;
    worker_generations = .{0} ** WORKER_COUNT;
    worker_started = 0;
    worker_failures = 0;
    signal_results = 0;
    cancel_results = 0;
    timeout_results = 0;
    release_workers = false;
    churn_completed = 0;
    short_completed = 0;
    wake_probe_entered = false;
    wake_probe_completed = false;
    wake_probe_latency = 0;
    deadline_probe_elapsed = 0;
    deadline_probe_irqs = 0;
    deadline_probe_lateness = 0;
    deadline_probe_checkpointed = false;
    deadline_storm_batches = 0;
    signal_event = sync.EventV2.initMode(false, .manual_reset);
    cancel_queue = sync.WaitQueue.init();
    timeout_queue = sync.WaitQueue.init();
    kill_queue = sync.WaitQueue.init();
    finish_queue = sync.WaitQueue.init();
    wake_probe_event = sync.EventV2.initMode(false, .manual_reset);
}

fn runDeadlineProbe() bool {
    const before = timer.deadlineStats();
    if (!before.enabled or !before.capable or timer.activeBackend() == .pit) return false;

    const irq_flags = interrupts.saveAndDisable();
    scheduler.preemptDisable();

    var ok = timer.enterIdleDeadline(timer.MAX_FINITE_DEADLINE);
    const long_state = timer.deadlineStats();
    deadline_probe_checkpointed = long_state.armed_deadline != timer.NO_DEADLINE and
        long_state.armed_deadline < timer.MAX_FINITE_DEADLINE;
    ok = timer.leaveIdleDeadline() and ok;

    const start = timer.tickCount();
    const long_deadline = timer.deadlineAfter(start, DEADLINE_LONG_TICKS);
    const early_deadline = timer.deadlineAfter(start, DEADLINE_EARLY_TICKS);
    ok = timer.enterIdleDeadline(long_deadline) and ok;
    ok = timer.enterIdleDeadline(early_deadline) and ok;

    while (timer.tickCount() < early_deadline) {
        interrupts.enable();
        interrupts.waitForInterrupt();
        interrupts.disable();
        if (timer.tickCount() < early_deadline) {
            ok = timer.leaveIdleDeadline() and ok;
            ok = timer.enterIdleDeadline(early_deadline) and ok;
        }
    }
    ok = timer.leaveIdleDeadline() and ok;

    scheduler.preemptEnable();
    interrupts.restore(irq_flags);

    const after = timer.deadlineStats();
    deadline_probe_elapsed = timer.tickCount() - start;
    deadline_probe_irqs = after.timer_irqs - before.timer_irqs;
    deadline_probe_lateness = after.last_lateness_ticks;
    return ok and
        deadline_probe_checkpointed and
        after.mode == .periodic and
        after.earlier_reprograms > before.earlier_reprograms and
        after.cancels > before.cancels and
        after.periodic_resumes >= before.periodic_resumes + 2 and
        after.runtime_fallbacks == before.runtime_fallbacks and
        deadline_probe_elapsed >= DEADLINE_EARLY_TICKS and
        deadline_probe_irqs >= 1 and deadline_probe_irqs <= DEADLINE_IRQ_BAR and
        after.last_lateness_ticks <= DEADLINE_LATENESS_BAR;
}

fn runDeadlineStormProbe() bool {
    var probes: [DEADLINE_STORM_COUNT]?*task.Task = .{null} ** DEADLINE_STORM_COUNT;
    var index: usize = 0;
    while (index < probes.len) : (index += 1) {
        const created = task.createKernelThreadBlocked("taskreg-deadline-storm", shortWorker) orelse {
            _ = cleanupDeadlineStorm(&probes);
            return false;
        };
        if (!task.pin(created)) {
            _ = task.retireIdentity(created.id, created.generation);
            _ = cleanupDeadlineStorm(&probes);
            return false;
        }
        probes[index] = created;
    }

    // Publish the already expired equal deadlines under one IRQ boundary so
    // the periodic source cannot drain a partial enrollment. Deadline 1 is
    // older than every runtime wait at this boot phase and makes FIFO order
    // directly observable at the projection head.
    const irq_flags = interrupts.saveAndDisable();
    for (probes) |created| {
        task.beginWait(created.?, 1, "taskreg-deadline-storm", scheduler.timer_wait_object);
    }
    const enrolled = task.firstTimedWait() == probes[0] and task.queueSnapshot().valid;
    const before = scheduler.structureStats();
    _ = scheduler.onTick(timer.tickCount(), false);
    const fifo_head = task.firstTimedWait() == probes[DEADLINE_STORM_COUNT - 1];
    var ready_after_first: usize = 0;
    index = 0;
    while (index < DEADLINE_STORM_COUNT - 1) : (index += 1) {
        if (probes[index].?.state == .ready) ready_after_first += 1;
    }
    const bounded_tail = probes[DEADLINE_STORM_COUNT - 1].?.state == .blocked;
    const deferred = scheduler.structureStats();
    const deferred_once = deferred.timeout_storm_deferrals == before.timeout_storm_deferrals + 1;

    _ = scheduler.onTick(timer.tickCount(), false);
    const drained = probes[DEADLINE_STORM_COUNT - 1].?.state == .ready;
    const projections_valid = task.queueSnapshot().valid;
    interrupts.restore(irq_flags);

    const bounded = enrolled and fifo_head and
        ready_after_first == DEADLINE_STORM_COUNT - 1 and bounded_tail and
        deferred_once and drained and projections_valid;
    if (!bounded) {
        k.puts("TASKREG06932 storm-failed enrolled=");
        k.putDec(@intFromBool(enrolled));
        k.puts(" fifo_head=");
        k.putDec(@intFromBool(fifo_head));
        k.puts(" ready_first=");
        k.putDec(ready_after_first);
        k.puts(" tail_blocked=");
        k.putDec(@intFromBool(bounded_tail));
        k.puts(" deferral=");
        k.putDec(@intFromBool(deferred_once));
        k.puts(" drained=");
        k.putDec(@intFromBool(drained));
        k.puts(" projection=");
        k.putDec(@intFromBool(projections_valid));
        k.puts("\r\n");
    }
    deadline_storm_batches = 2;
    const cleaned = cleanupDeadlineStorm(&probes);
    return bounded and cleaned;
}

fn cleanupDeadlineStorm(probes: *[DEADLINE_STORM_COUNT]?*task.Task) bool {
    var ok = true;
    for (probes) |*slot| {
        const created = slot.* orelse continue;
        if (task.isAliveIdentity(created.id, created.generation) and
            !task.killIdentity(created.id, created.generation))
        {
            ok = false;
        }
        if (!task.unpin(created)) ok = false;
        if (!task.releaseDeadIdentity(created.id, created.generation)) ok = false;
        slot.* = null;
    }
    _ = task.reapDeferred();
    _ = task.reclaimStackCache(0);
    return ok;
}

fn queueProjectionValid(label: []const u8) bool {
    const snapshot = task.queueSnapshot();
    if (snapshot.valid) return true;
    k.puts("TASKREG06931 queue-invalid phase=");
    k.puts(label);
    k.puts(" registry=");
    k.putDec(snapshot.registry);
    k.puts(" ready=");
    k.putDec(snapshot.ready);
    k.puts(" timed=");
    k.putDec(snapshot.timed);
    k.puts(" reap=");
    k.putDec(snapshot.reap);
    k.puts("\r\n");
    return false;
}

fn wakeProbeMain() callconv(.c) void {
    wake_probe_entered = true;
    if (wake_probe_event.waitResult(sync.WAIT_FOREVER) == .signaled) {
        wake_probe_completed = true;
    }
}

fn runWakeProbe() bool {
    const caller = scheduler.current() orelse return false;
    const old_priority = caller.priority;
    const probe = task.createKernelThreadBlocked("taskreg-wakeup", wakeProbeMain) orelse return false;
    const probe_id = probe.id;
    const probe_generation = probe.generation;
    if (!task.setPriority(probe, .high)) {
        _ = task.retireIdentity(probe_id, probe_generation);
        return false;
    }
    task.markReady(probe, timer.tickCount());

    var spins: usize = 0;
    while ((!wake_probe_entered or probe.state != .blocked) and spins < WAIT_BUDGET) : (spins += 1) scheduler.yield();
    if (!wake_probe_entered or probe.state != .blocked or !task.setPriority(caller, .normal)) {
        if (task.isAliveIdentity(probe_id, probe_generation)) _ = task.killIdentity(probe_id, probe_generation);
        _ = task.retireIdentity(probe_id, probe_generation);
        return false;
    }

    const before = scheduler.structureStats();
    wake_probe_event.signal();
    const after_signal = scheduler.structureStats();
    const published_without_switch = scheduler.current() == caller and
        after_signal.wakeup_reschedule_requests > before.wakeup_reschedule_requests and
        after_signal.reschedule_pending;

    spins = 0;
    while (!wake_probe_completed and spins < WAIT_BUDGET) : (spins += 1) scheduler.yield();
    wake_probe_latency = probe.last_ready_latency_ticks;
    _ = task.setPriority(caller, old_priority);
    const bounded = wake_probe_completed and wake_probe_latency <= WAKE_LATENCY_BAR;
    if (task.isAliveIdentity(probe_id, probe_generation)) _ = task.killIdentity(probe_id, probe_generation);
    const retired = task.retireIdentity(probe_id, probe_generation);
    return published_without_switch and bounded and retired != .pending;
}

fn createWorkers() bool {
    var index: usize = 0;
    while (index < WORKER_COUNT) : (index += 1) {
        const created = task.createKernelThreadBlocked("taskreg-worker", workerMain) orelse return false;
        if (!task.pin(created)) {
            _ = task.killIdentity(created.id, created.generation);
            _ = task.releaseDeadIdentity(created.id, created.generation);
            return false;
        }
        if (!task.setPriority(created, switch (index % 3) {
            0 => .high,
            1 => .normal,
            else => .low,
        })) return false;
        worker_tasks[index] = created;
        worker_ids[index] = created.id;
        worker_generations[index] = created.generation;
        task.markReady(created, timer.tickCount());
    }
    return true;
}

fn workerMain() callconv(.c) void {
    const current = scheduler.current() orelse {
        worker_failures +%= 1;
        return;
    };
    const index = workerIndex(current) orelse {
        worker_failures +%= 1;
        return;
    };
    worker_started +%= 1;

    switch (workerKind(index)) {
        .runnable => while (!release_workers) scheduler.yield(),
        .sleeping => while (!release_workers) scheduler.sleepTicksWithReason(2, "taskreg-sleep"),
        .signal_wait => {
            if (signal_event.waitResult(sync.WAIT_FOREVER) == .signaled) {
                signal_results +%= 1;
            } else {
                worker_failures +%= 1;
            }
            parkUntilRelease();
        },
        .cancel_wait => {
            if (cancel_queue.wait(sync.WAIT_FOREVER, "taskreg-cancel") == .cancelled) {
                cancel_results +%= 1;
            } else {
                worker_failures +%= 1;
            }
            parkUntilRelease();
        },
        .timeout_wait => {
            if (timeout_queue.wait(5, "taskreg-timeout") == .timeout) {
                timeout_results +%= 1;
            } else {
                worker_failures +%= 1;
            }
            parkUntilRelease();
        },
        .kill_wait => {
            _ = kill_queue.wait(sync.WAIT_FOREVER, "taskreg-kill");
            worker_failures +%= 1;
        },
    }
}

fn parkUntilRelease() void {
    while (!release_workers) {
        const result = finish_queue.wait(100, "taskreg-finish");
        if (result == .failed) {
            worker_failures +%= 1;
            return;
        }
    }
}

fn workerIndex(wanted: *task.Task) ?usize {
    var index: usize = 0;
    while (index < WORKER_COUNT) : (index += 1) {
        if (worker_tasks[index] == wanted) return index;
    }
    return null;
}

fn workerKind(index: usize) WorkerKind {
    if (index < RUNNABLE_END) return .runnable;
    if (index < SLEEP_END) return .sleeping;
    if (index < SIGNAL_END) return .signal_wait;
    if (index < CANCEL_END) return .cancel_wait;
    if (index < TIMEOUT_END) return .timeout_wait;
    return .kill_wait;
}

fn workersStable() bool {
    var index: usize = 0;
    while (index < WORKER_COUNT) : (index += 1) {
        const expected = worker_tasks[index] orelse return false;
        if (expected.id != worker_ids[index] or expected.generation != worker_generations[index]) return false;
    }
    return true;
}

fn waitForStarted() bool {
    var spins: usize = 0;
    while (worker_started < WORKER_COUNT and spins < WAIT_BUDGET) : (spins += 1) {
        scheduler.yield();
        if (spins != 0 and spins % 1000 == 0) logStartProgress(spins);
    }
    return worker_started == WORKER_COUNT;
}

fn logStartProgress(spins: usize) void {
    var states: [3][3]u32 = .{.{0} ** 3} ** 3;
    var index: usize = 0;
    while (index < WORKER_COUNT) : (index += 1) {
        const current = worker_tasks[index] orelse continue;
        const priority_index: usize = @intFromEnum(current.priority);
        const state_index: ?usize = switch (current.state) {
            .ready => 0,
            .running => 1,
            .blocked => 2,
            else => null,
        };
        if (state_index) |selected| states[priority_index][selected] += 1;
    }
    k.puts("TASKREG05910 phase=started-wait spins=");
    k.putDec(spins);
    k.puts(" started=");
    k.putDec(worker_started);
    k.puts(" high=");
    putStateTriplet(states[0]);
    k.puts(" normal=");
    putStateTriplet(states[1]);
    k.puts(" low=");
    putStateTriplet(states[2]);
    k.puts("\r\n");
}

fn putStateTriplet(states: [3]u32) void {
    k.putDec(states[0]);
    k.puts("/");
    k.putDec(states[1]);
    k.puts("/");
    k.putDec(states[2]);
}

fn waitForKindQueue(kind: WorkerKind, reason: []const u8) bool {
    var spins: usize = 0;
    while (spins < WAIT_BUDGET) : (spins += 1) {
        var ready = true;
        var index: usize = 0;
        while (index < WORKER_COUNT) : (index += 1) {
            if (workerKind(index) != kind) continue;
            const current = worker_tasks[index] orelse return false;
            if (current.generation != worker_generations[index] or current.state != .blocked or !std.mem.eql(u8, current.wait_reason, reason)) {
                ready = false;
                break;
            }
        }
        if (ready) return true;
        scheduler.yield();
    }
    return false;
}

fn waitForWaitResults() bool {
    var spins: usize = 0;
    while (spins < WAIT_BUDGET) : (spins += 1) {
        if (signal_results == SIGNAL_END - SLEEP_END and
            cancel_results == CANCEL_END - SIGNAL_END and
            timeout_results == TIMEOUT_END - CANCEL_END)
        {
            return true;
        }
        scheduler.yield();
    }
    return false;
}

fn countWorkerStates() WorkerMix {
    var out: WorkerMix = .{};
    var index: usize = 0;
    while (index < WORKER_COUNT) : (index += 1) {
        const expected = worker_tasks[index] orelse {
            out.stable = false;
            continue;
        };
        const current = worker_tasks[index] orelse {
            out.stable = false;
            continue;
        };
        if (current != expected or current.generation != worker_generations[index]) out.stable = false;
        if (current.state == .dead or current.state == .unused) continue;
        out.live += 1;
        out.priorities |= @as(u8, 1) << @intCast(@intFromEnum(current.priority));
        switch (current.state) {
            .ready, .running => out.runnable += 1,
            .blocked => out.blocked += 1,
            else => {},
        }
    }
    return out;
}

fn waitForAllWorkersDead() bool {
    var spins: usize = 0;
    while (spins < WAIT_BUDGET) : (spins += 1) {
        var all_dead = true;
        var index: usize = 0;
        while (index < WORKER_COUNT) : (index += 1) {
            const current = worker_tasks[index] orelse continue;
            if (current.generation != worker_generations[index]) return false;
            if (current.state != .dead) {
                all_dead = false;
                break;
            }
        }
        if (all_dead) return true;
        scheduler.yield();
    }
    return false;
}

fn releaseAllWorkers() bool {
    var ok = true;
    var index: usize = 0;
    while (index < WORKER_COUNT) : (index += 1) {
        const expected = worker_tasks[index];
        if (expected) |pinned| {
            if (!task.unpin(pinned)) ok = false;
            if (!task.releaseDeadIdentity(worker_ids[index], worker_generations[index])) ok = false;
        }
        worker_tasks[index] = null;
    }
    return ok;
}

fn cleanupWorkers() void {
    release_workers = true;
    var index: usize = 0;
    while (index < WORKER_COUNT) : (index += 1) {
        const id = worker_ids[index];
        if (id == 0) continue;
        if (task.isAlive(id)) _ = task.kill(id);
    }
    _ = signal_event.queue.cancelAll();
    _ = cancel_queue.cancelAll();
    _ = timeout_queue.cancelAll();
    _ = kill_queue.cancelAll();
    _ = finish_queue.cancelAll();
    _ = releaseAllWorkers();
    _ = task.reapDeferred();
    _ = task.reclaimStackCache(0);
}

fn runChurn(baseline: Baseline) bool {
    var cycle: usize = 0;
    while (cycle < CHURN_COUNT) : (cycle += 1) {
        if (cycle != 0 and cycle % 1000 == 0) {
            k.puts("TASKREG05910 phase=churn progress=");
            k.putDec(cycle);
            k.puts("\r\n");
        }
        const created = task.createKernelThread("taskreg-churn", churnWorker) orelse return false;
        const id = created.id;
        const generation = created.generation;
        if (!task.pin(created)) {
            _ = task.killIdentity(id, generation);
            _ = task.releaseDeadIdentity(id, generation);
            return false;
        }
        var spins: usize = 0;
        while (created.state != .dead and spins < 4096) : (spins += 1) scheduler.yield();
        if (created.state != .dead) {
            _ = task.killIdentity(id, generation);
            _ = task.unpin(created);
            _ = task.releaseDeadIdentity(id, generation);
            return false;
        }
        if (created.id != id or created.generation != generation) {
            _ = task.unpin(created);
            return false;
        }
        if (!task.unpin(created) or !task.releaseDeadIdentity(id, generation)) return false;
        if (task.count() != baseline.task_count) return false;
    }
    return true;
}

fn churnWorker() callconv(.c) void {
    churn_completed +%= 1;
}

fn shortWorker() callconv(.c) void {
    short_completed +%= 1;
}

fn runOomReserveTest() bool {
    const reserve_before = task.criticalReserveStats();
    if (reserve_before.available == 0) return false;
    if (!task.armNextCreateFailureForTest(.task_metadata)) return false;
    var normal_failure: task.CreateFailure = .none;
    if (task.createKernelThreadWithFailure("taskreg-normal-admission", shortWorker, &normal_failure)) |unexpected| {
        _ = task.killIdentity(unexpected.id, unexpected.generation);
        _ = task.releaseDeadIdentity(unexpected.id, unexpected.generation);
        return false;
    }
    if (normal_failure != .task_metadata) return false;

    short_completed = 0;
    var critical_failure: task.CreateFailure = .none;
    const critical = task.createKernelThreadCriticalWithFailure("taskreg-critical", shortWorker, &critical_failure) orelse return false;
    if (critical_failure != .none or !task.pin(critical)) return false;
    const critical_id = critical.id;
    const critical_generation = critical.generation;
    var spins: usize = 0;
    while (critical.state != .dead and spins < 4096) : (spins += 1) scheduler.yield();
    if (critical.state != .dead or short_completed != 1) {
        _ = task.unpin(critical);
        return false;
    }
    if (!task.unpin(critical) or !task.releaseDeadIdentity(critical_id, critical_generation)) return false;
    const reserve_after = task.criticalReserveStats();
    if (reserve_after.total != reserve_before.total or reserve_after.available != reserve_before.available or reserve_after.in_use != reserve_before.in_use) return false;

    short_completed = 0;
    var recovery_failure: task.CreateFailure = .none;
    const recovery = task.createKernelThreadWithFailure("taskreg-recovery", shortWorker, &recovery_failure) orelse return false;
    if (recovery_failure != .none or !task.pin(recovery)) return false;
    const recovery_id = recovery.id;
    const recovery_generation = recovery.generation;
    spins = 0;
    while (recovery.state != .dead and spins < 4096) : (spins += 1) scheduler.yield();
    if (recovery.state != .dead or short_completed != 1) {
        _ = task.unpin(recovery);
        return false;
    }
    if (!task.unpin(recovery) or !task.releaseDeadIdentity(recovery_id, recovery_generation)) return false;
    return true;
}

fn takeBaseline() Baseline {
    const heap_stats = heap.stats();
    const phys_stats = phys.stats();
    const virt_stats = virt.stats();
    return .{
        .task_count = task.count(),
        .heap_used = heap_stats.used_bytes,
        .heap_blocks = heap_stats.active_blocks,
        .pmm_free = phys_stats.free_frames,
        .vm_ranges = virt_stats.active_ranges,
        .vm_reserved = virt_stats.reserved_bytes,
        .vm_committed = virt_stats.committed_bytes,
        .vm_resident = virt_stats.resident_bytes,
        .fpu_live = task.validFpuStateCount(),
    };
}

fn matchesBaseline(expected: Baseline) bool {
    const current = takeBaseline();
    return current.task_count == expected.task_count and
        current.heap_used == expected.heap_used and
        current.heap_blocks == expected.heap_blocks and
        current.pmm_free == expected.pmm_free and
        current.vm_ranges == expected.vm_ranges and
        current.vm_reserved == expected.vm_reserved and
        current.vm_committed == expected.vm_committed and
        current.vm_resident == expected.vm_resident and
        current.fpu_live == expected.fpu_live and
        fpu.activeStateBytes() != 0;
}

fn logBaselineDelta(expected: Baseline) void {
    const current = takeBaseline();
    k.puts("TASKREG05910 baseline expected/current task=");
    k.putDec(expected.task_count);
    k.puts("/");
    k.putDec(current.task_count);
    k.puts(" heap=");
    k.putDec(expected.heap_used);
    k.puts("/");
    k.putDec(current.heap_used);
    k.puts(" blocks=");
    k.putDec(expected.heap_blocks);
    k.puts("/");
    k.putDec(current.heap_blocks);
    k.puts(" pmm=");
    k.putDec(expected.pmm_free);
    k.puts("/");
    k.putDec(current.pmm_free);
    k.puts(" vm=");
    k.putDec(expected.vm_ranges);
    k.puts("/");
    k.putDec(current.vm_ranges);
    k.puts(" reserved=");
    k.putDec(expected.vm_reserved);
    k.puts("/");
    k.putDec(current.vm_reserved);
    k.puts(" committed=");
    k.putDec(expected.vm_committed);
    k.puts("/");
    k.putDec(current.vm_committed);
    k.puts(" resident=");
    k.putDec(expected.vm_resident);
    k.puts("/");
    k.putDec(current.vm_resident);
    k.puts(" fpu=");
    k.putDec(expected.fpu_live);
    k.puts("/");
    k.putDec(current.fpu_live);
    k.puts("\r\n");
}

fn fail(reason: []const u8) bool {
    k.puts("TASKREG05910 result: FAILED reason=");
    k.puts(reason);
    k.puts("\r\n");
    return false;
}

fn phase(name: []const u8) void {
    k.puts("TASKREG05910 phase=");
    k.puts(name);
    k.puts("\r\n");
}
