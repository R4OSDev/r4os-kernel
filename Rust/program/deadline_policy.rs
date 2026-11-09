//! Shared original clock-bound absolute wait budget. No scheduler,
//! allocation, source lookup or resource completion lives in this value policy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ClockKey {
    pub source: u32,
    pub generation: u32,
    pub event_backend: u32,
    pub tick_hz: u32,
}
impl ClockKey {
    pub fn valid(self) -> bool {
        self.source != 0 && self.generation != 0 && self.tick_hz != 0
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Error {
    Invalid,
    ClockUnavailable,
    ClockChanged,
    ClockRegressed,
    Overflow,
}
#[derive(Clone, Copy)]
pub(super) struct Sample {
    pub key: ClockKey,
    pub instant_ns: u64,
}
#[derive(Clone, Copy)]
pub(super) struct Budget {
    pub kind: u32,
    pub key: ClockKey,
    pub absolute_ns: u64,
    pub last_ns: u64,
}
impl Budget {
    pub fn validate(self) -> Result<(), Error> {
        match self.kind {
            0 | 2
                if self.key == ClockKey::default()
                    && self.absolute_ns == 0
                    && self.last_ns == 0 =>
            {
                Ok(())
            }
            // An expired deadline remains valid after observing a later time.
            // Rechecking it must still report zero or the actual clock error.
            1 if self.key.valid() => Ok(()),
            1 if !self.key.valid() => Err(Error::ClockUnavailable),
            _ => Err(Error::Invalid),
        }
    }
    /// None is logical Forever. Finite max(u64) stays finite; errors preserve
    /// the original deadline and last accepted sample.
    pub fn remaining(&mut self, sample: Option<Sample>) -> Result<Option<u64>, Error> {
        self.validate()?;
        if self.kind == 0 {
            return Ok(Some(0));
        }
        if self.kind == 2 {
            return Ok(None);
        }
        let sample = sample.ok_or(Error::ClockUnavailable)?;
        if !sample.key.valid() {
            return Err(Error::ClockUnavailable);
        }
        if self.key != sample.key {
            return Err(Error::ClockChanged);
        }
        if sample.instant_ns < self.last_ns {
            return Err(Error::ClockRegressed);
        }
        self.last_ns = sample.instant_ns;
        Ok(Some(self.absolute_ns.saturating_sub(sample.instant_ns)))
    }
}
pub(super) fn ticks(nanoseconds: u64, hz: u32) -> Result<u64, Error> {
    if hz == 0 {
        return Err(Error::ClockUnavailable);
    }
    let product = u128::from(nanoseconds) * u128::from(hz);
    let result = product.div_ceil(1_000_000_000);
    if result >= u128::from(u64::MAX) {
        return Err(Error::Overflow);
    }
    Ok(result as u64)
}
