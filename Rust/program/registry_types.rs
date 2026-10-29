//! Stable program registry C views; native admission borrows exact records.
//! Native consumers check every field offset and the original core budget.
//! Every record retains its stable allocation and exact generation until
//! original task/payload/image/VM/DMA/completion release acknowledgments.
//! Notifications share the native process namespace; opaque payload pointers
//! confer neither Rust references nor cleanup rights to another owner.
use super::{memory, notifications};
use crate::contract;
use core::ffi::c_void;

#[repr(C)]
pub struct ProgramInstance {
    pub program_stack_guard_size: u64,
    pub memory_limits: memory::Limits,
    pub gui_payload: *mut c_void,
    pub notifications: notifications::Owner,
    pub console_payload: *mut c_void,
    pub process_payload: *mut c_void,
    pub runtime_payload: *mut c_void,
    pub program_stack_telemetry_high_water: u64,
    pub program_stack_create_cycles: u64,
    pub entry: *const c_void,
    pub stack_top: u64,
    pub program_stack_initial_commit_size: u64,
    pub program_image_base: u64,
    pub program_image_size: usize,
    pub program_stack_guard_base: u64,
    pub program_stack_base: u64,
    pub program_stack_reserve_size: u64,
    pub program_stack_committed_base: u64,
    pub program_stack_committed_size: u64,
    pub program_image_range_id: u32,
    pub program_stack_telemetry_committed_pages: u32,
    pub program_stack_range_id: u32,
    pub task_id: u32,
    pub id: u32,
    pub exit_code: i32,
    pub desktop_host: bool,
    pub program_stack_serial_telemetry: bool,
    pub app_class: u8,
    pub role: u8,
    pub parallel_execution: bool,
    pub program_stack_telemetry_measured: bool,
    pub memory_profile: u8,
    pub memory_tag: [u8; 16],
    pub used: bool,
    pub gfx_virtual_closed: bool,
    pub display_used: bool,
    pub close_requested: bool,
    pub desktop_requested: bool,
    pub done: bool,
    pub storage_teardown_blocked: bool,
    pub serial_closed: bool,
    pub parallel_workers: bool,
}

#[repr(C)]
pub struct ProgramRegistrySlot {
    pub generation: u64,
    pub retire_next: *mut ProgramRegistrySlot,
    pub completion: *mut ProgramCompletionNode,
    pub instance: ProgramInstance,
    pub public_id: u32,
    pub pin_count: u32,
    pub retire_attempts: u32,
    pub state: u8,
    pub reclaim_pending: bool,
    pub retire_queued: bool,
    pub retire_in_progress: bool,
    pub cancel_pending: bool,
    pub retire_phase: u8,
    pub retire_output_detached: bool,
    pub retire_storage_released: bool,
    pub retire_image_stack_released: bool,
    pub retire_owner_released: bool,
    _padding0: [u8; 2],
}

#[repr(C)]
pub struct ProgramRegistryChunk {
    pub next: *mut ProgramRegistryChunk,
    pub serial: u64,
    pub empty_since_epoch: u64,
    pub slots: [ProgramRegistrySlot; 16],
}

#[repr(C)]
pub struct ProgramInstanceReservation {
    pub slot: *mut ProgramRegistrySlot,
    pub generation: u64,
    pub id: u32,
    _padding0: [u8; 4],
}

