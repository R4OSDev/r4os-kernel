//! Existing bounded SMP/TLB/owner/clock diagnostic, with one native state owner.
//! Task and wait objects stay with their 0.83.11 owner behind fixed C cuts.
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
    sync::atomic::{AtomicU8, AtomicU32, AtomicU64, Ordering},
};
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
#[repr(C)]
pub struct TlbProbe {
    range_id: u32,
    address: u64,
    old_frame: u64,
    new_frame: u64,
    mapped_frame: u64,
}
#[repr(C)]
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
fn task_call(
    operation: u8,
    task: *mut c_void,
    value: u64,
    entry: Option<unsafe extern "C" fn()>,
) -> u64 {
    unsafe { smp_probe_task(operation, task, value, entry) }
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
    probe: TlbProbe,
    pending: bool,
    release_workers: bool,
}
impl Drop for Cleanup {
    fn drop(&mut self) {
        // Match original defer order: wake retiring workers before cleanup.
        if self.release_workers {
            RETIRE.store(1, Ordering::Release);
        }
        if self.pending {
            unsafe { smp_probe_cleanup(&mut self.probe) };
        }
    }
}
fn reset(address: u64) {
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
    let online = percpu::r4os_rust_percpu_control(14, 0, 0, 0);
    let count = online.count_ones();
    if count <= 1 {
        text(1, b"");
        return true;
    }
    let mut probe = core::mem::MaybeUninit::uninit();
    if !unsafe { smp_probe_setup(probe.as_mut_ptr()) } {
        return fail(b"tlb-setup");
    }
    let mut cleanup = Cleanup {
        probe: unsafe { probe.assume_init() },
        pending: true,
        release_workers: false,
    };
    let start = monotonic::capture();
    let mut expected = 0;
    for index in 0..MAX_CPUS as u32 {
        if online & (1 << index) != 0 {
            expected ^= work(index);
        }
    }
    if elapsed(start).is_none() {
        return fail(b"clock-sequential");
    }
    reset(cleanup.probe.address);
    cleanup.release_workers = true;
    let mut workers = [ptr::null_mut(); MAX_CPUS];
    let mut created = 0;
    for index in 0..MAX_CPUS as u32 {
        if online & (1 << index) == 0 {
            continue;
        }
        let worker: *mut c_void =
            ptr::with_exposed_provenance_mut(
                task_call(0, ptr::null_mut(), 0, Some(worker_main)) as usize
            );
        if worker.is_null() {
            return fail(b"worker-create");
        }
        if task_call(1, worker, u64::from(index), None) == 0 {
            return fail(b"worker-bind");
        }
        workers[created] = worker;
        created += 1;
    }
    let ready_tick = timer::tick_count();
    let mut placement = 0;
    for worker in workers.iter().take(count as usize) {
        if worker.is_null() {
            return fail(b"worker-missing");
        }
        let target = task_call(2, *worker, ready_tick, None);
        let bit = 1u64 << target;
        if online & bit == 0 || placement & bit != 0 {
            FAILURES.fetch_add(1, Ordering::AcqRel);
        }
        placement |= bit;
    }
    let tlb_start = monotonic::capture();
    RELEASE.store(1, Ordering::Release);
    for index in 1..MAX_CPUS as u32 {
        if online & (1 << index) != 0 {
            smp::r4os_rust_smp_control(4, index);
        }
    }
    let mut tlb_runtime_ok = true;
    while TLB_READY.load(Ordering::Acquire) & online != online {
        scheduler::yield_task();
        if elapsed(tlb_start).unwrap_or(0) >= TIMEOUT {
            tlb_runtime_ok = false;
            break;
        }
    }
    if paging::r4os_rust_paging_unmap(cleanup.probe.address, false) {
        cleanup.probe.mapped_frame = 0;
        if paging::r4os_rust_paging_map(
            cleanup.probe.address,
            cleanup.probe.new_frame,
            2 | (1 << 63),
            false,
        ) {
            cleanup.probe.mapped_frame = cleanup.probe.new_frame;
        } else {
            tlb_runtime_ok = false;
            if paging::r4os_rust_paging_map(
                cleanup.probe.address,
                cleanup.probe.old_frame,
                2 | (1 << 63),
                false,
            ) {
                cleanup.probe.mapped_frame = cleanup.probe.old_frame;
            }
        }
    } else {
        tlb_runtime_ok = false;
    }
    PHASE.store(2, Ordering::Release);
    while TLB_UPDATED.load(Ordering::Acquire) & online != online {
        scheduler::yield_task();
        if elapsed(tlb_start).unwrap_or(0) >= TIMEOUT {
            tlb_runtime_ok = false;
            break;
        }
    }
    let before = tlb_stats();
    let mut timeout_rejected = false;
    let mut frame_retained = false;
    if cleanup.probe.mapped_frame == cleanup.probe.new_frame
        && let Some(cpu) = remote(online)
        && tlb::r4os_rust_tlb_control(1, cpu)
    {
        timeout_rejected = !paging::r4os_rust_paging_unmap(cleanup.probe.address, false);
        tlb::r4os_rust_tlb_control(2, 0);
        let mut mapped = 0;
        frame_retained =
            unsafe { paging::r4os_rust_paging_read(cleanup.probe.address, 0, false, &mut mapped) }
                && mapped == cleanup.probe.new_frame
                && blocks::r4os_rust_blocks_prefix(cleanup.probe.new_frame, PAGE, false) == PAGE;
    }
    let after = tlb_stats();
    tlb_runtime_ok = tlb_runtime_ok
        && timeout_rejected
        && frame_retained
        && after.timeouts == before.timeouts + 1
        && after.expected_timeouts == before.expected_timeouts + 1;
    let deadline_before = timer::deadline_stats();
    let parallel_start = monotonic::capture();
    PHASE.store(3, Ordering::Release);
    while DONE.load(Ordering::Acquire) < count {
        scheduler::yield_task();
        let Some(time) = elapsed(parallel_start) else {
            return fail(b"clock-parallel");
        };
        if time >= TIMEOUT {
            return fail(b"timeout");
        }
    }
    if elapsed(parallel_start).is_none() {
        return fail(b"clock-parallel");
    }
    let mut actual = 0;
    let mut clock_samples = true;
    for (index, result) in RESULTS.iter().enumerate() {
        if online & (1 << index) == 0 {
            continue;
        }
        actual ^= result.load(Ordering::Relaxed);
        clock_samples = clock_samples
            && CLOCK_STARTS[index].load(Ordering::Relaxed) != 0
            && CLOCK_ENDS[index].load(Ordering::Relaxed)
                > CLOCK_STARTS[index].load(Ordering::Relaxed)
            && TICK_ENDS[index].load(Ordering::Relaxed)
                >= TICK_STARTS[index].load(Ordering::Relaxed);
    }
    let observed = CPUS.load(Ordering::Acquire);
    let failures = FAILURES.load(Ordering::Acquire);
    let regressions = REGRESSIONS.load(Ordering::Acquire);
    let deadline_after = timer::deadline_stats();
    let clock = crate::platform::clocksource::snapshot();
    let irq_delta = deadline_after
        .timer_irqs
        .wrapping_sub(deadline_before.timer_irqs);
    let clock_ok = clock.source != 0
        && clock_samples
        && clock.registered_cpu_mask & online == online
        && regressions == 0
        && irq_delta != 0;
    let mut heap_output = core::mem::MaybeUninit::uninit();
    unsafe { smp_probe_heap(heap_output.as_mut_ptr()) };
    let heap_probe = unsafe { heap_output.assume_init() };
    let heap_ok = heap_probe.ok;
    let stale = TLB_FAILURES.load(Ordering::Acquire);
    let ready = TLB_READY.load(Ordering::Acquire);
    let updated = TLB_UPDATED.load(Ordering::Acquire);
    let cleaned = unsafe { smp_probe_cleanup(&mut cleanup.probe) };
    cleanup.pending = false;
    RETIRE.store(1, Ordering::Release);
    let final_tlb = tlb_stats();
    let mut runtime_output = core::mem::MaybeUninit::uninit();
    let mut owner_output = core::mem::MaybeUninit::uninit();
    let mut serial_output = core::mem::MaybeUninit::uninit();
    let serial_ok = unsafe {
        smp_probe_counters(
            runtime_output.as_mut_ptr(),
            owner_output.as_mut_ptr(),
            serial_output.as_mut_ptr(),
        )
    };
    let runtime_stats = unsafe { runtime_output.assume_init() };
    let owner_stats = unsafe { owner_output.assume_init() };
    let serial_stats = unsafe { serial_output.assume_init() };
    let lock_base =
        runtime_stats.legacy_global_acquisitions == 0 && owner_stats.order_violations == 0;
    let owner_done = OWNER_DONE.load(Ordering::Acquire);
    let owner_failures = OWNER_FAILURES.load(Ordering::Acquire);
    let lock_ok = lock_base
        && owner_failures == 0
        && owner_done & online == online
        && MUTEX_COUNT.load(Ordering::Relaxed) == u64::from(count) * STRESS;
    let r4l_ok = if let Some(cpu) = remote(online) {
        unsafe { smp_probe_finish(0, cpu, usable) }
    } else {
        false
    };
    let owners_ok = unsafe { smp_probe_finish(1, 0, 0) };
    let tlb_ok = tlb_runtime_ok
        && cleaned
        && stale == 0
        && ready & online == online
        && updated & online == online
        && final_tlb.timeouts == final_tlb.expected_timeouts
        && final_tlb.successes != 0;
    let ok = owners_ok
        && failures == 0
        && clock_ok
        && placement == online
        && observed == online
        && actual == expected
        && heap_ok
        && tlb_ok
        && lock_ok
        && serial_ok
        && r4l_ok;
    let report = Report {
        ok,
        clock_ok,
        tlb_ok,
        lock_ok,
        serial_ok,
        timeout_rejected,
        frame_retained,
        tlb_cleanup_ok: cleaned,
        online_count: count,
        failures,
        clock_regressions: regressions,
        tlb_failures: stale,
        owner_failures,
        online_mask: online,
        observed_mask: observed,
        tlb_ready_mask: ready,
        tlb_updated_mask: updated,
        owner_done_mask: owner_done,
        irq_delta,
        heap_probe,
        clock_status: clock,
        tlb_stats: final_tlb,
        runtime_stats,
        owner_stats,
        serial_stats,
    };
    unsafe { smp_probe_report(&report) };
    ok
}
fn owner_stress(cpu: u32, bit: u64) {
    for iteration in 0..STRESS {
        let size = if iteration == 0 {
            (cpu as usize + 1) * 512 * 1024
        } else {
            64 + ((iteration + u64::from(cpu)) & 7) as usize * 32
        };
        let memory = heap::r4os_rust_heap_alloc(size, 16);
        if memory.is_null() {
            OWNER_FAILURES.fetch_add(1, Ordering::AcqRel);
            continue;
        }
        let witness = (iteration ^ u64::from(cpu)) as u8;
        unsafe {
            memory.write(witness);
            memory.add(size - 1).write(!witness)
        };
        let valid = unsafe { memory.read() == witness && memory.add(size - 1).read() == !witness };
        let freed = unsafe { heap::r4os_rust_heap_free(memory, size) };
        if !valid || freed != 0 {
            OWNER_FAILURES.fetch_add(1, Ordering::AcqRel);
        }
        if !unsafe { smp_probe_mutex(1) } {
            OWNER_FAILURES.fetch_add(1, Ordering::AcqRel);
            break;
        }
        // Keep checked arithmetic and the original exact sleepable mutex.
        let count = MUTEX_COUNT.load(Ordering::Relaxed) + 1;
        MUTEX_COUNT.store(count, Ordering::Relaxed);
        if !unsafe { smp_probe_mutex(2) } {
            OWNER_FAILURES.fetch_add(1, Ordering::AcqRel);
            break;
        }
    }
    OWNER_DONE.fetch_or(bit, Ordering::AcqRel);
}
unsafe extern "C" fn worker_main() {
    let cpu = percpu::current_index();
    while RELEASE.load(Ordering::Acquire) == 0 {
        core::hint::spin_loop();
    }
    if cpu as usize >= MAX_CPUS {
        FAILURES.fetch_add(1, Ordering::AcqRel);
    } else {
        let bit = 1u64 << cpu;
        let previous = CPUS.fetch_or(bit, Ordering::AcqRel);
        if previous & bit != 0 {
            FAILURES.fetch_add(1, Ordering::AcqRel);
        } else {
            let word =
                ptr::with_exposed_provenance::<u64>(ADDRESS.load(Ordering::Acquire) as usize);
            if PHASE.load(Ordering::Acquire) != 1 || unsafe { word.read_volatile() } != OLD {
                TLB_FAILURES.fetch_add(1, Ordering::AcqRel);
            }
            TLB_READY.fetch_or(bit, Ordering::AcqRel);
            while PHASE.load(Ordering::Acquire) < 2 {
                scheduler::yield_task();
            }
            if unsafe { word.read_volatile() } != NEW {
                TLB_FAILURES.fetch_add(1, Ordering::AcqRel);
            }
            TLB_UPDATED.fetch_or(bit, Ordering::AcqRel);
            while PHASE.load(Ordering::Acquire) < 3 {
                scheduler::yield_task();
            }
            owner_stress(cpu, bit);
            let start = monotonic::now_nanoseconds().unwrap_or(0);
            let tick_start = timer::tick_count();
            let mut prior = start;
            for _ in 0..256 {
                let now = monotonic::now_nanoseconds().unwrap_or(0);
                if now < prior {
                    REGRESSIONS.fetch_add(1, Ordering::AcqRel);
                }
                prior = now;
            }
            let index = cpu as usize;
            RESULTS[index].store(work(cpu), Ordering::Relaxed);
            let tick_end = timer::tick_count();
            let end = monotonic::now_nanoseconds().unwrap_or(0);
            CLOCK_STARTS[index].store(start, Ordering::Relaxed);
            CLOCK_ENDS[index].store(end, Ordering::Relaxed);
            TICK_STARTS[index].store(tick_start, Ordering::Relaxed);
            TICK_ENDS[index].store(tick_end, Ordering::Relaxed);
            let deadline = timer::r4os_rust_timer_control(6, tick_start, 2);
            if start == 0
                || end <= start
                || tick_end < tick_start
                || deadline <= tick_start
                || timer::r4os_rust_timer_control(8, tick_start, deadline) != 2
            {
                FAILURES.fetch_add(1, Ordering::AcqRel);
            }
        }
    }
    DONE.fetch_add(1, Ordering::Release);
    while RETIRE.load(Ordering::Acquire) == 0 {
        scheduler::yield_task();
    }
}
const _: () = {
    assert!(core::mem::size_of::<TlbProbe>() == 40);
    assert!(core::mem::offset_of!(TlbProbe, address) == 8);
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
    use crate::memory::{phys, vm_query as vq, vm_ranges as vr, vm_transactions as vm};
    let r = vr::Reserve {
        eviction_allowed: true,
        window: 0,
        kind: 4,
        owner: 0,
        owner_id: 0,
        name_ptr: b"tlb-acceptance".as_ptr(),
        name_len: 14,
        flags: 2 | (1 << 63),
        base: 0,
        len: PAGE,
    };
    let mut id = 0;
    if unsafe { vr::r4os_rust_vm_reserve(&r, PAGE, &mut id) } != 0 {
        return false;
    }
    let mut range = core::mem::MaybeUninit::uninit();
    if !unsafe { vq::r4os_rust_vm_range_info(id, range.as_mut_ptr()) } {
        vm::r4os_rust_vm_release(id);
        return false;
    }
    let range = unsafe { range.assume_init() };
    let Some(old) = claim_probe_frame(b"tlb-probe-old") else {
        vm::r4os_rust_vm_release(id);
        return false;
    };
    let Some(new) = claim_probe_frame(b"tlb-probe-new") else {
        release_probe_frame(old);
        vm::r4os_rust_vm_release(id);
        return false;
    };
    unsafe {
        ptr::with_exposed_provenance_mut::<u64>(phys::r4os_rust_phys_to_virt(old) as usize)
            .write_volatile(OLD);
        ptr::with_exposed_provenance_mut::<u64>(phys::r4os_rust_phys_to_virt(new) as usize)
            .write_volatile(NEW);
    }
    if !paging::r4os_rust_paging_map(range.base, old, 2 | (1 << 63), false) {
        release_probe_frame(old);
        release_probe_frame(new);
        vm::r4os_rust_vm_release(id);
        return false;
    }
    unsafe {
        output.write(TlbProbe {
            range_id: id,
            address: range.base,
            old_frame: old,
            new_frame: new,
            mapped_frame: old,
        });
    }
    true
}

