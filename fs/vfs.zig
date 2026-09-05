// FS-neutral filesystem dispatch (VFS).
//
// Owns the per-drive-letter volume table and dispatches every generic file
// operation to the mounted filesystem implementation.  Consumers speak the
// neutral vocabulary defined here (NodeRef, Entry, Volume) and never a
// filesystem-internal one (FAT32 cluster numbers, NTFS MFT references).
//
// Timestamps in Entry stay in the R4OS ABI format (DOS-style u16 date/time
// pairs from the fixed-layout file_info payload).  Every filesystem backend
// converts its native time format to this contract at this boundary.
//
// Filesystem probing (recognizing a volume on a partition) stays
// filesystem-specific in the MBR scanner; only mounted volumes live here.

const fat32 = @import("fat/fat32.zig");
const ntfs_fs = @import("ntfs/ntfs.zig");
const access = @import("../storage/access_runtime.zig");
const locks = @import("../memory/owner_locks.zig");
const drive = @import("drive.zig");

/// Windows-parity component limit (0.60.19): 255 characters, UTF-8 (BMP)
/// worst case 765 bytes, buffered as 768.
pub const NAME_MAX: usize = 768;
pub const MAX_MOUNTED_VOLUMES: usize = 26;

// ABI attribute bits (DOS layout, shared by FAT32 and NTFS file_attributes).
pub const ATTR_READ_ONLY: u8 = 0x01;
pub const ATTR_HIDDEN: u8 = 0x02;
pub const ATTR_SYSTEM: u8 = 0x04;
pub const ATTR_DIRECTORY: u8 = 0x10;
pub const ATTR_ARCHIVE: u8 = 0x20;

/// Opaque per-filesystem node reference.  FAT32 stores the first cluster,
/// NTFS will store the MFT record reference.  Only the owning filesystem
/// interprets the value; consumers treat it as a directory/file handle.
pub const NodeRef = u64;

pub const AppendStatus = enum(u8) {
    ok,
    invalid,
    not_found,
    offset_mismatch,
    too_large,
    io,
};

pub const LookupStatus = enum(u8) {
    found,
    not_found,
    io,
};

pub const RenameStatus = enum(u8) {
    ok,
    not_found,
    not_atomic,
    conflict,
    io,
};

pub const AtomicReplaceResult = enum(i32) {
    ok = 0,
    invalid = -1,
    not_found = -2,
    alias = -3,
    conflict = -4,
    read_only = -5,
    io = -6,
    not_atomic = -7,
};

pub const DeleteIfMatchResult = enum(u8) {
    deleted,
    not_found,
    mismatch,
    io,
};

/// Outcome of the recovery-only compare-and-delete (0.60.21).  It extends
/// the plain identity delete by the states that only an alias-first publish
/// window can produce.
pub const RecoveryDeleteResult = enum(u8) {
    deleted,
    /// Only a surplus index alias was detached; the object is still
    /// reachable under its canonical name.
    unlinked,
    not_found,
    mismatch,
    directory,
    /// A half-state the backend deliberately refuses to guess about.
    unsupported,
    io,
};

pub const Entry = struct {
    name: [NAME_MAX]u8 = .{0} ** NAME_MAX,
    name_len: usize = 0,
    attr: u8 = 0,
    node: NodeRef = 0,
    // Filesystem-internal generation of `node`. NTFS stores the MFT record
    // sequence here so a recycled record can never satisfy an ownership
    // check. FAT32 has no corresponding generation and leaves it zero.
    node_generation: u16 = 0,
    // Internal traversal/read guard; not part of any public R4SYS payload.
    reparse: bool = false,
    size: u64 = 0,
    created_time: u16 = 0,
    created_date: u16 = 0,
    access_date: u16 = 0,
    modified_time: u16 = 0,
    modified_date: u16 = 0,

    pub fn isDir(self: Entry) bool {
        return (self.attr & ATTR_DIRECTORY) != 0;
    }

    pub fn isReadOnly(self: Entry) bool {
        return (self.attr & ATTR_READ_ONLY) != 0;
    }

    pub fn isHiddenOrSystem(self: Entry) bool {
        return (self.attr & (ATTR_HIDDEN | ATTR_SYSTEM)) != 0;
    }
};

