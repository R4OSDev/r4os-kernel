// Rust owns ACPI parsing and resident state; this is presentation and C transport.
const std = @import("std");
pub const MAX_MADT_IOAPICS: usize = 4;
pub const MAX_MADT_ISO: usize = 16;
pub const MAX_MADT_CPUS: usize = 32;

pub const MadtCpu = extern struct {
    acpi_uid: u32 = 0,
    apic_id: u32 = 0,
    flags: u32 = 0,
    x2apic: bool = false,

    pub fn enabled(self: MadtCpu) bool {
        return (self.flags & 0x1) != 0;
    }

    pub fn onlineCapable(self: MadtCpu) bool {
        return (self.flags & 0x2) != 0;
    }
};

pub const MadtIoApic = extern struct {
    id: u8 = 0,
    address: u32 = 0,
    gsi_base: u32 = 0,
};

pub const InterruptSourceOverride = extern struct {
    bus: u8 = 0,
    source: u8 = 0,
    gsi: u32 = 0,
    flags: u16 = 0,
};

pub const Info = struct {
    found: bool = false,
    rsdp_phys: u64 = 0,
    rsdp_revision: u8 = 0,
    rsdp_extended_valid: bool = false,
    xsdt_phys: u64 = 0,
    rsdt_phys: u32 = 0,
    table_count: u32 = 0,
    invalid_table_count: u32 = 0,
    mcfg_phys: u64 = 0,
    madt_phys: u64 = 0,
    fadt_phys: u64 = 0,
    hpet_phys: u64 = 0,
    mcfg_base: u64 = 0,
    mcfg_segment: u16 = 0,
    mcfg_start_bus: u8 = 0,
    mcfg_end_bus: u8 = 0,
    madt_lapic_address: u64 = 0,
    madt_flags: u32 = 0,
    madt_lapic_count: u32 = 0,
    madt_lapic_enabled_count: u32 = 0,
    madt_x2apic_count: u32 = 0,
    madt_ioapic_count: u32 = 0,
    madt_iso_count: u32 = 0,
    madt_nmi_count: u32 = 0,
    madt_malformed_entries: u32 = 0,
    madt_cpu_entries: [MAX_MADT_CPUS]MadtCpu = .{MadtCpu{}} ** MAX_MADT_CPUS,
    madt_first_ioapic_id: u8 = 0,
    madt_first_ioapic_address: u32 = 0,
    madt_first_ioapic_gsi_base: u32 = 0,
    madt_first_iso_source: u8 = 0,
    madt_first_iso_gsi: u32 = 0,
    madt_first_iso_flags: u16 = 0,
    madt_ioapics: [MAX_MADT_IOAPICS]MadtIoApic = .{MadtIoApic{}} ** MAX_MADT_IOAPICS,
    madt_isos: [MAX_MADT_ISO]InterruptSourceOverride = .{InterruptSourceOverride{}} ** MAX_MADT_ISO,
    fadt_revision: u8 = 0,
    fadt_preferred_pm_profile: u8 = 0,
    fadt_sci_int: u16 = 0,
    fadt_smi_cmd: u32 = 0,
    fadt_acpi_enable: u8 = 0,
    fadt_acpi_disable: u8 = 0,
    fadt_pm1a_cnt_blk: u32 = 0,
    fadt_pm1b_cnt_blk: u32 = 0,
    fadt_flags: u32 = 0,
    fadt_century: u8 = 0,
    fadt_dsdt_phys: u64 = 0,
    fadt_x_dsdt_phys: u64 = 0,
    fadt_reset_supported: bool = false,
    fadt_reset_address_space: u8 = 0,
    fadt_reset_bit_width: u8 = 0,
    fadt_reset_bit_offset: u8 = 0,
    fadt_reset_access_size: u8 = 0,
    fadt_reset_address: u64 = 0,
    fadt_reset_value: u8 = 0,
    fadt_reset_gas_valid: bool = false,
    fadt_reset_gas_standard: bool = false,
    s5_found: bool = false,
    s5_slp_typa: u8 = 0,
    s5_slp_typb: u8 = 0,
    s5_dsdt_valid: bool = false,
    s5_reason: []const u8 = "not inspected",
    hpet_base: u64 = 0,
    hpet_number: u8 = 0,
    hpet_min_tick: u16 = 0,
    hpet_legacy_replacement: bool = false,
};