unsafe fn smp_probe_cleanup(probe: *mut TlbProbe) -> bool {
    let p = unsafe { &mut *probe };
    let mut ok = true;
    if p.mapped_frame != 0 {
        let mut frame = 0;
        if unsafe { paging::r4os_rust_paging_read(p.address, 0, false, &mut frame) }
            && frame == p.mapped_frame
            && paging::r4os_rust_paging_unmap(p.address, false)
        {
            p.mapped_frame = 0;
        } else {
            ok = false;
        }
    }
    if p.mapped_frame != p.old_frame {
        ok = release_probe_frame(p.old_frame) && ok;
    }
    if p.mapped_frame != p.new_frame {
        ok = release_probe_frame(p.new_frame) && ok;
    }
    if p.mapped_frame == 0 && crate::memory::vm_transactions::r4os_rust_vm_release(p.range_id) != 0
    {
        ok = false;
    }
    ok
}

unsafe fn smp_probe_task(
    operation: u8,
    task: *mut c_void,
    value: u64,
    entry: Option<unsafe extern "C" fn()>,
) -> u64 {
    use crate::sched::task::task_api;
    match operation {
        0 => {
            let r = task_api::CreateRequest {
                name: b"smp-kwork".as_ptr(),
                name_len: 9,
                state: 3,
                role: 1,
                needs_fpu: true,
                smp_eligible: true,
                smp_r4x: false,
                owner_kind: 0,
                owner_context: ptr::null_mut(),
                entry,
            };
            let mut failure = 0;
            unsafe { task_api::r4os_rust_task_create(&r, false, &mut failure) }.addr() as u64
        }
        1 => unsafe { task_api::r4os_rust_task_mutate(24, task, value, 0, ptr::null(), 0) },
        2 => unsafe {
            task_api::r4os_rust_task_mutate(0, task, value, 0, ptr::null(), 0);
            task_api::r4os_rust_task_field(task, 24)
        },
        _ => unreachable!(),
    }
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
