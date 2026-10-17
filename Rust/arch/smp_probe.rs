//! Existing bounded SMP/TLB/owner/clock diagnostic, with one native state owner.
//! Functional cases share exact native task and mapping owners.
mod cases;
mod live;
mod mapping;
mod workers;
use crate::{arch::runtime, boot::serial, memory::owner, platform::clocksource};
use crate::{
    arch::{percpu, smp, smp_policy::MAX_CPUS, tlb},
    memory::{blocks, heap, paging},
    platform::{monotonic, timer},
    sched::scheduler,
};
use core::{
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering},
};
pub(crate) use live::check;
use workers::worker_main;
const PAGE: u64 = 4096;
const TIMEOUT: u64 = 10_000_000_000;
const ITERATIONS: u64 = 4_000_000;
const STRESS: u64 = 64;
const OLD: u64 = 0x544c_422d_4f4c_4421;
const NEW: u64 = 0x544c_422d_4e45_5721;
static RELEASE: AtomicU8 = AtomicU8::new(0);
static RETIRE: AtomicU8 = AtomicU8::new(0);
static DONE: AtomicU32 = AtomicU32::new(0);
static FAILURES: AtomicU32 = AtomicU32::new(0);
static CPUS: AtomicU64 = AtomicU64::new(0);
static REGRESSIONS: AtomicU32 = AtomicU32::new(0);
static ADDRESS: AtomicU64 = AtomicU64::new(0);
static PHASE: AtomicU8 = AtomicU8::new(0);
static TLB_READY: AtomicU64 = AtomicU64::new(0);
static TLB_UPDATED: AtomicU64 = AtomicU64::new(0);
static TLB_FAILURES: AtomicU32 = AtomicU32::new(0);
static OWNER_DONE: AtomicU64 = AtomicU64::new(0);
static OWNER_FAILURES: AtomicU32 = AtomicU32::new(0);
static MUTEX_COUNT: AtomicU64 = AtomicU64::new(0);
static RESULTS: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
static CLOCK_STARTS: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
static CLOCK_ENDS: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
static TICK_STARTS: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
static TICK_ENDS: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
type TlbProbe = mapping::Mapping;
static ABORT: AtomicBool = AtomicBool::new(false);
static PROBING: AtomicBool = AtomicBool::new(false);
static OWNED_POINTERS: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
static OWNED_SIZES: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
struct Session {
    mapping: TlbProbe,
    tasks: crate::sched::task::diagnostic::Ledger<{ MAX_CPUS + 1 }>,
    controller: *mut crate::sched::task::task_types::Task,
    retained: bool,
    online_mask: u64,
    placement_mask: u64,
    expected: u64,
    actual: u64,
    stage: u32,
    checked_clock: Option<clocksource::Status>,
}
struct SessionCell(core::cell::UnsafeCell<Session>);
// One admitted BSP controller owns this ledger; workers use only atomics.
unsafe impl Sync for SessionCell {}
static SESSION: SessionCell = SessionCell(core::cell::UnsafeCell::new(Session {
    mapping: TlbProbe::EMPTY,
    tasks: crate::sched::task::diagnostic::Ledger::EMPTY,
    controller: ptr::null_mut(),
    retained: false,
    online_mask: 0,
    placement_mask: 0,
    expected: 0,
    actual: 0,
    stage: 0,
    checked_clock: None,
}));
struct ProbeGate(bool);
impl Drop for ProbeGate {
    fn drop(&mut self) {
        if self.0 {
            PROBING.store(false, Ordering::Release);
        }
    }
}
fn probe_gate() -> Option<ProbeGate> {
    PROBING
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .ok()
        .map(|_| ProbeGate(true))
}
fn session() -> *mut Session {
    SESSION.0.get()
}
#[repr(C)]
#[derive(Default)]
pub struct HeapReport {
    pub ok: bool,
    pub iterations: u64,
    pub commit_calls: u64,
    pub commit_pages: u64,
    pub uncommit_calls: u64,
    pub release_suppressed: u64,
    pub poison_bytes: u64,
    pub retained_pages: u64,
    pub block_claims: u64,
    pub extent_allocations: u64,
    pub map_batches: u64,
    pub unmap_batches: u64,
    pub under_pressure: bool,
}
#[repr(C)]
pub struct Report {
    pub ok: bool,
    pub clock_ok: bool,
    pub tlb_ok: bool,
    pub lock_ok: bool,
    pub serial_ok: bool,
    pub timeout_rejected: bool,
    pub frame_retained: bool,
    pub tlb_cleanup_ok: bool,
    pub online_count: u32,
    pub failures: u32,
    pub clock_regressions: u32,
    pub tlb_failures: u32,
    pub owner_failures: u32,
    pub online_mask: u64,
    pub observed_mask: u64,
    pub tlb_ready_mask: u64,
    pub tlb_updated_mask: u64,
    pub owner_done_mask: u64,
    pub irq_delta: u64,
    pub heap_probe: HeapReport,
    pub clock_status: clocksource::Status,
    pub tlb_stats: tlb::Stats,
    pub runtime_stats: runtime::Stats,
    pub owner_stats: owner::Stats,
    pub serial_stats: serial::Stats,
}

