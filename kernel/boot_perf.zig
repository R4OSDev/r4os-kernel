const crash = @import("crash.zig");
const timer = @import("timer.zig");
const time_core = @import("../platform/time.zig");

pub const MAX_PHASES: usize = 32;
const MAX_TIME_SPANS: usize = 128;

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
};

var initialized = false;
var boot_start_tick: u64 = 0;
var phase_count: usize = 0;
var transition_count: u64 = 0;
var current_phase: crash.BootPhase = .unknown;
var current_enter_tick: u64 = 0;
var boot_start_stamp: time_core.MonotonicStamp = .{};
var current_enter_stamp: time_core.MonotonicStamp = .{};
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
    phases = .{PhaseInfo{}} ** MAX_PHASES;
    time_spans = .{TimeSpan{}} ** MAX_TIME_SPANS;
    time_span_count = 0;
    timing_dropped_spans = 0;
    record(.entry);
}

pub fn record(phase: crash.BootPhase) void {
    if (!initialized) init();
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

pub fn snapshot() Summary {
    const now = timer.tickCount();
    const now_stamp = time_core.monotonicCapture();
    const clock = time_core.monotonicSnapshot();
    const total_ns = time_core.monotonicElapsed(boot_start_stamp, now_stamp);
    const unavailable = unavailableSpanCount(now_stamp);
    return .{
        .initialized = initialized,
        .boot_start_tick = boot_start_tick,
        .now_tick = now,
        .total_ticks = if (now >= boot_start_tick) now - boot_start_tick else 0,
        .phase_count = @intCast(@min(phase_count, @as(usize, 0xFFFF_FFFF))),
        .transition_count = transition_count,
        .current_phase = current_phase,
        .now_ns = time_core.monotonicResolve(now_stamp) orelse 0,
        .total_ns = total_ns orelse 0,
        .clock_flags = clock.flags,
        .clock_source = @intFromEnum(clock.source),
        .clock_generation = clock.generation,
        .clock_resolution_ns = clock.resolution_ns,
        .timing_valid = total_ns != null,
        .timing_span_count = @intCast(@min(time_span_count + @as(usize, if (current_phase == .unknown) 0 else 1), @as(usize, 0xFFFF_FFFF))),
        .timing_unavailable_spans = unavailable,
        .timing_dropped_spans = timing_dropped_spans,
    };
}

pub fn phaseAt(index: u32) ?PhaseInfo {
    const idx: usize = @intCast(index);
    if (idx >= phase_count) return null;
    var out = phases[idx];
    if (out.phase == current_phase) {
        const now = timer.tickCount();
        if (now >= current_enter_tick) out.total_ticks +%= now - current_enter_tick;
        out.last_tick = now;
    }
    populatePhaseTiming(&out, time_core.monotonicCapture());
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
    if (out.phase == current_phase) {
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

fn unavailableSpanCount(now_stamp: time_core.MonotonicStamp) u32 {
    var count: u32 = 0;
    for (time_spans[0..time_span_count]) |span| {
        if (time_core.monotonicElapsed(span.start, span.end) == null) count +|= 1;
    }
    if (current_phase != .unknown and time_core.monotonicElapsed(current_enter_stamp, now_stamp) == null) count +|= 1;
    return count;
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
