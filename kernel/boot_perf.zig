const crash = @import("crash.zig");
const timer = @import("timer.zig");
const time_core = @import("../platform/time.zig");

pub const MAX_PHASES: usize = 32;
const MAX_TIME_SPANS: usize = 128;

pub const CompletionState = enum(u32) {
    uninitialized = 0,
    running = 1,
    ready = 2,
    fallback_ready = 3,
    failed = 4,
};

pub const CompletionReason = enum(u32) {
    none = 0,
    configured_shell_ready = 1,
    terminal_fallback_ready = 2,
    recovery_fallback_ready = 3,
    no_shell = 4,
    fatal_error = 5,
    shell_exited_before_ready = 6,
};

pub const ShellKind = enum(u32) {
    none = 0,
    configured = 1,
    terminal_fallback = 2,
    recovery_fallback = 3,
};

pub const ready_result_completed: i32 = 0;
pub const ready_result_already_completed: i32 = 1;
pub const ready_error_not_expected_shell: i32 = -1;
pub const ready_error_boot_failed: i32 = -2;

const TimeSpan = struct {
    phase: crash.BootPhase = .unknown,
    start: time_core.MonotonicStamp = .{},
    end: time_core.MonotonicStamp = .{},
};

pub const PhaseInfo = struct {
    phase: crash.BootPhase = .unknown,
    first_tick: u64 = 0,
    last_tick: u64 = 0,
    total_ticks: u64 = 0,
    transitions: u32 = 0,
    first_ns: u64 = 0,
    last_ns: u64 = 0,
    total_ns: u64 = 0,
    timing_valid: bool = false,
    timing_unavailable_spans: u32 = 0,
};

pub const Summary = struct {
    initialized: bool = false,
    state: CompletionState = .uninitialized,
    completion_reason: CompletionReason = .none,
    boot_start_tick: u64 = 0,
    now_tick: u64 = 0,
    total_ticks: u64 = 0,
    phase_count: u32 = 0,
    transition_count: u64 = 0,
    current_phase: crash.BootPhase = .unknown,
    now_ns: u64 = 0,
    total_ns: u64 = 0,
    clock_flags: u32 = 0,
    clock_source: u32 = 0,
    clock_generation: u32 = 0,
    clock_resolution_ns: u64 = 0,
    timing_valid: bool = false,
    timing_span_count: u32 = 0,
    timing_unavailable_spans: u32 = 0,
    timing_dropped_spans: u32 = 0,
    configured_attempts: u32 = 0,
    fallback_attempts: u32 = 0,
    launch_failures: u32 = 0,
    shell_instance_id: u32 = 0,
};

var initialized = false;
var boot_start_tick: u64 = 0;
var phase_count: usize = 0;
var transition_count: u64 = 0;
var current_phase: crash.BootPhase = .unknown;
var current_enter_tick: u64 = 0;
var boot_start_stamp: time_core.MonotonicStamp = .{};
var current_enter_stamp: time_core.MonotonicStamp = .{};
var completion_state: CompletionState = .uninitialized;
var completion_reason: CompletionReason = .none;
var boot_end_tick: u64 = 0;
var boot_end_stamp: time_core.MonotonicStamp = .{};
var completion_clock_flags: u32 = 0;
var completion_clock_source: u32 = 0;
var completion_clock_generation: u32 = 0;
var completion_clock_resolution_ns: u64 = 0;
var active_shell_kind: ShellKind = .none;
var expected_shell_instance_id: u32 = 0;
var configured_attempts: u32 = 0;
var fallback_attempts: u32 = 0;
var launch_failures: u32 = 0;
var phases: [MAX_PHASES]PhaseInfo = .{PhaseInfo{}} ** MAX_PHASES;
var time_spans: [MAX_TIME_SPANS]TimeSpan = .{TimeSpan{}} ** MAX_TIME_SPANS;
var time_span_count: usize = 0;
var timing_dropped_spans: u32 = 0;

pub fn init() void {
    initialized = true;
    boot_start_tick = timer.tickCount();
    boot_start_stamp = time_core.monotonicCapture();
    phase_count = 0;
    transition_count = 0;
    current_phase = .unknown;
    current_enter_tick = boot_start_tick;
    current_enter_stamp = boot_start_stamp;
    completion_state = .running;
    completion_reason = .none;
    boot_end_tick = 0;
    boot_end_stamp = .{};
    completion_clock_flags = 0;
    completion_clock_source = 0;
    completion_clock_generation = 0;
    completion_clock_resolution_ns = 0;
    active_shell_kind = .none;
    expected_shell_instance_id = 0;
    configured_attempts = 0;
    fallback_attempts = 0;
    launch_failures = 0;
    phases = .{PhaseInfo{}} ** MAX_PHASES;
    time_spans = .{TimeSpan{}} ** MAX_TIME_SPANS;
    time_span_count = 0;
    timing_dropped_spans = 0;
    record(.entry);
}

