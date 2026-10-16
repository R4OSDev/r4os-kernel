//! Original boot-HID sequencing/classification. Its sole status is resident;
//! peers borrow only copied projections, never a mutable status reference.
use super::hid_boot_types::Status;
use crate::{
    boot::{fatal, foundation, log},
    driver::registry,
    sched::sync_types::Text,
};
use core::{cell::UnsafeCell, mem::MaybeUninit, slice};
#[repr(C)]
pub struct XhciStatus {
    pub interrupt_recoveries: u64,
    pub interrupt_pending_timeouts: u64,
    pub interrupt_halted_checks: u64,
}
#[repr(C)]
pub struct Peers {
    pub storage_ready: unsafe extern "C" fn() -> bool,
    pub hid_init: unsafe extern "C" fn() -> bool,
    pub hid_status: unsafe extern "C" fn(*mut Status),
    pub xhci_status: unsafe extern "C" fn(*mut XhciStatus),
    pub port_start: unsafe extern "C" fn() -> bool,
    pub hid_start: unsafe extern "C" fn() -> bool,
}
struct Storage(UnsafeCell<Status>);
// SAFETY: status is initialized only by the linear BSP bootstrap. Afterwards
// it is immutable; starting resident poll tasks never mutates this record.
unsafe impl Sync for Storage {}
static STATUS: Storage = Storage(UnsafeCell::new(Status::EMPTY));
fn status() -> Status {
    unsafe { STATUS.0.get().read() }
}
fn publish(value: Status) {
    unsafe { STATUS.0.get().write(value) }
}
fn text(value: Text) -> &'static [u8] {
    if value.len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(value.pointer, value.len) }
    }
}
fn yes_no(value: bool) -> &'static [u8] {
    if value { b"yes" } else { b"no" }
}
fn phase_name(value: u8) -> &'static [u8] {
    match value {
        0 => b"not-started",
        1 => b"dependency-missing",
        2 => b"no-hid",
        3 => b"keyboard-bound",
        4 => b"mouse-bound",
        5 => b"keyboard-mouse-bound",
        6 => b"warning",
        7 => b"failed",
        _ => panic!("USB HID boot phase"),
    }
}
fn registry_state_name(value: u8) -> &'static [u8] {
    match value {
        0 => b"none",
        1 => b"loaded",
        2 => b"initialized",
        3 => b"active",
        4 => b"failed",
        _ => panic!("USB HID registry state"),
    }
}
fn binding_name(present: bool, bound: bool) -> &'static [u8] {
    if bound {
        b"bound"
    } else if present {
        b"detected"
    } else {
        b"no"
    }
}
fn binding_summary(value: Status) -> &'static [u8] {
    if value.keyboard_bound && value.mouse_bound {
        b"keyboard+mouse"
    } else if value.keyboard_bound {
        b"keyboard"
    } else if value.mouse_bound {
        b"mouse"
    } else {
        b"no-binding"
    }
}
fn outcome_name(value: Status) -> &'static [u8] {
    if value.fatal {
        b"fatal"
    } else if value.failed {
        b"failed"
    } else if value.active {
        b"active"
    } else if value.phase == 2 {
        b"not-found"
    } else if value.phase == 6 {
        b"warning"
    } else {
        phase_name(value.phase)
    }
}
struct Sink(bool);
impl Sink {
    fn text(&self, bytes: &[u8]) {
        if self.0 {
            unsafe { log::r4os_rust_bootlog_write(bytes.as_ptr(), bytes.len()) };
        } else {
            foundation::puts(bytes);
        }
    }
    fn dec(&self, value: u64) {
        if self.0 {
            log::r4os_rust_bootlog_dec(value);
        } else {
            foundation::dec(value);
        }
    }
}
fn xhci_status(peers: &Peers) -> XhciStatus {
    let mut out = MaybeUninit::uninit();
    unsafe {
        (peers.xhci_status)(out.as_mut_ptr());
        out.assume_init()
    }
}
fn classify(input: Status) {
    let mut value = status();
    value.initialized = input.initialized;
    value.keyboard_present = input.keyboard_present;
    value.keyboard_bound = input.keyboard_bound;
    value.mouse_present = input.mouse_present;
    value.mouse_bound = input.mouse_bound;
    value.unsupported_hid = input.unsupported_hid;
    value.missing_endpoint_hid = input.missing_endpoint_hid;
    value.setup_warnings = input.setup_warnings;
    value.failures = input.failures;
    value.boot_dispatch_failures = input.boot_dispatch_failures;
    value.protocol_required_missing = input.protocol_required_missing;
    value.report_descriptor_malformed = input.report_descriptor_malformed;
    value.boot_source = input.boot_source;
    value.reason = input.reason;
    let any_bound = input.keyboard_bound || input.mouse_bound;
    let any_present = input.keyboard_present || input.mouse_present;
    let warning = input.setup_warnings != 0
        || input.missing_endpoint_hid != 0
        || input.unsupported_hid != 0
        || input.protocol_required_missing != 0
        || input.boot_dispatch_failures != 0
        || input.report_descriptor_malformed;
    let failed = input.failures != 0
        || (input.keyboard_present && !input.keyboard_bound)
        || (input.mouse_present && !input.mouse_bound);
    value.active = any_bound;
    value.failed = false;
    value.fatal = false;
    if any_bound {
        value.phase = if warning {
            6
        } else if input.keyboard_bound && input.mouse_bound {
            5
        } else if input.keyboard_bound {
            3
        } else {
            4
        };
        publish(value);
        return;
    }
    if failed {
        value.phase = 7;
        value.failed = true;
        publish(value);
        return;
    }
    if any_present
        || input.missing_endpoint_hid != 0
        || input.unsupported_hid != 0
        || input.protocol_required_missing != 0
    {
        value.phase = 6;
        if input.missing_endpoint_hid != 0 {
            value.reason = Text::literal(b"USB HID present without usable interrupt-in endpoint");
        }
        if input.unsupported_hid != 0 && input.missing_endpoint_hid == 0 {
            value.reason = Text::literal(b"USB HID present but unsupported for boot input");
        }
        if input.protocol_required_missing != 0 {
            value.reason = input.reason;
        }
        publish(value);
        return;
    }
    value.phase = 2;
    value.reason = Text::literal(b"no USB HID boot device found");
    publish(value);
}
fn apply_registry(slot: usize) {
    if slot == usize::MAX {
        return;
    }
    let mut value = status();
    if value.failed {
        registry::r4os_rust_driver_registry_set_state(slot, 4);
        value.registry_state = 4;
        publish(value);
        return;
    }
    registry::r4os_rust_driver_registry_set_state(slot, 2);
    value.registry_state = 2;
    if value.active {
        registry::r4os_rust_driver_registry_set_state(slot, 3);
        value.registry_state = 3;
    }
    publish(value);
}
fn emit_boot_line(peers: &Peers) {
    let value = status();
    let sink = Sink(false);
    sink.text(b"  USBHID ");
    match value.phase {
        2 => sink.text(b"[not found]\r\n"),
        3 => sink.text(b"keyboard [active]\r\n"),
        4 => sink.text(b"mouse [active]\r\n"),
        5 => sink.text(b"keyboard+mouse [active]\r\n"),
        6 => {
            if value.active {
                sink.text(binding_summary(value));
                sink.text(b" [warn]\r\n");
            } else {
                sink.text(b"[warn]\r\n");
            }
        }
        7 => sink.text(b"[failed]\r\n"),
        1 => sink.text(b"[fatal]\r\n"),
        0 => sink.text(b"[not started]\r\n"),
        _ => panic!("USB HID boot phase"),
    }
    if value.active {
        let xs = xhci_status(peers);
        sink.text(b"  USBHID xhci: recoveries=");
        sink.dec(xs.interrupt_recoveries);
        sink.text(b" pending_timeouts=");
        sink.dec(xs.interrupt_pending_timeouts);
        sink.text(b" halted_checks=");
        sink.dec(xs.interrupt_halted_checks);
        sink.text(b"\r\n");
    }
}
unsafe fn init(peers: &Peers) -> bool {
    let value = status();
    if value.attempted {
        let sink = Sink(true);
        sink.text(b"[USBHIDBOOT] init skipped phase=");
        sink.text(phase_name(value.phase));
        sink.text(b" fatal=");
        sink.text(yes_no(value.fatal));
        sink.text(b"\r\n");
        return !value.fatal;
    }
    publish(Status {
        attempted: true,
        reason: Text::literal(b"starting"),
        ..Status::EMPTY
    });
    if !unsafe { (peers.storage_ready)() } {
        let mut value = status();
        value.phase = 1;
        value.failed = true;
        value.fatal = true;
        value.reason = Text::literal(b"storage controllers missing");
        publish(value);
        dump_boot_status(peers);
        let message = b"USB-HID boot before storage controllers";
        unsafe { fatal::r4os_rust_fatal_record(16, message.as_ptr(), message.len()) };
        return false;
    }
    let slot = unsafe { registry::r4os_rust_driver_registry_begin(b"USBHID".as_ptr(), 6, 3, 1, 0) };
    let mut value = status();
    value.registry_slot_present = slot != usize::MAX;
    value.registry_state = u8::from(slot != usize::MAX);
    publish(value);
    if slot == usize::MAX {
        Sink(true).text(
            b"[USBHIDBOOT][WARN] registry slot missing; continuing without registry owner\r\n",
        );
    }
    let bound = unsafe { (peers.hid_init)() };
    let mut value = status();
    value.init_returned_bound = bound;
    publish(value);
    let mut input = MaybeUninit::uninit();
    unsafe { (peers.hid_status)(input.as_mut_ptr()) };
    classify(unsafe { input.assume_init() });
    apply_registry(slot);
    dump_boot_status(peers);
    emit_boot_line(peers);
    true
}
/// # Safety
/// Exact BSP bootstrap peers and resident original owner projections. No
/// peer may recursively start a new boot-HID initialization generation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_hid_boot_init(peers: *const Peers) -> bool {
    unsafe { init(&*peers) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_usb_hid_boot_initialized() -> bool {
    let v = status();
    v.attempted && v.initialized && !v.fatal
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_usb_hid_boot_active() -> bool {
    let v = status();
    v.active && !v.fatal
}
/// # Safety
/// Caller lends one exclusive aligned output; bootstrap has published status.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_hid_boot_status(out: *mut Status) {
    unsafe { out.write(status()) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_usb_hid_boot_phase(phase: u8) -> Text {
    Text::literal(phase_name(phase))
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_usb_hid_boot_registry(state: u8) -> Text {
    Text::literal(registry_state_name(state))
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_usb_hid_boot_dump() {
    dump_status()
}
/// # Safety
/// Exact retained task-start peers after task-runtime initialization.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_hid_boot_start(peers: *const Peers) -> bool {
    let peers = unsafe { &*peers };
    if !unsafe { (peers.port_start)() } {
        return false;
    }
    if !status().active {
        return true;
    }
    unsafe { (peers.hid_start)() }
}
const _: () = {
    assert!(core::mem::size_of::<Peers>() == 48);
    assert!(core::mem::size_of::<XhciStatus>() == 24);
};
fn dump_status() {
    let value = status();
    let sink = Sink(false);
    sink.text(b"USB-HID boot\r\n");
    sink.text(b"  phase=");
    sink.text(phase_name(value.phase));
    sink.text(b" initialized=");
    sink.text(yes_no(value.initialized));
    sink.text(b" active=");
    sink.text(yes_no(value.active));
    sink.text(b" failed=");
    sink.text(yes_no(value.failed));
    sink.text(b" fatal=");
    sink.text(yes_no(value.fatal));
    sink.text(b" registry=");
    sink.text(registry_state_name(value.registry_state));
    sink.text(b" continue=");
    sink.text(yes_no(!value.fatal));
    sink.text(b"\r\n");
    sink.text(b"  keyboard=");
    sink.text(binding_name(value.keyboard_present, value.keyboard_bound));
    sink.text(b" mouse=");
    sink.text(binding_name(value.mouse_present, value.mouse_bound));
    sink.text(b" unsupported=");
    sink.dec((value.unsupported_hid) as u64);
    sink.text(b" missing-ep=");
    sink.dec((value.missing_endpoint_hid) as u64);
    sink.text(b"\r\n");
    sink.text(b"  setup-warn=");
    sink.dec((value.setup_warnings) as u64);
    sink.text(b" failures=");
    sink.dec(value.failures);
    sink.text(b" r4p-required-missing=");
    sink.dec((value.protocol_required_missing) as u64);
    sink.text(b" boot-source=");
    sink.text(text(value.boot_source));
    sink.text(b" reason=");
    sink.text(text(value.reason));
    sink.text(b"\r\n");
}

fn dump_boot_status(peers: &Peers) {
    let value = status();
    let sink = Sink(true);
    sink.text(b"[USBHIDBOOT] status phase=");
    sink.text(phase_name(value.phase));
    sink.text(b" outcome=");
    sink.text(outcome_name(value));
    sink.text(b" initialized=");
    sink.text(yes_no(value.initialized));
    sink.text(b" init_bound=");
    sink.text(yes_no(value.init_returned_bound));
    sink.text(b" active=");
    sink.text(yes_no(value.active));
    sink.text(b" failed=");
    sink.text(yes_no(value.failed));
    sink.text(b" fatal=");
    sink.text(yes_no(value.fatal));
    sink.text(b" registry=");
    sink.text(registry_state_name(value.registry_state));
    sink.text(b" registry_slot=");
    sink.text(yes_no(value.registry_slot_present));
    sink.text(b" boot_continue=");
    sink.text(yes_no(!value.fatal));
    sink.text(b" keyboard=");
    sink.text(binding_name(value.keyboard_present, value.keyboard_bound));
    sink.text(b" mouse=");
    sink.text(binding_name(value.mouse_present, value.mouse_bound));
    sink.text(b" unsupported=");
    sink.dec((value.unsupported_hid) as u64);
    sink.text(b" missing_ep=");
    sink.dec((value.missing_endpoint_hid) as u64);
    sink.text(b" setup_warn=");
    sink.dec((value.setup_warnings) as u64);
    sink.text(b" failures=");
    sink.dec(value.failures);
    sink.text(b" dispatch_failures=");
    sink.dec(value.boot_dispatch_failures);
    sink.text(b" r4p_required_missing=");
    sink.dec((value.protocol_required_missing) as u64);
    sink.text(b" source=");
    sink.text(text(value.boot_source));
    sink.text(b" reason=");
    sink.text(text(value.reason));
    let xs = xhci_status(peers);
    sink.text(b" xhci_recoveries=");
    sink.dec(xs.interrupt_recoveries);
    sink.text(b" xhci_pending_timeouts=");
    sink.dec(xs.interrupt_pending_timeouts);
    sink.text(b" xhci_halted_checks=");
    sink.dec(xs.interrupt_halted_checks);
    sink.text(b"\r\n");
}
