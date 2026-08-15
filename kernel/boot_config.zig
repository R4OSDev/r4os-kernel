const vfs = @import("../fs/vfs.zig");
const bootlog = @import("bootlog.zig");
const loader_perf = @import("loader_perf.zig");

pub const MAX_DRIVERS: usize = 12;
pub const MAX_DISABLED: usize = 8;
pub const MAX_OPTIONS: usize = 8;
pub const MAX_SHELL_PATH: usize = 96;
pub const MAX_SHELL_ARGS: usize = 96;
const MAX_NAME: usize = 24;
const MAX_KEY: usize = 24;
const MAX_VALUE: usize = 48;
const DEFAULT_SHELL_PATH = "/R4OS/SOFTWARE/DESKTOP/R4DESK.R4X";

pub const Option = struct {
    driver: [MAX_NAME]u8 = .{0} ** MAX_NAME,
    driver_len: usize = 0,
    key: [MAX_KEY]u8 = .{0} ** MAX_KEY,
    key_len: usize = 0,
    value: [MAX_VALUE]u8 = .{0} ** MAX_VALUE,
    value_len: usize = 0,
};

pub const Config = struct {
    auto_pci: bool = true,
    auto_acpi: bool = true,
    log_verbose: bool = false,
    drivers: [MAX_DRIVERS][MAX_NAME]u8 = .{.{0} ** MAX_NAME} ** MAX_DRIVERS,
    driver_lens: [MAX_DRIVERS]usize = .{0} ** MAX_DRIVERS,
    driver_count: usize = 0,
    disabled: [MAX_DISABLED][MAX_NAME]u8 = .{.{0} ** MAX_NAME} ** MAX_DISABLED,
    disabled_lens: [MAX_DISABLED]usize = .{0} ** MAX_DISABLED,
    disabled_count: usize = 0,
    options: [MAX_OPTIONS]Option = .{Option{}} ** MAX_OPTIONS,
    option_count: usize = 0,
    shell_path: [MAX_SHELL_PATH]u8 = .{0} ** MAX_SHELL_PATH,
    shell_path_len: usize = 0,
    shell_args: [MAX_SHELL_ARGS]u8 = .{0} ** MAX_SHELL_ARGS,
    shell_args_len: usize = 0,
};

var current: Config = .{};

pub fn load() *const Config {
    const start_tick = loader_perf.now();
    resetDefaults();
    const volume = vfs.volumeForDrive('C') orelse {
        bootlog.puts("[CONF][WARN] C: FAT32 volume missing, using defaults\r\n");
        logDefaults();
        recordConfigPerf(start_tick, 0);
        return &current;
    };
    const entry = vfs.resolveEntry(volume, "/CONFIG.R4S") orelse {
        bootlog.puts("[CONF][WARN] /CONFIG.R4S missing, using defaults\r\n");
        logDefaults();
        recordConfigPerf(start_tick, 0);
        return &current;
    };
    var buf: [2048]u8 = undefined;
    const len = vfs.readFile(volume, entry, buf[0..]) orelse {
        bootlog.puts("[CONF][WARN] /CONFIG.R4S read failed, using defaults\r\n");
        logDefaults();
        recordConfigPerf(start_tick, 0);
        return &current;
    };
    bootlog.puts("[CONF] /CONFIG.R4S loaded bytes=");
    bootlog.putDec(len);
    bootlog.puts("\r\n");
    parse(stripBom(buf[0..len]));
    recordConfigPerf(start_tick, len);
    return &current;
}

pub fn get() *const Config {
    return &current;
}

pub fn driverName(index: usize) []const u8 {
    if (index >= current.driver_count) return "";
    return current.drivers[index][0..current.driver_lens[index]];
}

pub fn disabledName(index: usize) []const u8 {
    if (index >= current.disabled_count) return "";
    return current.disabled[index][0..current.disabled_lens[index]];
}

pub fn isDisabled(config: *const Config, name: []const u8) bool {
    var i: usize = 0;
    while (i < config.disabled_count) : (i += 1) {
        if (eqIgnoreCase(config.disabled[i][0..config.disabled_lens[i]], name)) return true;
    }
    return false;
}