fn text(event: u8, value: &[u8]) {
    unsafe { smp_probe_text(event, value.as_ptr(), value.len()) };
}
fn fail(reason: &[u8]) -> bool {
    text(0, reason);
    false
}
fn work(index: u32) -> u64 {
    let mut value = u64::from(index)
        .wrapping_mul(0x9e37_79b9_7f4a_7c15)
        .wrapping_add(0xd1b5_4a32_d192_ed03);
    for iteration in 0..ITERATIONS {
        value ^= value >> 12;
        value ^= value << 25;
        value ^= value >> 27;
        value = value
            .wrapping_mul(0x2545_f491_4f6c_dd1d)
            .wrapping_add(iteration ^ (u64::from(index) << 32));
    }
    core::hint::black_box(value)
}
fn elapsed(start: monotonic::Stamp) -> Option<u64> {
    monotonic::elapsed(start, monotonic::capture())
}
fn remote(mask: u64) -> Option<u32> {
    (0..MAX_CPUS as u32).find(|&i| i != percpu::current_index() && mask & (1 << i) != 0)
}
fn tlb_stats() -> tlb::Stats {
    let mut output = core::mem::MaybeUninit::uninit();
    unsafe {
        tlb::r4os_rust_tlb_stats(output.as_mut_ptr());
        output.assume_init()
    }
}
struct Cleanup {
    probe: *mut TlbProbe,
    pending: bool,
}
fn stop_workers() -> bool {
    ABORT.store(true, Ordering::Release);
    RELEASE.store(1, Ordering::Release);
    PHASE.store(3, Ordering::Release);
    RETIRE.store(1, Ordering::Release);
    unsafe { (*session()).tasks.cleanup_except((*session()).controller) }
}
fn retained_allocations() -> u64 {
    OWNED_POINTERS
        .iter()
        .filter(|p| p.load(Ordering::Acquire) != 0)
        .count() as u64
}
impl Drop for Cleanup {
    fn drop(&mut self) {
        let workers = stop_workers();
        unsafe {
            if !workers || retained_allocations() != 0 {
                (*session()).retained = true;
            }
            if self.pending {
                if workers {
                    if !smp_probe_cleanup(self.probe) {
                        (*session()).retained = true;
                    }
                } else {
                    (*self.probe).retained = true;
                }
            }
        }
    }
}
fn reset(address: u64) {
    ABORT.store(false, Ordering::Release);
    for index in 0..MAX_CPUS {
        RESULTS[index].store(0, Ordering::Relaxed);
        CLOCK_STARTS[index].store(0, Ordering::Relaxed);
        CLOCK_ENDS[index].store(0, Ordering::Relaxed);
        TICK_STARTS[index].store(0, Ordering::Relaxed);
        TICK_ENDS[index].store(0, Ordering::Relaxed);
    }
    RELEASE.store(0, Ordering::Release);
    DONE.store(0, Ordering::Release);
    FAILURES.store(0, Ordering::Release);
    REGRESSIONS.store(0, Ordering::Release);
    CPUS.store(0, Ordering::Release);
    ADDRESS.store(address, Ordering::Release);
    PHASE.store(1, Ordering::Release);
    TLB_READY.store(0, Ordering::Release);
    TLB_UPDATED.store(0, Ordering::Release);
    TLB_FAILURES.store(0, Ordering::Release);
    OWNER_DONE.store(0, Ordering::Release);
    OWNER_FAILURES.store(0, Ordering::Release);
    unsafe { smp_probe_mutex(0) };
    MUTEX_COUNT.store(0, Ordering::Relaxed);
    RETIRE.store(0, Ordering::Release);
}
pub fn run(usable: u64) -> bool {
    let Some(mut gate) = probe_gate() else {
        return fail(b"probe-busy-or-retained");
    };
    let Some(_admission) = crate::sched::task_context::Admission::acquire(false) else {
        return false;
    };
    unsafe {
        session().write(Session {
            mapping: TlbProbe::EMPTY,
            tasks: crate::sched::task::diagnostic::Ledger::EMPTY,
            controller: ptr::null_mut(),
            retained: false,
            online_mask: 0,
            placement_mask: 0,
            expected: 0,
            actual: 0,
            stage: 0,
            checked_clock: None,
        });
    }
    live::clear_cancel();
    reset(0);
    let result = cases::run(usable, false);
    if !result || unsafe { (*session()).retained } {
        gate.0 = false;
    }
    result
}
const _: () = {
    assert!(core::mem::size_of::<Report>() == 576);
    assert!(core::mem::offset_of!(Report, online_mask) == 32);
    assert!(core::mem::size_of::<HeapReport>() == 104);
    assert!(core::mem::offset_of!(Report, heap_probe) == 80);
    assert!(core::mem::offset_of!(Report, serial_stats) == 464);
};

