//! Single CPU-local table. BSP configures immutable identities before AP
//! publication; runtime admission serializes lifecycle and scheduling masks.
//! The CPU-bound depth slot is raw C storage, accessed with local IF=0.

use super::msr;
use super::{runtime::RuntimeGuard, smp_policy::MAX_CPUS};
use core::{
    cell::UnsafeCell,
    ptr,
    sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
};
#[repr(C)]
pub struct CpuLocal {
    index: u32,
    apic_id: u32,
    state: AtomicU8,
    reserved0: [u8; 3],
    runtime_critical_depth: UnsafeCell<u32>,
    scheduler_ready: AtomicU8,
    work_active: AtomicU8,
    reserved: [u8; 46],
}
impl CpuLocal {
    const fn empty() -> Self {
        Self {
            index: 0,
            apic_id: 0,
            state: AtomicU8::new(0),
            reserved0: [0; 3],
            runtime_critical_depth: UnsafeCell::new(0),
            scheduler_ready: AtomicU8::new(0),
            work_active: AtomicU8::new(0),
            reserved: [0; 46],
        }
    }
}
#[repr(align(64))]
struct Storage(UnsafeCell<[CpuLocal; MAX_CPUS]>);
// SAFETY: singleton BSP initialization precedes all readers; identities are
// immutable after detected publication. State/flags are atomic. Each depth
// slot belongs to its exact CPU with IF=0, including raw C runtime callbacks.
unsafe impl Sync for Storage {}
static LOCALS: Storage = Storage(UnsafeCell::new([const { CpuLocal::empty() }; MAX_CPUS]));
static READY: AtomicBool = AtomicBool::new(false);
static SCHEDULABLE: AtomicU64 = AtomicU64::new(0);
static PRODUCTIVE_KERNEL: AtomicU64 = AtomicU64::new(0);
static PRODUCTIVE_R4X: AtomicU64 = AtomicU64::new(0);

