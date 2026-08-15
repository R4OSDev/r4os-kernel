// Audio boot boundary for the core, SID boot option, and hardware selection.
//
// HDA and AC97 are enabled through the R4D driver plan; this layer owns the
// core, SID selection, and visible audio R4D status boundaries.

const audio = @import("../audio/core.zig");
const boot_config = @import("boot_config.zig");
const boot_status = @import("boot_status.zig");
const bootlog = @import("bootlog.zig");
const fatal = @import("fatal.zig");
const loader_boot = @import("loader_boot.zig");
const platform_irq_boot = @import("platform_irq_boot.zig");
const k = @import("log.zig");

const MAX_SID_MODEL: usize = 16;

pub const BackendState = enum {
    skipped,
    disabled,
    selected,
    failed,
    r4d_planned,
    r4d_missing,
    r4d_loaded,
    r4d_registered,
    r4d_active,
};

pub const BackendStatus = struct {
    state: BackendState = .skipped,
    selected: bool = false,
    disabled: bool = false,
    attempted: bool = false,
    failed: bool = false,
    r4d_expected: bool = false,
    r4d_loaded: bool = false,
    registered: bool = false,
    active: bool = false,
    reason: []const u8 = "not selected",
};

pub const R4dPlanResult = enum {
    loaded,
    already_active,
    missing,
    init_failed,
    load_failed,
};

pub const Status = struct {
    initialized: bool = false,
    core_initialized: bool = false,
    platform_irq_ready: bool = false,
    config_visible: bool = false,
    sid_option_present: bool = false,
    sid_option_valid: bool = false,
    sid_requested_model: [MAX_SID_MODEL]u8 = .{0} ** MAX_SID_MODEL,
    sid_requested_model_len: usize = 0,
    sid_model: [MAX_SID_MODEL]u8 = .{0} ** MAX_SID_MODEL,
    sid_model_len: usize = 0,
    sid_reason: []const u8 = "not initialized",
    hda: BackendStatus = .{},
    ac97: BackendStatus = .{},
    reason: []const u8 = "not initialized",
};

var current: Status = .{};

pub fn init() bool {
    if (current.initialized) return true;

    current = .{};
    if (!platform_irq_boot.isInitialized()) {
        return fail("Audio boot before platform IRQ boot", "platform irq missing");
    }
    current.platform_irq_ready = true;

    const loaded_config = loader_boot.config() orelse {
        return fail("Audio boot before loader config", "loader config missing");
    };
    current.config_visible = true;

    audio.init();
    current.core_initialized = true;
    applySidModel(loaded_config);
    copyActualSidModel();
    startHardwareBackends(loaded_config);

    current.initialized = true;
    current.reason = "ok";
    boot_status.statusLine("  Audio boot [OK]\r\n");
    logStatus();
    return true;
}

pub fn isInitialized() bool {
    return current.initialized;
}

pub fn status() Status {
    return current;
}

pub fn hdaStatus() BackendStatus {
    return current.hda;
}

pub fn ac97Status() BackendStatus {
    return current.ac97;
}

pub fn sidModel() []const u8 {
    return current.sid_model[0..current.sid_model_len];
}

pub fn sidRequestedModel() []const u8 {
    return current.sid_requested_model[0..current.sid_requested_model_len];
}

fn applySidModel(config: *const boot_config.Config) void {
    if (boot_config.optionValue(config, "SID", "model")) |model| {
        current.sid_option_present = true;
        current.sid_option_valid = isKnownSidModel(model);
        copySlice(model, current.sid_requested_model[0..], &current.sid_requested_model_len);
        audio.configureSidModel(model);
        current.sid_reason = if (current.sid_option_valid)
            "configured from BOOT.R4S"
        else
            "invalid BOOT.R4S model; default retained";
        return;
    }

    current.sid_reason = "default model";
}

fn copyActualSidModel() void {
    copyZ(audio.sidModelNameZ(), current.sid_model[0..], &current.sid_model_len);
}

fn startHardwareBackends(config: *const boot_config.Config) void {
    current.hda = startHda(config);
    current.ac97 = planAc97R4d(config);
}

fn startHda(config: *const boot_config.Config) BackendStatus {
    if (boot_config.isDisabled(config, "HDA")) {
        k.puts("[HDA] skipped, disabled by CONFIG.R4S\r\n");
        return .{
            .state = .disabled,
            .disabled = true,
            .reason = "disabled",
        };
    }
    if (!boot_config.hasDriver(config, "HDA")) {
        k.puts("[HDA] skipped, not selected by DRIVER=HDA\r\n");
        return .{ .reason = "not selected" };
    }

    k.puts("[HDA] R4D planned via DRIVER=HDA\r\n");
    return .{
        .state = .r4d_planned,
        .selected = true,
        .r4d_expected = true,
        .reason = "planned for HDA.R4D",
    };
}