struct ProbeMutex(core::cell::UnsafeCell<crate::sched::sync_types::Mutex>);
// Only the bounded explicitly requested probe resets this mutex before workers
// start. Runtime mutex ownership protects all subsequent worker operations.
unsafe impl Sync for ProbeMutex {}
const PROBE_MUTEX_VALUE: crate::sched::sync_types::Mutex = crate::sched::sync_types::Mutex {
    name: crate::sched::sync_types::Text::literal(b"smp-owner-probe"),
    ..crate::sched::sync_types::Mutex::EMPTY
};
static PROBE_MUTEX: ProbeMutex = ProbeMutex(core::cell::UnsafeCell::new(PROBE_MUTEX_VALUE));
fn claim_probe_frame(name: &'static [u8]) -> Option<u64> {
    let mut frame = 0;
    if !unsafe { crate::memory::phys::r4os_rust_phys_alloc(&mut frame) } {
        return None;
    }
    if unsafe { blocks::claim(blocks::Block::physical(frame, PAGE, 1, 0, 0, 2, name)) }.is_err() {
        crate::memory::phys::r4os_rust_phys_free(frame);
        return None;
    }
    Some(frame)
}
fn release_probe_frame(frame: u64) -> bool {
    let mut plan = core::mem::MaybeUninit::uninit();
    if unsafe { blocks::r4os_rust_blocks_prepare(frame, PAGE, plan.as_mut_ptr()) } != 0 {
        return false;
    }
    let mut plan = unsafe { plan.assume_init() };
    crate::memory::phys::r4os_rust_phys_free(frame);
    unsafe {
        blocks::r4os_rust_blocks_finish(&mut plan, false, false);
        blocks::r4os_rust_blocks_finish(&mut plan, false, true);
    }
    true
}
fn probe_text(text: &[u8]) {
    crate::boot::log_route::serial_write_raw(text);
}
fn probe_dec(mut value: u64) {
    let mut buffer = [0; 20];
    let mut i = buffer.len();
    if value == 0 {
        crate::boot::log_route::serial_putc_raw(b'0');
        return;
    }
    while value != 0 {
        i -= 1;
        buffer[i] = b'0' + (value % 10) as u8;
        value /= 10;
    }
    probe_text(&buffer[i..]);
}
fn probe_hex(value: u64, width: u8) {
    for index in (0..width).rev() {
        crate::boot::log_route::serial_putc_raw(
            b"0123456789ABCDEF"[((value >> (u32::from(index) * 4)) & 15) as usize],
        );
    }
}

