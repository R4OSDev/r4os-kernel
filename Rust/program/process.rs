//! Exact-generation close, kill, wait and completion ownership. Console trees
//! retire newest descendants first; no registry owner spans peers or sleep.
use super::{completions, exit, reaper, registry, registry_state::state, registry_types::*};
use crate::{
    contract::{ProgramProcessCompletion as Completion, ProgramProcessHandle as Handle},
    platform::timer,
    sched::{scheduler, sync_objects},
};
use core::{
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};

#[repr(C)]
struct ConsoleLink {
    parent: Handle,
    hosted: bool,
}

fn signal() {
    unsafe { sync_objects::r4os_rust_sync_event((*state()).event, 0, 0) };
}
unsafe fn current_is(h: Handle) -> bool {
    let mut current = Handle::default();
    unsafe {
        super::context::r4os_rust_program_context_handle(&mut current)
            && registry::equal(current, h)
    }
}
unsafe fn history(id: u32) -> Option<i32> {
    if !registry::lock() {
        return None;
    }
    let mut result = None;
    unsafe {
        let s = state();
        for age in 0..(*s).history_count {
            let index = ((*s).history_head + 16 - 1 - age) % 16;
            let record = &(*(*s).history)[index];
            if record.used && record.handle.instance_id == id {
                result = Some(record.exit_code);
                break;
            }
        }
    }
    registry::unlock();
    result
}
unsafe fn handle_for_id(id: u32) -> Option<Handle> {
    let mut h = Handle::default();
    if unsafe { registry::r4os_rust_program_registry_handle(3, ptr::null(), id, &mut h) } {
        Some(h)
    } else {
        None
    }
}
unsafe fn descends(slot: *const ProgramRegistrySlot, host: Handle) -> bool {
    unsafe {
        let mut current = registry::handle_for_slot(slot);
        if (*slot).instance.console_payload.is_null() {
            return false;
        }
        let mut parent = program_process_console_link(&(*slot).instance).parent;
        while registry::valid(parent) {
            if registry::equal(parent, host) {
                return true;
            }
            if parent.generation >= current.generation {
                return false;
            }
            let p = registry::lookup_handle(parent, true);
            if p.is_null()
                || (*p).instance.app_class != 0
                || (*p).instance.console_payload.is_null()
            {
                return false;
            }
            current = parent;
            parent = program_process_console_link(&(*p).instance).parent;
        }
        false
    }
}
unsafe fn kill_clients(host: Handle) {
    loop {
        if !registry::lock() {
            return;
        }
        let mut client: Option<Handle> = None;
        unsafe {
            let mut chunk = (*state()).head;
            while !chunk.is_null() {
                for slot in &(*chunk).slots {
                    if !registry::running(slot.state)
                        || slot.instance.app_class != 0
                        || !descends(slot, host)
                    {
                        continue;
                    }
                    let h = registry::handle_for_slot(slot);
                    if client.is_none_or(|old| h.generation > old.generation) {
                        client = Some(h);
                    }
                }
                chunk = (*chunk).next;
            }
        }
        registry::unlock();
        let Some(child) = client else {
            return;
        };
        unsafe { exit::r4os_rust_program_exit_commit(child, -9, 2) };
    }
}
unsafe fn close_clients() {
    loop {
        if !registry::lock() {
            return;
        }
        let mut changed = None;
        unsafe {
            let mut chunk = (*state()).head;
            'search: while !chunk.is_null() {
                for index in 0..16 {
                    let slot = ptr::addr_of_mut!((*chunk).slots)
                        .cast::<ProgramRegistrySlot>()
                        .add(index);
                    if !registry::running((*slot).state)
                        || (*slot).instance.app_class != 0
                        || AtomicBool::from_ptr(ptr::addr_of_mut!((*slot).instance.close_requested))
                            .load(Ordering::Acquire)
                    {
                        continue;
                    }
                    let parent = registry::lookup_handle(
                        program_process_console_link(&(*slot).instance).parent,
                        false,
                    );
                    if parent.is_null()
                        || !AtomicBool::from_ptr(ptr::addr_of_mut!(
                            (*parent).instance.close_requested
                        ))
                        .load(Ordering::Acquire)
                    {
                        continue;
                    }
                    AtomicBool::from_ptr(ptr::addr_of_mut!((*slot).instance.close_requested))
                        .store(true, Ordering::Release);
                    changed = Some(registry::handle_for_slot(slot));
                    registry::epoch();
                    break 'search;
                }
                chunk = (*chunk).next;
            }
        }
        registry::unlock();
        let Some(h) = changed else {
            return;
        };
        unsafe { program_process_input(h) };
    }
}
unsafe fn hosted(instance: *const ProgramInstance) -> bool {
    unsafe {
        (*instance).role == 2
            && (*instance).app_class == 0
            && program_process_console_link(instance).hosted
    }
}
/// # Safety
/// Resident exclusive output; history is diagnostic and never consumes a node.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_process_history(id: u32, out: *mut i32) -> bool {
    if let Some(code) = unsafe { history(id) } {
        unsafe { out.write(code) };
        true
    } else {
        false
    }
}
/// # Safety
/// Legacy value ID; exact generation is reacquired before queue admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_process_release(id: u32) {
    if let Some(h) = unsafe { handle_for_id(id) } {
        unsafe { registry::r4os_rust_program_registry_retire(2, ptr::null_mut(), h) };
    }
}
/// # Safety
/// Original exit epilogue lifetime; no foreign registry owner is held.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_process_finish(id: u32) -> i32 {
    unsafe {
        let Some(h) = handle_for_id(id) else {
            return history(id).unwrap_or(-1);
        };
        let mut code = 0;
        if registry::lock() {
            let slot = registry::lookup_handle(h, true);
            if !slot.is_null() {
                code = (*slot).instance.exit_code;
            }
            registry::unlock();
        }
        exit::r4os_rust_program_exit_begin(h, code, 0);
        code
    }
}
/// # Safety
/// Caller holds registry admission and the exact resident slot lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_process_descends(
    slot: *const ProgramRegistrySlot,
    host: Handle,
) -> bool {
    unsafe { descends(slot, host) }
}
/// # Safety
/// No registry owner held. Op0 commits exact descendants; op1 propagates every
/// published console parent's close flag, as in the original host-agnostic scan.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_process_clients(op: u8, host: Handle) {
    unsafe {
        match op {
            0 => kill_clients(host),
            1 => close_clients(),
            _ => unreachable!(),
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_process_signal() {
    signal();
}
/// # Safety
/// Exact borrowed instance; console payload lifetime remains with its owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_process_hosted(
    instance: *const ProgramInstance,
) -> bool {
    unsafe { hosted(instance) }
}
/// # Safety
/// Original bounded pressure path, no registry owner or borrowed slot survives
/// reaper work/yield. Exact handle is revalidated on every check.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_process_pressure() {
    unsafe {
        let mut target = None;
        if registry::lock() {
            let mut chunk = (*state()).head;
            'search: while !chunk.is_null() {
                for index in 0..16 {
                    let slot = ptr::addr_of_mut!((*chunk).slots)
                        .cast::<ProgramRegistrySlot>()
                        .add(index);
                    if !matches!((*slot).state, 5 | 6)
                        || (*slot).instance.storage_teardown_blocked
                        || !hosted(&(*slot).instance)
                    {
                        continue;
                    }
                    target = Some(registry::handle_for_slot(slot));
                    if (*slot).state == 5 {
                        (*slot).state = 6;
                        registry::epoch();
                        registry::enqueue(slot);
                    }
                    break 'search;
                }
                chunk = (*chunk).next;
            }
            registry::unlock();
        }
        if let Some(h) = target {
            signal();
            let deadline =
                timer::r4os_rust_timer_control(7, u64::from(timer::frequency()).max(100), 0);
            loop {
                if registry::lock() {
                    let released = registry::lookup_handle(h, true).is_null();
                    registry::unlock();
                    if released {
                        break;
                    }
                }
                reaper::r4os_rust_program_reaper_once();
                if timer::tick_count() >= deadline {
                    break;
                }
                scheduler::yield_task();
            }
        }
        registry::shrink(true);
    }
}
/// # Safety
/// Null output is rejected without a write; otherwise exclusive aligned output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_process_open(id: u32, out: *mut Handle) -> i32 {
    if out.is_null() || id == 0 {
        return -1;
    }
    unsafe { out.write(Handle::default()) };
    if !registry::lock() {
        return -5;
    }
    let result = unsafe {
        let slot = registry::lookup_id(id, true);
        if !slot.is_null() {
            out.write(registry::handle_for_slot(slot));
            0
        } else {
            let mut node = (*state()).completion_head;
            let mut status = -2;
            while !node.is_null() {
                if (*node).state != 3 && (*node).handle.instance_id == id {
                    out.write((*node).handle);
                    status = 0;
                    break;
                }
                node = (*node).next;
            }
            status
        }
    };
    registry::unlock();
    result
}
unsafe fn missing_running(h: Handle) -> i32 {
    unsafe {
        if !registry::completion_handle(h).is_null() {
            -4
        } else {
            registry::missing(h)
        }
    }
}
/// # Safety
/// Null is accepted as an invalid argument. Otherwise aligned resident handle;
/// published flag precedes unlocked input and GUI activity notifications.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_process_close(input: *const Handle) -> i32 {
    if input.is_null() || !registry::valid(unsafe { *input }) {
        return -1;
    }
    unsafe {
        let h = *input;
        if !registry::lock() {
            return -5;
        }
        let slot = registry::lookup_handle(h, true);
        let status = if slot.is_null() {
            missing_running(h)
        } else if !registry::running((*slot).state) {
            -4
        } else {
            0
        };
        if status != 0 {
            registry::unlock();
            return status;
        }
        let flag = AtomicBool::from_ptr(ptr::addr_of_mut!((*slot).instance.close_requested));
        if !flag.load(Ordering::Acquire) {
            flag.store(true, Ordering::Release);
            registry::epoch();
        }
        registry::unlock();
        program_process_input(h);
        crate::service::desktop_events::r4os_rust_desktop_signal();
        close_clients();
    }
    0
}
/// # Safety
/// Aligned resident handle or null. Caller identity is checked before registry;
/// retry revalidates exact generation and leaves admission before yielding.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_process_kill(input: *const Handle) -> i32 {
    if input.is_null() || !registry::valid(unsafe { *input }) {
        return -1;
    }
    unsafe {
        let h = *input;
        if current_is(h) {
            return -7;
        }
        if !registry::lock() {
            return -5;
        }
        let slot = registry::lookup_handle(h, true);
        let status = if slot.is_null() {
            missing_running(h)
        } else if !registry::running((*slot).state) {
            -4
        } else {
            0
        };
        registry::unlock();
        if status != 0 {
            return status;
        }
        loop {
            match exit::r4os_rust_program_exit_commit(h, -9, 2) {
                0 => {
                    kill_clients(h);
                    signal();
                    return 0;
                }
                1 => return -4,
                2 => {}
                _ => unreachable!(),
            }
            if !registry::lock() {
                return -5;
            }
            let slot = registry::lookup_handle(h, true);
            let status = if slot.is_null() {
                missing_running(h)
            } else if !registry::running((*slot).state) {
                -4
            } else {
                0
            };
            registry::unlock();
            if status != 0 {
                return status;
            }
            scheduler::yield_task();
        }
    }
}
/// # Safety
/// Aligned resident input and exclusive disjoint output or null. No registry
/// owner, node, unwind token or slot pin spans the one-tick blocking sleep.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_process_wait(
    input: *const Handle,
    timeout: u64,
    out: *mut Completion,
) -> i32 {
    if input.is_null() || out.is_null() || !registry::valid(unsafe { *input }) {
        return -1;
    }
    unsafe {
        out.write(Completion::default());
        let h = *input;
        let start = timer::tick_count();
        loop {
            let status = completions::r4os_rust_program_completion_access(1, h, out);
            if status != -5 {
                return status;
            }
            if current_is(h) {
                return -7;
            }
            if timeout != u64::MAX && timer::tick_count().wrapping_sub(start) >= timeout {
                return -6;
            }
            scheduler::r4os_rust_scheduler_task(4, ptr::null_mut(), 1, 0, b"sleep".as_ptr(), 5);
        }
    }
}
/// # Safety
/// Same value/output contract as wait; one ready completion is consumed once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_process_reap(
    input: *const Handle,
    out: *mut Completion,
) -> i32 {
    if input.is_null() || out.is_null() || !registry::valid(unsafe { *input }) {
        return -1;
    }
    unsafe {
        out.write(Completion::default());
        completions::r4os_rust_program_completion_access(2, *input, out)
    }
}
/// Read the executing pinned instance's cooperative close flag.
pub(crate) fn should_close() -> bool {
    unsafe {
        let instance = super::context::current();
        !instance.is_null()
            && AtomicBool::from_ptr(ptr::addr_of_mut!((*instance).close_requested))
                .load(Ordering::Acquire)
    }
}
/// # Safety
/// Original legacy value/output contracts; current execution remains pinned.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_process_legacy(op: u8, id: u32) -> i32 {
    unsafe {
        if op == 3 {
            return i32::from(should_close());
        }
        let mut h = Handle::default();
        if r4os_rust_program_process_open(id, &mut h) != 0 {
            return -1;
        }
        let mut out = Completion::default();
        let status = match op {
            0 => r4os_rust_program_process_close(&h),
            1 => r4os_rust_program_process_kill(&h),
            2 => r4os_rust_program_process_reap(&h, &mut out),
            _ => unreachable!(),
        };
        if status == 0 {
            if op == 2 { out.exit_code } else { 0 }
        } else {
            -2
        }
    }
}
const _: () = {
    assert!(core::mem::size_of::<ConsoleLink>() == 24);
    assert!(core::mem::offset_of!(ConsoleLink, hosted) == 16);
};

unsafe fn program_process_input(handle: Handle) {
    crate::program::console_input::r4os_rust_program_console_input_signal_handle(handle);
    super::gui_activity::signal_handle(handle);
}

unsafe fn program_process_console_link(instance: *const ProgramInstance) -> ConsoleLink {
    unsafe {
        let c = (*instance)
            .console_payload
            .cast::<crate::program::payload_types::Console>();
        ConsoleLink {
            parent: Handle {
                instance_id: (*c).io_target_id,
                reserved: 0,
                generation: (*c).io_target_generation,
            },
            hosted: (*c).host != 0,
        }
    }
}
