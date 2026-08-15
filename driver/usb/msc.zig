const block = @import("../../storage/block.zig");
const diag_screen = @import("../../kernel/diag_screen.zig");
const k = @import("../../kernel/log.zig");
const protocol_api = @import("../../kernel/protocol_api.zig");
const r4p = @import("../../program/r4p.zig");
const r4p_contract = @import("../../net/r4p_contract.zig");
const usb_core = @import("core.zig");
const retry_policy = @import("usb_msc_retry.zig");
const xhci = @import("xhci.zig");
const usb_timing = @import("usb_boot_timing.zig");
const usb_wait = @import("usb_boot_wait.zig");

const SECTOR_SIZE: u32 = 512;
const MAX_SECTORS_PER_REQUEST: u16 = 8;
const CBW_LEN: usize = 31;
const CSW_LEN: usize = 13;
const DATA_BUF_LEN: usize = 4096;

const Direction = enum {
    none,
    in,
    out,
};

const ScsiCommand = struct {
    cdb: [16]u8 = .{0} ** 16,
    len: usize = 0,
    direction: Direction = .none,
    transfer_len: u32 = 0,
};

pub const Status = struct {
    initialized: bool = false,
    present: bool = false,
    bound: bool = false,
    block_registered: bool = false,
    block_index: ?usize = null,
    port: u8 = 0,
    device: xhci.DeviceHandle = .{},
    bulk_in_handle: xhci.EndpointHandle = .{},
    bulk_out_handle: xhci.EndpointHandle = .{},
    interface_number: u8 = 0,
    bulk_in: u8 = 0,
    bulk_out: u8 = 0,
    bulk_in_max_packet: u16 = 0,
    bulk_out_max_packet: u16 = 0,
    sector_count: u64 = 0,
    sector_size: u32 = 0,
    inquiry_ok: bool = false,
    inquiry_retries: u64 = 0,
    test_unit_ready_ok: bool = false,
    test_unit_ready_retries: u64 = 0,
    read_capacity_ok: bool = false,
    read_capacity_retries: u64 = 0,
    mode_sense_ok: bool = false,
    write_protected_known: bool = false,
    write_protected: bool = false,
    tags: u32 = 0,
    commands: u64 = 0,
    failures: u64 = 0,
    reads: u64 = 0,
    read_failures: u64 = 0,
    read_transport_retries: u64 = 0,
    read_transport_retry_successes: u64 = 0,
    writes: u64 = 0,
    write_failures: u64 = 0,
    write_transport_retries: u64 = 0,
    write_transport_retry_successes: u64 = 0,
    flushes: u64 = 0,
    flush_failures: u64 = 0,
    flush_transport_retries: u64 = 0,
    flush_transport_retry_successes: u64 = 0,
    flush_unsupported: u64 = 0,
    recoveries: u64 = 0,
    recovery_failures: u64 = 0,
    resets: u64 = 0,
    clear_halts: u64 = 0,
    sense_requests: u64 = 0,
    sense_valid: bool = false,
    sense_for_opcode: u8 = 0,
    sense_key: u8 = 0,
    sense_asc: u8 = 0,
    sense_ascq: u8 = 0,
    sense_not_ready: u64 = 0,
    sense_unit_attention: u64 = 0,
    sense_illegal_request: u64 = 0,
    sense_write_protect: u64 = 0,
    command_failures: u64 = 0,
    transport_failures: u64 = 0,
    transport_faulted: bool = false,
    last_failure_transport: bool = false,
    last_recovery_ok: bool = false,
    invalid_csw: u64 = 0,
    csw_tag_mismatches: u64 = 0,
    csw_phase_errors: u64 = 0,
    csw_residue_errors: u64 = 0,
    short_data_errors: u64 = 0,
    last_opcode: u8 = 0,
    last_transfer_len: u32 = 0,
    last_data_actual_len: u32 = 0,
    last_csw_tag: u32 = 0,
    last_csw_residue: u32 = 0,
    last_csw_status: u8 = 0xFF,
    bot_source: []const u8 = "none",
    bot_r4p_build: u64 = 0,
    bot_r4p_csw: u64 = 0,
    bot_dispatch_failures: u64 = 0,
    bot_last_result: i32 = 0,
    scsi_source: []const u8 = "none",
    scsi_r4p_build: u64 = 0,
    scsi_r4p_parse: u64 = 0,
    scsi_dispatch_failures: u64 = 0,
    scsi_last_result: i32 = 0,
    protocol_required_missing: u64 = 0,
    reason: []const u8 = "not initialized",
};

var current: Status = .{};
var data_buf: [DATA_BUF_LEN]u8 = .{0} ** DATA_BUF_LEN;
var bot_r4p_build: u64 = 0;
var bot_r4p_csw: u64 = 0;
var bot_dispatch_failures: u64 = 0;
var bot_last_result: i32 = 0;
var scsi_r4p_build: u64 = 0;
var scsi_r4p_parse: u64 = 0;
var scsi_dispatch_failures: u64 = 0;
var scsi_last_result: i32 = 0;
var block_source: block.Source = .builtin;
var external_owner: bool = false;
var reason_buffer: [128]u8 = .{0} ** 128;
var last_transport_incident: diag_screen.IncidentToken = .{};