pub const Volume = union(enum) {
    fat32: fat32.Volume,
    ntfs: ntfs_fs.Volume,

    pub fn accessReference(self: Volume) ?access.MountRef {
        return switch (self) {
            inline else => |v| v.mount_ref,
        };
    }

    pub fn storageRegion(self: Volume) ?access.Region {
        if (self.accessReference()) |ref| return (access.mountSnapshot(ref) catch return null).region;
        const location: ntfs_fs.StorageLocation = switch (self) {
            .fat32 => |v| .{ .device = v.device_index, .first = @as(u64, v.partition_lba), .count = @as(u64, v.total_sectors) },
            .ntfs => |v| ntfs_fs.storageLocation(v) orelse return null,
        };
        return .{ .device = access.deviceReference(@intCast(location.device)) orelse return null, .first = location.first, .count = location.count };
    }

    fn bind(self: *Volume, ref: access.MountRef) void {
        switch (self.*) {
            inline else => |*v| v.mount_ref = ref,
        }
    }

    pub fn clusterBytes(self: Volume) u32 {
        return switch (self) {
            .fat32 => |v| v.clusterBytes(),
            .ntfs => |v| v.cluster_bytes,
        };
    }

    pub fn totalClusters(self: Volume) u32 {
        return switch (self) {
            .fat32 => |v| v.totalClusters(),
            .ntfs => |v| @intCast((v.total_sectors * 512) / v.cluster_bytes),
        };
    }

    pub fn rootNode(self: Volume) NodeRef {
        return switch (self) {
            .fat32 => |v| v.root_cluster,
            .ntfs => ntfs_fs.rootRecord(),
        };
    }

    pub fn isReadOnly(self: Volume) bool {
        // NTFS gained a phase-1 write path in 0.60.6; mkdir/rmdir/rename and
        // atomic replace remain unsupported until 0.60.7/0.60.8.
        return switch (self) {
            .fat32 => false,
            .ntfs => false,
        };
    }
};

var mounted_volumes: [MAX_MOUNTED_VOLUMES]?Volume = .{null} ** MAX_MOUNTED_VOLUMES;

// Internal mount of the unlettered FAT32 boot partition (Windows-ESP
// model, 0.60.11).  Only the letterless /boot subtree of the system drive
// routes here; the volume never appears in the lettered table or in the
// drive registry.  First boot partition wins.
var boot_volume_slot: ?Volume = null;

pub fn init() void {
    mounted_volumes = .{null} ** MAX_MOUNTED_VOLUMES;
    boot_volume_slot = null;
}

pub fn mountBootVolume(volume: Volume) bool {
    const region = volume.storageRegion() orelse return false;
    const guard = locks.storage.acquire();
    defer locks.storage.release(guard);
    if (boot_volume_slot != null) return false;
    var bound = volume;
    bound.bind(access.bindMountLocked(26, region, true) catch return false);
    boot_volume_slot = bound;
    return true;
}

pub fn bootVolume() ?Volume {
    const guard = locks.storage.acquire();
    defer locks.storage.release(guard);
    return boot_volume_slot;
}

pub fn mountForDrive(letter: u8, volume: Volume) bool {
    const index = driveIndex(letter) orelse return false;
    const region = volume.storageRegion() orelse return false;
    const guard = locks.storage.acquire();
    defer locks.storage.release(guard);
    if (mounted_volumes[index] != null) return false;
    var bound = volume;
    bound.bind(access.bindMountLocked(@intCast(index), region, index == 'C' - 'A') catch return false);
    mounted_volumes[index] = bound;
    return true;
}

pub fn volumeForDrive(letter: u8) ?Volume {
    const index = driveIndex(letter) orelse return null;
    const guard = locks.storage.acquire();
    defer locks.storage.release(guard);
    return mounted_volumes[index];
}

// The storage transaction has already stopped admissions and drained I/O.
// Generation invalidation and removal are one short metadata publication.
pub fn unmount(ref: access.MountRef) bool {
    const guard = locks.storage.acquire();
    defer locks.storage.release(guard);
    access.unbindMountLocked(ref) catch return false;
    if (ref.slot == 26) boot_volume_slot = null else {
        mounted_volumes[ref.slot] = null;
        drive.unmountLocked(@intCast('A' + ref.slot));
    }
    return true;
}

