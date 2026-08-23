const bootlog = @import("../../kernel/bootlog.zig");
const paging = @import("../../memory/paging.zig");
const phys = @import("../../memory/phys.zig");
const pcie = @import("../../platform/pci_inventory.zig");
const block = @import("../../storage/block.zig");

const REG_CAP: u64 = 0x00;
const REG_GHC: u64 = 0x04;
const REG_IS: u64 = 0x08;
const REG_PI: u64 = 0x0C;
const REG_VS: u64 = 0x10;
const REG_CAP2: u64 = 0x24;
const REG_BOHC: u64 = 0x28;

const PORT_BASE: u64 = 0x100;
const PORT_STRIDE: u64 = 0x80;
const PORT_CLB: u64 = 0x00;
const PORT_CLBU: u64 = 0x04;
const PORT_FB: u64 = 0x08;
const PORT_FBU: u64 = 0x0C;
const PORT_IS: u64 = 0x10;
const PORT_IE: u64 = 0x14;
const PORT_CMD: u64 = 0x18;
const PORT_TFD: u64 = 0x20;
const PORT_SIG: u64 = 0x24;
const PORT_SSTS: u64 = 0x28;
const PORT_SCTL: u64 = 0x2C;
const PORT_SERR: u64 = 0x30;
const PORT_CI: u64 = 0x38;

const MAP_BYTES: u64 = 0x2000;
const MAX_PORTS: usize = 8;
const SECTOR_SIZE: usize = 512;
const MAX_SECTORS_PER_REQUEST: u16 = 8;
const COMMAND_LIST_BYTES: usize = 1024;
const FIS_BYTES: usize = 256;
const COMMAND_TABLE_BYTES: usize = 256;
const DMA_BYTES: usize = SECTOR_SIZE * MAX_SECTORS_PER_REQUEST;
const HBA_GHC_HR: u32 = 1 << 0;
const HBA_GHC_AE: u32 = 1 << 31;
const BOHC_BOS: u32 = 1 << 0;
const BOHC_OOS: u32 = 1 << 1;
const PORT_CMD_ST: u32 = 1 << 0;
const PORT_CMD_SUD: u32 = 1 << 1;
const PORT_CMD_POD: u32 = 1 << 2;
const PORT_CMD_FRE: u32 = 1 << 4;
const PORT_CMD_FR: u32 = 1 << 14;
const PORT_CMD_CR: u32 = 1 << 15;
const PORT_TFD_ERR: u32 = 1 << 0;
const PORT_TFD_DRQ: u32 = 1 << 3;
const PORT_TFD_BSY: u32 = 1 << 7;
const PORT_IS_TFES: u32 = 1 << 30;
const FIS_TYPE_REG_H2D: u8 = 0x27;
const ATA_CMD_READ_DMA_EXT: u8 = 0x25;
const ATA_CMD_WRITE_DMA_EXT: u8 = 0x35;
const ATA_CMD_FLUSH_CACHE_EXT: u8 = 0xEA;
const ATA_DEVICE_LBA: u8 = 1 << 6;
const SIG_SATA: u32 = 0x0000_0101;
const SIG_SATAPI: u32 = 0xEB14_0101;
const SIG_SEMB: u32 = 0xC33C_0101;
const SIG_PORT_MULTIPLIER: u32 = 0x9669_0101;
const WAIT_GUARD: u32 = 1_000_000;

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

const CommandHeader = extern struct {
    flags: u16,
    prdtl: u16,
    prdbc: u32,
    ctba: u32,
    ctbau: u32,
    reserved: [4]u32,
};

const PrdtEntry = extern struct {
    dba: u32,
    dbau: u32,
    reserved: u32,
    dbc_i: u32,
};

const CommandDirection = enum {
    read,
    write,
    non_data,
};

const CommandRequest = struct {
    command: u8,
    lba: u64 = 0,
    sectors: u16 = 0,
    direction: CommandDirection = .non_data,
    data_bytes: usize = 0,
    write_data: ?[]const u8 = null,

    fn isWrite(self: CommandRequest) bool {
        return self.direction == .write;
    }

    fn hasData(self: CommandRequest) bool {
        return self.data_bytes > 0;
    }
};

