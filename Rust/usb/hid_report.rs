//! HID report adaptation and diagnostics. Decode remains in HIDREPORT.R4P;
//! no callback, pointer or mutable status reference survives its dispatch.
use super::hid_report_types::{ReportField, ReportStatus, ReportSummary};
use crate::{
    arch::runtime::RuntimeGuard, contract, fs::vfs_types::Bytes, loader::r4p,
    sched::sync_types::Text,
};
use core::{cell::UnsafeCell, mem, slice};
struct Storage(UnsafeCell<ReportStatus>);
// SAFETY: short RAM updates/snapshots use RuntimeGuard; no activation or
// protocol callback runs under it, and no reference escapes its scope.
unsafe impl Sync for Storage {}
static STATUS: Storage = Storage(UnsafeCell::new(ReportStatus::EMPTY));
fn change(f: impl FnOnce(&mut ReportStatus)) {
    let _guard = RuntimeGuard::acquire();
    f(unsafe { &mut *STATUS.0.get() });
}
fn role() -> Bytes {
    Bytes {
        data: b"usb.hid_report".as_ptr(),
        len: 14,
    }
}
fn source() -> Text {
    let s = unsafe { r4p::r4os_rust_r4p_source(role(), true) };
    Text {
        pointer: s.data,
        len: s.len,
    }
}
pub fn usage_page(value: u32) -> u16 {
    match value as u16 {
        1 => 1,
        7 => 7,
        9 => 9,
        12 => 12,
        _ => 65535,
    }
}
fn reason(code: u16) -> Text {
    Text::literal(match code {
        contract::hid_report_reason_parsed => b"parsed",
        contract::hid_report_reason_truncated_long_item => b"truncated long item",
        contract::hid_report_reason_truncated_long_payload => b"truncated long payload",
        contract::hid_report_reason_truncated_short_item => b"truncated short item",
        _ => b"not parsed",
    })
}
fn field(value: contract::HidReportField) -> ReportField {
    ReportField {
        kind: match value.kind {
            contract::hid_report_kind_output => 1,
            contract::hid_report_kind_feature => 2,
            _ => 0,
        },
        usage_page: usage_page(u32::from(value.usage_page)),
        usage_min: value.usage_min,
        usage_max: value.usage_max,
        usages: value.usages,
        usage_count: value.usage_count,
        report_id: value.report_id,
        bit_offset: value.bit_offset,
        bit_size: value.bit_size,
        count: value.count,
        logical_min: value.logical_min,
        logical_max: value.logical_max,
        flags: value.flags,
        relative: value.relative != 0,
        variable: value.variable != 0,
        constant: value.constant != 0,
    }
}
fn summary(value: contract::HidReportSummary) -> ReportSummary {
    let mut out = ReportSummary {
        parsed: value.parsed != 0,
        malformed: value.malformed != 0,
        has_report_id: value.has_report_id != 0,
        report_ids: value.report_ids,
        input_fields: value.input_fields,
        output_fields: value.output_fields,
        feature_fields: value.feature_fields,
        field_count: value.field_count,
        input_bits: value.input_bits,
        usage_keyboard: value.usage_keyboard != 0,
        usage_mouse: value.usage_mouse != 0,
        usage_pointer: value.usage_pointer != 0,
        usage_x: value.usage_x != 0,
        usage_y: value.usage_y != 0,
        usage_wheel: value.usage_wheel != 0,
        usage_buttons: value.usage_buttons != 0,
        reason: reason(value.reason_code),
        ..ReportSummary::EMPTY
    };
    for i in 0..usize::from(out.field_count).min(contract::hid_report_max_fields) {
        out.fields[i] = field(value.fields[i]);
    }
    out
}
pub(super) unsafe fn parse(bytes: &[u8], wire: *mut contract::HidReportSummary) -> ReportSummary {
    if !unsafe { r4p::r4os_rust_r4p_has_active(role()) } {
        change(|s| s.required_missing = s.required_missing.wrapping_add(1));
        return ReportSummary {
            reason: Text::literal(b"HIDREPORT.R4P required"),
            ..ReportSummary::EMPTY
        };
    }
    let result = unsafe { parse_r4p(bytes, wire) };
    result.unwrap_or(ReportSummary {
        malformed: true,
        reason: Text::literal(b"HIDREPORT.R4P dispatch failed"),
        ..ReportSummary::EMPTY
    })
}
unsafe fn parse_r4p(bytes: &[u8], wire: *mut contract::HidReportSummary) -> Option<ReportSummary> {
    if bytes.len() > contract::hid_report_max_descriptor {
        change(|s| {
            s.dispatch_failures = s.dispatch_failures.wrapping_add(1);
            s.last_result = contract::hid_report_result_bad_length;
        });
        return None;
    }
    let mut op = contract::HidReportOp {
        descriptor_len: u32::try_from(bytes.len()).expect("HID descriptor length"),
        ..contract::HidReportOp::default()
    };
    op.descriptor[..bytes.len()].copy_from_slice(bytes);
    let input = contract::ProtocolBuffer {
        data: (&mut op as *mut contract::HidReportOp).cast(),
        len: mem::size_of::<contract::HidReportOp>() as u32,
        capacity: mem::size_of::<contract::HidReportOp>() as u32,
        flags: 0,
        reserved: 0,
    };
    let mut output = contract::ProtocolBuffer::default();
    let result = unsafe {
        r4p::r4os_rust_r4p_dispatch(role(), contract::hid_report_op_parse, &input, &mut output)
    };
    change(|s| s.last_result = result);
    if result != contract::hid_report_result_ok || op.result != contract::hid_report_result_ok {
        change(|s| {
            s.dispatch_failures = s.dispatch_failures.wrapping_add(1);
            s.last_result = if result != contract::hid_report_result_ok {
                result
            } else {
                op.result
            };
        });
        return None;
    }
    change(|s| s.r4p_parse = s.r4p_parse.wrapping_add(1));
    unsafe { wire.write(op.summary) };
    Some(summary(op.summary))
}
pub(super) unsafe fn consumer(
    summary: *const contract::HidReportSummary,
    report: &[u8],
) -> Option<contract::HidConsumerOp> {
    if report.len() > 32 || !unsafe { r4p::r4os_rust_r4p_has_active(role()) } {
        return None;
    }
    let mut request = contract::HidConsumerOp {
        summary_address: summary.addr() as u64,
        report_len: report.len() as u32,
        ..contract::HidConsumerOp::default()
    };
    request.report[..report.len()].copy_from_slice(report);
    let input = contract::ProtocolBuffer {
        data: (&mut request as *mut contract::HidConsumerOp).cast(),
        len: mem::size_of::<contract::HidConsumerOp>() as u32,
        capacity: mem::size_of::<contract::HidConsumerOp>() as u32,
        flags: 0,
        reserved: 0,
    };
    let mut output = contract::ProtocolBuffer::default();
    if unsafe {
        r4p::r4os_rust_r4p_dispatch(
            role(),
            contract::hid_report_op_consumer,
            &input,
            &mut output,
        )
    } != 0
        || request.capabilities > 3
        || request.pressed > 3
        || (request.report_id != u32::MAX && request.report_id >= 16)
    {
        return None;
    }
    Some(request)
}

/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_hid_report_parse(
    bytes: *const u8,
    len: usize,
    wire: *mut contract::HidReportSummary,
    out: *mut ReportSummary,
) {
    unsafe {
        out.write(parse(
            if len == 0 {
                &[]
            } else {
                slice::from_raw_parts(bytes, len)
            },
            wire,
        ))
    }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_hid_report_status(out: *mut ReportStatus) {
    let source = source();
    let _guard = RuntimeGuard::acquire();
    let mut value = unsafe { STATUS.0.get().read() };
    value.source = source;
    unsafe { out.write(value) };
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_usb_hid_report_source() -> Text {
    source()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_usb_usage_page(value: u32) -> u16 {
    usage_page(value)
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_hid_consumer(
    summary: *const contract::HidReportSummary,
    report: *const u8,
    len: usize,
    out: *mut contract::HidConsumerOp,
) -> bool {
    if let Some(value) = unsafe {
        consumer(
            summary,
            if len == 0 {
                &[]
            } else {
                slice::from_raw_parts(report, len)
            },
        )
    } {
        unsafe { out.write(value) };
        true
    } else {
        false
    }
}
