const bootlog = @import("../../kernel/bootlog.zig");
const k = @import("../../kernel/log.zig");
const paging = @import("../../memory/paging.zig");
const phys = @import("../../memory/phys.zig");
const pcie = @import("../../platform/pci_inventory.zig");
const block = @import("../../storage/block.zig");

const REG_CAP: u64 = 0x00;
const REG_VS: u64 = 0x08;
const REG_CC: u64 = 0x14;
const REG_CSTS: u64 = 0x1C;
const REG_AQA: u64 = 0x24;
const REG_ASQ: u64 = 0x28;
const REG_ACQ: u64 = 0x30;

const DOORBELL_BASE: u64 = 0x1000;
const MAP_BYTES: u64 = 0x4000;

const ADMIN_QUEUE_DEPTH: u16 = 16;
const ADMIN_COMMAND_DWORDS: usize = 16;
const ADMIN_COMPLETION_BYTES: u64 = 16;
const ADMIN_WAIT_GUARD: u32 = 10_000_000;
const IO_WAIT_GUARD: u32 = 100_000_000;

const ADMIN_OP_CREATE_IO_SQ: u8 = 0x01;
const ADMIN_OP_CREATE_IO_CQ: u8 = 0x05;
const ADMIN_OP_IDENTIFY: u8 = 0x06;
const IDENTIFY_CNS_CONTROLLER: u32 = 0x01;
const IDENTIFY_CNS_ACTIVE_NAMESPACE_LIST: u32 = 0x02;
const NVM_OP_FLUSH: u8 = 0x00;
const NVM_OP_WRITE: u8 = 0x01;
const NVM_OP_READ: u8 = 0x02;
const IO_QUEUE_ID: u16 = 1;
const IO_QUEUE_DEPTH: u16 = 16;
const NVME_SECTOR_SIZE: usize = 512;
const MAX_IO_SECTORS: u16 = @intCast(phys.FRAME_SIZE / NVME_SECTOR_SIZE);
const MAX_NAMESPACES: usize = 4;

const CC_EN: u32 = 1 << 0;
const CC_CSS_NVM: u32 = 0 << 4;
const CC_MPS_SHIFT: u5 = 7;
const CC_AMS_RR: u32 = 0 << 11;
const CC_IOSQES_64: u32 = 6 << 16;
const CC_IOCQES_16: u32 = 4 << 20;
const CSTS_RDY: u32 = 1 << 0;
const CSTS_CFS: u32 = 1 << 1;

pub const Status = struct {
    probed: bool = false,
    present: bool = false,
    mapped: bool = false,
    device: pcie.Device = .{},
    command: u16 = 0,
    bar0_raw: u32 = 0,
    bar1_raw: u32 = 0,
    bar_is_io: bool = false,
    bar_is_64: bool = false,
    mmio_phys: u64 = 0,
    mmio_virt: u64 = 0,
    cap: u64 = 0,
    vs: u32 = 0,
    cc: u32 = 0,
    csts: u32 = 0,
    aqa: u32 = 0,
    asq: u64 = 0,
    acq: u64 = 0,
    mqes: u16 = 0,
    cqr: bool = false,
    ams: u8 = 0,
    timeout_units: u8 = 0,
    doorbell_stride: u32 = 0,
    nssrs: bool = false,
    css: u8 = 0,
    mpsmin: u8 = 0,
    mpsmax: u8 = 0,
    init_stage: []const u8 = "not-started",
    failure_stage: []const u8 = "none",
    admin_queues_allocated: bool = false,
    admin_queue_configured: bool = false,
    controller_disabled: bool = false,
    controller_enabled: bool = false,
    controller_ready: bool = false,
    identify_controller_ok: bool = false,
    queue_depth: u16 = 0,
    asq_phys: u64 = 0,
    asq_virt: u64 = 0,
    acq_phys: u64 = 0,
    acq_virt: u64 = 0,
    identify_phys: u64 = 0,
    identify_virt: u64 = 0,
    namespace_phys: u64 = 0,
    namespace_virt: u64 = 0,
    admin_sq_tail: u16 = 0,
    admin_cq_head: u16 = 0,
    admin_cq_phase: u8 = 1,
    admin_commands: u64 = 0,
    admin_completions: u64 = 0,
    admin_failures: u64 = 0,
    admin_timeouts: u64 = 0,
    last_admin_opcode: u8 = 0,
    last_admin_cid: u16 = 0,
    last_admin_cdw0: u32 = 0,
    last_admin_cdw3: u32 = 0,
    last_admin_status: u16 = 0,
    identify_vid: u16 = 0,
    identify_ssvid: u16 = 0,
    identify_cntlid: u16 = 0,
    identify_version: u32 = 0,
    identify_mdts: u8 = 0,
    identify_oacs: u16 = 0,
    identify_sqes: u8 = 0,
    identify_cqes: u8 = 0,
    identify_namespaces: u32 = 0,
    namespace_probe_attempted: bool = false,
    namespace_identify_ok: bool = false,
    namespace_usable: bool = false,
    namespace_id: u32 = 0,
    namespace_lba_format: u8 = 0,
    namespace_lba_format_count: u8 = 0,
    namespace_lbads: u8 = 0,
    namespace_metadata_size: u16 = 0,
    namespace_sector_size: u32 = 0,
    namespace_sector_count: u64 = 0,
    namespace_capacity: u64 = 0,
    namespace_reason: []const u8 = "not checked",
    active_namespace_list_ok: bool = false,
    namespace_slots: usize = 0,
    io_queues_allocated: bool = false,
    io_queue_configured: bool = false,
    io_test_read_ok: bool = false,
    io_test_write_ok: bool = false,
    io_test_flush_ok: bool = false,
    block_device_registered: bool = false,
    block_device_count: usize = 0,
    block_device_index: usize = 0,
    iosq_phys: u64 = 0,
    iosq_virt: u64 = 0,
    iocq_phys: u64 = 0,
    iocq_virt: u64 = 0,
    io_dma_phys: u64 = 0,
    io_dma_virt: u64 = 0,
    io_queue_depth: u16 = 0,
    io_sq_tail: u16 = 0,
    io_cq_head: u16 = 0,
    io_cq_phase: u8 = 1,
    io_commands: u64 = 0,
    io_completions: u64 = 0,
    io_failures: u64 = 0,
    io_timeouts: u64 = 0,
    io_cid_mismatches: u64 = 0,
    last_io_opcode: u8 = 0,
    last_io_cid: u16 = 0,
    last_io_cdw0: u32 = 0,
    last_io_cdw3: u32 = 0,
    last_io_status: u16 = 0,
    last_io_lba: u64 = 0,
    last_io_sectors: u16 = 0,
    io_reason: []const u8 = "not checked",
    reason: []const u8 = "not initialized",
};

