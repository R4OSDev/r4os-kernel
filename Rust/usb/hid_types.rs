//! Native HID binding and fixed private diagnostic records.
use super::{
    hid_report_types::ReportSummary,
    xhci_types::{DeviceHandle, EndpointHandle},
};
use crate::{contract, sched::sync_types::Text};
const EMPTY_WIRE: contract::HidReportSummary = contract::HidReportSummary {
    parsed: 0,
    malformed: 0,
    has_report_id: 0,
    report_ids: 0,
    input_fields: 0,
    output_fields: 0,
    feature_fields: 0,
    field_count: 0,
    input_bits: [0; 16],
    usage_keyboard: 0,
    usage_mouse: 0,
    usage_pointer: 0,
    usage_x: 0,
    usage_y: 0,
    usage_wheel: 0,
    usage_buttons: 0,
    reserved0: 0,
    reason_code: 1,
    reserved1: 0,
    fields: [contract::HidReportField {
        kind: 0,
        usage_page: 65535,
        usage_min: 0,
        usage_max: 0,
        usages: [0; 8],
        usage_count: 0,
        report_id: 0,
        bit_offset: 0,
        bit_size: 0,
        count: 0,
        logical_min: 0,
        logical_max: 0,
        flags: 0,
        relative: 0,
        variable: 0,
        constant: 0,
        reserved: 0,
    }; 24],
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct HidInterfaceCandidate {
    pub number: u8,
    pub class_code: u8,
    pub subclass: u8,
    pub protocol: u8,
    pub endpoint_address: u8,
    pub endpoint_max_packet: u16,
    pub endpoint_interval: u8,
    pub report_descriptor_len: u16,
}
impl HidInterfaceCandidate {
    pub const EMPTY: Self = Self {
        number: 0,
        class_code: 0,
        subclass: 0,
        protocol: 0,
        endpoint_address: 0,
        endpoint_max_packet: 0,
        endpoint_interval: 0,
        report_descriptor_len: 0,
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct HidBinding {
    pub present: bool,
    pub bound: bool,
    pub port: u8,
    pub device: DeviceHandle,
    pub endpoint: EndpointHandle,
    pub interface_number: u8,
    pub endpoint_address: u8,
    pub endpoint_interval: u8,
    pub endpoint_max_packet: u16,
    pub report_descriptor_len: u16,
    pub report_descriptor_read_len: u16,
    pub report_summary: ReportSummary,
    pub report_wire: contract::HidReportSummary,
    pub consumer_pressed: [u8; 16],
    pub consumer_caps: u32,
    pub report_protocol: bool,
    pub polls: u64,
    pub no_reports: u64,
    pub reports: u64,
    pub duplicate_reports: u64,
    pub failures: u64,
    pub setup_warnings: u8,
    pub protocol_ok: bool,
    pub idle_ok: bool,
    pub report_id_heuristic: bool,
    pub report_descriptor_ok: bool,
    pub report_descriptor_malformed: bool,
    pub last_report_request_len: u8,
    pub last_report_residue: u32,
    pub last_report_len: u8,
    pub last_nonzero_report_len: u8,
    pub last_report: [u8; 32],
    pub last_nonzero_report: [u8; 32],
}
impl HidBinding {
    pub const EMPTY: Self = Self {
        present: false,
        bound: false,
        port: 0,
        device: DeviceHandle::EMPTY,
        endpoint: EndpointHandle::EMPTY,
        interface_number: 0,
        endpoint_address: 0,
        endpoint_interval: 0,
        endpoint_max_packet: 0,
        report_descriptor_len: 0,
        report_descriptor_read_len: 0,
        report_summary: ReportSummary::EMPTY,
        report_wire: EMPTY_WIRE,
        consumer_pressed: [0; 16],
        consumer_caps: 0,
        report_protocol: false,
        polls: 0,
        no_reports: 0,
        reports: 0,
        duplicate_reports: 0,
        failures: 0,
        setup_warnings: 0,
        protocol_ok: false,
        idle_ok: false,
        report_id_heuristic: false,
        report_descriptor_ok: false,
        report_descriptor_malformed: false,
        last_report_request_len: 0,
        last_report_residue: 0,
        last_report_len: 0,
        last_nonzero_report_len: 0,
        last_report: [0; 32],
        last_nonzero_report: [0; 32],
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Status {
    pub initialized: bool,
    pub keyboard_present: bool,
    pub keyboard_bound: bool,
    pub mouse_present: bool,
    pub mouse_bound: bool,
    pub unsupported_hid: u8,
    pub missing_endpoint_hid: u8,
    pub port: u8,
    pub interface_number: u8,
    pub endpoint_address: u8,
    pub endpoint_interval: u8,
    pub endpoint_max_packet: u16,
    pub polls: u64,
    pub no_reports: u64,
    pub reports: u64,
    pub duplicate_reports: u64,
    pub service_polls: u64,
    pub decoded_keys: u64,
    pub decoded_mouse: u64,
    pub drops: u64,
    pub keyboard_backpressure_active: bool,
    pub keyboard_backpressure_polls: u64,
    pub keyboard_queue_capacity: u32,
    pub keyboard_queue_pending: u32,
    pub keyboard_queue_free: u32,
    pub keyboard_queue_drops: u64,
    pub topology_reconciles: u64,
    pub failures: u64,
    pub setup_warnings: u8,
    pub protocol_ok: bool,
    pub idle_ok: bool,
    pub report_id_heuristic: bool,
    pub report_descriptor_ok: bool,
    pub report_descriptor_malformed: bool,
    pub last_report_request_len: u8,
    pub last_report_residue: u32,
    pub last_report_len: u8,
    pub last_nonzero_report_len: u8,
    pub last_modifiers: u8,
    pub last_usage: u8,
    pub last_mouse_buttons: u8,
    pub last_mouse_dx: i32,
    pub last_mouse_dy: i32,
    pub last_mouse_wheel: i32,
    pub boot_source: Text,
    pub boot_r4p_classify: u64,
    pub boot_r4p_keyboard: u64,
    pub boot_r4p_mouse: u64,
    pub protocol_required_missing: u8,
    pub boot_dispatch_failures: u64,
    pub boot_last_result: i32,
    pub reason: Text,
}
impl Status {
    pub const EMPTY: Self = Self {
        initialized: false,
        keyboard_present: false,
        keyboard_bound: false,
        mouse_present: false,
        mouse_bound: false,
        unsupported_hid: 0,
        missing_endpoint_hid: 0,
        port: 0,
        interface_number: 0,
        endpoint_address: 0,
        endpoint_interval: 0,
        endpoint_max_packet: 0,
        polls: 0,
        no_reports: 0,
        reports: 0,
        duplicate_reports: 0,
        service_polls: 0,
        decoded_keys: 0,
        decoded_mouse: 0,
        drops: 0,
        keyboard_backpressure_active: false,
        keyboard_backpressure_polls: 0,
        keyboard_queue_capacity: 0,
        keyboard_queue_pending: 0,
        keyboard_queue_free: 0,
        keyboard_queue_drops: 0,
        topology_reconciles: 0,
        failures: 0,
        setup_warnings: 0,
        protocol_ok: false,
        idle_ok: false,
        report_id_heuristic: false,
        report_descriptor_ok: false,
        report_descriptor_malformed: false,
        last_report_request_len: 0,
        last_report_residue: 0,
        last_report_len: 0,
        last_nonzero_report_len: 0,
        last_modifiers: 0,
        last_usage: 0,
        last_mouse_buttons: 0,
        last_mouse_dx: 0,
        last_mouse_dy: 0,
        last_mouse_wheel: 0,
        boot_source: Text::literal(b"none"),
        boot_r4p_classify: 0,
        boot_r4p_keyboard: 0,
        boot_r4p_mouse: 0,
        protocol_required_missing: 0,
        boot_dispatch_failures: 0,
        boot_last_result: 0,
        reason: Text::literal(b"not initialized"),
    };
}
const _: () = {
    assert!(core::mem::size_of::<Status>() == 248);
    assert!(core::mem::align_of::<Status>() == 8);
    assert!(core::mem::offset_of!(Status, initialized) == 0);
    assert!(core::mem::offset_of!(Status, keyboard_present) == 1);
    assert!(core::mem::offset_of!(Status, keyboard_bound) == 2);
    assert!(core::mem::offset_of!(Status, mouse_present) == 3);
    assert!(core::mem::offset_of!(Status, mouse_bound) == 4);
    assert!(core::mem::offset_of!(Status, unsupported_hid) == 5);
    assert!(core::mem::offset_of!(Status, missing_endpoint_hid) == 6);
    assert!(core::mem::offset_of!(Status, port) == 7);
    assert!(core::mem::offset_of!(Status, interface_number) == 8);
    assert!(core::mem::offset_of!(Status, endpoint_address) == 9);
    assert!(core::mem::offset_of!(Status, endpoint_interval) == 10);
    assert!(core::mem::offset_of!(Status, endpoint_max_packet) == 12);
    assert!(core::mem::offset_of!(Status, polls) == 16);
    assert!(core::mem::offset_of!(Status, no_reports) == 24);
    assert!(core::mem::offset_of!(Status, reports) == 32);
    assert!(core::mem::offset_of!(Status, duplicate_reports) == 40);
    assert!(core::mem::offset_of!(Status, service_polls) == 48);
    assert!(core::mem::offset_of!(Status, decoded_keys) == 56);
    assert!(core::mem::offset_of!(Status, decoded_mouse) == 64);
    assert!(core::mem::offset_of!(Status, drops) == 72);
    assert!(core::mem::offset_of!(Status, keyboard_backpressure_active) == 80);
    assert!(core::mem::offset_of!(Status, keyboard_backpressure_polls) == 88);
    assert!(core::mem::offset_of!(Status, keyboard_queue_capacity) == 96);
    assert!(core::mem::offset_of!(Status, keyboard_queue_pending) == 100);
    assert!(core::mem::offset_of!(Status, keyboard_queue_free) == 104);
    assert!(core::mem::offset_of!(Status, keyboard_queue_drops) == 112);
    assert!(core::mem::offset_of!(Status, topology_reconciles) == 120);
    assert!(core::mem::offset_of!(Status, failures) == 128);
    assert!(core::mem::offset_of!(Status, setup_warnings) == 136);
    assert!(core::mem::offset_of!(Status, protocol_ok) == 137);
    assert!(core::mem::offset_of!(Status, idle_ok) == 138);
    assert!(core::mem::offset_of!(Status, report_id_heuristic) == 139);
    assert!(core::mem::offset_of!(Status, report_descriptor_ok) == 140);
    assert!(core::mem::offset_of!(Status, report_descriptor_malformed) == 141);
    assert!(core::mem::offset_of!(Status, last_report_request_len) == 142);
    assert!(core::mem::offset_of!(Status, last_report_residue) == 144);
    assert!(core::mem::offset_of!(Status, last_report_len) == 148);
    assert!(core::mem::offset_of!(Status, last_nonzero_report_len) == 149);
    assert!(core::mem::offset_of!(Status, last_modifiers) == 150);
    assert!(core::mem::offset_of!(Status, last_usage) == 151);
    assert!(core::mem::offset_of!(Status, last_mouse_buttons) == 152);
    assert!(core::mem::offset_of!(Status, last_mouse_dx) == 156);
    assert!(core::mem::offset_of!(Status, last_mouse_dy) == 160);
    assert!(core::mem::offset_of!(Status, last_mouse_wheel) == 164);
    assert!(core::mem::offset_of!(Status, boot_source) == 168);
    assert!(core::mem::offset_of!(Status, boot_r4p_classify) == 184);
    assert!(core::mem::offset_of!(Status, boot_r4p_keyboard) == 192);
    assert!(core::mem::offset_of!(Status, boot_r4p_mouse) == 200);
    assert!(core::mem::offset_of!(Status, protocol_required_missing) == 208);
    assert!(core::mem::offset_of!(Status, boot_dispatch_failures) == 216);
    assert!(core::mem::offset_of!(Status, boot_last_result) == 224);
    assert!(core::mem::offset_of!(Status, reason) == 232);
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Options {
    pub bind_mouse: bool,
}
impl Options {
    pub const EMPTY: Self = Self { bind_mouse: true };
}
const _: () = {
    assert!(core::mem::size_of::<Options>() == 1);
    assert!(core::mem::align_of::<Options>() == 1);
    assert!(core::mem::offset_of!(Options, bind_mouse) == 0);
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PollTaskSummary {
    pub started: bool,
    pub task_id: u32,
    pub iterations: u64,
}
impl PollTaskSummary {
    pub const EMPTY: Self = Self {
        started: false,
        task_id: 0,
        iterations: 0,
    };
}
const _: () = {
    assert!(core::mem::size_of::<PollTaskSummary>() == 16);
    assert!(core::mem::align_of::<PollTaskSummary>() == 8);
    assert!(core::mem::offset_of!(PollTaskSummary, started) == 0);
    assert!(core::mem::offset_of!(PollTaskSummary, task_id) == 4);
    assert!(core::mem::offset_of!(PollTaskSummary, iterations) == 8);
};
