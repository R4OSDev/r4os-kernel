//! Stateless R4SYS source facade. Rust owns every R4SYS algorithm, mutable
//! registry/stream/provider/diagnostic state and system-transition admission.
const r4x_api=@import("r4x_api.zig");
const drive=@import("../fs/drive.zig");
const vfs=@import("../fs/vfs.zig");
const fs_request=@import("../fs/request.zig");
const time_core=@import("../platform/time.zig");
const bootlog=@import("../kernel/bootlog.zig");
const k=@import("../kernel/log.zig");
const power=@import("../arch/x86_64/power.zig");
const reset=@import("../arch/x86_64/reset.zig");
const net=@import("../net/core.zig");
const r4d=@import("r4d.zig");

pub const name = "R4SYS";

pub const max_api_path: usize = @as(usize, r4x_api.file_path_max_bytes) + 1;

pub const dir_entry_result_end: i32 = -5;

pub const dir_entry_error_io: i32 = -9;

pub const Target = struct {
    drive_ref: *drive.Drive,
    path: []const u8,
};

pub const FsFailureDiagnostic = extern struct {
    sequence: u32 = 0,
    task_id: u32 = 0,
    kind: fs_request.Kind = .file_info,
    result: i32 = 0,
    lookup_stage: u32 = 0,
};

pub const ResolveTargetFn = *const fn ([]const u8, *[max_api_path]u8) ?Target;

