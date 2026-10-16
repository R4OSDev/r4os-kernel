//! Original boot load plan, directives, accounting and optional driver results.
//! Only metadata access holds Runtime; no state reference spans R4D or display.
use crate::{arch::runtime, driver::registry};
use core::{cell::UnsafeCell, ffi::c_void, mem, ptr, slice};
pub const MAX_PLAN: usize = 12;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Summary {
    pub run_started: bool,
    pub config_driver_count: usize,
    pub selected: usize,
    pub skipped: usize,
    pub disabled: usize,
    pub duplicates: usize,
    pub capacity_limited: usize,
    pub already_active: usize,
    pub loaded: usize,
    pub file_missing: usize,
    pub invalid_file: usize,
    pub name_invalid: usize,
    pub init_failed: usize,
    pub load_failed: usize,
    pub warnings: usize,
    pub ps2_disabled: bool,
    pub ps2_registry_missing: bool,
    pub atapio_disable_ignored: bool,
    pub registry_before: usize,
    pub registry_after: usize,
    pub boot_continue: bool,
}
impl Summary {
    pub const EMPTY: Self = Self {
        run_started: false,
        config_driver_count: 0,
        selected: 0,
        skipped: 0,
        disabled: 0,
        duplicates: 0,
        capacity_limited: 0,
        already_active: 0,
        loaded: 0,
        file_missing: 0,
        invalid_file: 0,
        name_invalid: 0,
        init_failed: 0,
        load_failed: 0,
        warnings: 0,
        ps2_disabled: false,
        ps2_registry_missing: false,
        atapio_disable_ignored: false,
        registry_before: 0,
        registry_after: 0,
        boot_continue: true,
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Span {
    pub pointer: *const u8,
    pub len: usize,
}
#[derive(Clone, Copy)]
struct Planned {
    name: [u8; 24],
    len: usize,
}
impl Planned {
    const EMPTY: Self = Self {
        name: [0; 24],
        len: 0,
    };
}
struct State {
    plan: [Planned; MAX_PLAN],
    count: usize,
    summary: Summary,
}
struct Shared(UnsafeCell<State>);
unsafe impl Sync for Shared {}
static STATE: Shared = Shared(UnsafeCell::new(State {
    plan: [Planned::EMPTY; MAX_PLAN],
    count: 0,
    summary: Summary::EMPTY,
}));

fn change<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let flags = runtime::save_runtime();
    let result = unsafe { f(&mut *STATE.0.get()) };
    runtime::restore_runtime(flags);
    result
}
fn snapshot() -> Summary {
    change(|s| s.summary)
}
unsafe fn input(config: *const c_void, operation: u8, index: usize) -> u64 {
    unsafe { driver_plan_config(config, operation, index, ptr::null_mut()) }
}
unsafe fn name<'a>(config: *const c_void, operation: u8, index: usize) -> &'a [u8] {
    let mut out = Span {
        pointer: ptr::NonNull::dangling().as_ptr(),
        len: 0,
    };
    unsafe {
        driver_plan_config(config, operation, index, &mut out);
        if out.len == 0 {
            &[]
        } else {
            slice::from_raw_parts(out.pointer, out.len)
        }
    }
}
fn effect(operation: u8, name: &[u8], value: u32) {
    unsafe { driver_plan_effect(operation, name.as_ptr(), name.len(), value) };
}
unsafe fn begin(config: *const c_void) {
    let count = unsafe { input(config, 0, 0) } as usize;
    let before = registry::r4os_rust_driver_registry_count();
    let after = registry::r4os_rust_driver_registry_count();
    change(|s| {
        s.summary = Summary {
            run_started: true,
            config_driver_count: count,
            registry_before: before,
            registry_after: after,
            ..Summary::EMPTY
        }
    });
}
unsafe fn ensure(config: *const c_void) {
    if !snapshot().run_started {
        unsafe { begin(config) };
    }
}
unsafe fn add(config: *const c_void, name: &[u8]) {
    if name.is_empty() {
        change(|s| {
            s.summary.skipped += 1;
            s.summary.name_invalid += 1;
            s.summary.warnings += 1;
        });
        return;
    }
    if change(|s| s.count) >= MAX_PLAN {
        effect(3, name, 0);
        change(|s| {
            s.summary.skipped += 1;
            s.summary.capacity_limited += 1;
            s.summary.warnings += 1;
        });
        return;
    }
    for index in 0..unsafe { input(config, 1, 0) } as usize {
        if name.eq_ignore_ascii_case(unsafe { self::name(config, 5, index) }) {
            effect(4, name, 0);
            change(|s| {
                s.summary.skipped += 1;
                s.summary.disabled += 1;
            });
            return;
        }
    }
    if change(|s| (0..s.count).any(|i| s.plan[i].name[..s.plan[i].len].eq_ignore_ascii_case(name)))
    {
        effect(5, name, 0);
        change(|s| {
            s.summary.skipped += 1;
            s.summary.duplicates += 1;
        });
        return;
    }
    change(|s| {
        let target = &mut s.plan[s.count];
        let len = name.len().min(23);
        for (output, input) in target.name[..len].iter_mut().zip(&name[..len]) {
            *output = input.to_ascii_uppercase();
        }
        target.name[len] = 0;
        target.len = len;
        s.count += 1;
    });
}
fn record(result: u8) {
    change(|s| match result {
        0 => s.summary.loaded += 1,
        1 => s.summary.already_active += 1,
        2 => {
            s.summary.name_invalid += 1;
            s.summary.warnings += 1;
        }
        3 => {
            s.summary.file_missing += 1;
            s.summary.warnings += 1;
        }
        4 => {
            s.summary.invalid_file += 1;
            s.summary.warnings += 1;
        }
        5 => {
            s.summary.init_failed += 1;
            s.summary.warnings += 1;
        }
        6 => {
            s.summary.load_failed += 1;
            s.summary.warnings += 1;
        }
        _ => panic!("Invalid R4D load result"),
    });
}
fn load(name: &[u8]) {
    effect(7, name, 0);
    let slot = unsafe { registry::r4os_rust_driver_registry_find(name.as_ptr(), name.len()) };
    if slot != usize::MAX {
        let mut entry = mem::MaybeUninit::<registry::Entry>::uninit();
        if unsafe { registry::r4os_rust_driver_registry_get(slot, entry.as_mut_ptr()) } {
            let entry = unsafe { entry.assume_init() };
            if matches!(entry.state, 1..=3) {
                effect(
                    8,
                    name,
                    u32::from(entry.source) | (u32::from(entry.state) << 8),
                );
                change(|s| s.summary.already_active += 1);
                return;
            }
        }
    }
    let result = unsafe { driver_plan_load(name.as_ptr(), name.len()) };
    record(result);
    let audio_result = match result {
        0 => 0,
        1 => 1,
        3 => 2,
        5 => 3,
        _ => 4,
    };
    if name.eq_ignore_ascii_case(b"AC97") {
        effect(10, name, audio_result);
    } else if name.eq_ignore_ascii_case(b"HDA") {
        effect(11, name, audio_result);
    }
    if result > 1 {
        effect(9, name, u32::from(result));
    }
}
/// # Safety
/// Opaque configuration belongs to the original boot owner, live through every
/// call; effects retain their own exact device/module/backing admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_plan_control(operation: u8, config: *const c_void) {
    unsafe {
        match operation {
            0 => begin(config),
            1 => {
                ensure(config);
                change(|s| s.count = 0);
                effect(0, b"", 0);
                if input(config, 2, 0) != 0 {
                    effect(1, b"", 0);
                }
                if input(config, 3, 0) != 0 {
                    effect(2, b"", 0);
                }
                for i in 0..input(config, 0, 0) as usize {
                    add(config, name(config, 4, i));
                }
                let count = change(|s| {
                    s.summary.selected = s.count;
                    s.count
                });
                effect(
                    6,
                    b"",
                    u32::try_from(count).expect("Driver plan count exceeds u32"),
                );
                for i in 0..count {
                    let planned = change(|s| s.plan[i]);
                    load(&planned.name[..planned.len]);
                }
                let after = registry::r4os_rust_driver_registry_count();
                change(|s| s.summary.registry_after = after);
            }
            2 => {
                ensure(config);
                if driver_plan_disabled(config, b"PS2".as_ptr(), 3) {
                    effect(12, b"", 0);
                    let slot = registry::r4os_rust_driver_registry_find(b"PS2".as_ptr(), 3);
                    if slot != usize::MAX {
                        effect(13, b"", slot as u32);
                        change(|s| s.summary.ps2_disabled = true);
                    } else {
                        effect(14, b"", 0);
                        change(|s| {
                            s.summary.ps2_registry_missing = true;
                            s.summary.warnings += 1;
                        });
                    }
                }
                if driver_plan_disabled(config, b"ATAPIO".as_ptr(), 6) {
                    effect(15, b"", 0);
                    change(|s| {
                        s.summary.atapio_disable_ignored = true;
                        s.summary.warnings += 1;
                    });
                }
                let after = registry::r4os_rust_driver_registry_count();
                change(|s| s.summary.registry_after = after);
            }
            _ => panic!("Invalid driver plan operation"),
        }
    }
}
/// # Safety
/// Exclusive aligned caller C output. Snapshot has no borrowed dynamic bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_plan_summary(out: *mut Summary) {
    unsafe { out.write(snapshot()) };
}

