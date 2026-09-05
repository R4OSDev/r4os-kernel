// Limine boot protocol structures and request markers.
//
// Limine scans the ".requests" section in the kernel ELF for these magic IDs
// and fills the response pointers before jumping to kmain.

const MAGIC_0: u64 = 0xc7b1dd30df4c8b88;
const MAGIC_1: u64 = 0x0a82e883a194f07b;

// --- Start-/End-Marker ---------------------------------------------------
// Limine 12 looks for requests only in the range between these markers.
// LLD places orphan sections alphabetically; the section names are selected so
// alphabetical order equals the required order (start < requests < end).
pub export var requests_start_marker: [4]u64 linksection(".limine_reqs_a_start") = .{
    0xf6b8f4b39de7d1ae,
    0xfab91a6940fcb9cf,
    0x785c6ed015d3e316,
    0x181e920a7852b9d9,
};

pub export var requests_end_marker: [2]u64 linksection(".limine_reqs_z_end") = .{
    0xadc0e0531bb10d03,
    0x9572709f31764c62,
};

pub fn keepRequests() void {
    touchU64(&requests_start_marker[0]);
    touchU64(&framebuffer_request.id[0]);
    touchU64(&memory_map_request.id[0]);
    touchU64(&module_request.id[0]);
    touchU64(&executable_file_request.id[0]);
    touchU64(&rsdp_request.id[0]);
    touchU64(&hhdm_request.id[0]);
    touchU64(&requests_end_marker[0]);
}

fn touchU64(ptr: *u64) void {
    const volatile_ptr: *volatile u64 = @ptrCast(ptr);
    _ = volatile_ptr.*;
}

// --- Framebuffer ----------------------------------------------------------
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

pub const FramebufferResponse = extern struct {
    revision: u64,
    framebuffer_count: u64,
    framebuffers: [*]const *Framebuffer,
};

pub const FramebufferRequest = extern struct {
    id: [4]u64,
    revision: u64,
    response: ?*FramebufferResponse,
};

pub export var framebuffer_request: FramebufferRequest linksection(".limine_reqs_m_data") = .{
    .id = .{ MAGIC_0, MAGIC_1, 0x9d5827dcd881dd75, 0xa3148604f6fab11b },
    .revision = 0,
    .response = null,
};

pub fn firstFramebuffer() ?*Framebuffer {
    // Volatile read of the response pointer for the same reason as above.
    const resp_ptr: *volatile ?*FramebufferResponse = &framebuffer_request.response;
    const resp = resp_ptr.* orelse return null;
    if (resp.framebuffer_count == 0) return null;
    return resp.framebuffers[0];
}

// --- Memory Map -----------------------------------------------------------
pub const MemoryMapEntry = extern struct {
    base: u64,
    length: u64,
    kind: u64,
};

pub const MemoryMapResponse = extern struct {
    revision: u64,
    entry_count: u64,
    entries: [*]const *MemoryMapEntry,
};

pub const MemoryMapRequest = extern struct {
    id: [4]u64,
    revision: u64,
    response: ?*MemoryMapResponse,
};

pub const MEMMAP_USABLE: u64 = 0;
pub const MEMMAP_RESERVED: u64 = 1;
pub const MEMMAP_ACPI_RECLAIMABLE: u64 = 2;
pub const MEMMAP_ACPI_NVS: u64 = 3;
pub const MEMMAP_BAD_MEMORY: u64 = 4;
pub const MEMMAP_BOOTLOADER_RECLAIMABLE: u64 = 5;
pub const MEMMAP_KERNEL_AND_MODULES: u64 = 6;
pub const MEMMAP_FRAMEBUFFER: u64 = 7;

pub export var memory_map_request: MemoryMapRequest linksection(".limine_reqs_m_data") = .{
    .id = .{ MAGIC_0, MAGIC_1, 0x67cf3d9d378a806f, 0xe304acdfc50c3c62 },
    .revision = 0,
    .response = null,
};

pub fn memoryMap() ?*MemoryMapResponse {
    const resp_ptr: *volatile ?*MemoryMapResponse = &memory_map_request.response;
    return resp_ptr.*;
}

