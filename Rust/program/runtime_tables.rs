//! Stable platform API tables and program runtime initialization. Canonical
//! Contract defaults supply every header and reserved slot. Typed readonly
//! native C callbacks carry the unchanged public slot signatures and records.
use super::{async_io, reaper, registry_state};
use crate::{contract as c, sched::task::task_api};
use core::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    ptr,
    sync::atomic::{AtomicBool, AtomicU8, Ordering},
};
const _: c::R4SysFns::system_power_transition = crate::program::system::control::power_transition;
struct Table<T> {
    data: UnsafeCell<MaybeUninit<T>>,
    phase: AtomicU8,
}
// SAFETY: native construction publishes through Acquire/Release. Defaults are
// boot-only views before program publication; configured tables are immutable.
unsafe impl<T> Sync for Table<T> {}
impl<T: Default> Table<T> {
    const fn new() -> Self {
        Self {
            data: UnsafeCell::new(MaybeUninit::uninit()),
            phase: AtomicU8::new(0),
        }
    }
    fn get(&self) -> *mut T {
        loop {
            match self.phase.load(Ordering::Acquire) {
                0 => {
                    if self
                        .phase
                        .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
                        .is_ok()
                    {
                        unsafe { self.data.get().cast::<T>().write(T::default()) };
                        self.phase.store(2, Ordering::Release);
                    }
                }
                2 | 4 => return self.data.get().cast(),
                1 | 3 => core::hint::spin_loop(),
                _ => unreachable!(),
            }
        }
    }
    fn configure(&self, value: T) {
        let target = self.get();
        if self
            .phase
            .compare_exchange(2, 3, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            unsafe { target.write(value) };
            self.phase.store(4, Ordering::Release);
        } else {
            while self.phase.load(Ordering::Acquire) != 4 {
                core::hint::spin_loop();
            }
        }
    }
}
static INITIALIZED: AtomicBool = AtomicBool::new(false);

