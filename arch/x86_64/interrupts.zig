const io = @import("io.zig");
const percpu = @import("percpu.zig");

const RFLAGS_IF: u64 = 1 << 9;

pub const RuntimeStats = struct {
    acquisitions: u64 = 0,
    nested_acquisitions: u64 = 0,
    collisions: u64 = 0,
    cpu_collisions: u64 = 0,
    wait_spins: u64 = 0,
    max_wait_spins: u64 = 0,
    hold_cycles: u64 = 0,
    max_hold_cycles: u64 = 0,
    legacy_global_acquisitions: u64 = 0,
};

// Scheduler queues, task state, wait queues and IRQ-side preemption form one
// runtime projection.  The context-switch assembly releases this owner only
// after the outgoing RSP is durable.  Every unrelated mutable subsystem uses
// an explicit owner lock and never enters this boundary.
var runtime_serialization_enabled: bool = false;
var runtime_serialization_lock: u8 = 0;
var runtime_owner_cpu_plus_one: u8 = 0;
var runtime_acquired_tsc: [percpu.max_cpus]u64 = .{0} ** percpu.max_cpus;
var runtime_acquisitions: u64 = 0;
var runtime_nested_acquisitions: u64 = 0;
var runtime_collisions: u64 = 0;
var runtime_cpu_collisions: u64 = 0;
var runtime_wait_spins: u64 = 0;
var runtime_max_wait_spins: u64 = 0;
var runtime_hold_cycles: u64 = 0;
var runtime_max_hold_cycles: u64 = 0;

pub fn disable() void {
    io.cli();
}

pub fn enable() void {
    io.sti();
}

pub fn saveAndDisableLocal() u64 {
    const flags = io.readRflags();
    io.cli();
    return flags;
}

pub fn restoreLocal(flags: u64) void {
    if ((flags & RFLAGS_IF) != 0) io.sti() else io.cli();
}

pub fn saveAndDisableRuntime() u64 {
    const flags = io.readRflags();
    io.cli();
    acquireRuntimeSerialization(flags);
    return flags;
}

pub fn restore(flags: u64) void {
    releaseRuntimeSerialization();
    restoreLocal(flags);
}

pub fn enableRuntimeSerialization() void {
    runtime_acquired_tsc = .{0} ** percpu.max_cpus;
    @atomicStore(u64, &runtime_acquisitions, 0, .monotonic);
    @atomicStore(u64, &runtime_nested_acquisitions, 0, .monotonic);
    @atomicStore(u64, &runtime_collisions, 0, .monotonic);
    @atomicStore(u64, &runtime_cpu_collisions, 0, .monotonic);
    @atomicStore(u64, &runtime_wait_spins, 0, .monotonic);
    @atomicStore(u64, &runtime_max_wait_spins, 0, .monotonic);
    @atomicStore(u64, &runtime_hold_cycles, 0, .monotonic);
    @atomicStore(u64, &runtime_max_hold_cycles, 0, .monotonic);
    @atomicStore(u8, &runtime_owner_cpu_plus_one, 0, .monotonic);
    @atomicStore(bool, &runtime_serialization_enabled, true, .release);
}

pub fn runtimeSerializationEnabled() bool {
    return @atomicLoad(bool, &runtime_serialization_enabled, .acquire);
}

pub fn inRuntimeCriticalSection() bool {
    return runtimeSerializationEnabled() and percpu.runtimeCriticalDepth().* != 0;
}

// A context switch must never lend the runtime owner lock to an unrelated
// task.  Scheduler transitions finish their state projection with IF=0,
// release the outermost token here, and then switch stacks.  The resumed task
// restores only its saved IF state.
pub fn releaseRuntimeForContextSwitch() bool {
    if (!runtimeSerializationEnabled()) return true;
    const depth = percpu.runtimeCriticalDepth();
    if (depth.* != 1) return false;
    depth.* = 0;
    finishRuntimeOuter(percpu.currentIndex());
    @atomicStore(u8, &runtime_serialization_lock, 0, .release);
    return true;
}

