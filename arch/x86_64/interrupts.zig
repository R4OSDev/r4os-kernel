const io = @import("io.zig");
const percpu = @import("percpu.zig");

const RFLAGS_IF: u64 = 1 << 9;

// Transitional SMP owner lock.  Existing R4OS owners were designed around
// "interrupts disabled" as their serialization boundary.  Once APs exist,
// local CLI alone is insufficient, so saveAndDisable/restore additionally
// enter this reentrant cross-CPU boundary.  Explicitly audited owners may be
// split into finer locks later without weakening the initial SMP contract.
var legacy_serialization_enabled: bool = false;
var legacy_serialization_lock: u8 = 0;

pub fn disable() void {
    io.cli();
}

pub fn enable() void {
    io.sti();
}

pub fn saveAndDisable() u64 {
    const flags = io.readRflags();
    io.cli();
    acquireLegacySerialization();
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
    @atomicStore(u8, &legacy_serialization_lock, 0, .release);
    return true;
}

fn acquireLegacySerialization() void {
    if (!legacySerializationEnabled()) return;
    const depth = percpu.legacyCriticalDepth();
    if (depth.* != 0) {
        depth.* +|= 1;
        return;
    }
    while (@cmpxchgWeak(u8, &legacy_serialization_lock, 0, 1, .acquire, .monotonic)) |_| {
        asm volatile ("pause");
    }
    depth.* = 1;
}

fn releaseLegacySerialization() void {
    if (!legacySerializationEnabled()) return;
    const depth = percpu.legacyCriticalDepth();
    if (depth.* == 0) return;
    depth.* -= 1;
    if (depth.* == 0) {
        @atomicStore(u8, &legacy_serialization_lock, 0, .release);
    }
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
