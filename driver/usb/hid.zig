const protocol_api = @import("../../kernel/protocol_api.zig");
const r4p = @import("../../program/r4p.zig");
const r4p_contract = @import("../../net/r4p_contract.zig");
const keyboard = @import("../input/keyboard.zig");
const timer_core = @import("../../kernel/timer.zig");
const hid_poll_idle_ticks: u64 = @max(1, (10 * @as(u64, timer_core.DEFAULT_HZ)) / 1000);
const mouse = @import("../input/mouse.zig");
const usb_core = @import("core.zig");
const hid_report = @import("hid_report.zig");
const xhci = @import("xhci.zig");
const usb_host = @import("host_controller.zig");
const k = @import("../../kernel/log.zig");
const sched_task = @import("../../sched/task.zig");
const scheduler = @import("../../sched/scheduler.zig");

const MOUSE_POLL_BUDGET: usize = 8;
const REPORT_BUFFER_LEN: usize = 32;
const REPORT_DESCRIPTOR_MAX: usize = 512;
// One boot-keyboard report can publish all decoded usages plus both GUI
// transitions. Polling pauses before taking ownership of such a report unless
// the canonical keyboard queue can accept the complete worst-case burst.
const KEYBOARD_REPORT_QUEUE_RESERVE: u32 = r4p_contract.USB_HID_BOOT_MAX_KEYS + 2;

const HidKind = enum {
    keyboard,
    mouse,
};

const HidInterfaceCandidate = struct {
    number: u8 = 0,
    class_code: u8 = 0,
    subclass: u8 = 0,
    protocol: u8 = 0,
    endpoint_address: u8 = 0,
    endpoint_max_packet: u16 = 0,
    endpoint_interval: u8 = 0,
    report_descriptor_len: u16 = 0,
};

const AggregateFlag = enum {
    protocol,
    idle,
};

const HidBinding = struct {
    present: bool = false,
    bound: bool = false,
    port: u8 = 0,
    device: xhci.DeviceHandle = .{},
    endpoint: xhci.EndpointHandle = .{},
    interface_number: u8 = 0,
    endpoint_address: u8 = 0,
    endpoint_interval: u8 = 0,
    endpoint_max_packet: u16 = 0,
    report_descriptor_len: u16 = 0,
    report_descriptor_read_len: u16 = 0,
    report_summary: hid_report.Summary = .{},
    polls: u64 = 0,
    no_reports: u64 = 0,
    reports: u64 = 0,
    duplicate_reports: u64 = 0,
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
    last_report: [REPORT_BUFFER_LEN]u8 = .{0} ** REPORT_BUFFER_LEN,
    last_nonzero_report: [REPORT_BUFFER_LEN]u8 = .{0} ** REPORT_BUFFER_LEN,
};

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

var current: Status = .{};
var keyboard_binding: HidBinding = .{};
var mouse_binding: HidBinding = .{};
var service_polls: u64 = 0;
var keyboard_backpressure_active = false;
var keyboard_backpressure_polls: u64 = 0;
var topology_reconciles: u64 = 0;
var boot_r4p_classify: u64 = 0;
var boot_r4p_keyboard: u64 = 0;
var boot_r4p_mouse: u64 = 0;
var boot_dispatch_failures: u64 = 0;
var boot_last_result: i32 = 0;

pub fn init() bool {
    if (!xhci.acquireControllerOwnership()) return false;
    defer xhci.releaseControllerOwnership();
    current = .{ .initialized = true, .reason = "no USB HID boot device found" };
    keyboard_binding = .{};
    mouse_binding = .{};
    service_polls = 0;
    keyboard_backpressure_active = false;
    keyboard_backpressure_polls = 0;
    topology_reconciles = 0;
    boot_r4p_classify = 0;
    boot_r4p_keyboard = 0;
    boot_r4p_mouse = 0;
    boot_dispatch_failures = 0;
    boot_last_result = 0;
    xhci.setTopologyHook(topologyChangedHook);

    var index: usize = 0;
    while (usb_core.deviceAt(index)) |dev| : (index += 1) {
        scanDevice(dev);
    }

    refreshAggregateStatus();
    if (current.keyboard_bound and current.mouse_bound) {
        current.reason = "USB HID boot keyboard bound; USB HID boot mouse bound";
        if (current.setup_warnings != 0) current.reason = "USB HID boot keyboard bound; USB HID boot mouse bound; optional class setup warning";
    }
    return current.keyboard_bound or current.mouse_bound;
}

fn topologyChangedHook() callconv(.c) void {
    topology_reconciles +%= 1;
    reconcileBindings();
    if (keyboard_binding.bound or mouse_binding.bound) _ = startPollTask();
}

