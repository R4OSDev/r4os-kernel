//! Original virtual windows, range validation and first-fit placement.
//! The legacy pager still owns its range table until the shared VM cutover.
use super::layout;
#[repr(C)]
pub struct Window {
    pub base: u64,
    pub len: u64,
    pub name: *const u8,
    pub name_len: usize,
}
struct Definition {
    base: u64,
    len: u64,
    name: &'static [u8],
}
const GB: u64 = 1024 * layout::MIB;
const DEFINITIONS: [Definition; 9] = [
    Definition {
        base: 0xffff_ff00_0000_0000,
        len: 16 * layout::MIB,
        name: b"temp-kernel",
    },
    Definition {
        base: 0xffff_ff10_0000_0000,
        len: GB,
        name: b"kernel-heap",
    },
    Definition {
        base: 0xffff_ff20_0000_0000,
        len: 512 * layout::MIB,
        name: b"program-image",
    },
    Definition {
        base: 0xffff_ff30_0000_0000,
        len: 0,
        name: b"app-heap-retired",
    },
    Definition {
        base: layout::APP_STACK_BASE,
        len: layout::APP_STACK_BYTES,
        name: b"app-stack",
    },
    Definition {
        base: 0xffff_ff50_0000_0000,
        len: 512 * layout::MIB,
        name: b"mmio",
    },
    Definition {
        base: 0xffff_fb00_0000_0000,
        len: 4 * 1024 * GB,
        name: b"r4x-vm",
    },
    Definition {
        base: 0xffff_ff60_0000_0000,
        len: 64 * layout::MIB,
        name: b"kernel-stack",
    },
    Definition {
        base: 0xffff_ff70_0000_0000,
        len: 16 * GB,
        name: b"graphics",
    },
];
#[repr(C)]
pub struct AddressRecord {
    pub base: u64,
    pub len: u64,
    pub window: u8,
}
unsafe extern "C" {
    fn r4os_zig_vm_fit_start(base: u64, count: *mut usize) -> usize;
    fn r4os_zig_vm_fit_record(position: usize, output: *mut AddressRecord) -> bool;
}
const EMPTY: u32 = 3;
const ALIGNMENT: u32 = 4;
const OVERFLOW: u32 = 5;
const NO_SPACE: u32 = 6;
const GUARD: u32 = 11;
const OUTSIDE: u32 = 15;
fn align_up(value: u64, alignment: u64) -> Option<u64> {
    Some(value.checked_add(alignment - 1)? & !(alignment - 1))
}
/// # Safety
/// window is a valid enum and output is exclusive live C storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_window(window: u8, output: *mut Window) {
    let definition = &DEFINITIONS[window as usize];
    unsafe {
        output.write(Window {
            base: definition.base,
            len: definition.len,
            name: definition.name.as_ptr(),
            name_len: definition.name.len(),
        });
    }
}
/// # Safety
/// output is exclusive live C storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_normalize(
    value: u64,
    alignment: bool,
    output: *mut u64,
) -> u32 {
    let value = if alignment {
        if value < layout::PAGE_SIZE || !value.is_power_of_two() {
            return ALIGNMENT;
        }
        value
    } else {
        if value == 0 {
            return EMPTY;
        }
        let Some(value) = align_up(value, layout::PAGE_SIZE) else {
            return OVERFLOW;
        };
        value
    };
    unsafe {
        output.write(value);
    }
    0
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_vm_inside(range_len: u64, offset: u64, len: u64) -> u32 {
    match offset.checked_add(len) {
        None => OVERFLOW,
        Some(end) if end > range_len => OUTSIDE,
        Some(_) => 0,
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_vm_overlap(a_base: u64, a_len: u64, b_base: u64, b_len: u64) -> bool {
    let Some(a_end) = a_base.checked_add(a_len) else {
        return true;
    };
    let Some(b_end) = b_base.checked_add(b_len) else {
        return true;
    };
    a_base < b_end && b_base < a_end
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_vm_guard(base: u64, len: u64, guard_base: u64, guard_len: u64) -> u32 {
    if guard_len == 0 || !r4os_rust_vm_overlap(base, len, guard_base, guard_len) {
        0
    } else {
        GUARD
    }
}
/// # Safety
/// The caller retains VM owner 5 and the stable sorted legacy address index.
/// window/alignment are normalized; output is exclusive live C storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_first_fit(
    window: u8,
    len: u64,
    alignment: u64,
    output: *mut u64,
) -> u32 {
    let definition = &DEFINITIONS[window as usize];
    let Some(end) = definition.base.checked_add(definition.len) else {
        return OVERFLOW;
    };
    let Some(mut candidate) = align_up(definition.base, alignment) else {
        return OVERFLOW;
    };
    let mut count = 0;
    let start = unsafe { r4os_zig_vm_fit_start(definition.base, &mut count) };
    for position in start..count {
        let mut record = core::mem::MaybeUninit::<AddressRecord>::uninit();
        if !unsafe { r4os_zig_vm_fit_record(position, record.as_mut_ptr()) } {
            return OVERFLOW;
        }
        let record = unsafe { record.assume_init() };
        if record.base >= end {
            break;
        }
        if record.window != window {
            continue;
        }
        let Some(candidate_end) = candidate.checked_add(len) else {
            return OVERFLOW;
        };
        if candidate_end > end {
            return NO_SPACE;
        }
        if candidate_end <= record.base {
            unsafe {
                output.write(candidate);
            }
            return 0;
        }
        let Some(record_end) = record.base.checked_add(record.len) else {
            return OVERFLOW;
        };
        if record_end > candidate {
            let Some(next) = align_up(record_end, alignment) else {
                return OVERFLOW;
            };
            if next <= candidate {
                return OVERFLOW;
            }
            candidate = next;
        }
    }
    let Some(candidate_end) = candidate.checked_add(len) else {
        return OVERFLOW;
    };
    if candidate_end > end {
        return NO_SPACE;
    }
    unsafe {
        output.write(candidate);
    }
    0
}
const _: () = {
    assert!(core::mem::size_of::<Window>() == 32);
    assert!(core::mem::size_of::<AddressRecord>() == 24);
};