pub fn resolvePathStatus(volume: Volume, path: []const u8, out: *NodeRef) LookupStatus {
    return switch (volume) {
        .fat32 => |v| blk: {
            var node: u32 = undefined;
            const status = lookupStatusFromFat32(fat32.resolvePathStatus(v, path, &node));
            if (status == .found) out.* = node;
            break :blk status;
        },
        .ntfs => |v| lookupStatusFromNtfs(ntfs_fs.resolvePathStatus(v, path, out)),
    };
}

pub fn resolvePath(volume: Volume, path: []const u8) ?NodeRef {
    var out: NodeRef = undefined;
    return if (resolvePathStatus(volume, path, &out) == .found) out else null;
}

pub fn resolveEntryStatus(volume: Volume, path: []const u8, out: *Entry) LookupStatus {
    return switch (volume) {
        .fat32 => |v| blk: {
            var entry: fat32.Entry = undefined;
            const status = lookupStatusFromFat32(fat32.resolveEntryStatus(v, path, &entry));
            if (status == .found) out.* = entryFromFat32(entry);
            break :blk status;
        },
        .ntfs => |v| blk: {
            var entry: ntfs_fs.Entry = undefined;
            const status = lookupStatusFromNtfs(ntfs_fs.resolveEntryStatus(v, path, &entry));
            if (status == .found) out.* = entryFromNtfs(entry);
            break :blk status;
        },
    };
}

pub fn resolveEntry(volume: Volume, path: []const u8) ?Entry {
    var out: Entry = undefined;
    return if (resolveEntryStatus(volume, path, &out) == .found) out else null;
}

pub fn lookupDiagnosticStage(volume: Volume) u32 {
    return switch (volume) {
        .fat32 => 0,
        .ntfs => ntfs_fs.lookupDiagnosticStage(),
    };
}

pub fn lookupEntryStatus(volume: Volume, parent: NodeRef, name: []const u8, out: *Entry) LookupStatus {
    return switch (volume) {
        .fat32 => |v| blk: {
            var entry: fat32.Entry = undefined;
            const status = lookupStatusFromFat32(fat32.findEntryStatus(v, @intCast(parent), name, &entry));
            if (status == .found) out.* = entryFromFat32(entry);
            break :blk status;
        },
        .ntfs => |v| blk: {
            var entry: ntfs_fs.Entry = undefined;
            const status = lookupStatusFromNtfs(ntfs_fs.lookupEntryStatus(v, parent, name, &entry));
            if (status == .found) out.* = entryFromNtfs(entry);
            break :blk status;
        },
    };
}

pub fn lookupEntry(volume: Volume, parent: NodeRef, name: []const u8) ?Entry {
    var out: Entry = undefined;
    return if (lookupEntryStatus(volume, parent, name, &out) == .found) out else null;
}

/// Statusful lookup used only by journal replay.  NTFS exposes its
/// alias-first transient view here; every normal VFS lookup continues to
/// reject a canonical-name mismatch.
pub fn lookupRecoveryEntryStatus(volume: Volume, parent: NodeRef, name: []const u8, out: *Entry) LookupStatus {
    return switch (volume) {
        .fat32 => lookupEntryStatus(volume, parent, name, out),
        .ntfs => |v| blk: {
            var entry: ntfs_fs.Entry = undefined;
            const status = lookupStatusFromNtfs(ntfs_fs.lookupRecoveryEntryStatus(v, parent, name, &entry));
            if (status == .found) out.* = entryFromNtfs(entry);
            break :blk status;
        },
    };
}

/// Exact backend ownership identity for two names observed under one
/// filesystem-request gate.  Empty FAT files have no cluster-chain identity;
/// treating cluster zero as a globally unique object would authorize a
/// foreign alias, so that ambiguous case deliberately returns false.
pub fn sameFileIdentity(volume: Volume, left: Entry, right: Entry) bool {
    if (left.isDir() or right.isDir()) return false;
    return switch (volume) {
        .fat32 => left.size != 0 and
            left.node != 0 and
            left.node == right.node and
            left.size == right.size,
        .ntfs => left.node == right.node and
            left.node_generation == right.node_generation,
    };
}

