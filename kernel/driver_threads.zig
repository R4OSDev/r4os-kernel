// Native Rust owns R4D dedicated thread construction, exact task/FPU
// identity, waits, callback abort and confirmed retirement/backing release.
const a = @import("r4os_kernel_contract");
extern fn r4os_rust_driver_thread_bind(owner: u32, epoch: u64) bool;
extern fn r4os_rust_driver_thread_discard(owner: u32, epoch: u64) bool;
extern fn r4os_rust_driver_thread_query(owner: u32) i32;
extern fn r4os_rust_driver_thread_current(op: u8, owner: u32) u64;
extern fn r4os_rust_driver_thread_request(owner: u32, out: *a.DriverThreadRequest) i32;
extern fn r4os_rust_driver_thread_start(owner: u32, input: *const a.DriverThreadRequest, out: *u64) i32;
extern fn r4os_rust_driver_thread_stop(owner: u32, handle: u64) i32;
extern fn r4os_rust_driver_thread_abort(owner: u32, result: i32) i32;
extern fn r4os_rust_driver_thread_status(owner: u32, handle: u64, out: *a.DriverThreadStatus) i32;
extern fn r4os_rust_driver_thread_stats(owner: u32, out: *a.DriverThreadStats) i32;
extern fn r4os_rust_driver_thread_join(owner: u32, handle: u64, ticks: u64, out: *i32) i32;
extern fn r4os_rust_driver_thread_sleep(owner: u32, ticks: u64) i32;
extern fn r4os_rust_driver_thread_release(owner: u32, handle: u64) i32;
extern fn r4os_rust_driver_thread_stop_existing(owner: u32) void;
extern fn r4os_rust_driver_thread_close(owner: u32) void;
extern fn r4os_rust_driver_thread_quiesced(owner: u32) bool;
extern fn r4os_rust_driver_thread_cleanup(owner: u32, out: *Cleanup) void;
pub const Cleanup = extern struct { released: u64 = 0, quiesced: bool = true };
comptime {
    if (@sizeOf(Cleanup) != 16) @compileError("Native R4D thread cleanup layout drift");
}
pub fn bind(owner: u32, epoch: u64) bool {
    return r4os_rust_driver_thread_bind(owner, epoch);
}
pub fn discardEmptyBinding(owner: u32, epoch: u64) bool {
    return r4os_rust_driver_thread_discard(owner, epoch);
}
pub fn query(owner: u32) i32 {
    return r4os_rust_driver_thread_query(owner);
}
pub fn currentOwner() u32 {
    return @intCast(r4os_rust_driver_thread_current(0, 0));
}
pub fn currentWorkOwner() u32 {
    return @intCast(r4os_rust_driver_thread_current(1, 0));
}
pub fn current(owner: u32) u64 {
    return r4os_rust_driver_thread_current(2, owner);
}
pub fn currentRequest(owner: u32, out: *a.DriverThreadRequest) i32 {
    return r4os_rust_driver_thread_request(owner, out);
}
pub fn start(owner: u32, input: *const a.DriverThreadRequest, out: *u64) i32 {
    return r4os_rust_driver_thread_start(owner, input, out);
}
pub fn stop(owner: u32, handle: u64) i32 {
    return r4os_rust_driver_thread_stop(owner, handle);
}
pub fn abortCurrent(owner: u32, result: i32) i32 {
    return r4os_rust_driver_thread_abort(owner, result);
}
pub fn status(owner: u32, handle: u64, out: *a.DriverThreadStatus) i32 {
    return r4os_rust_driver_thread_status(owner, handle, out);
}
pub fn stats(owner: u32, out: *a.DriverThreadStats) i32 {
    return r4os_rust_driver_thread_stats(owner, out);
}
pub fn join(owner: u32, handle: u64, ticks: u64, out: *i32) i32 {
    return r4os_rust_driver_thread_join(owner, handle, ticks, out);
}
pub fn sleepTicks(owner: u32, ticks: u64) i32 {
    return r4os_rust_driver_thread_sleep(owner, ticks);
}
pub fn release(owner: u32, handle: u64) i32 {
    return r4os_rust_driver_thread_release(owner, handle);
}
pub fn beginClose(owner: u32) void {
    r4os_rust_driver_thread_close(owner);
}
pub fn stopExisting(owner: u32) void {
    r4os_rust_driver_thread_stop_existing(owner);
}
pub fn callbacksQuiesced(owner: u32) bool {
    return r4os_rust_driver_thread_quiesced(owner);
}
pub fn cleanup(owner: u32) Cleanup {
    var out: Cleanup = undefined;
    r4os_rust_driver_thread_cleanup(owner, &out);
    return out;
}
