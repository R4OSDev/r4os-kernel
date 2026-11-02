//! Registry lifecycle and public snapshot orchestration. Registry-to-endpoint
//! handoff retains the original try/release/wait/revalidate ordering.
use super::{
    endpoint,
    registry::{self as reg, Identity},
    registry_types::*,
};
use crate::{contract as c, fs::vfs_types::Bytes};
use core::ptr;
#[derive(Clone, Copy)]
pub(super) enum Key {
    Name(Bytes),
    Target(ApiIndexTarget),
}
#[derive(Clone, Copy)]
pub(super) enum Change {
    Unregister,
    State(u8, u32, i32, Bytes),
    Begin(*mut StartReservation),
    Stopping,
    Finish(bool, i32, Bytes),
    Mode(u8),
    ClearBinding,
    RestartCount,
    CancelStart,
    Complete(i32),
}
unsafe fn key_slot(key: Key) -> Option<usize> {
    unsafe {
        match key {
            Key::Name(name) => reg::find_name(name),
            Key::Target(target) => reg::target_slot(&target, true),
        }
    }
}
unsafe fn finish_entry(slot: usize, failed: bool, code: i32, text: Bytes) {
    unsafe {
        let e = &mut *reg::entry(slot);
        e.state = if failed {
            STATE_FAILED
        } else if e.start_mode == MODE_DISABLED {
            STATE_DISABLED
        } else {
            STATE_STOPPED
        };
        e.start_generation = 0;
        e.instance_id = 0;
        e.exit_code = code;
        e.start_tick = 0;
        e.last_error_len = reg::copy(reg::bytes(text), &mut e.last_error);
    }
}
unsafe fn apply(key: Key, slot: usize, change: Change) -> i32 {
    unsafe {
        let r = reg::raw();
        match change {
            Change::Unregister => {
                let result = if (*r).entries.remove(slot) { 0 } else { -4 };
                if result == 0 {
                    reg::structure_changed();
                }
                result
            }
            Change::State(state, id, code, text) => {
                let Key::Name(name) = key else {
                    unreachable!();
                };
                let _ = name;
                reg::set_state_in(slot, state, id, code, reg::bytes(text))
            }
            Change::Begin(out) => {
                let Some(generation) = (*r).next_start_generation.checked_add(1) else {
                    return c::service_api_result_exhausted;
                };
                (*r).next_start_generation = generation;
                let e = reg::entry(slot);
                (*e).start_generation = (*r).next_start_generation;
                (*e).state = STATE_STARTING;
                (*e).instance_id = 0;
                (*e).exit_code = 0;
                (*e).last_error_len = 0;
                out.write(StartReservation {
                    slot,
                    generation: (*e).start_generation,
                    entry: e.read(),
                    registry_generation: (*r).generation,
                });
                c::service_api_result_ok
            }
            Change::Stopping => {
                (*reg::entry(slot)).state = STATE_STOPPING;
                0
            }
            Change::Finish(failed, code, text) => {
                finish_entry(slot, failed, code, text);
                0
            }
            Change::Mode(mode) => {
                let e = &mut *reg::entry(slot);
                if e.start_mode != mode {
                    let Some(revision) = (*r).inventory_revision.checked_add(1) else {
                        return c::service_api_result_exhausted;
                    };
                    (*r).inventory_revision = revision;
                }
                e.start_mode = mode;
                if mode == MODE_DISABLED {
                    e.state = STATE_DISABLED;
                    e.start_generation = 0;
                    e.instance_id = 0;
                    e.start_tick = 0;
                } else if e.state == STATE_DISABLED {
                    e.state = STATE_STOPPED;
                }
                0
            }
            Change::ClearBinding => {
                (*reg::entry(slot)).binding = EMPTY_BINDING;
                0
            }
            Change::RestartCount => {
                let e = reg::entry(slot);
                (*e).restart_count = (*e).restart_count.wrapping_add(1);
                0
            }
            Change::CancelStart => {
                finish_entry(
                    slot,
                    false,
                    0,
                    Bytes {
                        data: ptr::null(),
                        len: 0,
                    },
                );
                0
            }
            Change::Complete(code) => {
                // Decide against current stop intent under this owner, not
                // against a state copied before an unlocked completion read.
                let failed = (*reg::entry(slot)).state != STATE_STOPPING && code != 0;
                finish_entry(
                    slot,
                    failed,
                    code,
                    Bytes {
                        data: if failed {
                            b"service exited".as_ptr()
                        } else {
                            ptr::null()
                        },
                        len: if failed { b"service exited".len() } else { 0 },
                    },
                );
                0
            }
        }
    }
}
pub(super) unsafe fn change(key: Key, action: Change) -> i32 {
    unsafe {
        let admission_error = if matches!(action, Change::Begin(_)) {
            c::service_api_result_busy
        } else {
            -1
        };
        loop {
            let Some(mut guard) = reg::lock_registry(0) else {
                return admission_error;
            };
            let Some(slot) = key_slot(key) else {
                return if matches!(action, Change::Begin(_)) {
                    c::service_api_result_not_found
                } else {
                    -4
                };
            };
            if matches!(action, Change::Unregister)
                && ((*reg::raw()).generation == u64::MAX
                    || (*reg::raw()).inventory_revision == u64::MAX)
            {
                return c::service_api_result_exhausted;
            }
            let retire = {
                let e = &*reg::entry(slot);
                match action {
                    Change::Begin(_) => {
                        if e.start_mode == MODE_DISABLED {
                            return c::service_api_result_disabled;
                        }
                        if matches!(e.state, STATE_RUNNING | STATE_STARTING | STATE_STOPPING) {
                            return c::service_api_result_busy;
                        }
                        if e.binding.id != 0 {
                            return c::service_api_result_busy;
                        }
                        true
                    }
                    Change::State(state, id, _, _) => state != STATE_RUNNING || e.instance_id != id,
                    Change::Stopping => {
                        if e.state == STATE_STOPPING {
                            return c::service_api_result_ok;
                        }
                        if !matches!(e.state, STATE_RUNNING | STATE_STARTING) {
                            return -1;
                        }
                        true
                    }
                    Change::Mode(mode) => {
                        if mode != MODE_DISABLED {
                            false
                        } else {
                            if e.instance_id != 0 || e.binding.id != 0 {
                                return -1;
                            }
                            true
                        }
                    }
                    Change::Unregister => {
                        if e.instance_id != 0
                            || e.binding.id != 0
                            || matches!(e.state, STATE_RUNNING | STATE_STARTING | STATE_STOPPING)
                        {
                            return c::service_api_result_busy;
                        }
                        true
                    }
                    Change::Finish(_, _, _) => true,
                    Change::ClearBinding => {
                        if e.instance_id != 0
                            || matches!(e.state, STATE_STARTING | STATE_STOPPING | STATE_RUNNING)
                        {
                            return c::service_api_result_busy;
                        }
                        false
                    }
                    Change::RestartCount => false,
                    Change::CancelStart => {
                        if e.state != STATE_STARTING
                            || e.instance_id != 0
                            || e.binding.id != 0
                            || e.start_generation == 0
                        {
                            return c::service_api_result_busy;
                        }
                        true
                    }
                    Change::Complete(_) => {
                        if e.binding.id == 0 {
                            return c::service_api_result_busy;
                        }
                        if e.instance_id == 0
                            && matches!(e.state, STATE_STOPPED | STATE_DISABLED | STATE_FAILED)
                        {
                            return c::service_api_result_ok;
                        }
                        true
                    }
                }
            };
            if retire && let Some(i) = reg::endpoint_for_slot(slot) {
                let ep = reg::endpoint(i);
                let Some(mut endpoint_guard) = reg::endpoint_lock(ep, 3, true) else {
                    let identity = reg::identity(ep);
                    guard.release();
                    if !reg::wait_lifecycle(identity) {
                        return admission_error;
                    }
                    continue;
                };
                endpoint::retire(ep, 3);
                let result = apply(key, slot, action);
                endpoint_guard.release();
                guard.release();
                reg::collect();
                return result;
            }
            let result = apply(key, slot, action);
            guard.release();
            reg::collect();
            return result;
        }
    }
}
pub(super) unsafe fn register(
    name: Bytes,
    path: Bytes,
    args: Bytes,
    mode: u8,
    description: Bytes,
    out_target: *mut ApiIndexTarget,
) -> i32 {
    unsafe {
        let mut value = Entry::EMPTY;
        let result = reg::make_entry(
            &mut value,
            reg::bytes(name),
            reg::bytes(path),
            reg::bytes(args),
            mode,
            reg::bytes(description),
        );
        if result < 0 {
            return result;
        }
        {
            let Some(_guard) = reg::lock_registry(0) else {
                return -1;
            };
            if reg::find_name(name).is_some() {
                return -3;
            }
            if (*reg::raw()).generation == u64::MAX
                || (*reg::raw()).inventory_revision == u64::MAX
                || (*reg::raw()).entries.serial == usize::MAX
            {
                return c::service_api_result_exhausted;
            }
        }
        reg::collect();
        let Some(mut prepared) = reg::prepare_entry(value) else {
            return c::service_api_result_memory;
        };
        let Some(mut guard) = reg::lock_registry(0) else {
            return -1;
        };
        if reg::find_name(name).is_some() {
            return -3;
        }
        if (*reg::raw()).generation == u64::MAX || (*reg::raw()).inventory_revision == u64::MAX {
            return c::service_api_result_exhausted;
        }
        let Some(slot) = (*reg::raw()).entries.insert(prepared.pointer()) else {
            return c::service_api_result_exhausted;
        };
        prepared.publish();
        reg::structure_changed();
        out_target.write(reg::target_for_slot(slot, NAMED_TARGET_INDEX));
        guard.release();
        0
    }
}
pub(super) unsafe fn running(name: Bytes, id: u32, ticks: u64) -> i32 {
    unsafe {
        let Some(_guard) = reg::lock_registry(0) else {
            return -1;
        };
        let Some(slot) = reg::find_name(name) else {
            return -4;
        };
        reg::running_in(slot, id, ticks)
    }
}
pub(super) unsafe fn restart_count(name: Bytes) -> i32 {
    unsafe {
        let Some(_guard) = reg::lock_registry(0) else {
            return -1;
        };
        let Some(slot) = reg::find_name(name) else {
            return -4;
        };
        let e = reg::entry(slot);
        (*e).restart_count = (*e).restart_count.wrapping_add(1);
        0
    }
}
unsafe fn start_entry(reservation: *const StartReservation) -> Option<usize> {
    unsafe {
        let slot = (*reservation).slot;
        let generation = (*reservation).generation;
        if !reg::entry_exists(slot) || generation == 0 {
            return None;
        }
        let e = &*reg::entry(slot);
        (e.used
            && e.start_generation == generation
            && e.start_mode != MODE_DISABLED
            && e.state == STATE_STARTING
            && e.instance_id == 0
            && e.binding.id == 0)
            .then_some(slot)
    }
}
pub(super) unsafe fn complete_start(
    reservation: *const StartReservation,
    binding: c::ProgramObservationHandle,
    ticks: u64,
) -> bool {
    unsafe {
        // Caller holds ProgramRegistry110 and has prevalidated/allocated the
        // completion lease. Never wait or allocate while it is held.
        let Some(_guard) = reg::try_lock_registry(0) else {
            return false;
        };
        if (*reservation).registry_generation != (*reg::raw()).generation {
            return false;
        }
        let Some(slot) = start_entry(reservation) else {
            return false;
        };
        if binding.id == 0
            || binding.process.instance_id == 0
            || binding.process.generation == 0
            || binding.process.reserved != 0
        {
            return false;
        }
        let e = reg::entry(slot);
        (*e).state = STATE_RUNNING;
        (*e).instance_id = binding.process.instance_id;
        (*e).binding = binding;
        (*e).start_tick = ticks;
        (*e).start_generation = 0;
        true
    }
}
pub(super) unsafe fn fail_start(
    reservation: *const StartReservation,
    code: i32,
    text: Bytes,
) -> bool {
    unsafe {
        let Some(_guard) = reg::lock_registry(0) else {
            return false;
        };
        if let Some(slot) = start_entry(reservation) {
            finish_entry(slot, true, code, text);
            true
        } else {
            false
        }
    }
}
pub(super) unsafe fn cancel_start(name: Bytes, generation: u64) {
    unsafe {
        let Some(_guard) = reg::lock_registry(0) else {
            return;
        };
        let Some(slot) = reg::find_name(name) else {
            return;
        };
        let e = reg::entry(slot);
        if generation == 0
            || (*e).start_generation != generation
            || (*e).state != STATE_STARTING
            || (*e).instance_id != 0
        {
            return;
        }
        finish_entry(
            slot,
            false,
            0,
            Bytes {
                data: ptr::null(),
                len: 0,
            },
        );
    }
}
pub(super) unsafe fn entry_at(index: usize) -> Option<Entry> {
    let _guard = reg::lock_registry(2)?;
    unsafe { reg::api_slot(u32::try_from(index).ok()?).map(|slot| reg::entry(slot).read()) }
}
pub(super) unsafe fn entry_by_name(name: Bytes) -> Option<Entry> {
    let _guard = reg::lock_registry(2)?;
    unsafe { reg::find_name(name).map(|slot| reg::entry(slot).read()) }
}
pub(super) unsafe fn target(index: u32, query: bool) -> Option<ApiIndexTarget> {
    let _guard = reg::lock_registry(2)?;
    unsafe { reg::select_target(index, query) }
}
pub(super) unsafe fn target_by_name(name: Bytes, out: *mut ApiIndexTarget) -> i32 {
    let Some(_guard) = reg::lock_registry(2) else {
        return c::service_api_result_busy;
    };
    unsafe {
        let Some(slot) = reg::find_name(name) else {
            return c::service_api_result_not_found;
        };
        out.write(reg::target_for_slot(slot, NAMED_TARGET_INDEX));
    }
    c::service_api_result_ok
}
pub(super) unsafe fn entry_for_target(target: &ApiIndexTarget) -> Option<Entry> {
    let _guard = reg::lock_registry(2)?;
    unsafe { reg::target_slot(target, true).map(|slot| reg::entry(slot).read()) }
}
pub(super) unsafe fn note_instance(target: &ApiIndexTarget) -> bool {
    unsafe {
        let Some(_guard) = reg::lock_registry(2) else {
            return false;
        };
        if reg::target_slot(target, true).is_none() {
            return false;
        }
        let p = &mut (*reg::raw()).enumeration;
        p.instance_lookups = p.instance_lookups.wrapping_add(1);
        true
    }
}
pub(super) fn count() -> usize {
    let Some(_guard) = reg::lock_registry(2) else {
        return 0;
    };
    unsafe { reg::count(&(*reg::raw()).entries) }
}
pub(super) unsafe fn fill_entry(slot: usize, out: &mut c::ServiceInfo, now: u64) {
    *out = c::ServiceInfo::default();
    if !reg::entry_exists(slot) {
        return;
    }
    unsafe {
        let e = &*reg::entry(slot);
        if !e.used {
            return;
        }
        out.state = u32::from(e.state);
        out.start_mode = u32::from(e.start_mode) + 1;
        out.instance_id = e.instance_id;
        out.exit_code = e.exit_code;
        out.restart_count = e.restart_count;
        out.start_tick = e.start_tick;
        if e.state == STATE_RUNNING && e.start_tick != 0 && now >= e.start_tick {
            out.uptime_ticks = now - e.start_tick;
        }
        out.name[..e.name_len].copy_from_slice(&e.name[..e.name_len]);
        out.last_error[..e.last_error_len].copy_from_slice(&e.last_error[..e.last_error_len]);
    }
}
unsafe fn fill_detail(slot: usize, out: &mut c::ServiceDetail, now: u64) -> bool {
    *out = c::ServiceDetail::default();
    if !reg::entry_exists(slot) {
        return false;
    }
    unsafe {
        let e = &*reg::entry(slot);
        if !e.used {
            return false;
        }
        if e.path_len >= out.path.len() {
            return false;
        }
        fill_entry(slot, &mut out.info, now);
        // Keep the original checked public128-byte detail window. A private
        // 1024-byte path never permits an unchecked copy into that ABI field.
        out.path[..e.path_len].copy_from_slice(&e.path[..e.path_len]);
        out.args[..e.args_len].copy_from_slice(&e.args[..e.args_len]);
        out.description[..e.description_len].copy_from_slice(&e.description[..e.description_len]);
        true
    }
}
pub(super) unsafe fn fill_endpoint(ep: *mut Endpoint, out: &mut c::ServiceInfo) {
    unsafe {
        let counts = endpoint::counts(ep);
        out.handle = (*ep).handle;
        out.flags |= c::service_api_flag_endpoint | c::service_api_flag_queue_backed;
        if counts.queued != 0 || counts.delivered != 0 {
            out.flags |= c::service_api_flag_request_pending;
        }
        if counts.responded != 0 {
            out.flags |= c::service_api_flag_response_pending;
        }
        out.requests = (*ep).requests;
        out.responses = (*ep).responses;
        out.drops = (*ep).drops;
        out.queue_depth = DEPTH as u32;
        out.queue_used = counts.used;
        out.queue_high_water = (*ep).queue_high_water;
        out.active_workers = counts.delivered;
        out.max_active_workers = (*ep).max_active_workers;
        out.open_handles = (*ep).open_handles;
        out.busy_rejections = (*ep).busy_rejections;
        out.timeouts = (*ep).timeouts;
        out.cancellations = (*ep).cancellations;
    }
}
pub(super) unsafe fn append_endpoint(identity: Option<Identity>, out: *mut c::ServiceInfo) {
    if let Some(id) = identity
        && let Some(lease) = reg::lock_identity(id, 6)
    {
        unsafe { fill_endpoint(lease.endpoint, &mut *out) };
    }
}
pub(super) unsafe fn info(
    operation: u8,
    index: u32,
    name: Bytes,
    target: *const ApiIndexTarget,
    out: *mut c::ServiceInfo,
    now: u64,
) -> i32 {
    unsafe {
        let opening = matches!(operation, 3 | 4);
        let Some(mut guard) = reg::lock_registry(if opening { 1 } else { 2 }) else {
            return c::service_api_result_busy;
        };
        out.write(c::ServiceInfo::default());
        let selected = match operation {
            0 => reg::api_slot(index),
            1 | 4 => reg::target_slot(&*target, true),
            2 | 3 => reg::find_name(name),
            _ => unreachable!(),
        };
        let Some(slot) = selected else {
            return match operation {
                0 => 0,
                1 | 4 => c::service_api_result_busy,
                _ => c::service_api_result_not_found,
            };
        };
        fill_entry(slot, &mut *out, now);
        if opening {
            let e = reg::entry(slot);
            if (*e).state != STATE_RUNNING || (*e).instance_id == 0 {
                return c::service_api_result_not_running;
            }
        }
        let identity = reg::endpoint_for_slot(slot).map(|i| reg::identity(reg::endpoint(i)));
        if opening && identity.is_none() {
            return c::service_api_result_no_endpoint;
        }
        guard.release();
        if opening {
            let Some(lease) = reg::lock_identity(identity.expect("Admitted service endpoint"), 4)
            else {
                return c::service_api_result_not_running;
            };
            (*lease.endpoint).open_handles = (*lease.endpoint).open_handles.wrapping_add(1);
            fill_endpoint(lease.endpoint, &mut *out);
        } else {
            append_endpoint(identity, out);
        }
        if matches!(operation, 0 | 1) {
            1
        } else {
            c::service_api_result_ok
        }
    }
}
pub(super) unsafe fn detail(
    operation: u8,
    index: u32,
    name: Bytes,
    target: *const ApiIndexTarget,
    out: *mut c::ServiceDetail,
    now: u64,
) -> i32 {
    unsafe {
        let Some(mut guard) = reg::lock_registry(2) else {
            return c::service_api_result_busy;
        };
        out.write(c::ServiceDetail::default());
        let selected = match operation {
            0 => reg::api_slot(index),
            1 => reg::target_slot(&*target, true),
            2 => reg::find_name(name),
            _ => unreachable!(),
        };
        let Some(slot) = selected else {
            return match operation {
                0 => 0,
                1 => c::service_api_result_busy,
                _ => c::service_api_result_not_found,
            };
        };
        if !fill_detail(slot, &mut *out, now) {
            return c::service_api_result_buffer_too_small;
        }
        let identity = reg::endpoint_for_slot(slot).map(|i| reg::identity(reg::endpoint(i)));
        guard.release();
        append_endpoint(identity, ptr::addr_of_mut!((*out).info));
        if matches!(operation, 0 | 1) {
            1
        } else {
            c::service_api_result_ok
        }
    }
}
pub(super) unsafe fn register_endpoint(
    name: Bytes,
    process: c::ProgramProcessHandle,
    flags: u32,
    out: *mut c::ServiceInfo,
    now: u64,
) -> i32 {
    unsafe {
        if process.instance_id == 0 || process.generation == 0 || process.reserved != 0 {
            return c::service_api_result_invalid;
        }
        let mut prepared = None;
        loop {
            let Some(mut guard) = reg::lock_registry(0) else {
                return c::service_api_result_busy;
            };
            out.write(c::ServiceInfo::default());
            let Some(slot) = reg::find_name(name) else {
                return c::service_api_result_not_found;
            };
            fill_entry(slot, &mut *out, now);
            let e = reg::entry(slot);
            if (*e).state != STATE_RUNNING
                || (*e).instance_id != process.instance_id
                || (*e).binding.process.instance_id != process.instance_id
                || (*e).binding.process.generation != process.generation
                || (*e).binding.process.reserved != process.reserved
                || (*e).binding.id == 0
            {
                return c::service_api_result_not_running;
            }
            let ep = if let Some(i) = reg::endpoint_for_slot(slot) {
                reg::endpoint(i)
            } else {
                if prepared.is_none() {
                    guard.release();
                    prepared = Some(match reg::prepare_endpoint() {
                        Some(value) => value,
                        None => return c::service_api_result_memory,
                    });
                    continue;
                }
                let allocation = prepared.as_mut().expect("Prepared service endpoint");
                let node = allocation.pointer();
                let r = reg::raw();
                (*node).endpoint.service_slot = slot;
                if (*r).endpoints.insert(node).is_none() {
                    return c::service_api_result_exhausted;
                }
                allocation.publish();
                node.cast::<Endpoint>()
            };
            let Some(mut endpoint_guard) = reg::endpoint_lock(ep, 3, true) else {
                let identity = reg::identity(ep);
                guard.release();
                if !reg::wait_lifecycle(identity) {
                    return c::service_api_result_busy;
                }
                continue;
            };
            (*ep).flags = flags;
            fill_endpoint(ep, &mut *out);
            endpoint_guard.release();
            guard.release();
            return c::service_api_result_ok;
        }
    }
}
pub(super) unsafe fn unregister_endpoint(handle: u32) -> i32 {
    unsafe {
        loop {
            let Some(mut guard) = reg::lock_registry(0) else {
                return c::service_api_result_busy;
            };
            let Some(i) = reg::endpoint_for_handle(handle) else {
                return c::service_api_result_bad_handle;
            };
            let ep = reg::endpoint(i);
            let Some(mut endpoint_guard) = reg::endpoint_lock(ep, 3, true) else {
                let identity = reg::identity(ep);
                guard.release();
                if !reg::wait_lifecycle(identity) {
                    return c::service_api_result_busy;
                }
                continue;
            };
            endpoint::retire(ep, 3);
            endpoint_guard.release();
            guard.release();
            reg::collect();
            return c::service_api_result_ok;
        }
    }
}
pub(super) unsafe fn close(handle: u32) -> i32 {
    unsafe {
        let Some(lease) = reg::lock_handle(handle, 4) else {
            return c::service_api_result_bad_handle;
        };
        if (*lease.endpoint).open_handles != 0 {
            (*lease.endpoint).open_handles -= 1;
        }
        c::service_api_result_ok
    }
}
