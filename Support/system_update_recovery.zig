// Shared, allocation-free SYSUPD journal and terminal-recovery core.
//
// This module deliberately imports only `std`.  The R4X updater and the
// pre-runtime kernel recovery seam compile it for different execution
// environments and provide thin I/O adapters.  Durable format parsing,
// validation, slot ordering, rollback and cleanup policy must never diverge
// between those two consumers.

const std = @import("std");

pub const journal_max: usize = 192 * 1024;
pub const max_package_payloads: usize = 32;
// A single-package transaction adds MODULES.JSON.  A v5 restart batch may
// append VERSION.R4S only after successful post-boot validation.
pub const max_payloads: usize = max_package_payloads + 2;
pub const max_path: usize = 1024;
pub const max_version: usize = 32;
pub const checksum_seed: u32 = 2166136261;

pub const JournalPhase = enum {
    prepare,
    stage,
    commit,
    verify,
    inventory,
    applied,
    post_boot,
    cleanup,
    rollback,
    rolled_back,
};

pub const PathState = enum {
    match,
    not_found,
    other,
    io,
};

pub const PresenceState = enum {
    file,
    other,
    not_found,
    io,
};

pub const MutationStatus = enum {
    ok,
    conflict,
    io,
};

pub const ReplayStatus = enum {
    ok,
    invalid,
    conflict,
    io,
};

pub const SlotSelection = union(enum) {
    selected: u8,
    not_found,
    invalid,
};

pub const JournalPayload = struct {
    target_path: [max_path:0]u8 = .{0} ** max_path,
    target_len: usize = 0,
    stage_path: [max_path:0]u8 = .{0} ** max_path,
    stage_len: usize = 0,
    backup_path: [max_path:0]u8 = .{0} ** max_path,
    backup_len: usize = 0,
    previous_backup_path: [max_path:0]u8 = .{0} ** max_path,
    previous_backup_len: usize = 0,
    previous_backup_known: bool = false,
    previous_backup_size: u64 = 0,
    previous_backup_checksum: u32 = 0,
    size: u64 = 0,
    checksum: u32 = 0,
    target_existed: bool = false,
    old_known: bool = false,
    old_size: u64 = 0,
    old_checksum: u32 = 0,
    committed: bool = false,
    rolled_back: bool = false,
    replace_required: bool = true,

    pub fn targetText(self: *const JournalPayload) []const u8 {
        return self.target_path[0..self.target_len];
    }

    pub fn stageText(self: *const JournalPayload) []const u8 {
        return self.stage_path[0..self.stage_len];
    }

    pub fn backupText(self: *const JournalPayload) []const u8 {
        return self.backup_path[0..self.backup_len];
    }

    pub fn previousBackupText(self: *const JournalPayload) []const u8 {
        return self.previous_backup_path[0..self.previous_backup_len];
    }

    pub fn targetPtr(self: *const JournalPayload) [*:0]const u8 {
        return @ptrCast(self.target_path[0..].ptr);
    }

    pub fn stagePtr(self: *const JournalPayload) [*:0]const u8 {
        return @ptrCast(self.stage_path[0..].ptr);
    }

    pub fn backupPtr(self: *const JournalPayload) [*:0]const u8 {
        return @ptrCast(self.backup_path[0..].ptr);
    }

    pub fn previousBackupPtr(self: *const JournalPayload) [*:0]const u8 {
        return @ptrCast(self.previous_backup_path[0..].ptr);
    }
};

pub const JournalComponentKind = enum {
    kernel,
    r4x,
    r4l,
    r4d,
    r4p,

    pub fn text(self: JournalComponentKind) []const u8 {
        return switch (self) {
            .kernel => "KERNEL",
            .r4x => "R4X",
            .r4l => "R4L",
            .r4d => "R4D",
            .r4p => "R4P",
        };
    }

    pub fn parse(value: []const u8) ?JournalComponentKind {
        inline for (std.meta.tags(JournalComponentKind)) |kind| {
            if (std.ascii.eqlIgnoreCase(value, kind.text())) return kind;
        }
        return null;
    }
};

pub const JournalComponent = struct {
    payload_index: u32 = 0,
    kind: JournalComponentKind = .r4x,
    name: [49:0]u8 = .{0} ** 49,
    name_len: usize = 0,
    target: [max_path:0]u8 = .{0} ** max_path,
    target_len: usize = 0,
    version: [max_version:0]u8 = .{0} ** max_version,
    version_len: usize = 0,

    pub fn nameText(self: *const JournalComponent) []const u8 {
        return self.name[0..self.name_len];
    }

    pub fn targetText(self: *const JournalComponent) []const u8 {
        return self.target[0..self.target_len];
    }

    pub fn versionText(self: *const JournalComponent) []const u8 {
        return self.version[0..self.version_len];
    }
};

pub const TransactionJournal = struct {
    valid: bool = false,
    slot: u8 = 0,
    transaction_generation: u64 = 0,
    journal_generation: u64 = 0,
    phase: JournalPhase = .prepare,
    package_digest: u32 = 0,
    package_length: u64 = 0,
    manifest_checksum: u32 = 0,
    component_digest: u32 = 0,
    component_count: u32 = 0,
    components_bound: bool = true,
    components: [max_package_payloads]JournalComponent = .{JournalComponent{}} ** max_package_payloads,
    foundation: bool = false,
    package_version: [max_version:0]u8 = .{0} ** max_version,
    package_version_len: usize = 0,
    source_path: [max_path:0]u8 = .{0} ** max_path,
    source_len: usize = 0,
    source_version: [max_version:0]u8 = .{0} ** max_version,
    source_version_len: usize = 0,
    target_version: [max_version:0]u8 = .{0} ** max_version,
    target_version_len: usize = 0,
    package_payload_count: u32 = 0,
    payload_count: u32 = 0,
    committed_count: u32 = 0,
    rollback_count: u32 = 0,
    reboot: bool = false,
    /// v5: one journal spans the complete ordered restart batch.  Boot
    /// recovery may continue this transaction forward to `post_boot`; normal
    /// single-package journals retain the historical rollback-on-boot policy.
    batch: bool = false,
    // In-memory compatibility marker only; the canonical serializer always
    // writes per-payload rolled_back fields.
    rollback_markers_present: bool = true,
    payloads: [max_payloads]JournalPayload = .{JournalPayload{}} ** max_payloads,

    pub fn sourceText(self: *const TransactionJournal) []const u8 {
        return self.source_path[0..self.source_len];
    }

    pub fn sourcePtr(self: *const TransactionJournal) [*:0]const u8 {
        return @ptrCast(self.source_path[0..].ptr);
    }

    pub fn sourceVersionText(self: *const TransactionJournal) []const u8 {
        return self.source_version[0..self.source_version_len];
    }

    pub fn targetVersionText(self: *const TransactionJournal) []const u8 {
        return self.target_version[0..self.target_version_len];
    }

    pub fn sourceReleaseText(self: *const TransactionJournal) []const u8 {
        return self.sourceVersionText();
    }

    pub fn releaseText(self: *const TransactionJournal) []const u8 {
        return self.targetVersionText();
    }

    pub fn packageVersionText(self: *const TransactionJournal) []const u8 {
        return self.package_version[0..self.package_version_len];
    }
};

pub fn phaseTerminal(phase: JournalPhase) bool {
    return phase == .cleanup or phase == .rolled_back;
}

pub fn phaseName(phase: JournalPhase) []const u8 {
    return switch (phase) {
        .prepare => "prepare",
        .stage => "stage",
        .commit => "commit",
        .verify => "verify",
        .inventory => "inventory",
        .applied => "applied",
        .post_boot => "post_boot",
        .cleanup => "cleanup",
        .rollback => "rollback",
        .rolled_back => "rolled_back",
    };
}

pub fn replayStatusName(status: ReplayStatus) []const u8 {
    return switch (status) {
        .ok => "ok",
        .invalid => "journal-invalid",
        .conflict => "state-conflict",
        .io => "storage-io",
    };
}

/// Selects the newest independently valid slot.  A present invalid slot may
/// be ignored only when the other slot is valid.  Equal valid generations are
/// ambiguous because neither can be proven to be the inactive successor.
pub fn selectNewestSlot(
    present: [2]bool,
    valid: [2]bool,
    generation: [2]u64,
) SlotSelection {
    if ((valid[0] and !present[0]) or (valid[1] and !present[1])) return .invalid;
    if (valid[0] and valid[1]) {
        if (generation[0] == generation[1]) return .invalid;
        return .{ .selected = if (generation[0] > generation[1]) 0 else 1 };
    }
    if (valid[0]) return .{ .selected = 0 };
    if (valid[1]) return .{ .selected = 1 };
    return if (present[0] or present[1]) .invalid else .not_found;
}

