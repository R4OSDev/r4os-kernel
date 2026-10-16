const native_tasks = @import("../sched/task.zig");
const keyboard = @import("../driver/input/keyboard.zig");
const config = @import("config");
const audio = @import("../audio/core.zig");
const font = @import("../kernel/font.zig");
const drive = @import("../fs/drive.zig");
const vfs = @import("../fs/vfs.zig");
const fs_request = @import("../fs/request.zig");
const paging = @import("../memory/paging.zig");
const percpu = @import("../arch/x86_64/percpu.zig");
const heap = @import("../memory/heap.zig");
const mem_blocks = @import("../memory/blocks.zig");
const mem_backing_store = @import("../memory/backing_store.zig");
const mem_virt = @import("../memory/virt.zig");
const gfx_buffers = @import("../memory/gfx_buffers.zig");
const gfx_buffer_api = @import("gfx_buffer_api.zig");
const gfx_queue = @import("../display/queue.zig");
const gfx_queue_api = @import("gfx_queue_api.zig");
const gfx_allocation_api = @import("gfx_allocation_api.zig");
const gfx_allocations = @import("../kernel/gfx_allocations.zig");
const notifications = @import("notifications.zig");
const local_context = @import("local_context.zig");
const gfx_output_api = @import("gfx_output_api.zig");
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
const monotonic = @import("../platform/monotonic.zig");
const timer = @import("../kernel/timer.zig");
const owner_locks = @import("../memory/owner_locks.zig");
const scheduler = @import("../sched/scheduler.zig");
const sync = @import("../sched/sync.zig");
const task = @import("../sched/task.zig");
const task_context = @import("../sched/task_context.zig");
const r4api = @import("r4api.zig");
const r4x_api = @import("r4x_api.zig");
const r4x_start = @import("r4x_start.zig");
const execution_policy = @import("execution_policy.zig");
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
// Immutable GUI snapshots and unpublished clones may be copied cooperatively.
// Keep the boundary equal to the normal resource-node target so large legacy
// blobs cannot monopolize a CPU while no frame-state or allocator lock is held.
const GUI_COPY_RESCHEDULE_BYTES: usize = 64 * 1024;
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
const GuiFrameStreamInfo = r4x_api.GuiFrameStreamInfo;
const GuiIndexed8Resource = r4x_api.GuiIndexed8Resource;
const GuiXrgb32Resource = r4x_api.GuiXrgb32Resource;
const GuiSharedRasterHandle = r4x_api.GuiSharedRasterHandle;
const GuiSharedRasterCreateInfo = r4x_api.GuiSharedRasterCreateInfo;
const GuiSharedRasterWriteMap = r4x_api.GuiSharedRasterWriteMap;
const GuiSharedRasterLease = r4x_api.GuiSharedRasterLease;
const GuiSharedRasterMap = r4x_api.GuiSharedRasterMap;
const GuiSharedRasterResource = r4x_api.GuiSharedRasterResource;
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

const R4LCodeBinding = extern struct {
    generation: u32 = 0,
    module_slot: u8 = 0,
    reserved: [3]u8 = .{0} ** 3,
};

const R4XStartImportSeed = extern struct {
    group_id: u32 = 0,
    min_version: u32 = 0,
    resolved_version: u32 = 0,
    flags: u32 = 0,
    table: u64 = 0,
    r4l_binding_valid: bool = false,
    r4l_module_slot: u8 = 0,
    r4l_generation: u32 = 0,
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
    parallel_execution: bool = false,
    desktop_host: bool = false,
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
pub const stack_profile_count: u32 = MEMORY_PROFILE_COUNT;
const PROGRAM_STACK_HIGH_WATER_PATTERN: u8 = 0xA5;

const ProgramStackTelemetryStats = extern struct {
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

const ProgramStack = extern struct {
    base: u64 = 0,
    reserve_size: u64 = 0,
    committed_base: u64 = 0,
    committed_size: u64 = 0,
    guard_base: u64 = 0,
    guard_size: u64 = 0,
    top: u64 = 0,
    initial_commit_size: u64 = 0,
    create_cycles: u64 = 0,
    telemetry_high_water: u64 = 0,
    range_id: u32 = 0,
    owner_id: u32 = 0,
    telemetry_committed_pages: u32 = 0,
    profile: MemoryProfile = .unknown,
    serial_telemetry: bool = false,
    telemetry_measured: bool = false,
};

const ProgramResources = extern struct {
    image_base: u64 = 0,
    image_size: usize = 0,
    stack: ProgramStack = .{},
    image_range_id: u32 = 0,
    image_owner_id: u32 = 0,
};

const RawEntryFn = *const fn (usize) callconv(.c) i32;

extern fn r4os_call_program(entry: RawEntryFn, arg: usize, stack_top: u64) callconv(.c) i32;

const AppClass = enum(u8) {
    console,
    gui,
    service,
};

const ProgramMemoryLimits = extern struct {
    vm_reserve_limit: u64 = 1024 * MB,
    vm_commit_limit: u64 = 256 * MB,
    resident_limit: u64 = 256 * MB,
    stack_reserve: u64 = PROGRAM_STACK_RESERVE_SIZE,
    stack_initial_commit: u64 = PROGRAM_STACK_INITIAL_COMMIT_SIZE,
};

const ProgramMemoryContract = extern struct {
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

const NativeOptionalProgramInstanceLease = extern struct { value: ProgramInstanceLease = undefined, present: bool = false };
const ProgramThread = extern struct {
    console_input_lease: NativeOptionalProgramInstanceLease = .{},
    registry_next: ?*ProgramThread = null,
    join_queue: sync.WaitQueue = sync.WaitQueue.init(),
    created_tick: u64 = 0,
    arg: usize = 0,
    join_owner_task_generation: u64 = 0,
    instance_generation: u64 = 0,
    entry: RawEntryFn = undefined,
    stack: ProgramStack = .{},
    task_generation: u64 = 0,
    generation: u64 = 0,
    owner_instance: ?*ProgramInstance = null,
    registry_prev: ?*ProgramThread = null,
    finished_tick: u64 = 0,
    join_owner_task_id: u32 = 0,
    spawn_transaction_depth: u32 = 0,
    exit_code: i32 = 0,
    instance_id: u32 = 0,
    task_id: u32 = 0,
    id: u32 = 0,
    state: ProgramThreadState = .unused,
    flags: u32 = 0,
    join_count: u32 = 0,
    join_waiter_refs: u32 = 0,
    pin_count: u32 = 0,
    stack_released: bool = false,
    exit_deferred: bool = false,
    execution_pinned: bool = false,
    task_detached: bool = false,
    retire_for_instance: bool = false,
    retire_in_progress: bool = false,
    join_lease_active: bool = false,
    retire_pending: bool = false,
    used: bool = false,
};







const MAX_FILE_RANGE_LOCKS: usize = 64;

// Eight resident stacks serve the existing 64 request slots. Four file
// workers remain available even while all four service workers await a
// reply. All workers retain the original BSP placement and caller identity.
const ASYNC_IO_WORKER_COUNT: usize = 8;






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

const ProgramPayloadHeader = extern struct {
    magic: u32 = 0,
    owner_id: u32 = 0,
    requested_bytes: u32 = 0,
    kind: ProgramPayloadKind = .runtime,
    reserved: u16 = 0,
};

const ProgramRuntimePayload = extern struct {
    local_context: local_context.Owner = .{},
    r4xstart_context: R4XStartContext = .{},
    r4xstart_imports: [MAX_R4M_IMPORTS]R4XStartImport = .{R4XStartImport{}} ** MAX_R4M_IMPORTS,
    header: ProgramPayloadHeader = .{},
    r4xstart_import_count: u32 = 0,
    r4l_code_bindings: [MAX_R4M_IMPORTS]R4LCodeBinding = .{R4LCodeBinding{}} ** MAX_R4M_IMPORTS,
    module_path_len: u32 = 0,
    r4xstart_import_module_names: [MAX_R4M_IMPORTS][R4XSTART_IMPORT_NAME_BYTES]u8 = .{.{0} ** R4XSTART_IMPORT_NAME_BYTES} ** MAX_R4M_IMPORTS,
    r4xstart_import_symbol_names: [MAX_R4M_IMPORTS][R4XSTART_IMPORT_NAME_BYTES]u8 = .{.{0} ** R4XSTART_IMPORT_NAME_BYTES} ** MAX_R4M_IMPORTS,
    r4l_code_binding_count: u8 = 0,
    module_path: [MODULE_ORIGIN_MAX]u8 = .{0} ** MODULE_ORIGIN_MAX,
};

const ProgramEnvironmentPayload = extern struct {
    header: ProgramPayloadHeader = .{},
    bytes: [ENVIRONMENT_BLOCK_MAX]u8 = .{0} ** ENVIRONMENT_BLOCK_MAX,
};

const ProgramProcessPayload = extern struct {
    environment_payload: ?*ProgramEnvironmentPayload = null,
    environment_len: usize = 0,
    work_cwd_len: usize = 1,
    header: ProgramPayloadHeader = .{},
    args: [ARGS_MAX + 1]u8 = .{0} ** (ARGS_MAX + 1),
    work_drive_letter: u8 = 'C',
    work_cwd: [drive.MAX_PATH]u8 = .{0} ** drive.MAX_PATH,
};

const ProgramConsoleOutputPayload = extern struct {
    next_sequence: u64 = 0,
    header: ProgramPayloadHeader = .{},
    ref_count: u32 = 0,
    active_ref_count: u32 = 0,
    active_accounted: bool = false,
    reserved: [7]u8 = .{0} ** 7,
    bytes: [CONSOLE_OUTPUT_SIZE]u8 = .{0} ** CONSOLE_OUTPUT_SIZE,
};

const ProgramConsoleOutputSegment = extern struct {
    payload: ?*ProgramConsoleOutputPayload = null,
    start_sequence: u64 = 0,
    length: u32 = 0,
    reserved: u32 = 0,
};

const ProgramConsoleTranscriptPayload = extern struct {
    segments: [CONSOLE_TRANSCRIPT_SEGMENTS]ProgramConsoleOutputSegment = .{ProgramConsoleOutputSegment{}} ** CONSOLE_TRANSCRIPT_SEGMENTS,
    header: ProgramPayloadHeader = .{},
    segment_count: u32 = 0,
    output_len: u32 = 0,
};

const ProgramConsolePayload = extern struct {
    transcript_payload: ?*ProgramConsoleTranscriptPayload = null,
    writer_payload: ?*ProgramConsoleOutputPayload = null,
    input_head: usize = 0,
    input_tail: usize = 0,
    input_generation: u64 = 0,
    input_lock: sync.Mutex = sync.Mutex.initClass("console-input", sync.LockRank.program_instances, .sleepable),
    input_wait: sync.WaitQueue = sync.WaitQueue.init(),
    io_target_generation: u64 = 0,
    header: ProgramPayloadHeader = .{},
    input_high_water: u32 = 0,
    revision: u32 = 0,
    host: ConsoleHostKind = .none,
    io_target_id: u32 = 0,
    state: ConsoleState = .{},
    input_queue: [INPUT_QUEUE_SIZE]u8 = .{0} ** INPUT_QUEUE_SIZE,
};

const ProgramGuiCommandResourceKind = enum(u16) {
    none = 0,
    utf8 = 1,
    xrgb32 = 2,
    alpha8 = 3,
    path = 4,
    indexed8 = 5,
    xrgb32_nearest = 6,
    shared_raster = 7,
};

const ProgramGuiSharedRasterRef = extern struct {
    handle: GuiSharedRasterHandle = .{},
    raster_generation: u64 = 0,
    data_bytes: u64 = 0,
};

// Kernel-owned display-list truth.  The legacy GuiCommand ABI is materialized
// from this record on demand; its 64-byte inline text field is not storage.
const ProgramGuiCommand = extern struct {
    payload_offset: u64 = 0,
    payload_bytes: u64 = 0,
    parameter0: u64 = 0,
    parameter1: u64 = 0,
    raster_word_offset: u64 = 0,
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
    resource_kind: ProgramGuiCommandResourceKind = .none,
    reserved: u16 = 0,
};

// Variable-sized command block.  Commands follow the header in the same heap
// allocation.  Capacity is allocation granularity, never a display-list cap.
const ProgramGuiCommandPayload = extern struct {
    previous: ?*ProgramGuiCommandPayload = null,
    next: ?*ProgramGuiCommandPayload = null,
    allocation_sequence: u64 = 0,
    logical_offset: u64 = 0,
    header: ProgramPayloadHeader = .{},
    command_count: u32 = 0,
    capacity: u32 = 0,
};

// Common variable-sized frame resource node.  UTF-8, raster bytes and future
// paths share one append-ordered logical blob.  Header kind retains physical
// raster-vs-data telemetry without creating a second aggregate store.
const ProgramGuiResourcePayload = extern struct {
    previous: ?*ProgramGuiResourcePayload = null,
    next: ?*ProgramGuiResourcePayload = null,
    allocation_sequence: u64 = 0,
    logical_offset: u64 = 0,
    raster_word_offset: u64 = 0,
    header: ProgramPayloadHeader = .{},
    byte_count: u32 = 0,
    resource_kind: ProgramGuiCommandResourceKind = .none,
    reserved: u16 = 0,
};

const ProgramGuiFramePayload = extern struct {
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
    header: ProgramPayloadHeader = .{},
    damage_regions: [r4x_api.gui_frame_max_damage_regions]DisplayDamageRect = .{DisplayDamageRect{}} ** r4x_api.gui_frame_max_damage_regions,
    damage_count: u32 = 0,
    chain_depth: u32 = 1,
    reader_refs: u32 = 0,
    shared_raster_count: u32 = 0,
    build_failed: bool = false,
    explicit_build: bool = false,
    replacement: bool = false,
    retired: bool = false,
};

const ProgramGuiPayload = extern struct {
    frame_oom: u64 = 0,
    frame_xrgb32_nearest_commands: u64 = 0,
    frame_lock: sync.Mutex = sync.Mutex.initClass("r4x-gui-frame", sync.LockRank.program_instances, .no_sleep),
    committed_frame: ?*ProgramGuiFramePayload = null,
    building_frame: ?*ProgramGuiFramePayload = null,
    retired_frames: ?*ProgramGuiFramePayload = null,
    frame_peak_commands: u64 = 0,
    frame_peak_bytes: u64 = 0,
    events: [GUI_EVENT_QUEUE_SIZE]GuiEvent = .{GuiEvent{}} ** GUI_EVENT_QUEUE_SIZE,
    event_head: usize = 0,
    event_tail: usize = 0,
    frame_snapshot_reads: u64 = 0,
    frame_avoided_clone_bytes: u64 = 0,
    frame_stream_peak_bytes: u64 = 0,
    frame_xrgb32_nearest_resource_bytes: u64 = 0,
    frame_reader_retired_generations: u64 = 0,
    frame_coalesced_generations: u64 = 0,
    frame_superseded_generations: u64 = 0,
    frame_commits: u64 = 0,
    frame_cancels: u64 = 0,
    frame_replacement_commits: u64 = 0,
    frame_generation_reads: u64 = 0,
    operation_gate: sync.UnwindGuard = sync.UnwindGuard.init("gui-operation"),
    frame_delta_commits: u64 = 0,
    frame_full_commits: u64 = 0,
    frame_indexed8_commands: u64 = 0,
    frame_indexed8_resource_bytes: u64 = 0,
    header: ProgramPayloadHeader = .{},
    revision: u32 = 0,
    min_client_h: i32 = 0,
    min_client_w: i32 = 0,
    event_high_water: u32 = 0,
    font_id: u32 = GUI_FONT_BUILTIN_ID,
    frame_last_error: i32 = 0,
    window_info: GuiWindowInfo = .{},
    window_id: i32 = -1,
    host_launch_request: ProgramHostLaunchRequest = .{},
    title: [GUI_TITLE_SIZE]u8 = .{0} ** GUI_TITLE_SIZE,
    text: [GUI_TEXT_SIZE]u8 = .{0} ** GUI_TEXT_SIZE,
    start_attach_pending: bool = false,
    host_launch_pending: bool = false,
};

const ProgramInstanceStorage = extern struct {
    runtime: *ProgramRuntimePayload,
    process: *ProgramProcessPayload,
    console: ?*ProgramConsolePayload = null,
    gui: ?*ProgramGuiPayload = null,
};

const ProgramInstance = struct {
    parallel_execution: bool = false, // immutable after publication
    desktop_host: bool = false, // immutable admitted GUI owner declaration
    gfx_virtual_closed: bool = false, // common BO owner; closed before exit publication
    notifications: notifications.Owner = .{},
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
    cancel_pending: bool = false,
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

const ProgramInstanceLease = extern struct {
    slot: *ProgramRegistrySlot,
    instance: *ProgramInstance,
    generation: u64,
    id: u32,
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

pub const ProgramInstanceStorageStats = extern struct {
    gui_frame_cancels: u64 = 0,
    payload_releases: u64 = 0,
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
    quarantined_bytes: u64 = 0,
    quarantined_payloads: u64 = 0,
    free_failures: u64 = 0,
    header_errors: u64 = 0,
    owner_mismatches: u64 = 0,
    transaction_rollbacks: u64 = 0,
    allocation_failures: u64 = 0,
    payload_allocations: u64 = 0,
    allocation_attempts: u64 = 0,
    registry_reserved_core_bytes: u64 = 0,
    gui_frame_snapshot_reads: u64 = 0,
    core_bytes_per_instance: u64 = @sizeOf(ProgramInstance),
    gui_frame_oom_failures: u64 = 0,
    gui_frame_commits: u64 = 0,
    current_gui_frame_bytes: u64 = 0,
    peak_gui_frame_bytes: u64 = 0,
    current_gui_frame_commands: u64 = 0,
    peak_gui_frame_commands: u64 = 0,
    current_gui_frame_nodes: u64 = 0,
    peak_gui_frame_nodes: u64 = 0,
    active_gui_data_payloads: u32 = 0,
    active_gui_payloads: u32 = 0,
    active_gui_raster_payloads: u32 = 0,
    active_gui_command_payloads: u32 = 0,
    active_console_output_payloads: u32 = 0,
    active_console_payloads: u32 = 0,
    active_gui_frame_payloads: u32 = 0,
    active_environment_payloads: u32 = 0,
    active_process_payloads: u32 = 0,
    active_runtime_payloads: u32 = 0,
    active_gui_instances: u32 = 0,
    active_console_instances: u32 = 0,
    active_service_instances: u32 = 0,
    active_instances: u32 = 0,
};

pub const ProgramInstanceStorageSelfTestReport = extern struct {
    peak_payload_bytes: u64 = 0,
    cases: u32 = 0,
    failed_case: u32 = 0,
    heap_baseline_ok: bool = false,
    storage_baseline_ok: bool = false,
    zero_init_ok: bool = false,
};

fn addInstanceStorageStat(comptime field: []const u8, amount: u64) void {
    r4os_rust_program_payload_stat(@intCast(@offsetOf(ProgramInstanceStorageStats, field)), @intCast(@sizeOf(@TypeOf(@field(@as(ProgramInstanceStorageStats, undefined), field)))), amount);
}

// System-wide generation prevents a stale (instance id, generation) pair from
// aliasing a new frame after program-id reuse.  Zero means no committed frame.


const SHARED_RASTER_RESOURCE_CAPACITY: usize = 32;
const SHARED_RASTER_LEASE_CAPACITY: usize = 128;
const SHARED_RASTER_FRAME_REF_CAPACITY: usize = 256;
const SHARED_RASTER_STATS_CAPACITY: usize = 64;

const SharedRasterBuffer = struct {
    memory: ?[]u8 = null,
    backing: gfx_buffers.Handle = .{},
    reference: gfx_buffers.Handle = .{},
    write_lease: gfx_buffers.Handle = .{},
    frame_lease: gfx_buffers.Handle = .{},
    raster_generation: u64 = 0,
    write_token: u64 = 0,
    frame_refs: u32 = 0,
    lease_refs: u32 = 0,
};

const SharedRasterResourceState = struct {
    used: bool = false,
    closing: bool = false,
    owner: ProgramProcessHandle = .{},
    handle: GuiSharedRasterHandle = .{},
    info: GuiSharedRasterCreateInfo = .{},
    buffers: [r4x_api.gui_shared_raster_buffer_count]SharedRasterBuffer =
        .{SharedRasterBuffer{}} ** r4x_api.gui_shared_raster_buffer_count,
};

const SharedRasterLeaseRecord = struct {
    used: bool = false,
    consumer: ProgramProcessHandle = .{},
    producer: ProgramProcessHandle = .{},
    handle: GuiSharedRasterHandle = .{},
    raster_generation: u64 = 0,
    lease_token: u64 = 0,
    buffer_index: u32 = 0,
    backing_lease: gfx_buffers.Handle = .{},
};

const SharedRasterFrameRecord = struct {
    used: bool = false,
    frame: ?*ProgramGuiFramePayload = null,
    reference: ProgramGuiSharedRasterRef = .{},
};

const SharedRasterStats = extern struct {
    owner: ProgramProcessHandle = .{},
    publish_count: u64 = 0, acquire_count: u64 = 0, release_count: u64 = 0, backpressure_count: u64 = 0,
    published_bytes: u64 = 0, frame_bytes_avoided: u64 = 0, acquired_bytes: u64 = 0, live_bytes: u64 = 0,
    used: bool = false, reserved: [7]u8 = .{0} ** 7,
};

const SharedRasterFreeSet = struct {
    memories: [r4x_api.gui_shared_raster_buffer_count]?gfx_buffers.Created =
        .{null} ** r4x_api.gui_shared_raster_buffer_count,
};

// The shared raster mutex is the native BO runtime owner.

fn configureSharedRasterFailureForTest(fail_after: ?u32) void {
    r4os_rust_program_shared_failure(fail_after orelse 0, fail_after != null, true);
}

fn lockSharedRasterState() void {
    r4os_rust_program_shared_lock();
}



fn validSharedRasterHandle(handle: GuiSharedRasterHandle) bool {
    return r4os_rust_program_shared_valid(handle);
}

fn sharedRasterHandleEqual(a: GuiSharedRasterHandle, b: GuiSharedRasterHandle) bool {
    return r4os_rust_program_shared_equal(a, b);
}











fn graphicsOwner(owner: ProgramProcessHandle) gfx_buffers.Owner {
    const value = r4os_rust_program_shared_graphics_owner(owner);
    return .{ .kind = @enumFromInt(value.kind), .id = value.id, .generation = value.generation };
}







fn sharedRasterValidCreateInfo(info: GuiSharedRasterCreateInfo) bool {
    return r4os_rust_program_shared_info_valid(info);
}



fn sharedRasterCreate(owner: ProgramProcessHandle, info: GuiSharedRasterCreateInfo, out_handle: *GuiSharedRasterHandle) i32 {
    return r4os_rust_program_shared_create(owner, info, out_handle);
}

fn sharedRasterDestroy(owner: ProgramProcessHandle, handle: GuiSharedRasterHandle) i32 {
    return r4os_rust_program_shared_destroy(owner, handle);
}

fn sharedRasterMapWrite(owner: ProgramProcessHandle, handle: GuiSharedRasterHandle, out_map: *GuiSharedRasterWriteMap) i32 {
    return r4os_rust_program_shared_write(owner, handle, out_map);
}

fn sharedRasterPublish(owner: ProgramProcessHandle, map: GuiSharedRasterWriteMap, out_generation: *u64) i32 {
    return r4os_rust_program_shared_publish(owner, map, out_generation);
}

fn sharedRasterPinExact(
    owner: ProgramProcessHandle,
    frame: *ProgramGuiFramePayload,
    handle: GuiSharedRasterHandle,
    raster_generation: u64,
    expected_format: ?u32,
    expected_width: ?u32,
    expected_height: ?u32,
    expected_data_bytes: ?u64,
) ?ProgramGuiSharedRasterRef {
    var out: ProgramGuiSharedRasterRef = undefined;
    const mask: u8 = @as(u8, @intFromBool(expected_format != null)) | (@as(u8, @intFromBool(expected_width != null)) << 1) |
        (@as(u8, @intFromBool(expected_height != null)) << 2) | (@as(u8, @intFromBool(expected_data_bytes != null)) << 3);
    return if (r4os_rust_program_shared_pin(owner, frame, handle, raster_generation, expected_format orelse 0, expected_width orelse 0, expected_height orelse 0, expected_data_bytes orelse 0, mask, &out)) out else null;
}

fn sharedRasterPinFrame(owner: ProgramProcessHandle, frame: *ProgramGuiFramePayload, descriptor: GuiSharedRasterResource) ?ProgramGuiSharedRasterRef {
    var out: ProgramGuiSharedRasterRef = undefined;
    return if (r4os_rust_program_shared_pin_frame(owner, frame, descriptor, &out)) out else null;
}

fn sharedRasterCopyFrameReferences(owner: ProgramProcessHandle, source: *const ProgramGuiFramePayload, target: *ProgramGuiFramePayload) bool {
    return r4os_rust_program_shared_copy(owner, source, target);
}

fn sharedRasterMoveFrameReferences(source: *ProgramGuiFramePayload, target: *ProgramGuiFramePayload) bool {
    return r4os_rust_program_shared_move(source, target);
}

fn sharedRasterReleaseFrameReferences(frame: *ProgramGuiFramePayload) bool {
    return r4os_rust_program_shared_release_frames(frame);
}

fn sharedRasterFrameHasReference(frame: *const ProgramGuiFramePayload, handle: GuiSharedRasterHandle, raster_generation: u64) bool {
    return r4os_rust_program_shared_has(frame, handle, raster_generation);
}

fn sharedRasterAcquire(
    consumer: ProgramProcessHandle,
    frame_owner: ProgramProcessHandle,
    frame_generation: u64,
    handle: GuiSharedRasterHandle,
    raster_generation: u64,
    out_map: *GuiSharedRasterMap,
) i32 {
    return r4os_rust_program_shared_acquire(consumer, frame_owner, frame_generation, handle, raster_generation, out_map);
}

fn sharedRasterRelease(consumer: ProgramProcessHandle, lease: GuiSharedRasterLease) i32 {
    return r4os_rust_program_shared_release(consumer, lease);
}

fn sharedRasterReleaseProcess(handle: ProgramProcessHandle) void {
    r4os_rust_program_shared_release_process(handle);
}

fn sharedRasterStatsSnapshot(owner: ProgramProcessHandle) SharedRasterStats {
    var out: SharedRasterStats = undefined;
    r4os_rust_program_shared_stats(owner, &out);
    return out;
}

fn sharedRasterStateEmpty() bool {
    return r4os_rust_program_shared_empty();
}

fn sharedRasterNoteFrameCommit(owner: ProgramProcessHandle, frame: *const ProgramGuiFramePayload) void {
    r4os_rust_program_shared_note(owner, frame);
}

pub fn configureInstanceStorageFailureForTest(fail_after: ?u32) void {
    r4os_rust_program_payload_failure(fail_after != null, fail_after orelse 0);
}

fn shouldInjectInstanceStorageFailure() bool {
    return r4os_rust_program_payload_injected();
}

fn noteGuiFrameReleaseForSelfTest(kind: ProgramPayloadKind) void {
    r4os_rust_program_payload_release_trace(@intFromEnum(kind));
}

fn allocateInstancePayload(comptime Payload: type, owner_id: u32, kind: ProgramPayloadKind) ?*Payload {
    return registryPointer(Payload, r4os_rust_program_payload_allocate(nativePayloadType(Payload), owner_id, @intFromEnum(kind)));
}

fn guiCommandPayloadBytes(capacity: u32) ?usize {
    var bytes: usize = undefined;
    return if (r4os_rust_program_payload_bytes(false, capacity, &bytes)) bytes else null;
}

fn guiCommandPayloadCommands(payload: *ProgramGuiCommandPayload) []ProgramGuiCommand {
    const span = r4os_rust_program_payload_span(0, payload);
    const data: [*]ProgramGuiCommand = @ptrCast(@alignCast(span.data));
    return data[0..span.len];
}

fn guiCommandPayloadCommandsConst(payload: *const ProgramGuiCommandPayload) []const ProgramGuiCommand {
    const span = r4os_rust_program_payload_span(0, payload);
    const data: [*]const ProgramGuiCommand = @ptrCast(@alignCast(span.data));
    return data[0..span.len];
}

fn allocateGuiCommandPayload(owner_id: u32, logical_offset: u64, capacity: u32) ?*ProgramGuiCommandPayload {
    return r4os_rust_program_payload_commands(owner_id, logical_offset, capacity);
}

fn guiResourcePayloadBytes(byte_count: usize) ?usize {
    var bytes: usize = undefined;
    return if (r4os_rust_program_payload_bytes(true, byte_count, &bytes)) bytes else null;
}

fn guiResourcePayloadData(payload: *ProgramGuiResourcePayload) []u8 {
    const span = r4os_rust_program_payload_span(1, payload);
    return span.data[0..span.len];
}

fn guiResourcePayloadDataConst(payload: *const ProgramGuiResourcePayload) []const u8 {
    const span = r4os_rust_program_payload_span(1, payload);
    return span.data[0..span.len];
}

fn guiResourcePayloadWords(payload: *ProgramGuiResourcePayload) []u32 {
    const span = r4os_rust_program_payload_span(2, payload);
    const data: [*]u32 = @ptrCast(@alignCast(span.data));
    return data[0..span.len];
}

fn guiResourcePayloadWordsConst(payload: *const ProgramGuiResourcePayload) []const u32 {
    const span = r4os_rust_program_payload_span(2, payload);
    const data: [*]const u32 = @ptrCast(@alignCast(span.data));
    return data[0..span.len];
}

fn guiResourcePayloadKind(resource_kind: ProgramGuiCommandResourceKind) ?ProgramPayloadKind {
    const value = r4os_rust_program_payload_resource_kind(@intFromEnum(resource_kind));
    return if (value == 0) null else @enumFromInt(value);
}

fn allocateGuiResourcePayload(
    owner_id: u32,
    logical_offset: u64,
    raster_word_offset: u64,
    byte_count: usize,
    resource_kind: ProgramGuiCommandResourceKind,
) ?*ProgramGuiResourcePayload {
    return r4os_rust_program_payload_resource(owner_id, logical_offset, raster_word_offset, byte_count, @intFromEnum(resource_kind));
}

fn releaseInstancePayload(comptime Payload: type, payload: *Payload, owner_id: u32, kind: ProgramPayloadKind) bool {
    return r4os_rust_program_payload_release(nativePayloadType(Payload), payload, owner_id, @intFromEnum(kind));
}

fn releaseGuiCommandPayload(payload: *ProgramGuiCommandPayload, owner_id: u32) bool {
    return r4os_rust_program_payload_variable_release(false, payload, owner_id);
}

fn releaseGuiResourcePayload(payload: *ProgramGuiResourcePayload, owner_id: u32) bool {
    return r4os_rust_program_payload_variable_release(true, payload, owner_id);
}

fn validateInstancePayloadHeader(comptime Payload: type, payload: *const Payload, owner_id: u32, kind: ProgramPayloadKind) bool {
    return r4os_rust_program_payload_validate(&payload.header, owner_id, @intFromEnum(kind), @sizeOf(Payload));
}

fn validateInstancePayloadHeaderBytes(header: *const ProgramPayloadHeader, owner_id: u32, kind: ProgramPayloadKind, requested_bytes: usize) bool {
    return r4os_rust_program_payload_validate(header, owner_id, @intFromEnum(kind), requested_bytes);
}

fn validateConsoleOutputPayload(payload: *const ProgramConsoleOutputPayload, owner_id: u32) bool {
    return r4os_rust_program_console_output(0, @constCast(payload), owner_id, false);
}

fn allocateConsoleOutputPayload(owner_id: u32) ?*ProgramConsoleOutputPayload {
    return r4os_rust_program_console_allocate(owner_id);
}

fn retainConsoleOutputPayload(payload: *ProgramConsoleOutputPayload, active: bool) bool {
    return r4os_rust_program_console_output(1, payload, 0, active);
}

fn makeConsoleOutputPayloadRefInactive(payload: *ProgramConsoleOutputPayload) bool {
    return r4os_rust_program_console_output(2, payload, 0, false);
}

fn freeConsoleOutputPayloadMemory(payload: *ProgramConsoleOutputPayload) bool {
    return r4os_rust_program_console_output(3, payload, 0, false);
}

fn releaseConsoleOutputPayload(payload: *ProgramConsoleOutputPayload, active: bool) bool {
    return r4os_rust_program_console_output(4, payload, 0, active);
}

fn releaseConsoleTranscriptSegments(transcript: *ProgramConsoleTranscriptPayload, active: bool) bool {
    return r4os_rust_program_console_transcript(1, transcript, 0, active);
}

fn makeConsoleTranscriptInactive(transcript: *ProgramConsoleTranscriptPayload) bool {
    return r4os_rust_program_console_transcript(2, transcript, 0, false);
}

fn validateConsoleTranscript(transcript: *const ProgramConsoleTranscriptPayload, owner_id: u32) bool {
    return r4os_rust_program_console_transcript(0, @constCast(transcript), owner_id, false);
}

fn allocateProgramInstanceStorage(owner_id: u32, app_class: AppClass, inherit_environment: bool) ?ProgramInstanceStorage {
    var out: ProgramInstanceStorage = undefined;
    return if (r4os_rust_program_storage_allocate(owner_id, @intFromEnum(app_class), inherit_environment, &out)) out else null;
}

fn validateGuiCommandPayload(payload: *const ProgramGuiCommandPayload, owner_id: u32) bool {
    return r4os_rust_program_gui_validate(0, payload, owner_id, false);
}

fn validateGuiResourcePayload(payload: *const ProgramGuiResourcePayload, owner_id: u32) bool {
    return r4os_rust_program_gui_validate(1, payload, owner_id, false);
}

fn validateGuiFrameOwnership(frame: *const ProgramGuiFramePayload, owner_id: u32) bool {
    return r4os_rust_program_gui_validate(2, frame, owner_id, false);
}

fn nextPhysicalGuiResource(start: ?*const ProgramGuiResourcePayload) ?*const ProgramGuiResourcePayload {
    return r4os_rust_program_gui_physical(start);
}

fn validateGuiFrame(frame: *const ProgramGuiFramePayload, owner_id: u32) bool {
    return r4os_rust_program_gui_validate(3, frame, owner_id, false);
}

fn validateCommittedGuiFrameChain(root: *const ProgramGuiFramePayload, owner_id: u32, retired_root: bool) bool {
    return r4os_rust_program_gui_validate(4, root, owner_id, retired_root);
}

fn guiFrameLocalBytes(frame: *const ProgramGuiFramePayload) u64 {
    return r4os_rust_program_gui_count(0, frame);
}

fn guiFrameBytes(frame: *const ProgramGuiFramePayload) u64 {
    return r4os_rust_program_gui_count(1, frame);
}

fn guiFrameChainCommandCount(frame: *const ProgramGuiFramePayload) u64 {
    return r4os_rust_program_gui_count(2, frame);
}

fn guiFrameChainResourceBytes(frame: *const ProgramGuiFramePayload) u64 {
    return r4os_rust_program_gui_count(3, frame);
}

fn guiFrameChainRasterWords(frame: *const ProgramGuiFramePayload) u64 {
    return r4os_rust_program_gui_count(4, frame);
}

fn guiFrameChainHasReaders(frame: *const ProgramGuiFramePayload) bool {
    return r4os_rust_program_gui_count(5, frame) != 0;
}

fn guiLiveGenerationCount(gui: *const ProgramGuiPayload) u32 {
    return @intCast(r4os_rust_program_gui_count(6, gui));
}

fn guiOwnedFrameBytes(gui: *const ProgramGuiPayload) u64 {
    return r4os_rust_program_gui_count(7, gui);
}

fn guiOwnedFrameCommands(gui: *const ProgramGuiPayload) u64 {
    return r4os_rust_program_gui_count(8, gui);
}

fn refreshGuiFrameOwnerPeak(gui: *ProgramGuiPayload) void {
    r4os_rust_program_gui_peak(gui);
}

fn releaseGuiFrameLocal(frame: *ProgramGuiFramePayload, owner_id: u32) bool {
    return r4os_rust_program_gui_release(0, frame, owner_id);
}

fn releaseGuiFrame(frame: *ProgramGuiFramePayload, owner_id: u32) bool {
    return r4os_rust_program_gui_release(1, frame, owner_id);
}

fn validateGuiFrameSet(gui: *const ProgramGuiPayload, owner_id: u32) bool {
    return r4os_rust_program_gui_validate(5, gui, owner_id, false);
}

fn validateProgramInstanceStorageTransaction(owner_id: u32, storage: *const ProgramInstanceStorage) bool {
    return r4os_rust_program_storage_transaction(owner_id, @constCast(storage), false);
}

fn validateProgramInstanceStorage(instance: *const ProgramInstance) bool {
    return r4os_rust_program_storage_instance(0, @constCast(instance), null);
}

fn releaseGuiFrameSet(gui: *ProgramGuiPayload, owner_id: u32) bool {
    return r4os_rust_program_gui_release(2, gui, owner_id);
}

fn rollbackProgramInstanceStorage(owner_id: u32, storage: *ProgramInstanceStorage) void {
    _ = r4os_rust_program_storage_transaction(owner_id, storage, true);
}

fn detachProgramCompletionOutput(instance: *ProgramInstance, completion: *ProgramCompletionNode) bool {
    return r4os_rust_program_storage_instance(1, instance, completion);
}

fn releaseProgramInstanceStorage(instance: *ProgramInstance, completion: *ProgramCompletionNode) bool {
    return r4os_rust_program_storage_instance(2, instance, completion);
}

fn freeCompletionOutput(node: *ProgramCompletionNode) void {
    r4os_rust_program_storage_free_output(node);
}

pub fn instanceStorageStats() ProgramInstanceStorageStats {
    var out: ProgramInstanceStorageStats = undefined;
    r4os_rust_program_payload_stats(&out);
    return out;
}

pub fn instanceStorageSelfTestReport() ProgramInstanceStorageSelfTestReport {
    var out: ProgramInstanceStorageSelfTestReport = undefined;
    r4os_rust_program_payload_report(&out);
    return out;
}

pub fn runInstanceStorageSelfTest() bool {
    return r4os_rust_program_storage_self_test();
}



















































fn notePayloadAllocation(kind: ProgramPayloadKind, bytes: usize) void {
    r4os_rust_program_payload_account(0, @intFromEnum(kind), bytes);
}

fn notePayloadRelease(kind: ProgramPayloadKind, bytes: usize) void {
    r4os_rust_program_payload_account(1, @intFromEnum(kind), bytes);
}

fn isGuiFramePayloadKind(kind: ProgramPayloadKind) bool {
    return r4os_rust_program_payload_is_frame(@intFromEnum(kind));
}

fn noteGuiFrameCommandsAllocation(count: u64) void {
    r4os_rust_program_payload_account(4, 0, count);
}

fn noteGuiFrameCommandsRelease(count: u64) void {
    r4os_rust_program_payload_account(5, 0, count);
}

const PayloadByteCounters = struct {
    current: *u64,
    peak: *u64,
};





fn refreshInstanceByteTelemetry() void {
    r4os_rust_program_payload_account(9, 0, 0);
}

fn noteActivePayloadAllocation(bytes: usize) void {
    r4os_rust_program_payload_account(2, 0, bytes);
}

fn noteActivePayloadRelease(bytes: usize) void {
    r4os_rust_program_payload_account(3, 0, bytes);
}

fn programInstancePayloadBytes(instance: *const ProgramInstance) u64 {
    return r4os_rust_program_storage_bytes(instance);
}

fn noteProgramInstancePublished(app_class: AppClass, payload_bytes: u64) void {
    r4os_rust_program_payload_account(6, @intFromEnum(app_class), payload_bytes);
}

fn noteProgramInstanceRetired(app_class: AppClass) void {
    r4os_rust_program_payload_account(7, @intFromEnum(app_class), 0);
}

fn quarantinePayload(kind: ProgramPayloadKind, bytes: usize) void {
    r4os_rust_program_payload_account(8, @intFromEnum(kind), bytes);
}

fn runtimePayload(instance: *ProgramInstance) *ProgramRuntimePayload {
    return @ptrCast(@alignCast(r4os_rust_program_payload_pointer(0, instance)));
}

fn runtimePayloadConst(instance: *const ProgramInstance) *const ProgramRuntimePayload {
    return @ptrCast(@alignCast(r4os_rust_program_payload_pointer(0, instance)));
}

fn processPayload(instance: *ProgramInstance) *ProgramProcessPayload {
    return @ptrCast(@alignCast(r4os_rust_program_payload_pointer(1, instance)));
}

fn processPayloadConst(instance: *const ProgramInstance) *const ProgramProcessPayload {
    return @ptrCast(@alignCast(r4os_rust_program_payload_pointer(1, instance)));
}

fn consolePayload(instance: *ProgramInstance) *ProgramConsolePayload {
    return @ptrCast(@alignCast(r4os_rust_program_payload_pointer(2, instance)));
}

fn consolePayloadConst(instance: *const ProgramInstance) *const ProgramConsolePayload {
    return @ptrCast(@alignCast(r4os_rust_program_payload_pointer(2, instance)));
}

fn guiPayload(instance: *ProgramInstance) *ProgramGuiPayload {
    return @ptrCast(@alignCast(r4os_rust_program_payload_pointer(3, instance)));
}

fn guiPayloadConst(instance: *const ProgramInstance) *const ProgramGuiPayload {
    return @ptrCast(@alignCast(r4os_rust_program_payload_pointer(3, instance)));
}

fn ensureGuiPayload(instance: *ProgramInstance) ?*ProgramGuiPayload {
    return registryPointer(ProgramGuiPayload, r4os_rust_program_data_ensure(instance, false));
}

fn ensureEnvironmentPayload(instance: *ProgramInstance) ?*ProgramEnvironmentPayload {
    return registryPointer(ProgramEnvironmentPayload, r4os_rust_program_data_ensure(instance, true));
}

fn reserveGuiFrameGeneration() ?u64 {
    const generation = r4os_rust_program_gui_generation();
    return if (generation != 0) generation else null;
}

fn allocateGuiFramePayload(owner_id: u32, explicit_build: bool) ?*ProgramGuiFramePayload {
    return r4os_rust_program_gui_allocate_frame(owner_id, explicit_build);
}

fn nextGuiCommandCapacity(frame: *const ProgramGuiFramePayload) u32 {
    return @intCast(r4os_rust_program_gui_reservation_info(0, @constCast(frame), 0));
}

fn guiCommandNeedsNewPayload(frame: *const ProgramGuiFramePayload) bool {
    return r4os_rust_program_gui_reservation_info(1, @constCast(frame), 0) != 0;
}

fn guiFrameCanLinkNodes(frame: *const ProgramGuiFramePayload, count: u64) bool {
    return r4os_rust_program_gui_reservation_info(2, @constCast(frame), count) != 0;
}

fn nextGuiFrameNodeSequence(frame: *ProgramGuiFramePayload) u64 {
    return r4os_rust_program_gui_reservation_info(3, frame, 0);
}

const GuiCommandStorage = extern struct {
    gui: *ProgramGuiPayload,
    frame: *ProgramGuiFramePayload,
    payload: *ProgramGuiCommandPayload,
    new_payload: bool,
};

fn prepareGuiCommandStorage(instance: *ProgramInstance, frame: *ProgramGuiFramePayload) ?GuiCommandStorage {
    var out: GuiCommandStorage = undefined;
    return if (r4os_rust_program_gui_prepare(0, instance, frame, 0, 0, &out)) out else null;
}

fn cancelGuiCommandStorage(instance: *ProgramInstance, storage: *GuiCommandStorage) void {
    r4os_rust_program_gui_cancel(0, instance, storage);
}

fn commitGuiCommandStorage(storage: *GuiCommandStorage, command: ProgramGuiCommand) i32 {
    return r4os_rust_program_gui_commit(0, storage, &command);
}

const GuiResourceStorage = extern struct {
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
    var out: GuiResourceStorage = undefined;
    return if (r4os_rust_program_gui_prepare(1, instance, frame, byte_count, @intFromEnum(resource_kind), &out)) out else null;
}

fn cancelGuiResourceStorage(instance: *ProgramInstance, storage: *GuiResourceStorage) void {
    r4os_rust_program_gui_cancel(1, instance, storage);
}

fn commitGuiResourceStorage(storage: *GuiResourceStorage) void {
    _ = r4os_rust_program_gui_commit(1, storage, null);
}

const GuiBlitStorage = extern struct {
    command: GuiCommandStorage,
    resource: GuiResourceStorage,
};

fn prepareGuiBlitStorage(
    instance: *ProgramInstance,
    frame: *ProgramGuiFramePayload,
    word_count: usize,
    resource_kind: ProgramGuiCommandResourceKind,
) ?GuiBlitStorage {
    var out: GuiBlitStorage = undefined;
    return if (r4os_rust_program_gui_prepare(2, instance, frame, word_count, @intFromEnum(resource_kind), &out)) out else null;
}

fn cancelGuiBlitStorage(instance: *ProgramInstance, storage: *GuiBlitStorage) void {
    r4os_rust_program_gui_cancel(2, instance, storage);
}

fn commitGuiBlitStorage(storage: *GuiBlitStorage, command: ProgramGuiCommand) i32 {
    return r4os_rust_program_gui_commit(2, storage, &command);
}

const GuiFrameCapture = extern struct {
    gui: *ProgramGuiPayload,
    frame: *ProgramGuiFramePayload,
    generation: u64,
    unwind: task_context.UnwindToken = .{},
};

// Short resident metadata owner; captured immutable readers hold an unwind
// lease across allocation, copying and cooperative rescheduling.
fn lockGuiFrameState(gui: *ProgramGuiPayload) void {
    r4os_rust_program_gui_lock(gui, false);
}

fn unlockGuiFrameState(gui: *ProgramGuiPayload) void {
    r4os_rust_program_gui_lock(gui, true);
}

fn captureCommittedGuiFrame(instance: *ProgramInstance, expected_generation: ?u64) ?GuiFrameCapture {
    var out: GuiFrameCapture = undefined;
    return if (r4os_rust_program_gui_capture(instance, expected_generation orelse 0, expected_generation != null, false, &out)) out else null;
}

fn captureGuiFrameGeneration(instance: *ProgramInstance, generation: u64) ?GuiFrameCapture {
    var out: GuiFrameCapture = undefined;
    return if (r4os_rust_program_gui_capture(instance, generation, true, true, &out)) out else null;
}

fn removeRetiredGuiFrameLocked(gui: *ProgramGuiPayload, frame: *ProgramGuiFramePayload) bool {
    return r4os_rust_program_gui_remove_retired(gui, frame, false) != null;
}

fn guiFrameChainContains(root: *const ProgramGuiFramePayload, wanted: *const ProgramGuiFramePayload) bool {
    return r4os_rust_program_gui_contains(root, wanted);
}

fn removeReleasableRetiredGuiChainLocked(gui: *ProgramGuiPayload, changed: *const ProgramGuiFramePayload) ?*ProgramGuiFramePayload {
    return r4os_rust_program_gui_remove_retired(gui, @constCast(changed), true);
}

fn releaseCapturedGuiFrame(instance: *ProgramInstance, capture: *GuiFrameCapture) void {
    r4os_rust_program_gui_release_capture(instance, capture);
}

fn linkClonedGuiCommand(frame: *ProgramGuiFramePayload, payload: *ProgramGuiCommandPayload) void {
    r4os_rust_program_gui_link(frame, payload, false);
}

fn linkClonedGuiResource(frame: *ProgramGuiFramePayload, payload: *ProgramGuiResourcePayload) void {
    r4os_rust_program_gui_link(frame, payload, true);
}

fn copyGuiBytesCooperatively(destination: []u8, source: []const u8) void {
    std.debug.assert(destination.len == source.len);
    r4os_rust_program_gui_copy(destination.ptr, source.ptr, source.len);
}

fn cloneGuiFrame(instance: *ProgramInstance, source: *const ProgramGuiFramePayload) ?*ProgramGuiFramePayload {
    return r4os_rust_program_gui_clone(instance, source);
}

fn guiFrameHasSharedRasterRef(frame: *const ProgramGuiFramePayload, handle: GuiSharedRasterHandle, raster_generation: u64) bool {
    return r4os_rust_program_gui_has_ref(frame, handle, raster_generation);
}

fn guiFramePinBatchSharedRasters(
    owner: ProgramProcessHandle,
    building: *const ProgramGuiFramePayload,
    delta: *ProgramGuiFramePayload,
    commands: []const GuiFrameCommand,
    resources: []const u8,
) i32 {
    return r4os_rust_program_gui_pin_batch(owner, building, delta, commands.ptr, commands.len, .{ .data = resources.ptr, .len = resources.len });
}

fn batchCommandResourceKind(command: *const GuiFrameCommand) ?ProgramGuiCommandResourceKind {
    const kind = r4os_rust_program_gui_batch_kind(command);
    return if (kind == std.math.maxInt(u16)) null else @enumFromInt(kind);
}

fn batchCommandResourceSlice(command: *const GuiFrameCommand, resources: []const u8) ?[]const u8 {
    var out: NativeProgramBytes = undefined;
    return if (r4os_rust_program_gui_batch_slice(command, .{ .data = resources.ptr, .len = resources.len }, &out)) out.data[0..out.len] else null;
}

fn guiShapeReadU32(bytes: []const u8, offset: usize) ?u32 {
    var out: u32 = undefined;
    return if (r4os_rust_program_gui_shape_word(.{ .data = bytes.ptr, .len = bytes.len }, offset, &out)) out else null;
}

fn guiShapeField(bytes: []const u8, comptime name: []const u8) ?u32 {
    var out: u32 = undefined;
    return if (r4os_rust_program_gui_shape_word(.{ .data = bytes.ptr, .len = bytes.len }, @offsetOf(GuiShapeResource, name), &out)) out else null;
}

fn guiSharedRasterDescriptor(resource: []const u8) ?GuiSharedRasterResource {
    var out: GuiSharedRasterResource = undefined;
    return if (r4os_rust_program_gui_descriptor(.{ .data = resource.ptr, .len = resource.len }, &out)) out else null;
}

fn validateGuiSharedRasterResource(command: *const GuiFrameCommand, resource: []const u8) bool {
    return r4os_rust_program_gui_resource_valid(0, command, .{ .data = resource.ptr, .len = resource.len }, 0);
}

fn validateGuiIndexed8Resource(command: *const GuiFrameCommand, resource: []const u8) bool {
    return r4os_rust_program_gui_resource_valid(1, command, .{ .data = resource.ptr, .len = resource.len }, 0);
}

fn validateGuiXrgb32Resource(command: *const GuiFrameCommand, resource: []const u8) bool {
    return r4os_rust_program_gui_resource_valid(2, command, .{ .data = resource.ptr, .len = resource.len }, 0);
}

fn guiShapeFloat(bits: u32) ?f32 {
    return if (r4os_rust_program_gui_float(0, bits, 0)) @as(f32, @bitCast(bits)) else null;
}

fn validGuiShapeCoordinate(bits: u32) bool {
    return r4os_rust_program_gui_float(1, bits, 0);
}

fn validGuiShapeNonNegative(bits: u32, maximum: f32) bool {
    return r4os_rust_program_gui_float(2, bits, @bitCast(maximum));
}

fn validGuiPathSegments(resource: []const u8, segment_count: u32) bool {
    return r4os_rust_program_gui_resource_valid(5, null, .{ .data = resource.ptr, .len = resource.len }, segment_count);
}

fn validateGuiShapeResource(command: *const GuiFrameCommand, resource: []const u8) bool {
    return r4os_rust_program_gui_resource_valid(3, command, .{ .data = resource.ptr, .len = resource.len }, 0);
}

fn validateGuiFrameBatchCommand(command: *const GuiFrameCommand, resources: []const u8) bool {
    return r4os_rust_program_gui_resource_valid(4, command, .{ .data = resources.ptr, .len = resources.len }, 0);
}

fn internalGuiFrameCommand(command: *const GuiFrameCommand, resource_base: u64, raster_word_offset: u64) ProgramGuiCommand {
    var out: ProgramGuiCommand = undefined;
    r4os_rust_program_gui_internal(command, resource_base, raster_word_offset, &out);
    return out;
}

fn appendBatchResourceBlob(instance: *ProgramInstance, frame: *ProgramGuiFramePayload, resources: []const u8) bool {
    return r4os_rust_program_gui_blob(instance, frame, .{ .data = resources.ptr, .len = resources.len });
}

fn spliceGuiFrameDelta(gui: *ProgramGuiPayload, building: *ProgramGuiFramePayload, delta: *ProgramGuiFramePayload) void {
    r4os_rust_program_gui_splice(gui, building, delta);
}

fn guiFrameAppendBatch(instance: *ProgramInstance, commands: []const GuiFrameCommand, resources: []const u8) i32 {
    return r4os_rust_program_gui_batch(instance, commands.ptr, commands.len, .{ .data = resources.ptr, .len = resources.len });
}

fn guiFrameMarkBuildFailed(gui: *ProgramGuiPayload, frame: ?*ProgramGuiFramePayload) void {
    r4os_rust_program_gui_failed(gui, frame);
}

fn setGuiFrameResult(gui: *ProgramGuiPayload, result: i32) i32 {
    return r4os_rust_program_gui_result(gui, result);
}

fn guiFrameBegin(instance: *ProgramInstance) i32 {
    return r4os_rust_program_gui_begin(instance, 0, null, 0);
}

fn validGuiDamageRegion(region: DisplayDamageRect) bool {
    return r4os_rust_program_gui_damage_valid(region);
}

fn guiFrameBeginDamage(instance: *ProgramInstance, regions: []const DisplayDamageRect) i32 {
    return r4os_rust_program_gui_begin(instance, 1, regions.ptr, regions.len);
}

fn guiFrameBeginReplace(instance: *ProgramInstance, regions: []const DisplayDamageRect) i32 {
    return r4os_rust_program_gui_begin(instance, 2, regions.ptr, regions.len);
}

fn guiFrameReplaceBuild(instance: *ProgramInstance, explicit_build: bool) ?*ProgramGuiFramePayload {
    return r4os_rust_program_gui_build(instance, explicit_build, true);
}

fn guiFrameEnsureBuild(instance: *ProgramInstance) ?*ProgramGuiFramePayload {
    return r4os_rust_program_gui_build(instance, false, false);
}

fn guiFrameCancel(instance: *ProgramInstance) i32 {
    return r4os_rust_program_gui_publish(instance, true);
}

fn guiFrameCommit(instance: *ProgramInstance) i32 {
    return r4os_rust_program_gui_publish(instance, false);
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



// Private value projections retain the original physical stack/resource shape.

extern fn r4os_rust_program_image_allocate(usize, u32, *NativeProgramImage) bool;
extern fn r4os_rust_program_image_release(u32) bool;
extern fn r4os_rust_program_stack_telemetry(u32, *r4x_api.ProgramStackPerformanceInfo) bool;
extern fn r4os_rust_program_stack_measure(*ProgramStack) void;
extern fn r4os_rust_program_stack_log(*const ProgramInstance, *const ProgramStack, u32) void;
extern fn r4os_rust_program_stack_allocate(u32, u64, u64, u8, bool, *ProgramStack) bool;
extern fn r4os_rust_program_stack_free(*ProgramStack) bool;
extern fn r4os_rust_program_stack_from_instance(*const ProgramInstance, *ProgramStack) bool;
extern fn r4os_rust_program_stack_to_instance(*ProgramInstance, *const ProgramStack) void;
extern fn r4os_rust_program_resources_instance(*const ProgramInstance, *ProgramResources) void;
extern fn r4os_rust_program_resources_values(*const NativeProgramImage, *const ProgramStack, *ProgramResources) void;
extern fn r4os_rust_program_resources_cleanup(*ProgramResources) bool;
extern fn r4os_rust_program_resources_free(*const NativeProgramImage, *const ProgramStack) void;
extern fn r4os_rust_program_thread_stack_limit(u8, *const ProgramInstance, u64, *u64) bool;
extern fn r4os_rust_program_vm_limit(u8, ?*const ProgramInstance, u64) bool;
extern fn r4os_rust_program_vm_reserve(u64, u64, u64, *ProgramVmRegionInfo) i32;
extern fn r4os_rust_program_vm_commit(u32, u64, u64, u64) i32;
extern fn r4os_rust_program_vm_change(u8, u32, u64, u64) i32;
extern fn r4os_rust_program_vm_query(u32, *ProgramVmRegionInfo) i32;
extern fn r4os_rust_program_vm_probe(u64, *ProgramVmReserveProbe) i32;
extern fn r4os_rust_program_fault_prepare(u64, u64, u64, *u64) bool;
extern fn r4os_rust_program_fault_handle(u64, u64, bool) bool;




export fn r4os_zig_program_fault_demand(address: u64, error_code: u64, owner: u32) callconv(.c) bool {
    return mem_virt.handleDemandFault(address, error_code, .r4x_instance, owner);
}
comptime {
    if (@sizeOf(ProgramStack) != 96 or @alignOf(ProgramStack) != 8) @compileError("Native ProgramStack geometry drift");
    if (@offsetOf(ProgramStack, "range_id") != 80) @compileError("Native ProgramStack.range_id offset drift");
    if (@offsetOf(ProgramStack, "base") != 0) @compileError("Native ProgramStack.base offset drift");
    if (@offsetOf(ProgramStack, "reserve_size") != 8) @compileError("Native ProgramStack.reserve_size offset drift");
    if (@offsetOf(ProgramStack, "committed_base") != 16) @compileError("Native ProgramStack.committed_base offset drift");
    if (@offsetOf(ProgramStack, "committed_size") != 24) @compileError("Native ProgramStack.committed_size offset drift");
    if (@offsetOf(ProgramStack, "guard_base") != 32) @compileError("Native ProgramStack.guard_base offset drift");
    if (@offsetOf(ProgramStack, "guard_size") != 40) @compileError("Native ProgramStack.guard_size offset drift");
    if (@offsetOf(ProgramStack, "top") != 48) @compileError("Native ProgramStack.top offset drift");
    if (@offsetOf(ProgramStack, "owner_id") != 84) @compileError("Native ProgramStack.owner_id offset drift");
    if (@offsetOf(ProgramStack, "profile") != 92) @compileError("Native ProgramStack.profile offset drift");
    if (@offsetOf(ProgramStack, "initial_commit_size") != 56) @compileError("Native ProgramStack.initial_commit_size offset drift");
    if (@offsetOf(ProgramStack, "create_cycles") != 64) @compileError("Native ProgramStack.create_cycles offset drift");
    if (@offsetOf(ProgramStack, "telemetry_high_water") != 72) @compileError("Native ProgramStack.telemetry_high_water offset drift");
    if (@offsetOf(ProgramStack, "telemetry_committed_pages") != 88) @compileError("Native ProgramStack.telemetry_committed_pages offset drift");
    if (@offsetOf(ProgramStack, "serial_telemetry") != 93) @compileError("Native ProgramStack.serial_telemetry offset drift");
    if (@offsetOf(ProgramStack, "telemetry_measured") != 94) @compileError("Native ProgramStack.telemetry_measured offset drift");
    if (@sizeOf(ProgramResources) != 120 or @alignOf(ProgramResources) != 8) @compileError("Native ProgramResources geometry drift");
    if (@offsetOf(ProgramResources, "image_range_id") != 112) @compileError("Native ProgramResources.image_range_id offset drift");
    if (@offsetOf(ProgramResources, "image_base") != 0) @compileError("Native ProgramResources.image_base offset drift");
    if (@offsetOf(ProgramResources, "image_size") != 8) @compileError("Native ProgramResources.image_size offset drift");
    if (@offsetOf(ProgramResources, "image_owner_id") != 116) @compileError("Native ProgramResources.image_owner_id offset drift");
    if (@offsetOf(ProgramResources, "stack") != 16) @compileError("Native ProgramResources.stack offset drift");
    if (@sizeOf(ProgramStackTelemetryStats) != 80 or @alignOf(ProgramStackTelemetryStats) != 8) @compileError("Native ProgramStackTelemetryStats geometry drift");
    if (@offsetOf(ProgramStackTelemetryStats, "creates") != 0) @compileError("Native ProgramStackTelemetryStats.creates offset drift");
    if (@offsetOf(ProgramStackTelemetryStats, "releases") != 8) @compileError("Native ProgramStackTelemetryStats.releases offset drift");
    if (@offsetOf(ProgramStackTelemetryStats, "reserve_max_bytes") != 16) @compileError("Native ProgramStackTelemetryStats.reserve_max_bytes offset drift");
    if (@offsetOf(ProgramStackTelemetryStats, "initial_commit_max_bytes") != 24) @compileError("Native ProgramStackTelemetryStats.initial_commit_max_bytes offset drift");
    if (@offsetOf(ProgramStackTelemetryStats, "committed_max_bytes") != 32) @compileError("Native ProgramStackTelemetryStats.committed_max_bytes offset drift");
    if (@offsetOf(ProgramStackTelemetryStats, "high_water_max_bytes") != 40) @compileError("Native ProgramStackTelemetryStats.high_water_max_bytes offset drift");
    if (@offsetOf(ProgramStackTelemetryStats, "create_cycles_total") != 48) @compileError("Native ProgramStackTelemetryStats.create_cycles_total offset drift");
    if (@offsetOf(ProgramStackTelemetryStats, "create_cycles_max") != 56) @compileError("Native ProgramStackTelemetryStats.create_cycles_max offset drift");
    if (@offsetOf(ProgramStackTelemetryStats, "release_cycles_total") != 64) @compileError("Native ProgramStackTelemetryStats.release_cycles_total offset drift");
    if (@offsetOf(ProgramStackTelemetryStats, "release_cycles_max") != 72) @compileError("Native ProgramStackTelemetryStats.release_cycles_max offset drift");
    if (@intFromEnum(mem_virt.Window.program_image) != 2 or @intFromEnum(mem_virt.Window.app_stack) != 4 or @intFromEnum(mem_virt.Window.r4x_vm) != 6 or
        @intFromEnum(mem_blocks.Kind.program_image) != 5 or @intFromEnum(mem_blocks.Kind.app_stack) != 7 or @intFromEnum(mem_blocks.Kind.virtual_range) != 4 or
        @intFromEnum(mem_blocks.Owner.r4x_instance) != 3 or THREAD_FLAG_MAIN != 1 or owner_locks.STACK_HEADROOM != 64 * 1024 or
        paging.PAGE_SIZE != 4096 or PROGRAM_STACK_GROW_SIZE != 64 * 1024 or PROGRAM_STACK_GUARD_SIZE != 4096 or PROGRAM_STACK_HIGH_WATER_PATTERN != 0xA5)
        @compileError("Native R4X stack/VM/fault private contract drift");
}


const NativeProgramThreadState = extern struct {
    head: ?*ProgramThread, tail: ?*ProgramThread, count: usize, peak: usize,
    epoch: u64, create_failures: u64, next_generation: u64, next_id: u32,
};
const NativeProgramThreadBinding = extern struct { slot: u8, generation: u32 };
extern fn r4os_rust_program_threads_state() *NativeProgramThreadState;
fn threadState() *NativeProgramThreadState { return r4os_rust_program_threads_state(); }
extern fn r4os_rust_program_thread_lookup(u8, u32, ProgramJoinHandle, ?*const ProgramThread) ?*ProgramThread;
extern fn r4os_rust_program_thread_identity(bool, *u64) bool;
extern fn r4os_rust_program_thread_control(u8, *ProgramThread, i32) bool;
extern fn r4os_rust_program_thread_allocate() ?*ProgramThread;
extern fn r4os_rust_program_thread_epoch(bool) void;
extern fn r4os_rust_program_thread_pin(u32, ProgramJoinHandle, bool, bool) ?*ProgramThread;
extern fn r4os_rust_program_thread_execution(*ProgramThread, bool) ?*ProgramInstance;
extern fn r4os_rust_program_thread_input(*ProgramThread, ProgramProcessHandle, *ProgramInstanceLease) bool;
extern fn r4os_rust_program_thread_info(*const ProgramThread, *ProgramThreadInfo) void;
extern fn r4os_rust_program_thread_scalar(u8) u64;
extern fn r4os_rust_program_thread_create(usize, u64, u64, u32, *ProgramJoinHandle, *const fn() callconv(.c) void) i32;
extern fn r4os_rust_program_thread_create_legacy(usize, usize, u64, u32, *u32, *const fn() callconv(.c) void) i32;
extern fn r4os_rust_program_thread_register_main(*ProgramInstance, *task.Task) ?*ProgramThread;
extern fn r4os_rust_program_thread_exit(*ProgramThread, i32, bool, u8) noreturn;
extern fn r4os_rust_program_thread_api_exit(i32) noreturn;
extern fn r4os_rust_program_thread_program_exit(i32, u32) i32;
extern fn r4os_rust_program_thread_leave_spawn(?*ProgramThread) void;
extern fn r4os_rust_program_thread_current_handle(*ProgramJoinHandle) i32;
extern fn r4os_rust_program_thread_status(u32, ?*const ProgramJoinHandle, *ProgramThreadInfo, bool) i32;
extern fn r4os_rust_program_thread_entry(u8, *const ProgramInstance, usize, ?*usize) bool;
extern fn r4os_rust_program_thread_failure(u8, bool) i32;
extern fn r4os_rust_program_thread_join(u32, ?*const ProgramJoinHandle, bool, u64, *i32) i32;
extern fn r4os_rust_program_threads_terminate(ProgramProcessHandle, i32, bool, u32) void;
extern fn r4os_rust_program_threads_release(ProgramProcessHandle, bool) bool;
extern fn r4os_rust_program_threads_instance(u32, bool, i32, bool, u32) bool;
extern fn r4os_rust_program_thread_release_task(u32, u64, u32, u64) bool;
extern fn r4os_rust_program_thread_release_created(*task.Task) bool;

export fn r4os_zig_program_thread_retire_peer(operation: u8, handle: ProgramProcessHandle, task_id: u32, generation: u64) callconv(.c) bool {
    return switch (operation) {


        2 => r4api.r4sys.releaseStreamSlotsForProgramThread(handle.instance_id, handle.generation, task_id, generation),
        else => unreachable,
    };
}
export fn r4os_zig_program_thread_task_retired(task_id: u32, generation: u64) callconv(.c) void {
    gfx_queue.retiredTask(task_id, generation);
    gfx_allocations.retiredTask(task_id, generation);
    @import("../kernel/gfx_virtual.zig").retiredTask(task_id, generation);
}

comptime {
    if (@sizeOf(ProgramThread) != 320 or @alignOf(ProgramThread) != 8) @compileError("Native ProgramThread physical budget drift");
    if (@offsetOf(ProgramThread, "registry_prev") != 248) @compileError("Native ProgramThread.registry_prev drift");
    if (@offsetOf(ProgramThread, "registry_next") != 40) @compileError("Native ProgramThread.registry_next drift");
    if (@offsetOf(ProgramThread, "used") != 316) @compileError("Native ProgramThread.used drift");
    if (@offsetOf(ProgramThread, "pin_count") != 304) @compileError("Native ProgramThread.pin_count drift");
    if (@offsetOf(ProgramThread, "retire_pending") != 315) @compileError("Native ProgramThread.retire_pending drift");
    if (@offsetOf(ProgramThread, "retire_in_progress") != 313) @compileError("Native ProgramThread.retire_in_progress drift");
    if (@offsetOf(ProgramThread, "retire_for_instance") != 312) @compileError("Native ProgramThread.retire_for_instance drift");
    if (@offsetOf(ProgramThread, "task_detached") != 311) @compileError("Native ProgramThread.task_detached drift");
    if (@offsetOf(ProgramThread, "stack_released") != 308) @compileError("Native ProgramThread.stack_released drift");
    if (@offsetOf(ProgramThread, "id") != 284) @compileError("Native ProgramThread.id drift");
    if (@offsetOf(ProgramThread, "generation") != 232) @compileError("Native ProgramThread.generation drift");
    if (@offsetOf(ProgramThread, "instance_id") != 276) @compileError("Native ProgramThread.instance_id drift");
    if (@offsetOf(ProgramThread, "instance_generation") != 112) @compileError("Native ProgramThread.instance_generation drift");
    if (@offsetOf(ProgramThread, "execution_pinned") != 310) @compileError("Native ProgramThread.execution_pinned drift");
    if (@offsetOf(ProgramThread, "console_input_lease") != 0) @compileError("Native ProgramThread.console_input_lease drift");
    if (@offsetOf(ProgramThread, "spawn_transaction_depth") != 268) @compileError("Native ProgramThread.spawn_transaction_depth drift");
    if (@offsetOf(ProgramThread, "exit_deferred") != 309) @compileError("Native ProgramThread.exit_deferred drift");
    if (@offsetOf(ProgramThread, "owner_instance") != 240) @compileError("Native ProgramThread.owner_instance drift");
    if (@offsetOf(ProgramThread, "task_id") != 280) @compileError("Native ProgramThread.task_id drift");
    if (@offsetOf(ProgramThread, "task_generation") != 224) @compileError("Native ProgramThread.task_generation drift");
    if (@offsetOf(ProgramThread, "state") != 288) @compileError("Native ProgramThread.state drift");
    if (@offsetOf(ProgramThread, "flags") != 292) @compileError("Native ProgramThread.flags drift");
    if (@offsetOf(ProgramThread, "entry") != 120) @compileError("Native ProgramThread.entry drift");
    if (@offsetOf(ProgramThread, "arg") != 96) @compileError("Native ProgramThread.arg drift");
    if (@offsetOf(ProgramThread, "stack") != 128) @compileError("Native ProgramThread.stack drift");
    if (@offsetOf(ProgramThread, "exit_code") != 272) @compileError("Native ProgramThread.exit_code drift");
    if (@offsetOf(ProgramThread, "created_tick") != 88) @compileError("Native ProgramThread.created_tick drift");
    if (@offsetOf(ProgramThread, "finished_tick") != 256) @compileError("Native ProgramThread.finished_tick drift");
    if (@offsetOf(ProgramThread, "join_count") != 296) @compileError("Native ProgramThread.join_count drift");
    if (@offsetOf(ProgramThread, "join_owner_task_id") != 264) @compileError("Native ProgramThread.join_owner_task_id drift");
    if (@offsetOf(ProgramThread, "join_owner_task_generation") != 104) @compileError("Native ProgramThread.join_owner_task_generation drift");
    if (@offsetOf(ProgramThread, "join_lease_active") != 314) @compileError("Native ProgramThread.join_lease_active drift");
    if (@offsetOf(ProgramThread, "join_waiter_refs") != 300) @compileError("Native ProgramThread.join_waiter_refs drift");
    if (@offsetOf(ProgramThread, "join_queue") != 48) @compileError("Native ProgramThread.join_queue drift");
    if (@offsetOf(NativeProgramThreadState, "head") != 0) @compileError("Native thread state field drift");
    if (@offsetOf(NativeProgramThreadState, "tail") != 8) @compileError("Native thread state field drift");
    if (@offsetOf(NativeProgramThreadState, "count") != 16) @compileError("Native thread state field drift");
    if (@offsetOf(NativeProgramThreadState, "peak") != 24) @compileError("Native thread state field drift");
    if (@offsetOf(NativeProgramThreadState, "epoch") != 32) @compileError("Native thread state field drift");
    if (@offsetOf(NativeProgramThreadState, "create_failures") != 40) @compileError("Native thread state field drift");
    if (@offsetOf(NativeProgramThreadState, "next_generation") != 48) @compileError("Native thread state field drift");
    if (@offsetOf(NativeProgramThreadState, "next_id") != 56) @compileError("Native thread state field drift");
    if (@sizeOf(NativeProgramThreadState) != 64 or @sizeOf(NativeOptionalProgramInstanceLease) != 40 or @offsetOf(NativeOptionalProgramInstanceLease, "present") != 32 or
        @sizeOf(NativeProgramThreadBinding) != 8 or @offsetOf(NativeProgramThreadBinding, "generation") != 4 or
        THREAD_OK != 0 or THREAD_ERROR_INVALID != -1 or THREAD_ERROR_NO_INSTANCE != -2 or THREAD_ERROR_NO_MEMORY != -4 or THREAD_ERROR_NOT_FOUND != -5 or THREAD_ERROR_SELF_JOIN != -6 or
        THREAD_ERROR_BUSY != -7 or THREAD_ERROR_TIMEOUT != -8 or THREAD_ERROR_NOT_JOINABLE != -9 or THREAD_ERROR_UNSUPPORTED != -10 or THREAD_CREATE_FLAGS_SUPPORTED != 0 or
        THREAD_FLAG_MAIN != 1 or THREAD_FLAG_JOINABLE != 2 or THREAD_FLAG_JOINED != 4 or THREAD_INFO_VERSION != 1 or PROGRAM_EXIT_REASON_NATURAL != 0 or PROGRAM_EXIT_REASON_FAILED != 3 or
        R4X_MODULE_LINK_BASE != 0x00000004_00000000 or R4X_SIGNED_LINK_BASE != 0xFFFF_FFFF_00000000 or @sizeOf(ProgramThreadInfo) != 88 or MAX_R4M_IMPORTS != 16)
        @compileError("Native thread admission/private value contract drift");
}


extern fn r4os_rust_program_execution_summary(*ProgramInventorySummary) bool;
extern fn r4os_rust_program_execution_begin(*ProgramInventoryCursor, *ProgramInventorySummary) i32;
extern fn r4os_rust_program_execution_tasks(*ProgramInventoryCursor, [*]ProgramTaskSnapshot, u32, *ProgramInventoryPageInfo) i32;
extern fn r4os_rust_program_execution_threads(*ProgramInventoryCursor, [*]ProgramThreadSnapshot, u32, *ProgramInventoryPageInfo) i32;
comptime {
    if (@sizeOf(ProgramInventorySummary) != 160 or @sizeOf(ProgramTaskSnapshot) != 96 or @sizeOf(ProgramThreadSnapshot) != 136 or
        @sizeOf(ProgramInventoryCursor) != 80 or @sizeOf(ProgramInventoryPageInfo) != 48 or r4x_api.program_inventory_page_max != 64)
        @compileError("Native execution inventory canonical layout drift");
}


const NativeProgramEntryState = extern struct {
    entries_started: u64, attach_wait_events: u64, probe_main_mask: u64,
    probe_service_mask: u64, probe_thread_mask: u64, probe_bad_inheritance: u32, probe_active: bool,
};
const NativeProgramEntryView = extern struct {
    context: *R4XStartContext, args: NativeProgramBytes, imports: [*]const R4XStartImport, import_count: u32,
};
const NativeProgramSubsystemTrace = extern struct { id: NativeProgramBytes, mode: NativeProgramBytes };
extern fn r4os_rust_program_entry_state() *NativeProgramEntryState;
fn entryState() *NativeProgramEntryState { return r4os_rust_program_entry_state(); }
extern fn r4os_rust_program_entry_main(bool) noreturn;
extern fn r4os_rust_program_entry_worker() noreturn;
extern fn r4os_rust_program_entry_call(*ProgramInstance) i32;
extern fn r4os_rust_program_entry_context(*ProgramInstance) void;
extern fn r4os_rust_program_entry_cstring(NativeProgramBytes) u64;
extern fn r4os_rust_program_entry_trace(NativeProgramBytes, *NativeProgramSubsystemTrace) bool;
extern fn r4os_rust_program_entry_now() u64;
extern fn r4os_rust_program_entry_trace_phase(NativeProgramSubsystemTrace, NativeProgramBytes, u64) void;
extern fn r4os_rust_program_entry_report(bool, NativeProgramBytes, bool) void;


export fn r4os_zig_program_entry_boot_active() callconv(.c) bool { return bootscreen.isActive(); }
export fn r4os_zig_program_entry_boot_detail(detail: NativeProgramBytes) callconv(.c) void { bootscreen.setDetail(detail.data[0..detail.len]); }
comptime {
    if (@sizeOf(NativeProgramEntryState) != 48 or @alignOf(NativeProgramEntryState) != 8 or
        @offsetOf(NativeProgramEntryState, "entries_started") != 0 or @offsetOf(NativeProgramEntryState, "attach_wait_events") != 8 or
        @offsetOf(NativeProgramEntryState, "probe_main_mask") != 16 or @offsetOf(NativeProgramEntryState, "probe_service_mask") != 24 or @offsetOf(NativeProgramEntryState, "probe_thread_mask") != 32 or
        @offsetOf(NativeProgramEntryState, "probe_bad_inheritance") != 40 or @offsetOf(NativeProgramEntryState, "probe_active") != 44 or
        @sizeOf(NativeProgramEntryView) != 40 or @offsetOf(NativeProgramEntryView, "args") != 8 or @offsetOf(NativeProgramEntryView, "imports") != 24 or @offsetOf(NativeProgramEntryView, "import_count") != 32 or
        @sizeOf(NativeProgramSubsystemTrace) != 32 or @offsetOf(NativeProgramSubsystemTrace, "mode") != 16 or
        @intFromEnum(AppClass.console) != 0 or @intFromEnum(AppClass.gui) != 1 or @intFromEnum(AppClass.service) != 2 or
        @intFromEnum(ProgramThreadState.unused) != 0 or @intFromEnum(ProgramThreadState.ready) != 1 or @intFromEnum(ProgramThreadState.running) != 2 or @intFromEnum(ProgramThreadState.exited) != 3 or @intFromEnum(ProgramThreadState.killed) != 4)
        @compileError("Native entry/context/private counter layout drift");
}


const NativeProgramPayloadState = extern struct {
    stats: ProgramInstanceStorageStats, report: ProgramInstanceStorageSelfTestReport,
    active_published_bytes: u64, self_test_peak_bytes: u64, failure_after_value: u32, failure_cursor: u32,
    release_trace: [8]ProgramPayloadKind, release_trace_len: usize,
    failure_after_present: bool, self_test_active: bool, self_test_running: bool, release_trace_enabled: bool,
};
const NativeProgramPayloadSpan = extern struct { data: [*]u8, len: usize };
extern fn r4os_rust_program_payload_state() *NativeProgramPayloadState;
fn payloadState() *NativeProgramPayloadState { return r4os_rust_program_payload_state(); }
fn payloadFailureAfter() ?u32 { return if (payloadState().failure_after_present) payloadState().failure_after_value else null; }
fn setPayloadFailureAfter(value: ?u32) void { payloadState().failure_after_value = value orelse 0; payloadState().failure_after_present = value != null; }
fn nativePayloadType(comptime Payload: type) u8 {
    if (Payload == ProgramRuntimePayload) return 1;
    if (Payload == ProgramProcessPayload) return 2;
    if (Payload == ProgramEnvironmentPayload) return 3;
    if (Payload == ProgramConsolePayload) return 4;
    if (Payload == ProgramConsoleOutputPayload) return 5;
    if (Payload == ProgramGuiPayload) return 6;
    if (Payload == ProgramGuiCommandPayload) return 7;
    if (Payload == ProgramGuiResourcePayload) return 8;
    if (Payload == ProgramGuiFramePayload) return 9;
    if (Payload == ProgramConsoleTranscriptPayload) return 11;
    @compileError("Unknown canonical program payload type");
}
extern fn r4os_rust_program_payload_allocate(u8, u32, u16) ?*anyopaque;
extern fn r4os_rust_program_payload_release(u8, *anyopaque, u32, u16) bool;
extern fn r4os_rust_program_payload_validate(*const ProgramPayloadHeader, u32, u16, usize) bool;
extern fn r4os_rust_program_payload_stat(u32, u8, u64) void;
extern fn r4os_rust_program_payload_failure(bool, u32) void;
extern fn r4os_rust_program_payload_injected() bool;
extern fn r4os_rust_program_payload_release_trace(u16) void;
extern fn r4os_rust_program_payload_account(u8, u16, u64) void;
extern fn r4os_rust_program_payload_is_frame(u16) bool;
extern fn r4os_rust_program_payload_bytes(bool, usize, *usize) bool;
extern fn r4os_rust_program_payload_span(u8, *const anyopaque) NativeProgramPayloadSpan;
extern fn r4os_rust_program_payload_resource_kind(u16) u16;
extern fn r4os_rust_program_payload_commands(u32, u64, u32) ?*ProgramGuiCommandPayload;
extern fn r4os_rust_program_payload_resource(u32, u64, u64, usize, u16) ?*ProgramGuiResourcePayload;
extern fn r4os_rust_program_payload_variable_release(bool, *anyopaque, u32) bool;
extern fn r4os_rust_program_payload_pointer(u8, *const ProgramInstance) *anyopaque;
extern fn r4os_rust_program_payload_stats(*ProgramInstanceStorageStats) void;
extern fn r4os_rust_program_payload_report(*ProgramInstanceStorageSelfTestReport) void;
comptime {
    if (@sizeOf(ProgramPayloadHeader) != 16 or @alignOf(ProgramPayloadHeader) != 4) @compileError("Native ProgramPayloadHeader original geometry drift");
    if (@offsetOf(ProgramPayloadHeader, "magic") != 0) @compileError("Native ProgramPayloadHeader.magic offset drift");
    if (@offsetOf(ProgramPayloadHeader, "owner_id") != 4) @compileError("Native ProgramPayloadHeader.owner_id offset drift");
    if (@offsetOf(ProgramPayloadHeader, "requested_bytes") != 8) @compileError("Native ProgramPayloadHeader.requested_bytes offset drift");
    if (@offsetOf(ProgramPayloadHeader, "kind") != 12) @compileError("Native ProgramPayloadHeader.kind offset drift");
    if (@offsetOf(ProgramPayloadHeader, "reserved") != 14) @compileError("Native ProgramPayloadHeader.reserved offset drift");
    if (@sizeOf(ProgramRuntimePayload) != 2112 or @alignOf(ProgramRuntimePayload) != 8) @compileError("Native ProgramRuntimePayload original geometry drift");
    if (@offsetOf(ProgramRuntimePayload, "header") != 800) @compileError("Native ProgramRuntimePayload.header offset drift");
    if (@offsetOf(ProgramRuntimePayload, "local_context") != 0) @compileError("Native ProgramRuntimePayload.local_context offset drift");
    if (@offsetOf(ProgramRuntimePayload, "r4xstart_context") != 32) @compileError("Native ProgramRuntimePayload.r4xstart_context offset drift");
    if (@offsetOf(ProgramRuntimePayload, "r4xstart_imports") != 160) @compileError("Native ProgramRuntimePayload.r4xstart_imports offset drift");
    if (@offsetOf(ProgramRuntimePayload, "r4xstart_import_count") != 816) @compileError("Native ProgramRuntimePayload.r4xstart_import_count offset drift");
    if (@offsetOf(ProgramRuntimePayload, "r4xstart_import_module_names") != 952) @compileError("Native ProgramRuntimePayload.r4xstart_import_module_names offset drift");
    if (@offsetOf(ProgramRuntimePayload, "r4xstart_import_symbol_names") != 1464) @compileError("Native ProgramRuntimePayload.r4xstart_import_symbol_names offset drift");
    if (@offsetOf(ProgramRuntimePayload, "r4l_code_bindings") != 820) @compileError("Native ProgramRuntimePayload.r4l_code_bindings offset drift");
    if (@offsetOf(ProgramRuntimePayload, "r4l_code_binding_count") != 1976) @compileError("Native ProgramRuntimePayload.r4l_code_binding_count offset drift");
    if (@offsetOf(ProgramRuntimePayload, "module_path") != 1977) @compileError("Native ProgramRuntimePayload.module_path offset drift");
    if (@offsetOf(ProgramRuntimePayload, "module_path_len") != 948) @compileError("Native ProgramRuntimePayload.module_path_len offset drift");
    if (@sizeOf(ProgramEnvironmentPayload) != 2064 or @alignOf(ProgramEnvironmentPayload) != 4) @compileError("Native ProgramEnvironmentPayload original geometry drift");
    if (@offsetOf(ProgramEnvironmentPayload, "header") != 0) @compileError("Native ProgramEnvironmentPayload.header offset drift");
    if (@offsetOf(ProgramEnvironmentPayload, "bytes") != 16) @compileError("Native ProgramEnvironmentPayload.bytes offset drift");
    if (@sizeOf(ProgramProcessPayload) != 1200 or @alignOf(ProgramProcessPayload) != 8) @compileError("Native ProgramProcessPayload original geometry drift");
    if (@offsetOf(ProgramProcessPayload, "header") != 24) @compileError("Native ProgramProcessPayload.header offset drift");
    if (@offsetOf(ProgramProcessPayload, "environment_payload") != 0) @compileError("Native ProgramProcessPayload.environment_payload offset drift");
    if (@offsetOf(ProgramProcessPayload, "environment_len") != 8) @compileError("Native ProgramProcessPayload.environment_len offset drift");
    if (@offsetOf(ProgramProcessPayload, "args") != 40) @compileError("Native ProgramProcessPayload.args offset drift");
    if (@offsetOf(ProgramProcessPayload, "work_drive_letter") != 168) @compileError("Native ProgramProcessPayload.work_drive_letter offset drift");
    if (@offsetOf(ProgramProcessPayload, "work_cwd_len") != 16) @compileError("Native ProgramProcessPayload.work_cwd_len offset drift");
    if (@offsetOf(ProgramProcessPayload, "work_cwd") != 169) @compileError("Native ProgramProcessPayload.work_cwd offset drift");
    if (@sizeOf(ProgramConsoleOutputPayload) != 16424 or @alignOf(ProgramConsoleOutputPayload) != 8) @compileError("Native ProgramConsoleOutputPayload original geometry drift");
    if (@offsetOf(ProgramConsoleOutputPayload, "header") != 8) @compileError("Native ProgramConsoleOutputPayload.header offset drift");
    if (@offsetOf(ProgramConsoleOutputPayload, "ref_count") != 24) @compileError("Native ProgramConsoleOutputPayload.ref_count offset drift");
    if (@offsetOf(ProgramConsoleOutputPayload, "active_ref_count") != 28) @compileError("Native ProgramConsoleOutputPayload.active_ref_count offset drift");
    if (@offsetOf(ProgramConsoleOutputPayload, "next_sequence") != 0) @compileError("Native ProgramConsoleOutputPayload.next_sequence offset drift");
    if (@offsetOf(ProgramConsoleOutputPayload, "active_accounted") != 32) @compileError("Native ProgramConsoleOutputPayload.active_accounted offset drift");
    if (@offsetOf(ProgramConsoleOutputPayload, "reserved") != 33) @compileError("Native ProgramConsoleOutputPayload.reserved offset drift");
    if (@offsetOf(ProgramConsoleOutputPayload, "bytes") != 40) @compileError("Native ProgramConsoleOutputPayload.bytes offset drift");
    if (@sizeOf(ProgramConsoleOutputSegment) != 24 or @alignOf(ProgramConsoleOutputSegment) != 8) @compileError("Native ProgramConsoleOutputSegment original geometry drift");
    if (@offsetOf(ProgramConsoleOutputSegment, "payload") != 0) @compileError("Native ProgramConsoleOutputSegment.payload offset drift");
    if (@offsetOf(ProgramConsoleOutputSegment, "start_sequence") != 8) @compileError("Native ProgramConsoleOutputSegment.start_sequence offset drift");
    if (@offsetOf(ProgramConsoleOutputSegment, "length") != 16) @compileError("Native ProgramConsoleOutputSegment.length offset drift");
    if (@offsetOf(ProgramConsoleOutputSegment, "reserved") != 20) @compileError("Native ProgramConsoleOutputSegment.reserved offset drift");
    if (@sizeOf(ProgramConsoleTranscriptPayload) != 6168 or @alignOf(ProgramConsoleTranscriptPayload) != 8) @compileError("Native ProgramConsoleTranscriptPayload original geometry drift");
    if (@offsetOf(ProgramConsoleTranscriptPayload, "header") != 6144) @compileError("Native ProgramConsoleTranscriptPayload.header offset drift");
    if (@offsetOf(ProgramConsoleTranscriptPayload, "segment_count") != 6160) @compileError("Native ProgramConsoleTranscriptPayload.segment_count offset drift");
    if (@offsetOf(ProgramConsoleTranscriptPayload, "output_len") != 6164) @compileError("Native ProgramConsoleTranscriptPayload.output_len offset drift");
    if (@offsetOf(ProgramConsoleTranscriptPayload, "segments") != 0) @compileError("Native ProgramConsoleTranscriptPayload.segments offset drift");
    if (@sizeOf(ProgramConsolePayload) != 4368 or @alignOf(ProgramConsolePayload) != 8) @compileError("Native ProgramConsolePayload original geometry drift");
    if (@offsetOf(ProgramConsolePayload, "header") != 176) @compileError("Native ProgramConsolePayload.header offset drift");
    if (@offsetOf(ProgramConsolePayload, "transcript_payload") != 0) @compileError("Native ProgramConsolePayload.transcript_payload offset drift");
    if (@offsetOf(ProgramConsolePayload, "writer_payload") != 8) @compileError("Native ProgramConsolePayload.writer_payload offset drift");
    if (@offsetOf(ProgramConsolePayload, "input_queue") != 272) @compileError("Native ProgramConsolePayload.input_queue offset drift");
    if (@offsetOf(ProgramConsolePayload, "input_head") != 16) @compileError("Native ProgramConsolePayload.input_head offset drift");
    if (@offsetOf(ProgramConsolePayload, "input_tail") != 24) @compileError("Native ProgramConsolePayload.input_tail offset drift");
    if (@offsetOf(ProgramConsolePayload, "input_high_water") != 192) @compileError("Native ProgramConsolePayload.input_high_water offset drift");
    if (@offsetOf(ProgramConsolePayload, "input_generation") != 32) @compileError("Native ProgramConsolePayload.input_generation offset drift");
    if (@offsetOf(ProgramConsolePayload, "input_lock") != 40) @compileError("Native ProgramConsolePayload.input_lock offset drift");
    if (@offsetOf(ProgramConsolePayload, "input_wait") != 128) @compileError("Native ProgramConsolePayload.input_wait offset drift");
    if (@offsetOf(ProgramConsolePayload, "revision") != 196) @compileError("Native ProgramConsolePayload.revision offset drift");
    if (@offsetOf(ProgramConsolePayload, "host") != 200) @compileError("Native ProgramConsolePayload.host offset drift");
    if (@offsetOf(ProgramConsolePayload, "io_target_id") != 204) @compileError("Native ProgramConsolePayload.io_target_id offset drift");
    if (@offsetOf(ProgramConsolePayload, "io_target_generation") != 168) @compileError("Native ProgramConsolePayload.io_target_generation offset drift");
    if (@offsetOf(ProgramConsolePayload, "state") != 208) @compileError("Native ProgramConsolePayload.state offset drift");
    if (@sizeOf(ProgramGuiSharedRasterRef) != 32 or @alignOf(ProgramGuiSharedRasterRef) != 8) @compileError("Native ProgramGuiSharedRasterRef original geometry drift");
    if (@offsetOf(ProgramGuiSharedRasterRef, "handle") != 0) @compileError("Native ProgramGuiSharedRasterRef.handle offset drift");
    if (@offsetOf(ProgramGuiSharedRasterRef, "raster_generation") != 16) @compileError("Native ProgramGuiSharedRasterRef.raster_generation offset drift");
    if (@offsetOf(ProgramGuiSharedRasterRef, "data_bytes") != 24) @compileError("Native ProgramGuiSharedRasterRef.data_bytes offset drift");
    if (@sizeOf(ProgramGuiCommand) != 112 or @alignOf(ProgramGuiCommand) != 8) @compileError("Native ProgramGuiCommand original geometry drift");
    if (@offsetOf(ProgramGuiCommand, "version") != 40) @compileError("Native ProgramGuiCommand.version offset drift");
    if (@offsetOf(ProgramGuiCommand, "size") != 44) @compileError("Native ProgramGuiCommand.size offset drift");
    if (@offsetOf(ProgramGuiCommand, "kind") != 48) @compileError("Native ProgramGuiCommand.kind offset drift");
    if (@offsetOf(ProgramGuiCommand, "flags") != 52) @compileError("Native ProgramGuiCommand.flags offset drift");
    if (@offsetOf(ProgramGuiCommand, "x") != 56) @compileError("Native ProgramGuiCommand.x offset drift");
    if (@offsetOf(ProgramGuiCommand, "y") != 60) @compileError("Native ProgramGuiCommand.y offset drift");
    if (@offsetOf(ProgramGuiCommand, "w") != 64) @compileError("Native ProgramGuiCommand.w offset drift");
    if (@offsetOf(ProgramGuiCommand, "h") != 68) @compileError("Native ProgramGuiCommand.h offset drift");
    if (@offsetOf(ProgramGuiCommand, "rgb") != 72) @compileError("Native ProgramGuiCommand.rgb offset drift");
    if (@offsetOf(ProgramGuiCommand, "fg") != 76) @compileError("Native ProgramGuiCommand.fg offset drift");
    if (@offsetOf(ProgramGuiCommand, "bg") != 80) @compileError("Native ProgramGuiCommand.bg offset drift");
    if (@offsetOf(ProgramGuiCommand, "font_id") != 84) @compileError("Native ProgramGuiCommand.font_id offset drift");
    if (@offsetOf(ProgramGuiCommand, "text_w") != 88) @compileError("Native ProgramGuiCommand.text_w offset drift");
    if (@offsetOf(ProgramGuiCommand, "text_h") != 92) @compileError("Native ProgramGuiCommand.text_h offset drift");
    if (@offsetOf(ProgramGuiCommand, "baseline") != 96) @compileError("Native ProgramGuiCommand.baseline offset drift");
    if (@offsetOf(ProgramGuiCommand, "line_height") != 100) @compileError("Native ProgramGuiCommand.line_height offset drift");
    if (@offsetOf(ProgramGuiCommand, "payload_offset") != 0) @compileError("Native ProgramGuiCommand.payload_offset offset drift");
    if (@offsetOf(ProgramGuiCommand, "payload_bytes") != 8) @compileError("Native ProgramGuiCommand.payload_bytes offset drift");
    if (@offsetOf(ProgramGuiCommand, "parameter0") != 16) @compileError("Native ProgramGuiCommand.parameter0 offset drift");
    if (@offsetOf(ProgramGuiCommand, "parameter1") != 24) @compileError("Native ProgramGuiCommand.parameter1 offset drift");
    if (@offsetOf(ProgramGuiCommand, "resource_kind") != 104) @compileError("Native ProgramGuiCommand.resource_kind offset drift");
    if (@offsetOf(ProgramGuiCommand, "reserved") != 106) @compileError("Native ProgramGuiCommand.reserved offset drift");
    if (@offsetOf(ProgramGuiCommand, "raster_word_offset") != 32) @compileError("Native ProgramGuiCommand.raster_word_offset offset drift");
    if (@sizeOf(ProgramGuiCommandPayload) != 56 or @alignOf(ProgramGuiCommandPayload) != 8) @compileError("Native ProgramGuiCommandPayload original geometry drift");
    if (@offsetOf(ProgramGuiCommandPayload, "header") != 32) @compileError("Native ProgramGuiCommandPayload.header offset drift");
    if (@offsetOf(ProgramGuiCommandPayload, "previous") != 0) @compileError("Native ProgramGuiCommandPayload.previous offset drift");
    if (@offsetOf(ProgramGuiCommandPayload, "next") != 8) @compileError("Native ProgramGuiCommandPayload.next offset drift");
    if (@offsetOf(ProgramGuiCommandPayload, "allocation_sequence") != 16) @compileError("Native ProgramGuiCommandPayload.allocation_sequence offset drift");
    if (@offsetOf(ProgramGuiCommandPayload, "logical_offset") != 24) @compileError("Native ProgramGuiCommandPayload.logical_offset offset drift");
    if (@offsetOf(ProgramGuiCommandPayload, "command_count") != 48) @compileError("Native ProgramGuiCommandPayload.command_count offset drift");
    if (@offsetOf(ProgramGuiCommandPayload, "capacity") != 52) @compileError("Native ProgramGuiCommandPayload.capacity offset drift");
    if (@sizeOf(ProgramGuiResourcePayload) != 64 or @alignOf(ProgramGuiResourcePayload) != 8) @compileError("Native ProgramGuiResourcePayload original geometry drift");
    if (@offsetOf(ProgramGuiResourcePayload, "header") != 40) @compileError("Native ProgramGuiResourcePayload.header offset drift");
    if (@offsetOf(ProgramGuiResourcePayload, "previous") != 0) @compileError("Native ProgramGuiResourcePayload.previous offset drift");
    if (@offsetOf(ProgramGuiResourcePayload, "next") != 8) @compileError("Native ProgramGuiResourcePayload.next offset drift");
    if (@offsetOf(ProgramGuiResourcePayload, "allocation_sequence") != 16) @compileError("Native ProgramGuiResourcePayload.allocation_sequence offset drift");
    if (@offsetOf(ProgramGuiResourcePayload, "logical_offset") != 24) @compileError("Native ProgramGuiResourcePayload.logical_offset offset drift");
    if (@offsetOf(ProgramGuiResourcePayload, "raster_word_offset") != 32) @compileError("Native ProgramGuiResourcePayload.raster_word_offset offset drift");
    if (@offsetOf(ProgramGuiResourcePayload, "byte_count") != 56) @compileError("Native ProgramGuiResourcePayload.byte_count offset drift");
    if (@offsetOf(ProgramGuiResourcePayload, "resource_kind") != 60) @compileError("Native ProgramGuiResourcePayload.resource_kind offset drift");
    if (@offsetOf(ProgramGuiResourcePayload, "reserved") != 62) @compileError("Native ProgramGuiResourcePayload.reserved offset drift");
    if (@sizeOf(ProgramGuiFramePayload) != 256 or @alignOf(ProgramGuiFramePayload) != 8) @compileError("Native ProgramGuiFramePayload original geometry drift");
    if (@offsetOf(ProgramGuiFramePayload, "header") != 88) @compileError("Native ProgramGuiFramePayload.header offset drift");
    if (@offsetOf(ProgramGuiFramePayload, "command_payload") != 0) @compileError("Native ProgramGuiFramePayload.command_payload offset drift");
    if (@offsetOf(ProgramGuiFramePayload, "command_tail") != 8) @compileError("Native ProgramGuiFramePayload.command_tail offset drift");
    if (@offsetOf(ProgramGuiFramePayload, "resource_payload") != 16) @compileError("Native ProgramGuiFramePayload.resource_payload offset drift");
    if (@offsetOf(ProgramGuiFramePayload, "resource_tail") != 24) @compileError("Native ProgramGuiFramePayload.resource_tail offset drift");
    if (@offsetOf(ProgramGuiFramePayload, "retired_next") != 32) @compileError("Native ProgramGuiFramePayload.retired_next offset drift");
    if (@offsetOf(ProgramGuiFramePayload, "base_frame") != 40) @compileError("Native ProgramGuiFramePayload.base_frame offset drift");
    if (@offsetOf(ProgramGuiFramePayload, "command_count") != 48) @compileError("Native ProgramGuiFramePayload.command_count offset drift");
    if (@offsetOf(ProgramGuiFramePayload, "resource_len") != 56) @compileError("Native ProgramGuiFramePayload.resource_len offset drift");
    if (@offsetOf(ProgramGuiFramePayload, "raster_words") != 64) @compileError("Native ProgramGuiFramePayload.raster_words offset drift");
    if (@offsetOf(ProgramGuiFramePayload, "node_sequence") != 72) @compileError("Native ProgramGuiFramePayload.node_sequence offset drift");
    if (@offsetOf(ProgramGuiFramePayload, "generation") != 80) @compileError("Native ProgramGuiFramePayload.generation offset drift");
    if (@offsetOf(ProgramGuiFramePayload, "damage_regions") != 104) @compileError("Native ProgramGuiFramePayload.damage_regions offset drift");
    if (@offsetOf(ProgramGuiFramePayload, "damage_count") != 232) @compileError("Native ProgramGuiFramePayload.damage_count offset drift");
    if (@offsetOf(ProgramGuiFramePayload, "chain_depth") != 236) @compileError("Native ProgramGuiFramePayload.chain_depth offset drift");
    if (@offsetOf(ProgramGuiFramePayload, "reader_refs") != 240) @compileError("Native ProgramGuiFramePayload.reader_refs offset drift");
    if (@offsetOf(ProgramGuiFramePayload, "shared_raster_count") != 244) @compileError("Native ProgramGuiFramePayload.shared_raster_count offset drift");
    if (@offsetOf(ProgramGuiFramePayload, "build_failed") != 248) @compileError("Native ProgramGuiFramePayload.build_failed offset drift");
    if (@offsetOf(ProgramGuiFramePayload, "explicit_build") != 249) @compileError("Native ProgramGuiFramePayload.explicit_build offset drift");
    if (@offsetOf(ProgramGuiFramePayload, "replacement") != 250) @compileError("Native ProgramGuiFramePayload.replacement offset drift");
    if (@offsetOf(ProgramGuiFramePayload, "retired") != 251) @compileError("Native ProgramGuiFramePayload.retired offset drift");
    if (@sizeOf(ProgramGuiPayload) != 3848 or @alignOf(ProgramGuiPayload) != 8) @compileError("Native ProgramGuiPayload original geometry drift");
    if (@offsetOf(ProgramGuiPayload, "operation_gate") != 2808) @compileError("Native ProgramGuiPayload.operation_gate offset drift");
    if (@offsetOf(ProgramGuiPayload, "header") != 2920) @compileError("Native ProgramGuiPayload.header offset drift");
    if (@offsetOf(ProgramGuiPayload, "frame_lock") != 16) @compileError("Native ProgramGuiPayload.frame_lock offset drift");
    if (@offsetOf(ProgramGuiPayload, "committed_frame") != 104) @compileError("Native ProgramGuiPayload.committed_frame offset drift");
    if (@offsetOf(ProgramGuiPayload, "building_frame") != 112) @compileError("Native ProgramGuiPayload.building_frame offset drift");
    if (@offsetOf(ProgramGuiPayload, "retired_frames") != 120) @compileError("Native ProgramGuiPayload.retired_frames offset drift");
    if (@offsetOf(ProgramGuiPayload, "window_id") != 3000) @compileError("Native ProgramGuiPayload.window_id offset drift");
    if (@offsetOf(ProgramGuiPayload, "window_info") != 2960) @compileError("Native ProgramGuiPayload.window_info offset drift");
    if (@offsetOf(ProgramGuiPayload, "events") != 144) @compileError("Native ProgramGuiPayload.events offset drift");
    if (@offsetOf(ProgramGuiPayload, "event_head") != 2704) @compileError("Native ProgramGuiPayload.event_head offset drift");
    if (@offsetOf(ProgramGuiPayload, "event_tail") != 2712) @compileError("Native ProgramGuiPayload.event_tail offset drift");
    if (@offsetOf(ProgramGuiPayload, "event_high_water") != 2948) @compileError("Native ProgramGuiPayload.event_high_water offset drift");
    if (@offsetOf(ProgramGuiPayload, "text") != 3332) @compileError("Native ProgramGuiPayload.text offset drift");
    if (@offsetOf(ProgramGuiPayload, "font_id") != 2952) @compileError("Native ProgramGuiPayload.font_id offset drift");
    if (@offsetOf(ProgramGuiPayload, "title") != 3268) @compileError("Native ProgramGuiPayload.title offset drift");
    if (@offsetOf(ProgramGuiPayload, "min_client_w") != 2944) @compileError("Native ProgramGuiPayload.min_client_w offset drift");
    if (@offsetOf(ProgramGuiPayload, "min_client_h") != 2940) @compileError("Native ProgramGuiPayload.min_client_h offset drift");
    if (@offsetOf(ProgramGuiPayload, "revision") != 2936) @compileError("Native ProgramGuiPayload.revision offset drift");
    if (@offsetOf(ProgramGuiPayload, "frame_commits") != 2776) @compileError("Native ProgramGuiPayload.frame_commits offset drift");
    if (@offsetOf(ProgramGuiPayload, "frame_cancels") != 2784) @compileError("Native ProgramGuiPayload.frame_cancels offset drift");
    if (@offsetOf(ProgramGuiPayload, "frame_oom") != 0) @compileError("Native ProgramGuiPayload.frame_oom offset drift");
    if (@offsetOf(ProgramGuiPayload, "frame_snapshot_reads") != 2720) @compileError("Native ProgramGuiPayload.frame_snapshot_reads offset drift");
    if (@offsetOf(ProgramGuiPayload, "frame_generation_reads") != 2800) @compileError("Native ProgramGuiPayload.frame_generation_reads offset drift");
    if (@offsetOf(ProgramGuiPayload, "frame_delta_commits") != 2888) @compileError("Native ProgramGuiPayload.frame_delta_commits offset drift");
    if (@offsetOf(ProgramGuiPayload, "frame_full_commits") != 2896) @compileError("Native ProgramGuiPayload.frame_full_commits offset drift");
    if (@offsetOf(ProgramGuiPayload, "frame_indexed8_commands") != 2904) @compileError("Native ProgramGuiPayload.frame_indexed8_commands offset drift");
    if (@offsetOf(ProgramGuiPayload, "frame_indexed8_resource_bytes") != 2912) @compileError("Native ProgramGuiPayload.frame_indexed8_resource_bytes offset drift");
    if (@offsetOf(ProgramGuiPayload, "frame_replacement_commits") != 2792) @compileError("Native ProgramGuiPayload.frame_replacement_commits offset drift");
    if (@offsetOf(ProgramGuiPayload, "frame_superseded_generations") != 2768) @compileError("Native ProgramGuiPayload.frame_superseded_generations offset drift");
    if (@offsetOf(ProgramGuiPayload, "frame_coalesced_generations") != 2760) @compileError("Native ProgramGuiPayload.frame_coalesced_generations offset drift");
    if (@offsetOf(ProgramGuiPayload, "frame_reader_retired_generations") != 2752) @compileError("Native ProgramGuiPayload.frame_reader_retired_generations offset drift");
    if (@offsetOf(ProgramGuiPayload, "frame_xrgb32_nearest_commands") != 8) @compileError("Native ProgramGuiPayload.frame_xrgb32_nearest_commands offset drift");
    if (@offsetOf(ProgramGuiPayload, "frame_xrgb32_nearest_resource_bytes") != 2744) @compileError("Native ProgramGuiPayload.frame_xrgb32_nearest_resource_bytes offset drift");
    if (@offsetOf(ProgramGuiPayload, "frame_stream_peak_bytes") != 2736) @compileError("Native ProgramGuiPayload.frame_stream_peak_bytes offset drift");
    if (@offsetOf(ProgramGuiPayload, "frame_avoided_clone_bytes") != 2728) @compileError("Native ProgramGuiPayload.frame_avoided_clone_bytes offset drift");
    if (@offsetOf(ProgramGuiPayload, "frame_last_error") != 2956) @compileError("Native ProgramGuiPayload.frame_last_error offset drift");
    if (@offsetOf(ProgramGuiPayload, "frame_peak_bytes") != 136) @compileError("Native ProgramGuiPayload.frame_peak_bytes offset drift");
    if (@offsetOf(ProgramGuiPayload, "frame_peak_commands") != 128) @compileError("Native ProgramGuiPayload.frame_peak_commands offset drift");
    if (@offsetOf(ProgramGuiPayload, "start_attach_pending") != 3844) @compileError("Native ProgramGuiPayload.start_attach_pending offset drift");
    if (@offsetOf(ProgramGuiPayload, "host_launch_pending") != 3845) @compileError("Native ProgramGuiPayload.host_launch_pending offset drift");
    if (@offsetOf(ProgramGuiPayload, "host_launch_request") != 3004) @compileError("Native ProgramGuiPayload.host_launch_request offset drift");
    if (@sizeOf(ProgramInstanceStorage) != 32 or @alignOf(ProgramInstanceStorage) != 8) @compileError("Native ProgramInstanceStorage original geometry drift");
    if (@offsetOf(ProgramInstanceStorage, "runtime") != 0) @compileError("Native ProgramInstanceStorage.runtime offset drift");
    if (@offsetOf(ProgramInstanceStorage, "process") != 8) @compileError("Native ProgramInstanceStorage.process offset drift");
    if (@offsetOf(ProgramInstanceStorage, "console") != 16) @compileError("Native ProgramInstanceStorage.console offset drift");
    if (@offsetOf(ProgramInstanceStorage, "gui") != 24) @compileError("Native ProgramInstanceStorage.gui offset drift");
    if (@sizeOf(R4LCodeBinding) != 8 or @alignOf(R4LCodeBinding) != 4) @compileError("Native R4LCodeBinding original geometry drift");
    if (@offsetOf(R4LCodeBinding, "module_slot") != 4) @compileError("Native R4LCodeBinding.module_slot offset drift");
    if (@offsetOf(R4LCodeBinding, "reserved") != 5) @compileError("Native R4LCodeBinding.reserved offset drift");
    if (@offsetOf(R4LCodeBinding, "generation") != 0) @compileError("Native R4LCodeBinding.generation offset drift");
    if (@sizeOf(ProgramInstanceStorageStats) != 336 or @alignOf(ProgramInstanceStorageStats) != 8) @compileError("Native ProgramInstanceStorageStats original geometry drift");
    if (@offsetOf(ProgramInstanceStorageStats, "core_bytes_per_instance") != 208) @compileError("Native ProgramInstanceStorageStats.core_bytes_per_instance offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "registry_reserved_core_bytes") != 192) @compileError("Native ProgramInstanceStorageStats.registry_reserved_core_bytes offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "live_core_bytes") != 16) @compileError("Native ProgramInstanceStorageStats.live_core_bytes offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "active_instance_bytes") != 24) @compileError("Native ProgramInstanceStorageStats.active_instance_bytes offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "peak_active_instance_bytes") != 32) @compileError("Native ProgramInstanceStorageStats.peak_active_instance_bytes offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "reserved_instance_bytes") != 40) @compileError("Native ProgramInstanceStorageStats.reserved_instance_bytes offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "peak_reserved_instance_bytes") != 48) @compileError("Native ProgramInstanceStorageStats.peak_reserved_instance_bytes offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "current_payload_bytes") != 56) @compileError("Native ProgramInstanceStorageStats.current_payload_bytes offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "peak_payload_bytes") != 64) @compileError("Native ProgramInstanceStorageStats.peak_payload_bytes offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "current_runtime_bytes") != 72) @compileError("Native ProgramInstanceStorageStats.current_runtime_bytes offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "peak_runtime_bytes") != 80) @compileError("Native ProgramInstanceStorageStats.peak_runtime_bytes offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "current_console_bytes") != 88) @compileError("Native ProgramInstanceStorageStats.current_console_bytes offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "peak_console_bytes") != 96) @compileError("Native ProgramInstanceStorageStats.peak_console_bytes offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "current_gui_bytes") != 104) @compileError("Native ProgramInstanceStorageStats.current_gui_bytes offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "peak_gui_bytes") != 112) @compileError("Native ProgramInstanceStorageStats.peak_gui_bytes offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "active_instances") != 332) @compileError("Native ProgramInstanceStorageStats.active_instances offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "active_service_instances") != 328) @compileError("Native ProgramInstanceStorageStats.active_service_instances offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "active_console_instances") != 324) @compileError("Native ProgramInstanceStorageStats.active_console_instances offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "active_gui_instances") != 320) @compileError("Native ProgramInstanceStorageStats.active_gui_instances offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "active_runtime_payloads") != 316) @compileError("Native ProgramInstanceStorageStats.active_runtime_payloads offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "active_process_payloads") != 312) @compileError("Native ProgramInstanceStorageStats.active_process_payloads offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "active_environment_payloads") != 308) @compileError("Native ProgramInstanceStorageStats.active_environment_payloads offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "active_console_payloads") != 300) @compileError("Native ProgramInstanceStorageStats.active_console_payloads offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "active_console_output_payloads") != 296) @compileError("Native ProgramInstanceStorageStats.active_console_output_payloads offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "active_gui_payloads") != 284) @compileError("Native ProgramInstanceStorageStats.active_gui_payloads offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "active_gui_frame_payloads") != 304) @compileError("Native ProgramInstanceStorageStats.active_gui_frame_payloads offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "active_gui_command_payloads") != 292) @compileError("Native ProgramInstanceStorageStats.active_gui_command_payloads offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "active_gui_raster_payloads") != 288) @compileError("Native ProgramInstanceStorageStats.active_gui_raster_payloads offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "active_gui_data_payloads") != 280) @compileError("Native ProgramInstanceStorageStats.active_gui_data_payloads offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "current_gui_frame_bytes") != 232) @compileError("Native ProgramInstanceStorageStats.current_gui_frame_bytes offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "peak_gui_frame_bytes") != 240) @compileError("Native ProgramInstanceStorageStats.peak_gui_frame_bytes offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "current_gui_frame_commands") != 248) @compileError("Native ProgramInstanceStorageStats.current_gui_frame_commands offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "peak_gui_frame_commands") != 256) @compileError("Native ProgramInstanceStorageStats.peak_gui_frame_commands offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "current_gui_frame_nodes") != 264) @compileError("Native ProgramInstanceStorageStats.current_gui_frame_nodes offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "peak_gui_frame_nodes") != 272) @compileError("Native ProgramInstanceStorageStats.peak_gui_frame_nodes offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "gui_frame_commits") != 224) @compileError("Native ProgramInstanceStorageStats.gui_frame_commits offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "gui_frame_cancels") != 0) @compileError("Native ProgramInstanceStorageStats.gui_frame_cancels offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "gui_frame_oom_failures") != 216) @compileError("Native ProgramInstanceStorageStats.gui_frame_oom_failures offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "gui_frame_snapshot_reads") != 200) @compileError("Native ProgramInstanceStorageStats.gui_frame_snapshot_reads offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "allocation_attempts") != 184) @compileError("Native ProgramInstanceStorageStats.allocation_attempts offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "payload_allocations") != 176) @compileError("Native ProgramInstanceStorageStats.payload_allocations offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "payload_releases") != 8) @compileError("Native ProgramInstanceStorageStats.payload_releases offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "allocation_failures") != 168) @compileError("Native ProgramInstanceStorageStats.allocation_failures offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "transaction_rollbacks") != 160) @compileError("Native ProgramInstanceStorageStats.transaction_rollbacks offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "owner_mismatches") != 152) @compileError("Native ProgramInstanceStorageStats.owner_mismatches offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "header_errors") != 144) @compileError("Native ProgramInstanceStorageStats.header_errors offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "free_failures") != 136) @compileError("Native ProgramInstanceStorageStats.free_failures offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "quarantined_payloads") != 128) @compileError("Native ProgramInstanceStorageStats.quarantined_payloads offset drift");
    if (@offsetOf(ProgramInstanceStorageStats, "quarantined_bytes") != 120) @compileError("Native ProgramInstanceStorageStats.quarantined_bytes offset drift");
    if (@sizeOf(ProgramInstanceStorageSelfTestReport) != 24 or @alignOf(ProgramInstanceStorageSelfTestReport) != 8) @compileError("Native ProgramInstanceStorageSelfTestReport original geometry drift");
    if (@offsetOf(ProgramInstanceStorageSelfTestReport, "cases") != 8) @compileError("Native ProgramInstanceStorageSelfTestReport.cases offset drift");
    if (@offsetOf(ProgramInstanceStorageSelfTestReport, "failed_case") != 12) @compileError("Native ProgramInstanceStorageSelfTestReport.failed_case offset drift");
    if (@offsetOf(ProgramInstanceStorageSelfTestReport, "peak_payload_bytes") != 0) @compileError("Native ProgramInstanceStorageSelfTestReport.peak_payload_bytes offset drift");
    if (@offsetOf(ProgramInstanceStorageSelfTestReport, "heap_baseline_ok") != 16) @compileError("Native ProgramInstanceStorageSelfTestReport.heap_baseline_ok offset drift");
    if (@offsetOf(ProgramInstanceStorageSelfTestReport, "storage_baseline_ok") != 17) @compileError("Native ProgramInstanceStorageSelfTestReport.storage_baseline_ok offset drift");
    if (@offsetOf(ProgramInstanceStorageSelfTestReport, "zero_init_ok") != 18) @compileError("Native ProgramInstanceStorageSelfTestReport.zero_init_ok offset drift");
    if (@offsetOf(NativeProgramPayloadState, "stats") != 0) @compileError("Native payload state field drift");
    if (@offsetOf(NativeProgramPayloadState, "report") != 336) @compileError("Native payload state field drift");
    if (@offsetOf(NativeProgramPayloadState, "active_published_bytes") != 360) @compileError("Native payload state field drift");
    if (@offsetOf(NativeProgramPayloadState, "self_test_peak_bytes") != 368) @compileError("Native payload state field drift");
    if (@offsetOf(NativeProgramPayloadState, "failure_after_value") != 376) @compileError("Native payload state field drift");
    if (@offsetOf(NativeProgramPayloadState, "failure_cursor") != 380) @compileError("Native payload state field drift");
    if (@offsetOf(NativeProgramPayloadState, "release_trace") != 384) @compileError("Native payload state field drift");
    if (@offsetOf(NativeProgramPayloadState, "release_trace_len") != 400) @compileError("Native payload state field drift");
    if (@offsetOf(NativeProgramPayloadState, "failure_after_present") != 408) @compileError("Native payload state field drift");
    if (@offsetOf(NativeProgramPayloadState, "self_test_active") != 409) @compileError("Native payload state field drift");
    if (@offsetOf(NativeProgramPayloadState, "self_test_running") != 410) @compileError("Native payload state field drift");
    if (@offsetOf(NativeProgramPayloadState, "release_trace_enabled") != 411) @compileError("Native payload state field drift");
    if (@sizeOf(NativeProgramPayloadState) != 416 or @sizeOf(NativeProgramPayloadSpan) != 16 or
        PROGRAM_PAYLOAD_MAGIC != 0x5034_3452 or @sizeOf(ProgramInstance) != 256 or sync.LockRank.program_instances != 100 or
        @intFromEnum(sync.LockMode.sleepable) != 0 or @intFromEnum(sync.LockMode.no_sleep) != 1 or
        CONSOLE_OUTPUT_SIZE != 16384 or CONSOLE_TRANSCRIPT_SEGMENTS != 256 or INPUT_QUEUE_SIZE != 4096 or GUI_EVENT_QUEUE_SIZE != 64 or
        GUI_TEXT_SIZE != 512 or GUI_TITLE_SIZE != 64 or GUI_FONT_BUILTIN_ID != 0 or ENVIRONMENT_BLOCK_MAX != 2048 or
        ARGS_MAX != 127 or drive.MAX_PATH != 1024 or MODULE_ORIGIN_MAX != 128 or MAX_R4M_IMPORTS != 16 or R4XSTART_IMPORT_NAME_BYTES != 32 or
        r4x_api.gui_frame_max_damage_regions != 8)
        @compileError("Native payload initialization/private constant contract drift");
    if (@intFromEnum(ProgramPayloadKind.runtime) != 1) @compileError("Native payload kind drift");
    if (@intFromEnum(ProgramPayloadKind.process) != 2) @compileError("Native payload kind drift");
    if (@intFromEnum(ProgramPayloadKind.environment) != 3) @compileError("Native payload kind drift");
    if (@intFromEnum(ProgramPayloadKind.console) != 4) @compileError("Native payload kind drift");
    if (@intFromEnum(ProgramPayloadKind.console_output) != 5) @compileError("Native payload kind drift");
    if (@intFromEnum(ProgramPayloadKind.gui) != 6) @compileError("Native payload kind drift");
    if (@intFromEnum(ProgramPayloadKind.gui_commands) != 7) @compileError("Native payload kind drift");
    if (@intFromEnum(ProgramPayloadKind.gui_raster) != 8) @compileError("Native payload kind drift");
    if (@intFromEnum(ProgramPayloadKind.gui_frame) != 9) @compileError("Native payload kind drift");
    if (@intFromEnum(ProgramPayloadKind.gui_frame_data) != 10) @compileError("Native payload kind drift");
    if (@intFromEnum(ProgramPayloadKind.console_transcript) != 11) @compileError("Native payload kind drift");
    if (@intFromEnum(ProgramGuiCommandResourceKind.none) != 0) @compileError("Native payload kind drift");
    if (@intFromEnum(ProgramGuiCommandResourceKind.utf8) != 1) @compileError("Native payload kind drift");
    if (@intFromEnum(ProgramGuiCommandResourceKind.xrgb32) != 2) @compileError("Native payload kind drift");
    if (@intFromEnum(ProgramGuiCommandResourceKind.alpha8) != 3) @compileError("Native payload kind drift");
    if (@intFromEnum(ProgramGuiCommandResourceKind.path) != 4) @compileError("Native payload kind drift");
    if (@intFromEnum(ProgramGuiCommandResourceKind.indexed8) != 5) @compileError("Native payload kind drift");
    if (@intFromEnum(ProgramGuiCommandResourceKind.xrgb32_nearest) != 6) @compileError("Native payload kind drift");
    if (@intFromEnum(ProgramGuiCommandResourceKind.shared_raster) != 7) @compileError("Native payload kind drift");
}


extern fn r4os_rust_program_data_ensure(*ProgramInstance, bool) ?*anyopaque;
extern fn r4os_rust_program_data_arguments(*ProgramInstance, NativeProgramBytes) void;
extern fn r4os_rust_program_data_inherit(*ProgramInstance, ?*const ProgramInstance) void;
extern fn r4os_rust_program_data_env_get(*const ProgramInstance, NativeProgramBytes, [*]u8, u32) i32;
extern fn r4os_rust_program_data_env_set(*ProgramInstance, NativeProgramBytes, NativeProgramBytes) i32;
extern fn r4os_rust_program_data_env_name(NativeProgramBytes) bool;
extern fn r4os_rust_program_data_cwd_get(*const ProgramInstance, [*]u8, u32) i32;
extern fn r4os_rust_program_data_cwd_set(*ProgramInstance, NativeProgramBytes) i32;
extern fn r4os_rust_program_data_cwd_copy(*ProgramInstance, ?*const ProgramInstance, *const drive.Drive) bool;
extern fn r4os_rust_program_data_local_owner() ?*local_context.Owner;
extern fn r4os_rust_program_data_local_cleanup(*ProgramInstance) bool;
extern fn r4os_rust_program_data_local(u64, u64, *u64, bool) i32;
extern fn r4os_rust_program_data_env_api_get([*:0]const u8, [*]u8, u32) i32;
extern fn r4os_rust_program_data_env_api_set([*:0]const u8, [*]const u8, u32) i32;
extern fn r4os_rust_program_data_canonical(NativeProgramBytes, *[MAX_API_PATH]u8, *usize) bool;
extern fn r4os_rust_program_data_copy_z([*:0]const u8, [*]u8, usize, *usize) bool;
extern fn r4os_rust_program_data_copy_slice([*]u8, usize, NativeProgramBytes) void;
extern fn r4os_rust_program_data_equal(NativeProgramBytes, NativeProgramBytes) bool;
extern fn r4os_rust_program_data_upper(u8) u8;
extern fn r4os_rust_program_data_alpha(u8) bool;
export fn r4os_zig_program_data_file_info(path: [*:0]const u8, out: *r4x_api.FileInfo) callconv(.c) i32 { return r4api.r4sys.fileInfo(path, out); }
comptime {
    if (MAX_API_PATH != 1024 or drive.MAX_PATH != 1024 or ENVIRONMENT_NAME_MAX != 32 or ENVIRONMENT_VALUE_MAX != 512 or ENVIRONMENT_BLOCK_MAX != 2048 or
        ENV_OK != 0 or ENV_ERROR_INVALID != -1 or ENV_ERROR_NOT_FOUND != -2 or ENV_ERROR_BUFFER_TOO_SMALL != -3 or ENV_ERROR_UNSUPPORTED != -4 or ENV_ERROR_TOO_LONG != -5 or ENV_ERROR_NO_MEMORY != -6)
        @compileError("Native process argument/environment/path contract drift");
}


const NativeConsoleOutputState = extern struct {
    write_calls: u64,
    source_bytes: u64,
    visible_append_bytes: u64,
    capture_append_bytes: u64,
    shared_bytes: u64,
    revision_batches: u64,
    desktop_signals: u64,
    compactions: u64,
    compaction_bytes: u64,
    segment_drops: u64,
    segment_drop_bytes: u64,
    lock: sync.Mutex,
};
const NativeConsoleSourceByte = extern struct { payload: *ProgramConsoleOutputPayload, sequence: u64 };
extern fn r4os_rust_program_console_state() *NativeConsoleOutputState;
fn consoleOutputState() *NativeConsoleOutputState { return r4os_rust_program_console_state(); }
extern fn r4os_rust_program_console_output(u8, *ProgramConsoleOutputPayload, u32, bool) bool;
extern fn r4os_rust_program_console_allocate(u32) ?*ProgramConsoleOutputPayload;
extern fn r4os_rust_program_console_transcript(u8, *ProgramConsoleTranscriptPayload, u32, bool) bool;
extern fn r4os_rust_program_console_get(*const ProgramInstance) ?*ProgramConsoleTranscriptPayload;
extern fn r4os_rust_program_console_read(*const ProgramConsoleTranscriptPayload, usize, [*]u8, usize, bool) usize;
extern fn r4os_rust_program_console_append(*ProgramInstance, *ProgramConsoleOutputPayload, u64, bool) bool;
extern fn r4os_rust_program_console_source(*ProgramInstance, u8, *NativeConsoleSourceByte) bool;
extern fn r4os_rust_program_console_writer(*ProgramInstance) ?*ProgramConsoleOutputPayload;
extern fn r4os_rust_program_console_instance(u8, *ProgramInstance, u64) bool;
extern fn r4os_rust_program_console_pending(*ProgramConsolePayload) void;
extern fn r4os_rust_program_console_lines(*const ProgramConsoleTranscriptPayload, u32) u32;
extern fn r4os_rust_program_console_printable(u8) bool;
extern fn r4os_rust_program_console_stream(u32) bool;
extern fn r4os_rust_program_console_metric(u32, u32, u32) u32;
extern fn r4os_rust_program_console_route(u32, NativeProgramBytes, bool) bool;
extern fn r4os_rust_program_console_emit(*ProgramInstance, ?*ProgramInstance, u32, u8, bool, *bool, *bool) void;
extern fn r4os_rust_program_console_api_output(u32, [*]u8, u32) i32;
extern fn r4os_rust_program_console_api_revision(u32) u32;
extern fn r4os_rust_program_console_api_state(u32, *ConsoleState) i32;
extern fn r4os_rust_program_console_api_metrics(u32, u32, u32) i32;
extern fn r4os_rust_program_console_host(u8, u32, u32, ?*ConsoleState) i32;

export fn r4os_zig_program_console_sink(op: u8, a: u32, b: u32) callconv(.c) void {
    switch (op) {
        0 => k.serialPutcRaw(@intCast(a)),
        1 => k.consolePutcRaw(@intCast(a)),
        2 => k.clearConsole(),
        3 => k.setConsoleColors(a, b),
        4 => desktop_events.signal(),
        5 => bootlog.putc(@intCast(a)),
        else => unreachable,
    }
}
comptime {
    if (@offsetOf(NativeConsoleOutputState, "write_calls") != 0) @compileError("Native console state write_calls drift");
    if (@offsetOf(NativeConsoleOutputState, "source_bytes") != 8) @compileError("Native console state source_bytes drift");
    if (@offsetOf(NativeConsoleOutputState, "visible_append_bytes") != 16) @compileError("Native console state visible_append_bytes drift");
    if (@offsetOf(NativeConsoleOutputState, "capture_append_bytes") != 24) @compileError("Native console state capture_append_bytes drift");
    if (@offsetOf(NativeConsoleOutputState, "shared_bytes") != 32) @compileError("Native console state shared_bytes drift");
    if (@offsetOf(NativeConsoleOutputState, "revision_batches") != 40) @compileError("Native console state revision_batches drift");
    if (@offsetOf(NativeConsoleOutputState, "desktop_signals") != 48) @compileError("Native console state desktop_signals drift");
    if (@offsetOf(NativeConsoleOutputState, "compactions") != 56) @compileError("Native console state compactions drift");
    if (@offsetOf(NativeConsoleOutputState, "compaction_bytes") != 64) @compileError("Native console state compaction_bytes drift");
    if (@offsetOf(NativeConsoleOutputState, "segment_drops") != 72) @compileError("Native console state segment_drops drift");
    if (@offsetOf(NativeConsoleOutputState, "segment_drop_bytes") != 80) @compileError("Native console state segment_drop_bytes drift");

    if (@offsetOf(NativeConsoleOutputState, "lock") != 88 or @sizeOf(NativeConsoleSourceByte) != 16 or @offsetOf(NativeConsoleSourceByte, "sequence") != 8 or
        @intFromEnum(ConsoleHostKind.terminal_mode) != 2 or @intFromEnum(ConsoleStream.stdin) != 0 or @intFromEnum(ConsoleStream.stdout) != 1 or @intFromEnum(ConsoleStream.stderr) != 2 or
        @intFromEnum(InstanceRole.shell) != 1 or CONSOLE_OUTPUT_SIZE != 16384 or INPUT_QUEUE_SIZE != 4096 or CONSOLE_MIN_COLS != 1 or CONSOLE_MAX_COLS != 512 or CONSOLE_MIN_ROWS != 1 or CONSOLE_MAX_ROWS != 256)
        @compileError("Native console geometry/role/stream contract drift");
}


const NativeProgramAsyncExecution = extern struct { instance: ?*ProgramInstance, id: u32, reserved: u32, generation: u64 };
extern fn r4os_rust_program_context_current(bool) ?*ProgramInstance;
extern fn r4os_rust_program_context_memory_owner() u64;
extern fn r4os_rust_program_context_handle(*ProgramProcessHandle) bool;

const NativeConsoleInputState = extern struct {
    push_calls: u64,
    batch_calls: u64,
    bytes_attempted: u64,
    bytes_accepted: u64,
    full_events: u64,
    read_calls: u64,
    read_empty: u64,
    read_bytes: u64,
    wait_calls: u64,
    wait_blocks: u64,
    wait_immediate: u64,
    wait_wakes: u64,
    wait_timeouts: u64,
    wait_cancellations: u64,
    capture: NativeProgramBytes,
    capture_pos: usize,
    capture_present: bool,
};
extern fn r4os_rust_program_console_input_state() *NativeConsoleInputState;
fn consoleInputState() *NativeConsoleInputState { return r4os_rust_program_console_input_state(); }
extern fn r4os_rust_program_console_input_capture(NativeProgramBytes, bool) void;
extern fn r4os_rust_program_console_input_read(*u32, bool) bool;
extern fn r4os_rust_program_console_input_push(*ProgramInstance, NativeProgramBytes) usize;
extern fn r4os_rust_program_console_input_api_push(u32, [*]const u8, u32, bool) i32;
extern fn r4os_rust_program_console_input_api_read([*]u8, u32) i32;
extern fn r4os_rust_program_console_input_wait(u64, u64, *u64) i32;
extern fn r4os_rust_program_console_input_signal_payload(*ProgramConsolePayload) void;
extern fn r4os_rust_program_console_input_signal(*ProgramInstance) void;
extern fn r4os_rust_program_console_input_signal_handle(ProgramProcessHandle) void;
extern fn r4os_rust_program_console_input_current() ?*ProgramInstance;
extern fn r4os_rust_program_console_input_target(u32, ProgramProcessHandle, bool) ?*ProgramInstance;
extern fn r4os_rust_program_console_input_inherited(u8) u32;
extern fn r4os_rust_program_console_input_pop(*ProgramInstance, *u8) bool;
extern fn r4os_rust_program_console_keyboard_hook() u8;
extern fn r4os_rust_program_console_print_hook(u8) bool;
export fn r4os_zig_program_console_keyboard(op: u8) callconv(.c) u64 {
    return switch (op) {
        0 => keyboard.inputGeneration(),
        1 => @intFromBool(keyboard.pending()),
        2 => blk: { keyboard.signalInputActivity(); break :blk 0; },
        else => unreachable,
    };
}
export fn r4os_zig_program_console_keyboard_read(out: *u32) callconv(.c) bool {
    out.* = keyboard.readCodepoint() orelse return false;
    return true;
}
export fn r4os_zig_program_console_keyboard_wait(last: u64, timeout: u64, out: *u64) callconv(.c) u32 {
    return @intFromEnum(keyboard.waitInput(last, timeout, out));
}
comptime {
    if (@offsetOf(NativeConsoleInputState, "push_calls") != 0) @compileError("Native console input push_calls drift");
    if (@offsetOf(NativeConsoleInputState, "batch_calls") != 8) @compileError("Native console input batch_calls drift");
    if (@offsetOf(NativeConsoleInputState, "bytes_attempted") != 16) @compileError("Native console input bytes_attempted drift");
    if (@offsetOf(NativeConsoleInputState, "bytes_accepted") != 24) @compileError("Native console input bytes_accepted drift");
    if (@offsetOf(NativeConsoleInputState, "full_events") != 32) @compileError("Native console input full_events drift");
    if (@offsetOf(NativeConsoleInputState, "read_calls") != 40) @compileError("Native console input read_calls drift");
    if (@offsetOf(NativeConsoleInputState, "read_empty") != 48) @compileError("Native console input read_empty drift");
    if (@offsetOf(NativeConsoleInputState, "read_bytes") != 56) @compileError("Native console input read_bytes drift");
    if (@offsetOf(NativeConsoleInputState, "wait_calls") != 64) @compileError("Native console input wait_calls drift");
    if (@offsetOf(NativeConsoleInputState, "wait_blocks") != 72) @compileError("Native console input wait_blocks drift");
    if (@offsetOf(NativeConsoleInputState, "wait_immediate") != 80) @compileError("Native console input wait_immediate drift");
    if (@offsetOf(NativeConsoleInputState, "wait_wakes") != 88) @compileError("Native console input wait_wakes drift");
    if (@offsetOf(NativeConsoleInputState, "wait_timeouts") != 96) @compileError("Native console input wait_timeouts drift");
    if (@offsetOf(NativeConsoleInputState, "wait_cancellations") != 104) @compileError("Native console input wait_cancellations drift");

    if (@sizeOf(NativeConsoleInputState) != 144 or @offsetOf(NativeConsoleInputState, "capture") != 112 or @offsetOf(NativeConsoleInputState, "capture_pos") != 128 or @offsetOf(NativeConsoleInputState, "capture_present") != 136 or
        @sizeOf(NativeProgramAsyncExecution) != 24 or @offsetOf(NativeProgramAsyncExecution, "generation") != 16 or
        @intFromEnum(sync.WaitResult.none) != 0 or @intFromEnum(sync.WaitResult.signaled) != 1 or @intFromEnum(sync.WaitResult.timeout) != 2 or @intFromEnum(sync.WaitResult.cancelled) != 3 or @intFromEnum(sync.WaitResult.killed) != 4 or @intFromEnum(sync.WaitResult.failed) != 5)
        @compileError("Native input state/async projection/wait-result drift");
}


extern fn r4os_rust_program_gui_validate(u8, *const anyopaque, u32, bool) bool;
extern fn r4os_rust_program_gui_physical(?*const ProgramGuiResourcePayload) ?*const ProgramGuiResourcePayload;
extern fn r4os_rust_program_gui_count(u8, *const anyopaque) u64;
extern fn r4os_rust_program_gui_peak(*ProgramGuiPayload) void;
extern fn r4os_rust_program_gui_release(u8, *anyopaque, u32) bool;
extern fn r4os_rust_program_storage_allocate(u32, u8, bool, *ProgramInstanceStorage) bool;
extern fn r4os_rust_program_storage_transaction(u32, *ProgramInstanceStorage, bool) bool;
extern fn r4os_rust_program_storage_instance(u8, *ProgramInstance, ?*ProgramCompletionNode) bool;
extern fn r4os_rust_program_storage_free_output(*ProgramCompletionNode) void;
extern fn r4os_rust_program_storage_bytes(*const ProgramInstance) u64;




extern fn r4os_rust_program_gui_generation() u64;
extern fn r4os_rust_program_gui_allocate_frame(u32, bool) ?*ProgramGuiFramePayload;
extern fn r4os_rust_program_gui_reservation_info(u8, *ProgramGuiFramePayload, u64) u64;
extern fn r4os_rust_program_gui_prepare(u8, *ProgramInstance, *ProgramGuiFramePayload, usize, u16, *anyopaque) bool;
extern fn r4os_rust_program_gui_cancel(u8, *ProgramInstance, *anyopaque) void;
extern fn r4os_rust_program_gui_commit(u8, *anyopaque, ?*const ProgramGuiCommand) i32;
extern fn r4os_rust_program_gui_lock(*ProgramGuiPayload, bool) void;
comptime {
    if (@sizeOf(GuiCommandStorage) != 32 or @alignOf(GuiCommandStorage) != 8 or @offsetOf(GuiCommandStorage, "gui") != 0 or @offsetOf(GuiCommandStorage, "frame") != 8 or @offsetOf(GuiCommandStorage, "payload") != 16 or @offsetOf(GuiCommandStorage, "new_payload") != 24)
        @compileError("Native command reservation original geometry drift");
    if (@sizeOf(GuiResourceStorage) != 40 or @alignOf(GuiResourceStorage) != 8 or @offsetOf(GuiResourceStorage, "gui") != 0 or @offsetOf(GuiResourceStorage, "frame") != 8 or @offsetOf(GuiResourceStorage, "payload") != 16 or @offsetOf(GuiResourceStorage, "next_resource_len") != 24 or @offsetOf(GuiResourceStorage, "next_raster_words") != 32)
        @compileError("Native resource reservation original geometry drift");
    if (@sizeOf(GuiBlitStorage) != 72 or @alignOf(GuiBlitStorage) != 8 or @offsetOf(GuiBlitStorage, "command") != 0 or @offsetOf(GuiBlitStorage, "resource") != 32 or GUI_COMMAND_BLOCK_INITIAL_CAPACITY != 8 or GUI_COMMAND_BLOCK_TARGET_CAPACITY != 128 or GUI_RASTER_NODE_MAX_WORDS != 65536)
        @compileError("Native blit reservation original geometry/capacity drift");
}


extern fn r4os_rust_program_gui_capture(*ProgramInstance, u64, bool, bool, *GuiFrameCapture) bool;
extern fn r4os_rust_program_gui_release_capture(*ProgramInstance, *GuiFrameCapture) void;
extern fn r4os_rust_program_gui_remove_retired(*ProgramGuiPayload, *ProgramGuiFramePayload, bool) ?*ProgramGuiFramePayload;
extern fn r4os_rust_program_gui_contains(*const ProgramGuiFramePayload, *const ProgramGuiFramePayload) bool;
extern fn r4os_rust_program_gui_link(*ProgramGuiFramePayload, *anyopaque, bool) void;
extern fn r4os_rust_program_gui_copy([*]u8, [*]const u8, usize) void;
extern fn r4os_rust_program_gui_clone(*ProgramInstance, *const ProgramGuiFramePayload) ?*ProgramGuiFramePayload;
extern fn r4os_rust_program_gui_failed(*ProgramGuiPayload, ?*ProgramGuiFramePayload) void;
extern fn r4os_rust_program_gui_begin(*ProgramInstance, u8, ?[*]const DisplayDamageRect, usize) i32;
extern fn r4os_rust_program_gui_build(*ProgramInstance, bool, bool) ?*ProgramGuiFramePayload;
extern fn r4os_rust_program_gui_publish(*ProgramInstance, bool) i32;
extern fn r4os_rust_program_gui_revision(*ProgramInstance) void;
extern fn r4os_rust_program_gui_damage_valid(DisplayDamageRect) bool;
extern fn r4os_rust_program_gui_result(*ProgramGuiPayload, i32) i32;


comptime {
    if (@sizeOf(GuiFrameCapture) != 40 or @alignOf(GuiFrameCapture) != 8 or @offsetOf(GuiFrameCapture, "gui") != 0 or @offsetOf(GuiFrameCapture, "frame") != 8 or @offsetOf(GuiFrameCapture, "generation") != 16 or @offsetOf(GuiFrameCapture, "unwind") != 24 or GUI_COPY_RESCHEDULE_BYTES != 65536)
        @compileError("Native GUI reader capture original geometry/copy scheduling drift");
}


extern fn r4os_rust_program_gui_batch_kind(*const GuiFrameCommand) u16;
extern fn r4os_rust_program_gui_batch_slice(*const GuiFrameCommand, NativeProgramBytes, *NativeProgramBytes) bool;
extern fn r4os_rust_program_gui_shape_word(NativeProgramBytes, usize, *u32) bool;
extern fn r4os_rust_program_gui_descriptor(NativeProgramBytes, *GuiSharedRasterResource) bool;
extern fn r4os_rust_program_gui_float(u8, u32, u32) bool;
extern fn r4os_rust_program_gui_resource_valid(u8, ?*const GuiFrameCommand, NativeProgramBytes, u32) bool;
extern fn r4os_rust_program_gui_has_ref(*const ProgramGuiFramePayload, GuiSharedRasterHandle, u64) bool;
extern fn r4os_rust_program_gui_pin_batch(ProgramProcessHandle, *const ProgramGuiFramePayload, *ProgramGuiFramePayload, [*]const GuiFrameCommand, usize, NativeProgramBytes) i32;
extern fn r4os_rust_program_gui_internal(*const GuiFrameCommand, u64, u64, *ProgramGuiCommand) void;
extern fn r4os_rust_program_gui_blob(*ProgramInstance, *ProgramGuiFramePayload, NativeProgramBytes) bool;
extern fn r4os_rust_program_gui_splice(*ProgramGuiPayload, *ProgramGuiFramePayload, *ProgramGuiFramePayload) void;
extern fn r4os_rust_program_gui_batch(*ProgramInstance, [*]const GuiFrameCommand, usize, NativeProgramBytes) i32;



comptime {
    if (GUI_RESOURCE_BLOCK_TARGET_BYTES != 65536 or GUI_RASTER_MAX_WIDTH != 128 or GUI_RASTER_MAX_HEIGHT != 128 or GUI_RASTER_MAX_PIXELS != 16384)
        @compileError("Native GUI batch original blob/raster bounds drift");
}


extern fn r4os_rust_program_gui_view_chain(*const ProgramGuiFramePayload, *[r4x_api.gui_frame_max_delta_chain]*const ProgramGuiFramePayload) usize;
extern fn r4os_rust_program_gui_view_location(*const ProgramGuiFramePayload, u64, *GuiFrameCommandLocation) bool;
extern fn r4os_rust_program_gui_view_command(*const ProgramGuiFramePayload, u64, bool) ?*const ProgramGuiCommand;
extern fn r4os_rust_program_gui_view_resources(*const ProgramGuiFramePayload, u64, [*]u8, usize, bool) usize;
extern fn r4os_rust_program_gui_view_legacy(GuiFrameCommandLocation, *GuiCommand) bool;
extern fn r4os_rust_program_gui_view_raster_count(u64, u32) usize;
extern fn r4os_rust_program_gui_view_rasters(*const ProgramGuiFramePayload, u64, [*]u32, usize, bool) usize;
extern fn r4os_rust_program_gui_view_external(*const ProgramGuiCommand, u64, *GuiFrameCommand) void;
extern fn r4os_rust_program_gui_view_commands(*const ProgramGuiFramePayload, u64, [*]GuiFrameCommand, usize, bool) usize;
extern fn r4os_rust_program_gui_view_info(*const ProgramGuiPayload, ProgramProcessHandle, *GuiFrameInfo) void;
extern fn r4os_rust_program_gui_view_empty(ProgramProcessHandle, i32, *GuiFrameInfo) void;
extern fn r4os_rust_program_gui_view_current(u32, i32, *GuiFrameInfo) void;
extern fn r4os_rust_program_gui_view_valid(u8, *const anyopaque) bool;
extern fn r4os_rust_program_gui_view_contains(*const ProgramGuiFramePayload, bool) bool;
extern fn r4os_rust_program_gui_view_generation(*const ProgramGuiPayload, ProgramProcessHandle, *const ProgramGuiFramePayload, *GuiFrameGenerationInfo) void;
extern fn r4os_rust_program_gui_view_stream(*const ProgramGuiPayload, ProgramProcessHandle, *GuiFrameStreamInfo) void;
extern fn r4os_rust_program_gui_view_write_stream(*GuiFrameStreamInfo, u32, *const GuiFrameStreamInfo) void;
extern fn r4os_rust_program_gui_view_api(u8, ?*const ProgramProcessHandle, u64, *anyopaque) i32;

comptime {
    if (@sizeOf(GuiFrameCommandLocation) != 32 or @alignOf(GuiFrameCommandLocation) != 8 or @offsetOf(GuiFrameCommandLocation, "frame") != 0 or @offsetOf(GuiFrameCommandLocation, "command") != 8 or @offsetOf(GuiFrameCommandLocation, "resource_base") != 16 or @offsetOf(GuiFrameCommandLocation, "raster_base") != 24)
        @compileError("Native GUI immutable command-location original geometry drift");
}


const NativeGuiEventsState = extern struct {
    push_attempts: u64, accepted: u64, mouse_move_coalesced: u64, mouse_move_evicted: u64, rejected: u64,
};
extern fn r4os_rust_program_gui_events_state() *NativeGuiEventsState;
fn guiEventsState() *NativeGuiEventsState { return r4os_rust_program_gui_events_state(); }
extern fn r4os_rust_program_gui_producer_allowed(*const ProgramInstance) bool;
extern fn r4os_rust_program_gui_window_info(*GuiWindowInfo) i32;
extern fn r4os_rust_program_gui_window_set(u32, *const GuiWindowInfo) i32;
extern fn r4os_rust_program_gui_event_poll(*GuiEvent) i32;
extern fn r4os_rust_program_gui_event_push(u32, *const GuiEvent) i32;
extern fn r4os_rust_program_gui_event_enqueue(*ProgramGuiPayload, *const GuiEvent) i32;
extern fn r4os_rust_program_gui_event_find(*const ProgramGuiPayload) usize;
extern fn r4os_rust_program_gui_event_remove(*ProgramGuiPayload, usize) void;
extern fn r4os_rust_program_gui_event_pending(*const ProgramGuiPayload) u32;
extern fn r4os_rust_program_gui_font_changed() void;
extern fn r4os_rust_program_gui_text(*ProgramInstance, [*:0]const u8) i32;
extern fn r4os_rust_program_gui_simple(u8, ?[*:0]const u8, i32, i32) i32;
extern fn r4os_rust_program_gui_string(u32, [*]u8, u32, bool) i32;
extern fn r4os_rust_program_gui_revision_read(u32) u32;
extern fn r4os_rust_program_gui_min_size(u32, *GuiSize) i32;
comptime {
    if (@sizeOf(NativeGuiEventsState) != 40 or @offsetOf(NativeGuiEventsState, "push_attempts") != 0 or @offsetOf(NativeGuiEventsState, "accepted") != 8 or @offsetOf(NativeGuiEventsState, "mouse_move_coalesced") != 16 or @offsetOf(NativeGuiEventsState, "mouse_move_evicted") != 24 or @offsetOf(NativeGuiEventsState, "rejected") != 32 or GUI_EVENT_QUEUE_SIZE != 64 or GUI_EVENT_KIND_MOUSE_MOVE != 5 or GUI_EVENT_KIND_FONT_CHANGED != 7 or GUI_TEXT_SIZE != 512 or GUI_TITLE_SIZE != 64)
        @compileError("Native GUI event/text metadata geometry drift");
}


extern fn r4os_rust_program_gui_api_transition(u8, ?[*]const DisplayDamageRect, u32) i32;
extern fn r4os_rust_program_gui_api_append(?[*]const GuiFrameCommand, u64, ?[*]const u8, u64) i32;
extern fn r4os_rust_program_gui_api_read(*const ProgramProcessHandle, u64, ?[*]GuiFrameCommand, u64, ?[*]u8, u64, *GuiFrameInfo) i32;
extern fn r4os_rust_program_gui_api_generation_read(*const ProgramProcessHandle, u64, ?[*]GuiFrameCommand, u64, ?[*]u8, u64, ?[*]DisplayDamageRect, u32, *GuiFrameGenerationInfo) i32;
extern fn r4os_rust_program_gui_api_raster(u32, u32, [*]u32, u32) i32;
extern fn r4os_rust_program_gui_api_command(u32, u32, *GuiCommand) i32;
extern fn r4os_rust_program_gui_append_command(*ProgramInstance, *const ProgramGuiCommand) i32;


const NativeGuiFontCatalog = extern struct {
    info: GuiFontInfo, reserved: u32 = 0,
    path_len: usize, family_len: usize, face_len: usize, style_len: usize, status_len: usize,
    renderable: bool, selected: bool, reserved2: [6]u8 = .{0} ** 6,
};
const NativeGuiTextMeasurement = extern struct { width: u32, height: u32, baseline: i32, line_height: u32 };
extern fn r4os_rust_program_gui_font_set(u32) i32;
extern fn r4os_rust_program_gui_font_get(u32, *GuiFontInfo) i32;
extern fn r4os_rust_program_gui_font_info(u32, bool, *GuiFontInfo) i32;
export fn r4os_zig_program_font_control(op: u8, id: u32) callconv(.c) u32 {
    return switch (op) { 0 => @intFromBool(font.isRenderableFontId(id)), 1 => font.normalizeFontId(id), 2 => font.currentFontId(), else => unreachable };
}
export fn r4os_zig_program_font_catalog(id: u32, out: *NativeGuiFontCatalog) callconv(.c) bool {
    const entry = font.catalogEntryForFontId(id) orelse return false;
    out.* = .{
        .info = .{ .kind = @intFromEnum(entry.kind), .weight = entry.weight, .style_flags = entry.style_flags, .charset_flags = entry.charset_flags,
            .width = entry.width, .height = entry.height, .max_advance = entry.max_advance, .line_height = entry.line_height, .baseline = entry.baseline,
            .glyph_count = entry.glyph_count, .strike_count = entry.strike_count,
            .path = entry.path, .family = entry.family, .face = entry.face, .style = entry.style, .status = entry.status },
        .path_len = entry.path_len, .family_len = entry.family_len, .face_len = entry.face_len, .style_len = entry.style_len, .status_len = entry.status_len,
        .renderable = entry.renderable, .selected = entry.selected,
    };
    return true;
}
export fn r4os_zig_program_font_measure(id: u32, text: [*:0]const u8, max_bytes: usize, out: *NativeGuiTextMeasurement) callconv(.c) void {
    const value = font.measureZWithFont(id, text, max_bytes);
    out.* = .{ .width = value.width, .height = value.height, .baseline = value.baseline, .line_height = value.line_height };
}
comptime {
    if (@sizeOf(GuiFontInfo) != 316 or @sizeOf(NativeGuiFontCatalog) != 368 or @alignOf(NativeGuiFontCatalog) != 8 or
        @offsetOf(NativeGuiFontCatalog, "info") != 0 or @offsetOf(NativeGuiFontCatalog, "reserved") != 316 or @offsetOf(NativeGuiFontCatalog, "path_len") != 320 or @offsetOf(NativeGuiFontCatalog, "family_len") != 328 or
        @offsetOf(NativeGuiFontCatalog, "face_len") != 336 or @offsetOf(NativeGuiFontCatalog, "style_len") != 344 or @offsetOf(NativeGuiFontCatalog, "status_len") != 352 or
        @offsetOf(NativeGuiFontCatalog, "renderable") != 360 or @offsetOf(NativeGuiFontCatalog, "selected") != 361 or @offsetOf(NativeGuiFontCatalog, "reserved2") != 362 or
        font.MAX_FONT_PATH != 96 or font.MAX_FONT_NAME != 40 or font.MAX_STATUS_TEXT != 48)
        @compileError("Native bounded font catalog value projection drift");
    if (@sizeOf(NativeGuiTextMeasurement) != 16 or @alignOf(NativeGuiTextMeasurement) != 4 or @offsetOf(NativeGuiTextMeasurement, "width") != 0 or @offsetOf(NativeGuiTextMeasurement, "height") != 4 or @offsetOf(NativeGuiTextMeasurement, "baseline") != 8 or @offsetOf(NativeGuiTextMeasurement, "line_height") != 12 or
        GUI_FONT_BUILTIN_ID != 0 or GUI_FONT_FLAG_RENDERABLE != 1 or GUI_FONT_FLAG_SELECTED != 2 or GUI_FONT_FLAG_BUILTIN != 4)
        @compileError("Native font measurement/GUI flags drift");
}


extern fn r4os_rust_program_gui_clear(u32) i32;
extern fn r4os_rust_program_gui_rect(i32, i32, u32, u32, u32) i32;
extern fn r4os_rust_program_gui_blit(i32, i32, u32, u32, u32, [*]const u32, u32) i32;
extern fn r4os_rust_program_gui_alpha(i32, i32, u32, u32, u32, u32, [*]const u8, u32) i32;
extern fn r4os_rust_program_gui_draw_text(i32, i32, [*:0]const u8, u32, u32, u32, u32, bool) i32;
extern fn r4os_rust_program_gui_text_command(*ProgramInstance, i32, i32, [*:0]const u8, u32, u32, u32, u32) i32;
comptime {
    const empty = ProgramGuiCommand{};
    if (empty.version != 1 or empty.size != 96 or empty.kind != 0 or empty.flags != 0 or empty.font_id != 0 or empty.text_w != 0 or empty.text_h != 0 or empty.baseline != 0 or empty.line_height != 0 or empty.resource_kind != .none or empty.reserved != 0 or
        GUI_COMMAND_KIND_ALPHA8 != 5 or GUI_LEGACY_TEXT_APPEND_MAX_BYTES != 1048576)
        @compileError("Native legacy GUI command defaults/text limits drift");
}


const NativeSharedDebug = extern struct { failure_value: u32, failure_cursor: u32, failure_present: bool, reserved: [7]u8 };
extern fn r4os_rust_program_shared_debug() *NativeSharedDebug;
fn sharedRasterDebug() *NativeSharedDebug { return r4os_rust_program_shared_debug(); }
fn sharedRasterFailureAfter() ?u32 { const value = sharedRasterDebug(); return if (value.failure_present) value.failure_value else null; }
fn setSharedRasterFailureAfter(value: ?u32) void { r4os_rust_program_shared_failure(value orelse 0, value != null, false); }
extern fn r4os_rust_program_shared_failure(u32, bool, bool) void;
extern fn r4os_rust_program_shared_lock() void;
extern fn r4os_rust_program_shared_valid(GuiSharedRasterHandle) bool;
extern fn r4os_rust_program_shared_equal(GuiSharedRasterHandle, GuiSharedRasterHandle) bool;
extern fn r4os_rust_program_shared_info_valid(GuiSharedRasterCreateInfo) bool;
extern fn r4os_rust_program_shared_graphics_owner(ProgramProcessHandle) NativeSharedGraphicsOwner;
extern fn r4os_rust_program_shared_create(ProgramProcessHandle, GuiSharedRasterCreateInfo, *GuiSharedRasterHandle) i32;
extern fn r4os_rust_program_shared_destroy(ProgramProcessHandle, GuiSharedRasterHandle) i32;
extern fn r4os_rust_program_shared_write(ProgramProcessHandle, GuiSharedRasterHandle, *GuiSharedRasterWriteMap) i32;
extern fn r4os_rust_program_shared_publish(ProgramProcessHandle, GuiSharedRasterWriteMap, *u64) i32;
extern fn r4os_rust_program_shared_pin(ProgramProcessHandle, *ProgramGuiFramePayload, GuiSharedRasterHandle, u64, u32, u32, u32, u64, u8, *ProgramGuiSharedRasterRef) bool;
extern fn r4os_rust_program_shared_pin_frame(ProgramProcessHandle, *ProgramGuiFramePayload, GuiSharedRasterResource, *ProgramGuiSharedRasterRef) bool;
extern fn r4os_rust_program_shared_copy(ProgramProcessHandle, *const ProgramGuiFramePayload, *ProgramGuiFramePayload) bool;
extern fn r4os_rust_program_shared_move(*ProgramGuiFramePayload, *ProgramGuiFramePayload) bool;
extern fn r4os_rust_program_shared_release_frames(*ProgramGuiFramePayload) bool;
extern fn r4os_rust_program_shared_has(*const ProgramGuiFramePayload, GuiSharedRasterHandle, u64) bool;
extern fn r4os_rust_program_shared_acquire(ProgramProcessHandle, ProgramProcessHandle, u64, GuiSharedRasterHandle, u64, *GuiSharedRasterMap) i32;
extern fn r4os_rust_program_shared_release(ProgramProcessHandle, GuiSharedRasterLease) i32;
extern fn r4os_rust_program_shared_release_process(ProgramProcessHandle) void;
extern fn r4os_rust_program_shared_stats(ProgramProcessHandle, *SharedRasterStats) void;
extern fn r4os_rust_program_shared_empty() bool;
extern fn r4os_rust_program_shared_note(ProgramProcessHandle, *const ProgramGuiFramePayload) void;
extern fn r4os_rust_program_shared_api_create(*const GuiSharedRasterCreateInfo, *GuiSharedRasterHandle) i32;
extern fn r4os_rust_program_shared_api_destroy(*const GuiSharedRasterHandle) i32;
extern fn r4os_rust_program_shared_api_write(*const GuiSharedRasterHandle, *GuiSharedRasterWriteMap) i32;
extern fn r4os_rust_program_shared_api_publish(*const GuiSharedRasterWriteMap, *u64) i32;
extern fn r4os_rust_program_shared_api_acquire(*const ProgramProcessHandle, u64, *const GuiSharedRasterHandle, u64, *GuiSharedRasterMap) i32;
extern fn r4os_rust_program_shared_api_release(*const GuiSharedRasterLease) i32;
extern fn r4os_rust_program_shared_export(ProgramProcessHandle, *const GuiSharedRasterLease, *r4x_api.GfxBufferReference) i32;
extern fn r4os_rust_program_shared_api_export(*const GuiSharedRasterLease, *r4x_api.GfxBufferReference) i32;
comptime {
    if (@sizeOf(SharedRasterStats) != 88 or @alignOf(SharedRasterStats) != 8 or @offsetOf(SharedRasterStats, "owner") != 0 or @offsetOf(SharedRasterStats, "publish_count") != 16 or @offsetOf(SharedRasterStats, "acquire_count") != 24 or @offsetOf(SharedRasterStats, "release_count") != 32 or @offsetOf(SharedRasterStats, "backpressure_count") != 40 or @offsetOf(SharedRasterStats, "published_bytes") != 48 or @offsetOf(SharedRasterStats, "frame_bytes_avoided") != 56 or @offsetOf(SharedRasterStats, "acquired_bytes") != 64 or @offsetOf(SharedRasterStats, "live_bytes") != 72 or @offsetOf(SharedRasterStats, "used") != 80 or @offsetOf(SharedRasterStats, "reserved") != 81)
        @compileError("Native shared-raster readonly stats original geometry drift");
    if (@sizeOf(NativeSharedDebug) != 16 or @alignOf(NativeSharedDebug) != 4 or @offsetOf(NativeSharedDebug, "failure_value") != 0 or @offsetOf(NativeSharedDebug, "failure_cursor") != 4 or @offsetOf(NativeSharedDebug, "failure_present") != 8 or @offsetOf(NativeSharedDebug, "reserved") != 9 or SHARED_RASTER_RESOURCE_CAPACITY != 32 or SHARED_RASTER_LEASE_CAPACITY != 128 or SHARED_RASTER_FRAME_REF_CAPACITY != 256 or SHARED_RASTER_STATS_CAPACITY != 64 or r4x_api.gui_shared_raster_buffer_count != 3)
        @compileError("Native shared-raster diagnostic/capacity geometry drift");
}
// Temporary typed calls into BO/platform owners that remain separate from the
// native shared-raster owner. No shared-raster table or algorithm lives here.
const NativeSharedBackendHandle = extern struct { generation: u64, id: u32, reserved: u32 = 0 };
const NativeSharedCreated = extern struct { buffer: NativeSharedBackendHandle, reference: NativeSharedBackendHandle, address: u64, bytes: u64 };
const NativeSharedPrepared = extern struct { status: i32, out_of_memory: bool, reserved: [3]u8 = .{0} ** 3 };
const NativeSharedGraphicsOwner = extern struct { id: u64, generation: u64, kind: u32, reserved: u32 = 0 };
fn sharedBackendHandle(value: NativeSharedBackendHandle) gfx_buffers.Handle { return .{ .id = value.id, .generation = value.generation }; }
fn sharedBackendView(value: gfx_buffers.Handle) NativeSharedBackendHandle { return .{ .id = value.id, .generation = value.generation }; }
fn sharedBackendOwner(owner: ProgramProcessHandle, raster: bool) gfx_buffers.Owner { return if (raster) gfx_buffers.raster_owner else graphicsOwner(owner); }
export fn r4os_zig_program_shared_mutex() callconv(.c) *sync.Mutex { return gfx_buffers.metadataLock(); }
export fn r4os_zig_program_shared_prepare(references: usize, leases: usize) callconv(.c) NativeSharedPrepared {
    gfx_buffers.lockPrepared(.{ .references = references, .leases = leases }) catch |err| return .{ .status = gfx_buffer_api.status(err), .out_of_memory = err == error.OutOfMemory };
    return .{ .status = 0, .out_of_memory = false };
}
export fn r4os_zig_program_shared_create(owner: ProgramProcessHandle, info: GuiSharedRasterCreateInfo, xrgb: bool, out: *NativeSharedCreated) callconv(.c) bool {
    var descriptor = gfx_buffers.layout.Descriptor{ .bytes = info.data_bytes, .width = info.width, .height = info.height,
        .format = if (xrgb) .xrgb8888 else .r8, .plane_count = 1,
        .usage = gfx_buffers.layout.Usage.cpu_read | gfx_buffers.layout.Usage.cpu_write | gfx_buffers.layout.Usage.transfer_source };
    descriptor.planes[0] = .{ .offset = info.data_offset, .pitch = info.stride_bytes };
    const allocation = gfx_buffers.create(graphicsOwner(owner), descriptor) catch return false;
    out.* = .{ .buffer = sharedBackendView(allocation.buffer), .reference = sharedBackendView(allocation.reference), .address = allocation.address, .bytes = allocation.bytes };
    return true;
}
export fn r4os_zig_program_shared_import(buffer: NativeSharedBackendHandle, owner: ProgramProcessHandle, raster: bool, read_only: bool, out: *NativeSharedBackendHandle) callconv(.c) i32 {
    const reference = if (read_only)
        gfx_buffers.store.importMode(sharedBackendHandle(buffer), sharedBackendOwner(owner, raster), true) catch |err| return gfx_buffer_api.status(err)
    else
        gfx_buffers.store.import(sharedBackendHandle(buffer), sharedBackendOwner(owner, raster)) catch |err| return gfx_buffer_api.status(err);
    out.* = sharedBackendView(reference);
    return 0;
}
export fn r4os_zig_program_shared_drop(reference: NativeSharedBackendHandle, owner: ProgramProcessHandle, raster: bool, locked: bool) callconv(.c) i32 {
    if (locked) { gfx_buffers.store.drop(sharedBackendHandle(reference), sharedBackendOwner(owner, raster)) catch |err| return gfx_buffer_api.status(err); }
    else { gfx_buffers.drop(sharedBackendHandle(reference), sharedBackendOwner(owner, raster)) catch |err| return gfx_buffer_api.status(err); }
    return 0;
}
export fn r4os_zig_program_shared_map(reference: NativeSharedBackendHandle, write: bool, bytes: usize, out: *NativeSharedBackendHandle) callconv(.c) bool {
    const value = gfx_buffers.mapLocked(sharedBackendHandle(reference), gfx_buffers.raster_owner, if (write) .cpu_write else .cpu_read, 0, bytes) catch return false;
    out.* = sharedBackendView(value.lease);
    return true;
}
export fn r4os_zig_program_shared_unmap(lease: NativeSharedBackendHandle) callconv(.c) bool {
    gfx_buffers.unmapCpuLocked(sharedBackendHandle(lease), gfx_buffers.raster_owner) catch return false;
    return true;
}
export fn r4os_zig_program_shared_use(lease: NativeSharedBackendHandle, out: *NativeSharedBackendHandle) callconv(.c) bool {
    const value = gfx_buffers.store.useInfo(sharedBackendHandle(lease), gfx_buffers.raster_owner) catch return false;
    out.* = sharedBackendView(value.buffer);
    return true;
}
export fn r4os_zig_program_shared_reference(reference: NativeSharedBackendHandle, owner: ProgramProcessHandle, out: *r4x_api.GfxBufferReference) callconv(.c) bool {
    out.* = gfx_buffer_api.referenceLocked(sharedBackendHandle(reference), graphicsOwner(owner)) catch return false;
    return true;
}
export fn r4os_zig_program_shared_stopped(op: u8, owner: ProgramProcessHandle) callconv(.c) void {
    switch (op) {
        0 => gfx_buffers.store.stoppedOwner(graphicsOwner(owner)),
        1 => gfx_queue.stopped(graphicsOwner(owner)),
        2 => gfx_allocations.stopped(graphicsOwner(owner)),
        3 => @import("../kernel/gfx_virtual.zig").stopped(graphicsOwner(owner)),
        4 => @import("../display/cursor_work.zig").stopped(graphicsOwner(owner)),
        5 => @import("../display/outputs.zig").refreshStopped(graphicsOwner(owner)),
        6 => @import("../display/outputs.zig").powerStopped(graphicsOwner(owner)),
        7 => r4api.r4desk.remoteFrameStopped(.{ .id = owner.instance_id, .generation = owner.generation }),
        8 => gfx_buffers.collect(),
        else => unreachable,
    }
}
comptime {
    const owner: gfx_buffers.Owner = .{ .kind = .program, .id = 1, .generation = 1 };
    if (@intFromEnum(owner.kind) != 1 or @sizeOf(NativeSharedBackendHandle) != 16 or @alignOf(NativeSharedBackendHandle) != 8 or @offsetOf(NativeSharedBackendHandle, "generation") != 0 or @offsetOf(NativeSharedBackendHandle, "id") != 8 or @offsetOf(NativeSharedBackendHandle, "reserved") != 12)
        @compileError("Native BO handle value projection drift");
    if (@sizeOf(NativeSharedCreated) != 48 or @offsetOf(NativeSharedCreated, "buffer") != 0 or @offsetOf(NativeSharedCreated, "reference") != 16 or @offsetOf(NativeSharedCreated, "address") != 32 or @offsetOf(NativeSharedCreated, "bytes") != 40)
        @compileError("Native BO allocation value projection drift");
    if (@sizeOf(NativeSharedPrepared) != 8 or @offsetOf(NativeSharedPrepared, "status") != 0 or @offsetOf(NativeSharedPrepared, "out_of_memory") != 4 or @offsetOf(NativeSharedPrepared, "reserved") != 5)
        @compileError("Native BO preparation value projection drift");
    if (@sizeOf(NativeSharedGraphicsOwner) != 24 or @offsetOf(NativeSharedGraphicsOwner, "id") != 0 or @offsetOf(NativeSharedGraphicsOwner, "generation") != 8 or @offsetOf(NativeSharedGraphicsOwner, "kind") != 16 or @offsetOf(NativeSharedGraphicsOwner, "reserved") != 20)
        @compileError("Native BO owner value projection drift");
}


extern fn r4os_rust_program_async_initialize() void;
extern fn r4os_rust_program_async_file(u32, [*:0]const u8, u64, usize, u64, usize, u64, u32, bool, ?*u32) i32;
extern fn r4os_rust_program_async_service(u32, u16, [*]const u8, u32, *ServiceMessageHeader, [*]u8, u32, u64, u32, bool, ?*u32) i32;
extern fn r4os_rust_program_async_request(u8, u32, u64, ?*ProgramIoInfo) i32;
const NativeAsyncRequestView = extern struct { instance: ?*ProgramInstance, path: NativeProgramBytes };
extern fn r4os_rust_program_async_view(*const task.Task, *NativeAsyncRequestView) bool;
const NativeAsyncStreamOwner = extern struct { task: u32, program: u32, task_generation: u64, program_generation: u64 };
extern fn r4os_rust_program_async_stream_owner(*NativeAsyncStreamOwner) bool;
export fn r4os_zig_program_async_option(retry: bool, out: *NativeProgramBytes) callconv(.c) bool {
    const value = boot_config.optionValue(boot_config.get(), "TASKREGISTRY", if (retry) "async-retire-retry" else "selftest") orelse return false;
    out.* = .{ .data = value.ptr, .len = value.len };
    return true;
}
export fn r4os_zig_program_async_file(kind: u32, path: [*:0]const u8, offset: u64, data: [*]const u8, len: u32, out: [*]u8, out_len: u32, flags: u32) callconv(.c) i32 {
    return switch (kind) {
        1 => r4api.r4sys.fileRead(path, out, out_len),
        2 => r4api.r4sys.fileReadAt64(path, offset, out, out_len),
        3 => r4api.r4sys.fileWrite(path, data, len),
        4 => r4api.r4sys.fileAppend(path, data, len),
        5 => r4api.r4sys.fileStreamBegin(path, flags),
        6 => r4api.r4sys.fileStreamWrite(path, offset, data, len, flags),
        7 => r4api.r4sys.fileStreamFinish(path, offset, flags),
        8 => r4api.r4sys.fileStreamAbort(path),
        10 => r4api.r4sys.fileWriteAt(path, offset, data, len),
        else => unreachable,
    };
}
export fn r4os_zig_program_async_file_info(path: [*:0]const u8, out: *r4x_api.FileInfo) callconv(.c) i32 { return r4api.r4sys.fileInfo(path, out); }
export fn r4os_zig_program_async_service_submit(handle: u32, client: u32, op: u16, data: [*]const u8, len: u32, timeout: u64, token: *task_context.UnwindToken) callconv(.c) i32 {
    return services.submitRequestWaitGuarded(handle, client, op, if (len == 0) "" else data[0..len], timeout, token);
}
export fn r4os_zig_program_async_service_take(handle: u32, id: u32, header: *ServiceMessageHeader, data: [*]u8, len: u32) callconv(.c) i32 { return services.takeResponse(handle, id, header, data[0..len]); }
export fn r4os_zig_program_async_service_control(wait: bool, handle: u32, id: u32, timeout: u64) callconv(.c) i32 {
    return if (wait) services.waitResponse(handle, id, timeout) else services.cancelRequest(handle, id);
}
comptime {
    if (MAX_ASYNC_IO_REQUESTS != 64 or ASYNC_IO_WORKER_COUNT != 8 or MAX_FILE_RANGE_LOCKS != 64 or MAX_API_PATH != 1024 or
        ASYNC_IO_SYNC_CLOSE_RETRY_LIMIT != 64 or ASYNC_IO_RETIRE_RETRY_TEST_ATTEMPTS != 65 or IO_FLAGS_SUPPORTED != 0 or
        IO_OK != 0 or IO_ERROR_INVALID != -1 or IO_ERROR_NO_INSTANCE != -2 or IO_ERROR_NO_SLOTS != -3 or IO_ERROR_SPAWN_FAILED != -4 or IO_ERROR_NOT_FOUND != -5 or IO_ERROR_TIMEOUT != -6 or IO_ERROR_BUSY != -7 or IO_ERROR_UNSUPPORTED != -8 or IO_ERROR_CANCELLED != -9 or IO_ERROR_TOO_LARGE != -10 or
        @intFromEnum(task.ExecutionOwnerKind.none) != 0 or @intFromEnum(task.ExecutionOwnerKind.program_thread) != 1 or @intFromEnum(task.ExecutionOwnerKind.async_io) != 2 or @intFromEnum(task.ExecutionOwnerKind.driver_thread) != 3)
        @compileError("Native async request admission/worker identity constants drift");
    if (@sizeOf(NativeAsyncRequestView) != 24 or @alignOf(NativeAsyncRequestView) != 8 or @offsetOf(NativeAsyncRequestView, "instance") != 0 or @offsetOf(NativeAsyncRequestView, "path") != 8)
        @compileError("Native retained async request read projection drift");
    if (@sizeOf(NativeAsyncStreamOwner) != 24 or @alignOf(NativeAsyncStreamOwner) != 8 or @offsetOf(NativeAsyncStreamOwner,"task") != 0 or @offsetOf(NativeAsyncStreamOwner,"program") != 4 or @offsetOf(NativeAsyncStreamOwner,"task_generation") != 8 or @offsetOf(NativeAsyncStreamOwner,"program_generation") != 16)
        @compileError("Native exact async stream caller projection drift");
}


const NativeProgramLaunchLoaded = extern struct { value: NativeLoadedProgram, origin: [MODULE_ORIGIN_MAX]u8, origin_len: u16 };
const NativeProgramLaunchOptions = extern struct {
    owner_handle: ProgramProcessHandle, out_handle: ?*ProgramProcessHandle, error_out: ?*i32,
    owner: bool, owner_handle_present: bool, legacy_id: bool, retain_output: bool, report_boot: bool,
    forced_cpu: u8, forced_cpu_present: bool, reserved: u8 = 0,
};
fn nativeProgramLaunchLoaded(loaded: LoadedProgram) NativeProgramLaunchLoaded {
    return .{ .value = .{ .image = .{ .range_id = loaded.image.range_id, .code = loaded.image.code.ptr, .len = loaded.image.code.len, .owner_id = loaded.image.owner_id },
        .entry = @intFromPtr(loaded.entry), .memory = loaded.memory_contract, .parallel_execution = loaded.parallel_execution, .desktop_host = loaded.desktop_host,
        .imports = loaded.imports, .import_count = loaded.import_count, .section_count = loaded.loader_section_count, .relocation_count = loaded.loader_relocation_count },
        .origin = loaded.origin, .origin_len = loaded.origin_len };
}
fn nativeProgramLaunchOptions(options: ProgramLaunchOptions) NativeProgramLaunchOptions {
    return .{ .owner_handle = options.owner_handle orelse .{}, .out_handle = options.out_handle, .error_out = options.error_out,
        .owner = options.owner, .owner_handle_present = options.owner_handle != null, .legacy_id = options.legacy_id, .retain_output = options.retain_output,
        .report_boot = options.report_boot_launch, .forced_cpu = options.forced_cpu orelse 0, .forced_cpu_present = options.forced_cpu != null };
}
extern fn r4os_rust_program_launch_mode(u8, *const ProgramInstanceReservation, *bool, *const NativeProgramLaunchLoaded, *const ProgramStack, u8, NativeProgramBytes, *drive.Drive, u8, *const NativeProgramLaunchOptions) u8;
extern fn r4os_rust_program_launch_error(*const NativeProgramLaunchOptions, i32) void;
extern fn r4os_rust_program_launch_last(bool) i32;
extern fn r4os_rust_program_launch_cancel(ProgramProcessHandle) void;
export fn r4os_zig_program_launch_metric(rows: bool) callconv(.c) u32 { return if (rows) k.consoleRows() else k.consoleCols(); }
comptime {
    const empty_notifications = notifications.Owner{};
    if (empty_notifications.tree.root != null or empty_notifications.tree.prng.xorshift != 0 or empty_notifications.closing)
        @compileError("Native new instance empty notification owner drift");
    if (@sizeOf(NativeProgramLaunchLoaded) != 2056 or @alignOf(NativeProgramLaunchLoaded) != 8 or @offsetOf(NativeProgramLaunchLoaded,"value") != 0 or @offsetOf(NativeProgramLaunchLoaded,"origin") != 1920 or @offsetOf(NativeProgramLaunchLoaded,"origin_len") != 2048 or MODULE_ORIGIN_MAX != 128)
        @compileError("Native copied program launch image projection drift");
    if (@sizeOf(NativeProgramLaunchOptions) != 40 or @alignOf(NativeProgramLaunchOptions) != 8 or @offsetOf(NativeProgramLaunchOptions,"owner_handle") != 0 or @offsetOf(NativeProgramLaunchOptions,"out_handle") != 16 or @offsetOf(NativeProgramLaunchOptions,"error_out") != 24 or
        @offsetOf(NativeProgramLaunchOptions,"owner") != 32 or @offsetOf(NativeProgramLaunchOptions,"owner_handle_present") != 33 or @offsetOf(NativeProgramLaunchOptions,"legacy_id") != 34 or @offsetOf(NativeProgramLaunchOptions,"retain_output") != 35 or @offsetOf(NativeProgramLaunchOptions,"report_boot") != 36 or @offsetOf(NativeProgramLaunchOptions,"forced_cpu") != 37 or @offsetOf(NativeProgramLaunchOptions,"forced_cpu_present") != 38 or @offsetOf(NativeProgramLaunchOptions,"reserved") != 39)
        @compileError("Native explicit optional program launch options projection drift");
    if (@intFromEnum(RunResult.ran) != 0 or @intFromEnum(RunResult.not_found) != 1 or @intFromEnum(RunResult.failed) != 2 or
        @intFromEnum(ProgramLifecycleFailurePhase.storage) != 2 or @intFromEnum(ProgramLifecycleFailurePhase.task) != 5 or @intFromEnum(ProgramLifecycleFailurePhase.publish) != 6 or
        PROGRAM_HANDLE_ERROR_WOULD_BLOCK != -5 or PROGRAM_HANDLE_ERROR_NO_MEMORY != -8 or PROGRAM_HANDLE_ERROR_TASK_FAILED != -10 or THREAD_ERROR_INVALID != -1 or PROGRAM_EXIT_REASON_KILLED != 2 or PROGRAM_COMPLETION_FLAG_DISPLAY_USED != 4)
        @compileError("Native program launch rollback/result ordinals drift");
}


extern fn r4os_rust_program_util_number(u8, usize, usize, ?*usize) bool;
extern fn r4os_rust_program_util_compare(u8, NativeProgramBytes, NativeProgramBytes) bool;
extern fn r4os_rust_program_util_slice(u8, NativeProgramBytes) NativeProgramBytes;
extern fn r4os_rust_program_util_index(NativeProgramBytes, u8, *usize) bool;
extern fn r4os_rust_program_util_path(u8, NativeProgramBytes, NativeProgramBytes, [*]u8, usize) bool;
extern fn r4os_rust_program_util_read(NativeProgramBytes, u8) u64;
extern fn r4os_rust_program_util_write([*]u8, usize, u8, u64) void;
extern fn r4os_rust_program_util_enum(u8, u32, u32) u32;
extern fn r4os_rust_program_util_label(u8, u8) NativeProgramBytes;
extern fn r4os_rust_program_util_signed(i32) void;
comptime {
    if (R4X_FLAG_APP_CLASS_CONSOLE != 1 or R4X_FLAG_APP_CLASS_GUI != 2 or R4X_FLAG_APP_CLASS_SERVICE != 4 or @intFromEnum(AppClass.console) != 0 or @intFromEnum(AppClass.gui) != 1 or @intFromEnum(AppClass.service) != 2 or
        @intFromEnum(LaunchPolicy.auto) != 0 or @intFromEnum(LaunchPolicy.console) != 1 or @intFromEnum(LaunchPolicy.gui) != 2 or @intFromEnum(InstanceState.running) != 0 or @intFromEnum(InstanceState.close_requested) != 1 or @intFromEnum(InstanceState.done) != 2)
        @compileError("Native program labels/class flags/policy ordinals drift");
}


const NativeProgramLaunchFile = extern struct { source: module_file.NativeSource, origin: [128]u8, origin_len: u16 };
fn nativeProgramLaunchFile(file: ProgramFile) NativeProgramLaunchFile { return .{ .source = module_file.nativeSource(module_file.FileSource{ .volume = file.volume, .entry = file.entry, .drive_letter = file.drive_letter }), .origin = file.origin, .origin_len = file.origin_len }; }
fn programLaunchFileFromNative(file: NativeProgramLaunchFile) ProgramFile {
    const source = module_file.sourceFromNative(file.source);
    return .{ .volume = source.volume, .entry = source.entry, .drive_letter = source.drive_letter, .origin = file.origin, .origin_len = file.origin_len };
}
extern fn r4os_rust_program_launch_resolve(*const drive.Drive, NativeProgramBytes, bool, *NativeProgramLaunchFile) bool;
extern fn r4os_rust_program_launch_file(*const NativeProgramLaunchFile, u8, u32, NativeProgramBytes, *drive.Drive, u8, *const NativeProgramLaunchOptions) u8;
extern fn r4os_rust_program_launch_path(u8, *const drive.Drive, NativeProgramBytes, NativeProgramBytes, *drive.Drive, u8, ?*ProgramProcessHandle) i32;
extern fn r4os_rust_program_launch_classify(*const NativeProgramLaunchFile, u32, *u8) bool;
extern fn r4os_rust_program_launch_running(u8, NativeProgramBytes) i32;
extern fn r4os_rust_program_launch_origin(NativeProgramBytes, u8, NativeProgramBytes) bool;
extern fn r4os_rust_program_launch_basename(?*const ProgramInstance, *NativeProgramBytes) bool;
extern fn r4os_rust_program_launch_kind(u32) NativeProgramBytes;
extern fn r4os_rust_program_launch_fs_wait(bool, u8) void;
extern fn r4os_rust_program_launch_attempts() u64;
export fn r4os_zig_program_launch_service_stage(name: NativeProgramBytes, activity: NativeProgramBytes) callconv(.c) void { bootscreen.setServiceStage(name.data[0..name.len], activity.data[0..activity.len]); }

comptime {
    if (@sizeOf(NativeProgramLaunchFile) != 1072 or @alignOf(NativeProgramLaunchFile) != 8 or @offsetOf(NativeProgramLaunchFile,"source") != 0 or @offsetOf(NativeProgramLaunchFile,"origin") != 936 or @offsetOf(NativeProgramLaunchFile,"origin_len") != 1064 or
        @sizeOf(NativeProgramImageReader) != 16 or @alignOf(NativeProgramImageReader) != 8 or @offsetOf(NativeProgramImageReader,"context") != 0 or @offsetOf(NativeProgramImageReader,"read") != 8)
        @compileError("Native copied program file/typed reader geometry drift");
    if (MAX_R4M_SECTIONS != 16 or MAX_R4M_IMPORTS != 16 or MAX_R4M_EXPORTS != 16 or MAX_R4M_SYMBOL_PROBE != 64 or MAX_R4M_METADATA_PROBE != 2048 or MODULE_ORIGIN_MAX != 128 or PROGRAM_HANDLE_ERROR_LOAD_FAILED != -9 or PROGRAM_HANDLE_ERROR_GENERATION_EXHAUSTED != -11 or r4api.r4sys.program_module_running_error_unavailable != -2 or r4api.r4sys.program_module_running_result_running != 1 or r4api.r4sys.program_module_running_result_idle != 0)
        @compileError("Native full program load limits/status drift");
    if (@intFromEnum(fs_request.Kind.drive_info) != 1 or @intFromEnum(fs_request.Kind.file_read) != 2 or @intFromEnum(fs_request.Kind.file_read_at) != 3 or @intFromEnum(fs_request.Kind.file_write) != 4 or @intFromEnum(fs_request.Kind.file_append) != 5 or @intFromEnum(fs_request.Kind.stream_begin) != 6 or @intFromEnum(fs_request.Kind.stream_write) != 7 or @intFromEnum(fs_request.Kind.stream_finish) != 8 or @intFromEnum(fs_request.Kind.stream_abort) != 9 or @intFromEnum(fs_request.Kind.dir_list) != 10 or @intFromEnum(fs_request.Kind.dir_entry) != 11 or @intFromEnum(fs_request.Kind.file_info) != 12 or @intFromEnum(fs_request.Kind.file_delete) != 13 or @intFromEnum(fs_request.Kind.dir_create) != 14 or @intFromEnum(fs_request.Kind.dir_delete) != 15 or @intFromEnum(fs_request.Kind.file_rename) != 16 or @intFromEnum(fs_request.Kind.file_copy) != 17 or @intFromEnum(fs_request.Kind.file_move) != 18 or @intFromEnum(fs_request.Kind.loader_read) != 19 or @intFromEnum(fs_request.Kind.config_read) != 20 or @intFromEnum(fs_request.Kind.config_write) != 21 or @intFromEnum(fs_request.Kind.file_write_at) != 22 or @intFromEnum(fs_request.Kind.file_replace_atomic) != 23 or @intFromEnum(fs_request.Kind.file_delete_if_match) != 24 or @intFromEnum(fs_request.Kind.file_update_atomic_checked) != 25)
        @compileError("Native launch filesystem activity labels drift");
}


const NativeProgramApiTarget = extern struct { drive: *drive.Drive, path: NativeProgramBytes };
extern fn r4os_rust_program_api_launch(u8, ?[*:0]const u8, ?[*:0]const u8, u32, u32, ?*ProgramProcessHandle) i32;
extern fn r4os_rust_program_api_window(u32, ?*const ProgramProcessHandle, i32, bool) i32;
extern fn r4os_rust_program_api_console_host(u32, u32) i32;
extern fn r4os_rust_program_api_completion_read(*const ProgramProcessHandle, u32, [*]u8, u32, *u32) i32;
extern fn r4os_rust_program_api_host_request([*:0]const u8, [*:0]const u8, u32) i32;
extern fn r4os_rust_program_api_host_take(u32, *ProgramHostLaunchRequest) i32;
extern fn r4os_rust_program_api_module_path([*]u8, u32) i32;
extern fn r4os_rust_program_api_action(u8) i32;
extern fn r4os_rust_program_api_target(NativeProgramBytes, *[MAX_API_PATH]u8, *NativeProgramApiTarget) bool;
extern fn r4os_rust_program_api_build(NativeProgramBytes, NativeProgramBytes, *[MAX_API_PATH]u8, *usize) bool;
export fn r4os_zig_program_api_boot_ready(op: u8, id: u32) callconv(.c) i32 {
    return switch (op) {
        0 => boot_perf.completeReady(id),
        1 => blk: { bootscreen.completeForHandoff(); break :blk 0; },
        2 => blk: { boot_status.releaseForUserSession(); break :blk 0; },
        else => unreachable,
    };
}
export fn r4os_zig_program_api_display_used() callconv(.c) void { r4api.r4draw.markDisplayUsed(); }
comptime {
    if (@sizeOf(NativeProgramApiTarget) != 24 or @alignOf(NativeProgramApiTarget) != 8 or @offsetOf(NativeProgramApiTarget,"drive") != 0 or @offsetOf(NativeProgramApiTarget,"path") != 8 or MAX_API_PATH != 1024 or MAX_API_ARGS != 128 or drive.MAX_PATH != 1024 or
        @intFromEnum(ProgramCompletionState.ready) != 2 or PROGRAM_COMPLETION_FLAG_OUTPUT != 2 or PROGRAM_HANDLE_ERROR_NOT_FOUND != -2 or PROGRAM_HANDLE_ERROR_NOT_RUNNING != -4 or PROGRAM_HANDLE_ERROR_WOULD_BLOCK != -5 or PROGRAM_HANDLE_ERROR_TASK_FAILED != -10 or PROGRAM_HANDLE_ERROR_OUTPUT_UNAVAILABLE != -12 or PROGRAM_HANDLE_ERROR_OUTPUT_RANGE != -13)
        @compileError("Native public program API target/status contract drift");
    if (boot_perf.ready_result_completed != r4x_api.boot_ready_result_completed or boot_perf.ready_result_already_completed != r4x_api.boot_ready_result_already_completed or boot_perf.ready_error_not_expected_shell != r4x_api.boot_ready_error_not_boot_shell or boot_perf.ready_error_boot_failed != r4x_api.boot_ready_error_boot_failed)
        @compileError("boot readiness result contract drift");
}


const NativeProgramOutputCaptureResult = extern struct { len: usize, truncated: bool };
extern fn r4os_rust_program_io_capture(bool, ?[*]u8, usize, ?*NativeProgramOutputCaptureResult) void;
extern fn r4os_rust_program_io_output(u32, NativeProgramBytes) void;
extern fn r4os_rust_program_io_byte(u32, u8, bool) void;
extern fn r4os_rust_program_io_smoke(*const ProgramInstance, u32, u8) void;
extern fn r4os_rust_program_io_print([*:0]const u8) void;
extern fn r4os_rust_program_io_key(bool) u32;
extern fn r4os_rust_program_io_write(u32, [*]const u8, u32, bool) i32;
extern fn r4os_rust_program_io_retired(u8, ?*anyopaque) i32;
extern fn r4os_rust_program_io_allocator(u8) usize;
comptime {
    if (@sizeOf(NativeProgramOutputCaptureResult) != 16 or @alignOf(NativeProgramOutputCaptureResult) != 8 or @offsetOf(NativeProgramOutputCaptureResult,"len") != 0 or @offsetOf(NativeProgramOutputCaptureResult,"truncated") != 8 or MEM_ERROR_RETIRED != -8 or @sizeOf(ProgramMemoryStats) != 104 or @alignOf(ProgramMemoryStats) != 8 or @intFromEnum(ConsoleStream.stdin) != 0 or @intFromEnum(ConsoleStream.stdout) != 1 or @intFromEnum(ConsoleStream.stderr) != 2)
        @compileError("Native program output/capture/retired slot contract drift");
    if (@offsetOf(ProgramMemoryStats,"base") != 0 or @sizeOf(@TypeOf(@field(@as(ProgramMemoryStats, undefined),"base"))) != 8) @compileError("Native retired memory stats base drift");
    if (@offsetOf(ProgramMemoryStats,"reserved_bytes") != 8 or @sizeOf(@TypeOf(@field(@as(ProgramMemoryStats, undefined),"reserved_bytes"))) != 8) @compileError("Native retired memory stats reserved_bytes drift");
    if (@offsetOf(ProgramMemoryStats,"committed_bytes") != 16 or @sizeOf(@TypeOf(@field(@as(ProgramMemoryStats, undefined),"committed_bytes"))) != 8) @compileError("Native retired memory stats committed_bytes drift");
    if (@offsetOf(ProgramMemoryStats,"used_bytes") != 24 or @sizeOf(@TypeOf(@field(@as(ProgramMemoryStats, undefined),"used_bytes"))) != 8) @compileError("Native retired memory stats used_bytes drift");
    if (@offsetOf(ProgramMemoryStats,"free_bytes") != 32 or @sizeOf(@TypeOf(@field(@as(ProgramMemoryStats, undefined),"free_bytes"))) != 8) @compileError("Native retired memory stats free_bytes drift");
    if (@offsetOf(ProgramMemoryStats,"largest_free") != 40 or @sizeOf(@TypeOf(@field(@as(ProgramMemoryStats, undefined),"largest_free"))) != 8) @compileError("Native retired memory stats largest_free drift");
    if (@offsetOf(ProgramMemoryStats,"active_blocks") != 48 or @sizeOf(@TypeOf(@field(@as(ProgramMemoryStats, undefined),"active_blocks"))) != 4) @compileError("Native retired memory stats active_blocks drift");
    if (@offsetOf(ProgramMemoryStats,"free_blocks") != 52 or @sizeOf(@TypeOf(@field(@as(ProgramMemoryStats, undefined),"free_blocks"))) != 4) @compileError("Native retired memory stats free_blocks drift");
    if (@offsetOf(ProgramMemoryStats,"reserved0") != 56 or @sizeOf(@TypeOf(@field(@as(ProgramMemoryStats, undefined),"reserved0"))) != 4) @compileError("Native retired memory stats reserved0 drift");
    if (@offsetOf(ProgramMemoryStats,"error_flags") != 60 or @sizeOf(@TypeOf(@field(@as(ProgramMemoryStats, undefined),"error_flags"))) != 4) @compileError("Native retired memory stats error_flags drift");
    if (@offsetOf(ProgramMemoryStats,"allocation_errors") != 64 or @sizeOf(@TypeOf(@field(@as(ProgramMemoryStats, undefined),"allocation_errors"))) != 8) @compileError("Native retired memory stats allocation_errors drift");
    if (@offsetOf(ProgramMemoryStats,"invalid_free_errors") != 72 or @sizeOf(@TypeOf(@field(@as(ProgramMemoryStats, undefined),"invalid_free_errors"))) != 8) @compileError("Native retired memory stats invalid_free_errors drift");
    if (@offsetOf(ProgramMemoryStats,"size_mismatch_errors") != 80 or @sizeOf(@TypeOf(@field(@as(ProgramMemoryStats, undefined),"size_mismatch_errors"))) != 8) @compileError("Native retired memory stats size_mismatch_errors drift");
    if (@offsetOf(ProgramMemoryStats,"oom_errors") != 88 or @sizeOf(@TypeOf(@field(@as(ProgramMemoryStats, undefined),"oom_errors"))) != 8) @compileError("Native retired memory stats oom_errors drift");
    if (@offsetOf(ProgramMemoryStats,"owner_mismatch_errors") != 96 or @sizeOf(@TypeOf(@field(@as(ProgramMemoryStats, undefined),"owner_mismatch_errors"))) != 8) @compileError("Native retired memory stats owner_mismatch_errors drift");
}


extern fn r4os_rust_program_runtime_initialize(u64) void;
extern fn r4os_rust_program_runtime_configure(u8) void;
extern fn r4os_rust_program_runtime_group(u32) usize;
export fn r4os_zig_program_runtime_bindings(group: u8, out: [*]usize, count: usize) callconv(.c) void {
    switch (group) {
        1 => {
            const provider: r4x_api.R4SysProvider = .{
        .write = &r4xstartR4SysWrite,
        .putc = &r4xstartR4SysPutc,
        .sleep_ticks = &r4api.r4sys.sleepTicks,
        .ticks = &r4api.r4sys.ticks,
        .env_get = &apiEnvGet,
        .dir_entry = &r4api.r4sys.dirEntry,
        .directory_change_begin = &r4api.r4sys.directoryChangeBegin,
        .directory_next = &r4api.r4sys.directoryNext,
        .directory_change_poll = &r4api.r4sys.directoryChangePoll,
        .file_copy_buffered = &r4api.r4sys.fileCopyBuffered,
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
        .platform_input_snapshot = &@import("../driver/input/platform_input.zig").snapshot,
        .boot_ready = &apiBootReady,
        .registry_snapshot_begin = &r4api.r4sys.registrySnapshotBegin,
        .registry_snapshot_page = &r4api.r4sys.registrySnapshotPage,
        .registry_batch_mutate = &r4api.r4sys.registryBatchMutate,
        .io_file_write_at = &apiIoFileWriteAt,
        .io_file_info = &apiIoFileInfo,
        .io_file_lock = &apiIoFileLock,
        .storage_inventory = &@import("../storage/operations.zig").inventory,
        .storage_device = &@import("../storage/operations.zig").deviceInfo,
        .storage_partition = &@import("../storage/operations.zig").partitionInfo,
        .storage_volume = &@import("../storage/operations.zig").volumeInfo,
        .storage_claim_begin = &@import("../storage/operations.zig").claimBegin,
        .storage_claim_end = &@import("../storage/operations.zig").claimEnd,
        .storage_read = &@import("../storage/operations.zig").read,
        .storage_claim_read = &@import("../storage/operations.zig").claimRead,
        .storage_claim_write = &@import("../storage/operations.zig").claimWrite,
        .storage_claim_flush = &@import("../storage/operations.zig").claimFlush,
        .storage_rescan = &@import("../storage/operations.zig").rescan,
        .storage_mount = &@import("../storage/operations.zig").mount,
        .storage_unmount = &@import("../storage/operations.zig").unmount,
        .storage_use_begin = &r4api.r4sys.storageUseBegin,
        .storage_use_end = &@import("../storage/operations.zig").useEnd,
        .notification_create = &apiNotificationCreate,
        .notification_query = &apiNotificationQuery,
        .notification_notify = &apiNotificationNotify,
        .notification_wait = &apiNotificationWait,
        .notification_close = &apiNotificationClose,
        .program_local_get = &apiProgramLocalGet,
        .program_local_publish = &apiProgramLocalPublish,
        .thread_current_handle = &apiThreadCurrentHandle,
        .cpu_capacity = &r4api.r4sys.cpuCapacity,
        .program_exit = &apiProgramExit,
    };
            std.debug.assert(count == 153);
            inline for (std.meta.fields(r4x_api.R4SysProvider), 0..) |field, i| {
                out[i] = if (@field(provider, field.name)) |callback| @intFromPtr(callback) else 0;
            }
        },
        2 => {
            const provider: r4x_api.R4DeskProvider = .{
        .read_key = &apiReadKey,
        .read_key_codepoint = &apiReadKeyCodepoint,
        .mouse_state = &r4api.r4desk.mouseState,
        .mouse_motion = &r4api.r4desk.mouseMotion,
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
        .remote_frame_publish = &apiRemoteFramePublish,
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
        .remote_frame_acquire = &apiRemoteFrameAcquire,
        .remote_frame_release = &apiRemoteFrameRelease,
        .remote_frame_consumers = &r4api.r4desk.remoteFrameConsumers,
        .remote_frame_publish_regions = &apiRemoteFramePublishRegions,
        .physical_key_poll = &r4api.r4desk.physicalKeyPoll,
        .remote_frame_snapshot_acquire = &apiRemoteFrameSnapshotAcquire,
        .remote_frame_snapshot_release = &apiRemoteFrameSnapshotRelease,
        .remote_frame_source_reset = &apiRemoteFrameSourceReset,
        .remote_frame_capture_stats = &r4api.r4desk.remoteFrameCaptureStats,
        .desktop_activity_notify = &r4api.r4desk.desktopActivityNotify,
    };
            std.debug.assert(count == 64);
            inline for (std.meta.fields(r4x_api.R4DeskProvider), 0..) |field, i| {
                out[i] = if (@field(provider, field.name)) |callback| @intFromPtr(callback) else 0;
            }
        },
        3 => {
            const provider: r4x_api.R4DrawProvider = .{
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
        .display_presentation_stats = &r4api.r4draw.displayPresentationStats,
        .display_presentation_info = &r4api.r4draw.displayPresentationInfo,
        .display_presentation_feedback = &r4api.r4draw.displayPresentationFeedback,
        .display_cursor_info = &@import("../display/cursor_work.zig").info,
        .display_cursor_submit = &apiDisplayCursorSubmit,
        .display_cursor_status = &@import("../display/cursor_work.zig").status,
        .gui_frame_begin_damage = &apiGuiFrameBeginDamage,
        .gui_frame_generation_info = &apiGuiFrameGenerationInfo,
        .gui_frame_generation_read = &apiGuiFrameGenerationRead,
        .font_glyph_bitmap = &r4api.r4draw.fontGlyphBitmap,
        .font_revision = &r4api.r4draw.fontRevision,
        .gui_frame_begin_replace = &apiGuiFrameBeginReplace,
        .gui_frame_stream_info = &apiGuiFrameStreamInfo,
        .gui_shared_raster_create = &apiGuiSharedRasterCreate,
        .gui_shared_raster_destroy = &apiGuiSharedRasterDestroy,
        .gui_shared_raster_map_write = &apiGuiSharedRasterMapWrite,
        .gui_shared_raster_publish = &apiGuiSharedRasterPublish,
        .gui_shared_raster_acquire = &apiGuiSharedRasterAcquire,
        .gui_shared_raster_release = &apiGuiSharedRasterRelease,
        .gfx_buffer_create = &apiGfxBufferCreate,
        .gfx_buffer_describe = &apiGfxBufferDescribe,
        .gfx_buffer_import = &apiGfxBufferImport,
        .gfx_buffer_release = &apiGfxBufferRelease,
        .gfx_buffer_map = &apiGfxBufferMap,
        .gfx_buffer_unmap = &apiGfxBufferUnmap,
        .gfx_buffer_export_raster = &apiGfxBufferExportRaster,
        .gfx_buffer_stats = &gfx_buffer_api.stats,
        .gfx_memory_budget = &@import("gfx_memory_budget_api.zig").query,
        .gfx_telemetry = &@import("gfx_telemetry_api.zig").query,
        .gfx_queue_open = &apiGfxQueueOpen,
        .gfx_queue_close = &apiGfxQueueClose,
        .gfx_queue_submit = &apiGfxQueueSubmit,
        .gfx_queue_submit_render_list = &apiGfxQueueSubmitRenderList,
        .gfx_queue_submit_render_grid_list = &apiGfxQueueSubmitRenderGridList,
        .gfx_queue_submit_render_color_list = &apiGfxQueueSubmitRenderColorList,
        .gfx_queue_submit_render_color_grid_list = &apiGfxQueueSubmitRenderColorGridList,
        .gfx_queue_submit_native = &apiGfxQueueSubmitNative,
        .gfx_queue_submit_output = &apiGfxQueueSubmitOutput,
        .display_output_target = &r4api.r4draw.displayOutputTarget,
        .display_output_presentation_info = &r4api.r4draw.displayOutputPresentationInfo,
        .display_output_presentation_feedback = &r4api.r4draw.displayOutputPresentationFeedback,
        .gfx_fence_query = &gfx_queue_api.query,
        .gfx_fence_wait = &gfx_queue_api.wait,
        .gfx_fence_cancel = &apiGfxFenceCancel,
        .gfx_fence_release = &apiGfxFenceRelease,
        .gfx_queue_backend = &gfx_queue_api.backend,
        .gfx_queue_backend_info = &gfx_queue_api.backendInfo,
        .gfx_queue_backend_properties = &gfx_queue_api.backendProperties,
        .gfx_native_start = &apiGfxNativeStart,
        .gfx_native_query = &apiGfxNativeQuery,
        .gfx_native_receive = &apiGfxNativeReceive,
        .gfx_native_close = &apiGfxNativeClose,
        .gfx_native_wait = &apiGfxNativeWait,
        .gfx_virtual_start = &apiGfxVirtualStart,
        .gfx_virtual_query = &apiGfxVirtualQuery,
        .gfx_virtual_close = &apiGfxVirtualClose,
        .gfx_virtual_wait = &apiGfxVirtualWait,
        .gfx_buffer_map_persistent = &apiGfxBufferMapPersistent,
        .gfx_output_revision = &gfx_output_api.revision,
        .gfx_output_info = &gfx_output_api.info,
        .gfx_output_mode = &gfx_output_api.mode,
        .gfx_output_edid = &gfx_output_api.edid,
        .gfx_output_color = &gfx_output_api.color,
        .gfx_output_refresh = &gfx_output_api.refresh,
        .gfx_refresh_request = &apiGfxRefreshRequest,
        .gfx_output_power = &gfx_output_api.power,
        .gfx_power_request = &apiGfxPowerRequest,
        .gfx_output_brightness = &gfx_output_api.brightness,
        .gfx_brightness_request = &apiGfxBrightnessRequest,
        .gfx_atomic_test = &apiGfxAtomicTest,
        .gfx_atomic_commit = &apiGfxAtomicCommit,
        .gfx_atomic_submit = &apiGfxAtomicSubmit,
        .gfx_atomic_test_color = &apiGfxAtomicTestColor,
        .gfx_atomic_submit_color = &apiGfxAtomicSubmitColor,
        .gfx_atomic_status = &gfx_output_api.modeStatus,
        .gfx_atomic_resolve = &apiGfxAtomicResolve,
    };
            std.debug.assert(count == 112);
            inline for (std.meta.fields(r4x_api.R4DrawProvider), 0..) |field, i| {
                out[i] = if (@field(provider, field.name)) |callback| @intFromPtr(callback) else 0;
            }
        },
        4 => {
            const provider: r4x_api.R4NetProvider = .{
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
        .tcp_performance = &r4api.r4net.tcpPerformance,
    };
            std.debug.assert(count == 35);
            inline for (std.meta.fields(r4x_api.R4NetProvider), 0..) |field, i| {
                out[i] = if (@field(provider, field.name)) |callback| @intFromPtr(callback) else 0;
            }
        },
        5 => {
            const provider: r4x_api.R4AudioProvider = .{
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
        .audio_output_info = &apiAudioOutputInfo,
        .audio_select_output = &apiAudioSelectOutput,
    };
            std.debug.assert(count == 21);
            inline for (std.meta.fields(r4x_api.R4AudioProvider), 0..) |field, i| {
                out[i] = if (@field(provider, field.name)) |callback| @intFromPtr(callback) else 0;
            }
        },
        6 => {
            const provider: r4x_api.R4DevProvider = .{
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
        .display_state = &r4api.r4dev.displayState,
        .driver_module_info = &r4api.r4dev.driverModuleInfo,
        .performance_instrumentation = &r4api.r4dev.performanceInstrumentation,
        .performance_lock = &r4api.r4dev.performanceLock,
        .performance_stack = &r4api.r4dev.performanceStack,
    };
            std.debug.assert(count == 45);
            inline for (std.meta.fields(r4x_api.R4DevProvider), 0..) |field, i| {
                out[i] = if (@field(provider, field.name)) |callback| @intFromPtr(callback) else 0;
            }
        },
        else => unreachable,
    }
}
export fn r4os_zig_program_runtime_register(op: u8, value: u64) callconv(.c) void {
    switch (op) {
        0 => r4api.r4sys.setPathResolver(resolveApiTarget),
        1 => r4api.r4sys.setStreamOwnerResolver(resolveR4SysStreamOwner),
        2 => r4api.r4sys.setCopyCancellationProvider(apiProgramShouldClose),
        3 => @import("../storage/access_runtime.zig").setOwnerResolver(resolveStorageOwner),
        4 => r4api.r4sys.setProgramModuleRunningProvider(programModuleRunningByPath),
        5 => r4api.r4draw.setDisplayUsedHook(noteDisplayUsed),
        6 => r4api.r4draw.setFontCatalogChangedHook(broadcastGuiFontCatalogChanged),
        10 => r4api.r4sys.setRuntimeContext(value),
        11 => k.setOutputHookIntercept(kprintOutputHook),
        12 => keyboard.setInputHook(keyboardInputHook),
        20 => r4api.r4dev.setProgramInstanceStorageSummaryProvider(&fillProgramInstanceStorageSummary),
        21 => r4api.r4dev.setProgramInstanceStorageSelfTestProvider(&runProgramInstanceStorageSelfTestProvider),
        22 => r4api.r4dev.setProgramRegistrySummaryProvider(&fillProgramRegistrySummary),
        23 => r4api.r4dev.setProgramRegistrySelfTestProvider(&runProgramRegistrySelfTestProvider),
        24 => r4api.r4dev.setExecutionInventorySummaryProvider(&fillExecutionInventorySummaryProvider),
        25 => r4api.r4dev.setInputPerformanceProvider(&fillInputPerformanceInfo),
        else => unreachable,
    }
}
comptime {
    @setEvalBranchQuota(100000);
    if (@sizeOf(R4XStartR4Sys) != 1264 or std.meta.fields(r4x_api.R4SysProvider).len != 153) @compileError("Native program Sys table/provider shape drift");
    if (@offsetOf(R4XStartR4Sys,"magic") != 0) @compileError("Native program Sys.magic table offset drift");
    if (@offsetOf(R4XStartR4Sys,"abi_version") != 4) @compileError("Native program Sys.abi_version table offset drift");
    if (@offsetOf(R4XStartR4Sys,"size") != 8) @compileError("Native program Sys.size table offset drift");
    if (@offsetOf(R4XStartR4Sys,"flags") != 12) @compileError("Native program Sys.flags table offset drift");
    if (@offsetOf(R4XStartR4Sys,"write") != 16) @compileError("Native program Sys.write table offset drift");
    if (@offsetOf(R4XStartR4Sys,"putc") != 24) @compileError("Native program Sys.putc table offset drift");
    if (@offsetOf(R4XStartR4Sys,"sleep_ticks") != 32) @compileError("Native program Sys.sleep_ticks table offset drift");
    if (@offsetOf(R4XStartR4Sys,"ticks") != 40) @compileError("Native program Sys.ticks table offset drift");
    if (@offsetOf(R4XStartR4Sys,"env_get") != 48) @compileError("Native program Sys.env_get table offset drift");
    if (@offsetOf(R4XStartR4Sys,"dir_entry") != 56) @compileError("Native program Sys.dir_entry table offset drift");
    if (@offsetOf(R4XStartR4Sys,"program_should_close") != 64) @compileError("Native program Sys.program_should_close table offset drift");
    if (@offsetOf(R4XStartR4Sys,"program_class") != 72) @compileError("Native program Sys.program_class table offset drift");
    if (@offsetOf(R4XStartR4Sys,"program_instance") != 80) @compileError("Native program Sys.program_instance table offset drift");
    if (@offsetOf(R4XStartR4Sys,"service_status") != 88) @compileError("Native program Sys.service_status table offset drift");
    if (@offsetOf(R4XStartR4Sys,"service_open") != 96) @compileError("Native program Sys.service_open table offset drift");
    if (@offsetOf(R4XStartR4Sys,"service_close") != 104) @compileError("Native program Sys.service_close table offset drift");
    if (@offsetOf(R4XStartR4Sys,"service_call") != 112) @compileError("Native program Sys.service_call table offset drift");
    if (@offsetOf(R4XStartR4Sys,"service_endpoint_register") != 120) @compileError("Native program Sys.service_endpoint_register table offset drift");
    if (@offsetOf(R4XStartR4Sys,"service_endpoint_unregister") != 128) @compileError("Native program Sys.service_endpoint_unregister table offset drift");
    if (@offsetOf(R4XStartR4Sys,"service_endpoint_poll") != 136) @compileError("Native program Sys.service_endpoint_poll table offset drift");
    if (@offsetOf(R4XStartR4Sys,"service_endpoint_recv") != 144) @compileError("Native program Sys.service_endpoint_recv table offset drift");
    if (@offsetOf(R4XStartR4Sys,"service_endpoint_reply") != 152) @compileError("Native program Sys.service_endpoint_reply table offset drift");
    if (@offsetOf(R4XStartR4Sys,"service_detail_by_name") != 160) @compileError("Native program Sys.service_detail_by_name table offset drift");
    if (@offsetOf(R4XStartR4Sys,"service_start") != 168) @compileError("Native program Sys.service_start table offset drift");
    if (@offsetOf(R4XStartR4Sys,"service_stop") != 176) @compileError("Native program Sys.service_stop table offset drift");
    if (@offsetOf(R4XStartR4Sys,"service_restart") != 184) @compileError("Native program Sys.service_restart table offset drift");
    if (@offsetOf(R4XStartR4Sys,"service_install") != 192) @compileError("Native program Sys.service_install table offset drift");
    if (@offsetOf(R4XStartR4Sys,"service_remove") != 200) @compileError("Native program Sys.service_remove table offset drift");
    if (@offsetOf(R4XStartR4Sys,"reserved0") != 208) @compileError("Native program Sys.reserved0 table offset drift");
    if (@offsetOf(R4XStartR4Sys,"reserved1") != 216) @compileError("Native program Sys.reserved1 table offset drift");
    if (@offsetOf(R4XStartR4Sys,"vm_reserve") != 224) @compileError("Native program Sys.vm_reserve table offset drift");
    if (@offsetOf(R4XStartR4Sys,"vm_commit") != 232) @compileError("Native program Sys.vm_commit table offset drift");
    if (@offsetOf(R4XStartR4Sys,"vm_decommit") != 240) @compileError("Native program Sys.vm_decommit table offset drift");
    if (@offsetOf(R4XStartR4Sys,"vm_release") != 248) @compileError("Native program Sys.vm_release table offset drift");
    if (@offsetOf(R4XStartR4Sys,"vm_query") != 256) @compileError("Native program Sys.vm_query table offset drift");
    if (@offsetOf(R4XStartR4Sys,"thread_create") != 264) @compileError("Native program Sys.thread_create table offset drift");
    if (@offsetOf(R4XStartR4Sys,"thread_exit") != 272) @compileError("Native program Sys.thread_exit table offset drift");
    if (@offsetOf(R4XStartR4Sys,"thread_join") != 280) @compileError("Native program Sys.thread_join table offset drift");
    if (@offsetOf(R4XStartR4Sys,"thread_current") != 288) @compileError("Native program Sys.thread_current table offset drift");
    if (@offsetOf(R4XStartR4Sys,"thread_status") != 296) @compileError("Native program Sys.thread_status table offset drift");
    if (@offsetOf(R4XStartR4Sys,"io_file_read") != 304) @compileError("Native program Sys.io_file_read table offset drift");
    if (@offsetOf(R4XStartR4Sys,"io_file_read_at") != 312) @compileError("Native program Sys.io_file_read_at table offset drift");
    if (@offsetOf(R4XStartR4Sys,"io_file_write") != 320) @compileError("Native program Sys.io_file_write table offset drift");
    if (@offsetOf(R4XStartR4Sys,"io_file_append") != 328) @compileError("Native program Sys.io_file_append table offset drift");
    if (@offsetOf(R4XStartR4Sys,"io_file_stream_begin") != 336) @compileError("Native program Sys.io_file_stream_begin table offset drift");
    if (@offsetOf(R4XStartR4Sys,"io_file_stream_write") != 344) @compileError("Native program Sys.io_file_stream_write table offset drift");
    if (@offsetOf(R4XStartR4Sys,"io_file_stream_finish") != 352) @compileError("Native program Sys.io_file_stream_finish table offset drift");
    if (@offsetOf(R4XStartR4Sys,"io_file_stream_abort") != 360) @compileError("Native program Sys.io_file_stream_abort table offset drift");
    if (@offsetOf(R4XStartR4Sys,"io_service_call") != 368) @compileError("Native program Sys.io_service_call table offset drift");
    if (@offsetOf(R4XStartR4Sys,"io_status") != 376) @compileError("Native program Sys.io_status table offset drift");
    if (@offsetOf(R4XStartR4Sys,"io_wait") != 384) @compileError("Native program Sys.io_wait table offset drift");
    if (@offsetOf(R4XStartR4Sys,"io_close") != 392) @compileError("Native program Sys.io_close table offset drift");
    if (@offsetOf(R4XStartR4Sys,"task_yield") != 400) @compileError("Native program Sys.task_yield table offset drift");
    if (@offsetOf(R4XStartR4Sys,"reserved_shell_run") != 408) @compileError("Native program Sys.reserved_shell_run table offset drift");
    if (@offsetOf(R4XStartR4Sys,"system_halt") != 416) @compileError("Native program Sys.system_halt table offset drift");
    if (@offsetOf(R4XStartR4Sys,"system_reboot") != 424) @compileError("Native program Sys.system_reboot table offset drift");
    if (@offsetOf(R4XStartR4Sys,"system_poweroff") != 432) @compileError("Native program Sys.system_poweroff table offset drift");
    if (@offsetOf(R4XStartR4Sys,"time_state") != 440) @compileError("Native program Sys.time_state table offset drift");
    if (@offsetOf(R4XStartR4Sys,"time_set_state") != 448) @compileError("Native program Sys.time_set_state table offset drift");
    if (@offsetOf(R4XStartR4Sys,"time_seconds_since_midnight") != 456) @compileError("Native program Sys.time_seconds_since_midnight table offset drift");
    if (@offsetOf(R4XStartR4Sys,"dir_list") != 464) @compileError("Native program Sys.dir_list table offset drift");
    if (@offsetOf(R4XStartR4Sys,"dir_create") != 472) @compileError("Native program Sys.dir_create table offset drift");
    if (@offsetOf(R4XStartR4Sys,"dir_delete") != 480) @compileError("Native program Sys.dir_delete table offset drift");
    if (@offsetOf(R4XStartR4Sys,"drive_info") != 488) @compileError("Native program Sys.drive_info table offset drift");
    if (@offsetOf(R4XStartR4Sys,"file_info") != 496) @compileError("Native program Sys.file_info table offset drift");
    if (@offsetOf(R4XStartR4Sys,"file_delete") != 504) @compileError("Native program Sys.file_delete table offset drift");
    if (@offsetOf(R4XStartR4Sys,"file_rename") != 512) @compileError("Native program Sys.file_rename table offset drift");
    if (@offsetOf(R4XStartR4Sys,"file_copy") != 520) @compileError("Native program Sys.file_copy table offset drift");
    if (@offsetOf(R4XStartR4Sys,"file_move") != 528) @compileError("Native program Sys.file_move table offset drift");
    if (@offsetOf(R4XStartR4Sys,"file_read") != 536) @compileError("Native program Sys.file_read table offset drift");
    if (@offsetOf(R4XStartR4Sys,"file_write") != 544) @compileError("Native program Sys.file_write table offset drift");
    if (@offsetOf(R4XStartR4Sys,"file_read_at") != 552) @compileError("Native program Sys.file_read_at table offset drift");
    if (@offsetOf(R4XStartR4Sys,"file_append") != 560) @compileError("Native program Sys.file_append table offset drift");
    if (@offsetOf(R4XStartR4Sys,"file_stream_begin") != 568) @compileError("Native program Sys.file_stream_begin table offset drift");
    if (@offsetOf(R4XStartR4Sys,"file_stream_write") != 576) @compileError("Native program Sys.file_stream_write table offset drift");
    if (@offsetOf(R4XStartR4Sys,"file_stream_finish") != 584) @compileError("Native program Sys.file_stream_finish table offset drift");
    if (@offsetOf(R4XStartR4Sys,"file_stream_abort") != 592) @compileError("Native program Sys.file_stream_abort table offset drift");
    if (@offsetOf(R4XStartR4Sys,"env_set") != 600) @compileError("Native program Sys.env_set table offset drift");
    if (@offsetOf(R4XStartR4Sys,"registry_key_info") != 608) @compileError("Native program Sys.registry_key_info table offset drift");
    if (@offsetOf(R4XStartR4Sys,"registry_enum_key") != 616) @compileError("Native program Sys.registry_enum_key table offset drift");
    if (@offsetOf(R4XStartR4Sys,"registry_enum_value") != 624) @compileError("Native program Sys.registry_enum_value table offset drift");
    if (@offsetOf(R4XStartR4Sys,"registry_get_value") != 632) @compileError("Native program Sys.registry_get_value table offset drift");
    if (@offsetOf(R4XStartR4Sys,"registry_set_value") != 640) @compileError("Native program Sys.registry_set_value table offset drift");
    if (@offsetOf(R4XStartR4Sys,"registry_delete_value") != 648) @compileError("Native program Sys.registry_delete_value table offset drift");
    if (@offsetOf(R4XStartR4Sys,"service_info") != 656) @compileError("Native program Sys.service_info table offset drift");
    if (@offsetOf(R4XStartR4Sys,"service_detail") != 664) @compileError("Native program Sys.service_detail table offset drift");
    if (@offsetOf(R4XStartR4Sys,"service_set_start_mode") != 672) @compileError("Native program Sys.service_set_start_mode table offset drift");
    if (@offsetOf(R4XStartR4Sys,"service_endpoint_wait") != 680) @compileError("Native program Sys.service_endpoint_wait table offset drift");
    if (@offsetOf(R4XStartR4Sys,"program_run") != 688) @compileError("Native program Sys.program_run table offset drift");
    if (@offsetOf(R4XStartR4Sys,"program_launch") != 696) @compileError("Native program Sys.program_launch table offset drift");
    if (@offsetOf(R4XStartR4Sys,"program_spawn") != 704) @compileError("Native program Sys.program_spawn table offset drift");
    if (@offsetOf(R4XStartR4Sys,"program_kill") != 712) @compileError("Native program Sys.program_kill table offset drift");
    if (@offsetOf(R4XStartR4Sys,"program_status") != 720) @compileError("Native program Sys.program_status table offset drift");
    if (@offsetOf(R4XStartR4Sys,"program_request_close") != 728) @compileError("Native program Sys.program_request_close table offset drift");
    if (@offsetOf(R4XStartR4Sys,"program_reap_instance") != 736) @compileError("Native program Sys.program_reap_instance table offset drift");
    if (@offsetOf(R4XStartR4Sys,"boot_log_info") != 744) @compileError("Native program Sys.boot_log_info table offset drift");
    if (@offsetOf(R4XStartR4Sys,"boot_log_read") != 752) @compileError("Native program Sys.boot_log_read table offset drift");
    if (@offsetOf(R4XStartR4Sys,"program_spawn_handle") != 760) @compileError("Native program Sys.program_spawn_handle table offset drift");
    if (@offsetOf(R4XStartR4Sys,"program_open_handle") != 768) @compileError("Native program Sys.program_open_handle table offset drift");
    if (@offsetOf(R4XStartR4Sys,"program_handle_status") != 776) @compileError("Native program Sys.program_handle_status table offset drift");
    if (@offsetOf(R4XStartR4Sys,"program_handle_request_close") != 784) @compileError("Native program Sys.program_handle_request_close table offset drift");
    if (@offsetOf(R4XStartR4Sys,"program_handle_kill") != 792) @compileError("Native program Sys.program_handle_kill table offset drift");
    if (@offsetOf(R4XStartR4Sys,"program_handle_wait") != 800) @compileError("Native program Sys.program_handle_wait table offset drift");
    if (@offsetOf(R4XStartR4Sys,"program_handle_reap") != 808) @compileError("Native program Sys.program_handle_reap table offset drift");
    if (@offsetOf(R4XStartR4Sys,"program_completion_read") != 816) @compileError("Native program Sys.program_completion_read table offset drift");
    if (@offsetOf(R4XStartR4Sys,"program_inventory_begin") != 824) @compileError("Native program Sys.program_inventory_begin table offset drift");
    if (@offsetOf(R4XStartR4Sys,"program_inventory_programs") != 832) @compileError("Native program Sys.program_inventory_programs table offset drift");
    if (@offsetOf(R4XStartR4Sys,"program_inventory_tasks") != 840) @compileError("Native program Sys.program_inventory_tasks table offset drift");
    if (@offsetOf(R4XStartR4Sys,"program_inventory_threads") != 848) @compileError("Native program Sys.program_inventory_threads table offset drift");
    if (@offsetOf(R4XStartR4Sys,"thread_create_handle") != 856) @compileError("Native program Sys.thread_create_handle table offset drift");
    if (@offsetOf(R4XStartR4Sys,"thread_handle_join") != 864) @compileError("Native program Sys.thread_handle_join table offset drift");
    if (@offsetOf(R4XStartR4Sys,"thread_handle_status") != 872) @compileError("Native program Sys.thread_handle_status table offset drift");
    if (@offsetOf(R4XStartR4Sys,"file_replace_atomic") != 880) @compileError("Native program Sys.file_replace_atomic table offset drift");
    if (@offsetOf(R4XStartR4Sys,"file_delete_if_match") != 888) @compileError("Native program Sys.file_delete_if_match table offset drift");
    if (@offsetOf(R4XStartR4Sys,"file_update_atomic_checked") != 896) @compileError("Native program Sys.file_update_atomic_checked table offset drift");
    if (@offsetOf(R4XStartR4Sys,"path_names_equal_collated") != 904) @compileError("Native program Sys.path_names_equal_collated table offset drift");
    if (@offsetOf(R4XStartR4Sys,"file_update_cleanup_checked") != 912) @compileError("Native program Sys.file_update_cleanup_checked table offset drift");
    if (@offsetOf(R4XStartR4Sys,"file_stream_declare_publish") != 920) @compileError("Native program Sys.file_stream_declare_publish table offset drift");
    if (@offsetOf(R4XStartR4Sys,"module_resource_stat") != 928) @compileError("Native program Sys.module_resource_stat table offset drift");
    if (@offsetOf(R4XStartR4Sys,"module_resource_read") != 936) @compileError("Native program Sys.module_resource_read table offset drift");
    if (@offsetOf(R4XStartR4Sys,"program_module_path") != 944) @compileError("Native program Sys.program_module_path table offset drift");
    if (@offsetOf(R4XStartR4Sys,"program_module_running") != 952) @compileError("Native program Sys.program_module_running table offset drift");
    if (@offsetOf(R4XStartR4Sys,"monotonic_clock") != 960) @compileError("Native program Sys.monotonic_clock table offset drift");
    if (@offsetOf(R4XStartR4Sys,"boot_ready") != 968) @compileError("Native program Sys.boot_ready table offset drift");
    if (@offsetOf(R4XStartR4Sys,"registry_snapshot_begin") != 976) @compileError("Native program Sys.registry_snapshot_begin table offset drift");
    if (@offsetOf(R4XStartR4Sys,"registry_snapshot_page") != 984) @compileError("Native program Sys.registry_snapshot_page table offset drift");
    if (@offsetOf(R4XStartR4Sys,"registry_batch_mutate") != 992) @compileError("Native program Sys.registry_batch_mutate table offset drift");
    if (@offsetOf(R4XStartR4Sys,"io_file_write_at") != 1000) @compileError("Native program Sys.io_file_write_at table offset drift");
    if (@offsetOf(R4XStartR4Sys,"io_file_info") != 1008) @compileError("Native program Sys.io_file_info table offset drift");
    if (@offsetOf(R4XStartR4Sys,"io_file_lock") != 1016) @compileError("Native program Sys.io_file_lock table offset drift");
    if (@offsetOf(R4XStartR4Sys,"storage_inventory") != 1024) @compileError("Native program Sys.storage_inventory table offset drift");
    if (@offsetOf(R4XStartR4Sys,"storage_device") != 1032) @compileError("Native program Sys.storage_device table offset drift");
    if (@offsetOf(R4XStartR4Sys,"storage_partition") != 1040) @compileError("Native program Sys.storage_partition table offset drift");
    if (@offsetOf(R4XStartR4Sys,"storage_volume") != 1048) @compileError("Native program Sys.storage_volume table offset drift");
    if (@offsetOf(R4XStartR4Sys,"storage_claim_begin") != 1056) @compileError("Native program Sys.storage_claim_begin table offset drift");
    if (@offsetOf(R4XStartR4Sys,"storage_claim_end") != 1064) @compileError("Native program Sys.storage_claim_end table offset drift");
    if (@offsetOf(R4XStartR4Sys,"storage_read") != 1072) @compileError("Native program Sys.storage_read table offset drift");
    if (@offsetOf(R4XStartR4Sys,"storage_claim_read") != 1080) @compileError("Native program Sys.storage_claim_read table offset drift");
    if (@offsetOf(R4XStartR4Sys,"storage_claim_write") != 1088) @compileError("Native program Sys.storage_claim_write table offset drift");
    if (@offsetOf(R4XStartR4Sys,"storage_claim_flush") != 1096) @compileError("Native program Sys.storage_claim_flush table offset drift");
    if (@offsetOf(R4XStartR4Sys,"storage_rescan") != 1104) @compileError("Native program Sys.storage_rescan table offset drift");
    if (@offsetOf(R4XStartR4Sys,"storage_mount") != 1112) @compileError("Native program Sys.storage_mount table offset drift");
    if (@offsetOf(R4XStartR4Sys,"storage_unmount") != 1120) @compileError("Native program Sys.storage_unmount table offset drift");
    if (@offsetOf(R4XStartR4Sys,"storage_use_begin") != 1128) @compileError("Native program Sys.storage_use_begin table offset drift");
    if (@offsetOf(R4XStartR4Sys,"storage_use_end") != 1136) @compileError("Native program Sys.storage_use_end table offset drift");
    if (@offsetOf(R4XStartR4Sys,"directory_change_begin") != 1144) @compileError("Native program Sys.directory_change_begin table offset drift");
    if (@offsetOf(R4XStartR4Sys,"directory_change_poll") != 1152) @compileError("Native program Sys.directory_change_poll table offset drift");
    if (@offsetOf(R4XStartR4Sys,"file_copy_buffered") != 1160) @compileError("Native program Sys.file_copy_buffered table offset drift");
    if (@offsetOf(R4XStartR4Sys,"notification_create") != 1168) @compileError("Native program Sys.notification_create table offset drift");
    if (@offsetOf(R4XStartR4Sys,"notification_query") != 1176) @compileError("Native program Sys.notification_query table offset drift");
    if (@offsetOf(R4XStartR4Sys,"notification_notify") != 1184) @compileError("Native program Sys.notification_notify table offset drift");
    if (@offsetOf(R4XStartR4Sys,"notification_wait") != 1192) @compileError("Native program Sys.notification_wait table offset drift");
    if (@offsetOf(R4XStartR4Sys,"notification_close") != 1200) @compileError("Native program Sys.notification_close table offset drift");
    if (@offsetOf(R4XStartR4Sys,"program_local_get") != 1208) @compileError("Native program Sys.program_local_get table offset drift");
    if (@offsetOf(R4XStartR4Sys,"program_local_publish") != 1216) @compileError("Native program Sys.program_local_publish table offset drift");
    if (@offsetOf(R4XStartR4Sys,"thread_current_handle") != 1224) @compileError("Native program Sys.thread_current_handle table offset drift");
    if (@offsetOf(R4XStartR4Sys,"cpu_capacity") != 1232) @compileError("Native program Sys.cpu_capacity table offset drift");
    if (@offsetOf(R4XStartR4Sys,"program_exit") != 1240) @compileError("Native program Sys.program_exit table offset drift");
    if (@offsetOf(R4XStartR4Sys,"platform_input_snapshot") != 1248) @compileError("Native program Sys.platform_input_snapshot table offset drift");
    if (@offsetOf(R4XStartR4Sys,"directory_next") != 1256) @compileError("Native program Sys.directory_next table offset drift");
    for (std.meta.fields(r4x_api.R4SysProvider), 0..) |field, i| {
        const names = .{"write","putc","sleep_ticks","ticks","env_get","dir_entry","program_should_close","program_class","program_instance","service_status","service_open","service_close","service_call","service_endpoint_register","service_endpoint_unregister","service_endpoint_poll","service_endpoint_recv","service_endpoint_reply","service_detail_by_name","service_start","service_stop","service_restart","service_install","service_remove","vm_reserve","vm_commit","vm_decommit","vm_release","vm_query","thread_create","thread_exit","thread_join","thread_current","thread_status","io_file_read","io_file_read_at","io_file_write","io_file_append","io_file_stream_begin","io_file_stream_write","io_file_stream_finish","io_file_stream_abort","io_service_call","io_status","io_wait","io_close","task_yield","system_halt","system_reboot","system_poweroff","time_state","time_set_state","time_seconds_since_midnight","dir_list","dir_create","dir_delete","drive_info","file_info","file_delete","file_rename","file_copy","file_move","file_read","file_write","file_read_at","file_append","file_stream_begin","file_stream_write","file_stream_finish","file_stream_abort","env_set","registry_key_info","registry_enum_key","registry_enum_value","registry_get_value","registry_set_value","registry_delete_value","service_info","service_detail","service_set_start_mode","service_endpoint_wait","program_run","program_launch","program_spawn","program_kill","program_status","program_request_close","program_reap_instance","boot_log_info","boot_log_read","program_spawn_handle","program_open_handle","program_handle_status","program_handle_request_close","program_handle_kill","program_handle_wait","program_handle_reap","program_completion_read","program_inventory_begin","program_inventory_programs","program_inventory_tasks","program_inventory_threads","thread_create_handle","thread_handle_join","thread_handle_status","file_replace_atomic","file_delete_if_match","file_update_atomic_checked","path_names_equal_collated","file_update_cleanup_checked","file_stream_declare_publish","module_resource_stat","module_resource_read","program_module_path","program_module_running","monotonic_clock","boot_ready","registry_snapshot_begin","registry_snapshot_page","registry_batch_mutate","io_file_write_at","io_file_info","io_file_lock","storage_inventory","storage_device","storage_partition","storage_volume","storage_claim_begin","storage_claim_end","storage_read","storage_claim_read","storage_claim_write","storage_claim_flush","storage_rescan","storage_mount","storage_unmount","storage_use_begin","storage_use_end","directory_change_begin","directory_change_poll","file_copy_buffered","notification_create","notification_query","notification_notify","notification_wait","notification_close","program_local_get","program_local_publish","thread_current_handle","cpu_capacity","program_exit","platform_input_snapshot","directory_next"};
        if (!std.mem.eql(u8, field.name, names[i])) @compileError("Native program Sys provider ordinal drift");
    }
    if (@sizeOf(R4XStartR4Desk) != 536 or std.meta.fields(r4x_api.R4DeskProvider).len != 64) @compileError("Native program Desk table/provider shape drift");
    if (@offsetOf(R4XStartR4Desk,"magic") != 0) @compileError("Native program Desk.magic table offset drift");
    if (@offsetOf(R4XStartR4Desk,"abi_version") != 4) @compileError("Native program Desk.abi_version table offset drift");
    if (@offsetOf(R4XStartR4Desk,"size") != 8) @compileError("Native program Desk.size table offset drift");
    if (@offsetOf(R4XStartR4Desk,"flags") != 12) @compileError("Native program Desk.flags table offset drift");
    if (@offsetOf(R4XStartR4Desk,"read_key") != 16) @compileError("Native program Desk.read_key table offset drift");
    if (@offsetOf(R4XStartR4Desk,"mouse_state") != 24) @compileError("Native program Desk.mouse_state table offset drift");
    if (@offsetOf(R4XStartR4Desk,"mouse_show") != 32) @compileError("Native program Desk.mouse_show table offset drift");
    if (@offsetOf(R4XStartR4Desk,"mouse_hide") != 40) @compileError("Native program Desk.mouse_hide table offset drift");
    if (@offsetOf(R4XStartR4Desk,"keyboard_layout_current") != 48) @compileError("Native program Desk.keyboard_layout_current table offset drift");
    if (@offsetOf(R4XStartR4Desk,"keyboard_layout_at") != 56) @compileError("Native program Desk.keyboard_layout_at table offset drift");
    if (@offsetOf(R4XStartR4Desk,"keyboard_layout_set") != 64) @compileError("Native program Desk.keyboard_layout_set table offset drift");
    if (@offsetOf(R4XStartR4Desk,"program_set_window") != 72) @compileError("Native program Desk.program_set_window table offset drift");
    if (@offsetOf(R4XStartR4Desk,"program_set_console_host") != 80) @compileError("Native program Desk.program_set_console_host table offset drift");
    if (@offsetOf(R4XStartR4Desk,"program_request_host_launch") != 88) @compileError("Native program Desk.program_request_host_launch table offset drift");
    if (@offsetOf(R4XStartR4Desk,"program_take_host_launch") != 96) @compileError("Native program Desk.program_take_host_launch table offset drift");
    if (@offsetOf(R4XStartR4Desk,"program_window_id") != 104) @compileError("Native program Desk.program_window_id table offset drift");
    if (@offsetOf(R4XStartR4Desk,"gui_window_info") != 112) @compileError("Native program Desk.gui_window_info table offset drift");
    if (@offsetOf(R4XStartR4Desk,"gui_set_window_info") != 120) @compileError("Native program Desk.gui_set_window_info table offset drift");
    if (@offsetOf(R4XStartR4Desk,"gui_poll_event") != 128) @compileError("Native program Desk.gui_poll_event table offset drift");
    if (@offsetOf(R4XStartR4Desk,"gui_push_event") != 136) @compileError("Native program Desk.gui_push_event table offset drift");
    if (@offsetOf(R4XStartR4Desk,"gui_set_text") != 144) @compileError("Native program Desk.gui_set_text table offset drift");
    if (@offsetOf(R4XStartR4Desk,"gui_text") != 152) @compileError("Native program Desk.gui_text table offset drift");
    if (@offsetOf(R4XStartR4Desk,"gui_revision") != 160) @compileError("Native program Desk.gui_revision table offset drift");
    if (@offsetOf(R4XStartR4Desk,"gui_command") != 168) @compileError("Native program Desk.gui_command table offset drift");
    if (@offsetOf(R4XStartR4Desk,"gui_set_title") != 176) @compileError("Native program Desk.gui_set_title table offset drift");
    if (@offsetOf(R4XStartR4Desk,"gui_title") != 184) @compileError("Native program Desk.gui_title table offset drift");
    if (@offsetOf(R4XStartR4Desk,"gui_set_min_size") != 192) @compileError("Native program Desk.gui_set_min_size table offset drift");
    if (@offsetOf(R4XStartR4Desk,"gui_min_size") != 200) @compileError("Native program Desk.gui_min_size table offset drift");
    if (@offsetOf(R4XStartR4Desk,"console_output") != 208) @compileError("Native program Desk.console_output table offset drift");
    if (@offsetOf(R4XStartR4Desk,"console_revision") != 216) @compileError("Native program Desk.console_revision table offset drift");
    if (@offsetOf(R4XStartR4Desk,"console_state") != 224) @compileError("Native program Desk.console_state table offset drift");
    if (@offsetOf(R4XStartR4Desk,"console_set_metrics") != 232) @compileError("Native program Desk.console_set_metrics table offset drift");
    if (@offsetOf(R4XStartR4Desk,"console_push_key") != 240) @compileError("Native program Desk.console_push_key table offset drift");
    if (@offsetOf(R4XStartR4Desk,"console_write") != 248) @compileError("Native program Desk.console_write table offset drift");
    if (@offsetOf(R4XStartR4Desk,"console_read") != 256) @compileError("Native program Desk.console_read table offset drift");
    if (@offsetOf(R4XStartR4Desk,"clipboard_write") != 264) @compileError("Native program Desk.clipboard_write table offset drift");
    if (@offsetOf(R4XStartR4Desk,"clipboard_read") != 272) @compileError("Native program Desk.clipboard_read table offset drift");
    if (@offsetOf(R4XStartR4Desk,"clipboard_revision") != 280) @compileError("Native program Desk.clipboard_revision table offset drift");
    if (@offsetOf(R4XStartR4Desk,"clipboard_info") != 288) @compileError("Native program Desk.clipboard_info table offset drift");
    if (@offsetOf(R4XStartR4Desk,"clipboard_clear") != 296) @compileError("Native program Desk.clipboard_clear table offset drift");
    if (@offsetOf(R4XStartR4Desk,"read_key_codepoint") != 304) @compileError("Native program Desk.read_key_codepoint table offset drift");
    if (@offsetOf(R4XStartR4Desk,"reserved1") != 312) @compileError("Native program Desk.reserved1 table offset drift");
    if (@offsetOf(R4XStartR4Desk,"remote_frame_info") != 320) @compileError("Native program Desk.remote_frame_info table offset drift");
    if (@offsetOf(R4XStartR4Desk,"remote_frame_read") != 328) @compileError("Native program Desk.remote_frame_read table offset drift");
    if (@offsetOf(R4XStartR4Desk,"remote_frame_wait") != 336) @compileError("Native program Desk.remote_frame_wait table offset drift");
    if (@offsetOf(R4XStartR4Desk,"remote_frame_publish") != 344) @compileError("Native program Desk.remote_frame_publish table offset drift");
    if (@offsetOf(R4XStartR4Desk,"remote_input_push") != 352) @compileError("Native program Desk.remote_input_push table offset drift");
    if (@offsetOf(R4XStartR4Desk,"remote_input_poll") != 360) @compileError("Native program Desk.remote_input_poll table offset drift");
    if (@offsetOf(R4XStartR4Desk,"remote_input_status") != 368) @compileError("Native program Desk.remote_input_status table offset drift");
    if (@offsetOf(R4XStartR4Desk,"desktop_activity_wait") != 376) @compileError("Native program Desk.desktop_activity_wait table offset drift");
    if (@offsetOf(R4XStartR4Desk,"remote_frame_map") != 384) @compileError("Native program Desk.remote_frame_map table offset drift");
    if (@offsetOf(R4XStartR4Desk,"program_current_console_host") != 392) @compileError("Native program Desk.program_current_console_host table offset drift");
    if (@offsetOf(R4XStartR4Desk,"program_request_desktop") != 400) @compileError("Native program Desk.program_request_desktop table offset drift");
    if (@offsetOf(R4XStartR4Desk,"program_spawn_with_console_host") != 408) @compileError("Native program Desk.program_spawn_with_console_host table offset drift");
    if (@offsetOf(R4XStartR4Desk,"program_spawn_with_console_host_handle") != 416) @compileError("Native program Desk.program_spawn_with_console_host_handle table offset drift");
    if (@offsetOf(R4XStartR4Desk,"program_set_window_handle") != 424) @compileError("Native program Desk.program_set_window_handle table offset drift");
    if (@offsetOf(R4XStartR4Desk,"console_push_input") != 432) @compileError("Native program Desk.console_push_input table offset drift");
    if (@offsetOf(R4XStartR4Desk,"remote_frame_acquire") != 440) @compileError("Native program Desk.remote_frame_acquire table offset drift");
    if (@offsetOf(R4XStartR4Desk,"remote_frame_release") != 448) @compileError("Native program Desk.remote_frame_release table offset drift");
    if (@offsetOf(R4XStartR4Desk,"remote_frame_consumers") != 456) @compileError("Native program Desk.remote_frame_consumers table offset drift");
    if (@offsetOf(R4XStartR4Desk,"remote_frame_publish_regions") != 464) @compileError("Native program Desk.remote_frame_publish_regions table offset drift");
    if (@offsetOf(R4XStartR4Desk,"console_input_wait") != 472) @compileError("Native program Desk.console_input_wait table offset drift");
    if (@offsetOf(R4XStartR4Desk,"physical_key_poll") != 480) @compileError("Native program Desk.physical_key_poll table offset drift");
    if (@offsetOf(R4XStartR4Desk,"mouse_motion") != 488) @compileError("Native program Desk.mouse_motion table offset drift");
    if (@offsetOf(R4XStartR4Desk,"remote_frame_snapshot_acquire") != 496) @compileError("Native program Desk.remote_frame_snapshot_acquire table offset drift");
    if (@offsetOf(R4XStartR4Desk,"remote_frame_snapshot_release") != 504) @compileError("Native program Desk.remote_frame_snapshot_release table offset drift");
    if (@offsetOf(R4XStartR4Desk,"remote_frame_source_reset") != 512) @compileError("Native program Desk.remote_frame_source_reset table offset drift");
    if (@offsetOf(R4XStartR4Desk,"remote_frame_capture_stats") != 520) @compileError("Native program Desk.remote_frame_capture_stats table offset drift");
    if (@offsetOf(R4XStartR4Desk,"desktop_activity_notify") != 528) @compileError("Native program Desk.desktop_activity_notify table offset drift");
    for (std.meta.fields(r4x_api.R4DeskProvider), 0..) |field, i| {
        const names = .{"read_key","mouse_state","mouse_show","mouse_hide","keyboard_layout_current","keyboard_layout_at","keyboard_layout_set","program_set_window","program_set_console_host","program_request_host_launch","program_take_host_launch","program_window_id","gui_window_info","gui_set_window_info","gui_poll_event","gui_push_event","gui_set_text","gui_text","gui_revision","gui_command","gui_set_title","gui_title","gui_set_min_size","gui_min_size","console_output","console_revision","console_state","console_set_metrics","console_push_key","console_write","console_read","clipboard_write","clipboard_read","clipboard_revision","clipboard_info","clipboard_clear","read_key_codepoint","remote_frame_info","remote_frame_read","remote_frame_wait","remote_frame_publish","remote_input_push","remote_input_poll","remote_input_status","desktop_activity_wait","remote_frame_map","program_current_console_host","program_request_desktop","program_spawn_with_console_host","program_spawn_with_console_host_handle","program_set_window_handle","console_push_input","remote_frame_acquire","remote_frame_release","remote_frame_consumers","remote_frame_publish_regions","console_input_wait","physical_key_poll","mouse_motion","remote_frame_snapshot_acquire","remote_frame_snapshot_release","remote_frame_source_reset","remote_frame_capture_stats","desktop_activity_notify"};
        if (!std.mem.eql(u8, field.name, names[i])) @compileError("Native program Desk provider ordinal drift");
    }
    if (@sizeOf(R4XStartR4Draw) != 912 or std.meta.fields(r4x_api.R4DrawProvider).len != 112) @compileError("Native program Draw table/provider shape drift");
    if (@offsetOf(R4XStartR4Draw,"magic") != 0) @compileError("Native program Draw.magic table offset drift");
    if (@offsetOf(R4XStartR4Draw,"abi_version") != 4) @compileError("Native program Draw.abi_version table offset drift");
    if (@offsetOf(R4XStartR4Draw,"size") != 8) @compileError("Native program Draw.size table offset drift");
    if (@offsetOf(R4XStartR4Draw,"flags") != 12) @compileError("Native program Draw.flags table offset drift");
    if (@offsetOf(R4XStartR4Draw,"screen_width") != 16) @compileError("Native program Draw.screen_width table offset drift");
    if (@offsetOf(R4XStartR4Draw,"screen_height") != 24) @compileError("Native program Draw.screen_height table offset drift");
    if (@offsetOf(R4XStartR4Draw,"clear") != 32) @compileError("Native program Draw.clear table offset drift");
    if (@offsetOf(R4XStartR4Draw,"rect") != 40) @compileError("Native program Draw.rect table offset drift");
    if (@offsetOf(R4XStartR4Draw,"text") != 48) @compileError("Native program Draw.text table offset drift");
    if (@offsetOf(R4XStartR4Draw,"display_revision") != 56) @compileError("Native program Draw.display_revision table offset drift");
    if (@offsetOf(R4XStartR4Draw,"display_begin_frame") != 64) @compileError("Native program Draw.display_begin_frame table offset drift");
    if (@offsetOf(R4XStartR4Draw,"display_begin_frame_rect") != 72) @compileError("Native program Draw.display_begin_frame_rect table offset drift");
    if (@offsetOf(R4XStartR4Draw,"display_present") != 80) @compileError("Native program Draw.display_present table offset drift");
    if (@offsetOf(R4XStartR4Draw,"display_blit_xrgb32") != 88) @compileError("Native program Draw.display_blit_xrgb32 table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gui_clear") != 96) @compileError("Native program Draw.gui_clear table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gui_rect") != 104) @compileError("Native program Draw.gui_rect table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gui_draw_text") != 112) @compileError("Native program Draw.gui_draw_text table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gui_draw_text_ex") != 120) @compileError("Native program Draw.gui_draw_text_ex table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gui_blit") != 128) @compileError("Native program Draw.gui_blit table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gui_raster_read") != 136) @compileError("Native program Draw.gui_raster_read table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gui_present") != 144) @compileError("Native program Draw.gui_present table offset drift");
    if (@offsetOf(R4XStartR4Draw,"font_count") != 152) @compileError("Native program Draw.font_count table offset drift");
    if (@offsetOf(R4XStartR4Draw,"font_info") != 160) @compileError("Native program Draw.font_info table offset drift");
    if (@offsetOf(R4XStartR4Draw,"font_measure") != 168) @compileError("Native program Draw.font_measure table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gui_set_font") != 176) @compileError("Native program Draw.gui_set_font table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gui_font") != 184) @compileError("Native program Draw.gui_font table offset drift");
    if (@offsetOf(R4XStartR4Draw,"text_font") != 192) @compileError("Native program Draw.text_font table offset drift");
    if (@offsetOf(R4XStartR4Draw,"font_reload") != 200) @compileError("Native program Draw.font_reload table offset drift");
    if (@offsetOf(R4XStartR4Draw,"font_glyph_row") != 208) @compileError("Native program Draw.font_glyph_row table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gui_blend_alpha8") != 216) @compileError("Native program Draw.gui_blend_alpha8 table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gui_frame_begin") != 224) @compileError("Native program Draw.gui_frame_begin table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gui_frame_append") != 232) @compileError("Native program Draw.gui_frame_append table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gui_frame_commit") != 240) @compileError("Native program Draw.gui_frame_commit table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gui_frame_cancel") != 248) @compileError("Native program Draw.gui_frame_cancel table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gui_frame_info") != 256) @compileError("Native program Draw.gui_frame_info table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gui_frame_read") != 264) @compileError("Native program Draw.gui_frame_read table offset drift");
    if (@offsetOf(R4XStartR4Draw,"display_blit_xrgb32_stride") != 272) @compileError("Native program Draw.display_blit_xrgb32_stride table offset drift");
    if (@offsetOf(R4XStartR4Draw,"display_present_regions") != 280) @compileError("Native program Draw.display_present_regions table offset drift");
    if (@offsetOf(R4XStartR4Draw,"display_present_capabilities") != 288) @compileError("Native program Draw.display_present_capabilities table offset drift");
    if (@offsetOf(R4XStartR4Draw,"display_present_completion") != 296) @compileError("Native program Draw.display_present_completion table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gui_frame_begin_damage") != 304) @compileError("Native program Draw.gui_frame_begin_damage table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gui_frame_generation_info") != 312) @compileError("Native program Draw.gui_frame_generation_info table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gui_frame_generation_read") != 320) @compileError("Native program Draw.gui_frame_generation_read table offset drift");
    if (@offsetOf(R4XStartR4Draw,"font_glyph_bitmap") != 328) @compileError("Native program Draw.font_glyph_bitmap table offset drift");
    if (@offsetOf(R4XStartR4Draw,"font_revision") != 336) @compileError("Native program Draw.font_revision table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gui_frame_begin_replace") != 344) @compileError("Native program Draw.gui_frame_begin_replace table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gui_frame_stream_info") != 352) @compileError("Native program Draw.gui_frame_stream_info table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gui_shared_raster_create") != 360) @compileError("Native program Draw.gui_shared_raster_create table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gui_shared_raster_destroy") != 368) @compileError("Native program Draw.gui_shared_raster_destroy table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gui_shared_raster_map_write") != 376) @compileError("Native program Draw.gui_shared_raster_map_write table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gui_shared_raster_publish") != 384) @compileError("Native program Draw.gui_shared_raster_publish table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gui_shared_raster_acquire") != 392) @compileError("Native program Draw.gui_shared_raster_acquire table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gui_shared_raster_release") != 400) @compileError("Native program Draw.gui_shared_raster_release table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_buffer_create") != 408) @compileError("Native program Draw.gfx_buffer_create table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_buffer_describe") != 416) @compileError("Native program Draw.gfx_buffer_describe table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_buffer_import") != 424) @compileError("Native program Draw.gfx_buffer_import table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_buffer_release") != 432) @compileError("Native program Draw.gfx_buffer_release table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_buffer_map") != 440) @compileError("Native program Draw.gfx_buffer_map table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_buffer_unmap") != 448) @compileError("Native program Draw.gfx_buffer_unmap table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_buffer_export_raster") != 456) @compileError("Native program Draw.gfx_buffer_export_raster table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_buffer_stats") != 464) @compileError("Native program Draw.gfx_buffer_stats table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_queue_open") != 472) @compileError("Native program Draw.gfx_queue_open table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_queue_close") != 480) @compileError("Native program Draw.gfx_queue_close table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_queue_submit") != 488) @compileError("Native program Draw.gfx_queue_submit table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_fence_query") != 496) @compileError("Native program Draw.gfx_fence_query table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_fence_wait") != 504) @compileError("Native program Draw.gfx_fence_wait table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_fence_cancel") != 512) @compileError("Native program Draw.gfx_fence_cancel table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_fence_release") != 520) @compileError("Native program Draw.gfx_fence_release table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_queue_backend") != 528) @compileError("Native program Draw.gfx_queue_backend table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_output_revision") != 536) @compileError("Native program Draw.gfx_output_revision table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_output_info") != 544) @compileError("Native program Draw.gfx_output_info table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_output_mode") != 552) @compileError("Native program Draw.gfx_output_mode table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_output_edid") != 560) @compileError("Native program Draw.gfx_output_edid table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_atomic_test") != 568) @compileError("Native program Draw.gfx_atomic_test table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_atomic_commit") != 576) @compileError("Native program Draw.gfx_atomic_commit table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_atomic_submit") != 584) @compileError("Native program Draw.gfx_atomic_submit table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_atomic_status") != 592) @compileError("Native program Draw.gfx_atomic_status table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_atomic_resolve") != 600) @compileError("Native program Draw.gfx_atomic_resolve table offset drift");
    if (@offsetOf(R4XStartR4Draw,"display_presentation_stats") != 608) @compileError("Native program Draw.display_presentation_stats table offset drift");
    if (@offsetOf(R4XStartR4Draw,"display_cursor_info") != 616) @compileError("Native program Draw.display_cursor_info table offset drift");
    if (@offsetOf(R4XStartR4Draw,"display_cursor_submit") != 624) @compileError("Native program Draw.display_cursor_submit table offset drift");
    if (@offsetOf(R4XStartR4Draw,"display_cursor_status") != 632) @compileError("Native program Draw.display_cursor_status table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_queue_backend_info") != 640) @compileError("Native program Draw.gfx_queue_backend_info table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_native_start") != 648) @compileError("Native program Draw.gfx_native_start table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_native_query") != 656) @compileError("Native program Draw.gfx_native_query table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_native_receive") != 664) @compileError("Native program Draw.gfx_native_receive table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_native_close") != 672) @compileError("Native program Draw.gfx_native_close table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_native_wait") != 680) @compileError("Native program Draw.gfx_native_wait table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_queue_submit_render_list") != 688) @compileError("Native program Draw.gfx_queue_submit_render_list table offset drift");
    if (@offsetOf(R4XStartR4Draw,"display_presentation_info") != 696) @compileError("Native program Draw.display_presentation_info table offset drift");
    if (@offsetOf(R4XStartR4Draw,"display_presentation_feedback") != 704) @compileError("Native program Draw.display_presentation_feedback table offset drift");
    if (@offsetOf(R4XStartR4Draw,"display_output_target") != 712) @compileError("Native program Draw.display_output_target table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_queue_submit_output") != 720) @compileError("Native program Draw.gfx_queue_submit_output table offset drift");
    if (@offsetOf(R4XStartR4Draw,"display_output_presentation_info") != 728) @compileError("Native program Draw.display_output_presentation_info table offset drift");
    if (@offsetOf(R4XStartR4Draw,"display_output_presentation_feedback") != 736) @compileError("Native program Draw.display_output_presentation_feedback table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_queue_submit_render_grid_list") != 744) @compileError("Native program Draw.gfx_queue_submit_render_grid_list table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_output_color") != 752) @compileError("Native program Draw.gfx_output_color table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_queue_submit_render_color_list") != 760) @compileError("Native program Draw.gfx_queue_submit_render_color_list table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_atomic_test_color") != 768) @compileError("Native program Draw.gfx_atomic_test_color table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_atomic_submit_color") != 776) @compileError("Native program Draw.gfx_atomic_submit_color table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_output_refresh") != 784) @compileError("Native program Draw.gfx_output_refresh table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_refresh_request") != 792) @compileError("Native program Draw.gfx_refresh_request table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_memory_budget") != 800) @compileError("Native program Draw.gfx_memory_budget table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_telemetry") != 808) @compileError("Native program Draw.gfx_telemetry table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_output_power") != 816) @compileError("Native program Draw.gfx_output_power table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_power_request") != 824) @compileError("Native program Draw.gfx_power_request table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_virtual_start") != 832) @compileError("Native program Draw.gfx_virtual_start table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_virtual_query") != 840) @compileError("Native program Draw.gfx_virtual_query table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_virtual_close") != 848) @compileError("Native program Draw.gfx_virtual_close table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_virtual_wait") != 856) @compileError("Native program Draw.gfx_virtual_wait table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_buffer_map_persistent") != 864) @compileError("Native program Draw.gfx_buffer_map_persistent table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_queue_backend_properties") != 872) @compileError("Native program Draw.gfx_queue_backend_properties table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_queue_submit_native") != 880) @compileError("Native program Draw.gfx_queue_submit_native table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_queue_submit_render_color_grid_list") != 888) @compileError("Native program Draw.gfx_queue_submit_render_color_grid_list table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_output_brightness") != 896) @compileError("Native program Draw.gfx_output_brightness table offset drift");
    if (@offsetOf(R4XStartR4Draw,"gfx_brightness_request") != 904) @compileError("Native program Draw.gfx_brightness_request table offset drift");
    for (std.meta.fields(r4x_api.R4DrawProvider), 0..) |field, i| {
        const names = .{"screen_width","screen_height","clear","rect","text","display_revision","display_begin_frame","display_begin_frame_rect","display_present","display_blit_xrgb32","gui_clear","gui_rect","gui_draw_text","gui_draw_text_ex","gui_blit","gui_raster_read","gui_present","font_count","font_info","font_measure","gui_set_font","gui_font","text_font","font_reload","font_glyph_row","gui_blend_alpha8","gui_frame_begin","gui_frame_append","gui_frame_commit","gui_frame_cancel","gui_frame_info","gui_frame_read","display_blit_xrgb32_stride","display_present_regions","display_present_capabilities","display_present_completion","gui_frame_begin_damage","gui_frame_generation_info","gui_frame_generation_read","font_glyph_bitmap","font_revision","gui_frame_begin_replace","gui_frame_stream_info","gui_shared_raster_create","gui_shared_raster_destroy","gui_shared_raster_map_write","gui_shared_raster_publish","gui_shared_raster_acquire","gui_shared_raster_release","gfx_buffer_create","gfx_buffer_describe","gfx_buffer_import","gfx_buffer_release","gfx_buffer_map","gfx_buffer_unmap","gfx_buffer_export_raster","gfx_buffer_stats","gfx_queue_open","gfx_queue_close","gfx_queue_submit","gfx_fence_query","gfx_fence_wait","gfx_fence_cancel","gfx_fence_release","gfx_queue_backend","gfx_output_revision","gfx_output_info","gfx_output_mode","gfx_output_edid","gfx_atomic_test","gfx_atomic_commit","gfx_atomic_submit","gfx_atomic_status","gfx_atomic_resolve","display_presentation_stats","display_cursor_info","display_cursor_submit","display_cursor_status","gfx_queue_backend_info","gfx_native_start","gfx_native_query","gfx_native_receive","gfx_native_close","gfx_native_wait","gfx_queue_submit_render_list","display_presentation_info","display_presentation_feedback","display_output_target","gfx_queue_submit_output","display_output_presentation_info","display_output_presentation_feedback","gfx_queue_submit_render_grid_list","gfx_output_color","gfx_queue_submit_render_color_list","gfx_atomic_test_color","gfx_atomic_submit_color","gfx_output_refresh","gfx_refresh_request","gfx_memory_budget","gfx_telemetry","gfx_output_power","gfx_power_request","gfx_virtual_start","gfx_virtual_query","gfx_virtual_close","gfx_virtual_wait","gfx_buffer_map_persistent","gfx_queue_backend_properties","gfx_queue_submit_native","gfx_queue_submit_render_color_grid_list","gfx_output_brightness","gfx_brightness_request"};
        if (!std.mem.eql(u8, field.name, names[i])) @compileError("Native program Draw provider ordinal drift");
    }
    if (@sizeOf(R4XStartR4Net) != 296 or std.meta.fields(r4x_api.R4NetProvider).len != 35) @compileError("Native program Net table/provider shape drift");
    if (@offsetOf(R4XStartR4Net,"magic") != 0) @compileError("Native program Net.magic table offset drift");
    if (@offsetOf(R4XStartR4Net,"abi_version") != 4) @compileError("Native program Net.abi_version table offset drift");
    if (@offsetOf(R4XStartR4Net,"size") != 8) @compileError("Native program Net.size table offset drift");
    if (@offsetOf(R4XStartR4Net,"flags") != 12) @compileError("Native program Net.flags table offset drift");
    if (@offsetOf(R4XStartR4Net,"tcp_connect") != 16) @compileError("Native program Net.tcp_connect table offset drift");
    if (@offsetOf(R4XStartR4Net,"tcp_write") != 24) @compileError("Native program Net.tcp_write table offset drift");
    if (@offsetOf(R4XStartR4Net,"tcp_read") != 32) @compileError("Native program Net.tcp_read table offset drift");
    if (@offsetOf(R4XStartR4Net,"tcp_close") != 40) @compileError("Native program Net.tcp_close table offset drift");
    if (@offsetOf(R4XStartR4Net,"tcp_summary") != 48) @compileError("Native program Net.tcp_summary table offset drift");
    if (@offsetOf(R4XStartR4Net,"tcp_connection") != 56) @compileError("Native program Net.tcp_connection table offset drift");
    if (@offsetOf(R4XStartR4Net,"tcp_echo_listen_once") != 64) @compileError("Native program Net.tcp_echo_listen_once table offset drift");
    if (@offsetOf(R4XStartR4Net,"tcp_accept_read_once") != 72) @compileError("Native program Net.tcp_accept_read_once table offset drift");
    if (@offsetOf(R4XStartR4Net,"net_ipv4_send") != 80) @compileError("Native program Net.net_ipv4_send table offset drift");
    if (@offsetOf(R4XStartR4Net,"net_ipv4_recv") != 88) @compileError("Native program Net.net_ipv4_recv table offset drift");
    if (@offsetOf(R4XStartR4Net,"net_config_get") != 96) @compileError("Native program Net.net_config_get table offset drift");
    if (@offsetOf(R4XStartR4Net,"net_config_set") != 104) @compileError("Native program Net.net_config_set table offset drift");
    if (@offsetOf(R4XStartR4Net,"net_dns_resolve") != 112) @compileError("Native program Net.net_dns_resolve table offset drift");
    if (@offsetOf(R4XStartR4Net,"net_dns_resolve_server") != 120) @compileError("Native program Net.net_dns_resolve_server table offset drift");
    if (@offsetOf(R4XStartR4Net,"net_dhcp_acquire") != 128) @compileError("Native program Net.net_dhcp_acquire table offset drift");
    if (@offsetOf(R4XStartR4Net,"net_dhcp_renew") != 136) @compileError("Native program Net.net_dhcp_renew table offset drift");
    if (@offsetOf(R4XStartR4Net,"net_dhcp_release") != 144) @compileError("Native program Net.net_dhcp_release table offset drift");
    if (@offsetOf(R4XStartR4Net,"net_dhcp_status") != 152) @compileError("Native program Net.net_dhcp_status table offset drift");
    if (@offsetOf(R4XStartR4Net,"net_detail_get") != 160) @compileError("Native program Net.net_detail_get table offset drift");
    if (@offsetOf(R4XStartR4Net,"net_diag_run") != 168) @compileError("Native program Net.net_diag_run table offset drift");
    if (@offsetOf(R4XStartR4Net,"ipc_open") != 176) @compileError("Native program Net.ipc_open table offset drift");
    if (@offsetOf(R4XStartR4Net,"ipc_send") != 184) @compileError("Native program Net.ipc_send table offset drift");
    if (@offsetOf(R4XStartR4Net,"ipc_recv") != 192) @compileError("Native program Net.ipc_recv table offset drift");
    if (@offsetOf(R4XStartR4Net,"ipc_poll") != 200) @compileError("Native program Net.ipc_poll table offset drift");
    if (@offsetOf(R4XStartR4Net,"ipc_close") != 208) @compileError("Native program Net.ipc_close table offset drift");
    if (@offsetOf(R4XStartR4Net,"ipc_summary") != 216) @compileError("Native program Net.ipc_summary table offset drift");
    if (@offsetOf(R4XStartR4Net,"ipc_channel") != 224) @compileError("Native program Net.ipc_channel table offset drift");
    if (@offsetOf(R4XStartR4Net,"serial_link_status") != 232) @compileError("Native program Net.serial_link_status table offset drift");
    if (@offsetOf(R4XStartR4Net,"serial_link_poll") != 240) @compileError("Native program Net.serial_link_poll table offset drift");
    if (@offsetOf(R4XStartR4Net,"serial_link_send_message") != 248) @compileError("Native program Net.serial_link_send_message table offset drift");
    if (@offsetOf(R4XStartR4Net,"serial_link_host_test") != 256) @compileError("Native program Net.serial_link_host_test table offset drift");
    if (@offsetOf(R4XStartR4Net,"serial_link_inbox") != 264) @compileError("Native program Net.serial_link_inbox table offset drift");
    if (@offsetOf(R4XStartR4Net,"net_service_request") != 272) @compileError("Native program Net.net_service_request table offset drift");
    if (@offsetOf(R4XStartR4Net,"ipc_performance") != 280) @compileError("Native program Net.ipc_performance table offset drift");
    if (@offsetOf(R4XStartR4Net,"tcp_performance") != 288) @compileError("Native program Net.tcp_performance table offset drift");
    for (std.meta.fields(r4x_api.R4NetProvider), 0..) |field, i| {
        const names = .{"tcp_connect","tcp_write","tcp_read","tcp_close","tcp_summary","tcp_connection","tcp_echo_listen_once","tcp_accept_read_once","net_ipv4_send","net_ipv4_recv","net_config_get","net_config_set","net_dns_resolve","net_dns_resolve_server","net_dhcp_acquire","net_dhcp_renew","net_dhcp_release","net_dhcp_status","net_detail_get","net_diag_run","ipc_open","ipc_send","ipc_recv","ipc_poll","ipc_close","ipc_summary","ipc_channel","serial_link_status","serial_link_poll","serial_link_send_message","serial_link_host_test","serial_link_inbox","net_service_request","ipc_performance","tcp_performance"};
        if (!std.mem.eql(u8, field.name, names[i])) @compileError("Native program Net provider ordinal drift");
    }
    if (@sizeOf(R4XStartR4Audio) != 200 or std.meta.fields(r4x_api.R4AudioProvider).len != 21) @compileError("Native program Audio table/provider shape drift");
    if (@offsetOf(R4XStartR4Audio,"magic") != 0) @compileError("Native program Audio.magic table offset drift");
    if (@offsetOf(R4XStartR4Audio,"abi_version") != 4) @compileError("Native program Audio.abi_version table offset drift");
    if (@offsetOf(R4XStartR4Audio,"size") != 8) @compileError("Native program Audio.size table offset drift");
    if (@offsetOf(R4XStartR4Audio,"flags") != 12) @compileError("Native program Audio.flags table offset drift");
    if (@offsetOf(R4XStartR4Audio,"audio_open_stream") != 16) @compileError("Native program Audio.audio_open_stream table offset drift");
    if (@offsetOf(R4XStartR4Audio,"audio_write") != 24) @compileError("Native program Audio.audio_write table offset drift");
    if (@offsetOf(R4XStartR4Audio,"audio_close") != 32) @compileError("Native program Audio.audio_close table offset drift");
    if (@offsetOf(R4XStartR4Audio,"audio_set_volume") != 40) @compileError("Native program Audio.audio_set_volume table offset drift");
    if (@offsetOf(R4XStartR4Audio,"sid_acquire") != 48) @compileError("Native program Audio.sid_acquire table offset drift");
    if (@offsetOf(R4XStartR4Audio,"sid_write_register") != 56) @compileError("Native program Audio.sid_write_register table offset drift");
    if (@offsetOf(R4XStartR4Audio,"sid_release") != 64) @compileError("Native program Audio.sid_release table offset drift");
    if (@offsetOf(R4XStartR4Audio,"sid_load_data") != 72) @compileError("Native program Audio.sid_load_data table offset drift");
    if (@offsetOf(R4XStartR4Audio,"sid_init") != 80) @compileError("Native program Audio.sid_init table offset drift");
    if (@offsetOf(R4XStartR4Audio,"sid_play_frame") != 88) @compileError("Native program Audio.sid_play_frame table offset drift");
    if (@offsetOf(R4XStartR4Audio,"sid_stop") != 96) @compileError("Native program Audio.sid_stop table offset drift");
    if (@offsetOf(R4XStartR4Audio,"sid_model_name") != 104) @compileError("Native program Audio.sid_model_name table offset drift");
    if (@offsetOf(R4XStartR4Audio,"midi_open_synth") != 112) @compileError("Native program Audio.midi_open_synth table offset drift");
    if (@offsetOf(R4XStartR4Audio,"midi_send") != 120) @compileError("Native program Audio.midi_send table offset drift");
    if (@offsetOf(R4XStartR4Audio,"midi_close") != 128) @compileError("Native program Audio.midi_close table offset drift");
    if (@offsetOf(R4XStartR4Audio,"opl3_write_register") != 136) @compileError("Native program Audio.opl3_write_register table offset drift");
    if (@offsetOf(R4XStartR4Audio,"opl3_reset") != 144) @compileError("Native program Audio.opl3_reset table offset drift");
    if (@offsetOf(R4XStartR4Audio,"opl3_render_block") != 152) @compileError("Native program Audio.opl3_render_block table offset drift");
    if (@offsetOf(R4XStartR4Audio,"opl3_stop") != 160) @compileError("Native program Audio.opl3_stop table offset drift");
    if (@offsetOf(R4XStartR4Audio,"reserved0") != 168) @compileError("Native program Audio.reserved0 table offset drift");
    if (@offsetOf(R4XStartR4Audio,"reserved1") != 176) @compileError("Native program Audio.reserved1 table offset drift");
    if (@offsetOf(R4XStartR4Audio,"audio_output_info") != 184) @compileError("Native program Audio.audio_output_info table offset drift");
    if (@offsetOf(R4XStartR4Audio,"audio_select_output") != 192) @compileError("Native program Audio.audio_select_output table offset drift");
    for (std.meta.fields(r4x_api.R4AudioProvider), 0..) |field, i| {
        const names = .{"audio_open_stream","audio_write","audio_close","audio_set_volume","sid_acquire","sid_write_register","sid_release","sid_load_data","sid_init","sid_play_frame","sid_stop","sid_model_name","midi_open_synth","midi_send","midi_close","opl3_write_register","opl3_reset","opl3_render_block","opl3_stop","audio_output_info","audio_select_output"};
        if (!std.mem.eql(u8, field.name, names[i])) @compileError("Native program Audio provider ordinal drift");
    }
    if (@sizeOf(R4XStartR4Dev) != 392 or std.meta.fields(r4x_api.R4DevProvider).len != 45) @compileError("Native program Dev table/provider shape drift");
    if (@offsetOf(R4XStartR4Dev,"magic") != 0) @compileError("Native program Dev.magic table offset drift");
    if (@offsetOf(R4XStartR4Dev,"abi_version") != 4) @compileError("Native program Dev.abi_version table offset drift");
    if (@offsetOf(R4XStartR4Dev,"size") != 8) @compileError("Native program Dev.size table offset drift");
    if (@offsetOf(R4XStartR4Dev,"flags") != 12) @compileError("Native program Dev.flags table offset drift");
    if (@offsetOf(R4XStartR4Dev,"device_inventory_summary") != 16) @compileError("Native program Dev.device_inventory_summary table offset drift");
    if (@offsetOf(R4XStartR4Dev,"device_inventory_record") != 24) @compileError("Native program Dev.device_inventory_record table offset drift");
    if (@offsetOf(R4XStartR4Dev,"memory_summary") != 32) @compileError("Native program Dev.memory_summary table offset drift");
    if (@offsetOf(R4XStartR4Dev,"memory_block_count") != 40) @compileError("Native program Dev.memory_block_count table offset drift");
    if (@offsetOf(R4XStartR4Dev,"memory_block") != 48) @compileError("Native program Dev.memory_block table offset drift");
    if (@offsetOf(R4XStartR4Dev,"memory_pressure_snapshot") != 56) @compileError("Native program Dev.memory_pressure_snapshot table offset drift");
    if (@offsetOf(R4XStartR4Dev,"memory_reclaim_probe") != 64) @compileError("Native program Dev.memory_reclaim_probe table offset drift");
    if (@offsetOf(R4XStartR4Dev,"memory_backing_store_probe") != 72) @compileError("Native program Dev.memory_backing_store_probe table offset drift");
    if (@offsetOf(R4XStartR4Dev,"memory_backing_store_slot_probe") != 80) @compileError("Native program Dev.memory_backing_store_slot_probe table offset drift");
    if (@offsetOf(R4XStartR4Dev,"memory_pager_gate_probe") != 88) @compileError("Native program Dev.memory_pager_gate_probe table offset drift");
    if (@offsetOf(R4XStartR4Dev,"memory_page_io_probe") != 96) @compileError("Native program Dev.memory_page_io_probe table offset drift");
    if (@offsetOf(R4XStartR4Dev,"memory_vm_page_state_probe") != 104) @compileError("Native program Dev.memory_vm_page_state_probe table offset drift");
    if (@offsetOf(R4XStartR4Dev,"memory_vm_reserve_probe") != 112) @compileError("Native program Dev.memory_vm_reserve_probe table offset drift");
    if (@offsetOf(R4XStartR4Dev,"paging_summary") != 120) @compileError("Native program Dev.paging_summary table offset drift");
    if (@offsetOf(R4XStartR4Dev,"performance_summary") != 128) @compileError("Native program Dev.performance_summary table offset drift");
    if (@offsetOf(R4XStartR4Dev,"performance_task") != 136) @compileError("Native program Dev.performance_task table offset drift");
    if (@offsetOf(R4XStartR4Dev,"performance_storage") != 144) @compileError("Native program Dev.performance_storage table offset drift");
    if (@offsetOf(R4XStartR4Dev,"performance_boot_phase") != 152) @compileError("Native program Dev.performance_boot_phase table offset drift");
    if (@offsetOf(R4XStartR4Dev,"protocol_status") != 160) @compileError("Native program Dev.protocol_status table offset drift");
    if (@offsetOf(R4XStartR4Dev,"protocol_dispatch") != 168) @compileError("Native program Dev.protocol_dispatch table offset drift");
    if (@offsetOf(R4XStartR4Dev,"display_summary") != 176) @compileError("Native program Dev.display_summary table offset drift");
    if (@offsetOf(R4XStartR4Dev,"hardware_summary") != 184) @compileError("Native program Dev.hardware_summary table offset drift");
    if (@offsetOf(R4XStartR4Dev,"boot_info_summary") != 192) @compileError("Native program Dev.boot_info_summary table offset drift");
    if (@offsetOf(R4XStartR4Dev,"boot_info_memory_count") != 200) @compileError("Native program Dev.boot_info_memory_count table offset drift");
    if (@offsetOf(R4XStartR4Dev,"boot_info_memory_entry") != 208) @compileError("Native program Dev.boot_info_memory_entry table offset drift");
    if (@offsetOf(R4XStartR4Dev,"reserved0") != 216) @compileError("Native program Dev.reserved0 table offset drift");
    if (@offsetOf(R4XStartR4Dev,"reserved1") != 224) @compileError("Native program Dev.reserved1 table offset drift");
    if (@offsetOf(R4XStartR4Dev,"program_instance_storage_summary") != 232) @compileError("Native program Dev.program_instance_storage_summary table offset drift");
    if (@offsetOf(R4XStartR4Dev,"program_instance_storage_self_test") != 240) @compileError("Native program Dev.program_instance_storage_self_test table offset drift");
    if (@offsetOf(R4XStartR4Dev,"program_registry_summary") != 248) @compileError("Native program Dev.program_registry_summary table offset drift");
    if (@offsetOf(R4XStartR4Dev,"program_registry_self_test") != 256) @compileError("Native program Dev.program_registry_self_test table offset drift");
    if (@offsetOf(R4XStartR4Dev,"program_registry_summary_v2") != 264) @compileError("Native program Dev.program_registry_summary_v2 table offset drift");
    if (@offsetOf(R4XStartR4Dev,"program_registry_self_test_v2") != 272) @compileError("Native program Dev.program_registry_self_test_v2 table offset drift");
    if (@offsetOf(R4XStartR4Dev,"execution_inventory_summary") != 280) @compileError("Native program Dev.execution_inventory_summary table offset drift");
    if (@offsetOf(R4XStartR4Dev,"program_instance_storage_summary_v2") != 288) @compileError("Native program Dev.program_instance_storage_summary_v2 table offset drift");
    if (@offsetOf(R4XStartR4Dev,"kernel_version") != 296) @compileError("Native program Dev.kernel_version table offset drift");
    if (@offsetOf(R4XStartR4Dev,"performance_boot_phase_clock") != 304) @compileError("Native program Dev.performance_boot_phase_clock table offset drift");
    if (@offsetOf(R4XStartR4Dev,"performance_irq_timing") != 312) @compileError("Native program Dev.performance_irq_timing table offset drift");
    if (@offsetOf(R4XStartR4Dev,"performance_boot_summary") != 320) @compileError("Native program Dev.performance_boot_summary table offset drift");
    if (@offsetOf(R4XStartR4Dev,"performance_driver_work") != 328) @compileError("Native program Dev.performance_driver_work table offset drift");
    if (@offsetOf(R4XStartR4Dev,"performance_pci_inventory") != 336) @compileError("Native program Dev.performance_pci_inventory table offset drift");
    if (@offsetOf(R4XStartR4Dev,"performance_input") != 344) @compileError("Native program Dev.performance_input table offset drift");
    if (@offsetOf(R4XStartR4Dev,"display_state") != 352) @compileError("Native program Dev.display_state table offset drift");
    if (@offsetOf(R4XStartR4Dev,"driver_module_info") != 360) @compileError("Native program Dev.driver_module_info table offset drift");
    if (@offsetOf(R4XStartR4Dev,"performance_instrumentation") != 368) @compileError("Native program Dev.performance_instrumentation table offset drift");
    if (@offsetOf(R4XStartR4Dev,"performance_lock") != 376) @compileError("Native program Dev.performance_lock table offset drift");
    if (@offsetOf(R4XStartR4Dev,"performance_stack") != 384) @compileError("Native program Dev.performance_stack table offset drift");
    for (std.meta.fields(r4x_api.R4DevProvider), 0..) |field, i| {
        const names = .{"device_inventory_summary","device_inventory_record","memory_summary","memory_block_count","memory_block","memory_pressure_snapshot","memory_reclaim_probe","memory_backing_store_probe","memory_backing_store_slot_probe","memory_pager_gate_probe","memory_page_io_probe","memory_vm_page_state_probe","memory_vm_reserve_probe","paging_summary","performance_summary","performance_task","performance_storage","performance_boot_phase","protocol_status","protocol_dispatch","display_summary","hardware_summary","boot_info_summary","boot_info_memory_count","boot_info_memory_entry","program_instance_storage_summary","program_instance_storage_self_test","program_registry_summary","program_registry_self_test","program_registry_summary_v2","program_registry_self_test_v2","execution_inventory_summary","program_instance_storage_summary_v2","kernel_version","performance_boot_phase_clock","performance_irq_timing","performance_boot_summary","performance_driver_work","performance_pci_inventory","performance_input","display_state","driver_module_info","performance_instrumentation","performance_lock","performance_stack"};
        if (!std.mem.eql(u8, field.name, names[i])) @compileError("Native program Dev provider ordinal drift");
    }
}


extern fn r4os_rust_program_peer_cleanup(*ProgramInstance) bool;
extern fn r4os_rust_program_registry_iterator_new(bool, *ProgramRegistryIterator) void;
extern fn r4os_rust_program_peer_notification_owner() ?*anyopaque;
extern fn r4os_rust_program_peer_notification(u8, u64, u64, u64, ?*u64) i32;
extern fn r4os_rust_program_peer_preemptible(u64) bool;
extern fn r4os_rust_program_peer_contains(*const ProgramInstance, u64) bool;
extern fn r4os_rust_program_peer_crash(?*const ProgramInstance, u32, u64, bool, *crash.ExecutionContext) void;
extern fn r4os_rust_program_peer_dump() void;
export fn r4os_zig_program_peer_vm_release(id: u32) callconv(.c) bool { _ = mem_virt.releaseOwner(.r4x_instance, id, null) catch return false; return true; }
export fn r4os_zig_program_peer_backing_release(id: u32) callconv(.c) void { _ = mem_backing_store.releaseR4xOwner(id); }
comptime {
    if (@sizeOf(crash.ExecutionContext) != 64 or @alignOf(crash.ExecutionContext) != 8 or @offsetOf(crash.ExecutionContext,"state") != 0 or @offsetOf(crash.ExecutionContext,"r4x_instance_id") != 4 or @offsetOf(crash.ExecutionContext,"task_id") != 8 or @offsetOf(crash.ExecutionContext,"owner_id") != 16 or @offsetOf(crash.ExecutionContext,"image_base") != 24 or @offsetOf(crash.ExecutionContext,"image_size") != 32 or @offsetOf(crash.ExecutionContext,"instruction_offset") != 40 or @offsetOf(crash.ExecutionContext,"tag") != 48 or @intFromEnum(crash.ContextState.unavailable) != 0 or @intFromEnum(crash.ContextState.available) != 1)
        @compileError("Native IRQ-safe program crash context original geometry drift");
    if (@sizeOf(ProgramRegistryIterator) != 24 or @alignOf(ProgramRegistryIterator) != 8 or @offsetOf(ProgramRegistryIterator,"chunk") != 0 or @offsetOf(ProgramRegistryIterator,"slot_index") != 8 or @offsetOf(ProgramRegistryIterator,"include_done") != 16 or @sizeOf(notifications.Owner) != 24)
        @compileError("Native cooperative iterator/opaque notification owner geometry drift");
}

extern fn r4os_rust_program_domain_audio_open_stream(u32, u16, u16) i32;
extern fn r4os_rust_program_domain_audio_output_info(u32, *audio.AudioOutputInfo) i32;
extern fn r4os_rust_program_domain_audio_select_output(*const [64]u8) i32;
extern fn r4os_rust_program_domain_audio_write(u32, [*]const u8, u32) i32;
extern fn r4os_rust_program_domain_audio_close(u32) i32;
extern fn r4os_rust_program_domain_audio_set_volume(u32, u32) i32;
extern fn r4os_rust_program_domain_remote_frame_acquire() i32;
extern fn r4os_rust_program_domain_remote_frame_release() i32;
extern fn r4os_rust_program_domain_remote_frame_publish(*const r4x_api.RemoteFrameInfo, [*]const u32, u32) i32;
extern fn r4os_rust_program_domain_remote_frame_publish_regions(*const r4x_api.RemoteFrameInfo, [*]const u32, u32, [*]const r4x_api.DisplayDamageRect, u32) i32;
extern fn r4os_rust_program_domain_remote_frame_snapshot_acquire(u32, *r4x_api.RemoteFrameInfo, *r4x_api.RemoteFrameLease) i32;
extern fn r4os_rust_program_domain_remote_frame_snapshot_release(*const r4x_api.RemoteFrameLease) i32;
extern fn r4os_rust_program_domain_remote_frame_source_reset() i32;
extern fn r4os_rust_program_domain_gfx_buffer_create(*const r4x_api.GfxBufferDescriptor, *r4x_api.GfxBufferReference) i32;
extern fn r4os_rust_program_domain_gfx_virtual_start(*const gfx_virtual_api.abi.GfxVirtualRequest, *gfx_virtual_api.abi.GfxVirtualStatus) i32;
extern fn r4os_rust_program_domain_gfx_virtual_query(*const r4x_api.GfxBufferHandle, *gfx_virtual_api.abi.GfxVirtualStatus) i32;
extern fn r4os_rust_program_domain_gfx_virtual_close(*const r4x_api.GfxBufferHandle, u32) i32;
extern fn r4os_rust_program_domain_gfx_virtual_wait(*const r4x_api.GfxBufferHandle, u32, u64, *gfx_virtual_api.abi.GfxVirtualStatus) i32;
extern fn r4os_rust_program_domain_gfx_native_start(*const r4x_api.GfxNativeAllocation, *r4x_api.GfxNativeStatus) i32;
extern fn r4os_rust_program_domain_gfx_native_query(*const r4x_api.GfxBufferHandle, *r4x_api.GfxNativeStatus) i32;
extern fn r4os_rust_program_domain_gfx_native_receive(*const r4x_api.GfxBufferHandle, *r4x_api.GfxBufferReference) i32;
extern fn r4os_rust_program_domain_gfx_native_close(*const r4x_api.GfxBufferHandle) i32;
extern fn r4os_rust_program_domain_gfx_native_wait(*const r4x_api.GfxBufferHandle, u64, *r4x_api.GfxNativeStatus) i32;
extern fn r4os_rust_program_domain_gfx_queue_open(*const gfx_queue_api.abi.GfxQueueConfig, *gfx_queue_api.abi.GfxQueueHandle) i32;
extern fn r4os_rust_program_domain_gfx_atomic_test(*const gfx_output_api.abi.GfxAtomicState, *gfx_output_api.abi.GfxAtomicResult) i32;
extern fn r4os_rust_program_domain_gfx_atomic_commit(*const gfx_output_api.abi.GfxAtomicState, *gfx_output_api.abi.GfxAtomicResult) i32;
extern fn r4os_rust_program_domain_gfx_atomic_submit(*const gfx_output_api.abi.GfxAtomicState, u32, *gfx_output_api.abi.GfxModeStatus) i32;
extern fn r4os_rust_program_domain_gfx_atomic_test_color(*const gfx_output_api.abi.GfxModeColorRequest, *gfx_output_api.abi.GfxAtomicResult) i32;
extern fn r4os_rust_program_domain_gfx_atomic_submit_color(*const gfx_output_api.abi.GfxModeColorRequest, u32, *gfx_output_api.abi.GfxModeStatus) i32;
extern fn r4os_rust_program_domain_gfx_refresh_request(*const gfx_output_api.abi.GfxRefreshRequest, *gfx_output_api.abi.GfxRefreshRequest) i32;
extern fn r4os_rust_program_domain_gfx_power_request(*const gfx_output_api.abi.GfxPowerRequest, *gfx_output_api.abi.GfxPowerRequest) i32;
extern fn r4os_rust_program_domain_gfx_atomic_resolve(u64, u32, *gfx_output_api.abi.GfxModeStatus) i32;
extern fn r4os_rust_program_domain_gfx_queue_close(*const gfx_queue_api.abi.GfxQueueHandle) i32;
extern fn r4os_rust_program_domain_gfx_queue_submit(*const gfx_queue_api.abi.GfxQueueHandle, *const gfx_queue_api.abi.GfxSubmission, *gfx_queue_api.abi.GfxFenceStatus) i32;
extern fn r4os_rust_program_domain_gfx_queue_submit_render_list(*const gfx_queue_api.abi.GfxQueueHandle, *const gfx_queue_api.abi.GfxSubmission, *const gfx_queue_api.abi.GfxRenderList, *gfx_queue_api.abi.GfxFenceStatus) i32;
extern fn r4os_rust_program_domain_gfx_queue_submit_render_grid_list(*const gfx_queue_api.abi.GfxQueueHandle, *const gfx_queue_api.abi.GfxSubmission, *const gfx_queue_api.abi.GfxRenderGridList, *gfx_queue_api.abi.GfxFenceStatus) i32;
extern fn r4os_rust_program_domain_gfx_queue_submit_output(*const gfx_queue_api.abi.GfxQueueHandle, *const gfx_queue_api.abi.GfxSubmission, *const gfx_queue_api.abi.GfxOutputTarget, *gfx_queue_api.abi.GfxFenceStatus) i32;
extern fn r4os_rust_program_domain_gfx_queue_submit_render_color_list(*const gfx_queue_api.abi.GfxQueueHandle, *const gfx_queue_api.abi.GfxSubmission, *const gfx_queue_api.abi.GfxRenderColorList, *gfx_queue_api.abi.GfxFenceStatus) i32;
extern fn r4os_rust_program_domain_gfx_queue_submit_native(*const gfx_queue_api.abi.GfxQueueHandle, *const gfx_queue_api.abi.GfxSubmission, *const gfx_queue_api.abi.GfxNativeSubmission, *gfx_queue_api.abi.GfxFenceStatus) i32;
extern fn r4os_rust_program_domain_gfx_queue_submit_render_color_grid_list(*const gfx_queue_api.abi.GfxQueueHandle, *const gfx_queue_api.abi.GfxSubmission, *const gfx_queue_api.abi.GfxRenderColorGridList, *gfx_queue_api.abi.GfxFenceStatus) i32;
extern fn r4os_rust_program_domain_gfx_fence_cancel(*const gfx_queue_api.abi.GfxFence) i32;
extern fn r4os_rust_program_domain_gfx_fence_release(*const gfx_queue_api.abi.GfxFence) i32;
extern fn r4os_rust_program_domain_gfx_buffer_describe(*const r4x_api.GfxBufferHandle, *r4x_api.GfxBufferDescriptor) i32;
extern fn r4os_rust_program_domain_gfx_buffer_import(*const r4x_api.GfxBufferHandle, *r4x_api.GfxBufferReference) i32;
extern fn r4os_rust_program_domain_gfx_buffer_release(*const r4x_api.GfxBufferHandle) i32;
extern fn r4os_rust_program_domain_gfx_buffer_map(*const r4x_api.GfxBufferHandle, u32, u64, u64, *r4x_api.GfxBufferMap) i32;
extern fn r4os_rust_program_domain_gfx_buffer_unmap(*const r4x_api.GfxBufferHandle) i32;
extern fn r4os_rust_program_domain_gfx_buffer_map_persistent(*const r4x_api.GfxBufferHandle, u32, u64, u64, *r4x_api.GfxBufferMap) i32;
extern fn r4os_rust_program_domain_gfx_brightness_request(*const gfx_output_api.abi.GfxBrightnessRequest, *gfx_output_api.abi.GfxBrightnessRequest) i32;
extern fn r4os_rust_program_domain_display_cursor_submit(*const @import("r4os_kernel_contract").DisplayCursorRequest, *@import("r4os_kernel_contract").DisplayCursorStatus) i32;

export fn r4os_zig_program_domain_audio_open_stream(owner: ProgramProcessHandle, rate: u32, channels: u16, format: u16) callconv(.c) i32 {
    return audio.openStreamForOwner(.{
            .instance_id = owner.instance_id,
            .generation = owner.generation,
        }, rate, channels, format);
}
export fn r4os_zig_program_domain_audio_output_info(index: u32, out: *audio.AudioOutputInfo) callconv(.c) i32 {
    return audio.audioOutputInfo(index, out);
}
export fn r4os_zig_program_domain_audio_select_output(id: [*]const u8, len: usize) callconv(.c) i32 {
    return audio.audioSelectOutput(id[0..len]);
}
export fn r4os_zig_program_domain_audio_write(owner: ProgramProcessHandle, stream_id: u32, data_ptr: [*]const u8, byte_count: u32) callconv(.c) i32 {
    return audio.writeStreamForOwner(.{
            .instance_id = owner.instance_id,
            .generation = owner.generation,
        }, stream_id, data_ptr, byte_count);
}
export fn r4os_zig_program_domain_audio_close(owner: ProgramProcessHandle, stream_id: u32) callconv(.c) i32 {
    return audio.closeStreamForOwner(.{
            .instance_id = owner.instance_id,
            .generation = owner.generation,
        }, stream_id);
}
export fn r4os_zig_program_domain_audio_set_volume(owner: ProgramProcessHandle, stream_id: u32, fixed_volume: u32) callconv(.c) i32 {
    return audio.setVolumeForOwner(.{
            .instance_id = owner.instance_id,
            .generation = owner.generation,
        }, stream_id, fixed_volume);
}
export fn r4os_zig_program_domain_remote_frame_acquire(owner: ProgramProcessHandle) callconv(.c) i32 {
    const capture: r4api.r4desk.CaptureOwner = .{ .id = owner.instance_id, .generation = owner.generation };
    return r4api.r4desk.remoteFrameAcquire(capture);
}
export fn r4os_zig_program_domain_remote_frame_release(owner: ProgramProcessHandle) callconv(.c) i32 {
    const capture: r4api.r4desk.CaptureOwner = .{ .id = owner.instance_id, .generation = owner.generation };
    return r4api.r4desk.remoteFrameRelease(capture);
}
export fn r4os_zig_program_domain_remote_frame_publish(owner: ProgramProcessHandle, info: *const r4x_api.RemoteFrameInfo, pixels: [*]const u32, count: u32) callconv(.c) i32 {
    _ = owner;
    return r4api.r4desk.remoteFramePublish(info, pixels, count);
}
export fn r4os_zig_program_domain_remote_frame_publish_regions(owner: ProgramProcessHandle, info: *const r4x_api.RemoteFrameInfo, pixels: [*]const u32, count: u32, regions: [*]const r4x_api.DisplayDamageRect, region_count: u32) callconv(.c) i32 {
    _ = owner;
    return r4api.r4desk.remoteFramePublishRegions(info, pixels, count, regions, region_count);
}
export fn r4os_zig_program_domain_remote_frame_snapshot_acquire(owner: ProgramProcessHandle, expected: u32, info: *r4x_api.RemoteFrameInfo, lease: *r4x_api.RemoteFrameLease) callconv(.c) i32 {
    const capture: r4api.r4desk.CaptureOwner = .{ .id = owner.instance_id, .generation = owner.generation };
    return r4api.r4desk.remoteFrameSnapshotAcquire(capture, expected, info, lease);
}
export fn r4os_zig_program_domain_remote_frame_snapshot_release(owner: ProgramProcessHandle, lease: *const r4x_api.RemoteFrameLease) callconv(.c) i32 {
    const capture: r4api.r4desk.CaptureOwner = .{ .id = owner.instance_id, .generation = owner.generation };
    return r4api.r4desk.remoteFrameSnapshotRelease(capture, lease);
}
export fn r4os_zig_program_domain_remote_frame_source_reset(owner: ProgramProcessHandle) callconv(.c) i32 {
    const capture: r4api.r4desk.CaptureOwner = .{ .id = owner.instance_id, .generation = owner.generation };
    return r4api.r4desk.remoteFrameSourceReset(capture);
}
export fn r4os_zig_program_domain_gfx_buffer_create(owner: ProgramProcessHandle, input: *const r4x_api.GfxBufferDescriptor, output: *r4x_api.GfxBufferReference) callconv(.c) i32 {
    return gfx_buffer_api.create(graphicsOwner(owner), input, output);
}
export fn r4os_zig_program_domain_gfx_virtual_start(owner: ProgramProcessHandle, closed: *bool, input: *const gfx_virtual_api.abi.GfxVirtualRequest, output: *gfx_virtual_api.abi.GfxVirtualStatus) callconv(.c) i32 {
    return gfx_virtual_api.start(graphicsOwner(owner), closed, input, output);
}
export fn r4os_zig_program_domain_gfx_virtual_query(owner: ProgramProcessHandle, input: *const r4x_api.GfxBufferHandle, output: *gfx_virtual_api.abi.GfxVirtualStatus) callconv(.c) i32 {
    return gfx_virtual_api.query(graphicsOwner(owner), input, output);
}
export fn r4os_zig_program_domain_gfx_virtual_close(owner: ProgramProcessHandle, input: *const r4x_api.GfxBufferHandle, mode: u32) callconv(.c) i32 {
    return gfx_virtual_api.close(graphicsOwner(owner), input, mode);
}
export fn r4os_zig_program_domain_gfx_virtual_wait(owner: ProgramProcessHandle, input: *const r4x_api.GfxBufferHandle, until: u32, timeout: u64, output: *gfx_virtual_api.abi.GfxVirtualStatus) callconv(.c) i32 {
    return gfx_virtual_api.wait(graphicsOwner(owner), input, until, timeout, output);
}
export fn r4os_zig_program_domain_gfx_native_start(owner: ProgramProcessHandle, input: *const r4x_api.GfxNativeAllocation, output: *r4x_api.GfxNativeStatus) callconv(.c) i32 {
    return gfx_allocation_api.start(graphicsOwner(owner), input, output);
}
export fn r4os_zig_program_domain_gfx_native_query(owner: ProgramProcessHandle, input: *const r4x_api.GfxBufferHandle, output: *r4x_api.GfxNativeStatus) callconv(.c) i32 {
    return gfx_allocation_api.query(graphicsOwner(owner), input, output);
}
export fn r4os_zig_program_domain_gfx_native_receive(owner: ProgramProcessHandle, input: *const r4x_api.GfxBufferHandle, output: *r4x_api.GfxBufferReference) callconv(.c) i32 {
    return gfx_allocation_api.receive(graphicsOwner(owner), input, output);
}
export fn r4os_zig_program_domain_gfx_native_close(owner: ProgramProcessHandle, input: *const r4x_api.GfxBufferHandle) callconv(.c) i32 {
    return gfx_allocation_api.close(graphicsOwner(owner), input);
}
export fn r4os_zig_program_domain_gfx_native_wait(owner: ProgramProcessHandle, input: *const r4x_api.GfxBufferHandle, timeout_ticks: u64, output: *r4x_api.GfxNativeStatus) callconv(.c) i32 {
    return gfx_allocation_api.wait(graphicsOwner(owner), input, timeout_ticks, output);
}
export fn r4os_zig_program_domain_gfx_queue_open(owner: ProgramProcessHandle, input: *const gfx_queue_api.abi.GfxQueueConfig, output: *gfx_queue_api.abi.GfxQueueHandle) callconv(.c) i32 {
    return gfx_queue_api.open(graphicsOwner(owner), input, output);
}
export fn r4os_zig_program_domain_gfx_atomic_test(owner: ProgramProcessHandle, input: *const gfx_output_api.abi.GfxAtomicState, output: *gfx_output_api.abi.GfxAtomicResult) callconv(.c) i32 {
    return gfx_output_api.atomic(graphicsOwner(owner), input, output, false);
}
export fn r4os_zig_program_domain_gfx_atomic_commit(owner: ProgramProcessHandle, input: *const gfx_output_api.abi.GfxAtomicState, output: *gfx_output_api.abi.GfxAtomicResult) callconv(.c) i32 {
    return gfx_output_api.atomic(graphicsOwner(owner), input, output, true);
}
export fn r4os_zig_program_domain_gfx_atomic_submit(owner: ProgramProcessHandle, input: *const gfx_output_api.abi.GfxAtomicState, confirmation_ms: u32, output: *gfx_output_api.abi.GfxModeStatus) callconv(.c) i32 {
    return gfx_output_api.submit(graphicsOwner(owner), input, confirmation_ms, output);
}
export fn r4os_zig_program_domain_gfx_atomic_test_color(owner: ProgramProcessHandle, input: *const gfx_output_api.abi.GfxModeColorRequest, output: *gfx_output_api.abi.GfxAtomicResult) callconv(.c) i32 {
    return gfx_output_api.testColor(graphicsOwner(owner), input, output);
}
export fn r4os_zig_program_domain_gfx_atomic_submit_color(owner: ProgramProcessHandle, input: *const gfx_output_api.abi.GfxModeColorRequest, confirmation_ms: u32, output: *gfx_output_api.abi.GfxModeStatus) callconv(.c) i32 {
    return gfx_output_api.submitColor(graphicsOwner(owner), input, confirmation_ms, output);
}
export fn r4os_zig_program_domain_gfx_refresh_request(owner: ProgramProcessHandle, input: *const gfx_output_api.abi.GfxRefreshRequest, output: *gfx_output_api.abi.GfxRefreshRequest) callconv(.c) i32 {
    return gfx_output_api.requestRefresh(graphicsOwner(owner), input, output);
}
export fn r4os_zig_program_domain_gfx_power_request(owner: ProgramProcessHandle, input: *const gfx_output_api.abi.GfxPowerRequest, output: *gfx_output_api.abi.GfxPowerRequest) callconv(.c) i32 {
    return gfx_output_api.requestPower(graphicsOwner(owner), input, output);
}
export fn r4os_zig_program_domain_gfx_atomic_resolve(owner: ProgramProcessHandle, ticket: u64, action: u32, output: *gfx_output_api.abi.GfxModeStatus) callconv(.c) i32 {
    return gfx_output_api.resolve(graphicsOwner(owner), ticket, action, output);
}
export fn r4os_zig_program_domain_gfx_queue_close(owner: ProgramProcessHandle, input: *const gfx_queue_api.abi.GfxQueueHandle) callconv(.c) i32 {
    return gfx_queue_api.close(graphicsOwner(owner), input);
}
export fn r4os_zig_program_domain_gfx_queue_submit(owner: ProgramProcessHandle, queue: *const gfx_queue_api.abi.GfxQueueHandle, input: *const gfx_queue_api.abi.GfxSubmission, output: *gfx_queue_api.abi.GfxFenceStatus) callconv(.c) i32 {
    return gfx_queue_api.submit(graphicsOwner(owner), queue, input, output);
}
export fn r4os_zig_program_domain_gfx_queue_submit_render_list(owner: ProgramProcessHandle, queue: *const gfx_queue_api.abi.GfxQueueHandle, input: *const gfx_queue_api.abi.GfxSubmission, list: *const gfx_queue_api.abi.GfxRenderList, output: *gfx_queue_api.abi.GfxFenceStatus) callconv(.c) i32 {
    return gfx_queue_api.submitRenderList(graphicsOwner(owner), queue, input, list, output);
}
export fn r4os_zig_program_domain_gfx_queue_submit_render_grid_list(owner: ProgramProcessHandle, queue: *const gfx_queue_api.abi.GfxQueueHandle, input: *const gfx_queue_api.abi.GfxSubmission, list: *const gfx_queue_api.abi.GfxRenderGridList, output: *gfx_queue_api.abi.GfxFenceStatus) callconv(.c) i32 {
    return gfx_queue_api.submitRenderGridList(graphicsOwner(owner), queue, input, list, output);
}
export fn r4os_zig_program_domain_gfx_queue_submit_output(owner: ProgramProcessHandle, queue: *const gfx_queue_api.abi.GfxQueueHandle, input: *const gfx_queue_api.abi.GfxSubmission, target: *const gfx_queue_api.abi.GfxOutputTarget, output: *gfx_queue_api.abi.GfxFenceStatus) callconv(.c) i32 {
    return gfx_queue_api.submitOutput(graphicsOwner(owner), queue, input, target, output);
}
export fn r4os_zig_program_domain_gfx_queue_submit_render_color_list(owner: ProgramProcessHandle, queue: *const gfx_queue_api.abi.GfxQueueHandle, input: *const gfx_queue_api.abi.GfxSubmission, list: *const gfx_queue_api.abi.GfxRenderColorList, output: *gfx_queue_api.abi.GfxFenceStatus) callconv(.c) i32 {
    return gfx_queue_api.submitRenderColorList(graphicsOwner(owner), queue, input, list, output);
}
export fn r4os_zig_program_domain_gfx_queue_submit_native(owner: ProgramProcessHandle, queue: *const gfx_queue_api.abi.GfxQueueHandle, input: *const gfx_queue_api.abi.GfxSubmission, native: *const gfx_queue_api.abi.GfxNativeSubmission, output: *gfx_queue_api.abi.GfxFenceStatus) callconv(.c) i32 {
    return gfx_queue_api.submitNative(graphicsOwner(owner), queue, input, native, output);
}
export fn r4os_zig_program_domain_gfx_queue_submit_render_color_grid_list(owner: ProgramProcessHandle, queue: *const gfx_queue_api.abi.GfxQueueHandle, input: *const gfx_queue_api.abi.GfxSubmission, list: *const gfx_queue_api.abi.GfxRenderColorGridList, output: *gfx_queue_api.abi.GfxFenceStatus) callconv(.c) i32 {
    return gfx_queue_api.submitRenderColorGridList(graphicsOwner(owner), queue, input, list, output);
}
export fn r4os_zig_program_domain_gfx_fence_cancel(owner: ProgramProcessHandle, input: *const gfx_queue_api.abi.GfxFence) callconv(.c) i32 {
    return gfx_queue_api.cancel(graphicsOwner(owner), input);
}
export fn r4os_zig_program_domain_gfx_fence_release(owner: ProgramProcessHandle, input: *const gfx_queue_api.abi.GfxFence) callconv(.c) i32 {
    return gfx_queue_api.release(graphicsOwner(owner), input);
}
export fn r4os_zig_program_domain_gfx_buffer_describe(owner: ProgramProcessHandle, input: *const r4x_api.GfxBufferHandle, output: *r4x_api.GfxBufferDescriptor) callconv(.c) i32 {
    return gfx_buffer_api.describe(graphicsOwner(owner), input, output);
}
export fn r4os_zig_program_domain_gfx_buffer_import(owner: ProgramProcessHandle, input: *const r4x_api.GfxBufferHandle, output: *r4x_api.GfxBufferReference) callconv(.c) i32 {
    return gfx_buffer_api.import(graphicsOwner(owner), input, output);
}
export fn r4os_zig_program_domain_gfx_buffer_release(owner: ProgramProcessHandle, input: *const r4x_api.GfxBufferHandle) callconv(.c) i32 {
    return gfx_buffer_api.release(graphicsOwner(owner), input);
}
export fn r4os_zig_program_domain_gfx_buffer_map(owner: ProgramProcessHandle, input: *const r4x_api.GfxBufferHandle, access: u32, offset: u64, bytes: u64, output: *r4x_api.GfxBufferMap) callconv(.c) i32 {
    return gfx_buffer_api.map(graphicsOwner(owner), input, access, offset, bytes, output);
}
export fn r4os_zig_program_domain_gfx_buffer_unmap(owner: ProgramProcessHandle, input: *const r4x_api.GfxBufferHandle) callconv(.c) i32 {
    return gfx_buffer_api.unmap(graphicsOwner(owner), input);
}
export fn r4os_zig_program_domain_gfx_buffer_map_persistent(owner: ProgramProcessHandle, input: *const r4x_api.GfxBufferHandle, access: u32, offset: u64, bytes: u64, output: *r4x_api.GfxBufferMap) callconv(.c) i32 {
    return gfx_buffer_api.mapPersistent(graphicsOwner(owner), input, access, offset, bytes, output);
}
export fn r4os_zig_program_domain_gfx_brightness_request(owner: ProgramProcessHandle, input: *const gfx_output_api.abi.GfxBrightnessRequest, output: *gfx_output_api.abi.GfxBrightnessRequest) callconv(.c) i32 {
    return gfx_output_api.requestBrightness(graphicsOwner(owner), input, output);
}
export fn r4os_zig_program_domain_display_cursor_submit(owner: ProgramProcessHandle, input: *const @import("r4os_kernel_contract").DisplayCursorRequest, output: *@import("r4os_kernel_contract").DisplayCursorStatus) callconv(.c) i32 {
    return @import("../display/cursor_work.zig").submit(graphicsOwner(owner), input, output);
}
export fn r4os_zig_program_domain_remote_publisher(owner: ProgramProcessHandle) callconv(.c) bool { return r4api.r4desk.remoteFramePublisher(.{ .id = owner.instance_id, .generation = owner.generation }); }

extern fn r4os_rust_program_service_named(u8,[*:0]const u8,?*ServiceInfo,u32,u64) i32;
extern fn r4os_rust_program_service_endpoint(u8,u32,u64) i32;
extern fn r4os_rust_program_service_info(u32,*ServiceInfo) i32;
extern fn r4os_rust_program_service_detail(u32,*ServiceDetail) i32;
extern fn r4os_rust_program_service_detail_name([*:0]const u8,*ServiceDetail) i32;
extern fn r4os_rust_program_service_register([*:0]const u8,u32,*ServiceInfo) i32;
extern fn r4os_rust_program_service_recv(u32,*ServiceMessageHeader,[*]u8,u32) i32;
extern fn r4os_rust_program_service_reply(u32,u32,i32,[*]const u8,u32) i32;
extern fn r4os_rust_program_service_install([*:0]const u8,[*:0]const u8,[*:0]const u8,u32,[*:0]const u8,*ServiceInfo) i32;
extern fn r4os_rust_program_service_lifecycle(bool,NativeProgramBytes,*ServiceInfo,u64) i32;
const NativeServiceEntry = extern struct {id:u32,state:u8,mode:u8,reserved:u16=0,start_generation:u64,name_len:usize,path_len:usize,args_len:usize,name:[32]u8,path:[1024]u8,args:[96]u8};
const NativeServiceTarget = extern struct {registry_generation:u64,slot:usize,name_len:usize,index:u32,id:u32,state:u32,reserved:u32=0,name:[32]u8};
const NativeServiceReservation = extern struct {slot:usize,generation:u64};
fn serviceEntryValue(entry:services.Entry) NativeServiceEntry {return .{.id=entry.instance_id,.state=@intFromEnum(entry.state),.mode=@intFromEnum(entry.start_mode),.start_generation=entry.start_generation,.name_len=entry.name_len,.path_len=entry.path_len,.args_len=entry.args_len,.name=entry.name,.path=entry.path,.args=entry.args};}
fn serviceTargetValue(target:services.ApiIndexTarget) NativeServiceTarget{return .{.registry_generation=target.registry_generation,.slot=target.service_slot,.name_len=target.name_len,.index=target.api_index,.id=target.instance_id,.state=@intFromEnum(target.state),.name=target.name};}
fn serviceTargetNative(target:*const NativeServiceTarget) services.ApiIndexTarget{return .{.registry_generation=target.registry_generation,.api_index=target.index,.service_slot=target.slot,.instance_id=target.id,.state=@enumFromInt(@as(u8,@intCast(target.state))),.name=target.name,.name_len=target.name_len};}
fn serviceReservationNative(identity:*const NativeServiceReservation) services.StartReservation {return .{.slot=identity.slot,.generation=identity.generation};}
export fn r4os_zig_program_service_entry(name:NativeProgramBytes,out:*NativeServiceEntry) callconv(.c) bool {const entry=services.entryByName(name.data[0..name.len]) orelse return false;out.*=serviceEntryValue(entry);return true;}
export fn r4os_zig_program_service_index(retry:bool,index:u32,out:*NativeServiceTarget) callconv(.c) bool {const value=(if(retry) services.retryApiIndexRefresh(index) else services.beginApiIndexRefresh(index)) orelse return false;out.*=serviceTargetValue(value);return true;}
export fn r4os_zig_program_service_target_entry(target:*const NativeServiceTarget,out:*NativeServiceEntry) callconv(.c) bool {const entry=services.entryForApiIndexTarget(serviceTargetNative(target)) orelse return false;out.*=serviceEntryValue(entry);return true;}
export fn r4os_zig_program_service_target_note(target:*const NativeServiceTarget) callconv(.c) bool {return services.noteApiIndexInstanceLookup(serviceTargetNative(target));}
export fn r4os_zig_program_service_target_mutate(op:u8,target:*const NativeServiceTarget,code:i32,text:NativeProgramBytes) callconv(.c) i32 {const value=serviceTargetNative(target);return switch(op){0=>services.markStoppingTarget(value),1=>services.markStoppedTarget(value,code),2=>services.markFailedTarget(value,code,text.data[0..text.len]),else=>unreachable};}
export fn r4os_zig_program_service_name_mutate(op:u8,name:NativeProgramBytes,code:i32,text:NativeProgramBytes) callconv(.c) i32 {const value=name.data[0..name.len];return switch(op){0=>services.markStopping(value),1=>services.markStopped(value,code),2=>services.markFailed(value,code,text.data[0..text.len]),3=>services.bumpRestartCount(value),4=>services.unregister(value),else=>unreachable};}
export fn r4os_zig_program_service_info(op:u8,name:NativeProgramBytes,target:?*const NativeServiceTarget,out:*ServiceInfo,ticks:u64) callconv(.c) i32 {return switch(op){0=>services.apiStatus(name.data[0..name.len],out,ticks),1=>services.apiOpen(name.data[0..name.len],out,ticks),2=>services.apiInfoForIndexTarget(serviceTargetNative(target.?),out,ticks),else=>unreachable};}
export fn r4os_zig_program_service_detail(by_name:bool,name:NativeProgramBytes,target:?*const NativeServiceTarget,out:*ServiceDetail,ticks:u64) callconv(.c) i32 {return if(by_name) services.apiDetailByName(name.data[0..name.len],out,ticks) else services.apiDetailForIndexTarget(serviceTargetNative(target.?),out,ticks);}
export fn r4os_zig_program_service_begin(name:NativeProgramBytes,identity:*NativeServiceReservation,out:*NativeServiceEntry) callconv(.c) i32 {var reservation:services.StartReservation=.{};const result=services.beginStart(name.data[0..name.len],&reservation);if(result==services.API_OK){identity.*=.{.slot=reservation.slot,.generation=reservation.generation};out.*=serviceEntryValue(reservation.entry);}return result;}
export fn r4os_zig_program_service_complete(identity:*const NativeServiceReservation,id:u32,ticks:u64) callconv(.c) bool {const reservation=serviceReservationNative(identity);return services.completeStart(&reservation,id,ticks);}
export fn r4os_zig_program_service_fail(identity:*const NativeServiceReservation,code:i32,text:NativeProgramBytes) callconv(.c) void {const reservation=serviceReservationNative(identity);services.failStart(&reservation,code,text.data[0..text.len]);}
export fn r4os_zig_program_service_cancel(name:NativeProgramBytes,generation:u64) callconv(.c) void {services.cancelStarting(name.data[0..name.len],generation);}
export fn r4os_zig_program_service_mode(name:NativeProgramBytes,mode:u8) callconv(.c) i32 {return services.setStartMode(name.data[0..name.len],@enumFromInt(mode));}
export fn r4os_zig_program_service_install(name:NativeProgramBytes,path:NativeProgramBytes,args:NativeProgramBytes,mode:u8,description:NativeProgramBytes) callconv(.c) i32 {return services.registerWithDescription(name.data[0..name.len],path.data[0..path.len],args.data[0..args.len],@enumFromInt(mode),description.data[0..description.len]);}
export fn r4os_zig_program_service_endpoint_register(name:NativeProgramBytes,id:u32,flags:u32,out:*ServiceInfo,ticks:u64) callconv(.c) i32 {return services.registerEndpoint(name.data[0..name.len],id,flags,out,ticks);}
export fn r4os_zig_program_service_endpoint_control(op:u8,handle:u32,timeout:u64) callconv(.c) i32 {return switch(op){0=>services.apiClose(handle),1=>services.unregisterEndpoint(handle),2=>services.endpointPoll(handle),3=>services.endpointWait(handle,timeout),else=>unreachable};}
export fn r4os_zig_program_service_endpoint_recv(handle:u32,header:*ServiceMessageHeader,out:[*]u8,len:usize) callconv(.c) i32 {return services.recvRequest(handle,header,out[0..len]);}
export fn r4os_zig_program_service_endpoint_reply(handle:u32,id:u32,status:i32,payload:NativeProgramBytes) callconv(.c) i32 {return services.reply(handle,id,status,payload.data[0..payload.len]);}
export fn r4os_zig_program_service_boot(op:u8,name:NativeProgramBytes,stage:NativeProgramBytes) callconv(.c) void {switch(op){0=>bootscreen.setService(name.data[0..name.len]),1=>bootscreen.setServiceStage(name.data[0..name.len],stage.data[0..stage.len]),2=>bootscreen.setDetail("Dienstplan fertig"),3=>bootscreen.setServiceError(name.data[0..name.len]),else=>unreachable}}
comptime {
if(@sizeOf(NativeServiceEntry)!=1192) @compileError("service projection size");
if(@offsetOf(NativeServiceEntry,"id")!=0) @compileError("service projection field");
if(@offsetOf(NativeServiceEntry,"state")!=4) @compileError("service projection field");
if(@offsetOf(NativeServiceEntry,"mode")!=5) @compileError("service projection field");
if(@offsetOf(NativeServiceEntry,"reserved")!=6) @compileError("service projection field");
if(@offsetOf(NativeServiceEntry,"start_generation")!=8) @compileError("service projection field");
if(@offsetOf(NativeServiceEntry,"name_len")!=16) @compileError("service projection field");
if(@offsetOf(NativeServiceEntry,"path_len")!=24) @compileError("service projection field");
if(@offsetOf(NativeServiceEntry,"args_len")!=32) @compileError("service projection field");
if(@offsetOf(NativeServiceEntry,"name")!=40) @compileError("service projection field");
if(@offsetOf(NativeServiceEntry,"path")!=72) @compileError("service projection field");
if(@offsetOf(NativeServiceEntry,"args")!=1096) @compileError("service projection field");
if(@sizeOf(NativeServiceTarget)!=72) @compileError("service projection size");
if(@offsetOf(NativeServiceTarget,"registry_generation")!=0) @compileError("service projection field");
if(@offsetOf(NativeServiceTarget,"slot")!=8) @compileError("service projection field");
if(@offsetOf(NativeServiceTarget,"name_len")!=16) @compileError("service projection field");
if(@offsetOf(NativeServiceTarget,"index")!=24) @compileError("service projection field");
if(@offsetOf(NativeServiceTarget,"id")!=28) @compileError("service projection field");
if(@offsetOf(NativeServiceTarget,"state")!=32) @compileError("service projection field");
if(@offsetOf(NativeServiceTarget,"reserved")!=36) @compileError("service projection field");
if(@offsetOf(NativeServiceTarget,"name")!=40) @compileError("service projection field");
if(@sizeOf(NativeServiceReservation)!=16) @compileError("service projection size");
if(@offsetOf(NativeServiceReservation,"slot")!=0) @compileError("service projection field");
if(@offsetOf(NativeServiceReservation,"generation")!=8) @compileError("service projection field");
if(@intFromEnum(services.State.empty)!=0) @compileError("service state");
if(@intFromEnum(services.State.stopped)!=1) @compileError("service state");
if(@intFromEnum(services.State.starting)!=2) @compileError("service state");
if(@intFromEnum(services.State.running)!=3) @compileError("service state");
if(@intFromEnum(services.State.stopping)!=4) @compileError("service state");
if(@intFromEnum(services.State.failed)!=5) @compileError("service state");
if(@intFromEnum(services.State.disabled)!=6) @compileError("service state");
if(@intFromEnum(services.StartMode.manual)!=0) @compileError("service mode");
if(@intFromEnum(services.StartMode.auto)!=1) @compileError("service mode");
if(@intFromEnum(services.StartMode.disabled)!=2) @compileError("service mode");
if(services.MAX_NAME!=32 or services.MAX_PATH!=1024 or services.MAX_ARGS!=96 or services.MAX_DESCRIPTION!=80 or services.API_MAX_PAYLOAD!=4096) @compileError("service bounds");
}

extern fn r4os_rust_program_diagnostic_storage(*r4api.r4dev.ProgramInstanceStorageSummary) void;
extern fn r4os_rust_program_diagnostic_registry(*r4api.r4dev.ProgramRegistrySummaryV2) void;
extern fn r4os_rust_program_diagnostic_input(*r4api.r4dev.ProgramInputPerformanceInfo) void;
extern fn r4os_rust_program_diagnostic_storage_test(*r4api.r4dev.ProgramInstanceStorageSelfTestResult) i32;
extern fn r4os_rust_program_diagnostic_registry_test(*r4api.r4dev.ProgramRegistrySelfTestResultV2) i32;
extern fn r4os_rust_program_storage_self_test() bool;
const NativeProgramTestBufferStats=extern struct{objects:u64,references:u64,leases:u64,bytes:u64,retained_bytes:u64,system_bytes:u64,device_bytes:u64,system_backed_bytes:u64,device_backed_bytes:u64,system_pinned_bytes:u64,device_pinned_bytes:u64,scanout_pinned_bytes:u64,device_mapped_bytes:u64,allocating_bytes:u64,destroying_bytes:u64};
export fn r4os_zig_program_test_buffer_stats(out:*NativeProgramTestBufferStats) callconv(.c) void {gfx_buffers.lock();defer gfx_buffers.unlock();const value=gfx_buffers.store.stats();out.*=.{.objects=value.objects,.references=value.references,.leases=value.leases,.bytes=value.bytes,.retained_bytes=value.retained_bytes,.system_bytes=value.system_bytes,.device_bytes=value.device_bytes,.system_backed_bytes=value.system_backed_bytes,.device_backed_bytes=value.device_backed_bytes,.system_pinned_bytes=value.system_pinned_bytes,.device_pinned_bytes=value.device_pinned_bytes,.scanout_pinned_bytes=value.scanout_pinned_bytes,.device_mapped_bytes=value.device_mapped_bytes,.allocating_bytes=value.allocating_bytes,.destroying_bytes=value.destroying_bytes};}
export fn r4os_zig_program_test_buffer_collect() callconv(.c) void {gfx_buffers.collect();}
comptime{
if(@sizeOf(NativeProgramTestBufferStats)!=120) @compileError("program test buffers");
if(@offsetOf(NativeProgramTestBufferStats,"objects")!=0) @compileError("program test buffer field");
if(@offsetOf(NativeProgramTestBufferStats,"references")!=8) @compileError("program test buffer field");
if(@offsetOf(NativeProgramTestBufferStats,"leases")!=16) @compileError("program test buffer field");
if(@offsetOf(NativeProgramTestBufferStats,"bytes")!=24) @compileError("program test buffer field");
if(@offsetOf(NativeProgramTestBufferStats,"retained_bytes")!=32) @compileError("program test buffer field");
if(@offsetOf(NativeProgramTestBufferStats,"system_bytes")!=40) @compileError("program test buffer field");
if(@offsetOf(NativeProgramTestBufferStats,"device_bytes")!=48) @compileError("program test buffer field");
if(@offsetOf(NativeProgramTestBufferStats,"system_backed_bytes")!=56) @compileError("program test buffer field");
if(@offsetOf(NativeProgramTestBufferStats,"device_backed_bytes")!=64) @compileError("program test buffer field");
if(@offsetOf(NativeProgramTestBufferStats,"system_pinned_bytes")!=72) @compileError("program test buffer field");
if(@offsetOf(NativeProgramTestBufferStats,"device_pinned_bytes")!=80) @compileError("program test buffer field");
if(@offsetOf(NativeProgramTestBufferStats,"scanout_pinned_bytes")!=88) @compileError("program test buffer field");
if(@offsetOf(NativeProgramTestBufferStats,"device_mapped_bytes")!=96) @compileError("program test buffer field");
if(@offsetOf(NativeProgramTestBufferStats,"allocating_bytes")!=104) @compileError("program test buffer field");
if(@offsetOf(NativeProgramTestBufferStats,"destroying_bytes")!=112) @compileError("program test buffer field");
if(@intFromEnum(owner_locks.Class.physical_memory)!=7 or @intFromEnum(mem_virt.Window.graphics)!=8 or THREAD_FLAG_MAIN!=1 or GUI_RASTER_MAX_PIXELS!=16384 or GUI_RESOURCE_BLOCK_TARGET_BYTES!=65536 or PROGRAM_STACK_RESERVE_SIZE!=4194304 or PROGRAM_STACK_INITIAL_COMMIT_SIZE!=65536) @compileError("program selftest constants");
}

extern fn r4os_rust_program_preemption_acceptance(u32,u64) bool;
extern fn r4os_rust_program_parallel_acceptance() bool;
comptime { if(@intFromEnum(modules.Kind.r4l)!=2 or timer.DEFAULT_HZ!=1000 or percpu.max_cpus!=32) @compileError("program acceptance constants"); }

extern fn r4os_rust_program_history_append(ProgramProcessHandle, *const ProgramInstance, i32, u8, u64, u64) u64;
extern fn r4os_rust_program_completion_copy(*const ProgramCompletionNode, *ProgramProcessCompletion) void;
extern fn r4os_rust_program_completion_access(u8, ProgramProcessHandle, ?*ProgramProcessCompletion) i32;


comptime {
    if (@intFromEnum(ProgramCompletionState.ready) != 2 or @intFromEnum(ProgramCompletionState.consumed) != 3 or
        PROGRAM_COMPLETION_FLAG_OWNER != 8 or PROGRAM_HANDLE_OK != 0 or PROGRAM_HANDLE_ERROR_WOULD_BLOCK != -5 or
        @sizeOf(ProgramProcessCompletion) != 128 or @offsetOf(ProgramProcessCompletion, "console_state") != 64)
        @compileError("Native completion ordinal/public record contract changed");
}


const NativeProgramFailureSnapshot = extern struct {
    phase: u32, armed: bool, consumed: bool, reaper_signalled: bool, retried: bool, recovered: bool,
};
extern fn r4os_rust_program_failure_control(u8, u8) bool;
extern fn r4os_rust_program_failure_snapshot(*NativeProgramFailureSnapshot) void;
extern fn r4os_rust_program_selftest_allowed(u8) bool;
extern fn r4os_rust_program_retire_defer(ProgramProcessHandle) u8;
extern fn r4os_rust_program_retire_advance(*ProgramRegistrySlot, ProgramProcessHandle, *ProgramCompletionNode, u8, u8) bool;
export fn r4os_zig_program_selftest_option(group: u8) callconv(.c) NativeProgramBytes {
    const value = boot_config.optionValue(boot_config.get(), if (group == 1) "PROGRAMREGISTRY" else "PROGRAMLIFECYCLE", "selftest") orelse "";
    return .{ .data = value.ptr, .len = value.len };
}
comptime {
    if (@sizeOf(NativeProgramFailureSnapshot) != 12 or @offsetOf(NativeProgramFailureSnapshot, "recovered") != 8 or
        @sizeOf(NativeProgramBytes) != 16 or @offsetOf(NativeProgramBytes, "len") != 8 or
        sync.LockRank.program_instances != 100 or @intFromEnum(ProgramRetireResult.deferred) != 2)
        @compileError("Native lifecycle failure/phase view contract changed");
    if (@intFromEnum(ProgramLifecycleFailurePhase.none) != 0) @compileError("Native lifecycle phase ordinal changed");
    if (@intFromEnum(ProgramLifecycleFailurePhase.completion_reserve) != 1) @compileError("Native lifecycle phase ordinal changed");
    if (@intFromEnum(ProgramLifecycleFailurePhase.storage) != 2) @compileError("Native lifecycle phase ordinal changed");
    if (@intFromEnum(ProgramLifecycleFailurePhase.image) != 3) @compileError("Native lifecycle phase ordinal changed");
    if (@intFromEnum(ProgramLifecycleFailurePhase.stack) != 4) @compileError("Native lifecycle phase ordinal changed");
    if (@intFromEnum(ProgramLifecycleFailurePhase.task) != 5) @compileError("Native lifecycle phase ordinal changed");
    if (@intFromEnum(ProgramLifecycleFailurePhase.publish) != 6) @compileError("Native lifecycle phase ordinal changed");
    if (@intFromEnum(ProgramLifecycleFailurePhase.exit_commit) != 7) @compileError("Native lifecycle phase ordinal changed");
    if (@intFromEnum(ProgramLifecycleFailurePhase.cancel_execution) != 8) @compileError("Native lifecycle phase ordinal changed");
    if (@intFromEnum(ProgramLifecycleFailurePhase.detach_task) != 9) @compileError("Native lifecycle phase ordinal changed");
    if (@intFromEnum(ProgramLifecycleFailurePhase.output_detach) != 10) @compileError("Native lifecycle phase ordinal changed");
    if (@intFromEnum(ProgramLifecycleFailurePhase.storage_release) != 11) @compileError("Native lifecycle phase ordinal changed");
    if (@intFromEnum(ProgramLifecycleFailurePhase.image_stack_vm_release) != 12) @compileError("Native lifecycle phase ordinal changed");
    if (@intFromEnum(ProgramLifecycleFailurePhase.slot_reclaim) != 13) @compileError("Native lifecycle phase ordinal changed");
    if (@intFromEnum(ProgramRetirePhase.cancel_execution) != 0) @compileError("Native retire phase ordinal changed");
    if (@intFromEnum(ProgramRetirePhase.detach_task) != 1) @compileError("Native retire phase ordinal changed");
    if (@intFromEnum(ProgramRetirePhase.output_detach) != 2) @compileError("Native retire phase ordinal changed");
    if (@intFromEnum(ProgramRetirePhase.storage_release) != 3) @compileError("Native retire phase ordinal changed");
    if (@intFromEnum(ProgramRetirePhase.image_stack_vm_release) != 4) @compileError("Native retire phase ordinal changed");
    if (@intFromEnum(ProgramRetirePhase.slot_reclaim) != 5) @compileError("Native retire phase ordinal changed");
}


extern fn r4os_rust_program_reap_slot(*ProgramRegistrySlot) u8;
extern fn r4os_rust_program_reaper_once() bool;
extern fn r4os_rust_program_reaper_main() noreturn;
export fn r4os_zig_program_reap_report(enabled: bool, stage: u8) callconv(.c) void {
    reportBootForegroundRetireStage(enabled, switch (stage) {
        0 => "SERVMAN: Abbruchpruefung", 1 => "SERVMAN: Taskabbau", 2 => "SERVMAN: I/O-Abbau",
        3 => "SERVMAN: Ausgabeabbau", 4 => "SERVMAN: Kontextabbau", 5 => "SERVMAN: Imageabbau",
        6 => "SERVMAN: Abschluss", 7 => "SERVMAN: Bereit", else => unreachable,
    });
}

export fn r4os_zig_program_reap_action(operation: u8, handle: ProgramProcessHandle, slot: *ProgramRegistrySlot, completion: *ProgramCompletionNode, report: bool) callconv(.c) bool {
    _ = report;
    const instance = &slot.instance;
    return switch (operation) {







        7 => r4api.r4sys.releaseStreamSlotsForProgram(handle.instance_id, handle.generation),
        8 => @import("../storage/operations.zig").releaseOwner(handle.instance_id, handle.generation, 0, 0),
        9 => audio.closeStreamsForOwner(.{ .instance_id = handle.instance_id, .generation = handle.generation }),
        10 => detachProgramCompletionOutput(instance, completion),
        11 => releaseProgramInstanceStorage(instance, completion),



        else => unreachable,
    };
}
comptime {
    if (@intFromEnum(InstanceRole.foreground) != 0 or @intFromEnum(ProgramRetireResult.idle) != 0 or
        @intFromEnum(ProgramRetireResult.completed) != 1 or @intFromEnum(ProgramRetireResult.deferred) != 2 or
        PROGRAM_COMPLETION_FLAG_READY != 1) @compileError("Native program retirement ordinal contract changed");
}

comptime {
    if (@intFromEnum(AsyncIoPendingCloseResult.idle) != 0 or @intFromEnum(AsyncIoPendingCloseResult.completed) != 1 or
        @intFromEnum(AsyncIoPendingCloseResult.deferred) != 2) @compileError("Native async close result ordinal changed");
}


const NativeProgramInstanceSnapshot = extern struct { id: u32, task_id: u32, app_class: u8, state: u8, close_requested: bool, exit_code: i32 };
extern fn r4os_rust_program_info_shell() u32;
extern fn r4os_rust_program_info_snapshot(u32, *NativeProgramInstanceSnapshot) bool;
extern fn r4os_rust_program_info_by_id(u32) ?*ProgramInstance;
extern fn r4os_rust_program_info_count() u8;
extern fn r4os_rust_program_info_state(*const ProgramInstance) u8;
extern fn r4os_rust_program_info_instance(*const ProgramInstance, *ProgramInstanceInfo) void;
extern fn r4os_rust_program_info_completion(*const ProgramCompletionNode, *ProgramInstanceInfo) void;
extern fn r4os_rust_program_info_handle(*const ProgramProcessHandle, *ProgramInstanceInfo) i32;
extern fn r4os_rust_program_info_status(*ProgramStatus) void;
extern fn r4os_rust_program_info_index(u32, *ProgramInstanceInfo) i32;
extern fn r4os_rust_program_info_vm(*const ProgramInstance, bool, *mem_virt.OwnerStats) void;
extern fn r4os_rust_program_inventory_sort(u8, [*]ProgramInventorySelection, usize, usize) void;
extern fn r4os_rust_program_inventory_consider([*]ProgramInventorySelection, usize, *usize, u64, ProgramInstanceSnapshot) void;
extern fn r4os_rust_program_inventory_could_enter([*]const ProgramInventorySelection, usize, usize, u64) bool;
extern fn r4os_rust_program_inventory_scan(u64, [*]ProgramInventorySelection, usize, *ProgramInventoryScan) void;
extern fn r4os_rust_program_inventory_registry(*ProgramInventoryRegistrySnapshot) bool;
extern fn r4os_rust_program_inventory_epoch(*u64) bool;
extern fn r4os_rust_program_inventory_cursor(*const ProgramInventoryCursor) bool;
extern fn r4os_rust_program_inventory_init_page(*const ProgramInventoryCursor, *ProgramInventoryPageInfo, u32) void;
extern fn r4os_rust_program_inventory_programs(*ProgramInventoryCursor, [*]ProgramInstanceSnapshot, u32, *ProgramInventoryPageInfo) i32;
export fn r4os_zig_program_info_terminal(instance: *const ProgramInstance) callconv(.c) bool {
    return if (instance.console_payload) |console| console.host == .terminal_mode else false;
}
export fn r4os_zig_program_info_window(instance: *const ProgramInstance) callconv(.c) i32 {
    return if (instance.gui_payload) |gui| @atomicLoad(i32, &gui.window_id, .acquire) else -1;
}
comptime {
    if (@sizeOf(NativeProgramInstanceSnapshot) != 16 or @offsetOf(NativeProgramInstanceSnapshot, "exit_code") != 12 or
        @sizeOf(ProgramInventoryCounts) != 20 or @sizeOf(ProgramInventoryScan) != 28 or @offsetOf(ProgramInventoryScan, "has_more") != 24 or
        @sizeOf(ProgramInventoryRegistrySnapshot) != 64 or @offsetOf(ProgramInventoryRegistrySnapshot, "create_failures") != 40 or
        @sizeOf(ProgramInventorySelection) != 152 or @offsetOf(ProgramInventorySelection, "snapshot") != 8 or
        @sizeOf(ProgramInstanceSnapshot) != 144 or @sizeOf(ProgramInstanceInfo) != 112 or
        @intFromEnum(mem_blocks.Owner.r4x_instance) != 3 or @intFromEnum(mem_virt.Window.r4x_vm) != 6 or @intFromEnum(mem_blocks.Kind.virtual_range) != 4 or
        @intFromEnum(InstanceState.done) != 2 or ProgramInstanceFlag.close_requested != 1 or ProgramInstanceFlag.desktop_requested != 2 or ProgramInstanceFlag.terminal_mode != 4)
        @compileError("Native program information/inventory view changed");
}

const NativeProgramConsoleLink = extern struct { parent: ProgramProcessHandle, hosted: bool };
extern fn r4os_rust_program_process_history(u32, *i32) bool;
extern fn r4os_rust_program_process_release(u32) void;
extern fn r4os_rust_program_process_finish(u32) i32;
extern fn r4os_rust_program_process_descends(*const ProgramRegistrySlot, ProgramProcessHandle) bool;
extern fn r4os_rust_program_process_clients(u8, ProgramProcessHandle) void;
extern fn r4os_rust_program_process_signal() void;
extern fn r4os_rust_program_process_pressure() void;
extern fn r4os_rust_program_process_hosted(*const ProgramInstance) bool;
extern fn r4os_rust_program_process_open(u32, *ProgramProcessHandle) i32;
extern fn r4os_rust_program_process_close(*const ProgramProcessHandle) i32;
extern fn r4os_rust_program_process_kill(*const ProgramProcessHandle) i32;
extern fn r4os_rust_program_process_wait(*const ProgramProcessHandle, u64, *ProgramProcessCompletion) i32;
extern fn r4os_rust_program_process_reap(*const ProgramProcessHandle, *ProgramProcessCompletion) i32;
extern fn r4os_rust_program_process_legacy(u8, u32) i32;
export fn r4os_zig_program_process_console_link(instance: *const ProgramInstance) callconv(.c) NativeProgramConsoleLink {
    const c = consolePayloadConst(instance);
    return .{ .parent = .{ .instance_id = c.io_target_id, .generation = c.io_target_generation }, .hosted = c.host != .none };
}
export fn r4os_zig_program_process_input(handle: ProgramProcessHandle) callconv(.c) void { signalConsoleInputForHandle(handle); }
export fn r4os_zig_program_process_desktop_signal() callconv(.c) void { desktop_events.signal(); }

comptime {
    if (@sizeOf(NativeProgramConsoleLink) != 24 or @offsetOf(NativeProgramConsoleLink, "hosted") != 16 or
        @intFromEnum(InstanceRole.background) != 2 or @intFromEnum(AppClass.console) != 0 or PROGRAM_HANDLE_ERROR_INVALID != -1 or
        PROGRAM_HANDLE_ERROR_NOT_RUNNING != -4 or PROGRAM_HANDLE_ERROR_WOULD_BLOCK != -5 or
        PROGRAM_HANDLE_ERROR_TIMEOUT != -6 or PROGRAM_HANDLE_ERROR_SELF != -7 or PROGRAM_EXIT_REASON_KILLED != 2)
        @compileError("Native process lifetime contract changed");
}

const NativeProgramExitState = extern struct {
    foreground_generation: u64, shell_generation: u64,
    foreground_id: u32, shell_id: u32, last_exit_code: i32,
    foreground_present: bool, shell_present: bool, last_display_used: bool,
};
extern fn r4os_rust_program_exit_state() *NativeProgramExitState;
extern fn r4os_rust_program_exit_commit(ProgramProcessHandle, i32, u8) u8;
extern fn r4os_rust_program_exit_begin(ProgramProcessHandle, i32, u8) bool;
extern fn r4os_rust_program_exit_mark(ProgramProcessHandle, i32) void;
fn exitState() *NativeProgramExitState { return r4os_rust_program_exit_state(); }
fn foregroundInstanceId() ?u32 { const s = exitState(); return if (s.foreground_present) s.foreground_id else null; }
fn shellInstanceId() ?u32 { const s = exitState(); return if (s.shell_present) s.shell_id else null; }
fn setForegroundInstanceId(id: ?u32) void { const s = exitState(); s.foreground_present = id != null; s.foreground_id = id orelse 0; }
fn setShellInstanceId(id: ?u32) void { const s = exitState(); s.shell_present = id != null; s.shell_id = id orelse 0; }
export fn r4os_zig_program_exit_stop_admission(instance: *ProgramInstance) callconv(.c) void {
    @import("../kernel/gfx_virtual.zig").stopAdmission(&instance.gfx_virtual_closed);
}
export fn r4os_zig_program_exit_console_snapshot(instance: *const ProgramInstance, node: *ProgramCompletionNode) callconv(.c) void {
    if (instance.console_payload) |console| {
        node.console_state = console.state; node.output_revision = console.revision;
        node.output_length = if (console.transcript_payload) |transcript| transcript.output_len else 0;
    }
}
export fn r4os_zig_program_exit_console_close(console: *ProgramConsolePayload) callconv(.c) void { _ = console.input_wait.close(.cancelled); }
export fn r4os_zig_program_exit_notify(operation: u8, handle: ProgramProcessHandle) callconv(.c) void {
    switch (operation) {
        0 => boot_perf.failShellExited(handle.instance_id),
        1 => gfx_queue.stopped(graphicsOwner(handle)),
        2 => gfx_allocations.stopped(graphicsOwner(handle)),
        3 => @import("../kernel/gfx_virtual.zig").stopped(graphicsOwner(handle)),
        4 => terminateProgramThreadsForHandle(handle, -9, scheduler.currentId()),
        5 => killConsoleClients(handle),
        else => unreachable,
    }
}
comptime {
    if (@sizeOf(NativeProgramExitState) != 32 or @offsetOf(NativeProgramExitState, "last_exit_code") != 24 or
        @offsetOf(NativeProgramExitState, "last_display_used") != 30 or
        @intFromEnum(ProgramExitCommitResult.committed) != 0 or @intFromEnum(ProgramExitCommitResult.already_exiting) != 1 or
        @intFromEnum(ProgramExitCommitResult.retry) != 2 or PROGRAM_EXIT_REASON_NATURAL != 0 or PROGRAM_EXIT_REASON_CLOSE != 1 or
        PROGRAM_COMPLETION_FLAG_DISPLAY_USED != 4) @compileError("Native pinned program exit contract changed");
}

const NativeProgramRegistryState = extern struct {
    head: ?*ProgramRegistryChunk,
    tail: ?*ProgramRegistryChunk,
    stats: *ProgramRegistryStats,
    mutex: *sync.Mutex,
    event: *sync.Event,
    history: *[EXIT_HISTORY_MAX]ExitRecord,
    epoch: u64,
    next_chunk_serial: u64,
    next_generation: u64,
    history_head: usize,
    history_count: usize,
    next_exit_sequence: u64,
    last_exit_sequence: u64,
    completion_head: ?*ProgramCompletionNode,
    completion_tail: ?*ProgramCompletionNode,
    retire_head: ?*ProgramRegistrySlot,
    retire_tail: ?*ProgramRegistrySlot,
    failure_cursor: u32,
    next_instance_id: u32,
    failure_after: u32,
    failure_after_present: bool,
    next_growth_failure: bool,
    generation_exhausted: bool,
    reaper_started: bool,
};
const NativeProgramCompletionRequest = extern struct {
    reservation: *const ProgramInstanceReservation,
    owner: bool,
    override_present: bool,
    owner_handle: ProgramProcessHandle,
    legacy_id: bool,
    retain_output: bool,
};
extern fn r4os_rust_program_registry_state() *NativeProgramRegistryState;
fn registryState() *NativeProgramRegistryState { return r4os_rust_program_registry_state(); }
fn registryPointer(comptime T: type, value: ?*anyopaque) ?*T { return if (value) |p| @ptrCast(@alignCast(p)) else null; }
extern fn r4os_rust_program_registry_state_test(u8, u8) bool;
extern fn r4os_rust_program_handle_test(u8, ProgramProcessHandle, ProgramProcessHandle) bool;
extern fn r4os_rust_program_registry_lookup(u8, u32, ProgramProcessHandle, ?*anyopaque, bool) ?*anyopaque;
extern fn r4os_rust_program_registry_predicate(u8, u32, ?*const ProgramRegistryChunk) bool;
extern fn r4os_rust_program_registry_handle(u8, ?*const anyopaque, u32, *ProgramProcessHandle) bool;
extern fn r4os_rust_program_registry_control(u8, u64) u64;
extern fn r4os_rust_program_registry_candidate(*?*ProgramRegistryChunk) u8;
extern fn r4os_rust_program_registry_grow() u8;
extern fn r4os_rust_program_registry_chunk(u8, *ProgramRegistryChunk) bool;
extern fn r4os_rust_program_registry_shrink(bool) void;
extern fn r4os_rust_program_registry_reserve(*ProgramInstanceReservation) u8;
extern fn r4os_rust_program_registry_reservation(u8, *const ProgramInstanceReservation) u8;
extern fn r4os_rust_program_registry_clear(*ProgramRegistrySlot) void;
extern fn r4os_rust_program_registry_completion_allocate(*const NativeProgramCompletionRequest) bool;
extern fn r4os_rust_program_registry_completion(u8, *ProgramCompletionNode) bool;
extern fn r4os_rust_program_registry_done(*ProgramInstance) void;
extern fn r4os_rust_program_registry_pin(ProgramProcessHandle, bool, *ProgramInstanceLease) bool;
extern fn r4os_rust_program_registry_pin_id(u32, *ProgramInstanceLease) bool;
extern fn r4os_rust_program_registry_unpin(*const ProgramInstanceLease) void;
extern fn r4os_rust_program_registry_retire(u8, ?*ProgramRegistrySlot, ProgramProcessHandle) ?*ProgramRegistrySlot;
extern fn r4os_rust_program_registry_missing(ProgramProcessHandle) i32;
extern fn r4os_rust_program_registry_snapshot(*ProgramRegistryStats) void;
extern fn r4os_rust_program_registry_mix(u64) u64;
extern fn r4os_rust_program_registry_iterator(*ProgramRegistryIterator) ?*ProgramInstance;




comptime {
    if (@sizeOf(NativeProgramRegistryState) != 152 or @offsetOf(NativeProgramRegistryState, "epoch") != 48 or @offsetOf(NativeProgramRegistryState, "next_generation") != 64 or @offsetOf(NativeProgramRegistryState, "failure_cursor") != 136 or @offsetOf(NativeProgramRegistryState, "next_instance_id") != 140 or
        @sizeOf(NativeProgramCompletionRequest) != 40 or @offsetOf(NativeProgramCompletionRequest, "owner_handle") != 16 or
        @sizeOf(ProgramRegistryIterator) != 24 or @offsetOf(ProgramRegistryIterator, "chunk") != 0 or @offsetOf(ProgramRegistryIterator, "slot_index") != 8 or @offsetOf(ProgramRegistryIterator, "include_done") != 16 or
        sync.LockRank.program_registry != 110 or @intFromEnum(sync.LockMode.sleepable) != 0 or @intFromEnum(sync.EventMode.auto_reset) != 1)
        @compileError("Native registry owner C projection drift");
    const limits = ProgramMemoryLimits{};
    if (PROGRAM_REGISTRY_CHUNK_SLOTS != 16 or PROGRAM_REGISTRY_WARM_CHUNKS != 2 or PROGRAM_REGISTRY_SHRINK_HYSTERESIS != 64 or PROGRAM_REGISTRY_PRESSURE_SHRINK_HYSTERESIS != 8 or EXIT_HISTORY_MAX != 16 or
        limits.vm_reserve_limit != 1024 * 1024 * 1024 or limits.vm_commit_limit != 256 * 1024 * 1024 or limits.resident_limit != 256 * 1024 * 1024 or limits.stack_reserve != 4 * 1024 * 1024 or limits.stack_initial_commit != 64 * 1024 or
        @intFromEnum(AppClass.gui) != 1 or @intFromEnum(MemoryProfile.normal) != 2 or @intFromEnum(InstanceRole.foreground) != 0 or PROGRAM_EXIT_REASON_NATURAL != 0 or PROGRAM_COMPLETION_FLAG_OWNER != 8 or sync.WAIT_FOREVER != std.math.maxInt(u64) or
        @intFromEnum(ProgramRegistrySlotState.free) != 0 or @intFromEnum(ProgramRegistrySlotState.create) != 1 or @intFromEnum(ProgramRegistrySlotState.publish) != 2 or @intFromEnum(ProgramRegistrySlotState.run) != 3 or @intFromEnum(ProgramRegistrySlotState.exit) != 4 or @intFromEnum(ProgramRegistrySlotState.done) != 5 or @intFromEnum(ProgramRegistrySlotState.retire) != 6 or @intFromEnum(ProgramRegistrySlotState.reap) != 7 or
        @intFromEnum(ProgramCompletionState.private) != 0 or @intFromEnum(ProgramCompletionState.pending) != 1 or @intFromEnum(ProgramCompletionState.consumed) != 3 or
        @intFromEnum(ProgramRegistryGrowResult.grown) != 0 or @intFromEnum(ProgramRegistryGrowResult.capacity_available) != 1 or @intFromEnum(ProgramRegistryGrowResult.injected_failure) != 2 or @intFromEnum(ProgramRegistryGrowResult.allocation_failure) != 3 or @intFromEnum(ProgramRegistryGrowResult.lock_failure) != 4 or
        @intFromEnum(ProgramRetireResult.idle) != 0 or @intFromEnum(ProgramRetireResult.completed) != 1 or @intFromEnum(ProgramRetireResult.deferred) != 2)
        @compileError("Native registry defaults, ordinals or bounded growth contract drift");
}

var args_buffer: [ARGS_MAX + 1]u8 = .{0} ** (ARGS_MAX + 1);

const R4L_PREEMPTION_PATH = "/R4OS/SOFTWARE/TERMINAL/DIAG/LSTRX.R4X";
const R4L_PREEMPTION_MODULE = "EXTMATH";
const R4L_PREEMPTION_FLAG_EXPORT = "PREEMPT_FLAG";
const R4L_PREEMPTION_TIMEOUT_NS: u64 = 5_000_000_000;





































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


pub fn configureProgramLifecycleFailureForTest(phase: ?ProgramLifecycleFailurePhase) void {
    _ = r4os_rust_program_failure_control(0, @intFromEnum(phase orelse .none));
}

fn armProgramLifecycleFailureForTest(phase: ProgramLifecycleFailurePhase) bool {
    return r4os_rust_program_failure_control(1, @intFromEnum(phase));
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
    var value: NativeProgramFailureSnapshot = undefined;
    r4os_rust_program_failure_snapshot(&value);
    return .{ .phase = value.phase, .armed = value.armed, .consumed = value.consumed, .reaper_signalled = value.reaper_signalled, .retried = value.retried, .recovered = value.recovered };
}

fn signalProgramReaperForTest() void {
    _ = r4os_rust_program_failure_control(2, 0);
}

fn consumeProgramLifecycleFailure(phase: ProgramLifecycleFailurePhase) bool {
    return r4os_rust_program_failure_control(3, @intFromEnum(phase));
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
    return r4os_rust_program_registry_control(0, 0) != 0;
}

fn unlockProgramRegistry() void {
    _ = r4os_rust_program_registry_control(1, 0);
}

fn programRegistryStateIsVisible(state: ProgramRegistrySlotState) bool {
    return r4os_rust_program_registry_state_test(0, @intFromEnum(state));
}

fn programRegistryStateOwnsPublishedPayloads(state: ProgramRegistrySlotState) bool {
    return r4os_rust_program_registry_state_test(1, @intFromEnum(state));
}

fn programRegistryStateIsRunning(state: ProgramRegistrySlotState) bool {
    return r4os_rust_program_registry_state_test(2, @intFromEnum(state));
}

fn programHandleValid(handle: ProgramProcessHandle) bool {
    return r4os_rust_program_handle_test(0, handle, .{});
}

fn programHandleEqual(a: ProgramProcessHandle, b: ProgramProcessHandle) bool {
    return r4os_rust_program_handle_test(1, a, b);
}

fn programHandleForSlot(slot: *const ProgramRegistrySlot) ProgramProcessHandle {
    var out: ProgramProcessHandle = undefined;
    _ = r4os_rust_program_registry_handle(0, slot, 0, &out);
    return out;
}

fn programHandleForReservation(reservation: *const ProgramInstanceReservation) ProgramProcessHandle {
    var out: ProgramProcessHandle = undefined;
    _ = r4os_rust_program_registry_handle(1, reservation, 0, &out);
    return out;
}

fn programHandleForInstance(instance: *const ProgramInstance) ?ProgramProcessHandle {
    var out: ProgramProcessHandle = undefined;
    return if (r4os_rust_program_registry_handle(2, instance, 0, &out)) out else null;
}

fn programRegistryChunkIsEmptyAndUnpinned(chunk: *const ProgramRegistryChunk) bool {
    return r4os_rust_program_registry_predicate(0, 0, chunk);
}

fn findFreeProgramRegistrySlotLocked() ?*ProgramRegistrySlot {
    return registryPointer(ProgramRegistrySlot, r4os_rust_program_registry_lookup(0, 0, .{}, null, false));
}

fn programRegistryChunkForSlotLocked(wanted: *const ProgramRegistrySlot) ?*ProgramRegistryChunk {
    return registryPointer(ProgramRegistryChunk, r4os_rust_program_registry_lookup(1, 0, .{}, @constCast(wanted), false));
}

fn programRegistrySlotForInstanceLocked(wanted: *const ProgramInstance) ?*ProgramRegistrySlot {
    return registryPointer(ProgramRegistrySlot, r4os_rust_program_registry_lookup(2, 0, .{}, @constCast(wanted), false));
}

fn lookupProgramRegistrySlotLocked(id: u32, include_retiring: bool) ?*ProgramRegistrySlot {
    return registryPointer(ProgramRegistrySlot, r4os_rust_program_registry_lookup(3, id, .{}, null, include_retiring));
}

fn lookupProgramRegistryHandleLocked(handle: ProgramProcessHandle, include_retiring: bool) ?*ProgramRegistrySlot {
    return registryPointer(ProgramRegistrySlot, r4os_rust_program_registry_lookup(4, 0, handle, null, include_retiring));
}

fn programRegistryHasIdLocked(id: u32) bool {
    return r4os_rust_program_registry_predicate(1, id, null);
}

fn completionForHandleLocked(handle: ProgramProcessHandle) ?*ProgramCompletionNode {
    return registryPointer(ProgramCompletionNode, r4os_rust_program_registry_lookup(5, 0, handle, null, false));
}

fn completionForIdLocked(id: u32) ?*ProgramCompletionNode {
    return registryPointer(ProgramCompletionNode, r4os_rust_program_registry_lookup(6, id, .{}, null, false));
}

fn completionHasIdLocked(id: u32) bool {
    return r4os_rust_program_registry_predicate(2, id, null);
}

fn programHandleMissingStatusLocked(handle: ProgramProcessHandle) i32 {
    return r4os_rust_program_registry_missing(handle);
}

fn programHandleForId(id: u32) ?ProgramProcessHandle {
    var out: ProgramProcessHandle = undefined;
    return if (r4os_rust_program_registry_handle(3, null, id, &out)) out else null;
}

fn programRegistryIdInUseLocked(id: u32) bool {
    return r4os_rust_program_registry_predicate(3, id, null);
}

fn countPublishedProgramRegistrySlotsLocked() u64 {
    return r4os_rust_program_registry_control(5, 0);
}

fn allocateProgramGenerationLocked() ?u64 {
    const value = r4os_rust_program_registry_control(3, 0);
    return if (value == 0) null else value;
}

fn allocateProgramInstanceIdLocked() ?u32 {
    const value: u32 = @intCast(r4os_rust_program_registry_control(4, 0));
    return if (value == 0) null else value;
}

pub fn configureProgramRegistryFailureForTest(fail_after: ?u32) void {
    _ = r4os_rust_program_registry_control(7, if (fail_after) |value| value else std.math.maxInt(u64));
}

pub fn configureProgramRegistryNextIdForTest(next_id: u32) void {
    _ = r4os_rust_program_registry_control(8, next_id);
}

pub fn configureProgramRegistryNextGenerationForTest(next_generation: u64) void {
    _ = r4os_rust_program_registry_control(9, next_generation);
}

pub fn armProgramRegistryNextGrowthFailureForTest() bool {
    return r4os_rust_program_registry_control(10, 0) != 0;
}

pub fn resetProgramRegistryGrowthFailureForTest() void {
    _ = r4os_rust_program_registry_control(11, 0);
}

fn shouldFailProgramRegistryGrowthLocked() bool {
    return r4os_rust_program_registry_control(12, 0) != 0;
}

fn allocateProgramRegistryChunkCandidate() ProgramRegistryChunkCandidateResult {
    var out: ?*ProgramRegistryChunk = null;
    return switch (r4os_rust_program_registry_candidate(&out)) {
        0 => .{ .chunk = out.? }, 1 => .injected_failure, 2 => .allocation_failure, 3 => .lock_failure, else => unreachable,
    };
}

fn freeProgramRegistryChunkMemory(chunk: *ProgramRegistryChunk) bool {
    return r4os_rust_program_registry_chunk(2, chunk);
}

fn linkProgramRegistryChunkLocked(chunk: *ProgramRegistryChunk) void {
    _ = r4os_rust_program_registry_chunk(0, chunk);
}

fn growProgramRegistry() ProgramRegistryGrowResult {
    return @enumFromInt(r4os_rust_program_registry_grow());
}

fn allocateProgramCompletionNode(reservation: *const ProgramInstanceReservation, owner: bool, owner_handle_override: ?ProgramProcessHandle, legacy_id: bool, retain_output: bool) bool {
    const request = NativeProgramCompletionRequest{ .reservation = reservation, .owner = owner, .override_present = owner_handle_override != null, .owner_handle = owner_handle_override orelse .{}, .legacy_id = legacy_id, .retain_output = retain_output };
    return r4os_rust_program_registry_completion_allocate(&request);
}

fn freeProgramCompletionNodeMemory(node: *ProgramCompletionNode) void {
    _ = r4os_rust_program_registry_completion(2, node);
}

fn linkProgramCompletionLocked(node: *ProgramCompletionNode) void {
    _ = r4os_rust_program_registry_completion(0, node);
}

fn unlinkProgramCompletionLocked(node: *ProgramCompletionNode) bool {
    return r4os_rust_program_registry_completion(1, node);
}

fn reserveProgramInstanceSlot() ProgramInstanceReservationResult {
    var out: ProgramInstanceReservation = undefined;
    return switch (r4os_rust_program_registry_reserve(&out)) {
        0 => .{ .reservation = out }, 1 => .{ .failure = .no_memory }, 2 => .{ .failure = .generation_exhausted }, else => unreachable,
    };
}

fn clearProgramRegistrySlotLocked(slot: *ProgramRegistrySlot) void {
    r4os_rust_program_registry_clear(slot);
}

fn bumpProgramInventoryEpochLocked() void {
    _ = r4os_rust_program_registry_control(6, 0);
}

fn cancelProgramInstanceReservation(reservation: *const ProgramInstanceReservation) void {
    _ = r4os_rust_program_registry_reservation(0, reservation);
}

fn finishCancelledProgramReservation(reservation: *const ProgramInstanceReservation) ProgramRetireResult {
    return @enumFromInt(r4os_rust_program_registry_reservation(1, reservation));
}

fn cleanupCancelledProgramResources(instance: *ProgramInstance) bool {
    return r4os_rust_program_peer_cleanup(instance);
}

fn publishProgramInstance(reservation: *const ProgramInstanceReservation) bool {
    return r4os_rust_program_registry_reservation(2, reservation) != 0;
}

fn markProgramInstanceDone(instance: *ProgramInstance) void {
    r4os_rust_program_registry_done(instance);
}

fn pinProgramHandle(handle: ProgramProcessHandle, include_done: bool) ?ProgramInstanceLease {
    var out: ProgramInstanceLease = undefined;
    return if (r4os_rust_program_registry_pin(handle, include_done, &out)) out else null;
}

fn pinProgramInstance(id: u32) ?ProgramInstanceLease {
    var out: ProgramInstanceLease = undefined;
    return if (r4os_rust_program_registry_pin_id(id, &out)) out else null;
}

fn unpinProgramInstance(lease: *const ProgramInstanceLease) void {
    r4os_rust_program_registry_unpin(lease);
}

fn pinProgramThreadExecution(thread_ctx: *ProgramThread) ?*ProgramInstance {
    return r4os_rust_program_thread_execution(thread_ctx, false);
}

fn unpinProgramThreadExecution(thread_ctx: *ProgramThread) void {
    _ = r4os_rust_program_thread_execution(thread_ctx, true);
}

fn pinConsoleInputTarget(thread_ctx: *ProgramThread, handle: ProgramProcessHandle) ?ProgramInstanceLease {
    var lease: ProgramInstanceLease = undefined;
    return if (r4os_rust_program_thread_input(thread_ctx, handle, &lease)) lease else null;
}

fn releaseConsoleInputTarget(thread_ctx: *ProgramThread) bool {
    return r4os_rust_program_thread_control(8, thread_ctx, 0);
}

fn naturalExitEpilogueOwnsTask(thread_ctx: *const ProgramThread) bool {
    return r4os_rust_program_thread_control(7, @constCast(thread_ctx), 0);
}

fn enqueueProgramRetireLocked(slot: *ProgramRegistrySlot) bool {
    return r4os_rust_program_registry_retire(0, slot, .{}) != null;
}

fn takeProgramRetireLocked() ?*ProgramRegistrySlot {
    return r4os_rust_program_registry_retire(1, null, .{});
}

fn queueProgramRetire(handle: ProgramProcessHandle) bool {
    return r4os_rust_program_registry_retire(2, null, handle) != null;
}

fn markProgramHandleRunning(handle: ProgramProcessHandle) void {
    _ = r4os_rust_program_registry_retire(3, null, handle);
}

fn programRegistryOwnerIsPublished(id: u32) bool {
    return r4os_rust_program_registry_predicate(4, id, null);
}

// Borrowed raw iterator for short cooperative scans that cannot yield, wait,
// retire, reclaim, or free registry chunks. It is deliberately not a general
// lifetime guarantee: mutating kill/reap paths select one ID and restart after
// finishInstance. 0.59.9 carries leases/snapshots across preemption and waits.
const ProgramRegistryIterator = extern struct {
    chunk: ?*ProgramRegistryChunk = null,
    slot_index: usize = 0,
    include_done: bool = false,
    fn next(self: *ProgramRegistryIterator) ?*ProgramInstance {
        return r4os_rust_program_registry_iterator(self);
    }
};

fn programRegistryIterator(include_done: bool) ProgramRegistryIterator {
    var out: ProgramRegistryIterator = undefined;
    r4os_rust_program_registry_iterator_new(include_done, &out);
    return out;
}

pub fn programRegistryStats() ProgramRegistryStats {
    var out: ProgramRegistryStats = undefined;
    r4os_rust_program_registry_snapshot(&out);
    return out;
}

fn mixProgramRegistryHash(value: u64) u64 {
    return r4os_rust_program_registry_mix(value);
}

fn unlinkProgramRegistryChunkLocked(candidate: *ProgramRegistryChunk) bool {
    return r4os_rust_program_registry_chunk(1, candidate);
}

fn shrinkProgramRegistry(under_pressure: bool) void {
    r4os_rust_program_registry_shrink(under_pressure);
}

pub fn reclaimProgramRegistryUnderPressure() void {
    r4os_rust_program_registry_shrink(true);
}






// R4NET-Gruppentabelle fuer den R4L-Importvertrag.



const api_allocator_vtable = ProgramAllocatorVTable{
    .alloc = apiAllocatorAlloc,
    .resize = apiAllocatorResize,
    .remap = apiAllocatorRemap,
    .free = apiAllocatorFree,
};

pub fn initializeRuntime(usable_bytes: u64) void {
    r4os_rust_program_runtime_initialize(usable_bytes);
}

fn configureApiGroups() void {
    r4os_rust_program_runtime_configure(0);
}

fn configureR4XStartR4SysTable() void {
    r4os_rust_program_runtime_configure(1);
}

fn configureR4XStartR4DeskTable() void {
    r4os_rust_program_runtime_configure(2);
}

fn configureR4XStartR4DrawTable() void {
    r4os_rust_program_runtime_configure(3);
}

// Verdrahtung der R4NET-Gruppentabelle mit ihren aktuellen Kernel-
// Implementierungen. Die Tabellenfelder sind der oeffentliche R4L-Ausgang.
fn configureR4XStartR4NetTable() void {
    r4os_rust_program_runtime_configure(4);
}

fn configureR4XStartR4AudioTable() void {
    r4os_rust_program_runtime_configure(5);
}

fn apiAudioOpenStream(rate: u32, channels: u16, format: u16) callconv(.c) i32 {
    return r4os_rust_program_domain_audio_open_stream(rate, channels, format);
}

fn apiAudioOutputInfo(index: u32, out: *audio.AudioOutputInfo) callconv(.c) i32 {
    return r4os_rust_program_domain_audio_output_info(index, out);
}

fn apiAudioSelectOutput(id: *const [64]u8) callconv(.c) i32 {
    return r4os_rust_program_domain_audio_select_output(id);
}

fn apiAudioWrite(stream_id: u32, data_ptr: [*]const u8, byte_count: u32) callconv(.c) i32 {
    return r4os_rust_program_domain_audio_write(stream_id, data_ptr, byte_count);
}

fn apiAudioClose(stream_id: u32) callconv(.c) i32 {
    return r4os_rust_program_domain_audio_close(stream_id);
}

fn apiAudioSetVolume(stream_id: u32, fixed_volume: u32) callconv(.c) i32 {
    return r4os_rust_program_domain_audio_set_volume(stream_id, fixed_volume);
}

fn fillProgramInstanceStorageSummary(out: *r4api.r4dev.ProgramInstanceStorageSummary) void {
    r4os_rust_program_diagnostic_storage(out);
}

fn runProgramInstanceStorageSelfTestProvider(out: *r4api.r4dev.ProgramInstanceStorageSelfTestResult) i32 {
    return r4os_rust_program_diagnostic_storage_test(out);
}

fn fillProgramRegistrySummary(out: *r4api.r4dev.ProgramRegistrySummaryV2) void {
    r4os_rust_program_diagnostic_registry(out);
}

fn programRegistrySelfTestAllowed() bool {
    return r4os_rust_program_selftest_allowed(1);
}

fn programLifecycleSelfTestAllowed() bool {
    return r4os_rust_program_selftest_allowed(2);
}

fn runProgramRegistrySelfTestProvider(out: *r4api.r4dev.ProgramRegistrySelfTestResultV2) i32 {
    return r4os_rust_program_diagnostic_registry_test(out);
}

fn fillInputPerformanceInfo(out: *r4api.r4dev.ProgramInputPerformanceInfo) void {
    r4os_rust_program_diagnostic_input(out);
}

fn configureR4XStartR4DevTable() void {
    r4os_rust_program_runtime_configure(6);
}

pub fn runPath(d: *drive.Drive, path: []const u8, args: []const u8, working_drive: *drive.Drive) RunResult {
    return @enumFromInt(r4os_rust_program_launch_path(0, d, .{ .data = path.ptr, .len = path.len }, .{ .data = args.ptr, .len = args.len }, working_drive, 0, null));
}

pub fn runShellPath(d: *drive.Drive, path: []const u8, args: []const u8, working_drive: *drive.Drive) RunResult {
    return @enumFromInt(r4os_rust_program_launch_path(1, d, .{ .data = path.ptr, .len = path.len }, .{ .data = args.ptr, .len = args.len }, working_drive, 0, null));
}

pub fn runShellPathWithHost(d: *drive.Drive, path: []const u8, args: []const u8, working_drive: *drive.Drive, host: ConsoleHostKind) RunResult {
    return @enumFromInt(r4os_rust_program_launch_path(1, d, .{ .data = path.ptr, .len = path.len }, .{ .data = args.ptr, .len = args.len }, working_drive, @intCast(@intFromEnum(host)), null));
}

pub fn activeShellInstanceId() u32 {
    return r4os_rust_program_info_shell();
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
    var handle: ProgramProcessHandle = .{};
    return r4os_rust_program_launch_path(2, d, .{ .data = path.ptr, .len = path.len }, .{ .data = args.ptr, .len = args.len }, working_drive, 0, &handle);
}

fn spawnPathWithHandle(d: *drive.Drive, path: []const u8, args: []const u8, working_drive: *drive.Drive, handle: *ProgramProcessHandle) i32 {
    return r4os_rust_program_launch_path(2, d, .{ .data = path.ptr, .len = path.len }, .{ .data = args.ptr, .len = args.len }, working_drive, 0, handle);
}

pub fn programClassIdForPath(d: *drive.Drive, path: []const u8) i32 {
    return r4os_rust_program_launch_path(3, d, .{ .data = path.ptr, .len = path.len }, .{ .data = undefined, .len = 0 }, d, 0, null);
}

fn programModuleRunningByPath(drive_letter: u8, path: []const u8) i32 {
    return r4os_rust_program_launch_running(drive_letter, .{ .data = path.ptr, .len = path.len });
}

fn moduleOriginMatchesResolved(origin: []const u8, drive_letter: u8, path: []const u8) bool {
    return r4os_rust_program_launch_origin(.{ .data = origin.ptr, .len = origin.len }, drive_letter, .{ .data = path.ptr, .len = path.len });
}

pub fn requestCloseInstance(id: u32) i32 {
    return r4os_rust_program_process_legacy(0, id);
}

pub fn killInstance(id: u32) i32 {
    return r4os_rust_program_process_legacy(1, id);
}

pub fn instanceSnapshot(id: u32) ?InstanceSnapshot {
    var value: NativeProgramInstanceSnapshot = undefined;
    if (!r4os_rust_program_info_snapshot(id, &value)) return null;
    return .{ .id = value.id, .task_id = value.task_id, .app_class = value.app_class, .state = value.state, .close_requested = value.close_requested, .exit_code = value.exit_code };
}

pub fn takeExitCode(id: u32) ?i32 {
    var out: i32 = undefined;
    return if (r4os_rust_program_process_history(id, &out)) out else null;
}

fn resolveProgramFile(d: *drive.Drive, path: []const u8) ?ProgramFile {
    var out: NativeProgramLaunchFile = undefined;
    if (!r4os_rust_program_launch_resolve(d, .{ .data = path.ptr, .len = path.len }, false, &out)) return null;
    return programLaunchFileFromNative(out);
}

fn resolveProgramFileWithBootReport(d: *drive.Drive, path: []const u8, report_boot_launch: bool) ?ProgramFile {
    var out: NativeProgramLaunchFile = undefined;
    if (!r4os_rust_program_launch_resolve(d, .{ .data = path.ptr, .len = path.len }, report_boot_launch, &out)) return null;
    return programLaunchFileFromNative(out);
}

fn reportBootFilesystemWait(enabled: bool, drive_letter: u8) void {
    r4os_rust_program_launch_fs_wait(enabled, drive_letter);
}

fn programModuleBaseName(instance: ?*const ProgramInstance) ?[]const u8 {
    var out: NativeProgramBytes = undefined;
    return if (r4os_rust_program_launch_basename(instance, &out)) out.data[0..out.len] else null;
}

fn filesystemKindLabel(kind: fs_request.Kind) []const u8 {
    const out = r4os_rust_program_launch_kind(@intFromEnum(kind));
    return out.data[0..out.len];
}

pub fn usedDisplay() bool {
    return r4os_rust_program_launch_last(true) != 0;
}

pub fn lastExitCode() i32 {
    return r4os_rust_program_launch_last(false);
}

pub fn beginOutputCapture(buffer: []u8) void {
    r4os_rust_program_io_capture(true, buffer.ptr, buffer.len, null);
}

pub fn endOutputCapture() OutputCaptureResult {
    var result: NativeProgramOutputCaptureResult = undefined;
    r4os_rust_program_io_capture(false, null, 0, &result);
    return .{ .len = result.len, .truncated = result.truncated };
}

pub fn beginInputCapture(data: []const u8) void {
    r4os_rust_program_console_input_capture(.{ .data = data.ptr, .len = data.len }, true);
}

pub fn endInputCapture() void {
    r4os_rust_program_console_input_capture(.{ .data = undefined, .len = 0 }, false);
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
    // Test-profile-only construction seam used by the AP preemption proof.
    // It is accepted solely for the internal audited R4X allow-list.
    forced_cpu: ?u8 = null,
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
    var out: NativeProgramSubsystemTrace = undefined;
    if (!r4os_rust_program_entry_trace(.{ .data = args.ptr, .len = args.len }, &out)) return null;
    return .{ .id = out.id.data[0..out.id.len], .mode = out.mode.data[0..out.mode.len] };
}





fn traceNowNanoseconds() u64 {
    return r4os_rust_program_entry_now();
}

fn logSubsystemTracePhase(trace: SubsystemTrace, phase: []const u8, now_ns: u64) void {
    r4os_rust_program_entry_trace_phase(.{ .id = .{ .data = trace.id.ptr, .len = trace.id.len }, .mode = .{ .data = trace.mode.ptr, .len = trace.mode.len } }, .{ .data = phase.ptr, .len = phase.len }, now_ns);
}



fn setProgramLaunchError(options: ProgramLaunchOptions, status: i32) void {
    const native_options = nativeProgramLaunchOptions(options);
    r4os_rust_program_launch_error(&native_options, status);
}

fn leaveProgramSpawnTransaction(thread_ctx: ?*ProgramThread) void {
    r4os_rust_program_thread_leave_spawn(thread_ctx);
}

fn programSpawnTransactionCancelled() bool {
    return r4os_rust_program_thread_scalar(1) != 0;
}

fn runProgramFile(file: ProgramFile, mode: RunMode, policy: LaunchPolicy, args: []const u8, working_drive: *drive.Drive, shell_host: ConsoleHostKind, options: ProgramLaunchOptions) RunResult {
    const native_file = nativeProgramLaunchFile(file);
    const native_options = nativeProgramLaunchOptions(options);
    return @enumFromInt(r4os_rust_program_launch_file(&native_file, @intCast(@intFromEnum(mode)), @intFromEnum(policy), .{ .data = args.ptr, .len = args.len }, working_drive, @intCast(@intFromEnum(shell_host)), &native_options));
}



const R4XExportContract = struct {
    has_start_v1: bool = false,
    has_start_v2_or_newer: bool = false,

    fn hasExactStartV1(self: R4XExportContract) bool {
        return self.has_start_v1 and !self.has_start_v2_or_newer;
    }
};

const ProgramMetadataContract = struct {
    memory: ProgramMemoryContract,
    parallel_execution: bool,
    desktop_host: bool,
};







const NativeProgramBytes = extern struct { data: [*]const u8, len: usize };
const NativeProgramImage = extern struct { range_id: u32, code: [*]u8, len: usize, owner_id: u32 };
const NativeLoadedProgram = extern struct {
    image: NativeProgramImage,
    entry: u64,
    memory: ProgramMemoryContract,
    parallel_execution: bool,
    desktop_host: bool,
    imports: [MAX_R4M_IMPORTS]R4XStartImportSeed,
    import_count: u32,
    section_count: u32,
    relocation_count: u32,
};
const NativeProgramImageReader = extern struct {
    context: ?*anyopaque,
    read: *const fn (?*anyopaque, usize, [*]u8, usize, NativeProgramBytes, bool) callconv(.c) bool,
};
const NativeProgramImageRequest = extern struct {
    reader: *const NativeProgramImageReader,
    source: *const module_file.NativeSource,
    header: module_r4m.Header,
    file_size: usize,
    owner_id: u32,
    app_class: u8,
};
extern fn r4os_rust_program_image_load(*const NativeProgramImageRequest, *NativeLoadedProgram) bool;
extern fn r4os_rust_program_memory_contract(NativeProgramBytes, u8, *ProgramMemoryContract) bool;
fn loadR4MProgramImage(file: ProgramFile, owner_id: u32, app_class: AppClass, reader: *module_r4m.Reader, header: module_r4m.Header) ?LoadedProgram {
    const native_reader = NativeProgramImageReader{ .context = reader, .read = r4os_zig_program_reader_read };
    const source = module_file.nativeSource(module_file.FileSource{ .volume = file.volume, .entry = file.entry, .drive_letter = file.drive_letter });
    const request = NativeProgramImageRequest{ .reader = &native_reader, .source = &source, .header = header, .file_size = @intCast(file.entry.size), .owner_id = owner_id, .app_class = @intFromEnum(app_class) };
    var loaded: NativeLoadedProgram = undefined;
    if (!r4os_rust_program_image_load(&request, &loaded)) return null;
    return .{
        .image = .{ .range_id = loaded.image.range_id, .code = loaded.image.code[0..loaded.image.len], .owner_id = loaded.image.owner_id },
        .entry = @ptrFromInt(loaded.entry),
        .memory_contract = loaded.memory,
        .parallel_execution = loaded.parallel_execution,
        .desktop_host = loaded.desktop_host,
        .imports = loaded.imports,
        .import_count = loaded.import_count,
        .loader_section_count = loaded.section_count,
        .loader_relocation_count = loaded.relocation_count,
    };
}
export fn r4os_zig_program_reader_read(context: ?*anyopaque, offset: usize, out: [*]u8, len: usize, name: NativeProgramBytes, verbose: bool) callconv(.c) bool {
    const reader: *module_r4m.Reader = @ptrCast(@alignCast(context.?));
    return reader.file.readExactAt(offset, out[0..len], name.data[0..name.len], verbose);
}
export fn r4os_zig_program_image_read(source: *const module_file.NativeSource, offset: usize, out: [*]u8, len: usize) callconv(.c) bool {
    return module_file.readExact(.{ .source = module_file.sourceFromNative(source.*), .offset = offset, .out = out[0..len], .name = "r4x-section-payload", .verbose = true });
}



comptime {
    if (@sizeOf(ProgramMemoryLimits) != 40 or @sizeOf(ProgramMemoryContract) != 64 or @offsetOf(ProgramMemoryContract, "limits") != 8 or @offsetOf(ProgramMemoryContract, "tag") != 48 or
        @sizeOf(R4XStartImportSeed) != 112 or @offsetOf(R4XStartImportSeed, "module_name") != 32 or @offsetOf(R4XStartImportSeed, "module_name_len") != 64 or @offsetOf(R4XStartImportSeed, "symbol_name") != 72 or @offsetOf(R4XStartImportSeed, "symbol_name_len") != 104 or
        @sizeOf(NativeProgramImage) != 32 or @sizeOf(NativeLoadedProgram) != 1920 or @offsetOf(NativeLoadedProgram, "imports") != 112 or @sizeOf(NativeProgramImageRequest) != 96 or @offsetOf(NativeProgramImageRequest, "file_size") != 80)
        @compileError("Native R4X memory/image C projection layout drift");
}





fn resolveProgramMemoryContractMetadata(meta: []const u8, app_class: AppClass) ?ProgramMemoryContract {
    var out: ProgramMemoryContract = undefined;
    return if (r4os_rust_program_memory_contract(.{ .data = meta.ptr, .len = meta.len }, @intFromEnum(app_class), &out)) out else null;
}

fn r4xstartGroupInterface(group_id: u32) ?u64 {
    const address = r4os_rust_program_runtime_group(group_id);
    return if (address != 0) address else null;
}

fn allocateProgramImage(code_size: usize, owner_id: u32) ?ProgramImage {
    var image: NativeProgramImage = undefined;
    if (!r4os_rust_program_image_allocate(code_size, owner_id, &image)) return null;
    return .{ .range_id = image.range_id, .code = image.code[0..image.len], .owner_id = image.owner_id };
}





fn freeProgramImage(image: ProgramImage) bool {
    return r4os_rust_program_image_release(image.range_id);
}

fn releaseProgramImageRange(range_id: u32) bool {
    return r4os_rust_program_image_release(range_id);
}





pub fn stackTelemetryInfo(index: u32, out: *r4x_api.ProgramStackPerformanceInfo) bool {
    return r4os_rust_program_stack_telemetry(index, out);
}





fn measureProgramStackHighWater(stack: *ProgramStack) void {
    r4os_rust_program_stack_measure(stack);
}



// Explicit stack traces also enter the existing bounded kernel log. Format
// once per requested trace, append one whole record under the bootlog owner,
// then release that owner before writing to the console/serial sink.




fn allocateProgramStack(owner_id: u32, contract: ProgramMemoryContract, serial_telemetry: bool) ?ProgramStack {
    var stack: ProgramStack = undefined;
    if (!r4os_rust_program_stack_allocate(owner_id, contract.limits.stack_reserve, contract.limits.stack_initial_commit, @intFromEnum(contract.profile), serial_telemetry, &stack)) return null;
    return stack;
}

fn allocateProgramStackWithLimits(owner_id: u32, reserve_size: u64, initial_commit_size: u64, profile: MemoryProfile, serial_telemetry: bool) ?ProgramStack {
    var stack: ProgramStack = undefined;
    if (!r4os_rust_program_stack_allocate(owner_id, reserve_size, initial_commit_size, @intFromEnum(profile), serial_telemetry, &stack)) return null;
    return stack;
}

fn freeProgramStack(stack: *ProgramStack) bool {
    return r4os_rust_program_stack_free(stack);
}



fn freeProgramResources(image: ProgramImage, stack: ProgramStack) void {
    const native_image: NativeProgramImage = .{ .range_id = image.range_id, .code = image.code.ptr, .len = image.code.len, .owner_id = image.owner_id };
    r4os_rust_program_resources_free(&native_image, &stack);
}

fn programResourcesFromValues(image: ProgramImage, stack: ProgramStack) ProgramResources {
    const native_image: NativeProgramImage = .{ .range_id = image.range_id, .code = image.code.ptr, .len = image.code.len, .owner_id = image.owner_id };
    var resources: ProgramResources = undefined;
    r4os_rust_program_resources_values(&native_image, &stack, &resources);
    return resources;
}

fn programResourcesFromInstance(instance: *const ProgramInstance) ProgramResources {
    var resources: ProgramResources = undefined;
    r4os_rust_program_resources_instance(instance, &resources);
    return resources;
}

fn cleanupProgramResources(resources: *ProgramResources) bool {
    return r4os_rust_program_resources_cleanup(resources);
}
/// A pageable VM fault may wait for filesystem I/O. Its continuation must
/// live on the interrupted task's own stack, never the CPU's shared IST.
/// Called with interrupts disabled on that IST; only resident stack growth
/// is allowed here. The ordinary terminal guard remains the lower bound.
pub fn preparePageFaultStack(addr: u64, interrupted_rsp: u64, frame_bytes: u64) ?u64 {
    var frame: u64 = undefined;
    return if (r4os_rust_program_fault_prepare(addr, interrupted_rsp, frame_bytes, &frame)) frame else null;
}

pub fn handlePageFault(addr: u64, error_code: u64, allow_pageable_vm: bool) bool {
    return r4os_rust_program_fault_handle(addr, error_code, allow_pageable_vm);
}







fn stackFromInstance(instance: *const ProgramInstance) ?ProgramStack {
    var stack: ProgramStack = undefined;
    return if (r4os_rust_program_stack_from_instance(instance, &stack)) stack else null;
}

fn writeStackToInstance(instance: *ProgramInstance, stack: ProgramStack) void {
    r4os_rust_program_stack_to_instance(instance, &stack);
}







fn checkedAdd(a: usize, b: usize) ?usize {
    var out: usize = undefined;
    return if (r4os_rust_program_util_number(0, a, b, &out)) out else null;
}



fn alignUp(value: usize, alignment: usize) ?usize {
    var out: usize = undefined;
    return if (r4os_rust_program_util_number(1, value, alignment, &out)) out else null;
}

fn isPowerOfTwo(value: usize) bool {
    return r4os_rust_program_util_number(2, value, 0, null);
}



fn cancelPublishedSpawn(handle: ProgramProcessHandle) void {
    r4os_rust_program_launch_cancel(handle);
}

fn runForegroundProgram(reservation: *const ProgramInstanceReservation, reservation_active: *bool, loaded: LoadedProgram, stack: ProgramStack, app_class: AppClass, args: []const u8, working_drive: *drive.Drive, options: ProgramLaunchOptions) RunResult {
    const native_loaded = nativeProgramLaunchLoaded(loaded);
    const native_options = nativeProgramLaunchOptions(options);
    return @enumFromInt(r4os_rust_program_launch_mode(0, reservation, reservation_active, &native_loaded, &stack, @intFromEnum(app_class), .{ .data = args.ptr, .len = args.len }, working_drive, 0, &native_options));
}

fn runShellProgram(reservation: *const ProgramInstanceReservation, reservation_active: *bool, loaded: LoadedProgram, stack: ProgramStack, app_class: AppClass, args: []const u8, working_drive: *drive.Drive, host: ConsoleHostKind, options: ProgramLaunchOptions) RunResult {
    const native_loaded = nativeProgramLaunchLoaded(loaded);
    const native_options = nativeProgramLaunchOptions(options);
    return @enumFromInt(r4os_rust_program_launch_mode(1, reservation, reservation_active, &native_loaded, &stack, @intFromEnum(app_class), .{ .data = args.ptr, .len = args.len }, working_drive, @intCast(@intFromEnum(host)), &native_options));
}

fn runBackgroundProgram(reservation: *const ProgramInstanceReservation, reservation_active: *bool, loaded: LoadedProgram, stack: ProgramStack, app_class: AppClass, args: []const u8, working_drive: *drive.Drive, host: ConsoleHostKind, options: ProgramLaunchOptions) RunResult {
    const native_loaded = nativeProgramLaunchLoaded(loaded);
    const native_options = nativeProgramLaunchOptions(options);
    return @enumFromInt(r4os_rust_program_launch_mode(2, reservation, reservation_active, &native_loaded, &stack, @intFromEnum(app_class), .{ .data = args.ptr, .len = args.len }, working_drive, @intCast(@intFromEnum(host)), &native_options));
}











// Short Test-profile acceptance: bounded LSTRX invocations exercise imported
// R4L IRQ preemption and an equal-rank kernel continuation on the same AP.
pub fn runR4lPreemptionAcceptance(target_cpu: u32, usable_bytes: u64) bool {
    return r4os_rust_program_preemption_acceptance(target_cpu,usable_bytes);
}

// Private native registry views preserve the exact active Zig record layouts.
comptime {
    if (@offsetOf(ProgramInstance, "program_stack_guard_size") != 0) @compileError("Native ProgramInstance.program_stack_guard_size layout");
    if (@offsetOf(ProgramInstance, "memory_limits") != 8) @compileError("Native ProgramInstance.memory_limits layout");
    if (@offsetOf(ProgramInstance, "gui_payload") != 48) @compileError("Native ProgramInstance.gui_payload layout");
    if (@offsetOf(ProgramInstance, "notifications") != 56) @compileError("Native ProgramInstance.notifications layout");
    if (@offsetOf(ProgramInstance, "console_payload") != 80) @compileError("Native ProgramInstance.console_payload layout");
    if (@offsetOf(ProgramInstance, "process_payload") != 88) @compileError("Native ProgramInstance.process_payload layout");
    if (@offsetOf(ProgramInstance, "runtime_payload") != 96) @compileError("Native ProgramInstance.runtime_payload layout");
    if (@offsetOf(ProgramInstance, "program_stack_telemetry_high_water") != 104) @compileError("Native ProgramInstance.program_stack_telemetry_high_water layout");
    if (@offsetOf(ProgramInstance, "program_stack_create_cycles") != 112) @compileError("Native ProgramInstance.program_stack_create_cycles layout");
    if (@offsetOf(ProgramInstance, "entry") != 120) @compileError("Native ProgramInstance.entry layout");
    if (@offsetOf(ProgramInstance, "stack_top") != 128) @compileError("Native ProgramInstance.stack_top layout");
    if (@offsetOf(ProgramInstance, "program_stack_initial_commit_size") != 136) @compileError("Native ProgramInstance.program_stack_initial_commit_size layout");
    if (@offsetOf(ProgramInstance, "program_image_base") != 144) @compileError("Native ProgramInstance.program_image_base layout");
    if (@offsetOf(ProgramInstance, "program_image_size") != 152) @compileError("Native ProgramInstance.program_image_size layout");
    if (@offsetOf(ProgramInstance, "program_stack_guard_base") != 160) @compileError("Native ProgramInstance.program_stack_guard_base layout");
    if (@offsetOf(ProgramInstance, "program_stack_base") != 168) @compileError("Native ProgramInstance.program_stack_base layout");
    if (@offsetOf(ProgramInstance, "program_stack_reserve_size") != 176) @compileError("Native ProgramInstance.program_stack_reserve_size layout");
    if (@offsetOf(ProgramInstance, "program_stack_committed_base") != 184) @compileError("Native ProgramInstance.program_stack_committed_base layout");
    if (@offsetOf(ProgramInstance, "program_stack_committed_size") != 192) @compileError("Native ProgramInstance.program_stack_committed_size layout");
    if (@offsetOf(ProgramInstance, "program_image_range_id") != 200) @compileError("Native ProgramInstance.program_image_range_id layout");
    if (@offsetOf(ProgramInstance, "program_stack_telemetry_committed_pages") != 204) @compileError("Native ProgramInstance.program_stack_telemetry_committed_pages layout");
    if (@offsetOf(ProgramInstance, "program_stack_range_id") != 208) @compileError("Native ProgramInstance.program_stack_range_id layout");
    if (@offsetOf(ProgramInstance, "task_id") != 212) @compileError("Native ProgramInstance.task_id layout");
    if (@offsetOf(ProgramInstance, "id") != 216) @compileError("Native ProgramInstance.id layout");
    if (@offsetOf(ProgramInstance, "exit_code") != 220) @compileError("Native ProgramInstance.exit_code layout");
    if (@offsetOf(ProgramInstance, "desktop_host") != 224) @compileError("Native ProgramInstance.desktop_host layout");
    if (@offsetOf(ProgramInstance, "program_stack_serial_telemetry") != 225) @compileError("Native ProgramInstance.program_stack_serial_telemetry layout");
    if (@offsetOf(ProgramInstance, "app_class") != 226) @compileError("Native ProgramInstance.app_class layout");
    if (@offsetOf(ProgramInstance, "role") != 227) @compileError("Native ProgramInstance.role layout");
    if (@offsetOf(ProgramInstance, "parallel_execution") != 228) @compileError("Native ProgramInstance.parallel_execution layout");
    if (@offsetOf(ProgramInstance, "program_stack_telemetry_measured") != 229) @compileError("Native ProgramInstance.program_stack_telemetry_measured layout");
    if (@offsetOf(ProgramInstance, "memory_profile") != 230) @compileError("Native ProgramInstance.memory_profile layout");
    if (@offsetOf(ProgramInstance, "memory_tag") != 231) @compileError("Native ProgramInstance.memory_tag layout");
    if (@offsetOf(ProgramInstance, "used") != 247) @compileError("Native ProgramInstance.used layout");
    if (@offsetOf(ProgramInstance, "gfx_virtual_closed") != 248) @compileError("Native ProgramInstance.gfx_virtual_closed layout");
    if (@offsetOf(ProgramInstance, "display_used") != 249) @compileError("Native ProgramInstance.display_used layout");
    if (@offsetOf(ProgramInstance, "close_requested") != 250) @compileError("Native ProgramInstance.close_requested layout");
    if (@offsetOf(ProgramInstance, "desktop_requested") != 251) @compileError("Native ProgramInstance.desktop_requested layout");
    if (@offsetOf(ProgramInstance, "done") != 252) @compileError("Native ProgramInstance.done layout");
    if (@offsetOf(ProgramInstance, "storage_teardown_blocked") != 253) @compileError("Native ProgramInstance.storage_teardown_blocked layout");
    if (@sizeOf(ProgramInstance) != 256 or @alignOf(ProgramInstance) != 8) @compileError("Native ProgramInstance geometry");
    if (@offsetOf(ProgramRegistrySlot, "generation") != 0) @compileError("Native ProgramRegistrySlot.generation layout");
    if (@offsetOf(ProgramRegistrySlot, "retire_next") != 8) @compileError("Native ProgramRegistrySlot.retire_next layout");
    if (@offsetOf(ProgramRegistrySlot, "completion") != 16) @compileError("Native ProgramRegistrySlot.completion layout");
    if (@offsetOf(ProgramRegistrySlot, "instance") != 24) @compileError("Native ProgramRegistrySlot.instance layout");
    if (@offsetOf(ProgramRegistrySlot, "public_id") != 280) @compileError("Native ProgramRegistrySlot.public_id layout");
    if (@offsetOf(ProgramRegistrySlot, "pin_count") != 284) @compileError("Native ProgramRegistrySlot.pin_count layout");
    if (@offsetOf(ProgramRegistrySlot, "retire_attempts") != 288) @compileError("Native ProgramRegistrySlot.retire_attempts layout");
    if (@offsetOf(ProgramRegistrySlot, "state") != 292) @compileError("Native ProgramRegistrySlot.state layout");
    if (@offsetOf(ProgramRegistrySlot, "reclaim_pending") != 293) @compileError("Native ProgramRegistrySlot.reclaim_pending layout");
    if (@offsetOf(ProgramRegistrySlot, "retire_queued") != 294) @compileError("Native ProgramRegistrySlot.retire_queued layout");
    if (@offsetOf(ProgramRegistrySlot, "retire_in_progress") != 295) @compileError("Native ProgramRegistrySlot.retire_in_progress layout");
    if (@offsetOf(ProgramRegistrySlot, "cancel_pending") != 296) @compileError("Native ProgramRegistrySlot.cancel_pending layout");
    if (@offsetOf(ProgramRegistrySlot, "retire_phase") != 297) @compileError("Native ProgramRegistrySlot.retire_phase layout");
    if (@offsetOf(ProgramRegistrySlot, "retire_output_detached") != 298) @compileError("Native ProgramRegistrySlot.retire_output_detached layout");
    if (@offsetOf(ProgramRegistrySlot, "retire_storage_released") != 299) @compileError("Native ProgramRegistrySlot.retire_storage_released layout");
    if (@offsetOf(ProgramRegistrySlot, "retire_image_stack_released") != 300) @compileError("Native ProgramRegistrySlot.retire_image_stack_released layout");
    if (@offsetOf(ProgramRegistrySlot, "retire_owner_released") != 301) @compileError("Native ProgramRegistrySlot.retire_owner_released layout");
    if (@sizeOf(ProgramRegistrySlot) != 304 or @alignOf(ProgramRegistrySlot) != 8) @compileError("Native ProgramRegistrySlot geometry");
    if (@offsetOf(ProgramRegistryChunk, "next") != 0) @compileError("Native ProgramRegistryChunk.next layout");
    if (@offsetOf(ProgramRegistryChunk, "serial") != 8) @compileError("Native ProgramRegistryChunk.serial layout");
    if (@offsetOf(ProgramRegistryChunk, "empty_since_epoch") != 16) @compileError("Native ProgramRegistryChunk.empty_since_epoch layout");
    if (@offsetOf(ProgramRegistryChunk, "slots") != 24) @compileError("Native ProgramRegistryChunk.slots layout");
    if (@sizeOf(ProgramRegistryChunk) != 4888 or @alignOf(ProgramRegistryChunk) != 8) @compileError("Native ProgramRegistryChunk geometry");
    if (@offsetOf(ProgramInstanceReservation, "slot") != 0) @compileError("Native ProgramInstanceReservation.slot layout");
    if (@offsetOf(ProgramInstanceReservation, "generation") != 8) @compileError("Native ProgramInstanceReservation.generation layout");
    if (@offsetOf(ProgramInstanceReservation, "id") != 16) @compileError("Native ProgramInstanceReservation.id layout");
    if (@sizeOf(ProgramInstanceReservation) != 24 or @alignOf(ProgramInstanceReservation) != 8) @compileError("Native ProgramInstanceReservation geometry");
    if (@offsetOf(ProgramInstanceLease, "slot") != 0) @compileError("Native ProgramInstanceLease.slot layout");
    if (@offsetOf(ProgramInstanceLease, "instance") != 8) @compileError("Native ProgramInstanceLease.instance layout");
    if (@offsetOf(ProgramInstanceLease, "generation") != 16) @compileError("Native ProgramInstanceLease.generation layout");
    if (@offsetOf(ProgramInstanceLease, "id") != 24) @compileError("Native ProgramInstanceLease.id layout");
    if (@sizeOf(ProgramInstanceLease) != 32 or @alignOf(ProgramInstanceLease) != 8) @compileError("Native ProgramInstanceLease geometry");
    if (@offsetOf(ProgramRegistryStats, "id_wraps") != 0) @compileError("Native ProgramRegistryStats.id_wraps layout");
    if (@offsetOf(ProgramRegistryStats, "live_id_hash") != 8) @compileError("Native ProgramRegistryStats.live_id_hash layout");
    if (@offsetOf(ProgramRegistryStats, "launch_failures") != 16) @compileError("Native ProgramRegistryStats.launch_failures layout");
    if (@offsetOf(ProgramRegistryStats, "retire_retries") != 24) @compileError("Native ProgramRegistryStats.retire_retries layout");
    if (@offsetOf(ProgramRegistryStats, "completion_peak") != 32) @compileError("Native ProgramRegistryStats.completion_peak layout");
    if (@offsetOf(ProgramRegistryStats, "live_address_hash") != 40) @compileError("Native ProgramRegistryStats.live_address_hash layout");
    if (@offsetOf(ProgramRegistryStats, "history_sequence") != 48) @compileError("Native ProgramRegistryStats.history_sequence layout");
    if (@offsetOf(ProgramRegistryStats, "history_count") != 56) @compileError("Native ProgramRegistryStats.history_count layout");
    if (@offsetOf(ProgramRegistryStats, "history_head") != 64) @compileError("Native ProgramRegistryStats.history_head layout");
    if (@offsetOf(ProgramRegistryStats, "peak_chunks") != 72) @compileError("Native ProgramRegistryStats.peak_chunks layout");
    if (@offsetOf(ProgramRegistryStats, "peak_live") != 80) @compileError("Native ProgramRegistryStats.peak_live layout");
    if (@offsetOf(ProgramRegistryStats, "growth_attempts") != 88) @compileError("Native ProgramRegistryStats.growth_attempts layout");
    if (@offsetOf(ProgramRegistryStats, "growth_failures") != 96) @compileError("Native ProgramRegistryStats.growth_failures layout");
    if (@offsetOf(ProgramRegistryStats, "forced_failures") != 104) @compileError("Native ProgramRegistryStats.forced_failures layout");
    if (@offsetOf(ProgramRegistryStats, "publish_count") != 112) @compileError("Native ProgramRegistryStats.publish_count layout");
    if (@offsetOf(ProgramRegistryStats, "rollback_count") != 120) @compileError("Native ProgramRegistryStats.rollback_count layout");
    if (@offsetOf(ProgramRegistryStats, "shrink_count") != 128) @compileError("Native ProgramRegistryStats.shrink_count layout");
    if (@offsetOf(ProgramRegistryStats, "id_collisions") != 136) @compileError("Native ProgramRegistryStats.id_collisions layout");
    if (@offsetOf(ProgramRegistryStats, "retire_deferred") != 144) @compileError("Native ProgramRegistryStats.retire_deferred layout");
    if (@offsetOf(ProgramRegistryStats, "retire_queued") != 152) @compileError("Native ProgramRegistryStats.retire_queued layout");
    if (@offsetOf(ProgramRegistryStats, "completion_output_bytes") != 160) @compileError("Native ProgramRegistryStats.completion_output_bytes layout");
    if (@offsetOf(ProgramRegistryStats, "stale_lease_rejections") != 168) @compileError("Native ProgramRegistryStats.stale_lease_rejections layout");
    if (@offsetOf(ProgramRegistryStats, "completion_ready") != 176) @compileError("Native ProgramRegistryStats.completion_ready layout");
    if (@offsetOf(ProgramRegistryStats, "completion_pending") != 184) @compileError("Native ProgramRegistryStats.completion_pending layout");
    if (@offsetOf(ProgramRegistryStats, "next_generation") != 192) @compileError("Native ProgramRegistryStats.next_generation layout");
    if (@offsetOf(ProgramRegistryStats, "last_admission_error") != 200) @compileError("Native ProgramRegistryStats.last_admission_error layout");
    if (@offsetOf(ProgramRegistryStats, "live_slots") != 204) @compileError("Native ProgramRegistryStats.live_slots layout");
    if (@offsetOf(ProgramRegistryStats, "chunk_slots") != 208) @compileError("Native ProgramRegistryStats.chunk_slots layout");
    if (@offsetOf(ProgramRegistryStats, "chunk_count") != 212) @compileError("Native ProgramRegistryStats.chunk_count layout");
    if (@offsetOf(ProgramRegistryStats, "pinned_slots") != 216) @compileError("Native ProgramRegistryStats.pinned_slots layout");
    if (@offsetOf(ProgramRegistryStats, "retiring_slots") != 220) @compileError("Native ProgramRegistryStats.retiring_slots layout");
    if (@offsetOf(ProgramRegistryStats, "done_slots") != 224) @compileError("Native ProgramRegistryStats.done_slots layout");
    if (@offsetOf(ProgramRegistryStats, "reserved_slots") != 228) @compileError("Native ProgramRegistryStats.reserved_slots layout");
    if (@offsetOf(ProgramRegistryStats, "free_slots") != 232) @compileError("Native ProgramRegistryStats.free_slots layout");
    if (@offsetOf(ProgramRegistryStats, "slot_capacity") != 236) @compileError("Native ProgramRegistryStats.slot_capacity layout");
    if (@offsetOf(ProgramRegistryStats, "last_launch_error") != 240) @compileError("Native ProgramRegistryStats.last_launch_error layout");
    if (@offsetOf(ProgramRegistryStats, "failure_armed") != 244) @compileError("Native ProgramRegistryStats.failure_armed layout");
    if (@sizeOf(ProgramRegistryStats) != 248 or @alignOf(ProgramRegistryStats) != 8) @compileError("Native ProgramRegistryStats geometry");
    if (@offsetOf(ProgramCompletionNode, "next") != 0) @compileError("Native ProgramCompletionNode.next layout");
    if (@offsetOf(ProgramCompletionNode, "handle") != 8) @compileError("Native ProgramCompletionNode.handle layout");
    if (@offsetOf(ProgramCompletionNode, "owner_handle") != 24) @compileError("Native ProgramCompletionNode.owner_handle layout");
    if (@offsetOf(ProgramCompletionNode, "sequence") != 40) @compileError("Native ProgramCompletionNode.sequence layout");
    if (@offsetOf(ProgramCompletionNode, "start_tick") != 48) @compileError("Native ProgramCompletionNode.start_tick layout");
    if (@offsetOf(ProgramCompletionNode, "finish_tick") != 56) @compileError("Native ProgramCompletionNode.finish_tick layout");
    if (@offsetOf(ProgramCompletionNode, "output_payload") != 64) @compileError("Native ProgramCompletionNode.output_payload layout");
    if (@offsetOf(ProgramCompletionNode, "output_storage_bytes") != 72) @compileError("Native ProgramCompletionNode.output_storage_bytes layout");
    if (@offsetOf(ProgramCompletionNode, "exit_code") != 80) @compileError("Native ProgramCompletionNode.exit_code layout");
    if (@offsetOf(ProgramCompletionNode, "task_id") != 84) @compileError("Native ProgramCompletionNode.task_id layout");
    if (@offsetOf(ProgramCompletionNode, "output_revision") != 88) @compileError("Native ProgramCompletionNode.output_revision layout");
    if (@offsetOf(ProgramCompletionNode, "output_length") != 92) @compileError("Native ProgramCompletionNode.output_length layout");
    if (@offsetOf(ProgramCompletionNode, "flags") != 96) @compileError("Native ProgramCompletionNode.flags layout");
    if (@offsetOf(ProgramCompletionNode, "console_state") != 100) @compileError("Native ProgramCompletionNode.console_state layout");
    if (@offsetOf(ProgramCompletionNode, "state") != 164) @compileError("Native ProgramCompletionNode.state layout");
    if (@offsetOf(ProgramCompletionNode, "slot_attached") != 165) @compileError("Native ProgramCompletionNode.slot_attached layout");
    if (@offsetOf(ProgramCompletionNode, "owner") != 166) @compileError("Native ProgramCompletionNode.owner layout");
    if (@offsetOf(ProgramCompletionNode, "legacy_id") != 167) @compileError("Native ProgramCompletionNode.legacy_id layout");
    if (@offsetOf(ProgramCompletionNode, "retain_output") != 168) @compileError("Native ProgramCompletionNode.retain_output layout");
    if (@offsetOf(ProgramCompletionNode, "app_class") != 169) @compileError("Native ProgramCompletionNode.app_class layout");
    if (@offsetOf(ProgramCompletionNode, "role") != 170) @compileError("Native ProgramCompletionNode.role layout");
    if (@offsetOf(ProgramCompletionNode, "exit_reason") != 171) @compileError("Native ProgramCompletionNode.exit_reason layout");
    if (@sizeOf(ProgramCompletionNode) != 176 or @alignOf(ProgramCompletionNode) != 8) @compileError("Native ProgramCompletionNode geometry");
    if (@offsetOf(ExitRecord, "handle") != 0) @compileError("Native ExitRecord.handle layout");
    if (@offsetOf(ExitRecord, "sequence") != 16) @compileError("Native ExitRecord.sequence layout");
    if (@offsetOf(ExitRecord, "start_tick") != 24) @compileError("Native ExitRecord.start_tick layout");
    if (@offsetOf(ExitRecord, "finish_tick") != 32) @compileError("Native ExitRecord.finish_tick layout");
    if (@offsetOf(ExitRecord, "exit_code") != 40) @compileError("Native ExitRecord.exit_code layout");
    if (@offsetOf(ExitRecord, "used") != 44) @compileError("Native ExitRecord.used layout");
    if (@offsetOf(ExitRecord, "app_class") != 45) @compileError("Native ExitRecord.app_class layout");
    if (@offsetOf(ExitRecord, "role") != 46) @compileError("Native ExitRecord.role layout");
    if (@offsetOf(ExitRecord, "exit_reason") != 47) @compileError("Native ExitRecord.exit_reason layout");
    if (@sizeOf(ExitRecord) != 48 or @alignOf(ExitRecord) != 8) @compileError("Native ExitRecord geometry");
}

fn programTaskMain() callconv(.c) void {
    r4os_rust_program_entry_main(false);
}

fn shellTaskMain() callconv(.c) void {
    r4os_rust_program_entry_main(true);
}

fn programThreadTaskMain() callconv(.c) void {
    r4os_rust_program_entry_worker();
}

fn callInstanceEntry(run: *ProgramInstance) i32 {
    return r4os_rust_program_entry_call(run);
}

fn logProgramStackHighWater(instance: *const ProgramInstance, stack: *const ProgramStack, thread_id: u32) void {
    r4os_rust_program_stack_log(instance, stack, thread_id);
}

fn apiFileRead(path: [*:0]const u8, out: [*]u8, max_len: u32) callconv(.c) i32 {
    return r4os_rust_program_async_file(1, path, 0, 0, 0, @intFromPtr(out), max_len, 0, true, null);
}

fn apiFileWrite(path: [*:0]const u8, data: [*]const u8, len: u32) callconv(.c) i32 {
    return r4os_rust_program_async_file(3, path, 0, @intFromPtr(data), len, 0, 0, 0, true, null);
}

fn apiFileReadAt(path: [*:0]const u8, offset: u32, out: [*]u8, max_len: u32) callconv(.c) i32 {
    return r4os_rust_program_async_file(2, path, offset, 0, 0, @intFromPtr(out), max_len, 0, true, null);
}

fn apiFileAppend(path: [*:0]const u8, data: [*]const u8, len: u32) callconv(.c) i32 {
    return r4os_rust_program_async_file(4, path, 0, @intFromPtr(data), len, 0, 0, 0, true, null);
}

fn apiFileStreamBegin(path: [*:0]const u8, flags: u32) callconv(.c) i32 {
    return r4os_rust_program_async_file(5, path, 0, 0, 0, 0, 0, flags, true, null);
}

fn apiFileStreamWrite(path: [*:0]const u8, offset: u64, data: [*]const u8, len: u32, flags: u32) callconv(.c) i32 {
    return r4os_rust_program_async_file(6, path, offset, @intFromPtr(data), len, 0, 0, flags, true, null);
}

fn apiFileStreamFinish(path: [*:0]const u8, expected_size: u64, flags: u32) callconv(.c) i32 {
    return r4os_rust_program_async_file(7, path, expected_size, 0, 0, 0, 0, flags, true, null);
}

fn apiFileStreamAbort(path: [*:0]const u8) callconv(.c) i32 {
    return r4os_rust_program_async_file(8, path, 0, 0, 0, 0, 0, 0, true, null);
}

fn apiIoFileRead(path: [*:0]const u8, out: [*]u8, max_len: u64, flags: u32, out_request_id: *u32) callconv(.c) i32 {
    return r4os_rust_program_async_file(1, path, 0, 0, 0, @intFromPtr(out), max_len, flags, false, out_request_id);
}

fn apiIoFileReadAt(path: [*:0]const u8, offset: u64, out: [*]u8, max_len: u64, flags: u32, out_request_id: *u32) callconv(.c) i32 {
    return r4os_rust_program_async_file(2, path, offset, 0, 0, @intFromPtr(out), max_len, flags, false, out_request_id);
}

fn apiIoFileWrite(path: [*:0]const u8, data: [*]const u8, len: u64, flags: u32, out_request_id: *u32) callconv(.c) i32 {
    return r4os_rust_program_async_file(3, path, 0, @intFromPtr(data), len, 0, 0, flags, false, out_request_id);
}

fn apiIoFileAppend(path: [*:0]const u8, data: [*]const u8, len: u64, flags: u32, out_request_id: *u32) callconv(.c) i32 {
    return r4os_rust_program_async_file(4, path, 0, @intFromPtr(data), len, 0, 0, flags, false, out_request_id);
}

fn apiIoFileWriteAt(path: [*:0]const u8, offset: u64, data: [*]const u8, len: u64, flags: u32, out_request_id: *u32) callconv(.c) i32 {
    return r4os_rust_program_async_file(10, path, offset, @intFromPtr(data), len, 0, 0, flags, false, out_request_id);
}

fn apiIoFileInfo(path: [*:0]const u8, flags: u32, out_request_id: *u32) callconv(.c) i32 {
    return r4os_rust_program_async_file(11, path, 0, 0, 0, 0, 0, flags, false, out_request_id);
}

fn apiIoFileLock(path: [*:0]const u8, offset: u64, length: u64, flags: u32, out_request_id: *u32) callconv(.c) i32 {
    return r4os_rust_program_async_file(12, path, offset, 0, length, 0, 0, flags, false, out_request_id);
}

fn apiIoFileStreamBegin(path: [*:0]const u8, flags: u32, out_request_id: *u32) callconv(.c) i32 {
    return r4os_rust_program_async_file(5, path, 0, 0, 0, 0, 0, flags, false, out_request_id);
}

fn apiIoFileStreamWrite(path: [*:0]const u8, offset: u64, data: [*]const u8, len: u64, flags: u32, out_request_id: *u32) callconv(.c) i32 {
    return r4os_rust_program_async_file(6, path, offset, @intFromPtr(data), len, 0, 0, flags, false, out_request_id);
}

fn apiIoFileStreamFinish(path: [*:0]const u8, expected_size: u64, flags: u32, out_request_id: *u32) callconv(.c) i32 {
    return r4os_rust_program_async_file(7, path, expected_size, 0, 0, 0, 0, flags, false, out_request_id);
}

fn apiIoFileStreamAbort(path: [*:0]const u8, out_request_id: *u32) callconv(.c) i32 {
    return r4os_rust_program_async_file(8, path, 0, 0, 0, 0, 0, 0, false, out_request_id);
}

fn apiIoServiceCall(handle: u32, op: u16, request_ptr: [*]const u8, request_len: u32, response_header: *ServiceMessageHeader, response_ptr: [*]u8, response_capacity: u32, timeout_ticks: u64, flags: u32, out_request_id: *u32) callconv(.c) i32 {
    return r4os_rust_program_async_service(handle, op, request_ptr, request_len, response_header, response_ptr, response_capacity, timeout_ticks, flags, false, out_request_id);
}

fn apiIoStatus(request_id: u32, out: *ProgramIoInfo) callconv(.c) i32 {
    return r4os_rust_program_async_request(0, request_id, 0, out);
}

fn apiIoWait(request_id: u32, timeout_ticks: u64, out: *ProgramIoInfo) callconv(.c) i32 {
    return r4os_rust_program_async_request(1, request_id, timeout_ticks, out);
}

fn apiIoClose(request_id: u32) callconv(.c) i32 {
    return r4os_rust_program_async_request(2, request_id, 0, null);
}



























































































const AsyncIoPendingCloseResult = enum {
    idle,
    completed,
    deferred,
};



fn apiThreadCreate(entry: RawEntryFn, arg: usize, stack_reserve_bytes: u64, flags: u32, out_thread_id: *u32) callconv(.c) i32 {
    return r4os_rust_program_thread_create_legacy(@intFromPtr(entry), arg, stack_reserve_bytes, flags, out_thread_id, programThreadTaskMain);
}

fn apiThreadCreateHandle(entry: RawEntryFn, arg: u64, stack_reserve_bytes: u64, flags: u32, out_handle: *ProgramJoinHandle) callconv(.c) i32 {
    return r4os_rust_program_thread_create(@intFromPtr(entry), arg, stack_reserve_bytes, flags, out_handle, programThreadTaskMain);
}

fn recordProgramThreadCreateFailure() void {
    _ = r4os_rust_program_thread_failure(0, true);
}

fn threadCreateErrorForTaskFailure(failure: task.CreateFailure) i32 {
    return r4os_rust_program_thread_failure(@intFromEnum(failure), false);
}

fn apiThreadExit(exit_code: i32) callconv(.c) void {
    r4os_rust_program_thread_api_exit(exit_code);
}

// A process-scoped terminal operation, including when called by a worker.
// The caller's normal exit epilogue keeps its execution pin and stack until
// the terminal context switch; the existing reaper owns its peers/resources.
fn apiProgramExit(exit_code: i32, reason: u32) callconv(.c) i32 {
    return r4os_rust_program_thread_program_exit(exit_code, reason);
}

fn exitProgramThread(thread_ctx: *ProgramThread, exit_code: i32, process_reason: ?u8) noreturn {
    r4os_rust_program_thread_exit(thread_ctx, exit_code, process_reason != null, process_reason orelse 0);
}

const ProgramThreadJoinClaim = union(enum) {
    target: *ProgramThread,
    failure: i32,
};









fn apiThreadJoin(thread_id: u32, timeout_ticks: u64, out_exit_code: *i32) callconv(.c) i32 {
    return r4os_rust_program_thread_join(thread_id, null, false, timeout_ticks, out_exit_code);
}

fn apiThreadHandleJoin(handle: *const ProgramJoinHandle, timeout_ticks: u64, out_exit_code: *i32) callconv(.c) i32 {
    return r4os_rust_program_thread_join(0, handle, true, timeout_ticks, out_exit_code);
}







fn apiThreadCurrent() callconv(.c) u32 {
    return @intCast(r4os_rust_program_thread_scalar(0));
}

fn apiThreadCurrentHandle(out: *ProgramJoinHandle) callconv(.c) i32 {
    return r4os_rust_program_thread_current_handle(out);
}

fn notificationOwner() ?*notifications.Owner {
    const owner = r4os_rust_program_peer_notification_owner() orelse return null;
    return @ptrCast(@alignCast(owner));
}
fn apiNotificationCreate(output: *u64) callconv(.c) i32 {
    return r4os_rust_program_peer_notification(0, 0, 0, 0, output);
}
fn apiNotificationQuery(handle: u64, output: *u64) callconv(.c) i32 {
    return r4os_rust_program_peer_notification(1, handle, 0, 0, output);
}
fn apiNotificationNotify(handle: u64, count: u32) callconv(.c) i32 {
    return r4os_rust_program_peer_notification(2, handle, count, 0, null);
}
fn apiNotificationWait(handle: u64, observed: u64, timeout_ticks: u64) callconv(.c) i32 {
    return r4os_rust_program_peer_notification(3, handle, observed, timeout_ticks, null);
}
fn apiNotificationClose(handle: u64) callconv(.c) i32 {
    return r4os_rust_program_peer_notification(4, handle, 0, 0, null);
}

fn localContextOwner() ?*local_context.Owner {
    return r4os_rust_program_data_local_owner();
}
fn cleanupLocalContext(instance: *ProgramInstance) bool {
    return r4os_rust_program_data_local_cleanup(instance);
}
fn apiProgramLocalGet(key: u64, output: *u64) callconv(.c) i32 {
    return r4os_rust_program_data_local(key, 0, output, false);
}
fn apiProgramLocalPublish(key: u64, value: u64, output: *u64) callconv(.c) i32 {
    return r4os_rust_program_data_local(key, value, output, true);
}

fn apiThreadStatus(thread_id: u32, out: *ProgramThreadInfo) callconv(.c) i32 {
    return r4os_rust_program_thread_status(thread_id, null, out, false);
}

fn apiThreadHandleStatus(handle: *const ProgramJoinHandle, out: *ProgramThreadInfo) callconv(.c) i32 {
    return r4os_rust_program_thread_status(0, handle, out, true);
}

fn registerMainThread(instance: *ProgramInstance, program_task: *task.Task) ?*ProgramThread {
    return r4os_rust_program_thread_register_main(instance, program_task);
}

fn allocProgramThreadSlot() ?*ProgramThread {
    return r4os_rust_program_thread_allocate();
}





fn publishProgramThread(thread_ctx: *ProgramThread) bool {
    return r4os_rust_program_thread_control(2, thread_ctx, 0);
}

fn linkProgramThreadLocked(thread_ctx: *ProgramThread) void {
    _ = r4os_rust_program_thread_control(0, thread_ctx, 0);
}

fn unlinkProgramThreadLocked(thread_ctx: *ProgramThread) void {
    _ = r4os_rust_program_thread_control(1, thread_ctx, 0);
}

fn bumpProgramThreadInventoryEpochLocked() void {
    r4os_rust_program_thread_epoch(true);
}

fn bumpProgramThreadInventoryEpoch() void {
    r4os_rust_program_thread_epoch(false);
}

fn markProgramThreadRunning(thread_ctx: *ProgramThread) void {
    _ = r4os_rust_program_thread_control(3, thread_ctx, 0);
}

fn freeProgramThreadMemory(thread_ctx: *ProgramThread) bool {
    return r4os_rust_program_thread_control(6, thread_ctx, 0);
}

fn currentProgramThread() ?*ProgramThread {
    return r4os_rust_program_thread_lookup(4, 0, .{}, null);
}

fn programThreadByIdLocked(id: u32) ?*ProgramThread {
    return r4os_rust_program_thread_lookup(0, id, .{}, null);
}

fn programThreadByIdentityLocked(id: u32, instance_id: u32, instance_generation: u64, thread_generation: u64) ?*ProgramThread {
    return r4os_rust_program_thread_lookup(1, 0, .{ .thread_id = id, .instance_id = instance_id,
        .thread_generation = thread_generation, .instance_generation = instance_generation }, null);
}

fn containsProgramThreadLocked(wanted: *const ProgramThread) bool {
    return r4os_rust_program_thread_lookup(2, 0, .{}, wanted) != null;
}

fn pinProgramThreadById(id: u32, allow_retiring: bool) ?*ProgramThread {
    return r4os_rust_program_thread_pin(id, .{}, false, allow_retiring);
}

fn pinProgramThreadByHandle(handle: ProgramJoinHandle, allow_retiring: bool) ?*ProgramThread {
    return r4os_rust_program_thread_pin(0, handle, true, allow_retiring);
}

fn unpinProgramThread(thread_ctx: *ProgramThread) bool {
    return r4os_rust_program_thread_control(5, thread_ctx, 0);
}

fn markProgramThreadDone(thread_ctx: *ProgramThread, exit_code: i32) void {
    _ = r4os_rust_program_thread_control(4, thread_ctx, exit_code);
}

fn terminateProgramThreads(instance_id: u32, exit_code: i32, skip_task_id: ?u32) void {
    _ = r4os_rust_program_threads_instance(instance_id, true, exit_code, skip_task_id != null, skip_task_id orelse 0);
}

fn terminateProgramThreadsForHandle(handle: ProgramProcessHandle, exit_code: i32, skip_task_id: ?u32) void {
    r4os_rust_program_threads_terminate(handle, exit_code, skip_task_id != null, skip_task_id orelse 0);
}

fn releaseThreadsForInstance(instance_id: u32) bool {
    return r4os_rust_program_threads_instance(instance_id, false, 0, false, 0);
}

fn releaseThreadsForHandle(handle: ProgramProcessHandle) bool {
    return r4os_rust_program_threads_release(handle, false);
}

fn releaseThreadsForHandleReporting(handle: ProgramProcessHandle, report_boot_foreground: bool) bool {
    return r4os_rust_program_threads_release(handle, report_boot_foreground);
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









fn releaseCreatedProgramTask(created: *task.Task) bool {
    return r4os_rust_program_thread_release_created(created);
}

fn releaseProgramTaskGeneration(task_id: u32, task_generation: u64, current_task_id: u32, current_task_generation: u64) bool {
    return r4os_rust_program_thread_release_task(task_id, task_generation, current_task_id, current_task_generation);
}

fn normalizeThreadEntry(instance: *const ProgramInstance, entry: RawEntryFn) ?RawEntryFn {
    var normalized: usize = undefined;
    return if (r4os_rust_program_thread_entry(0, instance, @intFromPtr(entry), &normalized)) @ptrFromInt(normalized) else null;
}

fn currentProgramThreadReady() ?*ProgramThread {
    return r4os_rust_program_thread_lookup(5, 0, .{}, null);
}

fn normalizeThreadStackReserve(instance: *const ProgramInstance, requested: u64) ?u64 {
    var len: u64 = undefined;
    return if (r4os_rust_program_thread_stack_limit(0, instance, requested, &len)) len else null;
}

fn initialThreadStackCommit(instance: *const ProgramInstance, reserve_len: u64) ?u64 {
    var len: u64 = undefined;
    return if (r4os_rust_program_thread_stack_limit(1, instance, reserve_len, &len)) len else null;
}

fn threadInfo(thread_ctx: *const ProgramThread) ProgramThreadInfo {
    var out: ProgramThreadInfo = undefined;
    r4os_rust_program_thread_info(thread_ctx, &out);
    return out;
}

fn apiPrint(text: [*:0]const u8) callconv(.c) void {
    r4os_rust_program_io_print(text);
}

fn apiPutc(ch: u8) callconv(.c) void {
    r4os_rust_program_io_byte(1, ch, false);
}

fn apiOutputTextByte(stream: ConsoleStream, ch: u8) void {
    r4os_rust_program_io_byte(@intFromEnum(stream), ch, false);
}

fn captureOutputByte(ch: u8) void {
    r4os_rust_program_io_byte(0, ch, true);
}

fn apiOutputTextSpan(stream: ConsoleStream, data: []const u8) void {
    r4os_rust_program_io_output(@intFromEnum(stream), .{ .data = data.ptr, .len = data.len });
}

fn mirrorHeadlessShellSmokeOutput(instance: *const ProgramInstance, stream: ConsoleStream, ch: u8) void {
    r4os_rust_program_io_smoke(instance, @intFromEnum(stream), ch);
}

fn apiReadKey() callconv(.c) u8 {
    return @intCast(r4os_rust_program_io_key(true));
}

fn apiReadKeyCodepoint() callconv(.c) u32 {
    return r4os_rust_program_io_key(false);
}

fn apiMemAlloc(size: u32, alignment: u32, out_ptr: *u64) callconv(.c) i32 {
    _ = size; _ = alignment;
    return r4os_rust_program_io_retired(0, out_ptr);
}

fn apiMemFree(ptr_addr: u64, size: u32) callconv(.c) i32 {
    _ = ptr_addr; _ = size;
    return r4os_rust_program_io_retired(1, null);
}

fn apiMemRealloc(old_ptr: u64, old_size: u32, new_size: u32, alignment: u32, out_ptr: *u64) callconv(.c) i32 {
    _ = old_ptr; _ = old_size; _ = new_size; _ = alignment;
    return r4os_rust_program_io_retired(0, out_ptr);
}

fn apiMemStats(out: *ProgramMemoryStats) callconv(.c) i32 {
    return r4os_rust_program_io_retired(2, out);
}

fn apiMemLargestFree() callconv(.c) u64 {
    return r4os_rust_program_io_allocator(4);
}

fn apiMemoryVmReserveProbe(requested_bytes: u64, out: *ProgramVmReserveProbe) callconv(.c) i32 {
    return r4os_rust_program_vm_probe(requested_bytes, out);
}









fn instanceDynamicVmStats(instance: *const ProgramInstance) mem_virt.OwnerStats {
    var out: mem_virt.OwnerStats = undefined;
    r4os_rust_program_info_vm(instance, true, &out);
    return out;
}

fn instanceAllVmStats(instance: *const ProgramInstance) mem_virt.OwnerStats {
    var out: mem_virt.OwnerStats = undefined;
    r4os_rust_program_info_vm(instance, false, &out);
    return out;
}







fn systemCommitLimitAllows(add_bytes: u64) bool {
    return r4os_rust_program_vm_limit(3, null, add_bytes);
}

fn apiVmReserve(size: u64, alignment_raw: u64, flags: u64, out: *ProgramVmRegionInfo) callconv(.c) i32 {
    return r4os_rust_program_vm_reserve(size, alignment_raw, flags, out);
}

fn apiVmCommit(region_id: u32, offset: u64, len: u64, flags: u64) callconv(.c) i32 {
    return r4os_rust_program_vm_commit(region_id, offset, len, flags);
}

fn apiVmDecommit(region_id: u32, offset: u64, len: u64) callconv(.c) i32 {
    return r4os_rust_program_vm_change(0, region_id, offset, len);
}

fn apiVmRelease(region_id: u32) callconv(.c) i32 {
    return r4os_rust_program_vm_change(1, region_id, 0, 0);
}

fn apiVmQuery(region_id: u32, out: *ProgramVmRegionInfo) callconv(.c) i32 {
    return r4os_rust_program_vm_query(region_id, out);
}

const VmRangeLookup = struct {
    code: i32 = VM_OK,
    info: mem_virt.RangeInfo = .{},
};











fn apiAllocatorAlloc(ptr: *anyopaque, len: usize, alignment: ProgramAllocatorAlignment, ret_addr: usize) ?[*]u8 {
    _ = ptr; _ = len; _ = alignment; _ = ret_addr;
    const address = r4os_rust_program_io_allocator(0);
    return if (address == 0) null else @ptrFromInt(address);
}

fn apiAllocatorResize(ptr: *anyopaque, memory: []u8, alignment: ProgramAllocatorAlignment, new_len: usize, ret_addr: usize) bool {
    _ = ptr; _ = memory; _ = alignment; _ = new_len; _ = ret_addr;
    return r4os_rust_program_io_allocator(1) != 0;
}

fn apiAllocatorRemap(ptr: *anyopaque, memory: []u8, alignment: ProgramAllocatorAlignment, new_len: usize, ret_addr: usize) ?[*]u8 {
    _ = ptr; _ = memory; _ = alignment; _ = new_len; _ = ret_addr;
    const address = r4os_rust_program_io_allocator(2);
    return if (address == 0) null else @ptrFromInt(address);
}

fn apiAllocatorFree(ptr: *anyopaque, memory: []u8, alignment: ProgramAllocatorAlignment, ret_addr: usize) void {
    _ = ptr; _ = memory; _ = alignment; _ = ret_addr;
    _ = r4os_rust_program_io_allocator(3);
}

pub fn readConsoleInputByte() ?u8 {
    var out: u32 = undefined;
    return if (r4os_rust_program_console_input_read(&out, true)) @intCast(out) else null;
}

fn readInputByte() ?u8 {
    var out: u32 = undefined;
    return if (r4os_rust_program_console_input_read(&out, true)) @intCast(out) else null;
}

fn readInputCodepoint() ?u32 {
    var out: u32 = undefined;
    return if (r4os_rust_program_console_input_read(&out, false)) @intCast(out) else null;
}

fn guiProducerAllowed(instance: *const ProgramInstance) bool {
    return r4os_rust_program_gui_producer_allowed(instance);
}

fn apiGuiSetFont(font_id: u32) callconv(.c) i32 {
    return r4os_rust_program_gui_font_set(font_id);
}

fn apiGuiFont(instance_id: u32, out: *GuiFontInfo) callconv(.c) i32 {
    return r4os_rust_program_gui_font_get(instance_id, out);
}

fn fillGuiFontInfo(font_id: u32, force_selected: bool, out: *GuiFontInfo) i32 {
    return r4os_rust_program_gui_font_info(font_id, force_selected, out);
}

fn apiProgramRun(path_ptr: [*:0]const u8, args_ptr: [*:0]const u8) callconv(.c) i32 {
    return r4os_rust_program_api_launch(0, path_ptr, args_ptr, 0, 0, null);
}

fn apiProgramLaunch(path_ptr: [*:0]const u8, args_ptr: [*:0]const u8, policy_raw: u32) callconv(.c) i32 {
    return r4os_rust_program_api_launch(0, path_ptr, args_ptr, policy_raw, 0, null);
}

fn apiProgramSpawn(path_ptr: [*:0]const u8, args_ptr: [*:0]const u8, policy_raw: u32) callconv(.c) i32 {
    return r4os_rust_program_api_launch(1, path_ptr, args_ptr, policy_raw, 0, null);
}

fn apiProgramSpawnWithConsoleHost(path_ptr: [*:0]const u8, args_ptr: [*:0]const u8, policy_raw: u32, host_raw: u32) callconv(.c) i32 {
    return r4os_rust_program_api_launch(1, path_ptr, args_ptr, policy_raw, host_raw, null);
}

fn apiProgramSpawnHandle(path_ptr: [*:0]const u8, args_ptr: [*:0]const u8, policy_raw: u32, out: *ProgramProcessHandle) callconv(.c) i32 {
    return r4os_rust_program_api_launch(2, path_ptr, args_ptr, policy_raw, 0, out);
}

fn apiProgramSpawnWithConsoleHostHandle(path_ptr: [*:0]const u8, args_ptr: [*:0]const u8, policy_raw: u32, host_raw: u32, out: *ProgramProcessHandle) callconv(.c) i32 {
    return r4os_rust_program_api_launch(2, path_ptr, args_ptr, policy_raw, host_raw, out);
}

fn apiProgramOpenHandle(id: u32, out: *ProgramProcessHandle) callconv(.c) i32 {
    return r4os_rust_program_process_open(id, out);
}

fn completionInstanceInfo(node: *const ProgramCompletionNode) ProgramInstanceInfo {
    var out: ProgramInstanceInfo = undefined;
    r4os_rust_program_info_completion(node, &out);
    return out;
}

fn apiProgramHandleStatus(handle_ptr: *const ProgramProcessHandle, out: *ProgramInstanceInfo) callconv(.c) i32 {
    return r4os_rust_program_info_handle(handle_ptr, out);
}

fn apiProgramHandleRequestClose(handle_ptr: *const ProgramProcessHandle) callconv(.c) i32 {
    return r4os_rust_program_process_close(handle_ptr);
}

fn apiProgramHandleKill(handle_ptr: *const ProgramProcessHandle) callconv(.c) i32 {
    return r4os_rust_program_process_kill(handle_ptr);
}

fn apiProgramHandleWait(handle_ptr: *const ProgramProcessHandle, timeout_ticks: u64, out: *ProgramProcessCompletion) callconv(.c) i32 {
    return r4os_rust_program_process_wait(handle_ptr, timeout_ticks, out);
}

fn apiProgramHandleReap(handle_ptr: *const ProgramProcessHandle, out: *ProgramProcessCompletion) callconv(.c) i32 {
    return r4os_rust_program_process_reap(handle_ptr, out);
}

fn apiProgramCompletionRead(handle_ptr: *const ProgramProcessHandle, offset: u32, out: [*]u8, capacity: u32, out_read: *u32) callconv(.c) i32 {
    return r4os_rust_program_api_completion_read(handle_ptr, offset, out, capacity, out_read);
}

fn apiProgramRequestClose(id: u32) callconv(.c) i32 {
    return r4os_rust_program_process_legacy(0, id);
}

fn apiProgramShouldClose() callconv(.c) u32 {
    return @intCast(r4os_rust_program_process_legacy(3, 0));
}

fn apiProgramKill(id: u32) callconv(.c) i32 {
    return r4os_rust_program_process_legacy(1, id);
}

fn apiProgramReapInstance(id: u32) callconv(.c) i32 {
    return r4os_rust_program_process_legacy(2, id);
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

const ProgramInventoryCounts = extern struct {
    total: u32 = 0,
    reserved: u32 = 0,
    active: u32 = 0,
    done: u32 = 0,
    retiring: u32 = 0,
};

const ProgramInventoryRegistrySnapshot = extern struct {
    epoch: u64 = 0,
    counts: ProgramInventoryCounts = .{},
    completion_total: u32 = 0,
    peak: u32 = 0,
    create_failures: u64 = 0,
    rollback_count: u64 = 0,
    last_error: i32 = 0,
};

const ProgramInventorySelection = extern struct {
    generation: u64 = 0,
    snapshot: ProgramInstanceSnapshot = .{},
};

const ProgramInventoryScan = extern struct {
    counts: ProgramInventoryCounts = .{},
    returned: u32 = 0,
    has_more: bool = false,
};

const PROGRAM_INVENTORY_PAGE_MAX: usize = @intCast(r4x_api.program_inventory_page_max);

fn programInventorySelectionSiftUp(items: []ProgramInventorySelection, start_index: usize) void {
    r4os_rust_program_inventory_sort(0, items.ptr, items.len, start_index);
}

fn programInventorySelectionSiftDown(items: []ProgramInventorySelection, start_index: usize) void {
    r4os_rust_program_inventory_sort(1, items.ptr, items.len, start_index);
}

fn sortProgramInventorySelections(items: []ProgramInventorySelection) void {
    r4os_rust_program_inventory_sort(2, items.ptr, items.len, 0);
}

fn considerProgramInventorySelection(
    selections: []ProgramInventorySelection,
    selected: *usize,
    generation: u64,
    snapshot: ProgramInstanceSnapshot,
) void {
    r4os_rust_program_inventory_consider(selections.ptr, selections.len, selected, generation, snapshot);
}

fn programInventorySelectionCouldEnter(selections: []const ProgramInventorySelection, selected: usize, generation: u64) bool {
    return r4os_rust_program_inventory_could_enter(selections.ptr, selections.len, selected, generation);
}

fn scanProgramInventoryLocked(
    after_generation: u64,
    selections: []ProgramInventorySelection,
) ProgramInventoryScan {
    var out: ProgramInventoryScan = undefined;
    r4os_rust_program_inventory_scan(after_generation, selections.ptr, selections.len, &out);
    return out;
}

fn tryProgramInventoryRegistrySnapshot() ?ProgramInventoryRegistrySnapshot {
    var out: ProgramInventoryRegistrySnapshot = undefined;
    return if (r4os_rust_program_inventory_registry(&out)) out else null;
}

fn tryProgramInventoryEpoch() ?u64 {
    var out: u64 = undefined;
    return if (r4os_rust_program_inventory_epoch(&out)) out else null;
}







fn fillExecutionInventorySummaryProvider(out: *ProgramInventorySummary) i32 {
    return if (r4os_rust_program_execution_summary(out)) PROGRAM_HANDLE_OK else PROGRAM_HANDLE_ERROR_WOULD_BLOCK;
}

fn apiProgramInventoryBegin(cursor: *ProgramInventoryCursor, out_summary: *ProgramInventorySummary) callconv(.c) i32 {
    return r4os_rust_program_execution_begin(cursor, out_summary);
}

fn inventoryCursorValid(cursor: *const ProgramInventoryCursor) bool {
    return r4os_rust_program_inventory_cursor(cursor);
}

fn initInventoryPage(cursor: *const ProgramInventoryCursor, out_page: *ProgramInventoryPageInfo, kind: u32) void {
    r4os_rust_program_inventory_init_page(cursor, out_page, kind);
}

fn apiProgramInventoryPrograms(cursor: *ProgramInventoryCursor, out: [*]ProgramInstanceSnapshot, capacity: u32, out_page: *ProgramInventoryPageInfo) callconv(.c) i32 {
    return r4os_rust_program_inventory_programs(cursor, out, capacity, out_page);
}





fn apiProgramInventoryTasks(cursor: *ProgramInventoryCursor, out: [*]ProgramTaskSnapshot, capacity: u32, out_page: *ProgramInventoryPageInfo) callconv(.c) i32 {
    return r4os_rust_program_execution_tasks(cursor, out, capacity, out_page);
}







fn apiProgramInventoryThreads(cursor: *ProgramInventoryCursor, out: [*]ProgramThreadSnapshot, capacity: u32, out_page: *ProgramInventoryPageInfo) callconv(.c) i32 {
    return r4os_rust_program_execution_threads(cursor, out, capacity, out_page);
}

fn apiProgramStatus(out: *ProgramStatus) callconv(.c) void {
    r4os_rust_program_info_status(out);
}

fn apiProgramInstance(index: u32, out: *ProgramInstanceInfo) callconv(.c) i32 {
    return r4os_rust_program_info_index(index, out);
}

fn apiProgramSetWindow(id: u32, window_id: i32) callconv(.c) i32 {
    return r4os_rust_program_api_window(id, null, window_id, false);
}

fn apiProgramSetWindowHandle(handle_ptr: *const ProgramProcessHandle, window_id: i32) callconv(.c) i32 {
    return r4os_rust_program_api_window(0, handle_ptr, window_id, true);
}

fn apiProgramSetConsoleHost(id: u32, host_raw: u32) callconv(.c) i32 {
    return r4os_rust_program_api_console_host(id, host_raw);
}

fn apiProgramCurrentConsoleHost() callconv(.c) u32 {
    return @intCast(r4os_rust_program_api_action(1));
}

fn apiProgramRequestDesktop() callconv(.c) i32 {
    return r4os_rust_program_api_action(2);
}

fn apiEnvGet(name_ptr: [*:0]const u8, out_ptr: [*]u8, capacity: u32) callconv(.c) i32 {
    return r4os_rust_program_data_env_api_get(name_ptr, out_ptr, capacity);
}

fn apiEnvSet(name_ptr: [*:0]const u8, value_ptr: [*]const u8, value_len: u32) callconv(.c) i32 {
    return r4os_rust_program_data_env_api_set(name_ptr, value_ptr, value_len);
}

fn apiProgramRequestHostLaunch(path_ptr: [*:0]const u8, args_ptr: [*:0]const u8, policy_raw: u32) callconv(.c) i32 {
    return r4os_rust_program_api_host_request(path_ptr, args_ptr, policy_raw);
}

fn apiProgramTakeHostLaunch(id: u32, out: *ProgramHostLaunchRequest) callconv(.c) i32 {
    return r4os_rust_program_api_host_take(id, out);
}

fn apiProgramWindowId() callconv(.c) i32 {
    return r4os_rust_program_api_action(0);
}

fn apiGuiWindowInfo(out: *GuiWindowInfo) callconv(.c) i32 {
    return r4os_rust_program_gui_window_info(out);
}

fn apiGuiSetWindowInfo(id: u32, info: *const GuiWindowInfo) callconv(.c) i32 {
    return r4os_rust_program_gui_window_set(id, info);
}

fn apiGuiPollEvent(out: *GuiEvent) callconv(.c) i32 {
    return r4os_rust_program_gui_event_poll(out);
}

fn apiGuiPushEvent(id: u32, event: *const GuiEvent) callconv(.c) i32 {
    return r4os_rust_program_gui_event_push(id, event);
}

fn enqueueGuiEvent(gui: *ProgramGuiPayload, event_ptr: *const GuiEvent) i32 {
    return r4os_rust_program_gui_event_enqueue(gui, event_ptr);
}

fn findQueuedGuiMouseMove(gui: *const ProgramGuiPayload) ?usize {
    const index = r4os_rust_program_gui_event_find(gui);
    return if (index != std.math.maxInt(usize)) index else null;
}

fn removeQueuedGuiEvent(gui: *ProgramGuiPayload, remove_index: usize) void {
    r4os_rust_program_gui_event_remove(gui, remove_index);
}

fn guiEventPendingCount(gui: *const ProgramGuiPayload) u32 {
    return r4os_rust_program_gui_event_pending(gui);
}

/// Font ids are a live catalogue view.  A reload therefore sends every
/// running GUI application a lightweight event so it can rebuild its font
/// menu and redraw with a fresh id.  The registry lock makes the short scan
/// safe without holding it across the filesystem read that initiated reload.
fn broadcastGuiFontCatalogChanged() void {
    r4os_rust_program_gui_font_changed();
}

fn guiSetTextForInstance(instance: *ProgramInstance, text: [*:0]const u8) i32 {
    return r4os_rust_program_gui_text(instance, text);
}

fn apiGuiSetText(text: [*:0]const u8) callconv(.c) i32 {
    return r4os_rust_program_gui_simple(0, text, 0, 0);
}

fn apiGuiText(id: u32, out: [*]u8, capacity: u32) callconv(.c) i32 {
    return r4os_rust_program_gui_string(id, out, capacity, false);
}

fn apiGuiRevision(id: u32) callconv(.c) u32 {
    return r4os_rust_program_gui_revision_read(id);
}

fn apiProgramClass(path_ptr: [*:0]const u8, policy_raw: u32) callconv(.c) i32 {
    return r4os_rust_program_api_launch(3, path_ptr, null, policy_raw, 0, null);
}

fn apiServiceInfo(index: u32, out: *ServiceInfo) callconv(.c) i32 {
    return r4os_rust_program_service_info(index,out);
}

fn apiServiceStatus(name_ptr: [*:0]const u8, out: *ServiceInfo) callconv(.c) i32 {
    return r4os_rust_program_service_named(0,name_ptr,out,0,0);
}

fn apiServiceOpen(name_ptr: [*:0]const u8, out: *ServiceInfo) callconv(.c) i32 {
    return r4os_rust_program_service_named(1,name_ptr,out,0,0);
}

fn apiServiceClose(handle: u32) callconv(.c) i32 {
    return r4os_rust_program_service_endpoint(0,handle,0);
}

fn apiServiceCall(handle: u32, op: u16, request_ptr: [*]const u8, request_len: u32, response_header: *ServiceMessageHeader, response_ptr: [*]u8, response_capacity: u32, timeout_ticks: u64) callconv(.c) i32 {
    return r4os_rust_program_async_service(handle, op, request_ptr, request_len, response_header, response_ptr, response_capacity, timeout_ticks, 0, true, null);
}







fn apiServiceEndpointRegister(name_ptr: [*:0]const u8, flags: u32, out: *ServiceInfo) callconv(.c) i32 {
    return r4os_rust_program_service_register(name_ptr,flags,out);
}

fn apiServiceEndpointUnregister(handle: u32) callconv(.c) i32 {
    return r4os_rust_program_service_endpoint(1,handle,0);
}

fn apiServiceEndpointPoll(handle: u32) callconv(.c) i32 {
    return r4os_rust_program_service_endpoint(2,handle,0);
}

// 0.56.19: Blockierendes Endpoint-Warten (API-Version 149).
fn apiServiceEndpointPollWait(handle: u32, timeout_ticks: u64) callconv(.c) i32 {
    return r4os_rust_program_service_endpoint(3,handle,timeout_ticks);
}

fn apiServiceEndpointRecv(handle: u32, header: *ServiceMessageHeader, out_ptr: [*]u8, capacity: u32) callconv(.c) i32 {
    return r4os_rust_program_service_recv(handle,header,out_ptr,capacity);
}

fn apiServiceEndpointReply(handle: u32, request_id: u32, status: i32, payload_ptr: [*]const u8, payload_len: u32) callconv(.c) i32 {
    return r4os_rust_program_service_reply(handle,request_id,status,payload_ptr,payload_len);
}

fn apiServiceDetail(index: u32, out: *ServiceDetail) callconv(.c) i32 {
    return r4os_rust_program_service_detail(index,out);
}

fn apiServiceDetailByName(name_ptr: [*:0]const u8, out: *ServiceDetail) callconv(.c) i32 {
    return r4os_rust_program_service_detail_name(name_ptr,out);
}

fn apiServiceStart(name_ptr: [*:0]const u8, out: *ServiceInfo) callconv(.c) i32 {
    return r4os_rust_program_service_named(2,name_ptr,out,0,0);
}











fn apiServiceStop(name_ptr: [*:0]const u8, out: *ServiceInfo, timeout_ticks: u64) callconv(.c) i32 {
    return r4os_rust_program_service_named(3,name_ptr,out,0,timeout_ticks);
}

/// True when the calling program IS the service's own program or one of its
/// console descendants (0.60.29).
///
/// `SERVMAN RESTART SSHD` issued over an SSH session is exactly this case:
/// SERVMAN is a console child of SSHD, so stopping SSHD tears down the very
/// caller tree that is still inside the syscall.  The old behaviour killed
/// the caller and left a "success" that nobody could observe.  Detecting the
/// relationship here means every caller inherits the rule, not just SERVMAN.


fn apiServiceRestart(name_ptr: [*:0]const u8, out: *ServiceInfo) callconv(.c) i32 {
    return r4os_rust_program_service_named(4,name_ptr,out,0,0);
}

fn apiServiceSetStartMode(name_ptr: [*:0]const u8, start_mode_raw: u32, out: *ServiceInfo) callconv(.c) i32 {
    return r4os_rust_program_service_named(6,name_ptr,out,start_mode_raw,0);
}

fn apiServiceInstall(name_ptr: [*:0]const u8, path_ptr: [*:0]const u8, args_ptr: [*:0]const u8, start_mode_raw: u32, description_ptr: [*:0]const u8, out: *ServiceInfo) callconv(.c) i32 {
    return r4os_rust_program_service_install(name_ptr,path_ptr,args_ptr,start_mode_raw,description_ptr,out);
}

fn apiServiceRemove(name_ptr: [*:0]const u8) callconv(.c) i32 {
    return r4os_rust_program_service_named(7,name_ptr,null,0,0);
}

fn serviceApiStartByName(name: []const u8, out: *ServiceInfo) i32 {
    return r4os_rust_program_service_lifecycle(false,.{.data=name.ptr,.len=name.len},out,0);
}

fn serviceApiStopByName(name: []const u8, out: *ServiceInfo, timeout_ticks: u64) i32 {
    return r4os_rust_program_service_lifecycle(true,.{.data=name.ptr,.len=name.len},out,timeout_ticks);
}















pub fn dumpStatus() void {
    r4os_rust_program_peer_dump();
}

fn apiGuiClear(rgb: u32) callconv(.c) i32 {
    return r4os_rust_program_gui_clear(rgb);
}

fn apiGuiRect(x: i32, y: i32, w: u32, h: u32, rgb: u32) callconv(.c) i32 {
    return r4os_rust_program_gui_rect(x, y, w, h, rgb);
}

fn apiGuiBlit(x: i32, y: i32, w: u32, h: u32, scale: u32, pixels: [*]const u32, pixel_count: u32) callconv(.c) i32 {
    return r4os_rust_program_gui_blit(x, y, w, h, scale, pixels, pixel_count);
}

fn apiGuiBlendAlpha8(x: i32, y: i32, w: u32, h: u32, stride: u32, rgb: u32, alpha: [*]const u8, alpha_len: u32) callconv(.c) i32 {
    return r4os_rust_program_gui_alpha(x, y, w, h, stride, rgb, alpha, alpha_len);
}

fn apiGuiDrawText(x: i32, y: i32, text: [*:0]const u8, fg: u32, bg: u32) callconv(.c) i32 {
    return r4os_rust_program_gui_draw_text(x, y, text, fg, bg, 0, 0, true);
}

fn apiGuiDrawTextEx(x: i32, y: i32, text: [*:0]const u8, fg: u32, bg: u32, font_id: u32, flags: u32) callconv(.c) i32 {
    return r4os_rust_program_gui_draw_text(x, y, text, fg, bg, font_id, flags, false);
}

fn appendGuiTextCommand(instance: *ProgramInstance, x: i32, y: i32, text: [*:0]const u8, fg: u32, bg: u32, requested_font_id: u32, flags: u32) i32 {
    return r4os_rust_program_gui_text_command(instance, x, y, text, fg, bg, requested_font_id, flags);
}

fn apiGuiCommand(id: u32, index: u32, out: *GuiCommand) callconv(.c) i32 {
    return r4os_rust_program_gui_api_command(id, index, out);
}

const GuiFrameCommandLocation = extern struct {
    frame: *const ProgramGuiFramePayload,
    command: *const ProgramGuiCommand,
    resource_base: u64,
    raster_base: u64,
};

fn collectGuiFrameChain(frame: *const ProgramGuiFramePayload, out: *[r4x_api.gui_frame_max_delta_chain]*const ProgramGuiFramePayload) usize {
    return r4os_rust_program_gui_view_chain(frame, out);
}

fn guiFrameLocalCommandAt(frame: *const ProgramGuiFramePayload, index: u64) ?*const ProgramGuiCommand {
    return r4os_rust_program_gui_view_command(frame, index, true);
}

fn guiFrameCommandLocationAt(frame: *const ProgramGuiFramePayload, index: u64) ?GuiFrameCommandLocation {
    var out: GuiFrameCommandLocation = undefined;
    return if (r4os_rust_program_gui_view_location(frame, index, &out)) out else null;
}

fn guiFrameCommandAt(frame: *const ProgramGuiFramePayload, index: u64) ?*const ProgramGuiCommand {
    return r4os_rust_program_gui_view_command(frame, index, false);
}

fn copyGuiFrameLocalResourceBytes(frame: *const ProgramGuiFramePayload, start: u64, out: []u8) usize {
    return r4os_rust_program_gui_view_resources(frame, start, out.ptr, out.len, true);
}

fn copyGuiFrameResourceBytes(frame: *const ProgramGuiFramePayload, start: u64, out: []u8) usize {
    return r4os_rust_program_gui_view_resources(frame, start, out.ptr, out.len, false);
}

fn materializeLegacyGuiCommandAt(location: GuiFrameCommandLocation, out: *GuiCommand) bool {
    return r4os_rust_program_gui_view_legacy(location, out);
}

fn materializeLegacyGuiCommand(frame: *const ProgramGuiFramePayload, command: *const ProgramGuiCommand, out: *GuiCommand) bool {
    return r4os_rust_program_gui_view_legacy(.{ .frame = frame, .command = command, .resource_base = 0, .raster_base = 0 }, out);
}

fn guiRasterReadCount(available: u64, capacity: u32) usize {
    return r4os_rust_program_gui_view_raster_count(available, capacity);
}

fn copyGuiLocalRasterWords(frame: *const ProgramGuiFramePayload, start: u64, out: []u32) usize {
    return r4os_rust_program_gui_view_rasters(frame, start, out.ptr, out.len, true);
}

fn copyGuiRasterWords(frame: *const ProgramGuiFramePayload, start: u64, out: []u32) usize {
    return r4os_rust_program_gui_view_rasters(frame, start, out.ptr, out.len, false);
}

fn apiGuiRasterRead(id: u32, offset: u32, out: [*]u32, capacity: u32) callconv(.c) i32 {
    return r4os_rust_program_gui_api_raster(id, offset, out, capacity);
}

fn apiGuiSetTitle(title: [*:0]const u8) callconv(.c) i32 {
    return r4os_rust_program_gui_simple(1, title, 0, 0);
}

fn apiGuiTitle(id: u32, out: [*]u8, capacity: u32) callconv(.c) i32 {
    return r4os_rust_program_gui_string(id, out, capacity, true);
}

fn apiGuiSetMinSize(w: i32, h: i32) callconv(.c) i32 {
    return r4os_rust_program_gui_simple(2, null, w, h);
}

fn apiGuiMinSize(id: u32, out: *GuiSize) callconv(.c) i32 {
    return r4os_rust_program_gui_min_size(id, out);
}

fn apiGuiPresent() callconv(.c) i32 {
    return r4os_rust_program_gui_simple(3, null, 0, 0);
}

fn fillGuiFrameInfoLocked(gui: *const ProgramGuiPayload, owner: ProgramProcessHandle, out: *GuiFrameInfo) void {
    r4os_rust_program_gui_view_info(gui, owner, out);
}

fn fillEmptyGuiFrameInfo(owner: ProgramProcessHandle, result: i32, out: *GuiFrameInfo) void {
    r4os_rust_program_gui_view_empty(owner, result, out);
}

fn validGuiFrameInfoOutput(out: *const GuiFrameInfo) bool {
    return r4os_rust_program_gui_view_valid(0, out);
}

fn fillCurrentGuiFrameInfoForId(instance_id: u32, result: i32, out: *GuiFrameInfo) void {
    r4os_rust_program_gui_view_current(instance_id, result, out);
}

fn externalGuiFrameCommandWithBase(command: *const ProgramGuiCommand, resource_base: u64) GuiFrameCommand {
    var out: GuiFrameCommand = undefined;
    r4os_rust_program_gui_view_external(command, resource_base, &out);
    return out;
}

fn externalGuiFrameCommand(command: *const ProgramGuiCommand) GuiFrameCommand {
    var out: GuiFrameCommand = undefined;
    r4os_rust_program_gui_view_external(command, 0, &out);
    return out;
}

fn copyGuiFrameLocalCommands(frame: *const ProgramGuiFramePayload, resource_base: u64, out: []GuiFrameCommand) usize {
    return r4os_rust_program_gui_view_commands(frame, resource_base, out.ptr, out.len, true);
}

fn copyGuiFrameCommands(frame: *const ProgramGuiFramePayload, out: []GuiFrameCommand) usize {
    return r4os_rust_program_gui_view_commands(frame, 0, out.ptr, out.len, false);
}

fn apiGuiFrameBegin() callconv(.c) i32 {
    return r4os_rust_program_gui_api_transition(0, null, 0);
}

fn apiGuiFrameBeginDamage(regions_ptr: [*]const DisplayDamageRect, region_count: u32) callconv(.c) i32 {
    return r4os_rust_program_gui_api_transition(1, regions_ptr, region_count);
}

fn apiGuiFrameBeginReplace(regions_ptr: [*]const DisplayDamageRect, region_count: u32) callconv(.c) i32 {
    return r4os_rust_program_gui_api_transition(2, regions_ptr, region_count);
}

fn apiGuiFrameAppend(
    commands_ptr: ?[*]const GuiFrameCommand,
    command_count: u64,
    resources_ptr: ?[*]const u8,
    resource_len: u64,
) callconv(.c) i32 {
    return r4os_rust_program_gui_api_append(commands_ptr, command_count, resources_ptr, resource_len);
}

fn apiGuiFrameCommit() callconv(.c) i32 {
    return r4os_rust_program_gui_api_transition(3, null, 0);
}

fn apiGuiFrameCancel() callconv(.c) i32 {
    return r4os_rust_program_gui_api_transition(4, null, 0);
}

fn apiGuiFrameInfo(handle_ptr: ?*const ProgramProcessHandle, caller_out: *GuiFrameInfo) callconv(.c) i32 {
    return r4os_rust_program_gui_view_api(0, handle_ptr, 0, caller_out);
}

fn apiGuiFrameRead(
    handle_ptr: *const ProgramProcessHandle,
    expected_generation: u64,
    commands_ptr: ?[*]GuiFrameCommand,
    command_capacity: u64,
    resources_ptr: ?[*]u8,
    resource_capacity: u64,
    caller_out: *GuiFrameInfo,
) callconv(.c) i32 {
    return r4os_rust_program_gui_api_read(handle_ptr, expected_generation, commands_ptr, command_capacity, resources_ptr, resource_capacity, caller_out);
}

fn validGuiFrameGenerationInfoOutput(out: *const GuiFrameGenerationInfo) bool {
    return r4os_rust_program_gui_view_valid(1, out);
}

fn guiFrameContainsIndexed8(frame: *const ProgramGuiFramePayload) bool {
    return r4os_rust_program_gui_view_contains(frame, false);
}

fn guiFrameContainsSharedRaster(frame: *const ProgramGuiFramePayload) bool {
    return r4os_rust_program_gui_view_contains(frame, true);
}

fn fillGuiFrameGenerationInfo(gui: *const ProgramGuiPayload, owner: ProgramProcessHandle, frame: *const ProgramGuiFramePayload, out: *GuiFrameGenerationInfo) void {
    r4os_rust_program_gui_view_generation(gui, owner, frame, out);
}

fn validGuiFrameStreamInfoOutput(out: *const GuiFrameStreamInfo) bool {
    return r4os_rust_program_gui_view_valid(2, out);
}

fn fillGuiFrameStreamInfo(gui: *const ProgramGuiPayload, owner: ProgramProcessHandle, out: *GuiFrameStreamInfo) void {
    r4os_rust_program_gui_view_stream(gui, owner, out);
}

fn writeGuiFrameStreamInfoOutput(out: *GuiFrameStreamInfo, caller_version: u32, value: GuiFrameStreamInfo) void {
    r4os_rust_program_gui_view_write_stream(out, caller_version, &value);
}

fn apiGuiFrameStreamInfo(handle_ptr: *const ProgramProcessHandle, caller_out: *GuiFrameStreamInfo) callconv(.c) i32 {
    return r4os_rust_program_gui_view_api(1, handle_ptr, 0, caller_out);
}


fn apiRemoteFrameAcquire() callconv(.c) i32 {
    return r4os_rust_program_domain_remote_frame_acquire();
}
fn apiRemoteFrameRelease() callconv(.c) i32 {
    return r4os_rust_program_domain_remote_frame_release();
}
fn apiRemoteFramePublish(info: *const r4x_api.RemoteFrameInfo, pixels: [*]const u32, count: u32) callconv(.c) i32 {
    return r4os_rust_program_domain_remote_frame_publish(info, pixels, count);
}
fn apiRemoteFramePublishRegions(info: *const r4x_api.RemoteFrameInfo, pixels: [*]const u32, count: u32, regions: [*]const r4x_api.DisplayDamageRect, region_count: u32) callconv(.c) i32 {
    return r4os_rust_program_domain_remote_frame_publish_regions(info, pixels, count, regions, region_count);
}
fn apiRemoteFrameSnapshotAcquire(expected: u32, info: *r4x_api.RemoteFrameInfo, lease: *r4x_api.RemoteFrameLease) callconv(.c) i32 {
    return r4os_rust_program_domain_remote_frame_snapshot_acquire(expected, info, lease);
}
fn apiRemoteFrameSnapshotRelease(lease: *const r4x_api.RemoteFrameLease) callconv(.c) i32 {
    return r4os_rust_program_domain_remote_frame_snapshot_release(lease);
}
fn apiRemoteFrameSourceReset() callconv(.c) i32 {
    return r4os_rust_program_domain_remote_frame_source_reset();
}

fn apiGuiSharedRasterCreate(info: *const GuiSharedRasterCreateInfo, out_handle: *GuiSharedRasterHandle) callconv(.c) i32 {
    return r4os_rust_program_shared_api_create(info, out_handle);
}

fn apiGuiSharedRasterDestroy(handle: *const GuiSharedRasterHandle) callconv(.c) i32 {
    return r4os_rust_program_shared_api_destroy(handle);
}

fn apiGuiSharedRasterMapWrite(handle: *const GuiSharedRasterHandle, out_map: *GuiSharedRasterWriteMap) callconv(.c) i32 {
    return r4os_rust_program_shared_api_write(handle, out_map);
}

fn apiGuiSharedRasterPublish(map: *const GuiSharedRasterWriteMap, out_generation: *u64) callconv(.c) i32 {
    return r4os_rust_program_shared_api_publish(map, out_generation);
}

fn apiGuiSharedRasterAcquire(
    frame_owner_ptr: *const ProgramProcessHandle,
    frame_generation: u64,
    raster_handle_ptr: *const GuiSharedRasterHandle,
    raster_generation: u64,
    out_map: *GuiSharedRasterMap,
) callconv(.c) i32 {
    return r4os_rust_program_shared_api_acquire(frame_owner_ptr, frame_generation, raster_handle_ptr, raster_generation, out_map);
}

fn apiGuiSharedRasterRelease(lease: *const GuiSharedRasterLease) callconv(.c) i32 {
    return r4os_rust_program_shared_api_release(lease);
}

fn apiGfxBufferCreate(input: *const r4x_api.GfxBufferDescriptor, output: *r4x_api.GfxBufferReference) callconv(.c) i32 {
    return r4os_rust_program_domain_gfx_buffer_create(input, output);
}
const gfx_virtual_api = @import("gfx_virtual_api.zig");
fn apiGfxVirtualStart(input: *const gfx_virtual_api.abi.GfxVirtualRequest, output: *gfx_virtual_api.abi.GfxVirtualStatus) callconv(.c) i32 {
    return r4os_rust_program_domain_gfx_virtual_start(input, output);
}
fn apiGfxVirtualQuery(input: *const r4x_api.GfxBufferHandle, output: *gfx_virtual_api.abi.GfxVirtualStatus) callconv(.c) i32 {
    return r4os_rust_program_domain_gfx_virtual_query(input, output);
}
fn apiGfxVirtualClose(input: *const r4x_api.GfxBufferHandle, mode: u32) callconv(.c) i32 {
    return r4os_rust_program_domain_gfx_virtual_close(input, mode);
}
fn apiGfxVirtualWait(input: *const r4x_api.GfxBufferHandle, until: u32, timeout: u64, output: *gfx_virtual_api.abi.GfxVirtualStatus) callconv(.c) i32 {
    return r4os_rust_program_domain_gfx_virtual_wait(input, until, timeout, output);
}
fn apiGfxNativeStart(input: *const r4x_api.GfxNativeAllocation, output: *r4x_api.GfxNativeStatus) callconv(.c) i32 {
    return r4os_rust_program_domain_gfx_native_start(input, output);
}
fn apiGfxNativeQuery(input: *const r4x_api.GfxBufferHandle, output: *r4x_api.GfxNativeStatus) callconv(.c) i32 {
    return r4os_rust_program_domain_gfx_native_query(input, output);
}
fn apiGfxNativeReceive(input: *const r4x_api.GfxBufferHandle, output: *r4x_api.GfxBufferReference) callconv(.c) i32 {
    return r4os_rust_program_domain_gfx_native_receive(input, output);
}
fn apiGfxNativeClose(input: *const r4x_api.GfxBufferHandle) callconv(.c) i32 {
    return r4os_rust_program_domain_gfx_native_close(input);
}
fn apiGfxNativeWait(input: *const r4x_api.GfxBufferHandle, timeout_ticks: u64, output: *r4x_api.GfxNativeStatus) callconv(.c) i32 {
    return r4os_rust_program_domain_gfx_native_wait(input, timeout_ticks, output);
}
fn apiGfxQueueOpen(input: *const gfx_queue_api.abi.GfxQueueConfig, output: *gfx_queue_api.abi.GfxQueueHandle) callconv(.c) i32 {
    return r4os_rust_program_domain_gfx_queue_open(input, output);
}
fn apiGfxAtomicTest(input: *const gfx_output_api.abi.GfxAtomicState, output: *gfx_output_api.abi.GfxAtomicResult) callconv(.c) i32 {
    return r4os_rust_program_domain_gfx_atomic_test(input, output);
}
fn apiGfxAtomicCommit(input: *const gfx_output_api.abi.GfxAtomicState, output: *gfx_output_api.abi.GfxAtomicResult) callconv(.c) i32 {
    return r4os_rust_program_domain_gfx_atomic_commit(input, output);
}
fn apiGfxAtomicSubmit(input: *const gfx_output_api.abi.GfxAtomicState, confirmation_ms: u32, output: *gfx_output_api.abi.GfxModeStatus) callconv(.c) i32 {
    return r4os_rust_program_domain_gfx_atomic_submit(input, confirmation_ms, output);
}
fn apiGfxAtomicTestColor(input: *const gfx_output_api.abi.GfxModeColorRequest, output: *gfx_output_api.abi.GfxAtomicResult) callconv(.c) i32 {
    return r4os_rust_program_domain_gfx_atomic_test_color(input, output);
}
fn apiGfxAtomicSubmitColor(input: *const gfx_output_api.abi.GfxModeColorRequest, confirmation_ms: u32, output: *gfx_output_api.abi.GfxModeStatus) callconv(.c) i32 {
    return r4os_rust_program_domain_gfx_atomic_submit_color(input, confirmation_ms, output);
}
fn apiGfxRefreshRequest(input: *const gfx_output_api.abi.GfxRefreshRequest, output: *gfx_output_api.abi.GfxRefreshRequest) callconv(.c) i32 {
    return r4os_rust_program_domain_gfx_refresh_request(input, output);
}
fn apiGfxPowerRequest(input: *const gfx_output_api.abi.GfxPowerRequest, output: *gfx_output_api.abi.GfxPowerRequest) callconv(.c) i32 {
    return r4os_rust_program_domain_gfx_power_request(input, output);
}
fn apiDisplayCursorSubmit(input: *const @import("r4os_kernel_contract").DisplayCursorRequest, output: *@import("r4os_kernel_contract").DisplayCursorStatus) callconv(.c) i32 {
    return r4os_rust_program_domain_display_cursor_submit(input, output);
}
fn apiGfxAtomicResolve(ticket: u64, action: u32, output: *gfx_output_api.abi.GfxModeStatus) callconv(.c) i32 {
    return r4os_rust_program_domain_gfx_atomic_resolve(ticket, action, output);
}
fn apiGfxQueueClose(input: *const gfx_queue_api.abi.GfxQueueHandle) callconv(.c) i32 {
    return r4os_rust_program_domain_gfx_queue_close(input);
}
fn apiGfxQueueSubmit(queue: *const gfx_queue_api.abi.GfxQueueHandle, input: *const gfx_queue_api.abi.GfxSubmission, output: *gfx_queue_api.abi.GfxFenceStatus) callconv(.c) i32 {
    return r4os_rust_program_domain_gfx_queue_submit(queue, input, output);
}
fn apiGfxQueueSubmitRenderList(queue: *const gfx_queue_api.abi.GfxQueueHandle, input: *const gfx_queue_api.abi.GfxSubmission, list: *const gfx_queue_api.abi.GfxRenderList, output: *gfx_queue_api.abi.GfxFenceStatus) callconv(.c) i32 {
    return r4os_rust_program_domain_gfx_queue_submit_render_list(queue, input, list, output);
}
fn apiGfxQueueSubmitRenderGridList(queue: *const gfx_queue_api.abi.GfxQueueHandle, input: *const gfx_queue_api.abi.GfxSubmission, list: *const gfx_queue_api.abi.GfxRenderGridList, output: *gfx_queue_api.abi.GfxFenceStatus) callconv(.c) i32 {
    return r4os_rust_program_domain_gfx_queue_submit_render_grid_list(queue, input, list, output);
}
fn apiGfxQueueSubmitOutput(queue: *const gfx_queue_api.abi.GfxQueueHandle, input: *const gfx_queue_api.abi.GfxSubmission, target: *const gfx_queue_api.abi.GfxOutputTarget, output: *gfx_queue_api.abi.GfxFenceStatus) callconv(.c) i32 {
    return r4os_rust_program_domain_gfx_queue_submit_output(queue, input, target, output);
}
fn apiGfxQueueSubmitRenderColorList(queue: *const gfx_queue_api.abi.GfxQueueHandle, input: *const gfx_queue_api.abi.GfxSubmission, list: *const gfx_queue_api.abi.GfxRenderColorList, output: *gfx_queue_api.abi.GfxFenceStatus) callconv(.c) i32 {
    return r4os_rust_program_domain_gfx_queue_submit_render_color_list(queue, input, list, output);
}
fn apiGfxQueueSubmitNative(queue: *const gfx_queue_api.abi.GfxQueueHandle, input: *const gfx_queue_api.abi.GfxSubmission, native: *const gfx_queue_api.abi.GfxNativeSubmission, output: *gfx_queue_api.abi.GfxFenceStatus) callconv(.c) i32 {
    return r4os_rust_program_domain_gfx_queue_submit_native(queue, input, native, output);
}
fn apiGfxQueueSubmitRenderColorGridList(queue: *const gfx_queue_api.abi.GfxQueueHandle, input: *const gfx_queue_api.abi.GfxSubmission, list: *const gfx_queue_api.abi.GfxRenderColorGridList, output: *gfx_queue_api.abi.GfxFenceStatus) callconv(.c) i32 {
    return r4os_rust_program_domain_gfx_queue_submit_render_color_grid_list(queue, input, list, output);
}
fn apiGfxFenceCancel(input: *const gfx_queue_api.abi.GfxFence) callconv(.c) i32 {
    return r4os_rust_program_domain_gfx_fence_cancel(input);
}
fn apiGfxFenceRelease(input: *const gfx_queue_api.abi.GfxFence) callconv(.c) i32 {
    return r4os_rust_program_domain_gfx_fence_release(input);
}
fn apiGfxBufferDescribe(input: *const r4x_api.GfxBufferHandle, output: *r4x_api.GfxBufferDescriptor) callconv(.c) i32 {
    return r4os_rust_program_domain_gfx_buffer_describe(input, output);
}
fn apiGfxBufferImport(input: *const r4x_api.GfxBufferHandle, output: *r4x_api.GfxBufferReference) callconv(.c) i32 {
    return r4os_rust_program_domain_gfx_buffer_import(input, output);
}
fn apiGfxBufferRelease(input: *const r4x_api.GfxBufferHandle) callconv(.c) i32 {
    return r4os_rust_program_domain_gfx_buffer_release(input);
}
fn apiGfxBufferMap(input: *const r4x_api.GfxBufferHandle, access: u32, offset: u64, bytes: u64, output: *r4x_api.GfxBufferMap) callconv(.c) i32 {
    return r4os_rust_program_domain_gfx_buffer_map(input, access, offset, bytes, output);
}
fn apiGfxBufferUnmap(input: *const r4x_api.GfxBufferHandle) callconv(.c) i32 {
    return r4os_rust_program_domain_gfx_buffer_unmap(input);
}
fn apiGfxBufferMapPersistent(input: *const r4x_api.GfxBufferHandle, access: u32, offset: u64, bytes: u64, output: *r4x_api.GfxBufferMap) callconv(.c) i32 {
    return r4os_rust_program_domain_gfx_buffer_map_persistent(input, access, offset, bytes, output);
}
fn apiGfxBufferExportRaster(input: *const GuiSharedRasterLease, output: *r4x_api.GfxBufferReference) callconv(.c) i32 {
    return r4os_rust_program_shared_api_export(input, output);
}
fn gfxBufferExportRaster(consumer: ProgramProcessHandle, input: *const GuiSharedRasterLease, output: *r4x_api.GfxBufferReference) i32 {
    return r4os_rust_program_shared_export(consumer, input, output);
}

fn apiGuiFrameGenerationInfo(handle_ptr: *const ProgramProcessHandle, generation: u64, caller_out: *GuiFrameGenerationInfo) callconv(.c) i32 {
    return r4os_rust_program_gui_view_api(2, handle_ptr, generation, caller_out);
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
    caller_out: *GuiFrameGenerationInfo,
) callconv(.c) i32 {
    return r4os_rust_program_gui_api_generation_read(handle_ptr, generation, commands_ptr, command_capacity, resources_ptr, resource_capacity, regions_ptr, region_capacity, caller_out);
}

fn consoleTranscript(instance: *ProgramInstance) ?*ProgramConsoleTranscriptPayload {
    return r4os_rust_program_console_get(instance);
}

fn consoleTranscriptConst(instance: *const ProgramInstance) ?*const ProgramConsoleTranscriptPayload {
    return r4os_rust_program_console_get(instance);
}

fn readConsoleTranscript(transcript: *const ProgramConsoleTranscriptPayload, offset: usize, out: []u8) usize {
    return r4os_rust_program_console_read(transcript, offset, out.ptr, out.len, false);
}

fn removeFirstConsoleTranscriptSegment(transcript: *ProgramConsoleTranscriptPayload, active: bool) bool {
    return r4os_rust_program_console_transcript(3, transcript, 0, active);
}

fn dropConsoleTranscriptPrefix(instance: *ProgramInstance, byte_count: usize) void {
    _ = r4os_rust_program_console_instance(0, instance, byte_count);
}

fn compactConsoleTranscript(instance: *ProgramInstance) bool {
    return r4os_rust_program_console_instance(1, instance, 0);
}

fn ensureConsoleTranscriptSegmentCapacity(instance: *ProgramInstance) bool {
    return r4os_rust_program_console_instance(2, instance, 0);
}

fn appendConsoleTranscriptReference(instance: *ProgramInstance, payload: *ProgramConsoleOutputPayload, sequence: u64, capture: bool) bool {
    return r4os_rust_program_console_append(instance, payload, sequence, capture);
}

fn ensureConsoleWriter(instance: *ProgramInstance) ?*ProgramConsoleOutputPayload {
    return r4os_rust_program_console_writer(instance);
}

fn appendConsoleSourceByte(instance: *ProgramInstance, ch: u8) ?struct { payload: *ProgramConsoleOutputPayload, sequence: u64 } {
    var out: NativeConsoleSourceByte = undefined;
    return if (r4os_rust_program_console_source(instance, ch, &out)) .{ .payload = out.payload, .sequence = out.sequence } else null;
}

fn readConsoleTranscriptTail(transcript: *const ProgramConsoleTranscriptPayload, out: []u8) usize {
    return r4os_rust_program_console_read(transcript, 0, out.ptr, out.len, true);
}

fn apiConsoleOutput(id: u32, out: [*]u8, capacity: u32) callconv(.c) i32 {
    return r4os_rust_program_console_api_output(id, out, capacity);
}

fn apiConsoleRevision(id: u32) callconv(.c) u32 {
    return r4os_rust_program_console_api_revision(id);
}

fn apiConsoleState(id: u32, out: *ConsoleState) callconv(.c) i32 {
    return r4os_rust_program_console_api_state(id, out);
}

fn apiConsoleSetMetrics(id: u32, cols: u32, rows: u32) callconv(.c) i32 {
    return r4os_rust_program_console_api_metrics(id, cols, rows);
}

fn apiConsolePushKey(id: u32, key: u8) callconv(.c) i32 {
    return r4os_rust_program_console_input_api_push(id, &.{key}, 1, true);
}

fn apiConsolePushInput(id: u32, data: [*]const u8, length: u32) callconv(.c) i32 {
    return r4os_rust_program_console_input_api_push(id, data, length, false);
}

fn pushConsoleInput(instance: *ProgramInstance, data: []const u8) usize {
    return r4os_rust_program_console_input_push(instance, .{ .data = data.ptr, .len = data.len });
}

fn apiConsoleWrite(stream_raw: u32, data: [*]const u8, len: u32) callconv(.c) i32 {
    return r4os_rust_program_io_write(stream_raw, data, len, true);
}

fn apiConsoleRead(out: [*]u8, capacity: u32) callconv(.c) i32 {
    return r4os_rust_program_console_input_api_read(out, capacity);
}

const ConsoleInputWaitContext = struct {
    console: *ProgramConsolePayload,
    source: *ProgramInstance,
    last_generation: u64,
};







fn apiConsoleInputWait(last_generation: u64, timeout_ticks: u64, out_generation: *u64) callconv(.c) i32 {
    return r4os_rust_program_console_input_wait(last_generation, timeout_ticks, out_generation);
}

fn signalConsoleInputPayload(console: *ProgramConsolePayload) void {
    r4os_rust_program_console_input_signal_payload(console);
}

fn signalConsoleInputForInstance(source: *ProgramInstance) void {
    r4os_rust_program_console_input_signal(source);
}

fn signalConsoleInputForHandle(handle: ProgramProcessHandle) void {
    r4os_rust_program_console_input_signal_handle(handle);
}

fn kprintOutputHook(ch: u8) callconv(.c) bool {
    return r4os_rust_program_console_print_hook(ch);
}

fn keyboardInputHook() callconv(.c) u8 {
    return r4os_rust_program_console_keyboard_hook();
}

fn currentConsoleInstance() ?*ProgramInstance {
    return r4os_rust_program_console_input_current();
}

fn consoleTargetById(id: u32) ?*ProgramInstance {
    return r4os_rust_program_console_input_target(id, .{}, true);
}

fn consoleTargetByHandle(handle: ProgramProcessHandle) ?*ProgramInstance {
    return r4os_rust_program_console_input_target(0, handle, false);
}

fn inheritedConsoleTargetId(app_class: AppClass) u32 {
    return r4os_rust_program_console_input_inherited(@intFromEnum(app_class));
}

fn popInput(instance: *ProgramInstance) ?u8 {
    var out: u8 = undefined;
    return if (r4os_rust_program_console_input_pop(instance, &out)) out else null;
}

fn routeCurrentConsoleOutput(stream: ConsoleStream, ch: u8) bool {
    return r4os_rust_program_console_route(@intFromEnum(stream), .{ .data = &.{ch}, .len = 1 }, false);
}

fn routeCurrentConsoleOutputBatch(stream: ConsoleStream, data: []const u8, normalize_text: bool) bool {
    return r4os_rust_program_console_route(@intFromEnum(stream), .{ .data = data.ptr, .len = data.len }, normalize_text);
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
    r4os_rust_program_console_emit(source, target, @intFromEnum(stream), ch, inherited, source_changed, target_changed);
}

fn backspaceConsoleTranscriptState(instance: *ProgramInstance) void {
    _ = r4os_rust_program_console_instance(3, instance, 0);
}

pub fn isHostedConsoleContext() bool {
    return r4os_rust_program_console_host(0, 0, 0, null) != 0;
}

pub fn clearHostedConsoleSurface() bool {
    return r4os_rust_program_console_host(1, 0, 0, null) != 0;
}

pub fn setHostedConsoleColors(fg: u32, bg: u32) bool {
    return r4os_rust_program_console_host(2, fg, bg, null) != 0;
}

pub fn currentConsoleState() ?ConsoleState {
    var out: ConsoleState = undefined;
    return if (r4os_rust_program_console_host(3, 0, 0, &out) != 0) out else null;
}

pub fn currentConsoleHostKind() ConsoleHostKind {
    return @enumFromInt(@as(u32, @intCast(r4os_rust_program_console_host(4, 0, 0, null))));
}

pub fn requestDesktopFromHostedConsole() bool {
    return r4os_rust_program_console_host(5, 0, 0, null) != 0;
}

fn clearConsoleOutput(instance: *ProgramInstance) void {
    _ = r4os_rust_program_console_instance(4, instance, 0);
}

fn clearConsoleTranscriptState(instance: *ProgramInstance) void {
    _ = r4os_rust_program_console_instance(5, instance, 0);
}

fn mirrorConsoleControl(instance: *const ProgramInstance, ch: u8) void {
    _ = r4os_rust_program_console_instance(15, @constCast(instance), ch);
}

fn mirrorConsoleByte(instance: *const ProgramInstance, ch: u8) void {
    _ = r4os_rust_program_console_instance(15, @constCast(instance), ch);
}

fn isRawFullscreenPresenter(instance: *const ProgramInstance) bool {
    return r4os_rust_program_console_instance(6, @constCast(instance), 0);
}

fn bumpConsoleStreamCounter(instance: *ProgramInstance, stream: ConsoleStream) void {
    _ = r4os_rust_program_console_instance(7, instance, @intFromEnum(stream));
}

fn updateConsoleInputPending(instance: *ProgramInstance) void {
    _ = r4os_rust_program_console_instance(8, instance, 0);
}

fn updateConsoleInputPendingLocked(console: *ProgramConsolePayload) void {
    r4os_rust_program_console_pending(console);
}

fn refreshConsoleState(instance: *ProgramInstance) void {
    _ = r4os_rust_program_console_instance(9, instance, 0);
}

fn countConsoleOutputLines(transcript: *const ProgramConsoleTranscriptPayload, configured_cols: u32) u32 {
    return r4os_rust_program_console_lines(transcript, configured_cols);
}

fn consolePrintable(ch: u8) bool {
    return r4os_rust_program_console_printable(ch);
}

fn parseConsoleStream(raw: u32) ?ConsoleStream {
    return if (r4os_rust_program_console_stream(raw)) @enumFromInt(raw) else null;
}

fn advanceConsoleCursor(instance: *ProgramInstance, ch: u8) void {
    _ = r4os_rust_program_console_instance(10, instance, ch);
}

fn advanceConsoleRow(instance: *ProgramInstance) void {
    _ = r4os_rust_program_console_instance(11, instance, 0);
}

fn clampConsoleCursor(instance: *ProgramInstance) void {
    _ = r4os_rust_program_console_instance(12, instance, 0);
}

fn clampConsoleMetric(value: u32, min: u32, max: u32) u32 {
    return r4os_rust_program_console_metric(value, min, max);
}

fn bumpConsoleRevision(instance: *ProgramInstance) void {
    _ = r4os_rust_program_console_instance(13, instance, 0);
}

fn bumpConsoleOutputRevision(instance: *ProgramInstance, present: bool) void {
    _ = r4os_rust_program_console_instance(14, instance, @intFromBool(present));
}

fn appendGuiCommand(instance: *ProgramInstance, command: ProgramGuiCommand) i32 {
    return r4os_rust_program_gui_append_command(instance, &command);
}

fn bumpGuiRevision(instance: *ProgramInstance) void {
    r4os_rust_program_gui_revision(instance);
}

fn resolveAppClass(policy: LaunchPolicy, flags: u32) AppClass {
    return @enumFromInt(r4os_rust_program_util_enum(0, @intFromEnum(policy), flags));
}

fn appClassFlagCount(flags: u32) u8 {
    return @intCast(r4os_rust_program_util_enum(1, 0, flags));
}

fn classifyProgramFile(file: ProgramFile, policy: LaunchPolicy) ?AppClass {
    const native_file = nativeProgramLaunchFile(file);
    var out: u8 = undefined;
    return if (r4os_rust_program_launch_classify(&native_file, @intFromEnum(policy), &out)) @enumFromInt(out) else null;
}

fn parseLaunchPolicy(value: u32) ?LaunchPolicy {
    const raw = r4os_rust_program_util_enum(2, value, 0);
    return if (raw != 0xFFFF_FFFF) @enumFromInt(raw) else null;
}

fn roleName(role: InstanceRole) []const u8 {
    const label = r4os_rust_program_util_label(0, @intCast(@intFromEnum(role)));
    return label.data[0..label.len];
}

fn appClassName(app_class: AppClass) []const u8 {
    const label = r4os_rust_program_util_label(1, @intCast(@intFromEnum(app_class)));
    return label.data[0..label.len];
}

fn memoryProfileName(profile: MemoryProfile) []const u8 {
    const label = r4os_rust_program_util_label(2, @intCast(@intFromEnum(profile)));
    return label.data[0..label.len];
}

fn instanceStateName(state: InstanceState) []const u8 {
    const label = r4os_rust_program_util_label(3, @intCast(@intFromEnum(state)));
    return label.data[0..label.len];
}

fn consoleHostName(host: ConsoleHostKind) []const u8 {
    const label = r4os_rust_program_util_label(4, @intCast(@intFromEnum(host)));
    return label.data[0..label.len];
}

fn parseConsoleHost(value: u32) ?ConsoleHostKind {
    const raw = r4os_rust_program_util_enum(3, value, 0);
    return if (raw != 0xFFFF_FFFF) @enumFromInt(raw) else null;
}



fn rollbackReservedProgramInstance(reservation: *const ProgramInstanceReservation) void {
    cancelProgramInstanceReservation(reservation);
}



fn prepareR4XStartContext(instance: *ProgramInstance) void {
    r4os_rust_program_entry_context(instance);
}



fn cStringLen(bytes: []const u8) u64 {
    return r4os_rust_program_entry_cstring(.{ .data = bytes.ptr, .len = bytes.len });
}





fn r4xstartR4SysWrite(data: [*]const u8, len: u32) callconv(.c) i32 {
    return r4os_rust_program_io_write(1, data, len, false);
}

fn r4xstartR4SysPutc(ch: u8) callconv(.c) void {
    r4os_rust_program_io_byte(1, ch, false);
}

fn appendExitHistoryLocked(handle: ProgramProcessHandle, instance: *const ProgramInstance, exit_code: i32, exit_reason: u8, start_tick: u64, finish_tick: u64) u64 {
    return r4os_rust_program_history_append(handle, instance, exit_code, exit_reason, start_tick, finish_tick);
}

const ProgramExitCommitResult = enum {
    committed,
    already_exiting,
    retry,
};

fn commitProgramExit(handle: ProgramProcessHandle, exit_code: i32, requested_reason: u8) ProgramExitCommitResult {
    return @enumFromInt(r4os_rust_program_exit_commit(handle, exit_code, requested_reason));
}

fn beginProgramExit(handle: ProgramProcessHandle, exit_code: i32, requested_reason: u8) bool {
    return r4os_rust_program_exit_begin(handle, exit_code, requested_reason);
}

fn orphanOwnedProgramCompletions(owner_handle: ProgramProcessHandle) void {
    _ = r4os_rust_program_completion_access(3, owner_handle, null);
}

fn abandonProgramCompletion(handle: ProgramProcessHandle) void {
    _ = r4os_rust_program_completion_access(4, handle, null);
}

fn markInstanceDone(handle: ProgramProcessHandle, exit_code: i32) void {
    r4os_rust_program_exit_mark(handle, exit_code);
}

fn releaseInstance(id: u32) void {
    r4os_rust_program_process_release(id);
}

fn finishInstance(id: u32) i32 {
    return r4os_rust_program_process_finish(id);
}

fn consoleClientDescendsFromLocked(slot: *const ProgramRegistrySlot, host: ProgramProcessHandle) bool {
    return r4os_rust_program_process_descends(slot, host);
}

fn killConsoleClients(host: ProgramProcessHandle) void {
    r4os_rust_program_process_clients(0, host);
}

fn requestConsoleClientsClose(host: ProgramProcessHandle) void {
    r4os_rust_program_process_clients(1, host);
}

fn reapFinishedInstances() void {
    r4os_rust_program_process_signal();
}

fn reapHostedConsoleInstancesForPressure() void {
    r4os_rust_program_process_pressure();
}

fn isHostedConsoleInstance(instance: *const ProgramInstance) bool {
    return r4os_rust_program_process_hosted(instance);
}

fn completionFromNode(node: *const ProgramCompletionNode) ProgramProcessCompletion {
    var out: ProgramProcessCompletion = undefined;
    r4os_rust_program_completion_copy(node, &out);
    return out;
}

fn programCompletionIsReady(handle: ProgramProcessHandle) bool {
    return r4os_rust_program_completion_access(0, handle, null) != 0;
}

fn readProgramCompletion(handle: ProgramProcessHandle, out: *ProgramProcessCompletion) i32 {
    return r4os_rust_program_completion_access(1, handle, out);
}

fn consumeProgramCompletion(handle: ProgramProcessHandle, out: ?*ProgramProcessCompletion) i32 {
    return r4os_rust_program_completion_access(2, handle, out);
}

const ProgramRetireResult = enum {
    idle,
    completed,
    deferred,
};

fn deferProgramRetire(handle: ProgramProcessHandle) ProgramRetireResult {
    return @enumFromInt(r4os_rust_program_retire_defer(handle));
}

fn advanceProgramRetirePhase(
    slot: *ProgramRegistrySlot,
    handle: ProgramProcessHandle,
    completion: *ProgramCompletionNode,
    expected: ProgramRetirePhase,
    next: ProgramRetirePhase,
) bool {
    return r4os_rust_program_retire_advance(slot, handle, completion, @intFromEnum(expected), @intFromEnum(next));
}

fn reportBootForegroundRetireStage(enabled: bool, detail: []const u8) void {
    r4os_rust_program_entry_report(enabled, .{ .data = detail.ptr, .len = detail.len }, false);
}

fn reportBootLaunchStage(enabled: bool, detail: []const u8) void {
    r4os_rust_program_entry_report(enabled, .{ .data = detail.ptr, .len = detail.len }, true);
}

fn retireProgramSlot(slot: *ProgramRegistrySlot) ProgramRetireResult {
    return @enumFromInt(r4os_rust_program_reap_slot(slot));
}

pub fn runProgramReaperForTest() bool {
    return r4os_rust_program_reaper_once();
}

fn programReaperMain() callconv(.c) void {
    r4os_rust_program_reaper_main();
}

fn activeInstanceCount() u8 {
    return r4os_rust_program_info_count();
}

fn instanceInfo(instance: *const ProgramInstance) ProgramInstanceInfo {
    var out: ProgramInstanceInfo = undefined;
    r4os_rust_program_info_instance(instance, &out);
    return out;
}

fn putSignedDec(value: i32) void {
    r4os_rust_program_util_signed(value);
}

fn instanceState(instance: *const ProgramInstance) InstanceState {
    return @enumFromInt(r4os_rust_program_info_state(instance));
}

/// Pfad der eigenen Moduldatei in einen caller-owned Buffer. Rueckgabe ist
/// die Pfadlaenge oder negativ: -1 Argumente, -2 kein Programmkontext,
/// -3 Buffer zu klein (sichtbar statt still trunciert), -4 Pfad unbekannt.
fn apiProgramModulePath(out_ptr: [*]u8, out_len: u32) callconv(.c) i32 {
    return r4os_rust_program_api_module_path(out_ptr, out_len);
}

fn apiBootReady() callconv(.c) i32 {
    return r4os_rust_program_api_action(3);
}

fn currentInstance() ?*ProgramInstance {
    return r4os_rust_program_context_current(false);
}

// IRQ/exception-safe current-owner lookup.  Program registry chunks keep
// ProgramInstance addresses stable, and teardown clears task/request records
// before reclaiming their owner slot.  This helper therefore needs neither a
// registry pin nor a sleepable mutex and must remain free of waits/yields.
pub fn diagnosticMemoryOwnerId() u64 {
    return r4os_rust_program_context_memory_owner();
}

fn currentExecutionInstanceNoRegistry() ?*ProgramInstance {
    return r4os_rust_program_context_current(true);
}

fn currentProgramHandle() ?ProgramProcessHandle {
    var out: ProgramProcessHandle = undefined;
    return if (r4os_rust_program_context_handle(&out)) out else null;
}

fn resolveStorageOwner() ?@import("../storage/access_runtime.zig").Owner {
    var owner: NativeAsyncStreamOwner = undefined;
    if (!r4os_rust_program_async_stream_owner(&owner)) return null;
    return .{ .task = owner.task, .task_generation = owner.task_generation, .program = owner.program, .program_generation = owner.program_generation };
}

fn resolveR4SysStreamOwner() ?r4api.r4sys.StreamOwner {
    var owner: NativeAsyncStreamOwner = undefined;
    if (!r4os_rust_program_async_stream_owner(&owner)) return null;
    return .{ .kind = .program, .id = owner.task, .generation = owner.task_generation, .program_id = owner.program, .program_generation = owner.program_generation };
}

pub fn isPreemptibleInstructionPointer(rip: u64) bool {
    return r4os_rust_program_peer_preemptible(rip);
}

fn instructionPointerInImportedLibrary(instance: *const ProgramInstance, rip: u64) bool {
    return r4os_rust_program_thread_entry(1, instance, @intCast(rip), null);
}

pub fn crashContextForInstructionPointer(rip: u64) crash.ExecutionContext {
    var out: crash.ExecutionContext = undefined;
    r4os_rust_program_peer_crash(null, 0, rip, true, &out);
    return out;
}

fn instructionPointerInInstance(instance: *const ProgramInstance, rip: u64) bool {
    return r4os_rust_program_peer_contains(instance, rip);
}

fn crashContextFromInstance(instance: *const ProgramInstance, task_id: u32, rip: u64) crash.ExecutionContext {
    var out: crash.ExecutionContext = undefined;
    r4os_rust_program_peer_crash(instance, task_id, rip, false, &out);
    return out;
}

fn instanceById(id: u32) ?*ProgramInstance {
    return r4os_rust_program_info_by_id(id);
}

fn copyArgsInto(instance: *ProgramInstance, args: []const u8) void {
    r4os_rust_program_data_arguments(instance, .{ .data = args.ptr, .len = args.len });
}

const EnvironmentEntry = struct {
    start: usize,
    value_start: usize,
    end: usize,
};

fn inheritEnvironmentInto(instance: *ProgramInstance, parent: ?*const ProgramInstance) void {
    r4os_rust_program_data_inherit(instance, parent);
}

fn environmentGet(instance: *const ProgramInstance, name: []const u8, out_ptr: [*]u8, capacity: u32) i32 {
    return r4os_rust_program_data_env_get(instance, .{ .data = name.ptr, .len = name.len }, out_ptr, capacity);
}

fn environmentSet(instance: *ProgramInstance, name: []const u8, value: []const u8) i32 {
    return r4os_rust_program_data_env_set(instance, .{ .data = name.ptr, .len = name.len }, .{ .data = value.ptr, .len = value.len });
}





fn environmentNameValid(name: []const u8) bool {
    return r4os_rust_program_data_env_name(.{ .data = name.ptr, .len = name.len });
}

fn copySliceZ(out: []u8, value: []const u8) void {
    r4os_rust_program_data_copy_slice(out.ptr, out.len, .{ .data = value.ptr, .len = value.len });
}

// CWD is a view of the process path context, never a second environment value.
fn workingDirectoryGet(instance: *const ProgramInstance, out: [*]u8, capacity: u32) i32 {
    return r4os_rust_program_data_cwd_get(instance, out, capacity);
}

fn workingDirectorySet(instance: *ProgramInstance, value: []const u8) i32 {
    return r4os_rust_program_data_cwd_set(instance, .{ .data = value.ptr, .len = value.len });
}

fn copyWorkingDirectoryInto(instance: *ProgramInstance, parent: ?*const ProgramInstance, d: *drive.Drive) bool {
    return r4os_rust_program_data_cwd_copy(instance, parent, d);
}

fn markDisplayUsed() void {
    _ = r4os_rust_program_api_action(4);
}

fn noteDisplayUsed() void {
    _ = r4os_rust_program_api_action(5);
}

const ApiTarget = r4api.r4sys.Target;

fn resolveApiTarget(raw: []const u8, out: *[MAX_API_PATH]u8) ?ApiTarget {
    var target: NativeProgramApiTarget = undefined;
    if (!r4os_rust_program_api_target(.{ .data = raw.ptr, .len = raw.len }, out, &target)) return null;
    return .{ .drive_ref = target.drive, .path = target.path.data[0..target.path.len] };
}

fn buildApiPath(cwd: []const u8, arg: []const u8, out: *[MAX_API_PATH]u8) ?[]const u8 {
    var len: usize = undefined;
    return if (r4os_rust_program_api_build(.{ .data = cwd.ptr, .len = cwd.len }, .{ .data = arg.ptr, .len = arg.len }, out, &len)) out[0..len] else null;
}

/// Produces one volume-relative spelling for R4SYS ownership and ancestor
/// checks. The filesystem may accept '.', '..' and mixed separators, but
/// retaining those raw bytes let the same directory evade StreamSlot guards.
fn canonicalizeApiVolumePath(input: []const u8, out: *[MAX_API_PATH]u8) ?[]const u8 {
    var len: usize = undefined;
    return if (r4os_rust_program_data_canonical(.{ .data = input.ptr, .len = input.len }, out, &len)) out[0..len] else null;
}

fn stdMemEql(a: []const u8, b: []const u8) bool {
    return r4os_rust_program_util_compare(0, .{ .data = a.ptr, .len = a.len }, .{ .data = b.ptr, .len = b.len });
}

fn startsWith(s: []const u8, prefix: []const u8) bool {
    return r4os_rust_program_util_compare(1, .{ .data = s.ptr, .len = s.len }, .{ .data = prefix.ptr, .len = prefix.len });
}

fn trimAscii(s: []const u8) []const u8 {
    const out = r4os_rust_program_util_slice(0, .{ .data = s.ptr, .len = s.len });
    return out.data[0..out.len];
}

fn startsWithIgnoreCase(s: []const u8, prefix: []const u8) bool {
    return r4os_rust_program_util_compare(2, .{ .data = s.ptr, .len = s.len }, .{ .data = prefix.ptr, .len = prefix.len });
}

fn equalsIgnoreCase(a: []const u8, b: []const u8) bool {
    return r4os_rust_program_data_equal(.{ .data = a.ptr, .len = a.len }, .{ .data = b.ptr, .len = b.len });
}

fn fixedZContainsIgnoreCase(haystack_z: []const u8, needle: []const u8) bool {
    return r4os_rust_program_util_compare(3, .{ .data = haystack_z.ptr, .len = haystack_z.len }, .{ .data = needle.ptr, .len = needle.len });
}

fn fixedZSpan(buffer: []const u8) []const u8 {
    const out = r4os_rust_program_util_slice(1, .{ .data = buffer.ptr, .len = buffer.len });
    return out.data[0..out.len];
}

fn indexOfByte(s: []const u8, ch: u8) ?usize {
    var out: usize = undefined;
    return if (r4os_rust_program_util_index(.{ .data = s.ptr, .len = s.len }, ch, &out)) out else null;
}

fn copyZ(ptr: [*:0]const u8, out: []u8) ?[]const u8 {
    var len: usize = undefined;
    return if (r4os_rust_program_data_copy_z(ptr, out.ptr, out.len, &len)) out[0..len] else null;
}

fn copyFixedZ(out: []u8, value: []const u8) void {
    r4os_rust_program_data_copy_slice(out.ptr, out.len, .{ .data = value.ptr, .len = value.len });
}

fn stripDrive(path: []const u8) []const u8 {
    const out = r4os_rust_program_util_slice(2, .{ .data = path.ptr, .len = path.len });
    return out.data[0..out.len];
}

fn isAlpha(c: u8) bool {
    return r4os_rust_program_data_alpha(c);
}

fn upper(c: u8) u8 {
    return r4os_rust_program_data_upper(c);
}

fn parentPath(path: []const u8) []const u8 {
    const out = r4os_rust_program_util_slice(3, .{ .data = path.ptr, .len = path.len });
    return out.data[0..out.len];
}

fn isRootPath(path: []const u8) bool {
    return r4os_rust_program_util_compare(4, .{ .data = path.ptr, .len = path.len }, .{ .data = undefined, .len = 0 });
}

fn baseName(path: []const u8) []const u8 {
    const out = r4os_rust_program_util_slice(4, .{ .data = path.ptr, .len = path.len });
    return out.data[0..out.len];
}

fn joinPathZ(parent: []const u8, name: []const u8, out: []u8) bool {
    return r4os_rust_program_util_path(1, .{ .data = parent.ptr, .len = parent.len }, .{ .data = name.ptr, .len = name.len }, out.ptr, out.len);
}

fn copyPathZ(path: []const u8, out: []u8) bool {
    return r4os_rust_program_util_path(0, .{ .data = path.ptr, .len = path.len }, .{ .data = undefined, .len = 0 }, out.ptr, out.len);
}

fn parentPathZ(path: []const u8, out: []u8) bool {
    return r4os_rust_program_util_path(2, .{ .data = path.ptr, .len = path.len }, .{ .data = undefined, .len = 0 }, out.ptr, out.len);
}

fn readLe16(bytes: []const u8) u16 {
    return @intCast(r4os_rust_program_util_read(.{ .data = bytes.ptr, .len = bytes.len }, 2));
}

fn readLe32(bytes: []const u8) u32 {
    return @intCast(r4os_rust_program_util_read(.{ .data = bytes.ptr, .len = bytes.len }, 4));
}

fn readLe64(bytes: []const u8) u64 {
    return @intCast(r4os_rust_program_util_read(.{ .data = bytes.ptr, .len = bytes.len }, 8));
}

fn writeLe16(bytes: []u8, value: u16) void {
    r4os_rust_program_util_write(bytes.ptr, bytes.len, 2, value);
}

fn writeLe32(bytes: []u8, value: u32) void {
    r4os_rust_program_util_write(bytes.ptr, bytes.len, 4, value);
}

fn writeLe64(bytes: []u8, value: u64) void {
    r4os_rust_program_util_write(bytes.ptr, bytes.len, 8, value);
}

fn apiGfxBrightnessRequest(input: *const gfx_output_api.abi.GfxBrightnessRequest, output: *gfx_output_api.abi.GfxBrightnessRequest) callconv(.c) i32 {
    return r4os_rust_program_domain_gfx_brightness_request(input, output);
}

// Bounded extension of the existing SMP acceptance. These are normal shipped
// owners using the same loader/API paths as Desktop and SERVMAN.



pub fn runParallelOwnersAcceptance() bool {
    return r4os_rust_program_parallel_acceptance();
}
