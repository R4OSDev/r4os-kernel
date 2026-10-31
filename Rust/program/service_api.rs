//! R4X service admission, stable-index refresh and exact-generation lifecycle.
//! The service registry/endpoint state stays with its separately assigned owner.
//! No program registry guard or record borrow crosses a blocking peer operation.
use super::{
    context, exit_state, launch, observations, payload_types, process, process_data, program_api,
    registry, util,
};
use crate::{
    contract as c,
    fs::{
        drive::Drive,
        vfs,
        vfs_types::{Bytes, Volume},
    },
    platform::timer,
    sched::{scheduler, sync_types::WaitQueue, sync_wait, task_context},
};
use core::{
    mem::{self, MaybeUninit},
    ptr, slice,
};
const INVALID: i32 = c::service_api_result_invalid;
const NOT_FOUND: i32 = c::service_api_result_not_found;
const BUSY: i32 = c::service_api_result_busy;
const STOPPED: u8 = 1;
const STARTING: u8 = 2;
const RUNNING: u8 = 3;
const STOPPING: u8 = 4;
const FAILED: u8 = 5;
const DISABLED: u8 = 6;
const MODE_DISABLED: u8 = 2;
#[repr(C)]
pub struct Entry {
    id: u32,
    state: u8,
    mode: u8,
    reserved: u16,
    start_generation: u64,
    name_len: usize,
    path_len: usize,
    args_len: usize,
    name: [u8; 32],
    path: [u8; 1024],
    args: [u8; 96],
    binding: c::ProgramObservationHandle,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Target {
    registry_generation: u64,
    slot: usize,
    name_len: usize,
    index: u32,
    id: u32,
    state: u32,
    reserved: u32,
    name: [u8; 32],
    start_generation: u64,
    binding: c::ProgramObservationHandle,
}
#[repr(C)]
#[derive(Default)]
pub struct Reservation {
    slot: usize,
    generation: u64,
    registry_generation: u64,
    binding: c::ProgramObservationHandle,
}

fn span(s: &[u8]) -> Bytes {
    Bytes {
        data: s.as_ptr(),
        len: s.len(),
    }
}
unsafe fn target_entry(target: &Target) -> Option<Entry> {
    unsafe {
        let mut out = MaybeUninit::uninit();
        if program_service_target_entry(target, out.as_mut_ptr()) {
            Some(out.assume_init())
        } else {
            None
        }
    }
}
unsafe fn zcopy<const N: usize>(
    input: *const u8,
    output: &mut [MaybeUninit<u8>; N],
) -> Option<&[u8]> {
    unsafe {
        let mut len = 0;
        let data = output.as_mut_ptr().cast::<u8>();
        if process_data::r4os_rust_program_data_copy_z(input, data, N, &mut len) {
            Some(slice::from_raw_parts(data, len))
        } else {
            None
        }
    }
}
fn mode(raw: u32) -> Option<u8> {
    match raw {
        c::service_start_manual => Some(0),
        c::service_start_auto => Some(1),
        c::service_start_disabled => Some(MODE_DISABLED),
        _ => None,
    }
}
fn registry_result(result: i32) -> i32 {
    match result {
        0 => 0,
        -1 => INVALID,
        -2 => c::service_api_result_full,
        -3 => c::service_api_result_duplicate,
        -4 => NOT_FOUND,
        _ => INVALID,
    }
}
unsafe fn boot_caller() -> bool {
    unsafe {
        if !crate::display::bootscreen::is_active() {
            return false;
        }
        let state = exit_state::state();
        if !(*state).foreground_present {
            return false;
        }
        let instance = context::current();
        !instance.is_null() && (*instance).id == (*state).foreground_id
    }
}
unsafe fn report(op: u8, name: &[u8], stage: &[u8]) {
    unsafe {
        if boot_caller() {
            program_service_boot(op, span(name), span(stage));
        }
    }
}
unsafe fn mark_target(op: u8, target: &Target, code: i32, text: &[u8]) -> bool {
    unsafe { program_service_target_mutate(op, target, code, span(text)) == 0 }
}
unsafe fn named_target(name: &[u8]) -> Result<Target, i32> {
    let mut raw = st::ApiIndexTarget::EMPTY;
    let result = unsafe { sf::service_target_by_name(span(name), &mut raw) };
    if result == 0 {
        Ok(target_value(raw))
    } else {
        Err(result)
    }
}

fn same_binding(a: c::ProgramObservationHandle, b: c::ProgramObservationHandle) -> bool {
    a.id == b.id && registry::equal(a.process, b.process)
}

/// Unlocked work always uses this binding's exact retained process. Neither
/// a diagnostic history row nor logical callback return can finish a service.
unsafe fn refresh_target(target: &mut Target) -> bool {
    unsafe {
        let Some(entry) = target_entry(target) else {
            return false;
        };
        target.id = entry.id;
        target.state = u32::from(entry.state);
        if entry.id == 0 {
            return true;
        }
        if entry.binding.id == 0 {
            return false;
        }
        if target.index != st::NAMED_TARGET_INDEX && !program_service_target_note(target) {
            return false;
        }
        let mut completion = c::ProgramProcessCompletion::default();
        let mut close_requested = false;
        let observed =
            observations::service_state(entry.binding, &mut completion, &mut close_requested);
        if observed == 0 {
            if !registry::equal(completion.handle, entry.binding.process)
                || completion.app_class != 2
            {
                return false;
            }
            // The managed service is the existing completion owner. Consume
            // only this exact physical ACK so its legacy ID becomes reusable;
            // the private observation keeps detached terminal metadata alive.
            let mut consumed = c::ProgramProcessCompletion::default();
            match process::r4os_rust_program_process_reap(&entry.binding.process, &mut consumed) {
                c::program_handle_ok => {
                    if !registry::equal(consumed.handle, completion.handle)
                        || consumed.app_class != completion.app_class
                        || consumed.exit_code != completion.exit_code
                    {
                        return false;
                    }
                }
                // Another observer may already have consumed the same ACK.
                // The retained record above still proves this generation.
                c::program_handle_error_not_found | c::program_handle_error_stale => {}
                _ => return false,
            }
            if !mark_target(9, target, completion.exit_code, b"") {
                return false;
            }
        } else if observed == -5
            && close_requested
            && entry.state != STOPPING
            && !mark_target(0, target, 0, b"")
        {
            return false;
        }
        // Missing/pending evidence remains unknown. A same-binding terminal
        // transition by another observer can be adopted; a newer lease cannot.
        let Some(current) = target_entry(target) else {
            return false;
        };
        target.id = current.id;
        target.state = u32::from(current.state);
        target.start_generation = current.start_generation;
        true
    }
}

unsafe fn refreshed_target(name: &[u8]) -> Result<Target, i32> {
    unsafe {
        let mut target = named_target(name)?;
        if refresh_target(&mut target) {
            Ok(target)
        } else {
            Err(BUSY)
        }
    }
}

unsafe fn bound_status(target: &Target, out: *mut c::ServiceInfo) -> i32 {
    unsafe {
        out.write(c::ServiceInfo::default());
        let result = program_service_info(2, span(b""), target, out, timer::tick_count());
        if result == 1 { 0 } else { result }
    }
}

/// Release detached terminal evidence with no service/endpoint owner held.
/// If clear loses admission after release, the terminal slot keeps the closed
/// token: a subsequent attempt can observe -2 and finish the exact clear.
unsafe fn clear_terminal_binding(target: &mut Target) -> i32 {
    unsafe {
        let Some(entry) = target_entry(target) else {
            return BUSY;
        };
        if entry.id != 0 || matches!(entry.state, RUNNING | STARTING | STOPPING) {
            return BUSY;
        }
        if entry.binding.id != 0 {
            let released = observations::release_service(entry.binding);
            if released != 0 && released != -2 {
                return BUSY;
            }
            if !mark_target(4, target, 0, b"") {
                return BUSY;
            }
            target.binding = st::EMPTY_BINDING;
        }
        target.id = 0;
        target.state = u32::from(entry.state);
        target.start_generation = entry.start_generation;
        0
    }
}
struct Registry;
impl Drop for Registry {
    fn drop(&mut self) {
        registry::unlock();
    }
}
unsafe fn descends(handle: c::ProgramProcessHandle) -> bool {
    unsafe {
        if !registry::valid(handle) {
            return false;
        }
        let Some(caller) = context::handle() else {
            return false;
        };
        if registry::equal(caller, handle) {
            return true;
        }
        if !registry::lock() {
            return false;
        }
        let _guard = Registry;
        let mut current = caller;
        while registry::valid(current) {
            if registry::equal(current, handle) {
                return true;
            }
            let mut parent = c::ProgramProcessHandle::default();
            let slot = registry::lookup_handle(current, true);
            if !slot.is_null() {
                let console = (*slot)
                    .instance
                    .console_payload
                    .cast::<payload_types::Console>();
                if !console.is_null() {
                    parent.instance_id = (*console).io_target_id;
                    parent.generation = (*console).io_target_generation;
                }
            }
            if !registry::valid(parent) {
                let node = registry::completion_handle(current);
                if node.is_null() || !(*node).owner {
                    return false;
                }
                parent = (*node).owner_handle;
            }
            if !registry::valid(parent) || parent.generation >= current.generation {
                return false;
            }
            current = parent;
        }
        false
    }
}
unsafe fn resolve(path: &[u8], output: *mut u8) -> Option<program_api::Target> {
    unsafe {
        let target = program_api::resolve(span(path), output)?;
        let mut volume = MaybeUninit::<Volume>::uninit();
        if !vfs::r4os_rust_vfs_mount(
            4,
            (*target.drive).letter,
            ptr::null(),
            ptr::null(),
            volume.as_mut_ptr().cast(),
        ) {
            return None;
        }
        if launch::r4os_rust_program_launch_path(
            3,
            target.drive,
            target.path,
            span(b""),
            ptr::null_mut(),
            0,
            ptr::null_mut(),
        ) != 3
        {
            return None;
        }
        Some(target)
    }
}
unsafe fn dos_path<'a>(drive: *mut Drive, path: &[u8], output: &'a mut [u8]) -> Option<&'a [u8]> {
    unsafe {
        if output.len() < 3 {
            return None;
        }
        output[0] = (*drive).letter.to_ascii_uppercase();
        output[1] = b':';
        let mut len = 2;
        if path.is_empty() || !matches!(path[0], b'\\' | b'/') {
            if len >= output.len() {
                return None;
            }
            output[len] = b'\\';
            len += 1;
        }
        if len + path.len() > output.len() {
            return None;
        }
        for (i, &value) in path.iter().enumerate() {
            output[len + i] = if value == b'/' { b'\\' } else { value };
        }
        len += path.len();
        Some(&output[..len])
    }
}
unsafe fn start(name: &[u8], out: *mut c::ServiceInfo) -> i32 {
    unsafe {
        let Some(_call) = task_context::Admission::acquire(false) else {
            return BUSY;
        };
        out.write(c::ServiceInfo::default());
        let mut target = match named_target(name) {
            Ok(t) => t,
            Err(e) => return e,
        };
        start_bound(&mut target, out)
    }
}

