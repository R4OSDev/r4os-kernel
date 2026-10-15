// Stateless facade for native storage boot ordering, admission and evidence.
const std = @import("std");
const block = @import("../storage/block.zig");
const boot_status = @import("boot_status.zig");
const diag_screen = @import("diag_screen.zig");
const driver_api = @import("driver_api.zig");
const registry = @import("../driver/registry.zig");
const drive = @import("../fs/drive.zig");
const vfs = @import("../fs/vfs.zig");
const page_cache = @import("../fs/page_cache.zig");
const fs_request = @import("../fs/request.zig");
const fatal = @import("fatal.zig");
const memory_boot = @import("memory_boot.zig");
const usb_msc = @import("../driver/usb/msc.zig");
const xhci = @import("../driver/usb/xhci.zig");
const usb_host = @import("../driver/usb/host_controller.zig");
const k = @import("log.zig");
const Label = extern struct {
    data: [*]const u8,
    len: usize,
    fn text(self: @This()) []const u8 {
        return self.data[0..self.len];
    }
};
const Counts = extern struct { xhci: u32, ahci: u32, nvme: u32 };
const RegistryEntry = extern struct { source: u8, state: u8 };
const BlockBasic = extern struct { bus: u8, source: u8 };
const DeviceDiagnostics = extern struct {
    name: Label,
    bus: Label,
    source: Label,
    state: Label,
    last_error: Label,
    sector_size: u32,
    sector_count: u64,
    read_failures: u64,
    read_ops: u64,
    read_sectors: u64,
    last_lba: u64,
    completion_ticks: u64,
    last_sectors: u16,
};
extern fn r4os_rust_storage_boot(operation: u8, counts: ?*const Counts) usize;
pub fn initFoundation() bool {
    return r4os_rust_storage_boot(0, null) != 0;
}
pub fn isFoundationInitialized() bool {
    return r4os_rust_storage_boot(1, null) != 0;
}
pub fn initControllers(status: anytype) bool {
    const counts: Counts = .{ .xhci = status.xhci_count, .ahci = status.ahci_count, .nvme = status.nvme_count };
    return r4os_rust_storage_boot(2, &counts) != 0;
}
pub fn isControllersInitialized() bool {
    return r4os_rust_storage_boot(3, null) != 0;
}
pub fn mbrScans() usize {
    return r4os_rust_storage_boot(4, null);
}
pub fn mbrSuccesses() usize {
    return r4os_rust_storage_boot(5, null);
}
pub fn renderMountDiagnostics() void {
    _ = r4os_rust_storage_boot(6, null);
}
fn label(text: []const u8) Label {
    return .{ .data = text.ptr, .len = text.len };
}
export fn r4os_zig_storage_boot_foundation(operation: u8) bool {
    switch (operation) {
        0 => return memory_boot.isCoreInitialized(),
        1 => drive.init(),
        2 => vfs.init(),
        3 => block.init(),
        4 => page_cache.init(),
        5 => fs_request.init(),
        else => unreachable,
    }
    return true;
}
export fn r4os_zig_storage_boot_registry(operation: u8, index: usize, name: Label, kind: u16, version: u32, out: ?*RegistryEntry) usize {
    switch (operation) {
        0 => return registry.findByName(name.text()) orelse std.math.maxInt(usize),
        1 => {
            const entry = registry.get(index) orelse return 0;
            out.?.* = .{ .source = @intFromEnum(entry.source), .state = @intFromEnum(entry.state) };
            return 1;
        },
        2 => return registry.beginLoad(name.text(), kind, version) orelse std.math.maxInt(usize),
        3 => {
            registry.setState(index, @enumFromInt(version));
            return 0;
        },
        else => unreachable,
    }
}
export fn r4os_zig_storage_boot_usb(operation: u8, index: usize) usize {
    switch (operation) {
        0 => return usb_host.findByName("XHCI") orelse std.math.maxInt(usize),
        1 => {
            const host = usb_host.at(index) orelse return std.math.maxInt(usize);
            return @intFromEnum(host.source);
        },
        2 => return @intFromBool(xhci.probe()),
        3 => usb_msc.setPreloadOwner(),
        4 => usb_msc.resetBuiltInOwner(),
        5 => return @intFromBool(usb_msc.init()),
        6 => return usb_msc.blockDeviceCount(),
        7 => return usb_msc.deviceIndex() orelse std.math.maxInt(usize),
        8 => return @intFromBool(usb_msc.reselectActiveDevice()),
        else => unreachable,
    }
    return 0;
}
export fn r4os_zig_storage_boot_block(index: usize, out: *BlockBasic) bool {
    const device = block.get(index) orelse return false;
    out.* = .{ .bus = @intFromEnum(device.bus), .source = @intFromEnum(device.source) };
    return true;
}
export fn r4os_zig_storage_boot_device_diagnostics(index: usize, out: *DeviceDiagnostics) bool {
    const device = block.get(index) orelse return false;
    const stats = block.statsSnapshot(device);
    out.* = .{ .name = label(device.name), .bus = label(@tagName(device.bus)), .source = label(@tagName(device.source)), .state = label(@tagName(device.state)), .last_error = label(@tagName(stats.last_error)), .sector_size = device.sector_size, .sector_count = device.sector_count, .read_failures = stats.read_failures, .read_ops = stats.read_ops, .read_sectors = stats.read_sectors, .last_lba = stats.last_request.lba, .completion_ticks = stats.completion_last_ticks, .last_sectors = stats.last_request.sectors };
    return true;
}
export fn r4os_zig_storage_boot_backend(index: usize, out: *driver_api.StorageBackendStatus) bool {
    out.* = .{ .state = 0, .last_error = 0, .last_lba = 0, .last_sectors = 0, .recoveries = 0, .recovery_failures = 0 };
    return driver_api.queryStorageBackendStatus(index, out);
}
export fn r4os_zig_storage_boot_fail(message: Label) bool {
    return fatal.fail(.storage, message.text());
}
export fn r4os_zig_storage_boot_output(operation: u8, text: Label, value: u64) void {
    switch (operation) {
        0 => k.puts(text.text()),
        1 => k.putDec(value),
        2 => boot_status.statusLine(text.text()),
        3 => diag_screen.beginIncident(),
        4 => diag_screen.write(text.text()),
        5 => diag_screen.writeDec(value),
        6 => diag_screen.writeHex(value),
        7 => diag_screen.line(text.text()),
        8 => diag_screen.endLine(),
        else => unreachable,
    }
}
export fn r4os_zig_storage_boot_data_role() usize {
    const data = drive.get('D') orelse return std.math.maxInt(usize);
    return @intFromEnum(data.role);
}
const DataContext = struct { volume: vfs.Volume, root: vfs.NodeRef };
export fn r4os_zig_storage_boot_data_root(consume: *const fn (*anyopaque) callconv(.c) void) void {
    const volume = vfs.volumeForDrive('D') orelse return;
    const root = vfs.resolvePath(volume, "\\") orelse return;
    var context = DataContext{ .volume = volume, .root = root };
    consume(&context);
}
export fn r4os_zig_storage_boot_data_path(token: *anyopaque, operation: u8, text: Label) bool {
    const context: *const DataContext = @ptrCast(@alignCast(token));
    return switch (operation) {
        0 => vfs.resolvePath(context.volume, text.text()) != null,
        1 => vfs.makeDirectory(context.volume, context.root, text.text()),
        else => unreachable,
    };
}
comptime {
    // Retain only the temporary FS/drive C exports of the already native
    // owners; all storage boot policy is native and has a single state table.
    _ = @import("../storage/mbr.zig");
    _ = @import("../storage/normal_boot.zig");
    _ = @import("../driver/storage/ahci.zig");
    if (@sizeOf(Counts) != 12 or @sizeOf(RegistryEntry) != 2 or @sizeOf(BlockBasic) != 2 or @sizeOf(driver_api.StorageBackendStatus) != 40 or @sizeOf(DeviceDiagnostics) != 144) @compileError("Rust storage boot transport differs");
    if (@intFromEnum(block.Bus.usb) != 4 or @intFromEnum(block.Bus.nvme) != 3 or @intFromEnum(block.Source.preload) != 1 or @intFromEnum(registry.Source.preload) != 2 or @intFromEnum(registry.State.active) != 3 or @intFromEnum(drive.Role.data) != 2) @compileError("Rust storage boot vocabulary differs");
}