fn reconcileBindings() void {
    if (!xhci.acquireControllerOwnership()) {
        current.reason = "USB HID topology reconcile ownership failed";
        return;
    }
    defer xhci.releaseControllerOwnership();

    if (keyboard_binding.present and !bindingStillPublished(&keyboard_binding)) keyboard_binding = .{};
    if (mouse_binding.present and !bindingStillPublished(&mouse_binding)) mouse_binding = .{};
    if (!keyboard_binding.bound) {
        keyboard_backpressure_active = false;
        keyboard.setPollHook(null);
    }
    if (!mouse_binding.bound) mouse.setPollHook(null);

    current.unsupported_hid = 0;
    current.missing_endpoint_hid = 0;
    current.protocol_required_missing = 0;
    var index: usize = 0;
    while (usb_core.deviceAt(index)) |dev| : (index += 1) scanDevice(dev);

    refreshAggregateStatus();
    if (current.keyboard_bound and current.mouse_bound) {
        current.reason = "USB HID boot keyboard bound; USB HID boot mouse bound";
    } else if (current.keyboard_bound) {
        current.reason = "USB HID boot keyboard bound";
    } else if (current.mouse_bound) {
        current.reason = "USB HID boot mouse bound";
    } else {
        current.reason = "no USB HID boot device found";
    }
}

fn bindingStillPublished(binding: *const HidBinding) bool {
    if (!binding.present or binding.port == 0 or binding.device.slot_id == 0) return false;
    var index: usize = 0;
    while (usb_core.deviceAt(index)) |dev| : (index += 1) {
        if (!dev.active or !dev.configured) continue;
        if (dev.port == binding.port and dev.slot_id == binding.device.slot_id) return true;
    }
    return false;
}

fn scanDevice(dev: *const usb_core.Device) void {
    var records_seen = false;
    var iface_index: usize = 0;
    while (iface_index < @as(usize, dev.interface_record_count) and iface_index < usb_core.MAX_USB_INTERFACES) : (iface_index += 1) {
        const iface = dev.interfaces[iface_index];
        if (!iface.active) continue;
        records_seen = true;
        handleCandidate(dev, .{
            .number = iface.number,
            .class_code = iface.class_code,
            .subclass = iface.subclass,
            .protocol = iface.protocol,
            .endpoint_address = chooseInterruptEndpoint(iface.interrupt_in_endpoint_address, iface.first_endpoint_address, iface.first_endpoint_attributes),
            .endpoint_max_packet = if (iface.interrupt_in_endpoint_address != 0) iface.interrupt_in_endpoint_max_packet else iface.first_endpoint_max_packet,
            .endpoint_interval = if (iface.interrupt_in_endpoint_address != 0) iface.interrupt_in_endpoint_interval else iface.first_endpoint_interval,
            .report_descriptor_len = iface.hid_report_descriptor_len,
        });
    }
    if (!records_seen) {
        handleCandidate(dev, .{
            .number = dev.first_interface_number,
            .class_code = dev.first_interface_class,
            .subclass = dev.first_interface_subclass,
            .protocol = dev.first_interface_protocol,
            .endpoint_address = chooseInterruptEndpoint(0, dev.first_endpoint_address, dev.first_endpoint_attributes),
            .endpoint_max_packet = dev.first_endpoint_max_packet,
            .endpoint_interval = dev.first_endpoint_interval,
            .report_descriptor_len = dev.hid_report_descriptor_len,
        });
    }
}

fn chooseInterruptEndpoint(interrupt_in: u8, first_address: u8, first_attributes: u8) u8 {
    if (interrupt_in != 0) return interrupt_in;
    if ((first_address & 0x80) != 0 and (first_attributes & 0x03) == 0x03) return first_address;
    return 0;
}

fn handleCandidate(dev: *const usb_core.Device, candidate: HidInterfaceCandidate) void {
    if (candidate.class_code != 0x03) return;
    const kind = classifyCandidate(candidate) orelse {
        if (candidate.subclass == 0x01 and !candidateHasInterruptEndpoint(candidate)) {
            current.missing_endpoint_hid += 1;
            current.reason = "USB HID boot device missing interrupt-in endpoint";
        } else if (candidate.subclass != 0x01 or (candidate.protocol != 0x01 and candidate.protocol != 0x02)) {
            current.unsupported_hid += 1;
        }
        return;
    };
    if (kind == .keyboard) {
        if (keyboard_binding.bound) return;
        current.keyboard_present = true;
        fillBinding(&keyboard_binding, dev, candidate);
        if (bindDevice(&keyboard_binding, .keyboard)) {
            current.keyboard_bound = true;
            keyboard.setPollHook(pollHook);
            current.reason = if (keyboard_binding.setup_warnings == 0) "USB HID boot keyboard bound" else "USB HID boot keyboard bound; optional class setup warning";
        } else {
            current.reason = "USB HID keyboard bind failed";
        }
        return;
    }
    if (kind == .mouse) {
        if (mouse_binding.bound) return;
        current.mouse_present = true;
        fillBinding(&mouse_binding, dev, candidate);
        if (bindDevice(&mouse_binding, .mouse)) {
            current.mouse_bound = true;
            mouse.setPollHook(mousePollHook);
            current.reason = if (mouse_binding.setup_warnings == 0) "USB HID boot mouse bound" else "USB HID boot mouse bound; optional class setup warning";
        } else {
            current.reason = "USB HID mouse bind failed";
        }
        return;
    }
    current.unsupported_hid += 1;
}

