//! Original terminal x86 reset/power paths. Exact physical writes and fallback
//! order; after reset issuance no HPET/chipset timestamp read is introduced.
use crate::{
    arch::{io, smp},
    boot::log as bootlog,
    memory::root,
    platform::{acpi, cpu, pci, pcie},
};
use core::{arch::asm, ptr};
unsafe extern "C" {
    fn r4os_zig_terminal_log(op: u8, text: *const u8, len: usize, value: u64, width: usize);
}
fn log(s: &[u8]) {
    unsafe { r4os_zig_terminal_log(0, s.as_ptr(), s.len(), 0, 0) }
}
fn dec(v: u64) {
    unsafe { r4os_zig_terminal_log(1, ptr::null(), 0, v, 0) }
}
fn hex(v: u64, w: usize) {
    unsafe { r4os_zig_terminal_log(2, ptr::null(), 0, v, w) }
}
fn flush() {
    unsafe { r4os_zig_terminal_log(3, ptr::null(), 0, 0, 0) }
}
fn boot(s: &[u8]) {
    unsafe { bootlog::r4os_rust_bootlog_write(s.as_ptr(), s.len()) }
}
fn boot_dec(v: u64) {
    bootlog::r4os_rust_bootlog_dec(v)
}
fn boot_hex(v: u64, w: usize) {
    bootlog::r4os_rust_bootlog_hex(v, w)
}
fn reason(code: u8) -> &'static [u8] {
    match code {
        0 => b"not inspected",
        1 => b"FADT has no DSDT pointer",
        2 => b"DSDT mapping failed",
        3 => b"DSDT signature mismatch",
        4 => b"DSDT checksum invalid",
        5 => b"DSDT _S5 package parsed",
        6 => b"DSDT _S5 package not found",
        _ => panic!("Invalid original ACPI S5 reason"),
    }
}
fn halt() -> ! {
    loop {
        unsafe { asm!("cli; hlt", options(nomem, nostack)) };
    }
}
unsafe fn wait() {
    unsafe { io::outb(0x80, 0) }
}
unsafe fn outw(port: u16, value: u16) {
    unsafe { asm!("out dx, ax",in("dx")port,in("ax")value,options(nomem,nostack,preserves_flags)) }
}
unsafe fn stop_others() {
    unsafe {
        asm!("cli", options(nomem, nostack));
    }
    smp::r4os_rust_smp_control(3, 0);
}
fn read_tsc() -> u64 {
    let lo: u32;
    let hi: u32;
    unsafe { asm!("rdtsc",out("eax")lo,out("edx")hi,options(nomem,nostack,preserves_flags)) };
    (hi as u64) << 32 | lo as u64
}
fn reset_tsc_frequency() -> u64 {
    let info = cpu::snapshot();
    if info.tsc_denominator != 0 && info.tsc_numerator != 0 && info.crystal_hz != 0 {
        let frequency =
            info.crystal_hz as u128 * info.tsc_numerator as u128 / info.tsc_denominator as u128;
        if (1_000_000..=10_000_000_000).contains(&frequency) {
            return frequency.try_into().unwrap();
        }
    }
    let base = info.base_mhz as u64 * 1_000_000;
    if (1_000_000..=10_000_000_000).contains(&base) {
        return base;
    }
    10_000_000_000
}
fn reset_grace_cycles() -> u64 {
    (reset_tsc_frequency() * 15).div_ceil(1000)
}
fn wait_after_acpi_reset(cycles: u64) {
    let start = read_tsc();
    let mut spins = 0;
    while read_tsc().wrapping_sub(start) < cycles.max(1) && spins < 50_000_000 {
        core::hint::spin_loop();
        spins += 1;
    }
}
unsafe fn write_reset_memory(address: u64, value: u8) -> bool {
    unsafe {
        let mut alias = 0;
        if !root::r4os_rust_root_alias(address, 1, &mut alias) {
            boot(b"[RESET][WARN] ACPI reset uncached MMIO alias missing\r\n");
            log(b"[RESET][WARN] ACPI reset uncached MMIO alias missing\r\n");
            return false;
        }
        (alias as *mut u8).write_volatile(value);
        asm!("mfence", options(nostack, preserves_flags));
        true
    }
}
unsafe fn write_reset_pci(address: u64, value: u8) -> bool {
    unsafe {
        let reserved = address >> 48;
        let device = (address >> 32) & 0xffff;
        let function = (address >> 16) & 0xffff;
        let register = address & 0xffff;
        if reserved != 0 || device > 31 || function > 7 || register > 0xfff {
            boot(b"[RESET][WARN] ACPI reset PCI address out of range\r\n");
            return false;
        }
        let ecam = pcie::snapshot();
        if ecam.mcfg_base != 0 && ecam.segment == 0 && ecam.start_bus == 0 && register <= 0xfff {
            let physical = ecam
                .mcfg_base
                .checked_add(device << 15)
                .and_then(|base| base.checked_add(function << 12))
                .and_then(|base| base.checked_add(register));
            if let Some(physical) = physical {
                let mut alias = 0;
                if root::r4os_rust_root_alias(physical, 1, &mut alias) {
                    (alias as *mut u8).write_volatile(value);
                    asm!("mfence", options(nostack, preserves_flags));
                    wait();
                    return true;
                }
            }
        }
        if register > 0xff {
            boot(b"[RESET][WARN] ACPI reset PCI ECAM alias unavailable\r\n");
            return false;
        }
        pci::write_config8(
            0,
            device.try_into().unwrap(),
            function.try_into().unwrap(),
            register.try_into().unwrap(),
            value,
        );
        wait();
        true
    }
}
unsafe fn try_acpi_reset() -> bool {
    unsafe {
        let info = acpi::snapshot();
        if !info.fadt_reset_supported {
            boot(b"[RESET] ACPI reset not advertised\r\n");
            return false;
        }
        if !info.fadt_reset_gas_valid {
            boot(b"[RESET][WARN] ACPI reset GAS invalid\r\n");
            log(b"[RESET][WARN] ACPI reset GAS invalid; skipping write\r\n");
            return false;
        }
        boot(b"[RESET] ACPI reset space=");
        boot_dec(info.fadt_reset_address_space as u64);
        boot(b" addr=0x");
        boot_hex(info.fadt_reset_address, 16);
        boot(b" value=0x");
        boot_hex(info.fadt_reset_value as u64, 2);
        boot(b"\r\n");
        log(b"[RESET] ACPI reset attempt space=");
        dec(info.fadt_reset_address_space as u64);
        log(b" addr=0x");
        hex(info.fadt_reset_address, 16);
        log(b" value=0x");
        hex(info.fadt_reset_value as u64, 2);
        log(b" grace_ms=15\r\n");
        flush();
        match info.fadt_reset_address_space {
            1 => {
                if info.fadt_reset_address <= 0xffff {
                    io::outb(info.fadt_reset_address as u16, info.fadt_reset_value);
                    wait();
                    return true;
                } else {
                    boot(b"[RESET][WARN] ACPI reset IO address out of range\r\n");
                }
            }
            0 => return write_reset_memory(info.fadt_reset_address, info.fadt_reset_value),
            2 => return write_reset_pci(info.fadt_reset_address, info.fadt_reset_value),
            _ => boot(b"[RESET][WARN] unsupported ACPI reset address space\r\n"),
        }
        false
    }
}
pub(crate) unsafe fn reboot() -> ! {
    unsafe {
        stop_others();
        boot(b"[RESET] reboot requested\r\n");
        let grace = reset_grace_cycles();
        let attempted = try_acpi_reset();
        if attempted {
            wait_after_acpi_reset(grace);
            log(b"[RESET] ACPI grace expired; keyboard-controller fallback\r\n");
        } else {
            log(b"[RESET] no usable ACPI reset write; keyboard-controller fallback\r\n");
        }
        boot(b"[RESET] keyboard controller fallback\r\n");
        flush();
        let mut guard = 0;
        while io::inb(0x64) & 2 != 0 && guard < 100_000 {
            wait();
            guard += 1;
        }
        io::outb(0x64, 0xfe);
        halt()
    }
}
unsafe fn try_acpi_s5(info: acpi::Info) -> bool {
    unsafe {
        if !info.s5_found {
            return false;
        }
        let mut wrote = false;
        if info.fadt_pm1a_cnt_blk != 0 && info.fadt_pm1a_cnt_blk <= 0xffff {
            let value = ((info.s5_slp_typa as u16) << 10) | (1 << 13);
            boot(b"[POWER] ACPI S5 PM1a_CNT 0x");
            boot_hex(info.fadt_pm1a_cnt_blk as u64, 8);
            boot(b" value=0x");
            boot_hex(value as u64, 4);
            boot(b"\r\n");
            outw(info.fadt_pm1a_cnt_blk as u16, value);
            wait();
            wrote = true;
        }
        if info.fadt_pm1b_cnt_blk != 0 && info.fadt_pm1b_cnt_blk <= 0xffff {
            let value = ((info.s5_slp_typb as u16) << 10) | (1 << 13);
            boot(b"[POWER] ACPI S5 PM1b_CNT 0x");
            boot_hex(info.fadt_pm1b_cnt_blk as u64, 8);
            boot(b" value=0x");
            boot_hex(value as u64, 4);
            boot(b"\r\n");
            outw(info.fadt_pm1b_cnt_blk as u16, value);
            wait();
            wrote = true;
        }
        if !wrote {
            boot(b"[POWER][WARN] ACPI S5 available, but PM1 I/O port unavailable\r\n");
        }
        wrote
    }
}
unsafe fn known_pm1(port: u32) {
    unsafe {
        if port == 0x604 {
            boot(b"[POWER] FADT PM1 matches QEMU/i440FX, writing SLP_EN\r\n");
            outw(port as u16, 1 << 13);
            wait();
        }
    }
}
pub(crate) unsafe fn poweroff() -> ! {
    unsafe {
        stop_others();
        boot(b"[POWER] poweroff requested\r\n");
        let info = acpi::snapshot();
        if try_acpi_s5(info) {
            halt()
        }
        if info.fadt_pm1a_cnt_blk != 0 {
            boot(b"[POWER] ACPI PM1a_CNT 0x");
            boot_hex(info.fadt_pm1a_cnt_blk as u64, 8);
            boot(b"\r\n");
            known_pm1(info.fadt_pm1a_cnt_blk);
        }
        if info.fadt_pm1b_cnt_blk != 0 {
            boot(b"[POWER] ACPI PM1b_CNT 0x");
            boot_hex(info.fadt_pm1b_cnt_blk as u64, 8);
            boot(b"\r\n");
            known_pm1(info.fadt_pm1b_cnt_blk);
        }
        if info.fadt_pm1a_cnt_blk != 0 || info.fadt_pm1b_cnt_blk != 0 {
            boot(b"[POWER] generic ACPI S5 skipped: ");
            boot(reason(info.s5_reason));
            boot(b"\r\n");
        }
        boot(b"[POWER] emulator fallback ports\r\n");
        outw(0x604, 1 << 13);
        wait();
        outw(0xb004, 1 << 13);
        wait();
        outw(0x4004, 0x3400);
        halt()
    }
}
pub(crate) fn dump_status() {
    let info = acpi::snapshot();
    log(b"Power status\r\n");
    log(b"  FADT: ");
    if info.fadt_phys == 0 {
        log(b"missing\r\n");
    } else {
        log(b"rev=");
        dec(info.fadt_revision as u64);
        log(b" flags=0x");
        hex(info.fadt_flags as u64, 8);
        log(b" sci=");
        dec(info.fadt_sci_int as u64);
        log(b"\r\n");
    }
    log(b"  PM1 control: a=0x");
    hex(info.fadt_pm1a_cnt_blk as u64, 8);
    log(b" b=0x");
    hex(info.fadt_pm1b_cnt_blk as u64, 8);
    log(b"\r\n");
    log(b"  Reset register: ");
    if info.fadt_reset_supported {
        log(b"space=");
        dec(info.fadt_reset_address_space as u64);
        log(b" addr=0x");
        hex(info.fadt_reset_address, 16);
        log(b" value=0x");
        hex(info.fadt_reset_value as u64, 2);
    } else {
        log(b"not advertised");
    }
    log(b"\r\n");
    log(b"  S5: ");
    if info.s5_found {
        log(b"available typa=");
        dec(info.s5_slp_typa as u64);
        log(b" typb=");
        dec(info.s5_slp_typb as u64);
        log(b" dsdt_checksum=");
        log(if info.s5_dsdt_valid { b"ok" } else { b"bad" });
    } else {
        log(b"unavailable");
    }
    log(b" reason=");
    log(reason(info.s5_reason));
    log(b"\r\n");
    log(b"  Poweroff path: DSDT _S5 PM1 if available, known FADT PM1 emulator path, emulator ports, halt fallback\r\n");
    log(b"  Reboot path: FADT reset register if advertised, keyboard-controller fallback\r\n");
    log(b"  System suspend/resume: unavailable (no S3, S4 or S0ix platform path)\r\n");
}
/// # Safety
/// Terminal owner has completed the existing resource/driver barriers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_terminal_reset() -> ! {
    unsafe { reboot() }
}
/// # Safety
/// Original caller owns terminal poweroff; no cleanup acknowledgement is implied.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_terminal_poweroff() -> ! {
    unsafe { poweroff() }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_terminal_power_status() {
    dump_status()
}
