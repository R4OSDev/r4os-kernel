//! Fixed owner snapshots for the original native TaskRegistry probes.
//! Diagnostic constants are test loads, never task/resource capacities.
use crate::{
    arch::fpu,
    memory::{heap, phys},
    sched::task::{
        task_resources as resources,
        task_types::{Bytes, CreateFailure, Entry, ExecutionOwnerKind, Role, State, Task},
    },
};
use core::{mem::MaybeUninit, ptr};
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Vm {
    pub ranges: u64,
    pub reserved: u64,
    pub committed: u64,
    pub resident: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Owner {
    pub started: u64,
    pub id: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Owners {
    pub ipc: Owner,
    pub work: Owner,
    pub block: Owner,
    pub net: Owner,
    pub input: Owner,
    pub input_bound: bool,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Network {
    pub started: bool,
    pub id: u32,
    pub generation: u64,
    pub polls: u64,
    pub iterations: u64,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Baseline {
    pub tasks: u64,
    pub heap_used: usize,
    pub heap_blocks: usize,
    pub pmm_free: u64,
    pub ranges: u64,
    pub reserved: u64,
    pub committed: u64,
    pub resident: u64,
    pub fpu: u64,
}
unsafe extern "C" {

    fn r4os_rust_task_control(operation: u8, a: u64, b: u64) -> u64;
}
pub unsafe fn create(
    name: &'static [u8],
    entry: Entry,
    state: State,
    role: Role,
    failure: *mut CreateFailure,
) -> *mut Task {
    unsafe {
        resources::create(
            resources::CreateArgs {
                name: Bytes {
                    pointer: name.as_ptr(),
                    len: name.len(),
                },
                state,
                needs_fpu: true,
                entry: Some(entry),
                role,
                smp_eligible: false,
                smp_r4x: false,
                owner_kind: ExecutionOwnerKind::None,
                owner_context: ptr::null_mut(),
            },
            failure,
        )
    }
}
pub unsafe fn baseline() -> Baseline {
    unsafe {
        let mut h = MaybeUninit::uninit();
        heap::r4os_rust_heap_stats(h.as_mut_ptr());
        let h = h.assume_init();
        let mut p = phys::Stats::default();
        phys::r4os_rust_phys_stats(&mut p);
        let mut v = Vm::default();
        task_probe_vm(&mut v);
        Baseline {
            tasks: r4os_rust_task_control(1, 0, 0),
            heap_used: h.used_bytes,
            heap_blocks: h.active_blocks,
            pmm_free: p.free_frames,
            ranges: v.ranges,
            reserved: v.reserved,
            committed: v.committed,
            resident: v.resident,
            fpu: r4os_rust_task_control(33, 0, 0),
        }
    }
}
pub unsafe fn matches(expected: Baseline) -> bool {
    unsafe { baseline() == expected && fpu::active_bytes() != 0 }
}
pub unsafe fn owners() -> Owners {
    unsafe {
        let mut out = Owners::default();
        task_probe_owners(&mut out);
        out
    }
}
pub unsafe fn network() -> Network {
    unsafe {
        let mut out = Network::default();
        task_probe_network(&mut out);
        out
    }
}

unsafe fn task_probe_network(out: *mut Network) {
    let mut s = core::mem::MaybeUninit::uninit();
    unsafe {
        crate::net::rx_runtime::r4os_rust_net_rx_runtime_summary(s.as_mut_ptr());
        let s = s.assume_init();
        out.write(Network {
            started: s.started,
            id: s.task_id,
            generation: s.task_generation,
            polls: s.polls,
            iterations: s.iterations,
        });
    }
}

unsafe fn task_probe_vm(out: *mut Vm) {
    let mut s = core::mem::MaybeUninit::uninit();
    unsafe {
        crate::memory::vm_query::r4os_rust_vm_stats(s.as_mut_ptr());
        let s = s.assume_init();
        out.write(Vm {
            ranges: s.active_ranges,
            reserved: s.reserved_bytes,
            committed: s.committed_bytes,
            resident: s.resident_bytes,
        });
    }
}

unsafe fn task_probe_owners(out: *mut Owners) {
    unsafe {
        let mut a = MaybeUninit::uninit();
        let mut b = MaybeUninit::uninit();
        let mut c = MaybeUninit::uninit();
        let mut d = MaybeUninit::uninit();
        let mut e = MaybeUninit::uninit();
        let mut f = MaybeUninit::uninit();
        crate::service::ipc::r4os_rust_ipc_performance(0, false, a.as_mut_ptr());
        crate::driver::work::r4os_rust_driver_work_summary(b.as_mut_ptr());
        crate::storage::block_workers::r4os_rust_block_workers_summary(c.as_mut_ptr());
        crate::net::rx_runtime::r4os_rust_net_rx_runtime_summary(d.as_mut_ptr());
        crate::usb::hid::r4os_rust_usb_hid_poll_task_summary(e.as_mut_ptr());
        crate::usb::hid::r4os_rust_usb_hid_status(f.as_mut_ptr());
        let (a, b, c, d, e, f) = (
            a.assume_init(),
            b.assume_init(),
            c.assume_init(),
            d.assume_init(),
            e.assume_init(),
            f.assume_init(),
        );
        out.write(Owners {
            ipc: Owner {
                started: a.worker_started as u64,
                id: a.worker_task_id,
            },
            work: Owner {
                started: b.worker_started as u64,
                id: b.worker_task_id,
            },
            block: Owner {
                started: c.worker_started as u64,
                id: c.worker_task_id,
            },
            net: Owner {
                started: u64::from(d.started),
                id: d.task_id,
            },
            input: Owner {
                started: u64::from(e.started),
                id: e.task_id,
            },
            input_bound: f.keyboard_bound || f.mouse_bound,
        });
    }
}
