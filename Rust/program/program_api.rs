//! Public program launch/host APIs and canonical caller paths. Legacy starts
//! and handle starts keep distinct validation order and error contracts.
use super::{
    console::{self, Lease},
    context, exit_state, launch, payload, payload_types as t, process, process_data, registry,
    util,
};
use crate::{
    contract::{
        self as c, ProgramHostLaunchRequest as HostRequest, ProgramProcessHandle as Handle,
    },
    fs::{
        drive::{self, Drive},
        vfs_types::Bytes,
    },
    memory::owner::Guard,
};
use core::{
    mem,
    mem::MaybeUninit,
    ptr, slice,
    sync::atomic::{AtomicBool, AtomicI32, Ordering},
};
const PATH: usize = 1024;
const ARGS: usize = 128;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Target {
    pub drive: *mut Drive,
    pub path: Bytes,
}
struct Registry;
impl Registry {
    fn acquire() -> Option<Self> {
        if registry::lock() { Some(Self) } else { None }
    }
}
impl Drop for Registry {
    fn drop(&mut self) {
        registry::unlock()
    }
}
fn separator(value: u8) -> bool {
    matches!(value, b'/' | b'\\')
}
unsafe fn build(cwd: &[u8], arg: &[u8], out: *mut u8) -> Option<usize> {
    unsafe {
        let mut combined = [MaybeUninit::<u8>::uninit(); PATH];
        let data = combined.as_mut_ptr().cast::<u8>();
        let mut len;
        if !arg.is_empty() && separator(arg[0]) {
            if arg.len() > PATH {
                return None;
            }
            ptr::copy_nonoverlapping(arg.as_ptr(), data, arg.len());
            len = arg.len();
        } else {
            if cwd.len() > PATH {
                return None;
            }
            ptr::copy_nonoverlapping(cwd.as_ptr(), data, cwd.len());
            len = cwd.len();
            if len > 1 {
                if len >= PATH {
                    return None;
                }
                data.add(len).write(b'\\');
                len += 1;
            }
            if len + arg.len() > PATH {
                return None;
            }
            ptr::copy_nonoverlapping(arg.as_ptr(), data.add(len), arg.len());
            len += arg.len();
        }
        let mut n = 0;
        if process_data::r4os_rust_program_data_canonical(Bytes { data, len }, out, &mut n) {
            Some(n)
        } else {
            None
        }
    }
}
pub(super) unsafe fn resolve(raw: Bytes, out: *mut u8) -> Option<Target> {
    unsafe {
        if raw.len == 0 {
            return None;
        }
        let raw = util::bytes(raw);
        let instance = context::current();
        let mut current = b'C';
        let mut cwd = [MaybeUninit::<u8>::uninit(); PATH];
        let cwd_data = cwd.as_mut_ptr().cast::<u8>();
        let len;
        if !instance.is_null() {
            let _guard = Registry::acquire()?;
            let process = payload::process(instance);
            current = (*process).work_drive_letter;
            len = (*process).work_cwd_len;
            let source = &(&(*process).work_cwd)[..len];
            assert!(len <= PATH);
            ptr::copy_nonoverlapping(source.as_ptr(), cwd_data, len);
        } else {
            // Exact original kernel fallback globals: C:, length1, zero-filled cwd.
            // No old writer exists; these defaults are immutable after the port.
            len = 1;
            cwd_data.write(0);
        }
        let mut drive = drive::r4os_rust_fs_drive_lookup(1, usize::from(current));
        if drive.is_null() {
            return None;
        }
        let mut rest = raw;
        if raw.len() >= 2 && raw[1] == b':' && raw[0].is_ascii_alphabetic() {
            drive = drive::r4os_rust_fs_drive_lookup(1, usize::from(raw[0]));
            if drive.is_null() {
                return None;
            }
            rest = &raw[2..];
        }
        let cwd = if (*drive).letter == current {
            slice::from_raw_parts(cwd_data, len)
        } else {
            &(&(*drive).cwd)[..(*drive).cwd_len]
        };
        let len = build(cwd, rest, out)?;
        Some(Target {
            drive,
            path: Bytes { data: out, len },
        })
    }
}
unsafe fn zcopy<'a>(input: *const u8, buffer: *mut u8, capacity: usize) -> Option<&'a [u8]> {
    unsafe {
        let mut len = 0;
        if process_data::r4os_rust_program_data_copy_z(input, buffer, capacity, &mut len) {
            Some(slice::from_raw_parts(buffer, len))
        } else {
            None
        }
    }
}
/// # Safety
/// Original source strings and optional exact handle output retain their
/// process lifetime; caller buffers stay resident through the yielding launch.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_api_launch(
    mode: u8,
    path: *const u8,
    args: *const u8,
    policy: u32,
    host: u32,
    out: *mut Handle,
) -> i32 {
    unsafe {
        let handles = mode == 2;
        let invalid = if handles { -1 } else { -2 };
        if handles {
            if out.is_null() || path.is_null() || args.is_null() {
                return -1;
            }
            out.write(Handle::default());
        }
        let mut path_buffer = [MaybeUninit::<u8>::uninit(); PATH];
        let Some(path) = zcopy(path, path_buffer.as_mut_ptr().cast(), PATH) else {
            return invalid;
        };
        let mut args_buffer = [MaybeUninit::<u8>::uninit(); ARGS];
        let args = if mode == 3 {
            &[][..]
        } else {
            let Some(args) = zcopy(args, args_buffer.as_mut_ptr().cast(), ARGS) else {
                return invalid;
            };
            args
        };
        let mut resolved = [MaybeUninit::<u8>::uninit(); PATH];
        let Some(target) = resolve(util::span(path), resolved.as_mut_ptr().cast()) else {
            return invalid;
        };
        if handles && (policy > 2 || host > 2) {
            return -1;
        }
        let Some(file) = launch::resolve(target.drive, target.path, false) else {
            return if handles { -2 } else { -1 };
        };
        if !handles && (policy > 2 || (mode == 1 && host > 2)) {
            return -2;
        }
        if mode == 3 {
            return launch::classify(&file, policy).map_or(-2, |class| i32::from(class) + 1);
        }
        let mut handle = Handle::default();
        let mut launch_error = 0;
        let mut options = launch::options();
        options.owner = true;
        let host = if mode == 0 { 0 } else { host as u8 };
        if mode == 0 {
            options.owner_handle_present = true;
        } else {
            options.legacy_id = !handles;
            options.retain_output = true;
            options.out_handle = if handles { out } else { &mut handle };
            if handles {
                options.error_out = &mut launch_error;
            }
        }
        let result = launch::run(
            &file,
            if mode == 0 { 0 } else { 2 },
            policy,
            util::span(args),
            target.drive,
            host,
            &options,
        );
        if handles {
            match result {
                0 => 0,
                1 => -2,
                2 => {
                    if launch_error == 0 {
                        -10
                    } else {
                        launch_error
                    }
                }
                _ => unreachable!(),
            }
        } else {
            match result {
                0 => {
                    if mode == 0 {
                        0
                    } else {
                        i32::try_from(handle.instance_id).expect("Legacy program ID")
                    }
                }
                1 => -1,
                2 => -2,
                _ => unreachable!(),
            }
        }
    }
}
unsafe fn window_id() -> i32 {
    unsafe {
        let instance = context::current();
        if instance.is_null() {
            return -1;
        }
        let gui = (*instance).gui_payload.cast::<t::Gui>();
        if gui.is_null() {
            return -1;
        }
        let _guard = Guard::acquire(0);
        AtomicI32::from_ptr(ptr::addr_of_mut!((*gui).window_id)).load(Ordering::Acquire)
    }
}
/// # Safety
/// Selected exact process identity remains pinned; handle input is resident.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_api_window(
    id: u32,
    handle: *const Handle,
    value: i32,
    by_handle: bool,
) -> i32 {
    unsafe {
        if by_handle {
            if handle.is_null() || !registry::valid(handle.read()) {
                return -1;
            }
            process::r4os_rust_program_process_signal();
            let handle = handle.read();
            let Some(lease) = Lease::handle(handle) else {
                let Some(_guard) = Registry::acquire() else {
                    return -5;
                };
                if !registry::completion_handle(handle).is_null() {
                    return -4;
                }
                let slot = registry::lookup_handle(handle, true);
                if !slot.is_null() {
                    return if registry::running((*slot).state) {
                        -5
                    } else {
                        -4
                    };
                }
                return registry::missing(handle);
            };
            let gui = process_data::ensure_gui(lease.0.instance);
            if gui.is_null() {
                return -10;
            }
            let _guard = Guard::acquire(0);
            AtomicI32::from_ptr(ptr::addr_of_mut!((*gui).window_id))
                .store(value, Ordering::Release);
            (*gui).window_info.window_id = value;
            AtomicBool::from_ptr(ptr::addr_of_mut!((*gui).start_attach_pending))
                .store(false, Ordering::Release);
            0
        } else {
            process::r4os_rust_program_process_signal();
            let Some(lease) = Lease::id(id) else {
                return -1;
            };
            let instance = lease.0.instance;
            if AtomicBool::from_ptr(ptr::addr_of_mut!((*instance).done)).load(Ordering::Acquire) {
                return -2;
            }
            let gui = process_data::ensure_gui(instance);
            if gui.is_null() {
                return -3;
            }
            let _guard = Guard::acquire(0);
            AtomicI32::from_ptr(ptr::addr_of_mut!((*gui).window_id))
                .store(value, Ordering::Release);
            (*gui).window_info.window_id = value;
            0
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_api_console_host(id: u32, host: u32) -> i32 {
    unsafe {
        process::r4os_rust_program_process_signal();
        let Some(lease) = Lease::id(id) else {
            return -1;
        };
        let instance = lease.0.instance;
        if AtomicBool::from_ptr(ptr::addr_of_mut!((*instance).done)).load(Ordering::Acquire) {
            return -2;
        }
        if (*instance).app_class != 0 {
            return -3;
        }
        if host > 2 {
            return -4;
        }
        let console = payload::console(instance);
        (*console).host = host;
        (*console).revision = (*console).revision.wrapping_add(1);
        if (*console).revision == 0 {
            (*console).revision = 1;
        }
        0
    }
}
/// # Safety
/// Canonical exact handle/read count and output byte span are resident, aligned
/// and exclusive. Registry admission retains detached output until copy ends.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_api_completion_read(
    handle: *const Handle,
    offset: u32,
    out: *mut u8,
    capacity: u32,
    out_read: *mut u32,
) -> i32 {
    unsafe {
        if handle.is_null() || out_read.is_null() || !registry::valid(handle.read()) {
            return -1;
        }
        out_read.write(0);
        if out.is_null() {
            return -1;
        }
        let Some(_guard) = Registry::acquire() else {
            return -5;
        };
        let handle = handle.read();
        let node = registry::completion_handle(handle);
        if node.is_null() {
            return registry::missing(handle);
        }
        if (*node).state != 2 {
            return -5;
        }
        let transcript = (*node).output_payload.cast::<t::ConsoleTranscript>();
        if transcript.is_null() || (*node).flags & 2 == 0 {
            return -12;
        }
        let start = offset as usize;
        let len = (*node).output_length as usize;
        if start > len {
            return -13;
        }
        let count = (capacity as usize).min(len - start);
        if count != 0
            && console::r4os_rust_program_console_read(transcript, start, out, count, false)
                != count
        {
            return -13;
        }
        out_read.write(count as u32);
        0
    }
}
/// # Safety
/// Original caller-owned strings remain resident through bounded copy; current
/// GUI retains its execution lifetime and exact program-state owner admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_api_host_request(
    path: *const u8,
    args: *const u8,
    policy: u32,
) -> i32 {
    unsafe {
        let instance = context::current();
        if instance.is_null() {
            return -1;
        }
        if (*instance).role != 2 || (*instance).app_class != 1 {
            return -3;
        }
        let gui = (*instance).gui_payload.cast::<t::Gui>();
        assert!(!gui.is_null());
        if window_id() < 0 {
            return -3;
        }
        if policy > 2 {
            return -4;
        }
        let mut path_buffer = [MaybeUninit::<u8>::uninit(); PATH];
        let Some(path) = zcopy(path, path_buffer.as_mut_ptr().cast(), PATH) else {
            return -2;
        };
        if path.is_empty() {
            return -2;
        }
        let mut args_buffer = [MaybeUninit::<u8>::uninit(); ARGS];
        let Some(args) = zcopy(args, args_buffer.as_mut_ptr().cast(), ARGS) else {
            return -2;
        };
        let mut request = HostRequest {
            policy,
            reserved: 0,
            ..HostRequest::default()
        };
        if path.len() >= request.path.len() || args.len() >= request.args.len() {
            return -2;
        }
        process_data::r4os_rust_program_data_copy_slice(
            request.path.as_mut_ptr(),
            request.path.len(),
            util::span(path),
        );
        process_data::r4os_rust_program_data_copy_slice(
            request.args.as_mut_ptr(),
            request.args.len(),
            util::span(args),
        );
        {
            let _guard = Guard::acquire(0);
            (*gui).host_launch_request = request;
            (*gui).host_launch_pending = true;
        }
        console::sink(4, 0, 0);
        0
    }
}
/// # Safety
/// Exclusive canonical output and retained exact process lease. Original API
/// requires output even for missing/closed processes; no prefix is widened.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_api_host_take(id: u32, out: *mut HostRequest) -> i32 {
    unsafe {
        process::r4os_rust_program_process_signal();
        let Some(lease) = Lease::id(id) else {
            out.write(HostRequest::default());
            return -1;
        };
        let instance = lease.0.instance;
        if AtomicBool::from_ptr(ptr::addr_of_mut!((*instance).done)).load(Ordering::Acquire) {
            out.write(HostRequest::default());
            return -2;
        }
        let gui = (*instance).gui_payload.cast::<t::Gui>();
        if gui.is_null() {
            out.write(HostRequest::default());
            return 0;
        }
        let (pending, snapshot) = {
            let _guard = Guard::acquire(0);
            let pending = (*gui).host_launch_pending;
            let snapshot = if pending {
                (*gui).host_launch_request
            } else {
                HostRequest::default()
            };
            (*gui).host_launch_request = HostRequest::default();
            (*gui).host_launch_pending = false;
            (pending, snapshot)
        };
        out.write(snapshot);
        i32::from(pending)
    }
}
/// # Safety
/// Exclusive original output bytes through the bounded live runtime copy.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_api_module_path(out: *mut u8, capacity: u32) -> i32 {
    unsafe {
        if capacity == 0 {
            return -1;
        }
        let instance = context::current();
        if instance.is_null() {
            return -2;
        }
        let runtime = payload::runtime(instance);
        let len = (*runtime).module_path_len as usize;
        if len == 0 {
            return -4;
        }
        if len > capacity as usize {
            return -3;
        }
        let source = &(&(*runtime).module_path)[..len];
        ptr::copy_nonoverlapping(source.as_ptr(), out, len);
        len as i32
    }
}
unsafe extern "C" {
    fn r4os_zig_program_api_display_used();
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_api_action(op: u8) -> i32 {
    unsafe {
        match op {
            0 => window_id(),
            1 => console::r4os_rust_program_console_host(4, 0, 0, ptr::null_mut()),
            2 => {
                if console::r4os_rust_program_console_host(5, 0, 0, ptr::null_mut()) != 0 {
                    0
                } else {
                    -1
                }
            }
            3 => {
                let instance = context::current();
                if instance.is_null() || (*instance).role != 1 {
                    return c::boot_ready_error_not_boot_shell;
                }
                let result = crate::boot::performance::complete_ready((*instance).id);
                if result == c::boot_ready_result_completed {
                    crate::display::bootscreen::complete();
                    crate::boot::foundation::r4os_rust_boot_release();
                }
                result
            }
            4 => {
                r4os_zig_program_api_display_used();
                0
            }
            5 => {
                (*exit_state::state()).last_display_used = true;
                let instance = context::current();
                if !instance.is_null() {
                    (*instance).display_used = true;
                }
                0
            }
            _ => unreachable!(),
        }
    }
}
/// # Safety
/// Resident original input and exclusive1024-byte canonical result buffer.
/// Returned target borrows that caller buffer and the existing mounted drive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_api_target(
    raw: Bytes,
    out: *mut u8,
    target: *mut Target,
) -> bool {
    unsafe {
        let Some(value) = resolve(raw, out) else {
            return false;
        };
        target.write(value);
        true
    }
}
/// # Safety
/// Original bounded input paths and exclusive1024-byte canonical output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_api_build(
    cwd: Bytes,
    arg: Bytes,
    out: *mut u8,
    len: *mut usize,
) -> bool {
    unsafe {
        let Some(n) = build(util::bytes(cwd), util::bytes(arg), out) else {
            return false;
        };
        len.write(n);
        true
    }
}
const _: () = {
    assert!(mem::size_of::<Target>() == 24);
    assert!(mem::offset_of!(Target, drive) == 0);
    assert!(mem::offset_of!(Target, path) == 8);
};