#[repr(C)]
pub struct ProgramInstanceLease {
    pub slot: *mut ProgramRegistrySlot,
    pub instance: *mut ProgramInstance,
    pub generation: u64,
    pub id: u32,
    _padding0: [u8; 4],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ProgramRegistryStats {
    pub id_wraps: u64,
    pub live_id_hash: u64,
    pub launch_failures: u64,
    pub retire_retries: u64,
    pub completion_peak: u64,
    pub live_address_hash: u64,
    pub history_sequence: u64,
    pub history_count: u64,
    pub history_head: u64,
    pub peak_chunks: u64,
    pub peak_live: u64,
    pub growth_attempts: u64,
    pub growth_failures: u64,
    pub forced_failures: u64,
    pub publish_count: u64,
    pub rollback_count: u64,
    pub shrink_count: u64,
    pub id_collisions: u64,
    pub retire_deferred: u64,
    pub retire_queued: u64,
    pub completion_output_bytes: u64,
    pub stale_lease_rejections: u64,
    pub completion_ready: u64,
    pub completion_pending: u64,
    pub next_generation: u64,
    pub last_admission_error: i32,
    pub live_slots: u32,
    pub chunk_slots: u32,
    pub chunk_count: u32,
    pub pinned_slots: u32,
    pub retiring_slots: u32,
    pub done_slots: u32,
    pub reserved_slots: u32,
    pub free_slots: u32,
    pub slot_capacity: u32,
    pub last_launch_error: i32,
    pub failure_armed: bool,
    _padding0: [u8; 3],
}

#[repr(C)]
pub struct ProgramCompletionNode {
    pub next: *mut ProgramCompletionNode,
    pub handle: contract::ProgramProcessHandle,
    pub owner_handle: contract::ProgramProcessHandle,
    pub sequence: u64,
    pub start_tick: u64,
    pub finish_tick: u64,
    pub output_payload: *mut c_void,
    pub output_storage_bytes: u64,
    pub exit_code: i32,
    pub task_id: u32,
    pub output_revision: u32,
    pub output_length: u32,
    pub flags: u32,
    pub console_state: contract::ConsoleState,
    pub state: u8,
    pub slot_attached: bool,
    pub owner: bool,
    pub legacy_id: bool,
    pub retain_output: bool,
    pub app_class: u8,
    pub role: u8,
    pub exit_reason: u8,
    pub observation_refs: u32,
    pub text_info: contract::ConsoleTextInfo,
}

/// One independent observer binding; its target is completion storage only.
#[repr(C)]
pub struct ProgramObservationNode {
    pub next: *mut ProgramObservationNode,
    pub key: contract::ProgramObservationHandle,
    pub owner: contract::ProgramProcessHandle,
    pub completion: *mut ProgramCompletionNode,
}

#[repr(C)]
pub struct ExitRecord {
    pub handle: contract::ProgramProcessHandle,
    pub sequence: u64,
    pub start_tick: u64,
    pub finish_tick: u64,
    pub exit_code: i32,
    pub used: bool,
    pub app_class: u8,
    pub role: u8,
    pub exit_reason: u8,
}

const _: () = {
    assert!(core::mem::offset_of!(ProgramInstance, program_stack_guard_size) == 0);
    assert!(core::mem::offset_of!(ProgramInstance, memory_limits) == 8);
    assert!(core::mem::offset_of!(ProgramInstance, gui_payload) == 48);
    assert!(core::mem::offset_of!(ProgramInstance, notifications) == 56);
    assert!(core::mem::offset_of!(ProgramInstance, console_payload) == 80);
    assert!(core::mem::offset_of!(ProgramInstance, process_payload) == 88);
    assert!(core::mem::offset_of!(ProgramInstance, runtime_payload) == 96);
    assert!(core::mem::offset_of!(ProgramInstance, program_stack_telemetry_high_water) == 104);
    assert!(core::mem::offset_of!(ProgramInstance, program_stack_create_cycles) == 112);
    assert!(core::mem::offset_of!(ProgramInstance, entry) == 120);
    assert!(core::mem::offset_of!(ProgramInstance, stack_top) == 128);
    assert!(core::mem::offset_of!(ProgramInstance, program_stack_initial_commit_size) == 136);
    assert!(core::mem::offset_of!(ProgramInstance, program_image_base) == 144);
    assert!(core::mem::offset_of!(ProgramInstance, program_image_size) == 152);
    assert!(core::mem::offset_of!(ProgramInstance, program_stack_guard_base) == 160);
    assert!(core::mem::offset_of!(ProgramInstance, program_stack_base) == 168);
    assert!(core::mem::offset_of!(ProgramInstance, program_stack_reserve_size) == 176);
    assert!(core::mem::offset_of!(ProgramInstance, program_stack_committed_base) == 184);
    assert!(core::mem::offset_of!(ProgramInstance, program_stack_committed_size) == 192);
    assert!(core::mem::offset_of!(ProgramInstance, program_image_range_id) == 200);
    assert!(core::mem::offset_of!(ProgramInstance, program_stack_telemetry_committed_pages) == 204);
    assert!(core::mem::offset_of!(ProgramInstance, program_stack_range_id) == 208);
    assert!(core::mem::offset_of!(ProgramInstance, task_id) == 212);
    assert!(core::mem::offset_of!(ProgramInstance, id) == 216);
    assert!(core::mem::offset_of!(ProgramInstance, exit_code) == 220);
    assert!(core::mem::offset_of!(ProgramInstance, desktop_host) == 224);
    assert!(core::mem::offset_of!(ProgramInstance, program_stack_serial_telemetry) == 225);
    assert!(core::mem::offset_of!(ProgramInstance, app_class) == 226);
    assert!(core::mem::offset_of!(ProgramInstance, role) == 227);
    assert!(core::mem::offset_of!(ProgramInstance, parallel_execution) == 228);
    assert!(core::mem::offset_of!(ProgramInstance, program_stack_telemetry_measured) == 229);
    assert!(core::mem::offset_of!(ProgramInstance, memory_profile) == 230);
    assert!(core::mem::offset_of!(ProgramInstance, memory_tag) == 231);
    assert!(core::mem::offset_of!(ProgramInstance, used) == 247);
    assert!(core::mem::offset_of!(ProgramInstance, gfx_virtual_closed) == 248);
    assert!(core::mem::offset_of!(ProgramInstance, serial_closed) == 254);
    assert!(core::mem::offset_of!(ProgramInstance, display_used) == 249);
    assert!(core::mem::offset_of!(ProgramInstance, close_requested) == 250);
    assert!(core::mem::offset_of!(ProgramInstance, desktop_requested) == 251);
    assert!(core::mem::offset_of!(ProgramInstance, done) == 252);
    assert!(core::mem::offset_of!(ProgramInstance, storage_teardown_blocked) == 253);
    assert!(core::mem::offset_of!(ProgramInstance, parallel_workers) == 255);
    assert!(core::mem::size_of::<ProgramInstance>() == 256);
    assert!(core::mem::align_of::<ProgramInstance>() == 8);
    assert!(core::mem::offset_of!(ProgramRegistrySlot, generation) == 0);
    assert!(core::mem::offset_of!(ProgramRegistrySlot, retire_next) == 8);
    assert!(core::mem::offset_of!(ProgramRegistrySlot, completion) == 16);
    assert!(core::mem::offset_of!(ProgramRegistrySlot, instance) == 24);
    assert!(core::mem::offset_of!(ProgramRegistrySlot, public_id) == 280);
    assert!(core::mem::offset_of!(ProgramRegistrySlot, pin_count) == 284);
    assert!(core::mem::offset_of!(ProgramRegistrySlot, retire_attempts) == 288);
    assert!(core::mem::offset_of!(ProgramRegistrySlot, state) == 292);
    assert!(core::mem::offset_of!(ProgramRegistrySlot, reclaim_pending) == 293);
    assert!(core::mem::offset_of!(ProgramRegistrySlot, retire_queued) == 294);
    assert!(core::mem::offset_of!(ProgramRegistrySlot, retire_in_progress) == 295);
    assert!(core::mem::offset_of!(ProgramRegistrySlot, cancel_pending) == 296);
    assert!(core::mem::offset_of!(ProgramRegistrySlot, retire_phase) == 297);
    assert!(core::mem::offset_of!(ProgramRegistrySlot, retire_output_detached) == 298);
    assert!(core::mem::offset_of!(ProgramRegistrySlot, retire_storage_released) == 299);
    assert!(core::mem::offset_of!(ProgramRegistrySlot, retire_image_stack_released) == 300);
    assert!(core::mem::offset_of!(ProgramRegistrySlot, retire_owner_released) == 301);
    assert!(core::mem::size_of::<ProgramRegistrySlot>() == 304);
    assert!(core::mem::align_of::<ProgramRegistrySlot>() == 8);
    assert!(core::mem::offset_of!(ProgramRegistryChunk, next) == 0);
    assert!(core::mem::offset_of!(ProgramRegistryChunk, serial) == 8);
    assert!(core::mem::offset_of!(ProgramRegistryChunk, empty_since_epoch) == 16);
    assert!(core::mem::offset_of!(ProgramRegistryChunk, slots) == 24);
    assert!(core::mem::size_of::<ProgramRegistryChunk>() == 4888);
    assert!(core::mem::align_of::<ProgramRegistryChunk>() == 8);
    assert!(core::mem::offset_of!(ProgramInstanceReservation, slot) == 0);
    assert!(core::mem::offset_of!(ProgramInstanceReservation, generation) == 8);
    assert!(core::mem::offset_of!(ProgramInstanceReservation, id) == 16);
    assert!(core::mem::size_of::<ProgramInstanceReservation>() == 24);
    assert!(core::mem::align_of::<ProgramInstanceReservation>() == 8);
    assert!(core::mem::offset_of!(ProgramInstanceLease, slot) == 0);
    assert!(core::mem::offset_of!(ProgramInstanceLease, instance) == 8);
    assert!(core::mem::offset_of!(ProgramInstanceLease, generation) == 16);
    assert!(core::mem::offset_of!(ProgramInstanceLease, id) == 24);
    assert!(core::mem::size_of::<ProgramInstanceLease>() == 32);
    assert!(core::mem::align_of::<ProgramInstanceLease>() == 8);
    assert!(core::mem::offset_of!(ProgramRegistryStats, id_wraps) == 0);
    assert!(core::mem::offset_of!(ProgramRegistryStats, live_id_hash) == 8);
    assert!(core::mem::offset_of!(ProgramRegistryStats, launch_failures) == 16);
    assert!(core::mem::offset_of!(ProgramRegistryStats, retire_retries) == 24);
    assert!(core::mem::offset_of!(ProgramRegistryStats, completion_peak) == 32);
    assert!(core::mem::offset_of!(ProgramRegistryStats, live_address_hash) == 40);
    assert!(core::mem::offset_of!(ProgramRegistryStats, history_sequence) == 48);
    assert!(core::mem::offset_of!(ProgramRegistryStats, history_count) == 56);
    assert!(core::mem::offset_of!(ProgramRegistryStats, history_head) == 64);
    assert!(core::mem::offset_of!(ProgramRegistryStats, peak_chunks) == 72);
    assert!(core::mem::offset_of!(ProgramRegistryStats, peak_live) == 80);
    assert!(core::mem::offset_of!(ProgramRegistryStats, growth_attempts) == 88);
    assert!(core::mem::offset_of!(ProgramRegistryStats, growth_failures) == 96);
    assert!(core::mem::offset_of!(ProgramRegistryStats, forced_failures) == 104);
    assert!(core::mem::offset_of!(ProgramRegistryStats, publish_count) == 112);
    assert!(core::mem::offset_of!(ProgramRegistryStats, rollback_count) == 120);
    assert!(core::mem::offset_of!(ProgramRegistryStats, shrink_count) == 128);
    assert!(core::mem::offset_of!(ProgramRegistryStats, id_collisions) == 136);
    assert!(core::mem::offset_of!(ProgramRegistryStats, retire_deferred) == 144);
    assert!(core::mem::offset_of!(ProgramRegistryStats, retire_queued) == 152);
    assert!(core::mem::offset_of!(ProgramRegistryStats, completion_output_bytes) == 160);
    assert!(core::mem::offset_of!(ProgramRegistryStats, stale_lease_rejections) == 168);
    assert!(core::mem::offset_of!(ProgramRegistryStats, completion_ready) == 176);
    assert!(core::mem::offset_of!(ProgramRegistryStats, completion_pending) == 184);
    assert!(core::mem::offset_of!(ProgramRegistryStats, next_generation) == 192);
    assert!(core::mem::offset_of!(ProgramRegistryStats, last_admission_error) == 200);
    assert!(core::mem::offset_of!(ProgramRegistryStats, live_slots) == 204);
    assert!(core::mem::offset_of!(ProgramRegistryStats, chunk_slots) == 208);
    assert!(core::mem::offset_of!(ProgramRegistryStats, chunk_count) == 212);
    assert!(core::mem::offset_of!(ProgramRegistryStats, pinned_slots) == 216);
    assert!(core::mem::offset_of!(ProgramRegistryStats, retiring_slots) == 220);
    assert!(core::mem::offset_of!(ProgramRegistryStats, done_slots) == 224);
    assert!(core::mem::offset_of!(ProgramRegistryStats, reserved_slots) == 228);
    assert!(core::mem::offset_of!(ProgramRegistryStats, free_slots) == 232);
    assert!(core::mem::offset_of!(ProgramRegistryStats, slot_capacity) == 236);
    assert!(core::mem::offset_of!(ProgramRegistryStats, last_launch_error) == 240);
    assert!(core::mem::offset_of!(ProgramRegistryStats, failure_armed) == 244);
    assert!(core::mem::size_of::<ProgramRegistryStats>() == 248);
    assert!(core::mem::align_of::<ProgramRegistryStats>() == 8);
    assert!(core::mem::offset_of!(ProgramCompletionNode, next) == 0);
    assert!(core::mem::offset_of!(ProgramCompletionNode, handle) == 8);
    assert!(core::mem::offset_of!(ProgramCompletionNode, owner_handle) == 24);
    assert!(core::mem::offset_of!(ProgramCompletionNode, sequence) == 40);
    assert!(core::mem::offset_of!(ProgramCompletionNode, start_tick) == 48);
    assert!(core::mem::offset_of!(ProgramCompletionNode, finish_tick) == 56);
    assert!(core::mem::offset_of!(ProgramCompletionNode, output_payload) == 64);
    assert!(core::mem::offset_of!(ProgramCompletionNode, output_storage_bytes) == 72);
    assert!(core::mem::offset_of!(ProgramCompletionNode, exit_code) == 80);
    assert!(core::mem::offset_of!(ProgramCompletionNode, task_id) == 84);
    assert!(core::mem::offset_of!(ProgramCompletionNode, output_revision) == 88);
    assert!(core::mem::offset_of!(ProgramCompletionNode, output_length) == 92);
    assert!(core::mem::offset_of!(ProgramCompletionNode, flags) == 96);
    assert!(core::mem::offset_of!(ProgramCompletionNode, console_state) == 100);
    assert!(core::mem::offset_of!(ProgramCompletionNode, state) == 164);
    assert!(core::mem::offset_of!(ProgramCompletionNode, slot_attached) == 165);
    assert!(core::mem::offset_of!(ProgramCompletionNode, owner) == 166);
    assert!(core::mem::offset_of!(ProgramCompletionNode, legacy_id) == 167);
    assert!(core::mem::offset_of!(ProgramCompletionNode, retain_output) == 168);
    assert!(core::mem::offset_of!(ProgramCompletionNode, app_class) == 169);
    assert!(core::mem::offset_of!(ProgramCompletionNode, role) == 170);
    assert!(core::mem::offset_of!(ProgramCompletionNode, exit_reason) == 171);
    assert!(core::mem::offset_of!(ProgramCompletionNode, text_info) == 176);
    assert!(core::mem::size_of::<ProgramCompletionNode>() == 208);
    assert!(core::mem::offset_of!(ProgramCompletionNode, observation_refs) == 172);
    assert!(core::mem::size_of::<ProgramObservationNode>() == 56);
    assert!(core::mem::align_of::<ProgramCompletionNode>() == 8);
    assert!(core::mem::offset_of!(ExitRecord, handle) == 0);
    assert!(core::mem::offset_of!(ExitRecord, sequence) == 16);
    assert!(core::mem::offset_of!(ExitRecord, start_tick) == 24);
    assert!(core::mem::offset_of!(ExitRecord, finish_tick) == 32);
    assert!(core::mem::offset_of!(ExitRecord, exit_code) == 40);
    assert!(core::mem::offset_of!(ExitRecord, used) == 44);
    assert!(core::mem::offset_of!(ExitRecord, app_class) == 45);
    assert!(core::mem::offset_of!(ExitRecord, role) == 46);
    assert!(core::mem::offset_of!(ExitRecord, exit_reason) == 47);
    assert!(core::mem::size_of::<ExitRecord>() == 48);
    assert!(core::mem::align_of::<ExitRecord>() == 8);
};
