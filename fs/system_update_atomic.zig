// Checked SYSUPD ownership transition.
//
// Callers must hold the target volume's fs_request gate (or run during the
// single-threaded pre-runtime recovery phase).  Lookup, full-content
// fingerprints, transient alias identity, the backend ownership transfer,
// cleanup and final verification consequently form one namespace-critical
// operation.  No unverified name is ever passed to a mutating backend.

const std = @import("std");
const recovery = @import("system_update_recovery");
const fs_request = @import("request.zig");
const scheduler = @import("../sched/scheduler.zig");
const vfs = @import("vfs.zig");

pub const Direction = enum(u8) {
    forward,
    rollback,
};

pub const Result = enum(u8) {
    ok,
    conflict,
    io,
    not_atomic,
};

pub const Spec = struct {
    target_existed: bool,
    old_known: bool,
    new_size: u64,
    new_checksum: u32,
    old_size: u64 = 0,
    old_checksum: u32 = 0,
};

const Content = enum(u8) {
    missing,
    new,
    old,
    other,
};

const Observed = struct {
    entry: ?vfs.Entry = null,
    content: Content = .missing,
};

var checksum_buffer: [65536]u8 = undefined;

pub fn transition(
    volume: vfs.Volume,
    parent: vfs.NodeRef,
    target_name: []const u8,
    stage_name: []const u8,
    backup_name: []const u8,
    direction: Direction,
    spec: Spec,
) Result {
    if (target_name.len == 0 or stage_name.len == 0 or backup_name.len == 0 or
        !vfs.validateShortName83(stage_name) or
        !vfs.validateShortName83(backup_name))
    {
        return .conflict;
    }
    if (nameEqual(target_name, stage_name) or
        nameEqual(target_name, backup_name) or
        nameEqual(stage_name, backup_name))
    {
        return .conflict;
    }
    if (!spec.target_existed and spec.old_known) return .conflict;

    const target = observe(volume, parent, target_name, spec) orelse return .io;
    const stage = observe(volume, parent, stage_name, spec) orelse return .io;
    const backup = observe(volume, parent, backup_name, spec) orelse return .io;

    if (spec.target_existed) {
        if (!existingStateAllowed(volume, target, stage, backup, spec)) return .conflict;
        if (!spec.old_known) {
            // Legacy journals did not persist the old fingerprint.  They may
            // finish an already-restored state, but may never publish an
            // unidentifiable backup over a missing/new target.
            if (target.content != .other) return .conflict;
            return finishLegacyRollback(
                volume,
                parent,
                target_name,
                stage_name,
                backup_name,
                target,
                stage,
                backup,
                spec,
            );
        }
        return switch (direction) {
            .forward => finishExistingForward(
                volume,
                parent,
                target_name,
                stage_name,
                backup_name,
                target,
                stage,
                backup,
                spec,
            ),
            .rollback => finishExistingRollback(
                volume,
                parent,
                target_name,
                stage_name,
                backup_name,
                target,
                stage,
                backup,
                spec,
            ),
        };
    }

    if (!createStateAllowed(volume, target, stage, backup)) return .conflict;
    return switch (direction) {
        .forward => finishCreateForward(
            volume,
            parent,
            target_name,
            stage_name,
            backup_name,
            target,
            stage,
            spec,
        ),
        .rollback => finishCreateRollback(
            volume,
            parent,
            target_name,
            stage_name,
            backup_name,
            target,
            stage,
            spec,
        ),
    };
}

