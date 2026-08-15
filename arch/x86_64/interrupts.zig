const io = @import("io.zig");

const RFLAGS_IF: u64 = 1 << 9;

pub fn disable() void {
    io.cli();
}

pub fn enable() void {
    io.sti();
}

pub fn saveAndDisable() u64 {
    const flags = io.readRflags();
    io.cli();
    return flags;
}

pub fn restore(flags: u64) void {
    if ((flags & RFLAGS_IF) != 0) {
        io.sti();
    } else {
        io.cli();
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
