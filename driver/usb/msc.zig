// Native USBMSC owner. Existing Zig block callbacks retain their ABI;
// these typed trampolines store no medium/transport state.
const xhci = @import("xhci.zig");
const block = @import("../../storage/block.zig");
const diag_screen = @import("../../kernel/diag_screen.zig");
const Text = @import("core.zig").Text;
const DeviceHandle = xhci.DeviceHandle;
const EndpointHandle = xhci.EndpointHandle;
const IncidentToken = extern struct { generation: u64 = 0 };
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
    max_sectors_per_request: u16 = 0,
    capacity_format: u8 = 0,
    capacity16_used: bool = false,
    read10_commands: u64 = 0,
    read16_commands: u64 = 0,
    write10_commands: u64 = 0,
    write16_commands: u64 = 0,
    transport: []const u8 = "BOT",
    lun: u8 = 0,
    max_lun: u8 = 0,
    uas_supported: bool = false,
    inquiry_ok: bool = false,
    model: [32]u8 = .{0} ** 32,
    model_len: usize = 0,
    inquiry_retries: u64 = 0,
    inquiry_wait_elapsed_ns: u64 = 0,
    test_unit_ready_ok: bool = false,
    test_unit_ready_retries: u64 = 0,
    ready_wait_elapsed_ns: u64 = 0,
    read_capacity_ok: bool = false,
    read_capacity_retries: u64 = 0,
    capacity_wait_elapsed_ns: u64 = 0,
    retry_delay_calls: u64 = 0,
    retry_budget_timeouts: u64 = 0,
    transient_sense_retries: u64 = 0,
    unknown_sense_retries: u64 = 0,
    permanent_sense_stops: u64 = 0,
    last_retry_opcode: u8 = 0,
    last_retry_number: u8 = 0,
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
    media_changed: bool = false,
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
    reason_len: usize = 0,
    reason_storage: [128]u8 = .{0} ** 128,
    transport_incident: diag_screen.IncidentToken = .{},
    pub fn reasonText(self: *const Status) []const u8 {
        return self.reason_storage[0..self.reason_len];
    }
};
const NativeStatus = extern struct {
    initialized: bool,
    present: bool,
    bound: bool,
    block_registered: bool,
    block_index: usize,
    port: u8,
    device: DeviceHandle,
    bulk_in_handle: EndpointHandle,
    bulk_out_handle: EndpointHandle,
    interface_number: u8,
    bulk_in: u8,
    bulk_out: u8,
    bulk_in_max_packet: u16,
    bulk_out_max_packet: u16,
    sector_count: u64,
    sector_size: u32,
    max_sectors_per_request: u16,
    capacity_format: u8,
    capacity16_used: bool,
    read10_commands: u64,
    read16_commands: u64,
    write10_commands: u64,
    write16_commands: u64,
    transport: Text,
    lun: u8,
    max_lun: u8,
    uas_supported: bool,
    inquiry_ok: bool,
    model: [32]u8,
    model_len: usize,
    inquiry_retries: u64,
    inquiry_wait_elapsed_ns: u64,
    test_unit_ready_ok: bool,
    test_unit_ready_retries: u64,
    ready_wait_elapsed_ns: u64,
    read_capacity_ok: bool,
    read_capacity_retries: u64,
    capacity_wait_elapsed_ns: u64,
    retry_delay_calls: u64,
    retry_budget_timeouts: u64,
    transient_sense_retries: u64,
    unknown_sense_retries: u64,
    permanent_sense_stops: u64,
    last_retry_opcode: u8,
    last_retry_number: u8,
    mode_sense_ok: bool,
    write_protected_known: bool,
    write_protected: bool,
    tags: u32,
    commands: u64,
    failures: u64,
    reads: u64,
    read_failures: u64,
    read_transport_retries: u64,
    read_transport_retry_successes: u64,
    writes: u64,
    write_failures: u64,
    write_transport_retries: u64,
    write_transport_retry_successes: u64,
    flushes: u64,
    flush_failures: u64,
    flush_transport_retries: u64,
    flush_transport_retry_successes: u64,
    flush_unsupported: u64,
    recoveries: u64,
    recovery_failures: u64,
    resets: u64,
    clear_halts: u64,
    sense_requests: u64,
    sense_valid: bool,
    sense_for_opcode: u8,
    sense_key: u8,
    sense_asc: u8,
    sense_ascq: u8,
    sense_not_ready: u64,
    sense_unit_attention: u64,
    sense_illegal_request: u64,
    sense_write_protect: u64,
    command_failures: u64,
    transport_failures: u64,
    transport_faulted: bool,
    media_changed: bool,
    last_failure_transport: bool,
    last_recovery_ok: bool,
    invalid_csw: u64,
    csw_tag_mismatches: u64,
    csw_phase_errors: u64,
    csw_residue_errors: u64,
    short_data_errors: u64,
    last_opcode: u8,
    last_transfer_len: u32,
    last_data_actual_len: u32,
    last_csw_tag: u32,
    last_csw_residue: u32,
    last_csw_status: u8,
    bot_source: Text,
    bot_r4p_build: u64,
    bot_r4p_csw: u64,
    bot_dispatch_failures: u64,
    bot_last_result: i32,
    scsi_source: Text,
    scsi_r4p_build: u64,
    scsi_r4p_parse: u64,
    scsi_dispatch_failures: u64,
    scsi_last_result: i32,
    protocol_required_missing: u64,
    reason: Text,
    reason_storage: [128]u8,
    transport_incident: IncidentToken,
};
comptime {
    if (@sizeOf(NativeStatus) != 920 or @alignOf(NativeStatus) != 8) @compileError("USBMSC C status layout");
    if (@offsetOf(NativeStatus, "initialized") != 0) @compileError("USBMSC initialized C offset");
    if (@offsetOf(NativeStatus, "present") != 1) @compileError("USBMSC present C offset");
    if (@offsetOf(NativeStatus, "bound") != 2) @compileError("USBMSC bound C offset");
    if (@offsetOf(NativeStatus, "block_registered") != 3) @compileError("USBMSC block_registered C offset");
    if (@offsetOf(NativeStatus, "block_index") != 8) @compileError("USBMSC block_index C offset");
    if (@offsetOf(NativeStatus, "port") != 16) @compileError("USBMSC port C offset");
    if (@offsetOf(NativeStatus, "device") != 24) @compileError("USBMSC device C offset");
    if (@offsetOf(NativeStatus, "bulk_in_handle") != 56) @compileError("USBMSC bulk_in_handle C offset");
    if (@offsetOf(NativeStatus, "bulk_out_handle") != 96) @compileError("USBMSC bulk_out_handle C offset");
    if (@offsetOf(NativeStatus, "interface_number") != 136) @compileError("USBMSC interface_number C offset");
    if (@offsetOf(NativeStatus, "bulk_in") != 137) @compileError("USBMSC bulk_in C offset");
    if (@offsetOf(NativeStatus, "bulk_out") != 138) @compileError("USBMSC bulk_out C offset");
    if (@offsetOf(NativeStatus, "bulk_in_max_packet") != 140) @compileError("USBMSC bulk_in_max_packet C offset");
    if (@offsetOf(NativeStatus, "bulk_out_max_packet") != 142) @compileError("USBMSC bulk_out_max_packet C offset");
    if (@offsetOf(NativeStatus, "sector_count") != 144) @compileError("USBMSC sector_count C offset");
    if (@offsetOf(NativeStatus, "sector_size") != 152) @compileError("USBMSC sector_size C offset");
    if (@offsetOf(NativeStatus, "max_sectors_per_request") != 156) @compileError("USBMSC max_sectors_per_request C offset");
    if (@offsetOf(NativeStatus, "capacity_format") != 158) @compileError("USBMSC capacity_format C offset");
    if (@offsetOf(NativeStatus, "capacity16_used") != 159) @compileError("USBMSC capacity16_used C offset");
    if (@offsetOf(NativeStatus, "read10_commands") != 160) @compileError("USBMSC read10_commands C offset");
    if (@offsetOf(NativeStatus, "read16_commands") != 168) @compileError("USBMSC read16_commands C offset");
    if (@offsetOf(NativeStatus, "write10_commands") != 176) @compileError("USBMSC write10_commands C offset");
    if (@offsetOf(NativeStatus, "write16_commands") != 184) @compileError("USBMSC write16_commands C offset");
    if (@offsetOf(NativeStatus, "transport") != 192) @compileError("USBMSC transport C offset");
    if (@offsetOf(NativeStatus, "lun") != 208) @compileError("USBMSC lun C offset");
    if (@offsetOf(NativeStatus, "max_lun") != 209) @compileError("USBMSC max_lun C offset");
    if (@offsetOf(NativeStatus, "uas_supported") != 210) @compileError("USBMSC uas_supported C offset");
    if (@offsetOf(NativeStatus, "inquiry_ok") != 211) @compileError("USBMSC inquiry_ok C offset");
    if (@offsetOf(NativeStatus, "model") != 212) @compileError("USBMSC model C offset");
    if (@offsetOf(NativeStatus, "model_len") != 248) @compileError("USBMSC model_len C offset");
    if (@offsetOf(NativeStatus, "inquiry_retries") != 256) @compileError("USBMSC inquiry_retries C offset");
    if (@offsetOf(NativeStatus, "inquiry_wait_elapsed_ns") != 264) @compileError("USBMSC inquiry_wait_elapsed_ns C offset");
    if (@offsetOf(NativeStatus, "test_unit_ready_ok") != 272) @compileError("USBMSC test_unit_ready_ok C offset");
    if (@offsetOf(NativeStatus, "test_unit_ready_retries") != 280) @compileError("USBMSC test_unit_ready_retries C offset");
    if (@offsetOf(NativeStatus, "ready_wait_elapsed_ns") != 288) @compileError("USBMSC ready_wait_elapsed_ns C offset");
    if (@offsetOf(NativeStatus, "read_capacity_ok") != 296) @compileError("USBMSC read_capacity_ok C offset");
    if (@offsetOf(NativeStatus, "read_capacity_retries") != 304) @compileError("USBMSC read_capacity_retries C offset");
    if (@offsetOf(NativeStatus, "capacity_wait_elapsed_ns") != 312) @compileError("USBMSC capacity_wait_elapsed_ns C offset");
    if (@offsetOf(NativeStatus, "retry_delay_calls") != 320) @compileError("USBMSC retry_delay_calls C offset");
    if (@offsetOf(NativeStatus, "retry_budget_timeouts") != 328) @compileError("USBMSC retry_budget_timeouts C offset");
    if (@offsetOf(NativeStatus, "transient_sense_retries") != 336) @compileError("USBMSC transient_sense_retries C offset");
    if (@offsetOf(NativeStatus, "unknown_sense_retries") != 344) @compileError("USBMSC unknown_sense_retries C offset");
    if (@offsetOf(NativeStatus, "permanent_sense_stops") != 352) @compileError("USBMSC permanent_sense_stops C offset");
    if (@offsetOf(NativeStatus, "last_retry_opcode") != 360) @compileError("USBMSC last_retry_opcode C offset");
    if (@offsetOf(NativeStatus, "last_retry_number") != 361) @compileError("USBMSC last_retry_number C offset");
    if (@offsetOf(NativeStatus, "mode_sense_ok") != 362) @compileError("USBMSC mode_sense_ok C offset");
    if (@offsetOf(NativeStatus, "write_protected_known") != 363) @compileError("USBMSC write_protected_known C offset");
    if (@offsetOf(NativeStatus, "write_protected") != 364) @compileError("USBMSC write_protected C offset");
    if (@offsetOf(NativeStatus, "tags") != 368) @compileError("USBMSC tags C offset");
    if (@offsetOf(NativeStatus, "commands") != 376) @compileError("USBMSC commands C offset");
    if (@offsetOf(NativeStatus, "failures") != 384) @compileError("USBMSC failures C offset");
    if (@offsetOf(NativeStatus, "reads") != 392) @compileError("USBMSC reads C offset");
    if (@offsetOf(NativeStatus, "read_failures") != 400) @compileError("USBMSC read_failures C offset");
    if (@offsetOf(NativeStatus, "read_transport_retries") != 408) @compileError("USBMSC read_transport_retries C offset");
    if (@offsetOf(NativeStatus, "read_transport_retry_successes") != 416) @compileError("USBMSC read_transport_retry_successes C offset");
    if (@offsetOf(NativeStatus, "writes") != 424) @compileError("USBMSC writes C offset");
    if (@offsetOf(NativeStatus, "write_failures") != 432) @compileError("USBMSC write_failures C offset");
    if (@offsetOf(NativeStatus, "write_transport_retries") != 440) @compileError("USBMSC write_transport_retries C offset");
    if (@offsetOf(NativeStatus, "write_transport_retry_successes") != 448) @compileError("USBMSC write_transport_retry_successes C offset");
    if (@offsetOf(NativeStatus, "flushes") != 456) @compileError("USBMSC flushes C offset");
    if (@offsetOf(NativeStatus, "flush_failures") != 464) @compileError("USBMSC flush_failures C offset");
    if (@offsetOf(NativeStatus, "flush_transport_retries") != 472) @compileError("USBMSC flush_transport_retries C offset");
    if (@offsetOf(NativeStatus, "flush_transport_retry_successes") != 480) @compileError("USBMSC flush_transport_retry_successes C offset");
    if (@offsetOf(NativeStatus, "flush_unsupported") != 488) @compileError("USBMSC flush_unsupported C offset");
    if (@offsetOf(NativeStatus, "recoveries") != 496) @compileError("USBMSC recoveries C offset");
    if (@offsetOf(NativeStatus, "recovery_failures") != 504) @compileError("USBMSC recovery_failures C offset");
    if (@offsetOf(NativeStatus, "resets") != 512) @compileError("USBMSC resets C offset");
    if (@offsetOf(NativeStatus, "clear_halts") != 520) @compileError("USBMSC clear_halts C offset");
    if (@offsetOf(NativeStatus, "sense_requests") != 528) @compileError("USBMSC sense_requests C offset");
    if (@offsetOf(NativeStatus, "sense_valid") != 536) @compileError("USBMSC sense_valid C offset");
    if (@offsetOf(NativeStatus, "sense_for_opcode") != 537) @compileError("USBMSC sense_for_opcode C offset");
    if (@offsetOf(NativeStatus, "sense_key") != 538) @compileError("USBMSC sense_key C offset");
    if (@offsetOf(NativeStatus, "sense_asc") != 539) @compileError("USBMSC sense_asc C offset");
    if (@offsetOf(NativeStatus, "sense_ascq") != 540) @compileError("USBMSC sense_ascq C offset");
    if (@offsetOf(NativeStatus, "sense_not_ready") != 544) @compileError("USBMSC sense_not_ready C offset");
    if (@offsetOf(NativeStatus, "sense_unit_attention") != 552) @compileError("USBMSC sense_unit_attention C offset");
    if (@offsetOf(NativeStatus, "sense_illegal_request") != 560) @compileError("USBMSC sense_illegal_request C offset");
    if (@offsetOf(NativeStatus, "sense_write_protect") != 568) @compileError("USBMSC sense_write_protect C offset");
    if (@offsetOf(NativeStatus, "command_failures") != 576) @compileError("USBMSC command_failures C offset");
    if (@offsetOf(NativeStatus, "transport_failures") != 584) @compileError("USBMSC transport_failures C offset");
    if (@offsetOf(NativeStatus, "transport_faulted") != 592) @compileError("USBMSC transport_faulted C offset");
    if (@offsetOf(NativeStatus, "media_changed") != 593) @compileError("USBMSC media_changed C offset");
    if (@offsetOf(NativeStatus, "last_failure_transport") != 594) @compileError("USBMSC last_failure_transport C offset");
    if (@offsetOf(NativeStatus, "last_recovery_ok") != 595) @compileError("USBMSC last_recovery_ok C offset");
    if (@offsetOf(NativeStatus, "invalid_csw") != 600) @compileError("USBMSC invalid_csw C offset");
    if (@offsetOf(NativeStatus, "csw_tag_mismatches") != 608) @compileError("USBMSC csw_tag_mismatches C offset");
    if (@offsetOf(NativeStatus, "csw_phase_errors") != 616) @compileError("USBMSC csw_phase_errors C offset");
    if (@offsetOf(NativeStatus, "csw_residue_errors") != 624) @compileError("USBMSC csw_residue_errors C offset");
    if (@offsetOf(NativeStatus, "short_data_errors") != 632) @compileError("USBMSC short_data_errors C offset");
    if (@offsetOf(NativeStatus, "last_opcode") != 640) @compileError("USBMSC last_opcode C offset");
    if (@offsetOf(NativeStatus, "last_transfer_len") != 644) @compileError("USBMSC last_transfer_len C offset");
    if (@offsetOf(NativeStatus, "last_data_actual_len") != 648) @compileError("USBMSC last_data_actual_len C offset");
    if (@offsetOf(NativeStatus, "last_csw_tag") != 652) @compileError("USBMSC last_csw_tag C offset");
    if (@offsetOf(NativeStatus, "last_csw_residue") != 656) @compileError("USBMSC last_csw_residue C offset");
    if (@offsetOf(NativeStatus, "last_csw_status") != 660) @compileError("USBMSC last_csw_status C offset");
    if (@offsetOf(NativeStatus, "bot_source") != 664) @compileError("USBMSC bot_source C offset");
    if (@offsetOf(NativeStatus, "bot_r4p_build") != 680) @compileError("USBMSC bot_r4p_build C offset");
    if (@offsetOf(NativeStatus, "bot_r4p_csw") != 688) @compileError("USBMSC bot_r4p_csw C offset");
    if (@offsetOf(NativeStatus, "bot_dispatch_failures") != 696) @compileError("USBMSC bot_dispatch_failures C offset");
    if (@offsetOf(NativeStatus, "bot_last_result") != 704) @compileError("USBMSC bot_last_result C offset");
    if (@offsetOf(NativeStatus, "scsi_source") != 712) @compileError("USBMSC scsi_source C offset");
    if (@offsetOf(NativeStatus, "scsi_r4p_build") != 728) @compileError("USBMSC scsi_r4p_build C offset");
    if (@offsetOf(NativeStatus, "scsi_r4p_parse") != 736) @compileError("USBMSC scsi_r4p_parse C offset");
    if (@offsetOf(NativeStatus, "scsi_dispatch_failures") != 744) @compileError("USBMSC scsi_dispatch_failures C offset");
    if (@offsetOf(NativeStatus, "scsi_last_result") != 752) @compileError("USBMSC scsi_last_result C offset");
    if (@offsetOf(NativeStatus, "protocol_required_missing") != 760) @compileError("USBMSC protocol_required_missing C offset");
    if (@offsetOf(NativeStatus, "reason") != 768) @compileError("USBMSC reason C offset");
    if (@offsetOf(NativeStatus, "reason_storage") != 784) @compileError("USBMSC reason_storage C offset");
    if (@offsetOf(NativeStatus, "transport_incident") != 912) @compileError("USBMSC transport_incident C offset");
}

