// Boot-owned, read-only ACPI root tables. Populated before driver/task/SMP
// publication, sealed afterwards. No firmware-specific parser or execution.
const std = @import("std");
const boot = @import("../bootloader/boot_info.zig");
pub const max_tables = 256;
pub const max_bytes = 1024 * 1024;
pub const Error = error{ Invalid, Unavailable, Corrupt, NotFound, Stale };
pub const Info = struct { handle: u64, bytes: u64, generation: u64, signature: u32, revision: u8 };
const Record = struct { bytes: []const u8, signature: u32, revision: u8, valid: bool, digest: [32]u8 };
// Only retain firmware memory that blocks.zig never puts into the allocator
// and page_tables.mapHhdmDirectMap carries into the kernel's own CR3. A table
// mapped ad hoc by the bootloader outside that map is not a durable source.
pub fn retainedBacking(base: u64, bytes: u64, entries: []const boot.MemoryMapEntry) bool {
    if (base == 0 or bytes == 0) return false;
    const end = std.math.add(u64, base, bytes) catch return false;
    var cursor = base;
    while (cursor < end) {
        var next = cursor;
        for (entries) |entry| {
            if (!entry.valid or entry.length == 0 or entry.base > cursor or entry.end <= cursor) continue;
            switch (entry.kind) {
                .reserved, .acpi_reclaimable, .acpi_nvs => next = @min(end, entry.end),
                else => return false,
            }
            break;
        }
        if (next <= cursor) return false;
        cursor = next;
    }
    return true;
}
pub const Catalog = struct {
    records: [max_tables]Record = undefined,
    count: usize = 0,
    generation: u32 = 0,
    root_valid: bool = false,
    failed: bool = false,
    sealed: bool = false,

    pub fn begin(self: *Catalog) void {
        self.count = 0; self.root_valid = false; self.sealed = false;
        self.failed = self.generation == std.math.maxInt(u32);
        if (!self.failed) self.generation += 1;
    }
    pub fn rootValidated(self: *Catalog) void { if (!self.sealed) self.root_valid = true; }
    pub fn incomplete(self: *Catalog) void { if (!self.sealed) self.failed = true; }
    pub fn seal(self: *Catalog) void { self.sealed = true; }
    pub fn observe(self: *Catalog, bytes: []const u8) void {
        if (self.sealed or self.failed) return;
        if (bytes.len < 36 or bytes.len > max_bytes) { self.failed = true; return; }
        for (self.records[0..self.count]) |*record| if (record.bytes.ptr == bytes.ptr) return;
        if (self.count == self.records.len) { self.failed = true; return; }
        const size = std.mem.readInt(u32, bytes[4..8], .little);
        var sum: u8 = 0;
        for (bytes) |byte| sum +%= byte;
        var digest: [32]u8 = undefined;
        std.crypto.hash.sha2.Sha256.hash(bytes, &digest, .{});
        self.records[self.count] = .{ .bytes = bytes, .signature = std.mem.readInt(u32, bytes[0..4], .little),
            .revision = bytes[8], .valid = size == bytes.len and sum == 0, .digest = digest };
        self.count += 1;
    }
    pub fn stat(self: *const Catalog, signature: u32, occurrence: u32) Error!Info {
        if (!self.sealed or !self.root_valid or self.failed) return error.Unavailable;
        var index: u32 = 0;
        for (self.records[0..self.count], 0..) |*record, slot| {
            if (record.signature != signature) continue;
            if (index != occurrence) { index += 1; continue; }
            try unchanged(record);
            return .{ .handle = (@as(u64, self.generation) << 32) | (slot + 1), .bytes = record.bytes.len,
                .generation = self.generation, .signature = record.signature, .revision = record.revision };
        }
        return error.NotFound;
    }
    pub fn readAt(self: *const Catalog, handle: u64, offset: u64, output: []u8) Error!void {
        if (!self.sealed or !self.root_valid or self.failed) return error.Unavailable;
        const index = handle & 0xffffffff;
        if (handle >> 32 != self.generation or index == 0 or index > self.count) return error.Stale;
        const record = &self.records[index - 1];
        if (output.len == 0 or output.len > 65536 or offset > record.bytes.len or output.len > record.bytes.len - offset) return error.Invalid;
        // Never copy into any retained source or into catalog metadata, even
        // accidentally. This is a read API, not a writable firmware mapping.
        const address = @intFromPtr(output.ptr);
        const end = std.math.add(usize, address, output.len) catch return error.Invalid;
        if (address < @intFromPtr(self) + @sizeOf(Catalog) and @intFromPtr(self) < end) return error.Invalid;
        for (self.records[0..self.count]) |*source| {
            if (address < @intFromPtr(source.bytes.ptr) + source.bytes.len and @intFromPtr(source.bytes.ptr) < end) return error.Invalid;
        }
        try unchanged(record);
        @memcpy(output, record.bytes[@intCast(offset)..][0..output.len]);
        try unchanged(record);
    }
};
fn unchanged(record: *const Record) Error!void {
    if (!record.valid) return error.Corrupt;
    var digest: [32]u8 = undefined;
    std.crypto.hash.sha2.Sha256.hash(record.bytes, &digest, .{});
    if (!std.mem.eql(u8, &digest, &record.digest)) return error.Stale;
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
