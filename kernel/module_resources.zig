// Native R4M0 resource owner; private C values retain existing Zig callers.
const std = @import("std");
pub const capacity = 64;
pub const Record = extern struct {
    name: [63]u8 = .{0} ** 63,
    name_len: u8 = 0,
    kind: u16 = 0,
    index: u16 = 0,
    offset: u32 = 0,
    bytes: u32 = 0,
};
pub const Error = error{ Invalid, Io };
const NativeReader = extern struct {
    context: ?*anyopaque,
    read: *const fn (?*anyopaque, usize, [*]u8, usize) callconv(.c) bool,
};
extern fn r4os_rust_module_resource_parse(*Catalog, *const NativeReader, usize) callconv(.c) i32;
extern fn r4os_rust_module_resource_find(*const Catalog, [*]const u8, usize) callconv(.c) usize;
extern fn r4os_rust_module_resource_name([*]const u8, usize) callconv(.c) bool;
pub const Catalog = extern struct {
    count: usize = 0,
    records: [capacity]Record = .{Record{}} ** capacity,

    pub fn parse(self: *Catalog, reader: anytype, length: usize) Error!void {
        const Adapter = struct {
            reader: @TypeOf(reader),
            fn read(context: ?*anyopaque, offset: usize, out: [*]u8, len: usize) callconv(.c) bool {
                const adapter: *@This() = @ptrCast(@alignCast(context.?));
                return adapter.reader.read(offset, out[0..len]);
            }
        };
        var adapter: Adapter = .{ .reader = reader };
        const callback: NativeReader = .{ .context = &adapter, .read = Adapter.read };
        return switch (r4os_rust_module_resource_parse(self, &callback, length)) {
            0 => {},
            1 => error.Invalid,
            2 => error.Io,
            else => unreachable,
        };
    }

    pub fn find(self: *const Catalog, name: []const u8) ?usize {
        const index = r4os_rust_module_resource_find(self, name.ptr, name.len);
        return if (index == std.math.maxInt(usize)) null else index;
    }
};
pub fn validName(name: []const u8) bool {
    return r4os_rust_module_resource_name(name.ptr, name.len);
}
comptime {
    if (@sizeOf(Record) != 76 or @alignOf(Record) != 4 or
        @offsetOf(Record, "name") != 0 or @offsetOf(Record, "name_len") != 63 or
        @offsetOf(Record, "kind") != 64 or @offsetOf(Record, "index") != 66 or
        @offsetOf(Record, "offset") != 68 or @offsetOf(Record, "bytes") != 72 or
        @sizeOf(Catalog) != 4872 or @alignOf(Catalog) != 8 or
        @offsetOf(Catalog, "count") != 0 or @offsetOf(Catalog, "records") != 8 or
        @sizeOf(NativeReader) != 16 or @offsetOf(NativeReader, "read") != 8) @compileError("private resource layout");
}

test "resource catalog streams metadata and rejects ambiguous layouts" {
    const Reader = struct {
        data: []const u8,
        reads: usize = 0,
        pub fn read(self: *@This(), offset: usize, out: []u8) bool {
            if (offset > self.data.len or out.len > self.data.len - offset) return false;
            @memcpy(out, self.data[offset..][0..out.len]);
            self.reads += out.len;
            return true;
        }
    };
    var bytes: [4096]u8 = .{0} ** 4096;
    std.mem.writeInt(u32, bytes[0..4], 2, .little);
    for (0..2) |i| {
        const raw = bytes[4 + i * 16 ..][0..16];
        std.mem.writeInt(u16, raw[0..2], 3, .little);
        std.mem.writeInt(u32, raw[4..8], @intCast(36 + 2 * i), .little);
        std.mem.writeInt(u32, raw[8..12], @intCast(48 + 16 * i), .little);
        std.mem.writeInt(u32, raw[12..16], @intCast(if (i == 0) 1 else bytes.len - 64), .little);
    }
    bytes[36] = 'a';
    bytes[38] = 'B';
    bytes[48] = 0xff;
    @memset(bytes[64..], 0xee);
    var reader: Reader = .{ .data = &bytes };
    var catalog: Catalog = .{};
    try catalog.parse(&reader, bytes.len);
    try std.testing.expectEqual(@as(usize, 1), catalog.find("b").?);
    try std.testing.expectEqual(@as(usize, 0), catalog.find("A").?);
    try std.testing.expect(reader.reads < 100);
    for (0..bytes.len) |length| {
        try std.testing.expectError(error.Invalid, catalog.parse(&reader, length));
        try std.testing.expectEqual(@as(usize, 0), catalog.count);
    }
    const mutations = [_]struct { offset: usize, value: u8 }{
        .{ .offset = 0, .value = 0 },    .{ .offset = 0, .value = 65 },
        .{ .offset = 4, .value = 4 },    .{ .offset = 6, .value = 1 },
        .{ .offset = 8, .value = 35 },   .{ .offset = 12, .value = 47 },
        .{ .offset = 28, .value = 48 },  .{ .offset = 38, .value = 'A' },
        .{ .offset = 36, .value = '/' }, .{ .offset = 37, .value = ':' },
        .{ .offset = 40, .value = 1 },   .{ .offset = 49, .value = 1 },
    };
    for (mutations) |mutation| {
        const old = bytes[mutation.offset];
        bytes[mutation.offset] = mutation.value;
        try std.testing.expectError(error.Invalid, catalog.parse(&reader, bytes.len));
        bytes[mutation.offset] = old;
    }
    reader.data = bytes[0..36];
    try std.testing.expectError(error.Io, catalog.parse(&reader, bytes.len));
}
