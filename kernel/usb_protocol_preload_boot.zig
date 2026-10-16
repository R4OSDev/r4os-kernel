// Native BSP preload coordinator; external modules retain their Zig ABI.
extern fn r4os_rust_usb_preload_init() bool;
pub fn init() bool {
    return r4os_rust_usb_preload_init();
}
