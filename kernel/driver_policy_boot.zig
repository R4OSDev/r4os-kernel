// Driver-Policy-/R4D-Bootschnitt.

const boot_config = @import("boot_config.zig");
const bootlog = @import("bootlog.zig");
const driver_plan = @import("driver_plan.zig");
const driver_registry = @import("../driver/registry.zig");
const fatal = @import("fatal.zig");
const k = @import("log.zig");
const loader_boot = @import("loader_boot.zig");
const platform_irq_boot = @import("platform_irq_boot.zig");

pub const Phase = enum {
    not_started,
    config_missing,
    platform_irq_missing,
    core_directives_applied,
    plan_built,
    r4d_load_done,
    warning,
    failed,
};

pub const Status = struct {
    phase: Phase = .not_started,
    attempted: bool = false,
    initialized: bool = false,
    fatal: bool = false,
    config_visible: bool = false,
    platform_irq_ready: bool = false,
    core_directives_applied: bool = false,
    plan_built: bool = false,
    r4d_load_done: bool = false,
    registry_dumped: bool = false,
    summary: driver_plan.Summary = .{},
    reason: []const u8 = "not started",
};

var current: Status = .{};

pub fn init() bool {
    if (current.attempted) {
        bootlog.puts("[DRVPOLICY] init skipped phase=");
        bootlog.puts(phaseName(current.phase));
        bootlog.puts(" boot_continue=");
        bootlog.puts(yesNo(canContinueBoot()));
        bootlog.puts("\r\n");
        return canContinueBoot();
    }

    current = .{
        .attempted = true,
        .reason = "starting",
    };

    if (!loader_boot.isInitialized()) {
        return failFatal("Driver policy before loader config", "loader boot missing");
    }

    const loaded_config = loader_boot.config() orelse {
        return failFatal("Driver policy config missing", "loader config missing");
    };
    current.config_visible = true;

    if (!platform_irq_boot.isInitialized()) {
        return failFatal("Driver policy before platform IRQ boot", "platform irq missing");
    }
    current.platform_irq_ready = true;

    runPolicy(loaded_config);
    return true;
}

pub fn isInitialized() bool {
    return current.initialized and !current.fatal;
}

pub fn status() Status {
    return current;
}

pub fn phaseName(phase: Phase) []const u8 {
    return switch (phase) {
        .not_started => "not-started",
        .config_missing => "config-missing",
        .platform_irq_missing => "platform-irq-missing",
        .core_directives_applied => "core-directives-applied",
        .plan_built => "plan-built",
        .r4d_load_done => "r4d-load-done",
        .warning => "warning",
        .failed => "failed",
    };
}

pub fn dumpStatus() void {
    const summary = current.summary;
    k.puts("Driver policy boot\r\n");
    k.puts("  phase=");
    k.puts(phaseName(current.phase));
    k.puts(" initialized=");
    k.puts(yesNo(current.initialized));
    k.puts(" fatal=");
    k.puts(yesNo(current.fatal));
    k.puts(" continue=");
    k.puts(yesNo(canContinueBoot()));
    k.puts("\r\n");
    k.puts("  selected=");
    k.putDec(summary.selected);
    k.puts(" skipped=");
    k.putDec(summary.skipped);
    k.puts(" disabled=");
    k.putDec(summary.disabled);
    k.puts(" duplicates=");
    k.putDec(summary.duplicates);
    k.puts(" capacity=");
    k.putDec(summary.capacity_limited);
    k.puts("\r\n");
    k.puts("  loaded=");
    k.putDec(summary.loaded);
    k.puts(" already=");
    k.putDec(summary.already_active);
    k.puts(" missing=");
    k.putDec(summary.file_missing);
    k.puts(" invalid=");
    k.putDec(summary.invalid_file);
    k.puts(" init_failed=");
    k.putDec(summary.init_failed);
    k.puts(" load_failed=");
    k.putDec(summary.load_failed);
    k.puts(" warnings=");
    k.putDec(summary.warnings);
    k.puts("\r\n");
    k.puts("  registry_before=");
    k.putDec(summary.registry_before);
    k.puts(" registry_after=");
    k.putDec(summary.registry_after);
    k.puts(" ps2_disabled=");
    k.puts(yesNo(summary.ps2_disabled));
    k.puts(" atapio_ignored=");
    k.puts(yesNo(summary.atapio_disable_ignored));
    k.puts(" reason=");
    k.puts(current.reason);
    k.puts("\r\n");
}

fn runPolicy(loaded_config: *const boot_config.Config) void {
    driver_plan.beginPolicyRun(loaded_config);
    driver_plan.applyCoreDirectives(loaded_config);
    current.core_directives_applied = true;
    current.phase = .core_directives_applied;

    driver_plan.buildAndLoad(loaded_config);
    current.plan_built = true;
    current.r4d_load_done = true;
    current.summary = driver_plan.lastSummary();
    current.phase = if (current.summary.warnings == 0) .r4d_load_done else .warning;
    current.initialized = true;
    current.reason = if (current.summary.warnings == 0)
        "driver policy applied"
    else
        "driver policy applied with warnings";

    driver_registry.logRegistryToBootlog();
    current.registry_dumped = true;
    current.summary = driver_plan.lastSummary();
    dumpStatusToBootlog();
    emitBootStatusLine();
}

fn failFatal(message: []const u8, reason: []const u8) bool {
    current.reason = reason;
    current.fatal = true;
    current.initialized = false;
    current.phase = if (!current.config_visible)
        .config_missing
    else if (!current.platform_irq_ready)
        .platform_irq_missing
    else
        .failed;
    dumpStatusToBootlog();
    return fatal.fail(.driver_policy, message);
}

fn dumpStatusToBootlog() void {
    const summary = current.summary;
    bootlog.puts("[DRVPOLICY] status phase=");
    bootlog.puts(phaseName(current.phase));
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
    bootlog.puts(yesNo(canContinueBoot()));
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
    k.puts(yesNo(canContinueBoot()));
    k.puts("\r\n");
}

fn canContinueBoot() bool {
    return !current.fatal;
}

fn yesNo(value: bool) []const u8 {
    return if (value) "yes" else "no";
}
