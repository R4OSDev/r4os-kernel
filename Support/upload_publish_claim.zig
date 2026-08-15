// Persistent create-only upload publish claim (0.60.22).
//
// SFTP, SCP and FTP all publish an upload the same way: the payload is
// written under a private stage name and then handed over to its target name
// by the create-only alias publisher.  That hand-over has durable windows in
// which a reset loses the only knowledge of what was going on:
//
//   stage-only            the stage object exists, nothing was published yet
//   half-published        the canonical $FILE_NAME already names the target
//                         while the index still only carries the stage name
//                         (the 0.60.21 window)
//   target+stage alias    both names index the same object
//   target-only           the hand-over is complete
//   missing               nothing of the operation is left
//
// Before 0.60.22 the recovery token lived only in RAM, so a power loss inside
// those windows left an object that no later run could confidently finish or
// remove.  This module is the persistent counterpart: a small text claim is
// made durable BEFORE the first durable visibility point and removed only
// after the terminal stage detach.
//
// Design notes:
//   * The claim is a TEXT record, exactly like the SYSUPD journal (0.60.19).
//     Field order is fixed, unknown trailing fields are rejected, and a
//     checksum covers the body, so a torn write is never mistaken for a
//     valid claim.
//   * Every decision is bound to volume, parent, all three names AND the
//     exact backend file identity.  A merely equal name never satisfies a
//     replay; that is what makes "foreign" a first-class outcome instead of
//     an accidental delete.
//   * All filesystem access goes through a caller-supplied `io` seam, so the
//     host model drives the REAL replay logic without an R4OS runtime.

const std = @import("std");

pub const claim_magic = "R4U_CLAIM=1";
pub const max_path: usize = 1024;
pub const max_name: usize = 768;
pub const max_claims: usize = 16;
pub const claim_max: usize = 4096;
pub const checksum_seed: u32 = 2166136261;

/// What the protocol layer asked for.  Only used for telemetry and visible
/// error attribution; the replay rules are identical for every producer.
pub const Protocol = enum(u8) {
    sftp,
    scp,
    ftp,
    storage,

    pub fn text(self: Protocol) []const u8 {
        return switch (self) {
            .sftp => "sftp",
            .scp => "scp",
            .ftp => "ftp",
            .storage => "storage",
        };
    }

    pub fn parse(value: []const u8) ?Protocol {
        if (std.mem.eql(u8, value, "sftp")) return .sftp;
        if (std.mem.eql(u8, value, "scp")) return .scp;
        if (std.mem.eql(u8, value, "ftp")) return .ftp;
        if (std.mem.eql(u8, value, "storage")) return .storage;
        return null;
    }
};

/// Backend-neutral file identity.  NTFS uses {record, sequence}; FAT32 uses
/// {first cluster, 0} and needs the size as well because an empty file has no
/// cluster chain of its own.
pub const FileIdentity = struct {
    node: u64 = 0,
    generation: u16 = 0,
    size: u64 = 0,

    pub fn eql(a: FileIdentity, b: FileIdentity) bool {
        return a.node == b.node and a.generation == b.generation and a.size == b.size;
    }
};

/// The durable state a replay found on disk.
pub const ObservedState = enum {
    /// Only the stage name resolves and it is still canonically the stage.
    stage_only,
    /// Only the stage name resolves, but the record already names the target
    /// canonically: the publish passed its point of no return.
    half_published,
    /// Both names index the same object.
    alias,
    /// Only the target name resolves and it is our object.
    target_only,
    /// Neither name resolves.
    missing,
    /// Something is there, but it is not the object this claim owns.
    foreign,
    io,
};

/// Result of driving one claim to a terminal state.
pub const ReplayStatus = enum {
    /// The publish was completed forward; the target owns the payload.
    published,
    /// The publish was rolled back; the stage object is gone.
    rolled_back,
    /// Nothing was owned any more; the claim was simply retired.
    retired,
    /// A foreign object holds the name.  Nothing was touched and the claim
    /// is retired with visible telemetry.
    foreign,
    /// The claim itself is unusable (torn, wrong magic, bad checksum).
    invalid,
    io,
};

pub fn replayStatusName(status: ReplayStatus) []const u8 {
    return switch (status) {
        .published => "published",
        .rolled_back => "rolled-back",
        .retired => "retired",
        .foreign => "foreign",
        .invalid => "invalid",
        .io => "io",
    };
}

