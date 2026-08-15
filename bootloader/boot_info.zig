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

pub const MemoryMapEntry = struct {
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

pub const Info = struct {
    initialized: bool = false,
    bootloader_name: []const u8 = "Limine",
    memory_map_entries: []const MemoryMapEntry = &.{},
    boot_modules: []const BootModule = &.{},
    memory_map_truncated: bool = false,
    memory_map_invalid_entries: u64 = 0,
    hhdm_offset: ?u64 = null,
    framebuffer: ?*Framebuffer = null,
    rsdp_address: ?u64 = null,
};

var current: Info = .{};
var memory_map_storage: [MAX_MEMORY_MAP_ENTRIES]MemoryMapEntry = .{MemoryMapEntry{}} ** MAX_MEMORY_MAP_ENTRIES;
var boot_module_storage: [MAX_BOOT_MODULES]BootModule = .{BootModule{}} ** MAX_BOOT_MODULES;
var boot_framebuffer: Framebuffer = undefined;

pub fn keepRequests() void {
    limine.keepRequests();
}

pub fn init() bool {
    current = .{};

    const mem_response = limine.memoryMap() orelse return false;
    const entry_limit: usize = if (mem_response.entry_count > MAX_MEMORY_MAP_ENTRIES)
        MAX_MEMORY_MAP_ENTRIES
    else
        @intCast(mem_response.entry_count);

    var i: usize = 0;
    var stored: usize = 0;
    var invalid: u64 = 0;
    while (i < entry_limit) : (i += 1) {
        const src = mem_response.entries[i];
        if (normalizeMemoryMapEntry(src.base, src.length, mapMemoryKind(src.kind))) |entry| {
            memory_map_storage[stored] = entry;
            stored += 1;
        } else {
            invalid += 1;
        }
    }

    current.memory_map_entries = memory_map_storage[0..stored];
    current.memory_map_truncated = mem_response.entry_count > MAX_MEMORY_MAP_ENTRIES;
    current.memory_map_invalid_entries = invalid;
    current.hhdm_offset = limine.hhdmOffset() orelse return false;
    current.boot_modules = collectBootModules();

    if (limine.firstFramebuffer()) |src| {
        boot_framebuffer = .{
            .address = src.address,
            .width = src.width,
            .height = src.height,
            .pitch = src.pitch,
            .bpp = src.bpp,
            .memory_model = src.memory_model,
            .red_mask_size = src.red_mask_size,
            .red_mask_shift = src.red_mask_shift,
            .green_mask_size = src.green_mask_size,
            .green_mask_shift = src.green_mask_shift,
            .blue_mask_size = src.blue_mask_size,
            .blue_mask_shift = src.blue_mask_shift,
            .unused = src.unused,
            .edid_size = src.edid_size,
            .edid = src.edid,
        };
        current.framebuffer = &boot_framebuffer;
    }

    if (limine.rsdpAddress()) |addr| {
        current.rsdp_address = normalizeHhdmAddress(addr);
    }
    current.initialized = true;
    return true;
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

fn mapMemoryKind(kind: u64) MemoryKind {
    return switch (kind) {
        limine.MEMMAP_USABLE => .usable,
        limine.MEMMAP_RESERVED => .reserved,
        limine.MEMMAP_ACPI_RECLAIMABLE => .acpi_reclaimable,
        limine.MEMMAP_ACPI_NVS => .acpi_nvs,
        limine.MEMMAP_BAD_MEMORY => .bad_memory,
        limine.MEMMAP_BOOTLOADER_RECLAIMABLE => .bootloader_reclaimable,
        limine.MEMMAP_KERNEL_AND_MODULES => .kernel_and_modules,
        limine.MEMMAP_FRAMEBUFFER => .framebuffer,
        else => .unknown,
    };
}

fn collectBootModules() []const BootModule {
    boot_module_storage = .{BootModule{}} ** MAX_BOOT_MODULES;
    const response = limine.modules() orelse return boot_module_storage[0..0];
    const limit: usize = @min(@as(usize, @intCast(response.module_count)), boot_module_storage.len);
    var stored: usize = 0;
    var i: usize = 0;
    while (i < limit) : (i += 1) {
        const src = response.modules[i];
        const address = src.address orelse continue;
        if (src.size == 0 or src.size > maxUsize()) continue;
        boot_module_storage[stored] = .{
            .address = address,
            .size = @intCast(src.size),
            .path = zSlice(src.path),
            .cmdline = zSlice(src.cmdline),
            .valid = true,
        };
        stored += 1;
    }
    return boot_module_storage[0..stored];
}

fn zSlice(value: ?[*:0]const u8) []const u8 {
    const ptr = value orelse return "";
    var len: usize = 0;
    while (ptr[len] != 0 and len < 256) : (len += 1) {}
    return ptr[0..len];
}

fn maxUsize() u64 {
    return ~@as(u64, 0);
}

fn normalizeMemoryMapEntry(base: u64, length: u64, kind: MemoryKind) ?MemoryMapEntry {
    if (length == 0) return null;
    const end = checkedAdd(base, length) orelse return null;
    var usable_base: u64 = 0;
    var usable_len: u64 = 0;

    if (kind == .usable) {
        usable_base = alignUpChecked(base, PAGE_SIZE) orelse return null;
        const usable_end = alignDown(end, PAGE_SIZE);
        if (usable_end > usable_base) {
            usable_len = usable_end - usable_base;
        }
    }

    return .{
        .base = base,
        .length = length,
        .end = end,
        .usable_base = usable_base,
        .usable_len = usable_len,
        .kind = kind,
        .valid = true,
    };
}

fn normalizeHhdmAddress(addr: u64) u64 {
    return hhdmToPhys(addr) orelse addr;
}

fn checkedAdd(a: u64, b: u64) ?u64 {
    const result = a +% b;
    if (result < a) return null;
    return result;
}

fn alignDown(value: u64, alignment: u64) u64 {
    return value & ~(alignment - 1);
}

fn alignUpChecked(value: u64, alignment: u64) ?u64 {
    const adjusted = checkedAdd(value, alignment - 1) orelse return null;
    return alignDown(adjusted, alignment);
}
