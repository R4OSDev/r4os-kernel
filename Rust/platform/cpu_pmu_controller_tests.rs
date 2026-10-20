extern crate std;
use super as e;
use super::super::cpu_pmu_decode as d;
use crate::contract as c;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
};
use std::{vec, vec::Vec};
struct Peer {
    p: d::Plan,
    regs: RefCell<BTreeMap<u32, u64>>,
    writes: RefCell<Vec<(u32, u64)>>,
    reads: Cell<usize>,
    write_calls: Cell<usize>,
    work_calls: Cell<u32>,
    clock_calls: Cell<u32>,
    current: Cell<bool>,
    current_calls: Cell<u32>,
    read_fault: usize,
    write_fault: usize,
    write_busy: usize,
    fault_commits: bool,
    amount: u64,
    abort: i32,
    migrate: bool,
    foreign_control: bool,
    foreign_status: bool,
    clock_bad: bool,
    clock_missing: bool,
    reject_callback_admission: bool,
    invalid_raw_once: bool,
    invalid_raw_seen: Cell<bool>,
}
impl Peer {
    fn new(amd: bool, full: bool) -> Self {
        let profile = if amd { d::MILAN_B1 } else { d::INTEL };
        let ds = d::Discovery {
            profile,
            reason: d::NONE,
            version: 4,
            width: 48,
            counters: if amd { 6 } else { 4 },
            events: 63,
            ..Default::default()
        };
        let p = d::plan(&ds, d::INSTRUCTIONS).unwrap();
        let mut regs = BTreeMap::new();
        for (r, v) in [
            (0x345, if full { 1 << 13 } else { 0 }),
            (0x1d9, 0),
            (0x3f1, 0),
            (0x392, 0),
            (0x38f, 0),
            (0x38e, 0),
            (p.control, 0),
            (p.counter, 0x1234),
        ] {
            regs.insert(r, v);
        }
        if amd {
            for i in 0..6 {
                regs.insert(0xc0010200 + 2 * i, 0);
            }
        }
        Self {
            p,
            regs: RefCell::new(regs),
            writes: RefCell::new(vec![]),
            reads: Cell::new(0),
            write_calls: Cell::new(0),
            work_calls: Cell::new(0),
            clock_calls: Cell::new(0),
            current: Cell::new(true),
            current_calls: Cell::new(0),
            read_fault: usize::MAX,
            write_fault: usize::MAX,
            write_busy: usize::MAX,
            fault_commits: false,
            amount: 100,
            abort: 0,
            migrate: false,
            foreign_control: false,
            foreign_status: false,
            clock_bad: false,
            clock_missing: false,
            reject_callback_admission: false,
            invalid_raw_once: false,
            invalid_raw_seen: Cell::new(false),
        }
    }
    fn apply(&self, msr: u32, value: u64) {
        let mut regs = self.regs.borrow_mut();
        if msr == 0x390 {
            let status = regs.get_mut(&0x38e).unwrap();
            *status &= !value;
        } else if msr == self.p.counter || msr == self.p.counter + 0x400 {
            let value = if self.p.profile == d::INTEL && msr == self.p.counter {
                (value as u32 as i32 as i64 as u64) & d::mask(self.p.width).unwrap()
            } else {
                value
            };
            regs.insert(self.p.counter, value);
        } else {
            assert!(
                regs.contains_key(&msr),
                "No invented hardware address {msr:x}"
            );
            regs.insert(msr, value);
        }
    }
    fn run(&self) -> e::Outcome {
        e::run(self.p, d::INSTRUCTIONS, 1, 7, 11, 4096, self)
    }
}
impl e::Hardware for Peer {
    fn current(&self) -> bool {
        let n = self.current_calls.get() + 1;
        self.current_calls.set(n);
        self.current.get() && !(self.reject_callback_admission && n == 2)
    }
    fn now(&self) -> Option<u64> {
        let n = self.clock_calls.get() + 1;
        self.clock_calls.set(n);
        if self.clock_missing {
            None
        } else {
            Some(if n >= 3 && self.clock_bad {
                c::cpu_pmu_max_window_ns + 1000
            } else {
                u64::from(n) * 100
            })
        }
    }
    fn read(&self, msr: u32) -> Result<u64, e::Error> {
        let n = self.reads.get() + 1;
        self.reads.set(n);
        if n == self.read_fault {
            return Err(e::Error::Fault);
        }
        if msr == self.p.counter
            && self.work_calls.get() != 0
            && self.invalid_raw_once
            && !self.invalid_raw_seen.replace(true)
        {
            return Ok(1 << 48);
        }
        self.regs.borrow().get(&msr).copied().ok_or(e::Error::Fault)
    }
    fn write(&self, msr: u32, value: u64) -> e::Write {
        let n = self.write_calls.get() + 1;
        self.write_calls.set(n);
        if n == self.write_busy {
            return e::Write::NotSubmitted;
        }
        self.writes.borrow_mut().push((msr, value));
        if n == self.write_fault {
            if self.fault_commits {
                self.apply(msr, value)
            }
            return e::Write::Faulted;
        }
        self.apply(msr, value);
        e::Write::Written
    }
    fn work(&self, iterations: u32) -> i32 {
        assert_eq!(iterations, 4096);
        self.work_calls.set(self.work_calls.get() + 1);
        let mut r = self.regs.borrow_mut();
        assert_eq!(r[&self.p.control], self.p.select | d::ENABLE);
        assert_eq!(r[&self.p.control] & d::INTERRUPT, 0);
        if self.p.profile == d::INTEL {
            assert_eq!(r[&0x38f], 1)
        } else {
            assert_eq!(self.p.counter_index, 2);
            assert_ne!(self.p.select & (1 << 43), 0)
        }
        let old = r[&self.p.counter];
        let mask = d::mask(self.p.width).unwrap();
        let sum = u128::from(old) + u128::from(self.amount);
        r.insert(self.p.counter, sum as u64 & mask);
        if self.p.profile == d::INTEL && sum > u128::from(mask) {
            r.insert(0x38e, 1);
        }
        if self.foreign_control {
            r.insert(self.p.control, 0x1234_8888_4321);
        }
        if self.foreign_status {
            let v = r[&0x38e];
            r.insert(0x38e, v | (1 << 4));
        }
        if self.migrate {
            self.current.set(false)
        }
        self.abort
    }
}
#[test]
fn exact_native_intel_register_transaction_restores_full_original_state() {
    for full in [false, true] {
        let p = Peer::new(false, full);
        let before = p.regs.borrow().clone();
        let r = p.run();
        assert!(r.release);
        assert_eq!(r.value.status, c::cpu_pmu_status_completed);
        assert_eq!(r.value.delta_modulo, 100);
        assert_ne!(r.value.flags & c::cpu_pmu_result_flag_absolute_count, 0);
        assert_eq!(p.regs.borrow().clone(), before);
        assert_eq!(p.work_calls.get(), 1);
        assert_eq!(r.value.write_attempts as usize, p.writes.borrow().len());
        assert_eq!(r.value.epoch, 7);
        assert_eq!(r.value.generation, 11);
    }
}
#[test]
fn native_intel_full_width_alias_and_signed_legacy_restore_are_exact() {
    for (full, value) in [(true, 0x1234_5678_9abc), (false, 0xffff_8000_0001)] {
        let p = Peer::new(false, full);
        p.regs.borrow_mut().insert(p.p.counter, value);
        let before = p.regs.borrow().clone();
        let r = p.run();
        assert!(r.release);
        assert_eq!(r.value.status, 0);
        assert_eq!(p.regs.borrow().clone(), before);
        assert!(p.writes.borrow().iter().any(|&(msr, v)| msr
            == p.p.counter + if full { 0x400 } else { 0 }
            && v == if full {
                value
            } else {
                value & u64::from(u32::MAX)
            }));
    }
    let p = Peer::new(false, false);
    p.regs.borrow_mut().insert(p.p.counter, 0x1234_5678_9abc);
    let before = p.regs.borrow().clone();
    let r = p.run();
    assert!(r.release);
    assert_eq!(r.value.status, c::cpu_pmu_status_busy);
    assert_eq!((r.value.write_attempts, p.work_calls.get()), (0, 0));
    assert_eq!(p.regs.borrow().clone(), before);
}
#[test]
fn foreign_debug_pebs_inuse_global_status_and_control_refuse_before_writes() {
    for (reg, value) in [
        (0x1d9, 1),
        (0x3f1, 1),
        (0x392, 1),
        (0x38f, 2),
        (0x38e, 1),
        (0x186, d::ENABLE),
        (0x186, d::INTERRUPT),
        (0x186, 1 << 40),
    ] {
        let p = Peer::new(false, true);
        p.regs.borrow_mut().insert(reg, value);
        let before = p.regs.borrow().clone();
        let r = p.run();
        assert!(r.release);
        assert_eq!(r.value.status, c::cpu_pmu_status_busy);
        assert_eq!((r.value.write_attempts, p.work_calls.get()), (0, 0));
        assert_eq!(p.regs.borrow().clone(), before);
    }
}
#[test]
fn exact_milan_erratum_and_modulo_only_result_restore_original_values() {
    for amount in [100, (1 << 48) + 7] {
        let mut p = Peer::new(true, true);
        p.amount = amount;
        let before = p.regs.borrow().clone();
        let r = p.run();
        assert!(r.release);
        assert_eq!(r.value.status, 0);
        assert_eq!(r.value.delta_modulo, amount & ((1 << 48) - 1));
        assert_eq!(r.value.flags & c::cpu_pmu_result_flag_absolute_count, 0);
        assert_ne!(r.value.flags & c::cpu_pmu_result_flag_modulo_delta, 0);
        assert_eq!(p.regs.borrow().clone(), before);
        assert!(
            p.writes
                .borrow()
                .iter()
                .all(|(msr, _)| [p.p.control, p.p.counter].contains(msr))
        );
    }
}