pub fn init() bool {
    if (!xhci.acquireControllerOwnership()) return false;
    defer xhci.releaseControllerOwnership();
    resolveTransportIncident();
    current = .{ .initialized = true, .reason = "no USB mass storage device found" };
    bot_r4p_build = 0;
    bot_r4p_csw = 0;
    bot_dispatch_failures = 0;
    bot_last_result = 0;
    scsi_r4p_build = 0;
    scsi_r4p_parse = 0;
    scsi_dispatch_failures = 0;
    scsi_last_result = 0;
    last_transport_incident = .{};
    @memset(reason_buffer[0..], 0);

    var index: usize = 0;
    while (usb_core.deviceAt(index)) |dev| : (index += 1) {
        if (dev.first_interface_class != 0x08 or dev.first_interface_subclass != 0x06 or dev.first_interface_protocol != 0x50) continue;
        current.present = true;
        current.port = dev.port;
        current.device = xhci.deviceHandleFromCore(dev);
        current.bulk_in_handle = xhci.bulkInHandleFromCore(dev);
        current.bulk_out_handle = xhci.bulkOutHandleFromCore(dev);
        current.interface_number = dev.first_interface_number;
        current.bulk_in = dev.bulk_in_endpoint_address;
        current.bulk_out = dev.bulk_out_endpoint_address;
        current.bulk_in_max_packet = dev.bulk_in_endpoint_max_packet;
        current.bulk_out_max_packet = dev.bulk_out_endpoint_max_packet;
        if (current.bulk_in == 0 or current.bulk_out == 0) {
            current.failures += 1;
            current.reason = "bulk endpoints missing";
            return false;
        }
        if (!requireMscProtocolRoles()) return false;
        if (!ensureSelected()) return false;
        current.bound = true;
        // Real flash media may NAK bulk traffic for a while after becoming
        // Configured. Do not turn CPU speed into an accidental readiness
        // timeout; give the BOT transport one deterministic settle window.
        usb_wait.milliseconds(usb_timing.MSC_POST_CONFIG_SETTLE_MS);
        current.inquiry_ok = scsiInquiryWithRetry();
        if (!current.inquiry_ok) {
            return false;
        }
        current.test_unit_ready_ok = waitForScsiReady();
        if (!current.test_unit_ready_ok) {
            current.reason = "SCSI device not ready after retry window";
            return false;
        }
        current.read_capacity_ok = scsiReadCapacityWithRetry();
        if (!current.read_capacity_ok) {
            current.reason = "SCSI READ CAPACITY failed";
            return false;
        }
        current.mode_sense_ok = scsiModeSense6();
        if (current.sector_size != SECTOR_SIZE) {
            current.reason = "unsupported sector size";
            return false;
        }
        registerBlockDevice();
        current.reason = if (current.block_registered) "USB mass storage block device active" else "block register failed";
        return current.block_registered;
    }
    return false;
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

pub fn status() Status {
    current.bot_source = r4p.requiredSourceName("usb.msc_bot");
    current.bot_r4p_build = bot_r4p_build;
    current.bot_r4p_csw = bot_r4p_csw;
    current.bot_dispatch_failures = bot_dispatch_failures;
    current.bot_last_result = bot_last_result;
    current.scsi_source = r4p.requiredSourceName("usb.scsi_block");
    current.scsi_r4p_build = scsi_r4p_build;
    current.scsi_r4p_parse = scsi_r4p_parse;
    current.scsi_dispatch_failures = scsi_dispatch_failures;
    current.scsi_last_result = scsi_last_result;
    return current;
}

fn requireMscProtocolRoles() bool {
    if (mscProtocolRolesReady()) return true;
    current.protocol_required_missing +%= 1;
    current.reason = missingProtocolReason();
    return false;
}

fn mscProtocolRolesReady() bool {
    return r4p.hasActiveR4p("usb.msc_bot") and r4p.hasActiveR4p("usb.scsi_block");
}

fn missingProtocolReason() []const u8 {
    const bot_ready = r4p.hasActiveR4p("usb.msc_bot");
    const scsi_ready = r4p.hasActiveR4p("usb.scsi_block");
    if (!bot_ready and !scsi_ready) return "USBBOT.R4P and USBSCSI.R4P required";
    if (!bot_ready) return "USBBOT.R4P required";
    return "USBSCSI.R4P required";
}

pub fn blockDeviceCount() usize {
    return if (current.block_registered) 1 else 0;
}

pub fn deviceIndex() ?usize {
    return current.block_index;
}

pub fn reselectActiveDevice() bool {
    if (!current.block_registered) return false;
    if (!xhci.acquireControllerOwnership()) return false;
    defer xhci.releaseControllerOwnership();
    return ensureSelected();
}

fn registerBlockDevice() void {
    const index = block.register(.{
        .name = "usb0",
        .driver = "USBMSC",
        .bus = .usb,
        .controller = "xhci",
        .port = current.port,
        .sector_size = SECTOR_SIZE,
        .sector_count = current.sector_count,
        .max_sectors_per_request = MAX_SECTORS_PER_REQUEST,
        .queue_depth = 1,
        .removable = true,
        .writable = !current.write_protected,
        .owns_transport_retry = true,
        .source = block_source,
        .ctx = null,
        .read_fn = readBlock,
        .write_fn = if (current.write_protected) null else writeBlock,
        .flush_fn = flushBlock,
        .state = .active,
    }) orelse return;
    current.block_index = index;
    current.block_registered = true;
}

fn readBlock(_: ?*anyopaque, lba: u64, sectors: u16, out: []u8) bool {
    if (!xhci.acquireControllerOwnership()) {
        current.read_failures += 1;
        return false;
    }
    defer xhci.releaseControllerOwnership();
    if (sectors == 0 or sectors > MAX_SECTORS_PER_REQUEST) {
        current.read_failures += 1;
        return false;
    }
    if (out.len < @as(usize, sectors) * SECTOR_SIZE) {
        current.read_failures += 1;
        return false;
    }
    if (lba > 0xFFFF_FFFF) {
        current.read_failures += 1;
        return false;
    }
    if (!ensureSelected()) {
        current.read_failures += 1;
        return false;
    }
    const len = @as(usize, sectors) * SECTOR_SIZE;
    if (!scsiRead10(@intCast(lba), sectors, out[0..len])) {
        // A completed BOT reset plus xHCI endpoint recovery restores the
        // transport, but the failed READ itself has not happened again.
        // Retry that idempotent read exactly once.  This is especially
        // important during boot, where one transient USB timeout otherwise
        // makes the shell image unavailable and turns into a fatal screen.
        if (retry_policy.shouldRetryTransport(
            current.last_failure_transport,
            current.last_recovery_ok,
            0,
        )) {
            current.read_transport_retries += 1;
            usb_wait.milliseconds(usb_timing.SCSI_RETRY_DELAY_MS);
            if (ensureSelected() and scsiRead10(@intCast(lba), sectors, out[0..len])) {
                current.read_transport_retry_successes += 1;
                current.reads += 1;
                // command() has already resolved the generation-bound direct
                // diagnostic incident after receiving a valid CSW.  Logging
                // this success through diag_screen would create a new,
                // non-resolvable blue incident after recovery.
                k.puts("[USBMSC] READ10 transport retry recovered\n");
                return true;
            }
        }
        if (!current.last_failure_transport) _ = scsiRequestSense();
        current.read_failures += 1;
        return false;
    }
    current.reads += 1;
    return true;
}

fn writeBlock(_: ?*anyopaque, lba: u64, sectors: u16, data: []const u8) bool {
    if (!xhci.acquireControllerOwnership()) {
        current.write_failures += 1;
        return false;
    }
    defer xhci.releaseControllerOwnership();
    if (sectors == 0 or sectors > MAX_SECTORS_PER_REQUEST) {
        current.write_failures += 1;
        return false;
    }
    if (data.len < @as(usize, sectors) * SECTOR_SIZE) {
        current.write_failures += 1;
        return false;
    }
    if (lba > 0xFFFF_FFFF) {
        current.write_failures += 1;
        return false;
    }
    if (!ensureSelected()) {
        current.write_failures += 1;
        return false;
    }
    const len = @as(usize, sectors) * SECTOR_SIZE;
    if (!scsiWrite10(@intCast(lba), sectors, data[0..len])) {
        // Reissuing the exact same sector range with the same bytes is
        // idempotent even if the original CSW was lost after the device had
        // accepted some or all data. Recovery only repairs BOT/xHCI; replay
        // the failed WRITE10 once after that recovery actually succeeded.
        if (retry_policy.shouldRetryTransport(
            current.last_failure_transport,
            current.last_recovery_ok,
            0,
        )) {
            current.write_transport_retries += 1;
            usb_wait.milliseconds(usb_timing.SCSI_RETRY_DELAY_MS);
            if (ensureSelected() and scsiWrite10(@intCast(lba), sectors, data[0..len])) {
                current.write_transport_retry_successes += 1;
                current.writes += 1;
                k.puts("[USBMSC] WRITE10 transport retry recovered\n");
                return true;
            }
        }
        if (!current.last_failure_transport) _ = scsiRequestSense();
        current.write_failures += 1;
        return false;
    }
    current.writes += 1;
    return true;
}

fn flushBlock(_: ?*anyopaque) bool {
    if (!xhci.acquireControllerOwnership()) {
        current.flush_failures += 1;
        return false;
    }
    defer xhci.releaseControllerOwnership();
    if (!ensureSelected()) {
        current.flush_failures += 1;
        return false;
    }
    if (!scsiSynchronizeCache10()) {
        // SYNCHRONIZE CACHE is idempotent. As with READ/WRITE, a completed
        // transport recovery has not replayed the command itself.
        if (retry_policy.shouldRetryTransport(
            current.last_failure_transport,
            current.last_recovery_ok,
            0,
        )) {
            current.flush_transport_retries += 1;
            usb_wait.milliseconds(usb_timing.SCSI_RETRY_DELAY_MS);
            if (ensureSelected() and scsiSynchronizeCache10()) {
                current.flush_transport_retry_successes += 1;
                current.flushes += 1;
                k.puts("[USBMSC] SYNC-CACHE transport retry recovered\n");
                return true;
            }
        }
        if (!current.last_failure_transport) {
            const sense_ok = scsiRequestSense();
            if (sense_ok and
                current.sense_valid and
                current.sense_for_opcode == 0x35 and
                current.sense_key == 0x05)
            {
                current.flush_unsupported += 1;
                current.flushes += 1;
                return true;
            }
        }
        current.flush_failures += 1;
        return false;
    }
    current.flushes += 1;
    return true;
}

fn ensureSelected() bool {
    if (current.transport_faulted) {
        current.failures += 1;
        current.reason = "USBMSC transport recovery incomplete";
        return false;
    }
    const xs = xhci.status();
    const exact_runtime = current.device.slot_id != 0 and
        xs.addressed_slot_id == current.device.slot_id and
        xs.addressed_port == current.device.port;
    if (exact_runtime and
        (xs.bulk_endpoints_faulted or xs.control_endpoint_faulted))
    {
        current.failures += 1;
        current.reason = "USBMSC xHCI endpoint faulted";
        return false;
    }
    if (!exact_runtime or !xs.bulk_endpoints_configured) {
        if (!xhci.selectDeviceHandle(&current.device)) {
            current.failures += 1;
            current.reason = "USBMSC device select failed";
            return false;
        }
        const selected = xhci.status();
        if (selected.bulk_endpoints_faulted or selected.control_endpoint_faulted) {
            current.failures += 1;
            current.reason = "USBMSC xHCI endpoint faulted";
            return false;
        }
        if (selected.addressed_slot_id == current.device.slot_id and
            selected.addressed_port == current.device.port and
            selected.bulk_endpoints_configured)
            return true;
        if (!xhci.setConfigurationForHandle(&current.device)) {
            current.failures += 1;
            current.reason = "SET_CONFIGURATION failed";
            return false;
        }
        if (!xhci.configureBulkEndpointHandles(&current.bulk_in_handle, &current.bulk_out_handle)) {
            current.failures += 1;
            current.reason = "bulk endpoint configure failed";
            return false;
        }
    }
    return true;
}

fn scsiInquiry() bool {
    const cmd = buildScsiCommand(r4p_contract.USB_SCSI_OP_BUILD_INQUIRY, 0, 0) orelse {
        current.reason = "SCSI INQUIRY command build failed";
        return failCommand();
    };
    if (!command(cmd.cdb[0..cmd.len], cmd.direction, cmd.transfer_len, data_buf[0..cmd.transfer_len])) return false;
    const actual: usize = @intCast(current.last_data_actual_len);
    if (actual < 5) {
        setDataLengthReason("inquiry-short", current.last_data_actual_len);
        return failCommand();
    }
    const declared = @min(@as(usize, cmd.transfer_len), @as(usize, data_buf[4]) + 5);
    if (actual < declared) {
        setDataLengthReason("inquiry-truncated", current.last_data_actual_len);
        return failCommand();
    }
    return true;
}

fn scsiInquiryWithRetry() bool {
    var attempt: u8 = 0;
    while (attempt < usb_timing.SCSI_INQUIRY_ATTEMPTS) : (attempt += 1) {
        if (scsiInquiry()) return true;
        if (attempt + 1 >= usb_timing.SCSI_INQUIRY_ATTEMPTS) break;
        current.inquiry_retries += 1;
        usb_wait.milliseconds(usb_timing.SCSI_RETRY_DELAY_MS);
    }
    return false;
}

fn scsiTestUnitReady() bool {
    const cmd = buildScsiCommand(r4p_contract.USB_SCSI_OP_BUILD_TEST_UNIT_READY, 0, 0) orelse return failCommand();
    return command(cmd.cdb[0..cmd.len], cmd.direction, cmd.transfer_len, data_buf[0..cmd.transfer_len]);
}

fn waitForScsiReady() bool {
    var attempt: u8 = 0;
    while (attempt < usb_timing.SCSI_READY_ATTEMPTS) : (attempt += 1) {
        if (scsiTestUnitReady()) return true;
        if (!current.last_failure_transport) _ = scsiRequestSense();
        if (attempt + 1 >= usb_timing.SCSI_READY_ATTEMPTS) break;
        current.test_unit_ready_retries += 1;
        usb_wait.milliseconds(usb_timing.SCSI_RETRY_DELAY_MS);
    }
    return false;
}

fn scsiRequestSense() bool {
    const failed_opcode = current.last_opcode;
    invalidateSense();
    const cmd = buildScsiCommand(r4p_contract.USB_SCSI_OP_BUILD_REQUEST_SENSE, 0, 0) orelse return failCommand();
    current.sense_requests += 1;
    if (!command(cmd.cdb[0..cmd.len], cmd.direction, cmd.transfer_len, data_buf[0..cmd.transfer_len])) {
        current.last_opcode = failed_opcode;
        return false;
    }
    current.last_opcode = failed_opcode;
    const actual: usize = @intCast(current.last_data_actual_len);
    if (!parseScsiSense(data_buf[0..actual], failed_opcode)) {
        setDataLengthReason("sense-short-or-invalid", current.last_data_actual_len);
        return failCommand();
    }
    return true;
}

fn invalidateSense() void {
    current.sense_valid = false;
    current.sense_for_opcode = 0;
    current.sense_key = 0;
    current.sense_asc = 0;
    current.sense_ascq = 0;
}

fn classifySense() void {
    switch (current.sense_key) {
        0x02 => current.sense_not_ready += 1,
        0x05 => current.sense_illegal_request += 1,
        0x06 => current.sense_unit_attention += 1,
        0x07 => current.sense_write_protect += 1,
        else => {},
    }
}

fn scsiReadCapacity10() bool {
    const cmd = buildScsiCommand(r4p_contract.USB_SCSI_OP_BUILD_READ_CAPACITY10, 0, 0) orelse return failCommand();
    if (!command(cmd.cdb[0..cmd.len], cmd.direction, cmd.transfer_len, data_buf[0..cmd.transfer_len])) return false;
    const actual: usize = @intCast(current.last_data_actual_len);
    if (!parseScsiCapacity(data_buf[0..actual])) {
        setDataLengthReason("capacity-short-or-invalid", current.last_data_actual_len);
        return failCommand();
    }
    return true;
}

fn scsiReadCapacityWithRetry() bool {
    var attempt: u8 = 0;
    while (attempt < usb_timing.SCSI_CAPACITY_ATTEMPTS) : (attempt += 1) {
        if (scsiReadCapacity10()) return true;
        if (!current.last_failure_transport) _ = scsiRequestSense();
        if (attempt + 1 >= usb_timing.SCSI_CAPACITY_ATTEMPTS) break;
        current.read_capacity_retries += 1;
        usb_wait.milliseconds(usb_timing.SCSI_RETRY_DELAY_MS);
    }
    return false;
}

fn scsiModeSense6() bool {
    const cmd = buildScsiCommand(r4p_contract.USB_SCSI_OP_BUILD_MODE_SENSE6, 0, 0) orelse return failCommand();
    if (!command(cmd.cdb[0..cmd.len], cmd.direction, cmd.transfer_len, data_buf[0..cmd.transfer_len])) return false;
    const actual: usize = @intCast(current.last_data_actual_len);
    if (!parseScsiModeSense(data_buf[0..actual])) {
        setDataLengthReason("mode-sense-short-or-invalid", current.last_data_actual_len);
        return failCommand();
    }
    return true;
}

fn scsiRead10(lba: u32, sectors: u16, out: []u8) bool {
    const cmd = buildScsiCommand(r4p_contract.USB_SCSI_OP_BUILD_READ10, lba, sectors) orelse return failCommand();
    return command(cmd.cdb[0..cmd.len], cmd.direction, @intCast(out.len), out);
}

fn scsiWrite10(lba: u32, sectors: u16, data: []const u8) bool {
    const cmd = buildScsiCommand(r4p_contract.USB_SCSI_OP_BUILD_WRITE10, lba, sectors) orelse return failCommand();
    return command(cmd.cdb[0..cmd.len], cmd.direction, @intCast(data.len), @constCast(data));
}

fn scsiSynchronizeCache10() bool {
    const cmd = buildScsiCommand(r4p_contract.USB_SCSI_OP_BUILD_SYNC_CACHE10, 0, 0) orelse return failCommand();
    return command(cmd.cdb[0..cmd.len], cmd.direction, cmd.transfer_len, data_buf[0..cmd.transfer_len]);
}

fn buildScsiCommand(opcode: u32, lba: u32, sectors: u16) ?ScsiCommand {
    return buildScsiCommandR4p(opcode, lba, sectors);
}

fn buildScsiCommandR4p(opcode: u32, lba: u32, sectors: u16) ?ScsiCommand {
    if (!r4p.hasActiveR4p("usb.scsi_block")) return null;
    var op: r4p_contract.UsbScsiBlockOp = .{
        .lba = lba,
        .sectors = sectors,
    };
    if (!dispatchScsi(opcode, &op)) return null;
    if (op.result != r4p_contract.USB_SCSI_RESULT_OK or op.cdb_len == 0 or op.cdb_len > 16) return null;
    var cmd: ScsiCommand = .{
        .len = op.cdb_len,
        .direction = switch (op.direction) {
            r4p_contract.USB_SCSI_DIR_IN => .in,
            r4p_contract.USB_SCSI_DIR_OUT => .out,
            else => .none,
        },
        .transfer_len = op.transfer_len,
    };
    @memcpy(cmd.cdb[0..op.cdb_len], op.cdb[0..op.cdb_len]);
    scsi_r4p_build +%= 1;
    return cmd;
}

fn parseScsiSense(data: []const u8, failed_opcode: u8) bool {
    return parseScsiSenseR4p(data, failed_opcode);
}

fn parseScsiSenseR4p(data: []const u8, failed_opcode: u8) bool {
    if (!r4p.hasActiveR4p("usb.scsi_block") or data.len > r4p_contract.USB_SCSI_MAX_DATA) return false;
    var op: r4p_contract.UsbScsiBlockOp = .{
        .allocation_len = @intCast(data.len),
        .failed_opcode = failed_opcode,
    };
    @memcpy(op.data[0..data.len], data);
    if (!dispatchScsi(r4p_contract.USB_SCSI_OP_PARSE_SENSE, &op) or op.result != r4p_contract.USB_SCSI_RESULT_OK) return false;
    current.sense_valid = true;
    current.sense_for_opcode = failed_opcode;
    current.sense_key = op.sense_key;
    current.sense_asc = op.sense_asc;
    current.sense_ascq = op.sense_ascq;
    classifySense();
    if (current.block_index) |idx| {
        block.recordSense(idx, current.sense_for_opcode, current.sense_key, current.sense_asc, current.sense_ascq);
    }
    scsi_r4p_parse +%= 1;
    return true;
}

fn parseScsiCapacity(data: []const u8) bool {
    return parseScsiCapacityR4p(data);
}

fn parseScsiCapacityR4p(data: []const u8) bool {
    if (!r4p.hasActiveR4p("usb.scsi_block") or data.len > r4p_contract.USB_SCSI_MAX_DATA) return false;
    var op: r4p_contract.UsbScsiBlockOp = .{ .allocation_len = @intCast(data.len) };
    @memcpy(op.data[0..data.len], data);
    if (!dispatchScsi(r4p_contract.USB_SCSI_OP_PARSE_CAPACITY10, &op) or op.result != r4p_contract.USB_SCSI_RESULT_OK) return false;
    current.sector_count = op.sector_count;
    current.sector_size = op.sector_size;
    scsi_r4p_parse +%= 1;
    return true;
}

fn parseScsiModeSense(data: []const u8) bool {
    return parseScsiModeSenseR4p(data);
}

fn parseScsiModeSenseR4p(data: []const u8) bool {
    if (!r4p.hasActiveR4p("usb.scsi_block") or data.len > r4p_contract.USB_SCSI_MAX_DATA) return false;
    var op: r4p_contract.UsbScsiBlockOp = .{ .allocation_len = @intCast(data.len) };
    @memcpy(op.data[0..data.len], data);
    if (!dispatchScsi(r4p_contract.USB_SCSI_OP_PARSE_MODE_SENSE6, &op) or op.result != r4p_contract.USB_SCSI_RESULT_OK) return false;
    current.write_protected_known = op.write_protected_known != 0;
    current.write_protected = op.write_protected != 0;
    scsi_r4p_parse +%= 1;
    return true;
}

fn dispatchScsi(opcode: u32, op: *r4p_contract.UsbScsiBlockOp) bool {
    var buffer = protocol_api.ProtocolBuffer{
        .data = op,
        .len = @sizeOf(r4p_contract.UsbScsiBlockOp),
        .capacity = @sizeOf(r4p_contract.UsbScsiBlockOp),
        .flags = 0,
        .reserved = 0,
    };
    var out: protocol_api.ProtocolBuffer = .{};
    const result = r4p.dispatch("usb.scsi_block", opcode, &buffer, &out);
    scsi_last_result = result;
    if (result != r4p_contract.USB_SCSI_RESULT_OK) {
        scsi_dispatch_failures +%= 1;
        return false;
    }
    return true;
}

fn command(cdb: []const u8, direction: Direction, transfer_len: u32, buffer: []u8) bool {
    // The complete CBW/data/CSW attempt plus its bounded BOT/xHCI recovery is
    // one diagnostic lifetime. Retained evidence and framebuffer pixels stay
    // intact after resolve, while no failed command can orphan the generation
    // merely because its caller decides not to retry.
    defer resolveTransportIncident();
    current.last_failure_transport = false;
    current.last_recovery_ok = false;
    if (current.transport_faulted) {
        current.last_failure_transport = true;
        current.reason = "USBMSC transport recovery incomplete";
        return false;
    }
    if (cdb.len == 0 or cdb.len > 16) {
        current.reason = "SCSI invalid CDB length";
        return failCommand();
    }
    if (@as(usize, transfer_len) > buffer.len) {
        current.reason = "SCSI transfer buffer too small";
        return failCommand();
    }
    if (cdb[0] != 0x03) invalidateSense();
    var cbw: [CBW_LEN]u8 = .{0} ** CBW_LEN;
    const tag = nextTag();
    current.last_opcode = cdb[0];
    current.last_transfer_len = transfer_len;
    current.last_data_actual_len = 0;
    current.last_csw_tag = 0;
    current.last_csw_residue = 0;
    current.last_csw_status = 0xFF;
    if (buildCbw(cdb, direction, transfer_len, tag, cbw[0..]) == null) {
        current.reason = "SCSI BOT CBW build failed";
        return failCommand();
    }

    if (!xhci.bulkOutForHandle(&current.bulk_out_handle, cbw[0..])) return failTransportAt("cbw-out");
    var data_actual_len: u32 = 0;
    if (transfer_len != 0) {
        const data = buffer[0..@intCast(transfer_len)];
        switch (direction) {
            .in => {
                if (!xhci.bulkInForHandle(&current.bulk_in_handle, data)) return failTransportAt("data-in");
                data_actual_len = xhci.status().last_bulk_actual_len;
            },
            .out => {
                if (!xhci.bulkOutForHandle(&current.bulk_out_handle, data)) return failTransportAt("data-out");
                data_actual_len = xhci.status().last_bulk_actual_len;
            },
            .none => {
                current.reason = "SCSI data direction missing";
                return failCommand();
            },
        }
        current.last_data_actual_len = data_actual_len;
    }
    var csw: [CSW_LEN]u8 = .{0} ** CSW_LEN;
    if (!xhci.bulkInForHandle(&current.bulk_in_handle, csw[0..])) return failTransportAt("csw-in");
    const csw_transfer = xhci.status();
    if (csw_transfer.last_bulk_actual_len != CSW_LEN) return failTransportAt("csw-short");
    const csw_result = parseCsw(csw[0..], tag, transfer_len);
    current.commands += 1;

    // For an IN phase, xHCI's actual byte count and BOT's dCSWDataResidue
    // describe the same transfer from opposite sides.  Never accept a CSW
    // that would turn an xHCI Short Packet into apparently valid stale tail
    // bytes in the caller's page buffer.
    if (direction == .in and
        transfer_len != 0 and
        (csw_result == r4p_contract.USB_MSC_BOT_RESULT_OK or
            csw_result == r4p_contract.USB_MSC_BOT_RESULT_COMMAND_FAILED))
    {
        if (data_actual_len > transfer_len or
            transfer_len - data_actual_len != current.last_csw_residue)
        {
            current.csw_residue_errors += 1;
            setDataLengthReason("data-residue-mismatch", data_actual_len);
            return failTransport();
        }
    }

    // READ(10)/WRITE(10) have a fixed block count.  A successful CSW with a
    // non-zero residue (or a short USB IN) is not a successful block I/O.
    if (csw_result == r4p_contract.USB_MSC_BOT_RESULT_OK and
        (current.last_opcode == 0x28 or current.last_opcode == 0x2A) and
        (data_actual_len != transfer_len or current.last_csw_residue != 0))
    {
        current.short_data_errors += 1;
        setDataLengthReason("short-fixed-data", data_actual_len);
        return failTransport();
    }

    switch (csw_result) {
        r4p_contract.USB_MSC_BOT_RESULT_OK => {
            resolveTransportIncident();
            return true;
        },
        r4p_contract.USB_MSC_BOT_RESULT_COMMAND_FAILED => {
            // A valid command-failed CSW still proves that the BOT transport
            // has completed a full CBW/data/CSW exchange.
            resolveTransportIncident();
            setProtocolReason("command-failed");
            return failCommand();
        },
        r4p_contract.USB_MSC_BOT_RESULT_BAD_CSW => {
            current.invalid_csw += 1;
            setProtocolReason("bad-csw");
            return failTransport();
        },
        r4p_contract.USB_MSC_BOT_RESULT_TAG_MISMATCH => {
            current.csw_tag_mismatches += 1;
            setProtocolReason("tag-mismatch");
            return failTransport();
        },
        r4p_contract.USB_MSC_BOT_RESULT_RESIDUE => {
            current.csw_residue_errors += 1;
            setProtocolReason("invalid-residue");
            return failTransport();
        },
        r4p_contract.USB_MSC_BOT_RESULT_PHASE_ERROR => {
            current.csw_phase_errors += 1;
            setProtocolReason("phase-error");
            return failTransport();
        },
        else => {
            current.invalid_csw += 1;
            setProtocolReason("unsupported-csw");
            return failTransport();
        },
    }
}

fn buildCbw(cdb: []const u8, direction: Direction, transfer_len: u32, tag: u32, out: []u8) ?void {
    if (buildCbwR4p(cdb, direction, transfer_len, tag, out)) return {};
    return null;
}

fn buildCbwR4p(cdb: []const u8, direction: Direction, transfer_len: u32, tag: u32, out: []u8) bool {
    if (!r4p.hasActiveR4p("usb.msc_bot") or cdb.len > r4p_contract.USB_MSC_BOT_MAX_CDB or out.len < CBW_LEN) return false;
    var op: r4p_contract.UsbMscBotOp = .{
        .tag = tag,
        .transfer_len = transfer_len,
        .direction = switch (direction) {
            .none => r4p_contract.USB_MSC_BOT_DIR_NONE,
            .in => r4p_contract.USB_MSC_BOT_DIR_IN,
            .out => r4p_contract.USB_MSC_BOT_DIR_OUT,
        },
        .cdb_len = @intCast(cdb.len),
    };
    if (cdb.len != 0) @memcpy(op.cdb[0..cdb.len], cdb);
    if (!dispatchBot(r4p_contract.USB_MSC_BOT_OP_BUILD_CBW, &op)) return false;
    if (op.result != r4p_contract.USB_MSC_BOT_RESULT_OK) return false;
    @memcpy(out[0..CBW_LEN], op.cbw[0..CBW_LEN]);
    bot_r4p_build +%= 1;
    return true;
}

fn parseCsw(csw: []const u8, tag: u32, transfer_len: u32) i32 {
    if (parseCswR4p(csw, tag, transfer_len)) |result| return result;
    return r4p_contract.USB_MSC_BOT_RESULT_BAD_CSW;
}

fn parseCswR4p(csw: []const u8, tag: u32, transfer_len: u32) ?i32 {
    if (!r4p.hasActiveR4p("usb.msc_bot") or csw.len < CSW_LEN) return null;
    var op: r4p_contract.UsbMscBotOp = .{
        .tag = tag,
        .transfer_len = transfer_len,
    };
    @memcpy(op.csw[0..CSW_LEN], csw[0..CSW_LEN]);
    if (!dispatchBot(r4p_contract.USB_MSC_BOT_OP_PARSE_CSW, &op)) return null;
    current.last_csw_tag = op.csw_tag;
    current.last_csw_residue = op.residue;
    current.last_csw_status = op.status;
    bot_r4p_csw +%= 1;
    return op.result;
}

fn dispatchBot(opcode: u32, op: *r4p_contract.UsbMscBotOp) bool {
    var buffer = protocol_api.ProtocolBuffer{
        .data = op,
        .len = @sizeOf(r4p_contract.UsbMscBotOp),
        .capacity = @sizeOf(r4p_contract.UsbMscBotOp),
        .flags = 0,
        .reserved = 0,
    };
    var out: protocol_api.ProtocolBuffer = .{};
    const result = r4p.dispatch("usb.msc_bot", opcode, &buffer, &out);
    bot_last_result = result;
    if (result != r4p_contract.USB_MSC_BOT_RESULT_OK and result != r4p_contract.USB_MSC_BOT_RESULT_COMMAND_FAILED) {
        bot_dispatch_failures +%= 1;
        return false;
    }
    return true;
}

fn failCommand() bool {
    current.last_failure_transport = false;
    current.last_recovery_ok = false;
    current.failures += 1;
    current.command_failures += 1;
    return false;
}

fn failTransport() bool {
    current.last_failure_transport = true;
    current.transport_faulted = true;
    current.failures += 1;
    current.transport_failures += 1;
    const transfer_incident = xhci.takeLastSyncTransferIncidentToken();
    if (transfer_incident.valid()) {
        if (last_transport_incident.valid() and
            last_transport_incident.generation != transfer_incident.generation)
        {
            _ = diag_screen.resolveIncident(last_transport_incident);
        }
        last_transport_incident = transfer_incident;
    }
    if (!last_transport_incident.valid()) {
        last_transport_incident = diag_screen.beginResolvableIncident();
    }
    // Publish the immutable root diagnosis before STOP/RESET/CLEAR_FEATURE
    // can itself timeout or overwrite the most useful transport state.
    diag_screen.write("[USBMSC] transport root op=");
    diag_screen.write(opcodeName(current.last_opcode));
    diag_screen.write(" reason=");
    diag_screen.write(current.reason);
    diag_screen.write(" csw=");
    diag_screen.writeDec(current.last_csw_status);
    diag_screen.write(" residue=");
    diag_screen.writeDec(current.last_csw_residue);
    diag_screen.write(" bytes=");
    diag_screen.writeDec(current.last_data_actual_len);
    diag_screen.write("/");
    diag_screen.writeDec(current.last_transfer_len);
    diag_screen.endLine();
    current.last_recovery_ok = recoverTransport();
    diag_screen.write("[USBMSC] transport recovery=");
    diag_screen.write(if (current.last_recovery_ok) "ok" else "failed");
    diag_screen.endLine();
    return false;
}

fn failTransportAt(stage: []const u8) bool {
    setTransportReason(stage);
    return failTransport();
}

fn resolveTransportIncident() void {
    _ = diag_screen.resolveIncident(last_transport_incident);
    last_transport_incident = .{};
}

fn opcodeName(opcode: u8) []const u8 {
    return switch (opcode) {
        0x00 => "TUR",
        0x03 => "REQUEST-SENSE",
        0x12 => "INQUIRY",
        0x1A => "MODE-SENSE",
        0x25 => "READ-CAPACITY",
        0x28 => "READ10",
        0x2A => "WRITE10",
        0x35 => "SYNC-CACHE",
        else => "OP",
    };
}

fn appendReason(cursor: *usize, text: []const u8) void {
    const available = reason_buffer.len - @min(cursor.*, reason_buffer.len);
    const count = @min(available, text.len);
    if (count != 0) @memcpy(reason_buffer[cursor.* .. cursor.* + count], text[0..count]);
    cursor.* += count;
}

fn appendReasonU32(cursor: *usize, value_input: u32) void {
    var digits: [10]u8 = undefined;
    var value = value_input;
    var count: usize = 0;
    while (true) {
        digits[count] = @intCast('0' + value % 10);
        count += 1;
        value /= 10;
        if (value == 0) break;
    }
    while (count != 0) {
        count -= 1;
        appendReason(cursor, digits[count .. count + 1]);
    }
}

fn publishReason(cursor: usize) void {
    current.reason = reason_buffer[0..@min(cursor, reason_buffer.len)];
}

fn beginProtocolReason() usize {
    @memset(reason_buffer[0..], 0);
    var cursor: usize = 0;
    appendReason(&cursor, "SCSI ");
    appendReason(&cursor, opcodeName(current.last_opcode));
    appendReason(&cursor, " ");
    return cursor;
}

fn setProtocolReason(detail: []const u8) void {
    var cursor = beginProtocolReason();
    appendReason(&cursor, detail);
    publishReason(cursor);
}

fn setTransportReason(stage: []const u8) void {
    const xs = xhci.status();
    var cursor = beginProtocolReason();
    appendReason(&cursor, stage);
    appendReason(&cursor, " xHCI=");
    appendReason(&cursor, xs.last_bulk_result);
    appendReason(&cursor, " cc=");
    appendReasonU32(&cursor, xs.last_bulk_completion_code);
    appendReason(&cursor, " bytes=");
    appendReasonU32(&cursor, xs.last_bulk_actual_len);
    appendReason(&cursor, "/");
    appendReasonU32(&cursor, xs.last_bulk_request_len);
    publishReason(cursor);
}

fn setDataLengthReason(detail: []const u8, actual_len: u32) void {
    var cursor = beginProtocolReason();
    appendReason(&cursor, detail);
    appendReason(&cursor, " bytes=");
    appendReasonU32(&cursor, actual_len);
    appendReason(&cursor, "/");
    appendReasonU32(&cursor, current.last_transfer_len);
    appendReason(&cursor, " residue=");
    appendReasonU32(&cursor, current.last_csw_residue);
    publishReason(cursor);
}

fn recoverTransport() bool {
    current.recoveries += 1;
    // Admission stays closed from the first ambiguous transport result until
    // BOT reset, both device-side ClearFeature requests, and the host-side
    // Drop+Add Configure Endpoint have all completed successfully.
    current.transport_faulted = true;
    if (current.block_index) |idx| block.beginBackendRecovery(idx);
    var ok = false;
    defer {
        if (!ok) current.recovery_failures += 1;
        if (current.block_index) |idx| block.finishBackendRecovery(idx, ok);
    }
    var recovery_budget = usb_wait.Deadline.begin(usb_timing.MSC_RECOVERY_BUDGET_MS);
    defer recovery_budget.finish();

    // First quiesce and skip every timed-out xHC TD. A running data-out TD
    // must not still reach the device after BOT reset and contaminate the
    // first command of the recovered BOT session.
    if (current.bulk_in != 0 and
        !xhci.resetEndpointStateForHandleWithin(&current.device, current.bulk_in, &recovery_budget))
        return false;
    if (current.bulk_out != 0 and
        !xhci.resetEndpointStateForHandleWithin(&current.device, current.bulk_out, &recovery_budget))
        return false;

    // USB Mass Storage Bulk-Only Transport reset recovery (BOT 6.3.1):
    // class reset, then clear the device-side halts. The host-side endpoint
    // rings above are already stopped and advanced to producer+DCS.
    if (xhci.resetMassStorageInterfaceForHandleWithin(
        &current.device,
        current.interface_number,
        &recovery_budget,
    )) {
        current.resets += 1;
    } else {
        return false;
    }
    if (current.bulk_in != 0) {
        if (xhci.clearEndpointHaltForHandleWithin(
            &current.device,
            current.bulk_in,
            &recovery_budget,
        )) {
            current.clear_halts += 1;
        } else {
            return false;
        }
    }
    if (current.bulk_out != 0) {
        if (xhci.clearEndpointHaltForHandleWithin(
            &current.device,
            current.bulk_out,
            &recovery_budget,
        )) {
            current.clear_halts += 1;
        } else {
            return false;
        }
    }
    if (current.bulk_in == 0 or
        current.bulk_out == 0 or
        !xhci.reconfigureMassStorageBulkEndpointsForHandleWithin(
            &current.device,
            current.bulk_in,
            current.bulk_out,
            &recovery_budget,
        ))
    {
        return false;
    }
    current.transport_faulted = false;
    ok = true;
    return true;
}

fn nextTag() u32 {
    current.tags +%= 1;
    if (current.tags == 0) current.tags = 1;
    return current.tags;
}
