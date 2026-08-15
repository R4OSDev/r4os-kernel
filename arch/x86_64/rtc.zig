const io = @import("io.zig");

const CMOS_INDEX: u16 = 0x70;
const CMOS_DATA: u16 = 0x71;
const REG_SECONDS: u8 = 0x00;
const REG_MINUTES: u8 = 0x02;
const REG_HOURS: u8 = 0x04;
const REG_WEEKDAY: u8 = 0x06;
const REG_DAY: u8 = 0x07;
const REG_MONTH: u8 = 0x08;
const REG_YEAR: u8 = 0x09;
const REG_STATUS_A: u8 = 0x0A;
const REG_STATUS_B: u8 = 0x0B;
const REG_CENTURY_FALLBACK: u8 = 0x32;

const STATUS_A_UIP: u8 = 0x80;
const STATUS_B_24H: u8 = 0x02;
const STATUS_B_BINARY: u8 = 0x04;

pub const CenturySource = enum {
    none,
    fadt,
    fallback,
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

    pub fn secondsSinceMidnight(self: DateTime) u32 {
        return @as(u32, self.hour) * 3600 + @as(u32, self.minute) * 60 + @as(u32, self.second);
    }
};

const RawDateTime = struct {
    second: u8 = 0,
    minute: u8 = 0,
    hour: u8 = 0,
    weekday: u8 = 0,
    day: u8 = 1,
    month: u8 = 1,
    year: u8 = 0,
    century: u8 = 0,
    status_b: u8 = 0,
};

pub fn secondsSinceMidnight() u32 {
    return readDateTime(0).secondsSinceMidnight();
}

pub fn readDateTime(fadt_century_register: u8) DateTime {
    const raw = readRawStable(fadt_century_register);
    const binary = (raw.status_b & STATUS_B_BINARY) != 0;

    var sec = normalize(raw.second, binary);
    var min = normalize(raw.minute, binary);
    var hr_raw = raw.hour;
    const pm = (hr_raw & 0x80) != 0;
    hr_raw &= 0x7F;
    var hr = normalize(hr_raw, binary);
    var weekday = normalize(raw.weekday, binary);
    var day = normalize(raw.day, binary);
    var month = normalize(raw.month, binary);
    const year_two = normalize(raw.year, binary);

    if ((raw.status_b & STATUS_B_24H) == 0) {
        if (pm and hr < 12) hr += 12;
        if (!pm and hr == 12) hr = 0;
    }

    if (sec > 59) sec = 0;
    if (min > 59) min = 0;
    if (hr > 23) hr = 0;
    if (weekday > 7) weekday = 0;
    if (day < 1 or day > 31) day = 1;
    if (month < 1 or month > 12) month = 1;

    var source: CenturySource = .none;
    var century_reg: u8 = 0;
    var full_year: u16 = 0;
    if (fadt_century_register != 0) {
        const century = normalize(raw.century, binary);
        if (century >= 19 and century <= 99) {
            full_year = @as(u16, century) * 100 + @as(u16, year_two);
            source = .fadt;
            century_reg = fadt_century_register;
        }
    }
    if (full_year == 0 and fadt_century_register == 0) {
        const fallback_century = normalize(read(REG_CENTURY_FALLBACK), binary);
        if (fallback_century >= 19 and fallback_century <= 99) {
            full_year = @as(u16, fallback_century) * 100 + @as(u16, year_two);
            source = .fallback;
            century_reg = REG_CENTURY_FALLBACK;
        }
    }
    if (full_year == 0) {
        full_year = if (year_two >= 80) 1900 + @as(u16, year_two) else 2000 + @as(u16, year_two);
    }

    const valid = full_year >= 1980 and full_year <= 2099 and month >= 1 and month <= 12 and day >= 1 and day <= 31;
    return .{
        .valid = valid,
        .year = full_year,
        .month = month,
        .day = day,
        .weekday = weekday,
        .hour = hr,
        .minute = min,
        .second = sec,
        .century_register = century_reg,
        .century_source = source,
    };
}

