//! Original boot policy phase/order and fatal/warning accounting. Rendering,
//! loader configuration and display admission remain cuts to their owners.
use crate::{
    arch::runtime,
    driver::driver_plan::{self, Span, Summary},
};
use core::{cell::UnsafeCell, ffi::c_void, ptr};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Status {
    pub phase: u8,
    pub attempted: bool,
    pub initialized: bool,
    pub fatal: bool,
    pub config_visible: bool,
    pub platform_irq_ready: bool,
    pub core_directives_applied: bool,
    pub plan_built: bool,
    pub r4d_load_done: bool,
    pub registry_dumped: bool,
    pub summary: Summary,
    pub reason: Span,
}
fn span(value: &'static [u8]) -> Span {
    Span {
        pointer: value.as_ptr(),
        len: value.len(),
    }
}
const EMPTY: Status = Status {
    phase: 0,
    attempted: false,
    initialized: false,
    fatal: false,
    config_visible: false,
    platform_irq_ready: false,
    core_directives_applied: false,
    plan_built: false,
    r4d_load_done: false,
    registry_dumped: false,
    summary: Summary::EMPTY,
    reason: Span {
        pointer: b"not started".as_ptr(),
        len: 11,
    },
};
struct State(UnsafeCell<Status>);
unsafe impl Sync for State {}
static STATE: State = State(UnsafeCell::new(EMPTY));

