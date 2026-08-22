const r4x_api = @import("r4x_api.zig");
const std = @import("std");
const drive = @import("../fs/drive.zig");
const vfs = @import("../fs/vfs.zig");
const page_cache = @import("../fs/page_cache.zig");
const fs_request = @import("../fs/request.zig");
const system_update_atomic = @import("../fs/system_update_atomic.zig");
const upload_claim_store = @import("../fs/upload_claim_store.zig");
const upc = @import("upload_publish_claim");
const bootlog = @import("../kernel/bootlog.zig");
const heap = @import("../memory/heap.zig");
const interrupts = @import("../arch/x86_64/interrupts.zig");
const k = @import("../kernel/log.zig");
const power = @import("../arch/x86_64/power.zig");
const reset = @import("../arch/x86_64/reset.zig");
const r4d = @import("r4d.zig");
const net = @import("../net/core.zig");
const registry = @import("r4_registry_core");
const scheduler = @import("../sched/scheduler.zig");
const sched_task = @import("../sched/task.zig");
const sync = @import("../sched/sync.zig");
const time_core = @import("../platform/time.zig");

pub const name = "R4SYS";
/// API path buffer follows the contract limit (0.60.19: 1023 bytes + NUL,
/// the UTF-8 worst case of the 260-character Windows-parity path limit).
pub const max_api_path: usize = @as(usize, r4x_api.file_path_max_bytes) + 1;
pub const dir_entry_result_end: i32 = -5;
pub const dir_entry_error_io: i32 = -9;

pub const Target = struct {
    drive_ref: *drive.Drive,
    path: []const u8,
};

pub const FsFailureDiagnostic = struct {
    sequence: u32 = 0,
    task_id: u32 = 0,
    kind: fs_request.Kind = .file_info,
    result: i32 = 0,
    lookup_stage: u32 = 0,
};

var fs_failure_diagnostic: FsFailureDiagnostic = .{};

pub fn fsFailureDiagnostic() FsFailureDiagnostic {
    return fs_failure_diagnostic;
}

fn recordFsFailure(kind: fs_request.Kind, result: i32, lookup_stage: u32) void {
    var sequence = fs_failure_diagnostic.sequence +% 1;
    if (sequence == 0) sequence = 1;
    fs_failure_diagnostic = .{
        .sequence = sequence,
        .task_id = scheduler.currentId() orelse 0,
        .kind = kind,
        .result = result,
        .lookup_stage = lookup_stage,
    };
}

pub const ResolveTargetFn = *const fn ([]const u8, *[max_api_path]u8) ?Target;

pub const StreamOwner = struct {
    kind: Kind,
    // Logical caller identity. For R4X this is the exact ProgramThread task
    // whose async worker performs the operation, not the shared process.
    id: u32,
    generation: u64,
    // Process identity is kept separately so process retirement can release
    // every logical caller slot only after all ProgramThreads and async
    // workers have drained.
    program_id: u32 = 0,
    program_generation: u64 = 0,

    pub const Kind = enum(u8) {
        program,
        kernel_task,
    };
};

pub const ResolveStreamOwnerFn = *const fn () ?StreamOwner;
pub const ProgramModuleRunningFn = *const fn (u8, []const u8) i32;

pub const DriveInfo = r4x_api.DriveInfo;

pub const FileInfo = r4x_api.FileInfo;

pub const file_stream_result_ok = r4x_api.file_stream_result_ok;
pub const file_stream_error_invalid = r4x_api.file_stream_error_invalid;
pub const file_stream_error_unsupported = r4x_api.file_stream_error_unsupported;
pub const file_stream_error_not_found = r4x_api.file_stream_error_not_found;
pub const file_stream_error_exists = r4x_api.file_stream_error_exists;
pub const file_stream_error_io = r4x_api.file_stream_error_io;
pub const file_stream_error_offset_mismatch = r4x_api.file_stream_error_offset_mismatch;
pub const file_stream_error_size_mismatch = r4x_api.file_stream_error_size_mismatch;
pub const file_stream_error_too_large = r4x_api.file_stream_error_too_large;

pub const file_stream_open_create = r4x_api.file_stream_open_create;
pub const file_stream_open_truncate = r4x_api.file_stream_open_truncate;
pub const file_stream_open_replace = r4x_api.file_stream_open_replace;
// An exclusive in-memory lease backed by a private file.  The active
// StreamSlot is the owner token; a later lease opener may reclaim the file
// only when no live original R4X caller owns its slot (including after a
// reboot). The async-I/O worker itself is deliberately not the owner.
pub const file_stream_open_lease = r4x_api.file_stream_open_lease;
pub const file_stream_finish_keep_ownership: u32 = 1 << 0;
const file_stream_finish_supported_flags: u32 = file_stream_finish_keep_ownership;
const file_stream_open_supported_flags: u32 =
    file_stream_open_create |
    file_stream_open_truncate |
    file_stream_open_lease;

pub const file_replace_atomic_result_ok: i32 = 0;
pub const file_replace_atomic_error_invalid: i32 = -1;
pub const file_replace_atomic_error_unsupported: i32 = -2;
pub const file_replace_atomic_error_not_found: i32 = -3;
pub const file_replace_atomic_error_bad_path: i32 = -4;
pub const file_replace_atomic_error_alias: i32 = -5;
pub const file_replace_atomic_error_conflict: i32 = -6;
pub const file_replace_atomic_error_io: i32 = -7;
pub const file_replace_atomic_error_not_atomic: i32 = -8;
pub const file_replace_atomic_flag_consume_stage: u32 = 1 << 0;
pub const file_replace_atomic_flag_require_target_absent: u32 = 1 << 1;
pub const file_replace_atomic_flag_require_owned_stage: u32 = 1 << 2;
const file_replace_atomic_supported_flags =
    file_replace_atomic_flag_consume_stage |
    file_replace_atomic_flag_require_target_absent |
    file_replace_atomic_flag_require_owned_stage;
pub const file_delete_if_match_result_not_found: i32 = 0;
pub const file_delete_if_match_result_deleted: i32 = 1;
pub const file_delete_if_match_error_invalid: i32 = -1;
pub const file_delete_if_match_error_unsupported: i32 = -2;
pub const file_delete_if_match_error_conflict: i32 = -3;
pub const file_delete_if_match_error_io: i32 = -4;
pub const file_update_atomic_checked_result_ok: i32 = 0;
pub const file_update_atomic_checked_error_invalid: i32 = -1;
pub const file_update_atomic_checked_error_unsupported: i32 = -2;
pub const file_update_atomic_checked_error_bad_path: i32 = -3;
pub const file_update_atomic_checked_error_conflict: i32 = -4;
pub const file_update_atomic_checked_error_io: i32 = -5;
pub const file_update_atomic_checked_error_not_atomic: i32 = -6;
pub const file_update_atomic_checked_flag_forward: u32 = 1 << 0;
pub const file_update_atomic_checked_flag_rollback: u32 = 1 << 1;
pub const file_update_atomic_checked_flag_target_existed: u32 = 1 << 2;
pub const file_update_atomic_checked_flag_old_known: u32 = 1 << 3;
const file_update_atomic_checked_supported_flags: u32 =
    file_update_atomic_checked_flag_forward |
    file_update_atomic_checked_flag_rollback |
    file_update_atomic_checked_flag_target_existed |
    file_update_atomic_checked_flag_old_known;
/// Maps the public protocol code onto the claim's protocol tag.  An unknown
/// code is refused rather than silently attributed to SFTP.
fn publishProtocolFromCode(raw: u32) ?upc.Protocol {
    return switch (raw) {
        r4x_api.file_stream_publish_protocol_sftp => .sftp,
        r4x_api.file_stream_publish_protocol_scp => .scp,
        r4x_api.file_stream_publish_protocol_ftp => .ftp,
        else => null,
    };
}

// Declared create-only publish intent (0.60.30).
pub const file_stream_declare_publish_result_ok: i32 = 0;
pub const file_stream_declare_publish_error_invalid: i32 = -1;
pub const file_stream_declare_publish_error_unsupported: i32 = -2;
pub const file_stream_declare_publish_error_bad_path: i32 = -3;
pub const file_stream_declare_publish_error_not_found: i32 = -4;
pub const file_stream_declare_publish_error_not_atomic: i32 = -5;
pub const file_stream_declare_publish_error_io: i32 = -6;

// Backend-exact name collation (0.60.24).
pub const path_names_equal_result_different: i32 = 0;
pub const path_names_equal_result_equal: i32 = 1;
pub const path_names_equal_error_invalid: i32 = -1;
pub const path_names_equal_error_unsupported: i32 = -2;
pub const path_names_equal_error_bad_path: i32 = -3;
pub const path_names_equal_error_not_same_volume: i32 = -4;

// Per-payload checked cleanup (0.60.23).
pub const file_update_cleanup_checked_result_ok: i32 = 0;
pub const file_update_cleanup_checked_error_invalid: i32 = -1;
pub const file_update_cleanup_checked_error_unsupported: i32 = -2;
pub const file_update_cleanup_checked_error_bad_path: i32 = -3;
pub const file_update_cleanup_checked_error_conflict: i32 = -4;
pub const file_update_cleanup_checked_error_io: i32 = -5;
pub const file_update_cleanup_checked_error_not_atomic: i32 = -6;
pub const file_update_cleanup_checked_flag_target_existed: u32 = 1 << 0;
pub const file_update_cleanup_checked_flag_old_known: u32 = 1 << 1;
pub const file_update_cleanup_checked_flag_previous_known: u32 = 1 << 2;
const file_update_cleanup_checked_supported_flags: u32 =
    file_update_cleanup_checked_flag_target_existed |
    file_update_cleanup_checked_flag_old_known |
    file_update_cleanup_checked_flag_previous_known;
pub const program_module_running_result_idle: i32 = 0;
pub const program_module_running_result_running: i32 = 1;
pub const program_module_running_error_invalid: i32 = -1;
pub const program_module_running_error_unavailable: i32 = -2;

const max_stream_slots: usize = 16;
const file_delete_if_match_checksum_seed: u32 = 2166136261;
var file_delete_if_match_buffer: [32768]u8 = undefined;
// A legal public path contains at most this many Unicode characters and can
// therefore never contain more directory components.  Keeping the complete
// ancestor chain bounded here lets directory mutations fail closed instead of
// falling back to an unbounded allocation while the global filesystem gate is
// held.
const max_stream_ancestors: usize = @as(usize, r4x_api.file_path_max_chars);

const StreamSlot = struct {
    active: bool = false,
    // Published before the mutating Begin so an ambiguous create can still
    // be aborted. Write/Finish accept only a fully ready slot.
    ready: bool = false,
    finished: bool = false,
    publish_started: bool = false,
    // A create-only publication may return after its visibility point but
    // before the final durability acknowledgement. Keep the exact target
    // tuple with the stream lease so a later CLOSE/Abort can resume only the
    // same transfer instead of interpreting names afresh.
    publish_target_name: [vfs.NAME_MAX]u8 = .{0} ** vfs.NAME_MAX,
    publish_target_name_len: usize = 0,
    publish_backup_name: [12]u8 = .{0} ** 12,
    publish_backup_name_len: usize = 0,
    // Generation of the DURABLE claim backing this publication (0.60.22).
    // The in-memory tuple above is only the fast path; this is what survives
    // a reset and lets pre-runtime recovery finish or reverse the hand-over.
    publish_claim_generation: u64 = 0,
    // Set once the publish intent was declared at stream-begin (0.60.30), so
    // the claim brackets the WHOLE transfer instead of only the hand-over.
    // The declared names are kept because an identity refresh (empty FAT
    // files gain their cluster only on the first append) has to rewrite the
    // very same claim.
    publish_declared: bool = false,
    publish_declared_parent: [max_api_path]u8 = .{0} ** max_api_path,
    publish_declared_parent_len: usize = 0,
    publish_protocol: u8 = 0,
    // Resolver-canonical, volume-relative path used for ancestor conflicts.
    raw_path: [max_api_path]u8 = .{0} ** max_api_path,
    raw_len: usize = 0,
    drive_letter: u8 = 0,
    // Slot volume identity: parent_node belongs to the internal boot
    // volume, not to the lettered drive (see streamSlotVolume).
    on_boot_volume: bool = false,
    parent_node: vfs.NodeRef = 0,
    // Stable, root-excluding identities of every directory from the volume
    // root through parent_node. NTFS supplies record+sequence; FAT supplies
    // the directory's non-zero first cluster with generation zero.
    ancestor_nodes: [max_stream_ancestors]vfs.NodeRef = .{0} ** max_stream_ancestors,
    ancestor_generations: [max_stream_ancestors]u16 = .{0} ** max_stream_ancestors,
    ancestor_count: usize = 0,
    file_node: vfs.NodeRef = 0,
    file_node_generation: u16 = 0,
    has_file_node: bool = false,
    // FAT allocates the first cluster on the first non-empty append. If the
    // append succeeds but the immediate identity refresh is unreadable, one
    // later same-owner lookup may rebind the confirmed new size.
    identity_refresh_pending: bool = false,
    file_attr: u8 = 0,
    file_reparse: bool = false,
    file_created_time: u16 = 0,
    file_created_date: u16 = 0,
    file_access_date: u16 = 0,
    file_modified_time: u16 = 0,
    file_modified_date: u16 = 0,
    name: [max_api_path]u8 = .{0} ** max_api_path,
    name_len: usize = 0,
    size: u64 = 0,
    flags: u32 = 0,
    generation: u32 = 0,
    owner_kind: StreamOwner.Kind = .kernel_task,
    owner_id: u32 = 0,
    owner_generation: u64 = 0,
    owner_program_id: u32 = 0,
    owner_program_generation: u64 = 0,
};

const OwnedCreateOnlyState = enum(u8) {
    stage_only,
    aliased,
    target_only,
    missing,
    conflict,
    io,
};

var stream_slots: [max_stream_slots]StreamSlot = .{StreamSlot{}} ** max_stream_slots;
var stream_generation: u32 = 1;

pub const boot_log_flag_wrapped = r4x_api.boot_log_flag_wrapped;

pub const BootLogInfo = r4x_api.BootLogInfo;

pub const registry_api_result_ok = r4x_api.registry_api_result_ok;
pub const registry_api_result_invalid = r4x_api.registry_api_result_invalid;
pub const registry_api_result_bad_path = r4x_api.registry_api_result_bad_path;
pub const registry_api_result_hive_not_found = r4x_api.registry_api_result_hive_not_found;
pub const registry_api_result_key_not_found = r4x_api.registry_api_result_key_not_found;
pub const registry_api_result_value_not_found = r4x_api.registry_api_result_value_not_found;
pub const registry_api_result_buffer_too_small = r4x_api.registry_api_result_buffer_too_small;
pub const registry_api_result_hive_corrupt = r4x_api.registry_api_result_hive_corrupt;
pub const registry_api_result_io = r4x_api.registry_api_result_io;
pub const registry_api_result_unsupported = r4x_api.registry_api_result_unsupported;

pub const registry_name_max = r4x_api.registry_name_max;
pub const registry_max_path: usize = 256;
pub const registry_max_hive_bytes: usize = 32 * 1024;
const registry_path_pool_max: usize = 16 * 1024;
const registry_build_value_max: usize = 512;
const registry_build_key_max: usize = 256;
const registry_key_depth_max: usize = 32;

const empty_registry_build_value: registry.BuildValue = .{ .key_path = "", .name = "", .value_type = .string, .data = "" };
const empty_registry_build_key: registry.BuildKey = .{};
var registry_write_values: [registry_build_value_max]registry.BuildValue = [_]registry.BuildValue{empty_registry_build_value} ** registry_build_value_max;
var registry_write_keys: [registry_build_key_max]registry.BuildKey = [_]registry.BuildKey{empty_registry_build_key} ** registry_build_key_max;
var registry_value_key_indices: [registry_build_value_max]u32 = [_]u32{registry.invalid_index} ** registry_build_value_max;
var registry_flat_key_order: [registry_build_key_max]u32 = [_]u32{registry.invalid_index} ** registry_build_key_max;
const registry_recent_documents_root = "SYSTEM\\Shell\\RecentDocuments";

const RegistryWritebackCache = struct {
    valid: bool = false,
    dirty: bool = false,
    kind: registry.HiveKind = .system,
    len: usize = 0,
    bytes: [registry_max_hive_bytes]u8 = .{0} ** registry_max_hive_bytes,
    verify: [registry_max_hive_bytes]u8 = .{0} ** registry_max_hive_bytes,
};

var registry_writeback_cache: RegistryWritebackCache = .{};

pub const RegistryKeyInfo = r4x_api.RegistryKeyInfo;

pub const RegistryValueInfo = r4x_api.RegistryValueInfo;
pub const MonotonicClockInfo = r4x_api.MonotonicClockInfo;

var path_resolver: ?ResolveTargetFn = null;
var stream_owner_resolver: ?ResolveStreamOwnerFn = null;
var program_module_running_provider: ?ProgramModuleRunningFn = null;

pub fn setPathResolver(resolver: ResolveTargetFn) void {
    path_resolver = resolver;
}

pub fn setStreamOwnerResolver(resolver: ResolveStreamOwnerFn) void {
    stream_owner_resolver = resolver;
}

pub fn setProgramModuleRunningProvider(provider: ProgramModuleRunningFn) void {
    program_module_running_provider = provider;
}

pub fn setRuntimeContext(usable_bytes: u64) void {
    _ = usable_bytes;
}

pub fn taskYield() callconv(.c) void {
    scheduler.yield();
}

pub fn sleepTicks(ticks_value: u64) callconv(.c) void {
    var wait = sync.TimerWait.init();
    _ = wait.wait(ticks_value);
}

pub fn ticks() callconv(.c) u64 {
    return time_core.monotonicTicks();
}

pub fn timeSecondsSinceMidnight() callconv(.c) u32 {
    return time_core.secondsSinceMidnight();
}

pub fn timeState(out: *time_core.State) callconv(.c) void {
    out.* = time_core.state();
}

pub fn monotonicClock(out: *MonotonicClockInfo) callconv(.c) i32 {
    comptime {
        if (time_core.monotonic_flag_valid != r4x_api.monotonic_clock_flag_valid or
            time_core.monotonic_flag_continuous != r4x_api.monotonic_clock_flag_continuous or
            time_core.monotonic_flag_high_resolution != r4x_api.monotonic_clock_flag_high_resolution or
            time_core.monotonic_flag_irq_independent != r4x_api.monotonic_clock_flag_irq_independent or
            time_core.monotonic_flag_invariant != r4x_api.monotonic_clock_flag_invariant or
            time_core.monotonic_flag_early_origin != r4x_api.monotonic_clock_flag_early_origin or
            time_core.monotonic_flag_calibrated != r4x_api.monotonic_clock_flag_calibrated or
            time_core.monotonic_flag_degraded != r4x_api.monotonic_clock_flag_degraded)
        {
            @compileError("monotonic clock flag contract drift");
        }
    }
    const clock = time_core.monotonicSnapshot();
    out.* = .{
        .flags = clock.flags,
        .source = @intFromEnum(clock.source),
        .generation = clock.generation,
        .event_backend = switch (clock.event_backend) {
            .pit => 0,
            .hpet => 1,
            .lapic => 2,
        },
        .instant_ns = clock.instant_ns,
        .frequency_hz = r4x_api.monotonic_clock_frequency_hz,
        .resolution_ns = clock.resolution_ns,
        .source_frequency_hz = clock.source_frequency_hz,
        .event_frequency_numerator = clock.event.frequency_numerator,
        .event_frequency_denominator = clock.event.frequency_denominator,
        .event_requested_hz = clock.event.requested_hz,
        .event_effective_hz = clock.event.effective_hz,
    };
    return if (clock.valid) 1 else 0;
}

pub fn timeSetState(request: *const time_core.State) callconv(.c) i32 {
    return time_core.setState(request.*);
}

pub fn bootLogInfo(out: *BootLogInfo) callconv(.c) i32 {
    out.* = .{
        .capacity = @intCast(bootlog.capacity()),
        .length = @intCast(bootlog.length()),
        .flags = bootlog.flags(),
        .reserved = 0,
        .total_written = bootlog.totalWritten(),
        .dropped_bytes = bootlog.droppedBytes(),
    };
    return 1;
}

pub fn bootLogRead(offset: u32, out: [*]u8, capacity_value: u32) callconv(.c) i32 {
    if (capacity_value == 0) return 0;
    const len = bootlog.length();
    const offset_usize: usize = @intCast(offset);
    if (offset_usize >= len) return 0;
    const max_len = @min(@as(usize, @intCast(capacity_value)), len - offset_usize);
    const written = bootlog.read(offset_usize, out[0..max_len]);
    return @intCast(written);
}

pub fn systemHalt() callconv(.c) void {
    k.puts("System halted.\r\n");
    flushRegistryWritebackForShutdown();
    interrupts.haltForever();
}

pub fn systemReboot() callconv(.c) void {
    k.puts("System reboot.\r\n");
    flushRegistryWritebackForShutdown();
    flushPageCacheForShutdown();
    // ACPI warm reset does not guarantee that a PCI NIC stops DMA. Quiesce
    // runtime network drivers after durable writes and before the reset so the
    // next kernel never inherits a live descriptor ring from the old kernel.
    const callbacks_drained = net.beginSystemTransition("reboot");
    const drivers_stopped = callbacks_drained and r4d.shutdownNetworkForSystemTransition();
    if (!callbacks_drained or !drivers_stopped) {
        // Never cross a warm reset with an unproven callback/DMA handoff. A
        // real poweroff is the only safe fallback because it removes device
        // power instead of exposing retained hardware to the next kernel.
        k.puts("[RESET][ERROR] network handoff unsafe; powering off instead of warm reset\r\n");
        k.serialFlush();
        power.poweroff();
    }
    // 0.56.15: COM1-TX-Ring verlustfrei leeren, bevor die Maschine weg ist.
    k.serialFlush();
    reset.reboot();
}

