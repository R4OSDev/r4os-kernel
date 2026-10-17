//! R4DEV projects native owner snapshots into the unchanged Contract. Passive
//! observations and explicitly requested probes keep their existing split.
use core::mem::MaybeUninit;
pub mod boot;
pub mod providers;
/// # Safety
/// The supplied owner function fully initializes one T at its output pointer.
pub(crate) unsafe fn snapshot<T>(fill: unsafe extern "C" fn(*mut T)) -> T {
    let mut out = MaybeUninit::uninit();
    // SAFETY: only exact native owner snapshot functions are supplied; each
    // fully initializes the local, aligned, exclusively owned result.
    unsafe {
        fill(out.as_mut_ptr());
        out.assume_init()
    }
}
pub(crate) fn fixed_z(out: &mut [u8], bytes: &[u8]) {
    out.fill(0);
    if !out.is_empty() {
        let len = bytes.len().min(out.len() - 1);
        out[..len].copy_from_slice(&bytes[..len]);
    }
}

pub mod summary;

pub mod telemetry;

pub mod memory;

pub mod memory_probes;

pub mod devices;
pub mod display;
pub mod paging;
pub mod protocol;
pub mod scheduler;
pub mod smp;
pub mod storage;

pub mod block_queue;

pub mod nvme;
