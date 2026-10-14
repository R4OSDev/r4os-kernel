// Early PIT/periodic foundation and completion are owned by Rust.
extern fn r4os_rust_timer_boot_control(operation: u8) bool;

pub fn init() void {
    _ = r4os_rust_timer_boot_control(0);
}

pub fn isInitialized() bool {
    return r4os_rust_timer_boot_control(1);
}