unsafe fn started_target(name: &[u8], identity: &Reservation) -> Result<Target, i32> {
    let target = unsafe { named_target(name)? };
    if target.registry_generation != identity.registry_generation
        || target.slot != identity.slot
        || identity.binding.id == 0
        || !same_binding(target.binding, identity.binding)
    {
        Err(BUSY)
    } else {
        Ok(target)
    }
}

unsafe fn fail_reservation(
    target: &mut Target,
    identity: &Reservation,
    code: i32,
    text: &[u8],
) -> bool {
    if !unsafe { program_service_fail(identity, code, span(text)) } {
        return false;
    }
    target.id = 0;
    target.state = u32::from(FAILED);
    target.start_generation = 0;
    true
}

unsafe fn start_bound(target: &mut Target, out: *mut c::ServiceInfo) -> i32 {
    unsafe {
        let Some(_call) = task_context::Admission::acquire(false) else {
            return BUSY;
        };
        out.write(c::ServiceInfo::default());
        if !refresh_target(target) {
            return BUSY;
        }
        let Some(current) = target_entry(target) else {
            return BUSY;
        };
        if current.state == RUNNING {
            return bound_status(target, out);
        }
        let clear = clear_terminal_binding(target);
        if clear != 0 {
            return clear;
        }
        report(0, &target.name[..target.name_len], b"");
        let mut identity = Reservation::default();
        let mut raw = MaybeUninit::<Entry>::uninit();
        let reserved = program_service_begin(target, &mut identity, raw.as_mut_ptr());
        if reserved != 0 {
            bound_status(target, out);
            return reserved;
        }
        target.start_generation = identity.generation;
        target.state = u32::from(STARTING);
        let entry = raw.assume_init();
        let name = &entry.name[..entry.name_len];
        report(1, name, b"Pfad pruefen");
        let mut path = [MaybeUninit::<u8>::uninit(); 1024];
        let Some(resolved) = resolve(&entry.path[..entry.path_len], path.as_mut_ptr().cast())
        else {
            if !fail_reservation(target, &identity, -1, b"path invalid or not service R4X") {
                return BUSY;
            }
            bound_status(target, out);
            report(3, name, b"");
            return c::service_api_result_bad_path;
        };
        report(1, name, b"laden");
        let mut handle = c::ProgramProcessHandle::default();
        let launched = launch::service_path(
            resolved.drive,
            resolved.path,
            span(&entry.args[..entry.args_len]),
            resolved.drive,
            &mut handle,
            &mut identity,
        );
        if launched != 0 {
            if identity.binding.id == 0 {
                if !fail_reservation(target, &identity, launched, b"spawn failed") {
                    return BUSY;
                }
                bound_status(target, out);
            } else {
                // Publication failure may still be physically retiring. Keep
                // its binding until the actual retained resource ACK arrives.
                *target = match started_target(name, &identity) {
                    Ok(t) => t,
                    Err(e) => return e,
                };
                if !refresh_target(target) {
                    return BUSY;
                }
                bound_status(target, out);
            }
            report(3, name, b"");
            return launched;
        }
        *target = match started_target(name, &identity) {
            Ok(t) => t,
            Err(e) => return e,
        };
        if !refresh_target(target) {
            return BUSY;
        }
        let result = bound_status(target, out);
        if result != 0 {
            report(3, name, b"");
        }
        result
    }
}

