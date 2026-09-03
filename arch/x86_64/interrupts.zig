const io = @import("io.zig");
const percpu = @import("percpu.zig");

const RFLAGS_IF: u64 = 1 << 9;

pub const CallerClass = enum(u8) {
    unclassified = 0,
    interrupt = 1,
    scheduler = 2,
    synchronization = 3,
    task = 4,
    logging = 5,
    driver = 6,
    memory = 7,
    storage = 8,
    network = 9,
    program = 10,
    @"test" = 11,
};

pub const caller_class_count: usize = 12;

pub const LegacyStats = struct {
    acquisitions: u64 = 0,
    nested_acquisitions: u64 = 0,
    collisions: u64 = 0,
    cpu_collisions: u64 = 0,
    wait_spins: u64 = 0,
    max_wait_spins: u64 = 0,
    hold_cycles: u64 = 0,
    max_hold_cycles: u64 = 0,
    unclassified_acquisitions: u64 = 0,
    class_acquisitions: [caller_class_count]u64 = .{0} ** caller_class_count,
};

// Transitional SMP owner lock.  Existing R4OS owners were designed around
// "interrupts disabled" as their serialization boundary.  Once APs exist,
// local CLI alone is insufficient, so saveAndDisable/restore additionally
// enter this reentrant cross-CPU boundary.  Explicitly audited owners may be
// split into finer locks later without weakening the initial SMP contract.
var legacy_serialization_enabled: bool = false;
var legacy_serialization_lock: u8 = 0;
var legacy_owner_cpu_plus_one: u8 = 0;
var legacy_acquired_tsc: [percpu.max_cpus]u64 = .{0} ** percpu.max_cpus;
var legacy_outer_class: [percpu.max_cpus]u8 = .{0} ** percpu.max_cpus;
var legacy_acquisitions: u64 = 0;
var legacy_nested_acquisitions: u64 = 0;
var legacy_collisions: u64 = 0;
var legacy_cpu_collisions: u64 = 0;
var legacy_wait_spins: u64 = 0;
var legacy_max_wait_spins: u64 = 0;
var legacy_hold_cycles: u64 = 0;
var legacy_max_hold_cycles: u64 = 0;
var legacy_class_acquisitions: [caller_class_count]u64 = .{0} ** caller_class_count;

pub fn disable() void {
    io.cli();
}

pub fn enable() void {
    io.sti();
}

pub fn saveAndDisableFor(class: CallerClass) u64 {
    const flags = io.readRflags();
    io.cli();
    acquireLegacySerialization(class, flags);
    return flags;
}

pub fn restore(flags: u64) void {
    releaseLegacySerialization();
    if ((flags & RFLAGS_IF) != 0) {
        io.sti();
    } else {
        io.cli();
    }
}

pub fn enableLegacySerialization() void {
    legacy_acquired_tsc = .{0} ** percpu.max_cpus;
    legacy_outer_class = .{0} ** percpu.max_cpus;
    legacy_class_acquisitions = .{0} ** caller_class_count;
    @atomicStore(u64, &legacy_acquisitions, 0, .monotonic);
    @atomicStore(u64, &legacy_nested_acquisitions, 0, .monotonic);
    @atomicStore(u64, &legacy_collisions, 0, .monotonic);
    @atomicStore(u64, &legacy_cpu_collisions, 0, .monotonic);
    @atomicStore(u64, &legacy_wait_spins, 0, .monotonic);
    @atomicStore(u64, &legacy_max_wait_spins, 0, .monotonic);
    @atomicStore(u64, &legacy_hold_cycles, 0, .monotonic);
    @atomicStore(u64, &legacy_max_hold_cycles, 0, .monotonic);
    @atomicStore(u8, &legacy_owner_cpu_plus_one, 0, .monotonic);
    @atomicStore(bool, &legacy_serialization_enabled, true, .release);
}

pub fn legacySerializationEnabled() bool {
    return @atomicLoad(bool, &legacy_serialization_enabled, .acquire);
}

pub fn inLegacyCriticalSection() bool {
    return legacySerializationEnabled() and percpu.legacyCriticalDepth().* != 0;
}

// A context switch must never lend the legacy owner lock to an unrelated
// task.  Scheduler transitions finish their state projection with IF=0,
// release the outermost token here, and then switch stacks.  The resumed task
// restores only its saved IF state.
pub fn releaseLegacyForContextSwitch() bool {
    if (!legacySerializationEnabled()) return true;
    const depth = percpu.legacyCriticalDepth();
    if (depth.* != 1) return false;
    depth.* = 0;
    finishLegacyOuter(percpu.currentIndex());
    @atomicStore(u8, &legacy_serialization_lock, 0, .release);
    return true;
}

