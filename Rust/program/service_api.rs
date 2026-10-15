//! R4X service admission, stable-index refresh and exact-generation lifecycle.
//! The service registry/endpoint state stays with its separately assigned owner.
//! No program registry guard or record borrow crosses a blocking peer operation.
use super::{
    completions, context, exit_state, info, launch, launch_lifetime, payload_types, process,
    process_data, program_api, registry, util,
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
}
#[repr(C)]
#[derive(Default)]
pub struct Reservation {
    slot: usize,
    generation: u64,
}
unsafe extern "C" {
    fn r4os_zig_program_service_entry(name: Bytes, out: *mut Entry) -> bool;
    fn r4os_zig_program_service_index(retry: bool, index: u32, out: *mut Target) -> bool;
    fn r4os_zig_program_service_target_entry(target: *const Target, out: *mut Entry) -> bool;
    fn r4os_zig_program_service_target_note(target: *const Target) -> bool;
    fn r4os_zig_program_service_target_mutate(
        op: u8,
        target: *const Target,
        code: i32,
        text: Bytes,
    ) -> i32;
    fn r4os_zig_program_service_name_mutate(op: u8, name: Bytes, code: i32, text: Bytes) -> i32;
    fn r4os_zig_program_service_info(
        op: u8,
        name: Bytes,
        target: *const Target,
        out: *mut c::ServiceInfo,
        ticks: u64,
    ) -> i32;
    fn r4os_zig_program_service_detail(
        by_name: bool,
        name: Bytes,
        target: *const Target,
        out: *mut c::ServiceDetail,
        ticks: u64,
    ) -> i32;
    fn r4os_zig_program_service_begin(
        name: Bytes,
        identity: *mut Reservation,
        entry: *mut Entry,
    ) -> i32;
    fn r4os_zig_program_service_complete(identity: *const Reservation, id: u32, ticks: u64)
    -> bool;
    fn r4os_zig_program_service_fail(identity: *const Reservation, code: i32, text: Bytes);
    fn r4os_zig_program_service_cancel(name: Bytes, generation: u64);
    fn r4os_zig_program_service_mode(name: Bytes, mode: u8) -> i32;
    fn r4os_zig_program_service_install(
        name: Bytes,
        path: Bytes,
        args: Bytes,
        mode: u8,
        description: Bytes,
    ) -> i32;
    fn r4os_zig_program_service_endpoint_register(
        name: Bytes,
        id: u32,
        flags: u32,
        out: *mut c::ServiceInfo,
        ticks: u64,
    ) -> i32;
    fn r4os_zig_program_service_endpoint_control(op: u8, handle: u32, timeout: u64) -> i32;
    fn r4os_zig_program_service_endpoint_recv(
        handle: u32,
        header: *mut c::ServiceMessageHeader,
        output: *mut u8,
        len: usize,
    ) -> i32;
    fn r4os_zig_program_service_endpoint_reply(
        handle: u32,
        id: u32,
        status: i32,
        data: Bytes,
    ) -> i32;
    fn r4os_zig_program_service_boot(op: u8, name: Bytes, stage: Bytes);
    fn r4os_zig_program_entry_boot_active() -> bool;
}
fn span(s: &[u8]) -> Bytes {
    Bytes {
        data: s.as_ptr(),
        len: s.len(),
    }
}
unsafe fn entry(name: &[u8]) -> Option<Entry> {
    unsafe {
        let mut out = MaybeUninit::uninit();
        if r4os_zig_program_service_entry(span(name), out.as_mut_ptr()) {
            Some(out.assume_init())
        } else {
            None
        }
    }
}
unsafe fn target_entry(target: &Target) -> Option<Entry> {
    unsafe {
        let mut out = MaybeUninit::uninit();
        if r4os_zig_program_service_target_entry(target, out.as_mut_ptr()) {
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
        if !r4os_zig_program_entry_boot_active() {
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
            r4os_zig_program_service_boot(op, span(name), span(stage));
        }
    }
}
unsafe fn status(name: &[u8], out: *mut c::ServiceInfo) -> i32 {
    unsafe { r4os_zig_program_service_info(0, span(name), ptr::null(), out, timer::tick_count()) }
}
unsafe fn mutate(op: u8, name: &[u8], code: i32, text: &[u8]) -> i32 {
    unsafe { r4os_zig_program_service_name_mutate(op, span(name), code, span(text)) }
}
unsafe fn mark_target(op: u8, target: &Target, code: i32, text: &[u8]) -> bool {
    unsafe { r4os_zig_program_service_target_mutate(op, target, code, span(text)) == 0 }
}
unsafe fn old_exit(id: u32, stopping: bool) -> Option<i32> {
    unsafe {
        let mut code = if stopping { 0 } else { -1 };
        let mut history = 0;
        if process::r4os_rust_program_process_history(id, &mut history) {
            code = history;
        }
        let mut handle = c::ProgramProcessHandle::default();
        if process::r4os_rust_program_process_open(id, &mut handle) == 0 {
            let mut completion = c::ProgramProcessCompletion::default();
            let result = process::r4os_rust_program_process_reap(&handle, &mut completion);
            if result == -5 {
                return None;
            }
            if result == 0 {
                code = completion.exit_code;
            }
        }
        Some(code)
    }
}
unsafe fn refresh_name(name: &[u8]) {
    unsafe {
        let Some(entry) = entry(name) else {
            return;
        };
        if entry.id == 0 {
            return;
        }
        let name = &entry.name[..entry.name_len];
        let mut snapshot = MaybeUninit::uninit();
        if info::r4os_rust_program_info_snapshot(entry.id, snapshot.as_mut_ptr()) {
            let snapshot = snapshot.assume_init();
            if snapshot.app_class != 2 {
                mutate(2, name, -2, b"instance class mismatch");
            } else if snapshot.state == 2 {
                process::r4os_rust_program_process_finish(entry.id);
                if entry.state == STOPPING || snapshot.exit_code == 0 {
                    mutate(1, name, snapshot.exit_code, b"");
                } else {
                    mutate(2, name, snapshot.exit_code, b"service exited");
                }
            } else if snapshot.close_requested && entry.state != STOPPING {
                mutate(0, name, 0, b"");
            }
            return;
        }
        let Some(code) = old_exit(entry.id, entry.state == STOPPING) else {
            return;
        };
        if entry.state == STOPPING || code == 0 {
            mutate(1, name, code, b"");
        } else {
            mutate(2, name, code, b"service exited");
        }
    }
}
unsafe fn refresh_target(target: &mut Target) -> bool {
    unsafe {
        let Some(entry) = target_entry(target) else {
            return false;
        };
        if entry.id == 0 {
            return true;
        }
        if !r4os_zig_program_service_target_note(target) {
            return false;
        }
        let mut snapshot = MaybeUninit::uninit();
        if info::r4os_rust_program_info_snapshot(entry.id, snapshot.as_mut_ptr()) {
            let snapshot = snapshot.assume_init();
            if snapshot.app_class != 2 {
                if !mark_target(2, target, -2, b"instance class mismatch") {
                    return false;
                }
                target.id = 0;
                target.state = u32::from(FAILED);
            } else if snapshot.state == 2 {
                let code = snapshot.exit_code;
                process::r4os_rust_program_process_finish(entry.id);
                let Some(current) = target_entry(target) else {
                    return false;
                };
                if current.state == STOPPING || code == 0 {
                    if !mark_target(1, target, code, b"") {
                        return false;
                    }
                    target.state = u32::from(if current.mode == MODE_DISABLED {
                        DISABLED
                    } else {
                        STOPPED
                    });
                } else {
                    if !mark_target(2, target, code, b"service exited") {
                        return false;
                    }
                    target.state = u32::from(FAILED);
                }
                target.id = 0;
            } else if snapshot.close_requested && entry.state != STOPPING {
                if !mark_target(0, target, 0, b"") {
                    return false;
                }
                target.state = u32::from(STOPPING);
            }
            return target_entry(target).is_some();
        }
        let Some(code) = old_exit(entry.id, entry.state == STOPPING) else {
            return target_entry(target).is_some();
        };
        let Some(current) = target_entry(target) else {
            return false;
        };
        if current.state == STOPPING || code == 0 {
            if !mark_target(1, target, code, b"") {
                return false;
            }
            target.state = u32::from(if current.mode == MODE_DISABLED {
                DISABLED
            } else {
                STOPPED
            });
        } else {
            if !mark_target(2, target, code, b"service exited") {
                return false;
            }
            target.state = u32::from(FAILED);
        }
        target.id = 0;
        true
    }
}
struct Registry;
impl Drop for Registry {
    fn drop(&mut self) {
        registry::unlock();
    }
}
unsafe fn descends(id: u32) -> bool {
    unsafe {
        if id == 0 {
            return false;
        }
        let Some(caller) = context::handle() else {
            return false;
        };
        if caller.instance_id == id {
            return true;
        }
        if !registry::lock() {
            return false;
        }
        let _guard = Registry;
        let mut current = caller;
        while registry::valid(current) {
            if current.instance_id == id {
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
struct Unwind(task_context::Unwind);
impl Drop for Unwind {
    fn drop(&mut self) {
        unsafe {
            task_context::leave(self.0);
        }
    }
}
unsafe fn start(name: &[u8], out: *mut c::ServiceInfo) -> i32 {
    unsafe {
        out.write(c::ServiceInfo::default());
        report(0, name, b"");
        refresh_name(name);
        let Some(current) = entry(name) else {
            return NOT_FOUND;
        };
        if current.state == RUNNING {
            return status(name, out);
        }
        let _unwind = Unwind(task_context::enter());
        let mut identity = Reservation::default();
        let mut raw = MaybeUninit::<Entry>::uninit();
        let reserved = r4os_zig_program_service_begin(span(name), &mut identity, raw.as_mut_ptr());
        if reserved != 0 {
            status(name, out);
            report(3, name, b"");
            return reserved;
        }
        let entry = raw.assume_init();
        let name = &entry.name[..entry.name_len];
        report(1, name, b"Pfad pruefen");
        let mut path = [MaybeUninit::<u8>::uninit(); 1024];
        let Some(target) = resolve(&entry.path[..entry.path_len], path.as_mut_ptr().cast()) else {
            r4os_zig_program_service_fail(&identity, -1, span(b"path invalid or not service R4X"));
            status(name, out);
            report(3, name, b"");
            return c::service_api_result_bad_path;
        };
        report(1, name, b"laden");
        let mut handle = c::ProgramProcessHandle::default();
        let raw_id = launch::r4os_rust_program_launch_path(
            2,
            target.drive,
            target.path,
            span(&entry.args[..entry.args_len]),
            target.drive,
            0,
            &mut handle,
        );
        if raw_id <= 0 {
            r4os_zig_program_service_fail(&identity, raw_id, span(b"spawn failed"));
            status(name, out);
            report(3, name, b"");
            return c::service_api_result_spawn_failed;
        }
        if !r4os_zig_program_service_complete(&identity, handle.instance_id, timer::tick_count()) {
            launch_lifetime::cancel_published(handle);
            completions::r4os_rust_program_completion_access(4, handle, ptr::null_mut());
            status(name, out);
            report(3, name, b"");
            return BUSY;
        }
        report(1, name, b"gestartet");
        status(name, out)
    }
}
unsafe fn stop(name: &[u8], out: *mut c::ServiceInfo, timeout: u64) -> i32 {
    unsafe {
        out.write(c::ServiceInfo::default());
        refresh_name(name);
        let Some(entry) = entry(name) else {
            return NOT_FOUND;
        };
        let name = &entry.name[..entry.name_len];
        if entry.state == STARTING && entry.id == 0 {
            r4os_zig_program_service_cancel(span(name), entry.start_generation);
        }
        if matches!(entry.state, DISABLED | STOPPED | FAILED) || entry.id == 0 {
            return status(name, out);
        }
        mutate(0, name, 0, b"");
        let close = process::r4os_rust_program_process_legacy(0, entry.id);
        refresh_name(name);
        let Some(after) = self::entry(name) else {
            return NOT_FOUND;
        };
        if matches!(after.state, STOPPED | DISABLED) {
            return status(name, out);
        }
        if close != 0 {
            let mut handle = c::ProgramProcessHandle::default();
            let mut info = c::ProgramInstanceInfo::default();
            let terminal = process::r4os_rust_program_process_open(entry.id, &mut handle) == 0
                && info::r4os_rust_program_info_handle(&handle, &mut info) == 0
                && info.state != 0;
            if !terminal {
                status(name, out);
                return c::service_api_result_stop_failed;
            }
        }
        let mut waited = 0u64;
        while waited < timeout {
            refresh_name(name);
            let Some(current) = self::entry(name) else {
                return NOT_FOUND;
            };
            if matches!(current.state, STOPPED | DISABLED) {
                return status(name, out);
            }
            if current.state == FAILED {
                status(name, out);
                return c::service_api_result_stop_failed;
            }
            let mut queue = WaitQueue::default();
            sync_wait::r4os_rust_sync_timer(&mut queue, 1, b"service-stop".as_ptr(), 12);
            waited += 1;
        }
        refresh_name(name);
        status(name, out);
        c::service_api_result_timeout
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
        for attempt in 0..3 {
            let mut target = MaybeUninit::uninit();
            if !r4os_zig_program_service_index(attempt != 0, index, target.as_mut_ptr()) {
                report(2, b"", b"");
                out.write(c::ServiceInfo::default());
                return 0;
            }
            let mut target = target.assume_init();
            if !refresh_target(&mut target) {
                continue;
            }
            let result =
                r4os_zig_program_service_info(2, span(b""), &target, out, timer::tick_count());
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
        for attempt in 0..3 {
            let mut target = MaybeUninit::uninit();
            if !r4os_zig_program_service_index(attempt != 0, index, target.as_mut_ptr()) {
                report(2, b"", b"");
                out.write(c::ServiceDetail::default());
                return 0;
            }
            let mut target = target.assume_init();
            if !refresh_target(&mut target) {
                continue;
            }
            let result = r4os_zig_program_service_detail(
                false,
                span(b""),
                &target,
                out,
                timer::tick_count(),
            );
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
        match op {
            0 | 1 => {
                refresh_name(name);
                r4os_zig_program_service_info(op, span(name), ptr::null(), out, timer::tick_count())
            }
            2 => start(name, out),
            3 => {
                if let Some(entry) = entry(name)
                    && descends(entry.id)
                {
                    return c::service_api_result_self_restart;
                }
                stop(name, out, timeout)
            }
            4 => {
                refresh_name(name);
                let Some(entry) = entry(name) else {
                    return NOT_FOUND;
                };
                let name = &entry.name[..entry.name_len];
                if descends(entry.id) {
                    return c::service_api_result_self_restart;
                }
                if matches!(entry.state, RUNNING | STARTING | STOPPING) {
                    let result = stop(name, out, 40);
                    if result < 0 {
                        return result;
                    }
                }
                mutate(3, name, 0, b"");
                start(name, out)
            }
            6 => {
                refresh_name(name);
                let Some(entry) = entry(name) else {
                    return NOT_FOUND;
                };
                let name = &entry.name[..entry.name_len];
                if mode == MODE_DISABLED && descends(entry.id) {
                    return c::service_api_result_self_restart;
                }
                if mode == MODE_DISABLED && matches!(entry.state, RUNNING | STARTING | STOPPING) {
                    let result = stop(name, out, 40);
                    if result < 0 {
                        return result;
                    }
                }
                let result = r4os_zig_program_service_mode(span(name), mode);
                if result != 0 {
                    return registry_result(result);
                }
                status(name, out)
            }
            7 => {
                refresh_name(name);
                let Some(entry) = entry(name) else {
                    return NOT_FOUND;
                };
                if matches!(entry.state, RUNNING | STARTING | STOPPING) {
                    return c::service_api_result_running;
                }
                let result = mutate(4, &entry.name[..entry.name_len], 0, b"");
                if result != 0 {
                    registry_result(result)
                } else {
                    0
                }
            }
            _ => unreachable!(),
        }
    }
}
/// # Safety
/// Original NUL name and canonical detail output remain resident through refresh.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_service_detail_name(
    name: *const u8,
    out: *mut c::ServiceDetail,
) -> i32 {
    unsafe {
        if out.is_null() {
            return INVALID;
        }
        let mut buffer = [MaybeUninit::uninit(); 32];
        let Some(name) = zcopy(name, &mut buffer) else {
            return INVALID;
        };
        refresh_name(name);
        r4os_zig_program_service_detail(true, span(name), ptr::null(), out, timer::tick_count())
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
        let result = r4os_zig_program_service_install(
            span(name),
            span(full),
            span(args),
            mode,
            span(description),
        );
        if result < 0 {
            report(3, name, b"");
            return registry_result(result);
        }
        let status = status(name, out);
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
        let instance = context::current();
        if instance.is_null() || (*instance).app_class != 2 {
            return INVALID;
        }
        let id = (*instance).id;
        let mut buffer = [MaybeUninit::uninit(); 32];
        let Some(name) = zcopy(name, &mut buffer) else {
            return INVALID;
        };
        r4os_zig_program_service_endpoint_register(span(name), id, flags, out, timer::tick_count())
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
    unsafe { r4os_zig_program_service_endpoint_control(op, handle, timeout) }
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
        r4os_zig_program_service_endpoint_recv(handle, header, out, capacity as usize)
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
        let result = r4os_zig_program_service_endpoint_reply(handle, id, status, bytes);
        scheduler::r4os_rust_scheduler_control(11, 0, 0);
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
const _: () = {
    assert!(mem::size_of::<Entry>() == 1192);
    assert!(mem::size_of::<Target>() == 72);
    assert!(mem::size_of::<Reservation>() == 16);
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
    assert!(mem::offset_of!(Target, registry_generation) == 0);
    assert!(mem::offset_of!(Target, slot) == 8);
    assert!(mem::offset_of!(Target, name_len) == 16);
    assert!(mem::offset_of!(Target, index) == 24);
    assert!(mem::offset_of!(Target, id) == 28);
    assert!(mem::offset_of!(Target, state) == 32);
    assert!(mem::offset_of!(Target, reserved) == 36);
    assert!(mem::offset_of!(Target, name) == 40);
    assert!(mem::offset_of!(Reservation, slot) == 0);
    assert!(mem::offset_of!(Reservation, generation) == 8);
};
