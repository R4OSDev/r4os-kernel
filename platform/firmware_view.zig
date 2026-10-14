// Native owner borrows permanent platform mappings, without new aliases.
extern fn r4os_rust_firmware_root() u64;
extern fn r4os_rust_firmware_view(base: u64, bytes: u64, output: *u64) bool;
pub fn root() u64 {
    return r4os_rust_firmware_root();
}
pub fn view(base: u64, bytes: u64) ?u64 {
    var address: u64 = undefined;
    return if (r4os_rust_firmware_view(base, bytes, &address)) address else null;
}
