//! At most two 2-MB table paths for an already bounded contiguous batch.
//! Every pointer belongs to the caller's continuously held page-table owner,
//! including preflight, mutation and acknowledged invalidation or rollback.
use super::{HUGE, Leaf, PAGE, PRESENT, at, index, next, next_or_create, read, table};

#[derive(Clone, Copy)]
struct Path {
    entries: *mut u64,
    bytes: u64,
}
impl Path {
    fn leaf(self, slot: usize) -> Leaf {
        Leaf {
            entry: if self.bytes == PAGE {
                at(self.entries, slot)
            } else {
                self.entries
            },
            huge: self.bytes != PAGE,
            bytes: self.bytes,
        }
    }
}
fn child(parent: *mut u64, slot: usize, create: bool) -> Option<*mut u64> {
    if create {
        next_or_create(parent, slot)
    } else {
        next(parent, slot)
    }
}
fn path(root: u64, address: u64, create: bool) -> Option<Path> {
    let pdpt = child(table(root), index(address, 39), create)?;
    let entry = at(pdpt, index(address, 30));
    if read(entry) & (PRESENT | HUGE) == (PRESENT | HUGE) {
        return Some(Path {
            entries: entry,
            bytes: 1 << 30,
        });
    }
    let pd = child(pdpt, index(address, 30), create)?;
    let entry = at(pd, index(address, 21));
    if read(entry) & (PRESENT | HUGE) == (PRESENT | HUGE) {
        return Some(Path {
            entries: entry,
            bytes: 1 << 21,
        });
    }
    Some(Path {
        entries: child(pd, index(address, 21), create)?,
        bytes: PAGE,
    })
}
pub(super) struct Leaves {
    root: u64,
    base: u64,
    paths: [Option<Path>; 2],
}
impl Leaves {
    pub fn new(root: u64, base: u64) -> Self {
        Self {
            root,
            base,
            paths: [None; 2],
        }
    }
    /// Resolve lazily in original page order. A failed early preflight must
    /// not allocate a later page-table path that it never reached before.
    pub fn prepare(&mut self, page: u64, create: bool) -> Option<Leaf> {
        let slot = index(self.base, 12) + page as usize;
        let cached = &mut self.paths[slot / 512];
        if cached.is_none() {
            *cached = Some(path(self.root, self.base + page * PAGE, create)?);
        }
        Some(cached.expect("prepared batch path").leaf(slot & 511))
    }
    /// Only after every page passed preflight, while retaining the same owner.
    pub fn leaf(&self, page: u64) -> Leaf {
        let slot = index(self.base, 12) + page as usize;
        self.paths[slot / 512]
            .expect("preflight batch path")
            .leaf(slot & 511)
    }
    pub fn entry(&self, page: u64) -> *mut u64 {
        self.leaf(page).entry
    }
}
const _: () = {
    assert!(super::page_batch::MAX_EXTENT_PAGES <= 512);
    assert!(core::mem::size_of::<Leaves>() <= 64);
};