pub fn record(phase: crash.BootPhase) void {
    if (!initialized) init();
    if (completion_state != .running) return;
    const now = timer.tickCount();
    const now_stamp = time_core.monotonicCapture();
    finishCurrent(now, now_stamp);
    transition_count +%= 1;
    current_phase = phase;
    current_enter_tick = now;
    current_enter_stamp = now_stamp;
    const slot = phaseSlot(phase) orelse return;
    var p = &phases[slot];
    if (p.transitions == 0) {
        p.phase = phase;
        p.first_tick = now;
    }
    p.last_tick = now;
    p.transitions +%= 1;
}

pub fn beginShellAttempt(kind: ShellKind) void {
    if (!initialized) init();
    if (completion_state != .running) return;
    active_shell_kind = kind;
    expected_shell_instance_id = 0;
    switch (kind) {
        .configured => configured_attempts +|= 1,
        .terminal_fallback, .recovery_fallback => fallback_attempts +|= 1,
        .none => {},
    }
}

pub fn noteShellLaunched(instance_id: u32) void {
    if (completion_state != .running or active_shell_kind == .none or instance_id == 0) return;
    expected_shell_instance_id = instance_id;
}

pub fn noteShellLaunchFailure() void {
    if (completion_state != .running) return;
    launch_failures +|= 1;
    expected_shell_instance_id = 0;
}

pub fn completeReady(instance_id: u32) i32 {
    switch (completion_state) {
        .ready, .fallback_ready => return if (instance_id != 0 and instance_id == expected_shell_instance_id)
            ready_result_already_completed
        else
            ready_error_not_expected_shell,
        .failed => return ready_error_boot_failed,
        .uninitialized, .running => {},
    }
    if (active_shell_kind == .none or instance_id == 0 or instance_id != expected_shell_instance_id)
        return ready_error_not_expected_shell;

    const state: CompletionState = switch (active_shell_kind) {
        .configured => .ready,
        .terminal_fallback, .recovery_fallback => .fallback_ready,
        .none => unreachable,
    };
    const reason: CompletionReason = switch (active_shell_kind) {
        .configured => .configured_shell_ready,
        .terminal_fallback => .terminal_fallback_ready,
        .recovery_fallback => .recovery_fallback_ready,
        .none => unreachable,
    };
    freeze(state, reason);
    return ready_result_completed;
}

pub fn failNoShell() void {
    if (!initialized) init();
    if (completion_state != .running) return;
    freeze(.failed, .no_shell);
}

pub fn failFatal() void {
    if (!initialized) init();
    if (completion_state != .running) return;
    freeze(.failed, .fatal_error);
}

pub fn failShellExited(instance_id: u32) void {
    if (completion_state != .running or instance_id == 0 or instance_id != expected_shell_instance_id) return;
    freeze(.failed, .shell_exited_before_ready);
}

pub fn snapshot() Summary {
    const running = completion_state == .running;
    const now = if (running) timer.tickCount() else boot_end_tick;
    const now_stamp = if (running) time_core.monotonicCapture() else boot_end_stamp;
    const live_clock = time_core.monotonicSnapshot();
    const total_ns = time_core.monotonicElapsed(boot_start_stamp, now_stamp);
    const unavailable = unavailableSpanCount(now_stamp, running);
    return .{
        .initialized = initialized,
        .state = completion_state,
        .completion_reason = completion_reason,
        .boot_start_tick = boot_start_tick,
        .now_tick = now,
        .total_ticks = if (now >= boot_start_tick) now - boot_start_tick else 0,
        .phase_count = @intCast(@min(phase_count, @as(usize, 0xFFFF_FFFF))),
        .transition_count = transition_count,
        .current_phase = current_phase,
        .now_ns = time_core.monotonicResolve(now_stamp) orelse 0,
        .total_ns = total_ns orelse 0,
        .clock_flags = if (running) live_clock.flags else completion_clock_flags,
        .clock_source = if (running) @intFromEnum(live_clock.source) else completion_clock_source,
        .clock_generation = if (running) live_clock.generation else completion_clock_generation,
        .clock_resolution_ns = if (running) live_clock.resolution_ns else completion_clock_resolution_ns,
        .timing_valid = total_ns != null,
        .timing_span_count = @intCast(@min(time_span_count + @as(usize, if (running and current_phase != .unknown) 1 else 0), @as(usize, 0xFFFF_FFFF))),
        .timing_unavailable_spans = unavailable,
        .timing_dropped_spans = timing_dropped_spans,
        .configured_attempts = configured_attempts,
        .fallback_attempts = fallback_attempts,
        .launch_failures = launch_failures,
        .shell_instance_id = expected_shell_instance_id,
    };
}

