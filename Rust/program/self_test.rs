//! Original program-storage/GUI diagnostic cases, with unchanged failure seams,
//! expected values, rollback ordering and ownership baselines. No new test suite.
use super::{
    gui_batch, gui_draw, gui_events, gui_flow as flow, gui_lifetime as life, gui_storage as gs,
    gui_view as view, instance_storage as storage, memory, payload, payload_types as t,
    process_data, registry_types::ProgramInstance, shared_raster, threads,
};
use crate::{
    arch::runtime,
    boot::foundation,
    contract as c,
    fs::vfs_types::Bytes,
    memory::{blocks, heap, owner::Guard},
};
use core::{
    mem::{self, MaybeUninit},
    ptr, slice,
};
const PIXELS: usize = 128 * 128;
fn state() -> *mut payload::State {
    payload::r4os_rust_program_payload_state()
}
fn shared() -> *mut shared_raster::Debug {
    shared_raster::r4os_rust_program_shared_debug()
}
fn span(s: &[u8]) -> Bytes {
    Bytes {
        data: s.as_ptr(),
        len: s.len(),
    }
}
fn log(s: &[u8]) {
    foundation::puts(s)
}
fn dec(n: u64) {
    foundation::dec(n)
}
unsafe fn heap_stats() -> heap::Stats {
    unsafe {
        let mut out = MaybeUninit::uninit();
        heap::r4os_rust_heap_stats(out.as_mut_ptr());
        out.assume_init()
    }
}
unsafe fn stats() -> t::Stats {
    unsafe {
        let mut out = MaybeUninit::uninit();
        payload::r4os_rust_program_payload_stats(out.as_mut_ptr());
        out.assume_init()
    }
}
fn fail_after(value: Option<u32>) {
    unsafe {
        payload::r4os_rust_program_payload_failure(value.is_some(), value.unwrap_or(0));
    }
}
unsafe fn fake(id: u32, class: u8, storage: t::InstanceStorage) -> ProgramInstance {
    unsafe {
        let mut instance: ProgramInstance = mem::zeroed();
        instance.id = id;
        instance.app_class = class;
        instance.memory_profile = 2;
        instance.memory_limits = memory::Limits {
            vm_reserve_limit: 1024 * 1024 * 1024,
            vm_commit_limit: 256 * 1024 * 1024,
            resident_limit: 256 * 1024 * 1024,
            stack_reserve: 4 * 1024 * 1024,
            stack_initial_commit: 64 * 1024,
        };
        instance.runtime_payload = storage.runtime.cast();
        instance.process_payload = storage.process.cast();
        instance.console_payload = storage.console.cast();
        instance.gui_payload = storage.gui.cast();
        instance
    }
}
unsafe fn gui(instance: *const ProgramInstance) -> *mut t::Gui {
    unsafe { (*instance).gui_payload.cast() }
}
unsafe fn commands(payload: *mut t::Commands) -> *mut t::GuiCommand {
    unsafe {
        payload::r4os_rust_program_payload_span(0, payload.cast())
            .data
            .cast()
    }
}
unsafe fn resource_data(payload: *mut t::Resource) -> *mut u8 {
    unsafe {
        payload::r4os_rust_program_payload_span(1, payload.cast())
            .data
            .cast()
    }
}
fn command(kind: u32) -> t::GuiCommand {
    let mut c: t::GuiCommand = unsafe { mem::zeroed() };
    c.version = 1;
    c.size = 96;
    c.kind = kind;
    c
}
fn heap_equal(a: &heap::Stats, b: &heap::Stats) -> bool {
    a.used_bytes == b.used_bytes
        && a.active_blocks == b.active_blocks
        && a.free_blocks == b.free_blocks
        && a.invalid_free_errors == b.invalid_free_errors
        && a.double_free_errors == b.double_free_errors
        && a.size_mismatch_errors == b.size_mismatch_errors
        && a.reentry_errors == b.reentry_errors
}
fn storage_equal(a: &t::Stats, b: &t::Stats) -> bool {
    a.live_core_bytes == b.live_core_bytes
        && a.active_instance_bytes == b.active_instance_bytes
        && a.reserved_instance_bytes == b.reserved_instance_bytes
        && a.current_payload_bytes == b.current_payload_bytes
        && a.current_runtime_bytes == b.current_runtime_bytes
        && a.current_console_bytes == b.current_console_bytes
        && a.current_gui_bytes == b.current_gui_bytes
        && a.active_instances == b.active_instances
        && a.active_service_instances == b.active_service_instances
        && a.active_console_instances == b.active_console_instances
        && a.active_gui_instances == b.active_gui_instances
        && a.active_runtime_payloads == b.active_runtime_payloads
        && a.active_process_payloads == b.active_process_payloads
        && a.active_environment_payloads == b.active_environment_payloads
        && a.active_console_payloads == b.active_console_payloads
        && a.active_console_output_payloads == b.active_console_output_payloads
        && a.active_gui_payloads == b.active_gui_payloads
        && a.active_gui_frame_payloads == b.active_gui_frame_payloads
        && a.active_gui_command_payloads == b.active_gui_command_payloads
        && a.active_gui_raster_payloads == b.active_gui_raster_payloads
        && a.active_gui_data_payloads == b.active_gui_data_payloads
        && a.current_gui_frame_bytes == b.current_gui_frame_bytes
        && a.current_gui_frame_commands == b.current_gui_frame_commands
        && a.current_gui_frame_nodes == b.current_gui_frame_nodes
        && a.owner_mismatches == b.owner_mismatches
        && a.header_errors == b.header_errors
        && a.free_failures == b.free_failures
        && a.quarantined_payloads == b.quarantined_payloads
        && a.quarantined_bytes == b.quarantined_bytes
}
unsafe fn baseline(heap: &heap::Stats, storage: &t::Stats) -> bool {
    unsafe { heap_equal(heap, &heap_stats()) && storage_equal(storage, &stats()) }
}
unsafe fn warm() -> bool {
    unsafe {
        let Some(mut console) = storage::allocate(0xffff_ff01, 0, true) else {
            return false;
        };
        storage::rollback(0xffff_ff01, &mut console);
        let Some(mut storage) = storage::allocate(0xffff_ff02, 1, false) else {
            return false;
        };
        let mut fake = fake(0xffff_ff02, 1, storage);
        let frame = gs::allocate_frame(fake.id, false);
        if frame.is_null() {
            storage::rollback(fake.id, &mut storage);
            return false;
        }
        (*gui(&fake)).building_frame = frame;
        let Some(mut render) = gs::prepare_blit(&mut fake, frame, PIXELS, 2) else {
            storage::rollback(fake.id, &mut storage);
            return false;
        };
        gs::cancel_blit(&mut fake, &mut render);
        if gui_draw::text(
            &mut fake,
            0,
            0,
            c"warm".as_ptr().cast(),
            0x00ff_ffff,
            0,
            0,
            0,
        ) < 0
            || !append_rasters(&mut fake, 6)
        {
            storage::rollback(fake.id, &mut storage);
            return false;
        }
        storage::rollback(fake.id, &mut storage);
        true
    }
}
unsafe fn failed(case_id: u32, heap: &heap::Stats, storage: &t::Stats) -> bool {
    unsafe {
        let heap_ok = heap_equal(heap, &heap_stats());
        let storage_ok = storage_equal(storage, &stats());
        (*state()).report = t::SelfTestReport {
            cases: case_id,
            failed_case: case_id,
            peak_payload_bytes: (*state()).self_test_peak_bytes,
            heap_baseline_ok: heap_ok,
            storage_baseline_ok: storage_ok,
            zero_init_ok: false,
        };
        log(b"[PINSTSTOR] result=FAILED case=");
        dec(case_id.into());
        log(b" heap_baseline=");
        log(if heap_ok { b"ok" } else { b"failed" });
        log(b" storage_baseline=");
        log(if storage_ok { b"ok" } else { b"failed" });
        if !heap_ok {
            let after = heap_stats();
            log(b" used=");
            dec(heap.used_bytes as u64);
            log(b"/");
            dec(after.used_bytes as u64);
            log(b" blocks=");
            dec(heap.active_blocks as u64);
            log(b"/");
            dec(after.active_blocks as u64);
            log(b" committed=");
            dec(heap.committed_bytes as u64);
            log(b"/");
            dec(after.committed_bytes as u64);
            log(b" free-blocks=");
            dec(heap.free_blocks as u64);
            log(b"/");
            dec(after.free_blocks as u64);
        }
        log(b"\r\n");
        false
    }
}
unsafe fn lazy(id: u32, which: u8, fail: u32, heap: &heap::Stats, base: &t::Stats) -> bool {
    unsafe {
        let class = if which <= 1 { 2 } else { 1 };
        let Some(mut storage) = storage::allocate(id, class, false) else {
            return false;
        };
        let mut fake = fake(id, class, storage);
        let frame = if which == 3 {
            let frame = gs::allocate_frame(id, false);
            if frame.is_null() {
                storage::rollback(id, &mut storage);
                return false;
            }
            (*gui(&fake)).building_frame = frame;
            frame
        } else {
            ptr::null_mut()
        };
        fail_after(Some(fail));
        let failed = match which {
            0 => process_data::ensure_environment(&mut fake).is_null(),
            1 => {
                let value = process_data::ensure_gui(&mut fake).cast::<t::Gui>();
                if !value.is_null() {
                    storage.gui = value;
                }
                value.is_null()
            }
            2 => flow::ensure(&mut fake).is_null(),
            3 => {
                let prepared = gs::prepare_blit(&mut fake, frame, 1, 2);
                let failed = prepared.is_none();
                if let Some(mut render) = prepared {
                    gs::cancel_blit(&mut fake, &mut render);
                }
                failed
            }
            _ => unreachable!(),
        };
        fail_after(None);
        storage::rollback(id, &mut storage);
        failed && baseline(heap, base)
    }
}
unsafe fn zero_initialization(id: u32, heap: &heap::Stats, base: &t::Stats) -> bool {
    unsafe {
        let Some(mut first) = storage::allocate(id, 1, false) else {
            return false;
        };
        let mut first_fake = fake(id, 1, first);
        let environment =
            process_data::ensure_environment(&mut first_fake).cast::<t::Environment>();
        if environment.is_null() {
            storage::rollback(id, &mut first);
            return false;
        }
        let frame = gs::allocate_frame(id, false);
        if frame.is_null() {
            storage::rollback(id, &mut first);
            return false;
        }
        (*gui(&first_fake)).building_frame = frame;
        let Some(mut render) = gs::prepare_blit(&mut first_fake, frame, 1, 2) else {
            storage::rollback(id, &mut first);
            return false;
        };
        (*environment).bytes[0] = 0xa5;
        (*commands(render.command.payload)).kind = 0xa5a5_a5a5;
        resource_data(render.resource.payload)
            .cast::<u32>()
            .write(0xa5a5_a5a5);
        gs::cancel_blit(&mut first_fake, &mut render);
        storage::rollback(id, &mut first);
        let Some(mut second) = storage::allocate(id + 1, 1, false) else {
            return false;
        };
        let mut second_fake = fake(id + 1, 1, second);
        let environment =
            process_data::ensure_environment(&mut second_fake).cast::<t::Environment>();
        if environment.is_null() {
            storage::rollback(id + 1, &mut second);
            return false;
        }
        let frame = gs::allocate_frame(id + 1, false);
        if frame.is_null() {
            storage::rollback(id + 1, &mut second);
            return false;
        }
        (*gui(&second_fake)).building_frame = frame;
        let Some(mut render) = gs::prepare_blit(&mut second_fake, frame, 1, 2) else {
            storage::rollback(id + 1, &mut second);
            return false;
        };
        let zeroed = (*environment).bytes[0] == 0
            && (*commands(render.command.payload)).kind == 0
            && resource_data(render.resource.payload).cast::<u32>().read() == 0;
        gs::cancel_blit(&mut second_fake, &mut render);
        storage::rollback(id + 1, &mut second);
        zeroed && baseline(heap, base)
    }
}
unsafe fn append_rasters(instance: *mut ProgramInstance, count: usize) -> bool {
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
            (*gui).building_frame = frame;
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
unsafe fn raster_growth(id: u32, heap: &heap::Stats, base: &t::Stats) -> bool {
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
            && life::valid(frame, id);
        (*gui).building_frame = ptr::null_mut();
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
            && life::valid(retry, id);
        storage::rollback(id, &mut storage);
        full && cleared && valid && baseline(heap, base)
    }
}
unsafe fn raster_growth_failure(id: u32, heap: &heap::Stats, base: &t::Stats) -> bool {
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
            && life::valid(frame, id);
        storage::rollback(id, &mut storage);
        unchanged && baseline(heap, base)
    }
}
unsafe fn dynamic_commands(id: u32, heap: &heap::Stats, base: &t::Stats) -> bool {
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
            && (*gui).frame_peak_bytes >= life::owned(gui, false)
            && life::valid(building, id);
        let committed = boundaries
            && flow::commit(&mut fake) == 0
            && (*(*gui).committed_frame).command_count == 4096;
        storage::rollback(id, &mut storage);
        committed && baseline(heap, base)
    }
}
unsafe fn hash(frame: *const t::Frame) -> u64 {
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
unsafe fn retired_reader(id: u32, heap: &heap::Stats, base: &t::Stats) -> bool {
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
            && (*gui).frame_peak_bytes >= life::owned(gui, false);
        flow::release_capture(&mut fake, &mut capture);
        let released = (*gui).retired_frames.is_null();
        storage::rollback(id, &mut storage);
        valid && released && baseline(heap, base)
    }
}
unsafe fn batch(
    instance: *mut ProgramInstance,
    commands: &[c::GuiFrameCommand],
    resources: &[u8],
) -> i32 {
    unsafe {
        gui_batch::r4os_rust_program_gui_batch(
            instance,
            commands.as_ptr(),
            commands.len(),
            span(resources),
        )
    }
}
unsafe fn at(frame: *const t::Frame, index: u64) -> *const t::GuiCommand {
    unsafe { view::r4os_rust_program_gui_view_command(frame, index, false) }
}
unsafe fn legacy(frame: *const t::Frame, index: u64, out: *mut c::GuiCommand) -> bool {
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
unsafe fn external(frame: *const t::Frame, index: u64) -> c::GuiFrameCommand {
    unsafe { view::external(at(frame, index), 0) }
}
unsafe fn bytes_of<T>(value: &T) -> &[u8] {
    unsafe { slice::from_raw_parts(ptr::from_ref(value).cast(), mem::size_of::<T>()) }
}
unsafe fn mixed_resources(id: u32, heap: &heap::Stats, base: &t::Stats) -> bool {
    unsafe {
        let Some(mut storage) = storage::allocate(id, 1, false) else {
            return false;
        };
        let mut fake = fake(id, 1, storage);
        let frame = flow::replace(&mut fake, true);
        if frame.is_null() {
            storage::rollback(id, &mut storage);
            return false;
        }
        let expected_alpha = [1u8, 2, 3, 4, 5, 6];
        let mut resources = [0u8; 171];
        for (i, byte) in resources[..161].iter_mut().enumerate() {
            *byte = b'A' + (i % 26) as u8;
        }
        resources[161..165].copy_from_slice(&[0x33, 0x22, 0x11, 0]);
        resources[165..].copy_from_slice(&expected_alpha);
        let commands = [
            c::GuiFrameCommand {
                kind: c::gui_frame_command_kind_text,
                resource_bytes: 161,
                ..Default::default()
            },
            c::GuiFrameCommand {
                kind: c::gui_frame_command_kind_raster,
                w: 1,
                h: 1,
                resource_offset: 161,
                resource_bytes: 4,
                parameter0: 3,
                ..Default::default()
            },
            c::GuiFrameCommand {
                kind: c::gui_frame_command_kind_alpha8,
                w: 3,
                h: 2,
                resource_offset: 165,
                resource_bytes: 6,
                ..Default::default()
            },
        ];
        if batch(&mut fake, &commands, &resources) != 0 || !life::valid(frame, id) {
            storage::rollback(id, &mut storage);
            return false;
        }
        let mut copied_text = [0u8; 161];
        let text_roundtrip = view::resources(frame, 0, copied_text.as_mut_ptr(), 161) == 161
            && copied_text == resources[..161];
        let mut legacy_text = c::GuiCommand::default();
        let legacy_prefix = legacy(frame, 0, &mut legacy_text)
            && legacy_text.text[0] == b'A'
            && legacy_text.text[62] == resources[62]
            && legacy_text.text[63] == 0;
        let mut legacy_raster = c::GuiCommand::default();
        let mut legacy_alpha = c::GuiCommand::default();
        let legacy_commands = legacy(frame, 1, &mut legacy_raster)
            && legacy(frame, 2, &mut legacy_alpha)
            && legacy_raster.flags == 3
            && legacy_raster.rgb == 0
            && legacy_alpha.flags == 1
            && legacy_alpha.fg == 1
            && legacy_alpha.bg == 2;
        let mut words = [0u32; 3];
        let raster_roundtrip = view::rasters(frame, 0, words.as_mut_ptr(), 3) == 3
            && words == [0x00112233, 0x04030201, 0x00000605];
        let mut snapshot = [0u8; 171];
        let text = external(frame, 0);
        let raster = external(frame, 1);
        let alpha = external(frame, 2);
        let snapshot_roundtrip = view::resources(frame, 0, snapshot.as_mut_ptr(), 171) == 171
            && text.flags == 0
            && text.resource_offset == 0
            && text.resource_bytes == 161
            && raster.flags == 0
            && raster.resource_offset == 161
            && raster.resource_bytes == 4
            && raster.parameter0 == 3
            && alpha.flags == 0
            && alpha.resource_offset == 165
            && alpha.resource_bytes == 6
            && alpha.parameter0 == 0
            && alpha.parameter1 == 0
            && snapshot[161..165] == [0x33, 0x22, 0x11, 0]
            && snapshot[165..] == expected_alpha;
        let clone = flow::clone(&mut fake, frame);
        if clone.is_null() {
            return false;
        }
        let clone_ok = (*clone).raster_words == (*frame).raster_words
            && life::valid(clone, id)
            && hash(clone) == hash(frame);
        life::release_frame(clone, id);
        let committed = flow::commit(&mut fake) == 0;
        let mut mixed_append = false;
        if committed {
            let building = flow::ensure(&mut fake);
            if !building.is_null()
                && let Some(mut render) = gs::prepare_blit(&mut fake, building, 1, 2)
            {
                resource_data(render.resource.payload)
                    .cast::<u32>()
                    .write(0x0055_4433);
                let mut value = command(c::gui_frame_command_kind_raster);
                value.w = 1;
                value.h = 1;
                value.flags = 1;
                value.resource_kind = 2;
                value.payload_offset = (*render.resource.payload).logical_offset;
                value.payload_bytes = 4;
                value.raster_word_offset = (*render.resource.payload).raster_word_offset;
                value.parameter0 = 1;
                mixed_append = gs::commit_blit(&mut render, value) == 4;
            }
        }
        let gui = gui(&fake);
        let mixed = (*gui).building_frame;
        let mixed_ok = !mixed.is_null()
            && (*mixed).raster_words == 4
            && (*mixed).command_count == 4
            && (*mixed).resource_len == 175
            && (*(*mixed).resource_tail).resource_kind == 2
            && (*(*mixed).resource_tail).raster_word_offset == 3
            && life::valid(mixed, id);
        let mixed_committed =
            mixed_ok && flow::commit(&mut fake) == 0 && (*(*gui).committed_frame).raster_words == 4;
        let mut reclone_append = false;
        if mixed_committed {
            let building = flow::ensure(&mut fake);
            if !building.is_null()
                && life::valid(building, id)
                && (*(*building).resource_tail).raster_word_offset == 3
                && let Some(mut render) = gs::prepare_blit(&mut fake, building, 1, 2)
            {
                resource_data(render.resource.payload)
                    .cast::<u32>()
                    .write(0x0066_5544);
                let mut value = command(c::gui_frame_command_kind_raster);
                value.w = 1;
                value.h = 1;
                value.flags = 1;
                value.resource_kind = 2;
                value.payload_offset = (*render.resource.payload).logical_offset;
                value.payload_bytes = 4;
                value.raster_word_offset = (*render.resource.payload).raster_word_offset;
                value.parameter0 = 1;
                reclone_append = gs::commit_blit(&mut render, value) == 5;
            }
        }
        let reclone = (*gui).building_frame;
        let reclone_ok = !reclone.is_null()
            && (*reclone).raster_words == 5
            && (*reclone).command_count == 5
            && (*reclone).resource_len == 179
            && (*(*(*reclone).resource_payload).next).raster_word_offset == 3
            && (*(*reclone).resource_tail).raster_word_offset == 4
            && life::valid(reclone, id);
        let reclone_committed = reclone_ok
            && flow::commit(&mut fake) == 0
            && (*(*gui).committed_frame).raster_words == 5;
        let alpha_frame = flow::replace(&mut fake, true);
        if alpha_frame.is_null() {
            return false;
        }
        let Some(mut alpha) = gs::prepare_blit(&mut fake, alpha_frame, 2, 3) else {
            return false;
        };
        let data = resource_data(alpha.resource.payload);
        ptr::write_bytes(data, 0, (*alpha.resource.payload).byte_count as usize);
        ptr::copy_nonoverlapping(expected_alpha.as_ptr(), data, 6);
        let mut value = command(c::gui_frame_command_kind_alpha8);
        value.w = 3;
        value.h = 2;
        value.resource_kind = 3;
        value.payload_offset = (*alpha.resource.payload).logical_offset;
        value.payload_bytes = 6;
        value.raster_word_offset = (*alpha.resource.payload).raster_word_offset;
        gs::commit_blit(&mut alpha, value);
        let snapshot = external(alpha_frame, 0);
        let mut blob = [0xccu8; 8];
        let mut words = [0u32; 2];
        let sequence_ok = (*(*alpha_frame).command_tail).allocation_sequence == 1
            && (*(*alpha_frame).resource_tail).allocation_sequence == 2
            && (*alpha_frame).node_sequence == 2;
        let alpha_ok = life::valid(alpha_frame, id)
            && (*alpha_frame).resource_len == 8
            && snapshot.resource_offset == 0
            && snapshot.resource_bytes == 6
            && snapshot.parameter0 == 0
            && snapshot.parameter1 == 0
            && view::resources(alpha_frame, 0, blob.as_mut_ptr(), 8) == 8
            && blob[..6] == expected_alpha
            && blob[6] == 0
            && blob[7] == 0
            && view::rasters(alpha_frame, 0, words.as_mut_ptr(), 2) == 2
            && words == [0x04030201, 0x00000605];
        (*state()).release_trace_len = 0;
        (*state()).release_trace_enabled = true;
        let cancelled = flow::cancel(&mut fake) == 0;
        (*state()).release_trace_enabled = false;
        let reverse_ok =
            (*state()).release_trace_len == 3 && (&(*state()).release_trace)[..3] == [8, 7, 9];
        let shape_frame = flow::replace(&mut fake, true);
        if shape_frame.is_null() {
            return false;
        }
        let header = c::GuiShapeResource {
            geometry_kind: c::gui_shape_geometry_kind_path,
            segment_count: 2,
            fill_rule: c::gui_shape_fill_rule_nonzero,
            line_join: c::gui_shape_line_join_miter,
            line_cap: c::gui_shape_line_cap_square,
            stroke_argb: 0xff112233,
            stroke_width_bits: 2f32.to_bits(),
            miter_limit_bits: 4f32.to_bits(),
            ..Default::default()
        };
        let segments = [
            c::GuiPathSegment {
                kind: c::gui_path_segment_kind_move,
                x1_bits: 1f32.to_bits(),
                y1_bits: 2f32.to_bits(),
                ..Default::default()
            },
            c::GuiPathSegment {
                kind: c::gui_path_segment_kind_line,
                x1_bits: 20f32.to_bits(),
                y1_bits: 9f32.to_bits(),
                ..Default::default()
            },
        ];
        const SHAPE: usize = mem::size_of::<c::GuiShapeResource>();
        let mut shape = [0u8; SHAPE + 2 * mem::size_of::<c::GuiPathSegment>()];
        shape[..SHAPE].copy_from_slice(bytes_of(&header));
        shape[SHAPE..].copy_from_slice(bytes_of(&segments));
        let value = c::GuiFrameCommand {
            kind: c::gui_frame_command_kind_path_stroke,
            w: 32,
            h: 16,
            resource_bytes: shape.len() as u64,
            ..Default::default()
        };
        let shape_ok = batch(&mut fake, &[value], &shape) == 0
            && life::valid(shape_frame, id)
            && (*shape_frame).command_count == 1
            && (*shape_frame).resource_len == shape.len() as u64
            && (*at(shape_frame, 0)).resource_kind == 4;
        let mut truncated = value;
        truncated.resource_bytes -= 1;
        let mut offset = value;
        offset.resource_offset = 1;
        let mut oversized = value;
        oversized.w = c::gui_shape_max_dimension + 1;
        let mut invalid_kind = shape;
        invalid_kind[SHAPE + mem::offset_of!(c::GuiPathSegment, kind)] = 99;
        let mut non_finite = shape;
        let coordinate = SHAPE + mem::offset_of!(c::GuiPathSegment, x1_bits);
        non_finite[coordinate..coordinate + 4].copy_from_slice(&[0, 0, 0x80, 0x7f]);
        let shape_negative = !super::gui_resources::batch(&truncated, &shape)
            && !super::gui_resources::batch(&offset, &shape)
            && !super::gui_resources::batch(&oversized, &shape)
            && !super::gui_resources::batch(&value, &invalid_kind)
            && !super::gui_resources::batch(&value, &non_finite);
        let shape_cancelled = flow::cancel(&mut fake) == 0;
        let argb_frame = flow::replace(&mut fake, true);
        if argb_frame.is_null() {
            return false;
        }
        let argb = [0x33u8, 0x22, 0x11, 0x80, 0x66, 0x55, 0x44, 0xff];
        let value = c::GuiFrameCommand {
            kind: c::gui_frame_command_kind_argb32,
            w: 2,
            h: 1,
            resource_bytes: argb.len() as u64,
            parameter0: 2,
            ..Default::default()
        };
        let argb_ok = batch(&mut fake, &[value], &argb) == 0
            && life::valid(argb_frame, id)
            && (*argb_frame).command_count == 1
            && (*argb_frame).raster_words == 0
            && (*at(argb_frame, 0)).resource_kind == 4;
        let mut legacy_argb = c::GuiCommand::default();
        let mut invalid_size = value;
        invalid_size.resource_bytes -= 1;
        let mut invalid_scale = value;
        invalid_scale.parameter0 = 0;
        let mut invalid_width = value;
        invalid_width.w = c::gui_argb32_max_width + 1;
        let argb_negative = !legacy(argb_frame, 0, &mut legacy_argb)
            && !super::gui_resources::batch(&invalid_size, &argb)
            && !super::gui_resources::batch(&invalid_scale, &argb)
            && !super::gui_resources::batch(&invalid_width, &argb);
        let argb_cancelled = flow::cancel(&mut fake) == 0;
        let invalid_header = c::GuiFrameInfo {
            version: 0,
            size: c::gui_frame_info_size,
            last_error: 0x12345678,
            ..Default::default()
        };
        let before = invalid_header;
        let invalid_ok = !view::valid_info(&invalid_header) && info_equal(&invalid_header, &before);
        let future = c::GuiFrameInfo {
            version: c::gui_frame_info_version + 1,
            size: c::gui_frame_info_size,
            ..Default::default()
        };
        let future_ok = view::valid_info(&future);
        let valid = text_roundtrip
            && legacy_prefix
            && legacy_commands
            && raster_roundtrip
            && snapshot_roundtrip
            && clone_ok
            && mixed_append
            && mixed_committed
            && reclone_append
            && reclone_committed
            && sequence_ok
            && alpha_ok
            && cancelled
            && reverse_ok
            && shape_ok
            && shape_negative
            && shape_cancelled
            && argb_ok
            && argb_negative
            && argb_cancelled
            && invalid_ok
            && future_ok;
        storage::rollback(id, &mut storage);
        valid && baseline(heap, base)
    }
}
unsafe fn atomic_growth_failure(id: u32, heap: &heap::Stats, base: &t::Stats) -> bool {
    unsafe {
        let Some(mut storage) = storage::allocate(id, 1, false) else {
            return false;
        };
        let mut fake = fake(id, 1, storage);
        let initial = flow::replace(&mut fake, true);
        if initial.is_null() {
            return false;
        }
        let Some(mut prepared) = gs::prepare_command(&mut fake, initial) else {
            return false;
        };
        let mut value = command(c::gui_frame_command_kind_clear);
        value.rgb = 0x123456;
        gs::commit_command(&mut prepared, value);
        if flow::commit(&mut fake) != 0 {
            return false;
        }
        let gui = gui(&fake);
        let committed = (*gui).committed_frame;
        if committed.is_null() {
            return false;
        }
        let generation = (*committed).generation;
        let old_hash = hash(committed);
        for boundary in [0usize, 8, 24, 56, 120, 248] {
            if flow::begin(&mut fake, 0, &[]) != 0 {
                return false;
            }
            let building = (*gui).building_frame;
            if building.is_null() {
                return false;
            }
            for index in 0..boundary {
                let Some(mut prepared) = gs::prepare_command(&mut fake, building) else {
                    return false;
                };
                let mut value = command(c::gui_frame_command_kind_rect);
                value.rgb = index as u32;
                gs::commit_command(&mut prepared, value);
            }
            fail_after(Some(0));
            let rejected = gs::prepare_command(&mut fake, building).is_none();
            fail_after(None);
            flow::failed(gui, building);
            if !rejected
                || flow::commit(&mut fake) != c::gui_frame_error_state
                || (*gui).committed_frame != committed
                || (*committed).generation != generation
                || hash(committed) != old_hash
                || flow::cancel(&mut fake) != 0
            {
                storage::rollback(id, &mut storage);
                return false;
            }
        }
        const BYTES: usize = 64 * 1024 + 1;
        let resource = heap::r4os_rust_heap_alloc(BYTES, 1);
        if resource.is_null() {
            storage::rollback(id, &mut storage);
            return false;
        }
        ptr::write_bytes(resource, 0, BYTES);
        let mut commands = [c::GuiFrameCommand {
            kind: c::gui_frame_command_kind_rect,
            ..Default::default()
        }; 9];
        commands[8] = c::GuiFrameCommand {
            kind: c::gui_frame_command_kind_text,
            resource_bytes: BYTES as u64,
            ..Default::default()
        };
        let mut batches_ok = true;
        for failure in 0..5 {
            let iteration_heap = heap_stats();
            let iteration_storage = stats();
            if flow::begin(&mut fake, 0, &[]) != 0 {
                batches_ok = false;
                break;
            }
            fail_after(Some(failure));
            let rejected = batch(&mut fake, &commands, slice::from_raw_parts(resource, BYTES))
                == c::gui_frame_error_oom;
            fail_after(None);
            let failed_build =
                !(*gui).building_frame.is_null() && (*(*gui).building_frame).build_failed;
            let old_unchanged = flow::commit(&mut fake) == c::gui_frame_error_state
                && (*gui).committed_frame == committed
                && (*committed).generation == generation
                && hash(committed) == old_hash;
            let cancelled = flow::cancel(&mut fake) == 0;
            if !rejected
                || !failed_build
                || !old_unchanged
                || !cancelled
                || !baseline(&iteration_heap, &iteration_storage)
            {
                batches_ok = false;
                break;
            }
        }
        fail_after(None);
        if !(*gui).building_frame.is_null() {
            flow::cancel(&mut fake);
        }
        let resource_released = heap::r4os_rust_heap_free(resource, BYTES) == 0;
        let old_ready =
            gui_events::r4os_rust_program_gui_text(&mut fake, c"old".as_ptr().cast()) == 3;
        let text_committed = (*gui).committed_frame;
        if text_committed.is_null() {
            return false;
        }
        let text_generation = (*text_committed).generation;
        let text_hash = hash(text_committed);
        fail_after(Some(0));
        let text_rejected =
            gui_events::r4os_rust_program_gui_text(&mut fake, c"new".as_ptr().cast()) == -2;
        fail_after(None);
        let text_atomic = old_ready
            && text_rejected
            && (*gui).building_frame.is_null()
            && (*gui).committed_frame == text_committed
            && (*text_committed).generation == text_generation
            && hash(text_committed) == text_hash
            && (&(*gui).text)[..3] == *b"old"
            && (*gui).text[3] == 0;
        storage::rollback(id, &mut storage);
        batches_ok && resource_released && text_atomic && baseline(heap, base)
    }
}
unsafe fn delta_indexed(id: u32, heap: &heap::Stats, baseline_storage: &t::Stats) -> bool {
    unsafe {
        let Some(mut storage) = storage::allocate(id, 1, false) else {
            return false;
        };
        let mut fake = fake(id, 1, storage);
        let valid = (|| {
            if flow::begin(&mut fake, 0, &[]) != 0 {
                return false;
            }
            let commands = [
                c::GuiFrameCommand {
                    kind: c::gui_frame_command_kind_clear,
                    rgb: 0x010203,
                    ..Default::default()
                },
                c::GuiFrameCommand {
                    kind: c::gui_frame_command_kind_text,
                    resource_bytes: 1,
                    ..Default::default()
                },
            ];
            if batch(&mut fake, &commands, b"B") != 0 || flow::commit(&mut fake) != 0 {
                return false;
            }
            let gui = gui(&fake);
            let base = (*gui).committed_frame;
            if base.is_null() {
                return false;
            }
            let generation = (*base).generation;
            let base_bytes = life::frame_bytes(base);
            let damage = [
                c::DisplayDamageRect {
                    x: 0,
                    y: 0,
                    w: 1,
                    h: 1,
                },
                c::DisplayDamageRect {
                    x: 3,
                    y: 3,
                    w: 1,
                    h: 1,
                },
            ];
            if flow::begin(&mut fake, 1, &damage) != 0 {
                return false;
            }
            let mut resource = [0u8; c::gui_indexed8_pixels_offset as usize + 4];
            let header = c::GuiIndexed8Resource {
                source_w: 2,
                source_h: 2,
                guest_w: 2,
                guest_h: 2,
                viewport_w: 4,
                viewport_h: 4,
                pixel_stride: 2,
                ..Default::default()
            };
            resource[..mem::size_of::<c::GuiIndexed8Resource>()].copy_from_slice(bytes_of(&header));
            let palette = c::gui_indexed8_palette_offset as usize;
            resource[palette + 4..palette + 8].copy_from_slice(&[0x33, 0x22, 0x11, 0]);
            resource[c::gui_indexed8_pixels_offset as usize..].copy_from_slice(&[0, 1, 1, 0]);
            let value = c::GuiFrameCommand {
                kind: c::gui_frame_command_kind_indexed8,
                w: 4,
                h: 4,
                resource_bytes: resource.len() as u64,
                ..Default::default()
            };
            if !super::gui_resources::batch(&value, &resource)
                || batch(&mut fake, &[value], &resource) != 0
                || flow::commit(&mut fake) != 0
            {
                return false;
            }
            let delta = (*gui).committed_frame;
            if delta.is_null() {
                return false;
            }
            if (*delta).base_frame != base
                || (*delta).chain_depth != 2
                || (*delta).damage_count != damage.len() as u32
                || (*delta).command_count != 1
                || life::chain_count(delta, 0) != 3
                || life::chain_count(delta, 1) != resource.len() as u64 + 1
                || (*gui).frame_delta_commits != 1
                || (*gui).frame_full_commits != 1
                || (*gui).frame_indexed8_commands != 1
                || (*gui).frame_indexed8_resource_bytes != resource.len() as u64
                || (*gui).frame_avoided_clone_bytes < base_bytes
            {
                return false;
            }
            let mut info = c::GuiFrameGenerationInfo::default();
            view::generation(
                gui,
                c::ProgramProcessHandle {
                    instance_id: id,
                    generation: 1,
                    reserved: 0,
                },
                delta,
                &mut info,
            );
            if info.flags & c::gui_frame_generation_flag_delta == 0
                || info.flags & c::gui_frame_generation_flag_indexed8 == 0
                || info.base_generation != generation
                || info.command_count != 1
                || info.total_command_count != 3
                || info.damage_count != damage.len() as u32
            {
                return false;
            }
            let mut local = [c::GuiFrameCommand::default(); 1];
            let mut full = [c::GuiFrameCommand::default(); 3];
            let mut local_resource = [0u8; c::gui_indexed8_pixels_offset as usize + 4];
            if view::local_commands(delta, 0, local.as_mut_ptr(), 1) != 1
                || view::commands(delta, full.as_mut_ptr(), 3) != 3
                || view::local_resources(
                    delta,
                    0,
                    local_resource.as_mut_ptr(),
                    local_resource.len(),
                ) != local_resource.len()
            {
                return false;
            }
            let local_damage =
                ptr::addr_of!((*delta).damage_regions).cast::<c::DisplayDamageRect>();
            if local[0].kind != c::gui_frame_command_kind_indexed8
                || local[0].resource_offset != 0
                || full[2].resource_offset != 1
                || local_resource != resource
                || !(0..damage.len()).all(|i| {
                    let a = local_damage.add(i).read();
                    let b = damage[i];
                    a.x == b.x && a.y == b.y && a.w == b.w && a.h == b.h
                })
            {
                return false;
            }
            let mut bad = header;
            bad.pixel_stride = 3;
            let mut invalid = resource;
            invalid[..mem::size_of::<c::GuiIndexed8Resource>()].copy_from_slice(bytes_of(&bad));
            if super::gui_resources::batch(&value, &invalid) {
                return false;
            }
            let Some(mut capture) = flow::capture(&mut fake, Some(generation), true) else {
                return false;
            };
            if capture.frame != base || (*base).reader_refs != 1 {
                return false;
            }
            if flow::begin(&mut fake, 0, &[]) != 0 {
                return false;
            }
            let replacement = [c::GuiFrameCommand {
                kind: c::gui_frame_command_kind_clear,
                rgb: 0x0a0b0c,
                ..Default::default()
            }];
            if batch(&mut fake, &replacement, &[]) != 0
                || flow::commit(&mut fake) != 0
                || (*gui).retired_frames != delta
            {
                return false;
            }
            flow::release_capture(&mut fake, &mut capture);
            if !(*gui).retired_frames.is_null() {
                return false;
            }
            true
        })();
        storage::rollback(id, &mut storage);
        valid && baseline(heap, baseline_storage)
    }
}
unsafe fn replacement_xrgb(id: u32, heap: &heap::Stats, base: &t::Stats) -> bool {
    unsafe {
        let Some(mut storage) = storage::allocate(id, 1, false) else {
            return false;
        };
        let mut fake = fake(id, 1, storage);
        let damage = [c::DisplayDamageRect {
            x: 1,
            y: 1,
            w: 1,
            h: 1,
        }];
        let mut resource = [0u8; c::gui_xrgb32_pixels_offset as usize + 4 * mem::size_of::<u32>()];
        let header = c::GuiXrgb32Resource {
            source_w: 2,
            source_h: 2,
            guest_w: 2,
            guest_h: 2,
            viewport_w: 2,
            viewport_h: 2,
            pixel_stride: 2,
            ..Default::default()
        };
        resource[..mem::size_of::<c::GuiXrgb32Resource>()].copy_from_slice(bytes_of(&header));
        let value = c::GuiFrameCommand {
            kind: c::gui_frame_command_kind_xrgb32_nearest,
            w: 2,
            h: 2,
            resource_bytes: resource.len() as u64,
            ..Default::default()
        };
        let mut invalid = resource;
        invalid[c::gui_xrgb32_pixels_offset as usize + 3] = 1;
        if super::gui_resources::batch(&value, &invalid) {
            storage::rollback(id, &mut storage);
            return false;
        }
        let mut capture = None;
        let mut lifecycle_ok = true;
        let gui = gui(&fake);
        for index in 0u32..64 {
            resource[c::gui_xrgb32_pixels_offset as usize] = index as u8;
            let begin = if index == 0 {
                flow::begin(&mut fake, 0, &[])
            } else {
                flow::begin(&mut fake, 2, &damage)
            };
            if begin != 0
                || batch(&mut fake, &[value], &resource) != 0
                || flow::commit(&mut fake) != 0
            {
                lifecycle_ok = false;
                break;
            }
            let committed = (*gui).committed_frame;
            if committed.is_null() {
                lifecycle_ok = false;
                break;
            }
            if !(*committed).base_frame.is_null()
                || (*committed).chain_depth != 1
                || (*committed).replacement != (index != 0)
                || life::chain_count(committed, 0) != 1
                || life::chain_count(committed, 1) != resource.len() as u64
            {
                lifecycle_ok = false;
                break;
            }
            if index == 0 {
                let Some(held) = flow::capture(&mut fake, None, false) else {
                    lifecycle_ok = false;
                    break;
                };
                capture = Some(held);
            } else if index == 1 {
                if (*gui).retired_frames.is_null() || life::live_generations(gui) != 2 {
                    lifecycle_ok = false;
                    break;
                }
                flow::release_capture(&mut fake, capture.as_mut().expect("First frame capture"));
                capture = None;
                if !(*gui).retired_frames.is_null() || life::live_generations(gui) != 1 {
                    lifecycle_ok = false;
                    break;
                }
            }
        }
        if let Some(ref mut held) = capture {
            flow::release_capture(&mut fake, held);
        }
        let latest = (*gui).committed_frame;
        let owner = c::ProgramProcessHandle {
            instance_id: id,
            generation: 77,
            reserved: 0,
        };
        let mut info = c::GuiFrameGenerationInfo::default();
        if !latest.is_null() {
            view::generation(gui, owner, latest, &mut info);
        }
        let mut stream = c::GuiFrameStreamInfo::default();
        view::stream(gui, owner, &mut stream);
        let committed_before_oom = latest;
        fail_after(Some(0));
        let rejected = flow::begin(&mut fake, 2, &damage) == c::gui_frame_error_oom;
        fail_after(None);
        let counters = lifecycle_ok
            && !latest.is_null()
            && (*gui).frame_commits == 64
            && (*gui).frame_full_commits == 64
            && (*gui).frame_replacement_commits == 63
            && (*gui).frame_superseded_generations == 63
            && (*gui).frame_coalesced_generations == 63
            && (*gui).frame_reader_retired_generations == 1
            && (*gui).frame_xrgb32_nearest_commands == 64
            && (*gui).frame_xrgb32_nearest_resource_bytes == 64 * resource.len() as u64
            && life::live_generations(gui) == 1
            && info.flags & c::gui_frame_generation_flag_full != 0
            && info.flags & c::gui_frame_generation_flag_replacement != 0
            && info.chain_depth == 1
            && stream.live_generation_count == 1
            && stream.replacement_commit_count == 63
            && stream.coalesced_generation_count == 63
            && stream.xrgb32_nearest_command_count == 64
            && stream.current_frame_bytes == life::frame_bytes(latest)
            && stream.peak_frame_bytes >= stream.current_frame_bytes
            && rejected
            && (*gui).building_frame.is_null()
            && (*gui).committed_frame == committed_before_oom;
        storage::rollback(id, &mut storage);
        counters && baseline(heap, base)
    }
}

fn info_equal(a: &c::GuiFrameInfo, b: &c::GuiFrameInfo) -> bool {
    a.version == b.version
        && a.size == b.size
        && a.state == b.state
        && a.flags == b.flags
        && super::registry::equal(a.owner, b.owner)
        && a.committed_generation == b.committed_generation
        && a.building_generation == b.building_generation
        && a.committed_command_count == b.committed_command_count
        && a.committed_resource_bytes == b.committed_resource_bytes
        && a.building_command_count == b.building_command_count
        && a.building_resource_bytes == b.building_resource_bytes
        && a.current_frame_bytes == b.current_frame_bytes
        && a.peak_frame_bytes == b.peak_frame_bytes
        && a.commit_count == b.commit_count
        && a.cancel_count == b.cancel_count
        && a.oom_count == b.oom_count
        && a.snapshot_read_count == b.snapshot_read_count
        && a.last_error == b.last_error
        && a.command_version == b.command_version
        && a.command_size == b.command_size
        && a.reserved0 == b.reserved0
        && a.reserved1 == b.reserved1
        && a.reserved2 == b.reserved2
        && a.reserved3 == b.reserved3
        && a.reserved4 == b.reserved4
}
unsafe extern "C" {
    fn r4os_call_program(entry: usize, arg: usize, stack: u64) -> i32;
}
#[inline(never)]
fn owner_stack_body() -> bool {
    let mut scratch = [MaybeUninit::<u8>::uninit(); 32 * 1024];
    let bytes = scratch.as_mut_ptr().cast::<u8>();
    unsafe {
        for i in (0..scratch.len()).step_by(4096) {
            ptr::write_volatile(bytes.add(i), 0x79);
        }
        ptr::write_volatile(bytes.add(scratch.len() - 1), 0x05);
        blocks::r4os_rust_blocks_invariant()
            && ptr::read_volatile(bytes) == 0x79
            && ptr::read_volatile(bytes.add(scratch.len() - 1)) == 0x05
    }
}
struct OwnerProbe {
    committed: *const u64,
    runtime: bool,
}
unsafe extern "C" fn owner_stack_entry(arg: usize) -> i32 {
    unsafe {
        let probe = &*(arg as *const OwnerProbe);
        if probe.runtime {
            let token = runtime::save_runtime();
            let before = ptr::read_volatile(probe.committed);
            let result = owner_stack_body() && ptr::read_volatile(probe.committed) == before;
            runtime::restore_runtime(token);
            i32::from(result)
        } else {
            let _guard = Guard::acquire(7);
            let before = ptr::read_volatile(probe.committed);
            i32::from(owner_stack_body() && ptr::read_volatile(probe.committed) == before)
        }
    }
}
unsafe fn stack_headroom() -> bool {
    unsafe {
        let thread = threads::current();
        if thread.is_null() {
            return false;
        }
        let instance = (*thread).owner_instance;
        if instance.is_null() || (*thread).flags & 1 == 0 {
            return false;
        }
        for runtime in [false, true] {
            let flags = crate::arch::runtime::r4os_rust_runtime_control(2, 0);
            crate::memory::owner::prepare_stack();
            crate::arch::runtime::r4os_rust_runtime_control(3, flags);
            let rsp: u64;
            core::arch::asm!("mov {},rsp",out(reg)rsp,options(nomem,nostack,preserves_flags));
            let top = (*instance).program_stack_committed_base + 4096;
            if top + 32 * 1024 >= rsp {
                return false;
            }
            let before = (*instance).program_stack_committed_size;
            let probe = OwnerProbe {
                committed: ptr::addr_of!((*instance).program_stack_committed_size),
                runtime,
            };
            if r4os_call_program(
                owner_stack_entry as *const () as usize,
                ptr::from_ref(&probe).addr(),
                top,
            ) != 1
                || (*instance).program_stack_committed_size <= before
            {
                return false;
            }
        }
        log(b"[STACKOWNER] result=OK physical=resident runtime=resident index=ok\r\n");
        true
    }
}
/// # Safety
/// Original admitted single-test context. Every failure seam is restored on exit.
pub(super) unsafe fn run() -> bool {
    unsafe {
        let s = state();
        let shared = self::shared();
        let saved_failure = (
            (*s).failure_after_present,
            (*s).failure_after_value,
            (*s).failure_cursor,
        );
        let saved_active = (*s).self_test_active;
        let saved_peak = (*s).self_test_peak_bytes;
        let saved_trace = ((*s).release_trace_enabled, (*s).release_trace_len);
        let saved_shared = (
            (*shared).failure_present,
            (*shared).failure_value,
            (*shared).failure_cursor,
        );
        struct Restore {
            failure: (bool, u32, u32),
            active: bool,
            peak: u64,
            trace: (bool, usize),
            shared: (bool, u32, u32),
        }
        impl Drop for Restore {
            fn drop(&mut self) {
                unsafe {
                    let s = state();
                    (*s).failure_after_present = self.failure.0;
                    (*s).failure_after_value = self.failure.1;
                    (*s).failure_cursor = self.failure.2;
                    (*s).self_test_active = self.active;
                    (*s).self_test_peak_bytes = self.peak;
                    (*s).release_trace_enabled = self.trace.0;
                    (*s).release_trace_len = self.trace.1;
                    let shared = self::shared();
                    (*shared).failure_present = self.shared.0;
                    (*shared).failure_value = self.shared.1;
                    (*shared).failure_cursor = self.shared.2;
                }
            }
        }
        let _restore = Restore {
            failure: saved_failure,
            active: saved_active,
            peak: saved_peak,
            trace: saved_trace,
            shared: saved_shared,
        };
        (*s).report = t::EMPTY_REPORT;
        fail_after(None);
        shared_raster::r4os_rust_program_shared_failure(0, false, true);
        if !warm() {
            (*s).report.failed_case = 1;
            return false;
        }
        let heap = heap_stats();
        let base = stats();
        (*s).self_test_active = true;
        (*s).self_test_peak_bytes = base.current_payload_bytes;
        (*s).release_trace_enabled = false;
        (*s).release_trace_len = 0;
        let mut case = 0;
        for (class, environment, stages) in [
            (2, false, 2),
            (2, true, 3),
            (0, false, 4),
            (0, true, 5),
            (1, false, 3),
            (1, true, 4),
        ] {
            for failure in 0..stages {
                case += 1;
                fail_after(Some(failure));
                if let Some(mut storage) = storage::allocate(0xffff_0000 + case, class, environment)
                {
                    fail_after(None);
                    storage::rollback(0xffff_0000 + case, &mut storage);
                    return failed(case, &heap, &base);
                }
                fail_after(None);
                if !baseline(&heap, &base) {
                    return failed(case, &heap, &base);
                }
            }
        }
        macro_rules! check {
            ($value:expr) => {{
                case += 1;
                if !$value {
                    return failed(case, &heap, &base);
                }
            }};
        }
        check!(lazy(0xfffe_0001, 0, 0, &heap, &base));
        check!(lazy(0xfffe_0002, 1, 0, &heap, &base));
        check!(lazy(0xfffe_0003, 2, 0, &heap, &base));
        check!(lazy(0xfffe_0004, 3, 0, &heap, &base));
        check!(lazy(0xfffe_0005, 3, 1, &heap, &base));
        check!(zero_initialization(0xfffe_0006, &heap, &base));
        check!(raster_growth(0xfffe_0007, &heap, &base));
        check!(raster_growth_failure(0xfffe_0008, &heap, &base));
        check!(dynamic_commands(0xfffe_0009, &heap, &base));
        check!(mixed_resources(0xfffe_000a, &heap, &base));
        check!(atomic_growth_failure(0xfffe_000b, &heap, &base));
        check!(retired_reader(0xfffe_000c, &heap, &base));
        check!(delta_indexed(0xfffe_000d, &heap, &base));
        check!(replacement_xrgb(0xfffe_000e, &heap, &base));
        check!(shared_lifecycle(&heap, &base));
        check!(stack_headroom());
        (*s).report = t::SelfTestReport {
            cases: case,
            failed_case: 0,
            peak_payload_bytes: (*s).self_test_peak_bytes,
            heap_baseline_ok: true,
            storage_baseline_ok: true,
            zero_init_ok: true,
        };
        log(b"[PINSTSTOR] result=OK cases=");
        dec(case.into());
        log(b" heap_baseline=ok storage_baseline=ok zero_init=ok\r\n");
        true
    }
}
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
struct BufferStats {
    objects: u64,
    references: u64,
    leases: u64,
    bytes: u64,
    retained_bytes: u64,
    system_bytes: u64,
    device_bytes: u64,
    system_backed_bytes: u64,
    device_backed_bytes: u64,
    system_pinned_bytes: u64,
    device_pinned_bytes: u64,
    scanout_pinned_bytes: u64,
    device_mapped_bytes: u64,
    allocating_bytes: u64,
    destroying_bytes: u64,
}

unsafe fn buffer_stats() -> BufferStats {
    unsafe {
        let mut out = MaybeUninit::uninit();
        program_test_buffer_stats(out.as_mut_ptr());
        out.assume_init()
    }
}
unsafe fn graphics_usage() -> crate::memory::vm_query::Usage {
    unsafe {
        let mut out = MaybeUninit::uninit();
        crate::memory::vm_query::r4os_rust_vm_window_usage(8, out.as_mut_ptr());
        out.assume_init()
    }
}
unsafe fn shared_lifecycle(heap: &heap::Stats, base: &t::Stats) -> bool {
    unsafe {
        if !shared_raster::empty() {
            return false;
        }
        let buffers = buffer_stats();
        let vm = graphics_usage();
        let passed = shared_inner();
        crate::gfx::buffers::collect();
        let after = buffer_stats();
        let vm_after = graphics_usage();
        if !passed
            || buffers != after
            || vm.active_ranges != vm_after.active_ranges
            || vm.committed_bytes != vm_after.committed_bytes
        {
            log(b"[GFXBO] raster balance inner=");
            dec(passed.into());
            for (label, a, b) in [
                (b" objects=" as &[u8], buffers.objects, after.objects),
                (b" refs=", buffers.references, after.references),
                (b" leases=", buffers.leases, after.leases),
                (b" bytes=", buffers.bytes, after.bytes),
                (
                    b" ranges=",
                    vm.active_ranges.into(),
                    vm_after.active_ranges.into(),
                ),
                (b" committed=", vm.committed_bytes, vm_after.committed_bytes),
            ] {
                log(label);
                dec(a);
                log(b"/");
                dec(b);
            }
            log(b"\n");
        }
        passed
            && shared_raster::empty()
            && baseline(heap, base)
            && buffers == after
            && vm.active_ranges == vm_after.active_ranges
            && vm.committed_bytes == vm_after.committed_bytes
            && vm.reserved_bytes == vm_after.reserved_bytes
    }
}
fn shared_failure(line: u32) -> bool {
    log(b"[GFXBO] raster selftest failed line=");
    dec(line.into());
    log(b"\n");
    false
}
unsafe fn shared_inner() -> bool {
    unsafe {
        let owner_a = c::ProgramProcessHandle {
            instance_id: 0xfffd_0001,
            generation: 101,
            reserved: 0,
        };
        let owner_b = c::ProgramProcessHandle {
            instance_id: 0xfffd_0002,
            generation: 102,
            reserved: 0,
        };
        let consumer_a = c::ProgramProcessHandle {
            instance_id: 0xfffd_0011,
            generation: 201,
            reserved: 0,
        };
        let consumer_b = c::ProgramProcessHandle {
            instance_id: 0xfffd_0012,
            generation: 202,
            reserved: 0,
        };
        let mut frames: [t::Frame; 4] = mem::zeroed();
        for frame in &mut frames {
            frame.chain_depth = 1;
            frame.header.kind = 1;
        }
        struct Cleanup {
            frames: *mut t::Frame,
            owners: [c::ProgramProcessHandle; 4],
        }
        impl Drop for Cleanup {
            fn drop(&mut self) {
                unsafe {
                    shared_raster::r4os_rust_program_shared_failure(0, false, true);
                    for i in 0..4 {
                        shared_raster::release_frames(self.frames.add(i));
                    }
                    for owner in self.owners {
                        shared_raster::release_process(owner);
                    }
                }
            }
        }
        let _cleanup = Cleanup {
            frames: frames.as_mut_ptr(),
            owners: [consumer_a, consumer_b, owner_a, owner_b],
        };
        macro_rules! check {
            ($value:expr) => {
                if !$value {
                    return shared_failure(line!());
                }
            };
        }
        let info = c::GuiSharedRasterCreateInfo {
            format: c::gui_shared_raster_format_xrgb32,
            width: 4,
            height: 4,
            stride_bytes: 16,
            data_bytes: 64,
            ..Default::default()
        };
        let mut invalid = info;
        invalid.stride_bytes = 15;
        let mut invalid_handle = c::GuiSharedRasterHandle::default();
        check!(
            shared_raster::create(owner_a, invalid, &mut invalid_handle)
                == c::gui_frame_error_invalid
        );
        for failure in 0..c::gui_shared_raster_buffer_count {
            shared_raster::r4os_rust_program_shared_failure(failure, true, true);
            check!(
                shared_raster::create(owner_a, info, &mut invalid_handle) == c::gui_frame_error_oom
            );
        }
        shared_raster::r4os_rust_program_shared_failure(0, false, true);
        let mut handle = c::GuiSharedRasterHandle::default();
        check!(shared_raster::create(owner_a, info, &mut handle) == 0);
        let mut write = c::GuiSharedRasterWriteMap::default();
        check!(
            shared_raster::map_write(owner_a, handle, &mut write) == 0
                && write.byte_length == info.data_bytes
        );
        let bytes = write.data_address as *mut u8;
        let len = write.byte_length as usize;
        for i in 0..len {
            bytes.add(i).write((i * 13 + 7) as u8);
        }
        let mut generation = 0;
        check!(
            shared_raster::publish(owner_a, write, &mut generation) == 0
                && generation != 0
                && shared_raster::publish(owner_a, write, &mut generation)
                    == c::gui_frame_error_stale
        );
        let descriptor = c::GuiSharedRasterResource {
            handle,
            raster_generation: generation,
            format: info.format,
            source_w: info.width,
            source_h: info.height,
            guest_w: info.width,
            guest_h: info.height,
            viewport_w: info.width,
            viewport_h: info.height,
            ..Default::default()
        };
        check!(shared_raster::pin_frame(owner_a, &mut frames[0], descriptor).is_some());
        frames[0].shared_raster_count = 1;
        check!(
            shared_raster::pin_frame(owner_a, &mut frames[0], descriptor).is_some()
                && shared_raster::has_reference(&frames[0], handle, generation)
        );
        let mut map_a = c::GuiSharedRasterMap::default();
        let mut map_b = c::GuiSharedRasterMap::default();
        check!(
            shared_raster::acquire(consumer_a, owner_a, 301, handle, generation, &mut map_a) == 0
                && shared_raster::acquire(consumer_b, owner_a, 301, handle, generation, &mut map_b)
                    == 0
                && map_a.data_address == write.data_address
                && map_b.data_address == write.data_address
                && map_a.byte_length == info.data_bytes
        );
        check!(
            slice::from_raw_parts(map_a.data_address as *const u8, map_a.byte_length as usize)
                == slice::from_raw_parts(bytes, len)
        );
        let mut exported = c::GfxBufferReference::default();
        let mut exported_map = c::GfxBufferMap::default();
        check!(
            shared_raster::r4os_rust_program_shared_export(consumer_a, &map_a.lease, &mut exported)
                == c::gfx_buffer_result_ok
                && exported.flags == 1
                && program_domain_gfx_buffer_map(
                    consumer_a,
                    &exported.reference,
                    0,
                    0,
                    info.data_bytes,
                    &mut exported_map
                ) == c::gfx_buffer_result_ok
                && exported_map.cpu_address == map_a.data_address
        );
        check!(
            shared_raster::release(consumer_b, map_a.lease) == c::gui_frame_error_invalid
                && shared_raster::release(consumer_a, map_a.lease) == 0
                && shared_raster::release(consumer_a, map_a.lease) == c::gui_frame_error_stale
        );
        check!(
            shared_raster::destroy(owner_a, handle) == 0
                && shared_raster::map_write(owner_a, handle, &mut write)
                    == c::gui_frame_error_invalid
                && shared_raster::release_frames(&mut frames[0])
                && shared_raster::release(consumer_b, map_b.lease) == 0
        );
        shared_raster::release_process(owner_a);
        check!(
            slice::from_raw_parts(exported_map.cpu_address as *const u8, 64)
                == slice::from_raw_parts(bytes, len)
        );
        let mut denied = c::GfxBufferMap::default();
        check!(
            program_domain_gfx_buffer_map(consumer_a, &exported.reference, 1, 0, 64, &mut denied)
                == c::gfx_buffer_error_unsupported
                && program_domain_gfx_buffer_unmap(consumer_a, &exported_map.lease)
                    == c::gfx_buffer_result_ok
                && program_domain_gfx_buffer_release(consumer_a, &exported.reference)
                    == c::gfx_buffer_result_ok
        );
        check!(shared_raster::create(owner_a, info, &mut handle) == 0);
        for frame in frames
            .iter_mut()
            .take(c::gui_shared_raster_buffer_count as usize)
        {
            let mut map = c::GuiSharedRasterWriteMap::default();
            let mut generation = 0;
            check!(
                shared_raster::map_write(owner_a, handle, &mut map) == 0
                    && shared_raster::publish(owner_a, map, &mut generation) == 0
            );
            let descriptor = c::GuiSharedRasterResource {
                handle,
                raster_generation: generation,
                ..descriptor
            };
            check!(shared_raster::pin_frame(owner_a, frame, descriptor).is_some());
            frame.shared_raster_count = 1;
        }
        check!(shared_raster::map_write(owner_a, handle, &mut write) == c::gui_frame_error_state);
        let backpressure = shared_raster::snapshot(owner_a);
        check!(
            backpressure.publish_count == u64::from(c::gui_shared_raster_buffer_count)
                && backpressure.backpressure_count == 1
                && backpressure.published_bytes
                    == info.data_bytes * u64::from(c::gui_shared_raster_buffer_count)
        );
        check!(shared_raster::destroy(owner_a, handle) == 0);
        for frame in frames
            .iter_mut()
            .take(c::gui_shared_raster_buffer_count as usize)
        {
            check!(shared_raster::release_frames(frame));
        }
        shared_raster::release_process(owner_a);
        check!(
            shared_raster::create(owner_b, info, &mut handle) == 0
                && shared_raster::map_write(owner_b, handle, &mut write) == 0
                && shared_raster::publish(owner_b, write, &mut generation) == 0
        );
        let cleanup_descriptor = c::GuiSharedRasterResource {
            handle,
            raster_generation: generation,
            ..descriptor
        };
        check!(shared_raster::pin_frame(owner_b, &mut frames[3], cleanup_descriptor).is_some());
        frames[3].shared_raster_count = 1;
        check!(
            shared_raster::acquire(consumer_a, owner_b, 302, handle, generation, &mut map_a) == 0
        );
        shared_raster::release_process(consumer_a);
        check!(shared_raster::release(consumer_a, map_a.lease) == c::gui_frame_error_stale);
        shared_raster::release_process(owner_b);
        check!(shared_raster::release_frames(&mut frames[3]));
        let mut handle_a = c::GuiSharedRasterHandle::default();
        let mut handle_b = c::GuiSharedRasterHandle::default();
        check!(
            shared_raster::create(owner_a, info, &mut handle_a) == 0
                && shared_raster::create(owner_b, info, &mut handle_b) == 0
                && shared_raster::destroy(owner_b, handle_a) == c::gui_frame_error_invalid
                && shared_raster::destroy(owner_a, handle_a) == 0
                && shared_raster::destroy(owner_b, handle_b) == 0
        );
        shared_raster::release_process(owner_a);
        shared_raster::release_process(owner_b);
        true
    }
}

const _: () = {
    assert!(mem::size_of::<BufferStats>() == 120);
    assert!(mem::offset_of!(BufferStats, objects) == 0);
    assert!(mem::offset_of!(BufferStats, references) == 8);
    assert!(mem::offset_of!(BufferStats, leases) == 16);
    assert!(mem::offset_of!(BufferStats, bytes) == 24);
    assert!(mem::offset_of!(BufferStats, retained_bytes) == 32);
    assert!(mem::offset_of!(BufferStats, system_bytes) == 40);
    assert!(mem::offset_of!(BufferStats, device_bytes) == 48);
    assert!(mem::offset_of!(BufferStats, system_backed_bytes) == 56);
    assert!(mem::offset_of!(BufferStats, device_backed_bytes) == 64);
    assert!(mem::offset_of!(BufferStats, system_pinned_bytes) == 72);
    assert!(mem::offset_of!(BufferStats, device_pinned_bytes) == 80);
    assert!(mem::offset_of!(BufferStats, scanout_pinned_bytes) == 88);
    assert!(mem::offset_of!(BufferStats, device_mapped_bytes) == 96);
    assert!(mem::offset_of!(BufferStats, allocating_bytes) == 104);
    assert!(mem::offset_of!(BufferStats, destroying_bytes) == 112);
};

unsafe fn program_domain_gfx_buffer_release(
    owner: c::ProgramProcessHandle,
    input: *const c::GfxBufferHandle,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: crate::gfx::types::Owner {
            kind: 1,
            id: u64::from(owner.instance_id),
            generation: owner.generation,
        },
        input: input.cast(),
        ..Default::default()
    };
    unsafe { crate::gfx::buffer_api::r4os_rust_gfx_buffer_api(3, &args, core::ptr::null_mut()) }
}