const NamespaceRuntime = struct {
    probed: bool = false,
    identify_ok: bool = false,
    usable: bool = false,
    nsid: u32 = 0,
    lba_format: u8 = 0,
    lba_format_count: u8 = 0,
    lbads: u8 = 0,
    metadata_size: u16 = 0,
    sector_size: u32 = 0,
    sector_count: u64 = 0,
    capacity: u64 = 0,
    reason: []const u8 = "not checked",
    block_registered: bool = false,
    block_index: usize = 0,
};

var current: Status = .{};
var next_admin_cid: u16 = 1;
var next_io_cid: u16 = 1;
var namespaces: [MAX_NAMESPACES]NamespaceRuntime = .{NamespaceRuntime{}} ** MAX_NAMESPACES;

pub fn probe() bool {
    current = .{ .probed = true, .reason = "not-found", .init_stage = "probe" };
    next_admin_cid = 1;
    next_io_cid = 1;
    resetNamespaces();
    const ps = pcie.status();
    if (ps.nvme_count == 0) {
        bootlog.puts("[NVME] not found\r\n");
        return false;
    }

    current.present = true;
    current.device = ps.first_nvme;
    current.command = pcie.readCommand(current.device);
    current.bar0_raw = pcie.readBar(current.device, 0);
    current.bar1_raw = pcie.readBar(current.device, 1);
    current.bar_is_io = (current.bar0_raw & 0x1) != 0;
    current.bar_is_64 = !current.bar_is_io and (((current.bar0_raw >> 1) & 0x3) == 0x2);

    if (current.bar_is_io) {
        current.reason = "BAR0 is I/O space, expected MMIO";
        current.failure_stage = "bar";
        bootlog.puts("[NVME][WARN] BAR0 is I/O space\r\n");
        return false;
    }

    const bar = pcie.readBar64(current.device, 0);
    current.mmio_phys = bar & 0xFFFF_FFFF_FFFF_FFF0;
    if (current.mmio_phys == 0) {
        current.reason = "BAR0 MMIO base is zero";
        current.failure_stage = "bar";
        bootlog.puts("[NVME][WARN] BAR0 MMIO base is zero\r\n");
        return false;
    }

    enablePciMemoryBusMaster();
    if (!mapMmio(current.mmio_phys, MAP_BYTES)) {
        current.reason = "failed to map NVMe MMIO";
        current.failure_stage = "mmio-map";
        bootlog.puts("[NVME][WARN] MMIO map failed\r\n");
        return false;
    }

    current.mapped = true;
    current.mmio_virt = phys.physToVirt(current.mmio_phys);
    readControllerRegisters();
    current.reason = "diagnostic MMIO mapped, admin init pending";
    if (initAdminPath()) {
        current.reason = if (current.namespace_usable)
            if (current.block_device_registered) "NVMe block read path active" else "namespace identified, no block driver"
        else
            "admin identify-controller ok, no namespace/block driver";
    }
    readControllerRegisters();
    logSummary();
    return current.mapped;
}

pub fn status() Status {
    return current;
}

pub fn blockDeviceCount() usize {
    return current.block_device_count;
}

pub fn deviceIndexAt(index: usize) ?usize {
    var seen: usize = 0;
    for (namespaces) |ns| {
        if (!ns.block_registered) continue;
        if (seen == index) return ns.block_index;
        seen += 1;
    }
    return null;
}

