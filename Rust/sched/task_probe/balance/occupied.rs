//! Own four-CPU fixture: all currents productive and every possible target
//! already has an older/newer Ready mix. Only real periodic timer checkpoints
//! can move the newly unpinned saved workers; no explicit transfer/notification.
use super::*;
const PEER_COUNT: usize = 6;
const PEER_ALL: u32 = (1 << PEER_COUNT) - 1;
static PEERS: [AtomicPtr<Task>; PEER_COUNT] =
    [const { AtomicPtr::new(ptr::null_mut()) }; PEER_COUNT];
static BUSY: AtomicU32 = AtomicU32::new(0);
static PEER_DONE: AtomicU32 = AtomicU32::new(0);
static RELEASE: AtomicBool = AtomicBool::new(false);
static PREFLIGHT: AtomicU32 = AtomicU32::new(0);

unsafe fn ready_queue(cpu: u32) -> *mut registry::ReadyQueue {
    unsafe { ptr::addr_of_mut!((*registry::raw()).ready[cpu as usize]) }
}

unsafe extern "C" fn peer() {
    let t = scheduler::current().cast::<Task>();
    let Some(index) = PEERS.iter().position(|v| v.load(Ordering::Acquire) == t) else {
        FAILURES.fetch_add(1, Ordering::Relaxed);
        return;
    };
    if index < 3 {
        let Some(_admission) = Admission::acquire(true) else {
            FAILURES.fetch_add(1, Ordering::Relaxed);
            PEER_DONE.fetch_or(1 << index, Ordering::Release);
            return;
        };
        let (identity, token, depth) = {
            let _runtime = RuntimeGuard::acquire();
            unsafe {
                let c = registry::core(t);
                let depth = (*c).unwind_guard_count;
                (((*c).id, (*c).generation), task_context::enter(), depth)
            }
        };
        let expected = [
            0x0851_2000 + index as u64,
            identity.1,
            !identity.1,
            0xfedc_ba98_7654_3210,
        ];
        unsafe { fpu::load(&expected) };
        BUSY.fetch_or(1 << index, Ordering::Release);
        while !RELEASE.load(Ordering::Acquire) && !kernel_check::cancelled() {
            core::hint::spin_loop();
        }
        let observed = unsafe { fpu::read() };
        let valid = {
            let _runtime = RuntimeGuard::acquire();
            unsafe {
                let c = registry::core(t);
                let valid = ((*c).id, (*c).generation) == identity
                    && scheduler::current() == t.cast()
                    && token.active
                    && token.counter == ptr::addr_of_mut!((*c).unwind_guard_count)
                    && (*c).unwind_guard_count == depth + 1;
                let left = task_context::leave(token);
                valid && left && (*c).unwind_guard_count == depth
            }
        };
        if !ABORT.load(Ordering::Acquire) && (!valid || observed != expected) {
            FAILURES.fetch_add(1, Ordering::Relaxed);
        }
    }
    if PEER_DONE.fetch_or(1 << index, Ordering::Release) & (1 << index) != 0 {
        FAILURES.fetch_add(1, Ordering::Relaxed);
    }
}
fn clean(abort: bool) -> bool {
    RELEASE.store(true, Ordering::Release);
    let mut valid = finish(abort);
    for slot in &PEERS {
        let t = slot.load(Ordering::Acquire);
        if !t.is_null() {
            if tasks::finish_task(t, abort) {
                slot.store(ptr::null_mut(), Ordering::Release);
            } else {
                valid = false;
            }
        }
    }
    valid
}
fn start_busy() -> Result<bool, ()> {
    let _runtime = RuntimeGuard::acquire();
    for cpu in 0..4 {
        unsafe {
            let current = scheduler::current_on_cpu(cpu).cast::<Task>();
            if current.is_null()
                || (*ready_queue(cpu)).count != 0
                || (cpu != 0 && !(*registry::core(current)).cpu_idle)
            {
                return Ok(false);
            }
        }
    }
    for slot in &PEERS[..3] {
        if !tasks::start(slot.load(Ordering::Acquire)) {
            return Err(());
        }
    }
    Ok(true)
}
struct Prepared {
    facts: [Facts; COUNT],
    irq_flags: u64,
    source_load: usize,
    target_loads: [usize; 3],
}
fn enqueue() -> Result<Option<Prepared>, u32> {
    let irq_flags = crate::arch::runtime::save_local();
    let result = {
        let _runtime = RuntimeGuard::acquire();
        (|| unsafe {
            if irq_flags & (1 << 9) == 0 {
                return Err(8);
            }
            let controller = scheduler::current().cast::<Task>();
            if percpu::current_index() != 0
                || controller.is_null()
                || (*registry::core(controller)).state != State::Running
            {
                return Err(1);
            }
            for (i, slot) in PEERS[..3].iter().enumerate() {
                let current = scheduler::current_on_cpu(i as u32 + 1).cast::<Task>();
                if current != slot.load(Ordering::Acquire)
                    || (*registry::core(current)).state != State::Running
                {
                    PREFLIGHT.store(100 + i as u32, Ordering::Relaxed);
                    return Ok(None);
                }
            }
            let order = (*registry::raw()).next_ready_order;
            for i in 0..COUNT {
                sync_objects::r4os_rust_sync_event(event(i), 0, 0);
            }
            let before = core::array::from_fn(|i| facts(TASKS[i].load(Ordering::Acquire)));
            // Each target's existing filler is NEWER than every source worker.
            // A periodic migration must insert before it, across different ranks.
            for slot in &PEERS[3..] {
                if !tasks::start(slot.load(Ordering::Acquire)) {
                    return Err(3);
                }
            }
            if (*ready_queue(0)).count < COUNT {
                return Err(4);
            }
            if (1..4).any(|cpu| (*ready_queue(cpu)).count < 1) {
                return Err(5);
            }
            if (*registry::raw()).next_ready_order != order + 7 {
                return Err(6);
            }
            if !helpers::queue_valid() {
                return Err(7);
            }
            // Existing Ready work is legitimate load, never a reason to
            // clear another owner's queue. Record actual post-enqueue work.
            let source_load = crate::sched::task::balance::workload(0, controller);
            let target_loads = core::array::from_fn(|i| {
                crate::sched::task::balance::workload(
                    i as u32 + 1,
                    PEERS[i].load(Ordering::Acquire),
                )
            });
            if source_load < COUNT + 1
                || !target_loads
                    .iter()
                    .any(|&load| source_load > load.saturating_add(1))
            {
                return Err(9);
            }
            for slot in &TASKS[..3] {
                (*registry::core(slot.load(Ordering::Acquire))).cpu_affinity_bound = false;
            }
            Ok(Some((before, source_load, target_loads)))
        })()
    };
    // The successful phase must observe AP timer transfers independently of
    // BSP timeout/IRQ wake balancing. No runtime owner spans this local IF
    // suppression, and no sleep/yield occurs until the original IF is restored.
    if !matches!(&result, Ok(Some(_))) {
        crate::arch::runtime::restore_local(irq_flags);
    }
    result.map(|value| {
        value.map(|(facts, source_load, target_loads)| Prepared {
            facts,
            irq_flags,
            source_load,
            target_loads,
        })
    })
}
fn observe(before: &[Facts; COUNT]) -> Result<Option<(u32, u32)>, ()> {
    let _runtime = RuntimeGuard::acquire();
    let mut moved = 0;
    let mut targets = 0;
    unsafe {
        for (i, slot) in TASKS.iter().enumerate() {
            let t = slot.load(Ordering::Acquire);
            let c = registry::core(t);
            if facts(t) != before[i]
                || (*c).state != State::Ready
                || (*c).running_cpu != 0xff
                || !(*c).ready_linked
            {
                return Err(());
            }
            let cpu = u32::from((*c).home_cpu);
            if cpu != 0 {
                if i == 3
                    || !(1..4).contains(&cpu)
                    || (*c).last_migration_tick == 0
                    || !(*c).last_migration_periodic
                {
                    return Err(());
                }
                let busy = PEERS[cpu as usize - 1].load(Ordering::Acquire);
                let filler = PEERS[cpu as usize + 2].load(Ordering::Acquire);
                if scheduler::current_on_cpu(cpu) != busy.cast()
                    || (*registry::core(busy)).state != State::Running
                    || !(*registry::core(filler)).ready_linked
                    || !(*registry::core(filler)).cpu_affinity_bound
                    || (*registry::core(filler)).ready_order <= (*c).ready_order
                    || (*ready_queue(cpu)).count < 2
                {
                    return Err(());
                }
                targets |= 1 << cpu;
                moved += 1;
            }
        }
        let bound = registry::core(TASKS[3].load(Ordering::Acquire));
        if !(*bound).cpu_affinity_bound || (*bound).home_cpu != 0 || !helpers::queue_valid() {
            return Err(());
        }
    }
    Ok((moved != 0).then_some((moved, targets)))
}
pub(super) fn run() -> bool {
    if percpu::r4os_rust_percpu_control(14, 0, 0, 0) != 15 {
        helpers::puts(b"TASKREG08512 occupied unavailable: four-CPU fixture\r\n");
        return true;
    }
    if PEERS.iter().any(|p| !p.load(Ordering::Acquire).is_null()) || !prepare() {
        return false;
    }
    BUSY.store(0, Ordering::Relaxed);
    PEER_DONE.store(0, Ordering::Relaxed);
    RELEASE.store(false, Ordering::Release);
    PREFLIGHT.store(0, Ordering::Relaxed);
    for (i, role) in [
        Role::Input,
        Role::ShortCompletion,
        Role::Batch,
        Role::Interactive,
    ]
    .into_iter()
    .enumerate()
    {
        let t = tasks::create_on_cpu(b"spectra-occupied-source", worker, role, 0);
        if t.is_null() {
            clean(true);
            return false;
        }
        TASKS[i].store(t, Ordering::Release);
        if !tasks::start(t) {
            clean(true);
            return false;
        }
    }
    let end = timer::tick_count().saturating_add(2000);
    while !enrolled(COUNT) {
        if kernel_check::cancelled() || timer::tick_count() >= end {
            clean(true);
            return false;
        }
        tasks::sleep(1, b"spectra-occupied-enroll");
    }
    for (i, slot) in PEERS.iter().enumerate() {
        let t = tasks::create_on_cpu(
            b"spectra-occupied-peer",
            peer,
            Role::Batch,
            (i % 3 + 1) as u32,
        );
        if t.is_null() {
            clean(true);
            return false;
        }
        slot.store(t, Ordering::Release);
    }
    loop {
        match start_busy() {
            Ok(true) => break,
            Err(()) => {
                clean(true);
                return false;
            }
            Ok(false) => {}
        }
        if kernel_check::cancelled() || timer::tick_count() >= end {
            clean(true);
            return false;
        }
        wait_capacity(b"spectra-occupied-capacity");
    }
    while BUSY.load(Ordering::Acquire) != 7 {
        if kernel_check::cancelled() || timer::tick_count() >= end {
            clean(true);
            return false;
        }
        tasks::sleep(1, b"spectra-occupied-busy");
    }
    let mut preflight_retries = 0;
    let prepared = loop {
        match enqueue() {
            Ok(Some(before)) => break before,
            Err(code) => {
                helpers::puts(b"TASKREG08512 enqueue-invariant code=");
                helpers::dec(code as u64);
                helpers::puts(b"\r\n");
                clean(true);
                return helpers::fail(b"occupied-enqueue-invariant");
            }
            Ok(None) => preflight_retries += 1,
        }
        if kernel_check::cancelled() || timer::tick_count() >= end {
            helpers::puts(b"TASKREG08512 enqueue-preflight code=");
            helpers::dec(PREFLIGHT.load(Ordering::Relaxed) as u64);
            helpers::puts(b"\r\n");
            clean(true);
            return helpers::fail(b"occupied-enqueue-preflight");
        }
        // No event/queue mutation has happened on this retry path. Let real
        // preexisting BSP work park; never reset it to manufacture quiescence.
        scheduler::yield_task();
    };
    let mut polls = 0u32;
    let observed = loop {
        match observe(&prepared.facts) {
            Ok(Some(result)) => break Ok(result),
            Err(()) => break Err(b"occupied-facts".as_slice()),
            Ok(None) => {}
        }
        polls += 1;
        if kernel_check::cancelled() || timer::tick_count() >= end || polls >= 100_000 {
            break Err(b"occupied-periodic".as_slice());
        }
        for _ in 0..128 {
            core::hint::spin_loop();
        }
    };
    crate::arch::runtime::restore_local(prepared.irq_flags);
    let restored_flags = crate::arch::runtime::save_local();
    crate::arch::runtime::restore_local(restored_flags);
    if (restored_flags ^ prepared.irq_flags) & (1 << 9) != 0 {
        clean(true);
        return helpers::fail(b"occupied-if-restore");
    }
    let (moved, targets) = match observed {
        Ok(result) => result,
        Err(reason) => {
            clean(true);
            return helpers::fail(reason);
        }
    };
    RELEASE.store(true, Ordering::Release);
    while DONE.load(Ordering::Acquire) != ALL || PEER_DONE.load(Ordering::Acquire) != PEER_ALL {
        if kernel_check::cancelled() || timer::tick_count() >= end {
            clean(true);
            return false;
        }
        tasks::sleep(1, b"spectra-occupied-complete");
    }
    let valid = FIRST.iter().all(|p| p.load(Ordering::Acquire) == 0)
        && LAST[3].load(Ordering::Acquire) == 0
        && LAST[..3].iter().any(|p| p.load(Ordering::Acquire) != 0)
        && RUNS.iter().all(|p| p.load(Ordering::Acquire) == 1)
        && FAILURES.load(Ordering::Acquire) == 0;
    let cleaned = clean(false) && unsafe { helpers::queue_valid() };
    if valid && cleaned {
        helpers::puts(b"TASKREG08512 occupied PASS tasks=10 moved=");
        helpers::dec(moved as u64);
        helpers::puts(b" targets=");
        helpers::dec(targets as u64);
        helpers::puts(b" source-load=");
        helpers::dec(prepared.source_load as u64);
        helpers::puts(b" target-loads=");
        for (i, load) in prepared.target_loads.iter().enumerate() {
            if i != 0 {
                helpers::puts(b",");
            }
            helpers::dec(*load as u64);
        }
        helpers::puts(b" periodic=real origin=recorded bsp-if=restored pinned=1 fpu=7 unwind=7 age=preserved own=retired\r\n");
        helpers::puts(b"TASKREG08512 boundary PASS retries=");
        helpers::dec(preflight_retries);
        helpers::puts(b" last-code=");
        helpers::dec(PREFLIGHT.load(Ordering::Relaxed) as u64);
        helpers::puts(b"\r\n");
    }
    valid && cleaned
}