pub fn systemPoweroff() callconv(.c) void {
    k.puts("System poweroff.\r\n");
    flushRegistryWritebackForShutdown();
    flushPageCacheForShutdown();
    // 0.56.15: COM1-TX-Ring verlustfrei leeren, bevor die Maschine weg ist.
    k.serialFlush();
    power.poweroff();
}

fn flushRegistryWritebackForShutdown() void {
    const result = flushRegistryWriteback();
    if (result != registry_api_result_ok) {
        k.puts("R4SYS shutdown: registry writeback flush failed\r\n");
    }
}

fn flushPageCacheForShutdown() void {
    if (!page_cache.flushAll()) {
        k.puts("R4SYS shutdown: page-cache flush failed\r\n");
    }
}

pub fn registryKeyInfo(path_ptr: [*:0]const u8, out: *RegistryKeyInfo) callconv(.c) i32 {
    out.* = .{};
    var path_buf: [registry_max_path]u8 = undefined;
    const key_path = copyZ(path_ptr, path_buf[0..]) orelse return registry_api_result_bad_path;
    const parsed = registry.parseRoot(key_path) orelse return registry_api_result_bad_path;
    if (!activeRegistryHive(parsed.kind)) return registry_api_result_unsupported;

    const mem = heap.alloc(registry_max_hive_bytes, 16) orelse return registry_api_result_io;
    defer _ = heap.free(mem);
    const loaded = loadRegistryHive(parsed.kind, mem);
    if (loaded.result != registry_api_result_ok) return loaded.result;
    const hive = loaded.view.?;
    const key_index = hive.findKey(key_path) orelse return registry_api_result_key_not_found;
    const key = hive.keyAt(key_index);

    out.child_count = key.child_count;
    out.value_count = key.value_count;
    out.name = .{0} ** registry_name_max;
    if (key_index != 0) copyFixedZ(out.name[0..], hive.keyName(key));
    return registry_api_result_ok;
}

pub fn registryEnumKey(path_ptr: [*:0]const u8, index: u32, out_ptr: [*]u8, capacity: u32) callconv(.c) i32 {
    var path_buf: [registry_max_path]u8 = undefined;
    const key_path = copyZ(path_ptr, path_buf[0..]) orelse return registry_api_result_bad_path;
    const parsed = registry.parseRoot(key_path) orelse return registry_api_result_bad_path;
    if (!activeRegistryHive(parsed.kind)) return registry_api_result_unsupported;

    const mem = heap.alloc(registry_max_hive_bytes, 16) orelse return registry_api_result_io;
    defer _ = heap.free(mem);
    const loaded = loadRegistryHive(parsed.kind, mem);
    if (loaded.result != registry_api_result_ok) return loaded.result;
    const hive = loaded.view.?;
    const key_index = hive.findKey(key_path) orelse return registry_api_result_key_not_found;
    const key = hive.keyAt(key_index);
    if (index >= key.child_count) return registry_api_result_key_not_found;

    const child = hive.keyAt(key.first_child_index + index);
    return copyRegistryNameOut(hive.keyName(child), out_ptr, capacity);
}

pub fn registryEnumValue(path_ptr: [*:0]const u8, index: u32, out: *RegistryValueInfo) callconv(.c) i32 {
    out.* = .{};
    var path_buf: [registry_max_path]u8 = undefined;
    const key_path = copyZ(path_ptr, path_buf[0..]) orelse return registry_api_result_bad_path;
    const parsed = registry.parseRoot(key_path) orelse return registry_api_result_bad_path;
    if (!activeRegistryHive(parsed.kind)) return registry_api_result_unsupported;

    const mem = heap.alloc(registry_max_hive_bytes, 16) orelse return registry_api_result_io;
    defer _ = heap.free(mem);
    const loaded = loadRegistryHive(parsed.kind, mem);
    if (loaded.result != registry_api_result_ok) return loaded.result;
    const hive = loaded.view.?;
    const key_index = hive.findKey(key_path) orelse return registry_api_result_key_not_found;
    const key = hive.keyAt(key_index);
    if (index >= key.value_count) return registry_api_result_value_not_found;

    const value = hive.valueAt(key.first_value_index + index);
    fillRegistryValueInfo(out, hive, value);
    return registry_api_result_ok;
}

pub fn registryGetValue(path_ptr: [*:0]const u8, name_ptr: [*:0]const u8, out_info: *RegistryValueInfo, out_ptr: [*]u8, capacity: u32) callconv(.c) i32 {
    out_info.* = .{};
    var path_buf: [registry_max_path]u8 = undefined;
    var name_buf: [registry_name_max]u8 = undefined;
    const key_path = copyZ(path_ptr, path_buf[0..]) orelse return registry_api_result_bad_path;
    const value_name = copyZ(name_ptr, name_buf[0..]) orelse return registry_api_result_invalid;
    const parsed = registry.parseRoot(key_path) orelse return registry_api_result_bad_path;
    if (!activeRegistryHive(parsed.kind)) return registry_api_result_unsupported;

    const mem = heap.alloc(registry_max_hive_bytes, 16) orelse return registry_api_result_io;
    defer _ = heap.free(mem);
    const loaded = loadRegistryHive(parsed.kind, mem);
    if (loaded.result != registry_api_result_ok) return loaded.result;
    const hive = loaded.view.?;
    const key_index = hive.findKey(key_path) orelse return registry_api_result_key_not_found;
    const value_index = hive.findValue(key_index, value_name) orelse return registry_api_result_value_not_found;
    const value = hive.valueAt(value_index);
    fillRegistryValueInfo(out_info, hive, value);

    const data = hive.valueData(value);
    const out_capacity: usize = @intCast(capacity);
    if (out_capacity < data.len) return registry_api_result_buffer_too_small;
    if (data.len != 0) @memcpy(out_ptr[0..data.len], data);
    return @intCast(data.len);
}

pub fn registrySetValue(path_ptr: [*:0]const u8, name_ptr: [*:0]const u8, value_type_raw: u16, data_ptr: [*]const u8, data_len: u32) callconv(.c) i32 {
    var path_buf: [registry_max_path]u8 = undefined;
    var name_buf: [registry_name_max]u8 = undefined;
    const key_path = copyZ(path_ptr, path_buf[0..]) orelse return registry_api_result_bad_path;
    const value_name = copyZ(name_ptr, name_buf[0..]) orelse return registry_api_result_invalid;
    const parsed = registry.parseRoot(key_path) orelse return registry_api_result_bad_path;
    if (!activeRegistryHive(parsed.kind)) return registry_api_result_unsupported;
    const value_type = registry.ValueType.fromInt(value_type_raw) orelse return registry_api_result_invalid;
    const data = data_ptr[0..@intCast(data_len)];
    return registryMutateValue(.set, parsed.kind, key_path, value_name, value_type, data);
}

pub fn registryDeleteValue(path_ptr: [*:0]const u8, name_ptr: [*:0]const u8) callconv(.c) i32 {
    var path_buf: [registry_max_path]u8 = undefined;
    var name_buf: [registry_name_max]u8 = undefined;
    const key_path = copyZ(path_ptr, path_buf[0..]) orelse return registry_api_result_bad_path;
    const value_name = copyZ(name_ptr, name_buf[0..]) orelse return registry_api_result_invalid;
    const parsed = registry.parseRoot(key_path) orelse return registry_api_result_bad_path;
    if (!activeRegistryHive(parsed.kind)) return registry_api_result_unsupported;
    return registryMutateValue(.delete, parsed.kind, key_path, value_name, .binary, "");
}

pub fn driveInfo(index: u32, out: *DriveInfo) callconv(.c) i32 {
    if (index >= 26) return -1;
    const letter: u8 = 'A' + @as(u8, @intCast(index));
    out.* = .{ .letter = letter };
    const d = drive.get(letter) orelse return 0;
    out.mounted = 1;
    out.kind = @intFromEnum(d.kind);
    out.role = @intFromEnum(d.role);
    out.bytes = d.bytes;
    copyFixedZ(out.name[0..], d.name);
    {
        if (vfs.volumeForDrive(d.letter)) |volume| {
            var req = fs_request.begin(.drive_info, d.letter) orelse return -2;
            var ok = false;
            defer fs_request.finish(&req, ok);
            const cluster_bytes = volume.clusterBytes();
            const total_clusters = volume.totalClusters();
            out.cluster_bytes = cluster_bytes;
            out.total_clusters = total_clusters;
            out.bytes = @as(u64, total_clusters) * cluster_bytes;
            if (vfs.freeClusterCount(volume)) |free_clusters| {
                out.free_clusters = free_clusters;
                out.free_bytes = @as(u64, free_clusters) * cluster_bytes;
            }
            ok = true;
        }
    }
    return 1;
}

// ---------------------------------------------------------------------------
// R4M0-Ressourcenbereich (0.61.13): on-demand-Lesen eingebetteter Ressourcen
// einer Moduldatei. Bewusst PFADBASIERT und fuer BELIEBIGE Moduldateien -
// der Hauptkonsument ist der Desktop, der das Icon einer fremden .R4X liest,
// ohne sie zu starten. Nichts davon laeuft ueber das Programmimage; die
// non-alloc-Section .rsrc bleibt laut Loader draussen.

pub const module_resource_type_icon: u32 = 1;
pub const module_resource_type_help: u32 = 2;
pub const module_resource_type_file: u32 = 3;

pub const module_resource_error_invalid: i32 = -1;
pub const module_resource_error_volume: i32 = -2;
pub const module_resource_error_not_found: i32 = -3;
pub const module_resource_error_bad_module: i32 = -4;
pub const module_resource_error_no_resources: i32 = -5;
pub const module_resource_error_no_entry: i32 = -6;
pub const module_resource_error_too_small: i32 = -7;
pub const module_resource_error_io: i32 = -8;

const rsrc_max_entries: u32 = 64;
const rsrc_max_name: usize = 63;

const LocatedResource = struct {
    file_off: u32,
    size: u32,
};

fn locateModuleResourceImpl(volume: vfs.Volume, entry: vfs.Entry, resource_type: u32, resource_index: u32, resource_name: ?[]const u8, out: *LocatedResource) i32 {
    var header: [64]u8 = undefined;
    if ((vfs.readFileRange(volume, entry, 0, header[0..]) orelse return module_resource_error_io) != header.len) return module_resource_error_bad_module;
    if (!(header[0] == 'R' and header[1] == '4' and header[2] == 'M' and header[3] == '0')) return module_resource_error_bad_module;
    if (readLe16(header[4..6]) != 1 or readLe16(header[6..8]) != 1 or readLe16(header[10..12]) != 64) return module_resource_error_bad_module;
    const section_off = readLe32(header[16..20]);
    const section_count = readLe32(header[20..24]);
    if (section_count == 0 or section_count > 16) return module_resource_error_bad_module;

    // .rsrc suchen: die eine non-alloc-Section.
    var rsrc_off: u32 = 0;
    var rsrc_size: u32 = 0;
    var found_rsrc = false;
    var index: u32 = 0;
    while (index < section_count) : (index += 1) {
        var record: [32]u8 = undefined;
        const off: usize = @as(usize, section_off) + @as(usize, index) * 32;
        if ((vfs.readFileRange(volume, entry, off, record[0..]) orelse return module_resource_error_io) != record.len) return module_resource_error_bad_module;
        const flags = readLe32(record[8..12]);
        if ((flags & 0x1) != 0) continue;
        if (flags != 0) return module_resource_error_bad_module;
        if (!(record[0] == '.' and record[1] == 'r' and record[2] == 's' and record[3] == 'r' and record[4] == 'c' and record[5] == 0)) return module_resource_error_bad_module;
        if (found_rsrc) return module_resource_error_bad_module;
        found_rsrc = true;
        rsrc_off = readLe32(record[12..16]);
        rsrc_size = readLe32(record[16..20]);
    }
    if (!found_rsrc or rsrc_size < 4) return module_resource_error_no_resources;
    if (@as(u64, rsrc_off) + rsrc_size > entry.size) return module_resource_error_bad_module;

    var count_buf: [4]u8 = undefined;
    if ((vfs.readFileRange(volume, entry, @intCast(rsrc_off), count_buf[0..]) orelse return module_resource_error_io) != 4) return module_resource_error_bad_module;
    const count = readLe32(count_buf[0..4]);
    if (count == 0 or count > rsrc_max_entries) return module_resource_error_bad_module;
    if (4 + @as(u64, count) * 16 > rsrc_size) return module_resource_error_bad_module;

    index = 0;
    while (index < count) : (index += 1) {
        var record: [16]u8 = undefined;
        const off: usize = @as(usize, rsrc_off) + 4 + @as(usize, index) * 16;
        if ((vfs.readFileRange(volume, entry, off, record[0..]) orelse return module_resource_error_io) != record.len) return module_resource_error_bad_module;
        const typ = readLe16(record[0..2]);
        const entry_index = readLe16(record[2..4]);
        const name_off = readLe32(record[4..8]);
        const data_off = readLe32(record[8..12]);
        const size = readLe32(record[12..16]);
        if (typ != resource_type) continue;
        const matches = switch (resource_type) {
            module_resource_type_icon => entry_index == resource_index,
            module_resource_type_help => true,
            module_resource_type_file => blk: {
                const want = resource_name orelse break :blk false;
                if (name_off == 0 or name_off >= rsrc_size) break :blk false;
                var name_buf: [rsrc_max_name + 1]u8 = undefined;
                const name_abs: usize = @as(usize, rsrc_off) + @as(usize, name_off);
                var name_want: usize = name_buf.len;
                const rest = rsrc_size - name_off;
                if (rest < name_want) name_want = @intCast(rest);
                const got = vfs.readFileRange(volume, entry, name_abs, name_buf[0..name_want]) orelse return module_resource_error_io;
                var stored_len: usize = 0;
                while (stored_len < got and name_buf[stored_len] != 0) stored_len += 1;
                if (stored_len == got) break :blk false;
                break :blk asciiEqlIgnoreCase(name_buf[0..stored_len], want);
            },
            else => false,
        };
        if (!matches) continue;
        if (size == 0 or @as(u64, data_off) + size > rsrc_size) return module_resource_error_bad_module;
        out.* = .{ .file_off = rsrc_off + data_off, .size = size };
        return 0;
    }
    return module_resource_error_no_entry;
}

fn readLe16(bytes: []const u8) u16 {
    return @as(u16, bytes[0]) | (@as(u16, bytes[1]) << 8);
}

fn readLe32(bytes: []const u8) u32 {
    return @as(u32, bytes[0]) | (@as(u32, bytes[1]) << 8) | (@as(u32, bytes[2]) << 16) | (@as(u32, bytes[3]) << 24);
}

const ResolvedModuleFile = struct {
    volume: vfs.Volume,
    entry: vfs.Entry,
    letter: u8,
};

fn resolveModuleFile(path_ptr: [*:0]const u8, status: *i32) ?ResolvedModuleFile {
    var path_buf: [max_api_path]u8 = undefined;
    const raw_path = copyZ(path_ptr, path_buf[0..]) orelse {
        status.* = module_resource_error_invalid;
        return null;
    };
    var resolved_buf: [max_api_path]u8 = undefined;
    const target = resolveTarget(raw_path, &resolved_buf) orelse {
        status.* = module_resource_error_invalid;
        return null;
    };
    const volume = targetVolume(target) orelse {
        status.* = module_resource_error_volume;
        return null;
    };
    var entry: vfs.Entry = undefined;
    switch (vfs.resolveEntryStatus(volume, target.path, &entry)) {
        .found => {},
        .not_found => {
            status.* = module_resource_error_not_found;
            return null;
        },
        .io => {
            status.* = module_resource_error_io;
            return null;
        },
    }
    if (entry.isDir()) {
        status.* = module_resource_error_not_found;
        return null;
    }
    return .{ .volume = volume, .entry = entry, .letter = target.drive_ref.letter };
}

fn moduleResourceNameSlice(name_ptr: ?[*:0]const u8, buf: []u8) ?[]const u8 {
    const ptr = name_ptr orelse return null;
    var len: usize = 0;
    while (ptr[len] != 0 and len < buf.len) : (len += 1) buf[len] = ptr[len];
    if (ptr[len] != 0) return buf[0..0];
    return buf[0..len];
}

/// Groesse einer eingebetteten Ressource, ohne den Payload zu lesen.
pub fn moduleResourceStat(path_ptr: [*:0]const u8, resource_type: u32, resource_index: u32, name_ptr: ?[*:0]const u8) callconv(.c) i32 {
    if (resource_type == module_resource_type_file and name_ptr == null) return module_resource_error_invalid;
    var status: i32 = 0;
    const file = resolveModuleFile(path_ptr, &status) orelse return status;
    var name_buf: [rsrc_max_name + 1]u8 = undefined;
    const resource_name = moduleResourceNameSlice(name_ptr, name_buf[0..]);
    if (resource_name != null and resource_name.?.len == 0) return module_resource_error_invalid;
    var req = fs_request.begin(.file_read, file.letter) orelse return module_resource_error_io;
    var ok = false;
    defer fs_request.finish(&req, ok);
    var located: LocatedResource = undefined;
    const rc = locateModuleResourceImpl(file.volume, file.entry, resource_type, resource_index, resource_name, &located);
    if (rc != 0) return rc;
    ok = true;
    if (located.size > std.math.maxInt(i32)) return module_resource_error_bad_module;
    return @intCast(located.size);
}

/// Liest eine eingebettete Ressource KOMPLETT in einen caller-owned Buffer.
/// Ein zu kleiner Buffer ist ein sichtbarer Fehler, keine stille Truncation.
pub fn moduleResourceRead(path_ptr: [*:0]const u8, resource_type: u32, resource_index: u32, name_ptr: ?[*:0]const u8, out_ptr: [*]u8, out_len: u32) callconv(.c) i32 {
    if (resource_type == module_resource_type_file and name_ptr == null) return module_resource_error_invalid;
    if (out_len == 0) return module_resource_error_invalid;
    var status: i32 = 0;
    const file = resolveModuleFile(path_ptr, &status) orelse return status;
    var name_buf: [rsrc_max_name + 1]u8 = undefined;
    const resource_name = moduleResourceNameSlice(name_ptr, name_buf[0..]);
    if (resource_name != null and resource_name.?.len == 0) return module_resource_error_invalid;
    var req = fs_request.begin(.file_read, file.letter) orelse return module_resource_error_io;
    var ok = false;
    defer fs_request.finish(&req, ok);
    var located: LocatedResource = undefined;
    const rc = locateModuleResourceImpl(file.volume, file.entry, resource_type, resource_index, resource_name, &located);
    if (rc != 0) return rc;
    if (located.size > out_len) return module_resource_error_too_small;
    const out = out_ptr[0..@intCast(located.size)];
    const got = vfs.readFileRange(file.volume, file.entry, @intCast(located.file_off), out) orelse return module_resource_error_io;
    if (got != located.size) return module_resource_error_io;
    ok = true;
    return @intCast(located.size);
}

pub fn fileRead(path_ptr: [*:0]const u8, out_ptr: [*]u8, max_len: u32) callconv(.c) i32 {
    var path_buf: [max_api_path]u8 = undefined;
    const raw_path = copyZ(path_ptr, path_buf[0..]) orelse return -1;
    var resolved_buf: [max_api_path]u8 = undefined;
    const target = resolveTarget(raw_path, &resolved_buf) orelse return -1;
    const volume = targetVolume(target) orelse return -2;
    var req = fs_request.begin(.file_read, target.drive_ref.letter) orelse return -7;
    var ok = false;
    defer fs_request.finish(&req, ok);
    var entry: vfs.Entry = undefined;
    switch (vfs.resolveEntryStatus(volume, target.path, &entry)) {
        .found => {},
        .not_found => return -3,
        .io => return -6,
    }
    if (entry.isDir()) return -4;
    if (entry.size > max_len) return -5;
    const out = out_ptr[0..@intCast(max_len)];
    const len = vfs.readFile(volume, entry, out) orelse return -6;
    ok = true;
    return @intCast(len);
}

pub fn fileWrite(path_ptr: [*:0]const u8, data_ptr: [*]const u8, len: u32) callconv(.c) i32 {
    var path_buf: [max_api_path]u8 = undefined;
    const raw_path = copyZ(path_ptr, path_buf[0..]) orelse return -1;
    var resolved_buf: [max_api_path]u8 = undefined;
    const target = resolveTarget(raw_path, &resolved_buf) orelse return -1;
    const volume = targetVolume(target) orelse return -2;
    var req = fs_request.begin(.file_write, target.drive_ref.letter) orelse return -5;
    var ok = false;
    defer fs_request.finish(&req, ok);
    var parent: vfs.NodeRef = undefined;
    if (vfs.resolvePathStatus(volume, parentPath(target.path), &parent) != .found) return -3;
    const basename = baseName(target.path);
    var existing: ?vfs.Entry = null;
    if (resolveOptionalEntryStatus(volume, target.path, &existing) == .io) return -4;
    if (!invalidateStreamSlotsForResolved(target.drive_ref.letter, targetOnBootVolume(target), parent, existing, basename))
        return -4;
    if (!vfs.writeFile(volume, parent, basename, data_ptr[0..@intCast(len)])) return -4;
    ok = true;
    return @intCast(len);
}