const PortRuntime = struct {
    port: u8 = 0,
    implemented: bool = false,
    configured: bool = false,
    device_present: bool = false,
    command_list_phys: u64 = 0,
    fis_phys: u64 = 0,
    command_table_phys: u64 = 0,
    dma_phys: u64 = 0,
    command_list_virt: u64 = 0,
    fis_virt: u64 = 0,
    command_table_virt: u64 = 0,
    dma_virt: u64 = 0,
    block_device_index: ?usize = null,
    commands: u64 = 0,
    failures: u64 = 0,
    timeouts: u64 = 0,
    recoveries: u64 = 0,
    last_command: u8 = 0,
    last_lba: u64 = 0,
    last_sectors: u16 = 0,
    last_error: []const u8 = "none",
};

var current: Status = .{};
var runtimes: [MAX_PORTS]PortRuntime = .{PortRuntime{}} ** MAX_PORTS;
var registered_indices: [MAX_PORTS]?usize = .{null} ** MAX_PORTS;
var block_source: block.Source = .builtin;
var external_owner: bool = false;

pub fn probe() bool {
    current = .{};
    runtimes = .{PortRuntime{}} ** MAX_PORTS;
    registered_indices = .{null} ** MAX_PORTS;
    const ps = pcie.status();
    if (ps.ahci_count == 0) {
        current.reason = "no AHCI controller found";
        bootlog.puts("[AHCI] not found\r\n");
        return false;
    }

    current.present = true;
    current.device = ps.first_ahci;
    current.bar5_raw = pcie.readBar(current.device, 5);
    const bar = current.bar5_raw;
    if ((bar & 0x1) != 0) {
        current.reason = "BAR5 is I/O space, expected MMIO";
        bootlog.puts("[AHCI][WARN] BAR5 is I/O space\r\n");
        return false;
    }
    current.mmio_phys = bar & 0xFFFF_FFF0;
    if (current.mmio_phys == 0) {
        current.reason = "BAR5 MMIO base is zero";
        bootlog.puts("[AHCI][WARN] BAR5 MMIO base is zero\r\n");
        return false;
    }
    if (!mapMmio(current.mmio_phys, MAP_BYTES)) {
        current.reason = "failed to map AHCI MMIO";
        bootlog.puts("[AHCI][WARN] MMIO map failed\r\n");
        return false;
    }
    current.mapped = true;
    current.mmio_virt = phys.physToVirt(current.mmio_phys);
    enableBusMaster();
    handleBiosHandoff();
    current.hba_reset_ok = resetController();
    readController();
    readPorts();
    configurePorts();
    current.reason = if (current.block_devices > 0)
        "AHCI block devices active"
    else
        "diagnostic MMIO mapped, no active SATA block device";
    logSummary();
    return true;
}

pub fn status() Status {
    return current;
}

pub fn blockDeviceCount() usize {
    return current.block_devices;
}

pub fn setPreloadOwner() void {
    block_source = .preload;
    external_owner = true;
}

pub fn resetBuiltInOwner() void {
    block_source = .builtin;
    external_owner = false;
}

pub fn hasExternalOwner() bool {
    return external_owner;
}

pub fn deviceIndexAt(index: usize) ?usize {
    if (index >= registered_indices.len) return null;
    return registered_indices[index];
}

fn readController() void {
    current.cap = read32(REG_CAP);
    current.ghc = read32(REG_GHC);
    current.is = read32(REG_IS);
    current.pi = read32(REG_PI);
    current.vs = read32(REG_VS);
    current.cap2 = read32(REG_CAP2);
    current.bohc = read32(REG_BOHC);
    current.ports_advertised = @truncate((current.cap & 0x1F) + 1);
    current.command_slots = @truncate(((current.cap >> 8) & 0x1F) + 1);
    current.supports_staggered_spinup = (current.cap & (1 << 27)) != 0;
    current.supports_ncq = (current.cap & (1 << 30)) != 0;
    current.supports_64bit = (current.cap & (1 << 31)) != 0;
}

