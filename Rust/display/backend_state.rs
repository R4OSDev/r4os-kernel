//! Sole backend transition model. The enclosing display owner serializes it;
//! hardware quiescence and restoration receipts remain with the real driver.
use super::error::Error;
pub const UNAVAILABLE: u32 = 0;
pub const BOOTFB: u32 = 1;
pub const PREPARING: u32 = 2;
pub const NATIVE: u32 = 3;
pub const SOFTWARE_NATIVE: u32 = 4;
pub const RECOVERING: u32 = 5;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Snapshot {
    pub state: u32,
    pub policy: u32,
    pub reason: u32,
    pub revision: u64,
    pub generation: u64,
    pub reset_generation: u64,
    pub owner: usize,
    pub adapter_id: u32,
    pub pending_owner: usize,
    pub pending_adapter_id: u32,
    pub pending_generation: u64,
}
impl Default for Snapshot {
    fn default() -> Self {
        Self {
            state: UNAVAILABLE,
            policy: 0,
            reason: 1,
            revision: 0,
            generation: 0,
            reset_generation: 0,
            owner: 0,
            adapter_id: 0,
            pending_owner: 0,
            pending_adapter_id: 0,
            pending_generation: 0,
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Restored {
    pub owner: usize,
    pub adapter: u32,
    pub generation: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Manager {
    pub value: Snapshot,
    pub serial: u64,
    pub recovering_preparation: bool,
    pub has_restored_boot: bool,
    pub restored_boot: Restored,
}
impl Manager {
    pub fn init_boot(&mut self) {
        *self = Self {
            serial: 1,
            value: Snapshot {
                state: BOOTFB,
                revision: 1,
                generation: 1,
                ..Snapshot::default()
            },
            ..Self::default()
        };
    }
    pub fn set_policy(&mut self, policy: u32) -> Result<(), Error> {
        assert!(policy <= 2, "display policy");
        if !matches!(self.value.state, BOOTFB | UNAVAILABLE)
            || self.value.owner != 0
            || self.value.pending_owner != 0
        {
            return Err(Error::Busy);
        }
        self.value.policy = policy;
        self.value.reason = if policy == 0 { 1 } else { 2 };
        self.changed();
        Ok(())
    }
    pub fn reject(&mut self, reason: u32) {
        assert!(reason <= 8, "display reason");
        if self.value.state != BOOTFB || self.value.policy != 0 {
            return;
        }
        self.value.reason = reason;
        self.changed();
    }
    pub fn begin(&mut self, owner: usize, adapter: u32) -> Result<u64, Error> {
        if owner == 0 || owner > u32::MAX as usize || adapter == 0 {
            return Err(Error::Invalid);
        }
        if self.value.policy != 0 {
            return Err(Error::Disabled);
        }
        if self.value.state != BOOTFB {
            return Err(Error::Busy);
        }
        let generation = self.next_generation()?;
        self.value.state = PREPARING;
        self.value.pending_owner = owner;
        self.value.pending_adapter_id = adapter;
        self.value.pending_generation = generation;
        self.changed();
        Ok(generation)
    }
    pub fn abort(&mut self, owner: usize, generation: u64, reason: u32) -> Result<(), Error> {
        assert!(reason <= 8, "display reason");
        self.check_pending(owner, generation)?;
        self.value.state = if self.recovering_preparation {
            UNAVAILABLE
        } else {
            BOOTFB
        };
        self.value.reason = if self.recovering_preparation {
            8
        } else {
            reason
        };
        self.recovering_preparation = false;
        self.clear_pending();
        self.changed();
        Ok(())
    }
    pub fn commit(&mut self, owner: usize, generation: u64, software: bool) -> Result<(), Error> {
        self.check_pending(owner, generation)?;
        self.adopt_pending();
        self.value.state = if software { SOFTWARE_NATIVE } else { NATIVE };
        self.value.reason = 0;
        self.changed();
        Ok(())
    }
    pub fn fail_commit(&mut self, owner: usize, generation: u64) -> Result<(), Error> {
        self.check_pending(owner, generation)?;
        self.adopt_pending();
        self.value.state = UNAVAILABLE;
        self.value.reason = 6;
        self.changed();
        Ok(())
    }
    pub fn begin_recovery(&mut self, owner: usize, generation: u64) -> Result<u64, Error> {
        self.check_active(owner, generation)?;
        if !matches!(self.value.state, NATIVE | SOFTWARE_NATIVE | UNAVAILABLE) {
            return Err(Error::Busy);
        }
        if self.value.reset_generation == u64::MAX {
            return Err(Error::Exhausted);
        }
        let next = self.next_generation()?;
        self.value.generation = next;
        self.value.reset_generation += 1;
        self.value.state = RECOVERING;
        self.value.reason = 7;
        self.changed();
        Ok(next)
    }
    pub fn begin_device_reset(&mut self, owner: usize, generation: u64) -> Result<u64, Error> {
        if self.value.state == RECOVERING {
            self.check_active(owner, generation)?;
            if self.value.reset_generation == u64::MAX {
                return Err(Error::Exhausted);
            }
            let next = self.next_generation()?;
            self.value.generation = next;
            self.value.reset_generation += 1;
            self.changed();
            return Ok(next);
        }
        if self.value.state != PREPARING {
            return self.begin_recovery(owner, generation);
        }
        self.check_pending(owner, generation)?;
        if self.value.reset_generation == u64::MAX {
            return Err(Error::Exhausted);
        }
        let next = self.next_generation()?;
        self.adopt_pending();
        self.value.generation = next;
        self.value.reset_generation += 1;
        self.value.state = RECOVERING;
        self.value.reason = 7;
        self.changed();
        Ok(next)
    }
    pub fn restored_boot_owner(&self, owner: usize, generation: u64, adapter: u32) -> bool {
        self.has_restored_boot
            && self.value.state == BOOTFB
            && self.value.owner == 0
            && self.value.pending_owner == 0
            && self.value.generation == generation
            && self.restored_boot.generation == generation
            && self.restored_boot.owner == owner
            && self.restored_boot.adapter == adapter
            && owner != 0
            && adapter != 0
    }
    pub fn begin_restored_boot_reset(
        &mut self,
        owner: usize,
        generation: u64,
        adapter: u32,
    ) -> Result<u64, Error> {
        if !self.restored_boot_owner(owner, generation, adapter) {
            return Err(Error::Stale);
        }
        if self.value.reset_generation == u64::MAX {
            return Err(Error::Exhausted);
        }
        let next = self.next_generation()?;
        self.value.generation = next;
        self.value.reset_generation += 1;
        self.value.owner = owner;
        self.value.adapter_id = adapter;
        self.value.state = RECOVERING;
        self.value.reason = 7;
        self.has_restored_boot = false;
        self.changed();
        Ok(next)
    }
    pub fn reprepare(&mut self, owner: usize, generation: u64, adapter: u32) -> Result<u64, Error> {
        self.check_active(owner, generation)?;
        if self.value.state != RECOVERING {
            return Err(Error::Busy);
        }
        if adapter == 0 || adapter != self.value.adapter_id {
            return Err(Error::Stale);
        }
        let next = self.next_generation()?;
        self.value.state = PREPARING;
        self.value.pending_owner = owner;
        self.value.pending_adapter_id = adapter;
        self.value.pending_generation = next;
        self.recovering_preparation = true;
        self.changed();
        Ok(next)
    }
    pub fn recovery_failed(&mut self, owner: usize, generation: u64) -> Result<(), Error> {
        self.check_active(owner, generation)?;
        if self.value.state != RECOVERING {
            return Err(Error::Busy);
        }
        self.value.state = UNAVAILABLE;
        self.value.reason = 8;
        self.changed();
        Ok(())
    }
    pub fn restore_boot(&mut self, owner: usize, generation: u64) -> Result<(), Error> {
        self.check_active(owner, generation)?;
        if self.value.state != RECOVERING {
            return Err(Error::Busy);
        }
        let next = self.next_generation()?;
        self.has_restored_boot = true;
        self.restored_boot = Restored {
            owner,
            adapter: self.value.adapter_id,
            generation: next,
        };
        self.value = Snapshot {
            state: BOOTFB,
            policy: self.value.policy,
            reason: 7,
            revision: self.value.revision,
            generation: next,
            ..Snapshot::default()
        };
        self.changed();
        Ok(())
    }
    pub fn finish_terminal(&mut self, owner: usize, generation: u64) -> Result<(), Error> {
        self.check_active(owner, generation)?;
        if self.value.state != RECOVERING {
            return Err(Error::Busy);
        }
        let next = self.next_generation()?;
        self.value = Snapshot {
            state: UNAVAILABLE,
            policy: self.value.policy,
            reason: 7,
            revision: self.value.revision,
            generation: next,
            reset_generation: self.value.reset_generation,
            ..Snapshot::default()
        };
        self.recovering_preparation = false;
        self.changed();
        Ok(())
    }
    pub fn check_pending(&self, owner: usize, generation: u64) -> Result<(), Error> {
        if owner == 0
            || generation == 0
            || self.value.state != PREPARING
            || self.value.pending_owner != owner
            || self.value.pending_generation != generation
        {
            Err(Error::Stale)
        } else {
            Ok(())
        }
    }
    pub fn check_active(&self, owner: usize, generation: u64) -> Result<(), Error> {
        if owner == 0
            || generation == 0
            || self.value.owner != owner
            || self.value.generation != generation
        {
            Err(Error::Stale)
        } else {
            Ok(())
        }
    }
    pub fn mode_result(
        &mut self,
        owner: usize,
        generation: u64,
        confirmed: bool,
    ) -> Result<(), Error> {
        self.check_active(owner, generation)?;
        if !matches!(self.value.state, SOFTWARE_NATIVE | UNAVAILABLE) {
            return Err(Error::Busy);
        }
        self.value.state = if confirmed {
            SOFTWARE_NATIVE
        } else {
            UNAVAILABLE
        };
        self.value.reason = if confirmed { 0 } else { 6 };
        self.changed();
        Ok(())
    }
    pub fn retains_owner(&self, owner: usize) -> bool {
        owner != 0 && (self.value.owner == owner || self.value.pending_owner == owner)
    }
    fn adopt_pending(&mut self) {
        self.value.owner = self.value.pending_owner;
        self.value.adapter_id = self.value.pending_adapter_id;
        self.value.generation = self.value.pending_generation;
        if self.value.reset_generation == 0 {
            self.value.reset_generation = 1
        }
        self.recovering_preparation = false;
        self.clear_pending();
    }
    fn clear_pending(&mut self) {
        self.value.pending_owner = 0;
        self.value.pending_adapter_id = 0;
        self.value.pending_generation = 0
    }
    fn next_generation(&mut self) -> Result<u64, Error> {
        self.serial = self.serial.checked_add(1).ok_or(Error::Exhausted)?;
        Ok(self.serial)
    }
    fn changed(&mut self) {
        self.value.revision = self.value.revision.saturating_add(1)
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Args {
    pub owner: usize,
    pub generation: u64,
    pub adapter: u32,
    pub reason: u32,
    pub flag: bool,
}
/// # Safety
/// Exact initialized fixed C records, serialized by the existing display owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_backend(
    op: u32,
    state: *mut Manager,
    args: *const Args,
    out: *mut u64,
) -> i32 {
    let args = unsafe { args.read() };
    // Read operations never form an exclusive view of caller-owned metadata.
    let result = match op {
        13 => unsafe { &*state }
            .check_pending(args.owner, args.generation)
            .map(|()| 0),
        14 => unsafe { &*state }
            .check_active(args.owner, args.generation)
            .map(|()| 0),
        16 => Ok(u64::from(unsafe { &*state }.retains_owner(args.owner))),
        17 => Ok(u64::from(unsafe { &*state }.restored_boot_owner(
            args.owner,
            args.generation,
            args.adapter,
        ))),
        _ => {
            let state = unsafe { &mut *state };
            match op {
                0 => {
                    state.init_boot();
                    Ok(0)
                }
                1 => state.set_policy(args.reason).map(|()| 0),
                2 => {
                    state.reject(args.reason);
                    Ok(0)
                }
                3 => state.begin(args.owner, args.adapter),
                4 => state
                    .abort(args.owner, args.generation, args.reason)
                    .map(|()| 0),
                5 => state
                    .commit(args.owner, args.generation, args.flag)
                    .map(|()| 0),
                6 => state.fail_commit(args.owner, args.generation).map(|()| 0),
                7 => state.begin_recovery(args.owner, args.generation),
                8 => state.begin_device_reset(args.owner, args.generation),
                9 => state.reprepare(args.owner, args.generation, args.adapter),
                10 => state
                    .recovery_failed(args.owner, args.generation)
                    .map(|()| 0),
                11 => state.restore_boot(args.owner, args.generation).map(|()| 0),
                12 => state
                    .finish_terminal(args.owner, args.generation)
                    .map(|()| 0),
                15 => state
                    .mode_result(args.owner, args.generation, args.flag)
                    .map(|()| 0),
                18 => state.begin_restored_boot_reset(args.owner, args.generation, args.adapter),
                _ => panic!("display backend operation"),
            }
        }
    };
    match result {
        Ok(value) => {
            unsafe { out.write(value) };
            0
        }
        Err(error) => error as i32,
    }
}
const _: () = {
    use core::mem::{align_of, offset_of, size_of};
    assert!(size_of::<Snapshot>() == 80 && align_of::<Snapshot>() == 8);
    assert!(offset_of!(Snapshot, revision) == 16 && offset_of!(Snapshot, pending_generation) == 72);
    assert!(size_of::<Manager>() == 120 && offset_of!(Manager, restored_boot) == 96);
    assert!(size_of::<Args>() == 32 && offset_of!(Args, flag) == 24);
};
