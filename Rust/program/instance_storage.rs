//! Program payload transactions and persistent completion-output handoff.
//! Invalid preflight retains the complete hierarchy. Teardown acknowledges
//! exact reader/reference owners before publishing or freeing their storage.
use super::{
    console, gui_lifetime as gui, payload, payload_types as t, registry, registry_state,
    registry_types::*,
};
use crate::contract;
use core::{ffi::c_void, mem, ptr};
fn stat(offset: usize) {
    payload::add(offset, 8, 1);
}
unsafe fn valid(type_id: u8, data: *const c_void, owner: u32) -> bool {
    let shape = t::shape(type_id);
    unsafe {
        payload::valid(
            data.cast::<u8>().add(shape.header).cast(),
            owner,
            u16::from(type_id),
            shape.size,
        )
    }
}
unsafe fn free(type_id: u8, data: *mut c_void, owner: u32) -> bool {
    unsafe { payload::r4os_rust_program_payload_release(type_id, data, owner, u16::from(type_id)) }
}
/// Construct the same empty console hierarchy for eager CUI admission or a
/// GUI's first stdout/stderr write. Neither path allocates an output block yet.
pub(super) unsafe fn allocate_console(owner: u32) -> *mut t::Console {
    unsafe {
        let console = payload::r4os_rust_program_payload_allocate(4, owner, 4).cast::<t::Console>();
        if console.is_null() {
            return ptr::null_mut();
        }
        let transcript = payload::r4os_rust_program_payload_allocate(11, owner, 11)
            .cast::<t::ConsoleTranscript>();
        if transcript.is_null() {
            assert!(payload::r4os_rust_program_payload_release(
                4,
                console.cast(),
                owner,
                4
            ));
            return ptr::null_mut();
        }
        (*console).transcript_payload = transcript;
        console
    }
}
pub(super) unsafe fn valid_transaction(owner: u32, storage: *const t::InstanceStorage) -> bool {
    unsafe {
        let mut result = true;
        if !valid(1, (*storage).runtime.cast(), owner) {
            result = false;
        }
        let process = (*storage).process;
        if !valid(2, process.cast(), owner) {
            result = false;
        } else {
            let environment = (*process).environment_payload;
            if !environment.is_null() && !valid(3, environment.cast(), owner) {
                result = false;
            }
        }
        let console = (*storage).console;
        if !console.is_null() {
            if !valid(4, console.cast(), owner) {
                result = false;
            } else {
                let transcript = (*console).transcript_payload;
                let writer = (*console).writer_payload;
                if !transcript.is_null() && !console::transcript_valid(transcript, owner) {
                    result = false;
                }
                if !writer.is_null() && !console::output_valid(writer, owner) {
                    result = false;
                }
            }
        }
        let gui = (*storage).gui;
        if !gui.is_null() && (!valid(6, gui.cast(), owner) || !gui::valid_set(gui, owner)) {
            result = false;
        }
        result
    }
}
pub(super) unsafe fn valid_instance(instance: *const ProgramInstance) -> bool {
    unsafe {
        let owner = (*instance).id;
        let mut result = true;
        let runtime = (*instance).runtime_payload;
        if !runtime.is_null() {
            if !valid(1, runtime, owner) {
                result = false;
            }
        } else {
            stat(mem::offset_of!(t::Stats, header_errors));
            result = false;
        }
        let process = (*instance).process_payload.cast::<t::Process>();
        if !process.is_null() {
            if !valid(2, process.cast(), owner) {
                result = false;
            } else {
                let environment = (*process).environment_payload;
                if !environment.is_null() && !valid(3, environment.cast(), owner) {
                    result = false;
                }
            }
        } else {
            stat(mem::offset_of!(t::Stats, header_errors));
            result = false;
        }
        let console = (*instance).console_payload.cast::<t::Console>();
        if !console.is_null() {
            if !valid(4, console.cast(), owner) {
                result = false;
            } else {
                let transcript = (*console).transcript_payload;
                if !transcript.is_null() {
                    if !console::transcript_valid(transcript, owner) {
                        result = false;
                    }
                    let writer = (*console).writer_payload;
                    if !writer.is_null() && !console::output_valid(writer, owner) {
                        result = false;
                    }
                } else {
                    stat(mem::offset_of!(t::Stats, header_errors));
                    result = false;
                }
            }
        } else if (*instance).app_class == 0 {
            stat(mem::offset_of!(t::Stats, header_errors));
            result = false;
        }
        let gui = (*instance).gui_payload.cast::<t::Gui>();
        if !gui.is_null() {
            if !valid(6, gui.cast(), owner) || !gui::valid_set(gui, owner) {
                result = false;
            }
        } else if (*instance).app_class == 1 {
            stat(mem::offset_of!(t::Stats, header_errors));
            result = false;
        }
        result
    }
}
pub(super) unsafe fn rollback(owner: u32, storage: *mut t::InstanceStorage) {
    unsafe {
        if !valid_transaction(owner, storage) {
            return;
        }
        let gui = (*storage).gui;
        if !gui.is_null() {
            gui::release_set(gui, owner);
            (*storage).gui = ptr::null_mut();
            free(6, gui.cast(), owner);
        }
        let console = (*storage).console;
        if !console.is_null() {
            let transcript = (*console).transcript_payload;
            if !transcript.is_null() {
                (*console).transcript_payload = ptr::null_mut();
                console::release_segments(transcript, true);
                free(11, transcript.cast(), owner);
            }
            let writer = (*console).writer_payload;
            if !writer.is_null() {
                (*console).writer_payload = ptr::null_mut();
                console::release(writer, true);
            }
            (*storage).console = ptr::null_mut();
            free(4, console.cast(), owner);
        }
        let process = (*storage).process;
        let environment = (*process).environment_payload;
        if !environment.is_null() {
            (*process).environment_payload = ptr::null_mut();
            free(3, environment.cast(), owner);
        }
        free(2, process.cast(), owner);
        free(1, (*storage).runtime.cast(), owner);
    }
}
pub(super) unsafe fn allocate(
    owner: u32,
    class: u8,
    environment: bool,
) -> Option<t::InstanceStorage> {
    unsafe {
        let runtime = payload::r4os_rust_program_payload_allocate(1, owner, 1).cast::<t::Runtime>();
        if runtime.is_null() {
            stat(mem::offset_of!(t::Stats, transaction_rollbacks));
            return None;
        }
        let process = payload::r4os_rust_program_payload_allocate(2, owner, 2).cast::<t::Process>();
        if process.is_null() {
            stat(mem::offset_of!(t::Stats, transaction_rollbacks));
            free(1, runtime.cast(), owner);
            return None;
        }
        let mut storage = t::InstanceStorage {
            runtime,
            process,
            console: ptr::null_mut(),
            gui: ptr::null_mut(),
        };
        if environment {
            (*process).environment_payload =
                payload::r4os_rust_program_payload_allocate(3, owner, 3).cast();
            if (*process).environment_payload.is_null() {
                stat(mem::offset_of!(t::Stats, transaction_rollbacks));
                rollback(owner, &mut storage);
                return None;
            }
        }
        match class {
            0 => {
                let console = allocate_console(owner);
                if console.is_null() {
                    stat(mem::offset_of!(t::Stats, transaction_rollbacks));
                    rollback(owner, &mut storage);
                    return None;
                }
                storage.console = console;
            }
            1 => {
                storage.gui = payload::r4os_rust_program_payload_allocate(6, owner, 6).cast();
                if storage.gui.is_null() {
                    stat(mem::offset_of!(t::Stats, transaction_rollbacks));
                    rollback(owner, &mut storage);
                    return None;
                }
            }
            2 => {}
            _ => unreachable!(),
        }
        Some(storage)
    }
}
pub(super) unsafe fn detach(
    instance: *mut ProgramInstance,
    completion: *mut ProgramCompletionNode,
) -> bool {
    unsafe {
        if (*instance).storage_teardown_blocked {
            return false;
        }
        let console = (*instance).console_payload.cast::<t::Console>();
        if !console.is_null() && (*console).transcript_payload.is_null() {
            (*instance).storage_teardown_blocked = true;
            return false;
        }
        if !valid_instance(instance) {
            (*instance).storage_teardown_blocked = true;
            return false;
        }
        let Some(_guard) = console::OutputGuard::acquire() else {
            return false;
        };
        let console = (*instance).console_payload.cast::<t::Console>();
        if console.is_null() {
            return true;
        }
        let transcript = (*console).transcript_payload;
        assert!(!transcript.is_null());
        let writer = (*console).writer_payload;
        if !writer.is_null() {
            (*console).writer_payload = ptr::null_mut();
            if !console::release(writer, true) {
                (*instance).storage_teardown_blocked = true;
                return false;
            }
        }
        (*console).transcript_payload = ptr::null_mut();
        if (*completion).retain_output {
            if !console::transcript_inactive(transcript) {
                (*instance).storage_teardown_blocked = true;
                return false;
            }
            (*completion).output_payload = transcript.cast();
            (*completion).output_length = (*transcript).output_len;
            (*completion).output_revision = (*console).revision;
            (*completion).flags |= (*console).capture_flags;
            if (*completion).output_length != 0 {
                (*completion).flags |= contract::program_completion_flag_output;
            }
            payload::note_active(mem::size_of::<t::ConsoleTranscript>(), true);
            (*completion).output_storage_bytes = mem::size_of::<t::ConsoleTranscript>() as u64;
            if registry::lock() {
                let stats = (*registry_state::state()).stats;
                (*stats).completion_output_bytes = (*stats)
                    .completion_output_bytes
                    .wrapping_add((*completion).output_storage_bytes);
                registry::unlock();
            }
        } else if !console::release_segments(transcript, true)
            || !free(11, transcript.cast(), (*instance).id)
        {
            (*instance).storage_teardown_blocked = true;
            return false;
        }
        true
    }
}
pub(super) unsafe fn release(
    instance: *mut ProgramInstance,
    completion: *mut ProgramCompletionNode,
) -> bool {
    unsafe {
        if (*instance).storage_teardown_blocked {
            return false;
        }
        super::shared_raster::release_process((*completion).handle);
        let gui = (*instance).gui_payload.cast::<t::Gui>();
        if !gui.is_null() {
            super::gui_activity::close(gui);
            if !gui::release_set(gui, (*instance).id) {
                (*instance).storage_teardown_blocked = true;
                return false;
            }
            (*instance).gui_payload = ptr::null_mut();
            free(6, gui.cast(), (*instance).id);
        }
        let console = (*instance).console_payload.cast::<t::Console>();
        if !console.is_null() {
            if !(*console).transcript_payload.is_null() || !(*console).writer_payload.is_null() {
                (*instance).storage_teardown_blocked = true;
                return false;
            }
            (*instance).console_payload = ptr::null_mut();
            free(4, console.cast(), (*instance).id);
        }
        let process = (*instance).process_payload.cast::<t::Process>();
        if !process.is_null() {
            let environment = (*process).environment_payload;
            if !environment.is_null() {
                (*process).environment_payload = ptr::null_mut();
                free(3, environment.cast(), (*instance).id);
            }
            (*instance).process_payload = ptr::null_mut();
            free(2, process.cast(), (*instance).id);
        }
        let runtime = (*instance).runtime_payload;
        if !runtime.is_null() {
            (*instance).runtime_payload = ptr::null_mut();
            free(1, runtime, (*instance).id);
        }
        payload::r4os_rust_program_payload_account(7, u16::from((*instance).app_class), 0);
        true
    }
}
pub(super) unsafe fn free_output(node: *mut ProgramCompletionNode) {
    unsafe {
        let transcript = (*node).output_payload.cast::<t::ConsoleTranscript>();
        if transcript.is_null() {
            return;
        }
        (*node).output_payload = ptr::null_mut();
        let Some(_guard) = console::OutputGuard::acquire() else {
            (*node).output_payload = transcript.cast();
            return;
        };
        console::release_segments(transcript, false);
        free(11, transcript.cast(), (*node).handle.instance_id);
        if registry::lock() {
            let stats = (*registry_state::state()).stats;
            (*stats).completion_output_bytes = (*stats)
                .completion_output_bytes
                .saturating_sub((*node).output_storage_bytes);
            registry::unlock();
        }
        (*node).output_storage_bytes = 0;
    }
}
pub(super) unsafe fn bytes(instance: *const ProgramInstance) -> u64 {
    unsafe {
        let mut bytes = 0u64;
        if !(*instance).runtime_payload.is_null() {
            bytes += mem::size_of::<t::Runtime>() as u64;
        }
        let process = (*instance).process_payload.cast::<t::Process>();
        if !process.is_null() {
            bytes += mem::size_of::<t::Process>() as u64;
            if !(*process).environment_payload.is_null() {
                bytes += mem::size_of::<t::Environment>() as u64;
            }
        }
        let console = (*instance).console_payload.cast::<t::Console>();
        if !console.is_null() {
            bytes += mem::size_of::<t::Console>() as u64;
            if !(*console).transcript_payload.is_null() {
                bytes += mem::size_of::<t::ConsoleTranscript>() as u64;
            }
            if !(*console).writer_payload.is_null() {
                bytes += mem::size_of::<t::ConsoleOutput>() as u64;
            }
        }
        let gui = (*instance).gui_payload.cast::<t::Gui>();
        if !gui.is_null() {
            bytes += mem::size_of::<t::Gui>() as u64;
            let committed = (*gui).committed_frame;
            let building = (*gui).building_frame;
            if !committed.is_null() {
                bytes += gui::frame_bytes(committed);
            }
            if !building.is_null() {
                bytes += gui::frame_bytes(building);
            }
            let mut retired = (*gui).retired_frames;
            while !retired.is_null() {
                bytes += gui::frame_bytes(retired);
                retired = (*retired).retired_next;
            }
        }
        bytes
    }
}
/// # Safety
/// Exclusive transaction output and exact unpublished allocation ownership.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_storage_allocate(
    owner: u32,
    class: u8,
    environment: bool,
    out: *mut t::InstanceStorage,
) -> bool {
    unsafe {
        let Some(storage) = allocate(owner, class, environment) else {
            return false;
        };
        out.write(storage);
        true
    }
}
/// # Safety
/// Exact private transaction under its original lifetime/admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_storage_transaction(
    owner: u32,
    storage: *mut t::InstanceStorage,
    cleanup: bool,
) -> bool {
    unsafe {
        if cleanup {
            rollback(owner, storage);
            true
        } else {
            valid_transaction(owner, storage)
        }
    }
}
/// # Safety
/// Exact retained instance and its exact completion for op1/2; original retire
/// owner/persistent phase admission excludes new readers and partial repeats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_storage_instance(
    op: u8,
    instance: *mut ProgramInstance,
    completion: *mut ProgramCompletionNode,
) -> bool {
    unsafe {
        match op {
            0 => valid_instance(instance),
            1 => detach(instance, completion),
            2 => release(instance, completion),
            _ => unreachable!(),
        }
    }
}
/// # Safety
/// Exact unlinked completion with exclusive output-release ownership.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_storage_free_output(node: *mut ProgramCompletionNode) {
    unsafe {
        free_output(node);
    }
}
/// # Safety
/// Exact retained instance and original caller's accounting admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_storage_bytes(instance: *const ProgramInstance) -> u64 {
    unsafe { bytes(instance) }
}
