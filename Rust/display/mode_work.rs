//! Bounded asynchronous mode owner. The separate short-completion task never
//! blocks the graphics queue whose exact driver receipt it is waiting for.
use super::{
    cursor_work,
    equal::same,
    error::Error,
    mode_color,
    mode_state::{self, State},
    native_types::ModeBinding,
    output_state::Ticket,
    outputs,
    ownership::{self, Resident},
};
use crate::{
    contract as a,
    driver::irq_router,
    gfx::types::Owner,
    platform::{monotonic, timer},
    sched::{
        sync_objects,
        sync_types::{Event, UnwindGuard, WaitQueue},
        task::task_api,
    },
    service::desktop_events,
};
use core::{cell::UnsafeCell, ffi::c_void, mem::MaybeUninit, ptr};
struct StateCell {
    initialized: bool,
    value: MaybeUninit<State>,
}
static STATE: Resident<StateCell> = Resident(UnsafeCell::new(StateCell {
    initialized: false,
    value: MaybeUninit::uninit(),
}));
static BINDING: Resident<MaybeUninit<ModeBinding>> =
    Resident(UnsafeCell::new(MaybeUninit::uninit()));
static STARTED: Resident<bool> = Resident(UnsafeCell::new(false));
static WORKER_EVENT: Resident<Event> = Resident(UnsafeCell::new(Event {
    signaled: false,
    mode: 1,
    queue: WaitQueue::EMPTY,
}));
static ADMISSION: Resident<UnwindGuard> = Resident(UnsafeCell::new(ownership::execution(
    b"display-mode-admission",
)));
static COLOR: Resident<mode_color::Owner> = Resident(UnsafeCell::new(mode_color::Owner {
    reference: crate::gfx::types::Handle::EMPTY,
    driver_reference: crate::gfx::types::Handle::EMPTY,
    read: crate::gfx::types::Handle::EMPTY,
    driver: Owner {
        kind: 2,
        id: 0,
        generation: 0,
    },
}));

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
    state(|s| *s)
}
fn now_ns() -> u64 {
    monotonic::now_nanoseconds().unwrap_or(0)
}
fn signal() {
    desktop_events::r4os_rust_desktop_signal()
}
fn worker_signal() {
    unsafe { sync_objects::event_signal(WORKER_EVENT.0.get()) }
}
fn admission() -> Result<ownership::RecursiveExecution, Error> {
    unsafe { ownership::RecursiveExecution::acquire(ADMISSION.0.get()) }
}
fn validate_queue(driver: Owner, backend: a::GfxBackendBinding) -> Result<(), Error> {
    super::queue::validate_output_binding(u32::try_from(driver.id).expect("driver id"), backend)
}
// Typed private projection of the original complete mode operation.
#[allow(clippy::too_many_arguments)]
fn native(
    op: u32,
    driver: *const Owner,
    backend: *const a::GfxBackendBinding,
    binding: *const ModeBinding,
    input: *const c_void,
    out: *mut c_void,
    value: u32,
    other: u32,
) -> Result<(), Error> {
    use super::native_driver as native;
    unsafe {
        match op {
            0 => native::enable_modes(driver.read(), backend.read())?,
            1 => out.cast::<ModeBinding>().write(native::mode_binding(
                input.cast::<a::GfxScanoutState>().read(),
            )?),
            2 => {
                let request = &*input.cast::<Prepare>();
                out.cast::<a::GfxBufferReference>()
                    .write(native::prepare_mode_encoding(
                        driver.read(),
                        request.assignment,
                        request.mode,
                        if request.has_format {
                            Some(request.format)
                        } else {
                            None
                        },
                    )?);
            }
            3 => native::abort_prepared_mode(binding.read()),
            4 => native::start_mode(value, binding.read())?,
            5 => native::settle_mode_for(value, other, binding.read())?,
            6 => native::retire_mode_after_reset(driver.read(), backend.read())?,
            7 => super::queue::wake_native(
                u32::try_from((*driver).id).expect("driver id"),
                native::binding(backend.read()),
            )?,
            _ => panic!("mode native operation"),
        }
    }
    Ok(())
}

