//! One bounded atomic record set per CPU. No lock/seqlock data race, wait,
//! allocation or foreign lifetime exists on the terminal publication path.
use super::machine_check_capture::LIMIT;
use crate::contract as c;
use core::{
    mem,
    sync::atomic::{AtomicU32, AtomicU64, Ordering::SeqCst},
};

pub(crate) struct Slot {
    sequence: AtomicU64,
    terminal: AtomicU32,
    nested: AtomicU32,
    info: [AtomicU64; 18],
    banks: [[AtomicU64; 14]; LIMIT],
}
impl Slot {
    pub(crate) const fn new() -> Self {
        Self {
            sequence: AtomicU64::new(0),
            terminal: AtomicU32::new(0),
            nested: AtomicU32::new(0),
            info: [const { AtomicU64::new(0) }; 18],
            banks: [const { [const { AtomicU64::new(0) }; 14] }; LIMIT],
        }
    }
    /// Only the exact local IF=0 boot owner or delivered #MC may publish.
    /// A terminal claim may replace an interrupted local boot publication:
    /// that boot continuation is never resumed. Repeated #MC never overwrites.
    pub(crate) fn begin(&self, terminal: bool) -> Option<u64> {
        if terminal {
            if self
                .terminal
                .compare_exchange(0, 1, SeqCst, SeqCst)
                .is_err()
            {
                let _ = self
                    .nested
                    .try_update(SeqCst, SeqCst, |n| Some(n.saturating_add(1)));
                return None;
            }
        } else if self.terminal.load(SeqCst) != 0 {
            return None;
        }
        let old = self.sequence.load(SeqCst);
        if !terminal && old & 1 != 0 {
            return None;
        }
        let next = (old | 1).checked_add(1)?;
        self.sequence.store(next - 1, SeqCst);
        Some(next)
    }
    pub(crate) fn bank(&self, value: c::CpuMachineBank) {
        if let Some(row) = self.banks.get(value.bank_index as usize) {
            // Canonical fixed records have no implicit/uninitialized padding.
            let words: [u64; 14] = unsafe { mem::transmute(value) };
            for (cell, word) in row.iter().zip(words) {
                cell.store(word, SeqCst);
            }
        }
    }
    pub(crate) fn finish(&self, value: c::CpuMachineInfo) {
        let words: [u64; 18] = unsafe { mem::transmute(value) };
        for (cell, word) in self.info.iter().zip(words) {
            cell.store(word, SeqCst);
        }
        self.sequence.store(value.sequence, SeqCst);
    }
    pub(crate) fn info(&self) -> Result<c::CpuMachineInfo, i32> {
        let before = self.sequence.load(SeqCst);
        if before == 0 || before & 1 != 0 {
            return Err(c::cpu_capabilities_error_busy);
        }
        let words: [u64; 18] = core::array::from_fn(|i| self.info[i].load(SeqCst));
        if self.sequence.load(SeqCst) != before {
            return Err(c::cpu_capabilities_error_busy);
        }
        let mut value: c::CpuMachineInfo = unsafe { mem::transmute(words) };
        value.nested_errors = self.nested.load(SeqCst);
        Ok(value)
    }
    pub(crate) fn get_bank(
        &self,
        epoch: u64,
        sequence: u64,
        index: u32,
    ) -> Result<c::CpuMachineBank, i32> {
        let info = self.info()?;
        if epoch == 0 || info.epoch != epoch {
            return Err(c::cpu_capabilities_error_epoch);
        }
        if sequence == 0 || sequence != info.sequence {
            return Err(c::cpu_capabilities_error_busy);
        }
        if index >= info.stored_banks || index as usize >= LIMIT {
            return Err(c::cpu_capabilities_end);
        }
        let words: [u64; 14] = core::array::from_fn(|i| self.banks[index as usize][i].load(SeqCst));
        if self.sequence.load(SeqCst) != sequence {
            return Err(c::cpu_capabilities_error_busy);
        }
        Ok(unsafe { mem::transmute::<[u64; 14], c::CpuMachineBank>(words) })
    }
}
const _: () = {
    assert!(mem::size_of::<c::CpuMachineInfo>() == 144);
    assert!(mem::offset_of!(c::CpuMachineInfo, vendor_name) == 120);
    assert!(mem::offset_of!(c::CpuMachineInfo, reserved0) == 136);
    assert!(mem::size_of::<c::CpuMachineBank>() == 112);
    assert!(mem::offset_of!(c::CpuMachineBank, status) == 48);
};