pub fn observedStateName(state: ObservedState) []const u8 {
    return switch (state) {
        .stage_only => "stage-only",
        .half_published => "half-published",
        .alias => "alias",
        .target_only => "target-only",
        .missing => "missing",
        .foreign => "foreign",
        .io => "io",
    };
}

/// One in-flight create-only publish.
pub const Claim = struct {
    generation: u64 = 0,
    /// Stable mounted-volume token; a claim never acts across volumes.
    volume: u64 = 0,
    parent_path: [max_path]u8 = .{0} ** max_path,
    parent_len: usize = 0,
    stage_name: [max_name]u8 = .{0} ** max_name,
    stage_len: usize = 0,
    target_name: [max_name]u8 = .{0} ** max_name,
    target_len: usize = 0,
    backup_name: [max_name]u8 = .{0} ** max_name,
    backup_len: usize = 0,
    identity: FileIdentity = .{},
    protocol: Protocol = .sftp,

    pub fn parentText(self: *const Claim) []const u8 {
        return self.parent_path[0..self.parent_len];
    }
    pub fn stageText(self: *const Claim) []const u8 {
        return self.stage_name[0..self.stage_len];
    }
    pub fn targetText(self: *const Claim) []const u8 {
        return self.target_name[0..self.target_len];
    }
    pub fn backupText(self: *const Claim) []const u8 {
        return self.backup_name[0..self.backup_len];
    }
    pub fn hasBackup(self: *const Claim) bool {
        return self.backup_len != 0;
    }
};

pub fn checksum(data: []const u8) u32 {
    var hash: u32 = checksum_seed;
    for (data) |byte| {
        hash ^= byte;
        hash = hash *% 16777619;
    }
    return hash;
}

fn setField(out: []u8, len: *usize, value: []const u8) bool {
    if (value.len > out.len) return false;
    @memset(out, 0);
    @memcpy(out[0..value.len], value);
    len.* = value.len;
    return true;
}

/// A claim name must be a single path component: recovery resolves it inside
/// the recorded parent, so a separator or a traversal element would silently
/// change directory.
pub fn nameValid(value: []const u8) bool {
    if (value.len == 0 or value.len > max_name) return false;
    if (std.mem.eql(u8, value, ".") or std.mem.eql(u8, value, "..")) return false;
    for (value) |c| {
        if (c == '\\' or c == '/' or c == ':' or c == 0) return false;
    }
    return true;
}

pub fn parentValid(value: []const u8) bool {
    if (value.len == 0 or value.len > max_path) return false;
    // Absolute drive-rooted path, e.g. C:\R4OS\UPDATE\INBOX
    if (value.len < 3 or value[1] != ':' or value[2] != '\\') return false;
    if (std.mem.indexOf(u8, value, "..") != null) return false;
    for (value) |c| {
        if (c == 0 or c == '/') return false;
    }
    return true;
}

pub fn claimValid(claim: *const Claim) bool {
    if (claim.generation == 0) return false;
    if (!parentValid(claim.parentText())) return false;
    if (!nameValid(claim.stageText())) return false;
    if (!nameValid(claim.targetText())) return false;
    if (claim.backup_len != 0 and !nameValid(claim.backupText())) return false;
    // Stage and target must be distinct names; an alias would make the
    // hand-over meaningless and the replay ambiguous.
    if (eqlIgnoreAsciiCase(claim.stageText(), claim.targetText())) return false;
    if (claim.backup_len != 0 and
        (eqlIgnoreAsciiCase(claim.backupText(), claim.stageText()) or
            eqlIgnoreAsciiCase(claim.backupText(), claim.targetText()))) return false;
    return true;
}

pub fn eqlIgnoreAsciiCase(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    for (a, b) |x, y| {
        const lx = if (x >= 'A' and x <= 'Z') x + 32 else x;
        const ly = if (y >= 'A' and y <= 'Z') y + 32 else y;
        if (lx != ly) return false;
    }
    return true;
}

pub fn build(
    out: *Claim,
    generation: u64,
    volume: u64,
    parent: []const u8,
    stage: []const u8,
    target: []const u8,
    backup: []const u8,
    identity: FileIdentity,
    protocol: Protocol,
) bool {
    out.* = .{};
    out.generation = generation;
    out.volume = volume;
    out.identity = identity;
    out.protocol = protocol;
    if (!setField(out.parent_path[0..], &out.parent_len, parent)) return false;
    if (!setField(out.stage_name[0..], &out.stage_len, stage)) return false;
    if (!setField(out.target_name[0..], &out.target_len, target)) return false;
    if (!setField(out.backup_name[0..], &out.backup_len, backup)) return false;
    return claimValid(out);
}

