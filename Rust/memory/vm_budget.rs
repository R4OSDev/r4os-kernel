//! Derived VM admission totals. The VM range owner publishes every delta;
//! ranges remain authoritative. No resource, reservation or I/O lives here.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Usage {
    pub reserved: u64,
    pub committed: u64,
    pub resident: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Contribution {
    pub usage: Usage,
    pub application: Option<u64>,
    pub pending_resident: u64,
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Total([u128; 4]);
impl Total {
    const ZERO: Self = Self([0; 4]);
    fn replace(&mut self, before: Usage, after: Usage) {
        for (total, (old, new)) in self.0.iter_mut().zip([
            (before.reserved, after.reserved),
            (before.committed, after.committed),
            (before.resident, after.resident),
        ]) {
            *total = *total - u128::from(old) + u128::from(new);
        }
    }
    fn usage(self) -> Usage {
        let narrow = |value: u128| value.min(u128::from(u64::MAX)) as u64;
        Usage {
            reserved: narrow(self.0[0]),
            committed: narrow(self.0[1]),
            resident: narrow(self.0[2]),
        }
    }
    fn pending_replace(&mut self, before: u64, after: u64) {
        self.0[3] = self.0[3] - u128::from(before) + u128::from(after);
    }
    fn admitted_usage(self) -> Usage {
        let mut usage = self.usage();
        usage.resident = (self.0[2] + self.0[3]).min(u128::from(u64::MAX)) as u64;
        usage
    }
}
#[derive(Clone, Copy)]
struct Entry {
    owner: u64,
    ranges: usize,
    bytes: Total,
}
impl Entry {
    const EMPTY: Self = Self {
        owner: 0,
        ranges: 0,
        bytes: Total::ZERO,
    };
}
pub(super) struct Ledger<const N: usize> {
    bytes: Total,
    applications: [Entry; N],
}
impl<const N: usize> Ledger<N> {
    pub const EMPTY: Self = {
        assert!(N.is_power_of_two());
        Self {
            bytes: Total::ZERO,
            applications: [Entry::EMPTY; N],
        }
    };
    pub fn reset(&mut self) {
        self.bytes = Total::ZERO;
        // Initialize resident storage in place, without an N-entry stack value.
        self.applications.fill(Entry::EMPTY);
    }
    fn home(owner: u64) -> usize {
        (owner ^ (owner >> 32)).wrapping_mul(0x9e37_79b9_7f4a_7c15) as usize & (N - 1)
    }
    fn find(&self, owner: u64) -> Option<usize> {
        let home = Self::home(owner);
        for distance in 0..N {
            let at = (home + distance) & (N - 1);
            let entry = self.applications[at];
            if entry.ranges == 0 {
                return None;
            }
            if entry.owner == owner {
                return Some(at);
            }
        }
        None
    }
    fn insert(&mut self, owner: u64, usage: Usage, pending: u64) {
        let home = Self::home(owner);
        for distance in 0..N {
            let entry = &mut self.applications[(home + distance) & (N - 1)];
            if entry.ranges == 0 || entry.owner == owner {
                entry.owner = owner;
                entry.ranges += 1;
                entry.bytes.replace(Usage::default(), usage);
                entry.bytes.pending_replace(0, pending);
                return;
            }
        }
        // At most one distinct application key per live range. A range slot
        // must be available before reserve can publish another contribution.
        unreachable!("VM budget keys exceed live range capacity");
    }
    fn remove(&mut self, owner: u64, usage: Usage, pending: u64) {
        let at = self.find(owner).expect("published VM budget owner");
        let entry = &mut self.applications[at];
        entry.bytes.replace(usage, Usage::default());
        entry.bytes.pending_replace(pending, 0);
        entry.ranges -= 1;
        if entry.ranges != 0 {
            return;
        }
        assert!(entry.bytes == Total::ZERO);
        self.applications[at] = Entry::EMPTY;
        let mut hole = at;
        // Close the probe cluster instead of accumulating tombstones over
        // process generations. The bound also handles a completely full ring.
        for distance in 1..N {
            let next = (at + distance) & (N - 1);
            let entry = self.applications[next];
            if entry.ranges == 0 {
                break;
            }
            let home = Self::home(entry.owner);
            if (next.wrapping_sub(home) & (N - 1)) >= (next.wrapping_sub(hole) & (N - 1)) {
                self.applications[hole] = entry;
                self.applications[next] = Entry::EMPTY;
                hole = next;
            }
        }
    }
    pub fn change(&mut self, before: Contribution, after: Contribution) {
        if before == after {
            return;
        }
        self.bytes.replace(before.usage, after.usage);
        self.bytes
            .pending_replace(before.pending_resident, after.pending_resident);
        match (before.application, after.application) {
            (Some(old), Some(new)) if old == new => {
                let at = self.find(old).expect("published VM budget owner");
                self.applications[at]
                    .bytes
                    .replace(before.usage, after.usage);
                self.applications[at]
                    .bytes
                    .pending_replace(before.pending_resident, after.pending_resident);
            }
            (old, new) => {
                if let Some(owner) = old {
                    self.remove(owner, before.usage, before.pending_resident);
                }
                if let Some(owner) = new {
                    self.insert(owner, after.usage, after.pending_resident);
                }
            }
        }
    }
    pub fn global(&self) -> Usage {
        self.bytes.usage()
    }
    pub fn application(&self, owner: u64) -> Usage {
        self.find(owner).map_or(Usage::default(), |at| {
            self.applications[at].bytes.admitted_usage()
        })
    }
    /// Existing explicit diagnostics may scan ranges. This is never used by
    /// admission and keeps verification scratch independent of range capacity.
    pub fn matches(&self, range: impl Fn(usize) -> Contribution) -> bool {
        let mut global = Total::ZERO;
        for slot in 0..N {
            let contribution = range(slot);
            global.replace(Usage::default(), contribution.usage);
            global.pending_replace(0, contribution.pending_resident);
            if let Some(owner) = contribution.application
                && self.find(owner).is_none()
            {
                return false;
            }
        }
        if self.bytes != global {
            return false;
        }
        for (at, entry) in self.applications.iter().enumerate() {
            if entry.ranges == 0 {
                if entry.bytes != Total::ZERO {
                    return false;
                }
                continue;
            }
            if self.find(entry.owner) != Some(at) {
                return false;
            }
            let mut bytes = Total::ZERO;
            let mut ranges = 0;
            for slot in 0..N {
                let contribution = range(slot);
                if contribution.application == Some(entry.owner) {
                    bytes.replace(Usage::default(), contribution.usage);
                    bytes.pending_replace(0, contribution.pending_resident);
                    ranges += 1;
                }
            }
            if entry.bytes != bytes || entry.ranges != ranges {
                return false;
            }
        }
        true
    }
}