pub const RewriteFileFn = *const fn (
    input: []const u8,
    output: []u8,
    context: *const anyopaque,
) ?usize;

/// Kernel-internal read/modify/write transaction. The callback must not enter
/// R4SYS or block on another filesystem request; it runs while the single
/// namespace gate is held so CONFIG-style writers cannot lose each other's
/// updates between a public fileRead and fileWrite call.
pub fn rewriteFileUnderGate(
    raw_path: []const u8,
    input: []u8,
    output: []u8,
    context: *const anyopaque,
    rewrite: RewriteFileFn,
) i32 {
    var resolved_buf: [max_api_path]u8 = undefined;
    const target = resolveTarget(raw_path, &resolved_buf) orelse return -1;
    const volume = targetVolume(target) orelse return -2;
    if (isRootPath(target.path)) return -3;
    var req = fs_request.begin(.config_write, target.drive_ref.letter) orelse return -5;
    var ok = false;
    defer fs_request.finish(&req, ok);

    var parent: vfs.NodeRef = undefined;
    switch (vfs.resolvePathStatus(volume, parentPath(target.path), &parent)) {
        .found => {},
        .not_found => return -3,
        .io => return -6,
    }
    var existing: ?vfs.Entry = null;
    const existing_status = resolveOptionalEntryStatus(volume, target.path, &existing);
    if (existing_status == .io) return -6;
    var input_len: usize = 0;
    if (existing) |entry| {
        if (entry.isDir()) return -4;
        if (entry.size > input.len) return -5;
        input_len = vfs.readFile(volume, entry, input) orelse return -6;
    }
    const output_len = rewrite(input[0..input_len], output, context) orelse return -5;
    if (output_len > output.len or output_len > std.math.maxInt(i32)) return -5;

    const entry_name = baseName(target.path);
    if (!invalidateStreamSlotsForResolved(
        target.drive_ref.letter,
        targetOnBootVolume(target),
        parent,
        existing,
        entry_name,
    )) return -6;
    if (!vfs.writeFile(volume, parent, entry_name, output[0..output_len])) return -6;
    ok = true;
    return @intCast(output_len);
}

pub fn fileReadAt(path_ptr: [*:0]const u8, offset: u32, out_ptr: [*]u8, max_len: u32) callconv(.c) i32 {
    return fileReadAt64(path_ptr, offset, out_ptr, max_len);
}

pub fn fileReadAt64(path_ptr: [*:0]const u8, offset: u64, out_ptr: [*]u8, max_len: u32) callconv(.c) i32 {
    if (offset > @as(u64, @intCast(std.math.maxInt(usize)))) return -8;
    var path_buf: [max_api_path]u8 = undefined;
    const raw_path = copyZ(path_ptr, path_buf[0..]) orelse return -1;
    var resolved_buf: [max_api_path]u8 = undefined;
    const target = resolveTarget(raw_path, &resolved_buf) orelse return -1;
    const volume = targetVolume(target) orelse return -2;
    var req = fs_request.begin(.file_read_at, target.drive_ref.letter) orelse return -7;
    var ok = false;
    defer fs_request.finish(&req, ok);
    var entry: vfs.Entry = undefined;
    switch (vfs.resolveEntryStatus(volume, target.path, &entry)) {
        .found => {},
        .not_found => {
            recordFsFailure(.file_read_at, -3, vfs.lookupDiagnosticStage(volume));
            return -3;
        },
        .io => {
            recordFsFailure(.file_read_at, -6, 0);
            return -6;
        },
    }
    if (entry.isDir()) return -4;
    const out = out_ptr[0..@intCast(max_len)];
    const len = vfs.readFileRange(volume, entry, @intCast(offset), out) orelse {
        recordFsFailure(.file_read_at, -6, 0);
        return -6;
    };
    ok = true;
    return @intCast(len);
}

pub fn fileWriteAt(path_ptr: [*:0]const u8, offset: u64, data_ptr: [*]const u8, len: u32) callconv(.c) i32 {
    if (offset > @as(u64, @intCast(std.math.maxInt(usize)))) return -8;
    var path_buf: [max_api_path]u8 = undefined;
    const raw_path = copyZ(path_ptr, path_buf[0..]) orelse return -1;
    var resolved_buf: [max_api_path]u8 = undefined;
    const target = resolveTarget(raw_path, &resolved_buf) orelse return -1;
    const volume = targetVolume(target) orelse return -2;
    var req = fs_request.begin(.file_write_at, target.drive_ref.letter) orelse return -7;
    var ok = false;
    defer fs_request.finish(&req, ok);
    var parent: vfs.NodeRef = undefined;
    if (vfs.resolvePathStatus(volume, parentPath(target.path), &parent) != .found) return -3;
    var entry: vfs.Entry = undefined;
    switch (vfs.resolveEntryStatus(volume, target.path, &entry)) {
        .found => {},
        .not_found => return -3,
        .io => return -6,
    }
    if (!invalidateStreamSlotsForResolved(target.drive_ref.letter, targetOnBootVolume(target), parent, entry, baseName(target.path)))
        return -6;
    if (entry.isDir()) return -4;
    const data = data_ptr[0..@intCast(len)];
    const written = vfs.writeFileRange(volume, entry, @intCast(offset), data) orelse return -6;
    ok = true;
    return @intCast(written);
}

pub fn fileAppend(path_ptr: [*:0]const u8, data_ptr: [*]const u8, len: u32) callconv(.c) i32 {
    var path_buf: [max_api_path]u8 = undefined;
    const raw_path = copyZ(path_ptr, path_buf[0..]) orelse return -1;
    var resolved_buf: [max_api_path]u8 = undefined;
    const target = resolveTarget(raw_path, &resolved_buf) orelse return -1;
    const volume = targetVolume(target) orelse return -2;
    var req = fs_request.begin(.file_append, target.drive_ref.letter) orelse return -5;
    var ok = false;
    defer fs_request.finish(&req, ok);
    var parent: vfs.NodeRef = undefined;
    if (vfs.resolvePathStatus(volume, parentPath(target.path), &parent) != .found) return -3;
    const basename = baseName(target.path);
    var existing: ?vfs.Entry = null;
    if (resolveOptionalEntryStatus(volume, target.path, &existing) == .io) return -4;
    if (!invalidateStreamSlotsForResolved(target.drive_ref.letter, targetOnBootVolume(target), parent, existing, basename))
        return -4;
    if (!vfs.appendFile(volume, parent, basename, data_ptr[0..@intCast(len)])) return -4;
    ok = true;
    return @intCast(len);
}

pub fn fileStreamBegin(path_ptr: [*:0]const u8, flags: u32) callconv(.c) i32 {
    if ((flags & ~file_stream_open_supported_flags) != 0) return file_stream_error_unsupported;
    if ((flags & file_stream_open_supported_flags) == 0) return file_stream_error_invalid;
    const lease = (flags & file_stream_open_lease) != 0;
    if (lease and flags != (file_stream_open_create | file_stream_open_lease)) return file_stream_error_invalid;

    var path_buf: [max_api_path]u8 = undefined;
    const raw_path = copyZ(path_ptr, path_buf[0..]) orelse return file_stream_error_invalid;
    var resolved_buf: [max_api_path]u8 = undefined;
    const target = resolveTarget(raw_path, &resolved_buf) orelse return file_stream_error_invalid;
    const volume = targetVolume(target) orelse return file_stream_error_io;
    if (isRootPath(target.path)) return file_stream_error_io;
    var req = fs_request.begin(.stream_begin, target.drive_ref.letter) orelse return file_stream_error_io;
    var ok = false;
    defer fs_request.finish(&req, ok);

    reapDeadStreamSlots();
    var parent: vfs.NodeRef = undefined;
    if (vfs.resolvePathStatus(volume, parentPath(target.path), &parent) != .found)
        return file_stream_error_io;
    var existing: ?vfs.Entry = null;
    if (resolveOptionalEntryStatus(volume, target.path, &existing) == .io)
        return file_stream_error_io;
    const entry_name = baseName(target.path);
    // A create-only publish owns not only its private stage name, but also
    // the recorded target and backup names until the final durability
    // acknowledgement. Do not let a second Begin occupy either reserved
    // name while the first caller still owns the replay token.
    if (hasPublishStreamCollisionForResolved(
        target.drive_ref.letter,
        targetOnBootVolume(target),
        parent,
        existing,
        entry_name,
    )) return file_stream_error_exists;
    if (findStreamSlotForResolved(target.drive_ref.letter, targetOnBootVolume(target), parent, existing, entry_name) != null)
        return file_stream_error_exists;
    if (existing) |entry| {
        if (entry.isDir()) return file_stream_error_invalid;
        if (!lease and (flags & file_stream_open_truncate) == 0) return file_stream_error_exists;
    } else if ((flags & file_stream_open_create) == 0) {
        return file_stream_error_not_found;
    }

    // Key slots by the resolved VFS entry, not by the caller's spelling.
    // Slash variants, case variants and relative paths can otherwise open two
    // logical owners for the same file.
    // Never evict an unrelated active stream when the bounded table is full.
    const reserved_slot = freeStreamSlot() orelse return file_stream_error_io;
    rememberStreamSlot(reserved_slot, target.path, target.drive_ref.letter, targetOnBootVolume(target), parent, entry_name, flags);
    // Resolve and bind the complete physical ancestor chain before the first
    // namespace mutation. Overflow, a missing component or an I/O ambiguity
    // therefore leaves neither a half-described slot nor a newly-created file.
    if (!bindStreamSlotAncestors(reserved_slot, volume, parentPath(target.path), parent)) {
        clearStreamSlot(reserved_slot);
        return file_stream_error_io;
    }
    if (lease and existing != null) {
        // Lease files are explicitly transient.  A present file without a
        // live StreamSlot is an orphan from a dead process or an earlier
        // boot and may be reclaimed; ordinary stream targets never get this
        // destructive treatment.
        if (!vfs.deleteFile(volume, parent, entry_name)) {
            clearStreamSlot(reserved_slot);
            return file_stream_error_io;
        }
        if (resolveOptionalEntryStatus(volume, target.path, &existing) != .not_found) {
            clearStreamSlot(reserved_slot);
            return file_stream_error_io;
        }
    }
    if (!vfs.writeFile(volume, parent, entry_name, "")) {
        if (existing == null) {
            // Never delete by path after ambiguous create completion. The
            // write may have published our file; a statusful lookup may bind
            // it, while I/O ambiguity retains the reserved owner slot.
            var ambiguous: ?vfs.Entry = null;
            switch (resolveOptionalEntryStatus(volume, target.path, &ambiguous)) {
                .found => bindStreamSlotToEntry(reserved_slot, ambiguous.?),
                .not_found => clearStreamSlot(reserved_slot),
                // No later path-only call may claim an object whose identity
                // could not be observed in the mutation's gate interval.
                // Leave a possible private orphan for offline cleanup, but
                // do not retain an unbound ownership token.
                .io => clearStreamSlot(reserved_slot),
            }
        } else {
            // Never let Abort delete a pre-existing file after a failed
            // truncate whose mutation point is unknown.
            clearStreamSlot(reserved_slot);
        }
        return file_stream_error_io;
    }
    var created: vfs.Entry = undefined;
    if (vfs.resolveEntryStatus(volume, target.path, &created) != .found) {
        // The create may be durable, but without an observed backend identity
        // it is unsafe to let a future Abort delete by name.
        clearStreamSlot(reserved_slot);
        return file_stream_error_io;
    }
    bindStreamSlotToEntry(reserved_slot, created);
    reserved_slot.ready = true;
    ok = true;
    return file_stream_result_ok;
}

pub fn fileStreamWrite(path_ptr: [*:0]const u8, offset: u64, data_ptr: [*]const u8, len: u32, flags: u32) callconv(.c) i32 {
    if (flags != 0) return file_stream_error_unsupported;
    const len64: u64 = @intCast(len);
    if (offset > 0xFFFF_FFFF or len64 > 0xFFFF_FFFF - offset) return file_stream_error_too_large;

    var path_buf: [max_api_path]u8 = undefined;
    const raw_path = copyZ(path_ptr, path_buf[0..]) orelse return file_stream_error_invalid;
    var resolved_buf: [max_api_path]u8 = undefined;
    const target = resolveTarget(raw_path, &resolved_buf) orelse return file_stream_error_invalid;
    const volume = targetVolume(target) orelse return file_stream_error_io;
    var req = fs_request.begin(.stream_write, target.drive_ref.letter) orelse return file_stream_error_io;
    var ok = false;
    defer fs_request.finish(&req, ok);
    var parent: vfs.NodeRef = undefined;
    if (vfs.resolvePathStatus(volume, parentPath(target.path), &parent) != .found)
        return file_stream_error_io;
    var entry: ?vfs.Entry = null;
    if (resolveOptionalEntryStatus(volume, target.path, &entry) == .io)
        return file_stream_error_io;
    if (entry == null) return file_stream_error_not_found;

    // Slot discovery must happen while the global filesystem gate is held.
    // Looking up a raw slot pointer and then blocking on the gate allowed an
    // abort/delete to clear and reuse that storage for another stream (ABA).
    if (findOwnedStreamSlotForResolved(target.drive_ref.letter, targetOnBootVolume(target), parent, entry, baseName(target.path))) |slot| {
        if (!slot.ready or slot.finished) return file_stream_error_io;
        bindStreamSlotToEntry(slot, entry.?);
        if (slot.size != offset) return file_stream_error_offset_mismatch;
        if (len == 0) {
            ok = true;
            return 0;
        }
        const slot_volume = streamSlotVolume(slot) orelse return file_stream_error_io;
        const status = vfs.appendFileAtOffsetStatusDeferred(slot_volume, slot.parent_node, streamSlotName(slot), @intCast(offset), data_ptr[0..@intCast(len)]);
        switch (status) {
            .ok => {},
            .invalid => return file_stream_error_invalid,
            .not_found => return file_stream_error_not_found,
            .offset_mismatch => return file_stream_error_offset_mismatch,
            .too_large => return file_stream_error_too_large,
            .io => return file_stream_error_io,
        }
        // FAT empty files are initially identified by node 0. The first
        // append allocates their first cluster, so retaining the old node
        // makes every later chunk/Finish/Abort look like a foreign file.
        // Record the confirmed progress first (so a transient lookup error
        // can be reconciled on the next call), then bind the post-append
        // backend identity while the namespace gate is still held.
        const needs_identity_refresh = slot.file_node == 0 and slot.file_node_generation == 0;
        slot.size += len64;
        slot.identity_refresh_pending = needs_identity_refresh;
        var updated: vfs.Entry = undefined;
        switch (vfs.lookupEntryStatus(slot_volume, slot.parent_node, streamSlotName(slot), &updated)) {
            .found => {
                if (updated.isDir() or updated.size != slot.size) return file_stream_error_io;
                bindStreamSlotToEntry(slot, updated);
                // An empty FAT32 stage had no cluster identity when its claim
                // was declared; now that the first append established one, the
                // durable claim has to name the same object (0.60.30).
                if (needs_identity_refresh) refreshDeclaredClaimIdentity(slot);
            },
            .not_found => return file_stream_error_not_found,
            .io => return file_stream_error_io,
        }
        ok = true;
        return @intCast(len);
    }

    // A stream operation without its live ownership slot must fail closed.
    // The old fallback appended to whichever file currently occupied the
    // path after delete/replace invalidated the slot.
    const current_entry = entry orelse return file_stream_error_not_found;
    if (current_entry.isDir()) return file_stream_error_invalid;
    if (current_entry.size != offset) return file_stream_error_offset_mismatch;
    return file_stream_error_not_found;
}

pub fn fileStreamFinish(path_ptr: [*:0]const u8, expected_size: u64, flags: u32) callconv(.c) i32 {
    if ((flags & ~file_stream_finish_supported_flags) != 0) return file_stream_error_unsupported;
    const keep_ownership = (flags & file_stream_finish_keep_ownership) != 0;
    var path_buf: [max_api_path]u8 = undefined;
    const raw_path = copyZ(path_ptr, path_buf[0..]) orelse return file_stream_error_invalid;
    var resolved_buf: [max_api_path]u8 = undefined;
    const target = resolveTarget(raw_path, &resolved_buf) orelse return file_stream_error_invalid;
    const volume = targetVolume(target) orelse return file_stream_error_io;
    var req = fs_request.begin(.stream_finish, target.drive_ref.letter) orelse return file_stream_error_io;
    var ok = false;
    defer fs_request.finish(&req, ok);
    var parent: vfs.NodeRef = undefined;
    if (vfs.resolvePathStatus(volume, parentPath(target.path), &parent) != .found)
        return file_stream_error_io;
    var entry: ?vfs.Entry = null;
    if (resolveOptionalEntryStatus(volume, target.path, &entry) == .io)
        return file_stream_error_io;
    if (entry == null) return file_stream_error_not_found;

    if (findOwnedStreamSlotForResolved(target.drive_ref.letter, targetOnBootVolume(target), parent, entry, baseName(target.path))) |slot| {
        if (!slot.ready or slot.finished) return file_stream_error_io;
        bindStreamSlotToEntry(slot, entry.?);
        if (entry.?.isDir()) return file_stream_error_invalid;
        if (slot.size != expected_size) return file_stream_error_size_mismatch;
        if (entry.?.size != expected_size) return file_stream_error_size_mismatch;
        const slot_volume = streamSlotVolume(slot) orelse return file_stream_error_io;
        if (!vfs.flushVolume(slot_volume)) return file_stream_error_io;
        if (keep_ownership) {
            slot.finished = true;
        } else {
            clearStreamSlot(slot);
        }
        ok = true;
        return file_stream_result_ok;
    }

    const current_entry = entry orelse return file_stream_error_not_found;
    if (current_entry.isDir()) return file_stream_error_invalid;
    if (current_entry.size != expected_size) return file_stream_error_size_mismatch;
    return file_stream_error_not_found;
}

pub fn fileStreamAbort(path_ptr: [*:0]const u8) callconv(.c) i32 {
    var path_buf: [max_api_path]u8 = undefined;
    const raw_path = copyZ(path_ptr, path_buf[0..]) orelse return file_stream_error_invalid;
    var resolved_buf: [max_api_path]u8 = undefined;
    const target = resolveTarget(raw_path, &resolved_buf) orelse return file_stream_error_invalid;
    const volume = targetVolume(target) orelse return file_stream_error_io;
    if (isRootPath(target.path)) return file_stream_error_io;
    var req = fs_request.begin(.stream_abort, target.drive_ref.letter) orelse return file_stream_error_io;
    var ok = false;
    defer fs_request.finish(&req, ok);
    var parent: vfs.NodeRef = undefined;
    if (vfs.resolvePathStatus(volume, parentPath(target.path), &parent) != .found)
        return file_stream_error_io;
    var entry: ?vfs.Entry = null;
    const entry_status = resolveOptionalEntryStatus(volume, target.path, &entry);

    if (findOwnedStreamSlotForResolved(
        target.drive_ref.letter,
        targetOnBootVolume(target),
        parent,
        if (entry_status == .found) entry else null,
        baseName(target.path),
    )) |slot| {
        if (entry_status == .found) bindStreamSlotToEntry(slot, entry.?);
        if (slot.publish_started) {
            if (slot.publish_target_name_len == 0 or slot.publish_backup_name_len == 0)
                return file_stream_error_io;
            // Once the target visibility point may have been crossed, Abort
            // cannot safely turn the operation back into a path deletion.
            // Settle the exact recorded ownership transfer instead. This
            // makes session teardown idempotent and does not leak a StreamSlot
            // after an acknowledgement was lost.
            const publish_result = runOwnedCreateOnlyPublish(
                volume,
                slot.parent_node,
                streamPublishTargetName(slot),
                streamSlotName(slot),
                streamPublishBackupName(slot),
                slot,
            );
            switch (publish_result) {
                .ok, .not_found => {
                    if (slot.publish_claim_generation != 0 and
                        !upload_claim_store.retire(slot.publish_claim_generation))
                        return file_stream_error_io;
                    clearStreamSlot(slot);
                    ok = true;
                    return file_stream_result_ok;
                },
                .invalid, .alias, .conflict, .read_only, .io, .not_atomic => return file_stream_error_io,
            }
        }
        if (entry_status == .io) return file_stream_error_io;
        const slot_volume = streamSlotVolume(slot) orelse return file_stream_error_io;
        if (entry_status == .not_found) {
            if (!vfs.flushVolume(slot_volume)) return file_stream_error_io;
            clearStreamSlot(slot);
            ok = true;
            return file_stream_result_ok;
        }
        _ = vfs.deleteFile(slot_volume, slot.parent_node, streamSlotName(slot));
        var after: ?vfs.Entry = null;
        var after_status = resolveOptionalEntryStatus(slot_volume, target.path, &after);
        if (after_status == .found and streamSlotOwnsPublishedEntry(slot, after.?)) {
            // One bounded retry, still under the same namespace gate and
            // still bound to the exact slot identity checked above.
            _ = vfs.deleteFile(slot_volume, slot.parent_node, streamSlotName(slot));
            after_status = resolveOptionalEntryStatus(slot_volume, target.path, &after);
        }
        if (after_status == .io or after_status == .found) return file_stream_error_io;
        if (!vfs.flushVolume(slot_volume)) return file_stream_error_io;
        // The stage is provably gone, so a declared claim has nothing left to
        // recover (0.60.30).  Retiring it here keeps the boot sweep quiet;
        // leaving it would still be safe, just noisy on the next mount.
        if (slot.publish_claim_generation != 0 and
            !upload_claim_store.retire(slot.publish_claim_generation))
            return file_stream_error_io;
        clearStreamSlot(slot);
        ok = true;
        return file_stream_result_ok;
    }

    // Never delete a current occupant after this stream's ownership was
    // invalidated or already retired.
    return file_stream_error_not_found;
}

