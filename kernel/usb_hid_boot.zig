// USB HID boot boundary for boot-adjacent keyboard and mouse bindings.

const bootlog = @import("bootlog.zig");
const driver_registry = @import("../driver/registry.zig");
const fatal = @import("fatal.zig");
const k = @import("log.zig");
const storage_boot = @import("storage_boot.zig");
const usb_hid = @import("../driver/usb/hid.zig");
const xhci = @import("../driver/usb/xhci.zig");

pub const Phase = enum {
    not_started,
    dependency_missing,
    no_hid,
    keyboard_bound,
    mouse_bound,
    keyboard_mouse_bound,
    warning,
    failed,
};

pub const RegistryState = enum {
    none,
    loaded,
    initialized,
    active,
    failed,
};

pub const Status = struct {
    phase: Phase = .not_started,
    attempted: bool = false,
    initialized: bool = false,
    init_returned_bound: bool = false,
    active: bool = false,
    failed: bool = false,
    fatal: bool = false,
    registry_slot_present: bool = false,
    registry_state: RegistryState = .none,
    keyboard_present: bool = false,
    keyboard_bound: bool = false,
    mouse_present: bool = false,
    mouse_bound: bool = false,
    unsupported_hid: u8 = 0,
    missing_endpoint_hid: u8 = 0,
    setup_warnings: u8 = 0,
    failures: u64 = 0,
    boot_dispatch_failures: u64 = 0,
    protocol_required_missing: u8 = 0,
    report_descriptor_malformed: bool = false,
    boot_source: []const u8 = "none",
    reason: []const u8 = "not started",
};

var current: Status = .{};

pub fn init() bool {
    if (current.attempted) {
        bootlog.puts("[USBHIDBOOT] init skipped phase=");
        bootlog.puts(phaseName(current.phase));
        bootlog.puts(" fatal=");
        bootlog.puts(yesNo(current.fatal));
        bootlog.puts("\r\n");
        return !current.fatal;
    }

    current = .{
        .attempted = true,
        .reason = "starting",
    };

    if (!storage_boot.isControllersInitialized()) {
        return failFatal("USB-HID boot before storage controllers", "storage controllers missing");
    }

    const slot = driver_registry.beginLoad("USBHID", 3, 1);
    current.registry_slot_present = slot != null;
    current.registry_state = if (slot != null) .loaded else .none;
    if (slot == null) {
        bootlog.puts("[USBHIDBOOT][WARN] registry slot missing; continuing without registry owner\r\n");
    }

    current.init_returned_bound = usb_hid.init();
    classify(usb_hid.status());
    applyRegistryState(slot);
    dumpStatusToBootlog();
    emitBootStatusLine();
    return true;
}

pub fn isInitialized() bool {
    return current.attempted and current.initialized and !current.fatal;
}

pub fn isActive() bool {
    return current.active and !current.fatal;
}

pub fn status() Status {
    return current;
}

pub fn phaseName(phase: Phase) []const u8 {
    return switch (phase) {
        .not_started => "not-started",
        .dependency_missing => "dependency-missing",
        .no_hid => "no-hid",
        .keyboard_bound => "keyboard-bound",
        .mouse_bound => "mouse-bound",
        .keyboard_mouse_bound => "keyboard-mouse-bound",
        .warning => "warning",
        .failed => "failed",
    };
}

pub fn registryStateName(state: RegistryState) []const u8 {
    return switch (state) {
        .none => "none",
        .loaded => "loaded",
        .initialized => "initialized",
        .active => "active",
        .failed => "failed",
    };
}

pub fn dumpStatus() void {
    k.puts("USB-HID boot\r\n");
    k.puts("  phase=");
    k.puts(phaseName(current.phase));
    k.puts(" initialized=");
    k.puts(yesNo(current.initialized));
    k.puts(" active=");
    k.puts(yesNo(current.active));
    k.puts(" failed=");
    k.puts(yesNo(current.failed));
    k.puts(" fatal=");
    k.puts(yesNo(current.fatal));
    k.puts(" registry=");
    k.puts(registryStateName(current.registry_state));
    k.puts(" continue=");
    k.puts(yesNo(canContinueBoot()));
    k.puts("\r\n");
    k.puts("  keyboard=");
    k.puts(bindingName(current.keyboard_present, current.keyboard_bound));
    k.puts(" mouse=");
    k.puts(bindingName(current.mouse_present, current.mouse_bound));
    k.puts(" unsupported=");
    k.putDec(current.unsupported_hid);
    k.puts(" missing-ep=");
    k.putDec(current.missing_endpoint_hid);
    k.puts("\r\n");
    k.puts("  setup-warn=");
    k.putDec(current.setup_warnings);
    k.puts(" failures=");
    k.putDec(current.failures);
    k.puts(" r4p-required-missing=");
    k.putDec(current.protocol_required_missing);
    k.puts(" boot-source=");
    k.puts(current.boot_source);
    k.puts(" reason=");
    k.puts(current.reason);
    k.puts("\r\n");
}

