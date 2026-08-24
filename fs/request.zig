const diag_screen = @import("../kernel/diag_screen.zig");
const k = @import("../kernel/log.zig");
const scheduler = @import("../sched/scheduler.zig");
const sync = @import("../sched/sync.zig");
const timer = @import("../kernel/timer.zig");
const request_scope = @import("request_scope.zig");

pub const drive_gate_count: usize = request_scope.lane_count;

pub const Kind = enum(u32) {
    drive_info = 1,
    file_read = 2,
    file_read_at = 3,
    file_write = 4,
    file_append = 5,
    stream_begin = 6,
    stream_write = 7,
    stream_finish = 8,
    stream_abort = 9,
    dir_list = 10,
    dir_entry = 11,
    file_info = 12,
    file_delete = 13,
    dir_create = 14,
    dir_delete = 15,
    file_rename = 16,
    file_copy = 17,
    file_move = 18,
    loader_read = 19,
    config_read = 20,
    config_write = 21,
    file_write_at = 22,
    file_replace_atomic = 23,
    file_delete_if_match = 24,
    file_update_atomic_checked = 25,
};

pub const AtomicProgressPhase = enum(u32) {
    none = 0,
    lookup = 1,
    checksum = 2,
    replace = 3,
    cleanup = 4,
};

pub const Summary = struct {
    requests: u64 = 0,
    completed: u64 = 0,
    failed: u64 = 0,
    read_requests: u64 = 0,
    write_requests: u64 = 0,
    metadata_requests: u64 = 0,
    stream_requests: u64 = 0,
    lock_acquires: u64 = 0,
    lock_contention_waits: u64 = 0,
    lock_timeouts: u64 = 0,
    boot_bypass: u64 = 0,
    total_ticks: u64 = 0,
    max_ticks: u64 = 0,
    last_ticks: u64 = 0,
    active_kind: u32 = 0,
    last_kind: u32 = 0,
    active_drive: u32 = 0,
    last_drive: u32 = 0,
    active_phase: u32 = 0,
    active_progress_sequence: u32 = 0,
    active_progress: u64 = 0,
    active_progress_total: u64 = 0,
    single_drive_requests: u64 = 0,
    cross_drive_requests: u64 = 0,
    global_requests: u64 = 0,
    active_requests: u32 = 0,
    parallel_active_max: u32 = 0,
};

pub const Guard = struct {
    kind: Kind = .file_info,
    drive: u8 = 0,
    second_drive: u8 = 0,
    lanes: [request_scope.lane_count]u8 = .{0} ** request_scope.lane_count,
    lane_count: u8 = 0,
    gates_locked: bool = false,
    start_tick: u64 = 0,
    active: bool = false,
};

const LaneState = struct {
    kind: Kind = .file_info,
    drive: u8 = 0,
    active_depth: u32 = 0,
    progress_phase: AtomicProgressPhase = .none,
    progress_sequence: u32 = 0,
    progress: u64 = 0,
    progress_total: u64 = 0,
};

// Each volume owns its filesystem transaction lane. These owners deliberately
// span FAT/block-I/O waits, block hard task termination, and stay separate
// from sleep-under-lock diagnostics. Cross-volume requests acquire the two
// lanes in canonical drive order; ownership-free cleanup acquires all lanes.
var request_gates: [request_scope.lane_count]sync.UnwindGuard =
    .{sync.UnwindGuard.init("fs-drive")} ** request_scope.lane_count;
var lane_states: [request_scope.lane_count]LaneState = .{LaneState{}} ** request_scope.lane_count;
var stats: Summary = .{};

pub fn init() void {
    request_gates = .{sync.UnwindGuard.init("fs-drive")} ** request_scope.lane_count;
    lane_states = .{LaneState{}} ** request_scope.lane_count;
    stats = .{};
}

pub fn summary() Summary {
    var out = stats;
    out.active_kind = 0;
    out.active_drive = 0;
    out.active_phase = 0;
    out.active_progress = 0;
    out.active_progress_total = 0;
    var lane_index: usize = 0;
    while (lane_index < lane_states.len) : (lane_index += 1) {
        const state = lane_states[lane_index];
        if (state.active_depth == 0) continue;
        if (out.active_kind == 0) {
            out.active_kind = kindCode(state.kind);
            out.active_drive = request_scope.driveCode(state.drive);
            out.active_phase = @intFromEnum(state.progress_phase);
            out.active_progress_sequence = state.progress_sequence;
            out.active_progress = state.progress;
            out.active_progress_total = state.progress_total;
        }
    }
    return out;
}

