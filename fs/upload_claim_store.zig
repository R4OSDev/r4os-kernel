// Durable store for create-only upload publish claims (0.60.22).
//
// Policy, format and replay rules live in the allocation-free shared
// `upload_publish_claim` module.  This file is only the VFS adapter: it makes
// a claim durable BEFORE the publish crosses its first visibility point, and
// removes it only after the hand-over reached a terminal state.
//
// Placement: all claims live in one directory on the system volume, even when
// the target sits on another volume.  That is safe because the ordering
// requirement is one-directional - the claim only has to be durable BEFORE
// the first visibility point of the publish.  A reset between the claim write
// and the publish leaves a claim whose replay simply observes `stage_only` or
// `missing` and cleans up.  Keeping one directory also means recovery does
// not have to scan whole volumes to find in-flight work.
//
// The write path is called with the filesystem-request gate already held by
// the publishing operation, so it must never open a new `fs_request`.

const upc = @import("upload_publish_claim");
const vfs = @import("vfs.zig");

pub const claim_directory = "/R4OS/UPDATE/CLAIMS";
const claim_root = "/R4OS";
const claim_parent = "/R4OS/UPDATE";
const claim_root_name = "UPDATE";
const claim_parent_name = "CLAIMS";
const claim_suffix = ".CLM";

/// Module-owned buffers.  Claims are written and replayed from kernel task
/// context, so none of this may live on a task stack.
var claim_text: [upc.claim_max]u8 = undefined;
var claim_read: [upc.claim_max]u8 = undefined;
var claim_record: upc.Claim = .{};
var replay_record: upc.Claim = .{};
var name_buf: [16]u8 = undefined;
var dir_entry_name: [vfs.NAME_MAX]u8 = undefined;

var next_generation: u64 = 1;

/// Stable volume token used by a claim.  Drive letters are aliases, so the
/// token has to come from the resolved backend volume, exactly like
/// `vfs.sameVolume`.
pub fn volumeToken(volume: vfs.Volume) u64 {
    return switch (volume) {
        .fat32 => |v| (@as(u64, 1) << 62) |
            (@as(u64, v.device_index) << 48) |
            @as(u64, v.partition_lba),
        .ntfs => |v| (@as(u64, 2) << 62) | @as(u64, v.state_slot),
    };
}

fn claimName(generation: u64) []const u8 {
    // Eight hex digits plus ".CLM" stays inside 8.3 so the same name works on
    // the FAT32 boot volume and on NTFS.
    const digits = "0123456789ABCDEF";
    var index: usize = 0;
    while (index < 8) : (index += 1) {
        const shift: u6 = @intCast((7 - index) * 4);
        name_buf[index] = digits[@as(usize, @intCast((generation >> shift) & 0xF))];
    }
    @memcpy(name_buf[8 .. 8 + claim_suffix.len], claim_suffix);
    return name_buf[0 .. 8 + claim_suffix.len];
}

fn systemVolume() ?vfs.Volume {
    return vfs.volumeForDrive('C');
}

/// Resolves the claim directory, creating it on demand.  A creation failure
/// is reported so the caller can refuse the publish instead of proceeding
/// without a recovery token.
fn claimDirectory(volume: vfs.Volume, out: *vfs.NodeRef) bool {
    switch (vfs.resolvePathStatus(volume, claim_directory, out)) {
        .found => return true,
        .io => return false,
        .not_found => {},
    }
    var parent: vfs.NodeRef = undefined;
    switch (vfs.resolvePathStatus(volume, claim_parent, &parent)) {
        .found => {},
        .io => return false,
        .not_found => {
            var root: vfs.NodeRef = undefined;
            if (vfs.resolvePathStatus(volume, claim_root, &root) != .found) return false;
            _ = vfs.makeDirectory(volume, root, claim_root_name);
            if (vfs.resolvePathStatus(volume, claim_parent, &parent) != .found) return false;
        },
    }
    _ = vfs.makeDirectory(volume, parent, claim_parent_name);
    return vfs.resolvePathStatus(volume, claim_directory, out) == .found;
}

pub const WriteResult = struct {
    ok: bool,
    generation: u64,
};

/// Makes a claim durable for a publish that is about to start.
///
/// Returns the claim generation so the caller can retire exactly this record
/// later.  A failure here must abort the publish: without a durable claim a
/// reset inside the hand-over would leave an object nothing can resolve.
pub fn beginPublish(
    target_volume: vfs.Volume,
    parent_path: []const u8,
    stage_name: []const u8,
    target_name: []const u8,
    backup_name: []const u8,
    identity: upc.FileIdentity,
    protocol: upc.Protocol,
) WriteResult {
    return writeClaim(target_volume, parent_path, stage_name, target_name, backup_name, identity, protocol, 0);
}

