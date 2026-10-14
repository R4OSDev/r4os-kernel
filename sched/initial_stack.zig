comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
pub const saved_register_count: usize = 6;

pub const Layout = extern struct {
    aligned_top: u64,
    entry_rsp: u64,
    return_slot: u64,
    switch_rsp: u64,
};
extern fn r4os_rust_initial_stack(top: u64, output: *Layout) void;
comptime {
    if (@sizeOf(Layout) != 32 or saved_register_count != 6)
        @compileError("Native synthetic context layout differs");
}

/// Builds the synthetic context consumed by `r4os_context_switch`.
///
/// The switch restores six registers and enters the trampoline with `ret`,
/// not `call`. SysV x86_64 nevertheless requires RSP % 16 == 8 at a function
/// entry. One unused word below the aligned stack top supplies the missing
/// return-address alignment before the synthetic trampoline slot.
pub fn layout(stack_top: u64) Layout {
    var result: Layout = undefined;
    r4os_rust_initial_stack(stack_top, &result);
    return result;
}

test "synthetic task entry preserves SysV call alignment" {
    const std = @import("std");
    const prepared = layout(0x10007);
    try std.testing.expectEqual(@as(u64, 0), prepared.aligned_top & 0xF);
    try std.testing.expectEqual(@as(u64, 8), prepared.entry_rsp & 0xF);
    try std.testing.expectEqual(@as(u64, 0), prepared.return_slot & 0xF);
    try std.testing.expectEqual(@as(u64, 0), prepared.switch_rsp & 0xF);
    try std.testing.expectEqual(@as(u64, 64), prepared.aligned_top - prepared.switch_rsp);
}