unsafe fn smp_probe_setup(output: *mut TlbProbe) -> bool {
    unsafe { (&mut *output).acquire() }
}
unsafe fn smp_probe_cleanup(probe: *mut TlbProbe) -> bool {
    unsafe { (&mut *probe).cleanup() }
}

unsafe fn smp_probe_mutex(operation: u8) -> bool {
    match operation {
        0 => {
            unsafe {
                PROBE_MUTEX.0.get().write(PROBE_MUTEX_VALUE);
            }
            true
        }
        1 => unsafe {
            crate::sched::sync_locks::r4os_rust_sync_mutex(
                PROBE_MUTEX.0.get(),
                1,
                u64::from(crate::arch::pit::DEFAULT_HZ),
            )
        },
        2 => unsafe { crate::sched::sync_locks::r4os_rust_sync_mutex(PROBE_MUTEX.0.get(), 2, 0) },
        _ => unreachable!(),
    }
}

unsafe fn smp_probe_finish(operation: u8, cpu: u32, usable: u64) -> bool {
    match operation {
        0 => unsafe {
            crate::program::acceptance::r4os_rust_program_preemption_acceptance(cpu, usable)
        },
        1 => unsafe { crate::program::acceptance::r4os_rust_program_parallel_acceptance() },
        _ => unreachable!(),
    }
}

unsafe fn smp_probe_heap(output: *mut HeapReport) {
    unsafe {
        output.write(crate::memory::heap_probes::acceptance());
    }
}

unsafe fn smp_probe_counters(
    runtime_output: *mut runtime::Stats,
    owner_output: *mut owner::Stats,
    serial_output: *mut serial::Stats,
) -> bool {
    unsafe {
        runtime::r4os_rust_runtime_stats(runtime_output);
        serial::r4os_rust_serial_stats(serial_output);
    }
    let mut combined = owner::Stats::default();
    for class in 0..owner::CLASS_NAMES.len() as u8 {
        let s = owner::stats_for_class(class);
        combined.acquisitions = combined.acquisitions.wrapping_add(s.acquisitions);
        combined.nested_acquisitions = combined
            .nested_acquisitions
            .wrapping_add(s.nested_acquisitions);
        combined.collisions = combined.collisions.wrapping_add(s.collisions);
        combined.wait_spins = combined.wait_spins.wrapping_add(s.wait_spins);
        combined.max_wait_spins = combined.max_wait_spins.max(s.max_wait_spins);
        combined.hold_cycles = combined.hold_cycles.wrapping_add(s.hold_cycles);
        combined.max_hold_cycles = combined.max_hold_cycles.max(s.max_hold_cycles);
        combined.order_violations = combined.order_violations.wrapping_add(s.order_violations);
    }
    unsafe {
        owner_output.write(combined);
    }
    let serial = unsafe { serial_output.read() };
    serial.bulk_calls != 0
        && serial.max_span > 1
        && serial.lock_acquisitions == serial.write_calls
        && serial.uart_status_reads < serial.ring_bytes
        && serial.dropped_bytes == 0
}

