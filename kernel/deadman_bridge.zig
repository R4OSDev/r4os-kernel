// Temporary stateless display/storage owner calls. Native Rust owns watchdog
// state, scheduling, incident admission and the exact snapshot lifetime.
const block = @import("../storage/block.zig");
const diag = @import("diag_screen.zig");
const task = @import("../sched/task.zig");
export fn r4os_zig_deadman_storage(operation: u8, now: u64, threshold: u64) callconv(.c) bool {
    return switch (operation) {
        0 => block.hasStalledExecution(now, threshold),
        1 => blk: {
            block.dumpStalledToDiag(now, threshold);
            break :blk true;
        },
        2 => blk: {
            block.dumpStalledExecutions(now, threshold);
            break :blk true;
        },
        else => unreachable,
    };
}
export fn r4os_zig_deadman_screen(operation: u8, value: u64, text: [*]const u8, len: usize) callconv(.c) u64 {
    switch (operation) {
        0 => return diag.beginResolvableIncident().generation,
        1 => return @intFromBool(diag.resolveIncident(.{ .generation = value })),
        2 => diag.write(text[0..len]),
        3 => diag.endLine(),
        4 => diag.line(text[0..len]),
        else => unreachable,
    }
    return 0;
}
export fn r4os_zig_deadman_snapshot(display: bool, snapshot: *const task.DeadmanSnapshot) callconv(.c) void {
    if (display) task.dumpDeadmanSnapshotToDiag(snapshot) else task.dumpDeadmanSnapshotToLog(snapshot);
}
