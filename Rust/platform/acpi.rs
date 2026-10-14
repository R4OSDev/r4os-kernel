//! Static ACPI reader. Packed firmware is decoded from bytes, never through
//! aligned Rust references. The BSP publishes one immutable resident snapshot.
use super::firmware_tables;
use crate::{
    boot::info as boot,
    memory::{paging, phys},
};
use core::{cell::UnsafeCell, slice};
const HEADER: usize = 36;
const MAX_SDT_LENGTH: usize = 1024 * 1024;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MadtCpu {
    pub acpi_uid: u32,
    pub apic_id: u32,
    pub flags: u32,
    pub x2apic: bool,
}
impl MadtCpu {
    const EMPTY: Self = Self {
        acpi_uid: 0,
        apic_id: 0,
        flags: 0,
        x2apic: false,
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MadtIoApic {
    pub id: u8,
    pub address: u32,
    pub gsi_base: u32,
}
impl MadtIoApic {
    const EMPTY: Self = Self {
        id: 0,
        address: 0,
        gsi_base: 0,
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct InterruptSourceOverride {
    pub bus: u8,
    pub source: u8,
    pub gsi: u32,
    pub flags: u16,
}
impl InterruptSourceOverride {
    const EMPTY: Self = Self {
        bus: 0,
        source: 0,
        gsi: 0,
        flags: 0,
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Info {
    pub found: bool,
    pub rsdp_phys: u64,
    pub rsdp_revision: u8,
    pub rsdp_extended_valid: bool,
    pub xsdt_phys: u64,
    pub rsdt_phys: u32,
    pub table_count: u32,
    pub invalid_table_count: u32,
    pub mcfg_phys: u64,
    pub madt_phys: u64,
    pub fadt_phys: u64,
    pub hpet_phys: u64,
    pub mcfg_base: u64,
    pub mcfg_segment: u16,
    pub mcfg_start_bus: u8,
    pub mcfg_end_bus: u8,
    pub madt_lapic_address: u64,
    pub madt_flags: u32,
    pub madt_lapic_count: u32,
    pub madt_lapic_enabled_count: u32,
    pub madt_x2apic_count: u32,
    pub madt_ioapic_count: u32,
    pub madt_iso_count: u32,
    pub madt_nmi_count: u32,
    pub madt_malformed_entries: u32,
    pub madt_cpu_entries: [MadtCpu; 32],
    pub madt_first_ioapic_id: u8,
    pub madt_first_ioapic_address: u32,
    pub madt_first_ioapic_gsi_base: u32,
    pub madt_first_iso_source: u8,
    pub madt_first_iso_gsi: u32,
    pub madt_first_iso_flags: u16,
    pub madt_ioapics: [MadtIoApic; 4],
    pub madt_isos: [InterruptSourceOverride; 16],
    pub fadt_revision: u8,
    pub fadt_preferred_pm_profile: u8,
    pub fadt_sci_int: u16,
    pub fadt_smi_cmd: u32,
    pub fadt_acpi_enable: u8,
    pub fadt_acpi_disable: u8,
    pub fadt_pm1a_cnt_blk: u32,
    pub fadt_pm1b_cnt_blk: u32,
    pub fadt_flags: u32,
    pub fadt_century: u8,
    pub fadt_dsdt_phys: u64,
    pub fadt_x_dsdt_phys: u64,
    pub fadt_reset_supported: bool,
    pub fadt_reset_address_space: u8,
    pub fadt_reset_bit_width: u8,
    pub fadt_reset_bit_offset: u8,
    pub fadt_reset_access_size: u8,
    pub fadt_reset_address: u64,
    pub fadt_reset_value: u8,
    pub fadt_reset_gas_valid: bool,
    pub fadt_reset_gas_standard: bool,
    pub s5_found: bool,
    pub s5_slp_typa: u8,
    pub s5_slp_typb: u8,
    pub s5_dsdt_valid: bool,
    pub s5_reason: u8,
    pub hpet_base: u64,
    pub hpet_number: u8,
    pub hpet_min_tick: u16,
    pub hpet_legacy_replacement: bool,
}
impl Info {
    pub const EMPTY: Self = Self {
        found: false,
        rsdp_phys: 0,
        rsdp_revision: 0,
        rsdp_extended_valid: false,
        xsdt_phys: 0,
        rsdt_phys: 0,
        table_count: 0,
        invalid_table_count: 0,
        mcfg_phys: 0,
        madt_phys: 0,
        fadt_phys: 0,
        hpet_phys: 0,
        mcfg_base: 0,
        mcfg_segment: 0,
        mcfg_start_bus: 0,
        mcfg_end_bus: 0,
        madt_lapic_address: 0,
        madt_flags: 0,
        madt_lapic_count: 0,
        madt_lapic_enabled_count: 0,
        madt_x2apic_count: 0,
        madt_ioapic_count: 0,
        madt_iso_count: 0,
        madt_nmi_count: 0,
        madt_malformed_entries: 0,
        madt_cpu_entries: [MadtCpu::EMPTY; 32],
        madt_first_ioapic_id: 0,
        madt_first_ioapic_address: 0,
        madt_first_ioapic_gsi_base: 0,
        madt_first_iso_source: 0,
        madt_first_iso_gsi: 0,
        madt_first_iso_flags: 0,
        madt_ioapics: [MadtIoApic::EMPTY; 4],
        madt_isos: [InterruptSourceOverride::EMPTY; 16],
        fadt_revision: 0,
        fadt_preferred_pm_profile: 0,
        fadt_sci_int: 0,
        fadt_smi_cmd: 0,
        fadt_acpi_enable: 0,
        fadt_acpi_disable: 0,
        fadt_pm1a_cnt_blk: 0,
        fadt_pm1b_cnt_blk: 0,
        fadt_flags: 0,
        fadt_century: 0,
        fadt_dsdt_phys: 0,
        fadt_x_dsdt_phys: 0,
        fadt_reset_supported: false,
        fadt_reset_address_space: 0,
        fadt_reset_bit_width: 0,
        fadt_reset_bit_offset: 0,
        fadt_reset_access_size: 0,
        fadt_reset_address: 0,
        fadt_reset_value: 0,
        fadt_reset_gas_valid: false,
        fadt_reset_gas_standard: false,
        s5_found: false,
        s5_slp_typa: 0,
        s5_slp_typb: 0,
        s5_dsdt_valid: false,
        s5_reason: 0,
        hpet_base: 0,
        hpet_number: 0,
        hpet_min_tick: 0,
        hpet_legacy_replacement: false,
    };
}

struct Storage(UnsafeCell<Info>);
// Boot construction completes before drivers, tasks and AP observers exist.
unsafe impl Sync for Storage {}
static INFO: Storage = Storage(UnsafeCell::new(Info::EMPTY));
fn puts(text: &[u8]) {
    unsafe { crate::boot::log::r4os_rust_bootlog_write(text.as_ptr(), text.len()) };
}
fn dec(value: u64) {
    crate::boot::log::r4os_rust_bootlog_dec(value);
}
fn hex(value: u64, width: usize) {
    crate::boot::log::r4os_rust_bootlog_hex(value, width);
}
fn log(event: u8, a: u64, b: u64, c: u64, info: &Info) {
    match event {
        0 => puts(b"[ACPI] static reader\r\n"),
        1 => puts(b"[ACPI][WARN] RSDP not found\r\n"),
        2 => {
            puts(b"[ACPI] RSDP phys=0x");
            hex(a, 16);
            puts(b"\r\n");
        }
        3 => puts(b"[ACPI][WARN] RSDP mapping failed\r\n"),
        4 => puts(b"[ACPI][WARN] invalid RSDP signature/checksum\r\n"),
        5 => {
            puts(b"[ACPI] RSDP rev=");
            dec(u64::from(info.rsdp_revision));
            puts(b" rsdt=0x");
            hex(a, 8);
            puts(b" xsdt=0x");
            hex(b, 16);
            puts(b" len=");
            dec(c);
            puts(b"\r\n");
        }
        6 => puts(b"[ACPI][WARN] invalid extended RSDP checksum, using RSDT fallback\r\n"),
        7 => puts(b"[ACPI][WARN] MCFG not found\r\n"),
        8 => puts(b"[ACPI][WARN] MADT/APIC not found\r\n"),
        9 => puts(b"[ACPI][WARN] FADT/FACP not found\r\n"),
        10 => puts(b"[ACPI][WARN] HPET not found\r\n"),
        11 => puts(b"[ACPI][WARN] XSDT mapping failed\r\n"),
        12 => puts(b"[ACPI][WARN] invalid XSDT\r\n"),
        13 => puts(b"[ACPI][WARN] RSDT mapping failed\r\n"),
        14 => puts(b"[ACPI][WARN] invalid RSDT\r\n"),
        15 => {
            puts(b"[ACPI][WARN] table mapping failed phys=0x");
            hex(a, 16);
            puts(b"\r\n");
        }
        16 => {
            puts(b"[ACPI][WARN] invalid table at phys=0x");
            hex(a, 16);
            puts(b"\r\n");
        }
        17 => {
            puts(b"[ACPI] table ");
            puts(&(a as u32).to_le_bytes());
            puts(b" phys=0x");
            hex(b, 16);
            puts(b" len=");
            dec(c);
            puts(b"\r\n");
        }
        18 => puts(b"[ACPI][WARN] MCFG too small\r\n"),
        19 => {
            puts(b"[ACPI] MCFG ECAM base=0x");
            hex(info.mcfg_base, 16);
            puts(b" segment=");
            dec(u64::from(info.mcfg_segment));
            puts(b" buses=");
            dec(u64::from(info.mcfg_start_bus));
            puts(b"..");
            dec(u64::from(info.mcfg_end_bus));
            puts(b"\r\n");
        }
        20 => puts(b"[ACPI][WARN] MADT too small\r\n"),
        21 => {
            puts(b"[ACPI] MADT lapic=0x");
            hex(info.madt_lapic_address, 16);
            puts(b" cpus=");
            dec(u64::from(info.madt_lapic_enabled_count));
            puts(b"/");
            dec(u64::from(info.madt_lapic_count));
            puts(b" ioapic=");
            dec(u64::from(info.madt_ioapic_count));
            puts(b" iso=");
            dec(u64::from(info.madt_iso_count));
            puts(b" nmi=");
            dec(u64::from(info.madt_nmi_count));
            if info.madt_lapic_count != 0 {
                let first_cpu = info.madt_cpu_entries[0];
                puts(b" cpu0_apic=");
                dec(u64::from(first_cpu.apic_id));
                puts(if first_cpu.x2apic {
                    b"(x2)"
                } else {
                    b"(xapic)"
                });
            }
            if info.madt_malformed_entries != 0 {
                puts(b" malformed=yes");
            }
            puts(b"\r\n");
        }
        22 => {
            puts(b"[ACPI] FADT rev=");
            dec(u64::from(info.fadt_revision));
            puts(b" sci=");
            dec(u64::from(info.fadt_sci_int));
            puts(b" pm1a_cnt=0x");
            hex(u64::from(info.fadt_pm1a_cnt_blk), 8);
            puts(b" reset=");
            puts(if info.fadt_reset_supported {
                b"yes"
            } else {
                b"no"
            });
            if info.fadt_reset_supported {
                puts(b" space=");
                dec(u64::from(info.fadt_reset_address_space));
                puts(b" width=");
                dec(u64::from(info.fadt_reset_bit_width));
                puts(b" offset=");
                dec(u64::from(info.fadt_reset_bit_offset));
                puts(b" access=");
                dec(u64::from(info.fadt_reset_access_size));
                puts(b" addr=0x");
                hex(info.fadt_reset_address, 16);
                puts(b" value=0x");
                hex(u64::from(info.fadt_reset_value), 2);
                puts(b" path=");
                puts(match info.fadt_reset_address_space {
                    0 => b"mmio",
                    1 => b"io",
                    2 => b"pci",
                    _ => b"invalid",
                });
                puts(if info.fadt_reset_gas_valid {
                    b" gas=usable"
                } else {
                    b" gas=invalid"
                });
                puts(if info.fadt_reset_gas_standard {
                    b" layout=standard"
                } else {
                    b" layout=nonstandard-byte"
                });
            }
            puts(b"\r\n");
        }
        23 => {
            puts(b"[ACPI] S5 typa=");
            dec(u64::from(info.s5_slp_typa));
            puts(b" typb=");
            dec(u64::from(info.s5_slp_typb));
            puts(b" source=DSDT\r\n");
        }
        24 => puts(b"[ACPI][WARN] S5 package not found in DSDT\r\n"),
        25 => puts(b"[ACPI][WARN] HPET too small\r\n"),
        26 => {
            puts(b"[ACPI] HPET base=0x");
            hex(info.hpet_base, 16);
            puts(b" min_tick=");
            dec(u64::from(info.hpet_min_tick));
            puts(b" legacy=");
            puts(if info.hpet_legacy_replacement {
                b"yes"
            } else {
                b"no"
            });
            puts(b"\r\n");
        }
        _ => unreachable!(),
    }
}
pub fn snapshot() -> Info {
    unsafe { INFO.0.get().read() }
}
pub fn inspect() -> Info {
    let mut value = Info::EMPTY;
    unsafe { (&mut *firmware_tables::catalog()).begin() };
    inspect_into(&mut value);
    unsafe {
        (&mut *firmware_tables::catalog()).seal();
        INFO.0.get().write(value);
    }
    value
}
fn inspect_into(info: &mut Info) {
    log(0, 0, 0, 0, info);
    let Some(rsdp_phys) = find_rsdp() else {
        log(1, 0, 0, 0, info);
        return;
    };
    info.found = true;
    info.rsdp_phys = rsdp_phys;
    log(2, rsdp_phys, 0, 0, info);
    if !map_phys_range(rsdp_phys, 36) {
        log(3, 0, 0, 0, info);
        return;
    }
    let rsdp = bytes_at(rsdp_phys, 36);
    if &rsdp[..8] != b"RSD PTR " || checksum(&rsdp[..20]) != 0 {
        log(4, 0, 0, 0, info);
        return;
    }
    info.rsdp_revision = rsdp[15];
    let rsdt = read32(rsdp, 16);
    let length = read32(rsdp, 20);
    let xsdt = read64(rsdp, 24);
    log(5, u64::from(rsdt), xsdt, u64::from(length), info);
    if info.rsdp_revision >= 2
        && (36..=4096).contains(&length)
        && map_phys_range(rsdp_phys, u64::from(length))
    {
        info.rsdp_extended_valid = checksum(bytes_at(rsdp_phys, length as usize)) == 0;
    }
    info.rsdt_phys = rsdt;
    if info.rsdp_revision >= 2 && xsdt != 0 && info.rsdp_extended_valid {
        info.xsdt_phys = xsdt;
        inspect_root(xsdt, true, info);
    } else if rsdt != 0 {
        if info.rsdp_revision >= 2 && xsdt != 0 {
            log(6, 0, 0, 0, info);
        }
        inspect_root(u64::from(rsdt), false, info);
    }
    for (missing, event) in [
        (info.mcfg_phys == 0, 7),
        (info.madt_phys == 0, 8),
        (info.fadt_phys == 0, 9),
        (info.hpet_phys == 0, 10),
    ] {
        if missing {
            log(event, 0, 0, 0, info);
        }
    }
    inspect_mcfg(info);
    inspect_madt(info);
    inspect_fadt(info);
    inspect_s5(info);
    inspect_hpet(info);
}
#[derive(Clone, Copy)]
struct Table {
    physical: u64,
    length: usize,
}
impl Table {
    fn bytes(self) -> &'static [u8] {
        let length = if (HEADER..=MAX_SDT_LENGTH).contains(&self.length) {
            self.length
        } else {
            HEADER
        };
        bytes_at(self.physical, length)
    }
    fn valid(self) -> bool {
        (HEADER..=MAX_SDT_LENGTH).contains(&self.length) && checksum(self.bytes()) == 0
    }
}
fn inspect_root(physical: u64, extended: bool, info: &mut Info) {
    let Some(table) = map_table(physical) else {
        log(if extended { 11 } else { 13 }, 0, 0, 0, info);
        return;
    };
    let bytes = table.bytes();
    if &bytes[..4] != if extended { b"XSDT" } else { b"RSDT" } || !table.valid() {
        log(if extended { 12 } else { 14 }, 0, 0, 0, info);
        return;
    }
    let width = if extended { 8 } else { 4 };
    if !(table.length - HEADER).is_multiple_of(width) {
        unsafe { (&mut *firmware_tables::catalog()).incomplete() };
        return;
    }
    unsafe { (&mut *firmware_tables::catalog()).root_validated() };
    for offset in (HEADER..table.length).step_by(width) {
        let physical = if extended {
            read64(bytes, offset)
        } else {
            u64::from(read32(bytes, offset))
        };
        inspect_table(physical, info);
    }
}
fn inspect_table(physical: u64, info: &mut Info) {
    if physical == 0 {
        unsafe { (&mut *firmware_tables::catalog()).incomplete() };
        return;
    }
    let Some(table) = map_table(physical) else {
        unsafe { (&mut *firmware_tables::catalog()).incomplete() };
        info.invalid_table_count += 1;
        log(15, physical, 0, 0, info);
        return;
    };
    let bytes = table.bytes();
    if firmware_tables::retained_backing(physical, bytes.len() as u64, memory_map()) {
        unsafe { (&mut *firmware_tables::catalog()).observe(bytes) };
    } else {
        unsafe { (&mut *firmware_tables::catalog()).incomplete() };
    }
    if !table.valid() {
        info.invalid_table_count += 1;
        log(16, physical, 0, 0, info);
        return;
    }
    info.table_count += 1;
    log(
        17,
        u64::from(read32(bytes, 0)),
        physical,
        table.length as u64,
        info,
    );
    match &bytes[..4] {
        b"MCFG" => info.mcfg_phys = physical,
        b"APIC" => info.madt_phys = physical,
        b"FACP" => info.fadt_phys = physical,
        b"HPET" => info.hpet_phys = physical,
        _ => {}
    }
}
fn inspect_mcfg(info: &mut Info) {
    if info.mcfg_phys == 0 {
        return;
    }
    let Some(table) = map_table(info.mcfg_phys) else {
        return;
    };
    if table.length < HEADER + 8 + 16 {
        log(18, 0, 0, 0, info);
        return;
    }
    let bytes = table.bytes();
    let offset = HEADER + 8;
    info.mcfg_base = read64(bytes, offset);
    info.mcfg_segment = read16(bytes, offset + 8);
    info.mcfg_start_bus = bytes[offset + 10];
    info.mcfg_end_bus = bytes[offset + 11];
    log(19, 0, 0, 0, info);
}
fn inspect_madt(info: &mut Info) {
    if info.madt_phys == 0 {
        return;
    }
    let Some(table) = map_table(info.madt_phys) else {
        return;
    };
    if table.length < HEADER + 8 {
        log(20, 0, 0, 0, info);
        return;
    }
    let bytes = table.bytes();
    info.madt_lapic_address = u64::from(read32(bytes, HEADER));
    info.madt_flags = read32(bytes, HEADER + 4);
    let mut offset = HEADER + 8;
    while offset + 2 <= table.length {
        let kind = bytes[offset];
        let length = usize::from(bytes[offset + 1]);
        if length < 2 || offset + length > table.length {
            info.madt_malformed_entries += 1;
            break;
        }
        match kind {
            0 if length >= 8 => {
                let flags = read32(bytes, offset + 4);
                let index = info.madt_lapic_count as usize;
                if index < 32 {
                    info.madt_cpu_entries[index] = MadtCpu {
                        acpi_uid: u32::from(bytes[offset + 2]),
                        apic_id: u32::from(bytes[offset + 3]),
                        flags,
                        x2apic: false,
                    };
                }
                info.madt_lapic_count += 1;
                if flags & 1 != 0 {
                    info.madt_lapic_enabled_count += 1;
                }
            }
            1 if length >= 12 => {
                let index = info.madt_ioapic_count as usize;
                info.madt_ioapic_count += 1;
                let value = MadtIoApic {
                    id: bytes[offset + 2],
                    address: read32(bytes, offset + 4),
                    gsi_base: read32(bytes, offset + 8),
                };
                if index < 4 {
                    info.madt_ioapics[index] = value;
                }
                if info.madt_ioapic_count == 1 {
                    info.madt_first_ioapic_id = value.id;
                    info.madt_first_ioapic_address = value.address;
                    info.madt_first_ioapic_gsi_base = value.gsi_base;
                }
            }
            2 if length >= 10 => {
                let index = info.madt_iso_count as usize;
                info.madt_iso_count += 1;
                let value = InterruptSourceOverride {
                    bus: bytes[offset + 2],
                    source: bytes[offset + 3],
                    gsi: read32(bytes, offset + 4),
                    flags: read16(bytes, offset + 8),
                };
                if index < 16 {
                    info.madt_isos[index] = value;
                }
                if info.madt_iso_count == 1 {
                    info.madt_first_iso_source = value.source;
                    info.madt_first_iso_gsi = value.gsi;
                    info.madt_first_iso_flags = value.flags;
                }
            }
            4 if length >= 6 => {
                info.madt_nmi_count += 1;
            }
            5 if length >= 12 => {
                info.madt_lapic_address = read64(bytes, offset + 4);
            }
            9 if length >= 16 => {
                let flags = read32(bytes, offset + 8);
                let index = info.madt_lapic_count as usize;
                if index < 32 {
                    info.madt_cpu_entries[index] = MadtCpu {
                        acpi_uid: read32(bytes, offset + 12),
                        apic_id: read32(bytes, offset + 4),
                        flags,
                        x2apic: true,
                    };
                }
                info.madt_lapic_count += 1;
                if flags & 1 != 0 {
                    info.madt_lapic_enabled_count += 1;
                }
                info.madt_x2apic_count += 1;
            }
            _ => {}
        }
        offset += length;
    }
    log(21, 0, 0, 0, info);
}
fn inspect_fadt(info: &mut Info) {
    if info.fadt_phys == 0 {
        return;
    }
    let Some(table) = map_table(info.fadt_phys) else {
        return;
    };
    let b = table.bytes();
    let length = table.length;
    info.fadt_revision = b[8];
    if length > 45 {
        info.fadt_preferred_pm_profile = b[45];
    }
    if length >= 48 {
        info.fadt_sci_int = read16(b, 46);
    }
    if length >= 52 {
        info.fadt_smi_cmd = read32(b, 48);
    }
    if length > 52 {
        info.fadt_acpi_enable = b[52];
    }
    if length > 53 {
        info.fadt_acpi_disable = b[53];
    }
    if length >= 68 {
        info.fadt_pm1a_cnt_blk = read32(b, 64);
    }
    if length >= 72 {
        info.fadt_pm1b_cnt_blk = read32(b, 68);
    }
    if length > 108 {
        info.fadt_century = b[108];
    }
    if length >= 116 {
        info.fadt_flags = read32(b, 112);
    }
    if length >= 44 {
        info.fadt_dsdt_phys = u64::from(read32(b, 40));
    }
    if length >= 129 {
        info.fadt_reset_address_space = b[116];
        info.fadt_reset_bit_width = b[117];
        info.fadt_reset_bit_offset = b[118];
        info.fadt_reset_access_size = b[119];
        info.fadt_reset_address = read64(b, 120);
        info.fadt_reset_value = b[128];
        info.fadt_reset_gas_standard = info.fadt_reset_bit_width == 8
            && info.fadt_reset_bit_offset == 0
            && matches!(info.fadt_reset_access_size, 0 | 1);
        // Preserve the original exact-byte reset compatibility admission.
        info.fadt_reset_gas_valid = info.fadt_revision >= 2 && info.fadt_reset_address_space <= 2;
        info.fadt_reset_supported =
            info.fadt_flags & (1 << 10) != 0 && info.fadt_reset_address != 0;
    }
    if length >= 148 {
        info.fadt_x_dsdt_phys = read64(b, 140);
    }
    log(22, 0, 0, 0, info);
}
fn inspect_s5(info: &mut Info) {
    let physical = if info.fadt_x_dsdt_phys != 0 {
        info.fadt_x_dsdt_phys
    } else {
        info.fadt_dsdt_phys
    };
    if physical == 0 {
        info.s5_reason = 1;
        return;
    }
    let Some(table) = map_table(physical) else {
        info.s5_reason = 2;
        return;
    };
    let bytes = table.bytes();
    if &bytes[..4] != b"DSDT" {
        info.s5_reason = 3;
        return;
    }
    info.s5_dsdt_valid = table.valid();
    if !info.s5_dsdt_valid {
        info.s5_reason = 4;
        return;
    }
    let mut offset = HEADER;
    while offset + 8 < table.length {
        if &bytes[offset..offset + 4] == b"_S5_"
            && looks_like_name(bytes, offset)
            && let Some(types) = parse_s5(bytes, offset + 4)
        {
            info.s5_found = true;
            info.s5_slp_typa = types[0];
            info.s5_slp_typb = types[1];
            info.s5_reason = 5;
            log(23, 0, 0, 0, info);
            return;
        }
        offset += 1;
    }
    info.s5_reason = 6;
    log(24, 0, 0, 0, info);
}
fn looks_like_name(bytes: &[u8], offset: usize) -> bool {
    offset != 0
        && (bytes[offset - 1] == 0x08
            || (offset >= 2 && bytes[offset - 2] == 0x08 && bytes[offset - 1] == b'\\'))
}
fn parse_s5(bytes: &[u8], offset: usize) -> Option<[u8; 2]> {
    if offset >= bytes.len() || bytes[offset] != 0x12 {
        return None;
    }
    let (length, next) = parse_pkg_length(bytes, offset + 1)?;
    if next >= bytes.len() {
        return None;
    }
    let end = bytes.len().min(offset + 1 + length as usize);
    if next >= end || bytes[next] < 2 {
        return None;
    }
    let bounded = &bytes[..end];
    let (a, next) = parse_integer(bounded, next + 1)?;
    let (b, _) = parse_integer(bounded, next)?;
    if a > 7 || b > 7 {
        return None;
    }
    Some([a as u8, b as u8])
}
fn parse_pkg_length(bytes: &[u8], offset: usize) -> Option<(u32, usize)> {
    let lead = *bytes.get(offset)?;
    let following = usize::from(lead >> 6);
    let mut length = u32::from(if following == 0 {
        lead & 0x3f
    } else {
        lead & 0x0f
    });
    let mut shift = 4;
    for i in 0..following {
        length |= u32::from(*bytes.get(offset + 1 + i)?) << shift;
        shift += 8;
    }
    Some((length, offset + 1 + following))
}
fn parse_integer(bytes: &[u8], offset: usize) -> Option<(u64, usize)> {
    match *bytes.get(offset)? {
        0x00 => Some((0, offset + 1)),
        0x01 => Some((1, offset + 1)),
        0x0a if offset + 1 < bytes.len() => Some((u64::from(bytes[offset + 1]), offset + 2)),
        0x0b if offset + 2 < bytes.len() => {
            Some((u64::from(read16(bytes, offset + 1)), offset + 3))
        }
        0x0c if offset + 4 < bytes.len() => {
            Some((u64::from(read32(bytes, offset + 1)), offset + 5))
        }
        0x0e if offset + 8 < bytes.len() => Some((read64(bytes, offset + 1), offset + 9)),
        _ => None,
    }
}
fn inspect_hpet(info: &mut Info) {
    if info.hpet_phys == 0 {
        return;
    }
    let Some(table) = map_table(info.hpet_phys) else {
        return;
    };
    if table.length < HEADER + 20 {
        log(25, 0, 0, 0, info);
        return;
    }
    let bytes = table.bytes();
    info.hpet_base = read64(bytes, HEADER + 8);
    info.hpet_number = bytes[HEADER + 16];
    info.hpet_min_tick = read16(bytes, HEADER + 17);
    info.hpet_legacy_replacement = read32(bytes, HEADER) & (1 << 15) != 0;
    log(26, 0, 0, 0, info);
}
pub(super) fn memory_map() -> &'static [boot::MemoryEntry] {
    let current = unsafe { &*boot::r4os_rust_boot_info() };
    if current.memory_count == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(current.memory, current.memory_count) }
    }
}
fn find_rsdp() -> Option<u64> {
    let current = unsafe { &*boot::r4os_rust_boot_info() };
    if current.flags & 16 != 0 {
        return Some(current.rsdp);
    }
    for entry in memory_map() {
        if !matches!(entry.kind, 2 | 3) {
            continue;
        }
        if let Some(physical) = scan_range(entry.base, entry.length) {
            return Some(physical);
        }
    }
    None
}
fn scan_range(base: u64, length: u64) -> Option<u64> {
    if !map_phys_range(base, length) {
        return None;
    }
    let mut offset = 0;
    while offset + 20 <= length {
        let bytes = bytes_at(base + offset, 20);
        if &bytes[..8] == b"RSD PTR " && checksum(bytes) == 0 {
            return Some(base + offset);
        }
        offset += 16;
    }
    None
}
fn map_table(physical: u64) -> Option<Table> {
    if !map_phys_range(physical, HEADER as u64) {
        return None;
    }
    let length = read32(bytes_at(physical, HEADER), 4) as usize;
    if (HEADER..=MAX_SDT_LENGTH).contains(&length) && !map_phys_range(physical, length as u64) {
        return None;
    }
    Some(Table { physical, length })
}
fn map_phys_range(base: u64, length: u64) -> bool {
    if length == 0 {
        return true;
    }
    let Some(end) = base
        .checked_add(length)
        .and_then(|end| end.checked_add(4095))
        .map(|end| end & !4095)
    else {
        return false;
    };
    let mut page = base & !4095;
    while page < end {
        let address = phys::r4os_rust_phys_to_virt(page);
        let mut unused = 0;
        if !unsafe { paging::r4os_rust_paging_read(address, 3, false, &mut unused) }
            && !paging::r4os_rust_paging_map(address, page, 1 << 63, false)
        {
            return false;
        }
        page += 4096;
    }
    true
}
fn bytes_at(physical: u64, length: usize) -> &'static [u8] {
    unsafe { slice::from_raw_parts(phys::r4os_rust_phys_to_virt(physical) as *const u8, length) }
}
fn checksum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte))
}
fn read16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
}
fn read32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
fn read64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}
/// # Safety
/// BSP-exclusive boot context admits inspection; output is aligned exclusive C storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_acpi_inspect(output: *mut Info) {
    unsafe { output.write(inspect()) };
}
/// # Safety
/// Output is aligned exclusive C storage. Boot publication is complete before consumers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_acpi_info(output: *mut Info) {
    unsafe { output.write(snapshot()) };
}
const _: () = {
    assert!(core::mem::size_of::<MadtCpu>() == 16);
    assert!(core::mem::size_of::<MadtIoApic>() == 12);
    assert!(core::mem::size_of::<InterruptSourceOverride>() == 12);
    assert!(core::mem::size_of::<Info>() == 1000);
    assert!(core::mem::offset_of!(Info, madt_cpu_entries) == 136);
    assert!(core::mem::offset_of!(Info, s5_reason) == 983);
    assert!(core::mem::offset_of!(Info, hpet_base) == 984);
};
