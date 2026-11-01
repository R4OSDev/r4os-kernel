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
fn launch() -> Result<Option<([u32; COUNT], u32)>, ()> {
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
            crate::sched::task::balance::note_ready_publication(t);
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
    Ok(Some((chosen, percpu::current_index())))
}
// These bounded copies are diagnostic receipts, never scheduling truth.
// Raw Task fields are read only under Runtime; logging uses copies after drop.
#[derive(Clone, Copy)]
struct WakeRow {
    before: Facts,
    after: Facts,
    home: (u8, u8),
    state: (u8, u8),
    running: (u8, u8),
    linked: (bool, bool),
    affinity: (bool, bool),
    event_result: u64,
    errors: u32,
}
struct WakeAttempt {
    launch_cpu: u32,
    wake_cpu: u32,
    driver: (u32, u64),
    online: u64,
    mask: u64,
    queue_checked: bool,
    queue_valid: bool,
    stage: u8,
    rows: [Option<WakeRow>; COUNT],
}
fn wake_owned(receipt: &mut WakeAttempt) -> Result<bool, ()> {
    let _runtime = RuntimeGuard::acquire();
    if !quiet() {
        return Ok(false);
    }
    receipt.wake_cpu = percpu::current_index();
    receipt.online = percpu::r4os_rust_percpu_control(14, 0, 0, 0);
    receipt.driver = unsafe { facts(scheduler::current().cast()).identity };
    for (i, slot) in TASKS.iter().enumerate() {
        unsafe {
            let t = slot.load(Ordering::Acquire);
            let before = facts(t);
            let c = registry::core(t);
            let before_home = (*c).home_cpu;
            let before_state = (*c).state as u8;
            let before_running = (*c).running_cpu;
            let before_linked = (*c).ready_linked;
            let before_affinity = (*c).cpu_affinity_bound;
            let event_result = sync_objects::r4os_rust_sync_event(event(i), 0, 0);
            let after = facts(t);
            // Exactly the original predicates; bit numbers identify the
            // original failed boundary without relaxing any equality.
            let errors = u32::from(before.identity != after.identity)
                | (u32::from(before.role != after.role) << 1)
                | (u32::from(before.stack != after.stack) << 2)
                | (u32::from(before.fpu != after.fpu) << 3)
                | (u32::from(before.unwind != after.unwind) << 4)
                | (u32::from((*c).state != State::Ready) << 5)
                | (u32::from(!(*c).ready_linked) << 6)
                | (u32::from((*c).running_cpu != 0xff) << 7)
                | (u32::from((*c).cpu_affinity_bound) << 8);
            receipt.rows[i] = Some(WakeRow {
                before,
                after,
                home: (before_home, (*c).home_cpu),
                state: (before_state, (*c).state as u8),
                running: (before_running, (*c).running_cpu),
                linked: (before_linked, (*c).ready_linked),
                affinity: (before_affinity, (*c).cpu_affinity_bound),
                event_result: u64::from(event_result),
                errors,
            });
            if errors != 0 {
                receipt.stage = 1;
                return Err(());
            }
            receipt.mask |= 1 << (*c).home_cpu;
        }
    }
    // Copy the actual queue result even when the mask fails. This is a
    // bounded diagnostic read; it never changes admission, a CPU or a queue.
    receipt.queue_checked = true;
    receipt.queue_valid = unsafe { helpers::queue_valid() };
    if receipt.mask != receipt.online || !receipt.queue_valid {
        receipt.stage = 2;
        return Err(());
    }
    Ok(true)
}
fn value(label: &[u8], number: u64) {
    helpers::puts(label);
    helpers::dec(number);
}
fn pair(label: &[u8], before: u64, after: u64) {
    value(label, before);
    value(b"/", after);
}
fn log_wake(receipt: &WakeAttempt, failed: bool) {
    helpers::puts(b"TASKREG101 wake");
    value(b" driver=", u64::from(receipt.driver.0));
    value(b":", receipt.driver.1);
    value(b" launch_cpu=", u64::from(receipt.launch_cpu));
    value(b" wake_cpu=", u64::from(receipt.wake_cpu));
    value(b" mask=", receipt.mask);
    value(b" online=", receipt.online);
    value(b" queue_checked=", u64::from(receipt.queue_checked));
    value(b" queue_valid=", u64::from(receipt.queue_valid));
    value(b" stage=", u64::from(receipt.stage));
    helpers::puts(b"\r\n");
    if !failed {
        return;
    }
    for (i, row) in receipt.rows.iter().enumerate() {
        let Some(row) = row else { continue };
        value(b"TASKREG101 identity i=", i as u64);
        pair(
            b" id=",
            u64::from(row.before.identity.0),
            u64::from(row.after.identity.0),
        );
        pair(b" gen=", row.before.identity.1, row.after.identity.1);
        value(b" errors=", u64::from(row.errors));
        helpers::puts(b"\r\n");
        value(b"TASKREG101 ready i=", i as u64);
        pair(b" home=", u64::from(row.home.0), u64::from(row.home.1));
        pair(b" state=", u64::from(row.state.0), u64::from(row.state.1));
        pair(
            b" running=",
            u64::from(row.running.0),
            u64::from(row.running.1),
        );
        pair(
            b" linked=",
            u64::from(row.linked.0),
            u64::from(row.linked.1),
        );
        pair(
            b" affinity=",
            u64::from(row.affinity.0),
            u64::from(row.affinity.1),
        );
        value(b" event=", row.event_result);
        helpers::puts(b"\r\n");
        value(b"TASKREG101 role i=", i as u64);
        pair(
            b" role=",
            u64::from(row.before.role.0),
            u64::from(row.after.role.0),
        );
        pair(
            b" priority=",
            u64::from(row.before.role.1),
            u64::from(row.after.role.1),
        );
        helpers::puts(b"\r\n");
        value(b"TASKREG101 stack i=", i as u64);
        pair(b" rsp=", row.before.stack.0, row.after.stack.0);
        pair(b" range=", row.before.stack.1, row.after.stack.1);
        helpers::puts(b"\r\n");
        value(b"TASKREG101 fpu i=", i as u64);
        pair(
            b" pointer=",
            row.before.fpu.0 as u64,
            row.after.fpu.0 as u64,
        );
        pair(
            b" bytes=",
            u64::from(row.before.fpu.1),
            u64::from(row.after.fpu.1),
        );
        helpers::puts(b"\r\n");
        value(b"TASKREG101 unwind i=", i as u64);
        pair(
            b" pointer=",
            row.before.unwind.0 as u64,
            row.after.unwind.0 as u64,
        );
        pair(
            b" depth=",
            u64::from(row.before.unwind.1),
            u64::from(row.after.unwind.1),
        );
        helpers::puts(b"\r\n");
    }
}
#[derive(Clone, Copy)]
struct PostWake {
    done: u32,
    failures: u32,
    first: [u32; COUNT],
    last: [u32; COUNT],
    runs: [u32; COUNT],
}
impl PostWake {
    fn capture() -> Self {
        Self {
            done: DONE.load(Ordering::Acquire),
            failures: FAILURES.load(Ordering::Acquire),
            first: core::array::from_fn(|i| FIRST[i].load(Ordering::Acquire)),
            last: core::array::from_fn(|i| LAST[i].load(Ordering::Acquire)),
            runs: core::array::from_fn(|i| RUNS[i].load(Ordering::Acquire)),
        }
    }
}
fn log_post(receipt: &PostWake, online: u64, mask: Option<u64>, clean: bool) {
    value(b"TASKREG101 post done=", u64::from(receipt.done));
    value(b" failures=", u64::from(receipt.failures));
    value(b" online=", online);
    value(b" mask_known=", u64::from(mask.is_some()));
    value(b" mask=", mask.unwrap_or(0));
    value(b" clean=", u64::from(clean));
    helpers::puts(b"\r\n");
    for i in 0..COUNT {
        value(b"TASKREG101 execution i=", i as u64);
        value(b" first_cpu=", u64::from(receipt.first[i]));
        value(b" last_cpu=", u64::from(receipt.last[i]));
        value(b" runs=", u64::from(receipt.runs[i]));
        helpers::puts(b"\r\n");
    }
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
    let (chosen, launch_cpu) = loop {
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
    let mut receipt = WakeAttempt {
        launch_cpu,
        wake_cpu: 0,
        driver: (0, 0),
        online: 0,
        mask: 0,
        queue_checked: false,
        queue_valid: false,
        stage: 0,
        rows: [None; COUNT],
    };
    loop {
        match wake_owned(&mut receipt) {
            Ok(true) => break,
            Err(()) => {
                finish(true);
                log_wake(&receipt, true);
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
            let post = PostWake::capture();
            let clean = finish(true);
            log_wake(&receipt, true);
            log_post(&post, receipt.online, None, clean);
            return helpers::fail(b"placement-done");
        }
        tasks::sleep(1, b"spectra-placement-done");
    }
    let online = percpu::r4os_rust_percpu_control(14, 0, 0, 0);
    let post = PostWake::capture();
    let mask = post.last.iter().fold(0u64, |mask, &cpu| mask | (1 << cpu));
    let valid = mask == online && post.runs.iter().all(|&runs| runs == 1) && post.failures == 0;
    let clean = finish(false) && unsafe { helpers::queue_valid() };
    // Every actual worker observation and retirement is complete before any
    // successful-wake output can delay the driver and influence placement.
    log_wake(&receipt, !valid || !clean);
    log_post(&post, online, Some(mask), clean);
    if valid && clean {
        helpers::puts(b"TASKREG08511 unbound placement PASS tasks=4 initial-mask=");
        helpers::dec(online);
        helpers::puts(b" wake-mask=");
        helpers::dec(mask);
        helpers::puts(b" fixed=none fpu=4 unwind=4 own=retired\r\n");
    }
    if !valid || !clean {
        return helpers::fail(b"placement-post-wake");
    }
    true
}
