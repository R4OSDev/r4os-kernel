//! Shared functional phases; live mode excludes isolated global boot work.
use super::*;
pub(super) fn run(usable: u64, live: bool) -> bool {
    let online = percpu::r4os_rust_percpu_control(14, 0, 0, 0);
    let count = online.count_ones();
    unsafe {
        (*session()).online_mask = online;
        (*session()).stage = 1;
    }
    if live::cancelled() {
        return fail(b"cancelled");
    }
    if count <= 1 {
        text(1, b"");
        return !live;
    }
    let mut cleanup = Cleanup {
        probe: unsafe { ptr::addr_of_mut!((*session()).mapping) },
        pending: true,
    };
    if !unsafe { smp_probe_setup(cleanup.probe) } {
        return fail(b"tlb-setup");
    }
    let start = monotonic::capture();
    let mut expected = 0;
    for index in 0..MAX_CPUS as u32 {
        if live::cancelled() {
            return fail(b"cancelled");
        }
        if online & (1 << index) != 0 {
            expected ^= work(index);
        }
    }
    if elapsed(start).is_none() {
        return fail(b"clock-sequential");
    }
    unsafe {
        (*session()).expected = expected;
        (*session()).stage = 2;
    }
    reset(unsafe { (*cleanup.probe).address });
    let mut workers = [ptr::null_mut(); MAX_CPUS];
    let mut created = 0;
    for index in 0..MAX_CPUS as u32 {
        if online & (1 << index) == 0 {
            continue;
        }
        let worker = unsafe {
            (*session()).tasks.create(
                b"smp-kwork",
                worker_main,
                crate::sched::task::task_types::Role::Interactive,
                Some(index),
            )
        }
        .cast::<c_void>();
        if worker.is_null() {
            return fail(b"worker-create-or-bind");
        }
        workers[created] = worker;
        created += 1;
    }
    let mut placement = 0;
    for worker in workers.iter().take(count as usize) {
        if worker.is_null() {
            return fail(b"worker-missing");
        }
        let target = {
            let _runtime = crate::arch::runtime::RuntimeGuard::acquire();
            unsafe { crate::sched::task::scheduler_view(*worker).home_cpu as u64 }
        };
        if !crate::sched::task::diagnostic::start(worker.cast()) {
            return fail(b"worker-publish");
        }
        let bit = 1u64 << target;
        if online & bit == 0 || placement & bit != 0 {
            FAILURES.fetch_add(1, Ordering::AcqRel);
        }
        placement |= bit;
        unsafe {
            (*session()).placement_mask = placement;
        }
    }
    unsafe {
        (*session()).stage = 3;
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
        if live::cancelled() {
            return fail(b"cancelled");
        }
        scheduler::yield_task();
        if elapsed(tlb_start).is_none_or(|time| time >= TIMEOUT) {
            tlb_runtime_ok = false;
            break;
        }
    }
    if !unsafe { (&mut *cleanup.probe).replace() } {
        return fail(b"tlb-replace");
    }
    PHASE.store(2, Ordering::Release);
    while TLB_UPDATED.load(Ordering::Acquire) & online != online {
        if live::cancelled() {
            return fail(b"cancelled");
        }
        scheduler::yield_task();
        if elapsed(tlb_start).is_none_or(|time| time >= TIMEOUT) {
            tlb_runtime_ok = false;
            break;
        }
    }
    let before = tlb_stats();
    let mut timeout_rejected = false;
    let mut frame_retained = false;
    if !live {
        if unsafe { (*cleanup.probe).mapped_frame } == unsafe { (*cleanup.probe).new_frame }
            && let Some(cpu) = remote(online)
            && tlb::r4os_rust_tlb_control(1, cpu)
        {
            timeout_rejected =
                !paging::r4os_rust_paging_unmap(unsafe { (*cleanup.probe).address }, false);
            tlb::r4os_rust_tlb_control(2, 0);
            let mut mapped = 0;
            frame_retained = unsafe {
                paging::r4os_rust_paging_read((*cleanup.probe).address, 0, false, &mut mapped)
            } && mapped == unsafe { (*cleanup.probe).new_frame }
                && blocks::r4os_rust_blocks_prefix(
                    unsafe { (*cleanup.probe).new_frame },
                    PAGE,
                    false,
                ) == PAGE;
        }
        let after = tlb_stats();
        tlb_runtime_ok = tlb_runtime_ok
            && timeout_rejected
            && frame_retained
            && after.timeouts == before.timeouts + 1
            && after.expected_timeouts == before.expected_timeouts + 1;
    } else {
        let receipt = unsafe { (*cleanup.probe).last_unmap };
        tlb_runtime_ok = tlb_runtime_ok
            && receipt.ok
            && receipt.generation != 0
            && receipt.target_mask != 0
            && receipt.acknowledgement_mask & receipt.target_mask == receipt.target_mask;
    }
    let deadline_before = timer::deadline_stats();
    let parallel_start = monotonic::capture();
    unsafe {
        (*session()).stage = 4;
    }
    PHASE.store(3, Ordering::Release);
    while DONE.load(Ordering::Acquire) < count {
        if live::cancelled() {
            return fail(b"cancelled");
        }
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
    unsafe {
        (*session()).actual = actual;
    }
    let observed = CPUS.load(Ordering::Acquire);
    let failures = FAILURES.load(Ordering::Acquire);
    let regressions = REGRESSIONS.load(Ordering::Acquire);
    let deadline_after = timer::deadline_stats();
    let clock = crate::platform::clocksource::snapshot();
    unsafe {
        (*session()).checked_clock = Some(clock);
    }
    let irq_delta = deadline_after
        .timer_irqs
        .wrapping_sub(deadline_before.timer_irqs);
    let clock_ok = clock.source != 0
        && clock_samples
        && clock.registered_cpu_mask & online == online
        && regressions == 0
        && (live || irq_delta != 0);
    let heap_probe = if live {
        HeapReport::default()
    } else {
        let mut output = core::mem::MaybeUninit::uninit();
        unsafe {
            smp_probe_heap(output.as_mut_ptr());
            output.assume_init()
        }
    };
    let heap_ok = live || heap_probe.ok;
    let stale = TLB_FAILURES.load(Ordering::Acquire);
    let ready = TLB_READY.load(Ordering::Acquire);
    let updated = TLB_UPDATED.load(Ordering::Acquire);
    unsafe {
        (*session()).stage = 6;
    }
    let retired = stop_workers();
    let cleaned = retired && unsafe { smp_probe_cleanup(cleanup.probe) };
    if !cleaned || retained_allocations() != 0 {
        unsafe {
            (*session()).retained = true;
        }
    }
    cleanup.pending = false;
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
    let lock_base = live
        || (runtime_stats.legacy_global_acquisitions == 0 && owner_stats.order_violations == 0);
    let owner_done = OWNER_DONE.load(Ordering::Acquire);
    let owner_failures = OWNER_FAILURES.load(Ordering::Acquire);
    let lock_ok = lock_base
        && owner_failures == 0
        && owner_done & online == online
        && MUTEX_COUNT.load(Ordering::Relaxed) == u64::from(count) * STRESS
        && retained_allocations() == 0;
    let r4l_ok = if live {
        false
    } else if let Some(cpu) = remote(online) {
        unsafe { smp_probe_finish(0, cpu, usable) }
    } else {
        false
    };
    let owners_ok = !live && unsafe { smp_probe_finish(1, 0, 0) };
    let tlb_ok = tlb_runtime_ok
        && cleaned
        && retired
        && stale == 0
        && ready & online == online
        && updated & online == online
        && (live
            || (final_tlb.timeouts == final_tlb.expected_timeouts && final_tlb.successes != 0));
    let ok = (live || owners_ok)
        && failures == 0
        && clock_ok
        && placement == online
        && observed == online
        && actual == expected
        && heap_ok
        && tlb_ok
        && lock_ok
        && (live || serial_ok)
        && (live || r4l_ok);
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
    if !live {
        unsafe { smp_probe_report(&report) };
    }
    unsafe {
        (*session()).stage = if ok {
            0
        } else if !tlb_ok {
            3
        } else if !clock_ok {
            4
        } else if !lock_ok {
            5
        } else {
            2
        };
    }
    ok
}
