//! Private payload C views retain every measured original field offset.
//! Large allocations initialize in place; no complete payload temporary is
//! constructed on a kernel worker's64KB stack.
use super::local_context;
use crate::{
    contract,
    sched::sync_types::{Mutex, Text, UnwindGuard, WaitQueue},
};
use core::{mem, ptr};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Header {
    pub magic: u32,
    pub owner_id: u32,
    pub requested_bytes: u32,
    pub kind: u16,
    pub reserved: u16,
}

#[repr(C)]
pub struct Runtime {
    pub local_context: local_context::Owner,
    pub r4xstart_context: contract::R4XStartContext,
    pub r4xstart_imports: [contract::R4XStartImport; 16],
    pub header: Header,
    pub r4xstart_import_count: u32,
    pub r4l_code_bindings: [CodeBinding; 16],
    pub module_path_len: u32,
    pub r4xstart_import_module_names: [[u8; 32]; 16],
    pub r4xstart_import_symbol_names: [[u8; 32]; 16],
    pub r4l_code_binding_count: u8,
    pub module_path: [u8; 128],
}

#[repr(C)]
pub struct Environment {
    pub header: Header,
    pub bytes: [u8; 2048],
}

#[repr(C)]
pub struct Process {
    pub environment_payload: *mut Environment,
    pub environment_len: usize,
    pub work_cwd_len: usize,
    pub header: Header,
    pub args: [u8; 128],
    pub work_drive_letter: u8,
    pub work_cwd: [u8; 1024],
}

