//! Short IRQ-safe input metadata admission. No callback, wake, I/O or wait
//! spans this guard. Serialized pure host fixtures never execute CLI/ports.
use crate::memory::owner::Lock;

use crate::memory::owner::Token;
use core::marker::PhantomData;
pub struct MetadataLock(Lock);
impl MetadataLock {
    pub const fn new() -> Self {
        Self(Lock::new(3, 2))
    }
    pub fn acquire(&'static self) -> Guard {
        Guard {
            lock: self,

            token: self.0.acquire(),
            _cpu_bound: PhantomData,
        }
    }
}
pub struct Guard {
    lock: &'static MetadataLock,

    token: Token,
    _cpu_bound: PhantomData<*mut ()>,
}
impl Drop for Guard {
    fn drop(&mut self) {
        self.lock.0.release(self.token);
    }
}

impl Default for MetadataLock {
    fn default() -> Self {
        Self::new()
    }
}
