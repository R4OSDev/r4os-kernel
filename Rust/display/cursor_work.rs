//! Common cursor transport. Exact execution serializes image changes; short
//! state ownership ends before BO admission, driver calls and wakeups.
use super::{
    backend_state,
    cursor_source::Source,
    cursor_state::State,
    equal::same,
    error::Error,
    ownership::{self, Resident},
};
use crate::{
    contract as a,
    driver::irq_router,
    gfx::{buffer_api, buffers, types::Owner},
    platform::monotonic,
    service::desktop_events,
};
use core::{cell::UnsafeCell, ffi::c_void, mem::MaybeUninit};
struct StateCell {
    initialized: bool,
    value: MaybeUninit<State>,
}
static STATE: Resident<StateCell> = Resident(UnsafeCell::new(StateCell {
    initialized: false,
    value: MaybeUninit::uninit(),
}));
static SOURCE: Resident<Source> = Resident(UnsafeCell::new(Source {
    reference: crate::gfx::types::Handle::EMPTY,
    borrowed: crate::gfx::types::Handle::EMPTY,
    lease: crate::gfx::types::Handle::EMPTY,
    driver: Owner {
        kind: 2,
        id: 0,
        generation: 0,
    },
}));
static ADMISSION: Resident<crate::sched::sync_types::UnwindGuard> =
    Resident(UnsafeCell::new(ownership::execution(b"display-cursor")));
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Binding {
    pub driver: Owner,
    pub backend: a::GfxBackendBinding,
    pub generation: u64,
    pub head: u32,
    pub timeline: u64,
    pub point: u64,
}

fn state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let _owner = ownership::enter_state();
    let cell = unsafe { &mut *STATE.0.get() };
    if !cell.initialized {
        cell.value.write(State::default());
        cell.initialized = true;
    }
    f(unsafe { cell.value.assume_init_mut() })
}
fn snapshot() -> State {
    state(|v| *v)
}
pub fn code(error: Error) -> i32 {
    match error {
        Error::Busy => a::gfx_output_error_busy,
        Error::Stale | Error::WrongOwner | Error::Closed => a::gfx_output_error_stale,
        Error::Unsupported | Error::Disabled => a::gfx_output_error_unsupported,
        Error::OutOfMemory
        | Error::Capacity
        | Error::Budget
        | Error::Exhausted
        | Error::Overflow => a::gfx_output_error_capacity,
        Error::Unavailable => a::gfx_output_error_unavailable,
        _ => a::gfx_output_error_invalid,
    }
}
struct Commit;
impl Commit {
    fn enter() -> Result<Self, Error> {
        if super::manager::begin_output_commit() {
            Ok(Self)
        } else {
            Err(Error::Busy)
        }
    }
}
impl Drop for Commit {
    fn drop(&mut self) {
        super::manager::end_output_commit()
    }
}
fn admission() -> Result<ownership::Execution, Error> {
    unsafe { ownership::Execution::acquire(ADMISSION.0.get()) }
}
fn now_ns() -> u64 {
    monotonic::now_nanoseconds().unwrap_or(0)
}
fn signal() {
    desktop_events::r4os_rust_desktop_signal()
}
fn validate_queue(driver: Owner, backend: a::GfxBackendBinding) -> Result<(), Error> {
    super::queue::validate_output_binding(u32::try_from(driver.id).expect("driver id"), backend)
}
fn binding() -> Result<Binding, Error> {
    super::native_driver::cursor_binding()
}

