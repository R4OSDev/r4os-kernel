// Native IRQ-safe revision and scheduler-owned wait enrollment; no second state.
extern fn r4os_rust_desktop_signal() callconv(.c) void;
extern fn r4os_rust_desktop_sequence() callconv(.c) u64;
extern fn r4os_rust_desktop_wait(u64, u64, *u64) callconv(.c) i32;
pub fn signal() void {
    r4os_rust_desktop_signal();
}
pub fn sequence() u64 {
    return r4os_rust_desktop_sequence();
}
pub fn wait(last_seq: u64, timeout_ticks: u64, out_seq: *u64) i32 {
    return r4os_rust_desktop_wait(last_seq, timeout_ticks, out_seq);
}
