//! Private HID report projections, separate from the canonical R4P wire ABI.
use crate::sched::sync_types::Text;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ReportField {
    pub kind: u8,
    pub usage_page: u16,
    pub usage_min: u32,
    pub usage_max: u32,
    pub usages: [u32; 8],
    pub usage_count: u8,
    pub report_id: u8,
    pub bit_offset: u16,
    pub bit_size: u8,
    pub count: u8,
    pub logical_min: i32,
    pub logical_max: i32,
    pub flags: u8,
    pub relative: bool,
    pub variable: bool,
    pub constant: bool,
}
impl ReportField {
    pub const EMPTY: Self = Self {
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
        relative: false,
        variable: false,
        constant: false,
    };
}
const _: () = {
    assert!(core::mem::size_of::<ReportField>() == 64);
    assert!(core::mem::offset_of!(ReportField, kind) == 0);
    assert!(core::mem::offset_of!(ReportField, usage_page) == 2);
    assert!(core::mem::offset_of!(ReportField, usage_min) == 4);
    assert!(core::mem::offset_of!(ReportField, usage_max) == 8);
    assert!(core::mem::offset_of!(ReportField, usages) == 12);
    assert!(core::mem::offset_of!(ReportField, usage_count) == 44);
    assert!(core::mem::offset_of!(ReportField, report_id) == 45);
    assert!(core::mem::offset_of!(ReportField, bit_offset) == 46);
    assert!(core::mem::offset_of!(ReportField, bit_size) == 48);
    assert!(core::mem::offset_of!(ReportField, count) == 49);
    assert!(core::mem::offset_of!(ReportField, logical_min) == 52);
    assert!(core::mem::offset_of!(ReportField, logical_max) == 56);
    assert!(core::mem::offset_of!(ReportField, flags) == 60);
    assert!(core::mem::offset_of!(ReportField, relative) == 61);
    assert!(core::mem::offset_of!(ReportField, variable) == 62);
    assert!(core::mem::offset_of!(ReportField, constant) == 63);
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ReportSummary {
    pub parsed: bool,
    pub malformed: bool,
    pub has_report_id: bool,
    pub report_ids: u8,
    pub input_fields: u8,
    pub output_fields: u8,
    pub feature_fields: u8,
    pub fields: [ReportField; 24],
    pub field_count: u8,
    pub input_bits: [u16; 16],
    pub usage_keyboard: bool,
    pub usage_mouse: bool,
    pub usage_pointer: bool,
    pub usage_x: bool,
    pub usage_y: bool,
    pub usage_wheel: bool,
    pub usage_buttons: bool,
    pub reason: Text,
}
impl ReportSummary {
    pub const EMPTY: Self = Self {
        parsed: false,
        malformed: false,
        has_report_id: false,
        report_ids: 0,
        input_fields: 0,
        output_fields: 0,
        feature_fields: 0,
        fields: [ReportField::EMPTY; 24],
        field_count: 0,
        input_bits: [0; 16],
        usage_keyboard: false,
        usage_mouse: false,
        usage_pointer: false,
        usage_x: false,
        usage_y: false,
        usage_wheel: false,
        usage_buttons: false,
        reason: Text::literal(b"not parsed"),
    };
}
const _: () = {
    assert!(core::mem::size_of::<ReportSummary>() == 1608);
    assert!(core::mem::offset_of!(ReportSummary, parsed) == 0);
    assert!(core::mem::offset_of!(ReportSummary, malformed) == 1);
    assert!(core::mem::offset_of!(ReportSummary, has_report_id) == 2);
    assert!(core::mem::offset_of!(ReportSummary, report_ids) == 3);
    assert!(core::mem::offset_of!(ReportSummary, input_fields) == 4);
    assert!(core::mem::offset_of!(ReportSummary, output_fields) == 5);
    assert!(core::mem::offset_of!(ReportSummary, feature_fields) == 6);
    assert!(core::mem::offset_of!(ReportSummary, fields) == 8);
    assert!(core::mem::offset_of!(ReportSummary, field_count) == 1544);
    assert!(core::mem::offset_of!(ReportSummary, input_bits) == 1546);
    assert!(core::mem::offset_of!(ReportSummary, usage_keyboard) == 1578);
    assert!(core::mem::offset_of!(ReportSummary, usage_mouse) == 1579);
    assert!(core::mem::offset_of!(ReportSummary, usage_pointer) == 1580);
    assert!(core::mem::offset_of!(ReportSummary, usage_x) == 1581);
    assert!(core::mem::offset_of!(ReportSummary, usage_y) == 1582);
    assert!(core::mem::offset_of!(ReportSummary, usage_wheel) == 1583);
    assert!(core::mem::offset_of!(ReportSummary, usage_buttons) == 1584);
    assert!(core::mem::offset_of!(ReportSummary, reason) == 1592);
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ReportStatus {
    pub source: Text,
    pub r4p_parse: u64,
    pub required_missing: u64,
    pub dispatch_failures: u64,
    pub last_result: i32,
}
impl ReportStatus {
    pub const EMPTY: Self = Self {
        source: Text::literal(b"none"),
        r4p_parse: 0,
        required_missing: 0,
        dispatch_failures: 0,
        last_result: 0,
    };
}
const _: () = {
    assert!(core::mem::size_of::<ReportStatus>() == 48);
    assert!(core::mem::offset_of!(ReportStatus, source) == 0);
    assert!(core::mem::offset_of!(ReportStatus, r4p_parse) == 16);
    assert!(core::mem::offset_of!(ReportStatus, required_missing) == 24);
    assert!(core::mem::offset_of!(ReportStatus, dispatch_failures) == 32);
    assert!(core::mem::offset_of!(ReportStatus, last_result) == 40);
};
