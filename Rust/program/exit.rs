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
        {
            let _runtime = RuntimeGuard::acquire();
            AtomicBool::from_ptr(ptr::addr_of_mut!((*instance).serial_closed))
                .store(true, Ordering::Release);
            crate::serial::stop_process(h);
        }
        program_exit_stop_admission(instance);
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
            program_exit_console_snapshot(instance, node);
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
            program_exit_console_close(console);
        }
        if boot_shell_exited {
            crate::boot::performance::fail_shell_exited(h.instance_id);
        }
        completions::orphan(h);
        program_exit_notify(1, h);
        program_exit_notify(2, h);
        program_exit_notify(3, h);
        program_exit_notify(4, h);
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
        program_exit_notify(5, h);
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

unsafe fn program_exit_console_close(console: *mut c_void) {
    unsafe {
        let p = console.cast::<crate::program::payload_types::Console>();
        crate::sched::sync_objects::queue_close_cancelled(core::ptr::addr_of_mut!((*p).input_wait));
    }
}

unsafe fn program_exit_console_snapshot(
    instance: *const ProgramInstance,
    node: *mut ProgramCompletionNode,
) {
    unsafe {
        let p = (*instance)
            .console_payload
            .cast::<crate::program::payload_types::Console>();
        if !p.is_null() {
            (*node).console_state = (*p).state;
            (*node).output_revision = (*p).revision;
            let t = (*p).transcript_payload;
            (*node).output_length = if t.is_null() { 0 } else { (*t).output_len };
        }
    }
}

unsafe fn program_exit_stop_admission(instance: *mut ProgramInstance) {
    unsafe {
        crate::gfx::virtual_runtime::stop_admission(core::ptr::addr_of_mut!(
            (*instance).gfx_virtual_closed
        ));
    }
}

unsafe fn program_exit_notify(operation: u8, handle: Handle) {
    let owner = crate::gfx::types::Owner {
        kind: 1,
        id: u64::from(handle.instance_id),
        generation: handle.generation,
    };
    unsafe {
        match operation {
            1 => crate::display::queue::stopped(owner),
            2 => crate::gfx::allocations::stopped(owner),
            3 => crate::gfx::virtual_runtime::stopped(owner),
            4 => {
                let t = crate::sched::scheduler::current()
                    .cast::<crate::sched::task::task_types::Task>();
                crate::program::thread_retire::r4os_rust_program_threads_terminate(
                    handle,
                    -9,
                    !t.is_null(),
                    if t.is_null() {
                        0
                    } else {
                        (*(*t).core.get()).id
                    },
                );
            }
            5 => crate::program::process::r4os_rust_program_process_clients(0, handle),
            _ => panic!("Invalid process exit notification"),
        }
    }
}
