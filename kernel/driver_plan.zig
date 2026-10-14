// Stateless Config/Rust-plan facade. All plan/accounting state is native.
const boot_config = @import("boot_config.zig");
comptime {
    _ = @import("driver_plan_bridge.zig");
}
pub const Summary = extern struct {
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
extern fn r4os_rust_driver_plan_control(operation: u8, config: *const anyopaque) void;
extern fn r4os_rust_driver_plan_summary(output: *Summary) void;
pub fn beginPolicyRun(config: *const boot_config.Config) void {
    r4os_rust_driver_plan_control(0, config);
}
pub fn buildAndLoad(config: *const boot_config.Config) void {
    r4os_rust_driver_plan_control(1, config);
}
pub fn applyCoreDirectives(config: *const boot_config.Config) void {
    r4os_rust_driver_plan_control(2, config);
}
pub fn lastSummary() Summary {
    var value: Summary = undefined;
    r4os_rust_driver_plan_summary(&value);
    return value;
}
comptime {
    if (@sizeOf(Summary) != 152 or @offsetOf(Summary, "registry_before") != 128) @compileError("Native driver summary C layout drift");
}
