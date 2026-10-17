//! Value-only acknowledgements for completed virtual release batches.
//! Callers retain the actual VM range; this record owns no resource or lease.
use super::{
    blocks,
    owner::{self, Guard},
    page_batch::MAX_EXTENT_PAGES,
    paging, phys, vm_frames,
    vm_ranges::Range,
};
use crate::arch::tlb;
use core::{cell::UnsafeCell, marker::PhantomData};

struct Frames {
    active: bool,
    values: [u64; MAX_EXTENT_PAGES as usize],
}
struct Storage(UnsafeCell<Frames>);
// SAFETY: every preparation retains VM owner5. The independent busy bit also
// refuses a recursive release on the same CPU while its values are borrowed.
unsafe impl Sync for Storage {}
static FRAMES: Storage = Storage(UnsafeCell::new(Frames {
    active: false,
    values: [0; MAX_EXTENT_PAGES as usize],
}));
struct FrameLease(PhantomData<*mut ()>);
impl FrameLease {
    fn acquire() -> Option<Self> {
        let frames = unsafe { &mut *FRAMES.0.get() };
        if frames.active {
            return None;
        }
        frames.active = true;
        Some(Self(PhantomData))
    }
    fn buffer(&mut self, count: usize) -> &mut [u64] {
        unsafe { &mut (&mut *FRAMES.0.get()).values[..count] }
    }
    fn values(&self, count: usize) -> &[u64] {
        unsafe { &(&*FRAMES.0.get()).values[..count] }
    }
}
impl Drop for FrameLease {
    fn drop(&mut self) {
        unsafe { (*FRAMES.0.get()).active = false };
    }
}

#[derive(Clone, Copy)]
pub(super) struct Proof {
    pub pages: u64,
    pub rounds: u64,
    pub max_pages: u64,
    pub last: tlb::Result,
    pub all_acknowledged: bool,
}
impl Default for Proof {
    fn default() -> Self {
        Self {
            pages: 0,
            rounds: 0,
            max_pages: 0,
            last: tlb::Result::default(),
            all_acknowledged: true,
        }
    }
}
impl Proof {
    pub fn record(&mut self, pages: u64, receipt: tlb::Result) {
        self.pages = self.pages.saturating_add(pages);
        self.rounds = self.rounds.saturating_add(1);
        self.max_pages = self.max_pages.max(pages);
        self.all_acknowledged &=
            receipt.ok && receipt.acknowledgement_mask & receipt.target_mask == receipt.target_mask;
        self.last = receipt;
    }
    pub fn append(&mut self, other: Self) {
        self.pages = self.pages.saturating_add(other.pages);
        self.rounds = self.rounds.saturating_add(other.rounds);
        self.max_pages = self.max_pages.max(other.max_pages);
        self.all_acknowledged &= other.all_acknowledged;
        if other.rounds != 0 {
            self.last = other.last;
        }
    }
}

/// # Safety
/// The caller retains VM owner5 and its exact resident TABLE range. Release
/// performs no blocking I/O or owner handoff; exact bounded TLB waits remain.
/// No physical identity outlives its page-table owner. A vacant first PTE
/// returns a proof with zero pages for the caller's
/// original gap/partial-cursor policy.
pub(super) unsafe fn uncommit(
    range: *mut Range,
    address: u64,
    maximum: u64,
    logical: bool,
) -> Result<Proof, u32> {
    assert!(owner::named(5).held(), "VM release range owner");
    if maximum == 0 || maximum > MAX_EXTENT_PAGES {
        return Err(10);
    }
    let current = unsafe { range.read() };
    if !current.active() {
        return Err(10);
    }
    let _pages = Guard::acquire(6);
    let mut lease = FrameLease::acquire().ok_or(14u32)?;
    let captured = paging::release_frames_snapshot(address, lease.buffer(maximum as usize))
        .map_err(|_| 13u32)?;
    if captured == 0 {
        return Ok(Proof::default());
    }
    let frames = lease.values(captured);
    let expected = blocks::ReleaseOwner {
        kind: current.kind,
        owner: current.owner,
        owner_id: current.owner_id,
    };
    let mut count = captured;
    let mut coalesced = false;
    let prepared = loop {
        match blocks::ReleaseBatch::prepare(&frames[..count], expected) {
            Ok(prepared) => break prepared,
            Err(blocks::Error::TableFull) if !coalesced => {
                // Undo completed before leaving the failed preparation.
                // Existing global free-row coalescing is outside that undo.
                blocks::r4os_rust_blocks_coalesce();
                coalesced = true;
            }
            Err(blocks::Error::TableFull) if count > 1 => count /= 2,
            Err(error) => return Err(vm_frames::block_error(error as u32)),
        }
    };
    // The retained physical owner prevents a competing allocator between
    // this validation and the all-or-nothing post-ACK bitmap operation.
    if !phys::validate_release_extents(prepared.extents()) {
        return Err(15);
    }
    let receipt = paging::release_frames(address, &frames[..count], || {
        phys::release_extents_checked(prepared.extents())
    })
    .map_err(|reason| match reason {
        paging::ReleaseError::Physical => 15u32,
        paging::ReleaseError::Mapping | paging::ReleaseError::Tlb => 13u32,
    })?;
    unsafe { vm_frames::account_release(range, count as u64, logical) };
    prepared.commit();
    let mut proof = Proof::default();
    proof.record(count as u64, receipt);
    Ok(proof)
}
