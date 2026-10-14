// Single runtime owner and privileged sequences live in Rust.
const percpu = @import("percpu.zig");
extern fn r4os_rust_runtime_control(operation: u8, saved: u64) u64;
extern fn r4os_rust_runtime_stats(output: *RuntimeStats) void;
pub const RuntimeStats = extern struct {
    acquisitions: u64 = 0,
    nested_acquisitions: u64 = 0,
    collisions: u64 = 0,
    cpu_collisions: u64 = 0,
    wait_spins: u64 = 0,
    max_wait_spins: u64 = 0,
    hold_cycles: u64 = 0,
    max_hold_cycles: u64 = 0,
    legacy_global_acquisitions: u64 = 0,
};

export fn r4os_zig_runtime_depth() callconv(.c) *u32 {
    return percpu.runtimeCriticalDepth();
}
pub fn disable() void {
    _ = r4os_rust_runtime_control(0, 0);
}
pub fn enable() void {
    _ = r4os_rust_runtime_control(1, 0);
}
pub fn saveAndDisableLocal() u64 {
    return r4os_rust_runtime_control(2, 0);
}
pub fn restoreLocal(flags: u64) void {
    _ = r4os_rust_runtime_control(3, flags);
}
pub fn saveAndDisableRuntime() u64 {
    return r4os_rust_runtime_control(4, 0);
}
pub fn restore(flags: u64) void {
    _ = r4os_rust_runtime_control(5, flags);
}
pub fn enableRuntimeSerialization() void {
    _ = r4os_rust_runtime_control(6, 0);
}
pub fn runtimeSerializationEnabled() bool {
    return r4os_rust_runtime_control(7, 0) != 0;
}
pub fn inRuntimeCriticalSection() bool {
    return r4os_rust_runtime_control(8, 0) != 0;
}
pub fn releaseRuntimeForContextSwitch() bool {
    return r4os_rust_runtime_control(9, 0) != 0;
}
pub fn wereEnabled(flags: u64) bool {
    return r4os_rust_runtime_control(10, flags) != 0;
}
pub fn haltForever() noreturn {
    _ = r4os_rust_runtime_control(11, 0);
    unreachable;
}
pub fn waitForInterrupt() void {
    _ = r4os_rust_runtime_control(12, 0);
}
pub fn enableAndWaitForInterrupt() void {
    _ = r4os_rust_runtime_control(13, 0);
}
pub fn runtimeStats() RuntimeStats {
    var output: RuntimeStats = undefined;
    r4os_rust_runtime_stats(&output);
    return output;
}
comptime {
    if (percpu.max_cpus != 32 or @sizeOf(RuntimeStats) != 72 or @offsetOf(RuntimeStats, "legacy_global_acquisitions") != 64)
        @compileError("Rust runtime CPU bound or C snapshot differs");
}