fn readPorts() void {
    current.implemented_ports = 0;
    current.active_sata_ports = 0;
    var port: u8 = 0;
    while (port < 32) : (port += 1) {
        if ((current.pi & (@as(u32, 1) << @as(u5, @truncate(port)))) == 0) continue;
        current.implemented_ports += 1;
        const ps = readPort(port);
        if (port < current.first_ports.len) current.first_ports[port] = ps;
        if (isActiveSata(ps)) current.active_sata_ports += 1;
    }
}

fn readPort(port: u8) PortStatus {
    const base = PORT_BASE + @as(u64, port) * PORT_STRIDE;
    return .{
        .index = port,
        .implemented = true,
        .sig = read32(base + PORT_SIG),
        .ssts = read32(base + PORT_SSTS),
        .sctl = read32(base + PORT_SCTL),
        .serr = read32(base + PORT_SERR),
        .tfd = read32(base + PORT_TFD),
        .cmd = read32(base + PORT_CMD),
        .ci = read32(base + PORT_CI),
    };
}

fn configurePorts() void {
    current.configured_ports = 0;
    current.block_devices = 0;
    current.active_sata_ports = 0;
    var port: u8 = 0;
    while (port < @as(u8, @intCast(MAX_PORTS))) : (port += 1) {
        if ((current.pi & (@as(u32, 1) << @as(u5, @truncate(port)))) == 0) continue;
        const rt = &runtimes[port];
        rt.* = .{ .port = port, .implemented = true };
        if (!preparePort(rt)) {
            var ps = readPort(port);
            ps.last_error = rt.last_error;
            current.first_ports[port] = ps;
            continue;
        }
        current.configured_ports += 1;

        const ps = readPort(port);
        if (portSkipReason(ps)) |reason| {
            rt.last_error = reason;
            updatePortSnapshot(rt);
            continue;
        }
        rt.device_present = true;
        current.active_sata_ports += 1;

        const block_index = block.register(.{
            .name = nameForPort(port),
            .driver = "AHCI",
            .bus = .ahci,
            .controller = "ich9-ahci",
            .port = port,
            .sector_size = SECTOR_SIZE,
            .sector_count = 0,
            .max_sectors_per_request = MAX_SECTORS_PER_REQUEST,
            .queue_depth = 1,
            .timeout_ticks = 0,
            .writable = true,
            .source = block_source,
            .ctx = rt,
            .read_fn = readBlock,
            .write_fn = writeBlock,
            .flush_fn = flushBlock,
        }) orelse {
            rt.last_error = "block-register-failed";
            updatePortSnapshot(rt);
            continue;
        };
        rt.block_device_index = block_index;
        registered_indices[port] = block_index;
        current.block_devices += 1;

        updatePortSnapshot(rt);

        bootlog.puts("[AHCI] port ");
        bootlog.putDec(port);
        bootlog.puts(" block=#");
        bootlog.putDec(block_index);
        bootlog.puts(" dma-ready\r\n");
    }
}

fn preparePort(rt: *PortRuntime) bool {
    if (!allocateRuntime(rt)) return false;
    const base = portBase(rt.port);
    if (!stopPort(base, rt)) return false;
    write32(base + PORT_CLB, @truncate(rt.command_list_phys));
    write32(base + PORT_CLBU, @truncate(rt.command_list_phys >> 32));
    write32(base + PORT_FB, @truncate(rt.fis_phys));
    write32(base + PORT_FBU, @truncate(rt.fis_phys >> 32));
    write32(base + PORT_SERR, 0xFFFF_FFFF);
    write32(base + PORT_IS, 0xFFFF_FFFF);
    write32(base + PORT_IE, 0);
    if (!startPort(base, rt)) return false;
    rt.configured = true;
    rt.last_error = "none";
    return true;
}

fn allocateRuntime(rt: *PortRuntime) bool {
    rt.command_list_phys = allocFrameZero() orelse {
        rt.last_error = "alloc-command-list";
        return false;
    };
    rt.fis_phys = allocFrameZero() orelse {
        rt.last_error = "alloc-fis";
        return false;
    };
    rt.command_table_phys = allocFrameZero() orelse {
        rt.last_error = "alloc-command-table";
        return false;
    };
    rt.dma_phys = allocFrameZero() orelse {
        rt.last_error = "alloc-dma";
        return false;
    };
    rt.command_list_virt = phys.physToVirt(rt.command_list_phys);
    rt.fis_virt = phys.physToVirt(rt.fis_phys);
    rt.command_table_virt = phys.physToVirt(rt.command_table_phys);
    rt.dma_virt = phys.physToVirt(rt.dma_phys);
    return true;
}

