//! Ordered program retirement and its original background reaper loop.
//! Exact phase/once/pin ownership is native. Task, I/O, payload, VM and audio
//! leaves keep their current owners; no registry lock spans those operations.
use super::{
    completions, failure, registry, registry_state::state, registry_types::*, retire_phase,
    retire_progress, retire_wait::Wait,
};
use crate::{
    contract::ProgramProcessHandle as Handle,
    sched::{scheduler, sync_objects},
};
use core::{mem, ptr};

unsafe fn deferred(handle: Handle, wait: Wait, notify: bool) -> u8 {
    unsafe { retire_phase::defer(handle, wait, notify) }
}
unsafe fn advance(
    slot: *mut ProgramRegistrySlot,
    h: Handle,
    n: *mut ProgramCompletionNode,
    from: u8,
    to: u8,
) -> bool {
    unsafe { retire_phase::r4os_rust_program_retire_advance(slot, h, n, from, to) }
}
unsafe fn action(
    op: u8,
    h: Handle,
    s: *mut ProgramRegistrySlot,
    n: *mut ProgramCompletionNode,
    r: bool,
) -> bool {
    unsafe {
        match op {
            0 => super::async_io::cancel(h, None),
            1 => {
                super::thread_retire::r4os_rust_program_threads_terminate(h, -9, false, 0);
                true
            }
            2 => super::thread_retire::release_ready(h, r),
            3 => {
                crate::serial::reap_process(h)
                    && super::process_data::r4os_rust_program_data_local_cleanup(ptr::addr_of_mut!(
                        (*s).instance
                    ))
            }
            4 => super::peer_lifetime::notifications_cleanup(ptr::addr_of_mut!((*s).instance)),
            12 => super::peer_lifetime::resource_step(ptr::addr_of_mut!((*s).instance)),
            13 => super::peer_lifetime::vm_release(h.instance_id),
            14 => {
                super::peer_lifetime::backing_release(h.instance_id);
                true
            }
            5 => super::async_io::purge(h, None),
            6 => super::async_io::release_ranges(h),
            10 => super::instance_storage::detach(ptr::addr_of_mut!((*s).instance), n),
            11 => super::instance_storage::release(ptr::addr_of_mut!((*s).instance), n),
            _ => program_reap_action(op, h, s, n, r),
        }
    }
}
unsafe fn clear_image_stack(slot: *mut ProgramRegistrySlot) {
    unsafe {
        let i = ptr::addr_of_mut!((*slot).instance);
        (*i).used = false;
        (*i).task_id = 0;
        (*i).program_image_range_id = 0;
        (*i).program_image_base = 0;
        (*i).program_image_size = 0;
        (*i).program_stack_range_id = 0;
        (*i).program_stack_base = 0;
        (*i).program_stack_reserve_size = 0;
        (*i).program_stack_committed_base = 0;
        (*i).program_stack_committed_size = 0;
        (*i).program_stack_guard_base = 0;
        (*i).program_stack_guard_size = 0;
        (*i).program_stack_initial_commit_size = 0;
        (*i).program_stack_create_cycles = 0;
        (*i).program_stack_telemetry_high_water = 0;
        (*i).program_stack_telemetry_committed_pages = 0;
        (*i).program_stack_serial_telemetry = false;
        (*i).program_stack_telemetry_measured = false;
        (*slot).retire_image_stack_released = true;
    }
}
/// # Safety
/// Exact resident slot is selected under the original registry owner and
/// retains its worker/reservation lifetime through all unlocked leaf calls.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_reap_slot(slot: *mut ProgramRegistrySlot) -> u8 {
    unsafe { reap_slot(slot, true) }
}
unsafe fn reap_slot(slot: *mut ProgramRegistrySlot, notify: bool) -> u8 {
    unsafe {
        if !registry::lock() {
            return 2;
        }
        if (*slot).state == 1 && (*slot).cancel_pending && (*slot).retire_in_progress {
            let mut r: ProgramInstanceReservation = mem::zeroed();
            r.slot = slot;
            r.id = (*slot).public_id;
            r.generation = (*slot).generation;
            registry::unlock();
            return registry::finish_cancel_notify(&r, notify);
        }
        if (*slot).state != 6
            || !(*slot).retire_in_progress
            || (*slot).public_id == 0
            || (*slot).generation == 0
        {
            registry::unlock();
            return 0;
        }
        let handle = registry::handle_for_slot(slot);
        let node = (*slot).completion;
        if node.is_null() {
            registry::unlock();
            return deferred(handle, Wait::Retry, notify);
        }
        let report = (*node).role == 0;
        let mut phase = (*slot).retire_phase;
        registry::unlock();
        while phase != 5 {
            match phase {
                0 => {
                    program_reap_report(report, 0);
                    if !action(0, handle, slot, node, report) {
                        return deferred(handle, super::async_io::waiting(handle, None), notify);
                    }
                    if !advance(slot, handle, node, 0, 1) {
                        return deferred(handle, Wait::Retry, notify);
                    }
                    if failure::consume(8) {
                        return deferred(handle, Wait::Retry, notify);
                    }
                    phase = 1;
                }
                1 => {
                    program_reap_report(report, 1);
                    action(1, handle, slot, node, report);
                    if !action(2, handle, slot, node, report) {
                        return deferred(handle, super::thread_retire::waiting(handle), notify);
                    }
                    if !action(3, handle, slot, node, report) {
                        let wait = if crate::serial::retire_wait_process(handle) {
                            Wait::Progress
                        } else {
                            Wait::Retry
                        };
                        return deferred(handle, wait, notify);
                    }
                    if !action(4, handle, slot, node, report) {
                        return deferred(handle, Wait::Retry, notify);
                    }
                    program_reap_report(report, 2);
                    if !action(5, handle, slot, node, report) {
                        return deferred(handle, super::async_io::waiting(handle, None), notify);
                    }
                    if !action(6, handle, slot, node, report) {
                        return deferred(handle, Wait::Retry, notify);
                    }
                    if !action(7, handle, slot, node, report) {
                        return deferred(handle, Wait::Retry, notify);
                    }
                    if !action(8, handle, slot, node, report) {
                        let wait = if crate::storage::operations::retire_wait_owner(
                            crate::storage::operations::Args {
                                program: handle.instance_id,
                                program_generation: handle.generation,
                                ..Default::default()
                            },
                        ) {
                            Wait::Progress
                        } else {
                            Wait::Retry
                        };
                        return deferred(handle, wait, notify);
                    }
                    if !action(9, handle, slot, node, report) {
                        return deferred(handle, Wait::Retry, notify);
                    }
                    if !advance(slot, handle, node, 1, 2) {
                        return deferred(handle, retire_phase::pin_wait(handle), notify);
                    }
                    if failure::consume(9) {
                        return deferred(handle, Wait::Retry, notify);
                    }
                    phase = 2;
                }
                2 => {
                    program_reap_report(report, 3);
                    if !(*slot).retire_output_detached {
                        if !action(10, handle, slot, node, report) {
                            let wait = if (*slot).instance.storage_teardown_blocked {
                                Wait::Blocked
                            } else {
                                Wait::Retry
                            };
                            return deferred(handle, wait, notify);
                        }
                        (*slot).retire_output_detached = true;
                    }
                    if !advance(slot, handle, node, 2, 3) {
                        return deferred(handle, retire_phase::pin_wait(handle), notify);
                    }
                    if failure::consume(10) {
                        return deferred(handle, Wait::Retry, notify);
                    }
                    phase = 3;
                }
                3 => {
                    program_reap_report(report, 4);
                    if !(*slot).retire_storage_released {
                        if !action(11, handle, slot, node, report) {
                            let wait = if (*slot).instance.storage_teardown_blocked {
                                Wait::Blocked
                            } else {
                                Wait::Retry
                            };
                            return deferred(handle, wait, notify);
                        }
                        (*slot).retire_storage_released = true;
                    }
                    if !advance(slot, handle, node, 3, 4) {
                        return deferred(handle, retire_phase::pin_wait(handle), notify);
                    }
                    if failure::consume(11) {
                        return deferred(handle, Wait::Retry, notify);
                    }
                    phase = 4;
                }
                4 => {
                    program_reap_report(report, 5);
                    if !(*slot).retire_image_stack_released {
                        if !action(12, handle, slot, node, report) {
                            let instance = ptr::addr_of!((*slot).instance);
                            let range = if (*instance).program_stack_range_id != 0 {
                                (*instance).program_stack_range_id
                            } else {
                                (*instance).program_image_range_id
                            };
                            let wait = if crate::memory::vm_pager_api::retire_wait_range(range) {
                                Wait::Progress
                            } else {
                                Wait::Retry
                            };
                            return deferred(handle, wait, notify);
                        }
                        clear_image_stack(slot);
                    }
                    if !(*slot).retire_owner_released {
                        if !action(13, handle, slot, node, report) {
                            let wait = if crate::memory::vm_pager_api::retire_wait_owner(
                                3,
                                u64::from(handle.instance_id),
                            ) {
                                Wait::Progress
                            } else {
                                Wait::Retry
                            };
                            return deferred(handle, wait, notify);
                        }
                        action(14, handle, slot, node, report);
                        (*slot).retire_owner_released = true;
                    }
                    if !advance(slot, handle, node, 4, 5) {
                        return deferred(handle, retire_phase::pin_wait(handle), notify);
                    }
                    if failure::consume(12) {
                        return deferred(handle, Wait::Retry, notify);
                    }
                    phase = 5;
                }
                _ => unreachable!(),
            }
        }
        program_reap_report(report, 6);
        if failure::consume(13) {
            return deferred(handle, Wait::Retry, notify);
        }
        if !registry::lock() {
            return deferred(handle, Wait::Retry, notify);
        }
        let current = registry::lookup_handle(handle, true);
        if current.is_null() {
            registry::unlock();
            return 0;
        }
        if current != slot
            || (*current).state != 6
            || !(*current).retire_in_progress
            || (*current).retire_phase != 5
            || (*current).pin_count != 0
            || (*current).completion != node
        {
            registry::unlock();
            return deferred(handle, retire_phase::pin_wait(handle), notify);
        }
        (*current).state = 7;
        (*current).completion = ptr::null_mut();
        (*node).slot_attached = false;
        (*node).flags |= 1;
        if (*node).retain_output {
            (*node).flags |= crate::contract::program_completion_flag_output_available;
        }
        (*node).state = 2;
        let stats = (*state()).stats;
        if (*stats).completion_pending != 0 {
            (*stats).completion_pending -= 1;
        }
        let observed = (*node).observation_refs != 0;
        let auto_consume = if (*node).owner || observed {
            (*stats).completion_ready = (*stats).completion_ready.wrapping_add(1);
            false
        } else {
            registry::unlink_completion(node);
            (*node).state = 3;
            true
        };
        registry::clear_slot(current);
        registry::unlock();
        if observed {
            super::observations::signal();
        }
        program_reap_report(report, 7);
        if auto_consume {
            completions::release(node);
        }
        failure::note(true);
        registry::shrink(false);
        1
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_reaper_once() -> bool {
    if !registry::lock() {
        return false;
    }
    let slot = unsafe { registry::take() };
    registry::unlock();
    !slot.is_null() && unsafe { r4os_rust_program_reap_slot(slot) == 1 }
}
fn sleep(reason: &'static [u8]) {
    unsafe {
        scheduler::r4os_rust_scheduler_task(4, ptr::null_mut(), 1, 0, reason.as_ptr(), reason.len())
    };
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_reaper_main() -> ! {
    loop {
        // Each known dependency enrolls under its own owner before the final
        // check. The resident auto Event retains an edge arriving before wait.
        let async_result = unsafe { super::async_io::reap() };
        if !registry::lock() {
            sleep(b"program_reaper_lock");
            continue;
        }
        let slot = unsafe { registry::take() };
        registry::unlock();
        if !slot.is_null() {
            unsafe { reap_slot(slot, false) };
            continue;
        }
        if async_result == 1 {
            continue;
        }
        if !registry::lock() {
            sleep(b"program_reaper_lock");
            continue;
        }
        let (_program_pending, mut deadline) = unsafe { registry::retire_wait() };
        registry::unlock();
        let Some((_async_pending, async_deadline)) = (unsafe { super::async_io::retire_wait() })
        else {
            sleep(b"async_io_reaper_lock");
            continue;
        };
        if let Some(due) = async_deadline {
            deadline = Some(deadline.map_or(due, |old| old.min(due)));
        }
        if async_result == 2 {
            let due = crate::platform::timer::tick_count().saturating_add(1);
            deadline = Some(deadline.map_or(due, |old| old.min(due)));
        }
        let timeout = deadline.map_or(u64::MAX, |due| {
            due.saturating_sub(crate::platform::timer::tick_count())
        });
        let result = unsafe { sync_objects::r4os_rust_sync_event((*state()).event, 2, timeout) };
        if result == 1 {
            // A finite retry timeout must not make unrelated progress waits
            // eligible. Only a notification carrying actual progress advances this round.
            while !retire_progress::received() {
                sleep(b"program_reaper_round");
            }
        } else if result != 2 {
            sleep(b"program_reaper_wait");
        }
    }
}

unsafe fn program_reap_action(
    op: u8,
    handle: Handle,
    slot: *mut ProgramRegistrySlot,
    node: *mut ProgramCompletionNode,
    report: bool,
) -> bool {
    let _ = report;
    unsafe {
        match op {
            7 => crate::program::system::stream::r4os_rust_sys_stream_cleanup(
                handle.instance_id,
                handle.generation,
                0,
                0,
                false,
            ),
            8 => crate::storage::operations::release_owner(crate::storage::operations::Args {
                program: handle.instance_id,
                program_generation: handle.generation,
                ..Default::default()
            }),
            9 => {
                let owner = crate::audio::mixer::Owner {
                    instance_id: handle.instance_id,
                    generation: handle.generation,
                };
                crate::audio::core::close_synths_for_owner(owner)
                    && crate::audio::core::close_streams_for_owner(owner)
            }
            10 => crate::program::instance_storage::detach(
                core::ptr::addr_of_mut!((*slot).instance),
                node,
            ),
            11 => crate::program::instance_storage::release(
                core::ptr::addr_of_mut!((*slot).instance),
                node,
            ),
            _ => panic!("Invalid process reap peer action"),
        }
    }
}

unsafe fn program_reap_report(report: bool, stage: u8) {
    let detail: &[u8] = match stage {
        0 => b"SERVMAN: Abbruchpruefung",
        1 => b"SERVMAN: Taskabbau",
        2 => b"SERVMAN: I/O-Abbau",
        3 => b"SERVMAN: Ausgabeabbau",
        4 => b"SERVMAN: Kontextabbau",
        5 => b"SERVMAN: Imageabbau",
        6 => b"SERVMAN: Abschluss",
        7 => b"SERVMAN: Bereit",
        _ => panic!("Invalid reap stage"),
    };
    crate::program::entry::report_stage(report, detail, false);
}
