//! Copied output refresh facts and finite presenter intent. No hardware or I/O.
use super::{equal::same, error::Error, output_target};
use crate::{contract as a, gfx::types::Owner};
use core::mem::size_of;
pub const LEASE_NS: u64 = 1_500_000_000;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct State {
    pub has_value: bool,
    pub value: a::GfxOutputRefresh,
    pub has_actor: bool,
    pub actor: Owner,
    pub intent: a::GfxRefreshRequest,
    pub serial: u64,
}
impl State {
    pub fn publish(&mut self, input: a::GfxOutputRefresh) -> Result<bool, Error> {
        if !valid(&input) {
            return Err(Error::Invalid);
        }
        if input.status.request_sequence > self.serial {
            return Err(Error::Stale);
        }
        if self.has_value {
            let old = self.value;
            if !same(&old.target, &input.target) {
                self.off()
            } else {
                if input.status.sequence < old.status.sequence
                    || (input.status.sequence == old.status.sequence
                        && !same(&input.status, &old.status))
                    || input.measured.sequence < old.measured.sequence
                {
                    return Err(Error::Stale);
                }
                if input.measured.sequence == old.measured.sequence
                    && !same(&input.measured, &old.measured)
                {
                    return Err(Error::Stale);
                }
            }
        }
        let mut value = input;
        value.size = size_of::<a::GfxOutputRefresh>() as u32;
        value.capabilities.size = size_of::<a::GfxRefreshCapabilities>() as u32;
        value.status.size = size_of::<a::GfxRefreshStatus>() as u32;
        value.measured.size = size_of::<a::GfxRefreshMeasure>() as u32;
        let changed = !self.has_value
            || !same(&self.value.target, &value.target)
            || !same(&self.value.capabilities, &value.capabilities)
            || self.value.status.phase != value.status.phase
            || self.value.status.reason != value.status.reason
            || self.value.status.policy != value.status.policy
            || self.value.status.scene != value.status.scene
            || self.value.status.request_sequence != value.status.request_sequence;
        self.value = value;
        self.has_value = true;
        Ok(changed)
    }
    pub fn get(&self, target: a::GfxOutputTarget) -> Result<a::GfxOutputRefresh, Error> {
        if !self.has_value {
            return Err(Error::Unsupported);
        }
        if !same(&self.value.target, &target) {
            return Err(Error::Stale);
        }
        Ok(self.value)
    }
    fn off(&mut self) {
        if self.has_actor
            || self.intent.policy != 0
            || self.intent.scene != 0
            || self.intent.operation != 0
        {
            self.serial = self.serial.saturating_add(1);
            self.intent = a::GfxRefreshRequest {
                target: self.intent.target,
                sequence: self.serial,
                ..a::GfxRefreshRequest::default()
            };
        }
        self.has_actor = false;
    }
    pub fn stopped(&mut self, actor: Owner) -> bool {
        if !self.has_actor || self.actor != actor {
            return false;
        }
        self.off();
        true
    }
    pub fn pause(&mut self) {
        self.off()
    }
    pub fn read(
        &mut self,
        target: a::GfxOutputTarget,
        now: u64,
    ) -> Result<a::GfxRefreshRequest, Error> {
        let _ = self.get(target)?;
        if now == 0 || now == u64::MAX {
            return Err(Error::Invalid);
        }
        if self.has_actor && now >= self.intent.deadline_ns {
            self.off()
        }
        let mut value = self.intent;
        value.target = target;
        Ok(value)
    }
    pub fn request(
        &mut self,
        caller: Owner,
        input: a::GfxRefreshRequest,
        now: u64,
    ) -> Result<a::GfxRefreshRequest, Error> {
        if input.version != 1
            || (input.size as usize) < size_of::<a::GfxRefreshRequest>()
            || !caller.valid()
            || caller.kind != 1
            || !output_target::valid(&input.target)
            || input.operation > a::gfx_refresh_operation_clear_fault
            || input.policy > a::gfx_refresh_policy_windows
            || input.scene & !7 != 0
            || input.reserved0 != 0
            || input.sequence != 0
            || input.deadline_ns != 0
            || now == 0
            || now >= u64::MAX - LEASE_NS
        {
            return Err(Error::Invalid);
        }
        let current = self.get(input.target)?;
        if input.operation != a::gfx_refresh_operation_configure
            && (input.policy != 0 || input.scene != 0)
        {
            return Err(Error::Invalid);
        }
        if input.policy != 0 && current.capabilities.flags & a::gfx_refresh_cap_capable == 0 {
            return Err(Error::Unsupported);
        }
        if self.has_actor && now < self.intent.deadline_ns && self.actor != caller {
            return Err(Error::Busy);
        }
        let renewed = self.has_actor
            && self.actor == caller
            && now < self.intent.deadline_ns
            && input.operation == a::gfx_refresh_operation_configure
            && self.intent.operation == input.operation
            && self.intent.policy == input.policy
            && self.intent.scene == input.scene
            && same(&self.intent.target, &input.target);
        if !renewed && self.serial == u64::MAX {
            return Err(Error::Exhausted);
        }
        if !renewed {
            self.serial += 1
        }
        self.intent = input;
        self.intent.size = size_of::<a::GfxRefreshRequest>() as u32;
        self.intent.sequence = self.serial;
        self.intent.deadline_ns = now + LEASE_NS;
        self.has_actor = true;
        self.actor = caller;
        if input.operation == a::gfx_refresh_operation_release {
            self.has_actor = false;
            self.intent.operation = 0;
            self.intent.deadline_ns = 0
        }
        Ok(self.intent)
    }
}
pub fn valid(value: &a::GfxOutputRefresh) -> bool {
    let cap = value.capabilities;
    let status = value.status;
    let measured = value.measured;
    if value.version != 1
        || (value.size as usize) < size_of::<a::GfxOutputRefresh>()
        || cap.version != 1
        || (cap.size as usize) < size_of::<a::GfxRefreshCapabilities>()
        || status.version != 1
        || (status.size as usize) < size_of::<a::GfxRefreshStatus>()
        || measured.version != 1
        || (measured.size as usize) < size_of::<a::GfxRefreshMeasure>()
        || !output_target::valid(&value.target)
        || value.target.adapter_id == 0
        || cap.flags & !31 != 0
        || cap.flags & a::gfx_refresh_cap_known == 0
        || cap.origin > 4
        || cap.reserved0 != 0
        || cap.reserved1 != 0
        || status.sequence == 0
        || status.phase > 5
        || status.reason > a::gfx_refresh_reason_stale_clock
        || status.policy > 2
        || status.scene & !7 != 0
        || measured.reserved0 != 0
        || measured.reserved1 != 0
        || measured.samples > 32
    {
        return false;
    }
    if cap.flags & a::gfx_refresh_cap_capable != 0 {
        if cap.origin == 0
            || cap.min_millihz == 0
            || cap.max_millihz < cap.min_millihz
            || cap.nominal_millihz < cap.max_millihz
            || cap.min_period_ns == 0
            || cap.max_period_ns <= cap.min_period_ns
            || cap.max_vtotal == 0
            || cap.max_vtotal > 131072
        {
            return false;
        }
    } else if cap.origin != 0
        || cap.min_millihz != 0
        || cap.max_millihz != 0
        || cap.min_period_ns != 0
        || cap.max_period_ns != 0
        || cap.max_increase_ns != 0
        || cap.max_decrease_ns != 0
        || cap.max_vtotal != 0
        || cap.flags & a::gfx_refresh_cap_lfc != 0
        || status.phase == a::gfx_refresh_phase_active
    {
        return false;
    }
    if status.phase == a::gfx_refresh_phase_active
        && (status.core_point == 0 || status.receipt == 0 || status.since_ns == 0)
    {
        return false;
    }
    if measured.samples == 0 {
        return measured.last_period_ns == 0
            && measured.min_period_ns == 0
            && measured.max_period_ns == 0
            && measured.mean_period_ns == 0
            && measured.millihz == 0;
    }
    measured.sequence != 0
        && measured.observed_ns != 0
        && measured.min_period_ns != 0
        && measured.last_period_ns >= measured.min_period_ns
        && measured.last_period_ns <= measured.max_period_ns
        && measured.mean_period_ns >= measured.min_period_ns
        && measured.mean_period_ns <= measured.max_period_ns
        && 1_000_000_000_000 / measured.mean_period_ns == u64::from(measured.millihz)
}
const _: () = {
    assert!(
        size_of::<State>() == 408
            && core::mem::offset_of!(State, intent) == 312
            && core::mem::offset_of!(State, serial) == 400
    );
};

