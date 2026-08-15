const hpet = @import("../arch/x86_64/hpet.zig");
const ioapic = @import("../arch/x86_64/ioapic.zig");
const lapic = @import("../arch/x86_64/lapic.zig");
const pic = @import("../arch/x86_64/pic.zig");
const acpi = @import("acpi.zig");
const bootlog = @import("../kernel/bootlog.zig");
const timer_core = @import("../kernel/timer.zig");

pub const InterruptController = enum {
    pic_8259,
    apic_available_pic_active,
    apic_ioapic,
};

pub const TimerBackend = enum {
    pit,
    hpet_available_pit_active,
    lapic_available_pit_active,
    hpet,
    lapic,
};

pub const Status = struct {
    controller: InterruptController = .pic_8259,
    timer: TimerBackend = .pit,
    lapic_available: bool = false,
    lapic_runtime_enabled: bool = false,
    lapic_software_enabled: bool = false,
    ioapic_available: bool = false,
    ioapic_runtime_mapped: bool = false,
    ioapic_routing_active: bool = false,
    ioapic_programmed_routes: u8 = 0,
    hpet_available: bool = false,
    hpet_runtime_mapped: bool = false,
    hpet_enabled: bool = false,
    hpet_frequency_hz: u64 = 0,
    hpet_counter: u64 = 0,
    lapic_count: u32 = 0,
    lapic_enabled_count: u32 = 0,
    x2apic_count: u32 = 0,
    ioapic_count: u32 = 0,
    iso_count: u32 = 0,
    nmi_count: u32 = 0,
    lapic_address: u64 = 0,
    lapic_id: u8 = 0,
    lapic_version: u8 = 0,
    lapic_svr: u32 = 0,
    first_ioapic_id: u8 = 0,
    first_ioapic_reg_id: u8 = 0,
    first_ioapic_version: u8 = 0,
    first_ioapic_redirs: u8 = 0,
    first_ioapic_address: u32 = 0,
    first_ioapic_gsi_base: u32 = 0,
    first_iso_source: u8 = 0,
    first_iso_gsi: u32 = 0,
    first_iso_flags: u16 = 0,
    pit_hz: u32 = 0,
    fallback_reason: []const u8 = "MADT/APIC not active yet",
};

var current: Status = .{};

pub fn initFromAcpi(info: acpi.Info) Status {
    const lapic_status = lapic.status();
    const ioapic_status = ioapic.status();
    const hpet_status = hpet.status();
    current = .{
        .controller = .pic_8259,
        .timer = timerBackendFromCore(),
        .lapic_available = info.madt_lapic_address != 0 and info.madt_lapic_enabled_count > 0,
        .lapic_runtime_enabled = lapic_status.enabled,
        .lapic_software_enabled = lapic_status.software_enabled,
        .ioapic_available = info.madt_ioapic_count > 0,
        .ioapic_runtime_mapped = ioapic_status.mapped,
        .ioapic_routing_active = ioapic_status.routing_active,
        .ioapic_programmed_routes = ioapic_status.programmed_routes,
        .hpet_available = info.hpet_base != 0,
        .hpet_runtime_mapped = hpet_status.mapped,
        .hpet_enabled = hpet_status.enabled,
        .hpet_frequency_hz = hpet_status.frequency_hz,
        .hpet_counter = hpet_status.counter,
        .lapic_count = info.madt_lapic_count,
        .lapic_enabled_count = info.madt_lapic_enabled_count,
        .x2apic_count = info.madt_x2apic_count,
        .ioapic_count = info.madt_ioapic_count,
        .iso_count = info.madt_iso_count,
        .nmi_count = info.madt_nmi_count,
        .lapic_address = info.madt_lapic_address,
        .lapic_id = lapic_status.id,
        .lapic_version = lapic_status.version,
        .lapic_svr = lapic_status.svr,
        .first_ioapic_id = info.madt_first_ioapic_id,
        .first_ioapic_reg_id = ioapic_status.reg_id,
        .first_ioapic_version = ioapic_status.version,
        .first_ioapic_redirs = if (ioapic_status.mapped) ioapic_status.max_redirection_entry + 1 else 0,
        .first_ioapic_address = info.madt_first_ioapic_address,
        .first_ioapic_gsi_base = info.madt_first_ioapic_gsi_base,
        .first_iso_source = info.madt_first_iso_source,
        .first_iso_gsi = info.madt_first_iso_gsi,
        .first_iso_flags = info.madt_first_iso_flags,
        .pit_hz = timer_core.frequency(),
        .fallback_reason = fallbackReason(info),
    };
    if (current.lapic_runtime_enabled and current.lapic_software_enabled and current.ioapic_routing_active) {
        current.controller = .apic_ioapic;
    } else if (current.lapic_available and current.ioapic_available) {
        current.controller = .apic_available_pic_active;
    }
    if (timer_core.activeBackend() == .pit and current.hpet_available) {
        current.timer = .hpet_available_pit_active;
    } else if (timer_core.activeBackend() == .pit and current.lapic_runtime_enabled and current.lapic_software_enabled) {
        current.timer = .lapic_available_pit_active;
    }
    logStatus();
    return current;
}