fn acquireRuntimeSerialization(original_flags: u64) void {
    if (!runtimeSerializationEnabled()) return;
    const cpu_index = percpu.currentIndex();
    const slot: usize = @intCast(cpu_index);
    const depth = percpu.runtimeCriticalDepth();
    _ = @atomicRmw(u64, &runtime_acquisitions, .Add, 1, .monotonic);
    if (depth.* != 0) {
        depth.* +|= 1;
        _ = @atomicRmw(u64, &runtime_nested_acquisitions, .Add, 1, .monotonic);
        return;
    }
    var spins: u64 = 0;
    var collided = false;
    var cpu_collision = false;
    while (@cmpxchgWeak(u8, &runtime_serialization_lock, 0, 1, .acquire, .monotonic)) |_| {
        collided = true;
        spins +|= 1;
        const owner = @atomicLoad(u8, &runtime_owner_cpu_plus_one, .acquire);
        if (owner != 0 and owner != @as(u8, @intCast(cpu_index + 1))) cpu_collision = true;
        // A task waiting for the runtime projection must still be able to
        // receive the higher-priority TLB IPI.  Kernel code is not generally
        // preemptible here; preserve deliberately disabled callers.
        if ((original_flags & RFLAGS_IF) != 0) {
            io.sti();
            asm volatile ("pause");
            io.cli();
        } else {
            asm volatile ("pause");
        }
    }
    if (collided) _ = @atomicRmw(u64, &runtime_collisions, .Add, 1, .monotonic);
    if (cpu_collision) _ = @atomicRmw(u64, &runtime_cpu_collisions, .Add, 1, .monotonic);
    _ = @atomicRmw(u64, &runtime_wait_spins, .Add, spins, .monotonic);
    _ = @atomicRmw(u64, &runtime_max_wait_spins, .Max, spins, .monotonic);
    @atomicStore(u8, &runtime_owner_cpu_plus_one, @intCast(cpu_index + 1), .release);
    runtime_acquired_tsc[slot] = readTsc();
    depth.* = 1;
}

fn releaseRuntimeSerialization() void {
    if (!runtimeSerializationEnabled()) return;
    const depth = percpu.runtimeCriticalDepth();
    if (depth.* == 0) return;
    depth.* -= 1;
    if (depth.* == 0) {
        finishRuntimeOuter(percpu.currentIndex());
        @atomicStore(u8, &runtime_serialization_lock, 0, .release);
    }
}

fn finishRuntimeOuter(cpu_index: u32) void {
    const slot: usize = @intCast(cpu_index);
    const cycles = readTsc() -% runtime_acquired_tsc[slot];
    _ = @atomicRmw(u64, &runtime_hold_cycles, .Add, cycles, .monotonic);
    _ = @atomicRmw(u64, &runtime_max_hold_cycles, .Max, cycles, .monotonic);
    @atomicStore(u8, &runtime_owner_cpu_plus_one, 0, .release);
}

pub fn runtimeStats() RuntimeStats {
    return .{
        .acquisitions = @atomicLoad(u64, &runtime_acquisitions, .monotonic),
        .nested_acquisitions = @atomicLoad(u64, &runtime_nested_acquisitions, .monotonic),
        .collisions = @atomicLoad(u64, &runtime_collisions, .monotonic),
        .cpu_collisions = @atomicLoad(u64, &runtime_cpu_collisions, .monotonic),
        .wait_spins = @atomicLoad(u64, &runtime_wait_spins, .monotonic),
        .max_wait_spins = @atomicLoad(u64, &runtime_max_wait_spins, .monotonic),
        .hold_cycles = @atomicLoad(u64, &runtime_hold_cycles, .monotonic),
        .max_hold_cycles = @atomicLoad(u64, &runtime_max_hold_cycles, .monotonic),
        .legacy_global_acquisitions = 0,
    };
}

fn readTsc() u64 {
    var low: u32 = undefined;
    var high: u32 = undefined;
    asm volatile ("rdtsc"
        : [low] "={eax}" (low),
          [high] "={edx}" (high),
    );
    return (@as(u64, high) << 32) | low;
}

pub fn wereEnabled(flags: u64) bool {
    return (flags & RFLAGS_IF) != 0;
}

pub fn haltForever() noreturn {
    disable();
    while (true) {
        io.hlt();
    }
}

pub fn waitForInterrupt() void {
    io.hlt();
}

// `sti; hlt` must remain one architectural sequence.  Calling the two
// instructions through separate functions would reopen the classic lost-wake
// window in which an IPI arrives after STI but before HLT.
pub fn enableAndWaitForInterrupt() void {
    asm volatile ("sti; hlt");
}
