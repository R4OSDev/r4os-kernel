//! Linear BSP boot sequence. Every subsystem call binds its native owner;
//! external modules retain the unchanged public C tables.
use crate::build_config::config;

fn action(id: u32) -> bool {
    // SAFETY: the BSP owns the complete linear initialization sequence.
    unsafe { boot_action(id) }
}
fn step(phase: u8, progress: u8, text: &[u8]) {
    unsafe { boot_step(phase, progress, text.as_ptr(), text.len()) }
}
fn status(text: &[u8]) {
    unsafe { boot_status(text.as_ptr(), text.len()) }
}
fn require(ok: bool, phase: u8, text: &[u8]) {
    unsafe { boot_require(ok, phase, text.as_ptr(), text.len()) }
}
fn require_test(ok: bool, text: &[u8]) {
    if !ok {
        super::fatal::kernel(13, text);
    }
}

pub fn start() -> ! {
    action(0); // version metadata
    unsafe { super::limine::keep_requests() };
    action(1); // crash/fatal/COM1, before memory or task initialization
    action(2); // resident BootInfo, boot display, non-visible log routing
    step(1, 4, b"Startbild");
    action(3); // selected Rust configuration and explicit early fault probes
    step(2, 9, b"CPU initialisieren");
    action(4);
    step(3, 13, b"Timer initialisieren");
    action(5);
    step(4, 18, b"Basistreiber laden");
    action(6);
    step(5, 23, b"Eingabe initialisieren");
    action(7);
    step(1, 28, b"Startinfo laden");
    require(action(8), 1, b"Boot intro failed");
    step(6, 39, b"Speicher initialisieren");
    require(action(9), 6, b"Memory boot failed");
    action(10);
    step(7, 49, b"Laufwerke vorbereiten");
    require(action(11), 7, b"Storage foundation failed");
    step(8, 54, b"Module laden");
    action(12);
    step(9, 61, b"Plattform erfassen");
    require(action(13), 9, b"Platform boot failed");
    step(16, 65, b"USB-Protokolle laden");
    require(action(14), 16, b"USB protocol preload failed");
    step(12, 69, b"Dienste vorbereiten");
    action(15);
    action(16); // validate PCIe status while fatal phase is still platform
    step(7, 76, b"Controller starten");
    require(action(17), 7, b"Storage controller boot failed");
    step(10, 83, b"System laden");
    // Durable recovery must finish before library/config/font caches exist.
    require(
        crate::update::boot::recover_before_runtime(),
        10,
        b"System update recovery failed",
    );
    require(
        crate::update::upload_boot::recover_before_runtime(),
        10,
        b"Upload publish claim recovery failed",
    );
    require(action(20), 10, b"Loader boot failed");
    step(11, 87, b"Interrupts starten");
    require(action(21), 11, b"Platform IRQ boot failed");
    step(14, 90, b"Audio vorbereiten");
    require(action(22), 14, b"Audio boot failed");
    step(15, 93, b"Netzwerk vorbereiten");
    require(action(23), 15, b"Network boot failed");
    step(16, 96, b"USB-Eingabe starten");
    require(action(24), 16, b"USB-HID boot failed");
    step(19, 97, b"Task-Runtime starten");
    require(action(25), 19, b"Task runtime boot failed");
    step(17, 98, b"Treiberplan erstellen");
    require(action(26), 17, b"Driver policy boot failed");
    step(13, 99, b"Runtime vorbereiten");
    status(b"SMP pruefen");
    action(27); // bounded SMP/IRQ witnesses only if explicitly requested
    status(b"Netzwerk-Task starten");
    action(28);
    status(b"DHCP starten");
    action(29);
    if config::ENABLE_BOOT_SELFTESTS {
        require_test(action(30), b"Boot sync selftest failed");
    }
    if config::ENABLE_BOOT_SELFTESTS || config::ENABLE_BLOCK_DISPATCH_SELFTEST {
        require_test(action(31), b"Block dispatch selftest failed");
    }
    status(b"USB-Polling starten");
    action(32);
    if config::ENABLE_BOOT_SELFTESTS {
        require_test(action(33), b"Boot scheduler priority selftest failed");
    }
    action(34); // unchanged BOOTSELFTEST ON/OFF diagnostic
    status(b"Task-Registry pruefen");
    action(35);
    if config::ENABLE_STACK_GUARD_TEST {
        action(36);
    }
    status(b"Runtime starten");
    unsafe { runtime_start() }
}

unsafe fn runtime_start() -> ! {
    unsafe { crate::boot::runtime::r4os_rust_runtime_boot_start() }
}

unsafe fn boot_step(phase: u8, progress: u8, text: *const u8, len: usize) {
    super::fatal::r4os_rust_fatal_phase(phase);
    crate::display::bootscreen::r4os_rust_boot_screen_phase(progress);
    unsafe {
        crate::display::bootscreen::set_status(text, len);
    }
}

unsafe fn boot_status(text: *const u8, len: usize) {
    unsafe {
        crate::display::bootscreen::set_status(text, len);
    }
}

unsafe fn boot_require(ok: bool, phase: u8, text: *const u8, len: usize) {
    if !ok {
        unsafe {
            super::fatal::r4os_rust_fatal_pending(phase, text, len);
        }
    }
    super::fatal::r4os_rust_fatal_phase(phase);
}