#[test]
fn milan_erratum_bit43_is_not_a_reserved_bit_grant_for_counter0() {
    let mut peer = Peer::new(true, true);
    peer.p.counter_index = 0;
    peer.p.control = 0xc001_0200;
    peer.p.counter = 0xc001_0201;
    peer.p.select = 0x30076;
    peer.regs.borrow_mut().insert(peer.p.control, 1 << 43);
    peer.regs.borrow_mut().insert(peer.p.counter, 0x1234);
    let before = peer.regs.borrow().clone();
    let mut transaction = e::Transaction {
        value: c::CpuPmuResult {
            counter_mask: (1 << 48) - 1,
            ..Default::default()
        },
        touched: false,
        counter_write: peer.p.counter,
        counter_restore: 0,
    };
    assert!(!transaction.preflight(peer.p, &peer));
    assert_eq!(transaction.value.status, c::cpu_pmu_status_busy);
    assert_eq!(peer.write_calls.get(), 0);
    assert_eq!(peer.regs.borrow().clone(), before);
}
#[test]
fn intel_overflow_cannot_become_absolute_count_and_only_own_flag_is_cleared() {
    let mut p = Peer::new(false, true);
    p.amount = (1 << 48) + 9;
    let before = p.regs.borrow().clone();
    let r = p.run();
    assert!(r.release);
    assert_eq!(r.value.status, c::cpu_pmu_status_overflow);
    assert_eq!(r.value.flags & c::cpu_pmu_result_flag_absolute_count, 0);
    assert_ne!(r.value.flags & c::cpu_pmu_result_flag_overflow, 0);
    assert_eq!(r.value.delta_modulo, 9);
    assert_eq!(p.regs.borrow().clone(), before);
    assert_eq!(
        p.writes
            .borrow()
            .iter()
            .filter(|(msr, _)| *msr == 0x390)
            .copied()
            .collect::<Vec<_>>(),
        vec![(0x390, 1)]
    );
}
#[test]
fn callback_abort_restores_counter_but_never_returns_valid_counts() {
    let mut p = Peer::new(false, true);
    p.abort = -3;
    let before = p.regs.borrow().clone();
    let r = p.run();
    assert!(r.release);
    assert_eq!(r.value.status, c::cpu_pmu_status_callback_aborted);
    assert_eq!(r.value.callback_result, -3);
    assert_eq!(
        r.value.flags
            & (c::cpu_pmu_result_flag_absolute_count | c::cpu_pmu_result_flag_modulo_delta),
        0
    );
    assert_ne!(r.value.flags & c::cpu_pmu_result_flag_restored, 0);
    assert_eq!(p.regs.borrow().clone(), before);
}
#[test]
fn all_actual_read_fault_positions_keep_raw_validity_and_cleanup_honest() {
    let good = Peer::new(false, true);
    assert!(good.run().release);
    let count = good.reads.get();
    assert!(count > 10);
    for index in 1..=count {
        let mut p = Peer::new(false, true);
        p.read_fault = index;
        let before = p.regs.borrow().clone();
        let r = p.run();
        assert_eq!(r.value.read_faults, 1, "read{index}");
        assert_eq!(
            r.value.flags & c::cpu_pmu_result_flag_absolute_count,
            0,
            "read{index}"
        );
        if r.release {
            assert_eq!(p.regs.borrow().clone(), before, "read{index}")
        } else {
            assert_eq!(
                r.value.status,
                c::cpu_pmu_status_restore_failed,
                "read{index}"
            )
        }
    }
}
#[test]
fn every_actual_write_fault_and_preinstruction_busy_is_accounted_once() {
    let good = Peer::new(false, true);
    assert!(good.run().release);
    let count = good.write_calls.get();
    assert!(count >= 5);
    for index in 1..=count {
        for committed in [false, true] {
            let mut p = Peer::new(false, true);
            p.write_fault = index;
            p.fault_commits = committed;
            let before = p.regs.borrow().clone();
            let r = p.run();
            assert_eq!(r.value.write_attempts as usize, p.writes.borrow().len());
            assert_eq!(r.value.flags & c::cpu_pmu_result_flag_absolute_count, 0);
            if r.release {
                assert_eq!(p.regs.borrow().clone(), before, "write{index}/{committed}")
            } else {
                assert_eq!(r.value.status, c::cpu_pmu_status_restore_failed)
            }
        }
    }
    let mut p = Peer::new(false, true);
    p.write_busy = 1;
    let before = p.regs.borrow().clone();
    let r = p.run();
    assert!(r.release);
    assert_eq!((r.value.write_attempts, p.work_calls.get()), (0, 0));
    assert_eq!(p.regs.borrow().clone(), before);
}
#[test]
fn changed_cpu_never_restores_a_foreign_cpu_or_releases_the_original_claim() {
    let mut p = Peer::new(false, true);
    p.migrate = true;
    let r = p.run();
    assert!(!r.release);
    assert_eq!(r.value.status, c::cpu_pmu_status_identity_changed);
    assert_eq!(
        r.value.flags
            & (c::cpu_pmu_result_flag_identity_valid | c::cpu_pmu_result_flag_absolute_count),
        0
    );
    assert_eq!(p.writes.borrow().len(), 3);
    assert_eq!(p.regs.borrow()[&0x38f], 1);
}
#[test]
fn foreign_control_or_status_is_preserved_and_retains_uncertain_claim() {
    let mut p = Peer::new(false, true);
    p.foreign_control = true;
    let r = p.run();
    assert!(!r.release);
    assert_eq!(r.value.status, c::cpu_pmu_status_restore_failed);
    assert_eq!(p.regs.borrow()[&p.p.control], 0x1234_8888_4321);
    let mut p = Peer::new(false, true);
    p.foreign_status = true;
    let r = p.run();
    assert!(!r.release);
    assert_eq!(p.regs.borrow()[&0x38e], 1 << 4);
    assert!(p.writes.borrow().iter().all(|&(msr, _)| msr != 0x390));
}
#[test]
fn absent_regressed_or_overbudget_clock_never_substitutes_for_events() {
    let mut p = Peer::new(false, true);
    p.clock_missing = true;
    let r = p.run();
    assert!(r.release);
    assert_eq!(r.value.status, c::cpu_pmu_status_clock_invalid);
    assert_eq!(
        (p.reads.get(), p.write_calls.get(), p.work_calls.get()),
        (0, 0, 0)
    );
    let mut p = Peer::new(false, true);
    p.clock_bad = true;
    let before = p.regs.borrow().clone();
    let r = p.run();
    assert!(r.release);
    assert_eq!(r.value.status, c::cpu_pmu_status_clock_invalid);
    assert_eq!(
        r.value.flags
            & (c::cpu_pmu_result_flag_absolute_count | c::cpu_pmu_result_flag_modulo_delta),
        0
    );
    assert_eq!(p.regs.borrow().clone(), before);
}
#[test]
fn refused_callback_boundary_cannot_claim_a_completed_measurement() {
    let mut p = Peer::new(false, true);
    p.reject_callback_admission = true;
    let before = p.regs.borrow().clone();
    let r = p.run();
    assert!(r.release);
    assert_eq!(p.work_calls.get(), 0);
    assert_eq!(r.value.status, c::cpu_pmu_status_identity_changed);
    assert_eq!(r.value.flags & c::cpu_pmu_result_flag_absolute_count, 0);
    assert_eq!(p.regs.borrow().clone(), before);
}
#[test]
fn an_out_of_width_read_is_failed_data_even_after_exact_cleanup() {
    let mut p = Peer::new(false, true);
    p.invalid_raw_once = true;
    let before = p.regs.borrow().clone();
    let r = p.run();
    assert!(r.release);
    assert_eq!(r.value.status, c::cpu_pmu_status_read_failed);
    assert_eq!(r.value.flags & c::cpu_pmu_result_flag_absolute_count, 0);
    assert_eq!(p.regs.borrow().clone(), before);
}