pub fn begin(kind: Kind, drive_letter: u8) ?Guard {
    return beginPlan(kind, drive_letter, 0, request_scope.single(drive_letter));
}

pub fn beginPair(kind: Kind, first_drive: u8, second_drive: u8) ?Guard {
    return beginPlan(kind, first_drive, second_drive, request_scope.pair(first_drive, second_drive));
}

fn beginPlan(kind: Kind, drive_letter: u8, second_drive: u8, plan: request_scope.Plan) ?Guard {
    var acquired_count: u8 = 0;
    const runtime_owned = scheduler.currentId() != null;
    if (runtime_owned) {
        while (acquired_count < plan.count) : (acquired_count += 1) {
            const lane = plan.lanes[acquired_count];
            if (!acquireGate(lane)) {
                stats.lock_timeouts +%= 1;
                releasePlan(plan, acquired_count);
                return null;
            }
        }
        stats.lock_acquires +%= 1;
    } else {
        stats.boot_bypass +%= 1;
    }

    stats.requests +%= 1;
    switch (kind) {
        .file_read, .file_read_at, .loader_read, .config_read => stats.read_requests +%= 1,
        .file_write, .file_write_at, .file_append, .file_delete, .file_rename, .file_copy, .file_move, .file_replace_atomic, .file_delete_if_match, .file_update_atomic_checked, .config_write => stats.write_requests +%= 1,
        .stream_begin, .stream_write, .stream_finish, .stream_abort => stats.stream_requests +%= 1,
        else => stats.metadata_requests +%= 1,
    }
    if (plan.isGlobal()) {
        stats.global_requests +%= 1;
    } else if (plan.count == 2) {
        stats.cross_drive_requests +%= 1;
    } else {
        stats.single_drive_requests +%= 1;
    }
    stats.active_requests +|= 1;
    if (stats.active_requests > stats.parallel_active_max) {
        stats.parallel_active_max = stats.active_requests;
    }
    var lane_offset: u8 = 0;
    while (lane_offset < plan.count) : (lane_offset += 1) {
        const lane = plan.lanes[lane_offset];
        var state = &lane_states[lane];
        state.kind = kind;
        state.drive = 'A' + lane;
        state.active_depth +|= 1;
        state.progress_phase = .none;
        state.progress = 0;
        state.progress_total = 0;
        state.progress_sequence +%= 1;
    }

    return .{
        .kind = kind,
        .drive = drive_letter,
        .second_drive = second_drive,
        .lanes = plan.lanes,
        .lane_count = plan.count,
        .gates_locked = runtime_owned,
        .start_tick = timer.tickCount(),
        .active = true,
    };
}

pub fn finish(guard: *Guard, ok: bool) void {
    if (!guard.active) return;

    const now = timer.tickCount();
    const elapsed = if (now >= guard.start_tick) now - guard.start_tick else 0;
    stats.completed +%= 1;
    if (!ok) stats.failed +%= 1;
    stats.last_ticks = elapsed;
    stats.total_ticks +%= elapsed;
    if (elapsed > stats.max_ticks) stats.max_ticks = elapsed;
    stats.last_kind = kindCode(guard.kind);
    stats.last_drive = request_scope.driveCode(guard.drive);
    if (stats.active_requests != 0) stats.active_requests -= 1;
    stats.active_progress_sequence +%= 1;

    var lane_offset = guard.lane_count;
    while (lane_offset != 0) {
        lane_offset -= 1;
        const lane = guard.lanes[lane_offset];
        var state = &lane_states[lane];
        if (state.active_depth != 0) state.active_depth -= 1;
        if (state.active_depth == 0) {
            state.progress_phase = .none;
            state.progress = 0;
            state.progress_total = 0;
            state.progress_sequence +%= 1;
        }
        if (guard.gates_locked) _ = request_gates[lane].leave();
    }
    guard.active = false;
}

pub fn kindCode(kind: Kind) u32 {
    return @intFromEnum(kind);
}