fn classifyCandidate(candidate: HidInterfaceCandidate) ?HidKind {
    if (!hidProtocolRolesReady()) {
        recordRequiredProtocolBlock(candidate);
        return null;
    }
    return classifyCandidateR4p(candidate);
}

fn classifyCandidateR4p(candidate: HidInterfaceCandidate) ?HidKind {
    if (!r4p.hasActiveR4p("usb.hid_boot")) return null;
    var op: r4p_contract.UsbHidBootOp = .{
        .class_code = candidate.class_code,
        .subclass = candidate.subclass,
        .protocol = candidate.protocol,
        .endpoint_address = candidate.endpoint_address,
        .endpoint_max_packet = candidate.endpoint_max_packet,
    };
    if (!dispatchBoot(r4p_contract.USB_HID_BOOT_OP_CLASSIFY_INTERFACE, &op)) return null;
    if (op.result != r4p_contract.USB_HID_BOOT_RESULT_OK) return null;
    boot_r4p_classify +%= 1;
    return switch (op.kind) {
        r4p_contract.USB_HID_BOOT_KIND_KEYBOARD => .keyboard,
        r4p_contract.USB_HID_BOOT_KIND_MOUSE => .mouse,
        else => null,
    };
}

fn candidateHasInterruptEndpoint(candidate: HidInterfaceCandidate) bool {
    return (candidate.endpoint_address & 0x80) != 0 and candidate.endpoint_max_packet != 0;
}

fn hidProtocolRolesReady() bool {
    return r4p.hasActiveR4p("usb.hid_report") and r4p.hasActiveR4p("usb.hid_boot");
}

fn recordRequiredProtocolBlock(candidate: HidInterfaceCandidate) void {
    if (candidate.class_code != 0x03 or candidate.subclass != 0x01) return;
    if (candidate.protocol != 0x01 and candidate.protocol != 0x02) return;
    if (!candidateHasInterruptEndpoint(candidate)) return;
    current.protocol_required_missing +|= 1;
    current.reason = missingProtocolReason();
}

fn missingProtocolReason() []const u8 {
    const report_missing = !r4p.hasActiveR4p("usb.hid_report");
    const boot_missing = !r4p.hasActiveR4p("usb.hid_boot");
    if (report_missing and boot_missing) return "HIDREPORT.R4P and USBHID.R4P required";
    if (report_missing) return "HIDREPORT.R4P required";
    if (boot_missing) return "USBHID.R4P required";
    return "USB HID R4P required";
}

fn dispatchBoot(opcode: u32, op: *r4p_contract.UsbHidBootOp) bool {
    var buffer = protocol_api.ProtocolBuffer{
        .data = op,
        .len = @sizeOf(r4p_contract.UsbHidBootOp),
        .capacity = @sizeOf(r4p_contract.UsbHidBootOp),
        .flags = 0,
        .reserved = 0,
    };
    var out: protocol_api.ProtocolBuffer = .{};
    const result = r4p.dispatch("usb.hid_boot", opcode, &buffer, &out);
    boot_last_result = result;
    if (result != r4p_contract.USB_HID_BOOT_RESULT_OK and result != r4p_contract.USB_HID_BOOT_RESULT_IGNORED) {
        boot_dispatch_failures +%= 1;
        return false;
    }
    return true;
}

pub fn status() Status {
    refreshAggregateStatus();
    return current;
}

pub fn diagnosticPoll() void {
    servicePoll();
}

pub fn servicePoll() void {
    service_polls +%= 1;
    pollKeyboard();
    pollMouse();
}

// 0.56.17: Autonomer Input-Poll-Task (Befund 7.1). Bisher trieben nur die
// poll_hooks in keyboard.readChar/mouse.snapshotForApi das USB-HID-Polling -
// ohne Konsumenten stand die Eingabe still. Der Task pollt selbst im
// 1-Tick-Takt (10 ms, passend zum typischen Endpoint-bInterval von 8-10 ms);
// die poll_hooks bleiben als Fallback bestehen. Start aus main.zig NACH
// initTaskRuntime (task.init() wischt fruehere Threads).
const POLL_TASK_MARKER_FIRST: u64 = 300; // erster Marker nach ~3 s
const POLL_TASK_MARKER_INTERVAL: u64 = 6000; // danach alle ~60 s

var poll_task_started = false;
var poll_task_id: u32 = 0;
var poll_task_iterations: u64 = 0;

pub const PollTaskSummary = struct {
    started: bool = false,
    task_id: u32 = 0,
    iterations: u64 = 0,
};

pub fn pollTaskSummary() PollTaskSummary {
    return .{
        .started = poll_task_started,
        .task_id = poll_task_id,
        .iterations = poll_task_iterations,
    };
}

pub fn startPollTask() bool {
    if (poll_task_started) return true;
    const worker = sched_task.createKernelThreadBlockedWithRole("usb-hid-poll", pollTaskMain, .input) orelse {
        k.puts("USBHID poll-task create failed\r\n");
        return false;
    };
    poll_task_started = true;
    poll_task_id = worker.id;
    sched_task.markReady(worker, timer_core.tickCount());
    k.puts("USBHID poll-task started id=");
    k.putDec(worker.id);
    k.puts("\r\n");
    return true;
}