static SYS2: Table<c::generation2::R4XStartR4Sys2> = Table::new();
static DESK2: Table<c::generation2::R4XStartR4Desk2> = Table::new();
static DRAW2: Table<c::generation2::R4XStartR4Draw2> = Table::new();
static NET2: Table<c::generation2::R4XStartR4Net2> = Table::new();
static AUDIO2: Table<c::generation2::R4XStartR4Audio2> = Table::new();
static DEV2: Table<c::generation2::R4XStartR4Dev2> = Table::new();
// Current tables bind the original mechanism owners directly.
fn configure_sys2() {
    let value = c::generation2::R4XStartR4Sys2 {
        write: (super::platform_callbacks::sys_write as *const () as usize) as u64,
        putc: (super::platform_callbacks::sys_putc as *const () as usize) as u64,
        sleep_ticks: (crate::program::system::control::r4os_rust_sys_sleep as *const () as usize)
            as u64,
        ticks: (crate::program::system::control::r4os_rust_sys_ticks as *const () as usize) as u64,
        env_get: (crate::program::process_data::r4os_rust_program_data_env_api_get as *const ()
            as usize) as u64,
        dir_entry: (crate::program::system::directory::r4os_rust_sys_dir_entry as *const ()
            as usize) as u64,
        program_should_close: (super::platform_callbacks::sys_program_should_close as *const ()
            as usize) as u64,
        program_class: (super::platform_callbacks::sys_program_class as *const () as usize) as u64,
        program_instance: (crate::program::info::r4os_rust_program_info_index as *const () as usize)
            as u64,
        service_status: (super::platform_callbacks::sys_service_status as *const () as usize)
            as u64,
        service_open: (super::platform_callbacks::sys_service_open as *const () as usize) as u64,
        service_close: (super::platform_callbacks::sys_service_close as *const () as usize) as u64,
        service_call: (super::platform_callbacks::sys_service_call as *const () as usize) as u64,
        service_endpoint_register: (crate::program::service_api::r4os_rust_program_service_register
            as *const () as usize) as u64,
        service_endpoint_unregister: (super::platform_callbacks::sys_service_endpoint_unregister
            as *const () as usize) as u64,
        service_endpoint_poll: (super::platform_callbacks::sys_service_endpoint_poll as *const ()
            as usize) as u64,
        service_endpoint_recv: (crate::program::service_api::r4os_rust_program_service_recv
            as *const () as usize) as u64,
        service_endpoint_reply: (crate::program::service_api::r4os_rust_program_service_reply
            as *const () as usize) as u64,
        service_detail_by_name: (crate::program::service_api::r4os_rust_program_service_detail_name
            as *const () as usize) as u64,
        service_start: (super::platform_callbacks::sys_service_start as *const () as usize) as u64,
        service_stop: (super::platform_callbacks::sys_service_stop as *const () as usize) as u64,
        service_restart: (super::platform_callbacks::sys_service_restart as *const () as usize)
            as u64,
        service_install: (crate::program::service_api::r4os_rust_program_service_install
            as *const () as usize) as u64,
        service_remove: (super::platform_callbacks::sys_service_remove as *const () as usize)
            as u64,
        vm_reserve: (crate::program::vm::r4os_rust_program_vm_reserve as *const () as usize) as u64,
        vm_commit: (crate::program::vm::r4os_rust_program_vm_commit as *const () as usize) as u64,
        vm_decommit: (super::platform_callbacks::sys_vm_decommit as *const () as usize) as u64,
        vm_release: (super::platform_callbacks::sys_vm_release as *const () as usize) as u64,
        vm_query: (crate::program::vm::r4os_rust_program_vm_query as *const () as usize) as u64,
        thread_create: (super::platform_callbacks::sys_thread_create as *const () as usize) as u64,
        thread_exit: (super::platform_callbacks::sys_thread_exit as *const () as usize) as u64,
        thread_join: (super::platform_callbacks::sys_thread_join as *const () as usize) as u64,
        thread_current: (super::platform_callbacks::sys_thread_current as *const () as usize)
            as u64,
        thread_status: (super::platform_callbacks::sys_thread_status as *const () as usize) as u64,
        io_file_read: (super::platform_callbacks::sys_io_file_read as *const () as usize) as u64,
        io_file_read_at: (super::platform_callbacks::sys_io_file_read_at as *const () as usize)
            as u64,
        io_file_write: (super::platform_callbacks::sys_io_file_write as *const () as usize) as u64,
        io_file_append: (super::platform_callbacks::sys_io_file_append as *const () as usize)
            as u64,
        io_file_stream_begin: (super::platform_callbacks::sys_io_file_stream_begin as *const ()
            as usize) as u64,
        io_file_stream_write: (super::platform_callbacks::sys_io_file_stream_write as *const ()
            as usize) as u64,
        io_file_stream_finish: (super::platform_callbacks::sys_io_file_stream_finish as *const ()
            as usize) as u64,
        io_file_stream_abort: (super::platform_callbacks::sys_io_file_stream_abort as *const ()
            as usize) as u64,
        io_service_call: (super::platform_callbacks::sys_io_service_call as *const () as usize)
            as u64,
        io_status: (super::platform_callbacks::sys_io_status as *const () as usize) as u64,
        io_wait: (super::platform_callbacks::sys_io_wait as *const () as usize) as u64,
        io_close: (super::platform_callbacks::sys_io_close as *const () as usize) as u64,
        task_yield: (crate::program::system::control::r4os_rust_sys_yield as *const () as usize)
            as u64,
        system_halt: (super::platform_callbacks::sys_system_halt as *const () as usize) as u64,
        system_reboot: (crate::program::system::control::r4os_rust_sys_reboot as *const () as usize)
            as u64,
        system_poweroff: (super::platform_callbacks::sys_system_poweroff as *const () as usize)
            as u64,
        time_state: (crate::program::system::control::r4os_rust_sys_time_state as *const ()
            as usize) as u64,
        time_set_state: (crate::program::system::control::r4os_rust_sys_time_set as *const ()
            as usize) as u64,
        time_seconds_since_midnight: (crate::program::system::control::r4os_rust_sys_seconds
            as *const () as usize) as u64,
        dir_list: (crate::program::system::directory::r4os_rust_sys_dir_list as *const () as usize)
            as u64,
        dir_create: (crate::program::system::namespace::r4os_rust_sys_dir_create as *const ()
            as usize) as u64,
        dir_delete: (crate::program::system::namespace::r4os_rust_sys_dir_delete as *const ()
            as usize) as u64,
        drive_info: (crate::program::system::directory::r4os_rust_sys_drive_info as *const ()
            as usize) as u64,
        file_info: (crate::program::system::directory::r4os_rust_sys_file_info as *const ()
            as usize) as u64,
        file_delete: (crate::program::system::namespace::r4os_rust_sys_file_delete as *const ()
            as usize) as u64,
        file_rename: (crate::program::system::namespace::r4os_rust_sys_file_rename as *const ()
            as usize) as u64,
        file_copy: (crate::program::system::copy::r4os_rust_sys_file_copy as *const () as usize)
            as u64,
        file_move: (crate::program::system::copy::r4os_rust_sys_file_move as *const () as usize)
            as u64,
        file_read: (super::platform_callbacks::sys_file_read as *const () as usize) as u64,
        file_write: (super::platform_callbacks::sys_file_write as *const () as usize) as u64,
        file_read_at: (super::platform_callbacks::sys_file_read_at as *const () as usize) as u64,
        file_append: (super::platform_callbacks::sys_file_append as *const () as usize) as u64,
        file_stream_begin: (super::platform_callbacks::sys_file_stream_begin as *const () as usize)
            as u64,
        file_stream_write: (super::platform_callbacks::sys_file_stream_write as *const () as usize)
            as u64,
        file_stream_finish: (super::platform_callbacks::sys_file_stream_finish as *const ()
            as usize) as u64,
        file_stream_abort: (super::platform_callbacks::sys_file_stream_abort as *const () as usize)
            as u64,
        env_set: (crate::program::process_data::r4os_rust_program_data_env_api_set as *const ()
            as usize) as u64,
        registry_key_info: (crate::program::system::registry_api::r4os_rust_sys_registry_key_info
            as *const () as usize) as u64,
        registry_enum_key: (crate::program::system::registry_api::r4os_rust_sys_registry_enum_key
            as *const () as usize) as u64,
        registry_enum_value:
            (crate::program::system::registry_api::r4os_rust_sys_registry_enum_value as *const ()
                as usize) as u64,
        registry_get_value: (crate::program::system::registry_api::r4os_rust_sys_registry_get
            as *const () as usize) as u64,
        registry_set_value: (crate::program::system::registry_api::r4os_rust_sys_registry_set
            as *const () as usize) as u64,
        registry_delete_value: (crate::program::system::registry_api::r4os_rust_sys_registry_delete
            as *const () as usize) as u64,
        service_info: (crate::program::service_api::r4os_rust_program_service_info as *const ()
            as usize) as u64,
        service_detail: (crate::program::service_api::r4os_rust_program_service_detail as *const ()
            as usize) as u64,
        service_set_start_mode: (super::platform_callbacks::sys_service_set_start_mode as *const ()
            as usize) as u64,
        service_endpoint_wait: (super::platform_callbacks::sys_service_endpoint_wait as *const ()
            as usize) as u64,
        program_run: (super::platform_callbacks::sys_program_run as *const () as usize) as u64,
        program_launch: (super::platform_callbacks::sys_program_launch as *const () as usize)
            as u64,
        program_spawn: (super::platform_callbacks::sys_program_spawn as *const () as usize) as u64,
        program_kill: (super::platform_callbacks::sys_program_kill as *const () as usize) as u64,
        program_status: (crate::program::info::r4os_rust_program_info_status as *const () as usize)
            as u64,
        program_request_close: (super::platform_callbacks::sys_program_request_close as *const ()
            as usize) as u64,
        program_reap_instance: (super::platform_callbacks::sys_program_reap_instance as *const ()
            as usize) as u64,
        boot_log_info: (crate::program::system::control::r4os_rust_sys_bootlog_info as *const ()
            as usize) as u64,
        boot_log_read: (crate::program::system::control::r4os_rust_sys_bootlog_read as *const ()
            as usize) as u64,
        program_spawn_handle: (super::platform_callbacks::sys_program_spawn_handle as *const ()
            as usize) as u64,
        program_open_handle: (crate::program::process::r4os_rust_program_process_open as *const ()
            as usize) as u64,
        program_handle_status: (crate::program::info::r4os_rust_program_info_handle as *const ()
            as usize) as u64,
        program_handle_request_close: (crate::program::process::r4os_rust_program_process_close
            as *const () as usize) as u64,
        program_handle_kill: (crate::program::process::r4os_rust_program_process_kill as *const ()
            as usize) as u64,
        program_handle_wait: (crate::program::process::r4os_rust_program_process_wait as *const ()
            as usize) as u64,
        program_handle_reap: (crate::program::process::r4os_rust_program_process_reap as *const ()
            as usize) as u64,
        program_completion_read: (crate::program::program_api::r4os_rust_program_api_completion_read
            as *const () as usize) as u64,
        program_lifecycle_begin: (super::inventory::r4os_rust_program_lifecycle_begin as *const ()
            as usize) as u64,
        program_lifecycle_programs: (super::inventory::r4os_rust_program_lifecycle_programs
            as *const () as usize) as u64,
        program_observation_retain: (super::observations::retain as *const () as usize) as u64,
        program_observation_wait: (super::observations::wait as *const () as usize) as u64,
        program_observation_read: (super::observations::read as *const () as usize) as u64,
        program_observation_release: (super::observations::release as *const () as usize) as u64,
        file_name_limits: (super::system::name_limits::query as *const () as usize) as u64,
        service_bound_detail: (super::service_api::r4os_rust_program_service_bound_detail
            as *const () as usize) as u64,
        service_stop_bound: (super::service_api::r4os_rust_program_service_stop_bound as *const ()
            as usize) as u64,
        service_inventory_page: (crate::service::inventory::page as *const () as usize) as u64,
        service_control_bound: (super::service_api::control_bound as *const () as usize) as u64,
        system_update_target_supported:
            (crate::program::system::atomic::r4os_rust_sys_update_target_supported as *const ()
                as usize) as u64,
        file_metadata2: (crate::program::system::metadata::r4os_rust_sys_file_metadata2
            as c::R4SysFns::file_metadata2 as *const () as usize) as u64,
        directory_metadata_batch2:
            (crate::program::system::metadata::r4os_rust_sys_directory_metadata_batch2
                as c::R4SysFns::directory_metadata_batch2 as *const () as usize) as u64,
        directory_batch: (crate::program::system::directory::r4os_rust_sys_directory_batch
            as *const () as usize) as u64,
        drive_info_geometry: (crate::program::system::directory::r4os_rust_sys_drive_info_geometry
            as *const () as usize) as u64,
        program_inventory_begin:
            (crate::program::execution_inventory::r4os_rust_program_execution_begin as *const ()
                as usize) as u64,
        program_inventory_programs: (crate::program::inventory::r4os_rust_program_inventory_programs
            as *const () as usize) as u64,
        program_inventory_tasks:
            (crate::program::execution_inventory::r4os_rust_program_execution_tasks as *const ()
                as usize) as u64,
        program_inventory_threads:
            (crate::program::execution_inventory::r4os_rust_program_execution_threads as *const ()
                as usize) as u64,
        thread_create_handle: (super::platform_callbacks::sys_thread_create_handle as *const ()
            as usize) as u64,
        thread_handle_join: (super::platform_callbacks::sys_thread_handle_join as *const ()
            as usize) as u64,
        thread_handle_status: (super::platform_callbacks::sys_thread_handle_status as *const ()
            as usize) as u64,
        file_replace_atomic: (super::platform_callbacks::sys_file_replace_atomic as *const ()
            as usize) as u64,
        file_delete_if_match: (crate::program::system::namespace::r4os_rust_sys_file_delete_match
            as *const () as usize) as u64,
        file_update_atomic_checked: (crate::program::system::atomic::r4os_rust_sys_update_checked
            as *const () as usize) as u64,
        path_names_equal_collated: (crate::program::system::namespace::r4os_rust_sys_names_equal
            as *const () as usize) as u64,
        file_update_cleanup_checked: (crate::program::system::atomic::r4os_rust_sys_update_cleanup
            as *const () as usize) as u64,
        file_stream_declare_publish: (crate::program::system::atomic::r4os_rust_sys_stream_declare
            as *const () as usize) as u64,
        module_resource_stat: (crate::program::system::resources::r4os_rust_sys_resource_stat
            as *const () as usize) as u64,
        module_resource_read: (crate::program::system::resources::r4os_rust_sys_resource_read
            as *const () as usize) as u64,
        program_module_path: (crate::program::program_api::r4os_rust_program_api_module_path
            as *const () as usize) as u64,
        program_module_running: (crate::program::system::io::r4os_rust_sys_module_running
            as *const () as usize) as u64,
        monotonic_clock: (crate::program::system::control::r4os_rust_sys_monotonic as *const ()
            as usize) as u64,
        boot_ready: (super::platform_callbacks::sys_boot_ready as *const () as usize) as u64,
        registry_snapshot_begin:
            (crate::program::system::registry_api::r4os_rust_sys_registry_snapshot_begin
                as *const () as usize) as u64,
        registry_snapshot_page:
            (crate::program::system::registry_api::r4os_rust_sys_registry_snapshot_page as *const ()
                as usize) as u64,
        registry_batch_mutate: (crate::program::system::registry_batch::r4os_rust_sys_registry_batch
            as *const () as usize) as u64,
        io_file_write_at: (super::platform_callbacks::sys_io_file_write_at as *const () as usize)
            as u64,
        io_file_info: (super::platform_callbacks::sys_io_file_info as *const () as usize) as u64,
        io_file_lock: (super::platform_callbacks::sys_io_file_lock as *const () as usize) as u64,
        storage_inventory: (super::platform_callbacks::sys_storage_inventory as *const () as usize)
            as u64,
        storage_device: (super::platform_callbacks::sys_storage_device as *const () as usize)
            as u64,
        storage_partition: (super::platform_callbacks::sys_storage_partition as *const () as usize)
            as u64,
        storage_volume: (super::platform_callbacks::sys_storage_volume as *const () as usize)
            as u64,
        storage_claim_begin: (super::platform_callbacks::sys_storage_claim_begin as *const ()
            as usize) as u64,
        storage_claim_end: (super::platform_callbacks::sys_storage_claim_end as *const () as usize)
            as u64,
        storage_read: (super::platform_callbacks::sys_storage_read as *const () as usize) as u64,
        storage_claim_read: (super::platform_callbacks::sys_storage_claim_read as *const ()
            as usize) as u64,
        storage_claim_write: (super::platform_callbacks::sys_storage_claim_write as *const ()
            as usize) as u64,
        storage_claim_flush: (super::platform_callbacks::sys_storage_claim_flush as *const ()
            as usize) as u64,
        storage_rescan: (super::platform_callbacks::sys_storage_rescan as *const () as usize)
            as u64,
        storage_mount: (super::platform_callbacks::sys_storage_mount as *const () as usize) as u64,
        storage_unmount: (super::platform_callbacks::sys_storage_unmount as *const () as usize)
            as u64,
        storage_use_begin: (crate::program::system::stream_api::r4os_rust_sys_storage_use
            as *const () as usize) as u64,
        storage_use_end: (super::platform_callbacks::sys_storage_use_end as *const () as usize)
            as u64,
        directory_change_begin:
            (crate::program::system::directory::r4os_rust_sys_directory_change_begin as *const ()
                as usize) as u64,
        directory_change_poll:
            (crate::program::system::directory::r4os_rust_sys_directory_change_poll as *const ()
                as usize) as u64,
        file_copy_buffered: (crate::program::system::copy::r4os_rust_sys_file_copy_buffered
            as *const () as usize) as u64,
        notification_create: (super::platform_callbacks::sys_notification_create as *const ()
            as usize) as u64,
        notification_query: (super::platform_callbacks::sys_notification_query as *const ()
            as usize) as u64,
        notification_notify: (super::platform_callbacks::sys_notification_notify as *const ()
            as usize) as u64,
        notification_wait: (super::platform_callbacks::sys_notification_wait as *const () as usize)
            as u64,
        notification_close: (super::platform_callbacks::sys_notification_close as *const ()
            as usize) as u64,
        program_local_get: (super::platform_callbacks::sys_program_local_get as *const () as usize)
            as u64,
        program_local_publish: (super::platform_callbacks::sys_program_local_publish as *const ()
            as usize) as u64,
        thread_current_handle:
            (crate::program::thread_runtime::r4os_rust_program_thread_current_handle as *const ()
                as usize) as u64,
        cpu_capacity: (crate::program::system::control::r4os_rust_sys_cpu_capacity as *const ()
            as usize) as u64,
        program_exit: (crate::program::thread_runtime::r4os_rust_program_thread_program_exit
            as *const () as usize) as u64,
        platform_input_snapshot: (crate::input::platform_input::r4os_rust_input_platform_snapshot
            as *const () as usize) as u64,
        directory_next: (crate::program::system::directory::r4os_rust_sys_directory_next
            as *const () as usize) as u64,
        thread_placement_get: (super::thread_placement::get as *const () as usize) as u64,
        thread_placement_set: (super::thread_placement::set as *const () as usize) as u64,
        system_power_transition: (crate::program::system::control::power_transition as *const ()
            as usize) as u64,
        entropy_source_info: (crate::driver::entropy::info as *const () as usize) as u64,
        entropy_fill: (crate::driver::entropy::fill as *const () as usize) as u64,
        waitset_create: (super::platform_callbacks::sys_waitset_create as *const () as usize)
            as u64,
        waitset_enroll: (super::platform_callbacks::sys_waitset_enroll as *const () as usize)
            as u64,
        waitset_rearm: (super::platform_callbacks::sys_waitset_rearm as *const () as usize) as u64,
        waitset_remove: (super::platform_callbacks::sys_waitset_remove as *const () as usize)
            as u64,
        waitset_wait: (super::platform_callbacks::sys_waitset_wait as *const () as usize) as u64,
        waitset_close: (super::platform_callbacks::sys_waitset_close as *const () as usize) as u64,
        waitset_source_prepare: (super::platform_callbacks::sys_waitset_source_prepare as *const ()
            as usize) as u64,
        byte_pipe_create: (super::byte_pipes::create as *const () as usize) as u64,
        byte_pipe_dup: (super::byte_pipes::duplicate as *const () as usize) as u64,
        byte_pipe_read: (super::byte_pipes::read as *const () as usize) as u64,
        byte_pipe_write: (super::byte_pipes::write as *const () as usize) as u64,
        byte_pipe_cancel: (super::byte_pipes::cancel as *const () as usize) as u64,
        byte_pipe_close: (super::byte_pipes::close as *const () as usize) as u64,
        byte_pipe_info: (super::byte_pipes::info as *const () as usize) as u64,
        program_launch_io: (super::program_api::launch_io as *const () as usize) as u64,
        program_stdio: (super::launch_io::query as *const () as usize) as u64,
        ..Default::default()
    };
    SYS2.configure(value);
}

