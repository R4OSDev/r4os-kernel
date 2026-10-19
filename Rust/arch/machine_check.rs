//! Necessary CPU-local MCA capture. The terminal path owns a separate IST,
//! protected-read slot and resident atomics, never the normal crash runtime.
use super::{
    interrupt_frame::InterruptFrame, machine_check_capture as capture, machine_check_store::Slot,
    percpu, runtime, safe_msr, smp_policy::MAX_CPUS,
};
use crate::{
    contract as c,
    platform::{cpu_capabilities as capabilities, cpu_capabilities_store::Store, cpu_topology},
};
use core::{
    ptr,
    sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
};

#[cfg(target_os = "none")]
core::arch::global_asm!(include_str!("machine_check.S"), options(att_syntax));
unsafe extern "C" {
    fn r4os_mce_read(id: u32, out: *mut u64) -> i32;
    static r4os_mce_read_fault: u8;
    static r4os_mce_read_resume: u8;
}
struct ReadSlot {
    active: AtomicBool,
    register: AtomicU32,
    epoch: AtomicU64,
}
impl ReadSlot {
    const fn new() -> Self {
        Self {
            active: AtomicBool::new(false),
            register: AtomicU32::new(0),
            epoch: AtomicU64::new(0),
        }
    }
}
static CONFIG: [Store<5>; MAX_CPUS] = [const { Store::new() }; MAX_CPUS];
static RECORDS: [Slot; MAX_CPUS] = [const { Slot::new() }; MAX_CPUS];
static READS: [ReadSlot; MAX_CPUS] = [const { ReadSlot::new() }; MAX_CPUS];
static TERMINAL: AtomicBool = AtomicBool::new(false);
static OUTPUT: AtomicBool = AtomicBool::new(false);

