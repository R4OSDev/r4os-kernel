//! Atomic explicit GUI batch append. All fallible command/blob allocations and
//! immutable raster pins belong to a private delta until the exact overflow
//! preflight succeeds; command/resource node order survives the final splice.
use super::{
    gui_flow as flow, gui_lifetime as life, gui_resources as resource, gui_storage as storage,
    payload_types as t, registry_types::ProgramInstance,
};
use crate::{
    contract::{self as c, GuiFrameCommand as Command},
    fs::vfs_types::Bytes,
};
use core::{mem, ptr, slice};
unsafe fn bytes<'a>(span: Bytes) -> &'a [u8] {
    if span.len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(span.data, span.len) }
    }
}
pub(super) unsafe fn has_ref(
    frame: *const t::Frame,
    handle: c::GuiSharedRasterHandle,
    generation: u64,
) -> bool {
    unsafe { super::shared_raster::has_reference(frame, handle, generation) }
}
unsafe fn pin(
    owner: c::ProgramProcessHandle,
    building: *const t::Frame,
    delta: *mut t::Frame,
    commands: &[Command],
    resources: &[u8],
) -> i32 {
    unsafe {
        for command in commands {
            if command.kind != c::gui_frame_command_kind_shared_raster {
                continue;
            }
            let Some(data) = resource::resource(command, resources) else {
                return c::gui_frame_error_invalid;
            };
            let Some(descriptor) = resource::shared_descriptor(data) else {
                return c::gui_frame_error_invalid;
            };
            if has_ref(building, descriptor.handle, descriptor.raster_generation)
                || has_ref(delta, descriptor.handle, descriptor.raster_generation)
            {
                continue;
            }
            if (*building).shared_raster_count + (*delta).shared_raster_count
                >= c::gui_shared_raster_max_frame_resources
            {
                return c::gui_frame_error_overflow;
            }
            if super::shared_raster::pin_frame(owner, delta, descriptor).is_none() {
                return c::gui_frame_error_stale;
            }
            (*delta).shared_raster_count += 1;
        }
        c::gui_frame_result_ok
    }
}
fn internal(command: &Command, base: u64, raster: u64) -> t::GuiCommand {
    let kind = resource::kind(command).unwrap_or(0);
    t::GuiCommand {
        version: command.version,
        size: command.size,
        kind: command.kind,
        flags: command.flags,
        x: command.x,
        y: command.y,
        w: command.w,
        h: command.h,
        rgb: command.rgb,
        fg: command.fg,
        bg: command.bg,
        font_id: command.font_id,
        text_w: command.text_w,
        text_h: command.text_h,
        baseline: command.baseline,
        line_height: command.line_height,
        payload_offset: if command.resource_bytes == 0 {
            0
        } else {
            base + command.resource_offset
        },
        payload_bytes: command.resource_bytes,
        parameter0: command.parameter0,
        parameter1: command.parameter1,
        resource_kind: kind,
        raster_word_offset: if matches!(kind, 2 | 3) { raster } else { 0 },
        reserved: 0,
    }
}
unsafe fn append_blob(
    instance: *mut ProgramInstance,
    frame: *mut t::Frame,
    resources: &[u8],
) -> bool {
    unsafe {
        let mut copied = 0;
        while copied < resources.len() {
            let count = (resources.len() - copied).min(65536);
            let Some(mut storage) =
                storage::prepare_blob(instance, frame, &resources[copied..copied + count])
            else {
                return false;
            };
            assert!((*storage.payload).byte_count as usize == count);
            storage::commit_resource(&mut storage);
            copied += count;
        }
        true
    }
}
unsafe fn splice(gui: *mut t::Gui, building: *mut t::Frame, delta: *mut t::Frame) {
    unsafe {
        let command_base = (*building).command_count;
        let resource_base = (*building).resource_len;
        let raster_base = (*building).raster_words;
        let sequence_base = (*building).node_sequence;
        let mut cursor = (*delta).command_payload;
        while !cursor.is_null() {
            (*cursor).allocation_sequence += sequence_base;
            (*cursor).logical_offset += command_base;
            assert!((*cursor).command_count <= (*cursor).capacity);
            let commands = cursor
                .cast::<u8>()
                .add(mem::size_of::<t::Commands>())
                .cast::<t::GuiCommand>();
            for index in 0..(*cursor).command_count as usize {
                let command = commands.add(index);
                if (*command).payload_bytes != 0 {
                    (*command).payload_offset += resource_base;
                }
                if matches!((*command).resource_kind, 2 | 3) {
                    (*command).raster_word_offset += raster_base;
                }
            }
            cursor = (*cursor).next;
        }
        let mut cursor = (*delta).resource_payload;
        while !cursor.is_null() {
            (*cursor).allocation_sequence += sequence_base;
            (*cursor).logical_offset += resource_base;
            cursor = (*cursor).next;
        }
        let head = (*delta).command_payload;
        if !head.is_null() {
            (*head).previous = (*building).command_tail;
            let tail = (*building).command_tail;
            if !tail.is_null() {
                (*tail).next = head;
            } else {
                (*building).command_payload = head;
            }
            (*building).command_tail = (*delta).command_tail;
        }
        let head = (*delta).resource_payload;
        if !head.is_null() {
            (*head).previous = (*building).resource_tail;
            let tail = (*building).resource_tail;
            if !tail.is_null() {
                (*tail).next = head;
            } else {
                (*building).resource_payload = head;
            }
            (*building).resource_tail = (*delta).resource_tail;
        }
        // Private delta nodes enter the accepted GUI graph exactly here.
        // Its temporary frame header remains private and is released below.
        life::append(
            gui,
            building,
            (*delta)
                .local_bytes
                .checked_sub(u64::from((*delta).header.requested_bytes))
                .expect("GUI batch node bytes"),
            (*delta).command_count,
        );
        (*building).command_count += (*delta).command_count;
        (*building).resource_len += (*delta).resource_len;
        (*building).raster_words += (*delta).raster_words;
        (*building).node_sequence += (*delta).node_sequence;
        assert!(super::shared_raster::move_frames(delta, building));
        (*delta).command_payload = ptr::null_mut();
        (*delta).command_tail = ptr::null_mut();
        (*delta).resource_payload = ptr::null_mut();
        (*delta).resource_tail = ptr::null_mut();
        (*delta).command_count = 0;
        (*delta).resource_len = 0;
        (*delta).raster_words = 0;
        (*delta).node_sequence = 0;
        (*delta).local_bytes = u64::from((*delta).header.requested_bytes);
    }
}
struct Delta {
    frame: *mut t::Frame,
    owner: u32,
}
impl Drop for Delta {
    fn drop(&mut self) {
        unsafe { life::release_frame(self.frame, self.owner) };
    }
}
unsafe fn invalid(gui: *mut t::Gui, building: *mut t::Frame, error: i32) -> i32 {
    unsafe {
        (*building).build_failed = true;
        flow::result(gui, error)
    }
}
unsafe fn append(instance: *mut ProgramInstance, commands: &[Command], resources: &[u8]) -> i32 {
    unsafe {
        let gui = (*instance).gui_payload.cast::<t::Gui>();
        if gui.is_null() {
            return c::gui_frame_error_unavailable;
        }
        let building = (*gui).building_frame;
        if building.is_null() {
            return flow::result(gui, c::gui_frame_error_state);
        }
        if !(*building).explicit_build || (*building).build_failed {
            return flow::result(gui, c::gui_frame_error_state);
        }
        for command in commands {
            if !resource::batch(command, resources) {
                return invalid(gui, building, c::gui_frame_error_invalid);
            }
        }
        let mut shared = false;
        for command in commands {
            if command.kind == c::gui_frame_command_kind_shared_raster {
                shared = true;
                break;
            }
        }
        if shared && !(*building).replacement && (*building).damage_count != 0 {
            return invalid(gui, building, c::gui_frame_error_state);
        }
        if commands.is_empty() && resources.is_empty() {
            return flow::result(gui, c::gui_frame_result_ok);
        }
        let delta = storage::allocate_frame((*instance).id, true);
        if delta.is_null() {
            flow::failed(gui, building);
            return c::gui_frame_error_oom;
        }
        let _release = Delta {
            frame: delta,
            owner: (*instance).id,
        };
        let mut raster = 0u64;
        for command in commands {
            let Some(mut storage) = storage::prepare_command(instance, delta) else {
                flow::failed(gui, building);
                return c::gui_frame_error_oom;
            };
            let internal = internal(command, 0, raster);
            storage::commit_command(&mut storage, internal);
            if internal.resource_kind == 2 {
                let Some(next) = raster.checked_add(internal.payload_bytes / 4) else {
                    return invalid(gui, building, c::gui_frame_error_overflow);
                };
                raster = next;
            } else if internal.resource_kind == 3 {
                let Some(bytes) = internal.payload_bytes.checked_add(3) else {
                    return invalid(gui, building, c::gui_frame_error_overflow);
                };
                let Some(next) = raster.checked_add(bytes / 4) else {
                    return invalid(gui, building, c::gui_frame_error_overflow);
                };
                raster = next;
            }
        }
        (*delta).raster_words = raster;
        if !append_blob(instance, delta, resources) {
            flow::failed(gui, building);
            return c::gui_frame_error_oom;
        }
        if !life::valid(delta, (*instance).id) {
            return invalid(gui, building, c::gui_frame_error_invalid);
        }
        if shared {
            let Some(owner) = flow::owner(instance) else {
                return invalid(gui, building, c::gui_frame_error_unavailable);
            };
            let result = pin(owner, building, delta, commands, resources);
            if result != c::gui_frame_result_ok {
                return invalid(gui, building, result);
            }
        }
        if (*building)
            .command_count
            .checked_add((*delta).command_count)
            .is_none()
        {
            return invalid(gui, building, c::gui_frame_error_overflow);
        }
        if (*building)
            .resource_len
            .checked_add((*delta).resource_len)
            .is_none()
        {
            return invalid(gui, building, c::gui_frame_error_overflow);
        }
        if (*building)
            .raster_words
            .checked_add((*delta).raster_words)
            .is_none()
        {
            return invalid(gui, building, c::gui_frame_error_overflow);
        }
        if (*building)
            .node_sequence
            .checked_add((*delta).node_sequence)
            .is_none()
        {
            return invalid(gui, building, c::gui_frame_error_overflow);
        }
        splice(gui, building, delta);
        flow::result(gui, c::gui_frame_result_ok)
    }
}
/// # Safety
/// Original immutable command and readable bounded resource spans.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_batch_kind(command: *const Command) -> u16 {
    unsafe { resource::kind(&*command).unwrap_or(u16::MAX) }
}
/// # Safety
/// Readable original command/resources and exclusive successful byte-span output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_batch_slice(
    command: *const Command,
    resources: Bytes,
    out: *mut Bytes,
) -> bool {
    unsafe {
        let Some(data) = resource::resource(&*command, bytes(resources)) else {
            return false;
        };
        out.write(Bytes {
            data: data.as_ptr(),
            len: data.len(),
        });
        true
    }
}
/// # Safety
/// Readable original byte span and exclusive successful word output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_shape_word(
    data: Bytes,
    offset: usize,
    out: *mut u32,
) -> bool {
    unsafe {
        let Some(word) = resource::read_u32(bytes(data), offset) else {
            return false;
        };
        out.write(word);
        true
    }
}
/// # Safety
/// Exact original descriptor bytes and exclusive canonical descriptor output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_descriptor(
    data: Bytes,
    out: *mut c::GuiSharedRasterResource,
) -> bool {
    unsafe {
        let Some(descriptor) = resource::shared_descriptor(bytes(data)) else {
            return false;
        };
        out.write(descriptor);
        true
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_gui_float(op: u8, bits: u32, maximum_bits: u32) -> bool {
    match op {
        0 => resource::float(bits).is_some(),
        1 => resource::coordinate(bits),
        2 => resource::non_negative(bits, f32::from_bits(maximum_bits)),
        _ => unreachable!(),
    }
}
/// # Safety
/// Valid command for op0..4, readable byte span, canonical segment count for op5.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_resource_valid(
    op: u8,
    command: *const Command,
    data: Bytes,
    count: u32,
) -> bool {
    unsafe {
        let data = bytes(data);
        match op {
            0 => resource::shared(&*command, data).is_some(),
            1 => resource::indexed(&*command, data).is_some(),
            2 => resource::xrgb(&*command, data).is_some(),
            3 => resource::shape(&*command, data).is_some(),
            4 => resource::batch(&*command, data),
            5 => resource::path(data, count).is_some(),
            _ => unreachable!(),
        }
    }
}
/// # Safety
/// Retained exact frame and generation-bound reference under original admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_has_ref(
    frame: *const t::Frame,
    handle: c::GuiSharedRasterHandle,
    generation: u64,
) -> bool {
    unsafe { has_ref(frame, handle, generation) }
}
/// # Safety
/// Retained building/private delta, readable commands/resources and exact owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_pin_batch(
    owner: c::ProgramProcessHandle,
    building: *const t::Frame,
    delta: *mut t::Frame,
    commands: *const Command,
    count: usize,
    resources: Bytes,
) -> i32 {
    unsafe {
        pin(
            owner,
            building,
            delta,
            if count == 0 {
                &[]
            } else {
                slice::from_raw_parts(commands, count)
            },
            bytes(resources),
        )
    }
}
/// # Safety
/// Readable original command and exclusive complete internal command output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_internal(
    command: *const Command,
    base: u64,
    raster: u64,
    out: *mut t::GuiCommand,
) {
    unsafe {
        out.write(internal(&*command, base, raster));
    }
}
/// # Safety
/// Retained private frame, producer instance and readable original resource span.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_blob(
    instance: *mut ProgramInstance,
    frame: *mut t::Frame,
    resources: Bytes,
) -> bool {
    unsafe { append_blob(instance, frame, bytes(resources)) }
}
/// # Safety
/// Exact GUI, admitted private building/delta and successful overflow preflight.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_splice(
    gui: *mut t::Gui,
    building: *mut t::Frame,
    delta: *mut t::Frame,
) {
    unsafe {
        splice(gui, building, delta);
    }
}
/// # Safety
/// Retained producer instance and valid command/resource inputs until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_batch(
    instance: *mut ProgramInstance,
    commands: *const Command,
    count: usize,
    resources: Bytes,
) -> i32 {
    unsafe {
        append(
            instance,
            if count == 0 {
                &[]
            } else {
                slice::from_raw_parts(commands, count)
            },
            bytes(resources),
        )
    }
}