fn stop_deadline(timeout: u64) -> Option<u64> {
    (timeout != c::io_wait_forever).then(|| timer::tick_count().saturating_add(timeout))
}

unsafe fn stop(name: &[u8], out: *mut c::ServiceInfo, timeout: u64) -> i32 {
    // Preserve69: one absolute entry budget includes owner/status/Close work.
    let deadline = stop_deadline(timeout);
    unsafe {
        let Some(_call) = task_context::Admission::acquire(false) else {
            return BUSY;
        };
        out.write(c::ServiceInfo::default());
        let mut target = match named_target(name) {
            Ok(t) => t,
            Err(e) => return e,
        };
        stop_bound(&mut target, out, deadline)
    }
}

/// # Safety
/// Resident name, valid aligned expected handle and disjoint writable status
/// through this call. A changed binding returns BUSY before requesting Close.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_service_stop_bound(
    name: *const u8,
    expected: *const c::ProgramProcessHandle,
    out: *mut c::ServiceInfo,
    timeout: u64,
) -> i32 {
    let deadline = stop_deadline(timeout);
    unsafe {
        if expected.is_null() || out.is_null() {
            return INVALID;
        }
        let expected = expected.read();
        if !registry::valid(expected) {
            return INVALID;
        }
        let Some(_call) = task_context::Admission::acquire(false) else {
            return BUSY;
        };
        out.write(c::ServiceInfo::default());
        let mut buffer = [MaybeUninit::uninit(); 32];
        let Some(name) = zcopy(name, &mut buffer) else {
            return INVALID;
        };
        let mut target = match named_target(name) {
            Ok(t) => t,
            Err(e) => return e,
        };
        if !registry::equal(target.binding.process, expected) {
            return BUSY;
        }
        stop_bound(&mut target, out, deadline)
    }
}

