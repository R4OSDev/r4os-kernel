//! Autonomous port worker and IRQ wakeups. Shared producers use atomics;
//! the ISR samples only its admitted immutable MMIO binding, never Status.
use super::xhci_state::Context;
use super::{
    xhci::*,
    xhci_constants::*,
    xhci_diag as diag, xhci_owner as owner,
    xhci_state::{TopologyHook, current, state},
    xhci_types::Status,
};
use crate::{
    arch::runtime::RuntimeGuard,
    boot::foundation::puts,
    driver::irq_router,
    sched::sync_types::Text,
    sched::{
        scheduler, sync_objects,
        task::{task_api, task_registry},
    },
};
use core::{
    ptr,
    sync::atomic::{AtomicBool, AtomicU16, AtomicU32, AtomicU64, AtomicUsize, Ordering},
};

pub(super) struct AsyncState {
    pub irq_registered: AtomicBool,
    pub irq_op: AtomicU64,
    pub irq_runtime: AtomicU64,
    pub irq_count: AtomicU64,
    pub irq_handled: AtomicU64,
    pub irq_wakeups: AtomicU64,
    pub poll_fallbacks: AtomicU64,
    pub service_failures: AtomicU64,
    pub task_started: AtomicBool,
    pub task_stop: AtomicBool,
    pub task_id: AtomicU32,
    pub task_iterations: AtomicU64,
    pub topology_hook: AtomicUsize,
    pub completion_waiting: AtomicU16,
    pub completion_fast: AtomicU64,
    pub completion_enrolled: AtomicU64,
    pub completion_recheck: AtomicU64,
    pub completion_signaled: AtomicU64,
    pub completion_fallback: AtomicU64,
    pub completion_parks: AtomicU64,
}
impl AsyncState {
    pub(super) const fn empty() -> Self {
        Self {
            irq_registered: AtomicBool::new(false),
            irq_op: AtomicU64::new(0),
            irq_runtime: AtomicU64::new(0),
            irq_count: AtomicU64::new(0),
            irq_handled: AtomicU64::new(0),
            irq_wakeups: AtomicU64::new(0),
            poll_fallbacks: AtomicU64::new(0),
            service_failures: AtomicU64::new(0),
            task_started: AtomicBool::new(false),
            task_stop: AtomicBool::new(false),
            task_id: AtomicU32::new(0),
            task_iterations: AtomicU64::new(0),
            topology_hook: AtomicUsize::new(0),
            completion_waiting: AtomicU16::new(0),
            completion_fast: AtomicU64::new(0),
            completion_enrolled: AtomicU64::new(0),
            completion_recheck: AtomicU64::new(0),
            completion_signaled: AtomicU64::new(0),
            completion_fallback: AtomicU64::new(0),
            completion_parks: AtomicU64::new(0),
        }
    }
}
pub(super) fn irq_registered(ctx: Context) -> bool {
    ctx.asynchronous().irq_registered.load(Ordering::Acquire)
}
pub(super) fn port_task_started(ctx: Context) -> bool {
    ctx.asynchronous().task_started.load(Ordering::Acquire)
}
pub(super) fn request_port_task_stop(ctx: Context) {
    ctx.asynchronous().task_stop.store(true, Ordering::Release);
}
pub(super) fn resume_port_task(ctx: Context) {
    ctx.asynchronous().task_stop.store(false, Ordering::Release);
}
pub(super) fn set_topology_hook(ctx: Context, hook: Option<TopologyHook>) {
    ctx.asynchronous()
        .topology_hook
        .store(hook.map_or(0, |f| f as usize), Ordering::Release);
}
pub(super) fn record_irq_wakeup(ctx: Context) {
    ctx.asynchronous()
        .irq_wakeups
        .fetch_add(1, Ordering::Relaxed);
}
pub(super) fn record_poll_fallback(ctx: Context) {
    ctx.asynchronous()
        .poll_fallbacks
        .fetch_add(1, Ordering::Relaxed);
}
pub(super) fn record_port_service_failure(ctx: Context) {
    ctx.asynchronous()
        .service_failures
        .fetch_add(1, Ordering::Relaxed);
}
pub(super) fn wait_event_irq(ctx: Context, ticks: u64) -> bool {
    unsafe {
        sync_objects::r4os_rust_sync_event(ptr::addr_of_mut!((*state(ctx)).event_irq), 2, ticks)
            == 1
    }
}
pub(super) unsafe fn reset_after_teardown(ctx: Context) {
    // Preserve enrolled waiter links in the permanently resident event. The
    // reset operation clears only its signal, as opposed to replacing a queue
    // under a still-resident port worker during reprobe.
    unsafe {
        sync_objects::r4os_rust_sync_event(ptr::addr_of_mut!((*state(ctx)).event_irq), 1, 0);
    }
    assert_eq!(
        ctx.asynchronous()
            .completion_waiting
            .load(Ordering::Acquire),
        0,
        "xHCI teardown with a live completion waiter"
    );
    unsafe {
        for i in 0..9 {
            sync_objects::r4os_rust_sync_event(
                ptr::addr_of_mut!((*state(ctx)).completion_events[i]),
                1,
                0,
            );
        }
    }
    ctx.asynchronous()
        .irq_registered
        .store(false, Ordering::Release);
    for c in [
        &ctx.asynchronous().irq_count,
        &ctx.asynchronous().irq_handled,
        &ctx.asynchronous().irq_wakeups,
        &ctx.asynchronous().poll_fallbacks,
        &ctx.asynchronous().service_failures,
    ] {
        c.store(0, Ordering::Relaxed);
    }
}
pub(super) fn project_counters(ctx: Context, out: &mut Status) {
    out.irq_registered = irq_registered(ctx);
    out.irq_count = ctx.asynchronous().irq_count.load(Ordering::Relaxed);
    out.irq_handled = ctx.asynchronous().irq_handled.load(Ordering::Relaxed);
    out.irq_wakeups = ctx.asynchronous().irq_wakeups.load(Ordering::Relaxed);
    out.poll_fallbacks = ctx.asynchronous().poll_fallbacks.load(Ordering::Relaxed);
    out.port_service_failures = ctx.asynchronous().service_failures.load(Ordering::Relaxed);
    out.port_task_started = port_task_started(ctx);
    out.port_task_id = ctx.asynchronous().task_id.load(Ordering::Acquire);
    out.port_task_iterations = ctx.asynchronous().task_iterations.load(Ordering::Relaxed);
}
pub(super) unsafe fn start_port_task(ctx: Context) -> bool {
    unsafe {
        if port_task_started(ctx)
            || !(*current(ctx)).present
            || !(*current(ctx)).controller_running
            || (*state(ctx)).event_ring_virt == 0
        {
            return true;
        }
        start_event_irq(ctx);
        let request = task_api::CreateRequest {
            name: b"xhci-port".as_ptr(),
            name_len: 9,
            state: 1,
            role: 1,
            needs_fpu: true,
            smp_eligible: false,
            smp_r4x: false,
            owner_kind: 0,
            owner_context: ctx.raw(),
            entry: Some(port_task_main),
        };
        let mut failure = 0;
        let task = task_api::r4os_rust_task_create(&request, false, &mut failure);
        if task.is_null() {
            puts(b"[XHCI] port-task create failed\r\n");
            return false;
        }
        // The newborn worker cannot enter the controller until its creator
        // releases admission, so publication precedes its terminal check.
        let id = (*task_registry::core(task.cast())).id;
        ctx.asynchronous().task_id.store(id, Ordering::Release);
        ctx.asynchronous()
            .task_started
            .store(true, Ordering::Release);
        puts(b"[XHCI] port-task started id=");
        diag::console_dec(id);
        puts(b"\r\n");
        true
    }
}
unsafe extern "C" fn port_task_main() {
    let task = scheduler::current();
    let ctx =
        unsafe { Context::from_raw((*task_registry::core(task.cast())).execution_owner_context) };
    loop {
        if ctx.asynchronous().task_stop.load(Ordering::Acquire) {
            // Recheck under controller admission: reprobe may resume this exact
            // resident worker while it is reaching its stop boundary.
            owner::restore(ctx, 1);
            if ctx.asynchronous().task_stop.load(Ordering::Acquire) {
                ctx.asynchronous()
                    .task_started
                    .store(false, Ordering::Release);
                ctx.asynchronous().task_id.store(0, Ordering::Release);
                owner::release(ctx);
                return;
            }
            owner::release(ctx);
        }
        if irq_registered(ctx) {
            if wait_event_irq(ctx, PORT_SERVICE_IDLE_TICKS) {
                record_irq_wakeup(ctx);
            } else {
                record_poll_fallback(ctx);
            }
        } else {
            unsafe {
                scheduler::sleep(PORT_SERVICE_IDLE_TICKS, b"xhci-port-idle".as_ptr(), 14);
            }
            record_poll_fallback(ctx);
        }
        unsafe {
            service_port_changes(ctx);
        }
        ctx.asynchronous()
            .task_iterations
            .fetch_add(1, Ordering::Relaxed);
    }
}
pub(super) unsafe fn service_port_changes(ctx: Context) -> bool {
    if !owner::acquire(ctx) {
        record_port_service_failure(ctx);
        return false;
    }
    let changed = unsafe {
        if (*current(ctx)).present
            && (*current(ctx)).controller_running
            && (*state(ctx)).event_ring_virt != 0
        {
            service_port_changes_locked(ctx)
        } else {
            false
        }
    };
    owner::release(ctx);
    if changed {
        let hook = ctx.asynchronous().topology_hook.load(Ordering::Acquire);
        if hook != 0 {
            unsafe {
                core::mem::transmute::<usize, TopologyHook>(hook)();
            }
        }
    }
    changed
}
unsafe fn service_port_changes_locked(ctx: Context) -> bool {
    unsafe {
        (*current(ctx)).port_service_calls = (*current(ctx)).port_service_calls.wrapping_add(1);
        let before = (*state(ctx)).pending_port_changes.snapshot().events;
        drain_maintenance_event_batch(ctx);
        let mut changed = false;
        for _ in 0..MAX_PORTS {
            let Some(port) = (*state(ctx)).pending_port_changes.take_next() else {
                break;
            };
            let result = service_pending_port_change(ctx, port);
            changed |= result.catalog_changed;
            if result.retry {
                (*state(ctx)).pending_port_changes.retry(port);
                break;
            }
        }
        if read_op32(ctx, OP_USBSTS) & USBSTS_PCD != 0 {
            write_op32(ctx, OP_USBSTS, USBSTS_PCD);
            read_op32(ctx, OP_USBSTS);
        }
        if changed {
            (*current(ctx)).port_catalog_changes =
                (*current(ctx)).port_catalog_changes.wrapping_add(1);
        }
        if (*state(ctx)).pending_port_changes.snapshot().events != before || changed {
            emit_port_service_marker(ctx);
        }
        rearm_event_irq(ctx);
        changed
    }
}
pub(super) unsafe fn start_event_irq(ctx: Context) -> bool {
    unsafe {
        if irq_registered(ctx) {
            return true;
        }
        let line = (*current(ctx)).irq_line;
        if line == 255 || line == 0 || line >= 32 {
            (*current(ctx)).irq_mode = Text::literal(b"poll-fallback");
            return false;
        }
        let _runtime = RuntimeGuard::acquire();
        let result = irq_router::r4os_rust_irq_router_register(
            line,
            event_irq_handler,
            ctx.raw().addr(),
            3,
            (*state(ctx)).activated_owner_id,
        );
        if result != 0 {
            (*current(ctx)).irq_mode = Text::literal(b"poll-fallback");
            return false;
        }
        ctx.asynchronous()
            .irq_op
            .store((*current(ctx)).op_virt, Ordering::Relaxed);
        ctx.asynchronous()
            .irq_runtime
            .store((*current(ctx)).runtime_virt, Ordering::Relaxed);
        ctx.asynchronous()
            .irq_registered
            .store(true, Ordering::Release);
        (*current(ctx)).irq_registered = true;
        (*current(ctx)).irq_mode = Text::literal(b"intx-event");
        write_op32(ctx, OP_USBCMD, read_op32(ctx, OP_USBCMD) | USBCMD_INTE);
        rearm_event_irq(ctx);
    }
    puts(b"[XHCI] event IRQ active line=");
    unsafe {
        diag::console_dec((*current(ctx)).irq_line);
    }
    puts(b" with poll fallback\r\n");
    true
}
pub(super) unsafe fn stop_event_irq(ctx: Context) {
    unsafe {
        let _runtime = RuntimeGuard::acquire();
        if (*current(ctx)).mapped
            && (*current(ctx)).op_virt != 0
            && (*current(ctx)).runtime_virt != 0
        {
            write_rt32(ctx, RT_IR0_IMAN, 0);
            write_op32(ctx, OP_USBCMD, read_op32(ctx, OP_USBCMD) & !USBCMD_INTE);
        }
        if ctx
            .asynchronous()
            .irq_registered
            .swap(false, Ordering::AcqRel)
        {
            irq_router::r4os_rust_irq_router_unregister(
                (*current(ctx)).irq_line,
                event_irq_handler,
                ctx.raw().addr(),
            );
        }
        (*current(ctx)).irq_registered = false;
        (*current(ctx)).irq_mode = Text::literal(b"poll");
    }
}
pub(super) unsafe fn rearm_event_irq(ctx: Context) {
    unsafe {
        if irq_registered(ctx) && (*current(ctx)).controller_running {
            write_rt32(ctx, RT_IR0_IMAN, IMAN_IE);
            read_rt32(ctx, RT_IR0_IMAN);
        }
    }
}
unsafe extern "C" fn event_irq_handler(_: u8, context: usize) -> u32 {
    let ctx = unsafe { Context::from_raw(context as *mut core::ffi::c_void) };
    let _runtime = RuntimeGuard::acquire();
    if !irq_registered(ctx) {
        return 0;
    }
    let op = ctx.asynchronous().irq_op.load(Ordering::Relaxed);
    let rt = ctx.asynchronous().irq_runtime.load(Ordering::Relaxed);
    unsafe {
        let status = volatile_read32(op + OP_USBSTS);
        let iman = volatile_read32(rt + RT_IR0_IMAN);
        if status & (USBSTS_EINT | USBSTS_PCD) == 0 && iman & IMAN_IP == 0 {
            return 0;
        }
        ctx.asynchronous().irq_count.fetch_add(1, Ordering::Relaxed);
        volatile_write32(rt + RT_IR0_IMAN, (iman & !IMAN_IE) | IMAN_IP);
        if status & (USBSTS_EINT | USBSTS_PCD) != 0 {
            volatile_write32(op + OP_USBSTS, status & (USBSTS_EINT | USBSTS_PCD));
        }
        ctx.asynchronous()
            .irq_handled
            .fetch_add(1, Ordering::Relaxed);
        // Only currently enrolled waiters of this exact resident controller
        // are nudged. The ring/matcher, never this IRQ, grants completion.
        let mut waiting = ctx
            .asynchronous()
            .completion_waiting
            .load(Ordering::Acquire);
        while waiting != 0 {
            let i = waiting.trailing_zeros() as usize;
            waiting &= waiting - 1;
            sync_objects::r4os_rust_sync_event(
                ptr::addr_of_mut!((*state(ctx)).completion_events[i]),
                0,
                0,
            );
        }
        sync_objects::r4os_rust_sync_event(ptr::addr_of_mut!((*state(ctx)).event_irq), 0, 0);
    }
    1
}
pub(super) unsafe fn drain_maintenance_event_batch(ctx: Context) {
    unsafe {
        let mut processed = 0u16;
        for _ in 0..1024 {
            let Some(event) = poll_event(ctx) else {
                break;
            };
            processed = processed.wrapping_add(1);
            route_foreign_event(ctx, event);
        }
        commit_event_batch(ctx, processed);
    }
}
unsafe fn emit_port_service_marker(ctx: Context) {
    unsafe {
        let ports = (*state(ctx)).pending_port_changes.snapshot();
        puts(b"[XHCIPORT] events=");
        diag::console_dec(ports.events);
        puts(b" pending=");
        diag::console_dec(ports.pending);
        puts(b" queued=");
        diag::console_dec(ports.queued);
        puts(b" coalesced=");
        diag::console_dec(ports.coalesced);
        puts(b" invalid=");
        diag::console_dec(ports.invalid);
        puts(b" taken=");
        diag::console_dec(ports.taken);
        puts(b" retries=");
        diag::console_dec(ports.retries);
        puts(b" failures=");
        diag::console_dec(ctx.asynchronous().service_failures.load(Ordering::Relaxed));
        puts(b" catalog_changes=");
        diag::console_dec((*current(ctx)).port_catalog_changes);
        puts(b"\r\n");
    }
}