fn allocFrameZero() ?u64 {
    const frame = phys.allocFrame() orelse return null;
    const bytes: [*]u8 = @ptrFromInt(phys.physToVirt(frame));
    @memset(bytes[0..@intCast(phys.FRAME_SIZE)], 0);
    return frame;
}

fn readBlock(ctx: ?*anyopaque, lba: u64, sectors: u16, out: []u8) bool {
    const rt = runtimeFromContext(ctx) orelse return false;
    if (!issueAtaDma(rt, ATA_CMD_READ_DMA_EXT, lba, sectors, false, out)) return false;
    return true;
}

fn writeBlock(ctx: ?*anyopaque, lba: u64, sectors: u16, data: []const u8) bool {
    const rt = runtimeFromContext(ctx) orelse return false;
    return issueAtaDmaConst(rt, ATA_CMD_WRITE_DMA_EXT, lba, sectors, true, data);
}

fn flushBlock(ctx: ?*anyopaque) bool {
    const rt = runtimeFromContext(ctx) orelse return false;
    return issueFlush(rt);
}

fn issueAtaDma(rt: *PortRuntime, command: u8, lba: u64, sectors: u16, write: bool, buffer: []u8) bool {
    if (sectors == 0 or sectors > MAX_SECTORS_PER_REQUEST) {
        failCommand(rt, null, "bad-sector-count", false);
        return false;
    }
    const bytes = @as(usize, sectors) * SECTOR_SIZE;
    if (buffer.len < bytes) {
        failCommand(rt, null, "buffer-too-small", false);
        return false;
    }
    const request = CommandRequest{
        .command = command,
        .lba = lba,
        .sectors = sectors,
        .direction = if (write) .write else .read,
        .data_bytes = bytes,
        .write_data = if (write) buffer[0..bytes] else null,
    };
    const ok = issueCommand(rt, request);
    if (!ok) return false;
    if (!write) {
        const dma: [*]u8 = @ptrFromInt(rt.dma_virt);
        @memcpy(buffer[0..bytes], dma[0..bytes]);
    }
    return true;
}

fn issueAtaDmaConst(rt: *PortRuntime, command: u8, lba: u64, sectors: u16, write: bool, data: []const u8) bool {
    if (!write) return false;
    if (sectors == 0 or sectors > MAX_SECTORS_PER_REQUEST) {
        failCommand(rt, null, "bad-sector-count", false);
        return false;
    }
    const bytes = @as(usize, sectors) * SECTOR_SIZE;
    if (data.len < bytes) {
        failCommand(rt, null, "buffer-too-small", false);
        return false;
    }
    const dma: [*]u8 = @ptrFromInt(rt.dma_virt);
    @memcpy(dma[0..bytes], data[0..bytes]);
    return issueCommand(rt, .{
        .command = command,
        .lba = lba,
        .sectors = sectors,
        .direction = .write,
        .data_bytes = bytes,
    });
}

fn issueFlush(rt: *PortRuntime) bool {
    return issueCommand(rt, .{ .command = ATA_CMD_FLUSH_CACHE_EXT });
}

fn issueCommand(rt: *PortRuntime, request: CommandRequest) bool {
    if (!rt.configured) {
        failCommand(rt, null, "port-not-configured", false);
        return false;
    }
    if (request.write_data) |data| {
        const dma: [*]u8 = @ptrFromInt(rt.dma_virt);
        @memcpy(dma[0..data.len], data);
    }

    const base = portBase(rt.port);
    rt.commands += 1;
    current.commands += 1;
    rt.last_command = request.command;
    rt.last_lba = request.lba;
    rt.last_sectors = request.sectors;
    updatePortSnapshot(rt);
    if (!waitTfdReady(base)) {
        failCommand(rt, base, "tfd-busy", true);
        return false;
    }

    prepareCommandSlot(rt, request);
    submitCommandSlot(base);
    return pollCommandSlot(rt, base);
}