unsafe fn stop_bound(target: &mut Target, out: *mut c::ServiceInfo, deadline: Option<u64>) -> i32 {
    unsafe {
        if !refresh_target(target) {
            return BUSY;
        }
        let Some(entry) = target_entry(target) else {
            return BUSY;
        };
        if entry.state == STARTING && entry.id == 0 {
            if !mark_target(8, target, 0, b"") {
                return BUSY;
            }
            target.start_generation = 0;
            target.state = u32::from(if entry.mode == MODE_DISABLED {
                DISABLED
            } else {
                STOPPED
            });
            return bound_status(target, out);
        }
        if entry.id == 0 && matches!(entry.state, DISABLED | STOPPED | FAILED) {
            return bound_status(target, out);
        }
        if entry.binding.id == 0 {
            return BUSY;
        }
        if descends(entry.binding.process) {
            return c::service_api_result_self_restart;
        }
        if !mark_target(0, target, 0, b"") {
            return BUSY;
        }
        target.state = u32::from(STOPPING);
        let close = process::r4os_rust_program_process_close(&entry.binding.process);
        if !refresh_target(target) {
            return BUSY;
        }
        let Some(after) = target_entry(target) else {
            return BUSY;
        };
        if after.id == 0 && matches!(after.state, STOPPED | DISABLED) {
            return bound_status(target, out);
        }
        if close != 0 && close != c::program_handle_error_not_running {
            bound_status(target, out);
            return if close == c::program_handle_error_would_block {
                BUSY
            } else {
                c::service_api_result_stop_failed
            };
        }
        loop {
            if !refresh_target(target) {
                return BUSY;
            }
            let Some(current) = target_entry(target) else {
                return BUSY;
            };
            if current.id == 0 && matches!(current.state, STOPPED | DISABLED) {
                return bound_status(target, out);
            }
            if current.id == 0 && current.state == FAILED {
                bound_status(target, out);
                return c::service_api_result_stop_failed;
            }
            // Actual retained completion wins the boundary. Unknown evidence
            // never becomes completion merely because the wait budget ended.
            if deadline.is_some_and(|end| timer::tick_count() >= end) {
                let status = bound_status(target, out);
                return if status == 0 {
                    c::service_api_result_timeout
                } else {
                    status
                };
            }
            let mut queue = WaitQueue::default();
            sync_wait::r4os_rust_sync_timer(&mut queue, 1, b"service-stop".as_ptr(), 12);
        }
    }
}
/// # Safety
/// Original resident ServiceInfo output and stable index; no lock spans refresh.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_service_info(
    index: u32,
    out: *mut c::ServiceInfo,
) -> i32 {
    unsafe {
        if out.is_null() {
            return INVALID;
        }
        let Some(_call) = task_context::Admission::acquire(false) else {
            return BUSY;
        };
        for attempt in 0..3 {
            let mut target = MaybeUninit::uninit();
            if !program_service_index(attempt != 0, index, target.as_mut_ptr()) {
                report(2, b"", b"");
                out.write(c::ServiceInfo::default());
                return 0;
            }
            let mut target = target.assume_init();
            if !refresh_target(&mut target) {
                continue;
            }
            let result = program_service_info(2, span(b""), &target, out, timer::tick_count());
            if result != BUSY {
                return result;
            }
        }
        out.write(c::ServiceInfo::default());
        BUSY
    }
}
/// # Safety
/// Same original canonical output/refresh contract as ServiceInfo.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_service_detail(
    index: u32,
    out: *mut c::ServiceDetail,
) -> i32 {
    unsafe {
        if out.is_null() {
            return INVALID;
        }
        let Some(_call) = task_context::Admission::acquire(false) else {
            return BUSY;
        };
        for attempt in 0..3 {
            let mut target = MaybeUninit::uninit();
            if !program_service_index(attempt != 0, index, target.as_mut_ptr()) {
                report(2, b"", b"");
                out.write(c::ServiceDetail::default());
                return 0;
            }
            let mut target = target.assume_init();
            if !refresh_target(&mut target) {
                continue;
            }
            let result =
                program_service_detail(false, span(b""), &target, out, timer::tick_count());
            if result != BUSY {
                return result;
            }
        }
        out.write(c::ServiceDetail::default());
        BUSY
    }
}
/// # Safety
/// Canonical input NUL string/output, original NULL/validation order and lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_service_named(
    op: u8,
    name: *const u8,
    out: *mut c::ServiceInfo,
    mode_raw: u32,
    timeout: u64,
) -> i32 {
    unsafe {
        if op != 7 && out.is_null() {
            return INVALID;
        }
        let Some(_call) = task_context::Admission::acquire(false) else {
            return BUSY;
        };
        let mode = if op == 6 {
            let Some(mode) = mode(mode_raw) else {
                return INVALID;
            };
            mode
        } else {
            0
        };
        let mut buffer = [MaybeUninit::uninit(); 32];
        let Some(name) = zcopy(name, &mut buffer) else {
            return INVALID;
        };
        if op != 7 {
            out.write(c::ServiceInfo::default());
        }
        match op {
            0 | 1 => {
                let target = match refreshed_target(name) {
                    Ok(t) => t,
                    Err(e) => return e,
                };
                if op == 0 {
                    bound_status(&target, out)
                } else {
                    program_service_info(3, span(b""), &target, out, timer::tick_count())
                }
            }
            2 => start(name, out),
            3 => stop(name, out, timeout),
            4 => {
                let mut target = match refreshed_target(name) {
                    Ok(t) => t,
                    Err(e) => return e,
                };
                let Some(entry) = target_entry(&target) else {
                    return BUSY;
                };
                if entry.id != 0 && descends(entry.binding.process) {
                    return c::service_api_result_self_restart;
                }
                if matches!(entry.state, RUNNING | STARTING | STOPPING) {
                    let result = stop_bound(&mut target, out, stop_deadline(40));
                    if result < 0 {
                        return result;
                    }
                }
                let clear = clear_terminal_binding(&mut target);
                if clear != 0 {
                    return clear;
                }
                if !mark_target(5, &target, 0, b"") {
                    return BUSY;
                }
                start_bound(&mut target, out)
            }
            6 => {
                let mut target = match refreshed_target(name) {
                    Ok(t) => t,
                    Err(e) => return e,
                };
                let Some(entry) = target_entry(&target) else {
                    return BUSY;
                };
                if mode == MODE_DISABLED && entry.id != 0 && descends(entry.binding.process) {
                    return c::service_api_result_self_restart;
                }
                if mode == MODE_DISABLED && matches!(entry.state, RUNNING | STARTING | STOPPING) {
                    let result = stop_bound(&mut target, out, stop_deadline(40));
                    if result < 0 {
                        return result;
                    }
                }
                if mode == MODE_DISABLED {
                    let clear = clear_terminal_binding(&mut target);
                    if clear != 0 {
                        return clear;
                    }
                }
                if !mark_target(6, &target, i32::from(mode), b"") {
                    return BUSY;
                }
                bound_status(&target, out)
            }
            7 => {
                let mut target = match refreshed_target(name) {
                    Ok(t) => t,
                    Err(e) => return e,
                };
                let Some(entry) = target_entry(&target) else {
                    return BUSY;
                };
                if matches!(entry.state, RUNNING | STARTING | STOPPING) {
                    return c::service_api_result_running;
                }
                let clear = clear_terminal_binding(&mut target);
                if clear != 0 {
                    return clear;
                }
                if mark_target(7, &target, 0, b"") {
                    0
                } else {
                    BUSY
                }
            }
            _ => unreachable!(),
        }
    }
}

