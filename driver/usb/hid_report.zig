const Text = @import("core.zig").Text;
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
        return @enumFromInt(r4os_rust_usb_usage_page(value));
    }
};

pub const FieldKind = enum(u8) {
    input,
    output,
    feature,
};

pub const Field = extern struct {
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

pub const Summary = extern struct {
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
    reason: Text = Text.init("not parsed"),
};

pub const Status = struct {
    source: []const u8 = "none",
    r4p_parse: u64 = 0,
    required_missing: u64 = 0,
    dispatch_failures: u64 = 0,
    last_result: i32 = 0,
};

const RawStatus = extern struct { source: Text, r4p_parse: u64, required_missing: u64, dispatch_failures: u64, last_result: i32 };
extern fn r4os_rust_usb_usage_page(u32) u16;
extern fn r4os_rust_usb_hid_report_parse([*]const u8, usize, *r4p_contract.HidReportSummary, *Summary) void;
extern fn r4os_rust_usb_hid_report_status(*RawStatus) void;
extern fn r4os_rust_usb_hid_report_source() Text;
extern fn r4os_rust_usb_hid_consumer(*const r4p_contract.HidReportSummary, [*]const u8, usize, *r4p_contract.HidConsumerOp) bool;
pub fn parse(bytes: []const u8, wire: *r4p_contract.HidReportSummary) Summary {
    var out: Summary = undefined;
    r4os_rust_usb_hid_report_parse(bytes.ptr, bytes.len, wire, &out);
    return out;
}
pub fn status() Status {
    var out: RawStatus = undefined;
    r4os_rust_usb_hid_report_status(&out);
    return .{ .source = out.source.slice(), .r4p_parse = out.r4p_parse, .required_missing = out.required_missing, .dispatch_failures = out.dispatch_failures, .last_result = out.last_result };
}
pub fn sourceName() []const u8 {
    return r4os_rust_usb_hid_report_source().slice();
}
pub fn consumer(summary: *const r4p_contract.HidReportSummary, report: []const u8) ?r4p_contract.HidConsumerOp {
    var out: r4p_contract.HidConsumerOp = undefined;
    return if (r4os_rust_usb_hid_consumer(summary, report.ptr, report.len, &out)) out else null;
}
comptime {
    if (@sizeOf(Field) != 64 or
        @alignOf(Field) != 4 or
        @offsetOf(Field, "kind") != 0 or
        @offsetOf(Field, "usage_page") != 2 or
        @offsetOf(Field, "usage_min") != 4 or
        @offsetOf(Field, "usage_max") != 8 or
        @offsetOf(Field, "usages") != 12 or
        @offsetOf(Field, "usage_count") != 44 or
        @offsetOf(Field, "report_id") != 45 or
        @offsetOf(Field, "bit_offset") != 46 or
        @offsetOf(Field, "bit_size") != 48 or
        @offsetOf(Field, "count") != 49 or
        @offsetOf(Field, "logical_min") != 52 or
        @offsetOf(Field, "logical_max") != 56 or
        @offsetOf(Field, "flags") != 60 or
        @offsetOf(Field, "relative") != 61 or
        @offsetOf(Field, "variable") != 62 or
        @offsetOf(Field, "constant") != 63) @compileError("USB report Field C layout");
}
comptime {
    if (@sizeOf(Summary) != 1608 or
        @alignOf(Summary) != 8 or
        @offsetOf(Summary, "parsed") != 0 or
        @offsetOf(Summary, "malformed") != 1 or
        @offsetOf(Summary, "has_report_id") != 2 or
        @offsetOf(Summary, "report_ids") != 3 or
        @offsetOf(Summary, "input_fields") != 4 or
        @offsetOf(Summary, "output_fields") != 5 or
        @offsetOf(Summary, "feature_fields") != 6 or
        @offsetOf(Summary, "fields") != 8 or
        @offsetOf(Summary, "field_count") != 1544 or
        @offsetOf(Summary, "input_bits") != 1546 or
        @offsetOf(Summary, "usage_keyboard") != 1578 or
        @offsetOf(Summary, "usage_mouse") != 1579 or
        @offsetOf(Summary, "usage_pointer") != 1580 or
        @offsetOf(Summary, "usage_x") != 1581 or
        @offsetOf(Summary, "usage_y") != 1582 or
        @offsetOf(Summary, "usage_wheel") != 1583 or
        @offsetOf(Summary, "usage_buttons") != 1584 or
        @offsetOf(Summary, "reason") != 1592) @compileError("USB report Summary C layout");
}
comptime {
    if (@sizeOf(RawStatus) != 48 or
        @alignOf(RawStatus) != 8 or
        @offsetOf(RawStatus, "source") != 0 or
        @offsetOf(RawStatus, "r4p_parse") != 16 or
        @offsetOf(RawStatus, "required_missing") != 24 or
        @offsetOf(RawStatus, "dispatch_failures") != 32 or
        @offsetOf(RawStatus, "last_result") != 40) @compileError("USB report RawStatus C layout");
}
