// Native module-file owner. Existing Zig callers retain typed source views;
// the bridge only projects those immutable values and formats log messages.
const vfs = @import("../fs/vfs.zig");
const fs_request = @import("../fs/request.zig");
const drive = @import("../fs/drive.zig");
const k = @import("log.zig");
extern fn r4os_rust_module_file_restrict(u8) void;
extern fn r4os_rust_module_file_allowed(u8) bool;
pub fn restrictExecutionToDrive(letter: u8) void {
    r4os_rust_module_file_restrict(letter);
}
pub fn executionDriveAllowed(letter: u8) bool {
    return r4os_rust_module_file_allowed(letter);
}
pub const FileSource = struct {
    volume: vfs.Volume,
    entry: vfs.Entry,
    drive_letter: u8,
};

pub const RangeReadRequest = struct {
    source: FileSource,
    offset: usize,
    out: []u8,
    name: []const u8 = "module-file",
    verbose: bool = true,
};

/// Loader-local read-through storage for small R4M0 metadata. Two windows are
/// intentional: normal containers keep their fixed tables near the start and
/// their string/metadata block near the end. Large section payloads bypass
/// this reader and continue to stream directly into their final image.
pub const metadata_window_size: usize = 4096;
pub const metadata_window_count: usize = 2;
pub const metadata_window_capacity_bytes: usize = metadata_window_size * metadata_window_count;

pub const Stats = extern struct {
    active_buffers: u64 = 0,
    reserved_bytes: u64 = 0,
    committed_bytes: u64 = 0,
    peak_reserved_bytes: u64 = 0,
    peak_committed_bytes: u64 = 0,
    full_reads: u64 = 0,
    range_reads: u64 = 0,
    range_read_bytes: u64 = 0,
    metadata_reader_initializations: u64 = 0,
    metadata_logical_reads: u64 = 0,
    metadata_logical_bytes: u64 = 0,
    metadata_window_hits: u64 = 0,
    metadata_window_fills: u64 = 0,
    metadata_window_fill_bytes: u64 = 0,
    metadata_direct_reads: u64 = 0,
    metadata_direct_bytes: u64 = 0,
    reserve_failures: u64 = 0,
    commit_failures: u64 = 0,
    read_failures: u64 = 0,
    short_reads: u64 = 0,
    release_failures: u64 = 0,
    pressure_reclaim_attempts: u64 = 0,
    pressure_reclaimed_frames: u64 = 0,
    pressure_failures: u64 = 0,
};