fn prepareCommandSlot(rt: *PortRuntime, request: CommandRequest) void {
    const header = commandHeader(rt);
    header.* = .{
        .flags = @as(u16, 5) | if (request.isWrite()) @as(u16, 1 << 6) else 0,
        .prdtl = if (request.hasData()) 1 else 0,
        .prdbc = 0,
        .ctba = @truncate(rt.command_table_phys),
        .ctbau = @truncate(rt.command_table_phys >> 32),
        .reserved = .{0} ** 4,
    };
    const table = tableBytes(rt);
    @memset(table[0..COMMAND_TABLE_BYTES], 0);
    fillFis(table[0..64], request.command, request.lba, request.sectors);
    if (request.hasData()) {
        const prdt = prdtEntry(rt);
        prdt.* = .{
            .dba = @truncate(rt.dma_phys),
            .dbau = @truncate(rt.dma_phys >> 32),
            .reserved = 0,
            .dbc_i = @as(u32, @intCast(request.data_bytes - 1)) | (1 << 31),
        };
    }
}

fn submitCommandSlot(base: u64) void {
    write32(base + PORT_SERR, 0xFFFF_FFFF);
    write32(base + PORT_IS, 0xFFFF_FFFF);
    write32(base + PORT_CI, 1);
}

fn pollCommandSlot(rt: *PortRuntime, base: u64) bool {
    var guard: u32 = 0;
    while ((read32(base + PORT_CI) & 1) != 0 and guard < WAIT_GUARD) : (guard += 1) {
        if ((read32(base + PORT_IS) & PORT_IS_TFES) != 0) {
            failCommand(rt, base, "task-file-error", true);
            return false;
        }
    }
    if ((read32(base + PORT_CI) & 1) != 0) {
        failCommand(rt, base, "command-timeout", true);
        return false;
    }
    const is = read32(base + PORT_IS);
    const tfd = read32(base + PORT_TFD);
    if ((is & PORT_IS_TFES) != 0 or (tfd & (PORT_TFD_ERR | PORT_TFD_BSY | PORT_TFD_DRQ)) != 0) {
        failCommand(rt, base, "command-error", true);
        return false;
    }
    rt.last_error = "none";
    updatePortSnapshot(rt);
    return true;
}

fn failCommand(rt: *PortRuntime, maybe_base: ?u64, reason: []const u8, recover: bool) void {
    rt.failures += 1;
    current.failures += 1;
    if (stringEquals(reason, "command-timeout") or stringEquals(reason, "tfd-busy")) {
        rt.timeouts += 1;
        current.timeouts += 1;
    }
    rt.last_error = reason;
    if (recover) {
        if (maybe_base) |base| {
            _ = recoverPort(rt, base);
            rt.last_error = reason;
        }
    }
    updatePortSnapshot(rt);
}

fn recoverPort(rt: *PortRuntime, base: u64) bool {
    rt.recoveries += 1;
    current.recoveries += 1;
    _ = stopPort(base, rt);
    write32(base + PORT_SERR, 0xFFFF_FFFF);
    write32(base + PORT_IS, 0xFFFF_FFFF);
    return startPort(base, rt);
}

fn updatePortSnapshot(rt: *PortRuntime) void {
    if (rt.port >= current.first_ports.len) return;
    var ps = readPort(rt.port);
    ps.configured = rt.configured;
    ps.block_device_index = rt.block_device_index;
    ps.commands = rt.commands;
    ps.failures = rt.failures;
    ps.timeouts = rt.timeouts;
    ps.recoveries = rt.recoveries;
    ps.last_command = rt.last_command;
    ps.last_lba = rt.last_lba;
    ps.last_sectors = rt.last_sectors;
    ps.last_error = rt.last_error;
    current.first_ports[rt.port] = ps;
}

fn firstActiveRuntime() ?*PortRuntime {
    var i: usize = 0;
    while (i < runtimes.len) : (i += 1) {
        if (runtimes[i].configured and runtimes[i].block_device_index != null) return &runtimes[i];
    }
    return null;
}

