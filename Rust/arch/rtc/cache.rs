//! One coherent calendar generation. The RTC owner encloses every access;
//! there is no asynchronous refresh that can republish an older setter epoch.
use super::calendar::{self, DateTime};
const SECOND: u64 = 1_000_000_000;
const SYNC: u64 = 60 * SECOND;
const RETRY: u64 = 10_000_000;
const MAX_AGE: u64 = 300 * SECOND;
pub(super) struct Cache {
    value: DateTime,
    anchor: Option<u64>,
    attempted: Option<u64>,
    interval: u64,
    century: u8,
}
impl Cache {
    pub const EMPTY: Self = Self {
        value: DateTime::EMPTY,
        anchor: None,
        attempted: None,
        interval: RETRY,
        century: 0,
    };
    pub fn invalidate(&mut self) {
        *self = Self::EMPTY;
    }
    /// Early FADT publication, absent clocks and clock regressions require
    /// fresh hardware. Never combine an old wall value with a new anchor.
    pub fn due(&mut self, now: Option<u64>, century: u8) -> bool {
        if self.century != century
            || now.is_none()
            || matches!((now, self.attempted), (Some(n), Some(a)) if n < a)
        {
            self.invalidate();
            self.century = century;
            return true;
        }
        match (now, self.attempted) {
            (Some(n), Some(a)) => n - a >= self.interval,
            _ => true,
        }
    }
    pub fn publish(&mut self, now: Option<u64>, century: u8, value: Option<DateTime>) {
        self.century = century;
        self.attempted = now;
        self.interval = if value.is_some() { SYNC } else { RETRY };
        if let Some(value) = value {
            self.value = value;
            self.anchor = now;
        }
    }
    pub fn current(&self, now: Option<u64>) -> DateTime {
        let value = match (now, self.anchor) {
            (Some(n), Some(a)) if n >= a && n - a <= MAX_AGE => {
                calendar::advance(self.value, (n - a) / SECOND)
            }
            (None, None) if self.value.valid => Some(self.value),
            _ => None,
        };
        value.unwrap_or(DateTime::EMPTY)
    }
}
