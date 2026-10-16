pub const MAX_DRIVERS: usize = 12;
pub const MAX_DISABLED: usize = 8;
pub const MAX_OPTIONS: usize = 8;
pub const MAX_SHELL_PATH: usize = 96;
pub const MAX_SHELL_ARGS: usize = 96;
const MAX_NAME: usize = 24;
const MAX_KEY: usize = 24;
const MAX_VALUE: usize = 48;

pub const Option = extern struct {
    driver: [MAX_NAME]u8 = .{0} ** MAX_NAME,
    driver_len: usize = 0,
    key: [MAX_KEY]u8 = .{0} ** MAX_KEY,
    key_len: usize = 0,
    value: [MAX_VALUE]u8 = .{0} ** MAX_VALUE,
    value_len: usize = 0,
};

pub const Config = extern struct {
    graphics: @import("../display/backend_state.zig").Policy = .automatic,
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

const Span = extern struct {
    data: ?[*]const u8 = null,
    len: usize = 0,
    fn from(s: []const u8) @This() {
        return .{ .data = s.ptr, .len = s.len };
    }
    fn bytes(self: @This()) []const u8 {
        return if (self.len == 0) "" else self.data.?[0..self.len];
    }
};
extern fn r4os_rust_boot_config(op: u32, config: ?*const Config, name: Span, key: Span, index: usize, out: ?*Span) u64;
pub fn load() *const Config {
    return @ptrFromInt(r4os_rust_boot_config(0, null, .{}, .{}, 0, null));
}
pub fn get() *const Config {
    return @ptrFromInt(r4os_rust_boot_config(1, null, .{}, .{}, 0, null));
}
pub fn driverName(index: usize) []const u8 {
    var out: Span = .{};
    _ = r4os_rust_boot_config(2, null, .{}, .{}, index, &out);
    return out.bytes();
}
pub fn disabledName(index: usize) []const u8 {
    var out: Span = .{};
    _ = r4os_rust_boot_config(3, null, .{}, .{}, index, &out);
    return out.bytes();
}
pub fn isDisabled(config: *const Config, name: []const u8) bool {
    return r4os_rust_boot_config(4, config, Span.from(name), .{}, 0, null) != 0;
}
pub fn hasDriver(config: *const Config, name: []const u8) bool {
    return r4os_rust_boot_config(5, config, Span.from(name), .{}, 0, null) != 0;
}
pub fn optionValue(config: *const Config, driver: []const u8, key: []const u8) ?[]const u8 {
    var out: Span = .{};
    return if (r4os_rust_boot_config(6, config, Span.from(driver), Span.from(key), 0, &out) != 0) out.bytes() else null;
}
pub fn shellPath(config: *const Config) []const u8 {
    var out: Span = .{};
    _ = r4os_rust_boot_config(7, config, .{}, .{}, 0, &out);
    return out.bytes();
}
pub fn shellArgs(config: *const Config) []const u8 {
    var out: Span = .{};
    _ = r4os_rust_boot_config(8, config, .{}, .{}, 0, &out);
    return out.bytes();
}

comptime {
    if (@sizeOf(Config) != 1840 or @sizeOf(Option) != 120 or @offsetOf(Config, "options") != 664 or @offsetOf(Config, "shell_path") != 1632) @compileError("private boot config layout");
}
