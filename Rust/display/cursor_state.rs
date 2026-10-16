//! One primary cursor, one finite exact driver job. Metadata only; image
//! ownership and physical visibility remain separate barriers at their owners.
use super::{equal::same, error::Error};
use crate::{contract as a, gfx::types::Owner};
use core::mem::size_of;
pub const OPERATION_NS: u64 = 3_000_000_000;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct State {
    pub info: a::DisplayCursorInfo,
    pub driver: Owner,
    pub has_actor: bool,
    pub actor_data: Owner,
    pub status: a::DisplayCursorStatus,
    pub has_job: bool,
    pub job_data: a::GfxDriverCursorJob,
    pub has_reply: bool,
    pub reply_data: a::GfxDriverCursorCompletion,
    pub taken: bool,
    pub closing: bool,
    pub suspended: bool,
}
impl Default for State {
    fn default() -> Self {
        Self {
            info: a::DisplayCursorInfo::default(),
            driver: Owner {
                kind: 2,
                id: 0,
                generation: 0,
            },
            has_actor: false,
            actor_data: Owner::EMPTY,
            status: a::DisplayCursorStatus::default(),
            has_job: false,
            job_data: a::GfxDriverCursorJob::default(),
            has_reply: false,
            reply_data: a::GfxDriverCursorCompletion::default(),
            taken: false,
            closing: false,
            suspended: false,
        }
    }
}
impl State {
    pub fn configure(&mut self, driver: Owner, input: a::DisplayCursorInfo) -> Result<(), Error> {
        if input.version != 1
            || (input.size as usize) < size_of::<a::DisplayCursorInfo>()
            || input.display_generation == 0
            || driver.kind != 2
            || !driver.valid()
            || input.backend.version != 1
            || (input.backend.size as usize) < size_of::<a::GfxBackendBinding>()
            || input.backend.adapter_id == 0
            || input.backend.device_generation == 0
            || input.backend.reset_generation == 0
            || input.backend.milestone != a::gfx_queue_milestone_device_execution
            || input.head_id >= 8
        {
            return Err(Error::Invalid);
        }
        if self.info.display_generation != 0
            && (self.driver != driver
                || self.info.display_generation != input.display_generation
                || !same(&self.info.backend, &input.backend)
                || self.info.head_id != input.head_id)
        {
            return Err(Error::Stale);
        }
        if input.flags != 0
            && (input.flags != 15
                || input.max_width == 0
                || input.max_width > 256
                || input.max_height == 0
                || input.max_height > 256
                || input.min_x > 0
                || input.min_y > 0
                || input.max_x <= 0
                || input.max_y <= 0)
        {
            return Err(Error::Invalid);
        }
        if input.flags == 0 && (self.has_job || self.has_actor || self.visible() || self.lost()) {
            if !self.lost() && self.has_actor {
                self.closing = true
            }
            return Err(Error::Busy);
        }
        if self.info.flags != 0 && input.flags != 0 && !same(&self.info, &input) {
            return Err(Error::Stale);
        }
        if self.info.display_generation == 0 {
            self.status = a::DisplayCursorStatus {
                sequence: self.status.sequence,
                completed: self.status.completed,
                ..a::DisplayCursorStatus::default()
            }
        }
        self.info = input;
        self.info.size = size_of::<a::DisplayCursorInfo>() as u32;
        self.driver = driver;
        self.status.display_generation = input.display_generation;
        self.status.head_id = input.head_id;
        Ok(())
    }
    pub fn visible(&self) -> bool {
        self.status.flags & a::display_cursor_state_visible != 0
    }
    pub fn lost(&self) -> bool {
        self.status.phase == a::display_cursor_phase_lost
    }
    pub fn available(&self) -> bool {
        self.info.flags != 0 && !self.lost()
    }
    pub fn validate(&self, caller: Owner, request: a::DisplayCursorRequest) -> Result<(), Error> {
        if !self.available() {
            return Err(Error::Unsupported);
        }
        if !caller.valid()
            || caller.kind != 1
            || request.version != 1
            || (request.size as usize) < size_of::<a::DisplayCursorRequest>()
            || request.operation > a::display_cursor_operation_release
        {
            return Err(Error::Invalid);
        }
        if request.display_generation != self.info.display_generation
            || request.head_id != self.info.head_id
        {
            return Err(Error::Stale);
        }
        if self.has_job || self.closing || self.suspended {
            return Err(Error::Busy);
        }
        if self.has_actor && self.actor_data != caller {
            return Err(Error::Busy);
        }
        if self.status.sequence == u64::MAX {
            return Err(Error::Exhausted);
        }
        if request.operation == a::display_cursor_operation_prepare {
            if self.visible() {
                return Err(Error::Busy);
            }
            if request.reference.id == 0
                || request.reference.generation == 0
                || request.reference.reserved0 != 0
                || request.image_sequence != 0
                || request.x != 0
                || request.y != 0
                || request.width == 0
                || request.height == 0
                || request.width > self.info.max_width
                || request.height > self.info.max_height
                || request.hotspot_x >= request.width
                || request.hotspot_y >= request.height
                || request.pitch & 3 != 0
                || request.pitch < u64::from(request.width) * 4
                || request.pitch > u64::MAX / u64::from(request.height)
                || request.pitch * u64::from(request.height) != request.byte_length
            {
                return Err(Error::Invalid);
            }
        } else {
            if !same(&request.reference, &a::GfxBufferHandle::default())
                || request.width != 0
                || request.height != 0
                || request.pitch != 0
                || request.byte_length != 0
                || request.hotspot_x != 0
                || request.hotspot_y != 0
            {
                return Err(Error::Invalid);
            }
            if !self.has_actor {
                return Err(Error::Stale);
            }
            if request.operation == a::display_cursor_operation_show
                || request.operation == a::display_cursor_operation_move
            {
                if request.image_sequence == 0
                    || request.image_sequence != self.status.image_sequence
                {
                    return Err(Error::Stale);
                }
                if request.operation == a::display_cursor_operation_move && !self.visible() {
                    return Err(Error::Invalid);
                }
                if request.x < self.info.min_x
                    || request.y < self.info.min_y
                    || request.x > self.info.max_x
                    || request.y > self.info.max_y
                {
                    return Err(Error::Invalid);
                }
            } else if request.image_sequence != 0 || request.x != 0 || request.y != 0 {
                return Err(Error::Invalid);
            }
        }
        Ok(())
    }
    pub fn begin(
        &mut self,
        caller: Owner,
        request: a::DisplayCursorRequest,
        reference: a::GfxBufferHandle,
        timeline: u64,
        point: u64,
        now: u64,
    ) -> Result<(), Error> {
        self.validate(caller, request)?;
        if now == 0 || now >= u64::MAX - OPERATION_NS || (timeline == 0) != (point == 0) {
            return Err(Error::Invalid);
        }
        if request.operation == a::display_cursor_operation_prepare
            && (reference.id == 0
                || reference.generation == 0
                || reference.reserved0 != 0
                || same(&reference, &request.reference))
        {
            return Err(Error::Invalid);
        }
        self.has_actor = true;
        self.actor_data = caller;
        self.arm(request, now);
        if request.operation == a::display_cursor_operation_prepare {
            self.job_data.request.reference = reference
        }
        if request.operation == a::display_cursor_operation_show {
            self.job_data.barrier_timeline = timeline;
            self.job_data.barrier_point = point
        }
        Ok(())
    }
    fn arm(&mut self, request: a::DisplayCursorRequest, now: u64) {
        self.status.sequence += 1;
        self.status.phase = a::display_cursor_phase_queued;
        self.status.deadline_ns = now + OPERATION_NS;
        self.status.error_code = 0;
        self.status.flags |= a::display_cursor_state_claimed;
        self.job_data = a::GfxDriverCursorJob {
            backend: self.info.backend,
            sequence: self.status.sequence,
            deadline_ns: self.status.deadline_ns,
            request,
            ..a::GfxDriverCursorJob::default()
        };
        self.has_job = true;
        self.job_data.request.size = size_of::<a::DisplayCursorRequest>() as u32;
        self.taken = false;
        self.has_reply = false;
    }
    pub fn take(
        &mut self,
        driver: Owner,
        backend: a::GfxBackendBinding,
        now: u64,
    ) -> Result<Option<a::GfxDriverCursorJob>, Error> {
        if self.driver != driver || !same(&self.info.backend, &backend) {
            return Err(Error::Stale);
        }
        if !self.available() {
            return Ok(None);
        }
        if !self.has_job && (self.closing || (self.suspended && self.visible())) {
            if self.status.sequence == u64::MAX || now == 0 || now >= u64::MAX - OPERATION_NS {
                return Err(Error::Exhausted);
            }
            self.arm(
                a::DisplayCursorRequest {
                    display_generation: self.info.display_generation,
                    head_id: self.info.head_id,
                    operation: if self.closing {
                        a::display_cursor_operation_release
                    } else {
                        a::display_cursor_operation_hide
                    },
                    ..a::DisplayCursorRequest::default()
                },
                now,
            )
        }
        if !self.has_job || self.taken {
            return Ok(None);
        }
        let job = self.job_data;
        if now >= job.deadline_ns {
            return Err(Error::Busy);
        }
        self.taken = true;
        self.status.phase = a::display_cursor_phase_active;
        Ok(Some(job))
    }
    pub fn validate_reply(
        &self,
        driver: Owner,
        value: a::GfxDriverCursorCompletion,
    ) -> Result<bool, Error> {
        if self.driver != driver || value.display_generation != self.info.display_generation {
            return Err(Error::Stale);
        }
        if value.version != 1
            || (value.size as usize) < size_of::<a::GfxDriverCursorCompletion>()
            || value.reserved0 != 0
            || value.sequence == 0
            || value.visibility > a::display_cursor_visibility_unknown
        {
            return Err(Error::Invalid);
        }
        if self.has_reply {
            let mut normalized = value;
            normalized.size = size_of::<a::GfxDriverCursorCompletion>() as u32;
            if same(&self.reply_data, &normalized) {
                return Ok(false);
            }
        }
        if !self.has_job {
            return Err(Error::Stale);
        }
        let job = self.job_data;
        if !self.taken || value.sequence != job.sequence {
            return Err(Error::Stale);
        }
        if value.outcome == a::gfx_output_outcome_lost {
            if value.error_code >= 0 || value.visibility != a::display_cursor_visibility_unknown {
                return Err(Error::Invalid);
            }
            return Ok(true);
        }
        if value.outcome == a::gfx_output_outcome_old_preserved {
            if value.error_code >= 0 || value.visibility != u32::from(self.visible()) {
                return Err(Error::Invalid);
            }
        } else if value.outcome == a::gfx_output_outcome_applied {
            let shown = job.request.operation == a::display_cursor_operation_show
                || job.request.operation == a::display_cursor_operation_move;
            if value.error_code != 0 || value.visibility != u32::from(shown) {
                return Err(Error::Invalid);
            }
        } else {
            return Err(Error::Invalid);
        }
        Ok(true)
    }
    pub fn finish(&mut self, value: a::GfxDriverCursorCompletion) {
        self.reply_data = value;
        self.has_reply = true;
        self.reply_data.size = size_of::<a::GfxDriverCursorCompletion>() as u32;
        if value.outcome == a::gfx_output_outcome_lost {
            self.mark_lost(value.error_code);
            return;
        }
        assert!(self.has_job);
        let request = self.job_data.request;
        let applied = value.outcome == a::gfx_output_outcome_applied;
        self.status.flags &= !a::display_cursor_state_unknown;
        if applied {
            match request.operation {
                a::display_cursor_operation_prepare => {
                    self.status.image_sequence = self.job_data.sequence;
                    self.status.flags |= a::display_cursor_state_image_ready
                }
                a::display_cursor_operation_show | a::display_cursor_operation_move => {
                    self.status.x = request.x;
                    self.status.y = request.y;
                    self.status.flags |= a::display_cursor_state_visible
                }
                a::display_cursor_operation_hide | a::display_cursor_operation_release => {
                    self.status.flags &= !a::display_cursor_state_visible;
                    if request.operation == a::display_cursor_operation_release {
                        self.has_actor = false;
                        self.status.image_sequence = 0;
                        self.status.flags = if self.suspended {
                            a::display_cursor_state_suspended
                        } else {
                            0
                        };
                        self.closing = false
                    }
                }
                _ => panic!("cursor operation"),
            }
        }
        self.status.completed = value.sequence;
        self.status.error_code = value.error_code;
        self.status.phase = if applied {
            a::display_cursor_phase_complete
        } else {
            a::display_cursor_phase_failed
        };
        self.status.deadline_ns = 0;
        self.has_job = false;
        self.taken = false;
    }
    pub fn reject_untaken(&mut self, code: i32) {
        assert!(self.has_job && !self.taken);
        self.status.completed = self.job_data.sequence;
        self.status.phase = a::display_cursor_phase_failed;
        self.status.error_code = code;
        self.status.deadline_ns = 0;
        self.has_job = false;
    }
    pub fn close(&mut self, caller: Owner) -> bool {
        if !self.has_actor || self.actor_data != caller {
            return false;
        }
        self.closing = true;
        true
    }
    pub fn mark_lost(&mut self, code: i32) {
        self.status.phase = a::display_cursor_phase_lost;
        self.status.error_code = code;
        self.status.flags |= a::display_cursor_state_unknown;
        self.status.deadline_ns = 0;
    }
    pub fn retire_after_reset(
        &mut self,
        driver: Owner,
        backend: a::GfxBackendBinding,
    ) -> Result<(), Error> {
        if self.info.display_generation == 0 {
            return Ok(());
        }
        if self.driver != driver || !same(&self.info.backend, &backend) {
            return Err(Error::Stale);
        }
        self.mark_lost(a::gfx_output_error_unavailable);
        self.status.flags = 0;
        self.info = a::DisplayCursorInfo::default();
        self.has_job = false;
        self.has_reply = false;
        self.has_actor = false;
        self.taken = false;
        self.closing = false;
        self.suspended = false;
        Ok(())
    }
}
const _: () = {
    use core::mem::{align_of, offset_of};
    assert!(
        size_of::<State>() == 432
            && align_of::<State>() == 8
            && offset_of!(State, actor_data) == 112
            && offset_of!(State, status) == 136
            && offset_of!(State, job_data) == 216
            && offset_of!(State, reply_data) == 384
            && offset_of!(State, taken) == 424
    );
};