/// Recovery-only alias identity. Empty FAT files have no cluster identity, but
/// an ownership alias copies the complete short-entry metadata. SYSUPD holds
/// the namespace gate and uses generation-private stage/backup names, so this
/// full fingerprint can safely recognize that crash-transient alias without
/// weakening ordinary VFS identity checks.
pub fn sameFileRecoveryIdentity(volume: Volume, left: Entry, right: Entry) bool {
    if (sameFileIdentity(volume, left, right)) return true;
    return switch (volume) {
        .fat32 => left.node == 0 and
            right.node == 0 and
            left.node_generation == 0 and
            right.node_generation == 0 and
            left.size == 0 and
            right.size == 0 and
            left.attr == right.attr and
            left.reparse == right.reparse and
            left.created_time == right.created_time and
            left.created_date == right.created_date and
            left.access_date == right.access_date and
            left.modified_time == right.modified_time and
            left.modified_date == right.modified_date,
        .ntfs => false,
    };
}

/// Stable mounted-volume identity used by multi-path atomic operations.
/// Drive letters are aliases and the system drive's `\boot` subtree can route
/// to the unlettered boot partition, so equality must be established from the
/// resolved backend volume rather than from the caller-visible letter.
pub fn sameVolume(left: Volume, right: Volume) bool {
    return switch (left) {
        .fat32 => |left_fat| switch (right) {
            .fat32 => |right_fat| left_fat.device_index == right_fat.device_index and
                left_fat.partition_lba == right_fat.partition_lba,
            .ntfs => false,
        },
        .ntfs => |left_ntfs| switch (right) {
            .fat32 => false,
            .ntfs => |right_ntfs| left_ntfs.state_slot == right_ntfs.state_slot,
        },
    };
}

pub fn readFile(volume: Volume, entry: Entry, out: []u8) ?usize {
    return switch (volume) {
        .fat32 => |v| fat32.readFile(v, entryToFat32(entry), out),
        .ntfs => |v| ntfs_fs.readFile(v, entryToNtfs(entry), out),
    };
}

pub fn readFileRange(volume: Volume, entry: Entry, offset: usize, out: []u8) ?usize {
    return switch (volume) {
        .fat32 => |v| fat32.readFileRange(v, entryToFat32(entry), offset, out),
        .ntfs => |v| ntfs_fs.readFileRange(v, entryToNtfs(entry), offset, out),
    };
}

pub fn writeFileRange(volume: Volume, entry: Entry, offset: usize, data: []const u8) ?usize {
    return switch (volume) {
        .fat32 => |v| fat32.writeFileRange(v, entryToFat32(entry), offset, data),
        .ntfs => |v| ntfs_fs.writeFileRange(v, entryToNtfs(entry), offset, data),
    };
}

pub fn writeFile(volume: Volume, parent: NodeRef, name: []const u8, data: []const u8) bool {
    return switch (volume) {
        .fat32 => |v| fat32.writeFile(v, @intCast(parent), name, data),
        .ntfs => |v| ntfs_fs.writeFile(v, parent, name, data),
    };
}

pub fn appendFile(volume: Volume, parent: NodeRef, name: []const u8, data: []const u8) bool {
    return switch (volume) {
        .fat32 => |v| fat32.appendFile(v, @intCast(parent), name, data),
        .ntfs => |v| appendStatusFromNtfs(ntfs_fs.appendFileAtOffset(v, parent, name, ntfsCurrentSize(v, parent, name), data)) == .ok,
    };
}

pub fn appendFileAtOffsetStatus(volume: Volume, parent: NodeRef, name: []const u8, expected_size: u64, data: []const u8) AppendStatus {
    return switch (volume) {
        .fat32 => |v| appendStatusFromFat32(fat32.appendFileAtOffsetStatus(v, @intCast(parent), name, @intCast(expected_size), data)),
        .ntfs => |v| appendStatusFromNtfs(ntfs_fs.appendFileAtOffset(v, parent, name, expected_size, data)),
    };
}