pub const StreamOwner = extern struct {
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

pub const file_stream_open_lease = r4x_api.file_stream_open_lease;

pub const file_stream_finish_keep_ownership: u32 = 1 << 0;

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

pub const file_update_atomic_checked_flag_long_stage = r4x_api.file_update_atomic_checked_flag_long_stage;

pub const file_stream_declare_publish_result_ok: i32 = 0;

pub const file_stream_declare_publish_error_invalid: i32 = -1;

pub const file_stream_declare_publish_error_unsupported: i32 = -2;

pub const file_stream_declare_publish_error_bad_path: i32 = -3;

pub const file_stream_declare_publish_error_not_found: i32 = -4;

pub const file_stream_declare_publish_error_not_atomic: i32 = -5;

pub const file_stream_declare_publish_error_io: i32 = -6;

pub const path_names_equal_result_different: i32 = 0;

pub const path_names_equal_result_equal: i32 = 1;

pub const path_names_equal_error_invalid: i32 = -1;

pub const path_names_equal_error_unsupported: i32 = -2;

pub const path_names_equal_error_bad_path: i32 = -3;

pub const path_names_equal_error_not_same_volume: i32 = -4;

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

pub const program_module_running_result_idle: i32 = 0;

pub const program_module_running_result_running: i32 = 1;

pub const program_module_running_error_invalid: i32 = -1;

pub const program_module_running_error_unavailable: i32 = -2;

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

pub const RegistryKeyInfo = r4x_api.RegistryKeyInfo;

pub const RegistryValueInfo = r4x_api.RegistryValueInfo;

pub const RegistryBatchOperation = r4x_api.RegistryBatchOperation;

pub const RegistryBatchResult = r4x_api.RegistryBatchResult;

pub const RegistrySnapshotCursor = r4x_api.RegistrySnapshotCursor;

pub const RegistrySnapshotEntry = r4x_api.RegistrySnapshotEntry;

pub const RegistrySnapshotPageInfo = r4x_api.RegistrySnapshotPageInfo;

pub const MonotonicClockInfo = r4x_api.MonotonicClockInfo;

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

pub const RewriteFileFn = *const fn (
    input: []const u8,
    output: []u8,
    context: *const anyopaque,
) ?usize;

pub fn cpuCapacity(output: *r4x_api.CpuCapacity) callconv(.c) i32 {
    return r4os_rust_sys_cpu_capacity(output);
}

pub fn fsFailureDiagnostic() FsFailureDiagnostic {
    var value:FsFailureDiagnostic=undefined;r4os_rust_sys_failure(&value);return value;
}

pub fn setPathResolver(resolver: ResolveTargetFn) void {
    r4os_rust_sys_set_provider(0,@intFromPtr(resolver));
}

pub fn setStreamOwnerResolver(resolver: ResolveStreamOwnerFn) void {
    r4os_rust_sys_set_provider(1,@intFromPtr(resolver));
}

pub fn setProgramModuleRunningProvider(provider: ProgramModuleRunningFn) void {
    r4os_rust_sys_set_provider(2,@intFromPtr(provider));
}

pub fn setRuntimeContext(usable_bytes: u64) void {
    _ = usable_bytes;
}

pub fn taskYield() callconv(.c) void {
    return r4os_rust_sys_yield();
}

pub fn sleepTicks(ticks_value: u64) callconv(.c) void {
    return r4os_rust_sys_sleep(ticks_value);
}

pub fn ticks() callconv(.c) u64 {
    return r4os_rust_sys_ticks();
}

pub fn timeSecondsSinceMidnight() callconv(.c) u32 {
    return r4os_rust_sys_seconds();
}

pub fn timeState(out: *time_core.State) callconv(.c) void {
    return r4os_rust_sys_time_state(out);
}

pub fn monotonicClock(out: *MonotonicClockInfo) callconv(.c) i32 {
    return r4os_rust_sys_monotonic(out);
}

pub fn timeSetState(request: *const time_core.State) callconv(.c) i32 {
    return r4os_rust_sys_time_set(request);
}

pub fn bootLogInfo(out: *BootLogInfo) callconv(.c) i32 {
    return r4os_rust_sys_bootlog_info(out);
}

pub fn bootLogRead(offset: u32, out: [*]u8, capacity_value: u32) callconv(.c) i32 {
    return r4os_rust_sys_bootlog_read(offset,out,capacity_value);
}

pub fn systemHalt() callconv(.c) void {
    return r4os_rust_sys_halt();
}

pub fn systemReboot() callconv(.c) void {
    return r4os_rust_sys_reboot();
}

pub fn systemPoweroff() callconv(.c) void {
    return r4os_rust_sys_poweroff();
}

pub fn registryKeyInfo(path_ptr: [*:0]const u8, out: *RegistryKeyInfo) callconv(.c) i32 {
    return r4os_rust_sys_registry_key_info(path_ptr,out);
}

pub fn registryEnumKey(path_ptr: [*:0]const u8, index: u32, out_ptr: [*]u8, capacity: u32) callconv(.c) i32 {
    return r4os_rust_sys_registry_enum_key(path_ptr,index,out_ptr,capacity);
}

pub fn registryEnumValue(path_ptr: [*:0]const u8, index: u32, out: *RegistryValueInfo) callconv(.c) i32 {
    return r4os_rust_sys_registry_enum_value(path_ptr,index,out);
}

pub fn registryGetValue(path_ptr: [*:0]const u8, name_ptr: [*:0]const u8, out_info: *RegistryValueInfo, out_ptr: [*]u8, capacity: u32) callconv(.c) i32 {
    return r4os_rust_sys_registry_get(path_ptr,name_ptr,out_info,out_ptr,capacity);
}

pub fn registrySetValue(path_ptr: [*:0]const u8, name_ptr: [*:0]const u8, value_type_raw: u16, data_ptr: [*]const u8, data_len: u32) callconv(.c) i32 {
    return r4os_rust_sys_registry_set(path_ptr,name_ptr,value_type_raw,data_ptr,data_len);
}

pub fn registryDeleteValue(path_ptr: [*:0]const u8, name_ptr: [*:0]const u8) callconv(.c) i32 {
    return r4os_rust_sys_registry_delete(path_ptr,name_ptr);
}

pub fn registrySnapshotBegin(path_ptr: [*:0]const u8, kind: u32, cursor: *RegistrySnapshotCursor) callconv(.c) i32 {
    return r4os_rust_sys_registry_snapshot_begin(path_ptr,kind,cursor);
}

pub fn registrySnapshotPage(
    cursor: *RegistrySnapshotCursor,
    out_entries: [*]RegistrySnapshotEntry,
    entry_capacity: u32,
    out_data: [*]u8,
    data_capacity: u32,
    out_page: *RegistrySnapshotPageInfo,
) callconv(.c) i32 {
    return r4os_rust_sys_registry_snapshot_page(cursor,out_entries,entry_capacity,out_data,data_capacity,out_page);
}

pub fn registryBatchMutate(
    operations_ptr: [*]const RegistryBatchOperation,
    operation_count: u32,
    blob_ptr: [*]const u8,
    blob_len: u32,
    out_result: *RegistryBatchResult,
) callconv(.c) i32 {
    return r4os_rust_sys_registry_batch(operations_ptr,operation_count,blob_ptr,blob_len,out_result);
}

pub fn driveInfo(index: u32, out: *DriveInfo) callconv(.c) i32 {
    return r4os_rust_sys_drive_info(index,out);
}

pub fn moduleResourceStat(path_ptr: [*:0]const u8, resource_type: u32, resource_index: u32, name_ptr: ?[*:0]const u8) callconv(.c) i32 {
    return r4os_rust_sys_resource_stat(path_ptr,resource_type,resource_index,name_ptr);
}

pub fn moduleResourceRead(path_ptr: [*:0]const u8, resource_type: u32, resource_index: u32, name_ptr: ?[*:0]const u8, out_ptr: [*]u8, out_len: u32) callconv(.c) i32 {
    return r4os_rust_sys_resource_read(path_ptr,resource_type,resource_index,name_ptr,out_ptr,out_len);
}

pub fn fileRead(path_ptr: [*:0]const u8, out_ptr: [*]u8, max_len: u32) callconv(.c) i32 {
    return r4os_rust_sys_file_read(path_ptr,out_ptr,max_len);
}

pub fn fileWrite(path_ptr: [*:0]const u8, data_ptr: [*]const u8, len: u32) callconv(.c) i32 {
    return r4os_rust_sys_file_write(path_ptr,data_ptr,len);
}

pub fn rewriteFileUnderGate(
    raw_path: []const u8,
    input: []u8,
    output: []u8,
    context: *const anyopaque,
    rewrite: RewriteFileFn,
) i32 {
    return r4os_rust_sys_rewrite(nativeBytes(raw_path),input.ptr,input.len,output.ptr,output.len,context,@intFromPtr(rewrite));
}

pub fn fileReadAt(path_ptr: [*:0]const u8, offset: u32, out_ptr: [*]u8, max_len: u32) callconv(.c) i32 {
    return fileReadAt64(path_ptr,offset,out_ptr,max_len);
}

pub fn fileReadAt64(path_ptr: [*:0]const u8, offset: u64, out_ptr: [*]u8, max_len: u32) callconv(.c) i32 {
    return r4os_rust_sys_file_read_at(path_ptr,offset,out_ptr,max_len);
}

pub fn fileWriteAt(path_ptr: [*:0]const u8, offset: u64, data_ptr: [*]const u8, len: u32) callconv(.c) i32 {
    return r4os_rust_sys_file_write_at(path_ptr,offset,data_ptr,len);
}

pub fn fileAppend(path_ptr: [*:0]const u8, data_ptr: [*]const u8, len: u32) callconv(.c) i32 {
    return r4os_rust_sys_file_append(path_ptr,data_ptr,len);
}

pub fn fileStreamBegin(path_ptr: [*:0]const u8, flags: u32) callconv(.c) i32 {
    return r4os_rust_sys_stream_begin(path_ptr,flags);
}

pub fn storageUseBegin(path: [*:0]const u8, out: *u64) callconv(.c) i32 {
    return r4os_rust_sys_storage_use(path,out);
}

pub fn fileStreamWrite(path_ptr: [*:0]const u8, offset: u64, data_ptr: [*]const u8, len: u32, flags: u32) callconv(.c) i32 {
    return r4os_rust_sys_stream_write(path_ptr,offset,data_ptr,len,flags);
}

pub fn fileStreamFinish(path_ptr: [*:0]const u8, expected_size: u64, flags: u32) callconv(.c) i32 {
    return r4os_rust_sys_stream_finish(path_ptr,expected_size,flags);
}

pub fn fileStreamAbort(path_ptr: [*:0]const u8) callconv(.c) i32 {
    return r4os_rust_sys_stream_abort(path_ptr);
}

pub fn dirList(path_ptr: [*:0]const u8, out_ptr: [*]u8, max_len: u32) callconv(.c) i32 {
    return r4os_rust_sys_dir_list(path_ptr,out_ptr,max_len);
}

pub fn directoryChangeBegin(path_ptr: [*:0]const u8, cursor: *r4x_api.DirectoryChangeCursor) callconv(.c) i32 {
    return r4os_rust_sys_directory_change_begin(path_ptr,cursor);
}

pub fn directoryChangePoll(cursor: *r4x_api.DirectoryChangeCursor) callconv(.c) i32 {
    return r4os_rust_sys_directory_change_poll(cursor);
}

pub fn dirEntry(path_ptr: [*:0]const u8, index: u32, out_ptr: [*]u8, max_len: u32) callconv(.c) i32 {
    return r4os_rust_sys_dir_entry(path_ptr,index,out_ptr,max_len);
}

pub fn directoryNext(path_ptr: [*:0]const u8, cursor: *r4x_api.DirectoryScanCursor, out_ptr: [*]u8, max_len: u32, info: *FileInfo) callconv(.c) i32 {
    return r4os_rust_sys_directory_next(path_ptr,cursor,out_ptr,max_len,info);
}

pub fn fileInfo(path_ptr: [*:0]const u8, out: *FileInfo) callconv(.c) i32 {
    return r4os_rust_sys_file_info(path_ptr,out);
}

pub fn fileDelete(path_ptr: [*:0]const u8) callconv(.c) i32 {
    return r4os_rust_sys_file_delete(path_ptr);
}

pub fn fileDeleteIfMatch(
    path_ptr: [*:0]const u8,
    expected_size: u64,
    expected_checksum: u32,
) callconv(.c) i32 {
    return r4os_rust_sys_file_delete_match(path_ptr,expected_size,expected_checksum);
}

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
    return r4os_rust_sys_update_checked(target_ptr,staged_ptr,backup_ptr,new_size,new_checksum,old_size,old_checksum,flags);
}