#[cfg(test)]
mod tests {
    use super::*;
    fn publish(slot: &Slot, epoch: u64, terminal: bool) -> u64 {
        let sequence = slot.begin(terminal).unwrap();
        slot.bank(c::CpuMachineBank {
            cpu_index: 2,
            epoch,
            sequence,
            status: epoch * 11,
            ..Default::default()
        });
        slot.finish(c::CpuMachineInfo {
            cpu_index: 2,
            epoch,
            sequence,
            origin: if terminal { 2 } else { 1 },
            stored_banks: 1,
            ..Default::default()
        });
        sequence
    }
    #[test]
    fn incomplete_capture_is_busy_and_never_exposes_partial_words() {
        let slot = Slot::new();
        assert_eq!(slot.info().err(), Some(c::cpu_capabilities_error_busy));
        assert_eq!(slot.begin(false), Some(2));
        slot.bank(c::CpuMachineBank {
            epoch: 19,
            ..Default::default()
        });
        assert_eq!(slot.info().err(), Some(c::cpu_capabilities_error_busy));
        assert_eq!(slot.begin(false), None);
    }
    #[test]
    fn exact_epoch_sequence_and_end_are_distinct() {
        let slot = Slot::new();
        let seq = publish(&slot, 19, false);
        assert_eq!(
            slot.get_bank(18, seq, 0).err(),
            Some(c::cpu_capabilities_error_epoch)
        );
        assert_eq!(
            slot.get_bank(19, seq + 2, 0).err(),
            Some(c::cpu_capabilities_error_busy)
        );
        assert_eq!(
            slot.get_bank(19, seq, 1).err(),
            Some(c::cpu_capabilities_end)
        );
        assert_eq!(slot.get_bank(19, seq, 0).unwrap().status, 209);
    }
    #[test]
    fn terminal_claim_replaces_interrupted_boot_and_never_resumes_it() {
        let slot = Slot::new();
        assert_eq!(slot.begin(false), Some(2));
        let sequence = publish(&slot, 21, true);
        assert_eq!(sequence, 2);
        assert_eq!(slot.info().unwrap().origin, 2);
        assert_eq!(slot.begin(true), None);
        assert_eq!(slot.begin(false), None);
        assert_eq!(slot.info().unwrap().nested_errors, 1);
    }
    #[test]
    fn fresh_epoch_cannot_be_read_with_previous_bank_identity() {
        let slot = Slot::new();
        let seq = publish(&slot, 5, false);
        let next = publish(&slot, 6, false);
        assert!(next > seq);
        assert_eq!(
            slot.get_bank(5, seq, 0).err(),
            Some(c::cpu_capabilities_error_epoch)
        );
        assert_eq!(slot.get_bank(6, next, 0).unwrap().status, 66);
    }
    #[test]
    fn simultaneous_terminal_claims_have_one_writer_and_bounded_rejections() {
        let slot = std::sync::Arc::new(Slot::new());
        let threads: std::vec::Vec<_> = (0..32)
            .map(|_| {
                let slot = slot.clone();
                std::thread::spawn(move || slot.begin(true))
            })
            .collect();
        let sequences: std::vec::Vec<_> = threads
            .into_iter()
            .filter_map(|t| t.join().unwrap())
            .collect();
        assert_eq!(sequences, [2]);
        slot.finish(c::CpuMachineInfo {
            epoch: 7,
            sequence: 2,
            origin: 2,
            ..Default::default()
        });
        assert_eq!(slot.info().unwrap().nested_errors, 31);
    }
    #[test]
    fn parallel_cpus_retain_independent_exact_snapshots() {
        let threads: std::vec::Vec<_> = (1..=4)
            .map(|epoch| {
                std::thread::spawn(move || {
                    let slot = Slot::new();
                    let sequence = publish(&slot, epoch, true);
                    let bank = slot.get_bank(epoch, sequence, 0).unwrap();
                    (slot.info().unwrap().epoch, bank.status)
                })
            })
            .collect();
        for (i, t) in threads.into_iter().enumerate() {
            let e = i as u64 + 1;
            assert_eq!(t.join().unwrap(), (e, e * 11));
        }
    }
}