fn existingStateAllowed(
    volume: vfs.Volume,
    target: Observed,
    stage: Observed,
    backup: Observed,
    spec: Spec,
) bool {
    if (stage.content != .missing and stage.content != .new) return false;
    if (spec.old_known) {
        if (backup.content != .missing and backup.content != .old) return false;
        switch (target.content) {
            .old => {
                if (backup.entry) |backup_entry| {
                    const target_entry = target.entry orelse return false;
                    if (!vfs.sameFileRecoveryIdentity(volume, target_entry, backup_entry)) return false;
                }
                return true;
            },
            .missing => return backup.content == .old and stage.content == .new,
            .new => {
                if (backup.content != .old) return false;
                if (stage.entry) |stage_entry| {
                    const target_entry = target.entry orelse return false;
                    if (!vfs.sameFileRecoveryIdentity(volume, target_entry, stage_entry)) return false;
                }
                return true;
            },
            .other => return false,
        }
    }

    // A legacy old object is any regular file which is demonstrably not N.
    if (target.content != .other) return false;
    if (backup.entry) |backup_entry| {
        const target_entry = target.entry orelse return false;
        if (backup.content != .other or
            !vfs.sameFileRecoveryIdentity(volume, target_entry, backup_entry))
        {
            return false;
        }
    }
    return true;
}

fn createStateAllowed(
    volume: vfs.Volume,
    target: Observed,
    stage: Observed,
    backup: Observed,
) bool {
    if (backup.content != .missing) return false;
    if (target.content != .missing and target.content != .new) return false;
    if (stage.content != .missing and stage.content != .new) return false;
    if (target.entry != null and stage.entry != null and
        !vfs.sameFileRecoveryIdentity(volume, target.entry.?, stage.entry.?))
    {
        return false;
    }
    // Both missing is the already-rolled-back terminal state only.
    return true;
}

fn finishExistingForward(
    volume: vfs.Volume,
    parent: vfs.NodeRef,
    target_name: []const u8,
    stage_name: []const u8,
    backup_name: []const u8,
    target: Observed,
    stage: Observed,
    backup: Observed,
    spec: Spec,
) Result {
    if (!(target.content == .new and
        stage.content == .missing and
        backup.content == .old))
    {
        const changed = mutateReplace(volume, parent, target_name, stage_name, backup_name);
        if (changed != .ok) return changed;
    }
    return verifyExistingTerminal(
        volume,
        parent,
        target_name,
        stage_name,
        backup_name,
        .forward,
        spec,
    );
}

fn finishExistingRollback(
    volume: vfs.Volume,
    parent: vfs.NodeRef,
    target_name: []const u8,
    stage_name: []const u8,
    backup_name: []const u8,
    target: Observed,
    stage: Observed,
    backup: Observed,
    spec: Spec,
) Result {
    if (target.content == .old and backup.content == .missing) {
        // Pre-visibility rollback: the original target is already canonical;
        // terminal verification below removes only the fingerprinted N stage.
        return verifyExistingTerminal(
            volume,
            parent,
            target_name,
            stage_name,
            backup_name,
            .rollback,
            spec,
        );
    }

    // A crash in the first ownership leg can leave T=O and B=O aliases (or
    // NTFS can transiently detach T). Reversing directly would use the still
    // live N stage as the reverse operation's backup name and conflict. First
    // finish the checked forward transition to T=N,S=-,B=O, then reverse that
    // single canonical state.
    const forward = finishExistingForward(
        volume,
        parent,
        target_name,
        stage_name,
        backup_name,
        target,
        stage,
        backup,
        spec,
    );
    if (forward != .ok) return forward;
    const changed = mutateReplace(volume, parent, target_name, backup_name, stage_name);
    if (changed != .ok) return changed;
    return verifyExistingTerminal(
        volume,
        parent,
        target_name,
        stage_name,
        backup_name,
        .rollback,
        spec,
    );
}