/// # Safety
/// Exact initialized C metadata under the existing display owner; output is exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_refresh_publish(
    state: *mut State,
    input: *const a::GfxOutputRefresh,
    out: *mut bool,
) -> i32 {
    match unsafe { (&mut *state).publish(input.read()) } {
        Ok(value) => {
            unsafe { out.write(value) };
            0
        }
        Err(error) => error as i32,
    }
}

/// # Safety
/// Exact initialized C metadata under the existing display owner; output is exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_refresh_get(
    state: *const State,
    identity: *const a::GfxOutputTarget,
    out: *mut a::GfxOutputRefresh,
) -> i32 {
    match unsafe { (&*state).get(identity.read()) } {
        Ok(value) => {
            unsafe { out.write(value) };
            0
        }
        Err(error) => error as i32,
    }
}

/// # Safety
/// Exact initialized C metadata under the existing display owner; output is exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_refresh_read(
    state: *mut State,
    identity: *const a::GfxOutputTarget,
    now: u64,
    out: *mut a::GfxRefreshRequest,
) -> i32 {
    match unsafe { (&mut *state).read(identity.read(), now) } {
        Ok(value) => {
            unsafe { out.write(value) };
            0
        }
        Err(error) => error as i32,
    }
}

/// # Safety
/// Exact initialized C metadata under the existing display owner; output is exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_refresh_request(
    state: *mut State,
    caller: *const Owner,
    input: *const a::GfxRefreshRequest,
    now: u64,
    out: *mut a::GfxRefreshRequest,
) -> i32 {
    match unsafe { (&mut *state).request(caller.read(), input.read(), now) } {
        Ok(value) => {
            unsafe { out.write(value) };
            0
        }
        Err(error) => error as i32,
    }
}

/// # Safety
/// Exact initialized C metadata under the existing display owner; output is exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_refresh_stopped(
    state: *mut State,
    actor: *const Owner,
) -> bool {
    unsafe { (&mut *state).stopped(actor.read()) }
}

/// # Safety
/// Exact initialized C metadata under the existing display owner; output is exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_refresh_pause(state: *mut State) {
    unsafe { (&mut *state).pause() }
}
