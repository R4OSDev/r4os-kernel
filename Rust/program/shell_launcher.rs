//! Original external-shell startup, service boot and fallback ordering.
//! All state remains at its existing config/boot/R4X/driver owner.
use crate::{fs::drive, sched::scheduler, storage::boot as storage_boot};
use core::{ffi::c_void, ptr, slice};

const TERMINAL: &[u8] = b"/R4OS/SOFTWARE/TERMINAL/TERMINAL.R4X";
const SERVICES: &[u8] = b"/R4OS/SOFTWARE/TERMINAL/SERVMAN.R4X";
const RECOVERY: [&[u8]; 1] = [b"/R4OS/SOFTWARE/TERMINAL/RECOVERY.R4X"];
const RAN: u8 = 0;
const NOT_FOUND: u8 = 1;
const FAILED: u8 = 2;
const TERMINAL_MODE: u32 = 2;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Bytes {
    pub data: *const u8,
    pub len: usize,
}
#[repr(C)]
struct MscStatus {
    present: bool,
    block_registered: bool,
    read_failures: u64,
    transport_failures: u64,
    reason: Bytes,
}
#[repr(C)]
struct Config {
    path: Bytes,
    args: Bytes,
}
unsafe extern "C" {
    fn r4os_zig_program_shell_init(usable_bytes: u64);
    fn r4os_zig_program_shell_config(config: *const c_void, output: *mut Config);
    fn r4os_zig_program_shell_text(channel: u8, value: Bytes);
    fn r4os_zig_program_shell_perf(op: u8, a: u64, b: u64) -> u64;
    fn r4os_zig_program_shell_run(
        boot: *mut c_void,
        working: *mut c_void,
        path: Bytes,
        args: Bytes,
        shell: bool,
        host: u32,
    ) -> u8;
    fn r4os_zig_program_shell_msc(output: *mut MscStatus);
    fn r4os_zig_program_shell_fatal(reason: Bytes) -> !;
}
fn span(value: &[u8]) -> Bytes {
    Bytes {
        data: value.as_ptr(),
        len: value.len(),
    }
}
unsafe fn bytes<'a>(value: Bytes) -> &'a [u8] {
    if value.len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(value.data, value.len) }
    }
}
fn text(channel: u8, value: &[u8]) {
    unsafe { r4os_zig_program_shell_text(channel, span(value)) }
}
fn log(value: &[u8]) {
    text(1, value)
}
fn perf(op: u8, a: u64, b: u64) -> u64 {
    unsafe { r4os_zig_program_shell_perf(op, a, b) }
}
fn run(
    boot: *mut c_void,
    working: *mut c_void,
    path: &[u8],
    args: &[u8],
    shell: bool,
    host: u32,
) -> u8 {
    let result =
        unsafe { r4os_zig_program_shell_run(boot, working, span(path), span(args), shell, host) };
    assert!(result <= FAILED);
    result
}
fn boot_drive() -> *mut c_void {
    unsafe { drive::r4os_rust_fs_drive_lookup(1, b'C' as usize).cast() }
}
fn current_drive() -> *mut c_void {
    unsafe { drive::r4os_rust_fs_drive_lookup(0, 0).cast() }
}
fn path_len(path: &[u8]) -> usize {
    let mut end = path.len();
    while end > 0 && path[end - 1] == 0 {
        end -= 1;
    }
    end
}
fn normalized(byte: u8) -> u8 {
    if byte == b'\\' {
        b'/'
    } else {
        byte.to_ascii_uppercase()
    }
}
fn same_path(a: &[u8], b: &[u8]) -> bool {
    if path_len(a) != path_len(b) {
        return false;
    }
    a.iter()
        .zip(b)
        .all(|(a, b)| normalized(*a) == normalized(*b))
}
fn service_boot(boot: *mut c_void, working: *mut c_void) {
    log(b"[LAUNCH] service manager boot=");
    log(SERVICES);
    log(b" args=");
    log(b"BOOT");
    log(b"\r\n");
    text(3, b"Dienstplan laden");
    let start = perf(0, 0, 0);
    let result = run(boot, working, SERVICES, b"BOOT", false, 0);
    text(4, b"");
    match result {
        RAN => {
            perf(1, start, 1);
            log(b"[LAUNCH] service manager boot completed\r\n");
        }
        NOT_FOUND => {
            perf(1, start, 2);
            text(0, b"Service manager not found: ");
            text(0, SERVICES);
            text(0, b"\r\n");
            log(b"[LAUNCH][WARN] service manager boot not found\r\n");
        }
        FAILED => {
            perf(1, start, 3);
            text(0, b"Service manager failed: ");
            text(0, SERVICES);
            text(0, b"\r\n");
            log(b"[LAUNCH][WARN] service manager boot failed\r\n");
        }
        _ => unreachable!(),
    }
}
fn note_result(result: u8) -> u8 {
    if result == RAN {
        perf(3, 0, 0);
    } else {
        perf(4, 0, 0);
    }
    result
}
fn configured(boot: *mut c_void, working: *mut c_void, path: &[u8], args: &[u8]) -> u8 {
    log(b"[LAUNCH] configured shell=");
    log(path);
    if !args.is_empty() {
        log(b" args=");
        log(args);
    }
    log(b"\r\n");
    perf(2, 0, 0);
    let host = if same_path(path, TERMINAL) {
        TERMINAL_MODE
    } else {
        0
    };
    let result = run(boot, working, path, args, true, host);
    note_result(result);
    if result != RAN {
        let label = if result == NOT_FOUND {
            b" not found: ".as_slice()
        } else {
            b" failed: ".as_slice()
        };
        text(0, b"Configured shell");
        text(0, label);
        text(0, path);
        text(0, b"\r\n");
        log(b"[LAUNCH][WARN] ");
        log(b"Configured shell");
        log(label);
        log(path);
        log(b"\r\n");
    }
    result
}
fn terminal(boot: *mut c_void, working: *mut c_void) -> u8 {
    log(b"[LAUNCH] external terminal fallback=");
    log(TERMINAL);
    log(b"\r\n");
    perf(2, 1, 0);
    note_result(run(boot, working, TERMINAL, b"", true, TERMINAL_MODE))
}
fn recovery(boot: *mut c_void, working: *mut c_void, path: &[u8]) -> u8 {
    text(2, b"Recovery starten");
    log(b"[LAUNCH] external recovery shell=");
    log(path);
    log(b"\r\n");
    perf(2, 2, 0);
    note_result(run(boot, working, path, b"", true, TERMINAL_MODE))
}
fn retire() -> ! {
    log(b"[LAUNCH] boot task retiring after shell admission\r\n");
    scheduler::r4os_rust_scheduler_control(13, 0, 0);
    unreachable!("Retired boot task returned")
}
fn fatal(reason: &[u8]) -> ! {
    unsafe { r4os_zig_program_shell_fatal(span(reason)) }
}
fn crash(path: &[u8]) -> ! {
    perf(5, 0, 0);
    log(b"[LAUNCH][CRASH] no external shell available; checked config=");
    log(path);
    log(b" terminal=");
    log(TERMINAL);
    log(b" recovery=");
    for path in RECOVERY {
        log(path);
        log(b" ");
    }
    log(b"\r\n");
    if boot_drive().is_null() {
        unsafe { storage_boot::r4os_rust_storage_boot(6, ptr::null()) };
    }
    let mut status = core::mem::MaybeUninit::<MscStatus>::uninit();
    unsafe { r4os_zig_program_shell_msc(status.as_mut_ptr()) };
    let status = unsafe { status.assume_init() };
    if status.present && !status.block_registered {
        fatal(unsafe { bytes(status.reason) });
    }
    if status.present && (status.read_failures != 0 || status.transport_failures != 0) {
        fatal(unsafe { bytes(status.reason) });
    }
    if boot_drive().is_null() {
        fatal(b"Boot drive C: not mounted");
    }
    fatal(b"No external shell available")
}
/// # Safety
/// The original config owner keeps path/args resident throughout boot. Drive
/// handles retain their original stable slot lifetime through every peer call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_shell_start(
    config: *const c_void,
    usable_bytes: u64,
) -> ! {
    unsafe { r4os_zig_program_shell_init(usable_bytes) };
    log(b"[LAUNCH] Legacy shell runtime bridge disabled; external shell required\r\n");
    let mut values = core::mem::MaybeUninit::<Config>::uninit();
    unsafe { r4os_zig_program_shell_config(config, values.as_mut_ptr()) };
    let values = unsafe { values.assume_init() };
    let path = unsafe { bytes(values.path) };
    let args = unsafe { bytes(values.args) };
    let boot = boot_drive();
    if !boot.is_null() {
        let working = current_drive();
        let working = if working.is_null() { boot } else { working };
        text(2, b"Dienste starten");
        service_boot(boot, working);
        text(2, b"Desktop starten");
        if configured(boot, working, path, args) == RAN {
            retire();
        }
        if !same_path(path, TERMINAL) {
            text(2, b"Terminal starten");
            match terminal(boot, working) {
                RAN => retire(),
                NOT_FOUND => {
                    text(0, b"External Terminal fallback not found: ");
                    text(0, TERMINAL);
                    text(0, b"\r\n");
                    log(b"[LAUNCH][WARN] external terminal fallback not found\r\n");
                }
                FAILED => {
                    text(0, b"External Terminal fallback failed: ");
                    text(0, TERMINAL);
                    text(0, b"\r\n");
                    log(b"[LAUNCH][WARN] external terminal fallback failed\r\n");
                }
                _ => unreachable!(),
            }
        } else {
            log(b"[LAUNCH] terminal fallback skipped: configured shell is Terminal\r\n");
        }
        for fallback in RECOVERY {
            if same_path(path, fallback) || same_path(TERMINAL, fallback) {
                continue;
            }
            match recovery(boot, working, fallback) {
                RAN => retire(),
                NOT_FOUND => {
                    text(0, b"External recovery shell not found: ");
                    text(0, fallback);
                    text(0, b"\r\n");
                    log(b"[LAUNCH][WARN] external recovery shell not found: ");
                    log(fallback);
                    log(b"\r\n");
                }
                FAILED => {
                    text(0, b"External recovery shell failed: ");
                    text(0, fallback);
                    text(0, b"\r\n");
                    log(b"[LAUNCH][WARN] external recovery shell failed: ");
                    log(fallback);
                    log(b"\r\n");
                }
                _ => unreachable!(),
            }
        }
    } else {
        text(0, b"Boot drive C: not mounted\r\n");
        log(b"[LAUNCH][WARN] C: not mounted; external shell fallback unavailable\r\n");
    }
    crash(path)
}
const _: () = {
    assert!(core::mem::size_of::<Bytes>() == 16);
    assert!(core::mem::size_of::<MscStatus>() == 40);
    assert!(core::mem::size_of::<Config>() == 32);
    assert!(core::mem::offset_of!(MscStatus, reason) == 24);
};
