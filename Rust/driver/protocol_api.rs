//! Original PPI1 table and current execution slot. The loader keeps its
//! existing serialized module-call discipline; no new dispatch model is added.
use crate::{contract, driver::protocol_registry, memory::heap};
use core::{
    cell::UnsafeCell,
    ffi::c_void,
    mem,
    sync::atomic::{AtomicUsize, Ordering},
};
const NO_SLOT: usize = usize::MAX;
static CURRENT_SLOT: AtomicUsize = AtomicUsize::new(NO_SLOT);
struct Table(UnsafeCell<contract::ProtocolApi>);
// The table is installed before R4P init and retained through module calls.
unsafe impl Sync for Table {}
static TABLE: Table = Table(UnsafeCell::new(contract::ProtocolApi {
    magic: contract::protocol_magic,
    version: contract::protocol_api_version,
    size: mem::size_of::<contract::ProtocolApi>() as u32,
    reserved: 0,
    log_info,
    log_warn,
    log_error,
    register_role,
    set_status,
    dependency_status,
    alloc,
    free,
    file_read,
}));

unsafe fn length(text: *const u8) -> usize {
    let mut len = 0;
    while len < 512 && unsafe { text.add(len).read() != 0 } {
        len += 1;
    }
    len
}
fn slot_id() -> u32 {
    let slot = CURRENT_SLOT.load(Ordering::Relaxed);
    if slot == NO_SLOT {
        u32::MAX
    } else {
        u32::try_from(slot).expect("Protocol slot exceeds u32")
    }
}
unsafe extern "C" fn log_info(text: *const u8) {
    unsafe { crate::boot::log_event::protocol(0, slot_id(), text) };
}
unsafe extern "C" fn log_warn(text: *const u8) {
    unsafe { crate::boot::log_event::protocol(1, slot_id(), text) };
}
unsafe extern "C" fn log_error(text: *const u8) {
    unsafe { crate::boot::log_event::protocol(2, slot_id(), text) };
}
unsafe extern "C" fn register_role(role: *const u8, _category: u32, _flags: u32) -> i32 {
    let slot = CURRENT_SLOT.load(Ordering::Relaxed);
    if slot == NO_SLOT {
        return -6;
    }
    unsafe {
        let len = length(role);
        if protocol_registry::r4os_rust_protocol_registry_query(3, slot, role, len) == 0 {
            let note = b"registered role does not match R4P header";
            protocol_registry::r4os_rust_protocol_registry_mutate(
                slot,
                1,
                0,
                -2,
                note.as_ptr(),
                note.len(),
            );
            return -2;
        }
    }
    0
}
unsafe extern "C" fn set_status(state: u32, note: *const u8) -> i32 {
    let slot = CURRENT_SLOT.load(Ordering::Relaxed);
    if slot == NO_SLOT {
        return -6;
    }
    unsafe {
        protocol_registry::r4os_rust_protocol_registry_mutate(slot, 4, state, 0, note, length(note))
    };
    0
}
unsafe extern "C" fn dependency_status(role: *const u8) -> i32 {
    unsafe { protocol_registry::r4os_rust_protocol_registry_query(2, 0, role, length(role)) }
}
unsafe extern "C" fn alloc(bytes: u32, alignment: u32) -> *mut c_void {
    heap::r4os_rust_heap_alloc(bytes as usize, alignment as usize).cast()
}
unsafe extern "C" fn free(pointer: *mut c_void, bytes: u32) {
    if pointer.is_null() || bytes == 0 {
        return;
    }
    unsafe { heap::r4os_rust_heap_free(pointer.cast(), bytes as usize) };
}
unsafe extern "C" fn file_read(path: *const u8, out: *mut u8, len: u32) -> i32 {
    unsafe { protocol_file_read(path, out, len) }
}
/// # Safety
/// R4P receives the canonical Contract table for its existing guarded module
/// lifetime. Trusted consumers preserve the established table contents.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_protocol_table() -> *mut contract::ProtocolApi {
    TABLE.0.get()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_protocol_slot(operation: u8, slot: usize) {
    match operation {
        0 => CURRENT_SLOT.store(slot, Ordering::Relaxed),
        1 => CURRENT_SLOT.store(NO_SLOT, Ordering::Relaxed),
        _ => panic!("Invalid protocol execution scope"),
    }
}
const _: () = {
    assert!(mem::size_of::<contract::ProtocolApi>() == 88);
    assert!(mem::offset_of!(contract::ProtocolApi, file_read) == 80);
};

unsafe fn protocol_file_read(path: *const u8, out: *mut u8, len: u32) -> i32 {
    unsafe { crate::program::system::io::r4os_rust_sys_file_read(path.cast(), out.cast(), len) }
}
