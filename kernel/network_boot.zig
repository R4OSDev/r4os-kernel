// Early network/IPC boot interface.

const k = @import("log.zig");
const boot_status = @import("boot_status.zig");
const bootlog = @import("bootlog.zig");
const fatal = @import("fatal.zig");
const loader_boot = @import("loader_boot.zig");
const boot_config = @import("boot_config.zig");
const ipc = @import("ipc.zig");
const net_config = @import("../net/config.zig");
const net_core = @import("../net/core.zig");
const net_ipc_services = @import("../net/ipc_services.zig");
const serial_link = @import("../net/serial_link.zig");

pub const Phase = enum {
    not_initialized,
    contract_ready,
    core_ready,
    boot_config_applied,
    serial_link_ready,
    ipc_ready,
    net_ipc_services_ready,
    failed,
};

pub const Status = struct {
    phase: Phase = .not_initialized,
    contract_ready: bool = false,
    core_ready: bool = false,
    boot_config_applied: bool = false,
    serial_link_ready: bool = false,
    ipc_ready: bool = false,
    net_ipc_services_ready: bool = false,
    adapter_count: usize = 0,
    serial_link_present: bool = false,
    config_invalid_options: u64 = 0,
    config_source: []const u8 = "none",
    failed: bool = false,
    reason: []const u8 = "not initialized",
};

var current: Status = .{};

pub fn prepareContract() bool {
    if (current.failed) return false;
    if (current.contract_ready) return true;

    markContract("contract ready; waiting for productive init");
    bootlog.puts("[NETBOOT] contract [OK]\r\n");
    boot_status.statusLine("  Network boot [contract]\r\n");
    return true;
}

pub fn init() bool {
    if (current.failed) return false;
    if (isInitialized()) return true;

    if (!loader_boot.isInitialized()) {
        return fail("Network boot before loader config", "loader config missing");
    }

    const loaded_config = loader_boot.config() orelse {
        return fail("Network boot config missing", "loader config missing");
    };

    return initWithConfig(loaded_config);
}

pub fn isContractReady() bool {
    return current.contract_ready and !current.failed;
}

pub fn isInitialized() bool {
    return current.net_ipc_services_ready and !current.failed;
}

pub fn status() Status {
    return current;
}

pub fn phaseName(phase: Phase) []const u8 {
    return switch (phase) {
        .not_initialized => "not-initialized",
        .contract_ready => "contract-ready",
        .core_ready => "core-ready",
        .boot_config_applied => "boot-config-applied",
        .serial_link_ready => "serial-link-ready",
        .ipc_ready => "ipc-ready",
        .net_ipc_services_ready => "net-ipc-services-ready",
        .failed => "failed",
    };
}

fn initWithConfig(loaded_config: *const boot_config.Config) bool {
    if (current.failed) return false;
    if (isInitialized()) return true;
    if (!current.contract_ready) markContract("contract ready");

    net_core.init();
    current.core_ready = true;
    current.adapter_count = net_core.count();
    current.phase = .core_ready;
    bootlog.puts("[NETBOOT] core [OK] adapters=");
    bootlog.putDec(current.adapter_count);
    bootlog.puts("\r\n");

    net_config.applyBootConfig(loaded_config);
    net_core.configureDhcpTestInjection(loaded_config);
    current.boot_config_applied = true;
    current.config_invalid_options = net_config.invalidOptions();
    current.config_source = net_config.sourceName();
    current.phase = .boot_config_applied;
    bootlog.puts("[NETBOOT] boot-config [OK] source=");
    bootlog.puts(current.config_source);
    bootlog.puts(" invalid=");
    bootlog.putDec(current.config_invalid_options);
    bootlog.puts("\r\n");

    serial_link.init();
    current.serial_link_ready = true;
    current.serial_link_present = serialLinkPresent();
    current.phase = .serial_link_ready;
    bootlog.puts("[NETBOOT] serial-link [OK] present=");
    bootlog.puts(yesNo(current.serial_link_present));
    bootlog.puts("\r\n");

    ipc.init();
    current.ipc_ready = true;
    current.phase = .ipc_ready;
    bootlog.puts("[NETBOOT] ipc [OK]\r\n");

    net_ipc_services.init();
    current.net_ipc_services_ready = true;
    current.phase = .net_ipc_services_ready;
    current.reason = readyReason();
    bootlog.puts("[NETBOOT] net-ipc-services [OK]\r\n");
    dumpStatusToBootlog();
    emitBootStatusLine();
    boot_status.statusLine("  Network boot [OK]\r\n");
    return true;
}

