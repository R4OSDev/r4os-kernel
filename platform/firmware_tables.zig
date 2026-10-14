// Boot-owned read-only ACPI catalog; Rust owns construction, identity and reads.
const std = @import("std");
const boot = @import("../bootloader/boot_info.zig");
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
pub const max_tables = 256;
pub const max_bytes = 1024 * 1024;
pub const Error = error{ Invalid, Unavailable, Corrupt, NotFound, Stale };
pub const Info = extern struct { handle: u64, bytes: u64, generation: u64, signature: u32, revision: u8 };
const Record = extern struct {
    address: u64 = 0,
    bytes: u64 = 0,
    signature: u32 = 0,
    revision: u8 = 0,
    valid: bool = false,
    digest: [32]u8 = @splat(0),
};
extern fn r4os_rust_firmware_catalog() *Catalog;
extern fn r4os_rust_firmware_retained(base: u64, bytes: u64, entries: [*]const boot.MemoryMapEntry, count: u64) bool;
extern fn r4os_rust_firmware_control(catalog: *Catalog, operation: u8, input: ?[*]const u8, bytes: u64) void;
extern fn r4os_rust_firmware_stat(catalog: *const Catalog, signature: u32, occurrence: u32, output: *Info) i32;
extern fn r4os_rust_firmware_read(catalog: *const Catalog, handle: u64, offset: u64, output: [*]u8, bytes: u64) i32;
fn result(code: i32) Error!void {
    return switch (code) {
        0 => {},
        -1 => error.Invalid,
        -2 => error.Unavailable,
        -3 => error.Corrupt,
        -4 => error.NotFound,
        -5 => error.Stale,
        else => unreachable,
    };
}
pub fn retainedBacking(base: u64, bytes: u64, entries: []const boot.MemoryMapEntry) bool {
    return r4os_rust_firmware_retained(base, bytes, entries.ptr, entries.len);
}
// Explicit C storage also lets the unchanged existing fixture exercise the
// actual Rust owner on a caller-owned catalog. Production storage lives in Rust.
pub const Catalog = extern struct {
    records: [max_tables]Record = .{Record{}} ** max_tables,
    count: usize = 0,
    generation: u32 = 0,
    root_valid: bool = false,
    failed: bool = false,
    sealed: bool = false,
    pub fn begin(self: *Catalog) void {
        r4os_rust_firmware_control(self, 0, null, 0);
    }
    pub fn rootValidated(self: *Catalog) void {
        r4os_rust_firmware_control(self, 1, null, 0);
    }
    pub fn incomplete(self: *Catalog) void {
        r4os_rust_firmware_control(self, 2, null, 0);
    }
    pub fn seal(self: *Catalog) void {
        r4os_rust_firmware_control(self, 3, null, 0);
    }
    pub fn observe(self: *Catalog, bytes: []const u8) void {
        r4os_rust_firmware_control(self, 4, bytes.ptr, bytes.len);
    }
    pub fn stat(self: *const Catalog, signature: u32, occurrence: u32) Error!Info {
        var output: Info = undefined;
        try result(r4os_rust_firmware_stat(self, signature, occurrence, &output));
        return output;
    }
    pub fn readAt(self: *const Catalog, handle: u64, offset: u64, output: []u8) Error!void {
        try result(r4os_rust_firmware_read(self, handle, offset, output.ptr, output.len));
    }
};
pub fn residentCatalog() *Catalog {
    return r4os_rust_firmware_catalog();
}
// Namespace compatibility for the remaining ACPI/driver-resource callers;
// this contains no mutable state or second catalog.
pub const ResidentCatalog = struct {
    pub fn begin() void {
        residentCatalog().begin();
    }
    pub fn rootValidated() void {
        residentCatalog().rootValidated();
    }
    pub fn incomplete() void {
        residentCatalog().incomplete();
    }
    pub fn seal() void {
        residentCatalog().seal();
    }
    pub fn observe(bytes: []const u8) void {
        residentCatalog().observe(bytes);
    }
    pub fn stat(signature: u32, occurrence: u32) Error!Info {
        return residentCatalog().stat(signature, occurrence);
    }
    pub fn readAt(handle: u64, offset: u64, output: []u8) Error!void {
        try residentCatalog().readAt(handle, offset, output);
    }
};
comptime {
    if (@sizeOf(Record) != 56 or @sizeOf(Catalog) != 14352 or
        @offsetOf(Catalog, "count") != 14336 or @sizeOf(Info) != 32)
        @compileError("Rust firmware catalog layout differs");
}

