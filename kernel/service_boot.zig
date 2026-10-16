// Native singleton service boot flag; policy remains with external SERVMAN.
extern fn r4os_rust_service_boot_init() callconv(.c) void;
extern fn r4os_rust_service_boot_ready() callconv(.c) bool;
pub fn init() void {
    r4os_rust_service_boot_init();
}
pub fn isInitialized() bool {
    return r4os_rust_service_boot_ready();
}