fn configure_desk2() {
    let value = c::generation2::R4XStartR4Desk2 {
        read_key: (super::platform_callbacks::desk_read_key as *const () as usize) as u64,
        mouse_state: (super::platform_callbacks::desk_mouse_state as *const () as usize) as u64,
        mouse_show: (super::platform_callbacks::desk_mouse_show as *const () as usize) as u64,
        mouse_hide: (super::platform_callbacks::desk_mouse_hide as *const () as usize) as u64,
        keyboard_layout_current: (super::platform_callbacks::desk_keyboard_layout_current
            as *const () as usize) as u64,
        keyboard_layout_at: (super::platform_callbacks::desk_keyboard_layout_at as *const ()
            as usize) as u64,
        keyboard_layout_set: (super::platform_callbacks::desk_keyboard_layout_set as *const ()
            as usize) as u64,
        program_set_window: (super::platform_callbacks::desk_program_set_window as *const ()
            as usize) as u64,
        program_set_console_host: (crate::program::program_api::r4os_rust_program_api_console_host
            as *const () as usize) as u64,
        program_request_host_launch:
            (crate::program::program_api::r4os_rust_program_api_host_request as *const () as usize)
                as u64,
        program_take_host_launch: (crate::program::program_api::r4os_rust_program_api_host_take
            as *const () as usize) as u64,
        program_window_id: (super::platform_callbacks::desk_program_window_id as *const () as usize)
            as u64,
        gui_window_info: (crate::program::gui_events::r4os_rust_program_gui_window_info as *const ()
            as usize) as u64,
        gui_set_window_info: (crate::program::gui_events::r4os_rust_program_gui_window_set
            as *const () as usize) as u64,
        gui_poll_event: (crate::program::gui_events::r4os_rust_program_gui_event_poll as *const ()
            as usize) as u64,
        gui_push_event: (crate::program::gui_events::r4os_rust_program_gui_event_push as *const ()
            as usize) as u64,
        gui_set_text: (super::platform_callbacks::desk_gui_set_text as *const () as usize) as u64,
        gui_text: (super::platform_callbacks::desk_gui_text as *const () as usize) as u64,
        gui_revision: (crate::program::gui_events::r4os_rust_program_gui_revision_read as *const ()
            as usize) as u64,
        gui_command: (crate::program::gui_api::r4os_rust_program_gui_api_command as *const ()
            as usize) as u64,
        gui_set_title: (super::platform_callbacks::desk_gui_set_title as *const () as usize) as u64,
        gui_title: (super::platform_callbacks::desk_gui_title as *const () as usize) as u64,
        gui_set_min_size: (super::platform_callbacks::desk_gui_set_min_size as *const () as usize)
            as u64,
        gui_min_size: (crate::program::gui_events::r4os_rust_program_gui_min_size as *const ()
            as usize) as u64,
        console_output: (crate::program::console::r4os_rust_program_console_api_output as *const ()
            as usize) as u64,
        console_revision: (crate::program::console::r4os_rust_program_console_api_revision
            as *const () as usize) as u64,
        console_state: (crate::program::console::r4os_rust_program_console_api_state as *const ()
            as usize) as u64,
        console_set_metrics: (crate::program::console::r4os_rust_program_console_api_metrics
            as *const () as usize) as u64,
        console_push_key: (super::platform_callbacks::desk_console_push_key as *const () as usize)
            as u64,
        console_write: (super::platform_callbacks::desk_console_write as *const () as usize) as u64,
        console_read: (crate::program::console_input::r4os_rust_program_console_input_api_read
            as *const () as usize) as u64,
        clipboard_write: (super::platform_callbacks::desk_clipboard_write as *const () as usize)
            as u64,
        clipboard_read: (super::platform_callbacks::desk_clipboard_read as *const () as usize)
            as u64,
        clipboard_revision: (super::platform_callbacks::desk_clipboard_revision as *const ()
            as usize) as u64,
        clipboard_info: (super::platform_callbacks::desk_clipboard_info as *const () as usize)
            as u64,
        clipboard_clear: (super::platform_callbacks::desk_clipboard_clear as *const () as usize)
            as u64,
        read_key_codepoint: (super::platform_callbacks::desk_read_key_codepoint as *const ()
            as usize) as u64,
        remote_frame_info: (super::platform_callbacks::desk_remote_frame_info as *const () as usize)
            as u64,
        remote_frame_read: (super::platform_callbacks::desk_remote_frame_read as *const () as usize)
            as u64,
        remote_frame_wait: (super::platform_callbacks::desk_remote_frame_wait as *const () as usize)
            as u64,
        remote_frame_publish:
            (crate::program::domain_api::r4os_rust_program_domain_remote_frame_publish as *const ()
                as usize) as u64,
        remote_input_push: (super::platform_callbacks::desk_remote_input_push as *const () as usize)
            as u64,
        remote_input_poll: (super::platform_callbacks::desk_remote_input_poll as *const () as usize)
            as u64,
        remote_input_status: (super::platform_callbacks::desk_remote_input_status as *const ()
            as usize) as u64,
        desktop_activity_wait: (super::platform_callbacks::desk_desktop_activity_wait as *const ()
            as usize) as u64,
        remote_frame_map: (super::platform_callbacks::desk_remote_frame_map as *const () as usize)
            as u64,
        program_current_console_host: (super::platform_callbacks::desk_program_current_console_host
            as *const () as usize) as u64,
        program_request_desktop: (super::platform_callbacks::desk_program_request_desktop
            as *const () as usize) as u64,
        program_spawn_with_console_host:
            (super::platform_callbacks::desk_program_spawn_with_console_host as *const () as usize)
                as u64,
        program_spawn_with_console_host_handle:
            (super::platform_callbacks::desk_program_spawn_with_console_host_handle as *const ()
                as usize) as u64,
        program_set_window_handle: (super::platform_callbacks::desk_program_set_window_handle
            as *const () as usize) as u64,
        console_push_input: (super::platform_callbacks::desk_console_push_input as *const ()
            as usize) as u64,
        remote_frame_acquire:
            (crate::program::domain_api::r4os_rust_program_domain_remote_frame_acquire as *const ()
                as usize) as u64,
        remote_frame_release:
            (crate::program::domain_api::r4os_rust_program_domain_remote_frame_release as *const ()
                as usize) as u64,
        remote_frame_consumers: (super::platform_callbacks::desk_remote_frame_consumers as *const ()
            as usize) as u64,
        remote_frame_publish_regions:
            (crate::program::domain_api::r4os_rust_program_domain_remote_frame_publish_regions
                as *const () as usize) as u64,
        console_input_wait: (crate::program::console_input::r4os_rust_program_console_input_wait
            as *const () as usize) as u64,
        physical_key_poll: (super::platform_callbacks::desk_physical_key_poll as *const () as usize)
            as u64,
        mouse_motion: (super::platform_callbacks::desk_mouse_motion as *const () as usize) as u64,
        remote_frame_snapshot_acquire:
            (crate::program::domain_api::r4os_rust_program_domain_remote_frame_snapshot_acquire
                as *const () as usize) as u64,
        remote_frame_snapshot_release:
            (crate::program::domain_api::r4os_rust_program_domain_remote_frame_snapshot_release
                as *const () as usize) as u64,
        remote_frame_source_reset:
            (crate::program::domain_api::r4os_rust_program_domain_remote_frame_source_reset
                as *const () as usize) as u64,
        remote_frame_capture_stats: (super::platform_callbacks::desk_remote_frame_capture_stats
            as *const () as usize) as u64,
        desktop_activity_notify: (super::platform_callbacks::desk_desktop_activity_notify
            as *const () as usize) as u64,
        console_read_events: (super::console_input::read_events as *const () as usize) as u64,
        console_push_events: (super::console_input::push_events as *const () as usize) as u64,
        console_text_info: (super::console::api_text_info as *const () as usize) as u64,
        console_set_encoding: (super::console::api_set_encoding as *const () as usize) as u64,
        gui_activity_wait: (super::platform_callbacks::desk_gui_activity_wait as *const () as usize)
            as u64,
        gui_activity_notify: (super::platform_callbacks::desk_gui_activity_notify as *const ()
            as usize) as u64,
        gui_pointer_capture_info: (super::gui_pointer::info as *const () as usize) as u64,
        gui_pointer_capture_acquire: (super::gui_pointer::acquire as *const () as usize) as u64,
        gui_pointer_capture_release: (super::gui_pointer::release as *const () as usize) as u64,
        gui_pointer_capture_host: (super::gui_pointer::host as *const () as usize) as u64,
        gui_host_input: (super::gui_pointer::host_input as *const () as usize) as u64,
        gui_window_appearance: (super::gui_pointer::appearance as *const () as usize) as u64,
        ..Default::default()
    };
    DESK2.configure(value);
}

