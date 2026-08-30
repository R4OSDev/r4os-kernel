const boot_config = @import("boot_config.zig");
const audio_boot = @import("audio_boot.zig");
const bootlog = @import("bootlog.zig");
const bootscreen = @import("bootscreen.zig");
const keyboard = @import("../driver/input/keyboard.zig");
const mouse = @import("../driver/input/mouse.zig");
const driver_registry = @import("../driver/registry.zig");
const r4d = @import("../program/r4d.zig");

const MAX_PLAN: usize = 8;
const MAX_NAME: usize = 24;

const PlannedDriver = struct {
    name: [MAX_NAME]u8 = .{0} ** MAX_NAME,
    name_len: usize = 0,
};

pub const Summary = struct {
    run_started: bool = false,
    config_driver_count: usize = 0,
    selected: usize = 0,
    skipped: usize = 0,
    disabled: usize = 0,
    duplicates: usize = 0,
    capacity_limited: usize = 0,
    already_active: usize = 0,
    loaded: usize = 0,
    file_missing: usize = 0,
    invalid_file: usize = 0,
    name_invalid: usize = 0,
    init_failed: usize = 0,
    load_failed: usize = 0,
    warnings: usize = 0,
    ps2_disabled: bool = false,
    ps2_registry_missing: bool = false,
    atapio_disable_ignored: bool = false,
    registry_before: usize = 0,
    registry_after: usize = 0,
    boot_continue: bool = true,
};

var plan: [MAX_PLAN]PlannedDriver = .{PlannedDriver{}} ** MAX_PLAN;
var plan_count: usize = 0;
var last_summary: Summary = .{};

pub fn beginPolicyRun(config: *const boot_config.Config) void {
    last_summary = .{
        .run_started = true,
        .config_driver_count = config.driver_count,
        .registry_before = driver_registry.countUsed(),
        .registry_after = driver_registry.countUsed(),
        .boot_continue = true,
    };
}

pub fn lastSummary() Summary {
    return last_summary;
}

pub fn buildAndLoad(config: *const boot_config.Config) void {
    ensurePolicyRun(config);
    plan_count = 0;
    bootlog.puts("[PLAN] build\r\n");
    if (config.auto_acpi) bootlog.puts("[PLAN] AUTO=ACPI enabled\r\n");
    if (config.auto_pci) bootlog.puts("[PLAN] AUTO=PCI enabled\r\n");

    var i: usize = 0;
    while (i < config.driver_count) : (i += 1) {
        addIfNotDisabled(boot_config.driverName(i), config);
    }
    last_summary.selected = plan_count;
    bootlog.puts("[PLAN] drivers selected=");
    bootlog.putDec(plan_count);
    bootlog.puts("\r\n");

    i = 0;
    while (i < plan_count) : (i += 1) {
        const name = plan[i].name[0..plan[i].name_len];
        loadPlanned(name);
    }
    last_summary.registry_after = driver_registry.countUsed();
}

pub fn applyCoreDirectives(config: *const boot_config.Config) void {
    ensurePolicyRun(config);
    if (boot_config.isDisabled(config, "PS2")) {
        bootlog.puts("[PLAN] disable PS2 core input\r\n");
        if (driver_registry.findByName("PS2")) |slot| {
            driver_registry.setState(slot, .shutdown);
            keyboard.disable();
            mouse.disable();
            driver_registry.markUnloaded(slot);
            last_summary.ps2_disabled = true;
        } else {
            bootlog.puts("[PLAN][WARN] PS2 registry entry missing\r\n");
            last_summary.ps2_registry_missing = true;
            last_summary.warnings += 1;
        }
    }

    if (boot_config.isDisabled(config, "ATAPIO")) {
        bootlog.puts("[PLAN][WARN] DISABLE=ATAPIO ignored after boot-storage init\r\n");
        last_summary.atapio_disable_ignored = true;
        last_summary.warnings += 1;
    }
    last_summary.registry_after = driver_registry.countUsed();
}

