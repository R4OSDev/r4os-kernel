//! Exactly one waiting child and its still-running donor. Reuse the admitted
//! live case's event/FPU/unwind worker and real task-retirement receipts.
use super::*;
use crate::sched::task::balance;

unsafe fn ready_count(cpu: u32) -> usize {
    unsafe { (*registry::raw()).ready[cpu as usize].count }
}

// No foreign queue is changed to manufacture an idle CPU. Retry while ordinary
// work occupies the source queue or all other CPUs; every attempt is bounded.
fn release(source: u32) -> Result<Option<u32>, ()> {
    let _runtime = RuntimeGuard::acquire();
    unsafe {
        let parent = scheduler::current().cast::<Task>();
        if parent.is_null() || percpu::current_index() != source {
            return Err(());
        }
        let parent_core = registry::core(parent);
        if (*parent_core).state != State::Running || (*parent_core).cpu_idle {
            return Err(());
        }
        if ready_count(source) != 0 {
            return Ok(None);
        }
        let online = percpu::r4os_rust_percpu_control(14, 0, 0, 0);
        let target = (1..registry::MAX_CPUS as u32).find_map(|offset| {
            let cpu = (source + offset) % registry::MAX_CPUS as u32;
            if online & (1 << cpu) == 0 || ready_count(cpu) != 0 {
                return None;
            }
            let current = scheduler::current_on_cpu(cpu).cast::<Task>();
            (!current.is_null() && (*registry::core(current)).cpu_idle).then_some((cpu, current))
        });
        let Some((target, idle)) = target else {
            return Ok(None);
        };
        let t = TASKS[0].load(Ordering::Acquire);
        let c = registry::core(t);
        let parent_before = facts(parent);
        let next_order = (*registry::raw()).next_ready_order;
        sync_objects::r4os_rust_sync_event(event(0), 0, 0);
        let before = facts(t);
        if !(*c).cpu_affinity_bound
            || (*c).state != State::Ready
            || !(*c).ready_linked
            || (*c).running_cpu != 0xff
            || u32::from((*c).home_cpu) != source
            || ready_count(source) != 1
            || balance::workload(source, parent) != 2
            || balance::workload(target, idle) != 0
            || !balance::pull(source, target, parent, idle, timer::tick_count(), 1).is_null()
            || facts(t) != before
            || facts(parent) != parent_before
        {
            return Err(());
        }
        // Only this owned diagnostic child loses its temporary pin. Exercise
        // the production Ready notification, including pending and exact IPI.
        (*c).cpu_affinity_bound = false;
        scheduler::notify_ready(t.cast());
        if facts(t) != before
            || facts(parent) != parent_before
            || scheduler::current() != parent.cast()
            || scheduler::current_on_cpu(source) != parent.cast()
            || (*parent_core).state != State::Running
            || (*parent_core).running_cpu != source as u8
            || (*c).state != State::Ready
            || (*c).running_cpu != 0xff
            || !(*c).ready_linked
            || !(*c).home_cpu_bound
            || u32::from((*c).home_cpu) != target
            || (*c).last_migration_tick == 0
            || ready_count(source) != 0
            || ready_count(target) != 1
            || (*registry::raw()).next_ready_order != next_order + 1
            || !helpers::queue_valid()
        {
            return Err(());
        }
        Ok(Some(target))
    }
}

pub(super) fn run() -> bool {
    if percpu::r4os_rust_percpu_control(15, 0, 0, 0) < 2 {
        helpers::puts(b"TASKREG0859 single-ready unavailable: requires two CPUs\r\n");
        return true;
    }
    if !prepare() {
        return false;
    }
    let source = percpu::current_index();
    let t = tasks::create_on_cpu(b"spectra-single-ready", worker, Role::Batch, source);
    if t.is_null() {
        return false;
    }
    TASKS[0].store(t, Ordering::Release);
    if !tasks::start(t) {
        finish(true);
        return false;
    }
    let end = timer::tick_count().saturating_add(2000);
    while !enrolled(1) {
        if kernel_check::cancelled()
            || FAILURES.load(Ordering::Acquire) != 0
            || timer::tick_count() >= end
        {
            finish(true);
            return false;
        }
        tasks::sleep(1, b"spectra-single-enrollment");
    }
    let target = loop {
        match release(source) {
            Ok(Some(cpu)) => break cpu,
            Err(()) => {
                finish(true);
                return helpers::fail(b"single-ready-facts");
            }
            Ok(None) => {}
        }
        if kernel_check::cancelled() || timer::tick_count() >= end {
            finish(true);
            return helpers::fail(b"single-ready-idle");
        }
        tasks::sleep(1, b"spectra-single-capacity");
    };
    while DONE.load(Ordering::Acquire) != 1 {
        if kernel_check::cancelled() || timer::tick_count() >= end {
            finish(true);
            return false;
        }
        tasks::sleep(1, b"spectra-single-done");
    }
    let valid = FIRST[0].load(Ordering::Acquire) == source
        && LAST[0].load(Ordering::Acquire) == target
        && RUNS[0].load(Ordering::Acquire) == 1
        && FAILURES.load(Ordering::Acquire) == 0;
    let clean = finish(false) && unsafe { helpers::queue_valid() };
    if valid && clean {
        helpers::puts(b"TASKREG0859 single-ready PASS tasks=1 donor=running target=previously-idle pinned=excluded physical-current=preserved fpu=1 unwind=1 ready=preserved own=retired\r\n");
    }
    valid && clean
}
