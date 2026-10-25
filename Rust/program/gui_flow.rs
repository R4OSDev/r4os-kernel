//! Immutable GUI reader captures, cooperative flattening and frame publication.
//! Exact unwind tokens retain task ownership across copying/yields; frame locks
//! cover only resident metadata and never span allocation or resource release.
use super::{
    console, context, gui_lifetime as life, gui_storage as storage, payload, payload_types as t,
    process_data, registry, registry_types::ProgramInstance, threads,
};
use crate::{
    contract,
    sched::{scheduler, task_context},
};
use core::{
    mem::{self, MaybeUninit},
    ptr, slice,
    sync::atomic::{AtomicU32, Ordering},
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Capture {
    pub gui: *mut t::Gui,
    pub frame: *mut t::Frame,
    pub generation: u64,
    pub unwind: task_context::Unwind,
}
struct Lock(*mut t::Gui);
impl Lock {
    unsafe fn acquire(gui: *mut t::Gui) -> Self {
        unsafe {
            storage::lock(gui);
        }
        Self(gui)
    }
}
impl Drop for Lock {
    fn drop(&mut self) {
        unsafe {
            storage::unlock(self.0);
        }
    }
}
pub(super) unsafe fn capture(
    instance: *mut ProgramInstance,
    expected: Option<u64>,
    specific: bool,
) -> Option<Capture> {
    unsafe {
        if specific && expected == Some(0) {
            return None;
        }
        let gui = (*instance).gui_payload.cast::<t::Gui>();
        if gui.is_null() {
            return None;
        }
        let operation = threads::Operation::enter()?;
        let _lock = Lock::acquire(gui);
        let mut frame = (*gui).committed_frame;
        if specific {
            while !frame.is_null() {
                if Some((*frame).generation) == expected {
                    break;
                }
                frame = (*frame).base_frame;
            }
        }
        if frame.is_null()
            || (!specific && expected.is_some_and(|generation| (*frame).generation != generation))
            || (*frame).reader_refs == u32::MAX
        {
            return None;
        }
        (*frame).reader_refs += 1;
        let result = Capture {
            gui,
            frame,
            generation: (*frame).generation,
            unwind: operation.0,
        };
        mem::forget(operation);
        Some(result)
    }
}
pub(super) unsafe fn remove_retired(gui: *mut t::Gui, frame: *mut t::Frame) -> bool {
    unsafe {
        let mut previous: *mut t::Frame = ptr::null_mut();
        let mut cursor = (*gui).retired_frames;
        while !cursor.is_null() {
            if cursor == frame {
                if !previous.is_null() {
                    (*previous).retired_next = (*cursor).retired_next;
                } else {
                    (*gui).retired_frames = (*cursor).retired_next;
                }
                life::remove_owned(gui, cursor);
                (*cursor).retired_next = ptr::null_mut();
                (*cursor).retired = false;
                return true;
            }
            previous = cursor;
            cursor = (*cursor).retired_next;
        }
        false
    }
}
pub(super) unsafe fn contains(root: *const t::Frame, wanted: *const t::Frame) -> bool {
    unsafe {
        let mut cursor = root;
        while !cursor.is_null() {
            if cursor == wanted {
                return true;
            }
            cursor = (*cursor).base_frame;
        }
        false
    }
}
pub(super) unsafe fn remove_releasable(
    gui: *mut t::Gui,
    changed: *const t::Frame,
) -> *mut t::Frame {
    unsafe {
        let mut previous: *mut t::Frame = ptr::null_mut();
        let mut cursor = (*gui).retired_frames;
        while !cursor.is_null() {
            if contains(cursor, changed) && !life::readers(cursor) {
                if !previous.is_null() {
                    (*previous).retired_next = (*cursor).retired_next;
                } else {
                    (*gui).retired_frames = (*cursor).retired_next;
                }
                life::remove_owned(gui, cursor);
                (*cursor).retired_next = ptr::null_mut();
                (*cursor).retired = false;
                return cursor;
            }
            previous = cursor;
            cursor = (*cursor).retired_next;
        }
        ptr::null_mut()
    }
}
pub(super) unsafe fn release_capture(instance: *mut ProgramInstance, capture: *mut Capture) {
    unsafe {
        let _operation = threads::Operation((*capture).unwind);
        let mut release = ptr::null_mut();
        let gui = (*capture).gui;
        storage::lock(gui);
        let frame = (*capture).frame;
        if (*frame).reader_refs == 0 {
            payload::add(mem::offset_of!(t::Stats, header_errors), 8, 1);
        } else {
            (*frame).reader_refs -= 1;
            if (*frame).reader_refs == 0 {
                release = remove_releasable(gui, frame);
            }
        }
        storage::unlock(gui);
        if !release.is_null() {
            life::release_frame(release, (*instance).id);
        }
    }
}
pub(super) unsafe fn link_command(frame: *mut t::Frame, node: *mut t::Commands) {
    unsafe {
        (*node).allocation_sequence = storage::sequence(frame);
        (*node).previous = (*frame).command_tail;
        let tail = (*frame).command_tail;
        if !tail.is_null() {
            (*tail).next = node;
        } else {
            (*frame).command_payload = node;
        }
        (*frame).command_tail = node;
        (*frame).command_count += u64::from((*node).command_count);
        life::append(
            ptr::null_mut(),
            frame,
            u64::from((*node).header.requested_bytes),
            0,
        );
        payload::r4os_rust_program_payload_account(4, 0, u64::from((*node).command_count));
    }
}
pub(super) unsafe fn link_resource(frame: *mut t::Frame, node: *mut t::Resource) {
    unsafe {
        (*node).allocation_sequence = storage::sequence(frame);
        (*node).previous = (*frame).resource_tail;
        let tail = (*frame).resource_tail;
        if !tail.is_null() {
            (*tail).next = node;
        } else {
            (*frame).resource_payload = node;
        }
        (*frame).resource_tail = node;
        (*frame).resource_len += u64::from((*node).byte_count);
        life::append(
            ptr::null_mut(),
            frame,
            u64::from((*node).header.requested_bytes),
            0,
        );
        if matches!((*node).resource_kind, 2 | 3) {
            (*frame).raster_words += u64::from((*node).byte_count) / 4;
        }
    }
}
pub(super) unsafe fn copy(destination: *mut u8, source: *const u8, count: usize) {
    unsafe {
        if count <= 65536 {
            if count != 0 {
                ptr::copy_nonoverlapping(source, destination, count);
            }
            return;
        }
        let mut offset = 0;
        while offset < count {
            let chunk = (count - offset).min(65536);
            ptr::copy_nonoverlapping(source.add(offset), destination.add(offset), chunk);
            offset += chunk;
            scheduler::r4os_rust_scheduler_control(11, 0, 0);
        }
    }
}
pub(super) unsafe fn owner(
    instance: *const ProgramInstance,
) -> Option<contract::ProgramProcessHandle> {
    unsafe {
        if let Some(owner) = context::handle() {
            return Some(owner);
        }
        let mut owner = MaybeUninit::uninit();
        if registry::r4os_rust_program_registry_handle(2, instance.cast(), 0, owner.as_mut_ptr()) {
            Some(owner.assume_init())
        } else {
            None
        }
    }
}
pub(super) unsafe fn clone(
    instance: *mut ProgramInstance,
    source: *const t::Frame,
) -> *mut t::Frame {
    unsafe {
        let clone = storage::allocate_frame((*instance).id, false);
        if clone.is_null() {
            return clone;
        }
        let mut chain = [ptr::null::<t::Frame>(); contract::gui_frame_max_delta_chain as usize];
        let mut count = 0;
        let mut cursor = source;
        while !cursor.is_null() {
            if count >= chain.len() {
                life::release_frame(clone, (*instance).id);
                return ptr::null_mut();
            }
            chain[count] = cursor;
            count += 1;
            cursor = (*cursor).base_frame;
        }
        while count != 0 {
            count -= 1;
            let source = chain[count];
            let resource_base = (*clone).resource_len;
            let raster_base = (*clone).raster_words;
            let mut cursor = (*source).command_payload;
            while !cursor.is_null() {
                if !storage::can_link(clone, 1) {
                    life::release_frame(clone, (*instance).id);
                    return ptr::null_mut();
                }
                let node = payload::allocate_commands(
                    (*instance).id,
                    (*clone).command_count,
                    (*cursor).command_count,
                );
                if node.is_null() {
                    life::release_frame(clone, (*instance).id);
                    return ptr::null_mut();
                }
                (*node).command_count = (*cursor).command_count;
                assert!(
                    (*node).command_count <= (*node).capacity
                        && (*cursor).command_count <= (*cursor).capacity
                );
                let commands = node
                    .cast::<u8>()
                    .add(mem::size_of::<t::Commands>())
                    .cast::<t::GuiCommand>();
                let original = cursor
                    .cast::<u8>()
                    .add(mem::size_of::<t::Commands>())
                    .cast::<t::GuiCommand>();
                ptr::copy_nonoverlapping(original, commands, (*node).command_count as usize);
                for index in 0..(*node).command_count as usize {
                    let command = commands.add(index);
                    if (*command).payload_bytes != 0 {
                        (*command).payload_offset += resource_base;
                    }
                    if matches!((*command).resource_kind, 2 | 3) {
                        (*command).raster_word_offset += raster_base;
                    }
                }
                link_command(clone, node);
                scheduler::r4os_rust_scheduler_control(11, 0, 0);
                cursor = (*cursor).next;
            }
            let mut cursor = (*source).resource_payload;
            while !cursor.is_null() {
                if !storage::can_link(clone, 1) {
                    life::release_frame(clone, (*instance).id);
                    return ptr::null_mut();
                }
                let node = payload::allocate_resource(
                    (*instance).id,
                    (*clone).resource_len,
                    (*cursor).raster_word_offset + raster_base,
                    (*cursor).byte_count as usize,
                    (*cursor).resource_kind,
                );
                if node.is_null() {
                    life::release_frame(clone, (*instance).id);
                    return ptr::null_mut();
                }
                assert!((*node).byte_count == (*cursor).byte_count);
                copy(
                    node.cast::<u8>().add(mem::size_of::<t::Resource>()),
                    cursor.cast::<u8>().add(mem::size_of::<t::Resource>()),
                    (*node).byte_count as usize,
                );
                link_resource(clone, node);
                scheduler::r4os_rust_scheduler_control(11, 0, 0);
                cursor = (*cursor).next;
            }
            (*clone).raster_words = raster_base + (*source).raster_words;
        }
        let mut shared_owner = None;
        let mut cursor = source;
        while !cursor.is_null() {
            if (*cursor).shared_raster_count != 0 {
                let selected = if let Some(selected) = shared_owner {
                    Some(selected)
                } else {
                    owner(instance)
                };
                let Some(selected) = selected else {
                    life::release_frame(clone, (*instance).id);
                    return ptr::null_mut();
                };
                shared_owner = Some(selected);
                if !super::shared_raster::copy_frames(selected, cursor, clone) {
                    life::release_frame(clone, (*instance).id);
                    return ptr::null_mut();
                }
            }
            cursor = (*cursor).base_frame;
        }
        clone
    }
}
pub(super) unsafe fn failed(gui: *mut t::Gui, frame: *mut t::Frame) {
    unsafe {
        if !frame.is_null() {
            (*frame).build_failed = true;
        }
        (*gui).frame_oom = (*gui).frame_oom.wrapping_add(1);
        (*gui).frame_last_error = contract::gui_frame_error_oom;
        payload::add(mem::offset_of!(t::Stats, gui_frame_oom_failures), 8, 1);
    }
}
pub(super) unsafe fn result(gui: *mut t::Gui, result: i32) -> i32 {
    unsafe {
        (*gui).frame_last_error = result;
    }
    result
}
pub(super) fn damage_valid(region: contract::DisplayDamageRect) -> bool {
    if region.w == 0 || region.h == 0 {
        return false;
    }
    let Some(right) = i64::from(region.x).checked_add(i64::from(region.w)) else {
        return false;
    };
    let Some(bottom) = i64::from(region.y).checked_add(i64::from(region.h)) else {
        return false;
    };
    right <= i64::from(i32::MAX) && bottom <= i64::from(i32::MAX)
}
unsafe fn begin_invalid(gui: *const t::Gui, mode: u8) -> bool {
    unsafe {
        if !(*gui).building_frame.is_null() {
            return true;
        }
        mode == 1
            && ((*gui).committed_frame.is_null()
                || (*(*gui).committed_frame).chain_depth >= contract::gui_frame_max_delta_chain)
    }
}
pub(super) unsafe fn begin(
    instance: *mut ProgramInstance,
    mode: u8,
    regions: &[contract::DisplayDamageRect],
) -> i32 {
    unsafe {
        if mode != 0
            && (regions.is_empty()
                || regions.len() > contract::gui_frame_max_damage_regions as usize
                || regions.iter().any(|&region| !damage_valid(region)))
        {
            return contract::gui_frame_error_invalid;
        }
        let gui = process_data::ensure_gui(instance);
        if gui.is_null() {
            return if mode == 0 {
                -6
            } else {
                contract::gui_frame_error_unavailable
            };
        }
        storage::lock(gui);
        if begin_invalid(gui, mode) {
            storage::unlock(gui);
            return if mode == 0 {
                -3
            } else {
                result(gui, contract::gui_frame_error_state)
            };
        }
        storage::unlock(gui);
        let frame = storage::allocate_frame((*instance).id, true);
        if frame.is_null() {
            failed(gui, ptr::null_mut());
            return if mode == 0 {
                -6
            } else {
                contract::gui_frame_error_oom
            };
        }
        if mode != 0 {
            ptr::copy_nonoverlapping(
                regions.as_ptr(),
                ptr::addr_of_mut!((*frame).damage_regions).cast(),
                regions.len(),
            );
            (*frame).damage_count = u32::try_from(regions.len()).expect("Damage region count");
            if mode == 2 {
                (*frame).replacement = true;
            }
        }
        storage::lock(gui);
        if begin_invalid(gui, mode) {
            storage::unlock(gui);
            life::release_frame(frame, (*instance).id);
            return if mode == 0 {
                -3
            } else {
                result(gui, contract::gui_frame_error_state)
            };
        }
        life::set_building(gui, frame);
        storage::unlock(gui);
        if mode == 0 {
            0
        } else {
            result(gui, contract::gui_frame_result_ok)
        }
    }
}
pub(super) unsafe fn replace(instance: *mut ProgramInstance, explicit: bool) -> *mut t::Frame {
    unsafe {
        let gui = process_data::ensure_gui(instance);
        if gui.is_null() {
            return ptr::null_mut();
        }
        let frame = storage::allocate_frame((*instance).id, explicit);
        if frame.is_null() {
            failed(gui, (*gui).building_frame);
            return frame;
        }
        storage::lock(gui);
        let old = (*gui).building_frame;
        life::set_building(gui, frame);
        storage::unlock(gui);
        if !old.is_null() {
            life::release_frame(old, (*instance).id);
        }
        frame
    }
}
pub(super) unsafe fn ensure(instance: *mut ProgramInstance) -> *mut t::Frame {
    unsafe {
        let gui = process_data::ensure_gui(instance);
        if gui.is_null() {
            return ptr::null_mut();
        }
        let frame = (*gui).building_frame;
        if !frame.is_null() {
            return if (*frame).build_failed {
                ptr::null_mut()
            } else {
                frame
            };
        }
        loop {
            if let Some(mut captured) = capture(instance, None, false) {
                let clone = clone(instance, captured.frame);
                if clone.is_null() {
                    failed(gui, ptr::null_mut());
                    release_capture(instance, &mut captured);
                    return ptr::null_mut();
                }
                storage::lock(gui);
                let frame = (*gui).building_frame;
                if !frame.is_null() {
                    storage::unlock(gui);
                    release_capture(instance, &mut captured);
                    life::release_frame(clone, (*instance).id);
                    return if (*frame).build_failed {
                        ptr::null_mut()
                    } else {
                        frame
                    };
                }
                if (*gui).committed_frame == captured.frame
                    && (*(*gui).committed_frame).generation == captured.generation
                {
                    life::set_building(gui, clone);
                    storage::unlock(gui);
                    release_capture(instance, &mut captured);
                    return clone;
                }
                storage::unlock(gui);
                release_capture(instance, &mut captured);
                life::release_frame(clone, (*instance).id);
                continue;
            }
            let empty = storage::allocate_frame((*instance).id, false);
            if empty.is_null() {
                failed(gui, ptr::null_mut());
                return empty;
            }
            storage::lock(gui);
            let frame = (*gui).building_frame;
            if !frame.is_null() {
                storage::unlock(gui);
                life::release_frame(empty, (*instance).id);
                return if (*frame).build_failed {
                    ptr::null_mut()
                } else {
                    frame
                };
            }
            if (*gui).committed_frame.is_null() {
                life::set_building(gui, empty);
                storage::unlock(gui);
                return empty;
            }
            storage::unlock(gui);
            life::release_frame(empty, (*instance).id);
        }
    }
}
pub(super) unsafe fn cancel(instance: *mut ProgramInstance) -> i32 {
    unsafe {
        let gui = (*instance).gui_payload.cast::<t::Gui>();
        if gui.is_null() {
            return -2;
        }
        storage::lock(gui);
        let frame = (*gui).building_frame;
        if frame.is_null() {
            storage::unlock(gui);
            return -3;
        }
        life::set_building(gui, ptr::null_mut());
        (*gui).frame_cancels = (*gui).frame_cancels.wrapping_add(1);
        payload::add(mem::offset_of!(t::Stats, gui_frame_cancels), 8, 1);
        storage::unlock(gui);
        life::release_frame(frame, (*instance).id);
        0
    }
}
unsafe fn command_stats(gui: *mut t::Gui, frame: *const t::Frame, kind: u32) {
    unsafe {
        let mut node = (*frame).command_payload;
        while !node.is_null() {
            assert!((*node).command_count <= (*node).capacity);
            let commands = node
                .cast::<u8>()
                .add(mem::size_of::<t::Commands>())
                .cast::<t::GuiCommand>();
            for index in 0..(*node).command_count as usize {
                let command = commands.add(index).read();
                if command.kind != kind {
                    continue;
                }
                if kind == contract::gui_frame_command_kind_indexed8 {
                    (*gui).frame_indexed8_commands = (*gui).frame_indexed8_commands.wrapping_add(1);
                    (*gui).frame_indexed8_resource_bytes = (*gui)
                        .frame_indexed8_resource_bytes
                        .saturating_add(command.payload_bytes);
                } else {
                    (*gui).frame_xrgb32_nearest_commands =
                        (*gui).frame_xrgb32_nearest_commands.wrapping_add(1);
                    (*gui).frame_xrgb32_nearest_resource_bytes = (*gui)
                        .frame_xrgb32_nearest_resource_bytes
                        .saturating_add(command.payload_bytes);
                }
            }
            node = (*node).next;
        }
    }
}
pub(super) unsafe fn revision(instance: *mut ProgramInstance) {
    unsafe {
        let gui = (*instance).gui_payload.cast::<t::Gui>();
        if gui.is_null() {
            return;
        }
        let revision = AtomicU32::from_ptr(ptr::addr_of_mut!((*gui).revision));
        let mut old = revision.load(Ordering::Acquire);
        loop {
            let next = if old == u32::MAX { 1 } else { old + 1 };
            match revision.compare_exchange_weak(old, next, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => break,
                Err(actual) => old = actual,
            }
        }
        console::sink(4, 0, 0);
    }
}
pub(super) unsafe fn commit(instance: *mut ProgramInstance) -> i32 {
    unsafe {
        let gui = (*instance).gui_payload.cast::<t::Gui>();
        if gui.is_null() {
            return -2;
        }
        let building = (*gui).building_frame;
        if building.is_null() {
            return -3;
        }
        if (*building).build_failed || !life::valid(building, (*instance).id) {
            return -3;
        }
        let replacement = (*building).replacement;
        let delta = (*building).damage_count != 0 && !replacement;
        let mut release_old = ptr::null_mut();
        storage::lock(gui);
        if (*gui).building_frame != building || (*building).build_failed {
            storage::unlock(gui);
            return -3;
        }
        let old = (*gui).committed_frame;
        if delta && (old.is_null() || (*old).chain_depth >= contract::gui_frame_max_delta_chain) {
            storage::unlock(gui);
            return result(gui, contract::gui_frame_error_state);
        }
        let Some(generation) = storage::reserve_generation() else {
            (*building).build_failed = true;
            storage::unlock(gui);
            return -7;
        };
        (*building).generation = generation;
        (*building).explicit_build = false;
        if delta {
            assert!(!old.is_null());
            life::set_base(building, old);
        }
        (*gui).committed_frame = building;
        (*gui).building_frame = ptr::null_mut();
        (*gui).frame_commits = (*gui).frame_commits.wrapping_add(1);
        payload::add(mem::offset_of!(t::Stats, gui_frame_commits), 8, 1);
        if delta {
            (*gui).frame_delta_commits = (*gui).frame_delta_commits.wrapping_add(1);
            (*gui).frame_avoided_clone_bytes = (*gui)
                .frame_avoided_clone_bytes
                .saturating_add(life::frame_bytes(old));
        } else {
            (*gui).frame_full_commits = (*gui).frame_full_commits.wrapping_add(1);
            if replacement {
                (*gui).frame_replacement_commits = (*gui).frame_replacement_commits.wrapping_add(1);
            }
            if !old.is_null() {
                if replacement {
                    (*gui).frame_superseded_generations =
                        (*gui).frame_superseded_generations.wrapping_add(1);
                    (*gui).frame_coalesced_generations = (*gui)
                        .frame_coalesced_generations
                        .saturating_add(u64::from((*old).chain_depth));
                }
                if !life::readers(old) {
                    life::remove_owned(gui, old);
                    release_old = old;
                } else {
                    if replacement {
                        (*gui).frame_reader_retired_generations =
                            (*gui).frame_reader_retired_generations.wrapping_add(1);
                    }
                    (*old).retired = true;
                    (*old).retired_next = (*gui).retired_frames;
                    (*gui).retired_frames = old;
                }
            }
        }
        command_stats(gui, building, contract::gui_frame_command_kind_indexed8);
        command_stats(
            gui,
            building,
            contract::gui_frame_command_kind_xrgb32_nearest,
        );
        let bytes = life::frame_bytes(building);
        if bytes > (*gui).frame_stream_peak_bytes {
            (*gui).frame_stream_peak_bytes = bytes;
        }
        life::refresh_peak(gui);
        storage::unlock(gui);
        if !release_old.is_null() {
            life::release_frame(release_old, (*instance).id);
        }
        if (*building).shared_raster_count != 0
            && let Some(owner) = owner(instance)
        {
            super::shared_raster::note_commit(owner, building);
        }
        revision(instance);
        0
    }
}
/// # Safety
/// Retained instance and exclusive Capture output, transferred with its exact
/// unwind token. Absence leaves output untouched; caller releases once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_capture(
    instance: *mut ProgramInstance,
    generation: u64,
    present: bool,
    specific: bool,
    out: *mut Capture,
) -> bool {
    unsafe {
        let Some(captured) = capture(
            instance,
            if present { Some(generation) } else { None },
            specific,
        ) else {
            return false;
        };
        out.write(captured);
        true
    }
}
/// # Safety
/// Exact captured frame and original instance; consumes its reader/unwind lease.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_release_capture(
    instance: *mut ProgramInstance,
    capture: *mut Capture,
) {
    unsafe {
        release_capture(instance, capture);
    }
}
/// # Safety
/// Exact GUI metadata owner and frame under its original lock.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_remove_retired(
    gui: *mut t::Gui,
    frame: *mut t::Frame,
    releasable: bool,
) -> *mut t::Frame {
    unsafe {
        if releasable {
            remove_releasable(gui, frame)
        } else if remove_retired(gui, frame) {
            frame
        } else {
            ptr::null_mut()
        }
    }
}
/// # Safety
/// Exact retained immutable frame chain under original reader admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_contains(
    root: *const t::Frame,
    wanted: *const t::Frame,
) -> bool {
    unsafe { contains(root, wanted) }
}
/// # Safety
/// Exact unpublished private frame and newly allocated typed node.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_link(
    frame: *mut t::Frame,
    node: *mut core::ffi::c_void,
    resource: bool,
) {
    unsafe {
        if resource {
            link_resource(frame, node.cast());
        } else {
            link_command(frame, node.cast());
        }
    }
}
/// # Safety
/// Equal-length disjoint valid source/destination spans retained over reschedule.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_copy(
    destination: *mut u8,
    source: *const u8,
    count: usize,
) {
    unsafe {
        copy(destination, source, count);
    }
}
/// # Safety
/// Exact retained instance and captured immutable source across allocations/yields.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_clone(
    instance: *mut ProgramInstance,
    source: *const t::Frame,
) -> *mut t::Frame {
    unsafe { clone(instance, source) }
}
/// # Safety
/// Exact producer GUI and optional private build frame.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_failed(gui: *mut t::Gui, frame: *mut t::Frame) {
    unsafe {
        failed(gui, frame);
    }
}
/// # Safety
/// Retained producer instance; valid region span for the damage/replacement modes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_begin(
    instance: *mut ProgramInstance,
    mode: u8,
    regions: *const contract::DisplayDamageRect,
    count: usize,
) -> i32 {
    unsafe {
        begin(
            instance,
            mode,
            if count == 0 {
                &[]
            } else {
                slice::from_raw_parts(regions, count)
            },
        )
    }
}
/// # Safety
/// Retained producer instance under original caller operation admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_build(
    instance: *mut ProgramInstance,
    explicit: bool,
    replace_build: bool,
) -> *mut t::Frame {
    unsafe {
        if replace_build {
            replace(instance, explicit)
        } else {
            ensure(instance)
        }
    }
}
/// # Safety
/// Retained producer instance under original operation admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_publish(
    instance: *mut ProgramInstance,
    cancel_build: bool,
) -> i32 {
    unsafe {
        if cancel_build {
            cancel(instance)
        } else {
            commit(instance)
        }
    }
}
/// # Safety
/// Retained instance; absent lazy GUI is permitted.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_revision(instance: *mut ProgramInstance) {
    unsafe {
        revision(instance);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_gui_damage_valid(region: contract::DisplayDamageRect) -> bool {
    damage_valid(region)
}
/// # Safety
/// Retained exclusive GUI diagnostic result under original caller admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_result(gui: *mut t::Gui, value: i32) -> i32 {
    unsafe { result(gui, value) }
}
const _: () = {
    assert!(mem::size_of::<Capture>() == 40);
    assert!(mem::align_of::<Capture>() == 8);
    assert!(mem::offset_of!(Capture, gui) == 0);
    assert!(mem::offset_of!(Capture, frame) == 8);
    assert!(mem::offset_of!(Capture, generation) == 16);
    assert!(mem::offset_of!(Capture, unwind) == 24);
};
