const std = @import("std");
const tables = @import("partition_table.zig");
const installation = @import("installation.zig");
const guid = tables.guid;

pub const Identity = struct {
    present: bool = false,
    generic_media: bool = false,
    path: []const u8 = "",
    path_truncated: bool = false,
    disk_guid: guid.Guid = guid.zero,
    partition_guid: guid.Guid = guid.zero,
};
pub const DeviceView = struct {
    index: usize,
    usb: bool,
    local: bool,
    table: *const tables.Table,
    installation: ?*const installation.Manifest,
    installation_conflict: bool = false,
};
pub const Source = struct {
    confirmed: bool = false,
    reason: []const u8 = "missing-identity",
    device_index: usize = 0,
    usb: bool = false,
    slot: enum { unknown, current, previous } = .unknown,
    installation_id: guid.Guid = guid.zero,
    disk_guid: guid.Guid = guid.zero,
    boot_guid: guid.Guid = guid.zero,
    recovery_guid: guid.Guid = guid.zero,

    pub fn permitsInstall(self: Source, device_index: usize) bool {
        return self.confirmed and !(self.usb and self.device_index == device_index);
    }
    pub fn permitsRecoveryUpdate(self: Source, _: usize) bool {
        return self.confirmed;
    }
    pub fn exposesUsb(self: Source) bool {
        return self.confirmed and self.usb;
    }
};

pub fn resolve(identity: Identity, devices: []const DeviceView) Source {
    var result = Source{};
    if (!identity.present or identity.path_truncated or !identity.generic_media or guid.isZero(identity.disk_guid) or guid.isZero(identity.partition_guid)) return result;
    result.reason = "unknown-loaded-path";
    if (std.ascii.eqlIgnoreCase(identity.path, "/CURRENT/recovery.elf")) result.slot = .current else if (std.ascii.eqlIgnoreCase(identity.path, "/PREVIOUS/recovery.elf")) result.slot = .previous else return result;
    result.reason = "source-not-found";
    var selected: ?DeviceView = null;
    var disk_matches: usize = 0;
    for (devices) |device| {
        if (!guid.eql(device.table.disk_guid, identity.disk_guid)) continue;
        disk_matches += 1;
        if (device.table.valid and (device.usb or device.local)) selected = device;
    }
    if (disk_matches > 1) {
        result.reason = "duplicate-disk-guid";
        return result;
    }
    const device = selected orelse return result;
    const manifest = device.installation orelse {
        result.reason = "installation-unavailable";
        return result;
    };
    if (device.installation_conflict) {
        result.reason = "installation-conflict";
        return result;
    }
    if (!guid.eql(manifest.part(.RECOVERY).partition_guid, identity.partition_guid)) {
        result.reason = "loaded-partition-mismatch";
        return result;
    }
    for (devices) |other| {
        if (other.index == device.index) continue;
        if (other.installation) |m| if (guid.eql(m.installation_id, manifest.installation_id)) {
            result.reason = "duplicate-installation-id";
            return result;
        };
        for (other.table.items()) |p| if (guid.eql(p.unique_guid, identity.partition_guid)) {
            result.reason = "duplicate-source-partition-guid";
            return result;
        };
    }
    result.confirmed = true;
    result.reason = "ok";
    result.device_index = device.index;
    result.usb = device.usb;
    result.installation_id = manifest.installation_id;
    result.disk_guid = manifest.disk_guid;
    result.boot_guid = manifest.part(.BOOT).partition_guid;
    result.recovery_guid = manifest.part(.RECOVERY).partition_guid;
    return result;
}