fn wake() {
    let current = snapshot();
    if current.driver.valid() && !current.lost() {
        let _ = super::queue::wake_native(
            u32::try_from(current.driver.id).expect("driver id"),
            super::queue_state::Binding {
                adapter: current.info.backend.adapter_id,
                device_generation: current.info.backend.device_generation,
                reset_generation: current.info.backend.reset_generation,
            },
        );
    }
    signal()
}
fn release_source() -> Result<(), Error> {
    buffers::lock();
    let result = unsafe { &mut *SOURCE.0.get() }.close(&mut unsafe { buffers::table() });
    buffers::unlock();
    result?;
    buffers::collect();
    Ok(())
}
fn lost(error_code: i32) {
    state(|s| s.mark_lost(error_code))
}
fn maintenance(now: u64) -> Result<(), Error> {
    let current = snapshot();
    if !current.has_job {
        return Ok(());
    }
    let job = current.job_data;
    if !current.taken
        && ((current.closing && job.request.operation != a::display_cursor_operation_release)
            || now >= job.deadline_ns)
    {
        if let Err(error) = release_source() {
            lost(code(error));
            return Err(error);
        }
        state(|s| {
            s.reject_untaken(if current.closing {
                a::gfx_output_error_stale
            } else {
                a::gfx_output_error_timeout
            })
        });
        wake();
    } else if current.taken && now >= job.deadline_ns && !current.lost() {
        lost(a::gfx_output_error_timeout);
        signal(); // An expired deadline is not a physical plane receipt.
    }
    Ok(())
}
/// # Safety
/// The caller retains the input contract record throughout this admission.
pub unsafe fn configure(driver: Owner, input: *const a::DisplayCursorInfo) -> i32 {
    let operation = || -> Result<i32, Error> {
        if irq_router::in_dispatch() || input.is_null() {
            return Err(Error::Invalid);
        }
        let _commit = Commit::enter()?;
        let _admission = admission()?;
        if unsafe { (*input).flags } == 0 {
            let current = snapshot();
            if current.info.display_generation == 0
                || current.driver != driver
                || unsafe { (*input).display_generation } != current.info.display_generation
                || unsafe { (*input).head_id } != current.info.head_id
                || !same(unsafe { &(*input).backend }, &current.info.backend)
            {
                return Err(Error::Stale);
            }
            validate_queue(driver, unsafe { (*input).backend })?;
        } else {
            let target = binding()?;
            if target.driver != driver
                || unsafe { (*input).display_generation } != target.generation
                || unsafe { (*input).head_id } != target.head
                || !same(unsafe { &(*input).backend }, &target.backend)
            {
                return Err(Error::Stale);
            }
        }
        let (configured, newly_closing) = state(|s| {
            let was_closing = s.closing;
            match s.configure(driver, unsafe { input.read() }) {
                Ok(()) => (a::gfx_output_ok, false),
                Err(error) => (code(error), !was_closing && s.closing),
            }
        });
        if newly_closing {
            wake()
        } else if configured == a::gfx_output_ok {
            signal()
        }
        Ok(configured)
    };
    operation().unwrap_or_else(code)
}
unsafe fn info(out: *mut a::DisplayCursorInfo) -> Result<i32, Error> {
    if !unsafe { buffer_api::valid_output(out) } || irq_router::in_dispatch() {
        return Err(Error::Invalid);
    }
    let current = snapshot();
    if !current.available() {
        return Err(Error::Unsupported);
    }
    let active = super::manager::backend_state();
    if active.state != backend_state::SOFTWARE_NATIVE
        || active.owner as u64 != current.driver.id
        || active.generation != current.info.display_generation
    {
        return Err(Error::Unsupported);
    }
    unsafe { out.write(current.info) };
    Ok(a::gfx_output_ok)
}
unsafe fn status(out: *mut a::DisplayCursorStatus) -> Result<i32, Error> {
    if !unsafe { buffer_api::valid_output(out) } || irq_router::in_dispatch() {
        return Err(Error::Invalid);
    }
    let _admission = admission()?;
    let _ = maintenance(now_ns());
    let current = snapshot();
    if current.info.display_generation == 0 {
        return Err(Error::Unsupported);
    }
    unsafe { out.write(current.status) };
    Ok(a::gfx_output_ok)
}
/// # Safety
/// Exact retained request and a writable output header/capacity as in R4DESK.
pub unsafe fn submit(
    caller: Owner,
    input: *const a::DisplayCursorRequest,
    out: *mut a::DisplayCursorStatus,
) -> Result<i32, Error> {
    if irq_router::in_dispatch() || input.is_null() || !unsafe { buffer_api::valid_output(out) } {
        return Err(Error::Invalid);
    }
    let _call = ownership::Call::acquire()?;
    let _commit = Commit::enter()?;
    let _admission = admission()?;
    let now = now_ns();
    maintenance(now)?;
    let target = binding()?;
    let current = snapshot();
    if target.driver != current.driver
        || target.generation != current.info.display_generation
        || !same(&target.backend, &current.info.backend)
    {
        return Err(Error::Stale);
    }
    current.validate(caller, unsafe { input.read() })?;
    if unsafe { (*input).operation } == a::display_cursor_operation_prepare {
        buffers::lock_prepared(buffers::Need {
            objects: 0,
            references: 2,
            leases: 1,
        })?;
        let result = unsafe { &mut *SOURCE.0.get() }.open(
            &mut unsafe { buffers::table() },
            caller,
            target.driver,
            unsafe { input.read() },
        );
        buffers::unlock();
        result?;
    }
    let result = state(|s| {
        s.begin(
            caller,
            unsafe { input.read() },
            unsafe { &*SOURCE.0.get() }.wire(),
            target.timeline,
            target.point,
            now,
        )?;
        Ok(s.status)
    });
    let accepted = match result {
        Ok(v) => v,
        Err(error) => {
            if let Err(release) = release_source() {
                lost(code(release));
            }
            return Err(error);
        }
    };
    unsafe { out.write(accepted) };
    wake();
    Ok(a::gfx_output_ok)
}
unsafe fn take(
    driver: Owner,
    backend: *const a::GfxBackendBinding,
    out: *mut a::GfxDriverCursorJob,
) -> Result<i32, Error> {
    if irq_router::in_dispatch() || backend.is_null() || !unsafe { buffer_api::valid_output(out) } {
        return Err(Error::Invalid);
    }
    let _admission = admission()?;
    validate_queue(driver, unsafe { backend.read() })?;
    let now = now_ns();
    maintenance(now)?;
    let job = state(|s| s.take(driver, unsafe { backend.read() }, now))?;
    if let Some(value) = job {
        unsafe { out.write(value) };
        signal();
        return Ok(a::gfx_output_ok);
    }
    Ok(0)
}
unsafe fn complete(
    driver: Owner,
    input: *const a::GfxDriverCursorCompletion,
) -> Result<i32, Error> {
    if irq_router::in_dispatch() || input.is_null() {
        return Err(Error::Invalid);
    }
    let _admission = admission()?;
    let current = snapshot();
    if !current.validate_reply(driver, unsafe { input.read() })? {
        return Ok(a::gfx_output_ok);
    }
    if unsafe { (*input).outcome } != a::gfx_output_outcome_lost {
        validate_queue(driver, current.info.backend)?;
        if let Err(error) = release_source() {
            lost(code(error));
            return Err(error);
        }
    }
    state(|s| s.finish(unsafe { input.read() }));
    wake();
    Ok(a::gfx_output_ok)
}
pub fn stopped(caller: Owner) {
    if state(|s| s.close(caller)) {
        wake()
    }
}
pub fn stopped_driver(id: u32) {
    if state(|s| {
        if id == 0 || s.driver.id != u64::from(id) || s.info.display_generation == 0 || s.lost() {
            false
        } else {
            s.mark_lost(a::gfx_output_error_unavailable);
            true
        }
    }) {
        signal()
    }
}
pub fn retire_after_reset(driver: Owner, backend: a::GfxBackendBinding) -> Result<(), Error> {
    let _admission = admission()?;
    let current = snapshot();
    if current.info.display_generation == 0 {
        return Ok(());
    }
    if current.driver != driver || !same(&current.info.backend, &backend) {
        return Err(Error::Stale);
    }
    release_source()?;
    state(|s| s.retire_after_reset(driver, backend)).expect("exact cursor reset retirement");
    signal();
    Ok(())
}
pub fn before_mode(driver: Owner) -> Result<(), Error> {
    let _admission = admission()?;
    maintenance(now_ns())?;
    let ready = state(|s| {
        if s.info.flags == 0 {
            return Ok(true);
        }
        if s.driver != driver || s.lost() {
            return Err(Error::Unavailable);
        }
        s.suspended = true;
        s.status.flags |= a::display_cursor_state_suspended;
        Ok(!s.has_job && !s.visible())
    })?;
    if !ready {
        wake();
        return Err(Error::Busy);
    }
    Ok(())
}
pub fn resume_modes() {
    if super::native_driver::mode_pending() {
        return;
    }
    state(|s| {
        s.suspended = false;
        s.status.flags &= !a::display_cursor_state_suspended;
    });
    signal()
}
/// # Safety
/// Private fixed C dispatch. Every selected input/output has its exact live
/// record and original admission; no state or ownership is mirrored in Zig.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_cursor_work(
    op: u32,
    owner: *const Owner,
    input: *const c_void,
    out: *mut c_void,
) -> i32 {
    let identity = if owner.is_null() {
        Owner::EMPTY
    } else {
        unsafe { owner.read() }
    };
    let result = match op {
        0 => return unsafe { configure(identity, input.cast()) },
        1 => unsafe { info(out.cast()) },
        2 => unsafe { status(out.cast()) },
        3 => unsafe { submit(identity, input.cast(), out.cast()) },
        4 => unsafe { take(identity, input.cast(), out.cast()) },
        5 => unsafe { complete(identity, input.cast()) },
        6 => {
            stopped(identity);
            Ok(0)
        }
        7 => {
            stopped_driver(u32::try_from(identity.id).expect("driver id"));
            Ok(0)
        }
        8 => {
            return match retire_after_reset(identity, unsafe {
                input.cast::<a::GfxBackendBinding>().read()
            }) {
                Ok(()) => 0,
                Err(e) => e as i32,
            };
        }
        9 => {
            return match before_mode(identity) {
                Ok(()) => 0,
                Err(e) => e as i32,
            };
        }
        10 => {
            resume_modes();
            Ok(0)
        }
        _ => panic!("cursor work operation"),
    };
    result.unwrap_or_else(code)
}
const _: () = {
    assert!(core::mem::size_of::<Binding>() == 88);
};
