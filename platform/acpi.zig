const boot_info = @import("../bootloader/boot_info.zig");
const bootlog = @import("../kernel/bootlog.zig");
const phys = @import("../memory/phys.zig");
const paging = @import("../memory/paging.zig");

const RSDP_SIG = "RSD PTR ";
const MCFG_SIG = "MCFG";
const MADT_SIG = "APIC";
const FADT_SIG = "FACP";
const HPET_SIG = "HPET";
const DSDT_SIG = "DSDT";
const MAX_SDT_LENGTH: u32 = 1024 * 1024;

const AmlPkgLength = struct {
    length: u32 = 0,
    next: usize = 0,
};

const AmlInteger = struct {
    value: u64 = 0,
    next: usize = 0,
};

const Rsdp = extern struct {
    signature: [8]u8,
    checksum: u8,
    oem_id: [6]u8,
    revision: u8,
    rsdt_address: u32,
    length: u32,
    xsdt_address: u64,
    extended_checksum: u8,
    reserved: [3]u8,
};

const SdtHeader = extern struct {
    signature: [4]u8,
    length: u32,
    revision: u8,
    checksum: u8,
    oem_id: [6]u8,
    oem_table_id: [8]u8,
    oem_revision: u32,
    creator_id: u32,
    creator_revision: u32,
};

const McfgEntry = extern struct {
    base_address: u64,
    pci_segment_group: u16,
    start_bus: u8,
    end_bus: u8,
    reserved: u32,
};

pub const MAX_MADT_IOAPICS: usize = 4;
pub const MAX_MADT_ISO: usize = 16;
pub const MAX_MADT_CPUS: usize = 32;

