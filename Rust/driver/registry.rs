//! Canonical existing 32-entry driver status registry and early boot flag.
//! Names and states are copied under runtime admission; no shared Rust borrow
//! or pointer into a reusable registry slot escapes to an inventory reader.
use crate::arch::runtime;
use core::{cell::UnsafeCell, ptr, slice};
pub const MAX_DRIVERS: usize = 32;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Entry {
    pub used: bool,
    pub name: [u8; 32],
    pub name_len: usize,
    pub driver_type: u16,
    pub version: u32,
    pub source: u8,
    pub state: u8,
}
impl Entry {
    const EMPTY: Self = Self {
        used: false,
        name: [0; 32],
        name_len: 0,
        driver_type: 0,
        version: 0,
        source: 0,
        state: 0,
    };
}
struct Registry(UnsafeCell<[Entry; MAX_DRIVERS]>);
// The early BSP is the sole caller before runtime enable; afterwards every
// mutation and copy below holds the same native runtime owner.
unsafe impl Sync for Registry {}
static ENTRIES: Registry = Registry(UnsafeCell::new([Entry::EMPTY; MAX_DRIVERS]));
struct BootFlag(UnsafeCell<bool>);
unsafe impl Sync for BootFlag {}
static BOOT_INITIALIZED: BootFlag = BootFlag(UnsafeCell::new(false));
const _: () = {
    assert!(core::mem::size_of::<Entry>() == 64);
    assert!(core::mem::offset_of!(Entry, name_len) == 40);
    assert!(core::mem::offset_of!(Entry, version) == 52);
    assert!(core::mem::offset_of!(Entry, state) == 57);
};
fn row(slot: usize) -> *mut Entry {
    ENTRIES.0.get().cast::<Entry>().wrapping_add(slot)
}
unsafe fn reset() {
    for slot in 0..MAX_DRIVERS {
        unsafe { row(slot).write(Entry::EMPTY) };
    }
}
unsafe fn bytes<'a>(pointer: *const u8, len: usize) -> &'a [u8] {
    if len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(pointer, len) }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_registry_reset() {
    let flags = runtime::save_runtime();
    unsafe { reset() };
    runtime::restore_runtime(flags);
}

/// # Safety
/// Name is readable for `len` bytes through this bounded synchronous call.
/// Loader/DriverApi retain their exact instance and external resource lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_registry_begin(
    name: *const u8,
    len: usize,
    kind: u16,
    version: u32,
    source: u8,
) -> usize {
    assert!(source < 3, "Invalid driver source");
    let flags = runtime::save_runtime();
    let mut result = usize::MAX;
    for slot in 0..MAX_DRIVERS {
        let current = unsafe { row(slot).read() };
        if !current.used || current.state == 5 || current.state == 6 {
            let mut entry = Entry {
                used: true,
                driver_type: kind,
                version,
                source,
                state: 1,
                ..Entry::EMPTY
            };
            entry.name_len = if len < 32 { len } else { 31 };
            if entry.name_len != 0 {
                unsafe { ptr::copy_nonoverlapping(name, entry.name.as_mut_ptr(), entry.name_len) };
            }
            unsafe { row(slot).write(entry) };
            result = slot;
            break;
        }
    }
    runtime::restore_runtime(flags);
    result
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_registry_set_state(slot: usize, state: u8) {
    assert!(state < 8, "Invalid driver state");
    if slot >= MAX_DRIVERS {
        return;
    }
    let flags = runtime::save_runtime();
    unsafe {
        if (*row(slot)).used {
            (*row(slot)).state = state;
        }
    }
    runtime::restore_runtime(flags);
}

/// # Safety
/// Exclusive aligned caller output, disjoint from the canonical registry.
/// Copied fixed bytes stay caller-owned after runtime admission is released.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_registry_get(slot: usize, output: *mut Entry) -> bool {
    if slot >= MAX_DRIVERS {
        return false;
    }
    let flags = runtime::save_runtime();
    let entry = unsafe { row(slot).read() };
    if entry.used {
        unsafe { output.write(entry) };
    }
    runtime::restore_runtime(flags);
    entry.used
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_registry_count() -> usize {
    let flags = runtime::save_runtime();
    let mut count = 0;
    for slot in 0..MAX_DRIVERS {
        if unsafe { (*row(slot)).used } {
            count += 1;
        }
    }
    runtime::restore_runtime(flags);
    count
}

/// # Safety
/// Input bytes remain readable for this synchronous case-insensitive lookup.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_registry_find(name: *const u8, len: usize) -> usize {
    let flags = runtime::save_runtime();
    let name = unsafe { bytes(name, len) };
    let mut result = usize::MAX;
    for slot in 0..MAX_DRIVERS {
        let entry = unsafe { row(slot).read() };
        if entry.used
            && entry.name_len == name.len()
            && entry.name[..entry.name_len]
                .iter()
                .zip(name)
                .all(|(a, b)| a.eq_ignore_ascii_case(b))
        {
            result = slot;
            break;
        }
    }
    runtime::restore_runtime(flags);
    result
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_boot(operation: u8) -> bool {
    let flags = runtime::save_runtime();
    let initialized = unsafe {
        match operation {
            0 => {
                if !BOOT_INITIALIZED.0.get().read() {
                    reset();
                    BOOT_INITIALIZED.0.get().write(true);
                }
                true
            }
            1 => BOOT_INITIALIZED.0.get().read(),
            _ => panic!("Invalid driver boot operation"),
        }
    };
    runtime::restore_runtime(flags);
    initialized
}