pub fn appendFileAtOffsetStatusDeferred(volume: Volume, parent: NodeRef, name: []const u8, expected_size: u64, data: []const u8) AppendStatus {
    return switch (volume) {
        .fat32 => |v| appendStatusFromFat32(fat32.appendFileAtOffsetStatusDeferred(v, @intCast(parent), name, @intCast(expected_size), data)),
        .ntfs => |v| appendStatusFromNtfs(ntfs_fs.appendFileAtOffsetDeferred(v, parent, name, expected_size, data)),
    };
}

fn ntfsCurrentSize(v: ntfs_fs.Volume, parent: NodeRef, name: []const u8) u64 {
    // appendFile without an explicit offset appends at the current end.
    return ntfs_fs.childSize(v, parent, name);
}

pub fn copyFile(src_volume: Volume, dst_volume: Volume, src_entry: Entry, dst_parent: NodeRef, dst_name: []const u8) bool {
    return switch (src_volume) {
        .fat32 => |sv| switch (dst_volume) {
            .fat32 => |dv| fat32.copyFile(sv, dv, entryToFat32(src_entry), @intCast(dst_parent), dst_name),
            .ntfs => copyFileGeneric(src_volume, dst_volume, src_entry, dst_parent, dst_name, true),
        },
        .ntfs => copyFileGeneric(src_volume, dst_volume, src_entry, dst_parent, dst_name, true),
    };
}

pub fn copyFileNoReplace(src_volume: Volume, dst_volume: Volume, src_entry: Entry, dst_parent: NodeRef, dst_name: []const u8) bool {
    return switch (src_volume) {
        .fat32 => |sv| switch (dst_volume) {
            .fat32 => |dv| fat32.copyFileNoReplace(sv, dv, entryToFat32(src_entry), @intCast(dst_parent), dst_name),
            .ntfs => copyFileGeneric(src_volume, dst_volume, src_entry, dst_parent, dst_name, false),
        },
        .ntfs => copyFileGeneric(src_volume, dst_volume, src_entry, dst_parent, dst_name, false),
    };
}

/// FS-neutral copy used whenever at least one side is not FAT32: chunked
/// read + append through the generic volume interfaces.  Module-owned
/// buffer (never on a kernel task stack); callers are serialized by the
/// fs-request gate.
var copy_chunk: [32768]u8 = undefined;

fn copyFileGeneric(src_volume: Volume, dst_volume: Volume, src_entry: Entry, dst_parent: NodeRef, dst_name: []const u8, replace: bool) bool {
    if (src_entry.isDir()) return false;
    var existing: Entry = undefined;
    switch (lookupEntryStatus(dst_volume, dst_parent, dst_name, &existing)) {
        .found => {
            if (!replace) return false;
            if (existing.isDir()) return false;
            if (!deleteFile(dst_volume, dst_parent, dst_name)) return false;
        },
        .not_found => {},
        .io => return false,
    }
    if (!writeFile(dst_volume, dst_parent, dst_name, copy_chunk[0..0])) return false;

    var offset: u64 = 0;
    while (offset < src_entry.size) {
        const want: usize = @intCast(@min(src_entry.size - offset, copy_chunk.len));
        const got = readFileRange(src_volume, src_entry, @intCast(offset), copy_chunk[0..want]) orelse return false;
        if (got != want) return false;
        if (appendFileAtOffsetStatus(dst_volume, dst_parent, dst_name, offset, copy_chunk[0..want]) != .ok) return false;
        offset += want;
    }
    return true;
}

pub fn makeDirectory(volume: Volume, parent: NodeRef, name: []const u8) bool {
    return switch (volume) {
        .fat32 => |v| fat32.makeDirectory(v, @intCast(parent), name),
        .ntfs => |v| ntfs_fs.makeDirectory(v, parent, name),
    };
}

pub fn deleteFile(volume: Volume, parent: NodeRef, name: []const u8) bool {
    return switch (volume) {
        .fat32 => |v| fat32.deleteFile(v, @intCast(parent), name),
        .ntfs => |v| ntfs_fs.deleteFile(v, parent, name),
    };
}

