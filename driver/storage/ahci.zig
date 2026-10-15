// Temporary C facade for the native boot-only AHCI owner.
const std = @import("std");
const pcie = @import("../../platform/pci_inventory.zig");
const block = @import("../../storage/block.zig");
const MAX_PORTS: usize = 8;
const Label = extern struct {
    data: [*]const u8 = "".ptr,
    len: usize = 0,
    fn text(self: @This()) []const u8 {
        return self.data[0..self.len];
    }
};
pub const PortStatus = struct {
    index: u8 = 0,
    implemented: bool = false,
    configured: bool = false,
    block_device_index: ?usize = null,
    sig: u32 = 0,
    ssts: u32 = 0,
    sctl: u32 = 0,
    serr: u32 = 0,
    tfd: u32 = 0,
    cmd: u32 = 0,
    ci: u32 = 0,
    commands: u64 = 0,
    failures: u64 = 0,
    timeouts: u64 = 0,
    recoveries: u64 = 0,
    last_command: u8 = 0,
    last_lba: u64 = 0,
    last_sectors: u16 = 0,
    last_error: []const u8 = "none",
};

pub const Status = struct {
    present: bool = false,
    mapped: bool = false,
    device: pcie.Device = .{},
    bar5_raw: u32 = 0,
    mmio_phys: u64 = 0,
    mmio_virt: u64 = 0,
    cap: u32 = 0,
    ghc: u32 = 0,
    is: u32 = 0,
    pi: u32 = 0,
    vs: u32 = 0,
    cap2: u32 = 0,
    bohc: u32 = 0,
    ports_advertised: u8 = 0,
    command_slots: u8 = 0,
    supports_64bit: bool = false,
    supports_ncq: bool = false,
    supports_staggered_spinup: bool = false,
    implemented_ports: u8 = 0,
    active_sata_ports: u8 = 0,
    configured_ports: u8 = 0,
    block_devices: u8 = 0,
    hba_reset_ok: bool = false,
    bios_handoff: []const u8 = "not-needed",
    commands: u64 = 0,
    failures: u64 = 0,
    timeouts: u64 = 0,
    recoveries: u64 = 0,
    first_ports: [MAX_PORTS]PortStatus = .{PortStatus{}} ** MAX_PORTS,
    reason: []const u8 = "not initialized",
};

