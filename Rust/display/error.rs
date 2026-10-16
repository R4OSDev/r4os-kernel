//! Private migration status codes; public API errors retain their own mapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum Error {
    Invalid = 1,
    Overflow = 2,
    Unsupported = 3,
    Exhausted = 4,
    Budget = 5,
    Capacity = 6,
    Stale = 7,
    Busy = 8,
    Closed = 9,
    WrongOwner = 10,
    OutOfMemory = 11,
    Unavailable = 12,
    WaitTimeout = 13,
    WaitCancelled = 14,
    Disabled = 15,
    AlreadyCompleted = 16,
    Bandwidth = 17,
    Routing = 18,
    Dependency = 19,
    DeviceLost = 20,
    RestoreFailed = 21,
}
pub fn status(code: i32) -> Result<(), Error> {
    if code == 0 {
        return Ok(());
    }
    Err(match code {
        1 => Error::Invalid,
        2 => Error::Overflow,
        3 => Error::Unsupported,
        4 => Error::Exhausted,
        5 => Error::Budget,
        6 => Error::Capacity,
        7 => Error::Stale,
        8 => Error::Busy,
        9 => Error::Closed,
        10 => Error::WrongOwner,
        11 => Error::OutOfMemory,
        12 => Error::Unavailable,
        13 => Error::WaitTimeout,
        14 => Error::WaitCancelled,
        15 => Error::Disabled,
        16 => Error::AlreadyCompleted,
        17 => Error::Bandwidth,
        18 => Error::Routing,
        19 => Error::Dependency,
        20 => Error::DeviceLost,
        21 => Error::RestoreFailed,
        _ => panic!("display private status code"),
    })
}
impl From<crate::gfx::types::Error> for Error {
    fn from(value: crate::gfx::types::Error) -> Self {
        use crate::gfx::types::Error as G;
        match value {
            G::Invalid => Self::Invalid,
            G::Overflow => Self::Overflow,
            G::Unsupported => Self::Unsupported,
            G::Exhausted => Self::Exhausted,
            G::Budget => Self::Budget,
            G::Capacity => Self::Capacity,
            G::Stale => Self::Stale,
            G::Busy => Self::Busy,
            G::Closed => Self::Closed,
            G::WrongOwner => Self::WrongOwner,
            G::OutOfMemory => Self::OutOfMemory,
            G::Unavailable => Self::Unavailable,
            G::WaitTimeout => Self::WaitTimeout,
            G::WaitCancelled => Self::WaitCancelled,
        }
    }
}
