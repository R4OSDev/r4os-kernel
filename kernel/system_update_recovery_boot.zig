// Package-wide SYSUPD recovery before normal runtime consumers start.
//
// Durable journal parsing and terminal rollback/cleanup policy live in the
// allocation-free shared system_update_recovery module.  This file is only
// the pre-runtime VFS adapter.  It deliberately preserves lookup I/O as I/O,
// binds every cleanup delete to content plus filesystem identity, and never
// quarantines or deletes an untrusted name.

const std = @import("std");
const system_update_recovery = @import("system_update_recovery");
const system_update_atomic = @import("../fs/system_update_atomic.zig");
const vfs = @import("../fs/vfs.zig");
const boot_status = @import("boot_status.zig");

const journal_directory = "/R4OS/UPDATE/STAGED";
const journal_names = [2][]const u8{ "SYSUPD0.JRN", "SYSUPD1.JRN" };
const journal_paths = [2][]const u8{
    "/R4OS/UPDATE/STAGED/SYSUPD0.JRN",
    "/R4OS/UPDATE/STAGED/SYSUPD1.JRN",
};

const JournalLookupStatus = enum {
    found,
    not_found,
    io,
};

const PathParts = struct {
    parent: []const u8,
    name: []const u8,
};

var active_journal: system_update_recovery.TransactionJournal = .{};
var parse_journal: system_update_recovery.TransactionJournal = .{};
var verify_journal: system_update_recovery.TransactionJournal = .{};
var journal_buffers: [2][system_update_recovery.journal_max]u8 = undefined;
var journal_write_buffer: [system_update_recovery.journal_max]u8 = undefined;
var checksum_buffer: [65536]u8 = undefined;

pub fn recoverBeforeRuntime() bool {
    const journal_volume = vfs.volumeForDrive('C') orelse return true;
    var journal_directory_node: vfs.NodeRef = undefined;
    switch (vfs.resolvePathStatus(journal_volume, journal_directory, &journal_directory_node)) {
        .found => {},
        .not_found => return true,
        .io => return false,
    }

    switch (readNewestValidJournalInto(journal_volume, &active_journal)) {
        .found => {},
        .not_found => return true,
        .io => return false,
    }
    if (system_update_recovery.phaseTerminal(active_journal.phase)) return true;

    var io = BootRecoveryIo{
        .journal_volume = journal_volume,
        .journal_directory = journal_directory_node,
    };
    const status = if (active_journal.batch)
        switch (active_journal.phase) {
            .post_boot => .ok,
            .stage, .commit => blk: {
                const forward = system_update_recovery.resumeBatchForward(&io, &active_journal);
                break :blk if (forward == .conflict)
                    system_update_recovery.rollbackToTerminal(&io, &active_journal)
                else
                    forward;
            },
            // Only the runtime post-boot verifier may publish `applied` for
            // a batch.  From there boot recovery may safely finish the same
            // shared cleanup if power failed between that marker and the
            // terminal slot.
            .applied => system_update_recovery.cleanupToTerminal(&io, &active_journal),
            .prepare, .verify, .inventory, .rollback =>
                system_update_recovery.rollbackToTerminal(&io, &active_journal),
            .cleanup, .rolled_back => .ok,
        }
    else
        switch (active_journal.phase) {
            .applied => system_update_recovery.cleanupToTerminal(&io, &active_journal),
            .prepare, .stage, .commit, .verify, .inventory, .post_boot, .rollback =>
                system_update_recovery.rollbackToTerminal(&io, &active_journal),
            .cleanup, .rolled_back => .ok,
        };
    if (status != .ok) {
        boot_status.statusLine("  SYSUPD recovery [FAILED: ");
        boot_status.statusLine(system_update_recovery.replayStatusName(status));
        boot_status.statusLine("]\r\n");
        return false;
    }
    boot_status.statusLine("  SYSUPD recovery [OK]\r\n");
    return true;
}

