// Canonical native diagnostic orchestration; private bridge only copies
// still-Zig owner snapshots or forwards Work operations with explicit C args.
comptime {
    _ = @import("task_probe_bridge.zig");
}
extern fn r4os_rust_task_registry_selftest() callconv(.c) bool;
pub fn runIfEnabled() bool {
    return r4os_rust_task_registry_selftest();
}
