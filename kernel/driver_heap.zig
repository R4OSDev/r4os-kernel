// Rust owns the complete R4D CPU heap runtime, metadata and unwind boundary.
const a = @import("r4os_kernel_contract");
extern fn r4os_rust_driver_heap_bind(owner: u32, epoch: u64) bool;
extern fn r4os_rust_driver_heap_close(owner: u32) void;
extern fn r4os_rust_driver_heap_query(owner: u32) i32;
extern fn r4os_rust_driver_heap_allocate(owner: u32, bytes: u64, alignment: u32, output: *a.DriverHeapAllocation) i32;
extern fn r4os_rust_driver_heap_release(owner: u32, handle: u64) i32;
extern fn r4os_rust_driver_heap_stats(owner: u32, output: *a.DriverHeapStats) i32;
extern fn r4os_rust_driver_heap_cleanup(owner: u32, output: *Cleanup) void;
pub const Cleanup = extern struct { released: u64 = 0, bytes: u64 = 0, quiesced: bool = true };
comptime {
    if (@sizeOf(Cleanup) != 24) @compileError("Native driver heap cleanup layout drift");
}
pub fn bind(owner: u32, epoch: u64) bool {
    return r4os_rust_driver_heap_bind(owner, epoch);
}
pub fn beginClose(owner: u32) void {
    r4os_rust_driver_heap_close(owner);
}
pub fn query(owner: u32) i32 {
    return r4os_rust_driver_heap_query(owner);
}
pub fn allocate(owner: u32, bytes: u64, alignment: u32, output: *a.DriverHeapAllocation) i32 {
    return r4os_rust_driver_heap_allocate(owner, bytes, alignment, output);
}
pub fn release(owner: u32, handle: u64) i32 {
    return r4os_rust_driver_heap_release(owner, handle);
}
pub fn stats(owner: u32, output: *a.DriverHeapStats) i32 {
    return r4os_rust_driver_heap_stats(owner, output);
}
pub fn cleanup(owner: u32) Cleanup {
    var out: Cleanup = undefined;
    r4os_rust_driver_heap_cleanup(owner, &out);
    return out;
}