fn finishLegacyRollback(
    volume: vfs.Volume,
    parent: vfs.NodeRef,
    target_name: []const u8,
    stage_name: []const u8,
    backup_name: []const u8,
    target: Observed,
    stage: Observed,
    backup: Observed,
    spec: Spec,
) Result {
    _ = target;
    if (backup.entry != null) {
        // As in the fingerprinted path, an O/O target-backup alias cannot be
        // reversed while N still occupies the reverse backup name. Complete
        // the forward leg first; the observed backup identity then preserves
        // the otherwise unknown legacy O object through the reverse leg.
        if (stage.content != .new) return .conflict;
        const forward = mutateReplace(volume, parent, target_name, stage_name, backup_name);
        if (forward != .ok) return forward;
        const reverse = mutateReplace(volume, parent, target_name, backup_name, stage_name);
        if (reverse != .ok) return reverse;
    }
    var target_after = observe(volume, parent, target_name, spec) orelse return .io;
    var stage_after = observe(volume, parent, stage_name, spec) orelse return .io;
    const backup_after = observe(volume, parent, backup_name, spec) orelse return .io;
    if (target_after.content != .other or backup_after.content != .missing) return .conflict;
    if (stage_after.entry) |entry| {
        if (stage_after.content != .new) return .conflict;
        const removed = deleteObserved(volume, parent, stage_name, entry);
        if (removed != .ok) return removed;
    }
    target_after = observe(volume, parent, target_name, spec) orelse return .io;
    stage_after = observe(volume, parent, stage_name, spec) orelse return .io;
    return if (target_after.content == .other and stage_after.content == .missing and
        vfs.flushVolume(volume))
        .ok
    else
        .conflict;
}

fn finishCreateForward(
    volume: vfs.Volume,
    parent: vfs.NodeRef,
    target_name: []const u8,
    stage_name: []const u8,
    backup_name: []const u8,
    target: Observed,
    stage: Observed,
    spec: Spec,
) Result {
    if (!(target.content == .new and stage.content == .missing)) {
        const changed = mutateReplace(volume, parent, target_name, stage_name, backup_name);
        if (changed != .ok) return changed;
    }
    const target_after = observe(volume, parent, target_name, spec) orelse return .io;
    const stage_after = observe(volume, parent, stage_name, spec) orelse return .io;
    const backup_after = observe(volume, parent, backup_name, spec) orelse return .io;
    return if (target_after.content == .new and
        stage_after.content == .missing and
        backup_after.content == .missing and
        vfs.flushVolume(volume))
        .ok
    else
        .conflict;
}

fn finishCreateRollback(
    volume: vfs.Volume,
    parent: vfs.NodeRef,
    target_name: []const u8,
    stage_name: []const u8,
    backup_name: []const u8,
    target: Observed,
    stage: Observed,
    spec: Spec,
) Result {
    if (target.entry != null and stage.entry != null) {
        // Detach the stage alias without freeing the shared object.  Only
        // then may the remaining target owner be deleted.
        const detached = mutateReplace(volume, parent, target_name, stage_name, backup_name);
        if (detached != .ok) return detached;
    }

    var target_after = observe(volume, parent, target_name, spec) orelse return .io;
    var stage_after = observe(volume, parent, stage_name, spec) orelse return .io;
    const backup_after = observe(volume, parent, backup_name, spec) orelse return .io;
    if (backup_after.content != .missing) return .conflict;
    if (target_after.entry) |entry| {
        if (target_after.content != .new) return .conflict;
        const removed = deleteObserved(volume, parent, target_name, entry);
        if (removed != .ok) return removed;
    }
    if (stage_after.entry) |entry| {
        if (stage_after.content != .new) return .conflict;
        const removed = deleteObserved(volume, parent, stage_name, entry);
        if (removed != .ok) return removed;
    }
    target_after = observe(volume, parent, target_name, spec) orelse return .io;
    stage_after = observe(volume, parent, stage_name, spec) orelse return .io;
    return if (target_after.content == .missing and
        stage_after.content == .missing and
        vfs.flushVolume(volume))
        .ok
    else
        .conflict;
}

fn verifyExistingTerminal(
    volume: vfs.Volume,
    parent: vfs.NodeRef,
    target_name: []const u8,
    stage_name: []const u8,
    backup_name: []const u8,
    direction: Direction,
    spec: Spec,
) Result {
    var target = observe(volume, parent, target_name, spec) orelse return .io;
    var stage = observe(volume, parent, stage_name, spec) orelse return .io;
    const backup = observe(volume, parent, backup_name, spec) orelse return .io;
    switch (direction) {
        .forward => {
            if (target.content != .new or stage.content != .missing or backup.content != .old)
                return .conflict;
        },
        .rollback => {
            if (target.content != .old or backup.content != .missing) return .conflict;
            if (stage.entry) |entry| {
                if (stage.content != .new) return .conflict;
                const removed = deleteObserved(volume, parent, stage_name, entry);
                if (removed != .ok) return removed;
            }
            target = observe(volume, parent, target_name, spec) orelse return .io;
            stage = observe(volume, parent, stage_name, spec) orelse return .io;
            if (target.content != .old or stage.content != .missing) return .conflict;
        },
    }
    return if (vfs.flushVolume(volume)) .ok else .io;
}

