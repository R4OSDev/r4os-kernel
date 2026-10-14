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
unsafe extern "C" {
    fn r4os_zig_driver_policy_query(operation: u8) -> u64;
    fn r4os_zig_driver_policy_graphics(config: *const c_void) -> bool;
    fn r4os_zig_driver_policy_rejection_probe() -> bool;
    fn r4os_zig_driver_policy_effect(operation: u8, text: *const u8, len: usize) -> bool;
}
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
    unsafe { r4os_zig_driver_policy_effect(operation, text.as_ptr(), text.len()) }
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
    if unsafe { r4os_zig_driver_policy_query(0) } == 0 {
        return fail(
            b"Driver policy before loader config",
            b"loader boot missing",
        );
    }
    let config = unsafe { r4os_zig_driver_policy_query(1) } as usize as *const c_void;
    if config.is_null() {
        return fail(b"Driver policy config missing", b"loader config missing");
    }
    change(|s| s.config_visible = true);
    if unsafe { r4os_zig_driver_policy_query(2) } == 0 {
        return fail(
            b"Driver policy before platform IRQ boot",
            b"platform irq missing",
        );
    }
    change(|s| s.platform_irq_ready = true);
    if !unsafe { r4os_zig_driver_policy_graphics(config) } {
        return fail(
            b"Graphics policy after display takeover",
            b"display owner busy",
        );
    }
    if unsafe { r4os_zig_driver_policy_query(3) } != 0
        && !unsafe { r4os_zig_driver_policy_rejection_probe() }
    {
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
