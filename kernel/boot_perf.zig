const crash = @import("crash.zig");
const timer = @import("timer.zig");

pub const MAX_PHASES: usize = 32;

pub const PhaseInfo = struct {
    phase: crash.BootPhase = .unknown,
    first_tick: u64 = 0,
    last_tick: u64 = 0,
    total_ticks: u64 = 0,
    transitions: u32 = 0,
};

pub const Summary = struct {
    initialized: bool = false,
    boot_start_tick: u64 = 0,
    now_tick: u64 = 0,
    total_ticks: u64 = 0,
    phase_count: u32 = 0,
    transition_count: u64 = 0,
    current_phase: crash.BootPhase = .unknown,
};

var initialized = false;
var boot_start_tick: u64 = 0;
var phase_count: usize = 0;
var transition_count: u64 = 0;
var current_phase: crash.BootPhase = .unknown;
var current_enter_tick: u64 = 0;
var phases: [MAX_PHASES]PhaseInfo = .{PhaseInfo{}} ** MAX_PHASES;

pub fn init() void {
    initialized = true;
    boot_start_tick = timer.tickCount();
    phase_count = 0;
    transition_count = 0;
    current_phase = .unknown;
    current_enter_tick = boot_start_tick;
    phases = .{PhaseInfo{}} ** MAX_PHASES;
    record(.entry);
}

pub fn record(phase: crash.BootPhase) void {
    if (!initialized) init();
    const now = timer.tickCount();
    finishCurrent(now);
    transition_count +%= 1;
    current_phase = phase;
    current_enter_tick = now;
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
    return .{
        .initialized = initialized,
        .boot_start_tick = boot_start_tick,
        .now_tick = now,
        .total_ticks = if (now >= boot_start_tick) now - boot_start_tick else 0,
        .phase_count = @intCast(@min(phase_count, @as(usize, 0xFFFF_FFFF))),
        .transition_count = transition_count,
        .current_phase = current_phase,
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
    return out;
}

fn finishCurrent(now: u64) void {
    if (current_phase == .unknown) return;
    const slot = findPhase(current_phase) orelse return;
    if (now >= current_enter_tick) phases[slot].total_ticks +%= now - current_enter_tick;
    phases[slot].last_tick = now;
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
