// Native cached IRQ diagnostic owner; this facade only projects fixed C data.
const std = @import("std");
const acpi = @import("acpi.zig");
pub const InterruptController = enum(u8) {
    pic_8259,
    apic_available_pic_active,
    apic_ioapic,
};

pub const TimerBackend = enum(u8) {
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

pub const RawStatus = extern struct {
    controller: u8 = 0,
    timer: u8 = 0,
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
    fallback_reason: u8 = 0,
};

extern fn r4os_rust_irq_view_init(input: *const acpi.RawInfo, output: *RawStatus) void;
extern fn r4os_rust_irq_view_status(output: *RawStatus) void;
pub fn project(raw: RawStatus) Status {
    var value: Status = .{};
    inline for (std.meta.fields(Status)) |field| {
        if (comptime std.mem.eql(u8, field.name, "controller")) value.controller = @enumFromInt(raw.controller) else if (comptime std.mem.eql(u8, field.name, "timer")) value.timer = @enumFromInt(raw.timer) else if (comptime std.mem.eql(u8, field.name, "fallback_reason")) value.fallback_reason = switch (raw.fallback_reason) {
            0 => "MADT/APIC not active yet",
            1 => "MADT missing, PIC/PIT fallback",
            2 => "no enabled LAPIC, PIC/PIT fallback",
            3 => "IOAPIC missing, PIC/PIT fallback",
            4 => "IOAPIC routes active, 8259-PIC masked",
            5 => "APIC/IOAPIC parsed, routing not enabled yet",
            else => unreachable,
        } else @field(value, field.name) = @field(raw, field.name);
    }
    return value;
}
pub fn initFromAcpi(info: acpi.Info) Status {
    const input = acpi.encode(info);
    var output: RawStatus = undefined;
    r4os_rust_irq_view_init(&input, &output);
    return project(output);
}
pub fn status() Status {
    var output: RawStatus = undefined;
    r4os_rust_irq_view_status(&output);
    return project(output);
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

comptime {
    if (@sizeOf(RawStatus) != 104 or @offsetOf(RawStatus, "fallback_reason") != 100) @compileError("Rust IRQ view layout differs");
}