pub fn dirList(path_ptr: [*:0]const u8, out_ptr: [*]u8, max_len: u32) callconv(.c) i32 {
    if (max_len == 0) return -1;
    var path_buf: [max_api_path]u8 = undefined;
    const raw_path = copyZ(path_ptr, path_buf[0..]) orelse return -2;
    var resolved_buf: [max_api_path]u8 = undefined;
    const target = resolveTarget(raw_path, &resolved_buf) orelse return -2;
    const volume = targetVolume(target) orelse return -3;
    var req = fs_request.begin(.dir_list, target.drive_ref.letter) orelse return -6;
    var ok = false;
    defer fs_request.finish(&req, ok);
    const cluster = vfs.resolvePath(volume, target.path) orelse return -4;
    const out = out_ptr[0..@intCast(max_len)];
    const len = vfs.readDirectory(volume, cluster, out) orelse return -5;
    ok = true;
    return @intCast(len);
}

pub fn dirEntry(path_ptr: [*:0]const u8, index: u32, out_ptr: [*]u8, max_len: u32) callconv(.c) i32 {
    if (max_len == 0) return -1;
    var path_buf: [max_api_path]u8 = undefined;
    const raw_path = copyZ(path_ptr, path_buf[0..]) orelse return -2;
    var resolved_buf: [max_api_path]u8 = undefined;
    const target = resolveTarget(raw_path, &resolved_buf) orelse return -2;
    const out = out_ptr[0..@intCast(max_len)];
    if (index == 0) {
        if (!copyDrivePathZ(target.drive_ref.letter, target.path, out)) return -7;
        return 1;
    }
    if (index == 1) {
        var parent_buf: [max_api_path]u8 = undefined;
        const parent = parentPathToBuffer(target.path, parent_buf[0..]) orelse return -7;
        if (!copyDrivePathZ(target.drive_ref.letter, parent, out)) return -7;
        return 1;
    }
    const volume = targetVolume(target) orelse return -3;
    var req = fs_request.begin(.dir_entry, target.drive_ref.letter) orelse return -8;
    var ok = false;
    defer fs_request.finish(&req, ok);
    var cluster: vfs.NodeRef = undefined;
    switch (vfs.resolvePathStatus(volume, target.path, &cluster)) {
        .found => {},
        .not_found => return -4,
        .io => return dir_entry_error_io,
    }

    // NAME_MAX bytes plus the terminating NUL: a full-length 64-byte name
    // (e.g. the fixture's 64-char Win32 name) must survive the copy.
    var name_buf: [vfs.NAME_MAX + 1]u8 = undefined;
    var entry: vfs.Entry = undefined;
    switch (vfs.readDirectoryEntryStatus(volume, cluster, index, name_buf[0..], &entry)) {
        .found => {},
        .not_found => return dir_entry_result_end,
        .io => return dir_entry_error_io,
    }
    const entry_name = name_buf[0..entry.name_len];
    var entry_path_buf: [max_api_path]u8 = undefined;
    const entry_path = joinPathToBuffer(target.path, entry_name, entry_path_buf[0..]) orelse return -7;
    if (!copyDrivePathZ(target.drive_ref.letter, entry_path, out)) return -7;
    ok = true;
    return if (entry.isDir()) 1 else 0;
}

pub fn fileInfo(path_ptr: [*:0]const u8, out: *FileInfo) callconv(.c) i32 {
    out.* = .{};
    var path_buf: [max_api_path]u8 = undefined;
    const raw_path = copyZ(path_ptr, path_buf[0..]) orelse return -1;
    var resolved_buf: [max_api_path]u8 = undefined;
    const target = resolveTarget(raw_path, &resolved_buf) orelse return -1;
    out.drive = target.drive_ref.letter;
    copyFixedZ(out.name[0..], baseName(target.path));
    const volume = targetVolume(target) orelse return -2;
    var req = fs_request.begin(.file_info, target.drive_ref.letter) orelse return -3;
    var ok = false;
    defer fs_request.finish(&req, ok);
    if (isRootPath(target.path)) {
        out.exists = 1;
        out.is_dir = 1;
        out.attr = 0x10;
        out.first_cluster = @intCast(volume.rootNode());
        copyFixedZ(out.name[0..], "\\");
        ok = true;
        return 1;
    }
    var entry: vfs.Entry = undefined;
    switch (vfs.resolveEntryStatus(volume, target.path, &entry)) {
        .found => {},
        .not_found => {
            recordFsFailure(.file_info, 0, vfs.lookupDiagnosticStage(volume));
            return 0;
        },
        .io => {
            recordFsFailure(.file_info, -4, 0);
            return -4;
        },
    }
    out.exists = 1;
    out.is_dir = if (entry.isDir()) 1 else 0;
    out.attr = entry.attr;
    out.size = entry.size;
    out.first_cluster = @intCast(entry.node);
    out.created_time = entry.created_time;
    out.created_date = entry.created_date;
    out.access_date = entry.access_date;
    out.modified_time = entry.modified_time;
    out.modified_date = entry.modified_date;
    copyFixedZ(out.name[0..], entry.name[0..entry.name_len]);
    ok = true;
    return 1;
}

pub fn fileDelete(path_ptr: [*:0]const u8) callconv(.c) i32 {
    var path_buf: [max_api_path]u8 = undefined;
    const raw_path = copyZ(path_ptr, path_buf[0..]) orelse return -1;
    var resolved_buf: [max_api_path]u8 = undefined;
    const target = resolveTarget(raw_path, &resolved_buf) orelse return -1;
    const volume = targetVolume(target) orelse return -2;
    if (isRootPath(target.path)) return -3;
    var req = fs_request.begin(.file_delete, target.drive_ref.letter) orelse return -5;
    var ok = false;
    defer fs_request.finish(&req, ok);
    var parent: vfs.NodeRef = undefined;
    switch (vfs.resolvePathStatus(volume, parentPath(target.path), &parent)) {
        .found => {},
        .not_found => {
            recordFsFailure(.file_delete, -4, vfs.lookupDiagnosticStage(volume));
            return -4;
        },
        .io => {
            recordFsFailure(.file_delete, -4, 0);
            return -4;
        },
    }
    var existing: ?vfs.Entry = null;
    switch (resolveOptionalEntryStatus(volume, target.path, &existing)) {
        .found => {},
        .not_found => {
            ok = true;
            return 0;
        },
        .io => return -6,
    }
    if (!invalidateStreamSlotsForResolved(
        target.drive_ref.letter,
        targetOnBootVolume(target),
        parent,
        existing,
        baseName(target.path),
    )) return -6;
    const deleted = vfs.deleteFile(volume, parent, baseName(target.path));
    if (!deleted) {
        var after: vfs.Entry = undefined;
        switch (vfs.resolveEntryStatus(volume, target.path, &after)) {
            .not_found => {
                // The backend mutation completed but its final completion was
                // lost; absence is the authoritative idempotent result.
                ok = true;
                return 1;
            },
            .found, .io => return -6,
        }
    }
    ok = true;
    return 1;
}

/// Content- and identity-bound delete used by transactional cleanup.  Lookup,
/// full FNV-1a comparison, backend identity recheck, deletion and durability
/// reconciliation all run while the same filesystem-request gate is held.
/// A competing replace can therefore only happen before this operation
/// starts (and produce conflict) or after it has completed.
pub fn fileDeleteIfMatch(
    path_ptr: [*:0]const u8,
    expected_size: u64,
    expected_checksum: u32,
) callconv(.c) i32 {
    var path_buf: [max_api_path]u8 = undefined;
    const raw_path = copyZ(path_ptr, path_buf[0..]) orelse
        return file_delete_if_match_error_invalid;
    var resolved_buf: [max_api_path]u8 = undefined;
    const target = resolveTarget(raw_path, &resolved_buf) orelse
        return file_delete_if_match_error_invalid;
    const volume = targetVolume(target) orelse
        return file_delete_if_match_error_unsupported;
    if (isRootPath(target.path)) return file_delete_if_match_error_invalid;

    var req = fs_request.begin(.file_delete_if_match, target.drive_ref.letter) orelse
        return file_delete_if_match_error_io;
    var ok = false;
    defer fs_request.finish(&req, ok);

    var parent: vfs.NodeRef = undefined;
    switch (vfs.resolvePathStatus(volume, parentPath(target.path), &parent)) {
        .found => {},
        .not_found => {
            ok = vfs.flushVolume(volume);
            return if (ok)
                file_delete_if_match_result_not_found
            else
                file_delete_if_match_error_io;
        },
        .io => return file_delete_if_match_error_io,
    }

    const entry_name = baseName(target.path);
    var entry: vfs.Entry = undefined;
    switch (vfs.lookupEntryStatus(volume, parent, entry_name, &entry)) {
        .found => {},
        .not_found => {
            ok = vfs.flushVolume(volume);
            return if (ok)
                file_delete_if_match_result_not_found
            else
                file_delete_if_match_error_io;
        },
        .io => return file_delete_if_match_error_io,
    }
    if (entry.isDir() or entry.size != expected_size)
        return file_delete_if_match_error_conflict;
    if (entry.size > @as(u64, @intCast(std.math.maxInt(usize))))
        return file_delete_if_match_error_io;

    var checksum = file_delete_if_match_checksum_seed;
    var offset: u64 = 0;
    while (offset < entry.size) {
        const want_u64 = @min(
            @as(u64, @intCast(file_delete_if_match_buffer.len)),
            entry.size - offset,
        );
        const want: usize = @intCast(want_u64);
        const got = vfs.readFileRange(
            volume,
            entry,
            @intCast(offset),
            file_delete_if_match_buffer[0..want],
        ) orelse return file_delete_if_match_error_io;
        if (got != want) return file_delete_if_match_error_io;
        for (file_delete_if_match_buffer[0..want]) |byte| {
            checksum ^= byte;
            checksum *%= 16777619;
        }
        offset += want_u64;
    }
    if (checksum != expected_checksum)
        return file_delete_if_match_error_conflict;

    if (!invalidateStreamSlotsForResolved(
        target.drive_ref.letter,
        targetOnBootVolume(target),
        parent,
        entry,
        name,
    )) return file_delete_if_match_error_io;
    const result = vfs.deleteFileIfIdentity(volume, parent, entry_name, entry);
    ok = result != .io;
    return switch (result) {
        .deleted => file_delete_if_match_result_deleted,
        .not_found => file_delete_if_match_result_not_found,
        .mismatch => file_delete_if_match_error_conflict,
        .io => file_delete_if_match_error_io,
    };
}

/// Fingerprint- and identity-bound SYSUPD transition.  All three lookups,
/// transient-alias inspection, full checksums, backend mutation, cleanup and
/// final verification run under this one filesystem-request gate.
pub fn fileUpdateAtomicChecked(
    target_ptr: [*:0]const u8,
    staged_ptr: [*:0]const u8,
    backup_ptr: [*:0]const u8,
    new_size: u64,
    new_checksum: u32,
    old_size: u64,
    old_checksum: u32,
    flags: u32,
) callconv(.c) i32 {
    if ((flags & ~file_update_atomic_checked_supported_flags) != 0)
        return file_update_atomic_checked_error_invalid;
    const forward = (flags & file_update_atomic_checked_flag_forward) != 0;
    const rollback = (flags & file_update_atomic_checked_flag_rollback) != 0;
    const target_existed = (flags & file_update_atomic_checked_flag_target_existed) != 0;
    const old_known = (flags & file_update_atomic_checked_flag_old_known) != 0;
    if (forward == rollback or (old_known and !target_existed))
        return file_update_atomic_checked_error_invalid;

    var target_buf: [max_api_path]u8 = undefined;
    var staged_buf: [max_api_path]u8 = undefined;
    var backup_buf: [max_api_path]u8 = undefined;
    const raw_target = copyZ(target_ptr, target_buf[0..]) orelse
        return file_update_atomic_checked_error_invalid;
    const raw_staged = copyZ(staged_ptr, staged_buf[0..]) orelse
        return file_update_atomic_checked_error_invalid;
    const raw_backup = copyZ(backup_ptr, backup_buf[0..]) orelse
        return file_update_atomic_checked_error_invalid;

    var target_resolved: [max_api_path]u8 = undefined;
    var staged_resolved: [max_api_path]u8 = undefined;
    var backup_resolved: [max_api_path]u8 = undefined;
    const target = resolveTarget(raw_target, &target_resolved) orelse
        return file_update_atomic_checked_error_bad_path;
    const staged = resolveTarget(raw_staged, &staged_resolved) orelse
        return file_update_atomic_checked_error_bad_path;
    const backup = resolveTarget(raw_backup, &backup_resolved) orelse
        return file_update_atomic_checked_error_bad_path;
    const volume = targetVolume(target) orelse
        return file_update_atomic_checked_error_unsupported;
    const staged_volume = targetVolume(staged) orelse
        return file_update_atomic_checked_error_unsupported;
    const backup_volume = targetVolume(backup) orelse
        return file_update_atomic_checked_error_unsupported;
    if (!vfs.sameVolume(volume, staged_volume) or
        !vfs.sameVolume(volume, backup_volume))
    {
        return file_update_atomic_checked_error_not_atomic;
    }
    if (isRootPath(target.path) or isRootPath(staged.path) or isRootPath(backup.path))
        return file_update_atomic_checked_error_bad_path;

    const target_parent = parentPath(target.path);
    if (!stdMemEql(target_parent, parentPath(staged.path)) or
        !stdMemEql(target_parent, parentPath(backup.path)))
    {
        return file_update_atomic_checked_error_not_atomic;
    }
    const target_name = baseName(target.path);
    const staged_name = baseName(staged.path);
    const backup_name = baseName(backup.path);
    if (!vfs.validateShortName83(staged_name) or !vfs.validateShortName83(backup_name))
        return file_update_atomic_checked_error_not_atomic;

    var req = fs_request.begin(.file_update_atomic_checked, target.drive_ref.letter) orelse
        return file_update_atomic_checked_error_io;
    var ok = false;
    defer fs_request.finish(&req, ok);

    var parent: vfs.NodeRef = undefined;
    switch (vfs.resolvePathStatus(volume, target_parent, &parent)) {
        .found => {},
        .not_found => return file_update_atomic_checked_error_bad_path,
        .io => return file_update_atomic_checked_error_io,
    }
    // A lost create-only publish acknowledgement may leave target and stage
    // as durable aliases while its StreamSlot carries the only ownership
    // proof. Checked SYSUPD must not consume, overwrite or delete any member
    // of that tuple. Ordinary completed stream leases retain the historical
    // invalidation behaviour.
    if (hasPublishStreamCollisionForResolved(
        target.drive_ref.letter,
        targetOnBootVolume(target),
        parent,
        null,
        target_name,
    ) or hasPublishStreamCollisionForResolved(
        staged.drive_ref.letter,
        targetOnBootVolume(staged),
        parent,
        null,
        staged_name,
    ) or hasPublishStreamCollisionForResolved(
        backup.drive_ref.letter,
        targetOnBootVolume(backup),
        parent,
        null,
        backup_name,
    )) return file_update_atomic_checked_error_io;
    _ = invalidateStreamSlotsForResolved(
        target.drive_ref.letter,
        targetOnBootVolume(target),
        parent,
        null,
        target_name,
    );
    _ = invalidateStreamSlotsForResolved(
        staged.drive_ref.letter,
        targetOnBootVolume(staged),
        parent,
        null,
        staged_name,
    );
    _ = invalidateStreamSlotsForResolved(
        backup.drive_ref.letter,
        targetOnBootVolume(backup),
        parent,
        null,
        backup_name,
    );
    const result = system_update_atomic.transition(
        volume,
        parent,
        target_name,
        staged_name,
        backup_name,
        if (forward) .forward else .rollback,
        .{
            .target_existed = target_existed,
            .old_known = old_known,
            .new_size = new_size,
            .new_checksum = new_checksum,
            .old_size = old_size,
            .old_checksum = old_checksum,
        },
    );
    ok = result == .ok;
    return switch (result) {
        .ok => file_update_atomic_checked_result_ok,
        .conflict => file_update_atomic_checked_error_conflict,
        .io => file_update_atomic_checked_error_io,
        .not_atomic => file_update_atomic_checked_error_not_atomic,
    };
}

/// Declares the create-only publish intent of an ALREADY OPEN stream
/// (0.60.30).
///
/// Without this the durable claim only came into existence at the final
/// hand-over, so a reset while the payload was still streaming left a stage
/// file nothing could attribute or remove - the object was consistent, but it
/// stayed behind forever.  Declaring the intent up front makes the claim
/// bracket the WHOLE transfer, and the existing replay rules then apply
/// unchanged: before the visibility point the abandoned upload is rolled
/// back, after it the hand-over is completed.
pub fn fileStreamDeclarePublish(
    staged_ptr: [*:0]const u8,
    target_ptr: [*:0]const u8,
    backup_ptr: [*:0]const u8,
    protocol_raw: u32,
) callconv(.c) i32 {
    const protocol = publishProtocolFromCode(protocol_raw) orelse
        return file_stream_declare_publish_error_invalid;
    var staged_buf: [max_api_path]u8 = undefined;
    var target_buf: [max_api_path]u8 = undefined;
    var backup_buf: [max_api_path]u8 = undefined;
    const raw_staged = copyZ(staged_ptr, staged_buf[0..]) orelse
        return file_stream_declare_publish_error_invalid;
    const raw_target = copyZ(target_ptr, target_buf[0..]) orelse
        return file_stream_declare_publish_error_invalid;
    const raw_backup = copyZ(backup_ptr, backup_buf[0..]) orelse
        return file_stream_declare_publish_error_invalid;

    var staged_resolved: [max_api_path]u8 = undefined;
    var target_resolved: [max_api_path]u8 = undefined;
    var backup_resolved: [max_api_path]u8 = undefined;
    const staged = resolveTarget(raw_staged, &staged_resolved) orelse
        return file_stream_declare_publish_error_bad_path;
    const target = resolveTarget(raw_target, &target_resolved) orelse
        return file_stream_declare_publish_error_bad_path;
    const backup = resolveTarget(raw_backup, &backup_resolved) orelse
        return file_stream_declare_publish_error_bad_path;
    if (isRootPath(staged.path) or isRootPath(target.path) or isRootPath(backup.path))
        return file_stream_declare_publish_error_bad_path;

    const volume = targetVolume(staged) orelse return file_stream_declare_publish_error_unsupported;
    const target_volume = targetVolume(target) orelse return file_stream_declare_publish_error_unsupported;
    const backup_volume = targetVolume(backup) orelse return file_stream_declare_publish_error_unsupported;
    if (!vfs.sameVolume(volume, target_volume) or !vfs.sameVolume(volume, backup_volume))
        return file_stream_declare_publish_error_not_atomic;

    const parent_path = parentPath(staged.path);
    if (!stdMemEql(parent_path, parentPath(target.path)) or
        !stdMemEql(parent_path, parentPath(backup.path)))
    {
        return file_stream_declare_publish_error_not_atomic;
    }

    const staged_name = baseName(staged.path);
    const target_name = baseName(target.path);
    const backup_name = baseName(backup.path);
    if (!vfs.validateShortName83(staged_name) or !vfs.validateShortName83(backup_name))
        return file_stream_declare_publish_error_not_atomic;

    var req = fs_request.begin(.stream_begin, staged.drive_ref.letter) orelse
        return file_stream_declare_publish_error_io;
    var ok = false;
    defer fs_request.finish(&req, ok);

    var parent: vfs.NodeRef = undefined;
    if (vfs.resolvePathStatus(volume, parent_path, &parent) != .found)
        return file_stream_declare_publish_error_bad_path;
    var entry: ?vfs.Entry = null;
    if (resolveOptionalEntryStatus(volume, staged.path, &entry) == .io)
        return file_stream_declare_publish_error_io;

    // Only the caller that actually owns this open stream may declare its
    // intent; a foreign path must never mint a claim over someone else's
    // object.
    const slot = findOwnedStreamSlotForResolved(
        staged.drive_ref.letter,
        targetOnBootVolume(staged),
        parent,
        entry,
        staged_name,
    ) orelse return file_stream_declare_publish_error_not_found;
    if (slot.publish_declared or slot.publish_claim_generation != 0)
        return file_stream_declare_publish_error_invalid;

    const claim_parent_path = buildClaimParentPath(staged.drive_ref.letter, staged.path) orelse
        return file_stream_declare_publish_error_io;
    const identity = upc.FileIdentity{
        .node = slot.file_node,
        .generation = slot.file_node_generation,
        .size = slot.size,
    };
    const claim = upload_claim_store.beginPublish(
        volume,
        claim_parent_path,
        staged_name,
        target_name,
        backup_name,
        identity,
        protocol,
    );
    if (!claim.ok) return file_stream_declare_publish_error_io;

    slot.publish_claim_generation = claim.generation;
    slot.publish_declared = true;
    slot.publish_protocol = @intFromEnum(protocol);
    if (!rememberStreamPublishTuple(slot, target_name, backup_name)) {
        if (upload_claim_store.retire(slot.publish_claim_generation)) {
            slot.publish_claim_generation = 0;
            slot.publish_declared = false;
            slot.publish_protocol = 0;
        }
        return file_stream_declare_publish_error_not_atomic;
    }
    if (claim_parent_path.len <= slot.publish_declared_parent.len) {
        @memcpy(slot.publish_declared_parent[0..claim_parent_path.len], claim_parent_path);
        slot.publish_declared_parent_len = claim_parent_path.len;
    }
    // The tuple is reserved, not yet published: Abort/Close must still be
    // able to roll the stage back normally.
    slot.publish_started = false;
    ok = true;
    return file_stream_declare_publish_result_ok;
}