// A checked system update can legitimately hold its volume gate while it
// fingerprints large target/stage files. The holder reports bounded progress
// so waiters distinguish a slow, advancing transaction from a wedged one.
pub fn reportAtomicProgress(
    phase: AtomicProgressPhase,
    completed: u64,
    total: u64,
) void {
    const task_id = scheduler.currentId() orelse return;
    var lane_index: usize = 0;
    while (lane_index < request_gates.len) : (lane_index += 1) {
        if (request_gates[lane_index].owner != task_id) continue;
        var state = &lane_states[lane_index];
        if (state.active_depth == 0 or state.kind != .file_update_atomic_checked) continue;
        state.progress_phase = phase;
        state.progress = completed;
        state.progress_total = total;
        state.progress_sequence +%= 1;
        stats.active_progress_sequence +%= 1;
    }
}

// Bounded gate acquisition (0.60.20): the former single fs-request gate used
// to park every waiter with WAIT_FOREVER. A holder that never returned froze
// the whole system silently (Lenovo SSH-exec freeze). Volume lanes now retain
// the same bounded diagnosis for requests that actually share an owner. Waiters
// run in slices; every expired slice logs a loud [FSGATE] diagnosis with
// the current holder, and after the limit the operation fails visibly
// instead of hanging forever.
const GATE_SLICE_TICKS: u64 = 5 * @as(u64, timer.DEFAULT_HZ);
const GATE_SLICE_LIMIT: u32 = 12;

fn acquireGate(lane: u8) bool {
    if (scheduler.current() == null) return false;
    const gate = &request_gates[lane];
    if (gate.tryEnter()) return true;
    stats.lock_contention_waits +%= 1;
    var slices: u32 = 0;
    var progress_sequence = lane_states[lane].progress_sequence;
    while (true) {
        if (gate.enter(GATE_SLICE_TICKS)) {
            return true;
        }
        // A full 5-second wait is not a stall if the long checked update
        // crossed at least one explicit progress boundary in that interval.
        // Restart the consecutive-stall budget while retaining the gate.
        if (lane_states[lane].progress_sequence != progress_sequence) {
            progress_sequence = lane_states[lane].progress_sequence;
            slices = 0;
            continue;
        }
        slices += 1;
        const report = slices == 1 or (slices & (slices - 1)) == 0 or slices == GATE_SLICE_LIMIT;
        if (report) {
            const incident_token = diag_screen.beginResolvableIncident();
            // Framebuffer-direct: visible even when the desktop owns the
            // screen. Keep no generation across the next interruptible gate
            // wait: a hard-killed waiter cannot run Zig defers.
            diag_screen.write("[FSGATE] stall slice=");
            diag_screen.writeDec(slices);
            diag_screen.write(" holder_task=");
            diag_screen.writeDec(gate.owner);
            diag_screen.write(" depth=");
            diag_screen.writeDec(gate.depth);
            diag_screen.write(" active_kind=");
            diag_screen.writeDec(kindCode(lane_states[lane].kind));
            diag_screen.write(" active_drive=");
            diag_screen.writeDec(request_scope.driveCode(lane_states[lane].drive));
            if (lane_states[lane].progress_phase != .none) {
                diag_screen.write(" phase=");
                diag_screen.writeDec(@intFromEnum(lane_states[lane].progress_phase));
                diag_screen.write(" progress=");
                diag_screen.writeDec(lane_states[lane].progress);
                diag_screen.write("/");
                diag_screen.writeDec(lane_states[lane].progress_total);
            }
            diag_screen.endLine();
            _ = diag_screen.resolveIncident(incident_token);
            k.puts("[FSGATE] stall slice=");
            k.putDec(slices);
            k.puts(" holder_task=");
            k.putDec(gate.owner);
            k.puts(" active_kind=");
            k.putDec(kindCode(lane_states[lane].kind));
            k.puts(" active_drive=");
            k.putDec(request_scope.driveCode(lane_states[lane].drive));
            if (lane_states[lane].progress_phase != .none) {
                k.puts(" phase=");
                k.putDec(@intFromEnum(lane_states[lane].progress_phase));
                k.puts(" progress=");
                k.putDec(lane_states[lane].progress);
                k.puts("/");
                k.putDec(lane_states[lane].progress_total);
            }
            k.puts("\r\n");
        }
        if (slices >= GATE_SLICE_LIMIT) return false;
    }
}

fn releasePlan(plan: request_scope.Plan, acquired_count: u8) void {
    var lane_offset = acquired_count;
    while (lane_offset != 0) {
        lane_offset -= 1;
        _ = request_gates[plan.lanes[lane_offset]].leave();
    }
}
