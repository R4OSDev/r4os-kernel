//! Native HID binding, topology reconciliation and polling owner. Every
//! binding/status mutation is under its reentrant class guard;
//! raw resident identities, never Rust mutable borrows, span module calls.
use super::{
    core as usb_core, hid_report,
    hid_types::{HidBinding, HidInterfaceCandidate as Candidate, Options, PollTaskSummary, Status},
    host_controller, xhci_api as xhci, xhci_diag as diag,
    xhci_types::EndpointHandle,
};
use crate::{
    boot::foundation::puts,
    contract as c,
    fs::vfs_types::Bytes,
    loader::r4p,
    platform::timer,
    sched::{
        scheduler,
        sync_types::Text,
        task::{task_api, task_registry},
    },
};
use core::{
    cell::UnsafeCell,
    mem, ptr,
    sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
};
struct ClassOwner(core::cell::UnsafeCell<crate::sched::sync_types::UnwindGuard>);
unsafe impl Sync for ClassOwner {}
static CLASS_OWNER: ClassOwner = ClassOwner(core::cell::UnsafeCell::new(
    crate::sched::sync_types::UnwindGuard {
        owner: 0,
        owner_generation: 0,
        boot_owner: false,
        depth: 0,
        name: Text::literal(b"usb-hid-class"),
        queue: crate::sched::sync_types::WaitQueue::EMPTY,
    },
));
struct Guard;
impl Guard {
    fn enter() -> Option<Self> {
        if unsafe {
            crate::sched::sync_locks::guard_enter(
                CLASS_OWNER.0.get(),
                super::xhci_constants::CONTROLLER_OWNERSHIP_TIMEOUT_TICKS,
            )
        } {
            Some(Self)
        } else {
            None
        }
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        unsafe {
            crate::sched::sync_locks::guard_leave(CLASS_OWNER.0.get());
        }
    }
}
const REPORT_BUFFER_LEN: usize = 32;
const MOUSE_POLL_BUDGET: usize = 8;
const REPORT_DESCRIPTOR_MAX: usize = 512;
const KEYBOARD_REPORT_QUEUE_RESERVE: u32 = (c::usb_hid_boot_max_keys as u32 * 2) + 8;
const POLL_IDLE_TICKS: u64 = 10 * crate::arch::pit::DEFAULT_HZ as u64 / 1000;
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Keyboard,
    Mouse,
    Consumer,
}
#[repr(C)]
#[derive(Default)]
pub struct QueueStats {
    pub capacity: u32,
    pub pending: u32,
    pub drops: u64,
}