/// Keeps the durable claim in step when an empty FAT32 stage gains its
/// cluster identity on the first append (0.60.30).
fn refreshDeclaredClaimIdentity(slot: *StreamSlot) void {
    if (!slot.publish_declared or slot.publish_claim_generation == 0) return;
    if (slot.publish_declared_parent_len == 0) return;
    const volume = streamSlotVolume(slot) orelse return;
    _ = upload_claim_store.refreshIdentity(
        volume,
        slot.publish_declared_parent[0..slot.publish_declared_parent_len],
        streamSlotName(slot),
        streamPublishTargetName(slot),
        streamPublishBackupName(slot),
        .{ .node = slot.file_node, .generation = slot.file_node_generation, .size = slot.size },
        @enumFromInt(slot.publish_protocol),
        slot.publish_claim_generation,
    );
}

/// Backend-exact name collation (0.60.24).
///
/// Answers whether two paths would resolve to the SAME object on their shared
/// volume, using the filesystem's own rule: NTFS folds through `$UpCase`,
/// FAT32 folds ASCII case only.  These genuinely disagree beyond ASCII, so a
/// caller that folds bytes itself is not proving identity - it is guessing.
/// SYSUPD uses this to reject two payload targets that would silently land in
/// one file.
pub fn pathNamesEqualCollated(
    left_ptr: [*:0]const u8,
    right_ptr: [*:0]const u8,
) callconv(.c) i32 {
    var left_buf: [max_api_path]u8 = undefined;
    var right_buf: [max_api_path]u8 = undefined;
    const raw_left = copyZ(left_ptr, left_buf[0..]) orelse return path_names_equal_error_invalid;
    const raw_right = copyZ(right_ptr, right_buf[0..]) orelse return path_names_equal_error_invalid;

    var left_resolved: [max_api_path]u8 = undefined;
    var right_resolved: [max_api_path]u8 = undefined;
    const left = resolveTarget(raw_left, &left_resolved) orelse return path_names_equal_error_bad_path;
    const right = resolveTarget(raw_right, &right_resolved) orelse return path_names_equal_error_bad_path;
    if (isRootPath(left.path) or isRootPath(right.path)) return path_names_equal_error_bad_path;

    const left_volume = targetVolume(left) orelse return path_names_equal_error_unsupported;
    const right_volume = targetVolume(right) orelse return path_names_equal_error_unsupported;
    // Collation is a property of ONE volume; comparing across volumes has no
    // defined answer and must not be faked.
    if (!vfs.sameVolume(left_volume, right_volume)) return path_names_equal_error_not_same_volume;

    // Different parents can never be the same object, whatever the collation
    // says about the basenames.
    if (!stdMemEql(parentPath(left.path), parentPath(right.path)))
        return path_names_equal_result_different;

    const equal = vfs.namesEqualCollated(
        left_volume,
        baseName(left.path),
        baseName(right.path),
    ) orelse return path_names_equal_error_invalid;
    return if (equal) path_names_equal_result_equal else path_names_equal_result_different;
}

/// Per-payload checked SYSUPD cleanup under exactly one filesystem-request
/// gate (0.60.23).
///
/// Target, stage, current backup and previous backup are verified and acted
/// upon inside a single gate, so a local mutation can no longer slip between
/// a preflight and the deletion it authorized.
pub fn fileUpdateCleanupChecked(
    target_ptr: [*:0]const u8,
    staged_ptr: [*:0]const u8,
    backup_ptr: [*:0]const u8,
    previous_ptr: [*:0]const u8,
    new_size: u64,
    new_checksum: u32,
    old_size: u64,
    old_checksum: u32,
    previous_size: u64,
    previous_checksum: u32,
    flags: u32,
) callconv(.c) i32 {
    if ((flags & ~file_update_cleanup_checked_supported_flags) != 0)
        return file_update_cleanup_checked_error_invalid;
    const target_existed = (flags & file_update_cleanup_checked_flag_target_existed) != 0;
    const old_known = (flags & file_update_cleanup_checked_flag_old_known) != 0;
    const previous_known = (flags & file_update_cleanup_checked_flag_previous_known) != 0;
    if (old_known and !target_existed) return file_update_cleanup_checked_error_invalid;

    var target_buf: [max_api_path]u8 = undefined;
    var staged_buf: [max_api_path]u8 = undefined;
    var backup_buf: [max_api_path]u8 = undefined;
    var previous_buf: [max_api_path]u8 = undefined;
    const raw_target = copyZ(target_ptr, target_buf[0..]) orelse
        return file_update_cleanup_checked_error_invalid;
    const raw_staged = copyZ(staged_ptr, staged_buf[0..]) orelse
        return file_update_cleanup_checked_error_invalid;
    const raw_backup = copyZ(backup_ptr, backup_buf[0..]) orelse
        return file_update_cleanup_checked_error_invalid;
    const raw_previous = copyZ(previous_ptr, previous_buf[0..]) orelse
        return file_update_cleanup_checked_error_invalid;

    var target_resolved: [max_api_path]u8 = undefined;
    var staged_resolved: [max_api_path]u8 = undefined;
    var backup_resolved: [max_api_path]u8 = undefined;
    var previous_resolved: [max_api_path]u8 = undefined;
    const target = resolveTarget(raw_target, &target_resolved) orelse
        return file_update_cleanup_checked_error_bad_path;
    const staged = resolveTarget(raw_staged, &staged_resolved) orelse
        return file_update_cleanup_checked_error_bad_path;
    const backup = resolveTarget(raw_backup, &backup_resolved) orelse
        return file_update_cleanup_checked_error_bad_path;
    if (isRootPath(target.path) or isRootPath(staged.path) or isRootPath(backup.path))
        return file_update_cleanup_checked_error_bad_path;

    const volume = targetVolume(target) orelse return file_update_cleanup_checked_error_unsupported;
    const staged_volume = targetVolume(staged) orelse return file_update_cleanup_checked_error_unsupported;
    const backup_volume = targetVolume(backup) orelse return file_update_cleanup_checked_error_unsupported;
    if (!vfs.sameVolume(volume, staged_volume) or !vfs.sameVolume(volume, backup_volume))
        return file_update_cleanup_checked_error_not_atomic;

    const target_parent = parentPath(target.path);
    if (!stdMemEql(target_parent, parentPath(staged.path)) or
        !stdMemEql(target_parent, parentPath(backup.path)))
    {
        return file_update_cleanup_checked_error_not_atomic;
    }

    // An empty previous-backup path means "nothing inherited to rotate".
    var previous_name: []const u8 = &[_]u8{};
    if (raw_previous.len != 0) {
        const previous = resolveTarget(raw_previous, &previous_resolved) orelse
            return file_update_cleanup_checked_error_bad_path;
        if (isRootPath(previous.path)) return file_update_cleanup_checked_error_bad_path;
        const previous_volume = targetVolume(previous) orelse
            return file_update_cleanup_checked_error_unsupported;
        if (!vfs.sameVolume(volume, previous_volume))
            return file_update_cleanup_checked_error_not_atomic;
        if (!stdMemEql(target_parent, parentPath(previous.path)))
            return file_update_cleanup_checked_error_not_atomic;
        previous_name = baseName(previous.path);
    }

    const target_name = baseName(target.path);
    const staged_name = baseName(staged.path);
    const backup_name = baseName(backup.path);
    if (!vfs.validateShortName83(staged_name) or !vfs.validateShortName83(backup_name))
        return file_update_cleanup_checked_error_not_atomic;

    var req = fs_request.begin(.file_update_atomic_checked, target.drive_ref.letter) orelse
        return file_update_cleanup_checked_error_io;
    var ok = false;
    defer fs_request.finish(&req, ok);

    var parent: vfs.NodeRef = undefined;
    switch (vfs.resolvePathStatus(volume, target_parent, &parent)) {
        .found => {},
        .not_found => return file_update_cleanup_checked_error_bad_path,
        .io => return file_update_cleanup_checked_error_io,
    }
    // A live create-only publish lease owns its tuple; checked cleanup must
    // never consume a member of it (same rule as the forward transfer).
    if (hasPublishStreamCollisionForResolved(
        target.drive_ref.letter,
        targetOnBootVolume(target),
        parent,
        null,
        target_name,
    ) or hasPublishStreamCollisionForResolved(
        backup.drive_ref.letter,
        targetOnBootVolume(backup),
        parent,
        null,
        backup_name,
    )) return file_update_cleanup_checked_error_io;

    const result = system_update_atomic.cleanupPayload(
        volume,
        parent,
        target_name,
        staged_name,
        backup_name,
        previous_name,
        .{
            .new_size = new_size,
            .new_checksum = new_checksum,
            .target_existed = target_existed,
            .old_known = old_known,
            .old_size = old_size,
            .old_checksum = old_checksum,
            .previous_known = previous_known,
            .previous_size = previous_size,
            .previous_checksum = previous_checksum,
        },
    );
    ok = result == .ok;
    return switch (result) {
        .ok => file_update_cleanup_checked_result_ok,
        .conflict => file_update_cleanup_checked_error_conflict,
        .io => file_update_cleanup_checked_error_io,
        .not_atomic => file_update_cleanup_checked_error_not_atomic,
    };
}

/// Exact path-bound execution query used by SYSUPD's live activation gate.
/// The path resolver canonicalizes drive and separators before the program
/// registry provider compares it with the immutable launch origin.
pub fn programModuleRunning(module_path_ptr: [*:0]const u8) callconv(.c) i32 {
    var path_buf: [max_api_path]u8 = undefined;
    const raw_path = copyZ(module_path_ptr, path_buf[0..]) orelse
        return program_module_running_error_invalid;
    var resolved_buf: [max_api_path]u8 = undefined;
    const target = resolveTarget(raw_path, &resolved_buf) orelse
        return program_module_running_error_invalid;
    if (isRootPath(target.path)) return program_module_running_error_invalid;
    const provider = program_module_running_provider orelse
        return program_module_running_error_unavailable;
    return provider(target.drive_ref.letter, target.path);
}

pub fn dirCreate(path_ptr: [*:0]const u8) callconv(.c) i32 {
    var path_buf: [max_api_path]u8 = undefined;
    const raw_path = copyZ(path_ptr, path_buf[0..]) orelse return -1;
    var resolved_buf: [max_api_path]u8 = undefined;
    const target = resolveTarget(raw_path, &resolved_buf) orelse return -1;
    const volume = targetVolume(target) orelse return -2;
    if (isRootPath(target.path)) return -3;
    var req = fs_request.begin(.dir_create, target.drive_ref.letter) orelse return -5;
    var ok = false;
    defer fs_request.finish(&req, ok);
    var parent: vfs.NodeRef = undefined;
    if (vfs.resolvePathStatus(volume, parentPath(target.path), &parent) != .found) return -4;
    var existing: ?vfs.Entry = null;
    switch (resolveOptionalEntryStatus(volume, target.path, &existing)) {
        .found => {
            if (!existing.?.isDir()) return -6;
            ok = true;
            return 0;
        },
        .not_found => {},
        .io => return -6,
    }
    if (!invalidateStreamSlotsForResolved(
        target.drive_ref.letter,
        targetOnBootVolume(target),
        parent,
        existing,
        baseName(target.path),
    )) return -6;
    const created = vfs.makeDirectory(volume, parent, baseName(target.path));
    if (!created) {
        var after: vfs.Entry = undefined;
        switch (vfs.resolveEntryStatus(volume, target.path, &after)) {
            .found => {
                if (!after.isDir()) return -6;
                ok = true;
                return 1;
            },
            .not_found, .io => return -6,
        }
    }
    ok = true;
    return 1;
}

pub fn dirDelete(path_ptr: [*:0]const u8) callconv(.c) i32 {
    var path_buf: [max_api_path]u8 = undefined;
    const raw_path = copyZ(path_ptr, path_buf[0..]) orelse return -1;
    var resolved_buf: [max_api_path]u8 = undefined;
    const target = resolveTarget(raw_path, &resolved_buf) orelse return -1;
    const volume = targetVolume(target) orelse return -2;
    if (isRootPath(target.path)) return -3;
    var req = fs_request.begin(.dir_delete, target.drive_ref.letter) orelse return -5;
    var ok = false;
    defer fs_request.finish(&req, ok);
    var parent: vfs.NodeRef = undefined;
    if (vfs.resolvePathStatus(volume, parentPath(target.path), &parent) != .found) return -4;
    var existing: vfs.Entry = undefined;
    switch (vfs.resolveEntryStatus(volume, target.path, &existing)) {
        .found => {
            if (!existing.isDir()) return -4;
        },
        .not_found => {
            ok = true;
            return 0;
        },
        .io => return -6,
    }
    // Compare the resolved directory identity with every slot's captured
    // ancestor chain. The path check remains a conservative fallback, but is
    // no longer the proof and cannot be bypassed through a FAT 8.3 alias.
    if (hasActiveStreamInSubtree(
        target.drive_ref.letter,
        targetOnBootVolume(target),
        target.path,
        existing,
    )) return -5;
    const removed = vfs.removeDirectory(volume, parent, baseName(target.path));
    if (!removed) {
        var after: vfs.Entry = undefined;
        switch (vfs.resolveEntryStatus(volume, target.path, &after)) {
            .not_found => {
                ok = true;
                return 1;
            },
            .found, .io => return -6,
        }
    }
    ok = true;
    return 1;
}

pub fn fileRename(old_ptr: [*:0]const u8, new_ptr: [*:0]const u8) callconv(.c) i32 {
    var old_buf: [max_api_path]u8 = undefined;
    var new_buf: [max_api_path]u8 = undefined;
    const raw_old = copyZ(old_ptr, old_buf[0..]) orelse return -1;
    const raw_new = copyZ(new_ptr, new_buf[0..]) orelse return -1;
    var old_resolved: [max_api_path]u8 = undefined;
    var new_resolved: [max_api_path]u8 = undefined;
    const old_target = resolveTarget(raw_old, &old_resolved) orelse return -1;
    const new_target = resolveTarget(raw_new, &new_resolved) orelse return -1;
    const volume = targetVolume(old_target) orelse return -2;
    if (targetVolume(new_target) == null) return -2;
    if (old_target.drive_ref.letter != new_target.drive_ref.letter) return -3;
    if (isRootPath(old_target.path) or isRootPath(new_target.path)) return -4;
    const old_parent_path = parentPath(old_target.path);
    const new_parent_path = parentPath(new_target.path);
    if (!stdMemEql(old_parent_path, new_parent_path)) return -5;
    var req = fs_request.begin(.file_rename, old_target.drive_ref.letter) orelse return -7;
    var ok = false;
    defer fs_request.finish(&req, ok);
    var parent: vfs.NodeRef = undefined;
    switch (vfs.resolvePathStatus(volume, old_parent_path, &parent)) {
        .found => {},
        .not_found => return -6,
        .io => return -8,
    }
    var old_entry: ?vfs.Entry = null;
    var new_entry: ?vfs.Entry = null;
    const old_status = resolveOptionalEntryStatus(volume, old_target.path, &old_entry);
    const new_status = resolveOptionalEntryStatus(volume, new_target.path, &new_entry);
    if (old_status == .io or new_status == .io) return -8;
    if (old_status == .not_found) {
        ok = true;
        return 0;
    }
    if (old_entry != null and old_entry.?.isDir() and
        hasActiveStreamInSubtree(
            old_target.drive_ref.letter,
            targetOnBootVolume(old_target),
            old_target.path,
            old_entry.?,
        ))
    {
        // A directory containing an active stream cannot be re-keyed without
        // redirecting that stream's later path-based operations.
        return -7;
    }
    if (new_entry != null and new_entry.?.isDir() and
        hasActiveStreamInSubtree(
            new_target.drive_ref.letter,
            targetOnBootVolume(new_target),
            new_target.path,
            new_entry.?,
        ))
    {
        // Also protect a pre-existing destination directory should a backend
        // ever support replacement rather than reporting a name conflict.
        return -7;
    }
    if (!invalidateStreamSlotsForResolved(
        old_target.drive_ref.letter,
        targetOnBootVolume(old_target),
        parent,
        old_entry,
        baseName(old_target.path),
    )) return -8;
    if (!invalidateStreamSlotsForResolved(
        new_target.drive_ref.letter,
        targetOnBootVolume(new_target),
        parent,
        new_entry,
        baseName(new_target.path),
    )) return -8;
    switch (vfs.renameEntryStatus(volume, parent, baseName(old_target.path), baseName(new_target.path))) {
        .ok => {
            ok = true;
            return 1;
        },
        .not_found => {
            ok = true;
            return 0;
        },
        .conflict => {
            ok = true;
            return 0;
        },
        .io => return -8,
        .not_atomic => {},
    }

    // Compatibility fallback is permitted only after the backend has proven
    // that no rename mutation was attempted. An I/O/flush result above is an
    // ambiguous completion and must never enter Copy/Delete.
    var entry: vfs.Entry = undefined;
    switch (vfs.resolveEntryStatus(volume, old_target.path, &entry)) {
        .found => {},
        .not_found => {
            ok = true;
            return 0;
        },
        .io => return -8,
    }
    if (entry.isDir()) {
        ok = true;
        return 0;
    }
    if (!vfs.copyFileNoReplace(volume, volume, entry, parent, baseName(new_target.path))) {
        ok = true;
        return 0;
    }
    if (!vfs.deleteFile(volume, parent, baseName(old_target.path))) return -8;
    ok = true;
    return 1;
}