/// Serializes the canonical current journal.  The checksum line is always the
/// final line and always carries a trailing newline so exact readback is
/// unambiguous.
pub fn serializeJournal(journal: *const TransactionJournal, out: []u8) ?[]const u8 {
    if (!journalValid(journal)) return null;
    var len: usize = 0;
    if (!appendLine(out, &len, "R4U_JOURNAL=5")) return null;
    if (!appendKeyU64(out, &len, "TRANSACTION_GENERATION=", journal.transaction_generation)) return null;
    if (!appendKeyU64(out, &len, "JOURNAL_GENERATION=", journal.journal_generation)) return null;
    if (!appendKeyValue(out, &len, "PHASE=", phaseName(journal.phase))) return null;
    if (!appendKeyU64(out, &len, "PACKAGE_DIGEST=", journal.package_digest)) return null;
    if (!appendKeyU64(out, &len, "PACKAGE_LENGTH=", journal.package_length)) return null;
    if (!appendKeyU64(out, &len, "MANIFEST_CHECKSUM=", journal.manifest_checksum)) return null;
    if (!appendKeyU64(out, &len, "COMPONENT_DIGEST=", journal.component_digest)) return null;
    if (!appendKeyU64(out, &len, "COMPONENTS=", journal.component_count)) return null;
    if (!appendKeyValue(out, &len, "COMPONENT_BINDING=", if (journal.components_bound) "bound" else "legacy")) return null;
    if (!appendKeyValue(out, &len, "PRIORITY=", if (journal.foundation) "foundation" else "normal")) return null;
    if (!appendKeyValue(out, &len, "PACKAGE_VERSION=", journal.packageVersionText())) return null;
    if (!appendKeyValue(out, &len, "SOURCE_RELEASE=", journal.sourceReleaseText())) return null;
    if (!appendKeyValue(out, &len, "RELEASE=", journal.releaseText())) return null;
    if (!appendKeyValue(out, &len, "SOURCE=", journal.sourceText())) return null;
    if (!appendKeyU64(out, &len, "PACKAGE_PAYLOADS=", journal.package_payload_count)) return null;
    if (!appendKeyU64(out, &len, "PAYLOADS=", journal.payload_count)) return null;
    if (!appendKeyU64(out, &len, "COMMITTED=", journal.committed_count)) return null;
    if (!appendKeyU64(out, &len, "ROLLED_BACK=", journal.rollback_count)) return null;
    if (!appendKeyValue(out, &len, "REBOOT=", if (journal.reboot) "yes" else "no")) return null;
    if (!appendKeyValue(out, &len, "BATCH=", if (journal.batch) "yes" else "no")) return null;

    var component_index: usize = 0;
    while (journal.components_bound and component_index < journal.component_count) : (component_index += 1) {
        const component = &journal.components[component_index];
        if (!appendText(out, &len, "COMPONENT;index=") or
            !appendU64(out, &len, component_index) or
            !appendText(out, &len, ";payload=") or
            !appendU64(out, &len, component.payload_index) or
            !appendText(out, &len, ";kind=") or
            !appendText(out, &len, component.kind.text()) or
            !appendText(out, &len, ";name=") or
            !appendText(out, &len, component.nameText()) or
            !appendText(out, &len, ";target=") or
            !appendText(out, &len, component.targetText()) or
            !appendText(out, &len, ";version=") or
            !appendText(out, &len, component.versionText()) or
            !appendByte(out, &len, '\n')) return null;
    }

    var index: usize = 0;
    while (index < journal.payload_count) : (index += 1) {
        const entry = &journal.payloads[index];
        if (!appendText(out, &len, "PAYLOAD;index=") or
            !appendU64(out, &len, index) or
            !appendText(out, &len, ";target=") or
            !appendText(out, &len, entry.targetText()) or
            !appendText(out, &len, ";stage=") or
            !appendText(out, &len, entry.stageText()) or
            !appendText(out, &len, ";backup=") or
            !appendText(out, &len, entry.backupText()) or
            !appendText(out, &len, ";previous_backup=") or
            !appendText(out, &len, entry.previousBackupText()) or
            !appendText(out, &len, ";previous_known=") or
            !appendByte(out, &len, if (entry.previous_backup_known) '1' else '0') or
            !appendText(out, &len, ";previous_size=") or
            !appendU64(out, &len, entry.previous_backup_size) or
            !appendText(out, &len, ";previous_checksum=") or
            !appendU64(out, &len, entry.previous_backup_checksum) or
            !appendText(out, &len, ";size=") or
            !appendU64(out, &len, entry.size) or
            !appendText(out, &len, ";checksum=") or
            !appendU64(out, &len, entry.checksum) or
            !appendText(out, &len, ";existed=") or
            !appendByte(out, &len, if (entry.target_existed) '1' else '0') or
            !appendText(out, &len, ";old_known=") or
            !appendByte(out, &len, if (entry.old_known) '1' else '0') or
            !appendText(out, &len, ";old_size=") or
            !appendU64(out, &len, entry.old_size) or
            !appendText(out, &len, ";old_checksum=") or
            !appendU64(out, &len, entry.old_checksum) or
            !appendText(out, &len, ";required=") or
            !appendByte(out, &len, if (entry.replace_required) '1' else '0') or
            !appendText(out, &len, ";committed=") or
            !appendByte(out, &len, if (entry.committed) '1' else '0') or
            !appendText(out, &len, ";rolled_back=") or
            !appendByte(out, &len, if (entry.rolled_back) '1' else '0') or
            !appendByte(out, &len, '\n'))
        {
            return null;
        }
    }
    const digest = checksum(out[0..len]);
    if (!appendKeyU64(out, &len, "CHECKSUM=", digest)) return null;
    return out[0..len];
}