#[repr(C)]
pub struct ConsoleOutput {
    pub next_sequence: u64,
    pub header: Header,
    pub ref_count: u32,
    pub active_ref_count: u32,
    pub active_accounted: bool,
    pub reserved: [u8; 7],
    pub bytes: [u8; 16384],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ConsoleSegment {
    pub payload: *mut ConsoleOutput,
    pub start_sequence: u64,
    pub length: u32,
    pub reserved: u32,
}

#[repr(C)]
pub struct ConsoleTranscript {
    pub segments: [ConsoleSegment; 256],
    pub header: Header,
    pub segment_count: u32,
    pub output_len: u32,
}

#[repr(C)]
pub struct Console {
    pub transcript_payload: *mut ConsoleTranscript,
    pub writer_payload: *mut ConsoleOutput,
    pub input_head: usize,
    pub input_tail: usize,
    pub input_generation: u64,
    pub input_lock: Mutex,
    pub input_wait: WaitQueue,
    pub io_target_generation: u64,
    pub header: Header,
    pub input_high_water: u32,
    pub revision: u32,
    pub host: u32,
    pub io_target_id: u32,
    pub state: contract::ConsoleState,
    pub input_queue: [u32; 4096],
    pub input_pending_bytes: u32,
    pub text: r4os_text_core::Output,
    pub capture_flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SharedRasterRef {
    pub handle: contract::GuiSharedRasterHandle,
    pub raster_generation: u64,
    pub data_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GuiCommand {
    pub payload_offset: u64,
    pub payload_bytes: u64,
    pub parameter0: u64,
    pub parameter1: u64,
    pub raster_word_offset: u64,
    pub version: u32,
    pub size: u32,
    pub kind: u32,
    pub flags: u32,
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub rgb: u32,
    pub fg: u32,
    pub bg: u32,
    pub font_id: u32,
    pub text_w: u32,
    pub text_h: u32,
    pub baseline: i32,
    pub line_height: u32,
    pub resource_kind: u16,
    pub reserved: u16,
}

pub const EMPTY_COMMAND: GuiCommand = GuiCommand {
    payload_offset: 0,
    payload_bytes: 0,
    parameter0: 0,
    parameter1: 0,
    raster_word_offset: 0,
    version: 1,
    size: 96,
    kind: 0,
    flags: 0,
    x: 0,
    y: 0,
    w: 0,
    h: 0,
    rgb: 0,
    fg: 0,
    bg: 0,
    font_id: 0,
    text_w: 0,
    text_h: 0,
    baseline: 0,
    line_height: 0,
    resource_kind: 0,
    reserved: 0,
};

#[repr(C)]
pub struct Commands {
    pub previous: *mut Commands,
    pub next: *mut Commands,
    pub allocation_sequence: u64,
    pub logical_offset: u64,
    pub header: Header,
    pub command_count: u32,
    pub capacity: u32,
}

#[repr(C)]
pub struct Resource {
    pub previous: *mut Resource,
    pub next: *mut Resource,
    pub allocation_sequence: u64,
    pub logical_offset: u64,
    pub raster_word_offset: u64,
    pub header: Header,
    pub byte_count: u32,
    pub resource_kind: u16,
    pub reserved: u16,
}

#[repr(C)]
pub struct Frame {
    pub command_payload: *mut Commands,
    pub command_tail: *mut Commands,
    pub resource_payload: *mut Resource,
    pub resource_tail: *mut Resource,
    pub retired_next: *mut Frame,
    pub base_frame: *mut Frame,
    pub command_count: u64,
    pub resource_len: u64,
    pub raster_words: u64,
    pub node_sequence: u64,
    pub generation: u64,
    pub header: Header,
    pub damage_regions: [contract::DisplayDamageRect; 8],
    pub damage_count: u32,
    pub chain_depth: u32,
    pub reader_refs: u32,
    pub shared_raster_count: u32,
    pub build_failed: bool,
    pub explicit_build: bool,
    pub replacement: bool,
    pub retired: bool,
    // Requested bytes of this frame and its linked nodes. Private candidate
    // allocations count only after linking; immutable base totals are separate.
    pub local_bytes: u64,
    pub base_bytes: u64,
    pub base_commands: u64,
    pub base_resources: u64,
    pub base_raster_words: u64,
}

#[repr(C)]
pub struct Gui {
    pub frame_oom: u64,
    pub frame_xrgb32_nearest_commands: u64,
    pub frame_lock: Mutex,
    pub committed_frame: *mut Frame,
    pub building_frame: *mut Frame,
    pub retired_frames: *mut Frame,
    pub frame_peak_commands: u64,
    pub frame_peak_bytes: u64,
    pub events: [contract::GuiEvent; 64],
    pub event_head: usize,
    pub event_tail: usize,
    pub frame_snapshot_reads: u64,
    pub frame_avoided_clone_bytes: u64,
    pub frame_stream_peak_bytes: u64,
    pub frame_xrgb32_nearest_resource_bytes: u64,
    pub frame_reader_retired_generations: u64,
    pub frame_coalesced_generations: u64,
    pub frame_superseded_generations: u64,
    pub frame_commits: u64,
    pub frame_cancels: u64,
    pub frame_replacement_commits: u64,
    pub frame_generation_reads: u64,
    pub operation_gate: UnwindGuard,
    pub frame_delta_commits: u64,
    pub frame_full_commits: u64,
    pub frame_indexed8_commands: u64,
    pub frame_indexed8_resource_bytes: u64,
    pub header: Header,
    pub revision: u32,
    pub min_client_h: i32,
    pub min_client_w: i32,
    pub event_high_water: u32,
    pub font_id: u32,
    pub frame_last_error: i32,
    pub window_info: contract::GuiWindowInfo,
    pub window_id: i32,
    pub host_launch_request: contract::ProgramHostLaunchRequest,
    pub title: [u8; 64],
    pub text: [u8; 512],
    pub start_attach_pending: bool,
    pub host_launch_pending: bool,
    // Accepted building/committed/retired roots, excluding private candidates.
    pub frame_owned_bytes: u64,
    pub frame_owned_commands: u64,
    pub activity: super::gui_activity::Channel,
    pub pointer: super::gui_pointer::Window,
    pub input: super::gui_input::Control,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InstanceStorage {
    pub runtime: *mut Runtime,
    pub process: *mut Process,
    pub console: *mut Console,
    pub gui: *mut Gui,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CodeBinding {
    pub generation: u32,
    pub module_slot: u8,
    pub reserved: [u8; 3],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Stats {
    pub gui_frame_cancels: u64,
    pub payload_releases: u64,
    pub live_core_bytes: u64,
    pub active_instance_bytes: u64,
    pub peak_active_instance_bytes: u64,
    pub reserved_instance_bytes: u64,
    pub peak_reserved_instance_bytes: u64,
    pub current_payload_bytes: u64,
    pub peak_payload_bytes: u64,
    pub current_runtime_bytes: u64,
    pub peak_runtime_bytes: u64,
    pub current_console_bytes: u64,
    pub peak_console_bytes: u64,
    pub current_gui_bytes: u64,
    pub peak_gui_bytes: u64,
    pub quarantined_bytes: u64,
    pub quarantined_payloads: u64,
    pub free_failures: u64,
    pub header_errors: u64,
    pub owner_mismatches: u64,
    pub transaction_rollbacks: u64,
    pub allocation_failures: u64,
    pub payload_allocations: u64,
    pub allocation_attempts: u64,
    pub registry_reserved_core_bytes: u64,
    pub gui_frame_snapshot_reads: u64,
    pub core_bytes_per_instance: u64,
    pub gui_frame_oom_failures: u64,
    pub gui_frame_commits: u64,
    pub current_gui_frame_bytes: u64,
    pub peak_gui_frame_bytes: u64,
    pub current_gui_frame_commands: u64,
    pub peak_gui_frame_commands: u64,
    pub current_gui_frame_nodes: u64,
    pub peak_gui_frame_nodes: u64,
    pub active_gui_data_payloads: u32,
    pub active_gui_payloads: u32,
    pub active_gui_raster_payloads: u32,
    pub active_gui_command_payloads: u32,
    pub active_console_output_payloads: u32,
    pub active_console_payloads: u32,
    pub active_gui_frame_payloads: u32,
    pub active_environment_payloads: u32,
    pub active_process_payloads: u32,
    pub active_runtime_payloads: u32,
    pub active_gui_instances: u32,
    pub active_console_instances: u32,
    pub active_service_instances: u32,
    pub active_instances: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SelfTestReport {
    pub peak_payload_bytes: u64,
    pub cases: u32,
    pub failed_case: u32,
    pub heap_baseline_ok: bool,
    pub storage_baseline_ok: bool,
    pub zero_init_ok: bool,
}

pub struct Shape {
    pub size: usize,
    pub align: usize,
    pub header: usize,
}
pub fn shape(kind: u8) -> Shape {
    match kind {
        1 => Shape {
            size: mem::size_of::<Runtime>(),
            align: mem::align_of::<Runtime>(),
            header: mem::offset_of!(Runtime, header),
        },
        2 => Shape {
            size: mem::size_of::<Process>(),
            align: mem::align_of::<Process>(),
            header: mem::offset_of!(Process, header),
        },
        3 => Shape {
            size: mem::size_of::<Environment>(),
            align: mem::align_of::<Environment>(),
            header: mem::offset_of!(Environment, header),
        },
        4 => Shape {
            size: mem::size_of::<Console>(),
            align: mem::align_of::<Console>(),
            header: mem::offset_of!(Console, header),
        },
        5 => Shape {
            size: mem::size_of::<ConsoleOutput>(),
            align: mem::align_of::<ConsoleOutput>(),
            header: mem::offset_of!(ConsoleOutput, header),
        },
        6 => Shape {
            size: mem::size_of::<Gui>(),
            align: mem::align_of::<Gui>(),
            header: mem::offset_of!(Gui, header),
        },
        7 => Shape {
            size: mem::size_of::<Commands>(),
            align: mem::align_of::<Commands>(),
            header: mem::offset_of!(Commands, header),
        },
        8 => Shape {
            size: mem::size_of::<Resource>(),
            align: mem::align_of::<Resource>(),
            header: mem::offset_of!(Resource, header),
        },
        9 => Shape {
            size: mem::size_of::<Frame>(),
            align: mem::align_of::<Frame>(),
            header: mem::offset_of!(Frame, header),
        },
        11 => Shape {
            size: mem::size_of::<ConsoleTranscript>(),
            align: mem::align_of::<ConsoleTranscript>(),
            header: mem::offset_of!(ConsoleTranscript, header),
        },
        _ => unreachable!("Unknown private payload type"),
    }
}
/// # Safety
/// Exact fresh resident allocation for this private type, unpublished and exclusive.
pub unsafe fn initialize(kind: u8, memory: *mut u8) {
    unsafe {
        ptr::write_bytes(memory, 0, shape(kind).size);
        match kind {
            1 => {
                let payload = memory.cast::<Runtime>();
                ptr::addr_of_mut!((*payload).header).write(Header {
                    magic: 0,
                    owner_id: 0,
                    requested_bytes: 0,
                    kind: 1,
                    reserved: 0,
                });
                ptr::addr_of_mut!((*payload).local_context).write(local_context::Owner::default());
                ptr::addr_of_mut!((*payload).r4xstart_context)
                    .write(contract::R4XStartContext::default());
                for index in 0..16 {
                    ptr::addr_of_mut!((*payload).r4xstart_imports)
                        .cast::<contract::R4XStartImport>()
                        .add(index)
                        .write(contract::R4XStartImport::default());
                }
            }
            2 => {
                let payload = memory.cast::<Process>();
                ptr::addr_of_mut!((*payload).header).write(Header {
                    magic: 0,
                    owner_id: 0,
                    requested_bytes: 0,
                    kind: 1,
                    reserved: 0,
                });
                ptr::addr_of_mut!((*payload).work_drive_letter).write(b'C');
                ptr::addr_of_mut!((*payload).work_cwd_len).write(1);
            }
            3 => {
                let payload = memory.cast::<Environment>();
                ptr::addr_of_mut!((*payload).header).write(Header {
                    magic: 0,
                    owner_id: 0,
                    requested_bytes: 0,
                    kind: 1,
                    reserved: 0,
                });
            }
            4 => {
                let payload = memory.cast::<Console>();
                ptr::addr_of_mut!((*payload).header).write(Header {
                    magic: 0,
                    owner_id: 0,
                    requested_bytes: 0,
                    kind: 1,
                    reserved: 0,
                });
                ptr::addr_of_mut!((*payload).input_lock).write(Mutex {
                    rank: 100,
                    mode: 0,
                    name: Text::literal(b"console-input"),
                    ..Mutex::EMPTY
                });
                ptr::addr_of_mut!((*payload).input_wait).write(WaitQueue::EMPTY);
                ptr::addr_of_mut!((*payload).state).write(contract::ConsoleState::default());
                ptr::addr_of_mut!((*payload).text).write(r4os_text_core::Output::EMPTY);
                ptr::addr_of_mut!((*payload).capture_flags).write(0);
            }
            5 => {
                let payload = memory.cast::<ConsoleOutput>();
                ptr::addr_of_mut!((*payload).header).write(Header {
                    magic: 0,
                    owner_id: 0,
                    requested_bytes: 0,
                    kind: 1,
                    reserved: 0,
                });
            }
            6 => {
                let payload = memory.cast::<Gui>();
                ptr::addr_of_mut!((*payload).activity).write(super::gui_activity::Channel::EMPTY);
                ptr::addr_of_mut!((*payload).pointer).write(super::gui_pointer::Window::EMPTY);
                ptr::addr_of_mut!((*payload).input).write(super::gui_input::Control::EMPTY);
                ptr::addr_of_mut!((*payload).operation_gate).write(UnwindGuard {
                    owner: 0,
                    owner_generation: 0,
                    boot_owner: false,
                    depth: 0,
                    name: Text::literal(b"gui-operation"),
                    queue: WaitQueue::EMPTY,
                    ..UnwindGuard::EMPTY
                });
                ptr::addr_of_mut!((*payload).header).write(Header {
                    magic: 0,
                    owner_id: 0,
                    requested_bytes: 0,
                    kind: 1,
                    reserved: 0,
                });
                ptr::addr_of_mut!((*payload).frame_lock).write(Mutex {
                    rank: 100,
                    mode: 1,
                    name: Text::literal(b"r4x-gui-frame"),
                    ..Mutex::EMPTY
                });
                ptr::addr_of_mut!((*payload).window_id).write(-1);
                ptr::addr_of_mut!((*payload).window_info).write(contract::GuiWindowInfo::default());
                for index in 0..64 {
                    ptr::addr_of_mut!((*payload).events)
                        .cast::<contract::GuiEvent>()
                        .add(index)
                        .write(contract::GuiEvent::default());
                }
                ptr::addr_of_mut!((*payload).font_id).write(contract::gui_font_builtin_id);
                ptr::addr_of_mut!((*payload).host_launch_request)
                    .write(contract::ProgramHostLaunchRequest::default());
            }
            7 => {
                let payload = memory.cast::<Commands>();
                ptr::addr_of_mut!((*payload).header).write(Header {
                    magic: 0,
                    owner_id: 0,
                    requested_bytes: 0,
                    kind: 1,
                    reserved: 0,
                });
            }
            8 => {
                let payload = memory.cast::<Resource>();
                ptr::addr_of_mut!((*payload).header).write(Header {
                    magic: 0,
                    owner_id: 0,
                    requested_bytes: 0,
                    kind: 1,
                    reserved: 0,
                });
            }
            9 => {
                let payload = memory.cast::<Frame>();
                ptr::addr_of_mut!((*payload).header).write(Header {
                    magic: 0,
                    owner_id: 0,
                    requested_bytes: 0,
                    kind: 1,
                    reserved: 0,
                });
                for index in 0..8 {
                    ptr::addr_of_mut!((*payload).damage_regions)
                        .cast::<contract::DisplayDamageRect>()
                        .add(index)
                        .write(contract::DisplayDamageRect::default());
                }
                ptr::addr_of_mut!((*payload).chain_depth).write(1);
                ptr::addr_of_mut!((*payload).local_bytes).write(mem::size_of::<Frame>() as u64);
            }
            11 => {
                let payload = memory.cast::<ConsoleTranscript>();
                ptr::addr_of_mut!((*payload).header).write(Header {
                    magic: 0,
                    owner_id: 0,
                    requested_bytes: 0,
                    kind: 1,
                    reserved: 0,
                });
            }
            _ => unreachable!(),
        }
    }
}
pub const EMPTY_STATS: Stats = Stats {
    core_bytes_per_instance: 256,
    registry_reserved_core_bytes: 0,
    live_core_bytes: 0,
    active_instance_bytes: 0,
    peak_active_instance_bytes: 0,
    reserved_instance_bytes: 0,
    peak_reserved_instance_bytes: 0,
    current_payload_bytes: 0,
    peak_payload_bytes: 0,
    current_runtime_bytes: 0,
    peak_runtime_bytes: 0,
    current_console_bytes: 0,
    peak_console_bytes: 0,
    current_gui_bytes: 0,
    peak_gui_bytes: 0,
    active_instances: 0,
    active_service_instances: 0,
    active_console_instances: 0,
    active_gui_instances: 0,
    active_runtime_payloads: 0,
    active_process_payloads: 0,
    active_environment_payloads: 0,
    active_console_payloads: 0,
    active_console_output_payloads: 0,
    active_gui_payloads: 0,
    active_gui_frame_payloads: 0,
    active_gui_command_payloads: 0,
    active_gui_raster_payloads: 0,
    active_gui_data_payloads: 0,
    current_gui_frame_bytes: 0,
    peak_gui_frame_bytes: 0,
    current_gui_frame_commands: 0,
    peak_gui_frame_commands: 0,
    current_gui_frame_nodes: 0,
    peak_gui_frame_nodes: 0,
    gui_frame_commits: 0,
    gui_frame_cancels: 0,
    gui_frame_oom_failures: 0,
    gui_frame_snapshot_reads: 0,
    allocation_attempts: 0,
    payload_allocations: 0,
    payload_releases: 0,
    allocation_failures: 0,
    transaction_rollbacks: 0,
    owner_mismatches: 0,
    header_errors: 0,
    free_failures: 0,
    quarantined_payloads: 0,
    quarantined_bytes: 0,
};
pub const EMPTY_REPORT: SelfTestReport = SelfTestReport {
    cases: 0,
    failed_case: 0,
    peak_payload_bytes: 0,
    heap_baseline_ok: false,
    storage_baseline_ok: false,
    zero_init_ok: false,
};
const _: () = {
    assert!(mem::size_of::<Header>() == 16);
    assert!(mem::align_of::<Header>() == 4);
    assert!(mem::offset_of!(Header, magic) == 0);
    assert!(mem::offset_of!(Header, owner_id) == 4);
    assert!(mem::offset_of!(Header, requested_bytes) == 8);
    assert!(mem::offset_of!(Header, kind) == 12);
    assert!(mem::offset_of!(Header, reserved) == 14);
    assert!(mem::size_of::<Runtime>() == 2112);
    assert!(mem::align_of::<Runtime>() == 8);
    assert!(mem::offset_of!(Runtime, header) == 800);
    assert!(mem::offset_of!(Runtime, local_context) == 0);
    assert!(mem::offset_of!(Runtime, r4xstart_context) == 32);
    assert!(mem::offset_of!(Runtime, r4xstart_imports) == 160);
    assert!(mem::offset_of!(Runtime, r4xstart_import_count) == 816);
    assert!(mem::offset_of!(Runtime, r4xstart_import_module_names) == 952);
    assert!(mem::offset_of!(Runtime, r4xstart_import_symbol_names) == 1464);
    assert!(mem::offset_of!(Runtime, r4l_code_bindings) == 820);
    assert!(mem::offset_of!(Runtime, r4l_code_binding_count) == 1976);
    assert!(mem::offset_of!(Runtime, module_path) == 1977);
    assert!(mem::offset_of!(Runtime, module_path_len) == 948);
    assert!(mem::size_of::<Environment>() == 2064);
    assert!(mem::align_of::<Environment>() == 4);
    assert!(mem::offset_of!(Environment, header) == 0);
    assert!(mem::offset_of!(Environment, bytes) == 16);
    assert!(mem::size_of::<Process>() == 1200);
    assert!(mem::align_of::<Process>() == 8);
    assert!(mem::offset_of!(Process, header) == 24);
    assert!(mem::offset_of!(Process, environment_payload) == 0);
    assert!(mem::offset_of!(Process, environment_len) == 8);
    assert!(mem::offset_of!(Process, args) == 40);
    assert!(mem::offset_of!(Process, work_drive_letter) == 168);
    assert!(mem::offset_of!(Process, work_cwd_len) == 16);
    assert!(mem::offset_of!(Process, work_cwd) == 169);
    assert!(mem::size_of::<ConsoleOutput>() == 16424);
    assert!(mem::align_of::<ConsoleOutput>() == 8);
    assert!(mem::offset_of!(ConsoleOutput, header) == 8);
    assert!(mem::offset_of!(ConsoleOutput, ref_count) == 24);
    assert!(mem::offset_of!(ConsoleOutput, active_ref_count) == 28);
    assert!(mem::offset_of!(ConsoleOutput, next_sequence) == 0);
    assert!(mem::offset_of!(ConsoleOutput, active_accounted) == 32);
    assert!(mem::offset_of!(ConsoleOutput, reserved) == 33);
    assert!(mem::offset_of!(ConsoleOutput, bytes) == 40);
    assert!(mem::size_of::<ConsoleSegment>() == 24);
    assert!(mem::align_of::<ConsoleSegment>() == 8);
    assert!(mem::offset_of!(ConsoleSegment, payload) == 0);
    assert!(mem::offset_of!(ConsoleSegment, start_sequence) == 8);
    assert!(mem::offset_of!(ConsoleSegment, length) == 16);
    assert!(mem::offset_of!(ConsoleSegment, reserved) == 20);
    assert!(mem::size_of::<ConsoleTranscript>() == 6168);
    assert!(mem::align_of::<ConsoleTranscript>() == 8);
    assert!(mem::offset_of!(ConsoleTranscript, header) == 6144);
    assert!(mem::offset_of!(ConsoleTranscript, segment_count) == 6160);
    assert!(mem::offset_of!(ConsoleTranscript, output_len) == 6164);
    assert!(mem::offset_of!(ConsoleTranscript, segments) == 0);
    assert!(mem::offset_of!(Console, text) == 16700);
    assert!(mem::size_of::<Console>() == 16736);
    assert!(mem::offset_of!(Console, capture_flags) == 16728);
    assert!(mem::align_of::<Console>() == 8);
    assert!(mem::offset_of!(Console, header) == 216);
    assert!(mem::offset_of!(Console, transcript_payload) == 0);
    assert!(mem::offset_of!(Console, writer_payload) == 8);
    assert!(mem::offset_of!(Console, input_queue) == 312);
    assert!(mem::offset_of!(Console, input_pending_bytes) == 16696);
    assert!(mem::offset_of!(Console, input_head) == 16);
    assert!(mem::offset_of!(Console, input_tail) == 24);
    assert!(mem::offset_of!(Console, input_high_water) == 232);
    assert!(mem::offset_of!(Console, input_generation) == 32);
    assert!(mem::offset_of!(Console, input_lock) == 40);
    assert!(mem::offset_of!(Console, input_wait) == 168);
    assert!(mem::offset_of!(Console, revision) == 236);
    assert!(mem::offset_of!(Console, host) == 240);
    assert!(mem::offset_of!(Console, io_target_id) == 244);
    assert!(mem::offset_of!(Console, io_target_generation) == 208);
    assert!(mem::offset_of!(Console, state) == 248);
    assert!(mem::size_of::<SharedRasterRef>() == 32);
    assert!(mem::align_of::<SharedRasterRef>() == 8);
    assert!(mem::offset_of!(SharedRasterRef, handle) == 0);
    assert!(mem::offset_of!(SharedRasterRef, raster_generation) == 16);
    assert!(mem::offset_of!(SharedRasterRef, data_bytes) == 24);
    assert!(mem::size_of::<GuiCommand>() == 112);
    assert!(mem::align_of::<GuiCommand>() == 8);
    assert!(mem::offset_of!(GuiCommand, version) == 40);
    assert!(mem::offset_of!(GuiCommand, size) == 44);
    assert!(mem::offset_of!(GuiCommand, kind) == 48);
    assert!(mem::offset_of!(GuiCommand, flags) == 52);
    assert!(mem::offset_of!(GuiCommand, x) == 56);
    assert!(mem::offset_of!(GuiCommand, y) == 60);
    assert!(mem::offset_of!(GuiCommand, w) == 64);
    assert!(mem::offset_of!(GuiCommand, h) == 68);
    assert!(mem::offset_of!(GuiCommand, rgb) == 72);
    assert!(mem::offset_of!(GuiCommand, fg) == 76);
    assert!(mem::offset_of!(GuiCommand, bg) == 80);
    assert!(mem::offset_of!(GuiCommand, font_id) == 84);
    assert!(mem::offset_of!(GuiCommand, text_w) == 88);
    assert!(mem::offset_of!(GuiCommand, text_h) == 92);
    assert!(mem::offset_of!(GuiCommand, baseline) == 96);
    assert!(mem::offset_of!(GuiCommand, line_height) == 100);
    assert!(mem::offset_of!(GuiCommand, payload_offset) == 0);
    assert!(mem::offset_of!(GuiCommand, payload_bytes) == 8);
    assert!(mem::offset_of!(GuiCommand, parameter0) == 16);
    assert!(mem::offset_of!(GuiCommand, parameter1) == 24);
    assert!(mem::offset_of!(GuiCommand, resource_kind) == 104);
    assert!(mem::offset_of!(GuiCommand, reserved) == 106);
    assert!(mem::offset_of!(GuiCommand, raster_word_offset) == 32);
    assert!(mem::size_of::<Commands>() == 56);
    assert!(mem::align_of::<Commands>() == 8);
    assert!(mem::offset_of!(Commands, header) == 32);
    assert!(mem::offset_of!(Commands, previous) == 0);
    assert!(mem::offset_of!(Commands, next) == 8);
    assert!(mem::offset_of!(Commands, allocation_sequence) == 16);
    assert!(mem::offset_of!(Commands, logical_offset) == 24);
    assert!(mem::offset_of!(Commands, command_count) == 48);
    assert!(mem::offset_of!(Commands, capacity) == 52);
    assert!(mem::size_of::<Resource>() == 64);
    assert!(mem::align_of::<Resource>() == 8);
    assert!(mem::offset_of!(Resource, header) == 40);
    assert!(mem::offset_of!(Resource, previous) == 0);
    assert!(mem::offset_of!(Resource, next) == 8);
    assert!(mem::offset_of!(Resource, allocation_sequence) == 16);
    assert!(mem::offset_of!(Resource, logical_offset) == 24);
    assert!(mem::offset_of!(Resource, raster_word_offset) == 32);
    assert!(mem::offset_of!(Resource, byte_count) == 56);
    assert!(mem::offset_of!(Resource, resource_kind) == 60);
    assert!(mem::offset_of!(Resource, reserved) == 62);
    assert!(mem::size_of::<Frame>() == 296);
    assert!(mem::align_of::<Frame>() == 8);
    assert!(mem::offset_of!(Frame, header) == 88);
    assert!(mem::offset_of!(Frame, command_payload) == 0);
    assert!(mem::offset_of!(Frame, command_tail) == 8);
    assert!(mem::offset_of!(Frame, resource_payload) == 16);
    assert!(mem::offset_of!(Frame, resource_tail) == 24);
    assert!(mem::offset_of!(Frame, retired_next) == 32);
    assert!(mem::offset_of!(Frame, base_frame) == 40);
    assert!(mem::offset_of!(Frame, command_count) == 48);
    assert!(mem::offset_of!(Frame, resource_len) == 56);
    assert!(mem::offset_of!(Frame, raster_words) == 64);
    assert!(mem::offset_of!(Frame, node_sequence) == 72);
    assert!(mem::offset_of!(Frame, generation) == 80);
    assert!(mem::offset_of!(Frame, damage_regions) == 104);
    assert!(mem::offset_of!(Frame, damage_count) == 232);
    assert!(mem::offset_of!(Frame, chain_depth) == 236);
    assert!(mem::offset_of!(Frame, reader_refs) == 240);
    assert!(mem::offset_of!(Frame, shared_raster_count) == 244);
    assert!(mem::offset_of!(Frame, build_failed) == 248);
    assert!(mem::offset_of!(Frame, explicit_build) == 249);
    assert!(mem::offset_of!(Frame, replacement) == 250);
    assert!(mem::offset_of!(Frame, retired) == 251);
    assert!(mem::offset_of!(Frame, local_bytes) == 256);
    assert!(mem::offset_of!(Frame, base_bytes) == 264);
    assert!(mem::offset_of!(Frame, base_commands) == 272);
    assert!(mem::offset_of!(Frame, base_resources) == 280);
    assert!(mem::offset_of!(Frame, base_raster_words) == 288);
    assert!(mem::size_of::<Gui>() == 4336);
    assert!(mem::offset_of!(Gui, input) == 4280);
    assert!(mem::offset_of!(Gui, pointer) == 4032);
    assert!(mem::size_of::<super::gui_pointer::Window>() == 248);
    assert!(mem::offset_of!(Gui, activity) == 3960);
    assert!(mem::align_of::<Gui>() == 8);
    assert!(mem::offset_of!(Gui, operation_gate) == 2848);
    assert!(mem::offset_of!(Gui, header) == 3016);
    assert!(mem::offset_of!(Gui, frame_lock) == 16);
    assert!(mem::offset_of!(Gui, committed_frame) == 144);
    assert!(mem::offset_of!(Gui, building_frame) == 152);
    assert!(mem::offset_of!(Gui, retired_frames) == 160);
    assert!(mem::offset_of!(Gui, window_id) == 3096);
    assert!(mem::offset_of!(Gui, window_info) == 3056);
    assert!(mem::offset_of!(Gui, events) == 184);
    assert!(mem::offset_of!(Gui, event_head) == 2744);
    assert!(mem::offset_of!(Gui, event_tail) == 2752);
    assert!(mem::offset_of!(Gui, event_high_water) == 3044);
    assert!(mem::offset_of!(Gui, text) == 3428);
    assert!(mem::offset_of!(Gui, font_id) == 3048);
    assert!(mem::offset_of!(Gui, title) == 3364);
    assert!(mem::offset_of!(Gui, min_client_w) == 3040);
    assert!(mem::offset_of!(Gui, min_client_h) == 3036);
    assert!(mem::offset_of!(Gui, revision) == 3032);
    assert!(mem::offset_of!(Gui, frame_commits) == 2816);
    assert!(mem::offset_of!(Gui, frame_cancels) == 2824);
    assert!(mem::offset_of!(Gui, frame_oom) == 0);
    assert!(mem::offset_of!(Gui, frame_snapshot_reads) == 2760);
    assert!(mem::offset_of!(Gui, frame_generation_reads) == 2840);
    assert!(mem::offset_of!(Gui, frame_delta_commits) == 2984);
    assert!(mem::offset_of!(Gui, frame_full_commits) == 2992);
    assert!(mem::offset_of!(Gui, frame_indexed8_commands) == 3000);
    assert!(mem::offset_of!(Gui, frame_indexed8_resource_bytes) == 3008);
    assert!(mem::offset_of!(Gui, frame_replacement_commits) == 2832);
    assert!(mem::offset_of!(Gui, frame_superseded_generations) == 2808);
    assert!(mem::offset_of!(Gui, frame_coalesced_generations) == 2800);
    assert!(mem::offset_of!(Gui, frame_reader_retired_generations) == 2792);
    assert!(mem::offset_of!(Gui, frame_xrgb32_nearest_commands) == 8);
    assert!(mem::offset_of!(Gui, frame_xrgb32_nearest_resource_bytes) == 2784);
    assert!(mem::offset_of!(Gui, frame_stream_peak_bytes) == 2776);
    assert!(mem::offset_of!(Gui, frame_avoided_clone_bytes) == 2768);
    assert!(mem::offset_of!(Gui, frame_last_error) == 3052);
    assert!(mem::offset_of!(Gui, frame_peak_bytes) == 176);
    assert!(mem::offset_of!(Gui, frame_peak_commands) == 168);
    assert!(mem::offset_of!(Gui, start_attach_pending) == 3940);
    assert!(mem::offset_of!(Gui, host_launch_pending) == 3941);
    assert!(mem::offset_of!(Gui, frame_owned_bytes) == 3944);
    assert!(mem::offset_of!(Gui, frame_owned_commands) == 3952);
    assert!(mem::offset_of!(Gui, host_launch_request) == 3100);
    assert!(mem::size_of::<InstanceStorage>() == 32);
    assert!(mem::align_of::<InstanceStorage>() == 8);
    assert!(mem::offset_of!(InstanceStorage, runtime) == 0);
    assert!(mem::offset_of!(InstanceStorage, process) == 8);
    assert!(mem::offset_of!(InstanceStorage, console) == 16);
    assert!(mem::offset_of!(InstanceStorage, gui) == 24);
    assert!(mem::size_of::<CodeBinding>() == 8);
    assert!(mem::align_of::<CodeBinding>() == 4);
    assert!(mem::offset_of!(CodeBinding, module_slot) == 4);
    assert!(mem::offset_of!(CodeBinding, reserved) == 5);
    assert!(mem::offset_of!(CodeBinding, generation) == 0);
    assert!(mem::size_of::<Stats>() == 336);
    assert!(mem::align_of::<Stats>() == 8);
    assert!(mem::offset_of!(Stats, core_bytes_per_instance) == 208);
    assert!(mem::offset_of!(Stats, registry_reserved_core_bytes) == 192);
    assert!(mem::offset_of!(Stats, live_core_bytes) == 16);
    assert!(mem::offset_of!(Stats, active_instance_bytes) == 24);
    assert!(mem::offset_of!(Stats, peak_active_instance_bytes) == 32);
    assert!(mem::offset_of!(Stats, reserved_instance_bytes) == 40);
    assert!(mem::offset_of!(Stats, peak_reserved_instance_bytes) == 48);
    assert!(mem::offset_of!(Stats, current_payload_bytes) == 56);
    assert!(mem::offset_of!(Stats, peak_payload_bytes) == 64);
    assert!(mem::offset_of!(Stats, current_runtime_bytes) == 72);
    assert!(mem::offset_of!(Stats, peak_runtime_bytes) == 80);
    assert!(mem::offset_of!(Stats, current_console_bytes) == 88);
    assert!(mem::offset_of!(Stats, peak_console_bytes) == 96);
    assert!(mem::offset_of!(Stats, current_gui_bytes) == 104);
    assert!(mem::offset_of!(Stats, peak_gui_bytes) == 112);
    assert!(mem::offset_of!(Stats, active_instances) == 332);
    assert!(mem::offset_of!(Stats, active_service_instances) == 328);
    assert!(mem::offset_of!(Stats, active_console_instances) == 324);
    assert!(mem::offset_of!(Stats, active_gui_instances) == 320);
    assert!(mem::offset_of!(Stats, active_runtime_payloads) == 316);
    assert!(mem::offset_of!(Stats, active_process_payloads) == 312);
    assert!(mem::offset_of!(Stats, active_environment_payloads) == 308);
    assert!(mem::offset_of!(Stats, active_console_payloads) == 300);
    assert!(mem::offset_of!(Stats, active_console_output_payloads) == 296);
    assert!(mem::offset_of!(Stats, active_gui_payloads) == 284);
    assert!(mem::offset_of!(Stats, active_gui_frame_payloads) == 304);
    assert!(mem::offset_of!(Stats, active_gui_command_payloads) == 292);
    assert!(mem::offset_of!(Stats, active_gui_raster_payloads) == 288);
    assert!(mem::offset_of!(Stats, active_gui_data_payloads) == 280);
    assert!(mem::offset_of!(Stats, current_gui_frame_bytes) == 232);
    assert!(mem::offset_of!(Stats, peak_gui_frame_bytes) == 240);
    assert!(mem::offset_of!(Stats, current_gui_frame_commands) == 248);
    assert!(mem::offset_of!(Stats, peak_gui_frame_commands) == 256);
    assert!(mem::offset_of!(Stats, current_gui_frame_nodes) == 264);
    assert!(mem::offset_of!(Stats, peak_gui_frame_nodes) == 272);
    assert!(mem::offset_of!(Stats, gui_frame_commits) == 224);
    assert!(mem::offset_of!(Stats, gui_frame_cancels) == 0);
    assert!(mem::offset_of!(Stats, gui_frame_oom_failures) == 216);
    assert!(mem::offset_of!(Stats, gui_frame_snapshot_reads) == 200);
    assert!(mem::offset_of!(Stats, allocation_attempts) == 184);
    assert!(mem::offset_of!(Stats, payload_allocations) == 176);
    assert!(mem::offset_of!(Stats, payload_releases) == 8);
    assert!(mem::offset_of!(Stats, allocation_failures) == 168);
    assert!(mem::offset_of!(Stats, transaction_rollbacks) == 160);
    assert!(mem::offset_of!(Stats, owner_mismatches) == 152);
    assert!(mem::offset_of!(Stats, header_errors) == 144);
    assert!(mem::offset_of!(Stats, free_failures) == 136);
    assert!(mem::offset_of!(Stats, quarantined_payloads) == 128);
    assert!(mem::offset_of!(Stats, quarantined_bytes) == 120);
    assert!(mem::size_of::<SelfTestReport>() == 24);
    assert!(mem::align_of::<SelfTestReport>() == 8);
    assert!(mem::offset_of!(SelfTestReport, cases) == 8);
    assert!(mem::offset_of!(SelfTestReport, failed_case) == 12);
    assert!(mem::offset_of!(SelfTestReport, peak_payload_bytes) == 0);
    assert!(mem::offset_of!(SelfTestReport, heap_baseline_ok) == 16);
    assert!(mem::offset_of!(SelfTestReport, storage_baseline_ok) == 17);
    assert!(mem::offset_of!(SelfTestReport, zero_init_ok) == 18);
};