/// Rewrites an existing claim in place with a refreshed file identity
/// (0.60.30).
///
/// An empty FAT32 file has no cluster-chain identity yet, so a claim written
/// at stream-begin would carry node 0 and could never be matched against the
/// grown stage afterwards.  The stream write path already re-binds that
/// identity on the first append; this keeps the durable claim in step so
/// recovery can still recognize the object it owns.
pub fn refreshIdentity(
    target_volume: vfs.Volume,
    parent_path: []const u8,
    stage_name: []const u8,
    target_name: []const u8,
    backup_name: []const u8,
    identity: upc.FileIdentity,
    protocol: upc.Protocol,
    generation: u64,
) bool {
    if (generation == 0) return false;
    return writeClaim(target_volume, parent_path, stage_name, target_name, backup_name, identity, protocol, generation).ok;
}

fn writeClaim(
    target_volume: vfs.Volume,
    parent_path: []const u8,
    stage_name: []const u8,
    target_name: []const u8,
    backup_name: []const u8,
    identity: upc.FileIdentity,
    protocol: upc.Protocol,
    reuse_generation: u64,
) WriteResult {
    const volume = systemVolume() orelse return .{ .ok = false, .generation = 0 };
    var directory: vfs.NodeRef = undefined;
    if (!claimDirectory(volume, &directory)) return .{ .ok = false, .generation = 0 };

    const generation = if (reuse_generation != 0) reuse_generation else next_generation;
    if (!upc.build(
        &claim_record,
        generation,
        volumeToken(target_volume),
        parent_path,
        stage_name,
        target_name,
        backup_name,
        identity,
        protocol,
    )) return .{ .ok = false, .generation = 0 };

    const encoded = upc.serialize(&claim_record, claim_text[0..]) orelse
        return .{ .ok = false, .generation = 0 };
    if (!vfs.writeFile(volume, directory, claimName(generation), encoded))
        return .{ .ok = false, .generation = 0 };
    // The claim is only a recovery token once it survives a reset.
    if (!vfs.flushVolume(volume)) return .{ .ok = false, .generation = 0 };

    if (reuse_generation == 0) {
        next_generation +%= 1;
        if (next_generation == 0) next_generation = 1;
    }
    return .{ .ok = true, .generation = generation };
}

/// Removes a claim after its publish reached a terminal state.  Absence is
/// success: a lost acknowledgement must not keep the record alive.
fn retireInDirectory(volume: vfs.Volume, directory: vfs.NodeRef, generation: u64) bool {
    const name = claimName(generation);
    _ = vfs.deleteFile(volume, directory, name);
    if (!vfs.flushVolume(volume)) return false;
    var remaining: vfs.Entry = undefined;
    return vfs.lookupEntryStatus(volume, directory, name, &remaining) == .not_found;
}

pub fn retire(generation: u64) bool {
    if (generation == 0) return true;
    const volume = systemVolume() orelse return false;
    var directory: vfs.NodeRef = undefined;
    switch (vfs.resolvePathStatus(volume, claim_directory, &directory)) {
        .found => {},
        .not_found => return true,
        .io => return false,
    }
    return retireInDirectory(volume, directory, generation);
}

// ---------------------------------------------------------------------------
// Pre-runtime replay
// ---------------------------------------------------------------------------

