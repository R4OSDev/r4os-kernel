// CPU-local architectural state.  The first field is intentionally the
// scheduler-visible CPU index: r4os_current_cpu_index reads it through GS:0.

const msr = @import("msr.zig");
const builtin = @import("builtin");

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
};

pub const CpuLocal = extern struct {
    index: u32 = 0,
    apic_id: u32 = 0,
    state: u8 = @intFromEnum(State.absent),
    reserved0: [3]u8 = .{0} ** 3,
    legacy_critical_depth: u32 = 0,
    scheduler_ready: u8 = 0,
    work_active: u8 = 0,
    reserved: [46]u8 = .{0} ** 46,
};

comptime {
    if (@offsetOf(CpuLocal, "index") != 0 or @sizeOf(CpuLocal) != 64) {
        @compileError("CpuLocal must keep its assembly ABI and cache-line size");
    }
}

extern fn r4os_current_cpu_index() callconv(.c) u32;

var locals: [max_cpus]CpuLocal align(64) = .{CpuLocal{}} ** max_cpus;
var cpu_local_ready: bool = false;
var schedulable_mask: u64 = 0;
var productive_kernel_mask: u64 = 0;
var productive_r4x_mask: u64 = 0;

pub fn initBsp(apic_id: u32) void {
    locals = .{CpuLocal{}} ** max_cpus;
    locals[0].index = 0;
    locals[0].apic_id = apic_id;
    locals[0].state = @intFromEnum(State.online);
    install(0);
    @atomicStore(u64, &schedulable_mask, 1, .release);
    @atomicStore(u64, &productive_kernel_mask, 0, .release);
    @atomicStore(u64, &productive_r4x_mask, 0, .release);
    @atomicStore(bool, &cpu_local_ready, true, .release);
}

pub fn install(index: u32) void {
    if (index >= max_cpus) return;
    const base = @intFromPtr(&locals[index]);
    msr.write(IA32_GS_BASE, base);
    // R4OS does not use swapgs yet.  Keeping both bases identical prevents a
    // later defensive swapgs from exposing an unrelated address.
    msr.write(IA32_KERNEL_GS_BASE, base);
}

pub fn currentIndex() u32 {
    if (comptime builtin.os.tag != .freestanding) return 0;
    if (!@atomicLoad(bool, &cpu_local_ready, .acquire)) return 0;
    const index = r4os_current_cpu_index();
    return if (index < max_cpus) index else 0;
}

pub fn current() *CpuLocal {
    return &locals[currentIndex()];
}

pub fn at(index: u32) ?*CpuLocal {
    if (index >= max_cpus) return null;
    return &locals[index];
}

pub fn configure(index: u32, apic_id: u32, new_state: State) bool {
    const local = at(index) orelse return false;
    local.index = index;
    local.apic_id = apic_id;
    @atomicStore(u8, &local.state, @intFromEnum(new_state), .release);
    return true;
}

pub fn setState(index: u32, new_state: State) bool {
    const local = at(index) orelse return false;
    @atomicStore(u8, &local.state, @intFromEnum(new_state), .release);
    return true;
}

pub fn state(index: u32) State {
    const local = at(index) orelse return .absent;
    return @enumFromInt(@atomicLoad(u8, &local.state, .acquire));
}

pub fn apicId(index: u32) ?u32 {
    const local = at(index) orelse return null;
    return local.apic_id;
}

pub fn setSchedulerReady(index: u32, ready: bool) void {
    const local = at(index) orelse return;
    @atomicStore(u8, &local.scheduler_ready, @intFromBool(ready), .release);
}

pub fn schedulerReady(index: u32) bool {
    const local = at(index) orelse return false;
    return @atomicLoad(u8, &local.scheduler_ready, .acquire) != 0;
}

pub fn setWorkActive(index: u32, active: bool) void {
    const local = at(index) orelse return;
    @atomicStore(u8, &local.work_active, @intFromBool(active), .release);
}

pub fn workActive(index: u32) bool {
    const local = at(index) orelse return false;
    return @atomicLoad(u8, &local.work_active, .acquire) != 0;
}

pub fn setSchedulable(index: u32, enabled: bool) void {
    if (index >= max_cpus) return;
    const mask = @as(u64, 1) << @intCast(index);
    if (enabled) {
        _ = @atomicRmw(u64, &schedulable_mask, .Or, mask, .acq_rel);
    } else {
        _ = @atomicRmw(u64, &schedulable_mask, .And, ~mask, .acq_rel);
    }
}

pub fn isSchedulable(index: u32) bool {
    if (index >= max_cpus) return false;
    const mask = @as(u64, 1) << @intCast(index);
    return (@atomicLoad(u64, &schedulable_mask, .acquire) & mask) != 0;
}

pub fn schedulableMask() u64 {
    return @atomicLoad(u64, &schedulable_mask, .acquire);
}

pub fn schedulableCount() u32 {
    return @popCount(schedulableMask());
}

pub fn noteProductive(index: u32, r4x_work: bool) bool {
    if (index >= max_cpus) return false;
    const mask = @as(u64, 1) << @intCast(index);
    const target = if (r4x_work) &productive_r4x_mask else &productive_kernel_mask;
    const previous = @atomicRmw(u64, target, .Or, mask, .acq_rel);
    return (previous & mask) == 0;
}

pub fn productiveMask() u64 {
    return @atomicLoad(u64, &productive_kernel_mask, .acquire) |
        @atomicLoad(u64, &productive_r4x_mask, .acquire);
}

pub fn productiveKernelMask() u64 {
    return @atomicLoad(u64, &productive_kernel_mask, .acquire);
}

pub fn productiveR4xMask() u64 {
    return @atomicLoad(u64, &productive_r4x_mask, .acquire);
}

pub fn legacyCriticalDepth() *u32 {
    return &current().legacy_critical_depth;
}
