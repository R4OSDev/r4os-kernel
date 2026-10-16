//! Reentrant controller admission and generation-bound device transaction pins.
//! A class owner's UnwindGuard spans BOT/retry pauses; the shared controller
//! admission is explicitly released/restored only around an admitted park.
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
unsafe extern "C" {
    fn r4os_zig_usb_xhci_incident_begin() -> u64;
    fn r4os_zig_usb_xhci_incident_resolve(generation: u64) -> bool;
    fn r4os_zig_usb_xhci_diag_line(text: *const u8, len: usize);
}
fn incident_line(text: &[u8]) {
    let token = unsafe { r4os_zig_usb_xhci_incident_begin() };
    unsafe {
        r4os_zig_usb_xhci_diag_line(text.as_ptr(), text.len());
        r4os_zig_usb_xhci_incident_resolve(token)
    };
}
fn ownership() -> *mut crate::sched::sync_types::UnwindGuard {
    unsafe { ptr::addr_of_mut!((*state()).controller_ownership) }
}
pub(super) fn acquire() -> bool {
    if unsafe { sync_locks::guard_enter(ownership(), CONTROLLER_OWNERSHIP_TIMEOUT_TICKS) } {
        return true;
    }
    incident_line(b"[XHCI] controller ownership timeout");
    foundation::puts(b"[XHCI] controller ownership timeout\r\n");
    false
}
pub(super) fn release() {
    if !unsafe { sync_locks::guard_leave(ownership()) } {
        incident_line(b"[XHCI] controller ownership release mismatch");
        foundation::puts(b"[XHCI] controller ownership release mismatch\r\n");
    }
}
fn task_identity() -> (u32, u64, u32) {
    let _runtime = RuntimeGuard::acquire();
    let task = scheduler::current();
    if task.is_null() {
        return (0, 0, 0);
    }
    let core = unsafe { task_registry::core(task.cast()) };
    unsafe { ((*core).id, (*core).generation, (*core).unwind_guard_count) }
}
pub(super) fn transaction_owned(runtime: DeviceRuntime) -> bool {
    if !runtime.transaction_active {
        return false;
    }
    let (id, generation, _) = task_identity();
    runtime.transaction_task_id == id && runtime.transaction_task_generation == generation
}
pub(super) unsafe fn begin_transaction(handle: *mut DeviceHandle) -> Option<DeviceTransaction> {
    if !scheduler::current().is_null() {
        let controller_guard = u32::from(unsafe { sync_locks::guard_owned(ownership()) });
        let (_, _, count) = task_identity();
        if count <= controller_guard {
            return None;
        }
    }
    if !acquire() {
        return None;
    }
    let result = unsafe {
        if !super::xhci::select_device_handle(handle)
            || !super::xhci::device_handle_current(*handle)
        {
            None
        } else if let Some(index) = (*state()).active_runtime_index {
            let rt = ptr::addr_of_mut!((*state()).runtimes[index]);
            if (*rt).transaction_active || (*rt).detach_pending {
                None
            } else {
                let (id, generation, _) = task_identity();
                (*rt).transaction_active = true;
                (*rt).transaction_task_id = id;
                (*rt).transaction_task_generation = generation;
                Some(DeviceTransaction {
                    runtime_index: index,
                    generation: (*rt).generation,
                })
            }
        } else {
            None
        }
    };
    release();
    result
}
pub(super) unsafe fn end_transaction(transaction: DeviceTransaction) {
    restore(1);
    unsafe {
        let rt = ptr::addr_of_mut!((*state()).runtimes[transaction.runtime_index]);
        assert!(
            (*rt).active
                && (*rt).generation == transaction.generation
                && transaction_owned(rt.read()),
            "xHCI device transaction ownership mismatch"
        );
        (*rt).transaction_active = false;
        (*rt).transaction_task_id = 0;
        (*rt).transaction_task_generation = 0;
        if (*rt).detach_pending {
            (*state()).pending_port_changes.retry((*rt).port);
        }
    }
    release();
}
pub(super) unsafe fn selected_allows_mutation() -> bool {
    let Some(index) = (unsafe { (*state()).active_runtime_index }) else {
        return true;
    };
    let rt = unsafe { ptr::addr_of!((*state()).runtimes[index]).read() };
    !rt.detach_pending && (!rt.transaction_active || transaction_owned(rt))
}
pub(super) fn restore(depth: u32) {
    assert!(
        unsafe { sync_locks::guard_enter(ownership(), u64::MAX) },
        "xHCI retained transaction could not restore controller ownership"
    );
    let mut restored = 1;
    while restored < depth {
        assert!(
            unsafe { sync_locks::guard_try(ownership()) },
            "xHCI controller nesting mismatch"
        );
        restored += 1;
    }
}
pub(super) unsafe fn idle_device_wait(wait: *mut Wait) {
    let Some(index) = (unsafe { (*state()).active_runtime_index }) else {
        unsafe { (&mut *wait).idle() };
        return;
    };
    if scheduler::current().is_null()
        || !transaction_owned(unsafe { ptr::addr_of!((*state()).runtimes[index]).read() })
    {
        unsafe { (&mut *wait).idle() };
        return;
    }
    if !unsafe { (*wait).may_block_on_idle() } {
        unsafe { (&mut *wait).poll_once() };
        return;
    }
    let generation = unsafe { (*state()).runtimes[index].generation };
    let depth = unsafe { (*ownership()).depth };
    assert!(
        depth != 0 && unsafe { sync_locks::guard_owned(ownership()) },
        "xHCI wait without controller ownership"
    );
    unsafe { super::xhci::persist_active_runtime() };
    let mut released = 0;
    while released < depth {
        release();
        released += 1;
    }
    unsafe { (&mut *wait).idle() };
    restore(depth);
    assert!(
        unsafe {
            (*state()).runtimes[index].active
                && (*state()).runtimes[index].generation == generation
                && super::xhci::activate_runtime(index)
        },
        "xHCI pinned runtime changed during wait"
    );
}
