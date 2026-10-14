// Stateless Config/display/fatal/presentation cuts to still active owners.
const policy = @import("driver_policy_boot.zig");
const boot_config = @import("boot_config.zig");
const bootlog = @import("bootlog.zig");
const k = @import("log.zig");
const loader = @import("loader_boot.zig");
const irq_boot = @import("platform_irq_boot.zig");
const display = @import("../display/display.zig");
const graphics = @import("graphics_boot_policy.zig");
const boot_info = @import("../bootloader/boot_info.zig");
const registry = @import("../driver/registry.zig");
const bootscreen = @import("bootscreen.zig");
const fatal = @import("fatal.zig");
export fn r4os_zig_driver_policy_query(operation: u8) callconv(.c) u64 {
    return switch (operation) {
        0 => @intFromBool(loader.isInitialized()),
        1 => if (loader.config()) |config| @intFromPtr(config) else 0,
        2 => @intFromBool(irq_boot.isInitialized()),
        3 => @intFromBool(@import("config").display_reject_test),
        else => unreachable,
    };
}
export fn r4os_zig_driver_policy_graphics(opaque_config: *const anyopaque) callconv(.c) bool {
    const config: *const boot_config.Config = @ptrCast(@alignCast(opaque_config));
    const selected = graphics.effective(config.graphics, boot_info.get().software_graphics_once);
    if (!display.setBackendPolicy(selected)) return false;
    bootlog.puts("[GFX] policy=");
    bootlog.puts(@tagName(selected));
    bootlog.puts("\r\n");
    return true;
}
export fn r4os_zig_driver_policy_rejection_probe() callconv(.c) bool {
    if (!display.probeRejectedTakeover()) return false;
    k.puts("[GFX] rejected-takeover=OK old-owner=retained firmware-writers=open\r\n");
    return true;
}
export fn r4os_zig_driver_policy_effect(operation: u8, pointer: [*]const u8, len: usize) callconv(.c) bool {
    switch (operation) {
        0 => {
            const current = policy.status();
            bootlog.puts("[DRVPOLICY] init skipped phase=");
            bootlog.puts(policy.phaseName(current.phase));
            bootlog.puts(" boot_continue=");
            bootlog.puts(yesNo(!current.fatal));
            bootlog.puts("\r\n");
        },
        1 => dumpStatusToBootlog(),
        2 => emitBootStatusLine(),
        3 => {
            bootscreen.setStatus("Treiber abschliessen");
            registry.logRegistryToBootlog();
        },
        4 => return fatal.fail(.driver_policy, pointer[0..len]),
        else => unreachable,
    }
    return true;
}
fn dumpStatusToBootlog() void {
    const current = policy.status();
    const summary = current.summary;
    bootlog.puts("[DRVPOLICY] status phase=");
    bootlog.puts(policy.phaseName(current.phase));
    bootlog.puts(" initialized=");
    bootlog.puts(yesNo(current.initialized));
    bootlog.puts(" fatal=");
    bootlog.puts(yesNo(current.fatal));
    bootlog.puts(" config=");
    bootlog.puts(yesNo(current.config_visible));
    bootlog.puts(" platform_irq=");
    bootlog.puts(yesNo(current.platform_irq_ready));
    bootlog.puts(" directives=");
    bootlog.puts(yesNo(current.core_directives_applied));
    bootlog.puts(" plan=");
    bootlog.puts(yesNo(current.plan_built));
    bootlog.puts(" r4d=");
    bootlog.puts(yesNo(current.r4d_load_done));
    bootlog.puts(" registry_dump=");
    bootlog.puts(yesNo(current.registry_dumped));
    bootlog.puts(" boot_continue=");
    bootlog.puts(yesNo(!current.fatal));
    bootlog.puts(" config_drivers=");
    bootlog.putDec(summary.config_driver_count);
    bootlog.puts(" selected=");
    bootlog.putDec(summary.selected);
    bootlog.puts(" skipped=");
    bootlog.putDec(summary.skipped);
    bootlog.puts(" disabled=");
    bootlog.putDec(summary.disabled);
    bootlog.puts(" duplicates=");
    bootlog.putDec(summary.duplicates);
    bootlog.puts(" capacity=");
    bootlog.putDec(summary.capacity_limited);
    bootlog.puts(" already=");
    bootlog.putDec(summary.already_active);
    bootlog.puts(" loaded=");
    bootlog.putDec(summary.loaded);
    bootlog.puts(" missing=");
    bootlog.putDec(summary.file_missing);
    bootlog.puts(" invalid=");
    bootlog.putDec(summary.invalid_file);
    bootlog.puts(" name_invalid=");
    bootlog.putDec(summary.name_invalid);
    bootlog.puts(" init_failed=");
    bootlog.putDec(summary.init_failed);
    bootlog.puts(" load_failed=");
    bootlog.putDec(summary.load_failed);
    bootlog.puts(" warnings=");
    bootlog.putDec(summary.warnings);
    bootlog.puts(" registry_before=");
    bootlog.putDec(summary.registry_before);
    bootlog.puts(" registry_after=");
    bootlog.putDec(summary.registry_after);
    bootlog.puts(" ps2_disabled=");
    bootlog.puts(yesNo(summary.ps2_disabled));
    bootlog.puts(" atapio_ignored=");
    bootlog.puts(yesNo(summary.atapio_disable_ignored));
    bootlog.puts(" reason=");
    bootlog.puts(current.reason);
    bootlog.puts("\r\n");
}

fn emitBootStatusLine() void {
    const current = policy.status();
    const summary = current.summary;
    k.puts("  Driver policy ");
    if (current.fatal) {
        k.puts("[fatal]\r\n");
    } else if (current.summary.warnings != 0) {
        k.puts("[warn]\r\n");
    } else {
        k.puts("[OK]\r\n");
    }
    k.puts("  Driver policy summary config=");
    k.putDec(summary.config_driver_count);
    k.puts(" selected=");
    k.putDec(summary.selected);
    k.puts(" skipped=");
    k.putDec(summary.skipped);
    k.puts(" disabled=");
    k.putDec(summary.disabled);
    k.puts(" duplicates=");
    k.putDec(summary.duplicates);
    k.puts(" capacity=");
    k.putDec(summary.capacity_limited);
    k.puts(" already=");
    k.putDec(summary.already_active);
    k.puts(" loaded=");
    k.putDec(summary.loaded);
    k.puts(" missing=");
    k.putDec(summary.file_missing);
    k.puts(" invalid=");
    k.putDec(summary.invalid_file);
    k.puts(" name_invalid=");
    k.putDec(summary.name_invalid);
    k.puts(" init_failed=");
    k.putDec(summary.init_failed);
    k.puts(" load_failed=");
    k.putDec(summary.load_failed);
    k.puts(" warnings=");
    k.putDec(summary.warnings);
    k.puts(" registry_before=");
    k.putDec(summary.registry_before);
    k.puts(" registry_after=");
    k.putDec(summary.registry_after);
    k.puts(" ps2_disabled=");
    k.puts(yesNo(summary.ps2_disabled));
    k.puts(" atapio_ignored=");
    k.puts(yesNo(summary.atapio_disable_ignored));
    k.puts(" continue=");
    k.puts(yesNo(!current.fatal));
    k.puts("\r\n");
}

fn yesNo(value: bool) []const u8 {
    return if (value) "yes" else "no";
}
