//! Original controller lanes, submission barriers and exact completion latch.
//! Metadata is embedded in the block caller and keeps its original owner.
use core::{ptr, slice};
#[repr(C)]
#[derive(Default)]
pub struct Completion {
    pub result: i32,
    pub bytes: u32,
}
#[repr(C)]
#[derive(Default)]
pub struct Latch {
    pub handle: u64,
    pub claimed: bool,
    pub ready: bool,
    pub result: i32,
    pub bytes: u32,
}
#[repr(C)]
pub struct Controllers {
    pub used: [bool; 8],
    pub names: [[u8; 32]; 8],
    pub lengths: [u8; 8],
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_storage_timeout_action(ownership: u8) -> u8 {
    match ownership {
        0 => 0,
        1 => 1,
        2 => 2,
        _ => unreachable!(),
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_storage_submission(
    active: u32,
    max: u16,
    flush: bool,
    flush_in_flight: bool,
) -> bool {
    if max == 0 || active >= u32::from(max) {
        return false;
    }
    if flush_in_flight {
        return false;
    }
    if flush && active != 0 {
        return false;
    }
    true
}
/// # Safety
/// Initialized aligned resident latch is exclusive under the block runtime
/// owner. Successful take writes the exact aligned C completion output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_storage_latch(
    state: *mut Latch,
    op: u8,
    handle: u64,
    result: i32,
    bytes: u32,
    out: *mut Completion,
) -> bool {
    let s = unsafe { &mut *state };
    match op {
        0 => {
            if handle == 0 {
                return false;
            }
            *s = Latch {
                handle,
                ..Latch::default()
            };
            true
        }
        1 => {
            if handle == 0 || s.handle != handle || s.claimed {
                return false;
            }
            s.claimed = true;
            s.ready = true;
            s.result = result;
            s.bytes = bytes;
            true
        }
        2 => {
            if handle == 0 || s.handle != handle || s.claimed {
                return false;
            }
            s.claimed = true;
            true
        }
        3 => {
            if !s.ready {
                return false;
            }
            s.ready = false;
            unsafe {
                ptr::write(
                    out,
                    Completion {
                        result: s.result,
                        bytes: s.bytes,
                    },
                )
            };
            true
        }
        4 => {
            *s = Latch::default();
            true
        }
        _ => unreachable!(),
    }
}
/// # Safety
/// Resident controller map is exclusive under its original caller owner;
/// input has len readable bytes. Assignment never lends the input name.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_storage_controller_assign(
    state: *mut Controllers,
    input: *const u8,
    len: usize,
) -> u8 {
    let name = if len == 0 {
        b"unknown".as_slice()
    } else {
        if len > 32 {
            return 255;
        }
        unsafe { slice::from_raw_parts(input, len) }
    };
    let s = unsafe { &mut *state };
    let mut free = None;
    for i in 0..8 {
        if !s.used[i] {
            free = free.or(Some(i));
            continue;
        }
        let len = usize::from(s.lengths[i]);
        if s.names[i][..len].eq_ignore_ascii_case(name) {
            return i as u8;
        }
    }
    let Some(index) = free else {
        return 255;
    };
    s.used[index] = true;
    s.lengths[index] = name.len() as u8;
    s.names[index][..name.len()].copy_from_slice(name);
    index as u8
}
/// # Safety
/// Initialized aligned controller metadata stays under its exact caller owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_storage_controller(
    state: *mut Controllers,
    clear: bool,
    lane: u8,
) -> u32 {
    if clear {
        let s = unsafe { &mut *state };
        let index = usize::from(lane);
        if index < 8 {
            s.used[index] = false;
            s.lengths[index] = 0;
            s.names[index] = [0; 32];
        }
        return 0;
    }
    let s = unsafe { &*state };
    let mut total = 0;
    for used in &s.used {
        if *used {
            total += 1;
        }
    }
    total
}
const _: () = {
    assert!(core::mem::size_of::<Latch>() == 24);
    assert!(core::mem::offset_of!(Latch, result) == 12);
    assert!(core::mem::size_of::<Completion>() == 8);
    assert!(core::mem::size_of::<Controllers>() == 272);
};
