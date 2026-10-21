//! Reentrant controller admission and generation-bound device transaction pins.
//! A class owner's UnwindGuard spans BOT/retry pauses; the shared controller
//! admission is explicitly released/restored only around an admitted park.
use super::xhci_state::Context;
use super::{
    wait_types::Wait,
    xhci_constants::CONTROLLER_OWNERSHIP_TIMEOUT_TICKS,
    xhci_state::state,
    xhci_types::{DeviceHandle, DeviceRuntime, DeviceTransaction},
};
use crate::{
    arch::runtime::RuntimeGuard,
    boot::foundation,
    sched::{scheduler, sync_locks, task::task_registry},
};
use core::ptr;

fn incident_line(_ctx: Context, text: &[u8]) {
    let token = unsafe { usb_xhci_incident_begin() };
    unsafe {
        usb_xhci_diag_line(text.as_ptr(), text.len());
        crate::display::diag_screen::resolve(token)
    };
}
fn ownership(ctx: Context) -> *mut crate::sched::sync_types::UnwindGuard {
    unsafe { ptr::addr_of_mut!((*state(ctx)).controller_ownership) }
}
pub(super) fn acquire(ctx: Context) -> bool {
    if unsafe { sync_locks::guard_enter(ownership(ctx), CONTROLLER_OWNERSHIP_TIMEOUT_TICKS) } {
        return true;
    }
    incident_line(ctx, b"[XHCI] controller ownership timeout");
    foundation::puts(b"[XHCI] controller ownership timeout\r\n");
    false
}
pub(super) fn release(ctx: Context) {
    if !unsafe { sync_locks::guard_leave(ownership(ctx)) } {
        incident_line(ctx, b"[XHCI] controller ownership release mismatch");
        foundation::puts(b"[XHCI] controller ownership release mismatch\r\n");
    }
}
fn task_identity(_ctx: Context) -> (u32, u64, u32) {
    let _runtime = RuntimeGuard::acquire();
    let task = scheduler::current();
    if task.is_null() {
        return (0, 0, 0);
    }
    let core = unsafe { task_registry::core(task.cast()) };
    unsafe { ((*core).id, (*core).generation, (*core).unwind_guard_count) }
}
pub(super) fn transaction_owned(ctx: Context, runtime: DeviceRuntime) -> bool {
    if !runtime.transaction_active {
        return false;
    }
    let (id, generation, _) = task_identity(ctx);
    runtime.transaction_task_id == id && runtime.transaction_task_generation == generation
}
pub(super) unsafe fn begin_transaction(
    ctx: Context,
    handle: *mut DeviceHandle,
) -> Option<DeviceTransaction> {
    if !scheduler::current().is_null() {
        let controller_guard = u32::from(unsafe { sync_locks::guard_owned(ownership(ctx)) });
        let (_, _, count) = task_identity(ctx);
        if count <= controller_guard {
            return None;
        }
    }
    if !acquire(ctx) {
        return None;
    }
    let result = unsafe {
        if !super::xhci::select_device_handle(ctx, handle)
            || !super::xhci::device_handle_current(ctx, *handle)
        {
            None
        } else if let Some(index) = (*state(ctx)).active_runtime_index {
            let rt = ptr::addr_of_mut!((*state(ctx)).runtimes[index]);
            if (*rt).transaction_active || (*rt).detach_pending {
                None
            } else {
                let (id, generation, _) = task_identity(ctx);
                (*rt).transaction_active = true;
                (*rt).transaction_task_id = id;
                (*rt).transaction_task_generation = generation;
                Some(DeviceTransaction {
                    controller_id: ctx.id(),
                    runtime_index: index,
                    generation: (*rt).generation,
                })
            }
        } else {
            None
        }
    };
    release(ctx);
    result
}
pub(super) unsafe fn end_transaction(ctx: Context, transaction: DeviceTransaction) {
    restore(ctx, 1);
    assert!(
        transaction.controller_id == ctx.id(),
        "USB transaction controller epoch changed"
    );
    unsafe {
        let rt = ptr::addr_of_mut!((*state(ctx)).runtimes[transaction.runtime_index]);
        assert!(
            (*rt).active
                && (*rt).generation == transaction.generation
                && transaction_owned(ctx, rt.read()),
            "xHCI device transaction ownership mismatch"
        );
        (*rt).transaction_active = false;
        (*rt).transaction_task_id = 0;
        (*rt).transaction_task_generation = 0;
        if (*rt).detach_pending {
            (*state(ctx)).pending_port_changes.retry((*rt).port);
        }
    }
    release(ctx);
}
pub(super) unsafe fn selected_allows_mutation(ctx: Context) -> bool {
    let Some(index) = (unsafe { (*state(ctx)).active_runtime_index }) else {
        return true;
    };
    let rt = unsafe { ptr::addr_of!((*state(ctx)).runtimes[index]).read() };
    !rt.detach_pending && (!rt.transaction_active || transaction_owned(ctx, rt))
}
pub(super) fn restore(ctx: Context, depth: u32) {
    assert!(
        unsafe { sync_locks::guard_enter(ownership(ctx), u64::MAX) },
        "xHCI retained transaction could not restore controller ownership"
    );
    let mut restored = 1;
    while restored < depth {
        assert!(
            unsafe { sync_locks::guard_try(ownership(ctx)) },
            "xHCI controller nesting mismatch"
        );
        restored += 1;
    }
}
pub(super) unsafe fn idle_device_wait(ctx: Context, wait: *mut Wait, completion_slot: usize) {
    // Keep a small bounded immediate-completion poll; the former one-ms
    // spin grace is replaced only for this exact completion wait.
    if !unsafe { (*wait).deadline.can_block() } || unsafe { (*wait).iterations } < 32 {
        unsafe { (&mut *wait).poll_once() };
        return;
    }
    let Some(index) = (unsafe { (*state(ctx)).active_runtime_index }) else {
        unsafe { super::xhci_async::completion_idle(ctx, wait, completion_slot) };
        return;
    };
    if !transaction_owned(ctx, unsafe {
        ptr::addr_of!((*state(ctx)).runtimes[index]).read()
    }) {
        // Control/command state remains under its original owner. The IRQ
        // wakes this separate slot, so it need not borrow the port worker.
        unsafe { super::xhci_async::completion_idle(ctx, wait, completion_slot) };
        return;
    }
    let generation = unsafe { (*state(ctx)).runtimes[index].generation };
    let depth = unsafe { (*ownership(ctx)).depth };
    assert!(
        depth != 0 && unsafe { sync_locks::guard_owned(ownership(ctx)) },
        "xHCI wait without controller ownership"
    );
    unsafe { super::xhci::persist_active_runtime(ctx) };
    let mut released = 0;
    while released < depth {
        release(ctx);
        released += 1;
    }
    unsafe { super::xhci_async::completion_idle(ctx, wait, completion_slot) };
    restore(ctx, depth);
    assert!(
        unsafe {
            (*state(ctx)).runtimes[index].active
                && (*state(ctx)).runtimes[index].generation == generation
                && super::xhci::activate_runtime(ctx, index)
        },
        "xHCI pinned runtime changed during wait"
    );
}

unsafe fn usb_xhci_incident_begin() -> u64 {
    crate::display::diag_screen::begin_resolvable()
}

unsafe fn usb_xhci_diag_line(text: *const u8, len: usize) {
    unsafe { crate::display::diag_screen::write(text, len) };
    crate::display::diag_screen::end_line();
}