struct State {
    current: Status,
    bindings: [HidBinding; 3],
    options: Options,
    service_polls: u64,
    keyboard_backpressure_active: bool,
    keyboard_backpressure_polls: u64,
    topology_reconciles: u64,
    boot_r4p_classify: u64,
    boot_r4p_keyboard: u64,
    boot_r4p_mouse: u64,
    boot_dispatch_failures: u64,
    boot_last_result: i32,
}
impl State {
    const EMPTY: Self = Self {
        current: Status::EMPTY,
        bindings: [HidBinding::EMPTY; 3],
        options: Options::EMPTY,
        service_polls: 0,
        keyboard_backpressure_active: false,
        keyboard_backpressure_polls: 0,
        topology_reconciles: 0,
        boot_r4p_classify: 0,
        boot_r4p_keyboard: 0,
        boot_r4p_mouse: 0,
        boot_dispatch_failures: 0,
        boot_last_result: 0,
    };
}
struct Storage(UnsafeCell<State>);
// SAFETY: binding/state entries retain class admission through their
// exact callbacks; task lifecycle summary is separate atomic producer data.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State::EMPTY));
static POLL_STARTED: AtomicBool = AtomicBool::new(false);
static POLL_TASK_ID: AtomicU32 = AtomicU32::new(0);
static POLL_ITERATIONS: AtomicU64 = AtomicU64::new(0);
fn state() -> *mut State {
    STATE.0.get()
}
fn current() -> *mut Status {
    unsafe { ptr::addr_of_mut!((*state()).current) }
}
fn binding(kind: Kind) -> *mut HidBinding {
    unsafe { ptr::addr_of_mut!((*state()).bindings[kind as usize]) }
}
fn role(name: &'static [u8]) -> Bytes {
    Bytes {
        data: name.as_ptr(),
        len: name.len(),
    }
}
fn active(name: &'static [u8]) -> bool {
    unsafe { r4p::r4os_rust_r4p_has_active(role(name)) }
}
fn queue_stats() -> QueueStats {
    let mut out = QueueStats::default();
    unsafe {
        usb_hid_queue(&mut out);
    }
    out
}
fn source() -> Text {
    Text::literal(r4p::source(b"usb.hid_boot"))
}
fn roles_ready() -> bool {
    active(b"usb.hid_report") && active(b"usb.hid_boot")
}
fn missing_reason() -> Text {
    Text::literal(
        match (!active(b"usb.hid_report"), !active(b"usb.hid_boot")) {
            (true, true) => b"HIDREPORT.R4P and USBHID.R4P required",
            (true, false) => b"HIDREPORT.R4P required",
            (false, true) => b"USBHID.R4P required",
            _ => b"USB HID R4P required",
        },
    )
}
unsafe fn dispatch_boot(opcode: u32, op: *mut c::UsbHidBootOp) -> bool {
    let input = c::ProtocolBuffer {
        data: op.cast(),
        len: mem::size_of::<c::UsbHidBootOp>() as u32,
        capacity: mem::size_of::<c::UsbHidBootOp>() as u32,
        flags: 0,
        reserved: 0,
    };
    let mut out = c::ProtocolBuffer::default();
    let result =
        unsafe { r4p::r4os_rust_r4p_dispatch(role(b"usb.hid_boot"), opcode, &input, &mut out) };
    unsafe {
        (*state()).boot_last_result = result;
        if result != c::usb_hid_boot_result_ok && result != c::usb_hid_boot_result_ignored {
            (*state()).boot_dispatch_failures = (*state()).boot_dispatch_failures.wrapping_add(1);
            return false;
        }
    }
    true
}
pub fn init(selected: Options) -> bool {
    let Some(_g) = Guard::enter() else {
        return false;
    };
    unsafe {
        // The small individual records avoid placing the full binding state on
        // a task stack, while retaining exactly the original reset fields.
        (*state()).options = selected;
        current().write(Status {
            initialized: true,
            reason: Text::literal(b"no USB HID boot device found"),
            ..Status::EMPTY
        });
        for kind in [Kind::Keyboard, Kind::Mouse, Kind::Consumer] {
            binding(kind).write(HidBinding::EMPTY);
        }
        usb_hid_platform_usb(false);
        (*state()).service_polls = 0;
        (*state()).keyboard_backpressure_active = false;
        (*state()).keyboard_backpressure_polls = 0;
        (*state()).topology_reconciles = 0;
        (*state()).boot_r4p_classify = 0;
        (*state()).boot_r4p_keyboard = 0;
        (*state()).boot_r4p_mouse = 0;
        (*state()).boot_dispatch_failures = 0;
        (*state()).boot_last_result = 0;
        xhci::set_topology_hook(Some(topology_changed));
        scan_catalogue();
        refresh_aggregate_status();
        if (*current()).keyboard_bound && (*current()).mouse_bound {
            (*current()).reason = Text::literal(if (*current()).setup_warnings == 0 {
                b"USB HID boot keyboard bound; USB HID boot mouse bound"
            } else {
                b"USB HID boot keyboard bound; USB HID boot mouse bound; optional class setup warning"
            });
        }
        (*current()).keyboard_bound || (*current()).mouse_bound || (*binding(Kind::Consumer)).bound
    }
}
unsafe fn scan_catalogue() {
    let mut index = 0;
    while let Some(dev) = unsafe { usb_core::device_at(index) } {
        unsafe {
            scan_device(dev);
        }
        index += 1;
    }
}
unsafe extern "C" fn topology_changed() {
    if let Some(_g) = Guard::enter() {
        unsafe {
            (*state()).topology_reconciles = (*state()).topology_reconciles.wrapping_add(1);
            reconcile_bindings();
            if [Kind::Keyboard, Kind::Mouse, Kind::Consumer]
                .into_iter()
                .any(|k| (*binding(k)).bound)
            {
                start_poll_task_locked();
            }
        }
    }
    // Do not hold HID admission over storage discovery or media I/O.
    super::msc::topology_changed();
}
unsafe fn binding_still_published(b: *const HidBinding) -> bool {
    unsafe {
        if !(*b).present || (*b).port == 0 || (*b).device.slot_id == 0 {
            return false;
        }
        usb_core::contains_device((*b).device)
    }
}
unsafe fn reconcile_bindings() {
    unsafe {
        for kind in [Kind::Keyboard, Kind::Mouse, Kind::Consumer] {
            let b = binding(kind);
            if (*b).present && !binding_still_published(b) {
                b.write(HidBinding::EMPTY);
            }
        }
        if !(*binding(Kind::Keyboard)).bound {
            (*state()).keyboard_backpressure_active = false;
            usb_hid_set_keyboard_hook(None);
        }
        if !(*binding(Kind::Mouse)).bound {
            usb_hid_set_mouse_hook(None);
        }
        (*current()).unsupported_hid = 0;
        (*current()).missing_endpoint_hid = 0;
        (*current()).protocol_required_missing = 0;
        scan_catalogue();
        refresh_aggregate_status();
        (*current()).reason = Text::literal(
            match ((*current()).keyboard_bound, (*current()).mouse_bound) {
                (true, true) => b"USB HID boot keyboard bound; USB HID boot mouse bound",
                (true, false) => b"USB HID boot keyboard bound",
                (false, true) => b"USB HID boot mouse bound",
                _ => b"no USB HID boot device found",
            },
        );
    }
}
fn choose_interrupt_endpoint(interrupt: u8, first: u8, attributes: u8) -> u8 {
    if interrupt != 0 {
        interrupt
    } else if first & 0x80 != 0 && attributes & 3 == 3 {
        first
    } else {
        0
    }
}
unsafe fn scan_device(dev: super::types::Device) {
    unsafe {
        let mut records_seen = false;
        for i in 0..usize::from(dev.interface_record_count).min(8) {
            let f = dev.interfaces[i];
            if !f.active {
                continue;
            }
            records_seen = true;
            handle_candidate(
                dev,
                Candidate {
                    number: f.number,
                    class_code: f.class_code,
                    subclass: f.subclass,
                    protocol: f.protocol,
                    endpoint_address: choose_interrupt_endpoint(
                        f.interrupt_in_endpoint_address,
                        f.first_endpoint_address,
                        f.first_endpoint_attributes,
                    ),
                    endpoint_max_packet: if f.interrupt_in_endpoint_address != 0 {
                        f.interrupt_in_endpoint_max_packet
                    } else {
                        f.first_endpoint_max_packet
                    },
                    endpoint_interval: if f.interrupt_in_endpoint_address != 0 {
                        f.interrupt_in_endpoint_interval
                    } else {
                        f.first_endpoint_interval
                    },
                    report_descriptor_len: f.hid_report_descriptor_len,
                },
            );
        }
        if !records_seen {
            handle_candidate(
                dev,
                Candidate {
                    number: dev.first_interface_number,
                    class_code: dev.first_interface_class,
                    subclass: dev.first_interface_subclass,
                    protocol: dev.first_interface_protocol,
                    endpoint_address: choose_interrupt_endpoint(
                        0,
                        dev.first_endpoint_address,
                        dev.first_endpoint_attributes,
                    ),
                    endpoint_max_packet: dev.first_endpoint_max_packet,
                    endpoint_interval: dev.first_endpoint_interval,
                    report_descriptor_len: dev.hid_report_descriptor_len,
                },
            );
        }
    }
}
fn has_interrupt(c: Candidate) -> bool {
    c.endpoint_address & 0x80 != 0 && c.endpoint_max_packet != 0
}
unsafe fn classify(cand: Candidate) -> Option<Kind> {
    unsafe {
        if !roles_ready() {
            if cand.class_code == 3
                && cand.subclass == 1
                && matches!(cand.protocol, 1 | 2)
                && has_interrupt(cand)
            {
                (*current()).protocol_required_missing =
                    (*current()).protocol_required_missing.saturating_add(1);
                (*current()).reason = missing_reason();
            }
            return None;
        }
        if !active(b"usb.hid_boot") {
            return None;
        }
        let mut op = c::UsbHidBootOp {
            class_code: cand.class_code,
            subclass: cand.subclass,
            protocol: cand.protocol,
            endpoint_address: cand.endpoint_address,
            endpoint_max_packet: cand.endpoint_max_packet,
            ..c::UsbHidBootOp::default()
        };
        if !dispatch_boot(c::usb_hid_boot_op_classify_interface, &mut op)
            || op.result != c::usb_hid_boot_result_ok
        {
            return None;
        }
        (*state()).boot_r4p_classify = (*state()).boot_r4p_classify.wrapping_add(1);
        match op.kind {
            c::usb_hid_boot_kind_keyboard => Some(Kind::Keyboard),
            c::usb_hid_boot_kind_mouse => Some(Kind::Mouse),
            _ => None,
        }
    }
}
unsafe fn handle_candidate(dev: super::types::Device, cand: Candidate) {
    unsafe {
        if cand.class_code != 3
            || (!(*state()).options.bind_mouse && cand.subclass == 1 && cand.protocol == 2)
        {
            return;
        }
        if cand.subclass == 0 && cand.protocol == 0 && has_interrupt(cand) {
            let b = binding(Kind::Consumer);
            if (*b).bound || !roles_ready() {
                return;
            }
            fill_binding(b, dev, cand);
            (*b).report_protocol = true;
            read_report_descriptor(b);
            let Some(decoded) = hid_report::consumer(ptr::addr_of!((*b).report_wire), &[]) else {
                return;
            };
            if !(*b).report_descriptor_ok || decoded.capabilities == 0 {
                return;
            }
            (*b).consumer_caps = decoded.capabilities;
            if bind_device(b, Kind::Consumer) {
                usb_hid_platform_usb(true);
            }
            return;
        }
        let Some(kind) = classify(cand) else {
            if cand.subclass == 1 && !has_interrupt(cand) {
                (*current()).missing_endpoint_hid += 1;
                (*current()).reason =
                    Text::literal(b"USB HID boot device missing interrupt-in endpoint");
            } else if cand.subclass != 1 || !matches!(cand.protocol, 1 | 2) {
                (*current()).unsupported_hid += 1;
            }
            return;
        };
        let b = binding(kind);
        if (*b).bound || (kind == Kind::Mouse && !(*state()).options.bind_mouse) {
            return;
        }
        if kind == Kind::Keyboard {
            (*current()).keyboard_present = true;
        } else {
            (*current()).mouse_present = true;
        }
        fill_binding(b, dev, cand);
        let bound = bind_device(b, kind);
        match kind {
            Kind::Keyboard => {
                if bound {
                    (*current()).keyboard_bound = true;
                    usb_hid_set_keyboard_hook(Some(keyboard_poll_hook));
                    (*current()).reason = Text::literal(if (*b).setup_warnings == 0 {
                        b"USB HID boot keyboard bound"
                    } else {
                        b"USB HID boot keyboard bound; optional class setup warning"
                    });
                } else {
                    (*current()).reason = Text::literal(b"USB HID keyboard bind failed");
                }
            }
            Kind::Mouse => {
                if bound {
                    (*current()).mouse_bound = true;
                    usb_hid_set_mouse_hook(Some(mouse_poll_hook));
                    (*current()).reason = Text::literal(if (*b).setup_warnings == 0 {
                        b"USB HID boot mouse bound"
                    } else {
                        b"USB HID boot mouse bound; optional class setup warning"
                    });
                } else {
                    (*current()).reason = Text::literal(b"USB HID mouse bind failed");
                }
            }
            Kind::Consumer => {
                (*current()).unsupported_hid += 1;
            }
        }
    }
}
unsafe fn fill_binding(b: *mut HidBinding, dev: super::types::Device, cand: Candidate) {
    unsafe {
        let device = xhci::device_handle_from_core(dev);
        b.write(HidBinding {
            present: true,
            port: dev.port,
            device,
            endpoint: EndpointHandle {
                device,
                kind: 1,
                address: cand.endpoint_address,
                max_packet: cand.endpoint_max_packet,
                interval: cand.endpoint_interval,
                ..EndpointHandle::EMPTY
            },
            interface_number: cand.number,
            endpoint_address: cand.endpoint_address,
            endpoint_interval: cand.endpoint_interval,
            endpoint_max_packet: cand.endpoint_max_packet,
            report_descriptor_len: cand.report_descriptor_len,
            ..HidBinding::EMPTY
        });
    }
}
fn select_failure(kind: Kind) -> Text {
    Text::literal(match kind {
        Kind::Keyboard => b"USB HID keyboard select failed",
        Kind::Mouse => b"USB HID mouse select failed",
        Kind::Consumer => b"USB HID Consumer select failed",
    })
}
unsafe fn bind_device(b: *mut HidBinding, kind: Kind) -> bool {
    unsafe {
        if !xhci::select_device_handle(ptr::addr_of_mut!((*b).device)) {
            (*b).failures += 1;
            (*current()).reason = select_failure(kind);
            return false;
        }
        (*b).bound = bind_interrupt_input(b);
        (*b).bound
    }
}
unsafe fn bind_interrupt_input(b: *mut HidBinding) -> bool {
    unsafe {
        if !xhci::set_configuration_for_handle(ptr::addr_of_mut!((*b).device)) {
            (*b).setup_warnings += 1;
        }
        if (*b).report_protocol {
            (*b).protocol_ok = true;
        } else if !xhci::set_hid_boot_protocol_for_handle(
            ptr::addr_of_mut!((*b).device),
            (*b).interface_number,
        ) {
            (*b).setup_warnings += 1;
        } else {
            (*b).protocol_ok = true;
        }
        if !xhci::set_hid_idle_for_handle(ptr::addr_of_mut!((*b).device), (*b).interface_number) {
            (*b).setup_warnings += 1;
        } else {
            (*b).idle_ok = true;
        }
        if !(*b).report_protocol {
            read_report_descriptor(b);
        }
        if !xhci::configure_interrupt_in_endpoint_handle(ptr::addr_of_mut!((*b).endpoint)) {
            (*b).failures += 1;
            (*current()).reason = Text::literal(b"interrupt endpoint configure failed");
            return false;
        }
        true
    }
}
unsafe fn read_report_descriptor(b: *mut HidBinding) {
    unsafe {
        if (*b).report_descriptor_len == 0 {
            return;
        }
        let mut descriptor = [0; REPORT_DESCRIPTOR_MAX];
        let Some(actual) = xhci::get_hid_report_descriptor_for_handle(
            ptr::addr_of_mut!((*b).device),
            (*b).interface_number,
            (*b).report_descriptor_len,
            &mut descriptor,
        ) else {
            (*b).setup_warnings += 1;
            return;
        };
        (*b).report_descriptor_read_len = actual as u16;
        (*b).report_summary =
            hid_report::parse(&descriptor[..actual], ptr::addr_of_mut!((*b).report_wire));
        (*b).report_descriptor_ok = (*b).report_summary.parsed && !(*b).report_summary.malformed;
        (*b).report_descriptor_malformed = (*b).report_summary.malformed;
        if (*b).report_summary.has_report_id {
            (*b).report_id_heuristic = true;
        }
    }
}
unsafe extern "C" fn keyboard_poll_hook() {
    let Some(_g) = Guard::enter() else {
        return;
    };
    unsafe {
        poll_binding(binding(Kind::Keyboard), Kind::Keyboard);
    }
}
unsafe extern "C" fn mouse_poll_hook() {
    let Some(_g) = Guard::enter() else {
        return;
    };
    unsafe {
        poll_mouse();
    }
}
unsafe fn poll_mouse() {
    for _ in 0..MOUSE_POLL_BUDGET {
        if !unsafe { poll_binding(binding(Kind::Mouse), Kind::Mouse) } {
            break;
        }
    }
}
unsafe fn ensure_selected(b: *mut HidBinding, kind: Kind) -> bool {
    unsafe {
        let xs = xhci::device_status((*b).device);
        if (*b).device.slot_id != 0
            && xs.addressed_slot_id == (*b).device.slot_id
            && xs.addressed_port == (*b).device.port
            && xs.interrupt_endpoint_configured
            && !xs.interrupt_endpoint_faulted
            && xs.interrupt_endpoint_address == (*b).endpoint_address
        {
            return true;
        }
        if !xhci::select_device_handle(ptr::addr_of_mut!((*b).device)) {
            (*b).failures += 1;
            (*current()).reason = select_failure(kind);
            return false;
        }
        let selected = xhci::device_status((*b).device);
        if selected.addressed_slot_id == (*b).device.slot_id
            && selected.addressed_port == (*b).device.port
            && selected.interrupt_endpoint_configured
            && !selected.interrupt_endpoint_faulted
            && selected.interrupt_endpoint_address == (*b).endpoint_address
        {
            return true;
        }
        bind_interrupt_input(b)
    }
}
unsafe fn poll_binding(b: *mut HidBinding, kind: Kind) -> bool {
    unsafe {
        if !(*b).bound {
            return false;
        }
        // Retain this exact controller across public-v2 handle projection and
        // dispatch; local slot reuse cannot enter the gap between those calls.
        let Some(_controller) = super::xhci_api::Guard::for_device((*b).device) else {
            return false;
        };
        if kind == Kind::Keyboard {
            if !usb_hid_can_accept(KEYBOARD_REPORT_QUEUE_RESERVE) {
                (*state()).keyboard_backpressure_active = true;
                (*state()).keyboard_backpressure_polls =
                    (*state()).keyboard_backpressure_polls.wrapping_add(1);
                return false;
            }
            (*state()).keyboard_backpressure_active = false;
        }
        if !ensure_selected(b, kind) {
            return false;
        }
        let mut report = [0; REPORT_BUFFER_LEN];
        let requested = usize::from((*b).endpoint_max_packet).clamp(8, REPORT_BUFFER_LEN);
        (*b).polls += 1;
        let endpoint = xhci::usb_host_endpoint_handle((*b).endpoint);
        let mut actual = 0;
        match host_controller::r4os_rust_usb_host_interrupt(
            &endpoint,
            report.as_mut_ptr(),
            requested,
            &mut actual,
        ) {
            0 => {
                (*b).no_reports += 1;
                return false;
            }
            1 => {}
            _ => {
                (*b).failures += 1;
                return false;
            }
        }
        let n = if actual == 0 || actual as usize > requested {
            let len = xhci::last_interrupt_actual_length((*b).device);
            if len == 0 || len > requested {
                requested
            } else {
                len
            }
        } else {
            actual as usize
        };
        (*b).last_report_request_len =
            xhci::last_interrupt_requested_length((*b).device).min(255) as u8;
        (*b).last_report_residue = xhci::last_interrupt_residue((*b).device);
        (*b).reports += 1;
        let bytes = &report[..n];
        if bytes.iter().any(|v| *v != 0) {
            (*b).last_nonzero_report = report;
            (*b).last_nonzero_report_len = n as u8;
        }
        let previous = ptr::addr_of!((*b).last_report);
        let duplicate = n == usize::from((*b).last_report_len)
            && slice_equal(bytes, core::slice::from_raw_parts(previous.cast(), n));
        if duplicate && (kind != Kind::Mouse || !bytes.iter().any(|v| *v != 0)) {
            (*b).duplicate_reports += 1;
            return true;
        }
        match kind {
            Kind::Keyboard => {
                if !decode_keyboard(b, bytes) {
                    (*b).failures += 1;
                    (*current()).reason = Text::literal(b"USB HID keyboard R4P decode failed");
                }
            }
            Kind::Mouse => {
                if !decode_mouse(bytes) {
                    (*current()).reason = Text::literal(b"USB HID mouse R4P decode failed");
                }
            }
            Kind::Consumer => decode_consumer(b, bytes),
        }
        (*b).last_report = report;
        (*b).last_report_len = n as u8;
        true
    }
}
fn slice_equal(a: &[u8], b: &[u8]) -> bool {
    a == b
}
unsafe fn decode_consumer(b: *mut HidBinding, report: &[u8]) {
    unsafe {
        let Some(decoded) = hid_report::consumer(ptr::addr_of!((*b).report_wire), report) else {
            (*b).failures += 1;
            return;
        };
        if decoded.report_id as usize >= 16 {
            return;
        }
        let pressed = decoded.pressed as u8;
        let index = decoded.report_id as usize;
        let rising = pressed & !(*b).consumer_pressed[index];
        (*b).consumer_pressed[index] = pressed;
        if rising & 1 != 0 {
            usb_hid_consumer(0x6f);
        }
        if rising & 2 != 0 {
            usb_hid_consumer(0x70);
        }
    }
}
unsafe fn decode_keyboard(b: *mut HidBinding, report: &[u8]) -> bool {
    unsafe {
        if !active(b"usb.hid_boot")
            || report.len() > c::usb_hid_boot_max_report
            || usize::from((*b).last_report_len) > c::usb_hid_boot_max_report
        {
            return false;
        }
        let mut op = c::UsbHidBootOp {
            protocol_ok: u8::from((*b).protocol_ok),
            report_len: report.len() as u8,
            previous_len: (*b).last_report_len,
            ..c::UsbHidBootOp::default()
        };
        op.report[..report.len()].copy_from_slice(report);
        let prev = usize::from((*b).last_report_len);
        ptr::copy_nonoverlapping(
            ptr::addr_of!((*b).last_report).cast::<u8>(),
            op.previous.as_mut_ptr(),
            prev,
        );
        if !dispatch_boot(c::usb_hid_boot_op_decode_keyboard, &mut op)
            || op.result != c::usb_hid_boot_result_ok
        {
            return false;
        }
        (*state()).boot_r4p_keyboard = (*state()).boot_r4p_keyboard.wrapping_add(1);
        if op.flags & c::usb_hid_boot_flag_report_id_heuristic != 0 {
            (*b).report_id_heuristic = true;
            (*current()).report_id_heuristic = true;
        }
        for bit in [1u8, 2, 4, 8, 16, 32, 64, 128] {
            let old = op.old_modifiers & bit != 0;
            let new = op.new_modifiers & bit != 0;
            if old != new {
                usb_hid_inject_modifier(bit, new);
            }
        }
        (*current()).last_modifiers = op.new_modifiers;
        let mut released = [0; c::usb_hid_boot_max_keys];
        let count = usb_hid_released(&op, released.as_mut_ptr(), released.len());
        for usage in &released[..count] {
            record_injected_usage(*usage, false);
        }
        for usage in &op.keys[..usize::from(op.key_count).min(op.keys.len())] {
            if *usage == 0 {
                continue;
            }
            (*current()).last_usage = *usage;
            record_injected_usage(*usage, true);
        }
        true
    }
}
unsafe fn record_injected_usage(usage: u8, down: bool) {
    unsafe {
        if usb_hid_inject_usage(usage, down) {
            (*current()).decoded_keys += 1;
        } else {
            (*current()).drops += 1;
        }
    }
}
unsafe fn decode_mouse(report: &[u8]) -> bool {
    unsafe {
        if !active(b"usb.hid_boot") || report.len() > c::usb_hid_boot_max_report {
            return false;
        }
        let mut op = c::UsbHidBootOp {
            report_len: report.len() as u8,
            ..c::UsbHidBootOp::default()
        };
        op.report[..report.len()].copy_from_slice(report);
        if !dispatch_boot(c::usb_hid_boot_op_decode_mouse, &mut op)
            || op.result != c::usb_hid_boot_result_ok
        {
            return false;
        }
        (*state()).boot_r4p_mouse = (*state()).boot_r4p_mouse.wrapping_add(1);
        (*current()).last_mouse_buttons = op.mouse_buttons;
        (*current()).last_mouse_dx = op.mouse_dx;
        (*current()).last_mouse_dy = op.mouse_dy;
        (*current()).last_mouse_wheel = op.mouse_wheel;
        usb_hid_mouse(op.mouse_dx, op.mouse_dy, op.mouse_buttons, op.mouse_wheel);
        (*current()).decoded_mouse += 1;
        true
    }
}
unsafe fn refresh_aggregate_status() {
    unsafe {
        usb_hid_platform_usb(
            (*binding(Kind::Consumer)).bound && (*binding(Kind::Consumer)).consumer_caps != 0,
        );
        let q = queue_stats();
        let key = ptr::addr_of!((*state()).bindings[0]);
        let mouse = ptr::addr_of!((*state()).bindings[1]);
        (*current()).keyboard_present = (*key).present;
        (*current()).keyboard_bound = (*key).bound;
        (*current()).mouse_present = (*mouse).present;
        (*current()).mouse_bound = (*mouse).bound;
        (*current()).polls = (*key).polls + (*mouse).polls;
        (*current()).no_reports = (*key).no_reports + (*mouse).no_reports;
        (*current()).reports = (*key).reports + (*mouse).reports;
        (*current()).duplicate_reports = (*key).duplicate_reports + (*mouse).duplicate_reports;
        (*current()).service_polls = (*state()).service_polls;
        (*current()).keyboard_backpressure_active = (*state()).keyboard_backpressure_active;
        (*current()).keyboard_backpressure_polls = (*state()).keyboard_backpressure_polls;
        (*current()).keyboard_queue_capacity = q.capacity;
        (*current()).keyboard_queue_pending = q.pending;
        (*current()).keyboard_queue_free = q.capacity - q.pending;
        (*current()).keyboard_queue_drops = q.drops;
        (*current()).topology_reconciles = (*state()).topology_reconciles;
        (*current()).failures = (*key).failures + (*mouse).failures;
        (*current()).setup_warnings = (*key)
            .setup_warnings
            .saturating_add((*mouse).setup_warnings);
        (*current()).report_id_heuristic = (*current()).report_id_heuristic
            || (*key).report_id_heuristic
            || (*mouse).report_id_heuristic;
        (*current()).report_descriptor_ok = report_descriptor_aggregate_ok();
        (*current()).report_descriptor_malformed =
            (*key).report_descriptor_malformed || (*mouse).report_descriptor_malformed;
        (*current()).last_report_len = (*key).last_report_len.max((*mouse).last_report_len);
        (*current()).last_nonzero_report_len = (*key)
            .last_nonzero_report_len
            .max((*mouse).last_nonzero_report_len);
        (*current()).last_report_request_len = (*key)
            .last_report_request_len
            .max((*mouse).last_report_request_len);
        (*current()).last_report_residue =
            (*key).last_report_residue.max((*mouse).last_report_residue);
        (*current()).boot_source = source();
        (*current()).boot_r4p_classify = (*state()).boot_r4p_classify;
        (*current()).boot_r4p_keyboard = (*state()).boot_r4p_keyboard;
        (*current()).boot_r4p_mouse = (*state()).boot_r4p_mouse;
        (*current()).boot_dispatch_failures = (*state()).boot_dispatch_failures;
        (*current()).boot_last_result = (*state()).boot_last_result;
        (*current()).protocol_ok = aggregate_ok(true);
        (*current()).idle_ok = aggregate_ok(false);
        let primary = if (*key).bound || (*key).present {
            key
        } else if (*mouse).present {
            mouse
        } else {
            binding(Kind::Consumer)
        };
        (*current()).port = (*primary).port;
        (*current()).interface_number = (*primary).interface_number;
        (*current()).endpoint_address = (*primary).endpoint_address;
        (*current()).endpoint_interval = (*primary).endpoint_interval;
        (*current()).endpoint_max_packet = (*primary).endpoint_max_packet;
    }
}
unsafe fn report_descriptor_aggregate_ok() -> bool {
    unsafe {
        let mut have = false;
        for kind in [Kind::Keyboard, Kind::Mouse] {
            let b = binding(kind);
            if (*b).bound && (*b).report_descriptor_read_len != 0 {
                have = true;
                if !(*b).report_descriptor_ok {
                    return false;
                }
            }
        }
        have
    }
}
unsafe fn aggregate_ok(protocol: bool) -> bool {
    unsafe {
        let mut have = false;
        for kind in [Kind::Keyboard, Kind::Mouse] {
            let b = binding(kind);
            if (*b).bound {
                have = true;
                if (protocol && !(*b).protocol_ok) || (!protocol && !(*b).idle_ok) {
                    return false;
                }
            }
        }
        have
    }
}
pub(crate) fn binding_for(device: super::xhci_types::DeviceHandle) -> (bool, bool) {
    let Some(_g) = Guard::enter() else {
        return (false, false);
    };
    let matches = |kind| unsafe {
        let b = &*binding(kind);
        b.present
            && b.bound
            && b.device.controller_id == device.controller_id
            && b.device.generation == device.generation
            && b.device.port == device.port
            && b.device.slot_id == device.slot_id
    };
    (matches(Kind::Keyboard), matches(Kind::Mouse))
}
pub fn status() -> Status {
    let Some(_g) = Guard::enter() else {
        return Status {
            reason: Text::literal(b"USB HID controller busy; snapshot unavailable"),
            ..Status::EMPTY
        };
    };
    unsafe {
        refresh_aggregate_status();
        current().read()
    }
}
pub fn service_poll() {
    let Some(_g) = Guard::enter() else {
        return;
    };
    unsafe {
        (*state()).service_polls = (*state()).service_polls.wrapping_add(1);
        poll_binding(binding(Kind::Keyboard), Kind::Keyboard);
        poll_mouse();
        poll_binding(binding(Kind::Consumer), Kind::Consumer);
        usb_hid_platform_usb(
            (*binding(Kind::Consumer)).bound && (*binding(Kind::Consumer)).consumer_caps != 0,
        );
    }
}
pub fn poll_task_summary() -> PollTaskSummary {
    PollTaskSummary {
        started: POLL_STARTED.load(Ordering::Acquire),
        task_id: POLL_TASK_ID.load(Ordering::Acquire),
        iterations: POLL_ITERATIONS.load(Ordering::Relaxed),
    }
}
pub fn start_poll_task() -> bool {
    let Some(_g) = Guard::enter() else {
        return false;
    };
    unsafe { start_poll_task_locked() }
}
unsafe fn start_poll_task_locked() -> bool {
    unsafe {
        if POLL_STARTED.load(Ordering::Acquire) {
            return true;
        }
        let request = task_api::CreateRequest {
            name: b"usb-hid-poll".as_ptr(),
            name_len: 12,
            state: 3,
            role: 0,
            needs_fpu: true,
            smp_eligible: false,
            smp_r4x: false,
            owner_kind: 0,
            owner_context: ptr::null_mut(),
            entry: Some(poll_task_main),
        };
        let mut failure = 0;
        let worker = task_api::r4os_rust_task_create(&request, false, &mut failure);
        if worker.is_null() {
            puts(b"USBHID poll-task create failed\r\n");
            return false;
        }
        let id = (*task_registry::core(worker.cast())).id;
        POLL_STARTED.store(true, Ordering::Release);
        POLL_TASK_ID.store(id, Ordering::Release);
        task_api::r4os_rust_task_mutate(0, worker, timer::tick_count(), 0, ptr::null(), 0);
        puts(b"USBHID poll-task started id=");
        diag::console_dec(id);
        puts(b"\r\n");
        true
    }
}
unsafe extern "C" fn poll_task_main() {
    loop {
        service_poll();
        let iteration = POLL_ITERATIONS
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1);
        if (iteration == 300 || (iteration > 300 && (iteration - 300).is_multiple_of(6000)))
            && let Some(_g) = Guard::enter()
        {
            unsafe {
                emit_poll_task_marker();
            }
        }
        unsafe {
            scheduler::sleep(POLL_IDLE_TICKS, b"usb-hid-poll-idle".as_ptr(), 17);
        }
    }
}