/// Deletes a file only if the filesystem-specific identity still matches the
/// entry whose bytes were compared by the caller under the same FS gate.
pub fn deleteFileIfIdentity(
    volume: Volume,
    parent: NodeRef,
    name: []const u8,
    expected: Entry,
) DeleteIfMatchResult {
    return switch (volume) {
        .fat32 => |v| switch (fat32.deleteFileIfIdentity(
            v,
            @intCast(parent),
            name,
            entryToFat32(expected),
        )) {
            .deleted => .deleted,
            .not_found => .not_found,
            .mismatch => .mismatch,
            .io => .io,
        },
        .ntfs => |v| switch (ntfs_fs.deleteFileIfIdentity(
            v,
            parent,
            name,
            entryToNtfs(expected),
        )) {
            .deleted => .deleted,
            .not_found => .not_found,
            .mismatch => .mismatch,
            .io => .io,
        },
    };
}

/// Recovery-only compare-and-delete, reserved for journal/claim replay.
///
/// Alias-first publishing has durable windows in which an index name and the
/// record's canonical `$FILE_NAME` disagree.  `deleteFileIfIdentity` rejects
/// those windows on purpose, which would leave a transient stage entry stuck
/// forever.  This operation resolves the name through the recovery view but
/// still acts only on an exact backend identity match, so it can never
/// remove a merely equal-named or since-replaced foreign entry.
///
/// FAT32 has no canonical-name/index split and therefore cannot produce the
/// half-state at all; it reuses the ordinary identity delete unchanged.
pub fn deleteRecoveryEntryIfIdentity(
    volume: Volume,
    parent: NodeRef,
    name: []const u8,
    expected: Entry,
) RecoveryDeleteResult {
    return switch (volume) {
        .fat32 => |v| switch (fat32.deleteFileIfIdentity(
            v,
            @intCast(parent),
            name,
            entryToFat32(expected),
        )) {
            .deleted => .deleted,
            .not_found => .not_found,
            .mismatch => .mismatch,
            .io => .io,
        },
        .ntfs => |v| switch (ntfs_fs.deleteRecoveryEntryIfIdentity(
            v,
            parent,
            name,
            entryToNtfs(expected),
        )) {
            .deleted => .deleted,
            .unlinked => .unlinked,
            .not_found => .not_found,
            .mismatch => .mismatch,
            .directory => .directory,
            .unsupported => .unsupported,
            .io => .io,
        },
    };
}

/// Backend-exact name comparison (0.60.24).
///
/// Answers "would these two names resolve to the same object on THIS
/// volume?" using the filesystem's own collation: NTFS folds through
/// `$UpCase`, FAT32 folds ASCII case only.  The two genuinely disagree
/// beyond ASCII, which is why a caller must never fold bytes itself and
/// call that an identity proof.  `null` means the question could not be
/// answered (malformed name) and must be treated as a visible failure, never
/// as "different".
pub fn namesEqualCollated(volume: Volume, a: []const u8, b: []const u8) ?bool {
    return switch (volume) {
        .fat32 => |v| fat32.namesEqualCollated(v, a, b),
        .ntfs => |v| ntfs_fs.namesEqualCollated(v, a, b),
    };
}

pub fn removeDirectory(volume: Volume, parent: NodeRef, name: []const u8) bool {
    return switch (volume) {
        .fat32 => |v| fat32.removeDirectory(v, @intCast(parent), name),
        .ntfs => |v| ntfs_fs.removeDirectory(v, parent, name),
    };
}

pub fn renameEntry(volume: Volume, parent: NodeRef, old_name: []const u8, new_name: []const u8) bool {
    return renameEntryStatus(volume, parent, old_name, new_name) == .ok;
}

pub fn renameEntryStatus(volume: Volume, parent: NodeRef, old_name: []const u8, new_name: []const u8) RenameStatus {
    return switch (volume) {
        .fat32 => |v| renameStatusFromFat32(fat32.renameEntryStatus(v, @intCast(parent), old_name, new_name)),
        .ntfs => |v| renameStatusFromNtfs(ntfs_fs.renameEntryStatus(v, parent, old_name, new_name)),
    };
}

