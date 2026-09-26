// A copy's payload phase is independent of namespace publication. The I/O
// adapter owns mount/mutation leases, one bounded scratch buffer and each
// short backend transaction. No borrowed backend scratch survives a method.
pub const max_chunk_bytes: usize = 32768;
pub const Progress = struct {
    bytes: u64 = 0,
    source_size: u64 = 0,
    chunks: u32 = 0,
    max_chunk: u32 = 0,
};

pub fn transfer(io: anytype, buffer: []u8, size: u64, progress: *Progress) bool {
    progress.* = .{ .source_size = size };
    if (buffer.len == 0 or size > 0xffffffff or io.cancelled()) return false;
    const chunk = buffer[0..@min(buffer.len, max_chunk_bytes)];
    var published = false;
    var publication_attempted = false;
    // After an uncertain publish the stage may no longer own its payload.
    // Never free or retry publication after crossing this boundary.
    defer if (!published and !publication_attempted) io.abort();
    if (!io.prepare()) return false;
    io.allowReaders();
    while (progress.bytes < size) {
        if (io.cancelled()) return false;
        const want: usize = @intCast(@min(size - progress.bytes, chunk.len));
        const got = io.read(@intCast(progress.bytes), chunk[0..want]) orelse return false;
        if (got != want) return false;
        if (!io.append(progress.bytes, chunk[0..want])) return false;
        progress.bytes += want;
        progress.chunks += 1;
        progress.max_chunk = @max(progress.max_chunk, @as(u32, @intCast(want)));
        io.progress();
        io.pause();
    }
    if (!io.flush() or io.cancelled()) return false;
    publication_attempted = true;
    published = io.publish();
    return published;
}

test "bounded staged copy preserves old destination until publication and drains failures" {
    const std = @import("std");
    const Mock = struct {
        source: [70001]u8 = .{0x5a} ** 70001,
        stage: [70001]u8 = undefined,
        stage_bytes: usize = 0,
        visible_bytes: usize = 17,
        failure: enum { none, prepare, short_read, append, flush, publish } = .none,
        cancel_after: ?usize = null,
        paused: usize = 0,
        admits_readers: bool = false,
        live_stage: bool = false,
        aborts: usize = 0,
        pub fn cancelled(self: *@This()) bool {
            return if (self.cancel_after) |n| self.paused >= n else false;
        }
        pub fn prepare(self: *@This()) bool {
            self.live_stage = true;
            return self.failure != .prepare;
        }
        pub fn allowReaders(self: *@This()) void {
            self.admits_readers = true;
        }
        pub fn read(self: *@This(), at: usize, out: []u8) ?usize {
            std.debug.assert(self.admits_readers and self.visible_bytes == 17 and out.len <= max_chunk_bytes);
            @memcpy(out, self.source[at..][0..out.len]);
            return if (self.failure == .short_read) out.len - 1 else out.len;
        }
        pub fn append(self: *@This(), at: u64, bytes: []const u8) bool {
            if (self.failure == .append) return false;
            std.debug.assert(at == self.stage_bytes and self.visible_bytes == 17);
            @memcpy(self.stage[self.stage_bytes..][0..bytes.len], bytes);
            self.stage_bytes += bytes.len;
            return true;
        }
        pub fn progress(_: *@This()) void {}
        pub fn pause(self: *@This()) void {
            self.paused += 1;
        }
        pub fn flush(self: *@This()) bool {
            return self.failure != .flush;
        }
        pub fn publish(self: *@This()) bool {
            self.visible_bytes = self.stage_bytes;
            if (self.failure == .publish) return false;
            self.live_stage = false;
            return true;
        }
        pub fn abort(self: *@This()) void {
            self.aborts += 1;
            self.live_stage = false;
        }
    };
    var buffer: [65536]u8 = undefined;
    var progress: Progress = .{};
    var io: Mock = .{};
    try std.testing.expect(transfer(&io, &buffer, io.source.len, &progress));
    try std.testing.expectEqualSlices(u8, &io.source, &io.stage);
    try std.testing.expectEqual(@as(u32, 3), progress.chunks);
    try std.testing.expectEqual(@as(u32, max_chunk_bytes), progress.max_chunk);
    for ([_]@TypeOf(io.failure){ .prepare, .short_read, .append, .flush, .publish }) |failure| {
        io = .{ .failure = failure };
        try std.testing.expect(!transfer(&io, &buffer, io.source.len, &progress));
        if (failure == .publish) {
            try std.testing.expect(io.live_stage and io.aborts == 0);
        } else {
            try std.testing.expect(!io.live_stage and io.aborts == 1 and io.visible_bytes == 17);
        }
    }
    io = .{ .cancel_after = 1 };
    try std.testing.expect(!transfer(&io, &buffer, io.source.len, &progress));
    try std.testing.expect(progress.bytes == max_chunk_bytes and io.aborts == 1 and io.visible_bytes == 17);
    io = .{};
    try std.testing.expect(transfer(&io, &buffer, 0, &progress));
    try std.testing.expect(progress.bytes == 0 and progress.chunks == 0 and io.visible_bytes == 0);
}
