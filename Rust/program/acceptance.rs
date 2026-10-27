//! Original R4L timer/IPI/cooperative and parallel-program acceptance flows.
//! Private witness state has one native owner; the same exact handles, deadlines,
//! caller unwind, event enrollment and resource acknowledgments remain in use.
use super::{
    completions, entry, gui_events, gui_view, launch, payload, payload_types, process, program_api,
    reaper, registry, registry_state, runtime_tables, service_api, thread_retire,
};
use crate::{
    arch::{percpu, pit, smp_policy::MAX_CPUS},
    boot::foundation,
    contract as c,
    fs::{
        drive::{self, Drive},
        vfs_types::Bytes,
    },
    loader::registry_control,
    platform::{monotonic, timer},
    sched::{
        scheduler, sync_objects,
        sync_types::{Event, WaitQueue},
        sync_wait,
        task::task_api,
        task_context,
    },
};
use core::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    ptr,
    sync::atomic::{AtomicBool, AtomicPtr, AtomicU8, AtomicU32, AtomicU64, Ordering},
};
struct EventCell(UnsafeCell<Event>);
// SAFETY: the existing single boot probe initializes the queue only before its
// witness is published; operations use the native runtime-serialized Event.
unsafe impl Sync for EventCell {}
static EVENT: EventCell = EventCell(UnsafeCell::new(Event {
    signaled: false,
    mode: 0,
    queue: WaitQueue::EMPTY,
}));
static FLAG: AtomicPtr<u64> = AtomicPtr::new(ptr::null_mut());
static MODE: AtomicU8 = AtomicU8::new(0);
static STARTED: AtomicU8 = AtomicU8::new(0);
static DONE: AtomicU8 = AtomicU8::new(0);
static ABORT: AtomicU8 = AtomicU8::new(0);
static FAILURES: AtomicU32 = AtomicU32::new(0);
static CPU: AtomicU32 = AtomicU32::new(u32::MAX);
// One bounded IPI case may be woken by a timer before the reschedule IPI.
// Keep the same R4L work alive and re-enroll; a timer wake never proves IPI.
const IPI_WAKE_LIMIT: u32 = 8;
static IPI_BASE: AtomicU64 = AtomicU64::new(0);
static IPI_WAKES: AtomicU32 = AtomicU32::new(0);
static IPI_TIMER_RACES: AtomicU32 = AtomicU32::new(0);
static COOPERATIVE: AtomicU64 = AtomicU64::new(0);
static COOP_PROGRESS_NS: AtomicU64 = AtomicU64::new(0);
static COOP_MAX_WORK_NS: AtomicU64 = AtomicU64::new(0);
static COOP_MAX_GAP_NS: AtomicU64 = AtomicU64::new(0);
struct Scenario {
    ok: bool,
    exit: i32,
    timer: u64,
    ipi: u64,
    cooperative: u64,
    ipi_wakes: u32,
    ipi_timer_races: u32,
    progress_ns: u64,
    max_work_ns: u64,
    max_gap_ns: u64,
}
impl Scenario {
    const EMPTY: Self = Self {
        ok: false,
        exit: -1,
        timer: 0,
        ipi: 0,
        cooperative: 0,
        ipi_wakes: 0,
        ipi_timer_races: 0,
        progress_ns: 0,
        max_work_ns: 0,
        max_gap_ns: 0,
    };
}

