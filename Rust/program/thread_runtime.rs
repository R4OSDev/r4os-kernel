//! Original thread construction/current/status/exit and entry-address admission.
//! Task creation and identity operations call the existing native scheduler owner.
use super::{
    completions, exit, process, registry, registry_state,
    registry_types::ProgramInstance,
    stack,
    threads::{self, Thread},
    vm,
};
use crate::{
    arch::runtime,
    contract::{
        ProgramJoinHandle as JoinHandle, ProgramProcessHandle as Handle, ProgramThreadInfo as Info,
    },
    loader::registry_control as modules,
    platform::timer,
    sched::{
        scheduler, sync_objects,
        task::{task_api, task_registry},
    },
};
use core::{
    mem::MaybeUninit,
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};
type Entry = unsafe extern "C" fn();

pub(super) unsafe fn imported(instance: *const ProgramInstance, address: u64) -> bool {
    unsafe {
        let runtime = (*instance)
            .runtime_payload
            .cast::<super::payload_types::Runtime>();
        if runtime.is_null() {
            return false;
        }
        let count = usize::from((*runtime).r4l_code_binding_count).min(16);
        let bindings =
            ptr::addr_of!((*runtime).r4l_code_bindings).cast::<super::payload_types::CodeBinding>();
        for index in 0..count {
            let binding = bindings.add(index).read();
            if modules::r4os_rust_mod_code_owner(binding.module_slot, binding.generation, address) {
                return true;
            }
        }
        false
    }
}
pub(super) unsafe fn normalize(instance: *const ProgramInstance, entry: usize) -> Option<usize> {
    unsafe {
        let raw = entry as u64;
        let base = (*instance).program_image_base;
        let size = (*instance).program_image_size as u64;
        if base == 0 || size == 0 {
            return None;
        }
        let end = base.wrapping_add(size);
        if end >= base && raw >= base && raw < end {
            return Some(entry);
        }
        if imported(instance, raw) {
            return Some(entry);
        }
        if raw < size {
            return Some((base + raw) as usize);
        }
        for link in [0x0000_0004_0000_0000u64, 0xffff_ffff_0000_0000u64] {
            let end = link.wrapping_add(size);
            if end >= link && raw >= link && raw < end {
                return Some((base + (raw - link)) as usize);
            }
        }
        None
    }
}
struct Admission(bool);
impl Drop for Admission {
    fn drop(&mut self) {
        if !self.0 {
            threads::record_failure();
        }
    }
}
fn task_error(failure: u8) -> i32 {
    match failure {
        4 => -7,
        0 | 1 | 2 | 3 | 5 => -4,
        _ => unreachable!(),
    }
}
unsafe fn create(
    entry: usize,
    arg: u64,
    reserve: u64,
    flags: u32,
    out: *mut JoinHandle,
    task_entry: Entry,
) -> i32 {
    unsafe {
        if out.is_null() || entry == 0 {
            return -1;
        }
        out.write(JoinHandle::default());
        let mut admission = Admission(false);
        if flags & !crate::contract::thread_create_flag_compute_worker != 0 {
            return -10;
        }
        let instance = super::context::current();
        if instance.is_null()
            || AtomicBool::from_ptr(ptr::addr_of_mut!((*instance).done)).load(Ordering::Acquire)
        {
            return -2;
        }
        let compute = flags & crate::contract::thread_create_flag_compute_worker != 0
            || threads::compute_worker();
        if compute && !(*instance).parallel_workers {
            return -10;
        }
        let mut handle = Handle::default();
        if !super::context::r4os_rust_program_context_handle(&mut handle) {
            return -2;
        }
        let Some(entry) = normalize(instance, entry) else {
            return -1;
        };
        let mut reserve_len = 0;
        if !stack::r4os_rust_program_thread_stack_limit(0, instance, reserve, &mut reserve_len) {
            return -1;
        }
        let mut initial_len = 0;
        if !stack::r4os_rust_program_thread_stack_limit(1, instance, reserve_len, &mut initial_len)
        {
            return -1;
        }
        if !vm::system_limit(initial_len) {
            return -4;
        }
        let mut stack = MaybeUninit::<stack::Stack>::uninit();
        if !stack::r4os_rust_program_stack_allocate(
            (*instance).id,
            reserve_len,
            initial_len,
            (*instance).memory_profile,
            (*instance).program_stack_serial_telemetry,
            stack.as_mut_ptr(),
        ) {
            return -4;
        }
        let mut stack = stack.assume_init();
        let thread = threads::allocate();
        if thread.is_null() {
            stack::r4os_rust_program_stack_free(&mut stack);
            return -4;
        }
        let parallel = (*instance).parallel_execution || compute;
        let request = task_api::CreateRequest {
            name: b"r4x-thread".as_ptr(),
            name_len: 10,
            state: 3,
            role: 2,
            needs_fpu: true,
            smp_eligible: parallel,
            smp_r4x: parallel,
            owner_kind: 0,
            owner_context: ptr::null_mut(),
            entry: Some(task_entry),
        };
        let mut failure = 0;
        let task = task_api::r4os_rust_task_create(&request, false, &mut failure);
        if task.is_null() {
            threads::free(thread);
            stack::r4os_rust_program_stack_free(&mut stack);
            return task_error(failure);
        }
        let task_core = task_registry::core(task.cast());
        thread.write(Thread {
            used: true,
            instance_id: (*instance).id,
            instance_generation: handle.generation,
            owner_instance: instance,
            task_id: (*task_core).id,
            task_generation: (*task_core).generation,
            state: 1,
            flags: threads::JOINABLE | if compute { threads::COMPUTE_WORKER } else { 0 },
            entry,
            arg: arg as usize,
            stack,
            created_tick: timer::tick_count(),
            ..Thread::default()
        });
        if task_api::r4os_rust_task_mutate(23, task, 1, thread.addr() as u64, ptr::null(), 0) == 0 {
            task_api::r4os_rust_task_identity(
                6,
                (*task_core).id,
                (*task_core).generation,
                ptr::null_mut(),
                0,
            );
            threads::free(thread);
            stack::r4os_rust_program_stack_free(&mut stack);
            return -4;
        }
        if !threads::publish(thread) {
            task_api::r4os_rust_task_mutate(25, task, thread.addr() as u64, 0, ptr::null(), 0);
            task_api::r4os_rust_task_identity(
                6,
                (*task_core).id,
                (*task_core).generation,
                ptr::null_mut(),
                0,
            );
            threads::free(thread);
            stack::r4os_rust_program_stack_free(&mut stack);
            return -4;
        }
        out.write(JoinHandle {
            thread_id: (*thread).id,
            instance_id: (*thread).instance_id,
            thread_generation: (*thread).generation,
            instance_generation: (*thread).instance_generation,
            reserved: 0,
        });
        admission.0 = true;
        task_api::r4os_rust_task_mutate(0, task, timer::tick_count(), 0, ptr::null(), 0);
        0
    }
}
/// # Safety
/// Canonical resident output; code/task entry and current process retain their
/// original executable lifetime. Failed task/stack setup follows original cleanup.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_thread_create(
    entry: usize,
    arg: u64,
    reserve: u64,
    flags: u32,
    out: *mut JoinHandle,
    task_entry: Entry,
) -> i32 {
    unsafe { create(entry, arg, reserve, flags, out, task_entry) }
}
/// # Safety
/// Same lifetime as handle creation, with the original legacy scalar output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_thread_create_legacy(
    entry: usize,
    arg: usize,
    reserve: u64,
    flags: u32,
    out: *mut u32,
    task_entry: Entry,
) -> i32 {
    unsafe {
        if out.is_null() || entry == 0 {
            return -1;
        }
        out.write(0);
        let mut handle = JoinHandle::default();
        let code = create(entry, arg as u64, reserve, flags, &mut handle, task_entry);
        if code == 0 {
            out.write(handle.thread_id);
        }
        code
    }
}
/// # Safety
/// Exact resident program/task at original blocked-main-thread construction edge.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_thread_register_main(
    instance: *mut ProgramInstance,
    task: *mut core::ffi::c_void,
) -> *mut Thread {
    unsafe {
        let mut handle = Handle::default();
        if !registry::r4os_rust_program_registry_handle(2, instance.cast(), 0, &mut handle) {
            return ptr::null_mut();
        }
        let thread = threads::allocate();
        if thread.is_null() {
            return ptr::null_mut();
        }
        let core = task_registry::core(task.cast());
        thread.write(Thread {
            used: true,
            instance_id: (*instance).id,
            instance_generation: handle.generation,
            owner_instance: instance,
            task_id: (*core).id,
            task_generation: (*core).generation,
            state: 1,
            flags: threads::MAIN,
            entry: (*instance).entry.addr(),
            created_tick: timer::tick_count(),
            ..Thread::default()
        });
        if task_api::r4os_rust_task_mutate(23, task, 1, thread.addr() as u64, ptr::null(), 0) == 0 {
            threads::free(thread);
            return ptr::null_mut();
        }
        if !threads::publish(thread) {
            task_api::r4os_rust_task_mutate(25, task, thread.addr() as u64, 0, ptr::null(), 0);
            threads::free(thread);
            return ptr::null_mut();
        }
        thread
    }
}
pub(super) unsafe fn exit_thread(thread: *mut Thread, code: i32, reason: Option<u8>) -> ! {
    unsafe {
        threads::mark(thread, false, code);
        if let Some(reason) = reason {
            let h = Handle {
                instance_id: (*thread).instance_id,
                generation: (*thread).instance_generation,
                ..Handle::default()
            };
            while !exit::r4os_rust_program_exit_begin(h, code, reason) {
                threads::yield_task();
            }
        }
        threads::unpin_execution(thread);
        threads::exit_task()
    }
}
/// # Safety
/// Terminal exact thread. Optional reason has separate presence, including0.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_thread_exit(
    thread: *mut Thread,
    code: i32,
    has_reason: bool,
    reason: u8,
) -> ! {
    unsafe { exit_thread(thread, code, has_reason.then_some(reason)) }
}
/// # Safety
/// Terminal current-thread API; the caller obeys the original execution contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_thread_api_exit(code: i32) -> ! {
    unsafe {
        let thread = threads::current();
        if thread.is_null() {
            threads::exit_task();
        }
        exit_thread(
            thread,
            code,
            if (*thread).flags & threads::MAIN != 0 {
                Some(0)
            } else {
                None
            },
        )
    }
}
/// # Safety
/// Original process-scoped terminal entry. Validates IF/runtime/held-resource
/// state before handing its exact execution pin to terminal task retirement.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_thread_program_exit(code: i32, reason: u32) -> i32 {
    unsafe {
        if reason != 0 && reason != 3 {
            return -1;
        }
        let flags: u64;
        core::arch::asm!("pushfq; pop {}",out(reg)flags,options(preserves_flags));
        if flags & (1 << 9) == 0 || runtime::r4os_rust_runtime_control(8, 0) != 0 {
            return -7;
        }
        let task = scheduler::current();
        if task.is_null() {
            return -2;
        }
        let core = task_registry::core(task.cast());
        if (*core).preempt_disable_depth != 0
            || (*core).held_lock_count != 0
            || (*core).unwind_guard_count != 0
            || (*core).wait_handoff_guard_pending
        {
            return -7;
        }
        let thread = threads::current();
        if thread.is_null() {
            return -2;
        }
        exit_thread(thread, code, Some(reason as u8))
    }
}
/// # Safety
/// Optional exact spawn-transaction thread retained by its current task.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_thread_leave_spawn(thread: *mut Thread) {
    unsafe {
        if thread.is_null() || (*thread).spawn_transaction_depth == 0 {
            return;
        }
        (*thread).spawn_transaction_depth -= 1;
        if (*thread).spawn_transaction_depth != 0 || !(*thread).exit_deferred {
            return;
        }
        let h = Handle {
            instance_id: (*thread).instance_id,
            generation: (*thread).instance_generation,
            ..Handle::default()
        };
        completions::orphan(h);
        process::r4os_rust_program_process_clients(0, h);
        threads::unpin_execution(thread);
        sync_objects::r4os_rust_sync_event((*registry_state::state()).event, 0, 0);
        threads::exit_task();
    }
}
fn valid(handle: JoinHandle) -> bool {
    handle.thread_id != 0
        && handle.instance_id != 0
        && handle.thread_generation != 0
        && handle.instance_generation != 0
        && handle.reserved == 0
}
/// # Safety
/// Original exact current-thread lifetime and exclusive resident canonical output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_thread_current_handle(out: *mut JoinHandle) -> i32 {
    unsafe {
        if out.is_null() {
            return -1;
        }
        let t = threads::current();
        if t.is_null() {
            return -2;
        }
        out.write(JoinHandle {
            thread_id: (*t).id,
            instance_id: (*t).instance_id,
            thread_generation: (*t).generation,
            instance_generation: (*t).instance_generation,
            reserved: 0,
        });
        0
    }
}
/// # Safety
/// Original current-process lifetime and exclusive output. Explicit handle uses
/// the same resident input validation and exact generation pin as the old API.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_thread_status(
    id: u32,
    handle: *const JoinHandle,
    out: *mut Info,
    exact: bool,
) -> i32 {
    unsafe {
        if !exact {
            if out.is_null() {
                return -1;
            }
            let instance = super::context::current();
            if instance.is_null() {
                return -2;
            }
            if id == 0 {
                let t = threads::current();
                if t.is_null() || (*t).owner_instance != instance {
                    return -5;
                }
                out.write(threads::info(t));
                return 0;
            }
            let t = threads::pin(id, None, true);
            if t.is_null() {
                return -5;
            }
            let code = if (*t).owner_instance != instance {
                -5
            } else {
                out.write(threads::info(t));
                0
            };
            threads::unpin(t);
            code
        } else {
            if handle.is_null() || out.is_null() {
                return -1;
            }
            let h = handle.read();
            if !valid(h) {
                return -1;
            }
            let current = threads::current();
            if current.is_null() {
                return -2;
            }
            if (*current).instance_id != h.instance_id
                || (*current).instance_generation != h.instance_generation
            {
                return -5;
            }
            let t = threads::pin(h.thread_id, Some(h), true);
            if t.is_null() {
                return -5;
            }
            out.write(threads::info(t));
            threads::unpin(t);
            0
        }
    }
}
/// # Safety
/// Exact pinned instance and optional resident scalar output. op0 normalizes
/// an entry and1 checks its imported executable R4L generations.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_thread_entry(
    op: u8,
    instance: *const ProgramInstance,
    entry: usize,
    out: *mut usize,
) -> bool {
    unsafe {
        match op {
            0 => {
                if let Some(entry) = normalize(instance, entry) {
                    out.write(entry);
                    true
                } else {
                    false
                }
            }
            1 => imported(instance, entry as u64),
            _ => unreachable!(),
        }
    }
}
/// # Safety
/// Failure0..=5 is the original native task-construction failure byte.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_thread_failure(failure: u8, record: bool) -> i32 {
    if record {
        threads::record_failure();
    }
    task_error(failure)
}
