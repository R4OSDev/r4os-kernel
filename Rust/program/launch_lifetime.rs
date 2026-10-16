//! Construction, exact imports, three launch roles and publication/rollback.
//! Pre-publication cancellation owns every acquired resource. Background
//! publication remains the detach boundary; foreground retains its completion.
use super::{
    completions, console_input, context, entry, exit, exit_state, failure, image, instance_storage,
    payload, payload_types as t, process, process_data, registry, registry_state,
    registry_types::*, stack, thread_retire, thread_runtime, threads,
};
use crate::{
    boot::foundation,
    contract::{
        ProgramProcessCompletion as Completion, ProgramProcessHandle as Handle, R4XStartImport,
    },
    fs::{drive::Drive, vfs_types::Bytes},
    platform::timer,
    sched::{
        scheduler,
        task::{task_api, task_registry},
    },
};
use core::{
    mem, ptr,
    sync::atomic::{AtomicBool, Ordering},
};
const FAILED: u8 = 2;
#[repr(C)]
pub struct Loaded {
    pub value: image::Loaded,
    pub origin: [u8; 128],
    pub origin_len: u16,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Options {
    pub owner_handle: Handle,
    pub out_handle: *mut Handle,
    pub error_out: *mut i32,
    pub owner: bool,
    pub owner_handle_present: bool,
    pub legacy_id: bool,
    pub retain_output: bool,
    pub report_boot: bool,
    pub forced_cpu: u8,
    pub forced_cpu_present: bool,
    pub reserved: u8,
}

fn report(enabled: bool, text: &[u8]) {
    entry::report_stage(enabled, text, true);
}
fn detail(text: &[u8]) {
    unsafe {
        program_entry_boot_detail(Bytes {
            data: text.as_ptr(),
            len: text.len(),
        })
    }
}
pub(super) unsafe fn error(options: &Options, status: i32) {
    unsafe {
        if !options.error_out.is_null() {
            options.error_out.write(status);
        }
        if status == 0 || !registry::lock() {
            return;
        }
        let stats = (*registry_state::state()).stats;
        (*stats).launch_failures = (*stats).launch_failures.saturating_add(1);
        (*stats).last_launch_error = status;
        registry::unlock();
    }
}
unsafe fn cancelled() -> bool {
    unsafe { threads::r4os_rust_program_thread_scalar(1) != 0 }
}
unsafe fn rollback(reservation: *const ProgramInstanceReservation) {
    unsafe {
        registry::r4os_rust_program_registry_reservation(0, reservation);
    }
}
unsafe fn rollback_registered(reservation: *const ProgramInstanceReservation, active: *mut bool) {
    unsafe {
        let handle = registry::handle_for_reservation(reservation);
        while !thread_retire::r4os_rust_program_threads_release(handle, false) {
            scheduler::yield_task();
        }
        rollback(reservation);
        active.write(false);
    }
}
unsafe fn ready(handle: Handle) -> bool {
    unsafe { completions::r4os_rust_program_completion_access(0, handle, ptr::null_mut()) != 0 }
}
pub(super) unsafe fn cancel_published(handle: Handle) {
    unsafe {
        while !ready(handle) && !exit::r4os_rust_program_exit_begin(handle, -9, 2) {
            scheduler::yield_task();
        }
    }
}
unsafe fn imports(instance: *mut ProgramInstance, seeds: &[image::ImportSeed]) {
    unsafe {
        let runtime = payload::runtime(instance);
        let count = seeds.len().min(16);
        (*runtime).r4xstart_import_count = count as u32;
        (*runtime).r4l_code_binding_count = 0;
        for (i, seed) in seeds.iter().take(count).enumerate() {
            (*runtime).r4xstart_import_module_names[i].fill(0);
            (*runtime).r4xstart_import_symbol_names[i].fill(0);
            (&mut (*runtime).r4xstart_import_module_names[i])[..seed.module_name_len]
                .copy_from_slice(&seed.module_name[..seed.module_name_len]);
            (&mut (*runtime).r4xstart_import_symbol_names[i])[..seed.symbol_name_len]
                .copy_from_slice(&seed.symbol_name[..seed.symbol_name_len]);
            (*runtime).r4xstart_imports[i] = R4XStartImport {
                group_id: seed.group_id,
                min_version: seed.min_version,
                resolved_version: seed.resolved_version,
                flags: seed.flags,
                module_name: ptr::addr_of!((*runtime).r4xstart_import_module_names[i]).addr()
                    as u64,
                symbol_name: ptr::addr_of!((*runtime).r4xstart_import_symbol_names[i]).addr()
                    as u64,
                table: seed.table,
            };
            if seed.r4l_binding_valid && seed.r4l_generation != 0 {
                let count = usize::from((*runtime).r4l_code_binding_count);
                let mut duplicate = false;
                for b in 0..count {
                    let binding = (*runtime).r4l_code_bindings[b];
                    if binding.module_slot == seed.r4l_module_slot
                        && binding.generation == seed.r4l_generation
                    {
                        duplicate = true;
                        break;
                    }
                }
                if !duplicate && count < 16 {
                    (*runtime).r4l_code_bindings[count] = t::CodeBinding {
                        module_slot: seed.r4l_module_slot,
                        generation: seed.r4l_generation,
                        reserved: [0; 3],
                    };
                    (*runtime).r4l_code_binding_count += 1;
                }
            }
        }
    }
}
#[allow(clippy::too_many_arguments)]
unsafe fn create(
    reservation: *const ProgramInstanceReservation,
    role: u8,
    class: u8,
    loaded: &Loaded,
    stack: &stack::Stack,
    args: Bytes,
    drive: *mut Drive,
    origin: &[u8],
) -> *mut ProgramInstance {
    unsafe {
        let parent = context::current();
        let inherit = !parent.is_null() && (*payload::process(parent)).environment_len != 0;
        let id = (*reservation).id;
        let instance = ptr::addr_of_mut!((*(*reservation).slot).instance);
        let Some(storage) = instance_storage::allocate(id, class, inherit) else {
            return ptr::null_mut();
        };
        let value = &loaded.value;
        // Compiler-guarded original empty notification owner consists of null
        // treap root, unseeded PRNG and closing=false; no queue is fabricated.
        instance.write(mem::zeroed());
        (*instance).id = id;
        (*instance).role = role;
        (*instance).app_class = class;
        (*instance).parallel_execution = role == 2 && value.parallel_execution;
        (*instance).desktop_host = value.desktop_host;
        (*instance).entry = ptr::with_exposed_provenance(value.entry as usize);
        (*instance).stack_top = stack.top;
        (*instance).program_image_range_id = value.image.range_id;
        (*instance).program_image_base = value.image.code.addr() as u64;
        (*instance).program_image_size = value.image.len;
        stack::r4os_rust_program_stack_to_instance(instance, stack);
        (*instance).memory_profile = value.memory.profile;
        (*instance).memory_limits = value.memory.limits;
        (*instance).memory_tag = value.memory.tag;
        (*instance).runtime_payload = storage.runtime.cast();
        (*instance).process_payload = storage.process.cast();
        (*instance).console_payload = storage.console.cast();
        (*instance).gui_payload = storage.gui.cast();
        process_data::r4os_rust_program_data_arguments(instance, args);
        process_data::r4os_rust_program_data_inherit(instance, parent);
        if !process_data::r4os_rust_program_data_cwd_copy(instance, parent, drive) {
            return ptr::null_mut();
        }
        let runtime = payload::runtime(instance);
        let n = origin.len().min(128);
        ptr::copy_nonoverlapping(origin.as_ptr(), (*runtime).module_path.as_mut_ptr(), n);
        (*runtime).module_path_len = n as u32;
        imports(instance, &value.imports[..value.import_count as usize]);
        entry::r4os_rust_program_entry_context(instance);
        instance
    }
}
unsafe fn free(loaded: &Loaded, stack: &stack::Stack) {
    unsafe { stack::r4os_rust_program_resources_free(&loaded.value.image, stack) }
}
unsafe fn fail(phase: u8) -> bool {
    failure::r4os_rust_program_failure_control(3, phase)
}
unsafe fn task(name: &'static [u8], parallel: bool, shell: bool) -> *mut core::ffi::c_void {
    unsafe {
        let request = task_api::CreateRequest {
            name: name.as_ptr(),
            name_len: name.len(),
            state: 3,
            role: 2,
            needs_fpu: true,
            smp_eligible: parallel,
            smp_r4x: parallel,
            owner_kind: 0,
            owner_context: ptr::null_mut(),
            entry: Some(if shell { shell_entry } else { program_entry }),
        };
        let mut failure = 0;
        task_api::r4os_rust_task_create(&request, false, &mut failure)
    }
}
unsafe extern "C" fn program_entry() {
    unsafe { entry::r4os_rust_program_entry_main(false) }
}
unsafe extern "C" fn shell_entry() {
    unsafe { entry::r4os_rust_program_entry_main(true) }
}
unsafe fn mark_ready(task: *mut core::ffi::c_void) {
    unsafe {
        task_api::r4os_rust_task_mutate(0, task, timer::tick_count(), 0, ptr::null(), 0);
    }
}
unsafe fn inherit_console(instance: *mut ProgramInstance, id: u32) {
    unsafe {
        if id == 0 {
            return;
        }
        let console = payload::console(instance);
        (*console).io_target_id = id;
        let mut handle = Handle::default();
        if registry::r4os_rust_program_registry_handle(3, ptr::null(), id, &mut handle) {
            (*console).io_target_generation = handle.generation;
            let host = console_input::r4os_rust_program_console_input_target(0, handle, false);
            if !host.is_null() {
                (*console).host = (*payload::console(host)).host;
            }
        }
    }
}
unsafe fn console_metrics(instance: *mut ProgramInstance) {
    unsafe {
        let cols = program_launch_metric(false);
        let rows = program_launch_metric(true);
        let console = payload::console(instance);
        (*console).state.cols = if cols == 0 { 80 } else { cols }.clamp(1, 512);
        (*console).state.rows = if rows == 0 { 25 } else { rows }.clamp(1, 256);
        (*console).revision = 1;
    }
}
#[allow(clippy::too_many_arguments)]
unsafe fn foreground(
    reservation: *const ProgramInstanceReservation,
    active: *mut bool,
    loaded: &Loaded,
    stack: &stack::Stack,
    class: u8,
    args: Bytes,
    drive: *mut Drive,
    options: &Options,
) -> u8 {
    unsafe {
        process::r4os_rust_program_process_signal();
        if cancelled() {
            free(loaded, stack);
            error(options, -10);
            return FAILED;
        }
        if (*exit_state::state()).foreground_present {
            foundation::puts(b"Program already running\r\n");
            (*exit_state::state()).last_exit_code = 1;
            free(loaded, stack);
            error(options, -5);
            return FAILED;
        }
        let inherited = console_input::r4os_rust_program_console_input_inherited(class);
        let instance = create(
            reservation,
            0,
            class,
            loaded,
            stack,
            args,
            drive,
            &loaded.origin[..usize::from(loaded.origin_len)],
        );
        if instance.is_null() {
            foundation::puts(b"Program instance allocation failed\r\n");
            (*exit_state::state()).last_exit_code = 1;
            free(loaded, stack);
            error(options, -8);
            return FAILED;
        }
        if fail(2) {
            rollback(reservation);
            active.write(false);
            error(options, -8);
            return FAILED;
        }
        if cancelled() {
            rollback(reservation);
            active.write(false);
            error(options, -10);
            return FAILED;
        }
        inherit_console(instance, inherited);
        let task = task(b"r4x-program", false, false);
        if task.is_null() {
            rollback(reservation);
            active.write(false);
            foundation::puts(b"Program task create failed\r\n");
            (*exit_state::state()).last_exit_code = 1;
            error(options, -10);
            return FAILED;
        }
        if fail(5) {
            thread_retire::r4os_rust_program_thread_release_created(task);
            rollback(reservation);
            active.write(false);
            error(options, -10);
            return FAILED;
        }
        (*instance).task_id = (*task_registry::core(task.cast())).id;
        if thread_runtime::r4os_rust_program_thread_register_main(instance, task).is_null() {
            thread_retire::r4os_rust_program_thread_release_created(task);
            rollback(reservation);
            active.write(false);
            foundation::puts(b"Program main thread registration failed\r\n");
            (*exit_state::state()).last_exit_code = 1;
            error(options, -10);
            return FAILED;
        }
        if cancelled() {
            rollback_registered(reservation, active);
            error(options, -10);
            return FAILED;
        }
        if fail(6) || registry::r4os_rust_program_registry_reservation(2, reservation) == 0 {
            rollback_registered(reservation, active);
            foundation::puts(b"Program registry publish failed\r\n");
            (*exit_state::state()).last_exit_code = 1;
            error(options, -10);
            return FAILED;
        }
        active.write(false);
        let handle = registry::handle_for_slot((*reservation).slot);
        if cancelled() {
            cancel_published(handle);
            error(options, -10);
            completions::r4os_rust_program_completion_access(4, handle, ptr::null_mut());
            return FAILED;
        }
        if !options.out_handle.is_null() {
            options.out_handle.write(handle);
        }
        (*exit_state::state()).foreground_id = (*instance).id;
        (*exit_state::state()).foreground_present = true;
        (*exit_state::state()).foreground_generation = handle.generation;
        let boot = crate::display::bootscreen::is_active();
        mark_ready(task);
        loop {
            if ready(handle) {
                if boot {
                    detail(b"SERVMAN: Rueckgabe");
                }
                let mut completion = Completion::default();
                if completions::r4os_rust_program_completion_access(2, handle, &mut completion) == 0
                {
                    (*exit_state::state()).last_exit_code = completion.exit_code;
                    (*exit_state::state()).last_display_used = completion.flags & 4 != 0;
                }
                if boot {
                    detail(b"Dienstmanager fertig");
                }
                return 0;
            }
            if cancelled() {
                cancel_published(handle);
                error(options, -10);
                completions::r4os_rust_program_completion_access(4, handle, ptr::null_mut());
                return FAILED;
            }
            scheduler::yield_task();
        }
    }
}
#[allow(clippy::too_many_arguments)]
unsafe fn shell(
    reservation: *const ProgramInstanceReservation,
    active: *mut bool,
    loaded: &Loaded,
    stack: &stack::Stack,
    class: u8,
    args: Bytes,
    drive: *mut Drive,
    host: u8,
    options: &Options,
) -> u8 {
    unsafe {
        report(options.report_boot, b"Altlasten pruefen");
        process::r4os_rust_program_process_signal();
        if cancelled() {
            free(loaded, stack);
            error(options, -10);
            return FAILED;
        }
        if (*exit_state::state()).shell_present {
            foundation::puts(b"Shell already running\r\n");
            free(loaded, stack);
            error(options, -5);
            return FAILED;
        }
        report(options.report_boot, b"Instanz anlegen");
        let instance = create(
            reservation,
            1,
            class,
            loaded,
            stack,
            args,
            drive,
            &loaded.origin[..usize::from(loaded.origin_len)],
        );
        if instance.is_null() {
            foundation::puts(b"Shell instance allocation failed\r\n");
            free(loaded, stack);
            error(options, -8);
            return FAILED;
        }
        if fail(2) {
            rollback(reservation);
            active.write(false);
            error(options, -8);
            return FAILED;
        }
        if cancelled() {
            rollback(reservation);
            active.write(false);
            error(options, -10);
            return FAILED;
        }
        if class == 0 {
            (*payload::console(instance)).host = u32::from(host);
            if host != 0 {
                console_metrics(instance);
            }
        }
        report(options.report_boot, b"Task anlegen");
        let task = task(b"r4x-shell", false, true);
        if task.is_null() {
            rollback(reservation);
            active.write(false);
            foundation::puts(b"Shell task create failed\r\n");
            error(options, -10);
            return FAILED;
        }
        if fail(5) {
            thread_retire::r4os_rust_program_thread_release_created(task);
            rollback(reservation);
            active.write(false);
            error(options, -10);
            return FAILED;
        }
        (*instance).task_id = (*task_registry::core(task.cast())).id;
        report(options.report_boot, b"Thread binden");
        if thread_runtime::r4os_rust_program_thread_register_main(instance, task).is_null() {
            thread_retire::r4os_rust_program_thread_release_created(task);
            rollback(reservation);
            active.write(false);
            foundation::puts(b"Shell main thread registration failed\r\n");
            error(options, -10);
            return FAILED;
        }
        if cancelled() {
            rollback_registered(reservation, active);
            error(options, -10);
            return FAILED;
        }
        report(options.report_boot, b"Instanz veroeffentlichen");
        if fail(6) || registry::r4os_rust_program_registry_reservation(2, reservation) == 0 {
            rollback_registered(reservation, active);
            foundation::puts(b"Shell registry publish failed\r\n");
            error(options, -10);
            return FAILED;
        }
        active.write(false);
        let handle = registry::handle_for_slot((*reservation).slot);
        if cancelled() {
            cancel_published(handle);
            error(options, -10);
            return FAILED;
        }
        if !options.out_handle.is_null() {
            options.out_handle.write(handle);
        }
        (*exit_state::state()).shell_id = (*instance).id;
        (*exit_state::state()).shell_present = true;
        (*exit_state::state()).shell_generation = handle.generation;
        report(options.report_boot, b"Task einreihen");
        mark_ready(task);
        0
    }
}
#[allow(clippy::too_many_arguments)]
unsafe fn background(
    reservation: *const ProgramInstanceReservation,
    active: *mut bool,
    loaded: &Loaded,
    stack: &stack::Stack,
    class: u8,
    args: Bytes,
    drive: *mut Drive,
    host: u8,
    options: &Options,
) -> u8 {
    unsafe {
        process::r4os_rust_program_process_signal();
        if cancelled() {
            free(loaded, stack);
            error(options, -10);
            return FAILED;
        }
        let inherited = if host == 0 {
            console_input::r4os_rust_program_console_input_inherited(class)
        } else {
            0
        };
        if host != 0 && class != 0 {
            foundation::puts(b"Console host requested for non-console program\r\n");
            (*exit_state::state()).last_exit_code = 1;
            free(loaded, stack);
            error(options, -1);
            return FAILED;
        }
        let instance = create(
            reservation,
            2,
            class,
            loaded,
            stack,
            args,
            drive,
            &loaded.origin[..usize::from(loaded.origin_len)],
        );
        if instance.is_null() {
            foundation::puts(b"Program instance allocation failed\r\n");
            (*exit_state::state()).last_exit_code = 1;
            free(loaded, stack);
            error(options, -8);
            return FAILED;
        }
        if fail(2) {
            rollback(reservation);
            active.write(false);
            error(options, -8);
            return FAILED;
        }
        if cancelled() {
            rollback(reservation);
            active.write(false);
            error(options, -10);
            return FAILED;
        }
        if class == 0 {
            (*payload::console(instance)).host = u32::from(host);
        }
        inherit_console(instance, inherited);
        if host != 0 {
            console_metrics(instance);
        }
        let parallel = (*instance).parallel_execution;
        let task = task(
            if parallel {
                b"r4x-app"
            } else if class == 2 {
                b"r4x-service"
            } else {
                b"r4x-app"
            },
            parallel,
            false,
        );
        if task.is_null() {
            rollback(reservation);
            active.write(false);
            foundation::puts(b"Program task create failed\r\n");
            (*exit_state::state()).last_exit_code = 1;
            error(options, -10);
            return FAILED;
        }
        if fail(5) {
            thread_retire::r4os_rust_program_thread_release_created(task);
            rollback(reservation);
            active.write(false);
            error(options, -10);
            return FAILED;
        }
        if options.forced_cpu_present
            && (!parallel
                || task_api::r4os_rust_task_mutate(
                    24,
                    task,
                    u64::from(options.forced_cpu),
                    0,
                    ptr::null(),
                    0,
                ) == 0)
        {
            thread_retire::r4os_rust_program_thread_release_created(task);
            rollback(reservation);
            active.write(false);
            foundation::puts(b"Program task CPU placement failed\r\n");
            (*exit_state::state()).last_exit_code = 1;
            error(options, -10);
            return FAILED;
        }
        (*instance).task_id = (*task_registry::core(task.cast())).id;
        if thread_runtime::r4os_rust_program_thread_register_main(instance, task).is_null() {
            thread_retire::r4os_rust_program_thread_release_created(task);
            rollback(reservation);
            active.write(false);
            foundation::puts(b"Program main thread registration failed\r\n");
            (*exit_state::state()).last_exit_code = 1;
            error(options, -10);
            return FAILED;
        }
        if cancelled() {
            rollback_registered(reservation, active);
            error(options, -10);
            return FAILED;
        }
        if fail(6) || registry::r4os_rust_program_registry_reservation(2, reservation) == 0 {
            rollback_registered(reservation, active);
            foundation::puts(b"Program registry publish failed\r\n");
            (*exit_state::state()).last_exit_code = 1;
            error(options, -10);
            return FAILED;
        }
        active.write(false);
        let handle = registry::handle_for_slot((*reservation).slot);
        // A killed parent after this commit still starts its detached child.
        if class == 1 && !(*instance).desktop_host && !options.out_handle.is_null() {
            let gui = (*instance).gui_payload.cast::<t::Gui>();
            assert!(!gui.is_null());
            AtomicBool::from_ptr(ptr::addr_of_mut!((*gui).start_attach_pending))
                .store(true, Ordering::Release);
        }
        if !options.out_handle.is_null() {
            options.out_handle.write(handle);
        }
        if scheduler::r4os_rust_scheduler_task(3, task, 0, 0, ptr::null(), 0) == 0 {
            foundation::puts(b"Program task publication failed\r\n");
            while !ready(handle) && !exit::r4os_rust_program_exit_begin(handle, -1, 2) {
                scheduler::yield_task();
            }
            (*exit_state::state()).last_exit_code = 1;
            error(options, -10);
            return FAILED;
        }
        0
    }
}
/// # Safety
/// All records are exact retained construction values; publication owns their
/// resources and outputs, and no borrowed registry/Task owner spans a wait.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn r4os_rust_program_launch_mode(
    mode: u8,
    reservation: *const ProgramInstanceReservation,
    active: *mut bool,
    loaded: *const Loaded,
    stack: *const stack::Stack,
    class: u8,
    args: Bytes,
    drive: *mut Drive,
    host: u8,
    options: *const Options,
) -> u8 {
    unsafe {
        match mode {
            0 => foreground(
                reservation,
                active,
                &*loaded,
                &*stack,
                class,
                args,
                drive,
                &*options,
            ),
            1 => shell(
                reservation,
                active,
                &*loaded,
                &*stack,
                class,
                args,
                drive,
                host,
                &*options,
            ),
            2 => background(
                reservation,
                active,
                &*loaded,
                &*stack,
                class,
                args,
                drive,
                host,
                &*options,
            ),
            _ => unreachable!(),
        }
    }
}
/// # Safety
/// Exact published generation; caller retains its original spawn lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_launch_cancel(handle: Handle) {
    unsafe { cancel_published(handle) }
}
/// # Safety
/// Canonical copied private options and retained optional error cell.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_launch_error(options: *const Options, status: i32) {
    unsafe { error(&*options, status) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_launch_last(display: bool) -> i32 {
    unsafe {
        if display {
            i32::from((*exit_state::state()).last_display_used)
        } else {
            (*exit_state::state()).last_exit_code
        }
    }
}
const _: () = {
    assert!(mem::size_of::<Loaded>() == 2056);
    assert!(mem::offset_of!(Loaded, value) == 0);
    assert!(mem::offset_of!(Loaded, origin) == 1920);
    assert!(mem::offset_of!(Loaded, origin_len) == 2048);
    assert!(mem::size_of::<Options>() == 40);
    assert!(mem::offset_of!(Options, owner_handle) == 0);
    assert!(mem::offset_of!(Options, out_handle) == 16);
    assert!(mem::offset_of!(Options, error_out) == 24);
    assert!(mem::offset_of!(Options, owner) == 32);
    assert!(mem::offset_of!(Options, owner_handle_present) == 33);
    assert!(mem::offset_of!(Options, legacy_id) == 34);
    assert!(mem::offset_of!(Options, retain_output) == 35);
    assert!(mem::offset_of!(Options, report_boot) == 36);
    assert!(mem::offset_of!(Options, forced_cpu) == 37);
    assert!(mem::offset_of!(Options, forced_cpu_present) == 38);
    assert!(mem::offset_of!(Options, reserved) == 39);
};

unsafe fn program_entry_boot_detail(value: Bytes) {
    unsafe { crate::display::bootscreen::set_detail(value.data, value.len) }
}

unsafe fn program_launch_metric(rows: bool) -> u32 {
    crate::boot::log_route::r4os_rust_log_control(if rows { 11 } else { 10 }, 0, 0, 0, 0)
}