const BootRecoveryIo = struct {
    journal_volume: vfs.Volume,
    journal_directory: vfs.NodeRef,

    pub fn pathState(
        _: *const BootRecoveryIo,
        path: []const u8,
        expected_size: u64,
        expected_checksum: u32,
    ) system_update_recovery.PathState {
        const volume = payloadVolume(path) orelse return .io;
        var entry: vfs.Entry = undefined;
        switch (vfs.resolveEntryStatus(volume, stripDrive(path), &entry)) {
            .found => {},
            .not_found => return .not_found,
            .io => return .io,
        }
        if (entry.isDir() or entry.size != expected_size) return .other;
        const actual = checksumEntry(volume, entry) orelse return .io;
        return if (actual == expected_checksum) .match else .other;
    }

    pub fn presence(
        _: *const BootRecoveryIo,
        path: []const u8,
    ) system_update_recovery.PresenceState {
        const volume = payloadVolume(path) orelse return .io;
        var entry: vfs.Entry = undefined;
        return switch (vfs.resolveEntryStatus(volume, stripDrive(path), &entry)) {
            .found => if (entry.isDir()) .other else .file,
            .not_found => .not_found,
            .io => .io,
        };
    }

    pub fn rollbackPayload(
        _: *const BootRecoveryIo,
        entry: *const system_update_recovery.JournalPayload,
    ) system_update_recovery.MutationStatus {
        const target = entry.targetText();
        const staged = entry.stageText();
        const backup = entry.backupText();
        if (driveOf(target) != driveOf(staged) or driveOf(target) != driveOf(backup)) return .conflict;
        const volume = payloadVolume(target) orelse return .io;
        const target_parts = splitPath(target) orelse return .conflict;
        const staged_parts = splitPath(staged) orelse return .conflict;
        const backup_parts = splitPath(backup) orelse return .conflict;
        if (!system_update_recovery.pathEqualsIgnoreCase(target_parts.parent, staged_parts.parent) or
            !system_update_recovery.pathEqualsIgnoreCase(target_parts.parent, backup_parts.parent))
        {
            return .conflict;
        }
        var parent: vfs.NodeRef = undefined;
        switch (vfs.resolvePathStatus(volume, target_parts.parent, &parent)) {
            .found => {},
            .not_found => return .conflict,
            .io => return .io,
        }
        return switch (system_update_atomic.transition(
            volume,
            parent,
            target_parts.name,
            staged_parts.name,
            backup_parts.name,
            .rollback,
            .{
                .target_existed = entry.target_existed,
                .old_known = entry.old_known,
                .new_size = entry.size,
                .new_checksum = entry.checksum,
                .old_size = entry.old_size,
                .old_checksum = entry.old_checksum,
            },
        )) {
            .ok => .ok,
            .io => .io,
            .conflict, .not_atomic => .conflict,
        };
    }

    pub fn commitPayload(
        self: *const BootRecoveryIo,
        entry: *const system_update_recovery.JournalPayload,
    ) system_update_recovery.MutationStatus {
        if (!entry.replace_required) {
            return switch (self.pathState(entry.targetText(), entry.size, entry.checksum)) {
                .match => .ok,
                .not_found, .other => .conflict,
                .io => .io,
            };
        }
        const target = entry.targetText();
        const staged = entry.stageText();
        const backup = entry.backupText();
        if (driveOf(target) != driveOf(staged) or driveOf(target) != driveOf(backup)) return .conflict;
        const volume = payloadVolume(target) orelse return .io;
        const target_parts = splitPath(target) orelse return .conflict;
        const staged_parts = splitPath(staged) orelse return .conflict;
        const backup_parts = splitPath(backup) orelse return .conflict;
        if (!system_update_recovery.pathEqualsIgnoreCase(target_parts.parent, staged_parts.parent) or
            !system_update_recovery.pathEqualsIgnoreCase(target_parts.parent, backup_parts.parent))
        {
            return .conflict;
        }
        var parent: vfs.NodeRef = undefined;
        switch (vfs.resolvePathStatus(volume, target_parts.parent, &parent)) {
            .found => {},
            .not_found => return .conflict,
            .io => return .io,
        }
        return switch (system_update_atomic.transition(
            volume,
            parent,
            target_parts.name,
            staged_parts.name,
            backup_parts.name,
            .forward,
            .{
                .target_existed = entry.target_existed,
                .old_known = entry.old_known,
                .new_size = entry.size,
                .new_checksum = entry.checksum,
                .old_size = entry.old_size,
                .old_checksum = entry.old_checksum,
            },
        )) {
            .ok => .ok,
            .io => .io,
            .conflict, .not_atomic => .conflict,
        };
    }

    pub fn deleteIfMatch(
        _: *const BootRecoveryIo,
        path: []const u8,
        expected_size: u64,
        expected_checksum: u32,
    ) system_update_recovery.MutationStatus {
        const volume = payloadVolume(path) orelse return .io;
        const parts = splitPath(path) orelse return .conflict;
        var parent: vfs.NodeRef = undefined;
        switch (vfs.resolvePathStatus(volume, parts.parent, &parent)) {
            .found => {},
            .not_found => return if (vfs.flushVolume(volume)) .ok else .io,
            .io => return .io,
        }

        var entry: vfs.Entry = undefined;
        switch (vfs.lookupEntryStatus(volume, parent, parts.name, &entry)) {
            .found => {},
            .not_found => return if (vfs.flushVolume(volume)) .ok else .io,
            .io => return .io,
        }
        if (entry.isDir() or entry.size != expected_size) return .conflict;
        const actual = checksumEntry(volume, entry) orelse return .io;
        if (actual != expected_checksum) return .conflict;

        return switch (vfs.deleteFileIfIdentity(volume, parent, parts.name, entry)) {
            .deleted, .not_found => if (vfs.flushVolume(volume)) .ok else .io,
            .mismatch => .conflict,
            .io => .io,
        };
    }

    /// Per-payload checked cleanup under one gate (0.60.23).
    ///
    /// Same contract and same shared implementation as the R4X updater - the
    /// difference is only that pre-runtime recovery reaches
    /// `system_update_atomic` directly instead of through the R4SYS slot, so
    /// app and boot recovery cannot drift into two interpretations.
    pub fn cleanupPayload(
        _: *const BootRecoveryIo,
        entry: *const system_update_recovery.JournalPayload,
    ) system_update_recovery.MutationStatus {
        const volume = payloadVolume(entry.targetText()) orelse return .io;
        const target_parts = splitPath(entry.targetText()) orelse return .conflict;
        const stage_parts = splitPath(entry.stageText()) orelse return .conflict;
        const backup_parts = splitPath(entry.backupText()) orelse return .conflict;
        if (!system_update_recovery.pathEqualsIgnoreCase(target_parts.parent, stage_parts.parent) or
            !system_update_recovery.pathEqualsIgnoreCase(target_parts.parent, backup_parts.parent))
        {
            return .conflict;
        }

        // Only rotate an inherited backup that is genuinely a different name.
        var previous_name: []const u8 = &[_]u8{};
        if (entry.previous_backup_len != 0 and
            !system_update_recovery.pathEqualsIgnoreCase(entry.previousBackupText(), entry.backupText()))
        {
            const previous_parts = splitPath(entry.previousBackupText()) orelse return .conflict;
            if (!system_update_recovery.pathEqualsIgnoreCase(target_parts.parent, previous_parts.parent))
                return .conflict;
            previous_name = previous_parts.name;
        }

        var parent: vfs.NodeRef = undefined;
        switch (vfs.resolvePathStatus(volume, target_parts.parent, &parent)) {
            .found => {},
            .not_found, .io => return .io,
        }

        return switch (system_update_atomic.cleanupPayload(
            volume,
            parent,
            target_parts.name,
            stage_parts.name,
            backup_parts.name,
            previous_name,
            .{
                .new_size = entry.size,
                .new_checksum = entry.checksum,
                .target_existed = entry.target_existed,
                .old_known = entry.old_known,
                .old_size = entry.old_size,
                .old_checksum = entry.old_checksum,
                .previous_known = entry.previous_backup_known,
                .previous_size = entry.previous_backup_size,
                .previous_checksum = entry.previous_backup_checksum,
            },
        )) {
            .ok => .ok,
            .io => .io,
            .conflict, .not_atomic => .conflict,
        };
    }

    pub fn persist(
        self: *const BootRecoveryIo,
        journal: *system_update_recovery.TransactionJournal,
    ) bool {
        return writeRecoveryJournal(
            self.journal_volume,
            self.journal_directory,
            journal,
        );
    }
};