unsafe fn emit_poll_task_marker() {
    unsafe {
        let xs = xhci::status();
        let keys = queue_stats();
        puts(b"[USBHIDPOLL] service_polls=");
        diag::console_dec((*state()).service_polls);
        puts(b" interrupt_polls=");
        diag::console_dec(xs.interrupt_polls);
        puts(b" reports=");
        diag::console_dec(xs.interrupt_reports);
        puts(b" no_report=");
        diag::console_dec(xs.interrupt_no_report);
        puts(b" halted_checks=");
        diag::console_dec(xs.interrupt_halted_checks);
        puts(b" recoveries=");
        diag::console_dec(xs.interrupt_recoveries);
        puts(b" pending_timeouts=");
        diag::console_dec(xs.interrupt_pending_timeouts);
        puts(b" ring_enqueue=");
        diag::console_dec(xs.interrupt_enqueue);
        puts(b" ring_pcs=");
        diag::console_dec(xs.interrupt_cycle);
        puts(b" link_c=");
        diag::console_dec(xs.interrupt_link_cycle);
        puts(b" ring_wraps=");
        diag::console_dec(xs.interrupt_ring_wraps);
        puts(b" pending=");
        diag::console_dec(u8::from(xs.interrupt_pending));
        puts(b" pending_index=");
        diag::console_dec(xs.interrupt_pending_index);
        puts(b" pending_streak=");
        diag::console_dec(xs.interrupt_pending_streak);
        puts(b" ep_state=");
        diag::console_dec(xs.last_interrupt_ep_state);
        puts(b" event_wraps=");
        diag::console_dec(xs.event_ring_wraps);
        puts(b" events=");
        diag::console_dec(xs.events);
        puts(b" event_dequeue=");
        diag::console_dec(xs.event_dequeue);
        puts(b" event_ccs=");
        diag::console_dec(xs.event_cycle);
        puts(b" event_raw_c=");
        diag::console_dec(xs.event_raw_cycle);
        puts(b" event_raw_type=");
        diag::console_dec(xs.event_raw_type);
        puts(b" event_raw_code=");
        diag::console_dec(xs.event_raw_code);
        puts(b" event_commits=");
        diag::console_dec(xs.event_erdp_commits);
        puts(b" event_max_batch=");
        diag::console_dec(xs.event_max_batch);
        puts(b" event_tick_waits=");
        diag::console_dec(xs.event_tick_deadline_waits);
        puts(b" event_hpet_waits=");
        diag::console_dec(xs.event_hpet_deadline_waits);
        puts(b" event_tsc_waits=");
        diag::console_dec(xs.event_tsc_deadline_waits);
        puts(b" event_hpet_timeouts=");
        diag::console_dec(xs.event_hpet_timeouts);
        puts(b" event_tsc_timeouts=");
        diag::console_dec(xs.event_tsc_timeouts);
        puts(b" event_cpu_guard_timeouts=");
        diag::console_dec(xs.event_cpu_guard_timeouts);
        puts(b" ring_full=");
        diag::console_dec(xs.ring_full);
        puts(b" stale_events=");
        diag::console_dec(xs.stale_events);
        puts(b" port_events=");
        diag::console_dec(xs.port_change_events);
        puts(b" port_pending=");
        diag::console_dec(xs.port_change_pending);
        puts(b" port_coalesced=");
        diag::console_dec(xs.port_change_coalesced);
        puts(b" keyboard_pending=");
        diag::console_dec(keys.pending);
        puts(b" keyboard_free=");
        diag::console_dec(keys.capacity - keys.pending);
        puts(b" keyboard_drops=");
        diag::console_dec(keys.drops);
        puts(b" backpressure=");
        diag::console_dec(u8::from((*state()).keyboard_backpressure_active));
        puts(b" backpressure_polls=");
        diag::console_dec((*state()).keyboard_backpressure_polls);
        puts(b" erdp=0x");
        diag::console_hex(xs.erdp0, 16);
        puts(b" iman=0x");
        diag::console_hex(xs.iman0, 8);
        puts(b" usbsts=0x");
        diag::console_hex(xs.usbsts, 8);
        puts(b" hw_dequeue=0x");
        diag::console_hex(xs.interrupt_hw_dequeue, 16);
        puts(b" deferred_pending=");
        diag::console_dec(xs.deferred_events_pending as u64);
        puts(b" deferred_queued=");
        diag::console_dec(xs.deferred_events_queued);
        puts(b" deferred_delivered=");
        diag::console_dec(xs.deferred_events_delivered);
        puts(b" deferred_overflows=");
        diag::console_dec(xs.deferred_events_overflows);
        puts(b" deferred_purged=");
        diag::console_dec(xs.deferred_events_purged);
        puts(b"\r\n");
    }
}