/// # Safety
/// Exact initialized C cursor metadata under the display owner; outputs exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_cursor_configure(
    state: *mut State,
    driver: *const Owner,
    input: *const a::DisplayCursorInfo,
) -> i32 {
    match unsafe { (&mut *state).configure(driver.read(), input.read()) } {
        Ok(()) => 0,
        Err(error) => error as i32,
    }
}

/// # Safety
/// Exact initialized C cursor metadata under the display owner; outputs exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_cursor_visible(state: *const State) -> bool {
    unsafe { (&*state).visible() }
}

/// # Safety
/// Exact initialized C cursor metadata under the display owner; outputs exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_cursor_lost(state: *const State) -> bool {
    unsafe { (&*state).lost() }
}

/// # Safety
/// Exact initialized C cursor metadata under the display owner; outputs exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_cursor_available(state: *const State) -> bool {
    unsafe { (&*state).available() }
}

/// # Safety
/// Exact initialized C cursor metadata under the display owner; outputs exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_cursor_validate(
    state: *const State,
    caller: *const Owner,
    input: *const a::DisplayCursorRequest,
) -> i32 {
    match unsafe { (&*state).validate(caller.read(), input.read()) } {
        Ok(()) => 0,
        Err(error) => error as i32,
    }
}

