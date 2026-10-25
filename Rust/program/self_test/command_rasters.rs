//! Exact command_rasters operations of the parent owner; no duplicated state.
use super::*;

pub(super) unsafe fn append_rasters(instance: *mut ProgramInstance, count: usize) -> bool {
    unsafe {
        let gui = gui(instance);
        if gui.is_null() {
            return false;
        }
        let mut frame = (*gui).building_frame;
        if frame.is_null() {
            frame = gs::allocate_frame((*instance).id, false);
            if frame.is_null() {
                return false;
            }
            life::set_building(gui, frame);
        }
        for index in 0..count {
            let Some(mut render) = gs::prepare_blit(instance, frame, PIXELS, 2) else {
                return false;
            };
            let words = slice::from_raw_parts_mut(
                resource_data(render.resource.payload).cast::<u32>(),
                (*render.resource.payload).byte_count as usize / 4,
            );
            words.fill(u32::try_from(index + 1).expect("Raster test word"));
            let mut command = command(4);
            command.w = 128;
            command.h = 128;
            command.flags = 1;
            command.resource_kind = 2;
            command.payload_offset = (*render.resource.payload).logical_offset;
            command.payload_bytes = (*render.resource.payload).byte_count.into();
            command.raster_word_offset = (*render.resource.payload).raster_word_offset;
            gs::commit_blit(&mut render, command);
        }
        true
    }
}

pub(super) unsafe fn raster_growth(id: u32, heap: &heap::Stats, base: &t::Stats) -> bool {
    unsafe {
        let Some(mut storage) = storage::allocate(id, 1, false) else {
            return false;
        };
        let mut fake = fake(id, 1, storage);
        let appended = append_rasters(&mut fake, 6);
        let gui = gui(&fake);
        if gui.is_null() {
            return false;
        }
        let frame = (*gui).building_frame;
        if frame.is_null() {
            return false;
        }
        let expected = 6 * PIXELS;
        let mut crossing = [0u32; 8];
        let count = view::rasters(frame, (PIXELS - 4) as u64, crossing.as_mut_ptr(), 8);
        let crossing_valid = count == 8
            && crossing[..4].iter().all(|&w| w == 1)
            && crossing[4..].iter().all(|&w| w == 2);
        let safe = view::raster_count(u32::MAX.into(), u32::MAX) == i32::MAX as usize;
        let full = appended
            && (*frame).command_count == 6
            && (*frame).raster_words == expected as u64
            && expected >= 256 * 384
            && crossing_valid
            && safe
            && life::valid(frame, id)
            && accounting::at_peak(gui);
        life::set_building(gui, ptr::null_mut());
        let cleared = life::release_frame(frame, id);
        let retried = append_rasters(&mut fake, 1);
        let retry = (*gui).building_frame;
        if retry.is_null() {
            return false;
        }
        let head = (*retry).resource_payload;
        let valid = retried
            && (*retry).command_count == 1
            && (*retry).raster_words == PIXELS as u64
            && !head.is_null()
            && (*head).logical_offset == 0
            && (*retry).resource_tail == head
            && life::valid(retry, id)
            && accounting::checked(gui).is_some();
        storage::rollback(id, &mut storage);
        full && cleared && valid && baseline(heap, base)
    }
}

pub(super) unsafe fn raster_growth_failure(id: u32, heap: &heap::Stats, base: &t::Stats) -> bool {
    unsafe {
        let Some(mut storage) = storage::allocate(id, 1, false) else {
            return false;
        };
        let mut fake = fake(id, 1, storage);
        if !append_rasters(&mut fake, 2) {
            storage::rollback(id, &mut storage);
            return false;
        }
        let gui = gui(&fake);
        if gui.is_null() {
            return false;
        }
        let frame = (*gui).building_frame;
        if frame.is_null() {
            return false;
        }
        let old_commands = (*frame).command_count;
        let old_words = (*frame).raster_words;
        let old_tail = (*frame).resource_tail;
        fail_after(Some(0));
        let rejected = gs::prepare_blit(&mut fake, frame, PIXELS, 2).is_none();
        fail_after(None);
        let unchanged = rejected
            && (*frame).command_count == old_commands
            && (*frame).raster_words == old_words
            && (*frame).resource_tail == old_tail
            && life::valid(frame, id)
            && accounting::at_peak(gui);
        storage::rollback(id, &mut storage);
        unchanged && baseline(heap, base)
    }
}

pub(super) unsafe fn dynamic_commands(id: u32, heap: &heap::Stats, base: &t::Stats) -> bool {
    unsafe {
        let Some(mut storage) = storage::allocate(id, 1, false) else {
            return false;
        };
        let mut fake = fake(id, 1, storage);
        if flow::begin(&mut fake, 0, &[]) != 0 || flow::commit(&mut fake) != 0 {
            storage::rollback(id, &mut storage);
            return false;
        }
        let gui = gui(&fake);
        let empty = (*gui).committed_frame;
        if empty.is_null() {
            return false;
        }
        if (*empty).command_count != 0
            || !life::valid(empty, id)
            || flow::begin(&mut fake, 0, &[]) != 0
        {
            storage::rollback(id, &mut storage);
            return false;
        }
        let building = (*gui).building_frame;
        if building.is_null() {
            return false;
        }
        let peak =
            (*gui).frame_peak_bytes >= life::frame_bytes(empty) + life::frame_bytes(building);
        for index in 0..4096 {
            let Some(mut prepared) = gs::prepare_command(&mut fake, building) else {
                storage::rollback(id, &mut storage);
                return false;
            };
            let mut value = command(c::gui_frame_command_kind_rect);
            value.rgb = index;
            gs::commit_command(&mut prepared, value);
        }
        let mut boundaries = (*building).command_count == 4096;
        for index in [0u64, 1, 256, 4095] {
            let value = view::r4os_rust_program_gui_view_command(building, index, false);
            boundaries = boundaries && !value.is_null() && (*value).rgb == index as u32;
        }
        boundaries = boundaries
            && view::r4os_rust_program_gui_view_command(building, 4096, false).is_null()
            && peak
            && accounting::at_peak(gui)
            && life::valid(building, id);
        let final_peak = accounting::peak(gui);
        let committed = boundaries
            && flow::commit(&mut fake) == 0
            && (*(*gui).committed_frame).command_count == 4096
            && accounting::checked(gui).is_some()
            && accounting::peak(gui) == final_peak;
        storage::rollback(id, &mut storage);
        committed && baseline(heap, base)
    }
}