pub const NativeSource = extern struct { volume: vfs.NativeVolume, entry: vfs.NativeEntry, drive: u8 };
const Bytes = extern struct { data: [*]const u8, len: usize };
const Output = extern struct { len: usize, present: bool };
const NativeIo = extern struct { context: ?*anyopaque, read: *const fn (?*anyopaque, usize, [*]u8, usize) callconv(.c) Output, name: Bytes, verbose: bool };
extern fn r4os_rust_module_file_stats(*Stats) void;
extern fn r4os_rust_module_file_note_init() void;
extern fn r4os_rust_module_file_range(*const NativeSource, usize, [*]u8, usize, Bytes, bool) Output;
extern fn r4os_rust_module_file_exact(*const NativeSource, usize, [*]u8, usize, Bytes, bool) bool;
extern fn r4os_rust_module_file_find([*]const MetadataWindow, usize, usize) usize;
extern fn r4os_rust_module_file_replace([*]const MetadataWindow) usize;
extern fn r4os_rust_module_file_reset([*]MetadataWindow, *u64) void;
extern fn r4os_rust_module_file_cached([*]MetadataWindow, *u64, usize, usize, [*]u8, usize, *const NativeIo) bool;
pub fn nativeSource(source: FileSource) NativeSource {
    return .{ .volume = vfs.nativeVolume(source.volume), .entry = vfs.nativeEntry(source.entry), .drive = source.drive_letter };
}
pub fn sourceFromNative(source: NativeSource) FileSource {
    return .{ .volume = vfs.volumeFromNative(source.volume), .entry = vfs.entryFromNative(source.entry), .drive_letter = source.drive };
}
fn bytes(value: []const u8) Bytes {
    return .{ .data = value.ptr, .len = value.len };
}
pub fn stats() Stats {
    var out: Stats = undefined;
    r4os_rust_module_file_stats(&out);
    return out;
}
pub fn readRange(req: RangeReadRequest) ?usize {
    // An empty request never inspects even an undefined source. Conditional
    // marshalling retains this property; native Rust owns the empty result.
    var source: NativeSource = undefined;
    if (req.out.len != 0) source = nativeSource(req.source);
    const result = r4os_rust_module_file_range(&source, req.offset, req.out.ptr, req.out.len, bytes(req.name), req.verbose);
    return if (result.present) result.len else null;
}
pub fn readExact(req: RangeReadRequest) bool {
    var source: NativeSource = undefined;
    if (req.out.len != 0) source = nativeSource(req.source);
    return r4os_rust_module_file_exact(&source, req.offset, req.out.ptr, req.out.len, bytes(req.name), req.verbose);
}
const MetadataWindow = extern struct {
    valid: bool = false,
    offset: usize = 0,
    len: usize = 0,
    stamp: u64 = 0,
    bytes: [metadata_window_size]u8 = .{0} ** metadata_window_size,
};
pub const BoundedReader = struct {
    source: FileSource,
    file_size: usize,
    windows: [metadata_window_count]MetadataWindow = .{MetadataWindow{}} ** metadata_window_count,
    stamp: u64 = 0,

    pub fn init(source: FileSource, file_size: usize) BoundedReader {
        r4os_rust_module_file_note_init();
        return .{ .source = source, .file_size = file_size };
    }
    pub fn reset(self: *BoundedReader, source: FileSource, file_size: usize) void {
        self.source = source;
        self.file_size = file_size;
        r4os_rust_module_file_reset(&self.windows, &self.stamp);
    }
    pub fn readExactAt(self: *BoundedReader, offset: usize, out: []u8, name: []const u8, verbose: bool) bool {
        const Context = struct {
            source: *const FileSource,
            name: []const u8,
            verbose: bool,
            fn read(pointer: ?*anyopaque, read_offset: usize, buffer: [*]u8, len: usize) callconv(.c) Output {
                const context: *const @This() = @ptrCast(@alignCast(pointer.?));
                const got = readRange(.{ .source = context.source.*, .offset = read_offset, .out = buffer[0..len], .name = context.name, .verbose = context.verbose }) orelse return .{ .len = 0, .present = false };
                return .{ .len = got, .present = true };
            }
        };
        var context: Context = .{ .source = &self.source, .name = name, .verbose = verbose };
        const io: NativeIo = .{ .context = &context, .read = Context.read, .name = bytes(name), .verbose = verbose };
        return r4os_rust_module_file_cached(&self.windows, &self.stamp, self.file_size, offset, out.ptr, out.len, &io);
    }
    fn findWindow(self: *const BoundedReader, offset: usize, len: usize) ?usize {
        const index = r4os_rust_module_file_find(&self.windows, offset, len);
        return if (index == @import("std").math.maxInt(usize)) null else index;
    }
    fn replacementWindow(self: *const BoundedReader) usize {
        return r4os_rust_module_file_replace(&self.windows);
    }
};
export fn r4os_zig_module_file_log(kind: u8, name_ptr: [*]const u8, name_len: usize, verbose: bool, offset: usize, wanted: usize, got: usize) void {
    if (!verbose) return;
    const name = name_ptr[0..name_len];
    switch (kind) {
        0, 1 => {
            k.puts("[MODFILE] ");
            k.puts(if (kind == 0) "request" else "read");
            k.puts(" failed ");
            k.puts(name);
        },
        2 => {
            k.puts("[MODFILE] short read ");
            k.puts(name);
            k.puts(" offset=");
            k.putDec(@intCast(offset));
            k.puts(" want=");
            k.putDec(@intCast(wanted));
            k.puts(" got=");
            k.putDec(@intCast(got));
        },
        3 => {
            k.puts("[MODFILE] range failed ");
            k.puts(name);
            k.puts(" offset=");
            k.putDec(@intCast(offset));
            k.puts(" len=");
            k.putDec(@intCast(wanted));
            k.puts(" file=");
            k.putDec(@intCast(got));
        },
        else => unreachable,
    }
    k.puts("\r\n");
}
comptime {
    if (@intFromEnum(fs_request.Kind.loader_read) != 19 or @intFromEnum(drive.Role.ram) != 3 or
        @sizeOf(Stats) != 192 or @sizeOf(NativeSource) != 936 or @offsetOf(NativeSource, "entry") != 88 or @offsetOf(NativeSource, "drive") != 928 or
        @sizeOf(MetadataWindow) != 4128 or @offsetOf(MetadataWindow, "valid") != 0 or @offsetOf(MetadataWindow, "offset") != 8 or @offsetOf(MetadataWindow, "len") != 16 or @offsetOf(MetadataWindow, "stamp") != 24 or @offsetOf(MetadataWindow, "bytes") != 32 or
        @sizeOf(NativeIo) != 40 or @sizeOf(Output) != 16) @compileError("private module-file layout");
}

test "metadata windows use exact containment and bounded LRU replacement" {
    const testing = @import("std").testing;
    var reader: BoundedReader = undefined;
    reader.windows = .{MetadataWindow{}} ** metadata_window_count;
    reader.stamp = 2;
    reader.windows[0].valid = true;
    reader.windows[0].offset = 64;
    reader.windows[0].len = 128;
    reader.windows[0].stamp = 1;
    reader.windows[1].valid = true;
    reader.windows[1].offset = 4096;
    reader.windows[1].len = 64;
    reader.windows[1].stamp = 2;

    try testing.expectEqual(@as(?usize, 0), reader.findWindow(80, 32));
    try testing.expectEqual(@as(?usize, 1), reader.findWindow(4096, 64));
    try testing.expectEqual(@as(?usize, null), reader.findWindow(191, 2));
    try testing.expectEqual(@as(usize, 0), reader.replacementWindow());
    try testing.expectEqual(@as(usize, 8192), metadata_window_capacity_bytes);
}