test "firmware table catalog keeps complete boot identity, damaged sources and bounded immutable reads" {
    const t = std.testing;
    var map = [_]boot.MemoryMapEntry{
        .{ .valid = true, .base = 0x1000, .length = 0x1000, .end = 0x2000, .kind = .acpi_reclaimable },
        .{ .valid = true, .base = 0x2000, .length = 0x1000, .end = 0x3000, .kind = .acpi_nvs },
        .{ .valid = true, .base = 0x3000, .length = 0x1000, .end = 0x4000, .kind = .reserved },
    };
    try t.expect(retainedBacking(0x1800, 0x2000, &map));
    try t.expect(!retainedBacking(0x1800, 0x3000, &map));
    try t.expect(!retainedBacking(0, 0x1000, &map));
    try t.expect(!retainedBacking(std.math.maxInt(u64), 1, &map));
    map[1].kind = .usable; try t.expect(!retainedBacking(0x1800, 0x1000, &map));
    map[1].kind = .bootloader_reclaimable; try t.expect(!retainedBacking(0x1800, 0x1000, &map));
    map[1].valid = false; try t.expect(!retainedBacking(0x1800, 0x1000, &map));
    const signature = std.mem.readInt(u32, "TEST", .little);
    var bytes: [64]u8 = @splat(0);
    @memcpy(bytes[0..4], "TEST"); std.mem.writeInt(u32, bytes[4..8], bytes.len, .little); bytes[8] = 1;
    var sum: u8 = 0; for (bytes) |byte| sum +%= byte; bytes[9] -%= sum;
    var catalog: Catalog = .{};
    catalog.begin(); catalog.observe(&bytes);
    try t.expectError(error.Unavailable, catalog.stat(signature, 0));
    catalog.rootValidated(); catalog.observe(&bytes); catalog.seal();
    try t.expectEqual(@as(usize, 1), catalog.count);
    const info = try catalog.stat(signature, 0);
    try t.expect(info.handle != @intFromPtr(&bytes) and info.generation == 1 and info.bytes == 64 and info.revision == 1);
    try t.expectError(error.NotFound, catalog.stat(signature, 1));
    var output: [8]u8 = @splat(0x77);
    try catalog.readAt(info.handle, 56, &output);
    try t.expectEqualSlices(u8, bytes[56..64], &output);
    try t.expectError(error.Invalid, catalog.readAt(info.handle, 60, &output));
    try t.expectError(error.Invalid, catalog.readAt(info.handle, std.math.maxInt(u64), &output));
    try t.expectError(error.Invalid, catalog.readAt(info.handle, 0, bytes[0..8]));
    try t.expectError(error.Invalid, catalog.readAt(info.handle, 0, std.mem.asBytes(&catalog)[0..8]));
    bytes[40] +%= 1; bytes[41] -%= 1; // unchanged ACPI checksum is insufficient.
    try t.expectError(error.Stale, catalog.stat(signature, 0));
    try t.expectError(error.Stale, catalog.readAt(info.handle, 0, &output));
    catalog.begin(); catalog.rootValidated(); catalog.observe(&bytes); catalog.seal();
    try t.expectError(error.Stale, catalog.readAt(info.handle, 0, &output));
    bytes[9] +%= 1;
    catalog.begin(); catalog.rootValidated(); catalog.observe(&bytes); catalog.seal();
    try t.expectError(error.Corrupt, catalog.stat(signature, 0));
    catalog.begin(); catalog.rootValidated(); catalog.observe(&bytes); catalog.incomplete(); catalog.seal();
    try t.expectError(error.Unavailable, catalog.stat(signature, 99));
    catalog.begin(); catalog.rootValidated();
    var many: [max_tables + 1][64]u8 = undefined;
    for (&many) |*entry| { entry.* = bytes; catalog.observe(entry); }
    catalog.seal();
    try t.expectError(error.Unavailable, catalog.stat(signature, 0));
    catalog.generation = std.math.maxInt(u32); catalog.begin(); catalog.rootValidated(); catalog.seal();
    try t.expectError(error.Unavailable, catalog.stat(signature, 0));
}