/// Parses both the current canonical v2 journal and the older v2 shape which
/// predates content fingerprints/per-payload rollback markers.  Legacy
/// journals remain fail-closed during recovery whenever their missing
/// identity data cannot authorize a mutation.
pub fn parseJournalInto(data: []const u8, slot: u8, journal: *TransactionJournal) bool {
    if (slot > 1 or data.len == 0 or data.len > journal_max) return false;
    const checksum_marker = std.mem.lastIndexOf(u8, data, "CHECKSUM=") orelse return false;
    if (checksum_marker > 0 and data[checksum_marker - 1] != '\n') return false;
    const checksum_end = std.mem.indexOfScalarPos(u8, data, checksum_marker, '\n') orelse return false;
    if (checksum_end + 1 != data.len) return false;
    const expected = parseU32(trim(data[checksum_marker + "CHECKSUM=".len .. checksum_end])) orelse return false;
    if (checksum(data[0..checksum_marker]) != expected) return false;
    const journal_version = journalField(data, "R4U_JOURNAL=") orelse return false;
    const current = equalsIgnoreCase(journal_version, "5");
    const component_format = current or equalsIgnoreCase(journal_version, "4");
    const prior = equalsIgnoreCase(journal_version, "3");
    const legacy = equalsIgnoreCase(journal_version, "2");
    const modern = component_format or prior;
    if (!modern and !legacy) return false;
    if (!journalLinesKnown(data[0..checksum_marker], modern, component_format, current)) return false;
    const reboot = parseYesNo(journalField(data, "REBOOT=") orelse return false) orelse return false;
    const foundation = if (modern)
        if (equalsIgnoreCase(journalField(data, "PRIORITY=") orelse return false, "foundation"))
            true
        else if (equalsIgnoreCase(journalField(data, "PRIORITY=") orelse return false, "normal"))
            false
        else
            return false
    else
        false;

    journal.* = .{
        .valid = true,
        .slot = slot,
        .transaction_generation = parseU64(journalField(data, "TRANSACTION_GENERATION=") orelse "") orelse return false,
        .journal_generation = parseU64(journalField(data, "JOURNAL_GENERATION=") orelse "") orelse return false,
        .phase = parsePhase(journalField(data, "PHASE=") orelse "") orelse return false,
        .package_digest = parseU32(journalField(data, "PACKAGE_DIGEST=") orelse "") orelse return false,
        .package_length = parseU64(journalField(data, "PACKAGE_LENGTH=") orelse "") orelse return false,
        .manifest_checksum = if (modern) parseU32(journalField(data, "MANIFEST_CHECKSUM=") orelse "") orelse return false else 0,
        .component_digest = if (modern) parseU32(journalField(data, "COMPONENT_DIGEST=") orelse "") orelse return false else 0,
        .component_count = if (modern) parseU32(journalField(data, "COMPONENTS=") orelse "") orelse return false else 0,
        .components_bound = if (component_format)
            if (equalsIgnoreCase(journalField(data, "COMPONENT_BINDING=") orelse return false, "bound"))
                true
            else if (equalsIgnoreCase(journalField(data, "COMPONENT_BINDING=") orelse return false, "legacy"))
                false
            else
                return false
        else
            false,
        .foundation = foundation,
        .package_payload_count = if (component_format)
            parseU32(journalField(data, "PACKAGE_PAYLOADS=") orelse "") orelse return false
        else
            parseU32(journalField(data, "PAYLOADS=") orelse "") orelse return false,
        .payload_count = parseU32(journalField(data, "PAYLOADS=") orelse "") orelse return false,
        .committed_count = parseU32(journalField(data, "COMMITTED=") orelse "") orelse return false,
        .rollback_count = parseU32(journalField(data, "ROLLED_BACK=") orelse "") orelse return false,
        .reboot = reboot,
        .batch = if (current)
            parseYesNo(journalField(data, "BATCH=") orelse return false) orelse return false
        else
            false,
    };
    if (journal.transaction_generation == 0 or
        journal.journal_generation == 0 or
        journal.payload_count == 0 or
        journal.payload_count > max_payloads or
        journal.package_payload_count == 0 or
        journal.package_payload_count > max_package_payloads or
        journal.payload_count < journal.package_payload_count or
        journal.payload_count > journal.package_payload_count + (if (journal.batch) @as(u32, 2) else 1))
    {
        return false;
    }
    journal.source_len = (normalizeAbsolutePath(
        journal.source_path[0..],
        journalField(data, "SOURCE=") orelse return false,
    ) orelse return false).len;
    journal.source_version_len = copyTextZ(
        journal.source_version[0..],
        journalField(data, if (modern) "SOURCE_RELEASE=" else "SOURCE_VERSION=") orelse return false,
    ) orelse return false;
    journal.target_version_len = copyTextZ(
        journal.target_version[0..],
        journalField(data, if (modern) "RELEASE=" else "TARGET_VERSION=") orelse return false,
    ) orelse return false;
    journal.package_version_len = copyTextZ(
        journal.package_version[0..],
        if (modern)
            journalField(data, "PACKAGE_VERSION=") orelse return false
        else
            journal.targetVersionText(),
    ) orelse return false;
    if (!versionTextValid(journal.sourceVersionText()) or
        !versionTextValid(journal.targetVersionText()) or
        !versionTextValid(journal.packageVersionText()) or
        journal.component_count > journal.package_payload_count)
    {
        return false;
    }

    var seen_payloads: [max_payloads]bool = .{false} ** max_payloads;
    var seen_components: [max_package_payloads]bool = .{false} ** max_package_payloads;
    var found: u32 = 0;
    var found_components: u32 = 0;
    var rollback_marker_count: u32 = 0;
    var pos: usize = 0;
    while (nextLine(data[0..checksum_marker], &pos)) |line_raw| {
        const line = trim(line_raw);
        if (startsWith(line, "COMPONENT;")) {
            if (!component_format or !componentFieldsKnown(line)) return false;
            const index_u32 = parseU32(requiredField(line, "index=") orelse return false) orelse return false;
            if (index_u32 >= journal.component_count) return false;
            const index: usize = @intCast(index_u32);
            if (seen_components[index]) return false;
            seen_components[index] = true;
            var component = &journal.components[index];
            component.payload_index = parseU32(requiredField(line, "payload=") orelse return false) orelse return false;
            component.kind = JournalComponentKind.parse(requiredField(line, "kind=") orelse return false) orelse return false;
            component.name_len = copyTextZ(component.name[0..], requiredField(line, "name=") orelse return false) orelse return false;
            component.target_len = copyTextZ(component.target[0..], requiredField(line, "target=") orelse return false) orelse return false;
            component.version_len = copyTextZ(component.version[0..], requiredField(line, "version=") orelse return false) orelse return false;
            if (component.payload_index >= journal.package_payload_count or !componentValid(component)) return false;
            found_components += 1;
            continue;
        }
        if (!startsWith(line, "PAYLOAD;")) continue;
        if (!payloadFieldsKnown(line)) return false;
        const index_u32 = parseU32(requiredField(line, "index=") orelse return false) orelse return false;
        if (index_u32 >= journal.payload_count) return false;
        const index: usize = @intCast(index_u32);
        if (seen_payloads[index]) return false;
        seen_payloads[index] = true;

        var entry = &journal.payloads[index];
        entry.target_len = (normalizeAbsolutePath(
            entry.target_path[0..],
            requiredField(line, "target=") orelse return false,
        ) orelse return false).len;
        entry.stage_len = (normalizeAbsolutePath(
            entry.stage_path[0..],
            requiredField(line, "stage=") orelse return false,
        ) orelse return false).len;
        entry.backup_len = (normalizeAbsolutePath(
            entry.backup_path[0..],
            requiredField(line, "backup=") orelse return false,
        ) orelse return false).len;

        const previous_backup = optionalField(line, "previous_backup=") orelse "";
        entry.previous_backup_len = if (previous_backup.len == 0)
            0
        else
            (normalizeAbsolutePath(entry.previous_backup_path[0..], previous_backup) orelse return false).len;
        if (optionalField(line, "previous_known=")) |raw_known| {
            const known = parseBool01(raw_known) orelse return false;
            const raw_size = optionalField(line, "previous_size=");
            const raw_checksum = optionalField(line, "previous_checksum=");
            if (known) {
                entry.previous_backup_known = true;
                entry.previous_backup_size = parseU64(raw_size orelse return false) orelse return false;
                entry.previous_backup_checksum = parseU32(raw_checksum orelse return false) orelse return false;
            } else {
                if (raw_size) |value| {
                    if ((parseU64(value) orelse return false) != 0) return false;
                }
                if (raw_checksum) |value| {
                    if ((parseU32(value) orelse return false) != 0) return false;
                }
            }
        } else if (optionalField(line, "previous_size=") != null or
            optionalField(line, "previous_checksum=") != null)
        {
            return false;
        }
        if (entry.previous_backup_known and entry.previous_backup_len == 0) return false;

        entry.size = parseU64(requiredField(line, "size=") orelse return false) orelse return false;
        entry.checksum = parseU32(requiredField(line, "checksum=") orelse return false) orelse return false;
        entry.target_existed = parseBool01(requiredField(line, "existed=") orelse return false) orelse return false;
        if (optionalField(line, "old_known=")) |raw_known| {
            const known = parseBool01(raw_known) orelse return false;
            const raw_size = optionalField(line, "old_size=");
            const raw_checksum = optionalField(line, "old_checksum=");
            if (known) {
                entry.old_known = true;
                entry.old_size = parseU64(raw_size orelse return false) orelse return false;
                entry.old_checksum = parseU32(raw_checksum orelse return false) orelse return false;
            } else {
                if (raw_size) |value| {
                    if ((parseU64(value) orelse return false) != 0) return false;
                }
                if (raw_checksum) |value| {
                    if ((parseU32(value) orelse return false) != 0) return false;
                }
            }
        } else if (optionalField(line, "old_size=") != null or
            optionalField(line, "old_checksum=") != null)
        {
            return false;
        }
        if (entry.old_known and !entry.target_existed) return false;

        entry.replace_required = parseBool01(requiredField(line, "required=") orelse return false) orelse return false;
        entry.committed = parseBool01(requiredField(line, "committed=") orelse return false) orelse return false;
        if (optionalField(line, "rolled_back=")) |raw_rolled_back| {
            rollback_marker_count += 1;
            entry.rolled_back = parseBool01(raw_rolled_back) orelse return false;
        }
        if (entry.committed and entry.rolled_back) return false;
        if (!entry.replace_required) {
            if (!entry.target_existed) return false;
            if (entry.old_known and
                (entry.old_size != entry.size or entry.old_checksum != entry.checksum))
            {
                return false;
            }
        }
        found += 1;
    }
    if (component_format and journal.components_bound and found_components != journal.component_count) return false;
    if (component_format and !journal.components_bound and found_components != 0) return false;
    if (found != journal.payload_count) return false;
    if (rollback_marker_count != 0 and rollback_marker_count != journal.payload_count) return false;
    journal.rollback_markers_present = rollback_marker_count != 0;

    if (rollback_marker_count == 0 and journal.rollback_count != 0) {
        // Compatibility with journals written before the per-payload marker:
        // the legacy writer counted replace-required entries in reverse order.
        // Mark intervening no-op payloads too, producing the canonical suffix
        // that the current replay state machine persists.
        var remaining = journal.rollback_count;
        var cursor: usize = journal.payload_count;
        while (cursor > 0 and remaining > 0) {
            cursor -= 1;
            journal.payloads[cursor].rolled_back = true;
            if (journal.payloads[cursor].replace_required) remaining -= 1;
        }
        if (remaining != 0) return false;
        journal.rollback_count = 0;
        var inferred_index: usize = 0;
        while (inferred_index < journal.payload_count) : (inferred_index += 1) {
            if (journal.payloads[inferred_index].rolled_back) journal.rollback_count += 1;
        }
    }
    return journalValid(journal);
}