pub fn dumpStatus() void {
    dumpStatusToLog();
}

fn markContract(reason: []const u8) void {
    current = .{
        .phase = .contract_ready,
        .contract_ready = true,
        .reason = reason,
    };
}

fn serialLinkPresent() bool {
    var snapshot: serial_link.Snapshot = .{};
    serial_link.snapshot(&snapshot);
    return snapshot.present;
}

fn readyReason() []const u8 {
    if (current.config_invalid_options != 0) return "network/ipc ready with invalid net config options";
    if (current.adapter_count == 0) return "network/ipc ready without adapter";
    if (!current.serial_link_present) return "network/ipc ready without serial link";
    return "network/ipc ready";
}

fn fail(message: []const u8, reason: []const u8) bool {
    current.phase = .failed;
    current.failed = true;
    current.reason = reason;
    bootlog.puts("[NETBOOT][CRASH] reason=");
    bootlog.puts(reason);
    bootlog.puts("\r\n");
    return fatal.fail(.network, message);
}

fn dumpStatusToBootlog() void {
    bootlog.puts("[NETBOOT] status phase=");
    bootlog.puts(phaseName(current.phase));
    bootlog.puts(" contract=");
    bootlog.puts(yesNo(current.contract_ready));
    bootlog.puts(" core=");
    bootlog.puts(yesNo(current.core_ready));
    bootlog.puts(" config=");
    bootlog.puts(yesNo(current.boot_config_applied));
    bootlog.puts(" serial=");
    bootlog.puts(yesNo(current.serial_link_ready));
    bootlog.puts(" serial_present=");
    bootlog.puts(yesNo(current.serial_link_present));
    bootlog.puts(" ipc=");
    bootlog.puts(yesNo(current.ipc_ready));
    bootlog.puts(" services=");
    bootlog.puts(yesNo(current.net_ipc_services_ready));
    bootlog.puts(" adapters=");
    bootlog.putDec(current.adapter_count);
    bootlog.puts(" config_source=");
    bootlog.puts(current.config_source);
    bootlog.puts(" invalid=");
    bootlog.putDec(current.config_invalid_options);
    bootlog.puts(" reason=");
    bootlog.puts(current.reason);
    bootlog.puts("\r\n");
}

fn dumpStatusToLog() void {
    k.puts("Network boot\r\n");
    k.puts("  phase=");
    k.puts(phaseName(current.phase));
    k.puts(" initialized=");
    k.puts(yesNo(isInitialized()));
    k.puts(" failed=");
    k.puts(yesNo(current.failed));
    k.puts("\r\n");
    k.puts("  core=");
    k.puts(yesNo(current.core_ready));
    k.puts(" config=");
    k.puts(yesNo(current.boot_config_applied));
    k.puts(" serial=");
    k.puts(yesNo(current.serial_link_ready));
    k.puts(" ipc=");
    k.puts(yesNo(current.ipc_ready));
    k.puts(" services=");
    k.puts(yesNo(current.net_ipc_services_ready));
    k.puts("\r\n");
    k.puts("  adapters=");
    k.putDec(current.adapter_count);
    k.puts(" serial_present=");
    k.puts(yesNo(current.serial_link_present));
    k.puts(" config_source=");
    k.puts(current.config_source);
    k.puts(" invalid=");
    k.putDec(current.config_invalid_options);
    k.puts("\r\n");
    k.puts("  reason=");
    k.puts(current.reason);
    k.puts("\r\n");
}

fn emitBootStatusLine() void {
    k.puts("  NETBOOT cfg=");
    k.puts(current.config_source);
    k.puts(" adapters=");
    k.putDec(current.adapter_count);
    k.puts(" com2=");
    k.puts(yesNo(current.serial_link_present));
    k.puts(" invalid=");
    k.putDec(current.config_invalid_options);
    k.puts(" [OK]\r\n");
    // 0.56.20: Speicher-Nachweis der TCP-Connection-Tabelle (Befund 13.1.4).
    const tcp = @import("../net/tcp.zig");
    k.puts("  NETBOOT tcp-table conns=");
    k.putDec(tcp.MAX_CONNECTIONS);
    k.puts(" slot_kb=");
    k.putDec(tcp.connectionSlotBytes() / 1024);
    k.puts(" total_kb=");
    k.putDec(tcp.MAX_CONNECTIONS * tcp.connectionSlotBytes() / 1024);
    k.puts("\r\n");
}

fn yesNo(value: bool) []const u8 {
    return if (value) "yes" else "no";
}
