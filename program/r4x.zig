const keyboard = @import("../driver/input/keyboard.zig");
const audio = @import("../audio/core.zig");
const font = @import("../kernel/font.zig");
const drive = @import("../fs/drive.zig");
const vfs = @import("../fs/vfs.zig");
const fs_request = @import("../fs/request.zig");
const paging = @import("../memory/paging.zig");
const heap = @import("../memory/heap.zig");
const mem_blocks = @import("../memory/blocks.zig");
const mem_backing_store = @import("../memory/backing_store.zig");
const mem_virt = @import("../memory/virt.zig");
const module_file = @import("../kernel/module_file.zig");
const module_r4m = @import("../kernel/module_r4m.zig");
const modules = @import("../kernel/modules.zig");
const services = @import("../kernel/services.zig");
const crash = @import("../kernel/crash.zig");
const boot_perf = @import("../kernel/boot_perf.zig");
const boot_status = @import("../kernel/boot_status.zig");
const bootscreen = @import("../kernel/bootscreen.zig");
const bootlog = @import("../kernel/bootlog.zig");
const boot_config = @import("../kernel/boot_config.zig");
const k = @import("../kernel/log.zig");
const time_core = @import("../platform/time.zig");
const timer = @import("../kernel/timer.zig");
const interrupts = @import("../arch/x86_64/interrupts.zig");
const scheduler = @import("../sched/scheduler.zig");
const sync = @import("../sched/sync.zig");
const task = @import("../sched/task.zig");
const task_context = @import("../sched/task_context.zig");
const r4api = @import("r4api.zig");
const r4x_api = @import("r4x_api.zig");
const r4x_start = @import("r4x_start.zig");
const lifecycle_retire_policy = @import("lifecycle_retire_policy.zig");
const gui_alpha8 = @import("gui_alpha8.zig");
const desktop_events = @import("../kernel/desktop_events.zig");
const std = @import("std");
const ProgramAllocatorVTable = @import("std").mem.Allocator.VTable;
const ProgramAllocatorAlignment = @import("std").mem.Alignment;

const MAX_API_PATH: usize = r4api.r4sys.max_api_path;
// Modulpfad einer Instanz (0.61.13): Dateipfade sind laut Text-/Pfadvertrag
// hoechstens 127 Bytes - 128 reicht mit Abschluss, waehrend MAX_API_PATH
// (1024) das Payload-Budget sinnlos spraengen wuerde.
const MODULE_ORIGIN_MAX: usize = 128;
const MAX_API_ARGS: usize = 128;
const R4M_ENTRY_KIND_R4X: u32 = 1;
const DEFAULT_CONSOLE_FG: u32 = 0xD8D8D8;
const DEFAULT_CONSOLE_BG: u32 = 0x000000;
const R4X_FLAG_APP_CLASS_CONSOLE: u32 = 0x00000001;
const R4X_FLAG_APP_CLASS_GUI: u32 = 0x00000002;
const R4X_FLAG_APP_CLASS_SERVICE: u32 = 0x00000004;
const R4X_KNOWN_FLAGS: u32 = R4X_FLAG_APP_CLASS_CONSOLE | R4X_FLAG_APP_CLASS_GUI | R4X_FLAG_APP_CLASS_SERVICE;
const R4M_SECTION_FLAG_BSS: u32 = 0x00000008;
const R4M_SECTION_FLAG_ALLOC: u32 = 0x00000001;
const R4M_RELOC_ABS64: u32 = 1;
const R4M_RELOC_REL32: u32 = 2;
const R4M_RELOC_BASE_REL64: u32 = 3;
const R4M_RELOC_IMPORT_SLOT64: u32 = 4;
const R4X_MODULE_LINK_BASE: u64 = 0x00000004_00000000;
const R4X_SIGNED_LINK_BASE: u64 = 0xFFFF_FFFF_00000000;
const R4M_SECTION_FLAG_EXEC: u32 = 0x00000002;
const MAX_R4M_SECTIONS: usize = 16;
const MAX_R4M_IMPORTS: usize = 16;
const MAX_R4M_EXPORTS: usize = 16;
const MAX_R4M_METADATA_PROBE: usize = 2048;
const MAX_R4M_SYMBOL_PROBE: usize = 64;
const MAX_R4M_NAME_PROBE: usize = 128;
const R4XSTART_MAGIC = r4x_api.r4xstart_magic;
const R4XSTART_ABI_MAJOR = r4x_api.r4xstart_abi_major;
const R4XSTART_ABI_MINOR = r4x_api.r4xstart_abi_minor;
const R4XSTART_CONTEXT_SIZE = r4x_api.r4xstart_context_size;
const R4XSTART_IMPORT_SIZE = r4x_api.r4xstart_import_size;
const R4XSTART_R4SYS_MAGIC = r4x_api.r4xstart_r4sys_magic;
const R4XSTART_R4SYS_VERSION = r4x_api.r4xstart_r4sys_version;
const R4XSTART_R4SYS_SIZE = r4x_api.r4xstart_r4sys_size;
const R4XSTART_R4DESK_MAGIC = r4x_api.r4xstart_r4desk_magic;
const R4XSTART_R4DESK_VERSION = r4x_api.r4xstart_r4desk_version;
const R4XSTART_R4DESK_SIZE = r4x_api.r4xstart_r4desk_size;
const R4XSTART_R4DRAW_MAGIC = r4x_api.r4xstart_r4draw_magic;
const R4XSTART_R4DRAW_VERSION = r4x_api.r4xstart_r4draw_version;
const R4XSTART_R4DRAW_SIZE = r4x_api.r4xstart_r4draw_size;
const R4XSTART_R4NET_MAGIC = r4x_api.r4xstart_r4net_magic;
const R4XSTART_R4NET_VERSION = r4x_api.r4xstart_r4net_version;
const R4XSTART_R4NET_SIZE = r4x_api.r4xstart_r4net_size;
const R4XSTART_R4AUDIO_MAGIC = r4x_api.r4xstart_r4audio_magic;
const R4XSTART_R4AUDIO_VERSION = r4x_api.r4xstart_r4audio_version;
const R4XSTART_R4AUDIO_SIZE = r4x_api.r4xstart_r4audio_size;
const R4XSTART_R4DEV_MAGIC = r4x_api.r4xstart_r4dev_magic;
const R4XSTART_R4DEV_VERSION = r4x_api.r4xstart_r4dev_version;
const R4XSTART_R4DEV_SIZE = r4x_api.r4xstart_r4dev_size;
const R4XSTART_FLAG_IMPORTS_VALID = r4x_api.r4xstart_flag_imports_valid;
const R4XSTART_FLAG_CLOSE_SUPPORTED = r4x_api.r4xstart_flag_close_supported;
const R4XSTART_FLAG_YIELD_SUPPORTED = r4x_api.r4xstart_flag_yield_supported;
const R4XSTART_IMPORT_FLAG_GROUP_INTERFACE = r4x_api.r4xstart_import_flag_group_interface;
const R4XSTART_IMPORT_NAME_BYTES: usize = 32;
const R4L_GROUP_R4SYS: u32 = @intFromEnum(r4x_api.R4LGroup.r4sys);
const R4L_GROUP_R4DESK: u32 = @intFromEnum(r4x_api.R4LGroup.r4desk);
const R4L_GROUP_R4DRAW: u32 = @intFromEnum(r4x_api.R4LGroup.r4draw);
const R4L_GROUP_R4NET: u32 = @intFromEnum(r4x_api.R4LGroup.r4net);
const R4L_GROUP_R4AUDIO: u32 = @intFromEnum(r4x_api.R4LGroup.r4audio);
const R4L_GROUP_R4DEV: u32 = @intFromEnum(r4x_api.R4LGroup.r4dev);
const ARGS_MAX: usize = 127;
const KB: u64 = 1024;
const MB: u64 = 1024 * KB;
const GB: u64 = 1024 * MB;
// 0.69.59: Canary-Hochwasser im Standard-Gast: normal 139256 Byte,
// desktop 241344 Byte, service 48560 Byte, large-service 202456 Byte und
// build-tool 176232 Byte. Die verkleinerten Reserven behalten mindestens
// rund Faktor 16 zum beobachteten Profilmaximum. Browser/Workstation bleiben
// ohne repraesentativen Lauf bewusst unveraendert; Commit waechst weiter in
// kontrollierten 64-KB-Schritten hinter dem wandernden Guard.
const PROGRAM_STACK_RESERVE_SIZE: u64 = 4 * MB;
const PROGRAM_STACK_LARGE_RESERVE_SIZE: u64 = 8 * MB;
const PROGRAM_STACK_INITIAL_COMMIT_SIZE: u64 = 64 * KB;
const PROGRAM_STACK_SERVICE_INITIAL_COMMIT_SIZE: u64 = 64 * KB;
const PROGRAM_STACK_GROW_SIZE: u64 = 64 * 1024;
const PROGRAM_STACK_GUARD_SIZE: u64 = paging.PAGE_SIZE;
const PAGE_FAULT_PRESENT: u64 = 1 << 0;
const PROGRAM_REGISTRY_CHUNK_SLOTS: usize = 16;
const PROGRAM_REGISTRY_WARM_CHUNKS: usize = 2;
const PROGRAM_REGISTRY_SHRINK_HYSTERESIS: u64 = 64;
const PROGRAM_REGISTRY_PRESSURE_SHRINK_HYSTERESIS: u64 = 8;
const MAX_ASYNC_IO_REQUESTS: usize = 64;
const ASYNC_IO_SYNC_CLOSE_RETRY_LIMIT: u32 = 64;
const ASYNC_IO_RETIRE_RETRY_TEST_ATTEMPTS: u32 = ASYNC_IO_SYNC_CLOSE_RETRY_LIMIT + 1;
const INPUT_QUEUE_SIZE: usize = 4096;
const GUI_EVENT_QUEUE_SIZE: usize = 64;
const GUI_EVENT_KIND_MOUSE_MOVE: u32 = 5;
const GUI_EVENT_KIND_FONT_CHANGED: u32 = 7;
const GUI_TEXT_SIZE: usize = 512;
// Display lists grow through independently owned blocks.  The per-block
// target controls allocation granularity only; it is never an aggregate
// command limit.
const GUI_COMMAND_BLOCK_INITIAL_CAPACITY: u32 = 8;
const GUI_COMMAND_BLOCK_TARGET_CAPACITY: u32 = 128;
const GUI_RESOURCE_BLOCK_TARGET_BYTES: usize = 64 * 1024;
// The old NUL-pointer text ABI has no caller-supplied length.  Bound that one
// scan to 1 MB for technical stability.  Explicit-length frame resources and
// the aggregate frame have no corresponding fixed capacity.
const GUI_LEGACY_TEXT_APPEND_MAX_BYTES: usize = 1024 * 1024;
const GUI_COMMAND_KIND_ALPHA8: u32 = 5;
const GUI_RASTER_MAX_WIDTH: u32 = 128;
const GUI_RASTER_MAX_HEIGHT: u32 = 128;
const GUI_RASTER_MAX_PIXELS: usize = @as(usize, GUI_RASTER_MAX_WIDTH) * @as(usize, GUI_RASTER_MAX_HEIGHT);
const GUI_ALPHA8_MAX_WIDTH: u32 = r4x_api.gui_alpha8_max_width;
const GUI_ALPHA8_MAX_HEIGHT: u32 = r4x_api.gui_alpha8_max_height;
const GUI_ALPHA8_MAX_PIXELS: usize = r4x_api.gui_alpha8_max_pixels;
const GUI_ARGB32_MAX_WIDTH: u32 = r4x_api.gui_argb32_max_width;
const GUI_ARGB32_MAX_HEIGHT: u32 = r4x_api.gui_argb32_max_height;
const GUI_ARGB32_MAX_PIXELS: usize = r4x_api.gui_argb32_max_pixels;
// A single legacy raster command remains bounded by its public ABI contract.
// There is no separate fixed aggregate raster buffer or aggregate word cap:
// every successful raster append owns one separately allocated resource node.
const GUI_RASTER_NODE_MAX_WORDS: usize = @max(GUI_RASTER_MAX_PIXELS, (GUI_ALPHA8_MAX_PIXELS + 3) / 4);
const GUI_TITLE_SIZE: usize = 64;
const CONSOLE_OUTPUT_SIZE: usize = 16 * 1024;
const CONSOLE_TRANSCRIPT_SEGMENTS: usize = 256;
const CONSOLE_MIN_COLS: u32 = 1;
const CONSOLE_MAX_COLS: u32 = 512;
const CONSOLE_MIN_ROWS: u32 = 1;
const CONSOLE_MAX_ROWS: u32 = 256;
const GUI_FONT_BUILTIN_ID: u32 = r4api.r4draw.gui_font_builtin_id;
const GUI_FONT_FLAG_RENDERABLE: u32 = r4api.r4draw.gui_font_flag_renderable;
const GUI_FONT_FLAG_SELECTED: u32 = r4api.r4draw.gui_font_flag_selected;
const GUI_FONT_FLAG_BUILTIN: u32 = r4api.r4draw.gui_font_flag_builtin;
const ENVIRONMENT_NAME_MAX: usize = 32;
const ENVIRONMENT_VALUE_MAX: usize = 512;
const ENVIRONMENT_BLOCK_MAX: usize = 2048;
const ENV_OK: i32 = 0;
const ENV_ERROR_INVALID: i32 = -1;
const ENV_ERROR_NOT_FOUND: i32 = -2;
const ENV_ERROR_BUFFER_TOO_SMALL: i32 = -3;
const ENV_ERROR_UNSUPPORTED: i32 = -4;
const ENV_ERROR_TOO_LONG: i32 = -5;
const ENV_ERROR_NO_MEMORY: i32 = -6;

pub const RunResult = enum {
    ran,
    not_found,
    failed,
};

pub const INSTANCE_CLASS_SERVICE: u8 = 2;

pub const InstanceSnapshot = struct {
    id: u32 = 0,
    task_id: u32 = 0,
    app_class: u8 = 0,
    state: u8 = 0,
    close_requested: bool = false,
    exit_code: i32 = 0,
};

const THREAD_OK: i32 = 0;
const THREAD_ERROR_INVALID: i32 = -1;
const THREAD_ERROR_NO_INSTANCE: i32 = -2;
const THREAD_ERROR_NO_SLOTS: i32 = -3;
const THREAD_ERROR_NO_MEMORY: i32 = -4;
const THREAD_ERROR_NOT_FOUND: i32 = -5;
const THREAD_ERROR_SELF_JOIN: i32 = -6;
const THREAD_ERROR_BUSY: i32 = -7;
const THREAD_ERROR_TIMEOUT: i32 = -8;
const THREAD_ERROR_NOT_JOINABLE: i32 = -9;
const THREAD_ERROR_UNSUPPORTED: i32 = -10;
const THREAD_INFO_VERSION: u32 = 1;
const THREAD_CREATE_FLAGS_SUPPORTED: u32 = 0;
const THREAD_FLAG_MAIN: u32 = 0x0000_0001;
const THREAD_FLAG_JOINABLE: u32 = 0x0000_0002;
const THREAD_FLAG_JOINED: u32 = 0x0000_0004;

const IO_OK: i32 = 0;
const IO_ERROR_INVALID: i32 = -1;
const IO_ERROR_NO_INSTANCE: i32 = -2;
const IO_ERROR_NO_SLOTS: i32 = -3;
const IO_ERROR_SPAWN_FAILED: i32 = -4;
const IO_ERROR_NOT_FOUND: i32 = -5;
const IO_ERROR_TIMEOUT: i32 = -6;
const IO_ERROR_BUSY: i32 = -7;
const IO_ERROR_UNSUPPORTED: i32 = -8;
const IO_ERROR_CANCELLED: i32 = -9;
const IO_ERROR_TOO_LARGE: i32 = -10;
const IO_ERROR_LOCK_VIOLATION: i32 = r4x_api.io_error_lock_violation;
const IO_INFO_VERSION: u32 = 1;
const IO_FLAGS_SUPPORTED: u32 = 0;
const IO_FILE_LOCK_FLAG_UNLOCK: u32 = r4x_api.io_file_lock_flag_unlock;

const MEM_ERROR_RETIRED: i32 = -8;
const VM_PROBE_OK: i32 = 0;
const VM_PROBE_ERROR_NO_INSTANCE: i32 = -1;
const VM_PROBE_ERROR_INVALID_SIZE: i32 = -2;
const VM_PROBE_ERROR_TABLE_FULL: i32 = -3;
const VM_PROBE_ERROR_NO_SPACE: i32 = -4;
const VM_PROBE_ERROR_RANGE_MISSING: i32 = -5;
const VM_PROBE_ERROR_BLOCK_MISSING: i32 = -6;
const VM_PROBE_ERROR_RELEASE_FAILED: i32 = -7;
const VM_PROBE_ERROR_INTERNAL: i32 = -8;
const VM_OK: i32 = 0;
const VM_ERROR_INVALID_RANGE: i32 = -1;
const VM_ERROR_INVALID_ALIGNMENT: i32 = -2;
const VM_ERROR_OWNER_MISMATCH: i32 = -3;
const VM_ERROR_TABLE_FULL: i32 = -4;
const VM_ERROR_NO_SPACE: i32 = -5;
const VM_ERROR_OUT_OF_MEMORY: i32 = -6;
const VM_ERROR_ALREADY_COMMITTED: i32 = -7;
const VM_ERROR_NOT_COMMITTED: i32 = -8;
const VM_ERROR_GUARD_RANGE: i32 = -9;
const VM_ERROR_MAP_FAILED: i32 = -10;
const VM_ERROR_UNSUPPORTED_FLAGS: i32 = -11;
const VM_ERROR_NO_INSTANCE: i32 = -12;
const VM_ERROR_LIMIT_EXCEEDED: i32 = -13;
const VM_REGION_FLAG_WRITABLE: u64 = 1 << 0;
const VM_REGION_FLAG_EXECUTABLE: u64 = 1 << 1;
const VM_REGION_ALLOWED_FLAGS: u64 = VM_REGION_FLAG_WRITABLE | VM_REGION_FLAG_EXECUTABLE;

const ProgramMemoryStats = extern struct {
    base: u64 = 0,
    reserved_bytes: u64 = 0,
    committed_bytes: u64 = 0,
    used_bytes: u64 = 0,
    free_bytes: u64 = 0,
    largest_free: u64 = 0,
    active_blocks: u32 = 0,
    free_blocks: u32 = 0,
    reserved0: u32 = 0,
    error_flags: u32 = 0,
    allocation_errors: u64 = 0,
    invalid_free_errors: u64 = 0,
    size_mismatch_errors: u64 = 0,
    oom_errors: u64 = 0,
    owner_mismatch_errors: u64 = 0,
};

const ProgramMemorySummary = r4x_api.ProgramMemorySummary;
const ProgramMemoryBlockInfo = r4x_api.ProgramMemoryBlockInfo;
const ProgramVmReserveProbe = r4x_api.ProgramVmReserveProbe;
const ProgramMemoryPressureSnapshot = r4x_api.ProgramMemoryPressureSnapshot;
const ProgramMemoryReclaimProbe = r4x_api.ProgramMemoryReclaimProbe;
const ProgramMemoryBackingStoreProbe = r4x_api.ProgramMemoryBackingStoreProbe;
const ProgramMemoryBackingStoreSlotProbe = r4x_api.ProgramMemoryBackingStoreSlotProbe;
const ProgramMemoryPagerGateProbe = r4x_api.ProgramMemoryPagerGateProbe;
const ProgramMemoryPageIoProbe = r4x_api.ProgramMemoryPageIoProbe;
const ProgramMemoryVmPageStateProbe = r4x_api.ProgramMemoryVmPageStateProbe;
const ProgramPerformanceSummary = r4x_api.ProgramPerformanceSummary;
const ProgramTaskPerformanceInfo = r4x_api.ProgramTaskPerformanceInfo;
const ProgramStoragePerformanceInfo = r4x_api.ProgramStoragePerformanceInfo;
const ProgramBootPhasePerformanceInfo = r4x_api.ProgramBootPhasePerformanceInfo;
const ProgramVmRegionInfo = r4x_api.ProgramVmRegionInfo;
const BootInfoSummary = r4x_api.BootInfoSummary;
const BootInfoMemoryEntry = r4x_api.BootInfoMemoryEntry;
const ServiceInfo = r4x_api.ServiceInfo;
const ServiceDetail = r4x_api.ServiceDetail;
const ServiceMessageHeader = r4x_api.ServiceMessageHeader;
pub const MouseApiState = r4api.r4desk.MouseApiState;
const KeyboardLayoutInfo = r4x_api.KeyboardLayoutInfo;
const ProgramStatus = r4x_api.ProgramStatus;
const ProgramInstanceInfo = r4x_api.ProgramInstanceInfo;
const ProgramProcessHandle = r4x_api.ProgramProcessHandle;
const ProgramProcessCompletion = r4x_api.ProgramProcessCompletion;
const ProgramThreadInfo = r4x_api.ProgramThreadInfo;
const ProgramJoinHandle = r4x_api.ProgramJoinHandle;
const ProgramInventoryCursor = r4x_api.ProgramInventoryCursor;
const ProgramInventoryPageInfo = r4x_api.ProgramInventoryPageInfo;
const ProgramInstanceSnapshot = r4x_api.ProgramInstanceSnapshot;
const ProgramTaskSnapshot = r4x_api.ProgramTaskSnapshot;
const ProgramThreadSnapshot = r4x_api.ProgramThreadSnapshot;
const ProgramInventorySummary = r4x_api.ProgramInventorySummary;
const ProgramIoInfo = r4x_api.ProgramIoInfo;
const DeviceInventorySummary = r4x_api.DeviceInventorySummary;
const DeviceInventoryRecord = r4x_api.DeviceInventoryRecord;
const ProtocolStatus = r4x_api.ProtocolStatus;
const ProtocolBuffer = r4x_api.ProtocolBuffer;
const DisplaySummary = r4x_api.DisplaySummary;
const HardwareSummary = r4x_api.HardwareSummary;
const IpcSummary = r4x_api.IpcSummary;
const IpcChannelInfo = r4x_api.IpcChannelInfo;
const TcpSummary = r4x_api.TcpSummary;
const TcpConnectionInfo = r4x_api.TcpConnectionInfo;
const TcpAcceptResult = r4x_api.TcpAcceptResult;
const NetIpv4Packet = r4x_api.NetIpv4Packet;
const UdpRecvInfo = r4api.r4net.UdpRecvInfo;
const UdpStatus = r4api.r4net.UdpStatus;
const SerialLinkStatus = r4x_api.SerialLinkStatus;
const SerialLinkMessage = r4x_api.SerialLinkMessage;
const NetConfigSnapshot = r4x_api.NetConfigSnapshot;
const NetConfigRequest = r4x_api.NetConfigRequest;
const DhcpStatus = r4x_api.DhcpStatus;
const NetDetailSnapshot = r4x_api.NetDetailSnapshot;
const NetDiagResult = r4x_api.NetDiagResult;
const DriveInfo = r4x_api.DriveInfo;
const FileInfo = r4x_api.FileInfo;
const BootLogInfo = r4x_api.BootLogInfo;
const RegistryKeyInfo = r4x_api.RegistryKeyInfo;
const RegistryValueInfo = r4x_api.RegistryValueInfo;
const ProgramInstanceFlag = struct {
    const close_requested: u8 = 1;
    const desktop_requested: u8 = 2;
    const terminal_mode: u8 = 4;
};

const PROGRAM_HANDLE_OK: i32 = 0;
const PROGRAM_HANDLE_ERROR_INVALID: i32 = -1;
const PROGRAM_HANDLE_ERROR_NOT_FOUND: i32 = -2;
const PROGRAM_HANDLE_ERROR_STALE: i32 = -3;
const PROGRAM_HANDLE_ERROR_NOT_RUNNING: i32 = -4;
const PROGRAM_HANDLE_ERROR_WOULD_BLOCK: i32 = -5;
const PROGRAM_HANDLE_ERROR_TIMEOUT: i32 = -6;
const PROGRAM_HANDLE_ERROR_SELF: i32 = -7;
const PROGRAM_HANDLE_ERROR_NO_MEMORY: i32 = -8;
const PROGRAM_HANDLE_ERROR_LOAD_FAILED: i32 = -9;
const PROGRAM_HANDLE_ERROR_TASK_FAILED: i32 = -10;
const PROGRAM_HANDLE_ERROR_GENERATION_EXHAUSTED: i32 = -11;
const PROGRAM_HANDLE_ERROR_OUTPUT_UNAVAILABLE: i32 = -12;
const PROGRAM_HANDLE_ERROR_OUTPUT_RANGE: i32 = -13;

const PROGRAM_COMPLETION_FLAG_READY: u32 = 1 << 0;
const PROGRAM_COMPLETION_FLAG_OUTPUT: u32 = 1 << 1;
const PROGRAM_COMPLETION_FLAG_DISPLAY_USED: u32 = 1 << 2;
const PROGRAM_COMPLETION_FLAG_OWNER: u32 = 1 << 3;

const PROGRAM_EXIT_REASON_NATURAL: u8 = 0;
const PROGRAM_EXIT_REASON_CLOSE: u8 = 1;
const PROGRAM_EXIT_REASON_KILLED: u8 = 2;
const PROGRAM_EXIT_REASON_FAILED: u8 = 3;

pub const ConsoleHostKind = enum(u32) {
    none = 0,
    terminal_window = 1,
    terminal_mode = 2,
};

const ConsoleStream = enum(u32) {
    stdin = 0,
    stdout = 1,
    stderr = 2,
};

const GuiWindowInfo = r4x_api.GuiWindowInfo;
const GuiSize = r4x_api.GuiSize;
const GuiEvent = r4x_api.GuiEvent;
const GuiCommand = r4x_api.GuiCommand;
const GuiFrameCommand = r4x_api.GuiFrameCommand;
const GuiFrameGenerationInfo = r4x_api.GuiFrameGenerationInfo;
const GuiFrameInfo = r4x_api.GuiFrameInfo;
const GuiIndexed8Resource = r4x_api.GuiIndexed8Resource;
const DisplayDamageRect = r4x_api.DisplayDamageRect;
const GuiPathSegment = r4x_api.GuiPathSegment;
const GuiShapeResource = r4x_api.GuiShapeResource;
const GuiFontInfo = r4x_api.GuiFontInfo;
const GuiTextMetrics = r4x_api.GuiTextMetrics;
const ClipboardInfo = r4x_api.ClipboardInfo;
const ProgramHostLaunchRequest = r4x_api.ProgramHostLaunchRequest;
pub const ConsoleState = r4x_api.ConsoleState;
pub const OutputCaptureResult = struct {
    len: usize,
    truncated: bool,
};

const R4MSection = struct {
    flags: u32,
    file_off: u32,
    file_size: u32,
    mem_size: u32,
    alignment: u32,
};

/// Non-alloc-Sections (.rsrc, seit 0.61.12) existieren nur in der DATEI.
/// Sie bekommen keinen Platz im Programmimage, und kein Entry, Export oder
/// Relocationsziel darf auf sie zeigen.
fn r4mSectionLoadable(section: R4MSection) bool {
    return (section.flags & R4M_SECTION_FLAG_ALLOC) != 0;
}

const R4MEntry = struct {
    kind: u32,
    section: u32,
    offset: u32,
    flags: u32,
};

const R4MImport = struct {
    module: []const u8,
    symbol: []const u8,
    min_version: u32,
    flags: u32,
};

const R4MRelocation = struct {
    kind: u32,
    patch_section: u32,
    patch_offset: u32,
    target_section: u32,
    target_offset: u32,
    addend: i32,
};

const ProgramFile = struct {
    volume: vfs.Volume,
    entry: vfs.Entry,
    drive_letter: u8,
    /// Aufgeloester Startpfad als DOS-Pfad (L:\\...), fuer
    /// program_module_path - ein Programm liest damit seine eigenen
    /// eingebetteten Ressourcen ohne hartkodierte zweite Wahrheit.
    origin: [MODULE_ORIGIN_MAX]u8 = .{0} ** MODULE_ORIGIN_MAX,
    origin_len: u16 = 0,
};

const ProgramImage = struct {
    range_id: u32,
    code: []u8,
    owner_id: u32,
};

const ResolvedR4MImport = struct {
    address: u64 = 0,
    version: u32 = 0,
    generation: u32 = 0,
};

const R4XStartImportSeed = struct {
    group_id: u32 = 0,
    min_version: u32 = 0,
    resolved_version: u32 = 0,
    flags: u32 = 0,
    table: u64 = 0,
    module_name: [R4XSTART_IMPORT_NAME_BYTES]u8 = .{0} ** R4XSTART_IMPORT_NAME_BYTES,
    module_name_len: usize = 0,
    symbol_name: [R4XSTART_IMPORT_NAME_BYTES]u8 = .{0} ** R4XSTART_IMPORT_NAME_BYTES,
    symbol_name_len: usize = 0,
};

const R4XStartContext = r4x_api.R4XStartContext;
const R4XStartImport = r4x_api.R4XStartImport;
const R4XStartR4Sys = r4x_api.R4XStartR4Sys;
const R4XStartR4Desk = r4x_api.R4XStartR4Desk;
const R4XStartR4Draw = r4x_api.R4XStartR4Draw;
const R4XStartR4Net = r4x_api.R4XStartR4Net;
const R4XStartR4Audio = r4x_api.R4XStartR4Audio;
const R4XStartR4Dev = r4x_api.R4XStartR4Dev;

const LoadedProgram = struct {
    image: ProgramImage,
    entry: RawEntryFn,
    memory_contract: ProgramMemoryContract = .{},
    imports: [MAX_R4M_IMPORTS]R4XStartImportSeed = .{R4XStartImportSeed{}} ** MAX_R4M_IMPORTS,
    import_count: u32 = 0,
    loader_section_count: u32 = 0,
    loader_relocation_count: u32 = 0,
    /// Aufgeloester Startpfad aus ProgramFile.origin, siehe dort.
    origin: [MODULE_ORIGIN_MAX]u8 = .{0} ** MODULE_ORIGIN_MAX,
    origin_len: u16 = 0,
};

const MemoryProfile = enum(u8) {
    unknown = 0,
    tiny = 1,
    normal = 2,
    desktop = 3,
    service = 4,
    large_service = 5,
    build_tool = 6,
    browser = 7,
    workstation = 8,
};
const MEMORY_PROFILE_COUNT: usize = 9;
const PROGRAM_STACK_HIGH_WATER_PATTERN: u8 = 0xA5;

const ProgramStackTelemetryStats = struct {
    creates: u64 = 0,
    releases: u64 = 0,
    reserve_max_bytes: u64 = 0,
    initial_commit_max_bytes: u64 = 0,
    committed_max_bytes: u64 = 0,
    high_water_max_bytes: u64 = 0,
    create_cycles_total: u64 = 0,
    create_cycles_max: u64 = 0,
    release_cycles_total: u64 = 0,
    release_cycles_max: u64 = 0,
};

const ProgramStack = struct {
    range_id: u32 = 0,
    base: u64 = 0,
    reserve_size: u64 = 0,
    committed_base: u64 = 0,
    committed_size: u64 = 0,
    guard_base: u64 = 0,
    guard_size: u64 = 0,
    top: u64 = 0,
    owner_id: u32 = 0,
    profile: MemoryProfile = .unknown,
    initial_commit_size: u64 = 0,
    create_cycles: u64 = 0,
    telemetry_high_water: u64 = 0,
    telemetry_committed_pages: u32 = 0,
    serial_telemetry: bool = false,
    telemetry_measured: bool = false,
};

const ProgramResources = struct {
    image_range_id: u32 = 0,
    image_base: u64 = 0,
    image_size: usize = 0,
    image_owner_id: u32 = 0,
    stack: ProgramStack = .{},
};

const RawEntryFn = *const fn (usize) callconv(.c) i32;

extern fn r4os_call_program(entry: RawEntryFn, arg: usize, stack_top: u64) callconv(.c) i32;

const AppClass = enum(u8) {
    console,
    gui,
    service,
};

const ProgramMemoryLimits = struct {
    vm_reserve_limit: u64 = 1024 * MB,
    vm_commit_limit: u64 = 256 * MB,
    resident_limit: u64 = 256 * MB,
    stack_reserve: u64 = PROGRAM_STACK_RESERVE_SIZE,
    stack_initial_commit: u64 = PROGRAM_STACK_INITIAL_COMMIT_SIZE,
};

const ProgramMemoryContract = struct {
    profile: MemoryProfile = .normal,
    limits: ProgramMemoryLimits = .{},
    tag: [16]u8 = .{0} ** 16,
};

const LaunchPolicy = enum(u32) {
    auto = 0,
    console = 1,
    gui = 2,
};

const InstanceRole = enum(u8) {
    foreground,
    shell,
    background,
};

const InstanceState = enum(u8) {
    running,
    close_requested,
    done,
};

const ProgramThreadState = enum(u32) {
    unused = 0,
    ready = 1,
    running = 2,
    exited = 3,
    killed = 4,
};

const ProgramThread = struct {
    registry_prev: ?*ProgramThread = null,
    registry_next: ?*ProgramThread = null,
    used: bool = false,
    pin_count: u32 = 0,
    retire_pending: bool = false,
    retire_in_progress: bool = false,
    retire_for_instance: bool = false,
    task_detached: bool = false,
    stack_released: bool = false,
    id: u32 = 0,
    generation: u64 = 0,
    instance_id: u32 = 0,
    instance_generation: u64 = 0,
    execution_pinned: bool = false,
    spawn_transaction_depth: u32 = 0,
    exit_deferred: bool = false,
    // Stable chunk-backed owner address.  IRQ/exception paths must never
    // acquire the sleepable dynamic-registry mutex merely to classify the
    // currently executing R4X task.
    owner_instance: ?*ProgramInstance = null,
    task_id: u32 = 0,
    task_generation: u64 = 0,
    state: ProgramThreadState = .unused,
    flags: u32 = 0,
    entry: RawEntryFn = undefined,
    arg: usize = 0,
    stack: ProgramStack = .{},
    exit_code: i32 = 0,
    created_tick: u64 = 0,
    finished_tick: u64 = 0,
    join_count: u32 = 0,
    join_owner_task_id: u32 = 0,
    join_owner_task_generation: u64 = 0,
    join_lease_active: bool = false,
    join_waiter_refs: u32 = 0,
    join_queue: sync.WaitQueue = sync.WaitQueue.init(),
};

const AsyncIoKind = enum(u32) {
    none = 0,
    file_read = 1,
    file_read_at = 2,
    file_write = 3,
    file_append = 4,
    file_stream_begin = 5,
    file_stream_write = 6,
    file_stream_finish = 7,
    file_stream_abort = 8,
    service_call = 9,
    file_write_at = 10,
    file_info = 11,
    file_lock = 12,
};

const AsyncIoState = enum(u32) {
    unused = 0,
    pending = 1,
    running = 2,
    completed = 3,
    failed = 4,
};

const AsyncIoRequest = struct {
    used: bool = false,
    id: u32 = 0,
    owner_instance_id: u32 = 0,
    owner_instance_generation: u64 = 0,
    // The request is cancelled and cleared before its owner can be reclaimed,
    // so this stable address is safe for lock-free fault-context attribution.
    owner_instance: ?*ProgramInstance = null,
    // Stable identity of the ProgramThread that submitted this request. The
    // short-lived kernel worker below must not collapse independent callers
    // from one process into a shared R4SYS stream owner.
    caller_task_id: u32 = 0,
    caller_task_generation: u64 = 0,
    task_id: u32 = 0,
    task_generation: u64 = 0,
    kind: AsyncIoKind = .none,
    state: AsyncIoState = .unused,
    cancel_requested: bool = false,
    close_pending: bool = false,
    waiters: u32 = 0,
    flags: u32 = 0,
    status: i32 = IO_OK,
    result: i32 = 0,
    requested_bytes: u64 = 0,
    processed_bytes: u64 = 0,
    submitted_tick: u64 = 0,
    completed_tick: u64 = 0,
    completion: sync.Completion = sync.Completion.init(),
    path: [MAX_API_PATH]u8 = .{0} ** MAX_API_PATH,
    path_len: usize = 0,
    offset: u64 = 0,
    data_ptr: usize = 0,
    data_len: u64 = 0,
    out_ptr: usize = 0,
    out_len: u64 = 0,
    service_handle: u32 = 0,
    service_op: u16 = 0,
    service_request_ptr: usize = 0,
    service_request_len: u32 = 0,
    service_response_header_ptr: usize = 0,
    service_response_ptr: usize = 0,
    service_response_capacity: u32 = 0,
    service_timeout_ticks: u64 = 0,
    service_request_id: u32 = 0,
};

const MAX_FILE_RANGE_LOCKS: usize = 64;

const FileRangeLock = struct {
    used: bool = false,
    owner_instance_id: u32 = 0,
    owner_instance_generation: u64 = 0,
    offset: u64 = 0,
    length: u64 = 0,
    path: [MAX_API_PATH]u8 = .{0} ** MAX_API_PATH,
    path_len: usize = 0,
};

const AsyncIoRetireClaim = struct {
    slot: usize,
    request_id: u32,
    owner_instance_id: u32,
    owner_instance_generation: u64,
    caller_task_id: u32,
    caller_task_generation: u64,
    task_id: u32,
    task_generation: u64,
    service_handle: u32 = 0,
    service_request_id: u32 = 0,
};

const PROGRAM_PAYLOAD_MAGIC: u32 = 0x5034_3452;

const ProgramPayloadKind = enum(u16) {
    runtime = 1,
    process = 2,
    environment = 3,
    console = 4,
    console_output = 5,
    gui = 6,
    gui_commands = 7,
    gui_raster = 8,
    gui_frame = 9,
    gui_frame_data = 10,
    console_transcript = 11,
};

const ProgramPayloadHeader = struct {
    magic: u32 = 0,
    owner_id: u32 = 0,
    requested_bytes: u32 = 0,
    kind: ProgramPayloadKind = .runtime,
    reserved: u16 = 0,
};

const ProgramRuntimePayload = struct {
    header: ProgramPayloadHeader = .{},
    r4xstart_context: R4XStartContext = .{},
    r4xstart_imports: [MAX_R4M_IMPORTS]R4XStartImport = .{R4XStartImport{}} ** MAX_R4M_IMPORTS,
    r4xstart_import_count: u32 = 0,
    r4xstart_import_module_names: [MAX_R4M_IMPORTS][R4XSTART_IMPORT_NAME_BYTES]u8 = .{.{0} ** R4XSTART_IMPORT_NAME_BYTES} ** MAX_R4M_IMPORTS,
    r4xstart_import_symbol_names: [MAX_R4M_IMPORTS][R4XSTART_IMPORT_NAME_BYTES]u8 = .{.{0} ** R4XSTART_IMPORT_NAME_BYTES} ** MAX_R4M_IMPORTS,
    module_path: [MODULE_ORIGIN_MAX]u8 = .{0} ** MODULE_ORIGIN_MAX,
    module_path_len: u32 = 0,
};

const ProgramEnvironmentPayload = struct {
    header: ProgramPayloadHeader = .{},
    bytes: [ENVIRONMENT_BLOCK_MAX]u8 = .{0} ** ENVIRONMENT_BLOCK_MAX,
};

const ProgramProcessPayload = struct {
    header: ProgramPayloadHeader = .{},
    environment_payload: ?*ProgramEnvironmentPayload = null,
    environment_len: usize = 0,
    args: [ARGS_MAX + 1]u8 = .{0} ** (ARGS_MAX + 1),
    work_drive_letter: u8 = 'C',
    work_cwd_len: usize = 1,
    work_cwd: [drive.MAX_PATH]u8 = .{0} ** drive.MAX_PATH,
};

const ProgramConsoleOutputPayload = struct {
    header: ProgramPayloadHeader = .{},
    ref_count: u32 = 0,
    active_ref_count: u32 = 0,
    next_sequence: u64 = 0,
    active_accounted: bool = false,
    reserved: [7]u8 = .{0} ** 7,
    bytes: [CONSOLE_OUTPUT_SIZE]u8 = .{0} ** CONSOLE_OUTPUT_SIZE,
};

const ProgramConsoleOutputSegment = struct {
    payload: ?*ProgramConsoleOutputPayload = null,
    start_sequence: u64 = 0,
    length: u32 = 0,
    reserved: u32 = 0,
};

const ProgramConsoleTranscriptPayload = struct {
    header: ProgramPayloadHeader = .{},
    segment_count: u32 = 0,
    output_len: u32 = 0,
    segments: [CONSOLE_TRANSCRIPT_SEGMENTS]ProgramConsoleOutputSegment = .{ProgramConsoleOutputSegment{}} ** CONSOLE_TRANSCRIPT_SEGMENTS,
};

const ProgramConsolePayload = struct {
    header: ProgramPayloadHeader = .{},
    transcript_payload: ?*ProgramConsoleTranscriptPayload = null,
    writer_payload: ?*ProgramConsoleOutputPayload = null,
    input_queue: [INPUT_QUEUE_SIZE]u8 = .{0} ** INPUT_QUEUE_SIZE,
    input_head: usize = 0,
    input_tail: usize = 0,
    input_high_water: u32 = 0,
    input_generation: u64 = 0,
    input_lock: sync.Mutex = sync.Mutex.initClass("console-input", sync.LockRank.program_instances, .sleepable),
    input_wait: sync.WaitQueue = sync.WaitQueue.init(),
    revision: u32 = 0,
    host: ConsoleHostKind = .none,
    io_target_id: u32 = 0,
    io_target_generation: u64 = 0,
    state: ConsoleState = .{},
};

const ProgramGuiCommandResourceKind = enum(u16) {
    none = 0,
    utf8 = 1,
    xrgb32 = 2,
    alpha8 = 3,
    path = 4,
    indexed8 = 5,
};

// Kernel-owned display-list truth.  The legacy GuiCommand ABI is materialized
// from this record on demand; its 64-byte inline text field is not storage.
const ProgramGuiCommand = struct {
    version: u32 = 1,
    size: u32 = 96,
    kind: u32 = 0,
    flags: u32 = 0,
    x: i32 = 0,
    y: i32 = 0,
    w: u32 = 0,
    h: u32 = 0,
    rgb: u32 = 0,
    fg: u32 = 0,
    bg: u32 = 0,
    font_id: u32 = 0,
    text_w: u32 = 0,
    text_h: u32 = 0,
    baseline: i32 = 0,
    line_height: u32 = 0,
    payload_offset: u64 = 0,
    payload_bytes: u64 = 0,
    parameter0: u64 = 0,
    parameter1: u64 = 0,
    resource_kind: ProgramGuiCommandResourceKind = .none,
    reserved: u16 = 0,
    raster_word_offset: u64 = 0,
};

// Variable-sized command block.  Commands follow the header in the same heap
// allocation.  Capacity is allocation granularity, never a display-list cap.
const ProgramGuiCommandPayload = struct {
    header: ProgramPayloadHeader = .{},
    previous: ?*ProgramGuiCommandPayload = null,
    next: ?*ProgramGuiCommandPayload = null,
    allocation_sequence: u64 = 0,
    logical_offset: u64 = 0,
    command_count: u32 = 0,
    capacity: u32 = 0,
};

// Common variable-sized frame resource node.  UTF-8, raster bytes and future
// paths share one append-ordered logical blob.  Header kind retains physical
// raster-vs-data telemetry without creating a second aggregate store.
const ProgramGuiResourcePayload = struct {
    header: ProgramPayloadHeader = .{},
    previous: ?*ProgramGuiResourcePayload = null,
    next: ?*ProgramGuiResourcePayload = null,
    allocation_sequence: u64 = 0,
    logical_offset: u64 = 0,
    raster_word_offset: u64 = 0,
    byte_count: u32 = 0,
    resource_kind: ProgramGuiCommandResourceKind = .none,
    reserved: u16 = 0,
};

const ProgramGuiFramePayload = struct {
    header: ProgramPayloadHeader = .{},
    command_payload: ?*ProgramGuiCommandPayload = null,
    command_tail: ?*ProgramGuiCommandPayload = null,
    resource_payload: ?*ProgramGuiResourcePayload = null,
    resource_tail: ?*ProgramGuiResourcePayload = null,
    retired_next: ?*ProgramGuiFramePayload = null,
    base_frame: ?*ProgramGuiFramePayload = null,
    command_count: u64 = 0,
    resource_len: u64 = 0,
    raster_words: u64 = 0,
    node_sequence: u64 = 0,
    generation: u64 = 0,
    damage_regions: [r4x_api.gui_frame_max_damage_regions]DisplayDamageRect = .{DisplayDamageRect{}} ** r4x_api.gui_frame_max_damage_regions,
    damage_count: u32 = 0,
    chain_depth: u32 = 1,
    reader_refs: u32 = 0,
    build_failed: bool = false,
    explicit_build: bool = false,
    retired: bool = false,
};

const ProgramGuiPayload = struct {
    header: ProgramPayloadHeader = .{},
    frame_lock: sync.Mutex = sync.Mutex.initClass("r4x-gui-frame", sync.LockRank.program_instances, .no_sleep),
    committed_frame: ?*ProgramGuiFramePayload = null,
    building_frame: ?*ProgramGuiFramePayload = null,
    retired_frames: ?*ProgramGuiFramePayload = null,
    window_id: i32 = -1,
    window_info: GuiWindowInfo = .{},
    events: [GUI_EVENT_QUEUE_SIZE]GuiEvent = .{GuiEvent{}} ** GUI_EVENT_QUEUE_SIZE,
    event_head: usize = 0,
    event_tail: usize = 0,
    event_high_water: u32 = 0,
    text: [GUI_TEXT_SIZE]u8 = .{0} ** GUI_TEXT_SIZE,
    font_id: u32 = GUI_FONT_BUILTIN_ID,
    title: [GUI_TITLE_SIZE]u8 = .{0} ** GUI_TITLE_SIZE,
    min_client_w: i32 = 0,
    min_client_h: i32 = 0,
    revision: u32 = 0,
    frame_commits: u64 = 0,
    frame_cancels: u64 = 0,
    frame_oom: u64 = 0,
    frame_snapshot_reads: u64 = 0,
    frame_generation_reads: u64 = 0,
    frame_delta_commits: u64 = 0,
    frame_full_commits: u64 = 0,
    frame_indexed8_commands: u64 = 0,
    frame_indexed8_resource_bytes: u64 = 0,
    frame_avoided_clone_bytes: u64 = 0,
    frame_last_error: i32 = 0,
    frame_peak_bytes: u64 = 0,
    frame_peak_commands: u64 = 0,
    start_attach_pending: bool = false,
    host_launch_pending: bool = false,
    host_launch_request: ProgramHostLaunchRequest = .{},
};

const ProgramInstanceStorage = struct {
    runtime: *ProgramRuntimePayload,
    process: *ProgramProcessPayload,
    console: ?*ProgramConsolePayload = null,
    gui: ?*ProgramGuiPayload = null,
};

const ProgramInstance = struct {
    used: bool = false,
    id: u32 = 0,
    task_id: u32 = 0,
    role: InstanceRole = .foreground,
    app_class: AppClass = .gui,
    entry: RawEntryFn = undefined,
    stack_top: u64 = 0,
    program_image_range_id: u32 = 0,
    program_image_base: u64 = 0,
    program_image_size: usize = 0,
    program_stack_range_id: u32 = 0,
    program_stack_base: u64 = 0,
    program_stack_reserve_size: u64 = 0,
    program_stack_committed_base: u64 = 0,
    program_stack_committed_size: u64 = 0,
    program_stack_guard_base: u64 = 0,
    program_stack_guard_size: u64 = 0,
    program_stack_initial_commit_size: u64 = 0,
    program_stack_create_cycles: u64 = 0,
    program_stack_telemetry_high_water: u64 = 0,
    program_stack_telemetry_committed_pages: u32 = 0,
    program_stack_serial_telemetry: bool = false,
    program_stack_telemetry_measured: bool = false,
    memory_profile: MemoryProfile = .normal,
    memory_limits: ProgramMemoryLimits = .{},
    memory_tag: [16]u8 = .{0} ** 16,
    runtime_payload: ?*ProgramRuntimePayload = null,
    process_payload: ?*ProgramProcessPayload = null,
    console_payload: ?*ProgramConsolePayload = null,
    gui_payload: ?*ProgramGuiPayload = null,
    display_used: bool = false,
    close_requested: bool = false,
    desktop_requested: bool = false,
    done: bool = false,
    storage_teardown_blocked: bool = false,
    exit_code: i32 = 0,
};

const ProgramRegistrySlotState = enum(u8) {
    free = 0,
    create = 1,
    publish = 2,
    run = 3,
    exit = 4,
    done = 5,
    retire = 6,
    reap = 7,
};

// Slot-local retirement progress lives beside, rather than inside, the
// <=256-byte ProgramInstance core.  Each value names the next destructive
// phase that still owns work.  The reaper advances it only after that phase
// has completed, so a deferred retry never repeats an earlier release.
const ProgramRetirePhase = enum(u8) {
    cancel_execution,
    detach_task,
    output_detach,
    storage_release,
    image_stack_vm_release,
    slot_reclaim,
};

const ProgramRegistrySlot = struct {
    state: ProgramRegistrySlotState = .free,
    generation: u64 = 0,
    public_id: u32 = 0,
    pin_count: u32 = 0,
    reclaim_pending: bool = false,
    retire_queued: bool = false,
    retire_in_progress: bool = false,
    retire_phase: ProgramRetirePhase = .cancel_execution,
    retire_output_detached: bool = false,
    retire_storage_released: bool = false,
    retire_image_stack_released: bool = false,
    retire_owner_released: bool = false,
    retire_attempts: u32 = 0,
    retire_next: ?*ProgramRegistrySlot = null,
    completion: ?*ProgramCompletionNode = null,
    instance: ProgramInstance = .{},
};

const ProgramRegistryChunk = struct {
    next: ?*ProgramRegistryChunk = null,
    serial: u64 = 0,
    empty_since_epoch: u64 = 0,
    slots: [PROGRAM_REGISTRY_CHUNK_SLOTS]ProgramRegistrySlot = .{ProgramRegistrySlot{}} ** PROGRAM_REGISTRY_CHUNK_SLOTS,
};

const ProgramInstanceReservation = struct {
    slot: *ProgramRegistrySlot,
    id: u32,
    generation: u64,
};

const ProgramInstanceReservationFailure = enum {
    no_memory,
    generation_exhausted,
};

const ProgramInstanceReservationResult = union(enum) {
    reservation: ProgramInstanceReservation,
    failure: ProgramInstanceReservationFailure,
};

const ProgramInstanceLease = struct {
    slot: *ProgramRegistrySlot,
    instance: *ProgramInstance,
    id: u32,
    generation: u64,
};

pub const ProgramRegistryStats = struct {
    chunk_slots: u32 = PROGRAM_REGISTRY_CHUNK_SLOTS,
    chunk_count: u32 = 0,
    slot_capacity: u32 = 0,
    free_slots: u32 = 0,
    reserved_slots: u32 = 0,
    live_slots: u32 = 0,
    done_slots: u32 = 0,
    retiring_slots: u32 = 0,
    pinned_slots: u32 = 0,
    peak_chunks: u64 = 0,
    peak_live: u64 = 0,
    growth_attempts: u64 = 0,
    growth_failures: u64 = 0,
    forced_failures: u64 = 0,
    publish_count: u64 = 0,
    rollback_count: u64 = 0,
    shrink_count: u64 = 0,
    id_collisions: u64 = 0,
    id_wraps: u64 = 0,
    live_id_hash: u64 = 0,
    live_address_hash: u64 = 0,
    stale_lease_rejections: u64 = 0,
    last_admission_error: i32 = 0,
    failure_armed: bool = false,
    next_generation: u64 = 1,
    completion_pending: u64 = 0,
    completion_ready: u64 = 0,
    completion_output_bytes: u64 = 0,
    retire_queued: u64 = 0,
    retire_deferred: u64 = 0,
    history_head: u64 = 0,
    history_count: u64 = 0,
    history_sequence: u64 = 0,
    completion_peak: u64 = 0,
    retire_retries: u64 = 0,
    launch_failures: u64 = 0,
    last_launch_error: i32 = 0,
};

const PROGRAM_INSTANCE_CORE_MAX_BYTES: usize = 256;
const PROGRAM_PAYLOAD_HEADER_BYTES: usize = 16;
// 0.61.13: +192 Bytes fuer den Modulpfad der Instanz (program_module_path).
// Der Pfad IST Instanzzustand - ein Programm liest damit seine eigenen
// eingebetteten Ressourcen, ohne den Installationsort zu raten. Bewusste
// Budgeterhoehung, keine stille: pro laufender Instanz 132 Nutzbytes mehr.
const PROGRAM_RUNTIME_PAYLOAD_MAX_BYTES: usize = 2112;
// 0.59.7 sized this at 448 bytes with a 128-byte CWD; the 0.60.19
// Windows-parity path limits grow the per-instance CWD buffer to
// drive.MAX_PATH (1024).  The payload stays a single heap allocation per
// instance; only the budget follows the contract growth.
const PROGRAM_PROCESS_PAYLOAD_MAX_BYTES: usize = 448 - 128 + drive.MAX_PATH;
const PROGRAM_ENVIRONMENT_PAYLOAD_MAX_BYTES: usize = 2112;
// 0.69.18: the bounded 4095-byte console queue accepts one complete maximum
// clipboard transfer without per-byte registry and reaper work.
const PROGRAM_CONSOLE_PAYLOAD_MAX_BYTES: usize = 8 * 1024;
const PROGRAM_CONSOLE_OUTPUT_PAYLOAD_MAX_BYTES: usize = 17 * 1024;
const PROGRAM_CONSOLE_TRANSCRIPT_PAYLOAD_MAX_BYTES: usize = 8 * 1024;
// 0.69.18: 63 GUI events absorb ordinary focus/key/button bursts while
// mouse moves are still coalesced inside the bounded per-instance payload.
const PROGRAM_GUI_PAYLOAD_MAX_BYTES: usize = 8 * 1024;
const PROGRAM_GUI_FRAME_PAYLOAD_MAX_BYTES: usize = 256;
const PROGRAM_GUI_COMMAND_NODE_BUDGET_BYTES: usize = 31 * 1024;
const PROGRAM_GUI_COMMAND_NODE_MAX_BYTES: usize = @sizeOf(ProgramGuiCommandPayload) + GUI_COMMAND_BLOCK_TARGET_CAPACITY * @sizeOf(ProgramGuiCommand);
const PROGRAM_GUI_RESOURCE_NODE_BUDGET_BYTES: usize = 1025 * 1024;
const PROGRAM_GUI_RESOURCE_NODE_MAX_BYTES: usize = @sizeOf(ProgramGuiResourcePayload) + GUI_LEGACY_TEXT_APPEND_MAX_BYTES;

comptime {
    if (@sizeOf(ProgramInstance) > PROGRAM_INSTANCE_CORE_MAX_BYTES) @compileError("ProgramInstance core exceeds 256-byte 0.59.7 budget");
    if (@sizeOf(ProgramPayloadHeader) != PROGRAM_PAYLOAD_HEADER_BYTES) @compileError("ProgramPayloadHeader must stay exactly 16 bytes");
    if (@sizeOf(ProgramRuntimePayload) > PROGRAM_RUNTIME_PAYLOAD_MAX_BYTES) @compileError("ProgramRuntimePayload exceeds 2112-byte 0.61.13 budget");
    if (@sizeOf(ProgramProcessPayload) > PROGRAM_PROCESS_PAYLOAD_MAX_BYTES) @compileError("ProgramProcessPayload exceeds the 0.59.7 budget (448 base + 0.60.19 CWD growth)");
    if (@sizeOf(ProgramEnvironmentPayload) > PROGRAM_ENVIRONMENT_PAYLOAD_MAX_BYTES) @compileError("ProgramEnvironmentPayload exceeds 2112-byte 0.59.7 budget");
    if (@sizeOf(ProgramConsolePayload) > PROGRAM_CONSOLE_PAYLOAD_MAX_BYTES) @compileError("ProgramConsolePayload exceeds the 8-KB 0.69.18 budget");
    if (@sizeOf(ProgramConsoleOutputPayload) > PROGRAM_CONSOLE_OUTPUT_PAYLOAD_MAX_BYTES) @compileError("ProgramConsoleOutputPayload exceeds 17-KB 0.59.7 budget");
    if (@sizeOf(ProgramConsoleTranscriptPayload) > PROGRAM_CONSOLE_TRANSCRIPT_PAYLOAD_MAX_BYTES) @compileError("ProgramConsoleTranscriptPayload exceeds the 8-KB 0.69.67 budget");
    if (@sizeOf(ProgramGuiPayload) > PROGRAM_GUI_PAYLOAD_MAX_BYTES) @compileError("ProgramGuiPayload exceeds the 8-KB 0.69.18 budget");
    if (@sizeOf(ProgramGuiFramePayload) > PROGRAM_GUI_FRAME_PAYLOAD_MAX_BYTES) @compileError("ProgramGuiFramePayload exceeds 256-byte budget");
    if (PROGRAM_GUI_COMMAND_NODE_MAX_BYTES > PROGRAM_GUI_COMMAND_NODE_BUDGET_BYTES) @compileError("ProgramGuiCommandPayload node exceeds 31-KB budget");
    if (@sizeOf(ProgramGuiCommandPayload) % @alignOf(ProgramGuiCommand) != 0) @compileError("ProgramGuiCommandPayload trailing commands are not aligned");
    if (@sizeOf(ProgramGuiResourcePayload) % @alignOf(u64) != 0) @compileError("ProgramGuiResourcePayload trailing bytes are not aligned");
    if (PROGRAM_GUI_RESOURCE_NODE_MAX_BYTES > PROGRAM_GUI_RESOURCE_NODE_BUDGET_BYTES) @compileError("ProgramGuiResourcePayload node exceeds 1025-KB legacy text budget");
    if (PROGRAM_GUI_COMMAND_NODE_MAX_BYTES > std.math.maxInt(u32) or PROGRAM_GUI_RESOURCE_NODE_MAX_BYTES > std.math.maxInt(u32)) @compileError("one GUI frame node no longer fits the payload owner header");
}

pub const ProgramInstanceStorageStats = struct {
    core_bytes_per_instance: u64 = @sizeOf(ProgramInstance),
    registry_reserved_core_bytes: u64 = 0,
    live_core_bytes: u64 = 0,
    active_instance_bytes: u64 = 0,
    peak_active_instance_bytes: u64 = 0,
    reserved_instance_bytes: u64 = 0,
    peak_reserved_instance_bytes: u64 = 0,
    current_payload_bytes: u64 = 0,
    peak_payload_bytes: u64 = 0,
    current_runtime_bytes: u64 = 0,
    peak_runtime_bytes: u64 = 0,
    current_console_bytes: u64 = 0,
    peak_console_bytes: u64 = 0,
    current_gui_bytes: u64 = 0,
    peak_gui_bytes: u64 = 0,
    active_instances: u32 = 0,
    active_service_instances: u32 = 0,
    active_console_instances: u32 = 0,
    active_gui_instances: u32 = 0,
    active_runtime_payloads: u32 = 0,
    active_process_payloads: u32 = 0,
    active_environment_payloads: u32 = 0,
    active_console_payloads: u32 = 0,
    active_console_output_payloads: u32 = 0,
    active_gui_payloads: u32 = 0,
    active_gui_frame_payloads: u32 = 0,
    active_gui_command_payloads: u32 = 0,
    active_gui_raster_payloads: u32 = 0,
    active_gui_data_payloads: u32 = 0,
    current_gui_frame_bytes: u64 = 0,
    peak_gui_frame_bytes: u64 = 0,
    current_gui_frame_commands: u64 = 0,
    peak_gui_frame_commands: u64 = 0,
    current_gui_frame_nodes: u64 = 0,
    peak_gui_frame_nodes: u64 = 0,
    gui_frame_commits: u64 = 0,
    gui_frame_cancels: u64 = 0,
    gui_frame_oom_failures: u64 = 0,
    gui_frame_snapshot_reads: u64 = 0,
    allocation_attempts: u64 = 0,
    payload_allocations: u64 = 0,
    payload_releases: u64 = 0,
    allocation_failures: u64 = 0,
    transaction_rollbacks: u64 = 0,
    owner_mismatches: u64 = 0,
    header_errors: u64 = 0,
    free_failures: u64 = 0,
    quarantined_payloads: u64 = 0,
    quarantined_bytes: u64 = 0,
};

pub const ProgramInstanceStorageSelfTestReport = struct {
    cases: u32 = 0,
    failed_case: u32 = 0,
    peak_payload_bytes: u64 = 0,
    heap_baseline_ok: bool = false,
    storage_baseline_ok: bool = false,
    zero_init_ok: bool = false,
};

var instance_storage_stats: ProgramInstanceStorageStats = .{};
var active_published_payload_bytes: u64 = 0;
var instance_storage_self_test_report: ProgramInstanceStorageSelfTestReport = .{};
var instance_storage_failure_after: ?u32 = null;
var instance_storage_failure_cursor: u32 = 0;
var instance_storage_self_test_active: bool = false;
var instance_storage_self_test_peak_payload_bytes: u64 = 0;
var instance_storage_self_test_running: bool = false;
var gui_frame_release_trace_enabled: bool = false;
var gui_frame_release_trace: [8]ProgramPayloadKind = .{.runtime} ** 8;
var gui_frame_release_trace_len: usize = 0;
// System-wide generation prevents a stale (instance id, generation) pair from
// aliasing a new frame after program-id reuse.  Zero means no committed frame.
var gui_frame_generation_seed: u64 = 0;

pub fn configureInstanceStorageFailureForTest(fail_after: ?u32) void {
    instance_storage_failure_after = fail_after;
    instance_storage_failure_cursor = 0;
}

fn shouldInjectInstanceStorageFailure() bool {
    const target = instance_storage_failure_after orelse return false;
    const cursor = instance_storage_failure_cursor;
    instance_storage_failure_cursor +%= 1;
    return cursor == target;
}

fn noteGuiFrameReleaseForSelfTest(kind: ProgramPayloadKind) void {
    if (!gui_frame_release_trace_enabled or !isGuiFramePayloadKind(kind)) return;
    if (gui_frame_release_trace_len < gui_frame_release_trace.len) {
        gui_frame_release_trace[gui_frame_release_trace_len] = kind;
        gui_frame_release_trace_len += 1;
    }
}

fn allocateInstancePayload(comptime Payload: type, owner_id: u32, kind: ProgramPayloadKind) ?*Payload {
    instance_storage_stats.allocation_attempts +%= 1;
    if (shouldInjectInstanceStorageFailure()) {
        instance_storage_stats.allocation_failures +%= 1;
        return null;
    }
    const memory = heap.alloc(@sizeOf(Payload), @alignOf(Payload)) orelse {
        instance_storage_stats.allocation_failures +%= 1;
        return null;
    };
    const payload: *Payload = @ptrCast(@alignCast(memory.ptr));
    payload.* = .{};
    payload.header = .{
        .magic = PROGRAM_PAYLOAD_MAGIC,
        .owner_id = owner_id,
        .requested_bytes = @intCast(@sizeOf(Payload)),
        .kind = kind,
    };
    instance_storage_stats.payload_allocations +%= 1;
    notePayloadAllocation(kind, @sizeOf(Payload));
    if (instanceById(owner_id) != null) noteActivePayloadAllocation(@sizeOf(Payload));
    return payload;
}

fn guiCommandPayloadBytes(capacity: u32) ?usize {
    if (capacity == 0) return null;
    const command_bytes = std.math.mul(usize, @as(usize, capacity), @sizeOf(ProgramGuiCommand)) catch return null;
    const requested_bytes = std.math.add(usize, @sizeOf(ProgramGuiCommandPayload), command_bytes) catch return null;
    if (requested_bytes > std.math.maxInt(u32)) return null;
    return requested_bytes;
}

fn guiCommandPayloadCommands(payload: *ProgramGuiCommandPayload) []ProgramGuiCommand {
    const commands: [*]ProgramGuiCommand = @ptrFromInt(@intFromPtr(payload) + @sizeOf(ProgramGuiCommandPayload));
    return commands[0..@as(usize, payload.capacity)];
}

fn guiCommandPayloadCommandsConst(payload: *const ProgramGuiCommandPayload) []const ProgramGuiCommand {
    const commands: [*]const ProgramGuiCommand = @ptrFromInt(@intFromPtr(payload) + @sizeOf(ProgramGuiCommandPayload));
    return commands[0..@as(usize, payload.capacity)];
}

fn allocateGuiCommandPayload(owner_id: u32, logical_offset: u64, capacity: u32) ?*ProgramGuiCommandPayload {
    const requested_bytes = guiCommandPayloadBytes(capacity) orelse return null;
    instance_storage_stats.allocation_attempts +%= 1;
    if (shouldInjectInstanceStorageFailure()) {
        instance_storage_stats.allocation_failures +%= 1;
        return null;
    }
    const memory = heap.alloc(requested_bytes, @alignOf(ProgramGuiCommandPayload)) orelse {
        instance_storage_stats.allocation_failures +%= 1;
        return null;
    };
    @memset(memory, 0);
    const payload: *ProgramGuiCommandPayload = @ptrCast(@alignCast(memory.ptr));
    payload.header = .{
        .magic = PROGRAM_PAYLOAD_MAGIC,
        .owner_id = owner_id,
        .requested_bytes = @intCast(requested_bytes),
        .kind = .gui_commands,
    };
    payload.logical_offset = logical_offset;
    payload.capacity = capacity;
    instance_storage_stats.payload_allocations +%= 1;
    notePayloadAllocation(.gui_commands, requested_bytes);
    if (instanceById(owner_id) != null) noteActivePayloadAllocation(requested_bytes);
    return payload;
}

fn guiResourcePayloadBytes(byte_count: usize) ?usize {
    if (byte_count == 0) return null;
    const requested_bytes = std.math.add(usize, @sizeOf(ProgramGuiResourcePayload), byte_count) catch return null;
    if (requested_bytes > std.math.maxInt(u32)) return null;
    return requested_bytes;
}

fn guiResourcePayloadData(payload: *ProgramGuiResourcePayload) []u8 {
    const bytes: [*]u8 = @ptrFromInt(@intFromPtr(payload) + @sizeOf(ProgramGuiResourcePayload));
    return bytes[0..@as(usize, payload.byte_count)];
}

fn guiResourcePayloadDataConst(payload: *const ProgramGuiResourcePayload) []const u8 {
    const bytes: [*]const u8 = @ptrFromInt(@intFromPtr(payload) + @sizeOf(ProgramGuiResourcePayload));
    return bytes[0..@as(usize, payload.byte_count)];
}

fn guiResourcePayloadWords(payload: *ProgramGuiResourcePayload) []u32 {
    const bytes = guiResourcePayloadData(payload);
    const words: [*]u32 = @ptrCast(@alignCast(bytes.ptr));
    return words[0 .. bytes.len / @sizeOf(u32)];
}

fn guiResourcePayloadWordsConst(payload: *const ProgramGuiResourcePayload) []const u32 {
    const bytes = guiResourcePayloadDataConst(payload);
    const words: [*]const u32 = @ptrCast(@alignCast(bytes.ptr));
    return words[0 .. bytes.len / @sizeOf(u32)];
}

fn guiResourcePayloadKind(resource_kind: ProgramGuiCommandResourceKind) ?ProgramPayloadKind {
    return switch (resource_kind) {
        .xrgb32, .alpha8 => .gui_raster,
        .utf8, .path, .indexed8 => .gui_frame_data,
        .none => null,
    };
}

fn allocateGuiResourcePayload(
    owner_id: u32,
    logical_offset: u64,
    raster_word_offset: u64,
    byte_count: usize,
    resource_kind: ProgramGuiCommandResourceKind,
) ?*ProgramGuiResourcePayload {
    const payload_kind = guiResourcePayloadKind(resource_kind) orelse return null;
    const requested_bytes = guiResourcePayloadBytes(byte_count) orelse return null;
    instance_storage_stats.allocation_attempts +%= 1;
    if (shouldInjectInstanceStorageFailure()) {
        instance_storage_stats.allocation_failures +%= 1;
        return null;
    }
    const memory = heap.alloc(requested_bytes, @alignOf(ProgramGuiResourcePayload)) orelse {
        instance_storage_stats.allocation_failures +%= 1;
        return null;
    };
    @memset(memory, 0);
    const payload: *ProgramGuiResourcePayload = @ptrCast(@alignCast(memory.ptr));
    payload.header = .{
        .magic = PROGRAM_PAYLOAD_MAGIC,
        .owner_id = owner_id,
        .requested_bytes = @intCast(requested_bytes),
        .kind = payload_kind,
    };
    payload.logical_offset = logical_offset;
    payload.raster_word_offset = raster_word_offset;
    payload.byte_count = @intCast(byte_count);
    payload.resource_kind = resource_kind;
    instance_storage_stats.payload_allocations +%= 1;
    notePayloadAllocation(payload_kind, requested_bytes);
    if (instanceById(owner_id) != null) noteActivePayloadAllocation(requested_bytes);
    return payload;
}

fn releaseInstancePayload(comptime Payload: type, payload: *Payload, owner_id: u32, kind: ProgramPayloadKind) bool {
    if (!validateInstancePayloadHeader(Payload, payload, owner_id, kind)) {
        quarantinePayload(kind, @sizeOf(Payload));
        return false;
    }
    // 0.59.7's published-owner accounting remains in force while 0.59.8 has
    // hidden the owner in `retiring`: conceptually
    // `if (instanceById(owner_id) != null) noteActivePayloadRelease(...)`.
    if (programRegistryOwnerIsPublished(owner_id)) noteActivePayloadRelease(@sizeOf(Payload));
    const bytes: [*]u8 = @ptrCast(payload);
    noteGuiFrameReleaseForSelfTest(kind);
    if (heap.free(bytes[0..@sizeOf(Payload)]) != .ok) {
        instance_storage_stats.free_failures +%= 1;
        quarantinePayload(kind, @sizeOf(Payload));
        return false;
    }
    instance_storage_stats.payload_releases +%= 1;
    notePayloadRelease(kind, @sizeOf(Payload));
    return true;
}

fn releaseGuiCommandPayload(payload: *ProgramGuiCommandPayload, owner_id: u32) bool {
    const requested_bytes = guiCommandPayloadBytes(payload.capacity) orelse {
        quarantinePayload(.gui_commands, @sizeOf(ProgramGuiCommandPayload));
        return false;
    };
    if (!validateInstancePayloadHeaderBytes(&payload.header, owner_id, .gui_commands, requested_bytes)) {
        quarantinePayload(.gui_commands, requested_bytes);
        return false;
    }
    if (programRegistryOwnerIsPublished(owner_id)) noteActivePayloadRelease(requested_bytes);
    noteGuiFrameCommandsRelease(payload.command_count);
    const bytes: [*]u8 = @ptrCast(payload);
    noteGuiFrameReleaseForSelfTest(.gui_commands);
    if (heap.free(bytes[0..requested_bytes]) != .ok) {
        instance_storage_stats.free_failures +%= 1;
        quarantinePayload(.gui_commands, requested_bytes);
        return false;
    }
    instance_storage_stats.payload_releases +%= 1;
    notePayloadRelease(.gui_commands, requested_bytes);
    return true;
}

fn releaseGuiResourcePayload(payload: *ProgramGuiResourcePayload, owner_id: u32) bool {
    const payload_kind = guiResourcePayloadKind(payload.resource_kind) orelse {
        quarantinePayload(.gui_frame_data, @sizeOf(ProgramGuiResourcePayload));
        return false;
    };
    const requested_bytes = guiResourcePayloadBytes(payload.byte_count) orelse {
        quarantinePayload(payload_kind, @sizeOf(ProgramGuiResourcePayload));
        return false;
    };
    if (!validateInstancePayloadHeaderBytes(&payload.header, owner_id, payload_kind, requested_bytes)) {
        quarantinePayload(payload_kind, requested_bytes);
        return false;
    }
    if (programRegistryOwnerIsPublished(owner_id)) noteActivePayloadRelease(requested_bytes);
    const bytes: [*]u8 = @ptrCast(payload);
    noteGuiFrameReleaseForSelfTest(payload_kind);
    if (heap.free(bytes[0..requested_bytes]) != .ok) {
        instance_storage_stats.free_failures +%= 1;
        quarantinePayload(payload_kind, requested_bytes);
        return false;
    }
    instance_storage_stats.payload_releases +%= 1;
    notePayloadRelease(payload_kind, requested_bytes);
    return true;
}

fn validateInstancePayloadHeader(comptime Payload: type, payload: *const Payload, owner_id: u32, kind: ProgramPayloadKind) bool {
    return validateInstancePayloadHeaderBytes(&payload.header, owner_id, kind, @sizeOf(Payload));
}

fn validateInstancePayloadHeaderBytes(header: *const ProgramPayloadHeader, owner_id: u32, kind: ProgramPayloadKind, requested_bytes: usize) bool {
    if (requested_bytes > std.math.maxInt(u32) or
        header.magic != PROGRAM_PAYLOAD_MAGIC or
        header.requested_bytes != requested_bytes or
        header.kind != kind or
        header.reserved != 0)
    {
        instance_storage_stats.header_errors +%= 1;
        return false;
    }
    if (header.owner_id != owner_id) {
        instance_storage_stats.owner_mismatches +%= 1;
        return false;
    }
    return true;
}

fn validateConsoleOutputPayload(payload: *const ProgramConsoleOutputPayload, owner_id: u32) bool {
    if (!validateInstancePayloadHeader(ProgramConsoleOutputPayload, payload, owner_id, .console_output)) return false;
    if (payload.ref_count == 0 or payload.active_ref_count > payload.ref_count or !std.mem.allEqual(u8, payload.reserved[0..], 0)) {
        instance_storage_stats.header_errors +%= 1;
        return false;
    }
    return true;
}

fn allocateConsoleOutputPayload(owner_id: u32) ?*ProgramConsoleOutputPayload {
    const payload = allocateInstancePayload(ProgramConsoleOutputPayload, owner_id, .console_output) orelse return null;
    payload.ref_count = 1;
    payload.active_ref_count = 1;
    payload.active_accounted = programRegistryOwnerIsPublished(owner_id);
    return payload;
}

fn retainConsoleOutputPayload(payload: *ProgramConsoleOutputPayload, active: bool) bool {
    if (payload.ref_count == std.math.maxInt(u32) or (active and payload.active_ref_count == std.math.maxInt(u32))) return false;
    payload.ref_count += 1;
    if (active) {
        if (payload.active_ref_count == 0 and !payload.active_accounted) {
            noteActivePayloadAllocation(@sizeOf(ProgramConsoleOutputPayload));
            payload.active_accounted = true;
        }
        payload.active_ref_count += 1;
    }
    return true;
}

fn makeConsoleOutputPayloadRefInactive(payload: *ProgramConsoleOutputPayload) bool {
    if (payload.active_ref_count == 0) return false;
    payload.active_ref_count -= 1;
    if (payload.active_ref_count == 0 and payload.active_accounted) {
        noteActivePayloadRelease(@sizeOf(ProgramConsoleOutputPayload));
        payload.active_accounted = false;
    }
    return true;
}

fn freeConsoleOutputPayloadMemory(payload: *ProgramConsoleOutputPayload) bool {
    const owner_id = payload.header.owner_id;
    if (payload.ref_count != 0 or payload.active_ref_count != 0 or payload.active_accounted or
        !validateInstancePayloadHeader(ProgramConsoleOutputPayload, payload, owner_id, .console_output))
    {
        quarantinePayload(.console_output, @sizeOf(ProgramConsoleOutputPayload));
        return false;
    }
    const bytes: [*]u8 = @ptrCast(payload);
    if (heap.free(bytes[0..@sizeOf(ProgramConsoleOutputPayload)]) != .ok) {
        instance_storage_stats.free_failures +%= 1;
        quarantinePayload(.console_output, @sizeOf(ProgramConsoleOutputPayload));
        return false;
    }
    instance_storage_stats.payload_releases +%= 1;
    notePayloadRelease(.console_output, @sizeOf(ProgramConsoleOutputPayload));
    return true;
}

fn releaseConsoleOutputPayload(payload: *ProgramConsoleOutputPayload, active: bool) bool {
    if (payload.ref_count == 0 or (active and !makeConsoleOutputPayloadRefInactive(payload))) return false;
    payload.ref_count -= 1;
    if (payload.ref_count != 0) return true;
    if (payload.active_ref_count != 0) {
        instance_storage_stats.header_errors +%= 1;
        return false;
    }
    return freeConsoleOutputPayloadMemory(payload);
}

fn releaseConsoleTranscriptSegments(transcript: *ProgramConsoleTranscriptPayload, active: bool) bool {
    var released = true;
    var index: usize = 0;
    while (index < transcript.segment_count) : (index += 1) {
        const payload = transcript.segments[index].payload orelse {
            released = false;
            continue;
        };
        transcript.segments[index] = .{};
        if (!releaseConsoleOutputPayload(payload, active)) released = false;
    }
    transcript.segment_count = 0;
    transcript.output_len = 0;
    return released;
}

fn makeConsoleTranscriptInactive(transcript: *ProgramConsoleTranscriptPayload) bool {
    var converted = true;
    var index: usize = 0;
    while (index < transcript.segment_count) : (index += 1) {
        const payload = transcript.segments[index].payload orelse {
            converted = false;
            continue;
        };
        if (!makeConsoleOutputPayloadRefInactive(payload)) converted = false;
    }
    return converted;
}

fn validateConsoleTranscript(transcript: *const ProgramConsoleTranscriptPayload, owner_id: u32) bool {
    if (!validateInstancePayloadHeader(ProgramConsoleTranscriptPayload, transcript, owner_id, .console_transcript) or
        transcript.segment_count > transcript.segments.len or transcript.output_len > CONSOLE_OUTPUT_SIZE)
    {
        instance_storage_stats.header_errors +%= 1;
        return false;
    }
    var total: u64 = 0;
    var index: usize = 0;
    while (index < transcript.segment_count) : (index += 1) {
        const segment = transcript.segments[index];
        const payload = segment.payload orelse {
            instance_storage_stats.header_errors +%= 1;
            return false;
        };
        if (segment.length == 0 or segment.reserved != 0 or
            !validateConsoleOutputPayload(payload, payload.header.owner_id))
        {
            instance_storage_stats.header_errors +%= 1;
            return false;
        }
        const end = std.math.add(u64, segment.start_sequence, segment.length) catch {
            instance_storage_stats.header_errors +%= 1;
            return false;
        };
        const retained_start = payload.next_sequence -| @min(payload.next_sequence, CONSOLE_OUTPUT_SIZE);
        if (segment.start_sequence < retained_start or end > payload.next_sequence) {
            instance_storage_stats.header_errors +%= 1;
            return false;
        }
        total += segment.length;
    }
    if (total != transcript.output_len) {
        instance_storage_stats.header_errors +%= 1;
        return false;
    }
    return true;
}

fn allocateProgramInstanceStorage(owner_id: u32, app_class: AppClass, inherit_environment: bool) ?ProgramInstanceStorage {
    const runtime = allocateInstancePayload(ProgramRuntimePayload, owner_id, .runtime) orelse {
        instance_storage_stats.transaction_rollbacks +%= 1;
        return null;
    };
    const process = allocateInstancePayload(ProgramProcessPayload, owner_id, .process) orelse {
        instance_storage_stats.transaction_rollbacks +%= 1;
        _ = releaseInstancePayload(ProgramRuntimePayload, runtime, owner_id, .runtime);
        return null;
    };
    var storage = ProgramInstanceStorage{ .runtime = runtime, .process = process };
    if (inherit_environment) {
        process.environment_payload = allocateInstancePayload(ProgramEnvironmentPayload, owner_id, .environment) orelse {
            instance_storage_stats.transaction_rollbacks +%= 1;
            rollbackProgramInstanceStorage(owner_id, &storage);
            return null;
        };
    }
    switch (app_class) {
        .console => {
            const console = allocateInstancePayload(ProgramConsolePayload, owner_id, .console) orelse {
                instance_storage_stats.transaction_rollbacks +%= 1;
                rollbackProgramInstanceStorage(owner_id, &storage);
                return null;
            };
            storage.console = console;
            console.transcript_payload = allocateInstancePayload(ProgramConsoleTranscriptPayload, owner_id, .console_transcript) orelse {
                instance_storage_stats.transaction_rollbacks +%= 1;
                rollbackProgramInstanceStorage(owner_id, &storage);
                return null;
            };
        },
        .gui => {
            storage.gui = allocateInstancePayload(ProgramGuiPayload, owner_id, .gui) orelse {
                instance_storage_stats.transaction_rollbacks +%= 1;
                rollbackProgramInstanceStorage(owner_id, &storage);
                return null;
            };
        },
        .service => {},
    }
    return storage;
}

fn validateGuiCommandPayload(payload: *const ProgramGuiCommandPayload, owner_id: u32) bool {
    const requested_bytes = guiCommandPayloadBytes(payload.capacity) orelse {
        instance_storage_stats.header_errors +%= 1;
        return false;
    };
    if (payload.allocation_sequence == 0 or payload.command_count == 0 or payload.command_count > payload.capacity or
        !validateInstancePayloadHeaderBytes(&payload.header, owner_id, .gui_commands, requested_bytes))
    {
        instance_storage_stats.header_errors +%= 1;
        return false;
    }
    const commands = guiCommandPayloadCommandsConst(payload);
    for (commands[0..payload.command_count]) |command| {
        if (command.version != 1 or command.size != 96 or command.reserved != 0) {
            instance_storage_stats.header_errors +%= 1;
            return false;
        }
    }
    return true;
}

fn validateGuiResourcePayload(payload: *const ProgramGuiResourcePayload, owner_id: u32) bool {
    const payload_kind = guiResourcePayloadKind(payload.resource_kind) orelse {
        instance_storage_stats.header_errors +%= 1;
        return false;
    };
    const requested_bytes = guiResourcePayloadBytes(payload.byte_count) orelse {
        instance_storage_stats.header_errors +%= 1;
        return false;
    };
    if (payload.allocation_sequence == 0 or
        (payload_kind == .gui_raster and (payload.byte_count % @sizeOf(u32) != 0)) or
        payload.reserved != 0 or
        !validateInstancePayloadHeaderBytes(&payload.header, owner_id, payload_kind, requested_bytes))
    {
        instance_storage_stats.header_errors +%= 1;
        return false;
    }
    return true;
}

fn validateGuiFrameOwnership(frame: *const ProgramGuiFramePayload, owner_id: u32) bool {
    if (!validateInstancePayloadHeader(ProgramGuiFramePayload, frame, owner_id, .gui_frame)) return false;
    if (frame.build_failed and frame.generation != 0) {
        instance_storage_stats.header_errors +%= 1;
        return false;
    }

    if (frame.command_payload == null) {
        if (frame.command_tail != null or frame.command_count != 0) {
            instance_storage_stats.header_errors +%= 1;
            return false;
        }
    } else if (frame.command_tail == null or frame.command_count == 0) {
        instance_storage_stats.header_errors +%= 1;
        return false;
    }

    var previous_command: ?*const ProgramGuiCommandPayload = null;
    var command_cursor: ?*const ProgramGuiCommandPayload = frame.command_payload;
    var logical_command: u64 = 0;
    while (command_cursor) |payload| {
        if (!validateGuiCommandPayload(payload, owner_id)) return false;
        if (payload.previous != previous_command or payload.logical_offset != logical_command) {
            instance_storage_stats.header_errors +%= 1;
            return false;
        }
        logical_command = std.math.add(u64, logical_command, payload.command_count) catch {
            instance_storage_stats.header_errors +%= 1;
            return false;
        };
        if (logical_command > frame.command_count or (payload.next == null and frame.command_tail != payload)) {
            instance_storage_stats.header_errors +%= 1;
            return false;
        }
        previous_command = payload;
        command_cursor = payload.next;
    }
    if (previous_command != frame.command_tail or logical_command != frame.command_count) {
        instance_storage_stats.header_errors +%= 1;
        return false;
    }

    if (frame.resource_payload == null) {
        if (frame.resource_tail != null or frame.resource_len != 0) {
            instance_storage_stats.header_errors +%= 1;
            return false;
        }
    } else if (frame.resource_tail == null or frame.resource_len == 0) {
        instance_storage_stats.header_errors +%= 1;
        return false;
    }

    var previous: ?*const ProgramGuiResourcePayload = null;
    var cursor: ?*const ProgramGuiResourcePayload = frame.resource_payload;
    var logical_offset: u64 = 0;
    while (cursor) |payload| {
        if (!validateGuiResourcePayload(payload, owner_id)) return false;
        if (payload.previous != previous or payload.logical_offset != logical_offset) {
            instance_storage_stats.header_errors +%= 1;
            return false;
        }
        logical_offset = std.math.add(u64, logical_offset, payload.byte_count) catch {
            instance_storage_stats.header_errors +%= 1;
            return false;
        };
        if (logical_offset > frame.resource_len) {
            instance_storage_stats.header_errors +%= 1;
            return false;
        }
        // Generic shared-blob nodes do not own a legacy raster-word range.
        // Physical native raster nodes are matched to their commands in the
        // complete semantic validator below; their offset may follow raster
        // commands stored in earlier generic blob nodes.
        if (payload.resource_kind != .xrgb32 and payload.resource_kind != .alpha8 and payload.raster_word_offset != 0) {
            instance_storage_stats.header_errors +%= 1;
            return false;
        }
        if (payload.next == null and frame.resource_tail != payload) {
            instance_storage_stats.header_errors +%= 1;
            return false;
        }
        previous = payload;
        cursor = payload.next;
    }
    if (previous != frame.resource_tail or logical_offset != frame.resource_len) {
        instance_storage_stats.header_errors +%= 1;
        return false;
    }

    // Command and resource nodes form separate typed chains, but their
    // allocation_sequence values are one frame-local total order.  Merge the
    // chains here to reject zero, duplicate, reordered or missing tokens; the
    // same order drives exact reverse teardown below.
    var ordered_command = frame.command_payload;
    var ordered_resource = frame.resource_payload;
    var last_sequence: u64 = 0;
    while (ordered_command != null or ordered_resource != null) {
        const take_command = if (ordered_command == null)
            false
        else if (ordered_resource == null)
            true
        else blk: {
            if (ordered_command.?.allocation_sequence == ordered_resource.?.allocation_sequence) {
                instance_storage_stats.header_errors +%= 1;
                return false;
            }
            break :blk ordered_command.?.allocation_sequence < ordered_resource.?.allocation_sequence;
        };
        const sequence = if (take_command) ordered_command.?.allocation_sequence else ordered_resource.?.allocation_sequence;
        if (last_sequence == std.math.maxInt(u64) or sequence != last_sequence + 1) {
            instance_storage_stats.header_errors +%= 1;
            return false;
        }
        last_sequence = sequence;
        if (take_command) {
            ordered_command = ordered_command.?.next;
        } else {
            ordered_resource = ordered_resource.?.next;
        }
    }
    if (last_sequence != frame.node_sequence) {
        instance_storage_stats.header_errors +%= 1;
        return false;
    }

    return true;
}

fn nextPhysicalGuiResource(start: ?*const ProgramGuiResourcePayload) ?*const ProgramGuiResourcePayload {
    var cursor = start;
    while (cursor) |payload| : (cursor = payload.next) {
        if (payload.resource_kind == .xrgb32 or payload.resource_kind == .alpha8) return payload;
    }
    return null;
}

fn validateGuiFrame(frame: *const ProgramGuiFramePayload, owner_id: u32) bool {
    if (!validateGuiFrameOwnership(frame, owner_id)) return false;
    if (frame.damage_count > r4x_api.gui_frame_max_damage_regions or frame.chain_depth == 0 or
        frame.chain_depth > r4x_api.gui_frame_max_delta_chain) return false;
    var damage_index: usize = 0;
    while (damage_index < frame.damage_count) : (damage_index += 1) {
        const region = frame.damage_regions[damage_index];
        if (region.w == 0 or region.h == 0) return false;
        const right = std.math.add(i64, region.x, region.w) catch return false;
        const bottom = std.math.add(i64, region.y, region.h) catch return false;
        if (right > std.math.maxInt(i32) or bottom > std.math.maxInt(i32)) return false;
    }

    var command_raster_words: u64 = 0;
    var physical_resource = nextPhysicalGuiResource(frame.resource_payload);
    var command_cursor = frame.command_payload;
    while (command_cursor) |payload| : (command_cursor = payload.next) {
        const commands = guiCommandPayloadCommandsConst(payload);
        for (commands[0..payload.command_count]) |command| {
            const expected_kind: ProgramGuiCommandResourceKind = switch (command.kind) {
                r4x_api.gui_frame_command_kind_clear, r4x_api.gui_frame_command_kind_rect => .none,
                r4x_api.gui_frame_command_kind_text => if (command.payload_bytes == 0) .none else .utf8,
                r4x_api.gui_frame_command_kind_raster => .xrgb32,
                r4x_api.gui_frame_command_kind_alpha8 => .alpha8,
                r4x_api.gui_frame_command_kind_path_fill,
                r4x_api.gui_frame_command_kind_path_stroke,
                r4x_api.gui_frame_command_kind_rounded_rect,
                r4x_api.gui_frame_command_kind_shadow,
                r4x_api.gui_frame_command_kind_argb32,
                => .path,
                r4x_api.gui_frame_command_kind_indexed8 => .indexed8,
                else => {
                    instance_storage_stats.header_errors +%= 1;
                    return false;
                },
            };
            if (command.resource_kind != expected_kind) {
                instance_storage_stats.header_errors +%= 1;
                return false;
            }
            if (command.resource_kind == .none) {
                if (command.payload_offset != 0 or command.payload_bytes != 0 or command.raster_word_offset != 0) {
                    instance_storage_stats.header_errors +%= 1;
                    return false;
                }
                continue;
            }
            const payload_end = std.math.add(u64, command.payload_offset, command.payload_bytes) catch {
                instance_storage_stats.header_errors +%= 1;
                return false;
            };
            if (command.payload_bytes == 0 or payload_end > frame.resource_len) {
                instance_storage_stats.header_errors +%= 1;
                return false;
            }
            if (command.resource_kind == .xrgb32 or command.resource_kind == .alpha8) {
                const words = if (command.resource_kind == .xrgb32)
                    command.payload_bytes / @sizeOf(u32)
                else
                    (std.math.add(u64, command.payload_bytes, 3) catch return false) / 4;
                if ((command.resource_kind == .xrgb32 and (command.payload_bytes % @sizeOf(u32)) != 0) or
                    command.raster_word_offset != command_raster_words)
                {
                    instance_storage_stats.header_errors +%= 1;
                    return false;
                }
                if (physical_resource) |resource| {
                    if (command.payload_offset > resource.logical_offset) {
                        instance_storage_stats.header_errors +%= 1;
                        return false;
                    }
                    if (command.payload_offset == resource.logical_offset) {
                        const physical_bytes = std.math.mul(u64, words, @sizeOf(u32)) catch {
                            instance_storage_stats.header_errors +%= 1;
                            return false;
                        };
                        if (resource.resource_kind != command.resource_kind or resource.byte_count != physical_bytes or
                            resource.raster_word_offset != command.raster_word_offset)
                        {
                            instance_storage_stats.header_errors +%= 1;
                            return false;
                        }
                        physical_resource = nextPhysicalGuiResource(resource.next);
                    }
                }
                command_raster_words = std.math.add(u64, command_raster_words, words) catch {
                    instance_storage_stats.header_errors +%= 1;
                    return false;
                };
            } else if (command.raster_word_offset != 0) {
                instance_storage_stats.header_errors +%= 1;
                return false;
            }
        }
    }
    if (physical_resource != null or command_raster_words != frame.raster_words) {
        instance_storage_stats.header_errors +%= 1;
        return false;
    }
    return true;
}

fn validateCommittedGuiFrameChain(root: *const ProgramGuiFramePayload, owner_id: u32, retired_root: bool) bool {
    var cursor: ?*const ProgramGuiFramePayload = root;
    var depth: u32 = 0;
    while (cursor) |frame| : (cursor = frame.base_frame) {
        depth += 1;
        if (depth > r4x_api.gui_frame_max_delta_chain or frame.generation == 0 or frame.build_failed or
            !validateGuiFrame(frame, owner_id) or frame.retired != (depth == 1 and retired_root)) return false;
        if (frame.base_frame) |base| {
            if (frame.damage_count == 0 or frame.chain_depth != base.chain_depth + 1 or base.generation >= frame.generation) return false;
        } else if (frame.damage_count != 0 or frame.chain_depth != 1) return false;
    }
    return depth == root.chain_depth;
}

fn guiFrameLocalBytes(frame: *const ProgramGuiFramePayload) u64 {
    var bytes: u64 = frame.header.requested_bytes;
    var command_cursor = frame.command_payload;
    while (command_cursor) |payload| {
        bytes +%= payload.header.requested_bytes;
        command_cursor = payload.next;
    }
    var cursor = frame.resource_payload;
    while (cursor) |payload| {
        bytes +%= payload.header.requested_bytes;
        cursor = payload.next;
    }
    return bytes;
}

fn guiFrameBytes(frame: *const ProgramGuiFramePayload) u64 {
    var bytes: u64 = 0;
    var cursor: ?*const ProgramGuiFramePayload = frame;
    var depth: u32 = 0;
    while (cursor) |item| : (cursor = item.base_frame) {
        if (depth >= r4x_api.gui_frame_max_delta_chain) return std.math.maxInt(u64);
        bytes +|= guiFrameLocalBytes(item);
        depth += 1;
    }
    return bytes;
}

fn guiFrameChainCommandCount(frame: *const ProgramGuiFramePayload) u64 {
    var count: u64 = 0;
    var cursor: ?*const ProgramGuiFramePayload = frame;
    while (cursor) |item| : (cursor = item.base_frame) count +|= item.command_count;
    return count;
}

fn guiFrameChainResourceBytes(frame: *const ProgramGuiFramePayload) u64 {
    var count: u64 = 0;
    var cursor: ?*const ProgramGuiFramePayload = frame;
    while (cursor) |item| : (cursor = item.base_frame) count +|= item.resource_len;
    return count;
}

fn guiFrameChainRasterWords(frame: *const ProgramGuiFramePayload) u64 {
    var count: u64 = 0;
    var cursor: ?*const ProgramGuiFramePayload = frame;
    while (cursor) |item| : (cursor = item.base_frame) count +|= item.raster_words;
    return count;
}

fn guiFrameChainHasReaders(frame: *const ProgramGuiFramePayload) bool {
    var cursor: ?*const ProgramGuiFramePayload = frame;
    while (cursor) |item| : (cursor = item.base_frame) if (item.reader_refs != 0) return true;
    return false;
}

fn guiOwnedFrameBytes(gui: *const ProgramGuiPayload) u64 {
    var bytes: u64 = 0;
    if (gui.committed_frame) |frame| bytes +%= guiFrameBytes(frame);
    if (gui.building_frame) |frame| bytes +%= guiFrameBytes(frame);
    var retired = gui.retired_frames;
    while (retired) |frame| : (retired = frame.retired_next) bytes +%= guiFrameBytes(frame);
    return bytes;
}

fn guiOwnedFrameCommands(gui: *const ProgramGuiPayload) u64 {
    var count: u64 = 0;
    if (gui.committed_frame) |frame| count +%= guiFrameChainCommandCount(frame);
    if (gui.building_frame) |frame| count +%= frame.command_count;
    var retired = gui.retired_frames;
    while (retired) |frame| : (retired = frame.retired_next) count +%= guiFrameChainCommandCount(frame);
    return count;
}

fn refreshGuiFrameOwnerPeak(gui: *ProgramGuiPayload) void {
    const bytes = guiOwnedFrameBytes(gui);
    const commands = guiOwnedFrameCommands(gui);
    if (bytes > gui.frame_peak_bytes) gui.frame_peak_bytes = bytes;
    if (commands > gui.frame_peak_commands) gui.frame_peak_commands = commands;
}

fn releaseGuiFrameLocal(frame: *ProgramGuiFramePayload, owner_id: u32) bool {
    // Teardown only needs an owner/link-safe hierarchy.  A private fallible
    // clone or batch delta may legitimately contain commands whose resources
    // were not prepared yet; semantic validation would leak that partial OOM
    // state instead of unwinding its already-owned nodes.
    if (frame.reader_refs != 0 or !validateGuiFrameOwnership(frame, owner_id)) return false;
    var released = true;
    var resource_cursor = frame.resource_tail;
    var command_cursor = frame.command_tail;
    while (resource_cursor != null or command_cursor != null) {
        const release_resource = if (resource_cursor == null)
            false
        else if (command_cursor == null)
            true
        else
            resource_cursor.?.allocation_sequence > command_cursor.?.allocation_sequence;
        if (release_resource) {
            const payload = resource_cursor.?;
            const previous = payload.previous;
            frame.resource_tail = previous;
            if (previous) |before| before.next = null else frame.resource_payload = null;
            payload.previous = null;
            payload.next = null;
            if (!releaseGuiResourcePayload(payload, owner_id)) released = false;
            resource_cursor = previous;
        } else {
            const payload = command_cursor.?;
            const previous = payload.previous;
            frame.command_tail = previous;
            if (previous) |before| before.next = null else frame.command_payload = null;
            payload.previous = null;
            payload.next = null;
            if (!releaseGuiCommandPayload(payload, owner_id)) released = false;
            command_cursor = previous;
        }
    }
    frame.resource_len = 0;
    frame.raster_words = 0;
    frame.command_count = 0;
    frame.node_sequence = 0;
    frame.retired_next = null;
    if (!releaseInstancePayload(ProgramGuiFramePayload, frame, owner_id, .gui_frame)) released = false;
    return released;
}

fn releaseGuiFrame(frame: *ProgramGuiFramePayload, owner_id: u32) bool {
    if (guiFrameChainHasReaders(frame)) return false;
    var released = true;
    var cursor: ?*ProgramGuiFramePayload = frame;
    while (cursor) |item| {
        const base = item.base_frame;
        item.base_frame = null;
        item.chain_depth = 1;
        if (!releaseGuiFrameLocal(item, owner_id)) released = false;
        cursor = base;
    }
    return released;
}

fn validateGuiFrameSet(gui: *const ProgramGuiPayload, owner_id: u32) bool {
    if (gui.committed_frame != null and gui.committed_frame == gui.building_frame) {
        instance_storage_stats.header_errors +%= 1;
        return false;
    }
    if (gui.committed_frame) |frame| {
        if (!validateCommittedGuiFrameChain(frame, owner_id, false)) return false;
    }
    if (gui.building_frame) |frame| {
        if (frame.generation != 0 or frame.retired or frame.reader_refs != 0 or frame.base_frame != null or
            frame.chain_depth != 1 or !validateGuiFrame(frame, owner_id)) return false;
    }

    var cursor = gui.retired_frames;
    var previous_generation: u64 = std.math.maxInt(u64);
    while (cursor) |frame| {
        if (!frame.retired or frame.generation == 0 or frame.generation >= previous_generation or
            frame == gui.committed_frame or frame == gui.building_frame or !validateCommittedGuiFrameChain(frame, owner_id, true))
        {
            instance_storage_stats.header_errors +%= 1;
            return false;
        }
        previous_generation = frame.generation;
        cursor = frame.retired_next;
    }
    return true;
}

fn validateProgramInstanceStorageTransaction(owner_id: u32, storage: *const ProgramInstanceStorage) bool {
    var valid = true;
    if (!validateInstancePayloadHeader(ProgramRuntimePayload, storage.runtime, owner_id, .runtime)) valid = false;

    const process_valid = validateInstancePayloadHeader(ProgramProcessPayload, storage.process, owner_id, .process);
    if (!process_valid) {
        valid = false;
    } else if (storage.process.environment_payload) |environment| {
        if (!validateInstancePayloadHeader(ProgramEnvironmentPayload, environment, owner_id, .environment)) valid = false;
    }

    if (storage.console) |console| {
        const console_valid = validateInstancePayloadHeader(ProgramConsolePayload, console, owner_id, .console);
        if (!console_valid) {
            valid = false;
        } else if (console.transcript_payload) |transcript| {
            if (!validateConsoleTranscript(transcript, owner_id)) valid = false;
            if (console.writer_payload) |output| {
                if (!validateConsoleOutputPayload(output, owner_id)) valid = false;
            }
        } else {
            instance_storage_stats.header_errors +%= 1;
            valid = false;
        }
    }

    if (storage.gui) |gui| {
        const gui_valid = validateInstancePayloadHeader(ProgramGuiPayload, gui, owner_id, .gui);
        if (!gui_valid) {
            valid = false;
        } else if (!validateGuiFrameSet(gui, owner_id)) valid = false;
    }
    return valid;
}

fn validateProgramInstanceStorage(instance: *const ProgramInstance) bool {
    var valid = true;
    if (instance.runtime_payload) |runtime| {
        if (!validateInstancePayloadHeader(ProgramRuntimePayload, runtime, instance.id, .runtime)) valid = false;
    } else {
        instance_storage_stats.header_errors +%= 1;
        valid = false;
    }

    if (instance.process_payload) |process| {
        const process_valid = validateInstancePayloadHeader(ProgramProcessPayload, process, instance.id, .process);
        if (!process_valid) {
            valid = false;
        } else if (process.environment_payload) |environment| {
            if (!validateInstancePayloadHeader(ProgramEnvironmentPayload, environment, instance.id, .environment)) valid = false;
        }
    } else {
        instance_storage_stats.header_errors +%= 1;
        valid = false;
    }

    if (instance.console_payload) |console| {
        const console_valid = validateInstancePayloadHeader(ProgramConsolePayload, console, instance.id, .console);
        if (!console_valid) {
            valid = false;
        } else if (console.transcript_payload) |transcript| {
            if (!validateConsoleTranscript(transcript, instance.id)) valid = false;
            if (console.writer_payload) |output| {
                if (!validateConsoleOutputPayload(output, instance.id)) valid = false;
            }
        } else {
            instance_storage_stats.header_errors +%= 1;
            valid = false;
        }
    } else if (instance.app_class == .console) {
        instance_storage_stats.header_errors +%= 1;
        valid = false;
    }

    if (instance.gui_payload) |gui| {
        const gui_valid = validateInstancePayloadHeader(ProgramGuiPayload, gui, instance.id, .gui);
        if (!gui_valid) {
            valid = false;
        } else if (!validateGuiFrameSet(gui, instance.id)) valid = false;
    } else if (instance.app_class == .gui) {
        instance_storage_stats.header_errors +%= 1;
        valid = false;
    }
    return valid;
}

fn releaseGuiFrameSet(gui: *ProgramGuiPayload, owner_id: u32) bool {
    if (!validateGuiFrameSet(gui, owner_id)) return false;
    if ((gui.committed_frame != null and guiFrameChainHasReaders(gui.committed_frame.?)) or
        (gui.building_frame != null and guiFrameChainHasReaders(gui.building_frame.?))) return false;
    var retired_check = gui.retired_frames;
    while (retired_check) |frame| : (retired_check = frame.retired_next) {
        if (guiFrameChainHasReaders(frame)) return false;
    }

    const building = gui.building_frame;
    const committed = gui.committed_frame;
    var retired = gui.retired_frames;
    gui.building_frame = null;
    gui.committed_frame = null;
    gui.retired_frames = null;
    var released = true;
    if (building) |frame| {
        if (!releaseGuiFrame(frame, owner_id)) released = false;
    }
    if (committed) |frame| {
        if (!releaseGuiFrame(frame, owner_id)) released = false;
    }
    while (retired) |frame| {
        const next = frame.retired_next;
        frame.retired_next = null;
        if (!releaseGuiFrame(frame, owner_id)) released = false;
        retired = next;
    }
    return released;
}

fn rollbackProgramInstanceStorage(owner_id: u32, storage: *ProgramInstanceStorage) void {
    if (!validateProgramInstanceStorageTransaction(owner_id, storage)) return;
    if (storage.gui) |payload| {
        _ = releaseGuiFrameSet(payload, owner_id);
        storage.gui = null;
        _ = releaseInstancePayload(ProgramGuiPayload, payload, owner_id, .gui);
    }
    if (storage.console) |payload| {
        if (payload.transcript_payload) |transcript| {
            payload.transcript_payload = null;
            _ = releaseConsoleTranscriptSegments(transcript, true);
            _ = releaseInstancePayload(ProgramConsoleTranscriptPayload, transcript, owner_id, .console_transcript);
        }
        if (payload.writer_payload) |output| {
            payload.writer_payload = null;
            _ = releaseConsoleOutputPayload(output, true);
        }
        storage.console = null;
        _ = releaseInstancePayload(ProgramConsolePayload, payload, owner_id, .console);
    }
    if (storage.process.environment_payload) |environment| {
        storage.process.environment_payload = null;
        _ = releaseInstancePayload(ProgramEnvironmentPayload, environment, owner_id, .environment);
    }
    _ = releaseInstancePayload(ProgramProcessPayload, storage.process, owner_id, .process);
    _ = releaseInstancePayload(ProgramRuntimePayload, storage.runtime, owner_id, .runtime);
}

fn detachProgramCompletionOutput(instance: *ProgramInstance, completion: *ProgramCompletionNode) bool {
    if (instance.storage_teardown_blocked) return false;
    if (instance.console_payload) |console| {
        if (console.transcript_payload == null) {
            // A valid retry is suppressed by the slot-local once flag.  Seeing
            // an already-null source on the first attempt is therefore an
            // invalid hierarchy, not a transient state to spin on forever.
            instance.storage_teardown_blocked = true;
            return false;
        }
    }
    if (!validateProgramInstanceStorage(instance)) {
        // Preserve the complete hierarchy and its core pointers.  A failed
        // preflight must never partially free or mutate potentially foreign
        // ownership; the done instance remains contained for diagnostics.
        instance.storage_teardown_blocked = true;
        return false;
    }
    if (!console_output_lock.lock(sync.WAIT_FOREVER)) return false;
    defer _ = console_output_lock.unlock();
    const console = instance.console_payload orelse return true;
    const transcript = console.transcript_payload orelse unreachable;
    // Detach ownership before either transfer or free.  Once the slot's
    // output_detach once flag is set, no retry can observe this pointer in
    // ProgramInstance storage again.
    if (console.writer_payload) |writer| {
        console.writer_payload = null;
        if (!releaseConsoleOutputPayload(writer, true)) {
            instance.storage_teardown_blocked = true;
            return false;
        }
    }
    console.transcript_payload = null;
    if (completion.retain_output) {
        if (!makeConsoleTranscriptInactive(transcript)) {
            instance.storage_teardown_blocked = true;
            return false;
        }
        completion.output_payload = transcript;
        completion.output_length = transcript.output_len;
        completion.output_revision = console.revision;
        if (completion.output_length != 0) completion.flags |= PROGRAM_COMPLETION_FLAG_OUTPUT;
        noteActivePayloadRelease(@sizeOf(ProgramConsoleTranscriptPayload));
        completion.output_storage_bytes = @sizeOf(ProgramConsoleTranscriptPayload);
        const locked = lockProgramRegistry();
        if (locked) {
            program_registry_stats.completion_output_bytes +%= completion.output_storage_bytes;
            unlockProgramRegistry();
        }
    } else {
        if (!releaseConsoleTranscriptSegments(transcript, true) or
            !releaseInstancePayload(ProgramConsoleTranscriptPayload, transcript, instance.id, .console_transcript))
        {
            instance.storage_teardown_blocked = true;
            return false;
        }
    }
    return true;
}

fn releaseProgramInstanceStorage(instance: *ProgramInstance, completion: *ProgramCompletionNode) bool {
    _ = completion;
    if (instance.storage_teardown_blocked) return false;
    // detachProgramCompletionOutput performed the all-or-nothing hierarchy
    // preflight before retirement entered this phase.  No new lease can be
    // acquired in .retire, so the remaining hierarchy cannot change here.
    if (instance.gui_payload) |payload| {
        if (!releaseGuiFrameSet(payload, instance.id)) {
            instance.storage_teardown_blocked = true;
            return false;
        }
        instance.gui_payload = null;
        _ = releaseInstancePayload(ProgramGuiPayload, payload, instance.id, .gui);
    }
    if (instance.console_payload) |payload| {
        // The output pointer must have crossed the preceding persistent
        // output_detach boundary before the console payload can be freed.
        if (payload.transcript_payload != null or payload.writer_payload != null) {
            instance.storage_teardown_blocked = true;
            return false;
        }
        instance.console_payload = null;
        _ = releaseInstancePayload(ProgramConsolePayload, payload, instance.id, .console);
    }
    if (instance.process_payload) |payload| {
        if (payload.environment_payload) |environment| {
            payload.environment_payload = null;
            _ = releaseInstancePayload(ProgramEnvironmentPayload, environment, instance.id, .environment);
        }
        instance.process_payload = null;
        _ = releaseInstancePayload(ProgramProcessPayload, payload, instance.id, .process);
    }
    if (instance.runtime_payload) |payload| {
        instance.runtime_payload = null;
        _ = releaseInstancePayload(ProgramRuntimePayload, payload, instance.id, .runtime);
    }
    noteProgramInstanceRetired(instance.app_class);
    return true;
}

fn freeCompletionOutput(node: *ProgramCompletionNode) void {
    const transcript = node.output_payload orelse return;
    node.output_payload = null;
    if (!console_output_lock.lock(sync.WAIT_FOREVER)) {
        node.output_payload = transcript;
        return;
    }
    defer _ = console_output_lock.unlock();
    _ = releaseConsoleTranscriptSegments(transcript, false);
    _ = releaseInstancePayload(ProgramConsoleTranscriptPayload, transcript, node.handle.instance_id, .console_transcript);
    const locked = lockProgramRegistry();
    if (locked) {
        if (program_registry_stats.completion_output_bytes >= node.output_storage_bytes) {
            program_registry_stats.completion_output_bytes -= node.output_storage_bytes;
        } else {
            program_registry_stats.completion_output_bytes = 0;
        }
        unlockProgramRegistry();
    }
    node.output_storage_bytes = 0;
}

pub fn instanceStorageStats() ProgramInstanceStorageStats {
    refreshInstanceByteTelemetry();
    return instance_storage_stats;
}

pub fn instanceStorageSelfTestReport() ProgramInstanceStorageSelfTestReport {
    return instance_storage_self_test_report;
}

pub fn runInstanceStorageSelfTest() bool {
    const saved_failure_after = instance_storage_failure_after;
    const saved_failure_cursor = instance_storage_failure_cursor;
    const saved_self_test_active = instance_storage_self_test_active;
    const saved_self_test_peak = instance_storage_self_test_peak_payload_bytes;
    const saved_release_trace_enabled = gui_frame_release_trace_enabled;
    const saved_release_trace_len = gui_frame_release_trace_len;
    defer {
        instance_storage_failure_after = saved_failure_after;
        instance_storage_failure_cursor = saved_failure_cursor;
        instance_storage_self_test_active = saved_self_test_active;
        instance_storage_self_test_peak_payload_bytes = saved_self_test_peak;
        gui_frame_release_trace_enabled = saved_release_trace_enabled;
        gui_frame_release_trace_len = saved_release_trace_len;
    }

    instance_storage_self_test_report = .{};
    configureInstanceStorageFailureForTest(null);
    if (!warmInstanceStorageSelfTest()) {
        instance_storage_self_test_report.failed_case = 1;
        return false;
    }

    const heap_baseline = heap.stats();
    const storage_baseline = instanceStorageStats();
    instance_storage_self_test_active = true;
    instance_storage_self_test_peak_payload_bytes = storage_baseline.current_payload_bytes;
    gui_frame_release_trace_enabled = false;
    gui_frame_release_trace_len = 0;
    var case_id: u32 = 0;
    const plans = [_]struct { app_class: AppClass, inherit_environment: bool, stages: u32 }{
        .{ .app_class = .service, .inherit_environment = false, .stages = 2 },
        .{ .app_class = .service, .inherit_environment = true, .stages = 3 },
        .{ .app_class = .console, .inherit_environment = false, .stages = 4 },
        .{ .app_class = .console, .inherit_environment = true, .stages = 5 },
        .{ .app_class = .gui, .inherit_environment = false, .stages = 3 },
        .{ .app_class = .gui, .inherit_environment = true, .stages = 4 },
    };
    for (plans) |plan| {
        var fail_after: u32 = 0;
        while (fail_after < plan.stages) : (fail_after += 1) {
            case_id += 1;
            configureInstanceStorageFailureForTest(fail_after);
            if (allocateProgramInstanceStorage(0xFFFF_0000 + case_id, plan.app_class, plan.inherit_environment)) |storage_value| {
                var storage = storage_value;
                configureInstanceStorageFailureForTest(null);
                rollbackProgramInstanceStorage(0xFFFF_0000 + case_id, &storage);
                return failInstanceStorageSelfTest(case_id, heap_baseline, storage_baseline);
            }
            configureInstanceStorageFailureForTest(null);
            if (!instanceStorageHeapBaselineEqual(heap_baseline, heap.stats()) or !instanceStorageCurrentEqual(storage_baseline, instanceStorageStats())) {
                return failInstanceStorageSelfTest(case_id, heap_baseline, storage_baseline);
            }
        }
    }

    case_id += 1;
    if (!testLazyEnvironmentFailure(0xFFFE_0001, heap_baseline, storage_baseline)) return failInstanceStorageSelfTest(case_id, heap_baseline, storage_baseline);
    case_id += 1;
    if (!testLazyGuiStateFailure(0xFFFE_0002, heap_baseline, storage_baseline)) return failInstanceStorageSelfTest(case_id, heap_baseline, storage_baseline);
    case_id += 1;
    if (!testLazyGuiCommandFailure(0xFFFE_0003, heap_baseline, storage_baseline)) return failInstanceStorageSelfTest(case_id, heap_baseline, storage_baseline);
    case_id += 1;
    if (!testLazyGuiBlitFailure(0xFFFE_0004, 0, heap_baseline, storage_baseline)) return failInstanceStorageSelfTest(case_id, heap_baseline, storage_baseline);
    case_id += 1;
    if (!testLazyGuiBlitFailure(0xFFFE_0005, 1, heap_baseline, storage_baseline)) return failInstanceStorageSelfTest(case_id, heap_baseline, storage_baseline);
    case_id += 1;
    if (!testInstanceStorageZeroInitialization(0xFFFE_0006, heap_baseline, storage_baseline)) return failInstanceStorageSelfTest(case_id, heap_baseline, storage_baseline);
    case_id += 1;
    if (!testGuiRasterGrowth(0xFFFE_0007, heap_baseline, storage_baseline)) return failInstanceStorageSelfTest(case_id, heap_baseline, storage_baseline);
    case_id += 1;
    if (!testGuiRasterGrowthFailure(0xFFFE_0008, heap_baseline, storage_baseline)) return failInstanceStorageSelfTest(case_id, heap_baseline, storage_baseline);
    case_id += 1;
    if (!testGuiDynamicCommandGrowth(0xFFFE_0009, heap_baseline, storage_baseline)) return failInstanceStorageSelfTest(case_id, heap_baseline, storage_baseline);
    case_id += 1;
    if (!testGuiMixedFrameResources(0xFFFE_000A, heap_baseline, storage_baseline)) return failInstanceStorageSelfTest(case_id, heap_baseline, storage_baseline);
    case_id += 1;
    if (!testGuiAtomicCommitGrowthFailure(0xFFFE_000B, heap_baseline, storage_baseline)) return failInstanceStorageSelfTest(case_id, heap_baseline, storage_baseline);
    case_id += 1;
    if (!testGuiRetiredReaderLifetime(0xFFFE_000C, heap_baseline, storage_baseline)) return failInstanceStorageSelfTest(case_id, heap_baseline, storage_baseline);
    case_id += 1;
    if (!testGuiDeltaIndexed8(0xFFFE_000D, heap_baseline, storage_baseline)) return failInstanceStorageSelfTest(case_id, heap_baseline, storage_baseline);

    instance_storage_self_test_report = .{
        .cases = case_id,
        .failed_case = 0,
        .peak_payload_bytes = instance_storage_self_test_peak_payload_bytes,
        .heap_baseline_ok = true,
        .storage_baseline_ok = true,
        .zero_init_ok = true,
    };
    k.puts("[PINSTSTOR] result=OK cases=");
    k.putDec(case_id);
    k.puts(" heap_baseline=ok storage_baseline=ok zero_init=ok\r\n");
    return true;
}

fn warmInstanceStorageSelfTest() bool {
    var console = allocateProgramInstanceStorage(0xFFFF_FF01, .console, true) orelse return false;
    rollbackProgramInstanceStorage(0xFFFF_FF01, &console);

    var gui = allocateProgramInstanceStorage(0xFFFF_FF02, .gui, false) orelse return false;
    var fake = ProgramInstance{
        .id = 0xFFFF_FF02,
        .app_class = .gui,
        .entry = undefined,
        .stack_top = 0,
        .runtime_payload = gui.runtime,
        .process_payload = gui.process,
        .gui_payload = gui.gui,
    };
    const frame = allocateGuiFramePayload(fake.id, false) orelse {
        rollbackProgramInstanceStorage(0xFFFF_FF02, &gui);
        return false;
    };
    fake.gui_payload.?.building_frame = frame;
    var render = prepareGuiBlitStorage(&fake, frame, GUI_RASTER_MAX_PIXELS, .xrgb32) orelse {
        rollbackProgramInstanceStorage(0xFFFF_FF02, &gui);
        return false;
    };
    cancelGuiBlitStorage(&fake, &render);
    if (appendGuiTextCommand(&fake, 0, 0, "warm", 0x00FF_FFFF, 0, 0, 0) < 0) {
        rollbackProgramInstanceStorage(0xFFFF_FF02, &gui);
        return false;
    }
    if (!appendGuiRasterTestNodes(&fake, 6)) {
        rollbackProgramInstanceStorage(0xFFFF_FF02, &gui);
        return false;
    }
    rollbackProgramInstanceStorage(0xFFFF_FF02, &gui);
    return true;
}

fn testLazyEnvironmentFailure(owner_id: u32, heap_baseline: heap.Stats, storage_baseline: ProgramInstanceStorageStats) bool {
    var storage = allocateProgramInstanceStorage(owner_id, .service, false) orelse return false;
    var fake = ProgramInstance{
        .id = owner_id,
        .app_class = .service,
        .entry = undefined,
        .stack_top = 0,
        .runtime_payload = storage.runtime,
        .process_payload = storage.process,
    };
    configureInstanceStorageFailureForTest(0);
    const failed = ensureEnvironmentPayload(&fake) == null;
    configureInstanceStorageFailureForTest(null);
    rollbackProgramInstanceStorage(owner_id, &storage);
    return failed and instanceStorageHeapBaselineEqual(heap_baseline, heap.stats()) and instanceStorageCurrentEqual(storage_baseline, instanceStorageStats());
}

fn testLazyGuiStateFailure(owner_id: u32, heap_baseline: heap.Stats, storage_baseline: ProgramInstanceStorageStats) bool {
    var storage = allocateProgramInstanceStorage(owner_id, .service, false) orelse return false;
    var fake = ProgramInstance{
        .id = owner_id,
        .app_class = .service,
        .entry = undefined,
        .stack_top = 0,
        .runtime_payload = storage.runtime,
        .process_payload = storage.process,
    };
    configureInstanceStorageFailureForTest(0);
    const result = ensureGuiPayload(&fake);
    if (result) |payload| storage.gui = payload;
    const failed = result == null;
    configureInstanceStorageFailureForTest(null);
    rollbackProgramInstanceStorage(owner_id, &storage);
    return failed and instanceStorageHeapBaselineEqual(heap_baseline, heap.stats()) and instanceStorageCurrentEqual(storage_baseline, instanceStorageStats());
}

fn testLazyGuiCommandFailure(owner_id: u32, heap_baseline: heap.Stats, storage_baseline: ProgramInstanceStorageStats) bool {
    var storage = allocateProgramInstanceStorage(owner_id, .gui, false) orelse return false;
    var fake = ProgramInstance{
        .id = owner_id,
        .app_class = .gui,
        .entry = undefined,
        .stack_top = 0,
        .runtime_payload = storage.runtime,
        .process_payload = storage.process,
        .gui_payload = storage.gui,
    };
    configureInstanceStorageFailureForTest(0);
    const failed = guiFrameEnsureBuild(&fake) == null;
    configureInstanceStorageFailureForTest(null);
    rollbackProgramInstanceStorage(owner_id, &storage);
    return failed and instanceStorageHeapBaselineEqual(heap_baseline, heap.stats()) and instanceStorageCurrentEqual(storage_baseline, instanceStorageStats());
}

fn testLazyGuiBlitFailure(owner_id: u32, fail_after: u32, heap_baseline: heap.Stats, storage_baseline: ProgramInstanceStorageStats) bool {
    var storage = allocateProgramInstanceStorage(owner_id, .gui, false) orelse return false;
    var fake = ProgramInstance{
        .id = owner_id,
        .app_class = .gui,
        .entry = undefined,
        .stack_top = 0,
        .runtime_payload = storage.runtime,
        .process_payload = storage.process,
        .gui_payload = storage.gui,
    };
    const frame = allocateGuiFramePayload(owner_id, false) orelse {
        rollbackProgramInstanceStorage(owner_id, &storage);
        return false;
    };
    fake.gui_payload.?.building_frame = frame;
    configureInstanceStorageFailureForTest(fail_after);
    const prepared = prepareGuiBlitStorage(&fake, frame, 1, .xrgb32);
    const failed = prepared == null;
    if (prepared) |value| {
        var render = value;
        cancelGuiBlitStorage(&fake, &render);
    }
    configureInstanceStorageFailureForTest(null);
    rollbackProgramInstanceStorage(owner_id, &storage);
    return failed and instanceStorageHeapBaselineEqual(heap_baseline, heap.stats()) and instanceStorageCurrentEqual(storage_baseline, instanceStorageStats());
}

fn testInstanceStorageZeroInitialization(owner_id: u32, heap_baseline: heap.Stats, storage_baseline: ProgramInstanceStorageStats) bool {
    var first = allocateProgramInstanceStorage(owner_id, .gui, false) orelse return false;
    var first_fake = ProgramInstance{
        .id = owner_id,
        .app_class = .gui,
        .entry = undefined,
        .stack_top = 0,
        .runtime_payload = first.runtime,
        .process_payload = first.process,
        .gui_payload = first.gui,
    };
    const first_environment = ensureEnvironmentPayload(&first_fake) orelse {
        rollbackProgramInstanceStorage(owner_id, &first);
        return false;
    };
    const first_frame = allocateGuiFramePayload(owner_id, false) orelse {
        rollbackProgramInstanceStorage(owner_id, &first);
        return false;
    };
    first_fake.gui_payload.?.building_frame = first_frame;
    var first_render = prepareGuiBlitStorage(&first_fake, first_frame, 1, .xrgb32) orelse {
        rollbackProgramInstanceStorage(owner_id, &first);
        return false;
    };
    first_environment.bytes[0] = 0xA5;
    guiCommandPayloadCommands(first_render.command.payload)[0].kind = 0xA5A5_A5A5;
    guiResourcePayloadWords(first_render.resource.payload)[0] = 0xA5A5_A5A5;
    cancelGuiBlitStorage(&first_fake, &first_render);
    rollbackProgramInstanceStorage(owner_id, &first);

    var second = allocateProgramInstanceStorage(owner_id + 1, .gui, false) orelse return false;
    var second_fake = ProgramInstance{
        .id = owner_id + 1,
        .app_class = .gui,
        .entry = undefined,
        .stack_top = 0,
        .runtime_payload = second.runtime,
        .process_payload = second.process,
        .gui_payload = second.gui,
    };
    const second_environment = ensureEnvironmentPayload(&second_fake) orelse {
        rollbackProgramInstanceStorage(owner_id + 1, &second);
        return false;
    };
    const second_frame = allocateGuiFramePayload(owner_id + 1, false) orelse {
        rollbackProgramInstanceStorage(owner_id + 1, &second);
        return false;
    };
    second_fake.gui_payload.?.building_frame = second_frame;
    var second_render = prepareGuiBlitStorage(&second_fake, second_frame, 1, .xrgb32) orelse {
        rollbackProgramInstanceStorage(owner_id + 1, &second);
        return false;
    };
    const zeroed = second_environment.bytes[0] == 0 and
        guiCommandPayloadCommands(second_render.command.payload)[0].kind == 0 and
        guiResourcePayloadWords(second_render.resource.payload)[0] == 0;
    cancelGuiBlitStorage(&second_fake, &second_render);
    rollbackProgramInstanceStorage(owner_id + 1, &second);
    return zeroed and instanceStorageHeapBaselineEqual(heap_baseline, heap.stats()) and instanceStorageCurrentEqual(storage_baseline, instanceStorageStats());
}

fn appendGuiRasterTestNodes(instance: *ProgramInstance, count: usize) bool {
    const gui = instance.gui_payload orelse return false;
    const frame = gui.building_frame orelse blk: {
        const allocated = allocateGuiFramePayload(instance.id, false) orelse return false;
        gui.building_frame = allocated;
        break :blk allocated;
    };
    var index: usize = 0;
    while (index < count) : (index += 1) {
        var render = prepareGuiBlitStorage(instance, frame, GUI_RASTER_MAX_PIXELS, .xrgb32) orelse return false;
        const words = guiResourcePayloadWords(render.resource.payload);
        @memset(words, @intCast(index + 1));
        _ = commitGuiBlitStorage(&render, .{
            .kind = 4,
            .w = GUI_RASTER_MAX_WIDTH,
            .h = GUI_RASTER_MAX_HEIGHT,
            .flags = 1,
            .resource_kind = .xrgb32,
            .payload_offset = render.resource.payload.logical_offset,
            .payload_bytes = render.resource.payload.byte_count,
            .raster_word_offset = render.resource.payload.raster_word_offset,
        });
    }
    return true;
}

fn testGuiRasterGrowth(owner_id: u32, heap_baseline: heap.Stats, storage_baseline: ProgramInstanceStorageStats) bool {
    var storage = allocateProgramInstanceStorage(owner_id, .gui, false) orelse return false;
    var fake = ProgramInstance{
        .id = owner_id,
        .app_class = .gui,
        .entry = undefined,
        .stack_top = 0,
        .runtime_payload = storage.runtime,
        .process_payload = storage.process,
        .gui_payload = storage.gui,
    };
    const appended = appendGuiRasterTestNodes(&fake, 6);
    const gui = fake.gui_payload orelse return false;
    const frame = gui.building_frame orelse return false;
    const expected_words = 6 * GUI_RASTER_MAX_PIXELS;
    var crossing = [_]u32{0} ** 8;
    const crossing_read = copyGuiRasterWords(frame, GUI_RASTER_MAX_PIXELS - 4, crossing[0..]);
    var crossing_valid = crossing_read == crossing.len;
    for (crossing[0..4]) |word| crossing_valid = crossing_valid and word == 1;
    for (crossing[4..8]) |word| crossing_valid = crossing_valid and word == 2;
    const read_count_is_i32_safe = guiRasterReadCount(
        @as(u64, std.math.maxInt(u32)),
        std.math.maxInt(u32),
    ) == @as(usize, std.math.maxInt(i32));
    const full_chain_valid = appended and frame.command_count == 6 and frame.raster_words == expected_words and
        expected_words >= 256 * 384 and crossing_valid and read_count_is_i32_safe and validateGuiFrame(frame, owner_id);

    gui.building_frame = null;
    const cleared = releaseGuiFrame(frame, owner_id);
    const retried = appendGuiRasterTestNodes(&fake, 1);
    const retry_frame = gui.building_frame orelse return false;
    const retry_head = retry_frame.resource_payload;
    const retry_valid = retried and retry_frame.command_count == 1 and retry_frame.raster_words == GUI_RASTER_MAX_PIXELS and
        retry_head != null and retry_head.?.logical_offset == 0 and retry_frame.resource_tail == retry_head and
        validateGuiFrame(retry_frame, owner_id);
    const valid = full_chain_valid and cleared and retry_valid;
    rollbackProgramInstanceStorage(owner_id, &storage);
    return valid and instanceStorageHeapBaselineEqual(heap_baseline, heap.stats()) and instanceStorageCurrentEqual(storage_baseline, instanceStorageStats());
}

fn testGuiRasterGrowthFailure(owner_id: u32, heap_baseline: heap.Stats, storage_baseline: ProgramInstanceStorageStats) bool {
    var storage = allocateProgramInstanceStorage(owner_id, .gui, false) orelse return false;
    var fake = ProgramInstance{
        .id = owner_id,
        .app_class = .gui,
        .entry = undefined,
        .stack_top = 0,
        .runtime_payload = storage.runtime,
        .process_payload = storage.process,
        .gui_payload = storage.gui,
    };
    if (!appendGuiRasterTestNodes(&fake, 2)) {
        rollbackProgramInstanceStorage(owner_id, &storage);
        return false;
    }
    const gui = fake.gui_payload orelse return false;
    const frame = gui.building_frame orelse return false;
    const old_commands = frame.command_count;
    const old_words = frame.raster_words;
    const old_tail = frame.resource_tail;
    configureInstanceStorageFailureForTest(0);
    const rejected = prepareGuiBlitStorage(&fake, frame, GUI_RASTER_MAX_PIXELS, .xrgb32) == null;
    configureInstanceStorageFailureForTest(null);
    const unchanged = rejected and frame.command_count == old_commands and frame.raster_words == old_words and
        frame.resource_tail == old_tail and validateGuiFrame(frame, owner_id);
    rollbackProgramInstanceStorage(owner_id, &storage);
    return unchanged and instanceStorageHeapBaselineEqual(heap_baseline, heap.stats()) and instanceStorageCurrentEqual(storage_baseline, instanceStorageStats());
}

fn guiFrameSelfTestHash(frame: *const ProgramGuiFramePayload) u64 {
    var hash: u64 = 0xcbf29ce484222325;
    hash = (hash ^ frame.generation) *% 0x100000001b3;
    hash = (hash ^ frame.command_count) *% 0x100000001b3;
    hash = (hash ^ frame.resource_len) *% 0x100000001b3;
    hash = (hash ^ frame.node_sequence) *% 0x100000001b3;
    var command_cursor = frame.command_payload;
    while (command_cursor) |payload| : (command_cursor = payload.next) {
        hash = (hash ^ payload.allocation_sequence) *% 0x100000001b3;
        const commands = guiCommandPayloadCommandsConst(payload);
        for (commands[0..payload.command_count]) |command| {
            hash = (hash ^ command.kind) *% 0x100000001b3;
            hash = (hash ^ command.rgb) *% 0x100000001b3;
            hash = (hash ^ command.payload_offset) *% 0x100000001b3;
            hash = (hash ^ command.payload_bytes) *% 0x100000001b3;
        }
    }
    var resource_cursor = frame.resource_payload;
    while (resource_cursor) |payload| : (resource_cursor = payload.next) {
        hash = (hash ^ payload.allocation_sequence) *% 0x100000001b3;
        for (guiResourcePayloadDataConst(payload)) |byte| hash = (hash ^ byte) *% 0x100000001b3;
    }
    return hash;
}

fn testGuiDynamicCommandGrowth(owner_id: u32, heap_baseline: heap.Stats, storage_baseline: ProgramInstanceStorageStats) bool {
    var storage = allocateProgramInstanceStorage(owner_id, .gui, false) orelse return false;
    var fake = ProgramInstance{
        .id = owner_id,
        .app_class = .gui,
        .entry = undefined,
        .stack_top = 0,
        .runtime_payload = storage.runtime,
        .process_payload = storage.process,
        .gui_payload = storage.gui,
    };
    if (guiFrameBegin(&fake) != 0 or guiFrameCommit(&fake) != 0) {
        rollbackProgramInstanceStorage(owner_id, &storage);
        return false;
    }
    const empty = fake.gui_payload.?.committed_frame orelse return false;
    if (empty.command_count != 0 or !validateGuiFrame(empty, owner_id) or guiFrameBegin(&fake) != 0) {
        rollbackProgramInstanceStorage(owner_id, &storage);
        return false;
    }
    const building = fake.gui_payload.?.building_frame orelse return false;
    const simultaneous_peak_ok = fake.gui_payload.?.frame_peak_bytes >= guiFrameBytes(empty) + guiFrameBytes(building);
    var index: u64 = 0;
    while (index < 4096) : (index += 1) {
        var command = prepareGuiCommandStorage(&fake, building) orelse {
            rollbackProgramInstanceStorage(owner_id, &storage);
            return false;
        };
        _ = commitGuiCommandStorage(&command, .{ .kind = r4x_api.gui_frame_command_kind_rect, .rgb = @intCast(index) });
    }
    const boundaries_ok = building.command_count == 4096 and
        guiFrameCommandAt(building, 0).?.rgb == 0 and
        guiFrameCommandAt(building, 1).?.rgb == 1 and
        guiFrameCommandAt(building, 256).?.rgb == 256 and
        guiFrameCommandAt(building, 4095).?.rgb == 4095 and
        guiFrameCommandAt(building, 4096) == null and simultaneous_peak_ok and
        fake.gui_payload.?.frame_peak_bytes >= guiOwnedFrameBytes(fake.gui_payload.?) and validateGuiFrame(building, owner_id);
    const committed = boundaries_ok and guiFrameCommit(&fake) == 0 and fake.gui_payload.?.committed_frame.?.command_count == 4096;
    rollbackProgramInstanceStorage(owner_id, &storage);
    return committed and instanceStorageHeapBaselineEqual(heap_baseline, heap.stats()) and instanceStorageCurrentEqual(storage_baseline, instanceStorageStats());
}

fn testGuiMixedFrameResources(owner_id: u32, heap_baseline: heap.Stats, storage_baseline: ProgramInstanceStorageStats) bool {
    var storage = allocateProgramInstanceStorage(owner_id, .gui, false) orelse return false;
    var fake = ProgramInstance{
        .id = owner_id,
        .app_class = .gui,
        .entry = undefined,
        .stack_top = 0,
        .runtime_payload = storage.runtime,
        .process_payload = storage.process,
        .gui_payload = storage.gui,
    };
    const frame = guiFrameReplaceBuild(&fake, true) orelse {
        rollbackProgramInstanceStorage(owner_id, &storage);
        return false;
    };
    const expected_alpha = [_]u8{ 1, 2, 3, 4, 5, 6 };
    var resources: [171]u8 = undefined;
    for (resources[0..161], 0..) |*byte, index| byte.* = @intCast('A' + index % 26);
    @memcpy(resources[161..165], &[_]u8{ 0x33, 0x22, 0x11, 0 });
    @memcpy(resources[165..171], expected_alpha[0..]);
    const commands = [_]GuiFrameCommand{
        .{ .kind = r4x_api.gui_frame_command_kind_text, .resource_bytes = 161 },
        .{ .kind = r4x_api.gui_frame_command_kind_raster, .w = 1, .h = 1, .resource_offset = 161, .resource_bytes = 4, .parameter0 = 3 },
        .{ .kind = r4x_api.gui_frame_command_kind_alpha8, .w = 3, .h = 2, .resource_offset = 165, .resource_bytes = 6 },
    };
    if (guiFrameAppendBatch(&fake, commands[0..], resources[0..]) != 0 or !validateGuiFrame(frame, owner_id)) {
        rollbackProgramInstanceStorage(owner_id, &storage);
        return false;
    }

    var copied_text: [161]u8 = undefined;
    const text_roundtrip = copyGuiFrameResourceBytes(frame, 0, copied_text[0..]) == copied_text.len and
        std.mem.eql(u8, copied_text[0..], resources[0..161]);
    var legacy_text: GuiCommand = .{};
    const legacy_prefix = materializeLegacyGuiCommand(frame, guiFrameCommandAt(frame, 0).?, &legacy_text) and
        legacy_text.text[0] == 'A' and legacy_text.text[62] == resources[62] and legacy_text.text[63] == 0;
    var legacy_raster_command: GuiCommand = .{};
    var legacy_alpha_command: GuiCommand = .{};
    const legacy_raster_commands = materializeLegacyGuiCommand(frame, guiFrameCommandAt(frame, 1).?, &legacy_raster_command) and
        materializeLegacyGuiCommand(frame, guiFrameCommandAt(frame, 2).?, &legacy_alpha_command) and
        legacy_raster_command.flags == 3 and legacy_raster_command.rgb == 0 and
        legacy_alpha_command.flags == 1 and legacy_alpha_command.fg == 1 and legacy_alpha_command.bg == 2;
    var legacy_words = [_]u32{0} ** 3;
    const raster_roundtrip = copyGuiRasterWords(frame, 0, legacy_words[0..]) == 3 and
        legacy_words[0] == 0x00112233 and legacy_words[1] == 0x04030201 and legacy_words[2] == 0x00000605;
    var snapshot_bytes: [171]u8 = undefined;
    const text_snapshot = externalGuiFrameCommand(guiFrameCommandAt(frame, 0).?);
    const raster_snapshot = externalGuiFrameCommand(guiFrameCommandAt(frame, 1).?);
    const alpha_snapshot = externalGuiFrameCommand(guiFrameCommandAt(frame, 2).?);
    const snapshot_roundtrip = copyGuiFrameResourceBytes(frame, 0, snapshot_bytes[0..]) == snapshot_bytes.len and
        text_snapshot.flags == 0 and text_snapshot.resource_offset == 0 and text_snapshot.resource_bytes == 161 and
        raster_snapshot.flags == 0 and raster_snapshot.resource_offset == 161 and raster_snapshot.resource_bytes == 4 and raster_snapshot.parameter0 == 3 and
        alpha_snapshot.flags == 0 and alpha_snapshot.resource_offset == 165 and alpha_snapshot.resource_bytes == 6 and alpha_snapshot.parameter0 == 0 and alpha_snapshot.parameter1 == 0 and
        snapshot_bytes[161] == 0x33 and snapshot_bytes[162] == 0x22 and snapshot_bytes[163] == 0x11 and snapshot_bytes[164] == 0 and
        std.mem.eql(u8, snapshot_bytes[165..171], expected_alpha[0..]);
    const clone = cloneGuiFrame(&fake, frame) orelse return false;
    const clone_ok = clone.raster_words == frame.raster_words and validateGuiFrame(clone, owner_id) and
        guiFrameSelfTestHash(clone) == guiFrameSelfTestHash(frame);
    _ = releaseGuiFrame(clone, owner_id);
    const committed = guiFrameCommit(&fake) == 0;
    var mixed_append = false;
    if (committed) {
        if (guiFrameEnsureBuild(&fake)) |mixed_build| {
            if (prepareGuiBlitStorage(&fake, mixed_build, 1, .xrgb32)) |prepared| {
                var mixed_render = prepared;
                guiResourcePayloadWords(mixed_render.resource.payload)[0] = 0x0055_4433;
                mixed_append = commitGuiBlitStorage(&mixed_render, .{
                    .kind = r4x_api.gui_frame_command_kind_raster,
                    .w = 1,
                    .h = 1,
                    .flags = 1,
                    .resource_kind = .xrgb32,
                    .payload_offset = mixed_render.resource.payload.logical_offset,
                    .payload_bytes = 4,
                    .raster_word_offset = mixed_render.resource.payload.raster_word_offset,
                    .parameter0 = 1,
                }) == 4;
            }
        }
    }
    const mixed_frame = fake.gui_payload.?.building_frame;
    const mixed_ok = mixed_frame != null and mixed_frame.?.raster_words == 4 and mixed_frame.?.command_count == 4 and
        mixed_frame.?.resource_len == 175 and mixed_frame.?.resource_tail.?.resource_kind == .xrgb32 and
        mixed_frame.?.resource_tail.?.raster_word_offset == 3 and validateGuiFrame(mixed_frame.?, owner_id);
    const mixed_committed = mixed_ok and guiFrameCommit(&fake) == 0 and fake.gui_payload.?.committed_frame.?.raster_words == 4;

    var mixed_reclone_append = false;
    if (mixed_committed) {
        if (guiFrameEnsureBuild(&fake)) |mixed_reclone| {
            if (validateGuiFrame(mixed_reclone, owner_id) and mixed_reclone.resource_tail.?.raster_word_offset == 3) {
                if (prepareGuiBlitStorage(&fake, mixed_reclone, 1, .xrgb32)) |prepared| {
                    var mixed_render = prepared;
                    guiResourcePayloadWords(mixed_render.resource.payload)[0] = 0x0066_5544;
                    mixed_reclone_append = commitGuiBlitStorage(&mixed_render, .{
                        .kind = r4x_api.gui_frame_command_kind_raster,
                        .w = 1,
                        .h = 1,
                        .flags = 1,
                        .resource_kind = .xrgb32,
                        .payload_offset = mixed_render.resource.payload.logical_offset,
                        .payload_bytes = 4,
                        .raster_word_offset = mixed_render.resource.payload.raster_word_offset,
                        .parameter0 = 1,
                    }) == 5;
                }
            }
        }
    }
    const mixed_reclone = fake.gui_payload.?.building_frame;
    const mixed_reclone_ok = mixed_reclone != null and mixed_reclone.?.raster_words == 5 and mixed_reclone.?.command_count == 5 and
        mixed_reclone.?.resource_len == 179 and mixed_reclone.?.resource_payload.?.next.?.raster_word_offset == 3 and
        mixed_reclone.?.resource_tail.?.raster_word_offset == 4 and validateGuiFrame(mixed_reclone.?, owner_id);
    const mixed_reclone_committed = mixed_reclone_ok and guiFrameCommit(&fake) == 0 and fake.gui_payload.?.committed_frame.?.raster_words == 5;

    const native_alpha_frame = guiFrameReplaceBuild(&fake, true) orelse return false;
    var native_alpha = prepareGuiBlitStorage(&fake, native_alpha_frame, 2, .alpha8) orelse return false;
    const native_alpha_bytes = guiResourcePayloadData(native_alpha.resource.payload);
    @memset(native_alpha_bytes, 0);
    @memcpy(native_alpha_bytes[0..6], expected_alpha[0..]);
    _ = commitGuiBlitStorage(&native_alpha, .{
        .kind = r4x_api.gui_frame_command_kind_alpha8,
        .w = 3,
        .h = 2,
        .resource_kind = .alpha8,
        .payload_offset = native_alpha.resource.payload.logical_offset,
        .payload_bytes = 6,
        .raster_word_offset = native_alpha.resource.payload.raster_word_offset,
    });
    const native_alpha_snapshot = externalGuiFrameCommand(guiFrameCommandAt(native_alpha_frame, 0).?);
    var native_alpha_blob = [_]u8{0xCC} ** 8;
    var native_alpha_words = [_]u32{0} ** 2;
    const native_sequence_ok = native_alpha_frame.command_tail.?.allocation_sequence == 1 and
        native_alpha_frame.resource_tail.?.allocation_sequence == 2 and native_alpha_frame.node_sequence == 2;
    const native_alpha_ok = validateGuiFrame(native_alpha_frame, owner_id) and native_alpha_frame.resource_len == 8 and
        native_alpha_snapshot.resource_offset == 0 and native_alpha_snapshot.resource_bytes == 6 and native_alpha_snapshot.parameter0 == 0 and native_alpha_snapshot.parameter1 == 0 and
        copyGuiFrameResourceBytes(native_alpha_frame, 0, native_alpha_blob[0..]) == 8 and std.mem.eql(u8, native_alpha_blob[0..6], expected_alpha[0..]) and
        native_alpha_blob[6] == 0 and native_alpha_blob[7] == 0 and copyGuiRasterWords(native_alpha_frame, 0, native_alpha_words[0..]) == 2 and
        native_alpha_words[0] == 0x04030201 and native_alpha_words[1] == 0x00000605;
    gui_frame_release_trace_len = 0;
    gui_frame_release_trace_enabled = true;
    const native_cancelled = guiFrameCancel(&fake) == 0;
    gui_frame_release_trace_enabled = false;
    const native_reverse_release_ok = gui_frame_release_trace_len == 3 and
        gui_frame_release_trace[0] == .gui_raster and gui_frame_release_trace[1] == .gui_commands and
        gui_frame_release_trace[2] == .gui_frame;

    const shape_frame = guiFrameReplaceBuild(&fake, true) orelse return false;
    const shape_header = GuiShapeResource{
        .geometry_kind = r4x_api.gui_shape_geometry_kind_path,
        .segment_count = 2,
        .fill_rule = r4x_api.gui_shape_fill_rule_nonzero,
        .line_join = r4x_api.gui_shape_line_join_miter,
        .line_cap = r4x_api.gui_shape_line_cap_square,
        .stroke_argb = 0xFF112233,
        .stroke_width_bits = @bitCast(@as(f32, 2)),
        .miter_limit_bits = @bitCast(@as(f32, 4)),
    };
    const shape_segments = [_]GuiPathSegment{
        .{ .kind = r4x_api.gui_path_segment_kind_move, .x1_bits = @bitCast(@as(f32, 1)), .y1_bits = @bitCast(@as(f32, 2)) },
        .{ .kind = r4x_api.gui_path_segment_kind_line, .x1_bits = @bitCast(@as(f32, 20)), .y1_bits = @bitCast(@as(f32, 9)) },
    };
    var shape_resource: [@sizeOf(GuiShapeResource) + shape_segments.len * @sizeOf(GuiPathSegment)]u8 = undefined;
    @memcpy(shape_resource[0..@sizeOf(GuiShapeResource)], std.mem.asBytes(&shape_header));
    @memcpy(shape_resource[@sizeOf(GuiShapeResource)..], std.mem.sliceAsBytes(shape_segments[0..]));
    const shape_command = GuiFrameCommand{
        .kind = r4x_api.gui_frame_command_kind_path_stroke,
        .w = 32,
        .h = 16,
        .resource_bytes = shape_resource.len,
    };
    const shape_batch_ok = guiFrameAppendBatch(&fake, (&[_]GuiFrameCommand{shape_command})[0..], shape_resource[0..]) == 0 and
        validateGuiFrame(shape_frame, owner_id) and shape_frame.command_count == 1 and shape_frame.resource_len == shape_resource.len and
        guiFrameCommandAt(shape_frame, 0).?.resource_kind == .path;
    var truncated = shape_command;
    truncated.resource_bytes -= 1;
    var invalid_offset = shape_command;
    invalid_offset.resource_offset = 1;
    var oversized = shape_command;
    oversized.w = r4x_api.gui_shape_max_dimension + 1;
    var invalid_kind_resource = shape_resource;
    invalid_kind_resource[@sizeOf(GuiShapeResource) + @offsetOf(GuiPathSegment, "kind")] = 99;
    var non_finite_resource = shape_resource;
    const coordinate_offset = @sizeOf(GuiShapeResource) + @offsetOf(GuiPathSegment, "x1_bits");
    @memcpy(non_finite_resource[coordinate_offset .. coordinate_offset + 4], &[_]u8{ 0, 0, 0x80, 0x7F });
    const shape_negative_ok = !validateGuiFrameBatchCommand(&truncated, shape_resource[0..]) and
        !validateGuiFrameBatchCommand(&invalid_offset, shape_resource[0..]) and
        !validateGuiFrameBatchCommand(&oversized, shape_resource[0..]) and
        !validateGuiFrameBatchCommand(&shape_command, invalid_kind_resource[0..]) and
        !validateGuiFrameBatchCommand(&shape_command, non_finite_resource[0..]);
    const shape_cancelled = guiFrameCancel(&fake) == 0;

    const argb_frame = guiFrameReplaceBuild(&fake, true) orelse return false;
    const argb_resource = [_]u8{ 0x33, 0x22, 0x11, 0x80, 0x66, 0x55, 0x44, 0xFF };
    const argb_command = GuiFrameCommand{
        .kind = r4x_api.gui_frame_command_kind_argb32,
        .w = 2,
        .h = 1,
        .resource_bytes = argb_resource.len,
        .parameter0 = 2,
    };
    const argb_batch_ok = guiFrameAppendBatch(&fake, (&[_]GuiFrameCommand{argb_command})[0..], argb_resource[0..]) == 0 and
        validateGuiFrame(argb_frame, owner_id) and argb_frame.command_count == 1 and argb_frame.raster_words == 0 and
        guiFrameCommandAt(argb_frame, 0).?.resource_kind == .path;
    var legacy_argb: GuiCommand = .{};
    var invalid_argb_size = argb_command;
    invalid_argb_size.resource_bytes -= 1;
    var invalid_argb_scale = argb_command;
    invalid_argb_scale.parameter0 = 0;
    var invalid_argb_width = argb_command;
    invalid_argb_width.w = r4x_api.gui_argb32_max_width + 1;
    const argb_negative_ok = !materializeLegacyGuiCommand(argb_frame, guiFrameCommandAt(argb_frame, 0).?, &legacy_argb) and
        !validateGuiFrameBatchCommand(&invalid_argb_size, argb_resource[0..]) and
        !validateGuiFrameBatchCommand(&invalid_argb_scale, argb_resource[0..]) and
        !validateGuiFrameBatchCommand(&invalid_argb_width, argb_resource[0..]);
    const argb_cancelled = guiFrameCancel(&fake) == 0;

    const invalid_header = GuiFrameInfo{ .version = 0, .size = r4x_api.gui_frame_info_size, .last_error = 0x12345678 };
    const invalid_header_before = invalid_header;
    const invalid_header_ok = !validGuiFrameInfoOutput(&invalid_header) and std.meta.eql(invalid_header, invalid_header_before);
    const future_header = GuiFrameInfo{ .version = r4x_api.gui_frame_info_version + 1, .size = r4x_api.gui_frame_info_size };
    const future_header_ok = validGuiFrameInfoOutput(&future_header);
    const valid = text_roundtrip and legacy_prefix and legacy_raster_commands and raster_roundtrip and snapshot_roundtrip and clone_ok and mixed_append and mixed_committed and
        mixed_reclone_append and mixed_reclone_committed and native_sequence_ok and native_alpha_ok and native_cancelled and native_reverse_release_ok and
        shape_batch_ok and shape_negative_ok and shape_cancelled and argb_batch_ok and argb_negative_ok and argb_cancelled and
        invalid_header_ok and future_header_ok;
    rollbackProgramInstanceStorage(owner_id, &storage);
    return valid and instanceStorageHeapBaselineEqual(heap_baseline, heap.stats()) and instanceStorageCurrentEqual(storage_baseline, instanceStorageStats());
}

fn testGuiAtomicCommitGrowthFailure(owner_id: u32, heap_baseline: heap.Stats, storage_baseline: ProgramInstanceStorageStats) bool {
    var storage = allocateProgramInstanceStorage(owner_id, .gui, false) orelse return false;
    var fake = ProgramInstance{
        .id = owner_id,
        .app_class = .gui,
        .entry = undefined,
        .stack_top = 0,
        .runtime_payload = storage.runtime,
        .process_payload = storage.process,
        .gui_payload = storage.gui,
    };
    const initial = guiFrameReplaceBuild(&fake, true) orelse return false;
    var initial_command = prepareGuiCommandStorage(&fake, initial) orelse return false;
    _ = commitGuiCommandStorage(&initial_command, .{ .kind = r4x_api.gui_frame_command_kind_clear, .rgb = 0x123456 });
    if (guiFrameCommit(&fake) != 0) return false;
    const committed = fake.gui_payload.?.committed_frame orelse return false;
    const committed_generation = committed.generation;
    const committed_hash = guiFrameSelfTestHash(committed);
    const growth_boundaries = [_]usize{ 0, 8, 24, 56, 120, 248 };
    for (growth_boundaries) |boundary| {
        if (guiFrameBegin(&fake) != 0) return false;
        const building = fake.gui_payload.?.building_frame orelse return false;
        for (0..boundary) |index| {
            var command = prepareGuiCommandStorage(&fake, building) orelse return false;
            _ = commitGuiCommandStorage(&command, .{ .kind = r4x_api.gui_frame_command_kind_rect, .rgb = @intCast(index) });
        }
        configureInstanceStorageFailureForTest(0);
        const rejected = prepareGuiCommandStorage(&fake, building) == null;
        configureInstanceStorageFailureForTest(null);
        guiFrameMarkBuildFailed(fake.gui_payload.?, building);
        if (!rejected or guiFrameCommit(&fake) != r4x_api.gui_frame_error_state or
            fake.gui_payload.?.committed_frame != committed or committed.generation != committed_generation or
            guiFrameSelfTestHash(committed) != committed_hash or guiFrameCancel(&fake) != 0)
        {
            rollbackProgramInstanceStorage(owner_id, &storage);
            return false;
        }
    }

    // Exercise every real allocation stage of a multi-block batch: delta
    // frame, two command blocks, first resource block and second resource
    // block.  The last two failures cover zero and one already-linked resource
    // nodes after semantically incomplete commands, which must still unwind to
    // the exact pre-BEGIN heap/storage baseline.
    const fault_resource = heap.alloc(GUI_RESOURCE_BLOCK_TARGET_BYTES + 1, 1) orelse {
        rollbackProgramInstanceStorage(owner_id, &storage);
        return false;
    };
    @memset(fault_resource, 0);
    var fault_commands = [_]GuiFrameCommand{.{ .kind = r4x_api.gui_frame_command_kind_rect }} ** 9;
    fault_commands[8] = .{
        .kind = r4x_api.gui_frame_command_kind_text,
        .resource_bytes = fault_resource.len,
    };
    var batch_faults_ok = true;
    for (0..5) |fail_after| {
        const iteration_heap = heap.stats();
        const iteration_storage = instanceStorageStats();
        if (guiFrameBegin(&fake) != 0) {
            batch_faults_ok = false;
            break;
        }
        configureInstanceStorageFailureForTest(@intCast(fail_after));
        const rejected = guiFrameAppendBatch(&fake, fault_commands[0..], fault_resource) == r4x_api.gui_frame_error_oom;
        configureInstanceStorageFailureForTest(null);
        const failed_build = fake.gui_payload.?.building_frame != null and fake.gui_payload.?.building_frame.?.build_failed;
        const old_unchanged = guiFrameCommit(&fake) == r4x_api.gui_frame_error_state and
            fake.gui_payload.?.committed_frame == committed and committed.generation == committed_generation and
            guiFrameSelfTestHash(committed) == committed_hash;
        const cancelled = guiFrameCancel(&fake) == 0;
        if (!rejected or !failed_build or !old_unchanged or !cancelled or
            !instanceStorageHeapBaselineEqual(iteration_heap, heap.stats()) or
            !instanceStorageCurrentEqual(iteration_storage, instanceStorageStats()))
        {
            batch_faults_ok = false;
            break;
        }
    }
    configureInstanceStorageFailureForTest(null);
    if (fake.gui_payload.?.building_frame != null) _ = guiFrameCancel(&fake);
    const fault_resource_released = heap.free(fault_resource) == .ok;
    const old_text_ready = guiSetTextForInstance(&fake, "old") == 3;
    const text_committed = fake.gui_payload.?.committed_frame orelse return false;
    const text_generation = text_committed.generation;
    const text_hash = guiFrameSelfTestHash(text_committed);
    configureInstanceStorageFailureForTest(0);
    const text_oom_rejected = guiSetTextForInstance(&fake, "new") == -2;
    configureInstanceStorageFailureForTest(null);
    const text_oom_atomic = old_text_ready and text_oom_rejected and fake.gui_payload.?.building_frame == null and
        fake.gui_payload.?.committed_frame == text_committed and text_committed.generation == text_generation and
        guiFrameSelfTestHash(text_committed) == text_hash and std.mem.eql(u8, fake.gui_payload.?.text[0..3], "old") and
        fake.gui_payload.?.text[3] == 0;
    rollbackProgramInstanceStorage(owner_id, &storage);
    return batch_faults_ok and fault_resource_released and text_oom_atomic and instanceStorageHeapBaselineEqual(heap_baseline, heap.stats()) and
        instanceStorageCurrentEqual(storage_baseline, instanceStorageStats());
}

fn testGuiRetiredReaderLifetime(owner_id: u32, heap_baseline: heap.Stats, storage_baseline: ProgramInstanceStorageStats) bool {
    var storage = allocateProgramInstanceStorage(owner_id, .gui, false) orelse return false;
    var fake = ProgramInstance{
        .id = owner_id,
        .app_class = .gui,
        .entry = undefined,
        .stack_top = 0,
        .runtime_payload = storage.runtime,
        .process_payload = storage.process,
        .gui_payload = storage.gui,
    };
    const first = guiFrameReplaceBuild(&fake, true) orelse return false;
    var first_command = prepareGuiCommandStorage(&fake, first) orelse return false;
    _ = commitGuiCommandStorage(&first_command, .{ .kind = r4x_api.gui_frame_command_kind_rect, .rgb = 1 });
    if (guiFrameCommit(&fake) != 0) return false;
    var capture = captureCommittedGuiFrame(&fake, null) orelse return false;
    const first_generation = capture.generation;
    const first_hash = guiFrameSelfTestHash(capture.frame);
    if (guiFrameBegin(&fake) != 0) return false;
    const second = fake.gui_payload.?.building_frame orelse return false;
    var second_command = prepareGuiCommandStorage(&fake, second) orelse return false;
    _ = commitGuiCommandStorage(&second_command, .{ .kind = r4x_api.gui_frame_command_kind_rect, .rgb = 2 });
    if (guiFrameCommit(&fake) != 0) return false;
    const gui = fake.gui_payload.?;
    const retired_ok = gui.retired_frames == capture.frame and capture.frame.retired and capture.frame.reader_refs == 1 and
        capture.frame.generation == first_generation and guiFrameSelfTestHash(capture.frame) == first_hash and
        captureCommittedGuiFrame(&fake, first_generation) == null and gui.frame_peak_bytes >= guiOwnedFrameBytes(gui);
    releaseCapturedGuiFrame(&fake, &capture);
    const released = gui.retired_frames == null;
    rollbackProgramInstanceStorage(owner_id, &storage);
    return retired_ok and released and instanceStorageHeapBaselineEqual(heap_baseline, heap.stats()) and instanceStorageCurrentEqual(storage_baseline, instanceStorageStats());
}

fn testGuiDeltaIndexed8(owner_id: u32, heap_baseline: heap.Stats, storage_baseline: ProgramInstanceStorageStats) bool {
    var storage = allocateProgramInstanceStorage(owner_id, .gui, false) orelse return false;
    var fake = ProgramInstance{
        .id = owner_id,
        .app_class = .gui,
        .entry = undefined,
        .stack_top = 0,
        .runtime_payload = storage.runtime,
        .process_payload = storage.process,
        .gui_payload = storage.gui,
    };
    const valid = blk: {
        if (guiFrameBegin(&fake) != 0) break :blk false;
        const base_commands = [_]GuiFrameCommand{
            .{ .kind = r4x_api.gui_frame_command_kind_clear, .rgb = 0x010203 },
            .{ .kind = r4x_api.gui_frame_command_kind_text, .resource_bytes = 1 },
        };
        if (guiFrameAppendBatch(&fake, base_commands[0..], "B") != 0 or guiFrameCommit(&fake) != 0) break :blk false;
        const base = fake.gui_payload.?.committed_frame orelse break :blk false;
        const base_generation = base.generation;
        const base_bytes = guiFrameBytes(base);

        const damage = [_]DisplayDamageRect{
            .{ .x = 0, .y = 0, .w = 1, .h = 1 },
            .{ .x = 3, .y = 3, .w = 1, .h = 1 },
        };
        if (guiFrameBeginDamage(&fake, damage[0..]) != 0) break :blk false;
        var resource = [_]u8{0} ** (r4x_api.gui_indexed8_pixels_offset + 4);
        var header = GuiIndexed8Resource{
            .source_w = 2,
            .source_h = 2,
            .guest_w = 2,
            .guest_h = 2,
            .viewport_w = 4,
            .viewport_h = 4,
            .pixel_stride = 2,
        };
        @memcpy(resource[0..@sizeOf(GuiIndexed8Resource)], std.mem.asBytes(&header));
        @memcpy(resource[r4x_api.gui_indexed8_palette_offset + 4 .. r4x_api.gui_indexed8_palette_offset + 8], &[_]u8{ 0x33, 0x22, 0x11, 0 });
        @memcpy(resource[r4x_api.gui_indexed8_pixels_offset..], &[_]u8{ 0, 1, 1, 0 });
        const indexed_command = GuiFrameCommand{
            .kind = r4x_api.gui_frame_command_kind_indexed8,
            .w = 4,
            .h = 4,
            .resource_bytes = resource.len,
        };
        if (!validateGuiFrameBatchCommand(&indexed_command, resource[0..]) or
            guiFrameAppendBatch(&fake, (&[_]GuiFrameCommand{indexed_command})[0..], resource[0..]) != 0 or
            guiFrameCommit(&fake) != 0) break :blk false;

        const delta = fake.gui_payload.?.committed_frame orelse break :blk false;
        const gui = fake.gui_payload.?;
        if (delta.base_frame != base or delta.chain_depth != 2 or delta.damage_count != damage.len or
            delta.command_count != 1 or guiFrameChainCommandCount(delta) != 3 or
            guiFrameChainResourceBytes(delta) != resource.len + 1 or
            gui.frame_delta_commits != 1 or gui.frame_full_commits != 1 or
            gui.frame_indexed8_commands != 1 or gui.frame_indexed8_resource_bytes != resource.len or
            gui.frame_avoided_clone_bytes < base_bytes) break :blk false;

        var info: GuiFrameGenerationInfo = .{};
        fillGuiFrameGenerationInfo(gui, .{ .instance_id = owner_id, .generation = 1 }, delta, &info);
        if ((info.flags & r4x_api.gui_frame_generation_flag_delta) == 0 or
            (info.flags & r4x_api.gui_frame_generation_flag_indexed8) == 0 or
            info.base_generation != base_generation or info.command_count != 1 or
            info.total_command_count != 3 or info.damage_count != damage.len) break :blk false;

        var local_commands: [1]GuiFrameCommand = undefined;
        var full_commands: [3]GuiFrameCommand = undefined;
        var local_resource: [resource.len]u8 = undefined;
        var local_damage: [damage.len]DisplayDamageRect = undefined;
        if (copyGuiFrameLocalCommands(delta, 0, local_commands[0..]) != local_commands.len or
            copyGuiFrameCommands(delta, full_commands[0..]) != full_commands.len or
            copyGuiFrameLocalResourceBytes(delta, 0, local_resource[0..]) != local_resource.len) break :blk false;
        @memcpy(local_damage[0..], delta.damage_regions[0..damage.len]);
        if (local_commands[0].kind != r4x_api.gui_frame_command_kind_indexed8 or
            local_commands[0].resource_offset != 0 or full_commands[2].resource_offset != 1 or
            !std.mem.eql(u8, local_resource[0..], resource[0..]) or !std.meta.eql(local_damage, damage)) break :blk false;

        var bad_header = header;
        bad_header.pixel_stride = 3;
        var invalid_resource = resource;
        @memcpy(invalid_resource[0..@sizeOf(GuiIndexed8Resource)], std.mem.asBytes(&bad_header));
        if (validateGuiFrameBatchCommand(&indexed_command, invalid_resource[0..])) break :blk false;

        var base_capture = captureGuiFrameGeneration(&fake, base_generation) orelse break :blk false;
        if (base_capture.frame != base or base.reader_refs != 1) break :blk false;
        if (guiFrameBegin(&fake) != 0) break :blk false;
        const replacement = [_]GuiFrameCommand{.{ .kind = r4x_api.gui_frame_command_kind_clear, .rgb = 0x0A0B0C }};
        if (guiFrameAppendBatch(&fake, replacement[0..], &.{}) != 0 or guiFrameCommit(&fake) != 0 or gui.retired_frames != delta) break :blk false;
        releaseCapturedGuiFrame(&fake, &base_capture);
        if (gui.retired_frames != null) break :blk false;
        break :blk true;
    };
    rollbackProgramInstanceStorage(owner_id, &storage);
    return valid and instanceStorageHeapBaselineEqual(heap_baseline, heap.stats()) and instanceStorageCurrentEqual(storage_baseline, instanceStorageStats());
}

fn failInstanceStorageSelfTest(case_id: u32, heap_baseline: heap.Stats, storage_baseline: ProgramInstanceStorageStats) bool {
    const heap_ok = instanceStorageHeapBaselineEqual(heap_baseline, heap.stats());
    const storage_ok = instanceStorageCurrentEqual(storage_baseline, instanceStorageStats());
    instance_storage_self_test_report = .{
        .cases = case_id,
        .failed_case = case_id,
        .peak_payload_bytes = instance_storage_self_test_peak_payload_bytes,
        .heap_baseline_ok = heap_ok,
        .storage_baseline_ok = storage_ok,
        .zero_init_ok = false,
    };
    k.puts("[PINSTSTOR] result=FAILED case=");
    k.putDec(case_id);
    k.puts(" heap_baseline=");
    k.puts(if (heap_ok) "ok" else "failed");
    k.puts(" storage_baseline=");
    k.puts(if (storage_ok) "ok" else "failed");
    k.puts("\r\n");
    return false;
}

fn instanceStorageHeapBaselineEqual(a: heap.Stats, b: heap.Stats) bool {
    return a.committed_bytes == b.committed_bytes and
        a.used_bytes == b.used_bytes and
        a.free_bytes == b.free_bytes and
        a.active_blocks == b.active_blocks and
        a.free_blocks == b.free_blocks and
        a.largest_free == b.largest_free and
        a.invalid_free_errors == b.invalid_free_errors and
        a.double_free_errors == b.double_free_errors and
        a.size_mismatch_errors == b.size_mismatch_errors and
        a.reentry_errors == b.reentry_errors;
}

fn instanceStorageCurrentEqual(a: ProgramInstanceStorageStats, b: ProgramInstanceStorageStats) bool {
    return a.live_core_bytes == b.live_core_bytes and
        a.active_instance_bytes == b.active_instance_bytes and
        a.reserved_instance_bytes == b.reserved_instance_bytes and
        a.current_payload_bytes == b.current_payload_bytes and
        a.current_runtime_bytes == b.current_runtime_bytes and
        a.current_console_bytes == b.current_console_bytes and
        a.current_gui_bytes == b.current_gui_bytes and
        a.active_instances == b.active_instances and
        a.active_service_instances == b.active_service_instances and
        a.active_console_instances == b.active_console_instances and
        a.active_gui_instances == b.active_gui_instances and
        a.active_runtime_payloads == b.active_runtime_payloads and
        a.active_process_payloads == b.active_process_payloads and
        a.active_environment_payloads == b.active_environment_payloads and
        a.active_console_payloads == b.active_console_payloads and
        a.active_console_output_payloads == b.active_console_output_payloads and
        a.active_gui_payloads == b.active_gui_payloads and
        a.active_gui_frame_payloads == b.active_gui_frame_payloads and
        a.active_gui_command_payloads == b.active_gui_command_payloads and
        a.active_gui_raster_payloads == b.active_gui_raster_payloads and
        a.active_gui_data_payloads == b.active_gui_data_payloads and
        a.current_gui_frame_bytes == b.current_gui_frame_bytes and
        a.current_gui_frame_commands == b.current_gui_frame_commands and
        a.current_gui_frame_nodes == b.current_gui_frame_nodes and
        a.owner_mismatches == b.owner_mismatches and
        a.header_errors == b.header_errors and
        a.free_failures == b.free_failures and
        a.quarantined_payloads == b.quarantined_payloads and
        a.quarantined_bytes == b.quarantined_bytes;
}

fn notePayloadAllocation(kind: ProgramPayloadKind, bytes: usize) void {
    const count = payloadCount(kind);
    count.* +%= 1;
    const category = payloadCategoryBytes(kind);
    category.current.* +%= @intCast(bytes);
    if (category.current.* > category.peak.*) category.peak.* = category.current.*;
    instance_storage_stats.current_payload_bytes +%= @intCast(bytes);
    if (isGuiFramePayloadKind(kind)) {
        instance_storage_stats.current_gui_frame_bytes +%= @intCast(bytes);
        instance_storage_stats.current_gui_frame_nodes +%= 1;
        if (instance_storage_stats.current_gui_frame_bytes > instance_storage_stats.peak_gui_frame_bytes) {
            instance_storage_stats.peak_gui_frame_bytes = instance_storage_stats.current_gui_frame_bytes;
        }
        if (instance_storage_stats.current_gui_frame_nodes > instance_storage_stats.peak_gui_frame_nodes) {
            instance_storage_stats.peak_gui_frame_nodes = instance_storage_stats.current_gui_frame_nodes;
        }
    }
    if (instance_storage_stats.current_payload_bytes > instance_storage_stats.peak_payload_bytes) {
        instance_storage_stats.peak_payload_bytes = instance_storage_stats.current_payload_bytes;
    }
    if (instance_storage_self_test_active and instance_storage_stats.current_payload_bytes > instance_storage_self_test_peak_payload_bytes) {
        instance_storage_self_test_peak_payload_bytes = instance_storage_stats.current_payload_bytes;
    }
    refreshInstanceByteTelemetry();
}

fn notePayloadRelease(kind: ProgramPayloadKind, bytes: usize) void {
    const count = payloadCount(kind);
    if (count.* > 0) count.* -= 1 else instance_storage_stats.header_errors +%= 1;
    const category = payloadCategoryBytes(kind);
    const byte_count: u64 = @intCast(bytes);
    if (category.current.* >= byte_count) category.current.* -= byte_count else instance_storage_stats.header_errors +%= 1;
    if (instance_storage_stats.current_payload_bytes >= byte_count) {
        instance_storage_stats.current_payload_bytes -= byte_count;
    } else {
        instance_storage_stats.header_errors +%= 1;
    }
    if (isGuiFramePayloadKind(kind)) {
        if (instance_storage_stats.current_gui_frame_bytes >= byte_count) {
            instance_storage_stats.current_gui_frame_bytes -= byte_count;
        } else {
            instance_storage_stats.current_gui_frame_bytes = 0;
            instance_storage_stats.header_errors +%= 1;
        }
        if (instance_storage_stats.current_gui_frame_nodes > 0) {
            instance_storage_stats.current_gui_frame_nodes -= 1;
        } else {
            instance_storage_stats.header_errors +%= 1;
        }
    }
    refreshInstanceByteTelemetry();
}

fn isGuiFramePayloadKind(kind: ProgramPayloadKind) bool {
    return kind == .gui_frame or kind == .gui_commands or kind == .gui_raster or kind == .gui_frame_data;
}

fn noteGuiFrameCommandsAllocation(count: u64) void {
    instance_storage_stats.current_gui_frame_commands +%= count;
    if (instance_storage_stats.current_gui_frame_commands > instance_storage_stats.peak_gui_frame_commands) {
        instance_storage_stats.peak_gui_frame_commands = instance_storage_stats.current_gui_frame_commands;
    }
}

fn noteGuiFrameCommandsRelease(count: u64) void {
    if (instance_storage_stats.current_gui_frame_commands >= count) {
        instance_storage_stats.current_gui_frame_commands -= count;
    } else {
        instance_storage_stats.current_gui_frame_commands = 0;
        instance_storage_stats.header_errors +%= 1;
    }
}

const PayloadByteCounters = struct {
    current: *u64,
    peak: *u64,
};

fn payloadCategoryBytes(kind: ProgramPayloadKind) PayloadByteCounters {
    return switch (kind) {
        .runtime, .process, .environment => .{ .current = &instance_storage_stats.current_runtime_bytes, .peak = &instance_storage_stats.peak_runtime_bytes },
        .console, .console_output, .console_transcript => .{ .current = &instance_storage_stats.current_console_bytes, .peak = &instance_storage_stats.peak_console_bytes },
        .gui, .gui_frame, .gui_commands, .gui_raster, .gui_frame_data => .{ .current = &instance_storage_stats.current_gui_bytes, .peak = &instance_storage_stats.peak_gui_bytes },
    };
}

fn payloadCount(kind: ProgramPayloadKind) *u32 {
    return switch (kind) {
        .runtime => &instance_storage_stats.active_runtime_payloads,
        .process => &instance_storage_stats.active_process_payloads,
        .environment => &instance_storage_stats.active_environment_payloads,
        .console => &instance_storage_stats.active_console_payloads,
        .console_output => &instance_storage_stats.active_console_output_payloads,
        .console_transcript => &instance_storage_stats.active_console_output_payloads,
        .gui => &instance_storage_stats.active_gui_payloads,
        .gui_frame => &instance_storage_stats.active_gui_frame_payloads,
        .gui_commands => &instance_storage_stats.active_gui_command_payloads,
        .gui_raster => &instance_storage_stats.active_gui_raster_payloads,
        .gui_frame_data => &instance_storage_stats.active_gui_data_payloads,
    };
}

fn refreshInstanceByteTelemetry() void {
    instance_storage_stats.live_core_bytes = @as(u64, instance_storage_stats.active_instances) * @sizeOf(ProgramInstance);
    instance_storage_stats.active_instance_bytes = instance_storage_stats.live_core_bytes + active_published_payload_bytes;
    instance_storage_stats.reserved_instance_bytes = instance_storage_stats.registry_reserved_core_bytes + instance_storage_stats.current_payload_bytes;
    if (instance_storage_stats.active_instance_bytes > instance_storage_stats.peak_active_instance_bytes) {
        instance_storage_stats.peak_active_instance_bytes = instance_storage_stats.active_instance_bytes;
    }
    if (instance_storage_stats.reserved_instance_bytes > instance_storage_stats.peak_reserved_instance_bytes) {
        instance_storage_stats.peak_reserved_instance_bytes = instance_storage_stats.reserved_instance_bytes;
    }
}

fn noteActivePayloadAllocation(bytes: usize) void {
    active_published_payload_bytes +%= @intCast(bytes);
    refreshInstanceByteTelemetry();
}

fn noteActivePayloadRelease(bytes: usize) void {
    const byte_count: u64 = @intCast(bytes);
    if (active_published_payload_bytes >= byte_count) {
        active_published_payload_bytes -= byte_count;
    } else {
        active_published_payload_bytes = 0;
        instance_storage_stats.header_errors +%= 1;
    }
    refreshInstanceByteTelemetry();
}

fn programInstancePayloadBytes(instance: *const ProgramInstance) u64 {
    var bytes: u64 = 0;
    if (instance.runtime_payload != null) bytes += @sizeOf(ProgramRuntimePayload);
    if (instance.process_payload) |process| {
        bytes += @sizeOf(ProgramProcessPayload);
        if (process.environment_payload != null) bytes += @sizeOf(ProgramEnvironmentPayload);
    }
    if (instance.console_payload) |console| {
        bytes += @sizeOf(ProgramConsolePayload);
        if (console.transcript_payload != null) bytes += @sizeOf(ProgramConsoleTranscriptPayload);
        if (console.writer_payload != null) bytes += @sizeOf(ProgramConsoleOutputPayload);
    }
    if (instance.gui_payload) |gui| {
        bytes += @sizeOf(ProgramGuiPayload);
        if (gui.committed_frame) |frame| bytes += guiFrameBytes(frame);
        if (gui.building_frame) |frame| bytes += guiFrameBytes(frame);
        var retired = gui.retired_frames;
        while (retired) |frame| : (retired = frame.retired_next) bytes += guiFrameBytes(frame);
    }
    return bytes;
}

fn noteProgramInstancePublished(app_class: AppClass, payload_bytes: u64) void {
    instance_storage_stats.active_instances +%= 1;
    active_published_payload_bytes +%= payload_bytes;
    switch (app_class) {
        .service => instance_storage_stats.active_service_instances +%= 1,
        .console => instance_storage_stats.active_console_instances +%= 1,
        .gui => instance_storage_stats.active_gui_instances +%= 1,
    }
    refreshInstanceByteTelemetry();
}

fn noteProgramInstanceRetired(app_class: AppClass) void {
    if (instance_storage_stats.active_instances > 0) instance_storage_stats.active_instances -= 1 else instance_storage_stats.header_errors +%= 1;
    switch (app_class) {
        .service => if (instance_storage_stats.active_service_instances > 0) {
            instance_storage_stats.active_service_instances -= 1;
        } else {
            instance_storage_stats.header_errors +%= 1;
        },
        .console => if (instance_storage_stats.active_console_instances > 0) {
            instance_storage_stats.active_console_instances -= 1;
        } else {
            instance_storage_stats.header_errors +%= 1;
        },
        .gui => if (instance_storage_stats.active_gui_instances > 0) {
            instance_storage_stats.active_gui_instances -= 1;
        } else {
            instance_storage_stats.header_errors +%= 1;
        },
    }
    refreshInstanceByteTelemetry();
}

fn quarantinePayload(kind: ProgramPayloadKind, bytes: usize) void {
    const count = payloadCount(kind);
    if (count.* > 0) count.* -= 1 else instance_storage_stats.header_errors +%= 1;
    instance_storage_stats.quarantined_payloads +%= 1;
    instance_storage_stats.quarantined_bytes +%= @intCast(bytes);
    refreshInstanceByteTelemetry();
}

fn runtimePayload(instance: *ProgramInstance) *ProgramRuntimePayload {
    return instance.runtime_payload orelse unreachable;
}

fn runtimePayloadConst(instance: *const ProgramInstance) *const ProgramRuntimePayload {
    return instance.runtime_payload orelse unreachable;
}

fn processPayload(instance: *ProgramInstance) *ProgramProcessPayload {
    return instance.process_payload orelse unreachable;
}

fn processPayloadConst(instance: *const ProgramInstance) *const ProgramProcessPayload {
    return instance.process_payload orelse unreachable;
}

fn consolePayload(instance: *ProgramInstance) *ProgramConsolePayload {
    return instance.console_payload orelse unreachable;
}

fn consolePayloadConst(instance: *const ProgramInstance) *const ProgramConsolePayload {
    return instance.console_payload orelse unreachable;
}

fn guiPayload(instance: *ProgramInstance) *ProgramGuiPayload {
    return instance.gui_payload orelse unreachable;
}

fn guiPayloadConst(instance: *const ProgramInstance) *const ProgramGuiPayload {
    return instance.gui_payload orelse unreachable;
}

fn ensureGuiPayload(instance: *ProgramInstance) ?*ProgramGuiPayload {
    if (instance.gui_payload) |payload| return payload;
    const payload = allocateInstancePayload(ProgramGuiPayload, instance.id, .gui) orelse return null;
    instance.gui_payload = payload;
    return payload;
}

fn ensureEnvironmentPayload(instance: *ProgramInstance) ?*ProgramEnvironmentPayload {
    const process = processPayload(instance);
    if (process.environment_payload) |payload| return payload;
    const payload = allocateInstancePayload(ProgramEnvironmentPayload, instance.id, .environment) orelse return null;
    process.environment_payload = payload;
    return payload;
}

fn reserveGuiFrameGeneration() ?u64 {
    var current = @atomicLoad(u64, &gui_frame_generation_seed, .acquire);
    while (current != std.math.maxInt(u64)) {
        const next = current + 1;
        if (@cmpxchgWeak(u64, &gui_frame_generation_seed, current, next, .acq_rel, .acquire)) |actual| {
            current = actual;
        } else {
            return next;
        }
    }
    return null;
}

fn allocateGuiFramePayload(owner_id: u32, explicit_build: bool) ?*ProgramGuiFramePayload {
    const frame = allocateInstancePayload(ProgramGuiFramePayload, owner_id, .gui_frame) orelse return null;
    frame.explicit_build = explicit_build;
    return frame;
}

fn nextGuiCommandCapacity(frame: *const ProgramGuiFramePayload) u32 {
    const tail = frame.command_tail orelse return GUI_COMMAND_BLOCK_INITIAL_CAPACITY;
    if (tail.capacity >= GUI_COMMAND_BLOCK_TARGET_CAPACITY) return GUI_COMMAND_BLOCK_TARGET_CAPACITY;
    return @min(tail.capacity * 2, GUI_COMMAND_BLOCK_TARGET_CAPACITY);
}

fn guiCommandNeedsNewPayload(frame: *const ProgramGuiFramePayload) bool {
    const tail = frame.command_tail orelse return true;
    return tail.command_count == tail.capacity;
}

fn guiFrameCanLinkNodes(frame: *const ProgramGuiFramePayload, count: u64) bool {
    return count <= std.math.maxInt(u64) - frame.node_sequence;
}

fn nextGuiFrameNodeSequence(frame: *ProgramGuiFramePayload) u64 {
    std.debug.assert(guiFrameCanLinkNodes(frame, 1));
    frame.node_sequence += 1;
    return frame.node_sequence;
}

const GuiCommandStorage = struct {
    gui: *ProgramGuiPayload,
    frame: *ProgramGuiFramePayload,
    payload: *ProgramGuiCommandPayload,
    new_payload: bool,
};

fn prepareGuiCommandStorage(instance: *ProgramInstance, frame: *ProgramGuiFramePayload) ?GuiCommandStorage {
    const gui = instance.gui_payload orelse return null;
    if (frame.build_failed or frame.command_count == std.math.maxInt(u64)) return null;
    if (frame.command_tail) |tail| {
        if (tail.command_count < tail.capacity) return .{ .gui = gui, .frame = frame, .payload = tail, .new_payload = false };
    }
    if (!guiFrameCanLinkNodes(frame, 1)) return null;
    const payload = allocateGuiCommandPayload(instance.id, frame.command_count, nextGuiCommandCapacity(frame)) orelse return null;
    return .{ .gui = gui, .frame = frame, .payload = payload, .new_payload = true };
}

fn cancelGuiCommandStorage(instance: *ProgramInstance, storage: *GuiCommandStorage) void {
    if (storage.new_payload) _ = releaseGuiCommandPayload(storage.payload, instance.id);
}

fn commitGuiCommandStorage(storage: *GuiCommandStorage, command: ProgramGuiCommand) i32 {
    const frame = storage.frame;
    const payload = storage.payload;
    if (storage.new_payload) {
        payload.allocation_sequence = nextGuiFrameNodeSequence(frame);
        payload.previous = frame.command_tail;
        if (frame.command_tail) |tail| {
            tail.next = payload;
        } else {
            frame.command_payload = payload;
        }
        frame.command_tail = payload;
    }
    guiCommandPayloadCommands(payload)[payload.command_count] = command;
    payload.command_count += 1;
    frame.command_count += 1;
    noteGuiFrameCommandsAllocation(1);
    refreshGuiFrameOwnerPeak(storage.gui);
    return @intCast(@min(frame.command_count, @as(u64, std.math.maxInt(i32))));
}

const GuiResourceStorage = struct {
    gui: *ProgramGuiPayload,
    frame: *ProgramGuiFramePayload,
    payload: *ProgramGuiResourcePayload,
    next_resource_len: u64,
    next_raster_words: u64,
};

fn prepareGuiResourceStorage(
    instance: *ProgramInstance,
    frame: *ProgramGuiFramePayload,
    byte_count: usize,
    resource_kind: ProgramGuiCommandResourceKind,
) ?GuiResourceStorage {
    const gui = instance.gui_payload orelse return null;
    if (frame.build_failed or byte_count == 0 or !guiFrameCanLinkNodes(frame, 1)) return null;
    const next_resource_len = std.math.add(u64, frame.resource_len, byte_count) catch return null;
    var next_raster_words = frame.raster_words;
    if (resource_kind == .xrgb32 or resource_kind == .alpha8) {
        if (byte_count % @sizeOf(u32) != 0) return null;
        next_raster_words = std.math.add(u64, frame.raster_words, byte_count / @sizeOf(u32)) catch return null;
    }
    const payload = allocateGuiResourcePayload(
        instance.id,
        frame.resource_len,
        if (resource_kind == .xrgb32 or resource_kind == .alpha8) frame.raster_words else 0,
        byte_count,
        resource_kind,
    ) orelse return null;
    return .{
        .gui = gui,
        .frame = frame,
        .payload = payload,
        .next_resource_len = next_resource_len,
        .next_raster_words = next_raster_words,
    };
}

fn cancelGuiResourceStorage(instance: *ProgramInstance, storage: *GuiResourceStorage) void {
    _ = releaseGuiResourcePayload(storage.payload, instance.id);
}

fn commitGuiResourceStorage(storage: *GuiResourceStorage) void {
    const frame = storage.frame;
    const payload = storage.payload;
    payload.allocation_sequence = nextGuiFrameNodeSequence(frame);
    payload.previous = frame.resource_tail;
    if (frame.resource_tail) |tail| {
        tail.next = payload;
    } else {
        frame.resource_payload = payload;
    }
    frame.resource_tail = payload;
    frame.resource_len = storage.next_resource_len;
    frame.raster_words = storage.next_raster_words;
    refreshGuiFrameOwnerPeak(storage.gui);
}

const GuiBlitStorage = struct {
    command: GuiCommandStorage,
    resource: GuiResourceStorage,
};

fn prepareGuiBlitStorage(
    instance: *ProgramInstance,
    frame: *ProgramGuiFramePayload,
    word_count: usize,
    resource_kind: ProgramGuiCommandResourceKind,
) ?GuiBlitStorage {
    if (word_count == 0 or word_count > GUI_RASTER_NODE_MAX_WORDS) return null;
    const node_count: u64 = 1 + @as(u64, @intFromBool(guiCommandNeedsNewPayload(frame)));
    if (!guiFrameCanLinkNodes(frame, node_count)) return null;
    var command = prepareGuiCommandStorage(instance, frame) orelse return null;
    const byte_count = std.math.mul(usize, word_count, @sizeOf(u32)) catch {
        cancelGuiCommandStorage(instance, &command);
        return null;
    };
    const resource = prepareGuiResourceStorage(instance, frame, byte_count, resource_kind) orelse {
        cancelGuiCommandStorage(instance, &command);
        instance_storage_stats.transaction_rollbacks +%= 1;
        return null;
    };
    return .{ .command = command, .resource = resource };
}

fn cancelGuiBlitStorage(instance: *ProgramInstance, storage: *GuiBlitStorage) void {
    cancelGuiResourceStorage(instance, &storage.resource);
    cancelGuiCommandStorage(instance, &storage.command);
    instance_storage_stats.transaction_rollbacks +%= 1;
}

fn commitGuiBlitStorage(storage: *GuiBlitStorage, command: ProgramGuiCommand) i32 {
    // All allocation and payload copying completed before this yield-free
    // mutation.  Link commands before resources so the common per-frame node
    // sequence mirrors ownership acquisition; reverse teardown then removes
    // the resource node before a newly grown command block.
    const result = commitGuiCommandStorage(&storage.command, command);
    commitGuiResourceStorage(&storage.resource);
    return result;
}

const GuiFrameCapture = struct {
    gui: *ProgramGuiPayload,
    frame: *ProgramGuiFramePayload,
    generation: u64,
};

// Frame state is protected by a no-sleep mutex.  Kernel instruction pointers
// are not timer-preempted today, so contention here can only be transient
// future-SMP contention.  Spin without yielding: a captured frame reference
// must never cross a scheduler wait before it is released.
fn lockGuiFrameState(gui: *ProgramGuiPayload) void {
    while (!gui.frame_lock.tryLock()) asm volatile ("pause");
}

fn captureCommittedGuiFrame(instance: *ProgramInstance, expected_generation: ?u64) ?GuiFrameCapture {
    const gui = instance.gui_payload orelse return null;
    lockGuiFrameState(gui);
    defer _ = gui.frame_lock.unlock();
    const frame = gui.committed_frame orelse return null;
    if (expected_generation) |expected| if (frame.generation != expected) return null;
    if (frame.reader_refs == std.math.maxInt(u32)) return null;
    frame.reader_refs += 1;
    return .{ .gui = gui, .frame = frame, .generation = frame.generation };
}

fn captureGuiFrameGeneration(instance: *ProgramInstance, generation: u64) ?GuiFrameCapture {
    if (generation == 0) return null;
    const gui = instance.gui_payload orelse return null;
    lockGuiFrameState(gui);
    defer _ = gui.frame_lock.unlock();
    var cursor = gui.committed_frame;
    while (cursor) |frame| : (cursor = frame.base_frame) {
        if (frame.generation != generation) continue;
        if (frame.reader_refs == std.math.maxInt(u32)) return null;
        frame.reader_refs += 1;
        return .{ .gui = gui, .frame = frame, .generation = generation };
    }
    return null;
}

fn removeRetiredGuiFrameLocked(gui: *ProgramGuiPayload, frame: *ProgramGuiFramePayload) bool {
    var previous: ?*ProgramGuiFramePayload = null;
    var cursor = gui.retired_frames;
    while (cursor) |candidate| {
        if (candidate == frame) {
            if (previous) |before| {
                before.retired_next = candidate.retired_next;
            } else {
                gui.retired_frames = candidate.retired_next;
            }
            candidate.retired_next = null;
            candidate.retired = false;
            return true;
        }
        previous = candidate;
        cursor = candidate.retired_next;
    }
    return false;
}

fn guiFrameChainContains(root: *const ProgramGuiFramePayload, wanted: *const ProgramGuiFramePayload) bool {
    var cursor: ?*const ProgramGuiFramePayload = root;
    while (cursor) |frame| : (cursor = frame.base_frame) if (frame == wanted) return true;
    return false;
}

fn removeReleasableRetiredGuiChainLocked(gui: *ProgramGuiPayload, changed: *const ProgramGuiFramePayload) ?*ProgramGuiFramePayload {
    var previous: ?*ProgramGuiFramePayload = null;
    var cursor = gui.retired_frames;
    while (cursor) |root| {
        if (guiFrameChainContains(root, changed) and !guiFrameChainHasReaders(root)) {
            if (previous) |before| {
                before.retired_next = root.retired_next;
            } else {
                gui.retired_frames = root.retired_next;
            }
            root.retired_next = null;
            root.retired = false;
            return root;
        }
        previous = root;
        cursor = root.retired_next;
    }
    return null;
}

fn releaseCapturedGuiFrame(instance: *ProgramInstance, capture: *GuiFrameCapture) void {
    var release_frame: ?*ProgramGuiFramePayload = null;
    const gui = capture.gui;
    lockGuiFrameState(gui);
    if (capture.frame.reader_refs == 0) {
        instance_storage_stats.header_errors +%= 1;
    } else {
        capture.frame.reader_refs -= 1;
        if (capture.frame.reader_refs == 0) release_frame = removeReleasableRetiredGuiChainLocked(gui, capture.frame);
    }
    _ = gui.frame_lock.unlock();
    if (release_frame) |frame| _ = releaseGuiFrame(frame, instance.id);
}

fn linkClonedGuiCommand(frame: *ProgramGuiFramePayload, payload: *ProgramGuiCommandPayload) void {
    payload.allocation_sequence = nextGuiFrameNodeSequence(frame);
    payload.previous = frame.command_tail;
    if (frame.command_tail) |tail| {
        tail.next = payload;
    } else {
        frame.command_payload = payload;
    }
    frame.command_tail = payload;
    frame.command_count += payload.command_count;
    noteGuiFrameCommandsAllocation(payload.command_count);
}

fn linkClonedGuiResource(frame: *ProgramGuiFramePayload, payload: *ProgramGuiResourcePayload) void {
    payload.allocation_sequence = nextGuiFrameNodeSequence(frame);
    payload.previous = frame.resource_tail;
    if (frame.resource_tail) |tail| {
        tail.next = payload;
    } else {
        frame.resource_payload = payload;
    }
    frame.resource_tail = payload;
    frame.resource_len += payload.byte_count;
    if (payload.resource_kind == .xrgb32 or payload.resource_kind == .alpha8) {
        frame.raster_words += payload.byte_count / @sizeOf(u32);
    }
}

fn cloneGuiFrame(instance: *ProgramInstance, source: *const ProgramGuiFramePayload) ?*ProgramGuiFramePayload {
    const clone = allocateGuiFramePayload(instance.id, false) orelse return null;
    var chain: [r4x_api.gui_frame_max_delta_chain]*const ProgramGuiFramePayload = undefined;
    var chain_count: usize = 0;
    var chain_cursor: ?*const ProgramGuiFramePayload = source;
    while (chain_cursor) |frame| : (chain_cursor = frame.base_frame) {
        if (chain_count >= chain.len) {
            _ = releaseGuiFrame(clone, instance.id);
            return null;
        }
        chain[chain_count] = frame;
        chain_count += 1;
    }

    while (chain_count != 0) {
        chain_count -= 1;
        const source_frame = chain[chain_count];
        const resource_base = clone.resource_len;
        const raster_base = clone.raster_words;
        var command_cursor = source_frame.command_payload;
        while (command_cursor) |source_payload| : (command_cursor = source_payload.next) {
            if (!guiFrameCanLinkNodes(clone, 1)) {
                _ = releaseGuiFrame(clone, instance.id);
                return null;
            }
            const payload = allocateGuiCommandPayload(instance.id, clone.command_count, source_payload.command_count) orelse {
                _ = releaseGuiFrame(clone, instance.id);
                return null;
            };
            payload.command_count = source_payload.command_count;
            @memcpy(
                guiCommandPayloadCommands(payload)[0..payload.command_count],
                guiCommandPayloadCommandsConst(source_payload)[0..source_payload.command_count],
            );
            for (guiCommandPayloadCommands(payload)[0..payload.command_count]) |*command| {
                if (command.payload_bytes != 0) command.payload_offset += resource_base;
                if (command.resource_kind == .xrgb32 or command.resource_kind == .alpha8) command.raster_word_offset += raster_base;
            }
            linkClonedGuiCommand(clone, payload);
        }
        var resource_cursor = source_frame.resource_payload;
        while (resource_cursor) |source_payload| : (resource_cursor = source_payload.next) {
            if (!guiFrameCanLinkNodes(clone, 1)) {
                _ = releaseGuiFrame(clone, instance.id);
                return null;
            }
            const payload = allocateGuiResourcePayload(
                instance.id,
                clone.resource_len,
                source_payload.raster_word_offset + raster_base,
                source_payload.byte_count,
                source_payload.resource_kind,
            ) orelse {
                _ = releaseGuiFrame(clone, instance.id);
                return null;
            };
            @memcpy(guiResourcePayloadData(payload), guiResourcePayloadDataConst(source_payload));
            linkClonedGuiResource(clone, payload);
        }
        // Batch APPEND stores XRGB/Alpha resources in generic byte nodes. The
        // command semantics remain authoritative for the flattened legacy
        // raster stream, so advance by the generation-local aggregate.
        clone.raster_words = raster_base + source_frame.raster_words;
    }
    return clone;
}

fn batchCommandResourceKind(command: *const GuiFrameCommand) ?ProgramGuiCommandResourceKind {
    return switch (command.kind) {
        r4x_api.gui_frame_command_kind_clear, r4x_api.gui_frame_command_kind_rect => .none,
        r4x_api.gui_frame_command_kind_text => if (command.resource_bytes == 0) .none else .utf8,
        r4x_api.gui_frame_command_kind_raster => .xrgb32,
        r4x_api.gui_frame_command_kind_alpha8 => .alpha8,
        r4x_api.gui_frame_command_kind_path_fill,
        r4x_api.gui_frame_command_kind_path_stroke,
        r4x_api.gui_frame_command_kind_rounded_rect,
        r4x_api.gui_frame_command_kind_shadow,
        r4x_api.gui_frame_command_kind_argb32,
        => .path,
        r4x_api.gui_frame_command_kind_indexed8 => .indexed8,
        else => null,
    };
}

fn batchCommandResourceSlice(command: *const GuiFrameCommand, resources: []const u8) ?[]const u8 {
    if (command.resource_bytes == 0) return &.{};
    const end = std.math.add(u64, command.resource_offset, command.resource_bytes) catch return null;
    if (end > resources.len) return null;
    return resources[@intCast(command.resource_offset)..@intCast(end)];
}

fn guiShapeReadU32(bytes: []const u8, offset: usize) ?u32 {
    const end = std.math.add(usize, offset, @sizeOf(u32)) catch return null;
    if (end > bytes.len) return null;
    return @as(u32, bytes[offset]) |
        (@as(u32, bytes[offset + 1]) << 8) |
        (@as(u32, bytes[offset + 2]) << 16) |
        (@as(u32, bytes[offset + 3]) << 24);
}

fn guiShapeField(bytes: []const u8, comptime name: []const u8) ?u32 {
    return guiShapeReadU32(bytes, @offsetOf(GuiShapeResource, name));
}

fn validateGuiIndexed8Resource(command: *const GuiFrameCommand, resource: []const u8) bool {
    if (command.w == 0 or command.h == 0 or command.w > GUI_RASTER_MAX_WIDTH or command.h > GUI_RASTER_MAX_HEIGHT or
        command.rgb != 0 or command.fg != 0 or command.bg != 0 or command.font_id != 0 or
        command.text_w != 0 or command.text_h != 0 or command.baseline != 0 or command.line_height != 0 or
        command.parameter0 != 0 or command.parameter1 != 0) return false;
    if (resource.len < r4x_api.gui_indexed8_pixels_offset) return false;
    if (guiShapeReadU32(resource, @offsetOf(GuiIndexed8Resource, "version")) != r4x_api.gui_indexed8_resource_version or
        guiShapeReadU32(resource, @offsetOf(GuiIndexed8Resource, "size")) != r4x_api.gui_indexed8_resource_size) return false;

    const source_x = guiShapeReadU32(resource, @offsetOf(GuiIndexed8Resource, "source_x")) orelse return false;
    const source_y = guiShapeReadU32(resource, @offsetOf(GuiIndexed8Resource, "source_y")) orelse return false;
    const source_w = guiShapeReadU32(resource, @offsetOf(GuiIndexed8Resource, "source_w")) orelse return false;
    const source_h = guiShapeReadU32(resource, @offsetOf(GuiIndexed8Resource, "source_h")) orelse return false;
    const guest_w = guiShapeReadU32(resource, @offsetOf(GuiIndexed8Resource, "guest_w")) orelse return false;
    const guest_h = guiShapeReadU32(resource, @offsetOf(GuiIndexed8Resource, "guest_h")) orelse return false;
    const viewport_x: i32 = @bitCast(guiShapeReadU32(resource, @offsetOf(GuiIndexed8Resource, "viewport_x")) orelse return false);
    const viewport_y: i32 = @bitCast(guiShapeReadU32(resource, @offsetOf(GuiIndexed8Resource, "viewport_y")) orelse return false);
    const viewport_w = guiShapeReadU32(resource, @offsetOf(GuiIndexed8Resource, "viewport_w")) orelse return false;
    const viewport_h = guiShapeReadU32(resource, @offsetOf(GuiIndexed8Resource, "viewport_h")) orelse return false;
    const palette_entries = guiShapeReadU32(resource, @offsetOf(GuiIndexed8Resource, "palette_entries")) orelse return false;
    const palette_offset = guiShapeReadU32(resource, @offsetOf(GuiIndexed8Resource, "palette_offset")) orelse return false;
    const pixels_offset = guiShapeReadU32(resource, @offsetOf(GuiIndexed8Resource, "pixels_offset")) orelse return false;
    const pixel_stride = guiShapeReadU32(resource, @offsetOf(GuiIndexed8Resource, "pixel_stride")) orelse return false;
    if (source_w == 0 or source_h == 0 or source_w > GUI_RASTER_MAX_WIDTH or source_h > GUI_RASTER_MAX_HEIGHT or
        guest_w == 0 or guest_h == 0 or viewport_w == 0 or viewport_h == 0 or
        palette_entries != r4x_api.gui_indexed8_palette_entries or palette_offset != r4x_api.gui_indexed8_palette_offset or
        pixels_offset != r4x_api.gui_indexed8_pixels_offset or pixel_stride != source_w) return false;
    const source_right = std.math.add(u64, source_x, source_w) catch return false;
    const source_bottom = std.math.add(u64, source_y, source_h) catch return false;
    if (source_right > guest_w or source_bottom > guest_h) return false;
    const pixels = std.math.mul(u64, source_w, source_h) catch return false;
    const required = std.math.add(u64, pixels_offset, pixels) catch return false;
    if (required != resource.len or command.resource_bytes != required) return false;

    const command_left: i64 = command.x;
    const command_top: i64 = command.y;
    const command_right = std.math.add(i64, command_left, command.w) catch return false;
    const command_bottom = std.math.add(i64, command_top, command.h) catch return false;
    const viewport_left: i64 = viewport_x;
    const viewport_top: i64 = viewport_y;
    const viewport_right = std.math.add(i64, viewport_left, viewport_w) catch return false;
    const viewport_bottom = std.math.add(i64, viewport_top, viewport_h) catch return false;
    if (command_left < viewport_left or command_top < viewport_top or command_right > viewport_right or command_bottom > viewport_bottom) return false;
    const local_left: u64 = @intCast(command_left - viewport_left);
    const local_top: u64 = @intCast(command_top - viewport_top);
    const local_right: u64 = @intCast(command_right - viewport_left);
    const local_bottom: u64 = @intCast(command_bottom - viewport_top);
    const mapped_left = (local_left * guest_w) / viewport_w;
    const mapped_top = (local_top * guest_h) / viewport_h;
    const mapped_right = ((local_right - 1) * guest_w) / viewport_w;
    const mapped_bottom = ((local_bottom - 1) * guest_h) / viewport_h;
    if (mapped_left < source_x or mapped_top < source_y or mapped_right >= source_right or mapped_bottom >= source_bottom) return false;

    var palette_byte: usize = @intCast(palette_offset + 3);
    const palette_end: usize = @intCast(pixels_offset);
    while (palette_byte < palette_end) : (palette_byte += @sizeOf(u32)) if (resource[palette_byte] != 0) return false;
    return true;
}

fn guiShapeFloat(bits: u32) ?f32 {
    const value: f32 = @bitCast(bits);
    if (!std.math.isFinite(value)) return null;
    return value;
}

fn validGuiShapeCoordinate(bits: u32) bool {
    const value = guiShapeFloat(bits) orelse return false;
    return @abs(value) <= @as(f32, @floatFromInt(r4x_api.gui_shape_max_coordinate));
}

fn validGuiShapeNonNegative(bits: u32, maximum: f32) bool {
    const value = guiShapeFloat(bits) orelse return false;
    return value >= 0 and value <= maximum;
}

fn validGuiPathSegments(resource: []const u8, segment_count: u32) bool {
    var active = false;
    var index: u32 = 0;
    while (index < segment_count) : (index += 1) {
        const relative = std.math.mul(usize, @as(usize, index), @sizeOf(GuiPathSegment)) catch return false;
        const offset = std.math.add(usize, @sizeOf(GuiShapeResource), relative) catch return false;
        const kind = guiShapeReadU32(resource, offset + @offsetOf(GuiPathSegment, "kind")) orelse return false;
        const flags = guiShapeReadU32(resource, offset + @offsetOf(GuiPathSegment, "flags")) orelse return false;
        if (flags != 0) return false;
        const x1 = guiShapeReadU32(resource, offset + @offsetOf(GuiPathSegment, "x1_bits")) orelse return false;
        const y1 = guiShapeReadU32(resource, offset + @offsetOf(GuiPathSegment, "y1_bits")) orelse return false;
        const x2 = guiShapeReadU32(resource, offset + @offsetOf(GuiPathSegment, "x2_bits")) orelse return false;
        const y2 = guiShapeReadU32(resource, offset + @offsetOf(GuiPathSegment, "y2_bits")) orelse return false;
        const x3 = guiShapeReadU32(resource, offset + @offsetOf(GuiPathSegment, "x3_bits")) orelse return false;
        const y3 = guiShapeReadU32(resource, offset + @offsetOf(GuiPathSegment, "y3_bits")) orelse return false;
        switch (kind) {
            r4x_api.gui_path_segment_kind_move => {
                if (!validGuiShapeCoordinate(x1) or !validGuiShapeCoordinate(y1) or x2 != 0 or y2 != 0 or x3 != 0 or y3 != 0) return false;
                active = true;
            },
            r4x_api.gui_path_segment_kind_line => {
                if (!active or !validGuiShapeCoordinate(x1) or !validGuiShapeCoordinate(y1) or x2 != 0 or y2 != 0 or x3 != 0 or y3 != 0) return false;
            },
            r4x_api.gui_path_segment_kind_quadratic => {
                if (!active or !validGuiShapeCoordinate(x1) or !validGuiShapeCoordinate(y1) or
                    !validGuiShapeCoordinate(x2) or !validGuiShapeCoordinate(y2) or x3 != 0 or y3 != 0) return false;
            },
            r4x_api.gui_path_segment_kind_cubic => {
                if (!active or !validGuiShapeCoordinate(x1) or !validGuiShapeCoordinate(y1) or
                    !validGuiShapeCoordinate(x2) or !validGuiShapeCoordinate(y2) or
                    !validGuiShapeCoordinate(x3) or !validGuiShapeCoordinate(y3)) return false;
            },
            r4x_api.gui_path_segment_kind_close => {
                if (!active or x1 != 0 or y1 != 0 or x2 != 0 or y2 != 0 or x3 != 0 or y3 != 0) return false;
                active = false;
            },
            else => return false,
        }
    }
    return true;
}

fn validateGuiShapeResource(command: *const GuiFrameCommand, resource: []const u8) bool {
    if (command.w == 0 or command.h == 0 or command.w > r4x_api.gui_shape_max_dimension or command.h > r4x_api.gui_shape_max_dimension) return false;
    const pixel_count = std.math.mul(u64, command.w, command.h) catch return false;
    if (pixel_count > r4x_api.gui_shape_max_pixels or command.parameter0 != 0 or command.parameter1 != 0) return false;
    if (command.rgb != 0 or command.fg != 0 or command.bg != 0 or command.font_id != 0 or
        command.text_w != 0 or command.text_h != 0 or command.baseline != 0 or command.line_height != 0) return false;
    if (resource.len < @sizeOf(GuiShapeResource) or command.resource_bytes != resource.len) return false;
    if (guiShapeField(resource, "version") != r4x_api.gui_shape_resource_version or
        guiShapeField(resource, "size") != r4x_api.gui_shape_resource_size) return false;
    const geometry_kind = guiShapeField(resource, "geometry_kind") orelse return false;
    const flags = guiShapeField(resource, "flags") orelse return false;
    if ((flags & ~r4x_api.gui_shape_flag_shadow_inset) != 0) return false;
    const segment_count = guiShapeField(resource, "segment_count") orelse return false;
    if (segment_count > r4x_api.gui_shape_max_segments) return false;
    const segment_bytes = std.math.mul(u64, segment_count, r4x_api.gui_path_segment_size) catch return false;
    const expected_bytes = std.math.add(u64, r4x_api.gui_shape_resource_size, segment_bytes) catch return false;
    if (expected_bytes != resource.len) return false;
    const fill_rule = guiShapeField(resource, "fill_rule") orelse return false;
    if (fill_rule != r4x_api.gui_shape_fill_rule_nonzero and fill_rule != r4x_api.gui_shape_fill_rule_evenodd) return false;
    const line_join = guiShapeField(resource, "line_join") orelse return false;
    const line_cap = guiShapeField(resource, "line_cap") orelse return false;
    if (line_join > r4x_api.gui_shape_line_join_bevel or line_cap > r4x_api.gui_shape_line_cap_square) return false;
    const clip_w = guiShapeField(resource, "clip_w") orelse return false;
    const clip_h = guiShapeField(resource, "clip_h") orelse return false;
    if ((clip_w == 0) != (clip_h == 0) or clip_w > r4x_api.gui_shape_max_dimension or clip_h > r4x_api.gui_shape_max_dimension) return false;
    if (guiShapeField(resource, "reserved0") != 0 or guiShapeField(resource, "reserved1") != 0 or guiShapeField(resource, "reserved2") != 0) return false;

    const coordinate_max: f32 = @floatFromInt(r4x_api.gui_shape_max_coordinate);
    const stroke_width_bits = guiShapeField(resource, "stroke_width_bits") orelse return false;
    const miter_limit_bits = guiShapeField(resource, "miter_limit_bits") orelse return false;
    if (!validGuiShapeNonNegative(stroke_width_bits, coordinate_max) or !validGuiShapeNonNegative(miter_limit_bits, coordinate_max)) return false;
    const miter_limit = guiShapeFloat(miter_limit_bits) orelse return false;
    if (line_join == r4x_api.gui_shape_line_join_miter and miter_limit < 1) return false;
    if (!validGuiShapeCoordinate(guiShapeField(resource, "shadow_offset_x_bits") orelse return false) or
        !validGuiShapeCoordinate(guiShapeField(resource, "shadow_offset_y_bits") orelse return false) or
        !validGuiShapeCoordinate(guiShapeField(resource, "shadow_spread_bits") orelse return false) or
        !validGuiShapeNonNegative(guiShapeField(resource, "shadow_blur_bits") orelse return false, @floatFromInt(r4x_api.gui_shape_max_blur_radius))) return false;

    switch (geometry_kind) {
        r4x_api.gui_shape_geometry_kind_path => {
            if (segment_count == 0) return false;
            var offset: usize = @offsetOf(GuiShapeResource, "geometry_x_bits");
            while (offset <= @offsetOf(GuiShapeResource, "border_left_bits")) : (offset += @sizeOf(u32)) {
                if (guiShapeReadU32(resource, offset) != 0) return false;
            }
            if (!validGuiPathSegments(resource, segment_count)) return false;
        },
        r4x_api.gui_shape_geometry_kind_rounded_rect => {
            if (segment_count != 0 or line_join != 0 or line_cap != 0 or stroke_width_bits != 0 or miter_limit_bits != 0) return false;
            if (!validGuiShapeCoordinate(guiShapeField(resource, "geometry_x_bits") orelse return false) or
                !validGuiShapeCoordinate(guiShapeField(resource, "geometry_y_bits") orelse return false)) return false;
            const geometry_w = guiShapeFloat(guiShapeField(resource, "geometry_w_bits") orelse return false) orelse return false;
            const geometry_h = guiShapeFloat(guiShapeField(resource, "geometry_h_bits") orelse return false) orelse return false;
            if (geometry_w <= 0 or geometry_h <= 0 or geometry_w > coordinate_max or geometry_h > coordinate_max) return false;
            var offset: usize = @offsetOf(GuiShapeResource, "radius_top_left_x_bits");
            while (offset <= @offsetOf(GuiShapeResource, "border_left_bits")) : (offset += @sizeOf(u32)) {
                if (!validGuiShapeNonNegative(guiShapeReadU32(resource, offset) orelse return false, coordinate_max)) return false;
            }
        },
        else => return false,
    }

    return switch (command.kind) {
        r4x_api.gui_frame_command_kind_path_fill => geometry_kind == r4x_api.gui_shape_geometry_kind_path,
        r4x_api.gui_frame_command_kind_path_stroke => geometry_kind == r4x_api.gui_shape_geometry_kind_path and
            (guiShapeFloat(stroke_width_bits) orelse 0) > 0 and line_join != 0 and line_cap != 0,
        r4x_api.gui_frame_command_kind_rounded_rect => geometry_kind == r4x_api.gui_shape_geometry_kind_rounded_rect,
        r4x_api.gui_frame_command_kind_shadow => true,
        else => false,
    };
}

fn validateGuiFrameBatchCommand(command: *const GuiFrameCommand, resources: []const u8) bool {
    if (command.version != r4x_api.gui_frame_command_version or command.size != r4x_api.gui_frame_command_size or command.flags != 0) return false;
    const resource_kind = batchCommandResourceKind(command) orelse return false;
    const resource = batchCommandResourceSlice(command, resources) orelse return false;
    switch (command.kind) {
        r4x_api.gui_frame_command_kind_clear, r4x_api.gui_frame_command_kind_rect => {
            return resource_kind == .none and command.resource_offset == 0 and command.resource_bytes == 0 and
                command.parameter0 == 0 and command.parameter1 == 0;
        },
        r4x_api.gui_frame_command_kind_text => {
            if (command.parameter0 != 0 or command.parameter1 != 0) return false;
            if (resource.len == 0) return command.resource_offset == 0;
            return std.unicode.utf8ValidateSlice(resource);
        },
        r4x_api.gui_frame_command_kind_raster => {
            if (command.w == 0 or command.h == 0 or command.w > GUI_RASTER_MAX_WIDTH or command.h > GUI_RASTER_MAX_HEIGHT or
                command.parameter0 < 1 or command.parameter0 > 16 or command.parameter1 != 0) return false;
            const pixels = std.math.mul(u64, command.w, command.h) catch return false;
            if (pixels > GUI_RASTER_MAX_PIXELS) return false;
            const bytes = std.math.mul(u64, pixels, 4) catch return false;
            if (command.resource_bytes != bytes) return false;
            var offset: usize = 3;
            while (offset < resource.len) : (offset += 4) if (resource[offset] != 0) return false;
            return true;
        },
        r4x_api.gui_frame_command_kind_alpha8 => {
            if (command.w == 0 or command.h == 0 or command.w > GUI_ALPHA8_MAX_WIDTH or command.h > GUI_ALPHA8_MAX_HEIGHT or
                command.parameter0 != 0 or command.parameter1 != 0) return false;
            const bytes = std.math.mul(u64, command.w, command.h) catch return false;
            if (bytes > GUI_ALPHA8_MAX_PIXELS) return false;
            return command.resource_bytes == bytes;
        },
        r4x_api.gui_frame_command_kind_argb32 => {
            if (resource_kind != .path or command.w == 0 or command.h == 0 or
                command.w > GUI_ARGB32_MAX_WIDTH or command.h > GUI_ARGB32_MAX_HEIGHT or
                command.parameter0 < 1 or command.parameter0 > 16 or command.parameter1 != 0) return false;
            if (command.rgb != 0 or command.fg != 0 or command.bg != 0 or command.font_id != 0 or
                command.text_w != 0 or command.text_h != 0 or command.baseline != 0 or command.line_height != 0) return false;
            const pixels = std.math.mul(u64, command.w, command.h) catch return false;
            if (pixels > GUI_ARGB32_MAX_PIXELS) return false;
            const bytes = std.math.mul(u64, pixels, @sizeOf(u32)) catch return false;
            return command.resource_bytes == bytes;
        },
        r4x_api.gui_frame_command_kind_indexed8 => {
            return resource_kind == .indexed8 and validateGuiIndexed8Resource(command, resource);
        },
        r4x_api.gui_frame_command_kind_path_fill,
        r4x_api.gui_frame_command_kind_path_stroke,
        r4x_api.gui_frame_command_kind_rounded_rect,
        r4x_api.gui_frame_command_kind_shadow,
        => return resource_kind == .path and validateGuiShapeResource(command, resource),
        else => return false,
    }
}

fn internalGuiFrameCommand(command: *const GuiFrameCommand, resource_base: u64, raster_word_offset: u64) ProgramGuiCommand {
    const resource_kind = batchCommandResourceKind(command) orelse .none;
    return .{
        .version = command.version,
        .size = command.size,
        .kind = command.kind,
        .flags = command.flags,
        .x = command.x,
        .y = command.y,
        .w = command.w,
        .h = command.h,
        .rgb = command.rgb,
        .fg = command.fg,
        .bg = command.bg,
        .font_id = command.font_id,
        .text_w = command.text_w,
        .text_h = command.text_h,
        .baseline = command.baseline,
        .line_height = command.line_height,
        .payload_offset = if (command.resource_bytes == 0) 0 else resource_base + command.resource_offset,
        .payload_bytes = command.resource_bytes,
        .parameter0 = command.parameter0,
        .parameter1 = command.parameter1,
        .resource_kind = resource_kind,
        .raster_word_offset = if (resource_kind == .xrgb32 or resource_kind == .alpha8) raster_word_offset else 0,
    };
}

fn appendBatchResourceBlob(instance: *ProgramInstance, frame: *ProgramGuiFramePayload, resources: []const u8) bool {
    var copied: usize = 0;
    while (copied < resources.len) {
        const count = @min(resources.len - copied, GUI_RESOURCE_BLOCK_TARGET_BYTES);
        var storage = prepareGuiResourceStorage(instance, frame, count, .path) orelse return false;
        @memcpy(guiResourcePayloadData(storage.payload), resources[copied .. copied + count]);
        commitGuiResourceStorage(&storage);
        copied += count;
    }
    return true;
}

fn spliceGuiFrameDelta(gui: *ProgramGuiPayload, building: *ProgramGuiFramePayload, delta: *ProgramGuiFramePayload) void {
    const command_base = building.command_count;
    const resource_base = building.resource_len;
    const raster_base = building.raster_words;
    const node_sequence_base = building.node_sequence;

    var command_cursor = delta.command_payload;
    while (command_cursor) |payload| : (command_cursor = payload.next) {
        payload.allocation_sequence += node_sequence_base;
        payload.logical_offset += command_base;
        const commands = guiCommandPayloadCommands(payload);
        for (commands[0..payload.command_count]) |*command| {
            if (command.payload_bytes != 0) command.payload_offset += resource_base;
            if (command.resource_kind == .xrgb32 or command.resource_kind == .alpha8) command.raster_word_offset += raster_base;
        }
    }
    var resource_cursor = delta.resource_payload;
    while (resource_cursor) |payload| : (resource_cursor = payload.next) {
        payload.allocation_sequence += node_sequence_base;
        payload.logical_offset += resource_base;
    }

    if (delta.command_payload) |head| {
        head.previous = building.command_tail;
        if (building.command_tail) |tail| tail.next = head else building.command_payload = head;
        building.command_tail = delta.command_tail;
    }
    if (delta.resource_payload) |head| {
        head.previous = building.resource_tail;
        if (building.resource_tail) |tail| tail.next = head else building.resource_payload = head;
        building.resource_tail = delta.resource_tail;
    }
    building.command_count += delta.command_count;
    building.resource_len += delta.resource_len;
    building.raster_words += delta.raster_words;
    building.node_sequence += delta.node_sequence;

    delta.command_payload = null;
    delta.command_tail = null;
    delta.resource_payload = null;
    delta.resource_tail = null;
    delta.command_count = 0;
    delta.resource_len = 0;
    delta.raster_words = 0;
    delta.node_sequence = 0;
    refreshGuiFrameOwnerPeak(gui);
}

fn guiFrameAppendBatch(instance: *ProgramInstance, commands: []const GuiFrameCommand, resources: []const u8) i32 {
    const gui = instance.gui_payload orelse return r4x_api.gui_frame_error_unavailable;
    const building = gui.building_frame orelse return setGuiFrameResult(gui, r4x_api.gui_frame_error_state);
    if (!building.explicit_build or building.build_failed) return setGuiFrameResult(gui, r4x_api.gui_frame_error_state);

    for (commands) |*command| {
        if (!validateGuiFrameBatchCommand(command, resources)) {
            building.build_failed = true;
            return setGuiFrameResult(gui, r4x_api.gui_frame_error_invalid);
        }
    }
    if (commands.len == 0 and resources.len == 0) return setGuiFrameResult(gui, r4x_api.gui_frame_result_ok);

    const delta = allocateGuiFramePayload(instance.id, true) orelse {
        guiFrameMarkBuildFailed(gui, building);
        return r4x_api.gui_frame_error_oom;
    };
    defer _ = releaseGuiFrame(delta, instance.id);

    var raster_words: u64 = 0;
    for (commands) |*command| {
        var storage = prepareGuiCommandStorage(instance, delta) orelse {
            guiFrameMarkBuildFailed(gui, building);
            return r4x_api.gui_frame_error_oom;
        };
        const internal = internalGuiFrameCommand(command, 0, raster_words);
        _ = commitGuiCommandStorage(&storage, internal);
        if (internal.resource_kind == .xrgb32) {
            raster_words = std.math.add(u64, raster_words, internal.payload_bytes / 4) catch {
                building.build_failed = true;
                return setGuiFrameResult(gui, r4x_api.gui_frame_error_overflow);
            };
        } else if (internal.resource_kind == .alpha8) {
            const words = (std.math.add(u64, internal.payload_bytes, 3) catch {
                building.build_failed = true;
                return setGuiFrameResult(gui, r4x_api.gui_frame_error_overflow);
            }) / 4;
            raster_words = std.math.add(u64, raster_words, words) catch {
                building.build_failed = true;
                return setGuiFrameResult(gui, r4x_api.gui_frame_error_overflow);
            };
        }
    }
    delta.raster_words = raster_words;
    // This private batch links its command nodes before resource nodes.  The
    // common frame-local sequence records that order alongside any earlier
    // interleaved appends; releaseGuiFrame merges both typed tails by descending
    // sequence and releases the frame header last, including partial OOM.
    if (!appendBatchResourceBlob(instance, delta, resources)) {
        guiFrameMarkBuildFailed(gui, building);
        return r4x_api.gui_frame_error_oom;
    }
    if (!validateGuiFrame(delta, instance.id)) {
        building.build_failed = true;
        return setGuiFrameResult(gui, r4x_api.gui_frame_error_invalid);
    }
    _ = std.math.add(u64, building.command_count, delta.command_count) catch {
        building.build_failed = true;
        return setGuiFrameResult(gui, r4x_api.gui_frame_error_overflow);
    };
    _ = std.math.add(u64, building.resource_len, delta.resource_len) catch {
        building.build_failed = true;
        return setGuiFrameResult(gui, r4x_api.gui_frame_error_overflow);
    };
    _ = std.math.add(u64, building.raster_words, delta.raster_words) catch {
        building.build_failed = true;
        return setGuiFrameResult(gui, r4x_api.gui_frame_error_overflow);
    };
    _ = std.math.add(u64, building.node_sequence, delta.node_sequence) catch {
        building.build_failed = true;
        return setGuiFrameResult(gui, r4x_api.gui_frame_error_overflow);
    };

    spliceGuiFrameDelta(gui, building, delta);
    return setGuiFrameResult(gui, r4x_api.gui_frame_result_ok);
}

fn guiFrameMarkBuildFailed(gui: *ProgramGuiPayload, frame: ?*ProgramGuiFramePayload) void {
    if (frame) |value| value.build_failed = true;
    gui.frame_oom +%= 1;
    gui.frame_last_error = r4x_api.gui_frame_error_oom;
    instance_storage_stats.gui_frame_oom_failures +%= 1;
}

fn setGuiFrameResult(gui: *ProgramGuiPayload, result: i32) i32 {
    gui.frame_last_error = result;
    return result;
}

fn guiFrameBegin(instance: *ProgramInstance) i32 {
    const gui = ensureGuiPayload(instance) orelse return -6;

    // BEGIN is a state transition, so reject an already active transaction
    // before performing the fallible allocation for the replacement frame.
    // The second check below closes the future-SMP race without ever holding
    // the frame-state lock across allocation or release.
    lockGuiFrameState(gui);
    if (gui.building_frame != null) {
        _ = gui.frame_lock.unlock();
        return -3;
    }
    _ = gui.frame_lock.unlock();

    const frame = allocateGuiFramePayload(instance.id, true) orelse {
        guiFrameMarkBuildFailed(gui, null);
        return -6;
    };
    lockGuiFrameState(gui);
    if (gui.building_frame != null) {
        _ = gui.frame_lock.unlock();
        _ = releaseGuiFrame(frame, instance.id);
        return -3;
    }
    gui.building_frame = frame;
    refreshGuiFrameOwnerPeak(gui);
    _ = gui.frame_lock.unlock();
    return 0;
}

fn validGuiDamageRegion(region: DisplayDamageRect) bool {
    if (region.w == 0 or region.h == 0) return false;
    const right = std.math.add(i64, @as(i64, region.x), @as(i64, region.w)) catch return false;
    const bottom = std.math.add(i64, @as(i64, region.y), @as(i64, region.h)) catch return false;
    return right <= std.math.maxInt(i32) and bottom <= std.math.maxInt(i32);
}

fn guiFrameBeginDamage(instance: *ProgramInstance, regions: []const DisplayDamageRect) i32 {
    if (regions.len == 0 or regions.len > r4x_api.gui_frame_max_damage_regions) return r4x_api.gui_frame_error_invalid;
    for (regions) |region| if (!validGuiDamageRegion(region)) return r4x_api.gui_frame_error_invalid;
    const gui = ensureGuiPayload(instance) orelse return r4x_api.gui_frame_error_unavailable;
    lockGuiFrameState(gui);
    if (gui.building_frame != null or gui.committed_frame == null or
        gui.committed_frame.?.chain_depth >= r4x_api.gui_frame_max_delta_chain)
    {
        _ = gui.frame_lock.unlock();
        return setGuiFrameResult(gui, r4x_api.gui_frame_error_state);
    }
    _ = gui.frame_lock.unlock();

    const frame = allocateGuiFramePayload(instance.id, true) orelse {
        guiFrameMarkBuildFailed(gui, null);
        return r4x_api.gui_frame_error_oom;
    };
    @memcpy(frame.damage_regions[0..regions.len], regions);
    frame.damage_count = @intCast(regions.len);
    lockGuiFrameState(gui);
    if (gui.building_frame != null or gui.committed_frame == null or
        gui.committed_frame.?.chain_depth >= r4x_api.gui_frame_max_delta_chain)
    {
        _ = gui.frame_lock.unlock();
        _ = releaseGuiFrame(frame, instance.id);
        return setGuiFrameResult(gui, r4x_api.gui_frame_error_state);
    }
    gui.building_frame = frame;
    refreshGuiFrameOwnerPeak(gui);
    _ = gui.frame_lock.unlock();
    return setGuiFrameResult(gui, r4x_api.gui_frame_result_ok);
}

fn guiFrameReplaceBuild(instance: *ProgramInstance, explicit_build: bool) ?*ProgramGuiFramePayload {
    const gui = ensureGuiPayload(instance) orelse return null;
    const frame = allocateGuiFramePayload(instance.id, explicit_build) orelse {
        guiFrameMarkBuildFailed(gui, gui.building_frame);
        return null;
    };
    lockGuiFrameState(gui);
    const old = gui.building_frame;
    gui.building_frame = frame;
    refreshGuiFrameOwnerPeak(gui);
    _ = gui.frame_lock.unlock();
    if (old) |old_frame| _ = releaseGuiFrame(old_frame, instance.id);
    return frame;
}

fn guiFrameEnsureBuild(instance: *ProgramInstance) ?*ProgramGuiFramePayload {
    const gui = ensureGuiPayload(instance) orelse return null;
    if (gui.building_frame) |frame| return if (frame.build_failed) null else frame;

    while (true) {
        if (captureCommittedGuiFrame(instance, null)) |captured_value| {
            var captured = captured_value;
            const clone = cloneGuiFrame(instance, captured.frame) orelse {
                guiFrameMarkBuildFailed(gui, null);
                releaseCapturedGuiFrame(instance, &captured);
                return null;
            };
            lockGuiFrameState(gui);
            if (gui.building_frame) |frame| {
                _ = gui.frame_lock.unlock();
                releaseCapturedGuiFrame(instance, &captured);
                _ = releaseGuiFrame(clone, instance.id);
                return if (frame.build_failed) null else frame;
            }
            if (gui.committed_frame == captured.frame and gui.committed_frame.?.generation == captured.generation) {
                gui.building_frame = clone;
                refreshGuiFrameOwnerPeak(gui);
                _ = gui.frame_lock.unlock();
                releaseCapturedGuiFrame(instance, &captured);
                return clone;
            }
            _ = gui.frame_lock.unlock();
            releaseCapturedGuiFrame(instance, &captured);
            _ = releaseGuiFrame(clone, instance.id);
            continue;
        }

        const empty = allocateGuiFramePayload(instance.id, false) orelse {
            guiFrameMarkBuildFailed(gui, null);
            return null;
        };
        lockGuiFrameState(gui);
        if (gui.building_frame) |frame| {
            _ = gui.frame_lock.unlock();
            _ = releaseGuiFrame(empty, instance.id);
            return if (frame.build_failed) null else frame;
        }
        if (gui.committed_frame == null) {
            gui.building_frame = empty;
            refreshGuiFrameOwnerPeak(gui);
            _ = gui.frame_lock.unlock();
            return empty;
        }
        _ = gui.frame_lock.unlock();
        _ = releaseGuiFrame(empty, instance.id);
    }
}

fn guiFrameCancel(instance: *ProgramInstance) i32 {
    const gui = instance.gui_payload orelse return -2;
    lockGuiFrameState(gui);
    const frame = gui.building_frame orelse {
        _ = gui.frame_lock.unlock();
        return -3;
    };
    gui.building_frame = null;
    gui.frame_cancels +%= 1;
    instance_storage_stats.gui_frame_cancels +%= 1;
    _ = gui.frame_lock.unlock();
    _ = releaseGuiFrame(frame, instance.id);
    return 0;
}

fn guiFrameCommit(instance: *ProgramInstance) i32 {
    const gui = instance.gui_payload orelse return -2;
    const building = gui.building_frame orelse return -3;
    if (building.build_failed or !validateGuiFrame(building, instance.id)) return -3;
    const is_delta = building.damage_count != 0;
    var release_old: ?*ProgramGuiFramePayload = null;
    lockGuiFrameState(gui);
    if (gui.building_frame != building or building.build_failed) {
        _ = gui.frame_lock.unlock();
        return -3;
    }
    // Consume the global generation only after the prepared frame has won
    // the final state check.  From here through the pointer swap there are no
    // fallible steps, so failed/cancelled builds never burn a generation.
    const old = gui.committed_frame;
    if (is_delta and (old == null or old.?.chain_depth >= r4x_api.gui_frame_max_delta_chain)) {
        _ = gui.frame_lock.unlock();
        return setGuiFrameResult(gui, r4x_api.gui_frame_error_state);
    }
    const generation = reserveGuiFrameGeneration() orelse {
        building.build_failed = true;
        _ = gui.frame_lock.unlock();
        return -7;
    };
    building.generation = generation;
    building.explicit_build = false;
    if (is_delta) {
        building.base_frame = old.?;
        building.chain_depth = old.?.chain_depth + 1;
    }
    gui.committed_frame = building;
    gui.building_frame = null;
    gui.frame_commits +%= 1;
    instance_storage_stats.gui_frame_commits +%= 1;
    if (is_delta) {
        gui.frame_delta_commits +%= 1;
        gui.frame_avoided_clone_bytes +|= guiFrameBytes(old.?);
    } else {
        gui.frame_full_commits +%= 1;
        if (old) |old_frame| {
            if (!guiFrameChainHasReaders(old_frame)) {
                release_old = old_frame;
            } else {
                old_frame.retired = true;
                old_frame.retired_next = gui.retired_frames;
                gui.retired_frames = old_frame;
            }
        }
    }
    var command_cursor = building.command_payload;
    while (command_cursor) |payload| : (command_cursor = payload.next) {
        for (guiCommandPayloadCommandsConst(payload)[0..payload.command_count]) |command| {
            if (command.kind != r4x_api.gui_frame_command_kind_indexed8) continue;
            gui.frame_indexed8_commands +%= 1;
            gui.frame_indexed8_resource_bytes +|= command.payload_bytes;
        }
    }
    refreshGuiFrameOwnerPeak(gui);
    _ = gui.frame_lock.unlock();
    if (release_old) |frame| _ = releaseGuiFrame(frame, instance.id);
    bumpGuiRevision(instance);
    return 0;
}

const EXIT_HISTORY_MAX: usize = 16;

const ProgramCompletionState = enum(u8) {
    private,
    pending,
    ready,
    consumed,
};

const ProgramCompletionNode = struct {
    next: ?*ProgramCompletionNode = null,
    handle: ProgramProcessHandle = .{},
    state: ProgramCompletionState = .private,
    slot_attached: bool = false,
    owner: bool = false,
    owner_handle: ProgramProcessHandle = .{},
    legacy_id: bool = false,
    retain_output: bool = false,
    sequence: u64 = 0,
    start_tick: u64 = 0,
    finish_tick: u64 = 0,
    exit_code: i32 = 0,
    task_id: u32 = 0,
    output_revision: u32 = 0,
    output_length: u32 = 0,
    flags: u32 = 0,
    app_class: u8 = 0,
    role: u8 = 0,
    exit_reason: u8 = PROGRAM_EXIT_REASON_NATURAL,
    console_state: ConsoleState = .{},
    output_payload: ?*ProgramConsoleTranscriptPayload = null,
    output_storage_bytes: u64 = 0,
};

const ExitRecord = struct {
    used: bool = false,
    handle: ProgramProcessHandle = .{},
    sequence: u64 = 0,
    start_tick: u64 = 0,
    finish_tick: u64 = 0,
    app_class: u8 = 0,
    role: u8 = 0,
    exit_reason: u8 = PROGRAM_EXIT_REASON_NATURAL,
    exit_code: i32 = 0,
};

var args_buffer: [ARGS_MAX + 1]u8 = .{0} ** (ARGS_MAX + 1);
var program_registry_head: ?*ProgramRegistryChunk = null;
var program_registry_tail: ?*ProgramRegistryChunk = null;
var program_registry_lock = sync.Mutex.initClass("r4x-program-registry", sync.LockRank.program_registry, .sleepable);
var program_registry_stats: ProgramRegistryStats = .{};
var program_registry_mutation_epoch: u64 = 1;
var next_program_registry_chunk_serial: u64 = 1;
var program_registry_failure_after: ?u32 = null;
var program_registry_failure_cursor: u32 = 0;
var program_registry_failure_next_growth: bool = false;
var next_program_generation: u64 = 1;
var program_generation_exhausted: bool = false;
var program_completion_head: ?*ProgramCompletionNode = null;
var program_completion_tail: ?*ProgramCompletionNode = null;
var program_retire_head: ?*ProgramRegistrySlot = null;
var program_retire_tail: ?*ProgramRegistrySlot = null;
var program_reaper_event = sync.Event.initMode(false, .auto_reset);
var program_reaper_started: bool = false;
var program_thread_head: ?*ProgramThread = null;
var program_thread_tail: ?*ProgramThread = null;
var program_thread_count: usize = 0;
var program_thread_peak: usize = 0;
var program_thread_mutation_epoch: u64 = 1;
var program_thread_create_failures: u64 = 0;
var program_stack_telemetry_by_profile: [MEMORY_PROFILE_COUNT]ProgramStackTelemetryStats = .{ProgramStackTelemetryStats{}} ** MEMORY_PROFILE_COUNT;
var next_thread_generation: u64 = 1;
var next_inventory_snapshot_generation: u64 = 1;
var async_io_requests: [MAX_ASYNC_IO_REQUESTS]AsyncIoRequest = .{AsyncIoRequest{}} ** MAX_ASYNC_IO_REQUESTS;
var file_range_locks: [MAX_FILE_RANGE_LOCKS]FileRangeLock = .{FileRangeLock{}} ** MAX_FILE_RANGE_LOCKS;
var async_io_retire_retry_test_armed = false;
var async_io_retire_retry_test_consumed = false;
var async_io_retire_retry_test_remaining: u32 = 0;
var async_io_retire_retry_test_request_id: u32 = 0;
var exit_history: [EXIT_HISTORY_MAX]ExitRecord = .{ExitRecord{}} ** EXIT_HISTORY_MAX;
var exit_history_head: usize = 0;
var exit_history_count: usize = 0;
var next_exit_sequence: u64 = 1;
var last_exit_sequence: u64 = 0;
var next_instance_id: u32 = 1;

var gui_event_push_attempts: u64 = 0;
var gui_event_accepted: u64 = 0;
var gui_mouse_move_coalesced: u64 = 0;
var gui_mouse_move_evicted: u64 = 0;
var gui_event_rejected: u64 = 0;
var console_input_push_calls: u64 = 0;
var console_input_batch_calls: u64 = 0;
var console_input_bytes_attempted: u64 = 0;
var console_input_bytes_accepted: u64 = 0;
var console_input_full_events: u64 = 0;
var console_read_calls: u64 = 0;
var console_read_empty: u64 = 0;
var console_read_bytes: u64 = 0;
var console_wait_calls: u64 = 0;
var console_wait_blocks: u64 = 0;
var console_wait_immediate: u64 = 0;
var console_wait_wakes: u64 = 0;
var console_wait_timeouts: u64 = 0;
var console_wait_cancellations: u64 = 0;
var console_output_write_calls: u64 = 0;
var console_output_source_bytes: u64 = 0;
var console_output_visible_append_bytes: u64 = 0;
var console_output_capture_append_bytes: u64 = 0;
var console_output_shared_bytes: u64 = 0;
var console_output_revision_batches: u64 = 0;
var console_output_desktop_signals: u64 = 0;
var console_output_compactions: u64 = 0;
var console_output_compaction_bytes: u64 = 0;
var console_output_segment_drops: u64 = 0;
var console_output_segment_drop_bytes: u64 = 0;
var console_output_lock = sync.Mutex.initClass("console-output", sync.LockRank.program_instances, .sleepable);
var program_launch_attempts: u64 = 0;
var program_entries_started: u64 = 0;
var program_attach_wait_events: u64 = 0;

pub const ProgramLifecycleFailurePhase = enum(u8) {
    none = 0,
    completion_reserve = 1,
    storage = 2,
    image = 3,
    stack = 4,
    task = 5,
    publish = 6,
    exit_commit = 7,
    cancel_execution = 8,
    detach_task = 9,
    output_detach = 10,
    storage_release = 11,
    image_stack_vm_release = 12,
    slot_reclaim = 13,
};

var program_lifecycle_failure_phase: ?ProgramLifecycleFailurePhase = null;
var program_lifecycle_last_phase: u32 = 0;
var program_lifecycle_failure_consumed: bool = false;
var program_lifecycle_reaper_signalled: bool = false;
var program_lifecycle_retried: bool = false;
var program_lifecycle_recovered: bool = false;
var program_lifecycle_failure_lock = sync.Mutex.initClass("r4x-lifecycle-failure", sync.LockRank.program_instances, .sleepable);

pub fn configureProgramLifecycleFailureForTest(phase: ?ProgramLifecycleFailurePhase) void {
    const selected = phase orelse .none;
    if (selected != .none and !programLifecycleSelfTestAllowed()) return;
    if (!program_lifecycle_failure_lock.lock(sync.WAIT_FOREVER)) return;
    defer _ = program_lifecycle_failure_lock.unlock();
    program_lifecycle_failure_phase = if (selected == .none) null else selected;
    program_lifecycle_last_phase = if (selected == .none) 0 else @intFromEnum(selected);
    program_lifecycle_failure_consumed = false;
    program_lifecycle_reaper_signalled = false;
    program_lifecycle_retried = false;
    program_lifecycle_recovered = false;
}

fn armProgramLifecycleFailureForTest(phase: ProgramLifecycleFailurePhase) bool {
    if (phase == .none or !programLifecycleSelfTestAllowed()) return false;
    if (!program_lifecycle_failure_lock.lock(sync.WAIT_FOREVER)) return false;
    defer _ = program_lifecycle_failure_lock.unlock();
    if (program_lifecycle_failure_phase != null) return false;
    program_lifecycle_failure_phase = phase;
    program_lifecycle_last_phase = @intFromEnum(phase);
    program_lifecycle_failure_consumed = false;
    program_lifecycle_reaper_signalled = false;
    program_lifecycle_retried = false;
    program_lifecycle_recovered = false;
    return true;
}

const ProgramLifecycleFailureSnapshot = struct {
    phase: u32 = 0,
    armed: bool = false,
    consumed: bool = false,
    reaper_signalled: bool = false,
    retried: bool = false,
    recovered: bool = false,
};

fn programLifecycleFailureSnapshot() ProgramLifecycleFailureSnapshot {
    if (!program_lifecycle_failure_lock.lock(sync.WAIT_FOREVER)) return .{};
    defer _ = program_lifecycle_failure_lock.unlock();
    return .{
        .phase = if (program_lifecycle_failure_phase) |phase| @intFromEnum(phase) else program_lifecycle_last_phase,
        .armed = program_lifecycle_failure_phase != null,
        .consumed = program_lifecycle_failure_consumed,
        .reaper_signalled = program_lifecycle_reaper_signalled,
        .retried = program_lifecycle_retried,
        .recovered = program_lifecycle_recovered,
    };
}

fn signalProgramReaperForTest() void {
    if (program_lifecycle_failure_lock.lock(sync.WAIT_FOREVER)) {
        program_lifecycle_reaper_signalled = true;
        _ = program_lifecycle_failure_lock.unlock();
    }
    program_reaper_event.signal();
}

fn consumeProgramLifecycleFailure(phase: ProgramLifecycleFailurePhase) bool {
    if (!program_lifecycle_failure_lock.lock(sync.WAIT_FOREVER)) return false;
    defer _ = program_lifecycle_failure_lock.unlock();
    if (program_lifecycle_failure_phase != phase) return false;
    program_lifecycle_failure_phase = null;
    program_lifecycle_failure_consumed = true;
    return true;
}

const PROGRAM_REGISTRY_ADMISSION_NONE: i32 = 0;
const PROGRAM_REGISTRY_ADMISSION_MEMORY: i32 = -1;
const PROGRAM_REGISTRY_ADMISSION_ID_EXHAUSTED: i32 = -2;

const ProgramRegistryChunkCandidateResult = union(enum) {
    chunk: *ProgramRegistryChunk,
    injected_failure,
    allocation_failure,
    lock_failure,
};

const ProgramRegistryGrowResult = enum {
    grown,
    capacity_available,
    injected_failure,
    allocation_failure,
    lock_failure,
};

fn lockProgramRegistry() bool {
    return program_registry_lock.lock(sync.WAIT_FOREVER);
}

fn unlockProgramRegistry() void {
    _ = program_registry_lock.unlock();
}

fn programRegistryStateIsVisible(state: ProgramRegistrySlotState) bool {
    return state == .publish or state == .run or state == .exit or state == .done;
}

fn programRegistryStateOwnsPublishedPayloads(state: ProgramRegistrySlotState) bool {
    return state != .free and state != .create and state != .reap;
}

fn programRegistryStateIsRunning(state: ProgramRegistrySlotState) bool {
    return state == .publish or state == .run;
}

fn programHandleValid(handle: ProgramProcessHandle) bool {
    return handle.instance_id != 0 and handle.generation != 0 and handle.reserved == 0;
}

fn programHandleEqual(a: ProgramProcessHandle, b: ProgramProcessHandle) bool {
    return a.instance_id == b.instance_id and a.reserved == b.reserved and a.generation == b.generation;
}

fn programHandleForSlot(slot: *const ProgramRegistrySlot) ProgramProcessHandle {
    return .{ .instance_id = slot.public_id, .reserved = 0, .generation = slot.generation };
}

fn programHandleForReservation(reservation: *const ProgramInstanceReservation) ProgramProcessHandle {
    return .{ .instance_id = reservation.id, .reserved = 0, .generation = reservation.generation };
}

fn programHandleForInstance(instance: *const ProgramInstance) ?ProgramProcessHandle {
    const locked = lockProgramRegistry();
    if (!locked) return null;
    defer unlockProgramRegistry();
    const slot = programRegistrySlotForInstanceLocked(instance) orelse return null;
    if (slot.state == .free or slot.state == .reap) return null;
    return programHandleForSlot(slot);
}

fn programRegistryChunkIsEmptyAndUnpinned(chunk: *const ProgramRegistryChunk) bool {
    for (chunk.slots) |slot| {
        if (slot.state != .free or slot.pin_count != 0) return false;
    }
    return true;
}

fn findFreeProgramRegistrySlotLocked() ?*ProgramRegistrySlot {
    var chunk = program_registry_head;
    while (chunk) |current| : (chunk = current.next) {
        for (&current.slots) |*slot| {
            if (slot.state == .free and slot.pin_count == 0) return slot;
        }
    }
    return null;
}

fn programRegistryChunkForSlotLocked(wanted: *const ProgramRegistrySlot) ?*ProgramRegistryChunk {
    var chunk = program_registry_head;
    while (chunk) |current| : (chunk = current.next) {
        for (&current.slots) |*slot| {
            if (slot == wanted) return current;
        }
    }
    return null;
}

fn programRegistrySlotForInstanceLocked(wanted: *const ProgramInstance) ?*ProgramRegistrySlot {
    var chunk = program_registry_head;
    while (chunk) |current| : (chunk = current.next) {
        for (&current.slots) |*slot| {
            if (&slot.instance == wanted) return slot;
        }
    }
    return null;
}

fn lookupProgramRegistrySlotLocked(id: u32, include_retiring: bool) ?*ProgramRegistrySlot {
    if (id == 0) return null;
    var chunk = program_registry_head;
    while (chunk) |current| : (chunk = current.next) {
        for (&current.slots) |*slot| {
            const state_matches = programRegistryStateIsVisible(slot.state) or (include_retiring and (slot.state == .retire or slot.state == .reap));
            if (state_matches and slot.public_id == id and slot.instance.id == id) return slot;
        }
    }
    return null;
}

fn lookupProgramRegistryHandleLocked(handle: ProgramProcessHandle, include_retiring: bool) ?*ProgramRegistrySlot {
    if (!programHandleValid(handle)) return null;
    const slot = lookupProgramRegistrySlotLocked(handle.instance_id, include_retiring) orelse return null;
    if (slot.generation != handle.generation) return null;
    return slot;
}

fn programRegistryHasIdLocked(id: u32) bool {
    if (id == 0) return false;
    var chunk = program_registry_head;
    while (chunk) |current| : (chunk = current.next) {
        for (&current.slots) |*slot| {
            if (slot.state != .free and slot.public_id == id) return true;
        }
    }
    return false;
}

fn completionForHandleLocked(handle: ProgramProcessHandle) ?*ProgramCompletionNode {
    var current = program_completion_head;
    while (current) |node| : (current = node.next) {
        if (node.state != .consumed and programHandleEqual(node.handle, handle)) return node;
    }
    return null;
}

fn completionForIdLocked(id: u32) ?*ProgramCompletionNode {
    if (id == 0) return null;
    var current = program_completion_head;
    while (current) |node| : (current = node.next) {
        if (node.state != .consumed and node.handle.instance_id == id) return node;
    }
    return null;
}

fn completionHasIdLocked(id: u32) bool {
    return completionForIdLocked(id) != null;
}

fn programHandleMissingStatusLocked(handle: ProgramProcessHandle) i32 {
    if (!programHandleValid(handle)) return PROGRAM_HANDLE_ERROR_INVALID;
    return if (programRegistryHasIdLocked(handle.instance_id) or completionHasIdLocked(handle.instance_id))
        PROGRAM_HANDLE_ERROR_STALE
    else
        PROGRAM_HANDLE_ERROR_NOT_FOUND;
}

fn programHandleForId(id: u32) ?ProgramProcessHandle {
    const locked = lockProgramRegistry();
    if (!locked) return null;
    defer unlockProgramRegistry();
    const slot = lookupProgramRegistrySlotLocked(id, true) orelse return null;
    return programHandleForSlot(slot);
}

fn programRegistryIdInUseLocked(id: u32) bool {
    if (id == 0) return true;
    return programRegistryHasIdLocked(id) or completionHasIdLocked(id);
}

fn countPublishedProgramRegistrySlotsLocked() u64 {
    var count: u64 = 0;
    var chunk = program_registry_head;
    while (chunk) |current| : (chunk = current.next) {
        for (current.slots) |slot| {
            if (programRegistryStateIsVisible(slot.state)) count +%= 1;
        }
    }
    return count;
}

fn allocateProgramGenerationLocked() ?u64 {
    if (program_generation_exhausted or next_program_generation == 0) {
        program_registry_stats.last_admission_error = PROGRAM_HANDLE_ERROR_GENERATION_EXHAUSTED;
        return null;
    }
    const generation = next_program_generation;
    if (generation == std.math.maxInt(u64)) {
        next_program_generation = 0;
        program_generation_exhausted = true;
    } else {
        next_program_generation = generation + 1;
    }
    program_registry_stats.next_generation = next_program_generation;
    return generation;
}

fn allocateProgramInstanceIdLocked() ?u32 {
    // At most one more probe than the number of blocking identities is needed:
    // among N blocking identities, N+1 non-zero candidates contain a free ID.
    // Owned completions may outlive both their slot and later chunk shrink, so
    // slot capacity alone is not an upper bound anymore. Pending completions
    // duplicate live IDs, which only makes this a conservative bounded scan.
    const capacity: u64 = @as(u64, program_registry_stats.chunk_count) * PROGRAM_REGISTRY_CHUNK_SLOTS;
    const blockers = capacity +| program_registry_stats.completion_pending +| program_registry_stats.completion_ready;
    var remaining = @min(blockers +| 1, @as(u64, std.math.maxInt(u32)));
    while (remaining != 0) : (remaining -= 1) {
        var candidate = next_instance_id;
        if (candidate == 0) candidate = 1;
        next_instance_id = candidate +% 1;
        if (next_instance_id == 0) {
            next_instance_id = 1;
            program_registry_stats.id_wraps +%= 1;
        }
        if (!programRegistryIdInUseLocked(candidate)) return candidate;
        program_registry_stats.id_collisions +%= 1;
    }
    program_registry_stats.last_admission_error = PROGRAM_REGISTRY_ADMISSION_ID_EXHAUSTED;
    return null;
}

pub fn configureProgramRegistryFailureForTest(fail_after: ?u32) void {
    const locked = lockProgramRegistry();
    if (!locked) return;
    program_registry_failure_after = fail_after;
    program_registry_failure_cursor = 0;
    unlockProgramRegistry();
}

pub fn configureProgramRegistryNextIdForTest(next_id: u32) void {
    const locked = lockProgramRegistry();
    if (!locked) return;
    next_instance_id = if (next_id == 0) 1 else next_id;
    unlockProgramRegistry();
}

pub fn configureProgramRegistryNextGenerationForTest(next_generation: u64) void {
    const locked = lockProgramRegistry();
    if (!locked) return;
    next_program_generation = next_generation;
    program_generation_exhausted = next_generation == 0;
    program_registry_stats.next_generation = next_generation;
    unlockProgramRegistry();
}

pub fn armProgramRegistryNextGrowthFailureForTest() bool {
    const locked = lockProgramRegistry();
    if (!locked) return false;
    defer unlockProgramRegistry();
    if (program_registry_failure_next_growth) return false;
    program_registry_failure_next_growth = true;
    return true;
}

pub fn resetProgramRegistryGrowthFailureForTest() void {
    const locked = lockProgramRegistry();
    if (!locked) return;
    program_registry_failure_next_growth = false;
    unlockProgramRegistry();
}

fn shouldFailProgramRegistryGrowthLocked() bool {
    if (program_registry_failure_next_growth) {
        program_registry_failure_next_growth = false;
        program_registry_stats.forced_failures +%= 1;
        return true;
    }
    const target = program_registry_failure_after orelse return false;
    const cursor = program_registry_failure_cursor;
    program_registry_failure_cursor +%= 1;
    return cursor == target;
}

fn allocateProgramRegistryChunkCandidate() ProgramRegistryChunkCandidateResult {
    // Failure accounting and the one-shot seam are serialized, but the heap
    // operation itself deliberately happens with no registry lock held.
    const locked = lockProgramRegistry();
    if (!locked) return .lock_failure;
    program_registry_stats.growth_attempts +%= 1;
    const injected_failure = shouldFailProgramRegistryGrowthLocked();
    if (injected_failure) {
        program_registry_stats.growth_failures +%= 1;
        program_registry_stats.last_admission_error = PROGRAM_REGISTRY_ADMISSION_MEMORY;
    }
    unlockProgramRegistry();
    if (injected_failure) return .injected_failure;

    const memory = heap.alloc(@sizeOf(ProgramRegistryChunk), @alignOf(ProgramRegistryChunk)) orelse {
        const retry_locked = lockProgramRegistry();
        if (retry_locked) {
            program_registry_stats.growth_failures +%= 1;
            program_registry_stats.last_admission_error = PROGRAM_REGISTRY_ADMISSION_MEMORY;
            unlockProgramRegistry();
        }
        return .allocation_failure;
    };
    const chunk: *ProgramRegistryChunk = @ptrCast(@alignCast(memory.ptr));
    chunk.* = .{};
    return .{ .chunk = chunk };
}

fn freeProgramRegistryChunkMemory(chunk: *ProgramRegistryChunk) bool {
    const bytes: [*]u8 = @ptrCast(chunk);
    return heap.free(bytes[0..@sizeOf(ProgramRegistryChunk)]) == .ok;
}

fn linkProgramRegistryChunkLocked(chunk: *ProgramRegistryChunk) void {
    chunk.next = null;
    chunk.serial = next_program_registry_chunk_serial;
    next_program_registry_chunk_serial +%= 1;
    if (next_program_registry_chunk_serial == 0) next_program_registry_chunk_serial = 1;
    chunk.empty_since_epoch = program_registry_mutation_epoch;
    if (program_registry_tail) |tail| {
        tail.next = chunk;
    } else {
        program_registry_head = chunk;
    }
    program_registry_tail = chunk;
    program_registry_stats.chunk_count +%= 1;
    program_registry_stats.slot_capacity +%= PROGRAM_REGISTRY_CHUNK_SLOTS;
    if (@as(u64, program_registry_stats.chunk_count) > program_registry_stats.peak_chunks) {
        program_registry_stats.peak_chunks = program_registry_stats.chunk_count;
    }
    instance_storage_stats.registry_reserved_core_bytes +%= @sizeOf(ProgramRegistryChunk);
    refreshInstanceByteTelemetry();
}

fn growProgramRegistry() ProgramRegistryGrowResult {
    const candidate = switch (allocateProgramRegistryChunkCandidate()) {
        .chunk => |chunk| chunk,
        .injected_failure => return .injected_failure,
        .allocation_failure => return .allocation_failure,
        .lock_failure => return .lock_failure,
    };
    var linked = false;
    const locked = lockProgramRegistry();
    if (locked) {
        // A concurrent grower may have installed capacity while this candidate
        // was allocated. Keep only one winner; the loser frees after unlock.
        if (findFreeProgramRegistrySlotLocked() == null) {
            linkProgramRegistryChunkLocked(candidate);
            linked = true;
        }
        unlockProgramRegistry();
    }
    if (!linked) {
        if (!freeProgramRegistryChunkMemory(candidate)) {
            // Exact-size heap frees should succeed. If the allocator rejects
            // the race-loser release, retain the valid chunk as capacity
            // instead of leaking it outside the registry.
            const restore_locked = lockProgramRegistry();
            if (restore_locked) {
                linkProgramRegistryChunkLocked(candidate);
                linked = true;
                unlockProgramRegistry();
            }
        }
        if (!locked and !linked) return .lock_failure;
    }
    return if (linked) .grown else .capacity_available;
}

fn allocateProgramCompletionNode(reservation: *const ProgramInstanceReservation, owner: bool, owner_handle_override: ?ProgramProcessHandle, legacy_id: bool, retain_output: bool) bool {
    const memory = heap.alloc(@sizeOf(ProgramCompletionNode), @alignOf(ProgramCompletionNode)) orelse return false;
    const node: *ProgramCompletionNode = @ptrCast(@alignCast(memory.ptr));
    node.* = .{
        .handle = .{ .instance_id = reservation.id, .reserved = 0, .generation = reservation.generation },
        .owner = owner,
        .owner_handle = if (owner) owner_handle_override orelse currentProgramHandle() orelse ProgramProcessHandle{} else .{},
        .legacy_id = legacy_id,
        .retain_output = retain_output,
        .start_tick = timer.tickCount(),
        .flags = if (owner) PROGRAM_COMPLETION_FLAG_OWNER else 0,
    };

    const locked = lockProgramRegistry();
    if (locked) {
        const slot = reservation.slot;
        if (slot.state == .create and slot.public_id == reservation.id and slot.generation == reservation.generation and slot.completion == null) {
            slot.completion = node;
            node.slot_attached = true;
            unlockProgramRegistry();
            return true;
        }
        unlockProgramRegistry();
    }
    const bytes: [*]u8 = @ptrCast(node);
    _ = heap.free(bytes[0..@sizeOf(ProgramCompletionNode)]);
    return false;
}

fn freeProgramCompletionNodeMemory(node: *ProgramCompletionNode) void {
    const bytes: [*]u8 = @ptrCast(node);
    _ = heap.free(bytes[0..@sizeOf(ProgramCompletionNode)]);
}

fn linkProgramCompletionLocked(node: *ProgramCompletionNode) void {
    node.next = null;
    node.state = .pending;
    if (program_completion_tail) |tail| tail.next = node else program_completion_head = node;
    program_completion_tail = node;
    program_registry_stats.completion_pending +%= 1;
    const total = program_registry_stats.completion_pending +% program_registry_stats.completion_ready;
    if (total > program_registry_stats.completion_peak) program_registry_stats.completion_peak = total;
    bumpProgramInventoryEpochLocked();
}

fn unlinkProgramCompletionLocked(node: *ProgramCompletionNode) bool {
    var previous: ?*ProgramCompletionNode = null;
    var current = program_completion_head;
    while (current) |candidate| : (current = candidate.next) {
        if (candidate != node) {
            previous = candidate;
            continue;
        }
        if (previous) |prev| prev.next = candidate.next else program_completion_head = candidate.next;
        if (program_completion_tail == candidate) program_completion_tail = previous;
        candidate.next = null;
        bumpProgramInventoryEpochLocked();
        return true;
    }
    return false;
}

fn reserveProgramInstanceSlot() ProgramInstanceReservationResult {
    // A synthetic failure is an acceptance boundary and must remain visible
    // to its caller. A genuine heap OOM gets one bounded pressure-recovery
    // cycle: reap only eligible completed hosted consoles, run the positive
    // pressure shrink and retry admission. Existing live instances are never
    // retired by this path.
    var pressure_recovery_attempted = false;
    while (true) {
        const locked = lockProgramRegistry();
        if (!locked) return .{ .failure = .no_memory };
        if (findFreeProgramRegistrySlotLocked()) |slot| {
            const id = allocateProgramInstanceIdLocked() orelse {
                unlockProgramRegistry();
                return .{ .failure = .no_memory };
            };
            const generation = allocateProgramGenerationLocked() orelse {
                unlockProgramRegistry();
                return .{ .failure = .generation_exhausted };
            };
            slot.generation = generation;
            slot.public_id = id;
            slot.pin_count = 0;
            slot.reclaim_pending = false;
            slot.retire_queued = false;
            slot.retire_in_progress = false;
            slot.retire_phase = .cancel_execution;
            slot.retire_output_detached = false;
            slot.retire_storage_released = false;
            slot.retire_image_stack_released = false;
            slot.retire_owner_released = false;
            slot.retire_attempts = 0;
            slot.retire_next = null;
            slot.completion = null;
            slot.instance = .{ .id = id };
            slot.state = .create;
            if (programRegistryChunkForSlotLocked(slot)) |chunk| chunk.empty_since_epoch = 0;
            bumpProgramInventoryEpochLocked();
            program_registry_stats.last_admission_error = PROGRAM_REGISTRY_ADMISSION_NONE;
            const reservation = ProgramInstanceReservation{ .slot = slot, .id = id, .generation = generation };
            unlockProgramRegistry();
            return .{ .reservation = reservation };
        }
        unlockProgramRegistry();
        switch (growProgramRegistry()) {
            .grown, .capacity_available => {},
            .injected_failure, .lock_failure => return .{ .failure = .no_memory },
            .allocation_failure => {
                if (pressure_recovery_attempted) return .{ .failure = .no_memory };
                pressure_recovery_attempted = true;
                reapHostedConsoleInstancesForPressure();
            },
        }
    }
}

fn clearProgramRegistrySlotLocked(slot: *ProgramRegistrySlot) void {
    slot.state = .free;
    slot.public_id = 0;
    slot.pin_count = 0;
    slot.reclaim_pending = false;
    slot.retire_queued = false;
    slot.retire_in_progress = false;
    slot.retire_phase = .cancel_execution;
    slot.retire_output_detached = false;
    slot.retire_storage_released = false;
    slot.retire_image_stack_released = false;
    slot.retire_owner_released = false;
    slot.retire_attempts = 0;
    slot.retire_next = null;
    slot.completion = null;
    slot.instance = .{};
    bumpProgramInventoryEpochLocked();
    if (programRegistryChunkForSlotLocked(slot)) |chunk| {
        if (programRegistryChunkIsEmptyAndUnpinned(chunk)) chunk.empty_since_epoch = program_registry_mutation_epoch;
    }
}

fn bumpProgramInventoryEpochLocked() void {
    program_registry_mutation_epoch +%= 1;
    if (program_registry_mutation_epoch == 0) program_registry_mutation_epoch = 1;
}

fn cancelProgramInstanceReservation(reservation: *const ProgramInstanceReservation) void {
    var completion: ?*ProgramCompletionNode = null;
    const locked = lockProgramRegistry();
    if (!locked) return;
    const slot = reservation.slot;
    if (slot.state == .create and slot.public_id == reservation.id and slot.generation == reservation.generation) {
        completion = slot.completion;
        slot.completion = null;
        if (completion) |node| node.slot_attached = false;
        clearProgramRegistrySlotLocked(slot);
        program_registry_stats.rollback_count +%= 1;
    }
    unlockProgramRegistry();
    if (completion) |node| freeProgramCompletionNodeMemory(node);
    shrinkProgramRegistry(false);
}

fn publishProgramInstance(reservation: *const ProgramInstanceReservation) bool {
    const locked = lockProgramRegistry();
    if (!locked) return false;
    defer unlockProgramRegistry();
    const slot = reservation.slot;
    if (slot.state != .create or slot.public_id != reservation.id or slot.generation != reservation.generation or slot.instance.id != reservation.id) return false;
    const completion = slot.completion orelse return false;
    slot.instance.used = true;
    slot.state = .publish;
    linkProgramCompletionLocked(completion);
    program_registry_stats.publish_count +%= 1;
    const live_count = countPublishedProgramRegistrySlotsLocked();
    if (live_count > program_registry_stats.peak_live) program_registry_stats.peak_live = live_count;
    noteProgramInstancePublished(slot.instance.app_class, programInstancePayloadBytes(&slot.instance));
    return true;
}

fn markProgramInstanceDone(instance: *ProgramInstance) void {
    const locked = lockProgramRegistry();
    if (!locked) return;
    if (programRegistrySlotForInstanceLocked(instance)) |slot| {
        if ((slot.state == .publish or slot.state == .run or slot.state == .exit) and slot.public_id == instance.id) {
            slot.state = .done;
            bumpProgramInventoryEpochLocked();
        }
    }
    unlockProgramRegistry();
}

fn pinProgramHandle(handle: ProgramProcessHandle, include_done: bool) ?ProgramInstanceLease {
    const locked = lockProgramRegistry();
    if (!locked) return null;
    defer unlockProgramRegistry();
    const slot = lookupProgramRegistryHandleLocked(handle, false) orelse return null;
    if (!programRegistryStateIsRunning(slot.state) and !(include_done and (slot.state == .exit or slot.state == .done))) return null;
    if (slot.pin_count == std.math.maxInt(u32)) return null;
    slot.pin_count += 1;
    return .{ .slot = slot, .instance = &slot.instance, .id = handle.instance_id, .generation = handle.generation };
}

fn pinProgramInstance(id: u32) ?ProgramInstanceLease {
    const locked = lockProgramRegistry();
    if (!locked) return null;
    const slot = lookupProgramRegistrySlotLocked(id, false) orelse {
        unlockProgramRegistry();
        return null;
    };
    const handle = programHandleForSlot(slot);
    unlockProgramRegistry();
    return pinProgramHandle(handle, true);
}

fn unpinProgramInstance(lease: *const ProgramInstanceLease) void {
    var wake_reaper = false;
    const locked = lockProgramRegistry();
    if (!locked) return;
    const slot = lease.slot;
    if (slot.generation != lease.generation or slot.public_id != lease.id or slot.pin_count == 0) {
        program_registry_stats.stale_lease_rejections +%= 1;
    } else {
        slot.pin_count -= 1;
        wake_reaper = slot.pin_count == 0 and slot.state == .retire;
    }
    unlockProgramRegistry();
    if (wake_reaper) program_reaper_event.signal();
}

fn pinProgramThreadExecution(thread_ctx: *ProgramThread) ?*ProgramInstance {
    if (thread_ctx.execution_pinned) return thread_ctx.owner_instance;
    const handle = ProgramProcessHandle{
        .instance_id = thread_ctx.instance_id,
        .reserved = 0,
        .generation = thread_ctx.instance_generation,
    };
    const lease = pinProgramHandle(handle, false) orelse return null;
    thread_ctx.execution_pinned = true;
    return lease.instance;
}

fn unpinProgramThreadExecution(thread_ctx: *ProgramThread) void {
    if (!thread_ctx.execution_pinned) return;
    var wake_reaper = false;
    const locked = lockProgramRegistry();
    if (!locked) return;
    const handle = ProgramProcessHandle{
        .instance_id = thread_ctx.instance_id,
        .reserved = 0,
        .generation = thread_ctx.instance_generation,
    };
    if (lookupProgramRegistryHandleLocked(handle, true)) |slot| {
        if (slot.pin_count != 0) {
            slot.pin_count -= 1;
            thread_ctx.execution_pinned = false;
            wake_reaper = slot.pin_count == 0 and slot.state == .retire;
        } else {
            program_registry_stats.stale_lease_rejections +%= 1;
        }
    } else {
        program_registry_stats.stale_lease_rejections +%= 1;
    }
    unlockProgramRegistry();
    if (wake_reaper) program_reaper_event.signal();
}

fn naturalExitEpilogueOwnsTask(thread_ctx: *const ProgramThread) bool {
    const scheduler_task_present = thread_ctx.task_id != 0 and
        thread_ctx.task_generation != 0 and
        task.existsIdentity(thread_ctx.task_id, thread_ctx.task_generation);
    return lifecycle_retire_policy.naturalExitEpilogueOwnsTask(
        thread_ctx.state == .exited,
        thread_ctx.execution_pinned,
        scheduler_task_present,
    );
}

fn enqueueProgramRetireLocked(slot: *ProgramRegistrySlot) bool {
    // A slot has exactly one queue or worker owner.  In particular, an API
    // nudge cannot enqueue the stable slot address while a reaper is between
    // destructive phases.
    if (slot.retire_queued or slot.retire_in_progress) return false;
    slot.retire_queued = true;
    slot.retire_next = null;
    if (program_retire_tail) |tail| tail.retire_next = slot else program_retire_head = slot;
    program_retire_tail = slot;
    return true;
}

fn takeProgramRetireLocked() ?*ProgramRegistrySlot {
    const slot = program_retire_head orelse return null;
    program_retire_head = slot.retire_next;
    if (program_retire_head == null) program_retire_tail = null;
    slot.retire_next = null;
    slot.retire_queued = false;
    slot.retire_in_progress = true;
    return slot;
}

fn queueProgramRetire(handle: ProgramProcessHandle) bool {
    var queued = false;
    const locked = lockProgramRegistry();
    if (!locked) return false;
    if (lookupProgramRegistryHandleLocked(handle, true)) |slot| {
        if (slot.state == .retire) queued = enqueueProgramRetireLocked(slot);
    }
    unlockProgramRegistry();
    if (queued) program_reaper_event.signal();
    return queued;
}

fn markProgramHandleRunning(handle: ProgramProcessHandle) void {
    const locked = lockProgramRegistry();
    if (!locked) return;
    if (lookupProgramRegistryHandleLocked(handle, false)) |slot| {
        if (slot.state == .publish) {
            slot.state = .run;
            bumpProgramInventoryEpochLocked();
        }
    }
    unlockProgramRegistry();
}

fn programRegistryOwnerIsPublished(id: u32) bool {
    const locked = lockProgramRegistry();
    if (!locked) return false;
    defer unlockProgramRegistry();
    const slot = lookupProgramRegistrySlotLocked(id, true) orelse return false;
    return programRegistryStateOwnsPublishedPayloads(slot.state);
}

// Borrowed raw iterator for short cooperative scans that cannot yield, wait,
// retire, reclaim, or free registry chunks. It is deliberately not a general
// lifetime guarantee: mutating kill/reap paths select one ID and restart after
// finishInstance. 0.59.9 carries leases/snapshots across preemption and waits.
const ProgramRegistryIterator = struct {
    chunk: ?*ProgramRegistryChunk,
    slot_index: usize = 0,
    include_done: bool,

    fn next(self: *ProgramRegistryIterator) ?*ProgramInstance {
        while (self.chunk) |chunk| {
            while (self.slot_index < chunk.slots.len) {
                const slot = &chunk.slots[self.slot_index];
                self.slot_index += 1;
                if (slot.state == .publish or slot.state == .run or (self.include_done and (slot.state == .exit or slot.state == .done))) return &slot.instance;
            }
            self.chunk = chunk.next;
            self.slot_index = 0;
        }
        return null;
    }
};

fn programRegistryIterator(include_done: bool) ProgramRegistryIterator {
    return .{ .chunk = program_registry_head, .include_done = include_done };
}

pub fn programRegistryStats() ProgramRegistryStats {
    const locked = lockProgramRegistry();
    if (!locked) return program_registry_stats;
    defer unlockProgramRegistry();
    var result = program_registry_stats;
    result.failure_armed = program_registry_failure_next_growth;
    result.free_slots = 0;
    result.reserved_slots = 0;
    result.live_slots = 0;
    result.done_slots = 0;
    result.retiring_slots = 0;
    result.pinned_slots = 0;
    result.live_id_hash = 0;
    result.live_address_hash = 0;
    var chunk = program_registry_head;
    while (chunk) |current| : (chunk = current.next) {
        for (&current.slots) |*slot| {
            switch (slot.state) {
                .free => result.free_slots +%= 1,
                .create => result.reserved_slots +%= 1,
                .publish, .run, .exit => result.live_slots +%= 1,
                .done => result.done_slots +%= 1,
                .retire, .reap => result.retiring_slots +%= 1,
            }
            if (slot.pin_count != 0) result.pinned_slots +%= 1;
            if (slot.state == .publish or slot.state == .run) {
                result.live_id_hash ^= mixProgramRegistryHash(slot.public_id);
                result.live_address_hash ^= mixProgramRegistryHash(@intFromPtr(&slot.instance));
            }
        }
    }
    result.next_generation = next_program_generation;
    result.retire_queued = 0;
    var retire_slot = program_retire_head;
    while (retire_slot) |slot| : (retire_slot = slot.retire_next) result.retire_queued +%= 1;
    result.history_head = exit_history_head;
    result.history_count = exit_history_count;
    result.history_sequence = last_exit_sequence;
    return result;
}

fn mixProgramRegistryHash(value: u64) u64 {
    var mixed = value +% 0x9e3779b97f4a7c15;
    mixed = (mixed ^ (mixed >> 30)) *% 0xbf58476d1ce4e5b9;
    mixed = (mixed ^ (mixed >> 27)) *% 0x94d049bb133111eb;
    return mixed ^ (mixed >> 31);
}

fn unlinkProgramRegistryChunkLocked(candidate: *ProgramRegistryChunk) bool {
    var previous: ?*ProgramRegistryChunk = null;
    var current = program_registry_head;
    while (current) |chunk| : (current = chunk.next) {
        if (chunk != candidate) {
            previous = chunk;
            continue;
        }
        if (previous) |prev| prev.next = chunk.next else program_registry_head = chunk.next;
        if (program_registry_tail == chunk) program_registry_tail = previous;
        chunk.next = null;
        if (program_registry_stats.chunk_count != 0) program_registry_stats.chunk_count -= 1;
        if (program_registry_stats.slot_capacity >= PROGRAM_REGISTRY_CHUNK_SLOTS) program_registry_stats.slot_capacity -= PROGRAM_REGISTRY_CHUNK_SLOTS;
        if (instance_storage_stats.registry_reserved_core_bytes >= @sizeOf(ProgramRegistryChunk)) {
            instance_storage_stats.registry_reserved_core_bytes -= @sizeOf(ProgramRegistryChunk);
        }
        refreshInstanceByteTelemetry();
        return true;
    }
    return false;
}

fn shrinkProgramRegistry(under_pressure: bool) void {
    while (true) {
        var candidate: ?*ProgramRegistryChunk = null;
        const locked = lockProgramRegistry();
        if (!locked) return;
        if (program_registry_stats.chunk_count > PROGRAM_REGISTRY_WARM_CHUNKS) {
            const now = program_registry_mutation_epoch;
            const required_age = if (under_pressure)
                PROGRAM_REGISTRY_PRESSURE_SHRINK_HYSTERESIS
            else
                PROGRAM_REGISTRY_SHRINK_HYSTERESIS;
            var chunk = program_registry_head;
            while (chunk) |current| : (chunk = current.next) {
                if (!programRegistryChunkIsEmptyAndUnpinned(current) or current.empty_since_epoch == 0) continue;
                const old_enough = now -% current.empty_since_epoch >= required_age;
                if (!old_enough) continue;
                if (unlinkProgramRegistryChunkLocked(current)) candidate = current;
                break;
            }
        }
        unlockProgramRegistry();

        const removed = candidate orelse return;
        if (freeProgramRegistryChunkMemory(removed)) {
            const count_locked = lockProgramRegistry();
            if (count_locked) {
                program_registry_stats.shrink_count +%= 1;
                unlockProgramRegistry();
            }
            continue;
        }

        // Heap rejection leaves the candidate valid. Restore it instead of
        // losing capacity or leaking an unreachable chunk.
        const restore_locked = lockProgramRegistry();
        if (restore_locked) {
            linkProgramRegistryChunkLocked(removed);
            unlockProgramRegistry();
        }
        return;
    }
}

pub fn reclaimProgramRegistryUnderPressure() void {
    shrinkProgramRegistry(true);
}
var next_thread_id: u32 = 1;
var next_async_io_id: u32 = 1;
var async_io_lock = sync.Mutex.initClass("r4x-async-io", sync.LockRank.program_instances, .sleepable);
var file_range_lock = sync.Mutex.initClass("r4x-file-range-lock", sync.LockRank.program_instances, .sleepable);
var foreground_instance_id: ?u32 = null;
var foreground_instance_generation: u64 = 0;
var shell_instance_id: ?u32 = null;
var shell_instance_generation: u64 = 0;
var last_display_used: bool = false;
var last_exit_code: i32 = 0;
var output_capture: ?[]u8 = null;
var output_capture_len: usize = 0;
var output_capture_truncated: bool = false;
var input_capture: ?[]const u8 = null;
var input_capture_pos: usize = 0;
var work_drive_letter: u8 = 'C';
var work_cwd: [drive.MAX_PATH]u8 = .{0} ** drive.MAX_PATH;
var work_cwd_len: usize = 1;

var r4xstart_r4sys_table: R4XStartR4Sys = .{};

var r4xstart_r4desk_table: R4XStartR4Desk = .{};

var r4xstart_r4draw_table: R4XStartR4Draw = .{};

// R4NET-Gruppentabelle fuer den R4L-Importvertrag.
var r4xstart_r4net_table: R4XStartR4Net = .{};

var r4xstart_r4audio_table: R4XStartR4Audio = .{};

var r4xstart_r4dev_table: R4XStartR4Dev = .{};

const api_allocator_vtable = ProgramAllocatorVTable{
    .alloc = apiAllocatorAlloc,
    .resize = apiAllocatorResize,
    .remap = apiAllocatorRemap,
    .free = apiAllocatorFree,
};

pub fn initializeRuntime(usable_bytes: u64) void {
    async_io_retire_retry_test_armed = taskRegistryAsyncRetireRetryTestAllowed();
    async_io_retire_retry_test_consumed = false;
    async_io_retire_retry_test_remaining = if (async_io_retire_retry_test_armed) ASYNC_IO_RETIRE_RETRY_TEST_ATTEMPTS else 0;
    async_io_retire_retry_test_request_id = 0;
    configureApiGroups();
    r4api.r4sys.setRuntimeContext(usable_bytes);
    k.setOutputHookIntercept(kprintOutputHook);
    keyboard.setInputHook(keyboardInputHook);
    if (!program_reaper_started) {
        if (task.createKernelThreadCriticalWithRole("r4x-reaper", programReaperMain, .batch) != null) program_reaper_started = true;
    }
}

fn configureApiGroups() void {
    r4api.r4sys.setPathResolver(resolveApiTarget);
    r4api.r4sys.setStreamOwnerResolver(resolveR4SysStreamOwner);
    r4api.r4sys.setProgramModuleRunningProvider(programModuleRunningByPath);
    r4api.r4draw.setDisplayUsedHook(noteDisplayUsed);
    r4api.r4draw.setFontCatalogChangedHook(broadcastGuiFontCatalogChanged);
    configureR4XStartR4SysTable();
    configureR4XStartR4DeskTable();
    configureR4XStartR4DrawTable();
    configureR4XStartR4NetTable();
    configureR4XStartR4AudioTable();
    configureR4XStartR4DevTable();
}

fn configureR4XStartR4SysTable() void {
    r4xstart_r4sys_table = r4x_api.buildR4SysTable(.{
        .write = &r4xstartR4SysWrite,
        .putc = &r4xstartR4SysPutc,
        .sleep_ticks = &r4api.r4sys.sleepTicks,
        .ticks = &r4api.r4sys.ticks,
        .env_get = &apiEnvGet,
        .dir_entry = &r4api.r4sys.dirEntry,
        .program_should_close = &apiProgramShouldClose,
        .program_class = &apiProgramClass,
        .program_instance = &apiProgramInstance,
        .service_status = &apiServiceStatus,
        .service_open = &apiServiceOpen,
        .service_close = &apiServiceClose,
        .service_call = &apiServiceCall,
        .service_endpoint_register = &apiServiceEndpointRegister,
        .service_endpoint_unregister = &apiServiceEndpointUnregister,
        .service_endpoint_poll = &apiServiceEndpointPoll,
        .service_endpoint_recv = &apiServiceEndpointRecv,
        .service_endpoint_reply = &apiServiceEndpointReply,
        .service_detail_by_name = &apiServiceDetailByName,
        .service_start = &apiServiceStart,
        .service_stop = &apiServiceStop,
        .service_restart = &apiServiceRestart,
        .service_install = &apiServiceInstall,
        .service_remove = &apiServiceRemove,
        .vm_reserve = &apiVmReserve,
        .vm_commit = &apiVmCommit,
        .vm_decommit = &apiVmDecommit,
        .vm_release = &apiVmRelease,
        .vm_query = &apiVmQuery,
        .thread_create = &apiThreadCreate,
        .thread_exit = &apiThreadExit,
        .thread_join = &apiThreadJoin,
        .thread_current = &apiThreadCurrent,
        .thread_status = &apiThreadStatus,
        .io_file_read = &apiIoFileRead,
        .io_file_read_at = &apiIoFileReadAt,
        .io_file_write = &apiIoFileWrite,
        .io_file_append = &apiIoFileAppend,
        .io_file_stream_begin = &apiIoFileStreamBegin,
        .io_file_stream_write = &apiIoFileStreamWrite,
        .io_file_stream_finish = &apiIoFileStreamFinish,
        .io_file_stream_abort = &apiIoFileStreamAbort,
        .io_service_call = &apiIoServiceCall,
        .io_status = &apiIoStatus,
        .io_wait = &apiIoWait,
        .io_close = &apiIoClose,
        .task_yield = &r4api.r4sys.taskYield,
        .system_halt = &r4api.r4sys.systemHalt,
        .system_reboot = &r4api.r4sys.systemReboot,
        .system_poweroff = &r4api.r4sys.systemPoweroff,
        .time_state = &r4api.r4sys.timeState,
        .time_set_state = &r4api.r4sys.timeSetState,
        .time_seconds_since_midnight = &r4api.r4sys.timeSecondsSinceMidnight,
        .dir_list = &r4api.r4sys.dirList,
        .dir_create = &r4api.r4sys.dirCreate,
        .dir_delete = &r4api.r4sys.dirDelete,
        .drive_info = &r4api.r4sys.driveInfo,
        .file_info = &r4api.r4sys.fileInfo,
        .file_delete = &r4api.r4sys.fileDelete,
        .file_rename = &r4api.r4sys.fileRename,
        .file_copy = &r4api.r4sys.fileCopy,
        .file_move = &r4api.r4sys.fileMove,
        .file_read = &apiFileRead,
        .file_write = &apiFileWrite,
        .file_read_at = &apiFileReadAt,
        .file_append = &apiFileAppend,
        .file_stream_begin = &apiFileStreamBegin,
        .file_stream_write = &apiFileStreamWrite,
        .file_stream_finish = &apiFileStreamFinish,
        .file_stream_abort = &apiFileStreamAbort,
        .env_set = &apiEnvSet,
        .registry_key_info = &r4api.r4sys.registryKeyInfo,
        .registry_enum_key = &r4api.r4sys.registryEnumKey,
        .registry_enum_value = &r4api.r4sys.registryEnumValue,
        .registry_get_value = &r4api.r4sys.registryGetValue,
        .registry_set_value = &r4api.r4sys.registrySetValue,
        .registry_delete_value = &r4api.r4sys.registryDeleteValue,
        .service_info = &apiServiceInfo,
        .service_detail = &apiServiceDetail,
        .service_set_start_mode = &apiServiceSetStartMode,
        .service_endpoint_wait = &apiServiceEndpointPollWait,
        .program_run = &apiProgramRun,
        .program_launch = &apiProgramLaunch,
        .program_spawn = &apiProgramSpawn,
        .program_kill = &apiProgramKill,
        .program_status = &apiProgramStatus,
        .program_request_close = &apiProgramRequestClose,
        .program_reap_instance = &apiProgramReapInstance,
        .program_spawn_handle = &apiProgramSpawnHandle,
        .program_open_handle = &apiProgramOpenHandle,
        .program_handle_status = &apiProgramHandleStatus,
        .program_handle_request_close = &apiProgramHandleRequestClose,
        .program_handle_kill = &apiProgramHandleKill,
        .program_handle_wait = &apiProgramHandleWait,
        .program_handle_reap = &apiProgramHandleReap,
        .program_completion_read = &apiProgramCompletionRead,
        .program_inventory_begin = &apiProgramInventoryBegin,
        .program_inventory_programs = &apiProgramInventoryPrograms,
        .program_inventory_tasks = &apiProgramInventoryTasks,
        .program_inventory_threads = &apiProgramInventoryThreads,
        .thread_create_handle = &apiThreadCreateHandle,
        .thread_handle_join = &apiThreadHandleJoin,
        .thread_handle_status = &apiThreadHandleStatus,
        .file_replace_atomic = &r4api.r4sys.fileReplaceAtomic,
        .file_delete_if_match = &r4api.r4sys.fileDeleteIfMatch,
        .file_update_atomic_checked = &r4api.r4sys.fileUpdateAtomicChecked,
        .path_names_equal_collated = &r4api.r4sys.pathNamesEqualCollated,
        .file_update_cleanup_checked = &r4api.r4sys.fileUpdateCleanupChecked,
        .file_stream_declare_publish = &r4api.r4sys.fileStreamDeclarePublish,
        .boot_log_info = &r4api.r4sys.bootLogInfo,
        .boot_log_read = &r4api.r4sys.bootLogRead,
        .module_resource_stat = &r4api.r4sys.moduleResourceStat,
        .module_resource_read = &r4api.r4sys.moduleResourceRead,
        .program_module_path = &apiProgramModulePath,
        .program_module_running = &r4api.r4sys.programModuleRunning,
        .monotonic_clock = &r4api.r4sys.monotonicClock,
        .boot_ready = &apiBootReady,
        .registry_snapshot_begin = &r4api.r4sys.registrySnapshotBegin,
        .registry_snapshot_page = &r4api.r4sys.registrySnapshotPage,
        .registry_batch_mutate = &r4api.r4sys.registryBatchMutate,
        .io_file_write_at = &apiIoFileWriteAt,
        .io_file_info = &apiIoFileInfo,
        .io_file_lock = &apiIoFileLock,
    });
}

fn configureR4XStartR4DeskTable() void {
    r4xstart_r4desk_table = r4x_api.buildR4DeskTable(.{
        .read_key = &apiReadKey,
        .read_key_codepoint = &apiReadKeyCodepoint,
        .mouse_state = &r4api.r4desk.mouseState,
        .mouse_show = &r4api.r4desk.mouseShow,
        .mouse_hide = &r4api.r4desk.mouseHide,
        .keyboard_layout_current = &r4api.r4desk.keyboardLayoutCurrent,
        .keyboard_layout_at = &r4api.r4desk.keyboardLayoutAt,
        .keyboard_layout_set = &r4api.r4desk.keyboardLayoutSet,
        .program_set_window = &apiProgramSetWindow,
        .program_set_console_host = &apiProgramSetConsoleHost,
        .program_request_host_launch = &apiProgramRequestHostLaunch,
        .program_take_host_launch = &apiProgramTakeHostLaunch,
        .program_window_id = &apiProgramWindowId,
        .gui_window_info = &apiGuiWindowInfo,
        .gui_set_window_info = &apiGuiSetWindowInfo,
        .gui_poll_event = &apiGuiPollEvent,
        .gui_push_event = &apiGuiPushEvent,
        .gui_set_text = &apiGuiSetText,
        .gui_text = &apiGuiText,
        .gui_revision = &apiGuiRevision,
        .gui_command = &apiGuiCommand,
        .gui_set_title = &apiGuiSetTitle,
        .gui_title = &apiGuiTitle,
        .gui_set_min_size = &apiGuiSetMinSize,
        .gui_min_size = &apiGuiMinSize,
        .console_output = &apiConsoleOutput,
        .console_revision = &apiConsoleRevision,
        .console_state = &apiConsoleState,
        .console_set_metrics = &apiConsoleSetMetrics,
        .console_push_key = &apiConsolePushKey,
        .console_write = &apiConsoleWrite,
        .console_read = &apiConsoleRead,
        .console_input_wait = &apiConsoleInputWait,
        .clipboard_write = &r4api.r4desk.clipboardWrite,
        .clipboard_read = &r4api.r4desk.clipboardRead,
        .clipboard_revision = &r4api.r4desk.clipboardRevision,
        .clipboard_info = &r4api.r4desk.clipboardInfo,
        .clipboard_clear = &r4api.r4desk.clipboardClear,
        .remote_frame_info = &r4api.r4desk.remoteFrameInfo,
        .remote_frame_read = &r4api.r4desk.remoteFrameRead,
        .remote_frame_wait = &r4api.r4desk.remoteFrameWait,
        .remote_frame_publish = &r4api.r4desk.remoteFramePublish,
        .remote_input_push = &r4api.r4desk.remoteInputPush,
        .remote_input_poll = &r4api.r4desk.remoteInputPoll,
        .remote_input_status = &r4api.r4desk.remoteInputStatus,
        .desktop_activity_wait = &r4api.r4desk.desktopActivityWait,
        .remote_frame_map = &r4api.r4desk.remoteFrameMap,
        .program_current_console_host = &apiProgramCurrentConsoleHost,
        .program_request_desktop = &apiProgramRequestDesktop,
        .program_spawn_with_console_host = &apiProgramSpawnWithConsoleHost,
        .program_spawn_with_console_host_handle = &apiProgramSpawnWithConsoleHostHandle,
        .program_set_window_handle = &apiProgramSetWindowHandle,
        .console_push_input = &apiConsolePushInput,
        .remote_frame_acquire = &r4api.r4desk.remoteFrameAcquire,
        .remote_frame_release = &r4api.r4desk.remoteFrameRelease,
        .remote_frame_consumers = &r4api.r4desk.remoteFrameConsumers,
        .remote_frame_publish_regions = &r4api.r4desk.remoteFramePublishRegions,
        .physical_key_poll = &r4api.r4desk.physicalKeyPoll,
    });
}

fn configureR4XStartR4DrawTable() void {
    r4xstart_r4draw_table = r4x_api.buildR4DrawTable(.{
        .screen_width = &r4api.r4draw.screenWidth,
        .screen_height = &r4api.r4draw.screenHeight,
        .clear = &r4api.r4draw.clear,
        .rect = &r4api.r4draw.rect,
        .text = &r4api.r4draw.text,
        .display_revision = &r4api.r4draw.displayRevision,
        .display_begin_frame = &r4api.r4draw.displayBeginFrame,
        .display_begin_frame_rect = &r4api.r4draw.displayBeginFrameRect,
        .display_present = &r4api.r4draw.displayPresent,
        .display_blit_xrgb32 = &r4api.r4draw.displayBlitXrgb32,
        .gui_clear = &apiGuiClear,
        .gui_rect = &apiGuiRect,
        .gui_draw_text = &apiGuiDrawText,
        .gui_draw_text_ex = &apiGuiDrawTextEx,
        .gui_blit = &apiGuiBlit,
        .gui_raster_read = &apiGuiRasterRead,
        .gui_present = &apiGuiPresent,
        .font_count = &r4api.r4draw.fontCount,
        .font_info = &r4api.r4draw.fontInfo,
        .font_measure = &r4api.r4draw.fontMeasure,
        .gui_set_font = &apiGuiSetFont,
        .gui_font = &apiGuiFont,
        .text_font = &r4api.r4draw.textFont,
        .font_reload = &r4api.r4draw.fontReload,
        .font_glyph_row = &r4api.r4draw.fontGlyphRow,
        .gui_blend_alpha8 = &apiGuiBlendAlpha8,
        .gui_frame_begin = &apiGuiFrameBegin,
        .gui_frame_append = &apiGuiFrameAppend,
        .gui_frame_commit = &apiGuiFrameCommit,
        .gui_frame_cancel = &apiGuiFrameCancel,
        .gui_frame_info = &apiGuiFrameInfo,
        .gui_frame_read = &apiGuiFrameRead,
        .display_blit_xrgb32_stride = &r4api.r4draw.displayBlitXrgb32Stride,
        .display_present_regions = &r4api.r4draw.displayPresentRegions,
        .display_present_capabilities = &r4api.r4draw.displayPresentCapabilities,
        .display_present_completion = &r4api.r4draw.displayPresentCompletion,
        .gui_frame_begin_damage = &apiGuiFrameBeginDamage,
        .gui_frame_generation_info = &apiGuiFrameGenerationInfo,
        .gui_frame_generation_read = &apiGuiFrameGenerationRead,
        .font_glyph_bitmap = &r4api.r4draw.fontGlyphBitmap,
        .font_revision = &r4api.r4draw.fontRevision,
    });
}

// Verdrahtung der R4NET-Gruppentabelle mit ihren aktuellen Kernel-
// Implementierungen. Die Tabellenfelder sind der oeffentliche R4L-Ausgang.
fn configureR4XStartR4NetTable() void {
    r4xstart_r4net_table = r4x_api.buildR4NetTable(.{
        .tcp_connect = &r4api.r4net.tcpConnect,
        .tcp_write = &r4api.r4net.tcpWrite,
        .tcp_read = &r4api.r4net.tcpRead,
        .tcp_close = &r4api.r4net.tcpClose,
        .tcp_summary = &r4api.r4net.tcpSummary,
        .tcp_connection = &r4api.r4net.tcpConnection,
        .tcp_echo_listen_once = &r4api.r4net.tcpEchoListenOnce,
        .tcp_accept_read_once = &r4api.r4net.tcpAcceptReadOnce,
        .net_ipv4_send = &r4api.r4net.netIpv4Send,
        .net_ipv4_recv = &r4api.r4net.netIpv4Recv,
        .net_config_get = &r4api.r4net.netConfigGet,
        .net_config_set = &r4api.r4net.netConfigSet,
        .net_dns_resolve = &r4api.r4net.netDnsResolve,
        .net_dns_resolve_server = &r4api.r4net.netDnsResolveServer,
        .net_dhcp_acquire = &r4api.r4net.netDhcpAcquire,
        .net_dhcp_renew = &r4api.r4net.netDhcpRenew,
        .net_dhcp_release = &r4api.r4net.netDhcpRelease,
        .net_dhcp_status = &r4api.r4net.netDhcpStatus,
        .net_detail_get = &r4api.r4net.netDetailGet,
        .net_diag_run = &r4api.r4net.netDiagRun,
        .ipc_open = &r4api.r4net.ipcOpen,
        .ipc_send = &r4api.r4net.ipcSend,
        .ipc_recv = &r4api.r4net.ipcRecv,
        .ipc_poll = &r4api.r4net.ipcPoll,
        .ipc_close = &r4api.r4net.ipcClose,
        .ipc_summary = &r4api.r4net.ipcSummary,
        .ipc_channel = &r4api.r4net.ipcChannel,
        .serial_link_status = &r4api.r4net.serialLinkStatus,
        .serial_link_poll = &r4api.r4net.serialLinkPoll,
        .serial_link_send_message = &r4api.r4net.serialLinkSendMessage,
        .serial_link_host_test = &r4api.r4net.serialLinkHostTest,
        .serial_link_inbox = &r4api.r4net.serialLinkInbox,
        .net_service_request = &r4api.r4net.netServiceRequest,
        .ipc_performance = &r4api.r4net.ipcPerformance,
    });
}

fn configureR4XStartR4AudioTable() void {
    r4xstart_r4audio_table = r4x_api.buildR4AudioTable(.{
        .audio_open_stream = &apiAudioOpenStream,
        .audio_write = &apiAudioWrite,
        .audio_close = &apiAudioClose,
        .audio_set_volume = &apiAudioSetVolume,
        .sid_acquire = &r4api.r4audio.sidAcquire,
        .sid_write_register = &r4api.r4audio.sidWriteRegister,
        .sid_release = &r4api.r4audio.sidRelease,
        .sid_load_data = &r4api.r4audio.sidLoadData,
        .sid_init = &r4api.r4audio.sidInit,
        .sid_play_frame = &r4api.r4audio.sidPlayFrame,
        .sid_stop = &r4api.r4audio.sidStop,
        .sid_model_name = &r4api.r4audio.sidModelName,
        .midi_open_synth = &r4api.r4audio.midiOpenSynth,
        .midi_send = &r4api.r4audio.midiSend,
        .midi_close = &r4api.r4audio.midiClose,
        .opl3_write_register = &r4api.r4audio.opl3WriteRegister,
        .opl3_reset = &r4api.r4audio.opl3Reset,
        .opl3_render_block = &r4api.r4audio.opl3RenderBlock,
        .opl3_stop = &r4api.r4audio.opl3Stop,
    });
}

fn apiAudioOpenStream(rate: u32, channels: u16, format: u16) callconv(.c) i32 {
    const handle = currentProgramHandle() orelse return r4x_api.service_api_result_invalid;
    return audio.openStreamForOwner(.{
        .instance_id = handle.instance_id,
        .generation = handle.generation,
    }, rate, channels, format);
}

fn apiAudioWrite(stream_id: u32, data_ptr: [*]const u8, byte_count: u32) callconv(.c) i32 {
    const handle = currentProgramHandle() orelse return r4x_api.service_api_result_invalid;
    return audio.writeStreamForOwner(.{
        .instance_id = handle.instance_id,
        .generation = handle.generation,
    }, stream_id, data_ptr, byte_count);
}

fn apiAudioClose(stream_id: u32) callconv(.c) i32 {
    const handle = currentProgramHandle() orelse return r4x_api.service_api_result_invalid;
    return audio.closeStreamForOwner(.{
        .instance_id = handle.instance_id,
        .generation = handle.generation,
    }, stream_id);
}

fn apiAudioSetVolume(stream_id: u32, fixed_volume: u32) callconv(.c) i32 {
    const handle = currentProgramHandle() orelse return r4x_api.service_api_result_invalid;
    return audio.setVolumeForOwner(.{
        .instance_id = handle.instance_id,
        .generation = handle.generation,
    }, stream_id, fixed_volume);
}

fn fillProgramInstanceStorageSummary(out: *r4api.r4dev.ProgramInstanceStorageSummary) void {
    const storage = instanceStorageStats();
    out.* = .{
        .core_bytes_per_instance = storage.core_bytes_per_instance,
        .registry_reserved_core_bytes = storage.registry_reserved_core_bytes,
        .live_core_bytes = storage.live_core_bytes,
        .active_instance_bytes = storage.active_instance_bytes,
        .peak_active_instance_bytes = storage.peak_active_instance_bytes,
        .reserved_instance_bytes = storage.reserved_instance_bytes,
        .peak_reserved_instance_bytes = storage.peak_reserved_instance_bytes,
        .current_payload_bytes = storage.current_payload_bytes,
        .peak_payload_bytes = storage.peak_payload_bytes,
        .current_runtime_bytes = storage.current_runtime_bytes,
        .peak_runtime_bytes = storage.peak_runtime_bytes,
        .current_console_bytes = storage.current_console_bytes,
        .peak_console_bytes = storage.peak_console_bytes,
        .current_gui_bytes = storage.current_gui_bytes,
        .peak_gui_bytes = storage.peak_gui_bytes,
        .active_instances = storage.active_instances,
        .active_service_instances = storage.active_service_instances,
        .active_console_instances = storage.active_console_instances,
        .active_gui_instances = storage.active_gui_instances,
        .runtime_payloads = storage.active_runtime_payloads,
        .process_payloads = storage.active_process_payloads,
        .environment_payloads = storage.active_environment_payloads,
        .console_payloads = storage.active_console_payloads,
        .console_output_payloads = storage.active_console_output_payloads,
        .gui_payloads = storage.active_gui_payloads,
        .gui_command_payloads = storage.active_gui_command_payloads,
        .gui_raster_payloads = storage.active_gui_raster_payloads,
        .allocation_attempts = storage.allocation_attempts,
        .payload_allocations = storage.payload_allocations,
        .payload_releases = storage.payload_releases,
        .allocation_failures = storage.allocation_failures,
        .transaction_rollbacks = storage.transaction_rollbacks,
        .owner_mismatches = storage.owner_mismatches,
        .header_errors = storage.header_errors,
        .free_failures = storage.free_failures,
        .quarantined_payloads = storage.quarantined_payloads,
        .quarantined_bytes = storage.quarantined_bytes,
        .current_gui_frame_bytes = storage.current_gui_frame_bytes,
        .peak_gui_frame_bytes = storage.peak_gui_frame_bytes,
        .current_gui_frame_commands = storage.current_gui_frame_commands,
        .peak_gui_frame_commands = storage.peak_gui_frame_commands,
        .current_gui_frame_nodes = storage.current_gui_frame_nodes,
        .peak_gui_frame_nodes = storage.peak_gui_frame_nodes,
        .gui_frame_commits = storage.gui_frame_commits,
        .gui_frame_cancels = storage.gui_frame_cancels,
        .gui_frame_oom_failures = storage.gui_frame_oom_failures,
        .gui_frame_snapshot_reads = storage.gui_frame_snapshot_reads,
    };
}

fn runProgramInstanceStorageSelfTestProvider(out: *r4api.r4dev.ProgramInstanceStorageSelfTestResult) i32 {
    if (instance_storage_self_test_running) {
        out.* = .{};
        return -2;
    }
    instance_storage_self_test_running = true;
    defer instance_storage_self_test_running = false;

    const heap_before = heap.stats();
    const storage_before = instanceStorageStats();
    const passed = runInstanceStorageSelfTest();
    const report = instanceStorageSelfTestReport();
    const storage_after = instanceStorageStats();

    const allocation_delta = storage_after.payload_allocations -| storage_before.payload_allocations;
    const release_delta = storage_after.payload_releases -| storage_before.payload_releases;
    var flags: u32 = 0;
    if (heap_before.capacity_bytes != 0) flags |= r4api.r4dev.program_instance_storage_self_test_flag_heap_ready;
    if (report.storage_baseline_ok) flags |= r4api.r4dev.program_instance_storage_self_test_flag_storage_baseline_restored;
    if (allocation_delta == release_delta) flags |= r4api.r4dev.program_instance_storage_self_test_flag_payload_balance;
    if (storage_after.transaction_rollbacks > storage_before.transaction_rollbacks) flags |= r4api.r4dev.program_instance_storage_self_test_flag_rollback_path;

    out.* = .{
        .cases = report.cases,
        .passed_cases = if (passed) report.cases else report.failed_case -| 1,
        .failed_case = report.failed_case,
        .flags = flags,
        .baseline_payload_reserved_bytes = storage_before.current_payload_bytes,
        .final_payload_reserved_bytes = storage_after.current_payload_bytes,
        .peak_payload_reserved_bytes = report.peak_payload_bytes,
        .allocation_failures_before = storage_before.allocation_failures,
        .allocation_failures_after = storage_after.allocation_failures,
    };
    return if (passed) 1 else -1;
}

fn fillProgramRegistrySummary(out: *r4api.r4dev.ProgramRegistrySummaryV2) void {
    const registry = programRegistryStats();
    out.* = .{
        .chunk_slots = registry.chunk_slots,
        .chunk_count = registry.chunk_count,
        .slot_capacity = registry.slot_capacity,
        .free_slots = registry.free_slots,
        .reserved_slots = registry.reserved_slots,
        .live_slots = registry.live_slots,
        .done_slots = registry.done_slots,
        .retiring_slots = registry.retiring_slots,
        .pinned_slots = registry.pinned_slots,
        .warm_chunks = PROGRAM_REGISTRY_WARM_CHUNKS,
        .last_admission_error = registry.last_admission_error,
        .flags = if (registry.failure_armed) r4api.r4dev.program_registry_summary_flag_failure_armed else 0,
        .peak_chunks = registry.peak_chunks,
        .peak_live = registry.peak_live,
        .growth_attempts = registry.growth_attempts,
        .growth_failures = registry.growth_failures,
        .forced_failures = registry.forced_failures,
        .publish_count = registry.publish_count,
        .rollback_count = registry.rollback_count,
        .shrink_count = registry.shrink_count,
        .id_collisions = registry.id_collisions,
        .id_wraps = registry.id_wraps,
        .live_id_hash = registry.live_id_hash,
        .live_address_hash = registry.live_address_hash,
        .next_generation = registry.next_generation,
        .completion_pending = @intCast(@min(registry.completion_pending, std.math.maxInt(u32))),
        .completion_ready = @intCast(@min(registry.completion_ready, std.math.maxInt(u32))),
        .completion_output_bytes = registry.completion_output_bytes,
        .retire_queued = @intCast(@min(registry.retire_queued, std.math.maxInt(u32))),
        .retire_deferred = @intCast(@min(registry.retire_deferred, std.math.maxInt(u32))),
        .history_head = @intCast(@min(registry.history_head, std.math.maxInt(u32))),
        .history_count = @intCast(@min(registry.history_count, std.math.maxInt(u32))),
        .history_sequence = registry.history_sequence,
        .completion_peak = registry.completion_peak,
        .retire_retries = registry.retire_retries,
    };
}

fn programRegistrySelfTestAllowed() bool {
    const value = boot_config.optionValue(boot_config.get(), "PROGRAMREGISTRY", "selftest") orelse return false;
    return std.ascii.eqlIgnoreCase(value, "yes");
}

fn programLifecycleSelfTestAllowed() bool {
    const value = boot_config.optionValue(boot_config.get(), "PROGRAMLIFECYCLE", "selftest") orelse return false;
    return std.ascii.eqlIgnoreCase(value, "yes");
}

fn runProgramRegistrySelfTestProvider(out: *r4api.r4dev.ProgramRegistrySelfTestResultV2) i32 {
    const operation = out.operation;
    const requested_phase = out.lifecycle_phase;
    const requested_next_id = out.requested_next_id;
    const before = programRegistryStats();
    var flags: u32 = 0;
    var result: i32 = 1;
    var lifecycle_result: i32 = 0;
    var applied_next_id: u32 = 0;

    switch (operation) {
        r4api.r4dev.program_registry_self_test_operation_reset => {
            resetProgramRegistryGrowthFailureForTest();
            configureProgramLifecycleFailureForTest(null);
            flags |= r4api.r4dev.program_registry_self_test_flag_reset;
            if (programRegistrySelfTestAllowed() or programLifecycleSelfTestAllowed()) flags |= r4api.r4dev.program_registry_self_test_flag_allowed;
        },
        r4api.r4dev.program_registry_self_test_operation_arm_next_growth => {
            if (!programRegistrySelfTestAllowed()) {
                flags |= r4api.r4dev.program_registry_self_test_flag_denied;
                result = -1;
            } else {
                flags |= r4api.r4dev.program_registry_self_test_flag_allowed |
                    r4api.r4dev.program_registry_self_test_flag_one_shot;
                if (armProgramRegistryNextGrowthFailureForTest()) {
                    flags |= r4api.r4dev.program_registry_self_test_flag_armed;
                } else {
                    flags |= r4api.r4dev.program_registry_self_test_flag_busy;
                    result = -2;
                }
            }
        },
        r4api.r4dev.program_registry_self_test_operation_arm_lifecycle_failure => {
            if (!programLifecycleSelfTestAllowed() or requested_phase == 0 or requested_phase > r4api.r4dev.program_registry_self_test_phase_slot_reclaim) {
                flags |= r4api.r4dev.program_registry_self_test_flag_denied;
                lifecycle_result = PROGRAM_HANDLE_ERROR_INVALID;
                result = -1;
            } else {
                flags |= r4api.r4dev.program_registry_self_test_flag_allowed |
                    r4api.r4dev.program_registry_self_test_flag_one_shot;
                const phase: ProgramLifecycleFailurePhase = @enumFromInt(requested_phase);
                if (armProgramLifecycleFailureForTest(phase)) {
                    flags |= r4api.r4dev.program_registry_self_test_flag_lifecycle_armed;
                    lifecycle_result = PROGRAM_HANDLE_OK;
                } else {
                    flags |= r4api.r4dev.program_registry_self_test_flag_busy;
                    lifecycle_result = PROGRAM_HANDLE_ERROR_WOULD_BLOCK;
                    result = -2;
                }
            }
        },
        r4api.r4dev.program_registry_self_test_operation_signal_reaper => {
            if (!programLifecycleSelfTestAllowed()) {
                flags |= r4api.r4dev.program_registry_self_test_flag_denied;
                lifecycle_result = PROGRAM_HANDLE_ERROR_INVALID;
                result = -1;
            } else {
                flags |= r4api.r4dev.program_registry_self_test_flag_allowed;
                signalProgramReaperForTest();
                lifecycle_result = PROGRAM_HANDLE_OK;
            }
        },
        r4api.r4dev.program_registry_self_test_operation_force_next_id => {
            if (!programLifecycleSelfTestAllowed() or requested_next_id == 0) {
                flags |= r4api.r4dev.program_registry_self_test_flag_denied;
                lifecycle_result = PROGRAM_HANDLE_ERROR_INVALID;
                result = -1;
            } else {
                flags |= r4api.r4dev.program_registry_self_test_flag_allowed;
                configureProgramRegistryNextIdForTest(requested_next_id);
                applied_next_id = requested_next_id;
                lifecycle_result = PROGRAM_HANDLE_OK;
            }
        },
        else => result = -3,
    }

    const after = programRegistryStats();
    const lifecycle = programLifecycleFailureSnapshot();
    if (lifecycle.armed) flags |= r4api.r4dev.program_registry_self_test_flag_lifecycle_armed;
    if (lifecycle.consumed) flags |= r4api.r4dev.program_registry_self_test_flag_lifecycle_consumed;
    if (lifecycle.reaper_signalled) flags |= r4api.r4dev.program_registry_self_test_flag_reaper_signalled;
    if (lifecycle.retried) flags |= r4api.r4dev.program_registry_self_test_flag_lifecycle_retried;
    if (lifecycle.recovered) flags |= r4api.r4dev.program_registry_self_test_flag_lifecycle_recovered;
    out.* = .{
        .operation = operation,
        .flags = flags,
        .chunk_count_before = before.chunk_count,
        .slot_capacity_before = before.slot_capacity,
        .free_slots_before = before.free_slots,
        .growth_failures_before = before.growth_failures,
        .growth_failures_after = after.growth_failures,
        .forced_failures_before = before.forced_failures,
        .forced_failures_after = after.forced_failures,
        .lifecycle_phase = if (requested_phase != 0) requested_phase else lifecycle.phase,
        .lifecycle_result = lifecycle_result,
        .retire_queued_before = @intCast(@min(before.retire_queued, std.math.maxInt(u32))),
        .retire_queued_after = @intCast(@min(after.retire_queued, std.math.maxInt(u32))),
        .completion_pending_before = @intCast(@min(before.completion_pending, std.math.maxInt(u32))),
        .completion_pending_after = @intCast(@min(after.completion_pending, std.math.maxInt(u32))),
        .completion_ready_before = @intCast(@min(before.completion_ready, std.math.maxInt(u32))),
        .completion_ready_after = @intCast(@min(after.completion_ready, std.math.maxInt(u32))),
        .retire_retries_before = before.retire_retries,
        .retire_retries_after = after.retire_retries,
        .history_sequence_before = before.history_sequence,
        .history_sequence_after = after.history_sequence,
        .requested_next_id = requested_next_id,
        .applied_next_id = applied_next_id,
    };
    return result;
}

fn fillInputPerformanceInfo(out: *r4api.r4dev.ProgramInputPerformanceInfo) void {
    var gui_pending: u64 = 0;
    var console_pending: u64 = 0;
    var gui_active: u32 = 0;
    var console_active: u32 = 0;
    var gui_high_water: u32 = 0;
    var console_high_water: u32 = 0;
    var attach_pending: u32 = 0;

    if (lockProgramRegistry()) {
        var iterator = programRegistryIterator(false);
        while (iterator.next()) |instance| {
            if (instance.gui_payload) |gui| {
                gui_active +|= 1;
                gui_pending +|= guiEventPendingCount(gui);
                gui_high_water = @max(gui_high_water, gui.event_high_water);
                if (gui.start_attach_pending) attach_pending +|= 1;
            }
            if (instance.console_payload) |console| {
                console_active +|= 1;
                console_pending +|= console.state.stdin_pending;
                console_high_water = @max(console_high_water, console.input_high_water);
            }
        }
        unlockProgramRegistry();
    }

    out.gui_queue_capacity = @intCast(GUI_EVENT_QUEUE_SIZE - 1);
    out.gui_queue_pending = @intCast(@min(gui_pending, std.math.maxInt(u32)));
    out.gui_queue_high_water = gui_high_water;
    out.gui_queue_active = gui_active;
    out.console_queue_capacity = @intCast(INPUT_QUEUE_SIZE - 1);
    out.console_queue_pending = @intCast(@min(console_pending, std.math.maxInt(u32)));
    out.console_queue_high_water = console_high_water;
    out.console_queue_active = console_active;
    out.program_start_attach_pending = attach_pending;
    out.gui_push_attempts = gui_event_push_attempts;
    out.gui_accepted = gui_event_accepted;
    out.gui_mouse_move_coalesced = gui_mouse_move_coalesced;
    out.gui_mouse_move_evicted = gui_mouse_move_evicted;
    out.gui_rejected = gui_event_rejected;
    out.console_push_calls = console_input_push_calls;
    out.console_batch_calls = console_input_batch_calls;
    out.console_bytes_attempted = console_input_bytes_attempted;
    out.console_bytes_accepted = console_input_bytes_accepted;
    out.console_full_events = console_input_full_events;
    out.console_read_calls = console_read_calls;
    out.console_read_empty = console_read_empty;
    out.console_read_bytes = console_read_bytes;
    out.console_wait_calls = console_wait_calls;
    out.console_wait_blocks = console_wait_blocks;
    out.console_wait_immediate = console_wait_immediate;
    out.console_wait_wakes = console_wait_wakes;
    out.console_wait_timeouts = console_wait_timeouts;
    out.console_wait_cancellations = console_wait_cancellations;
    out.console_output_write_calls = console_output_write_calls;
    out.console_output_source_bytes = console_output_source_bytes;
    out.console_output_visible_append_bytes = console_output_visible_append_bytes;
    out.console_output_capture_append_bytes = console_output_capture_append_bytes;
    out.console_output_shared_bytes = console_output_shared_bytes;
    out.console_output_revision_batches = console_output_revision_batches;
    out.console_output_desktop_signals = console_output_desktop_signals;
    out.console_output_compactions = console_output_compactions;
    out.console_output_compaction_bytes = console_output_compaction_bytes;
    out.console_output_segment_drops = console_output_segment_drops;
    out.console_output_segment_drop_bytes = console_output_segment_drop_bytes;
    out.program_launch_attempts = program_launch_attempts;
    out.program_entries_started = program_entries_started;
    out.program_attach_wait_events = program_attach_wait_events;
}

fn configureR4XStartR4DevTable() void {
    r4api.r4dev.setProgramInstanceStorageSummaryProvider(&fillProgramInstanceStorageSummary);
    r4api.r4dev.setProgramInstanceStorageSelfTestProvider(&runProgramInstanceStorageSelfTestProvider);
    r4api.r4dev.setProgramRegistrySummaryProvider(&fillProgramRegistrySummary);
    r4api.r4dev.setProgramRegistrySelfTestProvider(&runProgramRegistrySelfTestProvider);
    r4api.r4dev.setExecutionInventorySummaryProvider(&fillExecutionInventorySummaryProvider);
    r4api.r4dev.setInputPerformanceProvider(&fillInputPerformanceInfo);
    r4xstart_r4dev_table = r4x_api.buildR4DevTable(.{
        .device_inventory_summary = &r4api.r4dev.deviceInventorySummary,
        .device_inventory_record = &r4api.r4dev.deviceInventoryRecord,
        .memory_summary = &r4api.r4dev.memorySummary,
        .memory_block_count = &r4api.r4dev.memoryBlockCount,
        .memory_block = &r4api.r4dev.memoryBlock,
        .memory_pressure_snapshot = &r4api.r4dev.memoryPressureSnapshot,
        .memory_reclaim_probe = &r4api.r4dev.memoryReclaimProbe,
        .memory_backing_store_probe = &r4api.r4dev.memoryBackingStoreProbe,
        .memory_backing_store_slot_probe = &r4api.r4dev.memoryBackingStoreSlotProbe,
        .memory_pager_gate_probe = &r4api.r4dev.memoryPagerGateProbe,
        .memory_page_io_probe = &r4api.r4dev.memoryPageIoProbe,
        .memory_vm_page_state_probe = &r4api.r4dev.memoryVmPageStateProbe,
        .memory_vm_reserve_probe = &apiMemoryVmReserveProbe,
        .paging_summary = &r4api.r4dev.pagingSummary,
        .performance_summary = &r4api.r4dev.performanceSummary,
        .performance_task = &r4api.r4dev.performanceTask,
        .performance_storage = &r4api.r4dev.performanceStorage,
        .performance_boot_phase = &r4api.r4dev.performanceBootPhase,
        .protocol_status = &r4api.r4dev.protocolStatus,
        .protocol_dispatch = &r4api.r4dev.protocolDispatch,
        .display_summary = &r4api.r4dev.displaySummary,
        .hardware_summary = &r4api.r4dev.hardwareSummary,
        .boot_info_summary = &r4api.r4dev.bootInfoSummary,
        .boot_info_memory_count = &r4api.r4dev.bootInfoMemoryCount,
        .boot_info_memory_entry = &r4api.r4dev.bootInfoMemoryEntry,
        .program_instance_storage_summary = &r4api.r4dev.programInstanceStorageSummary,
        .program_instance_storage_self_test = &r4api.r4dev.programInstanceStorageSelfTest,
        .program_registry_summary = &r4api.r4dev.programRegistrySummary,
        .program_registry_self_test = &r4api.r4dev.programRegistrySelfTest,
        .program_registry_summary_v2 = &r4api.r4dev.programRegistrySummaryV2,
        .program_registry_self_test_v2 = &r4api.r4dev.programRegistrySelfTestV2,
        .execution_inventory_summary = &r4api.r4dev.executionInventorySummary,
        .program_instance_storage_summary_v2 = &r4api.r4dev.programInstanceStorageSummaryV2,
        .kernel_version = &r4api.r4dev.kernelVersion,
        .performance_boot_phase_clock = &r4api.r4dev.performanceBootPhaseClock,
        .performance_irq_timing = &r4api.r4dev.performanceIrqTiming,
        .performance_boot_summary = &r4api.r4dev.performanceBootSummary,
        .performance_driver_work = &r4api.r4dev.performanceDriverWork,
        .performance_pci_inventory = &r4api.r4dev.performancePciInventory,
        .performance_input = &r4api.r4dev.performanceInput,
    });
}

pub fn runPath(d: *drive.Drive, path: []const u8, args: []const u8, working_drive: *drive.Drive) RunResult {
    const file = resolveProgramFile(d, path) orelse return .not_found;
    return runProgramFile(file, .foreground, .auto, args, working_drive, .none, .{
        .owner = true,
        .owner_handle = ProgramProcessHandle{},
    });
}

pub fn runShellPath(d: *drive.Drive, path: []const u8, args: []const u8, working_drive: *drive.Drive) RunResult {
    return runShellPathWithHost(d, path, args, working_drive, .none);
}

pub fn runShellPathWithHost(d: *drive.Drive, path: []const u8, args: []const u8, working_drive: *drive.Drive, host: ConsoleHostKind) RunResult {
    const report_boot_launch = bootscreen.isActive();
    reportBootLaunchStage(report_boot_launch, "Pfad pruefen");
    const file = resolveProgramFileWithBootReport(d, path, report_boot_launch) orelse return .not_found;
    reportBootLaunchStage(report_boot_launch, "Start vorbereiten");
    return runProgramFile(file, .shell, .auto, args, working_drive, host, .{
        .report_boot_launch = report_boot_launch,
    });
}

pub fn activeShellInstanceId() u32 {
    return shell_instance_id orelse 0;
}

// 0.56.3: Der fruehere program_spawn_lock um das komplette runProgramFile
// ist entfernt. Er schuetzte de facto nur die Owner-ID-Vergabe ueber die
// yieldende Lade-Phase - das erledigt jetzt reserveProgramInstanceSlot() VOR
// dem ersten R4X-Datei-I/O. Die Commit-Phase (createInstance/Task-Create/
// registerMainThread) ist yield-frei und damit im kooperativen Modell
// atomar; Handle und exakte Fehlerklasse werden ueber caller-eigene
// ProgramLaunchOptions-Ausgaben transportiert. Der Lock serialisierte sonst
// ALLE Programm-Starts ueber die gesamte Ladezeit (MEMSUITE pagerstress
// sleep_under_lock, Report-Befund; Regression aus 0.55.46).
pub fn spawnPath(d: *drive.Drive, path: []const u8, args: []const u8, working_drive: *drive.Drive) i32 {
    const file = resolveProgramFile(d, path) orelse return -1;
    var handle: ProgramProcessHandle = .{};
    return switch (runProgramFile(file, .background, .auto, args, working_drive, .none, .{
        .owner = true,
        .owner_handle = ProgramProcessHandle{},
        .legacy_id = true,
        .out_handle = &handle,
    })) {
        .ran => @intCast(handle.instance_id),
        .not_found => -1,
        .failed => -2,
    };
}

pub fn programClassIdForPath(d: *drive.Drive, path: []const u8) i32 {
    const file = resolveProgramFile(d, path) orelse return -1;
    const app_class = classifyProgramFile(file, .auto) orelse return -2;
    return switch (app_class) {
        .console => 1,
        .gui => 2,
        .service => 3,
    };
}

fn programModuleRunningByPath(drive_letter: u8, path: []const u8) i32 {
    const locked = lockProgramRegistry();
    if (!locked) return r4api.r4sys.program_module_running_error_unavailable;
    defer unlockProgramRegistry();
    var chunk = program_registry_head;
    while (chunk) |current| : (chunk = current.next) {
        for (&current.slots) |*slot| {
            // `publish` already owns a loaded image and `exit` may still be
            // executing its final unwind. Done/retire instances no longer
            // execute application code and do not block a live replacement.
            if (slot.state != .publish and slot.state != .run and slot.state != .exit) continue;
            const runtime = slot.instance.runtime_payload orelse continue;
            const origin_len: usize = @intCast(runtime.module_path_len);
            if (origin_len > runtime.module_path.len) continue;
            if (moduleOriginMatchesResolved(
                runtime.module_path[0..origin_len],
                drive_letter,
                path,
            )) return r4api.r4sys.program_module_running_result_running;
        }
    }
    return r4api.r4sys.program_module_running_result_idle;
}

fn moduleOriginMatchesResolved(origin: []const u8, drive_letter: u8, path: []const u8) bool {
    if (origin.len != path.len + 2 or origin.len < 3 or origin[1] != ':') return false;
    if (std.ascii.toUpper(origin[0]) != std.ascii.toUpper(drive_letter)) return false;
    for (origin[2..], path) |left_raw, right_raw| {
        const left = if (left_raw == '/') '\\' else std.ascii.toUpper(left_raw);
        const right = if (right_raw == '/') '\\' else std.ascii.toUpper(right_raw);
        if (left != right) return false;
    }
    return true;
}

pub fn requestCloseInstance(id: u32) i32 {
    var handle: ProgramProcessHandle = .{};
    if (apiProgramOpenHandle(id, &handle) != PROGRAM_HANDLE_OK) return -1;
    return if (apiProgramHandleRequestClose(&handle) == PROGRAM_HANDLE_OK) 0 else -2;
}

pub fn killInstance(id: u32) i32 {
    var handle: ProgramProcessHandle = .{};
    if (apiProgramOpenHandle(id, &handle) != PROGRAM_HANDLE_OK) return -1;
    return if (apiProgramHandleKill(&handle) == PROGRAM_HANDLE_OK) 0 else -2;
}

pub fn instanceSnapshot(id: u32) ?InstanceSnapshot {
    const instance = instanceById(id) orelse return null;
    return .{
        .id = instance.id,
        .task_id = instance.task_id,
        .app_class = @intFromEnum(instance.app_class),
        .state = @intFromEnum(instanceState(instance)),
        .close_requested = instance.close_requested,
        .exit_code = instance.exit_code,
    };
}

pub fn takeExitCode(id: u32) ?i32 {
    const locked = lockProgramRegistry();
    if (!locked) return null;
    defer unlockProgramRegistry();
    var age: usize = 0;
    while (age < exit_history_count) : (age += 1) {
        const index = (exit_history_head + exit_history.len - 1 - age) % exit_history.len;
        const record = exit_history[index];
        if (record.used and record.handle.instance_id == id) return record.exit_code;
    }
    return null;
}

fn resolveProgramFile(d: *drive.Drive, path: []const u8) ?ProgramFile {
    return resolveProgramFileWithBootReport(d, path, false);
}

fn resolveProgramFileWithBootReport(d: *drive.Drive, path: []const u8, report_boot_launch: bool) ?ProgramFile {
    const volume = vfs.volumeForDrive(d.letter) orelse return null;
    reportBootLaunchStage(report_boot_launch, "FS-Sperre pruefen");
    var req = fs_request.tryBegin(.loader_read, d.letter) orelse blk: {
        // Preserve the normal bounded wait after an immediate, observation-
        // only probe.  The persistent marker distinguishes a foreign volume
        // owner from a stall in the following path lookup on real hardware.
        reportBootFilesystemWait(report_boot_launch, d.letter);
        break :blk fs_request.begin(.loader_read, d.letter) orelse return null;
    };
    var ok = false;
    defer fs_request.finish(&req, ok);
    reportBootLaunchStage(report_boot_launch, "Dateipfad suchen");
    const entry = vfs.resolveEntry(volume, path) orelse return null;
    if (entry.isDir()) return null;
    if (entry.size == 0) {
        k.puts("Bad R4X file\r\n");
        return null;
    }
    ok = true;
    var resolved = ProgramFile{ .volume = volume, .entry = entry, .drive_letter = d.letter };
    var origin_len: usize = 0;
    if (path.len + 3 <= resolved.origin.len) {
        resolved.origin[0] = d.letter;
        resolved.origin[1] = ':';
        origin_len = 2;
        if (path.len == 0 or (path[0] != '/' and path[0] != 92)) {
            resolved.origin[2] = 92;
            origin_len = 3;
        }
        for (path) |byte| {
            resolved.origin[origin_len] = if (byte == '/') 92 else byte;
            origin_len += 1;
        }
    }
    resolved.origin_len = @intCast(origin_len);
    return resolved;
}

fn reportBootFilesystemWait(enabled: bool, drive_letter: u8) void {
    if (!enabled or !bootscreen.isActive()) return;
    const snapshot = fs_request.gateSnapshot(drive_letter) orelse {
        reportBootLaunchStage(enabled, "FS-Sperre wartet");
        return;
    };
    const owner_task = task.pinByIdentity(snapshot.owner_task_id, snapshot.owner_task_generation) orelse {
        bootscreen.setDetail("FS-Halter fehlt");
        bootlog.puts("[FSGATE] desktop wait owner missing task=");
        bootlog.putDec(snapshot.owner_task_id);
        bootlog.puts(" generation=");
        bootlog.putDec(snapshot.owner_task_generation);
        bootlog.puts(" kind=");
        bootlog.putDec(@intFromEnum(snapshot.kind));
        bootlog.puts("\r\n");
        return;
    };
    defer _ = task.unpin(owner_task);

    var owner_name = owner_task.name;
    var activity = filesystemKindLabel(snapshot.kind);
    const execution_owner = task.executionOwner(owner_task);
    switch (execution_owner.kind) {
        .program_thread => {
            if (execution_owner.context) |context| {
                const thread_ctx: *ProgramThread = @ptrCast(@alignCast(context));
                if (thread_ctx.used and
                    thread_ctx.task_id == snapshot.owner_task_id and
                    thread_ctx.task_generation == snapshot.owner_task_generation)
                {
                    if (programModuleBaseName(thread_ctx.owner_instance)) |name| owner_name = name;
                }
            }
        },
        .async_io => {
            if (execution_owner.context) |context| {
                const request: *AsyncIoRequest = @ptrCast(@alignCast(context));
                if (request.used and
                    request.task_id == snapshot.owner_task_id and
                    request.task_generation == snapshot.owner_task_generation)
                {
                    if (programModuleBaseName(request.owner_instance)) |name| owner_name = name;
                    if (request.path_len != 0 and request.path_len <= request.path.len) {
                        const file_name = baseName(request.path[0..request.path_len]);
                        if (file_name.len != 0) activity = file_name;
                    }
                }
            }
        },
        .none => {},
    }
    if (owner_task.state == .blocked and owner_task.wait_reason.len != 0) {
        activity = owner_task.wait_reason;
    }

    bootscreen.setServiceStage(owner_name, activity);
    bootlog.puts("[FSGATE] desktop wait owner_task=");
    bootlog.putDec(snapshot.owner_task_id);
    bootlog.puts(" owner_generation=");
    bootlog.putDec(snapshot.owner_task_generation);
    bootlog.puts(" kind=");
    bootlog.putDec(@intFromEnum(snapshot.kind));
    bootlog.puts(" owner=");
    bootlog.puts(owner_name);
    bootlog.puts(" activity=");
    bootlog.puts(activity);
    bootlog.puts("\r\n");
}

fn programModuleBaseName(instance: ?*const ProgramInstance) ?[]const u8 {
    const owner = instance orelse return null;
    const runtime = owner.runtime_payload orelse return null;
    const path_len: usize = @intCast(runtime.module_path_len);
    if (path_len == 0 or path_len > runtime.module_path.len) return null;
    const name = baseName(runtime.module_path[0..path_len]);
    return if (name.len == 0) null else name;
}

fn filesystemKindLabel(kind: fs_request.Kind) []const u8 {
    return switch (kind) {
        .drive_info, .dir_list, .dir_entry, .file_info => "Metadaten",
        .file_read, .file_read_at, .loader_read, .config_read => "lesen",
        .file_write, .file_write_at, .file_append, .config_write => "schreiben",
        .stream_begin => "Stream beginnen",
        .stream_write => "Stream schreiben",
        .stream_finish => "Stream beenden",
        .stream_abort => "Stream abbrechen",
        .file_delete, .file_delete_if_match => "Datei loeschen",
        .dir_create => "Ordner anlegen",
        .dir_delete => "Ordner loeschen",
        .file_rename => "umbenennen",
        .file_copy => "kopieren",
        .file_move => "verschieben",
        .file_replace_atomic, .file_update_atomic_checked => "Update schreiben",
    };
}

pub fn usedDisplay() bool {
    return last_display_used;
}

pub fn lastExitCode() i32 {
    return last_exit_code;
}

pub fn beginOutputCapture(buffer: []u8) void {
    output_capture = buffer;
    output_capture_len = 0;
    output_capture_truncated = false;
}

pub fn endOutputCapture() OutputCaptureResult {
    const result = OutputCaptureResult{ .len = output_capture_len, .truncated = output_capture_truncated };
    output_capture = null;
    output_capture_len = 0;
    output_capture_truncated = false;
    return result;
}

pub fn beginInputCapture(data: []const u8) void {
    input_capture = data;
    input_capture_pos = 0;
}

pub fn endInputCapture() void {
    input_capture = null;
    input_capture_pos = 0;
}

const RunMode = enum {
    foreground,
    shell,
    background,
};

const ProgramLaunchOptions = struct {
    owner: bool = false,
    // null binds an owned completion to the current R4X caller.  An explicit
    // zero handle is the persistent kernel/external owner used by internal
    // foreground waits and the service manager.
    owner_handle: ?ProgramProcessHandle = null,
    legacy_id: bool = false,
    retain_output: bool = false,
    out_handle: ?*ProgramProcessHandle = null,
    // Handle-spawn callers own this result cell for the complete yielding
    // launch transaction. Internal/legacy callers leave it null and retain
    // the established RunResult-only contract.
    error_out: ?*i32 = null,
    report_boot_launch: bool = false,
};

const SubsystemTrace = struct {
    id: []const u8,
    mode: []const u8,
};

const SubsystemTraceRecord = struct {
    key: []const u8,
    value: []const u8,
};

fn parseSubsystemTrace(args: []const u8) ?SubsystemTrace {
    const trace_magic = "R4SUBSYS1";
    if (!std.mem.startsWith(u8, args, trace_magic)) return null;
    var remaining = args[trace_magic.len..];
    var trace_id: ?[]const u8 = null;
    var trace_mode: []const u8 = "?";
    while (remaining.len != 0) {
        const record = takeSubsystemTraceRecord(&remaining) orelse return null;
        if (std.ascii.eqlIgnoreCase(record.key, "T")) {
            if (trace_id != null or record.value.len != 16) return null;
            for (record.value) |byte| if (!std.ascii.isHex(byte)) return null;
            trace_id = record.value;
        } else if (std.ascii.eqlIgnoreCase(record.key, "M")) {
            trace_mode = record.value;
        }
    }
    return .{ .id = trace_id orelse return null, .mode = trace_mode };
}

fn takeSubsystemTraceRecord(remaining: *[]const u8) ?SubsystemTraceRecord {
    if (remaining.*.len == 0 or remaining.*[0] != ';') return null;
    var cursor = remaining.*[1..];
    const key_len = takeSubsystemTraceLength(&cursor) orelse return null;
    if (key_len == 0 or key_len > cursor.len) return null;
    const key = cursor[0..key_len];
    cursor = cursor[key_len..];
    if (cursor.len == 0 or cursor[0] != '=') return null;
    cursor = cursor[1..];
    const value_len = takeSubsystemTraceLength(&cursor) orelse return null;
    if (value_len > cursor.len) return null;
    const value = cursor[0..value_len];
    remaining.* = cursor[value_len..];
    return .{ .key = key, .value = value };
}

fn takeSubsystemTraceLength(cursor: *[]const u8) ?usize {
    var length: usize = 0;
    var digits: usize = 0;
    while (digits < cursor.*.len and cursor.*[digits] >= '0' and cursor.*[digits] <= '9') : (digits += 1) {
        if (length > 127) return null;
        length = length * 10 + cursor.*[digits] - '0';
    }
    if (digits == 0 or digits >= cursor.*.len or cursor.*[digits] != ':') return null;
    cursor.* = cursor.*[digits + 1 ..];
    return length;
}

fn traceNowNanoseconds() u64 {
    return time_core.monotonicNanoseconds() orelse timer.eventNanoseconds();
}

fn logSubsystemTracePhase(trace: SubsystemTrace, phase: []const u8, now_ns: u64) void {
    k.puts("[R4BASIC-LAUNCH] id=");
    k.puts(trace.id);
    k.puts(" mode=");
    k.puts(trace.mode);
    k.puts(" phase=");
    k.puts(phase);
    k.puts(" ns=");
    k.putDec(now_ns);
    k.puts("\r\n");
}

fn logSubsystemLoaderTrace(
    trace: SubsystemTrace,
    start_ns: u64,
    module_before: module_file.Stats,
    fs_before: fs_request.Summary,
    loaded: *const LoadedProgram,
) void {
    const now_ns = traceNowNanoseconds();
    const module_after = module_file.stats();
    const fs_after = fs_request.summary();
    k.puts("[R4BASIC-LAUNCH] id=");
    k.puts(trace.id);
    k.puts(" mode=");
    k.puts(trace.mode);
    k.puts(" phase=loader-complete ns=");
    k.putDec(now_ns);
    k.puts(" duration_ns=");
    k.putDec(now_ns -| start_ns);
    k.puts(" range_reads=");
    k.putDec(module_after.range_reads -| module_before.range_reads);
    k.puts(" fs_requests=");
    k.putDec(fs_after.requests -| fs_before.requests);
    k.puts(" gate_waits=");
    k.putDec(fs_after.lock_contention_waits -| fs_before.lock_contention_waits);
    k.puts(" fs_ticks=");
    k.putDec(fs_after.total_ticks -| fs_before.total_ticks);
    k.puts(" sections=");
    k.putDec(loaded.loader_section_count);
    k.puts(" imports=");
    k.putDec(loaded.import_count);
    k.puts(" relocations=");
    k.putDec(loaded.loader_relocation_count);
    k.puts("\r\n");
}

fn setProgramLaunchError(options: ProgramLaunchOptions, status: i32) void {
    if (options.error_out) |error_out| error_out.* = status;
    if (status == PROGRAM_HANDLE_OK) return;
    const locked = lockProgramRegistry();
    if (!locked) return;
    program_registry_stats.launch_failures +|= 1;
    program_registry_stats.last_launch_error = status;
    unlockProgramRegistry();
}

fn leaveProgramSpawnTransaction(thread_ctx: ?*ProgramThread) void {
    const thread = thread_ctx orelse return;
    if (thread.spawn_transaction_depth == 0) return;
    thread.spawn_transaction_depth -= 1;
    if (thread.spawn_transaction_depth != 0 or !thread.exit_deferred) return;
    const handle = ProgramProcessHandle{
        .instance_id = thread.instance_id,
        .reserved = 0,
        .generation = thread.instance_generation,
    };
    // The owner may have been killed before a child reached Publish. Re-run
    // orphan/cascade closure after every externally anchored spawn rollback or
    // commit, then release the execution pin exactly once and terminate.
    orphanOwnedProgramCompletions(handle);
    killConsoleClients(handle);
    unpinProgramThreadExecution(thread);
    program_reaper_event.signal();
    scheduler.exitCurrentAndRetire();
}

fn programSpawnTransactionCancelled() bool {
    const thread = currentProgramThread() orelse return false;
    return thread.exit_deferred;
}

fn runProgramFile(file: ProgramFile, mode: RunMode, policy: LaunchPolicy, args: []const u8, working_drive: *drive.Drive, shell_host: ConsoleHostKind, options: ProgramLaunchOptions) RunResult {
    program_launch_attempts +%= 1;
    const subsystem_trace = parseSubsystemTrace(args);
    var loader_start_ns: u64 = 0;
    var module_stats_before: module_file.Stats = undefined;
    var fs_stats_before: fs_request.Summary = undefined;
    if (subsystem_trace) |trace| {
        loader_start_ns = traceNowNanoseconds();
        module_stats_before = module_file.stats();
        fs_stats_before = fs_request.summary();
        logSubsystemTracePhase(trace, "admission", loader_start_ns);
    }
    reportBootLaunchStage(options.report_boot_launch, "API vorbereiten");
    configureApiGroups();
    setProgramLaunchError(options, PROGRAM_HANDLE_OK);
    const spawn_thread = currentProgramThread();
    if (spawn_thread) |thread| {
        if (thread.spawn_transaction_depth == std.math.maxInt(u32)) {
            setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_TASK_FAILED);
            return .failed;
        }
        thread.spawn_transaction_depth += 1;
    }
    defer leaveProgramSpawnTransaction(spawn_thread);
    if (options.out_handle) |out_handle| out_handle.* = .{};
    if (mode != .background) last_display_used = false;
    // 0.59.8: Slot und kollisionssichere ID werden vor dem ersten R4X-Read
    // reserviert. Die Registry zeigt den Eintrag bis publishProgramInstance
    // nicht; der defer deckt jeden fruehen Loader-/Stackfehler ab.
    reportBootLaunchStage(options.report_boot_launch, "Slot reservieren");
    var reservation = switch (reserveProgramInstanceSlot()) {
        .reservation => |value| value,
        .failure => |failure| {
            k.puts("Program registry allocation failed\r\n");
            setProgramLaunchError(options, switch (failure) {
                .no_memory => PROGRAM_HANDLE_ERROR_NO_MEMORY,
                .generation_exhausted => PROGRAM_HANDLE_ERROR_GENERATION_EXHAUSTED,
            });
            return .failed;
        },
    };
    var reservation_active = true;
    defer if (reservation_active) cancelProgramInstanceReservation(&reservation);
    reportBootLaunchStage(options.report_boot_launch, "Completion anlegen");
    if (consumeProgramLifecycleFailure(.completion_reserve) or !allocateProgramCompletionNode(&reservation, options.owner, options.owner_handle, options.legacy_id, options.retain_output)) {
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_NO_MEMORY);
        return .failed;
    }
    const instance_id = reservation.id;
    reportBootLaunchStage(options.report_boot_launch, "Header laden");
    const source = programModuleFileSource(file);
    const file_size: usize = @intCast(file.entry.size);
    var module_reader = module_r4m.Reader.init(source, file_size);
    const r4m_header = readR4MProgramHeaderFromReader(&module_reader, "r4x-image-header", true) orelse {
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_LOAD_FAILED);
        return .failed;
    };
    if (programSpawnTransactionCancelled()) {
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_TASK_FAILED);
        return .failed;
    }
    const app_class = resolveAppClass(policy, r4m_header.flags);
    if (shell_host != .none and app_class != .console) {
        k.puts("Console host requires a console program\r\n");
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_INVALID);
        return .failed;
    }
    reportBootLaunchStage(options.report_boot_launch, "Image laden");
    var loaded = loadR4MProgramImage(file, instance_id, app_class, &module_reader, r4m_header) orelse {
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_LOAD_FAILED);
        return .failed;
    };
    if (subsystem_trace) |trace| logSubsystemLoaderTrace(
        trace,
        loader_start_ns,
        module_stats_before,
        fs_stats_before,
        &loaded,
    );
    loaded.origin = file.origin;
    loaded.origin_len = file.origin_len;
    if (programSpawnTransactionCancelled()) {
        freeProgramImage(loaded.image);
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_TASK_FAILED);
        return .failed;
    }
    if (consumeProgramLifecycleFailure(.image)) {
        freeProgramImage(loaded.image);
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_LOAD_FAILED);
        return .failed;
    }
    reportBootLaunchStage(options.report_boot_launch, "Stack anlegen");
    const stack = allocateProgramStack(instance_id, loaded.memory_contract, parseSubsystemTrace(args) != null) orelse {
        k.puts("Program stack allocation failed\r\n");
        freeProgramImage(loaded.image);
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_NO_MEMORY);
        return .failed;
    };
    if (programSpawnTransactionCancelled()) {
        freeProgramResources(loaded.image, stack);
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_TASK_FAILED);
        return .failed;
    }
    if (consumeProgramLifecycleFailure(.stack)) {
        freeProgramResources(loaded.image, stack);
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_NO_MEMORY);
        return .failed;
    }

    reportBootLaunchStage(options.report_boot_launch, "Startobjekte anlegen");
    return switch (mode) {
        .foreground => runForegroundProgram(&reservation, &reservation_active, loaded, stack, app_class, args, working_drive, options),
        .shell => runShellProgram(&reservation, &reservation_active, loaded, stack, app_class, args, working_drive, shell_host, options),
        .background => runBackgroundProgram(&reservation, &reservation_active, loaded, stack, app_class, args, working_drive, shell_host, options),
    };
}

fn rejectHeader(message: []const u8, verbose: bool) bool {
    if (verbose) k.puts(message);
    return false;
}

const R4XExportContract = struct {
    has_start_v1: bool = false,
    has_start_v2_or_newer: bool = false,

    fn hasExactStartV1(self: R4XExportContract) bool {
        return self.has_start_v1 and !self.has_start_v2_or_newer;
    }
};

fn readValidatedProgramMemoryContractFromReader(reader: *module_r4m.Reader, r4m: module_r4m.Header, app_class: AppClass, export_contract: R4XExportContract, verbose: bool) ?ProgramMemoryContract {
    var meta_buf: [MAX_R4M_METADATA_PROBE]u8 = .{0} ** MAX_R4M_METADATA_PROBE;
    const meta = reader.readMetadata(r4m, meta_buf[0..], "r4x-metadata-probe", verbose) orelse return null;
    if (!r4x_start.accepts(meta, export_contract.hasExactStartV1(), r4m.export_count)) return null;
    return resolveProgramMemoryContractMetadata(meta, app_class);
}

fn readR4MProgramHeaderFromReader(reader: *module_r4m.Reader, name: []const u8, verbose: bool) ?module_r4m.Header {
    const r4m = reader.readHeader(.r4x, .{
        .max_sections = MAX_R4M_SECTIONS,
        .max_imports = MAX_R4M_IMPORTS,
        .max_exports = MAX_R4M_EXPORTS,
    }, name, verbose) orelse return null;
    if (!validateR4MRangeHeader(r4m, verbose)) return null;
    return r4m;
}

fn scanR4XStartExports(reader: *module_r4m.Reader, header: module_r4m.Header, verbose: bool) ?R4XExportContract {
    var contract = R4XExportContract{};
    var i: usize = 0;
    while (i < header.export_count) : (i += 1) {
        const exp = reader.readExportRecord(header, i, "r4x-export-table", verbose) orelse return null;
        var name_buf: [MAX_R4M_SYMBOL_PROBE]u8 = .{0} ** MAX_R4M_SYMBOL_PROBE;
        const name = reader.readZString(exp.name_offset, name_buf[0..], "r4x-export-name", verbose) orelse return null;
        if (!stdMemEql(name, "R4XStart")) continue;
        if (exp.version >= 1) contract.has_start_v1 = true;
        if (exp.version >= 2) contract.has_start_v2_or_newer = true;
    }
    return contract;
}

fn loadR4MProgramImage(file: ProgramFile, owner_id: u32, app_class: AppClass, reader: *module_r4m.Reader, r4m: module_r4m.Header) ?LoadedProgram {
    const source = programModuleFileSource(file);
    const file_size: usize = @intCast(file.entry.size);

    var sections: [MAX_R4M_SECTIONS]R4MSection = undefined;
    if (!readR4MSectionsFromReader(reader, file_size, r4m, sections[0..])) {
        k.puts("Invalid R4M0 sections\r\n");
        return null;
    }
    const section_count: usize = @intCast(r4m.section_count);
    var export_contract = R4XExportContract{};
    if (!validateR4MExportsFromReader(reader, r4m, sections[0..section_count], &export_contract)) {
        k.puts("Invalid R4M0 exports\r\n");
        return null;
    }
    const memory_contract = readValidatedProgramMemoryContractFromReader(reader, r4m, app_class, export_contract, true) orelse return null;
    var section_offsets: [MAX_R4M_SECTIONS]usize = .{0} ** MAX_R4M_SECTIONS;
    const image_size = layoutR4MSections(sections[0..section_count], section_offsets[0..]) orelse {
        k.puts("Invalid R4M0 image layout\r\n");
        return null;
    };
    const image = allocateProgramImage(image_size, owner_id) orelse {
        k.puts("Program module image allocation failed\r\n");
        return null;
    };
    @memset(image.code, 0);

    if (!readR4MSectionsIntoImage(source, sections[0..section_count], section_offsets[0..], image.code)) {
        freeProgramImage(image);
        return null;
    }

    var resolved_imports: [MAX_R4M_IMPORTS]ResolvedR4MImport = .{ResolvedR4MImport{}} ** MAX_R4M_IMPORTS;
    var r4xstart_imports: [MAX_R4M_IMPORTS]R4XStartImportSeed = .{R4XStartImportSeed{}} ** MAX_R4M_IMPORTS;
    const import_count: usize = @intCast(r4m.import_count);
    if (!resolveR4MImportsFromReader(reader, r4m, resolved_imports[0..], r4xstart_imports[0..])) {
        freeProgramImage(image);
        return null;
    }
    const r4xstart_import_count = r4m.import_count;
    if (!applyR4MRelocationsFromReader(reader, r4m, sections[0..section_count], section_offsets[0..], image.code, resolved_imports[0..import_count])) {
        freeProgramImage(image);
        return null;
    }

    const entry_record = readR4MEntryFromReader(reader, r4m, 0) orelse {
        k.puts("Invalid R4M0 entry\r\n");
        freeProgramImage(image);
        return null;
    };
    if (entry_record.kind != R4M_ENTRY_KIND_R4X) {
        k.puts("Unsupported R4M0 entry kind\r\n");
        freeProgramImage(image);
        return null;
    }
    const entry_section: usize = @intCast(entry_record.section);
    if (entry_section >= section_count or !r4mSectionLoadable(sections[entry_section])) {
        k.puts("Invalid R4M0 entry section\r\n");
        freeProgramImage(image);
        return null;
    }
    const entry_offset = section_offsets[entry_section] + @as(usize, @intCast(entry_record.offset));
    if (entry_offset >= image.code.len) {
        k.puts("Invalid R4M0 entry offset\r\n");
        freeProgramImage(image);
        return null;
    }
    const entry: RawEntryFn = @ptrCast(@alignCast(image.code[entry_offset..].ptr));
    return .{
        .image = image,
        .entry = entry,
        .memory_contract = memory_contract,
        .imports = r4xstart_imports,
        .import_count = r4xstart_import_count,
        .loader_section_count = r4m.section_count,
        .loader_relocation_count = r4m.reloc_count,
    };
}

fn programModuleFileSource(file: ProgramFile) module_file.FileSource {
    return .{
        .volume = file.volume,
        .entry = file.entry,
        .drive_letter = file.drive_letter,
    };
}

fn validateR4MRangeHeader(header: module_r4m.Header, verbose: bool) bool {
    if ((header.flags & ~R4X_KNOWN_FLAGS) != 0) return rejectHeader("Unsupported R4M0 flags\r\n", verbose);
    if (appClassFlagCount(header.flags) > 1) {
        return rejectHeader("Conflicting R4M0 app class flags\r\n", verbose);
    }
    if (header.entry_count == 0) return rejectHeader("Invalid R4M0 entry count\r\n", verbose);
    return true;
}

fn readR4MSectionsFromReader(reader: *module_r4m.Reader, file_size: usize, header: module_r4m.Header, out: []R4MSection) bool {
    if (header.section_count > out.len) return false;
    var non_alloc_seen = false;
    var i: usize = 0;
    while (i < header.section_count) : (i += 1) {
        const record = reader.readSectionRecord(header, i, "r4x-section-table", true) orelse return false;
        const section = R4MSection{
            .flags = record.flags,
            .file_off = record.file_off,
            .file_size = record.file_size,
            .mem_size = record.mem_size,
            .alignment = record.alignment,
        };
        if (section.mem_size == 0 or section.mem_size < section.file_size) return false;
        if (section.alignment == 0 or !isPowerOfTwo(@intCast(section.alignment))) return false;
        if (section.file_size != 0 and !checkR4MRangeU32(file_size, section.file_off, section.file_size)) return false;
        if (!r4mSectionLoadable(section)) {
            // Vertrag: hoechstens EINE non-alloc-Section, sie heisst .rsrc
            // und traegt sonst keine Flags.
            const name_len = for (record.name, 0..) |byte, index| {
                if (byte == 0) break index;
            } else record.name.len;
            const is_rsrc = name_len == 5 and record.name[0] == '.' and record.name[1] == 'r' and record.name[2] == 's' and record.name[3] == 'r' and record.name[4] == 'c';
            if (section.flags != 0 or non_alloc_seen or !is_rsrc) {
                k.puts("Invalid R4M0 non-alloc section\r\n");
                return false;
            }
            non_alloc_seen = true;
        }
        out[i] = section;
    }
    return true;
}

fn validateR4MExportsFromReader(reader: *module_r4m.Reader, header: module_r4m.Header, sections: []const R4MSection, contract: *R4XExportContract) bool {
    contract.* = .{};
    var i: usize = 0;
    while (i < header.export_count) : (i += 1) {
        const exp = reader.readExportRecord(header, i, "r4x-export-table", true) orelse return false;
        var name_buf: [MAX_R4M_NAME_PROBE]u8 = .{0} ** MAX_R4M_NAME_PROBE;
        const name = reader.readZString(exp.name_offset, name_buf[0..], "r4x-export-name", true) orelse return false;
        if (stdMemEql(name, "R4XStart")) {
            if (exp.version >= 1) contract.has_start_v1 = true;
            if (exp.version >= 2) contract.has_start_v2_or_newer = true;
        }
        const section_index: usize = @intCast(exp.section);
        const section_offset = exp.offset;
        if (section_index >= sections.len) return false;
        if (!r4mSectionLoadable(sections[section_index])) return false;
        if (section_offset >= sections[section_index].mem_size) return false;
    }
    return true;
}

fn resolveProgramMemoryContractMetadata(meta: []const u8, app_class: AppClass) ?ProgramMemoryContract {
    const profile = if (module_r4m.metadataValue(meta, "memory.profile=")) |value|
        parseMemoryProfile(value) orelse return null
    else
        defaultMemoryProfileForClass(app_class);

    var contract = ProgramMemoryContract{
        .profile = profile,
        .limits = memoryLimitsForProfile(profile),
        .tag = .{0} ** 16,
    };

    if (module_r4m.metadataValue(meta, "memory.reserve=")) |value| {
        contract.limits.vm_reserve_limit = parseMemorySize(value) orelse return null;
    }
    if (module_r4m.metadataValue(meta, "memory.commit=")) |value| {
        contract.limits.vm_commit_limit = parseMemorySize(value) orelse return null;
    }
    if (module_r4m.metadataValue(meta, "memory.resident=")) |value| {
        contract.limits.resident_limit = parseMemorySize(value) orelse return null;
    }
    if (module_r4m.metadataValue(meta, "memory.stack.reserve=")) |value| {
        contract.limits.stack_reserve = parseMemorySize(value) orelse return null;
    }
    if (module_r4m.metadataValue(meta, "memory.stack.commit=")) |value| {
        contract.limits.stack_initial_commit = parseMemorySize(value) orelse return null;
    }
    if (module_r4m.metadataValue(meta, "memory.tag=")) |value| {
        copyFixedZ(contract.tag[0..], value);
    }

    if (!validateMemoryLimits(contract.limits)) return null;
    return contract;
}

fn validateMemoryLimits(limits: ProgramMemoryLimits) bool {
    const reserve = pageAlignU64(limits.vm_reserve_limit) orelse return false;
    const commit = pageAlignU64(limits.vm_commit_limit) orelse return false;
    const resident = pageAlignU64(limits.resident_limit) orelse return false;
    const stack_reserve = pageAlignU64(limits.stack_reserve) orelse return false;
    const stack_commit = pageAlignU64(limits.stack_initial_commit) orelse return false;
    if (commit > reserve) return false;
    if (resident > commit) return false;
    if (stack_reserve <= PROGRAM_STACK_GUARD_SIZE) return false;
    if (stack_commit > stack_reserve - PROGRAM_STACK_GUARD_SIZE) return false;
    return true;
}

fn defaultMemoryProfileForClass(app_class: AppClass) MemoryProfile {
    return switch (app_class) {
        .console => .normal,
        .gui => .desktop,
        .service => .service,
    };
}

fn parseMemoryProfile(value: []const u8) ?MemoryProfile {
    const trimmed = trimAscii(value);
    if (equalsIgnoreCase(trimmed, "tiny")) return .tiny;
    if (equalsIgnoreCase(trimmed, "normal")) return .normal;
    if (equalsIgnoreCase(trimmed, "desktop")) return .desktop;
    if (equalsIgnoreCase(trimmed, "service")) return .service;
    if (equalsIgnoreCase(trimmed, "large-service") or equalsIgnoreCase(trimmed, "large_service")) return .large_service;
    if (equalsIgnoreCase(trimmed, "build-tool") or equalsIgnoreCase(trimmed, "build_tool")) return .build_tool;
    if (equalsIgnoreCase(trimmed, "browser")) return .browser;
    if (equalsIgnoreCase(trimmed, "workstation")) return .workstation;
    return null;
}

fn memoryLimitsForProfile(profile: MemoryProfile) ProgramMemoryLimits {
    return switch (profile) {
        .tiny => .{
            .vm_reserve_limit = 128 * MB,
            .vm_commit_limit = 32 * MB,
            .resident_limit = 32 * MB,
            .stack_reserve = 2 * MB,
            .stack_initial_commit = PROGRAM_STACK_INITIAL_COMMIT_SIZE,
        },
        .normal, .unknown => .{
            .vm_reserve_limit = 1024 * MB,
            .vm_commit_limit = 256 * MB,
            .resident_limit = 256 * MB,
            .stack_reserve = PROGRAM_STACK_RESERVE_SIZE,
            .stack_initial_commit = PROGRAM_STACK_INITIAL_COMMIT_SIZE,
        },
        .desktop => .{
            .vm_reserve_limit = 2 * GB,
            .vm_commit_limit = 512 * MB,
            .resident_limit = 512 * MB,
            .stack_reserve = PROGRAM_STACK_RESERVE_SIZE,
            .stack_initial_commit = 128 * KB,
        },
        .service => .{
            .vm_reserve_limit = 1024 * MB,
            .vm_commit_limit = 256 * MB,
            .resident_limit = 256 * MB,
            .stack_reserve = PROGRAM_STACK_RESERVE_SIZE,
            .stack_initial_commit = PROGRAM_STACK_SERVICE_INITIAL_COMMIT_SIZE,
        },
        .large_service => .{
            .vm_reserve_limit = 4 * GB,
            .vm_commit_limit = 1024 * MB,
            .resident_limit = 1024 * MB,
            .stack_reserve = PROGRAM_STACK_LARGE_RESERVE_SIZE,
            .stack_initial_commit = 128 * KB,
        },
        .build_tool => .{
            .vm_reserve_limit = 4 * GB,
            .vm_commit_limit = 1024 * MB,
            .resident_limit = 1024 * MB,
            .stack_reserve = PROGRAM_STACK_LARGE_RESERVE_SIZE,
            .stack_initial_commit = 128 * KB,
        },
        .browser => .{
            .vm_reserve_limit = 16 * GB,
            .vm_commit_limit = 4 * GB,
            .resident_limit = 4 * GB,
            .stack_reserve = 32 * MB,
            .stack_initial_commit = 256 * KB,
        },
        .workstation => .{
            .vm_reserve_limit = 8 * GB,
            .vm_commit_limit = 2 * GB,
            .resident_limit = 2 * GB,
            .stack_reserve = 32 * MB,
            .stack_initial_commit = 256 * KB,
        },
    };
}

fn parseMemorySize(value: []const u8) ?u64 {
    const s = trimAscii(value);
    if (s.len == 0) return null;
    var end = s.len;
    while (end > 0 and isAlpha(s[end - 1])) : (end -= 1) {}
    if (end == 0) return null;
    var number: u64 = 0;
    var i: usize = 0;
    while (i < end) : (i += 1) {
        const ch = s[i];
        if (ch == '_') continue;
        if (ch < '0' or ch > '9') return null;
        const digit: u64 = @intCast(ch - '0');
        if (number > (~@as(u64, 0) - digit) / 10) return null;
        number = number * 10 + digit;
    }
    const suffix = trimAscii(s[end..]);
    const factor: u64 = if (suffix.len == 0 or equalsIgnoreCase(suffix, "b"))
        1
    else if (equalsIgnoreCase(suffix, "k") or equalsIgnoreCase(suffix, "kb"))
        KB
    else if (equalsIgnoreCase(suffix, "m") or equalsIgnoreCase(suffix, "mb"))
        MB
    else if (equalsIgnoreCase(suffix, "g") or equalsIgnoreCase(suffix, "gb"))
        GB
    else
        return null;
    if (number > (~@as(u64, 0)) / factor) return null;
    return pageAlignU64(number * factor) orelse null;
}

fn layoutR4MSections(sections: []const R4MSection, section_offsets: []usize) ?usize {
    var cursor: usize = 0;
    for (sections, 0..) |section, index| {
        if (!r4mSectionLoadable(section)) {
            section_offsets[index] = 0;
            continue;
        }
        const raw_align: usize = @intCast(section.alignment);
        const section_align = if (raw_align > paging.PAGE_SIZE) raw_align else paging.PAGE_SIZE;
        cursor = alignUp(cursor, section_align) orelse return null;
        section_offsets[index] = cursor;
        cursor = checkedAdd(cursor, @intCast(section.mem_size)) orelse return null;
    }
    const image_size = alignUp(cursor, paging.PAGE_SIZE) orelse return null;
    if (image_size == 0) return null;
    return image_size;
}

fn readR4MSectionsIntoImage(source: module_file.FileSource, sections: []const R4MSection, section_offsets: []const usize, image: []u8) bool {
    var section_index: usize = 0;
    while (section_index < sections.len) : (section_index += 1) {
        const section = sections[section_index];
        if (!r4mSectionLoadable(section)) continue;
        if (section.file_size == 0) continue;
        if ((section.flags & R4M_SECTION_FLAG_BSS) != 0) {
            k.puts("Invalid R4M0 BSS payload\r\n");
            return false;
        }
        const size: usize = @intCast(section.file_size);
        const dst_off = section_offsets[section_index];
        if (dst_off > image.len or size > image.len - dst_off) return false;
        if (!module_file.readExact(.{
            .source = source,
            .offset = @intCast(section.file_off),
            .out = image[dst_off .. dst_off + size],
            .name = "r4x-section-payload",
            .verbose = true,
        })) {
            k.puts("R4M0 section payload read failed\r\n");
            return false;
        }
    }
    return true;
}

fn resolveR4MImportsFromReader(reader: *module_r4m.Reader, header: module_r4m.Header, resolved_out: []ResolvedR4MImport, start_out: []R4XStartImportSeed) bool {
    if (header.import_count > resolved_out.len or header.import_count > start_out.len) return false;
    var i: usize = 0;
    while (i < header.import_count) : (i += 1) {
        var module_buf: [MAX_R4M_NAME_PROBE]u8 = .{0} ** MAX_R4M_NAME_PROBE;
        var symbol_buf: [MAX_R4M_NAME_PROBE]u8 = .{0} ** MAX_R4M_NAME_PROBE;
        const record = reader.readImportRecord(header, i, "r4x-import-table", true) orelse return false;
        const module_name = reader.readZString(record.module_offset, module_buf[0..], "r4x-import-module", true) orelse return false;
        const symbol_name = reader.readZString(record.symbol_offset, symbol_buf[0..], "r4x-import-symbol", true) orelse return false;
        const import = R4MImport{
            .module = module_name,
            .symbol = symbol_name,
            .min_version = record.min_version,
            .flags = record.flags,
        };
        const resolved = modules.resolveExportInfo(import.module, import.symbol, import.min_version) orelse {
            k.puts("Unresolved R4M0 import ");
            k.puts(import.module);
            k.puts(":");
            k.puts(import.symbol);
            k.puts("\r\n");
            return false;
        };
        resolved_out[i] = .{ .address = resolved.address, .version = resolved.version, .generation = resolved.generation };
        // Gruppen-IDs gehoeren ausschliesslich zu den sechs fest eingebauten
        // Plattform-APIs. Jeder Runtime-R4L-Export wird als benanntes
        // Interface mit group_id=0 transportiert.
        const platform_group_id = if (stdMemEql(import.symbol, "Query")) modules.platformApiGroupId(import.module) else null;
        const group_id = platform_group_id orelse 0;
        var seed = R4XStartImportSeed{
            .group_id = group_id,
            .min_version = import.min_version,
            .resolved_version = resolved.version,
            .flags = 0,
            .table = resolved.address,
        };
        if (!copyR4XStartName(import.module, seed.module_name[0..], &seed.module_name_len)) return false;
        if (!copyR4XStartName(import.symbol, seed.symbol_name[0..], &seed.symbol_name_len)) return false;
        if (stdMemEql(import.symbol, "Query")) {
            if (r4xstartGroupInterface(group_id)) |table| {
                seed.flags |= R4XSTART_IMPORT_FLAG_GROUP_INTERFACE;
                seed.table = table;
            }
        }
        if (group_id == 0 and !stdMemEql(import.symbol, "Query")) {
            k.puts("[R4X] named import module=");
            k.puts(import.module);
            k.puts(" symbol=");
            k.puts(import.symbol);
            k.puts(" need=");
            k.putDec(import.min_version);
            k.puts(" have=");
            k.putDec(resolved.version);
            k.puts(" generation=");
            k.putDec(resolved.generation);
            k.puts("\r\n");
        }
        start_out[i] = seed;
    }
    return true;
}

fn r4xstartGroupInterface(group_id: u32) ?u64 {
    return switch (group_id) {
        R4L_GROUP_R4SYS => @intFromPtr(&r4xstart_r4sys_table),
        R4L_GROUP_R4DESK => @intFromPtr(&r4xstart_r4desk_table),
        R4L_GROUP_R4DRAW => @intFromPtr(&r4xstart_r4draw_table),
        // NET/AUDIO/DEV liefern eigenstaendige Kernel-Gruppentabellen.
        R4L_GROUP_R4NET => @intFromPtr(&r4xstart_r4net_table),
        R4L_GROUP_R4AUDIO => @intFromPtr(&r4xstart_r4audio_table),
        R4L_GROUP_R4DEV => @intFromPtr(&r4xstart_r4dev_table),
        else => null,
    };
}

fn copyR4XStartName(src: []const u8, dest: []u8, out_len: *usize) bool {
    if (src.len >= dest.len) return false;
    @memset(dest, 0);
    @memcpy(dest[0..src.len], src);
    out_len.* = src.len;
    return true;
}

fn applyR4MRelocationsFromReader(reader: *module_r4m.Reader, header: module_r4m.Header, sections: []const R4MSection, section_offsets: []const usize, image: []u8, resolved_imports: []const ResolvedR4MImport) bool {
    var reloc_reader = module_r4m.RelocationWindowReader.init(reader, header, "r4x-relocation-table", true);
    var i: usize = 0;
    while (i < header.reloc_count) : (i += 1) {
        const record = reloc_reader.next() orelse return false;
        const reloc = R4MRelocation{
            .kind = record.kind,
            .patch_section = record.patch_section,
            .patch_offset = record.patch_offset,
            .target_section = record.target_section,
            .target_offset = record.target_offset,
            .addend = record.addend,
        };
        if (!applyR4MRelocation(reloc, sections, section_offsets, image, resolved_imports)) return false;
    }
    return true;
}

fn applyR4MRelocation(reloc: R4MRelocation, sections: []const R4MSection, section_offsets: []const usize, image: []u8, resolved_imports: []const ResolvedR4MImport) bool {
    switch (reloc.kind) {
        R4M_RELOC_ABS64, R4M_RELOC_BASE_REL64 => {
            const patch = r4mPatchSlice(reloc, sections, section_offsets, image, 8) orelse return false;
            const target = r4mTargetAddress(reloc, sections, section_offsets, @intFromPtr(image.ptr)) orelse return false;
            writeLe64(patch, addSignedU64(target, reloc.addend) orelse return false);
            return true;
        },
        R4M_RELOC_REL32 => {
            const patch = r4mPatchSlice(reloc, sections, section_offsets, image, 4) orelse return false;
            const target = r4mTargetAddress(reloc, sections, section_offsets, @intFromPtr(image.ptr)) orelse return false;
            const patched_target = addSignedU64(target, reloc.addend) orelse return false;
            const patch_addr: u64 = @intFromPtr(patch.ptr);
            const delta = @as(i128, patched_target) - @as(i128, patch_addr + 4);
            if (delta < -2147483648 or delta > 2147483647) return false;
            writeLe32(patch, @bitCast(@as(i32, @intCast(delta))));
            return true;
        },
        R4M_RELOC_IMPORT_SLOT64 => {
            const patch = r4mPatchSlice(reloc, sections, section_offsets, image, 8) orelse return false;
            const import_index: usize = @intCast(reloc.target_section);
            if (import_index >= resolved_imports.len) return false;
            writeLe64(patch, addSignedU64(resolved_imports[import_index].address, reloc.addend) orelse return false);
            return true;
        },
        else => {
            k.puts("Unsupported R4M0 relocation\r\n");
            return false;
        },
    }
}

fn r4mPatchSlice(reloc: R4MRelocation, sections: []const R4MSection, section_offsets: []const usize, image: []u8, size: usize) ?[]u8 {
    const section_index: usize = @intCast(reloc.patch_section);
    if (section_index >= sections.len) return null;
    const section = sections[section_index];
    if (!r4mSectionLoadable(section)) return null;
    const patch_offset: usize = @intCast(reloc.patch_offset);
    const mem_size: usize = @intCast(section.mem_size);
    if (patch_offset > mem_size or size > mem_size - patch_offset) return null;
    const off = section_offsets[section_index] + patch_offset;
    if (off > image.len or size > image.len - off) return null;
    return image[off .. off + size];
}

fn r4mTargetAddress(reloc: R4MRelocation, sections: []const R4MSection, section_offsets: []const usize, image_base: u64) ?u64 {
    const section_index: usize = @intCast(reloc.target_section);
    if (section_index >= sections.len) return null;
    const section = sections[section_index];
    if (!r4mSectionLoadable(section)) return null;
    if (reloc.target_offset >= section.mem_size) return null;
    return image_base + @as(u64, @intCast(section_offsets[section_index])) + reloc.target_offset;
}

fn readR4MEntryFromReader(reader: *module_r4m.Reader, header: module_r4m.Header, index: usize) ?R4MEntry {
    const record = reader.readEntryRecord(header, index, "r4x-entry-table", true) orelse return null;
    return .{
        .kind = record.kind,
        .section = record.section,
        .offset = record.offset,
        .flags = record.flags,
    };
}

fn checkR4MTable(file_size: usize, off: u32, count: u32, entry_size: usize, required: bool) bool {
    if (count == 0) return !required;
    if (off == 0) return false;
    const table_off: usize = @intCast(off);
    const table_count: usize = @intCast(count);
    if (table_count > file_size / entry_size) return false;
    return checkR4MRangeUsize(file_size, table_off, table_count * entry_size);
}

fn checkR4MRangeU32(file_size: usize, off: u32, len: u32) bool {
    return checkR4MRangeUsize(file_size, @as(usize, @intCast(off)), @as(usize, @intCast(len)));
}

fn checkR4MRangeUsize(file_size: usize, off: usize, len: usize) bool {
    return off <= file_size and len <= file_size - off;
}

fn addSignedU64(base: u64, addend: i32) ?u64 {
    if (addend >= 0) {
        const plus: u64 = @intCast(addend);
        if ((~@as(u64, 0)) - base < plus) return null;
        return base + plus;
    }
    const sub: u64 = @intCast(-@as(i64, addend));
    if (sub > base) return null;
    return base - sub;
}

fn allocateProgramImage(code_size: usize, owner_id: u32) ?ProgramImage {
    const reserved_len = pageAlign(code_size) orelse return null;
    const range_id = mem_virt.reserve(.{
        .window = .program_image,
        .len = reserved_len,
        .kind = .program_image,
        .owner = .r4x_instance,
        .owner_id = owner_id,
        .name = "r4x-program-image",
        .flags = paging.WRITABLE,
    }) catch |err| {
        k.puts("Program image reserve failed: ");
        k.puts(@errorName(err));
        k.puts("\r\n");
        return null;
    };
    mem_virt.commit(range_id, 0, reserved_len) catch |err| {
        k.puts("Program image commit failed: ");
        k.puts(@errorName(err));
        k.puts("\r\n");
        _ = mem_virt.release(range_id) catch {};
        return null;
    };
    const info = mem_virt.rangeInfo(range_id) orelse {
        _ = mem_virt.release(range_id) catch {};
        return null;
    };
    const ptr: [*]u8 = @ptrFromInt(info.base);
    return .{ .range_id = range_id, .code = ptr[0..code_size], .owner_id = owner_id };
}

fn pageAlign(value: usize) ?u64 {
    if (value == 0) return null;
    return pageAlignU64(@intCast(value));
}

fn pageAlignU64(value: u64) ?u64 {
    if (value == 0) return null;
    const mask = paging.PAGE_SIZE - 1;
    if (value > (~@as(u64, 0)) - mask) return null;
    return (value + mask) & ~mask;
}

fn freeProgramImage(image: ProgramImage) void {
    if (image.range_id == 0) return;
    mem_virt.release(image.range_id) catch {
        k.puts("Program image release failed\r\n");
    };
}

fn recordProgramStackCreate(profile: MemoryProfile, reserve_bytes: u64, initial_commit_bytes: u64, cycles: u64) void {
    const stats = &program_stack_telemetry_by_profile[@intFromEnum(profile)];
    _ = @atomicRmw(u64, &stats.creates, .Add, 1, .monotonic);
    _ = @atomicRmw(u64, &stats.reserve_max_bytes, .Max, reserve_bytes, .monotonic);
    _ = @atomicRmw(u64, &stats.initial_commit_max_bytes, .Max, initial_commit_bytes, .monotonic);
    _ = @atomicRmw(u64, &stats.create_cycles_total, .Add, cycles, .monotonic);
    _ = @atomicRmw(u64, &stats.create_cycles_max, .Max, cycles, .monotonic);
}

fn recordProgramStackRelease(stack: *const ProgramStack, committed_bytes: u64, release_cycles: u64) ProgramStackTelemetryStats {
    const stats = &program_stack_telemetry_by_profile[@intFromEnum(stack.profile)];
    _ = @atomicRmw(u64, &stats.releases, .Add, 1, .monotonic);
    _ = @atomicRmw(u64, &stats.committed_max_bytes, .Max, committed_bytes, .monotonic);
    _ = @atomicRmw(u64, &stats.high_water_max_bytes, .Max, stack.telemetry_high_water, .monotonic);
    _ = @atomicRmw(u64, &stats.release_cycles_total, .Add, release_cycles, .monotonic);
    _ = @atomicRmw(u64, &stats.release_cycles_max, .Max, release_cycles, .monotonic);
    return loadProgramStackTelemetryStats(stack.profile);
}

fn loadProgramStackTelemetryStats(profile: MemoryProfile) ProgramStackTelemetryStats {
    const stats = &program_stack_telemetry_by_profile[@intFromEnum(profile)];
    return .{
        .creates = @atomicLoad(u64, &stats.creates, .monotonic),
        .releases = @atomicLoad(u64, &stats.releases, .monotonic),
        .reserve_max_bytes = @atomicLoad(u64, &stats.reserve_max_bytes, .monotonic),
        .initial_commit_max_bytes = @atomicLoad(u64, &stats.initial_commit_max_bytes, .monotonic),
        .committed_max_bytes = @atomicLoad(u64, &stats.committed_max_bytes, .monotonic),
        .high_water_max_bytes = @atomicLoad(u64, &stats.high_water_max_bytes, .monotonic),
        .create_cycles_total = @atomicLoad(u64, &stats.create_cycles_total, .monotonic),
        .create_cycles_max = @atomicLoad(u64, &stats.create_cycles_max, .monotonic),
        .release_cycles_total = @atomicLoad(u64, &stats.release_cycles_total, .monotonic),
        .release_cycles_max = @atomicLoad(u64, &stats.release_cycles_max, .monotonic),
    };
}

fn poisonProgramStackSpan(base: u64, len_raw: u64) void {
    if (base == 0 or len_raw == 0) return;
    const len: usize = @intCast(len_raw);
    const bytes: [*]u8 = @ptrFromInt(base);
    @memset(bytes[0..len], PROGRAM_STACK_HIGH_WATER_PATTERN);
}

fn measureProgramStackHighWater(stack: *ProgramStack) void {
    if (stack.telemetry_measured) return;
    stack.telemetry_measured = true;
    stack.telemetry_committed_pages = std.math.cast(u32, stack.committed_size / paging.PAGE_SIZE) orelse std.math.maxInt(u32);
    if (stack.committed_base == 0 or stack.top <= stack.committed_base) return;
    const len: usize = @intCast(stack.top - stack.committed_base);
    const bytes: [*]const u8 = @ptrFromInt(stack.committed_base);
    var untouched: usize = 0;
    while (untouched < len and bytes[untouched] == PROGRAM_STACK_HIGH_WATER_PATTERN) : (untouched += 1) {}
    stack.telemetry_high_water = @intCast(len - untouched);
}

fn readStackTimestampCounter() u64 {
    var lo: u32 = undefined;
    var hi: u32 = undefined;
    asm volatile ("rdtsc"
        : [lo] "={eax}" (lo),
          [hi] "={edx}" (hi),
    );
    return (@as(u64, hi) << 32) | lo;
}

fn logProgramStackRelease(stack: *const ProgramStack, committed_bytes: u64, release_cycles: u64, stats: ProgramStackTelemetryStats) void {
    const task_stats = task.stackTelemetryStats();
    const cache_stats = task.stackCacheStats();
    const critical_stats = task.criticalReserveStats();
    k.puts("[R4XSTACK] release owner=");
    k.putDec(stack.owner_id);
    k.puts(" profile=");
    k.puts(memoryProfileName(stack.profile));
    k.puts(" reserve=");
    k.putDec(stack.reserve_size);
    k.puts(" initial=");
    k.putDec(stack.initial_commit_size);
    k.puts(" committed=");
    k.putDec(committed_bytes);
    k.puts(" highwater=");
    k.putDec(stack.telemetry_high_water);
    k.puts(" creates=");
    k.putDec(stats.creates);
    k.puts(" releases=");
    k.putDec(stats.releases);
    k.puts(" create_cycles=");
    k.putDec(stack.create_cycles);
    k.puts(" create_cycles_max=");
    k.putDec(stats.create_cycles_max);
    k.puts(" release_cycles=");
    k.putDec(release_cycles);
    k.puts(" release_cycles_max=");
    k.putDec(stats.release_cycles_max);
    k.puts(" kernel_highwater_max=");
    k.putDec(task_stats.high_water_max_bytes);
    k.puts(" kernel_create_cycles_max=");
    k.putDec(task_stats.create_cycles_max);
    k.puts(" kernel_release_cycles_max=");
    k.putDec(task_stats.release_cycles_max);
    k.puts(" kernel_cache_cached=");
    k.putDec(cache_stats.cached);
    k.puts(" kernel_cache_hits=");
    k.putDec(cache_stats.hits);
    k.puts(" kernel_cache_misses=");
    k.putDec(cache_stats.misses);
    k.puts(" critical_available=");
    k.putDec(critical_stats.available);
    k.puts(" critical_in_use=");
    k.putDec(critical_stats.in_use);
    k.puts("\r\n");
}

fn allocateProgramStack(owner_id: u32, contract: ProgramMemoryContract, serial_telemetry: bool) ?ProgramStack {
    return allocateProgramStackWithLimits(owner_id, contract.limits.stack_reserve, contract.limits.stack_initial_commit, contract.profile, serial_telemetry);
}

fn allocateProgramStackWithLimits(owner_id: u32, reserve_size: u64, initial_commit_size: u64, profile: MemoryProfile, serial_telemetry: bool) ?ProgramStack {
    const create_started_cycles = readStackTimestampCounter();
    const reserve_len = pageAlignU64(reserve_size) orelse return null;
    const initial_len = pageAlignU64(initial_commit_size) orelse return null;
    if (reserve_len <= PROGRAM_STACK_GUARD_SIZE or initial_len > reserve_len - PROGRAM_STACK_GUARD_SIZE) return null;

    const range_id = mem_virt.reserve(.{
        .window = .app_stack,
        .len = reserve_len,
        .kind = .app_stack,
        .owner = .r4x_instance,
        .owner_id = owner_id,
        .name = "r4x-app-stack",
        .flags = paging.WRITABLE | paging.NO_EXECUTE,
    }) catch |err| {
        k.puts("Program stack reserve failed: ");
        k.puts(@errorName(err));
        k.puts("\r\n");
        return null;
    };

    const info = mem_virt.rangeInfo(range_id) orelse {
        _ = mem_virt.release(range_id) catch {};
        return null;
    };
    const top = info.base + info.len;
    const committed_base = top - initial_len;
    const guard_base = committed_base - PROGRAM_STACK_GUARD_SIZE;

    mem_virt.protectGuard(range_id, guard_base - info.base, PROGRAM_STACK_GUARD_SIZE) catch |err| {
        k.puts("Program stack guard failed: ");
        k.puts(@errorName(err));
        k.puts("\r\n");
        _ = mem_virt.release(range_id) catch {};
        return null;
    };
    mem_virt.commit(range_id, committed_base - info.base, initial_len) catch |err| {
        k.puts("Program stack commit failed: ");
        k.puts(@errorName(err));
        k.puts("\r\n");
        _ = mem_virt.release(range_id) catch {};
        return null;
    };

    poisonProgramStackSpan(committed_base, initial_len);
    const create_cycles = readStackTimestampCounter() -% create_started_cycles;
    recordProgramStackCreate(profile, info.len, initial_len, create_cycles);
    return .{
        .range_id = range_id,
        .base = info.base,
        .reserve_size = info.len,
        .committed_base = committed_base,
        .committed_size = initial_len,
        .guard_base = guard_base,
        .guard_size = PROGRAM_STACK_GUARD_SIZE,
        .top = top,
        .owner_id = owner_id,
        .profile = profile,
        .initial_commit_size = initial_len,
        .create_cycles = create_cycles,
        .serial_telemetry = serial_telemetry,
    };
}

fn freeProgramStack(stack: *ProgramStack) bool {
    if (stack.range_id == 0) return true;
    measureProgramStackHighWater(stack);
    const committed_bytes = @as(u64, stack.telemetry_committed_pages) * paging.PAGE_SIZE;
    const release_started_tick = timer.tickCount();
    const release_started_cycles = readStackTimestampCounter();
    if (stack.committed_size != 0 and stack.committed_base >= stack.base) {
        mem_virt.uncommit(stack.range_id, stack.committed_base - stack.base, stack.committed_size) catch |err| {
            k.puts("Program stack committed-span release failed: ");
            k.puts(@errorName(err));
            k.puts("\r\n");
            // release() is retry-aware and completes any already-unmapped
            // prefix through the Range's partial-uncommit anchor. If that
            // fallback also cannot finish, keep this ProgramStack unchanged
            // in its owning ProgramThread/ProgramResources record.
            mem_virt.release(stack.range_id) catch |release_err| {
                // Virtual-range IDs are monotonic for the lifetime of this
                // boot. NotFound therefore proves that an earlier attempt
                // crossed the release point and only lost its acknowledgement;
                // retaining the stale stack would retry forever.
                if (release_err == error.NotFound)
                    return finishProgramStackRelease(stack, committed_bytes, release_started_tick, release_started_cycles);
                k.puts("Program stack deferred release failed: ");
                k.puts(@errorName(release_err));
                k.puts("\r\n");
                return false;
            };
            return finishProgramStackRelease(stack, committed_bytes, release_started_tick, release_started_cycles);
        };
        stack.committed_size = 0;
    }
    mem_virt.release(stack.range_id) catch |err| {
        if (err == error.NotFound)
            return finishProgramStackRelease(stack, committed_bytes, release_started_tick, release_started_cycles);
        k.puts("Program stack release failed: ");
        k.puts(@errorName(err));
        k.puts("\r\n");
        return false;
    };
    return finishProgramStackRelease(stack, committed_bytes, release_started_tick, release_started_cycles);
}

fn finishProgramStackRelease(stack: *ProgramStack, committed_bytes: u64, release_started_tick: u64, release_started_cycles: u64) bool {
    const release_finished_tick = timer.tickCount();
    const release_ticks = if (release_finished_tick >= release_started_tick) release_finished_tick - release_started_tick else 0;
    const release_cycles = readStackTimestampCounter() -% release_started_cycles;
    if (release_ticks >= 25) {
        k.puts("[R4XTHREAD] slow stack release ticks=");
        k.putDec(release_ticks);
        k.puts(" reserve=");
        k.putDec(stack.reserve_size);
        k.puts(" committed=");
        k.putDec(committed_bytes);
        k.puts("\r\n");
    }
    const stats = recordProgramStackRelease(stack, committed_bytes, release_cycles);
    if (stack.serial_telemetry) logProgramStackRelease(stack, committed_bytes, release_cycles, stats);
    stack.* = .{};
    return true;
}

fn freeProgramResources(image: ProgramImage, stack: ProgramStack) void {
    var resources = programResourcesFromValues(image, stack);
    _ = cleanupProgramResources(&resources);
}

fn programResourcesFromValues(image: ProgramImage, stack: ProgramStack) ProgramResources {
    return .{
        .image_range_id = image.range_id,
        .image_base = @intFromPtr(image.code.ptr),
        .image_size = image.code.len,
        .image_owner_id = image.owner_id,
        .stack = stack,
    };
}

fn programResourcesFromInstance(instance: *const ProgramInstance) ProgramResources {
    return .{
        .image_range_id = instance.program_image_range_id,
        .image_base = instance.program_image_base,
        .image_size = instance.program_image_size,
        .image_owner_id = instance.id,
        .stack = stackFromInstance(instance) orelse ProgramStack{},
    };
}

fn cleanupProgramResources(resources: *ProgramResources) bool {
    if (resources.stack.range_id != 0) {
        if (!freeProgramStack(&resources.stack)) return false;
    }
    if (resources.image_range_id != 0 and resources.image_base != 0 and resources.image_size != 0) {
        const ptr: [*]u8 = @ptrFromInt(resources.image_base);
        freeProgramImage(.{
            .range_id = resources.image_range_id,
            .code = ptr[0..resources.image_size],
            .owner_id = resources.image_owner_id,
        });
    }
    resources.image_range_id = 0;
    resources.image_base = 0;
    resources.image_size = 0;
    resources.image_owner_id = 0;
    return true;
}
pub fn handlePageFault(addr: u64, error_code: u64) bool {
    if ((error_code & PAGE_FAULT_PRESENT) != 0) return false;
    const current_task_id = scheduler.currentId();
    if (currentExecutionInstanceNoRegistry()) |instance| {
        if (vmResidentWithinProfile(instance, paging.PAGE_SIZE)) {
            if (mem_virt.handleDemandFault(addr, error_code, .r4x_instance, @intCast(instance.id))) return true;
        }
    }

    if (currentProgramThread()) |thread_ctx| {
        if ((thread_ctx.flags & THREAD_FLAG_MAIN) != 0) {
            if (thread_ctx.owner_instance) |instance| {
                if (addressInStackReserve(instance, addr)) return growInstanceStack(instance, addr);
            }
        } else if (addressInProgramStack(&thread_ctx.stack, addr)) {
            return growThreadStack(thread_ctx, addr);
        }
    }

    var stack_owner_by_address: ?*ProgramInstance = null;
    var thread_owner_by_address: ?*ProgramThread = null;
    var thread_cursor = program_thread_head;
    while (thread_cursor) |thread_ctx| : (thread_cursor = thread_ctx.registry_next) {
        if (!thread_ctx.used) continue;
        if ((thread_ctx.flags & THREAD_FLAG_MAIN) != 0) {
            const instance = thread_ctx.owner_instance orelse continue;
            if (!addressInStackReserve(instance, addr)) continue;
            if (current_task_id) |task_id| {
                if (thread_ctx.task_id == task_id) return growInstanceStack(instance, addr);
            }
            if (stack_owner_by_address == null) stack_owner_by_address = instance;
            continue;
        }
        if (!addressInProgramStack(&thread_ctx.stack, addr)) continue;
        if (current_task_id) |task_id| {
            if (thread_ctx.task_id == task_id) return growThreadStack(thread_ctx, addr);
        }
        if (thread_owner_by_address == null) thread_owner_by_address = thread_ctx;
    }
    if (stack_owner_by_address) |instance| return growInstanceStack(instance, addr);
    if (thread_owner_by_address) |thread_ctx| return growThreadStack(thread_ctx, addr);
    return false;
}

fn growInstanceStack(instance: *ProgramInstance, fault_addr: u64) bool {
    var stack = stackFromInstance(instance) orelse return false;
    const before = stack.committed_size;
    if (!growProgramStack(&stack, fault_addr)) return false;
    writeStackToInstance(instance, stack);
    bootlog.puts("[R4X] stack grow owner=");
    bootlog.putDec(instance.id);
    bootlog.puts(" committed=");
    bootlog.putDec(before);
    bootlog.puts(" -> ");
    bootlog.putDec(stack.committed_size);
    bootlog.puts("\r\n");
    return true;
}

fn growThreadStack(thread_ctx: *ProgramThread, fault_addr: u64) bool {
    const before = thread_ctx.stack.committed_size;
    if (!growProgramStack(&thread_ctx.stack, fault_addr)) return false;
    bootlog.puts("[R4X] thread stack grow owner=");
    bootlog.putDec(thread_ctx.instance_id);
    bootlog.puts(" thread=");
    bootlog.putDec(thread_ctx.id);
    bootlog.puts(" committed=");
    bootlog.putDec(before);
    bootlog.puts(" -> ");
    bootlog.putDec(thread_ctx.stack.committed_size);
    bootlog.puts("\r\n");
    return true;
}

fn growProgramStack(stack: *ProgramStack, fault_addr: u64) bool {
    if (stack.range_id == 0 or stack.guard_size == 0) return false;
    const fault_page = alignDown(fault_addr, paging.PAGE_SIZE);
    if (fault_page < stack.base + stack.guard_size) {
        // 0.57.9: Finaler Anschlag [base, base+guard) - die Stack-Reserve
        // ist bis zum Boden verbraucht (oder ein grosser Frame hat den
        // wandernden Guard uebersprungen und den Boden getroffen). Vor dem
        // Crash klassifizieren, damit ein cr2=APP_STACK_BASE-Treffer nicht
        // wieder als anonymer Page Fault durchschlaegt (0.57.9-Befund).
        if (fault_page >= stack.base) {
            bootlog.puts("[R4X] STACK EXHAUSTED fault=");
            bootlog.putHex(fault_addr, 16);
            bootlog.puts(" base=");
            bootlog.putHex(stack.base, 16);
            bootlog.puts(" committed_base=");
            bootlog.putHex(stack.committed_base, 16);
            bootlog.puts(" reserve=");
            bootlog.putDec(stack.reserve_size);
            bootlog.puts("\r\n");
            k.puts("[R4X] STACK EXHAUSTED (app stack reserve aufgebraucht)\r\n");
        }
        return false;
    }
    if (fault_page >= stack.committed_base) return false;

    var desired_base = stack.committed_base - @min(PROGRAM_STACK_GROW_SIZE, stack.committed_base - (stack.base + stack.guard_size));
    if (desired_base > fault_page) desired_base = fault_page;
    if (desired_base < stack.base + stack.guard_size) desired_base = stack.base + stack.guard_size;
    if (desired_base >= stack.committed_base) return false;

    const commit_len = stack.committed_base - desired_base;
    if (!systemCommitLimitAllows(commit_len)) return false;
    const old_guard_base = stack.guard_base;
    const old_guard_size = stack.guard_size;
    mem_virt.clearGuard(stack.range_id) catch return false;
    mem_virt.commit(stack.range_id, desired_base - stack.base, commit_len) catch |err| {
        k.puts("R4X stack grow failed: ");
        k.puts(@errorName(err));
        k.puts("\r\n");
        _ = mem_virt.protectGuard(stack.range_id, old_guard_base - stack.base, old_guard_size) catch {};
        return false;
    };

    const new_guard_base = desired_base - stack.guard_size;
    mem_virt.protectGuard(stack.range_id, new_guard_base - stack.base, stack.guard_size) catch |err| {
        k.puts("R4X stack guard move failed: ");
        k.puts(@errorName(err));
        k.puts("\r\n");
        _ = mem_virt.uncommit(stack.range_id, desired_base - stack.base, commit_len) catch {};
        _ = mem_virt.protectGuard(stack.range_id, old_guard_base - stack.base, old_guard_size) catch {};
        return false;
    };

    poisonProgramStackSpan(desired_base, commit_len);
    stack.committed_base = desired_base;
    stack.committed_size += commit_len;
    stack.guard_base = new_guard_base;
    return true;
}

fn stackFromInstance(instance: *const ProgramInstance) ?ProgramStack {
    if (instance.program_stack_range_id == 0) return null;
    return .{
        .range_id = instance.program_stack_range_id,
        .base = instance.program_stack_base,
        .reserve_size = instance.program_stack_reserve_size,
        .committed_base = instance.program_stack_committed_base,
        .committed_size = instance.program_stack_committed_size,
        .guard_base = instance.program_stack_guard_base,
        .guard_size = instance.program_stack_guard_size,
        .top = instance.stack_top,
        .owner_id = instance.id,
        .profile = instance.memory_profile,
        .initial_commit_size = instance.program_stack_initial_commit_size,
        .create_cycles = instance.program_stack_create_cycles,
        .telemetry_high_water = instance.program_stack_telemetry_high_water,
        .telemetry_committed_pages = instance.program_stack_telemetry_committed_pages,
        .serial_telemetry = instance.program_stack_serial_telemetry,
        .telemetry_measured = instance.program_stack_telemetry_measured,
    };
}

fn writeStackToInstance(instance: *ProgramInstance, stack: ProgramStack) void {
    instance.program_stack_range_id = stack.range_id;
    instance.program_stack_base = stack.base;
    instance.program_stack_reserve_size = stack.reserve_size;
    instance.program_stack_committed_base = stack.committed_base;
    instance.program_stack_committed_size = stack.committed_size;
    instance.program_stack_guard_base = stack.guard_base;
    instance.program_stack_guard_size = stack.guard_size;
    instance.program_stack_initial_commit_size = stack.initial_commit_size;
    instance.program_stack_create_cycles = stack.create_cycles;
    instance.program_stack_telemetry_high_water = stack.telemetry_high_water;
    instance.program_stack_telemetry_committed_pages = stack.telemetry_committed_pages;
    instance.program_stack_serial_telemetry = stack.serial_telemetry;
    instance.program_stack_telemetry_measured = stack.telemetry_measured;
    instance.stack_top = stack.top;
}

fn addressInStackReserve(instance: *const ProgramInstance, addr: u64) bool {
    if (instance.program_stack_range_id == 0 or instance.program_stack_reserve_size == 0) return false;
    const end = instance.program_stack_base + instance.program_stack_reserve_size;
    return addr >= instance.program_stack_base and addr < end;
}

fn addressInProgramStack(stack: *const ProgramStack, addr: u64) bool {
    if (stack.range_id == 0 or stack.reserve_size == 0) return false;
    const end = stack.base + stack.reserve_size;
    return addr >= stack.base and addr < end;
}

fn alignDown(value: u64, alignment: u64) u64 {
    return value & ~(alignment - 1);
}

fn checkedAdd(a: usize, b: usize) ?usize {
    const result = a +% b;
    if (result < a) return null;
    return result;
}

fn checkedAddU64(a: u64, b: u64) ?u64 {
    const result = a +% b;
    if (result < a) return null;
    return result;
}

fn alignUp(value: usize, alignment: usize) ?usize {
    const add = alignment - 1;
    const sum = checkedAdd(value, add) orelse return null;
    return sum & ~add;
}

fn isPowerOfTwo(value: usize) bool {
    return value != 0 and (value & (value - 1)) == 0;
}

fn rollbackRegisteredProgramReservation(reservation: *const ProgramInstanceReservation, reservation_active: *bool) void {
    const handle = programHandleForReservation(reservation);
    while (!releaseThreadsForHandle(handle)) scheduler.yield();
    rollbackReservedProgramInstance(reservation);
    reservation_active.* = false;
}

fn cancelPublishedSpawn(handle: ProgramProcessHandle) void {
    while (!programCompletionIsReady(handle) and !beginProgramExit(handle, -9, PROGRAM_EXIT_REASON_KILLED)) scheduler.yield();
}

fn runForegroundProgram(reservation: *const ProgramInstanceReservation, reservation_active: *bool, loaded: LoadedProgram, stack: ProgramStack, app_class: AppClass, args: []const u8, working_drive: *drive.Drive, options: ProgramLaunchOptions) RunResult {
    reapFinishedInstances();
    if (programSpawnTransactionCancelled()) {
        freeProgramResources(loaded.image, stack);
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_TASK_FAILED);
        return .failed;
    }
    if (foreground_instance_id != null) {
        k.puts("Program already running\r\n");
        last_exit_code = 1;
        freeProgramResources(loaded.image, stack);
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_WOULD_BLOCK);
        return .failed;
    }
    const inherited_console_id = inheritedConsoleTargetId(app_class);

    const instance = createInstance(reservation, .foreground, app_class, loaded, stack, args, working_drive, loaded.origin[0..loaded.origin_len]) orelse {
        k.puts("Program instance allocation failed\r\n");
        last_exit_code = 1;
        freeProgramResources(loaded.image, stack);
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_NO_MEMORY);
        return .failed;
    };
    if (consumeProgramLifecycleFailure(.storage)) {
        rollbackReservedProgramInstance(reservation);
        reservation_active.* = false;
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_NO_MEMORY);
        return .failed;
    }
    if (programSpawnTransactionCancelled()) {
        rollbackReservedProgramInstance(reservation);
        reservation_active.* = false;
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_TASK_FAILED);
        return .failed;
    }
    if (inherited_console_id != 0) {
        const console = consolePayload(instance);
        console.io_target_id = inherited_console_id;
        if (programHandleForId(inherited_console_id)) |target_handle| {
            console.io_target_generation = target_handle.generation;
            if (consoleTargetByHandle(target_handle)) |host| console.host = consolePayloadConst(host).host;
        }
    }
    // Foreground programs share the interactive shell/console transaction
    // and therefore retain the BSP legacy boundary. Detached console jobs
    // below are the first audited R4X class admitted to AP runqueues.
    const program_task = task.createLegacyThreadBlocked("r4x-program", programTaskMain) orelse {
        rollbackReservedProgramInstance(reservation);
        reservation_active.* = false;
        k.puts("Program task create failed\r\n");
        last_exit_code = 1;
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_TASK_FAILED);
        return .failed;
    };
    if (consumeProgramLifecycleFailure(.task)) {
        _ = releaseCreatedProgramTask(program_task);
        rollbackReservedProgramInstance(reservation);
        reservation_active.* = false;
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_TASK_FAILED);
        return .failed;
    }
    instance.task_id = program_task.id;
    if (registerMainThread(instance, program_task) == null) {
        _ = releaseCreatedProgramTask(program_task);
        rollbackReservedProgramInstance(reservation);
        reservation_active.* = false;
        k.puts("Program main thread registration failed\r\n");
        last_exit_code = 1;
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_TASK_FAILED);
        return .failed;
    }
    if (programSpawnTransactionCancelled()) {
        rollbackRegisteredProgramReservation(reservation, reservation_active);
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_TASK_FAILED);
        return .failed;
    }
    if (consumeProgramLifecycleFailure(.publish) or !publishProgramInstance(reservation)) {
        rollbackRegisteredProgramReservation(reservation, reservation_active);
        k.puts("Program registry publish failed\r\n");
        last_exit_code = 1;
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_TASK_FAILED);
        return .failed;
    }
    reservation_active.* = false;
    const handle = programHandleForSlot(reservation.slot);
    if (programSpawnTransactionCancelled()) {
        cancelPublishedSpawn(handle);
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_TASK_FAILED);
        abandonProgramCompletion(handle);
        return .failed;
    }
    if (options.out_handle) |out_handle| out_handle.* = handle;
    foreground_instance_id = instance.id;
    foreground_instance_generation = handle.generation;
    const report_boot_lifecycle = bootscreen.isActive();
    task.markReady(program_task, timer.tickCount());

    while (true) {
        if (programCompletionIsReady(handle)) {
            if (report_boot_lifecycle) bootscreen.setDetail("SERVMAN: Rueckgabe");
            var completion: ProgramProcessCompletion = .{};
            if (consumeProgramCompletion(handle, &completion) == PROGRAM_HANDLE_OK) {
                last_exit_code = completion.exit_code;
                last_display_used = (completion.flags & PROGRAM_COMPLETION_FLAG_DISPLAY_USED) != 0;
            }
            if (report_boot_lifecycle) bootscreen.setDetail("Dienstmanager fertig");
            return .ran;
        }
        if (programSpawnTransactionCancelled()) {
            cancelPublishedSpawn(handle);
            setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_TASK_FAILED);
            abandonProgramCompletion(handle);
            return .failed;
        }
        scheduler.yield();
    }
}

fn runShellProgram(reservation: *const ProgramInstanceReservation, reservation_active: *bool, loaded: LoadedProgram, stack: ProgramStack, app_class: AppClass, args: []const u8, working_drive: *drive.Drive, host: ConsoleHostKind, options: ProgramLaunchOptions) RunResult {
    reportBootLaunchStage(options.report_boot_launch, "Altlasten pruefen");
    reapFinishedInstances();
    if (programSpawnTransactionCancelled()) {
        freeProgramResources(loaded.image, stack);
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_TASK_FAILED);
        return .failed;
    }
    if (shell_instance_id != null) {
        k.puts("Shell already running\r\n");
        freeProgramResources(loaded.image, stack);
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_WOULD_BLOCK);
        return .failed;
    }

    reportBootLaunchStage(options.report_boot_launch, "Instanz anlegen");
    const instance = createInstance(reservation, .shell, app_class, loaded, stack, args, working_drive, loaded.origin[0..loaded.origin_len]) orelse {
        k.puts("Shell instance allocation failed\r\n");
        freeProgramResources(loaded.image, stack);
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_NO_MEMORY);
        return .failed;
    };
    if (consumeProgramLifecycleFailure(.storage)) {
        rollbackReservedProgramInstance(reservation);
        reservation_active.* = false;
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_NO_MEMORY);
        return .failed;
    }
    if (programSpawnTransactionCancelled()) {
        rollbackReservedProgramInstance(reservation);
        reservation_active.* = false;
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_TASK_FAILED);
        return .failed;
    }
    if (app_class == .console) {
        const console = consolePayload(instance);
        console.host = host;
        if (host != .none) {
            const cols = k.consoleCols();
            const rows = k.consoleRows();
            console.state.cols = clampConsoleMetric(if (cols == 0) 80 else cols, CONSOLE_MIN_COLS, CONSOLE_MAX_COLS);
            console.state.rows = clampConsoleMetric(if (rows == 0) 25 else rows, CONSOLE_MIN_ROWS, CONSOLE_MAX_ROWS);
            console.revision = 1;
        }
    }
    // The persistent shell owns global console and child-lifecycle state.
    reportBootLaunchStage(options.report_boot_launch, "Task anlegen");
    const shell_task = task.createLegacyThreadBlocked("r4x-shell", shellTaskMain) orelse {
        rollbackReservedProgramInstance(reservation);
        reservation_active.* = false;
        k.puts("Shell task create failed\r\n");
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_TASK_FAILED);
        return .failed;
    };
    if (consumeProgramLifecycleFailure(.task)) {
        _ = releaseCreatedProgramTask(shell_task);
        rollbackReservedProgramInstance(reservation);
        reservation_active.* = false;
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_TASK_FAILED);
        return .failed;
    }
    instance.task_id = shell_task.id;
    reportBootLaunchStage(options.report_boot_launch, "Thread binden");
    if (registerMainThread(instance, shell_task) == null) {
        _ = releaseCreatedProgramTask(shell_task);
        rollbackReservedProgramInstance(reservation);
        reservation_active.* = false;
        k.puts("Shell main thread registration failed\r\n");
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_TASK_FAILED);
        return .failed;
    }
    if (programSpawnTransactionCancelled()) {
        rollbackRegisteredProgramReservation(reservation, reservation_active);
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_TASK_FAILED);
        return .failed;
    }
    reportBootLaunchStage(options.report_boot_launch, "Instanz veroeffentlichen");
    if (consumeProgramLifecycleFailure(.publish) or !publishProgramInstance(reservation)) {
        rollbackRegisteredProgramReservation(reservation, reservation_active);
        k.puts("Shell registry publish failed\r\n");
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_TASK_FAILED);
        return .failed;
    }
    reservation_active.* = false;
    const handle = programHandleForSlot(reservation.slot);
    if (programSpawnTransactionCancelled()) {
        cancelPublishedSpawn(handle);
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_TASK_FAILED);
        return .failed;
    }
    if (options.out_handle) |out_handle| out_handle.* = handle;
    shell_instance_id = instance.id;
    shell_instance_generation = handle.generation;
    reportBootLaunchStage(options.report_boot_launch, "Task einreihen");
    task.markReady(shell_task, timer.tickCount());
    return .ran;
}

fn runBackgroundProgram(reservation: *const ProgramInstanceReservation, reservation_active: *bool, loaded: LoadedProgram, stack: ProgramStack, app_class: AppClass, args: []const u8, working_drive: *drive.Drive, host: ConsoleHostKind, options: ProgramLaunchOptions) RunResult {
    reapFinishedInstances();
    if (programSpawnTransactionCancelled()) {
        freeProgramResources(loaded.image, stack);
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_TASK_FAILED);
        return .failed;
    }
    const inherited_console_id = if (host == .none) inheritedConsoleTargetId(app_class) else 0;
    if (host != .none and app_class != .console) {
        k.puts("Console host requested for non-console program\r\n");
        last_exit_code = 1;
        freeProgramResources(loaded.image, stack);
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_INVALID);
        return .failed;
    }
    const instance = createInstance(reservation, .background, app_class, loaded, stack, args, working_drive, loaded.origin[0..loaded.origin_len]) orelse {
        k.puts("Program instance allocation failed\r\n");
        last_exit_code = 1;
        freeProgramResources(loaded.image, stack);
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_NO_MEMORY);
        return .failed;
    };
    if (consumeProgramLifecycleFailure(.storage)) {
        rollbackReservedProgramInstance(reservation);
        reservation_active.* = false;
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_NO_MEMORY);
        return .failed;
    }
    if (programSpawnTransactionCancelled()) {
        rollbackReservedProgramInstance(reservation);
        reservation_active.* = false;
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_TASK_FAILED);
        return .failed;
    }
    const console = if (app_class == .console) consolePayload(instance) else null;
    if (console) |payload| payload.host = host;
    if (inherited_console_id != 0) {
        const payload = console.?;
        payload.io_target_id = inherited_console_id;
        if (programHandleForId(inherited_console_id)) |target_handle| {
            payload.io_target_generation = target_handle.generation;
            if (consoleTargetByHandle(target_handle)) |console_target| payload.host = consolePayloadConst(console_target).host;
        }
    }
    if (host != .none) {
        const cols = k.consoleCols();
        const rows = k.consoleRows();
        const payload = console.?;
        payload.state.cols = clampConsoleMetric(if (cols == 0) 80 else cols, CONSOLE_MIN_COLS, CONSOLE_MAX_COLS);
        payload.state.rows = clampConsoleMetric(if (rows == 0) 25 else rows, CONSOLE_MIN_ROWS, CONSOLE_MAX_ROWS);
        payload.revision = 1;
    }

    // Global program/console owners remain on the BSP. LSTRX is the first
    // explicitly audited CPU-only R4X container: the Test-profile loader
    // probe starts four ordinary instances and verifies their full teardown.
    // This allow-list is internal ownership policy, not a public affinity ABI.
    const smp_audited = app_class == .console and isSmpAuditedBackgroundR4x(loaded);
    const program_task = (if (smp_audited)
        task.createParallelThreadBlocked("r4x-app", programTaskMain)
    else
        task.createLegacyThreadBlocked(if (app_class == .service) "r4x-service" else "r4x-app", programTaskMain)) orelse {
        rollbackReservedProgramInstance(reservation);
        reservation_active.* = false;
        k.puts("Program task create failed\r\n");
        last_exit_code = 1;
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_TASK_FAILED);
        return .failed;
    };
    if (consumeProgramLifecycleFailure(.task)) {
        _ = releaseCreatedProgramTask(program_task);
        rollbackReservedProgramInstance(reservation);
        reservation_active.* = false;
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_TASK_FAILED);
        return .failed;
    }
    instance.task_id = program_task.id;
    if (registerMainThread(instance, program_task) == null) {
        _ = releaseCreatedProgramTask(program_task);
        rollbackReservedProgramInstance(reservation);
        reservation_active.* = false;
        k.puts("Program main thread registration failed\r\n");
        last_exit_code = 1;
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_TASK_FAILED);
        return .failed;
    }
    if (programSpawnTransactionCancelled()) {
        rollbackRegisteredProgramReservation(reservation, reservation_active);
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_TASK_FAILED);
        return .failed;
    }
    if (consumeProgramLifecycleFailure(.publish) or !publishProgramInstance(reservation)) {
        rollbackRegisteredProgramReservation(reservation, reservation_active);
        k.puts("Program registry publish failed\r\n");
        last_exit_code = 1;
        setProgramLaunchError(options, PROGRAM_HANDLE_ERROR_TASK_FAILED);
        return .failed;
    }
    reservation_active.* = false;
    const handle = programHandleForSlot(reservation.slot);
    // Publish is the background detach boundary.  A parent killed after this
    // commit still starts the child; leaveProgramSpawnTransaction only
    // orphans caller-owned observation state.  Pre-Publish cancellation above
    // remains a complete rollback.
    if (app_class == .gui and options.out_handle != null) {
        guiPayload(instance).start_attach_pending = true;
    }
    if (options.out_handle) |out_handle| out_handle.* = handle;
    task.markReady(program_task, timer.tickCount());
    return .ran;
}

fn isSmpAuditedBackgroundR4x(loaded: LoadedProgram) bool {
    const origin = loaded.origin[0..loaded.origin_len];
    return std.ascii.eqlIgnoreCase(origin, "C:\\R4OS\\SOFTWARE\\TERMINAL\\DIAG\\LSTRX.R4X");
}

fn programTaskMain() callconv(.c) void {
    const thread_ctx = currentProgramThread() orelse {
        scheduler.exitCurrentAndRetire();
        return;
    };
    const run = pinProgramThreadExecution(thread_ctx) orelse {
        markProgramThreadDone(thread_ctx, THREAD_ERROR_NO_INSTANCE);
        scheduler.exitCurrentAndRetire();
        return;
    };
    const handle = ProgramProcessHandle{ .instance_id = thread_ctx.instance_id, .reserved = 0, .generation = thread_ctx.instance_generation };
    const report_boot_lifecycle = bootscreen.isActive() and
        foreground_instance_id != null and foreground_instance_id.? == handle.instance_id and
        foreground_instance_generation == handle.generation;
    markProgramHandleRunning(handle);
    markProgramThreadRunning(thread_ctx);
    // Handle-based GUI spawning is a two-step host transaction: Desktop first
    // receives the process handle and then attaches its window slot. Do not let
    // user code observe the intermediate, unhosted state. Other launch modes
    // never set this flag and retain their established start behaviour.
    if (run.app_class == .gui and guiPayloadConst(run).start_attach_pending) program_attach_wait_events +%= 1;
    while (run.app_class == .gui and guiPayloadConst(run).start_attach_pending) {
        if (run.close_requested) break;
        scheduler.yield();
    }
    {
        const exit_code = callInstanceEntry(run);
        if (report_boot_lifecycle) bootscreen.setDetail("SERVMAN: Programmende");
        markProgramThreadDone(thread_ctx, exit_code);
        if (report_boot_lifecycle) bootscreen.setDetail("SERVMAN: Threadende");
        markInstanceDone(handle, exit_code);
        if (report_boot_lifecycle) bootscreen.setDetail("SERVMAN: Instanzende");
    }
    if (report_boot_lifecycle) bootscreen.setDetail("SERVMAN: Freigabe");
    unpinProgramThreadExecution(thread_ctx);
    if (report_boot_lifecycle) bootscreen.setDetail("SERVMAN: Freigegeben");
    scheduler.exitCurrentAndRetire();
}

fn shellTaskMain() callconv(.c) void {
    const thread_ctx = currentProgramThread() orelse {
        scheduler.exitCurrentAndRetire();
        return;
    };
    const report_boot_launch = bootscreen.isActive() and
        shell_instance_id != null and shell_instance_id.? == thread_ctx.instance_id and
        shell_instance_generation == thread_ctx.instance_generation;
    reportBootLaunchStage(report_boot_launch, "Shelltask gestartet");
    const run = pinProgramThreadExecution(thread_ctx) orelse {
        markProgramThreadDone(thread_ctx, THREAD_ERROR_NO_INSTANCE);
        scheduler.exitCurrentAndRetire();
        return;
    };
    reportBootLaunchStage(report_boot_launch, "Instanz gebunden");
    const handle = ProgramProcessHandle{ .instance_id = thread_ctx.instance_id, .reserved = 0, .generation = thread_ctx.instance_generation };
    markProgramHandleRunning(handle);
    markProgramThreadRunning(thread_ctx);
    {
        reportBootLaunchStage(report_boot_launch, "R4XStart aufrufen");
        const exit_code = callInstanceEntry(run);
        markProgramThreadDone(thread_ctx, exit_code);
        markInstanceDone(handle, exit_code);
        k.puts("Shell program returned\r\n");
    }
    unpinProgramThreadExecution(thread_ctx);
    scheduler.exitCurrentAndRetire();
}

fn programThreadTaskMain() callconv(.c) void {
    const thread_ctx = currentProgramThreadReady() orelse {
        scheduler.exitCurrentAndRetire();
        return;
    };
    const instance = pinProgramThreadExecution(thread_ctx) orelse {
        markProgramThreadDone(thread_ctx, THREAD_ERROR_NO_INSTANCE);
        scheduler.exitCurrentAndRetire();
        return;
    };
    const entry = normalizeThreadEntry(instance, thread_ctx.entry) orelse {
        markProgramThreadDone(thread_ctx, THREAD_ERROR_INVALID);
        unpinProgramThreadExecution(thread_ctx);
        scheduler.exitCurrentAndRetire();
        return;
    };
    thread_ctx.entry = entry;
    markProgramThreadRunning(thread_ctx);
    const exit_code = r4os_call_program(entry, thread_ctx.arg, thread_ctx.stack.top);
    measureProgramStackHighWater(&thread_ctx.stack);
    if (thread_ctx.stack.serial_telemetry) logProgramStackHighWater(instance, &thread_ctx.stack, thread_ctx.id);
    markProgramThreadDone(thread_ctx, exit_code);
    unpinProgramThreadExecution(thread_ctx);
    scheduler.exitCurrentAndRetire();
}

fn callInstanceEntry(run: *ProgramInstance) i32 {
    const entry = normalizeThreadEntry(run, run.entry) orelse return THREAD_ERROR_INVALID;
    run.entry = entry;
    prepareR4XStartContext(run);
    const process = processPayloadConst(run);
    if (parseSubsystemTrace(process.args[0..cStringLen(process.args[0..])])) |trace| {
        logSubsystemTracePhase(trace, "r4xstart", traceNowNanoseconds());
    }
    program_entries_started +%= 1;
    const exit_code = r4os_call_program(entry, @intFromPtr(&runtimePayload(run).r4xstart_context), run.stack_top);
    if (stackFromInstance(run)) |initial_stack| {
        var measured_stack = initial_stack;
        measureProgramStackHighWater(&measured_stack);
        writeStackToInstance(run, measured_stack);
        if (measured_stack.serial_telemetry) logProgramStackHighWater(run, &measured_stack, 0);
    }
    return exit_code;
}

fn logProgramStackHighWater(instance: *const ProgramInstance, stack: *const ProgramStack, thread_id: u32) void {
    const runtime = runtimePayloadConst(instance);
    const module_len: usize = @min(@as(usize, @intCast(runtime.module_path_len)), runtime.module_path.len);
    k.puts("[R4XSTACK] highwater owner=");
    k.putDec(instance.id);
    k.puts(" thread=");
    k.putDec(thread_id);
    k.puts(" module=");
    if (module_len == 0) k.puts("unknown") else k.puts(runtime.module_path[0..module_len]);
    k.puts(" profile=");
    k.puts(memoryProfileName(stack.profile));
    k.puts(" reserve=");
    k.putDec(stack.reserve_size);
    k.puts(" initial=");
    k.putDec(stack.initial_commit_size);
    k.puts(" committed=");
    k.putDec(stack.committed_size);
    k.puts(" highwater=");
    k.putDec(stack.telemetry_high_water);
    k.puts(" create_cycles=");
    k.putDec(stack.create_cycles);
    k.puts("\r\n");
}

fn apiFileRead(path: [*:0]const u8, out: [*]u8, max_len: u32) callconv(.c) i32 {
    var request_id: u32 = 0;
    const submit = apiIoFileRead(path, out, max_len, 0, &request_id);
    if (submit != IO_OK) return submit;
    return waitAndCloseIoRequest(request_id);
}

fn apiFileWrite(path: [*:0]const u8, data: [*]const u8, len: u32) callconv(.c) i32 {
    var request_id: u32 = 0;
    const submit = apiIoFileWrite(path, data, len, 0, &request_id);
    if (submit != IO_OK) return submit;
    return waitAndCloseIoRequest(request_id);
}

fn apiFileReadAt(path: [*:0]const u8, offset: u32, out: [*]u8, max_len: u32) callconv(.c) i32 {
    var request_id: u32 = 0;
    const submit = apiIoFileReadAt(path, offset, out, max_len, 0, &request_id);
    if (submit != IO_OK) return submit;
    return waitAndCloseIoRequest(request_id);
}

fn apiFileAppend(path: [*:0]const u8, data: [*]const u8, len: u32) callconv(.c) i32 {
    var request_id: u32 = 0;
    const submit = apiIoFileAppend(path, data, len, 0, &request_id);
    if (submit != IO_OK) return submit;
    return waitAndCloseIoRequest(request_id);
}

fn apiFileStreamBegin(path: [*:0]const u8, flags: u32) callconv(.c) i32 {
    var request_id: u32 = 0;
    const submit = apiIoFileStreamBegin(path, flags, &request_id);
    if (submit != IO_OK) return submit;
    return waitAndCloseIoRequest(request_id);
}

fn apiFileStreamWrite(path: [*:0]const u8, offset: u64, data: [*]const u8, len: u32, flags: u32) callconv(.c) i32 {
    var request_id: u32 = 0;
    const submit = apiIoFileStreamWrite(path, offset, data, len, flags, &request_id);
    if (submit != IO_OK) return submit;
    return waitAndCloseIoRequest(request_id);
}

fn apiFileStreamFinish(path: [*:0]const u8, expected_size: u64, flags: u32) callconv(.c) i32 {
    var request_id: u32 = 0;
    const submit = apiIoFileStreamFinish(path, expected_size, flags, &request_id);
    if (submit != IO_OK) return submit;
    return waitAndCloseIoRequest(request_id);
}

fn apiFileStreamAbort(path: [*:0]const u8) callconv(.c) i32 {
    var request_id: u32 = 0;
    const submit = apiIoFileStreamAbort(path, &request_id);
    if (submit != IO_OK) return submit;
    return waitAndCloseIoRequest(request_id);
}

fn apiIoFileRead(path: [*:0]const u8, out: [*]u8, max_len: u64, flags: u32, out_request_id: *u32) callconv(.c) i32 {
    return submitAsyncFileRequest(.file_read, path, 0, 0, 0, @intFromPtr(out), max_len, flags, out_request_id);
}

fn apiIoFileReadAt(path: [*:0]const u8, offset: u64, out: [*]u8, max_len: u64, flags: u32, out_request_id: *u32) callconv(.c) i32 {
    return submitAsyncFileRequest(.file_read_at, path, offset, 0, 0, @intFromPtr(out), max_len, flags, out_request_id);
}

fn apiIoFileWrite(path: [*:0]const u8, data: [*]const u8, len: u64, flags: u32, out_request_id: *u32) callconv(.c) i32 {
    return submitAsyncFileRequest(.file_write, path, 0, @intFromPtr(data), len, 0, 0, flags, out_request_id);
}

fn apiIoFileAppend(path: [*:0]const u8, data: [*]const u8, len: u64, flags: u32, out_request_id: *u32) callconv(.c) i32 {
    return submitAsyncFileRequest(.file_append, path, 0, @intFromPtr(data), len, 0, 0, flags, out_request_id);
}

fn apiIoFileWriteAt(path: [*:0]const u8, offset: u64, data: [*]const u8, len: u64, flags: u32, out_request_id: *u32) callconv(.c) i32 {
    return submitAsyncFileRequest(.file_write_at, path, offset, @intFromPtr(data), len, 0, 0, flags, out_request_id);
}

fn apiIoFileInfo(path: [*:0]const u8, flags: u32, out_request_id: *u32) callconv(.c) i32 {
    return submitAsyncFileRequest(.file_info, path, 0, 0, 0, 0, 0, flags, out_request_id);
}

fn apiIoFileLock(path: [*:0]const u8, offset: u64, length: u64, flags: u32, out_request_id: *u32) callconv(.c) i32 {
    return submitAsyncFileRequest(.file_lock, path, offset, 0, length, 0, 0, flags, out_request_id);
}

fn apiIoFileStreamBegin(path: [*:0]const u8, flags: u32, out_request_id: *u32) callconv(.c) i32 {
    return submitAsyncFileRequest(.file_stream_begin, path, 0, 0, 0, 0, 0, flags, out_request_id);
}

fn apiIoFileStreamWrite(path: [*:0]const u8, offset: u64, data: [*]const u8, len: u64, flags: u32, out_request_id: *u32) callconv(.c) i32 {
    return submitAsyncFileRequest(.file_stream_write, path, offset, @intFromPtr(data), len, 0, 0, flags, out_request_id);
}

fn apiIoFileStreamFinish(path: [*:0]const u8, expected_size: u64, flags: u32, out_request_id: *u32) callconv(.c) i32 {
    return submitAsyncFileRequest(.file_stream_finish, path, expected_size, 0, 0, 0, 0, flags, out_request_id);
}

fn apiIoFileStreamAbort(path: [*:0]const u8, out_request_id: *u32) callconv(.c) i32 {
    return submitAsyncFileRequest(.file_stream_abort, path, 0, 0, 0, 0, 0, 0, out_request_id);
}

fn apiIoServiceCall(handle: u32, op: u16, request_ptr: [*]const u8, request_len: u32, response_header: *ServiceMessageHeader, response_ptr: [*]u8, response_capacity: u32, timeout_ticks: u64, flags: u32, out_request_id: *u32) callconv(.c) i32 {
    if (@intFromPtr(out_request_id) == 0) return IO_ERROR_INVALID;
    out_request_id.* = 0;
    if ((flags & ~IO_FLAGS_SUPPORTED) != 0) return IO_ERROR_UNSUPPORTED;
    if (@intFromPtr(response_header) == 0) return services.API_ERR_INVALID;
    response_header.* = .{ .magic = 0, .version = 0 };
    if (request_len > services.API_MAX_PAYLOAD) return services.API_ERR_PAYLOAD_TOO_LARGE;
    if (response_capacity > services.API_MAX_PAYLOAD) return services.API_ERR_INVALID;
    if (request_len != 0 and @intFromPtr(request_ptr) == 0) return services.API_ERR_INVALID;
    if (@intFromPtr(response_ptr) == 0) return services.API_ERR_INVALID;
    const instance = currentInstance() orelse return IO_ERROR_NO_INSTANCE;
    if (instance.done) return IO_ERROR_NO_INSTANCE;
    const instance_handle = currentProgramHandle() orelse return IO_ERROR_NO_INSTANCE;
    const caller_thread = currentProgramThread() orelse return IO_ERROR_NO_INSTANCE;

    if (!lockAsyncIoRequests()) return IO_ERROR_BUSY;
    const req = allocAsyncIoRequestSlot() orelse {
        unlockAsyncIoRequests();
        return IO_ERROR_NO_SLOTS;
    };
    req.* = .{
        .used = true,
        .id = allocateAsyncIoId(),
        .owner_instance_id = instance.id,
        .owner_instance_generation = instance_handle.generation,
        .owner_instance = instance,
        .caller_task_id = caller_thread.task_id,
        .caller_task_generation = caller_thread.task_generation,
        .kind = .service_call,
        .state = .pending,
        .flags = flags,
        .requested_bytes = request_len,
        .submitted_tick = timer.tickCount(),
        .completion = sync.Completion.init(),
        .service_handle = handle,
        .service_op = op,
        .service_request_ptr = @intFromPtr(request_ptr),
        .service_request_len = request_len,
        .service_response_header_ptr = @intFromPtr(response_header),
        .service_response_ptr = @intFromPtr(response_ptr),
        .service_response_capacity = response_capacity,
        .service_timeout_ticks = timeout_ticks,
    };
    const worker = task.createKernelWorkerBlockedWithRole("r4x-async-io", asyncIoTaskMain, .short_completion) orelse {
        req.* = .{};
        unlockAsyncIoRequests();
        return IO_ERROR_SPAWN_FAILED;
    };
    if (!task.bindExecutionOwner(worker, .async_io, @ptrCast(req))) {
        const worker_id = worker.id;
        const worker_generation = worker.generation;
        req.* = .{};
        unlockAsyncIoRequests();
        _ = task.retireIdentity(worker_id, worker_generation);
        return IO_ERROR_SPAWN_FAILED;
    }
    req.task_id = worker.id;
    req.task_generation = worker.generation;
    out_request_id.* = req.id;
    // The request mutex is also the publication boundary for owner cancel.
    // Make the worker runnable before releasing it so cancellation can never
    // retire the exact Task between unlock and this raw-pointer access.
    task.markReady(worker, timer.tickCount());
    unlockAsyncIoRequests();
    return IO_OK;
}

fn apiIoStatus(request_id: u32, out: *ProgramIoInfo) callconv(.c) i32 {
    if (@intFromPtr(out) == 0) return IO_ERROR_INVALID;
    if (!lockAsyncIoRequests()) return IO_ERROR_BUSY;
    const req = asyncIoRequestForCaller(request_id) orelse {
        unlockAsyncIoRequests();
        return IO_ERROR_NOT_FOUND;
    };
    const info = asyncIoInfo(req);
    unlockAsyncIoRequests();
    out.* = info;
    return IO_OK;
}

fn apiIoWait(request_id: u32, timeout_ticks: u64, out: *ProgramIoInfo) callconv(.c) i32 {
    if (@intFromPtr(out) == 0) return IO_ERROR_INVALID;
    if (!lockAsyncIoRequests()) return IO_ERROR_BUSY;
    var req = asyncIoRequestForCaller(request_id) orelse {
        unlockAsyncIoRequests();
        return IO_ERROR_NOT_FOUND;
    };
    if (req.state != .pending and req.state != .running) {
        // A fast request can complete before its submitter reaches IoWait.
        // Return that stable result directly while still owning the request
        // lock; the former shared tail unlocked this already-released lock a
        // second time when no Completion wait was needed.
        const info = asyncIoInfo(req);
        const status = req.status;
        unlockAsyncIoRequests();
        out.* = info;
        return status;
    }
    req.waiters +|= 1;
    const completion = &req.completion;
    unlockAsyncIoRequests();

    const wait_result = completion.wait(timeout_ticks);
    if (!lockAsyncIoRequests()) return IO_ERROR_BUSY;
    req = asyncIoRequestForCaller(request_id) orelse {
        unlockAsyncIoRequests();
        return IO_ERROR_NOT_FOUND;
    };
    if (req.waiters != 0) req.waiters -= 1;
    const completed = req.state != .pending and req.state != .running;
    if (wait_result == .timeout and !completed) {
        unlockAsyncIoRequests();
        return IO_ERROR_TIMEOUT;
    }
    if ((wait_result == .cancelled or wait_result == .killed) and !completed) {
        unlockAsyncIoRequests();
        return IO_ERROR_CANCELLED;
    }
    if (wait_result != .signaled and !completed) {
        unlockAsyncIoRequests();
        return IO_ERROR_BUSY;
    }
    const info = asyncIoInfo(req);
    const status = req.status;
    unlockAsyncIoRequests();
    out.* = info;
    return status;
}

fn apiIoClose(request_id: u32) callconv(.c) i32 {
    return closeAsyncIoRequest(request_id, true);
}

fn closeAsyncIoRequest(request_id: u32, defer_on_busy: bool) i32 {
    if (!lockAsyncIoRequests()) return IO_ERROR_BUSY;
    const req = asyncIoRequestForCaller(request_id) orelse {
        unlockAsyncIoRequests();
        return IO_ERROR_NOT_FOUND;
    };
    if (req.state == .pending or req.state == .running or req.waiters != 0) {
        unlockAsyncIoRequests();
        return IO_ERROR_BUSY;
    }
    const slot = asyncIoRequestIndexLocked(req) orelse {
        unlockAsyncIoRequests();
        return IO_ERROR_NOT_FOUND;
    };
    const claim = asyncIoRetireClaim(slot, req);
    const retry_injection = consumeAsyncIoRetireRetryForTest(req);
    unlockAsyncIoRequests();
    // Dynamic Task retirement can release stack/VM/heap storage and may
    // yield. Keep the exact request slot as a retry anchor, but never hold the
    // global async-I/O mutex across that teardown transaction.
    if (retry_injection != .none) {
        if (retry_injection == .first) {
            k.puts("ASYNCIO05910 retire retry: injected request=");
            k.putDec(claim.request_id);
            k.puts(" attempts=");
            k.putDec(ASYNC_IO_RETIRE_RETRY_TEST_ATTEMPTS);
            k.puts("\r\n");
        }
        return IO_ERROR_BUSY;
    }
    if (!retireAsyncIoTask(claim.task_id, claim.task_generation)) {
        if (!defer_on_busy) return IO_ERROR_BUSY;
        return switch (deferAsyncIoCloseClaim(claim)) {
            .busy => IO_ERROR_BUSY,
            .already_closed, .deferred => IO_OK,
        };
    }
    if (!lockAsyncIoRequests()) return IO_ERROR_BUSY;
    if (asyncIoRequestForRetireClaimLocked(claim)) |retired| retired.* = .{};
    unlockAsyncIoRequests();
    if (finishAsyncIoRetireRetryTest(claim.request_id)) {
        k.puts("ASYNCIO05910 retire retry: recovered request=");
        k.putDec(claim.request_id);
        k.puts("\r\n");
    }
    return IO_OK;
}

fn waitAndCloseIoRequest(request_id: u32) i32 {
    var info: ProgramIoInfo = .{};
    const waited = apiIoWait(request_id, sync.WAIT_FOREVER, &info);
    // A dynamic Task release can be temporarily retry-pending after the I/O
    // result is ready. Synchronous facades own no externally visible request
    // handle: retry for a bounded interval, then transfer that exact anchor
    // durably to the reaper before returning the completed operation result.
    var retries: u32 = 0;
    var closed = closeAsyncIoRequest(request_id, false);
    while (closed == IO_ERROR_BUSY and retries < ASYNC_IO_SYNC_CLOSE_RETRY_LIMIT) {
        retries += 1;
        scheduler.sleepTicksWithReason(1, "async-io-close-retry");
        closed = closeAsyncIoRequest(request_id, false);
    }
    if (closed == IO_ERROR_BUSY) {
        if (!markAsyncIoClosePendingForCaller(request_id)) return IO_ERROR_BUSY;
        k.puts("ASYNCIO05910 sync close: deferred request=");
        k.putDec(request_id);
        k.puts(" retries=");
        k.putDec(ASYNC_IO_SYNC_CLOSE_RETRY_LIMIT);
        k.puts("\r\n");
        if (waited != IO_OK) return waited;
        return info.result;
    }
    if (closed != IO_OK) return closed;
    if (waited != IO_OK) return waited;
    return info.result;
}

fn submitAsyncFileRequest(kind: AsyncIoKind, path: [*:0]const u8, offset: u64, data_ptr: usize, data_len: u64, out_ptr: usize, out_len: u64, flags: u32, out_request_id: *u32) i32 {
    if (@intFromPtr(out_request_id) == 0 or @intFromPtr(path) == 0) return IO_ERROR_INVALID;
    out_request_id.* = 0;
    if (kind == .file_lock) {
        if ((flags & ~IO_FILE_LOCK_FLAG_UNLOCK) != 0 or data_len == 0) return IO_ERROR_INVALID;
    } else if ((flags & ~IO_FLAGS_SUPPORTED) != 0 and kind != .file_stream_begin and kind != .file_stream_write and kind != .file_stream_finish) {
        return IO_ERROR_UNSUPPORTED;
    }
    if ((kind == .file_write or kind == .file_append or kind == .file_write_at or kind == .file_stream_write) and data_len != 0 and data_ptr == 0) return IO_ERROR_INVALID;
    if ((kind == .file_read or kind == .file_read_at) and out_len != 0 and out_ptr == 0) return IO_ERROR_INVALID;
    const instance = currentInstance() orelse return IO_ERROR_NO_INSTANCE;
    if (instance.done) return IO_ERROR_NO_INSTANCE;
    const instance_handle = currentProgramHandle() orelse return IO_ERROR_NO_INSTANCE;
    const caller_thread = currentProgramThread() orelse return IO_ERROR_NO_INSTANCE;

    if (!lockAsyncIoRequests()) return IO_ERROR_BUSY;
    const req = allocAsyncIoRequestSlot() orelse {
        unlockAsyncIoRequests();
        return IO_ERROR_NO_SLOTS;
    };
    req.* = .{
        .used = true,
        .id = allocateAsyncIoId(),
        .owner_instance_id = instance.id,
        .owner_instance_generation = instance_handle.generation,
        .owner_instance = instance,
        .caller_task_id = caller_thread.task_id,
        .caller_task_generation = caller_thread.task_generation,
        .kind = kind,
        .state = .pending,
        .flags = flags,
        .requested_bytes = if (out_len != 0) out_len else data_len,
        .submitted_tick = timer.tickCount(),
        .completion = sync.Completion.init(),
        .offset = offset,
        .data_ptr = data_ptr,
        .data_len = data_len,
        .out_ptr = out_ptr,
        .out_len = out_len,
    };
    if (!copyPathZIntoAsyncRequest(path, req)) {
        req.* = .{};
        unlockAsyncIoRequests();
        return IO_ERROR_INVALID;
    }
    const worker = task.createKernelWorkerBlockedWithRole("r4x-async-io", asyncIoTaskMain, .batch) orelse {
        req.* = .{};
        unlockAsyncIoRequests();
        return IO_ERROR_SPAWN_FAILED;
    };
    if (!task.bindExecutionOwner(worker, .async_io, @ptrCast(req))) {
        const worker_id = worker.id;
        const worker_generation = worker.generation;
        req.* = .{};
        unlockAsyncIoRequests();
        _ = task.retireIdentity(worker_id, worker_generation);
        return IO_ERROR_SPAWN_FAILED;
    }
    req.task_id = worker.id;
    req.task_generation = worker.generation;
    out_request_id.* = req.id;
    // Publish readiness while cancellation is still excluded; after unlock
    // no raw worker pointer is needed by the submitter.
    task.markReady(worker, timer.tickCount());
    unlockAsyncIoRequests();
    return IO_OK;
}

fn taskRegistryAsyncRetireRetryTestAllowed() bool {
    const selftest = boot_config.optionValue(boot_config.get(), "TASKREGISTRY", "selftest") orelse return false;
    if (!std.ascii.eqlIgnoreCase(selftest, "yes")) return false;
    // /RETIRERETRY on the owning diagnostic is the second opt-in. Keep the
    // dedicated boot option as an explicit disable/enable override, but do
    // not depend on it surviving a small legacy CONFIG option table.
    const retry = boot_config.optionValue(boot_config.get(), "TASKREGISTRY", "async-retire-retry") orelse return true;
    return std.ascii.eqlIgnoreCase(retry, "yes");
}

const AsyncIoRetireRetryInjection = enum {
    none,
    first,
    retry,
};

fn consumeAsyncIoRetireRetryForTest(req: *const AsyncIoRequest) AsyncIoRetireRetryInjection {
    // The owning /RETIRERETRY diagnostic is the only caller that writes this
    // private path. Target the copied request data itself: unlike argv, it is
    // part of the retirement anchor and cannot be consumed by an auto-service.
    if (req.kind != .file_write or
        !equalsIgnoreCase(fixedZSpan(req.path[0..]), "C:\\TEMP\\ASYNIOR.TXT")) return .none;
    if (!taskRegistryAsyncRetireRetryTestAllowed()) return .none;
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    if (!async_io_retire_retry_test_armed) {
        if (async_io_retire_retry_test_consumed) return .none;
        async_io_retire_retry_test_armed = true;
        async_io_retire_retry_test_remaining = ASYNC_IO_RETIRE_RETRY_TEST_ATTEMPTS;
    }
    if (async_io_retire_retry_test_request_id == 0) async_io_retire_retry_test_request_id = req.id;
    if (async_io_retire_retry_test_request_id != req.id or async_io_retire_retry_test_remaining == 0) return .none;
    async_io_retire_retry_test_remaining -= 1;
    const first = async_io_retire_retry_test_remaining == ASYNC_IO_SYNC_CLOSE_RETRY_LIMIT;
    if (async_io_retire_retry_test_remaining == 0) {
        async_io_retire_retry_test_armed = false;
        async_io_retire_retry_test_consumed = true;
    }
    return if (first) .first else .retry;
}

fn finishAsyncIoRetireRetryTest(request_id: u32) bool {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    if (request_id == 0 or request_id != async_io_retire_retry_test_request_id) return false;
    async_io_retire_retry_test_request_id = 0;
    return true;
}

const AsyncIoCloseHandoffResult = enum {
    busy,
    already_closed,
    deferred,
};

fn deferAsyncIoCloseClaim(claim: AsyncIoRetireClaim) AsyncIoCloseHandoffResult {
    if (!lockAsyncIoRequests()) return .busy;
    const req = asyncIoRequestForRetireClaimLocked(claim) orelse {
        unlockAsyncIoRequests();
        return .already_closed;
    };
    if (req.state == .pending or req.state == .running or req.waiters != 0) {
        unlockAsyncIoRequests();
        return .busy;
    }
    req.close_pending = true;
    unlockAsyncIoRequests();
    program_reaper_event.signal();
    return .deferred;
}

fn markAsyncIoClosePendingForCaller(request_id: u32) bool {
    if (!lockAsyncIoRequests()) return false;
    const req = asyncIoRequestForCaller(request_id) orelse {
        unlockAsyncIoRequests();
        return false;
    };
    if (req.state == .pending or req.state == .running or req.waiters != 0) {
        unlockAsyncIoRequests();
        return false;
    }
    req.close_pending = true;
    unlockAsyncIoRequests();
    program_reaper_event.signal();
    return true;
}

fn asyncIoTaskMain() callconv(.c) void {
    if (!lockAsyncIoRequests()) scheduler.exitCurrentAndRetire();
    const req = currentAsyncIoRequest() orelse {
        unlockAsyncIoRequests();
        scheduler.exitCurrentAndRetire();
    };
    req.state = .running;
    unlockAsyncIoRequests();

    const result = performAsyncIo(req);
    if (!lockAsyncIoRequests()) scheduler.exitCurrentAndRetire();
    const current_task = scheduler.current();
    if (!req.used or current_task == null or req.task_id != current_task.?.id or req.task_generation != current_task.?.generation) {
        unlockAsyncIoRequests();
        scheduler.exitCurrentAndRetire();
    }
    if (req.cancel_requested) {
        req.result = IO_ERROR_CANCELLED;
        req.status = IO_ERROR_CANCELLED;
        req.processed_bytes = 0;
        req.state = .failed;
    } else {
        req.result = result;
        req.status = IO_OK;
        req.processed_bytes = if (result > 0) @intCast(result) else 0;
        req.state = .completed;
    }
    req.completed_tick = timer.tickCount();
    req.completion.completeAll();
    unlockAsyncIoRequests();
    scheduler.exitCurrentAndRetire();
}

fn performAsyncIo(req: *AsyncIoRequest) i32 {
    if (req.kind != .file_lock and (req.data_len > 0xFFFF_FFFF or req.out_len > 0xFFFF_FFFF)) return IO_ERROR_TOO_LARGE;
    var empty_byte: [1]u8 = .{0};
    const path_ptr: [*:0]const u8 = @ptrCast(req.path[0..].ptr);
    const data_ptr: [*]const u8 = if (req.data_ptr == 0) @ptrCast(empty_byte[0..].ptr) else @ptrFromInt(req.data_ptr);
    const out_ptr: [*]u8 = if (req.out_ptr == 0) @ptrCast(empty_byte[0..].ptr) else @ptrFromInt(req.out_ptr);
    return switch (req.kind) {
        .file_read => r4api.r4sys.fileRead(path_ptr, out_ptr, @intCast(req.out_len)),
        .file_read_at => r4api.r4sys.fileReadAt64(path_ptr, req.offset, out_ptr, @intCast(req.out_len)),
        .file_write => r4api.r4sys.fileWrite(path_ptr, data_ptr, @intCast(req.data_len)),
        .file_append => r4api.r4sys.fileAppend(path_ptr, data_ptr, @intCast(req.data_len)),
        .file_write_at => normalizeAsyncWriteAtResult(r4api.r4sys.fileWriteAt(path_ptr, req.offset, data_ptr, @intCast(req.data_len))),
        .file_info => performAsyncFileInfo(path_ptr),
        .file_lock => performAsyncFileLock(req),
        .file_stream_begin => r4api.r4sys.fileStreamBegin(path_ptr, req.flags),
        .file_stream_write => r4api.r4sys.fileStreamWrite(path_ptr, req.offset, data_ptr, @intCast(req.data_len), req.flags),
        .file_stream_finish => r4api.r4sys.fileStreamFinish(path_ptr, req.offset, req.flags),
        .file_stream_abort => r4api.r4sys.fileStreamAbort(path_ptr),
        .service_call => performAsyncServiceCall(req),
        .none => IO_ERROR_INVALID,
    };
}

fn normalizeAsyncWriteAtResult(result: i32) i32 {
    if (result >= 0) return result;
    return switch (result) {
        -1 => IO_ERROR_INVALID,
        -2, -3 => IO_ERROR_NOT_FOUND,
        -7 => IO_ERROR_BUSY,
        -8 => IO_ERROR_TOO_LARGE,
        else => IO_ERROR_BUSY,
    };
}

fn performAsyncFileInfo(path: [*:0]const u8) i32 {
    var info: r4api.r4sys.FileInfo = .{};
    const result = r4api.r4sys.fileInfo(path, &info);
    if (result == 0) return IO_ERROR_NOT_FOUND;
    if (result < 0) {
        return switch (result) {
            -1 => IO_ERROR_INVALID,
            -2 => IO_ERROR_NOT_FOUND,
            else => IO_ERROR_BUSY,
        };
    }
    if (info.is_dir != 0) return IO_ERROR_INVALID;
    if (info.size > std.math.maxInt(i32)) return IO_ERROR_TOO_LARGE;
    return @intCast(info.size);
}

fn performAsyncFileLock(req: *const AsyncIoRequest) i32 {
    if (req.path_len == 0 or req.data_len == 0 or (req.flags & ~IO_FILE_LOCK_FLAG_UNLOCK) != 0) return IO_ERROR_INVALID;
    if (!file_range_lock.lock(sync.WAIT_FOREVER)) return IO_ERROR_BUSY;
    defer _ = file_range_lock.unlock();

    if ((req.flags & IO_FILE_LOCK_FLAG_UNLOCK) != 0) {
        for (&file_range_locks) |*entry| {
            if (!entry.used or
                entry.owner_instance_id != req.owner_instance_id or
                entry.owner_instance_generation != req.owner_instance_generation or
                entry.offset != req.offset or entry.length != req.data_len or
                !fileRangeLockPathEqual(entry, req)) continue;
            entry.* = .{};
            return IO_OK;
        }
        return IO_ERROR_LOCK_VIOLATION;
    }

    for (&file_range_locks) |*entry| {
        if (!entry.used or !fileRangeLockPathEqual(entry, req)) continue;
        if (entry.owner_instance_id == req.owner_instance_id and
            entry.owner_instance_generation == req.owner_instance_generation) continue;
        if (fileRangesOverlap(entry.offset, entry.length, req.offset, req.data_len)) return IO_ERROR_LOCK_VIOLATION;
    }

    for (&file_range_locks) |*entry| {
        if (entry.used) continue;
        entry.* = .{
            .used = true,
            .owner_instance_id = req.owner_instance_id,
            .owner_instance_generation = req.owner_instance_generation,
            .offset = req.offset,
            .length = req.data_len,
            .path_len = req.path_len,
        };
        @memcpy(entry.path[0..req.path_len], req.path[0..req.path_len]);
        entry.path[req.path_len] = 0;
        return IO_OK;
    }
    return IO_ERROR_NO_SLOTS;
}

fn fileRangesOverlap(a_offset: u64, a_length: u64, b_offset: u64, b_length: u64) bool {
    const a_end = std.math.add(u64, a_offset, a_length) catch std.math.maxInt(u64);
    const b_end = std.math.add(u64, b_offset, b_length) catch std.math.maxInt(u64);
    return a_offset < b_end and b_offset < a_end;
}

fn fileRangeLockPathEqual(entry: *const FileRangeLock, req: *const AsyncIoRequest) bool {
    if (entry.path_len != req.path_len) return false;
    var index: usize = 0;
    while (index < entry.path_len) : (index += 1) {
        if (normalizedFileLockPathByte(entry.path[index]) != normalizedFileLockPathByte(req.path[index])) return false;
    }
    return true;
}

fn normalizedFileLockPathByte(byte: u8) u8 {
    if (byte == '/') return '\\';
    return std.ascii.toUpper(byte);
}

fn releaseFileRangeLocksForHandle(handle: ProgramProcessHandle) bool {
    if (!file_range_lock.lock(sync.WAIT_FOREVER)) return false;
    defer _ = file_range_lock.unlock();
    for (&file_range_locks) |*entry| {
        if (entry.used and
            entry.owner_instance_id == handle.instance_id and
            entry.owner_instance_generation == handle.generation) entry.* = .{};
    }
    return true;
}

fn performAsyncServiceCall(req: *AsyncIoRequest) i32 {
    var empty_request: [1]u8 = .{0};
    var empty_response: [1]u8 = .{0};
    const request_ptr: [*]const u8 = if (req.service_request_len == 0) @ptrCast(empty_request[0..].ptr) else @ptrFromInt(req.service_request_ptr);
    const response_ptr: [*]u8 = if (req.service_response_capacity == 0) @ptrCast(empty_response[0..].ptr) else @ptrFromInt(req.service_response_ptr);
    const response_header: *ServiceMessageHeader = @ptrFromInt(req.service_response_header_ptr);
    const result = serviceCallCore(req, req.service_handle, req.service_op, request_ptr, req.service_request_len, response_header, response_ptr, req.service_response_capacity, req.service_timeout_ticks);
    if (result > 0) req.processed_bytes = @intCast(result);
    return result;
}

fn copyPathZIntoAsyncRequest(path: [*:0]const u8, req: *AsyncIoRequest) bool {
    var len: usize = 0;
    while (len + 1 < req.path.len and path[len] != 0) : (len += 1) {
        req.path[len] = path[len];
    }
    if (path[len] != 0) return false;
    req.path[len] = 0;
    req.path_len = len;
    return true;
}

fn allocAsyncIoRequestSlot() ?*AsyncIoRequest {
    var i: usize = 0;
    while (i < async_io_requests.len) : (i += 1) {
        if (!async_io_requests[i].used) return &async_io_requests[i];
    }
    return null;
}

fn allocateAsyncIoId() u32 {
    const id = next_async_io_id;
    next_async_io_id +%= 1;
    if (next_async_io_id == 0) next_async_io_id = 1;
    return if (id == 0) 1 else id;
}

fn lockAsyncIoRequests() bool {
    return async_io_lock.lock(sync.WAIT_FOREVER);
}

fn unlockAsyncIoRequests() void {
    _ = async_io_lock.unlock();
}

fn currentAsyncIoRequest() ?*AsyncIoRequest {
    const current_task = scheduler.current() orelse return null;
    const owner = task.executionOwner(current_task);
    if (owner.kind != .async_io) return null;
    const req: *AsyncIoRequest = @ptrCast(@alignCast(owner.context orelse return null));
    if (!req.used or req.task_id != current_task.id or req.task_generation != current_task.generation) return null;
    return req;
}

fn asyncIoRequestForCaller(request_id: u32) ?*AsyncIoRequest {
    if (request_id == 0) return null;
    const instance = currentInstance() orelse return null;
    const handle = currentProgramHandle() orelse return null;
    const caller_thread = currentProgramThread();
    var i: usize = 0;
    while (i < async_io_requests.len) : (i += 1) {
        const req = &async_io_requests[i];
        if (!req.used or
            req.close_pending or
            req.id != request_id or
            req.owner_instance_id != instance.id or
            req.owner_instance_generation != handle.generation)
            continue;
        // Stream requests carry namespace ownership across Begin/Write/
        // Finish/Abort and are deliberately scoped to one ProgramThread.
        // Generic file and service requests retain their process-wide handle
        // semantics.
        if (isAsyncStreamKind(req.kind)) {
            const thread_ctx = caller_thread orelse continue;
            if (req.caller_task_id != thread_ctx.task_id or
                req.caller_task_generation != thread_ctx.task_generation)
                continue;
        }
        return req;
    }
    return null;
}

fn asyncIoInfo(req: *const AsyncIoRequest) ProgramIoInfo {
    return .{
        .version = IO_INFO_VERSION,
        .size = @sizeOf(ProgramIoInfo),
        .request_id = req.id,
        .kind = @intFromEnum(req.kind),
        .state = @intFromEnum(req.state),
        .flags = req.flags,
        .status = req.status,
        .result = req.result,
        .requested_bytes = req.requested_bytes,
        .processed_bytes = req.processed_bytes,
        .submitted_tick = req.submitted_tick,
        .completed_tick = req.completed_tick,
        .owner_instance = req.owner_instance_id,
        .task_id = req.task_id,
        .reserved0 = 0,
    };
}

fn cancelAsyncIoRequestsForInstance(instance_id: u32) bool {
    const handle = programHandleForId(instance_id) orelse return true;
    return cancelAsyncIoRequestsForHandle(handle);
}

fn cancelAsyncIoRequestsForHandle(handle: ProgramProcessHandle) bool {
    while (true) {
        if (!lockAsyncIoRequests()) return false;
        var claim: ?AsyncIoRetireClaim = null;
        var i: usize = 0;
        while (i < async_io_requests.len) : (i += 1) {
            const req = &async_io_requests[i];
            if (!asyncIoRequestOwnedByHandle(req, handle) or req.task_id == 0) continue;
            req.cancel_requested = true;
            claim = asyncIoRetireClaim(i, req);
            // Prevent a second cancellation attempt from targeting a request
            // ID that the worker may already have completed and recycled.
            req.service_request_id = 0;
            break;
        }
        unlockAsyncIoRequests();

        const pending = claim orelse return true;
        if (pending.service_request_id != 0) {
            _ = services.cancelRequest(pending.service_handle, pending.service_request_id);
        }
        if (!retireAsyncIoTask(pending.task_id, pending.task_generation)) return false;

        if (!lockAsyncIoRequests()) return false;
        if (asyncIoRequestForRetireClaimLocked(pending)) |req| {
            req.task_id = 0;
            req.task_generation = 0;
            if (req.state == .pending or req.state == .running) {
                req.status = IO_ERROR_CANCELLED;
                req.result = IO_ERROR_CANCELLED;
                req.processed_bytes = 0;
                req.state = .failed;
                req.completed_tick = timer.tickCount();
                req.completion.completeAll();
            }
            // A live waiter owns the embedded Completion address across its
            // wait. Keep the slot until the program tasks are detached; the
            // purge phase drains those killed waiters before reuse.
            if (req.waiters == 0 and !req.completion.queue.hasWaiters()) req.* = .{};
        }
        unlockAsyncIoRequests();
    }
}

fn purgeCancelledAsyncIoRequestsForHandle(handle: ProgramProcessHandle) bool {
    while (true) {
        if (!lockAsyncIoRequests()) return false;
        var claim: ?AsyncIoRetireClaim = null;
        var i: usize = 0;
        while (i < async_io_requests.len) : (i += 1) {
            const req = &async_io_requests[i];
            if (!asyncIoRequestOwnedByHandle(req, handle)) continue;
            req.cancel_requested = true;
            claim = asyncIoRetireClaim(i, req);
            break;
        }
        unlockAsyncIoRequests();

        const pending = claim orelse return true;
        if (!retireAsyncIoTask(pending.task_id, pending.task_generation)) return false;

        if (!lockAsyncIoRequests()) return false;
        if (asyncIoRequestForRetireClaimLocked(pending)) |req| {
            _ = req.completion.queue.cancelAll();
            req.waiters = 0;
            req.* = .{};
        }
        unlockAsyncIoRequests();
    }
}

fn asyncIoRequestOwnedByHandle(req: *const AsyncIoRequest, handle: ProgramProcessHandle) bool {
    return req.used and
        req.owner_instance_id == handle.instance_id and
        req.owner_instance_generation == handle.generation;
}

fn isAsyncStreamKind(kind: AsyncIoKind) bool {
    return kind == .file_stream_begin or
        kind == .file_stream_write or
        kind == .file_stream_finish or
        kind == .file_stream_abort;
}

fn asyncIoRequestOwnedByCaller(
    req: *const AsyncIoRequest,
    handle: ProgramProcessHandle,
    caller_task_id: u32,
    caller_task_generation: u64,
) bool {
    return asyncIoRequestOwnedByHandle(req, handle) and
        req.caller_task_id == caller_task_id and
        req.caller_task_generation == caller_task_generation;
}

fn cancelAsyncIoRequestsForCaller(
    handle: ProgramProcessHandle,
    caller_task_id: u32,
    caller_task_generation: u64,
) bool {
    while (true) {
        if (!lockAsyncIoRequests()) return false;
        var claim: ?AsyncIoRetireClaim = null;
        var i: usize = 0;
        while (i < async_io_requests.len) : (i += 1) {
            const req = &async_io_requests[i];
            if (!asyncIoRequestOwnedByCaller(req, handle, caller_task_id, caller_task_generation)) continue;
            req.cancel_requested = true;
            if (req.task_id == 0) continue;
            claim = asyncIoRetireClaim(i, req);
            // Service requests need their endpoint-side waiter cancelled
            // before the worker task can be retired. Clear the published
            // request id under the same lock so a retry cannot cancel a
            // recycled service request.
            req.service_request_id = 0;
            break;
        }
        unlockAsyncIoRequests();

        const pending = claim orelse return true;
        if (pending.service_request_id != 0) {
            _ = services.cancelRequest(pending.service_handle, pending.service_request_id);
        }
        if (!retireAsyncIoTask(pending.task_id, pending.task_generation)) return false;

        if (!lockAsyncIoRequests()) return false;
        if (asyncIoRequestForRetireClaimLocked(pending)) |req| {
            req.task_id = 0;
            req.task_generation = 0;
            if (req.state == .pending or req.state == .running) {
                req.status = IO_ERROR_CANCELLED;
                req.result = IO_ERROR_CANCELLED;
                req.processed_bytes = 0;
                req.state = .failed;
                req.completed_tick = timer.tickCount();
                req.completion.completeAll();
            }
            // Generic request handles are process-wide, so another live
            // ProgramThread may still wait on this embedded Completion.
            // Keep the anchor until the purge below cancels those waiters.
        }
        unlockAsyncIoRequests();
    }
}

fn purgeCancelledAsyncIoRequestsForCaller(
    handle: ProgramProcessHandle,
    caller_task_id: u32,
    caller_task_generation: u64,
) bool {
    if (!lockAsyncIoRequests()) return false;
    defer unlockAsyncIoRequests();
    var i: usize = 0;
    while (i < async_io_requests.len) : (i += 1) {
        const req = &async_io_requests[i];
        if (!asyncIoRequestOwnedByCaller(req, handle, caller_task_id, caller_task_generation)) continue;
        if (!req.cancel_requested or req.task_id != 0 or req.task_generation != 0) return false;
        _ = req.completion.queue.cancelAll();
        req.waiters = 0;
        req.* = .{};
    }
    return true;
}

fn asyncIoRequestIndexLocked(wanted: *const AsyncIoRequest) ?usize {
    var i: usize = 0;
    while (i < async_io_requests.len) : (i += 1) {
        if (&async_io_requests[i] == wanted) return i;
    }
    return null;
}

fn asyncIoRetireClaim(slot: usize, req: *const AsyncIoRequest) AsyncIoRetireClaim {
    return .{
        .slot = slot,
        .request_id = req.id,
        .owner_instance_id = req.owner_instance_id,
        .owner_instance_generation = req.owner_instance_generation,
        .caller_task_id = req.caller_task_id,
        .caller_task_generation = req.caller_task_generation,
        .task_id = req.task_id,
        .task_generation = req.task_generation,
        .service_handle = req.service_handle,
        .service_request_id = if (req.kind == .service_call) req.service_request_id else 0,
    };
}

fn asyncIoRequestForRetireClaimLocked(claim: AsyncIoRetireClaim) ?*AsyncIoRequest {
    if (claim.slot >= async_io_requests.len) return null;
    const req = &async_io_requests[claim.slot];
    if (!req.used or
        req.id != claim.request_id or
        req.owner_instance_id != claim.owner_instance_id or
        req.owner_instance_generation != claim.owner_instance_generation or
        req.caller_task_id != claim.caller_task_id or
        req.caller_task_generation != claim.caller_task_generation or
        req.task_id != claim.task_id or
        req.task_generation != claim.task_generation) return null;
    return req;
}

fn retireAsyncIoTask(task_id: u32, task_generation: u64) bool {
    const current_task = scheduler.current();
    const current_task_id = if (current_task) |value| value.id else 0;
    const current_task_generation = if (current_task) |value| value.generation else 0;
    return releaseProgramTaskGeneration(task_id, task_generation, current_task_id, current_task_generation);
}

const AsyncIoPendingCloseResult = enum {
    idle,
    completed,
    deferred,
};

fn reapOnePendingAsyncIoClose() AsyncIoPendingCloseResult {
    if (!lockAsyncIoRequests()) return .deferred;
    var claim: ?AsyncIoRetireClaim = null;
    var slot: usize = 0;
    while (slot < async_io_requests.len) : (slot += 1) {
        const req = &async_io_requests[slot];
        if (!req.used or !req.close_pending or req.waiters != 0 or
            req.state == .pending or req.state == .running) continue;
        claim = asyncIoRetireClaim(slot, req);
        break;
    }
    unlockAsyncIoRequests();

    const pending = claim orelse return .idle;
    if (!retireAsyncIoTask(pending.task_id, pending.task_generation)) return .deferred;
    if (!lockAsyncIoRequests()) return .deferred;
    var reclaimed = false;
    if (asyncIoRequestForRetireClaimLocked(pending)) |req| {
        if (req.close_pending) {
            req.* = .{};
            reclaimed = true;
        }
    }
    unlockAsyncIoRequests();

    if (reclaimed and finishAsyncIoRetireRetryTest(pending.request_id)) {
        k.puts("ASYNCIO05910 retire retry: recovered request=");
        k.putDec(pending.request_id);
        k.puts("\r\n");
    }
    return .completed;
}

fn apiThreadCreate(entry: RawEntryFn, arg: usize, stack_reserve_bytes: u64, flags: u32, out_thread_id: *u32) callconv(.c) i32 {
    if (@intFromPtr(out_thread_id) == 0 or @intFromPtr(entry) == 0) return THREAD_ERROR_INVALID;
    out_thread_id.* = 0;
    var handle: ProgramJoinHandle = .{};
    const status = apiThreadCreateHandle(entry, @intCast(arg), stack_reserve_bytes, flags, &handle);
    if (status == THREAD_OK) out_thread_id.* = handle.thread_id;
    return status;
}

fn apiThreadCreateHandle(entry: RawEntryFn, arg: u64, stack_reserve_bytes: u64, flags: u32, out_handle: *ProgramJoinHandle) callconv(.c) i32 {
    if (@intFromPtr(out_handle) == 0 or @intFromPtr(entry) == 0) return THREAD_ERROR_INVALID;
    out_handle.* = .{};
    var admitted = false;
    defer if (!admitted) recordProgramThreadCreateFailure();
    if ((flags & ~THREAD_CREATE_FLAGS_SUPPORTED) != 0) return THREAD_ERROR_UNSUPPORTED;
    const instance = currentInstance() orelse return THREAD_ERROR_NO_INSTANCE;
    if (instance.done) return THREAD_ERROR_NO_INSTANCE;
    const instance_handle = currentProgramHandle() orelse return THREAD_ERROR_NO_INSTANCE;
    const canonical_entry = normalizeThreadEntry(instance, entry) orelse return THREAD_ERROR_INVALID;

    const reserve_len = normalizeThreadStackReserve(instance, stack_reserve_bytes) orelse return THREAD_ERROR_INVALID;
    const initial_len = initialThreadStackCommit(instance, reserve_len) orelse return THREAD_ERROR_INVALID;
    if (!systemCommitLimitAllows(initial_len)) return THREAD_ERROR_NO_MEMORY;
    var stack = allocateProgramStackWithLimits(instance.id, reserve_len, initial_len, instance.memory_profile, instance.program_stack_serial_telemetry) orelse return THREAD_ERROR_NO_MEMORY;

    const thread_ctx = allocProgramThreadSlot() orelse {
        _ = freeProgramStack(&stack);
        return THREAD_ERROR_NO_MEMORY;
    };
    var task_failure: task.CreateFailure = .none;
    const parallel_owner = if (scheduler.current()) |owner_task| owner_task.smp_eligible else false;
    const new_task = (if (parallel_owner)
        task.createParallelThreadBlockedWithFailure("r4x-thread", programThreadTaskMain, &task_failure)
    else
        task.createLegacyThreadBlockedWithFailure("r4x-thread", programThreadTaskMain, &task_failure)) orelse {
        _ = freeProgramThreadMemory(thread_ctx);
        _ = freeProgramStack(&stack);
        return threadCreateErrorForTaskFailure(task_failure);
    };
    thread_ctx.* = .{
        .used = true,
        .instance_id = instance.id,
        .instance_generation = instance_handle.generation,
        .owner_instance = instance,
        .task_id = new_task.id,
        .task_generation = new_task.generation,
        .state = .ready,
        .flags = THREAD_FLAG_JOINABLE,
        .entry = canonical_entry,
        .arg = @intCast(arg),
        .stack = stack,
        .exit_code = 0,
        .created_tick = timer.tickCount(),
        .finished_tick = 0,
        .join_count = 0,
        .join_owner_task_id = 0,
        .join_owner_task_generation = 0,
        .join_queue = sync.WaitQueue.init(),
    };
    if (!task.bindExecutionOwner(new_task, .program_thread, @ptrCast(thread_ctx))) {
        _ = task.retireIdentity(new_task.id, new_task.generation);
        _ = freeProgramThreadMemory(thread_ctx);
        _ = freeProgramStack(&stack);
        return THREAD_ERROR_NO_MEMORY;
    }
    if (!publishProgramThread(thread_ctx)) {
        _ = task.clearExecutionOwner(new_task, @ptrCast(thread_ctx));
        _ = task.retireIdentity(new_task.id, new_task.generation);
        _ = freeProgramThreadMemory(thread_ctx);
        _ = freeProgramStack(&stack);
        return THREAD_ERROR_NO_MEMORY;
    }
    out_handle.* = .{
        .thread_id = thread_ctx.id,
        .instance_id = thread_ctx.instance_id,
        .thread_generation = thread_ctx.generation,
        .instance_generation = thread_ctx.instance_generation,
        .reserved = 0,
    };
    admitted = true;
    task.markReady(new_task, timer.tickCount());
    return THREAD_OK;
}

fn recordProgramThreadCreateFailure() void {
    const irq_flags = interrupts.saveAndDisable();
    program_thread_create_failures +%= 1;
    interrupts.restore(irq_flags);
}

fn threadCreateErrorForTaskFailure(failure: task.CreateFailure) i32 {
    return switch (failure) {
        // The waiter is embedded in Task. A detached-state violation is an
        // admission/invariant conflict rather than allocator exhaustion.
        .waiter => THREAD_ERROR_BUSY,
        .none, .task_metadata, .stack, .fpu, .memory => THREAD_ERROR_NO_MEMORY,
    };
}

fn apiThreadExit(exit_code: i32) callconv(.c) void {
    const thread_ctx = currentProgramThread() orelse scheduler.exitCurrentAndRetire();
    markProgramThreadDone(thread_ctx, exit_code);
    if ((thread_ctx.flags & THREAD_FLAG_MAIN) != 0) {
        const handle = ProgramProcessHandle{
            .instance_id = thread_ctx.instance_id,
            .reserved = 0,
            .generation = thread_ctx.instance_generation,
        };
        markInstanceDone(handle, exit_code);
    }
    unpinProgramThreadExecution(thread_ctx);
    scheduler.exitCurrentAndRetire();
}

const ProgramThreadJoinClaim = union(enum) {
    target: *ProgramThread,
    failure: i32,
};

fn reclaimStaleJoinLeaseLocked(target: *ProgramThread) void {
    if (!target.join_lease_active or target.join_owner_task_id == 0 or target.join_owner_task_generation == 0) return;
    if (task.isAliveIdentity(target.join_owner_task_id, target.join_owner_task_generation)) return;
    if (target.join_queue.hasWaiters()) return;
    target.join_owner_task_id = 0;
    target.join_owner_task_generation = 0;
    target.join_lease_active = false;
    if (target.join_waiter_refs != 0) target.join_waiter_refs -= 1;
    if (target.pin_count != 0) target.pin_count -= 1;
}

fn claimProgramThreadJoin(
    thread_id: u32,
    expected_thread_generation: ?u64,
    instance_id: u32,
    instance_generation: u64,
    current_thread_id: u32,
    owner_task_id: u32,
    owner_task_generation: u64,
) ProgramThreadJoinClaim {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    const target = programThreadByIdLocked(thread_id) orelse return .{ .failure = THREAD_ERROR_NOT_FOUND };
    if (!target.used or target.retire_pending or target.retire_in_progress or
        target.instance_id != instance_id or target.instance_generation != instance_generation or
        (expected_thread_generation != null and target.generation != expected_thread_generation.?))
        return .{ .failure = THREAD_ERROR_NOT_FOUND };
    if (target.id == current_thread_id) return .{ .failure = THREAD_ERROR_SELF_JOIN };
    if ((target.flags & THREAD_FLAG_MAIN) != 0 or (target.flags & THREAD_FLAG_JOINABLE) == 0)
        return .{ .failure = THREAD_ERROR_NOT_JOINABLE };
    reclaimStaleJoinLeaseLocked(target);
    if (target.join_lease_active or target.join_owner_task_id != 0 or target.join_owner_task_generation != 0)
        return .{ .failure = THREAD_ERROR_BUSY };
    if (target.pin_count == std.math.maxInt(u32)) return .{ .failure = THREAD_ERROR_BUSY };
    target.join_owner_task_id = owner_task_id;
    target.join_owner_task_generation = owner_task_generation;
    target.join_lease_active = true;
    target.pin_count += 1;
    return .{ .target = target };
}

fn releaseJoinLeaseLocked(target: *ProgramThread, owner_task_id: u32, owner_task_generation: u64) bool {
    if (!target.join_lease_active or
        target.join_owner_task_id != owner_task_id or
        target.join_owner_task_generation != owner_task_generation or
        target.join_waiter_refs != 0)
        return false;
    target.join_owner_task_id = 0;
    target.join_owner_task_generation = 0;
    target.join_lease_active = false;
    if (target.pin_count != 0) target.pin_count -= 1;
    return true;
}

fn releaseJoinLease(target: *ProgramThread, owner_task_id: u32, owner_task_generation: u64) void {
    const irq_flags = interrupts.saveAndDisable();
    if (containsProgramThreadLocked(target)) _ = releaseJoinLeaseLocked(target, owner_task_id, owner_task_generation);
    interrupts.restore(irq_flags);
}

fn apiThreadJoin(thread_id: u32, timeout_ticks: u64, out_exit_code: *i32) callconv(.c) i32 {
    return joinProgramThread(thread_id, null, timeout_ticks, out_exit_code);
}

fn apiThreadHandleJoin(handle: *const ProgramJoinHandle, timeout_ticks: u64, out_exit_code: *i32) callconv(.c) i32 {
    if (@intFromPtr(handle) == 0) return THREAD_ERROR_INVALID;
    const exact = handle.*;
    if (exact.thread_id == 0 or exact.instance_id == 0 or exact.thread_generation == 0 or
        exact.instance_generation == 0 or exact.reserved != 0)
        return THREAD_ERROR_INVALID;
    return joinProgramThread(exact.thread_id, exact, timeout_ticks, out_exit_code);
}

fn joinProgramThread(thread_id: u32, exact: ?ProgramJoinHandle, timeout_ticks: u64, out_exit_code: *i32) i32 {
    if (@intFromPtr(out_exit_code) == 0) return THREAD_ERROR_INVALID;
    out_exit_code.* = 0;
    const current_thread = currentProgramThread() orelse return THREAD_ERROR_NO_INSTANCE;
    const current_instance_id = current_thread.instance_id;
    const current_instance_generation = current_thread.instance_generation;
    if (exact) |handle| {
        if (handle.instance_id != current_instance_id or handle.instance_generation != current_instance_generation)
            return THREAD_ERROR_NOT_FOUND;
    }
    const current_task = scheduler.current() orelse return THREAD_ERROR_NO_INSTANCE;
    const current_task_id = current_task.id;
    const current_task_generation = current_task.generation;
    const target = switch (claimProgramThreadJoin(
        thread_id,
        if (exact) |handle| handle.thread_generation else null,
        current_instance_id,
        current_instance_generation,
        current_thread.id,
        current_task_id,
        current_task_generation,
    )) {
        .target => |value| value,
        .failure => |status| return status,
    };
    if (target.state == .ready or target.state == .running) {
        const begin_flags = interrupts.saveAndDisable();
        if (!containsProgramThreadLocked(target) or
            !target.join_lease_active or
            target.join_owner_task_id != current_task_id or
            target.join_owner_task_generation != current_task_generation or
            target.join_waiter_refs == std.math.maxInt(u32))
        {
            interrupts.restore(begin_flags);
            releaseJoinLease(target, current_task_id, current_task_generation);
            return THREAD_ERROR_BUSY;
        }
        target.join_waiter_refs += 1;
        interrupts.restore(begin_flags);
        const queue = &target.join_queue;
        // The completion predicate and waiter enrollment must share the
        // WaitQueue critical section. Otherwise a worker can exit after the
        // state check above but before enrollment, send its wake to an empty
        // queue, and leave this join blocked forever.
        const result = queue.waitUnless(timeout_ticks, "thread_join", programThreadJoinStillNeeded, target);
        const end_flags = interrupts.saveAndDisable();
        if (!containsProgramThreadLocked(target) or
            !target.join_lease_active or
            target.join_owner_task_id != current_task_id or
            target.join_owner_task_generation != current_task_generation)
        {
            interrupts.restore(end_flags);
            return THREAD_ERROR_NOT_FOUND;
        }
        if (target.join_waiter_refs != 0) target.join_waiter_refs -= 1;
        const completed = target.state == .exited or target.state == .killed;
        interrupts.restore(end_flags);
        if (result == .timeout and !completed) {
            releaseJoinLease(target, current_task_id, current_task_generation);
            return THREAD_ERROR_TIMEOUT;
        }
        if (result != .signaled and result != .killed and !completed) {
            releaseJoinLease(target, current_task_id, current_task_generation);
            return THREAD_ERROR_BUSY;
        }
        return finishJoinedThread(target, current_task_id, current_task_generation, out_exit_code);
    }

    return finishJoinedThread(target, current_task_id, current_task_generation, out_exit_code);
}

fn programThreadJoinStillNeeded(raw: *anyopaque) bool {
    const target: *ProgramThread = @ptrCast(@alignCast(raw));
    return target.used and (target.state == .ready or target.state == .running);
}

fn finishJoinedThread(target: *ProgramThread, owner_task_id: u32, owner_task_generation: u64, out_exit_code: *i32) i32 {
    const target_id = target.id;
    const cleanup_started = timer.tickCount();
    const release_token = task_context.enterUnwind();
    if (!release_token.admitted()) {
        releaseJoinLease(target, owner_task_id, owner_task_generation);
        return THREAD_ERROR_BUSY;
    }
    const irq_flags = interrupts.saveAndDisable();
    if (!containsProgramThreadLocked(target) or
        !target.join_lease_active or
        target.join_owner_task_id != owner_task_id or
        target.join_owner_task_generation != owner_task_generation or
        target.join_waiter_refs != 0 or
        target.join_queue.hasWaiters() or
        target.pin_count != 1 or
        (target.state != .exited and target.state != .killed))
    {
        if (containsProgramThreadLocked(target)) _ = releaseJoinLeaseLocked(target, owner_task_id, owner_task_generation);
        interrupts.restore(irq_flags);
        _ = task_context.leaveUnwind(release_token);
        return THREAD_ERROR_BUSY;
    }
    const exit_code = target.exit_code;
    target.join_count +%= 1;
    target.retire_pending = true;
    target.retire_in_progress = true;
    bumpProgramThreadInventoryEpochLocked();
    interrupts.restore(irq_flags);
    if (!completeProgramThreadRetire(target, release_token, false)) {
        k.puts("[R4XTHREAD] join cleanup deferred thread=");
        k.putDec(target_id);
        k.puts("\r\n");
        return THREAD_ERROR_BUSY;
    }
    out_exit_code.* = exit_code;
    const cleanup_finished = timer.tickCount();
    const cleanup_ticks = if (cleanup_finished >= cleanup_started) cleanup_finished - cleanup_started else 0;
    if (cleanup_ticks >= 25) {
        k.puts("[R4XTHREAD] slow join cleanup thread=");
        k.putDec(target_id);
        k.puts(" ticks=");
        k.putDec(cleanup_ticks);
        k.puts("\r\n");
    }
    return THREAD_OK;
}

fn apiThreadCurrent() callconv(.c) u32 {
    const thread_ctx = currentProgramThread() orelse return 0;
    return thread_ctx.id;
}

fn apiThreadStatus(thread_id: u32, out: *ProgramThreadInfo) callconv(.c) i32 {
    if (@intFromPtr(out) == 0) return THREAD_ERROR_INVALID;
    const current_instance = currentInstance() orelse return THREAD_ERROR_NO_INSTANCE;
    if (thread_id == 0) {
        const current_thread = currentProgramThread() orelse return THREAD_ERROR_NOT_FOUND;
        if (current_thread.owner_instance != current_instance) return THREAD_ERROR_NOT_FOUND;
        out.* = threadInfo(current_thread);
        return THREAD_OK;
    }
    // Status is also the observable retirement probe: a failed object free is
    // relinked and remains visible until a later retry, never masquerading as
    // a successfully consumed join handle.
    const thread_ctx = pinProgramThreadById(thread_id, true) orelse return THREAD_ERROR_NOT_FOUND;
    defer _ = unpinProgramThread(thread_ctx);
    if (thread_ctx.owner_instance != current_instance) return THREAD_ERROR_NOT_FOUND;
    out.* = threadInfo(thread_ctx);
    return THREAD_OK;
}

fn apiThreadHandleStatus(handle: *const ProgramJoinHandle, out: *ProgramThreadInfo) callconv(.c) i32 {
    if (@intFromPtr(handle) == 0 or @intFromPtr(out) == 0) return THREAD_ERROR_INVALID;
    const exact = handle.*;
    if (exact.thread_id == 0 or exact.instance_id == 0 or exact.thread_generation == 0 or
        exact.instance_generation == 0 or exact.reserved != 0)
        return THREAD_ERROR_INVALID;
    const current = currentProgramThread() orelse return THREAD_ERROR_NO_INSTANCE;
    if (current.instance_id != exact.instance_id or current.instance_generation != exact.instance_generation)
        return THREAD_ERROR_NOT_FOUND;
    const thread_ctx = pinProgramThreadByHandle(exact, true) orelse return THREAD_ERROR_NOT_FOUND;
    defer _ = unpinProgramThread(thread_ctx);
    out.* = threadInfo(thread_ctx);
    return THREAD_OK;
}

fn registerMainThread(instance: *ProgramInstance, program_task: *task.Task) ?*ProgramThread {
    const instance_handle = programHandleForInstance(instance) orelse return null;
    const thread_ctx = allocProgramThreadSlot() orelse return null;
    thread_ctx.* = .{
        .used = true,
        .instance_id = instance.id,
        .instance_generation = instance_handle.generation,
        .owner_instance = instance,
        .task_id = program_task.id,
        .task_generation = program_task.generation,
        .state = .ready,
        .flags = THREAD_FLAG_MAIN,
        .entry = instance.entry,
        .arg = 0,
        .stack = .{},
        .exit_code = 0,
        .created_tick = timer.tickCount(),
        .finished_tick = 0,
        .join_count = 0,
        .join_owner_task_id = 0,
        .join_owner_task_generation = 0,
        .join_queue = sync.WaitQueue.init(),
    };
    if (!task.bindExecutionOwner(program_task, .program_thread, @ptrCast(thread_ctx))) {
        _ = freeProgramThreadMemory(thread_ctx);
        return null;
    }
    if (!publishProgramThread(thread_ctx)) {
        _ = task.clearExecutionOwner(program_task, @ptrCast(thread_ctx));
        _ = freeProgramThreadMemory(thread_ctx);
        return null;
    }
    return thread_ctx;
}

fn allocProgramThreadSlot() ?*ProgramThread {
    const memory = heap.alloc(@sizeOf(ProgramThread), @alignOf(ProgramThread)) orelse return null;
    const thread_ctx: *ProgramThread = @ptrCast(@alignCast(memory.ptr));
    thread_ctx.* = .{ .entry = undefined };
    return thread_ctx;
}

fn allocateThreadIdLocked() ?u32 {
    var candidate = if (next_thread_id == 0) @as(u32, 1) else next_thread_id;
    const start = candidate;
    while (true) {
        next_thread_id = candidate +% 1;
        if (next_thread_id == 0) next_thread_id = 1;
        if (programThreadByIdLocked(candidate) == null) return candidate;
        candidate = next_thread_id;
        if (candidate == start) return null;
    }
}

fn allocateThreadGenerationLocked() ?u64 {
    const generation = next_thread_generation;
    if (generation == 0 or generation == std.math.maxInt(u64)) return null;
    next_thread_generation += 1;
    return generation;
}

fn publishProgramThread(thread_ctx: *ProgramThread) bool {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    const thread_id = allocateThreadIdLocked() orelse return false;
    const thread_generation = allocateThreadGenerationLocked() orelse return false;
    thread_ctx.id = thread_id;
    thread_ctx.generation = thread_generation;
    linkProgramThreadLocked(thread_ctx);
    return true;
}

fn linkProgramThreadLocked(thread_ctx: *ProgramThread) void {
    thread_ctx.registry_prev = program_thread_tail;
    thread_ctx.registry_next = null;
    if (program_thread_tail) |tail| tail.registry_next = thread_ctx else program_thread_head = thread_ctx;
    program_thread_tail = thread_ctx;
    program_thread_count += 1;
    if (program_thread_count > program_thread_peak) program_thread_peak = program_thread_count;
    bumpProgramThreadInventoryEpochLocked();
}

fn unlinkProgramThreadLocked(thread_ctx: *ProgramThread) void {
    if (thread_ctx.registry_prev) |prev| prev.registry_next = thread_ctx.registry_next else program_thread_head = thread_ctx.registry_next;
    if (thread_ctx.registry_next) |next| next.registry_prev = thread_ctx.registry_prev else program_thread_tail = thread_ctx.registry_prev;
    thread_ctx.registry_prev = null;
    thread_ctx.registry_next = null;
    if (program_thread_count != 0) program_thread_count -= 1;
    bumpProgramThreadInventoryEpochLocked();
}

fn bumpProgramThreadInventoryEpochLocked() void {
    program_thread_mutation_epoch +%= 1;
    if (program_thread_mutation_epoch == 0) program_thread_mutation_epoch = 1;
}

fn bumpProgramThreadInventoryEpoch() void {
    const irq_flags = interrupts.saveAndDisable();
    bumpProgramThreadInventoryEpochLocked();
    interrupts.restore(irq_flags);
}

fn markProgramThreadRunning(thread_ctx: *ProgramThread) void {
    const irq_flags = interrupts.saveAndDisable();
    if (containsProgramThreadLocked(thread_ctx) and thread_ctx.used and thread_ctx.state == .ready) {
        thread_ctx.state = .running;
        bumpProgramThreadInventoryEpochLocked();
    }
    interrupts.restore(irq_flags);
}

fn freeProgramThreadMemory(thread_ctx: *ProgramThread) bool {
    const bytes: [*]u8 = @ptrCast(thread_ctx);
    if (heap.free(bytes[0..@sizeOf(ProgramThread)]) != .ok) {
        k.puts("ProgramThread object release failed\r\n");
        return false;
    }
    return true;
}

fn currentProgramThread() ?*ProgramThread {
    const current_task = scheduler.current() orelse return null;
    const owner = task.executionOwner(current_task);
    if (owner.kind != .program_thread) return null;
    const thread_ctx: *ProgramThread = @ptrCast(@alignCast(owner.context orelse return null));
    if (!thread_ctx.used or thread_ctx.task_id != current_task.id or thread_ctx.task_generation != current_task.generation) return null;
    return thread_ctx;
}

fn programThreadByIdLocked(id: u32) ?*ProgramThread {
    if (id == 0) return null;
    var cursor = program_thread_head;
    while (cursor) |thread_ctx| : (cursor = thread_ctx.registry_next) {
        if (thread_ctx.used and thread_ctx.id == id) return thread_ctx;
    }
    return null;
}

fn programThreadByIdentityLocked(id: u32, instance_id: u32, instance_generation: u64, thread_generation: u64) ?*ProgramThread {
    const thread_ctx = programThreadByIdLocked(id) orelse return null;
    if (thread_ctx.instance_id != instance_id or thread_ctx.instance_generation != instance_generation or thread_ctx.generation != thread_generation) return null;
    return thread_ctx;
}

fn containsProgramThreadLocked(wanted: *const ProgramThread) bool {
    var cursor = program_thread_head;
    while (cursor) |thread_ctx| : (cursor = thread_ctx.registry_next) {
        if (thread_ctx == wanted) return true;
    }
    return false;
}

fn pinProgramThreadById(id: u32, allow_retiring: bool) ?*ProgramThread {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    const thread_ctx = programThreadByIdLocked(id) orelse return null;
    if (!thread_ctx.used or
        (!allow_retiring and (thread_ctx.retire_pending or thread_ctx.retire_in_progress)) or
        thread_ctx.pin_count == std.math.maxInt(u32)) return null;
    thread_ctx.pin_count += 1;
    return thread_ctx;
}

fn pinProgramThreadByHandle(handle: ProgramJoinHandle, allow_retiring: bool) ?*ProgramThread {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    const thread_ctx = programThreadByIdentityLocked(
        handle.thread_id,
        handle.instance_id,
        handle.instance_generation,
        handle.thread_generation,
    ) orelse return null;
    if (!thread_ctx.used or
        (!allow_retiring and (thread_ctx.retire_pending or thread_ctx.retire_in_progress)) or
        thread_ctx.pin_count == std.math.maxInt(u32)) return null;
    thread_ctx.pin_count += 1;
    return thread_ctx;
}

fn unpinProgramThread(thread_ctx: *ProgramThread) bool {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    if (!containsProgramThreadLocked(thread_ctx) or thread_ctx.pin_count == 0) return false;
    thread_ctx.pin_count -= 1;
    return true;
}

fn markProgramThreadDone(thread_ctx: *ProgramThread, exit_code: i32) void {
    const irq_flags = interrupts.saveAndDisable();
    if (!containsProgramThreadLocked(thread_ctx) or !thread_ctx.used or thread_ctx.state == .exited or thread_ctx.state == .killed) {
        interrupts.restore(irq_flags);
        return;
    }
    thread_ctx.exit_code = exit_code;
    thread_ctx.state = .exited;
    thread_ctx.finished_tick = timer.tickCount();
    bumpProgramThreadInventoryEpochLocked();
    interrupts.restore(irq_flags);
    _ = thread_ctx.join_queue.wakeAll();
}

fn terminateProgramThreads(instance_id: u32, exit_code: i32, skip_task_id: ?u32) void {
    const handle = programHandleForId(instance_id) orelse return;
    terminateProgramThreadsForHandle(handle, exit_code, skip_task_id);
}

fn terminateProgramThreadsForHandle(handle: ProgramProcessHandle, exit_code: i32, skip_task_id: ?u32) void {
    const now = timer.tickCount();
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    var inventory_changed = false;
    var cursor = program_thread_head;
    while (cursor) |thread_ctx| : (cursor = thread_ctx.registry_next) {
        if (!thread_ctx.used or thread_ctx.instance_id != handle.instance_id or thread_ctx.instance_generation != handle.generation) continue;
        if (skip_task_id != null and thread_ctx.task_id == skip_task_id.?) continue;
        // A naturally returned R4X task still executes its kernel epilogue
        // until it releases the generation pin and its exact scheduler Task
        // generation has published .dead. Killing it in either half of this
        // handoff can strand the pin or preempt the terminal context switch.
        if (naturalExitEpilogueOwnsTask(thread_ctx)) continue;
        if (thread_ctx.state != .exited and thread_ctx.state != .killed) {
            thread_ctx.exit_code = exit_code;
            thread_ctx.state = .killed;
            thread_ctx.finished_tick = now;
            inventory_changed = true;
            _ = thread_ctx.join_queue.wakeAll();
        }
        // A spawn transaction owns private registry/image/stack/task state
        // that the reaper cannot discover yet.  Let that task unwind through
        // runProgramFile's rollback defers before terminating it.
        if (thread_ctx.spawn_transaction_depth != 0) {
            thread_ctx.exit_deferred = true;
            continue;
        }
        if (thread_ctx.task_id != 0) {
            _ = task.killIdentity(thread_ctx.task_id, thread_ctx.task_generation);
        }
    }
    if (inventory_changed) bumpProgramThreadInventoryEpochLocked();
}

fn releaseThreadsForInstance(instance_id: u32) bool {
    const handle = programHandleForId(instance_id) orelse return true;
    return releaseThreadsForHandle(handle);
}

fn releaseThreadsForHandle(handle: ProgramProcessHandle) bool {
    return releaseThreadsForHandleReporting(handle, false);
}

fn releaseThreadsForHandleReporting(handle: ProgramProcessHandle, report_boot_foreground: bool) bool {
    while (true) {
        switch (claimProgramThreadRetireForHandle(handle)) {
            .done => return true,
            .pending => |reason| {
                reportBootForegroundRetireStage(report_boot_foreground, programThreadRetirePendingDetail(reason));
                return false;
            },
            .claimed => |claim| {
                reportBootForegroundRetireStage(report_boot_foreground, "SERVMAN: Thread freigeben");
                if (!completeProgramThreadRetire(claim.thread_ctx, claim.release_token, report_boot_foreground)) return false;
            },
        }
    }
}

const ProgramThreadRetirePendingReason = enum {
    spawn_transaction,
    natural_task_owner,
    retire_claim,
    join,
    thread_pin,
    reaper_guard,
};

const ProgramThreadRetireClaim = union(enum) {
    done,
    pending: ProgramThreadRetirePendingReason,
    claimed: struct {
        thread_ctx: *ProgramThread,
        release_token: task_context.UnwindToken,
    },
};

fn programThreadRetirePendingDetail(reason: ProgramThreadRetirePendingReason) []const u8 {
    return switch (reason) {
        .spawn_transaction => "SERVMAN: Spawn wartet",
        .natural_task_owner => "SERVMAN: Task-Reaper wartet",
        .retire_claim => "SERVMAN: Reaper wartet",
        .join => "SERVMAN: Join wartet",
        .thread_pin => "SERVMAN: Thread-Pin wartet",
        .reaper_guard => "SERVMAN: Schutz wartet",
    };
}

fn claimProgramThreadRetireForHandle(handle: ProgramProcessHandle) ProgramThreadRetireClaim {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    var pending_reason: ?ProgramThreadRetirePendingReason = null;
    var inventory_changed = false;
    var cursor = program_thread_head;
    while (cursor) |thread_ctx| : (cursor = thread_ctx.registry_next) {
        if (!thread_ctx.used or thread_ctx.instance_id != handle.instance_id or thread_ctx.instance_generation != handle.generation) continue;
        thread_ctx.retire_pending = true;
        thread_ctx.retire_for_instance = true;
        if (thread_ctx.state != .exited and thread_ctx.state != .killed) {
            thread_ctx.exit_code = -9;
            thread_ctx.state = .killed;
            thread_ctx.finished_tick = timer.tickCount();
            inventory_changed = true;
        }
        reclaimStaleJoinLeaseLocked(thread_ctx);
        if (thread_ctx.join_queue.hasWaiters()) _ = thread_ctx.join_queue.cancelAll();
        if (thread_ctx.spawn_transaction_depth != 0) {
            thread_ctx.exit_deferred = true;
            if (pending_reason == null) pending_reason = .spawn_transaction;
            continue;
        }
        // The deferred reaper may run as soon as markInstanceDone publishes
        // .retire. A natural owner transfers its exact Task generation to the
        // scheduler reaper and remains present until that independent Task
        // teardown is complete. Hard-killed threads stay program-reaper-owned.
        if (naturalExitEpilogueOwnsTask(thread_ctx)) {
            if (pending_reason == null) pending_reason = .natural_task_owner;
            continue;
        }
        if (thread_ctx.retire_in_progress) {
            if (pending_reason == null) pending_reason = .retire_claim;
            continue;
        }
        if (thread_ctx.join_lease_active or
            thread_ctx.join_waiter_refs != 0 or
            thread_ctx.join_queue.hasWaiters())
        {
            if (pending_reason == null) pending_reason = .join;
            continue;
        }
        if (thread_ctx.pin_count != 0) {
            if (pending_reason == null) pending_reason = .thread_pin;
            continue;
        }
        const release_token = task_context.enterUnwind();
        if (!release_token.admitted()) {
            if (pending_reason == null) pending_reason = .reaper_guard;
            continue;
        }
        thread_ctx.retire_in_progress = true;
        thread_ctx.pin_count = 1;
        if (inventory_changed) bumpProgramThreadInventoryEpochLocked();
        return .{ .claimed = .{ .thread_ctx = thread_ctx, .release_token = release_token } };
    }
    if (inventory_changed) bumpProgramThreadInventoryEpochLocked();
    return if (pending_reason) |reason| .{ .pending = reason } else .done;
}

fn clearProgramThreadRetireClaim(thread_ctx: *ProgramThread) void {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    if (!containsProgramThreadLocked(thread_ctx) or !thread_ctx.retire_in_progress) return;
    thread_ctx.retire_in_progress = false;
    if (!thread_ctx.retire_for_instance) thread_ctx.retire_pending = false;
    if (thread_ctx.join_lease_active) {
        _ = releaseJoinLeaseLocked(thread_ctx, thread_ctx.join_owner_task_id, thread_ctx.join_owner_task_generation);
    } else if (thread_ctx.pin_count != 0) {
        thread_ctx.pin_count -= 1;
    }
}

fn completeProgramThreadRetire(
    thread_ctx: *ProgramThread,
    release_token: task_context.UnwindToken,
    report_boot_foreground: bool,
) bool {
    defer _ = task_context.leaveUnwind(release_token);
    const owner_task_id = thread_ctx.task_id;
    const owner_task_generation = thread_ctx.task_generation;
    const thread_handle = ProgramProcessHandle{
        .instance_id = thread_ctx.instance_id,
        .reserved = 0,
        .generation = thread_ctx.instance_generation,
    };
    if (!thread_ctx.task_detached) {
        reportBootForegroundRetireStage(report_boot_foreground, "SERVMAN: Task freigeben");
        const current_task = scheduler.current();
        const current_task_id = if (current_task) |value| value.id else 0;
        const current_task_generation = if (current_task) |value| value.generation else 0;
        if (!releaseProgramTaskGeneration(owner_task_id, owner_task_generation, current_task_id, current_task_generation)) {
            reportBootForegroundRetireStage(report_boot_foreground, "SERVMAN: Task wartet");
            clearProgramThreadRetireClaim(thread_ctx);
            return false;
        }
        thread_ctx.task_detached = true;
        bumpProgramThreadInventoryEpoch();
        reportBootForegroundRetireStage(report_boot_foreground, "SERVMAN: Task frei");
    }
    // The caller task and its intrusive Completion waiter are now physically
    // detached, so it cannot publish another request while cancellation
    // yields. Every request carries raw caller-owned pointers, including
    // generic file and service requests whose public handles are otherwise
    // process-wide. Retire all workers submitted by this exact task identity
    // before purging their anchors and releasing the matching R4SYS namespace
    // leases. Keep task identity until all three operations have succeeded.
    reportBootForegroundRetireStage(report_boot_foreground, "SERVMAN: I/O abbrechen");
    if (!cancelAsyncIoRequestsForCaller(
        thread_handle,
        owner_task_id,
        owner_task_generation,
    )) {
        reportBootForegroundRetireStage(report_boot_foreground, "SERVMAN: I/O wartet");
        clearProgramThreadRetireClaim(thread_ctx);
        return false;
    }
    reportBootForegroundRetireStage(report_boot_foreground, "SERVMAN: I/O entfernen");
    if (!purgeCancelledAsyncIoRequestsForCaller(
        thread_handle,
        owner_task_id,
        owner_task_generation,
    )) {
        reportBootForegroundRetireStage(report_boot_foreground, "SERVMAN: I/O-Abbau wartet");
        clearProgramThreadRetireClaim(thread_ctx);
        return false;
    }
    reportBootForegroundRetireStage(report_boot_foreground, "SERVMAN: Streams loesen");
    if (!r4api.r4sys.releaseStreamSlotsForProgramThread(
        thread_ctx.instance_id,
        thread_ctx.instance_generation,
        owner_task_id,
        owner_task_generation,
    )) {
        reportBootForegroundRetireStage(report_boot_foreground, "SERVMAN: Streams warten");
        clearProgramThreadRetireClaim(thread_ctx);
        return false;
    }
    if (thread_ctx.task_id != 0 or thread_ctx.task_generation != 0) {
        thread_ctx.task_id = 0;
        thread_ctx.task_generation = 0;
        bumpProgramThreadInventoryEpoch();
    }
    if (!thread_ctx.stack_released) {
        reportBootForegroundRetireStage(report_boot_foreground, "SERVMAN: Stack freigeben");
        if (thread_ctx.stack.range_id != 0 and !freeProgramStack(&thread_ctx.stack)) {
            reportBootForegroundRetireStage(report_boot_foreground, "SERVMAN: Stack wartet");
            clearProgramThreadRetireClaim(thread_ctx);
            return false;
        }
        thread_ctx.stack_released = true;
        bumpProgramThreadInventoryEpoch();
    }
    unpinProgramThreadExecution(thread_ctx);
    if (thread_ctx.execution_pinned) {
        reportBootForegroundRetireStage(report_boot_foreground, "SERVMAN: Pin wartet");
        clearProgramThreadRetireClaim(thread_ctx);
        return false;
    }

    const irq_flags = interrupts.saveAndDisable();
    if (!containsProgramThreadLocked(thread_ctx) or
        !thread_ctx.retire_pending or
        !thread_ctx.retire_in_progress or
        !thread_ctx.task_detached or
        !thread_ctx.stack_released or
        thread_ctx.pin_count != 1 or
        thread_ctx.join_waiter_refs != 0 or
        thread_ctx.join_queue.hasWaiters())
    {
        interrupts.restore(irq_flags);
        reportBootForegroundRetireStage(report_boot_foreground, "SERVMAN: Thread wartet");
        clearProgramThreadRetireClaim(thread_ctx);
        return false;
    }
    thread_ctx.flags |= THREAD_FLAG_JOINED;
    unlinkProgramThreadLocked(thread_ctx);
    interrupts.restore(irq_flags);

    if (freeProgramThreadMemory(thread_ctx)) {
        reportBootForegroundRetireStage(report_boot_foreground, "SERVMAN: Thread frei");
        return true;
    }

    reportBootForegroundRetireStage(report_boot_foreground, "SERVMAN: Speicher wartet");

    const retry_flags = interrupts.saveAndDisable();
    linkProgramThreadLocked(thread_ctx);
    thread_ctx.retire_in_progress = false;
    if (!thread_ctx.retire_for_instance) thread_ctx.retire_pending = false;
    if (thread_ctx.join_lease_active) {
        _ = releaseJoinLeaseLocked(thread_ctx, thread_ctx.join_owner_task_id, thread_ctx.join_owner_task_generation);
    } else if (thread_ctx.pin_count != 0) {
        thread_ctx.pin_count -= 1;
    }
    interrupts.restore(retry_flags);
    return false;
}

fn releaseCreatedProgramTask(created: *task.Task) bool {
    const current_task = scheduler.current();
    const current_task_id = if (current_task) |value| value.id else 0;
    const current_task_generation = if (current_task) |value| value.generation else 0;
    return releaseProgramTaskGeneration(created.id, created.generation, current_task_id, current_task_generation);
}

fn releaseProgramTaskGeneration(task_id: u32, task_generation: u64, current_task_id: u32, current_task_generation: u64) bool {
    if (task_id == 0) return true;
    if (task_id == current_task_id and task_generation == current_task_generation) return false;
    return switch (task.retireIdentity(task_id, task_generation)) {
        .gone, .released => true,
        .pending => false,
    };
}

fn normalizeThreadEntry(instance: *const ProgramInstance, entry: RawEntryFn) ?RawEntryFn {
    const raw = @intFromPtr(entry);
    const base = instance.program_image_base;
    const size: u64 = @intCast(instance.program_image_size);
    if (base == 0 or size == 0) return null;
    const end = base +% size;
    if (end >= base and raw >= base and raw < end) return entry;
    if (raw < size) return @ptrFromInt(base + raw);

    const link_end = R4X_MODULE_LINK_BASE +% size;
    if (link_end >= R4X_MODULE_LINK_BASE and raw >= R4X_MODULE_LINK_BASE and raw < link_end) {
        const offset = raw - R4X_MODULE_LINK_BASE;
        return @ptrFromInt(base + offset);
    }
    const signed_link_end = R4X_SIGNED_LINK_BASE +% size;
    if (signed_link_end >= R4X_SIGNED_LINK_BASE and raw >= R4X_SIGNED_LINK_BASE and raw < signed_link_end) {
        const offset = raw - R4X_SIGNED_LINK_BASE;
        return @ptrFromInt(base + offset);
    }
    return null;
}

fn currentProgramThreadReady() ?*ProgramThread {
    var attempts: u32 = 0;
    while (attempts < 64) : (attempts += 1) {
        if (currentProgramThread()) |thread_ctx| return thread_ctx;
        scheduler.yield();
    }
    return null;
}

fn normalizeThreadStackReserve(instance: *const ProgramInstance, requested: u64) ?u64 {
    const raw = if (requested == 0) instance.memory_limits.stack_reserve else requested;
    const reserve_len = pageAlignU64(raw) orelse return null;
    if (reserve_len < 64 * KB or reserve_len <= PROGRAM_STACK_GUARD_SIZE) return null;
    if (reserve_len > instance.memory_limits.vm_reserve_limit) return null;
    return reserve_len;
}

fn initialThreadStackCommit(instance: *const ProgramInstance, reserve_len: u64) ?u64 {
    var initial_len = pageAlignU64(instance.memory_limits.stack_initial_commit) orelse return null;
    const max_commit = reserve_len - PROGRAM_STACK_GUARD_SIZE;
    if (initial_len > max_commit) initial_len = max_commit;
    if (initial_len == 0) return null;
    return initial_len;
}

fn threadInfo(thread_ctx: *const ProgramThread) ProgramThreadInfo {
    var stack = thread_ctx.stack;
    if ((thread_ctx.flags & THREAD_FLAG_MAIN) != 0) {
        // ProgramThread lifetime is nested inside its owner ProgramInstance:
        // instance retirement releases every thread before instance storage.
        // Reading the stable owner pointer therefore needs no sleepable
        // registry lookup and remains valid in IRQ-atomic inventory copies.
        if (thread_ctx.owner_instance) |instance| {
            if (instance.id == thread_ctx.instance_id) stack = stackFromInstance(instance) orelse ProgramStack{};
        }
    }
    return .{
        .version = THREAD_INFO_VERSION,
        .size = @sizeOf(ProgramThreadInfo),
        .thread_id = thread_ctx.id,
        .instance_id = thread_ctx.instance_id,
        .task_id = thread_ctx.task_id,
        .state = @intFromEnum(thread_ctx.state),
        .flags = thread_ctx.flags,
        .exit_code = thread_ctx.exit_code,
        .stack_base = stack.base,
        .stack_reserved_bytes = stack.reserve_size,
        .stack_committed_bytes = stack.committed_size,
        .stack_guard_base = stack.guard_base,
        .created_tick = thread_ctx.created_tick,
        .finished_tick = thread_ctx.finished_tick,
        .join_count = thread_ctx.join_count,
        .reserved0 = 0,
    };
}

fn apiPrint(text: [*:0]const u8) callconv(.c) void {
    var i: usize = 0;
    while (i < 4096 and text[i] != 0) : (i += 1) {}
    apiOutputTextSpan(.stdout, text[0..i]);
}

fn apiPutc(ch: u8) callconv(.c) void {
    apiOutputTextByte(.stdout, ch);
}

fn apiOutputTextByte(stream: ConsoleStream, ch: u8) void {
    const data = [_]u8{ch};
    apiOutputTextSpan(stream, data[0..]);
}

fn captureOutputByte(ch: u8) void {
    if (output_capture) |buffer| {
        if (output_capture_len < buffer.len) {
            buffer[output_capture_len] = ch;
            output_capture_len += 1;
        } else {
            output_capture_truncated = true;
        }
    }
}

fn apiOutputTextSpan(stream: ConsoleStream, data: []const u8) void {
    if (data.len == 0) return;
    if (output_capture != null) {
        for (data) |ch| {
            if (ch == '\n') {
                captureOutputByte('\r');
                captureOutputByte('\n');
            } else if (ch != '\r') {
                captureOutputByte(ch);
            }
        }
        return;
    }
    if (routeCurrentConsoleOutputBatch(stream, data, true)) return;
    if (currentInstance()) |instance| {
        for (data) |ch| {
            if (ch == '\n') {
                mirrorHeadlessShellSmokeOutput(instance, stream, '\r');
                mirrorHeadlessShellSmokeOutput(instance, stream, '\n');
            } else if (ch != '\r') {
                mirrorHeadlessShellSmokeOutput(instance, stream, ch);
            }
        }
    }
}

fn mirrorHeadlessShellSmokeOutput(instance: *const ProgramInstance, stream: ConsoleStream, ch: u8) void {
    if (stream == .stdin) return;
    const host = if (instance.console_payload) |console| console.host else ConsoleHostKind.none;
    if (instance.role != .shell or host != .none) return;
    if (!fixedZContainsIgnoreCase(processPayloadConst(instance).args[0..], "SMOKE")) return;
    k.serialPutcRaw(ch);
}

fn apiReadKey() callconv(.c) u8 {
    console_read_calls +%= 1;
    const value = readInputByte() orelse {
        console_read_empty +%= 1;
        return 0;
    };
    console_read_bytes +%= 1;
    return value;
}

fn apiReadKeyCodepoint() callconv(.c) u32 {
    console_read_calls +%= 1;
    const value = readInputCodepoint() orelse {
        console_read_empty +%= 1;
        return 0;
    };
    console_read_bytes +%= 1;
    return value;
}

fn apiMemAlloc(size: u32, alignment: u32, out_ptr: *u64) callconv(.c) i32 {
    _ = size;
    _ = alignment;
    out_ptr.* = 0;
    return MEM_ERROR_RETIRED;
}

fn apiMemFree(ptr_addr: u64, size: u32) callconv(.c) i32 {
    _ = ptr_addr;
    _ = size;
    return MEM_ERROR_RETIRED;
}

fn apiMemRealloc(old_ptr: u64, old_size: u32, new_size: u32, alignment: u32, out_ptr: *u64) callconv(.c) i32 {
    _ = old_ptr;
    _ = old_size;
    _ = new_size;
    _ = alignment;
    out_ptr.* = 0;
    return MEM_ERROR_RETIRED;
}

fn apiMemStats(out: *ProgramMemoryStats) callconv(.c) i32 {
    out.* = .{};
    return MEM_ERROR_RETIRED;
}

fn apiMemLargestFree() callconv(.c) u64 {
    return 0;
}

fn apiMemoryVmReserveProbe(requested_bytes: u64, out: *ProgramVmReserveProbe) callconv(.c) i32 {
    out.* = .{ .requested_bytes = requested_bytes };
    fillVmProbeBefore(out);

    const instance = currentInstance() orelse {
        out.status = VM_PROBE_ERROR_NO_INSTANCE;
        fillVmProbeAfter(out);
        return VM_PROBE_OK;
    };
    const len = pageAlignU64(requested_bytes) orelse {
        out.status = VM_PROBE_ERROR_INVALID_SIZE;
        fillVmProbeAfter(out);
        return VM_PROBE_OK;
    };
    if (len == 0) {
        out.status = VM_PROBE_ERROR_INVALID_SIZE;
        fillVmProbeAfter(out);
        return VM_PROBE_OK;
    }

    const region_id = mem_virt.reserve(.{
        .window = .r4x_vm,
        .len = len,
        .kind = .virtual_range,
        .owner = .r4x_instance,
        .owner_id = @intCast(instance.id),
        .name = "r4x-vm-probe",
        .flags = paging.WRITABLE | paging.NO_EXECUTE,
    }) catch |err| {
        out.status = vmProbeErrorCode(err);
        fillVmProbeAfter(out);
        return VM_PROBE_OK;
    };
    out.region_id = region_id;
    fillVmProbeDuring(out);

    if (mem_virt.rangeInfo(region_id)) |info| {
        out.base = info.base;
        out.len = info.len;
        out.reserved_bytes = info.len;
        out.committed_bytes = info.committed_bytes;
    } else {
        out.status = VM_PROBE_ERROR_RANGE_MISSING;
    }

    if (out.status == VM_PROBE_OK) {
        if (mem_blocks.firstContainingVirtual(out.base)) |block| {
            out.reserved_bytes = block.reserved_bytes;
            out.committed_bytes = block.committed_bytes;
            out.phys_len = block.phys_len;
            out.owner_id = block.owner_id;
            out.kind = @intFromEnum(block.kind);
            out.owner = @intFromEnum(block.owner);
            out.block_status = @intFromEnum(block.status);
        } else {
            out.status = VM_PROBE_ERROR_BLOCK_MISSING;
        }
    }

    var released = true;
    mem_virt.release(region_id) catch {
        released = false;
    };
    out.released = if (released) 1 else 0;
    if (!released and out.status == VM_PROBE_OK) out.status = VM_PROBE_ERROR_RELEASE_FAILED;
    fillVmProbeAfter(out);
    return VM_PROBE_OK;
}

fn fillVmProbeBefore(out: *ProgramVmReserveProbe) void {
    const blocks = mem_blocks.summary();
    const virt = mem_virt.stats();
    out.active_before = blocks.active_blocks;
    out.committed_before = blocks.committed_bytes;
    out.largest_free_before = virt.largest_free_virtual_len;
}

fn fillVmProbeDuring(out: *ProgramVmReserveProbe) void {
    const blocks = mem_blocks.summary();
    out.active_during = blocks.active_blocks;
    out.committed_during = blocks.committed_bytes;
}

fn fillVmProbeAfter(out: *ProgramVmReserveProbe) void {
    const blocks = mem_blocks.summary();
    const virt = mem_virt.stats();
    out.active_after = blocks.active_blocks;
    out.committed_after = blocks.committed_bytes;
    out.largest_free_after = virt.largest_free_virtual_len;
    if (out.active_during == 0) out.active_during = out.active_before;
    if (out.committed_during == 0) out.committed_during = out.committed_before;
}

fn vmProbeErrorCode(err: mem_virt.Error) i32 {
    return switch (err) {
        error.EmptyRange, error.BadAlignment, error.Overflow, error.OutsideWindow => VM_PROBE_ERROR_INVALID_SIZE,
        error.TableFull => VM_PROBE_ERROR_TABLE_FULL,
        error.NoSpace, error.Overlap => VM_PROBE_ERROR_NO_SPACE,
        else => VM_PROBE_ERROR_INTERNAL,
    };
}

fn instanceDynamicVmStats(instance: *const ProgramInstance) mem_virt.OwnerStats {
    return mem_virt.ownerStats(.r4x_instance, @intCast(instance.id), .r4x_vm, .virtual_range);
}

fn instanceAllVmStats(instance: *const ProgramInstance) mem_virt.OwnerStats {
    return mem_virt.ownerStats(.r4x_instance, @intCast(instance.id), null, null);
}

fn vmReserveWithinProfile(instance: *const ProgramInstance, add_bytes: u64) bool {
    const stats = instanceDynamicVmStats(instance);
    const next = checkedAddU64(stats.reserved_bytes, add_bytes) orelse return false;
    return next <= instance.memory_limits.vm_reserve_limit;
}

fn vmCommitWithinProfile(instance: *const ProgramInstance, add_bytes: u64) bool {
    const stats = instanceDynamicVmStats(instance);
    const next = checkedAddU64(stats.committed_bytes, add_bytes) orelse return false;
    return next <= instance.memory_limits.vm_commit_limit;
}

fn vmResidentWithinProfile(instance: *const ProgramInstance, add_bytes: u64) bool {
    const stats = instanceDynamicVmStats(instance);
    const next = checkedAddU64(stats.resident_bytes, add_bytes) orelse return false;
    return next <= instance.memory_limits.resident_limit;
}

fn systemCommitLimitAllows(add_bytes: u64) bool {
    const stats = mem_virt.stats();
    const available = stats.app_available_frames * paging.PAGE_SIZE;
    const budget = checkedAddU64(stats.resident_bytes, available) orelse ~@as(u64, 0);
    const next = checkedAddU64(stats.committed_bytes, add_bytes) orelse return false;
    return next <= budget;
}

fn apiVmReserve(size: u64, alignment_raw: u64, flags: u64, out: *ProgramVmRegionInfo) callconv(.c) i32 {
    out.* = .{};
    const instance = currentInstance() orelse return VM_ERROR_NO_INSTANCE;
    const map_flags = vmPagingFlags(flags) orelse return VM_ERROR_UNSUPPORTED_FLAGS;
    const alignment = if (alignment_raw == 0) paging.PAGE_SIZE else alignment_raw;
    const reserve_len = pageAlignU64(size) orelse return VM_ERROR_INVALID_RANGE;
    if (!vmReserveWithinProfile(instance, reserve_len)) return VM_ERROR_LIMIT_EXCEEDED;
    const region_id = mem_virt.reserve(.{
        .window = .r4x_vm,
        .len = size,
        .alignment = alignment,
        .kind = .virtual_range,
        .owner = .r4x_instance,
        .owner_id = @intCast(instance.id),
        .name = "r4x-vm-region",
        .flags = map_flags,
    }) catch |err| return vmErrorCode(err);
    const rc = fillVmRegionInfoForInstance(instance.id, region_id, out);
    if (rc != VM_OK) {
        mem_virt.release(region_id) catch {};
        return rc;
    }
    return VM_OK;
}

fn apiVmCommit(region_id: u32, offset: u64, len: u64, flags: u64) callconv(.c) i32 {
    if (flags != 0) return VM_ERROR_UNSUPPORTED_FLAGS;
    const lookup = lookupCurrentVmRegion(region_id);
    if (lookup.code != VM_OK) return lookup.code;
    const commit_len = pageAlignU64(len) orelse return VM_ERROR_INVALID_RANGE;
    const instance = currentInstance() orelse return VM_ERROR_NO_INSTANCE;
    if (!vmCommitWithinProfile(instance, commit_len)) return VM_ERROR_LIMIT_EXCEEDED;
    if (!systemCommitLimitAllows(commit_len)) return VM_ERROR_LIMIT_EXCEEDED;
    mem_virt.commit(region_id, offset, len) catch |err| return vmErrorCode(err);
    return VM_OK;
}

fn apiVmDecommit(region_id: u32, offset: u64, len: u64) callconv(.c) i32 {
    const lookup = lookupCurrentVmRegion(region_id);
    if (lookup.code != VM_OK) return lookup.code;
    mem_virt.uncommit(region_id, offset, len) catch |err| return vmErrorCode(err);
    return VM_OK;
}

fn apiVmRelease(region_id: u32) callconv(.c) i32 {
    const lookup = lookupCurrentVmRegion(region_id);
    if (lookup.code != VM_OK) return lookup.code;
    mem_virt.release(region_id) catch |err| return vmErrorCode(err);
    return VM_OK;
}

fn apiVmQuery(region_id: u32, out: *ProgramVmRegionInfo) callconv(.c) i32 {
    out.* = .{};
    const instance = currentInstance() orelse return VM_ERROR_NO_INSTANCE;
    return fillVmRegionInfoForInstance(instance.id, region_id, out);
}

const VmRangeLookup = struct {
    code: i32 = VM_OK,
    info: mem_virt.RangeInfo = .{},
};

fn lookupCurrentVmRegion(region_id: u32) VmRangeLookup {
    const instance = currentInstance() orelse return .{ .code = VM_ERROR_NO_INSTANCE };
    const info = mem_virt.rangeInfo(region_id) orelse return .{ .code = VM_ERROR_INVALID_RANGE };
    if (info.owner != .r4x_instance or info.owner_id != @as(u64, @intCast(instance.id))) return .{ .code = VM_ERROR_OWNER_MISMATCH };
    if (info.window != .r4x_vm or info.kind != .virtual_range) return .{ .code = VM_ERROR_INVALID_RANGE };
    return .{ .code = VM_OK, .info = info };
}

fn fillVmRegionInfoForInstance(instance_id: u32, region_id: u32, out: *ProgramVmRegionInfo) i32 {
    const info = mem_virt.rangeInfo(region_id) orelse return VM_ERROR_INVALID_RANGE;
    if (info.owner != .r4x_instance or info.owner_id != @as(u64, @intCast(instance_id))) return VM_ERROR_OWNER_MISMATCH;
    if (info.window != .r4x_vm or info.kind != .virtual_range) return VM_ERROR_INVALID_RANGE;
    out.* = .{
        .id = info.id,
        .status = @intFromEnum(info.status),
        .owner = @intFromEnum(info.owner),
        .kind = @intFromEnum(info.kind),
        .window = @intFromEnum(info.window),
        .owner_id = info.owner_id,
        .base = info.base,
        .len = info.len,
        .committed_bytes = info.committed_bytes,
        .guard_base = info.guard_base,
        .guard_len = info.guard_len,
        .flags = vmFlagsFromPagingFlags(info.flags),
        .resident_bytes = info.resident_bytes,
        .peak_resident_bytes = info.peak_resident_bytes,
        .fault_count = info.fault_count,
        .failed_faults = info.failed_faults,
    };
    return VM_OK;
}

fn vmPagingFlags(flags_raw: u64) ?u64 {
    if ((flags_raw & ~VM_REGION_ALLOWED_FLAGS) != 0) return null;
    const flags = if (flags_raw == 0) VM_REGION_FLAG_WRITABLE else flags_raw;
    var out: u64 = paging.NO_EXECUTE;
    if ((flags & VM_REGION_FLAG_WRITABLE) != 0) out |= paging.WRITABLE;
    if ((flags & VM_REGION_FLAG_EXECUTABLE) != 0) out &= ~paging.NO_EXECUTE;
    return out;
}

fn vmFlagsFromPagingFlags(flags: u64) u64 {
    var out: u64 = 0;
    if ((flags & paging.WRITABLE) != 0) out |= VM_REGION_FLAG_WRITABLE;
    if ((flags & paging.NO_EXECUTE) == 0) out |= VM_REGION_FLAG_EXECUTABLE;
    return out;
}

fn vmErrorCode(err: mem_virt.Error) i32 {
    return switch (err) {
        error.EmptyRange, error.Overflow, error.OutsideWindow => VM_ERROR_INVALID_RANGE,
        error.BadAlignment => VM_ERROR_INVALID_ALIGNMENT,
        error.TableFull => VM_ERROR_TABLE_FULL,
        error.NoSpace, error.Overlap => VM_ERROR_NO_SPACE,
        error.NotFound => VM_ERROR_INVALID_RANGE,
        error.AlreadyCommitted => VM_ERROR_ALREADY_COMMITTED,
        error.NotCommitted => VM_ERROR_NOT_COMMITTED,
        error.GuardRange => VM_ERROR_GUARD_RANGE,
        error.OutOfMemory => VM_ERROR_OUT_OF_MEMORY,
        error.MapFailed => VM_ERROR_MAP_FAILED,
        error.NotInitialized => VM_ERROR_INVALID_RANGE,
    };
}

fn apiAllocatorAlloc(ptr: *anyopaque, len: usize, alignment: ProgramAllocatorAlignment, ret_addr: usize) ?[*]u8 {
    _ = ptr;
    _ = len;
    _ = alignment;
    _ = ret_addr;
    return null;
}

fn apiAllocatorResize(ptr: *anyopaque, memory: []u8, alignment: ProgramAllocatorAlignment, new_len: usize, ret_addr: usize) bool {
    _ = ptr;
    _ = memory;
    _ = ret_addr;
    _ = alignment;
    _ = new_len;
    return false;
}

fn apiAllocatorRemap(ptr: *anyopaque, memory: []u8, alignment: ProgramAllocatorAlignment, new_len: usize, ret_addr: usize) ?[*]u8 {
    _ = ptr;
    _ = memory;
    _ = alignment;
    _ = new_len;
    _ = ret_addr;
    return null;
}

fn apiAllocatorFree(ptr: *anyopaque, memory: []u8, alignment: ProgramAllocatorAlignment, ret_addr: usize) void {
    _ = ptr;
    _ = memory;
    _ = alignment;
    _ = ret_addr;
}

pub fn readConsoleInputByte() ?u8 {
    return readInputByte();
}

fn readInputByte() ?u8 {
    const codepoint = readInputCodepoint() orelse return null;
    if (codepoint > 0xff) return null;
    return @intCast(codepoint);
}

fn readInputCodepoint() ?u32 {
    if (input_capture) |data| {
        if (input_capture_pos >= data.len) return null;
        const ch = data[input_capture_pos];
        input_capture_pos += 1;
        return ch;
    }
    if (currentInstance()) |instance| {
        if (currentConsoleInstance()) |console_instance| {
            if (popInput(console_instance)) |ch| return ch;
            const current_console = consolePayloadConst(instance);
            const target_window_id = if (console_instance.gui_payload) |gui| gui.window_id else -1;
            if (current_console.io_target_id != 0 or target_window_id >= 0 or consolePayloadConst(console_instance).host != .none) return null;
        }
    }
    return keyboard.readCodepoint();
}

fn apiGuiSetFont(font_id: u32) callconv(.c) i32 {
    const instance = currentInstance() orelse return -1;
    if (!font.isRenderableFontId(font_id)) return -2;
    const gui = ensureGuiPayload(instance) orelse return -3;
    gui.font_id = font_id;
    bumpGuiRevision(instance);
    return 0;
}

fn apiGuiFont(instance_id: u32, out: *GuiFontInfo) callconv(.c) i32 {
    if (@intFromPtr(out) == 0) return -1;
    const selected_font_id = if (instance_id == 0) blk: {
        const instance = currentInstance() orelse return -1;
        break :blk if (instance.gui_payload) |gui| gui.font_id else GUI_FONT_BUILTIN_ID;
    } else blk: {
        reapFinishedInstances();
        const instance = instanceById(instance_id) orelse return -1;
        break :blk if (instance.gui_payload) |gui| gui.font_id else GUI_FONT_BUILTIN_ID;
    };
    return fillGuiFontInfo(selected_font_id, true, out);
}

fn fillGuiFontInfo(font_id: u32, force_selected: bool, out: *GuiFontInfo) i32 {
    out.* = .{};
    if (font_id == GUI_FONT_BUILTIN_ID) {
        out.* = .{
            .id = GUI_FONT_BUILTIN_ID,
            .kind = 0,
            .flags = GUI_FONT_FLAG_RENDERABLE | GUI_FONT_FLAG_BUILTIN | (if (force_selected or font.currentFontId() == GUI_FONT_BUILTIN_ID) GUI_FONT_FLAG_SELECTED else 0),
            .weight = 400,
            .style_flags = 0,
            .charset_flags = 0,
            .width = 8,
            .height = 8,
            .max_advance = 8,
            .line_height = 8,
            .baseline = 7,
            .glyph_count = 95,
            .strike_count = 1,
        };
        copyFixedZ(out.family[0..], "R4OS");
        copyFixedZ(out.face[0..], "Builtin 8x8");
        copyFixedZ(out.style[0..], "Regular");
        copyFixedZ(out.status[0..], "builtin fallback");
        return 1;
    }
    const entry = font.catalogEntryForFontId(font_id) orelse return 0;
    out.* = .{
        .id = font_id,
        .kind = @intFromEnum(entry.kind),
        .flags = (if (entry.renderable) GUI_FONT_FLAG_RENDERABLE else 0) | (if (force_selected or entry.selected) GUI_FONT_FLAG_SELECTED else 0),
        .weight = entry.weight,
        .style_flags = entry.style_flags,
        .charset_flags = entry.charset_flags,
        .width = entry.width,
        .height = entry.height,
        .max_advance = entry.max_advance,
        .line_height = entry.line_height,
        .baseline = entry.baseline,
        .glyph_count = entry.glyph_count,
        .strike_count = entry.strike_count,
    };
    copyFixedZ(out.path[0..], entry.path[0..entry.path_len]);
    copyFixedZ(out.family[0..], entry.family[0..entry.family_len]);
    copyFixedZ(out.face[0..], entry.face[0..entry.face_len]);
    copyFixedZ(out.style[0..], entry.style[0..entry.style_len]);
    copyFixedZ(out.status[0..], entry.status[0..entry.status_len]);
    return 1;
}

fn apiProgramRun(path_ptr: [*:0]const u8, args_ptr: [*:0]const u8) callconv(.c) i32 {
    return apiProgramLaunch(path_ptr, args_ptr, @intFromEnum(LaunchPolicy.auto));
}

fn apiProgramLaunch(path_ptr: [*:0]const u8, args_ptr: [*:0]const u8, policy_raw: u32) callconv(.c) i32 {
    var path_buf: [MAX_API_PATH]u8 = undefined;
    const raw_path = copyZ(path_ptr, path_buf[0..]) orelse return -2;
    var args_buf: [MAX_API_ARGS]u8 = undefined;
    const raw_args = copyZ(args_ptr, args_buf[0..]) orelse return -2;
    var resolved_buf: [MAX_API_PATH]u8 = undefined;
    const target = resolveApiTarget(raw_path, &resolved_buf) orelse return -2;
    const file = resolveProgramFile(target.drive_ref, target.path) orelse return -1;
    const policy = parseLaunchPolicy(policy_raw) orelse return -2;
    return switch (runProgramFile(file, .foreground, policy, raw_args, target.drive_ref, .none, .{
        .owner = true,
        .owner_handle = ProgramProcessHandle{},
    })) {
        .ran => 0,
        .not_found => -1,
        .failed => -2,
    };
}

fn apiProgramSpawn(path_ptr: [*:0]const u8, args_ptr: [*:0]const u8, policy_raw: u32) callconv(.c) i32 {
    return apiProgramSpawnWithConsoleHost(path_ptr, args_ptr, policy_raw, @intFromEnum(ConsoleHostKind.none));
}

fn apiProgramSpawnWithConsoleHost(path_ptr: [*:0]const u8, args_ptr: [*:0]const u8, policy_raw: u32, host_raw: u32) callconv(.c) i32 {
    var path_buf: [MAX_API_PATH]u8 = undefined;
    const raw_path = copyZ(path_ptr, path_buf[0..]) orelse return -2;
    var args_buf: [MAX_API_ARGS]u8 = undefined;
    const raw_args = copyZ(args_ptr, args_buf[0..]) orelse return -2;
    var resolved_buf: [MAX_API_PATH]u8 = undefined;
    const target = resolveApiTarget(raw_path, &resolved_buf) orelse return -2;
    const file = resolveProgramFile(target.drive_ref, target.path) orelse return -1;
    const policy = parseLaunchPolicy(policy_raw) orelse return -2;
    const host = parseConsoleHost(host_raw) orelse return -2;
    var handle: ProgramProcessHandle = .{};
    return switch (runProgramFile(file, .background, policy, raw_args, target.drive_ref, host, .{
        // Every returned legacy ID remains collision-blocking until its
        // explicit ID reap. Bind to the R4X caller (or external owner when no
        // caller exists) so Desktop/SSHSVC can observe it durably without
        // leaking the completion if that owning program exits first.
        .owner = true,
        .legacy_id = true,
        // Host assignment may happen after spawn, and RFDIAG intentionally
        // stays host-less. Preserve console output for every legacy ID launch.
        .retain_output = true,
        .out_handle = &handle,
    })) {
        .ran => @intCast(handle.instance_id),
        .not_found => -1,
        .failed => -2,
    };
}

fn apiProgramSpawnHandle(path_ptr: [*:0]const u8, args_ptr: [*:0]const u8, policy_raw: u32, out: *ProgramProcessHandle) callconv(.c) i32 {
    if (@intFromPtr(out) == 0 or @intFromPtr(path_ptr) == 0 or @intFromPtr(args_ptr) == 0) return PROGRAM_HANDLE_ERROR_INVALID;
    out.* = .{};
    var path_buf: [MAX_API_PATH]u8 = undefined;
    const raw_path = copyZ(path_ptr, path_buf[0..]) orelse return PROGRAM_HANDLE_ERROR_INVALID;
    var args_buf: [MAX_API_ARGS]u8 = undefined;
    const raw_args = copyZ(args_ptr, args_buf[0..]) orelse return PROGRAM_HANDLE_ERROR_INVALID;
    var resolved_buf: [MAX_API_PATH]u8 = undefined;
    const target = resolveApiTarget(raw_path, &resolved_buf) orelse return PROGRAM_HANDLE_ERROR_INVALID;
    const policy = parseLaunchPolicy(policy_raw) orelse return PROGRAM_HANDLE_ERROR_INVALID;
    const file = resolveProgramFile(target.drive_ref, target.path) orelse return PROGRAM_HANDLE_ERROR_NOT_FOUND;
    var launch_error = PROGRAM_HANDLE_OK;
    return switch (runProgramFile(file, .background, policy, raw_args, target.drive_ref, .none, .{
        .owner = true,
        .retain_output = true,
        .out_handle = out,
        .error_out = &launch_error,
    })) {
        .ran => PROGRAM_HANDLE_OK,
        .not_found => PROGRAM_HANDLE_ERROR_NOT_FOUND,
        .failed => if (launch_error == PROGRAM_HANDLE_OK) PROGRAM_HANDLE_ERROR_TASK_FAILED else launch_error,
    };
}

fn apiProgramSpawnWithConsoleHostHandle(path_ptr: [*:0]const u8, args_ptr: [*:0]const u8, policy_raw: u32, host_raw: u32, out: *ProgramProcessHandle) callconv(.c) i32 {
    if (@intFromPtr(out) == 0 or @intFromPtr(path_ptr) == 0 or @intFromPtr(args_ptr) == 0) return PROGRAM_HANDLE_ERROR_INVALID;
    out.* = .{};
    var path_buf: [MAX_API_PATH]u8 = undefined;
    const raw_path = copyZ(path_ptr, path_buf[0..]) orelse return PROGRAM_HANDLE_ERROR_INVALID;
    var args_buf: [MAX_API_ARGS]u8 = undefined;
    const raw_args = copyZ(args_ptr, args_buf[0..]) orelse return PROGRAM_HANDLE_ERROR_INVALID;
    var resolved_buf: [MAX_API_PATH]u8 = undefined;
    const target = resolveApiTarget(raw_path, &resolved_buf) orelse return PROGRAM_HANDLE_ERROR_INVALID;
    const policy = parseLaunchPolicy(policy_raw) orelse return PROGRAM_HANDLE_ERROR_INVALID;
    const host = parseConsoleHost(host_raw) orelse return PROGRAM_HANDLE_ERROR_INVALID;
    const file = resolveProgramFile(target.drive_ref, target.path) orelse return PROGRAM_HANDLE_ERROR_NOT_FOUND;
    var launch_error = PROGRAM_HANDLE_OK;
    return switch (runProgramFile(file, .background, policy, raw_args, target.drive_ref, host, .{
        .owner = true,
        .retain_output = true,
        .out_handle = out,
        .error_out = &launch_error,
    })) {
        .ran => PROGRAM_HANDLE_OK,
        .not_found => PROGRAM_HANDLE_ERROR_NOT_FOUND,
        .failed => if (launch_error == PROGRAM_HANDLE_OK) PROGRAM_HANDLE_ERROR_TASK_FAILED else launch_error,
    };
}

fn apiProgramOpenHandle(id: u32, out: *ProgramProcessHandle) callconv(.c) i32 {
    if (@intFromPtr(out) == 0 or id == 0) return PROGRAM_HANDLE_ERROR_INVALID;
    out.* = .{};
    const locked = lockProgramRegistry();
    if (!locked) return PROGRAM_HANDLE_ERROR_WOULD_BLOCK;
    defer unlockProgramRegistry();
    if (lookupProgramRegistrySlotLocked(id, true)) |slot| {
        out.* = programHandleForSlot(slot);
        return PROGRAM_HANDLE_OK;
    }
    var node = program_completion_head;
    while (node) |current| : (node = current.next) {
        if (current.state == .consumed or current.handle.instance_id != id) continue;
        out.* = current.handle;
        return PROGRAM_HANDLE_OK;
    }
    return PROGRAM_HANDLE_ERROR_NOT_FOUND;
}

fn completionInstanceInfo(node: *const ProgramCompletionNode) ProgramInstanceInfo {
    return .{
        .id = node.handle.instance_id,
        .task_id = node.task_id,
        .role = node.role,
        .app_class = node.app_class,
        // Exit is committed before asynchronous heavy teardown finishes.
        // Legacy ID enumeration must keep the row visible, but may advertise
        // `.done` only once programReapInstance can actually consume it.
        .state = @intFromEnum(if (node.state == .ready) InstanceState.done else InstanceState.close_requested),
        .exit_code = node.exit_code,
    };
}

fn apiProgramHandleStatus(handle_ptr: *const ProgramProcessHandle, out: *ProgramInstanceInfo) callconv(.c) i32 {
    if (@intFromPtr(handle_ptr) == 0 or @intFromPtr(out) == 0) return PROGRAM_HANDLE_ERROR_INVALID;
    out.* = .{};
    const handle = handle_ptr.*;
    if (!programHandleValid(handle)) return PROGRAM_HANDLE_ERROR_INVALID;
    if (pinProgramHandle(handle, true)) |lease| {
        out.* = instanceInfo(lease.instance);
        unpinProgramInstance(&lease);
        return PROGRAM_HANDLE_OK;
    }
    const locked = lockProgramRegistry();
    if (!locked) return PROGRAM_HANDLE_ERROR_WOULD_BLOCK;
    defer unlockProgramRegistry();
    if (completionForHandleLocked(handle)) |node| {
        out.* = completionInstanceInfo(node);
        return PROGRAM_HANDLE_OK;
    }
    return programHandleMissingStatusLocked(handle);
}

fn apiProgramHandleRequestClose(handle_ptr: *const ProgramProcessHandle) callconv(.c) i32 {
    if (@intFromPtr(handle_ptr) == 0 or !programHandleValid(handle_ptr.*)) return PROGRAM_HANDLE_ERROR_INVALID;
    const handle = handle_ptr.*;
    const locked = lockProgramRegistry();
    if (!locked) return PROGRAM_HANDLE_ERROR_WOULD_BLOCK;
    const slot = lookupProgramRegistryHandleLocked(handle, true) orelse {
        const status = if (completionForHandleLocked(handle) != null) PROGRAM_HANDLE_ERROR_NOT_RUNNING else programHandleMissingStatusLocked(handle);
        unlockProgramRegistry();
        return status;
    };
    if (!programRegistryStateIsRunning(slot.state)) {
        unlockProgramRegistry();
        return PROGRAM_HANDLE_ERROR_NOT_RUNNING;
    }
    if (!slot.instance.close_requested) {
        slot.instance.close_requested = true;
        bumpProgramInventoryEpochLocked();
    }
    unlockProgramRegistry();
    signalConsoleInputForHandle(handle);
    requestConsoleClientsClose(handle);
    return PROGRAM_HANDLE_OK;
}

fn apiProgramHandleKill(handle_ptr: *const ProgramProcessHandle) callconv(.c) i32 {
    if (@intFromPtr(handle_ptr) == 0 or !programHandleValid(handle_ptr.*)) return PROGRAM_HANDLE_ERROR_INVALID;
    const handle = handle_ptr.*;
    if (currentProgramHandle()) |current| if (programHandleEqual(current, handle)) return PROGRAM_HANDLE_ERROR_SELF;
    const locked = lockProgramRegistry();
    if (!locked) return PROGRAM_HANDLE_ERROR_WOULD_BLOCK;
    const slot = lookupProgramRegistryHandleLocked(handle, true) orelse {
        const status = if (completionForHandleLocked(handle) != null) PROGRAM_HANDLE_ERROR_NOT_RUNNING else programHandleMissingStatusLocked(handle);
        unlockProgramRegistry();
        return status;
    };
    if (!programRegistryStateIsRunning(slot.state)) {
        unlockProgramRegistry();
        return PROGRAM_HANDLE_ERROR_NOT_RUNNING;
    }
    unlockProgramRegistry();
    while (true) {
        switch (commitProgramExit(handle, -9, PROGRAM_EXIT_REASON_KILLED)) {
            .committed => {
                killConsoleClients(handle);
                program_reaper_event.signal();
                return PROGRAM_HANDLE_OK;
            },
            .already_exiting => return PROGRAM_HANDLE_ERROR_NOT_RUNNING,
            .retry => {},
        }
        // Natural exit may win after the initial running check. Never spin on
        // an already detached owned completion; only a still-running slot
        // (for example after the one-shot exit-commit failure seam) retries.
        const retry_locked = lockProgramRegistry();
        if (!retry_locked) return PROGRAM_HANDLE_ERROR_WOULD_BLOCK;
        const retry_slot = lookupProgramRegistryHandleLocked(handle, true);
        if (retry_slot == null) {
            const status = if (completionForHandleLocked(handle) != null)
                PROGRAM_HANDLE_ERROR_NOT_RUNNING
            else
                programHandleMissingStatusLocked(handle);
            unlockProgramRegistry();
            return status;
        }
        if (!programRegistryStateIsRunning(retry_slot.?.state)) {
            unlockProgramRegistry();
            return PROGRAM_HANDLE_ERROR_NOT_RUNNING;
        }
        unlockProgramRegistry();
        scheduler.yield();
    }
}

fn apiProgramHandleWait(handle_ptr: *const ProgramProcessHandle, timeout_ticks: u64, out: *ProgramProcessCompletion) callconv(.c) i32 {
    if (@intFromPtr(handle_ptr) == 0 or @intFromPtr(out) == 0 or !programHandleValid(handle_ptr.*)) return PROGRAM_HANDLE_ERROR_INVALID;
    out.* = .{};
    const handle = handle_ptr.*;
    const start = timer.tickCount();
    while (true) {
        const status = readProgramCompletion(handle, out);
        if (status != PROGRAM_HANDLE_ERROR_WOULD_BLOCK) return status;
        if (currentProgramHandle()) |current| if (programHandleEqual(current, handle)) return PROGRAM_HANDLE_ERROR_SELF;
        if (timeout_ticks != std.math.maxInt(u64)) {
            const now = timer.tickCount();
            if (now -% start >= timeout_ticks) return PROGRAM_HANDLE_ERROR_TIMEOUT;
        }
        scheduler.yield();
    }
}

fn apiProgramHandleReap(handle_ptr: *const ProgramProcessHandle, out: *ProgramProcessCompletion) callconv(.c) i32 {
    if (@intFromPtr(handle_ptr) == 0 or @intFromPtr(out) == 0 or !programHandleValid(handle_ptr.*)) return PROGRAM_HANDLE_ERROR_INVALID;
    out.* = .{};
    return consumeProgramCompletion(handle_ptr.*, out);
}

fn apiProgramCompletionRead(handle_ptr: *const ProgramProcessHandle, offset: u32, out: [*]u8, capacity: u32, out_read: *u32) callconv(.c) i32 {
    if (@intFromPtr(handle_ptr) == 0 or @intFromPtr(out_read) == 0 or !programHandleValid(handle_ptr.*)) return PROGRAM_HANDLE_ERROR_INVALID;
    out_read.* = 0;
    if (@intFromPtr(out) == 0) return PROGRAM_HANDLE_ERROR_INVALID;
    const locked = lockProgramRegistry();
    if (!locked) return PROGRAM_HANDLE_ERROR_WOULD_BLOCK;
    defer unlockProgramRegistry();
    const node = completionForHandleLocked(handle_ptr.*) orelse return programHandleMissingStatusLocked(handle_ptr.*);
    if (node.state != .ready) return PROGRAM_HANDLE_ERROR_WOULD_BLOCK;
    const transcript = node.output_payload orelse return PROGRAM_HANDLE_ERROR_OUTPUT_UNAVAILABLE;
    if ((node.flags & PROGRAM_COMPLETION_FLAG_OUTPUT) == 0) return PROGRAM_HANDLE_ERROR_OUTPUT_UNAVAILABLE;
    const start: usize = @intCast(offset);
    const length: usize = @intCast(node.output_length);
    if (start > length) return PROGRAM_HANDLE_ERROR_OUTPUT_RANGE;
    const count = @min(@as(usize, @intCast(capacity)), length - start);
    if (count != 0 and readConsoleTranscript(transcript, start, out[0..count]) != count) return PROGRAM_HANDLE_ERROR_OUTPUT_RANGE;
    out_read.* = @intCast(count);
    return PROGRAM_HANDLE_OK;
}

fn apiProgramRequestClose(id: u32) callconv(.c) i32 {
    var handle: ProgramProcessHandle = .{};
    if (apiProgramOpenHandle(id, &handle) != PROGRAM_HANDLE_OK) return -1;
    return if (apiProgramHandleRequestClose(&handle) == PROGRAM_HANDLE_OK) 0 else -2;
}

fn apiProgramShouldClose() callconv(.c) u32 {
    const instance = currentInstance() orelse return 0;
    return if (instance.close_requested) 1 else 0;
}

fn apiProgramKill(id: u32) callconv(.c) i32 {
    var handle: ProgramProcessHandle = .{};
    if (apiProgramOpenHandle(id, &handle) != PROGRAM_HANDLE_OK) return -1;
    return if (apiProgramHandleKill(&handle) == PROGRAM_HANDLE_OK) 0 else -2;
}

fn apiProgramReapInstance(id: u32) callconv(.c) i32 {
    var handle: ProgramProcessHandle = .{};
    // The retained completion is the legacy API's single-use ownership
    // marker.  Exit history is deliberately diagnostic and non-consuming; it
    // must never resurrect an already reaped ID on a second call.
    if (apiProgramOpenHandle(id, &handle) != PROGRAM_HANDLE_OK) return -1;
    var completion: ProgramProcessCompletion = .{};
    const status = apiProgramHandleReap(&handle, &completion);
    return if (status == PROGRAM_HANDLE_OK) completion.exit_code else -2;
}

const ThreadInventoryStats = struct {
    epoch: u64 = 0,
    total: u32 = 0,
    running: u32 = 0,
    done: u32 = 0,
    joining: u32 = 0,
    peak: u32 = 0,
    create_failures: u64 = 0,
};

const ProgramInventoryCounts = struct {
    total: u32 = 0,
    reserved: u32 = 0,
    active: u32 = 0,
    done: u32 = 0,
    retiring: u32 = 0,
};

const ProgramInventoryRegistrySnapshot = struct {
    epoch: u64 = 0,
    counts: ProgramInventoryCounts = .{},
    completion_total: u32 = 0,
    peak: u32 = 0,
    create_failures: u64 = 0,
    rollback_count: u64 = 0,
    last_error: i32 = 0,
};

const ProgramInventorySelection = struct {
    generation: u64 = 0,
    snapshot: ProgramInstanceSnapshot = .{},
};

const ProgramInventoryScan = struct {
    counts: ProgramInventoryCounts = .{},
    returned: u32 = 0,
    has_more: bool = false,
};

const PROGRAM_INVENTORY_PAGE_MAX: usize = @intCast(r4x_api.program_inventory_page_max);

fn programInventorySelectionSiftUp(items: []ProgramInventorySelection, start_index: usize) void {
    var index = start_index;
    while (index != 0) {
        const parent = (index - 1) / 2;
        if (items[parent].generation >= items[index].generation) break;
        const tmp = items[parent];
        items[parent] = items[index];
        items[index] = tmp;
        index = parent;
    }
}

fn programInventorySelectionSiftDown(items: []ProgramInventorySelection, start_index: usize) void {
    var index = start_index;
    while (true) {
        const left = index * 2 + 1;
        if (left >= items.len) return;
        const right = left + 1;
        var largest = left;
        if (right < items.len and items[right].generation > items[left].generation) largest = right;
        if (items[index].generation >= items[largest].generation) return;
        const tmp = items[index];
        items[index] = items[largest];
        items[largest] = tmp;
        index = largest;
    }
}

fn sortProgramInventorySelections(items: []ProgramInventorySelection) void {
    var index: usize = 1;
    while (index < items.len) : (index += 1) {
        const value = items[index];
        var insert = index;
        while (insert != 0 and items[insert - 1].generation > value.generation) : (insert -= 1) {
            items[insert] = items[insert - 1];
        }
        items[insert] = value;
    }
}

fn considerProgramInventorySelection(
    selections: []ProgramInventorySelection,
    selected: *usize,
    generation: u64,
    snapshot: ProgramInstanceSnapshot,
) void {
    if (selections.len == 0) return;
    if (selected.* < selections.len) {
        selections[selected.*] = .{ .generation = generation, .snapshot = snapshot };
        programInventorySelectionSiftUp(selections[0 .. selected.* + 1], selected.*);
        selected.* += 1;
    } else if (generation < selections[0].generation) {
        selections[0] = .{ .generation = generation, .snapshot = snapshot };
        programInventorySelectionSiftDown(selections[0..selected.*], 0);
    }
}

fn programInventorySelectionCouldEnter(selections: []const ProgramInventorySelection, selected: usize, generation: u64) bool {
    return selections.len != 0 and (selected < selections.len or generation < selections[0].generation);
}

fn scanProgramInventoryLocked(
    after_generation: u64,
    selections: []ProgramInventorySelection,
) ProgramInventoryScan {
    var counts: ProgramInventoryCounts = .{};
    var eligible: u32 = 0;
    var selected: usize = 0;

    var chunk = program_registry_head;
    while (chunk) |current_chunk| : (chunk = current_chunk.next) {
        for (&current_chunk.slots) |*slot| {
            switch (slot.state) {
                .free => {},
                .create => counts.reserved +|= 1,
                .publish, .run => {
                    counts.total +|= 1;
                    counts.active +|= 1;
                    if (slot.generation <= after_generation) continue;
                    eligible +|= 1;
                    if (!programInventorySelectionCouldEnter(selections, selected, slot.generation)) continue;
                    considerProgramInventorySelection(selections, &selected, slot.generation, .{
                        .version = r4x_api.program_inventory_version,
                        .size = @sizeOf(ProgramInstanceSnapshot),
                        .handle = programHandleForSlot(slot),
                        .info = instanceInfo(&slot.instance),
                        .state_generation = slot.generation,
                    });
                },
                .exit, .done => {
                    counts.total +|= 1;
                    if (slot.state == .done) counts.done +|= 1 else counts.retiring +|= 1;
                    if (slot.generation <= after_generation) continue;
                    eligible +|= 1;
                    if (!programInventorySelectionCouldEnter(selections, selected, slot.generation)) continue;
                    considerProgramInventorySelection(selections, &selected, slot.generation, .{
                        .version = r4x_api.program_inventory_version,
                        .size = @sizeOf(ProgramInstanceSnapshot),
                        .handle = programHandleForSlot(slot),
                        .info = instanceInfo(&slot.instance),
                        .state_generation = slot.generation,
                    });
                },
                .retire, .reap => {
                    // Partially released ProgramInstance storage is never
                    // exposed. Its immutable completion is the sole safe row.
                    const completion = slot.completion orelse continue;
                    if (completion.state == .consumed) continue;
                    counts.total +|= 1;
                    if (completion.state == .ready) counts.done +|= 1 else counts.retiring +|= 1;
                    if (completion.handle.generation <= after_generation) continue;
                    eligible +|= 1;
                    if (!programInventorySelectionCouldEnter(selections, selected, completion.handle.generation)) continue;
                    considerProgramInventorySelection(selections, &selected, completion.handle.generation, .{
                        .version = r4x_api.program_inventory_version,
                        .size = @sizeOf(ProgramInstanceSnapshot),
                        .handle = completion.handle,
                        .info = completionInstanceInfo(completion),
                        .state_generation = completion.handle.generation,
                    });
                },
            }
        }
    }

    // A ready completion outlives its already-cleared slot. Pending
    // completions still have a live/retiring slot and were handled above.
    var node = program_completion_head;
    while (node) |completion| : (node = completion.next) {
        if (completion.state != .ready or completion.slot_attached) continue;
        counts.total +|= 1;
        counts.done +|= 1;
        if (completion.handle.generation <= after_generation) continue;
        eligible +|= 1;
        if (!programInventorySelectionCouldEnter(selections, selected, completion.handle.generation)) continue;
        considerProgramInventorySelection(selections, &selected, completion.handle.generation, .{
            .version = r4x_api.program_inventory_version,
            .size = @sizeOf(ProgramInstanceSnapshot),
            .handle = completion.handle,
            .info = completionInstanceInfo(completion),
            .state_generation = completion.handle.generation,
        });
    }

    sortProgramInventorySelections(selections[0..selected]);
    return .{
        .counts = counts,
        .returned = @intCast(selected),
        .has_more = eligible > @as(u32, @intCast(selected)),
    };
}

fn tryProgramInventoryRegistrySnapshot() ?ProgramInventoryRegistrySnapshot {
    if (!program_registry_lock.tryLock()) return null;
    defer unlockProgramRegistry();
    const scan = scanProgramInventoryLocked(std.math.maxInt(u64), &.{});
    return .{
        .epoch = program_registry_mutation_epoch,
        .counts = scan.counts,
        .completion_total = @intCast(@min(
            program_registry_stats.completion_pending +| program_registry_stats.completion_ready,
            std.math.maxInt(u32),
        )),
        .peak = @intCast(@min(program_registry_stats.peak_live, std.math.maxInt(u32))),
        .create_failures = program_registry_stats.launch_failures,
        .rollback_count = program_registry_stats.rollback_count,
        .last_error = if (program_registry_stats.last_launch_error != PROGRAM_HANDLE_OK)
            program_registry_stats.last_launch_error
        else
            program_registry_stats.last_admission_error,
    };
}

fn tryProgramInventoryEpoch() ?u64 {
    if (!program_registry_lock.tryLock()) return null;
    defer unlockProgramRegistry();
    return program_registry_mutation_epoch;
}

fn threadInventoryStats() ThreadInventoryStats {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    var out = ThreadInventoryStats{
        .epoch = program_thread_mutation_epoch,
        .total = @intCast(@min(program_thread_count, @as(usize, std.math.maxInt(u32)))),
        .peak = @intCast(@min(program_thread_peak, @as(usize, std.math.maxInt(u32)))),
        .create_failures = program_thread_create_failures,
    };
    var cursor = program_thread_head;
    while (cursor) |thread_ctx| : (cursor = thread_ctx.registry_next) {
        switch (thread_ctx.state) {
            .ready, .running => out.running +|= 1,
            .exited, .killed => out.done +|= 1,
            .unused => {},
        }
        if (thread_ctx.join_lease_active) out.joining +|= 1;
    }
    return out;
}

fn allocateInventorySnapshotGeneration() u64 {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    const generation = next_inventory_snapshot_generation;
    next_inventory_snapshot_generation +%= 1;
    if (next_inventory_snapshot_generation == 0) next_inventory_snapshot_generation = 1;
    return if (generation == 0) 1 else generation;
}

fn captureExecutionInventorySummary(out: *ProgramInventorySummary) bool {
    var attempt: u32 = 0;
    while (attempt < 8) : (attempt += 1) {
        const program_before = tryProgramInventoryRegistrySnapshot() orelse continue;
        const task_before = task.inventoryEpoch();
        const thread_before = threadInventoryStats();

        // Runnable/running scheduler counters and failure telemetry are
        // instantaneous values. Epochs protect membership plus durable
        // lifecycle changes; they intentionally do not advance every tick.
        const tasks = task.summary();
        const task_failures = task.createFailureStats();
        const heap_snapshot = heap.stats();
        const thread_after = threadInventoryStats();
        const task_after = task.inventoryEpoch();
        const program_after = tryProgramInventoryEpoch() orelse continue;
        if (program_before.epoch != program_after or task_before != task_after or thread_before.epoch != thread_after.epoch) continue;

        const task_failure_total = task_failures.task_metadata +|
            task_failures.stack +| task_failures.fpu +| task_failures.waiter +| task_failures.memory;
        out.* = .{
            .version = r4x_api.program_inventory_version,
            .size = @sizeOf(ProgramInventorySummary),
            .snapshot_generation = allocateInventorySnapshotGeneration(),
            .program_epoch = program_after,
            .task_epoch = task_after,
            .thread_epoch = thread_after.epoch,
            .program_total = program_before.counts.total,
            .program_active = program_before.counts.active,
            .program_done = program_before.counts.done,
            .program_retiring = program_before.counts.retiring,
            .completion_total = program_before.completion_total,
            .task_total = tasks.total,
            .task_running = tasks.running,
            .task_ready = tasks.ready,
            .task_blocked = tasks.blocked,
            .thread_total = thread_after.total,
            .thread_running = thread_after.running,
            .thread_done = thread_after.done,
            .thread_joining = thread_after.joining,
            .program_peak = program_before.peak,
            .task_peak = task.inventoryPeak(),
            .thread_peak = thread_after.peak,
            .program_create_failures = program_before.create_failures,
            .task_create_failures = task_failure_total,
            .thread_create_failures = thread_after.create_failures,
            .rollback_failures = program_before.rollback_count +| @as(u64, task.unpublishedRollbackPending()),
            .last_error = program_before.last_error,
            .flags = r4x_api.program_inventory_summary_flag_stable,
            .program_reserved = program_before.counts.reserved,
            // The kernel heap window is capped at 1 GB and each allocation
            // occupies at least one 64-byte block, so this conversion is
            // lossless by construction (at most 16,777,216 active blocks).
            .heap_active_blocks = @intCast(heap_snapshot.active_blocks),
            .heap_used_bytes = @intCast(heap_snapshot.used_bytes),
        };
        return true;
    }
    return false;
}

fn fillExecutionInventorySummaryProvider(out: *ProgramInventorySummary) i32 {
    return if (captureExecutionInventorySummary(out)) PROGRAM_HANDLE_OK else PROGRAM_HANDLE_ERROR_WOULD_BLOCK;
}

fn apiProgramInventoryBegin(cursor: *ProgramInventoryCursor, out_summary: *ProgramInventorySummary) callconv(.c) i32 {
    if (@intFromPtr(cursor) == 0 or @intFromPtr(out_summary) == 0) return PROGRAM_HANDLE_ERROR_INVALID;
    const previous_restarts = if (cursor.version == r4x_api.program_inventory_version and
        cursor.size >= @sizeOf(ProgramInventoryCursor)) cursor.restarts else 0;
    if (!captureExecutionInventorySummary(out_summary)) return PROGRAM_HANDLE_ERROR_WOULD_BLOCK;
    cursor.* = .{
        .version = r4x_api.program_inventory_version,
        .size = @sizeOf(ProgramInventoryCursor),
        .snapshot_generation = out_summary.snapshot_generation,
        .program_epoch = out_summary.program_epoch,
        .task_epoch = out_summary.task_epoch,
        .thread_epoch = out_summary.thread_epoch,
        .flags = r4x_api.program_inventory_cursor_flag_initialized,
        .restarts = previous_restarts,
    };
    return PROGRAM_HANDLE_OK;
}

fn inventoryCursorValid(cursor: *const ProgramInventoryCursor) bool {
    return cursor.version == r4x_api.program_inventory_version and
        cursor.size >= @sizeOf(ProgramInventoryCursor) and
        cursor.snapshot_generation != 0 and
        (cursor.flags & r4x_api.program_inventory_cursor_flag_initialized) != 0;
}

fn initInventoryPage(cursor: *const ProgramInventoryCursor, out_page: *ProgramInventoryPageInfo, kind: u32) void {
    out_page.* = .{
        .version = r4x_api.program_inventory_version,
        .size = @sizeOf(ProgramInventoryPageInfo),
        .snapshot_generation = cursor.snapshot_generation,
        .kind = kind,
        .status = r4x_api.program_inventory_status_invalid,
    };
}

fn apiProgramInventoryPrograms(cursor: *ProgramInventoryCursor, out: [*]ProgramInstanceSnapshot, capacity: u32, out_page: *ProgramInventoryPageInfo) callconv(.c) i32 {
    if (@intFromPtr(cursor) == 0 or @intFromPtr(out) == 0 or @intFromPtr(out_page) == 0) return PROGRAM_HANDLE_ERROR_INVALID;
    initInventoryPage(cursor, out_page, r4x_api.program_inventory_kind_program);
    if (!inventoryCursorValid(cursor) or capacity == 0 or capacity > r4x_api.program_inventory_page_max) return PROGRAM_HANDLE_ERROR_INVALID;
    if (!program_registry_lock.tryLock()) return PROGRAM_HANDLE_ERROR_WOULD_BLOCK;
    defer unlockProgramRegistry();
    if (program_registry_mutation_epoch != cursor.program_epoch) {
        out_page.status = r4x_api.program_inventory_status_restart;
        return PROGRAM_HANDLE_OK;
    }
    var selections: [PROGRAM_INVENTORY_PAGE_MAX]ProgramInventorySelection = undefined;
    const scan = scanProgramInventoryLocked(cursor.program_after_generation, selections[0..@intCast(capacity)]);
    const returned: usize = @intCast(scan.returned);
    var index: usize = 0;
    while (index < returned) : (index += 1) out[index] = selections[index].snapshot;
    const after_generation = if (returned == 0)
        cursor.program_after_generation
    else
        selections[returned - 1].generation;
    cursor.program_after_generation = after_generation;
    out_page.* = .{
        .version = r4x_api.program_inventory_version,
        .size = @sizeOf(ProgramInventoryPageInfo),
        .snapshot_generation = cursor.snapshot_generation,
        .next_generation = after_generation,
        .total = scan.counts.total,
        .returned = scan.returned,
        .has_more = if (scan.has_more) 1 else 0,
        .kind = r4x_api.program_inventory_kind_program,
        .status = if (scan.has_more) r4x_api.program_inventory_status_more else r4x_api.program_inventory_status_complete,
    };
    return PROGRAM_HANDLE_OK;
}

fn threadInventoryEpoch() u64 {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    return program_thread_mutation_epoch;
}

fn populateProgramTaskOwners(entries: []ProgramTaskSnapshot, expected_thread_epoch: u64) bool {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    if (program_thread_mutation_epoch != expected_thread_epoch) return false;
    var thread_cursor = program_thread_head;
    while (thread_cursor) |thread_ctx| : (thread_cursor = thread_ctx.registry_next) {
        if (!thread_ctx.used or thread_ctx.task_generation == 0) continue;
        var low: usize = 0;
        var high: usize = entries.len;
        while (low < high) {
            const middle = low + (high - low) / 2;
            const generation = entries[middle].generation;
            if (generation < thread_ctx.task_generation) {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        if (low < entries.len and
            entries[low].generation == thread_ctx.task_generation and
            entries[low].task_id == thread_ctx.task_id)
        {
            entries[low].owner_instance_id = thread_ctx.instance_id;
            entries[low].instance_generation = thread_ctx.instance_generation;
        }
    }
    return program_thread_mutation_epoch == expected_thread_epoch;
}

fn apiProgramInventoryTasks(cursor: *ProgramInventoryCursor, out: [*]ProgramTaskSnapshot, capacity: u32, out_page: *ProgramInventoryPageInfo) callconv(.c) i32 {
    if (@intFromPtr(cursor) == 0 or @intFromPtr(out) == 0 or @intFromPtr(out_page) == 0) return PROGRAM_HANDLE_ERROR_INVALID;
    initInventoryPage(cursor, out_page, r4x_api.program_inventory_kind_task);
    if (!inventoryCursorValid(cursor) or capacity == 0 or capacity > r4x_api.program_inventory_page_max) return PROGRAM_HANDLE_ERROR_INVALID;

    var raw: [PROGRAM_INVENTORY_PAGE_MAX]task.InventorySnapshot = undefined;
    const raw_page = task.inventoryPage(cursor.task_after_generation, raw[0..@intCast(capacity)]);
    if (raw_page.epoch != cursor.task_epoch or threadInventoryEpoch() != cursor.thread_epoch) {
        out_page.status = r4x_api.program_inventory_status_restart;
        return PROGRAM_HANDLE_OK;
    }

    var staged: [PROGRAM_INVENTORY_PAGE_MAX]ProgramTaskSnapshot = undefined;
    const returned: usize = @intCast(raw_page.returned);
    var index: usize = 0;
    while (index < returned) : (index += 1) {
        const snapshot = raw[index];
        staged[index] = .{
            .version = r4x_api.program_inventory_version,
            .size = @sizeOf(ProgramTaskSnapshot),
            .task_id = snapshot.task_id,
            .state = @intFromEnum(snapshot.state),
            .owner_instance_id = 0,
            .flags = 0,
            .generation = snapshot.generation,
            .instance_generation = 0,
            .created_tick = snapshot.created_tick,
            .last_run_tick = snapshot.last_run_tick,
            .wake_tick = snapshot.wake_tick,
            .runtime_ticks = snapshot.runtime_ticks,
        };
    }
    if (!populateProgramTaskOwners(staged[0..returned], cursor.thread_epoch)) {
        out_page.status = r4x_api.program_inventory_status_restart;
        return PROGRAM_HANDLE_OK;
    }

    // Final epoch validation and the bounded caller-buffer publication share
    // one short IRQ boundary. Restart never advances the cursor or publishes
    // a partially mixed page.
    const publish_flags = interrupts.saveAndDisable();
    if (task.inventoryEpoch() != cursor.task_epoch or program_thread_mutation_epoch != cursor.thread_epoch) {
        out_page.status = r4x_api.program_inventory_status_restart;
        interrupts.restore(publish_flags);
        return PROGRAM_HANDLE_OK;
    }
    index = 0;
    while (index < returned) : (index += 1) out[index] = staged[index];
    const after_generation = if (returned == 0) cursor.task_after_generation else staged[returned - 1].generation;
    cursor.task_after_generation = after_generation;
    out_page.* = .{
        .version = r4x_api.program_inventory_version,
        .size = @sizeOf(ProgramInventoryPageInfo),
        .snapshot_generation = cursor.snapshot_generation,
        .next_generation = after_generation,
        .total = raw_page.total,
        .returned = raw_page.returned,
        .has_more = if (raw_page.has_more) 1 else 0,
        .kind = r4x_api.program_inventory_kind_task,
        .status = if (raw_page.has_more) r4x_api.program_inventory_status_more else r4x_api.program_inventory_status_complete,
    };
    interrupts.restore(publish_flags);
    return PROGRAM_HANDLE_OK;
}

fn threadInventorySnapshotSiftUp(items: []ProgramThreadSnapshot, start_index: usize) void {
    var index = start_index;
    while (index != 0) {
        const parent = (index - 1) / 2;
        if (items[parent].handle.thread_generation >= items[index].handle.thread_generation) break;
        const tmp = items[parent];
        items[parent] = items[index];
        items[index] = tmp;
        index = parent;
    }
}

fn threadInventorySnapshotSiftDown(items: []ProgramThreadSnapshot, start_index: usize) void {
    var index = start_index;
    while (true) {
        const left = index * 2 + 1;
        if (left >= items.len) return;
        const right = left + 1;
        var largest = left;
        if (right < items.len and items[right].handle.thread_generation > items[left].handle.thread_generation) largest = right;
        if (items[index].handle.thread_generation >= items[largest].handle.thread_generation) return;
        const tmp = items[index];
        items[index] = items[largest];
        items[largest] = tmp;
        index = largest;
    }
}

fn sortThreadInventorySnapshots(items: []ProgramThreadSnapshot) void {
    var index: usize = 1;
    while (index < items.len) : (index += 1) {
        const value = items[index];
        var insert = index;
        while (insert != 0 and items[insert - 1].handle.thread_generation > value.handle.thread_generation) : (insert -= 1) {
            items[insert] = items[insert - 1];
        }
        items[insert] = value;
    }
}

fn apiProgramInventoryThreads(cursor: *ProgramInventoryCursor, out: [*]ProgramThreadSnapshot, capacity: u32, out_page: *ProgramInventoryPageInfo) callconv(.c) i32 {
    if (@intFromPtr(cursor) == 0 or @intFromPtr(out) == 0 or @intFromPtr(out_page) == 0) return PROGRAM_HANDLE_ERROR_INVALID;
    initInventoryPage(cursor, out_page, r4x_api.program_inventory_kind_thread);
    if (!inventoryCursorValid(cursor) or capacity == 0 or capacity > r4x_api.program_inventory_page_max) return PROGRAM_HANDLE_ERROR_INVALID;

    var staged: [PROGRAM_INVENTORY_PAGE_MAX]ProgramThreadSnapshot = undefined;
    const requested: usize = @intCast(capacity);
    var returned: usize = 0;
    var eligible: u32 = 0;
    var total: u32 = 0;
    const capture_flags = interrupts.saveAndDisable();
    if (program_thread_mutation_epoch != cursor.thread_epoch) {
        out_page.status = r4x_api.program_inventory_status_restart;
        interrupts.restore(capture_flags);
        return PROGRAM_HANDLE_OK;
    }
    var scan = program_thread_head;
    while (scan) |candidate| : (scan = candidate.registry_next) {
        if (!candidate.used) continue;
        total +|= 1;
        if (candidate.generation <= cursor.thread_after_generation) continue;
        eligible +|= 1;
        if (returned == requested and candidate.generation >= staged[0].handle.thread_generation) continue;
        const snapshot = ProgramThreadSnapshot{
            .version = r4x_api.program_inventory_version,
            .size = @sizeOf(ProgramThreadSnapshot),
            .handle = .{
                .thread_id = candidate.id,
                .instance_id = candidate.instance_id,
                .thread_generation = candidate.generation,
                .instance_generation = candidate.instance_generation,
                .reserved = 0,
            },
            .info = threadInfo(candidate),
            .state_generation = candidate.generation,
        };
        if (returned < requested) {
            staged[returned] = snapshot;
            threadInventorySnapshotSiftUp(staged[0 .. returned + 1], returned);
            returned += 1;
        } else {
            staged[0] = snapshot;
            threadInventorySnapshotSiftDown(staged[0..returned], 0);
        }
    }
    interrupts.restore(capture_flags);
    sortThreadInventorySnapshots(staged[0..returned]);

    const publish_flags = interrupts.saveAndDisable();
    if (program_thread_mutation_epoch != cursor.thread_epoch) {
        out_page.status = r4x_api.program_inventory_status_restart;
        interrupts.restore(publish_flags);
        return PROGRAM_HANDLE_OK;
    }
    var index: usize = 0;
    while (index < returned) : (index += 1) out[index] = staged[index];
    const after_generation = if (returned == 0)
        cursor.thread_after_generation
    else
        staged[returned - 1].handle.thread_generation;
    const has_more = eligible > @as(u32, @intCast(returned));
    cursor.thread_after_generation = after_generation;
    out_page.* = .{
        .version = r4x_api.program_inventory_version,
        .size = @sizeOf(ProgramInventoryPageInfo),
        .snapshot_generation = cursor.snapshot_generation,
        .next_generation = after_generation,
        .total = total,
        .returned = @intCast(returned),
        .has_more = if (has_more) 1 else 0,
        .kind = r4x_api.program_inventory_kind_thread,
        .status = if (has_more) r4x_api.program_inventory_status_more else r4x_api.program_inventory_status_complete,
    };
    interrupts.restore(publish_flags);
    return PROGRAM_HANDLE_OK;
}

fn apiProgramStatus(out: *ProgramStatus) callconv(.c) void {
    out.* = .{
        .foreground_running = if (foreground_instance_id != null) 1 else 0,
        .shell_running = if (shell_instance_id != null) 1 else 0,
        .display_used = if (last_display_used) 1 else 0,
        .instance_count = activeInstanceCount(),
        .last_exit_code = last_exit_code,
    };
}

fn apiProgramInstance(index: u32, out: *ProgramInstanceInfo) callconv(.c) i32 {
    out.* = .{};
    var lease: ?ProgramInstanceLease = null;
    var completion_info: ?ProgramInstanceInfo = null;
    const locked = lockProgramRegistry();
    if (!locked) return 0;

    // Merge live slots and legacy completions in immutable global-generation
    // order. Moving A from live storage to its completion therefore cannot
    // reorder A/B between the Desktop's index-wise calls; live always wins and
    // a completion contributes only after its slot stops being visible.
    var after_generation: u64 = 0;
    var ordinal: u32 = 0;
    while (ordinal <= index) : (ordinal += 1) {
        var best_generation: u64 = std.math.maxInt(u64);
        var candidate_found = false;
        var best_slot: ?*ProgramRegistrySlot = null;
        var best_completion: ?*ProgramCompletionNode = null;

        var chunk = program_registry_head;
        while (chunk) |current_chunk| : (chunk = current_chunk.next) {
            for (&current_chunk.slots) |*slot| {
                if (!programRegistryStateIsVisible(slot.state) or slot.generation <= after_generation) continue;
                if (!candidate_found or slot.generation < best_generation) {
                    best_generation = slot.generation;
                    candidate_found = true;
                    best_slot = slot;
                    best_completion = null;
                }
            }
        }

        var node = program_completion_head;
        while (node) |current| : (node = current.next) {
            if (!current.legacy_id or current.state == .consumed or current.handle.generation <= after_generation) continue;
            if (lookupProgramRegistryHandleLocked(current.handle, true)) |slot| {
                if (programRegistryStateIsVisible(slot.state)) continue;
            }
            if (!candidate_found or current.handle.generation < best_generation) {
                best_generation = current.handle.generation;
                candidate_found = true;
                best_slot = null;
                best_completion = current;
            }
        }

        if (!candidate_found) {
            unlockProgramRegistry();
            return 0;
        }
        if (ordinal == index) {
            if (best_slot) |slot| {
                if (slot.pin_count == std.math.maxInt(u32)) {
                    unlockProgramRegistry();
                    return 0;
                }
                slot.pin_count += 1;
                lease = .{
                    .slot = slot,
                    .instance = &slot.instance,
                    .id = slot.public_id,
                    .generation = slot.generation,
                };
            } else if (best_completion) |selected| {
                completion_info = completionInstanceInfo(selected);
            }
            break;
        }
        after_generation = best_generation;
    }
    unlockProgramRegistry();

    if (lease) |selected| {
        out.* = instanceInfo(selected.instance);
        unpinProgramInstance(&selected);
        return 1;
    }
    if (completion_info) |info| {
        out.* = info;
        return 1;
    }
    return 0;
}

fn apiProgramSetWindow(id: u32, window_id: i32) callconv(.c) i32 {
    reapFinishedInstances();
    const lease = pinProgramInstance(id) orelse return -1;
    defer unpinProgramInstance(&lease);
    const instance = lease.instance;
    if (instance.done) return -2;
    const gui = ensureGuiPayload(instance) orelse return -3;
    gui.window_id = window_id;
    gui.window_info.window_id = window_id;
    return 0;
}

fn apiProgramSetWindowHandle(handle_ptr: *const ProgramProcessHandle, window_id: i32) callconv(.c) i32 {
    if (@intFromPtr(handle_ptr) == 0 or !programHandleValid(handle_ptr.*)) return PROGRAM_HANDLE_ERROR_INVALID;
    reapFinishedInstances();
    const handle = handle_ptr.*;
    const lease = pinProgramHandle(handle, false) orelse {
        const locked = lockProgramRegistry();
        if (!locked) return PROGRAM_HANDLE_ERROR_WOULD_BLOCK;
        defer unlockProgramRegistry();
        if (completionForHandleLocked(handle) != null) return PROGRAM_HANDLE_ERROR_NOT_RUNNING;
        if (lookupProgramRegistryHandleLocked(handle, true)) |slot| {
            return if (programRegistryStateIsRunning(slot.state)) PROGRAM_HANDLE_ERROR_WOULD_BLOCK else PROGRAM_HANDLE_ERROR_NOT_RUNNING;
        }
        return programHandleMissingStatusLocked(handle);
    };
    defer unpinProgramInstance(&lease);
    const gui = ensureGuiPayload(lease.instance) orelse return PROGRAM_HANDLE_ERROR_TASK_FAILED;
    gui.window_id = window_id;
    gui.window_info.window_id = window_id;
    gui.start_attach_pending = false;
    return PROGRAM_HANDLE_OK;
}

fn apiProgramSetConsoleHost(id: u32, host_raw: u32) callconv(.c) i32 {
    reapFinishedInstances();
    const instance = instanceById(id) orelse return -1;
    if (instance.done) return -2;
    if (instance.app_class != .console) return -3;
    const host = parseConsoleHost(host_raw) orelse return -4;
    const console = consolePayload(instance);
    console.host = host;
    console.revision +%= 1;
    if (console.revision == 0) console.revision = 1;
    return 0;
}

fn apiProgramCurrentConsoleHost() callconv(.c) u32 {
    return @intFromEnum(currentConsoleHostKind());
}

fn apiProgramRequestDesktop() callconv(.c) i32 {
    return if (requestDesktopFromHostedConsole()) 0 else -1;
}

fn apiEnvGet(name_ptr: [*:0]const u8, out_ptr: [*]u8, capacity: u32) callconv(.c) i32 {
    const instance = currentInstance() orelse return ENV_ERROR_UNSUPPORTED;
    var name_buf: [ENVIRONMENT_NAME_MAX + 1]u8 = undefined;
    const name = copyZ(name_ptr, name_buf[0..]) orelse return ENV_ERROR_INVALID;
    if (!environmentNameValid(name)) return ENV_ERROR_INVALID;
    return environmentGet(instance, name, out_ptr, capacity);
}

fn apiEnvSet(name_ptr: [*:0]const u8, value_ptr: [*]const u8, value_len: u32) callconv(.c) i32 {
    const instance = currentInstance() orelse return ENV_ERROR_UNSUPPORTED;
    var name_buf: [ENVIRONMENT_NAME_MAX + 1]u8 = undefined;
    const name = copyZ(name_ptr, name_buf[0..]) orelse return ENV_ERROR_INVALID;
    if (!environmentNameValid(name)) return ENV_ERROR_INVALID;
    const value_count: usize = @intCast(value_len);
    if (value_count > ENVIRONMENT_VALUE_MAX) return ENV_ERROR_TOO_LONG;
    return environmentSet(instance, name, value_ptr[0..value_count]);
}

fn apiProgramRequestHostLaunch(path_ptr: [*:0]const u8, args_ptr: [*:0]const u8, policy_raw: u32) callconv(.c) i32 {
    const instance = currentInstance() orelse return -1;
    if (instance.role != .background or instance.app_class != .gui) return -3;
    const gui = guiPayload(instance);
    if (gui.window_id < 0) return -3;
    const policy = parseLaunchPolicy(policy_raw) orelse return -4;
    var path_buf: [MAX_API_PATH]u8 = undefined;
    const raw_path = copyZ(path_ptr, path_buf[0..]) orelse return -2;
    if (raw_path.len == 0) return -2;
    var args_buf: [MAX_API_ARGS]u8 = undefined;
    const raw_args = copyZ(args_ptr, args_buf[0..]) orelse return -2;

    // ProgramHostLaunchRequest is a binary-frozen v1 payload: its path/args
    // fields keep their original widths.  Longer inputs (possible since the
    // 0.60.19 path limits) are rejected visibly instead of truncated.
    gui.host_launch_request = .{};
    gui.host_launch_request.policy = @intFromEnum(policy);
    gui.host_launch_request.reserved = 0;
    if (raw_path.len >= gui.host_launch_request.path.len) return -2;
    if (raw_args.len >= gui.host_launch_request.args.len) return -2;
    copySliceZ(gui.host_launch_request.path[0..], raw_path);
    copySliceZ(gui.host_launch_request.args[0..], raw_args);
    gui.host_launch_pending = true;
    return 0;
}

fn apiProgramTakeHostLaunch(id: u32, out: *ProgramHostLaunchRequest) callconv(.c) i32 {
    reapFinishedInstances();
    const instance = instanceById(id) orelse {
        out.* = .{};
        return -1;
    };
    if (instance.done) {
        out.* = .{};
        return -2;
    }
    const gui = instance.gui_payload orelse {
        out.* = .{};
        return 0;
    };
    if (!gui.host_launch_pending) {
        out.* = .{};
        return 0;
    }
    out.* = gui.host_launch_request;
    gui.host_launch_request = .{};
    gui.host_launch_pending = false;
    return 1;
}

fn apiProgramWindowId() callconv(.c) i32 {
    const instance = currentInstance() orelse return -1;
    const gui = instance.gui_payload orelse return -1;
    return gui.window_id;
}

fn apiGuiWindowInfo(out: *GuiWindowInfo) callconv(.c) i32 {
    const instance = currentInstance() orelse {
        out.* = .{};
        return -1;
    };
    const gui = instance.gui_payload orelse {
        out.* = .{};
        return 0;
    };
    out.* = gui.window_info;
    return if (gui.window_id >= 0) 1 else 0;
}

fn apiGuiSetWindowInfo(id: u32, info: *const GuiWindowInfo) callconv(.c) i32 {
    reapFinishedInstances();
    const lease = pinProgramInstance(id) orelse return -1;
    defer unpinProgramInstance(&lease);
    const instance = lease.instance;
    if (instance.done) return -2;
    const gui = ensureGuiPayload(instance) orelse return -3;
    gui.window_info = info.*;
    gui.window_id = info.window_id;
    return 0;
}

fn apiGuiPollEvent(out: *GuiEvent) callconv(.c) i32 {
    const instance = currentInstance() orelse {
        out.* = .{};
        return -1;
    };
    const gui = instance.gui_payload orelse {
        out.* = .{};
        return 0;
    };
    if (gui.event_head == gui.event_tail) {
        out.* = .{};
        return 0;
    }
    out.* = gui.events[gui.event_head];
    gui.event_head = (gui.event_head + 1) % GUI_EVENT_QUEUE_SIZE;
    return 1;
}

fn apiGuiPushEvent(id: u32, event: *const GuiEvent) callconv(.c) i32 {
    reapFinishedInstances();
    const lease = pinProgramInstance(id) orelse return -1;
    defer unpinProgramInstance(&lease);
    const instance = lease.instance;
    if (instance.done) return -2;
    const gui = ensureGuiPayload(instance) orelse return -3;
    return enqueueGuiEvent(gui, event);
}

fn enqueueGuiEvent(gui: *ProgramGuiPayload, event: *const GuiEvent) i32 {
    gui_event_push_attempts +%= 1;

    // Pointer events carry their own coordinates. Removing an older move and
    // appending the newest one therefore preserves all ordering-sensitive
    // events between them while bounding move traffic to one queued sample.
    if (event.kind == GUI_EVENT_KIND_MOUSE_MOVE) {
        if (findQueuedGuiMouseMove(gui)) |index| {
            removeQueuedGuiEvent(gui, index);
            gui_mouse_move_coalesced +%= 1;
        }
    }

    const next_tail = (gui.event_tail + 1) % GUI_EVENT_QUEUE_SIZE;
    if (next_tail == gui.event_head) {
        if (findQueuedGuiMouseMove(gui)) |index| {
            removeQueuedGuiEvent(gui, index);
            gui_mouse_move_evicted +%= 1;
        } else {
            gui_event_rejected +%= 1;
            return -3;
        }
    }
    gui.events[gui.event_tail] = event.*;
    gui.event_tail = (gui.event_tail + 1) % GUI_EVENT_QUEUE_SIZE;
    gui_event_accepted +%= 1;
    const pending = guiEventPendingCount(gui);
    if (pending > gui.event_high_water) gui.event_high_water = pending;
    return 0;
}

fn findQueuedGuiMouseMove(gui: *const ProgramGuiPayload) ?usize {
    var index = gui.event_head;
    while (index != gui.event_tail) : (index = (index + 1) % GUI_EVENT_QUEUE_SIZE) {
        if (gui.events[index].kind == GUI_EVENT_KIND_MOUSE_MOVE) return index;
    }
    return null;
}

fn removeQueuedGuiEvent(gui: *ProgramGuiPayload, remove_index: usize) void {
    var index = remove_index;
    while (true) {
        const next = (index + 1) % GUI_EVENT_QUEUE_SIZE;
        if (next == gui.event_tail) break;
        gui.events[index] = gui.events[next];
        index = next;
    }
    gui.event_tail = if (gui.event_tail == 0) GUI_EVENT_QUEUE_SIZE - 1 else gui.event_tail - 1;
}

fn guiEventPendingCount(gui: *const ProgramGuiPayload) u32 {
    return @intCast(if (gui.event_tail >= gui.event_head)
        gui.event_tail - gui.event_head
    else
        GUI_EVENT_QUEUE_SIZE - gui.event_head + gui.event_tail);
}

/// Font ids are a live catalogue view.  A reload therefore sends every
/// running GUI application a lightweight event so it can rebuild its font
/// menu and redraw with a fresh id.  The registry lock makes the short scan
/// safe without holding it across the filesystem read that initiated reload.
fn broadcastGuiFontCatalogChanged() void {
    const locked = lockProgramRegistry();
    if (!locked) return;
    defer unlockProgramRegistry();

    const event = GuiEvent{ .kind = GUI_EVENT_KIND_FONT_CHANGED };
    var iterator = programRegistryIterator(false);
    while (iterator.next()) |instance| {
        const gui = instance.gui_payload orelse continue;
        if (gui.window_id < 0) continue;
        _ = enqueueGuiEvent(gui, &event);
        bumpGuiRevision(instance);
    }
}

fn guiSetTextForInstance(instance: *ProgramInstance, text: [*:0]const u8) i32 {
    const gui = ensureGuiPayload(instance) orelse return -2;
    var next_text = [_]u8{0} ** GUI_TEXT_SIZE;
    var len: usize = 0;
    while (len + 1 < next_text.len and text[len] != 0) : (len += 1) {
        next_text[len] = text[len];
    }
    _ = guiFrameReplaceBuild(instance, false) orelse return -2;
    if (guiFrameCommit(instance) != 0) return -2;
    gui.text = next_text;
    return @intCast(len);
}

fn apiGuiSetText(text: [*:0]const u8) callconv(.c) i32 {
    const instance = currentInstance() orelse return -1;
    return guiSetTextForInstance(instance, text);
}

fn apiGuiText(id: u32, out: [*]u8, capacity: u32) callconv(.c) i32 {
    reapFinishedInstances();
    const instance = instanceById(id) orelse return -1;
    if (capacity == 0) return 0;
    const gui = instance.gui_payload orelse {
        out[0] = 0;
        return 0;
    };
    var len: usize = 0;
    const max_len: usize = @intCast(capacity - 1);
    while (len < max_len and len < gui.text.len and gui.text[len] != 0) : (len += 1) {
        out[len] = gui.text[len];
    }
    out[len] = 0;
    return @intCast(len);
}

fn apiGuiRevision(id: u32) callconv(.c) u32 {
    reapFinishedInstances();
    const instance = instanceById(id) orelse return 0;
    const gui = instance.gui_payload orelse return 0;
    return gui.revision;
}

fn apiProgramClass(path_ptr: [*:0]const u8, policy_raw: u32) callconv(.c) i32 {
    var path_buf: [MAX_API_PATH]u8 = undefined;
    const raw_path = copyZ(path_ptr, path_buf[0..]) orelse return -2;
    var resolved_buf: [MAX_API_PATH]u8 = undefined;
    const target = resolveApiTarget(raw_path, &resolved_buf) orelse return -2;
    const file = resolveProgramFile(target.drive_ref, target.path) orelse return -1;
    const policy = parseLaunchPolicy(policy_raw) orelse return -2;
    const app_class = classifyProgramFile(file, policy) orelse return -2;
    return switch (app_class) {
        .console => 1,
        .gui => 2,
        .service => 3,
    };
}

fn apiServiceInfo(index: u32, out: *ServiceInfo) callconv(.c) i32 {
    if (@intFromPtr(out) == 0) return services.API_ERR_INVALID;
    var attempt: u8 = 0;
    while (attempt < 3) : (attempt += 1) {
        var target = (if (attempt == 0)
            services.beginApiIndexRefresh(index)
        else
            services.retryApiIndexRefresh(index)) orelse {
            reportBootServicePlanComplete();
            out.* = .{};
            return 0;
        };
        if (!serviceApiRefreshTarget(&target)) continue;
        const result = services.apiInfoForIndexTarget(target, out, r4api.r4sys.ticks());
        if (result != services.API_ERR_BUSY) return result;
    }
    out.* = .{};
    return services.API_ERR_BUSY;
}

fn apiServiceStatus(name_ptr: [*:0]const u8, out: *ServiceInfo) callconv(.c) i32 {
    if (@intFromPtr(out) == 0) return services.API_ERR_INVALID;
    var name_buf: [services.MAX_NAME]u8 = undefined;
    const name = copyZ(name_ptr, name_buf[0..]) orelse return services.API_ERR_INVALID;
    serviceApiRefreshName(name);
    return services.apiStatus(name, out, r4api.r4sys.ticks());
}

fn apiServiceOpen(name_ptr: [*:0]const u8, out: *ServiceInfo) callconv(.c) i32 {
    if (@intFromPtr(out) == 0) return services.API_ERR_INVALID;
    var name_buf: [services.MAX_NAME]u8 = undefined;
    const name = copyZ(name_ptr, name_buf[0..]) orelse return services.API_ERR_INVALID;
    serviceApiRefreshName(name);
    return services.apiOpen(name, out, r4api.r4sys.ticks());
}

fn apiServiceClose(handle: u32) callconv(.c) i32 {
    return services.apiClose(handle);
}

fn apiServiceCall(handle: u32, op: u16, request_ptr: [*]const u8, request_len: u32, response_header: *ServiceMessageHeader, response_ptr: [*]u8, response_capacity: u32, timeout_ticks: u64) callconv(.c) i32 {
    var request_id: u32 = 0;
    const submit = apiIoServiceCall(handle, op, request_ptr, request_len, response_header, response_ptr, response_capacity, timeout_ticks, 0, &request_id);
    if (submit != IO_OK) return submit;
    return waitAndCloseIoRequest(request_id);
}

fn serviceCallCore(async_req: *AsyncIoRequest, handle: u32, op: u16, request_ptr: [*]const u8, request_len: u32, response_header: *ServiceMessageHeader, response_ptr: [*]u8, response_capacity: u32, timeout_ticks: u64) i32 {
    if (@intFromPtr(response_header) == 0) return services.API_ERR_INVALID;
    response_header.* = .{ .magic = 0, .version = 0 };
    if (request_len > services.API_MAX_PAYLOAD) return services.API_ERR_PAYLOAD_TOO_LARGE;
    if (response_capacity > services.API_MAX_PAYLOAD) return services.API_ERR_INVALID;
    if (request_len != 0 and @intFromPtr(request_ptr) == 0) return services.API_ERR_INVALID;
    if (@intFromPtr(response_ptr) == 0) return services.API_ERR_INVALID;
    const instance = currentInstance() orelse return services.API_ERR_INVALID;
    const request = if (request_len == 0) "" else request_ptr[0..@as(usize, @intCast(request_len))];
    const forever = timeout_ticks == sync.WAIT_FOREVER;
    const start_ticks = r4api.r4sys.ticks();
    const deadline = if (forever) @as(u64, 0) else timer.deadlineAfter(start_ticks, timeout_ticks);
    // Slot admission may block, so hard-kill remains enabled while queued.
    // The service core arms this token only after admission and immediately
    // before publishing the RequestSlot. It stays active across the short
    // cross-registry handoff into AsyncIoRequest.service_request_id.
    var publish_token: task_context.UnwindToken = .{};
    const request_id_raw = services.submitRequestWaitGuarded(handle, instance.id, op, request, timeout_ticks, &publish_token);
    if (request_id_raw <= 0) {
        return request_id_raw;
    }
    const request_id: u32 = @intCast(request_id_raw);
    // Publish the service-side request identity under the same mutex used by
    // owner cancellation. If cancellation won before submission completed,
    // remove the just-created endpoint request without exposing a stale ID.
    if (!publishAsyncServiceRequestId(async_req, request_id)) {
        _ = services.cancelRequest(handle, request_id);
        _ = task_context.leaveUnwind(publish_token);
        return IO_ERROR_CANCELLED;
    }
    if (!task_context.leaveUnwind(publish_token)) {
        _ = services.cancelRequest(handle, request_id);
        clearAsyncServiceRequestId(async_req, request_id);
        return IO_ERROR_BUSY;
    }
    defer clearAsyncServiceRequestId(async_req, request_id);
    const response = response_ptr[0..@as(usize, @intCast(response_capacity))];
    while (true) {
        const result = services.takeResponse(handle, request_id, response_header, response);
        if (result < 0) return result;
        if (result > 0 or response_header.magic == services.API_MAGIC) return result;
        const now_ticks = r4api.r4sys.ticks();
        if (!forever and now_ticks >= deadline) break;
        const remaining = if (forever) sync.WAIT_FOREVER else deadline - now_ticks;
        const wait_result = services.waitResponse(handle, request_id, remaining);
        if (wait_result == services.API_ERR_TIMEOUT) break;
        if (wait_result < 0) {
            _ = services.cancelRequest(handle, request_id);
            return wait_result;
        }
    }
    _ = services.cancelRequest(handle, request_id);
    return services.API_ERR_TIMEOUT;
}

fn publishAsyncServiceRequestId(req: *AsyncIoRequest, request_id: u32) bool {
    if (!lockAsyncIoRequests()) return false;
    defer unlockAsyncIoRequests();
    const current_task = scheduler.current() orelse return false;
    if (!req.used or
        req.task_id != current_task.id or
        req.task_generation != current_task.generation or
        req.cancel_requested) return false;
    req.service_request_id = request_id;
    return true;
}

fn clearAsyncServiceRequestId(req: *AsyncIoRequest, request_id: u32) void {
    if (!lockAsyncIoRequests()) return;
    defer unlockAsyncIoRequests();
    const current_task = scheduler.current() orelse return;
    if (!req.used or
        req.task_id != current_task.id or
        req.task_generation != current_task.generation or
        req.service_request_id != request_id) return;
    req.service_request_id = 0;
}

fn apiServiceEndpointRegister(name_ptr: [*:0]const u8, flags: u32, out: *ServiceInfo) callconv(.c) i32 {
    if (@intFromPtr(out) == 0) return services.API_ERR_INVALID;
    const instance = currentInstance() orelse return services.API_ERR_INVALID;
    if (instance.app_class != .service) return services.API_ERR_INVALID;
    var name_buf: [services.MAX_NAME]u8 = undefined;
    const name = copyZ(name_ptr, name_buf[0..]) orelse return services.API_ERR_INVALID;
    return services.registerEndpoint(name, instance.id, flags, out, r4api.r4sys.ticks());
}

fn apiServiceEndpointUnregister(handle: u32) callconv(.c) i32 {
    return services.unregisterEndpoint(handle);
}

fn apiServiceEndpointPoll(handle: u32) callconv(.c) i32 {
    return services.endpointPoll(handle);
}

// 0.56.19: Blockierendes Endpoint-Warten (API-Version 149).
fn apiServiceEndpointPollWait(handle: u32, timeout_ticks: u64) callconv(.c) i32 {
    return services.endpointWait(handle, timeout_ticks);
}

fn apiServiceEndpointRecv(handle: u32, header: *ServiceMessageHeader, out_ptr: [*]u8, capacity: u32) callconv(.c) i32 {
    if (@intFromPtr(header) == 0) return services.API_ERR_INVALID;
    if (capacity > services.API_MAX_PAYLOAD) return services.API_ERR_INVALID;
    if (@intFromPtr(out_ptr) == 0) return services.API_ERR_INVALID;
    return services.recvRequest(handle, header, out_ptr[0..@as(usize, @intCast(capacity))]);
}

fn apiServiceEndpointReply(handle: u32, request_id: u32, status: i32, payload_ptr: [*]const u8, payload_len: u32) callconv(.c) i32 {
    if (payload_len > services.API_MAX_PAYLOAD) return services.API_ERR_PAYLOAD_TOO_LARGE;
    if (payload_len != 0 and @intFromPtr(payload_ptr) == 0) return services.API_ERR_INVALID;
    const payload = if (payload_len == 0) "" else payload_ptr[0..@as(usize, @intCast(payload_len))];
    const result = services.reply(handle, request_id, status, payload);
    _ = scheduler.safeReschedulePoint();
    return result;
}

fn apiServiceDetail(index: u32, out: *ServiceDetail) callconv(.c) i32 {
    if (@intFromPtr(out) == 0) return services.API_ERR_INVALID;
    var attempt: u8 = 0;
    while (attempt < 3) : (attempt += 1) {
        var target = (if (attempt == 0)
            services.beginApiIndexRefresh(index)
        else
            services.retryApiIndexRefresh(index)) orelse {
            reportBootServicePlanComplete();
            out.* = .{};
            return 0;
        };
        if (!serviceApiRefreshTarget(&target)) continue;
        const result = services.apiDetailForIndexTarget(target, out, r4api.r4sys.ticks());
        if (result != services.API_ERR_BUSY) return result;
    }
    out.* = .{};
    return services.API_ERR_BUSY;
}

fn apiServiceDetailByName(name_ptr: [*:0]const u8, out: *ServiceDetail) callconv(.c) i32 {
    if (@intFromPtr(out) == 0) return services.API_ERR_INVALID;
    var name_buf: [services.MAX_NAME]u8 = undefined;
    const name = copyZ(name_ptr, name_buf[0..]) orelse return services.API_ERR_INVALID;
    serviceApiRefreshName(name);
    return services.apiDetailByName(name, out, r4api.r4sys.ticks());
}

fn apiServiceStart(name_ptr: [*:0]const u8, out: *ServiceInfo) callconv(.c) i32 {
    if (@intFromPtr(out) == 0) return services.API_ERR_INVALID;
    var name_buf: [services.MAX_NAME]u8 = undefined;
    const name = copyZ(name_ptr, name_buf[0..]) orelse return services.API_ERR_INVALID;
    return serviceApiStartByName(name, out);
}

fn reportBootService(name: []const u8) void {
    if (!isBootServiceManagerCaller()) return;
    bootscreen.setService(name);
}

fn reportBootServiceStage(name: []const u8, stage: []const u8) void {
    if (!isBootServiceManagerCaller()) return;
    bootscreen.setServiceStage(name, stage);
}

fn reportBootServicePlanComplete() void {
    if (!isBootServiceManagerCaller()) return;
    bootscreen.setDetail("Dienstplan fertig");
}

fn reportBootServiceError(name: []const u8) void {
    if (!isBootServiceManagerCaller()) return;
    bootscreen.setServiceError(name);
}

fn isBootServiceManagerCaller() bool {
    if (!bootscreen.isActive()) return false;
    const foreground_id = foreground_instance_id orelse return false;
    const instance = currentInstance() orelse return false;
    return instance.id == foreground_id;
}

fn apiServiceStop(name_ptr: [*:0]const u8, out: *ServiceInfo, timeout_ticks: u64) callconv(.c) i32 {
    if (@intFromPtr(out) == 0) return services.API_ERR_INVALID;
    var name_buf: [services.MAX_NAME]u8 = undefined;
    const name = copyZ(name_ptr, name_buf[0..]) orelse return services.API_ERR_INVALID;
    // Same rule as restart (0.60.29): stopping the service the caller lives
    // in is a self-kill.  Without this, the refusal below could simply be
    // bypassed with STOP followed by START.
    if (services.entryByName(name)) |entry| {
        if (callerDescendsFromServiceInstance(entry.instance_id))
            return services.API_ERR_SELF_RESTART;
    }
    return serviceApiStopByName(name, out, timeout_ticks);
}

/// True when the calling program IS the service's own program or one of its
/// console descendants (0.60.29).
///
/// `SERVMAN RESTART SSHD` issued over an SSH session is exactly this case:
/// SERVMAN is a console child of SSHD, so stopping SSHD tears down the very
/// caller tree that is still inside the syscall.  The old behaviour killed
/// the caller and left a "success" that nobody could observe.  Detecting the
/// relationship here means every caller inherits the rule, not just SERVMAN.
fn callerDescendsFromServiceInstance(service_instance_id: u32) bool {
    if (service_instance_id == 0) return false;
    const caller = currentProgramHandle() orelse return false;
    if (caller.instance_id == service_instance_id) return true;
    if (!lockProgramRegistry()) return false;
    defer unlockProgramRegistry();

    var current = caller;
    while (programHandleValid(current)) {
        if (current.instance_id == service_instance_id) return true;

        // Two different relationships can lead back to the service, and only
        // together do they cover the real cases:
        //
        //   * console ownership, used by shell-hosted children, and
        //   * the spawn owner recorded on the completion node, which is what
        //     a service-hosted launch carries.  SSHD spawns with an explicit
        //     console host, and that path deliberately leaves io_target_id at
        //     zero - so relying on the console chain alone silently missed
        //     exactly the `SERVMAN RESTART SSHD` case this rule exists for.
        var parent = ProgramProcessHandle{};
        if (lookupProgramRegistryHandleLocked(current, true)) |slot| {
            if (slot.instance.console_payload) |console| {
                parent = .{
                    .instance_id = console.io_target_id,
                    .reserved = 0,
                    .generation = console.io_target_generation,
                };
            }
        }
        if (!programHandleValid(parent)) {
            const node = completionForHandleLocked(current) orelse return false;
            if (!node.owner) return false;
            parent = node.owner_handle;
        }
        if (!programHandleValid(parent)) return false;
        // Ownership is only ever established from an already published
        // parent, so generations strictly decrease.  That is both the acyclic
        // invariant and the loop guard.
        if (parent.generation >= current.generation) return false;
        current = parent;
    }
    return false;
}

fn apiServiceRestart(name_ptr: [*:0]const u8, out: *ServiceInfo) callconv(.c) i32 {
    if (@intFromPtr(out) == 0) return services.API_ERR_INVALID;
    var name_buf: [services.MAX_NAME]u8 = undefined;
    const name = copyZ(name_ptr, name_buf[0..]) orelse return services.API_ERR_INVALID;
    serviceApiRefreshName(name);
    const entry = services.entryByName(name) orelse return services.API_ERR_NOT_FOUND;
    const service_name = entry.name[0..entry.name_len];
    if (callerDescendsFromServiceInstance(entry.instance_id))
        return services.API_ERR_SELF_RESTART;
    if (entry.state == .running or entry.state == .starting or entry.state == .stopping) {
        const stop_result = serviceApiStopByName(service_name, out, 40);
        if (stop_result < 0) return stop_result;
    }
    _ = services.bumpRestartCount(service_name);
    return serviceApiStartByName(service_name, out);
}

fn apiServiceSetStartMode(name_ptr: [*:0]const u8, start_mode_raw: u32, out: *ServiceInfo) callconv(.c) i32 {
    if (@intFromPtr(out) == 0) return services.API_ERR_INVALID;
    const start_mode = serviceStartModeFromCode(start_mode_raw) orelse return services.API_ERR_INVALID;
    var name_buf: [services.MAX_NAME]u8 = undefined;
    const name = copyZ(name_ptr, name_buf[0..]) orelse return services.API_ERR_INVALID;
    serviceApiRefreshName(name);
    const entry = services.entryByName(name) orelse return services.API_ERR_NOT_FOUND;
    const service_name = entry.name[0..entry.name_len];
    if (start_mode == .disabled and (entry.state == .running or entry.state == .starting or entry.state == .stopping)) {
        const stop_result = serviceApiStopByName(service_name, out, 40);
        if (stop_result < 0) return stop_result;
    }
    const result = services.setStartMode(service_name, start_mode);
    if (result != services.OK) return serviceApiRegistryResult(result);
    return services.apiStatus(service_name, out, r4api.r4sys.ticks());
}

fn apiServiceInstall(name_ptr: [*:0]const u8, path_ptr: [*:0]const u8, args_ptr: [*:0]const u8, start_mode_raw: u32, description_ptr: [*:0]const u8, out: *ServiceInfo) callconv(.c) i32 {
    if (@intFromPtr(out) == 0) return services.API_ERR_INVALID;
    const start_mode = serviceStartModeFromCode(start_mode_raw) orelse return services.API_ERR_INVALID;
    var name_buf: [services.MAX_NAME]u8 = undefined;
    var path_buf: [services.MAX_PATH]u8 = undefined;
    var args_buf: [services.MAX_ARGS]u8 = undefined;
    var description_buf: [services.MAX_DESCRIPTION]u8 = undefined;
    const name = copyZ(name_ptr, name_buf[0..]) orelse return services.API_ERR_INVALID;
    reportBootService(name);
    const raw_path = copyZ(path_ptr, path_buf[0..]) orelse {
        reportBootServiceError(name);
        return services.API_ERR_INVALID;
    };
    const args = copyZ(args_ptr, args_buf[0..]) orelse {
        reportBootServiceError(name);
        return services.API_ERR_INVALID;
    };
    const description = copyZ(description_ptr, description_buf[0..]) orelse {
        reportBootServiceError(name);
        return services.API_ERR_INVALID;
    };

    var resolved_buf: [MAX_API_PATH]u8 = undefined;
    const target = resolveServiceProgramTarget(raw_path, &resolved_buf) orelse {
        reportBootServiceError(name);
        return services.API_ERR_BAD_PATH;
    };
    var full_path_buf: [services.MAX_PATH]u8 = undefined;
    const full_path = buildServiceDosPath(target.drive_ref, target.path, full_path_buf[0..]) orelse {
        reportBootServiceError(name);
        return services.API_ERR_BAD_PATH;
    };
    const register_result = services.registerWithDescription(name, full_path, args, start_mode, description);
    if (register_result < 0) {
        reportBootServiceError(name);
        return serviceApiRegistryResult(register_result);
    }
    const status = services.apiStatus(name, out, r4api.r4sys.ticks());
    if (status != services.API_OK) reportBootServiceError(name);
    return status;
}

fn apiServiceRemove(name_ptr: [*:0]const u8) callconv(.c) i32 {
    var name_buf: [services.MAX_NAME]u8 = undefined;
    const name = copyZ(name_ptr, name_buf[0..]) orelse return services.API_ERR_INVALID;
    serviceApiRefreshName(name);
    const entry = services.entryByName(name) orelse return services.API_ERR_NOT_FOUND;
    if (entry.state == .running or entry.state == .starting or entry.state == .stopping) return services.API_ERR_RUNNING;
    const service_name = entry.name[0..entry.name_len];
    const result = services.unregister(service_name);
    if (result != services.OK) return serviceApiRegistryResult(result);
    return services.API_OK;
}

fn serviceApiStartByName(name: []const u8, out: *ServiceInfo) i32 {
    out.* = .{};
    reportBootService(name);
    serviceApiRefreshName(name);
    const entry = services.entryByName(name) orelse {
        reportBootServiceError(name);
        return services.API_ERR_NOT_FOUND;
    };
    const service_name = entry.name[0..entry.name_len];
    if (entry.start_mode == .disabled or entry.state == .disabled) {
        _ = services.apiStatus(service_name, out, r4api.r4sys.ticks());
        reportBootServiceError(service_name);
        return services.API_ERR_DISABLED;
    }
    if (entry.state == .running or entry.state == .starting or entry.state == .stopping) {
        const status = services.apiStatus(service_name, out, r4api.r4sys.ticks());
        if (status != services.API_OK) reportBootServiceError(service_name);
        return status;
    }

    _ = services.markStarting(service_name);
    reportBootServiceStage(service_name, "Pfad pruefen");
    var path_buf: [MAX_API_PATH]u8 = undefined;
    const target = resolveServiceProgramTarget(entry.path[0..entry.path_len], &path_buf) orelse {
        _ = services.markFailed(service_name, -1, "path invalid or not service R4X");
        _ = services.apiStatus(service_name, out, r4api.r4sys.ticks());
        reportBootServiceError(service_name);
        return services.API_ERR_BAD_PATH;
    };
    reportBootServiceStage(service_name, "laden");
    const raw_id = spawnPath(target.drive_ref, target.path, entry.args[0..entry.args_len], target.drive_ref);
    if (raw_id <= 0) {
        _ = services.markFailed(service_name, raw_id, "spawn failed");
        _ = services.apiStatus(service_name, out, r4api.r4sys.ticks());
        reportBootServiceError(service_name);
        return services.API_ERR_SPAWN_FAILED;
    }
    reportBootServiceStage(service_name, "gestartet");
    _ = services.markRunning(service_name, @intCast(raw_id), r4api.r4sys.ticks());
    const status = services.apiStatus(service_name, out, r4api.r4sys.ticks());
    if (status != services.API_OK) reportBootServiceError(service_name);
    return status;
}

fn serviceApiStopByName(name: []const u8, out: *ServiceInfo, timeout_ticks: u64) i32 {
    out.* = .{};
    serviceApiRefreshName(name);
    const entry = services.entryByName(name) orelse return services.API_ERR_NOT_FOUND;
    const service_name = entry.name[0..entry.name_len];
    if (entry.state == .disabled or entry.state == .stopped or entry.state == .failed or entry.instance_id == 0) {
        return services.apiStatus(service_name, out, r4api.r4sys.ticks());
    }

    const instance_id = entry.instance_id;
    _ = services.markStopping(service_name);
    const close_result = requestCloseInstance(instance_id);
    serviceApiRefreshName(service_name);
    const after_close = services.entryByName(service_name) orelse return services.API_ERR_NOT_FOUND;
    if (after_close.state == .stopped or after_close.state == .disabled) {
        return services.apiStatus(service_name, out, r4api.r4sys.ticks());
    }
    if (close_result != 0) {
        // Natural exit may win between markStopping and requestClose. A
        // pending/ready completion is already a terminal observation, so keep
        // refreshing until the reaper makes it consumable instead of turning
        // that benign race into STOP_FAILED.
        var exit_handle: ProgramProcessHandle = .{};
        var exit_info: ProgramInstanceInfo = .{};
        const terminal_observed = apiProgramOpenHandle(instance_id, &exit_handle) == PROGRAM_HANDLE_OK and
            apiProgramHandleStatus(&exit_handle, &exit_info) == PROGRAM_HANDLE_OK and
            exit_info.state != @intFromEnum(InstanceState.running);
        if (!terminal_observed) {
            _ = services.apiStatus(service_name, out, r4api.r4sys.ticks());
            return services.API_ERR_STOP_FAILED;
        }
    }

    const timeout = timeout_ticks;
    var waited: u64 = 0;
    while (waited < timeout) : (waited += 1) {
        serviceApiRefreshName(service_name);
        const current = services.entryByName(service_name) orelse return services.API_ERR_NOT_FOUND;
        if (current.state == .stopped or current.state == .disabled) {
            return services.apiStatus(service_name, out, r4api.r4sys.ticks());
        }
        if (current.state == .failed) {
            _ = services.apiStatus(service_name, out, r4api.r4sys.ticks());
            return services.API_ERR_STOP_FAILED;
        }
        waitOneTick("service-stop");
    }

    serviceApiRefreshName(service_name);
    _ = services.apiStatus(service_name, out, r4api.r4sys.ticks());
    return services.API_ERR_TIMEOUT;
}

fn serviceApiRefreshTarget(target: *services.ApiIndexTarget) bool {
    const entry = services.entryForApiIndexTarget(target.*) orelse return false;
    if (entry.instance_id == 0) return true;
    if (!services.noteApiIndexInstanceLookup(target.*)) return false;

    if (instanceSnapshot(entry.instance_id)) |snapshot| {
        if (snapshot.app_class != INSTANCE_CLASS_SERVICE) {
            if (services.markFailedTarget(target.*, -2, "instance class mismatch") != services.OK) return false;
            target.instance_id = 0;
            target.state = .failed;
        } else if (snapshot.state == @intFromEnum(InstanceState.done)) {
            const exit_code = snapshot.exit_code;
            _ = finishInstance(entry.instance_id);
            const current = services.entryForApiIndexTarget(target.*) orelse return false;
            if (current.state == .stopping or exit_code == 0) {
                if (services.markStoppedTarget(target.*, exit_code) != services.OK) return false;
                target.state = if (current.start_mode == .disabled) .disabled else .stopped;
            } else {
                if (services.markFailedTarget(target.*, exit_code, "service exited") != services.OK) return false;
                target.state = .failed;
            }
            target.instance_id = 0;
        } else if (snapshot.close_requested and entry.state != .stopping) {
            if (services.markStoppingTarget(target.*) != services.OK) return false;
            target.state = .stopping;
        }
        return services.entryForApiIndexTarget(target.*) != null;
    }

    var exit_code = takeExitCode(entry.instance_id) orelse (if (entry.state == .stopping) @as(i32, 0) else @as(i32, -1));
    var handle: ProgramProcessHandle = .{};
    if (apiProgramOpenHandle(entry.instance_id, &handle) == PROGRAM_HANDLE_OK) {
        var completion: ProgramProcessCompletion = .{};
        const reap_status = apiProgramHandleReap(&handle, &completion);
        if (reap_status == PROGRAM_HANDLE_ERROR_WOULD_BLOCK) {
            return services.entryForApiIndexTarget(target.*) != null;
        }
        if (reap_status == PROGRAM_HANDLE_OK) exit_code = completion.exit_code;
    }
    const current = services.entryForApiIndexTarget(target.*) orelse return false;
    if (current.state == .stopping or exit_code == 0) {
        if (services.markStoppedTarget(target.*, exit_code) != services.OK) return false;
        target.state = if (current.start_mode == .disabled) .disabled else .stopped;
    } else {
        if (services.markFailedTarget(target.*, exit_code, "service exited") != services.OK) return false;
        target.state = .failed;
    }
    target.instance_id = 0;
    return true;
}

fn waitOneTick(reason: []const u8) void {
    var wait = sync.TimerWait.init();
    _ = wait.waitReason(1, reason);
}

fn serviceApiRefreshName(name: []const u8) void {
    const entry = services.entryByName(name) orelse return;
    if (entry.instance_id == 0) return;
    const service_name = entry.name[0..entry.name_len];
    if (instanceSnapshot(entry.instance_id)) |snapshot| {
        if (snapshot.app_class != INSTANCE_CLASS_SERVICE) {
            _ = services.markFailed(service_name, -2, "instance class mismatch");
        } else if (snapshot.state == @intFromEnum(InstanceState.done)) {
            const exit_code = snapshot.exit_code;
            _ = finishInstance(entry.instance_id);
            if (entry.state == .stopping or exit_code == 0) {
                _ = services.markStopped(service_name, exit_code);
            } else {
                _ = services.markFailed(service_name, exit_code, "service exited");
            }
        } else if (snapshot.close_requested and entry.state != .stopping) {
            _ = services.markStopping(service_name);
        }
        return;
    }
    var exit_code = takeExitCode(entry.instance_id) orelse (if (entry.state == .stopping) @as(i32, 0) else @as(i32, -1));
    var handle: ProgramProcessHandle = .{};
    if (apiProgramOpenHandle(entry.instance_id, &handle) == PROGRAM_HANDLE_OK) {
        var completion: ProgramProcessCompletion = .{};
        const reap_status = apiProgramHandleReap(&handle, &completion);
        if (reap_status == PROGRAM_HANDLE_ERROR_WOULD_BLOCK) return;
        if (reap_status == PROGRAM_HANDLE_OK) exit_code = completion.exit_code;
    }
    if (entry.state == .stopping or exit_code == 0) {
        _ = services.markStopped(service_name, exit_code);
    } else {
        _ = services.markFailed(service_name, exit_code, "service exited");
    }
}

fn resolveServiceProgramTarget(path: []const u8, out: *[MAX_API_PATH]u8) ?ApiTarget {
    const target = resolveApiTarget(path, out) orelse return null;
    // FS-neutral: any VFS-mounted volume may carry services.  The old
    // `kind == .fat32` guard silently rejected every service install on
    // the NTFS system volume (real Lenovo finding, 0.60.11).
    if (vfs.volumeForDrive(target.drive_ref.letter) == null) return null;
    if (programClassIdForPath(target.drive_ref, target.path) != 3) return null;
    return target;
}

fn buildServiceDosPath(d: *drive.Drive, path: []const u8, out: []u8) ?[]const u8 {
    if (out.len < 3) return null;
    var len: usize = 0;
    out[len] = upper(d.letter);
    len += 1;
    out[len] = ':';
    len += 1;
    if (path.len == 0 or (path[0] != '\\' and path[0] != '/')) {
        if (len >= out.len) return null;
        out[len] = '\\';
        len += 1;
    }
    if (len + path.len > out.len) return null;
    var i: usize = 0;
    while (i < path.len) : (i += 1) {
        out[len + i] = if (path[i] == '/') '\\' else path[i];
    }
    len += path.len;
    return out[0..len];
}

fn serviceStartModeFromCode(raw: u32) ?services.StartMode {
    return switch (raw) {
        services.API_START_MANUAL => .manual,
        services.API_START_AUTO => .auto,
        services.API_START_DISABLED => .disabled,
        else => null,
    };
}

fn serviceApiRegistryResult(result: i32) i32 {
    return switch (result) {
        services.OK => services.API_OK,
        services.ERR_INVALID => services.API_ERR_INVALID,
        services.ERR_FULL => services.API_ERR_FULL,
        services.ERR_DUPLICATE => services.API_ERR_DUPLICATE,
        services.ERR_NOT_FOUND => services.API_ERR_NOT_FOUND,
        else => services.API_ERR_INVALID,
    };
}

pub fn dumpStatus() void {
    reapFinishedInstances();
    k.puts("R4X runtime\r\n");
    k.puts("  foreground=");
    k.puts(if (foreground_instance_id != null) "yes" else "no");
    k.puts(" shell=");
    k.puts(if (shell_instance_id != null) "yes" else "no");
    k.puts(" instances=");
    k.putDec(activeInstanceCount());
    k.puts(" last_exit=");
    putSignedDec(last_exit_code);
    k.puts("\r\n");

    var shown: usize = 0;
    var iterator = programRegistryIterator(false);
    while (iterator.next()) |instance| {
        shown += 1;
        k.puts("  #");
        k.putDec(instance.id);
        k.puts(" task=");
        k.putDec(instance.task_id);
        k.puts(" role=");
        k.puts(roleName(instance.role));
        k.puts(" class=");
        k.puts(appClassName(instance.app_class));
        k.puts(" profile=");
        k.puts(memoryProfileName(instance.memory_profile));
        k.puts(" state=");
        k.puts(instanceStateName(instanceState(instance)));
        k.puts(" window=");
        putSignedDec(if (instance.gui_payload) |gui| gui.window_id else -1);
        k.puts(" console_host=");
        k.puts(consoleHostName(if (instance.console_payload) |console| console.host else .none));
        k.puts(" exit=");
        putSignedDec(instance.exit_code);
        k.puts("\r\n");
    }
    if (shown == 0) k.puts("  no active R4X instances\r\n");
    const storage = instanceStorageStats();
    k.puts("  storage core=");
    k.putDec(storage.core_bytes_per_instance);
    k.puts(" live_core=");
    k.putDec(storage.live_core_bytes);
    k.puts(" registry_core=");
    k.putDec(storage.registry_reserved_core_bytes);
    k.puts(" payload=");
    k.putDec(storage.current_payload_bytes);
    k.puts(" peak=");
    k.putDec(storage.peak_payload_bytes);
    k.puts(" runtime=");
    k.putDec(storage.current_runtime_bytes);
    k.puts(" console=");
    k.putDec(storage.current_console_bytes);
    k.puts(" gui=");
    k.putDec(storage.current_gui_bytes);
    k.puts(" failures=");
    k.putDec(storage.allocation_failures);
    k.puts(" rollbacks=");
    k.putDec(storage.transaction_rollbacks);
    k.puts(" owner_errors=");
    k.putDec(storage.owner_mismatches + storage.header_errors + storage.free_failures);
    k.puts("\r\n");
}

fn apiGuiClear(rgb: u32) callconv(.c) i32 {
    const instance = currentInstance() orelse return -1;
    const gui = ensureGuiPayload(instance) orelse return -2;
    // An explicit frame transaction is terminal after its first build error;
    // only gui_frame_cancel may discard that failed transaction.  Legacy
    // implicit frames still use CLEAR as their compatible fresh-frame reset.
    if (gui.building_frame) |building| {
        if (building.explicit_build and building.build_failed) return -4;
    }
    const explicit_build = if (gui.building_frame) |frame| frame.explicit_build else false;
    const frame = guiFrameReplaceBuild(instance, explicit_build) orelse return -4;
    var storage = prepareGuiCommandStorage(instance, frame) orelse {
        guiFrameMarkBuildFailed(gui, frame);
        return -4;
    };
    return commitGuiCommandStorage(&storage, .{
        .kind = 1,
        .rgb = rgb,
    });
}

fn apiGuiRect(x: i32, y: i32, w: u32, h: u32, rgb: u32) callconv(.c) i32 {
    const instance = currentInstance() orelse return -1;
    return appendGuiCommand(instance, .{
        .kind = 2,
        .x = x,
        .y = y,
        .w = w,
        .h = h,
        .rgb = rgb,
    });
}

fn apiGuiBlit(x: i32, y: i32, w: u32, h: u32, scale: u32, pixels: [*]const u32, pixel_count: u32) callconv(.c) i32 {
    const instance = currentInstance() orelse return -1;
    if (@intFromPtr(pixels) == 0) return -1;
    if (w == 0 or h == 0) return -2;
    if (w > GUI_RASTER_MAX_WIDTH or h > GUI_RASTER_MAX_HEIGHT) return -2;
    const total_u64 = @as(u64, w) * @as(u64, h);
    if (total_u64 > @as(u64, GUI_RASTER_MAX_PIXELS)) return -2;
    const total: usize = @intCast(total_u64);
    if (@as(u64, pixel_count) < total_u64) return -3;
    const gui = ensureGuiPayload(instance) orelse return -6;
    const frame = guiFrameEnsureBuild(instance) orelse return -6;
    var render = prepareGuiBlitStorage(instance, frame, total, .xrgb32) orelse {
        guiFrameMarkBuildFailed(gui, frame);
        return -6;
    };

    const destination = guiResourcePayloadWords(render.resource.payload);
    var i: usize = 0;
    while (i < total) : (i += 1) {
        destination[i] = pixels[i] & 0x00FF_FFFF;
    }

    const scale_value = if (scale == 0) 1 else @min(scale, @as(u32, 16));
    return commitGuiBlitStorage(&render, .{
        .kind = 4,
        .x = x,
        .y = y,
        .w = w,
        .h = h,
        .flags = scale_value,
        .resource_kind = .xrgb32,
        .payload_offset = render.resource.payload.logical_offset,
        .payload_bytes = render.resource.payload.byte_count,
        .raster_word_offset = render.resource.payload.raster_word_offset,
        .parameter0 = scale_value,
    });
}

fn apiGuiBlendAlpha8(x: i32, y: i32, w: u32, h: u32, stride: u32, rgb: u32, alpha: [*]const u8, alpha_len: u32) callconv(.c) i32 {
    const instance = currentInstance() orelse return -1;
    if (@intFromPtr(alpha) == 0) return -1;
    const source = alpha[0..@as(usize, alpha_len)];
    const geometry = gui_alpha8.validate(
        w,
        h,
        stride,
        source.len,
        GUI_ALPHA8_MAX_WIDTH,
        GUI_ALPHA8_MAX_HEIGHT,
        GUI_ALPHA8_MAX_PIXELS,
    ) catch |err| return switch (err) {
        error.SourceTooSmall => -3,
        else => -2,
    };

    const gui = ensureGuiPayload(instance) orelse return -6;
    const frame = guiFrameEnsureBuild(instance) orelse return -6;
    var render = prepareGuiBlitStorage(instance, frame, geometry.packed_words, .alpha8) orelse {
        guiFrameMarkBuildFailed(gui, frame);
        return -6;
    };

    _ = gui_alpha8.packCompact(
        guiResourcePayloadWords(render.resource.payload),
        source,
        w,
        h,
        stride,
    ) catch {
        cancelGuiBlitStorage(instance, &render);
        return -2;
    };

    return commitGuiBlitStorage(&render, .{
        .kind = GUI_COMMAND_KIND_ALPHA8,
        .x = x,
        .y = y,
        .w = w,
        .h = h,
        .rgb = rgb & 0x00FF_FFFF,
        .flags = 1,
        .resource_kind = .alpha8,
        .payload_offset = render.resource.payload.logical_offset,
        // The frame ABI exposes the exact tight w*h Alpha8 byte slice.  The
        // physical node may retain up to three zero pad bytes so legacy
        // gui_raster_read can still return packed u32 words.
        .payload_bytes = geometry.pixel_count,
        .raster_word_offset = render.resource.payload.raster_word_offset,
        .parameter0 = 0,
        .parameter1 = 0,
    });
}

fn apiGuiDrawText(x: i32, y: i32, text: [*:0]const u8, fg: u32, bg: u32) callconv(.c) i32 {
    const instance = currentInstance() orelse return -1;
    const gui = ensureGuiPayload(instance) orelse return -2;
    return appendGuiTextCommand(instance, x, y, text, fg, bg, gui.font_id, 0);
}

fn apiGuiDrawTextEx(x: i32, y: i32, text: [*:0]const u8, fg: u32, bg: u32, font_id: u32, flags: u32) callconv(.c) i32 {
    const instance = currentInstance() orelse return -1;
    return appendGuiTextCommand(instance, x, y, text, fg, bg, font_id, flags);
}

fn appendGuiTextCommand(instance: *ProgramInstance, x: i32, y: i32, text: [*:0]const u8, fg: u32, bg: u32, requested_font_id: u32, flags: u32) i32 {
    if (@intFromPtr(text) == 0) return -1;
    if (!font.isRenderableFontId(requested_font_id)) return -2;
    const font_id = font.normalizeFontId(requested_font_id);
    var len: usize = 0;
    while (len < GUI_LEGACY_TEXT_APPEND_MAX_BYTES and text[len] != 0) : (len += 1) {}
    if (len == GUI_LEGACY_TEXT_APPEND_MAX_BYTES) return -2;
    // Frame snapshots promise UTF-8 resources even when they originate from
    // the legacy NUL-terminated producer slot.  Reject malformed input before
    // allocating or mutating the private build frame.
    if (!std.unicode.utf8ValidateSlice(text[0..len])) return -2;

    const gui = ensureGuiPayload(instance) orelse return -2;
    const frame = guiFrameEnsureBuild(instance) orelse return -4;
    const node_count: u64 = @as(u64, @intFromBool(guiCommandNeedsNewPayload(frame))) +
        @as(u64, @intFromBool(len != 0));
    if (!guiFrameCanLinkNodes(frame, node_count)) {
        frame.build_failed = true;
        return -4;
    }
    var command_storage = prepareGuiCommandStorage(instance, frame) orelse {
        guiFrameMarkBuildFailed(gui, frame);
        return -4;
    };
    var resource_storage: ?GuiResourceStorage = null;
    if (len != 0) {
        resource_storage = prepareGuiResourceStorage(instance, frame, len, .utf8) orelse {
            cancelGuiCommandStorage(instance, &command_storage);
            guiFrameMarkBuildFailed(gui, frame);
            instance_storage_stats.transaction_rollbacks +%= 1;
            return -4;
        };
        @memcpy(guiResourcePayloadData(resource_storage.?.payload), text[0..len]);
    }

    const metrics = font.measureZWithFont(font_id, text, len + 1);
    var command = ProgramGuiCommand{
        .kind = 3,
        .x = x,
        .y = y,
        .fg = fg,
        .bg = bg,
        .font_id = font_id,
        .flags = flags,
        .text_w = metrics.width,
        .text_h = metrics.height,
        .baseline = metrics.baseline,
        .line_height = metrics.line_height,
    };
    if (resource_storage) |*resource| {
        command.resource_kind = .utf8;
        command.payload_offset = resource.payload.logical_offset;
        command.payload_bytes = resource.payload.byte_count;
    }
    const result = commitGuiCommandStorage(&command_storage, command);
    if (resource_storage) |*resource| commitGuiResourceStorage(resource);
    if (result < 0) return result;
    return @intCast(len);
}

fn apiGuiCommand(id: u32, index: u32, out: *GuiCommand) callconv(.c) i32 {
    reapFinishedInstances();
    const lease = pinProgramInstance(id) orelse {
        out.* = .{};
        return -1;
    };
    defer unpinProgramInstance(&lease);
    const instance = lease.instance;
    var capture = captureCommittedGuiFrame(instance, null) orelse {
        out.* = .{};
        return 0;
    };
    defer releaseCapturedGuiFrame(instance, &capture);
    const location = guiFrameCommandLocationAt(capture.frame, index) orelse {
        out.* = .{};
        return 0;
    };
    if (!materializeLegacyGuiCommandAt(location, out)) {
        out.* = .{};
        return -2;
    }
    return 1;
}

const GuiFrameCommandLocation = struct {
    frame: *const ProgramGuiFramePayload,
    command: *const ProgramGuiCommand,
    resource_base: u64,
    raster_base: u64,
};

fn collectGuiFrameChain(frame: *const ProgramGuiFramePayload, out: *[r4x_api.gui_frame_max_delta_chain]*const ProgramGuiFramePayload) usize {
    var count: usize = 0;
    var cursor: ?*const ProgramGuiFramePayload = frame;
    while (cursor) |item| : (cursor = item.base_frame) {
        if (count >= out.len) return 0;
        out[count] = item;
        count += 1;
    }
    return count;
}

fn guiFrameLocalCommandAt(frame: *const ProgramGuiFramePayload, index: u64) ?*const ProgramGuiCommand {
    if (index >= frame.command_count) return null;
    var cursor = frame.command_payload;
    while (cursor) |payload| : (cursor = payload.next) {
        const end = payload.logical_offset + payload.command_count;
        if (index < end) return &guiCommandPayloadCommandsConst(payload)[index - payload.logical_offset];
    }
    return null;
}

fn guiFrameCommandLocationAt(frame: *const ProgramGuiFramePayload, index: u64) ?GuiFrameCommandLocation {
    var chain: [r4x_api.gui_frame_max_delta_chain]*const ProgramGuiFramePayload = undefined;
    var count = collectGuiFrameChain(frame, &chain);
    if (count == 0) return null;
    var command_base: u64 = 0;
    var resource_base: u64 = 0;
    var raster_base: u64 = 0;
    while (count != 0) {
        count -= 1;
        const item = chain[count];
        if (index < command_base + item.command_count) {
            const command = guiFrameLocalCommandAt(item, index - command_base) orelse return null;
            return .{ .frame = item, .command = command, .resource_base = resource_base, .raster_base = raster_base };
        }
        command_base += item.command_count;
        resource_base += item.resource_len;
        raster_base += item.raster_words;
    }
    return null;
}

fn guiFrameCommandAt(frame: *const ProgramGuiFramePayload, index: u64) ?*const ProgramGuiCommand {
    const location = guiFrameCommandLocationAt(frame, index) orelse return null;
    return location.command;
}

fn copyGuiFrameLocalResourceBytes(frame: *const ProgramGuiFramePayload, start: u64, out: []u8) usize {
    if (out.len == 0 or start >= frame.resource_len) return 0;
    const wanted_u64 = @min(@as(u64, out.len), frame.resource_len - start);
    const wanted: usize = @intCast(wanted_u64);
    var copied: usize = 0;
    var cursor = frame.resource_payload;
    while (cursor) |payload| : (cursor = payload.next) {
        const node_end = payload.logical_offset + payload.byte_count;
        if (start + copied >= node_end) continue;
        const source_start: usize = @intCast(if (start + copied > payload.logical_offset) start + copied - payload.logical_offset else 0);
        const bytes = guiResourcePayloadDataConst(payload);
        const chunk = @min(wanted - copied, bytes.len - source_start);
        @memcpy(out[copied .. copied + chunk], bytes[source_start .. source_start + chunk]);
        copied += chunk;
        if (copied == wanted) break;
    }
    return copied;
}

fn copyGuiFrameResourceBytes(frame: *const ProgramGuiFramePayload, start: u64, out: []u8) usize {
    const total = guiFrameChainResourceBytes(frame);
    if (out.len == 0 or start >= total) return 0;
    var chain: [r4x_api.gui_frame_max_delta_chain]*const ProgramGuiFramePayload = undefined;
    var count = collectGuiFrameChain(frame, &chain);
    if (count == 0) return 0;
    var global_base: u64 = 0;
    var copied: usize = 0;
    while (count != 0 and copied < out.len) {
        count -= 1;
        const item = chain[count];
        const item_end = global_base + item.resource_len;
        if (start + copied < item_end) {
            const local_start = if (start + copied > global_base) start + copied - global_base else 0;
            copied += copyGuiFrameLocalResourceBytes(item, local_start, out[copied..]);
        }
        global_base = item_end;
    }
    return copied;
}

fn materializeLegacyGuiCommandAt(location: GuiFrameCommandLocation, out: *GuiCommand) bool {
    const command = location.command;
    out.* = .{
        .kind = command.kind,
        .x = command.x,
        .y = command.y,
        .w = command.w,
        .h = command.h,
        .rgb = command.rgb,
        .fg = command.fg,
        .bg = command.bg,
        .font_id = command.font_id,
        .flags = command.flags,
        .text_w = command.text_w,
        .text_h = command.text_h,
        .baseline = command.baseline,
        .line_height = command.line_height,
    };
    switch (command.resource_kind) {
        .none => {},
        .utf8 => {
            const count = @min(command.payload_bytes, out.text.len - 1);
            if (copyGuiFrameLocalResourceBytes(location.frame, command.payload_offset, out.text[0..count]) != count) return false;
            out.text[count] = 0;
        },
        .xrgb32 => {
            const raster_offset = std.math.add(u64, location.raster_base, command.raster_word_offset) catch return false;
            if (raster_offset > std.math.maxInt(u32) or command.parameter0 > std.math.maxInt(u32)) return false;
            out.rgb = @intCast(raster_offset);
            // Frame v1 reserves flags and carries XRGB scale in parameter0;
            // synthesize the frozen legacy GuiCommand representation.
            out.flags = @intCast(command.parameter0);
        },
        .alpha8 => {
            const packed_words = (std.math.add(u64, command.payload_bytes, 3) catch return false) / 4;
            const raster_offset = std.math.add(u64, location.raster_base, command.raster_word_offset) catch return false;
            if (raster_offset > std.math.maxInt(u32) or packed_words > std.math.maxInt(u32)) return false;
            out.fg = @intCast(raster_offset);
            out.bg = @intCast(packed_words);
            out.flags = 1;
        },
        .path, .indexed8 => return false,
    }
    return true;
}

fn materializeLegacyGuiCommand(frame: *const ProgramGuiFramePayload, command: *const ProgramGuiCommand, out: *GuiCommand) bool {
    return materializeLegacyGuiCommandAt(.{ .frame = frame, .command = command, .resource_base = 0, .raster_base = 0 }, out);
}

fn guiRasterReadCount(available: u64, capacity: u32) usize {
    const requested: u64 = capacity;
    const return_limit: u64 = std.math.maxInt(i32);
    return @intCast(@min(available, @min(requested, return_limit)));
}

fn copyGuiLocalRasterWords(frame: *const ProgramGuiFramePayload, start: u64, out: []u32) usize {
    if (out.len == 0 or start >= frame.raster_words) return 0;
    const wanted: usize = @intCast(@min(@as(u64, out.len), frame.raster_words - start));
    var copied: usize = 0;
    var command_cursor = frame.command_payload;
    while (command_cursor) |payload| : (command_cursor = payload.next) {
        const commands = guiCommandPayloadCommandsConst(payload);
        for (commands[0..payload.command_count]) |command| {
            if (command.resource_kind != .xrgb32 and command.resource_kind != .alpha8) continue;
            const command_words = if (command.resource_kind == .xrgb32)
                command.payload_bytes / 4
            else
                (std.math.add(u64, command.payload_bytes, 3) catch return copied) / 4;
            const command_end = command.raster_word_offset + command_words;
            var word_index = start + copied;
            if (word_index >= command_end or word_index < command.raster_word_offset) continue;
            while (word_index < command_end and copied < wanted) : ({
                copied += 1;
                word_index += 1;
            }) {
                const local_word = word_index - command.raster_word_offset;
                const local_byte = local_word * 4;
                var encoded = [_]u8{0} ** 4;
                const available = @min(@as(u64, 4), command.payload_bytes - local_byte);
                if (copyGuiFrameLocalResourceBytes(frame, command.payload_offset + local_byte, encoded[0..@intCast(available)]) != available) return copied;
                out[copied] = @as(u32, encoded[0]) |
                    (@as(u32, encoded[1]) << 8) |
                    (@as(u32, encoded[2]) << 16) |
                    (@as(u32, encoded[3]) << 24);
            }
            if (copied == wanted) return copied;
        }
    }
    return copied;
}

fn copyGuiRasterWords(frame: *const ProgramGuiFramePayload, start: u64, out: []u32) usize {
    const total = guiFrameChainRasterWords(frame);
    if (out.len == 0 or start >= total) return 0;
    var chain: [r4x_api.gui_frame_max_delta_chain]*const ProgramGuiFramePayload = undefined;
    var count = collectGuiFrameChain(frame, &chain);
    if (count == 0) return 0;
    var global_base: u64 = 0;
    var copied: usize = 0;
    while (count != 0 and copied < out.len) {
        count -= 1;
        const item = chain[count];
        const item_end = global_base + item.raster_words;
        const position = start + @as(u64, @intCast(copied));
        if (position < item_end) {
            const local_start = if (position > global_base) position - global_base else 0;
            copied += copyGuiLocalRasterWords(item, local_start, out[copied..]);
        }
        global_base = item_end;
    }
    return copied;
}

fn apiGuiRasterRead(id: u32, offset: u32, out: [*]u32, capacity: u32) callconv(.c) i32 {
    reapFinishedInstances();
    if (capacity == 0) return 0;
    if (@intFromPtr(out) == 0) return -1;
    const lease = pinProgramInstance(id) orelse return -2;
    defer unpinProgramInstance(&lease);
    const instance = lease.instance;
    var capture = captureCommittedGuiFrame(instance, null) orelse return 0;
    defer releaseCapturedGuiFrame(instance, &capture);
    const start: u64 = offset;
    const total_words = guiFrameChainRasterWords(capture.frame);
    if (start >= total_words) return 0;
    const available = total_words - start;
    const count = guiRasterReadCount(available, capacity);
    const copied = copyGuiRasterWords(capture.frame, start, out[0..count]);
    return @intCast(copied);
}

fn apiGuiSetTitle(title: [*:0]const u8) callconv(.c) i32 {
    const instance = currentInstance() orelse return -1;
    const gui = ensureGuiPayload(instance) orelse return -2;
    @memset(gui.title[0..], 0);
    var len: usize = 0;
    while (len + 1 < gui.title.len and title[len] != 0) : (len += 1) {
        gui.title[len] = title[len];
    }
    bumpGuiRevision(instance);
    return @intCast(len);
}

fn apiGuiTitle(id: u32, out: [*]u8, capacity: u32) callconv(.c) i32 {
    reapFinishedInstances();
    const instance = instanceById(id) orelse return -1;
    if (capacity == 0) return -2;
    const gui = instance.gui_payload orelse {
        out[0] = 0;
        return 0;
    };
    const max_len: usize = @intCast(capacity - 1);
    var len: usize = 0;
    while (len < max_len and len < gui.title.len and gui.title[len] != 0) : (len += 1) {
        out[len] = gui.title[len];
    }
    out[len] = 0;
    return @intCast(len);
}

fn apiGuiSetMinSize(w: i32, h: i32) callconv(.c) i32 {
    const instance = currentInstance() orelse return -1;
    const gui = ensureGuiPayload(instance) orelse return -2;
    gui.min_client_w = @max(0, w);
    gui.min_client_h = @max(0, h);
    bumpGuiRevision(instance);
    return 0;
}

fn apiGuiMinSize(id: u32, out: *GuiSize) callconv(.c) i32 {
    reapFinishedInstances();
    const instance = instanceById(id) orelse {
        out.* = .{};
        return -1;
    };
    const gui = instance.gui_payload orelse {
        out.* = .{};
        return 0;
    };
    out.* = .{
        .w = gui.min_client_w,
        .h = gui.min_client_h,
    };
    return 0;
}

fn apiGuiPresent() callconv(.c) i32 {
    const instance = currentInstance() orelse return -1;
    const gui = ensureGuiPayload(instance) orelse return -2;
    if (gui.building_frame != null) return guiFrameCommit(instance);
    bumpGuiRevision(instance);
    return 0;
}

fn fillGuiFrameInfoLocked(gui: *const ProgramGuiPayload, owner: ProgramProcessHandle, out: *GuiFrameInfo) void {
    const committed = gui.committed_frame;
    const building = gui.building_frame;
    var flags: u32 = 0;
    if (committed != null) flags |= r4x_api.gui_frame_flag_committed;
    if (building != null) flags |= r4x_api.gui_frame_flag_building;
    if (gui.frame_last_error == r4x_api.gui_frame_error_oom) flags |= r4x_api.gui_frame_flag_last_oom;
    out.* = .{
        .state = if (building != null) r4x_api.gui_frame_state_building else r4x_api.gui_frame_state_idle,
        .flags = flags,
        .owner = owner,
        .committed_generation = if (committed) |frame| frame.generation else 0,
        .building_generation = if (building) |frame| frame.generation else 0,
        .committed_command_count = if (committed) |frame| guiFrameChainCommandCount(frame) else 0,
        .committed_resource_bytes = if (committed) |frame| guiFrameChainResourceBytes(frame) else 0,
        .building_command_count = if (building) |frame| frame.command_count else 0,
        .building_resource_bytes = if (building) |frame| frame.resource_len else 0,
        .current_frame_bytes = guiOwnedFrameBytes(gui),
        .peak_frame_bytes = gui.frame_peak_bytes,
        .commit_count = gui.frame_commits,
        .cancel_count = gui.frame_cancels,
        .oom_count = gui.frame_oom,
        .snapshot_read_count = gui.frame_snapshot_reads,
        .last_error = gui.frame_last_error,
    };
}

fn fillEmptyGuiFrameInfo(owner: ProgramProcessHandle, result: i32, out: *GuiFrameInfo) void {
    out.* = .{ .owner = owner, .last_error = result };
}

fn validGuiFrameInfoOutput(out: *const GuiFrameInfo) bool {
    // The new v1 slots are strict in/out-prefix APIs.  A future caller may
    // advertise a higher version, but it must reserve at least the complete
    // v1 prefix that this kernel writes.
    return out.version >= r4x_api.gui_frame_info_version and out.size >= r4x_api.gui_frame_info_size;
}

fn fillCurrentGuiFrameInfoForId(instance_id: u32, result: i32, out: *GuiFrameInfo) void {
    const lease_value = pinProgramInstance(instance_id) orelse return;
    var lease = lease_value;
    defer unpinProgramInstance(&lease);
    const owner = programHandleForInstance(lease.instance) orelse return;
    const gui = lease.instance.gui_payload orelse return;
    lockGuiFrameState(gui);
    fillGuiFrameInfoLocked(gui, owner, out);
    out.last_error = result;
    _ = gui.frame_lock.unlock();
}

fn externalGuiFrameCommandWithBase(command: *const ProgramGuiCommand, resource_base: u64) GuiFrameCommand {
    return .{
        .version = r4x_api.gui_frame_command_version,
        .size = r4x_api.gui_frame_command_size,
        .kind = command.kind,
        // v1 reserves flags.  Legacy display-list commands still use their
        // internal flags field, but the frame transport normalizes it to 0;
        // raster scale is carried by parameter0.
        .flags = 0,
        .x = command.x,
        .y = command.y,
        .w = command.w,
        .h = command.h,
        .rgb = command.rgb,
        .fg = command.fg,
        .bg = command.bg,
        .font_id = command.font_id,
        .text_w = command.text_w,
        .text_h = command.text_h,
        .baseline = command.baseline,
        .line_height = command.line_height,
        .resource_offset = if (command.payload_bytes == 0) 0 else command.payload_offset + resource_base,
        .resource_bytes = command.payload_bytes,
        .parameter0 = command.parameter0,
        .parameter1 = command.parameter1,
    };
}

fn externalGuiFrameCommand(command: *const ProgramGuiCommand) GuiFrameCommand {
    return externalGuiFrameCommandWithBase(command, 0);
}

fn copyGuiFrameLocalCommands(frame: *const ProgramGuiFramePayload, resource_base: u64, out: []GuiFrameCommand) usize {
    var copied: usize = 0;
    var cursor = frame.command_payload;
    while (cursor) |payload| : (cursor = payload.next) {
        const commands = guiCommandPayloadCommandsConst(payload);
        for (commands[0..payload.command_count]) |*command| {
            out[copied] = externalGuiFrameCommandWithBase(command, resource_base);
            copied += 1;
        }
    }
    return copied;
}

fn copyGuiFrameCommands(frame: *const ProgramGuiFramePayload, out: []GuiFrameCommand) usize {
    var chain: [r4x_api.gui_frame_max_delta_chain]*const ProgramGuiFramePayload = undefined;
    var count = collectGuiFrameChain(frame, &chain);
    if (count == 0) return 0;
    var copied: usize = 0;
    var resource_base: u64 = 0;
    while (count != 0) {
        count -= 1;
        const item = chain[count];
        copied += copyGuiFrameLocalCommands(item, resource_base, out[copied..]);
        resource_base += item.resource_len;
    }
    return copied;
}

fn apiGuiFrameBegin() callconv(.c) i32 {
    const instance = currentInstance() orelse return r4x_api.gui_frame_error_unavailable;
    const result = guiFrameBegin(instance);
    if (instance.gui_payload) |gui| gui.frame_last_error = result;
    return result;
}

fn apiGuiFrameBeginDamage(regions_ptr: [*]const DisplayDamageRect, region_count: u32) callconv(.c) i32 {
    const instance = currentInstance() orelse return r4x_api.gui_frame_error_unavailable;
    const gui = ensureGuiPayload(instance) orelse return r4x_api.gui_frame_error_unavailable;
    if (@intFromPtr(regions_ptr) == 0 or region_count == 0 or region_count > r4x_api.gui_frame_max_damage_regions) {
        return setGuiFrameResult(gui, r4x_api.gui_frame_error_invalid);
    }
    return guiFrameBeginDamage(instance, regions_ptr[0..region_count]);
}

fn apiGuiFrameAppend(
    commands_ptr: ?[*]const GuiFrameCommand,
    command_count: u64,
    resources_ptr: ?[*]const u8,
    resource_len: u64,
) callconv(.c) i32 {
    const instance = currentInstance() orelse return r4x_api.gui_frame_error_unavailable;
    const gui = instance.gui_payload orelse return r4x_api.gui_frame_error_unavailable;
    const building = gui.building_frame orelse return setGuiFrameResult(gui, r4x_api.gui_frame_error_state);
    if (!building.explicit_build or building.build_failed) return setGuiFrameResult(gui, r4x_api.gui_frame_error_state);
    if ((commands_ptr == null and command_count != 0) or (resources_ptr == null and resource_len != 0)) {
        building.build_failed = true;
        return setGuiFrameResult(gui, r4x_api.gui_frame_error_invalid);
    }
    if (command_count > std.math.maxInt(usize) or resource_len > std.math.maxInt(usize)) {
        building.build_failed = true;
        return setGuiFrameResult(gui, r4x_api.gui_frame_error_overflow);
    }
    _ = std.math.add(u64, building.command_count, command_count) catch {
        building.build_failed = true;
        return setGuiFrameResult(gui, r4x_api.gui_frame_error_overflow);
    };
    _ = std.math.add(u64, building.resource_len, resource_len) catch {
        building.build_failed = true;
        return setGuiFrameResult(gui, r4x_api.gui_frame_error_overflow);
    };
    const commands: []const GuiFrameCommand = if (command_count == 0) &.{} else commands_ptr.?[0..@intCast(command_count)];
    const resources: []const u8 = if (resource_len == 0) &.{} else resources_ptr.?[0..@intCast(resource_len)];
    return guiFrameAppendBatch(instance, commands, resources);
}

fn apiGuiFrameCommit() callconv(.c) i32 {
    const instance = currentInstance() orelse return r4x_api.gui_frame_error_unavailable;
    const gui = instance.gui_payload orelse return r4x_api.gui_frame_error_unavailable;
    const result = guiFrameCommit(instance);
    gui.frame_last_error = result;
    return result;
}

fn apiGuiFrameCancel() callconv(.c) i32 {
    const instance = currentInstance() orelse return r4x_api.gui_frame_error_unavailable;
    const gui = instance.gui_payload orelse return r4x_api.gui_frame_error_unavailable;
    const result = guiFrameCancel(instance);
    gui.frame_last_error = result;
    return result;
}

fn apiGuiFrameInfo(handle_ptr: ?*const ProgramProcessHandle, out: *GuiFrameInfo) callconv(.c) i32 {
    if (@intFromPtr(out) == 0) return r4x_api.gui_frame_error_invalid;
    if (!validGuiFrameInfoOutput(out)) return r4x_api.gui_frame_error_invalid;
    const handle = if (handle_ptr) |value| value.* else currentProgramHandle() orelse {
        fillEmptyGuiFrameInfo(.{}, r4x_api.gui_frame_error_unavailable, out);
        return r4x_api.gui_frame_error_unavailable;
    };
    if (!programHandleValid(handle)) {
        fillEmptyGuiFrameInfo(handle, r4x_api.gui_frame_error_invalid, out);
        return r4x_api.gui_frame_error_invalid;
    }
    const lease_value = pinProgramHandle(handle, false) orelse {
        fillEmptyGuiFrameInfo(handle, r4x_api.gui_frame_error_invalid, out);
        fillCurrentGuiFrameInfoForId(handle.instance_id, r4x_api.gui_frame_error_invalid, out);
        return r4x_api.gui_frame_error_invalid;
    };
    var lease = lease_value;
    defer unpinProgramInstance(&lease);
    const gui = lease.instance.gui_payload orelse {
        fillEmptyGuiFrameInfo(handle, r4x_api.gui_frame_error_unavailable, out);
        return r4x_api.gui_frame_error_unavailable;
    };
    lockGuiFrameState(gui);
    fillGuiFrameInfoLocked(gui, handle, out);
    _ = gui.frame_lock.unlock();
    return r4x_api.gui_frame_result_ok;
}

fn apiGuiFrameRead(
    handle_ptr: *const ProgramProcessHandle,
    expected_generation: u64,
    commands_ptr: ?[*]GuiFrameCommand,
    command_capacity: u64,
    resources_ptr: ?[*]u8,
    resource_capacity: u64,
    out: *GuiFrameInfo,
) callconv(.c) i32 {
    if (@intFromPtr(handle_ptr) == 0 or @intFromPtr(out) == 0) return r4x_api.gui_frame_error_invalid;
    if (!validGuiFrameInfoOutput(out)) return r4x_api.gui_frame_error_invalid;
    const handle = handle_ptr.*;
    if (!programHandleValid(handle)) {
        fillEmptyGuiFrameInfo(handle, r4x_api.gui_frame_error_invalid, out);
        return r4x_api.gui_frame_error_invalid;
    }
    const lease_value = pinProgramHandle(handle, false) orelse {
        fillEmptyGuiFrameInfo(handle, r4x_api.gui_frame_error_invalid, out);
        fillCurrentGuiFrameInfoForId(handle.instance_id, r4x_api.gui_frame_error_invalid, out);
        return r4x_api.gui_frame_error_invalid;
    };
    var lease = lease_value;
    defer unpinProgramInstance(&lease);
    const gui = lease.instance.gui_payload orelse {
        fillEmptyGuiFrameInfo(handle, r4x_api.gui_frame_error_unavailable, out);
        return r4x_api.gui_frame_error_unavailable;
    };

    lockGuiFrameState(gui);
    const frame = gui.committed_frame orelse {
        gui.frame_last_error = r4x_api.gui_frame_error_unavailable;
        fillGuiFrameInfoLocked(gui, handle, out);
        _ = gui.frame_lock.unlock();
        return r4x_api.gui_frame_error_unavailable;
    };
    if (frame.generation != expected_generation) {
        gui.frame_last_error = r4x_api.gui_frame_error_stale;
        fillGuiFrameInfoLocked(gui, handle, out);
        _ = gui.frame_lock.unlock();
        return r4x_api.gui_frame_error_stale;
    }
    if ((commands_ptr == null and command_capacity != 0) or (resources_ptr == null and resource_capacity != 0)) {
        gui.frame_last_error = r4x_api.gui_frame_error_invalid;
        fillGuiFrameInfoLocked(gui, handle, out);
        _ = gui.frame_lock.unlock();
        return r4x_api.gui_frame_error_invalid;
    }
    const total_commands = guiFrameChainCommandCount(frame);
    const total_resources = guiFrameChainResourceBytes(frame);
    if (command_capacity < total_commands or resource_capacity < total_resources) {
        gui.frame_last_error = r4x_api.gui_frame_error_buffer_too_small;
        fillGuiFrameInfoLocked(gui, handle, out);
        _ = gui.frame_lock.unlock();
        return r4x_api.gui_frame_error_buffer_too_small;
    }
    if (command_capacity > std.math.maxInt(usize) or resource_capacity > std.math.maxInt(usize) or
        !validateGuiFrame(frame, lease.instance.id) or frame.reader_refs == std.math.maxInt(u32))
    {
        gui.frame_last_error = r4x_api.gui_frame_error_overflow;
        fillGuiFrameInfoLocked(gui, handle, out);
        _ = gui.frame_lock.unlock();
        return r4x_api.gui_frame_error_overflow;
    }
    frame.reader_refs += 1;
    gui.frame_snapshot_reads +%= 1;
    instance_storage_stats.gui_frame_snapshot_reads +%= 1;
    gui.frame_last_error = r4x_api.gui_frame_result_ok;
    fillGuiFrameInfoLocked(gui, handle, out);
    _ = gui.frame_lock.unlock();

    var capture = GuiFrameCapture{ .gui = gui, .frame = frame, .generation = frame.generation };
    if (total_commands != 0) {
        const commands = commands_ptr.?[0..@intCast(total_commands)];
        _ = copyGuiFrameCommands(frame, commands);
    }
    if (total_resources != 0) {
        const resources = resources_ptr.?[0..@intCast(total_resources)];
        _ = copyGuiFrameResourceBytes(frame, 0, resources);
    }
    releaseCapturedGuiFrame(lease.instance, &capture);
    return r4x_api.gui_frame_result_ok;
}

fn validGuiFrameGenerationInfoOutput(out: *const GuiFrameGenerationInfo) bool {
    return out.version >= r4x_api.gui_frame_generation_info_version and out.size >= r4x_api.gui_frame_generation_info_size;
}

fn guiFrameContainsIndexed8(frame: *const ProgramGuiFramePayload) bool {
    var cursor = frame.command_payload;
    while (cursor) |payload| : (cursor = payload.next) {
        for (guiCommandPayloadCommandsConst(payload)[0..payload.command_count]) |command| {
            if (command.kind == r4x_api.gui_frame_command_kind_indexed8) return true;
        }
    }
    return false;
}

fn fillGuiFrameGenerationInfo(gui: *const ProgramGuiPayload, owner: ProgramProcessHandle, frame: *const ProgramGuiFramePayload, out: *GuiFrameGenerationInfo) void {
    var flags: u32 = if (frame.base_frame == null)
        r4x_api.gui_frame_generation_flag_full
    else
        r4x_api.gui_frame_generation_flag_delta;
    if (guiFrameContainsIndexed8(frame)) flags |= r4x_api.gui_frame_generation_flag_indexed8;
    out.* = .{
        .flags = flags,
        .damage_count = frame.damage_count,
        .owner = owner,
        .generation = frame.generation,
        .base_generation = if (frame.base_frame) |base| base.generation else 0,
        .command_count = frame.command_count,
        .resource_bytes = frame.resource_len,
        .total_command_count = guiFrameChainCommandCount(frame),
        .total_resource_bytes = guiFrameChainResourceBytes(frame),
        .chain_depth = frame.chain_depth,
        .delta_commit_count = gui.frame_delta_commits,
        .full_commit_count = gui.frame_full_commits,
        .indexed8_command_count = gui.frame_indexed8_commands,
        .indexed8_resource_bytes = gui.frame_indexed8_resource_bytes,
        .avoided_clone_bytes = gui.frame_avoided_clone_bytes,
        .generation_read_count = gui.frame_generation_reads,
    };
}

fn apiGuiFrameGenerationInfo(handle_ptr: *const ProgramProcessHandle, generation: u64, out: *GuiFrameGenerationInfo) callconv(.c) i32 {
    if (@intFromPtr(handle_ptr) == 0 or @intFromPtr(out) == 0 or generation == 0) return r4x_api.gui_frame_error_invalid;
    if (!validGuiFrameGenerationInfoOutput(out)) return r4x_api.gui_frame_error_invalid;
    const handle = handle_ptr.*;
    if (!programHandleValid(handle)) {
        out.* = .{ .owner = handle, .generation = generation };
        return r4x_api.gui_frame_error_invalid;
    }
    const lease_value = pinProgramHandle(handle, false) orelse {
        out.* = .{ .owner = handle, .generation = generation };
        return r4x_api.gui_frame_error_invalid;
    };
    var lease = lease_value;
    defer unpinProgramInstance(&lease);
    var capture = captureGuiFrameGeneration(lease.instance, generation) orelse {
        out.* = .{ .owner = handle, .generation = generation };
        return r4x_api.gui_frame_error_stale;
    };
    defer releaseCapturedGuiFrame(lease.instance, &capture);
    const gui = capture.gui;
    lockGuiFrameState(gui);
    fillGuiFrameGenerationInfo(gui, handle, capture.frame, out);
    _ = gui.frame_lock.unlock();
    return r4x_api.gui_frame_result_ok;
}

fn apiGuiFrameGenerationRead(
    handle_ptr: *const ProgramProcessHandle,
    generation: u64,
    commands_ptr: ?[*]GuiFrameCommand,
    command_capacity: u64,
    resources_ptr: ?[*]u8,
    resource_capacity: u64,
    regions_ptr: ?[*]DisplayDamageRect,
    region_capacity: u32,
    out: *GuiFrameGenerationInfo,
) callconv(.c) i32 {
    if (@intFromPtr(handle_ptr) == 0 or @intFromPtr(out) == 0 or generation == 0) return r4x_api.gui_frame_error_invalid;
    if (!validGuiFrameGenerationInfoOutput(out)) return r4x_api.gui_frame_error_invalid;
    const handle = handle_ptr.*;
    if (!programHandleValid(handle)) {
        out.* = .{ .owner = handle, .generation = generation };
        return r4x_api.gui_frame_error_invalid;
    }
    const lease_value = pinProgramHandle(handle, false) orelse {
        out.* = .{ .owner = handle, .generation = generation };
        return r4x_api.gui_frame_error_invalid;
    };
    var lease = lease_value;
    defer unpinProgramInstance(&lease);
    var capture = captureGuiFrameGeneration(lease.instance, generation) orelse {
        out.* = .{ .owner = handle, .generation = generation };
        return r4x_api.gui_frame_error_stale;
    };
    defer releaseCapturedGuiFrame(lease.instance, &capture);
    const gui = capture.gui;
    const frame = capture.frame;
    lockGuiFrameState(gui);
    fillGuiFrameGenerationInfo(gui, handle, frame, out);
    _ = gui.frame_lock.unlock();
    if ((commands_ptr == null and command_capacity != 0) or
        (resources_ptr == null and resource_capacity != 0) or
        (regions_ptr == null and region_capacity != 0)) return r4x_api.gui_frame_error_invalid;
    if (command_capacity < frame.command_count or resource_capacity < frame.resource_len or region_capacity < frame.damage_count) {
        return r4x_api.gui_frame_error_buffer_too_small;
    }
    if (command_capacity > std.math.maxInt(usize) or resource_capacity > std.math.maxInt(usize) or
        !validateGuiFrame(frame, lease.instance.id)) return r4x_api.gui_frame_error_overflow;

    if (frame.command_count != 0) {
        _ = copyGuiFrameLocalCommands(frame, 0, commands_ptr.?[0..@intCast(frame.command_count)]);
    }
    if (frame.resource_len != 0) {
        _ = copyGuiFrameLocalResourceBytes(frame, 0, resources_ptr.?[0..@intCast(frame.resource_len)]);
    }
    if (frame.damage_count != 0) @memcpy(regions_ptr.?[0..frame.damage_count], frame.damage_regions[0..frame.damage_count]);
    lockGuiFrameState(gui);
    gui.frame_generation_reads +%= 1;
    fillGuiFrameGenerationInfo(gui, handle, frame, out);
    _ = gui.frame_lock.unlock();
    return r4x_api.gui_frame_result_ok;
}

fn consoleTranscript(instance: *ProgramInstance) ?*ProgramConsoleTranscriptPayload {
    return consolePayload(instance).transcript_payload;
}

fn consoleTranscriptConst(instance: *const ProgramInstance) ?*const ProgramConsoleTranscriptPayload {
    return consolePayloadConst(instance).transcript_payload;
}

fn readConsoleTranscript(transcript: *const ProgramConsoleTranscriptPayload, offset: usize, out: []u8) usize {
    if (offset >= transcript.output_len or out.len == 0) return 0;
    var logical_offset = offset;
    var written: usize = 0;
    var segment_index: usize = 0;
    while (segment_index < transcript.segment_count and written < out.len) : (segment_index += 1) {
        const segment = transcript.segments[segment_index];
        const segment_len: usize = @intCast(segment.length);
        if (logical_offset >= segment_len) {
            logical_offset -= segment_len;
            continue;
        }
        const payload = segment.payload orelse break;
        var sequence = segment.start_sequence + logical_offset;
        var remaining = @min(segment_len - logical_offset, out.len - written);
        while (remaining != 0) {
            const ring_offset: usize = @intCast(sequence % CONSOLE_OUTPUT_SIZE);
            const count = @min(remaining, CONSOLE_OUTPUT_SIZE - ring_offset);
            @memcpy(out[written .. written + count], payload.bytes[ring_offset .. ring_offset + count]);
            written += count;
            sequence += count;
            remaining -= count;
        }
        logical_offset = 0;
    }
    return written;
}

fn removeFirstConsoleTranscriptSegment(transcript: *ProgramConsoleTranscriptPayload, active: bool) bool {
    if (transcript.segment_count == 0) return false;
    const payload = transcript.segments[0].payload orelse return false;
    var index: usize = 1;
    while (index < transcript.segment_count) : (index += 1) transcript.segments[index - 1] = transcript.segments[index];
    transcript.segment_count -= 1;
    transcript.segments[transcript.segment_count] = .{};
    return releaseConsoleOutputPayload(payload, active);
}

fn dropConsoleTranscriptPrefix(instance: *ProgramInstance, byte_count: usize) void {
    const transcript = consoleTranscript(instance) orelse return;
    var remaining = @min(byte_count, @as(usize, @intCast(transcript.output_len)));
    const dropped = remaining;
    while (remaining != 0 and transcript.segment_count != 0) {
        var first = &transcript.segments[0];
        const segment_len: usize = @intCast(first.length);
        if (remaining < segment_len) {
            first.start_sequence += remaining;
            first.length -= @intCast(remaining);
            transcript.output_len -= @intCast(remaining);
            remaining = 0;
            break;
        }
        remaining -= segment_len;
        transcript.output_len -= first.length;
        console_output_segment_drops +%= 1;
        _ = removeFirstConsoleTranscriptSegment(transcript, true);
    }
    const actual = dropped - remaining;
    if (actual != 0) {
        const console = consolePayload(instance);
        console.state.output_dropped_bytes +%= @intCast(actual);
        console_output_segment_drop_bytes +%= @intCast(actual);
    }
}

fn compactConsoleTranscript(instance: *ProgramInstance) bool {
    const transcript = consoleTranscript(instance) orelse return false;
    if (transcript.segment_count < transcript.segments.len) return true;
    const output = allocateConsoleOutputPayload(instance.id) orelse return false;
    const length: usize = @intCast(transcript.output_len);
    const copied = readConsoleTranscript(transcript, 0, output.bytes[0..length]);
    if (copied != length) {
        _ = releaseConsoleOutputPayload(output, true);
        return false;
    }
    output.next_sequence = length;
    if (length != 0 and !retainConsoleOutputPayload(output, true)) {
        _ = releaseConsoleOutputPayload(output, true);
        return false;
    }
    if (!releaseConsoleTranscriptSegments(transcript, true)) {
        if (length != 0) _ = releaseConsoleOutputPayload(output, true);
        _ = releaseConsoleOutputPayload(output, true);
        return false;
    }
    if (length != 0) {
        transcript.segments[0] = .{ .payload = output, .start_sequence = 0, .length = @intCast(length) };
        transcript.segment_count = 1;
        transcript.output_len = @intCast(length);
    }
    _ = releaseConsoleOutputPayload(output, true);
    console_output_compactions +%= 1;
    console_output_compaction_bytes +%= @intCast(length);
    return true;
}

fn ensureConsoleTranscriptSegmentCapacity(instance: *ProgramInstance) bool {
    const transcript = consoleTranscript(instance) orelse return false;
    if (transcript.segment_count < transcript.segments.len) return true;
    if (compactConsoleTranscript(instance)) return true;
    while (transcript.segment_count >= transcript.segments.len and transcript.segment_count != 0) {
        const drop = transcript.segments[0].length;
        dropConsoleTranscriptPrefix(instance, drop);
    }
    return transcript.segment_count < transcript.segments.len;
}

fn appendConsoleTranscriptReference(instance: *ProgramInstance, payload: *ProgramConsoleOutputPayload, sequence: u64, capture: bool) bool {
    const transcript = consoleTranscript(instance) orelse return false;
    if (transcript.segment_count != 0) {
        const tail = &transcript.segments[transcript.segment_count - 1];
        if (tail.payload == payload and tail.start_sequence + tail.length == sequence and tail.length != std.math.maxInt(u32)) {
            tail.length += 1;
            transcript.output_len += 1;
            if (capture) console_output_capture_append_bytes +%= 1 else console_output_visible_append_bytes +%= 1;
            if (transcript.output_len > CONSOLE_OUTPUT_SIZE) dropConsoleTranscriptPrefix(instance, transcript.output_len - CONSOLE_OUTPUT_SIZE);
            consolePayload(instance).state.output_len = transcript.output_len;
            return true;
        }
    }
    if (!ensureConsoleTranscriptSegmentCapacity(instance) or !retainConsoleOutputPayload(payload, true)) return false;
    const index: usize = @intCast(transcript.segment_count);
    transcript.segments[index] = .{ .payload = payload, .start_sequence = sequence, .length = 1 };
    transcript.segment_count += 1;
    transcript.output_len += 1;
    if (capture) console_output_capture_append_bytes +%= 1 else console_output_visible_append_bytes +%= 1;
    if (transcript.output_len > CONSOLE_OUTPUT_SIZE) dropConsoleTranscriptPrefix(instance, transcript.output_len - CONSOLE_OUTPUT_SIZE);
    consolePayload(instance).state.output_len = transcript.output_len;
    return true;
}

fn ensureConsoleWriter(instance: *ProgramInstance) ?*ProgramConsoleOutputPayload {
    const console = consolePayload(instance);
    if (console.writer_payload) |writer| {
        // A backing block is sealed before its first byte could be
        // overwritten.  Visible and completion transcripts may retain
        // different subsets of the source stream, so wrapping one producer
        // ring in place would corrupt the older subset of either consumer.
        if (writer.next_sequence < CONSOLE_OUTPUT_SIZE) return writer;
        const replacement = allocateConsoleOutputPayload(instance.id) orelse return null;
        console.writer_payload = replacement;
        _ = releaseConsoleOutputPayload(writer, true);
        return replacement;
    }
    const writer = allocateConsoleOutputPayload(instance.id) orelse return null;
    console.writer_payload = writer;
    return writer;
}

fn appendConsoleSourceByte(instance: *ProgramInstance, ch: u8) ?struct { payload: *ProgramConsoleOutputPayload, sequence: u64 } {
    const writer = ensureConsoleWriter(instance) orelse return null;
    const sequence = writer.next_sequence;
    writer.bytes[@intCast(sequence % CONSOLE_OUTPUT_SIZE)] = ch;
    writer.next_sequence += 1;
    console_output_source_bytes +%= 1;
    return .{ .payload = writer, .sequence = sequence };
}

fn readConsoleTranscriptTail(transcript: *const ProgramConsoleTranscriptPayload, out: []u8) usize {
    const count = @min(out.len, @as(usize, @intCast(transcript.output_len)));
    return readConsoleTranscript(transcript, @as(usize, @intCast(transcript.output_len)) - count, out[0..count]);
}

fn apiConsoleOutput(id: u32, out: [*]u8, capacity: u32) callconv(.c) i32 {
    if (capacity == 0) return -2;
    if (pinProgramInstance(id)) |lease| {
        defer unpinProgramInstance(&lease);
        const instance = lease.instance;
        if (instance.console_payload == null) {
            out[0] = 0;
            return -3;
        }
        if (!console_output_lock.lock(sync.WAIT_FOREVER)) {
            out[0] = 0;
            return -1;
        }
        defer _ = console_output_lock.unlock();
        const transcript = consoleTranscriptConst(instance) orelse {
            out[0] = 0;
            return -3;
        };
        const max_len: usize = @intCast(capacity - 1);
        const count = readConsoleTranscriptTail(transcript, out[0..max_len]);
        out[count] = 0;
        return @intCast(count);
    }

    const locked = lockProgramRegistry();
    if (!locked) return -1;
    defer unlockProgramRegistry();
    const completion = completionForIdLocked(id) orelse return -1;
    if (completion.app_class != @intFromEnum(AppClass.console)) {
        out[0] = 0;
        return -3;
    }
    const transcript = completion.output_payload orelse {
        out[0] = 0;
        return 0;
    };
    const length: usize = @intCast(completion.output_length);
    const count = @min(@as(usize, @intCast(capacity - 1)), length);
    const copied = readConsoleTranscript(transcript, length - count, out[0..count]);
    if (copied != count) {
        out[0] = 0;
        return -1;
    }
    out[count] = 0;
    return @intCast(count);
}

fn apiConsoleRevision(id: u32) callconv(.c) u32 {
    if (pinProgramInstance(id)) |lease| {
        defer unpinProgramInstance(&lease);
        const instance = lease.instance;
        const console = instance.console_payload orelse return 0;
        return console.revision;
    }
    const locked = lockProgramRegistry();
    if (!locked) return 0;
    defer unlockProgramRegistry();
    const completion = completionForIdLocked(id) orelse return 0;
    return if (completion.app_class == @intFromEnum(AppClass.console)) completion.output_revision else 0;
}

fn apiConsoleState(id: u32, out: *ConsoleState) callconv(.c) i32 {
    if (pinProgramInstance(id)) |lease| {
        defer unpinProgramInstance(&lease);
        const instance = lease.instance;
        if (instance.app_class != .console) {
            out.* = .{};
            return -2;
        }
        refreshConsoleState(instance);
        out.* = consolePayloadConst(instance).state;
        return 0;
    }
    const locked = lockProgramRegistry();
    if (!locked) {
        out.* = .{};
        return -1;
    }
    defer unlockProgramRegistry();
    const completion = completionForIdLocked(id) orelse {
        out.* = .{};
        return -1;
    };
    if (completion.app_class != @intFromEnum(AppClass.console)) {
        out.* = .{};
        return -2;
    }
    out.* = completion.console_state;
    return 0;
}

fn apiConsoleSetMetrics(id: u32, cols: u32, rows: u32) callconv(.c) i32 {
    reapFinishedInstances();
    const instance = instanceById(id) orelse return -1;
    if (instance.done) return -2;
    if (instance.app_class != .console) return -3;
    const next_cols = clampConsoleMetric(cols, CONSOLE_MIN_COLS, CONSOLE_MAX_COLS);
    const next_rows = clampConsoleMetric(rows, CONSOLE_MIN_ROWS, CONSOLE_MAX_ROWS);
    const console = consolePayload(instance);
    if (console.state.cols == next_cols and console.state.rows == next_rows) return 0;
    console.state.cols = next_cols;
    console.state.rows = next_rows;
    clampConsoleCursor(instance);
    bumpConsoleRevision(instance);
    return 0;
}

fn apiConsolePushKey(id: u32, key: u8) callconv(.c) i32 {
    console_input_push_calls +%= 1;
    reapFinishedInstances();
    const lease = pinProgramInstance(id) orelse return -1;
    defer unpinProgramInstance(&lease);
    const instance = lease.instance;
    if (instance.done) return -2;
    if (instance.app_class != .console) return -3;
    const data = [_]u8{key};
    return if (pushConsoleInput(instance, data[0..]) == 1) 0 else -4;
}

fn apiConsolePushInput(id: u32, data: [*]const u8, length: u32) callconv(.c) i32 {
    console_input_push_calls +%= 1;
    console_input_batch_calls +%= 1;
    if (@intFromPtr(data) == 0) return -5;
    reapFinishedInstances();
    const lease = pinProgramInstance(id) orelse return -1;
    defer unpinProgramInstance(&lease);
    const instance = lease.instance;
    if (instance.done) return -2;
    if (instance.app_class != .console) return -3;
    return @intCast(pushConsoleInput(instance, data[0..@as(usize, @intCast(length))]));
}

fn pushConsoleInput(instance: *ProgramInstance, data: []const u8) usize {
    const console = consolePayload(instance);
    console_input_bytes_attempted +%= @as(u64, @intCast(data.len));
    if (!console.input_lock.lock(sync.WAIT_FOREVER)) return 0;
    defer _ = console.input_lock.unlock();
    var accepted: usize = 0;
    while (accepted < data.len) : (accepted += 1) {
        const next_head = (console.input_head + 1) % INPUT_QUEUE_SIZE;
        if (next_head == console.input_tail) break;
        console.input_queue[console.input_head] = data[accepted];
        console.input_head = next_head;
    }
    if (accepted != data.len) console_input_full_events +%= 1;
    if (accepted == 0) return 0;
    console_input_bytes_accepted +%= @as(u64, @intCast(accepted));
    console.state.stdin_bytes +%= @as(u32, @intCast(accepted));
    updateConsoleInputPendingLocked(console);
    if (console.state.stdin_pending > console.input_high_water) console.input_high_water = console.state.stdin_pending;
    _ = console.input_wait.bumpSequenceAndWakeAll(&console.input_generation);
    return accepted;
}

fn apiConsoleWrite(stream_raw: u32, data: [*]const u8, len: u32) callconv(.c) i32 {
    const stream = parseConsoleStream(stream_raw) orelse return -1;
    if (stream == .stdin) return -2;
    const count = @min(@as(usize, @intCast(len)), @as(usize, 4096));
    apiOutputTextSpan(stream, data[0..count]);
    return @intCast(count);
}

fn apiConsoleRead(out: [*]u8, capacity: u32) callconv(.c) i32 {
    console_read_calls +%= 1;
    if (capacity == 0) return 0;
    var count: usize = 0;
    const max_len: usize = @intCast(capacity);
    while (count < max_len) {
        const ch = readInputByte() orelse break;
        out[count] = ch;
        count += 1;
    }
    if (count == 0) console_read_empty +%= 1 else console_read_bytes +%= @intCast(count);
    return @intCast(count);
}

const ConsoleInputWaitContext = struct {
    console: *ProgramConsolePayload,
    source: *ProgramInstance,
    last_generation: u64,
};

fn consoleInputWaitStillNeeded(raw: *anyopaque) bool {
    const context: *ConsoleInputWaitContext = @ptrCast(@alignCast(raw));
    return context.console.input_generation == context.last_generation and
        context.console.input_head == context.console.input_tail and
        !context.source.close_requested;
}

fn releaseConsoleInputLock(raw: *anyopaque) void {
    const console: *ProgramConsolePayload = @ptrCast(@alignCast(raw));
    _ = console.input_lock.unlock();
}

fn mapConsoleInputWaitResult(result: sync.WaitResult) i32 {
    return switch (result) {
        .signaled => blk: {
            console_wait_wakes +%= 1;
            break :blk r4x_api.console_input_wait_ready;
        },
        .timeout => blk: {
            console_wait_timeouts +%= 1;
            break :blk r4x_api.console_input_wait_timeout;
        },
        .cancelled, .killed => blk: {
            console_wait_cancellations +%= 1;
            break :blk r4x_api.console_input_wait_error_closed;
        },
        .none, .failed => r4x_api.console_input_wait_error_failed,
    };
}

fn apiConsoleInputWait(last_generation: u64, timeout_ticks: u64, out_generation: *u64) callconv(.c) i32 {
    if (@intFromPtr(out_generation) == 0) return r4x_api.console_input_wait_error_invalid;
    out_generation.* = last_generation;
    console_wait_calls +%= 1;
    const source = currentInstance() orelse return r4x_api.console_input_wait_error_invalid;
    if (source.app_class != .console) return r4x_api.console_input_wait_error_invalid;
    const source_console = consolePayload(source);

    var input_lease: ?ProgramInstanceLease = null;
    if (source_console.io_target_id != 0) {
        const handle = ProgramProcessHandle{
            .instance_id = source_console.io_target_id,
            .reserved = 0,
            .generation = source_console.io_target_generation,
        };
        input_lease = pinProgramHandle(handle, false) orelse return r4x_api.console_input_wait_error_closed;
    } else if (source.role == .background or (source.role == .shell and source_console.host != .none)) {
        const handle = currentProgramHandle() orelse return r4x_api.console_input_wait_error_invalid;
        input_lease = pinProgramHandle(handle, false) orelse return r4x_api.console_input_wait_error_closed;
    }
    defer if (input_lease) |*lease| unpinProgramInstance(lease);

    if (input_lease) |lease| {
        const console = consolePayload(lease.instance);
        if (!console.input_lock.lock(sync.WAIT_FOREVER)) return r4x_api.console_input_wait_error_failed;
        const current_generation = console.input_generation;
        if (source.close_requested) {
            out_generation.* = current_generation;
            _ = console.input_lock.unlock();
            console_wait_cancellations +%= 1;
            return r4x_api.console_input_wait_error_closed;
        }
        if (console.input_head != console.input_tail or current_generation != last_generation) {
            out_generation.* = current_generation;
            _ = console.input_lock.unlock();
            console_wait_immediate +%= 1;
            return r4x_api.console_input_wait_ready;
        }
        if (timeout_ticks == 0) {
            out_generation.* = current_generation;
            _ = console.input_lock.unlock();
            console_wait_timeouts +%= 1;
            return r4x_api.console_input_wait_timeout;
        }
        var context = ConsoleInputWaitContext{ .console = console, .source = source, .last_generation = last_generation };
        console_wait_blocks +%= 1;
        const result = console.input_wait.waitUnlessReleasing(
            timeout_ticks,
            "console-input",
            consoleInputWaitStillNeeded,
            &context,
            releaseConsoleInputLock,
            console,
        );
        out_generation.* = console.input_wait.readSequence(&console.input_generation);
        return mapConsoleInputWaitResult(result);
    }

    const current_generation = keyboard.inputGeneration();
    if (keyboard.pending() or current_generation != last_generation) {
        out_generation.* = current_generation;
        console_wait_immediate +%= 1;
        return r4x_api.console_input_wait_ready;
    }
    if (timeout_ticks == 0) {
        out_generation.* = current_generation;
        console_wait_timeouts +%= 1;
        return r4x_api.console_input_wait_timeout;
    }
    console_wait_blocks +%= 1;
    return mapConsoleInputWaitResult(keyboard.waitInput(last_generation, timeout_ticks, out_generation));
}

fn signalConsoleInputPayload(console: *ProgramConsolePayload) void {
    if (!console.input_lock.lock(sync.WAIT_FOREVER)) return;
    _ = console.input_wait.bumpSequenceAndWakeAll(&console.input_generation);
    _ = console.input_lock.unlock();
}

fn signalConsoleInputForInstance(source: *ProgramInstance) void {
    if (source.app_class != .console) return;
    const source_console = consolePayload(source);
    if (source_console.io_target_id != 0) {
        const handle = ProgramProcessHandle{
            .instance_id = source_console.io_target_id,
            .reserved = 0,
            .generation = source_console.io_target_generation,
        };
        if (pinProgramHandle(handle, false)) |target| {
            signalConsoleInputPayload(consolePayload(target.instance));
            var lease = target;
            unpinProgramInstance(&lease);
        }
        return;
    }
    if (source.role == .background or (source.role == .shell and source_console.host != .none)) {
        signalConsoleInputPayload(source_console);
    } else {
        keyboard.signalInputActivity();
    }
}

fn signalConsoleInputForHandle(handle: ProgramProcessHandle) void {
    var lease = pinProgramHandle(handle, true) orelse return;
    defer unpinProgramInstance(&lease);
    signalConsoleInputForInstance(lease.instance);
}

fn kprintOutputHook(ch: u8) callconv(.c) bool {
    if (boot_status.isBootLogRedirectActive()) {
        k.serialPutcRaw(ch);
        bootlog.putc(ch);
        return true;
    }
    return routeCurrentConsoleOutput(.stdout, ch);
}

fn keyboardInputHook() callconv(.c) u8 {
    const instance = currentConsoleInstance() orelse return 0;
    return popInput(instance) orelse 0;
}

fn currentConsoleInstance() ?*ProgramInstance {
    const instance = currentInstance() orelse return null;
    if (instance.app_class != .console) return null;
    const console = consolePayload(instance);
    if (console.io_target_id != 0) return consoleTargetByHandle(.{
        .instance_id = console.io_target_id,
        .reserved = 0,
        .generation = console.io_target_generation,
    });
    if (instance.role == .background) return instance;
    if (instance.role == .shell and console.host != .none) return instance;
    return null;
}

fn consoleTargetById(id: u32) ?*ProgramInstance {
    const handle = programHandleForId(id) orelse return null;
    return consoleTargetByHandle(handle);
}

fn consoleTargetByHandle(handle: ProgramProcessHandle) ?*ProgramInstance {
    const locked = lockProgramRegistry();
    if (!locked) return null;
    defer unlockProgramRegistry();
    const slot = lookupProgramRegistryHandleLocked(handle, false) orelse return null;
    if (!programRegistryStateIsRunning(slot.state) or slot.instance.done or slot.instance.app_class != .console) return null;
    return &slot.instance;
}

fn inheritedConsoleTargetId(app_class: AppClass) u32 {
    if (app_class != .console) return 0;
    const host = currentConsoleInstance() orelse return 0;
    return host.id;
}

fn popInput(instance: *ProgramInstance) ?u8 {
    const console = consolePayload(instance);
    if (!console.input_lock.lock(sync.WAIT_FOREVER)) return null;
    defer _ = console.input_lock.unlock();
    if (console.input_head == console.input_tail) return null;
    const ch = console.input_queue[console.input_tail];
    console.input_tail = (console.input_tail + 1) % INPUT_QUEUE_SIZE;
    updateConsoleInputPendingLocked(console);
    return ch;
}

fn routeCurrentConsoleOutput(stream: ConsoleStream, ch: u8) bool {
    const data = [_]u8{ch};
    return routeCurrentConsoleOutputBatch(stream, data[0..], false);
}

fn routeCurrentConsoleOutputBatch(stream: ConsoleStream, data: []const u8, normalize_text: bool) bool {
    if (data.len == 0) return true;
    const source = currentInstance() orelse return false;
    if (source.app_class != .console) return false;

    const source_console = consolePayload(source);
    const inherited = source_console.io_target_id != 0;
    var target_lease: ?ProgramInstanceLease = null;
    var target: ?*ProgramInstance = null;
    if (inherited) {
        const target_handle = ProgramProcessHandle{
            .instance_id = source_console.io_target_id,
            .reserved = 0,
            .generation = source_console.io_target_generation,
        };
        if (pinProgramHandle(target_handle, false)) |lease| {
            target_lease = lease;
            if (lease.instance.app_class == .console) target = lease.instance;
        }
    } else {
        target = currentConsoleInstance() orelse return false;
    }
    defer if (target_lease) |*lease| unpinProgramInstance(lease);

    if (!console_output_lock.lock(sync.WAIT_FOREVER)) return true;
    defer _ = console_output_lock.unlock();
    console_output_write_calls +%= 1;
    var source_changed = false;
    var target_changed = false;

    for (data) |raw| {
        if (normalize_text and raw == '\r') continue;
        if (normalize_text and raw == '\n') {
            emitConsoleOutputByte(source, target, stream, '\r', inherited, &source_changed, &target_changed);
            emitConsoleOutputByte(source, target, stream, '\n', inherited, &source_changed, &target_changed);
        } else {
            emitConsoleOutputByte(source, target, stream, raw, inherited, &source_changed, &target_changed);
        }
    }

    if (target) |presented| {
        if (presented == source) {
            if (source_changed or target_changed) bumpConsoleOutputRevision(source, true);
        } else {
            if (source_changed) bumpConsoleOutputRevision(source, false);
            if (target_changed) bumpConsoleOutputRevision(presented, true);
        }
    } else if (source_changed) {
        bumpConsoleOutputRevision(source, false);
    }
    return true;
}

fn emitConsoleOutputByte(
    source: *ProgramInstance,
    target: ?*ProgramInstance,
    stream: ConsoleStream,
    ch: u8,
    inherited: bool,
    source_changed: *bool,
    target_changed: *bool,
) void {
    if (ch == 0x0C) {
        clearConsoleTranscriptState(source);
        source_changed.* = true;
        if (target) |presented| {
            if (presented != source) clearConsoleTranscriptState(presented);
            target_changed.* = true;
            mirrorConsoleControl(presented, ch);
        }
        return;
    }
    if (ch == 0x08) {
        backspaceConsoleTranscriptState(source);
        source_changed.* = true;
        if (target) |presented| {
            if (presented != source) backspaceConsoleTranscriptState(presented);
            target_changed.* = true;
            mirrorConsoleControl(presented, ch);
        }
        return;
    }

    const stored = appendConsoleSourceByte(source, ch) orelse return;
    const source_capture = inherited and (target == null or target.? != source);
    const source_appended = appendConsoleTranscriptReference(source, stored.payload, stored.sequence, source_capture);
    if (source_appended) {
        bumpConsoleStreamCounter(source, stream);
        advanceConsoleCursor(source, ch);
        source_changed.* = true;
    }

    if (target) |presented| {
        if (presented == source) {
            target_changed.* = source_appended;
            if (source_appended) mirrorConsoleByte(presented, ch);
        } else {
            const target_appended = appendConsoleTranscriptReference(presented, stored.payload, stored.sequence, false);
            if (target_appended) {
                bumpConsoleStreamCounter(presented, stream);
                advanceConsoleCursor(presented, ch);
                target_changed.* = true;
                mirrorConsoleByte(presented, ch);
            }
            if (source_appended and target_appended) console_output_shared_bytes +%= 1;
        }
    }
}

fn backspaceConsoleTranscriptState(instance: *ProgramInstance) void {
    const console = consolePayload(instance);
    const transcript = consoleTranscript(instance) orelse return;
    if (transcript.segment_count != 0 and transcript.output_len != 0) {
        const tail = &transcript.segments[transcript.segment_count - 1];
        tail.length -= 1;
        transcript.output_len -= 1;
        if (tail.length == 0) {
            const payload = tail.payload orelse return;
            transcript.segment_count -= 1;
            transcript.segments[transcript.segment_count] = .{};
            _ = releaseConsoleOutputPayload(payload, true);
        }
    }
    console.state.output_len = transcript.output_len;
    if (console.state.cursor_x > 0) {
        console.state.cursor_x -= 1;
    } else if (console.state.cursor_y > 0) {
        console.state.cursor_y -= 1;
        console.state.cursor_x = @intCast(if (console.state.cols > 0) console.state.cols - 1 else 0);
    }
}

pub fn isHostedConsoleContext() bool {
    return currentConsoleInstance() != null;
}

pub fn clearHostedConsoleSurface() bool {
    const instance = currentConsoleInstance() orelse return false;
    clearConsoleOutput(instance);
    return true;
}

pub fn setHostedConsoleColors(fg: u32, bg: u32) bool {
    const instance = currentConsoleInstance() orelse return false;
    const console = consolePayload(instance);
    console.state.fg = fg;
    console.state.bg = bg;
    if (isRawFullscreenPresenter(instance)) k.setConsoleColors(fg, bg);
    bumpConsoleRevision(instance);
    return true;
}

pub fn currentConsoleState() ?ConsoleState {
    const instance = currentConsoleInstance() orelse return null;
    refreshConsoleState(instance);
    return consolePayloadConst(instance).state;
}

pub fn currentConsoleHostKind() ConsoleHostKind {
    const instance = currentConsoleInstance() orelse return .none;
    return consolePayloadConst(instance).host;
}

pub fn requestDesktopFromHostedConsole() bool {
    const instance = currentConsoleInstance() orelse return false;
    if (instance.role != .background or consolePayloadConst(instance).host != .terminal_mode) return false;
    instance.desktop_requested = true;
    bumpConsoleRevision(instance);
    return true;
}

fn clearConsoleOutput(instance: *ProgramInstance) void {
    if (!console_output_lock.lock(sync.WAIT_FOREVER)) return;
    clearConsoleTranscriptState(instance);
    _ = console_output_lock.unlock();
    bumpConsoleOutputRevision(instance, true);
}

fn clearConsoleTranscriptState(instance: *ProgramInstance) void {
    const console = consolePayload(instance);
    const transcript = consoleTranscript(instance) orelse return;
    _ = releaseConsoleTranscriptSegments(transcript, true);
    console.state.clear_count +%= 1;
    console.state.output_len = 0;
    console.state.scrollback_lines = 1;
    console.state.output_dropped_bytes = 0;
    console.state.cursor_x = 0;
    console.state.cursor_y = 0;
    console.state.cursor_visible = 1;
    if (isRawFullscreenPresenter(instance)) k.clearConsole();
}

fn mirrorConsoleControl(instance: *const ProgramInstance, ch: u8) void {
    if (consolePayloadConst(instance).host != .terminal_mode) return;
    k.serialPutcRaw(ch);
    if (isRawFullscreenPresenter(instance)) k.consolePutcRaw(ch);
}

fn mirrorConsoleByte(instance: *const ProgramInstance, ch: u8) void {
    if (consolePayloadConst(instance).host != .terminal_mode) return;
    k.serialPutcRaw(ch);
    if (isRawFullscreenPresenter(instance)) k.consolePutcRaw(ch);
}

fn isRawFullscreenPresenter(instance: *const ProgramInstance) bool {
    return instance.role == .shell and consolePayloadConst(instance).host == .terminal_mode;
}

fn bumpConsoleStreamCounter(instance: *ProgramInstance, stream: ConsoleStream) void {
    const console = consolePayload(instance);
    switch (stream) {
        .stdout => console.state.stdout_bytes +%= 1,
        .stderr => console.state.stderr_bytes +%= 1,
        .stdin => {},
    }
}

fn updateConsoleInputPending(instance: *ProgramInstance) void {
    const console = consolePayload(instance);
    if (!console.input_lock.lock(sync.WAIT_FOREVER)) return;
    defer _ = console.input_lock.unlock();
    updateConsoleInputPendingLocked(console);
}

fn updateConsoleInputPendingLocked(console: *ProgramConsolePayload) void {
    if (console.input_head >= console.input_tail) {
        console.state.stdin_pending = @intCast(console.input_head - console.input_tail);
    } else {
        console.state.stdin_pending = @intCast(INPUT_QUEUE_SIZE - console.input_tail + console.input_head);
    }
}

fn refreshConsoleState(instance: *ProgramInstance) void {
    updateConsoleInputPending(instance);
    const console = consolePayload(instance);
    console.state.output_capacity = @intCast(CONSOLE_OUTPUT_SIZE);
    if (!console_output_lock.lock(sync.WAIT_FOREVER)) return;
    defer _ = console_output_lock.unlock();
    const transcript = consoleTranscriptConst(instance) orelse return;
    console.state.output_len = transcript.output_len;
    console.state.scrollback_lines = countConsoleOutputLines(transcript, console.state.cols);
}

fn countConsoleOutputLines(transcript: *const ProgramConsoleTranscriptPayload, configured_cols: u32) u32 {
    const cols = @max(@as(u32, 1), configured_cols);
    var lines: u32 = 0;
    var line_len: u32 = 0;
    var offset: usize = 0;
    var buffer: [256]u8 = undefined;
    while (offset < transcript.output_len) {
        const count = readConsoleTranscript(transcript, offset, buffer[0..]);
        if (count == 0) break;
        for (buffer[0..count]) |ch| {
            if (ch == '\r') continue;
            if (ch == '\n' or line_len >= cols) {
                lines += 1;
                line_len = 0;
                if (ch == '\n') continue;
            }
            if (consolePrintable(ch)) line_len += 1;
        }
        offset += count;
    }
    if (line_len > 0 or lines == 0) lines += 1;
    return lines;
}

fn consolePrintable(ch: u8) bool {
    return ch >= 0x20 and ch != 0x7F;
}

fn parseConsoleStream(raw: u32) ?ConsoleStream {
    return switch (raw) {
        0 => .stdin,
        1 => .stdout,
        2 => .stderr,
        else => null,
    };
}

fn advanceConsoleCursor(instance: *ProgramInstance, ch: u8) void {
    const console = consolePayload(instance);
    if (ch == '\r') {
        console.state.cursor_x = 0;
        return;
    }
    if (ch == '\n') {
        console.state.cursor_x = 0;
        advanceConsoleRow(instance);
        return;
    }
    if (ch < 0x20 or ch == 0x7F) return;
    console.state.cursor_x += 1;
    if (console.state.cursor_x >= @as(i32, @intCast(console.state.cols))) {
        console.state.cursor_x = 0;
        advanceConsoleRow(instance);
    }
    clampConsoleCursor(instance);
}

fn advanceConsoleRow(instance: *ProgramInstance) void {
    const console = consolePayload(instance);
    const max_y: i32 = @intCast(if (console.state.rows > 0) console.state.rows - 1 else 0);
    if (console.state.cursor_y < max_y) {
        console.state.cursor_y += 1;
    } else {
        console.state.cursor_y = max_y;
    }
}

fn clampConsoleCursor(instance: *ProgramInstance) void {
    const console = consolePayload(instance);
    const max_x: i32 = @intCast(if (console.state.cols > 0) console.state.cols - 1 else 0);
    const max_y: i32 = @intCast(if (console.state.rows > 0) console.state.rows - 1 else 0);
    if (console.state.cursor_x < 0) console.state.cursor_x = 0;
    if (console.state.cursor_y < 0) console.state.cursor_y = 0;
    if (console.state.cursor_x > max_x) console.state.cursor_x = max_x;
    if (console.state.cursor_y > max_y) console.state.cursor_y = max_y;
}

fn clampConsoleMetric(value: u32, min: u32, max: u32) u32 {
    if (value < min) return min;
    if (value > max) return max;
    return value;
}

fn bumpConsoleRevision(instance: *ProgramInstance) void {
    const console = consolePayload(instance);
    console.revision +%= 1;
    if (console.revision == 0) console.revision = 1;
    desktop_events.signal();
}

fn bumpConsoleOutputRevision(instance: *ProgramInstance, present: bool) void {
    const console = consolePayload(instance);
    console.revision +%= 1;
    if (console.revision == 0) console.revision = 1;
    console_output_revision_batches +%= 1;
    if (present) {
        console_output_desktop_signals +%= 1;
        desktop_events.signal();
    }
}

fn appendGuiCommand(instance: *ProgramInstance, command: ProgramGuiCommand) i32 {
    const gui = ensureGuiPayload(instance) orelse return -2;
    const frame = guiFrameEnsureBuild(instance) orelse return -4;
    var storage = prepareGuiCommandStorage(instance, frame) orelse {
        guiFrameMarkBuildFailed(gui, frame);
        return -4;
    };
    return commitGuiCommandStorage(&storage, command);
}

fn bumpGuiRevision(instance: *ProgramInstance) void {
    const gui = instance.gui_payload orelse return;
    gui.revision +%= 1;
    if (gui.revision == 0) gui.revision = 1;
    desktop_events.signal();
}

fn resolveAppClass(policy: LaunchPolicy, flags: u32) AppClass {
    return switch (policy) {
        .console => .console,
        .gui => .gui,
        .auto => {
            if ((flags & R4X_FLAG_APP_CLASS_CONSOLE) != 0) return .console;
            if ((flags & R4X_FLAG_APP_CLASS_GUI) != 0) return .gui;
            if ((flags & R4X_FLAG_APP_CLASS_SERVICE) != 0) return .service;
            return .gui;
        },
    };
}

fn appClassFlagCount(flags: u32) u8 {
    var count: u8 = 0;
    if ((flags & R4X_FLAG_APP_CLASS_CONSOLE) != 0) count += 1;
    if ((flags & R4X_FLAG_APP_CLASS_GUI) != 0) count += 1;
    if ((flags & R4X_FLAG_APP_CLASS_SERVICE) != 0) count += 1;
    return count;
}

fn classifyProgramFile(file: ProgramFile, policy: LaunchPolicy) ?AppClass {
    const source = programModuleFileSource(file);
    var reader = module_r4m.Reader.init(source, @intCast(file.entry.size));
    const header = readR4MProgramHeaderFromReader(&reader, "r4x-classify-header", false) orelse return null;
    const app_class = resolveAppClass(policy, header.flags);
    const export_contract = scanR4XStartExports(&reader, header, false) orelse return null;
    _ = readValidatedProgramMemoryContractFromReader(&reader, header, app_class, export_contract, false) orelse return null;
    return app_class;
}

fn parseLaunchPolicy(value: u32) ?LaunchPolicy {
    return switch (value) {
        0 => .auto,
        1 => .console,
        2 => .gui,
        else => null,
    };
}

fn roleName(role: InstanceRole) []const u8 {
    return switch (role) {
        .foreground => "foreground",
        .shell => "shell",
        .background => "background",
    };
}

fn appClassName(app_class: AppClass) []const u8 {
    return switch (app_class) {
        .console => "console",
        .gui => "gui",
        .service => "service",
    };
}

fn memoryProfileName(profile: MemoryProfile) []const u8 {
    return switch (profile) {
        .unknown => "unknown",
        .tiny => "tiny",
        .normal => "normal",
        .desktop => "desktop",
        .service => "service",
        .large_service => "large-service",
        .build_tool => "build-tool",
        .browser => "browser",
        .workstation => "workstation",
    };
}

fn instanceStateName(state: InstanceState) []const u8 {
    return switch (state) {
        .running => "running",
        .close_requested => "closing",
        .done => "done",
    };
}

fn consoleHostName(host: ConsoleHostKind) []const u8 {
    return switch (host) {
        .none => "none",
        .terminal_window => "terminal-window",
        .terminal_mode => "terminal-mode",
    };
}

fn parseConsoleHost(value: u32) ?ConsoleHostKind {
    return switch (value) {
        0 => .none,
        1 => .terminal_window,
        2 => .terminal_mode,
        else => null,
    };
}

fn createInstance(
    reservation: *const ProgramInstanceReservation,
    role: InstanceRole,
    app_class: AppClass,
    loaded: LoadedProgram,
    stack: ProgramStack,
    args: []const u8,
    working_drive: *drive.Drive,
    module_origin: []const u8,
) ?*ProgramInstance {
    const parent = currentInstance();
    const inherit_environment = if (parent) |source| processPayloadConst(source).environment_len != 0 else false;
    const instance_id = reservation.id;
    const instance = &reservation.slot.instance;
    const storage = allocateProgramInstanceStorage(instance_id, app_class, inherit_environment) orelse return null;
    {
        instance.* = .{
            .used = false,
            .id = instance_id,
            .task_id = 0,
            .role = role,
            .app_class = app_class,
            .entry = loaded.entry,
            .stack_top = stack.top,
            .program_image_range_id = loaded.image.range_id,
            .program_image_base = @intFromPtr(loaded.image.code.ptr),
            .program_image_size = loaded.image.code.len,
            .program_stack_range_id = stack.range_id,
            .program_stack_base = stack.base,
            .program_stack_reserve_size = stack.reserve_size,
            .program_stack_committed_base = stack.committed_base,
            .program_stack_committed_size = stack.committed_size,
            .program_stack_guard_base = stack.guard_base,
            .program_stack_guard_size = stack.guard_size,
            .program_stack_initial_commit_size = stack.initial_commit_size,
            .program_stack_create_cycles = stack.create_cycles,
            .program_stack_telemetry_high_water = stack.telemetry_high_water,
            .program_stack_telemetry_committed_pages = stack.telemetry_committed_pages,
            .program_stack_serial_telemetry = stack.serial_telemetry,
            .program_stack_telemetry_measured = stack.telemetry_measured,
            .memory_profile = loaded.memory_contract.profile,
            .memory_limits = loaded.memory_contract.limits,
            .memory_tag = loaded.memory_contract.tag,
            .runtime_payload = storage.runtime,
            .process_payload = storage.process,
            .console_payload = storage.console,
            .gui_payload = storage.gui,
            .display_used = false,
            .close_requested = false,
            .desktop_requested = false,
            .done = false,
            .exit_code = 0,
        };
        copyArgsInto(instance, args);
        inheritEnvironmentInto(instance, parent);
        copyWorkingDirectoryInto(instance, working_drive);
        {
            const runtime = runtimePayload(instance);
            const n = if (module_origin.len > runtime.module_path.len) runtime.module_path.len else module_origin.len;
            @memcpy(runtime.module_path[0..n], module_origin[0..n]);
            runtime.module_path_len = @intCast(n);
        }
        copyR4XStartImportsInto(instance, loaded.imports[0..@intCast(loaded.import_count)]);
        prepareR4XStartContext(instance);
        return instance;
    }
}

fn rollbackReservedProgramInstance(reservation: *const ProgramInstanceReservation) void {
    const instance = &reservation.slot.instance;
    if (reservation.slot.state != .create or reservation.slot.generation != reservation.generation or reservation.slot.public_id != reservation.id) return;

    var resources = programResourcesFromInstance(instance);
    if (instance.runtime_payload) |runtime| {
        if (instance.process_payload) |process| {
            var storage = ProgramInstanceStorage{
                .runtime = runtime,
                .process = process,
                .console = instance.console_payload,
                .gui = instance.gui_payload,
            };
            rollbackProgramInstanceStorage(instance.id, &storage);
        }
    }
    instance.runtime_payload = null;
    instance.process_payload = null;
    instance.console_payload = null;
    instance.gui_payload = null;
    _ = cleanupProgramResources(&resources);
    _ = mem_virt.releaseOwner(.r4x_instance, reservation.id, .virtual_range);
    _ = mem_backing_store.releaseR4xOwner(reservation.id);
    instance.* = .{ .id = reservation.id };
    cancelProgramInstanceReservation(reservation);
}

fn copyR4XStartImportsInto(instance: *ProgramInstance, seeds: []const R4XStartImportSeed) void {
    const runtime = runtimePayload(instance);
    const count = if (seeds.len > MAX_R4M_IMPORTS) MAX_R4M_IMPORTS else seeds.len;
    runtime.r4xstart_import_count = @intCast(count);
    var i: usize = 0;
    while (i < count) : (i += 1) {
        const seed = seeds[i];
        @memset(runtime.r4xstart_import_module_names[i][0..], 0);
        @memset(runtime.r4xstart_import_symbol_names[i][0..], 0);
        @memcpy(runtime.r4xstart_import_module_names[i][0..seed.module_name_len], seed.module_name[0..seed.module_name_len]);
        @memcpy(runtime.r4xstart_import_symbol_names[i][0..seed.symbol_name_len], seed.symbol_name[0..seed.symbol_name_len]);
        runtime.r4xstart_imports[i] = .{
            .group_id = seed.group_id,
            .min_version = seed.min_version,
            .resolved_version = seed.resolved_version,
            .flags = seed.flags,
            .module_name = @intFromPtr(&runtime.r4xstart_import_module_names[i]),
            .symbol_name = @intFromPtr(&runtime.r4xstart_import_symbol_names[i]),
            .table = @intCast(seed.table),
        };
    }
}

fn prepareR4XStartContext(instance: *ProgramInstance) void {
    const runtime = runtimePayload(instance);
    const process = processPayload(instance);
    var flags: u32 = R4XSTART_FLAG_CLOSE_SUPPORTED | R4XSTART_FLAG_YIELD_SUPPORTED;
    if (runtime.r4xstart_import_count > 0) flags |= R4XSTART_FLAG_IMPORTS_VALID;
    const arg_len = cStringLen(process.args[0..]);
    runtime.r4xstart_context = .{
        .magic = R4XSTART_MAGIC,
        .abi_major = R4XSTART_ABI_MAJOR,
        .abi_minor = R4XSTART_ABI_MINOR,
        .size = R4XSTART_CONTEXT_SIZE,
        .flags = flags,
        .instance_id = instance.id,
        .app_class = r4xstartAppClass(instance.app_class),
        .reserved0 = 0,
        .program_path = 0,
        .args = @intFromPtr(&process.args),
        .args_len = arg_len,
        .imports = if (runtime.r4xstart_import_count > 0) @intFromPtr(&runtime.r4xstart_imports) else 0,
        .import_count = runtime.r4xstart_import_count,
        .reserved1 = 0,
        .reserved_memory0 = 0,
        .reserved_memory1 = 0,
        .exit = 0,
        .should_close = @intFromPtr(&r4xstartShouldClose),
        .yield = @intFromPtr(&r4xstartYield),
        // Der ABI-reservierte Slot bleibt 0; alle Systemfunktionen werden
        // ausschliesslich ueber die R4L-Gruppentabellen in imports geliefert.
        .reserved_runtime = 0,
        .reserved2 = 0,
    };
}

fn r4xstartAppClass(app_class: AppClass) u32 {
    return switch (app_class) {
        .console => 1,
        .gui => 2,
        .service => 3,
    };
}

fn cStringLen(bytes: []const u8) u64 {
    var len: usize = 0;
    while (len < bytes.len and bytes[len] != 0) : (len += 1) {}
    return @intCast(len);
}

fn r4xstartShouldClose(ctx: *const R4XStartContext) callconv(.c) u32 {
    _ = ctx;
    const instance = currentInstance() orelse return 1;
    return if (instance.close_requested) 1 else 0;
}

fn r4xstartYield(ctx: *const R4XStartContext) callconv(.c) void {
    _ = ctx;
    scheduler.yield();
}

fn r4xstartR4SysWrite(data: [*]const u8, len: u32) callconv(.c) i32 {
    const limit: usize = @intCast(len);
    apiOutputTextSpan(.stdout, data[0..limit]);
    return @intCast(len);
}

fn r4xstartR4SysPutc(ch: u8) callconv(.c) void {
    apiOutputTextByte(.stdout, ch);
}

fn appendExitHistoryLocked(handle: ProgramProcessHandle, instance: *const ProgramInstance, exit_code: i32, exit_reason: u8, start_tick: u64, finish_tick: u64) u64 {
    const sequence = next_exit_sequence;
    last_exit_sequence = sequence;
    if (next_exit_sequence != std.math.maxInt(u64)) next_exit_sequence += 1;
    exit_history[exit_history_head] = .{
        .used = true,
        .handle = handle,
        .sequence = sequence,
        .start_tick = start_tick,
        .finish_tick = finish_tick,
        .app_class = @intFromEnum(instance.app_class),
        .role = @intFromEnum(instance.role),
        .exit_reason = exit_reason,
        .exit_code = exit_code,
    };
    exit_history_head = (exit_history_head + 1) % exit_history.len;
    if (exit_history_count < exit_history.len) exit_history_count += 1;
    return sequence;
}

const ProgramExitCommitResult = enum {
    committed,
    already_exiting,
    retry,
};

fn commitProgramExit(handle: ProgramProcessHandle, exit_code: i32, requested_reason: u8) ProgramExitCommitResult {
    if (consumeProgramLifecycleFailure(.exit_commit)) return .retry;
    const locked = lockProgramRegistry();
    if (!locked) return .retry;
    const slot = lookupProgramRegistryHandleLocked(handle, true) orelse {
        unlockProgramRegistry();
        return .retry;
    };
    if (slot.state == .exit or slot.state == .done or slot.state == .retire or slot.state == .reap) {
        unlockProgramRegistry();
        return .already_exiting;
    }
    if (!programRegistryStateIsRunning(slot.state)) {
        unlockProgramRegistry();
        return .retry;
    }

    slot.state = .exit;
    const instance = &slot.instance;
    const completion = slot.completion orelse {
        slot.state = .run;
        unlockProgramRegistry();
        return .retry;
    };
    const finish_tick = timer.tickCount();
    const exit_reason = if (requested_reason == PROGRAM_EXIT_REASON_NATURAL and instance.close_requested)
        PROGRAM_EXIT_REASON_CLOSE
    else
        requested_reason;
    instance.exit_code = exit_code;
    instance.done = true;
    completion.finish_tick = finish_tick;
    completion.exit_code = exit_code;
    completion.task_id = instance.task_id;
    completion.app_class = @intFromEnum(instance.app_class);
    completion.role = @intFromEnum(instance.role);
    completion.exit_reason = exit_reason;
    if (instance.display_used) completion.flags |= PROGRAM_COMPLETION_FLAG_DISPLAY_USED;
    if (instance.console_payload) |console| {
        completion.console_state = console.state;
        completion.output_revision = console.revision;
        completion.output_length = if (console.transcript_payload) |transcript| transcript.output_len else 0;
    }
    completion.sequence = appendExitHistoryLocked(handle, instance, exit_code, exit_reason, completion.start_tick, finish_tick);
    slot.state = .done;
    slot.state = .retire;
    bumpProgramInventoryEpochLocked();
    _ = enqueueProgramRetireLocked(slot);
    last_exit_code = exit_code;
    last_display_used = instance.display_used;
    if (foreground_instance_id != null and foreground_instance_id.? == handle.instance_id and foreground_instance_generation == handle.generation) {
        foreground_instance_id = null;
        foreground_instance_generation = 0;
    }
    const boot_shell_exited = shell_instance_id != null and shell_instance_id.? == handle.instance_id and shell_instance_generation == handle.generation;
    const exit_console = instance.console_payload;
    if (boot_shell_exited) {
        shell_instance_id = null;
        shell_instance_generation = 0;
    }
    unlockProgramRegistry();

    if (exit_console) |console| _ = console.input_wait.close(.cancelled);

    if (boot_shell_exited) boot_perf.failShellExited(handle.instance_id);

    orphanOwnedProgramCompletions(handle);
    terminateProgramThreadsForHandle(handle, -9, scheduler.currentId());
    return .committed;
}

fn beginProgramExit(handle: ProgramProcessHandle, exit_code: i32, requested_reason: u8) bool {
    const result = commitProgramExit(handle, exit_code, requested_reason);
    if (result == .retry) return false;
    // Console descendants are committed iteratively without recursively
    // nesting beginProgramExit on the kernel stack.
    killConsoleClients(handle);
    program_reaper_event.signal();
    return true;
}

fn orphanOwnedProgramCompletions(owner_handle: ProgramProcessHandle) void {
    while (true) {
        var reap_node: ?*ProgramCompletionNode = null;
        var changed = false;
        const locked = lockProgramRegistry();
        if (!locked) return;
        var current = program_completion_head;
        while (current) |node| : (current = node.next) {
            if (!node.owner or !programHandleEqual(node.owner_handle, owner_handle)) continue;
            node.owner = false;
            node.owner_handle = .{};
            node.flags &= ~PROGRAM_COMPLETION_FLAG_OWNER;
            changed = true;
            if (node.state == .ready) {
                _ = unlinkProgramCompletionLocked(node);
                node.state = .consumed;
                if (program_registry_stats.completion_ready != 0) program_registry_stats.completion_ready -= 1;
                reap_node = node;
            }
            break;
        }
        unlockProgramRegistry();
        if (reap_node) |node| {
            freeCompletionOutput(node);
            freeProgramCompletionNodeMemory(node);
        }
        if (!changed) return;
    }
}

fn abandonProgramCompletion(handle: ProgramProcessHandle) void {
    var consumed: ?*ProgramCompletionNode = null;
    const locked = lockProgramRegistry();
    if (!locked) return;
    if (completionForHandleLocked(handle)) |node| {
        node.owner = false;
        node.owner_handle = .{};
        node.flags &= ~PROGRAM_COMPLETION_FLAG_OWNER;
        if (node.state == .ready) {
            _ = unlinkProgramCompletionLocked(node);
            node.state = .consumed;
            if (program_registry_stats.completion_ready != 0) program_registry_stats.completion_ready -= 1;
            consumed = node;
        }
    }
    unlockProgramRegistry();
    if (consumed) |node| {
        freeCompletionOutput(node);
        freeProgramCompletionNodeMemory(node);
    }
}

fn markInstanceDone(handle: ProgramProcessHandle, exit_code: i32) void {
    while (!beginProgramExit(handle, exit_code, PROGRAM_EXIT_REASON_NATURAL)) scheduler.yield();
}

fn releaseInstance(id: u32) void {
    const handle = programHandleForId(id) orelse return;
    _ = queueProgramRetire(handle);
}

fn finishInstance(id: u32) i32 {
    const handle = programHandleForId(id) orelse return takeExitCode(id) orelse -1;
    var exit_code: i32 = 0;
    const locked = lockProgramRegistry();
    if (locked) {
        if (lookupProgramRegistryHandleLocked(handle, true)) |slot| exit_code = slot.instance.exit_code;
        unlockProgramRegistry();
    }
    _ = beginProgramExit(handle, exit_code, PROGRAM_EXIT_REASON_NATURAL);
    return exit_code;
}

fn consoleClientDescendsFromLocked(slot: *const ProgramRegistrySlot, host: ProgramProcessHandle) bool {
    var current_handle = programHandleForSlot(slot);
    const first_console = slot.instance.console_payload orelse return false;
    var parent_handle = ProgramProcessHandle{
        .instance_id = first_console.io_target_id,
        .reserved = 0,
        .generation = first_console.io_target_generation,
    };
    while (programHandleValid(parent_handle)) {
        if (programHandleEqual(parent_handle, host)) return true;
        // Console ownership is established only from an already published
        // parent, hence generations strictly decrease. This is both the
        // acyclic invariant and an unbounded-depth loop guard.
        if (parent_handle.generation >= current_handle.generation) return false;
        const parent_slot = lookupProgramRegistryHandleLocked(parent_handle, true) orelse return false;
        if (parent_slot.instance.app_class != .console) return false;
        const parent_console = parent_slot.instance.console_payload orelse return false;
        current_handle = parent_handle;
        parent_handle = .{
            .instance_id = parent_console.io_target_id,
            .reserved = 0,
            .generation = parent_console.io_target_generation,
        };
    }
    return false;
}

fn killConsoleClients(host: ProgramProcessHandle) void {
    while (true) {
        var client_handle: ?ProgramProcessHandle = null;
        const locked = lockProgramRegistry();
        if (!locked) return;
        var chunk = program_registry_head;
        while (chunk) |current| : (chunk = current.next) {
            for (&current.slots) |*slot| {
                if (!programRegistryStateIsRunning(slot.state) or slot.instance.app_class != .console) continue;
                if (!consoleClientDescendsFromLocked(slot, host)) continue;
                const candidate = programHandleForSlot(slot);
                // Child generations are strictly newer than every ancestor.
                // Selecting the newest descendant commits leaves before their
                // parents, so even an already-awake reaper can never erase an
                // ancestry payload that a later scan still needs.
                if (client_handle == null or candidate.generation > client_handle.?.generation) client_handle = candidate;
            }
        }
        unlockProgramRegistry();
        const child = client_handle orelse return;
        _ = commitProgramExit(child, -9, PROGRAM_EXIT_REASON_KILLED);
    }
}

fn requestConsoleClientsClose(host: ProgramProcessHandle) void {
    _ = host;
    while (true) {
        var changed = false;
        var changed_handle: ?ProgramProcessHandle = null;
        const locked = lockProgramRegistry();
        if (!locked) return;
        var chunk = program_registry_head;
        search: while (chunk) |current| : (chunk = current.next) {
            for (&current.slots) |*slot| {
                if (!programRegistryStateIsRunning(slot.state) or slot.instance.app_class != .console or slot.instance.close_requested) continue;
                const console = consolePayloadConst(&slot.instance);
                const parent_handle = ProgramProcessHandle{
                    .instance_id = console.io_target_id,
                    .reserved = 0,
                    .generation = console.io_target_generation,
                };
                const parent = lookupProgramRegistryHandleLocked(parent_handle, false) orelse continue;
                if (!parent.instance.close_requested) continue;
                slot.instance.close_requested = true;
                changed_handle = programHandleForSlot(slot);
                bumpProgramInventoryEpochLocked();
                changed = true;
                break :search;
            }
        }
        unlockProgramRegistry();
        if (changed_handle) |handle| signalConsoleInputForHandle(handle);
        if (!changed) return;
    }
}

fn reapFinishedInstances() void {
    program_reaper_event.signal();
}

fn reapHostedConsoleInstancesForPressure() void {
    var target: ?ProgramProcessHandle = null;
    const locked = lockProgramRegistry();
    if (locked) {
        var chunk = program_registry_head;
        search: while (chunk) |current| : (chunk = current.next) {
            for (&current.slots) |*slot| {
                if ((slot.state != .done and slot.state != .retire) or slot.instance.storage_teardown_blocked) continue;
                if (!isHostedConsoleInstance(&slot.instance)) continue;
                target = programHandleForSlot(slot);
                if (slot.state == .done) {
                    slot.state = .retire;
                    bumpProgramInventoryEpochLocked();
                    _ = enqueueProgramRetireLocked(slot);
                }
                break :search;
            }
        }
        unlockProgramRegistry();
    }
    if (target) |handle| {
        program_reaper_event.signal();
        const deadline = timer.deadlineAfterNow(@max(@as(u64, timer.frequency()), 100));
        while (true) {
            const check_locked = lockProgramRegistry();
            if (check_locked) {
                const heavy_released = lookupProgramRegistryHandleLocked(handle, true) == null;
                unlockProgramRegistry();
                if (heavy_released) break;
            }
            _ = runProgramReaperForTest();
            if (timer.tickCount() >= deadline) break;
            scheduler.yield();
        }
    }
    reclaimProgramRegistryUnderPressure();
}

fn isHostedConsoleInstance(instance: *const ProgramInstance) bool {
    return instance.role == .background and instance.app_class == .console and consolePayloadConst(instance).host != .none;
}

fn completionFromNode(node: *const ProgramCompletionNode) ProgramProcessCompletion {
    return .{
        .handle = node.handle,
        .sequence = node.sequence,
        .start_tick = node.start_tick,
        .finish_tick = node.finish_tick,
        .exit_code = node.exit_code,
        .task_id = node.task_id,
        .output_revision = node.output_revision,
        .output_length = node.output_length,
        .flags = node.flags,
        .app_class = node.app_class,
        .role = node.role,
        .exit_reason = node.exit_reason,
        .reserved0 = 0,
        .console_state = node.console_state,
    };
}

fn programCompletionIsReady(handle: ProgramProcessHandle) bool {
    const locked = lockProgramRegistry();
    if (!locked) return false;
    defer unlockProgramRegistry();
    const node = completionForHandleLocked(handle) orelse return false;
    return node.state == .ready;
}

fn readProgramCompletion(handle: ProgramProcessHandle, out: *ProgramProcessCompletion) i32 {
    const locked = lockProgramRegistry();
    if (!locked) return PROGRAM_HANDLE_ERROR_WOULD_BLOCK;
    defer unlockProgramRegistry();
    const node = completionForHandleLocked(handle) orelse return programHandleMissingStatusLocked(handle);
    if (node.state != .ready) return PROGRAM_HANDLE_ERROR_WOULD_BLOCK;
    out.* = completionFromNode(node);
    return PROGRAM_HANDLE_OK;
}

fn consumeProgramCompletion(handle: ProgramProcessHandle, out: ?*ProgramProcessCompletion) i32 {
    var consumed: ?*ProgramCompletionNode = null;
    const locked = lockProgramRegistry();
    if (!locked) return PROGRAM_HANDLE_ERROR_WOULD_BLOCK;
    const node = completionForHandleLocked(handle) orelse {
        const status = programHandleMissingStatusLocked(handle);
        unlockProgramRegistry();
        return status;
    };
    if (node.state != .ready) {
        unlockProgramRegistry();
        return PROGRAM_HANDLE_ERROR_WOULD_BLOCK;
    }
    if (out) |destination| destination.* = completionFromNode(node);
    _ = unlinkProgramCompletionLocked(node);
    node.state = .consumed;
    if (program_registry_stats.completion_ready != 0) program_registry_stats.completion_ready -= 1;
    consumed = node;
    unlockProgramRegistry();
    if (consumed) |finished| {
        freeCompletionOutput(finished);
        freeProgramCompletionNodeMemory(finished);
    }
    return PROGRAM_HANDLE_OK;
}

const ProgramRetireResult = enum {
    idle,
    completed,
    deferred,
};

fn deferProgramRetire(handle: ProgramProcessHandle) ProgramRetireResult {
    const locked = lockProgramRegistry();
    if (!locked) return .deferred;
    if (lookupProgramRegistryHandleLocked(handle, true)) |slot| {
        if (slot.state == .retire) {
            // Transfer exclusive ownership back to the queue atomically.  A
            // concurrent API nudge can therefore observe either a worker or a
            // queue owner, never both.
            slot.retire_in_progress = false;
            slot.retire_attempts +%= 1;
            program_registry_stats.retire_deferred +%= 1;
            program_registry_stats.retire_retries +%= 1;
            _ = enqueueProgramRetireLocked(slot);
        }
    }
    unlockProgramRegistry();
    if (program_lifecycle_failure_lock.lock(sync.WAIT_FOREVER)) {
        program_lifecycle_retried = true;
        _ = program_lifecycle_failure_lock.unlock();
    }
    return .deferred;
}

fn advanceProgramRetirePhase(
    slot: *ProgramRegistrySlot,
    handle: ProgramProcessHandle,
    completion: *ProgramCompletionNode,
    expected: ProgramRetirePhase,
    next: ProgramRetirePhase,
) bool {
    const locked = lockProgramRegistry();
    if (!locked) return false;
    defer unlockProgramRegistry();
    const current = lookupProgramRegistryHandleLocked(handle, true) orelse return false;
    // A hard-killed task keeps its execution pin until detach_task has
    // released the dead scheduler task and ProgramThread record. Therefore
    // cancel_execution may cross into detach_task with pins. Every later
    // boundary still requires zero pins, so external API leases cannot race
    // output, payload, image, stack, VM or slot teardown.
    const pins_must_be_drained = expected != .cancel_execution;
    if (current != slot or
        current.state != .retire or
        !current.retire_in_progress or
        (pins_must_be_drained and current.pin_count != 0) or
        current.completion != completion or
        current.retire_phase != expected) return false;
    current.retire_phase = next;
    return true;
}

fn reportBootForegroundRetireStage(enabled: bool, detail: []const u8) void {
    if (!enabled or !bootscreen.isActive()) return;
    bootscreen.setDetail(detail);
}

fn reportBootLaunchStage(enabled: bool, detail: []const u8) void {
    if (!enabled or !bootscreen.isActive()) return;
    bootscreen.setDetail(detail);
    bootlog.puts("[LAUNCH] shell stage=");
    bootlog.puts(detail);
    bootlog.puts("\r\n");
}

fn retireProgramSlot(slot: *ProgramRegistrySlot) ProgramRetireResult {
    var handle: ProgramProcessHandle = .{};
    var completion: *ProgramCompletionNode = undefined;
    var report_boot_foreground = false;
    var phase: ProgramRetirePhase = .cancel_execution;
    const locked = lockProgramRegistry();
    if (!locked) return .deferred;
    if (slot.state != .retire or
        !slot.retire_in_progress or
        slot.public_id == 0 or
        slot.generation == 0)
    {
        unlockProgramRegistry();
        return .idle;
    }
    handle = programHandleForSlot(slot);
    completion = slot.completion orelse {
        unlockProgramRegistry();
        return deferProgramRetire(handle);
    };
    report_boot_foreground = completion.role == @intFromEnum(InstanceRole.foreground);
    // Do not wait on the task's own execution pin here: detach_task is the
    // phase that can release it. advanceProgramRetirePhase gates the first
    // payload-owning boundary until that pin and every external lease are 0.
    phase = slot.retire_phase;
    unlockProgramRegistry();

    while (phase != .slot_reclaim) {
        switch (phase) {
            .cancel_execution => {
                reportBootForegroundRetireStage(report_boot_foreground, "SERVMAN: Abbruchpruefung");
                if (!cancelAsyncIoRequestsForHandle(handle)) return deferProgramRetire(handle);
                if (!advanceProgramRetirePhase(slot, handle, completion, .cancel_execution, .detach_task)) return deferProgramRetire(handle);
                if (consumeProgramLifecycleFailure(.cancel_execution)) return deferProgramRetire(handle);
                phase = .detach_task;
            },
            .detach_task => {
                reportBootForegroundRetireStage(report_boot_foreground, "SERVMAN: Taskabbau");
                terminateProgramThreadsForHandle(handle, -9, null);
                if (!releaseThreadsForHandleReporting(handle, report_boot_foreground)) return deferProgramRetire(handle);
                reportBootForegroundRetireStage(report_boot_foreground, "SERVMAN: I/O-Abbau");
                if (!purgeCancelledAsyncIoRequestsForHandle(handle)) return deferProgramRetire(handle);
                if (!releaseFileRangeLocksForHandle(handle)) return deferProgramRetire(handle);
                if (!r4api.r4sys.releaseStreamSlotsForProgram(handle.instance_id, handle.generation)) return deferProgramRetire(handle);
                if (!audio.closeStreamsForOwner(.{
                    .instance_id = handle.instance_id,
                    .generation = handle.generation,
                })) return deferProgramRetire(handle);
                if (!advanceProgramRetirePhase(slot, handle, completion, .detach_task, .output_detach)) return deferProgramRetire(handle);
                if (consumeProgramLifecycleFailure(.detach_task)) return deferProgramRetire(handle);
                phase = .output_detach;
            },
            .output_detach => {
                reportBootForegroundRetireStage(report_boot_foreground, "SERVMAN: Ausgabeabbau");
                if (!slot.retire_output_detached) {
                    if (!detachProgramCompletionOutput(&slot.instance, completion)) return deferProgramRetire(handle);
                    slot.retire_output_detached = true;
                }
                if (!advanceProgramRetirePhase(slot, handle, completion, .output_detach, .storage_release)) return deferProgramRetire(handle);
                if (consumeProgramLifecycleFailure(.output_detach)) return deferProgramRetire(handle);
                phase = .storage_release;
            },
            .storage_release => {
                reportBootForegroundRetireStage(report_boot_foreground, "SERVMAN: Kontextabbau");
                if (!slot.retire_storage_released) {
                    if (!releaseProgramInstanceStorage(&slot.instance, completion)) return deferProgramRetire(handle);
                    slot.retire_storage_released = true;
                }
                if (!advanceProgramRetirePhase(slot, handle, completion, .storage_release, .image_stack_vm_release)) return deferProgramRetire(handle);
                if (consumeProgramLifecycleFailure(.storage_release)) return deferProgramRetire(handle);
                phase = .image_stack_vm_release;
            },
            .image_stack_vm_release => {
                reportBootForegroundRetireStage(report_boot_foreground, "SERVMAN: Imageabbau");
                // First detach every core-owned address into this worker's
                // local snapshot.  The persisted phase is advanced only after
                // image, stack and residual owner teardown have all returned.
                if (!slot.retire_image_stack_released) {
                    var resources = programResourcesFromInstance(&slot.instance);
                    if (!cleanupProgramResources(&resources)) {
                        // The ProgramInstance remains the durable retry
                        // anchor. Preserve any partial-uncommit progress made
                        // in the mutable ProgramStack before deferring retire.
                        writeStackToInstance(&slot.instance, resources.stack);
                        return deferProgramRetire(handle);
                    }
                    slot.instance.used = false;
                    slot.instance.task_id = 0;
                    slot.instance.program_image_range_id = 0;
                    slot.instance.program_image_base = 0;
                    slot.instance.program_image_size = 0;
                    slot.instance.program_stack_range_id = 0;
                    slot.instance.program_stack_base = 0;
                    slot.instance.program_stack_reserve_size = 0;
                    slot.instance.program_stack_committed_base = 0;
                    slot.instance.program_stack_committed_size = 0;
                    slot.instance.program_stack_guard_base = 0;
                    slot.instance.program_stack_guard_size = 0;
                    slot.instance.program_stack_initial_commit_size = 0;
                    slot.instance.program_stack_create_cycles = 0;
                    slot.instance.program_stack_telemetry_high_water = 0;
                    slot.instance.program_stack_telemetry_committed_pages = 0;
                    slot.instance.program_stack_serial_telemetry = false;
                    slot.instance.program_stack_telemetry_measured = false;
                    slot.retire_image_stack_released = true;
                }
                if (!slot.retire_owner_released) {
                    _ = mem_virt.releaseOwner(.r4x_instance, handle.instance_id, .virtual_range);
                    _ = mem_backing_store.releaseR4xOwner(handle.instance_id);
                    slot.retire_owner_released = true;
                }
                if (!advanceProgramRetirePhase(slot, handle, completion, .image_stack_vm_release, .slot_reclaim)) return deferProgramRetire(handle);
                if (consumeProgramLifecycleFailure(.image_stack_vm_release)) return deferProgramRetire(handle);
                phase = .slot_reclaim;
            },
            .slot_reclaim => unreachable,
        }
    }

    // The final seam models a failed READY/slot-clear publication after all
    // destructive owner phases are durably complete.  Retrying this boundary
    // cannot revisit output, heap, image, stack or VM teardown.
    reportBootForegroundRetireStage(report_boot_foreground, "SERVMAN: Abschluss");
    if (consumeProgramLifecycleFailure(.slot_reclaim)) return deferProgramRetire(handle);

    var auto_consume: ?*ProgramCompletionNode = null;
    const reap_locked = lockProgramRegistry();
    if (!reap_locked) return deferProgramRetire(handle);
    const reap_slot = lookupProgramRegistryHandleLocked(handle, true) orelse {
        unlockProgramRegistry();
        return .idle;
    };
    if (reap_slot != slot or
        reap_slot.state != .retire or
        !reap_slot.retire_in_progress or
        reap_slot.retire_phase != .slot_reclaim or
        reap_slot.pin_count != 0 or
        reap_slot.completion != completion)
    {
        unlockProgramRegistry();
        return deferProgramRetire(handle);
    }
    reap_slot.state = .reap;
    reap_slot.completion = null;
    completion.slot_attached = false;
    completion.flags |= PROGRAM_COMPLETION_FLAG_READY;
    completion.state = .ready;
    if (program_registry_stats.completion_pending != 0) program_registry_stats.completion_pending -= 1;
    if (completion.owner) {
        program_registry_stats.completion_ready +%= 1;
    } else {
        _ = unlinkProgramCompletionLocked(completion);
        completion.state = .consumed;
        auto_consume = completion;
    }
    clearProgramRegistrySlotLocked(reap_slot);
    unlockProgramRegistry();

    reportBootForegroundRetireStage(report_boot_foreground, "SERVMAN: Bereit");

    if (auto_consume) |node| {
        freeCompletionOutput(node);
        freeProgramCompletionNodeMemory(node);
    }
    if (program_lifecycle_failure_lock.lock(sync.WAIT_FOREVER)) {
        if (program_lifecycle_failure_consumed) program_lifecycle_recovered = true;
        _ = program_lifecycle_failure_lock.unlock();
    }
    shrinkProgramRegistry(false);
    return .completed;
}

pub fn runProgramReaperForTest() bool {
    const locked = lockProgramRegistry();
    if (!locked) return false;
    const slot = takeProgramRetireLocked() orelse {
        unlockProgramRegistry();
        return false;
    };
    unlockProgramRegistry();
    return retireProgramSlot(slot) == .completed;
}

fn programReaperMain() callconv(.c) void {
    while (true) {
        const async_result = reapOnePendingAsyncIoClose();
        const locked = lockProgramRegistry();
        if (!locked) {
            scheduler.sleepTicksWithReason(1, "program_reaper_lock");
            continue;
        }
        const slot = takeProgramRetireLocked();
        unlockProgramRegistry();
        if (slot) |candidate| {
            if (retireProgramSlot(candidate) == .deferred) scheduler.sleepTicksWithReason(1, "program_reaper_retry");
            continue;
        }
        switch (async_result) {
            .completed => continue,
            .deferred => {
                scheduler.sleepTicksWithReason(1, "async_io_reaper_retry");
                continue;
            },
            .idle => {},
        }
        _ = program_reaper_event.waitResult(sync.WAIT_FOREVER);
    }
}

fn activeInstanceCount() u8 {
    var count: u8 = 0;
    var iterator = programRegistryIterator(false);
    while (iterator.next() != null) {
        if (count == std.math.maxInt(u8)) return count;
        count += 1;
    }
    return count;
}

fn instanceInfo(instance: *const ProgramInstance) ProgramInstanceInfo {
    var flags: u8 = 0;
    if (instance.close_requested) flags |= ProgramInstanceFlag.close_requested;
    if (instance.desktop_requested) flags |= ProgramInstanceFlag.desktop_requested;
    if (instance.console_payload) |console| {
        if (console.host == .terminal_mode) flags |= ProgramInstanceFlag.terminal_mode;
    }
    const usage = instanceAllVmStats(instance);
    return .{
        .id = instance.id,
        .task_id = instance.task_id,
        .role = @intFromEnum(instance.role),
        .app_class = @intFromEnum(instance.app_class),
        .state = @intFromEnum(instanceState(instance)),
        .flags = flags,
        .exit_code = instance.exit_code,
        .window_id = if (instance.gui_payload) |gui| gui.window_id else -1,
        .memory_profile = @intFromEnum(instance.memory_profile),
        .reserved0 = .{0} ** 3,
        .memory_reserved_limit = instance.memory_limits.vm_reserve_limit,
        .memory_committed_limit = instance.memory_limits.vm_commit_limit,
        .memory_resident_limit = instance.memory_limits.resident_limit,
        .memory_reserved_bytes = usage.reserved_bytes,
        .memory_committed_bytes = usage.committed_bytes,
        .memory_resident_bytes = usage.resident_bytes,
        .memory_peak_resident_bytes = usage.peak_resident_bytes,
        .stack_reserved_bytes = instance.program_stack_reserve_size,
        .stack_committed_bytes = instance.program_stack_committed_size,
        .memory_tag = instance.memory_tag,
    };
}

fn putSignedDec(value: i32) void {
    if (value < 0) {
        k.putc('-');
        k.putDec(@intCast(-value));
    } else {
        k.putDec(@intCast(value));
    }
}

fn instanceState(instance: *const ProgramInstance) InstanceState {
    if (instance.done) return .done;
    if (instance.close_requested) return .close_requested;
    return .running;
}

/// Pfad der eigenen Moduldatei in einen caller-owned Buffer. Rueckgabe ist
/// die Pfadlaenge oder negativ: -1 Argumente, -2 kein Programmkontext,
/// -3 Buffer zu klein (sichtbar statt still trunciert), -4 Pfad unbekannt.
fn apiProgramModulePath(out_ptr: [*]u8, out_len: u32) callconv(.c) i32 {
    if (out_len == 0) return -1;
    const instance = currentInstance() orelse return -2;
    const runtime = runtimePayload(instance);
    const len: usize = @intCast(runtime.module_path_len);
    if (len == 0) return -4;
    if (len > out_len) return -3;
    @memcpy(out_ptr[0..len], runtime.module_path[0..len]);
    return @intCast(len);
}

fn apiBootReady() callconv(.c) i32 {
    comptime {
        if (boot_perf.ready_result_completed != r4x_api.boot_ready_result_completed or
            boot_perf.ready_result_already_completed != r4x_api.boot_ready_result_already_completed or
            boot_perf.ready_error_not_expected_shell != r4x_api.boot_ready_error_not_boot_shell or
            boot_perf.ready_error_boot_failed != r4x_api.boot_ready_error_boot_failed)
        {
            @compileError("boot readiness result contract drift");
        }
    }
    const instance = currentInstance() orelse return boot_perf.ready_error_not_expected_shell;
    if (instance.role != .shell) return boot_perf.ready_error_not_expected_shell;
    const result = boot_perf.completeReady(instance.id);
    if (result == boot_perf.ready_result_completed) {
        bootscreen.completeForHandoff();
        boot_status.releaseForUserSession();
    }
    return result;
}

fn currentInstance() ?*ProgramInstance {
    const id = scheduler.currentId() orelse return null;
    if (currentExecutionInstanceNoRegistry()) |instance| return instance;
    var iterator = programRegistryIterator(true);
    while (iterator.next()) |instance| {
        if (instance.task_id == id) return instance;
    }
    return null;
}

// IRQ/exception-safe current-owner lookup.  Program registry chunks keep
// ProgramInstance addresses stable, and teardown clears task/request records
// before reclaiming their owner slot.  This helper therefore needs neither a
// registry pin nor a sleepable mutex and must remain free of waits/yields.
fn currentExecutionInstanceNoRegistry() ?*ProgramInstance {
    if (currentProgramThread()) |thread_ctx| return thread_ctx.owner_instance;
    if (currentAsyncIoRequest()) |request| return request.owner_instance;
    return null;
}

fn currentProgramHandle() ?ProgramProcessHandle {
    if (currentProgramThread()) |thread_ctx| {
        const handle = ProgramProcessHandle{
            .instance_id = thread_ctx.instance_id,
            .reserved = 0,
            .generation = thread_ctx.instance_generation,
        };
        return if (programHandleValid(handle)) handle else null;
    }
    if (currentAsyncIoRequest()) |request| {
        const handle = ProgramProcessHandle{
            .instance_id = request.owner_instance_id,
            .reserved = 0,
            .generation = request.owner_instance_generation,
        };
        return if (programHandleValid(handle)) handle else null;
    }
    return null;
}

fn resolveR4SysStreamOwner() ?r4api.r4sys.StreamOwner {
    // Most file-stream calls run in short-lived async-I/O workers, but
    // fileReplaceAtomic is an existing direct R4SYS slot and therefore runs
    // in the caller's ProgramThread. Both execution forms must resolve to the
    // same stable process handle or a finished stage can never be published.
    if (currentProgramThread()) |thread_ctx| {
        if (thread_ctx.instance_id == 0 or
            thread_ctx.instance_generation == 0 or
            thread_ctx.task_id == 0 or
            thread_ctx.task_generation == 0)
            return null;
        return .{
            .kind = .program,
            .id = thread_ctx.task_id,
            .generation = thread_ctx.task_generation,
            .program_id = thread_ctx.instance_id,
            .program_generation = thread_ctx.instance_generation,
        };
    }
    const request = currentAsyncIoRequest() orelse return null;
    if (request.owner_instance_id == 0 or
        request.owner_instance_generation == 0 or
        request.caller_task_id == 0 or
        request.caller_task_generation == 0)
        return null;
    return .{
        .kind = .program,
        .id = request.caller_task_id,
        .generation = request.caller_task_generation,
        .program_id = request.owner_instance_id,
        .program_generation = request.owner_instance_generation,
    };
}

pub fn isPreemptibleInstructionPointer(rip: u64) bool {
    const instance = currentExecutionInstanceNoRegistry() orelse return false;
    return instructionPointerInInstance(instance, rip);
}

pub fn crashContextForInstructionPointer(rip: u64) crash.ExecutionContext {
    const task_id = scheduler.currentId() orelse 0;
    if (currentExecutionInstanceNoRegistry()) |instance| return crashContextFromInstance(instance, task_id, rip);

    // Crash reporting cannot acquire the registry mutex either.  Every live
    // program has at least its main ProgramThread record until teardown, so
    // the stable intrusive thread registry is a complete lock-free image
    // owner index without taking the sleepable ProgramRegistry lock.
    var cursor = program_thread_head;
    while (cursor) |thread_ctx| : (cursor = thread_ctx.registry_next) {
        if (!thread_ctx.used) continue;
        const instance = thread_ctx.owner_instance orelse continue;
        if (!instructionPointerInInstance(instance, rip)) continue;
        return crashContextFromInstance(instance, task_id, rip);
    }
    return crash.unavailableContext();
}

fn instructionPointerInInstance(instance: *const ProgramInstance, rip: u64) bool {
    const base = instance.program_image_base;
    const size: u64 = @intCast(instance.program_image_size);
    if (base == 0 or size == 0) return false;
    const end = base +% size;
    if (end < base) return false;
    return rip >= base and rip < end;
}

fn crashContextFromInstance(instance: *const ProgramInstance, task_id: u32, rip: u64) crash.ExecutionContext {
    const base = instance.program_image_base;
    const size: u64 = @intCast(instance.program_image_size);
    const offset = if (instructionPointerInInstance(instance, rip)) rip - base else 0;
    return crash.programContextInfo(instance.id, task_id, instance.id, base, size, offset, instance.memory_tag);
}

fn instanceById(id: u32) ?*ProgramInstance {
    const locked = lockProgramRegistry();
    if (!locked) return null;
    defer unlockProgramRegistry();
    const slot = lookupProgramRegistrySlotLocked(id, false) orelse return null;
    return &slot.instance;
}

fn copyArgsInto(instance: *ProgramInstance, args: []const u8) void {
    const process = processPayload(instance);
    @memset(process.args[0..], 0);
    const count = if (args.len > ARGS_MAX) ARGS_MAX else args.len;
    if (count > 0) @memcpy(process.args[0..count], args[0..count]);
    process.args[count] = 0;
}

const EnvironmentEntry = struct {
    start: usize,
    value_start: usize,
    end: usize,
};

fn inheritEnvironmentInto(instance: *ProgramInstance, parent: ?*const ProgramInstance) void {
    const process = processPayload(instance);
    process.environment_len = 0;
    const source = parent orelse return;
    const source_process = processPayloadConst(source);
    if (source_process.environment_len == 0) return;
    const destination = process.environment_payload orelse return;
    const source_environment = source_process.environment_payload orelse return;
    @memset(destination.bytes[0..], 0);
    const count = @min(source_process.environment_len, destination.bytes.len);
    @memcpy(destination.bytes[0..count], source_environment.bytes[0..count]);
    process.environment_len = count;
}

fn environmentGet(instance: *const ProgramInstance, name: []const u8, out_ptr: [*]u8, capacity: u32) i32 {
    const process = processPayloadConst(instance);
    const environment = process.environment_payload orelse return ENV_ERROR_NOT_FOUND;
    const entry = findEnvironmentEntry(instance, name) orelse return ENV_ERROR_NOT_FOUND;
    const value = environment.bytes[entry.value_start..entry.end];
    const out_capacity: usize = @intCast(capacity);
    if (out_capacity < value.len) return ENV_ERROR_BUFFER_TOO_SMALL;
    if (value.len != 0) @memcpy(out_ptr[0..value.len], value);
    if (out_capacity > value.len) out_ptr[value.len] = 0;
    return @intCast(value.len);
}

fn environmentSet(instance: *ProgramInstance, name: []const u8, value: []const u8) i32 {
    if (name.len > ENVIRONMENT_NAME_MAX or value.len > ENVIRONMENT_VALUE_MAX) return ENV_ERROR_TOO_LONG;
    const process = processPayload(instance);
    const existing = findEnvironmentEntry(instance, name);
    const entry_len = name.len + 1 + value.len + 1;
    const replaced_len = if (existing) |entry| entry.end - entry.start + 1 else 0;
    const retained_len = process.environment_len - replaced_len;
    if (entry_len > ENVIRONMENT_BLOCK_MAX or retained_len > ENVIRONMENT_BLOCK_MAX - entry_len) return ENV_ERROR_TOO_LONG;

    const environment = ensureEnvironmentPayload(instance) orelse return ENV_ERROR_NO_MEMORY;
    if (existing) |entry| removeEnvironmentEntry(instance, entry);

    var pos = process.environment_len;
    @memcpy(environment.bytes[pos .. pos + name.len], name);
    pos += name.len;
    environment.bytes[pos] = '=';
    pos += 1;
    if (value.len != 0) @memcpy(environment.bytes[pos .. pos + value.len], value);
    pos += value.len;
    environment.bytes[pos] = 0;
    pos += 1;
    process.environment_len = pos;
    return ENV_OK;
}

fn removeEnvironmentEntry(instance: *ProgramInstance, entry: EnvironmentEntry) void {
    const process = processPayload(instance);
    const environment = process.environment_payload orelse return;
    if (entry.end >= process.environment_len) {
        process.environment_len = entry.start;
        @memset(environment.bytes[process.environment_len..], 0);
        return;
    }
    const next = entry.end + 1;
    const tail_len = process.environment_len - next;
    var i: usize = 0;
    while (i < tail_len) : (i += 1) {
        environment.bytes[entry.start + i] = environment.bytes[next + i];
    }
    process.environment_len -= next - entry.start;
    @memset(environment.bytes[process.environment_len..], 0);
}

fn findEnvironmentEntry(instance: *const ProgramInstance, name: []const u8) ?EnvironmentEntry {
    const process = processPayloadConst(instance);
    const environment = process.environment_payload orelse return null;
    var start: usize = 0;
    while (start < process.environment_len) {
        var end = start;
        while (end < process.environment_len and environment.bytes[end] != 0) : (end += 1) {}
        if (end == start) {
            start += 1;
            continue;
        }
        var eq = start;
        while (eq < end and environment.bytes[eq] != '=') : (eq += 1) {}
        if (eq < end and equalsIgnoreCase(environment.bytes[start..eq], name)) {
            return .{ .start = start, .value_start = eq + 1, .end = end };
        }
        start = end + 1;
    }
    return null;
}

fn environmentNameValid(name: []const u8) bool {
    if (name.len == 0 or name.len > ENVIRONMENT_NAME_MAX) return false;
    var i: usize = 0;
    while (i < name.len) : (i += 1) {
        const ch = name[i];
        if (ch <= ' ' or ch == '=' or ch == 0) return false;
    }
    return true;
}

fn copySliceZ(out: []u8, value: []const u8) void {
    @memset(out, 0);
    if (out.len == 0) return;
    const count = @min(value.len, out.len - 1);
    if (count > 0) @memcpy(out[0..count], value[0..count]);
}

fn copyWorkingDirectoryInto(instance: *ProgramInstance, d: *drive.Drive) void {
    const process = processPayload(instance);
    process.work_drive_letter = d.letter;
    @memset(process.work_cwd[0..], 0);
    process.work_cwd_len = d.cwd_len;
    if (process.work_cwd_len == 0 or process.work_cwd_len > process.work_cwd.len) {
        process.work_cwd[0] = '\\';
        process.work_cwd_len = 1;
        return;
    }
    @memcpy(process.work_cwd[0..process.work_cwd_len], d.cwd[0..process.work_cwd_len]);
}

fn markDisplayUsed() void {
    r4api.r4draw.markDisplayUsed();
}

fn noteDisplayUsed() void {
    last_display_used = true;
    if (currentInstance()) |instance| instance.display_used = true;
}

const ApiTarget = r4api.r4sys.Target;

fn resolveApiTarget(raw: []const u8, out: *[MAX_API_PATH]u8) ?ApiTarget {
    if (raw.len == 0) return null;

    const ctx = currentInstance();
    const current_drive = if (ctx) |instance| processPayloadConst(instance).work_drive_letter else work_drive_letter;
    var d = drive.get(current_drive) orelse return null;
    var rest = raw;
    if (raw.len >= 2 and raw[1] == ':' and isAlpha(raw[0])) {
        d = drive.get(raw[0]) orelse return null;
        rest = raw[2..];
    }

    const cwd = if (ctx) |instance| blk: {
        const process = processPayloadConst(instance);
        break :blk if (d.letter == process.work_drive_letter) process.work_cwd[0..process.work_cwd_len] else d.cwd[0..d.cwd_len];
    } else if (d.letter == work_drive_letter) work_cwd[0..work_cwd_len] else d.cwd[0..d.cwd_len];
    const path = buildApiPath(cwd, rest, out) orelse return null;
    return .{ .drive_ref = d, .path = path };
}

fn buildApiPath(cwd: []const u8, arg: []const u8, out: *[MAX_API_PATH]u8) ?[]const u8 {
    var combined: [MAX_API_PATH]u8 = undefined;
    var len: usize = 0;
    if (arg.len > 0 and (arg[0] == '\\' or arg[0] == '/')) {
        if (arg.len > combined.len) return null;
        @memcpy(combined[0..arg.len], arg);
        len = arg.len;
    } else {
        if (cwd.len > combined.len) return null;
        @memcpy(combined[0..cwd.len], cwd);
        len = cwd.len;
        if (len > 1) {
            if (len >= combined.len) return null;
            combined[len] = '\\';
            len += 1;
        }
        if (len + arg.len > combined.len) return null;
        @memcpy(combined[len .. len + arg.len], arg);
        len += arg.len;
    }
    return canonicalizeApiVolumePath(combined[0..len], out);
}

/// Produces one volume-relative spelling for R4SYS ownership and ancestor
/// checks. The filesystem may accept '.', '..' and mixed separators, but
/// retaining those raw bytes let the same directory evade StreamSlot guards.
fn canonicalizeApiVolumePath(input: []const u8, out: *[MAX_API_PATH]u8) ?[]const u8 {
    if (input.len == 0 or (input[0] != '\\' and input[0] != '/')) return null;
    out[0] = '\\';
    var out_len: usize = 1;
    var segment_starts: [160]usize = undefined;
    var segment_count: usize = 0;
    var pos: usize = 0;
    while (pos < input.len) {
        while (pos < input.len and (input[pos] == '\\' or input[pos] == '/')) : (pos += 1) {}
        if (pos >= input.len) break;
        const start = pos;
        while (pos < input.len and input[pos] != '\\' and input[pos] != '/') : (pos += 1) {}
        const segment = input[start..pos];
        if (std.mem.eql(u8, segment, ".")) continue;
        if (std.mem.eql(u8, segment, "..")) {
            if (segment_count == 0) return null;
            segment_count -= 1;
            out_len = segment_starts[segment_count];
            continue;
        }
        if (segment_count >= segment_starts.len) return null;
        const before = out_len;
        if (out_len > 1) {
            if (out_len >= out.len) return null;
            out[out_len] = '\\';
            out_len += 1;
        }
        if (segment.len > out.len - out_len) return null;
        segment_starts[segment_count] = before;
        segment_count += 1;
        @memcpy(out[out_len .. out_len + segment.len], segment);
        out_len += segment.len;
    }
    return out[0..out_len];
}

fn stdMemEql(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var i: usize = 0;
    while (i < a.len) : (i += 1) {
        if (a[i] != b[i]) return false;
    }
    return true;
}

fn startsWith(s: []const u8, prefix: []const u8) bool {
    return s.len >= prefix.len and stdMemEql(s[0..prefix.len], prefix);
}

fn trimAscii(s: []const u8) []const u8 {
    var start: usize = 0;
    var end = s.len;
    while (start < end and (s[start] == ' ' or s[start] == '\t')) : (start += 1) {}
    while (end > start and (s[end - 1] == ' ' or s[end - 1] == '\t')) : (end -= 1) {}
    return s[start..end];
}

fn startsWithIgnoreCase(s: []const u8, prefix: []const u8) bool {
    return s.len >= prefix.len and equalsIgnoreCase(s[0..prefix.len], prefix);
}

fn equalsIgnoreCase(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var i: usize = 0;
    while (i < a.len) : (i += 1) {
        if (upper(a[i]) != upper(b[i])) return false;
    }
    return true;
}

fn fixedZContainsIgnoreCase(haystack_z: []const u8, needle: []const u8) bool {
    const haystack = fixedZSpan(haystack_z);
    if (needle.len == 0) return true;
    if (haystack.len < needle.len) return false;
    var i: usize = 0;
    while (i + needle.len <= haystack.len) : (i += 1) {
        if (equalsIgnoreCase(haystack[i .. i + needle.len], needle)) return true;
    }
    return false;
}

fn fixedZSpan(buffer: []const u8) []const u8 {
    var len: usize = 0;
    while (len < buffer.len and buffer[len] != 0) : (len += 1) {}
    return buffer[0..len];
}

fn indexOfByte(s: []const u8, ch: u8) ?usize {
    var i: usize = 0;
    while (i < s.len) : (i += 1) {
        if (s[i] == ch) return i;
    }
    return null;
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

fn stripDrive(path: []const u8) []const u8 {
    if (path.len >= 2 and path[1] == ':') return path[2..];
    return path;
}

fn isAlpha(c: u8) bool {
    const u = upper(c);
    return u >= 'A' and u <= 'Z';
}

fn upper(c: u8) u8 {
    if (c >= 'a' and c <= 'z') return c - ('a' - 'A');
    return c;
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

fn joinPathZ(parent: []const u8, name: []const u8, out: []u8) bool {
    const needs_sep = parent.len > 0 and parent[parent.len - 1] != '\\' and parent[parent.len - 1] != '/';
    const sep_len: usize = if (needs_sep) 1 else 0;
    if (parent.len + sep_len + name.len + 1 > out.len) return false;
    @memcpy(out[0..parent.len], parent);
    var pos = parent.len;
    if (needs_sep) {
        out[pos] = '\\';
        pos += 1;
    }
    @memcpy(out[pos .. pos + name.len], name);
    pos += name.len;
    out[pos] = 0;
    return true;
}

fn copyPathZ(path: []const u8, out: []u8) bool {
    if (path.len + 1 > out.len) return false;
    @memcpy(out[0..path.len], path);
    out[path.len] = 0;
    return true;
}

fn parentPathZ(path: []const u8, out: []u8) bool {
    if (path.len <= 3 and path.len >= 2 and path[1] == ':') return copyPathZ(path, out);
    var end = path.len;
    while (end > 0 and (path[end - 1] == '\\' or path[end - 1] == '/')) : (end -= 1) {}
    var i = end;
    while (i > 0) : (i -= 1) {
        if (path[i - 1] == '\\' or path[i - 1] == '/') {
            const keep = if (i == 3 and path.len >= 2 and path[1] == ':') 3 else i - 1;
            return copyPathZ(path[0..keep], out);
        }
    }
    return copyPathZ(path, out);
}

fn readLe16(bytes: []const u8) u16 {
    return @as(u16, bytes[0]) | (@as(u16, bytes[1]) << 8);
}

fn readLe32(bytes: []const u8) u32 {
    return @as(u32, bytes[0]) |
        (@as(u32, bytes[1]) << 8) |
        (@as(u32, bytes[2]) << 16) |
        (@as(u32, bytes[3]) << 24);
}

fn readLe64(bytes: []const u8) u64 {
    return @as(u64, bytes[0]) |
        (@as(u64, bytes[1]) << 8) |
        (@as(u64, bytes[2]) << 16) |
        (@as(u64, bytes[3]) << 24) |
        (@as(u64, bytes[4]) << 32) |
        (@as(u64, bytes[5]) << 40) |
        (@as(u64, bytes[6]) << 48) |
        (@as(u64, bytes[7]) << 56);
}

fn writeLe16(bytes: []u8, value: u16) void {
    bytes[0] = @intCast(value & 0xff);
    bytes[1] = @intCast((value >> 8) & 0xff);
}

fn writeLe32(bytes: []u8, value: u32) void {
    bytes[0] = @intCast(value & 0xff);
    bytes[1] = @intCast((value >> 8) & 0xff);
    bytes[2] = @intCast((value >> 16) & 0xff);
    bytes[3] = @intCast((value >> 24) & 0xff);
}

fn writeLe64(bytes: []u8, value: u64) void {
    bytes[0] = @intCast(value & 0xff);
    bytes[1] = @intCast((value >> 8) & 0xff);
    bytes[2] = @intCast((value >> 16) & 0xff);
    bytes[3] = @intCast((value >> 24) & 0xff);
    bytes[4] = @intCast((value >> 32) & 0xff);
    bytes[5] = @intCast((value >> 40) & 0xff);
    bytes[6] = @intCast((value >> 48) & 0xff);
    bytes[7] = @intCast((value >> 56) & 0xff);
}