pub const RawInfo = extern struct {
    found: bool = false,
    rsdp_phys: u64 = 0,
    rsdp_revision: u8 = 0,
    rsdp_extended_valid: bool = false,
    xsdt_phys: u64 = 0,
    rsdt_phys: u32 = 0,
    table_count: u32 = 0,
    invalid_table_count: u32 = 0,
    mcfg_phys: u64 = 0,
    madt_phys: u64 = 0,
    fadt_phys: u64 = 0,
    hpet_phys: u64 = 0,
    mcfg_base: u64 = 0,
    mcfg_segment: u16 = 0,
    mcfg_start_bus: u8 = 0,
    mcfg_end_bus: u8 = 0,
    madt_lapic_address: u64 = 0,
    madt_flags: u32 = 0,
    madt_lapic_count: u32 = 0,
    madt_lapic_enabled_count: u32 = 0,
    madt_x2apic_count: u32 = 0,
    madt_ioapic_count: u32 = 0,
    madt_iso_count: u32 = 0,
    madt_nmi_count: u32 = 0,
    madt_malformed_entries: u32 = 0,
    madt_cpu_entries: [MAX_MADT_CPUS]MadtCpu = .{MadtCpu{}} ** MAX_MADT_CPUS,
    madt_first_ioapic_id: u8 = 0,
    madt_first_ioapic_address: u32 = 0,
    madt_first_ioapic_gsi_base: u32 = 0,
    madt_first_iso_source: u8 = 0,
    madt_first_iso_gsi: u32 = 0,
    madt_first_iso_flags: u16 = 0,
    madt_ioapics: [MAX_MADT_IOAPICS]MadtIoApic = .{MadtIoApic{}} ** MAX_MADT_IOAPICS,
    madt_isos: [MAX_MADT_ISO]InterruptSourceOverride = .{InterruptSourceOverride{}} ** MAX_MADT_ISO,
    fadt_revision: u8 = 0,
    fadt_preferred_pm_profile: u8 = 0,
    fadt_sci_int: u16 = 0,
    fadt_smi_cmd: u32 = 0,
    fadt_acpi_enable: u8 = 0,
    fadt_acpi_disable: u8 = 0,
    fadt_pm1a_cnt_blk: u32 = 0,
    fadt_pm1b_cnt_blk: u32 = 0,
    fadt_flags: u32 = 0,
    fadt_century: u8 = 0,
    fadt_dsdt_phys: u64 = 0,
    fadt_x_dsdt_phys: u64 = 0,
    fadt_reset_supported: bool = false,
    fadt_reset_address_space: u8 = 0,
    fadt_reset_bit_width: u8 = 0,
    fadt_reset_bit_offset: u8 = 0,
    fadt_reset_access_size: u8 = 0,
    fadt_reset_address: u64 = 0,
    fadt_reset_value: u8 = 0,
    fadt_reset_gas_valid: bool = false,
    fadt_reset_gas_standard: bool = false,
    s5_found: bool = false,
    s5_slp_typa: u8 = 0,
    s5_slp_typb: u8 = 0,
    s5_dsdt_valid: bool = false,
    s5_reason: u8 = 0,
    hpet_base: u64 = 0,
    hpet_number: u8 = 0,
    hpet_min_tick: u16 = 0,
    hpet_legacy_replacement: bool = false,
};

extern fn r4os_rust_acpi_inspect(output: *RawInfo) void;
extern fn r4os_rust_acpi_info(output: *RawInfo) void;
pub const firmware_tables = @import("firmware_tables.zig").ResidentCatalog;
pub fn project(raw: RawInfo) Info {
    var value: Info = .{};
    inline for (std.meta.fields(Info)) |field| {
        if (comptime std.mem.eql(u8, field.name, "s5_reason")) {
            value.s5_reason = switch (raw.s5_reason) {
                0 => "not inspected",
                1 => "FADT has no DSDT pointer",
                2 => "DSDT mapping failed",
                3 => "DSDT signature mismatch",
                4 => "DSDT checksum invalid",
                5 => "DSDT _S5 package parsed",
                6 => "DSDT _S5 package not found",
                else => unreachable,
            };
        } else @field(value, field.name) = @field(raw, field.name);
    }
    return value;
}
pub fn encode(value: Info) RawInfo {
    var output: RawInfo = .{};
    inline for (std.meta.fields(RawInfo)) |field| {
        if (comptime std.mem.eql(u8, field.name, "s5_reason")) {
            const names = [_][]const u8{ "not inspected", "FADT has no DSDT pointer", "DSDT mapping failed", "DSDT signature mismatch", "DSDT checksum invalid", "DSDT _S5 package parsed", "DSDT _S5 package not found" };
            for (names, 0..) |name, index| {
                if (std.mem.eql(u8, value.s5_reason, name)) {
                    output.s5_reason = @intCast(index);
                    break;
                }
            }
        } else @field(output, field.name) = @field(value, field.name);
    }
    return output;
}
pub fn inspect() Info {
    var raw: RawInfo = undefined;
    r4os_rust_acpi_inspect(&raw);
    return project(raw);
}
pub fn info() Info {
    var raw: RawInfo = undefined;
    r4os_rust_acpi_info(&raw);
    return project(raw);
}
comptime {
    if (@sizeOf(MadtCpu) != 16 or @sizeOf(MadtIoApic) != 12 or @sizeOf(InterruptSourceOverride) != 12 or
        @sizeOf(RawInfo) != 1000 or @offsetOf(RawInfo, "madt_cpu_entries") != 136 or
        @offsetOf(RawInfo, "s5_reason") != 983 or @offsetOf(RawInfo, "hpet_base") != 984)
        @compileError("Rust ACPI snapshot layout differs");
}
