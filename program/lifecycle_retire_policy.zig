//! Native natural-exit ownership decision; the original fixture stays Zig.
const std = @import("std");
extern fn r4os_rust_program_natural_exit(bool, bool, bool) bool;
pub fn naturalExitEpilogueOwnsTask(
    exited: bool,
    execution_pinned: bool,
    scheduler_task_present: bool,
) bool {
    return r4os_rust_program_natural_exit(exited, execution_pinned, scheduler_task_present);
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