fn configure_draw2() {
    let value = c::generation2::R4XStartR4Draw2 {
        screen_width: (super::platform_callbacks::draw_screen_width as *const () as usize) as u64,
        screen_height: (super::platform_callbacks::draw_screen_height as *const () as usize) as u64,
        clear: (super::platform_callbacks::draw_clear as *const () as usize) as u64,
        rect: (super::platform_callbacks::draw_rect as *const () as usize) as u64,
        text: (super::platform_callbacks::draw_text as *const () as usize) as u64,
        display_revision: (super::platform_callbacks::draw_display_revision as *const () as usize) as u64,
        display_begin_frame: (super::platform_callbacks::draw_display_begin_frame as *const () as usize) as u64,
        display_begin_frame_rect: (super::platform_callbacks::draw_display_begin_frame_rect as *const () as usize) as u64,
        display_present: (super::platform_callbacks::draw_display_present as *const () as usize) as u64,
        display_blit_xrgb32: (super::platform_callbacks::draw_display_blit_xrgb32 as *const () as usize) as u64,
        gui_clear: (crate::program::gui_draw::r4os_rust_program_gui_clear as *const () as usize) as u64,
        gui_rect: (crate::program::gui_draw::r4os_rust_program_gui_rect as *const () as usize) as u64,
        gui_draw_text: (super::platform_callbacks::draw_gui_draw_text as *const () as usize) as u64,
        gui_draw_text_ex: (super::platform_callbacks::draw_gui_draw_text_ex as *const () as usize) as u64,
        gui_blit: (crate::program::gui_draw::r4os_rust_program_gui_blit as *const () as usize) as u64,
        gui_raster_read: (crate::program::gui_api::r4os_rust_program_gui_api_raster as *const () as usize) as u64,
        gui_present: (super::platform_callbacks::draw_gui_present as *const () as usize) as u64,
        font_count: (super::platform_callbacks::draw_font_count as *const () as usize) as u64,
        font_info: (super::platform_callbacks::draw_font_info as *const () as usize) as u64,
        font_measure: (super::platform_callbacks::draw_font_measure as *const () as usize) as u64,
        gui_set_font: (crate::program::gui_font::r4os_rust_program_gui_font_set as *const () as usize) as u64,
        gui_font: (crate::program::gui_font::r4os_rust_program_gui_font_get as *const () as usize) as u64,
        text_font: (super::platform_callbacks::draw_text_font as *const () as usize) as u64,
        font_reload: (super::platform_callbacks::draw_font_reload as *const () as usize) as u64,
        font_glyph_row: (super::platform_callbacks::draw_font_glyph_row as *const () as usize) as u64,
        gui_blend_alpha8: (crate::program::gui_draw::r4os_rust_program_gui_alpha as *const () as usize) as u64,
        gui_frame_begin: (super::platform_callbacks::draw_gui_frame_begin as *const () as usize) as u64,
        gui_frame_append: (crate::program::gui_api::r4os_rust_program_gui_api_append as *const () as usize) as u64,
        gui_frame_commit: (super::platform_callbacks::draw_gui_frame_commit as *const () as usize) as u64,
        gui_frame_cancel: (super::platform_callbacks::draw_gui_frame_cancel as *const () as usize) as u64,
        gui_frame_info: (super::platform_callbacks::draw_gui_frame_info as *const () as usize) as u64,
        gui_frame_read: (crate::program::gui_api::r4os_rust_program_gui_api_read as *const () as usize) as u64,
        display_blit_xrgb32_stride: (super::platform_callbacks::draw_display_blit_xrgb32_stride as *const () as usize) as u64,
        display_present_regions: (super::platform_callbacks::draw_display_present_regions as *const () as usize) as u64,
        display_present_capabilities: (super::platform_callbacks::draw_display_present_capabilities as *const () as usize) as u64,
        display_present_completion: (super::platform_callbacks::draw_display_present_completion as *const () as usize) as u64,
        gui_frame_begin_damage: (super::platform_callbacks::draw_gui_frame_begin_damage as *const () as usize) as u64,
        gui_frame_generation_info: (super::platform_callbacks::draw_gui_frame_generation_info as *const () as usize) as u64,
        gui_frame_generation_read: (crate::program::gui_api::r4os_rust_program_gui_api_generation_read as *const () as usize) as u64,
        font_glyph_bitmap: (super::platform_callbacks::draw_font_glyph_bitmap as *const () as usize) as u64,
        font_revision: (super::platform_callbacks::draw_font_revision as *const () as usize) as u64,
        gui_frame_begin_replace: (super::platform_callbacks::draw_gui_frame_begin_replace as *const () as usize) as u64,
        gui_frame_stream_info: (super::platform_callbacks::draw_gui_frame_stream_info as *const () as usize) as u64,
        gui_shared_raster_create: (crate::program::shared_raster::r4os_rust_program_shared_api_create as *const () as usize) as u64,
        gui_shared_raster_destroy: (crate::program::shared_raster::r4os_rust_program_shared_api_destroy as *const () as usize) as u64,
        gui_shared_raster_map_write: (crate::program::shared_raster::r4os_rust_program_shared_api_write as *const () as usize) as u64,
        gui_shared_raster_publish: (crate::program::shared_raster::r4os_rust_program_shared_api_publish as *const () as usize) as u64,
        gui_shared_raster_acquire: (crate::program::shared_raster::r4os_rust_program_shared_api_acquire as *const () as usize) as u64,
        gui_shared_raster_release: (crate::program::shared_raster::r4os_rust_program_shared_api_release as *const () as usize) as u64,
        gfx_buffer_create: (crate::program::domain_api::r4os_rust_program_domain_gfx_buffer_create as *const () as usize) as u64,
        gfx_buffer_describe: (crate::program::domain_api::r4os_rust_program_domain_gfx_buffer_describe as *const () as usize) as u64,
        gfx_buffer_import: (crate::program::domain_api::r4os_rust_program_domain_gfx_buffer_import as *const () as usize) as u64,
        gfx_buffer_release: (crate::program::domain_api::r4os_rust_program_domain_gfx_buffer_release as *const () as usize) as u64,
        gfx_buffer_map: (crate::program::domain_api::r4os_rust_program_domain_gfx_buffer_map as *const () as usize) as u64,
        gfx_buffer_unmap: (crate::program::domain_api::r4os_rust_program_domain_gfx_buffer_unmap as *const () as usize) as u64,
        gfx_buffer_export_raster: (crate::program::shared_raster::r4os_rust_program_shared_api_export as *const () as usize) as u64,
        gfx_buffer_stats: (super::platform_callbacks::draw_gfx_buffer_stats as *const () as usize) as u64,
        gfx_queue_open: (crate::program::domain_api::r4os_rust_program_domain_gfx_queue_open as *const () as usize) as u64,
        gfx_queue_close: (crate::program::domain_api::r4os_rust_program_domain_gfx_queue_close as *const () as usize) as u64,
        gfx_queue_submit: (crate::program::domain_api::r4os_rust_program_domain_gfx_queue_submit as *const () as usize) as u64,
        gfx_fence_query: (super::platform_callbacks::draw_gfx_fence_query as *const () as usize) as u64,
        gfx_fence_wait: (super::platform_callbacks::draw_gfx_fence_wait as *const () as usize) as u64,
        gfx_fence_cancel: (crate::program::domain_api::r4os_rust_program_domain_gfx_fence_cancel as *const () as usize) as u64,
        gfx_fence_release: (crate::program::domain_api::r4os_rust_program_domain_gfx_fence_release as *const () as usize) as u64,
        gfx_queue_backend: (super::platform_callbacks::draw_gfx_queue_backend as *const () as usize) as u64,
        gfx_output_revision: (super::platform_callbacks::draw_gfx_output_revision as *const () as usize) as u64,
        gfx_output_info: (super::platform_callbacks::draw_gfx_output_info as *const () as usize) as u64,
        gfx_output_mode: (super::platform_callbacks::draw_gfx_output_mode as *const () as usize) as u64,
        gfx_output_edid: (super::platform_callbacks::draw_gfx_output_edid as *const () as usize) as u64,
        gfx_atomic_test: (crate::program::domain_api::r4os_rust_program_domain_gfx_atomic_test as *const () as usize) as u64,
        gfx_atomic_commit: (crate::program::domain_api::r4os_rust_program_domain_gfx_atomic_commit as *const () as usize) as u64,
        gfx_atomic_submit: (crate::program::domain_api::r4os_rust_program_domain_gfx_atomic_submit as *const () as usize) as u64,
        gfx_atomic_status: (super::platform_callbacks::draw_gfx_atomic_status as *const () as usize) as u64,
        gfx_atomic_resolve: (crate::program::domain_api::r4os_rust_program_domain_gfx_atomic_resolve as *const () as usize) as u64,
        display_presentation_stats: (super::platform_callbacks::draw_display_presentation_stats as *const () as usize) as u64,
        display_cursor_info: (super::platform_callbacks::draw_display_cursor_info as *const () as usize) as u64,
        display_cursor_submit: (crate::program::domain_api::r4os_rust_program_domain_display_cursor_submit as *const () as usize) as u64,
        display_cursor_status: (super::platform_callbacks::draw_display_cursor_status as *const () as usize) as u64,
        gfx_queue_backend_info: (super::platform_callbacks::draw_gfx_queue_backend_info as *const () as usize) as u64,
        gfx_native_start: (crate::program::domain_api::r4os_rust_program_domain_gfx_native_start as *const () as usize) as u64,
        gfx_native_query: (crate::program::domain_api::r4os_rust_program_domain_gfx_native_query as *const () as usize) as u64,
        gfx_native_receive: (crate::program::domain_api::r4os_rust_program_domain_gfx_native_receive as *const () as usize) as u64,
        gfx_native_close: (crate::program::domain_api::r4os_rust_program_domain_gfx_native_close as *const () as usize) as u64,
        gfx_native_wait: (crate::program::domain_api::r4os_rust_program_domain_gfx_native_wait as *const () as usize) as u64,
        gfx_queue_submit_render_list: (crate::program::domain_api::r4os_rust_program_domain_gfx_queue_submit_render_list as *const () as usize) as u64,
        display_presentation_info: (super::platform_callbacks::draw_display_presentation_info as *const () as usize) as u64,
        display_presentation_feedback: (super::platform_callbacks::draw_display_presentation_feedback as *const () as usize) as u64,
        display_output_target: (super::platform_callbacks::draw_display_output_target as *const () as usize) as u64,
        gfx_queue_submit_output: (crate::program::domain_api::r4os_rust_program_domain_gfx_queue_submit_output as *const () as usize) as u64,
        display_output_presentation_info: (super::platform_callbacks::draw_display_output_presentation_info as *const () as usize) as u64,
        display_output_presentation_feedback: (super::platform_callbacks::draw_display_output_presentation_feedback as *const () as usize) as u64,
        gfx_queue_submit_render_grid_list: (crate::program::domain_api::r4os_rust_program_domain_gfx_queue_submit_render_grid_list as *const () as usize) as u64,
        gfx_output_color: (super::platform_callbacks::draw_gfx_output_color as *const () as usize) as u64,
        gfx_queue_submit_render_color_list: (crate::program::domain_api::r4os_rust_program_domain_gfx_queue_submit_render_color_list as *const () as usize) as u64,
        gfx_atomic_test_color: (crate::program::domain_api::r4os_rust_program_domain_gfx_atomic_test_color as *const () as usize) as u64,
        gfx_atomic_submit_color: (crate::program::domain_api::r4os_rust_program_domain_gfx_atomic_submit_color as *const () as usize) as u64,
        gfx_output_refresh: (super::platform_callbacks::draw_gfx_output_refresh as *const () as usize) as u64,
        gfx_refresh_request: (crate::program::domain_api::r4os_rust_program_domain_gfx_refresh_request as *const () as usize) as u64,
        gfx_memory_budget: (super::platform_callbacks::draw_gfx_memory_budget as *const () as usize) as u64,
        gfx_telemetry: (super::platform_callbacks::draw_gfx_telemetry as *const () as usize) as u64,
        gfx_output_power: (super::platform_callbacks::draw_gfx_output_power as *const () as usize) as u64,
        gfx_power_request: (crate::program::domain_api::r4os_rust_program_domain_gfx_power_request as *const () as usize) as u64,
        gfx_virtual_start: (crate::program::domain_api::r4os_rust_program_domain_gfx_virtual_start as *const () as usize) as u64,
        gfx_virtual_query: (crate::program::domain_api::r4os_rust_program_domain_gfx_virtual_query as *const () as usize) as u64,
        gfx_virtual_close: (crate::program::domain_api::r4os_rust_program_domain_gfx_virtual_close as *const () as usize) as u64,
        gfx_virtual_wait: (crate::program::domain_api::r4os_rust_program_domain_gfx_virtual_wait as *const () as usize) as u64,
        gfx_buffer_map_persistent: (crate::program::domain_api::r4os_rust_program_domain_gfx_buffer_map_persistent as *const () as usize) as u64,
        gfx_queue_backend_properties: (super::platform_callbacks::draw_gfx_queue_backend_properties as *const () as usize) as u64,
        gfx_queue_submit_native: (crate::program::domain_api::r4os_rust_program_domain_gfx_queue_submit_native as *const () as usize) as u64,
        gfx_queue_submit_render_color_grid_list: (crate::program::domain_api::r4os_rust_program_domain_gfx_queue_submit_render_color_grid_list as *const () as usize) as u64,
        gfx_output_brightness: (super::platform_callbacks::draw_gfx_output_brightness as *const () as usize) as u64,
        gfx_brightness_request: (crate::program::domain_api::r4os_rust_program_domain_gfx_brightness_request as *const () as usize) as u64,
        font_prefix_metrics2: (super::font_metrics::r4os_rust_draw_font_prefix_metrics2 as *const () as usize) as u64,
        ..Default::default()
    };
    DRAW2.configure(value);
}

