//! Exact request slot transitions. Device queue admission stays with its
//! original caller; short storage metadata admission closes IRQ races.
//! No token, reference or metadata guard spans a backend call, wait or free.
use crate::{
    memory::owner::Guard,
    storage::{block_requests::Slot, dispatch},
};
use core::ptr;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Execution {
    pub slot_index: usize,
    pub id: u64,
    pub kind: u8,
    pub lba: u64,
    pub sectors: u16,
    pub buffer: *mut u8,
    pub const_buffer: *const u8,
    pub buffer_len: usize,
    pub buffer_ownership: u8,
    pub start_tick: u64,
    pub mode: u8,
    pub backend_handle: u64,
}
impl Execution {
    unsafe fn from_slot(index: usize, p: *const Slot) -> Self {
        unsafe {
            Self {
                slot_index: index,
                id: (*p).id,
                kind: (*p).kind,
                lba: (*p).lba,
                sectors: (*p).sectors,
                buffer: (*p).buffer,
                const_buffer: (*p).const_buffer,
                buffer_len: (*p).buffer_len,
                buffer_ownership: (*p).buffer_ownership,
                start_tick: (*p).start_tick,
                mode: (*p).execution_mode,
                backend_handle: (*p).backend_handle,
            }
        }
    }
}
#[repr(C)]
#[derive(Default)]
pub struct Finish {
    pub accepted: bool,
    pub caller_detached: bool,
    pub completion_override: u8,
    pub result_ok: bool,
    pub result_error: u8,
    pub detached_buffer: *mut u8,
    pub detached_len: usize,
}
#[repr(C)]
#[derive(Default)]
pub struct Abort {
    pub action: u8,
    pub kind: u8,
    pub first_timeout: bool,
    pub first_cancel: bool,
    pub first_detach: bool,
    pub buffer_detached: bool,
}
/// # Safety
/// Caller owns the exact free slot under device queue admission. Input is an
/// initialized disjoint C request record; accepted id/tick are caller values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_slot_enqueue(
    slot: *mut Slot,
    input: *const Execution,
    id: u64,
    tick: u64,
) {
    let a = unsafe { input.read() };
    let _owner = Guard::acquire(1);
    unsafe {
        ptr::write_bytes(slot, 0, 1);
        (*slot).state = 1;
        (*slot).id = id;
        (*slot).kind = a.kind;
        (*slot).lba = a.lba;
        (*slot).sectors = a.sectors;
        (*slot).buffer = a.buffer;
        (*slot).const_buffer = a.const_buffer;
        (*slot).buffer_len = a.buffer_len;
        (*slot).buffer_ownership = a.buffer_ownership;
        (*slot).submit_tick = tick
    };
}
/// # Safety
/// Selected exact queued slot is exclusive under device queue admission;
/// mode/handle are from the original execution-owner/handle generator. Output
/// is disjoint aligned C storage. False preserves queued retry semantics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_slot_start(
    slot: *mut Slot,
    index: usize,
    mode: u8,
    tick: u64,
    asynchronous: bool,
    handle: u64,
    out: *mut Execution,
) -> bool {
    let _owner = Guard::acquire(1);
    unsafe {
        (*slot).state = 2;
        (*slot).start_tick = tick;
        (*slot).execution_mode = mode
    };
    if asynchronous {
        unsafe { (*slot).backend_handle = handle };
        if !unsafe {
            dispatch::r4os_rust_storage_latch(
                ptr::addr_of_mut!((*slot).completion_latch),
                0,
                handle,
                0,
                0,
                ptr::null_mut(),
            )
        } {
            unsafe {
                (*slot).state = 1;
                (*slot).backend_handle = 0
            };
            return false;
        }
    }
    unsafe { out.write(Execution::from_slot(index, slot)) };
    true
}
/// # Safety
/// Caller owns device queue admission; original execution retains exact
/// id/handle and its resident buffers until true completion/reset. Outputs
/// contain only caller-owned snapshots; metadata mutation never frees data.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_slot_finish(
    slot: *mut Slot,
    original: *const Execution,
    ok: bool,
    error: u8,
    tick: u64,
    out: *mut Finish,
) -> bool {
    let a = unsafe { original.read() };
    let _owner = Guard::acquire(1);
    if unsafe {
        (*slot).id != a.id
            || (*slot).state != 2
            || (a.backend_handle != 0 && (*slot).backend_handle != a.backend_handle)
    } {
        return false;
    }
    unsafe {
        dispatch::r4os_rust_storage_latch(
            ptr::addr_of_mut!((*slot).completion_latch),
            4,
            0,
            0,
            0,
            ptr::null_mut(),
        )
    };
    let completion_override = unsafe { (*slot).completion_override };
    let caller_detached = unsafe { (*slot).caller_detached };
    let detached_buffer = if caller_detached && unsafe { (*slot).buffer_ownership } == 1 {
        match a.kind {
            1 => a.buffer,
            2 => a.const_buffer.cast_mut(),
            _ => ptr::null_mut(),
        }
    } else {
        ptr::null_mut()
    };
    let result_ok = completion_override == 0 && ok;
    let result_error = if completion_override != 0 {
        completion_override
    } else {
        error
    };
    unsafe {
        (*slot).state = 3;
        (*slot).ok = result_ok;
        (*slot).err = result_error;
        (*slot).complete_tick = tick;
        out.write(Finish {
            accepted: true,
            caller_detached,
            completion_override,
            result_ok,
            result_error,
            detached_buffer,
            detached_len: if detached_buffer.is_null() {
                0
            } else {
                a.buffer_len
            },
        })
    };
    true
}
/// # Safety
/// Exact live slot under device queue admission. Original cancellation/timeout
/// policy permits caller detach only for none/bounce ownership; borrowed data
/// remains until physical completion. No cancel callback or wake occurs here.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_slot_abort(
    slot: *mut Slot,
    error: u8,
    out: *mut Abort,
    execution: *mut Execution,
    index: usize,
) {
    let _owner = Guard::acquire(1);
    let mut a = Abort {
        kind: unsafe { (*slot).kind },
        ..Abort::default()
    };
    match unsafe { (*slot).state } {
        0 => a.action = 0,
        1 => {
            a.action = 1;
        }
        3 => a.action = 2,
        2 => {
            if unsafe { (*slot).completion_override } == 0 {
                unsafe { (*slot).completion_override = error };
            }
            a.first_timeout = error == 7 && !unsafe { (*slot).timeout_requested };
            if a.first_timeout {
                unsafe { (*slot).timeout_requested = true };
            }
            a.first_cancel = !unsafe { (*slot).cancel_requested };
            if a.first_cancel {
                unsafe { (*slot).cancel_requested = true };
            }
            unsafe { execution.write(Execution::from_slot(index, slot)) };
            let action =
                dispatch::r4os_rust_storage_timeout_action(unsafe { (*slot).buffer_ownership });
            if action == 2 {
                a.action = 3;
            } else {
                a.action = 4;
                a.first_detach = !unsafe { (*slot).caller_detached };
                unsafe { (*slot).caller_detached = true };
                a.buffer_detached = action == 1;
            }
        }
        _ => unreachable!(),
    }
    unsafe { out.write(a) };
}
/// # Safety
/// Exact completed/queued-cancelled slot under queue admission; caller has
/// captured accounting and any detached buffer before this resident clear.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_slot_clear(slot: *mut Slot) {
    let _owner = Guard::acquire(1);
    unsafe { ptr::write_bytes(slot, 0, 1) }
}
/// # Safety
/// Readable initialized slot is under exact queue admission; C execution
/// output is exclusive/disjoint and kept as a copied handle, not a borrow.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_execution(
    index: usize,
    slot: *const Slot,
    out: *mut Execution,
) {
    let _owner = Guard::acquire(1);
    unsafe { out.write(Execution::from_slot(index, slot)) }
}
const _: () = {
    assert!(core::mem::size_of::<Execution>() == 96);
    assert!(core::mem::offset_of!(Execution, backend_handle) == 88);
    assert!(core::mem::size_of::<Finish>() == 24);
    assert!(core::mem::size_of::<Abort>() == 6);
};

/// # Safety
/// Stable initialized slot under original device queue admission. Reset closes
/// new admission before this scan. Output is one exclusive copied execution;
/// first-cancel is written only for an active slot. No cancel/wake/free here.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_slot_reset(
    slot: *mut Slot,
    index: usize,
    out: *mut Execution,
    first_cancel: *mut bool,
) -> u8 {
    let _owner = Guard::acquire(1);
    match unsafe { (*slot).state } {
        1 => {
            unsafe {
                (*slot).state = 3;
                (*slot).ok = false;
                (*slot).err = 12;
                (*slot).complete_tick = crate::platform::timer::tick_count()
            };
            1
        }
        2 => {
            if unsafe { (*slot).completion_override } == 0 {
                unsafe { (*slot).completion_override = 12 };
            }
            let first = !unsafe { (*slot).cancel_requested };
            unsafe {
                (*slot).cancel_requested = true;
                first_cancel.write(first);
                out.write(Execution::from_slot(index, slot))
            };
            2
        }
        0 | 3 => 0,
        _ => unreachable!(),
    }
}
