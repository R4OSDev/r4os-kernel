// Read-only media inventory. No drive letters, system heuristics or writes.
const builtin = @import("builtin");
comptime {
    if (builtin.is_test) {
        _ = @import("rust_owner_test");
    }
}
pub const gpt = @import("gpt.zig");
pub const guid = @import("guid.zig");
pub const max_partitions = 128;
pub const bios_boot_guid: guid.Guid = .{ 0x48, 0x61, 0x68, 0x21, 0x49, 0x64, 0x6F, 0x6E, 0x74, 0x4E, 0x65, 0x65, 0x64, 0x45, 0x46, 0x49 };
pub const esp_guid: guid.Guid = .{ 0x28, 0x73, 0x2A, 0xC1, 0x1F, 0xF8, 0xD2, 0x11, 0xBA, 0x4B, 0x00, 0xA0, 0xC9, 0x3E, 0xC9, 0x3B };
pub const basic_guid: guid.Guid = .{ 0xA2, 0xA0, 0xD0, 0xEB, 0xE5, 0xB9, 0x33, 0x44, 0x87, 0xC0, 0x68, 0xB6, 0xB7, 0x26, 0x99, 0xC7 };

pub const Reader = struct { ctx: *anyopaque, read: *const fn (*anyopaque, u64, *[512]u8) bool, sector_bytes: u32, sectors: u64 };
pub const Partition = extern struct {
    number: u32 = 0,
    first_lba: u64 = 0,
    sector_count: u64 = 0,
    unique_guid: guid.Guid = guid.zero,
    type_guid: guid.Guid = guid.zero,
    mbr_type: u8 = 0,
    attributes: u64 = 0,
    name: [36]u16 = .{0} ** 36,
    pub fn end(self: Partition) u64 {
        return r4os_rust_partition_end(&self);
    }
};
pub const Table = struct {
    kind: enum { unknown, mbr, gpt } = .unknown,
    valid: bool = false,
    reason: []const u8 = "not-scanned",
    disk_guid: guid.Guid = guid.zero,
    mbr_disk_id: u32 = 0,
    first_usable: u64 = 0,
    last_usable: u64 = 0,
    partitions: [max_partitions]Partition = .{Partition{}} ** max_partitions,
    count: usize = 0,
    pub fn items(self: *const Table) []const Partition {
        return self.partitions[0..self.count];
    }
};
const NativeReader = extern struct { ctx: *anyopaque, read: *const fn (*anyopaque, u64, *[512]u8) callconv(.c) bool, sector_bytes: u32, sectors: u64 };
const Label = extern struct { ptr: [*]const u8, len: usize };
const NativeTable = extern struct {
    kind: u8 = 0,
    valid: bool = false,
    reason: Label = .{ .ptr = "not-scanned", .len = 11 },
    disk_guid: guid.Guid = guid.zero,
    mbr_disk_id: u32 = 0,
    first_usable: u64 = 0,
    last_usable: u64 = 0,
    partitions: [*]Partition,
    count: usize = 0,
};
extern fn r4os_rust_partition_scan(*const NativeReader, *NativeTable) callconv(.c) void;
extern fn r4os_rust_partition_end(*const Partition) callconv(.c) u64;
const Context = struct {
    reader: Reader,
    fn read(raw: *anyopaque, lba: u64, out: *[512]u8) callconv(.c) bool {
        const self: *const Context = @ptrCast(@alignCast(raw));
        return self.reader.read(self.reader.ctx, lba, out);
    }
};
pub fn scan(reader: Reader, result: *Table) void {
    var context = Context{ .reader = reader };
    const input = NativeReader{ .ctx = &context, .read = Context.read, .sector_bytes = reader.sector_bytes, .sectors = reader.sectors };
    var output = NativeTable{ .partitions = &result.partitions };
    r4os_rust_partition_scan(&input, &output);
    result.kind = @enumFromInt(output.kind);
    result.valid = output.valid;
    result.reason = output.reason.ptr[0..output.reason.len];
    result.disk_guid = output.disk_guid;
    result.mbr_disk_id = output.mbr_disk_id;
    result.first_usable = output.first_usable;
    result.last_usable = output.last_usable;
    result.count = output.count;
}
comptime {
    if (@sizeOf(NativeReader) != 32 or @sizeOf(Partition) != 144 or @offsetOf(Partition, "attributes") != 64 or @sizeOf(NativeTable) != 80 or @offsetOf(NativeTable, "partitions") != 64) @compileError("Native partition inventory C layout differs");
}