pub fn code(error: Error) -> i32 {
    match error {
        Error::Busy => a::gfx_output_error_busy,
        Error::Stale | Error::WrongOwner => a::gfx_output_error_stale,
        Error::Disabled | Error::Unsupported => a::gfx_output_error_unsupported,
        Error::Unavailable => a::gfx_output_error_unavailable,
        Error::OutOfMemory | Error::Capacity => a::gfx_output_error_capacity,
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
struct Reset;
impl Drop for Reset {
    fn drop(&mut self) {
        super::manager::end_reset_retirement()
    }
}
pub fn enable(driver: Owner, backend: a::GfxBackendBinding) -> Result<(), Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Invalid);
    }
    let _admission = admission()?;
    if !unsafe { STARTED.0.get().read() } {
        let request = task_api::CreateRequest {
            name: b"gfx-mode".as_ptr(),
            name_len: 8,
            state: 3,
            role: 1,
            needs_fpu: true,
            smp_eligible: false,
            smp_r4x: false,
            owner_kind: 0,
            owner_context: ptr::null_mut(),
            entry: Some(worker_main),
        };
        let mut failure = 0;
        let task = unsafe { task_api::r4os_rust_task_create(&request, false, &mut failure) };
        if task.is_null() {
            return Err(Error::OutOfMemory);
        }
        unsafe { STARTED.0.get().write(true) };
        assert!(
            crate::sched::scheduler::publish_created(task),
            "graphics mode worker first publication"
        );
    }
    native(
        0,
        &driver,
        &backend,
        ptr::null(),
        ptr::null(),
        ptr::null_mut(),
        0,
        0,
    )
}
pub fn status(ticket: u64) -> Result<a::GfxModeStatus, Error> {
    state(|s| {
        if ticket != 0 && ticket != s.status.ticket {
            Err(Error::Stale)
        } else {
            Ok::<_, Error>(s.status)
        }
    })
}
pub fn presentation_ready(driver: Owner, backend: a::GfxBackendBinding) -> bool {
    state(|s| s.presentation_ready(driver, backend))
}
pub fn submit(
    caller: Owner,
    input: &a::GfxAtomicState,
    confirmation_ms: u32,
) -> Result<a::GfxModeStatus, Error> {
    submit_impl(caller, input, confirmation_ms, false, None)
}
pub fn submit_color(
    caller: Owner,
    input: &a::GfxModeColorRequest,
    confirmation_ms: u32,
) -> Result<a::GfxModeStatus, Error> {
    submit_impl(caller, &input.state, confirmation_ms, false, Some(input))
}
pub fn restore(driver: Owner, input: &a::GfxAtomicState) -> Result<a::GfxModeStatus, Error> {
    if driver.kind != 2
        || !driver.valid()
        || input.version != 1
        || input.size < size_of::<a::GfxAtomicState>() as u32
        || input.count != 1
    {
        return Err(Error::Invalid);
    }
    if !outputs::native_paused(
        u32::try_from(driver.id).expect("driver id"),
        input.assignments[0].output,
    ) {
        return Err(Error::Stale);
    }
    let mut state_copy = *input;
    if state_copy.topology_revision == 0 {
        state_copy.topology_revision = outputs::revision().revision;
    }
    submit_impl(driver, &state_copy, 1000, true, None)
}
pub fn driver_status(driver: Owner, ticket: u64) -> Result<a::GfxModeStatus, Error> {
    state(|s| {
        if ticket == 0 || ticket != s.status.ticket || driver != s.driver {
            Err(Error::Stale)
        } else {
            Ok::<_, Error>(s.status)
        }
    })
}
fn submit_impl(
    caller: Owner,
    input: &a::GfxAtomicState,
    confirmation_ms: u32,
    automatic: bool,
    color: Option<&a::GfxModeColorRequest>,
) -> Result<a::GfxModeStatus, Error> {
    if irq_router::in_dispatch() || !(1000..=60000).contains(&confirmation_ms) {
        return Err(Error::Invalid);
    }
    let _admission = admission()?;
    if !unsafe { STARTED.0.get().read() } {
        return Err(Error::Unsupported);
    }
    if !snapshot().available() || !unsafe { &*COLOR.0.get() }.empty() {
        return Err(Error::Busy);
    }
    let now = now_ns();
    if now == 0 || now > u64::MAX - mode_state::OPERATION_NS {
        return Err(Error::Unavailable);
    }
    let mode = outputs::native_mode(caller, input)?;
    let _commit = Commit::enter()?;
    let mut target = MaybeUninit::<ModeBinding>::uninit();
    native(
        1,
        ptr::null(),
        ptr::null(),
        ptr::null(),
        ptr::from_ref(&input.assignments[0]).cast(),
        target.as_mut_ptr().cast(),
        0,
        0,
    )?;
    let target = unsafe { target.assume_init() };
    if !automatic
        && outputs::native_paused(
            u32::try_from(target.driver.id).expect("driver id"),
            input.assignments[0].output,
        )
    {
        return Err(Error::Busy);
    }
    if automatic && target.driver != caller {
        return Err(Error::Stale);
    }
    let color_job = match color {
        Some(request) => {
            Some(unsafe { mode_color::prepare(COLOR.0.get(), caller, target.driver, request) }?)
        }
        None => None,
    };
    let result = (|| {
        let mut reference = MaybeUninit::<a::GfxBufferReference>::uninit();
        let encoding = Prepare {
            assignment: input.assignments[0],
            mode,
            has_format: color_job.is_some(),
            format: color_job.map_or(0, |v| v.signal.format),
        };
        native(
            2,
            &caller,
            ptr::null(),
            ptr::null(),
            ptr::from_ref(&encoding).cast(),
            reference.as_mut_ptr().cast(),
            0,
            0,
        )?;
        let result = (|| {
            let accepted = outputs::begin_native(caller, input)?;
            let result = state(|s| {
                s.begin_color(
                    caller,
                    target.driver,
                    a::GfxDriverModeJob {
                        ticket: accepted.ticket.id,
                        backend: target.backend,
                        assignment: input.assignments[0],
                        mode: accepted.mode,
                        reference: unsafe { reference.assume_init() },
                        ..a::GfxDriverModeJob::default()
                    },
                    color_job,
                    accepted.revision,
                    confirmation_ms,
                    now,
                )?;
                s.automatic = automatic;
                unsafe { BINDING.0.get().write(MaybeUninit::new(target)) };
                Ok::<_, Error>(s.status)
            });
            if result.is_err() {
                let _ = outputs::finish_native(
                    accepted.ticket,
                    a::gfx_mode_operation_apply,
                    a::gfx_output_outcome_old_preserved,
                    2,
                );
            }
            result
        })();
        if result.is_err() {
            let _ = native(
                3,
                ptr::null(),
                ptr::null(),
                &target,
                ptr::null(),
                ptr::null_mut(),
                0,
                0,
            );
        }
        result
    })();
    if result.is_err() {
        let _ = unsafe { mode_color::release(COLOR.0.get()) };
    }
    let result = result?;
    worker_signal();
    signal();
    Ok(result)
}
pub fn resolve(caller: Owner, ticket: u64, action: u32) -> Result<a::GfxModeStatus, Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Invalid);
    }
    let now = now_ns();
    if now == 0 {
        return Err(Error::Unavailable);
    }
    let result = state(|s| {
        s.resolve(caller, ticket, action, now)?;
        Ok::<_, Error>(s.status)
    })?;
    worker_signal();
    signal();
    Ok(result)
}
pub fn take(
    driver: Owner,
    backend: a::GfxBackendBinding,
) -> Result<Option<a::GfxDriverModeJob>, Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Invalid);
    }
    validate_queue(driver, backend)?;
    let result = state(|s| {
        if s.status.ticket == 0 || s.available() {
            Ok(None)
        } else {
            s.take(driver, backend)
        }
    })?;
    if result.is_some() {
        signal()
    }
    Ok(result)
}
pub fn complete(driver: Owner, value: a::GfxDriverModeCompletion) -> Result<(), Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Invalid);
    }
    let before = snapshot();
    if value.outcome != a::gfx_output_outcome_lost {
        validate_queue(driver, before.job.backend)?;
    }
    state(|s| s.complete(driver, value))?;
    worker_signal();
    Ok(())
}
pub fn read_color(
    driver: Owner,
    ticket: u64,
    sequence: u64,
) -> Result<Option<a::GfxDriverModeColor>, Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Invalid);
    }
    state(|s| s.read_color(driver, ticket, sequence))
}
pub fn retire_after_reset(
    driver: Owner,
    backend: a::GfxBackendBinding,
    reset_generation: u64,
) -> Result<(), Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Invalid);
    }
    let _admission = admission()?;
    let current = snapshot();
    let matches = current.driver == driver && same(&current.job.backend, &backend);
    if current.status.ticket != 0
        && !matches
        && (!current.available() || !unsafe { &*COLOR.0.get() }.empty())
    {
        return Err(Error::Busy);
    }
    super::manager::begin_reset_retirement(
        usize::try_from(driver.id).expect("driver id"),
        reset_generation,
        backend.adapter_id,
    )?;
    let _reset = Reset;
    native(
        6,
        &driver,
        &backend,
        ptr::null(),
        ptr::null(),
        ptr::null_mut(),
        0,
        0,
    )?;
    if matches && !current.device_retired {
        unsafe { mode_color::release(COLOR.0.get()) }?;
        outputs::retire_native_after_reset(Ticket {
            id: current.status.ticket,
        })?;
        state(|s| s.retire_after_reset(driver, backend))?;
    }
    worker_signal();
    signal();
    Ok(())
}
pub fn stopped_driver(id: u32) {
    if state(|s| {
        if s.available() || s.driver.id != u64::from(id) {
            return false;
        }
        s.cancelled = true;
        s.expired = true;
        s.status.phase = a::gfx_mode_phase_lost;
        s.status.outcome = a::gfx_output_outcome_lost;
        s.status.error_code = a::gfx_output_error_unavailable;
        s.status.operation_deadline_ns = 0;
        s.status.confirmation_deadline_ns = 0;
        true
    }) {
        worker_signal()
    }
}
pub fn withdrawing_output(driver_id: u32, output: a::GfxOutputId) -> bool {
    let now = now_ns();
    let (pending, changed) = state(|s| {
        if s.available()
            || s.driver.kind != 2
            || s.driver.id != u64::from(driver_id)
            || !same(&s.job.assignment.output, &output)
        {
            return (false, false);
        }
        (true, s.output_gone(driver_id, output, now))
    });
    if changed {
        worker_signal();
        signal();
    }
    pending
}
fn rejected(error_code: i32) {
    state(|s| {
        if s.has_reply {
            return;
        }
        if !s.taken {
            s.reject_untaken(error_code);
        } else {
            s.cancelled = true;
            s.expired = true;
            s.status.phase = a::gfx_mode_phase_lost;
            s.status.outcome = a::gfx_output_outcome_lost;
            s.status.error_code = error_code;
            s.status.operation_deadline_ns = 0;
        }
    })
}
// Retain the original checked arithmetic and overflow boundary.
#[allow(clippy::manual_div_ceil)]
fn ticks_until(deadline: u64, now: u64) -> u64 {
    if deadline == 0 {
        return u64::MAX;
    }
    if deadline <= now {
        return 1;
    }
    u64::try_from(
        (u128::from(deadline - now) * u128::from(timer::frequency().max(1)) + 1_000_000_000 - 1)
            / 1_000_000_000,
    )
    .expect("mode event delay")
    .max(1)
}
#[derive(Default)]
struct Worker {
    locked: bool,
    hidden_ticket: u64,
    hidden_sequence: u64,
}
// Original receipt/validation branch scopes retained during the port.
#[allow(clippy::collapsible_if)]
fn worker_step(worker: &mut Worker) -> u64 {
    let now = now_ns();
    if state(|s| s.expire(now)) {
        signal()
    }
    let mut current = snapshot();
    if current.cancelled
        && current.job.operation == a::gfx_mode_operation_apply
        && !current.taken
        && !current.has_reply
        && current.status.phase != a::gfx_mode_phase_lost
    {
        rejected(0);
        current = snapshot();
    }
    let needs_lock = current.needs_start()
        || current.has_reply
        || (current.status.phase == a::gfx_mode_phase_lost
            && !current.device_retired
            && (worker.hidden_ticket != current.status.ticket
                || worker.hidden_sequence != current.job.sequence));
    // A binding is published by begin before any work becomes actionable.
    let target = if needs_lock {
        Some(unsafe { (*BINDING.0.get()).assume_init_read() })
    } else {
        None
    };
    if needs_lock && !target.expect("mode binding").has_additional && !worker.locked {
        worker.locked = super::manager::begin_output_commit();
        if !worker.locked {
            return 1;
        }
    }
    if current.needs_start() {
        let target = target.expect("mode binding");
        if !target.has_additional {
            if let Err(error) = cursor_work::before_mode(target.driver) {
                if error == Error::Busy {
                    return 1;
                }
                rejected(cursor_work::code(error));
                return 0;
            }
        }
        if let Err(error) = native(
            4,
            ptr::null(),
            ptr::null(),
            &target,
            ptr::null(),
            ptr::null_mut(),
            current.job.operation,
            0,
        ) {
            if error == Error::Busy {
                return 1;
            }
            rejected(code(error));
            return 0;
        }
        let _ = state(|s| s.arm());
        if let Err(error) = native(
            7,
            &current.driver,
            &current.job.backend,
            ptr::null(),
            ptr::null(),
            ptr::null_mut(),
            0,
            0,
        ) {
            rejected(code(error));
        }
        return 0;
    }
    if current.has_reply {
        let receipt = current.reply_data;
        let mut outcome = receipt.outcome;
        if validate_queue(current.driver, current.job.backend).is_err() {
            outcome = a::gfx_output_outcome_lost;
        }
        if outputs::can_finish_native(Ticket { id: receipt.ticket }, receipt.operation).is_err() {
            outcome = a::gfx_output_outcome_lost;
        }
        let target = target.expect("mode binding");
        if let Err(error) = native(
            5,
            ptr::null(),
            ptr::null(),
            &target,
            ptr::null(),
            ptr::null_mut(),
            receipt.operation,
            outcome,
        ) {
            outcome = a::gfx_output_outcome_lost;
            let _ = native(
                5,
                ptr::null(),
                ptr::null(),
                &target,
                ptr::null(),
                ptr::null_mut(),
                receipt.operation,
                outcome,
            );
            state(|s| s.status.error_code = code(error));
        }
        if outcome != a::gfx_output_outcome_lost
            && (receipt.operation == a::gfx_mode_operation_confirm
                || outcome == a::gfx_output_outcome_old_preserved)
            && unsafe { mode_color::release(COLOR.0.get()) }.is_err()
        {
            outcome = a::gfx_output_outcome_lost;
        }
        let result = outputs::finish_native(
            Ticket { id: receipt.ticket },
            receipt.operation,
            outcome,
            if outcome == a::gfx_output_outcome_lost {
                0
            } else {
                receipt.quiesced
            },
        )
        .unwrap_or(a::GfxAtomicResult {
            outcome: a::gfx_output_outcome_lost,
            retained: 3,
            ..a::GfxAtomicResult::default()
        });
        state(|s| s.settled(result, now));
        signal();
        return 0;
    }
    if current.status.phase == a::gfx_mode_phase_lost
        && !current.device_retired
        && (worker.hidden_ticket != current.status.ticket
            || worker.hidden_sequence != current.job.sequence)
    {
        let target = target.expect("mode binding");
        let _ = native(
            5,
            ptr::null(),
            ptr::null(),
            &target,
            ptr::null(),
            ptr::null_mut(),
            current.job.operation,
            a::gfx_output_outcome_lost,
        );
        worker.hidden_ticket = current.status.ticket;
        worker.hidden_sequence = current.job.sequence;
        signal();
    }
    if worker.locked && (!current.offered || current.expired) {
        cursor_work::resume_modes();
        super::manager::end_output_commit();
        worker.locked = false;
    }
    ticks_until(current.deadline(), now)
}
unsafe extern "C" fn worker_main() {
    let mut worker = Worker::default();
    loop {
        let delay = match admission() {
            Ok(_admission) => worker_step(&mut worker),
            Err(_) => 1,
        };
        if delay != 0 {
            let _ = unsafe { sync_objects::event_wait(WORKER_EVENT.0.get(), delay) };
        }
    }
}
#[repr(C)]
pub struct Prepare {
    pub assignment: a::GfxScanoutState,
    pub mode: a::GfxOutputMode,
    pub has_format: bool,
    pub format: u32,
}
#[repr(C)]
pub struct Args {
    pub owner: Owner,
    pub backend: a::GfxBackendBinding,
    pub ticket: u64,
    pub value: u32,
    pub sequence: u64,
    pub input: *const c_void,
}
#[repr(C)]
pub struct Optional<T> {
    pub has: bool,
    pub value: MaybeUninit<T>,
}
/// # Safety
/// Exact fixed C arguments. Contract payloads and outputs remain retained for
/// the original admitted operation; callbacks and storage never mirror state.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_mode_work(
    op: u32,
    args: *const Args,
    out: *mut c_void,
) -> i32 {
    let args = unsafe { &*args };
    let result = match op {
        0 => enable(args.owner, args.backend),
        1 => status(args.ticket).map(|v| unsafe { out.cast::<a::GfxModeStatus>().write(v) }),
        2 => {
            unsafe {
                out.cast::<bool>()
                    .write(presentation_ready(args.owner, args.backend))
            };
            Ok(())
        }
        3 => submit(
            args.owner,
            unsafe { &*args.input.cast::<a::GfxAtomicState>() },
            args.value,
        )
        .map(|v| unsafe { out.cast::<a::GfxModeStatus>().write(v) }),
        4 => submit_color(
            args.owner,
            unsafe { &*args.input.cast::<a::GfxModeColorRequest>() },
            args.value,
        )
        .map(|v| unsafe { out.cast::<a::GfxModeStatus>().write(v) }),
        5 => restore(args.owner, unsafe {
            &*args.input.cast::<a::GfxAtomicState>()
        })
        .map(|v| unsafe { out.cast::<a::GfxModeStatus>().write(v) }),
        6 => driver_status(args.owner, args.ticket)
            .map(|v| unsafe { out.cast::<a::GfxModeStatus>().write(v) }),
        7 => resolve(args.owner, args.ticket, args.value)
            .map(|v| unsafe { out.cast::<a::GfxModeStatus>().write(v) }),
        8 => take(args.owner, args.backend).map(|v| unsafe {
            out.cast::<Optional<a::GfxDriverModeJob>>().write(Optional {
                has: v.is_some(),
                value: v.map_or(MaybeUninit::uninit(), MaybeUninit::new),
            })
        }),
        9 => complete(args.owner, unsafe {
            args.input.cast::<a::GfxDriverModeCompletion>().read()
        }),
        10 => read_color(args.owner, args.ticket, args.sequence).map(|v| unsafe {
            out.cast::<Optional<a::GfxDriverModeColor>>()
                .write(Optional {
                    has: v.is_some(),
                    value: v.map_or(MaybeUninit::uninit(), MaybeUninit::new),
                })
        }),
        11 => retire_after_reset(args.owner, args.backend, args.sequence),
        12 => {
            stopped_driver(u32::try_from(args.owner.id).expect("driver id"));
            Ok(())
        }
        13 => {
            unsafe {
                out.cast::<bool>().write(withdrawing_output(
                    u32::try_from(args.owner.id).expect("driver id"),
                    args.input.cast::<a::GfxOutputId>().read(),
                ))
            };
            Ok(())
        }
        _ => panic!("mode work operation"),
    };
    match result {
        Ok(()) => 0,
        Err(error) => error as i32,
    }
}