pub fn fileStreamDeclarePublish(
    staged_ptr: [*:0]const u8,
    target_ptr: [*:0]const u8,
    backup_ptr: [*:0]const u8,
    protocol_raw: u32,
) callconv(.c) i32 {
    return r4os_rust_sys_stream_declare(staged_ptr,target_ptr,backup_ptr,protocol_raw);
}

pub fn pathNamesEqualCollated(
    left_ptr: [*:0]const u8,
    right_ptr: [*:0]const u8,
) callconv(.c) i32 {
    return r4os_rust_sys_names_equal(left_ptr,right_ptr);
}

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
    return r4os_rust_sys_update_cleanup(target_ptr,staged_ptr,backup_ptr,previous_ptr,new_size,new_checksum,old_size,old_checksum,previous_size,previous_checksum,flags);
}

pub fn programModuleRunning(module_path_ptr: [*:0]const u8) callconv(.c) i32 {
    return r4os_rust_sys_module_running(module_path_ptr);
}

pub fn dirCreate(path_ptr: [*:0]const u8) callconv(.c) i32 {
    return r4os_rust_sys_dir_create(path_ptr);
}

pub fn dirDelete(path_ptr: [*:0]const u8) callconv(.c) i32 {
    return r4os_rust_sys_dir_delete(path_ptr);
}

