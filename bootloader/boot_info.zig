const limine = @import("limine.zig");

pub const MAX_MEMORY_MAP_ENTRIES: usize = 128;
pub const MAX_BOOT_MODULES: usize = 16;
pub const PAGE_SIZE: u64 = 4096;

pub const MemoryKind = enum(u8) {
    usable,
    reserved,
    acpi_reclaimable,
    acpi_nvs,
    bad_memory,
    bootloader_reclaimable,
    kernel_and_modules,
    framebuffer,
    unknown,
};

pub const MemoryMapEntry = extern struct {
    base: u64 = 0,
    length: u64 = 0,
    end: u64 = 0,
    usable_base: u64 = 0,
    usable_len: u64 = 0,
    kind: MemoryKind = .unknown,
    valid: bool = false,
};

pub const Framebuffer = extern struct {
    address: [*]volatile u8,
    width: u64,
    height: u64,
    pitch: u64,
    bpp: u16,
    memory_model: u8,
    red_mask_size: u8,
    red_mask_shift: u8,
    green_mask_size: u8,
    green_mask_shift: u8,
    blue_mask_size: u8,
    blue_mask_shift: u8,
    unused: [5]u8,
    edid_size: u64,
    edid: ?*const anyopaque,
};

pub const BootModule = struct {
    address: [*]const u8 = undefined,
    size: usize = 0,
    path: []const u8 = "",
    cmdline: []const u8 = "",
    valid: bool = false,
};

/// Copy identity before bootloader memory can be reclaimed. This identifies
/// the actual executable; it does not infer a disk from labels or modules.
pub const ExecutableSource = extern struct {
    present: bool = false,
    media_type: u32 = 0,
    partition_index: u32 = 0,
    mbr_disk_id: u32 = 0,
    disk_guid: [16]u8 = .{0} ** 16,
    partition_guid: [16]u8 = .{0} ** 16,
    filesystem_uuid: [16]u8 = .{0} ** 16,
    path_bytes: [256]u8 = .{0} ** 256,
    path_len: usize = 0,
    path_truncated: bool = false,

    pub fn path(self: *const ExecutableSource) []const u8 {
        return self.path_bytes[0..self.path_len];
    }
};

pub const Info = struct {
    initialized: bool = false,
    bootloader_name: []const u8 = "Limine",
    memory_map_entries: []const MemoryMapEntry = &.{},
    boot_modules: []const BootModule = &.{},
    executable_source: ExecutableSource = .{},
    software_graphics_once: bool = false,
    memory_map_truncated: bool = false,
    memory_map_invalid_entries: u64 = 0,
    hhdm_offset: ?u64 = null,
    framebuffer: ?*Framebuffer = null,
    rsdp_address: ?u64 = null,
};

// Compatibility views for remaining Zig owners: immutable after BSP init.
// All normalization, Limine storage and boot decisions belong to Rust.
var current: Info = .{};
var boot_module_views: [MAX_BOOT_MODULES]BootModule = .{BootModule{}} ** MAX_BOOT_MODULES;
const RawBootModule = extern struct {
    address: ?[*]const u8,
    size: usize,
    path: ?[*]const u8,
    path_len: usize,
    cmdline: ?[*]const u8,
    cmdline_len: usize,
    valid: bool,
};
const Snapshot = extern struct {
    flags: u32,
    invalid_entries: u64,
    hhdm: u64,
    rsdp: u64,
    memory: ?[*]const MemoryMapEntry,
    memory_count: usize,
    modules: ?[*]const RawBootModule,
    module_count: usize,
    executable: *const ExecutableSource,
    framebuffer: ?*Framebuffer,
};
comptime {
    if (@sizeOf(MemoryMapEntry) != 48 or @offsetOf(MemoryMapEntry, "kind") != 40 or
        @sizeOf(RawBootModule) != 56 or @offsetOf(RawBootModule, "valid") != 48 or
        @sizeOf(ExecutableSource) != 336 or @offsetOf(ExecutableSource, "path_len") != 320 or
        @sizeOf(Snapshot) != 80 or @offsetOf(Snapshot, "memory") != 32) @compileError("Rust BootInfo layout differs");
}
extern fn r4os_rust_boot_info_init() callconv(.c) bool;
extern fn r4os_rust_boot_info() callconv(.c) *const Snapshot;
pub fn keepRequests() void {
    limine.keepRequests();
}
pub fn init() bool {
    const ok = r4os_rust_boot_info_init();
    const raw = r4os_rust_boot_info();
    current = .{
        .initialized = (raw.flags & 1) != 0,
        .memory_map_truncated = (raw.flags & 2) != 0,
        .memory_map_invalid_entries = raw.invalid_entries,
        .hhdm_offset = if ((raw.flags & 4) != 0) raw.hhdm else null,
        .framebuffer = raw.framebuffer,
        .rsdp_address = if ((raw.flags & 16) != 0) raw.rsdp else null,
        .software_graphics_once = (raw.flags & 32) != 0,
        .executable_source = raw.executable.*,
        .memory_map_entries = if (raw.memory) |entries| entries[0..raw.memory_count] else &.{},
    };
    if (raw.modules) |modules| {
        for (0..raw.module_count) |i| {
            const source = modules[i];
            boot_module_views[i] = .{
                .address = source.address.?,
                .size = source.size,
                .valid = source.valid,
                .path = if (source.path) |ptr| ptr[0..source.path_len] else "",
                .cmdline = if (source.cmdline) |ptr| ptr[0..source.cmdline_len] else "",
            };
        }
        current.boot_modules = boot_module_views[0..raw.module_count];
    }
    return ok;
}

pub fn get() *const Info {
    return &current;
}

pub fn memoryMap() []const MemoryMapEntry {
    return current.memory_map_entries;
}

pub fn bootModules() []const BootModule {
    return current.boot_modules;
}

pub fn hhdmOffset() ?u64 {
    return current.hhdm_offset;
}

pub fn physToHhdm(addr: u64) ?u64 {
    const offset = current.hhdm_offset orelse return null;
    return checkedAdd(addr, offset);
}

pub fn hhdmToPhys(addr: u64) ?u64 {
    const offset = current.hhdm_offset orelse return null;
    if (addr < offset) return null;
    return addr - offset;
}

pub fn isHhdmAddress(addr: u64) bool {
    return hhdmToPhys(addr) != null;
}

pub fn framebuffer() ?*Framebuffer {
    return current.framebuffer;
}

pub fn rsdpAddress() ?u64 {
    return current.rsdp_address;
}

pub fn memoryKindName(kind: MemoryKind) []const u8 {
    return switch (kind) {
        .usable => "usable",
        .reserved => "reserved",
        .acpi_reclaimable => "acpi-reclaim",
        .acpi_nvs => "acpi-nvs",
        .bad_memory => "bad",
        .bootloader_reclaimable => "bootloader",
        .kernel_and_modules => "kernel",
        .framebuffer => "framebuffer",
        .unknown => "unknown",
    };
}

fn checkedAdd(a: u64, b: u64) ?u64 {
    const result = a +% b;
    if (result < a) return null;
    return result;
}
