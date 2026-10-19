//! Resident run ledger; only the admitted BSP controller writes until DONE.
use super::*;
use crate::{
    contract as c,
    storage::{access_state::Reference, block_registry as registry},
};
#[derive(Clone, Copy)]
struct DeviceRecord {
    reference: Reference,
    lane: u8,
    live: bool,
    retained: bool,
    stats: accounting::Stats,
}
pub(super) struct Session {
    pub tasks: diagnostic::Ledger<16>,
    pub controller: *mut Task,
    devices: [DeviceRecord; 4],
    count: usize,
    unregistered: u64,
    pub attempted: u32,
    pub passed: u32,
    pub failed: u32,
    pub stage: u32,
    pub cleanup_errors: u32,
    pub retained: bool,
}
struct Cell(UnsafeCell<MaybeUninit<Session>>);
// Exclusive gate -> one BSP controller -> release/acquire DONE -> caller.
unsafe impl Sync for Cell {}
static SESSION: Cell = Cell(UnsafeCell::new(MaybeUninit::uninit()));
static BUSY: AtomicBool = AtomicBool::new(false);
pub(super) struct Gate(pub bool);
impl Drop for Gate {
    fn drop(&mut self) {
        if self.0 {
            BUSY.store(false, Ordering::Release);
        }
    }
}
pub(super) fn gate() -> Option<Gate> {
    BUSY.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .ok()
        .map(|_| Gate(true))
}
pub(super) fn session() -> *mut Session {
    SESSION.0.get().cast()
}
pub(super) unsafe fn reset() {
    assert_eq!(
        REQUESTER_ACTIVE.load(Ordering::Acquire),
        0,
        "retained requester overwritten"
    );
    assert_eq!(
        SYNC_ACTIVE.load(Ordering::Acquire),
        0,
        "retained callback overwritten"
    );
    assert_eq!(
        PENDING.load(Ordering::Acquire),
        0,
        "retained backend overwritten"
    );
    unsafe {
        session().write(Session {
            tasks: diagnostic::Ledger::EMPTY,
            controller: ptr::null_mut(),
            devices: core::array::from_fn(|_| DeviceRecord {
                reference: Reference::default(),
                lane: 255,
                live: false,
                retained: false,
                stats: accounting::Stats::default(),
            }),
            count: 0,
            unregistered: 0,
            attempted: 0,
            passed: 0,
            failed: 0,
            stage: 0,
            cleanup_errors: 0,
            retained: false,
        });
    }
    for atom in [
        &SUBMISSIONS,
        &PENDING,
        &CANCELS,
        &TIMEOUT_CANCELS,
        &RESET_CANCELS,
        &RESETS,
        &ACCEPTED,
        &DUPLICATE_REJECTED,
        &DUPLICATE_KIND,
        &LATE_REJECTED,
        &SYNC_MAX,
    ] {
        atom.store(0, Ordering::Relaxed);
    }
    for atom in &FLAGS {
        atom.store(false, Ordering::Relaxed);
    }
    ABORT.store(false, Ordering::Release);
    CANCEL.store(false, Ordering::Release);
}
pub(super) fn reference(index: usize) -> Option<Reference> {
    unsafe {
        for i in 0..(*session()).count {
            let p = ptr::addr_of!((*session()).devices[i]);
            if (*p).live && (*p).reference.slot as usize == index {
                return Some((*p).reference);
            }
        }
    }
    None
}
pub(super) unsafe fn register(mut config: device::Config) -> Option<usize> {
    // Only this exact diagnostic's reviewed resident backend functions enter.
    config.parallel_callbacks = true;
    unsafe { register_policy(config) }
}
pub(super) unsafe fn register_policy(config: device::Config) -> Option<usize> {
    if cancelled() || unsafe { (*session()).count } == 4 {
        return None;
    }
    let reference = unsafe { device::register_reference(config) }?;
    let at = unsafe { (*session()).count };
    unsafe {
        (*session()).devices[at] = DeviceRecord {
            reference,
            lane: 255,
            live: true,
            retained: false,
            stats: accounting::Stats::default(),
        };
        (*session()).count += 1;
    }
    let mut slot = MaybeUninit::uninit();
    if !unsafe {
        registry::r4os_rust_block_registry_slot(
            reference.slot as usize,
            0,
            0,
            false,
            slot.as_mut_ptr(),
        )
    } {
        return None;
    }
    let slot = unsafe { slot.assume_init() };
    if slot.reference != reference {
        return None;
    }
    unsafe {
        (*session()).devices[at].lane = slot.lane;
    }
    Some(reference.slot as usize)
}
pub(super) fn active(index: usize) -> Option<u32> {
    device::probe_facts(reference(index)?).map(|v| v.0)
}
pub(super) fn queue(index: usize) -> u32 {
    reference(index)
        .and_then(device::probe_facts)
        .map(|v| v.1)
        .unwrap_or(u32::MAX)
}
pub(super) fn unregister(index: usize) -> bool {
    let Some(reference) = reference(index) else {
        return false;
    };
    let at = unsafe {
        (0..(*session()).count)
            .find(|&i| {
                (*session()).devices[i].live && (*session()).devices[i].reference == reference
            })
            .unwrap()
    };
    if unsafe { (*session()).devices[at].retained } {
        return false;
    }
    let deadline = timer::tick_count().saturating_add(crate::arch::pit::DEFAULT_HZ as u64 / 2);
    loop {
        let Some((active, queued, stats)) = device::probe_facts(reference) else {
            unsafe {
                (*session()).devices[at].retained = true;
                (*session()).cleanup_errors += 1;
            }
            return false;
        };
        unsafe {
            (*session()).devices[at].stats = stats;
        }
        if unsafe { execution::unregister_exact(reference) } {
            unsafe {
                (*session()).devices[at].live = false;
                (*session()).unregistered += 1;
            }
            return true;
        }
        // A negative busy-I/O case must observe immediate refusal. For idle
        // teardown only, false is the existing rollback acknowledgment: wait
        // for a transient runtime-worker pin, then revalidate the exact
        // generation at the same owner. Never retry a raw reused index.
        if active != 0 || queued != 0 {
            return false;
        }
        if timer::tick_count() >= deadline {
            unsafe {
                (*session()).devices[at].retained = true;
                (*session()).cleanup_errors += 1;
            }
            return false;
        }
        diagnostic::sleep(1, b"block-probe-unregister");
    }
}
pub(super) fn create(name: &'static [u8], entry: unsafe extern "C" fn()) -> *mut c_void {
    create_on(name, entry, None)
}
pub(super) fn create_on(
    name: &'static [u8],
    entry: unsafe extern "C" fn(),
    cpu: Option<u32>,
) -> *mut c_void {
    if cancelled() {
        return ptr::null_mut();
    }
    let task = unsafe { (*session()).tasks.create(name, entry, Role::Batch, cpu) };
    if task.is_null() {
        return ptr::null_mut();
    }
    if !diagnostic::start(task) {
        let released = unsafe { (*session()).tasks.finish_task(task, true) };
        if !released {
            unsafe {
                (*session()).retained = true;
            }
        }
        return ptr::null_mut();
    }
    task.cast()
}
pub(super) fn finish_workers() -> bool {
    unsafe {
        (*session())
            .tasks
            .finish_except((*session()).controller, false)
    }
}
pub(super) fn kill_denied(task: *mut c_void) -> bool {
    if task.is_null() {
        return false;
    }
    let (id, generation) = diagnostic::identity(task.cast());
    let guarded = {
        let _runtime = crate::arch::runtime::RuntimeGuard::acquire();
        unsafe {
            let p = crate::sched::task::task_registry::core(task.cast());
            (*p).id == id
                && (*p).generation == generation
                && matches!(
                    (*p).state,
                    crate::sched::task::task_types::State::Ready
                        | crate::sched::task::task_types::State::Blocked
                )
                && (*p).unwind_guard_count != 0
        }
    };
    guarded && !unsafe { crate::sched::task::task_lifecycle::kill(id, Some(generation)) }
}
pub(super) fn case(bit: u32, ok: bool) -> bool {
    let ok = ok && !cancelled();
    unsafe {
        (*session()).attempted |= bit;
        if ok {
            (*session()).passed |= bit;
        } else {
            (*session()).passed &= !bit;
            (*session()).failed |= bit;
            (*session()).stage = bit.trailing_zeros() + 1;
        }
    }
    ok
}
pub(super) fn summary() -> accounting::RuntimeSummary {
    let mut out = accounting::RuntimeSummary::default();
    unsafe {
        out.controller_count = (*session()).count as u32;
        let mut lanes = 0u32;
        for i in 0..(*session()).count {
            let record = (*session()).devices[i];
            let stats = if record.live {
                device::probe_facts(record.reference)
                    .map(|v| v.2)
                    .unwrap_or(record.stats)
            } else {
                record.stats
            };
            if record.lane < 8 && workers::r4os_rust_block_worker_alive(record.lane) {
                lanes |= 1 << record.lane;
            }
            out.direct_requests += stats.direct_requests;
            out.bounce_allocations += stats.bounce_allocations;
            out.bounce_copy_bytes += stats.bounce_copy_bytes;
            out.async_submissions += stats.async_submissions;
            out.async_completions += stats.async_completions;
            out.async_resets += stats.async_resets;
            out.async_cancel_requests += stats.async_cancel_requests;
            out.in_flight_high_water = out.in_flight_high_water.max(stats.in_flight_high_water);
        }
        out.worker_count = lanes.count_ones();
    }
    out.worker_parallel_active_max = SYNC_MAX.load(Ordering::Acquire);
    out.duplicate_completions = u64::from(DUPLICATE_REJECTED.load(Ordering::Acquire));
    out.late_completions = u64::from(LATE_REJECTED.load(Ordering::Acquire));
    out
}
pub(super) fn cleanup() -> bool {
    ABORT.store(true, Ordering::Release);
    let deadline = timer::tick_count().saturating_add(10 * crate::arch::pit::DEFAULT_HZ as u64);
    loop {
        signal(1);
        let pending = PENDING.load(Ordering::Acquire);
        for n in 0..SUBMISSIONS.load(Ordering::Acquire) as usize {
            if pending & (1 << n) != 0 {
                let request = backend::request(n);
                let _ = backend::completion(request, -4, 0);
            }
        }
        if REQUESTER_ACTIVE.load(Ordering::Acquire) == 0
            && PENDING.load(Ordering::Acquire) == 0
            && SYNC_ACTIVE.load(Ordering::Acquire) == 0
        {
            break;
        }
        if timer::tick_count() >= deadline {
            unsafe {
                (*session()).retained = true;
                (*session()).cleanup_errors += 1;
            }
            return false;
        }
        diagnostic::sleep(1, b"block-probe-drain");
    }
    let tasks = finish_workers();
    if !tasks {
        unsafe {
            (*session()).retained = true;
            (*session()).cleanup_errors += 1;
        }
        return false;
    }
    let mut devices = true;
    for i in 0..unsafe { (*session()).count } {
        let record = unsafe { (*session()).devices[i] };
        if record.live && !unregister(record.reference.slot as usize) {
            devices = false;
        }
    }
    if !devices {
        unsafe {
            (*session()).retained = true;
        }
    }
    tasks && devices && !unsafe { (*session()).retained }
}
pub(super) fn collect(run_id: u64) -> c::ProgramKernelBlockQueueCheck {
    let s = session();
    let (created, reaped, retained) = unsafe { (*s).tasks.summary() };
    let mut out = c::ProgramKernelBlockQueueCheck {
        run_id,
        tasks_created: created,
        tasks_reaped: reaped,
        retained_task_count: u64::from(retained),
        attempted_cases: unsafe { (*s).attempted },
        passed_cases: unsafe { (*s).passed },
        failed_cases: unsafe { (*s).failed },
        fail_stage: unsafe { (*s).stage },
        cleanup_errors: unsafe { (*s).cleanup_errors },
        controllers_registered: unsafe { (*s).count as u64 },
        controllers_unregistered: unsafe { (*s).unregistered },
        completion_receipts: u64::from(ACCEPTED.load(Ordering::Acquire)),
        duplicate_rejections: u64::from(DUPLICATE_REJECTED.load(Ordering::Acquire)),
        late_rejections: u64::from(LATE_REJECTED.load(Ordering::Acquire)),
        duplicate_disposition: DUPLICATE_KIND.load(Ordering::Acquire),
        cancel_requests: u64::from(CANCELS.load(Ordering::Acquire)),
        timeout_cancels: u64::from(TIMEOUT_CANCELS.load(Ordering::Acquire)),
        reset_cancels: u64::from(RESET_CANCELS.load(Ordering::Acquire)),
        resets: u64::from(RESETS.load(Ordering::Acquire)),
        parallel_active_max: u64::from(SYNC_MAX.load(Ordering::Acquire)),
        retained_request_count: u64::from(PENDING.load(Ordering::Acquire).count_ones()),
        ..Default::default()
    };
    for i in 0..unsafe { (*s).count } {
        let record = unsafe { (*s).devices[i] };
        out.retained_controller_count += u64::from(record.live);
        let stats = if record.live {
            device::probe_facts(record.reference)
                .map(|v| v.2)
                .unwrap_or(record.stats)
        } else {
            record.stats
        };
        out.async_submissions += stats.async_submissions;
        out.async_completions += stats.async_completions;
        out.read_ops += stats.read_ops;
        out.write_ops += stats.write_ops;
        out.flush_ops += stats.flush_ops;
        out.queue_high_water = out.queue_high_water.max(u64::from(stats.queue_high_water));
        out.in_flight_high_water = out
            .in_flight_high_water
            .max(u64::from(stats.in_flight_high_water));
    }
    out.retained_buffer_count = if out.retained_task_count != 0
        || out.retained_controller_count != 0
        || out.retained_request_count != 0
        || REQUESTER_ACTIVE.load(Ordering::Acquire) != 0
        || SYNC_ACTIVE.load(Ordering::Acquire) != 0
    {
        8
    } else {
        0
    };
    if out.retained_task_count != 0 || out.retained_request_count != 0 {
        out.flags |= c::kernel_block_queue_check_flag_partial_snapshot;
    }
    out
}
