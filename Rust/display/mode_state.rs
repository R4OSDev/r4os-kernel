//! One bounded primary mode transaction and exact late completion retention.
//! The display bridge owns images; only driver receipts establish quiescence.
use super::{equal::same, error::Error};
use crate::{contract as a, gfx::types::Owner};
use core::mem::size_of;
pub const OPERATION_NS: u64 = 30_000_000_000;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct State {
    pub status: a::GfxModeStatus,
    pub job: a::GfxDriverModeJob,
    pub has_color: bool,
    pub color_data: a::GfxDriverModeColor,
    pub caller: Owner,
    pub driver: Owner,
    pub confirmation_ns: u64,
    pub offered: bool,
    pub taken: bool,
    pub cancelled: bool,
    pub expired: bool,
    pub automatic: bool,
    pub has_reply: bool,
    pub reply_data: a::GfxDriverModeCompletion,
    pub device_retired: bool,
}
impl State {
    pub fn available(&self) -> bool {
        matches!(
            self.status.phase,
            a::gfx_mode_phase_idle | a::gfx_mode_phase_confirmed | a::gfx_mode_phase_reverted
        ) || (self.status.phase == a::gfx_mode_phase_lost && self.device_retired)
    }
    pub fn presentation_ready(&self, driver: Owner, backend: a::GfxBackendBinding) -> bool {
        self.driver == driver
            && same(&self.job.backend, &backend)
            && self.status.phase == a::gfx_mode_phase_awaiting_confirmation
            && self.status.outcome == a::gfx_output_outcome_applied
            && self.status.retained == 3
            && !self.offered
            && !self.has_reply
            && !self.cancelled
            && !self.expired
            && !self.device_retired
    }
    pub fn retire_after_reset(
        &mut self,
        driver: Owner,
        backend: a::GfxBackendBinding,
    ) -> Result<(), Error> {
        if self.status.ticket == 0 {
            return Ok(());
        }
        if self.driver != driver || !same(&self.job.backend, &backend) {
            return Err(Error::Stale);
        }
        self.status.phase = a::gfx_mode_phase_lost;
        self.status.outcome = a::gfx_output_outcome_lost;
        self.status.error_code = a::gfx_output_error_unavailable;
        self.status.retained = 0;
        self.status.operation_deadline_ns = 0;
        self.status.confirmation_deadline_ns = 0;
        self.cancelled = true;
        self.expired = true;
        self.offered = false;
        self.taken = false;
        self.has_reply = false;
        self.has_color = false;
        self.device_retired = true;
        Ok(())
    }
    pub fn begin(
        &mut self,
        caller: Owner,
        driver: Owner,
        job: a::GfxDriverModeJob,
        revision: u64,
        confirmation_ms: u32,
        now: u64,
    ) -> Result<(), Error> {
        self.begin_color(caller, driver, job, None, revision, confirmation_ms, now)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn begin_color(
        &mut self,
        caller: Owner,
        driver: Owner,
        job: a::GfxDriverModeJob,
        color: Option<a::GfxDriverModeColor>,
        revision: u64,
        confirmation_ms: u32,
        now: u64,
    ) -> Result<(), Error> {
        if !self.available() {
            return Err(Error::Busy);
        }
        if !caller.valid()
            || driver.kind != 2
            || !driver.valid()
            || job.ticket == 0
            || job.ticket <= self.status.ticket
            || !(1000..=60000).contains(&confirmation_ms)
            || now == 0
            || now > u64::MAX - OPERATION_NS
        {
            return Err(Error::Invalid);
        }
        *self = Self {
            caller,
            driver,
            job,
            has_color: color.is_some(),
            color_data: color.unwrap_or_default(),
            confirmation_ns: u64::from(confirmation_ms) * 1_000_000,
            status: a::GfxModeStatus {
                ticket: job.ticket,
                topology_revision: revision,
                output: job.assignment.output,
                retained: 3,
                ..a::GfxModeStatus::default()
            },
            ..Self::default()
        };
        self.schedule(a::gfx_mode_operation_apply, now);
        Ok(())
    }
    fn schedule(&mut self, operation: u32, now: u64) {
        self.job.sequence += 1;
        if self.has_color {
            self.color_data.ticket = self.job.ticket;
            self.color_data.sequence = self.job.sequence
        }
        self.job.operation = operation;
        self.job.deadline_ns = now.saturating_add(OPERATION_NS);
        self.status.operation_deadline_ns = self.job.deadline_ns;
        self.status.confirmation_deadline_ns = 0;
        self.status.phase = match operation {
            a::gfx_mode_operation_apply => a::gfx_mode_phase_queued,
            a::gfx_mode_operation_confirm => a::gfx_mode_phase_confirming,
            _ => a::gfx_mode_phase_reverting,
        };
        self.offered = false;
        self.taken = false;
        self.has_reply = false;
        self.expired = false;
    }
    pub fn needs_start(&self) -> bool {
        !self.offered
            && !self.has_reply
            && matches!(
                self.status.phase,
                a::gfx_mode_phase_queued
                    | a::gfx_mode_phase_confirming
                    | a::gfx_mode_phase_reverting
            )
    }
    pub fn arm(&mut self) -> Result<(), Error> {
        if !self.needs_start() {
            return Err(Error::Busy);
        }
        self.offered = true;
        Ok(())
    }
    pub fn take(
        &mut self,
        driver: Owner,
        backend: a::GfxBackendBinding,
    ) -> Result<Option<a::GfxDriverModeJob>, Error> {
        if self.driver != driver || !same(&self.job.backend, &backend) {
            return Err(Error::Stale);
        }
        if !self.offered || self.taken || self.has_reply {
            return Ok(None);
        }
        self.taken = true;
        if self.job.operation == a::gfx_mode_operation_apply {
            self.status.phase = a::gfx_mode_phase_executing
        }
        Ok(Some(self.job))
    }
    pub fn complete(
        &mut self,
        driver: Owner,
        value: a::GfxDriverModeCompletion,
    ) -> Result<(), Error> {
        if value.version != 1
            || (value.size as usize) < size_of::<a::GfxDriverModeCompletion>()
            || value.quiesced & !3 != 0
            || value.error_code > 0
        {
            return Err(Error::Invalid);
        }
        if self.driver != driver
            || !self.offered
            || !self.taken
            || self.has_reply
            || value.ticket != self.job.ticket
            || value.sequence != self.job.sequence
            || value.operation != self.job.operation
        {
            return Err(Error::Stale);
        }
        match value.outcome {
            a::gfx_output_outcome_applied => {
                if value.operation == a::gfx_mode_operation_rollback
                    || value.quiesced != 1
                    || value.error_code != 0
                {
                    return Err(Error::Invalid);
                }
            }
            a::gfx_output_outcome_old_preserved => {
                if value.operation == a::gfx_mode_operation_confirm || value.quiesced != 2 {
                    return Err(Error::Invalid);
                }
            }
            a::gfx_output_outcome_lost => {}
            _ => return Err(Error::Invalid),
        }
        self.reply_data = value;
        self.has_reply = true;
        Ok(())
    }
    pub fn read_color(
        &self,
        driver: Owner,
        ticket: u64,
        sequence: u64,
    ) -> Result<Option<a::GfxDriverModeColor>, Error> {
        if self.driver != driver
            || ticket == 0
            || ticket != self.job.ticket
            || sequence != self.job.sequence
            || !self.taken
            || !self.offered
            || self.has_reply
        {
            return Err(Error::Stale);
        }
        Ok(if self.has_color {
            Some(self.color_data)
        } else {
            None
        })
    }
    pub fn resolve(
        &mut self,
        caller: Owner,
        ticket: u64,
        action: u32,
        now: u64,
    ) -> Result<(), Error> {
        if ticket == 0 || ticket != self.status.ticket || self.caller != caller {
            return Err(Error::Stale);
        }
        if action != a::gfx_mode_resolve_confirm && action != a::gfx_mode_resolve_rollback {
            return Err(Error::Invalid);
        }
        if self.available() {
            return Err(Error::Stale);
        }
        if action == a::gfx_mode_resolve_confirm {
            if self.status.phase != a::gfx_mode_phase_awaiting_confirmation
                || self.cancelled
                || now >= self.status.confirmation_deadline_ns
            {
                return Err(Error::Busy);
            }
            self.schedule(a::gfx_mode_operation_confirm, now);
        } else {
            if self.status.phase == a::gfx_mode_phase_confirming {
                return Err(Error::Busy);
            }
            self.cancelled = true;
            if self.status.phase == a::gfx_mode_phase_awaiting_confirmation {
                self.schedule(a::gfx_mode_operation_rollback, now)
            }
        }
        Ok(())
    }
    pub fn deadline(&self) -> u64 {
        if self.status.phase == a::gfx_mode_phase_awaiting_confirmation {
            self.status.confirmation_deadline_ns
        } else {
            self.status.operation_deadline_ns
        }
    }
    pub fn output_gone(&mut self, driver_id: u32, output: a::GfxOutputId, now: u64) -> bool {
        if self.available()
            || self.driver.kind != 2
            || self.driver.id != u64::from(driver_id)
            || !same(&self.job.assignment.output, &output)
        {
            return false;
        }
        let changed =
            !self.cancelled || self.status.phase == a::gfx_mode_phase_awaiting_confirmation;
        self.cancelled = true;
        if self.status.phase == a::gfx_mode_phase_awaiting_confirmation {
            self.schedule(a::gfx_mode_operation_rollback, now)
        }
        changed
    }
    pub fn expire(&mut self, now: u64) -> bool {
        let end = self.deadline();
        if end == 0 || now < end || self.has_reply {
            return false;
        }
        self.cancelled = true;
        self.status.error_code = a::gfx_output_error_timeout;
        if self.status.phase == a::gfx_mode_phase_awaiting_confirmation
            || (!self.taken && self.job.operation == a::gfx_mode_operation_confirm)
        {
            self.schedule(a::gfx_mode_operation_rollback, now)
        } else if !self.taken && self.job.operation == a::gfx_mode_operation_apply {
            self.reject_untaken(a::gfx_output_error_timeout)
        } else {
            self.expired = true;
            self.status.phase = a::gfx_mode_phase_lost;
            self.status.outcome = a::gfx_output_outcome_lost;
            self.status.operation_deadline_ns = 0
        }
        true
    }
    pub fn reject_untaken(&mut self, error_code: i32) {
        assert!(!self.taken && !self.has_reply);
        let apply = self.job.operation == a::gfx_mode_operation_apply;
        self.reply_data = a::GfxDriverModeCompletion {
            ticket: self.job.ticket,
            sequence: self.job.sequence,
            operation: self.job.operation,
            outcome: if apply {
                a::gfx_output_outcome_old_preserved
            } else {
                a::gfx_output_outcome_lost
            },
            quiesced: if apply { 2 } else { 0 },
            error_code,
            ..a::GfxDriverModeCompletion::default()
        };
        self.has_reply = true;
    }
    pub fn settled(&mut self, result: a::GfxAtomicResult, now: u64) {
        assert!(self.has_reply);
        let value = self.reply_data;
        let operation = self.job.operation;
        self.status.topology_revision = result.topology_revision;
        self.status.commit_sequence = result.commit_sequence;
        self.status.outcome = result.outcome;
        if value.error_code != 0 {
            self.status.error_code = value.error_code
        }
        self.status.operation_deadline_ns = 0;
        self.offered = false;
        self.has_reply = false;
        if result.outcome == a::gfx_output_outcome_lost {
            self.status.phase = a::gfx_mode_phase_lost;
            self.status.retained = 3
        } else if result.outcome == a::gfx_output_outcome_old_preserved {
            self.status.phase = a::gfx_mode_phase_reverted;
            self.status.retained = 1
        } else if operation == a::gfx_mode_operation_confirm {
            self.status.phase = a::gfx_mode_phase_confirmed;
            self.status.retained = 2
        } else {
            self.status.retained = 3;
            if self.cancelled || self.expired {
                self.schedule(a::gfx_mode_operation_rollback, now)
            } else if self.automatic {
                self.schedule(a::gfx_mode_operation_confirm, now)
            } else {
                self.status.phase = a::gfx_mode_phase_awaiting_confirmation;
                self.status.confirmation_deadline_ns = now.saturating_add(self.confirmation_ns)
            }
        }
    }
}
const _: () = {
    use core::mem::{align_of, offset_of};
    assert!(
        size_of::<State>() == 696
            && align_of::<State>() == 8
            && offset_of!(State, job) == 88
            && offset_of!(State, color_data) == 432
            && offset_of!(State, caller) == 584
            && offset_of!(State, driver) == 608
            && offset_of!(State, confirmation_ns) == 632
            && offset_of!(State, reply_data) == 648
            && offset_of!(State, device_retired) == 688
    );
};

/// # Safety
/// Exact initialized C transaction under the display owner; inputs readable and outputs exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_mode_available(state: *const State) -> bool {
    unsafe { (&*state).available() }
}