pub const MadtCpu = struct {
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

pub const MadtIoApic = struct {
    id: u8 = 0,
    address: u32 = 0,
    gsi_base: u32 = 0,
};

pub const InterruptSourceOverride = struct {
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

var last_info: Info = .{};

pub fn inspect() Info {
    last_info = .{};
    bootlog.puts("[ACPI] static reader\r\n");

    const rsdp_phys = findRsdp() orelse {
        bootlog.puts("[ACPI][WARN] RSDP not found\r\n");
        return last_info;
    };
    last_info.found = true;
    last_info.rsdp_phys = rsdp_phys;
    bootlog.puts("[ACPI] RSDP phys=0x");
    bootlog.putHex(rsdp_phys, 16);
    bootlog.puts("\r\n");

    if (!mapPhysRange(rsdp_phys, @sizeOf(Rsdp))) {
        bootlog.puts("[ACPI][WARN] RSDP mapping failed\r\n");
        return last_info;
    }
    const rsdp: *align(1) const Rsdp = @ptrFromInt(phys.physToVirt(rsdp_phys));
    if (!memEq(rsdp.signature[0..8], RSDP_SIG) or checksumBytes(rsdp, 20) != 0) {
        bootlog.puts("[ACPI][WARN] invalid RSDP signature/checksum\r\n");
        return last_info;
    }
    last_info.rsdp_revision = rsdp.revision;
    bootlog.puts("[ACPI] RSDP rev=");
    bootlog.putDec(rsdp.revision);
    bootlog.puts(" rsdt=0x");
    bootlog.putHex(rsdp.rsdt_address, 8);
    bootlog.puts(" xsdt=0x");
    bootlog.putHex(rsdp.xsdt_address, 16);
    bootlog.puts(" len=");
    bootlog.putDec(rsdp.length);
    bootlog.puts("\r\n");
    if (rsdp.revision >= 2 and rsdp.length >= 36 and rsdp.length <= 4096 and mapPhysRange(rsdp_phys, rsdp.length)) {
        last_info.rsdp_extended_valid = checksumBytes(rsdp, rsdp.length) == 0;
    }
    last_info.rsdt_phys = rsdp.rsdt_address;
    if (rsdp.revision >= 2 and rsdp.xsdt_address != 0 and last_info.rsdp_extended_valid) {
        last_info.xsdt_phys = rsdp.xsdt_address;
        inspectXsdt(rsdp.xsdt_address);
    } else if (rsdp.rsdt_address != 0) {
        if (rsdp.revision >= 2 and rsdp.xsdt_address != 0) {
            bootlog.puts("[ACPI][WARN] invalid extended RSDP checksum, using RSDT fallback\r\n");
        }
        inspectRsdt(rsdp.rsdt_address);
    }

    if (last_info.mcfg_phys == 0) bootlog.puts("[ACPI][WARN] MCFG not found\r\n");
    if (last_info.madt_phys == 0) bootlog.puts("[ACPI][WARN] MADT/APIC not found\r\n");
    if (last_info.fadt_phys == 0) bootlog.puts("[ACPI][WARN] FADT/FACP not found\r\n");
    if (last_info.hpet_phys == 0) bootlog.puts("[ACPI][WARN] HPET not found\r\n");
    inspectMcfg();
    inspectMadt();
    inspectFadt();
    inspectS5();
    inspectHpet();
    return last_info;
}

pub fn info() Info {
    return last_info;
}

fn inspectXsdt(xsdt_phys: u64) void {
    const header = mapTable(xsdt_phys) orelse {
        bootlog.puts("[ACPI][WARN] XSDT mapping failed\r\n");
        return;
    };
    if (!validSdt(header, "XSDT")) {
        bootlog.puts("[ACPI][WARN] invalid XSDT\r\n");
        return;
    }
    const count = (header.length - @sizeOf(SdtHeader)) / 8;
    const bytes: [*]const u8 = @ptrCast(header);
    var i: usize = 0;
    while (i < count) : (i += 1) inspectTable(readU64(bytes, @sizeOf(SdtHeader) + i * 8));
}

fn inspectRsdt(rsdt_phys: u32) void {
    const header = mapTable(rsdt_phys) orelse {
        bootlog.puts("[ACPI][WARN] RSDT mapping failed\r\n");
        return;
    };
    if (!validSdt(header, "RSDT")) {
        bootlog.puts("[ACPI][WARN] invalid RSDT\r\n");
        return;
    }
    const count = (header.length - @sizeOf(SdtHeader)) / 4;
    const bytes: [*]const u8 = @ptrCast(header);
    var i: usize = 0;
    while (i < count) : (i += 1) inspectTable(readU32(bytes, @sizeOf(SdtHeader) + i * 4));
}

fn inspectTable(table_phys: u64) void {
    if (table_phys == 0) return;
    const header = mapTable(table_phys) orelse {
        last_info.invalid_table_count += 1;
        bootlog.puts("[ACPI][WARN] table mapping failed phys=0x");
        bootlog.putHex(table_phys, 16);
        bootlog.puts("\r\n");
        return;
    };
    if (!validAnySdt(header)) {
        last_info.invalid_table_count += 1;
        bootlog.puts("[ACPI][WARN] invalid table at phys=0x");
        bootlog.putHex(table_phys, 16);
        bootlog.puts("\r\n");
        return;
    }
    last_info.table_count += 1;
    bootlog.puts("[ACPI] table ");
    bootlog.puts(header.signature[0..4]);
    bootlog.puts(" phys=0x");
    bootlog.putHex(table_phys, 16);
    bootlog.puts(" len=");
    bootlog.putDec(header.length);
    bootlog.puts("\r\n");

    if (sigEq(&header.signature, MCFG_SIG)) last_info.mcfg_phys = table_phys;
    if (sigEq(&header.signature, MADT_SIG)) last_info.madt_phys = table_phys;
    if (sigEq(&header.signature, FADT_SIG)) last_info.fadt_phys = table_phys;
    if (sigEq(&header.signature, HPET_SIG)) last_info.hpet_phys = table_phys;
}

fn inspectMcfg() void {
    if (last_info.mcfg_phys == 0) return;
    const header = mapTable(last_info.mcfg_phys) orelse return;
    if (header.length < @sizeOf(SdtHeader) + 8 + @sizeOf(McfgEntry)) {
        bootlog.puts("[ACPI][WARN] MCFG too small\r\n");
        return;
    }
    const entry: *align(1) const McfgEntry = @ptrFromInt(phys.physToVirt(last_info.mcfg_phys + @sizeOf(SdtHeader) + 8));
    last_info.mcfg_base = entry.base_address;
    last_info.mcfg_segment = entry.pci_segment_group;
    last_info.mcfg_start_bus = entry.start_bus;
    last_info.mcfg_end_bus = entry.end_bus;
    bootlog.puts("[ACPI] MCFG ECAM base=0x");
    bootlog.putHex(entry.base_address, 16);
    bootlog.puts(" segment=");
    bootlog.putDec(entry.pci_segment_group);
    bootlog.puts(" buses=");
    bootlog.putDec(entry.start_bus);
    bootlog.puts("..");
    bootlog.putDec(entry.end_bus);
    bootlog.puts("\r\n");
}

fn inspectMadt() void {
    if (last_info.madt_phys == 0) return;
    const header = mapTable(last_info.madt_phys) orelse return;
    const len: usize = @intCast(header.length);
    const base = @sizeOf(SdtHeader);
    if (len < base + 8) {
        bootlog.puts("[ACPI][WARN] MADT too small\r\n");
        return;
    }
    const bytes: [*]const u8 = @ptrCast(header);
    last_info.madt_lapic_address = readU32(bytes, base);
    last_info.madt_flags = readU32(bytes, base + 4);

    var off: usize = base + 8;
    while (off + 2 <= len) {
        const entry_type = bytes[off];
        const entry_len: usize = bytes[off + 1];
        if (entry_len < 2 or off + entry_len > len) {
            last_info.madt_malformed_entries += 1;
            break;
        }
        switch (entry_type) {
            0 => if (entry_len >= 8) {
                const cpu_index = last_info.madt_lapic_count;
                const flags = readU32(bytes, off + 4);
                storeCpu(cpu_index, .{
                    .acpi_uid = bytes[off + 2],
                    .apic_id = bytes[off + 3],
                    .flags = flags,
                    .x2apic = false,
                });
                last_info.madt_lapic_count += 1;
                if ((flags & 0x1) != 0) last_info.madt_lapic_enabled_count += 1;
            },
            1 => if (entry_len >= 12) {
                const ioapic_index = last_info.madt_ioapic_count;
                last_info.madt_ioapic_count += 1;
                if (ioapic_index < MAX_MADT_IOAPICS) {
                    last_info.madt_ioapics[@intCast(ioapic_index)] = .{
                        .id = bytes[off + 2],
                        .address = readU32(bytes, off + 4),
                        .gsi_base = readU32(bytes, off + 8),
                    };
                }
                if (last_info.madt_ioapic_count == 1) {
                    last_info.madt_first_ioapic_id = bytes[off + 2];
                    last_info.madt_first_ioapic_address = readU32(bytes, off + 4);
                    last_info.madt_first_ioapic_gsi_base = readU32(bytes, off + 8);
                }
            },
            2 => if (entry_len >= 10) {
                const iso_index = last_info.madt_iso_count;
                last_info.madt_iso_count += 1;
                if (iso_index < MAX_MADT_ISO) {
                    last_info.madt_isos[@intCast(iso_index)] = .{
                        .bus = bytes[off + 2],
                        .source = bytes[off + 3],
                        .gsi = readU32(bytes, off + 4),
                        .flags = readU16(bytes, off + 8),
                    };
                }
                if (last_info.madt_iso_count == 1) {
                    last_info.madt_first_iso_source = bytes[off + 3];
                    last_info.madt_first_iso_gsi = readU32(bytes, off + 4);
                    last_info.madt_first_iso_flags = readU16(bytes, off + 8);
                }
            },
            4 => if (entry_len >= 6) {
                last_info.madt_nmi_count += 1;
            },
            5 => if (entry_len >= 12) {
                last_info.madt_lapic_address = readU64(bytes, off + 4);
            },
            9 => if (entry_len >= 16) {
                const cpu_index = last_info.madt_lapic_count;
                const flags = readU32(bytes, off + 8);
                storeCpu(cpu_index, .{
                    .acpi_uid = readU32(bytes, off + 12),
                    .apic_id = readU32(bytes, off + 4),
                    .flags = flags,
                    .x2apic = true,
                });
                last_info.madt_lapic_count += 1;
                if ((flags & 0x1) != 0) last_info.madt_lapic_enabled_count += 1;
                last_info.madt_x2apic_count += 1;
            },
            else => {},
        }
        off += entry_len;
    }

    bootlog.puts("[ACPI] MADT lapic=0x");
    bootlog.putHex(last_info.madt_lapic_address, 16);
    bootlog.puts(" cpus=");
    bootlog.putDec(last_info.madt_lapic_enabled_count);
    bootlog.puts("/");
    bootlog.putDec(last_info.madt_lapic_count);
    bootlog.puts(" ioapic=");
    bootlog.putDec(last_info.madt_ioapic_count);
    bootlog.puts(" iso=");
    bootlog.putDec(last_info.madt_iso_count);
    bootlog.puts(" nmi=");
    bootlog.putDec(last_info.madt_nmi_count);
    if (last_info.madt_lapic_count != 0) {
        const first_cpu = last_info.madt_cpu_entries[0];
        bootlog.puts(" cpu0_apic=");
        bootlog.putDec(first_cpu.apic_id);
        bootlog.puts(if (first_cpu.x2apic) "(x2)" else "(xapic)");
    }
    if (last_info.madt_malformed_entries != 0) bootlog.puts(" malformed=yes");
    bootlog.puts("\r\n");
}

fn storeCpu(index: u32, cpu: MadtCpu) void {
    if (index >= MAX_MADT_CPUS) return;
    last_info.madt_cpu_entries[@intCast(index)] = cpu;
}

fn minCpuCount(value: u32) usize {
    return if (value < MAX_MADT_CPUS) @intCast(value) else MAX_MADT_CPUS;
}

fn inspectFadt() void {
    if (last_info.fadt_phys == 0) return;
    const header = mapTable(last_info.fadt_phys) orelse return;
    const len: usize = @intCast(header.length);
    const bytes: [*]const u8 = @ptrCast(header);
    last_info.fadt_revision = header.revision;
    if (len > 45) last_info.fadt_preferred_pm_profile = bytes[45];
    if (len >= 48) last_info.fadt_sci_int = readU16(bytes, 46);
    if (len >= 52) last_info.fadt_smi_cmd = readU32(bytes, 48);
    if (len > 52) last_info.fadt_acpi_enable = bytes[52];
    if (len > 53) last_info.fadt_acpi_disable = bytes[53];
    if (len >= 68) last_info.fadt_pm1a_cnt_blk = readU32(bytes, 64);
    if (len >= 72) last_info.fadt_pm1b_cnt_blk = readU32(bytes, 68);
    if (len > 108) last_info.fadt_century = bytes[108];
    if (len >= 116) last_info.fadt_flags = readU32(bytes, 112);
    if (len >= 44) last_info.fadt_dsdt_phys = readU32(bytes, 40);
    if (len >= 129) {
        last_info.fadt_reset_address_space = bytes[116];
        last_info.fadt_reset_bit_width = bytes[117];
        last_info.fadt_reset_bit_offset = bytes[118];
        last_info.fadt_reset_access_size = bytes[119];
        last_info.fadt_reset_address = readU64(bytes, 120);
        last_info.fadt_reset_value = bytes[128];
        last_info.fadt_reset_gas_standard = last_info.fadt_reset_bit_width == 8 and
            last_info.fadt_reset_bit_offset == 0 and
            (last_info.fadt_reset_access_size == 0 or last_info.fadt_reset_access_size == 1);
        // Width/offset/access are advisory here for firmware compatibility:
        // the reset register is always an exact byte access. Some real FADTs
        // carry non-standard layout metadata that Windows and Linux tolerate.
        last_info.fadt_reset_gas_valid = last_info.fadt_revision >= 2 and
            last_info.fadt_reset_address_space <= 2;
        last_info.fadt_reset_supported = (last_info.fadt_flags & (1 << 10)) != 0 and
            last_info.fadt_reset_address != 0;
    }
    if (len >= 148) last_info.fadt_x_dsdt_phys = readU64(bytes, 140);

    bootlog.puts("[ACPI] FADT rev=");
    bootlog.putDec(last_info.fadt_revision);
    bootlog.puts(" sci=");
    bootlog.putDec(last_info.fadt_sci_int);
    bootlog.puts(" pm1a_cnt=0x");
    bootlog.putHex(last_info.fadt_pm1a_cnt_blk, 8);
    bootlog.puts(" reset=");
    bootlog.puts(if (last_info.fadt_reset_supported) "yes" else "no");
    if (last_info.fadt_reset_supported) {
        bootlog.puts(" space=");
        bootlog.putDec(last_info.fadt_reset_address_space);
        bootlog.puts(" width=");
        bootlog.putDec(last_info.fadt_reset_bit_width);
        bootlog.puts(" offset=");
        bootlog.putDec(last_info.fadt_reset_bit_offset);
        bootlog.puts(" access=");
        bootlog.putDec(last_info.fadt_reset_access_size);
        bootlog.puts(" addr=0x");
        bootlog.putHex(last_info.fadt_reset_address, 16);
        bootlog.puts(" value=0x");
        bootlog.putHex(last_info.fadt_reset_value, 2);
        bootlog.puts(" path=");
        bootlog.puts(switch (last_info.fadt_reset_address_space) {
            0 => "mmio",
            1 => "io",
            2 => "pci",
            else => "invalid",
        });
        bootlog.puts(if (last_info.fadt_reset_gas_valid) " gas=usable" else " gas=invalid");
        bootlog.puts(if (last_info.fadt_reset_gas_standard) " layout=standard" else " layout=nonstandard-byte");
    }
    bootlog.puts("\r\n");
}

fn inspectS5() void {
    const dsdt_phys = if (last_info.fadt_x_dsdt_phys != 0) last_info.fadt_x_dsdt_phys else last_info.fadt_dsdt_phys;
    if (dsdt_phys == 0) {
        last_info.s5_reason = "FADT has no DSDT pointer";
        return;
    }
    const header = mapTable(dsdt_phys) orelse {
        last_info.s5_reason = "DSDT mapping failed";
        return;
    };
    if (!memEq(header.signature[0..4], DSDT_SIG)) {
        last_info.s5_reason = "DSDT signature mismatch";
        return;
    }
    last_info.s5_dsdt_valid = validAnySdt(header);
    if (!last_info.s5_dsdt_valid) {
        last_info.s5_reason = "DSDT checksum invalid";
        return;
    }
    const bytes: [*]const u8 = @ptrCast(header);
    const len: usize = @intCast(header.length);
    var off: usize = @sizeOf(SdtHeader);
    while (off + 8 < len) : (off += 1) {
        if (bytes[off] != '_' or bytes[off + 1] != 'S' or bytes[off + 2] != '5' or bytes[off + 3] != '_') continue;
        if (!looksLikeNameOpBeforeS5(bytes, off)) continue;
        if (parseS5Package(bytes, len, off + 4)) |s5| {
            last_info.s5_found = true;
            last_info.s5_slp_typa = s5[0];
            last_info.s5_slp_typb = s5[1];
            last_info.s5_reason = "DSDT _S5 package parsed";
            bootlog.puts("[ACPI] S5 typa=");
            bootlog.putDec(last_info.s5_slp_typa);
            bootlog.puts(" typb=");
            bootlog.putDec(last_info.s5_slp_typb);
            bootlog.puts(" source=DSDT\r\n");
            return;
        }
    }
    last_info.s5_reason = "DSDT _S5 package not found";
    bootlog.puts("[ACPI][WARN] S5 package not found in DSDT\r\n");
}

fn looksLikeNameOpBeforeS5(bytes: [*]const u8, off: usize) bool {
    if (off == 0) return false;
    if (bytes[off - 1] == 0x08) return true;
    if (off >= 2 and bytes[off - 2] == 0x08 and bytes[off - 1] == '\\') return true;
    return false;
}

fn parseS5Package(bytes: [*]const u8, len: usize, package_op_off: usize) ?[2]u8 {
    if (package_op_off >= len or bytes[package_op_off] != 0x12) return null;
    const pkg = parsePkgLength(bytes, len, package_op_off + 1) orelse return null;
    if (pkg.next >= len) return null;
    const package_end = @min(len, package_op_off + 1 + @as(usize, pkg.length));
    if (pkg.next >= package_end) return null;
    const element_count = bytes[pkg.next];
    if (element_count < 2) return null;
    var off = pkg.next + 1;
    const typa = parseAmlInteger(bytes, package_end, off) orelse return null;
    off = typa.next;
    const typb = parseAmlInteger(bytes, package_end, off) orelse return null;
    if (typa.value > 7 or typb.value > 7) return null;
    return .{ @intCast(typa.value), @intCast(typb.value) };
}

fn parsePkgLength(bytes: [*]const u8, len: usize, off: usize) ?AmlPkgLength {
    if (off >= len) return null;
    const lead = bytes[off];
    const following = lead >> 6;
    var length: u32 = if (following == 0) (lead & 0x3F) else (lead & 0x0F);
    var shift: u5 = 4;
    var i: usize = 0;
    while (i < following) : (i += 1) {
        const idx = off + 1 + i;
        if (idx >= len) return null;
        length |= @as(u32, bytes[idx]) << shift;
        shift += 8;
    }
    return .{ .length = length, .next = off + 1 + following };
}

fn parseAmlInteger(bytes: [*]const u8, len: usize, off: usize) ?AmlInteger {
    if (off >= len) return null;
    return switch (bytes[off]) {
        0x00 => .{ .value = 0, .next = off + 1 },
        0x01 => .{ .value = 1, .next = off + 1 },
        0x0A => if (off + 1 < len) .{ .value = bytes[off + 1], .next = off + 2 } else null,
        0x0B => if (off + 2 < len) .{ .value = readU16(bytes, off + 1), .next = off + 3 } else null,
        0x0C => if (off + 4 < len) .{ .value = readU32(bytes, off + 1), .next = off + 5 } else null,
        0x0E => if (off + 8 < len) .{ .value = readU64(bytes, off + 1), .next = off + 9 } else null,
        else => null,
    };
}

fn inspectHpet() void {
    if (last_info.hpet_phys == 0) return;
    const header = mapTable(last_info.hpet_phys) orelse return;
    const len: usize = @intCast(header.length);
    if (len < @sizeOf(SdtHeader) + 20) {
        bootlog.puts("[ACPI][WARN] HPET too small\r\n");
        return;
    }
    const bytes: [*]const u8 = @ptrCast(header);
    const id = readU32(bytes, @sizeOf(SdtHeader));
    last_info.hpet_base = readU64(bytes, @sizeOf(SdtHeader) + 8);
    last_info.hpet_number = bytes[@sizeOf(SdtHeader) + 16];
    last_info.hpet_min_tick = readU16(bytes, @sizeOf(SdtHeader) + 17);
    last_info.hpet_legacy_replacement = (id & (1 << 15)) != 0;

    bootlog.puts("[ACPI] HPET base=0x");
    bootlog.putHex(last_info.hpet_base, 16);
    bootlog.puts(" min_tick=");
    bootlog.putDec(last_info.hpet_min_tick);
    bootlog.puts(" legacy=");
    bootlog.puts(if (last_info.hpet_legacy_replacement) "yes" else "no");
    bootlog.puts("\r\n");
}

fn findRsdp() ?u64 {
    if (boot_info.rsdpAddress()) |addr| return addr;
    const entries = boot_info.memoryMap();
    var i: usize = 0;
    while (i < entries.len) : (i += 1) {
        const e = entries[i];
        if (e.kind != .acpi_reclaimable and e.kind != .acpi_nvs) continue;
        if (scanRange(e.base, e.length)) |addr| return addr;
    }
    return null;
}

fn scanRange(base: u64, len: u64) ?u64 {
    if (!mapPhysRange(base, len)) return null;
    var off: u64 = 0;
    while (off + 20 <= len) : (off += 16) {
        const ptr: [*]const u8 = @ptrFromInt(phys.physToVirt(base + off));
        if (memEq(ptr[0..8], RSDP_SIG) and checksum(ptr[0..20]) == 0) return base + off;
    }
    return null;
}

fn mapTable(table_phys: u64) ?*align(1) const SdtHeader {
    if (!mapPhysRange(table_phys, @sizeOf(SdtHeader))) return null;
    const header: *align(1) const SdtHeader = @ptrFromInt(phys.physToVirt(table_phys));
    if (header.length < @sizeOf(SdtHeader) or header.length > MAX_SDT_LENGTH) return header;
    if (!mapPhysRange(table_phys, header.length)) return null;
    return header;
}

fn mapPhysRange(base: u64, len: u64) bool {
    if (len == 0) return true;
    var page = base & ~(paging.PAGE_SIZE - 1);
    const range_end = checkedAdd(base, len) orelse return false;
    const end = alignUpChecked(range_end, paging.PAGE_SIZE) orelse return false;
    while (page < end) : (page += paging.PAGE_SIZE) {
        const virt = phys.physToVirt(page);
        if (!paging.isMapped(virt)) {
            if (!paging.mapPage(virt, page, paging.NO_EXECUTE)) return false;
        }
    }
    return true;
}

fn validSdt(header: *align(1) const SdtHeader, expected: []const u8) bool {
    if (!memEq(header.signature[0..4], expected)) return false;
    return validAnySdt(header);
}

fn validAnySdt(header: *align(1) const SdtHeader) bool {
    if (header.length < @sizeOf(SdtHeader) or header.length > MAX_SDT_LENGTH) return false;
    const bytes: [*]const u8 = @ptrCast(header);
    return checksum(bytes[0..header.length]) == 0;
}

fn checksumBytes(ptr: anytype, len: u32) u8 {
    const bytes: [*]const u8 = @ptrCast(ptr);
    return checksum(bytes[0..len]);
}

fn checksum(bytes: []const u8) u8 {
    var sum: u8 = 0;
    for (bytes) |b| sum +%= b;
    return sum;
}

fn sigEq(sig: *const [4]u8, expected: []const u8) bool {
    return memEq(sig[0..4], expected);
}

fn memEq(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var i: usize = 0;
    while (i < a.len) : (i += 1) {
        if (a[i] != b[i]) return false;
    }
    return true;
}

fn readU16(bytes: [*]const u8, off: usize) u16 {
    return @as(u16, bytes[off]) | (@as(u16, bytes[off + 1]) << 8);
}

fn readU32(bytes: [*]const u8, off: usize) u32 {
    return @as(u32, readU16(bytes, off)) | (@as(u32, readU16(bytes, off + 2)) << 16);
}

fn readU64(bytes: [*]const u8, off: usize) u64 {
    return @as(u64, readU32(bytes, off)) | (@as(u64, readU32(bytes, off + 4)) << 32);
}

fn checkedAdd(a: u64, b: u64) ?u64 {
    if (a > ~@as(u64, 0) - b) return null;
    return a + b;
}

fn alignUpChecked(value: u64, alignment: u64) ?u64 {
    const addend = alignment - 1;
    const rounded = checkedAdd(value, addend) orelse return null;
    return rounded & ~addend;
}