pub fn fileRename(old_ptr: [*:0]const u8, new_ptr: [*:0]const u8) callconv(.c) i32 {
    return r4os_rust_sys_file_rename(old_ptr,new_ptr);
}

pub fn fileReplaceAtomic(target_ptr: [*:0]const u8, staged_ptr: [*:0]const u8, backup_ptr: [*:0]const u8, flags: u32) callconv(.c) i32 {
    return r4os_rust_sys_file_replace(target_ptr,staged_ptr,backup_ptr,flags,true);
}

pub fn setCopyCancellationProvider(provider: *const fn () callconv(.c) u32) void {
    r4os_rust_sys_copy_cancel_provider(@intFromPtr(provider));
}

pub fn fileCopy(src_ptr: [*:0]const u8, dst_ptr: [*:0]const u8) callconv(.c) i32 {
    return r4os_rust_sys_file_copy(src_ptr,dst_ptr);
}

pub fn fileCopyBuffered(src_ptr: [*:0]const u8, dst_ptr: [*:0]const u8, buffer: [*]u8, length: u32, out: *r4x_api.FileCopyProgress) callconv(.c) i32 {
    return r4os_rust_sys_file_copy_buffered(src_ptr,dst_ptr,buffer,length,out);
}

pub fn fileMove(src_ptr: [*:0]const u8, dst_ptr: [*:0]const u8) callconv(.c) i32 {
    return r4os_rust_sys_file_move(src_ptr,dst_ptr);
}