unsafe extern "C" {
    fn r4os_current_cpu_index() -> u32;
}
#[inline]
fn local(index: u32) -> Option<*mut CpuLocal> {
    if index as usize >= MAX_CPUS {
        None
    } else {
        Some(unsafe { LOCALS.0.get().cast::<CpuLocal>().add(index as usize) })
    }
}
pub fn current_index() -> u32 {
    {
        if !READY.load(Ordering::Acquire) {
            return 0;
        }
        let index = unsafe { r4os_current_cpu_index() };
        if index as usize >= MAX_CPUS { 0 } else { index }
    }
}
fn install(index: u32) {
    let Some(local) = local(index) else { return };

    unsafe {
        // R4OS does not swapgs; both bases retain the exact installed slot.
        msr::write(0xc000_0101, local.addr() as u64);
        msr::write(0xc000_0102, local.addr() as u64);
    }
}
fn init_bsp(apic_id: u32) {
    for index in 0..MAX_CPUS as u32 {
        unsafe { local(index).unwrap().write(CpuLocal::empty()) };
    }
    let bsp = local(0).unwrap();
    unsafe {
        ptr::addr_of_mut!((*bsp).index).write(0);
        ptr::addr_of_mut!((*bsp).apic_id).write(apic_id);
        (*bsp).state.store(4, Ordering::Relaxed);
    }
    install(0);
    SCHEDULABLE.store(1, Ordering::Release);
    PRODUCTIVE_KERNEL.store(0, Ordering::Release);
    PRODUCTIVE_R4X.store(0, Ordering::Release);
    READY.store(true, Ordering::Release);
}
pub fn state(index: u32) -> u8 {
    local(index).map_or(0, |local| unsafe { (*local).state.load(Ordering::Acquire) })
}
fn allowed(from: u8, to: u8) -> bool {
    match from {
        1 => to == 2 || to == 7,
        2 => to == 3 || to == 7,
        3 => to == 8 || to == 7,
        8 => to == 4 || to == 7,
        4 => to == 5,
        5 => to == 6,
        0 | 6 | 7 => false,
        _ => panic!("Invalid private CPU lifecycle state"),
    }
}
fn transition_held(index: u32, from: u8, to: u8) -> bool {
    let Some(local) = local(index) else {
        return false;
    };
    if unsafe {
        (*local)
            .state
            .compare_exchange(from, to, Ordering::AcqRel, Ordering::Acquire)
    }
    .is_err()
    {
        return false;
    }
    let bit = 1u64 << index;
    if to == 4 {
        SCHEDULABLE.fetch_or(bit, Ordering::AcqRel);
    } else {
        SCHEDULABLE.fetch_and(!bit, Ordering::AcqRel);
    }
    true
}
fn configure(index: u32, apic_id: u32) -> bool {
    let Some(local) = local(index) else {
        return false;
    };
    let _owner = RuntimeGuard::acquire();
    if state(index) != 0 {
        return false;
    }
    unsafe {
        ptr::addr_of_mut!((*local).index).write(index);
        ptr::addr_of_mut!((*local).apic_id).write(apic_id);
        (*local).state.store(1, Ordering::Release);
    }
    true
}
fn transition(index: u32, from: u8, to: u8) -> bool {
    if !allowed(from, to) {
        return false;
    }
    let _owner = RuntimeGuard::acquire();
    transition_held(index, from, to)
}
fn reject_start(index: u32) -> bool {
    let _owner = RuntimeGuard::acquire();
    let from = state(index);
    if from == 7 {
        return true;
    }
    allowed(from, 7) && transition_held(index, from, 7)
}
fn lifecycle_mask(qualified: bool) -> u64 {
    let mut mask = 0;
    for index in 0..MAX_CPUS as u32 {
        let current = state(index);
        if current == 4 || (qualified && (current == 8 || current == 5)) {
            mask |= 1 << index;
        }
    }
    mask
}
fn flag(index: u32, work: bool) -> Option<&'static AtomicU8> {
    let local = local(index)?;
    // Immutable atomic reference remains local to one load/store; never C.
    Some(unsafe {
        if work {
            &(*local).work_active
        } else {
            &(*local).scheduler_ready
        }
    })
}
/// Exact static C slot. Caller follows immutable identity, atomic flags and
/// CPU-bound IF=0 depth rules; the pointer cannot replace lifecycle admission.
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_percpu_slot(index: u32) -> *mut CpuLocal {
    local(index).unwrap_or(ptr::null_mut())
}
/// CPU-local raw depth storage, valid until shutdown. Foreign runtime owners
/// retain IF=0 and never lend a slot across a context switch or another CPU.
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_percpu_depth() -> *mut u32 {
    unsafe {
        (*local(current_index()).unwrap())
            .runtime_critical_depth
            .get()
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_percpu_control(
    operation: u8,
    index: u32,
    value: u64,
    extra: u8,
) -> u64 {
    match operation {
        0 => {
            init_bsp(index);
            0
        }
        1 => {
            install(index);
            0
        }
        2 => u64::from(current_index()),
        3 => u64::from(configure(index, value as u32)),
        4 => u64::from(transition(index, value as u8, extra)),
        5 => u64::from(reject_start(index)),
        6 => lifecycle_mask(true),
        7 => u64::from(state(index)),
        8 => local(index).map_or(u64::MAX, |local| {
            u64::from(unsafe { ptr::addr_of!((*local).apic_id).read() })
        }),
        9 | 11 => {
            if let Some(flag) = flag(index, operation == 11) {
                flag.store(u8::from(value != 0), Ordering::Release);
            }
            0
        }
        10 | 12 => u64::from(
            flag(index, operation == 12).is_some_and(|flag| flag.load(Ordering::Acquire) != 0),
        ),
        13 => u64::from(
            index < MAX_CPUS as u32 && SCHEDULABLE.load(Ordering::Acquire) & (1 << index) != 0,
        ),
        14 => SCHEDULABLE.load(Ordering::Acquire),
        15 => u64::from(SCHEDULABLE.load(Ordering::Acquire).count_ones()),
        16 => lifecycle_mask(false),
        17 => {
            if index >= MAX_CPUS as u32 {
                return 0;
            }
            let target = if value != 0 {
                &PRODUCTIVE_R4X
            } else {
                &PRODUCTIVE_KERNEL
            };
            let bit = 1u64 << index;
            u64::from(target.fetch_or(bit, Ordering::AcqRel) & bit == 0)
        }
        18 => PRODUCTIVE_KERNEL.load(Ordering::Acquire) | PRODUCTIVE_R4X.load(Ordering::Acquire),
        19 => PRODUCTIVE_KERNEL.load(Ordering::Acquire),
        20 => PRODUCTIVE_R4X.load(Ordering::Acquire),
        _ => panic!("Invalid private per-CPU operation"),
    }
}
const _: () = {
    assert!(core::mem::size_of::<CpuLocal>() == 64);
    assert!(core::mem::offset_of!(CpuLocal, index) == 0);
    assert!(core::mem::offset_of!(CpuLocal, runtime_critical_depth) == 12);
    assert!(core::mem::offset_of!(CpuLocal, scheduler_ready) == 16);
    assert!(core::mem::align_of::<Storage>() == 64);
};