pub fn replaceFileAtomic(
    volume: Volume,
    parent: NodeRef,
    target_name: []const u8,
    staged_name: []const u8,
    backup_name: []const u8,
    consume_stage: bool,
) AtomicReplaceResult {
    return switch (volume) {
        .fat32 => |v| replaceResultFromFat32(fat32.replaceFileAtomic(v, @intCast(parent), target_name, staged_name, backup_name, consume_stage)),
        .ntfs => |v| replaceResultFromNtfs(ntfs_fs.replaceFileAtomic(v, parent, target_name, staged_name, backup_name, consume_stage)),
    };
}

/// Publish a previously staged sibling only when the target and backup were
/// proven absent. Backends publish the target before detaching the stage and
/// reconcile ambiguous completion without generic path deletion.
pub fn replaceFileAtomicCreateOnly(
    volume: Volume,
    parent: NodeRef,
    target_name: []const u8,
    staged_name: []const u8,
    backup_name: []const u8,
) AtomicReplaceResult {
    return switch (volume) {
        .fat32 => |v| replaceResultFromFat32(fat32.replaceFileAtomicCreateOnly(
            v,
            @intCast(parent),
            target_name,
            staged_name,
            backup_name,
        )),
        .ntfs => |v| replaceResultFromNtfs(ntfs_fs.replaceFileAtomicCreateOnly(
            v,
            parent,
            target_name,
            staged_name,
            backup_name,
        )),
    };
}

fn replaceResultFromNtfs(result: ntfs_fs.ReplaceResult) AtomicReplaceResult {
    return switch (result) {
        .ok => .ok,
        .invalid => .invalid,
        .not_found => .not_found,
        .alias => .alias,
        .conflict => .conflict,
        .read_only => .read_only,
        .io => .io,
        .not_atomic => .not_atomic,
    };
}

pub fn flushVolume(volume: Volume) bool {
    return switch (volume) {
        .fat32 => |v| fat32.flushVolume(v),
        // Durability point of the deferred NTFS stream path (0.60.14).
        .ntfs => |v| ntfs_fs.flushVolume(v),
    };
}

pub fn readDirectory(volume: Volume, dir: NodeRef, out: []u8) ?usize {
    return switch (volume) {
        .fat32 => |v| fat32.readDirectory(v, @intCast(dir), out),
        .ntfs => |v| ntfs_fs.readDirectory(v, dir, out),
    };
}

pub fn readDirectoryEntry(volume: Volume, dir: NodeRef, index: usize, out: []u8) ?Entry {
    var entry: Entry = undefined;
    return if (readDirectoryEntryStatus(volume, dir, index, out, &entry) == .found)
        entry
    else
        null;
}

pub fn readDirectoryEntryStatus(volume: Volume, dir: NodeRef, index: usize, out: []u8, entry_out: *Entry) LookupStatus {
    return switch (volume) {
        .fat32 => |v| blk: {
            var entry: fat32.Entry = undefined;
            const status = lookupStatusFromFat32(fat32.readDirectoryEntryStatus(v, @intCast(dir), index, out, &entry));
            if (status == .found) entry_out.* = entryFromFat32(entry);
            break :blk status;
        },
        .ntfs => |v| blk: {
            var entry: ntfs_fs.Entry = undefined;
            const status = lookupStatusFromNtfs(ntfs_fs.readDirectoryEntryStatus(v, dir, index, out, &entry));
            if (status == .found) entry_out.* = entryFromNtfs(entry);
            break :blk status;
        },
    };
}

pub fn freeClusterCount(volume: Volume) ?u32 {
    return switch (volume) {
        .fat32 => |v| fat32.freeClusterCount(v),
        .ntfs => |v| ntfs_fs.freeClusterCount(v),
    };
}

pub fn listRoot(volume: Volume) bool {
    return switch (volume) {
        .fat32 => |v| fat32.listRoot(v),
        .ntfs => |v| ntfs_fs.listRoot(v),
    };
}

/// Stage/backup naming contract of the atomic replace path: names must stay
/// short-name-safe on every filesystem backend.
pub fn validateShortName83(name: []const u8) bool {
    return fat32.validateShortName83(name);
}

fn entryFromFat32(entry: fat32.Entry) Entry {
    var out = Entry{
        .name_len = entry.name_len,
        .attr = entry.attr,
        .node = entry.first_cluster,
        .size = entry.size,
        .created_time = entry.created_time,
        .created_date = entry.created_date,
        .access_date = entry.access_date,
        .modified_time = entry.modified_time,
        .modified_date = entry.modified_date,
    };
    out.name = entry.name;
    return out;
}

