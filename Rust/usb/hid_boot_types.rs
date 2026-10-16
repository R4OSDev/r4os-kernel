//! Native boot-HID status; only this owner publishes phase/registry outcome.
use crate::sched::sync_types::Text;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Status {
    pub phase: u8,
    pub attempted: bool,
    pub initialized: bool,
    pub init_returned_bound: bool,
    pub active: bool,
    pub failed: bool,
    pub fatal: bool,
    pub registry_slot_present: bool,
    pub registry_state: u8,
    pub keyboard_present: bool,
    pub keyboard_bound: bool,
    pub mouse_present: bool,
    pub mouse_bound: bool,
    pub unsupported_hid: u8,
    pub missing_endpoint_hid: u8,
    pub setup_warnings: u8,
    pub failures: u64,
    pub boot_dispatch_failures: u64,
    pub protocol_required_missing: u8,
    pub report_descriptor_malformed: bool,
    pub boot_source: Text,
    pub reason: Text,
}
impl Status {
    pub const EMPTY: Self = Self {
        phase: 0,
        attempted: false,
        initialized: false,
        init_returned_bound: false,
        active: false,
        failed: false,
        fatal: false,
        registry_slot_present: false,
        registry_state: 0,
        keyboard_present: false,
        keyboard_bound: false,
        mouse_present: false,
        mouse_bound: false,
        unsupported_hid: 0,
        missing_endpoint_hid: 0,
        setup_warnings: 0,
        failures: 0,
        boot_dispatch_failures: 0,
        protocol_required_missing: 0,
        report_descriptor_malformed: false,
        boot_source: Text::literal(b"none"),
        reason: Text::literal(b"not started"),
    };
}
const _: () = {
    assert!(core::mem::size_of::<Status>() == 72);
    assert!(core::mem::offset_of!(Status, phase) == 0);
    assert!(core::mem::offset_of!(Status, attempted) == 1);
    assert!(core::mem::offset_of!(Status, initialized) == 2);
    assert!(core::mem::offset_of!(Status, init_returned_bound) == 3);
    assert!(core::mem::offset_of!(Status, active) == 4);
    assert!(core::mem::offset_of!(Status, failed) == 5);
    assert!(core::mem::offset_of!(Status, fatal) == 6);
    assert!(core::mem::offset_of!(Status, registry_slot_present) == 7);
    assert!(core::mem::offset_of!(Status, registry_state) == 8);
    assert!(core::mem::offset_of!(Status, keyboard_present) == 9);
    assert!(core::mem::offset_of!(Status, keyboard_bound) == 10);
    assert!(core::mem::offset_of!(Status, mouse_present) == 11);
    assert!(core::mem::offset_of!(Status, mouse_bound) == 12);
    assert!(core::mem::offset_of!(Status, unsupported_hid) == 13);
    assert!(core::mem::offset_of!(Status, missing_endpoint_hid) == 14);
    assert!(core::mem::offset_of!(Status, setup_warnings) == 15);
    assert!(core::mem::offset_of!(Status, failures) == 16);
    assert!(core::mem::offset_of!(Status, boot_dispatch_failures) == 24);
    assert!(core::mem::offset_of!(Status, protocol_required_missing) == 32);
    assert!(core::mem::offset_of!(Status, report_descriptor_malformed) == 33);
    assert!(core::mem::offset_of!(Status, boot_source) == 40);
    assert!(core::mem::offset_of!(Status, reason) == 56);
};
