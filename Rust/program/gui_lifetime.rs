//! Exact GUI frame node ownership, command/resource semantics and reverse
//! release order. Partial private builds use ownership validation for cleanup;
//! published chains additionally retain their complete semantic validation.
use super::{payload, payload_types as t};
use crate::contract;
use core::{mem, ptr, slice};
fn error() -> bool {
    payload::add(mem::offset_of!(t::Stats, header_errors), 8, 1);
    false
}
unsafe fn commands_valid(node: *const t::Commands, owner: u32) -> bool {
    unsafe {
        let Some(bytes) = payload::command_bytes((*node).capacity) else {
            return error();
        };
        if (*node).allocation_sequence == 0
            || (*node).command_count == 0
            || (*node).command_count > (*node).capacity
            || !payload::valid(ptr::addr_of!((*node).header), owner, 7, bytes)
        {
            return error();
        }
        let commands = node
            .cast::<u8>()
            .add(mem::size_of::<t::Commands>())
            .cast::<t::GuiCommand>();
        for command in slice::from_raw_parts(commands, (*node).command_count as usize) {
            if command.version != 1 || command.size != 96 || command.reserved != 0 {
                return error();
            }
        }
        true
    }
}
unsafe fn resources_valid(node: *const t::Resource, owner: u32) -> bool {
    unsafe {
        let Some(kind) = payload::resource_kind((*node).resource_kind) else {
            return error();
        };
        let Some(bytes) = payload::resource_bytes((*node).byte_count as usize) else {
            return error();
        };
        if (*node).allocation_sequence == 0
            || (kind == 8 && !(*node).byte_count.is_multiple_of(4))
            || (*node).reserved != 0
            || !payload::valid(ptr::addr_of!((*node).header), owner, kind, bytes)
        {
            return error();
        }
        true
    }
}
pub(super) unsafe fn ownership(frame: *const t::Frame, owner: u32) -> bool {
    unsafe {
        if !payload::valid(
            ptr::addr_of!((*frame).header),
            owner,
            9,
            mem::size_of::<t::Frame>(),
        ) {
            return false;
        }
        if ((*frame).build_failed && (*frame).generation != 0)
            || (*frame).shared_raster_count > contract::gui_shared_raster_max_frame_resources
        {
            return error();
        }
        if (*frame).command_payload.is_null() {
            if !(*frame).command_tail.is_null() || (*frame).command_count != 0 {
                return error();
            }
        } else if (*frame).command_tail.is_null() || (*frame).command_count == 0 {
            return error();
        }
        let mut previous = ptr::null_mut();
        let mut cursor = (*frame).command_payload;
        let mut logical = 0u64;
        while !cursor.is_null() {
            if !commands_valid(cursor, owner) {
                return false;
            }
            if (*cursor).previous != previous || (*cursor).logical_offset != logical {
                return error();
            }
            let Some(next_logical) = logical.checked_add(u64::from((*cursor).command_count)) else {
                return error();
            };
            logical = next_logical;
            if logical > (*frame).command_count
                || ((*cursor).next.is_null() && (*frame).command_tail != cursor)
            {
                return error();
            }
            previous = cursor;
            cursor = (*cursor).next;
        }
        if previous != (*frame).command_tail || logical != (*frame).command_count {
            return error();
        }
        if (*frame).resource_payload.is_null() {
            if !(*frame).resource_tail.is_null() || (*frame).resource_len != 0 {
                return error();
            }
        } else if (*frame).resource_tail.is_null() || (*frame).resource_len == 0 {
            return error();
        }
        let mut previous = ptr::null_mut();
        let mut cursor = (*frame).resource_payload;
        let mut logical = 0u64;
        while !cursor.is_null() {
            if !resources_valid(cursor, owner) {
                return false;
            }
            if (*cursor).previous != previous || (*cursor).logical_offset != logical {
                return error();
            }
            let Some(next_logical) = logical.checked_add(u64::from((*cursor).byte_count)) else {
                return error();
            };
            logical = next_logical;
            if logical > (*frame).resource_len {
                return error();
            }
            if !matches!((*cursor).resource_kind, 2 | 3) && (*cursor).raster_word_offset != 0 {
                return error();
            }
            if (*cursor).next.is_null() && (*frame).resource_tail != cursor {
                return error();
            }
            previous = cursor;
            cursor = (*cursor).next;
        }
        if previous != (*frame).resource_tail || logical != (*frame).resource_len {
            return error();
        }
        let mut commands = (*frame).command_payload;
        let mut resources = (*frame).resource_payload;
        let mut last = 0;
        while !commands.is_null() || !resources.is_null() {
            let take_command = if commands.is_null() {
                false
            } else if resources.is_null() {
                true
            } else {
                if (*commands).allocation_sequence == (*resources).allocation_sequence {
                    return error();
                }
                (*commands).allocation_sequence < (*resources).allocation_sequence
            };
            let sequence = if take_command {
                (*commands).allocation_sequence
            } else {
                (*resources).allocation_sequence
            };
            if last == u64::MAX || sequence != last + 1 {
                return error();
            }
            last = sequence;
            if take_command {
                commands = (*commands).next;
            } else {
                resources = (*resources).next;
            }
        }
        if last != (*frame).node_sequence {
            return error();
        }
        true
    }
}
unsafe fn physical(mut cursor: *const t::Resource) -> *const t::Resource {
    unsafe {
        while !cursor.is_null() {
            if matches!((*cursor).resource_kind, 2 | 3) {
                return cursor;
            }
            cursor = (*cursor).next;
        }
        ptr::null()
    }
}
pub(super) unsafe fn valid(frame: *const t::Frame, owner: u32) -> bool {
    unsafe {
        if !ownership(frame, owner) {
            return false;
        }
        if (*frame).damage_count > contract::gui_frame_max_damage_regions
            || (*frame).chain_depth == 0
            || (*frame).chain_depth > contract::gui_frame_max_delta_chain
        {
            return false;
        }
        for index in 0..(*frame).damage_count as usize {
            let region = (*frame).damage_regions[index];
            if region.w == 0 || region.h == 0 {
                return false;
            }
            let Some(right) = i64::from(region.x).checked_add(i64::from(region.w)) else {
                return false;
            };
            let Some(bottom) = i64::from(region.y).checked_add(i64::from(region.h)) else {
                return false;
            };
            if right > i64::from(i32::MAX) || bottom > i64::from(i32::MAX) {
                return false;
            }
        }
        let mut raster_words = 0u64;
        let mut resource = physical((*frame).resource_payload);
        let mut cursor = (*frame).command_payload;
        while !cursor.is_null() {
            let commands = cursor
                .cast::<u8>()
                .add(mem::size_of::<t::Commands>())
                .cast::<t::GuiCommand>();
            for command in slice::from_raw_parts(commands, (*cursor).command_count as usize) {
                let expected = match command.kind {
                    contract::gui_frame_command_kind_clear
                    | contract::gui_frame_command_kind_rect => 0,
                    contract::gui_frame_command_kind_text => {
                        if command.payload_bytes == 0 {
                            0
                        } else {
                            1
                        }
                    }
                    contract::gui_frame_command_kind_raster => 2,
                    contract::gui_frame_command_kind_alpha8 => 3,
                    contract::gui_frame_command_kind_path_fill
                    | contract::gui_frame_command_kind_path_stroke
                    | contract::gui_frame_command_kind_rounded_rect
                    | contract::gui_frame_command_kind_shadow
                    | contract::gui_frame_command_kind_argb32 => 4,
                    contract::gui_frame_command_kind_indexed8 => 5,
                    contract::gui_frame_command_kind_xrgb32_nearest => 6,
                    contract::gui_frame_command_kind_shared_raster => 7,
                    _ => return error(),
                };
                if command.resource_kind != expected {
                    return error();
                }
                if command.resource_kind == 0 {
                    if command.payload_offset != 0
                        || command.payload_bytes != 0
                        || command.raster_word_offset != 0
                    {
                        return error();
                    }
                    continue;
                }
                let Some(end) = command.payload_offset.checked_add(command.payload_bytes) else {
                    return error();
                };
                if command.payload_bytes == 0 || end > (*frame).resource_len {
                    return error();
                }
                if matches!(command.resource_kind, 2 | 3) {
                    let words = if command.resource_kind == 2 {
                        command.payload_bytes / 4
                    } else {
                        let Some(bytes) = command.payload_bytes.checked_add(3) else {
                            return false;
                        };
                        bytes / 4
                    };
                    if (command.resource_kind == 2 && !command.payload_bytes.is_multiple_of(4))
                        || command.raster_word_offset != raster_words
                    {
                        return error();
                    }
                    if !resource.is_null() {
                        if command.payload_offset > (*resource).logical_offset {
                            return error();
                        }
                        if command.payload_offset == (*resource).logical_offset {
                            let Some(bytes) = words.checked_mul(4) else {
                                return error();
                            };
                            if (*resource).resource_kind != command.resource_kind
                                || u64::from((*resource).byte_count) != bytes
                                || (*resource).raster_word_offset != command.raster_word_offset
                            {
                                return error();
                            }
                            resource = physical((*resource).next);
                        }
                    }
                    let Some(next) = raster_words.checked_add(words) else {
                        return error();
                    };
                    raster_words = next;
                } else if command.raster_word_offset != 0 {
                    return error();
                }
            }
            cursor = (*cursor).next;
        }
        if !resource.is_null() || raster_words != (*frame).raster_words {
            return error();
        }
        true
    }
}
unsafe fn committed(root: *const t::Frame, owner: u32, retired: bool) -> bool {
    unsafe {
        let mut cursor = root;
        let mut depth = 0u32;
        while !cursor.is_null() {
            depth += 1;
            if depth > contract::gui_frame_max_delta_chain
                || (*cursor).generation == 0
                || (*cursor).build_failed
                || !valid(cursor, owner)
                || (*cursor).retired != (depth == 1 && retired)
            {
                return false;
            }
            let base = (*cursor).base_frame;
            if !base.is_null() {
                if (*cursor).replacement
                    || (*cursor).damage_count == 0
                    || (*cursor).chain_depth != (*base).chain_depth + 1
                    || (*base).generation >= (*cursor).generation
                {
                    return false;
                }
            } else if (*cursor).chain_depth != 1
                || (*cursor).replacement != ((*cursor).damage_count != 0)
            {
                return false;
            }
            cursor = base;
        }
        depth == (*root).chain_depth
    }
}
unsafe fn local_bytes(frame: *const t::Frame) -> u64 {
    unsafe { (*frame).local_bytes }
}
pub(super) unsafe fn frame_bytes(frame: *const t::Frame) -> u64 {
    if frame.is_null() {
        return 0;
    }
    unsafe {
        (*frame)
            .local_bytes
            .checked_add((*frame).base_bytes)
            .expect("GUI frame bytes")
    }
}
pub(super) unsafe fn chain_count(frame: *const t::Frame, kind: u8) -> u64 {
    if frame.is_null() {
        return 0;
    }
    unsafe {
        let (local, base) = match kind {
            0 => ((*frame).command_count, (*frame).base_commands),
            1 => ((*frame).resource_len, (*frame).base_resources),
            2 => ((*frame).raster_words, (*frame).base_raster_words),
            _ => unreachable!(),
        };
        local.checked_add(base).expect("GUI chain count")
    }
}
/// Bind an immutable base, or detach it during terminal release. Ownership
/// moves between roots without charging that same base a second time.
pub(super) unsafe fn set_base(frame: *mut t::Frame, base: *mut t::Frame) {
    unsafe {
        (*frame).base_bytes = frame_bytes(base);
        (*frame).base_commands = chain_count(base, 0);
        (*frame).base_resources = chain_count(base, 1);
        (*frame).base_raster_words = chain_count(base, 2);
        (*frame).base_frame = base;
        (*frame).chain_depth = if base.is_null() {
            1
        } else {
            (*base).chain_depth + 1
        };
    }
}
pub(super) unsafe fn readers(frame: *const t::Frame) -> bool {
    unsafe {
        let mut cursor = frame;
        while !cursor.is_null() {
            if (*cursor).reader_refs != 0 {
                return true;
            }
            cursor = (*cursor).base_frame;
        }
        false
    }
}
pub(super) unsafe fn live_generations(gui: *const t::Gui) -> u32 {
    unsafe {
        let committed = (*gui).committed_frame;
        let mut count = if committed.is_null() {
            0
        } else {
            u64::from((*committed).chain_depth)
        };
        let mut retired = (*gui).retired_frames;
        while !retired.is_null() {
            count = count.saturating_add(u64::from((*retired).chain_depth));
            retired = (*retired).retired_next;
        }
        count.min(u64::from(u32::MAX)) as u32
    }
}
pub(super) unsafe fn owned(gui: *const t::Gui, commands: bool) -> u64 {
    unsafe {
        if commands {
            (*gui).frame_owned_commands
        } else {
            (*gui).frame_owned_bytes
        }
    }
}
pub(super) unsafe fn refresh_peak(gui: *mut t::Gui) {
    unsafe {
        (*gui).frame_peak_bytes = (*gui).frame_peak_bytes.max((*gui).frame_owned_bytes);
        (*gui).frame_peak_commands = (*gui).frame_peak_commands.max((*gui).frame_owned_commands);
    }
}
unsafe fn add_owned(gui: *mut t::Gui, bytes: u64, commands: u64) {
    unsafe {
        (*gui).frame_owned_bytes = (*gui)
            .frame_owned_bytes
            .checked_add(bytes)
            .expect("GUI owned bytes");
        (*gui).frame_owned_commands = (*gui)
            .frame_owned_commands
            .checked_add(commands)
            .expect("GUI owned commands");
    }
}
pub(super) unsafe fn remove_owned(gui: *mut t::Gui, frame: *const t::Frame) {
    unsafe {
        (*gui).frame_owned_bytes = (*gui)
            .frame_owned_bytes
            .checked_sub(frame_bytes(frame))
            .expect("GUI removed bytes");
        (*gui).frame_owned_commands = (*gui)
            .frame_owned_commands
            .checked_sub(chain_count(frame, 0))
            .expect("GUI removed commands");
    }
}
/// The original GUI admission covers this bounded metadata operation. Reserved
/// private nodes/batches are charged only when their links are adopted.
pub(super) unsafe fn append(gui: *mut t::Gui, frame: *mut t::Frame, bytes: u64, commands: u64) {
    unsafe {
        (*frame).local_bytes = (*frame)
            .local_bytes
            .checked_add(bytes)
            .expect("GUI local bytes");
        if !gui.is_null() && (*gui).building_frame == frame {
            add_owned(gui, bytes, commands);
            refresh_peak(gui);
        }
    }
}
/// Adopt/replace/cancel a private building root under the existing GUI lock.
pub(super) unsafe fn set_building(gui: *mut t::Gui, frame: *mut t::Frame) {
    unsafe {
        remove_owned(gui, (*gui).building_frame);
        (*gui).building_frame = frame;
        if !frame.is_null() {
            assert!((*frame).base_frame.is_null());
            add_owned(gui, frame_bytes(frame), (*frame).command_count);
        }
        refresh_peak(gui);
    }
}
pub(super) unsafe fn release_local(frame: *mut t::Frame, owner: u32) -> bool {
    unsafe {
        if (*frame).reader_refs != 0 || !ownership(frame, owner) {
            return false;
        }
        let mut released = true;
        let mut resource = (*frame).resource_tail;
        let mut command = (*frame).command_tail;
        while !resource.is_null() || !command.is_null() {
            let release_resource = if resource.is_null() {
                false
            } else if command.is_null() {
                true
            } else {
                (*resource).allocation_sequence > (*command).allocation_sequence
            };
            if release_resource {
                let previous = (*resource).previous;
                (*frame).resource_tail = previous;
                if !previous.is_null() {
                    (*previous).next = ptr::null_mut();
                } else {
                    (*frame).resource_payload = ptr::null_mut();
                }
                (*resource).previous = ptr::null_mut();
                (*resource).next = ptr::null_mut();
                if !payload::release_resource(resource, owner) {
                    released = false;
                }
                resource = previous;
            } else {
                let previous = (*command).previous;
                (*frame).command_tail = previous;
                if !previous.is_null() {
                    (*previous).next = ptr::null_mut();
                } else {
                    (*frame).command_payload = ptr::null_mut();
                }
                (*command).previous = ptr::null_mut();
                (*command).next = ptr::null_mut();
                if !payload::release_commands(command, owner) {
                    released = false;
                }
                command = previous;
            }
        }
        (*frame).resource_len = 0;
        (*frame).raster_words = 0;
        (*frame).command_count = 0;
        (*frame).node_sequence = 0;
        (*frame).local_bytes = u64::from((*frame).header.requested_bytes);
        (*frame).retired_next = ptr::null_mut();
        if !super::shared_raster::release_frames(frame) {
            released = false;
        }
        if !payload::r4os_rust_program_payload_release(9, frame.cast(), owner, 9) {
            released = false;
        }
        released
    }
}
pub(super) unsafe fn release_frame(frame: *mut t::Frame, owner: u32) -> bool {
    unsafe {
        if readers(frame) {
            return false;
        }
        let mut released = true;
        let mut cursor = frame;
        while !cursor.is_null() {
            let base = (*cursor).base_frame;
            set_base(cursor, ptr::null_mut());
            if !release_local(cursor, owner) {
                released = false;
            }
            cursor = base;
        }
        released
    }
}
pub(super) unsafe fn valid_set(gui: *const t::Gui, owner: u32) -> bool {
    unsafe {
        let committed_frame = (*gui).committed_frame;
        let building = (*gui).building_frame;
        if !committed_frame.is_null() && committed_frame == building {
            return error();
        }
        if !committed_frame.is_null() && !committed(committed_frame, owner, false) {
            return false;
        }
        if !building.is_null()
            && ((*building).generation != 0
                || (*building).retired
                || (*building).reader_refs != 0
                || !(*building).base_frame.is_null()
                || (*building).chain_depth != 1
                || !valid(building, owner))
        {
            return false;
        }
        let mut cursor = (*gui).retired_frames;
        let mut previous = u64::MAX;
        while !cursor.is_null() {
            if !(*cursor).retired
                || (*cursor).generation == 0
                || (*cursor).generation >= previous
                || cursor == committed_frame
                || cursor == building
                || !committed(cursor, owner, true)
            {
                return error();
            }
            previous = (*cursor).generation;
            cursor = (*cursor).retired_next;
        }
        true
    }
}
pub(super) unsafe fn release_set(gui: *mut t::Gui, owner: u32) -> bool {
    unsafe {
        if !valid_set(gui, owner) {
            return false;
        }
        let committed = (*gui).committed_frame;
        let building = (*gui).building_frame;
        if (!committed.is_null() && readers(committed))
            || (!building.is_null() && readers(building))
        {
            return false;
        }
        let mut check = (*gui).retired_frames;
        while !check.is_null() {
            if readers(check) {
                return false;
            }
            check = (*check).retired_next;
        }
        let mut retired = (*gui).retired_frames;
        (*gui).building_frame = ptr::null_mut();
        (*gui).committed_frame = ptr::null_mut();
        (*gui).retired_frames = ptr::null_mut();
        (*gui).frame_owned_bytes = 0;
        (*gui).frame_owned_commands = 0;
        let mut released = true;
        if !building.is_null() && !release_frame(building, owner) {
            released = false;
        }
        if !committed.is_null() && !release_frame(committed, owner) {
            released = false;
        }
        while !retired.is_null() {
            let next = (*retired).retired_next;
            (*retired).retired_next = ptr::null_mut();
            if !release_frame(retired, owner) {
                released = false;
            }
            retired = next;
        }
        released
    }
}
/// # Safety
/// Exact frame/node allocation under the caller's original GUI admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_validate(
    op: u8,
    data: *const core::ffi::c_void,
    owner: u32,
    retired: bool,
) -> bool {
    unsafe {
        match op {
            0 => commands_valid(data.cast(), owner),
            1 => resources_valid(data.cast(), owner),
            2 => ownership(data.cast(), owner),
            3 => valid(data.cast(), owner),
            4 => committed(data.cast(), owner, retired),
            5 => valid_set(data.cast(), owner),
            _ => unreachable!(),
        }
    }
}
/// # Safety
/// Exact resource node chain under the caller's original GUI admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_physical(
    start: *const t::Resource,
) -> *const t::Resource {
    unsafe { physical(start) }
}
/// # Safety
/// Exact frame or GUI payload under the original caller admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_count(
    op: u8,
    data: *const core::ffi::c_void,
) -> u64 {
    unsafe {
        match op {
            0 => local_bytes(data.cast()),
            1 => frame_bytes(data.cast()),
            2..=4 => chain_count(data.cast(), op - 2),
            5 => u64::from(readers(data.cast())),
            6 => u64::from(live_generations(data.cast())),
            7 => owned(data.cast(), false),
            8 => owned(data.cast(), true),
            _ => unreachable!(),
        }
    }
}
/// # Safety
/// Exclusive GUI owner under its unchanged frame lock/execution admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_peak(gui: *mut t::Gui) {
    unsafe {
        refresh_peak(gui);
    }
}
/// # Safety
/// Exact frame/GUI owner with no remaining readers and original cleanup rights.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_release(
    op: u8,
    data: *mut core::ffi::c_void,
    owner: u32,
) -> bool {
    unsafe {
        match op {
            0 => release_local(data.cast(), owner),
            1 => release_frame(data.cast(), owner),
            2 => release_set(data.cast(), owner),
            _ => unreachable!(),
        }
    }
}