fn change<R>(f: impl FnOnce(&mut Status) -> R) -> R {
    let flags = runtime::save_runtime();
    let value = unsafe { f(&mut *STATE.0.get()) };
    runtime::restore_runtime(flags);
    value
}
fn snapshot() -> Status {
    change(|s| *s)
}
fn effect(operation: u8, text: &[u8]) -> bool {
    unsafe { driver_policy_effect(operation, text.as_ptr(), text.len()) }
}
fn summary() -> Summary {
    let mut value = core::mem::MaybeUninit::uninit();
    unsafe {
        driver_plan::r4os_rust_driver_plan_summary(value.as_mut_ptr());
        value.assume_init()
    }
}
fn fail(message: &'static [u8], reason: &'static [u8]) -> bool {
    change(|s| {
        s.reason = span(reason);
        s.fatal = true;
        s.initialized = false;
        s.phase = if !s.config_visible {
            1
        } else if !s.platform_irq_ready {
            2
        } else {
            7
        };
    });
    effect(1, b"");
    effect(4, message)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_policy_init() -> bool {
    if snapshot().attempted {
        effect(0, b"");
        return !snapshot().fatal;
    }
    change(|s| {
        *s = Status {
            attempted: true,
            reason: span(b"starting"),
            ..EMPTY
        }
    });
    if unsafe { driver_policy_query(0) } == 0 {
        return fail(
            b"Driver policy before loader config",
            b"loader boot missing",
        );
    }
    let config = unsafe { driver_policy_query(1) } as usize as *const c_void;
    if config.is_null() {
        return fail(b"Driver policy config missing", b"loader config missing");
    }
    change(|s| s.config_visible = true);
    if unsafe { driver_policy_query(2) } == 0 {
        return fail(
            b"Driver policy before platform IRQ boot",
            b"platform irq missing",
        );
    }
    change(|s| s.platform_irq_ready = true);
    if !unsafe { driver_policy_graphics(config) } {
        return fail(
            b"Graphics policy after display takeover",
            b"display owner busy",
        );
    }
    if unsafe { driver_policy_query(3) } != 0 && !unsafe { driver_policy_rejection_probe() } {
        return fail(
            b"Display rollback probe failed",
            b"display rejection test failed",
        );
    }
    unsafe {
        driver_plan::r4os_rust_driver_plan_control(0, config);
        driver_plan::r4os_rust_driver_plan_control(2, config);
    }
    change(|s| {
        s.core_directives_applied = true;
        s.phase = 3;
    });
    unsafe {
        driver_plan::r4os_rust_driver_plan_control(1, config);
    }
    let result = summary();
    change(|s| {
        s.plan_built = true;
        s.r4d_load_done = true;
        s.summary = result;
        s.phase = if result.warnings == 0 { 5 } else { 6 };
        s.initialized = true;
        s.reason = if result.warnings == 0 {
            span(b"driver policy applied")
        } else {
            span(b"driver policy applied with warnings")
        };
    });
    effect(3, b"");
    let result = summary();
    change(|s| {
        s.registry_dumped = true;
        s.summary = result;
    });
    effect(1, b"");
    effect(2, b"");
    true
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_policy_initialized() -> bool {
    let s = snapshot();
    s.initialized && !s.fatal
}
/// # Safety
/// Exclusive aligned caller C output; reason points to permanent literal bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_policy_status(out: *mut Status) {
    unsafe { ptr::write(out, snapshot()) };
}

unsafe fn driver_policy_query(operation: u8) -> u64 {
    match operation {
        0 => u64::from(crate::loader::boot::r4os_rust_loader_boot_ready()),
        1 => crate::loader::boot::r4os_rust_loader_boot_config() as u64,
        2 => u64::from(crate::platform::irq_boot::initialized()),
        3 => u64::from(crate::build_config::config::DISPLAY_REJECT_TEST),
        _ => panic!("Invalid driver policy query"),
    }
}

unsafe fn driver_policy_graphics(config: *const c_void) -> bool {
    unsafe {
        let selected = crate::display::boot_policy::effective(
            (*config.cast::<crate::boot::config::Config>()).graphics,
            (*crate::boot::info::r4os_rust_boot_info()).flags & 32 != 0,
        );
        if !crate::display::manager::set_backend_policy(selected) {
            return false;
        }
        crate::boot::log::text(b"[GFX] policy=");
        crate::boot::log::text(match selected {
            0 => b"automatic",
            1 => b"software",
            2 => b"software_once",
            _ => unreachable!(),
        });
        crate::boot::log::text(b"\r\n");
        true
    }
}

unsafe fn driver_policy_rejection_probe() -> bool {
    if !crate::display::manager::probe_rejected_takeover() {
        return false;
    }
    crate::boot::log_route::puts(
        b"[GFX] rejected-takeover=OK old-owner=retained firmware-writers=open\r\n",
    );
    true
}

fn yes_no(v: bool) -> &'static [u8] {
    if v { b"yes" } else { b"no" }
}
fn phase_name(v: u8) -> &'static [u8] {
    match v {
        0 => b"not-started",
        1 => b"config-missing",
        2 => b"platform-irq-missing",
        3 => b"core-directives-applied",
        4 => b"plan-built",
        5 => b"r4d-load-done",
        6 => b"warning",
        7 => b"failed",
        _ => panic!("Invalid policy phase"),
    }
}
fn dump_status() {
    let current = snapshot();
    let summary = current.summary;
    crate::boot::log::text(b"[DRVPOLICY] status phase=");
    crate::boot::log::text(phase_name(current.phase));
    crate::boot::log::text(b" initialized=");
    crate::boot::log::text(yes_no(current.initialized));
    crate::boot::log::text(b" fatal=");
    crate::boot::log::text(yes_no(current.fatal));
    crate::boot::log::text(b" config=");
    crate::boot::log::text(yes_no(current.config_visible));
    crate::boot::log::text(b" platform_irq=");
    crate::boot::log::text(yes_no(current.platform_irq_ready));
    crate::boot::log::text(b" directives=");
    crate::boot::log::text(yes_no(current.core_directives_applied));
    crate::boot::log::text(b" plan=");
    crate::boot::log::text(yes_no(current.plan_built));
    crate::boot::log::text(b" r4d=");
    crate::boot::log::text(yes_no(current.r4d_load_done));
    crate::boot::log::text(b" registry_dump=");
    crate::boot::log::text(yes_no(current.registry_dumped));
    crate::boot::log::text(b" boot_continue=");
    crate::boot::log::text(yes_no(!current.fatal));
    crate::boot::log::text(b" config_drivers=");
    crate::boot::log::r4os_rust_bootlog_dec(summary.config_driver_count as u64);
    crate::boot::log::text(b" selected=");
    crate::boot::log::r4os_rust_bootlog_dec(summary.selected as u64);
    crate::boot::log::text(b" skipped=");
    crate::boot::log::r4os_rust_bootlog_dec(summary.skipped as u64);
    crate::boot::log::text(b" disabled=");
    crate::boot::log::r4os_rust_bootlog_dec(summary.disabled as u64);
    crate::boot::log::text(b" duplicates=");
    crate::boot::log::r4os_rust_bootlog_dec(summary.duplicates as u64);
    crate::boot::log::text(b" capacity=");
    crate::boot::log::r4os_rust_bootlog_dec(summary.capacity_limited as u64);
    crate::boot::log::text(b" already=");
    crate::boot::log::r4os_rust_bootlog_dec(summary.already_active as u64);
    crate::boot::log::text(b" loaded=");
    crate::boot::log::r4os_rust_bootlog_dec(summary.loaded as u64);
    crate::boot::log::text(b" missing=");
    crate::boot::log::r4os_rust_bootlog_dec(summary.file_missing as u64);
    crate::boot::log::text(b" invalid=");
    crate::boot::log::r4os_rust_bootlog_dec(summary.invalid_file as u64);
    crate::boot::log::text(b" name_invalid=");
    crate::boot::log::r4os_rust_bootlog_dec(summary.name_invalid as u64);
    crate::boot::log::text(b" init_failed=");
    crate::boot::log::r4os_rust_bootlog_dec(summary.init_failed as u64);
    crate::boot::log::text(b" load_failed=");
    crate::boot::log::r4os_rust_bootlog_dec(summary.load_failed as u64);
    crate::boot::log::text(b" warnings=");
    crate::boot::log::r4os_rust_bootlog_dec(summary.warnings as u64);
    crate::boot::log::text(b" registry_before=");
    crate::boot::log::r4os_rust_bootlog_dec(summary.registry_before as u64);
    crate::boot::log::text(b" registry_after=");
    crate::boot::log::r4os_rust_bootlog_dec(summary.registry_after as u64);
    crate::boot::log::text(b" ps2_disabled=");
    crate::boot::log::text(yes_no(summary.ps2_disabled));
    crate::boot::log::text(b" atapio_ignored=");
    crate::boot::log::text(yes_no(summary.atapio_disable_ignored));
    crate::boot::log::text(b" reason=");
    crate::boot::log::text(unsafe {
        core::slice::from_raw_parts(current.reason.pointer, current.reason.len)
    });
    crate::boot::log::text(b"\r\n");
}
fn emit_status() {
    let current = snapshot();
    let summary = current.summary;
    crate::boot::log_route::puts(b"  Driver policy ");
    if current.fatal {
        crate::boot::log_route::puts(b"[fatal]\r\n");
    } else if current.summary.warnings != 0 {
        crate::boot::log_route::puts(b"[warn]\r\n");
    } else {
        crate::boot::log_route::puts(b"[OK]\r\n");
    }
    crate::boot::log_route::puts(b"  Driver policy summary config=");
    crate::boot::log_route::dec(summary.config_driver_count as u64);
    crate::boot::log_route::puts(b" selected=");
    crate::boot::log_route::dec(summary.selected as u64);
    crate::boot::log_route::puts(b" skipped=");
    crate::boot::log_route::dec(summary.skipped as u64);
    crate::boot::log_route::puts(b" disabled=");
    crate::boot::log_route::dec(summary.disabled as u64);
    crate::boot::log_route::puts(b" duplicates=");
    crate::boot::log_route::dec(summary.duplicates as u64);
    crate::boot::log_route::puts(b" capacity=");
    crate::boot::log_route::dec(summary.capacity_limited as u64);
    crate::boot::log_route::puts(b" already=");
    crate::boot::log_route::dec(summary.already_active as u64);
    crate::boot::log_route::puts(b" loaded=");
    crate::boot::log_route::dec(summary.loaded as u64);
    crate::boot::log_route::puts(b" missing=");
    crate::boot::log_route::dec(summary.file_missing as u64);
    crate::boot::log_route::puts(b" invalid=");
    crate::boot::log_route::dec(summary.invalid_file as u64);
    crate::boot::log_route::puts(b" name_invalid=");
    crate::boot::log_route::dec(summary.name_invalid as u64);
    crate::boot::log_route::puts(b" init_failed=");
    crate::boot::log_route::dec(summary.init_failed as u64);
    crate::boot::log_route::puts(b" load_failed=");
    crate::boot::log_route::dec(summary.load_failed as u64);
    crate::boot::log_route::puts(b" warnings=");
    crate::boot::log_route::dec(summary.warnings as u64);
    crate::boot::log_route::puts(b" registry_before=");
    crate::boot::log_route::dec(summary.registry_before as u64);
    crate::boot::log_route::puts(b" registry_after=");
    crate::boot::log_route::dec(summary.registry_after as u64);
    crate::boot::log_route::puts(b" ps2_disabled=");
    crate::boot::log_route::puts(yes_no(summary.ps2_disabled));
    crate::boot::log_route::puts(b" atapio_ignored=");
    crate::boot::log_route::puts(yes_no(summary.atapio_disable_ignored));
    crate::boot::log_route::puts(b" continue=");
    crate::boot::log_route::puts(yes_no(!current.fatal));
    crate::boot::log_route::puts(b"\r\n");
}

unsafe fn driver_policy_effect(operation: u8, text: *const u8, len: usize) -> bool {
    match operation {
        0 => {
            let s = snapshot();
            crate::boot::log::text(b"[DRVPOLICY] init skipped phase=");
            crate::boot::log::text(phase_name(s.phase));
            crate::boot::log::text(b" boot_continue=");
            crate::boot::log::text(yes_no(!s.fatal));
            crate::boot::log::text(b"\r\n");
        }
        1 => dump_status(),
        2 => emit_status(),
        3 => {
            unsafe {
                crate::display::bootscreen::set_status(b"Treiber abschliessen".as_ptr(), 20);
            }
            crate::driver::registry::log_registry();
        }
        4 => {
            unsafe {
                crate::boot::fatal::r4os_rust_fatal_record(17, text, len);
            }
            return false;
        }
        _ => panic!("Invalid policy event"),
    };
    true
}