fn pollTaskMain() callconv(.c) void {
    while (true) {
        servicePoll();
        poll_task_iterations +%= 1;
        if (poll_task_iterations == POLL_TASK_MARKER_FIRST or
            (poll_task_iterations > POLL_TASK_MARKER_FIRST and
                (poll_task_iterations - POLL_TASK_MARKER_FIRST) % POLL_TASK_MARKER_INTERVAL == 0))
        {
            emitPollTaskMarker();
        }
        // 0.56.29: 10 ms Echtzeit-Raster (wie 1 Tick bei 100 Hz) - bei
        // 1000 Hz wuerde 1 Tick sonst 1000 MMIO-Pollrunden/s bedeuten.
        scheduler.sleepTicksWithReason(hid_poll_idle_ticks, "usb-hid-poll-idle");
    }
}

// COM1-Marker: belegt autonomes Polling ohne Konsumenten und verifiziert
// den 0.56.16-Halted-Check unter Dauer-Polling (halted_checks>0 bei
// weiterhin aktiven Bindings = Output-Context-Offset korrekt).
fn emitPollTaskMarker() void {
    const xs = xhci.status();
    const keys = keyboard.stats();
    k.puts("[USBHIDPOLL] service_polls=");
    k.putDec(service_polls);
    k.puts(" interrupt_polls=");
    k.putDec(xs.interrupt_polls);
    k.puts(" reports=");
    k.putDec(xs.interrupt_reports);
    k.puts(" no_report=");
    k.putDec(xs.interrupt_no_report);
    k.puts(" halted_checks=");
    k.putDec(xs.interrupt_halted_checks);
    k.puts(" recoveries=");
    k.putDec(xs.interrupt_recoveries);
    k.puts(" pending_timeouts=");
    k.putDec(xs.interrupt_pending_timeouts);
    k.puts(" ring_enqueue=");
    k.putDec(xs.interrupt_enqueue);
    k.puts(" ring_pcs=");
    k.putDec(xs.interrupt_cycle);
    k.puts(" link_c=");
    k.putDec(xs.interrupt_link_cycle);
    k.puts(" ring_wraps=");
    k.putDec(xs.interrupt_ring_wraps);
    k.puts(" pending=");
    k.putDec(@intFromBool(xs.interrupt_pending));
    k.puts(" pending_index=");
    k.putDec(xs.interrupt_pending_index);
    k.puts(" pending_streak=");
    k.putDec(xs.interrupt_pending_streak);
    k.puts(" ep_state=");
    k.putDec(xs.last_interrupt_ep_state);
    k.puts(" event_wraps=");
    k.putDec(xs.event_ring_wraps);
    k.puts(" events=");
    k.putDec(xs.events);
    k.puts(" event_dequeue=");
    k.putDec(xs.event_dequeue);
    k.puts(" event_ccs=");
    k.putDec(xs.event_cycle);
    k.puts(" event_raw_c=");
    k.putDec(xs.event_raw_cycle);
    k.puts(" event_raw_type=");
    k.putDec(xs.event_raw_type);
    k.puts(" event_raw_code=");
    k.putDec(xs.event_raw_code);
    k.puts(" event_commits=");
    k.putDec(xs.event_erdp_commits);
    k.puts(" event_max_batch=");
    k.putDec(xs.event_max_batch);
    k.puts(" event_tick_waits=");
    k.putDec(xs.event_tick_deadline_waits);
    k.puts(" event_hpet_waits=");
    k.putDec(xs.event_hpet_deadline_waits);
    k.puts(" event_tsc_waits=");
    k.putDec(xs.event_tsc_deadline_waits);
    k.puts(" event_hpet_timeouts=");
    k.putDec(xs.event_hpet_timeouts);
    k.puts(" event_tsc_timeouts=");
    k.putDec(xs.event_tsc_timeouts);
    k.puts(" event_cpu_guard_timeouts=");
    k.putDec(xs.event_cpu_guard_timeouts);
    k.puts(" ring_full=");
    k.putDec(xs.ring_full);
    k.puts(" stale_events=");
    k.putDec(xs.stale_events);
    k.puts(" port_events=");
    k.putDec(xs.port_change_events);
    k.puts(" port_pending=");
    k.putDec(xs.port_change_pending);
    k.puts(" port_coalesced=");
    k.putDec(xs.port_change_coalesced);
    k.puts(" keyboard_pending=");
    k.putDec(keys.queue_pending);
    k.puts(" keyboard_free=");
    k.putDec(keys.queue_capacity - keys.queue_pending);
    k.puts(" keyboard_drops=");
    k.putDec(keys.dropped_count);
    k.puts(" backpressure=");
    k.putDec(@intFromBool(keyboard_backpressure_active));
    k.puts(" backpressure_polls=");
    k.putDec(keyboard_backpressure_polls);
    k.puts(" erdp=0x");
    k.putHex(xs.erdp0, 16);
    k.puts(" iman=0x");
    k.putHex(xs.iman0, 8);
    k.puts(" usbsts=0x");
    k.putHex(xs.usbsts, 8);
    k.puts(" hw_dequeue=0x");
    k.putHex(xs.interrupt_hw_dequeue, 16);
    k.puts(" deferred_pending=");
    k.putDec(xs.deferred_events_pending);
    k.puts(" deferred_queued=");
    k.putDec(xs.deferred_events_queued);
    k.puts(" deferred_delivered=");
    k.putDec(xs.deferred_events_delivered);
    k.puts(" deferred_overflows=");
    k.putDec(xs.deferred_events_overflows);
    k.puts(" deferred_purged=");
    k.putDec(xs.deferred_events_purged);
    k.puts("\r\n");
}