fn configure_net2() {
    let value = c::generation2::R4XStartR4Net2 {
        tcp_connect: (crate::program::network_api::r4os_rust_r4net_tcp_connect as *const ()
            as usize) as u64,
        tcp_write: (crate::program::network_api::r4os_rust_r4net_tcp_write as *const () as usize)
            as u64,
        tcp_read: (crate::program::network_api::r4os_rust_r4net_tcp_read as *const () as usize)
            as u64,
        tcp_close: (crate::program::network_api::r4os_rust_r4net_tcp_close as *const () as usize)
            as u64,
        tcp_summary: (crate::program::network_api::r4os_rust_r4net_tcp_summary as *const ()
            as usize) as u64,
        tcp_connection: (crate::program::network_api::r4os_rust_r4net_tcp_connection as *const ()
            as usize) as u64,
        tcp_echo_listen_once: (crate::program::network_api::r4os_rust_r4net_tcp_echo_listen_once
            as *const () as usize) as u64,
        tcp_accept_read_once: (crate::program::network_api::r4os_rust_r4net_tcp_accept_read_once
            as *const () as usize) as u64,
        net_ipv4_send: (crate::program::network_api::r4os_rust_r4net_net_ipv4_send as *const ()
            as usize) as u64,
        net_ipv4_recv: (crate::program::network_api::r4os_rust_r4net_net_ipv4_recv as *const ()
            as usize) as u64,
        net_config_get: (crate::program::network_api::r4os_rust_r4net_net_config_get as *const ()
            as usize) as u64,
        net_config_set: (crate::program::network_api::r4os_rust_r4net_net_config_set as *const ()
            as usize) as u64,
        net_dns_resolve: (crate::program::network_api::r4os_rust_r4net_net_dns_resolve as *const ()
            as usize) as u64,
        net_dns_resolve_server: (crate::program::network_api::r4os_rust_r4net_net_dns_resolve_server
            as *const () as usize) as u64,
        net_dhcp_acquire: (crate::program::network_api::r4os_rust_r4net_net_dhcp_acquire
            as *const () as usize) as u64,
        net_dhcp_renew: (crate::program::network_api::r4os_rust_r4net_net_dhcp_renew as *const ()
            as usize) as u64,
        net_dhcp_release: (crate::program::network_api::r4os_rust_r4net_net_dhcp_release
            as *const () as usize) as u64,
        net_dhcp_status: (crate::program::network_api::r4os_rust_r4net_net_dhcp_status as *const ()
            as usize) as u64,
        net_detail_get: (crate::program::network_api::r4os_rust_r4net_net_detail_get as *const ()
            as usize) as u64,
        net_diag_run: (crate::program::network_api::r4os_rust_r4net_net_diag_run as *const ()
            as usize) as u64,
        ipc_open: (crate::program::network_api::r4os_rust_r4net_ipc_open as *const () as usize)
            as u64,
        ipc_send: (crate::program::network_api::r4os_rust_r4net_ipc_send as *const () as usize)
            as u64,
        ipc_recv: (crate::program::network_api::r4os_rust_r4net_ipc_recv as *const () as usize)
            as u64,
        ipc_poll: (crate::program::network_api::r4os_rust_r4net_ipc_poll as *const () as usize)
            as u64,
        ipc_close: (crate::program::network_api::r4os_rust_r4net_ipc_close as *const () as usize)
            as u64,
        ipc_summary: (crate::program::network_api::r4os_rust_r4net_ipc_summary as *const ()
            as usize) as u64,
        ipc_channel: (crate::program::network_api::r4os_rust_r4net_ipc_channel as *const ()
            as usize) as u64,
        serial_link_status: (crate::program::network_api::r4os_rust_r4net_serial_link_status
            as *const () as usize) as u64,
        serial_link_poll: (crate::program::network_api::r4os_rust_r4net_serial_link_poll
            as *const () as usize) as u64,
        serial_link_send_message:
            (crate::program::network_api::r4os_rust_r4net_serial_link_send_message as *const ()
                as usize) as u64,
        serial_link_host_test: (crate::program::network_api::r4os_rust_r4net_serial_link_host_test
            as *const () as usize) as u64,
        serial_link_inbox: (crate::program::network_api::r4os_rust_r4net_serial_link_inbox
            as *const () as usize) as u64,
        net_service_request: (crate::program::network_api::r4os_rust_r4net_net_service_request
            as *const () as usize) as u64,
        ipc_performance: (crate::program::network_api::r4os_rust_r4net_ipc_performance as *const ()
            as usize) as u64,
        tcp_performance: (crate::program::network_api::r4os_rust_r4net_tcp_performance as *const ()
            as usize) as u64,
        ..Default::default()
    };
    NET2.configure(value);
}