fn acquireLegacySerialization(class: CallerClass, original_flags: u64) void {
    if (!legacySerializationEnabled()) return;
    const cpu_index = percpu.currentIndex();
    const slot: usize = @intCast(cpu_index);
    const depth = percpu.legacyCriticalDepth();
    _ = @atomicRmw(u64, &legacy_acquisitions, .Add, 1, .monotonic);
    _ = @atomicRmw(u64, &legacy_class_acquisitions[@intFromEnum(class)], .Add, 1, .monotonic);
    if (depth.* != 0) {
        depth.* +|= 1;
        _ = @atomicRmw(u64, &legacy_nested_acquisitions, .Add, 1, .monotonic);
        return;
    }
    var spins: u64 = 0;
    var collided = false;
    var cpu_collision = false;
    while (@cmpxchgWeak(u8, &legacy_serialization_lock, 0, 1, .acquire, .monotonic)) |_| {
        collided = true;
        spins +|= 1;
        const owner = @atomicLoad(u8, &legacy_owner_cpu_plus_one, .acquire);
        if (owner != 0 and owner != @as(u8, @intCast(cpu_index + 1))) cpu_collision = true;
        // A task waiting for the transitional BKL must still be able to
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
    if (collided) _ = @atomicRmw(u64, &legacy_collisions, .Add, 1, .monotonic);
    if (cpu_collision) _ = @atomicRmw(u64, &legacy_cpu_collisions, .Add, 1, .monotonic);
    _ = @atomicRmw(u64, &legacy_wait_spins, .Add, spins, .monotonic);
    _ = @atomicRmw(u64, &legacy_max_wait_spins, .Max, spins, .monotonic);
    @atomicStore(u8, &legacy_owner_cpu_plus_one, @intCast(cpu_index + 1), .release);
    legacy_acquired_tsc[slot] = readTsc();
    legacy_outer_class[slot] = @intFromEnum(class);
    depth.* = 1;
}

fn releaseLegacySerialization() void {
    if (!legacySerializationEnabled()) return;
    const depth = percpu.legacyCriticalDepth();
    if (depth.* == 0) return;
    depth.* -= 1;
    if (depth.* == 0) {
        finishLegacyOuter(percpu.currentIndex());
        @atomicStore(u8, &legacy_serialization_lock, 0, .release);
    }
}

fn finishLegacyOuter(cpu_index: u32) void {
    const slot: usize = @intCast(cpu_index);
    const cycles = readTsc() -% legacy_acquired_tsc[slot];
    _ = @atomicRmw(u64, &legacy_hold_cycles, .Add, cycles, .monotonic);
    _ = @atomicRmw(u64, &legacy_max_hold_cycles, .Max, cycles, .monotonic);
    legacy_outer_class[slot] = 0;
    @atomicStore(u8, &legacy_owner_cpu_plus_one, 0, .release);
}

pub fn legacyStats() LegacyStats {
    var result = LegacyStats{
        .acquisitions = @atomicLoad(u64, &legacy_acquisitions, .monotonic),
        .nested_acquisitions = @atomicLoad(u64, &legacy_nested_acquisitions, .monotonic),
        .collisions = @atomicLoad(u64, &legacy_collisions, .monotonic),
        .cpu_collisions = @atomicLoad(u64, &legacy_cpu_collisions, .monotonic),
        .wait_spins = @atomicLoad(u64, &legacy_wait_spins, .monotonic),
        .max_wait_spins = @atomicLoad(u64, &legacy_max_wait_spins, .monotonic),
        .hold_cycles = @atomicLoad(u64, &legacy_hold_cycles, .monotonic),
        .max_hold_cycles = @atomicLoad(u64, &legacy_max_hold_cycles, .monotonic),
    };
    var index: usize = 0;
    while (index < result.class_acquisitions.len) : (index += 1) {
        result.class_acquisitions[index] = @atomicLoad(u64, &legacy_class_acquisitions[index], .monotonic);
    }
    result.unclassified_acquisitions = result.class_acquisitions[@intFromEnum(CallerClass.unclassified)];
    return result;
}

pub fn callerClassName(class: CallerClass) []const u8 {
    return switch (class) {
        .unclassified => "unclassified",
        .interrupt => "interrupt",
        .scheduler => "scheduler",
        .synchronization => "synchronization",
        .task => "task",
        .logging => "logging",
        .driver => "driver",
        .memory => "memory",
        .storage => "storage",
        .network => "network",
        .program => "program",
        .@"test" => "test",
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
