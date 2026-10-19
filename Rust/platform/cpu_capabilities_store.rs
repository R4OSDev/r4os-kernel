//! Copy-only atomic publication for one CPU's execution epoch. All words and
//! the sequence share SeqCst ordering, so readers never accept a partial image
//! and no non-atomic seqlock data race or borrowed replacement image exists.
use core::sync::atomic::{AtomicU64, Ordering::SeqCst};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Error {
    Busy,
    Epoch,
    Overflow,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Snapshot<const N: usize> {
    pub epoch: u64,
    pub sequence: u64,
    pub words: [u64; N],
}
pub(crate) struct Store<const N: usize> {
    sequence: AtomicU64,
    epoch: AtomicU64,
    words: [AtomicU64; N],
}
pub(crate) fn next_sequence(old: u64) -> Result<u64, Error> {
    if old & 1 != 0 {
        return Err(Error::Busy);
    }
    old.checked_add(2).ok_or(Error::Overflow)
}
impl<const N: usize> Store<N> {
    pub(crate) const fn new() -> Self {
        Self {
            sequence: AtomicU64::new(0),
            epoch: AtomicU64::new(0),
            words: [const { AtomicU64::new(0) }; N],
        }
    }
    /// The hardware owner established exact current CPU/epoch admission. A
    /// repeated or older epoch cannot replace an already published snapshot.
    pub(crate) fn publish(&self, epoch: u64, values: [u64; N]) -> Result<u64, Error> {
        let old = self.sequence.load(SeqCst);
        let next = next_sequence(old)?;
        if epoch == 0 || epoch <= self.epoch.load(SeqCst) {
            return Err(Error::Epoch);
        }
        self.sequence
            .compare_exchange(old, old + 1, SeqCst, SeqCst)
            .map_err(|_| Error::Busy)?;
        for (word, value) in self.words.iter().zip(values) {
            word.store(value, SeqCst);
        }
        self.epoch.store(epoch, SeqCst);
        self.sequence.store(next, SeqCst);
        Ok(next)
    }
    pub(crate) fn snapshot(&self) -> Option<Snapshot<N>> {
        let sequence = self.sequence.load(SeqCst);
        if sequence == 0 || sequence & 1 != 0 {
            return None;
        }
        let epoch = self.epoch.load(SeqCst);
        let words = core::array::from_fn(|i| self.words[i].load(SeqCst));
        if self.sequence.load(SeqCst) != sequence {
            return None;
        }
        Some(Snapshot {
            epoch,
            sequence,
            words,
        })
    }
}
