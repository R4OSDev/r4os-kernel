//! Copied brightness intent and exact driver receipt. No policy or I/O.
use super::{equal::same, error::Error, power_state::valid_id};
use crate::{contract as a, gfx::types::Owner};
use core::mem::size_of;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct State {
    pub has_value: bool,
    pub value: a::GfxOutputBrightness,
    pub intent: a::GfxBrightnessRequest,
    pub serial: u64,
}
impl State {
    pub fn publish(&mut self, input: a::GfxOutputBrightness) -> Result<bool, Error> {
        if input.version != 1
            || input.size as usize != size_of::<a::GfxOutputBrightness>()
            || !valid_id(input.identity)
            || input.path > a::gfx_brightness_path_aux16
            || input.phase > a::gfx_brightness_phase_failed
            || input.reason > a::gfx_brightness_reason_inactive
            || input.flags & !a::gfx_brightness_flag_current_known != 0
            || input.reserved0 != 0
            || input.sequence == 0
            || input.since_ns == 0
            || input.minimum > input.maximum
            || input.maximum > 65535
            || input.current > 65535
        {
            return Err(Error::Invalid);
        }
        if input.request_sequence > self.serial {
            return Err(Error::Stale);
        }
        if input.phase == a::gfx_brightness_phase_unavailable {
            if input.path != 0 || input.reason == 0 || input.flags != 0 {
                return Err(Error::Invalid);
            }
        } else {
            if input.path == 0 || input.minimum == input.maximum {
                return Err(Error::Invalid);
            }
            if input.phase == a::gfx_brightness_phase_ready
                && (input.reason != 0 || input.flags == 0)
            {
                return Err(Error::Invalid);
            }
            if input.phase == a::gfx_brightness_phase_failed && input.reason == 0 {
                return Err(Error::Invalid);
            }
        }
        if self.has_value {
            let old = self.value;
            if !same(&old.identity, &input.identity)
                || input.sequence < old.sequence
                || input.request_sequence < old.request_sequence
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
    pub fn get(&self, identity: a::GfxOutputId) -> Result<a::GfxOutputBrightness, Error> {
        if !self.has_value {
            return Err(Error::Unsupported);
        }
        if !same(&self.value.identity, &identity) {
            return Err(Error::Stale);
        }
        Ok(self.value)
    }
    pub fn read(&self, identity: a::GfxOutputId) -> Result<a::GfxBrightnessRequest, Error> {
        let _ = self.get(identity)?;
        let mut value = self.intent;
        value.identity = identity;
        Ok(value)
    }
    pub fn request(
        &mut self,
        caller: Owner,
        input: a::GfxBrightnessRequest,
    ) -> Result<a::GfxBrightnessRequest, Error> {
        if input.version != 1
            || input.size as usize != size_of::<a::GfxBrightnessRequest>()
            || !caller.valid()
            || caller.kind != 1
            || input.level > 65535
            || input.reserved0 != 0
            || input.sequence != 0
        {
            return Err(Error::Invalid);
        }
        let value = self.get(input.identity)?;
        if value.path == 0 || value.phase == a::gfx_brightness_phase_unavailable {
            return Err(Error::Unsupported);
        }
        if input.level < value.minimum || input.level > value.maximum {
            return Err(Error::Invalid);
        }
        if self.serial == u64::MAX {
            return Err(Error::Exhausted);
        }
        self.serial += 1;
        self.intent = input;
        self.intent.sequence = self.serial;
        Ok(self.intent)
    }
}
const _: () = {
    assert!(
        size_of::<State>() == 152
            && core::mem::offset_of!(State, intent) == 96
            && core::mem::offset_of!(State, serial) == 144
    );
};

/// # Safety
/// Exact initialized C metadata under the existing display owner; output is exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_brightness_publish(
    state: *mut State,
    input: *const a::GfxOutputBrightness,
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
pub unsafe extern "C" fn r4os_rust_display_brightness_get(
    state: *const State,
    identity: *const a::GfxOutputId,
    out: *mut a::GfxOutputBrightness,
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
pub unsafe extern "C" fn r4os_rust_display_brightness_read(
    state: *const State,
    identity: *const a::GfxOutputId,
    out: *mut a::GfxBrightnessRequest,
) -> i32 {
    match unsafe { (&*state).read(identity.read()) } {
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
pub unsafe extern "C" fn r4os_rust_display_brightness_request(
    state: *mut State,
    caller: *const Owner,
    input: *const a::GfxBrightnessRequest,
    out: *mut a::GfxBrightnessRequest,
) -> i32 {
    match unsafe { (&mut *state).request(caller.read(), input.read()) } {
        Ok(value) => {
            unsafe { out.write(value) };
            0
        }
        Err(error) => error as i32,
    }
}
