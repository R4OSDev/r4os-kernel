//! Copied power receipts and one finite program intent. No hardware or I/O.
use super::{equal::same, error::Error};
use crate::{contract as a, gfx::types::Owner};
use core::mem::size_of;
pub const LEASE_NS: u64 = 30_000_000_000;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct State {
    pub has_value: bool,
    pub value: a::GfxOutputPower,
    pub has_actor: bool,
    pub actor: Owner,
    pub intent: a::GfxPowerRequest,
    pub serial: u64,
}
pub fn sleeping_phase(phase: u32) -> bool {
    (a::gfx_power_phase_stopping..=a::gfx_power_phase_waking).contains(&phase)
}
pub fn valid_id(id: a::GfxOutputId) -> bool {
    id.adapter_id != 0
        && id.connector_id != 0
        && id.device_generation != 0
        && id.connection_generation != 0
}
impl State {
    pub fn sleeping(&self) -> bool {
        self.has_value && sleeping_phase(self.value.phase)
    }
    pub fn publish(&mut self, input: a::GfxOutputPower) -> Result<bool, Error> {
        if input.version != 1
            || input.size as usize != size_of::<a::GfxOutputPower>()
            || !valid_id(input.identity)
            || input.capabilities & !3 != 0
            || input.phase > a::gfx_power_phase_unavailable
            || input.sequence == 0
            || input.since_ns == 0
            || input.reason > a::gfx_power_reason_unsupported
            || input.reserved0 != 0
        {
            return Err(Error::Invalid);
        }
        if input.request_sequence > self.serial {
            return Err(Error::Stale);
        }
        if input.capabilities & a::gfx_power_cap_signal == 0
            && (input.capabilities != 0
                || input.phase != a::gfx_power_phase_unavailable
                || input.reason != a::gfx_power_reason_unsupported)
        {
            return Err(Error::Invalid);
        }
        if (input.phase == a::gfx_power_phase_on || input.phase == a::gfx_power_phase_off)
            && (input.core_point == 0 || input.window_point == 0 || input.reason != 0)
        {
            return Err(Error::Invalid);
        }
        if input.phase == a::gfx_power_phase_off
            && input.capabilities & a::gfx_power_cap_sink != 0
            && input.control_receipt == 0
        {
            return Err(Error::Invalid);
        }
        if self.has_value {
            let old = self.value;
            if !same(&old.identity, &input.identity)
                || input.sequence < old.sequence
                || input.since_ns < old.since_ns
                || (input.sequence == old.sequence && !same(&old, &input))
            {
                return Err(Error::Stale);
            }
            if same(&old, &input) {
                return Ok(false);
            }
        }
        self.value = input;
        self.has_value = true;
        Ok(true)
    }
    pub fn get(&self, identity: a::GfxOutputId) -> Result<a::GfxOutputPower, Error> {
        if !self.has_value {
            return Err(Error::Unsupported);
        }
        if !same(&self.value.identity, &identity) {
            return Err(Error::Stale);
        }
        Ok(self.value)
    }
    fn wake(&mut self) {
        if self.intent.off != 0 {
            self.serial = self.serial.saturating_add(1);
            self.intent = a::GfxPowerRequest {
                identity: self.intent.identity,
                sequence: self.serial,
                ..a::GfxPowerRequest::default()
            };
        }
        self.has_actor = false;
    }
    pub fn stopped(&mut self, actor: Owner) -> bool {
        if !self.has_actor || self.actor != actor {
            return false;
        }
        self.wake();
        true
    }
    pub fn read(
        &mut self,
        identity: a::GfxOutputId,
        now: u64,
    ) -> Result<a::GfxPowerRequest, Error> {
        let _ = self.get(identity)?;
        if now == 0 || now == u64::MAX {
            return Err(Error::Invalid);
        }
        if self.has_actor && now >= self.intent.deadline_ns {
            self.wake()
        }
        let mut value = self.intent;
        value.identity = identity;
        Ok(value)
    }
    pub fn request(
        &mut self,
        caller: Owner,
        input: a::GfxPowerRequest,
        now: u64,
    ) -> Result<a::GfxPowerRequest, Error> {
        if input.version != 1
            || input.size as usize != size_of::<a::GfxPowerRequest>()
            || !caller.valid()
            || caller.kind != 1
            || !valid_id(input.identity)
            || input.off > 1
            || input.reserved0 != 0
            || input.reserved1 != 0
            || input.sequence != 0
            || input.deadline_ns != 0
            || now == 0
            || now >= u64::MAX - LEASE_NS
        {
            return Err(Error::Invalid);
        }
        let value = self.get(input.identity)?;
        if value.capabilities & a::gfx_power_cap_signal == 0 {
            return Err(Error::Unsupported);
        }
        if input.off != 0 && self.has_actor && now < self.intent.deadline_ns && self.actor != caller
        {
            return Err(Error::Busy);
        }
        let renew = input.off != 0
            && self.has_actor
            && self.actor == caller
            && now < self.intent.deadline_ns
            && self.intent.off == input.off;
        if !renew && self.serial == u64::MAX {
            return Err(Error::Exhausted);
        }
        if !renew {
            self.serial += 1
        }
        self.intent = input;
        self.intent.size = size_of::<a::GfxPowerRequest>() as u32;
        self.intent.sequence = self.serial;
        self.intent.deadline_ns = if input.off != 0 { now + LEASE_NS } else { 0 };
        self.has_actor = input.off != 0;
        if self.has_actor {
            self.actor = caller
        }
        Ok(self.intent)
    }
}
const _: () = {
    assert!(
        size_of::<State>() == 208
            && core::mem::offset_of!(State, intent) == 136
            && core::mem::offset_of!(State, serial) == 200
    );
};

/// # Safety
/// Exact initialized C metadata under the existing display owner; output is exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_power_publish(
    state: *mut State,
    input: *const a::GfxOutputPower,
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
pub unsafe extern "C" fn r4os_rust_display_power_get(
    state: *const State,
    identity: *const a::GfxOutputId,
    out: *mut a::GfxOutputPower,
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
pub unsafe extern "C" fn r4os_rust_display_power_read(
    state: *mut State,
    identity: *const a::GfxOutputId,
    now: u64,
    out: *mut a::GfxPowerRequest,
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
pub unsafe extern "C" fn r4os_rust_display_power_request(
    state: *mut State,
    caller: *const Owner,
    input: *const a::GfxPowerRequest,
    now: u64,
    out: *mut a::GfxPowerRequest,
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
pub unsafe extern "C" fn r4os_rust_display_power_stopped(
    state: *mut State,
    actor: *const Owner,
) -> bool {
    unsafe { (&mut *state).stopped(actor.read()) }
}

/// # Safety
/// Exact initialized C metadata under the existing display owner; output is exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_power_sleeping(state: *const State) -> bool {
    unsafe { (&*state).sleeping() }
}

/// # Safety
/// Exact initialized C metadata under the existing display owner; output is exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_power_phase(phase: u32) -> bool {
    sleeping_phase(phase)
}

/// # Safety
/// Exact initialized C metadata under the existing display owner; output is exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_power_id_valid(identity: *const a::GfxOutputId) -> bool {
    valid_id(unsafe { identity.read() })
}