unsafe fn bound_detail(
    name: &[u8],
    out: *mut c::ServiceDetail,
    process: *mut c::ProgramProcessHandle,
) -> i32 {
    unsafe {
        out.write(c::ServiceDetail::default());
        if !process.is_null() {
            process.write(c::ProgramProcessHandle::default());
        }
        let target = match refreshed_target(name) {
            Ok(t) => t,
            Err(e) => return e,
        };
        let result = program_service_detail(false, span(b""), &target, out, timer::tick_count());
        if result != 1 {
            return result;
        }
        if target_entry(&target).is_none() {
            out.write(c::ServiceDetail::default());
            return BUSY;
        }
        // A terminal service retains its last bound identity until next start
        // or removal. This is a borrowed identity value, not an app-owned lease.
        if !process.is_null() {
            process.write(target.binding.process);
        }
        0
    }
}
/// # Safety
/// Resident original name/detail output through the exact refresh transaction.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_service_detail_name(
    name: *const u8,
    out: *mut c::ServiceDetail,
) -> i32 {
    unsafe {
        if out.is_null() {
            return INVALID;
        }
        let Some(_call) = task_context::Admission::acquire(false) else {
            return BUSY;
        };
        let mut buffer = [MaybeUninit::uninit(); 32];
        let Some(name) = zcopy(name, &mut buffer) else {
            return INVALID;
        };
        bound_detail(name, out, ptr::null_mut())
    }
}
/// # Safety
/// Resident NUL name and disjoint aligned detail/process outputs through the
/// call. Both output values refer to the same validated service generation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_service_bound_detail(
    name: *const u8,
    out: *mut c::ServiceDetail,
    process: *mut c::ProgramProcessHandle,
) -> i32 {
    unsafe {
        if out.is_null() || process.is_null() {
            return INVALID;
        }
        let Some(_call) = task_context::Admission::acquire(false) else {
            return BUSY;
        };
        let mut buffer = [MaybeUninit::uninit(); 32];
        let Some(name) = zcopy(name, &mut buffer) else {
            return INVALID;
        };
        bound_detail(name, out, process)
    }
}
/// # Safety
/// Original resident strings/output and caller execution throughout file waits.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_service_install(
    name: *const u8,
    path: *const u8,
    args: *const u8,
    mode_raw: u32,
    description: *const u8,
    out: *mut c::ServiceInfo,
) -> i32 {
    unsafe {
        if out.is_null() {
            return INVALID;
        }
        let Some(_call) = task_context::Admission::acquire(false) else {
            return BUSY;
        };
        let Some(mode) = mode(mode_raw) else {
            return INVALID;
        };
        let mut names = [MaybeUninit::uninit(); 32];
        let mut paths = [MaybeUninit::uninit(); 1024];
        let mut argv = [MaybeUninit::uninit(); 96];
        let mut descriptions = [MaybeUninit::uninit(); 80];
        let Some(name) = zcopy(name, &mut names) else {
            return INVALID;
        };
        report(0, name, b"");
        let Some(path) = zcopy(path, &mut paths) else {
            report(3, name, b"");
            return INVALID;
        };
        let Some(args) = zcopy(args, &mut argv) else {
            report(3, name, b"");
            return INVALID;
        };
        let Some(description) = zcopy(description, &mut descriptions) else {
            report(3, name, b"");
            return INVALID;
        };
        let mut resolved = [MaybeUninit::<u8>::uninit(); 1024];
        let Some(target) = resolve(path, resolved.as_mut_ptr().cast()) else {
            report(3, name, b"");
            return c::service_api_result_bad_path;
        };
        let mut full = [0; 1024];
        let Some(full) = dos_path(target.drive, util::bytes(target.path), &mut full) else {
            report(3, name, b"");
            return c::service_api_result_bad_path;
        };
        let mut installed = MaybeUninit::<Target>::uninit();
        let result = crate::program::service_api::native_service_install(
            span(name),
            span(full),
            span(args),
            mode,
            span(description),
            installed.as_mut_ptr(),
        );
        if result < 0 {
            report(3, name, b"");
            return registry_result(result);
        }
        let status = bound_status(&installed.assume_init(), out);
        if status != 0 {
            report(3, name, b"");
        }
        status
    }
}
/// # Safety
/// Original canonical name/info buffers and current service instance lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_service_register(
    name: *const u8,
    flags: u32,
    out: *mut c::ServiceInfo,
) -> i32 {
    unsafe {
        if out.is_null() {
            return INVALID;
        }
        let Some(_call) = task_context::Admission::acquire(false) else {
            return BUSY;
        };
        let instance = context::current();
        if instance.is_null() || (*instance).app_class != 2 {
            return INVALID;
        }
        let Some(handle) = context::handle() else {
            return INVALID;
        };
        let mut buffer = [MaybeUninit::uninit(); 32];
        let Some(name) = zcopy(name, &mut buffer) else {
            return INVALID;
        };
        program_service_endpoint_register(span(name), handle, flags, out, timer::tick_count())
    }
}
/// # Safety
/// Exact endpoint handle retains the original lower-owner admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_service_endpoint(
    op: u8,
    handle: u32,
    timeout: u64,
) -> i32 {
    unsafe { program_service_endpoint_control(op, handle, timeout) }
}
/// # Safety
/// Resident header and byte span retain the original endpoint validation order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_service_recv(
    handle: u32,
    header: *mut c::ServiceMessageHeader,
    out: *mut u8,
    capacity: u32,
) -> i32 {
    unsafe {
        if header.is_null() || capacity > 4096 || out.is_null() {
            return INVALID;
        }
        program_service_endpoint_recv(handle, header, out, capacity as usize)
    }
}
/// # Safety
/// The exact payload is readable until the existing reply operation completes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_service_reply(
    handle: u32,
    id: u32,
    status: i32,
    payload: *const u8,
    len: u32,
) -> i32 {
    unsafe {
        if len > 4096 {
            return c::service_api_result_payload_too_large;
        }
        if len != 0 && payload.is_null() {
            return INVALID;
        }
        let bytes = if len == 0 {
            span(b"")
        } else {
            Bytes {
                data: payload,
                len: len as usize,
            }
        };
        let result = program_service_endpoint_reply(handle, id, status, bytes);
        scheduler::safe_reschedule();
        result
    }
}
/// # Safety
/// Internal bounded name/output; same unwind, reservation and generation contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_service_lifecycle(
    stop_request: bool,
    name: Bytes,
    out: *mut c::ServiceInfo,
    timeout: u64,
) -> i32 {
    unsafe {
        if stop_request {
            stop(util::bytes(name), out, timeout)
        } else {
            start(util::bytes(name), out)
        }
    }
}
/// Normal system transitions fence new CPU controls first, then use the
/// existing cooperative stop and real process-exit acknowledgement. No
/// registry reference or Runtime owner survives its bounded waits. A policy
/// service cannot synchronously stop itself/its ancestor to authorize reset.
pub(crate) fn prepare_cpu_power_stop(timeout: u64) -> bool {
    let deadline = stop_deadline(timeout);
    unsafe {
        let Some(_call) = task_context::Admission::acquire(false) else {
            return false;
        };
        let mut target = match refreshed_target(b"POWERSVC") {
            Ok(t) => t,
            Err(NOT_FOUND) => return true,
            Err(_) => return false,
        };
        let Some(current) = target_entry(&target) else {
            return false;
        };
        if current.id != 0 && descends(current.binding.process) {
            return false;
        }
        let mut out = c::ServiceInfo::default();
        stop_bound(&mut target, &mut out, deadline) == c::service_api_result_ok
            && matches!(out.state, x if x == u32::from(STOPPED)
                || x == u32::from(DISABLED) || x == u32::from(FAILED))
    }
}