/// Replays an interrupted transaction back to its pre-update state.  `io`
/// supplies these methods:
///
///   rollbackPayload(entry) MutationStatus
///   persist(journal) bool
///
/// rollbackPayload is a purpose-specific kernel primitive: it validates the
/// complete T/S/B fingerprint and filesystem-identity state and performs the
/// ownership transition under one filesystem-request gate.  The shared
/// engine never authorizes a by-name mutation.
pub fn rollbackToTerminal(io: anytype, journal: *TransactionJournal) ReplayStatus {
    if (!journalValid(journal) or
        journal.phase == .applied or
        journal.phase == .cleanup)
    {
        return .invalid;
    }
    if (journal.phase == .rolled_back) return .ok;

    journal.phase = .rollback;
    journal.rollback_markers_present = true;
    recount(journal);
    if (!io.persist(journal)) return .io;

    var cursor: usize = journal.payload_count;
    while (cursor > 0) {
        cursor -= 1;
        var entry = &journal.payloads[cursor];
        if (entry.replace_required) {
            switch (io.rollbackPayload(entry)) {
                .ok => {},
                .conflict => return .conflict,
                .io => return .io,
            }
        }

        if (entry.rolled_back) continue;
        entry.committed = false;
        entry.rolled_back = true;
        recount(journal);
        if (!io.persist(journal)) return .io;
    }

    journal.phase = .rolled_back;
    recount(journal);
    return if (io.persist(journal)) .ok else .io;
}

/// Continues a fully staged v5 restart batch forward without reopening any
/// transport package.  Every stage fingerprint, old target fingerprint and
/// final target is already bound by the shared transaction journal.  This is
/// the only forward-recovery path used before the R4X runtime exists.
pub fn resumeBatchForward(io: anytype, journal: *TransactionJournal) ReplayStatus {
    if (!journalValid(journal) or !journal.batch) return .invalid;
    if (journal.phase == .post_boot) return .ok;
    if (journal.phase != .stage and journal.phase != .commit) return .invalid;

    journal.phase = .commit;
    recount(journal);
    if (!io.persist(journal)) return .io;
    var index: usize = 0;
    while (index < journal.payload_count) : (index += 1) {
        var entry = &journal.payloads[index];
        if (!entry.committed) {
            switch (io.commitPayload(entry)) {
                .ok => {},
                .conflict => return .conflict,
                .io => return .io,
            }
            entry.committed = true;
            entry.rolled_back = false;
            recount(journal);
            if (!io.persist(journal)) return .io;
        }
    }

    index = 0;
    while (index < journal.payload_count) : (index += 1) {
        const entry = &journal.payloads[index];
        switch (io.pathState(entry.targetText(), entry.size, entry.checksum)) {
            .match => {},
            .not_found, .other => return .conflict,
            .io => return .io,
        }
    }
    journal.phase = .post_boot;
    recount(journal);
    return if (io.persist(journal)) .ok else .io;
}

/// Performs only post-apply cleanup.  The newly produced backup remains the
/// last-good object; an inherited older backup is removed only with its
/// persisted content fingerprint.
pub fn cleanupToTerminal(io: anytype, journal: *TransactionJournal) ReplayStatus {
    if (!journalValid(journal)) return .invalid;
    if (journal.phase == .cleanup) return .ok;
    if (journal.phase != .applied) return .invalid;

    // Point-of-no-return cleanup is destructive only after the complete
    // package-wide applied state has been re-established.  In particular,
    // never rotate away an older last-good backup while any current target
    // or current backup is missing, foreign or unreadable.
    var index: usize = 0;
    while (index < journal.payload_count) : (index += 1) {
        const entry = &journal.payloads[index];
        switch (io.pathState(entry.targetText(), entry.size, entry.checksum)) {
            .match => {},
            .not_found, .other => return .conflict,
            .io => return .io,
        }
        if (!entry.replace_required) continue;
        switch (io.presence(entry.stageText())) {
            .not_found => {},
            .file, .other => return .conflict,
            .io => return .io,
        }
        if (entry.target_existed) {
            if (entry.old_known) {
                switch (io.pathState(
                    entry.backupText(),
                    entry.old_size,
                    entry.old_checksum,
                )) {
                    .match => {},
                    .not_found, .other => return .conflict,
                    .io => return .io,
                }
            } else {
                switch (io.presence(entry.backupText())) {
                    .file => {},
                    .not_found, .other => return .conflict,
                    .io => return .io,
                }
            }
        } else {
            switch (io.presence(entry.backupText())) {
                .not_found => {},
                .file, .other => return .conflict,
                .io => return .io,
            }
        }
    }

    // The package-wide preflight above stays: it guarantees that no backup is
    // rotated away while ANY payload of the package is still missing, foreign
    // or unreadable.  What changed in 0.60.23 is the second pass - instead of
    // a sequence of independently gated probes and deletes, each payload is
    // handed to ONE checked operation that verifies target, stage, current
    // backup and previous backup and acts on them inside a single filesystem
    // gate.  A local mutation can therefore no longer slip between a check
    // and the delete it authorized.
    //
    // No stage delete happens in there either: reaching cleanup means the
    // stage was already consumed, so a delete could only hit an object that
    // appeared afterwards - and the stage fingerprint IS the fingerprint of
    // the freshly installed payload, so that would be an unrelated file with
    // identical content.
    index = 0;
    while (index < journal.payload_count) : (index += 1) {
        const entry = &journal.payloads[index];
        if (!entry.replace_required) continue;
        switch (io.cleanupPayload(entry)) {
            .ok => {},
            .conflict => return .conflict,
            .io => return .io,
        }
    }
    journal.phase = .cleanup;
    journal.rollback_markers_present = true;
    return if (io.persist(journal)) .ok else .io;
}

pub fn journalValid(journal: *const TransactionJournal) bool {
    if (!journal.valid or
        journal.slot > 1 or
        journal.transaction_generation == 0 or
        journal.journal_generation == 0 or
        journal.package_length == 0 or
        journal.package_payload_count == 0 or
        journal.package_payload_count > max_package_payloads or
        journal.payload_count == 0 or
        journal.payload_count > max_payloads or
        journal.payload_count < journal.package_payload_count or
        journal.payload_count > journal.package_payload_count + (if (journal.batch) @as(u32, 2) else 1) or
        journal.committed_count > journal.payload_count or
        journal.rollback_count > journal.payload_count or
        journal.component_count > journal.package_payload_count or
        !versionTextValid(journal.sourceVersionText()) or
        !versionTextValid(journal.targetVersionText()) or
        !versionTextValid(journal.packageVersionText()) or
        !journalPathsValid(journal))
    {
        return false;
    }
    if ((journal.batch and (!journal.reboot or !journal.components_bound)) or
        (journal.phase == .post_boot and !journal.batch))
    {
        return false;
    }

    if (journal.components_bound) {
        var component_index: usize = 0;
        while (component_index < journal.component_count) : (component_index += 1) {
            const component = &journal.components[component_index];
            if (component.payload_index >= journal.package_payload_count or !componentValid(component)) return false;
            for (journal.components[0..component_index]) |prior| {
                if (component.kind == prior.kind and equalsIgnoreCase(component.nameText(), prior.nameText())) return false;
                if (pathEqualsIgnoreCase(component.targetText(), prior.targetText())) return false;
            }
        }
    }

    var committed_count: u32 = 0;
    var rollback_count: u32 = 0;
    var saw_uncommitted = false;
    var saw_rollback = false;
    var index: usize = 0;
    while (index < journal.payload_count) : (index += 1) {
        const entry = &journal.payloads[index];
        if (entry.target_len == 0 or
            entry.stage_len == 0 or
            entry.backup_len == 0 or
            (entry.old_known and !entry.target_existed) or
            (entry.previous_backup_known and entry.previous_backup_len == 0) or
            (entry.committed and entry.rolled_back))
        {
            return false;
        }
        if (!entry.target_existed and
            (entry.old_known or entry.old_size != 0 or entry.old_checksum != 0))
        {
            return false;
        }
        if (!entry.previous_backup_known and
            (entry.previous_backup_size != 0 or entry.previous_backup_checksum != 0))
        {
            return false;
        }
        if (!entry.replace_required) {
            if (!entry.target_existed) return false;
            if (entry.old_known and
                (entry.old_size != entry.size or entry.old_checksum != entry.checksum))
            {
                return false;
            }
        }

        if (entry.committed) {
            if (saw_uncommitted) return false;
            committed_count += 1;
        } else {
            saw_uncommitted = true;
        }
        if (entry.rolled_back) {
            saw_rollback = true;
            rollback_count += 1;
        } else if (saw_rollback) {
            // Rollback is persisted in reverse payload order.
            return false;
        }
    }
    if (committed_count != journal.committed_count or rollback_count != journal.rollback_count) return false;

    return switch (journal.phase) {
        .prepare, .stage => committed_count == 0 and rollback_count == 0,
        .commit => rollback_count == 0,
        .verify => journal.payload_count == journal.package_payload_count and committed_count == journal.payload_count and rollback_count == 0,
        .inventory => journal.payload_count == journal.package_payload_count + 1 and
            (committed_count == journal.package_payload_count or committed_count == journal.payload_count) and rollback_count == 0,
        .applied, .post_boot, .cleanup => committed_count == journal.payload_count and rollback_count == 0,
        .rollback => true,
        // Current journals mark every payload.  A legacy terminal journal may
        // have counted only replace-required entries and is still safe because
        // terminal recovery performs no further mutation.
        .rolled_back => committed_count == 0 and
            (if (journal.rollback_markers_present)
                rollback_count == journal.payload_count
            else
                allRequiredPayloadsRolledBack(journal)),
    };
}