pub(super) unsafe fn hash(frame: *const t::Frame) -> u64 {
    unsafe {
        fn mix(hash: u64, value: u64) -> u64 {
            (hash ^ value).wrapping_mul(0x100000001b3)
        }
        let mut hash = 0xcbf29ce484222325;
        hash = mix(hash, (*frame).generation);
        hash = mix(hash, (*frame).command_count);
        hash = mix(hash, (*frame).resource_len);
        hash = mix(hash, (*frame).node_sequence);
        let mut node = (*frame).command_payload;
        while !node.is_null() {
            hash = mix(hash, (*node).allocation_sequence);
            for command in slice::from_raw_parts(commands(node), (*node).command_count as usize) {
                hash = mix(hash, command.kind.into());
                hash = mix(hash, command.rgb.into());
                hash = mix(hash, command.payload_offset);
                hash = mix(hash, command.payload_bytes);
            }
            node = (*node).next;
        }
        let mut node = (*frame).resource_payload;
        while !node.is_null() {
            hash = mix(hash, (*node).allocation_sequence);
            for &byte in slice::from_raw_parts(resource_data(node), (*node).byte_count as usize) {
                hash = mix(hash, byte.into());
            }
            node = (*node).next;
        }
        hash
    }
}

pub(super) unsafe fn retired_reader(id: u32, heap: &heap::Stats, base: &t::Stats) -> bool {
    unsafe {
        let Some(mut storage) = storage::allocate(id, 1, false) else {
            return false;
        };
        let mut fake = fake(id, 1, storage);
        let first = flow::replace(&mut fake, true);
        if first.is_null() {
            return false;
        }
        let Some(mut prepared) = gs::prepare_command(&mut fake, first) else {
            return false;
        };
        let mut value = command(c::gui_frame_command_kind_rect);
        value.rgb = 1;
        gs::commit_command(&mut prepared, value);
        if flow::commit(&mut fake) != 0 {
            return false;
        }
        let Some(mut capture) = flow::capture(&mut fake, None, false) else {
            return false;
        };
        let generation = capture.generation;
        let old_hash = hash(capture.frame);
        if flow::begin(&mut fake, 0, &[]) != 0 {
            return false;
        }
        let gui = gui(&fake);
        let second = (*gui).building_frame;
        if second.is_null() {
            return false;
        }
        let Some(mut prepared) = gs::prepare_command(&mut fake, second) else {
            return false;
        };
        let mut value = command(c::gui_frame_command_kind_rect);
        value.rgb = 2;
        gs::commit_command(&mut prepared, value);
        if flow::commit(&mut fake) != 0 {
            return false;
        }
        let valid = (*gui).retired_frames == capture.frame
            && (*capture.frame).retired
            && (*capture.frame).reader_refs == 1
            && (*capture.frame).generation == generation
            && hash(capture.frame) == old_hash
            && flow::capture(&mut fake, Some(generation), false).is_none()
            && accounting::at_peak(gui);
        let final_peak = accounting::peak(gui);
        flow::release_capture(&mut fake, &mut capture);
        let released = (*gui).retired_frames.is_null()
            && accounting::checked(gui).is_some()
            && accounting::peak(gui) == final_peak;
        storage::rollback(id, &mut storage);
        valid && released && baseline(heap, base)
    }
}

pub(super) unsafe fn batch(
    instance: *mut ProgramInstance,
    commands: &[c::GuiFrameCommand],
    resources: &[u8],
) -> i32 {
    unsafe {
        let gui = gui(instance);
        let Some(before) = accounting::checked(gui) else {
            return c::gui_frame_error_state;
        };
        let peak = accounting::peak(gui);
        let result = gui_batch::r4os_rust_program_gui_batch(
            instance,
            commands.as_ptr(),
            commands.len(),
            span(resources),
        );
        if !accounting::after_append(gui, peak)
            || (result != c::gui_frame_result_ok && accounting::checked(gui) != Some(before))
        {
            log(b"[PINSTSTOR] batch accounting mismatch\r\n");
            return c::gui_frame_error_state;
        }
        result
    }
}

pub(super) unsafe fn at(frame: *const t::Frame, index: u64) -> *const t::GuiCommand {
    unsafe { view::r4os_rust_program_gui_view_command(frame, index, false) }
}

pub(super) unsafe fn legacy(frame: *const t::Frame, index: u64, out: *mut c::GuiCommand) -> bool {
    unsafe {
        view::legacy(
            view::Location {
                frame,
                command: at(frame, index),
                resource_base: 0,
                raster_base: 0,
            },
            out,
        )
    }
}

pub(super) unsafe fn external(frame: *const t::Frame, index: u64) -> c::GuiFrameCommand {
    unsafe { view::external(at(frame, index), 0) }
}

pub(super) unsafe fn bytes_of<T>(value: &T) -> &[u8] {
    unsafe { slice::from_raw_parts(ptr::from_ref(value).cast(), mem::size_of::<T>()) }
}
