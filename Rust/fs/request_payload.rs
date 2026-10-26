//! Borrow only the backend of one exact admitted data read or prepared
//! metadata snapshot; shared namespace/mounted identity stays admitted.
//! Its task-local Guard, mounted use and shared admission remain resident.
use super::{Guard, access, acquire_gate, gate_op, gate_ptr};
use crate::{arch::runtime::RuntimeGuard, sched::sync_locks, storage::access_state::Reference};

pub(crate) struct PayloadBorrow {
    guard: *mut Guard,
    lane: u8,
    owner: access::Owner,
}
impl PayloadBorrow {
    /// Caller retains this exact native Guard on its task stack through Drop.
    /// No Rust exclusive borrow or caller buffer enters scheduler metadata.
    pub(crate) unsafe fn can_start(guard: *mut Guard, reference: Reference) -> bool {
        if guard.is_null() {
            return false;
        }
        let _runtime = RuntimeGuard::acquire();
        let g = unsafe { &*guard };
        if !g.active
            || !g.gates_locked
            || !g.access_owned
            || g.access_mode != access::READ
            || !g.parallel_payload
            || g.lane_count != 1
            || !g.read_reference_present
            || g.read_reference != reference
            || !g.uses[0].present
            || !g.unwind.active
            || access::current() != g.access_owner
        {
            return false;
        }
        let own = unsafe { sync_locks::guard_metadata(gate_ptr(usize::from(g.lanes[0]), false)) };
        own.owner == g.access_owner.id
            && own.generation == g.access_owner.generation
            && own.depth == 1
            && !own.boot_owner
    }
    pub(crate) unsafe fn begin(guard: *mut Guard, reference: Reference) -> Option<Self> {
        if !unsafe { Self::can_start(guard, reference) } {
            return None;
        }
        let _runtime = RuntimeGuard::acquire();
        let lane = unsafe { (*guard).lanes[0] };
        let owner = unsafe { (*guard).access_owner };
        if !access::borrow(lane, owner, true) {
            return None;
        }
        if !unsafe { gate_op(lane, false, 3, 0) } {
            assert!(access::borrow(lane, owner, false));
            return None;
        }
        unsafe {
            (*guard).gates_locked = false;
        }
        Some(Self { guard, lane, owner })
    }
}
impl Drop for PayloadBorrow {
    fn drop(&mut self) {
        assert!(
            access::borrow(self.lane, self.owner, false),
            "FS payload borrow identity"
        );
        // A completed data phase must regain its exact metadata owner before
        // any caller continuation, error query or Guard teardown. Never turn
        // a stalled reacquisition into an unowned metadata continuation.
        while !unsafe { acquire_gate(self.lane, false) } {}
        let _runtime = RuntimeGuard::acquire();
        assert!(unsafe {
            (*self.guard).active
                && (*self.guard).access_owned
                && (*self.guard).access_owner == self.owner
                && !(*self.guard).gates_locked
        });
        unsafe {
            (*self.guard).gates_locked = true;
        }
    }
}
