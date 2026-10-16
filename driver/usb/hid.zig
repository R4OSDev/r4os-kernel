// Native HID owner; stateless fixed projections to existing kernel peers.
const usb_core = @import("core.zig");
const keyboard = @import("../input/keyboard.zig");
const mouse = @import("../input/mouse.zig");
const hid_set1 = @import("../input/hid_set1.zig");
const platform_input = @import("../input/platform_input.zig");
const r4p_contract = @import("../../net/r4p_contract.zig");
const Text = usb_core.Text;
pub const Status = struct {
    initialized: bool = false,
    keyboard_present: bool = false,
    keyboard_bound: bool = false,
    mouse_present: bool = false,
    mouse_bound: bool = false,
    unsupported_hid: u8 = 0,
    missing_endpoint_hid: u8 = 0,
    port: u8 = 0,
    interface_number: u8 = 0,
    endpoint_address: u8 = 0,
    endpoint_interval: u8 = 0,
    endpoint_max_packet: u16 = 0,
    polls: u64 = 0,
    no_reports: u64 = 0,
    reports: u64 = 0,
    duplicate_reports: u64 = 0,
    service_polls: u64 = 0,
    decoded_keys: u64 = 0,
    decoded_mouse: u64 = 0,
    drops: u64 = 0,
    keyboard_backpressure_active: bool = false,
    keyboard_backpressure_polls: u64 = 0,
    keyboard_queue_capacity: u32 = 0,
    keyboard_queue_pending: u32 = 0,
    keyboard_queue_free: u32 = 0,
    keyboard_queue_drops: u64 = 0,
    topology_reconciles: u64 = 0,
    failures: u64 = 0,
    setup_warnings: u8 = 0,
    protocol_ok: bool = false,
    idle_ok: bool = false,
    report_id_heuristic: bool = false,
    report_descriptor_ok: bool = false,
    report_descriptor_malformed: bool = false,
    last_report_request_len: u8 = 0,
    last_report_residue: u32 = 0,
    last_report_len: u8 = 0,
    last_nonzero_report_len: u8 = 0,
    last_modifiers: u8 = 0,
    last_usage: u8 = 0,
    last_mouse_buttons: u8 = 0,
    last_mouse_dx: i32 = 0,
    last_mouse_dy: i32 = 0,
    last_mouse_wheel: i32 = 0,
    boot_source: []const u8 = "none",
    boot_r4p_classify: u64 = 0,
    boot_r4p_keyboard: u64 = 0,
    boot_r4p_mouse: u64 = 0,
    protocol_required_missing: u8 = 0,
    boot_dispatch_failures: u64 = 0,
    boot_last_result: i32 = 0,
    reason: []const u8 = "not initialized",
};
const NativeStatus = extern struct {
    initialized: bool,
    keyboard_present: bool,
    keyboard_bound: bool,
    mouse_present: bool,
    mouse_bound: bool,
    unsupported_hid: u8,
    missing_endpoint_hid: u8,
    port: u8,
    interface_number: u8,
    endpoint_address: u8,
    endpoint_interval: u8,
    endpoint_max_packet: u16,
    polls: u64,
    no_reports: u64,
    reports: u64,
    duplicate_reports: u64,
    service_polls: u64,
    decoded_keys: u64,
    decoded_mouse: u64,
    drops: u64,
    keyboard_backpressure_active: bool,
    keyboard_backpressure_polls: u64,
    keyboard_queue_capacity: u32,
    keyboard_queue_pending: u32,
    keyboard_queue_free: u32,
    keyboard_queue_drops: u64,
    topology_reconciles: u64,
    failures: u64,
    setup_warnings: u8,
    protocol_ok: bool,
    idle_ok: bool,
    report_id_heuristic: bool,
    report_descriptor_ok: bool,
    report_descriptor_malformed: bool,
    last_report_request_len: u8,
    last_report_residue: u32,
    last_report_len: u8,
    last_nonzero_report_len: u8,
    last_modifiers: u8,
    last_usage: u8,
    last_mouse_buttons: u8,
    last_mouse_dx: i32,
    last_mouse_dy: i32,
    last_mouse_wheel: i32,
    boot_source: Text,
    boot_r4p_classify: u64,
    boot_r4p_keyboard: u64,
    boot_r4p_mouse: u64,
    protocol_required_missing: u8,
    boot_dispatch_failures: u64,
    boot_last_result: i32,
    reason: Text,
};
comptime {
    if (@sizeOf(NativeStatus) != 248 or @alignOf(NativeStatus) != 8) @compileError("HID status C layout");
    if (@offsetOf(NativeStatus, "initialized") != 0) @compileError("HID status initialized C offset");
    if (@offsetOf(NativeStatus, "keyboard_present") != 1) @compileError("HID status keyboard_present C offset");
    if (@offsetOf(NativeStatus, "keyboard_bound") != 2) @compileError("HID status keyboard_bound C offset");
    if (@offsetOf(NativeStatus, "mouse_present") != 3) @compileError("HID status mouse_present C offset");
    if (@offsetOf(NativeStatus, "mouse_bound") != 4) @compileError("HID status mouse_bound C offset");
    if (@offsetOf(NativeStatus, "unsupported_hid") != 5) @compileError("HID status unsupported_hid C offset");
    if (@offsetOf(NativeStatus, "missing_endpoint_hid") != 6) @compileError("HID status missing_endpoint_hid C offset");
    if (@offsetOf(NativeStatus, "port") != 7) @compileError("HID status port C offset");
    if (@offsetOf(NativeStatus, "interface_number") != 8) @compileError("HID status interface_number C offset");
    if (@offsetOf(NativeStatus, "endpoint_address") != 9) @compileError("HID status endpoint_address C offset");
    if (@offsetOf(NativeStatus, "endpoint_interval") != 10) @compileError("HID status endpoint_interval C offset");
    if (@offsetOf(NativeStatus, "endpoint_max_packet") != 12) @compileError("HID status endpoint_max_packet C offset");
    if (@offsetOf(NativeStatus, "polls") != 16) @compileError("HID status polls C offset");
    if (@offsetOf(NativeStatus, "no_reports") != 24) @compileError("HID status no_reports C offset");
    if (@offsetOf(NativeStatus, "reports") != 32) @compileError("HID status reports C offset");
    if (@offsetOf(NativeStatus, "duplicate_reports") != 40) @compileError("HID status duplicate_reports C offset");
    if (@offsetOf(NativeStatus, "service_polls") != 48) @compileError("HID status service_polls C offset");
    if (@offsetOf(NativeStatus, "decoded_keys") != 56) @compileError("HID status decoded_keys C offset");
    if (@offsetOf(NativeStatus, "decoded_mouse") != 64) @compileError("HID status decoded_mouse C offset");
    if (@offsetOf(NativeStatus, "drops") != 72) @compileError("HID status drops C offset");
    if (@offsetOf(NativeStatus, "keyboard_backpressure_active") != 80) @compileError("HID status keyboard_backpressure_active C offset");
    if (@offsetOf(NativeStatus, "keyboard_backpressure_polls") != 88) @compileError("HID status keyboard_backpressure_polls C offset");
    if (@offsetOf(NativeStatus, "keyboard_queue_capacity") != 96) @compileError("HID status keyboard_queue_capacity C offset");
    if (@offsetOf(NativeStatus, "keyboard_queue_pending") != 100) @compileError("HID status keyboard_queue_pending C offset");
    if (@offsetOf(NativeStatus, "keyboard_queue_free") != 104) @compileError("HID status keyboard_queue_free C offset");
    if (@offsetOf(NativeStatus, "keyboard_queue_drops") != 112) @compileError("HID status keyboard_queue_drops C offset");
    if (@offsetOf(NativeStatus, "topology_reconciles") != 120) @compileError("HID status topology_reconciles C offset");
    if (@offsetOf(NativeStatus, "failures") != 128) @compileError("HID status failures C offset");
    if (@offsetOf(NativeStatus, "setup_warnings") != 136) @compileError("HID status setup_warnings C offset");
    if (@offsetOf(NativeStatus, "protocol_ok") != 137) @compileError("HID status protocol_ok C offset");
    if (@offsetOf(NativeStatus, "idle_ok") != 138) @compileError("HID status idle_ok C offset");
    if (@offsetOf(NativeStatus, "report_id_heuristic") != 139) @compileError("HID status report_id_heuristic C offset");
    if (@offsetOf(NativeStatus, "report_descriptor_ok") != 140) @compileError("HID status report_descriptor_ok C offset");
    if (@offsetOf(NativeStatus, "report_descriptor_malformed") != 141) @compileError("HID status report_descriptor_malformed C offset");
    if (@offsetOf(NativeStatus, "last_report_request_len") != 142) @compileError("HID status last_report_request_len C offset");
    if (@offsetOf(NativeStatus, "last_report_residue") != 144) @compileError("HID status last_report_residue C offset");
    if (@offsetOf(NativeStatus, "last_report_len") != 148) @compileError("HID status last_report_len C offset");
    if (@offsetOf(NativeStatus, "last_nonzero_report_len") != 149) @compileError("HID status last_nonzero_report_len C offset");
    if (@offsetOf(NativeStatus, "last_modifiers") != 150) @compileError("HID status last_modifiers C offset");
    if (@offsetOf(NativeStatus, "last_usage") != 151) @compileError("HID status last_usage C offset");
    if (@offsetOf(NativeStatus, "last_mouse_buttons") != 152) @compileError("HID status last_mouse_buttons C offset");
    if (@offsetOf(NativeStatus, "last_mouse_dx") != 156) @compileError("HID status last_mouse_dx C offset");
    if (@offsetOf(NativeStatus, "last_mouse_dy") != 160) @compileError("HID status last_mouse_dy C offset");
    if (@offsetOf(NativeStatus, "last_mouse_wheel") != 164) @compileError("HID status last_mouse_wheel C offset");
    if (@offsetOf(NativeStatus, "boot_source") != 168) @compileError("HID status boot_source C offset");
    if (@offsetOf(NativeStatus, "boot_r4p_classify") != 184) @compileError("HID status boot_r4p_classify C offset");
    if (@offsetOf(NativeStatus, "boot_r4p_keyboard") != 192) @compileError("HID status boot_r4p_keyboard C offset");
    if (@offsetOf(NativeStatus, "boot_r4p_mouse") != 200) @compileError("HID status boot_r4p_mouse C offset");
    if (@offsetOf(NativeStatus, "protocol_required_missing") != 208) @compileError("HID status protocol_required_missing C offset");
    if (@offsetOf(NativeStatus, "boot_dispatch_failures") != 216) @compileError("HID status boot_dispatch_failures C offset");
    if (@offsetOf(NativeStatus, "boot_last_result") != 224) @compileError("HID status boot_last_result C offset");
    if (@offsetOf(NativeStatus, "reason") != 232) @compileError("HID status reason C offset");
}
pub const Options = struct {
    bind_mouse: bool = true,
};
pub const PollTaskSummary = extern struct {
    started: bool = false,
    task_id: u32 = 0,
    iterations: u64 = 0,
};