/// # Safety
/// Exact initialized C cursor metadata under the display owner; outputs exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_cursor_begin(
    state: *mut State,
    caller: *const Owner,
    input: *const a::DisplayCursorRequest,
    reference: *const a::GfxBufferHandle,
    timeline: u64,
    point: u64,
    now: u64,
) -> i32 {
    match unsafe {
        (&mut *state).begin(
            caller.read(),
            input.read(),
            reference.read(),
            timeline,
            point,
            now,
        )
    } {
        Ok(()) => 0,
        Err(error) => error as i32,
    }
}

/// # Safety
/// Exact initialized C cursor metadata under the display owner; outputs exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_cursor_take(
    state: *mut State,
    driver: *const Owner,
    backend: *const a::GfxBackendBinding,
    now: u64,
    out: *mut a::GfxDriverCursorJob,
    has: *mut bool,
) -> i32 {
    match unsafe { (&mut *state).take(driver.read(), backend.read(), now) } {
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
/// Exact initialized C cursor metadata under the display owner; outputs exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_cursor_validate_reply(
    state: *const State,
    driver: *const Owner,
    input: *const a::GfxDriverCursorCompletion,
    out: *mut bool,
) -> i32 {
    match unsafe { (&*state).validate_reply(driver.read(), input.read()) } {
        Ok(value) => {
            unsafe { out.write(value) };
            0
        }
        Err(error) => error as i32,
    }
}

/// # Safety
/// Exact initialized C cursor metadata under the display owner; outputs exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_cursor_finish(
    state: *mut State,
    input: *const a::GfxDriverCursorCompletion,
) {
    unsafe { (&mut *state).finish(input.read()) }
}

/// # Safety
/// Exact initialized C cursor metadata under the display owner; outputs exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_cursor_reject(state: *mut State, code: i32) {
    unsafe { (&mut *state).reject_untaken(code) }
}

/// # Safety
/// Exact initialized C cursor metadata under the display owner; outputs exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_cursor_close(
    state: *mut State,
    caller: *const Owner,
) -> bool {
    unsafe { (&mut *state).close(caller.read()) }
}

/// # Safety
/// Exact initialized C cursor metadata under the display owner; outputs exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_cursor_mark_lost(state: *mut State, code: i32) {
    unsafe { (&mut *state).mark_lost(code) }
}

/// # Safety
/// Exact initialized C cursor metadata under the display owner; outputs exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_cursor_retire(
    state: *mut State,
    driver: *const Owner,
    backend: *const a::GfxBackendBinding,
) -> i32 {
    match unsafe { (&mut *state).retire_after_reset(driver.read(), backend.read()) } {
        Ok(()) => 0,
        Err(error) => error as i32,
    }
}