const _: () = {
    assert!(mem::size_of::<Entry>() == 1216);
    assert!(mem::size_of::<Target>() == 104);
    assert!(mem::size_of::<Reservation>() == 48);
};

const _: () = {
    assert!(mem::offset_of!(Entry, id) == 0);
    assert!(mem::offset_of!(Entry, state) == 4);
    assert!(mem::offset_of!(Entry, mode) == 5);
    assert!(mem::offset_of!(Entry, reserved) == 6);
    assert!(mem::offset_of!(Entry, start_generation) == 8);
    assert!(mem::offset_of!(Entry, name_len) == 16);
    assert!(mem::offset_of!(Entry, path_len) == 24);
    assert!(mem::offset_of!(Entry, args_len) == 32);
    assert!(mem::offset_of!(Entry, name) == 40);
    assert!(mem::offset_of!(Entry, path) == 72);
    assert!(mem::offset_of!(Entry, args) == 1096);
    assert!(mem::offset_of!(Entry, binding) == 1192);
    assert!(mem::offset_of!(Target, registry_generation) == 0);
    assert!(mem::offset_of!(Target, slot) == 8);
    assert!(mem::offset_of!(Target, name_len) == 16);
    assert!(mem::offset_of!(Target, index) == 24);
    assert!(mem::offset_of!(Target, id) == 28);
    assert!(mem::offset_of!(Target, state) == 32);
    assert!(mem::offset_of!(Target, reserved) == 36);
    assert!(mem::offset_of!(Target, name) == 40);
    assert!(mem::offset_of!(Target, start_generation) == 72);
    assert!(mem::offset_of!(Target, binding) == 80);
    assert!(mem::offset_of!(Reservation, slot) == 0);
    assert!(mem::offset_of!(Reservation, generation) == 8);
    assert!(mem::offset_of!(Reservation, registry_generation) == 16);
    assert!(mem::offset_of!(Reservation, binding) == 24);
};

