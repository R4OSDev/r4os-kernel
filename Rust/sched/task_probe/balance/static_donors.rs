//! Exact owned queues under Runtime. Foreign publication is made by this
//! session's own thirteenth Task; no unrelated queue or physical current moves.
use super::*;
use crate::sched::task::balance;
use core::cell::Cell;

const DONORS: usize = 12;
const ACTORS: usize = DONORS + 1;

unsafe extern "C" fn worker() {
    if scheduler::current() == TASKS[0].load(Ordering::Acquire).cast() {
        FIRST[0].store(percpu::current_index(), Ordering::Release);
    }
}

fn attempt(actors: &[*mut Task; ACTORS]) -> Result<Option<u32>, &'static [u8]> {
    let _runtime = RuntimeGuard::acquire();
    unsafe {
        let mut idle = (1..registry::MAX_CPUS as u32).filter(|&cpu| {
            let current = scheduler::current_on_cpu(cpu).cast::<Task>();
            percpu::is_schedulable(cpu)
                && !current.is_null()
                && (*registry::core(current)).cpu_idle
                && (*registry::raw()).ready[cpu as usize].count == 0
        });
        let (Some(source), Some(target)) = (idle.next(), idle.next()) else {
            return Ok(None);
        };
        let current = scheduler::current_on_cpu(source).cast::<Task>();
        let target_current = scheduler::current_on_cpu(target).cast::<Task>();
        for &task in &actors[..DONORS] {
            if helpers::mutate(24, task, u64::from(source), 0) == 0 || !tasks::start(task) {
                return Err(b"static-donor-admission");
            }
        }
        let foreign = actors[DONORS];
        if !tasks::start(foreign) {
            return Err(b"static-donor-foreign-admission");
        }
        let queue = ptr::addr_of_mut!((*registry::raw()).ready[source as usize]);
        if (*queue).count != DONORS {
            return Err(b"static-donor-own-membership");
        }
        // A real own in-place affinity roundtrip starts a fresh traversal;
        // neither the queue cursor nor any foreign state is reset for it.
        let first = registry::core(actors[0]);
        (*first).cpu_affinity_bound = false;
        balance::note_ready_publication(actors[0]);
        (*first).cpu_affinity_bound = true;
        balance::note_ready_publication(actors[0]);
        let revision = (*queue).transfer_revision;
        let clocks = Cell::new(0u32);
        let mut clock = || {
            clocks.set(clocks.get() + 1);
            timer::tick_count()
        };
        for turn in 0..2 {
            let global = (*registry::raw()).ready_revision;
            if !tasks::republish_unstarted_ready(foreign)
                || (*queue).transfer_revision != revision
                || (*registry::raw()).ready_revision == global
                || !balance::pull(
                    source,
                    target,
                    current,
                    target_current,
                    &mut clock,
                    scheduler::PLACEMENT_COOLDOWN,
                )
                .is_null()
                || clocks.get() != 0
                || (*queue).transfer_summary.excludes(target) != (turn == 1)
            {
                return Err(b"static-donor-progress");
            }
        }
        let cursor = (*queue).balance_cursor;
        if !tasks::republish_unstarted_ready(foreign)
            || (*queue).transfer_revision != revision
            || balance::prepare(source, target, current, target_current).is_some()
            || (*queue).balance_cursor != cursor
            || !(*queue).transfer_summary.excludes(target)
        {
            return Err(b"static-donor-complete-retention");
        }
        // Release exactly the ninth Task from the real shared cursor. A
        // single bounded prefix must neither find it nor prove it absent.
        let mut selected = if cursor.is_null() {
            (*queue).head
        } else {
            cursor
        };
        for _ in 0..8 {
            let next = (*registry::core(selected)).ready_next;
            selected = if next.is_null() { (*queue).head } else { next };
        }
        if !actors[..DONORS].contains(&selected) {
            return Err(b"static-donor-selected-identity");
        }
        let before = facts(selected);
        (*registry::core(selected)).cpu_affinity_bound = false;
        balance::note_ready_publication(selected);
        TASKS[0].store(selected, Ordering::Release);
        if (*queue).transfer_revision == revision
            || !balance::pull(
                source,
                target,
                current,
                target_current,
                &mut clock,
                scheduler::PLACEMENT_COOLDOWN,
            )
            .is_null()
            || (*queue).transfer_summary.excludes(target)
            || clocks.get() != 0
        {
            return Err(b"static-donor-local-policy-prefix");
        }
        let moved = balance::pull(
            source,
            target,
            current,
            target_current,
            &mut clock,
            scheduler::PLACEMENT_COOLDOWN,
        );
        if moved != selected
            || facts(selected) != before
            || u32::from((*registry::core(selected)).home_cpu) != target
            || (*registry::core(selected)).running_cpu != 0xff
            || clocks.get() != 1
            || FIRST[0].load(Ordering::Acquire) != u32::MAX
            || !helpers::queue_valid()
        {
            return Err(b"static-donor-saved-transfer");
        }
        // The actual Scheduler publishes its unchanged destination wake/IPI.
        // Runtime and all local queue borrows end before the Task can execute.
        scheduler::notify_ready(selected.cast());
        Ok(Some(target))
    }
}

pub(super) fn run() -> bool {
    if !prepare() {
        return false;
    }
    let mut actors = [ptr::null_mut(); ACTORS];
    let mut error: Option<&'static [u8]> = None;
    for task in &mut actors {
        *task = tasks::create_on_cpu(b"spectra-static-donor", worker, Role::Batch, 0);
        if task.is_null() {
            error = Some(b"static-donor-allocation");
            break;
        }
    }
    let end = timer::tick_count().saturating_add(2000);
    let mut expected = None;
    while error.is_none() && expected.is_none() {
        if kernel_check::cancelled() || timer::tick_count() >= end {
            error = Some(b"static-donor-capacity");
            break;
        }
        match attempt(&actors) {
            Ok(Some(target)) => expected = Some(target),
            Ok(None) => wait_capacity(b"spectra-static-donor-capacity"),
            Err(reason) => error = Some(reason),
        }
    }
    while error.is_none() && FIRST[0].load(Ordering::Acquire) == u32::MAX {
        if kernel_check::cancelled() || timer::tick_count() >= end {
            error = Some(b"static-donor-actual-dispatch");
            break;
        }
        tasks::sleep(1, b"spectra-static-donor-dispatch");
    }
    let observed = FIRST[0].load(Ordering::Acquire);
    let passed = error.is_none() && expected == Some(observed);
    let mut clean = true;
    for task in actors {
        if !task.is_null() {
            if tasks::finish_task(task, !passed) {
                if TASKS[0].load(Ordering::Acquire) == task {
                    TASKS[0].store(ptr::null_mut(), Ordering::Release);
                }
            } else {
                clean = false;
            }
        }
    }
    if let Some(reason) = error {
        helpers::fail(reason);
    }
    if passed && clean && unsafe { helpers::queue_valid() } {
        helpers::puts(b"TASKREG08861 static-donor PASS donors=12 foreign-republish=3 visits=8/8 complete=retained local-policy=restarted ninth=transferred clock=0/1 execution-cpu=");
        helpers::dec(u64::from(observed));
        helpers::puts(b" tasks=13 own=retired\r\n");
        true
    } else {
        false
    }
}
