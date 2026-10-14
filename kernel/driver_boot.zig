// Stateless early-boot facade; registry and idempotence belong to Rust.
extern fn r4os_rust_driver_boot(operation: u8) bool;
pub fn init() void {
    _ = r4os_rust_driver_boot(0);
}
pub fn isInitialized() bool {
    return r4os_rust_driver_boot(1);
}