use crate::service::{ffi as sf, registry_types as st};
fn empty_service_bytes() -> Bytes {
    Bytes {
        data: ptr::null(),
        len: 0,
    }
}
fn entry_value(v: st::Entry) -> Entry {
    Entry {
        id: v.instance_id,
        state: v.state,
        mode: v.start_mode,
        reserved: 0,
        start_generation: v.start_generation,
        name_len: v.name_len,
        path_len: v.path_len,
        args_len: v.args_len,
        name: v.name,
        path: v.path,
        args: v.args,
        binding: v.binding,
    }
}
unsafe fn target_native(p: *const Target) -> st::ApiIndexTarget {
    unsafe {
        let t = &*p;
        st::ApiIndexTarget {
            registry_generation: t.registry_generation,
            api_index: t.index,
            service_slot: t.slot,
            instance_id: t.id,
            state: u8::try_from(t.state).unwrap(),
            name: t.name,
            name_len: t.name_len,
            start_generation: t.start_generation,
            binding: t.binding,
        }
    }
}
fn target_value(t: st::ApiIndexTarget) -> Target {
    Target {
        registry_generation: t.registry_generation,
        slot: t.service_slot,
        name_len: t.name_len,
        index: t.api_index,
        id: t.instance_id,
        state: u32::from(t.state),
        reserved: 0,
        name: t.name,
        start_generation: t.start_generation,
        binding: t.binding,
    }
}
unsafe fn reservation_native(p: *const Reservation) -> st::StartReservation {
    unsafe {
        st::StartReservation {
            slot: (*p).slot,
            generation: (*p).generation,
            registry_generation: (*p).registry_generation,
            ..Default::default()
        }
    }
}

pub(crate) unsafe fn native_service_entry(name: Bytes, out: *mut Entry) -> bool {
    let mut value = st::Entry::EMPTY;
    if !unsafe { sf::r4os_rust_services_get(1, 0, name, ptr::null(), &mut value, ptr::null_mut()) }
    {
        return false;
    }
    unsafe {
        out.write(entry_value(value));
    }
    true
}

pub(crate) unsafe fn native_service_install(
    name: Bytes,
    path: Bytes,
    args: Bytes,
    mode: u8,
    description: Bytes,
    out_target: *mut Target,
) -> i32 {
    let mut target = st::ApiIndexTarget::EMPTY;
    let result = unsafe {
        sf::r4os_rust_services_register(name, path, args, mode, description, &mut target)
    };
    if result >= 0 {
        unsafe {
            out_target.write(target_value(target));
        }
    }
    result
}

unsafe fn program_service_index(retry: bool, index: u32, out: *mut Target) -> bool {
    let mut t = st::ApiIndexTarget::EMPTY;
    if !unsafe {
        sf::r4os_rust_services_get(
            if retry { 4 } else { 3 },
            index as usize,
            empty_service_bytes(),
            ptr::null(),
            ptr::null_mut(),
            &mut t,
        )
    } {
        return false;
    }
    unsafe {
        out.write(target_value(t));
    }
    true
}

unsafe fn program_service_target_entry(target: *const Target, out: *mut Entry) -> bool {
    let mut e = st::Entry::EMPTY;
    let target = unsafe { target_native(target) };
    if !unsafe {
        sf::r4os_rust_services_get(
            2,
            0,
            empty_service_bytes(),
            &target,
            &mut e,
            ptr::null_mut(),
        )
    } {
        return false;
    }
    unsafe {
        out.write(entry_value(e));
    }
    true
}

