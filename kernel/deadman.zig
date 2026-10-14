// Native cooperative watchdog. The bridge calls only the not-yet-migrated
// storage/display owners and has no watchdog state or scheduling policy.
comptime {
    _ = @import("deadman_bridge.zig");
}
extern fn r4os_rust_deadman_start() callconv(.c) bool;
pub fn start() bool {
    return r4os_rust_deadman_start();
}
