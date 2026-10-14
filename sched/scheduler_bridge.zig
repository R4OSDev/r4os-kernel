// Temporary Task-owner cuts. Rust schedules; Zig retains the exact Task object
// and intrusive queues until 0.83.11. No Task-native layout crosses the ABI.
const task = @import("task.zig");
const timer = @import("../kernel/timer.zig");
const percpu = @import("../arch/x86_64/percpu.zig");
const k = @import("../kernel/log.zig");
const scheduler = @import("scheduler.zig");
pub const TaskView = extern struct {
    rsp_slot: *u64,
    unwind_slot: *u32,
    entry: ?task.Entry,
    generation: u64,
    rsp: u64,
    wake_tick: u64,
    wait_object: u64,
    last_scheduled_tick: u64,
    ready_since_tick: u64,
    created_tick: u64,
    last_long_warning: u64,
    last_starvation_warning: u64,
    max_ready_latency: u64,
    id: u32,
    home_cpu: u32,
    preempt_depth: u32,
    held_locks: u32,
    unwind_count: u32,
    state: u8,
    role: u8,
    priority: u8,
    dispatch_rank: u8,
    cpu_idle: bool,
    smp_eligible: bool,
    smp_r4x: bool,
    uses_fpu: bool,
    fpu_valid: bool,
    fpu_present: bool,
    handoff_pending: bool,
};
comptime {
    if (@sizeOf(TaskView) != 136 or @offsetOf(TaskView, "id") != 104 or
        @offsetOf(TaskView, "state") != 124 or @offsetOf(TaskView, "handoff_pending") != 134)
        @compileError("Native scheduler Task projection differs");
    if (task.role_count != 4 or @intFromEnum(task.State.running) != 2 or
        @intFromEnum(task.State.blocked) != 3 or @intFromEnum(task.WaitResult.timeout) != 2)
        @compileError("Native scheduler Task tags differ");
}
export fn r4os_zig_scheduler_view(t: *task.Task, output: *TaskView) callconv(.c) void {
    output.* = .{
        .rsp_slot = &t.rsp,
        .unwind_slot = &t.unwind_guard_count,
        .entry = t.entry,
        .generation = t.generation,
        .rsp = t.rsp,
        .wake_tick = t.wake_tick,
        .wait_object = t.wait_object,
        .last_scheduled_tick = t.last_scheduled_tick,
        .ready_since_tick = t.ready_since_tick,
        .created_tick = t.created_tick,
        .last_long_warning = t.last_long_run_warning_tick,
        .last_starvation_warning = t.last_starvation_warning_tick,
        .max_ready_latency = t.max_ready_latency_ticks,
        .id = t.id,
        .home_cpu = t.home_cpu,
        .preempt_depth = t.preempt_disable_depth,
        .held_locks = t.held_lock_count,
        .unwind_count = t.unwind_guard_count,
        .state = @intFromEnum(t.state),
        .role = @intFromEnum(t.role),
        .priority = @intFromEnum(task.effectivePriority(t)),
        .dispatch_rank = task.dispatchRank(t),
        .cpu_idle = t.cpu_idle,
        .smp_eligible = t.smp_eligible,
        .smp_r4x = t.smp_r4x_work,
        .uses_fpu = t.uses_fpu,
        .fpu_valid = t.fpu_state_valid,
        .fpu_present = t.fpu_state != null,
        .handoff_pending = t.wait_handoff_guard_pending,
    };
}
fn pointer(value: ?*task.Task) u64 {
    return if (value) |t| @intFromPtr(t) else 0;
}
export fn r4os_zig_scheduler_query(operation: u8, target: ?*task.Task, value: u64) callconv(.c) u64 {
    return switch (operation) {
        0 => task.count(),
        1 => pointer(task.first()),
        2 => pointer(task.createCpuIdleTask(@intCast(value))),
        3 => task.readyCountForCpu(@intCast(value)),
        4 => pointer(task.firstReady(@intCast(value))),
        5 => pointer(task.firstReadyAtRank(@truncate(value), @intCast(value >> 32))),
        6 => pointer(task.nextReadyWarning(@intCast(value))),
        7 => @intFromBool(task.hasMoreUrgentReady(@intCast(value), target.?)),
        8 => task.minWakeTick(),
        9 => pointer(task.firstTimedWait()),
        10 => task.ordinalOf(target.?) orelse 0,
        11 => pointer(task.createKernelThread("st-prio-busy", @as(task.Entry, @ptrFromInt(value)))),
        else => unreachable,
    };
}
export fn r4os_zig_scheduler_mutate(operation: u8, target: ?*task.Task, a: u64, b: u64, text: ?[*]const u8, len: usize) callconv(.c) u64 {
    if (operation == 17) return task.reapDeferred();
    const t = target.?;
    switch (operation) {
        0 => task.markRunning(t),
        1 => task.recordYield(t, a),
        2 => task.markReady(t, a),
        3 => task.parkCpuIdle(t),
        4 => task.noteSwitchedOut(t),
        5 => return task.recordScheduled(t, a),
        6 => task.saveFpuState(t),
        7 => task.restoreFpuState(t),
        8 => return task.recordPreemptDisable(t),
        9 => return @intFromBool(task.recordPreemptEnable(t)),
        10 => task.recordPreemptionProbe(t),
        11 => task.recordPreemptionDeferred(t, a),
        12 => task.recordLongRunWarning(t, a),
        13 => task.recordStarvationWarning(t, a),
        14 => task.beginWait(t, a, if (text) |p| p[0..len] else "", b),
        15 => return task.finishWait(t, @enumFromInt(@as(u32, @intCast(a)))),
        16 => return @intFromBool(task.finishCurrentForExit(t, a != 0)),
        18 => return @intFromBool(task.assignRole(t, @enumFromInt(@as(u8, @intCast(a))))),
        19 => t.max_ready_latency_ticks = 0,
        21 => return task.recordRunTick(t, a),
        else => unreachable,
    }
    return 0;
}
export fn r4os_zig_scheduler_attach() callconv(.c) void {
    task.setCurrentProvider(scheduler.current);
}
export fn r4os_zig_scheduler_log(event: u8, target: ?*task.Task, a: u64, b: u64) callconv(.c) void {
    switch (event) {
        0 => {
            k.puts("[TIMER] deadline_queue=ordered idle=");
            k.puts(if (a != 0) "one-shot" else "periodic-fallback");
            k.puts(" backend=");
            k.puts(timer.backendName());
            k.puts("\r\n");
        },
        1 => {
            k.puts("[SMP] productive cpu=");
            k.putDec(a);
            k.puts(" task=");
            k.puts(target.?.name);
            k.puts(" class=r4x\r\n");
        },
        2 => {
            const t = target.?;
            k.puts("[SMP] switch boundary violation cpu=");
            k.putDec(a);
            k.puts(" task=");
            k.putDec(t.id);
            k.puts("/");
            k.puts(t.name);
            k.puts(" depth=");
            k.putDec(b);
            k.puts(" state=");
            k.puts(task.stateName(t.state));
            k.puts("\r\n");
        },
        3 => {
            const t = target.?;
            k.puts("TASK EXIT INVARIANT: owned synchronization remains id=");
            k.putDec(t.id);
            k.puts(" locks=");
            k.putDec(t.held_lock_count);
            k.puts(" unwind=");
            k.putDec(t.unwind_guard_count);
            k.puts(" objects=");
            var wrote = false;
            for (t.held_locks) |held| {
                if (!held.active) continue;
                if (wrote) k.puts(",");
                k.puts(held.name);
                k.puts("=");
                k.putHex(held.object_id, 16);
                k.puts("@");
                k.putDec(held.rank);
                wrote = true;
            }
            if (!wrote) k.puts("none");
            k.puts("\r\n");
        },
        4 => {
            k.puts("TASK EXIT INVARIANT: transition rejected id=");
            k.putDec(target.?.id);
            k.puts("\r\n");
        },
        5 => k.puts("IRQ FPU GUARD task identity changed inside handler\r\n"),
        6 => k.puts("SCHEDPRIO FAIL busy-spawn\r\n"),
        7 => {
            const st = scheduler.stats();
            k.puts(if (a != 0) "SCHEDPRIO OK" else "SCHEDPRIO FAIL");
            k.puts(" high_latency_max=");
            k.putDec(b);
            k.puts(" bar=3 picks_high=");
            k.putDec(st.priority_picks_high);
            k.puts(" picks_normal=");
            k.putDec(st.priority_picks_normal);
            k.puts(" rr_picks=");
            k.putDec(st.priority_rr_picks);
            k.puts(" selects=");
            k.putDec(st.priority_selects);
            k.puts("\r\n");
        },
        else => unreachable,
    }
}
