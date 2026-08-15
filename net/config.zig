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

var current: Settings = .{};

pub fn reset() void {
    current = .{};
}

pub fn applyBootConfig(config: *const boot_config.Config) void {
    reset();

    const ip_keys = [_][]const u8{ "ip", "addr", "address" };
    const mask_keys = [_][]const u8{ "mask", "netmask" };
    const gateway_keys = [_][]const u8{ "gateway", "gw" };
    const dns_keys = [_][]const u8{ "dns", "nameserver" };

    if (boot_config.optionValue(config, "NET", "dhcp")) |value| {
        if (parseBool(value)) |enabled| {
            current.dhcp_enabled = enabled;
            if (enabled) current.configured = true;
        } else {
            current.invalid_options += 1;
            current.last_error = "bad-dhcp";
        }
    }

    _ = applyFirstOption(config, ip_keys[0..], &current.local_ip);
    _ = applyFirstOption(config, mask_keys[0..], &current.netmask);
    _ = applyFirstOption(config, gateway_keys[0..], &current.gateway_ip);
    if (applyFirstOption(config, dns_keys[0..], &current.dns_ip)) {
        current.dns_configured = true;
    }

    if (current.invalid_options == 0) {
        current.last_error = if (current.configured) "config" else "default";
    }

    // DHCP is a desired configuration mode, not a promise that a lease is
    // already present. Never expose the baked-in QEMU defaults while waiting
    // for a late R4D link.
    if (current.dhcp_enabled) clearDhcpLeasePreservingMode("dhcp-pending");
}

pub fn settings() Settings {
    return current;
}

pub fn restore(saved: Settings) void {
    current = saved;
}

pub fn applyRuntime(local_ip: [4]u8, next_netmask: [4]u8, gateway_ip: [4]u8, dns_ip: [4]u8, next_dns_configured: bool) void {
    current.local_ip = local_ip;
    current.netmask = next_netmask;
    current.gateway_ip = gateway_ip;
    current.dns_ip = dns_ip;
    current.dns_configured = next_dns_configured;
    current.dhcp_enabled = false;
    current.dhcp_bound = false;
    current.configured = true;
    current.invalid_options = 0;
    current.last_error = "runtime";
}

pub fn applyDhcpLease(local_ip: [4]u8, next_netmask: [4]u8, gateway_ip: [4]u8, dns_ip: [4]u8, next_dns_configured: bool) void {
    current.local_ip = local_ip;
    current.netmask = next_netmask;
    current.gateway_ip = gateway_ip;
    current.dns_ip = dns_ip;
    current.dns_configured = next_dns_configured;
    current.dhcp_enabled = true;
    current.dhcp_bound = true;
    current.configured = true;
    current.invalid_options = 0;
    current.last_error = "dhcp";
}

pub fn clearDhcpLease() void {
    current.dhcp_enabled = false;
    clearDhcpLeasePreservingMode("dhcp-released");
}

pub fn clearDhcpLeasePreservingMode(reason: []const u8) void {
    current.local_ip = .{0} ** 4;
    current.netmask = DEFAULT_NETMASK;
    current.gateway_ip = .{0} ** 4;
    current.dns_ip = .{0} ** 4;
    current.dns_configured = false;
    current.dhcp_bound = false;
    current.configured = true;
    current.invalid_options = 0;
    current.last_error = reason;
}

pub fn enableDhcp() void {
    current.dhcp_enabled = true;
    current.configured = true;
    if (!current.dhcp_bound) clearDhcpLeasePreservingMode("dhcp-pending");
}

pub fn localIp() [4]u8 {
    return current.local_ip;
}

pub fn netmask() [4]u8 {
    return current.netmask;
}

pub fn gatewayIp() [4]u8 {
    return current.gateway_ip;
}

pub fn dnsIp() [4]u8 {
    return current.dns_ip;
}

pub fn dnsConfigured() bool {
    return current.dns_configured;
}

pub fn dhcpEnabled() bool {
    return current.dhcp_enabled;
}

pub fn dhcpBound() bool {
    return current.dhcp_bound;
}

pub fn sourceName() []const u8 {
    if (current.invalid_options > 0) return current.last_error;
    if (current.dhcp_enabled and current.dhcp_bound) return "dhcp";
    if (current.dhcp_enabled) return "dhcp-pending";
    if (eqIgnoreCase(current.last_error, "dhcp-released")) return "released";
    return if (current.configured) "config" else "default";
}

pub fn invalidOptions() u64 {
    return current.invalid_options;
}

pub fn parseIpv4(value: []const u8) ?[4]u8 {
    var out: [4]u8 = .{0} ** 4;
    var part: usize = 0;
    var accum: u16 = 0;
    var digits: usize = 0;

    var index: usize = 0;
    while (index < value.len) : (index += 1) {
        const ch = value[index];
        if (ch >= '0' and ch <= '9') {
            accum = accum * 10 + @as(u16, ch - '0');
            if (accum > 255) return null;
            digits += 1;
            if (digits > 3) return null;
        } else if (ch == '.') {
            if (digits == 0 or part >= 3) return null;
            out[part] = @intCast(accum);
            part += 1;
            accum = 0;
            digits = 0;
        } else {
            return null;
        }
    }

    if (digits == 0 or part != 3) return null;
    out[part] = @intCast(accum);
    return out;
}

fn applyFirstOption(config: *const boot_config.Config, keys: []const []const u8, target: *[4]u8) bool {
    var index: usize = 0;
    while (index < keys.len) : (index += 1) {
        if (boot_config.optionValue(config, "NET", keys[index])) |value| {
            if (parseIpv4(value)) |ip| {
                target.* = ip;
                current.configured = true;
                return true;
            }
            current.invalid_options += 1;
            current.last_error = "bad-ipv4";
            return false;
        }
    }
    return false;
}

fn parseBool(value: []const u8) ?bool {
    if (eqIgnoreCase(value, "on") or eqIgnoreCase(value, "yes") or eqIgnoreCase(value, "true") or eqIgnoreCase(value, "1")) return true;
    if (eqIgnoreCase(value, "off") or eqIgnoreCase(value, "no") or eqIgnoreCase(value, "false") or eqIgnoreCase(value, "0")) return false;
    return null;
}

fn eqIgnoreCase(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var index: usize = 0;
    while (index < a.len) : (index += 1) {
        var ca = a[index];
        var cb = b[index];
        if (ca >= 'A' and ca <= 'Z') ca += 32;
        if (cb >= 'A' and cb <= 'Z') cb += 32;
        if (ca != cb) return false;
    }
    return true;
}
