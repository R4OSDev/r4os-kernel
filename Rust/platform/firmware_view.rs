//! Borrow only already permanent firmware/device mappings. This creates no
//! aliases or mappings; AML/EC/operation regions remain an external owner.
use super::{acpi, cpu, firmware_tables};
use crate::{boot::info as boot, memory::paging};

pub fn root() -> u64 {
    let info = acpi::snapshot();
    if info.rsdt_phys != 0 || info.xsdt_phys != 0 {
        info.rsdp_phys
    } else {
        0
    }
}
fn hhdm(physical: u64) -> Option<u64> {
    let current = unsafe { &*boot::r4os_rust_boot_info() };
    if current.flags & 4 == 0 {
        return None;
    }
    physical.checked_add(current.hhdm)
}
pub fn view(base: u64, bytes: u64) -> Option<u64> {
    if base == 0 || bytes == 0 || bytes > 16 * 1024 * 1024 {
        return None;
    }
    let end = base.checked_add(bytes)?;
    if end > (1 << 52) {
        return None;
    }
    let address = hhdm(base)?;
    address.checked_add(bytes)?;
    let entries = acpi::memory_map();
    let retained = firmware_tables::retained_backing(base, bytes, entries);
    let mut cursor = base & !4095;
    while cursor < end {
        let virtual_address = hhdm(cursor)?;
        let mut physical = 0;
        if !unsafe { paging::r4os_rust_paging_read(virtual_address, 2, false, &mut physical) }
            || physical != cursor
        {
            return None;
        }
        if !retained {
            for entry in entries {
                if entry.valid
                    && cursor < entry.end
                    && entry.base < cursor + 4096
                    && entry.kind != 1
                {
                    return None;
                }
            }
            let mut selector = 0;
            if !unsafe { paging::r4os_rust_paging_read(virtual_address, 1, false, &mut selector) } {
                return None;
            }
            if cpu::r4os_rust_cpu_control(0, 0) == 0
                || (cpu::snapshot().pat_msr >> (selector * 8)) as u8 != 0
            {
                return None;
            }
        }
        cursor += 4096;
    }
    Some(address)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_firmware_root() -> u64 {
    root()
}
/// # Safety
/// Output is aligned exclusive C storage; the caller retains permanent boot
/// mappings throughout the call and the existing driver context afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_firmware_view(base: u64, bytes: u64, output: *mut u64) -> bool {
    let Some(address) = view(base, bytes) else {
        return false;
    };
    unsafe { output.write(address) };
    true
}