fn initAdminPath() bool {
    setStage("admin-preflight");
    if ((current.css & 0x01) == 0) return failStage("admin-preflight", "NVM command set not supported");
    if (current.mpsmin != 0) return failStage("admin-preflight", "controller requires page size above 4 KiB");
    if (pageSizeFor(current.mpsmin) != phys.FRAME_SIZE) return failStage("admin-preflight", "unsupported memory page size");
    if (!allocateAdminBuffers()) return false;
    if (!disableController()) return false;
    configureAdminQueues();
    if (!enableController()) return false;
    if (!identifyController()) return false;
    _ = identifyNamespaces();
    if (current.namespace_usable) {
        _ = initIoPath();
    }
    setStage(if (current.block_device_registered) "block-ready" else if (current.io_queue_configured) "io-queues-ready" else if (current.namespace_usable) "namespace-ready" else "admin-identify-ok");
    if (!current.namespace_usable or current.block_device_registered) current.failure_stage = "none";
    return true;
}

fn allocateAdminBuffers() bool {
    setStage("admin-alloc");
    current.asq_phys = allocFrameZero() orelse return failStage("admin-alloc-asq", "failed to allocate admin submission queue");
    current.acq_phys = allocFrameZero() orelse return failStage("admin-alloc-acq", "failed to allocate admin completion queue");
    current.identify_phys = allocFrameZero() orelse return failStage("admin-alloc-identify", "failed to allocate identify buffer");
    current.namespace_phys = allocFrameZero() orelse return failStage("admin-alloc-namespace", "failed to allocate namespace identify buffer");
    current.asq_virt = phys.physToVirt(current.asq_phys);
    current.acq_virt = phys.physToVirt(current.acq_phys);
    current.identify_virt = phys.physToVirt(current.identify_phys);
    current.namespace_virt = phys.physToVirt(current.namespace_phys);
    current.admin_queues_allocated = true;
    current.queue_depth = ADMIN_QUEUE_DEPTH;
    current.admin_sq_tail = 0;
    current.admin_cq_head = 0;
    current.admin_cq_phase = 1;
    return true;
}

fn disableController() bool {
    setStage("disable-controller");
    const cc = read32(REG_CC);
    write32(REG_CC, cc & ~CC_EN);
    if (!waitReady(false)) return failStage("disable-controller", current.reason);
    current.controller_disabled = true;
    current.controller_enabled = false;
    current.controller_ready = false;
    readControllerRegisters();
    return true;
}

fn configureAdminQueues() void {
    setStage("configure-admin-queues");
    const size_zero_based = @as(u32, ADMIN_QUEUE_DEPTH - 1);
    write32(REG_AQA, size_zero_based | (size_zero_based << 16));
    write64(REG_ASQ, current.asq_phys);
    write64(REG_ACQ, current.acq_phys);
    current.aqa = read32(REG_AQA);
    current.asq = read64(REG_ASQ);
    current.acq = read64(REG_ACQ);
    current.admin_queue_configured = true;
}

fn enableController() bool {
    setStage("enable-controller");
    const mps = @as(u32, current.mpsmin) << CC_MPS_SHIFT;
    const cc = CC_EN | CC_CSS_NVM | mps | CC_AMS_RR | CC_IOSQES_64 | CC_IOCQES_16;
    write32(REG_CC, cc);
    if (!waitReady(true)) return failStage("enable-controller", current.reason);
    current.controller_enabled = true;
    current.controller_ready = true;
    readControllerRegisters();
    return true;
}

fn identifyController() bool {
    setStage("identify-controller");
    zeroFrame(current.identify_virt);
    if (!submitIdentifyCommand(0, IDENTIFY_CNS_CONTROLLER, current.identify_phys)) return false;
    current.identify_controller_ok = true;
    parseIdentifyController();
    return true;
}

fn identifyNamespaces() bool {
    if (current.identify_namespaces == 0) {
        current.namespace_probe_attempted = true;
        current.namespace_reason = "controller reports zero namespaces";
        return false;
    }

    var nsids: [MAX_NAMESPACES]u32 = .{0} ** MAX_NAMESPACES;
    var count: usize = 0;
    if (identifyActiveNamespaceList(&nsids)) {
        current.active_namespace_list_ok = true;
        while (count < nsids.len and nsids[count] != 0) : (count += 1) {}
    } else {
        nsids[0] = 1;
        count = 1;
        current.namespace_reason = "active namespace list unavailable; falling back to NSID 1";
    }

    var ok = false;
    var index: usize = 0;
    while (index < count) : (index += 1) {
        if (identifyNamespace(nsids[index], index)) ok = true;
    }
    current.namespace_slots = count;
    return ok;
}

fn identifyActiveNamespaceList(out: *[MAX_NAMESPACES]u32) bool {
    current.namespace_probe_attempted = true;
    setStage("identify-active-ns-list");
    zeroFrame(current.namespace_virt);
    if (!submitIdentifyCommand(0, IDENTIFY_CNS_ACTIVE_NAMESPACE_LIST, current.namespace_phys)) return false;

    const words: [*]const u32 = @ptrFromInt(current.namespace_virt);
    var index: usize = 0;
    while (index < out.len) : (index += 1) {
        out[index] = words[index];
    }
    return out[0] != 0;
}