pub fn phaseAt(index: u32) ?PhaseInfo {
    const idx: usize = @intCast(index);
    if (idx >= phase_count) return null;
    var out = phases[idx];
    if (out.phase == current_phase and completion_state == .running) {
        const now = timer.tickCount();
        if (now >= current_enter_tick) out.total_ticks +%= now - current_enter_tick;
        out.last_tick = now;
    }
    populatePhaseTiming(&out, if (completion_state == .running) time_core.monotonicCapture() else boot_end_stamp);
    return out;
}

fn finishCurrent(now: u64, now_stamp: time_core.MonotonicStamp) void {
    if (current_phase == .unknown) return;
    const slot = findPhase(current_phase) orelse return;
    if (now >= current_enter_tick) phases[slot].total_ticks +%= now - current_enter_tick;
    phases[slot].last_tick = now;
    if (time_span_count < time_spans.len) {
        time_spans[time_span_count] = .{
            .phase = current_phase,
            .start = current_enter_stamp,
            .end = now_stamp,
        };
        time_span_count += 1;
    } else {
        timing_dropped_spans +|= 1;
    }
}

fn populatePhaseTiming(out: *PhaseInfo, now_stamp: time_core.MonotonicStamp) void {
    var first: ?u64 = null;
    var last: ?u64 = null;
    var total: u64 = 0;
    var unavailable: u32 = 0;
    for (time_spans[0..time_span_count]) |span| {
        if (span.phase != out.phase) continue;
        const start_ns = time_core.monotonicResolve(span.start);
        const end_ns = time_core.monotonicResolve(span.end);
        const elapsed = time_core.monotonicElapsed(span.start, span.end);
        if (start_ns == null or end_ns == null or elapsed == null) {
            unavailable +|= 1;
            continue;
        }
        if (first == null) first = start_ns.?;
        last = end_ns.?;
        total +|= elapsed.?;
    }
    if (out.phase == current_phase and completion_state == .running) {
        const start_ns = time_core.monotonicResolve(current_enter_stamp);
        const end_ns = time_core.monotonicResolve(now_stamp);
        const elapsed = time_core.monotonicElapsed(current_enter_stamp, now_stamp);
        if (start_ns == null or end_ns == null or elapsed == null) {
            unavailable +|= 1;
        } else {
            if (first == null) first = start_ns.?;
            last = end_ns.?;
            total +|= elapsed.?;
        }
    }
    out.first_ns = first orelse 0;
    out.last_ns = last orelse 0;
    out.total_ns = total;
    out.timing_unavailable_spans = unavailable;
    out.timing_valid = first != null and last != null and unavailable == 0;
}

fn unavailableSpanCount(now_stamp: time_core.MonotonicStamp, include_current: bool) u32 {
    var count: u32 = 0;
    for (time_spans[0..time_span_count]) |span| {
        if (time_core.monotonicElapsed(span.start, span.end) == null) count +|= 1;
    }
    if (include_current and current_phase != .unknown and time_core.monotonicElapsed(current_enter_stamp, now_stamp) == null) count +|= 1;
    return count;
}

fn freeze(state: CompletionState, reason: CompletionReason) void {
    const now = timer.tickCount();
    const now_stamp = time_core.monotonicCapture();
    finishCurrent(now, now_stamp);
    const clock = time_core.monotonicSnapshot();
    boot_end_tick = now;
    boot_end_stamp = now_stamp;
    completion_clock_flags = clock.flags;
    completion_clock_source = @intFromEnum(clock.source);
    completion_clock_generation = clock.generation;
    completion_clock_resolution_ns = clock.resolution_ns;
    completion_state = state;
    completion_reason = reason;
}

fn phaseSlot(phase: crash.BootPhase) ?usize {
    if (findPhase(phase)) |slot| return slot;
    if (phase_count >= phases.len) return null;
    const slot = phase_count;
    phase_count += 1;
    return slot;
}

fn findPhase(phase: crash.BootPhase) ?usize {
    var i: usize = 0;
    while (i < phase_count) : (i += 1) {
        if (phases[i].phase == phase) return i;
    }
    return null;
}