pub fn hasDriver(config: *const Config, name: []const u8) bool {
    var i: usize = 0;
    while (i < config.driver_count) : (i += 1) {
        if (eqIgnoreCase(config.drivers[i][0..config.driver_lens[i]], name)) return true;
    }
    return false;
}

pub fn optionValue(config: *const Config, driver: []const u8, key: []const u8) ?[]const u8 {
    var i: usize = 0;
    while (i < config.option_count) : (i += 1) {
        const opt = &config.options[i];
        if (eqIgnoreCase(opt.driver[0..opt.driver_len], driver) and eqIgnoreCase(opt.key[0..opt.key_len], key)) {
            return opt.value[0..opt.value_len];
        }
    }
    return null;
}

pub fn shellPath(config: *const Config) []const u8 {
    return config.shell_path[0..config.shell_path_len];
}

pub fn shellArgs(config: *const Config) []const u8 {
    return config.shell_args[0..config.shell_args_len];
}

fn resetDefaults() void {
    current = .{};
    setShellPath(DEFAULT_SHELL_PATH);
}

fn parse(data: []const u8) void {
    var start: usize = 0;
    while (start < data.len) {
        var end = start;
        while (end < data.len and data[end] != '\n' and data[end] != '\r') : (end += 1) {}
        parseLine(trim(data[start..end]));
        start = end;
        while (start < data.len and (data[start] == '\n' or data[start] == '\r')) : (start += 1) {}
    }
    logSummary();
}

fn stripBom(data: []const u8) []const u8 {
    if (data.len >= 3 and data[0] == 0xEF and data[1] == 0xBB and data[2] == 0xBF) return data[3..];
    return data;
}

fn parseLine(line_raw: []const u8) void {
    const comment_pos = indexOf(line_raw, '#') orelse line_raw.len;
    const line = trim(line_raw[0..comment_pos]);
    if (line.len == 0) return;
    if (startsWith(line, "AUTO=")) {
        const value = line[5..];
        if (eqIgnoreCase(value, "PCI")) current.auto_pci = true;
        if (eqIgnoreCase(value, "ACPI")) current.auto_acpi = true;
        return;
    }
    if (startsWith(line, "DISABLE=")) {
        addDisabled(trim(line[8..]));
        return;
    }
    if (startsWith(line, "DRIVER=")) {
        addDriver(trim(line[7..]));
        return;
    }
    if (startsWith(line, "SHELL=")) {
        setShellPath(trim(line[6..]));
        return;
    }
    if (startsWith(line, "SHELL_ARGS=")) {
        setShellArgs(trim(line[11..]));
        return;
    }
    if (startsWith(line, "SHELLARGS=")) {
        setShellArgs(trim(line[10..]));
        return;
    }
    if (startsWith(line, "LOG=")) {
        current.log_verbose = eqIgnoreCase(trim(line[4..]), "verbose");
        return;
    }
    if (startsWith(line, "OPTION ")) {
        parseOption(trim(line[7..]));
        return;
    }
}

fn parseOption(rest: []const u8) void {
    if (current.option_count >= MAX_OPTIONS) return;
    const space = indexOf(rest, ' ') orelse return;
    const eq = indexOf(rest[space + 1 ..], '=') orelse return;
    var opt: Option = .{};
    copyName(trim(rest[0..space]), opt.driver[0..], &opt.driver_len);
    copyName(trim(rest[space + 1 .. space + 1 + eq]), opt.key[0..], &opt.key_len);
    copyName(trim(rest[space + 1 + eq + 1 ..]), opt.value[0..], &opt.value_len);
    current.options[current.option_count] = opt;
    current.option_count += 1;
}

fn addDriver(name: []const u8) void {
    if (current.driver_count >= MAX_DRIVERS) return;
    if (!validDriverName(name)) {
        bootlog.puts("[CONF][WARN] invalid DRIVER entry\r\n");
        current.driver_lens[current.driver_count] = 0;
        current.driver_count += 1;
        return;
    }
    copyName(name, current.drivers[current.driver_count][0..], &current.driver_lens[current.driver_count]);
    current.driver_count += 1;
}