// ---------------------------------------------------------------------------
// Text serialization
// ---------------------------------------------------------------------------

fn appendLine(out: []u8, cursor: *usize, key: []const u8, value: []const u8) bool {
    const need = key.len + 1 + value.len + 1;
    if (cursor.* + need > out.len) return false;
    @memcpy(out[cursor.* .. cursor.* + key.len], key);
    cursor.* += key.len;
    out[cursor.*] = '=';
    cursor.* += 1;
    @memcpy(out[cursor.* .. cursor.* + value.len], value);
    cursor.* += value.len;
    out[cursor.*] = '\n';
    cursor.* += 1;
    return true;
}

fn appendNumber(out: []u8, cursor: *usize, key: []const u8, value: u64) bool {
    var digits: [20]u8 = undefined;
    var written: usize = 0;
    if (value == 0) {
        digits[0] = '0';
        written = 1;
    } else {
        var rest = value;
        while (rest != 0) : (rest /= 10) {
            digits[written] = @intCast('0' + (rest % 10));
            written += 1;
        }
        var i: usize = 0;
        while (i < written / 2) : (i += 1) {
            const tmp = digits[i];
            digits[i] = digits[written - 1 - i];
            digits[written - 1 - i] = tmp;
        }
    }
    return appendLine(out, cursor, key, digits[0..written]);
}

fn parseNumber(value: []const u8) ?u64 {
    if (value.len == 0 or value.len > 20) return null;
    var result: u64 = 0;
    for (value) |c| {
        if (c < '0' or c > '9') return null;
        const digit: u64 = c - '0';
        if (result > (std.math.maxInt(u64) - digit) / 10) return null;
        result = result * 10 + digit;
    }
    return result;
}

/// Serializes a claim.  The trailing `checksum=` line covers everything
/// before it, so a torn tail can never parse as a complete claim.
pub fn serialize(claim: *const Claim, out: []u8) ?[]const u8 {
    if (!claimValid(claim)) return null;
    var cursor: usize = 0;
    if (!appendLine(out, &cursor, "magic", claim_magic)) return null;
    if (!appendNumber(out, &cursor, "generation", claim.generation)) return null;
    if (!appendNumber(out, &cursor, "volume", claim.volume)) return null;
    if (!appendLine(out, &cursor, "parent", claim.parentText())) return null;
    if (!appendLine(out, &cursor, "stage", claim.stageText())) return null;
    if (!appendLine(out, &cursor, "target", claim.targetText())) return null;
    if (!appendLine(out, &cursor, "backup", claim.backupText())) return null;
    if (!appendNumber(out, &cursor, "node", claim.identity.node)) return null;
    if (!appendNumber(out, &cursor, "nodegen", claim.identity.generation)) return null;
    if (!appendNumber(out, &cursor, "size", claim.identity.size)) return null;
    if (!appendLine(out, &cursor, "protocol", claim.protocol.text())) return null;
    const body = out[0..cursor];
    if (!appendNumber(out, &cursor, "checksum", checksum(body))) return null;
    return out[0..cursor];
}

fn nextLine(data: []const u8, cursor: *usize) ?[]const u8 {
    if (cursor.* >= data.len) return null;
    const start = cursor.*;
    var end = start;
    while (end < data.len and data[end] != '\n') : (end += 1) {}
    if (end >= data.len) return null; // unterminated tail: torn write
    cursor.* = end + 1;
    return data[start..end];
}

fn splitField(line: []const u8, key: []const u8) ?[]const u8 {
    if (line.len < key.len + 1) return null;
    if (!std.mem.eql(u8, line[0..key.len], key)) return null;
    if (line[key.len] != '=') return null;
    return line[key.len + 1 ..];
}