fn mutateReplace(
    volume: vfs.Volume,
    parent: vfs.NodeRef,
    target_name: []const u8,
    stage_name: []const u8,
    backup_name: []const u8,
) Result {
    fs_request.reportAtomicProgress(.replace, 0, 0);
    return switch (vfs.replaceFileAtomic(
        volume,
        parent,
        target_name,
        stage_name,
        backup_name,
        true,
    )) {
        .ok => .ok,
        .io => .io,
        .not_atomic => .not_atomic,
        else => .conflict,
    };
}

/// Expectations for the per-payload checked cleanup (0.60.23).
pub const CleanupSpec = struct {
    /// Fingerprint the freshly installed target must still carry.
    new_size: u64,
    new_checksum: u32,
    /// Whether this payload replaced an existing object (so a backup exists).
    target_existed: bool,
    /// Fingerprint of that backup, when the journal recorded one.
    old_known: bool = false,
    old_size: u64 = 0,
    old_checksum: u32 = 0,
    /// Fingerprint of an inherited older backup that may be rotated away.
    /// Without it the object stays - one extra last-good file is safe, an
    /// unidentifiable delete is not.
    previous_known: bool = false,
    previous_size: u64 = 0,
    previous_checksum: u32 = 0,
};

/// Per-payload cleanup with target, stage, current backup and previous backup
/// bound together under EXACTLY ONE filesystem-request gate (0.60.23).
///
/// The old shape preflighted the whole package and then deleted in a second
/// pass, with every probe and every delete taking its own gate.  A local
/// mutation between those two passes could therefore invalidate the very
/// state the preflight had just approved - and the last-good guarantee with
/// it.  Here the caller holds one gate across the complete decision, so the
/// state that was verified is the state that is acted upon.
///
/// Deliberately NOT done here: deleting the stage.  Reaching cleanup means
/// the stage was already consumed by the ownership transfer, so a delete
/// could only ever hit an object created afterwards - and since the stage
/// fingerprint IS the fingerprint of the freshly installed payload, that
/// would be an unrelated file with identical content.
pub fn cleanupPayload(
    volume: vfs.Volume,
    parent: vfs.NodeRef,
    target_name: []const u8,
    stage_name: []const u8,
    backup_name: []const u8,
    previous_backup_name: []const u8,
    spec: CleanupSpec,
) Result {
    if (target_name.len == 0 or stage_name.len == 0 or backup_name.len == 0 or
        !vfs.validateShortName83(stage_name) or
        !vfs.validateShortName83(backup_name))
    {
        return .not_atomic;
    }
    if (previous_backup_name.len != 0 and !vfs.validateShortName83(previous_backup_name))
        return .not_atomic;

    const probe = Spec{
        .target_existed = spec.target_existed,
        .old_known = spec.old_known,
        .new_size = spec.new_size,
        .new_checksum = spec.new_checksum,
        .old_size = spec.old_size,
        .old_checksum = spec.old_checksum,
    };

    // ---- verify the whole tuple first, still inside the caller's gate ----
    const target = observe(volume, parent, target_name, probe) orelse return .io;
    if (target.content != .new) return .conflict;

    const stage = observe(volume, parent, stage_name, probe) orelse return .io;
    if (stage.entry != null) return .conflict;

    const backup = observe(volume, parent, backup_name, probe) orelse return .io;
    if (spec.target_existed) {
        if (backup.entry == null) return .conflict;
        // With a recorded fingerprint the backup must match it exactly;
        // without one its mere presence is all the journal ever promised.
        if (spec.old_known and backup.content != .old) return .conflict;
    } else if (backup.entry != null) {
        return .conflict;
    }

    // ---- rotate the inherited backup, only on an exact match -------------
    if (previous_backup_name.len == 0 or
        nameEqual(previous_backup_name, backup_name) or
        !spec.target_existed)
    {
        return .ok;
    }

    if (!spec.previous_known) return .ok; // unidentifiable: keep it, safely

    // Probe the inherited backup against its own fingerprint directly.  It
    // may legitimately contain the same bytes as the newly installed target
    // (for example after a downgrade followed by an upgrade).  Classifying it
    // as the transaction's `old` content would then lose to `new` in
    // `observe` and incorrectly retain two last-good generations.
    const previous_probe = Spec{
        .target_existed = true,
        .old_known = false,
        .new_size = spec.previous_size,
        .new_checksum = spec.previous_checksum,
    };
    const previous = observe(volume, parent, previous_backup_name, previous_probe) orelse return .io;
    if (previous.entry == null) return .ok; // already gone: nothing to rotate
    if (previous.content != .new) return .ok; // foreign object: never touched

    return deleteObserved(volume, parent, previous_backup_name, previous.entry.?);
}