unsafe fn driver_plan_config(
    config: *const c_void,
    operation: u8,
    index: usize,
    out: *mut Span,
) -> u64 {
    unsafe {
        let c = config.cast::<crate::boot::config::Config>();
        match operation {
            0 => (*c).driver_count as u64,
            1 => (*c).disabled_count as u64,
            2 => u64::from((*c).auto_acpi),
            3 => u64::from((*c).auto_pci),
            4 | 5 => {
                let v = if operation == 4 {
                    crate::boot::config::driver_name(index)
                } else {
                    crate::boot::config::disabled_name(index)
                };
                out.write(Span {
                    pointer: v.data,
                    len: v.len,
                });
                0
            }
            _ => panic!("Invalid driver plan input"),
        }
    }
}

unsafe fn driver_plan_disabled(config: *const c_void, name: *const u8, len: usize) -> bool {
    unsafe {
        crate::boot::config::is_disabled(config.cast(), core::slice::from_raw_parts(name, len))
    }
}

unsafe fn driver_plan_load(name: *const u8, len: usize) -> u8 {
    unsafe {
        crate::loader::r4d::r4os_rust_r4d_load(crate::fs::vfs_types::Bytes { data: name, len })
    }
}

unsafe fn driver_plan_effect(operation: u8, name: *const u8, len: usize, value: u32) {
    use crate::boot::log as l;
    let name = unsafe { core::slice::from_raw_parts(name, len) };
    match operation {
        0 => l::text(b"[PLAN] build\r\n"),
        1 => l::text(b"[PLAN] AUTO=ACPI enabled\r\n"),
        2 => l::text(b"[PLAN] AUTO=PCI enabled\r\n"),
        3..=5 => {
            l::text(if operation == 3 {
                b"[PLAN][WARN] skip "
            } else {
                b"[PLAN] skip "
            });
            l::text(name);
            l::text(match operation {
                3 => b" plan capacity reached\r\n",
                4 => b" disabled by CONFIG.R4S\r\n",
                5 => b" already planned\r\n",
                _ => unreachable!(),
            });
        }
        6 => {
            l::text(b"[PLAN] drivers selected=");
            l::r4os_rust_bootlog_dec(u64::from(value));
            l::text(b"\r\n");
        }
        7 => {
            unsafe {
                crate::display::bootscreen::driver(name.as_ptr(), name.len());
            }
            l::text(b"[PLAN] load ");
            l::text(name);
            l::text(b"\r\n");
        }
        8 => {
            l::text(b"[PLAN] ");
            l::text(name);
            l::text(b" already ");
            l::text(registry::source_name((value & 255).try_into().unwrap()));
            l::text(b"/");
            l::text(registry::state_name((value >> 8).try_into().unwrap()));
            l::text(b"\r\n");
        }
        9 => {
            if matches!(value, 2 | 3 | 4 | 6) {
                unsafe {
                    crate::display::bootscreen::driver_error(name.as_ptr(), name.len());
                }
            }
            l::text(b"[PLAN][WARN] load failed ");
            l::text(name);
            l::text(b" result=");
            let r = crate::loader::r4d::r4os_rust_r4d_result_name(value.try_into().unwrap());
            unsafe {
                l::text(core::slice::from_raw_parts(r.data, r.len));
            }
            l::text(b"\r\n");
        }
        10 => unsafe { crate::audio::boot::report_ac97(value) },
        11 => unsafe { crate::audio::boot::report_hda(value) },
        12 => l::text(b"[PLAN] disable PS2 core input\r\n"),
        13 => {
            registry::set_state_logged(value as usize, 4);
            crate::input::keyboard::disable();
            crate::input::mouse::disable();
            registry::set_state_logged(value as usize, 5);
        }
        14 => l::text(b"[PLAN][WARN] PS2 registry entry missing\r\n"),
        15 => l::text(b"[PLAN][WARN] DISABLE=ATAPIO ignored after boot-storage init\r\n"),
        _ => panic!("Invalid driver plan event"),
    }
}
