// Canonical R4M0 .rsrc catalog. Parsing reads metadata and at most 15 bytes
// of padding per blob; payloads remain in the original container.
const std = @import("std");
pub const capacity = 64;
pub const Record = struct {
    name: [63]u8 = .{0} ** 63,
    name_len: u8 = 0,
    kind: u16 = 0,
    index: u16 = 0,
    offset: u32 = 0,
    bytes: u32 = 0,
};
pub const Error = error{ Invalid, Io };
pub const Catalog = struct {
    count: usize = 0,
    records: [capacity]Record = .{Record{}} ** capacity,

    // Reader offsets are relative to this section and read() returns bool.
    // A failed parse never publishes a partial catalog.
    pub fn parse(self: *Catalog, reader: anytype, length: usize) Error!void {
        self.count = 0;
        var count_bytes: [4]u8 = undefined;
        try read(reader, length, 0, &count_bytes);
        const count = std.mem.readInt(u32, &count_bytes, .little);
        if (count == 0 or count > capacity) return error.Invalid;
        var names_end: usize = 4 + 16 * @as(usize, count);
        if (names_end > length) return error.Invalid;
        var last_kind: u16 = 1;
        var next_icon: u16 = 0;
        var help_seen = false;
        for (self.records[0..count], 0..) |*record, i| {
            var raw: [16]u8 = undefined;
            try read(reader, length, 4 + 16 * i, &raw);
            const kind = std.mem.readInt(u16, raw[0..2], .little);
            const index = std.mem.readInt(u16, raw[2..4], .little);
            const name_offset = std.mem.readInt(u32, raw[4..8], .little);
            record.* = .{
                .kind = kind,
                .index = index,
                .offset = std.mem.readInt(u32, raw[8..12], .little),
                .bytes = std.mem.readInt(u32, raw[12..16], .little),
            };
            if (kind < last_kind or kind > 3 or record.bytes == 0) return error.Invalid;
            last_kind = kind;
            switch (kind) {
                1 => {
                    if (index != next_icon or name_offset != 0) return error.Invalid;
                    next_icon += 1;
                },
                2 => {
                    if (help_seen or index != 0 or name_offset != 0) return error.Invalid;
                    help_seen = true;
                },
                3 => {
                    if (index != 0 or name_offset != names_end) return error.Invalid;
                    while (true) {
                        var byte: [1]u8 = undefined;
                        try read(reader, length, names_end, &byte);
                        names_end += 1;
                        if (byte[0] == 0) break;
                        if (!nameByte(byte[0]) or record.name_len == record.name.len) return error.Invalid;
                        record.name[record.name_len] = byte[0];
                        record.name_len += 1;
                    }
                    if (record.name_len == 0) return error.Invalid;
                    for (self.records[0..i]) |previous| {
                        if (previous.kind == 3 and std.ascii.eqlIgnoreCase(previous.name[0..previous.name_len], record.name[0..record.name_len])) return error.Invalid;
                    }
                },
                else => return error.Invalid,
            }
        }
        var cursor = names_end;
        for (self.records[0..count]) |record| {
            const padding = (16 - (cursor & 15)) & 15;
            var zeros: [15]u8 = undefined;
            try read(reader, length, cursor, zeros[0..padding]);
            for (zeros[0..padding]) |byte| if (byte != 0) return error.Invalid;
            cursor += padding;
            if (record.offset != cursor or cursor > length or record.bytes > length - cursor) return error.Invalid;
            cursor += record.bytes;
        }
        if (cursor != length) return error.Invalid;
        self.count = count;
    }

    pub fn find(self: *const Catalog, name: []const u8) ?usize {
        if (!validName(name)) return null;
        for (self.records[0..self.count], 0..) |record, i| {
            if (record.kind == 3 and std.ascii.eqlIgnoreCase(name, record.name[0..record.name_len])) return i;
        }
        return null;
    }
};

pub fn validName(name: []const u8) bool {
    if (name.len == 0 or name.len > 63) return false;
    for (name) |byte| if (!nameByte(byte)) return false;
    return true;
}
fn nameByte(byte: u8) bool {
    return byte >= 0x20 and byte <= 0x7e and byte != '/' and byte != '\\' and byte != ':';
}
fn read(reader: anytype, length: usize, offset: usize, out: []u8) Error!void {
    if (offset > length or out.len > length - offset) return error.Invalid;
    if (out.len != 0 and !reader.read(offset, out)) return error.Io;
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