unsafe fn smp_probe_text(event: u8, text: *const u8, len: usize) {
    match event {
        0 => {
            probe_text(b"[SMPPROBE] result=FAILED reason=");
            probe_text(unsafe { core::slice::from_raw_parts(text, len) });
            probe_text(b"\r\n");
        }
        1 => probe_text(b"[SMPPROBE] result=SKIPPED cpus=1 reason=single-cpu\r\n"),
        _ => unreachable!(),
    }
}

unsafe fn smp_probe_report(report: *const Report) {
    let report = unsafe { &*report };
    let clock_status = &report.clock_status;
    probe_text(b"[SMPPROBE] result=");
    probe_text(if report.ok { b"OK" } else { b"FAILED" });
    probe_text(b" cpus=");
    probe_dec(u64::from(report.online_count));
    probe_text(b" expected_mask=0x");
    probe_hex(report.online_mask, 8);
    probe_text(b" observed_mask=0x");
    probe_hex(report.observed_mask, 8);
    probe_text(b" failures=");
    probe_dec(u64::from(report.failures));
    probe_text(b"\r\n");
    probe_text(b"[HEAPPROBE] result=");
    probe_text(if report.heap_probe.ok {
        b"OK"
    } else {
        b"FAILED"
    });
    probe_text(b" iterations=");
    probe_dec(report.heap_probe.iterations);
    probe_text(b" commit_calls=");
    probe_dec(report.heap_probe.commit_calls);
    probe_text(b" commit_pages=");
    probe_dec(report.heap_probe.commit_pages);
    probe_text(b" uncommit_calls=");
    probe_dec(report.heap_probe.uncommit_calls);
    probe_text(b" release_suppressed=");
    probe_dec(report.heap_probe.release_suppressed);
    probe_text(b" poison_bytes=");
    probe_dec(report.heap_probe.poison_bytes);
    probe_text(b" retained_pages=");
    probe_dec(report.heap_probe.retained_pages);
    probe_text(b" block_claims=");
    probe_dec(report.heap_probe.block_claims);
    probe_text(b" extent_allocations=");
    probe_dec(report.heap_probe.extent_allocations);
    probe_text(b" map_batches=");
    probe_dec(report.heap_probe.map_batches);
    probe_text(b" unmap_batches=");
    probe_dec(report.heap_probe.unmap_batches);
    probe_text(b" pressure=");
    probe_text(if report.heap_probe.under_pressure {
        b"yes"
    } else {
        b"no"
    });
    probe_text(b"\r\n");
    probe_text(b"[TLBPROBE] result=");
    probe_text(if report.tlb_ok { b"OK" } else { b"FAILED" });
    probe_text(b" cpus=");
    probe_dec(u64::from(report.online_count));
    probe_text(b" ready_mask=0x");
    probe_hex(report.tlb_ready_mask, 8);
    probe_text(b" updated_mask=0x");
    probe_hex(report.tlb_updated_mask, 8);
    probe_text(b" stale_failures=");
    probe_dec(u64::from(report.tlb_failures));
    probe_text(b" timeout_rejected=");
    probe_dec(u64::from(report.timeout_rejected));
    probe_text(b" frame_retained=");
    probe_dec(u64::from(report.frame_retained));
    probe_text(b" requests=");
    probe_dec(report.tlb_stats.requests);
    probe_text(b" ipis=");
    probe_dec(report.tlb_stats.ipis_sent);
    probe_text(b" acks=");
    probe_dec(report.tlb_stats.acknowledgements);
    probe_text(b" timeouts=");
    probe_dec(report.tlb_stats.timeouts);
    probe_text(b" expected_timeouts=");
    probe_dec(report.tlb_stats.expected_timeouts);
    probe_text(b" generation=");
    probe_dec(report.tlb_stats.generation);
    probe_text(b" cleanup=");
    probe_text(if report.tlb_cleanup_ok { b"yes" } else { b"no" });
    probe_text(b"\r\n");
    probe_text(b"[LOCKPROBE] result=");
    probe_text(if report.lock_ok { b"OK" } else { b"FAILED" });
    probe_text(b" runtime_acquisitions=");
    probe_dec(report.runtime_stats.acquisitions);
    probe_text(b" nested=");
    probe_dec(report.runtime_stats.nested_acquisitions);
    probe_text(b" collisions=");
    probe_dec(report.runtime_stats.collisions);
    probe_text(b" cpu_collisions=");
    probe_dec(report.runtime_stats.cpu_collisions);
    probe_text(b" wait_spins=");
    probe_dec(report.runtime_stats.wait_spins);
    probe_text(b" max_wait=");
    probe_dec(report.runtime_stats.max_wait_spins);
    probe_text(b" max_hold_cycles=");
    probe_dec(report.runtime_stats.max_hold_cycles);
    probe_text(b" legacy_global=");
    probe_dec(report.runtime_stats.legacy_global_acquisitions);
    probe_text(b" runtime_owner_classes=1 owner_classes=");
    probe_dec(owner::CLASS_NAMES.len() as u64);
    probe_text(b" owner_acquisitions=");
    probe_dec(report.owner_stats.acquisitions);
    probe_text(b" owner_collisions=");
    probe_dec(report.owner_stats.collisions);
    probe_text(b" order_violations=");
    probe_dec(report.owner_stats.order_violations);
    probe_text(b" stress_iterations=");
    probe_dec(STRESS * u64::from(report.online_count));
    probe_text(b" stress_mask=0x");
    probe_hex(report.owner_done_mask, 8);
    probe_text(b" stress_failures=");
    probe_dec(u64::from(report.owner_failures));
    probe_text(b"\r\n");
    probe_text(b"[SERIALPROBE] result=");
    probe_text(if report.serial_ok { b"OK" } else { b"FAILED" });
    probe_text(b" write_calls=");
    probe_dec(report.serial_stats.write_calls);
    probe_text(b" bulk_calls=");
    probe_dec(report.serial_stats.bulk_calls);
    probe_text(b" lock_acquisitions=");
    probe_dec(report.serial_stats.lock_acquisitions);
    probe_text(b" status_reads=");
    probe_dec(report.serial_stats.uart_status_reads);
    probe_text(b" ring_bytes=");
    probe_dec(report.serial_stats.ring_bytes);
    probe_text(b" max_span=");
    probe_dec(report.serial_stats.max_span);
    probe_text(b" dropped=");
    probe_dec(report.serial_stats.dropped_bytes);
    probe_text(b"\r\n");
    probe_text(b"[CLOCKPROBE] result=");
    probe_text(if report.clock_ok { b"OK" } else { b"FAILED" });
    probe_text(b" source=");
    probe_text(match clock_status.source {
        0 => b"unavailable",
        1 => b"TSC",
        2 => b"HPET",
        _ => unreachable!(),
    });
    probe_text(b" cpus=");
    probe_dec(u64::from(report.online_count));
    probe_text(b" registered_mask=0x");
    probe_hex(clock_status.registered_cpu_mask, 8);
    probe_text(b" regressions=");
    probe_dec(u64::from(report.clock_regressions));
    probe_text(b" irq_delta=");
    probe_dec(report.irq_delta);
    probe_text(b" generation=");
    probe_dec(u64::from(clock_status.generation));
    probe_text(b" max_skew_ns=");
    probe_dec(clock_status.max_cpu_skew_ns);
    probe_text(b" calibration_ppm=");
    probe_dec(clock_status.calibration_error_ppm);
    probe_text(b" fallback=");
    probe_text(clocksource::fallback_name(clock_status.fallback_reason));
    probe_text(b"\r\n");
}
