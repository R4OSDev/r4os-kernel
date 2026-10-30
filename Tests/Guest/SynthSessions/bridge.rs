//! Explicit private-image bridge to actual Core/R4D lifecycle functions.
//! No normal build includes this file or exposes this symbol.
use super::*;
use crate::{contract as a, driver::{api_admission, registry, resource_state, resources}, fs::vfs_types::Bytes, loader::r4d};
use core::ptr;

fn name(which: u64) -> Bytes { Bytes { data: if which == 52 { b"P52D".as_ptr() } else { b"P53D".as_ptr() }, len: 4 } }
fn driver_owner() -> u32 {
    let index = unsafe { registry::r4os_rust_driver_registry_find(b"P53D".as_ptr(), 4) };
    if index == usize::MAX { 0 } else { u32::try_from(index + 1).unwrap() }
}
fn driver_info(owner: u32) -> (u8, a::DriverModuleInfo) {
    let mut info = a::DriverModuleInfo::default();
    let result = unsafe { r4d::r4os_rust_r4d_info(owner, &mut info) };
    (result, info)
}
fn current() -> a::ProgramProcessHandle {
    let mut handle = a::ProgramProcessHandle::default();
    assert!(unsafe { crate::program::context::r4os_rust_program_context_handle(&mut handle) });
    handle
}
fn registration(source: SynthSource, name: &[u8]) -> i64 {
    let Some(_execution) = (unsafe { sessions::Execution::acquire(source) }) else { return i64::from(BUSY) };
    unsafe { synths::find_synth(name).map_or(0, |i| i64::try_from((*synth(i)).generation).unwrap()) }
}
fn epoch(owner: u32) -> i64 {
    if !api_admission::r4os_rust_driver_api_enter(owner, 1000) { return -1; }
    let mut binding = ptr::null();
    let result = unsafe { resource_state::r4os_rust_resource_current(resources::r4os_rust_driver_resource_state(), owner, &mut binding) };
    let value = if result == 0 { unsafe { i64::try_from((*binding).epoch).unwrap() } } else { -2 };
    assert!(api_admission::r4os_rust_driver_api_leave());
    value
}

/// Only a SHA-pinned private native guest may call this scalar C bridge.
#[unsafe(no_mangle)]
#[inline(never)]
pub unsafe extern "C" fn r4os_test_synth_sessions53(op: u32, id: u32, generation: u64, value: u64) -> i64 {
    let owner = Owner { instance_id: id, generation };
    unsafe {
        match op {
            0 => 0x5359_5353,
            1 => i64::from(current().instance_id),
            2 => i64::try_from(current().generation).unwrap(),
            3 => i64::from(driver_owner()),
            4 => i64::try_from(driver_info(id).1.generation).unwrap(),
            5 => i64::from(driver_info(id).0),
            6 => i64::from(crate::driver::heap::r4os_rust_driver_heap_query(id)),
            7 => registration(SynthSource::Sid, b"SID"),
            8 => registration(SynthSource::Midi, b"OPL3"),
            9 => epoch(id),
            10 => i64::from(sid_write_register_for_owner(owner, 1, 4, 0x11)),
            11 => i64::from(sid_release_for_owner(owner, 1)),
            12 => i64::from(close_synths_for_owner(owner)),
            13 => i64::from(midi_send_for_owner(owner, 1, 0, 0x90, 60, 80)),
            14 => i64::from(midi_close_for_owner(owner, 1)),
            15 => i64::from(sid_acquire_for_owner(owner)),
            16 => i64::from(midi_open_synth_for_owner(owner, c"OPL3".as_ptr().cast())),
            20 => i64::from(set_synth_driver_closing(id, value != 0)),
            21 => i64::from(unregister_synth_driver(id)),
            22 => i64::from(r4d::r4os_rust_r4d_unload(name(value))),
            23 => i64::from(r4d::r4os_rust_r4d_succeeded(r4d::r4os_rust_r4d_load(name(value)))),
            _ => -999,
        }
    }
}
