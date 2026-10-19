//! Stable platform API tables and program runtime initialization. Canonical
//! Contract defaults supply every header and reserved slot. Typed readonly
//! native C callbacks carry the unchanged public slot signatures and records.
use super::{async_io, reaper, registry_state};
use crate::{contract as c, sched::task::task_api};
use core::{
    cell::UnsafeCell,
    mem::{self, MaybeUninit},
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

static SYS: Table<c::R4XStartR4Sys> = Table::new();
fn configure_sys() {
    let value = c::R4XStartR4Sys {
        write: super::platform_callbacks::sys_write as *const () as usize,
        putc: super::platform_callbacks::sys_putc as *const () as usize,
        sleep_ticks: crate::program::system::control::r4os_rust_sys_sleep as *const () as usize,
        ticks: crate::program::system::control::r4os_rust_sys_ticks as *const () as usize,
        env_get: crate::program::process_data::r4os_rust_program_data_env_api_get as *const ()
            as usize,
        dir_entry: crate::program::system::directory::r4os_rust_sys_dir_entry as *const () as usize,
        program_should_close: super::platform_callbacks::sys_program_should_close as *const ()
            as usize,
        program_class: super::platform_callbacks::sys_program_class as *const () as usize,
        program_instance: crate::program::info::r4os_rust_program_info_index as *const () as usize,
        service_status: super::platform_callbacks::sys_service_status as *const () as usize,
        service_open: super::platform_callbacks::sys_service_open as *const () as usize,
        service_close: super::platform_callbacks::sys_service_close as *const () as usize,
        service_call: super::platform_callbacks::sys_service_call as *const () as usize,
        service_endpoint_register: crate::program::service_api::r4os_rust_program_service_register
            as *const () as usize,
        service_endpoint_unregister: super::platform_callbacks::sys_service_endpoint_unregister
            as *const () as usize,
        service_endpoint_poll: super::platform_callbacks::sys_service_endpoint_poll as *const ()
            as usize,
        service_endpoint_recv: crate::program::service_api::r4os_rust_program_service_recv
            as *const () as usize,
        service_endpoint_reply: crate::program::service_api::r4os_rust_program_service_reply
            as *const () as usize,
        service_detail_by_name: crate::program::service_api::r4os_rust_program_service_detail_name
            as *const () as usize,
        service_start: super::platform_callbacks::sys_service_start as *const () as usize,
        service_stop: super::platform_callbacks::sys_service_stop as *const () as usize,
        service_restart: super::platform_callbacks::sys_service_restart as *const () as usize,
        service_install: crate::program::service_api::r4os_rust_program_service_install as *const ()
            as usize,
        service_remove: super::platform_callbacks::sys_service_remove as *const () as usize,
        vm_reserve: crate::program::vm::r4os_rust_program_vm_reserve as *const () as usize,
        vm_commit: crate::program::vm::r4os_rust_program_vm_commit as *const () as usize,
        vm_decommit: super::platform_callbacks::sys_vm_decommit as *const () as usize,
        vm_release: super::platform_callbacks::sys_vm_release as *const () as usize,
        vm_query: crate::program::vm::r4os_rust_program_vm_query as *const () as usize,
        thread_create: super::platform_callbacks::sys_thread_create as *const () as usize,
        thread_exit: super::platform_callbacks::sys_thread_exit as *const () as usize,
        thread_join: super::platform_callbacks::sys_thread_join as *const () as usize,
        thread_current: super::platform_callbacks::sys_thread_current as *const () as usize,
        thread_status: super::platform_callbacks::sys_thread_status as *const () as usize,
        io_file_read: super::platform_callbacks::sys_io_file_read as *const () as usize,
        io_file_read_at: super::platform_callbacks::sys_io_file_read_at as *const () as usize,
        io_file_write: super::platform_callbacks::sys_io_file_write as *const () as usize,
        io_file_append: super::platform_callbacks::sys_io_file_append as *const () as usize,
        io_file_stream_begin: super::platform_callbacks::sys_io_file_stream_begin as *const ()
            as usize,
        io_file_stream_write: super::platform_callbacks::sys_io_file_stream_write as *const ()
            as usize,
        io_file_stream_finish: super::platform_callbacks::sys_io_file_stream_finish as *const ()
            as usize,
        io_file_stream_abort: super::platform_callbacks::sys_io_file_stream_abort as *const ()
            as usize,
        io_service_call: super::platform_callbacks::sys_io_service_call as *const () as usize,
        io_status: super::platform_callbacks::sys_io_status as *const () as usize,
        io_wait: super::platform_callbacks::sys_io_wait as *const () as usize,
        io_close: super::platform_callbacks::sys_io_close as *const () as usize,
        task_yield: crate::program::system::control::r4os_rust_sys_yield as *const () as usize,
        system_halt: super::platform_callbacks::sys_system_halt as *const () as usize,
        system_reboot: crate::program::system::control::r4os_rust_sys_reboot as *const () as usize,
        system_poweroff: super::platform_callbacks::sys_system_poweroff as *const () as usize,
        time_state: crate::program::system::control::r4os_rust_sys_time_state as *const () as usize,
        time_set_state: crate::program::system::control::r4os_rust_sys_time_set as *const ()
            as usize,
        time_seconds_since_midnight: crate::program::system::control::r4os_rust_sys_seconds
            as *const () as usize,
        dir_list: crate::program::system::directory::r4os_rust_sys_dir_list as *const () as usize,
        dir_create: crate::program::system::namespace::r4os_rust_sys_dir_create as *const ()
            as usize,
        dir_delete: crate::program::system::namespace::r4os_rust_sys_dir_delete as *const ()
            as usize,
        drive_info: crate::program::system::directory::r4os_rust_sys_drive_info as *const ()
            as usize,
        file_info: crate::program::system::directory::r4os_rust_sys_file_info as *const () as usize,
        file_delete: crate::program::system::namespace::r4os_rust_sys_file_delete as *const ()
            as usize,
        file_rename: crate::program::system::namespace::r4os_rust_sys_file_rename as *const ()
            as usize,
        file_copy: crate::program::system::copy::r4os_rust_sys_file_copy as *const () as usize,
        file_move: crate::program::system::copy::r4os_rust_sys_file_move as *const () as usize,
        file_read: super::platform_callbacks::sys_file_read as *const () as usize,
        file_write: super::platform_callbacks::sys_file_write as *const () as usize,
        file_read_at: super::platform_callbacks::sys_file_read_at as *const () as usize,
        file_append: super::platform_callbacks::sys_file_append as *const () as usize,
        file_stream_begin: super::platform_callbacks::sys_file_stream_begin as *const () as usize,
        file_stream_write: super::platform_callbacks::sys_file_stream_write as *const () as usize,
        file_stream_finish: super::platform_callbacks::sys_file_stream_finish as *const () as usize,
        file_stream_abort: super::platform_callbacks::sys_file_stream_abort as *const () as usize,
        env_set: crate::program::process_data::r4os_rust_program_data_env_api_set as *const ()
            as usize,
        registry_key_info: crate::program::system::registry_api::r4os_rust_sys_registry_key_info
            as *const () as usize,
        registry_enum_key: crate::program::system::registry_api::r4os_rust_sys_registry_enum_key
            as *const () as usize,
        registry_enum_value: crate::program::system::registry_api::r4os_rust_sys_registry_enum_value
            as *const () as usize,
        registry_get_value: crate::program::system::registry_api::r4os_rust_sys_registry_get
            as *const () as usize,
        registry_set_value: crate::program::system::registry_api::r4os_rust_sys_registry_set
            as *const () as usize,
        registry_delete_value: crate::program::system::registry_api::r4os_rust_sys_registry_delete
            as *const () as usize,
        service_info: crate::program::service_api::r4os_rust_program_service_info as *const ()
            as usize,
        service_detail: crate::program::service_api::r4os_rust_program_service_detail as *const ()
            as usize,
        service_set_start_mode: super::platform_callbacks::sys_service_set_start_mode as *const ()
            as usize,
        service_endpoint_wait: super::platform_callbacks::sys_service_endpoint_wait as *const ()
            as usize,
        program_run: super::platform_callbacks::sys_program_run as *const () as usize,
        program_launch: super::platform_callbacks::sys_program_launch as *const () as usize,
        program_spawn: super::platform_callbacks::sys_program_spawn as *const () as usize,
        program_kill: super::platform_callbacks::sys_program_kill as *const () as usize,
        program_status: crate::program::info::r4os_rust_program_info_status as *const () as usize,
        program_request_close: super::platform_callbacks::sys_program_request_close as *const ()
            as usize,
        program_reap_instance: super::platform_callbacks::sys_program_reap_instance as *const ()
            as usize,
        boot_log_info: crate::program::system::control::r4os_rust_sys_bootlog_info as *const ()
            as usize,
        boot_log_read: crate::program::system::control::r4os_rust_sys_bootlog_read as *const ()
            as usize,
        program_spawn_handle: super::platform_callbacks::sys_program_spawn_handle as *const ()
            as usize,
        program_open_handle: crate::program::process::r4os_rust_program_process_open as *const ()
            as usize,
        program_handle_status: crate::program::info::r4os_rust_program_info_handle as *const ()
            as usize,
        program_handle_request_close: crate::program::process::r4os_rust_program_process_close
            as *const () as usize,
        program_handle_kill: crate::program::process::r4os_rust_program_process_kill as *const ()
            as usize,
        program_handle_wait: crate::program::process::r4os_rust_program_process_wait as *const ()
            as usize,
        program_handle_reap: crate::program::process::r4os_rust_program_process_reap as *const ()
            as usize,
        program_completion_read: crate::program::program_api::r4os_rust_program_api_completion_read
            as *const () as usize,
        program_inventory_begin:
            crate::program::execution_inventory::r4os_rust_program_execution_begin as *const ()
                as usize,
        program_inventory_programs: crate::program::inventory::r4os_rust_program_inventory_programs
            as *const () as usize,
        program_inventory_tasks:
            crate::program::execution_inventory::r4os_rust_program_execution_tasks as *const ()
                as usize,
        program_inventory_threads:
            crate::program::execution_inventory::r4os_rust_program_execution_threads as *const ()
                as usize,
        thread_create_handle: super::platform_callbacks::sys_thread_create_handle as *const ()
            as usize,
        thread_handle_join: super::platform_callbacks::sys_thread_handle_join as *const () as usize,
        thread_handle_status: super::platform_callbacks::sys_thread_handle_status as *const ()
            as usize,
        file_replace_atomic: super::platform_callbacks::sys_file_replace_atomic as *const ()
            as usize,
        file_delete_if_match: crate::program::system::namespace::r4os_rust_sys_file_delete_match
            as *const () as usize,
        file_update_atomic_checked: crate::program::system::atomic::r4os_rust_sys_update_checked
            as *const () as usize,
        path_names_equal_collated: crate::program::system::namespace::r4os_rust_sys_names_equal
            as *const () as usize,
        file_update_cleanup_checked: crate::program::system::atomic::r4os_rust_sys_update_cleanup
            as *const () as usize,
        file_stream_declare_publish: crate::program::system::atomic::r4os_rust_sys_stream_declare
            as *const () as usize,
        module_resource_stat: crate::program::system::resources::r4os_rust_sys_resource_stat
            as *const () as usize,
        module_resource_read: crate::program::system::resources::r4os_rust_sys_resource_read
            as *const () as usize,
        program_module_path: crate::program::program_api::r4os_rust_program_api_module_path
            as *const () as usize,
        program_module_running: crate::program::system::io::r4os_rust_sys_module_running
            as *const () as usize,
        monotonic_clock: crate::program::system::control::r4os_rust_sys_monotonic as *const ()
            as usize,
        boot_ready: super::platform_callbacks::sys_boot_ready as *const () as usize,
        registry_snapshot_begin:
            crate::program::system::registry_api::r4os_rust_sys_registry_snapshot_begin as *const ()
                as usize,
        registry_snapshot_page:
            crate::program::system::registry_api::r4os_rust_sys_registry_snapshot_page as *const ()
                as usize,
        registry_batch_mutate: crate::program::system::registry_batch::r4os_rust_sys_registry_batch
            as *const () as usize,
        io_file_write_at: super::platform_callbacks::sys_io_file_write_at as *const () as usize,
        io_file_info: super::platform_callbacks::sys_io_file_info as *const () as usize,
        io_file_lock: super::platform_callbacks::sys_io_file_lock as *const () as usize,
        storage_inventory: super::platform_callbacks::sys_storage_inventory as *const () as usize,
        storage_device: super::platform_callbacks::sys_storage_device as *const () as usize,
        storage_partition: super::platform_callbacks::sys_storage_partition as *const () as usize,
        storage_volume: super::platform_callbacks::sys_storage_volume as *const () as usize,
        storage_claim_begin: super::platform_callbacks::sys_storage_claim_begin as *const ()
            as usize,
        storage_claim_end: super::platform_callbacks::sys_storage_claim_end as *const () as usize,
        storage_read: super::platform_callbacks::sys_storage_read as *const () as usize,
        storage_claim_read: super::platform_callbacks::sys_storage_claim_read as *const () as usize,
        storage_claim_write: super::platform_callbacks::sys_storage_claim_write as *const ()
            as usize,
        storage_claim_flush: super::platform_callbacks::sys_storage_claim_flush as *const ()
            as usize,
        storage_rescan: super::platform_callbacks::sys_storage_rescan as *const () as usize,
        storage_mount: super::platform_callbacks::sys_storage_mount as *const () as usize,
        storage_unmount: super::platform_callbacks::sys_storage_unmount as *const () as usize,
        storage_use_begin: crate::program::system::stream_api::r4os_rust_sys_storage_use
            as *const () as usize,
        storage_use_end: super::platform_callbacks::sys_storage_use_end as *const () as usize,
        directory_change_begin:
            crate::program::system::directory::r4os_rust_sys_directory_change_begin as *const ()
                as usize,
        directory_change_poll:
            crate::program::system::directory::r4os_rust_sys_directory_change_poll as *const ()
                as usize,
        file_copy_buffered: crate::program::system::copy::r4os_rust_sys_file_copy_buffered
            as *const () as usize,
        notification_create: super::platform_callbacks::sys_notification_create as *const ()
            as usize,
        notification_query: super::platform_callbacks::sys_notification_query as *const () as usize,
        notification_notify: super::platform_callbacks::sys_notification_notify as *const ()
            as usize,
        notification_wait: super::platform_callbacks::sys_notification_wait as *const () as usize,
        notification_close: super::platform_callbacks::sys_notification_close as *const () as usize,
        program_local_get: super::platform_callbacks::sys_program_local_get as *const () as usize,
        program_local_publish: super::platform_callbacks::sys_program_local_publish as *const ()
            as usize,
        thread_current_handle:
            crate::program::thread_runtime::r4os_rust_program_thread_current_handle as *const ()
                as usize,
        cpu_capacity: crate::program::system::control::r4os_rust_sys_cpu_capacity as *const ()
            as usize,
        program_exit: crate::program::thread_runtime::r4os_rust_program_thread_program_exit
            as *const () as usize,
        platform_input_snapshot: crate::input::platform_input::r4os_rust_input_platform_snapshot
            as *const () as usize,
        directory_next: crate::program::system::directory::r4os_rust_sys_directory_next as *const ()
            as usize,
        thread_placement_get: super::thread_placement::get as *const () as usize,
        thread_placement_set: super::thread_placement::set as *const () as usize,
        system_power_transition: crate::program::system::control::power_transition as *const ()
            as usize,
        ..Default::default()
    };
    SYS.configure(value);
}
const _: () = {
    assert!(mem::size_of::<c::R4XStartR4Sys>() == 1288);
    assert!(mem::offset_of!(c::R4XStartR4Sys, magic) == 0);
    assert!(mem::offset_of!(c::R4XStartR4Sys, abi_version) == 4);
    assert!(mem::offset_of!(c::R4XStartR4Sys, size) == 8);
    assert!(mem::offset_of!(c::R4XStartR4Sys, flags) == 12);
    assert!(mem::offset_of!(c::R4XStartR4Sys, write) == 16);
    assert!(mem::offset_of!(c::R4XStartR4Sys, putc) == 24);
    assert!(mem::offset_of!(c::R4XStartR4Sys, sleep_ticks) == 32);
    assert!(mem::offset_of!(c::R4XStartR4Sys, ticks) == 40);
    assert!(mem::offset_of!(c::R4XStartR4Sys, env_get) == 48);
    assert!(mem::offset_of!(c::R4XStartR4Sys, dir_entry) == 56);
    assert!(mem::offset_of!(c::R4XStartR4Sys, program_should_close) == 64);
    assert!(mem::offset_of!(c::R4XStartR4Sys, program_class) == 72);
    assert!(mem::offset_of!(c::R4XStartR4Sys, program_instance) == 80);
    assert!(mem::offset_of!(c::R4XStartR4Sys, service_status) == 88);
    assert!(mem::offset_of!(c::R4XStartR4Sys, service_open) == 96);
    assert!(mem::offset_of!(c::R4XStartR4Sys, service_close) == 104);
    assert!(mem::offset_of!(c::R4XStartR4Sys, service_call) == 112);
    assert!(mem::offset_of!(c::R4XStartR4Sys, service_endpoint_register) == 120);
    assert!(mem::offset_of!(c::R4XStartR4Sys, service_endpoint_unregister) == 128);
    assert!(mem::offset_of!(c::R4XStartR4Sys, service_endpoint_poll) == 136);
    assert!(mem::offset_of!(c::R4XStartR4Sys, service_endpoint_recv) == 144);
    assert!(mem::offset_of!(c::R4XStartR4Sys, service_endpoint_reply) == 152);
    assert!(mem::offset_of!(c::R4XStartR4Sys, service_detail_by_name) == 160);
    assert!(mem::offset_of!(c::R4XStartR4Sys, service_start) == 168);
    assert!(mem::offset_of!(c::R4XStartR4Sys, service_stop) == 176);
    assert!(mem::offset_of!(c::R4XStartR4Sys, service_restart) == 184);
    assert!(mem::offset_of!(c::R4XStartR4Sys, service_install) == 192);
    assert!(mem::offset_of!(c::R4XStartR4Sys, service_remove) == 200);
    assert!(mem::offset_of!(c::R4XStartR4Sys, reserved0) == 208);
    assert!(mem::offset_of!(c::R4XStartR4Sys, reserved1) == 216);
    assert!(mem::offset_of!(c::R4XStartR4Sys, vm_reserve) == 224);
    assert!(mem::offset_of!(c::R4XStartR4Sys, vm_commit) == 232);
    assert!(mem::offset_of!(c::R4XStartR4Sys, vm_decommit) == 240);
    assert!(mem::offset_of!(c::R4XStartR4Sys, vm_release) == 248);
    assert!(mem::offset_of!(c::R4XStartR4Sys, vm_query) == 256);
    assert!(mem::offset_of!(c::R4XStartR4Sys, thread_create) == 264);
    assert!(mem::offset_of!(c::R4XStartR4Sys, thread_exit) == 272);
    assert!(mem::offset_of!(c::R4XStartR4Sys, thread_join) == 280);
    assert!(mem::offset_of!(c::R4XStartR4Sys, thread_current) == 288);
    assert!(mem::offset_of!(c::R4XStartR4Sys, thread_status) == 296);
    assert!(mem::offset_of!(c::R4XStartR4Sys, io_file_read) == 304);
    assert!(mem::offset_of!(c::R4XStartR4Sys, io_file_read_at) == 312);
    assert!(mem::offset_of!(c::R4XStartR4Sys, io_file_write) == 320);
    assert!(mem::offset_of!(c::R4XStartR4Sys, io_file_append) == 328);
    assert!(mem::offset_of!(c::R4XStartR4Sys, io_file_stream_begin) == 336);
    assert!(mem::offset_of!(c::R4XStartR4Sys, io_file_stream_write) == 344);
    assert!(mem::offset_of!(c::R4XStartR4Sys, io_file_stream_finish) == 352);
    assert!(mem::offset_of!(c::R4XStartR4Sys, io_file_stream_abort) == 360);
    assert!(mem::offset_of!(c::R4XStartR4Sys, io_service_call) == 368);
    assert!(mem::offset_of!(c::R4XStartR4Sys, io_status) == 376);
    assert!(mem::offset_of!(c::R4XStartR4Sys, io_wait) == 384);
    assert!(mem::offset_of!(c::R4XStartR4Sys, io_close) == 392);
    assert!(mem::offset_of!(c::R4XStartR4Sys, task_yield) == 400);
    assert!(mem::offset_of!(c::R4XStartR4Sys, reserved_shell_run) == 408);
    assert!(mem::offset_of!(c::R4XStartR4Sys, system_halt) == 416);
    assert!(mem::offset_of!(c::R4XStartR4Sys, system_reboot) == 424);
    assert!(mem::offset_of!(c::R4XStartR4Sys, system_poweroff) == 432);
    assert!(mem::offset_of!(c::R4XStartR4Sys, system_power_transition) == 1280);
    assert!(mem::offset_of!(c::R4XStartR4Sys, time_state) == 440);
    assert!(mem::offset_of!(c::R4XStartR4Sys, time_set_state) == 448);
    assert!(mem::offset_of!(c::R4XStartR4Sys, time_seconds_since_midnight) == 456);
    assert!(mem::offset_of!(c::R4XStartR4Sys, dir_list) == 464);
    assert!(mem::offset_of!(c::R4XStartR4Sys, dir_create) == 472);
    assert!(mem::offset_of!(c::R4XStartR4Sys, dir_delete) == 480);
    assert!(mem::offset_of!(c::R4XStartR4Sys, drive_info) == 488);
    assert!(mem::offset_of!(c::R4XStartR4Sys, file_info) == 496);
    assert!(mem::offset_of!(c::R4XStartR4Sys, file_delete) == 504);
    assert!(mem::offset_of!(c::R4XStartR4Sys, file_rename) == 512);
    assert!(mem::offset_of!(c::R4XStartR4Sys, file_copy) == 520);
    assert!(mem::offset_of!(c::R4XStartR4Sys, file_move) == 528);
    assert!(mem::offset_of!(c::R4XStartR4Sys, file_read) == 536);
    assert!(mem::offset_of!(c::R4XStartR4Sys, file_write) == 544);
    assert!(mem::offset_of!(c::R4XStartR4Sys, file_read_at) == 552);
    assert!(mem::offset_of!(c::R4XStartR4Sys, file_append) == 560);
    assert!(mem::offset_of!(c::R4XStartR4Sys, file_stream_begin) == 568);
    assert!(mem::offset_of!(c::R4XStartR4Sys, file_stream_write) == 576);
    assert!(mem::offset_of!(c::R4XStartR4Sys, file_stream_finish) == 584);
    assert!(mem::offset_of!(c::R4XStartR4Sys, file_stream_abort) == 592);
    assert!(mem::offset_of!(c::R4XStartR4Sys, env_set) == 600);
    assert!(mem::offset_of!(c::R4XStartR4Sys, registry_key_info) == 608);
    assert!(mem::offset_of!(c::R4XStartR4Sys, registry_enum_key) == 616);
    assert!(mem::offset_of!(c::R4XStartR4Sys, registry_enum_value) == 624);
    assert!(mem::offset_of!(c::R4XStartR4Sys, registry_get_value) == 632);
    assert!(mem::offset_of!(c::R4XStartR4Sys, registry_set_value) == 640);
    assert!(mem::offset_of!(c::R4XStartR4Sys, registry_delete_value) == 648);
    assert!(mem::offset_of!(c::R4XStartR4Sys, service_info) == 656);
    assert!(mem::offset_of!(c::R4XStartR4Sys, service_detail) == 664);
    assert!(mem::offset_of!(c::R4XStartR4Sys, service_set_start_mode) == 672);
    assert!(mem::offset_of!(c::R4XStartR4Sys, service_endpoint_wait) == 680);
    assert!(mem::offset_of!(c::R4XStartR4Sys, program_run) == 688);
    assert!(mem::offset_of!(c::R4XStartR4Sys, program_launch) == 696);
    assert!(mem::offset_of!(c::R4XStartR4Sys, program_spawn) == 704);
    assert!(mem::offset_of!(c::R4XStartR4Sys, program_kill) == 712);
    assert!(mem::offset_of!(c::R4XStartR4Sys, program_status) == 720);
    assert!(mem::offset_of!(c::R4XStartR4Sys, program_request_close) == 728);
    assert!(mem::offset_of!(c::R4XStartR4Sys, program_reap_instance) == 736);
    assert!(mem::offset_of!(c::R4XStartR4Sys, boot_log_info) == 744);
    assert!(mem::offset_of!(c::R4XStartR4Sys, boot_log_read) == 752);
    assert!(mem::offset_of!(c::R4XStartR4Sys, program_spawn_handle) == 760);
    assert!(mem::offset_of!(c::R4XStartR4Sys, program_open_handle) == 768);
    assert!(mem::offset_of!(c::R4XStartR4Sys, program_handle_status) == 776);
    assert!(mem::offset_of!(c::R4XStartR4Sys, program_handle_request_close) == 784);
    assert!(mem::offset_of!(c::R4XStartR4Sys, program_handle_kill) == 792);
    assert!(mem::offset_of!(c::R4XStartR4Sys, program_handle_wait) == 800);
    assert!(mem::offset_of!(c::R4XStartR4Sys, program_handle_reap) == 808);
    assert!(mem::offset_of!(c::R4XStartR4Sys, program_completion_read) == 816);
    assert!(mem::offset_of!(c::R4XStartR4Sys, program_inventory_begin) == 824);
    assert!(mem::offset_of!(c::R4XStartR4Sys, program_inventory_programs) == 832);
    assert!(mem::offset_of!(c::R4XStartR4Sys, program_inventory_tasks) == 840);
    assert!(mem::offset_of!(c::R4XStartR4Sys, program_inventory_threads) == 848);
    assert!(mem::offset_of!(c::R4XStartR4Sys, thread_create_handle) == 856);
    assert!(mem::offset_of!(c::R4XStartR4Sys, thread_handle_join) == 864);
    assert!(mem::offset_of!(c::R4XStartR4Sys, thread_handle_status) == 872);
    assert!(mem::offset_of!(c::R4XStartR4Sys, file_replace_atomic) == 880);
    assert!(mem::offset_of!(c::R4XStartR4Sys, file_delete_if_match) == 888);
    assert!(mem::offset_of!(c::R4XStartR4Sys, file_update_atomic_checked) == 896);
    assert!(mem::offset_of!(c::R4XStartR4Sys, path_names_equal_collated) == 904);
    assert!(mem::offset_of!(c::R4XStartR4Sys, file_update_cleanup_checked) == 912);
    assert!(mem::offset_of!(c::R4XStartR4Sys, file_stream_declare_publish) == 920);
    assert!(mem::offset_of!(c::R4XStartR4Sys, module_resource_stat) == 928);
    assert!(mem::offset_of!(c::R4XStartR4Sys, module_resource_read) == 936);
    assert!(mem::offset_of!(c::R4XStartR4Sys, program_module_path) == 944);
    assert!(mem::offset_of!(c::R4XStartR4Sys, program_module_running) == 952);
    assert!(mem::offset_of!(c::R4XStartR4Sys, monotonic_clock) == 960);
    assert!(mem::offset_of!(c::R4XStartR4Sys, boot_ready) == 968);
    assert!(mem::offset_of!(c::R4XStartR4Sys, registry_snapshot_begin) == 976);
    assert!(mem::offset_of!(c::R4XStartR4Sys, registry_snapshot_page) == 984);
    assert!(mem::offset_of!(c::R4XStartR4Sys, registry_batch_mutate) == 992);
    assert!(mem::offset_of!(c::R4XStartR4Sys, io_file_write_at) == 1000);
    assert!(mem::offset_of!(c::R4XStartR4Sys, io_file_info) == 1008);
    assert!(mem::offset_of!(c::R4XStartR4Sys, io_file_lock) == 1016);
    assert!(mem::offset_of!(c::R4XStartR4Sys, storage_inventory) == 1024);
    assert!(mem::offset_of!(c::R4XStartR4Sys, storage_device) == 1032);
    assert!(mem::offset_of!(c::R4XStartR4Sys, storage_partition) == 1040);
    assert!(mem::offset_of!(c::R4XStartR4Sys, storage_volume) == 1048);
    assert!(mem::offset_of!(c::R4XStartR4Sys, storage_claim_begin) == 1056);
    assert!(mem::offset_of!(c::R4XStartR4Sys, storage_claim_end) == 1064);
    assert!(mem::offset_of!(c::R4XStartR4Sys, storage_read) == 1072);
    assert!(mem::offset_of!(c::R4XStartR4Sys, storage_claim_read) == 1080);
    assert!(mem::offset_of!(c::R4XStartR4Sys, storage_claim_write) == 1088);
    assert!(mem::offset_of!(c::R4XStartR4Sys, storage_claim_flush) == 1096);
    assert!(mem::offset_of!(c::R4XStartR4Sys, storage_rescan) == 1104);
    assert!(mem::offset_of!(c::R4XStartR4Sys, storage_mount) == 1112);
    assert!(mem::offset_of!(c::R4XStartR4Sys, storage_unmount) == 1120);
    assert!(mem::offset_of!(c::R4XStartR4Sys, storage_use_begin) == 1128);
    assert!(mem::offset_of!(c::R4XStartR4Sys, storage_use_end) == 1136);
    assert!(mem::offset_of!(c::R4XStartR4Sys, directory_change_begin) == 1144);
    assert!(mem::offset_of!(c::R4XStartR4Sys, directory_change_poll) == 1152);
    assert!(mem::offset_of!(c::R4XStartR4Sys, file_copy_buffered) == 1160);
    assert!(mem::offset_of!(c::R4XStartR4Sys, notification_create) == 1168);
    assert!(mem::offset_of!(c::R4XStartR4Sys, notification_query) == 1176);
    assert!(mem::offset_of!(c::R4XStartR4Sys, notification_notify) == 1184);
    assert!(mem::offset_of!(c::R4XStartR4Sys, notification_wait) == 1192);
    assert!(mem::offset_of!(c::R4XStartR4Sys, notification_close) == 1200);
    assert!(mem::offset_of!(c::R4XStartR4Sys, program_local_get) == 1208);
    assert!(mem::offset_of!(c::R4XStartR4Sys, program_local_publish) == 1216);
    assert!(mem::offset_of!(c::R4XStartR4Sys, thread_current_handle) == 1224);
    assert!(mem::offset_of!(c::R4XStartR4Sys, cpu_capacity) == 1232);
    assert!(mem::offset_of!(c::R4XStartR4Sys, program_exit) == 1240);
    assert!(mem::offset_of!(c::R4XStartR4Sys, platform_input_snapshot) == 1248);
    assert!(mem::offset_of!(c::R4XStartR4Sys, directory_next) == 1256);
};
static DESK: Table<c::R4XStartR4Desk> = Table::new();
fn configure_desk() {
    let value = c::R4XStartR4Desk {
        read_key: super::platform_callbacks::desk_read_key as *const () as usize,
        mouse_state: super::platform_callbacks::desk_mouse_state as *const () as usize,
        mouse_show: super::platform_callbacks::desk_mouse_show as *const () as usize,
        mouse_hide: super::platform_callbacks::desk_mouse_hide as *const () as usize,
        keyboard_layout_current: super::platform_callbacks::desk_keyboard_layout_current
            as *const () as usize,
        keyboard_layout_at: super::platform_callbacks::desk_keyboard_layout_at as *const ()
            as usize,
        keyboard_layout_set: super::platform_callbacks::desk_keyboard_layout_set as *const ()
            as usize,
        program_set_window: super::platform_callbacks::desk_program_set_window as *const ()
            as usize,
        program_set_console_host: crate::program::program_api::r4os_rust_program_api_console_host
            as *const () as usize,
        program_request_host_launch: crate::program::program_api::r4os_rust_program_api_host_request
            as *const () as usize,
        program_take_host_launch: crate::program::program_api::r4os_rust_program_api_host_take
            as *const () as usize,
        program_window_id: super::platform_callbacks::desk_program_window_id as *const () as usize,
        gui_window_info: crate::program::gui_events::r4os_rust_program_gui_window_info as *const ()
            as usize,
        gui_set_window_info: crate::program::gui_events::r4os_rust_program_gui_window_set
            as *const () as usize,
        gui_poll_event: crate::program::gui_events::r4os_rust_program_gui_event_poll as *const ()
            as usize,
        gui_push_event: crate::program::gui_events::r4os_rust_program_gui_event_push as *const ()
            as usize,
        gui_set_text: super::platform_callbacks::desk_gui_set_text as *const () as usize,
        gui_text: super::platform_callbacks::desk_gui_text as *const () as usize,
        gui_revision: crate::program::gui_events::r4os_rust_program_gui_revision_read as *const ()
            as usize,
        gui_command: crate::program::gui_api::r4os_rust_program_gui_api_command as *const ()
            as usize,
        gui_set_title: super::platform_callbacks::desk_gui_set_title as *const () as usize,
        gui_title: super::platform_callbacks::desk_gui_title as *const () as usize,
        gui_set_min_size: super::platform_callbacks::desk_gui_set_min_size as *const () as usize,
        gui_min_size: crate::program::gui_events::r4os_rust_program_gui_min_size as *const ()
            as usize,
        console_output: crate::program::console::r4os_rust_program_console_api_output as *const ()
            as usize,
        console_revision: crate::program::console::r4os_rust_program_console_api_revision
            as *const () as usize,
        console_state: crate::program::console::r4os_rust_program_console_api_state as *const ()
            as usize,
        console_set_metrics: crate::program::console::r4os_rust_program_console_api_metrics
            as *const () as usize,
        console_push_key: super::platform_callbacks::desk_console_push_key as *const () as usize,
        console_write: super::platform_callbacks::desk_console_write as *const () as usize,
        console_read: crate::program::console_input::r4os_rust_program_console_input_api_read
            as *const () as usize,
        clipboard_write: super::platform_callbacks::desk_clipboard_write as *const () as usize,
        clipboard_read: super::platform_callbacks::desk_clipboard_read as *const () as usize,
        clipboard_revision: super::platform_callbacks::desk_clipboard_revision as *const ()
            as usize,
        clipboard_info: super::platform_callbacks::desk_clipboard_info as *const () as usize,
        clipboard_clear: super::platform_callbacks::desk_clipboard_clear as *const () as usize,
        read_key_codepoint: super::platform_callbacks::desk_read_key_codepoint as *const ()
            as usize,
        remote_frame_info: super::platform_callbacks::desk_remote_frame_info as *const () as usize,
        remote_frame_read: super::platform_callbacks::desk_remote_frame_read as *const () as usize,
        remote_frame_wait: super::platform_callbacks::desk_remote_frame_wait as *const () as usize,
        remote_frame_publish:
            crate::program::domain_api::r4os_rust_program_domain_remote_frame_publish as *const ()
                as usize,
        remote_input_push: super::platform_callbacks::desk_remote_input_push as *const () as usize,
        remote_input_poll: super::platform_callbacks::desk_remote_input_poll as *const () as usize,
        remote_input_status: super::platform_callbacks::desk_remote_input_status as *const ()
            as usize,
        desktop_activity_wait: super::platform_callbacks::desk_desktop_activity_wait as *const ()
            as usize,
        remote_frame_map: super::platform_callbacks::desk_remote_frame_map as *const () as usize,
        program_current_console_host: super::platform_callbacks::desk_program_current_console_host
            as *const () as usize,
        program_request_desktop: super::platform_callbacks::desk_program_request_desktop
            as *const () as usize,
        program_spawn_with_console_host:
            super::platform_callbacks::desk_program_spawn_with_console_host as *const () as usize,
        program_spawn_with_console_host_handle:
            super::platform_callbacks::desk_program_spawn_with_console_host_handle as *const ()
                as usize,
        program_set_window_handle: super::platform_callbacks::desk_program_set_window_handle
            as *const () as usize,
        console_push_input: super::platform_callbacks::desk_console_push_input as *const ()
            as usize,
        remote_frame_acquire:
            crate::program::domain_api::r4os_rust_program_domain_remote_frame_acquire as *const ()
                as usize,
        remote_frame_release:
            crate::program::domain_api::r4os_rust_program_domain_remote_frame_release as *const ()
                as usize,
        remote_frame_consumers: super::platform_callbacks::desk_remote_frame_consumers as *const ()
            as usize,
        remote_frame_publish_regions:
            crate::program::domain_api::r4os_rust_program_domain_remote_frame_publish_regions
                as *const () as usize,
        console_input_wait: crate::program::console_input::r4os_rust_program_console_input_wait
            as *const () as usize,
        physical_key_poll: super::platform_callbacks::desk_physical_key_poll as *const () as usize,
        mouse_motion: super::platform_callbacks::desk_mouse_motion as *const () as usize,
        remote_frame_snapshot_acquire:
            crate::program::domain_api::r4os_rust_program_domain_remote_frame_snapshot_acquire
                as *const () as usize,
        remote_frame_snapshot_release:
            crate::program::domain_api::r4os_rust_program_domain_remote_frame_snapshot_release
                as *const () as usize,
        remote_frame_source_reset:
            crate::program::domain_api::r4os_rust_program_domain_remote_frame_source_reset
                as *const () as usize,
        remote_frame_capture_stats: super::platform_callbacks::desk_remote_frame_capture_stats
            as *const () as usize,
        desktop_activity_notify: super::platform_callbacks::desk_desktop_activity_notify
            as *const () as usize,
        ..Default::default()
    };
    DESK.configure(value);
}
const _: () = {
    assert!(mem::size_of::<c::R4XStartR4Desk>() == 536);
    assert!(mem::offset_of!(c::R4XStartR4Desk, magic) == 0);
    assert!(mem::offset_of!(c::R4XStartR4Desk, abi_version) == 4);
    assert!(mem::offset_of!(c::R4XStartR4Desk, size) == 8);
    assert!(mem::offset_of!(c::R4XStartR4Desk, flags) == 12);
    assert!(mem::offset_of!(c::R4XStartR4Desk, read_key) == 16);
    assert!(mem::offset_of!(c::R4XStartR4Desk, mouse_state) == 24);
    assert!(mem::offset_of!(c::R4XStartR4Desk, mouse_show) == 32);
    assert!(mem::offset_of!(c::R4XStartR4Desk, mouse_hide) == 40);
    assert!(mem::offset_of!(c::R4XStartR4Desk, keyboard_layout_current) == 48);
    assert!(mem::offset_of!(c::R4XStartR4Desk, keyboard_layout_at) == 56);
    assert!(mem::offset_of!(c::R4XStartR4Desk, keyboard_layout_set) == 64);
    assert!(mem::offset_of!(c::R4XStartR4Desk, program_set_window) == 72);
    assert!(mem::offset_of!(c::R4XStartR4Desk, program_set_console_host) == 80);
    assert!(mem::offset_of!(c::R4XStartR4Desk, program_request_host_launch) == 88);
    assert!(mem::offset_of!(c::R4XStartR4Desk, program_take_host_launch) == 96);
    assert!(mem::offset_of!(c::R4XStartR4Desk, program_window_id) == 104);
    assert!(mem::offset_of!(c::R4XStartR4Desk, gui_window_info) == 112);
    assert!(mem::offset_of!(c::R4XStartR4Desk, gui_set_window_info) == 120);
    assert!(mem::offset_of!(c::R4XStartR4Desk, gui_poll_event) == 128);
    assert!(mem::offset_of!(c::R4XStartR4Desk, gui_push_event) == 136);
    assert!(mem::offset_of!(c::R4XStartR4Desk, gui_set_text) == 144);
    assert!(mem::offset_of!(c::R4XStartR4Desk, gui_text) == 152);
    assert!(mem::offset_of!(c::R4XStartR4Desk, gui_revision) == 160);
    assert!(mem::offset_of!(c::R4XStartR4Desk, gui_command) == 168);
    assert!(mem::offset_of!(c::R4XStartR4Desk, gui_set_title) == 176);
    assert!(mem::offset_of!(c::R4XStartR4Desk, gui_title) == 184);
    assert!(mem::offset_of!(c::R4XStartR4Desk, gui_set_min_size) == 192);
    assert!(mem::offset_of!(c::R4XStartR4Desk, gui_min_size) == 200);
    assert!(mem::offset_of!(c::R4XStartR4Desk, console_output) == 208);
    assert!(mem::offset_of!(c::R4XStartR4Desk, console_revision) == 216);
    assert!(mem::offset_of!(c::R4XStartR4Desk, console_state) == 224);
    assert!(mem::offset_of!(c::R4XStartR4Desk, console_set_metrics) == 232);
    assert!(mem::offset_of!(c::R4XStartR4Desk, console_push_key) == 240);
    assert!(mem::offset_of!(c::R4XStartR4Desk, console_write) == 248);
    assert!(mem::offset_of!(c::R4XStartR4Desk, console_read) == 256);
    assert!(mem::offset_of!(c::R4XStartR4Desk, clipboard_write) == 264);
    assert!(mem::offset_of!(c::R4XStartR4Desk, clipboard_read) == 272);
    assert!(mem::offset_of!(c::R4XStartR4Desk, clipboard_revision) == 280);
    assert!(mem::offset_of!(c::R4XStartR4Desk, clipboard_info) == 288);
    assert!(mem::offset_of!(c::R4XStartR4Desk, clipboard_clear) == 296);
    assert!(mem::offset_of!(c::R4XStartR4Desk, read_key_codepoint) == 304);
    assert!(mem::offset_of!(c::R4XStartR4Desk, reserved1) == 312);
    assert!(mem::offset_of!(c::R4XStartR4Desk, remote_frame_info) == 320);
    assert!(mem::offset_of!(c::R4XStartR4Desk, remote_frame_read) == 328);
    assert!(mem::offset_of!(c::R4XStartR4Desk, remote_frame_wait) == 336);
    assert!(mem::offset_of!(c::R4XStartR4Desk, remote_frame_publish) == 344);
    assert!(mem::offset_of!(c::R4XStartR4Desk, remote_input_push) == 352);
    assert!(mem::offset_of!(c::R4XStartR4Desk, remote_input_poll) == 360);
    assert!(mem::offset_of!(c::R4XStartR4Desk, remote_input_status) == 368);
    assert!(mem::offset_of!(c::R4XStartR4Desk, desktop_activity_wait) == 376);
    assert!(mem::offset_of!(c::R4XStartR4Desk, remote_frame_map) == 384);
    assert!(mem::offset_of!(c::R4XStartR4Desk, program_current_console_host) == 392);
    assert!(mem::offset_of!(c::R4XStartR4Desk, program_request_desktop) == 400);
    assert!(mem::offset_of!(c::R4XStartR4Desk, program_spawn_with_console_host) == 408);
    assert!(mem::offset_of!(c::R4XStartR4Desk, program_spawn_with_console_host_handle) == 416);
    assert!(mem::offset_of!(c::R4XStartR4Desk, program_set_window_handle) == 424);
    assert!(mem::offset_of!(c::R4XStartR4Desk, console_push_input) == 432);
    assert!(mem::offset_of!(c::R4XStartR4Desk, remote_frame_acquire) == 440);
    assert!(mem::offset_of!(c::R4XStartR4Desk, remote_frame_release) == 448);
    assert!(mem::offset_of!(c::R4XStartR4Desk, remote_frame_consumers) == 456);
    assert!(mem::offset_of!(c::R4XStartR4Desk, remote_frame_publish_regions) == 464);
    assert!(mem::offset_of!(c::R4XStartR4Desk, console_input_wait) == 472);
    assert!(mem::offset_of!(c::R4XStartR4Desk, physical_key_poll) == 480);
    assert!(mem::offset_of!(c::R4XStartR4Desk, mouse_motion) == 488);
    assert!(mem::offset_of!(c::R4XStartR4Desk, remote_frame_snapshot_acquire) == 496);
    assert!(mem::offset_of!(c::R4XStartR4Desk, remote_frame_snapshot_release) == 504);
    assert!(mem::offset_of!(c::R4XStartR4Desk, remote_frame_source_reset) == 512);
    assert!(mem::offset_of!(c::R4XStartR4Desk, remote_frame_capture_stats) == 520);
    assert!(mem::offset_of!(c::R4XStartR4Desk, desktop_activity_notify) == 528);
};
static DRAW: Table<c::R4XStartR4Draw> = Table::new();
fn configure_draw() {
    let value = c::R4XStartR4Draw {
        screen_width: super::platform_callbacks::draw_screen_width as *const () as usize,
        screen_height: super::platform_callbacks::draw_screen_height as *const () as usize,
        clear: super::platform_callbacks::draw_clear as *const () as usize,
        rect: super::platform_callbacks::draw_rect as *const () as usize,
        text: super::platform_callbacks::draw_text as *const () as usize,
        display_revision: super::platform_callbacks::draw_display_revision as *const () as usize,
        display_begin_frame: super::platform_callbacks::draw_display_begin_frame as *const () as usize,
        display_begin_frame_rect: super::platform_callbacks::draw_display_begin_frame_rect as *const () as usize,
        display_present: super::platform_callbacks::draw_display_present as *const () as usize,
        display_blit_xrgb32: super::platform_callbacks::draw_display_blit_xrgb32 as *const () as usize,
        gui_clear: crate::program::gui_draw::r4os_rust_program_gui_clear as *const () as usize,
        gui_rect: crate::program::gui_draw::r4os_rust_program_gui_rect as *const () as usize,
        gui_draw_text: super::platform_callbacks::draw_gui_draw_text as *const () as usize,
        gui_draw_text_ex: super::platform_callbacks::draw_gui_draw_text_ex as *const () as usize,
        gui_blit: crate::program::gui_draw::r4os_rust_program_gui_blit as *const () as usize,
        gui_raster_read: crate::program::gui_api::r4os_rust_program_gui_api_raster as *const () as usize,
        gui_present: super::platform_callbacks::draw_gui_present as *const () as usize,
        font_count: super::platform_callbacks::draw_font_count as *const () as usize,
        font_info: super::platform_callbacks::draw_font_info as *const () as usize,
        font_measure: super::platform_callbacks::draw_font_measure as *const () as usize,
        gui_set_font: crate::program::gui_font::r4os_rust_program_gui_font_set as *const () as usize,
        gui_font: crate::program::gui_font::r4os_rust_program_gui_font_get as *const () as usize,
        text_font: super::platform_callbacks::draw_text_font as *const () as usize,
        font_reload: super::platform_callbacks::draw_font_reload as *const () as usize,
        font_glyph_row: super::platform_callbacks::draw_font_glyph_row as *const () as usize,
        gui_blend_alpha8: crate::program::gui_draw::r4os_rust_program_gui_alpha as *const () as usize,
        gui_frame_begin: super::platform_callbacks::draw_gui_frame_begin as *const () as usize,
        gui_frame_append: crate::program::gui_api::r4os_rust_program_gui_api_append as *const () as usize,
        gui_frame_commit: super::platform_callbacks::draw_gui_frame_commit as *const () as usize,
        gui_frame_cancel: super::platform_callbacks::draw_gui_frame_cancel as *const () as usize,
        gui_frame_info: super::platform_callbacks::draw_gui_frame_info as *const () as usize,
        gui_frame_read: crate::program::gui_api::r4os_rust_program_gui_api_read as *const () as usize,
        display_blit_xrgb32_stride: super::platform_callbacks::draw_display_blit_xrgb32_stride as *const () as usize,
        display_present_regions: super::platform_callbacks::draw_display_present_regions as *const () as usize,
        display_present_capabilities: super::platform_callbacks::draw_display_present_capabilities as *const () as usize,
        display_present_completion: super::platform_callbacks::draw_display_present_completion as *const () as usize,
        gui_frame_begin_damage: super::platform_callbacks::draw_gui_frame_begin_damage as *const () as usize,
        gui_frame_generation_info: super::platform_callbacks::draw_gui_frame_generation_info as *const () as usize,
        gui_frame_generation_read: crate::program::gui_api::r4os_rust_program_gui_api_generation_read as *const () as usize,
        font_glyph_bitmap: super::platform_callbacks::draw_font_glyph_bitmap as *const () as usize,
        font_revision: super::platform_callbacks::draw_font_revision as *const () as usize,
        gui_frame_begin_replace: super::platform_callbacks::draw_gui_frame_begin_replace as *const () as usize,
        gui_frame_stream_info: super::platform_callbacks::draw_gui_frame_stream_info as *const () as usize,
        gui_shared_raster_create: crate::program::shared_raster::r4os_rust_program_shared_api_create as *const () as usize,
        gui_shared_raster_destroy: crate::program::shared_raster::r4os_rust_program_shared_api_destroy as *const () as usize,
        gui_shared_raster_map_write: crate::program::shared_raster::r4os_rust_program_shared_api_write as *const () as usize,
        gui_shared_raster_publish: crate::program::shared_raster::r4os_rust_program_shared_api_publish as *const () as usize,
        gui_shared_raster_acquire: crate::program::shared_raster::r4os_rust_program_shared_api_acquire as *const () as usize,
        gui_shared_raster_release: crate::program::shared_raster::r4os_rust_program_shared_api_release as *const () as usize,
        gfx_buffer_create: crate::program::domain_api::r4os_rust_program_domain_gfx_buffer_create as *const () as usize,
        gfx_buffer_describe: crate::program::domain_api::r4os_rust_program_domain_gfx_buffer_describe as *const () as usize,
        gfx_buffer_import: crate::program::domain_api::r4os_rust_program_domain_gfx_buffer_import as *const () as usize,
        gfx_buffer_release: crate::program::domain_api::r4os_rust_program_domain_gfx_buffer_release as *const () as usize,
        gfx_buffer_map: crate::program::domain_api::r4os_rust_program_domain_gfx_buffer_map as *const () as usize,
        gfx_buffer_unmap: crate::program::domain_api::r4os_rust_program_domain_gfx_buffer_unmap as *const () as usize,
        gfx_buffer_export_raster: crate::program::shared_raster::r4os_rust_program_shared_api_export as *const () as usize,
        gfx_buffer_stats: super::platform_callbacks::draw_gfx_buffer_stats as *const () as usize,
        gfx_queue_open: crate::program::domain_api::r4os_rust_program_domain_gfx_queue_open as *const () as usize,
        gfx_queue_close: crate::program::domain_api::r4os_rust_program_domain_gfx_queue_close as *const () as usize,
        gfx_queue_submit: crate::program::domain_api::r4os_rust_program_domain_gfx_queue_submit as *const () as usize,
        gfx_fence_query: super::platform_callbacks::draw_gfx_fence_query as *const () as usize,
        gfx_fence_wait: super::platform_callbacks::draw_gfx_fence_wait as *const () as usize,
        gfx_fence_cancel: crate::program::domain_api::r4os_rust_program_domain_gfx_fence_cancel as *const () as usize,
        gfx_fence_release: crate::program::domain_api::r4os_rust_program_domain_gfx_fence_release as *const () as usize,
        gfx_queue_backend: super::platform_callbacks::draw_gfx_queue_backend as *const () as usize,
        gfx_output_revision: super::platform_callbacks::draw_gfx_output_revision as *const () as usize,
        gfx_output_info: super::platform_callbacks::draw_gfx_output_info as *const () as usize,
        gfx_output_mode: super::platform_callbacks::draw_gfx_output_mode as *const () as usize,
        gfx_output_edid: super::platform_callbacks::draw_gfx_output_edid as *const () as usize,
        gfx_atomic_test: crate::program::domain_api::r4os_rust_program_domain_gfx_atomic_test as *const () as usize,
        gfx_atomic_commit: crate::program::domain_api::r4os_rust_program_domain_gfx_atomic_commit as *const () as usize,
        gfx_atomic_submit: crate::program::domain_api::r4os_rust_program_domain_gfx_atomic_submit as *const () as usize,
        gfx_atomic_status: super::platform_callbacks::draw_gfx_atomic_status as *const () as usize,
        gfx_atomic_resolve: crate::program::domain_api::r4os_rust_program_domain_gfx_atomic_resolve as *const () as usize,
        display_presentation_stats: super::platform_callbacks::draw_display_presentation_stats as *const () as usize,
        display_cursor_info: super::platform_callbacks::draw_display_cursor_info as *const () as usize,
        display_cursor_submit: crate::program::domain_api::r4os_rust_program_domain_display_cursor_submit as *const () as usize,
        display_cursor_status: super::platform_callbacks::draw_display_cursor_status as *const () as usize,
        gfx_queue_backend_info: super::platform_callbacks::draw_gfx_queue_backend_info as *const () as usize,
        gfx_native_start: crate::program::domain_api::r4os_rust_program_domain_gfx_native_start as *const () as usize,
        gfx_native_query: crate::program::domain_api::r4os_rust_program_domain_gfx_native_query as *const () as usize,
        gfx_native_receive: crate::program::domain_api::r4os_rust_program_domain_gfx_native_receive as *const () as usize,
        gfx_native_close: crate::program::domain_api::r4os_rust_program_domain_gfx_native_close as *const () as usize,
        gfx_native_wait: crate::program::domain_api::r4os_rust_program_domain_gfx_native_wait as *const () as usize,
        gfx_queue_submit_render_list: crate::program::domain_api::r4os_rust_program_domain_gfx_queue_submit_render_list as *const () as usize,
        display_presentation_info: super::platform_callbacks::draw_display_presentation_info as *const () as usize,
        display_presentation_feedback: super::platform_callbacks::draw_display_presentation_feedback as *const () as usize,
        display_output_target: super::platform_callbacks::draw_display_output_target as *const () as usize,
        gfx_queue_submit_output: crate::program::domain_api::r4os_rust_program_domain_gfx_queue_submit_output as *const () as usize,
        display_output_presentation_info: super::platform_callbacks::draw_display_output_presentation_info as *const () as usize,
        display_output_presentation_feedback: super::platform_callbacks::draw_display_output_presentation_feedback as *const () as usize,
        gfx_queue_submit_render_grid_list: crate::program::domain_api::r4os_rust_program_domain_gfx_queue_submit_render_grid_list as *const () as usize,
        gfx_output_color: super::platform_callbacks::draw_gfx_output_color as *const () as usize,
        gfx_queue_submit_render_color_list: crate::program::domain_api::r4os_rust_program_domain_gfx_queue_submit_render_color_list as *const () as usize,
        gfx_atomic_test_color: crate::program::domain_api::r4os_rust_program_domain_gfx_atomic_test_color as *const () as usize,
        gfx_atomic_submit_color: crate::program::domain_api::r4os_rust_program_domain_gfx_atomic_submit_color as *const () as usize,
        gfx_output_refresh: super::platform_callbacks::draw_gfx_output_refresh as *const () as usize,
        gfx_refresh_request: crate::program::domain_api::r4os_rust_program_domain_gfx_refresh_request as *const () as usize,
        gfx_memory_budget: super::platform_callbacks::draw_gfx_memory_budget as *const () as usize,
        gfx_telemetry: super::platform_callbacks::draw_gfx_telemetry as *const () as usize,
        gfx_output_power: super::platform_callbacks::draw_gfx_output_power as *const () as usize,
        gfx_power_request: crate::program::domain_api::r4os_rust_program_domain_gfx_power_request as *const () as usize,
        gfx_virtual_start: crate::program::domain_api::r4os_rust_program_domain_gfx_virtual_start as *const () as usize,
        gfx_virtual_query: crate::program::domain_api::r4os_rust_program_domain_gfx_virtual_query as *const () as usize,
        gfx_virtual_close: crate::program::domain_api::r4os_rust_program_domain_gfx_virtual_close as *const () as usize,
        gfx_virtual_wait: crate::program::domain_api::r4os_rust_program_domain_gfx_virtual_wait as *const () as usize,
        gfx_buffer_map_persistent: crate::program::domain_api::r4os_rust_program_domain_gfx_buffer_map_persistent as *const () as usize,
        gfx_queue_backend_properties: super::platform_callbacks::draw_gfx_queue_backend_properties as *const () as usize,
        gfx_queue_submit_native: crate::program::domain_api::r4os_rust_program_domain_gfx_queue_submit_native as *const () as usize,
        gfx_queue_submit_render_color_grid_list: crate::program::domain_api::r4os_rust_program_domain_gfx_queue_submit_render_color_grid_list as *const () as usize,
        gfx_output_brightness: super::platform_callbacks::draw_gfx_output_brightness as *const () as usize,
        gfx_brightness_request: crate::program::domain_api::r4os_rust_program_domain_gfx_brightness_request as *const () as usize,
        ..Default::default()
    };
    DRAW.configure(value);
}
const _: () = {
    assert!(mem::size_of::<c::R4XStartR4Draw>() == 912);
    assert!(mem::offset_of!(c::R4XStartR4Draw, magic) == 0);
    assert!(mem::offset_of!(c::R4XStartR4Draw, abi_version) == 4);
    assert!(mem::offset_of!(c::R4XStartR4Draw, size) == 8);
    assert!(mem::offset_of!(c::R4XStartR4Draw, flags) == 12);
    assert!(mem::offset_of!(c::R4XStartR4Draw, screen_width) == 16);
    assert!(mem::offset_of!(c::R4XStartR4Draw, screen_height) == 24);
    assert!(mem::offset_of!(c::R4XStartR4Draw, clear) == 32);
    assert!(mem::offset_of!(c::R4XStartR4Draw, rect) == 40);
    assert!(mem::offset_of!(c::R4XStartR4Draw, text) == 48);
    assert!(mem::offset_of!(c::R4XStartR4Draw, display_revision) == 56);
    assert!(mem::offset_of!(c::R4XStartR4Draw, display_begin_frame) == 64);
    assert!(mem::offset_of!(c::R4XStartR4Draw, display_begin_frame_rect) == 72);
    assert!(mem::offset_of!(c::R4XStartR4Draw, display_present) == 80);
    assert!(mem::offset_of!(c::R4XStartR4Draw, display_blit_xrgb32) == 88);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gui_clear) == 96);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gui_rect) == 104);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gui_draw_text) == 112);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gui_draw_text_ex) == 120);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gui_blit) == 128);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gui_raster_read) == 136);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gui_present) == 144);
    assert!(mem::offset_of!(c::R4XStartR4Draw, font_count) == 152);
    assert!(mem::offset_of!(c::R4XStartR4Draw, font_info) == 160);
    assert!(mem::offset_of!(c::R4XStartR4Draw, font_measure) == 168);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gui_set_font) == 176);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gui_font) == 184);
    assert!(mem::offset_of!(c::R4XStartR4Draw, text_font) == 192);
    assert!(mem::offset_of!(c::R4XStartR4Draw, font_reload) == 200);
    assert!(mem::offset_of!(c::R4XStartR4Draw, font_glyph_row) == 208);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gui_blend_alpha8) == 216);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gui_frame_begin) == 224);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gui_frame_append) == 232);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gui_frame_commit) == 240);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gui_frame_cancel) == 248);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gui_frame_info) == 256);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gui_frame_read) == 264);
    assert!(mem::offset_of!(c::R4XStartR4Draw, display_blit_xrgb32_stride) == 272);
    assert!(mem::offset_of!(c::R4XStartR4Draw, display_present_regions) == 280);
    assert!(mem::offset_of!(c::R4XStartR4Draw, display_present_capabilities) == 288);
    assert!(mem::offset_of!(c::R4XStartR4Draw, display_present_completion) == 296);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gui_frame_begin_damage) == 304);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gui_frame_generation_info) == 312);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gui_frame_generation_read) == 320);
    assert!(mem::offset_of!(c::R4XStartR4Draw, font_glyph_bitmap) == 328);
    assert!(mem::offset_of!(c::R4XStartR4Draw, font_revision) == 336);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gui_frame_begin_replace) == 344);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gui_frame_stream_info) == 352);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gui_shared_raster_create) == 360);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gui_shared_raster_destroy) == 368);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gui_shared_raster_map_write) == 376);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gui_shared_raster_publish) == 384);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gui_shared_raster_acquire) == 392);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gui_shared_raster_release) == 400);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_buffer_create) == 408);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_buffer_describe) == 416);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_buffer_import) == 424);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_buffer_release) == 432);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_buffer_map) == 440);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_buffer_unmap) == 448);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_buffer_export_raster) == 456);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_buffer_stats) == 464);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_queue_open) == 472);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_queue_close) == 480);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_queue_submit) == 488);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_fence_query) == 496);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_fence_wait) == 504);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_fence_cancel) == 512);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_fence_release) == 520);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_queue_backend) == 528);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_output_revision) == 536);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_output_info) == 544);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_output_mode) == 552);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_output_edid) == 560);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_atomic_test) == 568);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_atomic_commit) == 576);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_atomic_submit) == 584);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_atomic_status) == 592);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_atomic_resolve) == 600);
    assert!(mem::offset_of!(c::R4XStartR4Draw, display_presentation_stats) == 608);
    assert!(mem::offset_of!(c::R4XStartR4Draw, display_cursor_info) == 616);
    assert!(mem::offset_of!(c::R4XStartR4Draw, display_cursor_submit) == 624);
    assert!(mem::offset_of!(c::R4XStartR4Draw, display_cursor_status) == 632);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_queue_backend_info) == 640);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_native_start) == 648);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_native_query) == 656);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_native_receive) == 664);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_native_close) == 672);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_native_wait) == 680);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_queue_submit_render_list) == 688);
    assert!(mem::offset_of!(c::R4XStartR4Draw, display_presentation_info) == 696);
    assert!(mem::offset_of!(c::R4XStartR4Draw, display_presentation_feedback) == 704);
    assert!(mem::offset_of!(c::R4XStartR4Draw, display_output_target) == 712);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_queue_submit_output) == 720);
    assert!(mem::offset_of!(c::R4XStartR4Draw, display_output_presentation_info) == 728);
    assert!(mem::offset_of!(c::R4XStartR4Draw, display_output_presentation_feedback) == 736);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_queue_submit_render_grid_list) == 744);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_output_color) == 752);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_queue_submit_render_color_list) == 760);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_atomic_test_color) == 768);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_atomic_submit_color) == 776);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_output_refresh) == 784);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_refresh_request) == 792);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_memory_budget) == 800);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_telemetry) == 808);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_output_power) == 816);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_power_request) == 824);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_virtual_start) == 832);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_virtual_query) == 840);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_virtual_close) == 848);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_virtual_wait) == 856);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_buffer_map_persistent) == 864);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_queue_backend_properties) == 872);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_queue_submit_native) == 880);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_queue_submit_render_color_grid_list) == 888);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_output_brightness) == 896);
    assert!(mem::offset_of!(c::R4XStartR4Draw, gfx_brightness_request) == 904);
};
static NET: Table<c::R4XStartR4Net> = Table::new();
fn configure_net() {
    let value = c::R4XStartR4Net {
        tcp_connect: crate::program::network_api::r4os_rust_r4net_tcp_connect as *const () as usize,
        tcp_write: crate::program::network_api::r4os_rust_r4net_tcp_write as *const () as usize,
        tcp_read: crate::program::network_api::r4os_rust_r4net_tcp_read as *const () as usize,
        tcp_close: crate::program::network_api::r4os_rust_r4net_tcp_close as *const () as usize,
        tcp_summary: crate::program::network_api::r4os_rust_r4net_tcp_summary as *const () as usize,
        tcp_connection: crate::program::network_api::r4os_rust_r4net_tcp_connection as *const ()
            as usize,
        tcp_echo_listen_once: crate::program::network_api::r4os_rust_r4net_tcp_echo_listen_once
            as *const () as usize,
        tcp_accept_read_once: crate::program::network_api::r4os_rust_r4net_tcp_accept_read_once
            as *const () as usize,
        net_ipv4_send: crate::program::network_api::r4os_rust_r4net_net_ipv4_send as *const ()
            as usize,
        net_ipv4_recv: crate::program::network_api::r4os_rust_r4net_net_ipv4_recv as *const ()
            as usize,
        net_config_get: crate::program::network_api::r4os_rust_r4net_net_config_get as *const ()
            as usize,
        net_config_set: crate::program::network_api::r4os_rust_r4net_net_config_set as *const ()
            as usize,
        net_dns_resolve: crate::program::network_api::r4os_rust_r4net_net_dns_resolve as *const ()
            as usize,
        net_dns_resolve_server: crate::program::network_api::r4os_rust_r4net_net_dns_resolve_server
            as *const () as usize,
        net_dhcp_acquire: crate::program::network_api::r4os_rust_r4net_net_dhcp_acquire as *const ()
            as usize,
        net_dhcp_renew: crate::program::network_api::r4os_rust_r4net_net_dhcp_renew as *const ()
            as usize,
        net_dhcp_release: crate::program::network_api::r4os_rust_r4net_net_dhcp_release as *const ()
            as usize,
        net_dhcp_status: crate::program::network_api::r4os_rust_r4net_net_dhcp_status as *const ()
            as usize,
        net_detail_get: crate::program::network_api::r4os_rust_r4net_net_detail_get as *const ()
            as usize,
        net_diag_run: crate::program::network_api::r4os_rust_r4net_net_diag_run as *const ()
            as usize,
        ipc_open: crate::program::network_api::r4os_rust_r4net_ipc_open as *const () as usize,
        ipc_send: crate::program::network_api::r4os_rust_r4net_ipc_send as *const () as usize,
        ipc_recv: crate::program::network_api::r4os_rust_r4net_ipc_recv as *const () as usize,
        ipc_poll: crate::program::network_api::r4os_rust_r4net_ipc_poll as *const () as usize,
        ipc_close: crate::program::network_api::r4os_rust_r4net_ipc_close as *const () as usize,
        ipc_summary: crate::program::network_api::r4os_rust_r4net_ipc_summary as *const () as usize,
        ipc_channel: crate::program::network_api::r4os_rust_r4net_ipc_channel as *const () as usize,
        serial_link_status: crate::program::network_api::r4os_rust_r4net_serial_link_status
            as *const () as usize,
        serial_link_poll: crate::program::network_api::r4os_rust_r4net_serial_link_poll as *const ()
            as usize,
        serial_link_send_message:
            crate::program::network_api::r4os_rust_r4net_serial_link_send_message as *const ()
                as usize,
        serial_link_host_test: crate::program::network_api::r4os_rust_r4net_serial_link_host_test
            as *const () as usize,
        serial_link_inbox: crate::program::network_api::r4os_rust_r4net_serial_link_inbox
            as *const () as usize,
        net_service_request: crate::program::network_api::r4os_rust_r4net_net_service_request
            as *const () as usize,
        ipc_performance: crate::program::network_api::r4os_rust_r4net_ipc_performance as *const ()
            as usize,
        tcp_performance: crate::program::network_api::r4os_rust_r4net_tcp_performance as *const ()
            as usize,
        ..Default::default()
    };
    NET.configure(value);
}
const _: () = {
    assert!(mem::size_of::<c::R4XStartR4Net>() == 296);
    assert!(mem::offset_of!(c::R4XStartR4Net, magic) == 0);
    assert!(mem::offset_of!(c::R4XStartR4Net, abi_version) == 4);
    assert!(mem::offset_of!(c::R4XStartR4Net, size) == 8);
    assert!(mem::offset_of!(c::R4XStartR4Net, flags) == 12);
    assert!(mem::offset_of!(c::R4XStartR4Net, tcp_connect) == 16);
    assert!(mem::offset_of!(c::R4XStartR4Net, tcp_write) == 24);
    assert!(mem::offset_of!(c::R4XStartR4Net, tcp_read) == 32);
    assert!(mem::offset_of!(c::R4XStartR4Net, tcp_close) == 40);
    assert!(mem::offset_of!(c::R4XStartR4Net, tcp_summary) == 48);
    assert!(mem::offset_of!(c::R4XStartR4Net, tcp_connection) == 56);
    assert!(mem::offset_of!(c::R4XStartR4Net, tcp_echo_listen_once) == 64);
    assert!(mem::offset_of!(c::R4XStartR4Net, tcp_accept_read_once) == 72);
    assert!(mem::offset_of!(c::R4XStartR4Net, net_ipv4_send) == 80);
    assert!(mem::offset_of!(c::R4XStartR4Net, net_ipv4_recv) == 88);
    assert!(mem::offset_of!(c::R4XStartR4Net, net_config_get) == 96);
    assert!(mem::offset_of!(c::R4XStartR4Net, net_config_set) == 104);
    assert!(mem::offset_of!(c::R4XStartR4Net, net_dns_resolve) == 112);
    assert!(mem::offset_of!(c::R4XStartR4Net, net_dns_resolve_server) == 120);
    assert!(mem::offset_of!(c::R4XStartR4Net, net_dhcp_acquire) == 128);
    assert!(mem::offset_of!(c::R4XStartR4Net, net_dhcp_renew) == 136);
    assert!(mem::offset_of!(c::R4XStartR4Net, net_dhcp_release) == 144);
    assert!(mem::offset_of!(c::R4XStartR4Net, net_dhcp_status) == 152);
    assert!(mem::offset_of!(c::R4XStartR4Net, net_detail_get) == 160);
    assert!(mem::offset_of!(c::R4XStartR4Net, net_diag_run) == 168);
    assert!(mem::offset_of!(c::R4XStartR4Net, ipc_open) == 176);
    assert!(mem::offset_of!(c::R4XStartR4Net, ipc_send) == 184);
    assert!(mem::offset_of!(c::R4XStartR4Net, ipc_recv) == 192);
    assert!(mem::offset_of!(c::R4XStartR4Net, ipc_poll) == 200);
    assert!(mem::offset_of!(c::R4XStartR4Net, ipc_close) == 208);
    assert!(mem::offset_of!(c::R4XStartR4Net, ipc_summary) == 216);
    assert!(mem::offset_of!(c::R4XStartR4Net, ipc_channel) == 224);
    assert!(mem::offset_of!(c::R4XStartR4Net, serial_link_status) == 232);
    assert!(mem::offset_of!(c::R4XStartR4Net, serial_link_poll) == 240);
    assert!(mem::offset_of!(c::R4XStartR4Net, serial_link_send_message) == 248);
    assert!(mem::offset_of!(c::R4XStartR4Net, serial_link_host_test) == 256);
    assert!(mem::offset_of!(c::R4XStartR4Net, serial_link_inbox) == 264);
    assert!(mem::offset_of!(c::R4XStartR4Net, net_service_request) == 272);
    assert!(mem::offset_of!(c::R4XStartR4Net, ipc_performance) == 280);
    assert!(mem::offset_of!(c::R4XStartR4Net, tcp_performance) == 288);
};
static AUDIO: Table<c::R4XStartR4Audio> = Table::new();
fn configure_audio() {
    let value = c::R4XStartR4Audio {
        audio_open_stream: crate::program::domain_api::r4os_rust_program_domain_audio_open_stream
            as *const () as usize,
        audio_write: crate::program::domain_api::r4os_rust_program_domain_audio_write as *const ()
            as usize,
        audio_close: crate::program::domain_api::r4os_rust_program_domain_audio_close as *const ()
            as usize,
        audio_set_volume: crate::program::domain_api::r4os_rust_program_domain_audio_set_volume
            as *const () as usize,
        sid_acquire: crate::program::r4audio::r4os_rust_r4audio_sid_acquire as *const () as usize,
        sid_write_register: crate::program::r4audio::r4os_rust_r4audio_sid_write_register
            as *const () as usize,
        sid_release: crate::program::r4audio::r4os_rust_r4audio_sid_release as *const () as usize,
        sid_load_data: crate::program::r4audio::r4os_rust_r4audio_sid_load_data as *const ()
            as usize,
        sid_init: crate::program::r4audio::r4os_rust_r4audio_sid_init as *const () as usize,
        sid_play_frame: crate::program::r4audio::r4os_rust_r4audio_sid_play_frame as *const ()
            as usize,
        sid_stop: crate::program::r4audio::r4os_rust_r4audio_sid_stop as *const () as usize,
        sid_model_name: crate::program::r4audio::r4os_rust_r4audio_sid_model_name as *const ()
            as usize,
        midi_open_synth: crate::program::r4audio::r4os_rust_r4audio_midi_open_synth as *const ()
            as usize,
        midi_send: crate::program::r4audio::r4os_rust_r4audio_midi_send as *const () as usize,
        midi_close: crate::program::r4audio::r4os_rust_r4audio_midi_close as *const () as usize,
        opl3_write_register: crate::program::r4audio::r4os_rust_r4audio_opl3_write_register
            as *const () as usize,
        opl3_reset: crate::program::r4audio::r4os_rust_r4audio_opl3_reset as *const () as usize,
        opl3_render_block: crate::program::r4audio::r4os_rust_r4audio_opl3_render_block as *const ()
            as usize,
        opl3_stop: crate::program::r4audio::r4os_rust_r4audio_opl3_stop as *const () as usize,
        audio_output_info: crate::program::domain_api::r4os_rust_program_domain_audio_output_info
            as *const () as usize,
        audio_select_output:
            crate::program::domain_api::r4os_rust_program_domain_audio_select_output as *const ()
                as usize,
        ..Default::default()
    };
    AUDIO.configure(value);
}
const _: () = {
    assert!(mem::size_of::<c::R4XStartR4Audio>() == 200);
    assert!(mem::offset_of!(c::R4XStartR4Audio, magic) == 0);
    assert!(mem::offset_of!(c::R4XStartR4Audio, abi_version) == 4);
    assert!(mem::offset_of!(c::R4XStartR4Audio, size) == 8);
    assert!(mem::offset_of!(c::R4XStartR4Audio, flags) == 12);
    assert!(mem::offset_of!(c::R4XStartR4Audio, audio_open_stream) == 16);
    assert!(mem::offset_of!(c::R4XStartR4Audio, audio_write) == 24);
    assert!(mem::offset_of!(c::R4XStartR4Audio, audio_close) == 32);
    assert!(mem::offset_of!(c::R4XStartR4Audio, audio_set_volume) == 40);
    assert!(mem::offset_of!(c::R4XStartR4Audio, sid_acquire) == 48);
    assert!(mem::offset_of!(c::R4XStartR4Audio, sid_write_register) == 56);
    assert!(mem::offset_of!(c::R4XStartR4Audio, sid_release) == 64);
    assert!(mem::offset_of!(c::R4XStartR4Audio, sid_load_data) == 72);
    assert!(mem::offset_of!(c::R4XStartR4Audio, sid_init) == 80);
    assert!(mem::offset_of!(c::R4XStartR4Audio, sid_play_frame) == 88);
    assert!(mem::offset_of!(c::R4XStartR4Audio, sid_stop) == 96);
    assert!(mem::offset_of!(c::R4XStartR4Audio, sid_model_name) == 104);
    assert!(mem::offset_of!(c::R4XStartR4Audio, midi_open_synth) == 112);
    assert!(mem::offset_of!(c::R4XStartR4Audio, midi_send) == 120);
    assert!(mem::offset_of!(c::R4XStartR4Audio, midi_close) == 128);
    assert!(mem::offset_of!(c::R4XStartR4Audio, opl3_write_register) == 136);
    assert!(mem::offset_of!(c::R4XStartR4Audio, opl3_reset) == 144);
    assert!(mem::offset_of!(c::R4XStartR4Audio, opl3_render_block) == 152);
    assert!(mem::offset_of!(c::R4XStartR4Audio, opl3_stop) == 160);
    assert!(mem::offset_of!(c::R4XStartR4Audio, reserved0) == 168);
    assert!(mem::offset_of!(c::R4XStartR4Audio, reserved1) == 176);
    assert!(mem::offset_of!(c::R4XStartR4Audio, audio_output_info) == 184);
    assert!(mem::offset_of!(c::R4XStartR4Audio, audio_select_output) == 192);
};
static DEV: Table<c::R4XStartR4Dev> = Table::new();
fn configure_dev() {
    use super::r4dev;
    const HEAP_CHECK: c::R4DevFns::memory_kernel_heap_check = r4dev::memory::kernel_heap_check;
    const SCHEDULER_CHECK: c::R4DevFns::scheduler_kernel_check = r4dev::scheduler::kernel_check;
    const NVME_CHECK: c::R4DevFns::storage_nvme_check = r4dev::nvme::check;
    const USB_CHECK: c::R4DevFns::usb_contract_check = r4dev::usb::check;
    const BLOCK_QUEUE_CHECK: c::R4DevFns::storage_block_queue_check = r4dev::block_queue::check;
    const SMP_CHECK: c::R4DevFns::smp_kernel_check = r4dev::smp::check;
    const SMP_SNAPSHOT: c::R4DevFns::performance_smp_tlb = r4dev::smp::snapshot;
    const CPU_TOPOLOGY: c::R4DevFns::cpu_topology = r4dev::cpu_topology::info;
    const CPU_CACHE: c::R4DevFns::cpu_cache_topology = r4dev::cpu_topology::cache;
    const CPU_LIFECYCLE: c::R4DevFns::cpu_lifecycle = r4dev::cpu_lifecycle::control;
    const CPU_LIFECYCLE_CHECK: c::R4DevFns::kernel_cpu_lifecycle_check =
        r4dev::cpu_lifecycle::check;
    const CPU_CAPABILITIES: c::R4DevFns::cpu_capabilities = r4dev::cpu_capabilities::info;
    const CPU_FEATURE: c::R4DevFns::cpu_feature = r4dev::cpu_capabilities::feature;
    const CPU_REGISTER_READ: c::R4DevFns::cpu_register_read = r4dev::cpu_capabilities::read;
    const CPU_REGISTER_CHECK: c::R4DevFns::kernel_cpu_register_check =
        r4dev::cpu_capabilities::check;
    const CPU_POWER_INFO: c::R4DevFns::cpu_power_info = crate::platform::cpu_power::info;
    const CPU_POWER_CAPTURE: c::R4DevFns::cpu_power_capture = crate::platform::cpu_power::capture;
    const CPU_PERFORMANCE: c::R4DevFns::cpu_performance = crate::platform::cpu_power::performance;
    const CPU_CPPC: c::R4DevFns::cpu_cppc = crate::platform::cpu_power::cppc;
    const CPU_HINT: c::R4DevFns::cpu_performance_hint = crate::platform::cpu_power::hints::info;
    const CPU_FIRMWARE: c::R4DevFns::cpu_firmware_performance =
        crate::platform::cpu_power::firmware::call;
    const CPU_IDLE: c::R4DevFns::cpu_idle_info = r4dev::cpu_capabilities::idle;
    r4dev::providers::install();
    let value = c::R4XStartR4Dev {
        serial_count: super::serial_api::count as c::R4DevFns::serial_count as *const () as usize,
        serial_port_at: super::serial_api::port_at as c::R4DevFns::serial_port_at as *const ()
            as usize,
        serial_open: super::serial_api::open as c::R4DevFns::serial_open as *const () as usize,
        serial_configure: super::serial_api::configure as c::R4DevFns::serial_configure as *const ()
            as usize,
        serial_read: super::serial_api::read as c::R4DevFns::serial_read as *const () as usize,
        serial_write: super::serial_api::write as c::R4DevFns::serial_write as *const () as usize,
        serial_drain: super::serial_api::drain as c::R4DevFns::serial_drain as *const () as usize,
        serial_status: super::serial_api::status as c::R4DevFns::serial_status as *const ()
            as usize,
        serial_cancel: super::serial_api::cancel as c::R4DevFns::serial_cancel as *const ()
            as usize,
        serial_close: super::serial_api::close as c::R4DevFns::serial_close as *const () as usize,

        device_inventory_summary: r4dev::devices::r4os_rust_r4dev_device_inventory_summary
            as *const () as usize,
        device_inventory_record: r4dev::devices::r4os_rust_r4dev_device_inventory_record
            as *const () as usize,
        memory_summary: r4dev::memory::r4os_rust_r4dev_memory_summary as *const () as usize,
        memory_block_count: r4dev::memory::r4os_rust_r4dev_memory_block_count as *const () as usize,
        memory_block: r4dev::memory::r4os_rust_r4dev_memory_block as *const () as usize,
        memory_pressure_snapshot: r4dev::memory::r4os_rust_r4dev_memory_pressure_snapshot
            as *const () as usize,
        memory_reclaim_probe: r4dev::memory::r4os_rust_r4dev_memory_reclaim_probe as *const ()
            as usize,
        memory_backing_store_probe: r4dev::memory_probes::r4os_rust_r4dev_memory_backing_store_probe
            as *const () as usize,
        memory_backing_store_slot_probe:
            r4dev::memory_probes::r4os_rust_r4dev_memory_backing_store_slot_probe as *const ()
                as usize,
        memory_pager_gate_probe: r4dev::memory_probes::r4os_rust_r4dev_memory_pager_gate_probe
            as *const () as usize,
        memory_page_io_probe: r4dev::memory_probes::r4os_rust_r4dev_memory_page_io_probe
            as *const () as usize,
        memory_vm_page_state_probe: r4dev::memory_probes::r4os_rust_r4dev_memory_vm_page_state_probe
            as *const () as usize,
        memory_vm_reserve_probe: super::vm::r4os_rust_program_vm_probe as *const () as usize,
        paging_summary: r4dev::paging::r4os_rust_r4dev_paging_summary as *const () as usize,
        performance_summary: r4dev::summary::r4os_rust_r4dev_performance_summary as *const ()
            as usize,
        performance_task: r4dev::paging::r4os_rust_r4dev_performance_task as *const () as usize,
        performance_storage: r4dev::storage::r4os_rust_r4dev_performance_storage as *const ()
            as usize,
        performance_boot_phase: r4dev::boot::r4os_rust_r4dev_performance_boot_phase as *const ()
            as usize,
        protocol_status: r4dev::protocol::r4os_rust_r4dev_protocol_status as *const () as usize,
        protocol_dispatch: r4dev::protocol::r4os_rust_r4dev_protocol_dispatch as *const () as usize,
        display_summary: r4dev::display::r4os_rust_r4dev_display_summary as *const () as usize,
        hardware_summary: r4dev::devices::r4os_rust_r4dev_hardware_summary as *const () as usize,
        boot_info_summary: r4dev::boot::r4os_rust_r4dev_boot_info_summary as *const () as usize,
        boot_info_memory_count: r4dev::boot::r4os_rust_r4dev_boot_info_memory_count as *const ()
            as usize,
        boot_info_memory_entry: r4dev::boot::r4os_rust_r4dev_boot_info_memory_entry as *const ()
            as usize,
        program_instance_storage_summary:
            r4dev::providers::r4os_rust_r4dev_program_instance_storage_summary as *const () as usize,
        program_instance_storage_self_test:
            r4dev::providers::r4os_rust_r4dev_program_instance_storage_self_test as *const ()
                as usize,
        program_registry_summary: r4dev::providers::r4os_rust_r4dev_program_registry_summary
            as *const () as usize,
        program_registry_self_test: r4dev::providers::r4os_rust_r4dev_program_registry_self_test
            as *const () as usize,
        program_registry_summary_v2: r4dev::providers::r4os_rust_r4dev_program_registry_summary_v2
            as *const () as usize,
        program_registry_self_test_v2:
            r4dev::providers::r4os_rust_r4dev_program_registry_self_test_v2 as *const () as usize,
        execution_inventory_summary: r4dev::providers::r4os_rust_r4dev_execution_inventory_summary
            as *const () as usize,
        program_instance_storage_summary_v2:
            r4dev::providers::r4os_rust_r4dev_program_instance_storage_summary_v2 as *const ()
                as usize,
        kernel_version: r4dev::providers::r4os_rust_r4dev_kernel_version as *const () as usize,
        performance_boot_phase_clock: r4dev::boot::r4os_rust_r4dev_performance_boot_phase_clock
            as *const () as usize,
        performance_irq_timing: r4dev::telemetry::r4os_rust_r4dev_performance_irq_timing
            as *const () as usize,
        performance_boot_summary: r4dev::boot::r4os_rust_r4dev_performance_boot_summary as *const ()
            as usize,
        performance_driver_work: r4dev::telemetry::r4os_rust_r4dev_performance_driver_work
            as *const () as usize,
        performance_pci_inventory: r4dev::telemetry::r4os_rust_r4dev_performance_pci_inventory
            as *const () as usize,
        performance_input: r4dev::telemetry::r4os_rust_r4dev_performance_input as *const ()
            as usize,
        display_state: r4dev::display::r4os_rust_r4dev_display_state as *const () as usize,
        driver_module_info: r4dev::display::r4os_rust_r4dev_driver_module_info as *const ()
            as usize,
        performance_instrumentation: r4dev::telemetry::r4os_rust_r4dev_performance_instrumentation
            as *const () as usize,
        performance_lock: r4dev::telemetry::r4os_rust_r4dev_performance_lock as *const () as usize,
        performance_stack: r4dev::telemetry::r4os_rust_r4dev_performance_stack as *const ()
            as usize,
        stack_fault: super::stack_fault::snapshot as *const () as usize,
        memory_kernel_heap_check: HEAP_CHECK as *const () as usize,
        scheduler_kernel_check: SCHEDULER_CHECK as *const () as usize,
        storage_block_queue_check: BLOCK_QUEUE_CHECK as *const () as usize,
        storage_nvme_check: NVME_CHECK as *const () as usize,
        usb_contract_check: USB_CHECK as *const () as usize,
        smp_kernel_check: SMP_CHECK as *const () as usize,
        performance_smp_tlb: SMP_SNAPSHOT as *const () as usize,
        cpu_topology: CPU_TOPOLOGY as *const () as usize,
        cpu_cache_topology: CPU_CACHE as *const () as usize,
        cpu_lifecycle: CPU_LIFECYCLE as *const () as usize,
        kernel_cpu_lifecycle_check: CPU_LIFECYCLE_CHECK as *const () as usize,
        cpu_capabilities: CPU_CAPABILITIES as *const () as usize,
        cpu_feature: CPU_FEATURE as *const () as usize,
        cpu_register_read: CPU_REGISTER_READ as *const () as usize,
        kernel_cpu_register_check: CPU_REGISTER_CHECK as *const () as usize,
        cpu_power_info: CPU_POWER_INFO as *const () as usize,
        cpu_power_capture: CPU_POWER_CAPTURE as *const () as usize,
        cpu_performance: CPU_PERFORMANCE as *const () as usize,
        cpu_cppc: CPU_CPPC as *const () as usize,
        cpu_performance_hint: CPU_HINT as *const () as usize,
        cpu_firmware_performance: CPU_FIRMWARE as *const () as usize,
        cpu_idle_info: CPU_IDLE as *const () as usize,
        ..Default::default()
    };
    DEV.configure(value);
}

