//! Derived VM admission totals and resident high-water accounting.
//! The VM range owner publishes every delta;
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
    pub process: Option<u64>,
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
    resident: u128,
    peak_resident: u128,
}
impl Entry {
    const EMPTY: Self = Self {
        owner: 0,
        ranges: 0,
        bytes: Total::ZERO,
        resident: 0,
        peak_resident: 0,
    };
}
pub(super) struct Ledger<const N: usize> {
    bytes: Total,
    peak_resident: u128,
    applications: [Entry; N],
}
impl<const N: usize> Ledger<N> {
    pub const EMPTY: Self = {
        assert!(N.is_power_of_two());
        Self {
            bytes: Total::ZERO,
            peak_resident: 0,
            applications: [Entry::EMPTY; N],
        }
    };
    pub fn reset(&mut self) {
        self.bytes = Total::ZERO;
        self.peak_resident = 0;
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
    fn application_contribution(c: Contribution) -> (Usage, u64) {
        if c.application.is_some() {
            (c.usage, c.pending_resident)
        } else {
            (Usage::default(), 0)
        }
    }
    fn insert(&mut self, owner: u64, contribution: Contribution) {
        let (usage, pending) = Self::application_contribution(contribution);
        let home = Self::home(owner);
        for distance in 0..N {
            let entry = &mut self.applications[(home + distance) & (N - 1)];
            if entry.ranges == 0 || entry.owner == owner {
                entry.owner = owner;
                entry.ranges += 1;
                entry.bytes.replace(Usage::default(), usage);
                entry.bytes.pending_replace(0, pending);
                entry.resident += u128::from(contribution.usage.resident);
                entry.peak_resident = entry.peak_resident.max(entry.resident);
                return;
            }
        }
        // At most one distinct process key per live range. A range slot
        // must be available before reserve can publish another contribution.
        unreachable!("VM budget keys exceed live range capacity");
    }
    fn remove(&mut self, owner: u64, contribution: Contribution) {
        let (usage, pending) = Self::application_contribution(contribution);
        let at = self.find(owner).expect("published VM budget owner");
        let entry = &mut self.applications[at];
        entry.bytes.replace(usage, Usage::default());
        entry.bytes.pending_replace(pending, 0);
        entry.resident -= u128::from(contribution.usage.resident);
        entry.ranges -= 1;
        if entry.ranges != 0 {
            return;
        }
        assert!(entry.bytes == Total::ZERO && entry.resident == 0);
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
        self.peak_resident = self.peak_resident.max(self.bytes.0[2]);
        match (before.process, after.process) {
            (Some(old), Some(new)) if old == new => {
                let at = self.find(old).expect("published VM budget owner");
                let (old_usage, old_pending) = Self::application_contribution(before);
                let (new_usage, new_pending) = Self::application_contribution(after);
                let entry = &mut self.applications[at];
                entry.bytes.replace(old_usage, new_usage);
                entry.bytes.pending_replace(old_pending, new_pending);
                entry.resident = entry.resident - u128::from(before.usage.resident)
                    + u128::from(after.usage.resident);
                entry.peak_resident = entry.peak_resident.max(entry.resident);
            }
            (old, new) => {
                if let Some(owner) = old {
                    self.remove(owner, before);
                }
                if let Some(owner) = new {
                    self.insert(owner, after);
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
    fn peak(value: u128) -> (u64, bool) {
        (
            value.min(u128::from(u64::MAX)) as u64,
            value > u128::from(u64::MAX),
        )
    }
    pub fn global_peak(&self) -> (u64, bool) {
        Self::peak(self.peak_resident)
    }
    pub fn process_peak(&self, owner: u64) -> (u64, bool) {
        self.find(owner).map_or((0, false), |at| {
            Self::peak(self.applications[at].peak_resident)
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
            if let Some(owner) = contribution.process
                && self.find(owner).is_none()
            {
                return false;
            }
        }
        if self.bytes != global || self.peak_resident < global.0[2] {
            return false;
        }
        for (at, entry) in self.applications.iter().enumerate() {
            if entry.ranges == 0 {
                if entry.bytes != Total::ZERO || entry.resident != 0 || entry.peak_resident != 0 {
                    return false;
                }
                continue;
            }
            if self.find(entry.owner) != Some(at) {
                return false;
            }
            let mut bytes = Total::ZERO;
            let mut ranges = 0;
            let mut resident = 0;
            for slot in 0..N {
                let contribution = range(slot);
                if contribution.application == Some(entry.owner) {
                    bytes.replace(Usage::default(), contribution.usage);
                    bytes.pending_replace(0, contribution.pending_resident);
                }
                if contribution.process == Some(entry.owner) {
                    ranges += 1;
                    resident += u128::from(contribution.usage.resident);
                }
            }
            if entry.bytes != bytes
                || entry.ranges != ranges
                || entry.resident != resident
                || entry.peak_resident < resident
            {
                return false;
            }
        }
        true
    }
}