fn classify(hid_status: usb_hid.Status) void {
    current.initialized = hid_status.initialized;
    current.keyboard_present = hid_status.keyboard_present;
    current.keyboard_bound = hid_status.keyboard_bound;
    current.mouse_present = hid_status.mouse_present;
    current.mouse_bound = hid_status.mouse_bound;
    current.unsupported_hid = hid_status.unsupported_hid;
    current.missing_endpoint_hid = hid_status.missing_endpoint_hid;
    current.setup_warnings = hid_status.setup_warnings;
    current.failures = hid_status.failures;
    current.boot_dispatch_failures = hid_status.boot_dispatch_failures;
    current.protocol_required_missing = hid_status.protocol_required_missing;
    current.report_descriptor_malformed = hid_status.report_descriptor_malformed;
    current.boot_source = hid_status.boot_source;
    current.reason = hid_status.reason;

    const any_bound = hid_status.keyboard_bound or hid_status.mouse_bound;
    const any_present = hid_status.keyboard_present or hid_status.mouse_present;
    const has_warning = hid_status.setup_warnings != 0 or
        hid_status.missing_endpoint_hid != 0 or
        hid_status.unsupported_hid != 0 or
        hid_status.protocol_required_missing != 0 or
        hid_status.boot_dispatch_failures != 0 or
        hid_status.report_descriptor_malformed;
    const bind_failed = hid_status.failures != 0 or
        (hid_status.keyboard_present and !hid_status.keyboard_bound) or
        (hid_status.mouse_present and !hid_status.mouse_bound);

    current.active = any_bound;
    current.failed = false;
    current.fatal = false;

    if (any_bound) {
        current.phase = if (has_warning)
            .warning
        else if (hid_status.keyboard_bound and hid_status.mouse_bound)
            .keyboard_mouse_bound
        else if (hid_status.keyboard_bound)
            .keyboard_bound
        else
            .mouse_bound;
        return;
    }

    if (bind_failed) {
        current.phase = .failed;
        current.failed = true;
        return;
    }

    if (any_present or hid_status.missing_endpoint_hid != 0 or hid_status.unsupported_hid != 0 or hid_status.protocol_required_missing != 0) {
        current.phase = .warning;
        if (hid_status.missing_endpoint_hid != 0) current.reason = "USB HID present without usable interrupt-in endpoint";
        if (hid_status.unsupported_hid != 0 and hid_status.missing_endpoint_hid == 0) current.reason = "USB HID present but unsupported for boot input";
        if (hid_status.protocol_required_missing != 0) current.reason = hid_status.reason;
        return;
    }

    current.phase = .no_hid;
    current.reason = "no USB HID boot device found";
}

fn applyRegistryState(slot: ?usize) void {
    if (slot == null) return;

    if (current.failed) {
        driver_registry.setState(slot.?, .failed);
        current.registry_state = .failed;
        return;
    }

    driver_registry.setState(slot.?, .initialized);
    current.registry_state = .initialized;
    if (current.active) {
        driver_registry.setState(slot.?, .active);
        current.registry_state = .active;
    }
}

fn failFatal(message: []const u8, reason: []const u8) bool {
    current.phase = .dependency_missing;
    current.failed = true;
    current.fatal = true;
    current.reason = reason;
    dumpStatusToBootlog();
    return fatal.fail(.usb, message);
}

