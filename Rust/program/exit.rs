//! First-exit publication with exact caller unwind and target generation pin.
//! BO admission closes outside registry rank110, then the same slot and state
//! are revalidated before history/publication. Lease/unwind survive all peers.
use super::{completions, exit_state::state, failure, registry, registry_types::*};
use crate::{
    arch::runtime::RuntimeGuard,
    contract::ProgramProcessHandle as Handle,
    platform::timer,
    sched::{scheduler, sync_objects, task_context},
};
use core::{
    ffi::c_void,
    mem, ptr,
    sync::atomic::{AtomicBool, Ordering},
};
unsafe extern "C" {
    fn r4os_zig_program_exit_stop_admission(instance: *mut ProgramInstance);
    fn r4os_zig_program_exit_console_snapshot(
        instance: *const ProgramInstance,
        node: *mut ProgramCompletionNode,
    );
    fn r4os_zig_program_exit_console_close(console: *mut c_void);
    fn r4os_zig_program_exit_notify(operation: u8, handle: Handle);
}
struct Operation(task_context::Unwind);
impl Operation {
    fn enter() -> Option<Self> {
        let _guard = RuntimeGuard::acquire();
        let token = unsafe { task_context::enter() };
        if token.admitted() {
            Some(Self(token))
        } else {
            None
        }
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        let _guard = RuntimeGuard::acquire();
        assert!(unsafe { task_context::leave(self.0) });
    }
}
struct Lease(ProgramInstanceLease);
impl Drop for Lease {
    fn drop(&mut self) {
        unsafe { registry::unpin(&self.0) };
    }
}
/// # Safety
/// Value handle; caller holds no registry lock. Temporary exact generation
/// lease and caller unwind remain resident through all existing peer work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_exit_commit(h: Handle, code: i32, requested: u8) -> u8 {
    unsafe {
        if failure::consume(7) {
            return 2;
        }
        let Some(_operation) = Operation::enter() else {
            return 2;
        };
        if !registry::lock() {
            return 2;
        }
        let slot = registry::lookup_handle(h, true);
        if slot.is_null() {
            registry::unlock();
            return 2;
        }
        if matches!((*slot).state, 4..=7) {
            registry::unlock();
            return 1;
        }
        if !registry::running((*slot).state) {
            registry::unlock();
            return 2;
        }
        let instance = ptr::addr_of_mut!((*slot).instance);
        if (*slot).completion.is_null() || (*slot).pin_count == u32::MAX {
            registry::unlock();
            return 2;
        }
        let mut raw: ProgramInstanceLease = mem::zeroed();
        raw.slot = slot;
        raw.instance = instance;
        raw.id = h.instance_id;
        raw.generation = h.generation;
        (*slot).pin_count += 1;
        registry::unlock();
        let _lease = Lease(raw);
        r4os_zig_program_exit_stop_admission(instance);
        if !registry::lock() {
            return 2;
        }
        let current = registry::lookup_handle(h, true);
        if current.is_null() {
            registry::unlock();
            return 2;
        }
        if current != slot || !registry::running((*current).state) {
            registry::unlock();
            return if current == slot { 1 } else { 2 };
        }
        let node = (*slot).completion;
        if node.is_null() {
            registry::unlock();
            return 2;
        }
        (*slot).state = 4;
        let finish = timer::tick_count();
        let reason = if requested == 0
            && AtomicBool::from_ptr(ptr::addr_of_mut!((*instance).close_requested))
                .load(Ordering::Acquire)
        {
            1
        } else {
            requested
        };
        (*instance).exit_code = code;
        AtomicBool::from_ptr(ptr::addr_of_mut!((*instance).done)).store(true, Ordering::Release);
        (*node).finish_tick = finish;
        (*node).exit_code = code;
        (*node).task_id = (*instance).task_id;
        (*node).app_class = (*instance).app_class;
        (*node).role = (*instance).role;
        (*node).exit_reason = reason;
        if (*instance).display_used {
            (*node).flags |= 4;
        }
        if !(*instance).console_payload.is_null() {
            r4os_zig_program_exit_console_snapshot(instance, node);
        }
        (*node).sequence = completions::r4os_rust_program_history_append(
            h,
            instance,
            code,
            reason,
            (*node).start_tick,
            finish,
        );
        (*slot).state = 5;
        (*slot).state = 6;
        registry::epoch();
        registry::enqueue(slot);
        let s = state();
        (*s).last_exit_code = code;
        (*s).last_display_used = (*instance).display_used;
        if (*s).foreground_present
            && (*s).foreground_id == h.instance_id
            && (*s).foreground_generation == h.generation
        {
            (*s).foreground_present = false;
            (*s).foreground_id = 0;
            (*s).foreground_generation = 0;
        }
        let boot_shell_exited = (*s).shell_present
            && (*s).shell_id == h.instance_id
            && (*s).shell_generation == h.generation;
        let console = (*instance).console_payload;
        if boot_shell_exited {
            (*s).shell_present = false;
            (*s).shell_id = 0;
            (*s).shell_generation = 0;
        }
        registry::unlock();
        if !console.is_null() {
            r4os_zig_program_exit_console_close(console);
        }
        if boot_shell_exited {
            r4os_zig_program_exit_notify(0, h);
        }
        completions::orphan(h);
        r4os_zig_program_exit_notify(1, h);
        r4os_zig_program_exit_notify(2, h);
        r4os_zig_program_exit_notify(3, h);
        r4os_zig_program_exit_notify(4, h);
        0
    }
}
/// # Safety
/// Same exit and caller lifetime as commit. Descendant closure retains its
/// existing iterative console-owner implementation and never nests begin.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_exit_begin(h: Handle, code: i32, reason: u8) -> bool {
    if unsafe { r4os_rust_program_exit_commit(h, code, reason) } == 2 {
        return false;
    }
    unsafe {
        r4os_zig_program_exit_notify(5, h);
        sync_objects::r4os_rust_sync_event((*super::registry_state::state()).event, 0, 0)
    };
    true
}
/// # Safety
/// Caller owns its original natural-exit epilogue. No registry/lease/unwind
/// spans retry yield; each attempt reacquires the exact original admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_exit_mark(h: Handle, code: i32) {
    while !unsafe { r4os_rust_program_exit_begin(h, code, 0) } {
        scheduler::yield_task();
    }
}
