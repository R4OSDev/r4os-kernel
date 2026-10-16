//! Stable platform API tables and program runtime initialization. Canonical
//! Contract defaults supply every header and reserved slot. Typed readonly
//! callback projections have no backing tables or program state in Zig.
use super::{async_io, reaper, registry_state};
use crate::{contract as c, sched::task::task_api};
use core::{
    cell::UnsafeCell,
    mem::{self, MaybeUninit},
    ptr,
    sync::atomic::{AtomicBool, AtomicU8, Ordering},
};
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
unsafe extern "C" {
    fn r4os_zig_program_runtime_bindings(group: u8, out: *mut usize, count: usize);
    fn r4os_zig_program_runtime_register(op: u8, value: u64);
}
static SYS: Table<c::R4XStartR4Sys> = Table::new();
fn configure_sys() {
    let mut bindings = [0usize; 153];
    unsafe { r4os_zig_program_runtime_bindings(1, bindings.as_mut_ptr(), bindings.len()) };
    let value = c::R4XStartR4Sys {
        write: bindings[0],
        putc: bindings[1],
        sleep_ticks: bindings[2],
        ticks: bindings[3],
        env_get: bindings[4],
        dir_entry: bindings[5],
        program_should_close: bindings[6],
        program_class: bindings[7],
        program_instance: bindings[8],
        service_status: bindings[9],
        service_open: bindings[10],
        service_close: bindings[11],
        service_call: bindings[12],
        service_endpoint_register: bindings[13],
        service_endpoint_unregister: bindings[14],
        service_endpoint_poll: bindings[15],
        service_endpoint_recv: bindings[16],
        service_endpoint_reply: bindings[17],
        service_detail_by_name: bindings[18],
        service_start: bindings[19],
        service_stop: bindings[20],
        service_restart: bindings[21],
        service_install: bindings[22],
        service_remove: bindings[23],
        vm_reserve: bindings[24],
        vm_commit: bindings[25],
        vm_decommit: bindings[26],
        vm_release: bindings[27],
        vm_query: bindings[28],
        thread_create: bindings[29],
        thread_exit: bindings[30],
        thread_join: bindings[31],
        thread_current: bindings[32],
        thread_status: bindings[33],
        io_file_read: bindings[34],
        io_file_read_at: bindings[35],
        io_file_write: bindings[36],
        io_file_append: bindings[37],
        io_file_stream_begin: bindings[38],
        io_file_stream_write: bindings[39],
        io_file_stream_finish: bindings[40],
        io_file_stream_abort: bindings[41],
        io_service_call: bindings[42],
        io_status: bindings[43],
        io_wait: bindings[44],
        io_close: bindings[45],
        task_yield: bindings[46],
        system_halt: bindings[47],
        system_reboot: bindings[48],
        system_poweroff: bindings[49],
        time_state: bindings[50],
        time_set_state: bindings[51],
        time_seconds_since_midnight: bindings[52],
        dir_list: bindings[53],
        dir_create: bindings[54],
        dir_delete: bindings[55],
        drive_info: bindings[56],
        file_info: bindings[57],
        file_delete: bindings[58],
        file_rename: bindings[59],
        file_copy: bindings[60],
        file_move: bindings[61],
        file_read: bindings[62],
        file_write: bindings[63],
        file_read_at: bindings[64],
        file_append: bindings[65],
        file_stream_begin: bindings[66],
        file_stream_write: bindings[67],
        file_stream_finish: bindings[68],
        file_stream_abort: bindings[69],
        env_set: bindings[70],
        registry_key_info: bindings[71],
        registry_enum_key: bindings[72],
        registry_enum_value: bindings[73],
        registry_get_value: bindings[74],
        registry_set_value: bindings[75],
        registry_delete_value: bindings[76],
        service_info: bindings[77],
        service_detail: bindings[78],
        service_set_start_mode: bindings[79],
        service_endpoint_wait: bindings[80],
        program_run: bindings[81],
        program_launch: bindings[82],
        program_spawn: bindings[83],
        program_kill: bindings[84],
        program_status: bindings[85],
        program_request_close: bindings[86],
        program_reap_instance: bindings[87],
        boot_log_info: bindings[88],
        boot_log_read: bindings[89],
        program_spawn_handle: bindings[90],
        program_open_handle: bindings[91],
        program_handle_status: bindings[92],
        program_handle_request_close: bindings[93],
        program_handle_kill: bindings[94],
        program_handle_wait: bindings[95],
        program_handle_reap: bindings[96],
        program_completion_read: bindings[97],
        program_inventory_begin: bindings[98],
        program_inventory_programs: bindings[99],
        program_inventory_tasks: bindings[100],
        program_inventory_threads: bindings[101],
        thread_create_handle: bindings[102],
        thread_handle_join: bindings[103],
        thread_handle_status: bindings[104],
        file_replace_atomic: bindings[105],
        file_delete_if_match: bindings[106],
        file_update_atomic_checked: bindings[107],
        path_names_equal_collated: bindings[108],
        file_update_cleanup_checked: bindings[109],
        file_stream_declare_publish: bindings[110],
        module_resource_stat: bindings[111],
        module_resource_read: bindings[112],
        program_module_path: bindings[113],
        program_module_running: bindings[114],
        monotonic_clock: bindings[115],
        boot_ready: bindings[116],
        registry_snapshot_begin: bindings[117],
        registry_snapshot_page: bindings[118],
        registry_batch_mutate: bindings[119],
        io_file_write_at: bindings[120],
        io_file_info: bindings[121],
        io_file_lock: bindings[122],
        storage_inventory: bindings[123],
        storage_device: bindings[124],
        storage_partition: bindings[125],
        storage_volume: bindings[126],
        storage_claim_begin: bindings[127],
        storage_claim_end: bindings[128],
        storage_read: bindings[129],
        storage_claim_read: bindings[130],
        storage_claim_write: bindings[131],
        storage_claim_flush: bindings[132],
        storage_rescan: bindings[133],
        storage_mount: bindings[134],
        storage_unmount: bindings[135],
        storage_use_begin: bindings[136],
        storage_use_end: bindings[137],
        directory_change_begin: bindings[138],
        directory_change_poll: bindings[139],
        file_copy_buffered: bindings[140],
        notification_create: bindings[141],
        notification_query: bindings[142],
        notification_notify: bindings[143],
        notification_wait: bindings[144],
        notification_close: bindings[145],
        program_local_get: bindings[146],
        program_local_publish: bindings[147],
        thread_current_handle: bindings[148],
        cpu_capacity: bindings[149],
        program_exit: bindings[150],
        platform_input_snapshot: bindings[151],
        directory_next: bindings[152],
        ..Default::default()
    };
    SYS.configure(value);
}
const _: () = {
    assert!(mem::size_of::<c::R4XStartR4Sys>() == 1264);
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
    let mut bindings = [0usize; 64];
    unsafe { r4os_zig_program_runtime_bindings(2, bindings.as_mut_ptr(), bindings.len()) };
    let value = c::R4XStartR4Desk {
        read_key: bindings[0],
        mouse_state: bindings[1],
        mouse_show: bindings[2],
        mouse_hide: bindings[3],
        keyboard_layout_current: bindings[4],
        keyboard_layout_at: bindings[5],
        keyboard_layout_set: bindings[6],
        program_set_window: bindings[7],
        program_set_console_host: bindings[8],
        program_request_host_launch: bindings[9],
        program_take_host_launch: bindings[10],
        program_window_id: bindings[11],
        gui_window_info: bindings[12],
        gui_set_window_info: bindings[13],
        gui_poll_event: bindings[14],
        gui_push_event: bindings[15],
        gui_set_text: bindings[16],
        gui_text: bindings[17],
        gui_revision: bindings[18],
        gui_command: bindings[19],
        gui_set_title: bindings[20],
        gui_title: bindings[21],
        gui_set_min_size: bindings[22],
        gui_min_size: bindings[23],
        console_output: bindings[24],
        console_revision: bindings[25],
        console_state: bindings[26],
        console_set_metrics: bindings[27],
        console_push_key: bindings[28],
        console_write: bindings[29],
        console_read: bindings[30],
        clipboard_write: bindings[31],
        clipboard_read: bindings[32],
        clipboard_revision: bindings[33],
        clipboard_info: bindings[34],
        clipboard_clear: bindings[35],
        read_key_codepoint: bindings[36],
        remote_frame_info: bindings[37],
        remote_frame_read: bindings[38],
        remote_frame_wait: bindings[39],
        remote_frame_publish: bindings[40],
        remote_input_push: bindings[41],
        remote_input_poll: bindings[42],
        remote_input_status: bindings[43],
        desktop_activity_wait: bindings[44],
        remote_frame_map: bindings[45],
        program_current_console_host: bindings[46],
        program_request_desktop: bindings[47],
        program_spawn_with_console_host: bindings[48],
        program_spawn_with_console_host_handle: bindings[49],
        program_set_window_handle: bindings[50],
        console_push_input: bindings[51],
        remote_frame_acquire: bindings[52],
        remote_frame_release: bindings[53],
        remote_frame_consumers: bindings[54],
        remote_frame_publish_regions: bindings[55],
        console_input_wait: bindings[56],
        physical_key_poll: bindings[57],
        mouse_motion: bindings[58],
        remote_frame_snapshot_acquire: bindings[59],
        remote_frame_snapshot_release: bindings[60],
        remote_frame_source_reset: bindings[61],
        remote_frame_capture_stats: bindings[62],
        desktop_activity_notify: bindings[63],
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
    let mut bindings = [0usize; 112];
    unsafe { r4os_zig_program_runtime_bindings(3, bindings.as_mut_ptr(), bindings.len()) };
    let value = c::R4XStartR4Draw {
        screen_width: bindings[0],
        screen_height: bindings[1],
        clear: bindings[2],
        rect: bindings[3],
        text: bindings[4],
        display_revision: bindings[5],
        display_begin_frame: bindings[6],
        display_begin_frame_rect: bindings[7],
        display_present: bindings[8],
        display_blit_xrgb32: bindings[9],
        gui_clear: bindings[10],
        gui_rect: bindings[11],
        gui_draw_text: bindings[12],
        gui_draw_text_ex: bindings[13],
        gui_blit: bindings[14],
        gui_raster_read: bindings[15],
        gui_present: bindings[16],
        font_count: bindings[17],
        font_info: bindings[18],
        font_measure: bindings[19],
        gui_set_font: bindings[20],
        gui_font: bindings[21],
        text_font: bindings[22],
        font_reload: bindings[23],
        font_glyph_row: bindings[24],
        gui_blend_alpha8: bindings[25],
        gui_frame_begin: bindings[26],
        gui_frame_append: bindings[27],
        gui_frame_commit: bindings[28],
        gui_frame_cancel: bindings[29],
        gui_frame_info: bindings[30],
        gui_frame_read: bindings[31],
        display_blit_xrgb32_stride: bindings[32],
        display_present_regions: bindings[33],
        display_present_capabilities: bindings[34],
        display_present_completion: bindings[35],
        gui_frame_begin_damage: bindings[36],
        gui_frame_generation_info: bindings[37],
        gui_frame_generation_read: bindings[38],
        font_glyph_bitmap: bindings[39],
        font_revision: bindings[40],
        gui_frame_begin_replace: bindings[41],
        gui_frame_stream_info: bindings[42],
        gui_shared_raster_create: bindings[43],
        gui_shared_raster_destroy: bindings[44],
        gui_shared_raster_map_write: bindings[45],
        gui_shared_raster_publish: bindings[46],
        gui_shared_raster_acquire: bindings[47],
        gui_shared_raster_release: bindings[48],
        gfx_buffer_create: bindings[49],
        gfx_buffer_describe: bindings[50],
        gfx_buffer_import: bindings[51],
        gfx_buffer_release: bindings[52],
        gfx_buffer_map: bindings[53],
        gfx_buffer_unmap: bindings[54],
        gfx_buffer_export_raster: bindings[55],
        gfx_buffer_stats: bindings[56],
        gfx_queue_open: bindings[57],
        gfx_queue_close: bindings[58],
        gfx_queue_submit: bindings[59],
        gfx_fence_query: bindings[60],
        gfx_fence_wait: bindings[61],
        gfx_fence_cancel: bindings[62],
        gfx_fence_release: bindings[63],
        gfx_queue_backend: bindings[64],
        gfx_output_revision: bindings[65],
        gfx_output_info: bindings[66],
        gfx_output_mode: bindings[67],
        gfx_output_edid: bindings[68],
        gfx_atomic_test: bindings[69],
        gfx_atomic_commit: bindings[70],
        gfx_atomic_submit: bindings[71],
        gfx_atomic_status: bindings[72],
        gfx_atomic_resolve: bindings[73],
        display_presentation_stats: bindings[74],
        display_cursor_info: bindings[75],
        display_cursor_submit: bindings[76],
        display_cursor_status: bindings[77],
        gfx_queue_backend_info: bindings[78],
        gfx_native_start: bindings[79],
        gfx_native_query: bindings[80],
        gfx_native_receive: bindings[81],
        gfx_native_close: bindings[82],
        gfx_native_wait: bindings[83],
        gfx_queue_submit_render_list: bindings[84],
        display_presentation_info: bindings[85],
        display_presentation_feedback: bindings[86],
        display_output_target: bindings[87],
        gfx_queue_submit_output: bindings[88],
        display_output_presentation_info: bindings[89],
        display_output_presentation_feedback: bindings[90],
        gfx_queue_submit_render_grid_list: bindings[91],
        gfx_output_color: bindings[92],
        gfx_queue_submit_render_color_list: bindings[93],
        gfx_atomic_test_color: bindings[94],
        gfx_atomic_submit_color: bindings[95],
        gfx_output_refresh: bindings[96],
        gfx_refresh_request: bindings[97],
        gfx_memory_budget: bindings[98],
        gfx_telemetry: bindings[99],
        gfx_output_power: bindings[100],
        gfx_power_request: bindings[101],
        gfx_virtual_start: bindings[102],
        gfx_virtual_query: bindings[103],
        gfx_virtual_close: bindings[104],
        gfx_virtual_wait: bindings[105],
        gfx_buffer_map_persistent: bindings[106],
        gfx_queue_backend_properties: bindings[107],
        gfx_queue_submit_native: bindings[108],
        gfx_queue_submit_render_color_grid_list: bindings[109],
        gfx_output_brightness: bindings[110],
        gfx_brightness_request: bindings[111],
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
    let mut bindings = [0usize; 35];
    unsafe { r4os_zig_program_runtime_bindings(4, bindings.as_mut_ptr(), bindings.len()) };
    let value = c::R4XStartR4Net {
        tcp_connect: bindings[0],
        tcp_write: bindings[1],
        tcp_read: bindings[2],
        tcp_close: bindings[3],
        tcp_summary: bindings[4],
        tcp_connection: bindings[5],
        tcp_echo_listen_once: bindings[6],
        tcp_accept_read_once: bindings[7],
        net_ipv4_send: bindings[8],
        net_ipv4_recv: bindings[9],
        net_config_get: bindings[10],
        net_config_set: bindings[11],
        net_dns_resolve: bindings[12],
        net_dns_resolve_server: bindings[13],
        net_dhcp_acquire: bindings[14],
        net_dhcp_renew: bindings[15],
        net_dhcp_release: bindings[16],
        net_dhcp_status: bindings[17],
        net_detail_get: bindings[18],
        net_diag_run: bindings[19],
        ipc_open: bindings[20],
        ipc_send: bindings[21],
        ipc_recv: bindings[22],
        ipc_poll: bindings[23],
        ipc_close: bindings[24],
        ipc_summary: bindings[25],
        ipc_channel: bindings[26],
        serial_link_status: bindings[27],
        serial_link_poll: bindings[28],
        serial_link_send_message: bindings[29],
        serial_link_host_test: bindings[30],
        serial_link_inbox: bindings[31],
        net_service_request: bindings[32],
        ipc_performance: bindings[33],
        tcp_performance: bindings[34],
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
    let mut bindings = [0usize; 21];
    unsafe { r4os_zig_program_runtime_bindings(5, bindings.as_mut_ptr(), bindings.len()) };
    let value = c::R4XStartR4Audio {
        audio_open_stream: bindings[0],
        audio_write: bindings[1],
        audio_close: bindings[2],
        audio_set_volume: bindings[3],
        sid_acquire: bindings[4],
        sid_write_register: bindings[5],
        sid_release: bindings[6],
        sid_load_data: bindings[7],
        sid_init: bindings[8],
        sid_play_frame: bindings[9],
        sid_stop: bindings[10],
        sid_model_name: bindings[11],
        midi_open_synth: bindings[12],
        midi_send: bindings[13],
        midi_close: bindings[14],
        opl3_write_register: bindings[15],
        opl3_reset: bindings[16],
        opl3_render_block: bindings[17],
        opl3_stop: bindings[18],
        audio_output_info: bindings[19],
        audio_select_output: bindings[20],
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
    r4dev::providers::install();
    let value = c::R4XStartR4Dev {
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
        ..Default::default()
    };
    DEV.configure(value);
}

const _: () = {
    assert!(mem::size_of::<c::R4XStartR4Dev>() == 392);
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
            for op in 0..7 {
                unsafe { r4os_zig_program_runtime_register(op, 0) }
            }
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
    unsafe {
        r4os_zig_program_runtime_register(10, usable);
        r4os_zig_program_runtime_register(11, 0);
        r4os_zig_program_runtime_register(12, 0);
    }
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