fn dumpStatusToBootlog() void {
    bootlog.puts("[USBHIDBOOT] status phase=");
    bootlog.puts(phaseName(current.phase));
    bootlog.puts(" outcome=");
    bootlog.puts(outcomeName());
    bootlog.puts(" initialized=");
    bootlog.puts(yesNo(current.initialized));
    bootlog.puts(" init_bound=");
    bootlog.puts(yesNo(current.init_returned_bound));
    bootlog.puts(" active=");
    bootlog.puts(yesNo(current.active));
    bootlog.puts(" failed=");
    bootlog.puts(yesNo(current.failed));
    bootlog.puts(" fatal=");
    bootlog.puts(yesNo(current.fatal));
    bootlog.puts(" registry=");
    bootlog.puts(registryStateName(current.registry_state));
    bootlog.puts(" registry_slot=");
    bootlog.puts(yesNo(current.registry_slot_present));
    bootlog.puts(" boot_continue=");
    bootlog.puts(yesNo(canContinueBoot()));
    bootlog.puts(" keyboard=");
    bootlog.puts(bindingName(current.keyboard_present, current.keyboard_bound));
    bootlog.puts(" mouse=");
    bootlog.puts(bindingName(current.mouse_present, current.mouse_bound));
    bootlog.puts(" unsupported=");
    bootlog.putDec(current.unsupported_hid);
    bootlog.puts(" missing_ep=");
    bootlog.putDec(current.missing_endpoint_hid);
    bootlog.puts(" setup_warn=");
    bootlog.putDec(current.setup_warnings);
    bootlog.puts(" failures=");
    bootlog.putDec(current.failures);
    bootlog.puts(" dispatch_failures=");
    bootlog.putDec(current.boot_dispatch_failures);
    bootlog.puts(" r4p_required_missing=");
    bootlog.putDec(current.protocol_required_missing);
    bootlog.puts(" source=");
    bootlog.puts(current.boot_source);
    bootlog.puts(" reason=");
    bootlog.puts(current.reason);
    // 0.56.16: Halt-Recovery-Zaehler des built-in xHCI sichtbar machen
    // (Abnahme: im fehlerfreien QEMU-Lauf recoveries=0, pending_timeouts=0).
    const xs = xhci.status();
    bootlog.puts(" xhci_recoveries=");
    bootlog.putDec(xs.interrupt_recoveries);
    bootlog.puts(" xhci_pending_timeouts=");
    bootlog.putDec(xs.interrupt_pending_timeouts);
    bootlog.puts(" xhci_halted_checks=");
    bootlog.putDec(xs.interrupt_halted_checks);
    bootlog.puts("\r\n");
}

// 0.56.17: Autonomen Input-Poll-Task starten - nur wenn USB-HID aktiv
// gebunden ist (ohne Bindings kein Task-Slot-Verbrauch). Aufruf aus
// main.zig NACH initTaskRuntime.
pub fn startPollTask() bool {
    if (!current.active) return true;
    return usb_hid.startPollTask();
}

fn emitBootStatusLine() void {
    k.puts("  USBHID ");
    switch (current.phase) {
        .no_hid => k.puts("[not found]\r\n"),
        .keyboard_bound => k.puts("keyboard [active]\r\n"),
        .mouse_bound => k.puts("mouse [active]\r\n"),
        .keyboard_mouse_bound => k.puts("keyboard+mouse [active]\r\n"),
        .warning => {
            if (current.active) {
                k.puts(bindingSummary());
                k.puts(" [warn]\r\n");
            } else {
                k.puts("[warn]\r\n");
            }
        },
        .failed => k.puts("[failed]\r\n"),
        .dependency_missing => k.puts("[fatal]\r\n"),
        .not_started => k.puts("[not started]\r\n"),
    }
    // 0.56.16: Halt-Recovery-Zaehler auf COM1 (Bootlog allein landet nicht
    // im Test-Log - Lektion aus dem [LAPIC]-Marker). Abnahme-Kriterium:
    // recoveries=0 und pending_timeouts=0 im fehlerfreien QEMU-Lauf.
    if (current.active) {
        const xs = xhci.status();
        k.puts("  USBHID xhci: recoveries=");
        k.putDec(xs.interrupt_recoveries);
        k.puts(" pending_timeouts=");
        k.putDec(xs.interrupt_pending_timeouts);
        k.puts(" halted_checks=");
        k.putDec(xs.interrupt_halted_checks);
        k.puts("\r\n");
    }
}

fn bindingSummary() []const u8 {
    if (current.keyboard_bound and current.mouse_bound) return "keyboard+mouse";
    if (current.keyboard_bound) return "keyboard";
    if (current.mouse_bound) return "mouse";
    return "no-binding";
}

fn bindingName(present: bool, bound: bool) []const u8 {
    if (bound) return "bound";
    if (present) return "detected";
    return "no";
}

fn outcomeName() []const u8 {
    if (current.fatal) return "fatal";
    if (current.failed) return "failed";
    if (current.active) return "active";
    if (current.phase == .no_hid) return "not-found";
    if (current.phase == .warning) return "warning";
    return phaseName(current.phase);
}

fn canContinueBoot() bool {
    return !current.fatal;
}

fn yesNo(value: bool) []const u8 {
    return if (value) "yes" else "no";
}