unsafe fn program_domain_gfx_buffer_map(
    owner: c::ProgramProcessHandle,
    input: *const c::GfxBufferHandle,
    access: u32,
    offset: u64,
    bytes: u64,
    out: *mut c::GfxBufferMap,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: crate::gfx::types::Owner {
            kind: 1,
            id: u64::from(owner.instance_id),
            generation: owner.generation,
        },
        input: input.cast(),
        access,
        offset,
        bytes,
        ..Default::default()
    };
    unsafe { crate::gfx::buffer_api::r4os_rust_gfx_buffer_api(4, &args, out.cast()) }
}

unsafe fn program_domain_gfx_buffer_unmap(
    owner: c::ProgramProcessHandle,
    input: *const c::GfxBufferHandle,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: crate::gfx::types::Owner {
            kind: 1,
            id: u64::from(owner.instance_id),
            generation: owner.generation,
        },
        input: input.cast(),
        ..Default::default()
    };
    unsafe { crate::gfx::buffer_api::r4os_rust_gfx_buffer_api(6, &args, core::ptr::null_mut()) }
}

unsafe fn program_test_buffer_stats(out: *mut BufferStats) {
    unsafe {
        crate::gfx::buffers::lock();
        let value = crate::gfx::buffers::read().stats();
        crate::gfx::buffers::unlock();
        out.write(BufferStats {
            objects: value.objects as u64,
            references: value.references as u64,
            leases: value.leases as u64,
            bytes: value.bytes,
            retained_bytes: value.retained_bytes,
            system_bytes: value.system_bytes,
            device_bytes: value.device_bytes,
            system_backed_bytes: value.system_backed_bytes,
            device_backed_bytes: value.device_backed_bytes,
            system_pinned_bytes: value.system_pinned_bytes,
            device_pinned_bytes: value.device_pinned_bytes,
            scanout_pinned_bytes: value.scanout_pinned_bytes,
            device_mapped_bytes: value.device_mapped_bytes,
            allocating_bytes: value.allocating_bytes,
            destroying_bytes: value.destroying_bytes,
        });
    }
}