fn configure_audio2() {
    let value = c::generation2::R4XStartR4Audio2 {
        audio_capture_io: (crate::program::domain_api::r4os_rust_program_domain_audio_capture_io
            as *const () as usize) as u64,
        audio_output_catalog:
            (crate::program::domain_api::r4os_rust_program_domain_audio_output_catalog as *const ()
                as usize) as u64,
        audio_open_stream: (crate::program::domain_api::r4os_rust_program_domain_audio_open_stream
            as *const () as usize) as u64,
        audio_stream_progress:
            (crate::program::domain_api::r4os_rust_program_domain_audio_stream_progress as *const ()
                as usize) as u64,
        audio_set_stream_expectation:
            (crate::program::domain_api::r4os_rust_program_domain_audio_set_stream_expectation
                as *const () as usize) as u64,
        audio_close_stream_progress:
            (crate::program::domain_api::r4os_rust_program_domain_audio_close_stream_progress
                as *const () as usize) as u64,
        audio_write: (crate::program::domain_api::r4os_rust_program_domain_audio_write as *const ()
            as usize) as u64,
        audio_close: (crate::program::domain_api::r4os_rust_program_domain_audio_close as *const ()
            as usize) as u64,
        audio_set_volume: (crate::program::domain_api::r4os_rust_program_domain_audio_set_volume
            as *const () as usize) as u64,
        sid_acquire: (crate::program::r4audio::r4os_rust_r4audio_sid_acquire as *const () as usize)
            as u64,
        sid_write_register: (crate::program::r4audio::r4os_rust_r4audio_sid_write_register
            as *const () as usize) as u64,
        sid_release: (crate::program::r4audio::r4os_rust_r4audio_sid_release as *const () as usize)
            as u64,
        sid_load_data: (crate::program::r4audio::r4os_rust_r4audio_sid_load_data as *const ()
            as usize) as u64,
        sid_init: (crate::program::r4audio::r4os_rust_r4audio_sid_init as *const () as usize)
            as u64,
        sid_play_frame: (crate::program::r4audio::r4os_rust_r4audio_sid_play_frame as *const ()
            as usize) as u64,
        sid_stop: (crate::program::r4audio::r4os_rust_r4audio_sid_stop as *const () as usize)
            as u64,
        sid_model_name: (crate::program::r4audio::r4os_rust_r4audio_sid_model_name as *const ()
            as usize) as u64,
        midi_open_synth: (crate::program::r4audio::r4os_rust_r4audio_midi_open_synth as *const ()
            as usize) as u64,
        midi_send: (crate::program::r4audio::r4os_rust_r4audio_midi_send as *const () as usize)
            as u64,
        midi_close: (crate::program::r4audio::r4os_rust_r4audio_midi_close as *const () as usize)
            as u64,
        opl3_write_register: (crate::program::r4audio::r4os_rust_r4audio_opl3_write_register
            as *const () as usize) as u64,
        opl3_reset: (crate::program::r4audio::r4os_rust_r4audio_opl3_reset as *const () as usize)
            as u64,
        opl3_render_block: (crate::program::r4audio::r4os_rust_r4audio_opl3_render_block
            as *const () as usize) as u64,
        opl3_stop: (crate::program::r4audio::r4os_rust_r4audio_opl3_stop as *const () as usize)
            as u64,
        audio_output_info: (crate::program::domain_api::r4os_rust_program_domain_audio_output_info
            as *const () as usize) as u64,
        audio_select_output:
            (crate::program::domain_api::r4os_rust_program_domain_audio_select_output as *const ()
                as usize) as u64,
        ..Default::default()
    };
    AUDIO2.configure(value);
}