fn entryToFat32(entry: Entry) fat32.Entry {
    var out = fat32.Entry{
        .name_len = entry.name_len,
        .attr = entry.attr,
        .first_cluster = @intCast(entry.node),
        .size = @intCast(entry.size),
        .created_time = entry.created_time,
        .created_date = entry.created_date,
        .access_date = entry.access_date,
        .modified_time = entry.modified_time,
        .modified_date = entry.modified_date,
    };
    out.name = entry.name;
    return out;
}

fn entryFromNtfs(entry: ntfs_fs.Entry) Entry {
    var out = Entry{
        .name_len = entry.name_len,
        .attr = entry.attr,
        .node = entry.record,
        .node_generation = entry.sequence,
        .reparse = entry.reparse,
        .size = entry.size,
        .created_time = entry.created_time,
        .created_date = entry.created_date,
        .access_date = entry.access_date,
        .modified_time = entry.modified_time,
        .modified_date = entry.modified_date,
    };
    out.name = entry.name;
    return out;
}

fn entryToNtfs(entry: Entry) ntfs_fs.Entry {
    var out = ntfs_fs.Entry{
        .name_len = entry.name_len,
        .attr = entry.attr,
        .record = entry.node,
        .sequence = entry.node_generation,
        .reparse = entry.reparse,
        .size = entry.size,
        .created_time = entry.created_time,
        .created_date = entry.created_date,
        .access_date = entry.access_date,
        .modified_time = entry.modified_time,
        .modified_date = entry.modified_date,
    };
    out.name = entry.name;
    return out;
}

fn appendStatusFromFat32(status: fat32.AppendStatus) AppendStatus {
    return switch (status) {
        .ok => .ok,
        .invalid => .invalid,
        .not_found => .not_found,
        .offset_mismatch => .offset_mismatch,
        .too_large => .too_large,
        .io => .io,
    };
}

fn lookupStatusFromFat32(status: fat32.LookupStatus) LookupStatus {
    return switch (status) {
        .found => .found,
        .not_found => .not_found,
        .io => .io,
    };
}

fn lookupStatusFromNtfs(status: ntfs_fs.LookupStatus) LookupStatus {
    return switch (status) {
        .found => .found,
        .not_found => .not_found,
        .io => .io,
    };
}

fn renameStatusFromFat32(status: fat32.RenameStatus) RenameStatus {
    return switch (status) {
        .ok => .ok,
        .not_found => .not_found,
        .not_atomic => .not_atomic,
        .conflict, .read_only => .conflict,
        .io => .io,
    };
}

fn renameStatusFromNtfs(status: ntfs_fs.RenameStatus) RenameStatus {
    return switch (status) {
        .ok => .ok,
        .not_found => .not_found,
        .not_atomic => .not_atomic,
        .conflict => .conflict,
        .io => .io,
    };
}

fn appendStatusFromNtfs(status: ntfs_fs.WriteStatus) AppendStatus {
    return switch (status) {
        .ok => .ok,
        .invalid, .directory, .not_directory, .not_empty, .read_only_target, .unsupported => .invalid,
        .not_found => .not_found,
        .exists => .invalid,
        .offset_mismatch => .offset_mismatch,
        .no_space, .dir_full, .record_full => .too_large,
        .io, .cleanup_failed => .io,
    };
}

fn replaceResultFromFat32(result: fat32.AtomicReplaceResult) AtomicReplaceResult {
    return switch (result) {
        .ok => .ok,
        .invalid => .invalid,
        .not_found => .not_found,
        .alias => .alias,
        .conflict => .conflict,
        .read_only => .read_only,
        .io => .io,
        .not_atomic => .not_atomic,
    };
}

fn driveIndex(letter: u8) ?usize {
    const upper = upperLetter(letter);
    if (upper < 'A' or upper > 'Z') return null;
    return @as(usize, upper - 'A');
}

fn upperLetter(letter: u8) u8 {
    if (letter >= 'a' and letter <= 'z') return letter - ('a' - 'A');
    return letter;
}
