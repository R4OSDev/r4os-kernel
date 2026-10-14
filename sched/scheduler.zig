const native_tasks = @import("task.zig");
// Private C facade. Rust owns scheduling, CPU slots, counters and switch policy.
const task = @import("task.zig");
const timer = @import("../kernel/timer.zig");
const k = @import("../kernel/log.zig");
comptime {
    _ = @import("scheduler_bridge.zig");
}
pub const WAIT_FOREVER: u64 = 0xFFFF_FFFF_FFFF_FFFF;
pub const timer_wait_object: u64 = 0x5449_4D45_5741_4954;
pub const preemption_supported: u32 = 1;
pub const preemption_enabled: u32 = 1;
pub const preemption_test_mode: u32 = 0;
pub const preemption_quantum_ticks: u32 = @intCast(@max(1, (30 * timer.DEFAULT_HZ) / 1000));
pub const long_running_warn_ticks: u64 = @max(1, (1000 * @as(u64, timer.DEFAULT_HZ)) / 1000);
pub const starvation_warn_ticks: u64 = @max(1, (2000 * @as(u64, timer.DEFAULT_HZ)) / 1000);
pub const ready_latency_bucket_count: usize = 5;
pub const Stats = extern struct {
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

pub const CpuPreemptionStats = extern struct {
    timer_switches: u64 = 0,
    reschedule_ipi_switches: u64 = 0,
};

pub const StructureStats = extern struct {
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

comptime {
    if (@sizeOf(Stats) != 648) @compileError("Native Stats size differs");
    if (@offsetOf(Stats, "initialized") != 0) @compileError("Native Stats.initialized offset differs");
    if (@offsetOf(Stats, "current_index") != 4) @compileError("Native Stats.current_index offset differs");
    if (@offsetOf(Stats, "yields") != 8) @compileError("Native Stats.yields offset differs");
    if (@offsetOf(Stats, "sleeps") != 16) @compileError("Native Stats.sleeps offset differs");
    if (@offsetOf(Stats, "wakes") != 24) @compileError("Native Stats.wakes offset differs");
    if (@offsetOf(Stats, "idle_waits") != 32) @compileError("Native Stats.idle_waits offset differs");
    if (@offsetOf(Stats, "object_waits") != 40) @compileError("Native Stats.object_waits offset differs");
    if (@offsetOf(Stats, "object_wakes") != 48) @compileError("Native Stats.object_wakes offset differs");
    if (@offsetOf(Stats, "object_timeouts") != 56) @compileError("Native Stats.object_timeouts offset differs");
    if (@offsetOf(Stats, "object_cancels") != 64) @compileError("Native Stats.object_cancels offset differs");
    if (@offsetOf(Stats, "ticks") != 72) @compileError("Native Stats.ticks offset differs");
    if (@offsetOf(Stats, "preemption_supported") != 80) @compileError("Native Stats.preemption_supported offset differs");
    if (@offsetOf(Stats, "preemption_enabled") != 84) @compileError("Native Stats.preemption_enabled offset differs");
    if (@offsetOf(Stats, "preemption_test_mode") != 88) @compileError("Native Stats.preemption_test_mode offset differs");
    if (@offsetOf(Stats, "preempt_disable_depth") != 92) @compileError("Native Stats.preempt_disable_depth offset differs");
    if (@offsetOf(Stats, "preempt_disable_max_depth") != 96) @compileError("Native Stats.preempt_disable_max_depth offset differs");
    if (@offsetOf(Stats, "preempt_disable_calls") != 104) @compileError("Native Stats.preempt_disable_calls offset differs");
    if (@offsetOf(Stats, "preempt_enable_calls") != 112) @compileError("Native Stats.preempt_enable_calls offset differs");
    if (@offsetOf(Stats, "preempt_disable_underflows") != 120) @compileError("Native Stats.preempt_disable_underflows offset differs");
    if (@offsetOf(Stats, "preemption_simulation_ticks") != 128) @compileError("Native Stats.preemption_simulation_ticks offset differs");
    if (@offsetOf(Stats, "preemption_eligible_ticks") != 136) @compileError("Native Stats.preemption_eligible_ticks offset differs");
    if (@offsetOf(Stats, "preemption_deferred_disabled") != 144) @compileError("Native Stats.preemption_deferred_disabled offset differs");
    if (@offsetOf(Stats, "preemption_deferred_critical") != 152) @compileError("Native Stats.preemption_deferred_critical offset differs");
    if (@offsetOf(Stats, "preemption_deferred_no_task") != 160) @compileError("Native Stats.preemption_deferred_no_task offset differs");
    if (@offsetOf(Stats, "preemption_deferred_no_ready") != 168) @compileError("Native Stats.preemption_deferred_no_ready offset differs");
    if (@offsetOf(Stats, "preemption_deferred_quantum") != 176) @compileError("Native Stats.preemption_deferred_quantum offset differs");
    if (@offsetOf(Stats, "preemption_deferred_kernel_ip") != 184) @compileError("Native Stats.preemption_deferred_kernel_ip offset differs");
    if (@offsetOf(Stats, "preemption_switch_ticks") != 192) @compileError("Native Stats.preemption_switch_ticks offset differs");
    if (@offsetOf(Stats, "preemption_quantum_ticks") != 200) @compileError("Native Stats.preemption_quantum_ticks offset differs");
    if (@offsetOf(Stats, "preemption_quantum_expired") != 208) @compileError("Native Stats.preemption_quantum_expired offset differs");
    if (@offsetOf(Stats, "preemption_app_code_ticks") != 216) @compileError("Native Stats.preemption_app_code_ticks offset differs");
    if (@offsetOf(Stats, "long_running_task_warnings") != 224) @compileError("Native Stats.long_running_task_warnings offset differs");
    if (@offsetOf(Stats, "starvation_warnings") != 232) @compileError("Native Stats.starvation_warnings offset differs");
    if (@offsetOf(Stats, "ready_latency_samples") != 240) @compileError("Native Stats.ready_latency_samples offset differs");
    if (@offsetOf(Stats, "ready_latency_total_ticks") != 248) @compileError("Native Stats.ready_latency_total_ticks offset differs");
    if (@offsetOf(Stats, "ready_latency_max_ticks") != 256) @compileError("Native Stats.ready_latency_max_ticks offset differs");
    if (@offsetOf(Stats, "ready_latency_last_ticks") != 264) @compileError("Native Stats.ready_latency_last_ticks offset differs");
    if (@offsetOf(Stats, "ready_waiting_max_ticks") != 272) @compileError("Native Stats.ready_waiting_max_ticks offset differs");
    if (@offsetOf(Stats, "wait_object_total_ticks") != 280) @compileError("Native Stats.wait_object_total_ticks offset differs");
    if (@offsetOf(Stats, "wait_object_max_ticks") != 288) @compileError("Native Stats.wait_object_max_ticks offset differs");
    if (@offsetOf(Stats, "wait_object_last_ticks") != 296) @compileError("Native Stats.wait_object_last_ticks offset differs");
    if (@offsetOf(Stats, "run_without_switch_max_ticks") != 304) @compileError("Native Stats.run_without_switch_max_ticks offset differs");
    if (@offsetOf(Stats, "quantum_overrun_count") != 312) @compileError("Native Stats.quantum_overrun_count offset differs");
    if (@offsetOf(Stats, "quantum_overrun_max_ticks") != 320) @compileError("Native Stats.quantum_overrun_max_ticks offset differs");
    if (@offsetOf(Stats, "preemption_deferred_max_ticks") != 328) @compileError("Native Stats.preemption_deferred_max_ticks offset differs");
    if (@offsetOf(Stats, "long_running_warn_threshold_ticks") != 336) @compileError("Native Stats.long_running_warn_threshold_ticks offset differs");
    if (@offsetOf(Stats, "starvation_warn_threshold_ticks") != 344) @compileError("Native Stats.starvation_warn_threshold_ticks offset differs");
    if (@offsetOf(Stats, "priority_selects") != 352) @compileError("Native Stats.priority_selects offset differs");
    if (@offsetOf(Stats, "priority_picks_high") != 360) @compileError("Native Stats.priority_picks_high offset differs");
    if (@offsetOf(Stats, "priority_picks_normal") != 368) @compileError("Native Stats.priority_picks_normal offset differs");
    if (@offsetOf(Stats, "priority_picks_low") != 376) @compileError("Native Stats.priority_picks_low offset differs");
    if (@offsetOf(Stats, "priority_rr_picks") != 384) @compileError("Native Stats.priority_rr_picks offset differs");
    if (@offsetOf(Stats, "role_picks") != 392) @compileError("Native Stats.role_picks offset differs");
    if (@offsetOf(Stats, "ready_latency_role_samples") != 424) @compileError("Native Stats.ready_latency_role_samples offset differs");
    if (@offsetOf(Stats, "ready_latency_role_buckets") != 456) @compileError("Native Stats.ready_latency_role_buckets offset differs");
    if (@offsetOf(Stats, "safe_reschedule_points") != 616) @compileError("Native Stats.safe_reschedule_points offset differs");
    if (@offsetOf(Stats, "safe_reschedule_switches") != 624) @compileError("Native Stats.safe_reschedule_switches offset differs");
    if (@offsetOf(Stats, "safe_reschedule_deferred_irq") != 632) @compileError("Native Stats.safe_reschedule_deferred_irq offset differs");
    if (@offsetOf(Stats, "safe_reschedule_deferred_owner") != 640) @compileError("Native Stats.safe_reschedule_deferred_owner offset differs");
    if (@sizeOf(CpuPreemptionStats) != 16) @compileError("Native CpuPreemptionStats size differs");
    if (@offsetOf(CpuPreemptionStats, "timer_switches") != 0) @compileError("Native CpuPreemptionStats.timer_switches offset differs");
    if (@offsetOf(CpuPreemptionStats, "reschedule_ipi_switches") != 8) @compileError("Native CpuPreemptionStats.reschedule_ipi_switches offset differs");
    if (@sizeOf(StructureStats) != 88) @compileError("Native StructureStats size differs");
    if (@offsetOf(StructureStats, "ready_candidate_visits") != 0) @compileError("Native StructureStats.ready_candidate_visits offset differs");
    if (@offsetOf(StructureStats, "timeout_candidate_visits") != 8) @compileError("Native StructureStats.timeout_candidate_visits offset differs");
    if (@offsetOf(StructureStats, "warning_candidate_visits") != 16) @compileError("Native StructureStats.warning_candidate_visits offset differs");
    if (@offsetOf(StructureStats, "timeout_storm_deferrals") != 24) @compileError("Native StructureStats.timeout_storm_deferrals offset differs");
    if (@offsetOf(StructureStats, "wakeup_reschedule_requests") != 32) @compileError("Native StructureStats.wakeup_reschedule_requests offset differs");
    if (@offsetOf(StructureStats, "wakeup_preemption_switches") != 40) @compileError("Native StructureStats.wakeup_preemption_switches offset differs");
    if (@offsetOf(StructureStats, "safe_reschedule_points") != 48) @compileError("Native StructureStats.safe_reschedule_points offset differs");
    if (@offsetOf(StructureStats, "safe_reschedule_switches") != 56) @compileError("Native StructureStats.safe_reschedule_switches offset differs");
    if (@offsetOf(StructureStats, "safe_reschedule_deferred_irq") != 64) @compileError("Native StructureStats.safe_reschedule_deferred_irq offset differs");
    if (@offsetOf(StructureStats, "safe_reschedule_deferred_owner") != 72) @compileError("Native StructureStats.safe_reschedule_deferred_owner offset differs");
    if (@offsetOf(StructureStats, "reschedule_pending") != 80) @compileError("Native StructureStats.reschedule_pending offset differs");
}
pub const ExternalIrqFpuGuard = extern struct {
    interrupted_task: ?*task.Task = null,
    interrupted_generation: u64 = 0,
    restore_task_state: bool = false,
    armed: bool = false,
};
extern fn r4os_rust_scheduler_control(operation: u8, index: u32, value: u64) u64;
extern fn r4os_rust_scheduler_task(operation: u8, target: ?*task.Task, a: u64, b: u64, text: ?[*]const u8, len: usize) u64;
extern fn r4os_rust_scheduler_tick(now: u64, preemptible: bool, secondary: bool) bool;
extern fn r4os_rust_scheduler_stats(output: *Stats) void;
extern fn r4os_rust_scheduler_structure(output: *StructureStats) void;
extern fn r4os_rust_scheduler_cpu_stats(index: u32, output: *CpuPreemptionStats) void;
extern fn r4os_rust_scheduler_irq_enter(output: *ExternalIrqFpuGuard) void;
extern fn r4os_rust_scheduler_irq_leave(input: *const ExternalIrqFpuGuard) void;
comptime {
    if (@sizeOf(ExternalIrqFpuGuard) != 24 or @offsetOf(ExternalIrqFpuGuard, "restore_task_state") != 16)
        @compileError("Native scheduler IRQ token differs");
}
pub fn init() bool {
    return r4os_rust_scheduler_control(0, 0, 0) != 0;
}
pub fn prepareSecondary(index: u32) bool {
    return r4os_rust_scheduler_control(1, index, 0) != 0;
}
pub fn initSecondary(index: u32) bool {
    return r4os_rust_scheduler_control(2, index, 0) != 0;
}
pub fn secondaryLoop() noreturn {
    _ = r4os_rust_scheduler_control(3, 0, 0);
    unreachable;
}
pub fn current() ?*task.Task {
    const raw = r4os_rust_scheduler_control(4, 0, 0);
    return if (raw == 0) null else @ptrFromInt(raw);
}
pub fn hasOtherReadyTask() bool {
    return r4os_rust_scheduler_control(5, 0, 0) != 0;
}
pub fn preemptDisable() void {
    _ = r4os_rust_scheduler_control(6, 0, 0);
}
pub fn preemptEnable() void {
    _ = r4os_rust_scheduler_control(7, 0, 0);
}
pub fn yield() void {
    _ = r4os_rust_scheduler_control(8, 0, 0);
}
pub fn preemptFromIrq() void {
    _ = r4os_rust_scheduler_control(9, 0, 0);
}
pub fn preemptPendingWake(preemptible: bool) bool {
    return r4os_rust_scheduler_control(10, 0, @intFromBool(preemptible)) != 0;
}
pub fn safeReschedulePoint() bool {
    return r4os_rust_scheduler_control(11, 0, 0) != 0;
}
pub fn exitCurrent() noreturn {
    _ = r4os_rust_scheduler_control(12, 0, 0);
    unreachable;
}
pub fn exitCurrentAndRetire() noreturn {
    _ = r4os_rust_scheduler_control(13, 0, 0);
    unreachable;
}
pub fn pollTimedWaitsBeforeIdle() bool {
    return r4os_rust_scheduler_control(14, 0, 0) != 0;
}
pub fn onRescheduleIpi(preemptible: bool) void {
    _ = r4os_rust_scheduler_control(15, 0, @intFromBool(preemptible));
}
pub fn prioritySelfTest() bool {
    return r4os_rust_scheduler_control(16, 0, 0) != 0;
}
pub fn onTick(now: u64, preemptible: bool) bool {
    return r4os_rust_scheduler_tick(now, preemptible, false);
}
pub fn onSecondaryTick(now: u64, preemptible: bool) bool {
    return r4os_rust_scheduler_tick(now, preemptible, true);
}
pub fn sleepTicks(ticks: u64) void {
    sleepTicksWithReason(ticks, "sleep");
}
pub fn sleepTicksWithReason(ticks: u64, reason: []const u8) void {
    _ = r4os_rust_scheduler_task(4, null, ticks, 0, reason.ptr, reason.len);
}
pub fn blockCurrent(object: u64, timeout: u64, reason: []const u8) ?*task.Task {
    const raw = r4os_rust_scheduler_task(0, null, object, timeout, reason.ptr, reason.len);
    return if (raw == 0) null else @ptrFromInt(raw);
}
pub fn parkBlocked(blocked: *task.Task) void {
    _ = r4os_rust_scheduler_task(1, blocked, 0, 0, null, 0);
}
pub fn wakeTask(target: *task.Task, result: task.WaitResult) bool {
    return r4os_rust_scheduler_task(2, target, @intFromEnum(result), 0, null, 0) != 0;
}
pub fn publishCreatedTask(target: *task.Task) bool {
    return r4os_rust_scheduler_task(3, target, 0, 0, null, 0) != 0;
}
pub fn currentId() ?u32 {
    return if (current()) |t| native_tasks.get(t, .id) else null;
}
pub fn currentName() ?[]const u8 {
    return if (current()) |t| native_tasks.get(t, .name) else null;
}
pub fn stats() Stats {
    var value: Stats = undefined;
    r4os_rust_scheduler_stats(&value);
    return value;
}
pub fn structureStats() StructureStats {
    var value: StructureStats = undefined;
    r4os_rust_scheduler_structure(&value);
    return value;
}
pub fn cpuPreemptionStats(index: u32) CpuPreemptionStats {
    var value: CpuPreemptionStats = undefined;
    r4os_rust_scheduler_cpu_stats(index, &value);
    return value;
}
pub fn enterExternalIrqFpuGuard() ExternalIrqFpuGuard {
    var value: ExternalIrqFpuGuard = undefined;
    r4os_rust_scheduler_irq_enter(&value);
    return value;
}
pub fn leaveExternalIrqFpuGuard(value: ExternalIrqFpuGuard) void {
    r4os_rust_scheduler_irq_leave(&value);
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
    const st = stats();
    k.puts("Scheduler status\r\n");
    k.puts("  Initialized: ");
    k.puts(if (st.initialized) "yes" else "no");
    k.puts(" current_index=");
    k.putDec(st.current_index);
    k.puts(" ticks=");
    k.putDec(timer.tickCount());
    k.puts("\r\n");
    k.puts("  Counters: yields=");
    k.putDec(st.yields);
    k.puts(" sleeps=");
    k.putDec(st.sleeps);
    k.puts(" wakes=");
    k.putDec(st.wakes);
    k.puts(" idle_waits=");
    k.putDec(st.idle_waits);
    k.puts(" object_waits=");
    k.putDec(st.object_waits);
    k.puts(" object_wakes=");
    k.putDec(st.object_wakes);
    k.puts(" object_timeouts=");
    k.putDec(st.object_timeouts);
    k.puts(" object_cancels=");
    k.putDec(st.object_cancels);
    k.puts("\r\n");
    k.puts("  Preemption: supported=");
    k.putDec(preemption_supported);
    k.puts(" enabled=");
    k.putDec(preemption_enabled);
    k.puts(" test=");
    k.putDec(preemption_test_mode);
    k.puts(" depth=");
    k.putDec(st.preempt_disable_depth);
    k.puts(" max=");
    k.putDec(st.preempt_disable_max_depth);
    k.puts(" sim=");
    k.putDec(st.preemption_simulation_ticks);
    k.puts(" eligible=");
    k.putDec(st.preemption_eligible_ticks);
    k.puts(" disabled=");
    k.putDec(st.preemption_deferred_disabled);
    k.puts(" critical=");
    k.putDec(st.preemption_deferred_critical);
    k.puts(" no_ready=");
    k.putDec(st.preemption_deferred_no_ready);
    k.puts(" quantum=");
    k.putDec(st.preemption_deferred_quantum);
    k.puts(" kernel_ip=");
    k.putDec(st.preemption_deferred_kernel_ip);
    k.puts(" long=");
    k.putDec(st.long_running_task_warnings);
    k.puts(" starve=");
    k.putDec(st.starvation_warnings);
    k.puts(" ready_max=");
    k.putDec(st.ready_latency_max_ticks);
    k.puts(" wait_max=");
    k.putDec(st.wait_object_max_ticks);
    k.puts(" run_max=");
    k.putDec(st.run_without_switch_max_ticks);
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
    k.putDec(structureStats().timeout_storm_deferrals);
    k.puts("\r\n");
    dumpCurrent();
    task.dump();
}