pub fn releaseStreamSlotsForProgram(instance_id: u32, instance_generation: u64) bool {
    return r4os_rust_sys_stream_cleanup(instance_id,instance_generation,0,0,false);
}

pub fn releaseStreamSlotsForProgramThread(
    instance_id: u32,
    instance_generation: u64,
    task_id: u32,
    task_generation: u64,
) bool {
    return r4os_rust_sys_stream_cleanup(instance_id,instance_generation,task_id,task_generation,true);
}

extern fn r4os_rust_sys_cpu_capacity(output: *r4x_api.CpuCapacity) callconv(.c) i32;

extern fn r4os_rust_sys_yield() callconv(.c) void;

extern fn r4os_rust_sys_sleep(ticks_value: u64) callconv(.c) void;

extern fn r4os_rust_sys_ticks() callconv(.c) u64;

extern fn r4os_rust_sys_seconds() callconv(.c) u32;

extern fn r4os_rust_sys_time_state(out: *time_core.State) callconv(.c) void;

extern fn r4os_rust_sys_monotonic(out: *MonotonicClockInfo) callconv(.c) i32;

extern fn r4os_rust_sys_time_set(request: *const time_core.State) callconv(.c) i32;

extern fn r4os_rust_sys_bootlog_info(out: *BootLogInfo) callconv(.c) i32;

extern fn r4os_rust_sys_bootlog_read(offset: u32, out: [*]u8, capacity_value: u32) callconv(.c) i32;

extern fn r4os_rust_sys_halt() callconv(.c) void;

extern fn r4os_rust_sys_reboot() callconv(.c) void;

extern fn r4os_rust_sys_poweroff() callconv(.c) void;

extern fn r4os_rust_sys_registry_key_info(path_ptr: [*:0]const u8, out: *RegistryKeyInfo) callconv(.c) i32;

extern fn r4os_rust_sys_registry_enum_key(path_ptr: [*:0]const u8, index: u32, out_ptr: [*]u8, capacity: u32) callconv(.c) i32;

extern fn r4os_rust_sys_registry_enum_value(path_ptr: [*:0]const u8, index: u32, out: *RegistryValueInfo) callconv(.c) i32;

extern fn r4os_rust_sys_registry_get(path_ptr: [*:0]const u8, name_ptr: [*:0]const u8, out_info: *RegistryValueInfo, out_ptr: [*]u8, capacity: u32) callconv(.c) i32;

extern fn r4os_rust_sys_registry_set(path_ptr: [*:0]const u8, name_ptr: [*:0]const u8, value_type_raw: u16, data_ptr: [*]const u8, data_len: u32) callconv(.c) i32;

extern fn r4os_rust_sys_registry_delete(path_ptr: [*:0]const u8, name_ptr: [*:0]const u8) callconv(.c) i32;

extern fn r4os_rust_sys_registry_snapshot_begin(path_ptr: [*:0]const u8, kind: u32, cursor: *RegistrySnapshotCursor) callconv(.c) i32;

extern fn r4os_rust_sys_registry_snapshot_page(
    cursor: *RegistrySnapshotCursor,
    out_entries: [*]RegistrySnapshotEntry,
    entry_capacity: u32,
    out_data: [*]u8,
    data_capacity: u32,
    out_page: *RegistrySnapshotPageInfo,
) callconv(.c) i32;

extern fn r4os_rust_sys_registry_batch(
    operations_ptr: [*]const RegistryBatchOperation,
    operation_count: u32,
    blob_ptr: [*]const u8,
    blob_len: u32,
    out_result: *RegistryBatchResult,
) callconv(.c) i32;

extern fn r4os_rust_sys_drive_info(index: u32, out: *DriveInfo) callconv(.c) i32;

extern fn r4os_rust_sys_resource_stat(path_ptr: [*:0]const u8, resource_type: u32, resource_index: u32, name_ptr: ?[*:0]const u8) callconv(.c) i32;

extern fn r4os_rust_sys_resource_read(path_ptr: [*:0]const u8, resource_type: u32, resource_index: u32, name_ptr: ?[*:0]const u8, out_ptr: [*]u8, out_len: u32) callconv(.c) i32;