pub(crate) fn terminal_active() -> bool {
    TERMINAL.load(Ordering::Acquire)
}
fn cr4() -> u64 {
    let value;
    unsafe {
        core::arch::asm!("mov {}, cr4", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
}
fn configured(index: u32, epoch: u64) -> c::CpuMachineInfo {
    let mut value = c::CpuMachineInfo {
        cpu_index: index,
        epoch,
        bank_limit: capture::LIMIT as u32,
        ..Default::default()
    };
    if let Some(config) = CONFIG[index as usize]
        .snapshot()
        .filter(|s| s.epoch == epoch)
    {
        let w = config.words;
        value.flags = w[0] as u32;
        value.profile = (w[0] >> 32) as u32;
        value.family = w[1] as u32;
        value.model = (w[1] >> 32) as u32;
        value.stepping = w[2] as u32;
        value.apic_id = (w[2] >> 32) as u32;
        value.vendor_name[..8].copy_from_slice(&w[3].to_le_bytes());
        value.vendor_name[8..].copy_from_slice(&(w[4] as u32).to_le_bytes());
    }
    if cr4() & (1 << 6) != 0 {
        value.flags |= c::cpu_machine_flag_mce_enabled;
    }
    value
}

/// Exact local post-IDT/capability boot owner only, before CPU admission.
/// Scan preserves all existing controls/status, including firmware error data.
pub(crate) fn prepare(index: u32, epoch: u64, raw: &crate::platform::cpu_capabilities_decode::Raw) {
    let flags = runtime::save_local();
    if index != percpu::current_index() || index as usize >= MAX_CPUS || epoch == 0 {
        runtime::restore_local(flags);
        return;
    }
    let vendor = raw.vendor();
    let (family, model, stepping) = raw.signature().unwrap_or((0, 0, 0));
    let mut bits = c::cpu_machine_flag_prepared;
    for (leaf, reg, bit, flag) in [
        (1, 3, 7, c::cpu_machine_flag_mce),
        (1, 3, 14, c::cpu_machine_flag_mca),
        (1, 3, 5, c::cpu_machine_flag_msr),
        (1, 2, 31, c::cpu_machine_flag_virtual),
    ] {
        if capabilities::bit(raw, leaf, 0, reg, bit) == Some(true) {
            bits |= flag;
        }
    }
    let smca =
        vendor == *b"AuthenticAMD" && capabilities::bit(raw, 0x8000_0007, 0, 1, 3) == Some(true);
    if smca {
        bits |= c::cpu_machine_flag_smca_advertised;
    }
    let profile = capture::profile(&vendor, family, model, stepping, smca);
    let apic = cpu_topology::snapshot(index).map_or(0, |v| v.apic_id);
    let words = [
        u64::from(bits) | (u64::from(profile) << 32),
        u64::from(family) | (u64::from(model) << 32),
        u64::from(stepping) | (u64::from(apic) << 32),
        u64::from_le_bytes(vendor[..8].try_into().unwrap()),
        u64::from(u32::from_le_bytes(vendor[8..].try_into().unwrap())),
    ];
    if CONFIG[index as usize].publish(epoch, words).is_err() {
        runtime::restore_local(flags);
        return;
    }
    let slot = &RECORDS[index as usize];
    if let Some(sequence) = slot.begin(false) {
        let mut base = configured(index, epoch);
        base.origin = 1;
        base.sequence = sequence;
        let mut unsafe_error = false;
        let mut value = capture::collect(
            base,
            |id| unsafe { safe_msr::read_exact(index, epoch, id).map_err(|_| ()) },
            |bank| {
                // Never newly enable #MC over a known uncorrected/context/deferred
                // boot record. This scan is diagnostic, never repair or clearing.
                unsafe_error |= capture::unsafe_boot_bank(profile, &bank);
                slot.bank(bank);
            },
        );
        if capture::may_enable(&value, unsafe_error) {
            // Documented architectural CR4.MCE only, after resident handler,
            // config and safe-read preparation. No MCA MSR enable/clear write.
            let control = cr4() | (1 << 6);
            unsafe {
                core::arch::asm!("mov cr4, {}", in(reg) control, options(nostack, preserves_flags));
            }
        }
        if cr4() & (1 << 6) != 0 {
            value.flags |= c::cpu_machine_flag_mce_enabled;
        }
        slot.finish(value);
    }
    runtime::restore_local(flags);
}

unsafe fn exception_read(index: u32, epoch: u64, id: u32) -> Result<u64, ()> {
    let slot = &READS[index as usize];
    // Only the single claimed #MC body can execute this on this CPU. Neither
    // ordinary safe_msr admission nor its interrupted slot is reused.
    if slot.active.load(Ordering::Acquire) {
        return Err(());
    }
    slot.register.store(id, Ordering::Relaxed);
    slot.epoch.store(epoch, Ordering::Relaxed);
    slot.active.store(true, Ordering::Release);
    let mut value = 0;
    let result = unsafe { r4os_mce_read(id, &mut value) };
    slot.active.store(false, Ordering::Release);
    if result == 0 { Ok(value) } else { Err(()) }
}
/// The exclusive complete GP frame is on IST2, separate from the #MC IST3.
/// Only the exact labelled read instruction/current CPU/epoch/register resumes.
pub(crate) unsafe fn recover(frame: *mut InterruptFrame) -> bool {
    if frame.is_null() || !frame.is_aligned() || !terminal_active() {
        return false;
    }
    let index = percpu::current_index();
    let Some(slot) = READS.get(index as usize) else {
        return false;
    };
    unsafe {
        if !slot.active.load(Ordering::Acquire)
            || (*frame).vector != 13
            || (*frame).error_code != 0
            || (*frame).cs != 8
            || (*frame).rflags & (1 << 9) != 0
            || (*frame).rip != ptr::addr_of!(r4os_mce_read_fault) as u64
            || (*frame).rcx != u64::from(slot.register.load(Ordering::Relaxed))
            || cpu_topology::epoch(index) != slot.epoch.load(Ordering::Relaxed)
        {
            return false;
        }
        ptr::addr_of_mut!((*frame).rip).write(ptr::addr_of!(r4os_mce_read_resume) as u64);
    }
    true
}

/// Never returns to the failed hardware context, even for corrected/unknown
/// bank data. No regular runtime, owner, allocation or module callback here.
pub(crate) unsafe fn fatal(frame: *const InterruptFrame) -> ! {
    TERMINAL.store(true, Ordering::Release);
    let index = percpu::current_index();
    let slot = &RECORDS[index as usize];
    let Some(sequence) = slot.begin(true) else {
        crate::boot::fatal::halt();
    };
    let epoch = cpu_topology::epoch(index);
    let mut base = configured(index, epoch);
    base.origin = 2;
    base.sequence = sequence;
    base.rip = unsafe { ptr::addr_of!((*frame).rip).read() };
    base.rflags = unsafe { ptr::addr_of!((*frame).rflags).read() };
    base.rsp = unsafe { super::tables::interrupted_rsp(frame) };
    let value = capture::collect(
        base,
        |id| unsafe { exception_read(index, epoch, id) },
        |bank| slot.bank(bank),
    );
    slot.finish(value);
    if OUTPUT
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        terminal_output(&value, slot);
    }
    crate::boot::fatal::halt()
}
fn terminal_output(value: &c::CpuMachineInfo, slot: &Slot) {
    let Some(mut out) = crate::boot::serial::emergency() else {
        return;
    };
    out.write(b"\r\n[CRASH] cause=Machine Check stop=");
    out.hex(u64::from(crate::boot::crash::stop_cause(4)));
    out.write(b"\r\n[CRASH] Machine Check terminal; raw resident snapshot CPU=");
    out.hex(u64::from(value.cpu_index));
    out.write(b" epoch=");
    out.hex(value.epoch);
    out.write(b" sequence=");
    out.hex(value.sequence);
    out.write(b" flags=");
    out.hex(u64::from(value.flags));
    out.write(b" MCG_CAP=");
    out.hex(value.mcg_cap);
    out.write(b" MCG_STATUS=");
    out.hex(value.mcg_status);
    out.write(b" RIP=");
    out.hex(value.rip);
    out.write(b" RSP=");
    out.hex(value.rsp);
    out.write(b" banks=");
    out.hex(u64::from(value.bank_count));
    out.write(b"\r\n");
    for index in 0..value.stored_banks {
        if let Ok(bank) = slot.get_bank(value.epoch, value.sequence, index) {
            out.write(b"[CRASH] MCA bank=");
            out.hex(u64::from(index));
            out.write(b" flags=");
            out.hex(u64::from(bank.flags));
            for (name, raw) in [
                (b" STATUS=".as_slice(), bank.status),
                (b" AFTER=", bank.status_after),
                (b" ADDR=", bank.address),
                (b" MISC=", bank.misc),
                (b" CONFIG=", bank.config),
                (b" IPID=", bank.ipid),
                (b" SYND=", bank.syndrome),
            ] {
                out.write(name);
                out.hex(raw);
            }
            out.write(b"\r\n");
        }
    }
    out.write(b"[CRASH] No status cleared; no generic recovery. Busy/released COM1 keeps RAM evidence only.\r\n");
}
pub(crate) fn info(index: u32) -> Result<c::CpuMachineInfo, i32> {
    if index >= cpu_topology::count() {
        return Err(c::cpu_capabilities_error_index);
    }
    let mut value = RECORDS[index as usize].info()?;
    value.configured_cpus = cpu_topology::count();
    Ok(value)
}
pub(crate) fn bank(
    index: u32,
    epoch: u64,
    sequence: u64,
    ordinal: u32,
) -> Result<c::CpuMachineBank, i32> {
    if index >= cpu_topology::count() {
        return Err(c::cpu_capabilities_error_index);
    }
    RECORDS[index as usize].get_bank(epoch, sequence, ordinal)
}
