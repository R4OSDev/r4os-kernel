//! Autonomous port worker and IRQ wakeups. Shared producers use atomics;
//! the ISR samples only its admitted immutable MMIO binding, never Status.
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
    sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering},
};
static IRQ_REGISTERED: AtomicBool = AtomicBool::new(false);
static IRQ_OP: AtomicU64 = AtomicU64::new(0);
static IRQ_RUNTIME: AtomicU64 = AtomicU64::new(0);
static IRQ_COUNT: AtomicU64 = AtomicU64::new(0);
static IRQ_HANDLED: AtomicU64 = AtomicU64::new(0);
static IRQ_WAKEUPS: AtomicU64 = AtomicU64::new(0);
static POLL_FALLBACKS: AtomicU64 = AtomicU64::new(0);
static SERVICE_FAILURES: AtomicU64 = AtomicU64::new(0);
static TASK_STARTED: AtomicBool = AtomicBool::new(false);
static TASK_STOP: AtomicBool = AtomicBool::new(false);
static TASK_ID: AtomicU32 = AtomicU32::new(0);
static TASK_ITERATIONS: AtomicU64 = AtomicU64::new(0);
static TOPOLOGY_HOOK: AtomicUsize = AtomicUsize::new(0);
pub(super) fn irq_registered() -> bool {
    IRQ_REGISTERED.load(Ordering::Acquire)
}
pub(super) fn port_task_started() -> bool {
    TASK_STARTED.load(Ordering::Acquire)
}
pub(super) fn request_port_task_stop() {
    TASK_STOP.store(true, Ordering::Release);
}
pub(super) fn resume_port_task() {
    TASK_STOP.store(false, Ordering::Release);
}
pub(super) fn set_topology_hook(hook: Option<TopologyHook>) {
    TOPOLOGY_HOOK.store(hook.map_or(0, |f| f as usize), Ordering::Release);
}
pub(super) fn record_irq_wakeup() {
    IRQ_WAKEUPS.fetch_add(1, Ordering::Relaxed);
}
pub(super) fn record_poll_fallback() {
    POLL_FALLBACKS.fetch_add(1, Ordering::Relaxed);
}
pub(super) fn record_port_service_failure() {
    SERVICE_FAILURES.fetch_add(1, Ordering::Relaxed);
}
pub(super) fn wait_event_irq(ticks: u64) -> bool {
    unsafe {
        sync_objects::r4os_rust_sync_event(ptr::addr_of_mut!((*state()).event_irq), 2, ticks) == 1
    }
}
pub(super) unsafe fn reset_after_teardown() {
    // Preserve enrolled waiter links in the permanently resident event. The
    // reset operation clears only its signal, as opposed to replacing a queue
    // under a still-resident port worker during reprobe.
    unsafe {
        sync_objects::r4os_rust_sync_event(ptr::addr_of_mut!((*state()).event_irq), 1, 0);
    }
    IRQ_REGISTERED.store(false, Ordering::Release);
    for c in [
        &IRQ_COUNT,
        &IRQ_HANDLED,
        &IRQ_WAKEUPS,
        &POLL_FALLBACKS,
        &SERVICE_FAILURES,
    ] {
        c.store(0, Ordering::Relaxed);
    }
}
pub(super) fn project_counters(out: &mut Status) {
    out.irq_registered = irq_registered();
    out.irq_count = IRQ_COUNT.load(Ordering::Relaxed);
    out.irq_handled = IRQ_HANDLED.load(Ordering::Relaxed);
    out.irq_wakeups = IRQ_WAKEUPS.load(Ordering::Relaxed);
    out.poll_fallbacks = POLL_FALLBACKS.load(Ordering::Relaxed);
    out.port_service_failures = SERVICE_FAILURES.load(Ordering::Relaxed);
    out.port_task_started = port_task_started();
    out.port_task_id = TASK_ID.load(Ordering::Acquire);
    out.port_task_iterations = TASK_ITERATIONS.load(Ordering::Relaxed);
}
pub(super) unsafe fn start_port_task() -> bool {
    unsafe {
        if port_task_started()
            || !(*current()).present
            || !(*current()).controller_running
            || (*state()).event_ring_virt == 0
        {
            return true;
        }
        start_event_irq();
        let request = task_api::CreateRequest {
            name: b"xhci-port".as_ptr(),
            name_len: 9,
            state: 1,
            role: 1,
            needs_fpu: true,
            smp_eligible: false,
            smp_r4x: false,
            owner_kind: 0,
            owner_context: ptr::null_mut(),
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
        TASK_ID.store(id, Ordering::Release);
        TASK_STARTED.store(true, Ordering::Release);
        puts(b"[XHCI] port-task started id=");
        diag::console_dec(id);
        puts(b"\r\n");
        true
    }
}
unsafe extern "C" fn port_task_main() {
    loop {
        if TASK_STOP.load(Ordering::Acquire) {
            // Recheck under controller admission: reprobe may resume this exact
            // resident worker while it is reaching its stop boundary.
            owner::restore(1);
            if TASK_STOP.load(Ordering::Acquire) {
                TASK_STARTED.store(false, Ordering::Release);
                TASK_ID.store(0, Ordering::Release);
                owner::release();
                return;
            }
            owner::release();
        }
        if irq_registered() {
            if wait_event_irq(PORT_SERVICE_IDLE_TICKS) {
                record_irq_wakeup();
            } else {
                record_poll_fallback();
            }
        } else {
            unsafe {
                scheduler::sleep(PORT_SERVICE_IDLE_TICKS, b"xhci-port-idle".as_ptr(), 14);
            }
            record_poll_fallback();
        }
        unsafe {
            service_port_changes();
        }
        TASK_ITERATIONS.fetch_add(1, Ordering::Relaxed);
    }
}
pub(super) unsafe fn service_port_changes() -> bool {
    if !owner::acquire() {
        record_port_service_failure();
        return false;
    }
    let changed = unsafe {
        if (*current()).present
            && (*current()).controller_running
            && (*state()).event_ring_virt != 0
        {
            service_port_changes_locked()
        } else {
            false
        }
    };
    owner::release();
    if changed {
        let hook = TOPOLOGY_HOOK.load(Ordering::Acquire);
        if hook != 0 {
            unsafe {
                core::mem::transmute::<usize, TopologyHook>(hook)();
            }
        }
    }
    changed
}
unsafe fn service_port_changes_locked() -> bool {
    unsafe {
        (*current()).port_service_calls = (*current()).port_service_calls.wrapping_add(1);
        let before = (*state()).pending_port_changes.snapshot().events;
        drain_maintenance_event_batch();
        let mut changed = false;
        for _ in 0..MAX_PORTS {
            let Some(port) = (*state()).pending_port_changes.take_next() else {
                break;
            };
            let result = service_pending_port_change(port);
            changed |= result.catalog_changed;
            if result.retry {
                (*state()).pending_port_changes.retry(port);
                break;
            }
        }
        if read_op32(OP_USBSTS) & USBSTS_PCD != 0 {
            write_op32(OP_USBSTS, USBSTS_PCD);
            read_op32(OP_USBSTS);
        }
        if changed {
            (*current()).port_catalog_changes = (*current()).port_catalog_changes.wrapping_add(1);
        }
        if (*state()).pending_port_changes.snapshot().events != before || changed {
            emit_port_service_marker();
        }
        rearm_event_irq();
        changed
    }
}
pub(super) unsafe fn start_event_irq() -> bool {
    unsafe {
        if irq_registered() {
            return true;
        }
        let line = (*current()).irq_line;
        if line == 255 || line == 0 || line >= 32 {
            (*current()).irq_mode = Text::literal(b"poll-fallback");
            return false;
        }
        let _runtime = RuntimeGuard::acquire();
        let result = irq_router::r4os_rust_irq_router_register(
            line,
            event_irq_handler,
            0,
            3,
            (*state()).activated_owner_id,
        );
        if result != 0 {
            (*current()).irq_mode = Text::literal(b"poll-fallback");
            return false;
        }
        IRQ_OP.store((*current()).op_virt, Ordering::Relaxed);
        IRQ_RUNTIME.store((*current()).runtime_virt, Ordering::Relaxed);
        IRQ_REGISTERED.store(true, Ordering::Release);
        (*current()).irq_registered = true;
        (*current()).irq_mode = Text::literal(b"intx-event");
        write_op32(OP_USBCMD, read_op32(OP_USBCMD) | USBCMD_INTE);
        rearm_event_irq();
    }
    puts(b"[XHCI] event IRQ active line=");
    unsafe {
        diag::console_dec((*current()).irq_line);
    }
    puts(b" with poll fallback\r\n");
    true
}
pub(super) unsafe fn stop_event_irq() {
    unsafe {
        let _runtime = RuntimeGuard::acquire();
        if (*current()).mapped && (*current()).op_virt != 0 && (*current()).runtime_virt != 0 {
            write_rt32(RT_IR0_IMAN, 0);
            write_op32(OP_USBCMD, read_op32(OP_USBCMD) & !USBCMD_INTE);
        }
        if IRQ_REGISTERED.swap(false, Ordering::AcqRel) {
            irq_router::r4os_rust_irq_router_unregister(
                (*current()).irq_line,
                event_irq_handler,
                0,
            );
        }
        (*current()).irq_registered = false;
        (*current()).irq_mode = Text::literal(b"poll");
    }
}
pub(super) unsafe fn rearm_event_irq() {
    unsafe {
        if irq_registered() && (*current()).controller_running {
            write_rt32(RT_IR0_IMAN, IMAN_IE);
            read_rt32(RT_IR0_IMAN);
        }
    }
}
unsafe extern "C" fn event_irq_handler(_: u8, _: usize) -> u32 {
    let _runtime = RuntimeGuard::acquire();
    if !irq_registered() {
        return 0;
    }
    let op = IRQ_OP.load(Ordering::Relaxed);
    let rt = IRQ_RUNTIME.load(Ordering::Relaxed);
    unsafe {
        let status = volatile_read32(op + OP_USBSTS);
        let iman = volatile_read32(rt + RT_IR0_IMAN);
        if status & (USBSTS_EINT | USBSTS_PCD) == 0 && iman & IMAN_IP == 0 {
            return 0;
        }
        IRQ_COUNT.fetch_add(1, Ordering::Relaxed);
        volatile_write32(rt + RT_IR0_IMAN, (iman & !IMAN_IE) | IMAN_IP);
        if status & (USBSTS_EINT | USBSTS_PCD) != 0 {
            volatile_write32(op + OP_USBSTS, status & (USBSTS_EINT | USBSTS_PCD));
        }
        IRQ_HANDLED.fetch_add(1, Ordering::Relaxed);
        sync_objects::r4os_rust_sync_event(ptr::addr_of_mut!((*state()).event_irq), 0, 0);
    }
    1
}
pub(super) unsafe fn drain_maintenance_event_batch() {
    unsafe {
        let mut processed = 0u16;
        for _ in 0..1024 {
            let Some(event) = poll_event() else {
                break;
            };
            processed = processed.wrapping_add(1);
            route_foreign_event(event);
        }
        commit_event_batch(processed);
    }
}
unsafe fn emit_port_service_marker() {
    unsafe {
        let ports = (*state()).pending_port_changes.snapshot();
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
        diag::console_dec(SERVICE_FAILURES.load(Ordering::Relaxed));
        puts(b" catalog_changes=");
        diag::console_dec((*current()).port_catalog_changes);
        puts(b"\r\n");
    }
}