fn allRequiredPayloadsRolledBack(journal: *const TransactionJournal) bool {
    var index: usize = 0;
    while (index < journal.payload_count) : (index += 1) {
        const entry = &journal.payloads[index];
        if (entry.replace_required and !entry.rolled_back) return false;
    }
    return true;
}

pub fn journalPathsValid(journal: *const TransactionJournal) bool {
    if (!startsWithIgnoreCase(journal.sourceText(), "C:\\R4OS\\UPDATE\\INBOX\\")) return false;
    // The package source takes part in the same alias comparison as every
    // payload path, so it is bound to the same proven-ASCII rule.
    var index: usize = 0;
    while (index < journal.payload_count) : (index += 1) {
        const entry = &journal.payloads[index];
        const target = entry.targetText();
        const stage = entry.stageText();
        const backup = entry.backupText();
        const previous_backup = entry.previousBackupText();
        var expected_stage: [max_path:0]u8 = .{0} ** max_path;
        var expected_backup: [max_path:0]u8 = .{0} ** max_path;
        const expected_stage_len = buildInternal83Name(
            expected_stage[0..],
            target,
            'S',
            journal.transaction_generation,
            index,
        ) orelse return false;
        const expected_backup_len = buildInternal83Name(
            expected_backup[0..],
            target,
            'B',
            journal.transaction_generation,
            index,
        ) orelse return false;
        if (!validTarget(target) or
            !validateShortName83Text(baseName(stage)) or
            !validateShortName83Text(baseName(backup)) or
            !pathEqualsIgnoreCase(parentPathText(target), parentPathText(stage)) or
            !pathEqualsIgnoreCase(parentPathText(target), parentPathText(backup)) or
            !pathEqualsIgnoreCase(stage, expected_stage[0..expected_stage_len]) or
            !pathEqualsIgnoreCase(backup, expected_backup[0..expected_backup_len]) or
            (previous_backup.len != 0 and
                (!validateShortName83Text(baseName(previous_backup)) or
                    !pathEqualsIgnoreCase(parentPathText(target), parentPathText(previous_backup)))))
        {
            return false;
        }

        var path_index: usize = 0;
        while (path_index < 4) : (path_index += 1) {
            const path = journalPayloadPath(entry, path_index);
            if (path.len == 0) continue;
            if (pathEqualsIgnoreCase(journal.sourceText(), path)) return false;
            var other_index: usize = index;
            while (other_index < journal.payload_count) : (other_index += 1) {
                const other = &journal.payloads[other_index];
                var other_path_index: usize = if (other_index == index) path_index + 1 else 0;
                while (other_path_index < 4) : (other_path_index += 1) {
                    const other_path = journalPayloadPath(other, other_path_index);
                    if (other_path.len != 0 and pathEqualsIgnoreCase(path, other_path)) return false;
                }
            }
        }
    }
    return true;
}

pub fn normalizeAbsolutePath(out: []u8, value: []const u8) ?[]const u8 {
    if (out.len < 2 or value.len == 0) return null;
    @memset(out, 0);

    var out_len: usize = 0;
    var input_pos: usize = 0;
    if (value.len >= 3 and isPathSeparator(value[0]) and isDriveLetter(value[1]) and isPathSeparator(value[2])) {
        if (!appendNormalizedPathByte(out, &out_len, std.ascii.toUpper(value[1]))) return null;
        if (!appendNormalizedPathByte(out, &out_len, ':')) return null;
        if (!appendNormalizedPathByte(out, &out_len, '\\')) return null;
        input_pos = 3;
    } else if (value.len >= 2 and isDriveLetter(value[0]) and value[1] == ':') {
        if (!appendNormalizedPathByte(out, &out_len, std.ascii.toUpper(value[0]))) return null;
        if (!appendNormalizedPathByte(out, &out_len, ':')) return null;
        if (value.len > 2 and !isPathSeparator(value[2])) return null;
        if (!appendNormalizedPathByte(out, &out_len, '\\')) return null;
        input_pos = 2;
    } else if (isPathSeparator(value[0])) {
        if (!appendNormalizedPathByte(out, &out_len, '\\')) return null;
        input_pos = 1;
    } else {
        return null;
    }

    while (input_pos < value.len) {
        while (input_pos < value.len and isPathSeparator(value[input_pos])) : (input_pos += 1) {}
        if (input_pos >= value.len) break;
        const start = input_pos;
        while (input_pos < value.len and !isPathSeparator(value[input_pos])) : (input_pos += 1) {}
        const component = value[start..input_pos];
        if (equalsIgnoreCase(component, ".")) continue;
        if (equalsIgnoreCase(component, "..")) return null;
        if (component.len == 0 or component[component.len - 1] == ' ' or component[component.len - 1] == '.') return null;
        if (!std.unicode.utf8ValidateSlice(component)) return null;
        for (component) |ch| {
            if (ch < ' ' or ch == 0x7F or
                ch == '"' or ch == '*' or ch == ':' or ch == ';' or
                ch == '<' or ch == '>' or ch == '?' or ch == '|')
            {
                return null;
            }
        }
        if (out_len > 0 and out[out_len - 1] != '\\') {
            if (!appendNormalizedPathByte(out, &out_len, '\\')) return null;
        }
        for (component) |ch| {
            if (!appendNormalizedPathByte(out, &out_len, ch)) return null;
        }
    }
    if (out_len == 0 or out_len + 1 > out.len) return null;
    out[out_len] = 0;
    return out[0..out_len];
}

pub fn buildInternal83Name(
    out: []u8,
    target: []const u8,
    prefix: u8,
    transaction_generation: u64,
    index: usize,
) ?usize {
    if (prefix != 'S' and prefix != 'B') return null;
    var separator: ?usize = null;
    var cursor: usize = target.len;
    while (cursor > 0) {
        cursor -= 1;
        if (isPathSeparator(target[cursor])) {
            separator = cursor;
            break;
        }
    }
    const parent_len = if (separator) |value| value + 1 else 0;
    const name_len: usize = 12;
    if (parent_len + name_len + 1 > out.len) return null;
    @memset(out, 0);
    if (parent_len > 0) @memcpy(out[0..parent_len], target[0..parent_len]);
    out[parent_len] = prefix;
    const mixed: u32 = @truncate(
        (transaction_generation *% 0x9E37_79B9) ^
            (@as(u64, @intCast(index)) *% 0x85EB_CA6B),
    );
    var digit: usize = 0;
    while (digit < 7) : (digit += 1) {
        const shift: u5 = @intCast((6 - digit) * 4);
        out[parent_len + 1 + digit] = hexDigit(@truncate(mixed >> shift));
    }
    @memcpy(out[parent_len + 8 .. parent_len + 12], ".R4U");
    return parent_len + name_len;
}