extern fn r4os_rust_sys_file_read(path_ptr: [*:0]const u8, out_ptr: [*]u8, max_len: u32) callconv(.c) i32;

extern fn r4os_rust_sys_file_write(path_ptr: [*:0]const u8, data_ptr: [*]const u8, len: u32) callconv(.c) i32;

extern fn r4os_rust_sys_file_read_at(path_ptr: [*:0]const u8, offset: u64, out_ptr: [*]u8, max_len: u32) callconv(.c) i32;

extern fn r4os_rust_sys_file_write_at(path_ptr: [*:0]const u8, offset: u64, data_ptr: [*]const u8, len: u32) callconv(.c) i32;

extern fn r4os_rust_sys_file_append(path_ptr: [*:0]const u8, data_ptr: [*]const u8, len: u32) callconv(.c) i32;

extern fn r4os_rust_sys_stream_begin(path_ptr: [*:0]const u8, flags: u32) callconv(.c) i32;

extern fn r4os_rust_sys_storage_use(path: [*:0]const u8, out: *u64) callconv(.c) i32;

extern fn r4os_rust_sys_stream_write(path_ptr: [*:0]const u8, offset: u64, data_ptr: [*]const u8, len: u32, flags: u32) callconv(.c) i32;

extern fn r4os_rust_sys_stream_finish(path_ptr: [*:0]const u8, expected_size: u64, flags: u32) callconv(.c) i32;

extern fn r4os_rust_sys_stream_abort(path_ptr: [*:0]const u8) callconv(.c) i32;

extern fn r4os_rust_sys_dir_list(path_ptr: [*:0]const u8, out_ptr: [*]u8, max_len: u32) callconv(.c) i32;

extern fn r4os_rust_sys_directory_change_begin(path_ptr: [*:0]const u8, cursor: *r4x_api.DirectoryChangeCursor) callconv(.c) i32;

extern fn r4os_rust_sys_directory_change_poll(cursor: *r4x_api.DirectoryChangeCursor) callconv(.c) i32;

extern fn r4os_rust_sys_dir_entry(path_ptr: [*:0]const u8, index: u32, out_ptr: [*]u8, max_len: u32) callconv(.c) i32;

extern fn r4os_rust_sys_directory_next(path_ptr: [*:0]const u8, cursor: *r4x_api.DirectoryScanCursor, out_ptr: [*]u8, max_len: u32, info: *FileInfo) callconv(.c) i32;

extern fn r4os_rust_sys_file_info(path_ptr: [*:0]const u8, out: *FileInfo) callconv(.c) i32;

extern fn r4os_rust_sys_file_delete(path_ptr: [*:0]const u8) callconv(.c) i32;

extern fn r4os_rust_sys_file_delete_match(
    path_ptr: [*:0]const u8,
    expected_size: u64,
    expected_checksum: u32,
) callconv(.c) i32;

extern fn r4os_rust_sys_update_checked(
    target_ptr: [*:0]const u8,
    staged_ptr: [*:0]const u8,
    backup_ptr: [*:0]const u8,
    new_size: u64,
    new_checksum: u32,
    old_size: u64,
    old_checksum: u32,
    flags: u32,
) callconv(.c) i32;

extern fn r4os_rust_sys_stream_declare(
    staged_ptr: [*:0]const u8,
    target_ptr: [*:0]const u8,
    backup_ptr: [*:0]const u8,
    protocol_raw: u32,
) callconv(.c) i32;

extern fn r4os_rust_sys_names_equal(
    left_ptr: [*:0]const u8,
    right_ptr: [*:0]const u8,
) callconv(.c) i32;

extern fn r4os_rust_sys_update_cleanup(
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
) callconv(.c) i32;

extern fn r4os_rust_sys_module_running(module_path_ptr: [*:0]const u8) callconv(.c) i32;

extern fn r4os_rust_sys_dir_create(path_ptr: [*:0]const u8) callconv(.c) i32;

extern fn r4os_rust_sys_dir_delete(path_ptr: [*:0]const u8) callconv(.c) i32;

extern fn r4os_rust_sys_file_rename(old_ptr: [*:0]const u8, new_ptr: [*:0]const u8) callconv(.c) i32;