fn identifyNamespace(nsid: u32, slot: usize) bool {
    current.namespace_probe_attempted = true;
    if (slot >= namespaces.len) return false;
    const ns = &namespaces[slot];
    ns.* = .{ .probed = true, .nsid = nsid };

    setStage("identify-namespace");
    zeroFrame(current.namespace_virt);
    if (!submitIdentifyCommand(nsid, 0, current.namespace_phys)) {
        ns.reason = current.reason;
        if (slot == 0) syncPrimaryNamespace(ns);
        return false;
    }

    ns.identify_ok = true;
    parseIdentifyNamespace(ns);
    if (slot == 0) syncPrimaryNamespace(ns);
    return ns.usable;
}

fn submitIdentifyCommand(nsid: u32, cns: u32, buffer_phys: u64) bool {
    return submitAdminCommand(ADMIN_OP_IDENTIFY, nsid, buffer_phys, cns, 0);
}

fn submitAdminCommand(opcode: u8, nsid: u32, prp1: u64, cdw10: u32, cdw11: u32) bool {
    const cid = allocateAdminCid();
    const tail = current.admin_sq_tail;
    const cmd = adminSqCommand(tail);
    @memset(cmd[0..ADMIN_COMMAND_DWORDS], 0);
    cmd[0] = @as(u32, opcode) | (@as(u32, cid) << 16);
    cmd[1] = nsid;
    cmd[6] = @truncate(prp1);
    cmd[7] = @truncate(prp1 >> 32);
    cmd[10] = cdw10;
    cmd[11] = cdw11;

    current.last_admin_opcode = opcode;
    current.last_admin_cid = cid;
    current.last_admin_status = 0xFFFF;
    current.admin_commands += 1;
    current.admin_sq_tail = nextQueueIndex(current.admin_sq_tail, current.queue_depth);
    dmaFence();
    write32(doorbellOffset(0, 0), current.admin_sq_tail);

    return pollAdminCompletion(cid);
}

fn initIoPath() bool {
    if (!allocateIoBuffers()) return false;
    if (!createIoCompletionQueue()) return false;
    if (!createIoSubmissionQueue()) return false;
    current.io_queue_configured = true;
    current.io_reason = "I/O queues ready";
    return registerNamespaceBlockDevices();
}

fn allocateIoBuffers() bool {
    setStage("io-alloc");
    current.iosq_phys = allocFrameZero() orelse return failIoStage("io-alloc-iosq", "failed to allocate I/O submission queue");
    current.iocq_phys = allocFrameZero() orelse return failIoStage("io-alloc-iocq", "failed to allocate I/O completion queue");
    current.io_dma_phys = allocFrameZero() orelse return failIoStage("io-alloc-dma", "failed to allocate I/O DMA buffer");
    current.iosq_virt = phys.physToVirt(current.iosq_phys);
    current.iocq_virt = phys.physToVirt(current.iocq_phys);
    current.io_dma_virt = phys.physToVirt(current.io_dma_phys);
    current.io_queues_allocated = true;
    current.io_queue_depth = IO_QUEUE_DEPTH;
    current.io_sq_tail = 0;
    current.io_cq_head = 0;
    current.io_cq_phase = 1;
    return true;
}

fn createIoCompletionQueue() bool {
    setStage("create-io-cq");
    const size_zero_based = @as(u32, current.io_queue_depth - 1);
    const cdw10 = @as(u32, IO_QUEUE_ID) | (size_zero_based << 16);
    const cdw11 = @as(u32, 1); // physically contiguous, interrupts disabled
    return submitAdminCommand(ADMIN_OP_CREATE_IO_CQ, 0, current.iocq_phys, cdw10, cdw11);
}

fn createIoSubmissionQueue() bool {
    setStage("create-io-sq");
    const size_zero_based = @as(u32, current.io_queue_depth - 1);
    const cdw10 = @as(u32, IO_QUEUE_ID) | (size_zero_based << 16);
    const cdw11 = @as(u32, 1) | (@as(u32, IO_QUEUE_ID) << 16); // physically contiguous, paired with CQ 1
    return submitAdminCommand(ADMIN_OP_CREATE_IO_SQ, 0, current.iosq_phys, cdw10, cdw11);
}

fn testReadSectorZero(ns: *NamespaceRuntime) bool {
    var scratch: [NVME_SECTOR_SIZE]u8 = undefined;
    if (!readBlock(ns, 0, 1, scratch[0..])) return false;
    current.io_test_read_ok = true;
    current.io_reason = "sector 0 read ok";
    return true;
}

fn registerNamespaceBlockDevices() bool {
    var ok = false;
    var index: usize = 0;
    while (index < namespaces.len) : (index += 1) {
        const ns = &namespaces[index];
        if (!ns.usable) continue;
        if (!testReadSectorZero(ns)) continue;
        if (registerBlockDevice(ns, index)) ok = true;
    }
    if (!ok) return failIoStage("register-block", "no namespace block device registered");
    return true;
}

