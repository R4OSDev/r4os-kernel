// Sole state and all configuration decisions belong to Rust.
const Text = @import("../driver/usb/core.zig").Text;
const Addresses = @import("config_writer.zig").Addresses;
extern fn r4os_rust_net_config_reset() void;
extern fn r4os_rust_net_config_settings(*NativeSettings) void;
extern fn r4os_rust_net_config_restore(*const NativeSettings) void;
extern fn r4os_rust_net_config_boot(*const boot_config.Config) void;
extern fn r4os_rust_net_config_addresses(*const Addresses, bool) void;
extern fn r4os_rust_net_config_clear() void;
extern fn r4os_rust_net_config_enable() void;
extern fn r4os_rust_net_config_clear_preserving(*const Text) void;
extern fn r4os_rust_net_config_source(*Text) void;
extern fn r4os_rust_net_config_parse([*]const u8, usize, *[4]u8) bool;
export fn r4os_zig_net_boot_option(config: *const boot_config.Config, key: [*]const u8, len: usize, out: *Text) callconv(.c) bool {
    const value = boot_config.optionValue(config, "NET", key[0..len]) orelse return false;
    out.* = .{ .pointer = value.ptr, .len = value.len };
    return true;
}
const boot_config = @import("../kernel/boot_config.zig");

pub const DEFAULT_LOCAL_IP: [4]u8 = .{ 10, 0, 2, 15 };
pub const DEFAULT_NETMASK: [4]u8 = .{ 255, 255, 255, 0 };
pub const DEFAULT_GATEWAY_IP: [4]u8 = .{ 10, 0, 2, 2 };
pub const DEFAULT_DNS_IP: [4]u8 = .{ 10, 0, 2, 3 };

pub const Settings = struct {
    local_ip: [4]u8 = DEFAULT_LOCAL_IP,
    netmask: [4]u8 = DEFAULT_NETMASK,
    gateway_ip: [4]u8 = DEFAULT_GATEWAY_IP,
    dns_ip: [4]u8 = DEFAULT_DNS_IP,
    dns_configured: bool = false,
    dhcp_enabled: bool = false,
    dhcp_bound: bool = false,
    configured: bool = false,
    invalid_options: u64 = 0,
    last_error: []const u8 = "default",
};

pub fn reset() void {
    r4os_rust_net_config_reset();
}

pub fn applyBootConfig(config: *const boot_config.Config) void {
    r4os_rust_net_config_boot(config);
}

pub fn settings() Settings {
    var value: NativeSettings = undefined;
    r4os_rust_net_config_settings(&value);
    return .{ .local_ip = value.local_ip, .netmask = value.netmask, .gateway_ip = value.gateway_ip, .dns_ip = value.dns_ip, .dns_configured = value.dns_configured, .dhcp_enabled = value.dhcp_enabled, .dhcp_bound = value.dhcp_bound, .configured = value.configured, .invalid_options = value.invalid_options, .last_error = value.last_error.slice() };
}

pub fn restore(saved: Settings) void {
    const value = NativeSettings{ .local_ip = saved.local_ip, .netmask = saved.netmask, .gateway_ip = saved.gateway_ip, .dns_ip = saved.dns_ip, .dns_configured = saved.dns_configured, .dhcp_enabled = saved.dhcp_enabled, .dhcp_bound = saved.dhcp_bound, .configured = saved.configured, .invalid_options = saved.invalid_options, .last_error = .{ .pointer = saved.last_error.ptr, .len = saved.last_error.len } };
    r4os_rust_net_config_restore(&value);
}

pub fn applyRuntime(local_ip: [4]u8, next_netmask: [4]u8, gateway_ip: [4]u8, dns_ip: [4]u8, next_dns_configured: bool) void {
    const addresses = Addresses{ .local_ip = local_ip, .netmask = next_netmask, .gateway_ip = gateway_ip, .dns_ip = dns_ip, .dns_configured = next_dns_configured };
    r4os_rust_net_config_addresses(&addresses, false);
}

pub fn applyDhcpLease(local_ip: [4]u8, next_netmask: [4]u8, gateway_ip: [4]u8, dns_ip: [4]u8, next_dns_configured: bool) void {
    const addresses = Addresses{ .local_ip = local_ip, .netmask = next_netmask, .gateway_ip = gateway_ip, .dns_ip = dns_ip, .dns_configured = next_dns_configured };
    r4os_rust_net_config_addresses(&addresses, true);
}

pub fn clearDhcpLease() void {
    r4os_rust_net_config_clear();
}

pub fn clearDhcpLeasePreservingMode(reason: []const u8) void {
    const text = Text{ .pointer = reason.ptr, .len = reason.len };
    r4os_rust_net_config_clear_preserving(&text);
}

pub fn enableDhcp() void {
    r4os_rust_net_config_enable();
}

pub fn localIp() [4]u8 {
    return settings().local_ip;
}

pub fn netmask() [4]u8 {
    return settings().netmask;
}

pub fn gatewayIp() [4]u8 {
    return settings().gateway_ip;
}

pub fn dnsIp() [4]u8 {
    return settings().dns_ip;
}

pub fn dnsConfigured() bool {
    return settings().dns_configured;
}

pub fn dhcpEnabled() bool {
    return settings().dhcp_enabled;
}

pub fn dhcpBound() bool {
    return settings().dhcp_bound;
}

pub fn sourceName() []const u8 {
    var value: Text = undefined;
    r4os_rust_net_config_source(&value);
    return value.slice();
}

pub fn invalidOptions() u64 {
    return settings().invalid_options;
}

pub fn parseIpv4(value: []const u8) ?[4]u8 {
    var out: [4]u8 = undefined;
    return if (r4os_rust_net_config_parse(value.ptr, value.len, &out)) out else null;
}

const NativeSettings = extern struct {
    local_ip: [4]u8 = DEFAULT_LOCAL_IP,
    netmask: [4]u8 = DEFAULT_NETMASK,
    gateway_ip: [4]u8 = DEFAULT_GATEWAY_IP,
    dns_ip: [4]u8 = DEFAULT_DNS_IP,
    dns_configured: bool = false,
    dhcp_enabled: bool = false,
    dhcp_bound: bool = false,
    configured: bool = false,
    invalid_options: u64 = 0,
    last_error: Text = .{ .pointer = "default".ptr, .len = 7 },
};

comptime {
    if (@sizeOf(NativeSettings) != 48 or @alignOf(NativeSettings) != 8 or @offsetOf(NativeSettings, "local_ip") != 0 or @offsetOf(NativeSettings, "netmask") != 4 or @offsetOf(NativeSettings, "gateway_ip") != 8 or @offsetOf(NativeSettings, "dns_ip") != 12 or @offsetOf(NativeSettings, "dns_configured") != 16 or @offsetOf(NativeSettings, "dhcp_enabled") != 17 or @offsetOf(NativeSettings, "dhcp_bound") != 18 or @offsetOf(NativeSettings, "configured") != 19 or @offsetOf(NativeSettings, "invalid_options") != 24 or @offsetOf(NativeSettings, "last_error") != 32) @compileError("Native network NativeSettings layout");
}
