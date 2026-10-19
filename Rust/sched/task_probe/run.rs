//! Original complete diagnostic sequence, failure limits and release proofs.
use super::{
    deadlines,
    helpers::*,
    owner_snapshots::{self, Baseline},
    resources, roles,
    state::*,
    workers,
};
use crate::sched::{
    sync_objects,
    task::task_types::{State, Task},
};
use core::ptr;
unsafe fn projection(label: &[u8]) -> bool {
    unsafe {
        let q = queue_snapshot();
        if q.valid {
            return true;
        }
        puts(b"TASKREG06931 queue-invalid phase=");
        puts(label);
        for (text, n) in [
            (b" registry=".as_slice(), q.registry),
            (b" ready=".as_slice(), q.ready),
            (b" timed=".as_slice(), q.timed),
            (b" reap=".as_slice(), q.reap),
        ] {
            puts(text);
            dec(n as u64);
        }
        puts(b"\r\n");
        false
    }
}
unsafe fn baseline_delta(expected: Baseline) {
    unsafe {
        let actual = owner_snapshots::baseline();
        puts(b"TASKREG05910 baseline expected/current");
        for (label, a, b) in [
            (b" task=".as_slice(), expected.tasks, actual.tasks),
            (
                b" heap=".as_slice(),
                expected.heap_used as u64,
                actual.heap_used as u64,
            ),
            (
                b" blocks=".as_slice(),
                expected.heap_blocks as u64,
                actual.heap_blocks as u64,
            ),
            (b" pmm=".as_slice(), expected.pmm_free, actual.pmm_free),
            (b" vm=".as_slice(), expected.ranges, actual.ranges),
            (b" reserved=".as_slice(), expected.reserved, actual.reserved),
            (
                b" committed=".as_slice(),
                expected.committed,
                actual.committed,
            ),
            (b" resident=".as_slice(), expected.resident, actual.resident),
            (b" fpu=".as_slice(), expected.fpu, actual.fpu),
        ] {
            puts(label);
            dec(a);
            puts(b"/");
            dec(b);
        }
        puts(b"\r\n");
    }
}
pub(super) unsafe fn prepare() -> bool {
    unsafe {
        if !resources::work_fpu() {
            return fail(b"fpu-work");
        }
        reset();
        drain_resources();
        phase(b"warm-begin");
        if !workers::create_workers() {
            workers::cleanup();
            return fail(b"warm-create");
        }
        workers::cleanup();
        reset();
        if !projection(b"warm") {
            return fail(b"queue-warm");
        }
        true
    }
}
pub unsafe fn run() -> bool {
    unsafe {
        if !prepare() {
            return false;
        }
        let deadline_ok = deadlines::probe();
        if !deadlines::storm() {
            return fail(b"deadline-storm");
        }
        if !roles::owner_roles() {
            return fail(b"role-owners");
        }
        if !roles::directed() {
            return fail(b"role-directed-wake");
        }
        if !roles::fairness() {
            return fail(b"role-budget-fairness");
        }
        if !roles::inversion() {
            return fail(b"role-inversion");
        }
        if !roles::wake_probe() {
            return fail(b"wakeup-probe");
        }
        if !roles::latency_distributions() {
            return fail(b"role-latency-distribution");
        }
        drain_resources();
        if !projection(b"wake-reaped") {
            return fail(b"queue-wake-reaped");
        }
        phase(b"begin");
        let baseline = owner_snapshots::baseline();
        if !workers::create_workers() {
            workers::cleanup();
            return fail(b"concurrency-create");
        }
        phase(b"created");
        if r4os_rust_task_control(1, 0, 0) != baseline.tasks + WORKERS as u64 || !workers::stable()
        {
            workers::cleanup();
            return fail(b"concurrency-publish");
        }
        if !projection(b"created") {
            workers::cleanup();
            return fail(b"queue-created");
        }
        let before = reap_visits();
        r4os_rust_task_control(16, 0, 0);
        if reap_visits() != before {
            workers::cleanup();
            return fail(b"reaper-empty-scan");
        }
        if !workers::wait_started() {
            workers::cleanup();
            return fail(b"concurrency-start");
        }
        phase(b"started");
        if !workers::wait_kind(Kind::Cancel, b"taskreg-cancel")
            || !workers::wait_kind(Kind::Kill, b"taskreg-kill")
        {
            workers::cleanup();
            return fail(b"wait-enrollment");
        }
        phase(b"enrolled");
        let enrolled = queue_snapshot();
        if !enrolled.valid || enrolled.registry < WORKERS || enrolled.ready >= enrolled.registry {
            workers::cleanup();
            return fail(b"queue-enrolled");
        }
        sync_objects::r4os_rust_sync_event(ptr::addr_of_mut!((*raw()).signal_event), 0, 0);
        if queue(ptr::addr_of_mut!((*raw()).cancel_queue), 3) != (CANCEL_END - SIGNAL_END) as u32 {
            workers::cleanup();
            return fail(b"cancel-count");
        }
        for i in TIMEOUT_END..WORKERS {
            if !resources::kill_id((*raw()).ids[i]) {
                workers::cleanup();
                return fail(b"kill-detach");
            }
        }
        if !workers::wait_results() {
            workers::cleanup();
            return fail(b"wait-results");
        }
        phase(b"wait-results");
        if !projection(b"wait-results") {
            workers::cleanup();
            return fail(b"queue-wait-results");
        }
        let mix = workers::mix();
        if !mix.stable
            || mix.live < 128
            || mix.runnable == 0
            || mix.blocked == 0
            || mix.priorities != 7
            || mix.roles != 15
            || (*raw()).failures != 0
        {
            workers::cleanup();
            return fail(b"concurrency-state");
        }
        puts(b"TASKREG05910 concurrency live=");
        dec(mix.live);
        puts(b" runnable=");
        dec(mix.runnable);
        puts(b" blocked=");
        dec(mix.blocked);
        puts(b" priorities=3 roles=4 wait=OK stable=OK\r\n");
        (*raw()).release_workers = true;
        queue(ptr::addr_of_mut!((*raw()).finish_queue), 1);
        if !workers::wait_dead() || !workers::release_workers() {
            workers::cleanup();
            return fail(b"concurrency-reap");
        }
        drain_resources();
        if !owner_snapshots::matches(baseline) {
            baseline_delta(baseline);
            return fail(b"concurrency-baseline");
        }
        phase(b"reaped");
        if !projection(b"reaped") {
            return fail(b"queue-reaped");
        }
        phase(b"churn-begin");
        if !workers::churn(baseline) {
            return fail(b"churn");
        }
        drain_resources();
        if (*raw()).churn_completed != CHURN as u32 || !owner_snapshots::matches(baseline) {
            baseline_delta(baseline);
            return fail(b"churn-baseline");
        }
        phase(b"churn-complete");
        if !projection(b"churn") {
            return fail(b"queue-churn");
        }
        puts(b"TASKREG05910 churn=10000 task=baseline heap=baseline pmm=baseline vm=baseline fpu=baseline\r\n");
        let caller = current();
        if caller.is_null() {
            return fail(b"caller-missing");
        }
        let caller_view = view(caller);
        let net = owner_snapshots::network();
        if !net.started || net.id == 0 {
            return fail(b"net-rx-missing");
        }
        let rx = identity(0, net.id, net.generation) as *mut Task;
        if rx.is_null() {
            return fail(b"net-rx-identity");
        }
        let result = finish_reserve(baseline, caller, caller_view, net, rx, enrolled);
        unpin(rx);
        if !result {
            return false;
        }
        if !deadline_ok {
            return fail(b"deadline-probe");
        }
        puts(b"TASKREG05910 result: OK\r\n");
        true
    }
}
// Explicit control keeps the pin until all reserve/network/caller checks end.
unsafe fn finish_reserve(
    baseline: Baseline,
    caller: *mut Task,
    caller_before: View,
    net: owner_snapshots::Network,
    rx: *mut Task,
    enrolled: QueueView,
) -> bool {
    unsafe {
        let rx_before = view(rx);
        if !resources::reserve_probe() {
            return fail(b"oom-reserve");
        }
        drain_resources();
        if !owner_snapshots::matches(baseline) {
            baseline_delta(baseline);
            return fail(b"oom-baseline");
        }
        let mut net_after = owner_snapshots::network();
        let mut v = view(rx);
        let mut progress = v.generation == rx_before.generation
            && (net_after.iterations > net.iterations
                || v.run_ticks > rx_before.run_ticks
                || (v.switches > rx_before.switches && net_after.polls > net.polls));
        let mut spins = 0;
        while !progress && spins < NETWORK_PROGRESS {
            spins += 1;
            sleep(1, b"taskreg-net-progress");
            net_after = owner_snapshots::network();
            v = view(rx);
            progress = v.generation == rx_before.generation
                && (net_after.iterations > net.iterations
                    || v.run_ticks > rx_before.run_ticks
                    || (v.switches > rx_before.switches && net_after.polls > net.polls));
        }
        puts(b"TASKREG05910 network");
        for (label, a, b) in [
            (b" run_ticks=".as_slice(), rx_before.run_ticks, v.run_ticks),
            (b" switches=".as_slice(), rx_before.switches, v.switches),
            (b" polls=".as_slice(), net.polls, net_after.polls),
            (
                b" iterations=".as_slice(),
                net.iterations,
                net_after.iterations,
            ),
        ] {
            puts(label);
            dec(a);
            puts(b"->");
            dec(b);
        }
        puts(b"\r\n");
        let actual = current();
        if actual.is_null() {
            return fail(b"caller-lost");
        }
        let actual_view = view(actual);
        let caller_alive = actual == caller
            && actual_view.id == caller_before.id
            && actual_view.generation == caller_before.generation
            && actual_view.state == State::Running;
        if !progress || !caller_alive {
            return fail(b"critical-progress");
        }
        puts(b"TASKREG05910 admission fault=task_metadata normal=REJECTED critical=OK reserve=returned netrx=progress caller=alive recovery=OK\r\n");
        puts(b"TASKREG06931 queues=OK registry=");
        dec(enrolled.registry as u64);
        puts(b" ready=");
        dec(enrolled.ready as u64);
        puts(b" timed=");
        dec(enrolled.timed as u64);
        puts(b" reaper_empty_visits=0 wake_latency=");
        dec((*raw()).wake_latency);
        puts(b" wake_bar=");
        dec(WAKE_BAR);
        puts(b" wake_request=OK lock_handoff=OK\r\n");
        puts(b"TASKREG06932 deadline_queue=ordered backend=");
        puts(match crate::platform::timer::active_backend() {
            0 => b"PIT",
            1 => b"HPET",
            2 => b"LAPIC",
            _ => panic!("Invalid probe timer backend"),
        });
        puts(b" idle_irqs=");
        dec((*raw()).deadline_irqs);
        puts(b" total_irqs=");
        dec((*raw()).deadline_total_irqs);
        puts(b" irq_bar=");
        dec(IRQ_BAR);
        puts(b" elapsed=");
        dec((*raw()).deadline_elapsed);
        puts(b" requested=");
        dec(EARLY_TICKS);
        puts(b" lateness=");
        dec((*raw()).deadline_lateness);
        puts(b" earlier=OK cancel=OK long=");
        puts(if (*raw()).deadline_checkpointed {
            b"checkpointed"
        } else {
            b"FAILED"
        });
        puts(b" storm=");
        dec(STORM as u64);
        puts(b" batches=");
        dec(u64::from((*raw()).storm_batches));
        puts(b" periodic=restored fallback=0\r\n");
        let roles = role_stats();
        let waits = wait_summary();
        let sched = structure();
        puts(b"TASKREG06933 roles=input+completion+interactive+batch owners=OK directed=");
        dec(waits.directed_wakes);
        puts(b" bypasses=");
        dec(waits.directed_fifo_bypasses);
        puts(b" budget_exhaust=");
        dec(roles.budget_exhaustions[0]);
        puts(b" donation=");
        dec(roles.donations_applied);
        puts(b" safe_switch=");
        dec(sched.safe_reschedule_switches);
        puts(b" latency_buckets=OK fairness=normal+low timeout=OK lock_owner=OK\r\n");
        true
    }
}
#[repr(C)]
struct Reap {
    passes: u64,
    visits: u64,
    pending: usize,
}
fn reap_visits() -> u64 {
    unsafe {
        unsafe extern "C" {
            fn r4os_rust_task_reap_stats(out: *mut Reap);
        }
        let mut out = core::mem::MaybeUninit::uninit();
        r4os_rust_task_reap_stats(out.as_mut_ptr());
        out.assume_init().visits
    }
}