// An enrolled slot remains resident and is exclusive under controller
// admission. Runtime transaction pins preserve its generation across parks;
// the command lane cannot release its owner to another command.
pub(super) struct CompletionWait {
    ctx: Context,
    pub slot: usize,
}
impl CompletionWait {
    pub(super) unsafe fn enroll(ctx: Context, expected: super::event_router::Match) -> Self {
        let slot = if expected.tag == 1 {
            8
        } else {
            unsafe { (*state(ctx)).active_runtime_index }
                .expect("completion without device runtime")
        };
        unsafe {
            assert!(
                (*state(ctx)).completion_matches[slot].is_none(),
                "completion slot already enrolled"
            );
            // Clear the old signal before publication, then publish before
            // the caller rechecks the mailbox/ring. An earlier completion
            // stays on that ring; a later signal persists until enrollment.
            sync_objects::r4os_rust_sync_event(
                ptr::addr_of_mut!((*state(ctx)).completion_events[slot]),
                1,
                0,
            );
            (*state(ctx)).completion_matches[slot] = Some(expected);
        }
        ctx.asynchronous()
            .completion_waiting
            .fetch_or(1 << slot, Ordering::Release);
        ctx.asynchronous()
            .completion_enrolled
            .fetch_add(1, Ordering::Relaxed);
        Self { ctx, slot }
    }
    pub(super) fn rechecked(&self) {
        self.ctx
            .asynchronous()
            .completion_recheck
            .fetch_add(1, Ordering::Relaxed);
    }
}
impl Drop for CompletionWait {
    fn drop(&mut self) {
        self.ctx
            .asynchronous()
            .completion_waiting
            .fetch_and(!(1 << self.slot), Ordering::AcqRel);
        unsafe { (*state(self.ctx)).completion_matches[self.slot] = None };
    }
}
pub(super) unsafe fn signal_completion(ctx: Context, event: super::event_router::Event) {
    unsafe {
        for i in 0..9 {
            if (*state(ctx)).completion_matches[i]
                .is_some_and(|m| super::event_router::matches(event, m))
            {
                // Exact event is already in the deferred mailbox here.
                sync_objects::r4os_rust_sync_event(
                    ptr::addr_of_mut!((*state(ctx)).completion_events[i]),
                    0,
                    0,
                );
            }
        }
    }
}
pub(super) unsafe fn completion_idle(
    ctx: Context,
    wait: *mut super::wait_types::Wait,
    slot: usize,
) {
    unsafe {
        (*wait).iterations = (*wait).iterations.wrapping_add(1);
        if !(*wait).deadline.can_block() {
            core::hint::spin_loop();
            return;
        }
        ctx.asynchronous()
            .completion_parks
            .fetch_add(1, Ordering::Relaxed);
        // At most one tick is a progress fallback, not a renewed deadline.
        // Only a matching ring event can finish the caller's original wait.
        let result = sync_objects::r4os_rust_sync_event(
            ptr::addr_of_mut!((*state(ctx)).completion_events[slot]),
            2,
            1,
        );
        (*wait).blocked_ticks = (*wait).blocked_ticks.wrapping_add(1);
        if result == 1 {
            ctx.asynchronous()
                .completion_signaled
                .fetch_add(1, Ordering::Relaxed);
        } else {
            ctx.asynchronous()
                .completion_fallback
                .fetch_add(1, Ordering::Relaxed);
        }
    }
}