fn fillBinding(binding: *HidBinding, dev: *const usb_core.Device, candidate: HidInterfaceCandidate) void {
    const device = xhci.deviceHandleFromCore(dev);
    binding.* = .{
        .present = true,
        .port = dev.port,
        .device = device,
        .endpoint = .{
            .device = device,
            .kind = .interrupt_in,
            .address = candidate.endpoint_address,
            .max_packet = candidate.endpoint_max_packet,
            .interval = candidate.endpoint_interval,
        },
        .interface_number = candidate.number,
        .endpoint_address = candidate.endpoint_address,
        .endpoint_interval = candidate.endpoint_interval,
        .endpoint_max_packet = candidate.endpoint_max_packet,
        .report_descriptor_len = candidate.report_descriptor_len,
    };
}

fn bindDevice(binding: *HidBinding, kind: HidKind) bool {
    if (!xhci.selectDeviceHandle(&binding.device)) {
        binding.failures += 1;
        current.reason = switch (kind) {
            .keyboard => "USB HID keyboard select failed",
            .mouse => "USB HID mouse select failed",
        };
        return false;
    }
    binding.bound = bindInterruptInput(binding);
    return binding.bound;
}

fn bindInterruptInput(binding: *HidBinding) bool {
    if (!xhci.setConfigurationForHandle(&binding.device)) {
        binding.setup_warnings += 1;
    }
    if (!xhci.setHidBootProtocolForHandle(&binding.device, binding.interface_number)) {
        binding.setup_warnings += 1;
    } else {
        binding.protocol_ok = true;
    }
    if (!xhci.setHidIdleForHandle(&binding.device, binding.interface_number)) {
        binding.setup_warnings += 1;
    } else {
        binding.idle_ok = true;
    }
    readReportDescriptor(binding);
    if (!xhci.configureInterruptInEndpointHandle(&binding.endpoint)) {
        binding.failures += 1;
        current.reason = "interrupt endpoint configure failed";
        return false;
    }
    return true;
}

fn readReportDescriptor(binding: *HidBinding) void {
    if (binding.report_descriptor_len == 0) return;
    var report_desc: [REPORT_DESCRIPTOR_MAX]u8 = .{0} ** REPORT_DESCRIPTOR_MAX;
    const actual = xhci.getHidReportDescriptorForHandle(&binding.device, binding.interface_number, binding.report_descriptor_len, report_desc[0..]) orelse {
        binding.setup_warnings += 1;
        return;
    };
    binding.report_descriptor_read_len = @intCast(actual);
    binding.report_summary = hid_report.parse(report_desc[0..actual]);
    binding.report_descriptor_ok = binding.report_summary.parsed and !binding.report_summary.malformed;
    binding.report_descriptor_malformed = binding.report_summary.malformed;
    if (binding.report_summary.has_report_id) binding.report_id_heuristic = true;
}

fn pollHook() callconv(.c) void {
    pollKeyboard();
}

fn mousePollHook() callconv(.c) void {
    pollMouse();
}

fn pollKeyboard() void {
    _ = pollBinding(&keyboard_binding, .keyboard);
}

fn pollMouse() void {
    var count: usize = 0;
    while (count < MOUSE_POLL_BUDGET) : (count += 1) {
        if (!pollBinding(&mouse_binding, .mouse)) break;
    }
}