extern fn r4os_rust_sys_file_copy(src_ptr: [*:0]const u8, dst_ptr: [*:0]const u8) callconv(.c) i32;

extern fn r4os_rust_sys_file_copy_buffered(src_ptr: [*:0]const u8, dst_ptr: [*:0]const u8, buffer: [*]u8, length: u32, out: *r4x_api.FileCopyProgress) callconv(.c) i32;

extern fn r4os_rust_sys_file_move(src_ptr: [*:0]const u8, dst_ptr: [*:0]const u8) callconv(.c) i32;


const NativeBytes=extern struct {
    data:?[*]const u8,len:usize,
    fn slice(self:NativeBytes) []const u8{return if(self.len==0) "" else self.data.?[0..self.len];}
};
fn nativeBytes(value:[]const u8) NativeBytes{return .{.data=value.ptr,.len=value.len};}
const NativeTarget=extern struct {drive_ref:*drive.Drive,path:NativeBytes};


extern fn r4os_rust_sys_set_provider(u8,usize) callconv(.c) void;
extern fn r4os_rust_sys_failure(*FsFailureDiagnostic) callconv(.c) void;
extern fn r4os_rust_sys_rewrite(NativeBytes,[*]u8,usize,[*]u8,usize,*const anyopaque,usize) callconv(.c) i32;
extern fn r4os_rust_sys_file_replace([*:0]const u8,[*:0]const u8,[*:0]const u8,u32,bool) callconv(.c) i32;
extern fn r4os_rust_sys_copy_cancel_provider(usize) callconv(.c) void;
extern fn r4os_rust_sys_stream_cleanup(u32,u64,u32,u64,bool) callconv(.c) bool;

// Typed calls to original Zig-ABI providers; no provider handle/state lives here.
export fn r4os_zig_sys_path_resolve(address:usize,raw:NativeBytes,buffer:[*]u8,out:*NativeTarget) callconv(.c) bool {
    const provider:ResolveTargetFn=@ptrFromInt(address);const target=provider(raw.slice(),@ptrCast(buffer)) orelse return false;
    out.*=.{.drive_ref=target.drive_ref,.path=nativeBytes(target.path)};return true;
}
export fn r4os_zig_sys_stream_owner(address:usize,out:*StreamOwner) callconv(.c) bool {
    const provider:ResolveStreamOwnerFn=@ptrFromInt(address);out.*=provider() orelse return false;return true;
}
export fn r4os_zig_sys_module_running(address:usize,letter:u8,path:NativeBytes) callconv(.c) i32 {
    const provider:ProgramModuleRunningFn=@ptrFromInt(address);return provider(letter,path.slice());
}
export fn r4os_zig_sys_rewrite(address:usize,input:NativeBytes,output:[*]u8,capacity:usize,context:*const anyopaque,out:*usize) callconv(.c) bool {
    const provider:RewriteFileFn=@ptrFromInt(address);out.*=provider(input.slice(),output[0..capacity],context) orelse return false;return true;
}