fn registerBlockDevice(ns: *NamespaceRuntime, slot: usize) bool {
    setStage("register-block");
    const block_index = block.register(.{
        .name = nameForNamespace(slot),
        .driver = "NVMe",
        .bus = .nvme,
        .controller = "pcie-nvme",
        .port = @truncate(ns.nsid),
        .sector_size = ns.sector_size,
        .sector_count = ns.sector_count,
        .max_sectors_per_request = MAX_IO_SECTORS,
        .queue_depth = current.io_queue_depth,
        .timeout_ticks = 0,
        .writable = true,
        .ctx = ns,
        .read_fn = readBlock,
        .write_fn = writeBlock,
        .flush_fn = flushBlock,
    }) orelse return failIoStage("register-block", "block register failed");
    ns.block_registered = true;
    ns.block_index = block_index;
    current.block_device_registered = true;
    current.block_device_count += 1;
    if (current.block_device_count == 1) current.block_device_index = block_index;
    current.io_reason = "read-write block device registered";
    bootlog.puts("[NVME] ");
    bootlog.puts(nameForNamespace(slot));
    bootlog.puts(" block=#");
    bootlog.putDec(block_index);
    bootlog.puts(" read-write nsid=");
    bootlog.putDec(ns.nsid);
    bootlog.puts(" sectors=");
    bootlog.putDec(ns.sector_count);
    bootlog.puts(" lba=");
    bootlog.putDec(ns.sector_size);
    bootlog.puts("\r\n");
    return true;
}

fn pollAdminCompletion(expected_cid: u16) bool {
    setStage("admin-completion");
    var guard: u32 = 0;
    while (guard < ADMIN_WAIT_GUARD) : (guard += 1) {
        const dw3 = adminCqDword(current.admin_cq_head, 3);
        if (completionPhase(dw3) != current.admin_cq_phase) continue;
        const dw0 = adminCqDword(current.admin_cq_head, 0);
        current.last_admin_cdw0 = dw0;
        current.last_admin_cdw3 = dw3;
        current.last_admin_cid = completionCid(dw3);
        current.last_admin_status = completionStatus(dw3);
        current.admin_completions += 1;
        advanceAdminCqHead();

        if (current.last_admin_cid != expected_cid) {
            return failStage("admin-completion", "completion CID mismatch");
        }
        if (current.last_admin_status != 0) {
            return failStage("admin-completion", "completion status error");
        }
        current.reason = "admin command completed";
        return true;
    }

    current.admin_timeouts += 1;
    current.last_admin_cdw0 = adminCqDword(current.admin_cq_head, 0);
    current.last_admin_cdw3 = adminCqDword(current.admin_cq_head, 3);
    current.last_admin_status = completionStatus(current.last_admin_cdw3);
    return failStage("admin-completion", "admin completion timeout");
}

fn advanceAdminCqHead() void {
    current.admin_cq_head = nextQueueIndex(current.admin_cq_head, current.queue_depth);
    if (current.admin_cq_head == 0) {
        current.admin_cq_phase = if (current.admin_cq_phase == 1) 0 else 1;
    }
    dmaFence();
    write32(doorbellOffset(0, 1), current.admin_cq_head);
}

fn readBlock(ctx: ?*anyopaque, lba: u64, sectors: u16, out: []u8) bool {
    const ns = namespaceFromContext(ctx) orelse return failIoStage("read", "namespace context missing");
    const bytes = validateIoTransfer(ns, "read", lba, sectors, out.len) orelse return false;

    const dma: [*]u8 = @ptrFromInt(current.io_dma_virt);
    @memset(dma[0..bytes], 0);
    if (!submitIoCommand(ns.nsid, NVM_OP_READ, lba, sectors, current.io_dma_phys)) return false;
    dmaFence();
    @memcpy(out[0..bytes], dma[0..bytes]);
    current.io_reason = "read completed";
    return true;
}

fn writeBlock(ctx: ?*anyopaque, lba: u64, sectors: u16, data: []const u8) bool {
    const ns = namespaceFromContext(ctx) orelse return failIoStage("write", "namespace context missing");
    const bytes = validateIoTransfer(ns, "write", lba, sectors, data.len) orelse return false;
    const dma: [*]u8 = @ptrFromInt(current.io_dma_virt);
    @memcpy(dma[0..bytes], data[0..bytes]);
    dmaFence();
    if (!submitIoCommand(ns.nsid, NVM_OP_WRITE, lba, sectors, current.io_dma_phys)) return false;
    current.io_test_write_ok = true;
    current.io_reason = "write completed";
    return true;
}

fn flushBlock(ctx: ?*anyopaque) bool {
    const ns = namespaceFromContext(ctx) orelse return failIoStage("flush", "namespace context missing");
    if (!current.io_queue_configured) return failIoStage("flush", "I/O queue not configured");
    if (!ns.usable) return failIoStage("flush", "namespace not usable");
    if (!submitIoCommand(ns.nsid, NVM_OP_FLUSH, 0, 0, 0)) return false;
    current.io_test_flush_ok = true;
    current.io_reason = "flush completed";
    return true;
}