fn fillFis(fis: []u8, command: u8, lba: u64, sectors: u16) void {
    fis[0] = FIS_TYPE_REG_H2D;
    fis[1] = 1 << 7;
    fis[2] = command;
    fis[3] = 0;
    fis[4] = @truncate(lba);
    fis[5] = @truncate(lba >> 8);
    fis[6] = @truncate(lba >> 16);
    fis[7] = ATA_DEVICE_LBA;
    fis[8] = @truncate(lba >> 24);
    fis[9] = @truncate(lba >> 32);
    fis[10] = @truncate(lba >> 40);
    fis[11] = 0;
    fis[12] = @truncate(sectors);
    fis[13] = @truncate(sectors >> 8);
    fis[14] = 0;
    fis[15] = 0;
}

fn commandHeader(rt: *PortRuntime) *CommandHeader {
    const headers: [*]CommandHeader = @ptrFromInt(rt.command_list_virt);
    return &headers[0];
}

fn tableBytes(rt: *PortRuntime) []u8 {
    const bytes: [*]u8 = @ptrFromInt(rt.command_table_virt);
    return bytes[0..@intCast(phys.FRAME_SIZE)];
}

fn prdtEntry(rt: *PortRuntime) *PrdtEntry {
    return @ptrFromInt(rt.command_table_virt + 128);
}

fn runtimeFromContext(ctx: ?*anyopaque) ?*PortRuntime {
    const ptr = ctx orelse return null;
    return @ptrCast(@alignCast(ptr));
}

fn isActiveSata(ps: PortStatus) bool {
    const det = ps.ssts & 0x0F;
    const ipm = (ps.ssts >> 8) & 0x0F;
    return det == 3 and (ipm == 1 or ipm == 2) and ps.sig == SIG_SATA;
}

fn portSkipReason(ps: PortStatus) ?[]const u8 {
    const det = ps.ssts & 0x0F;
    const ipm = (ps.ssts >> 8) & 0x0F;
    if (det == 0) return "no-device";
    if (det != 3) return "link-not-ready";
    if (ipm != 1 and ipm != 2) return "link-power-inactive";
    return switch (ps.sig) {
        SIG_SATA => null,
        SIG_SATAPI => "unsupported-atapi",
        SIG_SEMB => "unsupported-enclosure",
        SIG_PORT_MULTIPLIER => "unsupported-port-multiplier",
        else => "unsupported-device-type",
    };
}

fn enableBusMaster() void {
    var command = pcie.readCommand(current.device);
    command |= 0x0006;
    _ = pcie.writeCommand(current.device, command);
}

fn handleBiosHandoff() void {
    current.bohc = read32(REG_BOHC);
    if ((current.bohc & BOHC_BOS) == 0) {
        current.bios_handoff = "not-needed";
        return;
    }
    write32(REG_BOHC, current.bohc | BOHC_OOS);
    var guard: u32 = 0;
    while ((read32(REG_BOHC) & BOHC_BOS) != 0 and guard < WAIT_GUARD) : (guard += 1) {}
    current.bohc = read32(REG_BOHC);
    current.bios_handoff = if ((current.bohc & BOHC_BOS) == 0) "ok" else "timeout";
}

fn resetController() bool {
    write32(REG_GHC, read32(REG_GHC) | HBA_GHC_AE);
    write32(REG_GHC, read32(REG_GHC) | HBA_GHC_HR);
    var guard: u32 = 0;
    while ((read32(REG_GHC) & HBA_GHC_HR) != 0 and guard < WAIT_GUARD) : (guard += 1) {}
    write32(REG_GHC, read32(REG_GHC) | HBA_GHC_AE);
    return (read32(REG_GHC) & HBA_GHC_HR) == 0;
}

fn stopPort(base: u64, rt: *PortRuntime) bool {
    var cmd = read32(base + PORT_CMD);
    cmd &= ~PORT_CMD_ST;
    write32(base + PORT_CMD, cmd);
    var guard: u32 = 0;
    while ((read32(base + PORT_CMD) & PORT_CMD_CR) != 0 and guard < WAIT_GUARD) : (guard += 1) {}
    cmd = read32(base + PORT_CMD);
    cmd &= ~PORT_CMD_FRE;
    write32(base + PORT_CMD, cmd);
    guard = 0;
    while ((read32(base + PORT_CMD) & PORT_CMD_FR) != 0 and guard < WAIT_GUARD) : (guard += 1) {}
    if ((read32(base + PORT_CMD) & (PORT_CMD_CR | PORT_CMD_FR)) != 0) {
        rt.last_error = "stop-timeout";
        return false;
    }
    return true;
}