pub fn writeDateTime(fadt_century_register: u8, value: DateTime) bool {
    if (!validWriteDateTime(value)) return false;
    waitUntilStable();
    const status_b = read(REG_STATUS_B);
    const binary = (status_b & STATUS_B_BINARY) != 0;
    const hour_value = encodeHour(value.hour, binary, (status_b & STATUS_B_24H) != 0);
    const weekday = if (value.weekday <= 6) value.weekday + 1 else value.weekday;
    const year_two: u8 = @intCast(value.year % 100);
    const century: u8 = @intCast(value.year / 100);
    const century_register = if (fadt_century_register != 0) fadt_century_register else REG_CENTURY_FALLBACK;

    write(REG_SECONDS, encode(value.second, binary));
    write(REG_MINUTES, encode(value.minute, binary));
    write(REG_HOURS, hour_value);
    write(REG_WEEKDAY, encode(weekday, binary));
    write(REG_DAY, encode(value.day, binary));
    write(REG_MONTH, encode(value.month, binary));
    write(REG_YEAR, encode(year_two, binary));
    write(century_register, encode(century, binary));
    return true;
}

fn readRawStable(century_register: u8) RawDateTime {
    var last = RawDateTime{};
    var attempts: u8 = 0;
    while (attempts < 4) : (attempts += 1) {
        waitUntilStable();
        const a = readRaw(century_register);
        waitUntilStable();
        const b = readRaw(century_register);
        if (sameRaw(a, b)) return b;
        last = b;
    }
    return last;
}

fn readRaw(century_register: u8) RawDateTime {
    return .{
        .second = read(REG_SECONDS),
        .minute = read(REG_MINUTES),
        .hour = read(REG_HOURS),
        .weekday = read(REG_WEEKDAY),
        .day = read(REG_DAY),
        .month = read(REG_MONTH),
        .year = read(REG_YEAR),
        .century = if (century_register != 0) read(century_register) else 0,
        .status_b = read(REG_STATUS_B),
    };
}

fn sameRaw(a: RawDateTime, b: RawDateTime) bool {
    return a.second == b.second and
        a.minute == b.minute and
        a.hour == b.hour and
        a.weekday == b.weekday and
        a.day == b.day and
        a.month == b.month and
        a.year == b.year and
        a.century == b.century and
        a.status_b == b.status_b;
}

fn waitUntilStable() void {
    var guard: u16 = 0;
    while (guard < 10000) : (guard += 1) {
        if ((read(REG_STATUS_A) & STATUS_A_UIP) == 0) return;
    }
}

fn read(reg: u8) u8 {
    io.outb(CMOS_INDEX, reg | 0x80);
    io.wait();
    return io.inb(CMOS_DATA);
}

fn write(reg: u8, value: u8) void {
    io.outb(CMOS_INDEX, reg | 0x80);
    io.wait();
    io.outb(CMOS_DATA, value);
    io.wait();
}

fn normalize(value: u8, binary: bool) u8 {
    if (binary) return value;
    return ((value >> 4) * 10) + (value & 0x0F);
}

fn encode(value: u8, binary: bool) u8 {
    if (binary) return value;
    return ((value / 10) << 4) | (value % 10);
}

fn encodeHour(hour: u8, binary: bool, is_24h: bool) u8 {
    if (is_24h) return encode(hour, binary);
    const pm = hour >= 12;
    var h = hour % 12;
    if (h == 0) h = 12;
    var out = encode(h, binary);
    if (pm) out |= 0x80;
    return out;
}

fn validWriteDateTime(value: DateTime) bool {
    if (!value.valid) return false;
    if (value.year < 1980 or value.year > 2099) return false;
    if (value.month < 1 or value.month > 12) return false;
    if (value.day < 1 or value.day > daysInMonth(value.year, value.month)) return false;
    if (value.hour > 23 or value.minute > 59 or value.second > 59) return false;
    return true;
}

fn daysInMonth(year: u16, month: u8) u8 {
    return switch (month) {
        1, 3, 5, 7, 8, 10, 12 => 31,
        4, 6, 9, 11 => 30,
        2 => if (isLeapYear(year)) 29 else 28,
        else => 0,
    };
}

fn isLeapYear(year: u16) bool {
    return (year % 4 == 0 and year % 100 != 0) or (year % 400 == 0);
}