/// # Safety
/// Exact initialized C transaction under the display owner; inputs readable and outputs exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_mode_presentation_ready(
    state: *const State,
    driver: *const Owner,
    backend: *const a::GfxBackendBinding,
) -> bool {
    unsafe { (&*state).presentation_ready(driver.read(), backend.read()) }
}

/// # Safety
/// Exact initialized C transaction under the display owner; inputs readable and outputs exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_mode_retire(
    state: *mut State,
    driver: *const Owner,
    backend: *const a::GfxBackendBinding,
) -> i32 {
    match unsafe { (&mut *state).retire_after_reset(driver.read(), backend.read()) } {
        Ok(()) => 0,
        Err(error) => error as i32,
    }
}

/// # Safety
/// Exact initialized C transaction under the display owner; inputs readable and outputs exclusive.
#[allow(clippy::too_many_arguments)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_mode_begin_color(
    state: *mut State,
    caller: *const Owner,
    driver: *const Owner,
    job: *const a::GfxDriverModeJob,
    color: *const a::GfxDriverModeColor,
    revision: u64,
    confirmation_ms: u32,
    now: u64,
) -> i32 {
    match unsafe {
        (&mut *state).begin_color(
            caller.read(),
            driver.read(),
            job.read(),
            if color.is_null() {
                None
            } else {
                Some(color.read())
            },
            revision,
            confirmation_ms,
            now,
        )
    } {
        Ok(()) => 0,
        Err(error) => error as i32,
    }
}

