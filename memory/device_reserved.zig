//! Immutable boot-map proof for device storage. No allocator mutation, cache
//! mapping, driver policy or ownership transfer occurs here.
const std = @import("std");
const boot = @import("../bootloader/boot_info.zig");
pub fn bootCovers(base: u64, bytes: u64, info: *const boot.Info) bool {
    return info.initialized and !info.memory_map_truncated and info.memory_map_invalid_entries == 0 and
        covers(base, bytes, info.memory_map_entries);
}
/// Negative system-memory proof for independently measured device storage.
/// Some UEFI maps omit stolen memory completely. A complete map still proves
/// that its gaps cannot be allocated/reclaimed by this kernel. It does not
/// prove that a gap contains storage, assign ownership or permit a cache alias.
pub fn bootExcludesSystemMemory(base: u64, bytes: u64, info: *const boot.Info) bool {
    return info.initialized and !info.memory_map_truncated and info.memory_map_invalid_entries == 0 and
        excludesSystemMemory(base, bytes, info.memory_map_entries);
}
extern fn r4os_rust_device_extent(u64, u64, [*]const boot.MemoryMapEntry, usize, bool) callconv(.c) bool;
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
pub fn excludesSystemMemory(base: u64, bytes: u64, entries: []const boot.MemoryMapEntry) bool {
    return r4os_rust_device_extent(base, bytes, entries.ptr, entries.len, true);
}
pub fn covers(base: u64, bytes: u64, entries: []const boot.MemoryMapEntry) bool {
    return r4os_rust_device_extent(base, bytes, entries.ptr, entries.len, false);
}

test "device reserved proof rejects RAM overlap, reclaimable pages and holes without counting storage" {
    const t = std.testing;
    var map = [_]boot.MemoryMapEntry{
        .{ .valid = true, .base = 0x4000, .length = 0x4000, .end = 0x8000, .kind = .framebuffer },
        .{ .valid = true, .base = 0x1000, .length = 0x4000, .end = 0x5000, .kind = .reserved },
        .{ .valid = true, .base = 0x10000, .length = 0x1000, .end = 0x11000, .kind = .usable },
    };
    var info: boot.Info = .{ .initialized = true, .memory_map_entries = &map };
    try t.expect(bootCovers(0x1000, 0x7000, &info));
    info.memory_map_truncated = true;
    try t.expect(!bootCovers(0x1000, 0x7000, &info));
    info.memory_map_truncated = false;
    info.memory_map_invalid_entries = 1;
    try t.expect(!bootCovers(0x1000, 0x7000, &info));
    info.memory_map_invalid_entries = 0;
    info.initialized = false;
    try t.expect(!bootCovers(0x1000, 0x7000, &info));
    const before = map;
    try t.expect(covers(0x1000, 0x7000, &map));
    try t.expect(std.meta.eql(before, map));
    try t.expect(!covers(0x1000, 0x8000, &map));
    try t.expect(!covers(0x1001, 4096, &map));
    try t.expect(!covers(0, 4096, &map));
    try t.expect(!covers(4096, 0, &map));
    try t.expect(!covers(std.math.maxInt(u64) - 4095, 4096, &map));
    for ([_]boot.MemoryKind{ .usable, .acpi_reclaimable, .acpi_nvs, .bootloader_reclaimable, .kernel_and_modules, .bad_memory, .unknown }) |kind| {
        map = before;
        map[2] = .{ .valid = true, .base = 0x4000, .length = 4096, .end = 0x5000, .kind = kind };
        try t.expect(!covers(0x1000, 0x7000, &map));
    }
    map = before;
    map[1].length = 0x2000;
    map[1].end = 0x3000;
    try t.expect(!covers(0x1000, 0x7000, &map));
    map = before;
    map[0].valid = false;
    try t.expect(!covers(0x1000, 0x7000, &map));
    map = before;
    map[0].end += 1;
    try t.expect(!covers(0x1000, 0x7000, &map));
}

test "unmanaged device extent excludes every system-memory kind while preserving strict reserved coverage" {
    const t = std.testing;
    const uma: u64 = 0x1bf000000;
    var map = [_]boot.MemoryMapEntry{
        .{ .valid = true, .base = 0x100000000, .length = uma - 0x100000000, .end = uma, .kind = .usable },
        .{ .valid = true, .base = uma + 0x80000000, .length = 4096, .end = uma + 0x80001000, .kind = .reserved },
    };
    var info: boot.Info = .{ .initialized = true, .memory_map_entries = &map };
    const before = map;
    try t.expect(bootExcludesSystemMemory(uma, 0x80000000, &info));
    try t.expect(!bootCovers(uma, 0x80000000, &info));
    try t.expect(std.meta.eql(before, map));
    try t.expect(!bootExcludesSystemMemory(uma - 4096, 0x80000000, &info));
    info.memory_map_truncated = true;
    try t.expect(!bootExcludesSystemMemory(uma, 4096, &info));
    info.memory_map_truncated = false;
    info.memory_map_invalid_entries = 1;
    try t.expect(!bootExcludesSystemMemory(uma, 4096, &info));
    info.memory_map_invalid_entries = 0;
    info.initialized = false;
    try t.expect(!bootExcludesSystemMemory(uma, 4096, &info));
    info.initialized = true;
    for ([_]boot.MemoryKind{ .usable, .acpi_reclaimable, .acpi_nvs, .bootloader_reclaimable, .kernel_and_modules, .bad_memory, .unknown }) |kind| {
        map[1] = .{ .valid = true, .base = uma + 4096, .length = 4096, .end = uma + 8192, .kind = kind };
        try t.expect(!bootExcludesSystemMemory(uma, 0x80000000, &info));
    }
    map = before;
    map[1].valid = false;
    try t.expect(!bootExcludesSystemMemory(uma, 4096, &info));
    map = before;
    map[1].end += 1;
    try t.expect(!bootExcludesSystemMemory(uma, 4096, &info));
    map = before;
    try t.expect(!excludesSystemMemory(uma, 4096, &.{}));
    try t.expect(!excludesSystemMemory(0, 4096, &map));
    try t.expect(!excludesSystemMemory(uma, 0, &map));
    try t.expect(!excludesSystemMemory(uma + 1, 4096, &map));
    try t.expect(!excludesSystemMemory(uma, 4097, &map));
    try t.expect(!excludesSystemMemory(std.math.maxInt(u64) - 4095, 4096, &map));
    try t.expect(!excludesSystemMemory((@as(u64, 1) << 52) - 4096, 8192, &map));
}