unsafe fn boot_action(action: u32) -> bool {
    use crate::{
        arch, display, driver, input, loader, memory, net, platform, sched, service, storage, usb,
    };
    match action {
        0 => super::version::keep(),
        1 => {
            super::crash::r4os_rust_crash_reset();
            super::fatal::r4os_rust_fatal_init();
            super::fatal::r4os_rust_fatal_phase(1);
            super::foundation::r4os_rust_com_debug_boot();
        }
        2 => {
            {
                super::info::r4os_rust_boot_info_init();
            }
            display::boot_display::init();
            super::foundation::r4os_rust_boot_redirect();
        }
        3 => {
            crate::build_config::r4os_rust_boot_probe();
            if config::ENABLE_KERNEL_FATAL_TEST {
                super::fatal::kernel(1, b"Kernel fatal crash test");
            }
            if config::ENABLE_CRASH_SCREEN_TEST {
                super::probes::crash_renderer();
            }
        }
        4 => super::foundation::r4os_rust_cpu_boot(),
        5 => {
            platform::timer_boot::r4os_rust_timer_boot_control(0);
        }
        6 => {
            driver::registry::r4os_rust_driver_boot(0);
        }
        7 => {
            input::boot::r4os_rust_input_boot_keyboard();
            input::boot::r4os_rust_input_boot_mouse();
            input::boot::r4os_rust_input_boot_complete();
        }
        8 => return super::foundation::r4os_rust_boot_intro(),
        9 => return unsafe { memory::boot::r4os_rust_memory_core() },
        10 => memory::boot_diagnostics::blocks_summary(),
        11 => return unsafe { storage::boot::r4os_rust_storage_boot(0, core::ptr::null()) != 0 },
        12 => unsafe {
            loader::boot::r4os_rust_module_boot_init();
        },
        13 => return platform::boot::init(),
        14 => return unsafe { usb::preload::r4os_rust_usb_preload_init() },
        15 => unsafe {
            service::boot::r4os_rust_service_boot_init();
        },
        16 => {
            super::fatal::r4os_rust_fatal_phase(9);
            let mut pci = core::mem::MaybeUninit::uninit();
            if !unsafe { platform::boot::r4os_rust_platform_boot_pci(pci.as_mut_ptr()) } {
                super::fatal::kernel(9, b"PCIe status missing after platform boot");
            }
        }
        17 => {
            let mut pci = core::mem::MaybeUninit::uninit();
            let ready = unsafe { platform::boot::r4os_rust_platform_boot_pci(pci.as_mut_ptr()) };
            assert!(ready, "Platform PCI status retained after validation");
            let pci = unsafe { pci.assume_init() };
            let counts = storage::boot::Counts {
                xhci: pci.xhci_count,
                ahci: pci.ahci_count,
                nvme: pci.nvme_count,
            };
            return unsafe { storage::boot::r4os_rust_storage_boot(2, &counts) != 0 };
        }
        20 => return unsafe { loader::boot::r4os_rust_loader_boot_init() },
        21 => return platform::irq_boot::init(),
        22 => {
            return unsafe {
                crate::audio::boot::r4os_rust_audio_boot(0, 0, core::ptr::null_mut())
            };
        }
        23 => return unsafe { net::boot::r4os_rust_network_boot_init() },
        24 => return unsafe { usb::hid_boot::r4os_rust_usb_hid_boot_init() },
        25 => return unsafe { super::runtime::r4os_rust_runtime_boot_init() },
        26 => return driver::driver_policy::r4os_rust_driver_policy_init(),
        27 => {
            let quick = super::probes::smp_enabled();
            if quick {
                let mut memory = core::mem::MaybeUninit::uninit();
                unsafe {
                    memory::boot::r4os_rust_memory_snapshot(memory.as_mut_ptr());
                }
                let memory = unsafe { memory.assume_init() };
                arch::smp_probe::run(memory.usable);
                super::probes::nvme_interrupt();
            }
        }
        28 => unsafe {
            net::rx_runtime::r4os_rust_net_rx_runtime_start(&net::bindings::RX);
        },
        29 => {
            net::dhcp_client::r4os_rust_dhcp_client_start();
        }
        30 => {
            return !config::ENABLE_BOOT_SELFTESTS
                || sched::sync_selftest::r4os_rust_sync_selftest();
        }
        31 => {
            return !(config::ENABLE_BOOT_SELFTESTS || config::ENABLE_BLOCK_DISPATCH_SELFTEST)
                || storage::block_probes::parallel_dispatch();
        }
        32 => unsafe {
            usb::hid_boot::r4os_rust_usb_hid_boot_start();
        },
        33 => {
            return !config::ENABLE_BOOT_SELFTESTS
                || sched::scheduler::r4os_rust_scheduler_control(16, 0, 0) != 0;
        }
        34 => super::log_route::puts(if config::ENABLE_BOOT_SELFTESTS {
            b"BOOTSELFTEST OK heap=1 page_tables=1 sync=1 priority=1\r\n"
        } else {
            b"BOOTSELFTEST OFF heap=0 page_tables=0 sync=0 priority=0\r\n"
        }),
        35 => {
            sched::task_probe::r4os_rust_task_registry_selftest();
        }
        36 => {
            if config::ENABLE_STACK_GUARD_TEST {
                super::probes::stack_guard_task();
            }
        }
        _ => super::fatal::kernel(1, b"Invalid private boot cut"),
    }
    true
}
