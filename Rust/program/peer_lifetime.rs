//! Program peer cleanup and IRQ-safe fault attribution. A retry preserves
//! exact remaining stack/image/VM ownership. Native notification transport
//! retains its separate namespace and scheduler enrollment owner.
use super::{
    context, info, instance_storage, notifications, payload, payload_types as t, process,
    process_data, registry, registry_state, registry_types::*, stack, thread_runtime, threads,
    util,
};
use crate::{
    boot::{crash::ExecutionContext, foundation},
    contract,
    sched::{scheduler, task::task_registry},
};
use core::{
    ffi::c_void,
    mem::{self, MaybeUninit},
    ptr,
    sync::atomic::{AtomicI32, Ordering},
};
unsafe extern "C" {
    fn r4os_zig_program_peer_vm_release(id: u32) -> bool;
    fn r4os_zig_program_peer_backing_release(id: u32);
}
pub(super) unsafe fn notifications_cleanup(instance: *mut ProgramInstance) -> bool {
    unsafe {
        notifications::r4os_rust_notifications_cleanup(ptr::addr_of_mut!((*instance).notifications))
    }
}
pub(super) unsafe fn vm_release(id: u32) -> bool {
    unsafe { r4os_zig_program_peer_vm_release(id) }
}
pub(super) unsafe fn backing_release(id: u32) {
    unsafe { r4os_zig_program_peer_backing_release(id) }
}
pub(super) unsafe fn resource_step(instance: *mut ProgramInstance) -> bool {
    unsafe {
        let mut resources = MaybeUninit::<stack::Resources>::uninit();
        stack::r4os_rust_program_resources_instance(instance, resources.as_mut_ptr());
        let mut resources = resources.assume_init();
        if !stack::r4os_rust_program_resources_cleanup(&mut resources) {
            stack::r4os_rust_program_stack_to_instance(instance, &resources.stack);
            false
        } else {
            true
        }
    }
}
pub(super) unsafe fn cleanup(instance: *mut ProgramInstance) -> bool {
    unsafe {
        if !process_data::r4os_rust_program_data_local_cleanup(instance)
            || !notifications_cleanup(instance)
        {
            return false;
        }
        let mut resources = MaybeUninit::<stack::Resources>::uninit();
        stack::r4os_rust_program_resources_instance(instance, resources.as_mut_ptr());
        let mut resources = resources.assume_init();
        let runtime = (*instance).runtime_payload.cast::<t::Runtime>();
        let process = (*instance).process_payload.cast::<t::Process>();
        if !runtime.is_null() && !process.is_null() {
            let mut storage = t::InstanceStorage {
                runtime,
                process,
                console: (*instance).console_payload.cast(),
                gui: (*instance).gui_payload.cast(),
            };
            instance_storage::rollback((*instance).id, &mut storage);
        }
        (*instance).runtime_payload = ptr::null_mut();
        (*instance).process_payload = ptr::null_mut();
        (*instance).console_payload = ptr::null_mut();
        (*instance).gui_payload = ptr::null_mut();
        let complete = stack::r4os_rust_program_resources_cleanup(&mut resources);
        stack::r4os_rust_program_stack_to_instance(instance, &resources.stack);
        if !complete {
            return false;
        }
        (*instance).program_image_range_id = 0;
        (*instance).program_image_base = 0;
        (*instance).program_image_size = 0;
        if !vm_release((*instance).id) {
            return false;
        }
        backing_release((*instance).id);
        true
    }
}
unsafe fn notification_owner() -> *mut c_void {
    unsafe {
        let thread = threads::current();
        if thread.is_null() {
            return ptr::null_mut();
        }
        let instance = (*thread).owner_instance;
        if instance.is_null() {
            ptr::null_mut()
        } else {
            ptr::addr_of_mut!((*instance).notifications).cast()
        }
    }
}
/// # Safety
/// Exact exclusive unpublished/cancelled process; its reservation keeps the
/// stable instance through every unlocked original peer acknowledgment.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_peer_cleanup(instance: *mut ProgramInstance) -> bool {
    unsafe { cleanup(instance) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_peer_notification_owner() -> *mut c_void {
    unsafe { notification_owner() }
}
/// # Safety
/// Current ProgramThread owns its embedded notification namespace. Selected
/// output keeps the original query/create lifetime and validity contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_peer_notification(
    op: u8,
    handle: u64,
    value: u64,
    timeout: u64,
    out: *mut u64,
) -> i32 {
    unsafe {
        let owner = notification_owner();
        if owner.is_null() {
            return contract::notification_error_context;
        }
        if matches!(op, 0 | 1) && out.is_null() {
            return contract::notification_error_invalid;
        }
        notifications::r4os_rust_notifications_call(op, owner.cast(), handle, value, timeout, out)
    }
}
pub(super) unsafe fn contains(instance: *const ProgramInstance, rip: u64) -> bool {
    unsafe {
        let base = (*instance).program_image_base;
        let size = (*instance).program_image_size as u64;
        if base == 0 || size == 0 {
            return false;
        }
        let end = base.wrapping_add(size);
        end >= base && rip >= base && rip < end
    }
}
unsafe fn crash_context(instance: *const ProgramInstance, task: u32, rip: u64) -> ExecutionContext {
    unsafe {
        let base = (*instance).program_image_base;
        ExecutionContext {
            state: 1,
            r4x_instance_id: (*instance).id,
            task_id: task,
            owner_id: u64::from((*instance).id),
            image_base: base,
            image_size: (*instance).program_image_size as u64,
            instruction_offset: if contains(instance, rip) {
                rip - base
            } else {
                0
            },
            tag: (*instance).memory_tag,
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_peer_preemptible(rip: u64) -> bool {
    unsafe {
        let instance = context::execution();
        !instance.is_null()
            && (contains(instance, rip)
                || thread_runtime::r4os_rust_program_thread_entry(
                    1,
                    instance,
                    rip as usize,
                    ptr::null_mut(),
                ))
    }
}
/// # Safety
/// Exact retained instance metadata in resident fault context; no mutex, pin,
/// allocation, formatter or pager call is made while attributing the fault.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_peer_contains(
    instance: *const ProgramInstance,
    rip: u64,
) -> bool {
    unsafe { contains(instance, rip) }
}
/// # Safety
/// Exclusive canonical crash output and IRQ-safe retained task/thread records.
/// scan=false requires the original exact nonnull instance; scan=true uses
/// current execution first, then the stable intrusive thread owner index.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_peer_crash(
    instance: *const ProgramInstance,
    task: u32,
    rip: u64,
    scan: bool,
    out: *mut ExecutionContext,
) {
    unsafe {
        if !scan {
            out.write(crash_context(instance, task, rip));
            return;
        }
        let current = scheduler::current();
        let task = if current.is_null() {
            0
        } else {
            (*task_registry::core(current.cast())).id
        };
        let instance = context::execution();
        if !instance.is_null() {
            out.write(crash_context(instance, task, rip));
            return;
        }
        let mut thread = (*threads::state()).head;
        while !thread.is_null() {
            let node = thread;
            thread = (*node).registry_next;
            if !(*node).used {
                continue;
            }
            let instance = (*node).owner_instance;
            if instance.is_null() || !contains(instance, rip) {
                continue;
            }
            out.write(crash_context(instance, task, rip));
            return;
        }
        out.write(ExecutionContext::default());
    }
}
fn text(value: &[u8]) {
    foundation::puts(value)
}
fn dec(value: u64) {
    foundation::dec(value)
}
fn signed(value: i32) {
    util::r4os_rust_program_util_signed(value)
}
fn label(group: u8, value: u8) {
    text(unsafe { util::bytes(util::r4os_rust_program_util_label(group, value)) });
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_peer_dump() {
    unsafe {
        process::r4os_rust_program_process_signal();
        text(b"R4X runtime\r\n");
        text(b"  foreground=");
        text(if (*super::exit_state::state()).foreground_present {
            b"yes"
        } else {
            b"no"
        });
        text(b" shell=");
        text(if (*super::exit_state::state()).shell_present {
            b"yes"
        } else {
            b"no"
        });
        text(b" instances=");
        dec(u64::from(info::r4os_rust_program_info_count()));
        text(b" last_exit=");
        signed((*super::exit_state::state()).last_exit_code);
        text(b"\r\n");
        let mut shown = 0;
        let mut iterator = registry::Iterator {
            chunk: (*registry_state::state()).head,
            slot_index: 0,
            include_done: false,
        };
        loop {
            let instance = registry::r4os_rust_program_registry_iterator(&mut iterator);
            if instance.is_null() {
                break;
            }
            shown += 1;
            text(b"  #");
            dec(u64::from((*instance).id));
            text(b" task=");
            dec(u64::from((*instance).task_id));
            text(b" role=");
            label(0, (*instance).role);
            text(b" class=");
            label(1, (*instance).app_class);
            text(b" profile=");
            label(2, (*instance).memory_profile);
            text(b" state=");
            label(3, info::r4os_rust_program_info_state(instance));
            text(b" window=");
            let gui = (*instance).gui_payload.cast::<t::Gui>();
            signed(if gui.is_null() {
                -1
            } else {
                AtomicI32::from_ptr(ptr::addr_of_mut!((*gui).window_id)).load(Ordering::Acquire)
            });
            text(b" console_host=");
            label(
                4,
                if (*instance).console_payload.is_null() {
                    0
                } else {
                    u8::try_from((*payload::console(instance)).host).expect("Console host enum")
                },
            );
            text(b" exit=");
            signed((*instance).exit_code);
            text(b"\r\n");
        }
        if shown == 0 {
            text(b"  no active R4X instances\r\n");
        }
        let mut storage = MaybeUninit::<t::Stats>::uninit();
        payload::r4os_rust_program_payload_stats(storage.as_mut_ptr());
        let storage = storage.assume_init();
        for (name, value) in [
            (
                b"  storage core=".as_slice(),
                storage.core_bytes_per_instance,
            ),
            (b" live_core=", storage.live_core_bytes),
            (b" registry_core=", storage.registry_reserved_core_bytes),
            (b" payload=", storage.current_payload_bytes),
            (b" peak=", storage.peak_payload_bytes),
            (b" runtime=", storage.current_runtime_bytes),
            (b" console=", storage.current_console_bytes),
            (b" gui=", storage.current_gui_bytes),
            (b" failures=", storage.allocation_failures),
            (b" rollbacks=", storage.transaction_rollbacks),
            (
                b" owner_errors=",
                storage.owner_mismatches + storage.header_errors + storage.free_failures,
            ),
        ] {
            text(name);
            dec(value);
        }
        text(b"\r\n");
    }
}
const _: () = {
    assert!(mem::size_of::<ExecutionContext>() == 64);
    assert!(mem::align_of::<ExecutionContext>() == 8);
    assert!(mem::offset_of!(ExecutionContext, state) == 0);
    assert!(mem::offset_of!(ExecutionContext, r4x_instance_id) == 4);
    assert!(mem::offset_of!(ExecutionContext, task_id) == 8);
    assert!(mem::offset_of!(ExecutionContext, owner_id) == 16);
    assert!(mem::offset_of!(ExecutionContext, image_base) == 24);
    assert!(mem::offset_of!(ExecutionContext, image_size) == 32);
    assert!(mem::offset_of!(ExecutionContext, instruction_offset) == 40);
    assert!(mem::offset_of!(ExecutionContext, tag) == 48);
};