fn readNewestValidJournalInto(
    volume: vfs.Volume,
    out: *system_update_recovery.TransactionJournal,
) JournalLookupStatus {
    out.* = .{};
    var lengths: [2]usize = .{ 0, 0 };
    var present: [2]bool = .{ false, false };
    var valid: [2]bool = .{ false, false };
    var generations: [2]u64 = .{ 0, 0 };

    var slot: usize = 0;
    while (slot < journal_paths.len) : (slot += 1) {
        var entry: vfs.Entry = undefined;
        switch (vfs.resolveEntryStatus(volume, journal_paths[slot], &entry)) {
            .found => {},
            .not_found => continue,
            .io => return .io,
        }
        present[slot] = true;
        if (entry.isDir() or entry.size == 0 or entry.size > system_update_recovery.journal_max) continue;
        const expected_len: usize = @intCast(entry.size);
        const got = vfs.readFile(volume, entry, journal_buffers[slot][0..expected_len]) orelse return .io;
        if (got != expected_len) return .io;
        lengths[slot] = expected_len;
        if (!system_update_recovery.parseJournalInto(
            journal_buffers[slot][0..expected_len],
            @intCast(slot),
            &parse_journal,
        )) {
            continue;
        }
        valid[slot] = true;
        generations[slot] = parse_journal.journal_generation;
    }

    const selected: usize = switch (system_update_recovery.selectNewestSlot(
        present,
        valid,
        generations,
    )) {
        .selected => |selected_slot| selected_slot,
        .not_found => return .not_found,
        .invalid => return .io,
    };
    return if (system_update_recovery.parseJournalInto(
        journal_buffers[selected][0..lengths[selected]],
        @intCast(selected),
        out,
    ))
        .found
    else
        .io;
}