fn pollBinding(binding: *HidBinding, kind: HidKind) bool {
    if (!binding.bound) return false;
    if (kind == .keyboard) {
        if (!keyboard.canAccept(KEYBOARD_REPORT_QUEUE_RESERVE)) {
            keyboard_backpressure_active = true;
            keyboard_backpressure_polls +%= 1;
            return false;
        }
        keyboard_backpressure_active = false;
    }
    if (!xhci.acquireControllerOwnership()) {
        binding.failures += 1;
        return false;
    }
    defer xhci.releaseControllerOwnership();
    if (!ensureSelected(binding, kind)) return false;
    var report: [REPORT_BUFFER_LEN]u8 = .{0} ** REPORT_BUFFER_LEN;
    const report_len = reportTransferLength(binding);
    binding.polls += 1;
    var host_endpoint = xhci.usbHostEndpointHandle(binding.endpoint);
    var actual_len: u32 = 0;
    switch (usb_host.interruptTransfer(&host_endpoint, report[0..report_len], &actual_len)) {
        0 => {
            binding.no_reports += 1;
            return false;
        },
        1 => {},
        else => {
            binding.failures += 1;
            return false;
        },
    }
    const actual_report_len = if (actual_len == 0 or actual_len > report_len)
        actualReportLength(report_len)
    else
        @as(usize, @intCast(actual_len));
    binding.last_report_request_len = clippedU8(xhci.lastInterruptRequestedLength());
    binding.last_report_residue = xhci.lastInterruptResidue();
    binding.reports += 1;
    if (reportHasNonzero(report[0..actual_report_len])) {
        binding.last_nonzero_report = report;
        binding.last_nonzero_report_len = @intCast(actual_report_len);
    }
    if (shouldSuppressDuplicate(binding, kind, report[0..actual_report_len])) {
        binding.duplicate_reports += 1;
        return true;
    }
    switch (kind) {
        .keyboard => decodeKeyboardReport(binding, report[0..actual_report_len]),
        .mouse => decodeMouseReport(binding, report[0..actual_report_len]),
    }
    binding.last_report = report;
    binding.last_report_len = @intCast(actual_report_len);
    return true;
}

fn reportTransferLength(binding: *const HidBinding) usize {
    var len: usize = binding.endpoint_max_packet;
    if (len < 8) len = 8;
    if (len > REPORT_BUFFER_LEN) len = REPORT_BUFFER_LEN;
    return len;
}

fn actualReportLength(requested_len: usize) usize {
    const actual = xhci.lastInterruptActualLength();
    if (actual == 0 or actual > requested_len) return requested_len;
    return actual;
}

fn clippedU8(value: usize) u8 {
    if (value > 255) return 255;
    return @intCast(value);
}

fn ensureSelected(binding: *HidBinding, kind: HidKind) bool {
    const xs = xhci.status();
    const exact_runtime = binding.device.slot_id != 0 and
        xs.addressed_slot_id == binding.device.slot_id and
        xs.addressed_port == binding.device.port;
    if (exact_runtime and
        xs.interrupt_endpoint_configured and
        !xs.interrupt_endpoint_faulted and
        xs.interrupt_endpoint_address == binding.endpoint_address)
        return true;
    if (!xhci.selectDeviceHandle(&binding.device)) {
        binding.failures += 1;
        current.reason = switch (kind) {
            .keyboard => "USB HID keyboard select failed",
            .mouse => "USB HID mouse select failed",
        };
        return false;
    }
    const selected = xhci.status();
    if (selected.addressed_slot_id == binding.device.slot_id and
        selected.addressed_port == binding.device.port and
        selected.interrupt_endpoint_configured and
        !selected.interrupt_endpoint_faulted and
        selected.interrupt_endpoint_address == binding.endpoint_address)
    {
        return true;
    }
    return bindInterruptInput(binding);
}

fn decodeKeyboardReport(binding: *HidBinding, report: []const u8) void {
    if (decodeKeyboardReportR4p(binding, report)) return;
    binding.failures += 1;
    current.reason = "USB HID keyboard R4P decode failed";
}

fn decodeMouseReport(binding: *const HidBinding, report: []const u8) void {
    _ = binding;
    if (decodeMouseReportR4p(report)) return;
    current.reason = "USB HID mouse R4P decode failed";
}

fn decodeKeyboardReportR4p(binding: *HidBinding, report: []const u8) bool {
    if (!r4p.hasActiveR4p("usb.hid_boot")) return false;
    if (report.len > r4p_contract.USB_HID_BOOT_MAX_REPORT or binding.last_report_len > r4p_contract.USB_HID_BOOT_MAX_REPORT) return false;
    var op: r4p_contract.UsbHidBootOp = .{
        .protocol_ok = if (binding.protocol_ok) 1 else 0,
        .report_len = @intCast(report.len),
        .previous_len = binding.last_report_len,
    };
    if (report.len != 0) @memcpy(op.report[0..report.len], report);
    const prev_len: usize = binding.last_report_len;
    if (prev_len != 0) @memcpy(op.previous[0..prev_len], binding.last_report[0..prev_len]);
    if (!dispatchBoot(r4p_contract.USB_HID_BOOT_OP_DECODE_KEYBOARD, &op)) return false;
    if (op.result != r4p_contract.USB_HID_BOOT_RESULT_OK) return false;
    boot_r4p_keyboard +%= 1;
    if ((op.flags & r4p_contract.USB_HID_BOOT_FLAG_REPORT_ID_HEURISTIC) != 0) {
        binding.report_id_heuristic = true;
        current.report_id_heuristic = true;
    }
    handleModifiers(op.old_modifiers, op.new_modifiers);
    current.last_modifiers = op.new_modifiers;
    var i: usize = 0;
    while (i < op.key_count and i < op.keys.len) : (i += 1) {
        const usage = op.keys[i];
        if (usage == 0) continue;
        current.last_usage = usage;
        if (injectUsage(usage)) {
            current.decoded_keys += 1;
        } else {
            current.drops += 1;
        }
    }
    return true;
}

