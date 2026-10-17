//! Before-images for one bounded VM release under the physical owner.
//! Staged catalogue entries cannot admit another allocator: owner 7 remains
//! held until physical release succeeds or every original entry is restored.
use super::{Block, Error, Guard, MAX_BLOCKS, NONE, Result, TABLE, Table};
use crate::memory::{page_batch::MAX_EXTENT_PAGES, phys::Extent};
use core::{cell::UnsafeCell, mem::MaybeUninit};

const PAGE: u64 = 4096;
const MAX_PAGES: usize = MAX_EXTENT_PAGES as usize;
// target, three split outputs and two free neighbours per natural extent.
const MAX_UNDO: usize = 6 * MAX_PAGES;
#[derive(Clone, Copy)]
struct Before {
    slot: usize,
    entry: Block,
}
struct Scratch {
    active: bool,
    count: usize,
    next_id: u32,
    seen: [u64; MAX_BLOCKS / 64],
    before: [MaybeUninit<Before>; MAX_UNDO],
    extents: [Extent; MAX_PAGES],
    extent_count: usize,
}
impl Scratch {
    const EMPTY: Self = Self {
        active: false,
        count: 0,
        next_id: 0,
        seen: [0; MAX_BLOCKS / 64],
        before: [MaybeUninit::uninit(); MAX_UNDO],
        extents: [Extent { base: 0, count: 0 }; MAX_PAGES],
        extent_count: 0,
    };
    fn begin(&mut self, next_id: u32) {
        self.count = 0;
        self.next_id = next_id;
        self.seen.fill(0);
        self.extent_count = 0;
        self.active = true;
    }
    fn remember(&mut self, table: &Table, slot: usize) {
        let bit = 1 << (slot % 64);
        let word = &mut self.seen[slot / 64];
        if *word & bit != 0 {
            return;
        }
        assert!(
            self.count < MAX_UNDO,
            "bounded physical release before-images"
        );
        self.before[self.count].write(Before {
            slot,
            entry: table.entries[slot],
        });
        self.count += 1;
        *word |= bit;
    }
    fn rollback(&mut self, table: &mut Table) {
        for index in (0..self.count).rev() {
            // Each counted image was initialized before its first mutation.
            let before = unsafe { self.before[index].assume_init_read() };
            table.entries[before.slot] = before.entry;
        }
        table.next_id = self.next_id;
        if self.count != 0 {
            // The existing claim journal restores the same derived indices.
            table.rebuild();
        }
        self.active = false;
        self.count = 0;
    }
}
struct Storage(UnsafeCell<Scratch>);
// SAFETY: the physical owner admits the sole transaction; nested admission
// is refused. No mutable table/scratch borrow crosses the TLB/PMM callback;
// only immutable preparation values remain available through the guard.
unsafe impl Sync for Storage {}
static SCRATCH: Storage = Storage(UnsafeCell::new(Scratch::EMPTY));

#[derive(Clone, Copy)]
pub(in crate::memory) struct Owner {
    pub kind: u8,
    pub owner: u8,
    pub owner_id: u64,
}
pub(in crate::memory) struct Batch {
    _guard: Guard,
    armed: bool,
}
impl Batch {
    pub fn prepare(frames: &[u64], expected: Owner) -> Result<Self> {
        if frames.is_empty()
            || frames.len() > MAX_PAGES
            || frames.iter().any(|frame| {
                !frame.is_multiple_of(PAGE) || frame.checked_add(MAX_EXTENT_PAGES * PAGE).is_none()
            })
        {
            return Err(Error::InvalidRange);
        }
        let guard = Guard::acquire(7);
        {
            let table = unsafe { &*TABLE.0.get() };
            let scratch = unsafe { &mut *SCRATCH.0.get() };
            if !table.initialized {
                return Err(Error::NotInitialized);
            }
            if scratch.active {
                return Err(Error::InvalidRange);
            }
            scratch.begin(table.next_id);
        }
        // Only the admitted transaction gets a rollback guard. Its owner and
        // before-images already exist before the first fallible preparation.
        let batch = Self {
            _guard: guard,
            armed: true,
        };
        {
            let table = unsafe { &mut *TABLE.0.get() };
            let scratch = unsafe { &mut *SCRATCH.0.get() };
            let mut first = 0;
            while first < frames.len() {
                let base = frames[first];
                let slot = table.containing(base, PAGE, false).ok_or(Error::NotFound)?;
                let block = table.entries[slot];
                if block.kind != expected.kind
                    || block.owner != expected.owner
                    || block.owner_id != expected.owner_id
                {
                    return Err(Error::InvalidRange);
                }
                let available =
                    table.prefix(base, (frames.len() - first) as u64 * PAGE, false) / PAGE;
                let mut pages = 1;
                while pages < available && frames[first + pages as usize] == base + pages * PAGE {
                    pages += 1;
                }
                // build_release is value-only; prepare_release's implicit
                // coalesce must stay outside this bounded undo transaction.
                let plan = table.build_release(base, pages * PAGE)?;
                scratch.remember(table, plan.target_index);
                for index in 0..plan.write_count as usize {
                    scratch.remember(table, plan.write_indices[index]);
                }
                for slot in [plan.merge_primary_index, plan.merge_secondary_index] {
                    if slot != NONE {
                        scratch.remember(table, slot);
                    }
                }
                table.finish_release(&plan, false);
                scratch.extents[scratch.extent_count] = Extent { base, count: pages };
                scratch.extent_count += 1;
                first += pages as usize;
            }
        }
        Ok(batch)
    }
    /// Immutable preparation values, valid only while this exact owner lives.
    pub fn extents(&self) -> &[Extent] {
        let scratch = unsafe { &*SCRATCH.0.get() };
        &scratch.extents[..scratch.extent_count]
    }
    pub fn commit(mut self) {
        let scratch = unsafe { &mut *SCRATCH.0.get() };
        scratch.active = false;
        scratch.count = 0;
        self.armed = false;
    }
}
impl Drop for Batch {
    fn drop(&mut self) {
        if self.armed {
            unsafe { (&mut *SCRATCH.0.get()).rollback(&mut *TABLE.0.get()) };
        }
    }
}
const _: () = assert!(core::mem::size_of::<Scratch>() <= 40 * 1024);
