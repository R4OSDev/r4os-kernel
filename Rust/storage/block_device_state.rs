//! Original device execution/recovery/reset status and result accounting.
//! Resident scalar C field addresses belong to the exact pinned device;
//! no view/reference survives a backend callback, wake, wait or free.
use crate::{memory::owner::Guard, storage::block_accounting::Stats};
use core::ptr;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DeviceState {
    pub state: *mut u8,
    pub resetting: *mut bool,
    pub active: *mut u32,
    pub stats: *mut Stats,
}
/// # Safety
/// Input is an initialized disjoint C descriptor of the exact admitted
/// resident device. Initialization is boot-only; other operations retain
/// original queue/pin admission. The device and fields remain alive for this
/// call. No external callback or blocking operation occurs under metadata.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_device_state(
    input: *const DeviceState,
    operation: u8,
    kind: u8,
    error: u8,
    sectors: u16,
) -> bool {
    let a = unsafe { input.read() };
    let _owner = Guard::acquire(1);
    let active = if operation == 0 {
        0
    } else {
        unsafe { a.active.read() }
    };
    match operation {
        0 => unsafe {
            a.state.write(0);
            a.resetting.write(false);
            a.active.write(0);
            ptr::write_bytes(a.stats, 0, 1);
        },
        1 => unsafe {
            a.state.write(3);
        },
        2 | 3 => unsafe {
            (*a.stats).backend_recoveries += 1;
            if operation == 2 {
                a.state.write(if active != 0 { 2 } else { 1 });
            } else {
                (*a.stats).backend_recovery_failures += 1;
                a.state.write(4);
            }
        },
        4 => {
            if unsafe { a.resetting.read() } {
                return false;
            }
            unsafe {
                a.resetting.write(true);
                a.state.write(3);
            }
        }
        5 => unsafe {
            a.resetting.write(false);
            a.state.write(4);
        },
        6 => unsafe {
            a.resetting.write(false);
            a.state.write(if active == 0 { 1 } else { 2 });
        },
        7 => unsafe {
            let next = active.saturating_add(1);
            a.active.write(next);
            (*a.stats).in_flight_high_water = (*a.stats).in_flight_high_water.max(next);
        },
        8 => unsafe {
            a.active.write(active.saturating_sub(1));
        },
        9 => unsafe {
            (*a.stats).last_error = 0;
            (*a.stats).completions += 1;
            a.state.write(if active != 0 { 2 } else { 1 });
            match kind {
                0 => {}
                1 => {
                    (*a.stats).read_ops += 1;
                    (*a.stats).read_sectors += u64::from(sectors);
                }
                2 => {
                    (*a.stats).write_ops += 1;
                    (*a.stats).write_sectors += u64::from(sectors);
                }
                3 => (*a.stats).flush_ops += 1,
                _ => unreachable!(),
            }
        },
        10 => unsafe {
            (*a.stats).last_error = error;
            if error == 7 {
                (*a.stats).timeout_failures += 1;
            }
            a.state.write(if active != 0 { 2 } else { 4 });
            match kind {
                0 => {}
                1 => (*a.stats).read_failures += 1,
                2 => (*a.stats).write_failures += 1,
                3 => (*a.stats).flush_failures += 1,
                _ => unreachable!(),
            }
        },
        11 => unsafe {
            a.state.write(2);
        },
        _ => unreachable!(),
    }
    true
}
const _: () = {
    assert!(core::mem::size_of::<DeviceState>() == 32);
};