fn decodeMouseReportR4p(report: []const u8) bool {
    if (!r4p.hasActiveR4p("usb.hid_boot")) return false;
    if (report.len > r4p_contract.USB_HID_BOOT_MAX_REPORT) return false;
    var op: r4p_contract.UsbHidBootOp = .{ .report_len = @intCast(report.len) };
    if (report.len != 0) @memcpy(op.report[0..report.len], report);
    if (!dispatchBoot(r4p_contract.USB_HID_BOOT_OP_DECODE_MOUSE, &op)) return false;
    if (op.result != r4p_contract.USB_HID_BOOT_RESULT_OK) return false;
    boot_r4p_mouse +%= 1;
    current.last_mouse_buttons = op.mouse_buttons;
    current.last_mouse_dx = op.mouse_dx;
    current.last_mouse_dy = op.mouse_dy;
    current.last_mouse_wheel = op.mouse_wheel;
    mouse.injectRelativePacketWheel(op.mouse_dx, op.mouse_dy, op.mouse_buttons, op.mouse_wheel);
    current.decoded_mouse += 1;
    return true;
}

fn handleModifiers(old_mods: u8, new_mods: u8) void {
    updateModifier(old_mods, new_mods, 0x02, 0x2A, 0xAA); // left shift
    updateModifier(old_mods, new_mods, 0x20, 0x36, 0xB6); // right shift
    updateModifier(old_mods, new_mods, 0x01, 0x1D, 0x9D); // left ctrl
    updateModifier(old_mods, new_mods, 0x10, 0x1D, 0x9D); // right ctrl
    updateModifier(old_mods, new_mods, 0x04, 0x38, 0xB8); // left alt
    updateModifier(old_mods, new_mods, 0x40, 0x38, 0xB8); // right alt
    updateExtendedModifier(old_mods, new_mods, 0x08, 0x5B, 0xDB); // left GUI / Windows
    updateExtendedModifier(old_mods, new_mods, 0x80, 0x5C, 0xDC); // right GUI / Windows
}

fn updateModifier(old_mods: u8, new_mods: u8, bit: u8, make: u8, break_code: u8) void {
    const was = (old_mods & bit) != 0;
    const is = (new_mods & bit) != 0;
    if (was == is) return;
    keyboard.injectScancode(if (is) make else break_code);
}

fn updateExtendedModifier(old_mods: u8, new_mods: u8, bit: u8, make: u8, break_code: u8) void {
    const was = (old_mods & bit) != 0;
    const is = (new_mods & bit) != 0;
    if (was == is) return;
    keyboard.injectScancode(0xE0);
    keyboard.injectScancode(if (is) make else break_code);
}

fn injectUsage(usage: u8) bool {
    if (usageToSet1(usage)) |scancode| {
        keyboard.injectScancode(scancode);
        return true;
    }
    if (usageToExtendedSet1(usage)) |scancode| {
        keyboard.injectScancode(0xE0);
        keyboard.injectScancode(scancode);
        return true;
    }
    return false;
}

fn usageToSet1(usage: u8) ?u8 {
    return switch (usage) {
        0x04 => 0x1E, // A
        0x05 => 0x30,
        0x06 => 0x2E,
        0x07 => 0x20,
        0x08 => 0x12,
        0x09 => 0x21,
        0x0A => 0x22,
        0x0B => 0x23,
        0x0C => 0x17,
        0x0D => 0x24,
        0x0E => 0x25,
        0x0F => 0x26,
        0x10 => 0x32,
        0x11 => 0x31,
        0x12 => 0x18,
        0x13 => 0x19,
        0x14 => 0x10,
        0x15 => 0x13,
        0x16 => 0x1F,
        0x17 => 0x14,
        0x18 => 0x16,
        0x19 => 0x2F,
        0x1A => 0x11,
        0x1B => 0x2D,
        0x1C => 0x15,
        0x1D => 0x2C, // Z
        0x1E => 0x02, // 1
        0x1F => 0x03,
        0x20 => 0x04,
        0x21 => 0x05,
        0x22 => 0x06,
        0x23 => 0x07,
        0x24 => 0x08,
        0x25 => 0x09,
        0x26 => 0x0A,
        0x27 => 0x0B, // 0
        0x28 => 0x1C, // Enter
        0x29 => 0x01, // Escape
        0x2A => 0x0E, // Backspace
        0x2B => 0x0F, // Tab
        0x2C => 0x39, // Space
        0x2D => 0x0C,
        0x2E => 0x0D,
        0x2F => 0x1A,
        0x30 => 0x1B,
        0x31 => 0x2B,
        0x33 => 0x27,
        0x34 => 0x28,
        0x35 => 0x29,
        0x36 => 0x33,
        0x37 => 0x34,
        0x38 => 0x35,
        0x3C => 0x3D, // F3
        0x3D => 0x3E, // F4
        else => null,
    };
}

