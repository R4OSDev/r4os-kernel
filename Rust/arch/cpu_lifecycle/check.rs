//! One explicit native session. Workers borrow no caller stack/output; exact
//! ledger retirement precedes reuse of their resident atomic mailboxes.
use super::*;
use crate::{
    memory::owner,
    sched::task::{
        cpu_drain,
        diagnostic::{self, Ledger},
        task_types::Role,
    },
};
use core::{cell::UnsafeCell, ptr};
static BUSY: AtomicBool = AtomicBool::new(false);
struct Cell(UnsafeCell<Ledger<8>>);
// One lifecycle writer owns the ledger; workers touch only static atomics.
unsafe impl Sync for Cell {}
static LEDGER: Cell = Cell(UnsafeCell::new(Ledger::EMPTY));
static READY: AtomicBool = AtomicBool::new(false);
static RELEASE: AtomicBool = AtomicBool::new(false);
static PAGE_GO: AtomicBool = AtomicBool::new(false);
static PAGE_FIRE: AtomicBool = AtomicBool::new(false);
static PAGE_DONE: AtomicBool = AtomicBool::new(false);
static PAGE_OK: AtomicBool = AtomicBool::new(false);
static RT_GO: AtomicBool = AtomicBool::new(false);
static RT_READY: AtomicBool = AtomicBool::new(false);
static RT_REFUSED: AtomicBool = AtomicBool::new(false);
static RT_DONE: AtomicBool = AtomicBool::new(false);
static RT_OK: AtomicBool = AtomicBool::new(false);
static MODE: AtomicU32 = AtomicU32::new(0);
static PCI_READY: AtomicU32 = AtomicU32::new(0);
static PCI_GO: AtomicBool = AtomicBool::new(false);
static PCI_AVAILABLE: AtomicBool = AtomicBool::new(false);
static PCI_ADDRESS: [AtomicU32; 2] = [const { AtomicU32::new(0) }; 2];
static PCI_EXPECTED: [AtomicU32; 2] = [const { AtomicU32::new(0) }; 2];
static PCI_MATCHED: [AtomicU32; 2] = [const { AtomicU32::new(0) }; 2];
#[repr(align(4096))]
struct Page([u8; 4096]);
static OWN_PAGE: Page = Page([0x5a; 4096]);
fn flags() -> u64 {
    let value;
    unsafe {
        core::arch::asm!("pushfq", "pop {}", out(reg) value, options(preserves_flags));
    }
    value
}
fn spin_until(test: impl FnMut() -> bool) -> bool {
    bounded_wait(test)
}
fn prepare_pci() {
    use crate::platform::{pci, pci_inventory};
    let mut count = 0;
    for index in 0..pci_inventory::count() {
        let Some(device) = pci_inventory::device_at(index) else {
            continue;
        };
        let expected = u32::from(device.vendor_id) | u32::from(device.device_id) << 16;
        if expected == u32::MAX
            || expected == 0
            || count != 0 && expected == PCI_EXPECTED[0].load(Ordering::Relaxed)
        {
            continue;
        }
        // Admit only actual CF8/CFC readback; an ECAM descriptor alone does
        // not prove that this machine also implements the legacy transport.
        if pci::read_config32(device.bus, device.device, device.function, 0) != expected {
            continue;
        }
        PCI_ADDRESS[count].store(
            u32::from(device.bus) << 16
                | u32::from(device.device) << 8
                | u32::from(device.function),
            Ordering::Relaxed,
        );
        PCI_EXPECTED[count].store(expected, Ordering::Relaxed);
        count += 1;
        if count == 2 {
            break;
        }
    }
    PCI_AVAILABLE.store(count == 2, Ordering::Release);
}
fn pci_worker(slot: usize) {
    PCI_READY.fetch_or(1 << slot, Ordering::Release);
    if !spin_until(|| PCI_GO.load(Ordering::Acquire)) || !PCI_AVAILABLE.load(Ordering::Acquire) {
        return;
    }
    let before = flags();
    let address = PCI_ADDRESS[slot].load(Ordering::Acquire);
    let expected = PCI_EXPECTED[slot].load(Ordering::Acquire);
    let mut matched = 0;
    for _ in 0..1024 {
        if crate::platform::pci::read_config32(
            (address >> 16) as u8,
            (address >> 8) as u8,
            address as u8,
            0,
        ) != expected
        {
            break;
        }
        matched += 1;
    }
    if flags() & (1 << 9) == before & (1 << 9) {
        PCI_MATCHED[slot].store(matched, Ordering::Release);
    }
}
fn pci_report() -> bool {
    use core::fmt::Write;
    let available = PCI_AVAILABLE.load(Ordering::Acquire);
    let matched = PCI_MATCHED[0].load(Ordering::Acquire) + PCI_MATCHED[1].load(Ordering::Acquire);
    let ok = !available || matched == 2048;
    let mut line = crate::boot::log::Line::<96>::new();
    write!(
        &mut line,
        "[CPUCTL] legacy-pci available={} matched={matched} result={}\r\n",
        if available { "yes" } else { "no" },
        if ok { "OK" } else { "FAILED" }
    )
    .expect("Bounded PCI diagnostic line");
    crate::boot::log::text(line.as_bytes());
    crate::boot::log_route::serial_write_raw(line.as_bytes());
    ok
}
unsafe extern "C" fn kernel_worker() {
    let released = unsafe { cpu_drain::diagnostic_release_current_binding() };
    READY.store(released, Ordering::Release);
    let _ = spin_until(|| RELEASE.load(Ordering::Acquire));
}
unsafe extern "C" fn page_worker() {
    if !spin_until(|| PAGE_GO.load(Ordering::Acquire)) {
        return;
    }
    if MODE.load(Ordering::Acquire) == 1 {
        pci_worker(1);
    }
    let _pages = owner::Guard::acquire(6);
    READY.store(true, Ordering::Release);
    if MODE.load(Ordering::Acquire) == 0 {
        let _ = spin_until(|| RELEASE.load(Ordering::Acquire));
    } else if spin_until(|| PAGE_FIRE.load(Ordering::Acquire)) {
        let mut receipt = super::super::tlb::Result::default();
        unsafe {
            super::super::tlb::r4os_rust_tlb_invalidate(
                OWN_PAGE.0.as_ptr() as u64,
                1,
                &mut receipt,
            );
        }
        PAGE_OK.store(
            receipt.ok
                && receipt.generation != 0
                && receipt.target_mask & 3 == 3
                && receipt.acknowledgement_mask & receipt.target_mask == receipt.target_mask
                && receipt.offline_mask == 0,
            Ordering::Release,
        );
    }
    PAGE_DONE.store(true, Ordering::Release);
}
unsafe extern "C" fn runtime_worker() {
    pci_worker(0);
    RT_READY.store(true, Ordering::Release);
    if !spin_until(|| RT_GO.load(Ordering::Acquire)) {
        return;
    }
    let saved = super::super::runtime::save_local();
    let refused =
        super::super::runtime::RuntimeGuard::try_acquire().is_none() && flags() & (1 << 9) == 0;
    RT_REFUSED.store(refused, Ordering::Release);
    let runtime = super::super::runtime::RuntimeGuard::acquire();
    RT_OK.store(refused && flags() & (1 << 9) == 0, Ordering::Release);
    drop(runtime);
    RT_OK.fetch_and(flags() & (1 << 9) == 0, Ordering::Release);
    super::super::runtime::restore_local(saved);
    RT_DONE.store(true, Ordering::Release);
}
fn reset() {
    PCI_READY.store(0, Ordering::Relaxed);
    PCI_GO.store(false, Ordering::Relaxed);
    PCI_AVAILABLE.store(false, Ordering::Relaxed);
    for value in &PCI_MATCHED {
        value.store(0, Ordering::Relaxed);
    }
    for value in [
        &READY,
        &RELEASE,
        &PAGE_GO,
        &PAGE_FIRE,
        &PAGE_DONE,
        &PAGE_OK,
        &RT_GO,
        &RT_READY,
        &RT_REFUSED,
        &RT_DONE,
        &RT_OK,
    ] {
        value.store(false, Ordering::Relaxed);
    }
}
fn case(report: &mut c::ProgramKernelCpuLifecycleCheck, bit: u32, ok: bool) {
    if ok {
        report.passed |= bit;
    } else {
        report.failed |= bit;
    }
}
pub(crate) fn run() -> c::ProgramKernelCpuLifecycleCheck {
    use super::super::{percpu, runtime::RuntimeGuard};
    let mut r = c::ProgramKernelCpuLifecycleCheck {
        selected: c::cpu_lifecycle_check_case_all,
        ..Default::default()
    };
    let claim = match claim() {
        Ok(value) => value,
        Err(reason) => {
            r.status = c::cpu_lifecycle_check_status_busy;
            r.reason = reason_value(reason);
            return r;
        }
    };
    let mask = percpu::r4os_rust_percpu_control(14, 0, 0, 0);
    if mask.count_ones() < 3 || mask & 7 != 7 {
        return r;
    }
    if BUSY
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        r.status = c::cpu_lifecycle_check_status_busy;
        return r;
    }
    let cpu = 1;
    let helper = 2;
    r.cpu_index = cpu;
    r.configured_cpus = crate::platform::cpu_topology::count();
    r.available_before = mask.count_ones();
    r.old_epoch = crate::platform::cpu_topology::epoch(cpu);
    r.capacity_before = capacity_generation();
    unsafe {
        LEDGER.0.get().write(Ledger::EMPTY);
    }
    let ledger = unsafe { &mut *LEDGER.0.get() };
    reset();
    let fixed = ledger.create(b"cpu-offline-fixed", kernel_worker, Role::Batch, Some(cpu));
    let fixed_ok = !fixed.is_null()
        && offline(&claim, cpu, r.old_epoch) == Err(Reason::Fixed)
        && percpu::state(cpu) == 4
        && ledger.finish_task(fixed, true);
    case(&mut r, c::cpu_lifecycle_check_case_fixed, fixed_ok);
    if fixed_ok {
        reset();
        let worker = ledger.create(b"cpu-offline-kernel", kernel_worker, Role::Batch, Some(cpu));
        let ready = !worker.is_null()
            && diagnostic::start(worker)
            && spin_until(|| READY.load(Ordering::Acquire));
        let outcome = if ready {
            offline(&claim, cpu, r.old_epoch)
        } else {
            Err(Reason::Resources)
        };
        RELEASE.store(true, Ordering::Release);
        let reaped = !worker.is_null() && ledger.finish_task(worker, !ready);
        case(
            &mut r,
            c::cpu_lifecycle_check_case_kernel,
            ready && outcome == Err(Reason::KernelContext) && percpu::state(cpu) == 4 && reaped,
        );
    }
    if r.failed == 0 {
        reset();
        MODE.store(0, Ordering::Release);
        let worker = ledger.create(
            b"cpu-offline-page-owner",
            page_worker,
            Role::Batch,
            Some(helper),
        );
        let started = !worker.is_null() && diagnostic::start(worker);
        PAGE_GO.store(true, Ordering::Release);
        let ready = started && spin_until(|| READY.load(Ordering::Acquire));
        // Both refusals must leave IF unchanged and must not release the
        // helper's foreign PageTableOwner, including the IF=0 countercase.
        let on = ready
            && flags() & (1 << 9) != 0
            && owner::Guard::try_acquire(6).is_none()
            && flags() & (1 << 9) != 0;
        let saved = super::super::runtime::save_local();
        let off = ready && owner::Guard::try_acquire(6).is_none() && flags() & (1 << 9) == 0;
        super::super::runtime::restore_local(saved);
        r.if_checks += u32::from(on) + u32::from(off);
        let outcome = if ready {
            offline(&claim, cpu, r.old_epoch)
        } else {
            Err(Reason::Resources)
        };
        RELEASE.store(true, Ordering::Release);
        let reaped = !worker.is_null() && ledger.finish_task(worker, !ready);
        case(
            &mut r,
            c::cpu_lifecycle_check_case_page_owner,
            ready
                && on
                && off
                && outcome == Err(Reason::PageOwner)
                && percpu::state(cpu) == 4
                && reaped,
        );
    }
    if r.failed == 0 {
        reset();
        MODE.store(1, Ordering::Release);
        prepare_pci();
        let page = ledger.create(
            b"cpu-runtime-tlb-source",
            page_worker,
            Role::Batch,
            Some(helper),
        );
        let rt = ledger.create(
            b"cpu-runtime-if0-waiter",
            runtime_worker,
            Role::Batch,
            Some(cpu),
        );
        let started =
            !page.is_null() && !rt.is_null() && diagnostic::start(page) && diagnostic::start(rt);
        PAGE_GO.store(true, Ordering::Release);
        let participants = started && spin_until(|| PCI_READY.load(Ordering::Acquire) == 3);
        PCI_GO.store(true, Ordering::Release);
        let ready = participants
            && spin_until(|| READY.load(Ordering::Acquire) && RT_READY.load(Ordering::Acquire));
        let progress;
        {
            let _runtime = RuntimeGuard::acquire();
            RT_GO.store(true, Ordering::Release);
            let refused = ready && spin_until(|| RT_REFUSED.load(Ordering::Acquire));
            PAGE_FIRE.store(true, Ordering::Release);
            progress = refused
                && spin_until(|| PAGE_DONE.load(Ordering::Acquire))
                && PAGE_OK.load(Ordering::Acquire)
                && super::super::runtime::r4os_rust_runtime_control(8, 0) != 0;
        }
        let joined =
            spin_until(|| RT_DONE.load(Ordering::Acquire)) && RT_OK.load(Ordering::Acquire);
        r.if_checks += u32::from(joined);
        RELEASE.store(true, Ordering::Release);
        let clean_page = !page.is_null() && ledger.finish_task(page, !ready);
        let clean_rt = !rt.is_null() && ledger.finish_task(rt, !ready);
        let pci_ok = pci_report();
        case(
            &mut r,
            c::cpu_lifecycle_check_case_runtime_tlb,
            progress && joined && clean_page && clean_rt && pci_ok,
        );
    }
    if r.failed == 0 {
        let outcome = offline(&claim, cpu, r.old_epoch);
        if let Some(info) = snapshot(cpu) {
            r.available_offline = info.available_cpus;
            r.parked_epoch = info.parked_epoch;
            r.stopped_epoch = info.stopped_epoch;
            r.tlb_generation = info.tlb_generation;
            r.switch_serial = info.switch_serial;
        }
        let stopped = outcome.is_ok()
            && stopped_receipt(cpu, r.old_epoch).is_some()
            && r.available_offline + 1 == r.available_before;
        let restarted = stopped && online(&claim, cpu, r.old_epoch).is_ok();
        r.new_epoch = crate::platform::cpu_topology::epoch(cpu);
        let cycle_ok = restarted && r.new_epoch == r.old_epoch + 1 && percpu::state(cpu) == 4;
        let stale_ok = restarted
            && offline(&claim, cpu, r.old_epoch) == Err(Reason::Epoch)
            && online(&claim, cpu, r.old_epoch) == Err(Reason::Epoch)
            && percpu::state(cpu) == 4;
        case(&mut r, c::cpu_lifecycle_check_case_cycle, cycle_ok);
        case(&mut r, c::cpu_lifecycle_check_case_stale_epoch, stale_ok);
        if let Err(reason) = outcome {
            r.reason = reason_value(reason);
        }
    }
    RELEASE.store(true, Ordering::Release);
    PAGE_GO.store(true, Ordering::Release);
    PAGE_FIRE.store(true, Ordering::Release);
    RT_GO.store(true, Ordering::Release);
    PCI_GO.store(true, Ordering::Release);
    let clean = ledger.cleanup_except(ptr::null_mut());
    let (created, reaped, live) = ledger.summary();
    r.tasks_created = created as u32;
    r.tasks_reaped = reaped as u32;
    r.cleanup_errors = u32::from(!clean || live != 0);
    r.available_after = percpu::r4os_rust_percpu_control(14, 0, 0, 0).count_ones();
    r.capacity_after = capacity_generation();
    r.failed |= r.selected & !r.passed;
    r.status = if r.failed == 0 && r.cleanup_errors == 0 && r.available_after == r.available_before
    {
        c::cpu_lifecycle_check_status_passed
    } else {
        c::cpu_lifecycle_check_status_failed
    };
    if r.cleanup_errors != 0 {
        // The resident ledger/worker mailboxes remain owned; neither a new
        // check nor CPU mutation may overwrite an uncertain live session.
        core::mem::forget(claim);
    } else {
        BUSY.store(false, Ordering::Release);
    }
    r
}
