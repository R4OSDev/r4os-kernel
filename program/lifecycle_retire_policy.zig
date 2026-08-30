const std = @import("std");

/// A naturally returned R4X task owns its kernel exit epilogue until its
/// generation-checked execution pin has been released and the scheduler
/// reaper has removed the exact Task generation. The program reaper must not
/// kill or claim the task while either owner is still present. Hard-killed
/// threads use a different ProgramThread state and remain program-reaper-owned.
pub fn naturalExitEpilogueOwnsTask(
    exited: bool,
    execution_pinned: bool,
    scheduler_task_present: bool,
) bool {
    return exited and (execution_pinned or scheduler_task_present);
}

test "natural exit owns task through scheduler task retirement" {
    try std.testing.expect(naturalExitEpilogueOwnsTask(true, true, true));
    try std.testing.expect(naturalExitEpilogueOwnsTask(true, true, false));
    try std.testing.expect(naturalExitEpilogueOwnsTask(true, false, true));
    try std.testing.expect(!naturalExitEpilogueOwnsTask(true, false, false));
    try std.testing.expect(!naturalExitEpilogueOwnsTask(false, true, true));
    try std.testing.expect(!naturalExitEpilogueOwnsTask(false, false, true));
    try std.testing.expect(!naturalExitEpilogueOwnsTask(false, true, false));
    try std.testing.expect(!naturalExitEpilogueOwnsTask(false, false, false));
}
