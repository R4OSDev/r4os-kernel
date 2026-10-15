// Stateless loaded-identity transport. Rust owns resolution and permission policy.
const builtin = @import("builtin");
comptime {
    if (builtin.is_test) {
        _ = @import("rust_owner_test");
    }
}
const tables = @import("partition_table.zig");
const installation = @import("installation.zig");
const guid = tables.guid;
pub const Identity = struct { present: bool = false, generic_media: bool = false, path: []const u8 = "", path_truncated: bool = false, disk_guid: guid.Guid = guid.zero, partition_guid: guid.Guid = guid.zero };
pub const DeviceView = struct { index: usize, usb: bool, local: bool, table: *const tables.Table, installation: ?*const installation.Manifest, installation_conflict: bool = false };
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
    system_guid: guid.Guid = guid.zero,
    data_guid: guid.Guid = guid.zero,
    pub fn permitsInstall(self: Source, index: usize) bool {
        return r4os_rust_boot_source_permit(0, self.confirmed, self.usb, self.device_index, index);
    }
    pub fn permitsRecoveryUpdate(self: Source, index: usize) bool {
        return r4os_rust_boot_source_permit(1, self.confirmed, self.usb, self.device_index, index);
    }
    pub fn exposesUsb(self: Source) bool {
        return r4os_rust_boot_source_permit(2, self.confirmed, self.usb, self.device_index, 0);
    }
};
const Label = extern struct { ptr: [*]const u8, len: usize };
const NativeIdentity = extern struct { present: bool, generic_media: bool, path: Label, path_truncated: bool, disk_guid: guid.Guid, partition_guid: guid.Guid };
const TableView = extern struct { valid: bool, kind: u8, disk_guid: guid.Guid, partitions: [*]const tables.Partition, count: usize };
const NativeDevice = extern struct { index: usize, usb: bool, local: bool, table: TableView, installation: ?*const installation.Manifest, installation_conflict: bool };
const NativeSource = extern struct { confirmed: bool, reason: Label, device_index: usize, usb: bool, slot: u8, installation_id: guid.Guid, disk_guid: guid.Guid, boot_guid: guid.Guid, recovery_guid: guid.Guid, system_guid: guid.Guid, data_guid: guid.Guid };
const Getter = *const fn (*const anyopaque, usize, *NativeDevice) callconv(.c) void;
extern fn r4os_rust_boot_source(*const NativeIdentity, *const anyopaque, usize, Getter, bool, *NativeSource) callconv(.c) void;
extern fn r4os_rust_boot_source_permit(u8, bool, bool, usize, usize) callconv(.c) bool;
const Context = struct {
    devices: []const DeviceView,
    fn get(raw: *const anyopaque, index: usize, out: *NativeDevice) callconv(.c) void {
        const self: *const Context = @ptrCast(@alignCast(raw));
        const value = self.devices[index];
        const table = value.table;
        out.* = .{ .index = value.index, .usb = value.usb, .local = value.local, .table = .{ .valid = table.valid, .kind = @intFromEnum(table.kind), .disk_guid = table.disk_guid, .partitions = &table.partitions, .count = table.count }, .installation = value.installation, .installation_conflict = value.installation_conflict };
    }
};
pub fn resolve(identity: Identity, devices: []const DeviceView) Source {
    return resolveLoaded(identity, devices, false);
}
pub fn resolveNormal(identity: Identity, devices: []const DeviceView) Source {
    return resolveLoaded(identity, devices, true);
}
fn resolveLoaded(identity: Identity, devices: []const DeviceView, normal: bool) Source {
    const input = NativeIdentity{ .present = identity.present, .generic_media = identity.generic_media, .path = .{ .ptr = identity.path.ptr, .len = identity.path.len }, .path_truncated = identity.path_truncated, .disk_guid = identity.disk_guid, .partition_guid = identity.partition_guid };
    const context = Context{ .devices = devices };
    var out: NativeSource = undefined;
    r4os_rust_boot_source(&input, &context, devices.len, Context.get, normal, &out);
    return .{ .confirmed = out.confirmed, .reason = out.reason.ptr[0..out.reason.len], .device_index = out.device_index, .usb = out.usb, .slot = @enumFromInt(out.slot), .installation_id = out.installation_id, .disk_guid = out.disk_guid, .boot_guid = out.boot_guid, .recovery_guid = out.recovery_guid, .system_guid = out.system_guid, .data_guid = out.data_guid };
}
comptime {
    if (@sizeOf(NativeIdentity) != 64 or @sizeOf(NativeDevice) != 72 or @sizeOf(NativeSource) != 136 or @offsetOf(NativeSource, "installation_id") != 34) @compileError("Native loaded identity C layout differs");
}