fn configure_dev2() {
    use super::r4dev;
    const HEAP_CHECK: c::generation2::R4DevFns2::memory_kernel_heap_check =
        r4dev::memory::kernel_heap_check;
    const SCHEDULER_CHECK: c::generation2::R4DevFns2::scheduler_kernel_check =
        r4dev::scheduler::kernel_check;
    const NVME_CHECK: c::generation2::R4DevFns2::storage_nvme_check = r4dev::nvme::check;
    const USB_CHECK: c::generation2::R4DevFns2::usb_contract_check = r4dev::usb::check;
    const BLOCK_QUEUE_CHECK: c::generation2::R4DevFns2::storage_block_queue_check =
        r4dev::block_queue::check;
    const SMP_CHECK: c::generation2::R4DevFns2::smp_kernel_check = r4dev::smp::check;
    const SMP_SNAPSHOT: c::generation2::R4DevFns2::performance_smp_tlb = r4dev::smp::snapshot;
    const CPU_TOPOLOGY: c::generation2::R4DevFns2::cpu_topology = r4dev::cpu_topology::info;
    const CPU_CACHE: c::generation2::R4DevFns2::cpu_cache_topology = r4dev::cpu_topology::cache;
    const CPU_LIFECYCLE: c::generation2::R4DevFns2::cpu_lifecycle = r4dev::cpu_lifecycle::control;
    const CPU_LIFECYCLE_CHECK: c::generation2::R4DevFns2::kernel_cpu_lifecycle_check =
        r4dev::cpu_lifecycle::check;
    const CPU_CAPABILITIES: c::generation2::R4DevFns2::cpu_capabilities =
        r4dev::cpu_capabilities::info;
    const CPU_FEATURE: c::generation2::R4DevFns2::cpu_feature = r4dev::cpu_capabilities::feature;
    const CPU_REGISTER_READ: c::generation2::R4DevFns2::cpu_register_read =
        r4dev::cpu_capabilities::read;
    const CPU_REGISTER_CHECK: c::generation2::R4DevFns2::kernel_cpu_register_check =
        r4dev::cpu_capabilities::check;
    const CPU_POWER_INFO: c::generation2::R4DevFns2::cpu_power_info =
        crate::platform::cpu_power::info;
    const CPU_POWER_CAPTURE: c::generation2::R4DevFns2::cpu_power_capture =
        crate::platform::cpu_power::capture;
    const CPU_PERFORMANCE: c::generation2::R4DevFns2::cpu_performance =
        crate::platform::cpu_power::performance;
    const CPU_CPPC: c::generation2::R4DevFns2::cpu_cppc = crate::platform::cpu_power::cppc;
    const CPU_HINT: c::generation2::R4DevFns2::cpu_performance_hint =
        crate::platform::cpu_power::hints::info;
    const CPU_FIRMWARE: c::generation2::R4DevFns2::cpu_firmware_performance =
        crate::platform::cpu_power::firmware::call;
    const CPU_IDLE: c::generation2::R4DevFns2::cpu_idle_info = r4dev::cpu_capabilities::idle;
    const CPU_MACHINE_INFO: c::generation2::R4DevFns2::cpu_machine_info =
        r4dev::cpu_capabilities::machine_info;
    const CPU_MACHINE_BANK: c::generation2::R4DevFns2::cpu_machine_bank =
        r4dev::cpu_capabilities::machine_bank;
    const CPU_MICROCODE_INFO: c::generation2::R4DevFns2::cpu_microcode_info =
        r4dev::cpu_capabilities::microcode;
    const CPU_PMU_INFO: c::generation2::R4DevFns2::cpu_pmu_info = r4dev::cpu_capabilities::pmu_info;
    const CPU_PMU_MEASURE: c::generation2::R4DevFns2::cpu_pmu_measure =
        r4dev::cpu_capabilities::pmu_measure;
    const CPU_PMU_RESULT: c::generation2::R4DevFns2::cpu_pmu_result =
        r4dev::cpu_capabilities::pmu_result;
    const CPU_ACCELERATION_INFO: c::generation2::R4DevFns2::cpu_acceleration_info =
        r4dev::cpu_capabilities::acceleration_info;
    const CPU_ACCELERATION_WORK: c::generation2::R4DevFns2::cpu_acceleration_work =
        r4dev::cpu_capabilities::acceleration_work;
    const FPU_OBSERVATION: c::generation2::R4DevFns2::fpu_observation =
        r4dev::fpu_observation::call;
    const FIRMWARE_SYSTEM_UUID: c::generation2::R4DevFns2::firmware_system_uuid =
        r4dev::devices::r4os_rust_r4dev_firmware_system_uuid;
    r4dev::providers::install();
    let value = c::generation2::R4XStartR4Dev2 {
        serial_count: (super::serial_api::count as c::R4DevFns::serial_count as *const () as usize)
            as u64,
        serial_port_at: (super::serial_api::port_at as c::R4DevFns::serial_port_at as *const ()
            as usize) as u64,
        serial_open: (super::serial_api::open as c::R4DevFns::serial_open as *const () as usize)
            as u64,
        serial_configure: (super::serial_api::configure as c::R4DevFns::serial_configure
            as *const () as usize) as u64,
        serial_read: (super::serial_api::read as c::R4DevFns::serial_read as *const () as usize)
            as u64,
        serial_write: (super::serial_api::write as c::R4DevFns::serial_write as *const () as usize)
            as u64,
        serial_drain: (super::serial_api::drain as c::R4DevFns::serial_drain as *const () as usize)
            as u64,
        serial_status: (super::serial_api::status as c::R4DevFns::serial_status as *const ()
            as usize) as u64,
        serial_cancel: (super::serial_api::cancel as c::R4DevFns::serial_cancel as *const ()
            as usize) as u64,
        serial_close: (super::serial_api::close as c::R4DevFns::serial_close as *const () as usize)
            as u64,
        device_inventory_summary: (r4dev::devices::r4os_rust_r4dev_device_inventory_summary
            as *const () as usize) as u64,
        device_inventory_record: (r4dev::devices::r4os_rust_r4dev_device_inventory_record
            as *const () as usize) as u64,
        device_inventory_capture: (r4dev::devices::r4os_rust_r4dev_device_inventory_capture
            as c::R4DevFns::device_inventory_capture as *const ()
            as usize) as u64,
        memory_summary: (r4dev::memory::r4os_rust_r4dev_memory_summary as *const () as usize)
            as u64,
        memory_block_count: (r4dev::memory::r4os_rust_r4dev_memory_block_count as *const ()
            as usize) as u64,
        memory_block: (r4dev::memory::r4os_rust_r4dev_memory_block as *const () as usize) as u64,
        memory_pressure_snapshot: (r4dev::memory::r4os_rust_r4dev_memory_pressure_snapshot
            as *const () as usize) as u64,
        memory_reclaim_probe: (r4dev::memory::r4os_rust_r4dev_memory_reclaim_probe as *const ()
            as usize) as u64,
        memory_backing_store_probe:
            (r4dev::memory_probes::r4os_rust_r4dev_memory_backing_store_probe as *const () as usize)
                as u64,
        memory_backing_store_slot_probe:
            (r4dev::memory_probes::r4os_rust_r4dev_memory_backing_store_slot_probe as *const ()
                as usize) as u64,
        memory_pager_gate_probe: (r4dev::memory_probes::r4os_rust_r4dev_memory_pager_gate_probe
            as *const () as usize) as u64,
        memory_page_io_probe: (r4dev::memory_probes::r4os_rust_r4dev_memory_page_io_probe
            as *const () as usize) as u64,
        memory_vm_page_state_probe:
            (r4dev::memory_probes::r4os_rust_r4dev_memory_vm_page_state_probe as *const () as usize)
                as u64,
        memory_vm_reserve_probe: (super::vm::r4os_rust_program_vm_probe as *const () as usize)
            as u64,
        paging_summary: (r4dev::paging::r4os_rust_r4dev_paging_summary as *const () as usize)
            as u64,
        performance_summary: (r4dev::summary::r4os_rust_r4dev_performance_summary as *const ()
            as usize) as u64,
        performance_task: (r4dev::paging::r4os_rust_r4dev_performance_task as *const () as usize)
            as u64,
        performance_storage: (r4dev::storage::r4os_rust_r4dev_performance_storage as *const ()
            as usize) as u64,
        performance_boot_phase: (r4dev::boot::r4os_rust_r4dev_performance_boot_phase as *const ()
            as usize) as u64,
        protocol_status: (r4dev::protocol::r4os_rust_r4dev_protocol_status as *const () as usize)
            as u64,
        protocol_dispatch: (r4dev::protocol::r4os_rust_r4dev_protocol_dispatch as *const ()
            as usize) as u64,
        display_summary: (r4dev::display::r4os_rust_r4dev_display_summary as *const () as usize)
            as u64,
        hardware_summary: (r4dev::devices::r4os_rust_r4dev_hardware_summary as *const () as usize)
            as u64,
        boot_info_summary: (r4dev::boot::r4os_rust_r4dev_boot_info_summary as *const () as usize)
            as u64,
        boot_info_memory_count: (r4dev::boot::r4os_rust_r4dev_boot_info_memory_count as *const ()
            as usize) as u64,
        boot_info_memory_entry: (r4dev::boot::r4os_rust_r4dev_boot_info_memory_entry as *const ()
            as usize) as u64,
        program_instance_storage_self_test:
            (r4dev::providers::r4os_rust_r4dev_program_instance_storage_self_test as *const ()
                as usize) as u64,
        program_registry_summary_v2: (r4dev::providers::r4os_rust_r4dev_program_registry_summary_v2
            as *const () as usize) as u64,
        program_registry_self_test_v2:
            (r4dev::providers::r4os_rust_r4dev_program_registry_self_test_v2 as *const () as usize)
                as u64,
        execution_inventory_summary: (r4dev::providers::r4os_rust_r4dev_execution_inventory_summary
            as *const () as usize) as u64,
        program_instance_storage_summary_v2:
            (r4dev::providers::r4os_rust_r4dev_program_instance_storage_summary_v2 as *const ()
                as usize) as u64,
        kernel_version: (r4dev::providers::r4os_rust_r4dev_kernel_version as *const () as usize)
            as u64,
        performance_boot_phase_clock: (r4dev::boot::r4os_rust_r4dev_performance_boot_phase_clock
            as *const () as usize) as u64,
        performance_irq_timing: (r4dev::telemetry::r4os_rust_r4dev_performance_irq_timing
            as *const () as usize) as u64,
        performance_boot_summary: (r4dev::boot::r4os_rust_r4dev_performance_boot_summary
            as *const () as usize) as u64,
        performance_driver_work: (r4dev::telemetry::r4os_rust_r4dev_performance_driver_work
            as *const () as usize) as u64,
        performance_pci_inventory: (r4dev::telemetry::r4os_rust_r4dev_performance_pci_inventory
            as *const () as usize) as u64,
        performance_input: (r4dev::telemetry::r4os_rust_r4dev_performance_input as *const ()
            as usize) as u64,
        display_state: (r4dev::display::r4os_rust_r4dev_display_state as *const () as usize) as u64,
        driver_module_info: (r4dev::display::r4os_rust_r4dev_driver_module_info as *const ()
            as usize) as u64,
        performance_instrumentation: (r4dev::telemetry::r4os_rust_r4dev_performance_instrumentation
            as *const () as usize) as u64,
        performance_lock: (r4dev::telemetry::r4os_rust_r4dev_performance_lock as *const () as usize)
            as u64,
        performance_stack: (r4dev::telemetry::r4os_rust_r4dev_performance_stack as *const ()
            as usize) as u64,
        stack_fault: (super::stack_fault::snapshot as *const () as usize) as u64,
        memory_kernel_heap_check: (HEAP_CHECK as *const () as usize) as u64,
        scheduler_kernel_check: (SCHEDULER_CHECK as *const () as usize) as u64,
        storage_block_queue_check: (BLOCK_QUEUE_CHECK as *const () as usize) as u64,
        storage_nvme_check: (NVME_CHECK as *const () as usize) as u64,
        usb_contract_check: (USB_CHECK as *const () as usize) as u64,
        smp_kernel_check: (SMP_CHECK as *const () as usize) as u64,
        performance_smp_tlb: (SMP_SNAPSHOT as *const () as usize) as u64,
        cpu_topology: (CPU_TOPOLOGY as *const () as usize) as u64,
        cpu_cache_topology: (CPU_CACHE as *const () as usize) as u64,
        cpu_lifecycle: (CPU_LIFECYCLE as *const () as usize) as u64,
        kernel_cpu_lifecycle_check: (CPU_LIFECYCLE_CHECK as *const () as usize) as u64,
        cpu_capabilities: (CPU_CAPABILITIES as *const () as usize) as u64,
        cpu_feature: (CPU_FEATURE as *const () as usize) as u64,
        cpu_register_read: (CPU_REGISTER_READ as *const () as usize) as u64,
        kernel_cpu_register_check: (CPU_REGISTER_CHECK as *const () as usize) as u64,
        cpu_power_info: (CPU_POWER_INFO as *const () as usize) as u64,
        cpu_power_capture: (CPU_POWER_CAPTURE as *const () as usize) as u64,
        cpu_performance: (CPU_PERFORMANCE as *const () as usize) as u64,
        cpu_cppc: (CPU_CPPC as *const () as usize) as u64,
        cpu_performance_hint: (CPU_HINT as *const () as usize) as u64,
        cpu_firmware_performance: (CPU_FIRMWARE as *const () as usize) as u64,
        cpu_idle_info: (CPU_IDLE as *const () as usize) as u64,
        cpu_machine_info: (CPU_MACHINE_INFO as *const () as usize) as u64,
        cpu_machine_bank: (CPU_MACHINE_BANK as *const () as usize) as u64,
        cpu_microcode_info: (CPU_MICROCODE_INFO as *const () as usize) as u64,
        cpu_pmu_info: (CPU_PMU_INFO as *const () as usize) as u64,
        cpu_pmu_measure: (CPU_PMU_MEASURE as *const () as usize) as u64,
        cpu_pmu_result: (CPU_PMU_RESULT as *const () as usize) as u64,
        cpu_acceleration_info: (CPU_ACCELERATION_INFO as *const () as usize) as u64,
        cpu_acceleration_work: (CPU_ACCELERATION_WORK as *const () as usize) as u64,
        fpu_observation: (FPU_OBSERVATION as *const () as usize) as u64,
        firmware_system_uuid: (FIRMWARE_SYSTEM_UUID as *const () as usize) as u64,
        ..Default::default()
    };
    DEV2.configure(value);
}
fn configure_generation2(group: u8) {
    match group {
        1 => configure_sys2(),
        2 => configure_desk2(),
        3 => configure_draw2(),
        4 => configure_net2(),
        5 => configure_audio2(),
        6 => configure_dev2(),
        _ => unreachable!(),
    }
}

