//! Original bounded CONFIG.R4S parser and sole resident BSP configuration.
use crate::{
    boot::log,
    display::boot_policy,
    fs::{
        vfs,
        vfs_types::{self as t, Args, Entry, op},
    },
};
use core::{cell::UnsafeCell, mem::MaybeUninit, ptr, slice};
pub const MAX_DRIVERS: usize = 12;
pub const MAX_DISABLED: usize = 8;
pub const MAX_OPTIONS: usize = 8;
pub const MAX_SHELL_PATH: usize = 96;
pub const MAX_SHELL_ARGS: usize = 96;
const MAX_NAME: usize = 24;
const DEFAULT_SHELL_PATH: &[u8] = b"/R4OS/SOFTWARE/DESKTOP/R4DESK.R4X";
#[repr(C)]
#[derive(Clone, Copy)]
pub struct OptionRecord {
    pub driver: [u8; 24],
    pub driver_len: usize,
    pub key: [u8; 24],
    pub key_len: usize,
    pub value: [u8; 48],
    pub value_len: usize,
}
impl OptionRecord {
    pub const EMPTY: Self = Self {
        driver: [0; 24],
        driver_len: 0,
        key: [0; 24],
        key_len: 0,
        value: [0; 48],
        value_len: 0,
    };
}
#[repr(C)]
pub struct Config {
    pub graphics: u32,
    pub auto_pci: bool,
    pub auto_acpi: bool,
    pub log_verbose: bool,
    pub drivers: [[u8; 24]; 12],
    pub driver_lens: [usize; 12],
    pub driver_count: usize,
    pub disabled: [[u8; 24]; 8],
    pub disabled_lens: [usize; 8],
    pub disabled_count: usize,
    pub options: [OptionRecord; 8],
    pub option_count: usize,
    pub shell_path: [u8; 96],
    pub shell_path_len: usize,
    pub shell_args: [u8; 96],
    pub shell_args_len: usize,
}
impl Config {
    pub const EMPTY: Self = Self {
        graphics: 0,
        auto_pci: true,
        auto_acpi: true,
        log_verbose: false,
        drivers: [[0; 24]; 12],
        driver_lens: [0; 12],
        driver_count: 0,
        disabled: [[0; 24]; 8],
        disabled_lens: [0; 8],
        disabled_count: 0,
        options: [OptionRecord::EMPTY; 8],
        option_count: 0,
        shell_path: [0; 96],
        shell_path_len: 0,
        shell_args: [0; 96],
        shell_args_len: 0,
    };
}
struct Storage(UnsafeCell<Config>);
// SAFETY: original linear BSP loader alone mutates; its immutable exact config
// outlives every runtime/driver/shell observer. No reference spans VFS I/O.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(Config::EMPTY));
macro_rules! g {
    ($f:ident) => {
        (*STATE.0.get()).$f
    };
}
fn puts(text: &[u8]) {
    unsafe { log::r4os_rust_bootlog_write(text.as_ptr(), text.len()) }
}
fn dec(v: usize) {
    log::r4os_rust_bootlog_dec(v as u64)
}
unsafe fn record(start: u64, bytes: usize) {
    unsafe {
        crate::loader::performance::record_config_load(
            start,
            bytes,
            g!(driver_count),
            g!(disabled_count),
            g!(option_count),
        );
    }
}
pub(crate) unsafe fn load() -> *const Config {
    unsafe {
        let start = crate::loader::performance::begin_config_load();
        STATE.0.get().write(Config::EMPTY);
        set_shell_path(DEFAULT_SHELL_PATH);
        let Some(volume) = vfs::mounted_volume(b'C') else {
            puts(b"[CONF][WARN] C: FAT32 volume missing, using defaults\r\n");
            log_defaults();
            record(start, 0);
            return STATE.0.get();
        };
        let mut entry = MaybeUninit::<Entry>::uninit();
        let path = b"/CONFIG.R4S";
        let result = vfs::r4os_rust_vfs_run(
            op::RESOLVE_ENTRY,
            &Args {
                volume,
                name: t::Bytes {
                    data: path.as_ptr(),
                    len: path.len(),
                },
                ..Default::default()
            },
            entry.as_mut_ptr(),
        );
        if result.status != 0 {
            puts(b"[CONF][WARN] /CONFIG.R4S missing, using defaults\r\n");
            log_defaults();
            record(start, 0);
            return STATE.0.get();
        }
        let entry = entry.assume_init();
        let mut buffer = [0; 2048];
        let Some(len) = vfs::read_file_raw(volume, &entry, buffer.as_mut_ptr(), buffer.len())
        else {
            puts(b"[CONF][WARN] /CONFIG.R4S read failed, using defaults\r\n");
            log_defaults();
            record(start, 0);
            return STATE.0.get();
        };
        puts(b"[CONF] /CONFIG.R4S loaded bytes=");
        dec(len);
        puts(b"\r\n");
        let data = &buffer[..len];
        parse(if data.starts_with(&[0xef, 0xbb, 0xbf]) {
            &data[3..]
        } else {
            data
        });
        record(start, len);
        STATE.0.get()
    }
}
pub(crate) fn get() -> *const Config {
    STATE.0.get()
}
pub(crate) unsafe fn driver_name(index: usize) -> t::Bytes {
    unsafe {
        if index >= g!(driver_count) {
            return span(&[]);
        }
        span(&(&g!(drivers)[index])[..g!(driver_lens)[index]])
    }
}
pub(crate) unsafe fn disabled_name(index: usize) -> t::Bytes {
    unsafe {
        if index >= g!(disabled_count) {
            return span(&[]);
        }
        span(&(&g!(disabled)[index])[..g!(disabled_lens)[index]])
    }
}
pub(crate) unsafe fn is_disabled(config: *const Config, name: &[u8]) -> bool {
    unsafe {
        for i in 0..(*config).disabled_count {
            if (&(*config).disabled[i])[..(*config).disabled_lens[i]].eq_ignore_ascii_case(name) {
                return true;
            }
        }
        false
    }
}
pub(crate) unsafe fn has_driver(config: *const Config, name: &[u8]) -> bool {
    unsafe {
        for i in 0..(*config).driver_count {
            if (&(*config).drivers[i])[..(*config).driver_lens[i]].eq_ignore_ascii_case(name) {
                return true;
            }
        }
        false
    }
}
pub(crate) unsafe fn option_value<'a>(
    config: *const Config,
    driver: &[u8],
    key: &[u8],
) -> Option<&'a [u8]> {
    unsafe {
        for i in 0..(*config).option_count {
            let p = ptr::addr_of!((*config).options)
                .cast::<OptionRecord>()
                .add(i);
            let d = slice::from_raw_parts(ptr::addr_of!((*p).driver).cast(), (*p).driver_len);
            let k = slice::from_raw_parts(ptr::addr_of!((*p).key).cast(), (*p).key_len);
            if d.eq_ignore_ascii_case(driver) && k.eq_ignore_ascii_case(key) {
                return Some(slice::from_raw_parts(
                    ptr::addr_of!((*p).value).cast(),
                    (*p).value_len,
                ));
            }
        }
        None
    }
}
pub(crate) unsafe fn shell_path(config: *const Config) -> t::Bytes {
    unsafe {
        t::Bytes {
            data: ptr::addr_of!((*config).shell_path).cast(),
            len: (*config).shell_path_len,
        }
    }
}
pub(crate) unsafe fn shell_args(config: *const Config) -> t::Bytes {
    unsafe {
        t::Bytes {
            data: ptr::addr_of!((*config).shell_args).cast(),
            len: (*config).shell_args_len,
        }
    }
}
fn span(s: &[u8]) -> t::Bytes {
    t::Bytes {
        data: s.as_ptr(),
        len: s.len(),
    }
}
unsafe fn parse(data: &[u8]) {
    unsafe {
        let mut start = 0;
        while start < data.len() {
            let mut end = start;
            while end < data.len() && data[end] != b'\n' && data[end] != b'\r' {
                end += 1;
            }
            parse_line(trim(&data[start..end]));
            start = end;
            while start < data.len() && (data[start] == b'\n' || data[start] == b'\r') {
                start += 1;
            }
        }
        log_summary();
    }
}
unsafe fn parse_line(raw: &[u8]) {
    unsafe {
        let comment = raw.iter().position(|&b| b == b'#').unwrap_or(raw.len());
        let line = trim(&raw[..comment]);
        if line.is_empty() {
            return;
        }
        if starts(line, b"GRAPHICS=") {
            g!(graphics) = boot_policy::config_value(trim(&line[9..])).unwrap_or_else(|| {
                puts(b"[CONF][WARN] invalid GRAPHICS value, using SOFTWARE\r\n");
                1
            });
            return;
        }
        if starts(line, b"AUTO=") {
            let value = &line[5..];
            if value.eq_ignore_ascii_case(b"PCI") {
                g!(auto_pci) = true;
            }
            if value.eq_ignore_ascii_case(b"ACPI") {
                g!(auto_acpi) = true;
            }
            return;
        }
        if starts(line, b"DISABLE=") {
            add_disabled(trim(&line[8..]));
            return;
        }
        if starts(line, b"DRIVER=") {
            add_driver(trim(&line[7..]));
            return;
        }
        if starts(line, b"SHELL=") {
            set_shell_path(trim(&line[6..]));
            return;
        }
        if starts(line, b"SHELL_ARGS=") {
            set_shell_args(trim(&line[11..]));
            return;
        }
        if starts(line, b"SHELLARGS=") {
            set_shell_args(trim(&line[10..]));
            return;
        }
        if starts(line, b"LOG=") {
            g!(log_verbose) = trim(&line[4..]).eq_ignore_ascii_case(b"verbose");
            return;
        }
        if starts(line, b"OPTION ") {
            parse_option(trim(&line[7..]));
        }
    }
}
unsafe fn parse_option(rest: &[u8]) {
    unsafe {
        if g!(option_count) >= MAX_OPTIONS {
            return;
        }
        let Some(space) = rest.iter().position(|&b| b == b' ') else {
            return;
        };
        let Some(equal) = rest[space + 1..].iter().position(|&b| b == b'=') else {
            return;
        };
        let mut opt = OptionRecord::EMPTY;
        copy_name(trim(&rest[..space]), &mut opt.driver, &mut opt.driver_len);
        copy_name(
            trim(&rest[space + 1..space + 1 + equal]),
            &mut opt.key,
            &mut opt.key_len,
        );
        copy_name(
            trim(&rest[space + 1 + equal + 1..]),
            &mut opt.value,
            &mut opt.value_len,
        );
        let index = g!(option_count);
        g!(options)[index] = opt;
        g!(option_count) += 1;
    }
}
unsafe fn add_driver(name: &[u8]) {
    unsafe {
        if g!(driver_count) >= MAX_DRIVERS {
            return;
        }
        let index = g!(driver_count);
        if !valid_driver_name(name) {
            puts(b"[CONF][WARN] invalid DRIVER entry\r\n");
            g!(driver_lens)[index] = 0;
            g!(driver_count) += 1;
            return;
        }
        copy_name(name, &mut g!(drivers)[index], &mut g!(driver_lens)[index]);
        g!(driver_count) += 1;
    }
}
fn valid_driver_name(name: &[u8]) -> bool {
    !name.is_empty()
        && name.len() < MAX_NAME
        && name
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'-' || *b == b'_')
}
unsafe fn add_disabled(name: &[u8]) {
    unsafe {
        if name.is_empty() || g!(disabled_count) >= MAX_DISABLED {
            return;
        }
        let index = g!(disabled_count);
        copy_name(
            name,
            &mut g!(disabled)[index],
            &mut g!(disabled_lens)[index],
        );
        g!(disabled_count) += 1;
    }
}
unsafe fn log_defaults() {
    unsafe {
        puts(b"[CONF] defaults AUTO=PCI AUTO=ACPI DRIVER=HDA DRIVER=SID DRIVER=OPL3 DRIVER=MIDI SHELL=/R4OS/SOFTWARE/DESKTOP/R4DESK.R4X LOG=normal\r\n");
        for name in [&b"HDA"[..], &b"SID"[..], &b"OPL3"[..], &b"MIDI"[..]] {
            add_driver(name);
        }
    }
}
unsafe fn log_summary() {
    unsafe {
        puts(b"[CONF] auto pci=");
        puts(if g!(auto_pci) { b"yes" } else { b"no" });
        puts(b" acpi=");
        puts(if g!(auto_acpi) { b"yes" } else { b"no" });
        puts(b" drivers=");
        dec(g!(driver_count));
        puts(b" disabled=");
        dec(g!(disabled_count));
        puts(b" options=");
        dec(g!(option_count));
        puts(b" shell=");
        puts(&(&g!(shell_path))[..g!(shell_path_len)]);
        if g!(shell_args_len) > 0 {
            puts(b" args=");
            puts(&(&g!(shell_args))[..g!(shell_args_len)]);
        }
        puts(b"\r\n");
    }
}
fn copy_name(src: &[u8], dst: &mut [u8], len: &mut usize) {
    let n = if src.len() < dst.len() {
        src.len()
    } else {
        dst.len() - 1
    };
    for i in 0..n {
        dst[i] = src[i].to_ascii_uppercase();
    }
    if n < dst.len() {
        dst[n] = 0;
    }
    *len = n;
}
unsafe fn set_shell_path(path: &[u8]) {
    unsafe {
        if path.is_empty() {
            return;
        }
        copy_name(path, &mut g!(shell_path), &mut g!(shell_path_len));
    }
}
unsafe fn set_shell_args(args: &[u8]) {
    unsafe {
        let n = if args.len() < MAX_SHELL_ARGS {
            args.len()
        } else {
            MAX_SHELL_ARGS - 1
        };
        if n > 0 {
            (&mut g!(shell_args))[..n].copy_from_slice(&args[..n]);
        }
        if n < MAX_SHELL_ARGS {
            g!(shell_args)[n] = 0;
        }
        g!(shell_args_len) = n;
    }
}
fn trim(data: &[u8]) -> &[u8] {
    let mut a = 0;
    let mut b = data.len();
    while a < b && (data[a] == b' ' || data[a] == b'\t') {
        a += 1;
    }
    while b > a && (data[b - 1] == b' ' || data[b - 1] == b'\t') {
        b -= 1;
    }
    &data[a..b]
}
fn starts(data: &[u8], prefix: &[u8]) -> bool {
    data.len() >= prefix.len() && data[..prefix.len()].eq_ignore_ascii_case(prefix)
}
unsafe fn input<'a>(v: t::Bytes) -> &'a [u8] {
    if v.len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(v.data, v.len) }
    }
}
/// # Safety
/// BSP or immutable runtime caller retains the exact copied C config/span.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_boot_config(
    op: u32,
    config: *const Config,
    name: t::Bytes,
    key: t::Bytes,
    index: usize,
    out: *mut t::Bytes,
) -> u64 {
    unsafe {
        match op {
            0 => load() as u64,
            1 => get() as u64,
            2 => {
                out.write(driver_name(index));
                0
            }
            3 => {
                out.write(disabled_name(index));
                0
            }
            4 => u64::from(is_disabled(config, input(name))),
            5 => u64::from(has_driver(config, input(name))),
            6 => {
                if let Some(v) = option_value(config, input(name), input(key)) {
                    out.write(span(v));
                    1
                } else {
                    0
                }
            }
            7 => {
                out.write(shell_path(config));
                0
            }
            8 => {
                out.write(shell_args(config));
                0
            }
            _ => panic!("Invalid private bootconfig operation"),
        }
    }
}
const _: () = {
    assert!(size_of::<Config>() == 1840);
    assert!(size_of::<OptionRecord>() == 120);
    assert!(core::mem::offset_of!(Config, options) == 664);
    assert!(core::mem::offset_of!(Config, shell_path) == 1632);
};