/// VFS seam handed to the shared replay logic.
const ReplayIo = struct {
    claim_volume: vfs.Volume,
    claim_directory: vfs.NodeRef,

    fn resolveTarget(claim: *const upc.Claim, volume: *vfs.Volume, parent: *vfs.NodeRef) bool {
        const drive = claim.parentText();
        if (drive.len < 3 or drive[1] != ':') return false;
        const candidate = vfs.volumeForDrive(drive[0]) orelse return false;
        if (volumeToken(candidate) != claim.volume) return false;
        volume.* = candidate;
        // The claim stores a DOS path; the VFS wants the drive-relative part.
        return vfs.resolvePathStatus(candidate, drive[2..], parent) == .found;
    }

    fn identityOf(entry: vfs.Entry) upc.FileIdentity {
        return .{ .node = entry.node, .generation = entry.node_generation, .size = entry.size };
    }

    pub fn observe(self: *ReplayIo, claim: *const upc.Claim) upc.ObservedState {
        _ = self;
        var volume: vfs.Volume = undefined;
        var parent: vfs.NodeRef = undefined;
        // A claim that cannot be bound to its exact volume and parent any more
        // must never authorize a mutation.
        if (!resolveTarget(claim, &volume, &parent)) return .foreign;

        var stage: vfs.Entry = undefined;
        const stage_status = vfs.lookupRecoveryEntryStatus(volume, parent, claim.stageText(), &stage);
        if (stage_status == .io) return .io;
        var target: vfs.Entry = undefined;
        const target_status = vfs.lookupRecoveryEntryStatus(volume, parent, claim.targetText(), &target);
        if (target_status == .io) return .io;

        const have_stage = stage_status == .found;
        const have_target = target_status == .found;
        if (!have_stage and !have_target) return .missing;
        if (have_stage and !identityOf(stage).eql(claim.identity)) return .foreign;
        if (have_target and !identityOf(target).eql(claim.identity)) return .foreign;

        if (have_stage and have_target) {
            if (stage.node != target.node or stage.node_generation != target.node_generation) return .foreign;
            return .alias;
        }
        if (have_target) return .target_only;

        // Stage-only: a generic lookup still accepting the name proves the
        // canonical name is still the stage name, so the publish had not yet
        // passed its point of no return.  A generic rejection means the
        // canonical name is already the target (the 0.60.21 window).
        var generic: vfs.Entry = undefined;
        return switch (vfs.lookupEntryStatus(volume, parent, claim.stageText(), &generic)) {
            .found => .stage_only,
            .io => .half_published,
            .not_found => .io,
        };
    }

    pub fn publish(self: *ReplayIo, claim: *const upc.Claim) bool {
        _ = self;
        var volume: vfs.Volume = undefined;
        var parent: vfs.NodeRef = undefined;
        if (!resolveTarget(claim, &volume, &parent)) return false;
        const result = vfs.replaceFileAtomicCreateOnly(
            volume,
            parent,
            claim.targetText(),
            claim.stageText(),
            claim.backupText(),
        );
        return switch (result) {
            .ok => vfs.flushVolume(volume),
            else => false,
        };
    }

    pub fn discardStage(self: *ReplayIo, claim: *const upc.Claim) bool {
        _ = self;
        var volume: vfs.Volume = undefined;
        var parent: vfs.NodeRef = undefined;
        if (!resolveTarget(claim, &volume, &parent)) return false;
        var expected = vfs.Entry{};
        expected.node = claim.identity.node;
        expected.node_generation = claim.identity.generation;
        expected.size = claim.identity.size;
        return switch (vfs.deleteRecoveryEntryIfIdentity(volume, parent, claim.stageText(), expected)) {
            .deleted, .unlinked, .not_found => vfs.flushVolume(volume),
            else => false,
        };
    }

    pub fn retire(self: *ReplayIo, claim: *const upc.Claim) bool {
        return retireInDirectory(self.claim_volume, self.claim_directory, claim.generation);
    }
};

pub const ReplaySummary = upc.ReplaySummary;

/// Replays every durable claim before normal runtime consumers start.
///
/// A single failing claim never aborts the sweep: it stays on disk for the
/// next boot while the others still reach a terminal state.
pub fn recoverBeforeRuntime(summary: *ReplaySummary) bool {
    summary.* = .{};
    const volume = systemVolume() orelse return true;
    var directory: vfs.NodeRef = undefined;
    switch (vfs.resolvePathStatus(volume, claim_directory, &directory)) {
        .found => {},
        .not_found => return true,
        .io => return false,
    }

    var io = ReplayIo{ .claim_volume = volume, .claim_directory = directory };
    // Directory indices 0 and 1 are the synthetic dot entries.  They are not
    // part of the bounded claim namespace and must not consume its budget.
    var index: usize = 2;
    var visited: usize = 0;
    // Bounded on purpose: the claim namespace is capped, and an unbounded
    // sweep must never be able to stall the boot path.
    while (visited < upc.max_claims) {
        var entry: vfs.Entry = undefined;
        switch (vfs.readDirectoryEntryStatus(volume, directory, index, dir_entry_name[0..], &entry)) {
            .found => {},
            .not_found => break,
            .io => return false,
        }
        visited += 1;
        if (entry.isDir()) {
            index += 1;
            continue;
        }
        if (entry.size == 0 or entry.size > claim_read.len) {
            summary.invalid += 1;
            index += 1;
            continue;
        }
        const got = vfs.readFile(volume, entry, claim_read[0..@intCast(entry.size)]) orelse {
            summary.failed += 1;
            index += 1;
            continue;
        };
        if (!upc.parse(claim_read[0..got], &replay_record)) {
            summary.invalid += 1;
            index += 1;
            continue;
        }
        const replay_status = upc.replayClaim(&io, &replay_record);
        switch (replay_status) {
            .published => summary.published += 1,
            .rolled_back => summary.rolled_back += 1,
            .retired => summary.retired += 1,
            .foreign => summary.foreign += 1,
            .invalid => summary.invalid += 1,
            .io => summary.failed += 1,
        }
        // Successful replay retired this live directory entry. Its successor
        // shifted into the same logical index; failures remain and advance.
        switch (replay_status) {
            .published, .rolled_back, .retired, .foreign => {},
            .invalid, .io => index += 1,
        }
    }
    return true;
}