pub fn validateShortName83Text(name: []const u8) bool {
    if (name.len == 0 or name.len > 12 or name[0] == '.') return false;
    var dot: ?usize = null;
    for (name, 0..) |ch, index| {
        if (ch == '.') {
            if (dot != null) return false;
            dot = index;
            continue;
        }
        const upper = std.ascii.toUpper(ch);
        if (!((upper >= 'A' and upper <= 'Z') or
            (upper >= '0' and upper <= '9') or
            upper == '$' or upper == '%' or upper == '\'' or upper == '-' or
            upper == '_' or upper == '@' or upper == '~'))
        {
            return false;
        }
    }
    const base_len = dot orelse name.len;
    const ext_len = if (dot) |at| name.len - at - 1 else 0;
    return base_len >= 1 and base_len <= 8 and ext_len <= 3 and
        (dot == null or ext_len >= 1);
}

pub fn pathEqualsIgnoreCase(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var index: usize = 0;
    while (index < a.len) : (index += 1) {
        const left = if (a[index] == '/') '\\' else std.ascii.toUpper(a[index]);
        const right = if (b[index] == '/') '\\' else std.ascii.toUpper(b[index]);
        if (left != right) return false;
    }
    return true;
}

pub fn versionTextValid(value: []const u8) bool {
    if (value.len == 0 or value.len >= max_version) return false;
    var digits: usize = 0;
    var part: u32 = 0;
    for (value) |ch| {
        if (ch >= '0' and ch <= '9') {
            part = std.math.mul(u32, part, 10) catch return false;
            part = std.math.add(u32, part, @as(u32, ch - '0')) catch return false;
            digits += 1;
            continue;
        }
        if (ch != '.' or digits == 0) return false;
        digits = 0;
        part = 0;
    }
    return digits != 0;
}

pub fn checksum(data: []const u8) u32 {
    return checksumUpdate(checksum_seed, data);
}

pub fn checksumUpdate(seed: u32, data: []const u8) u32 {
    var out = seed;
    for (data) |ch| {
        out ^= ch;
        out *%= 16777619;
    }
    return out;
}

fn recount(journal: *TransactionJournal) void {
    journal.committed_count = 0;
    journal.rollback_count = 0;
    var index: usize = 0;
    while (index < journal.payload_count) : (index += 1) {
        if (journal.payloads[index].committed) journal.committed_count += 1;
        if (journal.payloads[index].rolled_back) journal.rollback_count += 1;
    }
}

fn parsePhase(value: []const u8) ?JournalPhase {
    inline for (std.meta.fields(JournalPhase)) |field| {
        if (equalsIgnoreCase(value, field.name)) return @enumFromInt(field.value);
    }
    return null;
}

/// The alias detection in this module folds case byte-wise, which is only a
/// proof for ASCII: NTFS resolves names through `$UpCase` and folds many
/// further pairs.  Until 0.60.24 that gap was closed by refusing non-ASCII
/// targets outright.
///
/// Since 0.60.24 the authoritative check happens where the backend can
/// actually be asked: `validateTargetAliases` in the updater consults the
/// volume's own collation through R4SYS `path_names_equal_collated` BEFORE
/// the prepare journal is written.  The fold here remains as a conservative
/// structural sanity check - it can only ever report FEWER collisions than
/// the backend, never more - so non-ASCII targets are installable again.
fn validTarget(path: []const u8) bool {
    if (pathEqualsIgnoreCase(path, "\\boot\\r4os.elf")) return true;
    if (pathEqualsIgnoreCase(path, "C:\\CONFIG.R4S")) return true;
    if (startsWithIgnoreCase(path, "C:\\R4OS\\LIBS\\") and endsWithIgnoreCase(path, ".R4L")) return true;
    if (startsWithIgnoreCase(path, "C:\\R4OS\\DRIVERS\\") and endsWithIgnoreCase(path, ".R4D")) return true;
    if (startsWithIgnoreCase(path, "C:\\R4OS\\PROTOCOLS\\") and endsWithIgnoreCase(path, ".R4P")) return true;
    if (startsWithIgnoreCase(path, "C:\\R4OS\\SERVICES\\") and endsWithIgnoreCase(path, ".R4X")) return true;
    if (startsWithIgnoreCase(path, "C:\\R4OS\\SOFTWARE\\")) return true;
    if (startsWithIgnoreCase(path, "C:\\R4OS\\SUBSYSTEMS\\") and endsWithIgnoreCase(path, ".R4X")) return true;
    if (startsWithIgnoreCase(path, "C:\\R4OS\\FONTS\\") and endsWithIgnoreCase(path, ".R4F")) return true;
    if (startsWithIgnoreCase(path, "C:\\R4OS\\CONFIG\\")) return true;
    if (startsWithIgnoreCase(path, "C:\\R4OS\\SDK\\")) return true;
    if (startsWithIgnoreCase(path, "C:\\TEMP\\")) return true;
    if (path.len >= 4 and isDriveLetter(path[0]) and
        std.ascii.toUpper(path[0]) != 'C' and path[1] == ':' and path[2] == '\\')
    {
        return true;
    }
    return false;
}

fn journalPayloadPath(entry: *const JournalPayload, index: usize) []const u8 {
    return switch (index) {
        0 => entry.targetText(),
        1 => entry.stageText(),
        2 => entry.backupText(),
        3 => entry.previousBackupText(),
        else => &[_]u8{},
    };
}

fn parentPathText(path: []const u8) []const u8 {
    var index = path.len;
    while (index > 1) : (index -= 1) {
        if (isPathSeparator(path[index - 1])) return path[0 .. index - 1];
    }
    return path[0..1];
}

fn baseName(path: []const u8) []const u8 {
    var index = path.len;
    while (index > 0) : (index -= 1) {
        if (isPathSeparator(path[index - 1])) return path[index..];
    }
    return path;
}

fn journalLinesKnown(data: []const u8, modern: bool, component_format: bool, current: bool) bool {
    const common = [_][]const u8{
        "R4U_JOURNAL=",
        "TRANSACTION_GENERATION=",
        "JOURNAL_GENERATION=",
        "PHASE=",
        "PACKAGE_DIGEST=",
        "PACKAGE_LENGTH=",
        "SOURCE=",
        "PAYLOADS=",
        "COMMITTED=",
        "ROLLED_BACK=",
        "REBOOT=",
    };
    var pos: usize = 0;
    while (nextLine(data, &pos)) |line_raw| {
        const line = trim(line_raw);
        if (line.len == 0) return false;
        if (startsWith(line, "PAYLOAD;")) continue;
        if (startsWith(line, "COMPONENT;")) {
            if (!component_format) return false;
            continue;
        }
        var matched = false;
        for (common) |prefix| {
            if (startsWith(line, prefix)) {
                matched = true;
                break;
            }
        }
        if (!matched and modern) {
            const current_fields = [_][]const u8{
                "MANIFEST_CHECKSUM=",
                "COMPONENT_DIGEST=",
                "COMPONENTS=",
                "COMPONENT_BINDING=",
                "PRIORITY=",
                "PACKAGE_VERSION=",
                "SOURCE_RELEASE=",
                "RELEASE=",
            };
            for (current_fields) |prefix| {
                if (startsWith(line, prefix)) {
                    matched = true;
                    break;
                }
            }
        }
        if (!matched and component_format and startsWith(line, "PACKAGE_PAYLOADS=")) matched = true;
        if (!matched and current and startsWith(line, "BATCH=")) matched = true;
        if (!matched and !modern) {
            const legacy_fields = [_][]const u8{ "SOURCE_VERSION=", "TARGET_VERSION=" };
            for (legacy_fields) |prefix| {
                if (startsWith(line, prefix)) {
                    matched = true;
                    break;
                }
            }
        }
        if (!matched) return false;
    }
    return true;
}

fn payloadFieldsKnown(line: []const u8) bool {
    const allowed = [_][]const u8{
        "index=",
        "target=",
        "stage=",
        "backup=",
        "previous_backup=",
        "previous_known=",
        "previous_size=",
        "previous_checksum=",
        "size=",
        "checksum=",
        "existed=",
        "old_known=",
        "old_size=",
        "old_checksum=",
        "required=",
        "committed=",
        "rolled_back=",
    };
    var pos: usize = 0;
    var segment_index: usize = 0;
    while (pos <= line.len) : (segment_index += 1) {
        const next = std.mem.indexOfScalarPos(u8, line, pos, ';') orelse line.len;
        const segment = line[pos..next];
        if (segment_index == 0) {
            if (!std.mem.eql(u8, segment, "PAYLOAD")) return false;
        } else {
            var matched = false;
            for (allowed) |prefix| {
                if (startsWith(segment, prefix)) {
                    matched = true;
                    break;
                }
            }
            if (!matched) return false;
        }
        if (next == line.len) break;
        pos = next + 1;
    }
    for (allowed) |prefix| {
        if (fieldOccurrences(line, prefix) > 1) return false;
    }
    return true;
}

