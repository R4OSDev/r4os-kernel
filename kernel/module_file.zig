const vfs = @import("../fs/vfs.zig");
const fs_request = @import("../fs/request.zig");
const k = @import("log.zig");
const drive = @import("../fs/drive.zig");

var execution_drive: ?u8 = null;

/// Boot-only admission policy, installed before userland starts. Normal R4OS
/// leaves it unset; a RAM runtime can restrict executable sources to itself.
pub fn restrictExecutionToDrive(letter: u8) void {
    execution_drive = letter;
}

pub fn executionDriveAllowed(letter: u8) bool {
    const required = execution_drive orelse return true;
    const mounted = drive.get(letter) orelse return false;
    return letter == required and mounted.role == .ram;
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

pub const Stats = struct {
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

var stats_state = Stats{};

pub fn stats() Stats {
    return stats_state;
}

pub fn readRange(req: RangeReadRequest) ?usize {
    if (req.out.len == 0) return 0;
    var fs_req = fs_request.beginVolume(.loader_read, req.source.drive_letter, req.source.volume) orelse {
        stats_state.read_failures += 1;
        logFailure(req.verbose, req.name, "request");
        return null;
    };
    var ok = false;
    const len = vfs.readFileRange(req.source.volume, req.source.entry, req.offset, req.out) orelse {
        fs_request.finish(&fs_req, ok);
        stats_state.read_failures += 1;
        logFailure(req.verbose, req.name, "read");
        return null;
    };
    ok = true;
    fs_request.finish(&fs_req, ok);
    stats_state.range_reads += 1;
    stats_state.range_read_bytes +|= len;
    return len;
}

pub fn readExact(req: RangeReadRequest) bool {
    const len = readRange(req) orelse return false;
    if (len != req.out.len) {
        noteShortRead(req.verbose, req.name, req.offset, req.out.len, len);
        return false;
    }
    return true;
}

const MetadataWindow = struct {
    valid: bool = false,
    offset: usize = 0,
    len: usize = 0,
    stamp: u64 = 0,
    bytes: [metadata_window_size]u8 = .{0} ** metadata_window_size,

    fn contains(self: *const MetadataWindow, offset: usize, len: usize) bool {
        if (!self.valid or offset < self.offset) return false;
        const local = offset - self.offset;
        return local <= self.len and len <= self.len - local;
    }
};

/// Bounded, caller-owned R4M0 metadata reader. It never allocates, never owns
/// file state globally and performs a direct exact read for requests larger
/// than one window. Callers keep one instance for one coherent load attempt.
pub const BoundedReader = struct {
    source: FileSource,
    file_size: usize,
    windows: [metadata_window_count]MetadataWindow = .{MetadataWindow{}} ** metadata_window_count,
    stamp: u64 = 0,

    pub fn init(source: FileSource, file_size: usize) BoundedReader {
        stats_state.metadata_reader_initializations +|= 1;
        return .{
            .source = source,
            .file_size = file_size,
        };
    }

    pub fn readExactAt(self: *BoundedReader, offset: usize, out: []u8, name: []const u8, verbose: bool) bool {
        stats_state.metadata_logical_reads +|= 1;
        stats_state.metadata_logical_bytes +|= out.len;
        if (out.len == 0) return offset <= self.file_size;
        if (offset > self.file_size or out.len > self.file_size - offset) {
            stats_state.read_failures +|= 1;
            logBoundsFailure(verbose, name, offset, out.len, self.file_size);
            return false;
        }

        if (out.len > metadata_window_size) {
            stats_state.metadata_direct_reads +|= 1;
            stats_state.metadata_direct_bytes +|= out.len;
            return readExact(.{
                .source = self.source,
                .offset = offset,
                .out = out,
                .name = name,
                .verbose = verbose,
            });
        }

        if (self.findWindow(offset, out.len)) |index| {
            const window = &self.windows[index];
            const local = offset - window.offset;
            @memcpy(out, window.bytes[local .. local + out.len]);
            stats_state.metadata_window_hits +|= 1;
            self.touch(index);
            return true;
        }

        const index = self.replacementWindow();
        const wanted = @min(metadata_window_size, self.file_size - offset);
        var window = &self.windows[index];
        window.valid = false;
        const len = readRange(.{
            .source = self.source,
            .offset = offset,
            .out = window.bytes[0..wanted],
            .name = name,
            .verbose = verbose,
        }) orelse return false;
        stats_state.metadata_window_fills +|= 1;
        stats_state.metadata_window_fill_bytes +|= len;
        if (len < out.len) {
            noteShortRead(verbose, name, offset, out.len, len);
            return false;
        }
        window.offset = offset;
        window.len = len;
        window.valid = true;
        self.touch(index);
        @memcpy(out, window.bytes[0..out.len]);
        return true;
    }

    fn findWindow(self: *const BoundedReader, offset: usize, len: usize) ?usize {
        for (&self.windows, 0..) |*window, index| {
            if (window.contains(offset, len)) return index;
        }
        return null;
    }

    fn replacementWindow(self: *const BoundedReader) usize {
        for (&self.windows, 0..) |*window, index| {
            if (!window.valid) return index;
        }
        var oldest: usize = 0;
        var index: usize = 1;
        while (index < self.windows.len) : (index += 1) {
            if (self.windows[index].stamp < self.windows[oldest].stamp) oldest = index;
        }
        return oldest;
    }

    fn touch(self: *BoundedReader, index: usize) void {
        self.stamp +%= 1;
        if (self.stamp == 0) {
            for (&self.windows) |*window| window.stamp = 0;
            self.stamp = 1;
        }
        self.windows[index].stamp = self.stamp;
    }
};

fn noteShortRead(verbose: bool, name: []const u8, offset: usize, wanted: usize, got: usize) void {
    stats_state.short_reads +|= 1;
    if (!verbose) return;
    k.puts("[MODFILE] short read ");
    k.puts(name);
    k.puts(" offset=");
    k.putDec(@intCast(offset));
    k.puts(" want=");
    k.putDec(@intCast(wanted));
    k.puts(" got=");
    k.putDec(@intCast(got));
    k.puts("\r\n");
}

fn logBoundsFailure(verbose: bool, name: []const u8, offset: usize, len: usize, file_size: usize) void {
    if (!verbose) return;
    k.puts("[MODFILE] range failed ");
    k.puts(name);
    k.puts(" offset=");
    k.putDec(@intCast(offset));
    k.puts(" len=");
    k.putDec(@intCast(len));
    k.puts(" file=");
    k.putDec(@intCast(file_size));
    k.puts("\r\n");
}

fn logFailure(verbose: bool, name: []const u8, phase: []const u8) void {
    if (!verbose) return;
    k.puts("[MODFILE] ");
    k.puts(phase);
    k.puts(" failed ");
    k.puts(name);
    k.puts("\r\n");
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