/// # Safety
/// Exact initialized C transaction under the display owner; inputs readable and outputs exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_mode_needs_start(state: *const State) -> bool {
    unsafe { (&*state).needs_start() }
}

/// # Safety
/// Exact initialized C transaction under the display owner; inputs readable and outputs exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_mode_arm(state: *mut State) -> i32 {
    match unsafe { (&mut *state).arm() } {
        Ok(()) => 0,
        Err(error) => error as i32,
    }
}

/// # Safety
/// Exact initialized C transaction under the display owner; inputs readable and outputs exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_mode_take(
    state: *mut State,
    driver: *const Owner,
    backend: *const a::GfxBackendBinding,
    out: *mut a::GfxDriverModeJob,
    has: *mut bool,
) -> i32 {
    match unsafe { (&mut *state).take(driver.read(), backend.read()) } {
        Ok(value) => {
            unsafe {
                has.write(value.is_some());
                if let Some(value) = value {
                    out.write(value)
                }
            };
            0
        }
        Err(error) => error as i32,
    }
}

/// # Safety
/// Exact initialized C transaction under the display owner; inputs readable and outputs exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_mode_complete(
    state: *mut State,
    driver: *const Owner,
    input: *const a::GfxDriverModeCompletion,
) -> i32 {
    match unsafe { (&mut *state).complete(driver.read(), input.read()) } {
        Ok(()) => 0,
        Err(error) => error as i32,
    }
}