fn componentFieldsKnown(line: []const u8) bool {
    const allowed = [_][]const u8{ "index=", "payload=", "kind=", "name=", "target=", "version=" };
    var pos: usize = 0;
    var segment_index: usize = 0;
    while (pos <= line.len) : (segment_index += 1) {
        const next = std.mem.indexOfScalarPos(u8, line, pos, ';') orelse line.len;
        const segment = line[pos..next];
        if (segment_index == 0) {
            if (!std.mem.eql(u8, segment, "COMPONENT")) return false;
        } else {
            var matched = false;
            for (allowed) |prefix| {
                if (startsWith(segment, prefix)) {
                    matched = true;
                    break;
                }
            }
            if (!matched) return false;
        }
        if (next == line.len) break;
        pos = next + 1;
    }
    for (allowed) |prefix| {
        if (fieldOccurrences(line, prefix) != 1) return false;
    }
    return true;
}

fn componentValid(component: *const JournalComponent) bool {
    if (!componentTokenValid(component.nameText()) or
        !versionTextValid(component.versionText()) or
        component.target_len == 0 or component.target_len >= max_path or
        component.targetText()[0] != '/') return false;
    for (component.targetText()) |byte| {
        if (byte < 0x20 or byte >= 0x7f or byte == ';' or byte == '=' or byte == '|' or byte == '\\') return false;
    }
    return switch (component.kind) {
        .kernel => equalsIgnoreCase(component.nameText(), "KERNEL") and pathEqualsIgnoreCase(component.targetText(), "/boot/r4os.elf"),
        .r4x => endsWithIgnoreCase(component.targetText(), ".R4X"),
        .r4l => endsWithIgnoreCase(component.targetText(), ".R4L"),
        .r4d => endsWithIgnoreCase(component.targetText(), ".R4D"),
        .r4p => endsWithIgnoreCase(component.targetText(), ".R4P"),
    };
}

fn componentTokenValid(value: []const u8) bool {
    if (value.len == 0 or value.len > 48) return false;
    for (value) |byte| {
        if (byte <= ' ' or byte >= 0x7f or byte == ';' or byte == '=' or byte == '|') return false;
    }
    return true;
}

fn journalField(journal: []const u8, key: []const u8) ?[]const u8 {
    var pos: usize = 0;
    var found: ?[]const u8 = null;
    while (nextLine(journal, &pos)) |line_raw| {
        const line = trim(line_raw);
        if (startsWith(line, key)) {
            if (found != null) return null;
            found = line[key.len..];
        }
    }
    return found;
}

fn requiredField(line: []const u8, key: []const u8) ?[]const u8 {
    if (fieldOccurrences(line, key) != 1) return null;
    return fieldValue(line, key);
}

fn optionalField(line: []const u8, key: []const u8) ?[]const u8 {
    if (fieldOccurrences(line, key) > 1) return null;
    return fieldValue(line, key);
}

fn fieldValue(line: []const u8, key: []const u8) ?[]const u8 {
    var pos: usize = 0;
    var found: ?[]const u8 = null;
    while (pos <= line.len) {
        const next = std.mem.indexOfScalarPos(u8, line, pos, ';') orelse line.len;
        const part = line[pos..next];
        if (startsWith(part, key)) {
            if (found != null) return null;
            found = part[key.len..];
        }
        if (next == line.len) break;
        pos = next + 1;
    }
    return found;
}

fn fieldOccurrences(line: []const u8, key: []const u8) usize {
    var pos: usize = 0;
    var count: usize = 0;
    while (pos <= line.len) {
        const next = std.mem.indexOfScalarPos(u8, line, pos, ';') orelse line.len;
        if (startsWith(line[pos..next], key)) count += 1;
        if (next == line.len) break;
        pos = next + 1;
    }
    return count;
}

fn parseBool01(value: []const u8) ?bool {
    if (std.mem.eql(u8, value, "0")) return false;
    if (std.mem.eql(u8, value, "1")) return true;
    return null;
}

fn parseYesNo(value: []const u8) ?bool {
    if (equalsIgnoreCase(value, "no")) return false;
    if (equalsIgnoreCase(value, "yes")) return true;
    return null;
}

fn parseU32(value: []const u8) ?u32 {
    const parsed = parseU64(value) orelse return null;
    return if (parsed <= std.math.maxInt(u32)) @intCast(parsed) else null;
}

fn parseU64(value_raw: []const u8) ?u64 {
    const value = trim(value_raw);
    if (value.len == 0) return null;
    var out: u64 = 0;
    for (value) |ch| {
        if (ch < '0' or ch > '9') return null;
        out = std.math.mul(u64, out, 10) catch return null;
        out = std.math.add(u64, out, @as(u64, ch - '0')) catch return null;
    }
    return out;
}

fn nextLine(value: []const u8, pos: *usize) ?[]const u8 {
    if (pos.* >= value.len) return null;
    const start = pos.*;
    while (pos.* < value.len and value[pos.*] != '\n') : (pos.* += 1) {}
    const end = pos.*;
    if (pos.* < value.len) pos.* += 1;
    return value[start..end];
}

fn trim(value: []const u8) []const u8 {
    return std.mem.trim(u8, value, " \t\r\n");
}

fn startsWith(value: []const u8, prefix: []const u8) bool {
    return value.len >= prefix.len and std.mem.eql(u8, value[0..prefix.len], prefix);
}

fn startsWithIgnoreCase(value: []const u8, prefix: []const u8) bool {
    if (value.len < prefix.len) return false;
    return pathEqualsIgnoreCase(value[0..prefix.len], prefix);
}

fn endsWithIgnoreCase(value: []const u8, suffix: []const u8) bool {
    if (value.len < suffix.len) return false;
    return pathEqualsIgnoreCase(value[value.len - suffix.len ..], suffix);
}

fn equalsIgnoreCase(a: []const u8, b: []const u8) bool {
    return pathEqualsIgnoreCase(a, b);
}

fn isPathSeparator(ch: u8) bool {
    return ch == '\\' or ch == '/';
}

fn isDriveLetter(ch: u8) bool {
    const upper = std.ascii.toUpper(ch);
    return upper >= 'A' and upper <= 'Z';
}

fn appendNormalizedPathByte(out: []u8, out_len: *usize, value: u8) bool {
    if (out_len.* + 1 >= out.len) return false;
    out[out_len.*] = value;
    out_len.* += 1;
    return true;
}

fn copyTextZ(out: []u8, value: []const u8) ?usize {
    if (value.len + 1 > out.len) return null;
    @memset(out, 0);
    @memcpy(out[0..value.len], value);
    return value.len;
}

fn hexDigit(value: u4) u8 {
    return if (value < 10) '0' + @as(u8, value) else 'A' + @as(u8, value - 10);
}

fn appendLine(out: []u8, len: *usize, value: []const u8) bool {
    return appendText(out, len, value) and appendByte(out, len, '\n');
}

fn appendKeyValue(out: []u8, len: *usize, key: []const u8, value: []const u8) bool {
    return appendText(out, len, key) and appendText(out, len, value) and appendByte(out, len, '\n');
}

fn appendKeyU64(out: []u8, len: *usize, key: []const u8, value: u64) bool {
    return appendText(out, len, key) and appendU64(out, len, value) and appendByte(out, len, '\n');
}

fn appendText(out: []u8, len: *usize, value: []const u8) bool {
    if (value.len > out.len -| len.*) return false;
    @memcpy(out[len.* .. len.* + value.len], value);
    len.* += value.len;
    return true;
}

fn appendByte(out: []u8, len: *usize, value: u8) bool {
    if (len.* >= out.len) return false;
    out[len.*] = value;
    len.* += 1;
    return true;
}

fn appendU64(out: []u8, len: *usize, value_raw: u64) bool {
    var digits: [20]u8 = undefined;
    var value = value_raw;
    var count: usize = 0;
    if (value == 0) {
        digits[0] = '0';
        count = 1;
    } else {
        while (value != 0) : (value /= 10) {
            digits[count] = '0' + @as(u8, @intCast(value % 10));
            count += 1;
        }
    }
    if (count > out.len -| len.*) return false;
    var index: usize = 0;
    while (index < count) : (index += 1) {
        out[len.* + index] = digits[count - index - 1];
    }
    len.* += count;
    return true;
}