pub(super) fn interface2(group: u32) -> usize {
    match group {
        1 => SYS2.get().addr(),
        2 => DESK2.get().addr(),
        3 => DRAW2.get().addr(),
        4 => NET2.get().addr(),
        5 => AUDIO2.get().addr(),
        6 => DEV2.get().addr(),
        _ => 0,
    }
}
pub(super) fn configure(group: u8) {
    match group {
        0 => {
            super::system::context::install_program_providers();
            super::system::copy::install_program_cancellation();
            unsafe {
                crate::storage::access_runtime::r4os_rust_storage_access_resolver(storage_owner)
            };
            super::r4draw::install_program_hooks();
            for group in 1..=6 {
                configure(group);
            }
        }
        1..=6 => configure_generation2(group),
        _ => unreachable!(),
    }
}
/// # Safety
/// Original single boot runtime entry; foreign owners retain their own setters.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_runtime_initialize(usable: u64) {
    if INITIALIZED.swap(true, Ordering::AcqRel) {
        return;
    }
    async_io::r4os_rust_program_async_initialize();
    configure(0);
    let _ = usable; // The original setRuntimeContext intentionally did no work.
    crate::boot::log_route::set_output_hook(
        Some(super::console_input::r4os_rust_program_console_print_hook),
        true,
    );
    crate::input::keyboard::r4os_rust_input_keyboard_input_hook(Some(
        super::console_input::r4os_rust_program_console_keyboard_hook,
    ));
    unsafe {
        if !(*registry_state::state()).reaper_started {
            let name = b"r4x-reaper";
            let request = task_api::CreateRequest {
                name: name.as_ptr(),
                name_len: name.len(),
                state: 1,
                role: 3,
                needs_fpu: true,
                smp_eligible: false,
                smp_r4x: false,
                owner_kind: 0,
                owner_context: ptr::null_mut(),
                entry: Some(reaper_entry),
            };
            let mut failure = 0;
            if !task_api::r4os_rust_task_create(&request, true, &mut failure).is_null() {
                (*registry_state::state()).reaper_started = true;
            }
        }
    }
}
unsafe extern "C" fn reaper_entry() {
    reaper::r4os_rust_program_reaper_main()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_runtime_configure(group: u8) {
    configure(group)
}

unsafe extern "C" fn storage_owner(output: *mut crate::storage::access_state::Owner) -> bool {
    let Some(owner) = (unsafe { super::async_io::stream_owner() }) else {
        return false;
    };
    unsafe {
        output.write(crate::storage::access_state::Owner {
            task: owner.task,
            task_generation: owner.task_generation,
            program: owner.program,
            program_generation: owner.program_generation,
        })
    };
    true
}