fn deleteObserved(
    volume: vfs.Volume,
    parent: vfs.NodeRef,
    name: []const u8,
    expected: vfs.Entry,
) Result {
    fs_request.reportAtomicProgress(.cleanup, 0, 0);
    return switch (vfs.deleteFileIfIdentity(volume, parent, name, expected)) {
        .deleted, .not_found => if (vfs.flushVolume(volume)) .ok else .io,
        .mismatch => .conflict,
        .io => .io,
    };
}

fn observe(
    volume: vfs.Volume,
    parent: vfs.NodeRef,
    name: []const u8,
    spec: Spec,
) ?Observed {
    fs_request.reportAtomicProgress(.lookup, 0, 0);
    var entry: vfs.Entry = undefined;
    switch (vfs.lookupRecoveryEntryStatus(volume, parent, name, &entry)) {
        .not_found => return .{},
        .io => return null,
        .found => {},
    }
    if (entry.isDir() or entry.size > @as(u64, @intCast(std.math.maxInt(usize))))
        return .{ .entry = entry, .content = .other };
    const checksum = checksumEntry(volume, entry) orelse return null;
    const content: Content = if (entry.size == spec.new_size and checksum == spec.new_checksum)
        .new
    else if (spec.old_known and entry.size == spec.old_size and checksum == spec.old_checksum)
        .old
    else
        .other;
    return .{ .entry = entry, .content = content };
}

fn checksumEntry(volume: vfs.Volume, entry: vfs.Entry) ?u32 {
    var state = recovery.checksum_seed;
    var offset: u64 = 0;
    const chunk_bytes: u64 = checksum_buffer.len;
    const total_chunks = entry.size / chunk_bytes + @intFromBool(entry.size % chunk_bytes != 0);
    var completed_chunks: u64 = 0;
    fs_request.reportAtomicProgress(.checksum, completed_chunks, total_chunks);
    while (offset < entry.size) {
        const want_u64 = @min(
            @as(u64, @intCast(checksum_buffer.len)),
            entry.size - offset,
        );
        const want: usize = @intCast(want_u64);
        const got = vfs.readFileRange(
            volume,
            entry,
            @intCast(offset),
            checksum_buffer[0..want],
        ) orelse return null;
        if (got != want) return null;
        state = recovery.checksumUpdate(state, checksum_buffer[0..want]);
        offset += want_u64;
        completed_chunks += 1;
        fs_request.reportAtomicProgress(.checksum, completed_chunks, total_chunks);
        // Timer preemption deliberately does not interrupt kernel IP. Yield
        // between bounded checksum chunks so a large cached fingerprint
        // cannot starve the desktop, network and the gate's own watchdog.
        // fs_request uses an UnwindGuard specifically so this wait-spanning
        // owner remains valid across cooperative scheduling points.
        scheduler.yield();
    }
    return state;
}

fn nameEqual(left: []const u8, right: []const u8) bool {
    return std.ascii.eqlIgnoreCase(left, right);
}
