//! Boot ordering stays in the entry owner. Each not-yet-migrated subsystem
//! is reached through a bounded private C call, never through Zig slices.
use crate::build_bridge::config;

unsafe extern "C" {
    fn r4os_zig_boot_action(action: u32) -> bool;
    fn r4os_zig_boot_step(phase: u8, progress: u8, text: *const u8, len: usize);
    fn r4os_zig_boot_status(text: *const u8, len: usize);
    fn r4os_zig_boot_require(ok: bool, phase: u8, text: *const u8, len: usize);
    fn r4os_zig_runtime_start() -> !;
}

fn action(id: u32) -> bool {
    // SAFETY: the BSP owns this linear boot sequence; the private bridge
    // dispatches only the exact named subsystem cuts described in main.zig.
    unsafe { r4os_zig_boot_action(id) }
}
fn step(phase: u8, progress: u8, text: &[u8]) {
    unsafe { r4os_zig_boot_step(phase, progress, text.as_ptr(), text.len()) }
}
fn status(text: &[u8]) {
    unsafe { r4os_zig_boot_status(text.as_ptr(), text.len()) }
}
fn require(ok: bool, phase: u8, text: &[u8]) {
    unsafe { r4os_zig_boot_require(ok, phase, text.as_ptr(), text.len()) }
}
fn require_test(ok: bool, text: &[u8]) {
    if !ok {
        super::fatal::kernel(13, text);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn kmain() -> ! {
    action(0); // version metadata
    unsafe { super::limine::keep_requests() };
    action(1); // crash/fatal/COM1, before memory or task initialization
    action(2); // resident BootInfo, boot display, non-visible log routing
    step(1, 4, b"Startbild");
    action(3); // exact Rust/Zig build configuration and early fault probes
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
    action(27); // bounded SMP/IRQ/TCP witnesses only if explicitly requested
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
    unsafe { r4os_zig_runtime_start() }
}