extern fn r4os_rust_usb_msc_init() bool;
extern fn r4os_rust_usb_msc_owner(u8) void;
extern fn r4os_rust_usb_msc_external_owner() bool;
extern fn r4os_rust_usb_msc_status(*NativeStatus, [*]u8, *usize) void;
extern fn r4os_rust_usb_msc_block_device_count() usize;
extern fn r4os_rust_usb_msc_device_index() usize;
extern fn r4os_rust_usb_msc_reselect() bool;
pub fn init() bool {
    return r4os_rust_usb_msc_init();
}
pub fn setPreloadOwner() void {
    r4os_rust_usb_msc_owner(1);
}
pub fn setRuntimeOwner() void {
    r4os_rust_usb_msc_owner(2);
}
pub fn resetBuiltInOwner() void {
    r4os_rust_usb_msc_owner(0);
}
pub fn hasExternalOwner() bool {
    return r4os_rust_usb_msc_external_owner();
}
pub fn blockDeviceCount() usize {
    return r4os_rust_usb_msc_block_device_count();
}
pub fn deviceIndex() ?usize {
    const n = r4os_rust_usb_msc_device_index();
    return if (n == ~@as(usize, 0)) null else n;
}
pub fn reselectActiveDevice() bool {
    return r4os_rust_usb_msc_reselect();
}
pub fn status() Status {
    var out: NativeStatus = undefined;
    var reason: [128]u8 = undefined;
    var reason_len: usize = 0;
    r4os_rust_usb_msc_status(&out, &reason, &reason_len);
    return .{
        .initialized = out.initialized,
        .present = out.present,
        .bound = out.bound,
        .block_registered = out.block_registered,
        .block_index = if (out.block_index == ~@as(usize, 0)) null else out.block_index,
        .port = out.port,
        .device = out.device,
        .bulk_in_handle = out.bulk_in_handle,
        .bulk_out_handle = out.bulk_out_handle,
        .interface_number = out.interface_number,
        .bulk_in = out.bulk_in,
        .bulk_out = out.bulk_out,
        .bulk_in_max_packet = out.bulk_in_max_packet,
        .bulk_out_max_packet = out.bulk_out_max_packet,
        .sector_count = out.sector_count,
        .sector_size = out.sector_size,
        .max_sectors_per_request = out.max_sectors_per_request,
        .capacity_format = out.capacity_format,
        .capacity16_used = out.capacity16_used,
        .read10_commands = out.read10_commands,
        .read16_commands = out.read16_commands,
        .write10_commands = out.write10_commands,
        .write16_commands = out.write16_commands,
        .transport = out.transport.slice(),
        .lun = out.lun,
        .max_lun = out.max_lun,
        .uas_supported = out.uas_supported,
        .inquiry_ok = out.inquiry_ok,
        .model = out.model,
        .model_len = out.model_len,
        .inquiry_retries = out.inquiry_retries,
        .inquiry_wait_elapsed_ns = out.inquiry_wait_elapsed_ns,
        .test_unit_ready_ok = out.test_unit_ready_ok,
        .test_unit_ready_retries = out.test_unit_ready_retries,
        .ready_wait_elapsed_ns = out.ready_wait_elapsed_ns,
        .read_capacity_ok = out.read_capacity_ok,
        .read_capacity_retries = out.read_capacity_retries,
        .capacity_wait_elapsed_ns = out.capacity_wait_elapsed_ns,
        .retry_delay_calls = out.retry_delay_calls,
        .retry_budget_timeouts = out.retry_budget_timeouts,
        .transient_sense_retries = out.transient_sense_retries,
        .unknown_sense_retries = out.unknown_sense_retries,
        .permanent_sense_stops = out.permanent_sense_stops,
        .last_retry_opcode = out.last_retry_opcode,
        .last_retry_number = out.last_retry_number,
        .mode_sense_ok = out.mode_sense_ok,
        .write_protected_known = out.write_protected_known,
        .write_protected = out.write_protected,
        .tags = out.tags,
        .commands = out.commands,
        .failures = out.failures,
        .reads = out.reads,
        .read_failures = out.read_failures,
        .read_transport_retries = out.read_transport_retries,
        .read_transport_retry_successes = out.read_transport_retry_successes,
        .writes = out.writes,
        .write_failures = out.write_failures,
        .write_transport_retries = out.write_transport_retries,
        .write_transport_retry_successes = out.write_transport_retry_successes,
        .flushes = out.flushes,
        .flush_failures = out.flush_failures,
        .flush_transport_retries = out.flush_transport_retries,
        .flush_transport_retry_successes = out.flush_transport_retry_successes,
        .flush_unsupported = out.flush_unsupported,
        .recoveries = out.recoveries,
        .recovery_failures = out.recovery_failures,
        .resets = out.resets,
        .clear_halts = out.clear_halts,
        .sense_requests = out.sense_requests,
        .sense_valid = out.sense_valid,
        .sense_for_opcode = out.sense_for_opcode,
        .sense_key = out.sense_key,
        .sense_asc = out.sense_asc,
        .sense_ascq = out.sense_ascq,
        .sense_not_ready = out.sense_not_ready,
        .sense_unit_attention = out.sense_unit_attention,
        .sense_illegal_request = out.sense_illegal_request,
        .sense_write_protect = out.sense_write_protect,
        .command_failures = out.command_failures,
        .transport_failures = out.transport_failures,
        .transport_faulted = out.transport_faulted,
        .media_changed = out.media_changed,
        .last_failure_transport = out.last_failure_transport,
        .last_recovery_ok = out.last_recovery_ok,
        .invalid_csw = out.invalid_csw,
        .csw_tag_mismatches = out.csw_tag_mismatches,
        .csw_phase_errors = out.csw_phase_errors,
        .csw_residue_errors = out.csw_residue_errors,
        .short_data_errors = out.short_data_errors,
        .last_opcode = out.last_opcode,
        .last_transfer_len = out.last_transfer_len,
        .last_data_actual_len = out.last_data_actual_len,
        .last_csw_tag = out.last_csw_tag,
        .last_csw_residue = out.last_csw_residue,
        .last_csw_status = out.last_csw_status,
        .bot_source = out.bot_source.slice(),
        .bot_r4p_build = out.bot_r4p_build,
        .bot_r4p_csw = out.bot_r4p_csw,
        .bot_dispatch_failures = out.bot_dispatch_failures,
        .bot_last_result = out.bot_last_result,
        .scsi_source = out.scsi_source.slice(),
        .scsi_r4p_build = out.scsi_r4p_build,
        .scsi_r4p_parse = out.scsi_r4p_parse,
        .scsi_dispatch_failures = out.scsi_dispatch_failures,
        .scsi_last_result = out.scsi_last_result,
        .protocol_required_missing = out.protocol_required_missing,
        .reason_len = reason_len,
        .reason_storage = reason,
        .transport_incident = .{ .generation = out.transport_incident.generation },
    };
}
extern fn r4os_rust_usb_msc_read(?*anyopaque, u64, u16, [*]u8, usize) bool;
extern fn r4os_rust_usb_msc_write(?*anyopaque, u64, u16, [*]const u8, usize) bool;
extern fn r4os_rust_usb_msc_flush(?*anyopaque) bool;
fn readBlock(ctx: ?*anyopaque, lba: u64, sectors: u16, out: []u8) bool {
    return r4os_rust_usb_msc_read(ctx, lba, sectors, out.ptr, out.len);
}
fn writeBlock(ctx: ?*anyopaque, lba: u64, sectors: u16, data: []const u8) bool {
    return r4os_rust_usb_msc_write(ctx, lba, sectors, data.ptr, data.len);
}
fn flushBlock(ctx: ?*anyopaque) bool {
    return r4os_rust_usb_msc_flush(ctx);
}
const media_names = [_][]const u8{ "usb0", "usb1", "usb2", "usb3", "usb4", "usb5", "usb6", "usb7", "usb8", "usb9", "usb10", "usb11", "usb12", "usb13", "usb14", "usb15" };
export fn r4os_zig_usb_msc_register(current: *NativeStatus, index: usize, source: u8) usize {
    return block.register(.{
        .name = media_names[index],
        .model = current.model[0..current.model_len],
        .driver = "USBMSC",
        .bus = .usb,
        .controller = "xhci",
        .port = current.port,
        .sector_size = current.sector_size,
        .sector_count = current.sector_count,
        .max_sectors_per_request = current.max_sectors_per_request,
        .queue_depth = 1,
        .removable = true,
        .writable = !current.write_protected,
        .owns_transport_retry = true,
        .source = @enumFromInt(source),
        .ctx = current,
        .read_fn = readBlock,
        .write_fn = if (current.write_protected) null else writeBlock,
        .flush_fn = flushBlock,
        .state = .active,
    }) orelse ~@as(usize, 0);
}