/// No-fallback same-directory FAT ownership transfer used by SYSUPD.  Generic
/// FILE RENAME deliberately keeps its compatibility fallback; this API never
/// calls it and reports `not_atomic` for every unsupported shape.
pub fn fileReplaceAtomic(target_ptr: [*:0]const u8, staged_ptr: [*:0]const u8, backup_ptr: [*:0]const u8, flags: u32) callconv(.c) i32 {
    if (flags == 0 or (flags & ~file_replace_atomic_supported_flags) != 0) return file_replace_atomic_error_invalid;
    const consume_stage = (flags & file_replace_atomic_flag_consume_stage) != 0;
    const require_target_absent = (flags & file_replace_atomic_flag_require_target_absent) != 0;
    const require_owned_stage = (flags & file_replace_atomic_flag_require_owned_stage) != 0;
    if (require_target_absent and !consume_stage)
        return file_replace_atomic_error_invalid;
    if (require_owned_stage and (!consume_stage or !require_target_absent))
        return file_replace_atomic_error_invalid;

    var target_buf: [max_api_path]u8 = undefined;
    var staged_buf: [max_api_path]u8 = undefined;
    var backup_buf: [max_api_path]u8 = undefined;
    const raw_target = copyZ(target_ptr, target_buf[0..]) orelse return file_replace_atomic_error_invalid;
    const raw_staged = copyZ(staged_ptr, staged_buf[0..]) orelse return file_replace_atomic_error_invalid;
    const raw_backup = copyZ(backup_ptr, backup_buf[0..]) orelse return file_replace_atomic_error_invalid;

    var target_resolved: [max_api_path]u8 = undefined;
    var staged_resolved: [max_api_path]u8 = undefined;
    var backup_resolved: [max_api_path]u8 = undefined;
    const target = resolveTarget(raw_target, &target_resolved) orelse return file_replace_atomic_error_bad_path;
    const staged = resolveTarget(raw_staged, &staged_resolved) orelse return file_replace_atomic_error_bad_path;
    const backup = resolveTarget(raw_backup, &backup_resolved) orelse return file_replace_atomic_error_bad_path;
    const volume = targetVolume(target) orelse return file_replace_atomic_error_unsupported;
    const staged_volume = targetVolume(staged) orelse return file_replace_atomic_error_unsupported;
    const backup_volume = targetVolume(backup) orelse return file_replace_atomic_error_unsupported;
    if (!vfs.sameVolume(volume, staged_volume) or !vfs.sameVolume(volume, backup_volume))
        return file_replace_atomic_error_not_atomic;
    if (isRootPath(target.path) or isRootPath(staged.path) or isRootPath(backup.path)) return file_replace_atomic_error_bad_path;

    const target_parent = parentPath(target.path);
    if (!stdMemEql(target_parent, parentPath(staged.path)) or !stdMemEql(target_parent, parentPath(backup.path))) return file_replace_atomic_error_not_atomic;
    const target_name = baseName(target.path);
    const staged_name = baseName(staged.path);
    const backup_name = baseName(backup.path);
    if (!vfs.validateShortName83(staged_name) or !vfs.validateShortName83(backup_name)) return file_replace_atomic_error_not_atomic;

    var req = fs_request.begin(.file_replace_atomic, target.drive_ref.letter) orelse return file_replace_atomic_error_io;
    var ok = false;
    defer fs_request.finish(&req, ok);
    // An atomic publish only invalidates streams whose exact names it owns.
    // Clearing every slot here broke unrelated concurrent SSH uploads.
    var parent: vfs.NodeRef = undefined;
    switch (vfs.resolvePathStatus(volume, target_parent, &parent)) {
        .found => {},
        .not_found => return file_replace_atomic_error_bad_path,
        .io => return file_replace_atomic_error_io,
    }
    var target_entry: ?vfs.Entry = null;
    var staged_entry: ?vfs.Entry = null;
    var backup_entry: ?vfs.Entry = null;
    const target_entry_status = resolveOptionalEntryStatus(volume, target.path, &target_entry);
    const staged_entry_status = resolveOptionalEntryStatus(volume, staged.path, &staged_entry);
    const backup_entry_status = resolveOptionalEntryStatus(volume, backup.path, &backup_entry);

    var owned_stage_slot: ?*StreamSlot = null;
    var owned_publish_resume = false;
    if (require_owned_stage) {
        // A resumed NTFS publication can intentionally make the normal stage
        // lookup ambiguous while target and stage are aliases. Slot lookup by
        // the still-private stage name remains safe because ownership also
        // binds the exact caller task/process generation.
        const slot = findOwnedStreamSlotForResolved(
            staged.drive_ref.letter,
            targetOnBootVolume(staged),
            parent,
            if (staged_entry_status == .found) staged_entry else null,
            staged_name,
        ) orelse return file_replace_atomic_error_conflict;
        if (!slot.ready or !slot.finished)
            return file_replace_atomic_error_conflict;
        if (slot.publish_started) {
            if (!streamPublishTupleMatches(slot, target_name, backup_name))
                return file_replace_atomic_error_conflict;
            owned_publish_resume = true;
        } else {
            // First publication must establish absence and the exact live
            // stage identity in this gate interval. Only a previously started
            // publication may use the transient recovery view below.
            if (target_entry_status == .io or
                staged_entry_status == .io or
                backup_entry_status == .io)
                return file_replace_atomic_error_io;
            if (target_entry != null or backup_entry != null)
                return file_replace_atomic_error_conflict;
            const first_stage = staged_entry orelse return file_replace_atomic_error_not_found;
            if (slot.size != first_stage.size)
                return file_replace_atomic_error_conflict;
            if (slot.publish_declared and
                !streamPublishTupleNamesMatch(slot, target_name, backup_name))
                return file_replace_atomic_error_conflict;
        }
        owned_stage_slot = slot;
    } else if (require_target_absent) {
        // Unowned compatibility callers do not possess a replay token.
        if (target_entry_status == .io or
            staged_entry_status == .io or
            backup_entry_status == .io)
            return file_replace_atomic_error_io;
        if (target_entry != null) return file_replace_atomic_error_conflict;
    }

    // The owned slot may resume its own exact tuple, but no publication may
    // enter names or identities reserved by another in-flight upload.
    const ignored_publish_slot: ?*const StreamSlot = if (owned_stage_slot) |slot| slot else null;
    if (hasPublishStreamCollisionForResolvedExcept(
        target.drive_ref.letter,
        targetOnBootVolume(target),
        parent,
        if (target_entry_status == .found) target_entry else null,
        target_name,
        ignored_publish_slot,
    ) or hasPublishStreamCollisionForResolvedExcept(
        staged.drive_ref.letter,
        targetOnBootVolume(staged),
        parent,
        if (staged_entry_status == .found) staged_entry else null,
        staged_name,
        ignored_publish_slot,
    ) or hasPublishStreamCollisionForResolvedExcept(
        backup.drive_ref.letter,
        targetOnBootVolume(backup),
        parent,
        if (backup_entry_status == .found) backup_entry else null,
        backup_name,
        ignored_publish_slot,
    )) return file_replace_atomic_error_io;

    // Never derive absence or identity from an ambiguous generic lookup.
    // A normal alias-state replay has already invalidated these slots before
    // the attempt which created the half-state (or runs after reboot, where
    // no StreamSlot survives).
    if (!require_owned_stage) {
        if (target_entry_status != .io)
            if (!invalidateStreamSlotsForResolved(target.drive_ref.letter, targetOnBootVolume(target), parent, target_entry, target_name))
                return file_replace_atomic_error_io;
        if (backup_entry_status != .io)
            if (!invalidateStreamSlotsForResolved(backup.drive_ref.letter, targetOnBootVolume(backup), parent, backup_entry, backup_name))
                return file_replace_atomic_error_io;
        if (staged_entry_status != .io)
            if (!invalidateStreamSlotsForResolved(staged.drive_ref.letter, targetOnBootVolume(staged), parent, staged_entry, staged_name))
                return file_replace_atomic_error_io;
    }

    var result: vfs.AtomicReplaceResult = undefined;
    if (owned_stage_slot) |slot| {
        // 0.60.22: the durable claim must exist BEFORE the publish can cross
        // its first visibility point.  Without it a reset inside the
        // hand-over would leave an object no later boot can resolve, so a
        // failure here refuses the publish instead of proceeding blind.
        // A resume already owns its claim and must not mint a second one.
        // Since 0.60.30 a declared intent already wrote the claim at
        // stream-begin, so that case must not mint one either.
        if (!owned_publish_resume and slot.publish_claim_generation == 0) {
            if (staged_entry_status != .found) return file_replace_atomic_error_io;
            const claim_parent_path = buildClaimParentPath(target.drive_ref.letter, target.path) orelse
                return file_replace_atomic_error_io;
            const claim = upload_claim_store.beginPublish(
                volume,
                claim_parent_path,
                staged_name,
                target_name,
                backup_name,
                claimIdentityFromEntry(staged_entry.?),
                .storage,
            );
            if (!claim.ok) return file_replace_atomic_error_io;
            slot.publish_claim_generation = claim.generation;
        }
        if (!owned_publish_resume and !rememberStreamPublishTuple(slot, target_name, backup_name)) {
            if (slot.publish_claim_generation != 0 and
                upload_claim_store.retire(slot.publish_claim_generation))
            {
                slot.publish_claim_generation = 0;
            }
            return file_replace_atomic_error_not_atomic;
        }
        result = runOwnedCreateOnlyPublish(volume, parent, target_name, staged_name, backup_name, slot);
        // Only a terminal hand-over may drop the recovery token.
        if (result == .ok and slot.publish_claim_generation != 0) {
            if (upload_claim_store.retire(slot.publish_claim_generation)) {
                slot.publish_claim_generation = 0;
            } else {
                result = .io;
            }
        }
    } else {
        result = if (require_target_absent)
            vfs.replaceFileAtomicCreateOnly(volume, parent, target_name, staged_name, backup_name)
        else
            vfs.replaceFileAtomic(volume, parent, target_name, staged_name, backup_name, consume_stage);
    }
    if (result == .ok) {
        if (owned_stage_slot) |slot| clearStreamSlot(slot);
    }
    ok = result == .ok;
    return switch (result) {
        .ok => file_replace_atomic_result_ok,
        .invalid => file_replace_atomic_error_invalid,
        .not_found => file_replace_atomic_error_not_found,
        .alias => file_replace_atomic_error_alias,
        .conflict, .read_only => file_replace_atomic_error_conflict,
        .io => file_replace_atomic_error_io,
        .not_atomic => file_replace_atomic_error_not_atomic,
    };
}

pub fn fileCopy(src_ptr: [*:0]const u8, dst_ptr: [*:0]const u8) callconv(.c) i32 {
    var src_buf: [max_api_path]u8 = undefined;
    var dst_buf: [max_api_path]u8 = undefined;
    const raw_src = copyZ(src_ptr, src_buf[0..]) orelse return -1;
    const raw_dst = copyZ(dst_ptr, dst_buf[0..]) orelse return -1;
    var src_resolved: [max_api_path]u8 = undefined;
    var dst_resolved: [max_api_path]u8 = undefined;
    const src_target = resolveTarget(raw_src, &src_resolved) orelse return -1;
    const dst_target = resolveTarget(raw_dst, &dst_resolved) orelse return -1;
    const src_volume = targetVolume(src_target) orelse return -2;
    const dst_volume = targetVolume(dst_target) orelse return -2;
    if (isRootPath(src_target.path) or isRootPath(dst_target.path)) return -3;
    if (src_target.drive_ref.letter == dst_target.drive_ref.letter and stdMemEql(src_target.path, dst_target.path)) return -8;

    var req = fs_request.begin(.file_copy, src_target.drive_ref.letter) orelse return -10;
    var ok = false;
    defer fs_request.finish(&req, ok);
    var entry: vfs.Entry = undefined;
    switch (vfs.resolveEntryStatus(src_volume, src_target.path, &entry)) {
        .found => {},
        .not_found => return 0,
        .io => return -9,
    }
    if (entry.isDir()) return -4;
    var dst_parent: vfs.NodeRef = undefined;
    if (vfs.resolvePathStatus(dst_volume, parentPath(dst_target.path), &dst_parent) != .found) return -5;
    var dst_entry: ?vfs.Entry = null;
    if (resolveOptionalEntryStatus(dst_volume, dst_target.path, &dst_entry) == .io) return -9;
    if (!invalidateStreamSlotsForResolved(
        dst_target.drive_ref.letter,
        targetOnBootVolume(dst_target),
        dst_parent,
        dst_entry,
        baseName(dst_target.path),
    )) return -9;
    if (!vfs.copyFile(src_volume, dst_volume, entry, dst_parent, baseName(dst_target.path))) return -9;
    ok = true;
    return 1;
}

pub fn fileMove(src_ptr: [*:0]const u8, dst_ptr: [*:0]const u8) callconv(.c) i32 {
    var src_buf: [max_api_path]u8 = undefined;
    var dst_buf: [max_api_path]u8 = undefined;
    const raw_src = copyZ(src_ptr, src_buf[0..]) orelse return -1;
    const raw_dst = copyZ(dst_ptr, dst_buf[0..]) orelse return -1;
    var src_resolved: [max_api_path]u8 = undefined;
    var dst_resolved: [max_api_path]u8 = undefined;
    const src_target = resolveTarget(raw_src, &src_resolved) orelse return -1;
    const dst_target = resolveTarget(raw_dst, &dst_resolved) orelse return -1;
    const src_volume = targetVolume(src_target) orelse return -2;
    const dst_volume = targetVolume(dst_target) orelse return -2;
    if (isRootPath(src_target.path) or isRootPath(dst_target.path)) return -3;
    if (src_target.drive_ref.letter == dst_target.drive_ref.letter and stdMemEql(src_target.path, dst_target.path)) return -8;

    var req = fs_request.begin(.file_move, src_target.drive_ref.letter) orelse return -12;
    var ok = false;
    defer fs_request.finish(&req, ok);
    var entry: vfs.Entry = undefined;
    switch (vfs.resolveEntryStatus(src_volume, src_target.path, &entry)) {
        .found => {},
        .not_found => return 0,
        .io => return -9,
    }
    if (entry.isDir()) return -4;
    if (entry.isReadOnly()) return -11;
    var dst_parent: vfs.NodeRef = undefined;
    var src_parent: vfs.NodeRef = undefined;
    if (vfs.resolvePathStatus(dst_volume, parentPath(dst_target.path), &dst_parent) != .found) return -5;
    if (vfs.resolvePathStatus(src_volume, parentPath(src_target.path), &src_parent) != .found) return -6;
    var dst_entry: ?vfs.Entry = null;
    if (resolveOptionalEntryStatus(dst_volume, dst_target.path, &dst_entry) == .io) return -9;
    if (!invalidateStreamSlotsForResolved(
        src_target.drive_ref.letter,
        targetOnBootVolume(src_target),
        src_parent,
        entry,
        baseName(src_target.path),
    )) return -9;
    if (!invalidateStreamSlotsForResolved(
        dst_target.drive_ref.letter,
        targetOnBootVolume(dst_target),
        dst_parent,
        dst_entry,
        baseName(dst_target.path),
    )) return -9;
    if (!vfs.copyFile(src_volume, dst_volume, entry, dst_parent, baseName(dst_target.path))) return -9;
    if (!vfs.deleteFile(src_volume, src_parent, baseName(src_target.path))) return -10;
    ok = true;
    return 1;
}

fn resolveTarget(raw: []const u8, out: *[max_api_path]u8) ?Target {
    const resolver = path_resolver orelse return null;
    return resolver(raw, out);
}

/// The unlettered FAT32 boot partition is mounted internally (0.60.11):
/// the letterless /boot subtree of the system drive routes to it whenever
/// it exists, so SYSUPD can stage and atomically replace /boot/r4os.elf.
/// Same-parent constraints in rename/replace keep both sides on one
/// volume; copy/move handle the two volumes independently.
fn isBootSubtreePath(path: []const u8) bool {
    if (path.len < 5) return false;
    if (path[0] != '\\' and path[0] != '/') return false;
    const lower = "boot";
    var i: usize = 0;
    while (i < 4) : (i += 1) {
        const ch = path[1 + i];
        const folded = if (ch >= 'A' and ch <= 'Z') ch + 32 else ch;
        if (folded != lower[i]) return false;
    }
    if (path.len == 5) return true;
    return path[5] == '\\' or path[5] == '/';
}

fn targetVolume(target: Target) ?vfs.Volume {
    if (target.drive_ref.role == .system and isBootSubtreePath(target.path)) {
        if (vfs.bootVolume()) |volume| return volume;
    }
    return vfs.volumeForDrive(target.drive_ref.letter);
}

fn resolveOptionalEntryStatus(volume: vfs.Volume, path: []const u8, out: *?vfs.Entry) vfs.LookupStatus {
    var entry: vfs.Entry = undefined;
    const status = vfs.resolveEntryStatus(volume, path, &entry);
    out.* = if (status == .found) entry else null;
    return status;
}

fn targetOnBootVolume(target: Target) bool {
    return target.drive_ref.role == .system and isBootSubtreePath(target.path) and vfs.bootVolume() != null;
}

fn streamSlotVolume(slot: *const StreamSlot) ?vfs.Volume {
    if (slot.on_boot_volume) return vfs.bootVolume();
    return vfs.volumeForDrive(slot.drive_letter);
}

fn copyZ(ptr: [*:0]const u8, out: []u8) ?[]const u8 {
    var len: usize = 0;
    while (len < out.len and ptr[len] != 0) : (len += 1) out[len] = ptr[len];
    if (len == out.len) return null;
    return out[0..len];
}

fn copyFixedZ(out: []u8, value: []const u8) void {
    @memset(out, 0);
    if (out.len == 0) return;
    const count = @min(value.len, out.len - 1);
    if (count > 0) @memcpy(out[0..count], value[0..count]);
}

fn rememberStreamSlot(slot: *StreamSlot, raw_path: []const u8, drive_letter: u8, on_boot_volume: bool, parent_node: vfs.NodeRef, entry_name: []const u8, flags: u32) void {
    if (raw_path.len > max_api_path or entry_name.len > max_api_path) return;
    const owner = currentStreamOwner();
    slot.* = .{
        .active = true,
        .ready = false,
        .finished = false,
        .publish_started = false,
        .raw_len = raw_path.len,
        .drive_letter = drive_letter,
        .on_boot_volume = on_boot_volume,
        .parent_node = parent_node,
        .name_len = entry_name.len,
        .size = 0,
        .flags = flags,
        .generation = nextStreamGeneration(),
        .owner_kind = if (owner) |current| current.kind else .kernel_task,
        .owner_id = if (owner) |current| current.id else 0,
        .owner_generation = if (owner) |current| current.generation else 0,
        .owner_program_id = if (owner) |current| current.program_id else 0,
        .owner_program_generation = if (owner) |current| current.program_generation else 0,
    };
    if (raw_path.len > 0) @memcpy(slot.raw_path[0..raw_path.len], raw_path);
    if (entry_name.len > 0) @memcpy(slot.name[0..entry_name.len], entry_name);
}

/// Captures the physical directory chain represented by `parent_path`.
/// Lexical "."/".." components are normalized before each prefix is resolved,
/// so traversed-but-popped directories do not become false ancestors.  Entry
/// resolution preserves FAT long/short-name aliasing and NTFS record
/// generations. The caller holds the global filesystem request gate.
fn bindStreamSlotAncestors(
    slot: *StreamSlot,
    volume: vfs.Volume,
    parent_path: []const u8,
    expected_parent: vfs.NodeRef,
) bool {
    slot.ancestor_count = 0;
    var normalized: [max_api_path]u8 = undefined;
    normalized[0] = '\\';
    var normalized_len: usize = 1;
    var prefix_ends: [max_stream_ancestors]usize = undefined;
    var scan: usize = 0;

    while (scan < parent_path.len) {
        while (scan < parent_path.len and isPathSeparator(parent_path[scan])) : (scan += 1) {}
        if (scan >= parent_path.len) break;
        var end = scan;
        while (end < parent_path.len and !isPathSeparator(parent_path[end])) : (end += 1) {}
        const component = parent_path[scan..end];
        scan = end;

        if (component.len == 1 and component[0] == '.') continue;
        if (component.len == 2 and component[0] == '.' and component[1] == '.') {
            if (slot.ancestor_count != 0) {
                slot.ancestor_count -= 1;
                slot.ancestor_nodes[slot.ancestor_count] = 0;
                slot.ancestor_generations[slot.ancestor_count] = 0;
                normalized_len = if (slot.ancestor_count == 0)
                    1
                else
                    prefix_ends[slot.ancestor_count - 1];
            }
            continue;
        }
        if (slot.ancestor_count >= max_stream_ancestors) return false;
        if (normalized_len > 1) {
            if (normalized_len >= normalized.len) return false;
            normalized[normalized_len] = '\\';
            normalized_len += 1;
        }
        if (component.len > normalized.len - normalized_len) return false;
        @memcpy(normalized[normalized_len .. normalized_len + component.len], component);
        normalized_len += component.len;
        prefix_ends[slot.ancestor_count] = normalized_len;
        slot.ancestor_count += 1;
    }

    if (slot.ancestor_count == 0) return expected_parent == volume.rootNode();
    // Resolve each component exactly once from its already-proven parent.
    // Re-resolving every growing prefix would turn a deep Begin into quadratic
    // directory walks while holding the global filesystem gate.
    var current_parent = volume.rootNode();
    var index: usize = 0;
    while (index < slot.ancestor_count) : (index += 1) {
        const component_start = if (index == 0) 1 else prefix_ends[index - 1] + 1;
        const component_end = prefix_ends[index];
        if (component_start >= component_end or component_end > normalized_len) return false;
        var entry: vfs.Entry = undefined;
        switch (vfs.lookupEntryStatus(
            volume,
            current_parent,
            normalized[component_start..component_end],
            &entry,
        )) {
            .found => {},
            .not_found, .io => return false,
        }
        if (!entry.isDir() or entry.reparse or entry.node == 0) return false;
        slot.ancestor_nodes[index] = entry.node;
        slot.ancestor_generations[index] = entry.node_generation;
        current_parent = entry.node;
    }
    return current_parent == expected_parent;
}

fn bindStreamSlotToEntry(slot: *StreamSlot, entry: vfs.Entry) void {
    slot.file_node = entry.node;
    slot.file_node_generation = entry.node_generation;
    slot.has_file_node = true;
    slot.identity_refresh_pending = false;
    slot.file_attr = entry.attr;
    slot.file_reparse = entry.reparse;
    slot.file_created_time = entry.created_time;
    slot.file_created_date = entry.created_date;
    slot.file_access_date = entry.access_date;
    slot.file_modified_time = entry.modified_time;
    slot.file_modified_date = entry.modified_date;
    slot.name_len = @min(entry.name_len, slot.name.len);
    if (slot.name_len > 0) @memcpy(slot.name[0..slot.name_len], entry.name[0..slot.name_len]);
}

fn streamSlotOwnsPublishedEntry(slot: *const StreamSlot, entry: vfs.Entry) bool {
    if (!slot.has_file_node or entry.isDir() or entry.size != slot.size) return false;
    if (slot.file_node != 0 or
        slot.file_node_generation != 0 or
        entry.node != 0 or
        entry.node_generation != 0)
    {
        return slot.file_node == entry.node and
            slot.file_node_generation == entry.node_generation;
    }
    // FAT represents an empty file as node 0. Its complete directory-entry
    // fingerprint plus the live create-only lease is the strongest available
    // ownership token. Every R4SYS namespace mutation also invalidates the
    // recorded publish target name before changing it.
    return streamSlotEntryFingerprintMatches(slot, entry);
}

fn streamSlotEntryFingerprintMatches(slot: *const StreamSlot, entry: vfs.Entry) bool {
    return slot.has_file_node and
        slot.file_node == entry.node and
        slot.file_node_generation == entry.node_generation and
        slot.size == entry.size and
        slot.file_attr == entry.attr and
        slot.file_reparse == entry.reparse and
        slot.file_created_time == entry.created_time and
        slot.file_created_date == entry.created_date and
        slot.file_access_date == entry.access_date and
        slot.file_modified_time == entry.modified_time and
        slot.file_modified_date == entry.modified_date;
}

fn rememberStreamPublishTuple(slot: *StreamSlot, target_name: []const u8, backup_name: []const u8) bool {
    if (target_name.len == 0 or target_name.len > slot.publish_target_name.len or
        backup_name.len == 0 or backup_name.len > slot.publish_backup_name.len)
        return false;
    @memset(slot.publish_target_name[0..], 0);
    @memset(slot.publish_backup_name[0..], 0);
    @memcpy(slot.publish_target_name[0..target_name.len], target_name);
    @memcpy(slot.publish_backup_name[0..backup_name.len], backup_name);
    slot.publish_target_name_len = target_name.len;
    slot.publish_backup_name_len = backup_name.len;
    slot.publish_started = true;
    return true;
}

