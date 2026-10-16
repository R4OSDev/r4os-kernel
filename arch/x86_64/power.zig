// Terminal power ownership is native; logging keeps its actual current owner.
const k = @import("../../kernel/log.zig");
extern fn r4os_rust_terminal_poweroff() noreturn;
extern fn r4os_rust_terminal_power_status() void;
pub fn poweroff() noreturn {
    r4os_rust_terminal_poweroff();
}
pub fn dumpStatus() void {
    r4os_rust_terminal_power_status();
}
export fn r4os_zig_terminal_log(op: u8, text: ?[*]const u8, len: usize, value: u64, width: usize) callconv(.c) void {
    switch (op) {
        0 => k.puts(text.?[0..len]),
        1 => k.putDec(value),
        2 => k.putHex(value, @intCast(width)),
        3 => k.serialFlush(),
        else => unreachable,
    }
}