fn validDriverName(name: []const u8) bool {
    if (name.len == 0 or name.len >= MAX_NAME) return false;
    var i: usize = 0;
    while (i < name.len) : (i += 1) {
        const c = name[i];
        if ((c >= 'a' and c <= 'z') or
            (c >= 'A' and c <= 'Z') or
            (c >= '0' and c <= '9') or
            c == '-' or
            c == '_')
        {
            continue;
        }
        return false;
    }
    return true;
}

fn addDisabled(name: []const u8) void {
    if (name.len == 0 or current.disabled_count >= MAX_DISABLED) return;
    copyName(name, current.disabled[current.disabled_count][0..], &current.disabled_lens[current.disabled_count]);
    current.disabled_count += 1;
}

fn logDefaults() void {
    bootlog.puts("[CONF] defaults AUTO=PCI AUTO=ACPI DRIVER=HDA DRIVER=SID DRIVER=OPL3 DRIVER=MIDI SHELL=/R4OS/SOFTWARE/DESKTOP/R4DESK.R4X LOG=normal\r\n");
    addDriver("HDA");
    addDriver("SID");
    addDriver("OPL3");
    addDriver("MIDI");
}

fn logSummary() void {
    bootlog.puts("[CONF] auto pci=");
    bootlog.puts(if (current.auto_pci) "yes" else "no");
    bootlog.puts(" acpi=");
    bootlog.puts(if (current.auto_acpi) "yes" else "no");
    bootlog.puts(" drivers=");
    bootlog.putDec(current.driver_count);
    bootlog.puts(" disabled=");
    bootlog.putDec(current.disabled_count);
    bootlog.puts(" options=");
    bootlog.putDec(current.option_count);
    bootlog.puts(" shell=");
    bootlog.puts(current.shell_path[0..current.shell_path_len]);
    if (current.shell_args_len > 0) {
        bootlog.puts(" args=");
        bootlog.puts(current.shell_args[0..current.shell_args_len]);
    }
    bootlog.puts("\r\n");
}

fn recordConfigPerf(start_tick: u64, bytes: usize) void {
    loader_perf.recordConfigLoad(start_tick, bytes, current.driver_count, current.disabled_count, current.option_count);
}

fn copyName(src: []const u8, dst: []u8, len_out: *usize) void {
    const n = if (src.len < dst.len) src.len else dst.len - 1;
    var i: usize = 0;
    while (i < n) : (i += 1) dst[i] = upper(src[i]);
    if (n < dst.len) dst[n] = 0;
    len_out.* = n;
}

fn setShellPath(path: []const u8) void {
    if (path.len == 0) return;
    copyPath(path, current.shell_path[0..], &current.shell_path_len);
}

fn setShellArgs(args: []const u8) void {
    copyRaw(args, current.shell_args[0..], &current.shell_args_len);
}

fn copyPath(src: []const u8, dst: []u8, len_out: *usize) void {
    const n = if (src.len < dst.len) src.len else dst.len - 1;
    var i: usize = 0;
    while (i < n) : (i += 1) dst[i] = upper(src[i]);
    if (n < dst.len) dst[n] = 0;
    len_out.* = n;
}

fn copyRaw(src: []const u8, dst: []u8, len_out: *usize) void {
    const n = if (src.len < dst.len) src.len else dst.len - 1;
    if (n > 0) @memcpy(dst[0..n], src[0..n]);
    if (n < dst.len) dst[n] = 0;
    len_out.* = n;
}

fn trim(s: []const u8) []const u8 {
    var a: usize = 0;
    var b: usize = s.len;
    while (a < b and (s[a] == ' ' or s[a] == '\t')) : (a += 1) {}
    while (b > a and (s[b - 1] == ' ' or s[b - 1] == '\t')) : (b -= 1) {}
    return s[a..b];
}

fn startsWith(s: []const u8, prefix: []const u8) bool {
    return s.len >= prefix.len and eqIgnoreCase(s[0..prefix.len], prefix);
}

fn eqIgnoreCase(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var i: usize = 0;
    while (i < a.len) : (i += 1) {
        if (upper(a[i]) != upper(b[i])) return false;
    }
    return true;
}

fn upper(c: u8) u8 {
    if (c >= 'a' and c <= 'z') return c - ('a' - 'A');
    return c;
}

fn indexOf(s: []const u8, ch: u8) ?usize {
    var i: usize = 0;
    while (i < s.len) : (i += 1) if (s[i] == ch) return i;
    return null;
}