fn writeRecoveryJournal(
    volume: vfs.Volume,
    directory: vfs.NodeRef,
    journal: *system_update_recovery.TransactionJournal,
) bool {
    if (journal.journal_generation == std.math.maxInt(u64)) return false;
    const previous_slot = journal.slot;
    const previous_generation = journal.journal_generation;
    const previous_valid = journal.valid;
    var durable = false;
    defer if (!durable) {
        journal.slot = previous_slot;
        journal.journal_generation = previous_generation;
        journal.valid = previous_valid;
    };
    journal.slot = 1 - journal.slot;
    journal.journal_generation += 1;
    journal.valid = true;
    const encoded = system_update_recovery.serializeJournal(
        journal,
        journal_write_buffer[0..],
    ) orelse return false;

    // A failed completion may still have reached stable storage.  Read the
    // inactive slot back and accept only the exact intended byte sequence.
    _ = vfs.writeFile(volume, directory, journal_names[journal.slot], encoded);
    var entry: vfs.Entry = undefined;
    switch (vfs.resolveEntryStatus(volume, journal_paths[journal.slot], &entry)) {
        .found => {},
        .not_found, .io => return false,
    }
    if (entry.isDir() or entry.size != encoded.len) return false;
    const got = vfs.readFile(
        volume,
        entry,
        journal_buffers[journal.slot][0..encoded.len],
    ) orelse return false;
    if (got != encoded.len or
        !std.mem.eql(u8, encoded, journal_buffers[journal.slot][0..encoded.len]))
    {
        return false;
    }
    if (!system_update_recovery.parseJournalInto(
        journal_buffers[journal.slot][0..encoded.len],
        journal.slot,
        &verify_journal,
    )) {
        return false;
    }
    durable = verify_journal.transaction_generation == journal.transaction_generation and
        verify_journal.journal_generation == journal.journal_generation and
        verify_journal.phase == journal.phase;
    return durable;
}

fn checksumEntry(volume: vfs.Volume, entry: vfs.Entry) ?u32 {
    if (entry.isDir() or entry.size > @as(u64, @intCast(std.math.maxInt(usize)))) return null;
    var state = system_update_recovery.checksum_seed;
    var done: u64 = 0;
    while (done < entry.size) {
        const want_u64 = @min(
            @as(u64, @intCast(checksum_buffer.len)),
            entry.size - done,
        );
        const want: usize = @intCast(want_u64);
        const got = vfs.readFileRange(
            volume,
            entry,
            @intCast(done),
            checksum_buffer[0..want],
        ) orelse return null;
        if (got != want) return null;
        state = system_update_recovery.checksumUpdate(state, checksum_buffer[0..want]);
        done += want_u64;
    }
    return state;
}

fn payloadVolume(path: []const u8) ?vfs.Volume {
    if (path.len >= 3 and path[1] == ':' and path[2] == '\\') {
        return vfs.volumeForDrive(path[0]);
    }
    if (isBootSubtreePath(path)) {
        if (vfs.bootVolume()) |volume| return volume;
    }
    return vfs.volumeForDrive('C');
}

fn isBootSubtreePath(path: []const u8) bool {
    if (path.len < 5 or path[0] != '\\') return false;
    if (!std.ascii.eqlIgnoreCase(path[1..5], "boot")) return false;
    return path.len == 5 or path[5] == '\\';
}

fn driveOf(path: []const u8) u8 {
    if (path.len >= 3 and path[1] == ':' and path[2] == '\\') {
        return std.ascii.toUpper(path[0]);
    }
    return 0;
}

fn splitPath(path_raw: []const u8) ?PathParts {
    const path = stripDrive(path_raw);
    var index = path.len;
    while (index > 0) : (index -= 1) {
        if (path[index - 1] == '\\') {
            if (index >= path.len) return null;
            return .{
                .parent = if (index == 1) "/" else path[0 .. index - 1],
                .name = path[index..],
            };
        }
    }
    return null;
}

fn stripDrive(path: []const u8) []const u8 {
    if (path.len >= 3 and path[1] == ':' and path[2] == '\\') return path[2..];
    return path;
}
