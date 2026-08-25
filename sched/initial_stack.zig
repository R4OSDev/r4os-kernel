pub const saved_register_count: usize = 6;

pub const Layout = struct {
    aligned_top: u64,
    entry_rsp: u64,
    return_slot: u64,
    switch_rsp: u64,
};

/// Builds the synthetic context consumed by `r4os_context_switch`.
///
/// The switch restores six registers and enters the trampoline with `ret`,
/// not `call`. SysV x86_64 nevertheless requires RSP % 16 == 8 at a function
/// entry. One unused word below the aligned stack top supplies the missing
/// return-address alignment before the synthetic trampoline slot.
pub fn layout(stack_top: u64) Layout {
    const aligned_top = stack_top & ~@as(u64, 0xF);
    const entry_rsp = aligned_top - 8;
    const return_slot = entry_rsp - 8;
    const switch_rsp = return_slot - saved_register_count * @sizeOf(u64);
    return .{
        .aligned_top = aligned_top,
        .entry_rsp = entry_rsp,
        .return_slot = return_slot,
        .switch_rsp = switch_rsp,
    };
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