pub fn parse(data: []const u8, out: *Claim) bool {
    out.* = .{};
    if (data.len > claim_max) return false;
    var cursor: usize = 0;

    const magic_line = nextLine(data, &cursor) orelse return false;
    const magic = splitField(magic_line, "magic") orelse return false;
    if (!std.mem.eql(u8, magic, claim_magic)) return false;

    const gen_line = nextLine(data, &cursor) orelse return false;
    out.generation = parseNumber(splitField(gen_line, "generation") orelse return false) orelse return false;

    const vol_line = nextLine(data, &cursor) orelse return false;
    out.volume = parseNumber(splitField(vol_line, "volume") orelse return false) orelse return false;

    const parent_line = nextLine(data, &cursor) orelse return false;
    if (!setField(out.parent_path[0..], &out.parent_len, splitField(parent_line, "parent") orelse return false)) return false;

    const stage_line = nextLine(data, &cursor) orelse return false;
    if (!setField(out.stage_name[0..], &out.stage_len, splitField(stage_line, "stage") orelse return false)) return false;

    const target_line = nextLine(data, &cursor) orelse return false;
    if (!setField(out.target_name[0..], &out.target_len, splitField(target_line, "target") orelse return false)) return false;

    const backup_line = nextLine(data, &cursor) orelse return false;
    if (!setField(out.backup_name[0..], &out.backup_len, splitField(backup_line, "backup") orelse return false)) return false;

    const node_line = nextLine(data, &cursor) orelse return false;
    out.identity.node = parseNumber(splitField(node_line, "node") orelse return false) orelse return false;

    const nodegen_line = nextLine(data, &cursor) orelse return false;
    const nodegen = parseNumber(splitField(nodegen_line, "nodegen") orelse return false) orelse return false;
    if (nodegen > std.math.maxInt(u16)) return false;
    out.identity.generation = @intCast(nodegen);

    const size_line = nextLine(data, &cursor) orelse return false;
    out.identity.size = parseNumber(splitField(size_line, "size") orelse return false) orelse return false;

    const protocol_line = nextLine(data, &cursor) orelse return false;
    out.protocol = Protocol.parse(splitField(protocol_line, "protocol") orelse return false) orelse return false;

    const body_end = cursor;
    const checksum_line = nextLine(data, &cursor) orelse return false;
    const want = parseNumber(splitField(checksum_line, "checksum") orelse return false) orelse return false;
    if (want > std.math.maxInt(u32)) return false;
    if (checksum(data[0..body_end]) != @as(u32, @intCast(want))) return false;

    // Trailing content beyond the checksum line is not a claim we understand.
    if (cursor != data.len) return false;
    return claimValid(out);
}

// ---------------------------------------------------------------------------
// Replay
// ---------------------------------------------------------------------------
//
// The `io` seam must provide:
//
//   observe(claim) ObservedState
//       Resolves stage and target inside the claim's parent on the claim's
//       volume and classifies the durable state.  Every identity comparison
//       is the caller's job, so an equal name with a different identity has
//       to come back as `.foreign`.
//   publish(claim) bool      complete the hand-over (create-only publisher)
//   discardStage(claim) bool identity-bound removal of the stage object
//   retire(claim) bool       delete the persistent claim record
//
// Each returns false only for a real I/O failure, which keeps the claim.

/// Drives exactly one claim to a terminal state.
///
/// The rule is deliberately asymmetric around the point of no return: before
/// the canonical rewrite an unfinished upload is rolled back, after it the
/// hand-over is completed.  That is what makes a lost acknowledgement safe -
/// the client either sees the file or does not, never a half object.
pub fn replayClaim(io: anytype, claim: *const Claim) ReplayStatus {
    if (!claimValid(claim)) return .invalid;

    return switch (io.observe(claim)) {
        .io => .io,

        // Nothing durable was published yet: the upload is abandoned.
        .stage_only => blk: {
            if (!io.discardStage(claim)) break :blk .io;
            break :blk if (io.retire(claim)) .rolled_back else .io;
        },

        // Past the point of no return: finish what the publisher started.
        // This is the 0.60.21 window and needs the recovery-only mutation.
        .half_published, .alias => blk: {
            if (!io.publish(claim)) break :blk .io;
            break :blk if (io.retire(claim)) .published else .io;
        },

        // Already complete.
        .target_only => if (io.retire(claim)) .published else .io,

        // Nothing of ours is left; the claim is just bookkeeping now.
        .missing => if (io.retire(claim)) .retired else .io,

        // Someone else owns the name.  Never touch it - only drop the claim
        // so it cannot authorize a future delete.
        .foreign => if (io.retire(claim)) .foreign else .io,
    };
}

pub const ReplaySummary = struct {
    published: usize = 0,
    rolled_back: usize = 0,
    retired: usize = 0,
    foreign: usize = 0,
    invalid: usize = 0,
    failed: usize = 0,

    pub fn total(self: ReplaySummary) usize {
        return self.published + self.rolled_back + self.retired +
            self.foreign + self.invalid + self.failed;
    }
};

