//! Original bounded channel-slot selection and completion decisions. The
//! caller's admitted channel owns all metadata; this model stores no state.
use core::mem::{offset_of, size_of};

pub const FREE: u8 = 0;
pub const RAW_READY: u8 = 1;
pub const HANDLER_QUEUED: u8 = 2;
pub const HANDLER_RUNNING: u8 = 3;
pub const HANDLER_DONE: u8 = 4;
pub const HANDLER_READY: u8 = 5;
pub const RAW: u8 = 0;
pub const QUEUED_RESPONSE: u8 = 1;
pub const DIRECT_RESPONSE: u8 = 2;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SlotMeta {
    pub state: u8,
    pub mode: u8,
    pub generation: u64,
    pub sequence: u64,
    pub len: u16,
    pub result: i32,
    pub abandoned: bool,
    pub completion_published: bool,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct View {
    pub base: *const u8,
    pub len: usize,
    pub stride: usize,
    pub meta_offset: usize,
}
impl View {
    unsafe fn at(self, index: usize) -> SlotMeta {
        assert!(index < self.len);
        unsafe {
            self.base
                .add(index * self.stride + self.meta_offset)
                .cast::<SlotMeta>()
                .read()
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Selection {
    pub index: usize,
    pub visits: usize,
}
#[repr(C)]
#[derive(Default)]
pub struct Decision {
    pub selection: Selection,
    pub required: usize,
    pub kind: u8,
}
unsafe fn first_free(slots: View) -> Option<Selection> {
    for index in 0..slots.len {
        if unsafe { slots.at(index) }.state == FREE {
            return Some(Selection {
                index,
                visits: index + 1,
            });
        }
    }
    None
}
unsafe fn oldest(slots: View, queued: bool) -> Option<Selection> {
    let mut selected = None;
    let mut sequence = 0;
    for index in 0..slots.len {
        let meta = unsafe { slots.at(index) };
        if if queued {
            meta.state != HANDLER_QUEUED
        } else {
            meta.state != RAW_READY && meta.state != HANDLER_READY
        } {
            continue;
        }
        if selected.is_none() || meta.sequence < sequence {
            selected = Some(index);
            sequence = meta.sequence;
        }
    }
    selected.map(|index| Selection {
        index,
        visits: slots.len,
    })
}
/// # Safety
/// `slots` describes valid aligned SlotMeta fields in resident records admitted
/// by their original owner for this call. `output` is an exclusive C record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_service_queue_query(
    slots: View,
    operation: u8,
    capacity: usize,
    output: *mut Decision,
) {
    let mut result = Decision::default();
    if operation <= 3 {
        let selection = match operation {
            0 => unsafe { first_free(slots) },
            1 | 3 => unsafe { oldest(slots, false) },
            2 => unsafe { oldest(slots, true) },
            _ => unreachable!(),
        };
        if let Some(selection) = selection {
            result.selection = selection;
            result.kind = 1;
            if operation == 3 {
                result.required = usize::from(unsafe { slots.at(selection.index) }.len);
                if capacity < result.required {
                    result.kind = 2;
                }
            }
        }
    } else {
        assert!(operation <= 6);
        for index in 0..slots.len {
            let state = unsafe { slots.at(index) }.state;
            let count = match operation {
                4 => state == RAW_READY || state == HANDLER_READY,
                5 => state != FREE,
                6 => state == HANDLER_RUNNING,
                _ => unreachable!(),
            };
            result.selection.index += usize::from(count);
        }
        result.kind = 1;
    }
    unsafe { output.write(result) };
}
pub fn timeout(meta: SlotMeta, generation: u64) -> u8 {
    if meta.generation != generation {
        return 0;
    }
    match meta.state {
        HANDLER_QUEUED => 1,
        HANDLER_RUNNING => 2,
        HANDLER_DONE => {
            if meta.completion_published {
                1
            } else {
                2
            }
        }
        _ => 0,
    }
}
pub fn completion(meta: SlotMeta, generation: u64, capacity: Option<usize>) -> u8 {
    if meta.generation != generation || meta.state != HANDLER_DONE {
        return 0;
    }
    if meta.result < 0 {
        return 1;
    }
    match meta.mode {
        QUEUED_RESPONSE => 2,
        DIRECT_RESPONSE => {
            if capacity.is_none_or(|n| n < usize::from(meta.len)) {
                4
            } else {
                3
            }
        }
        RAW => 1,
        _ => panic!("Invalid private service queue mode"),
    }
}
/// # Safety
/// Except for operation3, `meta` is a valid caller-owned C snapshot. This pure
/// decision neither mutates the slot nor retains its pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_service_queue_decide(
    operation: u8,
    meta: *const SlotMeta,
    generation: u64,
    value: u64,
    present: bool,
) -> u8 {
    if operation == 3 {
        return u8::from(generation == value);
    }
    let meta = unsafe { meta.read() };
    match operation {
        0 => u8::from(meta.generation == generation && u64::from(meta.state) == value),
        1 => timeout(meta, generation),
        2 => completion(meta, generation, present.then_some(value as usize)),
        _ => panic!("Invalid private service queue decision"),
    }
}
const _: () = {
    assert!(size_of::<SlotMeta>() == 40);
    assert!(offset_of!(SlotMeta, generation) == 8);
    assert!(offset_of!(SlotMeta, sequence) == 16);
    assert!(offset_of!(SlotMeta, len) == 24);
    assert!(offset_of!(SlotMeta, result) == 28);
    assert!(offset_of!(SlotMeta, abandoned) == 32);
    assert!(offset_of!(SlotMeta, completion_published) == 33);
    assert!(size_of::<View>() == 32);
    assert!(size_of::<Selection>() == 16);
    assert!(size_of::<Decision>() == 32);
    assert!(offset_of!(Decision, required) == 16);
    assert!(offset_of!(Decision, kind) == 24);
};