fn submitIoCommand(nsid: u32, opcode: u8, lba: u64, sectors: u16, prp1: u64) bool {
    const cid = allocateIoCid();
    const tail = current.io_sq_tail;
    const cmd = ioSqCommand(tail);
    @memset(cmd[0..ADMIN_COMMAND_DWORDS], 0);
    cmd[0] = @as(u32, opcode) | (@as(u32, cid) << 16);
    cmd[1] = nsid;
    if (prp1 != 0) {
        cmd[6] = @truncate(prp1);
        cmd[7] = @truncate(prp1 >> 32);
    }
    if (sectors != 0) {
        cmd[10] = @truncate(lba);
        cmd[11] = @truncate(lba >> 32);
        cmd[12] = @as(u32, sectors - 1);
    }

    current.last_io_opcode = opcode;
    current.last_io_cid = cid;
    current.last_io_status = 0xFFFF;
    current.last_io_lba = lba;
    current.last_io_sectors = sectors;
    current.io_commands += 1;
    current.io_sq_tail = nextQueueIndex(current.io_sq_tail, current.io_queue_depth);
    dmaFence();
    write32(doorbellOffset(IO_QUEUE_ID, 0), current.io_sq_tail);

    return pollIoCompletion(cid);
}

fn validateIoTransfer(ns: *const NamespaceRuntime, stage: []const u8, lba: u64, sectors: u16, buffer_len: usize) ?usize {
    if (!current.io_queue_configured) {
        _ = failIoStage(stage, "I/O queue not configured");
        return null;
    }
    if (!ns.usable) {
        _ = failIoStage(stage, "namespace not usable");
        return null;
    }
    if (sectors == 0 or sectors > MAX_IO_SECTORS) {
        _ = failIoStage(stage, "bad sector count");
        return null;
    }
    const bytes = @as(usize, sectors) * NVME_SECTOR_SIZE;
    if (buffer_len < bytes) {
        _ = failIoStage(stage, "buffer too small");
        return null;
    }
    if (lba >= ns.sector_count) {
        _ = failIoStage(stage, "LBA out of range");
        return null;
    }
    if (@as(u64, sectors) > ns.sector_count - lba) {
        _ = failIoStage(stage, "request crosses namespace end");
        return null;
    }
    return bytes;
}

fn pollIoCompletion(expected_cid: u16) bool {
    var guard: u32 = 0;
    while (guard < IO_WAIT_GUARD) : (guard += 1) {
        const dw3 = ioCqDword(current.io_cq_head, 3);
        if (completionPhase(dw3) != current.io_cq_phase) continue;
        const dw0 = ioCqDword(current.io_cq_head, 0);
        current.last_io_cdw0 = dw0;
        current.last_io_cdw3 = dw3;
        current.last_io_cid = completionCid(dw3);
        current.last_io_status = completionStatus(dw3);
        current.io_completions += 1;
        advanceIoCqHead();

        if (current.last_io_cid != expected_cid) {
            current.io_cid_mismatches += 1;
            current.io_reason = "skipped stale completion CID";
            continue;
        }
        if (current.last_io_status != 0) {
            return failIoStage("io-completion", "completion status error");
        }
        current.io_reason = "I/O command completed";
        return true;
    }

    current.io_timeouts += 1;
    current.last_io_cdw0 = ioCqDword(current.io_cq_head, 0);
    current.last_io_cdw3 = ioCqDword(current.io_cq_head, 3);
    current.last_io_status = completionStatus(current.last_io_cdw3);
    return failIoStage("io-completion", "I/O completion timeout");
}

fn advanceIoCqHead() void {
    current.io_cq_head = nextQueueIndex(current.io_cq_head, current.io_queue_depth);
    if (current.io_cq_head == 0) {
        current.io_cq_phase = if (current.io_cq_phase == 1) 0 else 1;
    }
    dmaFence();
    write32(doorbellOffset(IO_QUEUE_ID, 1), current.io_cq_head);
}

fn readControllerRegisters() void {
    current.cap = read64(REG_CAP);
    current.vs = read32(REG_VS);
    current.cc = read32(REG_CC);
    current.csts = read32(REG_CSTS);
    current.aqa = read32(REG_AQA);
    current.asq = read64(REG_ASQ);
    current.acq = read64(REG_ACQ);

    current.mqes = @truncate(current.cap & 0xFFFF);
    current.cqr = (current.cap & (1 << 16)) != 0;
    current.ams = @truncate((current.cap >> 17) & 0x3);
    current.timeout_units = @truncate((current.cap >> 24) & 0xFF);
    const dstrd: u5 = @truncate((current.cap >> 32) & 0xF);
    current.doorbell_stride = @as(u32, 4) << dstrd;
    current.nssrs = (current.cap & (1 << 36)) != 0;
    current.css = @truncate((current.cap >> 37) & 0xFF);
    current.mpsmin = @truncate((current.cap >> 48) & 0xF);
    current.mpsmax = @truncate((current.cap >> 52) & 0xF);
}

fn parseIdentifyController() void {
    current.identify_vid = identify16(0);
    current.identify_ssvid = identify16(2);
    current.identify_mdts = identify8(77);
    current.identify_cntlid = identify16(78);
    current.identify_version = identify32(80);
    current.identify_oacs = identify16(256);
    current.identify_sqes = identify8(512);
    current.identify_cqes = identify8(513);
    current.identify_namespaces = identify32(516);
}