/// # Safety
/// Exact initialized C transaction under the display owner; inputs readable and outputs exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_mode_read_color(
    state: *const State,
    driver: *const Owner,
    ticket: u64,
    sequence: u64,
    out: *mut a::GfxDriverModeColor,
    has: *mut bool,
) -> i32 {
    match unsafe { (&*state).read_color(driver.read(), ticket, sequence) } {
        Ok(value) => {
            unsafe {
                has.write(value.is_some());
                if let Some(value) = value {
                    out.write(value)
                }
            };
            0
        }
        Err(error) => error as i32,
    }
}

/// # Safety
/// Exact initialized C transaction under the display owner; inputs readable and outputs exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_mode_resolve(
    state: *mut State,
    caller: *const Owner,
    ticket: u64,
    action: u32,
    now: u64,
) -> i32 {
    match unsafe { (&mut *state).resolve(caller.read(), ticket, action, now) } {
        Ok(()) => 0,
        Err(error) => error as i32,
    }
}

/// # Safety
/// Exact initialized C transaction under the display owner; inputs readable and outputs exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_mode_deadline(state: *const State) -> u64 {
    unsafe { (&*state).deadline() }
}

/// # Safety
/// Exact initialized C transaction under the display owner; inputs readable and outputs exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_mode_output_gone(
    state: *mut State,
    driver: u32,
    output: *const a::GfxOutputId,
    now: u64,
) -> bool {
    unsafe { (&mut *state).output_gone(driver, output.read(), now) }
}

/// # Safety
/// Exact initialized C transaction under the display owner; inputs readable and outputs exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_mode_expire(state: *mut State, now: u64) -> bool {
    unsafe { (&mut *state).expire(now) }
}

/// # Safety
/// Exact initialized C transaction under the display owner; inputs readable and outputs exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_mode_reject(state: *mut State, code: i32) {
    unsafe { (&mut *state).reject_untaken(code) }
}

/// # Safety
/// Exact initialized C transaction under the display owner; inputs readable and outputs exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_mode_settled(
    state: *mut State,
    result: *const a::GfxAtomicResult,
    now: u64,
) {
    unsafe { (&mut *state).settled(result.read(), now) }
}
