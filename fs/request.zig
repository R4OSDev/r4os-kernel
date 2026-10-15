// Stateless C facade. Rust owns request admission, lanes and all counters.
const diag_screen = @import("../kernel/diag_screen.zig");
const k = @import("../kernel/log.zig");
const request_scope = @import("request_scope.zig");
const access = @import("../storage/access_runtime.zig");
const task_context = @import("../sched/task_context.zig");
const vfs = @import("vfs.zig");
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

pub const Summary = extern struct {
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


const empty_owner: access.Owner = .{ .task = 0, .task_generation = 0 };
pub const Use = extern struct {
    id: u64 = 0,
    owner: access.Owner = empty_owner,
    present: bool = false,
};
pub const Guard = extern struct {
    kind: Kind = .file_info,
    drive: u8 = 0,
    second_drive: u8 = 0,
    lanes: [drive_gate_count]u8 = .{0} ** drive_gate_count,
    lane_count: u8 = 0,
    gates_locked: bool = false,
    mutations_locked: bool = false,
    lanes_active: bool = false,
    start_tick: u64 = 0,
    active: bool = false,
    uses: [drive_gate_count]Use = .{Use{}} ** drive_gate_count,
    unwind: task_context.UnwindToken = .{},
};
const BoundMount = extern struct {
    reference: access.MountRef,
    present: bool,
};
pub const GateSnapshot = extern struct {
    owner_task_id: u32 = 0,
    owner_task_generation: u64 = 0,
    kind: Kind = .file_info,
    drive: u8 = 0,
    depth: u32 = 0,
    active_depth: u32 = 0,
};


const Stall = extern struct {
    slices: u32, owner: u32, depth: u32, kind: u32, drive: u32,
    phase: u32, progress: u64, total: u64,
};
extern fn r4os_rust_fs_request_init() void;
extern fn r4os_rust_fs_request_summary(*Summary) void;
extern fn r4os_rust_fs_request_gate(u8, *GateSnapshot) bool;
extern fn r4os_rust_fs_request_begin(u32, u8, u8, u8, bool, ?[*]const BoundMount, usize, bool, *Guard) bool;
extern fn r4os_rust_fs_request_finish(*Guard, bool) void;
extern fn r4os_rust_fs_request_readers(*Guard) bool;
extern fn r4os_rust_fs_request_copy_progress(*const Guard) void;
extern fn r4os_rust_fs_request_atomic_progress(u32, u64, u64) void;
pub fn init() void { r4os_rust_fs_request_init(); }
pub fn summary() Summary {
    var out: Summary = undefined;
    r4os_rust_fs_request_summary(&out);
    return out;
}
pub fn gateSnapshot(drive_letter: u8) ?GateSnapshot {
    var out: GateSnapshot = undefined;
    return if (r4os_rust_fs_request_gate(drive_letter, &out)) out else null;
}
fn start(kind: Kind, first: u8, second: u8, selection: u8, immediate: bool, bound: ?[]const BoundMount) ?Guard {
    var out: Guard = undefined;
    return if (r4os_rust_fs_request_begin(@intFromEnum(kind), first, second, selection, immediate,
        if (bound) |refs| refs.ptr else null, if (bound) |refs| refs.len else 0, bound != null, &out)) out else null;
}
fn binding(volume: vfs.Volume) BoundMount {
    const ref = volume.accessReference();
    return .{ .reference = ref orelse .{ .slot = 0, .generation = 0 }, .present = ref != null };
}
pub fn begin(kind: Kind, drive_letter: u8) ?Guard { return start(kind, drive_letter, 0, 0, false, null); }
pub fn tryBegin(kind: Kind, drive_letter: u8) ?Guard { return start(kind, drive_letter, 0, 0, true, null); }
pub fn beginPair(kind: Kind, first_drive: u8, second_drive: u8) ?Guard { return start(kind, first_drive, second_drive, 1, false, null); }
pub fn beginVolume(kind: Kind, letter: u8, volume: vfs.Volume) ?Guard {
    return start(kind, letter, 0, 0, false, &.{binding(volume)});
}
pub fn tryBeginVolume(kind: Kind, letter: u8, volume: vfs.Volume) ?Guard {
    return start(kind, letter, 0, 0, true, &.{binding(volume)});
}
pub fn beginPairVolumes(kind: Kind, first: u8, second: u8, a: vfs.Volume, b: vfs.Volume) ?Guard {
    return start(kind, first, second, 1, false, &.{ binding(a), binding(b) });
}
pub fn finish(guard: *Guard, ok: bool) void { r4os_rust_fs_request_finish(guard, ok); }
pub fn allowCopyReaders(guard: *Guard) bool { return r4os_rust_fs_request_readers(guard); }
pub fn copyProgress(guard: *const Guard) void { r4os_rust_fs_request_copy_progress(guard); }
pub fn kindCode(kind: Kind) u32 { return @intFromEnum(kind); }
pub fn reportAtomicProgress(phase: AtomicProgressPhase, completed: u64, total: u64) void {
    r4os_rust_fs_request_atomic_progress(@intFromEnum(phase), completed, total);
}
// Until VFS migration: projection only, never acquire a request or change a
// mount here. Absence and missing reference remain distinct results.
export fn r4os_zig_fs_request_volume(letter: u8, out: *access.MountRef) u8 {
    const volume = vfs.volumeForDrive(letter) orelse return 0;
    out.* = volume.accessReference() orelse return 2;
    return 1;
}
// Existing display owners receive an immutable bounded native snapshot.
export fn r4os_zig_fs_request_stall(s: *const Stall) void {
    const incident = diag_screen.beginResolvableIncident();
    diag_screen.write("[FSGATE] stall slice="); diag_screen.writeDec(s.slices);
    diag_screen.write(" holder_task="); diag_screen.writeDec(s.owner);
    diag_screen.write(" depth="); diag_screen.writeDec(s.depth);
    diag_screen.write(" active_kind="); diag_screen.writeDec(s.kind);
    diag_screen.write(" active_drive="); diag_screen.writeDec(s.drive);
    if (s.phase != 0) {
        diag_screen.write(" phase="); diag_screen.writeDec(s.phase);
        diag_screen.write(" progress="); diag_screen.writeDec(s.progress);
        diag_screen.write("/"); diag_screen.writeDec(s.total);
    }
    diag_screen.endLine();
    _ = diag_screen.resolveIncident(incident);
    k.puts("[FSGATE] stall slice="); k.putDec(s.slices);
    k.puts(" holder_task="); k.putDec(s.owner);
    k.puts(" active_kind="); k.putDec(s.kind);
    k.puts(" active_drive="); k.putDec(s.drive);
    if (s.phase != 0) {
        k.puts(" phase="); k.putDec(s.phase);
        k.puts(" progress="); k.putDec(s.progress);
        k.puts("/"); k.putDec(s.total);
    }
    k.puts("\r\n");
}
comptime {
    if (@sizeOf(Summary) != 184 or @sizeOf(BoundMount) != 24 or @sizeOf(Use) != 48 or
        @sizeOf(Guard) != 1320 or @sizeOf(GateSnapshot) != 32 or @sizeOf(Stall) != 40)
        @compileError("Native FS request private C layout changed");
}