fn addIfNotDisabled(name: []const u8, config: *const boot_config.Config) void {
    if (name.len == 0) {
        last_summary.skipped += 1;
        last_summary.name_invalid += 1;
        last_summary.warnings += 1;
        return;
    }
    if (plan_count >= MAX_PLAN) {
        bootlog.puts("[PLAN][WARN] skip ");
        bootlog.puts(name);
        bootlog.puts(" plan capacity reached\r\n");
        last_summary.skipped += 1;
        last_summary.capacity_limited += 1;
        last_summary.warnings += 1;
        return;
    }
    var i: usize = 0;
    while (i < config.disabled_count) : (i += 1) {
        if (nameEq(name, boot_config.disabledName(i))) {
            bootlog.puts("[PLAN] skip ");
            bootlog.puts(name);
            bootlog.puts(" disabled by CONFIG.R4S\r\n");
            last_summary.skipped += 1;
            last_summary.disabled += 1;
            return;
        }
    }
    if (planContains(name)) {
        bootlog.puts("[PLAN] skip ");
        bootlog.puts(name);
        bootlog.puts(" already planned\r\n");
        last_summary.skipped += 1;
        last_summary.duplicates += 1;
        return;
    }
    copyName(name, plan[plan_count].name[0..], &plan[plan_count].name_len);
    plan_count += 1;
}

fn loadPlanned(name: []const u8) void {
    bootscreen.setDriver(name);
    bootlog.puts("[PLAN] load ");
    bootlog.puts(name);
    bootlog.puts("\r\n");

    if (driver_registry.findByName(name)) |slot| {
        if (driver_registry.get(slot)) |entry| {
            if (entry.state == .loaded or entry.state == .initialized or entry.state == .active) {
                bootlog.puts("[PLAN] ");
                bootlog.puts(name);
                bootlog.puts(" already ");
                bootlog.puts(driver_registry.sourceName(entry.source));
                bootlog.puts("/");
                bootlog.puts(driver_registry.stateName(entry.state));
                bootlog.puts("\r\n");
                last_summary.already_active += 1;
                return;
            }
        }
    }

    const result = r4d.loadRuntimeNameResult(name);
    recordLoadResult(result);
    reportAudioR4dPlan(name, result);
    if (!r4d.runtimeLoadSucceeded(result)) {
        switch (result) {
            // A configured optional driver may report init-failed simply
            // because its device is absent. File/format/load failures are
            // different: they are actionable boot-image errors and belong
            // in the second bootscreen row.
            .name_invalid, .file_missing, .invalid_file, .load_failed => bootscreen.setDriverError(name),
            .init_failed, .loaded, .already_active => {},
        }
        bootlog.puts("[PLAN][WARN] load failed ");
        bootlog.puts(name);
        bootlog.puts(" result=");
        bootlog.puts(r4d.runtimeLoadResultName(result));
        bootlog.puts("\r\n");
    }
}

fn recordLoadResult(result: r4d.RuntimeLoadResult) void {
    switch (result) {
        .loaded => last_summary.loaded += 1,
        .already_active => last_summary.already_active += 1,
        .name_invalid => {
            last_summary.name_invalid += 1;
            last_summary.warnings += 1;
        },
        .file_missing => {
            last_summary.file_missing += 1;
            last_summary.warnings += 1;
        },
        .invalid_file => {
            last_summary.invalid_file += 1;
            last_summary.warnings += 1;
        },
        .init_failed => {
            last_summary.init_failed += 1;
            last_summary.warnings += 1;
        },
        .load_failed => {
            last_summary.load_failed += 1;
            last_summary.warnings += 1;
        },
    }
}

fn reportAudioR4dPlan(name: []const u8, result: r4d.RuntimeLoadResult) void {
    const plan_result: audio_boot.R4dPlanResult = switch (result) {
        .loaded => .loaded,
        .already_active => .already_active,
        .file_missing => .missing,
        .init_failed => .init_failed,
        else => .load_failed,
    };
    if (nameEq(name, "AC97")) {
        audio_boot.reportAc97R4dPlan(plan_result);
        return;
    }
    if (nameEq(name, "HDA")) {
        audio_boot.reportHdaR4dPlan(plan_result);
        return;
    }
}

fn planContains(name: []const u8) bool {
    var i: usize = 0;
    while (i < plan_count) : (i += 1) {
        if (nameEq(plan[i].name[0..plan[i].name_len], name)) return true;
    }
    return false;
}

fn copyName(src: []const u8, dst: []u8, len_out: *usize) void {
    const n = if (src.len < dst.len) src.len else dst.len - 1;
    var i: usize = 0;
    while (i < n) : (i += 1) dst[i] = upper(src[i]);
    if (n < dst.len) dst[n] = 0;
    len_out.* = n;
}

fn nameEq(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var i: usize = 0;
    while (i < a.len) : (i += 1) if (upper(a[i]) != upper(b[i])) return false;
    return true;
}

fn upper(c: u8) u8 {
    if (c >= 'a' and c <= 'z') return c - ('a' - 'A');
    return c;
}

fn ensurePolicyRun(config: *const boot_config.Config) void {
    if (!last_summary.run_started) beginPolicyRun(config);
}
