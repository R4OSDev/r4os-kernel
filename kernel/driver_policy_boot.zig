// Stateless boot-policy facade; state/order/failure belongs to native Rust.
const driver_plan = @import("driver_plan.zig");
const k = @import("log.zig");
comptime {
    _ = @import("driver_policy_boot_bridge.zig");
}
const Span = extern struct { pointer: [*]const u8, len: usize };
pub const Phase = enum(u8) {
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
const RawStatus = extern struct {
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
    reason: Span = .{ .pointer = "not started", .len = 11 },
};
extern fn r4os_rust_driver_policy_init() bool;
extern fn r4os_rust_driver_policy_initialized() bool;
extern fn r4os_rust_driver_policy_status(output: *RawStatus) void;
pub fn init() bool {
    return r4os_rust_driver_policy_init();
}
pub fn isInitialized() bool {
    return r4os_rust_driver_policy_initialized();
}
pub fn status() Status {
    var value: RawStatus = undefined;
    r4os_rust_driver_policy_status(&value);
    return .{ .phase = value.phase, .attempted = value.attempted, .initialized = value.initialized, .fatal = value.fatal, .config_visible = value.config_visible, .platform_irq_ready = value.platform_irq_ready, .core_directives_applied = value.core_directives_applied, .plan_built = value.plan_built, .r4d_load_done = value.r4d_load_done, .registry_dumped = value.registry_dumped, .summary = value.summary, .reason = value.reason.pointer[0..value.reason.len] };
}
comptime {
    if (@sizeOf(RawStatus) != 184 or @offsetOf(RawStatus, "summary") != 16 or @offsetOf(RawStatus, "reason") != 168) @compileError("Native driver policy C layout drift");
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
    const current = status();
    const summary = current.summary;
    k.puts("Driver policy boot\r\n");
    k.puts("  phase=");
    k.puts(phaseName(current.phase));
    k.puts(" initialized=");
    k.puts(yesNo(current.initialized));
    k.puts(" fatal=");
    k.puts(yesNo(current.fatal));
    k.puts(" continue=");
    k.puts(yesNo(!current.fatal));
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

fn yesNo(value: bool) []const u8 {
    return if (value) "yes" else "no";
}
