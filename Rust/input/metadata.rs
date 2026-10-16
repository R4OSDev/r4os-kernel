//! Short IRQ-safe input metadata admission. No callback, wake, I/O or wait
//! spans this guard. Serialized pure host fixtures never execute CLI/ports.
use crate::memory::owner::Lock;
#[cfg(not(r4os_owner_fixture))]
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
            #[cfg(not(r4os_owner_fixture))]
            token: self.0.acquire(),
            _cpu_bound: PhantomData,
        }
    }
}
pub struct Guard {
    lock: &'static MetadataLock,
    #[cfg(not(r4os_owner_fixture))]
    token: Token,
    _cpu_bound: PhantomData<*mut ()>,
}
impl Drop for Guard {
    fn drop(&mut self) {
        #[cfg(not(r4os_owner_fixture))]
        self.lock.0.release(self.token);
        #[cfg(r4os_owner_fixture)]
        let _ = &self.lock.0;
    }
}

impl Default for MetadataLock {
    fn default() -> Self {
        Self::new()
    }
}
