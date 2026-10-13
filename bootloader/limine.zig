// Limine boot protocol structures and request markers.
//
// Compatibility view for remaining Zig owners. Protocol state and request
// storage live in Rust/boot/limine.rs.

// Rust owns all retained requests and volatile response publication.
extern fn r4os_rust_limine_keep() callconv(.c) void;
extern fn r4os_rust_limine_response(kind: u32) callconv(.c) ?*anyopaque;
pub fn keepRequests() void {
    r4os_rust_limine_keep();
}
fn response(comptime T: type, kind: u32) ?*T {
    return @ptrCast(@alignCast(r4os_rust_limine_response(kind) orelse return null));
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

pub fn firstFramebuffer() ?*Framebuffer {
    // Volatile read of the response pointer for the same reason as above.
    const resp = response(FramebufferResponse, 0) orelse return null;
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

pub fn memoryMap() ?*MemoryMapResponse {
    return response(MemoryMapResponse, 1);
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

pub fn executableFile() ?*const File {
    const resp = response(ExecutableFileResponse, 3) orelse return null;
    return resp.file;
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

pub fn modules() ?*ModuleResponse {
    return response(ModuleResponse, 2);
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

pub fn rsdpAddress() ?u64 {
    const resp = response(RsdpResponse, 4) orelse return null;
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

pub fn hhdmOffset() ?u64 {
    const resp = response(HhdmResponse, 5) orelse return null;
    return resp.offset;
}