// ---------------------------------------------------------------------------
// Tests (run by the 0.59.14 gate against this shared core)
// ---------------------------------------------------------------------------

const testing = std.testing;

fn testJournalWithTarget(journal: *TransactionJournal, target: []const u8) void {
    journal.* = .{};
    journal.valid = true;
    journal.transaction_generation = 5;
    journal.journal_generation = 1;
    journal.package_payload_count = 1;
    journal.payload_count = 1;

    const source = "C:\\R4OS\\UPDATE\\INBOX\\PKG.R4U";
    @memcpy(journal.source_path[0..source.len], source);
    journal.source_len = source.len;

    const entry = &journal.payloads[0];
    @memcpy(entry.target_path[0..target.len], target);
    entry.target_len = target.len;
    entry.stage_len = buildInternal83Name(entry.stage_path[0..], target, 'S', 5, 0) orelse 0;
    entry.backup_len = buildInternal83Name(entry.backup_path[0..], target, 'B', 5, 0) orelse 0;
}

test "v5 journal roundtrip preserves v4 component bindings and batch marker" {
    var journal: TransactionJournal = undefined;
    testJournalWithTarget(&journal, "C:\\R4OS\\SOFTWARE\\TERMINAL\\TERMINAL.R4X");
    journal.package_digest = 1234;
    journal.package_length = 5678;
    journal.manifest_checksum = 9012;
    journal.component_digest = 3456;
    journal.component_count = 1;
    const component = &journal.components[0];
    component.kind = .r4x;
    component.name_len = copyTextZ(component.name[0..], "TERMINAL") orelse 0;
    component.target_len = copyTextZ(component.target[0..], "/R4OS/SOFTWARE/TERMINAL/TERMINAL.R4X") orelse 0;
    component.version_len = copyTextZ(component.version[0..], "0.1.3") orelse 0;
    journal.foundation = true;
    journal.payloads[0].size = 42;
    journal.payloads[0].checksum = 99;
    journal.source_version_len = copyTextZ(journal.source_version[0..], "0.63.10") orelse 0;
    journal.target_version_len = copyTextZ(journal.target_version[0..], "0.63.11") orelse 0;
    journal.package_version_len = copyTextZ(journal.package_version[0..], "2.1.0") orelse 0;
    var buffer: [8192]u8 = undefined;
    const encoded = serializeJournal(&journal, buffer[0..]) orelse return error.TestUnexpectedResult;
    try testing.expect(std.mem.startsWith(u8, encoded, "R4U_JOURNAL=5\n"));
    var parsed: TransactionJournal = .{};
    try testing.expect(parseJournalInto(encoded, 1, &parsed));
    try testing.expectEqualStrings("0.63.10", parsed.sourceReleaseText());
    try testing.expectEqualStrings("0.63.11", parsed.releaseText());
    try testing.expectEqualStrings("2.1.0", parsed.packageVersionText());
    try testing.expectEqual(@as(u32, 3456), parsed.component_digest);
    try testing.expectEqual(@as(u32, 1), parsed.component_count);
    try testing.expect(parsed.components_bound);
    try testing.expectEqualStrings("TERMINAL", parsed.components[0].nameText());
    try testing.expectEqualStrings("0.1.3", parsed.components[0].versionText());
    try testing.expect(parsed.foundation);
    try testing.expect(!parsed.batch);
}

test "restart batch recovery commits every bound payload before post boot" {
    var journal: TransactionJournal = undefined;
    testJournalWithTarget(&journal, "C:\\R4OS\\SOFTWARE\\APP\\APP.R4X");
    journal.batch = true;
    journal.reboot = true;
    journal.phase = .stage;
    journal.package_digest = 10;
    journal.package_length = 1000;
    journal.manifest_checksum = 20;
    journal.component_digest = 30;
    journal.source_version_len = copyTextZ(journal.source_version[0..], "0.63.12") orelse 0;
    journal.target_version_len = copyTextZ(journal.target_version[0..], "0.63.13") orelse 0;
    journal.package_version_len = copyTextZ(journal.package_version[0..], "1.0.0") orelse 0;
    journal.payloads[0].size = 50;
    journal.payloads[0].checksum = 60;
    journal.payloads[0].target_existed = true;
    journal.payloads[0].old_known = true;
    journal.payloads[0].old_size = 40;
    journal.payloads[0].old_checksum = 41;

    const MockIo = struct {
        commit_count: usize = 0,
        persist_count: usize = 0,

        pub fn persist(self: *@This(), _: *TransactionJournal) bool {
            self.persist_count += 1;
            return true;
        }

        pub fn commitPayload(self: *@This(), _: *const JournalPayload) MutationStatus {
            self.commit_count += 1;
            return .ok;
        }

        pub fn pathState(_: *@This(), _: []const u8, _: u64, _: u32) PathState {
            return .match;
        }
    };
    var io: MockIo = .{};
    try testing.expectEqual(ReplayStatus.ok, resumeBatchForward(&io, &journal));
    try testing.expectEqual(JournalPhase.post_boot, journal.phase);
    try testing.expectEqual(@as(u32, 1), journal.committed_count);
    try testing.expectEqual(@as(usize, 1), io.commit_count);
    try testing.expect(io.persist_count >= 3);
}

test "an ASCII target set stays acceptable" {
    var journal: TransactionJournal = undefined;
    testJournalWithTarget(&journal, "C:\\R4OS\\SERVICES\\SSHD.R4X");
    try testing.expect(journalPathsValid(&journal));
}

test "a subsystem R4X target is durable recovery state but subsystem data is not" {
    var journal: TransactionJournal = undefined;
    testJournalWithTarget(&journal, "C:\\R4OS\\SUBSYSTEMS\\r4os.gb\\R4GB.R4X");
    try testing.expect(journalPathsValid(&journal));

    testJournalWithTarget(&journal, "C:\\R4OS\\SUBSYSTEMS\\r4os.gb\\SAVE\\GAME.SAV");
    try testing.expect(!journalPathsValid(&journal));
}

test "a non-ASCII target is installable again now that collation is backend-exact" {
    // Before 0.60.24 these were refused outright, because a byte-wise ASCII
    // fold cannot prove non-ASCII identity against NTFS `$UpCase`.  The
    // authoritative check now happens in the updater against the volume's own
    // collation before the prepare journal, so the structural validator no
    // longer has to fail closed here.
    var journal: TransactionJournal = undefined;
    testJournalWithTarget(&journal, "C:\\R4OS\\SOFTWARE\\GR\xC3\x9CN.R4X");
    try testing.expect(journalPathsValid(&journal));

    // The package source path may carry non-ASCII as well.
    testJournalWithTarget(&journal, "C:\\R4OS\\SERVICES\\SSHD.R4X");
    const source = "C:\\R4OS\\UPDATE\\INBOX\\P\xC3\x84K.R4U";
    @memset(journal.source_path[0..], 0);
    @memcpy(journal.source_path[0..source.len], source);
    journal.source_len = source.len;
    try testing.expect(journalPathsValid(&journal));
}

test "the structural fold still catches every ASCII-visible duplicate" {
    // The fold is deliberately conservative: it may report FEWER collisions
    // than the backend would, never more.  What it does see must still be
    // rejected here, so a package that is obviously self-colliding never
    // reaches the prepare journal.
    var journal: TransactionJournal = undefined;
    testJournalWithTarget(&journal, "C:\\R4OS\\SOFTWARE\\MIXED.R4X");
    journal.payload_count = 2;
    const second = &journal.payloads[1];
    const target = "C:\\R4OS\\SOFTWARE\\mixed.r4x";
    @memcpy(second.target_path[0..target.len], target);
    second.target_len = target.len;
    second.stage_len = buildInternal83Name(second.stage_path[0..], target, 'S', 5, 1) orelse 0;
    second.backup_len = buildInternal83Name(second.backup_path[0..], target, 'B', 5, 1) orelse 0;
    try testing.expect(!journalPathsValid(&journal));
}

test "duplicate payload targets are still rejected package wide" {
    var journal: TransactionJournal = undefined;
    testJournalWithTarget(&journal, "C:\\R4OS\\SERVICES\\SSHD.R4X");
    journal.payload_count = 2;
    const second = &journal.payloads[1];
    const target = "c:\\r4os\\services\\sshd.r4x"; // same object, different case
    @memcpy(second.target_path[0..target.len], target);
    second.target_len = target.len;
    second.stage_len = buildInternal83Name(second.stage_path[0..], target, 'S', 5, 1) orelse 0;
    second.backup_len = buildInternal83Name(second.backup_path[0..], target, 'B', 5, 1) orelse 0;
    try testing.expect(!journalPathsValid(&journal));
}