/// # Safety
/// Exclusive paired output remains resident for this observer call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_hid_status(out: *mut Status) {
    unsafe {
        out.write(status());
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_usb_hid_init(mouse: bool) -> bool {
    init(Options { bind_mouse: mouse })
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_usb_hid_service_poll() {
    service_poll();
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_usb_hid_start_poll_task() -> bool {
    start_poll_task()
}
/// # Safety
/// Exclusive paired output remains resident for the copied atomic summary.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_hid_poll_task_summary(out: *mut PollTaskSummary) {
    unsafe {
        out.write(poll_task_summary());
    }
}

unsafe fn usb_hid_can_accept(count: u32) -> bool {
    crate::input::keyboard::r4os_rust_input_keyboard_accept(count)
}

unsafe fn usb_hid_consumer(usage: u16) {
    crate::input::platform_input::r4os_rust_input_platform_consumer(u32::from(usage));
}

unsafe fn usb_hid_mouse(dx: i32, dy: i32, buttons: u8, wheel: i32) {
    crate::input::mouse::r4os_rust_input_mouse_inject(dx, dy, buttons, wheel);
}

unsafe fn usb_hid_platform_usb(value: bool) {
    crate::input::platform_input::r4os_rust_input_platform_usb(value);
}

unsafe fn usb_hid_set_keyboard_hook(hook: Option<unsafe extern "C" fn()>) {
    crate::input::keyboard::r4os_rust_input_keyboard_poll_hook(hook);
}

unsafe fn usb_hid_set_mouse_hook(hook: Option<unsafe extern "C" fn()>) {
    crate::input::mouse::r4os_rust_input_mouse_poll_hook(hook);
}

unsafe fn usb_hid_queue(out: *mut QueueStats) {
    let s = crate::input::keyboard::snapshot();
    unsafe {
        out.write(QueueStats {
            capacity: s.queue_capacity,
            pending: s.queue_pending,
            drops: s.dropped_count,
        });
    }
}

unsafe fn usb_hid_inject_modifier(bit: u8, down: bool) {
    let code = crate::input::hid_set1::modifier_to_code(bit).unwrap();
    if code.extended {
        crate::input::keyboard::controller_byte(0xE0);
    }
    crate::input::keyboard::controller_byte(if down { code.make } else { code.make | 0x80 });
}

unsafe fn usb_hid_inject_usage(usage: u8, down: bool) -> bool {
    let Some(code) = crate::input::hid_set1::usage_to_code(usage) else {
        return false;
    };
    if code.extended {
        crate::input::keyboard::controller_byte(0xE0);
    }
    crate::input::keyboard::controller_byte(if down { code.make } else { code.make | 0x80 });
    true
}

unsafe fn usb_hid_released(op: *const c::UsbHidBootOp, out: *mut u8, capacity: usize) -> usize {
    unsafe {
        let op = &*op;
        let pl = usize::from(op.previous_len).min(op.previous.len());
        let cl = usize::from(op.report_len).min(op.report.len());
        crate::input::hid_set1::collect_released(
            &op.previous[..pl],
            usize::from(op.previous_offset).min(pl),
            &op.report[..cl],
            usize::from(op.report_offset).min(cl),
            core::slice::from_raw_parts_mut(out, capacity),
        )
    }
}
