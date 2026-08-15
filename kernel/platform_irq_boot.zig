// APIC, IOAPIC, and HPET foundation for kernel startup.
//
// This layer consumes the prepared ACPI state from platform_boot exactly once,
// configures the modern legacy IRQ route, and hands the timer core from PIT to
// HPET or LAPIC once acceptance sees real ticks. It then emits the final
// IRQ/timer status summary.

const boot_status = @import("boot_status.zig");
const bootlog = @import("bootlog.zig");
const fatal = @import("fatal.zig");
const hpet = @import("../arch/x86_64/hpet.zig");
const interrupts = @import("../arch/x86_64/interrupts.zig");
const ioapic = @import("../arch/x86_64/ioapic.zig");
const irq_status = @import("../platform/irq.zig");
const lapic = @import("../arch/x86_64/lapic.zig");
const pic = @import("../arch/x86_64/pic.zig");
const platform_boot = @import("platform_boot.zig");
const time_core = @import("../platform/time.zig");
const timer = @import("timer.zig");
const k = @import("log.zig");

pub const TimerHandoffStatus = struct {
    attempted: bool = false,
    active_backend: timer.Backend = .pit,
    requested_hz: u32 = timer.DEFAULT_HZ,
    validated_ticks: u64 = 0,
    pit_irq0_route_masked: bool = false,
    fallback_reason: []const u8 = "not initialized",
};

pub const Status = struct {
    initialized: bool = false,
    lapic_status: lapic.Status = .{},
    ioapic_status: ioapic.Status = .{},
    hpet_status: hpet.Status = .{},
    ioapic_routing_active: bool = false,
    hpet_visible: bool = false,
    hpet_initialized: bool = false,
    pic_masked: bool = false,
    pic_master_mask: u8 = 0xFF,
    pic_slave_mask: u8 = 0xFF,
    pic_fallback_reason: []const u8 = "not initialized",
    timer_handoff: TimerHandoffStatus = .{},
    final_irq_status: irq_status.Status = .{},
    status_finalized: bool = false,
    time_status_logged: bool = false,
};

var current: Status = .{};

pub fn init() bool {
    if (current.initialized) return true;

    const acpi_info = platform_boot.acpiInfo() orelse {
        return fail("Platform IRQ boot before platform state");
    };

    current = .{
        .pic_master_mask = pic.masterMask(),
        .pic_slave_mask = pic.slaveMask(),
    };

    current.lapic_status = lapic.initFromAcpi(acpi_info);
    current.ioapic_status = ioapic.initFromAcpi(acpi_info);
    current.hpet_status = hpet.initFromAcpi(acpi_info);
    current.hpet_visible = current.hpet_status.available;
    current.hpet_initialized = current.hpet_status.mapped and current.hpet_status.enabled;

    if (ioapic.activateLegacyRoutes()) {
        pic.maskAll();
        current.ioapic_status = ioapic.status();
        current.ioapic_routing_active = true;
        current.pic_masked = true;
        current.pic_fallback_reason = "not needed; IOAPIC legacy routing active";
        bootlog.puts("[IRQ] IOAPIC legacy routing [OK]\r\n");
        boot_status.statusLine("  Platform IRQ [IOAPIC]\r\n");
    } else {
        current.ioapic_status = ioapic.status();
        current.ioapic_routing_active = false;
        current.pic_masked = false;
        current.pic_fallback_reason = current.ioapic_status.reason;
        boot_status.statusLine("  IOAPIC legacy routing [PIC fallback]\r\n");
    }

    current.pic_master_mask = pic.masterMask();
    current.pic_slave_mask = pic.slaveMask();
    handoffTimer();
    finalizeStatus(acpi_info);
    current.initialized = true;
    logStatus();
    return true;
}

pub fn isInitialized() bool {
    return current.initialized;
}

pub fn status() Status {
    return current;
}

fn fail(message: []const u8) bool {
    return fatal.fail(.irq, message);
}

const TimerAttempt = struct {
    ok: bool = false,
    validated_ticks: u64 = 0,
    reason: []const u8 = "",
};

const TimerValidation = struct {
    ok: bool = false,
    observed_ticks: u64 = 0,
};

fn handoffTimer() void {
    current.timer_handoff = .{
        .attempted = true,
        .active_backend = timer.activeBackend(),
        .requested_hz = timer.DEFAULT_HZ,
        .fallback_reason = "PIT remains active until a modern timer validates",
    };

    const hpet_attempt = activateHpetTimerIfPossible();
    if (hpet_attempt.ok) {
        current.timer_handoff.active_backend = timer.activeBackend();
        current.timer_handoff.validated_ticks = hpet_attempt.validated_ticks;
        current.timer_handoff.fallback_reason = "HPET timer active";
        bootlogTimerStatus("HPET");
        boot_status.statusLine("  Timer [HPET]\r\n");
        return;
    }

    const lapic_attempt = activateLapicTimerIfPossible();
    if (lapic_attempt.ok) {
        current.timer_handoff.active_backend = timer.activeBackend();
        current.timer_handoff.validated_ticks = lapic_attempt.validated_ticks;
        current.timer_handoff.fallback_reason = "LAPIC timer active";
        bootlogTimerStatus("LAPIC");
        boot_status.statusLine("  Timer [LAPIC]\r\n");
        return;
    }

    timer.fallbackToPit();
    current.timer_handoff.active_backend = timer.activeBackend();
    current.timer_handoff.validated_ticks = @max(hpet_attempt.validated_ticks, lapic_attempt.validated_ticks);
    current.timer_handoff.pit_irq0_route_masked = false;
    current.timer_handoff.fallback_reason = if (lapic_attempt.reason.len != 0) lapic_attempt.reason else hpet_attempt.reason;
    boot_status.statusLine("  Modern timer [PIT fallback]\r\n");
}

