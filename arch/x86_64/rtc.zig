// Private fixed C facade; bounded CMOS I/O and calendar policy live in Rust.
extern fn r4os_rust_rtc_read(century: u8, output: *RawDateTime) void;
extern fn r4os_rust_rtc_write(century: u8, value: *const RawDateTime) bool;
extern fn r4os_rust_rtc_value(operation: u8, value: *const RawDateTime) u32;
pub const CenturySource = enum { none, fadt, fallback };
const RawDateTime = extern struct {
    year: u16,
    valid: bool,
    month: u8,
    day: u8,
    weekday: u8,
    hour: u8,
    minute: u8,
    second: u8,
    century_register: u8,
    century_source: u8,
};
pub const DateTime = struct {
    valid: bool = false,
    year: u16 = 1980,
    month: u8 = 1,
    day: u8 = 1,
    weekday: u8 = 0,
    hour: u8 = 0,
    minute: u8 = 0,
    second: u8 = 0,
    century_register: u8 = 0,
    century_source: CenturySource = .none,
    fn raw(self: DateTime) RawDateTime {
        return .{ .year = self.year, .valid = self.valid, .month = self.month, .day = self.day, .weekday = self.weekday, .hour = self.hour, .minute = self.minute, .second = self.second, .century_register = self.century_register, .century_source = @intFromEnum(self.century_source) };
    }
    pub fn secondsSinceMidnight(self: DateTime) u32 {
        const value = self.raw();
        return r4os_rust_rtc_value(0, &value);
    }
};
pub fn secondsSinceMidnight() u32 {
    return readDateTime(0).secondsSinceMidnight();
}
pub fn readDateTime(century: u8) DateTime {
    var value: RawDateTime = undefined;
    r4os_rust_rtc_read(century, &value);
    return .{ .valid = value.valid, .year = value.year, .month = value.month, .day = value.day, .weekday = value.weekday, .hour = value.hour, .minute = value.minute, .second = value.second, .century_register = value.century_register, .century_source = @enumFromInt(value.century_source) };
}
pub fn writeDateTime(century: u8, value: DateTime) bool {
    const raw = value.raw();
    return r4os_rust_rtc_write(century, &raw);
}
pub fn validDateTime(value: DateTime) bool {
    const raw = value.raw();
    return r4os_rust_rtc_value(1, &raw) != 0;
}
comptime {
    if (@sizeOf(RawDateTime) != 12 or @offsetOf(RawDateTime, "valid") != 2 or
        @offsetOf(RawDateTime, "century_source") != 10) @compileError("Rust RTC C layout differs");
}