fn usageToExtendedSet1(usage: u8) ?u8 {
    return switch (usage) {
        0x4F => 0x4D, // right
        0x50 => 0x4B, // left
        0x51 => 0x50, // down
        0x52 => 0x48, // up
        0x4A => 0x47, // home
        0x4D => 0x4F, // end
        0x4B => 0x49, // page up
        0x4E => 0x51, // page down
        0x4C => 0x53, // delete
        else => null,
    };
}

fn refreshAggregateStatus() void {
    const keyboard_queue = keyboard.stats();
    current.keyboard_present = keyboard_binding.present;
    current.keyboard_bound = keyboard_binding.bound;
    current.mouse_present = mouse_binding.present;
    current.mouse_bound = mouse_binding.bound;
    current.polls = keyboard_binding.polls + mouse_binding.polls;
    current.no_reports = keyboard_binding.no_reports + mouse_binding.no_reports;
    current.reports = keyboard_binding.reports + mouse_binding.reports;
    current.duplicate_reports = keyboard_binding.duplicate_reports + mouse_binding.duplicate_reports;
    current.service_polls = service_polls;
    current.keyboard_backpressure_active = keyboard_backpressure_active;
    current.keyboard_backpressure_polls = keyboard_backpressure_polls;
    current.keyboard_queue_capacity = keyboard_queue.queue_capacity;
    current.keyboard_queue_pending = keyboard_queue.queue_pending;
    current.keyboard_queue_free = keyboard_queue.queue_capacity - keyboard_queue.queue_pending;
    current.keyboard_queue_drops = keyboard_queue.dropped_count;
    current.topology_reconciles = topology_reconciles;
    current.failures = keyboard_binding.failures + mouse_binding.failures;
    current.setup_warnings = keyboard_binding.setup_warnings +| mouse_binding.setup_warnings;
    current.report_id_heuristic = current.report_id_heuristic or keyboard_binding.report_id_heuristic or mouse_binding.report_id_heuristic;
    current.report_descriptor_ok = reportDescriptorAggregateOk();
    current.report_descriptor_malformed = keyboard_binding.report_descriptor_malformed or mouse_binding.report_descriptor_malformed;
    current.last_report_len = @max(keyboard_binding.last_report_len, mouse_binding.last_report_len);
    current.last_nonzero_report_len = @max(keyboard_binding.last_nonzero_report_len, mouse_binding.last_nonzero_report_len);
    current.last_report_request_len = @max(keyboard_binding.last_report_request_len, mouse_binding.last_report_request_len);
    current.last_report_residue = @max(keyboard_binding.last_report_residue, mouse_binding.last_report_residue);
    current.boot_source = r4p.requiredSourceName("usb.hid_boot");
    current.boot_r4p_classify = boot_r4p_classify;
    current.boot_r4p_keyboard = boot_r4p_keyboard;
    current.boot_r4p_mouse = boot_r4p_mouse;
    current.boot_dispatch_failures = boot_dispatch_failures;
    current.boot_last_result = boot_last_result;
    current.protocol_ok = aggregateOk(.protocol);
    current.idle_ok = aggregateOk(.idle);
    const primary = if (keyboard_binding.bound or keyboard_binding.present) keyboard_binding else mouse_binding;
    current.port = primary.port;
    current.interface_number = primary.interface_number;
    current.endpoint_address = primary.endpoint_address;
    current.endpoint_interval = primary.endpoint_interval;
    current.endpoint_max_packet = primary.endpoint_max_packet;
}

fn reportDescriptorAggregateOk() bool {
    var have_report = false;
    if (keyboard_binding.bound and keyboard_binding.report_descriptor_read_len != 0) {
        have_report = true;
        if (!keyboard_binding.report_descriptor_ok) return false;
    }
    if (mouse_binding.bound and mouse_binding.report_descriptor_read_len != 0) {
        have_report = true;
        if (!mouse_binding.report_descriptor_ok) return false;
    }
    return have_report;
}

fn aggregateOk(flag: AggregateFlag) bool {
    var have_bound = false;
    if (keyboard_binding.bound) {
        have_bound = true;
        if (flag == .protocol and !keyboard_binding.protocol_ok) return false;
        if (flag == .idle and !keyboard_binding.idle_ok) return false;
    }
    if (mouse_binding.bound) {
        have_bound = true;
        if (flag == .protocol and !mouse_binding.protocol_ok) return false;
        if (flag == .idle and !mouse_binding.idle_ok) return false;
    }
    return have_bound;
}

fn reportsEqual(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var i: usize = 0;
    while (i < a.len) : (i += 1) {
        if (a[i] != b[i]) return false;
    }
    return true;
}

fn shouldSuppressDuplicate(binding: *const HidBinding, kind: HidKind, report: []const u8) bool {
    if (!reportsEqual(report, binding.last_report[0..binding.last_report_len])) return false;
    return switch (kind) {
        .keyboard => true,
        .mouse => !reportHasNonzero(report),
    };
}

fn reportHasNonzero(report: []const u8) bool {
    var i: usize = 0;
    while (i < report.len) : (i += 1) {
        if (report[i] != 0) return true;
    }
    return false;
}