// Resident log snapshot/peer leaves. Native R4SYS owns caller-buffer staging,
// system transition decisions and the single reboot-admission flag.
export fn r4os_zig_sys_bootlog_info(out:*BootLogInfo) callconv(.c) void {
    const value=bootlog.info();out.*=.{.capacity=@intCast(bootlog.capacity()),.length=@intCast(value.length),.flags=value.flags,.reserved=0,.total_written=value.total_written,.dropped_bytes=value.dropped_bytes};
}
export fn r4os_zig_sys_bootlog_read(snapshot:bool,offset:usize,out:[*]u8,len:usize) callconv(.c) usize {
    return if(snapshot) bootlog.snapshot(out[0..len]) else bootlog.read(offset,out[0..len]);
}
export fn r4os_zig_sys_power_transition(operation:u8,ticks_value:u64,reason:NativeBytes) callconv(.c) bool {
    return switch(operation){
        0=>blk:{power.dumpStatus();break :blk true;},
        1=>@import("../display/display.zig").beginSystemTransition(ticks_value),
        2=>net.beginSystemTransition(reason.slice()),
        3=>r4d.shutdownForSystemTransition(),
        4=>@import("../display/display.zig").systemTransitionQuiesced(),else=>unreachable,
    };
}
export fn r4os_zig_sys_serial_flush() callconv(.c) void {k.serialFlush();}
export fn r4os_zig_sys_version() callconv(.c) NativeBytes {return nativeBytes(@import("../kernel/version.zig").text);}
export fn r4os_zig_sys_stream_error(admission:bool,letter:u8,phase:u32,offset:u64,len:u32,native_volume:*const vfs.NativeVolume) callconv(.c) void {
    if(admission){k.puts("[FSSTREAM] write admission failed\r\n");return;}
    const volume=vfs.volumeFromNative(native_volume.*);

        k.puts("[FSSTREAM] write failed drive=");
        k.putc(letter);
        k.puts(" phase=");
        k.putDec(phase);
        k.puts(" offset=");
        k.putDec(offset);
        k.puts(" bytes=");
        k.putDec(len);
        k.puts(" lookup=");
        k.putDec(vfs.lookupDiagnosticStage(volume));
        if (volume == .ntfs) {
            k.puts(" append=");
            k.putDec(@import("../fs/ntfs/ntfs.zig").appendDiagnosticStage());
        }
        k.puts("\r\n");
        const cache = @import("../fs/page_cache.zig").summary();
        k.puts("[FSSTREAM] cache read/write/writeback/alloc/lock failures=");
        k.putDec(cache.read_errors);
        k.puts("/");
        k.putDec(cache.write_errors);
        k.puts("/");
        k.putDec(cache.writeback_errors);
        k.puts("/");
        k.putDec(cache.payload_allocation_failures);
        k.puts("/");
        k.putDec(cache.lock_timeouts);
        k.puts("\r\n");
}

// Every field/size/alignment of the private transports is checked by Rust too.
comptime {
    if(@sizeOf(NativeBytes)!=16 or @alignOf(NativeBytes)!=8) @compileError("R4SYS C NativeBytes size/alignment");
    if(@offsetOf(NativeBytes,"data")!=0) @compileError("R4SYS C NativeBytes.data");
    if(@offsetOf(NativeBytes,"len")!=8) @compileError("R4SYS C NativeBytes.len");
    if(@sizeOf(NativeTarget)!=24 or @alignOf(NativeTarget)!=8) @compileError("R4SYS C NativeTarget size/alignment");
    if(@offsetOf(NativeTarget,"drive_ref")!=0) @compileError("R4SYS C NativeTarget.drive_ref");
    if(@offsetOf(NativeTarget,"path")!=8) @compileError("R4SYS C NativeTarget.path");
    if(@sizeOf(StreamOwner)!=32 or @alignOf(StreamOwner)!=8) @compileError("R4SYS C StreamOwner size/alignment");
    if(@offsetOf(StreamOwner,"kind")!=0) @compileError("R4SYS C StreamOwner.kind");
    if(@offsetOf(StreamOwner,"id")!=4) @compileError("R4SYS C StreamOwner.id");
    if(@offsetOf(StreamOwner,"generation")!=8) @compileError("R4SYS C StreamOwner.generation");
    if(@offsetOf(StreamOwner,"program_id")!=16) @compileError("R4SYS C StreamOwner.program_id");
    if(@offsetOf(StreamOwner,"program_generation")!=24) @compileError("R4SYS C StreamOwner.program_generation");
    if(@sizeOf(FsFailureDiagnostic)!=20 or @alignOf(FsFailureDiagnostic)!=4) @compileError("R4SYS C FsFailureDiagnostic size/alignment");
    if(@offsetOf(FsFailureDiagnostic,"sequence")!=0) @compileError("R4SYS C FsFailureDiagnostic.sequence");
    if(@offsetOf(FsFailureDiagnostic,"task_id")!=4) @compileError("R4SYS C FsFailureDiagnostic.task_id");
    if(@offsetOf(FsFailureDiagnostic,"kind")!=8) @compileError("R4SYS C FsFailureDiagnostic.kind");
    if(@offsetOf(FsFailureDiagnostic,"result")!=12) @compileError("R4SYS C FsFailureDiagnostic.result");
    if(@offsetOf(FsFailureDiagnostic,"lookup_stage")!=16) @compileError("R4SYS C FsFailureDiagnostic.lookup_stage");
    _ = @import("../fs/directory_changes.zig");
}
