const interrupts = @import("../arch/x86_64/interrupts.zig");
const boot_status = @import("boot_status.zig");
const boot_perf = @import("boot_perf.zig");
const crash = @import("crash.zig");
const crash_screen = @import("crash_screen.zig");
const timer = @import("timer.zig");
const monotonic = @import("../platform/monotonic.zig");

const PendingFailure = struct {
    active: bool = false,
    phase: crash.BootPhase = .unknown,
    message: crash.FixedText = .{},
};

var current_phase: crash.BootPhase = .entry;
var pending_failure: PendingFailure = .{};

pub fn init() void {
    monotonic.earlyInit();
    current_phase = .entry;
    pending_failure = .{};
    boot_perf.init();
}

pub fn setBootPhase(phase: crash.BootPhase) void {
    current_phase = phase;
    boot_perf.record(phase);
}

pub fn fail(phase: crash.BootPhase, message: []const u8) bool {
    recordFailure(phase, message);
    return false;
}

pub fn recordFailure(phase: crash.BootPhase, message: []const u8) void {
    current_phase = phase;
    pending_failure = .{
        .active = true,
        .phase = phase,
        .message = .{},
    };
    pending_failure.message.set(message);
    boot_status.disableForCrash();
}

pub fn haltPendingOrMessage(phase: crash.BootPhase, fallback_message: []const u8) noreturn {
    if (pending_failure.active) {
        const message = pending_failure.message.slice();
        kernelFatal(pending_failure.phase, message);
    }
    kernelFatal(phase, fallback_message);
}

pub fn kernelFatal(phase: crash.BootPhase, message: []const u8) noreturn {
    current_phase = phase;
    var report = crash.fromKernelFatal(phase, timer.tickCount(), message);
    renderAndHalt(&report);
}

pub fn zigPanic(message: []const u8) noreturn {
    var report = crash.fromZigPanic(current_phase, timer.tickCount(), message);
    renderAndHalt(&report);
}

fn renderAndHalt(report: *const crash.CrashReport) noreturn {
    boot_status.disableForCrash();
    const entry = crash.enterCrashPath();
    if (entry == .reentrant) {
        crash_screen.serialMirror(report);
    } else {
        _ = crash_screen.render(report);
    }
    interrupts.haltForever();
}