/// Replays a bounded set of claims.  A single failing claim never aborts the
/// sweep: it stays on disk for the next attempt while the others still reach
/// a terminal state.
pub fn replayAll(io: anytype, claims: []const Claim) ReplaySummary {
    var summary = ReplaySummary{};
    for (claims) |*claim| {
        switch (replayClaim(io, claim)) {
            .published => summary.published += 1,
            .rolled_back => summary.rolled_back += 1,
            .retired => summary.retired += 1,
            .foreign => summary.foreign += 1,
            .invalid => summary.invalid += 1,
            .io => summary.failed += 1,
        }
    }
    return summary;
}

// ---------------------------------------------------------------------------
// Tests (run by the 0.60.22 gate and by `zig test` on this module)
// ---------------------------------------------------------------------------

const testing = std.testing;

fn sampleClaim() Claim {
    var claim: Claim = undefined;
    _ = build(
        &claim,
        7,
        0x1234,
        "C:\\R4OS\\UPDATE\\INBOX",
        "UPLOAD.STG",
        "PACKAGE.R4U",
        "PACKAGE.BAK",
        .{ .node = 42, .generation = 3, .size = 8_388_608 },
        .sftp,
    );
    return claim;
}

test "claim survives a serialize/parse roundtrip" {
    const claim = sampleClaim();
    var buf: [claim_max]u8 = undefined;
    const text = serialize(&claim, buf[0..]) orelse return error.SerializeFailed;

    var back: Claim = undefined;
    try testing.expect(parse(text, &back));
    try testing.expectEqual(claim.generation, back.generation);
    try testing.expectEqual(claim.volume, back.volume);
    try testing.expectEqualStrings(claim.parentText(), back.parentText());
    try testing.expectEqualStrings(claim.stageText(), back.stageText());
    try testing.expectEqualStrings(claim.targetText(), back.targetText());
    try testing.expectEqualStrings(claim.backupText(), back.backupText());
    try testing.expect(claim.identity.eql(back.identity));
    try testing.expectEqual(claim.protocol, back.protocol);
}

test "every producer attribution survives the durable format" {
    const protocols = [_]Protocol{ .sftp, .scp, .ftp, .storage };
    for (protocols) |protocol| {
        var claim = sampleClaim();
        claim.protocol = protocol;
        var buf: [claim_max]u8 = undefined;
        const text = serialize(&claim, buf[0..]) orelse return error.SerializeFailed;
        var back: Claim = undefined;
        try testing.expect(parse(text, &back));
        try testing.expectEqual(protocol, back.protocol);
    }
}

test "a torn or corrupted claim never parses" {
    const claim = sampleClaim();
    var buf: [claim_max]u8 = undefined;
    const text = serialize(&claim, buf[0..]) orelse return error.SerializeFailed;
    var back: Claim = undefined;

    // Every truncation is rejected: the checksum line is the only terminator.
    var cut: usize = 0;
    while (cut < text.len) : (cut += 1) {
        try testing.expect(!parse(text[0..cut], &back));
    }
    // A flipped body byte breaks the checksum.
    var damaged: [claim_max]u8 = undefined;
    @memcpy(damaged[0..text.len], text);
    damaged[text.len / 2] ^= 0x20;
    try testing.expect(!parse(damaged[0..text.len], &back));
    // Trailing junk is not accepted either.
    @memcpy(damaged[0..text.len], text);
    damaged[text.len] = 'x';
    try testing.expect(!parse(damaged[0 .. text.len + 1], &back));
}

test "structurally impossible claims are refused" {
    var claim: Claim = undefined;
    // stage == target would make the hand-over meaningless
    try testing.expect(!build(&claim, 1, 1, "C:\\X", "A.STG", "A.STG", "", .{}, .sftp));
    // names must stay inside the recorded parent
    try testing.expect(!build(&claim, 1, 1, "C:\\X", "sub\\A.STG", "A.BIN", "", .{}, .sftp));
    try testing.expect(!build(&claim, 1, 1, "C:\\X", "..", "A.BIN", "", .{}, .sftp));
    // parent must be absolute and traversal free
    try testing.expect(!build(&claim, 1, 1, "R4OS\\X", "A.STG", "A.BIN", "", .{}, .sftp));
    try testing.expect(!build(&claim, 1, 1, "C:\\X\\..\\Y", "A.STG", "A.BIN", "", .{}, .sftp));
    // generation zero is never a live claim
    try testing.expect(!build(&claim, 0, 1, "C:\\X", "A.STG", "A.BIN", "", .{}, .sftp));
    // a backup aliasing one of the other names is rejected
    try testing.expect(!build(&claim, 1, 1, "C:\\X", "A.STG", "A.BIN", "a.bin", .{}, .sftp));
}
