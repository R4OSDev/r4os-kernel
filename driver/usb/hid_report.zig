const protocol_api = @import("../../kernel/protocol_api.zig");
const r4p = @import("../../program/r4p.zig");
const r4p_contract = @import("../../net/r4p_contract.zig");

const MAX_FIELDS: usize = r4p_contract.HID_REPORT_MAX_FIELDS;
const MAX_FIELD_USAGES: usize = r4p_contract.HID_REPORT_MAX_FIELD_USAGES;

pub const UsagePage = enum(u16) {
    generic_desktop = 0x01,
    keyboard = 0x07,
    button = 0x09,
    consumer = 0x0C,
    unknown = 0xFFFF,

    pub fn fromRaw(value: u32) UsagePage {
        return switch (@as(u16, @truncate(value))) {
            0x01 => .generic_desktop,
            0x07 => .keyboard,
            0x09 => .button,
            0x0C => .consumer,
            else => .unknown,
        };
    }
};

pub const FieldKind = enum(u8) {
    input,
    output,
    feature,
};

pub const Field = struct {
    kind: FieldKind = .input,
    usage_page: UsagePage = .unknown,
    usage_min: u32 = 0,
    usage_max: u32 = 0,
    usages: [MAX_FIELD_USAGES]u32 = .{0} ** MAX_FIELD_USAGES,
    usage_count: u8 = 0,
    report_id: u8 = 0,
    bit_offset: u16 = 0,
    bit_size: u8 = 0,
    count: u8 = 0,
    logical_min: i32 = 0,
    logical_max: i32 = 0,
    flags: u8 = 0,
    relative: bool = false,
    variable: bool = false,
    constant: bool = false,
};

pub const Summary = struct {
    parsed: bool = false,
    malformed: bool = false,
    has_report_id: bool = false,
    report_ids: u8 = 0,
    input_fields: u8 = 0,
    output_fields: u8 = 0,
    feature_fields: u8 = 0,
    fields: [MAX_FIELDS]Field = .{Field{}} ** MAX_FIELDS,
    field_count: u8 = 0,
    input_bits: [16]u16 = .{0} ** 16,
    usage_keyboard: bool = false,
    usage_mouse: bool = false,
    usage_pointer: bool = false,
    usage_x: bool = false,
    usage_y: bool = false,
    usage_wheel: bool = false,
    usage_buttons: bool = false,
    reason: []const u8 = "not parsed",
};

pub const Status = struct {
    source: []const u8 = "none",
    r4p_parse: u64 = 0,
    required_missing: u64 = 0,
    dispatch_failures: u64 = 0,
    last_result: i32 = 0,
};

var status_state: Status = .{};

pub fn parse(bytes: []const u8) Summary {
    if (!r4p.hasActiveR4p("usb.hid_report")) {
        status_state.required_missing +%= 1;
        return .{ .reason = "HIDREPORT.R4P required" };
    }
    return parseR4p(bytes) orelse .{ .malformed = true, .reason = "HIDREPORT.R4P dispatch failed" };
}

pub fn status() Status {
    status_state.source = r4p.requiredSourceName("usb.hid_report");
    return status_state;
}

pub fn sourceName() []const u8 {
    return r4p.requiredSourceName("usb.hid_report");
}

fn parseR4p(bytes: []const u8) ?Summary {
    if (bytes.len > r4p_contract.HID_REPORT_MAX_DESCRIPTOR) {
        status_state.dispatch_failures +%= 1;
        status_state.last_result = r4p_contract.HID_REPORT_RESULT_BAD_LENGTH;
        return null;
    }
    var op: r4p_contract.HidReportOp = .{ .descriptor_len = @intCast(bytes.len) };
    if (bytes.len != 0) @memcpy(op.descriptor[0..bytes.len], bytes);
    var buffer = protocol_api.ProtocolBuffer{
        .data = &op,
        .len = @sizeOf(r4p_contract.HidReportOp),
        .capacity = @sizeOf(r4p_contract.HidReportOp),
        .flags = 0,
        .reserved = 0,
    };
    var out: protocol_api.ProtocolBuffer = .{};
    const result = r4p.dispatch("usb.hid_report", r4p_contract.HID_REPORT_OP_PARSE, &buffer, &out);
    status_state.last_result = result;
    if (result != r4p_contract.HID_REPORT_RESULT_OK or op.result != r4p_contract.HID_REPORT_RESULT_OK) {
        status_state.dispatch_failures +%= 1;
        status_state.last_result = if (result != r4p_contract.HID_REPORT_RESULT_OK) result else op.result;
        return null;
    }
    status_state.r4p_parse +%= 1;
    return summaryFromR4p(op.summary);
}

fn summaryFromR4p(in_summary: r4p_contract.HidReportSummary) Summary {
    var out: Summary = .{
        .parsed = in_summary.parsed != 0,
        .malformed = in_summary.malformed != 0,
        .has_report_id = in_summary.has_report_id != 0,
        .report_ids = in_summary.report_ids,
        .input_fields = in_summary.input_fields,
        .output_fields = in_summary.output_fields,
        .feature_fields = in_summary.feature_fields,
        .field_count = in_summary.field_count,
        .input_bits = in_summary.input_bits,
        .usage_keyboard = in_summary.usage_keyboard != 0,
        .usage_mouse = in_summary.usage_mouse != 0,
        .usage_pointer = in_summary.usage_pointer != 0,
        .usage_x = in_summary.usage_x != 0,
        .usage_y = in_summary.usage_y != 0,
        .usage_wheel = in_summary.usage_wheel != 0,
        .usage_buttons = in_summary.usage_buttons != 0,
        .reason = reasonName(in_summary.reason_code),
    };
    var i: usize = 0;
    while (i < out.field_count and i < MAX_FIELDS) : (i += 1) {
        out.fields[i] = fieldFromR4p(in_summary.fields[i]);
    }
    return out;
}

fn fieldFromR4p(in_field: r4p_contract.HidReportField) Field {
    return .{
        .kind = switch (in_field.kind) {
            r4p_contract.HID_REPORT_KIND_OUTPUT => .output,
            r4p_contract.HID_REPORT_KIND_FEATURE => .feature,
            else => .input,
        },
        .usage_page = UsagePage.fromRaw(in_field.usage_page),
        .usage_min = in_field.usage_min,
        .usage_max = in_field.usage_max,
        .usages = in_field.usages,
        .usage_count = in_field.usage_count,
        .report_id = in_field.report_id,
        .bit_offset = in_field.bit_offset,
        .bit_size = in_field.bit_size,
        .count = in_field.count,
        .logical_min = in_field.logical_min,
        .logical_max = in_field.logical_max,
        .flags = in_field.flags,
        .relative = in_field.relative != 0,
        .variable = in_field.variable != 0,
        .constant = in_field.constant != 0,
    };
}

fn reasonName(code: u16) []const u8 {
    return switch (code) {
        r4p_contract.HID_REPORT_REASON_PARSED => "parsed",
        r4p_contract.HID_REPORT_REASON_TRUNCATED_LONG_ITEM => "truncated long item",
        r4p_contract.HID_REPORT_REASON_TRUNCATED_LONG_PAYLOAD => "truncated long payload",
        r4p_contract.HID_REPORT_REASON_TRUNCATED_SHORT_ITEM => "truncated short item",
        else => "not parsed",
    };
}
