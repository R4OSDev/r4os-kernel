//! Public GUI transaction/read admission. Producer gates and exact process
//! pins retain the original order; immutable reads publish metadata before
//! copying, with generation-bound reader/unwind ownership through the copy.
use super::{
    console::Lease, gui_batch as batch, gui_events::Producer, gui_flow as flow,
    gui_lifetime as life, gui_storage as storage, gui_view as view, payload, payload_types as t,
    process, process_data, registry, registry_types::ProgramInstance, threads,
};
use crate::{
    contract::{self as c, GuiFrameCommand as Command, ProgramProcessHandle as Handle},
    fs::vfs_types::Bytes,
    sched::task_context,
};
use core::{mem, ptr, slice};
pub(super) unsafe fn append_command(instance: *mut ProgramInstance, command: t::GuiCommand) -> i32 {
    unsafe {
        let gui = process_data::ensure_gui(instance);
        if gui.is_null() {
            return -2;
        }
        let frame = flow::ensure(instance);
        if frame.is_null() {
            return -4;
        }
        let Some(mut reservation) = storage::prepare_command(instance, frame) else {
            flow::failed(gui, frame);
            return -4;
        };
        storage::commit_command(&mut reservation, command)
    }
}
unsafe fn transition(op: u8, regions: *const c::DisplayDamageRect, count: u32) -> i32 {
    unsafe {
        let Some(producer) = Producer::enter() else {
            return c::gui_frame_error_unavailable;
        };
        let instance = producer.instance;
        if op == 0 {
            let result = flow::begin(instance, 0, &[]);
            let gui = (*instance).gui_payload.cast::<t::Gui>();
            if !gui.is_null() {
                (*gui).frame_last_error = result;
            }
            return result;
        }
        if op == 1 || op == 2 {
            let gui = process_data::ensure_gui(instance);
            if gui.is_null() {
                return c::gui_frame_error_unavailable;
            }
            if regions.is_null() || count == 0 || count > c::gui_frame_max_damage_regions {
                return flow::result(gui, c::gui_frame_error_invalid);
            }
            return flow::begin(instance, op, slice::from_raw_parts(regions, count as usize));
        }
        let gui = (*instance).gui_payload.cast::<t::Gui>();
        if gui.is_null() {
            return c::gui_frame_error_unavailable;
        }
        let result = match op {
            3 => flow::commit(instance),
            4 => flow::cancel(instance),
            _ => unreachable!(),
        };
        (*gui).frame_last_error = result;
        result
    }
}
unsafe fn append(commands: *const Command, count: u64, resources: *const u8, len: u64) -> i32 {
    unsafe {
        let Some(producer) = Producer::enter() else {
            return c::gui_frame_error_unavailable;
        };
        let instance = producer.instance;
        let gui = (*instance).gui_payload.cast::<t::Gui>();
        if gui.is_null() {
            return c::gui_frame_error_unavailable;
        }
        let building = (*gui).building_frame;
        if building.is_null() || !(*building).explicit_build || (*building).build_failed {
            return flow::result(gui, c::gui_frame_error_state);
        }
        if (commands.is_null() && count != 0) || (resources.is_null() && len != 0) {
            (*building).build_failed = true;
            return flow::result(gui, c::gui_frame_error_invalid);
        }
        if usize::try_from(count).is_err()
            || usize::try_from(len).is_err()
            || (*building).command_count.checked_add(count).is_none()
            || (*building).resource_len.checked_add(len).is_none()
        {
            (*building).build_failed = true;
            return flow::result(gui, c::gui_frame_error_overflow);
        }
        batch::r4os_rust_program_gui_batch(
            instance,
            commands,
            count as usize,
            Bytes {
                data: resources,
                len: len as usize,
            },
        )
    }
}
unsafe fn read_error(
    gui: *mut t::Gui,
    owner: Handle,
    error: i32,
    out: *mut c::GuiFrameInfo,
) -> i32 {
    unsafe {
        (*gui).frame_last_error = error;
        view::info(gui, owner, out);
        storage::unlock(gui);
    }
    error
}
unsafe fn read(
    handle: *const Handle,
    expected: u64,
    commands: *mut Command,
    command_capacity: u64,
    resources: *mut u8,
    resource_capacity: u64,
    out: *mut c::GuiFrameInfo,
) -> i32 {
    unsafe {
        if out.is_null() {
            return c::gui_frame_error_invalid;
        }
        let mut snapshot = view::Snapshot {
            caller: out,
            value: out.read(),
        };
        let out = ptr::addr_of_mut!(snapshot.value);
        if handle.is_null() || !view::valid_info(out) {
            return c::gui_frame_error_invalid;
        }
        let handle = handle.read();
        if !registry::valid(handle) {
            view::empty(handle, c::gui_frame_error_invalid, out);
            return c::gui_frame_error_invalid;
        }
        let Some(lease) = Lease::handle(handle) else {
            view::empty(handle, c::gui_frame_error_invalid, out);
            view::current_info(handle.instance_id, c::gui_frame_error_invalid, out);
            return c::gui_frame_error_invalid;
        };
        let instance = lease.0.instance;
        let gui = (*instance).gui_payload.cast::<t::Gui>();
        if gui.is_null() {
            view::empty(handle, c::gui_frame_error_unavailable, out);
            return c::gui_frame_error_unavailable;
        }
        let Some(_operation) = threads::Operation::enter() else {
            return c::gui_frame_error_invalid;
        };
        storage::lock(gui);
        let frame = (*gui).committed_frame;
        if frame.is_null() {
            return read_error(gui, handle, c::gui_frame_error_unavailable, out);
        }
        if (*frame).generation != expected {
            return read_error(gui, handle, c::gui_frame_error_stale, out);
        }
        if (commands.is_null() && command_capacity != 0)
            || (resources.is_null() && resource_capacity != 0)
        {
            return read_error(gui, handle, c::gui_frame_error_invalid, out);
        }
        let total_commands = life::chain_count(frame, 0);
        let total_resources = life::chain_count(frame, 1);
        if command_capacity < total_commands || resource_capacity < total_resources {
            return read_error(gui, handle, c::gui_frame_error_buffer_too_small, out);
        }
        if usize::try_from(command_capacity).is_err()
            || usize::try_from(resource_capacity).is_err()
            || !life::valid(frame, (*instance).id)
            || (*frame).reader_refs == u32::MAX
        {
            return read_error(gui, handle, c::gui_frame_error_overflow, out);
        }
        (*frame).reader_refs += 1;
        (*gui).frame_snapshot_reads = (*gui).frame_snapshot_reads.wrapping_add(1);
        payload::add(mem::offset_of!(t::Stats, gui_frame_snapshot_reads), 8, 1);
        (*gui).frame_last_error = c::gui_frame_result_ok;
        view::info(gui, handle, out);
        storage::unlock(gui);
        let _reader = view::Reader {
            instance,
            capture: flow::Capture {
                gui,
                frame,
                generation: (*frame).generation,
                unwind: task_context::Unwind::EMPTY,
            },
        };
        if total_commands != 0 {
            view::commands(
                frame,
                commands,
                usize::try_from(total_commands).expect("Frame command count"),
            );
        }
        if total_resources != 0 {
            view::resources(
                frame,
                0,
                resources,
                usize::try_from(total_resources).expect("Frame resource bytes"),
            );
        }
        c::gui_frame_result_ok
    }
}
#[expect(
    clippy::too_many_arguments,
    reason = "Frozen read ABI has independent command/resource/damage buffers and capacities"
)]
unsafe fn read_generation(
    handle: *const Handle,
    generation: u64,
    commands: *mut Command,
    command_capacity: u64,
    resources: *mut u8,
    resource_capacity: u64,
    regions: *mut c::DisplayDamageRect,
    region_capacity: u32,
    out: *mut c::GuiFrameGenerationInfo,
) -> i32 {
    unsafe {
        if out.is_null() {
            return c::gui_frame_error_invalid;
        }
        let mut snapshot = view::Snapshot {
            caller: out,
            value: out.read(),
        };
        let out = ptr::addr_of_mut!(snapshot.value);
        if handle.is_null() || generation == 0 || !view::valid_generation(out) {
            return c::gui_frame_error_invalid;
        }
        let handle = handle.read();
        if !registry::valid(handle) {
            out.write(c::GuiFrameGenerationInfo {
                owner: handle,
                generation,
                ..Default::default()
            });
            return c::gui_frame_error_invalid;
        }
        let Some(lease) = Lease::handle(handle) else {
            out.write(c::GuiFrameGenerationInfo {
                owner: handle,
                generation,
                ..Default::default()
            });
            return c::gui_frame_error_invalid;
        };
        let instance = lease.0.instance;
        let Some(capture) = flow::capture(instance, Some(generation), true) else {
            out.write(c::GuiFrameGenerationInfo {
                owner: handle,
                generation,
                ..Default::default()
            });
            return c::gui_frame_error_stale;
        };
        let reader = view::Reader { instance, capture };
        let gui = reader.capture.gui;
        let frame = reader.capture.frame;
        storage::lock(gui);
        view::generation(gui, handle, frame, out);
        storage::unlock(gui);
        if (commands.is_null() && command_capacity != 0)
            || (resources.is_null() && resource_capacity != 0)
            || (regions.is_null() && region_capacity != 0)
        {
            return c::gui_frame_error_invalid;
        }
        if command_capacity < (*frame).command_count
            || resource_capacity < (*frame).resource_len
            || region_capacity < (*frame).damage_count
        {
            return c::gui_frame_error_buffer_too_small;
        }
        if usize::try_from(command_capacity).is_err()
            || usize::try_from(resource_capacity).is_err()
            || !life::valid(frame, (*instance).id)
        {
            return c::gui_frame_error_overflow;
        }
        if (*frame).command_count != 0 {
            view::local_commands(
                frame,
                0,
                commands,
                usize::try_from((*frame).command_count).expect("Generation commands"),
            );
        }
        if (*frame).resource_len != 0 {
            view::local_resources(
                frame,
                0,
                resources,
                usize::try_from((*frame).resource_len).expect("Generation resources"),
            );
        }
        if (*frame).damage_count != 0 {
            assert!((*frame).damage_count <= c::gui_frame_max_damage_regions);
            ptr::copy_nonoverlapping(
                ptr::addr_of!((*frame).damage_regions).cast(),
                regions,
                (*frame).damage_count as usize,
            );
        }
        storage::lock(gui);
        (*gui).frame_generation_reads = (*gui).frame_generation_reads.wrapping_add(1);
        view::generation(gui, handle, frame, out);
        storage::unlock(gui);
        c::gui_frame_result_ok
    }
}
/// # Safety
/// Valid original region span for op1/2. Current exact producer/gate admission
/// and error precedence are retained before any caller-input validation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_api_transition(
    op: u8,
    regions: *const c::DisplayDamageRect,
    count: u32,
) -> i32 {
    unsafe { transition(op, regions, count) }
}
/// # Safety
/// Readable original command/resource spans retained throughout the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_api_append(
    commands: *const Command,
    count: u64,
    resources: *const u8,
    len: u64,
) -> i32 {
    unsafe { append(commands, count, resources, len) }
}
/// # Safety
/// Original handle, exclusive canonical info and valid optional command/resource
/// outputs of their declared capacities. Reads retain their exact generation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_api_read(
    handle: *const Handle,
    expected: u64,
    commands: *mut Command,
    command_capacity: u64,
    resources: *mut u8,
    resource_capacity: u64,
    out: *mut c::GuiFrameInfo,
) -> i32 {
    unsafe {
        read(
            handle,
            expected,
            commands,
            command_capacity,
            resources,
            resource_capacity,
            out,
        )
    }
}
/// # Safety
/// Original handle, exclusive canonical generation info and independent valid
/// optional command/resource/damage outputs of their declared capacities.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_api_generation_read(
    handle: *const Handle,
    generation: u64,
    commands: *mut Command,
    command_capacity: u64,
    resources: *mut u8,
    resource_capacity: u64,
    regions: *mut c::DisplayDamageRect,
    region_capacity: u32,
    out: *mut c::GuiFrameGenerationInfo,
) -> i32 {
    unsafe {
        read_generation(
            handle,
            generation,
            commands,
            command_capacity,
            resources,
            resource_capacity,
            regions,
            region_capacity,
            out,
        )
    }
}
/// # Safety
/// Writable original word output span. Lookup/capture retain exact generation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_api_raster(
    id: u32,
    offset: u32,
    out: *mut u32,
    capacity: u32,
) -> i32 {
    unsafe {
        process::r4os_rust_program_process_signal();
        if capacity == 0 {
            return 0;
        }
        if out.is_null() {
            return -1;
        }
        let Some(lease) = Lease::id(id) else {
            return -2;
        };
        let instance = lease.0.instance;
        let Some(capture) = flow::capture(instance, None, false) else {
            return 0;
        };
        let reader = view::Reader { instance, capture };
        let frame = reader.capture.frame;
        let start = u64::from(offset);
        let total = life::chain_count(frame, 2);
        if start >= total {
            return 0;
        }
        let count = view::raster_count(total - start, capacity);
        i32::try_from(view::rasters(frame, start, out, count)).expect("Raster read count")
    }
}
/// # Safety
/// Exclusive complete legacy command output under exact captured generation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_api_command(
    id: u32,
    index: u32,
    out: *mut c::GuiCommand,
) -> i32 {
    unsafe {
        process::r4os_rust_program_process_signal();
        let Some(lease) = Lease::id(id) else {
            out.write(Default::default());
            return -1;
        };
        let instance = lease.0.instance;
        let Some(capture) = flow::capture(instance, None, false) else {
            out.write(Default::default());
            return 0;
        };
        let reader = view::Reader { instance, capture };
        let Some(location) = view::location(reader.capture.frame, u64::from(index)) else {
            out.write(Default::default());
            return 0;
        };
        if !view::legacy(location, out) {
            out.write(Default::default());
            return -2;
        }
        1
    }
}
/// # Safety
/// Exact admitted producer instance and readable complete private command.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_append_command(
    instance: *mut ProgramInstance,
    command: *const t::GuiCommand,
) -> i32 {
    unsafe { append_command(instance, command.read()) }
}