pub fn status() Status {
    return current;
}

fn logStatus() void {
    bootlog.puts("[IRQ] controller=");
    bootlog.puts(controllerName(current.controller));
    bootlog.puts(" timer=");
    bootlog.puts(timerName(current.timer));
    bootlog.puts(" lapic_runtime=");
    bootlog.puts(if (current.lapic_runtime_enabled and current.lapic_software_enabled) "on" else "off");
    bootlog.puts(" lapic=");
    bootlog.putDec(current.lapic_enabled_count);
    bootlog.puts("/");
    bootlog.putDec(current.lapic_count);
    bootlog.puts(" ioapic=");
    bootlog.putDec(current.ioapic_count);
    bootlog.puts(" ioapic_runtime=");
    bootlog.puts(if (current.ioapic_runtime_mapped) "mapped" else "off");
    bootlog.puts(" ioapic_routing=");
    bootlog.puts(if (current.ioapic_routing_active) "active" else "off");
    bootlog.puts(" iso=");
    bootlog.putDec(current.iso_count);
    bootlog.puts(" hpet_runtime=");
    bootlog.puts(if (current.hpet_runtime_mapped) "mapped" else "off");
    bootlog.puts(" fallback=");
    bootlog.puts(current.fallback_reason);
    bootlog.puts("\r\n");
}

fn fallbackReason(info: acpi.Info) []const u8 {
    if (info.madt_phys == 0) return "MADT missing, PIC/PIT fallback";
    if (info.madt_lapic_address == 0 or info.madt_lapic_enabled_count == 0) return "no enabled LAPIC, PIC/PIT fallback";
    if (info.madt_ioapic_count == 0) return "IOAPIC missing, PIC/PIT fallback";
    if (ioapic.isRoutingActive()) return "IOAPIC routes active, 8259-PIC masked";
    return "APIC/IOAPIC parsed, routing not enabled yet";
}

pub fn controllerName(controller: InterruptController) []const u8 {
    return switch (controller) {
        .pic_8259 => "8259-PIC",
        .apic_available_pic_active => "8259-PIC (APIC available)",
        .apic_ioapic => "APIC/IOAPIC",
    };
}

pub fn timerName(timer: TimerBackend) []const u8 {
    return switch (timer) {
        .pit => "PIT",
        .hpet_available_pit_active => "PIT (HPET available)",
        .lapic_available_pit_active => "PIT (LAPIC available)",
        .hpet => "HPET",
        .lapic => "LAPIC timer",
    };
}

fn timerBackendFromCore() TimerBackend {
    return switch (timer_core.activeBackend()) {
        .pit => .pit,
        .hpet => .hpet,
        .lapic => .lapic,
    };
}
