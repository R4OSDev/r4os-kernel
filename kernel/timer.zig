const config = @import("config");
const lapic = @import("../arch/x86_64/lapic.zig");
const hpet = @import("../arch/x86_64/hpet.zig");
const pit = @import("../arch/x86_64/pit.zig");

pub const Backend = enum {
    pit,
    hpet,
    lapic,
};

pub const PIT_IRQ = pit.IRQ;
pub const DEFAULT_HZ = pit.DEFAULT_HZ;

var backend: Backend = .pit;

pub fn initPit(requested_hz: u32) void {
    pit.init(requested_hz);
    backend = .pit;
}

pub fn trySwitchToLapic(requested_hz: u32) bool {
    if (!lapic.initTimerFromHpet(requested_hz)) return false;
    backend = .lapic;
    return true;
}

pub fn trySwitchToHpet(requested_hz: u32) bool {
    if (!hpet.startLegacyIrqTimer(requested_hz)) return false;
    backend = .hpet;
    return true;
}

pub fn fallbackToPit() void {
    if (backend == .lapic) lapic.stopTimer();
    if (backend == .hpet) hpet.stopLegacyIrqTimer();
    backend = .pit;
}

pub fn onIrq() u64 {
    // 0.56.15: COM1-TX-Ring pro Tick opportunistisch drainen, damit
    // gepufferte Logzeilen auch ohne weitere Ausgaben rausgehen.
    if (comptime config.enable_com1_debug) {
        const com = @import("../driver/com.zig");
        com.logDrain();
    }
    switch (backend) {
        .pit => pit.onTick(),
        .hpet => return hpet.onTimerIrq(),
        .lapic => return lapic.onTimerIrq(),
    }
    return tickCount();
}

pub fn tickCount() u64 {
    return switch (backend) {
        .pit => pit.tickCount(),
        .hpet => hpet.timerTickCount(),
        .lapic => lapic.timerTickCount(),
    };
}

pub fn frequency() u32 {
    return switch (backend) {
        .pit => pit.frequency(),
        .hpet => hpet.timerFrequency(),
        .lapic => lapic.timerFrequency(),
    };
}

pub fn activeBackend() Backend {
    return backend;
}

pub fn backendName() []const u8 {
    return switch (backend) {
        .pit => "PIT",
        .hpet => "HPET",
        .lapic => "LAPIC timer",
    };
}