// --- Modules --------------------------------------------------------------
// Protocol reference: https://raw.githubusercontent.com/limine-bootloader/limine-protocol/617369cb577108e483c096e373c4f2ecc9d2081d/include/limine.h
// 0BSD, Copyright (C) 2022-2026 Mintsuki and contributors.
// The UUID fields have the same little-endian field layout as GPT on x86_64.
pub const Uuid = extern struct {
    a: u32,
    b: u16,
    c: u16,
    d: [8]u8,

    pub fn bytes(self: Uuid) [16]u8 {
        return @bitCast(self);
    }
};

pub const File = extern struct {
    revision: u64,
    address: ?[*]const u8,
    size: u64,
    path: ?[*:0]const u8,
    cmdline: ?[*:0]const u8,
    media_type: u32,
    unused: u32,
    tftp_ipv4: [4]u8,
    tftp_port: u32,
    partition_index: u32,
    mbr_disk_id: u32,
    gpt_disk_uuid: Uuid,
    gpt_part_uuid: Uuid,
    part_uuid: Uuid,
};

comptime {
    if (@sizeOf(File) != 112 or @offsetOf(File, "gpt_disk_uuid") != 64 or
        @offsetOf(File, "gpt_part_uuid") != 80) @compileError("Limine file ABI mismatch");
}

pub const ExecutableFileResponse = extern struct { revision: u64, file: *const File };
pub const ExecutableFileRequest = extern struct {
    id: [4]u64,
    revision: u64,
    response: ?*ExecutableFileResponse,
};

pub export var executable_file_request: ExecutableFileRequest linksection(".limine_reqs_m_data") = .{
    .id = .{ MAGIC_0, MAGIC_1, 0xad97e90e83f1ed67, 0x31eb5d1c5ff23b69 },
    .revision = 0,
    .response = null,
};

pub fn executableFile() ?*const File {
    const ptr: *volatile ?*ExecutableFileResponse = &executable_file_request.response;
    const response = ptr.* orelse return null;
    return response.file;
}

pub const ModuleResponse = extern struct {
    revision: u64,
    module_count: u64,
    modules: [*]const *File,
};

pub const ModuleRequest = extern struct {
    id: [4]u64,
    revision: u64,
    response: ?*ModuleResponse,
};

pub export var module_request: ModuleRequest linksection(".limine_reqs_m_data") = .{
    .id = .{ MAGIC_0, MAGIC_1, 0x3e7e279702be32af, 0xca1c4f3bd1280cee },
    .revision = 0,
    .response = null,
};

pub fn modules() ?*ModuleResponse {
    const resp_ptr: *volatile ?*ModuleResponse = &module_request.response;
    return resp_ptr.*;
}

// --- RSDP ---------------------------------------------------------------
pub const RsdpResponse = extern struct {
    revision: u64,
    address: u64,
};

pub const RsdpRequest = extern struct {
    id: [4]u64,
    revision: u64,
    response: ?*RsdpResponse,
};

pub export var rsdp_request: RsdpRequest linksection(".limine_reqs_m_data") = .{
    .id = .{ MAGIC_0, MAGIC_1, 0xc5e77b6b397e7b43, 0x27637845accdcf3c },
    .revision = 0,
    .response = null,
};

pub fn rsdpAddress() ?u64 {
    const resp_ptr: *volatile ?*RsdpResponse = &rsdp_request.response;
    const resp = resp_ptr.* orelse return null;
    if (resp.address == 0) return null;
    return resp.address;
}

// --- Higher-Half Direct Map ----------------------------------------------
pub const HhdmResponse = extern struct {
    revision: u64,
    offset: u64,
};

pub const HhdmRequest = extern struct {
    id: [4]u64,
    revision: u64,
    response: ?*HhdmResponse,
};

pub export var hhdm_request: HhdmRequest linksection(".limine_reqs_m_data") = .{
    .id = .{ MAGIC_0, MAGIC_1, 0x48dcf1cb8ad2b852, 0x63984e959a98244b },
    .revision = 0,
    .response = null,
};

pub fn hhdmOffset() ?u64 {
    const resp_ptr: *volatile ?*HhdmResponse = &hhdm_request.response;
    const resp = resp_ptr.* orelse return null;
    return resp.offset;
}
