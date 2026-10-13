//! Boot callers use the same Rust owner implementation and exact named lock
//! as memory/legacy callers. There is no separate boot lock or rank state.
pub use crate::memory::owner::Token;
pub struct Guard {
    _inner: crate::memory::owner::Guard,
}
impl Guard {
    pub fn acquire(class: u8) -> Self {
        Self {
            _inner: crate::memory::owner::Guard::acquire(if class == 0 { 8 } else { 9 }),
        }
    }
}
