// Private C facade; PIT rate/divisor and wrapping epoch live only in Rust.
extern fn r4os_rust_pit_control(operation: u8, argument: u32) u64;
pub const BASE_HZ: u32 = 1_193_182;
pub const DEFAULT_HZ: u32 = 1000;
pub const IRQ: u8 = 0;
pub fn init(requested_hz: u32) void {
    _ = r4os_rust_pit_control(0, requested_hz);
}
pub fn onTick() void {
    _ = r4os_rust_pit_control(1, 0);
}
pub fn tickCount() u64 {
    return r4os_rust_pit_control(2, 0);
}
pub fn frequency() u32 {
    return @intCast(r4os_rust_pit_control(3, 0));
}
pub fn divisor() u32 {
    return @intCast(r4os_rust_pit_control(4, 0));
}
