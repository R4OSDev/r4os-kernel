const std = @import("std");

pub const Rank = struct {
    pub const program_state: u8 = 1;
    pub const io_owner: u8 = 2;
    pub const heap: u8 = 5;
    pub const virtual_memory: u8 = 10;
    pub const page_tables: u8 = 20;
    pub const physical_memory: u8 = 30;
    pub const boot_log: u8 = 70;
    pub const serial_output: u8 = 80;
};

pub fn orderAllowed(previous_rank: u8, requested_rank: u8) bool {
    return previous_rank == 0 or previous_rank < requested_rank;
}

test "owner ranks admit the canonical memory and terminal logging order" {
    try std.testing.expect(orderAllowed(0, Rank.program_state));
    try std.testing.expect(orderAllowed(Rank.program_state, Rank.heap));
    try std.testing.expect(orderAllowed(Rank.heap, Rank.virtual_memory));
    try std.testing.expect(orderAllowed(Rank.virtual_memory, Rank.page_tables));
    try std.testing.expect(orderAllowed(Rank.page_tables, Rank.physical_memory));
    try std.testing.expect(orderAllowed(Rank.physical_memory, Rank.serial_output));
}

test "owner ranks reject reverse acquisition" {
    try std.testing.expect(!orderAllowed(Rank.physical_memory, Rank.page_tables));
    try std.testing.expect(!orderAllowed(Rank.serial_output, Rank.heap));
    try std.testing.expect(!orderAllowed(Rank.heap, Rank.program_state));
}

test "independent owners at one rank cannot nest" {
    try std.testing.expect(!orderAllowed(Rank.io_owner, Rank.io_owner));
}
