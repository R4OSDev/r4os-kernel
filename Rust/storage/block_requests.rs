//! Original bounded request geometry/FIFO/completion metadata helpers.
//! Embedded resident C records keep the existing device queue owner. These
//! functions never wait, call a backend, allocate or retain a slot reference.
use crate::{memory::owner::Guard, storage::dispatch::Latch};
use core::ptr;
#[repr(C)]
pub struct Slot {
    pub state: u8,
    pub id: u64,
    pub kind: u8,
    pub lba: u64,
    pub sectors: u16,
    pub buffer: *mut u8,
    pub const_buffer: *const u8,
    pub buffer_len: usize,
    pub ok: bool,
    pub err: u8,
    pub submit_tick: u64,
    pub start_tick: u64,
    pub complete_tick: u64,
    pub buffer_ownership: u8,
    pub timeout_requested: bool,
    pub caller_detached: bool,
    pub cancel_requested: bool,
    pub completion_override: u8,
    pub(crate) pending_cancel_reason: u8,
    pub backend_handle: u64,
    pub completion_latch: Latch,
    pub execution_mode: u8,
    pub(crate) observation: *const crate::storage::block_observation::Subscription,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Snapshot {
    pub id: u64,
    pub kind: u8,
    pub lba: u64,
    pub sectors: u16,
}
#[repr(C)]
#[derive(Default)]
pub struct ResultRecord {
    pub ok: bool,
    pub err: u8,
    pub buffer_detached: bool,
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_block_async_submission(mode: u8, has_submit: bool) -> bool {
    has_submit && mode == 1
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_block_backend_error(kind: u8) -> u8 {
    match kind {
        0 => 2,
        1 => 8,
        2 => 9,
        3 => 10,
        _ => unreachable!(),
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_block_validate(
    sector_size: u32,
    sector_count: u64,
    lba: u64,
    sectors: u16,
    bytes: usize,
) -> u8 {
    if sectors == 0 {
        return 2;
    }
    if u64::from(sectors) > u64::MAX - lba {
        return 4;
    }
    if bytes < usize::from(sectors) * sector_size as usize {
        return 5;
    }
    if sector_count != 0 && (lba >= sector_count || u64::from(sectors) > sector_count - lba) {
        return 4;
    }
    0
}
/// # Safety
/// Output is one exclusive aligned C completion result; kind is an original
/// request kind. Expected byte count is the exact accepted request length.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_classify(
    kind: u8,
    len: usize,
    result: i32,
    bytes: u32,
    out: *mut ResultRecord,
) {
    let error = if result == 0 {
        let expected = match kind {
            1 | 2 => u32::try_from(len).expect("Block completion length overflow"),
            0 | 3 => 0,
            _ => unreachable!(),
        };
        if bytes == expected {
            0
        } else {
            r4os_rust_block_backend_error(kind)
        }
    } else {
        match result {
            -2 => 11,
            -3 => 7,
            -4 => 12,
            -5 => 13,
            _ => r4os_rust_block_backend_error(kind),
        }
    };
    unsafe {
        out.write(ResultRecord {
            ok: error == 0,
            err: error,
            buffer_detached: false,
        })
    };
}
/// # Safety
/// Input is one initialized resident slot under the caller's queue admission;
/// output is an exclusive disjoint C Snapshot. The short storage metadata owner
/// also excludes IRQ completion publication; no table borrow crosses a wait.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_snapshot(slot: *const Slot, out: *mut Snapshot) {
    let _owner = Guard::acquire(1);
    unsafe {
        out.write(Snapshot {
            id: (*slot).id,
            kind: (*slot).kind,
            lba: (*slot).lba,
            sectors: (*slot).sectors,
        })
    };
}
/// # Safety
/// Exactly16 initialized stable C slots; task queue admission protects all
/// request mutations and storage owner excludes IRQ latch publication. Select
/// free/queued/id/used count/active flush with op0..4. No raw slot is returned.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_select(slots: *const Slot, op: u8, id: u64) -> usize {
    let _owner = Guard::acquire(1);
    let mut selected = usize::MAX;
    let mut count = 0;
    for i in 0..16 {
        let p = unsafe { slots.add(i) };
        let state = unsafe { (*p).state };
        match op {
            0 => {
                if state == 0 {
                    return i;
                }
            }
            1 => {
                if state == 1
                    && (selected == usize::MAX || unsafe { (*p).id < (*slots.add(selected)).id })
                {
                    selected = i;
                }
            }
            2 => {
                if state != 0 && unsafe { (*p).id == id } {
                    return i;
                }
            }
            3 => {
                if state != 0 {
                    count += 1;
                }
            }
            4 => {
                if state == 2 && unsafe { (*p).kind == 3 } {
                    return 1;
                }
            }
            _ => unreachable!(),
        }
    }
    match op {
        3 => count,
        4 => 0,
        _ => selected,
    }
}
/// # Safety
/// Same original stable queue/storage metadata admission as select(); output
/// receives the oldest active snapshot excluding one exact completed slot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_oldest(
    slots: *const Slot,
    excluding: usize,
    out: *mut Snapshot,
) {
    let _owner = Guard::acquire(1);
    let mut selected = usize::MAX;
    for i in 0..16 {
        if i == excluding {
            continue;
        }
        let p = unsafe { slots.add(i) };
        if unsafe { (*p).state } != 2 {
            continue;
        }
        if selected == usize::MAX || unsafe { (*p).id < (*slots.add(selected)).id } {
            selected = i;
        }
    }
    let result = if selected == usize::MAX {
        Snapshot::default()
    } else {
        let p = unsafe { slots.add(selected) };
        unsafe {
            Snapshot {
                id: (*p).id,
                kind: (*p).kind,
                lba: (*p).lba,
                sectors: (*p).sectors,
            }
        }
    };
    unsafe { ptr::write(out, result) };
}
const _: () = {
    assert!(core::mem::size_of::<Slot>() == 152);
    assert!(core::mem::offset_of!(Slot, backend_handle) == 104);
    assert!(core::mem::offset_of!(Slot, completion_latch) == 112);
    assert!(core::mem::size_of::<Snapshot>() == 32);
    assert!(core::mem::size_of::<ResultRecord>() == 3);
};
