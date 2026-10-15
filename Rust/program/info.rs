//! Legacy/current process information and coherent pinned row selection.
//! Query values borrow existing registry, exit and VM owners; no second state.
use super::{exit_state, registry, registry_state::state, registry_types::*};
use crate::{
    contract::{
        self, ProgramInstanceInfo as Info, ProgramProcessHandle as Handle, ProgramStatus as Status,
    },
    memory::vm_query,
};
use core::{
    mem::MaybeUninit,
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};
unsafe extern "C" {
    fn r4os_zig_program_info_terminal(instance: *const ProgramInstance) -> bool;
    fn r4os_zig_program_info_window(instance: *const ProgramInstance) -> i32;
}
#[repr(C)]
pub struct Snapshot {
    pub id: u32,
    pub task_id: u32,
    pub app_class: u8,
    pub state: u8,
    pub close_requested: bool,
    pub exit_code: i32,
}
unsafe fn instance_state(instance: *const ProgramInstance) -> u8 {
    unsafe {
        if AtomicBool::from_ptr(ptr::addr_of!((*instance).done).cast_mut()).load(Ordering::Acquire)
        {
            2
        } else if AtomicBool::from_ptr(ptr::addr_of!((*instance).close_requested).cast_mut())
            .load(Ordering::Acquire)
        {
            1
        } else {
            0
        }
    }
}
unsafe fn vm_stats(instance: *const ProgramInstance, dynamic: bool) -> vm_query::OwnerStats {
    let mut out = MaybeUninit::uninit();
    unsafe {
        vm_query::r4os_rust_vm_owner_stats(
            3,
            u64::from((*instance).id),
            if dynamic { 6 } else { 255 },
            if dynamic { 4 } else { 255 },
            out.as_mut_ptr(),
        );
        out.assume_init()
    }
}
pub(super) unsafe fn instance_info(instance: *const ProgramInstance) -> Info {
    unsafe {
        let mut flags = 0;
        if (*instance).desktop_host {
            flags |= contract::program_instance_flag_desktop_host;
        }
        if AtomicBool::from_ptr(ptr::addr_of!((*instance).close_requested).cast_mut())
            .load(Ordering::Acquire)
        {
            flags |= 1;
        }
        if AtomicBool::from_ptr(ptr::addr_of!((*instance).desktop_requested).cast_mut())
            .load(Ordering::Acquire)
        {
            flags |= 2;
        }
        if r4os_zig_program_info_terminal(instance) {
            flags |= 4;
        }
        let usage = vm_stats(instance, false);
        Info {
            id: (*instance).id,
            task_id: (*instance).task_id,
            role: (*instance).role,
            app_class: (*instance).app_class,
            state: instance_state(instance),
            flags,
            exit_code: (*instance).exit_code,
            window_id: r4os_zig_program_info_window(instance),
            memory_profile: (*instance).memory_profile,
            reserved0: [0; 3],
            memory_reserved_limit: (*instance).memory_limits.vm_reserve_limit,
            memory_committed_limit: (*instance).memory_limits.vm_commit_limit,
            memory_resident_limit: (*instance).memory_limits.resident_limit,
            memory_reserved_bytes: usage.reserved_bytes,
            memory_committed_bytes: usage.committed_bytes,
            memory_resident_bytes: usage.resident_bytes,
            memory_peak_resident_bytes: usage.peak_resident_bytes,
            stack_reserved_bytes: (*instance).program_stack_reserve_size,
            stack_committed_bytes: (*instance).program_stack_committed_size,
            memory_tag: (*instance).memory_tag,
        }
    }
}
pub(super) unsafe fn completion_info(node: *const ProgramCompletionNode) -> Info {
    unsafe {
        Info {
            id: (*node).handle.instance_id,
            task_id: (*node).task_id,
            role: (*node).role,
            app_class: (*node).app_class,
            state: if (*node).state == 2 { 2 } else { 1 },
            exit_code: (*node).exit_code,
            ..Info::default()
        }
    }
}
unsafe fn by_id(id: u32) -> *mut ProgramInstance {
    if !registry::lock() {
        return ptr::null_mut();
    }
    let slot = unsafe { registry::lookup_id(id, false) };
    let result = if slot.is_null() {
        ptr::null_mut()
    } else {
        unsafe { ptr::addr_of_mut!((*slot).instance) }
    };
    registry::unlock();
    result
}
unsafe fn active_count() -> u8 {
    let mut it = registry::Iterator {
        chunk: unsafe { (*state()).head },
        slot_index: 0,
        include_done: false,
    };
    let mut count = 0u8;
    while !unsafe { registry::r4os_rust_program_registry_iterator(&mut it) }.is_null() {
        if count == u8::MAX {
            return count;
        }
        count += 1;
    }
    count
}
/// # Safety
/// Caller retains the original boot/registry/cooperative shell-state admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_info_shell() -> u32 {
    unsafe {
        let s = exit_state::state();
        if (*s).shell_present { (*s).shell_id } else { 0 }
    }
}
/// # Safety
/// Original cooperative by-ID borrow. Caller must retain its unchanged task/
/// registry lifetime; the lookup releases registry before returning the pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_info_by_id(id: u32) -> *mut ProgramInstance {
    unsafe { by_id(id) }
}
/// # Safety
/// Exclusive aligned snapshot output; same cooperative lifetime as by-ID lookup.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_info_snapshot(id: u32, out: *mut Snapshot) -> bool {
    unsafe {
        let instance = by_id(id);
        if instance.is_null() {
            return false;
        }
        out.write(Snapshot {
            id: (*instance).id,
            task_id: (*instance).task_id,
            app_class: (*instance).app_class,
            state: instance_state(instance),
            close_requested: AtomicBool::from_ptr(ptr::addr_of_mut!((*instance).close_requested))
                .load(Ordering::Acquire),
            exit_code: (*instance).exit_code,
        });
        true
    }
}
/// # Safety
/// Original cooperative registry iterator lifetime; no wait/reclaim spans scan.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_info_count() -> u8 {
    unsafe { active_count() }
}
/// # Safety
/// Exact resident instance under original cooperative/pinned/registry lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_info_state(instance: *const ProgramInstance) -> u8 {
    unsafe { instance_state(instance) }
}
/// # Safety
/// Stable instance under original lifetime and exclusive aligned output. VM
/// owner query and window atomic load retain their original order and owners.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_info_instance(
    instance: *const ProgramInstance,
    out: *mut Info,
) {
    unsafe { out.write(instance_info(instance)) };
}
/// # Safety
/// Exact completion under registry admission; output is exclusive aligned info.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_info_completion(
    node: *const ProgramCompletionNode,
    out: *mut Info,
) {
    unsafe { out.write(completion_info(node)) };
}
/// # Safety
/// Stable instance and exclusive aligned existing VM OwnerStats output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_info_vm(
    instance: *const ProgramInstance,
    dynamic: bool,
    out: *mut vm_query::OwnerStats,
) {
    unsafe { out.write(vm_stats(instance, dynamic)) };
}
/// # Safety
/// Aligned resident input/exclusive output or null; invalid value handles reset
/// output after pointer admission, matching the original API ordering.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_info_handle(
    input: *const Handle,
    out: *mut Info,
) -> i32 {
    if input.is_null() || out.is_null() {
        return -1;
    }
    unsafe {
        out.write(Info::default());
        let h = *input;
        if !registry::valid(h) {
            return -1;
        }
        let mut raw = MaybeUninit::<ProgramInstanceLease>::uninit();
        if registry::r4os_rust_program_registry_pin(h, true, raw.as_mut_ptr()) {
            let lease = raw.assume_init();
            out.write(instance_info(lease.instance));
            registry::unpin(&lease);
            return 0;
        }
        if !registry::lock() {
            return -5;
        }
        let node = registry::completion_handle(h);
        let result = if node.is_null() {
            registry::missing(h)
        } else {
            out.write(completion_info(node));
            0
        };
        registry::unlock();
        result
    }
}
/// # Safety
/// Exclusive resident status output; caller keeps original cooperative scan
/// lifetime. Optional presence remains distinct from a stored zero ID.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_info_status(out: *mut Status) {
    unsafe {
        let s = exit_state::state();
        out.write(Status {
            foreground_running: u8::from((*s).foreground_present),
            shell_running: u8::from((*s).shell_present),
            display_used: u8::from((*s).last_display_used),
            instance_count: active_count(),
            last_exit_code: (*s).last_exit_code,
        });
    }
}
/// # Safety
/// Original legacy ordinal API, exclusive resident output. The generation merge
/// selects under registry and pins only its exact live result before unlocking.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_info_index(index: u32, out: *mut Info) -> i32 {
    unsafe {
        out.write(Info::default());
        let mut lease: Option<ProgramInstanceLease> = None;
        let mut completed = None;
        if !registry::lock() {
            return 0;
        }
        let mut after = 0;
        let mut ordinal = 0;
        while ordinal <= index {
            let mut best = u64::MAX;
            let mut found = false;
            let mut best_slot = ptr::null_mut();
            let mut best_node = ptr::null_mut();
            let mut chunk = (*state()).head;
            while !chunk.is_null() {
                for slot in &(*chunk).slots {
                    if !registry::visible(slot.state) || slot.generation <= after {
                        continue;
                    }
                    if !found || slot.generation < best {
                        best = slot.generation;
                        found = true;
                        best_slot = (slot as *const ProgramRegistrySlot).cast_mut();
                        best_node = ptr::null_mut();
                    }
                }
                chunk = (*chunk).next;
            }
            let mut node = (*state()).completion_head;
            while !node.is_null() {
                if !(*node).legacy_id || (*node).state == 3 || (*node).handle.generation <= after {
                    node = (*node).next;
                    continue;
                }
                let slot = registry::lookup_handle((*node).handle, true);
                if !slot.is_null() && registry::visible((*slot).state) {
                    node = (*node).next;
                    continue;
                }
                if !found || (*node).handle.generation < best {
                    best = (*node).handle.generation;
                    found = true;
                    best_slot = ptr::null_mut();
                    best_node = node;
                }
                node = (*node).next;
            }
            if !found {
                registry::unlock();
                return 0;
            }
            if ordinal == index {
                if !best_slot.is_null() {
                    if (*best_slot).pin_count == u32::MAX {
                        registry::unlock();
                        return 0;
                    }
                    (*best_slot).pin_count += 1;
                    let mut selected: ProgramInstanceLease = core::mem::zeroed();
                    selected.slot = best_slot;
                    selected.instance = ptr::addr_of_mut!((*best_slot).instance);
                    selected.id = (*best_slot).public_id;
                    selected.generation = (*best_slot).generation;
                    lease = Some(selected);
                } else if !best_node.is_null() {
                    completed = Some(completion_info(best_node));
                }
                break;
            }
            after = best;
            ordinal += 1;
        }
        registry::unlock();
        if let Some(lease) = lease {
            out.write(instance_info(lease.instance));
            registry::unpin(&lease);
            return 1;
        }
        if let Some(info) = completed {
            out.write(info);
            return 1;
        }
        0
    }
}
const _: () = {
    assert!(core::mem::size_of::<Snapshot>() == 16);
    assert!(core::mem::offset_of!(Snapshot, exit_code) == 12);
    assert!(core::mem::size_of::<Info>() == 112);
};
