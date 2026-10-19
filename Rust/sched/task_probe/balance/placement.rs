//! Existing four-worker ledger/FPU/event case, with an initial busy barrier.
//! All four CPUs must be selected before any worker may park. Foreign queues
//! are only observed; the fixture never changes another task's binding/load.
use super::*;
fn quiet() -> bool {
    let source = percpu::current_index();
    (0..registry::MAX_CPUS as u32).all(|cpu| unsafe {
        if percpu::r4os_rust_percpu_control(13, cpu, 0, 0) == 0 {
            return true;
        }
        let current = scheduler::current_on_cpu(cpu).cast::<Task>();
        (*registry::raw()).ready[cpu as usize].count == 0
            && !current.is_null()
            && (cpu == source || (*registry::core(current)).cpu_idle)
    })
}
fn launch() -> Result<Option<[u32; COUNT]>, ()> {
    let _runtime = RuntimeGuard::acquire();
    if !quiet() {
        return Ok(None);
    }
    let mut chosen = [u32::MAX; COUNT];
    for (i, choice) in chosen.iter_mut().enumerate() {
        unsafe {
            let t = TASKS[i].load(Ordering::Acquire);
            let c = registry::core(t);
            if (*c).state != State::Blocked || (*c).switches_in != 0 {
                return Err(());
            }
            // Only these exact owned, unstarted descriptors lose their temporary
            // diagnostic pin; their real SMP admission/FPU/ledger remain intact.
            (*c).cpu_affinity_bound = false;
            (*c).home_cpu_bound = false;
            if !tasks::start(t) {
                return Err(());
            }
            *choice = u32::from((*c).home_cpu);
        }
    }
    let mask = chosen.iter().fold(0u64, |mask, &cpu| mask | (1 << cpu));
    if mask != percpu::r4os_rust_percpu_control(14, 0, 0, 0) {
        return Err(());
    }
    Ok(Some(chosen))
}
fn wake() -> Result<bool, ()> {
    let _runtime = RuntimeGuard::acquire();
    if !quiet() {
        return Ok(false);
    }
    let mut mask = 0;
    for (i, slot) in TASKS.iter().enumerate() {
        unsafe {
            let t = slot.load(Ordering::Acquire);
            let before = facts(t);
            sync_objects::r4os_rust_sync_event(event(i), 0, 0);
            let after = facts(t);
            let c = registry::core(t);
            if (
                before.identity,
                before.role,
                before.stack,
                before.fpu,
                before.unwind,
            ) != (
                after.identity,
                after.role,
                after.stack,
                after.fpu,
                after.unwind,
            ) || (*c).state != State::Ready
                || !(*c).ready_linked
                || (*c).running_cpu != 0xff
                || (*c).cpu_affinity_bound
            {
                return Err(());
            }
            mask |= 1 << (*c).home_cpu;
        }
    }
    if mask != percpu::r4os_rust_percpu_control(14, 0, 0, 0) || !unsafe { helpers::queue_valid() } {
        return Err(());
    }
    Ok(true)
}
pub(super) fn run() -> bool {
    // A bounded four-CPU fixture, not a system CPU-capacity limit. Larger
    // machines retain all other scheduler cases and the full-CPU STANDARD.
    if percpu::r4os_rust_percpu_control(15, 0, 0, 0) != COUNT as u64 {
        helpers::puts(b"TASKREG08511 unbound placement unavailable: four-CPU fixture\r\n");
        return true;
    }
    if !prepare() {
        return false;
    }
    INITIAL_BARRIER.store(true, Ordering::Release);
    let source = percpu::current_index();
    for slot in &TASKS {
        let t = tasks::create_on_cpu(b"spectra-placement", worker, Role::Batch, source);
        if t.is_null() {
            finish(true);
            return false;
        }
        slot.store(t, Ordering::Release);
    }
    let end = timer::tick_count().saturating_add(2000);
    let chosen = loop {
        match launch() {
            Ok(Some(chosen)) => break chosen,
            Err(()) => {
                finish(true);
                return helpers::fail(b"placement-launch");
            }
            Ok(None) => {}
        }
        if kernel_check::cancelled() || timer::tick_count() >= end {
            finish(true);
            return helpers::fail(b"placement-initial-idle");
        }
        wait_capacity(b"spectra-placement-capacity");
    };
    while !enrolled(COUNT) {
        if kernel_check::cancelled() || timer::tick_count() >= end {
            finish(true);
            return helpers::fail(b"placement-enrollment");
        }
        tasks::sleep(1, b"spectra-placement-enrollment");
    }
    if FIRST
        .iter()
        .zip(chosen)
        .any(|(actual, wanted)| actual.load(Ordering::Acquire) != wanted)
    {
        finish(true);
        return helpers::fail(b"placement-initial-execution");
    }
    loop {
        match wake() {
            Ok(true) => break,
            Err(()) => {
                finish(true);
                return helpers::fail(b"placement-wake");
            }
            Ok(false) => {}
        }
        if kernel_check::cancelled() || timer::tick_count() >= end {
            finish(true);
            return false;
        }
        wait_capacity(b"spectra-placement-wake-capacity");
    }
    while DONE.load(Ordering::Acquire) != ALL {
        if kernel_check::cancelled() || timer::tick_count() >= end {
            finish(true);
            return false;
        }
        tasks::sleep(1, b"spectra-placement-done");
    }
    let online = percpu::r4os_rust_percpu_control(14, 0, 0, 0);
    let mask = LAST
        .iter()
        .fold(0u64, |mask, v| mask | (1 << v.load(Ordering::Acquire)));
    let valid = mask == online
        && RUNS.iter().all(|v| v.load(Ordering::Acquire) == 1)
        && FAILURES.load(Ordering::Acquire) == 0;
    let clean = finish(false) && unsafe { helpers::queue_valid() };
    if valid && clean {
        helpers::puts(b"TASKREG08511 unbound placement PASS tasks=4 initial-mask=");
        helpers::dec(online);
        helpers::puts(b" wake-mask=");
        helpers::dec(mask);
        helpers::puts(b" fixed=none fpu=4 unwind=4 own=retired\r\n");
    }
    valid && clean
}