fn planAc97R4d(config: *const boot_config.Config) BackendStatus {
    const selected = boot_config.hasDriver(config, "AC97");
    if (boot_config.isDisabled(config, "AC97")) {
        k.puts("[AC97] R4D skipped, disabled by CONFIG.R4S\r\n");
        if (selected) boot_status.statusLine("  AC97.R4D [disabled]\r\n");
        return .{
            .state = .disabled,
            .selected = selected,
            .disabled = true,
            .reason = "disabled",
        };
    }
    if (!selected) {
        k.puts("[AC97] R4D skipped, not selected by DRIVER=AC97\r\n");
        return .{ .reason = "not selected" };
    }

    k.puts("[AC97] R4D planned via DRIVER=AC97\r\n");
    return .{
        .state = .r4d_planned,
        .selected = true,
        .r4d_expected = true,
        .reason = "planned for AC97.R4D",
    };
}

pub fn reportAc97R4dPlan(result: R4dPlanResult) void {
    if (!current.ac97.r4d_expected) return;

    switch (result) {
        .loaded, .already_active => {
            refreshAc97R4dBackend("r4d loaded");
            visibleAc97R4dLoadedLine();
        },
        .missing => {
            current.ac97.state = .r4d_missing;
            current.ac97.attempted = true;
            current.ac97.failed = false;
            current.ac97.reason = "AC97.R4D missing";
            boot_status.statusLine("  AC97.R4D [missing]\r\n");
        },
        .init_failed => {
            current.ac97.state = .failed;
            current.ac97.attempted = true;
            current.ac97.failed = true;
            current.ac97.reason = "AC97.R4D init failed";
            boot_status.statusLine("  AC97.R4D [init failed]\r\n");
        },
        .load_failed => {
            current.ac97.state = .failed;
            current.ac97.attempted = true;
            current.ac97.failed = true;
            current.ac97.reason = "AC97.R4D load failed";
            boot_status.statusLine("  AC97.R4D [load failed]\r\n");
        },
    }
    logStatus();
}

pub fn reportHdaR4dPlan(result: R4dPlanResult) void {
    if (!current.hda.r4d_expected) return;

    switch (result) {
        .loaded, .already_active => {
            refreshHdaR4dBackend("r4d loaded");
            visibleHdaR4dLoadedLine();
        },
        .missing => {
            current.hda.state = .r4d_missing;
            current.hda.attempted = true;
            current.hda.failed = false;
            current.hda.reason = "HDA.R4D missing";
            boot_status.statusLine("  HDA.R4D [missing]\r\n");
        },
        .init_failed => {
            current.hda.state = .failed;
            current.hda.attempted = true;
            current.hda.failed = true;
            current.hda.reason = "HDA.R4D init failed";
            boot_status.statusLine("  HDA.R4D [init failed]\r\n");
        },
        .load_failed => {
            current.hda.state = .failed;
            current.hda.attempted = true;
            current.hda.failed = true;
            current.hda.reason = "HDA.R4D load failed";
            boot_status.statusLine("  HDA.R4D [load failed]\r\n");
        },
    }
    logStatus();
}

fn refreshHdaR4dBackend(reason: []const u8) void {
    current.hda.r4d_loaded = true;
    current.hda.attempted = true;
    current.hda.failed = false;
    current.hda.registered = audio.audioBackendRegistered("HDA");
    current.hda.active = audio.audioBackendActive("HDA");
    current.hda.state = if (current.hda.active)
        .r4d_active
    else if (current.hda.registered and audio.audioBackendHasOutput("HDA"))
        .r4d_registered
    else
        .r4d_loaded;
    current.hda.reason = if (current.hda.active)
        "HDA.R4D backend active"
    else if (current.hda.registered)
        "HDA.R4D backend registered"
    else
        reason;
}

fn visibleHdaR4dLoadedLine() void {
    switch (current.hda.state) {
        .r4d_active => boot_status.statusLine("  HDA.R4D [active]\r\n"),
        .r4d_registered => boot_status.statusLine("  HDA.R4D [registered]\r\n"),
        .r4d_loaded => boot_status.statusLine("  HDA.R4D [loaded]\r\n"),
        else => {},
    }
}

fn refreshAc97R4dBackend(reason: []const u8) void {
    current.ac97.r4d_loaded = true;
    current.ac97.attempted = true;
    current.ac97.failed = false;
    current.ac97.registered = audio.audioBackendRegistered("AC97");
    current.ac97.active = audio.audioBackendActive("AC97");
    current.ac97.state = if (current.ac97.active)
        .r4d_active
    else if (current.ac97.registered and audio.audioBackendHasOutput("AC97"))
        .r4d_registered
    else
        .r4d_loaded;
    current.ac97.reason = if (current.ac97.active)
        "AC97.R4D backend active"
    else if (current.ac97.registered)
        "AC97.R4D backend registered"
    else
        reason;
}

fn visibleAc97R4dLoadedLine() void {
    switch (current.ac97.state) {
        .r4d_active => boot_status.statusLine("  AC97.R4D [active]\r\n"),
        .r4d_registered => boot_status.statusLine("  AC97.R4D [registered]\r\n"),
        .r4d_loaded => boot_status.statusLine("  AC97.R4D [loaded]\r\n"),
        else => {},
    }
}