fn streamPublishTupleMatches(slot: *const StreamSlot, target_name: []const u8, backup_name: []const u8) bool {
    return slot.publish_started and streamPublishTupleNamesMatch(slot, target_name, backup_name);
}

fn streamPublishTupleNamesMatch(slot: *const StreamSlot, target_name: []const u8, backup_name: []const u8) bool {
    return slot.publish_target_name_len == target_name.len and
        slot.publish_backup_name_len == backup_name.len and
        asciiEqlIgnoreCase(slot.publish_target_name[0..slot.publish_target_name_len], target_name) and
        asciiEqlIgnoreCase(slot.publish_backup_name[0..slot.publish_backup_name_len], backup_name);
}

fn streamPublishTargetName(slot: *const StreamSlot) []const u8 {
    return slot.publish_target_name[0..slot.publish_target_name_len];
}

fn streamPublishBackupName(slot: *const StreamSlot) []const u8 {
    return slot.publish_backup_name[0..slot.publish_backup_name_len];
}

fn sameOwnedPublishedEntry(volume: vfs.Volume, slot: *const StreamSlot, left: vfs.Entry, right: vfs.Entry) bool {
    if (!streamSlotOwnsPublishedEntry(slot, left) or !streamSlotOwnsPublishedEntry(slot, right))
        return false;
    if (vfs.sameFileIdentity(volume, left, right)) return true;
    // Empty FAT aliases have cluster zero. They were created from the exact
    // staged directory entry and therefore carry the same complete metadata
    // fingerprint while this publish lease remains live.
    return left.node == 0 and
        right.node == 0 and
        left.node_generation == 0 and
        right.node_generation == 0 and
        left.size == 0 and
        right.size == 0 and
        left.attr == right.attr and
        left.reparse == right.reparse and
        left.created_time == right.created_time and
        left.created_date == right.created_date and
        left.access_date == right.access_date and
        left.modified_time == right.modified_time and
        left.modified_date == right.modified_date;
}

fn inspectOwnedCreateOnlyState(
    volume: vfs.Volume,
    parent: vfs.NodeRef,
    target_name: []const u8,
    staged_name: []const u8,
    backup_name: []const u8,
    slot: *const StreamSlot,
) OwnedCreateOnlyState {
    var target_entry: vfs.Entry = undefined;
    var staged_entry: vfs.Entry = undefined;
    var backup_entry: vfs.Entry = undefined;
    const target_status = vfs.lookupRecoveryEntryStatus(volume, parent, target_name, &target_entry);
    const staged_status = vfs.lookupRecoveryEntryStatus(volume, parent, staged_name, &staged_entry);
    const backup_status = vfs.lookupRecoveryEntryStatus(volume, parent, backup_name, &backup_entry);
    if (target_status == .io or staged_status == .io or backup_status == .io) return .io;
    if (backup_status == .found) return .conflict;

    if (target_status == .found and !streamSlotOwnsPublishedEntry(slot, target_entry))
        return .conflict;
    if (staged_status == .found and !streamSlotOwnsPublishedEntry(slot, staged_entry))
        return .conflict;

    if (target_status == .found and staged_status == .found) {
        return if (sameOwnedPublishedEntry(volume, slot, target_entry, staged_entry))
            .aliased
        else
            .conflict;
    }
    if (target_status == .found) return .target_only;
    if (staged_status == .found) return .stage_only;
    return .missing;
}

fn runOwnedCreateOnlyPublish(
    volume: vfs.Volume,
    parent: vfs.NodeRef,
    target_name: []const u8,
    staged_name: []const u8,
    backup_name: []const u8,
    slot: *const StreamSlot,
) vfs.AtomicReplaceResult {
    var backend_attempts: u8 = 0;
    while (backend_attempts < 2) {
        switch (inspectOwnedCreateOnlyState(volume, parent, target_name, staged_name, backup_name, slot)) {
            .target_only => return if (vfs.flushVolume(volume)) .ok else .io,
            .stage_only, .aliased => {},
            .missing => return .not_found,
            .conflict => return .conflict,
            .io => return .io,
        }
        const result = vfs.replaceFileAtomicCreateOnly(volume, parent, target_name, staged_name, backup_name);
        if (result != .io) return result;
        backend_attempts += 1;
    }

    // A backend can report I/O after detaching the final alias or after an
    // ownership-boundary flush. Re-observe once under the same namespace gate
    // before handing the exact lease back to the caller for a later retry.
    return switch (inspectOwnedCreateOnlyState(volume, parent, target_name, staged_name, backup_name, slot)) {
        .target_only => if (vfs.flushVolume(volume)) .ok else .io,
        .missing => .not_found,
        .conflict => .conflict,
        .stage_only, .aliased, .io => .io,
    };
}

/// Completes only the create-only publication recorded in this exact stream
/// lease. The caller already holds the filesystem-request gate. A terminal
/// target-only or missing state releases the slot; every ambiguous or foreign
/// state keeps the ownership token live for a later retry.
fn settlePublishStreamSlot(slot: *StreamSlot) bool {
    if (!slot.active or !slot.publish_started) return true;
    if (slot.publish_target_name_len == 0 or slot.publish_backup_name_len == 0)
        return false;
    const volume = streamSlotVolume(slot) orelse return false;
    const result = runOwnedCreateOnlyPublish(
        volume,
        slot.parent_node,
        streamPublishTargetName(slot),
        streamSlotName(slot),
        streamPublishBackupName(slot),
        slot,
    );
    return switch (result) {
        .ok, .not_found => terminal: {
            // Terminal: the hand-over either completed or owns nothing any
            // more, so the durable claim may go before the slot is released.
            if (slot.publish_claim_generation != 0 and
                !upload_claim_store.retire(slot.publish_claim_generation))
                break :terminal false;
            clearStreamSlot(slot);
            break :terminal true;
        },
        else => false,
    };
}

fn findStreamSlotForResolved(
    drive_letter: u8,
    on_boot_volume: bool,
    parent_node: vfs.NodeRef,
    entry: ?vfs.Entry,
    entry_name: []const u8,
) ?*StreamSlot {
    var i: usize = 0;
    while (i < stream_slots.len) : (i += 1) {
        const slot = &stream_slots[i];
        if (streamSlotMatchesResolved(slot, drive_letter, on_boot_volume, parent_node, entry, entry_name)) return slot;
    }
    return null;
}

fn findOwnedStreamSlotForResolved(
    drive_letter: u8,
    on_boot_volume: bool,
    parent_node: vfs.NodeRef,
    entry: ?vfs.Entry,
    entry_name: []const u8,
) ?*StreamSlot {
    var i: usize = 0;
    while (i < stream_slots.len) : (i += 1) {
        const slot = &stream_slots[i];
        if (!streamSlotOwnedByCurrentTask(slot)) continue;
        if (streamSlotMatchesResolved(slot, drive_letter, on_boot_volume, parent_node, entry, entry_name)) return slot;
    }
    return null;
}

fn streamSlotMatchesResolved(
    slot: *const StreamSlot,
    drive_letter: u8,
    on_boot_volume: bool,
    parent_node: vfs.NodeRef,
    entry: ?vfs.Entry,
    entry_name: []const u8,
) bool {
    if (!slot.active or
        slot.drive_letter != drive_letter or
        slot.on_boot_volume != on_boot_volume)
        return false;
    if (entry) |resolved| {
        if (!slot.has_file_node) return false;
        // NTFS hardlinks in different directories still name the same live
        // MFT record. Sequence-bearing identities therefore match
        // volume-wide before the parent/name fallback is considered.
        if (slot.has_file_node and
            slot.file_node_generation != 0 and
            resolved.node_generation != 0)
        {
            return slot.file_node == resolved.node and
                slot.file_node_generation == resolved.node_generation;
        }
        if (slot.parent_node != parent_node) return false;
        const same_name = slot.name_len == resolved.name_len and
            asciiEqlIgnoreCase(slot.name[0..slot.name_len], resolved.name[0..resolved.name_len]);
        if (slot.identity_refresh_pending and
            slot.parent_node == parent_node and
            same_name and
            resolved.node_generation == 0 and
            resolved.size == slot.size)
        {
            return true;
        }
        // FAT has no stable record sequence and represents an empty file as
        // first_cluster 0. If the identity refresh after the first successful
        // append was temporarily unreadable, accept exactly the same name and
        // confirmed stream size once so the caller can rebind the new cluster.
        if (slot.has_file_node and
            slot.file_node == 0 and
            slot.file_node_generation == 0 and
            resolved.node_generation == 0)
        {
            return same_name and streamSlotEntryFingerprintMatches(slot, resolved);
        }
        // A bound non-empty FAT cluster or NTFS record+sequence is an exact
        // identity. Never fall back to a matching name after it changes: a
        // direct VFS truncate/replace could otherwise let an old owner write
        // or delete the new occupant. FAT empty files use node zero and must
        // retain the guarded name fallback.
        if (slot.has_file_node and
            (slot.file_node != 0 or slot.file_node_generation != 0 or resolved.node != 0 or resolved.node_generation != 0))
        {
            return slot.file_node == resolved.node and
                slot.file_node_generation == resolved.node_generation;
        }
        return same_name;
    }
    if (slot.parent_node != parent_node) return false;
    return slot.name_len == entry_name.len and
        asciiEqlIgnoreCase(slot.name[0..slot.name_len], entry_name);
}

fn streamSlotOwnedByCurrentTask(slot: *const StreamSlot) bool {
    const current = currentStreamOwner();
    if (current) |owner| {
        if (slot.owner_kind != owner.kind or
            slot.owner_id != owner.id or
            slot.owner_generation != owner.generation)
            return false;
        if (owner.kind == .program) {
            return slot.owner_program_id == owner.program_id and
                slot.owner_program_generation == owner.program_generation;
        }
        return true;
    }
    return slot.owner_id == 0 and slot.owner_generation == 0;
}

fn currentStreamOwner() ?StreamOwner {
    if (stream_owner_resolver) |resolver| {
        if (resolver()) |owner| return owner;
    }
    const current = scheduler.current() orelse return null;
    return .{
        .kind = .kernel_task,
        .id = current.id,
        .generation = current.generation,
    };
}

fn streamSlotOwnerAlive(slot: *const StreamSlot) bool {
    if (slot.owner_id == 0 or slot.owner_generation == 0) return stream_owner_resolver == null and scheduler.current() == null;
    if (slot.owner_kind == .program) {
        // Program owners are cleared by the R4X retire path only after every
        // worker and ProgramThread has drained. Querying the sleepable program
        // registry from inside the filesystem gate would invert lock order.
        return true;
    }
    return sched_task.isAliveIdentity(slot.owner_id, slot.owner_generation);
}

fn reapDeadStreamSlots() void {
    var i: usize = 0;
    while (i < stream_slots.len) : (i += 1) {
        const slot = &stream_slots[i];
        if (!slot.active or streamSlotOwnerAlive(slot)) continue;
        if (slot.publish_started) {
            // The slot is the only in-memory proof that a create-only
            // target/stage alias belongs to this upload. A dead task must not
            // discard that token: finish the exact recorded transition, and
            // retain it for a later gate entry if storage is still ambiguous.
            _ = settlePublishStreamSlot(slot);
            continue;
        }
        // The caller-owned file is deliberately left in place.  A normal
        // partial stream remains visible for diagnosis; an explicit lease
        // opener may reclaim its own transient path above.
        clearStreamSlot(slot);
    }
}

fn freeStreamSlot() ?*StreamSlot {
    var i: usize = 0;
    while (i < stream_slots.len) : (i += 1) {
        if (!stream_slots[i].active) return &stream_slots[i];
    }
    return null;
}

fn nextStreamGeneration() u32 {
    stream_generation +%= 1;
    if (stream_generation == 0) stream_generation = 1;
    return stream_generation;
}

fn streamSlotName(slot: *const StreamSlot) []const u8 {
    return slot.name[0..slot.name_len];
}

fn clearStreamSlot(slot: *StreamSlot) void {
    slot.active = false;
    slot.ready = false;
    slot.finished = false;
    slot.publish_started = false;
    slot.publish_target_name_len = 0;
    slot.publish_backup_name_len = 0;
    // A still-set claim generation here means the durable token outlived its
    // slot.  Dropping only the in-memory reference is correct: the claim
    // stays on disk and pre-runtime recovery drives it to a terminal state.
    slot.publish_claim_generation = 0;
    slot.publish_declared = false;
    slot.publish_declared_parent_len = 0;
    slot.publish_protocol = 0;
    slot.raw_len = 0;
    slot.name_len = 0;
    slot.ancestor_count = 0;
    @memset(slot.ancestor_nodes[0..], 0);
    @memset(slot.ancestor_generations[0..], 0);
    slot.file_node = 0;
    slot.file_node_generation = 0;
    slot.has_file_node = false;
    slot.identity_refresh_pending = false;
    slot.file_attr = 0;
    slot.file_reparse = false;
    slot.file_created_time = 0;
    slot.file_created_date = 0;
    slot.file_access_date = 0;
    slot.file_modified_time = 0;
    slot.file_modified_date = 0;
    slot.size = 0;
    slot.flags = 0;
    slot.generation = 0;
    slot.owner_kind = .kernel_task;
    slot.owner_id = 0;
    slot.owner_generation = 0;
    slot.owner_program_id = 0;
    slot.owner_program_generation = 0;
}

/// R4X calls this after all ProgramThreads and async-I/O workers belonging to
/// the exact process generation have drained. Acquiring the normal FS gate
/// keeps slot reclamation serialized with Begin/Write/Finish/Abort.
pub fn releaseStreamSlotsForProgram(instance_id: u32, instance_generation: u64) bool {
    if (instance_id == 0 or instance_generation == 0) return true;
    var req = fs_request.begin(.stream_abort, 0) orelse return false;
    var all_released = true;
    defer fs_request.finish(&req, all_released);
    var i: usize = 0;
    while (i < stream_slots.len) : (i += 1) {
        const slot = &stream_slots[i];
        if (!slot.active or
            slot.owner_kind != .program or
            slot.owner_program_id != instance_id or
            slot.owner_program_generation != instance_generation)
            continue;
        if (slot.publish_started and !settlePublishStreamSlot(slot)) {
            all_released = false;
            continue;
        }
        if (!slot.active) continue;
        clearStreamSlot(slot);
    }
    return all_released;
}

/// Releases streams owned by one exact logical ProgramThread. The R4X
/// lifecycle calls this only after all caller-bound async requests are gone
/// (or have been cancelled and physically detached during process retire).
pub fn releaseStreamSlotsForProgramThread(
    instance_id: u32,
    instance_generation: u64,
    task_id: u32,
    task_generation: u64,
) bool {
    if (instance_id == 0 or instance_generation == 0 or task_id == 0 or task_generation == 0) return true;
    var req = fs_request.begin(.stream_abort, 0) orelse return false;
    var all_released = true;
    defer fs_request.finish(&req, all_released);
    var i: usize = 0;
    while (i < stream_slots.len) : (i += 1) {
        const slot = &stream_slots[i];
        if (!slot.active or
            slot.owner_kind != .program or
            slot.owner_program_id != instance_id or
            slot.owner_program_generation != instance_generation or
            slot.owner_id != task_id or
            slot.owner_generation != task_generation)
            continue;
        if (slot.publish_started and !settlePublishStreamSlot(slot)) {
            all_released = false;
            continue;
        }
        if (!slot.active) continue;
        // As in process retirement, leave any caller-owned partial file in
        // place for diagnosis; only the in-memory lease is retired.
        clearStreamSlot(slot);
    }
    return all_released;
}

fn hasActiveStreamInSubtree(
    drive_letter: u8,
    on_boot_volume: bool,
    directory_path: []const u8,
    directory_entry: vfs.Entry,
) bool {
    reapDeadStreamSlots();
    var i: usize = 0;
    while (i < stream_slots.len) : (i += 1) {
        const slot = &stream_slots[i];
        if (!slot.active or
            slot.drive_letter != drive_letter or
            slot.on_boot_volume != on_boot_volume)
            continue;
        if (streamSlotHasAncestor(slot, directory_entry)) return true;
        // Retain the canonical-path check as a conservative fallback for
        // legacy/boot slots without a representable non-root identity.
        if (pathIsSameOrDescendant(slot.raw_path[0..slot.raw_len], directory_path))
            return true;
    }
    return false;
}

fn streamSlotHasAncestor(slot: *const StreamSlot, directory_entry: vfs.Entry) bool {
    if (directory_entry.node == 0) return false;
    var i: usize = 0;
    while (i < slot.ancestor_count) : (i += 1) {
        if (slot.ancestor_nodes[i] == directory_entry.node and
            slot.ancestor_generations[i] == directory_entry.node_generation)
            return true;
    }
    return false;
}

fn pathIsSameOrDescendant(candidate: []const u8, directory: []const u8) bool {
    var directory_len = directory.len;
    while (directory_len > 1 and isPathSeparator(directory[directory_len - 1])) directory_len -= 1;
    if (candidate.len < directory_len) return false;
    var i: usize = 0;
    while (i < directory_len) : (i += 1) {
        const left = candidate[i];
        const right = directory[i];
        if (isPathSeparator(left) and isPathSeparator(right)) continue;
        const left_folded = if (left >= 'A' and left <= 'Z') left + 32 else left;
        const right_folded = if (right >= 'A' and right <= 'Z') right + 32 else right;
        if (left_folded != right_folded) return false;
    }
    return candidate.len == directory_len or
        isPathSeparator(candidate[directory_len]) or
        (directory_len == 1 and isPathSeparator(directory[0]));
}

fn isPathSeparator(ch: u8) bool {
    return ch == '\\' or ch == '/';
}

fn streamNamesCouldAlias(left: []const u8, right: []const u8) bool {
    if (left.len == right.len and asciiEqlIgnoreCase(left, right)) return true;
    // FAT currently folds only ASCII, while NTFS uses the volume $UpCase
    // table. Until the VFS exposes backend-exact collation, fail closed for
    // non-ASCII spellings in the same directory during the short ambiguous
    // publish window. This may briefly serialize unrelated Unicode uploads,
    // but cannot let a collating alias steal a reserved target or backup.
    for (left) |ch| {
        if (ch >= 0x80) return true;
    }
    for (right) |ch| {
        if (ch >= 0x80) return true;
    }
    return false;
}

fn publishStreamSlotMatchesResolved(
    slot: *const StreamSlot,
    drive_letter: u8,
    on_boot_volume: bool,
    parent_node: vfs.NodeRef,
    entry: ?vfs.Entry,
    entry_name: []const u8,
) bool {
    if (!slot.active or !slot.publish_started or
        slot.drive_letter != drive_letter or
        slot.on_boot_volume != on_boot_volume)
        return false;

    // Target and stage can temporarily be two directory aliases of the exact
    // same object. Identity therefore has precedence over path spelling.
    if (entry) |resolved| {
        if (streamSlotOwnsPublishedEntry(slot, resolved)) return true;
    }
    if (slot.parent_node != parent_node) return false;
    const candidate_name = if (entry) |resolved|
        resolved.name[0..resolved.name_len]
    else
        entry_name;
    return streamNamesCouldAlias(streamSlotName(slot), candidate_name) or
        streamNamesCouldAlias(streamPublishTargetName(slot), candidate_name) or
        streamNamesCouldAlias(streamPublishBackupName(slot), candidate_name);
}

fn hasPublishStreamCollisionForResolvedExcept(
    drive_letter: u8,
    on_boot_volume: bool,
    parent_node: vfs.NodeRef,
    entry: ?vfs.Entry,
    entry_name: []const u8,
    ignored_slot: ?*const StreamSlot,
) bool {
    var i: usize = 0;
    while (i < stream_slots.len) : (i += 1) {
        const slot = &stream_slots[i];
        if (ignored_slot) |ignored| {
            if (slot == ignored) continue;
        }
        if (publishStreamSlotMatchesResolved(
            slot,
            drive_letter,
            on_boot_volume,
            parent_node,
            entry,
            entry_name,
        )) return true;
    }
    return false;
}

fn hasPublishStreamCollisionForResolved(
    drive_letter: u8,
    on_boot_volume: bool,
    parent_node: vfs.NodeRef,
    entry: ?vfs.Entry,
    entry_name: []const u8,
) bool {
    return hasPublishStreamCollisionForResolvedExcept(
        drive_letter,
        on_boot_volume,
        parent_node,
        entry,
        entry_name,
        null,
    );
}

/// Invalidates ordinary stream leases before a namespace mutation. A
/// publication that may already have crossed its visibility point is
/// different: its slot is a recovery token and must never be cleared here.
/// The caller must report a retryable error without touching the namespace.
fn invalidateStreamSlotsForResolved(
    drive_letter: u8,
    on_boot_volume: bool,
    parent_node: vfs.NodeRef,
    entry: ?vfs.Entry,
    entry_name: []const u8,
) bool {
    if (hasPublishStreamCollisionForResolved(
        drive_letter,
        on_boot_volume,
        parent_node,
        entry,
        entry_name,
    )) return false;

    var i: usize = 0;
    while (i < stream_slots.len) : (i += 1) {
        const slot = &stream_slots[i];
        if (!slot.active or
            slot.drive_letter != drive_letter or
            slot.on_boot_volume != on_boot_volume)
            continue;
        if (slot.publish_started) continue;
        if (entry) |resolved| {
            if (slot.has_file_node and
                slot.file_node_generation != 0 and
                resolved.node_generation != 0 and
                slot.file_node == resolved.node and
                slot.file_node_generation == resolved.node_generation)
            {
                clearStreamSlot(slot);
                continue;
            }
        }
        if (slot.parent_node != parent_node) continue;
        const matches = if (entry) |resolved|
            (slot.has_file_node and slot.file_node != 0 and resolved.node != 0 and slot.file_node == resolved.node) or
                (slot.name_len == resolved.name_len and
                    asciiEqlIgnoreCase(slot.name[0..slot.name_len], resolved.name[0..resolved.name_len]))
        else
            slot.name_len == entry_name.len and
                asciiEqlIgnoreCase(slot.name[0..slot.name_len], entry_name);
        if (matches) {
            clearStreamSlot(slot);
        }
    }
    return true;
}

