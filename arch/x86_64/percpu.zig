// Fixed C projection of the single native CPU-local/lifecycle owner.
pub const max_cpus: usize = 32;

const IA32_GS_BASE: u32 = 0xC000_0101;
const IA32_KERNEL_GS_BASE: u32 = 0xC000_0102;

pub const State = enum(u8) {
    absent,
    detected,
    starting,
    parked,
    online,
    stopping,
    offline,
    failed,
    // BSP completed clock registration; the AP may activate after release.
    qualified,
};

pub const CpuLocal = extern struct {
    index: u32 = 0,
    apic_id: u32 = 0,
    state: u8 = @intFromEnum(State.absent),
    reserved0: [3]u8 = .{0} ** 3,
    runtime_critical_depth: u32 = 0,
    scheduler_ready: u8 = 0,
    work_active: u8 = 0,
    reserved: [46]u8 = .{0} ** 46,
};

extern fn r4os_rust_percpu_control(operation: u8, index: u32, value: u64, extra: u8) u64;
extern fn r4os_rust_percpu_slot(index: u32) ?*CpuLocal;
extern fn r4os_rust_percpu_depth() *u32;
comptime {
    if (@offsetOf(CpuLocal, "index") != 0 or @sizeOf(CpuLocal) != 64 or
        @offsetOf(CpuLocal, "runtime_critical_depth") != 12 or
        @offsetOf(CpuLocal, "scheduler_ready") != 16)
        @compileError("Native CpuLocal assembly/C layout differs");
}
pub fn initBsp(apic_id: u32) void {
    _ = r4os_rust_percpu_control(0, apic_id, 0, 0);
}
pub fn install(index: u32) void {
    _ = r4os_rust_percpu_control(1, index, 0, 0);
}
pub fn currentIndex() u32 {
    return @intCast(r4os_rust_percpu_control(2, 0, 0, 0));
}
pub fn current() *CpuLocal {
    return at(currentIndex()).?;
}
pub fn at(index: u32) ?*CpuLocal {
    return r4os_rust_percpu_slot(index);
}
pub fn configure(index: u32, apic_id: u32) bool {
    return r4os_rust_percpu_control(3, index, apic_id, 0) != 0;
}
pub fn transition(index: u32, from: State, to: State) bool {
    return r4os_rust_percpu_control(4, index, @intFromEnum(from), @intFromEnum(to)) != 0;
}
pub fn rejectStart(index: u32) bool {
    return r4os_rust_percpu_control(5, index, 0, 0) != 0;
}
pub fn clockQualifiedMask() u64 {
    return r4os_rust_percpu_control(6, 0, 0, 0);
}
pub fn state(index: u32) State {
    return @enumFromInt(@as(u8, @intCast(r4os_rust_percpu_control(7, index, 0, 0))));
}
pub fn apicId(index: u32) ?u32 {
    const value = r4os_rust_percpu_control(8, index, 0, 0);
    return if (value == ~@as(u64, 0)) null else @intCast(value);
}
pub fn setSchedulerReady(index: u32, ready: bool) void {
    _ = r4os_rust_percpu_control(9, index, @intFromBool(ready), 0);
}
pub fn schedulerReady(index: u32) bool {
    return r4os_rust_percpu_control(10, index, 0, 0) != 0;
}
pub fn setWorkActive(index: u32, active: bool) void {
    _ = r4os_rust_percpu_control(11, index, @intFromBool(active), 0);
}
pub fn workActive(index: u32) bool {
    return r4os_rust_percpu_control(12, index, 0, 0) != 0;
}
pub fn isSchedulable(index: u32) bool {
    return r4os_rust_percpu_control(13, index, 0, 0) != 0;
}
pub fn schedulableMask() u64 {
    return r4os_rust_percpu_control(14, 0, 0, 0);
}
pub fn schedulableCount() u32 {
    return @intCast(r4os_rust_percpu_control(15, 0, 0, 0));
}
pub fn onlineMask() u64 {
    return r4os_rust_percpu_control(16, 0, 0, 0);
}
pub fn noteProductive(index: u32, r4x_work: bool) bool {
    return r4os_rust_percpu_control(17, index, @intFromBool(r4x_work), 0) != 0;
}
pub fn productiveMask() u64 {
    return r4os_rust_percpu_control(18, 0, 0, 0);
}
pub fn productiveKernelMask() u64 {
    return r4os_rust_percpu_control(19, 0, 0, 0);
}
pub fn productiveR4xMask() u64 {
    return r4os_rust_percpu_control(20, 0, 0, 0);
}
pub fn runtimeCriticalDepth() *u32 {
    return r4os_rust_percpu_depth();
}
