// Private C facade; Rust owns the original privileged MSR operations.
extern fn r4os_rust_msr_read(id: u32) u64;
extern fn r4os_rust_msr_write(id: u32, value: u64) void;
pub fn read(id: u32) u64 {
    return r4os_rust_msr_read(id);
}
pub fn write(id: u32, value: u64) void {
    r4os_rust_msr_write(id, value);
}