fn startPort(base: u64, rt: *PortRuntime) bool {
    var cmd = read32(base + PORT_CMD);
    cmd |= PORT_CMD_FRE | PORT_CMD_ST | PORT_CMD_SUD | PORT_CMD_POD;
    write32(base + PORT_CMD, cmd);
    var guard: u32 = 0;
    while ((read32(base + PORT_CMD) & PORT_CMD_CR) == 0 and guard < WAIT_GUARD) : (guard += 1) {}
    if ((read32(base + PORT_CMD) & PORT_CMD_CR) == 0) {
        rt.last_error = "start-timeout";
        return false;
    }
    return true;
}

fn waitTfdReady(base: u64) bool {
    var guard: u32 = 0;
    while ((read32(base + PORT_TFD) & (PORT_TFD_BSY | PORT_TFD_DRQ)) != 0 and guard < WAIT_GUARD) : (guard += 1) {}
    return (read32(base + PORT_TFD) & (PORT_TFD_BSY | PORT_TFD_DRQ)) == 0;
}

fn mapMmio(base: u64, bytes: u64) bool {
    var offset: u64 = 0;
    while (offset < bytes) : (offset += paging.PAGE_SIZE) {
        const phys_addr = (base + offset) & ~(paging.PAGE_SIZE - 1);
        const virt = phys.physToVirt(phys_addr);
        if (!paging.isMapped(virt)) {
            if (!paging.mapPage(virt, phys_addr, paging.WRITABLE | paging.CACHE_DISABLE | paging.NO_EXECUTE)) return false;
        }
    }
    return true;
}

fn read32(offset: u64) u32 {
    const ptr: *volatile u32 = @ptrFromInt(current.mmio_virt + offset);
    return ptr.*;
}

fn write32(offset: u64, value: u32) void {
    const ptr: *volatile u32 = @ptrFromInt(current.mmio_virt + offset);
    ptr.* = value;
}

fn logSummary() void {
    bootlog.puts("[AHCI] device ");
    bootlog.putDec(current.device.bus);
    bootlog.puts(":");
    bootlog.putDec(current.device.device);
    bootlog.puts(".");
    bootlog.putDec(current.device.function);
    bootlog.puts(" bar5=0x");
    bootlog.putHex(current.bar5_raw, 8);
    bootlog.puts(" mmio=0x");
    bootlog.putHex(current.mmio_phys, 16);
    bootlog.puts(" cap=0x");
    bootlog.putHex(current.cap, 8);
    bootlog.puts(" pi=0x");
    bootlog.putHex(current.pi, 8);
    bootlog.puts(" ports=");
    bootlog.putDec(current.implemented_ports);
    bootlog.puts(" active_sata=");
    bootlog.putDec(current.active_sata_ports);
    bootlog.puts(" blockdevs=");
    bootlog.putDec(current.block_devices);
    bootlog.puts("\r\n");
}

fn portBase(port: u8) u64 {
    return PORT_BASE + @as(u64, port) * PORT_STRIDE;
}

fn portTypeName(sig: u32) []const u8 {
    return switch (sig) {
        SIG_SATA => "sata",
        SIG_SATAPI => "satapi",
        SIG_SEMB => "enclosure",
        SIG_PORT_MULTIPLIER => "port-multiplier",
        else => "unknown",
    };
}

fn nameForPort(port: u8) []const u8 {
    return switch (port) {
        0 => "ahci0",
        1 => "ahci1",
        2 => "ahci2",
        3 => "ahci3",
        4 => "ahci4",
        5 => "ahci5",
        6 => "ahci6",
        7 => "ahci7",
        else => "ahci?",
    };
}

fn stringEquals(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var i: usize = 0;
    while (i < a.len) : (i += 1) {
        if (a[i] != b[i]) return false;
    }
    return true;
}