fn fail(message: []const u8, reason: []const u8) bool {
    current.reason = reason;
    return fatal.fail(.audio, message);
}

fn logStatus() void {
    bootlog.puts("[AUDIOBOOT] initialized=");
    bootlog.puts(if (current.initialized) "yes" else "no");
    bootlog.puts(" core=");
    bootlog.puts(if (current.core_initialized) "yes" else "no");
    bootlog.puts(" config=");
    bootlog.puts(if (current.config_visible) "yes" else "no");
    bootlog.puts(" sid_option=");
    bootlog.puts(if (current.sid_option_present) "present" else "default");
    if (current.sid_option_present) {
        bootlog.puts(" requested=");
        bootlog.puts(sidRequestedModel());
        bootlog.puts(" valid=");
        bootlog.puts(if (current.sid_option_valid) "yes" else "no");
    }
    bootlog.puts(" sid_model=");
    bootlog.puts(sidModel());
    bootlog.puts(" sid_reason=");
    bootlog.puts(current.sid_reason);
    bootlog.puts(" hda=");
    bootlog.puts(backendStateName(current.hda.state));
    bootlog.puts(" hda_reason=");
    bootlog.puts(current.hda.reason);
    if (current.hda.r4d_expected) {
        bootlog.puts(" hda_r4d_loaded=");
        bootlog.puts(if (current.hda.r4d_loaded) "yes" else "no");
        bootlog.puts(" hda_registered=");
        bootlog.puts(if (current.hda.registered) "yes" else "no");
        bootlog.puts(" hda_active=");
        bootlog.puts(if (current.hda.active) "yes" else "no");
    }
    bootlog.puts(" ac97=");
    bootlog.puts(backendStateName(current.ac97.state));
    bootlog.puts(" ac97_reason=");
    bootlog.puts(current.ac97.reason);
    if (current.ac97.r4d_expected) {
        bootlog.puts(" ac97_r4d_loaded=");
        bootlog.puts(if (current.ac97.r4d_loaded) "yes" else "no");
        bootlog.puts(" ac97_registered=");
        bootlog.puts(if (current.ac97.registered) "yes" else "no");
        bootlog.puts(" ac97_active=");
        bootlog.puts(if (current.ac97.active) "yes" else "no");
    }
    bootlog.puts("\r\n");
}

pub fn backendStateName(state: BackendState) []const u8 {
    return switch (state) {
        .skipped => "skipped",
        .disabled => "disabled",
        .selected => "selected",
        .failed => "failed",
        .r4d_planned => "r4d-planned",
        .r4d_missing => "r4d-missing",
        .r4d_loaded => "r4d-loaded",
        .r4d_registered => "r4d-registered",
        .r4d_active => "r4d-active",
    };
}

pub fn dumpStatus() void {
    k.puts("Audio boot:\r\n");
    k.puts("  Core: ");
    k.puts(if (current.core_initialized) "ready" else "not ready");
    k.puts(" config=");
    k.puts(if (current.config_visible) "yes" else "no");
    k.puts("\r\n");
    dumpBackend("HDA", current.hda);
    dumpBackend("AC97", current.ac97);
    k.puts("  SID boot: model=");
    k.puts(sidModel());
    k.puts(" reason=");
    k.puts(current.sid_reason);
    k.puts("\r\n");
}

fn dumpBackend(name: []const u8, backend: BackendStatus) void {
    k.puts("  ");
    k.puts(name);
    k.puts(": state=");
    k.puts(backendStateName(backend.state));
    k.puts(" selected=");
    k.puts(if (backend.selected) "yes" else "no");
    k.puts(" disabled=");
    k.puts(if (backend.disabled) "yes" else "no");
    if (backend.r4d_expected) {
        k.puts(" r4d=");
        k.puts(if (backend.r4d_loaded) "loaded" else "expected");
        k.puts(" registered=");
        k.puts(if (backend.registered) "yes" else "no");
        k.puts(" active=");
        k.puts(if (backend.active) "yes" else "no");
    }
    k.puts(" reason=");
    k.puts(backend.reason);
    k.puts("\r\n");
}

fn isKnownSidModel(model: []const u8) bool {
    return eqIgnoreCase(model, "6581") or
        eqIgnoreCase(model, "MOS6581") or
        eqIgnoreCase(model, "8580") or
        eqIgnoreCase(model, "MOS8580");
}

fn copyZ(src: [*:0]const u8, dst: []u8, len_out: *usize) void {
    var len: usize = 0;
    while (src[len] != 0 and len + 1 < dst.len) : (len += 1) {
        dst[len] = src[len];
    }
    if (len < dst.len) dst[len] = 0;
    len_out.* = len;
}

fn copySlice(src: []const u8, dst: []u8, len_out: *usize) void {
    const len = if (src.len < dst.len) src.len else dst.len - 1;
    if (len > 0) @memcpy(dst[0..len], src[0..len]);
    if (len < dst.len) dst[len] = 0;
    len_out.* = len;
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