fn span(s: &[u8]) -> Bytes {
    Bytes {
        data: s.as_ptr(),
        len: s.len(),
    }
}
fn log(s: &[u8]) {
    foundation::puts(s)
}
fn dec(n: u64) {
    foundation::dec(n)
}
fn expired(start: monotonic::Stamp, tick: u64) -> bool {
    if let Some(ns) = monotonic::elapsed(start, monotonic::capture()) {
        ns >= 5_000_000_000
    } else {
        timer::tick_count().wrapping_sub(tick) >= 5 * u64::from(pit::DEFAULT_HZ)
    }
}
fn wait_witness(start: monotonic::Stamp, tick: u64) -> bool {
    while DONE.load(Ordering::Acquire) == 0 {
        if expired(start, tick) {
            return false;
        }
        scheduler::yield_task();
    }
    true
}
unsafe fn signal() {
    unsafe {
        sync_objects::r4os_rust_sync_event(EVENT.0.get(), 0, 0);
    }
}
unsafe fn abort() {
    ABORT.store(1, Ordering::Release);
    unsafe {
        let flag = FLAG.load(Ordering::Relaxed);
        if !flag.is_null() {
            AtomicU64::from_ptr(flag).store(2, Ordering::Release);
        }
        signal();
    }
}
fn failure() {
    FAILURES.fetch_add(1, Ordering::AcqRel);
}
fn retire() -> ! {
    scheduler::r4os_rust_scheduler_control(13, 0, 0);
    panic!("Retired R4L witness returned")
}
unsafe extern "C" fn witness() {
    unsafe {
        CPU.store(percpu::current_index(), Ordering::Release);
        let mode = MODE.load(Ordering::Relaxed);
        if mode < 2 {
            STARTED.store(1, Ordering::Release);
        }
        let flag = FLAG.load(Ordering::Relaxed);
        if flag.is_null() {
            failure();
            DONE.store(1, Ordering::Release);
            retire();
        }
        let flag = AtomicU64::from_ptr(flag);
        match mode {
            0 => {
                while flag.load(Ordering::Acquire) != 1 && ABORT.load(Ordering::Acquire) == 0 {
                    scheduler::yield_task();
                }
            }
            1 => {
                loop {
                    if sync_objects::r4os_rust_sync_event(EVENT.0.get(), 2, u64::MAX) != 1 {
                        failure();
                        break;
                    }
                    if ABORT.load(Ordering::Acquire) != 0 {
                        break;
                    }
                    let observed = preemption(percpu::current_index());
                    if observed
                        .reschedule_ipi_switches
                        .wrapping_sub(IPI_BASE.load(Ordering::Acquire))
                        != 0
                    {
                        break;
                    }
                    // The timer legitimately won this wake. Do not finish the
                    // library's sentinel loop or claim IPI; consume/reset the
                    // manual event before the next exact wait enrollment.
                    IPI_TIMER_RACES.fetch_add(1, Ordering::Relaxed);
                    sync_objects::r4os_rust_sync_event(EVENT.0.get(), 1, 0);
                }
            }
            2 | 3 => {
                let token = task_context::enter();
                if token.admitted() {
                    let buffer = crate::memory::heap::r4os_rust_heap_alloc(64 * 1024, 1);
                    if !buffer.is_null() {
                        let mut value = 0u8;
                        let progress = monotonic::capture();
                        let mut previous = progress;
                        STARTED.store(1, Ordering::Release);
                        while flag.load(Ordering::Acquire) != 1
                            && ABORT.load(Ordering::Acquire) == 0
                        {
                            let work = monotonic::capture();
                            value = value.wrapping_add(1);
                            ptr::write_bytes(buffer, value, 64 * 1024);
                            if let Some(ns) = monotonic::elapsed(work, monotonic::capture()) {
                                COOP_MAX_WORK_NS.fetch_max(ns, Ordering::Relaxed);
                            }
                            if scheduler::safe_reschedule() {
                                COOPERATIVE.fetch_add(1, Ordering::Relaxed);
                            }
                            let reached = monotonic::capture();
                            if let Some(ns) = monotonic::elapsed(previous, reached) {
                                // Includes actual suspension; this is an observed
                                // probe interval, never a real-time guarantee.
                                COOP_MAX_GAP_NS.fetch_max(ns, Ordering::Relaxed);
                            }
                            previous = reached;
                            if buffer.read() != value || buffer.add(64 * 1024 - 1).read() != value {
                                failure();
                                break;
                            }
                        }
                        if let Some(ns) = monotonic::elapsed(progress, monotonic::capture()) {
                            COOP_PROGRESS_NS.store(ns, Ordering::Relaxed);
                        }
                        crate::memory::heap::r4os_rust_heap_free(buffer, 64 * 1024);
                    } else {
                        failure();
                    }
                    task_context::leave(token);
                } else {
                    failure();
                }
                STARTED.store(1, Ordering::Release);
            }
            _ => unreachable!(),
        }
        if ABORT.load(Ordering::Acquire) == 0 && flag.load(Ordering::Acquire) == 1 {
            flag.store(2, Ordering::Release);
        } else if ABORT.load(Ordering::Acquire) == 0 {
            failure();
        }
        DONE.store(1, Ordering::Release);
        retire();
    }
}
fn ready(handle: c::ProgramProcessHandle) -> bool {
    unsafe { completions::r4os_rust_program_completion_access(0, handle, ptr::null_mut()) != 0 }
}
unsafe fn preemption(cpu: u32) -> scheduler::CpuPreemptionStats {
    unsafe {
        let mut out = MaybeUninit::uninit();
        scheduler::r4os_rust_scheduler_cpu_stats(cpu, out.as_mut_ptr());
        out.assume_init()
    }
}
fn flags() -> u64 {
    let flags;
    unsafe {
        core::arch::asm!("pushfq","pop {}",out(reg)flags,options(preserves_flags));
    }
    flags
}
unsafe fn scenario(file: &launch::File, working: *mut Drive, cpu: u32, mode: u8) -> Scenario {
    unsafe {
        let flag = FLAG.load(Ordering::Relaxed);
        if flag.is_null() {
            return Scenario::EMPTY;
        }
        let flag = AtomicU64::from_ptr(flag);
        let mut before = preemption(cpu);
        let start = monotonic::capture();
        let tick = timer::tick_count();
        let entry_flags = flags();
        let entry_timer = timer::deadline_stats();
        MODE.store(mode, Ordering::Relaxed);
        EVENT.0.get().write(Event {
            signaled: false,
            mode: 0,
            queue: WaitQueue::EMPTY,
        });
        STARTED.store(0, Ordering::Release);
        DONE.store(0, Ordering::Release);
        ABORT.store(0, Ordering::Release);
        FAILURES.store(0, Ordering::Release);
        CPU.store(u32::MAX, Ordering::Release);
        COOPERATIVE.store(0, Ordering::Relaxed);
        IPI_BASE.store(0, Ordering::Relaxed);
        IPI_WAKES.store(0, Ordering::Relaxed);
        IPI_TIMER_RACES.store(0, Ordering::Relaxed);
        COOP_PROGRESS_NS.store(0, Ordering::Relaxed);
        COOP_MAX_WORK_NS.store(0, Ordering::Relaxed);
        COOP_MAX_GAP_NS.store(0, Ordering::Relaxed);
        flag.store(3, Ordering::Release);
        let request = task_api::CreateRequest {
            name: b"r4l-preempt-witness".as_ptr(),
            name_len: 19,
            state: 3,
            role: match mode {
                2 => 2,
                3 => 3,
                _ => 0,
            },
            needs_fpu: true,
            smp_eligible: true,
            smp_r4x: false,
            owner_kind: 0,
            owner_context: ptr::null_mut(),
            entry: Some(witness),
        };
        let mut failure = 0;
        let task = task_api::r4os_rust_task_create(&request, false, &mut failure);
        if task.is_null() {
            return Scenario::EMPTY;
        }
        if task_api::r4os_rust_task_mutate(24, task, cpu.into(), 0, ptr::null(), 0) == 0 {
            thread_retire::r4os_rust_program_thread_release_created(task);
            return Scenario::EMPTY;
        }
        if mode != 0 {
            if scheduler::r4os_rust_scheduler_task(3, task, 0, 0, ptr::null(), 0) == 0 {
                thread_retire::r4os_rust_program_thread_release_created(task);
                return Scenario::EMPTY;
            }
            while STARTED.load(Ordering::Acquire) == 0
                || (mode == 1
                    && sync_wait::r4os_rust_sync_queue(
                        ptr::addr_of_mut!((*EVENT.0.get()).queue),
                        6,
                        ptr::null_mut(),
                        0,
                        ptr::null_mut(),
                    ) == 0)
            {
                if expired(start, tick) {
                    abort();
                    wait_witness(start, tick);
                    return Scenario::EMPTY;
                }
                scheduler::yield_task();
            }
        }
        let mut handle = c::ProgramProcessHandle::default();
        let mut options = launch::options();
        options.owner = true;
        options.owner_handle_present = true;
        options.out_handle = &mut handle;
        options.forced_cpu = cpu as u8;
        options.forced_cpu_present = true;
        if launch::run(file, 2, 0, span(b""), working, 0, &options) != 0 {
            if mode == 0 {
                thread_retire::r4os_rust_program_thread_release_created(task);
            } else {
                abort();
                wait_witness(start, tick);
            }
            return Scenario::EMPTY;
        }
        let mut timed_out = false;
        if mode == 0 || mode == 1 {
            while flag.load(Ordering::Acquire) != 1 && !ready(handle) {
                if expired(start, tick) {
                    timed_out = true;
                    break;
                }
                scheduler::yield_task();
            }
            if mode == 0 {
                if flag.load(Ordering::Acquire) != 1 {
                    timed_out = true;
                    abort();
                }
                // Explicit periodic-preemption baseline: the witnessed peer
                // is already physically spinning with its active timer. Do
                // not replace this isolated no-IPI comparison with a normal
                // first-publication doorbell (production uses publish_created).
                task_api::r4os_rust_task_mutate(0, task, timer::tick_count(), 0, ptr::null(), 0);
            } else if !timed_out && flag.load(Ordering::Acquire) == 1 {
                // Baseline after actual PREEMPT_SPIN entry, not witness startup.
                // No timing/counter success is inferred from a signal request.
                before = preemption(cpu);
                IPI_BASE.store(before.reschedule_ipi_switches, Ordering::Release);
                while DONE.load(Ordering::Acquire) == 0 && !ready(handle) {
                    if expired(start, tick) {
                        timed_out = true;
                        break;
                    }
                    if sync_wait::r4os_rust_sync_queue(
                        ptr::addr_of_mut!((*EVENT.0.get()).queue),
                        6,
                        ptr::null_mut(),
                        0,
                        ptr::null_mut(),
                    ) != 0
                    {
                        if IPI_WAKES.load(Ordering::Relaxed) >= IPI_WAKE_LIMIT {
                            timed_out = true;
                            break;
                        }
                        IPI_WAKES.fetch_add(1, Ordering::Relaxed);
                        signal();
                    }
                    scheduler::yield_task();
                }
            }
        }
        while !timed_out && !ready(handle) {
            if expired(start, tick) {
                timed_out = true;
                break;
            }
            scheduler::yield_task();
        }
        if timed_out {
            abort();
        }
        let cleanup_start = monotonic::capture();
        let cleanup_tick = timer::tick_count();
        while !ready(handle) && !expired(cleanup_start, cleanup_tick) {
            scheduler::yield_task();
        }
        let mut completion = c::ProgramProcessCompletion::default();
        let completion_ok =
            completions::r4os_rust_program_completion_access(2, handle, &mut completion) == 0;
        if !completion_ok {
            let mut detail = [0u64; 6];
            if registry::lock() {
                let slot = registry::lookup_handle(handle, true);
                if !slot.is_null() {
                    detail = [
                        (*slot).state.into(),
                        (*slot).retire_phase.into(),
                        (*slot).pin_count.into(),
                        (*slot).retire_attempts.into(),
                        (*slot).retire_in_progress.into(),
                        (*registry_state::state()).reaper_started.into(),
                    ];
                }
                registry::unlock();
            }
            log(b"[R4LPREEMPT] incomplete state/phase/pins/retries/owner/reaper=");
            for value in detail {
                dec(value);
                log(b" ");
            }
            log(b"\r\n");
            let failed_flags = flags();
            let failed_timer = timer::deadline_stats();
            log(b"[R4LPREEMPT] flags/irq_delta/last_irq/now/mode=");
            for value in [
                entry_flags,
                failed_flags,
                failed_timer.timer_irqs.wrapping_sub(entry_timer.timer_irqs),
                failed_timer.last_irq_tick,
                timer::tick_count(),
                failed_timer.mode.into(),
            ] {
                dec(value);
                log(b" ");
            }
            log(b"\r\n");
        }
        if DONE.load(Ordering::Acquire) == 0 {
            abort();
        }
        let witness_ok = wait_witness(cleanup_start, cleanup_tick);
        let after = preemption(cpu);
        let timer_switches = after.timer_switches.wrapping_sub(before.timer_switches);
        let ipi = after
            .reschedule_ipi_switches
            .wrapping_sub(before.reschedule_ipi_switches);
        let switched = match mode {
            0 => timer_switches != 0,
            1 => ipi != 0,
            2 | 3 => COOPERATIVE.load(Ordering::Relaxed) != 0,
            _ => unreachable!(),
        };
        let exit = if completion_ok {
            completion.exit_code
        } else {
            -1
        };
        Scenario {
            ok: !timed_out
                && completion_ok
                && exit == 0
                && witness_ok
                && FAILURES.load(Ordering::Acquire) == 0
                && CPU.load(Ordering::Acquire) == cpu
                && flag.load(Ordering::Acquire) == 2
                && switched,
            exit,
            timer: timer_switches,
            ipi,
            cooperative: COOPERATIVE.load(Ordering::Relaxed),
            ipi_wakes: IPI_WAKES.load(Ordering::Relaxed),
            ipi_timer_races: IPI_TIMER_RACES.load(Ordering::Relaxed),
            progress_ns: COOP_PROGRESS_NS.load(Ordering::Relaxed),
            max_work_ns: COOP_MAX_WORK_NS.load(Ordering::Relaxed),
            max_gap_ns: COOP_MAX_GAP_NS.load(Ordering::Relaxed),
        }
    }
}
/// # Safety
/// Existing boot acceptance context, schedulable target and installed fixture.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_preemption_acceptance(cpu: u32, usable: u64) -> bool {
    unsafe {
        runtime_tables::r4os_rust_program_runtime_initialize(usable);
        let mut timer_result = Scenario::EMPTY;
        let mut ipi = Scenario::EMPTY;
        let mut cooperative = Scenario::EMPTY;
        let mut urgent = Scenario::EMPTY;
        let mut generation = 0;
        if cpu != 0 && cpu < MAX_CPUS as u32 && percpu::r4os_rust_percpu_control(13, cpu, 0, 0) != 0
        {
            (|| {
                let boot = drive::r4os_rust_fs_drive_lookup(1, b'C' as usize);
                if boot.is_null() {
                    return;
                }
                let current = drive::r4os_rust_fs_drive_lookup(0, 0);
                let working = if current.is_null() { boot } else { current };
                let Some(file) =
                    launch::resolve(boot, span(b"/R4OS/SOFTWARE/TERMINAL/DIAG/LSTRX.R4X"), false)
                else {
                    return;
                };
                let mut export = MaybeUninit::uninit();
                if !registry_control::r4os_rust_mod_resolve_info(
                    span(b"EXTMATH"),
                    span(b"PREEMPT_FLAG"),
                    1,
                    export.as_mut_ptr(),
                ) {
                    return;
                }
                let export = export.assume_init();
                if export.kind != 2 || export.available_size < 8 || export.address & 7 != 0 {
                    return;
                }
                generation = export.generation;
                FLAG.store(export.address as *mut u64, Ordering::Relaxed);
                timer_result = scenario(&file, working, cpu, 0);
                ipi = scenario(&file, working, cpu, 1);
                cooperative = scenario(&file, working, cpu, 2);
                urgent = scenario(&file, working, cpu, 3);
                let flag = FLAG.load(Ordering::Relaxed);
                if !flag.is_null() {
                    AtomicU64::from_ptr(flag).store(0, Ordering::Release);
                }
                FLAG.store(ptr::null_mut(), Ordering::Relaxed);
            })();
        }
        let ok = timer_result.ok && ipi.ok && cooperative.ok && urgent.ok;
        if !ipi.ok {
            log(b"[R4LPREEMPT] IPI scenario timer_switches=");
            dec(ipi.timer);
            log(b" ipi_switches=");
            dec(ipi.ipi);
            log(b" exit=");
            dec(ipi.exit as i64 as u64);
            log(b"\r\n");
        }
        log(b"[KERNELCOOP] result=");
        log(if cooperative.ok && urgent.ok {
            b"OK"
        } else {
            b"FAILED"
        });
        log(b" switches=");
        dec(cooperative.cooperative);
        log(b" exit=");
        dec(cooperative.exit as i64 as u64);
        log(b" urgent_switches=");
        dec(urgent.cooperative);
        log(b" urgent_exit=");
        dec(urgent.exit as i64 as u64);
        log(b" equal_progress_ns=");
        dec(cooperative.progress_ns);
        log(b" urgent_progress_ns=");
        dec(urgent.progress_ns);
        log(b" max_work_ns=");
        dec(cooperative.max_work_ns.max(urgent.max_work_ns));
        log(b" max_gap_ns=");
        dec(cooperative.max_gap_ns.max(urgent.max_gap_ns));
        log(b"\r\n");
        log(b"[R4LPREEMPT] IPI event qualification wakes=");
        dec(ipi.ipi_wakes.into());
        log(b" timer_races=");
        dec(ipi.ipi_timer_races.into());
        log(b" limit=8 live-library-baseline=yes\r\n");
        log(b"[R4LPREEMPT] result=");
        log(if ok { b"OK" } else { b"FAILED" });
        log(b" cpu=");
        dec(cpu.into());
        log(b" timer_switches=");
        dec(timer_result.timer);
        log(b" ipi_switches=");
        dec(ipi.ipi);
        log(b" timer_exit=");
        dec(timer_result.exit as i64 as u64);
        log(b" ipi_exit=");
        dec(ipi.exit as i64 as u64);
        log(b" generation=");
        dec(generation.into());
        log(b"\r\n");
        ok
    }
}
unsafe fn storage_stats() -> payload_types::Stats {
    unsafe {
        let mut out = MaybeUninit::uninit();
        payload::r4os_rust_program_payload_stats(out.as_mut_ptr());
        out.assume_init()
    }
}
/// # Safety
/// Existing boot-only SMP acceptance context and three installed original apps.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_parallel_acceptance() -> bool {
    unsafe {
        let ap_mask = percpu::r4os_rust_percpu_control(14, 0, 0, 0) & !1;
        if ap_mask.count_ones() < 2 {
            return false;
        }
        let first = ap_mask.trailing_zeros() as u8;
        let after_first = ap_mask & (ap_mask - 1);
        let second = after_first.trailing_zeros() as u8;
        let after_second = after_first & (after_first - 1);
        let third = if after_second != 0 {
            Some(after_second.trailing_zeros() as u8)
        } else {
            None
        };
        let gui_cpus = [first, third.unwrap_or(second)];
        let clip_cpu = if third.is_some() { second } else { first };
        let expected = (1u64 << gui_cpus[0]) | (1u64 << gui_cpus[1]) | (1u64 << clip_cpu);
        let working = drive::r4os_rust_fs_drive_lookup(1, b'C' as usize);
        if working.is_null() {
            return false;
        }
        let Some(calc) = launch::resolve(working, span(b"/R4OS/SOFTWARE/DESKTOP/CALC.R4X"), false)
        else {
            return false;
        };
        let Some(clip) = launch::resolve(working, span(b"/R4OS/SERVICES/CLIPSVC.R4X"), false)
        else {
            return false;
        };
        if !super::gui_activity::probe::empty() {
            return false;
        }
        let before = storage_stats();
        let mut handles = [c::ProgramProcessHandle::default(); 3];
        let mut service = c::ServiceInfo::default();
        let mut existing = MaybeUninit::<service_api::Entry>::uninit();
        let registered = !crate::program::service_api::native_service_entry(
            span(b"CLIPSVC"),
            existing.as_mut_ptr(),
        );
        if registered
            && crate::program::service_api::native_service_install(
                span(b"CLIPSVC"),
                span(b"C:\\R4OS\\SERVICES\\CLIPSVC.R4X"),
                span(b"/RUN"),
                0,
                span(b""),
            ) != 0
        {
            return false;
        }
        let entry = entry::r4os_rust_program_entry_state();
        AtomicU64::from_ptr(ptr::addr_of_mut!((*entry).probe_main_mask))
            .store(0, Ordering::Release);
        AtomicU64::from_ptr(ptr::addr_of_mut!((*entry).probe_service_mask))
            .store(0, Ordering::Release);
        AtomicU64::from_ptr(ptr::addr_of_mut!((*entry).probe_thread_mask))
            .store(0, Ordering::Release);
        AtomicU32::from_ptr(ptr::addr_of_mut!((*entry).probe_bad_inheritance))
            .store(0, Ordering::Release);
        AtomicBool::from_ptr(ptr::addr_of_mut!((*entry).probe_active))
            .store(true, Ordering::Release);
        struct Active;
        impl Drop for Active {
            fn drop(&mut self) {
                unsafe {
                    AtomicBool::from_ptr(ptr::addr_of_mut!(
                        (*entry::r4os_rust_program_entry_state()).probe_active
                    ))
                    .store(false, Ordering::Release);
                }
            }
        }
        let _active = Active;
        log(b"[PARALLEL-OWNERS] begin CALC CLIPSVC\r\n");
        let work = (|| {
            for i in 0..2 {
                let mut options = launch::options();
                options.owner = true;
                options.owner_handle_present = true;
                options.out_handle = &mut handles[i];
                options.forced_cpu = gui_cpus[i];
                options.forced_cpu_present = true;
                if launch::run(&calc, 2, 0, span(b""), working, 0, &options) != 0 {
                    return false;
                }
                let window = c::GuiWindowInfo {
                    window_id: i as i32,
                    client_w: 300,
                    client_h: 360,
                    ..Default::default()
                };
                if gui_events::r4os_rust_program_gui_window_set(handles[i].instance_id, &window)
                    != 0
                    || program_api::r4os_rust_program_api_window(0, &handles[i], i as i32, true)
                        != 0
                {
                    return false;
                }
            }
            let start = timer::tick_count();
            for handle in &handles[..2] {
                loop {
                    let mut info = c::GuiFrameInfo::default();
                    if gui_view::r4os_rust_program_gui_view_api(
                        0,
                        handle,
                        0,
                        ptr::from_mut(&mut info).cast(),
                    ) == 0
                        && info.committed_generation != 0
                        && info.committed_command_count != 0
                    {
                        break;
                    }
                    if timer::tick_count().wrapping_sub(start) > 5 * u64::from(pit::DEFAULT_HZ) {
                        return false;
                    }
                    scheduler::yield_task();
                }
                let revision = gui_events::r4os_rust_program_gui_revision_read(handle.instance_id);
                let key = c::GuiEvent {
                    kind: 6,
                    key: b'7'.into(),
                    ..Default::default()
                };
                if gui_events::r4os_rust_program_gui_event_push(handle.instance_id, &key) != 0 {
                    return false;
                }
                while gui_events::r4os_rust_program_gui_revision_read(handle.instance_id)
                    == revision
                {
                    if timer::tick_count().wrapping_sub(start) > 5 * u64::from(pit::DEFAULT_HZ) {
                        return false;
                    }
                    scheduler::yield_task();
                }
            }
            log(b"[PARALLEL-OWNERS] GUI frames and input OK\r\n");
            if !super::gui_activity::probe::run(&handles[..2]) {
                return false;
            }
            let Some(clipboard_before) =
                super::gui_activity::probe::shared_sequences(&handles[..2])
            else {
                return false;
            };
            if service_api::r4os_rust_program_service_lifecycle(
                false,
                span(b"CLIPSVC"),
                &mut service,
                0,
            ) != 0
            {
                return false;
            }
            let mut options = launch::options();
            options.owner = true;
            options.owner_handle_present = true;
            options.out_handle = &mut handles[2];
            options.forced_cpu = clip_cpu;
            options.forced_cpu_present = true;
            if launch::run(&clip, 2, 0, span(b"/SELFTEST"), working, 0, &options) != 0 {
                return false;
            }
            let mut completion = c::ProgramProcessCompletion::default();
            if process::r4os_rust_program_process_wait(
                &handles[2],
                8 * u64::from(pit::DEFAULT_HZ),
                &mut completion,
            ) != 0
                || completion.exit_code != 0
            {
                return false;
            }
            if !super::gui_activity::probe::shared_commits(&handles[..2], clipboard_before) {
                return false;
            }
            if process::r4os_rust_program_process_close(&handles[0]) != 0
                || process::r4os_rust_program_process_wait(
                    &handles[0],
                    3 * u64::from(pit::DEFAULT_HZ),
                    &mut completion,
                ) != 0
                || completion.exit_code != 0
            {
                return false;
            }
            if process::r4os_rust_program_process_kill(&handles[1]) != 0
                || process::r4os_rust_program_process_wait(
                    &handles[1],
                    3 * u64::from(pit::DEFAULT_HZ),
                    &mut completion,
                ) != 0
                || completion.exit_code != -9
            {
                return false;
            }
            true
        })();
        let mut cleanup = true;
        for handle in &handles {
            if !registry::valid(*handle) {
                continue;
            }
            if !ready(*handle) {
                process::r4os_rust_program_process_kill(handle);
            }
            let mut completion = c::ProgramProcessCompletion::default();
            cleanup = (process::r4os_rust_program_process_wait(
                handle,
                5 * u64::from(pit::DEFAULT_HZ),
                &mut completion,
            ) == 0)
                && cleanup;
            cleanup =
                (process::r4os_rust_program_process_reap(handle, &mut completion) == 0) && cleanup;
        }
        if service.instance_id != 0 {
            cleanup = (service_api::r4os_rust_program_service_lifecycle(
                true,
                span(b"CLIPSVC"),
                &mut service,
                5 * u64::from(pit::DEFAULT_HZ),
            ) == 0)
                && cleanup;
        }
        if registered {
            cleanup = (crate::program::service_api::native_service_name_mutate(
                4,
                span(b"CLIPSVC"),
                0,
                span(b""),
            ) == 0)
                && cleanup;
        }
        let cleanup_start = timer::tick_count();
        let mut after = storage_stats();
        while after.current_payload_bytes != before.current_payload_bytes
            && timer::tick_count().wrapping_sub(cleanup_start) < 3 * u64::from(pit::DEFAULT_HZ)
        {
            reaper::r4os_rust_program_reaper_once();
            scheduler::yield_task();
            after = storage_stats();
        }
        let mains = AtomicU64::from_ptr(ptr::addr_of_mut!((*entry).probe_main_mask))
            .load(Ordering::Acquire);
        let services = AtomicU64::from_ptr(ptr::addr_of_mut!((*entry).probe_service_mask))
            .load(Ordering::Acquire);
        let workers = AtomicU64::from_ptr(ptr::addr_of_mut!((*entry).probe_thread_mask))
            .load(Ordering::Acquire);
        let subscriptions_empty = super::gui_activity::probe::empty();
        let ok = work
            && cleanup
            && subscriptions_empty
            && mains & expected == expected
            && mains & !ap_mask == 0
            && services & ap_mask != 0
            && workers != 0
            && AtomicU32::from_ptr(ptr::addr_of_mut!((*entry).probe_bad_inheritance))
                .load(Ordering::Acquire)
                == 0
            && before.current_payload_bytes == after.current_payload_bytes
            && before.active_instances == after.active_instances
            && before.header_errors == after.header_errors
            && before.free_failures == after.free_failures;
        log(b"[PARALLEL-OWNERS] result=");
        log(if ok { b"OK" } else { b"FAILED" });
        log(b" main_mask=");
        dec(mains);
        log(b" service_mask=");
        dec(services);
        log(b" thread_mask=");
        dec(workers);
        log(b" work=");
        dec(work.into());
        log(b" cleanup=");
        dec(cleanup.into());
        log(b" subscriptions_empty=");
        dec(subscriptions_empty.into());
        log(b" payload_before=");
        dec(before.current_payload_bytes);
        log(b" payload_after=");
        dec(after.current_payload_bytes);
        log(b"\r\n");
        ok
    }
}