fn parseIdentifyNamespace(ns: *NamespaceRuntime) void {
    const nsze = namespace64(0);
    const ncap = namespace64(8);
    const nlbaf = namespace8(25);
    const flbas = namespace8(26);
    const format = flbas & 0x0F;
    const lbaf_offset = 128 + @as(u64, format) * 4;
    const metadata = namespace16(lbaf_offset);
    const lbads = namespace8(lbaf_offset + 2);

    ns.sector_count = nsze;
    ns.lba_format = format;
    ns.lba_format_count = nlbaf + 1;
    ns.metadata_size = metadata;
    ns.lbads = lbads;
    ns.sector_size = if (lbads < 32) @as(u32, 1) << @intCast(lbads) else 0;
    ns.capacity = 0;
    ns.usable = false;

    if (format > nlbaf) {
        ns.reason = "active LBA format outside advertised range";
        return;
    }
    if (nsze == 0 or ncap == 0) {
        ns.reason = "namespace has zero capacity";
        return;
    }
    if (ns.sector_size == 0) {
        ns.reason = "unsupported LBA data size";
        return;
    }
    if (ns.sector_size != 512) {
        ns.reason = "LBA size is not 512 bytes";
        return;
    }
    if (metadata != 0) {
        ns.reason = "metadata LBA format not supported yet";
        return;
    }
    if (nsze > maxU64() / ns.sector_size) {
        ns.reason = "namespace capacity overflow";
        return;
    }

    ns.capacity = nsze * ns.sector_size;
    ns.usable = true;
    ns.reason = "namespace ready for block I/O";
}

fn waitReady(want_ready: bool) bool {
    var guard: u32 = 0;
    while (guard < ADMIN_WAIT_GUARD) : (guard += 1) {
        const csts = read32(REG_CSTS);
        current.csts = csts;
        if ((csts & CSTS_CFS) != 0) {
            current.reason = "controller fatal status";
            return false;
        }
        const ready = (csts & CSTS_RDY) != 0;
        if (ready == want_ready) return true;
    }
    current.admin_timeouts += 1;
    current.reason = if (want_ready) "controller ready timeout" else "controller disable timeout";
    return false;
}

fn setStage(stage: []const u8) void {
    current.init_stage = stage;
}

fn failStage(stage: []const u8, reason: []const u8) bool {
    current.failure_stage = stage;
    current.reason = reason;
    current.admin_failures += 1;
    return false;
}

fn failIoStage(stage: []const u8, reason: []const u8) bool {
    current.failure_stage = stage;
    current.io_reason = reason;
    current.io_failures += 1;
    return false;
}

fn resetNamespaces() void {
    namespaces = .{NamespaceRuntime{}} ** MAX_NAMESPACES;
}

fn syncPrimaryNamespace(ns: *const NamespaceRuntime) void {
    current.namespace_identify_ok = ns.identify_ok;
    current.namespace_usable = ns.usable;
    current.namespace_id = ns.nsid;
    current.namespace_lba_format = ns.lba_format;
    current.namespace_lba_format_count = ns.lba_format_count;
    current.namespace_lbads = ns.lbads;
    current.namespace_metadata_size = ns.metadata_size;
    current.namespace_sector_size = ns.sector_size;
    current.namespace_sector_count = ns.sector_count;
    current.namespace_capacity = ns.capacity;
    current.namespace_reason = ns.reason;
}

fn namespaceFromContext(ctx: ?*anyopaque) ?*NamespaceRuntime {
    const ptr = ctx orelse return null;
    return @ptrCast(@alignCast(ptr));
}

fn nameForNamespace(slot: usize) []const u8 {
    return switch (slot) {
        0 => "nvme0",
        1 => "nvme1",
        2 => "nvme2",
        3 => "nvme3",
        else => "nvme?",
    };
}

fn enablePciMemoryBusMaster() void {
    var command = pcie.readCommand(current.device);
    command |= 0x0006;
    _ = pcie.writeCommand(current.device, command);
    current.command = pcie.readCommand(current.device);
}

fn dmaFence() void {
    asm volatile ("mfence");
}

fn allocFrameZero() ?u64 {
    const frame = phys.allocFrame() orelse return null;
    zeroFrame(phys.physToVirt(frame));
    return frame;
}

fn zeroFrame(virt: u64) void {
    const bytes: [*]u8 = @ptrFromInt(virt);
    @memset(bytes[0..@intCast(phys.FRAME_SIZE)], 0);
}

fn adminSqCommand(index_value: u16) []u32 {
    const offset = @as(u64, index_value) * 64;
    const dwords: [*]u32 = @ptrFromInt(current.asq_virt + offset);
    return dwords[0..ADMIN_COMMAND_DWORDS];
}

fn adminCqDword(index_value: u16, dword: u64) u32 {
    const offset = @as(u64, index_value) * ADMIN_COMPLETION_BYTES + dword * 4;
    const ptr: *volatile u32 = @ptrFromInt(current.acq_virt + offset);
    return ptr.*;
}

fn ioSqCommand(index_value: u16) []u32 {
    const offset = @as(u64, index_value) * 64;
    const dwords: [*]u32 = @ptrFromInt(current.iosq_virt + offset);
    return dwords[0..ADMIN_COMMAND_DWORDS];
}

fn ioCqDword(index_value: u16, dword: u64) u32 {
    const offset = @as(u64, index_value) * ADMIN_COMPLETION_BYTES + dword * 4;
    const ptr: *volatile u32 = @ptrFromInt(current.iocq_virt + offset);
    return ptr.*;
}

fn completionCid(dw3: u32) u16 {
    return @truncate(dw3 & 0xFFFF);
}

