// Native Rust owns R4D semaphore metadata, queues and wait/free boundaries.
const a = @import("r4os_kernel_contract");
extern fn r4os_rust_driver_semaphore_bind(owner: u32, epoch: u64) bool;
extern fn r4os_rust_driver_semaphore_discard(owner: u32, epoch: u64) bool;
extern fn r4os_rust_driver_semaphore_query(owner: u32) i32;
extern fn r4os_rust_driver_semaphore_create(owner: u32, initial: u32, maximum: u32, out: *u64) i32;
extern fn r4os_rust_driver_semaphore_acquire(owner: u32, handle: u64, ticks: u64) i32;
extern fn r4os_rust_driver_semaphore_release(owner: u32, handle: u64) i32;
extern fn r4os_rust_driver_semaphore_destroy(owner: u32, handle: u64) i32;
extern fn r4os_rust_driver_semaphore_status(owner: u32, handle: u64, out: *a.DriverSemaphoreStatus) i32;
extern fn r4os_rust_driver_semaphore_stats(owner: u32, out: *a.DriverSemaphoreStats) i32;
extern fn r4os_rust_driver_semaphore_close(owner: u32) void;
extern fn r4os_rust_driver_semaphore_cleanup(owner: u32, out: *Cleanup) void;
extern fn r4os_rust_driver_semaphore_can_sleep() bool;
pub const Cleanup = extern struct { released: u64 = 0, quiesced: bool = true };
comptime {
    if (@sizeOf(Cleanup) != 16) @compileError("Native semaphore cleanup C layout drift");
}
pub fn bind(owner: u32, epoch: u64) bool {
    return r4os_rust_driver_semaphore_bind(owner, epoch);
}
pub fn discardEmptyBinding(owner: u32, epoch: u64) bool {
    return r4os_rust_driver_semaphore_discard(owner, epoch);
}
pub fn query(owner: u32) i32 {
    return r4os_rust_driver_semaphore_query(owner);
}
pub fn create(owner: u32, initial: u32, maximum: u32, out: *u64) i32 {
    return r4os_rust_driver_semaphore_create(owner, initial, maximum, out);
}
pub fn acquire(owner: u32, handle: u64, ticks: u64) i32 {
    return r4os_rust_driver_semaphore_acquire(owner, handle, ticks);
}
pub fn release(owner: u32, handle: u64) i32 {
    return r4os_rust_driver_semaphore_release(owner, handle);
}
pub fn destroy(owner: u32, handle: u64) i32 {
    return r4os_rust_driver_semaphore_destroy(owner, handle);
}
pub fn status(owner: u32, handle: u64, out: *a.DriverSemaphoreStatus) i32 {
    return r4os_rust_driver_semaphore_status(owner, handle, out);
}
pub fn stats(owner: u32, out: *a.DriverSemaphoreStats) i32 {
    return r4os_rust_driver_semaphore_stats(owner, out);
}
pub fn beginClose(owner: u32) void {
    r4os_rust_driver_semaphore_close(owner);
}
pub fn cleanup(owner: u32) Cleanup {
    var out: Cleanup = undefined;
    r4os_rust_driver_semaphore_cleanup(owner, &out);
    return out;
}
pub fn canSleep() bool {
    return r4os_rust_driver_semaphore_can_sleep();
}