/// Builds the drive-rooted parent path a durable upload claim records
/// (0.60.22).  Recovery only has the claim, so it must be able to resolve the
/// volume from the letter and the directory from the rest.  Module-owned
/// buffer: this runs under the filesystem-request gate, never on a task
/// stack.
var claim_parent_buf: [max_api_path + 2]u8 = undefined;

fn buildClaimParentPath(letter: u8, path: []const u8) ?[]const u8 {
    const parent = parentPath(path);
    if (parent.len + 2 > claim_parent_buf.len) return null;
    claim_parent_buf[0] = letter;
    claim_parent_buf[1] = ':';
    var out: usize = 2;
    for (parent) |c| {
        // The claim format is DOS-style and rejects forward slashes.
        claim_parent_buf[out] = if (c == '/') '\\' else c;
        out += 1;
    }
    if (out < 3) return null;
    return claim_parent_buf[0..out];
}

fn claimIdentityFromEntry(entry: vfs.Entry) upc.FileIdentity {
    return .{ .node = entry.node, .generation = entry.node_generation, .size = entry.size };
}

fn parentPath(path: []const u8) []const u8 {
    var i = path.len;
    while (i > 1) : (i -= 1) {
        if (path[i - 1] == '\\' or path[i - 1] == '/') return path[0 .. i - 1];
    }
    return "\\";
}

fn isRootPath(path: []const u8) bool {
    return path.len == 0 or (path.len == 1 and (path[0] == '\\' or path[0] == '/'));
}

fn baseName(path: []const u8) []const u8 {
    var i = path.len;
    while (i > 0) : (i -= 1) {
        if (path[i - 1] == '\\' or path[i - 1] == '/') return path[i..];
    }
    return path;
}

fn joinPathToBuffer(parent: []const u8, entry_name: []const u8, out: []u8) ?[]const u8 {
    const needs_sep = parent.len > 0 and parent[parent.len - 1] != '\\' and parent[parent.len - 1] != '/';
    const sep_len: usize = if (needs_sep) 1 else 0;
    if (parent.len + sep_len + entry_name.len > out.len) return null;
    @memcpy(out[0..parent.len], parent);
    var pos = parent.len;
    if (needs_sep) {
        out[pos] = '\\';
        pos += 1;
    }
    @memcpy(out[pos .. pos + entry_name.len], entry_name);
    pos += entry_name.len;
    return out[0..pos];
}

fn copyDrivePathZ(letter: u8, path: []const u8, out: []u8) bool {
    if (path.len >= 2 and path[1] == ':') return copyPathZ(path, out);
    const needs_sep = path.len == 0 or (path[0] != '\\' and path[0] != '/');
    const sep_len: usize = if (needs_sep) 1 else 0;
    if (2 + sep_len + path.len + 1 > out.len) return false;
    out[0] = asciiUpper(letter);
    out[1] = ':';
    var pos: usize = 2;
    if (needs_sep) {
        out[pos] = '\\';
        pos += 1;
    }
    if (path.len > 0) @memcpy(out[pos .. pos + path.len], path);
    pos += path.len;
    out[pos] = 0;
    return true;
}

fn copyPathZ(path: []const u8, out: []u8) bool {
    if (path.len + 1 > out.len) return false;
    @memcpy(out[0..path.len], path);
    out[path.len] = 0;
    return true;
}

fn parentPathToBuffer(path: []const u8, out: []u8) ?[]const u8 {
    if (path.len <= 1) {
        if (out.len < 1) return null;
        out[0] = '\\';
        return out[0..1];
    }
    var end = path.len;
    while (end > 0 and (path[end - 1] == '\\' or path[end - 1] == '/')) : (end -= 1) {}
    var i = end;
    while (i > 0) : (i -= 1) {
        if (path[i - 1] == '\\' or path[i - 1] == '/') {
            const keep = if (i <= 1) 1 else i - 1;
            if (keep > out.len) return null;
            @memcpy(out[0..keep], path[0..keep]);
            return out[0..keep];
        }
    }
    if (out.len < 1) return null;
    out[0] = '\\';
    return out[0..1];
}

fn stdMemEql(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var i: usize = 0;
    while (i < a.len) : (i += 1) {
        if (a[i] != b[i]) return false;
    }
    return true;
}

const RegistryMutationKind = enum {
    set,
    delete,
};

const RegistryValueList = struct {
    count: usize = 0,
    path_len: usize = 0,
    path_pool: []u8,

    fn init(path_pool: []u8) RegistryValueList {
        return .{ .path_pool = path_pool };
    }

    fn items(self: *const RegistryValueList) []const registry.BuildValue {
        return registry_write_values[0..self.count];
    }

    fn append(self: *RegistryValueList, key_path: []const u8, value_name: []const u8, value_type: registry.ValueType, data: []const u8) bool {
        if (self.count >= registry_write_values.len) return false;
        registry_write_values[self.count] = .{
            .key_path = key_path,
            .name = value_name,
            .value_type = value_type,
            .data = data,
        };
        self.count += 1;
        return true;
    }

    fn appendPath(self: *RegistryValueList, text: []const u8) ?[]const u8 {
        if (self.path_len + text.len > self.path_pool.len) return null;
        const start = self.path_len;
        if (text.len != 0) @memcpy(self.path_pool[start .. start + text.len], text);
        self.path_len += text.len;
        return self.path_pool[start..self.path_len];
    }

    fn appendByte(self: *RegistryValueList, byte: u8) bool {
        if (self.path_len >= self.path_pool.len) return false;
        self.path_pool[self.path_len] = byte;
        self.path_len += 1;
        return true;
    }
};

const MutatingRegistryHive = struct {
    result: i32,
    present: bool = false,
    view: ?registry.HiveView = null,
};

fn registryMutateValue(kind: RegistryMutationKind, hive_kind: registry.HiveKind, key_path: []const u8, value_name: []const u8, value_type: registry.ValueType, data: []const u8) i32 {
    if (!activeRegistryHive(hive_kind)) return registry_api_result_unsupported;
    const deferred_commit = isDeferredRegistryWritebackPath(hive_kind, key_path);
    if (!deferred_commit) {
        const flush_result = flushRegistryWriteback();
        if (flush_result != registry_api_result_ok) return flush_result;
    }

    const hive_mem = heap.alloc(registry_max_hive_bytes, 16) orelse return registry_api_result_io;
    defer _ = heap.free(hive_mem);
    const out_mem = heap.alloc(registry_max_hive_bytes, 16) orelse return registry_api_result_io;
    defer _ = heap.free(out_mem);
    const path_pool = heap.alloc(registry_path_pool_max, 16) orelse return registry_api_result_io;
    defer _ = heap.free(path_pool);

    const loaded = loadRegistryHiveForMutation(hive_kind, hive_mem);
    if (loaded.result != registry_api_result_ok) return loaded.result;
    if (kind == .delete and !loaded.present) return registry_api_result_hive_not_found;

    var list = RegistryValueList.init(path_pool);
    var target_key_index: ?u32 = null;
    if (loaded.view) |hive| {
        target_key_index = hive.findKey(key_path);
        if (kind == .delete) {
            const key_index = target_key_index orelse return registry_api_result_key_not_found;
            _ = hive.findValue(key_index, value_name) orelse return registry_api_result_value_not_found;
        }
        const collect_result = collectRegistryValues(hive, &list, target_key_index, value_name);
        if (collect_result != registry_api_result_ok) return collect_result;
    }

    if (kind == .set) {
        if (!list.append(key_path, value_name, value_type, data)) return registry_api_result_buffer_too_small;
    }

    const generation: u64 = if (loaded.view) |hive| hive.header.generation + 1 else 1;
    if (deferred_commit) {
        return buildAndDeferRegistryHive(hive_kind, generation, list.items(), out_mem);
    }
    return buildAndCommitRegistryHive(hive_kind, generation, list.items(), out_mem, hive_mem);
}

fn loadRegistryHiveForMutation(kind: registry.HiveKind, buffer: []u8) MutatingRegistryHive {
    if (registryCacheBytes(kind)) |bytes| {
        if (buffer.len < bytes.len) return .{ .result = registry_api_result_buffer_too_small, .present = true };
        @memcpy(buffer[0..bytes.len], bytes);
        const view = registry.HiveView.parse(buffer[0..bytes.len]) catch return .{ .result = registry_api_result_hive_corrupt, .present = true };
        return .{ .result = registry_api_result_ok, .present = true, .view = view };
    }
    var path_buf: [max_api_path]u8 = undefined;
    const path = registryHivePathZ(kind, path_buf[0..]) orelse return .{ .result = registry_api_result_bad_path };
    const read = fileRead(path, buffer.ptr, @intCast(buffer.len));
    if (read == -5) return .{ .result = registry_api_result_buffer_too_small };
    if (read <= 0) return .{ .result = registry_api_result_ok };
    const bytes = buffer[0..@intCast(read)];
    const view = registry.HiveView.parse(bytes) catch return .{ .result = registry_api_result_hive_corrupt, .present = true };
    return .{ .result = registry_api_result_ok, .present = true, .view = view };
}

fn collectRegistryValues(hive: registry.HiveView, list: *RegistryValueList, skip_key_index: ?u32, skip_value_name: []const u8) i32 {
    var key_index: u32 = 0;
    while (key_index < hive.header.key_count) : (key_index += 1) {
        const key = hive.keyAt(key_index);
        if (key.value_count == 0) continue;

        const build_path = registryKeyPathForBuild(hive, key_index, list) orelse return registry_api_result_buffer_too_small;
        var value_offset: u32 = 0;
        while (value_offset < key.value_count) : (value_offset += 1) {
            const value = hive.valueAt(key.first_value_index + value_offset);
            if (skip_key_index != null and key_index == skip_key_index.? and asciiEqlIgnoreCase(hive.valueName(value), skip_value_name)) continue;
            if (!list.append(build_path, hive.valueName(value), value.value_type, hive.valueData(value))) return registry_api_result_buffer_too_small;
        }
    }
    return registry_api_result_ok;
}

fn registryKeyPathForBuild(hive: registry.HiveView, key_index: u32, list: *RegistryValueList) ?[]const u8 {
    const start = list.path_len;
    _ = list.appendPath(hive.header.hive_kind.shortRoot()) orelse return null;

    var parts: [registry_key_depth_max]u32 = undefined;
    var count: usize = 0;
    var current = key_index;
    while (current != 0 and count < parts.len) {
        parts[count] = current;
        count += 1;
        current = hive.keyAt(current).parent_index;
        if (current >= hive.header.key_count) return null;
    }
    if (current != 0) return null;

    while (count > 0) {
        count -= 1;
        if (!list.appendByte('\\')) return null;
        _ = list.appendPath(hive.keyName(hive.keyAt(parts[count]))) orelse return null;
    }
    return list.path_pool[start..list.path_len];
}

fn buildAndCommitRegistryHive(kind: registry.HiveKind, generation: u64, values: []const registry.BuildValue, out_mem: []u8, verify_mem: []u8) i32 {
    const bytes = registry.buildHiveInto(out_mem, .{
        .keys = registry_write_keys[0..],
        .value_key_indices = registry_value_key_indices[0..],
        .flat_key_order = registry_flat_key_order[0..],
    }, kind, generation, values) catch |err| return registryBuildError(err);
    return commitRegistryHiveBytes(kind, bytes, verify_mem);
}

fn buildAndDeferRegistryHive(kind: registry.HiveKind, generation: u64, values: []const registry.BuildValue, out_mem: []u8) i32 {
    const bytes = registry.buildHiveInto(out_mem, .{
        .keys = registry_write_keys[0..],
        .value_key_indices = registry_value_key_indices[0..],
        .flat_key_order = registry_flat_key_order[0..],
    }, kind, generation, values) catch |err| return registryBuildError(err);
    return storeRegistryWriteback(kind, bytes, true);
}

fn registryBuildError(err: registry.Error) i32 {
    return switch (err) {
        error.InvalidPath, error.RootMismatch => registry_api_result_bad_path,
        error.BadName, error.BadData, error.BadValue, error.DuplicateKey, error.DuplicateValue => registry_api_result_invalid,
        error.OutOfMemory, error.TooManyEntries => registry_api_result_buffer_too_small,
        else => registry_api_result_hive_corrupt,
    };
}

fn commitRegistryHiveBytes(kind: registry.HiveKind, bytes: []const u8, verify_mem: []u8) i32 {
    _ = registry.HiveView.parse(bytes) catch return registry_api_result_hive_corrupt;

    var sys_path_buf: [max_api_path]u8 = undefined;
    var registry_dir_buf: [max_api_path]u8 = undefined;
    _ = dirCreate(literalRegistryPathZ("C:\\R4OS", sys_path_buf[0..]) orelse return registry_api_result_bad_path);
    _ = dirCreate(literalRegistryPathZ("C:\\R4OS\\REGISTRY", registry_dir_buf[0..]) orelse return registry_api_result_bad_path);

    var tmp_buf: [max_api_path]u8 = undefined;
    var hive_buf: [max_api_path]u8 = undefined;
    var bak_buf: [max_api_path]u8 = undefined;
    const tmp_path = registryHiveTmpPathZ(kind, tmp_buf[0..]) orelse return registry_api_result_bad_path;
    const hive_path = registryHivePathZ(kind, hive_buf[0..]) orelse return registry_api_result_bad_path;
    const bak_path = registryHiveBakPathZ(kind, bak_buf[0..]) orelse return registry_api_result_bad_path;

    _ = fileDelete(tmp_path);
    const written = fileWrite(tmp_path, bytes.ptr, @intCast(bytes.len));
    if (written < 0 or @as(usize, @intCast(written)) != bytes.len) return registry_api_result_io;

    const read_back = fileRead(tmp_path, verify_mem.ptr, @intCast(verify_mem.len));
    if (read_back < 0 or @as(usize, @intCast(read_back)) != bytes.len) return registry_api_result_io;
    _ = registry.HiveView.parse(verify_mem[0..@intCast(read_back)]) catch return registry_api_result_hive_corrupt;

    if (registryFileExists(hive_path)) {
        _ = fileDelete(bak_path);
        if (fileCopy(hive_path, bak_path) <= 0) return registry_api_result_io;
        _ = fileDelete(hive_path);
    }
    if (fileRename(tmp_path, hive_path) <= 0) return registry_api_result_io;
    invalidateRegistryWriteback(kind);
    return registry_api_result_ok;
}

fn flushRegistryWriteback() i32 {
    if (!registry_writeback_cache.valid or !registry_writeback_cache.dirty) return registry_api_result_ok;
    const kind = registry_writeback_cache.kind;
    const bytes = registry_writeback_cache.bytes[0..registry_writeback_cache.len];
    const result = commitRegistryHiveBytes(kind, bytes, registry_writeback_cache.verify[0..]);
    if (result == registry_api_result_ok) {
        invalidateRegistryWriteback(kind);
    }
    return result;
}

fn storeRegistryWriteback(kind: registry.HiveKind, bytes: []const u8, dirty: bool) i32 {
    _ = registry.HiveView.parse(bytes) catch return registry_api_result_hive_corrupt;
    if (bytes.len > registry_writeback_cache.bytes.len) return registry_api_result_buffer_too_small;
    const cache_start = @intFromPtr(&registry_writeback_cache.bytes[0]);
    const cache_end = cache_start + registry_writeback_cache.bytes.len;
    const source_start = @intFromPtr(bytes.ptr);
    const source_inside_cache = source_start >= cache_start and source_start < cache_end;
    if (!source_inside_cache and bytes.len != 0) {
        @memcpy(registry_writeback_cache.bytes[0..bytes.len], bytes);
    }
    registry_writeback_cache.valid = true;
    registry_writeback_cache.dirty = dirty;
    registry_writeback_cache.kind = kind;
    registry_writeback_cache.len = bytes.len;
    return registry_api_result_ok;
}

fn registryCacheBytes(kind: registry.HiveKind) ?[]const u8 {
    if (!registry_writeback_cache.valid or registry_writeback_cache.kind != kind) return null;
    return registry_writeback_cache.bytes[0..registry_writeback_cache.len];
}

fn invalidateRegistryWriteback(kind: registry.HiveKind) void {
    if (!registry_writeback_cache.valid or registry_writeback_cache.kind != kind) return;
    registry_writeback_cache.valid = false;
    registry_writeback_cache.dirty = false;
    registry_writeback_cache.len = 0;
}

const LoadedRegistryHive = struct {
    result: i32,
    view: ?registry.HiveView = null,
};

fn loadRegistryHive(kind: registry.HiveKind, buffer: []u8) LoadedRegistryHive {
    if (registryCacheBytes(kind)) |bytes| {
        if (buffer.len < bytes.len) return .{ .result = registry_api_result_buffer_too_small };
        @memcpy(buffer[0..bytes.len], bytes);
        const view = registry.HiveView.parse(buffer[0..bytes.len]) catch return .{ .result = registry_api_result_hive_corrupt };
        return .{ .result = registry_api_result_ok, .view = view };
    }
    var path_buf: [max_api_path]u8 = undefined;
    const path = registryHivePathZ(kind, path_buf[0..]) orelse return .{ .result = registry_api_result_bad_path };
    const read = fileRead(path, buffer.ptr, @intCast(buffer.len));
    if (read == -5) return .{ .result = registry_api_result_buffer_too_small };
    if (read <= 0) return .{ .result = registry_api_result_hive_not_found };
    const bytes = buffer[0..@intCast(read)];
    const view = registry.HiveView.parse(bytes) catch return .{ .result = registry_api_result_hive_corrupt };
    return .{ .result = registry_api_result_ok, .view = view };
}

fn registryHivePathZ(kind: registry.HiveKind, out: []u8) ?[*:0]const u8 {
    const text = switch (kind) {
        .system => "C:\\R4OS\\REGISTRY\\SYSTEM.R4R",
        else => return null,
    };
    if (!copyPathZ(text, out)) return null;
    return @ptrCast(out.ptr);
}

fn registryHiveTmpPathZ(kind: registry.HiveKind, out: []u8) ?[*:0]const u8 {
    const text = switch (kind) {
        .system => "C:\\R4OS\\REGISTRY\\SYSTEM.TMP",
        else => return null,
    };
    if (!copyPathZ(text, out)) return null;
    return @ptrCast(out.ptr);
}

fn registryHiveBakPathZ(kind: registry.HiveKind, out: []u8) ?[*:0]const u8 {
    const text = switch (kind) {
        .system => "C:\\R4OS\\REGISTRY\\SYSTEM.BAK",
        else => return null,
    };
    if (!copyPathZ(text, out)) return null;
    return @ptrCast(out.ptr);
}

fn activeRegistryHive(kind: registry.HiveKind) bool {
    return kind == .system;
}

fn isDeferredRegistryWritebackPath(kind: registry.HiveKind, key_path: []const u8) bool {
    if (kind != .system) return false;
    if (!startsWithIgnoreCase(key_path, registry_recent_documents_root)) return false;
    return key_path.len == registry_recent_documents_root.len or key_path[registry_recent_documents_root.len] == '\\';
}

fn startsWithIgnoreCase(value: []const u8, prefix: []const u8) bool {
    if (value.len < prefix.len) return false;
    var i: usize = 0;
    while (i < prefix.len) : (i += 1) {
        if (asciiLower(value[i]) != asciiLower(prefix[i])) return false;
    }
    return true;
}

fn asciiLower(value: u8) u8 {
    return if (value >= 'A' and value <= 'Z') value + ('a' - 'A') else value;
}

fn literalRegistryPathZ(text: []const u8, out: []u8) ?[*:0]const u8 {
    if (!copyPathZ(text, out)) return null;
    return @ptrCast(out.ptr);
}

fn registryFileExists(path: [*:0]const u8) bool {
    var info: FileInfo = .{};
    return fileInfo(path, &info) > 0 and info.exists != 0;
}

fn fillRegistryValueInfo(out: *RegistryValueInfo, hive: registry.HiveView, value: registry.ValueRecord) void {
    out.* = .{
        .value_type = @intFromEnum(value.value_type),
        .data_len = value.data_len,
    };
    copyFixedZ(out.name[0..], hive.valueName(value));
}

fn copyRegistryNameOut(entry_name: []const u8, out_ptr: [*]u8, capacity: u32) i32 {
    const out_capacity: usize = @intCast(capacity);
    if (out_capacity == 0 or out_capacity <= entry_name.len) return registry_api_result_buffer_too_small;
    const out = out_ptr[0..out_capacity];
    if (entry_name.len != 0) @memcpy(out[0..entry_name.len], entry_name);
    out[entry_name.len] = 0;
    return @intCast(entry_name.len);
}

fn asciiEqlIgnoreCase(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var index: usize = 0;
    while (index < a.len) : (index += 1) {
        if (asciiUpper(a[index]) != asciiUpper(b[index])) return false;
    }
    return true;
}

fn asciiUpper(ch: u8) u8 {
    if (ch >= 'a' and ch <= 'z') return ch - ('a' - 'A');
    return ch;
}
