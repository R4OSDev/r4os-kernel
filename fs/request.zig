const diag_screen = @import("../kernel/diag_screen.zig");
const k = @import("../kernel/log.zig");
const scheduler = @import("../sched/scheduler.zig");
const sync = @import("../sched/sync.zig");
const timer = @import("../kernel/timer.zig");

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
};

pub const Guard = struct {
    kind: Kind = .file_info,
    drive: u8 = 0,
    locked: bool = false,
    start_tick: u64 = 0,
    active: bool = false,
};

// This transaction owner intentionally spans FAT/block-I/O waits. It blocks
// hard task termination but stays separate from sleep-under-lock diagnostics.
var request_gate = sync.UnwindGuard.init("fs-request");
var stats: Summary = .{};

pub fn init() void {
    request_gate = sync.UnwindGuard.init("fs-request");
    stats = .{};
}

pub fn summary() Summary {
    return stats;
}

pub fn begin(kind: Kind, drive_letter: u8) ?Guard {
    var locked = false;
    if (scheduler.currentId() != null) {
        locked = acquireGate();
        if (!locked) {
            stats.lock_timeouts +%= 1;
            return null;
        }
        stats.lock_acquires +%= 1;
    } else {
        stats.boot_bypass +%= 1;
    }

    const kind_code = kindCode(kind);
    const drive_code: u32 = if (drive_letter >= 'a' and drive_letter <= 'z')
        @as(u32, drive_letter - 32)
    else
        @as(u32, drive_letter);
    stats.requests +%= 1;
    switch (kind) {
        .file_read, .file_read_at, .loader_read, .config_read => stats.read_requests +%= 1,
        .file_write, .file_write_at, .file_append, .file_delete, .file_rename, .file_copy, .file_move, .file_replace_atomic, .file_delete_if_match, .file_update_atomic_checked, .config_write => stats.write_requests +%= 1,
        .stream_begin, .stream_write, .stream_finish, .stream_abort => stats.stream_requests +%= 1,
        else => stats.metadata_requests +%= 1,
    }
    stats.active_kind = kind_code;
    stats.active_drive = drive_code;
    stats.active_phase = 0;
    stats.active_progress = 0;
    stats.active_progress_total = 0;
    stats.active_progress_sequence +%= 1;

    return .{
        .kind = kind,
        .drive = drive_letter,
        .locked = locked,
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
    stats.last_drive = if (guard.drive >= 'a' and guard.drive <= 'z')
        @as(u32, guard.drive - 32)
    else
        @as(u32, guard.drive);
    stats.active_kind = 0;
    stats.active_drive = 0;
    stats.active_phase = 0;
    stats.active_progress = 0;
    stats.active_progress_total = 0;
    stats.active_progress_sequence +%= 1;

    if (guard.locked) _ = releaseGate();
    guard.active = false;
}

pub fn kindCode(kind: Kind) u32 {
    return @intFromEnum(kind);
}

// A checked system update can legitimately hold the namespace gate while it
// fingerprints large target/stage files. The holder reports bounded progress
// so waiters distinguish a slow, advancing transaction from a wedged one.
pub fn reportAtomicProgress(
    phase: AtomicProgressPhase,
    completed: u64,
    total: u64,
) void {
    if (stats.active_kind != kindCode(.file_update_atomic_checked)) return;
    const task_id = scheduler.currentId() orelse return;
    if (request_gate.owner != task_id) return;
    stats.active_phase = @intFromEnum(phase);
    stats.active_progress = completed;
    stats.active_progress_total = total;
    stats.active_progress_sequence +%= 1;
}

// Bounded gate acquisition (0.60.20): the single fs-request gate used to
// park every waiter with WAIT_FOREVER -- one holder that never returns
// froze the whole system silently (Lenovo SSH-exec freeze).  Now waiters
// run in slices; every expired slice logs a loud [FSGATE] diagnosis with
// the current holder, and after the limit the operation fails visibly
// instead of hanging forever.
const GATE_SLICE_TICKS: u64 = 5 * @as(u64, timer.DEFAULT_HZ);
const GATE_SLICE_LIMIT: u32 = 12;

fn acquireGate() bool {
    if (scheduler.current() == null) return false;
    if (request_gate.tryEnter()) return true;
    stats.lock_contention_waits +%= 1;
    var slices: u32 = 0;
    var progress_sequence = stats.active_progress_sequence;
    while (true) {
        if (request_gate.enter(GATE_SLICE_TICKS)) {
            return true;
        }
        // A full 5-second wait is not a stall if the long checked update
        // crossed at least one explicit progress boundary in that interval.
        // Restart the consecutive-stall budget while retaining the gate.
        if (stats.active_progress_sequence != progress_sequence) {
            progress_sequence = stats.active_progress_sequence;
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
            diag_screen.writeDec(request_gate.owner);
            diag_screen.write(" depth=");
            diag_screen.writeDec(request_gate.depth);
            diag_screen.write(" active_kind=");
            diag_screen.writeDec(stats.active_kind);
            diag_screen.write(" active_drive=");
            diag_screen.writeDec(stats.active_drive);
            if (stats.active_phase != 0) {
                diag_screen.write(" phase=");
                diag_screen.writeDec(stats.active_phase);
                diag_screen.write(" progress=");
                diag_screen.writeDec(stats.active_progress);
                diag_screen.write("/");
                diag_screen.writeDec(stats.active_progress_total);
            }
            diag_screen.endLine();
            _ = diag_screen.resolveIncident(incident_token);
            k.puts("[FSGATE] stall slice=");
            k.putDec(slices);
            k.puts(" holder_task=");
            k.putDec(request_gate.owner);
            k.puts(" active_kind=");
            k.putDec(stats.active_kind);
            if (stats.active_phase != 0) {
                k.puts(" phase=");
                k.putDec(stats.active_phase);
                k.puts(" progress=");
                k.putDec(stats.active_progress);
                k.puts("/");
                k.putDec(stats.active_progress_total);
            }
            k.puts("\r\n");
        }
        if (slices >= GATE_SLICE_LIMIT) return false;
    }
}

fn releaseGate() bool {
    return request_gate.leave();
}
