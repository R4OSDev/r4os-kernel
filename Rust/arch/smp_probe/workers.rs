//! Bound FPU-ready workers retain their exact task/heap resources through exit.
use super::*;
fn owner_stress(cpu: u32, bit: u64) {
    for iteration in 0..STRESS {
        if ABORT.load(Ordering::Acquire) {
            break;
        }
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
        OWNED_SIZES[cpu as usize].store(size as u64, Ordering::Relaxed);
        OWNED_POINTERS[cpu as usize].store(memory.addr() as u64, Ordering::Release);
        let witness = (iteration ^ u64::from(cpu)) as u8;
        unsafe {
            memory.write(witness);
            memory.add(size - 1).write(!witness)
        };
        let valid = unsafe { memory.read() == witness && memory.add(size - 1).read() == !witness };
        let freed = unsafe { heap::r4os_rust_heap_free(memory, size) };
        if freed == 0 {
            OWNED_POINTERS[cpu as usize].store(0, Ordering::Release);
            OWNED_SIZES[cpu as usize].store(0, Ordering::Relaxed);
        }
        if !valid || freed != 0 {
            OWNER_FAILURES.fetch_add(1, Ordering::AcqRel);
            if freed != 0 {
                break;
            }
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
pub(super) unsafe extern "C" fn worker_main() {
    let Some(_admission) = crate::sched::task_context::Admission::acquire(true) else {
        FAILURES.fetch_add(1, Ordering::AcqRel);
        DONE.fetch_add(1, Ordering::Release);
        return;
    };
    let cpu = percpu::current_index();
    while RELEASE.load(Ordering::Acquire) == 0 && !ABORT.load(Ordering::Acquire) {
        crate::sched::task::diagnostic::sleep(1, b"smp-probe-start-barrier");
    }
    if ABORT.load(Ordering::Acquire) {
        DONE.fetch_add(1, Ordering::Release);
        return;
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
            while PHASE.load(Ordering::Acquire) < 2 && !ABORT.load(Ordering::Acquire) {
                scheduler::yield_task();
            }
            if ABORT.load(Ordering::Acquire) {
                DONE.fetch_add(1, Ordering::Release);
                return;
            }
            if unsafe { word.read_volatile() } != NEW {
                TLB_FAILURES.fetch_add(1, Ordering::AcqRel);
            }
            TLB_UPDATED.fetch_or(bit, Ordering::AcqRel);
            while PHASE.load(Ordering::Acquire) < 3 && !ABORT.load(Ordering::Acquire) {
                scheduler::yield_task();
            }
            if ABORT.load(Ordering::Acquire) {
                DONE.fetch_add(1, Ordering::Release);
                return;
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