const RawPortStatus = extern struct {
    index: u8 = 0,
    implemented: bool = false,
    configured: bool = false,
    block_device_index: usize = std.math.maxInt(usize),
    sig: u32 = 0,
    ssts: u32 = 0,
    sctl: u32 = 0,
    serr: u32 = 0,
    tfd: u32 = 0,
    cmd: u32 = 0,
    ci: u32 = 0,
    commands: u64 = 0,
    failures: u64 = 0,
    timeouts: u64 = 0,
    recoveries: u64 = 0,
    last_command: u8 = 0,
    last_lba: u64 = 0,
    last_sectors: u16 = 0,
    last_error: Label = .{},
};
const RawStatus = extern struct {
    present: bool = false,
    mapped: bool = false,
    device: pcie.Device = .{},
    bar5_raw: u32 = 0,
    mmio_phys: u64 = 0,
    mmio_virt: u64 = 0,
    cap: u32 = 0,
    ghc: u32 = 0,
    is: u32 = 0,
    pi: u32 = 0,
    vs: u32 = 0,
    cap2: u32 = 0,
    bohc: u32 = 0,
    ports_advertised: u8 = 0,
    command_slots: u8 = 0,
    supports_64bit: bool = false,
    supports_ncq: bool = false,
    supports_staggered_spinup: bool = false,
    implemented_ports: u8 = 0,
    active_sata_ports: u8 = 0,
    configured_ports: u8 = 0,
    block_devices: u8 = 0,
    hba_reset_ok: bool = false,
    bios_handoff: Label = .{},
    commands: u64 = 0,
    failures: u64 = 0,
    timeouts: u64 = 0,
    recoveries: u64 = 0,
    first_ports: [MAX_PORTS]RawPortStatus = .{RawPortStatus{}} ** MAX_PORTS,
    reason: Label = .{},
};
extern fn r4os_rust_ahci_control(operation: u8, index: usize) usize;
extern fn r4os_rust_ahci_status(out: *RawStatus) void;
extern fn r4os_rust_ahci_io(context: ?*anyopaque, operation: u8, lba: u64, sectors: u16, data: ?[*]u8, len: usize) bool;
pub fn probe() bool {
    return r4os_rust_ahci_control(0, 0) != 0;
}
pub fn blockDeviceCount() usize {
    return r4os_rust_ahci_control(1, 0);
}
pub fn setPreloadOwner() void {
    _ = r4os_rust_ahci_control(2, 0);
}
pub fn resetBuiltInOwner() void {
    _ = r4os_rust_ahci_control(3, 0);
}
pub fn hasExternalOwner() bool {
    return r4os_rust_ahci_control(4, 0) != 0;
}
pub fn deviceIndexAt(index: usize) ?usize {
    const value = r4os_rust_ahci_control(5, index);
    return if (value == std.math.maxInt(usize)) null else value;
}
pub fn status() Status {
    var raw: RawStatus = undefined;
    r4os_rust_ahci_status(&raw);
    var out: Status = .{};
    inline for (std.meta.fields(Status)) |field| {
        if (comptime std.mem.eql(u8, field.name, "reason") or std.mem.eql(u8, field.name, "bios_handoff")) {
            @field(out, field.name) = @field(raw, field.name).text();
        } else if (comptime std.mem.eql(u8, field.name, "first_ports")) {
            for (&out.first_ports, raw.first_ports) |*port, item| {
                inline for (std.meta.fields(PortStatus)) |part| {
                    if (comptime std.mem.eql(u8, part.name, "last_error")) {
                        port.last_error = item.last_error.text();
                    } else if (comptime std.mem.eql(u8, part.name, "block_device_index")) {
                        port.block_device_index = if (item.block_device_index == std.math.maxInt(usize)) null else item.block_device_index;
                    } else @field(port, part.name) = @field(item, part.name);
                }
            }
        } else @field(out, field.name) = @field(raw, field.name);
    }
    return out;
}
fn readBlock(ctx: ?*anyopaque, lba: u64, sectors: u16, out: []u8) bool {
    return r4os_rust_ahci_io(ctx, 0, lba, sectors, out.ptr, out.len);
}
fn writeBlock(ctx: ?*anyopaque, lba: u64, sectors: u16, data: []const u8) bool {
    return r4os_rust_ahci_io(ctx, 1, lba, sectors, @constCast(data.ptr), data.len);
}
fn flushBlock(ctx: ?*anyopaque) bool {
    return r4os_rust_ahci_io(ctx, 2, 0, 0, null, 0);
}
export fn r4os_zig_ahci_register(port: u8, source: u8, context: ?*anyopaque, name: Label) usize {
    return block.register(.{
        .name = name.text(),
        .driver = "AHCI",
        .bus = .ahci,
        .controller = "ich9-ahci",
        .port = port,
        .sector_size = 512,
        .sector_count = 0,
        .max_sectors_per_request = 8,
        .queue_depth = 1,
        .timeout_ticks = 0,
        .writable = true,
        .source = @enumFromInt(source),
        .ctx = context,
        .read_fn = readBlock,
        .write_fn = writeBlock,
        .flush_fn = flushBlock,
    }) orelse std.math.maxInt(usize);
}
comptime {
    if (@sizeOf(RawPortStatus) != 120 or @offsetOf(RawPortStatus, "last_error") != 104 or @sizeOf(RawStatus) != 1104 or @offsetOf(RawStatus, "first_ports") != 128) @compileError("Rust AHCI snapshot transport differs");
}
