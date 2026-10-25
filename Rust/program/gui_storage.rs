//! Fallible private GUI command/resource reservations and yield-free commit.
//! The acquisition sequence is the same sequence validated and unwound in
//! reverse by gui_lifetime. Cancellation releases only unpublished candidates.
use super::{gui_lifetime, payload, payload_types as t, registry_types::ProgramInstance, threads};
use crate::{contract, sched::sync_locks};
use core::{
    ffi::c_void,
    mem, ptr,
    sync::atomic::{AtomicU64, Ordering},
};
static GENERATION: AtomicU64 = AtomicU64::new(0);
const INITIAL: u32 = 8;
const TARGET: u32 = 128;
const ALPHA_WORDS: usize = (contract::gui_alpha8_max_pixels as usize).div_ceil(4);
const MAX_WORDS: usize = if ALPHA_WORDS > 16384 {
    ALPHA_WORDS
} else {
    16384
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CommandStorage {
    pub gui: *mut t::Gui,
    pub frame: *mut t::Frame,
    pub payload: *mut t::Commands,
    pub new_payload: bool,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ResourceStorage {
    pub gui: *mut t::Gui,
    pub frame: *mut t::Frame,
    pub payload: *mut t::Resource,
    pub next_resource_len: u64,
    pub next_raster_words: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BlitStorage {
    pub command: CommandStorage,
    pub resource: ResourceStorage,
}
pub(super) fn reserve_generation() -> Option<u64> {
    let mut current = GENERATION.load(Ordering::Acquire);
    while current != u64::MAX {
        let next = current + 1;
        match GENERATION.compare_exchange_weak(current, next, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => return Some(next),
            Err(actual) => current = actual,
        }
    }
    None
}
pub(super) unsafe fn allocate_frame(owner: u32, explicit: bool) -> *mut t::Frame {
    unsafe {
        let frame = payload::r4os_rust_program_payload_allocate(9, owner, 9).cast::<t::Frame>();
        if frame.is_null() {
            return frame;
        }
        (*frame).explicit_build = explicit;
        frame
    }
}
unsafe fn capacity(frame: *const t::Frame) -> u32 {
    unsafe {
        let tail = (*frame).command_tail;
        if tail.is_null() {
            return INITIAL;
        }
        if (*tail).capacity >= TARGET {
            return TARGET;
        }
        ((*tail).capacity * 2).min(TARGET)
    }
}
pub(super) unsafe fn needs_payload(frame: *const t::Frame) -> bool {
    unsafe {
        let tail = (*frame).command_tail;
        tail.is_null() || (*tail).command_count == (*tail).capacity
    }
}
pub(super) unsafe fn can_link(frame: *const t::Frame, count: u64) -> bool {
    unsafe { count <= u64::MAX - (*frame).node_sequence }
}
pub(super) unsafe fn sequence(frame: *mut t::Frame) -> u64 {
    unsafe {
        assert!(can_link(frame, 1));
        (*frame).node_sequence += 1;
        (*frame).node_sequence
    }
}
pub(super) unsafe fn prepare_command(
    instance: *mut ProgramInstance,
    frame: *mut t::Frame,
) -> Option<CommandStorage> {
    unsafe {
        let gui = (*instance).gui_payload.cast::<t::Gui>();
        if gui.is_null() || (*frame).build_failed || (*frame).command_count == u64::MAX {
            return None;
        }
        let tail = (*frame).command_tail;
        if !tail.is_null() && (*tail).command_count < (*tail).capacity {
            return Some(CommandStorage {
                gui,
                frame,
                payload: tail,
                new_payload: false,
            });
        }
        if !can_link(frame, 1) {
            return None;
        }
        let payload =
            payload::allocate_commands((*instance).id, (*frame).command_count, capacity(frame));
        if payload.is_null() {
            None
        } else {
            Some(CommandStorage {
                gui,
                frame,
                payload,
                new_payload: true,
            })
        }
    }
}
pub(super) unsafe fn cancel_command(instance: *mut ProgramInstance, storage: *mut CommandStorage) {
    unsafe {
        if (*storage).new_payload {
            payload::release_commands((*storage).payload, (*instance).id);
        }
    }
}
pub(super) unsafe fn commit_command(storage: *mut CommandStorage, command: t::GuiCommand) -> i32 {
    unsafe {
        let frame = (*storage).frame;
        let node = (*storage).payload;
        if (*storage).new_payload {
            (*node).allocation_sequence = sequence(frame);
            (*node).previous = (*frame).command_tail;
            let tail = (*frame).command_tail;
            if !tail.is_null() {
                (*tail).next = node;
            } else {
                (*frame).command_payload = node;
            }
            (*frame).command_tail = node;
        }
        assert!((*node).command_count < (*node).capacity);
        node.cast::<u8>()
            .add(mem::size_of::<t::Commands>())
            .cast::<t::GuiCommand>()
            .add((*node).command_count as usize)
            .write(command);
        (*node).command_count += 1;
        (*frame).command_count += 1;
        payload::r4os_rust_program_payload_account(4, 0, 1);
        gui_lifetime::append(
            (*storage).gui,
            frame,
            if (*storage).new_payload {
                u64::from((*node).header.requested_bytes)
            } else {
                0
            },
            1,
        );
        (*frame).command_count.min(i32::MAX as u64) as i32
    }
}
pub(super) unsafe fn prepare_resource(
    instance: *mut ProgramInstance,
    frame: *mut t::Frame,
    count: usize,
    kind: u16,
) -> Option<ResourceStorage> {
    unsafe {
        let gui = (*instance).gui_payload.cast::<t::Gui>();
        if gui.is_null() || (*frame).build_failed || count == 0 || !can_link(frame, 1) {
            return None;
        }
        let next_resource_len = (*frame).resource_len.checked_add(count as u64)?;
        let mut next_raster_words = (*frame).raster_words;
        if matches!(kind, 2 | 3) {
            if !count.is_multiple_of(4) {
                return None;
            }
            next_raster_words = (*frame).raster_words.checked_add((count / 4) as u64)?;
        }
        let payload = payload::allocate_resource(
            (*instance).id,
            (*frame).resource_len,
            if matches!(kind, 2 | 3) {
                (*frame).raster_words
            } else {
                0
            },
            count,
            kind,
        );
        if payload.is_null() {
            None
        } else {
            Some(ResourceStorage {
                gui,
                frame,
                payload,
                next_resource_len,
                next_raster_words,
            })
        }
    }
}
pub(super) unsafe fn cancel_resource(
    instance: *mut ProgramInstance,
    storage: *mut ResourceStorage,
) {
    unsafe { payload::release_resource((*storage).payload, (*instance).id) };
}
pub(super) unsafe fn commit_resource(storage: *mut ResourceStorage) {
    unsafe {
        let frame = (*storage).frame;
        let node = (*storage).payload;
        (*node).allocation_sequence = sequence(frame);
        (*node).previous = (*frame).resource_tail;
        let tail = (*frame).resource_tail;
        if !tail.is_null() {
            (*tail).next = node;
        } else {
            (*frame).resource_payload = node;
        }
        (*frame).resource_tail = node;
        (*frame).resource_len = (*storage).next_resource_len;
        (*frame).raster_words = (*storage).next_raster_words;
        gui_lifetime::append(
            (*storage).gui,
            frame,
            u64::from((*node).header.requested_bytes),
            0,
        );
    }
}
fn rollback_stat() {
    payload::add(mem::offset_of!(t::Stats, transaction_rollbacks), 8, 1);
}
pub(super) unsafe fn prepare_blit(
    instance: *mut ProgramInstance,
    frame: *mut t::Frame,
    words: usize,
    kind: u16,
) -> Option<BlitStorage> {
    unsafe {
        if words == 0 || words > MAX_WORDS {
            return None;
        }
        let nodes = 1 + u64::from(needs_payload(frame));
        if !can_link(frame, nodes) {
            return None;
        }
        let mut command = prepare_command(instance, frame)?;
        let Some(bytes) = words.checked_mul(4) else {
            cancel_command(instance, &mut command);
            return None;
        };
        let Some(resource) = prepare_resource(instance, frame, bytes, kind) else {
            cancel_command(instance, &mut command);
            rollback_stat();
            return None;
        };
        Some(BlitStorage { command, resource })
    }
}
pub(super) unsafe fn cancel_blit(instance: *mut ProgramInstance, storage: *mut BlitStorage) {
    unsafe {
        cancel_resource(instance, ptr::addr_of_mut!((*storage).resource));
        cancel_command(instance, ptr::addr_of_mut!((*storage).command));
        rollback_stat();
    }
}
pub(super) unsafe fn commit_blit(storage: *mut BlitStorage, command: t::GuiCommand) -> i32 {
    unsafe {
        let result = commit_command(ptr::addr_of_mut!((*storage).command), command);
        commit_resource(ptr::addr_of_mut!((*storage).resource));
        result
    }
}
pub(super) unsafe fn lock(gui: *mut t::Gui) {
    unsafe {
        while !sync_locks::r4os_rust_sync_guard(
            ptr::addr_of_mut!((*gui).operation_gate),
            2,
            u64::MAX,
        ) {
            threads::yield_task();
        }
        while !sync_locks::r4os_rust_sync_mutex(ptr::addr_of_mut!((*gui).frame_lock), 0, 0) {
            core::hint::spin_loop();
        }
    }
}
pub(super) unsafe fn unlock(gui: *mut t::Gui) {
    unsafe {
        sync_locks::r4os_rust_sync_mutex(ptr::addr_of_mut!((*gui).frame_lock), 2, 0);
        sync_locks::r4os_rust_sync_guard(ptr::addr_of_mut!((*gui).operation_gate), 3, 0);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_gui_generation() -> u64 {
    reserve_generation().unwrap_or(0)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_gui_allocate_frame(
    owner: u32,
    explicit: bool,
) -> *mut t::Frame {
    unsafe { allocate_frame(owner, explicit) }
}
/// # Safety
/// Exact retained frame under original producer admission; op3 mutates it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_reservation_info(
    op: u8,
    frame: *mut t::Frame,
    count: u64,
) -> u64 {
    unsafe {
        match op {
            0 => u64::from(capacity(frame)),
            1 => u64::from(needs_payload(frame)),
            2 => u64::from(can_link(frame, count)),
            3 => sequence(frame),
            _ => unreachable!(),
        }
    }
}
/// # Safety
/// Retained producer instance/frame and correctly aligned exclusive storage for
/// the selected32/40/72-byte reservation. Failure leaves output untouched.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_prepare(
    op: u8,
    instance: *mut ProgramInstance,
    frame: *mut t::Frame,
    count: usize,
    kind: u16,
    out: *mut c_void,
) -> bool {
    unsafe {
        match op {
            0 => {
                let Some(value) = prepare_command(instance, frame) else {
                    return false;
                };
                out.cast::<CommandStorage>().write(value);
            }
            1 => {
                let Some(value) = prepare_resource(instance, frame, count, kind) else {
                    return false;
                };
                out.cast::<ResourceStorage>().write(value);
            }
            2 => {
                let Some(value) = prepare_blit(instance, frame, count, kind) else {
                    return false;
                };
                out.cast::<BlitStorage>().write(value);
            }
            _ => unreachable!(),
        }
        true
    }
}
/// # Safety
/// Exact unpublished reservation and its original owner, cancelled once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_cancel(
    op: u8,
    instance: *mut ProgramInstance,
    storage: *mut c_void,
) {
    unsafe {
        match op {
            0 => cancel_command(instance, storage.cast()),
            1 => cancel_resource(instance, storage.cast()),
            2 => cancel_blit(instance, storage.cast()),
            _ => unreachable!(),
        }
    }
}
/// # Safety
/// Exact producer reservation and readable complete command for op0/2. All
/// candidate allocation/copying has completed before this yield-free commit.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_commit(
    op: u8,
    storage: *mut c_void,
    command: *const t::GuiCommand,
) -> i32 {
    unsafe {
        match op {
            0 => commit_command(storage.cast(), command.read()),
            1 => {
                commit_resource(storage.cast());
                0
            }
            2 => commit_blit(storage.cast(), command.read()),
            _ => unreachable!(),
        }
    }
}
/// # Safety
/// Retained GUI owner, paired lock/unlock on its original exact task lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_lock(gui: *mut t::Gui, release: bool) {
    unsafe {
        if release {
            unlock(gui);
        } else {
            lock(gui);
        }
    }
}
const _: () = {
    assert!(mem::size_of::<CommandStorage>() == 32);
    assert!(mem::align_of::<CommandStorage>() == 8);
    assert!(mem::offset_of!(CommandStorage, gui) == 0);
    assert!(mem::offset_of!(CommandStorage, frame) == 8);
    assert!(mem::offset_of!(CommandStorage, payload) == 16);
    assert!(mem::offset_of!(CommandStorage, new_payload) == 24);
    assert!(mem::size_of::<ResourceStorage>() == 40);
    assert!(mem::align_of::<ResourceStorage>() == 8);
    assert!(mem::offset_of!(ResourceStorage, gui) == 0);
    assert!(mem::offset_of!(ResourceStorage, frame) == 8);
    assert!(mem::offset_of!(ResourceStorage, payload) == 16);
    assert!(mem::offset_of!(ResourceStorage, next_resource_len) == 24);
    assert!(mem::offset_of!(ResourceStorage, next_raster_words) == 32);
    assert!(mem::size_of::<BlitStorage>() == 72);
    assert!(mem::align_of::<BlitStorage>() == 8);
    assert!(mem::offset_of!(BlitStorage, command) == 0);
    assert!(mem::offset_of!(BlitStorage, resource) == 32);
};
