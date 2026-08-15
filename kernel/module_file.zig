const vfs = @import("../fs/vfs.zig");
const fs_request = @import("../fs/request.zig");
const k = @import("log.zig");

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

pub const Stats = struct {
    active_buffers: u64 = 0,
    reserved_bytes: u64 = 0,
    committed_bytes: u64 = 0,
    peak_reserved_bytes: u64 = 0,
    peak_committed_bytes: u64 = 0,
    full_reads: u64 = 0,
    range_reads: u64 = 0,
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
    var fs_req = fs_request.begin(.loader_read, req.source.drive_letter) orelse {
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
    return len;
}

pub fn readExact(req: RangeReadRequest) bool {
    const len = readRange(req) orelse return false;
    if (len != req.out.len) {
        stats_state.short_reads += 1;
        if (req.verbose) {
            k.puts("[MODFILE] short read ");
            k.puts(req.name);
            k.puts(" want=");
            k.putDec(@intCast(req.out.len));
            k.puts(" got=");
            k.putDec(@intCast(len));
            k.puts("\r\n");
        }
        return false;
    }
    return true;
}

fn logFailure(verbose: bool, name: []const u8, phase: []const u8) void {
    if (!verbose) return;
    k.puts("[MODFILE] ");
    k.puts(phase);
    k.puts(" failed ");
    k.puts(name);
    k.puts("\r\n");
}
