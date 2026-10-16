// Sole platform-input owner is native; no Zig lock/state or policy copy.
const a = @import("r4os_kernel_contract");
extern fn r4os_rust_input_platform_snapshot(*a.PlatformInputSnapshot) i32;
extern fn r4os_rust_input_platform_submit(u32, u64, u32, u32) i32;
extern fn r4os_rust_input_platform_remove(u32) void;
extern fn r4os_rust_input_platform_usb(bool) void;
extern fn r4os_rust_input_platform_consumer(u32) void;
pub fn snapshot(out: *a.PlatformInputSnapshot) callconv(.c) i32 {
    return r4os_rust_input_platform_snapshot(out);
}
pub fn submit(owner: u32, epoch: u64, kind: u32, value: u32) i32 {
    return r4os_rust_input_platform_submit(owner, epoch, kind, value);
}
pub fn remove(owner: u32) void {
    r4os_rust_input_platform_remove(owner);
}
pub fn usb(present: bool) void {
    r4os_rust_input_platform_usb(present);
}
pub fn consumer(usage: u32) void {
    r4os_rust_input_platform_consumer(usage);
}