fn activateHpetTimerIfPossible() TimerAttempt {
    if (!current.ioapic_routing_active) {
        return .{ .reason = "IOAPIC routing inactive; PIT remains active" };
    }
    if (!timer.trySwitchToHpet(timer.DEFAULT_HZ)) {
        current.hpet_status = hpet.status();
        return .{ .reason = current.hpet_status.reason };
    }

    const validation = validateActiveTimerTicks(2);
    if (validation.ok) {
        current.hpet_status = hpet.status();
        return .{ .ok = true, .validated_ticks = validation.observed_ticks };
    }

    timer.fallbackToPit();
    current.hpet_status = hpet.status();
    return .{
        .validated_ticks = validation.observed_ticks,
        .reason = "HPET timer did not deliver validated ticks; PIT restored",
    };
}

fn activateLapicTimerIfPossible() TimerAttempt {
    if (!current.ioapic_routing_active) {
        return .{ .reason = "IOAPIC routing inactive; PIT remains active" };
    }
    if (!timer.trySwitchToLapic(timer.DEFAULT_HZ)) {
        current.lapic_status = lapic.status();
        return .{ .reason = current.lapic_status.timer_reason };
    }
    if (!ioapic.setLegacyIrqMasked(timer.PIT_IRQ, true)) {
        timer.fallbackToPit();
        current.lapic_status = lapic.status();
        current.ioapic_status = ioapic.status();
        return .{ .reason = "IOAPIC PIT IRQ0 route could not be masked; PIT restored" };
    }

    current.timer_handoff.pit_irq0_route_masked = true;
    current.ioapic_status = ioapic.status();
    const validation = validateActiveTimerTicks(2);
    if (validation.ok) {
        current.lapic_status = lapic.status();
        return .{ .ok = true, .validated_ticks = validation.observed_ticks };
    }

    timer.fallbackToPit();
    _ = ioapic.setLegacyIrqMasked(timer.PIT_IRQ, false);
    current.timer_handoff.pit_irq0_route_masked = false;
    current.lapic_status = lapic.status();
    current.ioapic_status = ioapic.status();
    return .{
        .validated_ticks = validation.observed_ticks,
        .reason = "LAPIC timer did not deliver validated ticks; PIT route restored",
    };
}

fn validateActiveTimerTicks(min_ticks: u64) TimerValidation {
    const hpet_status = hpet.status();
    if (!hpet_status.enabled or hpet_status.frequency_hz == 0) return .{};

    const start_hpet = hpet.readMainCounter();
    const deadline = start_hpet + hpet_status.frequency_hz / 20;
    const start_ticks = timer.tickCount();
    interrupts.enable();
    while (hpet.readMainCounter() < deadline and timer.tickCount() < start_ticks + min_ticks) {
        asm volatile ("pause");
    }
    interrupts.disable();

    const observed_ticks = timer.tickCount() - start_ticks;
    return .{
        .ok = observed_ticks >= min_ticks,
        .observed_ticks = observed_ticks,
    };
}

fn bootlogTimerStatus(name: []const u8) void {
    bootlog.puts("[TIMER] ");
    bootlog.puts(name);
    bootlog.puts(" ");
    bootlog.putDec(timer.frequency());
    bootlog.puts(" Hz ticks=");
    bootlog.putDec(current.timer_handoff.validated_ticks);
    bootlog.puts(" [OK]\r\n");
}

fn finalizeStatus(acpi_info: anytype) void {
    current.final_irq_status = irq_status.initFromAcpi(acpi_info);
    current.status_finalized = true;
    time_core.logStatus();
    current.time_status_logged = true;
}

fn logStatus() void {
    bootlog.puts("[IRQBOOT] initialized=");
    bootlog.puts(if (current.initialized) "yes" else "no");
    bootlog.puts(" lapic=");
    bootlog.puts(if (current.lapic_status.enabled and current.lapic_status.software_enabled) "enabled" else "off");
    bootlog.puts(" ioapic_routing=");
    bootlog.puts(if (current.ioapic_routing_active) "active" else "pic-fallback");
    bootlog.puts(" hpet=");
    bootlog.puts(if (current.hpet_initialized) "initialized" else if (current.hpet_visible) "visible" else "missing");
    bootlog.puts(" pic_masked=");
    bootlog.puts(if (current.pic_masked) "yes" else "no");
    bootlog.puts(" pic_master=0x");
    bootlog.putHex(current.pic_master_mask, 2);
    bootlog.puts(" pic_slave=0x");
    bootlog.putHex(current.pic_slave_mask, 2);
    bootlog.puts(" fallback=");
    bootlog.puts(current.pic_fallback_reason);
    bootlog.puts(" timer=");
    bootlog.puts(timer.backendName());
    bootlog.puts(" validated_ticks=");
    bootlog.putDec(current.timer_handoff.validated_ticks);
    bootlog.puts(" pit_irq0_masked=");
    bootlog.puts(if (current.timer_handoff.pit_irq0_route_masked) "yes" else "no");
    bootlog.puts(" timer_reason=");
    bootlog.puts(current.timer_handoff.fallback_reason);
    bootlog.puts(" final_status=");
    bootlog.puts(if (current.status_finalized) "yes" else "no");
    bootlog.puts(" time_logged=");
    bootlog.puts(if (current.time_status_logged) "yes" else "no");
    bootlog.puts("\r\n");
}