fn completionPhase(dw3: u32) u8 {
    return @truncate((dw3 >> 16) & 1);
}

fn completionStatus(dw3: u32) u16 {
    return @truncate((dw3 >> 17) & 0x7FFF);
}

fn nextQueueIndex(value: u16, depth: u16) u16 {
    const next = value + 1;
    return if (next >= depth) 0 else next;
}

fn allocateAdminCid() u16 {
    const cid = next_admin_cid;
    next_admin_cid +%= 1;
    if (next_admin_cid == 0) next_admin_cid = 1;
    return cid;
}

fn allocateIoCid() u16 {
    const cid = next_io_cid;
    next_io_cid +%= 1;
    if (next_io_cid == 0) next_io_cid = 1;
    return cid;
}

fn doorbellOffset(qid: u16, doorbell: u8) u64 {
    const index_value = @as(u64, qid) * 2 + @as(u64, doorbell);
    return DOORBELL_BASE + index_value * current.doorbell_stride;
}

fn identify8(offset: u64) u8 {
    const bytes: [*]u8 = @ptrFromInt(current.identify_virt);
    return bytes[offset];
}

fn identify16(offset: u64) u16 {
    return @as(u16, identify8(offset)) | (@as(u16, identify8(offset + 1)) << 8);
}

fn identify32(offset: u64) u32 {
    return @as(u32, identify16(offset)) | (@as(u32, identify16(offset + 2)) << 16);
}

fn namespace8(offset: u64) u8 {
    const bytes: [*]u8 = @ptrFromInt(current.namespace_virt);
    return bytes[offset];
}

fn namespace16(offset: u64) u16 {
    return @as(u16, namespace8(offset)) | (@as(u16, namespace8(offset + 1)) << 8);
}

fn namespace32(offset: u64) u32 {
    return @as(u32, namespace16(offset)) | (@as(u32, namespace16(offset + 2)) << 16);
}

fn namespace64(offset: u64) u64 {
    return @as(u64, namespace32(offset)) | (@as(u64, namespace32(offset + 4)) << 32);
}

fn maxU64() u64 {
    return ~@as(u64, 0);
}

fn dumpIdentifyAscii(offset: u64, len: u64) void {
    if (current.identify_virt == 0 or len == 0) {
        k.puts("(none)");
        return;
    }
    var end = offset + len;
    while (end > offset) {
        const ch = identify8(end - 1);
        if (ch != 0 and ch != ' ') break;
        end -= 1;
    }
    var start = offset;
    while (start < end) {
        const ch = identify8(start);
        if (ch != 0 and ch != ' ') break;
        start += 1;
    }
    if (start >= end) {
        k.puts("(blank)");
        return;
    }
    var i = start;
    while (i < end) : (i += 1) {
        const ch = identify8(i);
        k.putc(if (ch >= 32 and ch < 127) ch else '.');
    }
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

fn read64(offset: u64) u64 {
    const lo = read32(offset);
    const hi = read32(offset + 4);
    return (@as(u64, hi) << 32) | @as(u64, lo);
}

fn write64(offset: u64, value: u64) void {
    write32(offset, @truncate(value));
    write32(offset + 4, @truncate(value >> 32));
}

fn pageSizeFor(mps: u8) u64 {
    const shift: u6 = @intCast(12 + @as(u16, mps));
    return @as(u64, 1) << shift;
}

fn logSummary() void {
    bootlog.puts("[NVME] device ");
    bootlog.putDec(current.device.bus);
    bootlog.puts(":");
    bootlog.putDec(current.device.device);
    bootlog.puts(".");
    bootlog.putDec(current.device.function);
    bootlog.puts(" bar0=0x");
    bootlog.putHex(current.bar0_raw, 8);
    bootlog.puts(" mmio=0x");
    bootlog.putHex(current.mmio_phys, 16);
    bootlog.puts(" vs=");
    bootlog.putDec((current.vs >> 16) & 0xFFFF);
    bootlog.puts(".");
    bootlog.putDec((current.vs >> 8) & 0xFF);
    bootlog.puts(".");
    bootlog.putDec(current.vs & 0xFF);
    bootlog.puts(" csts=0x");
    bootlog.putHex(current.csts, 8);
    bootlog.puts(" caps: mqes=");
    bootlog.putDec(current.mqes);
    bootlog.puts(" doorbell_stride=");
    bootlog.putDec(current.doorbell_stride);
    bootlog.puts(" admin=");
    if (current.namespace_usable) {
        bootlog.puts("namespace-ready nsid=");
        bootlog.putDec(current.namespace_id);
        bootlog.puts(" sectors=");
        bootlog.putDec(current.namespace_sector_count);
        bootlog.puts(" lba=");
        bootlog.putDec(current.namespace_sector_size);
    } else if (current.identify_controller_ok) {
        bootlog.puts("identify-ok namespaces=");
        bootlog.putDec(current.identify_namespaces);
    } else if (current.admin_queue_configured) {
        bootlog.puts("queue-configured");
    } else if (current.mapped) {
        bootlog.puts("mmio-only");
    } else {
        bootlog.puts("not-ready");
    }
    bootlog.puts(" [diagnostic]\r\n");
}
