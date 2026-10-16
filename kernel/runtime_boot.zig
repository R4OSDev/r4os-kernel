// Native BSP runtime/worker/launcher orchestration. These remaining cuts are
// stateless adapters to typed Zig callback ABI, presentation and audio owners.
const bootscreen = @import("bootscreen.zig");
const task = @import("../sched/task.zig");
const audio = @import("../audio/core.zig");
const crash = @import("crash.zig");
comptime {
    // Direct native calls still need these separately owned presentation /
    // config adapters to publish their existing C exports into this product.
    _ = @import("shell_launcher.zig");
    _ = @import("deadman.zig");
}
extern fn r4os_rust_runtime_boot_init() callconv(.c) bool;
extern fn r4os_rust_runtime_boot_start() callconv(.c) noreturn;
pub fn initTaskRuntime() bool {
    return r4os_rust_runtime_boot_init();
}
pub fn start() noreturn {
    r4os_rust_runtime_boot_start();
}
fn reclaimTaskStackCache(requested_stacks: u32) u32 {
    return task.reclaimStackCache(requested_stacks);
}
export fn r4os_zig_runtime_stack_reclaimer() callconv(.c) usize {
    return @intFromPtr(&reclaimTaskStackCache);
}
export fn r4os_zig_runtime_status(data: [*]const u8, len: usize) callconv(.c) void {
    bootscreen.setStatus(data[0..len]);
}
export fn r4os_zig_runtime_audio(progress_worker: bool) callconv(.c) bool {
    if (progress_worker) return audio.startProgressWorker();
    _ = audio.applyConfiguredSidModel();
    return true;
}

comptime {
    if (@intFromEnum(crash.BootPhase.runtime) != 13 or @intFromEnum(crash.BootPhase.shell) != 18)
        @compileError("Native runtime boot phase drift");
}