extern fn r4os_rust_usb_hid_status(*NativeStatus) void;
extern fn r4os_rust_usb_hid_init(bool) bool;
extern fn r4os_rust_usb_hid_service_poll() void;
extern fn r4os_rust_usb_hid_start_poll_task() bool;
extern fn r4os_rust_usb_hid_poll_task_summary(*PollTaskSummary) void;
pub fn init() bool {
    return initWithOptions(.{});
}
pub fn initWithOptions(selected: Options) bool {
    return r4os_rust_usb_hid_init(selected.bind_mouse);
}
pub fn status() Status {
    var out: NativeStatus = undefined;
    r4os_rust_usb_hid_status(&out);
    return .{
        .initialized = out.initialized,
        .keyboard_present = out.keyboard_present,
        .keyboard_bound = out.keyboard_bound,
        .mouse_present = out.mouse_present,
        .mouse_bound = out.mouse_bound,
        .unsupported_hid = out.unsupported_hid,
        .missing_endpoint_hid = out.missing_endpoint_hid,
        .port = out.port,
        .interface_number = out.interface_number,
        .endpoint_address = out.endpoint_address,
        .endpoint_interval = out.endpoint_interval,
        .endpoint_max_packet = out.endpoint_max_packet,
        .polls = out.polls,
        .no_reports = out.no_reports,
        .reports = out.reports,
        .duplicate_reports = out.duplicate_reports,
        .service_polls = out.service_polls,
        .decoded_keys = out.decoded_keys,
        .decoded_mouse = out.decoded_mouse,
        .drops = out.drops,
        .keyboard_backpressure_active = out.keyboard_backpressure_active,
        .keyboard_backpressure_polls = out.keyboard_backpressure_polls,
        .keyboard_queue_capacity = out.keyboard_queue_capacity,
        .keyboard_queue_pending = out.keyboard_queue_pending,
        .keyboard_queue_free = out.keyboard_queue_free,
        .keyboard_queue_drops = out.keyboard_queue_drops,
        .topology_reconciles = out.topology_reconciles,
        .failures = out.failures,
        .setup_warnings = out.setup_warnings,
        .protocol_ok = out.protocol_ok,
        .idle_ok = out.idle_ok,
        .report_id_heuristic = out.report_id_heuristic,
        .report_descriptor_ok = out.report_descriptor_ok,
        .report_descriptor_malformed = out.report_descriptor_malformed,
        .last_report_request_len = out.last_report_request_len,
        .last_report_residue = out.last_report_residue,
        .last_report_len = out.last_report_len,
        .last_nonzero_report_len = out.last_nonzero_report_len,
        .last_modifiers = out.last_modifiers,
        .last_usage = out.last_usage,
        .last_mouse_buttons = out.last_mouse_buttons,
        .last_mouse_dx = out.last_mouse_dx,
        .last_mouse_dy = out.last_mouse_dy,
        .last_mouse_wheel = out.last_mouse_wheel,
        .boot_source = out.boot_source.slice(),
        .boot_r4p_classify = out.boot_r4p_classify,
        .boot_r4p_keyboard = out.boot_r4p_keyboard,
        .boot_r4p_mouse = out.boot_r4p_mouse,
        .protocol_required_missing = out.protocol_required_missing,
        .boot_dispatch_failures = out.boot_dispatch_failures,
        .boot_last_result = out.boot_last_result,
        .reason = out.reason.slice(),
    };
}
pub fn servicePoll() void {
    r4os_rust_usb_hid_service_poll();
}
pub fn diagnosticPoll() void {
    r4os_rust_usb_hid_service_poll();
}
pub fn startPollTask() bool {
    return r4os_rust_usb_hid_start_poll_task();
}
pub fn pollTaskSummary() PollTaskSummary {
    var out: PollTaskSummary = undefined;
    r4os_rust_usb_hid_poll_task_summary(&out);
    return out;
}
const QueueStats = extern struct { capacity: u32, pending: u32, drops: u64 };
export fn r4os_zig_usb_hid_queue(out: *QueueStats) void {
    const value = keyboard.stats();
    out.* = .{ .capacity = value.queue_capacity, .pending = value.queue_pending, .drops = value.dropped_count };
}
export fn r4os_zig_usb_hid_can_accept(count: u32) bool {
    return keyboard.canAccept(count);
}
export fn r4os_zig_usb_hid_platform_usb(value: bool) void {
    platform_input.usb(value);
}
export fn r4os_zig_usb_hid_consumer(usage: u16) void {
    platform_input.consumer(usage);
}
export fn r4os_zig_usb_hid_set_keyboard_hook(hook: ?keyboard.PollHook) void {
    keyboard.setPollHook(hook);
}
export fn r4os_zig_usb_hid_set_mouse_hook(hook: ?mouse.PollHook) void {
    mouse.setPollHook(hook);
}
fn injectCode(code: hid_set1.Code, down: bool) void {
    if (code.extended) keyboard.injectScancode(0xE0);
    keyboard.injectScancode(if (down) code.make else code.make | 0x80);
}
export fn r4os_zig_usb_hid_inject_usage(usage: u8, down: bool) bool {
    const code = hid_set1.usageToCode(usage) orelse return false;
    injectCode(code, down);
    return true;
}
export fn r4os_zig_usb_hid_inject_modifier(bit: u8, down: bool) void {
    injectCode(hid_set1.modifierToCode(bit).?, down);
}
export fn r4os_zig_usb_hid_released(op: *const r4p_contract.UsbHidBootOp, out: [*]u8, capacity: usize) usize {
    const previous_len: usize = @min(op.previous_len, op.previous.len);
    const report_len: usize = @min(op.report_len, op.report.len);
    const previous_offset: usize = @min(@as(usize, op.previous_offset), previous_len);
    const report_offset: usize = @min(@as(usize, op.report_offset), report_len);
    return hid_set1.collectReleasedUsages(op.previous[0..previous_len], previous_offset, op.report[0..report_len], report_offset, out[0..capacity]);
}
export fn r4os_zig_usb_hid_mouse(dx: i32, dy: i32, buttons: u8, wheel: i32) void {
    mouse.injectRelativePacketWheel(dx, dy, buttons, wheel);
}
