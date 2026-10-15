//! Immutable GUI command/resource/raster views and versioned frame metadata.
//! Readers retain the captured generation across cooperative copy boundaries;
//! legacy materialization keeps its frozen flags and packed-alpha layout.
use super::{
    console::Lease, context, gui_flow as flow, gui_lifetime as life, gui_storage as storage,
    payload_types as t, registry, registry_types::ProgramInstance,
};
use crate::{
    contract::{self as c, GuiFrameCommand as Command, ProgramProcessHandle as Handle},
    sched::scheduler,
};
use core::{
    mem::{self, MaybeUninit},
    ptr,
};
const CHAIN: usize = c::gui_frame_max_delta_chain as usize;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Location {
    pub frame: *const t::Frame,
    pub command: *const t::GuiCommand,
    pub resource_base: u64,
    pub raster_base: u64,
}
pub(super) unsafe fn collect(frame: *const t::Frame, out: *mut [*const t::Frame; CHAIN]) -> usize {
    unsafe {
        let mut count = 0;
        let mut cursor = frame;
        while !cursor.is_null() {
            if count >= CHAIN {
                return 0;
            }
            (*out)[count] = cursor;
            count += 1;
            cursor = (*cursor).base_frame;
        }
        count
    }
}
pub(super) unsafe fn local_command(frame: *const t::Frame, index: u64) -> *const t::GuiCommand {
    unsafe {
        if index >= (*frame).command_count {
            return ptr::null();
        }
        let mut cursor = (*frame).command_payload;
        while !cursor.is_null() {
            let end = (*cursor).logical_offset + u64::from((*cursor).command_count);
            if index < end {
                let offset = index - (*cursor).logical_offset;
                assert!(offset < u64::from((*cursor).capacity));
                return cursor
                    .cast::<u8>()
                    .add(mem::size_of::<t::Commands>())
                    .cast::<t::GuiCommand>()
                    .add(usize::try_from(offset).expect("Local command index"));
            }
            cursor = (*cursor).next;
        }
        ptr::null()
    }
}
pub(super) unsafe fn location(frame: *const t::Frame, index: u64) -> Option<Location> {
    unsafe {
        let mut chain = [ptr::null(); CHAIN];
        let mut count = collect(frame, &mut chain);
        if count == 0 {
            return None;
        }
        let mut commands = 0;
        let mut resources = 0;
        let mut rasters = 0;
        while count != 0 {
            count -= 1;
            let item = chain[count];
            if index < commands + (*item).command_count {
                let command = local_command(item, index - commands);
                if command.is_null() {
                    return None;
                }
                return Some(Location {
                    frame: item,
                    command,
                    resource_base: resources,
                    raster_base: rasters,
                });
            }
            commands += (*item).command_count;
            resources += (*item).resource_len;
            rasters += (*item).raster_words;
        }
        None
    }
}
pub(super) unsafe fn local_resources(
    frame: *const t::Frame,
    start: u64,
    out: *mut u8,
    len: usize,
) -> usize {
    unsafe {
        if len == 0 || start >= (*frame).resource_len {
            return 0;
        }
        let wanted = usize::try_from((len as u64).min((*frame).resource_len - start))
            .expect("Resource read length");
        let mut copied = 0;
        let mut cursor = (*frame).resource_payload;
        while !cursor.is_null() {
            let end = (*cursor).logical_offset + u64::from((*cursor).byte_count);
            if start + (copied as u64) >= end {
                cursor = (*cursor).next;
                continue;
            }
            let source =
                usize::try_from((start + copied as u64).saturating_sub((*cursor).logical_offset))
                    .expect("Resource node offset");
            let chunk = (wanted - copied).min((*cursor).byte_count as usize - source);
            flow::copy(
                out.add(copied),
                cursor
                    .cast::<u8>()
                    .add(mem::size_of::<t::Resource>())
                    .add(source),
                chunk,
            );
            copied += chunk;
            if copied == wanted {
                break;
            }
            cursor = (*cursor).next;
        }
        copied
    }
}
pub(super) unsafe fn resources(
    frame: *const t::Frame,
    start: u64,
    out: *mut u8,
    len: usize,
) -> usize {
    unsafe {
        let total = life::chain_count(frame, 1);
        if len == 0 || start >= total {
            return 0;
        }
        let mut chain = [ptr::null(); CHAIN];
        let mut count = collect(frame, &mut chain);
        if count == 0 {
            return 0;
        }
        let mut base = 0;
        let mut copied = 0;
        while count != 0 && copied < len {
            count -= 1;
            let item = chain[count];
            let end = base + (*item).resource_len;
            if start + (copied as u64) < end {
                let local = (start + copied as u64).saturating_sub(base);
                copied += local_resources(item, local, out.add(copied), len - copied);
            }
            base = end;
        }
        copied
    }
}
pub(super) unsafe fn legacy(location: Location, out: *mut c::GuiCommand) -> bool {
    unsafe {
        let command = location.command;
        out.write(c::GuiCommand {
            kind: (*command).kind,
            x: (*command).x,
            y: (*command).y,
            w: (*command).w,
            h: (*command).h,
            rgb: (*command).rgb,
            fg: (*command).fg,
            bg: (*command).bg,
            font_id: (*command).font_id,
            flags: (*command).flags,
            text_w: (*command).text_w,
            text_h: (*command).text_h,
            baseline: (*command).baseline,
            line_height: (*command).line_height,
            ..Default::default()
        });
        match (*command).resource_kind {
            0 => {}
            1 => {
                let count = (*command).payload_bytes.min(((*out).text.len() - 1) as u64) as usize;
                let text = ptr::addr_of_mut!((*out).text).cast::<u8>();
                if local_resources(location.frame, (*command).payload_offset, text, count) != count
                {
                    return false;
                }
                text.add(count).write(0);
            }
            2 => {
                let Some(offset) = location
                    .raster_base
                    .checked_add((*command).raster_word_offset)
                else {
                    return false;
                };
                if offset > u64::from(u32::MAX) || (*command).parameter0 > u64::from(u32::MAX) {
                    return false;
                }
                (*out).rgb = offset as u32;
                (*out).flags = (*command).parameter0 as u32;
            }
            3 => {
                let Some(bytes) = (*command).payload_bytes.checked_add(3) else {
                    return false;
                };
                let words = bytes / 4;
                let Some(offset) = location
                    .raster_base
                    .checked_add((*command).raster_word_offset)
                else {
                    return false;
                };
                if offset > u64::from(u32::MAX) || words > u64::from(u32::MAX) {
                    return false;
                }
                (*out).fg = offset as u32;
                (*out).bg = words as u32;
                (*out).flags = 1;
            }
            4..=7 => return false,
            _ => unreachable!(),
        }
        true
    }
}
pub(super) fn raster_count(available: u64, capacity: u32) -> usize {
    available.min(u64::from(capacity).min(i32::MAX as u64)) as usize
}
pub(super) unsafe fn local_rasters(
    frame: *const t::Frame,
    start: u64,
    out: *mut u32,
    len: usize,
) -> usize {
    unsafe {
        if len == 0 || start >= (*frame).raster_words {
            return 0;
        }
        let wanted = usize::try_from((len as u64).min((*frame).raster_words - start))
            .expect("Raster read length");
        let mut copied = 0;
        let mut cursor = (*frame).command_payload;
        while !cursor.is_null() {
            assert!((*cursor).command_count <= (*cursor).capacity);
            let commands = cursor
                .cast::<u8>()
                .add(mem::size_of::<t::Commands>())
                .cast::<t::GuiCommand>();
            for index in 0..(*cursor).command_count as usize {
                let command = commands.add(index).read();
                if !matches!(command.resource_kind, 2 | 3) {
                    continue;
                }
                let words = if command.resource_kind == 2 {
                    command.payload_bytes / 4
                } else {
                    let Some(bytes) = command.payload_bytes.checked_add(3) else {
                        return copied;
                    };
                    bytes / 4
                };
                let end = command.raster_word_offset + words;
                let mut word = start + (copied as u64);
                if word >= end || word < command.raster_word_offset {
                    continue;
                }
                while word < end && copied < wanted {
                    let local = word - command.raster_word_offset;
                    let byte = local * 4;
                    let mut encoded = [0u8; 4];
                    let available = 4u64.min(command.payload_bytes - byte) as usize;
                    if local_resources(
                        frame,
                        command.payload_offset + byte,
                        encoded.as_mut_ptr(),
                        available,
                    ) != available
                    {
                        return copied;
                    }
                    out.add(copied).write(u32::from_le_bytes(encoded));
                    copied += 1;
                    word += 1;
                }
                scheduler::r4os_rust_scheduler_control(11, 0, 0);
                if copied == wanted {
                    return copied;
                }
            }
            cursor = (*cursor).next;
        }
        copied
    }
}
pub(super) unsafe fn rasters(
    frame: *const t::Frame,
    start: u64,
    out: *mut u32,
    len: usize,
) -> usize {
    unsafe {
        let total = life::chain_count(frame, 2);
        if len == 0 || start >= total {
            return 0;
        }
        let mut chain = [ptr::null(); CHAIN];
        let mut count = collect(frame, &mut chain);
        if count == 0 {
            return 0;
        }
        let mut base = 0;
        let mut copied = 0;
        while count != 0 && copied < len {
            count -= 1;
            let item = chain[count];
            let end = base + (*item).raster_words;
            let position = start + (copied as u64);
            if position < end {
                let local = position.saturating_sub(base);
                copied += local_rasters(item, local, out.add(copied), len - copied);
            }
            base = end;
        }
        copied
    }
}
pub(super) unsafe fn external(command: *const t::GuiCommand, base: u64) -> Command {
    unsafe {
        Command {
            version: c::gui_frame_command_version,
            size: c::gui_frame_command_size,
            kind: (*command).kind,
            flags: 0,
            x: (*command).x,
            y: (*command).y,
            w: (*command).w,
            h: (*command).h,
            rgb: (*command).rgb,
            fg: (*command).fg,
            bg: (*command).bg,
            font_id: (*command).font_id,
            text_w: (*command).text_w,
            text_h: (*command).text_h,
            baseline: (*command).baseline,
            line_height: (*command).line_height,
            resource_offset: if (*command).payload_bytes == 0 {
                0
            } else {
                (*command).payload_offset + base
            },
            resource_bytes: (*command).payload_bytes,
            parameter0: (*command).parameter0,
            parameter1: (*command).parameter1,
        }
    }
}
pub(super) unsafe fn local_commands(
    frame: *const t::Frame,
    base: u64,
    out: *mut Command,
    len: usize,
) -> usize {
    unsafe {
        let mut copied = 0;
        let mut cursor = (*frame).command_payload;
        while !cursor.is_null() {
            assert!((*cursor).command_count <= (*cursor).capacity);
            let commands = cursor
                .cast::<u8>()
                .add(mem::size_of::<t::Commands>())
                .cast::<t::GuiCommand>();
            for index in 0..(*cursor).command_count as usize {
                assert!(copied < len);
                out.add(copied).write(external(commands.add(index), base));
                copied += 1;
            }
            scheduler::r4os_rust_scheduler_control(11, 0, 0);
            cursor = (*cursor).next;
        }
        copied
    }
}
pub(super) unsafe fn commands(frame: *const t::Frame, out: *mut Command, len: usize) -> usize {
    unsafe {
        let mut chain = [ptr::null(); CHAIN];
        let mut count = collect(frame, &mut chain);
        if count == 0 {
            return 0;
        }
        let mut copied = 0;
        let mut base = 0;
        while count != 0 {
            count -= 1;
            let item = chain[count];
            assert!(copied <= len);
            copied += local_commands(item, base, out.add(copied), len - copied);
            base += (*item).resource_len;
        }
        copied
    }
}
pub(super) unsafe fn info(gui: *const t::Gui, owner: Handle, out: *mut c::GuiFrameInfo) {
    unsafe {
        let committed = (*gui).committed_frame;
        let building = (*gui).building_frame;
        let mut flags = 0;
        if !committed.is_null() {
            flags |= c::gui_frame_flag_committed;
        }
        if !building.is_null() {
            flags |= c::gui_frame_flag_building;
        }
        if (*gui).frame_last_error == c::gui_frame_error_oom {
            flags |= c::gui_frame_flag_last_oom;
        }
        out.write(c::GuiFrameInfo {
            state: if !building.is_null() {
                c::gui_frame_state_building
            } else {
                c::gui_frame_state_idle
            },
            flags,
            owner,
            committed_generation: if committed.is_null() {
                0
            } else {
                (*committed).generation
            },
            building_generation: if building.is_null() {
                0
            } else {
                (*building).generation
            },
            committed_command_count: if committed.is_null() {
                0
            } else {
                life::chain_count(committed, 0)
            },
            committed_resource_bytes: if committed.is_null() {
                0
            } else {
                life::chain_count(committed, 1)
            },
            building_command_count: if building.is_null() {
                0
            } else {
                (*building).command_count
            },
            building_resource_bytes: if building.is_null() {
                0
            } else {
                (*building).resource_len
            },
            current_frame_bytes: life::owned(gui, false),
            peak_frame_bytes: (*gui).frame_peak_bytes,
            commit_count: (*gui).frame_commits,
            cancel_count: (*gui).frame_cancels,
            oom_count: (*gui).frame_oom,
            snapshot_read_count: (*gui).frame_snapshot_reads,
            last_error: (*gui).frame_last_error,
            ..Default::default()
        });
    }
}
pub(super) unsafe fn empty(owner: Handle, result: i32, out: *mut c::GuiFrameInfo) {
    unsafe {
        out.write(c::GuiFrameInfo {
            owner,
            last_error: result,
            ..Default::default()
        });
    }
}
pub(super) unsafe fn current_info(id: u32, result: i32, out: *mut c::GuiFrameInfo) {
    unsafe {
        let Some(lease) = Lease::id(id) else {
            return;
        };
        let instance = lease.0.instance;
        let mut owner = MaybeUninit::uninit();
        if !registry::r4os_rust_program_registry_handle(2, instance.cast(), 0, owner.as_mut_ptr()) {
            return;
        }
        let gui = (*instance).gui_payload.cast::<t::Gui>();
        if gui.is_null() {
            return;
        }
        storage::lock(gui);
        info(gui, owner.assume_init(), out);
        (*out).last_error = result;
        storage::unlock(gui);
    }
}
pub(super) unsafe fn indexed(frame: *const t::Frame) -> bool {
    unsafe {
        let mut cursor = (*frame).command_payload;
        while !cursor.is_null() {
            assert!((*cursor).command_count <= (*cursor).capacity);
            let commands = cursor
                .cast::<u8>()
                .add(mem::size_of::<t::Commands>())
                .cast::<t::GuiCommand>();
            for index in 0..(*cursor).command_count as usize {
                if (*commands.add(index)).kind == c::gui_frame_command_kind_indexed8 {
                    return true;
                }
            }
            cursor = (*cursor).next;
        }
        false
    }
}
pub(super) unsafe fn shared(frame: *const t::Frame) -> bool {
    unsafe {
        let mut cursor = frame;
        while !cursor.is_null() {
            if (*cursor).shared_raster_count != 0 {
                return true;
            }
            cursor = (*cursor).base_frame;
        }
        false
    }
}
pub(super) unsafe fn generation(
    gui: *const t::Gui,
    owner: Handle,
    frame: *const t::Frame,
    out: *mut c::GuiFrameGenerationInfo,
) {
    unsafe {
        let base = (*frame).base_frame;
        let mut flags = if base.is_null() {
            c::gui_frame_generation_flag_full
        } else {
            c::gui_frame_generation_flag_delta
        };
        if (*frame).replacement {
            flags |= c::gui_frame_generation_flag_replacement;
        }
        if indexed(frame) {
            flags |= c::gui_frame_generation_flag_indexed8;
        }
        if shared(frame) {
            flags |= c::gui_frame_generation_flag_shared_raster;
        }
        out.write(c::GuiFrameGenerationInfo {
            flags,
            damage_count: (*frame).damage_count,
            owner,
            generation: (*frame).generation,
            base_generation: if base.is_null() {
                0
            } else {
                (*base).generation
            },
            command_count: (*frame).command_count,
            resource_bytes: (*frame).resource_len,
            total_command_count: life::chain_count(frame, 0),
            total_resource_bytes: life::chain_count(frame, 1),
            chain_depth: (*frame).chain_depth,
            delta_commit_count: (*gui).frame_delta_commits,
            full_commit_count: (*gui).frame_full_commits,
            indexed8_command_count: (*gui).frame_indexed8_commands,
            indexed8_resource_bytes: (*gui).frame_indexed8_resource_bytes,
            avoided_clone_bytes: (*gui).frame_avoided_clone_bytes,
            generation_read_count: (*gui).frame_generation_reads,
            ..Default::default()
        });
    }
}
pub(super) unsafe fn stream(gui: *const t::Gui, owner: Handle, out: *mut c::GuiFrameStreamInfo) {
    unsafe {
        let frame = (*gui).committed_frame;
        out.write(c::GuiFrameStreamInfo {
            live_generation_count: life::live_generations(gui),
            owner,
            committed_generation: if frame.is_null() {
                0
            } else {
                (*frame).generation
            },
            replacement_commit_count: (*gui).frame_replacement_commits,
            superseded_generation_count: (*gui).frame_superseded_generations,
            coalesced_generation_count: (*gui).frame_coalesced_generations,
            reader_retired_generation_count: (*gui).frame_reader_retired_generations,
            xrgb32_nearest_command_count: (*gui).frame_xrgb32_nearest_commands,
            xrgb32_nearest_resource_bytes: (*gui).frame_xrgb32_nearest_resource_bytes,
            current_frame_bytes: if frame.is_null() {
                0
            } else {
                life::frame_bytes(frame)
            },
            peak_frame_bytes: (*gui).frame_stream_peak_bytes,
            ..Default::default()
        });
    }
}
pub(super) unsafe fn write_stream(
    out: *mut c::GuiFrameStreamInfo,
    version: u32,
    mut value: c::GuiFrameStreamInfo,
) {
    let size = if version == 1 {
        112
    } else {
        mem::size_of::<c::GuiFrameStreamInfo>()
    };
    value.version = if version == 1 {
        1
    } else {
        c::gui_frame_stream_info_version
    };
    value.size = u32::try_from(size).expect("Frame stream prefix");
    unsafe {
        ptr::copy_nonoverlapping(ptr::addr_of!(value).cast::<u8>(), out.cast(), size);
    }
}
pub(super) unsafe fn valid_info(out: *const c::GuiFrameInfo) -> bool {
    unsafe { (*out).version >= c::gui_frame_info_version && (*out).size >= c::gui_frame_info_size }
}
pub(super) unsafe fn valid_generation(out: *const c::GuiFrameGenerationInfo) -> bool {
    unsafe {
        (*out).version >= c::gui_frame_generation_info_version
            && (*out).size >= c::gui_frame_generation_info_size
    }
}
unsafe fn valid_stream(out: *const c::GuiFrameStreamInfo) -> bool {
    unsafe {
        ((*out).version == 1 && (*out).size >= 112)
            || ((*out).version >= c::gui_frame_stream_info_version
                && (*out).size >= c::gui_frame_stream_info_size)
    }
}
pub(super) struct Snapshot<T: Copy> {
    pub(super) caller: *mut T,
    pub(super) value: T,
}
impl<T: Copy> Drop for Snapshot<T> {
    fn drop(&mut self) {
        unsafe {
            self.caller.write(self.value);
        }
    }
}
pub(super) struct Reader {
    pub(super) instance: *mut ProgramInstance,
    pub(super) capture: flow::Capture,
}
impl Drop for Reader {
    fn drop(&mut self) {
        unsafe {
            flow::release_capture(self.instance, ptr::addr_of_mut!(self.capture));
        }
    }
}
unsafe fn api_info(handle: *const Handle, out: *mut c::GuiFrameInfo) -> i32 {
    unsafe {
        if out.is_null() {
            return c::gui_frame_error_invalid;
        }
        let mut snapshot = Snapshot {
            caller: out,
            value: out.read(),
        };
        let out = ptr::addr_of_mut!(snapshot.value);
        if !valid_info(out) {
            return c::gui_frame_error_invalid;
        }
        let handle = if handle.is_null() {
            let Some(handle) = context::handle() else {
                empty(Handle::default(), c::gui_frame_error_unavailable, out);
                return c::gui_frame_error_unavailable;
            };
            handle
        } else {
            handle.read()
        };
        if !registry::valid(handle) {
            empty(handle, c::gui_frame_error_invalid, out);
            return c::gui_frame_error_invalid;
        }
        let Some(lease) = Lease::handle(handle) else {
            empty(handle, c::gui_frame_error_invalid, out);
            current_info(handle.instance_id, c::gui_frame_error_invalid, out);
            return c::gui_frame_error_invalid;
        };
        let gui = (*lease.0.instance).gui_payload.cast::<t::Gui>();
        if gui.is_null() {
            empty(handle, c::gui_frame_error_unavailable, out);
            return c::gui_frame_error_unavailable;
        }
        storage::lock(gui);
        info(gui, handle, out);
        storage::unlock(gui);
        c::gui_frame_result_ok
    }
}
unsafe fn api_stream(handle: *const Handle, out: *mut c::GuiFrameStreamInfo) -> i32 {
    unsafe {
        if out.is_null() {
            return c::gui_frame_error_invalid;
        }
        let mut snapshot = Snapshot {
            caller: out,
            value: out.read(),
        };
        let out = ptr::addr_of_mut!(snapshot.value);
        if handle.is_null() || !valid_stream(out) {
            return c::gui_frame_error_invalid;
        }
        let version = (*out).version;
        let handle = handle.read();
        if !registry::valid(handle) {
            write_stream(
                out,
                version,
                c::GuiFrameStreamInfo {
                    owner: handle,
                    ..Default::default()
                },
            );
            return c::gui_frame_error_invalid;
        }
        let Some(lease) = Lease::handle(handle) else {
            write_stream(
                out,
                version,
                c::GuiFrameStreamInfo {
                    owner: handle,
                    ..Default::default()
                },
            );
            return c::gui_frame_error_invalid;
        };
        let gui = (*lease.0.instance).gui_payload.cast::<t::Gui>();
        if gui.is_null() {
            write_stream(
                out,
                version,
                c::GuiFrameStreamInfo {
                    owner: handle,
                    ..Default::default()
                },
            );
            return c::gui_frame_error_unavailable;
        }
        let mut result = c::GuiFrameStreamInfo::default();
        storage::lock(gui);
        stream(gui, handle, &mut result);
        storage::unlock(gui);
        let shared = super::shared_raster::snapshot(handle);
        result.shared_publish_count = shared.publish_count;
        result.shared_acquire_count = shared.acquire_count;
        result.shared_release_count = shared.release_count;
        result.shared_backpressure_count = shared.backpressure_count;
        result.shared_published_bytes = shared.published_bytes;
        result.shared_frame_bytes_avoided = shared.frame_bytes_avoided;
        result.shared_acquired_bytes = shared.acquired_bytes;
        result.shared_live_bytes = shared.live_bytes;

        write_stream(out, version, result);
        c::gui_frame_result_ok
    }
}
unsafe fn api_generation(
    handle: *const Handle,
    requested: u64,
    out: *mut c::GuiFrameGenerationInfo,
) -> i32 {
    unsafe {
        if out.is_null() {
            return c::gui_frame_error_invalid;
        }
        let mut snapshot = Snapshot {
            caller: out,
            value: out.read(),
        };
        let out = ptr::addr_of_mut!(snapshot.value);
        if handle.is_null() || requested == 0 || !valid_generation(out) {
            return c::gui_frame_error_invalid;
        }
        let handle = handle.read();
        if !registry::valid(handle) {
            out.write(c::GuiFrameGenerationInfo {
                owner: handle,
                generation: requested,
                ..Default::default()
            });
            return c::gui_frame_error_invalid;
        }
        let Some(lease) = Lease::handle(handle) else {
            out.write(c::GuiFrameGenerationInfo {
                owner: handle,
                generation: requested,
                ..Default::default()
            });
            return c::gui_frame_error_invalid;
        };
        let instance = lease.0.instance;
        let Some(capture) = flow::capture(instance, Some(requested), true) else {
            out.write(c::GuiFrameGenerationInfo {
                owner: handle,
                generation: requested,
                ..Default::default()
            });
            return c::gui_frame_error_stale;
        };
        let reader = Reader { instance, capture };
        let gui = reader.capture.gui;
        storage::lock(gui);
        generation(gui, handle, reader.capture.frame, out);
        storage::unlock(gui);
        c::gui_frame_result_ok
    }
}
/// # Safety
/// Exact captured frame chain and exclusive32-pointer chain output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_view_chain(
    frame: *const t::Frame,
    out: *mut [*const t::Frame; CHAIN],
) -> usize {
    unsafe { collect(frame, out) }
}
/// # Safety
/// Exact reader-retained immutable frame and exclusive successful location output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_view_location(
    frame: *const t::Frame,
    index: u64,
    out: *mut Location,
) -> bool {
    unsafe {
        let Some(value) = location(frame, index) else {
            return false;
        };
        out.write(value);
        true
    }
}
/// # Safety
/// Exact reader-retained immutable frame chain. Returns a borrowed command only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_view_command(
    frame: *const t::Frame,
    index: u64,
    local: bool,
) -> *const t::GuiCommand {
    unsafe {
        if local {
            local_command(frame, index)
        } else {
            location(frame, index).map_or(ptr::null(), |value| value.command)
        }
    }
}
/// # Safety
/// Exact reader-retained immutable frame and exclusive byte output span.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_view_resources(
    frame: *const t::Frame,
    start: u64,
    out: *mut u8,
    len: usize,
    local: bool,
) -> usize {
    unsafe {
        if local {
            local_resources(frame, start, out, len)
        } else {
            resources(frame, start, out, len)
        }
    }
}
/// # Safety
/// Exact captured location and exclusive complete canonical legacy output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_view_legacy(
    location: Location,
    out: *mut c::GuiCommand,
) -> bool {
    unsafe { legacy(location, out) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_gui_view_raster_count(available: u64, capacity: u32) -> usize {
    raster_count(available, capacity)
}
/// # Safety
/// Exact captured immutable frame and exclusive word output span across yields.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_view_rasters(
    frame: *const t::Frame,
    start: u64,
    out: *mut u32,
    len: usize,
    local: bool,
) -> usize {
    unsafe {
        if local {
            local_rasters(frame, start, out, len)
        } else {
            rasters(frame, start, out, len)
        }
    }
}
/// # Safety
/// Exact immutable command and exclusive canonical public command output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_view_external(
    command: *const t::GuiCommand,
    base: u64,
    out: *mut Command,
) {
    unsafe {
        out.write(external(command, base));
    }
}
/// # Safety
/// Captured immutable frame chain and exclusive sufficiently large command span.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_view_commands(
    frame: *const t::Frame,
    base: u64,
    out: *mut Command,
    len: usize,
    local: bool,
) -> usize {
    unsafe {
        if local {
            local_commands(frame, base, out, len)
        } else {
            commands(frame, out, len)
        }
    }
}
/// # Safety
/// Exact GUI under its metadata lock and exclusive canonical frame-info output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_view_info(
    gui: *const t::Gui,
    owner: Handle,
    out: *mut c::GuiFrameInfo,
) {
    unsafe {
        info(gui, owner, out);
    }
}
/// # Safety
/// Exclusive canonical frame-info output under its original caller lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_view_empty(
    owner: Handle,
    result: i32,
    out: *mut c::GuiFrameInfo,
) {
    unsafe {
        empty(owner, result, out);
    }
}
/// # Safety
/// Exclusive canonical frame-info output; lookup retains the exact owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_view_current(
    id: u32,
    result: i32,
    out: *mut c::GuiFrameInfo,
) {
    unsafe {
        current_info(id, result, out);
    }
}
/// # Safety
/// Readable canonical info record of the selected type, including its prefix.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_view_valid(
    op: u8,
    out: *const core::ffi::c_void,
) -> bool {
    unsafe {
        match op {
            0 => valid_info(out.cast()),
            1 => valid_generation(out.cast()),
            2 => valid_stream(out.cast()),
            _ => unreachable!(),
        }
    }
}
/// # Safety
/// Exact captured immutable frame chain under its original reader lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_view_contains(
    frame: *const t::Frame,
    shared_rasters: bool,
) -> bool {
    unsafe {
        if shared_rasters {
            shared(frame)
        } else {
            indexed(frame)
        }
    }
}
/// # Safety
/// Exact locked GUI, captured frame and exclusive canonical generation output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_view_generation(
    gui: *const t::Gui,
    owner: Handle,
    frame: *const t::Frame,
    out: *mut c::GuiFrameGenerationInfo,
) {
    unsafe {
        generation(gui, owner, frame, out);
    }
}
/// # Safety
/// Exact locked GUI and exclusive canonical stream-info output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_view_stream(
    gui: *const t::Gui,
    owner: Handle,
    out: *mut c::GuiFrameStreamInfo,
) {
    unsafe {
        stream(gui, owner, out);
    }
}
/// # Safety
/// Exclusive stream output with the caller's advertised valid prefix; complete
/// canonical input value remains readable and disjoint until the copy completes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_view_write_stream(
    out: *mut c::GuiFrameStreamInfo,
    version: u32,
    value: *const c::GuiFrameStreamInfo,
) {
    unsafe {
        write_stream(out, version, value.read());
    }
}
/// # Safety
/// Original API handles and exclusive canonical in/out record of selected type.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_view_api(
    op: u8,
    handle: *const Handle,
    generation: u64,
    out: *mut core::ffi::c_void,
) -> i32 {
    unsafe {
        match op {
            0 => api_info(handle, out.cast()),
            1 => api_stream(handle, out.cast()),
            2 => api_generation(handle, generation, out.cast()),
            _ => unreachable!(),
        }
    }
}
const _: () = {
    assert!(mem::size_of::<Location>() == 32);
    assert!(mem::align_of::<Location>() == 8);
    assert!(mem::offset_of!(Location, frame) == 0);
    assert!(mem::offset_of!(Location, command) == 8);
    assert!(mem::offset_of!(Location, resource_base) == 16);
    assert!(mem::offset_of!(Location, raster_base) == 24);
};