const _: () = {
    assert!(mem::size_of::<c::R4XStartR4Dev>() == 656);
    assert!(mem::offset_of!(c::R4XStartR4Dev, cpu_idle_info) == 648);
    assert!(mem::offset_of!(c::R4XStartR4Dev, cpu_firmware_performance) == 640);
    assert!(mem::size_of::<c::CpuFirmwareInfo>() == 192);
    assert!(mem::offset_of!(c::R4XStartR4Dev, cpu_capabilities) == 568);
    assert!(mem::offset_of!(c::R4XStartR4Dev, cpu_feature) == 576);
    assert!(mem::offset_of!(c::R4XStartR4Dev, cpu_register_read) == 584);
    assert!(mem::offset_of!(c::R4XStartR4Dev, kernel_cpu_register_check) == 592);
    assert!(mem::offset_of!(c::R4XStartR4Dev, cpu_power_info) == 600);
    assert!(mem::offset_of!(c::R4XStartR4Dev, cpu_power_capture) == 608);
    assert!(mem::offset_of!(c::R4XStartR4Dev, cpu_performance) == 616);
    assert!(mem::offset_of!(c::R4XStartR4Dev, cpu_cppc) == 624);
    assert!(mem::offset_of!(c::R4XStartR4Dev, cpu_performance_hint) == 632);
    assert!(mem::size_of::<c::CpuPerformanceRequest>() == 64);
    assert!(mem::size_of::<c::CpuPerformanceInfo>() == 176);
    assert!(mem::size_of::<c::CpuCapabilitiesInfo>() == 272);
    assert!(mem::size_of::<c::CpuFeatureInfo>() == 112);
    assert!(mem::size_of::<c::CpuRegisterInfo>() == 80);
    assert!(mem::size_of::<c::ProgramKernelCpuRegisterCheck>() == 128);
    assert!(mem::offset_of!(c::R4XStartR4Dev, cpu_lifecycle) == 552);
    assert!(mem::offset_of!(c::R4XStartR4Dev, kernel_cpu_lifecycle_check) == 560);
    assert!(mem::size_of::<c::CpuLifecycleInfo>() == 128);
    assert!(mem::size_of::<c::ProgramKernelCpuLifecycleCheck>() == 128);
    assert!(mem::offset_of!(c::R4XStartR4Dev, cpu_topology) == 536);
    assert!(mem::offset_of!(c::R4XStartR4Dev, cpu_cache_topology) == 544);
    assert!(mem::size_of::<c::CpuTopologyInfo>() == 112);
    assert!(mem::size_of::<c::CpuCacheTopologyInfo>() == 64);
    assert!(mem::offset_of!(c::R4XStartR4Dev, usb_contract_check) == 448);
    assert!(mem::offset_of!(c::R4XStartR4Dev, storage_nvme_check) == 440);
    assert!(mem::offset_of!(c::R4XStartR4Dev, storage_block_queue_check) == 432);
    assert!(mem::offset_of!(c::R4XStartR4Dev, smp_kernel_check) == 416);
    assert!(mem::offset_of!(c::R4XStartR4Dev, performance_smp_tlb) == 424);
    assert!(mem::offset_of!(c::R4XStartR4Dev, scheduler_kernel_check) == 408);
    assert!(mem::offset_of!(c::R4XStartR4Dev, memory_kernel_heap_check) == 400);
    assert!(mem::offset_of!(c::R4XStartR4Dev, magic) == 0);
    assert!(mem::offset_of!(c::R4XStartR4Dev, abi_version) == 4);
    assert!(mem::offset_of!(c::R4XStartR4Dev, size) == 8);
    assert!(mem::offset_of!(c::R4XStartR4Dev, flags) == 12);
    assert!(mem::offset_of!(c::R4XStartR4Dev, device_inventory_summary) == 16);
    assert!(mem::offset_of!(c::R4XStartR4Dev, device_inventory_record) == 24);
    assert!(mem::offset_of!(c::R4XStartR4Dev, memory_summary) == 32);
    assert!(mem::offset_of!(c::R4XStartR4Dev, memory_block_count) == 40);
    assert!(mem::offset_of!(c::R4XStartR4Dev, memory_block) == 48);
    assert!(mem::offset_of!(c::R4XStartR4Dev, memory_pressure_snapshot) == 56);
    assert!(mem::offset_of!(c::R4XStartR4Dev, memory_reclaim_probe) == 64);
    assert!(mem::offset_of!(c::R4XStartR4Dev, memory_backing_store_probe) == 72);
    assert!(mem::offset_of!(c::R4XStartR4Dev, memory_backing_store_slot_probe) == 80);
    assert!(mem::offset_of!(c::R4XStartR4Dev, memory_pager_gate_probe) == 88);
    assert!(mem::offset_of!(c::R4XStartR4Dev, memory_page_io_probe) == 96);
    assert!(mem::offset_of!(c::R4XStartR4Dev, memory_vm_page_state_probe) == 104);
    assert!(mem::offset_of!(c::R4XStartR4Dev, memory_vm_reserve_probe) == 112);
    assert!(mem::offset_of!(c::R4XStartR4Dev, paging_summary) == 120);
    assert!(mem::offset_of!(c::R4XStartR4Dev, performance_summary) == 128);
    assert!(mem::offset_of!(c::R4XStartR4Dev, performance_task) == 136);
    assert!(mem::offset_of!(c::R4XStartR4Dev, performance_storage) == 144);
    assert!(mem::offset_of!(c::R4XStartR4Dev, performance_boot_phase) == 152);
    assert!(mem::offset_of!(c::R4XStartR4Dev, protocol_status) == 160);
    assert!(mem::offset_of!(c::R4XStartR4Dev, protocol_dispatch) == 168);
    assert!(mem::offset_of!(c::R4XStartR4Dev, display_summary) == 176);
    assert!(mem::offset_of!(c::R4XStartR4Dev, hardware_summary) == 184);
    assert!(mem::offset_of!(c::R4XStartR4Dev, boot_info_summary) == 192);
    assert!(mem::offset_of!(c::R4XStartR4Dev, boot_info_memory_count) == 200);
    assert!(mem::offset_of!(c::R4XStartR4Dev, boot_info_memory_entry) == 208);
    assert!(mem::offset_of!(c::R4XStartR4Dev, reserved0) == 216);
    assert!(mem::offset_of!(c::R4XStartR4Dev, reserved1) == 224);
    assert!(mem::offset_of!(c::R4XStartR4Dev, program_instance_storage_summary) == 232);
    assert!(mem::offset_of!(c::R4XStartR4Dev, program_instance_storage_self_test) == 240);
    assert!(mem::offset_of!(c::R4XStartR4Dev, program_registry_summary) == 248);
    assert!(mem::offset_of!(c::R4XStartR4Dev, program_registry_self_test) == 256);
    assert!(mem::offset_of!(c::R4XStartR4Dev, program_registry_summary_v2) == 264);
    assert!(mem::offset_of!(c::R4XStartR4Dev, program_registry_self_test_v2) == 272);
    assert!(mem::offset_of!(c::R4XStartR4Dev, execution_inventory_summary) == 280);
    assert!(mem::offset_of!(c::R4XStartR4Dev, program_instance_storage_summary_v2) == 288);
    assert!(mem::offset_of!(c::R4XStartR4Dev, kernel_version) == 296);
    assert!(mem::offset_of!(c::R4XStartR4Dev, performance_boot_phase_clock) == 304);
    assert!(mem::offset_of!(c::R4XStartR4Dev, performance_irq_timing) == 312);
    assert!(mem::offset_of!(c::R4XStartR4Dev, performance_boot_summary) == 320);
    assert!(mem::offset_of!(c::R4XStartR4Dev, performance_driver_work) == 328);
    assert!(mem::offset_of!(c::R4XStartR4Dev, performance_pci_inventory) == 336);
    assert!(mem::offset_of!(c::R4XStartR4Dev, performance_input) == 344);
    assert!(mem::offset_of!(c::R4XStartR4Dev, display_state) == 352);
    assert!(mem::offset_of!(c::R4XStartR4Dev, driver_module_info) == 360);
    assert!(mem::offset_of!(c::R4XStartR4Dev, performance_instrumentation) == 368);
    assert!(mem::offset_of!(c::R4XStartR4Dev, performance_lock) == 376);
    assert!(mem::offset_of!(c::R4XStartR4Dev, performance_stack) == 384);
};
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
        1 => configure_sys(),
        2 => configure_desk(),
        3 => configure_draw(),
        4 => configure_net(),
        5 => configure_audio(),
        6 => configure_dev(),
        _ => unreachable!(),
    }
}
pub(super) fn interface(group: u32) -> usize {
    match group {
        g if g == c::R4LGroup::r4sys.0 => SYS.get().addr(),
        g if g == c::R4LGroup::r4desk.0 => DESK.get().addr(),
        g if g == c::R4LGroup::r4draw.0 => DRAW.get().addr(),
        g if g == c::R4LGroup::r4net.0 => NET.get().addr(),
        g if g == c::R4LGroup::r4audio.0 => AUDIO.get().addr(),
        g if g == c::R4LGroup::r4dev.0 => DEV.get().addr(),
        _ => 0,
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
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_runtime_group(group: u32) -> usize {
    interface(group)
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
