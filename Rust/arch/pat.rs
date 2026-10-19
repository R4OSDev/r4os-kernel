//! Immutable BSP memory-type contract and early, CPU-local AP adoption.
//! Only a still-starting AP may write PAT, before any device/MMIO consumer.
//! The BSP's complete live table is adopted without rewriting firmware state.
use super::{
    memory_types::{self as types, Mtrrs},
    percpu, safe_msr,
    smp_policy::MAX_CPUS,
};
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};
const PAT_MSR: u32 = 0x277;
struct Contract {
    table: u64,
    mtrrs: Mtrrs,
}
struct Storage(UnsafeCell<Contract>);
// SAFETY: BSP fills once before READY release. All subsequent references are
// immutable for the boot lifetime; no runtime memory-type MSR update exists.
unsafe impl Sync for Storage {}
static CONTRACT: Storage = Storage(UnsafeCell::new(Contract {
    table: 0,
    mtrrs: Mtrrs::EMPTY,
}));
static READY: AtomicBool = AtomicBool::new(false);
static VERIFIED: AtomicU64 = AtomicU64::new(0);
static READBACKS: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
static BEFORE: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
unsafe extern "C" {
    fn r4os_read_cr0() -> u64;
    fn r4os_write_cr0(value: u64);
    fn r4os_read_cr3() -> u64;
    fn r4os_write_cr3(value: u64);
    fn r4os_read_cr4() -> u64;
    fn r4os_write_cr4(value: u64);
}
fn contract() -> Option<&'static Contract> {
    READY
        .load(Ordering::Acquire)
        .then(|| unsafe { &*CONTRACT.0.get() })
}
fn capabilities() -> Result<(), &'static [u8]> {
    let features = core::arch::x86_64::__cpuid_count(1, 0).edx;
    if features & (1 << 5) == 0 {
        return Err(b"msr");
    }
    if features & (1 << 16) == 0 {
        return Err(b"pat-unavailable");
    }
    if features & (1 << 12) == 0 {
        return Err(b"mtrr-unavailable");
    }
    Ok(())
}
fn capture_mtrrs() -> Result<Mtrrs, &'static [u8]> {
    let extended = core::arch::x86_64::__cpuid_count(0x8000_0000, 0).eax;
    let bits = if extended >= 0x8000_0008 {
        core::arch::x86_64::__cpuid_count(0x8000_0008, 0).eax & 255
    } else {
        36
    };
    if !(32..=52).contains(&bits) {
        return Err(b"mtrr-addressing");
    }
    let cap = unsafe { safe_msr::read(0xfe) }.map_err(|_| b"mtrr-msr-access".as_slice())?;
    let default = unsafe { safe_msr::read(0x2ff) }.map_err(|_| b"mtrr-msr-access".as_slice())?;
    let kind = default as u8;
    if !types::valid_type(kind) || kind == types::UC_MINUS {
        return Err(b"mtrr-type");
    }
    let mut result = Mtrrs {
        limit: 1u64 << bits,
        default: kind,
        enabled: default & (1 << 11) != 0,
        ..Mtrrs::EMPTY
    };
    result.fixed_enabled = result.enabled && default & (1 << 10) != 0;
    if result.fixed_enabled {
        if cap & (1 << 8) == 0 {
            return Err(b"mtrr-fixed");
        }
        for (index, number) in [
            0x250, 0x258, 0x259, 0x268, 0x269, 0x26a, 0x26b, 0x26c, 0x26d, 0x26e, 0x26f,
        ]
        .into_iter()
        .enumerate()
        {
            let value =
                unsafe { safe_msr::read(number) }.map_err(|_| b"mtrr-msr-access".as_slice())?;
            for byte in 0..8 {
                let kind = (value >> (byte * 8)) as u8;
                if !types::valid_type(kind) || kind == types::UC_MINUS {
                    return Err(b"mtrr-fixed");
                }
                result.fixed[index * 8 + byte] = kind;
            }
        }
    }
    for index in 0..(cap as u8) {
        let base = unsafe { safe_msr::read(0x200 + 2 * u32::from(index)) }
            .map_err(|_| b"mtrr-msr-access".as_slice())?;
        let mask = unsafe { safe_msr::read(0x201 + 2 * u32::from(index)) }
            .map_err(|_| b"mtrr-msr-access".as_slice())?;
        if !result.append(base, mask) {
            return Err(b"mtrr-range");
        }
    }
    Ok(result)
}
pub(crate) fn adopt_bsp() -> Result<(), &'static [u8]> {
    capabilities()?;
    let table = unsafe { safe_msr::read(PAT_MSR) }.map_err(|_| b"pat-msr-access".as_slice())?;
    if !types::valid_pat(table) {
        return Err(b"pat-invalid");
    }
    // Existing RAM/page-table mappings use the firmware's index zero. Never
    // reinterpret them or rewrite a live BSP table to manufacture this base.
    if table as u8 != types::WB {
        return Err(b"pat-default-wb");
    }
    if types::selector(table, types::UC).is_none() {
        return Err(b"pat-uc-selector");
    }
    if unsafe { r4os_read_cr0() } & ((1 << 30) | (1 << 29)) != 0 {
        return Err(b"pat-cache-control");
    }
    let mtrrs = capture_mtrrs()?;
    if !mtrrs.equivalent(&mtrrs) {
        return Err(b"mtrr-ambiguous");
    }
    unsafe { CONTRACT.0.get().write(Contract { table, mtrrs }) };
    READBACKS[0].store(table, Ordering::Relaxed);
    VERIFIED.store(1, Ordering::Relaxed);
    READY.store(true, Ordering::Release);
    log(0, table, table);
    Ok(())
}
pub(crate) fn qualified(index: u32) -> bool {
    let Some(common) = contract() else {
        return false;
    };
    index < MAX_CPUS as u32
        && VERIFIED.load(Ordering::Acquire) & (1 << index) != 0
        && READBACKS[index as usize].load(Ordering::Relaxed) == common.table
}
pub(crate) fn withdraw(stopped: &super::cpu_lifecycle::Stopped) -> bool {
    if !stopped.current() {
        return false;
    }
    let index = stopped.index() as usize;
    VERIFIED.fetch_and(!(1u64 << index), Ordering::AcqRel);
    READBACKS[index].store(0, Ordering::Release);
    BEFORE[index].store(0, Ordering::Release);
    true
}
/// Confirm the current CPU's real register, not just the saved BSP snapshot.
pub(crate) fn local_table() -> Option<u64> {
    let common = contract()?;
    if !qualified(percpu::current_index()) {
        return None;
    }
    let actual = unsafe { safe_msr::read(PAT_MSR) }.ok()?;
    (actual == common.table).then_some(actual)
}
pub(crate) fn cache_bits(kind: u8, huge: bool) -> Option<u64> {
    types::encode(types::selector(local_table()?, kind)?, huge)
}
pub(crate) fn effective_at(index: u8, physical: u64) -> Option<u8> {
    if index >= 8 {
        return None;
    }
    let table = local_table()?;
    types::effective(
        (table >> (index * 8)) as u8,
        contract()?.mtrrs.at(physical)?,
    )
}
fn program_ap(table: u64) -> Result<(), &'static [u8]> {
    let flags: u64;
    unsafe {
        core::arch::asm!("pushfq", "pop {}", out(reg) flags, options(preserves_flags));
    }
    let cr0 = unsafe { r4os_read_cr0() };
    let cr4 = unsafe { r4os_read_cr4() };
    if flags & (1 << 9) != 0 || cr4 & (1 << 17) != 0 {
        return Err(b"pat-start-window");
    }
    let cr3 = unsafe { r4os_read_cr3() };
    // AP is private, IF=0 and has no device users. WB RAM remains WB at PAT0.
    // No-fill cache mode, writeback/invalidation, and a global-aware full
    // translation flush bracket the complete, validated MSR write.
    let written = unsafe {
        r4os_write_cr0((cr0 | (1 << 30)) & !(1 << 29));
        core::arch::asm!("wbinvd", options(nostack));
        r4os_write_cr4(cr4 & !(1 << 7));
        r4os_write_cr3(cr3);
        let written = safe_msr::write(PAT_MSR, table);
        core::arch::asm!("wbinvd", options(nostack));
        r4os_write_cr3(cr3);
        r4os_write_cr4(cr4);
        r4os_write_cr0(cr0);
        written
    };
    // A rejected write must still restore the exact cache/TLB/control window
    // before the startup owner can reject/quarantine this AP.
    written.map_err(|_| b"pat-msr-access".as_slice())
}
pub(crate) fn init_ap(index: u32) -> Result<(), &'static [u8]> {
    let Some(common) = contract() else {
        return Err(b"pat-unavailable");
    };
    if index == 0
        || index >= MAX_CPUS as u32
        || percpu::current_index() != index
        || percpu::state(index) != 2
    {
        return Err(b"pat-start-window");
    }
    capabilities()?;
    let before = unsafe { safe_msr::read(PAT_MSR) }.map_err(|_| b"pat-msr-access".as_slice())?;
    if !types::valid_pat(before) || before as u8 != types::WB {
        return Err(b"pat-invalid");
    }
    let local = capture_mtrrs()?;
    if !common.mtrrs.equivalent(&local) {
        return Err(b"mtrr-mismatch");
    }
    if before != common.table {
        program_ap(common.table)?;
    }
    let actual = unsafe { safe_msr::read(PAT_MSR) }.map_err(|_| b"pat-msr-access".as_slice())?;
    if actual != common.table || index == crate::build_config::config::AP_PAT_READBACK_TEST_INDEX {
        return Err(b"pat-readback");
    }
    if unsafe { r4os_read_cr0() } & ((1 << 30) | (1 << 29)) != 0 {
        return Err(b"pat-cache-control");
    }
    READBACKS[index as usize].store(actual, Ordering::Relaxed);
    BEFORE[index as usize].store(before, Ordering::Relaxed);
    VERIFIED.fetch_or(1 << index, Ordering::Release);
    // Only BSP prints after the corresponding exact AP record is parked.
    Ok(())
}
pub(crate) fn log(index: u32, before: u64, actual: u64) {
    use crate::boot::log_route::{dec, hex, puts};
    puts(b"[PAT] cpu=");
    dec(u64::from(index));
    puts(b" table=0x");
    hex(actual, 16);
    puts(b" selectors wb=");
    dec(u64::from(types::selector(actual, types::WB).unwrap_or(255)));
    puts(b" uc=");
    dec(u64::from(types::selector(actual, types::UC).unwrap_or(255)));
    puts(b" wc=");
    dec(u64::from(types::selector(actual, types::WC).unwrap_or(255)));
    puts(if before == actual {
        b" sync=same"
    } else {
        b" sync=updated"
    });
    puts(b" mtrr=equivalent\r\n");
}
pub(crate) fn log_ap(index: u32, record_only: bool) {
    if qualified(index) {
        let value = READBACKS[index as usize].load(Ordering::Acquire);
        // This line states the confirmed table; the exact SMP identity/epoch
        // is emitted alongside it by the startup owner.
        let before = BEFORE[index as usize].load(Ordering::Relaxed);
        if record_only {
            use core::fmt::Write;
            let mut line = crate::boot::log::Line::<192>::new();
            write!(
                &mut line,
                "[PAT] cpu={index} table=0x{value:016X} selectors wb={} uc={} wc={} sync={} mtrr=equivalent\r\n",
                types::selector(value, types::WB).unwrap_or(255),
                types::selector(value, types::UC).unwrap_or(255),
                types::selector(value, types::WC).unwrap_or(255),
                if before == value { "same" } else { "updated" }
            )
            .expect("Bounded PAT record");
            line.emit();
        } else {
            log(index, before, value);
        }
    }
}
/// The boot root owner changes device aliases before releasing any AP. Keep
/// old global translations and speculative cache lines out of the new root.
pub(crate) fn switch_boot_root(root: u64) -> bool {
    if percpu::r4os_rust_percpu_control(6, 0, 0, 0) != 1 {
        return false;
    }
    let cr4 = unsafe { r4os_read_cr4() };
    if cr4 & (1 << 17) != 0 {
        return false;
    }
    let saved = super::runtime::save_local();
    unsafe {
        core::arch::asm!("wbinvd", options(nostack));
        r4os_write_cr4(cr4 & !(1 << 7));
        r4os_write_cr3(root);
        core::arch::asm!("wbinvd", options(nostack));
        r4os_write_cr4(cr4);
    }
    super::runtime::restore_local(saved);
    true
}
