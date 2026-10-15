// Native transfer owner; generic callbacks only project the exact existing
// backend instance. Its binding stays on this task's stack through every wait.
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
pub const max_chunk_bytes: usize = 32768;
pub const Progress = extern struct { bytes: u64 = 0, source_size: u64 = 0, chunks: u32 = 0, max_chunk: u32 = 0 };
const Io = extern struct {
    context: *anyopaque,
    cancelled: *const fn (*anyopaque) callconv(.c) bool,
    prepare: *const fn (*anyopaque) callconv(.c) bool,
    allow_readers: *const fn (*anyopaque) callconv(.c) void,
    read: *const fn (*anyopaque, u32, [*]u8, usize, *usize) callconv(.c) bool,
    append: *const fn (*anyopaque, u64, [*]const u8, usize) callconv(.c) bool,
    progress: *const fn (*anyopaque) callconv(.c) void,
    pause: *const fn (*anyopaque) callconv(.c) void,
    flush: *const fn (*anyopaque) callconv(.c) bool,
    publish: *const fn (*anyopaque) callconv(.c) bool,
    abort: *const fn (*anyopaque) callconv(.c) void,
};
extern fn r4os_rust_fs_copy_transfer(*const Io, [*]u8, usize, u64, *Progress) callconv(.c) bool;
pub fn transfer(io: anytype, buffer: []u8, size: u64, progress: *Progress) bool {
    const Binding = struct {
        fn instance(raw: *anyopaque) *@TypeOf(io) {
            return @ptrCast(@alignCast(raw));
        }
        fn cancelled(raw: *anyopaque) callconv(.c) bool {
            return instance(raw).*.cancelled();
        }
        fn prepare(raw: *anyopaque) callconv(.c) bool {
            return instance(raw).*.prepare();
        }
        fn allowReaders(raw: *anyopaque) callconv(.c) void {
            instance(raw).*.allowReaders();
        }
        fn read(raw: *anyopaque, at: u32, data: [*]u8, len: usize, out: *usize) callconv(.c) bool {
            out.* = instance(raw).*.read(at, data[0..len]) orelse return false;
            return true;
        }
        fn append(raw: *anyopaque, at: u64, data: [*]const u8, len: usize) callconv(.c) bool {
            return instance(raw).*.append(at, data[0..len]);
        }
        fn report(raw: *anyopaque) callconv(.c) void {
            instance(raw).*.progress();
        }
        fn pause(raw: *anyopaque) callconv(.c) void {
            instance(raw).*.pause();
        }
        fn flush(raw: *anyopaque) callconv(.c) bool {
            return instance(raw).*.flush();
        }
        fn publish(raw: *anyopaque) callconv(.c) bool {
            return instance(raw).*.publish();
        }
        fn abort(raw: *anyopaque) callconv(.c) void {
            instance(raw).*.abort();
        }
    };
    var binding = io;
    const callbacks: Io = .{ .context = @ptrCast(&binding), .cancelled = Binding.cancelled, .prepare = Binding.prepare, .allow_readers = Binding.allowReaders, .read = Binding.read, .append = Binding.append, .progress = Binding.report, .pause = Binding.pause, .flush = Binding.flush, .publish = Binding.publish, .abort = Binding.abort };
    return r4os_rust_fs_copy_transfer(&callbacks, buffer.ptr, buffer.len, size, progress);
}
comptime {
    if (@sizeOf(Progress) != 24 or @offsetOf(Progress, "chunks") != 16 or @sizeOf(Io) != 88) @compileError("private copy transfer layout drift");
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
