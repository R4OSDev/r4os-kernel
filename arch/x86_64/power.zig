const interrupts = @import("interrupts.zig");
const io = @import("io.zig");
const acpi = @import("../../platform/acpi.zig");
const bootlog = @import("../../kernel/bootlog.zig");
const k = @import("../../kernel/log.zig");
const smp = @import("../../kernel/smp.zig");

const QEMU_PM1_CONTROL: u16 = 0x0604;
const BOCHS_PM_CONTROL: u16 = 0xB004;
const VBOX_PM_CONTROL: u16 = 0x4004;
const SLP_EN: u16 = 1 << 13;

pub fn poweroff() noreturn {
    io.cli();
    smp.stopOthers();

    bootlog.puts("[POWER] poweroff requested\r\n");
    const info = acpi.info();
    if (tryAcpiS5Poweroff(info)) {
        interrupts.haltForever();
    }
    if (info.fadt_pm1a_cnt_blk != 0) {
        bootlog.puts("[POWER] ACPI PM1a_CNT 0x");
        bootlog.putHex(info.fadt_pm1a_cnt_blk, 8);
        bootlog.puts("\r\n");
        tryKnownPm1Poweroff(info.fadt_pm1a_cnt_blk);
    }
    if (info.fadt_pm1b_cnt_blk != 0) {
        bootlog.puts("[POWER] ACPI PM1b_CNT 0x");
        bootlog.putHex(info.fadt_pm1b_cnt_blk, 8);
        bootlog.puts("\r\n");
        tryKnownPm1Poweroff(info.fadt_pm1b_cnt_blk);
    }
    if (info.fadt_pm1a_cnt_blk != 0 or info.fadt_pm1b_cnt_blk != 0) {
        bootlog.puts("[POWER] generic ACPI S5 skipped: ");
        bootlog.puts(info.s5_reason);
        bootlog.puts("\r\n");
    }

    bootlog.puts("[POWER] emulator fallback ports\r\n");
    io.outw(QEMU_PM1_CONTROL, SLP_EN);
    io.wait();
    io.outw(BOCHS_PM_CONTROL, SLP_EN);
    io.wait();
    io.outw(VBOX_PM_CONTROL, 0x3400);

    interrupts.haltForever();
}

pub fn dumpStatus() void {
    const info = acpi.info();
    k.puts("Power status\r\n");
    k.puts("  FADT: ");
    if (info.fadt_phys == 0) {
        k.puts("missing\r\n");
    } else {
        k.puts("rev=");
        k.putDec(info.fadt_revision);
        k.puts(" flags=0x");
        k.putHex(info.fadt_flags, 8);
        k.puts(" sci=");
        k.putDec(info.fadt_sci_int);
        k.puts("\r\n");
    }
    k.puts("  PM1 control: a=0x");
    k.putHex(info.fadt_pm1a_cnt_blk, 8);
    k.puts(" b=0x");
    k.putHex(info.fadt_pm1b_cnt_blk, 8);
    k.puts("\r\n");
    k.puts("  Reset register: ");
    if (info.fadt_reset_supported) {
        k.puts("space=");
        k.putDec(info.fadt_reset_address_space);
        k.puts(" addr=0x");
        k.putHex(info.fadt_reset_address, 16);
        k.puts(" value=0x");
        k.putHex(info.fadt_reset_value, 2);
    } else {
        k.puts("not advertised");
    }
    k.puts("\r\n");
    k.puts("  S5: ");
    if (info.s5_found) {
        k.puts("available typa=");
        k.putDec(info.s5_slp_typa);
        k.puts(" typb=");
        k.putDec(info.s5_slp_typb);
        k.puts(" dsdt_checksum=");
        k.puts(if (info.s5_dsdt_valid) "ok" else "bad");
    } else {
        k.puts("unavailable");
    }
    k.puts(" reason=");
    k.puts(info.s5_reason);
    k.puts("\r\n");
    k.puts("  Poweroff path: DSDT _S5 PM1 if available, known FADT PM1 emulator path, emulator ports, halt fallback\r\n");
    k.puts("  Reboot path: FADT reset register if advertised, keyboard-controller fallback\r\n");
}

fn tryAcpiS5Poweroff(info: acpi.Info) bool {
    if (!info.s5_found) return false;
    var wrote = false;
    if (info.fadt_pm1a_cnt_blk != 0 and info.fadt_pm1a_cnt_blk <= 0xFFFF) {
        const value = (@as(u16, info.s5_slp_typa) << 10) | SLP_EN;
        bootlog.puts("[POWER] ACPI S5 PM1a_CNT 0x");
        bootlog.putHex(info.fadt_pm1a_cnt_blk, 8);
        bootlog.puts(" value=0x");
        bootlog.putHex(value, 4);
        bootlog.puts("\r\n");
        io.outw(@intCast(info.fadt_pm1a_cnt_blk), value);
        io.wait();
        wrote = true;
    }
    if (info.fadt_pm1b_cnt_blk != 0 and info.fadt_pm1b_cnt_blk <= 0xFFFF) {
        const value = (@as(u16, info.s5_slp_typb) << 10) | SLP_EN;
        bootlog.puts("[POWER] ACPI S5 PM1b_CNT 0x");
        bootlog.putHex(info.fadt_pm1b_cnt_blk, 8);
        bootlog.puts(" value=0x");
        bootlog.putHex(value, 4);
        bootlog.puts("\r\n");
        io.outw(@intCast(info.fadt_pm1b_cnt_blk), value);
        io.wait();
        wrote = true;
    }
    if (!wrote) bootlog.puts("[POWER][WARN] ACPI S5 available, but PM1 I/O port unavailable\r\n");
    return wrote;
}

fn tryKnownPm1Poweroff(port: u32) void {
    if (port == QEMU_PM1_CONTROL) {
        bootlog.puts("[POWER] FADT PM1 matches QEMU/i440FX, writing SLP_EN\r\n");
        io.outw(@intCast(port), SLP_EN);
        io.wait();
    }
}
