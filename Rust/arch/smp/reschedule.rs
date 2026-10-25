//! Runtime serializes validated CPU identity, transport credit and receipt.
//! ICR completion is retained separately from the target's real vector entry.
use super::{
    MAX_CPUS,
    apic_target::{self, Target},
    identity_target, lapic, live_target, percpu, runtime,
};
use core::cell::UnsafeCell;
#[path = "reschedule_gate.rs"]
mod reschedule_gate;
use reschedule_gate::Gate;

#[derive(Clone, Copy)]
struct Record {
    gate: Gate<Target>,
    sent: u64,
    merged: u64,
    consumed: u64,
    failed: u64,
}
impl Record {
    const EMPTY: Self = Self {
        gate: Gate::EMPTY,
        sent: 0,
        merged: 0,
        consumed: 0,
        failed: 0,
    };
}
struct Storage(UnsafeCell<[Record; MAX_CPUS]>);
// BSP-only bootstrap, then the existing uninterrupted Runtime owner.
unsafe impl Sync for Storage {}
static RECORDS: Storage = Storage(UnsafeCell::new([Record::EMPTY; MAX_CPUS]));
fn change<T>(index: u32, f: impl FnOnce(&mut Record) -> T) -> T {
    assert!((index as usize) < MAX_CPUS);
    unsafe { f(&mut *RECORDS.0.get().cast::<Record>().add(index as usize)) }
}
#[derive(Clone, Copy)]
pub(crate) struct Stats {
    pub target: Option<Target>,
    pub in_flight: bool,
    pub sent: u64,
    pub merged: u64,
    pub consumed: u64,
    pub failed: u64,
}
pub(crate) fn snapshot(index: u32) -> Stats {
    let _runtime = runtime::RuntimeGuard::acquire();
    change(index, |r| {
        let (target, in_flight) = r.gate.snapshot();
        Stats {
            target,
            in_flight,
            sent: r.sent,
            merged: r.merged,
            consumed: r.consumed,
            failed: r.failed,
        }
    })
}
// Caller holds Runtime. No Record reference spans LAPIC MMIO/ICR delivery.
fn deliver(target: Target, probe: fn(u32) -> Option<Target>) -> bool {
    if !apic_target::current(target, probe) {
        return false;
    }
    let send = change(target.index, |r| {
        let send = r.gate.request(target);
        if !send {
            r.merged = r.merged.wrapping_add(1);
        }
        send
    });
    if !send {
        return true;
    }
    let delivered = lapic::r4os_rust_lapic_control(5, u64::from(target.apic_id), 0xf0) != 0;
    change(target.index, |r| {
        if delivered {
            r.sent = r.sent.wrapping_add(1);
        } else {
            r.failed = r.failed.wrapping_add(1);
            r.gate.failed(target);
        }
    });
    delivered
}
pub(crate) fn live(index: u32) -> bool {
    if index == percpu::current_index() {
        return true;
    }
    let _runtime = runtime::RuntimeGuard::acquire();
    live_target(index).is_some_and(|target| deliver(target, live_target))
}
pub(crate) fn draining(index: u32, expected: u64) -> bool {
    let _runtime = runtime::RuntimeGuard::acquire();
    identity_target(index)
        .filter(|t| t.epoch == expected && percpu::state(index) == percpu::DRAINING)
        .is_some_and(|target| deliver(target, identity_target))
}
/// Actual0xf0 entry only. A software safe point is not a receipt.
/// # Safety
/// The exact Scheduler IRQ admission holds Runtime; this consumes no depth
/// and retains no borrow/token across its subsequent physical switch.
pub(crate) unsafe fn received_owned() {
    let index = percpu::current_index();
    if let Some(target) = identity_target(index) {
        change(index, |r| {
            if r.gate.received(target) {
                r.consumed = r.consumed.wrapping_add(1);
            }
        });
    }
}