unsafe fn program_service_target_note(target: *const Target) -> bool {
    let target = unsafe { target_native(target) };
    unsafe { sf::r4os_rust_services_target(3, &target, 0, empty_service_bytes()) != 0 }
}

unsafe fn program_service_target_mutate(
    op: u8,
    target: *const Target,
    code: i32,
    text: Bytes,
) -> i32 {
    assert!(op <= 9);
    let t = unsafe { target_native(target) };
    unsafe {
        sf::r4os_rust_services_target(
            op,
            &t,
            if matches!(op, 1 | 2 | 6 | 9) { code } else { 0 },
            if op == 2 { text } else { empty_service_bytes() },
        )
    }
}

unsafe fn program_service_info(
    op: u8,
    name: Bytes,
    target: *const Target,
    out: *mut c::ServiceInfo,
    ticks: u64,
) -> i32 {
    unsafe {
        match op {
            0 => sf::r4os_rust_services_info(2, 0, name, ptr::null(), out, ticks),
            1 => sf::r4os_rust_services_info(3, 0, name, ptr::null(), out, ticks),
            2 => {
                let t = target_native(target);
                sf::r4os_rust_services_info(1, 0, empty_service_bytes(), &t, out, ticks)
            }
            3 => {
                let t = target_native(target);
                sf::r4os_rust_services_info(4, 0, empty_service_bytes(), &t, out, ticks)
            }
            _ => panic!("Invalid service info query"),
        }
    }
}

unsafe fn program_service_detail(
    by_name: bool,
    name: Bytes,
    target: *const Target,
    out: *mut c::ServiceDetail,
    ticks: u64,
) -> i32 {
    unsafe {
        if by_name {
            sf::r4os_rust_services_detail(2, 0, name, ptr::null(), out, ticks)
        } else {
            let t = target_native(target);
            sf::r4os_rust_services_detail(1, 0, empty_service_bytes(), &t, out, ticks)
        }
    }
}

unsafe fn program_service_begin(
    target: *const Target,
    identity: *mut Reservation,
    entry: *mut Entry,
) -> i32 {
    let mut r = st::StartReservation::default();
    let target = unsafe { target_native(target) };
    let result = unsafe {
        sf::r4os_rust_services_start(
            0,
            empty_service_bytes(),
            &mut r,
            st::EMPTY_BINDING,
            0,
            0,
            empty_service_bytes(),
            &target,
        )
    };
    if result == 0 {
        unsafe {
            identity.write(Reservation {
                slot: r.slot,
                generation: r.generation,
                registry_generation: r.registry_generation,
                binding: st::EMPTY_BINDING,
            });
            entry.write(entry_value(r.entry));
        }
    }
    result
}

pub(super) unsafe fn bind_start(
    identity: *mut Reservation,
    handle: c::ProgramProcessHandle,
) -> i32 {
    let mut r = unsafe { reservation_native(identity) };
    unsafe {
        observations::retain_service(handle, ptr::addr_of_mut!((*identity).binding), |binding| {
            sf::r4os_rust_services_start(
                1,
                empty_service_bytes(),
                &mut r,
                binding,
                timer::tick_count(),
                0,
                empty_service_bytes(),
                ptr::null(),
            ) != 0
        })
    }
}

unsafe fn program_service_fail(identity: *const Reservation, code: i32, text: Bytes) -> bool {
    let mut r = unsafe { reservation_native(identity) };
    unsafe {
        sf::r4os_rust_services_start(
            2,
            empty_service_bytes(),
            &mut r,
            st::EMPTY_BINDING,
            0,
            code,
            text,
            ptr::null(),
        ) != 0
    }
}

unsafe fn program_service_endpoint_register(
    name: Bytes,
    process: c::ProgramProcessHandle,
    flags: u32,
    out: *mut c::ServiceInfo,
    ticks: u64,
) -> i32 {
    unsafe { sf::r4os_rust_services_endpoint_register(name, process, flags, out, ticks) }
}

unsafe fn program_service_endpoint_control(op: u8, handle: u32, timeout: u64) -> i32 {
    assert!(op <= 3);
    unsafe {
        sf::r4os_rust_services_endpoint_control(op, handle, 0, if op == 3 { timeout } else { 0 })
    }
}

unsafe fn program_service_endpoint_recv(
    handle: u32,
    header: *mut c::ServiceMessageHeader,
    output: *mut u8,
    len: usize,
) -> i32 {
    unsafe { sf::r4os_rust_services_recv(handle, 0, true, header, output, len) }
}

unsafe fn program_service_endpoint_reply(handle: u32, id: u32, status: i32, data: Bytes) -> i32 {
    unsafe { sf::r4os_rust_services_reply(handle, id, status, data) }
}

unsafe fn program_service_boot(op: u8, name: Bytes, stage: Bytes) {
    unsafe {
        match op {
            0 => crate::display::bootscreen::service(name.data, name.len),
            1 => crate::display::bootscreen::service_stage(
                name.data, name.len, stage.data, stage.len,
            ),
            2 => crate::display::bootscreen::set_detail(b"Dienstplan fertig".as_ptr(), 17),
            3 => crate::display::bootscreen::service_error(name.data, name.len),
            _ => panic!("Invalid service boot event"),
        }
    }
}
